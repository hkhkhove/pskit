<script setup>
import { ref, computed, watch, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import InputStructure from "../../components/InputStructure.vue";
import TaskLayout from "../../components/TaskLayout.vue";
import { extractFragmentInWorker, bytesToDownloadItem, sanitizeKey, stripExtension, getFormatFromFileName, isValidPdbId } from "../../utils/wasmBatch.js";
import { renderPdbeMolstar, applySelectionWithRetry, waitForStructureReady, molstarFormatFromPskitFormat, createBlobUrlFromBytes } from "../../utils/pdbeMolstar.js";
import { pdbIdFromSource, molstarFormatFromFileName } from "../../utils/structureUtils.js";
import { useMolstar, MOLSTAR_COLORS } from "../../composables/useMolstar.js";
import { useBatchTask } from "../../composables/useBatchTask.js";

const route = useRoute();
const router = useRouter();

const { viewerContainer, initViewer, getViewerInstance, revokeViewerObjectUrl, idStructureCache, getViewerStructureKey, setViewerStructureKey, setViewerLastObjectUrl } = useMolstar();
const { input_method, ids, files, processing, error_message, results, file_errors, last_run_input_method, is_results_view, has_results, grouped_results, can_download_all, run_button_text, executeBatchTask, downloadAllAsZip } = useBatchTask();

const chain_id = ref("");
const start = ref(null);
const end = ref(null);
const selected_result = ref(null);

function chain_id_example() {
    chain_id.value = "A";
}
function start_example() {
    start.value = 50;
}
function end_example() {
    end.value = 120;
}

function makeFragmentFilename({ base, format, start, end }) {
    const c = chain_id.value.trim();
    const cPart = c ? sanitizeKey(c) : "all";
    const s = start;
    const e = end;
    return `${base}.fragment.${cPart}.${s ?? "start"}-${e ?? "end"}.${format}`;
}

async function runExtractFragment() {
    const chainArg = chain_id.value.trim();
    selected_result.value = null; // 重新提交前，先清空选中状态
    await executeBatchTask({
        onInputsPrepared: (inputs, lastInputMethod) => {
            idStructureCache.clear();
            if (lastInputMethod === "id") {
                for (const input of inputs || []) {
                    const id = String(input?.base || "")
                        .trim()
                        .toLowerCase();
                    if (id && input?.bytes) {
                        idStructureCache.set(id, { bytes: input.bytes, format: input.format });
                    }
                }
            }
        },
        processOne: (input) => {
            const bytesForWorker = last_run_input_method.value === "id" ? input.bytes.slice() : input.bytes;
            return extractFragmentInWorker(bytesForWorker, chainArg, start.value, end.value, input.format);
        },
        toDownloadItems: (result, input) => {
            const filename = makeFragmentFilename({
                base: input.base,
                format: input.format,
                start: result?.start,
                end: result?.end,
            });
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
    });
}

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
    const viewerInstance = getViewerInstance();
    if (!viewerInstance) return;
    if (!item?.blob) return;

    const chainInput = chain_id.value.trim();
    if (!chainInput) return;

    const baseParams = {
        start_auth_residue_number: item?.meta?.start,
        end_auth_residue_number: item?.meta?.end,
    };

    const data = [
        {
            ...baseParams,
            auth_asym_id: chainInput,
            color: MOLSTAR_COLORS.highlight,
        },
    ];

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
        const viewerInstance = await initViewer();

        let nextKey = "";

        if (last_run_input_method.value === "file") {
            const f = (files.value || []).find((x) => x?.name === item.source);
            if (f) {
                nextKey = `file:${f.name}`;
            } else {
                const id = pdbIdFromSource(item.source, stripExtension, isValidPdbId);
                if (id) nextKey = `id:${id}`;
            }
        } else {
            const id = pdbIdFromSource(item.source, stripExtension, isValidPdbId);
            if (id) nextKey = `id:${id}`;
        }

        if (!nextKey) return;

        if (getViewerStructureKey() !== nextKey) {
            setViewerStructureKey(nextKey);
            error_message.value = "";

            const options = {};
            // Changing structure: revoke any previous object URL now.
            revokeViewerObjectUrl();

            if (nextKey.startsWith("file:")) {
                const fileName = nextKey.slice("file:".length);
                const f = (files.value || []).find((x) => x?.name === fileName);
                if (!f) return;
                const url = URL.createObjectURL(f);
                setViewerLastObjectUrl(url);
                options.customData = {
                    url,
                    format: molstarFormatFromFileName(f.name, getFormatFromFileName),
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
                    setViewerLastObjectUrl(url);
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

            const ok = await waitForStructureReady(viewerInstance, {
                maxTries: 20,
                intervalMs: 150,
            });
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
    { immediate: true },
);

async function triggerDownloadAll() {
    await downloadAllAsZip("extract_fragment_results.zip");
}
</script>

<template>
    <TaskLayout title="Extract Fragment" :processing="processing" :runButtonText="run_button_text" :errorMessage="error_message" :fileErrors="file_errors" :isResultsView="is_results_view" :hasResults="has_results" @submit="runExtractFragment">
        <template #viewer>
            <div class="flex items-center justify-between gap-3">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Structure</p>
                <div class="text-sm font-semibold text-gray-700 dark:text-gray-300">
                    {{ current_title }}
                </div>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div class="w-full rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden" style="height: 720px; position: relative">
                <div ref="viewerContainer" class="w-full h-full" style="height: 100%; width: 100%; position: relative"></div>
            </div>
        </template>

        <template #results>
            <div class="flex items-center justify-between gap-3">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Results</p>
                <button v-if="can_download_all" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_all" @click="triggerDownloadAll">Download All (ZIP)</button>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div class="rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800 overflow-y-auto max-h-[720px]">
                <div class="space-y-4">
                    <div v-for="g in grouped_results" :key="g.source">
                        <div class="mb-2 text-sm font-semibold text-gray-900 dark:text-gray-200">
                            {{ g.source }}
                        </div>
                        <ul class="space-y-2">
                            <li v-for="r in g.items" :key="r.filename" class="flex items-center justify-between rounded-lg px-2 py-2 cursor-pointer" :class="isSelected(r) ? 'bg-blue-50 ring-2 ring-blue-200 dark:bg-blue-950/40 dark:ring-blue-800' : 'hover:bg-gray-50 dark:hover:bg-gray-700/40'" @click="selectResultItem(r)">
                                <div class="min-w-0">
                                    <div class="truncate text-sm font-medium text-gray-900 dark:text-gray-200">
                                        {{ r.filename }}
                                    </div>
                                    <div class="text-xs text-gray-500 dark:text-gray-300">key: {{ r.key }} · {{ (r.size / 1024).toFixed(2) }} KB</div>
                                </div>
                                <a class="rounded-lg border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :href="r.url" :download="r.filename" @click.stop> Download </a>
                            </li>
                        </ul>
                    </div>
                </div>
            </div>
        </template>

        <template #input>
            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" :max-files="200" :max-size="500 * 1024 * 1024" />
        </template>

        <template #custom-params>
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
        </template>
    </TaskLayout>
</template>
