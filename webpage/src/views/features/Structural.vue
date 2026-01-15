<script setup>
import { ref, computed, watch, onBeforeUnmount } from "vue";
import { useRoute, useRouter } from "vue-router";
import { nanoid } from "nanoid";
import InputStructure from "../../components/InputStructure.vue";
import TaskResult from "../../components/TaskResult.vue";
import JSZip from "jszip";

const route = useRoute();
const router = useRouter();

const task_id = ref("");
const task_name = "emp_feats";

const input_method = ref("id");
const inputRef = ref(null);
const ids = ref("");
const files = ref([]);

const emp_feats = ref(["dssp"]);
const rosetta_relax = ref(false);

const isLoading = ref(false);
const submissionError = ref(null);

// Result display state - only shown after completion
const showResults = ref(false);
const resultFiles = ref([]);
const errorItems = ref({});
const selectedFile = ref(null);
const fileContent = ref("");
const parsedDssp = ref([]);
const parsedRosetta = ref({});

const is_results_view = computed(() => route.query.view === "results" && task_id.value);

const featureDescriptions = {
    dssp: {
        name: "DSSP Secondary Structure",
        description: "Extract secondary structure information from protein structures using the DSSP algorithm, including α-helices, β-sheets, turns, etc.",
    },
    rosetta: {
        name: "Rosetta Energy Scoring",
        description: "Score protein structures using the Rosetta energy function (ref2015) to evaluate the physical reasonableness of the structure.",
    },
};

function formatFileSize(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

async function selectFile(file) {
    selectedFile.value = file;
    fileContent.value = "";
    parsedDssp.value = [];
    parsedRosetta.value = {};

    try {
        const response = await fetch(file.download_url);
        if (!response.ok) {
            throw new Error(`Failed to fetch file content: ${response.status}`);
        }
        const text = await response.text();
        fileContent.value = text;

        // Parse based on file type
        if (file.filename.endsWith(".dssp")) {
            parseDsspFile(text);
        } else if (file.filename.endsWith("_score.txt") || file.filename === "score_relaxed.sc") {
            parseRosettaScore(text);
        }
    } catch (e) {
        console.error("Error fetching file content:", e);
    }
}

function parseDsspFile(text) {
    const lines = text.split("\n");
    const residues = [];
    let dataStarted = false;

    for (const line of lines) {
        // DSSP data starts after the line containing "  #  RESIDUE AA STRUCTURE"
        if (line.includes("#  RESIDUE AA STRUCTURE")) {
            dataStarted = true;
            continue;
        }
        if (!dataStarted || line.trim() === "") continue;
        if (line.length < 17) continue;

        try {
            const resNum = line.substring(0, 5).trim();
            const chain = line.substring(11, 12).trim();
            const aa = line.substring(13, 14).trim();
            const ss = line.substring(16, 17).trim() || "-";
            const acc = line.substring(35, 38).trim();

            if (resNum && aa) {
                residues.push({
                    resNum: parseInt(resNum) || resNum,
                    chain: chain || "-",
                    aa,
                    ss,
                    acc: parseInt(acc) || 0,
                });
            }
        } catch {
            continue;
        }
    }
    parsedDssp.value = residues;
}

function parseRosettaScore(text) {
    const lines = text.split("\n").filter((l) => l.trim());
    const scores = {};

    // Find header line (SCORE: with column names) and data line (SCORE: with values)
    let headerLine = null;
    let dataLine = null;

    for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed.startsWith("SCORE:")) {
            const parts = trimmed.split(/\s+/);
            // Header line has non-numeric values after SCORE:
            // Data line has numeric values
            if (parts.length > 1) {
                const firstValue = parts[1];
                // Check if this is the header line (first value is not a number)
                if (isNaN(parseFloat(firstValue))) {
                    headerLine = trimmed;
                } else {
                    dataLine = trimmed;
                }
            }
        }
    }

    if (headerLine && dataLine) {
        const headers = headerLine.split(/\s+/).slice(1); // Remove "SCORE:"
        const values = dataLine.split(/\s+/).slice(1); // Remove "SCORE:"

        for (let i = 0; i < headers.length && i < values.length; i++) {
            const header = headers[i];
            const value = values[i];
            scores[header] = value;
        }
    }

    parsedRosetta.value = scores;
}

async function downloadAllAsZip() {
    if (resultFiles.value.length === 0) return;

    try {
        const zip = new JSZip();
        for (const file of resultFiles.value) {
            const response = await fetch(file.download_url);
            const blob = await response.blob();
            zip.file(file.filename, blob);
        }
        const zipBlob = await zip.generateAsync({ type: "blob" });
        const url = URL.createObjectURL(zipBlob);
        const a = document.createElement("a");
        a.href = url;
        a.download = `empirical_features_${task_id.value}.zip`;
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
    } catch (e) {
        console.error("Error creating zip:", e);
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
    selectedFile.value = null;
    fileContent.value = "";
    parsedDssp.value = [];
    parsedRosetta.value = {};

    task_id.value = nanoid();

    const formData = new FormData();
    formData.append("task_id", task_id.value);
    formData.append("task_name", task_name);
    formData.append("input_method", input_method.value);
    formData.append("ids", ids.value);
    formData.append("emp_feats", emp_feats.value.join(","));
    formData.append("rosetta_relax", rosetta_relax.value ? "true" : "false");

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
    showResults.value = true;

    // Auto-select first file
    if (resultFiles.value.length > 0) {
        await selectFile(resultFiles.value[0]);
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
</script>

<template>
    <div class="mx-auto py-8 px-4" :class="showResults && is_results_view ? 'max-w-6xl' : 'max-w-3xl'">
        <!-- Results View with Data Visualization (after completion) -->
        <div v-if="is_results_view && showResults" class="grid grid-cols-1 gap-6 lg:grid-cols-2">
            <!-- Left: File Content / Table -->
            <div class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Result Preview</p>
                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <!-- DSSP Table -->
                <div v-if="selectedFile && parsedDssp.length > 0" class="overflow-hidden">
                    <h4 class="text-lg font-medium text-gray-900 dark:text-gray-200 mb-3">DSSP Secondary Structure <span class="text-[10px] font-semibold text-gray-500 dark:text-gray-400">[1]</span></h4>
                    <div class="max-h-[600px] overflow-y-auto rounded-lg border border-gray-200 dark:border-gray-700">
                        <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                            <thead class="bg-gray-50 dark:bg-gray-800 sticky top-0">
                                <tr>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Residue #</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Chain</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">AA</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">SS</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">ACC</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-gray-200 dark:divide-gray-700 bg-white dark:bg-gray-900">
                                <tr v-for="(r, idx) in parsedDssp" :key="idx" class="hover:bg-gray-50 dark:hover:bg-gray-800">
                                    <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ r.resNum }}</td>
                                    <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-200">{{ r.chain }}</td>
                                    <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-200">{{ r.aa }}</td>
                                    <td
                                        class="px-4 py-2 text-xs font-mono"
                                        :class="{
                                            'text-red-600 dark:text-red-400': r.ss === 'H',
                                            'text-blue-600 dark:text-blue-400': r.ss === 'E',
                                            'text-green-600 dark:text-green-400': r.ss === 'T' || r.ss === 'S',
                                            'text-gray-600 dark:text-gray-400': r.ss === '-' || r.ss === ' ',
                                        }">
                                        {{ r.ss }}
                                    </td>
                                    <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ r.acc }}</td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                    <div class="text-center mt-2 text-xs text-gray-500 dark:text-gray-400"><span class="text-red-600 dark:text-red-400">H</span>=α-helix, <span class="text-blue-600 dark:text-blue-400">E</span>=β-sheet, <span class="text-green-600 dark:text-green-400">T</span>=turn, <span class="text-green-600 dark:text-green-400">S</span>=bend, <span class="text-gray-600 dark:text-gray-400">-</span>=coil</div>
                </div>

                <!-- Rosetta Score Table -->
                <div v-else-if="selectedFile && Object.keys(parsedRosetta).length > 0" class="overflow-hidden">
                    <h4 class="text-lg font-medium text-gray-900 dark:text-gray-200 mb-3">Rosetta Energy Scores <span class="text-[10px] font-semibold text-gray-500 dark:text-gray-400">[2]</span></h4>
                    <div class="max-h-[600px] overflow-y-auto rounded-lg border border-gray-200 dark:border-gray-700">
                        <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                            <thead class="bg-gray-50 dark:bg-gray-800 sticky top-0">
                                <tr>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Term</th>
                                    <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Value</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-gray-200 dark:divide-gray-700 bg-white dark:bg-gray-900">
                                <tr v-for="(value, key) in parsedRosetta" :key="key" class="hover:bg-gray-50 dark:hover:bg-gray-800">
                                    <td class="px-4 py-2 text-xs font-medium text-gray-900 dark:text-gray-200">{{ key }}</td>
                                    <td class="px-4 py-2 text-xs font-mono text-gray-700 dark:text-gray-300">{{ value }}</td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </div>

                <!-- Raw File Content Fallback -->
                <div v-else-if="selectedFile && fileContent" class="overflow-hidden">
                    <h4 class="text-lg font-medium text-gray-900 dark:text-gray-200 mb-3">{{ selectedFile.filename }}</h4>
                    <div class="max-h-[600px] overflow-auto rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 p-4">
                        <pre class="text-xs font-mono text-gray-800 dark:text-gray-200 whitespace-pre">{{ fileContent }}</pre>
                    </div>
                </div>
            </div>

            <!-- Right: File List -->
            <div class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Files</p>
                    <button v-if="resultFiles.length > 1" @click="downloadAllAsZip" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600">Download All (ZIP)</button>
                </div>
                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <!-- File List -->
                <div v-if="resultFiles.length > 0" class="space-y-2">
                    <div v-for="file in resultFiles" :key="file.filename" class="flex items-center justify-between rounded-lg px-4 py-3 cursor-pointer transition-colors" :class="selectedFile?.filename === file.filename ? 'bg-blue-50 ring-2 ring-blue-200 dark:bg-blue-950/40 dark:ring-blue-800' : 'bg-gray-50 hover:bg-gray-100 dark:bg-gray-800 dark:hover:bg-gray-700'" @click="selectFile(file)">
                        <div class="min-w-0">
                            <div class="truncate text-sm font-medium text-gray-900 dark:text-gray-200">{{ file.filename }}</div>
                            <div class="text-xs text-gray-500 dark:text-gray-400">{{ formatFileSize(file.size) }}</div>
                        </div>
                        <a :href="file.download_url" download class="rounded-lg border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" @click.stop> Download </a>
                    </div>
                </div>

                <!-- Error Items from error.json -->
                <div v-if="Object.keys(errorItems).length > 0" class="m-4">
                    <h4 class="text-lg font-semibold text-red-600 dark:text-red-400 mb-3">Processing Errors</h4>
                    <div class="space-y-2">
                        <div v-for="(message, filename) in errorItems" :key="filename" class="p-3 bg-red-50 border border-red-300 rounded-lg dark:bg-red-900/20 dark:border-red-800">
                            <p class="text-sm text-red-800 dark:text-red-300">
                                <span class="font-semibold">{{ filename }}:</span> {{ message }}
                            </p>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <!-- Task Status View (pending/processing/failed - uses TaskResult component) -->
        <TaskResult v-else-if="is_results_view && !showResults" :task-id="task_id" task-name="Structural Feature Extraction" @completed="handleTaskCompleted" @failed="handleTaskFailed" />

        <!-- Form View -->
        <form v-else @submit.prevent="handleSubmit" class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Structural Feature Extraction</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <InputStructure ref="inputRef" v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" />

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <!-- Feature Type Selection -->
            <div class="my-4">
                <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Feature Type</span>
            </div>
            <div>
                <ul class="w-full rounded-lg border border-gray-300 bg-white text-sm font-medium text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-white">
                    <li class="border-b border-gray-300 dark:border-gray-600">
                        <div class="flex items-start p-4">
                            <div class="flex items-center h-5">
                                <input id="dssp" type="checkbox" value="dssp" v-model="emp_feats" class="h-4 w-4 accent-blue-600" />
                            </div>
                            <div class="ms-3">
                                <label for="dssp" class="text-sm font-medium text-gray-900 dark:text-gray-300">
                                    {{ featureDescriptions.dssp.name }}
                                    <sup class="ml-1 text-[10px] font-semibold text-gray-500 dark:text-gray-400">[1]</sup>
                                </label>
                                <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
                                    {{ featureDescriptions.dssp.description }}
                                </p>
                            </div>
                        </div>
                    </li>
                    <li>
                        <div class="flex items-start p-4">
                            <div class="flex items-center h-5">
                                <input id="rosetta" type="checkbox" value="rosetta" v-model="emp_feats" class="h-4 w-4 accent-blue-600" />
                            </div>
                            <div class="ms-3">
                                <label for="rosetta" class="text-sm font-medium text-gray-900 dark:text-gray-300">
                                    {{ featureDescriptions.rosetta.name }}
                                    <sup class="ml-1 text-[10px] font-semibold text-gray-500 dark:text-gray-400">[2]</sup>
                                </label>
                                <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
                                    {{ featureDescriptions.rosetta.description }}
                                </p>

                                <!-- Rosetta Relax Option -->
                                <div v-if="emp_feats.includes('rosetta')" class="mt-3 ml-4 p-3 bg-gray-50 dark:bg-gray-800 rounded-lg">
                                    <div class="flex items-center">
                                        <input id="rosetta_relax" type="checkbox" v-model="rosetta_relax" class="h-4 w-4 accent-blue-600" />
                                        <label for="rosetta_relax" class="ms-2 text-sm font-medium text-gray-700 dark:text-gray-300"> Apply Structure Relaxation First </label>
                                    </div>
                                    <p class="text-xs text-gray-500 dark:text-gray-400 mt-1 ml-6">Use Rosetta FastRelax for energy minimization before scoring. This may take longer.</p>
                                </div>
                            </div>
                        </div>
                    </li>
                </ul>

                <details class="mt-4 rounded-lg border border-gray-200 bg-gray-50 px-4 py-3 dark:border-gray-700 dark:bg-gray-800/60" id="structural-references">
                    <summary class="cursor-pointer select-none text-sm font-semibold text-gray-700 dark:text-gray-200">References</summary>
                    <ol class="mt-3 list-decimal list-inside space-y-2 text-xs text-gray-600 dark:text-gray-300">
                        <li>
                            Hekkelman ML, Salmoral DÁ, Perrakis A, Joosten RP. DSSP 4: FAIR annotation of protein secondary structure. <span class="italic">Protein Science</span>. 2025; 34(8): e70208.
                            <a class="ml-1 text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1002/pro.70208" target="_blank" rel="noreferrer">doi:10.1002/pro.70208</a>
                        </li>
                        <li>
                            Alford RF, Leaver-Fay A, Jeliazkov JR, et al. The Rosetta all-atom energy function for macromolecular modeling and design. <span class="italic">J. Chem. Theory Comput.</span> 2017; 13(6): 3031-3048.
                            <a class="ml-1 text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1021/acs.jctc.7b00125" target="_blank" rel="noreferrer">doi:10.1021/acs.jctc.7b00125</a>
                        </li>
                    </ol>
                </details>
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
