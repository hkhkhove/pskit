<script setup>
import { ref, watch } from "vue";
import InputStructure from "../../components/InputStructure.vue";
import TaskStatus from "../../components/TaskStatus.vue";
import TaskLayout from "../../components/TaskLayout.vue";
import { useRemoteTask } from "../../composables/useRemoteTask.js";

const { task_id, input_method, ids, files, isLoading, submissionError, showResults, resultFiles, errorItems, is_results_view, is_task_view, submitTask, handleTaskCompleted: handleBaseTaskCompleted, handleTaskFailed, downloadAllAsZip } = useRemoteTask("emp_feats");

const emp_feats = ref(["dssp"]);
const rosetta_relax = ref(false);

const selectedFile = ref(null);
const fileContent = ref("");
const parsedDssp = ref([]);
const parsedRosetta = ref({});

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
        if (!response.ok) throw new Error(`Failed to fetch: ${response.status}`);
        const text = await response.text();
        fileContent.value = text;

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
        if (line.includes("#  RESIDUE AA STRUCTURE")) {
            dataStarted = true;
            continue;
        }
        if (!dataStarted || line.trim() === "" || line.length < 17) continue;
        try {
            const resNum = line.substring(0, 5).trim();
            const chain = line.substring(11, 12).trim();
            const aa = line.substring(13, 14).trim();
            const ss = line.substring(16, 17).trim() || "-";
            const acc = line.substring(35, 38).trim();
            if (resNum && aa) residues.push({ resNum: parseInt(resNum) || resNum, chain: chain || "-", aa, ss, acc: parseInt(acc) || 0 });
        } catch {
            continue;
        }
    }
    parsedDssp.value = residues;
}

function parseRosettaScore(text) {
    const lines = text.split("\n").filter((l) => l.trim());
    const scores = {};
    let headerLine = null,
        dataLine = null;
    for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed.startsWith("SCORE:")) {
            const parts = trimmed.split(/\s+/);
            if (parts.length > 1) {
                if (isNaN(parseFloat(parts[1]))) headerLine = trimmed;
                else dataLine = trimmed;
            }
        }
    }
    if (headerLine && dataLine) {
        const headers = headerLine.split(/\s+/).slice(1);
        const values = dataLine.split(/\s+/).slice(1);
        for (let i = 0; i < headers.length && i < values.length; i++) scores[headers[i]] = values[i];
    }
    parsedRosetta.value = scores;
}

async function handleSubmit() {
    selectedFile.value = null;
    await submitTask({
        emp_feats: emp_feats.value.join(","),
        rosetta_relax: rosetta_relax.value ? "true" : "false",
    });
}

async function handleTaskCompleted(data) {
    await handleBaseTaskCompleted(data);
    if (resultFiles.value.length > 0) {
        await selectFile(resultFiles.value[0]);
    }
}

async function triggerDownloadAll() {
    await downloadAllAsZip(`empirical_features_${task_id.value}.zip`);
}
</script>

<template>
    <TaskLayout title="Structural Feature Extraction" :processing="isLoading" :errorMessage="submissionError" :isTaskView="is_task_view" :isResultsView="is_results_view" :showResults="showResults" @submit="handleSubmit">
        <template #input>
            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" />
        </template>

        <template #custom-params>
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
                            <label for="dssp" class="ms-3 text-sm font-medium text-gray-900 dark:text-gray-300">
                                {{ featureDescriptions.dssp.name }}
                                <sup class="ml-1 text-[10px] font-semibold text-gray-500 dark:text-gray-400">[1]</sup>
                                <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">{{ featureDescriptions.dssp.description }}</p>
                            </label>
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
                                    <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">{{ featureDescriptions.rosetta.description }}</p>
                                </label>
                                <div v-if="emp_feats.includes('rosetta')" class="mt-3 ml-4 p-3 bg-gray-50 dark:bg-gray-800 rounded-lg">
                                    <div class="flex items-start">
                                        <input id="rosetta_relax" type="checkbox" v-model="rosetta_relax" class="h-4 w-4 accent-blue-600" />
                                        <label for="rosetta_relax" class="ms-3 text-sm font-medium text-gray-700 dark:text-gray-300">
                                            Apply Structure Relaxation First
                                            <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">Use Rosetta FastRelax for energy minimization before scoring.</p>
                                        </label>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </li>
                </ul>

                <details class="mt-4 rounded-lg border border-gray-200 bg-gray-50 px-4 py-3 dark:border-gray-700 dark:bg-gray-800/60" id="structural-references">
                    <summary class="cursor-pointer select-none text-sm font-semibold text-gray-700 dark:text-gray-200">References</summary>
                    <ol class="mt-3 list-decimal list-inside space-y-2 text-xs text-gray-600 dark:text-gray-300">
                        <li>Hekkelman ML, et al. DSSP 4: FAIR annotation of protein secondary structure. <span class="italic">Protein Science</span>. 2025. <a class="text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1002/pro.70208" target="_blank" rel="noreferrer">doi:10.1002/pro.70208</a></li>
                        <li>Alford RF, et al. The Rosetta all-atom energy function for macromolecular modeling and design. <span class="italic">J. Chem. Theory Comput.</span> 2017. <a class="text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1021/acs.jctc.7b00125" target="_blank" rel="noreferrer">doi:10.1021/acs.jctc.7b00125</a></li>
                    </ol>
                </details>
            </div>
        </template>

        <template #status>
            <TaskStatus :task-id="task_id" task-name="Structural Feature Extraction" @completed="handleTaskCompleted" @failed="handleTaskFailed" />
        </template>

        <template #viewer>
            <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Result Preview</p>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />
            <div v-if="selectedFile && parsedDssp.length > 0">
                <h4 class="text-lg font-medium text-gray-900 dark:text-gray-400 mb-3">DSSP Secondary Structure</h4>
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
                                <td class="px-4 py-2 text-xs font-mono" :class="{ 'text-red-600 dark:text-red-400': r.ss === 'H', 'text-blue-600 dark:text-blue-400': r.ss === 'E', 'text-green-600 dark:text-green-400': r.ss === 'T' || r.ss === 'S', 'text-gray-600 dark:text-gray-400': r.ss === '-' || r.ss === ' ' }">{{ r.ss }}</td>
                                <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">{{ r.acc }}</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
                <div class="text-center mt-4 text-xs space-x-3">
                    <span class="text-gray-900 dark:text-gray-200"><span class="text-red-600 dark:text-red-400">H</span>=α-helix</span>
                    <span class="text-gray-900 dark:text-gray-200"><span class="text-blue-600 dark:text-blue-400">E</span>=β-sheet</span>
                    <span class="text-gray-900 dark:text-gray-200"><span class="text-green-600 dark:text-green-400">T</span>=turn</span>
                    <span class="text-gray-900 dark:text-gray-200"><span class="text-green-600 dark:text-green-400">S</span>=bend</span>
                    <span class="text-gray-900 dark:text-gray-200"><span class="text-gray-600 dark:text-gray-400">-</span>=coil</span>
                </div>
            </div>
            <div v-else-if="selectedFile && Object.keys(parsedRosetta).length > 0">
                <h4 class="text-lg font-medium text-gray-900 dark:text-gray-400 mb-3">Rosetta Energy Scores</h4>
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
            <div v-else-if="selectedFile && fileContent" class="overflow-hidden">
                <h4 class="text-lg font-medium text-gray-900 dark:text-gray-400 mb-3">{{ selectedFile.filename }}</h4>
                <div class="max-h-[600px] overflow-auto rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 p-4">
                    <pre class="text-xs font-mono text-gray-800 dark:text-gray-200 whitespace-pre">{{ fileContent }}</pre>
                </div>
            </div>
        </template>

        <template #results>
            <div class="flex items-center justify-between gap-3">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Files</p>
                <button v-if="resultFiles.length > 1" @click="triggerDownloadAll" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600">Download All (ZIP)</button>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />
            <div v-if="resultFiles.length > 0" class="space-y-2">
                <div v-for="file in resultFiles" :key="file.filename" class="flex items-center justify-between rounded-lg px-4 py-3 cursor-pointer transition-colors" :class="selectedFile?.filename === file.filename ? 'bg-blue-50 ring-2 ring-blue-200 dark:bg-blue-950/40 dark:ring-blue-800' : 'bg-gray-50 hover:bg-gray-100 dark:bg-gray-800 dark:hover:bg-gray-700'" @click="selectFile(file)">
                    <div class="min-w-0">
                        <div class="truncate text-sm font-medium text-gray-900 dark:text-gray-200">{{ file.filename }}</div>
                        <div class="text-xs text-gray-500 dark:text-gray-400">{{ formatFileSize(file.size) }}</div>
                    </div>
                    <a :href="file.download_url" download class="rounded-lg border border-gray-300 bg-white px-3 py-1.5 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" @click.stop> Download </a>
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
