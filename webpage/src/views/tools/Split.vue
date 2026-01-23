<script setup>
import { ref, computed, onBeforeUnmount, watch, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import InputStructure from "../../components/InputStructure.vue";
import { parsePdbIds, isValidPdbId, revokeDownloadItems, groupDownloadItemsBySource, prepareInputsFromFiles, prepareInputsFromPdbIds, runBatch, splitComplexInWorker, splitByChainInWorker, workerChunksToDownloadItems, downloadGroupedAsZip, stripExtension, getFormatFromFileName } from "../../utils/wasmBatch.js";
import { ensurePdbeMolstarLoaded, createPdbeMolstarViewer, renderPdbeMolstar, applySelectionWithRetry, waitForStructureReady, molstarFormatFromPskitFormat, createBlobUrlFromBytes } from "../../utils/pdbeMolstar.js";
import Loading from "../../components/Loading.vue";

const route = useRoute();
const router = useRouter();

const MOLSTAR_COLORS = {
    nonSelected: { r: 190, g: 190, b: 190 },
    highlight: { r: 52, g: 152, b: 219 },
};

const input_method = ref("file");
const ids = ref("");
const files = ref([]);
const split_type = ref("chain");
const processing = ref(false);
const error_message = ref("");
const results = ref([]);
const file_errors = ref([]);
const progress = ref({ current: 0, total: 0, current_file: "" });
const last_run_input_method = ref("file");

const viewerContainer = ref(null);
let viewerInstance = null;
let viewerLastObjectUrl = null;
let viewerStructureKey = "";
let idStructureCache = new Map();

const selected_result = ref(null);

function revokeViewerObjectUrl() {
    if (!viewerLastObjectUrl) return;
    try {
        URL.revokeObjectURL(viewerLastObjectUrl);
    } catch {
        // ignore
    }
    viewerLastObjectUrl = null;
}

function pdbIdFromSource(source) {
    const base = stripExtension(String(source || ""))
        .trim()
        .toLowerCase();
    return isValidPdbId(base) ? base : "";
}

function molstarFormatFromFileName(name) {
    const ext = getFormatFromFileName(String(name || ""));
    if (ext === "cif") return "mmcif";
    return "pdb";
}

function tokenizeCifLine(line) {
    return (line.match(/'[^']*'|"[^"]*"|\S+/g) || []).map((t) => t.replace(/^['"]|['"]$/g, ""));
}

function inferChainSelectorFromStructureText(text, format) {
    const fmt = String(format || "").toLowerCase();
    if (fmt === "pdb") {
        const ids = new Set();
        const lines = String(text || "").split(/\r?\n/);
        for (const line of lines) {
            if (!line || line.length < 22) continue;
            if (!(line.startsWith("ATOM") || line.startsWith("HETATM"))) continue;
            const c = String(line[21] || "").trim();
            if (c) ids.add(c);
        }
        return { field: "auth_asym_id", ids: Array.from(ids) };
    }

    const lines = String(text || "").split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
        if (lines[i].trim() !== "loop_") continue;

        const headers = [];
        let j = i + 1;
        for (; j < lines.length; j++) {
            const s = lines[j].trim();
            if (!s) continue;
            if (!s.startsWith("_")) break;
            if (s.startsWith("_atom_site.")) headers.push(s);
        }

        if (headers.length === 0) continue;
        const authIdx = headers.findIndex((h) => h === "_atom_site.auth_asym_id");
        const labelIdx = headers.findIndex((h) => h === "_atom_site.label_asym_id");
        const colIdx = authIdx >= 0 ? authIdx : labelIdx;
        if (colIdx < 0) continue;

        const field = authIdx >= 0 ? "auth_asym_id" : "struct_asym_id";
        const ids = new Set();
        for (; j < lines.length; j++) {
            const s = lines[j].trim();
            if (!s) continue;
            if (s === "#" || s === "loop_" || s.startsWith("_")) break;
            const tokens = tokenizeCifLine(s);
            if (tokens.length <= colIdx) continue;
            const v = String(tokens[colIdx] || "").trim();
            if (v && v !== "." && v !== "?") ids.add(v);
        }

        return { field, ids: Array.from(ids) };
    }

    return { field: "auth_asym_id", ids: [] };
}

onBeforeUnmount(() => {
    revokeDownloadItems(results.value);
    revokeViewerObjectUrl();
    idStructureCache = new Map();
    try {
        viewerInstance?.clear?.();
    } catch {
        // ignore
    }
    viewerInstance = null;
});
const parsed_ids = computed(() => parsePdbIds(ids.value));
const ids_valid = computed(() => {
    if (parsed_ids.value.length === 0) return false;
    return parsed_ids.value.every(isValidPdbId);
});

const progress_text = computed(() => {
    if (!processing.value || progress.value.total === 0) return "";
    return `(${progress.value.current}/${progress.value.total}) ${progress.value.current_file}`;
});

const is_results_view = computed(() => route.query.view === "results");

const run_button_text = computed(() => {
    if (!processing.value) return "Run";
    // When using PDB IDs, we first download structures in prepareInputsFromPdbIds().
    // During that phase, we have no batch progress yet.
    if (input_method.value === "id" && progress.value.total === 0) return "Downloading PDB files by ID...";
    return progress_text.value ? `Processing... ${progress_text.value}` : "Processing...";
});

async function runSplit() {
    error_message.value = "";
    file_errors.value = [];
    progress.value = { current: 0, total: 0, current_file: "" };
    //清理之前的结果
    revokeDownloadItems(results.value);
    results.value = [];

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

    last_run_input_method.value = input_method.value;
    processing.value = true;
    try {
        const inputs = input_method.value === "file" ? await prepareInputsFromFiles(files.value) : await prepareInputsFromPdbIds(parsed_ids.value);

        // ID mode optimization: keep downloaded bytes for Mol*; worker should use a copy.
        idStructureCache = new Map();
        if (last_run_input_method.value === "id") {
            for (const input of inputs || []) {
                const id = String(input?.base || "")
                    .trim()
                    .toLowerCase();
                if (id && input?.bytes) idStructureCache.set(id, { bytes: input.bytes, format: input.format });
            }
        }

        const processOne = (input) => {
            const bytesForWorker = last_run_input_method.value === "id" ? input.bytes.slice() : input.bytes;
            if (split_type.value === "mol_type") {
                return splitComplexInWorker(bytesForWorker, input.format);
            }
            return splitByChainInWorker(bytesForWorker, input.format);
        };

        const { downloads, errors } = await runBatch({
            inputs,
            processOne,
            toDownloadItems: (result, input) =>
                workerChunksToDownloadItems({
                    items: result.items,
                    base: input.base,
                    format: input.format,
                    source: input.source,
                }),
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

const grouped_results = computed(() => {
    return groupDownloadItemsBySource(results.value);
});

const can_download_all = computed(() => {
    return has_results.value && !processing.value;
});

const current_title = computed(() => {
    const r = selected_result.value;
    if (!r) return "";
    return `${r.source} · ${String(r.key || "").trim()}`;
});

function selectResultItem(item) {
    selected_result.value = item;
}

function isSelected(item) {
    if (!selected_result.value) return false;
    return selected_result.value.source === item.source && selected_result.value.filename === item.filename;
}

async function applyGreyAndHighlightSplitPart(item) {
    if (!viewerInstance) return;
    if (!item) return;

    if (split_type.value === "chain") {
        const chainId = String(item.key || "").trim();
        if (!chainId) return;
        const data = [{ auth_asym_id: chainId, color: MOLSTAR_COLORS.highlight }];
        await applySelectionWithRetry(viewerInstance, {
            data,
            nonSelectedColor: MOLSTAR_COLORS.nonSelected,
            focus: true,
            keepRepresentations: true,
        });
        return;
    }

    if (!item.blob) return;
    let selector = { field: "auth_asym_id", ids: [] };
    try {
        const text = await item.blob.text();
        selector = inferChainSelectorFromStructureText(text, getFormatFromFileName(item.filename));
    } catch {
        selector = { field: "auth_asym_id", ids: [] };
    }

    if (selector.ids.length === 0) return;
    const data = selector.ids.map((id) => ({ [selector.field]: id, color: MOLSTAR_COLORS.highlight }));
    await applySelectionWithRetry(viewerInstance, {
        data,
        nonSelectedColor: MOLSTAR_COLORS.nonSelected,
        focus: true,
        keepRepresentations: true,
    });
}

async function renderMolstarForSelected() {
    const item = selected_result.value;
    if (!item) return;
    if (!viewerContainer.value) return;

    try {
        await ensurePdbeMolstarLoaded();
        await nextTick();

        if (!viewerInstance) {
            viewerInstance = createPdbeMolstarViewer();
        }

        let nextKey = "";

        if (last_run_input_method.value === "file") {
            const f = (files.value || []).find((x) => x?.name === item.source);
            if (f) {
                nextKey = `file:${f.name}`;
            } else {
                const id = pdbIdFromSource(item.source);
                if (id) nextKey = `id:${id}`;
            }
        } else {
            const id = pdbIdFromSource(item.source);
            if (id) nextKey = `id:${id}`;
        }

        if (!nextKey) return;

        if (viewerStructureKey !== nextKey) {
            viewerStructureKey = nextKey;
            error_message.value = "";

            const options = {};
            // Changing structure: revoke any previous object URL now.
            revokeViewerObjectUrl();

            if (nextKey.startsWith("file:")) {
                const fileName = nextKey.slice("file:".length);
                const f = (files.value || []).find((x) => x?.name === fileName);
                if (!f) return;
                const url = URL.createObjectURL(f);
                viewerLastObjectUrl = url;
                options.customData = {
                    url,
                    format: molstarFormatFromFileName(f.name),
                    binary: false,
                };
            } else {
                const id = nextKey.slice("id:".length);
                const cached = idStructureCache.get(
                    String(id || "")
                        .trim()
                        .toLowerCase(),
                );
                if (cached?.bytes) {
                    const url = createBlobUrlFromBytes(cached.bytes);
                    viewerLastObjectUrl = url;
                    options.customData = {
                        url,
                        format: molstarFormatFromPskitFormat(cached.format),
                        binary: false,
                    };
                } else {
                    options.moleculeId = id.toLowerCase();
                }
            }

            await renderPdbeMolstar(viewerInstance, viewerContainer.value, options);

            const ok = await waitForStructureReady(viewerInstance, { maxTries: 20, intervalMs: 150 });
            if (!ok) {
                error_message.value = "Mol* 未能加载结构（请检查输入结构格式，.cif 需要 mmCIF；或打开浏览器控制台查看网络请求是否失败）。";
                return;
            }
        }

        await applyGreyAndHighlightSplitPart(item);
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

watch(
    () => results.value.length,
    async () => {
        if (!is_results_view.value) return;
        if (!has_results.value) return;
        if (!selected_result.value) {
            selected_result.value = grouped_results.value?.[0]?.items?.[0] ?? null;
        }
        await renderMolstarForSelected();
    },
    { flush: "post" },
);

watch(
    () => selected_result.value,
    async () => {
        if (!is_results_view.value) return;
        if (!has_results.value) return;
        await renderMolstarForSelected();
    },
    { flush: "post" },
);

// When navigating back/forward into results view, ensure the viewer is rendered.
watch(
    () => is_results_view.value,
    async (v) => {
        if (!v) return;
        if (!has_results.value) return;
        await nextTick();
        if (!selected_result.value) {
            selected_result.value = grouped_results.value?.[0]?.items?.[0] ?? null;
        }
        await renderMolstarForSelected();
    },
    { flush: "post" },
);

// If the user refreshes or directly visits SplitComplex?view=results, we may have no
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

async function downloadAllAsZip() {
    if (!can_download_all.value) return;

    try {
        const zipName = split_type.value === "chain" ? "split_by_chain_results.zip" : "split_results.zip";
        await downloadGroupedAsZip(grouped_results.value, zipName);
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}
</script>
<template>
    <div class="mx-auto py-8 px-4" :class="is_results_view && has_results ? 'max-w-full' : 'max-w-3xl'">
        <div v-if="is_results_view && has_results" class="grid grid-cols-1 gap-6 lg:grid-cols-2">
            <!-- Left: structure viewer -->
            <div class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Structure</p>
                    <div class="text-sm font-semibold text-gray-700 dark:text-gray-300">{{ current_title }}</div>
                </div>
                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <div class="w-full rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden" style="height: 720px; position: relative">
                    <div ref="viewerContainer" class="w-full h-full" style="height: 100%; width: 100%; position: relative"></div>
                </div>
            </div>

            <!-- Right: results -->
            <div class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Results</p>
                    <button v-if="can_download_all" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_all" @click="downloadAllAsZip">Download All (ZIP)</button>
                </div>
                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <div class="rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800 max-h-screen overflow-y-auto">
                    <div class="space-y-4">
                        <div v-for="g in grouped_results" :key="g.source">
                            <div class="mb-2 text-sm font-semibold text-gray-900 dark:text-gray-200">{{ g.source }}</div>
                            <ul class="space-y-2">
                                <li v-for="r in g.items" :key="r.filename" class="flex items-center justify-between rounded-lg px-2 py-2 cursor-pointer" :class="isSelected(r) ? 'bg-blue-50 ring-2 ring-blue-200 dark:bg-blue-950/40 dark:ring-blue-800' : 'hover:bg-gray-50 dark:hover:bg-gray-700/40'" @click="selectResultItem(r)">
                                    <div class="min-w-0">
                                        <div class="truncate text-sm font-medium text-gray-900 dark:text-gray-200">{{ r.filename }}</div>
                                        <div class="text-xs text-gray-500 dark:text-gray-300">key: {{ r.key }} · {{ (r.size / 1024).toFixed(2) }} KB</div>
                                    </div>
                                    <a class="rounded-lg border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :href="r.url" :download="r.filename" @click.stop> Download </a>
                                </li>
                            </ul>
                        </div>
                    </div>
                </div>

                <div v-if="error_message" class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                    {{ error_message }}
                </div>

                <div v-if="file_errors.length > 0" class="mt-4 rounded-lg border border-yellow-200 bg-yellow-50 p-3 text-sm text-yellow-800 dark:border-yellow-900 dark:bg-yellow-950 dark:text-yellow-200">
                    <div class="font-medium">Failed to process the following file(s):</div>
                    <ul class="mt-2 space-y-1">
                        <li v-for="e in file_errors" :key="e.source" class="text-xs">
                            <span class="font-semibold">{{ e.source }}</span
                            >: {{ e.message }}
                        </li>
                    </ul>
                </div>
            </div>
        </div>

        <form v-else @submit.prevent="runSplit" class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Split Complex</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />
            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" :max-files="200" :max-size="500 * 1024 * 1024" />
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />
            <!--split type: "by chain" or "by type" (protein or nucleic acid)-->
            <div class="my-4">
                <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Split Type</span>
            </div>
            <div>
                <ul class="w-full rounded-lg border border-gray-300 bg-white text-sm font-medium text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-white">
                    <li class="border-b border-gray-300 dark:border-gray-600">
                        <div class="flex items-center p-4">
                            <input id="chain" type="radio" value="chain" v-model="split_type" class="h-4 w-4 accent-blue-600" />
                            <label for="chain" class="ms-2 w-full">
                                <span class="text-sm font-medium text-gray-900 dark:text-gray-300">Split by Chain</span>
                                <span class="text-xs text-gray-500 dark:text-gray-400 block">Separate all chains from the uploaded structures.</span>
                            </label>
                        </div>
                    </li>
                    <li>
                        <div class="flex items-center p-4">
                            <input id="mol_type" type="radio" value="mol_type" v-model="split_type" class="h-4 w-4 accent-blue-600" />
                            <label for="mol_type" class="ms-2 w-full">
                                <span class="text-sm font-medium text-gray-900 dark:text-gray-300"> Split by Molecule Type </span>
                                <span class="text-xs text-gray-500 dark:text-gray-400 block">Separate proteins and nucleic acids from the uploaded structures.</span>
                            </label>
                        </div>
                    </li>
                </ul>
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
                <div class="font-medium">Failed to process the following file(s):</div>
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
