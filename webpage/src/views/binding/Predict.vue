<script setup>
import { ref, computed, watch, nextTick } from "vue";
import JSZip from "jszip";
import InputStructure from "../../components/InputStructure.vue";
import TaskStatus from "../../components/TaskStatus.vue";
import TaskLayout from "../../components/TaskLayout.vue";
import { renderPdbeMolstar, applySelectionWithRetry, highlightResidues, waitForStructureReady } from "../../utils/pdbeMolstar.js";
import { useMolstar, MOLSTAR_COLORS as BASE_COLORS } from "../../composables/useMolstar.js";
import { useRemoteTask } from "../../composables/useRemoteTask.js";

const { task_id, input_method, ids, files, isLoading, submissionError, showResults, resultFiles, errorItems, is_results_view, is_task_view, submitTask, handleTaskCompleted: handleBaseTaskCompleted, handleTaskFailed, downloadAllAsZip } = useRemoteTask("pred_bs");

const { viewerContainer, initViewer, getViewerInstance, revokeViewerObjectUrl, setViewerLastObjectUrl } = useMolstar();

const ligand_type = ref("DNA");
const selectedRowIndex = ref(-1);
const structureFile = ref(null);
const current_index = ref(0);
const parsedResultsAll = ref([]);
const parsedPredictions = ref([]);
const isStructureLoading = ref(false);
const structureBlobCache = new Map();

const MOLSTAR_COLORS = {
    ...BASE_COLORS,
    binding: { r: 231, g: 76, b: 60 },
    nonBinding: { r: 52, g: 152, b: 219 },
};

const has_results = computed(() => parsedResultsAll.value.length > 0);
const has_multiple_results = computed(() => parsedResultsAll.value.length > 1);

const current_result = computed(() => {
    if (!has_results.value) return null;
    const i = Math.min(Math.max(0, current_index.value), parsedResultsAll.value.length - 1);
    return parsedResultsAll.value[i] ?? null;
});

const current_title = computed(() => {
    if (!current_result.value) return "";
    const i = Math.min(Math.max(0, current_index.value), parsedResultsAll.value.length - 1);
    return `${current_result.value.baseName} (${i + 1}/${parsedResultsAll.value.length})`;
});

const bindingSiteCount = computed(() => current_result.value?.predictions.filter((r) => r.prediction === 1).length || 0);

async function switchToCurrentResult() {
    const res = current_result.value;
    if (!res) return;
    parsedPredictions.value = res.predictions;
    structureFile.value = res.structureFile;
    await nextTick();
    await renderStructureWithPredictions();
}

function nextTable() {
    if (!has_multiple_results.value) return;
    current_index.value = (current_index.value + 1) % parsedResultsAll.value.length;
    selectedRowIndex.value = -1;
    switchToCurrentResult();
}

function downloadTextFile({ text, filename }) {
    const blob = new Blob([text], { type: "text/csv;charset=utf-8;" });
    const link = document.createElement("a");
    const url = URL.createObjectURL(blob);
    link.setAttribute("href", url);
    link.setAttribute("download", filename);
    link.style.visibility = "hidden";
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
}

function uniqueZipName(filename, usedSet) {
    let safeName = filename;
    let counter = 1;
    while (usedSet.has(safeName)) {
        safeName = `${filename.replace(/\.csv$/, "")}_${counter}.csv`;
        counter++;
    }
    usedSet.add(safeName);
    return safeName;
}

function downloadCurrentTable() {
    if (!current_result.value) return;
    const predictions = current_result.value.predictions;
    const header = "chain,residue_number,residue_name,score,prediction";
    const rows = predictions.map((r) => `${r.chain},${r.resNum},${r.resName},${r.score.toFixed(4)},${r.prediction}`);
    const text = [header, ...rows].join("\n") + "\n";
    downloadTextFile({ text, filename: `${current_result.value.baseName}_binding_sites.csv` });
}

async function triggerDownloadAll() {
    if (!has_multiple_results.value) {
        downloadCurrentTable();
        return;
    }
    try {
        const zip = new JSZip();
        const used = new Set();
        for (const res of parsedResultsAll.value || []) {
            const filename = uniqueZipName(`${res.baseName}_binding_sites.csv`, used);
            const header = "chain,residue_number,residue_name,score,prediction";
            const rows = res.predictions.map((r) => `${r.chain},${r.resNum},${r.resName},${r.score.toFixed(4)},${r.prediction}`);
            zip.file(filename, [header, ...rows].join("\n") + "\n");
        }
        const blob = await zip.generateAsync({ type: "blob" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "binding_sites_predictions.zip";
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
    } catch (e) {
        console.error(e);
    }
}

async function parseSingleCsv(file) {
    try {
        const response = await fetch(file.download_url);
        const text = await response.text();
        const lines = text.trim().split("\n");
        if (lines.length < 2) return [];
        const header = lines[0].split(",").map((h) => h.trim().toLowerCase());
        const predictions = [];
        for (let i = 1; i < lines.length; i++) {
            const values = lines[i].split(",").map((v) => v.trim());
            const row = {};
            header.forEach((h, idx) => {
                row[h] = values[idx] || "";
            });
            const score = parseFloat(row.score || row.probability || row.prob || "0");
            predictions.push({
                chain: row.chain || row.chain_id || "-",
                resNum: parseInt(row.residue_number || row.resi || row.resnum || "0") || i,
                resName: row.residue_name || row.resn || row.aa || "-",
                score: score,
                prediction: score >= 0.4 ? 1 : 0,
            });
        }
        return predictions;
    } catch (e) {
        return [];
    }
}

async function handleTaskCompleted(data) {
    await handleBaseTaskCompleted(data);
    const csvFiles = resultFiles.value.filter((f) => f.filename.endsWith(".csv"));
    const structureFiles = resultFiles.value.filter((f) => f.filename.endsWith(".pdb") || f.filename.endsWith(".cif"));
    const allResults = [];
    for (const csvFile of csvFiles) {
        const baseName = csvFile.filename.replace(/_binding_sites\.csv$/, "");
        const matchingStructure = structureFiles.find((s) => s.filename.replace(/\.(pdb|cif)$/, "") === baseName);
        const predictions = await parseSingleCsv(csvFile);
        allResults.push({ baseName, csvFile, structureFile: matchingStructure || null, predictions });
    }
    parsedResultsAll.value = allResults;
    current_index.value = 0;
    if (allResults.length > 0) {
        parsedPredictions.value = allResults[0].predictions;
        structureFile.value = allResults[0].structureFile;
        await nextTick();
        await renderStructureWithPredictions();
    }
}

async function renderStructureWithPredictions() {
    if (!viewerContainer.value || parsedPredictions.value.length === 0) return;
    isStructureLoading.value = true;
    try {
        const viewerInstance = await initViewer();
        const options = {};
        if (structureFile.value) {
            revokeViewerObjectUrl();
            const cacheKey = structureFile.value.download_url;
            let blob = structureBlobCache.get(cacheKey);
            if (!blob) {
                const response = await fetch(cacheKey);
                blob = await response.blob();
                structureBlobCache.set(cacheKey, blob);
            }
            const url = URL.createObjectURL(blob);
            setViewerLastObjectUrl(url);
            options.customData = { url, format: structureFile.value.filename.endsWith(".cif") ? "mmcif" : "pdb", binary: false };
        } else if (input_method.value === "id" && ids.value) {
            const pdbId = ids.value.trim().split(/[,\s]+/)[0];
            if (pdbId?.length === 4) options.moleculeId = pdbId.toLowerCase();
        } else if (input_method.value === "file" && files.value.length > 0) {
            const f = files.value[0];
            revokeViewerObjectUrl();
            const url = URL.createObjectURL(f);
            setViewerLastObjectUrl(url);
            options.customData = { url, format: f.name.endsWith(".cif") ? "mmcif" : "pdb", binary: false };
        }

        if (!options.moleculeId && !options.customData) return;
        await renderPdbeMolstar(viewerInstance, viewerContainer.value, options);
        await waitForStructureReady(viewerInstance);

        const bindingResidues = parsedPredictions.value.filter((r) => r.prediction === 1).map((r) => ({ auth_asym_id: r.chain, auth_residue_number: r.resNum, color: MOLSTAR_COLORS.binding }));

        if (bindingResidues.length > 0) {
            await applySelectionWithRetry(viewerInstance, { data: bindingResidues, nonSelectedColor: MOLSTAR_COLORS.nonSelected, focus: false, keepRepresentations: true });
        }
    } catch (e) {
        console.error(e);
    } finally {
        isStructureLoading.value = false;
    }
}

async function focusResidue(row, index) {
    selectedRowIndex.value = index;
    const viewerInstance = getViewerInstance();
    if (!viewerInstance) return;
    try {
        await highlightResidues(viewerInstance, { data: [{ auth_asym_id: row.chain, auth_residue_number: row.resNum }], color: MOLSTAR_COLORS.focus, focus: true });
    } catch (e) {
        console.error(e);
    }
}

async function handleSubmit() {
    structureBlobCache.clear();
    parsedResultsAll.value = [];
    current_index.value = 0;
    selectedRowIndex.value = -1;
    await submitTask({ ligand_type: ligand_type.value });
}
</script>
<template>
    <TaskLayout title="Predict Binding Sites" :processing="isLoading" :errorMessage="submissionError" :isTaskView="is_task_view" :isResultsView="is_results_view" :hasResults="showResults" @submit="handleSubmit">
        <template #input>
            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" />
        </template>

        <template #custom-params>
            <div class="my-4">
                <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Ligand Type</span>
            </div>
            <div>
                <ul class="w-full items-center rounded-lg border border-gray-300 bg-white text-sm font-medium text-gray-900 sm:flex dark:border-gray-600 dark:bg-gray-700 dark:text-white">
                    <li class="w-full border-b border-gray-300 sm:border-r sm:border-b-0 dark:border-gray-600">
                        <div class="flex items-center ps-3">
                            <input id="DNA" type="radio" value="DNA" v-model="ligand_type" class="h-4 w-4 accent-blue-600" />
                            <label for="DNA" class="ms-2 w-full py-3">
                                <span class="text-sm font-medium text-gray-900 dark:text-gray-300">DNA</span>
                                <span class="text-xs text-gray-500 dark:text-gray-400 block">Predict DNA-binding residues</span>
                            </label>
                        </div>
                    </li>
                    <li class="w-full border-gray-300 dark:border-gray-600">
                        <div class="flex items-center ps-3">
                            <input id="RNA" type="radio" value="RNA" v-model="ligand_type" class="h-4 w-4 accent-blue-600" />
                            <label for="RNA" class="ms-2 w-full py-3">
                                <span class="text-sm font-medium text-gray-900 dark:text-gray-300">RNA</span>
                                <span class="text-xs text-gray-500 dark:text-gray-400 block">Predict RNA-binding residues</span>
                            </label>
                        </div>
                    </li>
                </ul>
                <ul class="mt-4 text-sm dark:text-gray-500 space-y-1.5 list-disc list-inside">
                    <li class="text-xs text-gray-500 dark:text-gray-300">Input files must contain a single protein chain. <span class="font-bold">If multiple chains are present, only the first one will be processed.</span></li>
                    <li class="text-xs text-gray-500 dark:text-gray-300">Output CSV file contains binding scores for each residue.</li>
                    <li class="text-xs text-gray-500 dark:text-gray-300">Prediction requires extracting features first, which may take some time.</li>
                </ul>
            </div>
        </template>

        <template #status>
            <TaskStatus :task-id="task_id" task-name="Binding Site Prediction" @completed="handleTaskCompleted" @failed="handleTaskFailed" />
        </template>

        <template #viewer>
            <div class="flex items-center justify-between gap-3">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Structure</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />
            <div v-if="parsedPredictions.length > 0" class="w-full h-[720px] relative rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
                <div ref="viewerContainer" class="w-full h-full relative z-50"></div>
                <div v-if="isStructureLoading" class="absolute inset-0 z-[60] flex items-center justify-center bg-white/80 dark:bg-gray-900/80 backdrop-blur-sm">
                    <div class="flex flex-col items-center gap-3">
                        <svg class="animate-spin h-10 w-10 text-blue-600" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                        </svg>
                        <span class="text-sm font-medium text-gray-700 dark:text-gray-300">Loading structure...</span>
                    </div>
                </div>
            </div>
            <div v-if="parsedPredictions.length > 0" class="mt-4 flex items-center justify-center gap-4 text-sm">
                <div class="flex items-center gap-2">
                    <div class="w-4 h-4 rounded" style="background-color: rgb(231, 76, 60)"></div>
                    <span class="text-gray-700 dark:text-gray-300">Binding Site</span>
                </div>
                <div class="flex items-center gap-2">
                    <div class="w-4 h-4 rounded" style="background-color: rgb(190, 190, 190)"></div>
                    <span class="text-gray-700 dark:text-gray-300">Non-binding</span>
                </div>
                <div class="flex items-center gap-2">
                    <div class="w-4 h-4 rounded" style="background-color: rgb(255, 235, 59)"></div>
                    <span class="text-gray-700 dark:text-gray-300">Selected</span>
                </div>
            </div>
        </template>

        <template #results>
            <div class="flex items-center justify-between gap-3">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Predictions</p>
                <div class="flex items-center gap-2">
                    <button v-if="has_multiple_results" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="isLoading" @click="nextTable">Next</button>
                    <button class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="isLoading" @click="triggerDownloadAll">Download All</button>
                </div>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />
            <div v-if="parsedPredictions.length > 0" class="flex flex-col h-[720px] rounded-lg border border-gray-200 dark:border-gray-700">
                <div class="flex justify-between items-center mb-2 px-3 pt-3">
                    <div class="space-y-1">
                        <div class="text-sm font-semibold text-gray-900 dark:text-gray-200">{{ current_title }}</div>
                        <div class="text-xs text-gray-500 dark:text-gray-300">{{ bindingSiteCount }} binding sites found</div>
                    </div>
                    <button class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="isLoading" @click="downloadCurrentTable">Download (CSV)</button>
                </div>
                <div class="max-h-screen overflow-y-auto">
                    <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                        <thead class="bg-gray-100 dark:bg-gray-700 sticky top-0 z-10">
                            <tr>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">#</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Chain</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Residue</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">AA</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Score</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Prediction</th>
                            </tr>
                        </thead>
                        <tbody class="divide-y divide-gray-200 dark:divide-gray-700">
                            <tr v-for="(r, idx) in parsedPredictions" :key="idx" class="cursor-pointer transition-colors" :class="idx === selectedRowIndex ? 'bg-blue-50 dark:bg-blue-900/30 ring-2 ring-blue-400/60 ring-inset' : r.prediction === 1 ? 'bg-red-50 dark:bg-red-900/20 hover:bg-red-100 dark:hover:bg-red-900/30' : 'bg-white dark:bg-gray-800 hover:bg-gray-50 dark:hover:bg-gray-700'" @click="focusResidue(r, idx)">
                                <td class="px-4 py-2 text-xs text-gray-700 dark:text-gray-300">{{ idx + 1 }}</td>
                                <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-200">{{ r.chain }}</td>
                                <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ r.resNum }}</td>
                                <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-200">{{ r.resName }}</td>
                                <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">
                                    <div class="flex items-center gap-2">
                                        <div class="w-16 h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden"><div class="h-full rounded-full" :class="r.score > 0.4 ? 'bg-red-500' : 'bg-blue-500'" :style="{ width: `${Math.max(0, Math.min(1, r.score)) * 100}%` }"></div></div>
                                        <span>{{ r.score.toFixed(3) }}</span>
                                    </div>
                                </td>
                                <td class="px-4 py-2 text-xs"><span v-if="r.prediction === 1" class="px-2 py-1 rounded-full bg-red-100 text-red-800 dark:bg-red-900/50 dark:text-red-300"> Binding </span><span v-else class="px-2 py-1 rounded-full bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-400"> Non-binding </span></td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
            <div v-if="Object.keys(errorItems).length > 0" class="mt-6">
                <h4 class="text-lg font-semibold text-red-600 dark:text-red-400 mb-3">Processing Errors</h4>
                <div class="space-y-2">
                    <div v-for="(message, filename) in errorItems" :key="filename" class="p-3 bg-red-50 border border-red-300 rounded-lg dark:bg-red-900/20 dark:border-red-800">
                        <p class="text-sm text-red-800 dark:text-red-300">
                            <span class="font-semibold">{{ filename }}:</span> {{ message }}
                        </p>
                    </div>
                </div>
            </div>
        </template>
    </TaskLayout>
</template>
