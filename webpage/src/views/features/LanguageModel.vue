<script setup>
import { ref, computed } from "vue";
import InputStructure from "../../components/InputStructure.vue";
import TaskStatus from "../../components/TaskStatus.vue";
import TaskLayout from "../../components/TaskLayout.vue";
import { useRemoteTask } from "../../composables/useRemoteTask.js";

const { task_id, input_method, ids, files, isLoading, submissionError, showResults, resultFiles, errorItems, is_results_view, is_task_view, submitTask, handleTaskCompleted, handleTaskFailed, downloadAllAsZip } = useRemoteTask("lm_embed");

const model_type = ref("esm2");

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

async function handleSubmit() {
    await submitTask({ model_type: model_type.value });
}

async function triggerDownloadAll() {
    await downloadAllAsZip("lm_embed.zip");
}
</script>

<template>
    <TaskLayout title="Language Model Embedding" :processing="isLoading" :errorMessage="submissionError" :isTaskView="is_task_view" :isResultsView="is_results_view" :hasResults="showResults" @submit="handleSubmit">
        <template #input>
            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" />
        </template>

        <template #custom-params>
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
                </ul>
            </div>

            <details class="mt-4 rounded-lg border border-gray-200 bg-gray-50 px-4 py-3 dark:border-gray-700 dark:bg-gray-800/60">
                <summary class="cursor-pointer select-none text-sm font-semibold text-gray-700 dark:text-gray-200">References</summary>
                <ol class="mt-3 list-decimal list-inside space-y-2 text-xs text-gray-600 dark:text-gray-300">
                    <li>
                        Lin Z, Akin H, Rao R, et al. Evolutionary-scale prediction of atomic-level protein structure with a language model.
                        <span class="italic">Science</span>. 2023; 379(6637): 1123-1130.
                        <a class="ml-1 text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1126/science.ade2574" target="_blank" rel="noreferrer">doi:10.1126/science.ade2574</a>
                    </li>
                    <li>
                        Su J, Han C, Zhou Y, et al. SaProt: Protein Language Modeling with Structure-aware Vocabulary.
                        <span class="italic">bioRxiv</span>. 2023.
                        <a class="ml-1 text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1101/2023.10.01.560349" target="_blank" rel="noreferrer">doi:10.1101/2023.10.01.560349</a>
                    </li>
                </ol>
            </details>
        </template>

        <template #status>
            <TaskStatus :task-id="task_id" :task-name="taskDisplayName" @completed="handleTaskCompleted" @failed="handleTaskFailed" />
        </template>

        <template #results>
            <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">
                {{ taskDisplayName }}
            </p>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div class="flex items-center justify-between gap-3 mb-4">
                <h4 class="text-xl font-semibold text-gray-700 dark:text-gray-400">Result Files</h4>
                <button v-if="resultFiles.length > 1" @click="triggerDownloadAll" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600">Download All (ZIP)</button>
            </div>

            <div v-if="resultFiles.length > 0" class="space-y-2">
                <div v-for="file in resultFiles" :key="file.filename" class="flex items-center justify-between rounded-lg px-4 py-3 bg-gray-50 hover:bg-gray-100 dark:bg-gray-800 dark:hover:bg-gray-700 transition-colors">
                    <div class="min-w-0">
                        <div class="truncate text-sm font-medium text-gray-900 dark:text-gray-200">{{ file.filename }}</div>
                        <div class="text-xs text-gray-500 dark:text-gray-400">{{ formatFileSize(file.size) }}</div>
                    </div>
                    <a :href="file.download_url" download class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600">Download</a>
                </div>
            </div>

            <div v-else class="text-center py-4 text-gray-500 dark:text-gray-400">
                <p>Processing completed, but no result files were generated.</p>
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
