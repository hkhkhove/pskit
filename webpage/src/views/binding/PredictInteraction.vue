<script setup>
import { ref, computed, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { nanoid } from "nanoid";
import TaskLayout from "../../components/TaskLayout.vue";
import TaskStatus from "../../components/TaskStatus.vue";
import InputSequencePairs from "../../components/InputSequencePairs.vue";

const TASK_NAME = "pred_pni";
const MAX_PAIRS = 100;
const MAX_SEQUENCE_LENGTH = 1000;

const route = useRoute();
const router = useRouter();

const task_id = ref("");
const sequencePairs = ref([{ protein: "", nucleic: "" }]);
const bulkInputText = ref("");
const parseWarning = ref("");

const isLoading = ref(false);
const submissionError = ref("");
const showResults = ref(false);
const resultFiles = ref([]);
const errorItems = ref({});
const tableRows = ref([]);
const currentCsvFile = ref(null);

const has_task_id = computed(() => Boolean(task_id.value));
const is_results_view = computed(() => route.query.view === "results" && has_task_id.value);
const is_task_view = computed(() => (route.query.view === "status" || route.query.view === "results") && has_task_id.value);

const hasTableRows = computed(() => tableRows.value.length > 0);

function normalizeProteinSeq(seq) {
    return String(seq || "")
        .replace(/\s+/g, "")
        .toUpperCase();
}

function normalizeNucleicSeq(seq) {
    return String(seq || "")
        .replace(/\s+/g, "")
        .toUpperCase();
}

function isValidProteinSeq(seq) {
    return /^[LAGVSERTIDPKQNFYMHWCXBUZO.\-]+$/.test(seq);
}

function isValidNucleicSeq(seq) {
    return /^[ACGUTRYKMSWBDHVN\-]+$/.test(seq);
}

function collectValidatedPairs() {
    const normalizedPairs = [];

    for (let i = 0; i < sequencePairs.value.length; i++) {
        const item = sequencePairs.value[i] || {};
        const protein = normalizeProteinSeq(item.protein);
        const nucleic = normalizeNucleicSeq(item.nucleic);

        if (!protein && !nucleic) {
            continue;
        }
        if (!protein || !nucleic) {
            submissionError.value = `Pair ${i + 1} is incomplete. Please provide both protein and nucleic acid sequences.`;
            return null;
        }
        if (protein.length > MAX_SEQUENCE_LENGTH || nucleic.length > MAX_SEQUENCE_LENGTH) {
            submissionError.value = `Pair ${i + 1} exceeds maximum length ${MAX_SEQUENCE_LENGTH}.`;
            return null;
        }
        if (!isValidProteinSeq(protein)) {
            submissionError.value = `Pair ${i + 1} protein sequence contains invalid characters. Allowed: L A G V S E R T I D P K Q N F Y M H W C X B U Z O . -`;
            return null;
        }
        if (!isValidNucleicSeq(nucleic)) {
            submissionError.value = `Pair ${i + 1} nucleic sequence contains invalid characters. Allowed: A C G U R Y K M S W B D H V N -`;
            return null;
        }

        normalizedPairs.push({ protein, nucleic });
    }

    if (normalizedPairs.length === 0) {
        submissionError.value = "Please provide at least one protein-nucleic acid sequence pair.";
        return null;
    }

    if (normalizedPairs.length > MAX_PAIRS) {
        submissionError.value = `A maximum of ${MAX_PAIRS} sequence pairs is allowed per task.`;
        return null;
    }

    return normalizedPairs;
}

async function submitTask() {
    const pairs = collectValidatedPairs();
    if (!pairs) return false;

    isLoading.value = true;
    submissionError.value = "";
    showResults.value = false;
    tableRows.value = [];
    currentCsvFile.value = null;
    resultFiles.value = [];
    errorItems.value = {};

    task_id.value = nanoid();

    const formData = new FormData();
    formData.append("task_id", task_id.value);
    formData.append("task_name", TASK_NAME);
    formData.append("input_method", "sequence_pairs");
    formData.append("sequence_pairs", JSON.stringify(pairs));

    try {
        const response = await fetch("/api/tasks", {
            method: "POST",
            body: formData,
        });

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`[${response.status}]: ${errorText || "Unknown error"}`);
        }

        const q = { ...route.query, view: "results", task_id: task_id.value };
        await router.push({ query: q });
        return true;
    } catch (error) {
        submissionError.value = error.message;
        task_id.value = "";
        return false;
    } finally {
        isLoading.value = false;
    }
}

async function parsePredictionCsv(file) {
    try {
        const response = await fetch(file.download_url);
        const text = await response.text();
        const lines = text.split(/\r?\n/).filter((line) => line.trim() !== "");
        if (lines.length <= 1) return [];

        return lines.slice(1).map((line) => {
            const [protein = "", nucleic = "", scoreStr = "0"] = line.split(",").map((v) => v.trim());
            const score = Number.parseFloat(scoreStr);
            return {
                protein,
                nucleic,
                score: Number.isFinite(score) ? score : 0,
            };
        });
    } catch (e) {
        console.error("Failed to parse prediction CSV:", e);
        return [];
    }
}

async function handleTaskCompleted({ files, errorItems: errors }) {
    resultFiles.value = files.filter((f) => f.filename !== "error.json");
    errorItems.value = errors || {};
    showResults.value = true;

    const csvFile = resultFiles.value.find((f) => f.filename.toLowerCase().endsWith(".csv")) || null;
    currentCsvFile.value = csvFile;
    tableRows.value = csvFile ? await parsePredictionCsv(csvFile) : [];

    if (route.query.view !== "results") {
        const q = { ...route.query, view: "results", task_id: task_id.value };
        await router.replace({ query: q });
    }
}

function handleTaskFailed() {
    showResults.value = false;
}

async function downloadResultCsv() {
    if (!currentCsvFile.value) return;
    try {
        const response = await fetch(currentCsvFile.value.download_url);
        const blob = await response.blob();
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = currentCsvFile.value.filename || "interaction_prediction.csv";
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
    } catch (e) {
        console.error("Failed to download CSV:", e);
    }
}

watch(
    () => route.query,
    (query) => {
        if (query.task_id && (query.view === "status" || query.view === "results")) {
            task_id.value = String(query.task_id);
        } else if (query.view === "results" && !query.task_id && !task_id.value) {
            const q = { ...route.query };
            delete q.view;
            router.replace({ query: q });
        }
    },
    { immediate: true },
);
</script>

<template>
    <TaskLayout title="Protein-Nucleic acid Interaction Prediction" :processing="isLoading"
        :errorMessage="submissionError" :isTaskView="is_task_view" :isResultsView="is_results_view"
        :showResults="showResults" @submit="submitTask">
        <template #input>
            <InputSequencePairs v-model:pairs="sequencePairs" v-model:paste-text="bulkInputText"
                v-model:parse-warning="parseWarning" />
        </template>

        <template #custom-params>
            <ul class="space-y-1.5 list-disc list-inside">
                <li class="text-xs text-gray-500 dark:text-gray-300">One pair means one protein sequence and one nucleic
                    acid sequence.</li>
                <li class="text-xs text-gray-500 dark:text-gray-300">Any <span class="font-mono">T</span>
                    in nucleic
                    acid sequences is automatically converted to <span class="font-mono">U</span>.</li>
            </ul>
        </template>

        <template #status>
            <TaskStatus :task-id="task_id" task-name="Protein-Nucleic acid Interaction Prediction"
                @completed="handleTaskCompleted" @failed="handleTaskFailed" />
        </template>

        <template #results>
            <div class="flex items-center justify-between gap-3">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Prediction Results</p>
                <button
                    class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600 disabled:opacity-50 disabled:cursor-not-allowed"
                    :disabled="!currentCsvFile" @click="downloadResultCsv">Download CSV</button>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div v-if="hasTableRows"
                class="max-h-[720px] overflow-y-auto rounded-lg border border-gray-200 dark:border-gray-700">
                <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                    <thead class="bg-gray-100 dark:bg-gray-700 sticky top-0 z-10">
                        <tr>
                            <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">#
                            </th>
                            <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">
                                Protein Sequence</th>
                            <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">
                                Nucleic Sequence</th>
                            <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">
                                Binding Score</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-gray-200 dark:divide-gray-700">
                        <tr v-for="(row, index) in tableRows" :key="index"
                            :class="row.score >= 0.5 ? 'bg-red-50 dark:bg-red-900/20 hover:bg-red-100 dark:hover:bg-red-900/30' : 'bg-white hover:bg-gray-50 dark:bg-gray-800 dark:hover:bg-gray-700'">
                            <td class="px-4 py-2 text-xs text-gray-700 dark:text-gray-300">{{ index + 1 }}</td>
                            <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-100 break-all">{{
                                row.protein }}</td>
                            <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-100 break-all">{{
                                row.nucleic }}</td>
                            <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-100">{{ row.score.toFixed(3) }}
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>

            <div v-else
                class="rounded-lg border border-gray-200 bg-gray-50 p-3 text-sm text-gray-600 dark:border-gray-700 dark:bg-gray-800/50 dark:text-gray-300">
                No CSV rows were parsed from the result file.</div>

            <div v-if="Object.keys(errorItems).length > 0" class="mt-6">
                <h4 class="mb-3 text-lg font-semibold text-red-600 dark:text-red-400">Processing Errors</h4>
                <div class="space-y-2">
                    <div v-for="(message, filename) in errorItems" :key="filename"
                        class="rounded-lg border border-red-300 bg-red-50 p-3 dark:border-red-800 dark:bg-red-900/20">
                        <p class="text-sm text-red-800 dark:text-red-300">
                            <span class="font-semibold">{{ filename }}:</span> {{ message }}
                        </p>
                    </div>
                </div>
            </div>
        </template>
    </TaskLayout>
</template>
