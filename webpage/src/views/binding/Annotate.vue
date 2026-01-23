<script setup>
import { ref, computed, watch, nextTick, onBeforeUnmount } from "vue";
import { useRoute, useRouter } from "vue-router";
import JSZip from "jszip";
import InputStructure from "../../components/InputStructure.vue";
import { parsePdbIds, isValidPdbId, prepareInputsFromFiles, prepareInputsFromPdbIds, runBatch, annotateBindingPairsInWorker, sanitizeKey } from "../../utils/wasmBatch.js";
import { ensurePdbeMolstarLoaded, createPdbeMolstarViewer, renderPdbeMolstar, applySelectionWithRetry, highlightResidues, molstarFormatFromPskitFormat, createBlobUrlFromBytes } from "../../utils/pdbeMolstar.js";
import Loading from "../../components/Loading.vue";

const route = useRoute();
const router = useRouter();

const input_method = ref("file");
const ids = ref("");
const files = ref([]);

const cutoff = ref("");

const processing = ref(false);
const error_message = ref("");
const results = ref([]);
const file_errors = ref([]);
const progress = ref({ current: 0, total: 0, current_file: "" });

const current_index = ref(0);
const last_run_input_method = ref("file");
const selected_row_index = ref(-1);

const parsed_ids = computed(() => parsePdbIds(ids.value));
const ids_valid = computed(() => {
    if (parsed_ids.value.length === 0) return false;
    return parsed_ids.value.every(isValidPdbId);
});

const viewerContainer = ref(null);
let viewerInstance = null;
let viewerLastObjectUrl = "";
let viewerStructureKey = "";
let idStructureCache = new Map();

function revokeViewerObjectUrl() {
    if (viewerLastObjectUrl) {
        try {
            URL.revokeObjectURL(viewerLastObjectUrl);
        } catch {
            // ignore
        }
        viewerLastObjectUrl = "";
    }
}

function normalizePdbId(id) {
    const s = String(id || "")
        .trim()
        .toLowerCase();
    return s.length === 4 ? s : s;
}

function bindingSiteParamsFromRows(rows) {
    const protParams = [];
    const naParams = [];
    const seenProt = new Set();
    const seenNa = new Set();

    for (const r of rows || []) {
        const protChain = String(r?.prot_chain ?? "").trim();
        const protResiNum = Number.parseInt(String(r?.prot_resi ?? ""), 10);
        if (protChain && Number.isFinite(protResiNum)) {
            const key = `${protChain}:${protResiNum}`;
            if (!seenProt.has(key)) {
                seenProt.add(key);
                // PDBeMolstar QueryHelper supports auth_asym_id + auth_residue_number
                protParams.push({ auth_asym_id: protChain, auth_residue_number: protResiNum });
            }
        }

        const naChain = String(r?.na_chain ?? "").trim();
        const naResiNum = Number.parseInt(String(r?.na_resi ?? ""), 10);
        if (naChain && Number.isFinite(naResiNum)) {
            const key = `${naChain}:${naResiNum}`;
            if (!seenNa.has(key)) {
                seenNa.add(key);
                naParams.push({ auth_asym_id: naChain, auth_residue_number: naResiNum });
            }
        }
    }

    return { protParams, naParams };
}

const MOLSTAR_COLORS = {
    nonSelected: { r: 190, g: 190, b: 190 },
    protein: { r: 52, g: 152, b: 219 },
    nucleic: { r: 231, g: 76, b: 60 },
    focus: { r: 255, g: 235, b: 59 },
};

function buildColoredSelectionsFromRows(rows) {
    const { protParams, naParams } = bindingSiteParamsFromRows(rows);
    return [...protParams.map((p) => ({ ...p, color: MOLSTAR_COLORS.protein })), ...naParams.map((p) => ({ ...p, color: MOLSTAR_COLORS.nucleic }))];
}

function cutoffExample() {
    cutoff.value = 3.5;
}

const progress_text = computed(() => {
    if (!processing.value || progress.value.total === 0) return "";
    return `(${progress.value.current}/${progress.value.total}) ${progress.value.current_file}`;
});

const run_button_text = computed(() => {
    if (!processing.value) return "Run";
    // When using PDB IDs, we first download structures in prepareInputsFromPdbIds().
    // During that phase, we have no batch progress yet.
    if (input_method.value === "id" && progress.value.total === 0) return "Downloading PDB files by ID...";
    return progress_text.value ? `Processing... ${progress_text.value}` : "Processing...";
});

const is_results_view = computed(() => route.query.view === "results");

function makeJsonFilename({ base }) {
    const c = sanitizeKey(String(cutoff.value));
    return `${base}.binding_pairs.cutoff_${c}.csv`;
}

function normalizePairsResult(result) {
    // Worker returns: { ok:true, kind:"binding_pairs", pairs: string[], distances: number[] }
    // Be defensive in case older payloads still come through.
    const pairs = Array.isArray(result?.pairs) ? result.pairs.map((x) => String(x)) : [];
    const distancesRaw = Array.isArray(result?.distances) ? result.distances : [];
    const distances = distancesRaw.map((x) => Number(x));

    if (pairs.length > 0 || distances.length > 0) return { pairs, distances };

    const entries = Array.isArray(result?.entries) ? result.entries : [];
    return {
        pairs: entries.map((e) => String(e?.[0] ?? "")),
        distances: entries.map((e) => Number(e?.[1])),
    };
}

function parseResidueToken(token) {
    // token format: chain-resSeq-resName
    // Be tolerant: resName may contain extra '-' (rare), keep the remainder.
    const parts = String(token || "").split("-");
    const chain = parts[0] ?? "";
    const resi = parts.length >= 2 ? parts[1] : "";
    const resn = parts.length >= 3 ? parts.slice(2).join("-") : "";
    return { chain, resi, resn };
}

function parsePair(pair) {
    // pair format: protChain-protResi-protResn_naChain-naResi-naResn
    const [left, right] = String(pair || "").split("_");
    const prot = parseResidueToken(left);
    const na = parseResidueToken(right);
    return { prot, na };
}

function toCsvRow(values) {
    return values
        .map((v) => {
            const s = v === null || v === undefined ? "" : String(v);
            const escaped = s.replaceAll('"', '""');
            return /[\n\r,\"]/g.test(escaped) ? `"${escaped}"` : escaped;
        })
        .join(",");
}

function downloadTextFile({ text, filename, mime = "text/csv;charset=utf-8" }) {
    const blob = new Blob([text], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
}

function uniqueZipName(name, used) {
    const clean = String(name).replace(/[/\\]+/g, "_");
    if (!used.has(clean)) {
        used.add(clean);
        return clean;
    }
    let i = 2;
    while (used.has(`${clean} (${i})`)) i++;
    const finalName = `${clean} (${i})`;
    used.add(finalName);
    return finalName;
}

function csvTextForResult(res) {
    const header = toCsvRow(["prot_chain", "prot_resi", "prot_resn", "na_chain", "na_resi", "na_resn", "distance"]);
    const body = (res?.rows || []).map((r) => toCsvRow([r.prot_chain, r.prot_resi, r.prot_resn, r.na_chain, r.na_resi, r.na_resn, r.distance]));
    return [header, ...body].join("\n") + "\n";
}

async function runAnnotateBindingPairs() {
    error_message.value = "";
    file_errors.value = [];
    progress.value = { current: 0, total: 0, current_file: "" };
    results.value = [];
    current_index.value = 0;

    if (input_method.value === "file") {
        if (files.value.length === 0) {
            error_message.value = "Please upload at least one structure file (.pdb or .cif).";
            return;
        }
    } else if (input_method.value === "id") {
        if (parsed_ids.value.length === 0) {
            error_message.value = "Please enter at least one PDB ID (separated by commas).";
            return;
        }
        if (!ids_valid.value) {
            error_message.value = "PDB ID format is incorrect: must be 4 alphanumeric characters (separated by commas).";
            return;
        }
    } else {
        error_message.value = "Please select an input method (ID or file).";
        return;
    }

    processing.value = true;
    try {
        last_run_input_method.value = input_method.value;
        const inputs = input_method.value === "file" ? await prepareInputsFromFiles(files.value) : await prepareInputsFromPdbIds(parsed_ids.value);

        // ID mode optimization: keep the downloaded bytes for Mol* rendering.
        // Worker calls should use a copy to avoid transferring/detaching our cached buffer.
        idStructureCache = new Map();
        if (last_run_input_method.value === "id") {
            for (const input of inputs || []) {
                const id = normalizePdbId(input?.base);
                if (id && input?.bytes) {
                    idStructureCache.set(id, { bytes: input.bytes, format: input.format });
                }
            }
        }

        const { downloads, errors } = await runBatch({
            inputs,
            processOne: (input) => {
                const bytesForWorker = last_run_input_method.value === "id" ? input.bytes.slice() : input.bytes;
                return annotateBindingPairsInWorker(bytesForWorker, cutoff.value, input.format);
            },
            toDownloadItems: (result, input) => {
                const normalized = normalizePairsResult(result);
                const n = Math.min(normalized.pairs.length, normalized.distances.length);
                const rows = [];
                for (let i = 0; i < n; i++) {
                    const pair = normalized.pairs[i];
                    const parsed = parsePair(pair);
                    rows.push({
                        prot_chain: parsed.prot.chain,
                        prot_resi: parsed.prot.resi,
                        prot_resn: parsed.prot.resn,
                        na_chain: parsed.na.chain,
                        na_resi: parsed.na.resi,
                        na_resn: parsed.na.resn,
                        distance: normalized.distances[i],
                        _raw_pair: pair,
                    });
                }

                return [
                    {
                        source: input.source,
                        base: input.base,
                        format: input.format,
                        cutoff: cutoff.value,
                        pairs: normalized.pairs,
                        distances: normalized.distances,
                        rows,
                    },
                ];
            },
            onProgress: (p) => {
                progress.value = p;
            },
        });

        results.value = downloads;
        file_errors.value = errors;

        // Switch to results view via URL state so the browser back button returns to the form.
        if ((downloads?.length || 0) > 0) {
            const q = { ...route.query, view: "results" };
            await router.push({ query: q });
        }
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    } finally {
        processing.value = false;
        progress.value = { current: 0, total: 0, current_file: "" };
    }
}

const has_results = computed(() => results.value.length > 0);

const has_multiple_results = computed(() => results.value.length > 1);

const current_result = computed(() => {
    if (!has_results.value) return null;
    const i = Math.min(Math.max(0, current_index.value), results.value.length - 1);
    return results.value[i] ?? null;
});

const current_title = computed(() => {
    if (!current_result.value) return "";
    const i = Math.min(Math.max(0, current_index.value), results.value.length - 1);
    return `${current_result.value.source} (${i + 1}/${results.value.length})`;
});

const can_next_table = computed(() => {
    return has_multiple_results.value && !processing.value;
});

function nextTable() {
    if (!has_multiple_results.value) return;
    current_index.value = (current_index.value + 1) % results.value.length;
    selected_row_index.value = -1;
}

async function focusRowPair(row, rowIndex = -1) {
    const prot_chain = String(row?.prot_chain ?? "").trim();
    const prot_resi = Number.parseInt(String(row?.prot_resi ?? ""), 10);
    const na_chain = String(row?.na_chain ?? "").trim();
    const na_resi = Number.parseInt(String(row?.na_resi ?? ""), 10);

    if (!viewerInstance) return;
    if (!prot_chain || !Number.isFinite(prot_resi) || !na_chain || !Number.isFinite(na_resi)) return;

    selected_row_index.value = typeof rowIndex === "number" ? rowIndex : -1;

    try {
        await highlightResidues(viewerInstance, {
            data: [
                { auth_asym_id: prot_chain, auth_residue_number: prot_resi },
                { auth_asym_id: na_chain, auth_residue_number: na_resi },
            ],
            color: MOLSTAR_COLORS.focus,
            focus: true,
        });
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

const can_download_table = computed(() => {
    return !!current_result.value && !processing.value;
});

const can_download_all_tables = computed(() => {
    return has_results.value && !processing.value;
});

function downloadCurrentTable() {
    if (!current_result.value) return;
    const header = toCsvRow(["prot_chain", "prot_resi", "prot_resn", "na_chain", "na_resi", "na_resn", "distance"]);
    const body = (current_result.value.rows || []).map((r) => toCsvRow([r.prot_chain, r.prot_resi, r.prot_resn, r.na_chain, r.na_resi, r.na_resn, r.distance]));
    const text = [header, ...body].join("\n") + "\n";
    const filename = makeJsonFilename({ base: current_result.value.base || "results" });
    downloadTextFile({ text, filename });
}

async function downloadAllTablesZip() {
    if (!can_download_all_tables.value) return;

    try {
        const zip = new JSZip();
        const used = new Set();
        const c = sanitizeKey(String(cutoff.value));

        for (const res of results.value || []) {
            const base = sanitizeKey(String(res?.base || "results")) || "results";
            const filename = `${base}.binding_pairs.cutoff_${c}.csv`;
            const safeName = uniqueZipName(filename, used);
            zip.file(safeName, csvTextForResult(res));
        }

        const blob = await zip.generateAsync({ type: "blob" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "binding_pairs_tables.zip";
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

async function renderMolstarForCurrentResult() {
    const res = current_result.value;
    if (!res) return;
    if (!viewerContainer.value) return;

    try {
        await ensurePdbeMolstarLoaded();
        await nextTick();

        if (!viewerInstance) {
            viewerInstance = createPdbeMolstarViewer();
        }

        // Build options matching MoleculeViewer.vue
        const options = {};
        let nextKey = "";
        if (last_run_input_method.value === "file") {
            const f = (files.value || []).find((x) => x?.name === res.source);
            if (f) {
                nextKey = `file:${f.name}`;
                if (viewerStructureKey !== nextKey) {
                    viewerStructureKey = nextKey;
                    revokeViewerObjectUrl();
                    const url = URL.createObjectURL(f);
                    viewerLastObjectUrl = url;
                }
                options.customData = {
                    url: viewerLastObjectUrl,
                    format: molstarFormatFromPskitFormat(res.format),
                    binary: false,
                };
            } else {
                // Fallback: best effort for file-missing edge case
                const id = String(res.base || "").trim();
                if (id) nextKey = `id:${normalizePdbId(id)}`;
            }
        } else {
            const id = String(res.base || "").trim();
            if (id) nextKey = `id:${normalizePdbId(id)}`;
        }

        if (nextKey && nextKey.startsWith("id:")) {
            const id = nextKey.slice("id:".length);
            const cached = idStructureCache.get(id);
            if (cached?.bytes) {
                if (viewerStructureKey !== nextKey) {
                    viewerStructureKey = nextKey;
                    revokeViewerObjectUrl();
                    viewerLastObjectUrl = createBlobUrlFromBytes(cached.bytes);
                }
                options.customData = {
                    url: viewerLastObjectUrl,
                    format: molstarFormatFromPskitFormat(cached.format),
                    binary: false,
                };
            } else if (id) {
                // Fallback if cache is missing
                options.moleculeId = id.toLowerCase();
            }
        }

        await renderPdbeMolstar(viewerInstance, viewerContainer.value, options);

        // Default view: non-selected = grey; binding residues colored by type.
        const selections = buildColoredSelectionsFromRows(res.rows);
        await applySelectionWithRetry(viewerInstance, {
            data: selections,
            nonSelectedColor: MOLSTAR_COLORS.nonSelected,
            focus: false,
            keepRepresentations: true,
        });

        selected_row_index.value = -1;
    } catch (e) {
        // Mol* is optional enhancement; show error in the existing alert area.
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

watch(
    () => ({ inResults: is_results_view.value, has: has_results.value, idx: current_index.value }),
    async () => {
        if (!is_results_view.value) return;
        if (!has_results.value) return;
        await renderMolstarForCurrentResult();
    },
    { flush: "post" },
);

// If the user refreshes or directly visits AnnotateBindingSites?view=results, we may have no
// in-memory results. In that case, fall back to the form view.
watch(
    () => route.query.view,
    (v) => {
        if (v !== "results") return;
        if (processing.value) return;
        if (has_results.value) return;
        const q = { ...route.query };
        delete q.view;
        router.replace({ query: q });
    },
    { immediate: true },
);

onBeforeUnmount(async () => {
    revokeViewerObjectUrl();
    idStructureCache = new Map();
    try {
        if (viewerInstance?.clear) await viewerInstance.clear();
    } catch {
        // ignore
    }
    viewerInstance = null;
    viewerStructureKey = "";
});
</script>

<template>
    <div class="mx-auto py-8 px-4" :class="is_results_view && has_results ? 'max-w-full' : 'max-w-3xl'">
        <div v-if="is_results_view && has_results" class="grid grid-cols-1 gap-6 lg:grid-cols-2">
            <!-- Left: Mol* viewer (replaces form after results) -->
            <div class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Structure</p>
                    <div class="text-sm font-semibold text-gray-700 dark:text-gray-300">{{ current_title }}</div>
                </div>
                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <div class="w-full h-[720px] relative rounded-lg border border-gray-200 dark:border-gray-700">
                    <div ref="viewerContainer" class="w-full h-full relative"></div>
                </div>
                <!-- Color Legend -->
                <div v-if="current_result" class="mt-4 flex items-center justify-center gap-4 text-sm">
                    <div class="flex items-center gap-2">
                        <div class="w-4 h-4 rounded" style="background-color: rgb(52, 152, 219)"></div>
                        <span class="text-gray-700 dark:text-gray-300">Binding AA</span>
                    </div>
                    <div class="flex items-center gap-2">
                        <div class="w-4 h-4 rounded" style="background-color: rgb(231, 76, 60)"></div>
                        <span class="text-gray-700 dark:text-gray-300">Binding NT</span>
                    </div>
                    <div class="flex items-center gap-2">
                        <div class="w-4 h-4 rounded" style="background-color: rgb(255, 235, 59)"></div>
                        <span class="text-gray-700 dark:text-gray-300">Selected</span>
                    </div>
                </div>
            </div>

            <!-- Right: results -->
            <div class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Results</p>
                    <div class="flex items-center gap-2">
                        <button v-if="has_multiple_results" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_next_table" @click="nextTable">Next</button>
                        <button v-if="has_multiple_results" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_all_tables" @click="downloadAllTablesZip">Download All (ZIP)</button>
                    </div>
                </div>

                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <div v-if="current_result" class="flex flex-col h-[720px] rounded-lg border border-gray-200 dark:border-gray-700">
                    <div class="flex justify-between items-center mb-2 px-3 pt-3">
                        <div class="space-y-2">
                            <div class="text-sm font-semibold text-gray-900 dark:text-gray-200">{{ current_title }}</div>
                            <div class="text-xs text-gray-500 dark:text-gray-300">cutoff: {{ current_result.cutoff }} Å, {{ current_result.rows.length }} pairs</div>
                        </div>
                        <button class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_table" @click="downloadCurrentTable">Download (CSV)</button>
                    </div>
                    <div class="max-h-screen overflow-y-auto">
                        <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                            <thead class="bg-gray-100 dark:bg-gray-700 sticky top-0 z-10">
                                <tr>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">#</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">prot_chain</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">prot_resi</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">prot_resn</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">na_chain</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">na_resi</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">na_resn</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">distance (Å)</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-gray-200 dark:divide-gray-700">
                                <tr v-for="(r, idx) in current_result.rows" :key="`${idx}-${r._raw_pair || ''}`" class="cursor-pointer transition-colors" :class="idx === selected_row_index ? 'bg-blue-50 dark:bg-blue-900/30 ring-2 ring-blue-400/60 ring-inset' : 'bg-white dark:bg-gray-800 hover:bg-gray-50 dark:hover:bg-gray-700'" :aria-selected="idx === selected_row_index" @click="focusRowPair(r, idx)">
                                    <td class="px-4 py-2 text-xs text-gray-700 dark:text-gray-300">{{ idx + 1 }}</td>
                                    <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-200">{{ r.prot_chain }}</td>
                                    <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ r.prot_resi }}</td>
                                    <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ r.prot_resn }}</td>
                                    <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-200">{{ r.na_chain }}</td>
                                    <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ r.na_resi }}</td>
                                    <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ r.na_resn }}</td>
                                    <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ Number.isFinite(r.distance) ? r.distance.toFixed(3) : r.distance }}</td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </div>
                <div v-if="error_message" class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                    {{ error_message }}
                </div>

                <div v-if="file_errors.length > 0" class="mt-4 rounded-lg border border-yellow-200 bg-yellow-50 p-3 text-sm text-yellow-800 dark:border-yellow-900 dark:bg-yellow-950 dark:text-yellow-200">
                    <div class="font-medium">Failed to process the following file(s)</div>
                    <ul class="mt-2 space-y-1">
                        <li v-for="e in file_errors" :key="e.source" class="text-xs">
                            <span class="font-semibold">{{ e.source }}</span
                            >: {{ e.message }}
                        </li>
                    </ul>
                </div>
            </div>
        </div>

        <form v-else @submit.prevent="runAnnotateBindingPairs" class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Annotate Binding Sites</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" :max-files="200" :max-size="500 * 1024 * 1024" />
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div class="flex flex-row justify-between my-4">
                <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Cutoff (Å)</span>
            </div>

            <div class="w-auto">
                <label class="w-full block mb-2 text-sm font-medium text-gray-900 dark:text-gray-300">nearest atomic distance <span @click="cutoffExample" class="text-xs cursor-pointer hover:text-blue-700 hover:underline font-normal">(e.g., 3.5)</span></label>
                <input type="number" required step="0.1" v-model.number="cutoff" min="0" class="w-full rounded-lg border border-gray-300 bg-gray-50 p-2.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400 focus:border-blue-400 text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:placeholder-gray-400" />
            </div>

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <button type="submit" class="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-lg text-center font-medium text-white hover:bg-blue-700 transition disabled:opacity-60 disabled:cursor-not-allowed" :disabled="processing" :aria-busy="processing">
                <Loading v-if="processing" class="h-5 w-5 text-white" />
                <span>{{ run_button_text }}</span>
            </button>

            <div v-if="error_message" class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                {{ error_message }}
            </div>

            <div v-if="file_errors.length > 0" class="mt-4 rounded-lg border border-yellow-200 bg-yellow-50 p-3 text-sm text-yellow-800 dark:border-yellow-900 dark:bg-yellow-950 dark:text-yellow-200">
                <div class="font-medium">Failed to process the following file(s)</div>
                <ul class="mt-2 space-y-1">
                    <li v-for="e in file_errors" :key="e.source" class="text-xs">
                        <span class="font-semibold">{{ e.source }}</span
                        >: {{ e.message }}
                    </li>
                </ul>
            </div>
        </form>
    </div>
</template>
