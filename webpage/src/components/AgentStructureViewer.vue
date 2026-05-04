<script setup>
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { renderPdbeMolstar, applySelectionWithRetry, molstarFormatFromPskitFormat } from "../utils/pdbeMolstar.js";
import { useMolstar, MOLSTAR_COLORS } from "../composables/useMolstar.js";

const props = defineProps({
    tool: {
        type: Object,
        required: true,
    },
    sessionFiles: {
        type: Array,
        default: () => [],
    },
});

const supportedTools = new Set(["predict_binding_sites", "annotate_binding_pairs", "annotate_binding_sites"]);
const { viewerContainer, initViewer, revokeViewerObjectUrl, setViewerLastObjectUrl, destroyViewer } = useMolstar();

const isOpen = ref(true);
const isLoading = ref(false);
const errorMessage = ref("");
const parsedRows = ref([]);
const pairCount = ref(0);
const structureFile = ref(null);
const csvFile = ref(null);
let activeRenderKey = "";
let renderedKey = "";
let renderRunId = 0;

const toolName = computed(() => String(props.tool?.name || ""));
const isSupported = computed(() => supportedTools.has(toolName.value));
const normalizedToolName = computed(() => toolName.value === "annotate_binding_sites" ? "annotate_binding_pairs" : toolName.value);
const isPrediction = computed(() => normalizedToolName.value === "predict_binding_sites");
const isAnnotation = computed(() => normalizedToolName.value === "annotate_binding_pairs");

const resultCsv = computed(() => {
    const files = Array.isArray(props.tool?.files) ? props.tool.files : [];
    const suffix = isPrediction.value ? "_binding_sites.csv" : "_binding_pairs.csv";
    return files.find((file) => String(file?.filename || file?.path || "").endsWith(suffix)) || null;
});

const canOfferPreview = computed(() => isSupported.value && !!resultCsv.value);

const previewTitle = computed(() => isPrediction.value ? "Binding-site score preview" : "Binding-pair preview");
const previewSubtitle = computed(() => {
    if (!parsedRows.value.length) return "Load structure preview";
    if (isPrediction.value) {
        const high = parsedRows.value.filter((row) => row.score >= 0.4).length;
        return `${parsedRows.value.length} residues scored · ${high} above 0.4`;
    }
    return `${pairCount.value} binding pairs · ${parsedRows.value.length} residues marked`;
});

const colorLegend = computed(() => {
    if (isPrediction.value) {
        return {
            type: "gradient",
            low: "Low score",
            high: "High score",
        };
    }
    return {
        type: "pairs",
        protein: "Binding AA",
        nucleic: "Binding NT",
    };
});

function parseToolArgs() {
    const args = props.tool?.args;
    if (!args) return {};
    if (typeof args === "object") return args;
    try {
        return JSON.parse(args);
    } catch {
        return {};
    }
}

function normalizePath(value) {
    return String(value || "").replaceAll("\\", "/");
}

function basename(path) {
    return normalizePath(path).split("/").filter(Boolean).pop() || "";
}

function findStructureFile() {
    const args = parseToolArgs();
    const requestedPath = normalizePath(args.pdb_path || "");
    const requestedName = basename(requestedPath);
    const allFiles = [
        ...(Array.isArray(props.sessionFiles) ? props.sessionFiles : []),
        ...(Array.isArray(props.tool?.files) ? props.tool.files : []),
    ];
    const structureFiles = allFiles.filter((file) => /\.(pdb|cif)$/i.test(String(file?.filename || file?.path || "")));

    if (!requestedPath && structureFiles.length === 0) return null;

    const exactByAbsoluteSuffix = structureFiles.find((file) => {
        const rel = normalizePath(file?.path || "");
        return requestedPath.endsWith(rel);
    });
    if (exactByAbsoluteSuffix) return exactByAbsoluteSuffix;

    const exactByFilename = structureFiles.find((file) => String(file?.filename || "") === requestedName);
    if (exactByFilename) return exactByFilename;

    const baseWithoutExt = requestedName.replace(/\.(pdb|cif)$/i, "");
    return structureFiles.find((file) => String(file?.filename || "").replace(/\.(pdb|cif)$/i, "") === baseWithoutExt) || null;
}

function parseCsv(text) {
    const lines = String(text || "").trim().split(/\r?\n/).filter(Boolean);
    if (lines.length < 2) return [];
    const header = lines[0].split(",").map((item) => item.trim().toLowerCase());
    return lines.slice(1).map((line) => {
        const values = line.split(",").map((item) => item.trim());
        const row = {};
        header.forEach((key, index) => {
            row[key] = values[index] || "";
        });
        return row;
    });
}

function clamp01(value) {
    if (!Number.isFinite(value)) return 0;
    return Math.max(0, Math.min(1, value));
}

function mix(a, b, t) {
    return Math.round(a + (b - a) * clamp01(t));
}

function interpolateColor(stops, value) {
    const t = clamp01(value);
    if (t <= 0.5) {
        const p = t / 0.5;
        return {
            r: mix(stops[0].r, stops[1].r, p),
            g: mix(stops[0].g, stops[1].g, p),
            b: mix(stops[0].b, stops[1].b, p),
        };
    }

    const p = (t - 0.5) / 0.5;
    return {
        r: mix(stops[1].r, stops[2].r, p),
        g: mix(stops[1].g, stops[2].g, p),
        b: mix(stops[1].b, stops[2].b, p),
    };
}

const gradientStops = [
    { r: 37, g: 99, b: 235 },
    { r: 245, g: 158, b: 11 },
    { r: 220, g: 38, b: 38 },
];

function parsePredictionRows(rows) {
    return rows.map((row, index) => {
        const score = clamp01(Number.parseFloat(row.score || row.probability || row.prob || "0"));
        return {
            chain: row.chain || row.chain_id || "-",
            resNum: Number.parseInt(row.residue_number || row.resi || row.resnum || `${index + 1}`, 10),
            resName: row.residue_name || row.resn || row.aa || "-",
            score,
            colorValue: score,
        };
    }).filter((row) => row.chain && Number.isFinite(row.resNum));
}

function parseResidueToken(token) {
    const parts = String(token || "").split("-");
    return {
        chain: parts[0] || "",
        resNum: Number.parseInt(parts[1] || "", 10),
        resName: parts.slice(2).join("-") || "-",
    };
}

function parsePairRows(rows) {
    const residueByKey = new Map();
    let validPairCount = 0;
    for (const row of rows) {
        const distance = Number.parseFloat(row.distance || "");
        if (!Number.isFinite(distance)) continue;
        const [protRaw, naRaw] = String(row.pair || "").split("_");
        const prot = parseResidueToken(protRaw);
        const na = parseResidueToken(naRaw);
        if (prot.chain && Number.isFinite(prot.resNum) && na.chain && Number.isFinite(na.resNum)) {
            validPairCount += 1;
        }

        for (const residue of [{ ...prot, role: "protein" }, { ...na, role: "nucleic" }]) {
            if (!residue.chain || !Number.isFinite(residue.resNum)) continue;
            const key = `${residue.chain}:${residue.resNum}`;
            const existing = residueByKey.get(key);
            if (!existing || distance < existing.distance) {
                residueByKey.set(key, { ...residue, distance });
            }
        }
    }

    pairCount.value = validPairCount;
    return Array.from(residueByKey.values());
}

function selectionColorForRow(row) {
    if (isAnnotation.value) {
        return row.role === "nucleic" ? MOLSTAR_COLORS.nucleic : MOLSTAR_COLORS.protein;
    }
    return interpolateColor(gradientStops, row.colorValue);
}

async function loadCsvRows(file) {
    const response = await fetch(file.download_url);
    if (!response.ok) throw new Error("Failed to load result CSV.");
    const text = await response.text();
    const rows = parseCsv(text);
    pairCount.value = 0;
    return isPrediction.value ? parsePredictionRows(rows) : parsePairRows(rows);
}

function buildSelections(rows) {
    return rows.map((row) => ({
        auth_asym_id: row.chain,
        auth_residue_number: row.resNum,
        color: selectionColorForRow(row),
    }));
}

async function renderPreview() {
    if (!isOpen.value || !canOfferPreview.value) return;

    const nextCsvFile = resultCsv.value;
    const nextStructureFile = findStructureFile();
    const nextRenderKey = [
        normalizedToolName.value,
        nextCsvFile?.download_url || "",
        nextStructureFile?.download_url || "",
    ].join("|");

    if (nextRenderKey === renderedKey && parsedRows.value.length > 0 && !errorMessage.value) {
        return;
    }
    if (nextRenderKey === activeRenderKey && isLoading.value) {
        return;
    }

    const runId = ++renderRunId;
    activeRenderKey = nextRenderKey;
    isLoading.value = true;
    errorMessage.value = "";
    try {
        csvFile.value = nextCsvFile;
        const rows = await loadCsvRows(csvFile.value);
        if (runId !== renderRunId) return;
        parsedRows.value = rows;
        if (rows.length === 0) {
            throw new Error("No residues were found in the result CSV.");
        }

        structureFile.value = nextStructureFile;
        if (!structureFile.value?.download_url) {
            throw new Error("Structure file unavailable for this tool result.");
        }

        await nextTick();
        const viewer = await initViewer();
        if (runId !== renderRunId) return;
        revokeViewerObjectUrl();
        const structureResponse = await fetch(structureFile.value.download_url);
        if (!structureResponse.ok) throw new Error("Failed to load structure file.");
        const blob = await structureResponse.blob();
        if (runId !== renderRunId) return;
        const url = URL.createObjectURL(blob);
        setViewerLastObjectUrl(url);

        const filename = String(structureFile.value.filename || structureFile.value.path || "");
        await renderPdbeMolstar(viewer, viewerContainer.value, {
            customData: {
                url,
                format: molstarFormatFromPskitFormat(filename.endsWith(".cif") ? "cif" : "pdb"),
                binary: false,
            },
        });
        if (runId !== renderRunId) return;

        await applySelectionWithRetry(viewer, {
            data: buildSelections(rows),
            nonSelectedColor: MOLSTAR_COLORS.nonSelected,
            focus: false,
            keepRepresentations: true,
        });
        if (runId === renderRunId) {
            renderedKey = nextRenderKey;
        }
    } catch (error) {
        if (runId === renderRunId) {
            errorMessage.value = error?.message || String(error);
            renderedKey = "";
            await destroyViewer();
        }
    } finally {
        if (runId === renderRunId) {
            activeRenderKey = "";
            isLoading.value = false;
        }
    }
}

function togglePreview() {
    isOpen.value = !isOpen.value;
}

watch(
    () => isOpen.value,
    async (open) => {
        if (open) await renderPreview();
        else await destroyViewer();
    },
);

watch(
    () => [props.tool?.id, props.tool?.files?.length, props.sessionFiles?.length],
    async (nextValue, previousValue) => {
        const toolChanged = !previousValue || nextValue[0] !== previousValue[0] || nextValue[1] !== previousValue[1];
        if (toolChanged) {
            parsedRows.value = [];
            pairCount.value = 0;
            structureFile.value = null;
            csvFile.value = null;
            errorMessage.value = "";
            renderedKey = "";
        }
        if (isOpen.value) await renderPreview();
    },
);

watch(
    () => canOfferPreview.value,
    async (canPreview) => {
        if (canPreview && isOpen.value) await renderPreview();
    },
    { immediate: true },
);

onBeforeUnmount(() => {
    renderRunId += 1;
    void destroyViewer();
});
</script>

<template>
    <div v-if="canOfferPreview" class="border-t border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
        <button type="button"
            class="flex w-full items-center justify-between gap-3 px-3 py-2.5 text-left transition hover:bg-gray-50 dark:hover:bg-gray-800"
            @click="togglePreview">
            <span class="flex min-w-0 items-center gap-2">
                <span
                    class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-700 ring-1 ring-blue-100 dark:bg-blue-950/40 dark:text-blue-200 dark:ring-blue-900">
                    <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24">
                        <path d="M12 4.75 5.5 7.45 12 10.15 18.5 7.45 12 4.75Z" stroke="currentColor"
                            stroke-width="1.8" stroke-linejoin="round" />
                        <path d="M5.5 11.15 12 13.85 18.5 11.15" stroke="currentColor" stroke-width="1.8"
                            stroke-linecap="round" stroke-linejoin="round" />
                        <path d="M5.5 14.85 12 17.55 18.5 14.85" stroke="currentColor" stroke-width="1.8"
                            stroke-linecap="round" stroke-linejoin="round" />
                    </svg>

                </span>
                <span class="min-w-0">
                    <span class="block truncate text-xs font-semibold text-gray-800 dark:text-gray-100">Structure
                        preview</span>
                    <span class="block truncate text-xs text-gray-500 dark:text-gray-400">{{ previewSubtitle }}</span>
                </span>
            </span>
            <span class="flex shrink-0 items-center gap-2">
                <span class="hidden text-xs text-gray-500 dark:text-gray-400 sm:inline">{{ previewTitle }}</span>
                <svg class="h-4 w-4 text-gray-500 transition-transform" :class="isOpen ? 'rotate-180' : ''" fill="none"
                    stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
            </span>
        </button>

        <div v-if="isOpen" class="border-t border-gray-200 bg-gray-50/80 p-3 dark:border-gray-700 dark:bg-gray-950/35">
            <div
                class="overflow-hidden rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
                <div
                    class="flex flex-wrap items-center justify-between gap-3 border-b border-gray-200 px-3 py-2 dark:border-gray-700">
                    <div class="min-w-0">
                        <div class="truncate text-sm font-semibold text-gray-900 dark:text-gray-100">{{ previewTitle }}
                        </div>
                        <div class="truncate text-xs text-gray-500 dark:text-gray-400">
                            {{ structureFile?.filename || 'Structure file pending' }}
                        </div>
                    </div>
                    <div v-if="colorLegend.type === 'gradient'"
                        class="flex items-center gap-2 text-xs text-gray-600 dark:text-gray-300">
                        <span>{{ colorLegend.low }}</span>
                        <span class="h-2 w-24 rounded-full agent-structure-gradient"></span>
                        <span>{{ colorLegend.high }}</span>
                    </div>
                    <div v-else class="flex flex-wrap items-center gap-3 text-xs text-gray-600 dark:text-gray-300">
                        <span class="flex items-center gap-1.5">
                            <span class="h-2.5 w-2.5 rounded-sm bg-[rgb(52,152,219)]"></span>
                            {{ colorLegend.protein }}
                        </span>
                        <span class="flex items-center gap-1.5">
                            <span class="h-2.5 w-2.5 rounded-sm bg-[rgb(231,76,60)]"></span>
                            {{ colorLegend.nucleic }}
                        </span>
                    </div>
                </div>

                <div class="relative h-[280px] bg-gray-100 dark:bg-gray-950 sm:h-[360px]">
                    <div ref="viewerContainer" class="h-full w-full"></div>
                    <div v-if="isLoading"
                        class="absolute inset-0 flex items-center justify-center bg-white/80 backdrop-blur-sm dark:bg-gray-950/80">
                        <div
                            class="flex items-center gap-2 rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 shadow-sm dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200">
                            <svg class="h-4 w-4 animate-spin text-blue-600" fill="none" viewBox="0 0 24 24">
                                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor"
                                    stroke-width="4"></circle>
                                <path class="opacity-75" fill="currentColor"
                                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z">
                                </path>
                            </svg>
                            Loading structure
                        </div>
                    </div>
                    <div v-if="errorMessage && !isLoading"
                        class="absolute inset-0 flex items-center justify-center bg-white/90 p-4 text-center dark:bg-gray-950/90">
                        <div
                            class="max-w-md rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200">
                            {{ errorMessage }}
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped>
.agent-structure-gradient {
    background: linear-gradient(90deg, rgb(37, 99, 235), rgb(245, 158, 11), rgb(220, 38, 38));
}
</style>
