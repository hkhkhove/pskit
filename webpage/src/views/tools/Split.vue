<script setup>
import { ref, computed, watch, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import InputStructure from "../../components/InputStructure.vue";
import TaskLayout from "../../components/TaskLayout.vue";
import { splitComplexInWorker, splitByChainInWorker, workerChunksToDownloadItems, stripExtension, getFormatFromFileName, isValidPdbId } from "../../utils/wasmBatch.js";
import { renderPdbeMolstar, applySelectionWithRetry, waitForStructureReady, molstarFormatFromPskitFormat, createBlobUrlFromBytes } from "../../utils/pdbeMolstar.js";
import { pdbIdFromSource, molstarFormatFromFileName, inferChainSelectorFromStructureText } from "../../utils/structureUtils.js";
import { useMolstar, MOLSTAR_COLORS } from "../../composables/useMolstar.js";
import { useBatchTask } from "../../composables/useBatchTask.js";

const route = useRoute();
const router = useRouter();

const { viewerContainer, initViewer, getViewerInstance, revokeViewerObjectUrl, idStructureCache, getViewerStructureKey, setViewerStructureKey, setViewerLastObjectUrl } = useMolstar();
const { input_method, ids, files, processing, error_message, results, file_errors, last_run_input_method, is_results_view, has_results, grouped_results, can_download_all, run_button_text, executeBatchTask, downloadAllAsZip } = useBatchTask();

const split_type = ref("chain");
const selected_result = ref(null);

async function triggerDownloadAll() {
    const zipName = split_type.value === "chain" ? "split_by_chain_results.zip" : "split_results.zip";
    await downloadAllAsZip(zipName);
}

async function runSplit() {
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
            if (split_type.value === "mol_type") {
                return splitComplexInWorker(bytesForWorker, input.format);
            }
            return splitByChainInWorker(bytesForWorker, input.format);
        },
        toDownloadItems: (result, input) =>
            workerChunksToDownloadItems({
                items: result.items,
                base: input.base,
                format: input.format,
                source: input.source,
            }),
    });
}

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
    const viewerInstance = getViewerInstance();
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
    const data = selector.ids.map((id) => ({
        [selector.field]: id,
        color: MOLSTAR_COLORS.highlight,
    }));
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
                error_message.value = "Failed to load structure in viewer.";
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

watch(
    () => is_results_view.value,
    async (v) => {
        if (!v) return;
        if (!has_results.value) return;
        await nextTick(); // 等 DOM 更新，确保 viewerContainer 可用
        if (!selected_result.value) {
            selected_result.value = grouped_results.value?.[0]?.items?.[0] ?? null;
        }
        await renderMolstarForSelected();
    },
    { flush: "post" },
);

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
    { immediate: true }, // 带 immediate：变量“在不在”都先执行一次，以后“变了”再执行
);
</script>
<template>
    <TaskLayout title="Split Complex" :processing="processing" :runButtonText="run_button_text" :errorMessage="error_message" :fileErrors="file_errors" :isResultsView="is_results_view" :hasResults="has_results" @submit="runSplit">
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

            <div class="rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800 max-h-screen overflow-y-auto">
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
        </template>
    </TaskLayout>
</template>
