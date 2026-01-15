<script setup>
import { ref, computed, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { nanoid } from "nanoid";
import JSZip from "jszip";
import InputStructure from "../../components/InputStructure.vue";
import TaskResult from "../../components/TaskResult.vue";

const route = useRoute();
const router = useRouter();

const task_id = ref("");
const task_name = "lm_embed";

const input_method = ref("id");
const inputRef = ref(null);
const ids = ref("");
const files = ref([]);

const model_type = ref("esm2");

const isLoading = ref(false);
const submissionError = ref(null);

// Result display state - only shown after completion
const showResults = ref(false);
const resultFiles = ref([]);
const errorItems = ref({});

const is_results_view = computed(() => route.query.view === "results" && task_id.value);

const taskDisplayName = computed(() => {
    if (model_type.value === "esm2") return "ESM-2 Language Model Embedding";
    if (model_type.value === "saprot") return "SaProt Language Model Embedding";
    return "Language Model Embedding";
});

function formatFileSize(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
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
        a.download = `lm_embed.zip`;
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
    errorItems.value = {};
    // 生成新的 task_id
    task_id.value = nanoid();

    const formData = new FormData();
    formData.append("task_id", task_id.value);
    formData.append("task_name", task_name);
    formData.append("input_method", input_method.value);
    formData.append("ids", ids.value);
    formData.append("model_type", model_type.value);

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

        // 成功后跳转到结果视图，包含 task_id
        const q = { ...route.query, view: "results", task_id: task_id.value };
        await router.push({ query: q });
    } catch (error) {
        submissionError.value = error.message;
        task_id.value = "";
    } finally {
        isLoading.value = false;
    }
}

function handleTaskCompleted({ files, errorItems: errors }) {
    resultFiles.value = files.filter((f) => f.filename !== "error.json");
    errorItems.value = errors || {};
    showResults.value = true;
}

function handleTaskFailed(error) {
    // Stay on TaskResult view to show error
    showResults.value = false;
}

function handleBack() {
    task_id.value = "";
    showResults.value = false;
    resultFiles.value = [];
    errorItems.value = {};
    submissionError.value = null;

    const q = { ...route.query };
    delete q.view;
    delete q.task_id;
    router.replace({ query: q });
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
    <div class="mx-auto py-8 px-4 max-w-3xl">
        <!-- Results View with File List (after completion) -->
        <div v-if="is_results_view && showResults" class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
            <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">{{ taskDisplayName }}</p>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <!-- Result Files Section -->
            <div class="flex items-center justify-between gap-3 mb-4">
                <h4 class="text-xl font-semibold text-gray-700 dark:text-gray-400">Result Files</h4>
                <button v-if="resultFiles.length > 1" @click="downloadAllAsZip" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600">Download All (ZIP)</button>
            </div>

            <!-- File List -->
            <div v-if="resultFiles.length > 0" class="space-y-2">
                <div v-for="file in resultFiles" :key="file.filename" class="flex items-center justify-between rounded-lg px-4 py-3 bg-gray-50 hover:bg-gray-100 dark:bg-gray-800 dark:hover:bg-gray-700 transition-colors">
                    <div class="min-w-0">
                        <div class="truncate text-sm font-medium text-gray-900 dark:text-gray-200">{{ file.filename }}</div>
                        <div class="text-xs text-gray-500 dark:text-gray-400">{{ formatFileSize(file.size) }}</div>
                    </div>
                    <a :href="file.download_url" download class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600">Download</a>
                </div>
            </div>

            <!-- No files -->
            <div v-else class="text-center py-4 text-gray-500 dark:text-gray-400">
                <p>Processing completed, but no result files were generated.</p>
            </div>

            <!-- Error Items from error.json -->
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
        </div>

        <!-- Task Status View (pending/processing/failed - uses TaskResult component) -->
        <TaskResult v-else-if="is_results_view && !showResults" :task-id="task_id" :task-name="taskDisplayName" @completed="handleTaskCompleted" @failed="handleTaskFailed" />

        <!-- form -->
        <form v-else @submit.prevent="handleSubmit" class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Language Model Embedding</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <InputStructure ref="inputRef" v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" />

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <!-- Model Selection -->
            <div class="my-4">
                <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Model Selection</span>
            </div>
            <div>
                <ul class="w-full items-center rounded-lg border border-gray-300 bg-white text-sm font-medium text-gray-900 sm:flex dark:border-gray-600 dark:bg-gray-700 dark:text-white">
                    <li class="w-full border-b border-gray-300 sm:border-r sm:border-b-0 dark:border-gray-600">
                        <div class="flex items-center ps-3">
                            <input id="esm2" type="radio" value="esm2" v-model="model_type" class="h-4 w-4 accent-blue-600" />
                            <label for="esm2" class="ms-2 w-full py-3 text-sm font-medium text-gray-900 dark:text-gray-300">
                                ESM-2
                                <sup class="ml-1 text-[10px] font-semibold text-gray-500 dark:text-gray-400">[1]</sup>
                                <span class="text-xs text-gray-500 dark:text-gray-400 block">Sequence Embedding (650M)</span>
                            </label>
                        </div>
                    </li>
                    <li class="w-full border-b border-gray-300 sm:border-r sm:border-b-0 dark:border-gray-600">
                        <div class="flex items-center ps-3">
                            <input id="saprot" type="radio" value="saprot" v-model="model_type" class="h-4 w-4 accent-blue-600" />
                            <label for="saprot" class="ms-2 w-full py-3 text-sm font-medium text-gray-900 dark:text-gray-300">
                                SaProt
                                <sup class="ml-1 text-[10px] font-semibold text-gray-500 dark:text-gray-400">[2]</sup>
                                <span class="text-xs text-gray-500 dark:text-gray-400 block">Structure-aware Embedding (650M)</span>
                            </label>
                        </div>
                    </li>
                    <li class="w-full dark:border-gray-600">
                        <div class="flex items-center ps-3">
                            <input id="both" type="radio" value="both" v-model="model_type" class="h-4 w-4 accent-blue-600" />
                            <label for="both" class="ms-2 w-full py-3 text-sm font-medium text-gray-900 dark:text-gray-300">
                                Both
                                <sup class="ml-1 text-[10px] font-semibold text-gray-500 dark:text-gray-400">[1,2]</sup>
                                <span class="text-xs text-gray-500 dark:text-gray-400 block">ESM-2 + SaProt</span>
                            </label>
                        </div>
                    </li>
                </ul>

                <details class="mt-4 rounded-lg border border-gray-200 bg-gray-50 px-4 py-3 dark:border-gray-700 dark:bg-gray-800/60" id="lm-references">
                    <summary class="cursor-pointer select-none text-sm font-semibold text-gray-700 dark:text-gray-200">References</summary>
                    <ol class="mt-3 list-decimal list-inside space-y-2 text-xs text-gray-600 dark:text-gray-300">
                        <li>
                            Lin Z, Akin H, Rao R, et al. Language models of protein sequences at the scale of evolution enable accurate structure prediction. <span class="italic">bioRxiv</span>, 2022: 500902.
                            <a class="ml-1 text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1101/2022.07.20.500902" target="_blank" rel="noreferrer">doi:10.1101/2022.07.20.500902</a>
                        </li>
                        <li>
                            Su J, Han C, Zhou Y, et al. SaProt: Protein language modeling with structure-aware vocabulary. <span class="italic">bioRxiv</span>, 2023: 560349.
                            <a class="ml-1 text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1101/2023.10.01.560349" target="_blank" rel="noreferrer">doi:10.1101/2023.10.01.560349</a>
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
