<script setup>
import { ref, computed, watch, nextTick, onBeforeUnmount } from "vue";
import { useRoute, useRouter } from "vue-router";
import { nanoid } from "nanoid";
import JSZip from "jszip";
import InputStructure from "../../components/InputStructure.vue";
import TaskResult from "../../components/TaskResult.vue";
import { ensurePdbeMolstarLoaded, createPdbeMolstarViewer, renderPdbeMolstar, applySelectionWithRetry, highlightResidues, waitForStructureReady } from "../../utils/pdbeMolstar.js";

const route = useRoute();
const router = useRouter();

const task_id = ref("");
const task_name = "pred_bs";

const input_method = ref("id");
const inputRef = ref(null);
const ids = ref("");
const files = ref([]);

const ligand_type = ref("DNA");

const isLoading = ref(false);
const submissionError = ref(null);

// Result display state - only shown after completion
const showResults = ref(false);
const resultFiles = ref([]);
const errorItems = ref({});
const parsedPredictions = ref([]);
const selectedRowIndex = ref(-1);
const structureFile = ref(null);

// Multi-result support
const current_index = ref(0);
const parsedResultsAll = ref([]); // Array of { baseName, csvFile, structureFile, predictions }

// Molstar viewer
const viewerContainer = ref(null);
let viewerInstance = null;
let viewerObjectUrl = null;
const isStructureLoading = ref(false);

// Structure file cache: Map<download_url, Blob>
const structureBlobCache = new Map();

const MOLSTAR_COLORS = {
    nonSelected: { r: 190, g: 190, b: 190 },
    binding: { r: 231, g: 76, b: 60 },
    nonBinding: { r: 52, g: 152, b: 219 },
    focus: { r: 255, g: 235, b: 59 },
};

const is_results_view = computed(() => route.query.view === "results" && task_id.value);

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

const bindingSites = computed(() => {
    if (!current_result.value) return [];
    return current_result.value.predictions.filter((r) => r.prediction === 1);
});

const bindingSiteCount = computed(() => bindingSites.value.length);

const can_next_table = computed(() => has_multiple_results.value);
const can_download_table = computed(() => !!current_result.value);
const can_download_all_tables = computed(() => has_results.value);

function nextTable() {
    if (!has_multiple_results.value) return;
    current_index.value = (current_index.value + 1) % parsedResultsAll.value.length;
    selectedRowIndex.value = -1;
    // Re-render structure and apply coloring for current result
    switchToCurrentResult();
}

async function switchToCurrentResult() {
    const res = current_result.value;
    if (!res) return;
    parsedPredictions.value = res.predictions;
    structureFile.value = res.structureFile;
    await nextTick();
    await renderStructureWithPredictions();
}

function downloadCurrentTable() {
    if (!current_result.value) return;
    const predictions = current_result.value.predictions;
    const header = "chain,residue_number,residue_name,score,prediction";
    const rows = predictions.map((r) => `${r.chain},${r.resNum},${r.resName},${r.score.toFixed(4)},${r.prediction}`);
    const text = [header, ...rows].join("\n") + "\n";
    const filename = `${current_result.value.baseName}_binding_sites.csv`;
    downloadTextFile({ text, filename });
}

function downloadTextFile({ text, filename }) {
    const blob = new Blob([text], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
}

async function downloadAllTablesZip() {
    if (!can_download_all_tables.value) return;
    try {
        const zip = new JSZip();
        const used = new Set();

        for (const res of parsedResultsAll.value || []) {
            const filename = `${res.baseName}_binding_sites.csv`;
            const safeName = uniqueZipName(filename, used);
            const header = "chain,residue_number,residue_name,score,prediction";
            const rows = res.predictions.map((r) => `${r.chain},${r.resNum},${r.resName},${r.score.toFixed(4)},${r.prediction}`);
            const text = [header, ...rows].join("\n") + "\n";
            zip.file(safeName, text);
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
        console.error("Error creating ZIP:", e);
    }
}

function uniqueZipName(name, usedSet) {
    if (!usedSet.has(name)) {
        usedSet.add(name);
        return name;
    }
    let i = 1;
    const dot = name.lastIndexOf(".");
    const base = dot > 0 ? name.slice(0, dot) : name;
    const ext = dot > 0 ? name.slice(dot) : "";
    while (usedSet.has(`${base}_${i}${ext}`)) i++;
    const unique = `${base}_${i}${ext}`;
    usedSet.add(unique);
    return unique;
}

async function renderStructureWithPredictions() {
    if (!viewerContainer.value) return;
    if (parsedPredictions.value.length === 0) return;

    isStructureLoading.value = true;
    try {
        await ensurePdbeMolstarLoaded();
        await nextTick();

        if (!viewerInstance) {
            viewerInstance = createPdbeMolstarViewer();
        }

        const options = {};

        // Try to load structure
        if (structureFile.value) {
            // Use result structure file (with caching)
            revokeViewerObjectUrl();
            const cacheKey = structureFile.value.download_url;
            let blob = structureBlobCache.get(cacheKey);
            if (!blob) {
                // Fetch and cache
                const response = await fetch(cacheKey);
                blob = await response.blob();
                structureBlobCache.set(cacheKey, blob);
            }
            viewerObjectUrl = URL.createObjectURL(blob);
            options.customData = {
                url: viewerObjectUrl,
                format: structureFile.value.filename.endsWith(".cif") ? "mmcif" : "pdb",
                binary: false,
            };
        } else if (input_method.value === "id" && ids.value) {
            // Use PDB ID
            const pdbId = ids.value.trim().split(/[,\s]+/)[0];
            if (pdbId && pdbId.length === 4) {
                options.moleculeId = pdbId.toLowerCase();
            }
        } else if (input_method.value === "file" && files.value.length > 0) {
            // Use uploaded file
            const f = files.value[0];
            revokeViewerObjectUrl();
            viewerObjectUrl = URL.createObjectURL(f);
            options.customData = {
                url: viewerObjectUrl,
                format: f.name.endsWith(".cif") ? "mmcif" : "pdb",
                binary: false,
            };
        }

        if (!options.moleculeId && !options.customData) return;

        await renderPdbeMolstar(viewerInstance, viewerContainer.value, options);

        const ready = await waitForStructureReady(viewerInstance, { maxTries: 20, intervalMs: 150 });
        if (!ready) {
            console.warn("Structure may not be fully loaded");
        }

        // Color residues by prediction
        const bindingResidues = parsedPredictions.value
            .filter((r) => r.prediction === 1)
            .map((r) => ({
                auth_asym_id: r.chain,
                auth_residue_number: r.resNum,
                color: MOLSTAR_COLORS.binding,
            }));

        if (bindingResidues.length > 0) {
            await applySelectionWithRetry(viewerInstance, {
                data: bindingResidues,
                nonSelectedColor: MOLSTAR_COLORS.nonSelected,
                focus: false,
                keepRepresentations: true,
            });
        }
    } catch (e) {
        console.error("Error rendering structure:", e);
    } finally {
        isStructureLoading.value = false;
    }
}

async function focusResidue(row, index) {
    selectedRowIndex.value = index;

    if (!viewerInstance) return;

    try {
        await highlightResidues(viewerInstance, {
            data: [{ auth_asym_id: row.chain, auth_residue_number: row.resNum }],
            color: MOLSTAR_COLORS.focus,
            focus: true,
        });
    } catch (e) {
        console.error("Error focusing residue:", e);
    }
}

function revokeViewerObjectUrl() {
    if (viewerObjectUrl) {
        try {
            URL.revokeObjectURL(viewerObjectUrl);
        } catch {}
        viewerObjectUrl = null;
    }
}

async function handleSubmit() {
    if (input_method.value === "file") {
        if (files.value.length === 0) {
            submissionError.value = "Please upload at least one structure file (.pdb or .cif).";
            return;
        }
    } else if (input_method.value === "id") {
        if (ids.value.length === 0) {
            submissionError.value = "Please enter at least one PDB ID (separated by commas).";
            return;
        }
        if (!inputRef.value.ids_valid) {
            submissionError.value = "PDB ID format is incorrect: must be 4 alphanumeric characters (separated by commas).";
            return;
        }
    } else {
        submissionError.value = "Please select an input method (ID or file).";
        return;
    }

    isLoading.value = true;
    submissionError.value = null;

    // Reset result state for new submission
    showResults.value = false;
    resultFiles.value = [];
    errorItems.value = {};
    parsedPredictions.value = [];
    selectedRowIndex.value = -1;
    structureFile.value = null;
    parsedResultsAll.value = [];
    current_index.value = 0;
    structureBlobCache.clear();

    task_id.value = nanoid();

    const formData = new FormData();
    formData.append("task_id", task_id.value);
    formData.append("task_name", task_name);
    formData.append("input_method", input_method.value);
    formData.append("ids", ids.value);
    formData.append("ligand_type", ligand_type.value);

    if (files.value.length > 0) {
        files.value.forEach((file) => formData.append("files", file));
    }

    try {
        const response = await fetch("/api/tasks", {
            method: "POST",
            body: formData,
        });

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`[${response.status}]: ${errorText || "Unknown error"}`);
        }

        const data = await response.json();

        // 包含 task_id，用户可保存链接稍后查看
        const q = { ...route.query, view: "results", task_id: task_id.value };
        await router.push({ query: q });
    } catch (error) {
        submissionError.value = error.message;
        task_id.value = "";
    } finally {
        isLoading.value = false;
    }
}

async function handleTaskCompleted({ files, errorItems: errors }) {
    resultFiles.value = files.filter((f) => f.filename !== "error.json");
    errorItems.value = errors || {};

    // Group files by base name: each structure file pairs with its CSV
    const csvFiles = resultFiles.value.filter((f) => f.filename.endsWith(".csv"));
    const structureFiles = resultFiles.value.filter((f) => f.filename.endsWith(".pdb") || f.filename.endsWith(".cif"));

    // Parse all CSV files and match with structures
    const allResults = [];
    for (const csvFile of csvFiles) {
        const baseName = csvFile.filename.replace(/_binding_sites\.csv$/, "");
        const matchingStructure = structureFiles.find((s) => {
            const structBase = s.filename.replace(/\.(pdb|cif)$/, "");
            return structBase === baseName;
        });

        const predictions = await parseSingleCsv(csvFile);
        allResults.push({
            baseName,
            csvFile,
            structureFile: matchingStructure || null,
            predictions,
        });
    }

    parsedResultsAll.value = allResults;
    current_index.value = 0;

    if (allResults.length > 0) {
        // Set current display data
        parsedPredictions.value = allResults[0].predictions;
        structureFile.value = allResults[0].structureFile;
        showResults.value = true;
        await nextTick();
        await renderStructureWithPredictions();
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

            // Parse common fields - CSV format: chain,residue_number,residue_name,score
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
        console.error("Error parsing CSV:", e);
        return [];
    }
}
function handleTaskFailed(error) {
    // Stay on TaskResult view to show error
    showResults.value = false;
}
// 从 URL 恢复 task_id，支持用户保存链接稍后查看
watch(
    () => route.query,
    (query) => {
        if (query.view === "results" && query.task_id) {
            // 从 URL 恢复 task_id
            task_id.value = query.task_id;
        } else if (query.view === "results" && !query.task_id && !task_id.value) {
            // 没有 task_id，返回表单视图
            const q = { ...route.query };
            delete q.view;
            router.replace({ query: q });
        }
    },
    { immediate: true }
);

onBeforeUnmount(() => {
    revokeViewerObjectUrl();
    try {
        viewerInstance?.clear?.();
    } catch {}
    viewerInstance = null;
});
</script>
<template>
    <div class="mx-auto py-8 px-4" :class="showResults && is_results_view ? 'max-w-full' : 'max-w-3xl'">
        <!-- Results View with 3D Viewer (after completion) -->
        <div v-if="is_results_view && showResults" class="grid grid-cols-1 gap-6 lg:grid-cols-2">
            <!-- Left: Structure Viewer -->
            <div class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Structure</p>
                </div>
                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <!-- Mol* Viewer -->
                <div v-if="parsedPredictions.length > 0" class="w-full h-[720px] relative rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
                    <div ref="viewerContainer" class="w-full h-full relative z-50"></div>
                    <!-- Loading overlay -->
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

                <!-- Color Legend -->
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
            </div>

            <!-- Right: Prediction Table -->
            <div class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Predictions</p>
                    <div class="flex items-center gap-2">
                        <button v-if="has_multiple_results" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_next_table" @click="nextTable">Next</button>
                        <button v-if="has_multiple_results" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_all_tables" @click="downloadAllTablesZip">Download All (ZIP)</button>
                    </div>
                </div>

                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <!-- Prediction Table -->
                <div v-if="parsedPredictions.length > 0" class="flex flex-col h-[720px] rounded-lg border border-gray-200 dark:border-gray-700">
                    <div class="flex justify-between items-center mb-2 px-3 pt-3">
                        <div class="space-y-1">
                            <div class="text-sm font-semibold text-gray-900 dark:text-gray-200">{{ current_title }}</div>
                            <div class="text-xs text-gray-500 dark:text-gray-300">{{ bindingSiteCount }} binding sites found</div>
                        </div>
                        <button class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_table" @click="downloadCurrentTable">Download (CSV)</button>
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
                                            <div class="w-16 h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
                                                <div class="h-full rounded-full" :class="r.score > 0.4 ? 'bg-red-500' : 'bg-blue-500'" :style="{ width: `${Math.max(0, Math.min(1, r.score)) * 100}%` }"></div>
                                            </div>
                                            <span>{{ r.score.toFixed(3) }}</span>
                                        </div>
                                    </td>
                                    <td class="px-4 py-2 text-xs">
                                        <span v-if="r.prediction === 1" class="px-2 py-1 rounded-full bg-red-100 text-red-800 dark:bg-red-900/50 dark:text-red-300"> Binding </span>
                                        <span v-else class="px-2 py-1 rounded-full bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-400"> Non-binding </span>
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>

        <!-- Error Items from error.json -->
        <div v-if="Object.keys(errorItems).length > 0 && is_results_view && showResults" class="m-4">
            <h4 class="text-lg font-semibold text-red-600 dark:text-red-400 mb-3">Processing Errors</h4>
            <div class="space-y-2">
                <div v-for="(message, filename) in errorItems" :key="filename" class="p-3 bg-red-50 border border-red-300 rounded-lg dark:bg-red-900/20 dark:border-red-800">
                    <p class="text-sm text-red-800 dark:text-red-300">
                        <span class="font-semibold">{{ filename }}:</span> {{ message }}
                    </p>
                </div>
            </div>
        </div>

        <!-- Task Status View (pending/processing/failed - uses TaskResult component) -->
        <TaskResult v-else-if="is_results_view && !showResults" :task-id="task_id" task-name="Binding Site Prediction" @completed="handleTaskCompleted" @failed="handleTaskFailed" />

        <!-- Form View -->
        <form v-if="!is_results_view" @submit.prevent="handleSubmit" class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Predict Binding Sites</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <InputStructure ref="inputRef" v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" />

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <!-- Ligand Type Selection -->
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
                    <li class="my-1 text-xs font-normal text-gray-500 dark:text-gray-300">Input files must contain a single protein chain. <span class="border-b-1 font-bold">If multiple chains are present, only the first one will be processed.</span></li>
                    <li class="my-1 text-xs font-normal text-gray-500 dark:text-gray-300">Output CSV file contains binding scores for each residue.</li>
                    <li class="my-1 text-xs font-normal text-gray-500 dark:text-gray-300">Prediction requires extracting features first, which may take some time.</li>
                </ul>
            </div>

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <!-- Submit Button -->
            <div class="flex items-center gap-3">
                <button type="submit" :disabled="isLoading" :class="{ 'cursor-not-allowed opacity-50': isLoading }" class="w-full rounded-lg bg-blue-600 px-4 py-2 text-lg text-center font-medium text-white hover:bg-blue-700">
                    <span v-if="isLoading">Submitting...</span>
                    <span v-else>Submit</span>
                </button>
            </div>

            <div v-if="submissionError" class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                {{ submissionError }}
            </div>
        </form>
    </div>
</template>
