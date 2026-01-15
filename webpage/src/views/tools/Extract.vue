<script setup>
import { ref, computed, onBeforeUnmount, watch, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import InputStructure from "../../components/InputStructure.vue";
import { parsePdbIds, isValidPdbId, revokeDownloadItems, groupDownloadItemsBySource, prepareInputsFromFiles, prepareInputsFromPdbIds, runBatch, extractFragmentInWorker, bytesToDownloadItem, sanitizeKey, downloadGroupedAsZip, stripExtension, getFormatFromFileName } from "../../utils/wasmBatch.js";
import { ensurePdbeMolstarLoaded, createPdbeMolstarViewer, renderPdbeMolstar, applySelectionWithRetry, waitForStructureReady, molstarFormatFromPskitFormat, createBlobUrlFromBytes } from "../../utils/pdbeMolstar.js";

const route = useRoute();
const router = useRouter();

const MOLSTAR_COLORS = {
    nonSelected: { r: 190, g: 190, b: 190 },
    highlight: { r: 52, g: 152, b: 219 },
};

const input_method = ref("file");
const ids = ref("");
const files = ref([]);

const chain_id = ref("");

function chain_id_example() {
    chain_id.value = "A";
}

const start = ref(null);
function start_example() {
    start.value = 50;
}
const end = ref(null);
function end_example() {
    end.value = 120;
}

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

function makeFragmentFilename({ base, format, start, end }) {
    const c = chain_id.value.trim();
    const cPart = c ? sanitizeKey(c) : "all";
    const s = start;
    const e = end;
    return `${base}.fragment.${cPart}.${s ?? "start"}-${e ?? "end"}.${format}`;
}

async function runExtractFragment() {
    error_message.value = "";
    file_errors.value = [];
    progress.value = { current: 0, total: 0, current_file: "" };
    revokeDownloadItems(results.value);
    results.value = [];

    if (input_method.value === "file") {
        if (files.value.length === 0) {
            error_message.value = "Please upload at least 1 structure file (.pdb or .cif).";
            return;
        }
    } else if (input_method.value === "id") {
        if (parsed_ids.value.length === 0) {
            error_message.value = "Please enter at least 1 PDB ID (separated by commas).";
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

        const chainArg = chain_id.value.trim();

        const { downloads, errors } = await runBatch({
            inputs,
            processOne: (input) => {
                const bytesForWorker = last_run_input_method.value === "id" ? input.bytes.slice() : input.bytes;
                return extractFragmentInWorker(bytesForWorker, chainArg, start.value, end.value, input.format);
            },
            toDownloadItems: (result, input) => {
                const filename = makeFragmentFilename({ base: input.base, format: input.format, start: result?.start, end: result?.end });
                const item = bytesToDownloadItem({
                    bytes: result.bytes,
                    filename,
                    source: input.source,
                    key: "fragment",
                });
                item.meta = {
                    chain_id: chainArg,
                    start: result?.start,
                    end: result?.end,
                };
                return [item];
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

const grouped_results = computed(() => {
    return groupDownloadItemsBySource(results.value);
});

const can_download_all = computed(() => {
    return has_results.value && !processing.value;
});

const current_title = computed(() => {
    const r = selected_result.value;
    if (!r) return "";
    const c = chain_id.value.trim();
    const s = r?.meta?.start;
    const e = r?.meta?.end;
    return `${r.source} · chain ${c} · ${s}-${e}`;
});

function selectResultItem(item) {
    selected_result.value = item;
}

function isSelected(item) {
    if (!selected_result.value) return false;
    return selected_result.value.source === item.source && selected_result.value.filename === item.filename;
}

async function applyGreyAndHighlightFragment(item) {
    if (!viewerInstance) return;
    if (!item?.blob) return;

    const chainInput = chain_id.value.trim();
    if (!chainInput) return;

    const baseParams = {
        start_auth_residue_number: item?.meta?.start,
        end_auth_residue_number: item?.meta?.end,
    };

    const data = [{ ...baseParams, auth_asym_id: chainInput, color: MOLSTAR_COLORS.highlight }];

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
                        .toLowerCase()
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
                error_message.value = "Mol* could not load the structure.";
                return;
            }
        }

        await applyGreyAndHighlightFragment(item);
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
    { flush: "post" }
);

watch(
    () => selected_result.value,
    async () => {
        if (!is_results_view.value) return;
        if (!has_results.value) return;
        await renderMolstarForSelected();
    },
    { flush: "post" }
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
    { flush: "post" }
);

// If the user refreshes or directly visits ExtractFragment?view=results, we may have no
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
    { immediate: true }
);

async function downloadAllAsZip() {
    if (!can_download_all.value) return;

    try {
        await downloadGroupedAsZip(grouped_results.value, "extract_fragment_results.zip");
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
            <div class="w-full max-h-screen bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Results</p>
                    <button v-if="can_download_all" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-gray-500 dark:hover:bg-gray-600" :disabled="!can_download_all" @click="downloadAllAsZip">下载全部（ZIP）</button>
                </div>
                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <div class="rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800 overflow-y-auto">
                    <div class="space-y-4">
                        <div v-for="g in grouped_results" :key="g.source">
                            <div class="mb-2 text-sm font-semibold text-gray-900 dark:text-gray-200">{{ g.source }}</div>
                            <ul class="space-y-2">
                                <li v-for="r in g.items" :key="r.filename" class="flex items-center justify-between rounded-lg px-2 py-2 cursor-pointer" :class="isSelected(r) ? 'bg-blue-50 ring-2 ring-blue-200 dark:bg-blue-950/40 dark:ring-blue-800' : 'hover:bg-gray-50 dark:hover:bg-gray-700/40'" @click="selectResultItem(r)">
                                    <div class="min-w-0">
                                        <div class="truncate text-sm font-medium text-gray-900 dark:text-gray-200">{{ r.filename }}</div>
                                        <div class="text-xs text-gray-500 dark:text-gray-300">key: {{ r.key }} · {{ (r.size / 1024).toFixed(2) }} KB</div>
                                    </div>
                                    <a class="rounded-lg border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-gray-500 dark:hover:bg-gray-600" :href="r.url" :download="r.filename" @click.stop> Download </a>
                                </li>
                            </ul>
                        </div>
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
        <form v-else @submit.prevent="runExtractFragment" class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Extract Fragment</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" :max-files="200" :max-size="500 * 1024 * 1024" />

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div class="my-4">
                <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Range</span>
            </div>

            <div class="grid grid-cols-1 gap-4 md:grid-cols-3">
                <div>
                    <label class="block mb-2 text-sm font-medium text-gray-900 dark:text-gray-300">Chain ID <span @click="chain_id_example" class="text-xs cursor-pointer hover:text-blue-700 hover:underline font-normal">(e.g., A)</span></label>
                    <input type="text" required v-model="chain_id" class="w-full rounded-lg border border-gray-300 bg-gray-50 p-2.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400 focus:border-blue-400 text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-gray-300 dark:placeholder-gray-400" />
                </div>
                <div>
                    <label class="block mb-2 text-sm font-medium text-gray-900 dark:text-gray-300">Start Residue Number <span @click="start_example" class="text-xs cursor-pointer hover:text-blue-700 hover:underline font-normal">(e.g., 50)</span></label>
                    <input type="number" v-model.number="start" class="w-full rounded-lg border border-gray-300 bg-gray-50 p-2.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400 focus:border-blue-400 text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-gray-300 dark:placeholder-gray-400" />
                </div>
                <div>
                    <label class="block mb-2 text-sm font-medium text-gray-900 dark:text-gray-300">End Residue Number <span @click="end_example" class="text-xs cursor-pointer hover:text-blue-700 hover:underline font-normal">(e.g., 120)</span></label>
                    <input type="number" :min="start" v-model.number="end" class="w-full rounded-lg border border-gray-300 bg-gray-50 p-2.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400 focus:border-blue-400 text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-gray-300 dark:placeholder-gray-400" />
                </div>
            </div>

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <button type="submit" class="w-full rounded-lg bg-blue-600 px-4 py-2 text-lg text-center font-medium text-white hover:bg-blue-700">
                {{ run_button_text }}
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
