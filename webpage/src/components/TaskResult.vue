<script setup>
import { ref, computed, watch, onUnmounted } from "vue";
import Loading from "./Loading.vue";

const props = defineProps({
    taskId: {
        type: String,
        required: true,
    },
    taskName: {
        type: String,
        required: true,
    },
});

const emit = defineEmits(["completed", "failed"]);

const status = ref("pending");
const position = ref(null);
const uploadTime = ref(null);
const startTime = ref(null);
const endTime = ref(null);
const errorMessage = ref("");
const resultFiles = ref([]);
const errorItems = ref({}); // 存储 error.json 中的错误项
const polling = ref(false);
const isLoading = ref(true);
const fetchError = ref(null);
let pollInterval = null;

const isPending = computed(() => status.value === "pending");
const isProcessing = computed(() => status.value === "processing");
const isCompleted = computed(() => status.value === "completed");
const isFailed = computed(() => status.value === "failed");

// 过滤掉 error.json，只显示其他文件
const displayFiles = computed(() => resultFiles.value.filter((f) => f.filename !== "error.json"));

// 是否有错误项
const hasErrorItems = computed(() => Object.keys(errorItems.value).length > 0);

// Expose for parent component access
defineExpose({
    status,
    resultFiles,
    isCompleted,
    isFailed,
    isPending,
    isProcessing,
});

async function fetchTaskStatus() {
    try {
        const response = await fetch(`/api/tasks/${props.taskId}`);
        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`[${response.status}]: ${errorText || "Unknown error"}`);
        }
        const data = await response.json();

        if (data.type === "Pending") {
            status.value = "pending";
            position.value = data.data?.position || null;
            uploadTime.value = data.data?.upload_time || null;
        } else if (data.type === "Processing") {
            status.value = "processing";
            startTime.value = data.data?.start_time || null;
            uploadTime.value = data.data?.upload_time || null;
        } else if (data.type === "Completed") {
            status.value = "completed";
            startTime.value = data.data?.start_time || null;
            endTime.value = data.data?.end_time || null;
            uploadTime.value = data.data?.upload_time || null;
            await fetchResults();
            stopPolling();
            emit("completed", { files: resultFiles.value, errorItems: errorItems.value });
        } else if (data.type === "Failed") {
            status.value = "failed";
            errorMessage.value = data.data?.error || "Unknown error";
            startTime.value = data.data?.start_time || null;
            endTime.value = data.data?.end_time || null;
            uploadTime.value = data.data?.upload_time || null;
            stopPolling();
            emit("failed", errorMessage.value);
        }
    } catch (error) {
        console.error("Failed to fetch task status:", error);
        fetchError.value = error.message;
        stopPolling();
    } finally {
        isLoading.value = false;
    }
}

async function fetchResults() {
    try {
        const response = await fetch(`/api/tasks/${props.taskId}/results`);
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}`);
        }
        const data = await response.json();
        resultFiles.value = data.files || [];

        // 检查是否存在 error.json 文件
        const errorFile = resultFiles.value.find((f) => f.filename === "error.json");
        if (errorFile) {
            await fetchErrorJson(errorFile.download_url);
        }
    } catch (error) {
        console.error("Failed to fetch results:", error);
    }
}

async function fetchErrorJson(url) {
    try {
        const response = await fetch(url);
        if (!response.ok) {
            throw new Error(`HTTP ${response.status}`);
        }
        const data = await response.json();
        errorItems.value = data;
    } catch (error) {
        console.error("Failed to fetch error.json:", error);
    }
}

function startPolling() {
    if (polling.value) return;
    polling.value = true;
    fetchTaskStatus();
    pollInterval = setInterval(fetchTaskStatus, 3000);
}

function stopPolling() {
    polling.value = false;
    if (pollInterval) {
        clearInterval(pollInterval);
        pollInterval = null;
    }
}

function formatFileSize(bytes) {
    if (bytes < 1024) return bytes + " B";
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
    return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

function formatDateTime(dateStr) {
    if (!dateStr) return "N/A";
    return new Date(dateStr).toLocaleString();
}

watch(
    () => props.taskId,
    (newId) => {
        if (newId) {
            startPolling();
        }
    },
    { immediate: true },
);

onUnmounted(() => {
    stopPolling();
});
</script>

<template>
    <div class="w-full max-w-3xl rounded-lg shadow-xl p-8 bg-white dark:bg-gray-900">
        <h1 class="text-3xl font-bold text-center text-gray-800 dark:text-gray-400 mb-6">
            {{ taskName }}
        </h1>

        <!-- Loading initial state -->
        <div v-if="isLoading" class="text-center text-gray-500 dark:text-gray-400">
            <p>Loading task details...</p>
        </div>

        <!-- Fetch error -->
        <div v-else-if="fetchError" class="p-4 rounded-lg bg-red-100 border border-red-400 text-red-800 dark:bg-red-900/30 dark:border-red-800 dark:text-red-300">
            <p>Error: {{ fetchError }}</p>
        </div>

        <!-- Task status loaded -->
        <div v-else>
            <!-- Status and time info grid -->
            <div class="grid grid-cols-2 gap-4 mb-6 border-b border-gray-300 pb-4">
                <div>
                    <strong class="text-gray-600 dark:text-gray-400">Status: </strong>
                    <span
                        class="font-semibold"
                        :class="{
                            'text-violet-600': isPending,
                            'text-blue-600': isProcessing,
                            'text-green-600': isCompleted,
                            'text-red-600': isFailed,
                        }">
                        {{ isPending ? "Pending" : isProcessing ? "Processing" : isCompleted ? "Completed" : "Failed" }}
                    </span>
                </div>
                <div class="dark:text-gray-500">
                    <strong class="text-gray-600 dark:text-gray-400">Submitted: </strong>
                    {{ formatDateTime(uploadTime) }}
                </div>
                <div class="dark:text-gray-500">
                    <strong class="text-gray-600 dark:text-gray-400">Started: </strong>
                    {{ formatDateTime(startTime) }}
                </div>
                <div class="dark:text-gray-500">
                    <strong class="text-gray-600 dark:text-gray-400">Finished: </strong>
                    {{ formatDateTime(endTime) }}
                </div>
            </div>

            <!-- Pending or Processing -->
            <div v-if="isPending || isProcessing" class="text-center py-8">
                <!-- Spinner animation -->
                <Loading class="h-8 w-8 text-blue-600 mx-auto mb-4" />
                <p v-if="isPending" class="text-lg text-gray-600 dark:text-gray-400">
                    Your task is queued<span v-if="position">
                        at position <span class="font-bold">{{ position }}</span></span
                    >. Please wait...
                </p>
                <p v-else class="text-lg text-gray-600 dark:text-gray-400">Your task is being processed. Please wait...</p>
                <p class="text-sm text-gray-400 dark:text-gray-500">The page will update automatically.</p>
                <p class="text-sm text-gray-400 mb-4 dark:text-gray-500">You can bookmark this page to view your results later.</p>
            </div>

            <!-- Task Failed -->
            <div v-else-if="isFailed" class="p-4 rounded-lg bg-red-100 border border-red-400 text-red-800 dark:bg-red-900/30 dark:border-red-800 dark:text-red-300">
                <p>
                    <strong>Task Failed: </strong>
                    {{ errorMessage || "An unknown error occurred." }}
                </p>
            </div>

            <!-- Task Completed -->
            <div v-else-if="isCompleted">
                <!-- Error items from error.json -->
                <div v-if="hasErrorItems" class="mb-6">
                    <h2 class="text-xl font-semibold text-red-600 dark:text-red-400 mb-4">Processing Errors</h2>
                    <div class="space-y-2">
                        <div v-for="(message, filename) in errorItems" :key="filename" class="p-3 bg-red-50 border border-red-300 rounded-lg dark:bg-red-900/20 dark:border-red-800">
                            <p class="text-sm font-medium text-red-800 dark:text-red-300">
                                <span class="font-bold">{{ filename }}:</span> {{ message }}
                            </p>
                        </div>
                    </div>
                </div>

                <h2 class="text-xl font-semibold text-gray-700 dark:text-gray-400 mb-4">Result Files</h2>

                <!-- File list -->
                <div v-if="displayFiles.length > 0" class="space-y-3">
                    <div v-for="file in displayFiles" :key="file.filename" class="flex items-center justify-between p-3 bg-gray-50 border border-gray-300 rounded-lg dark:bg-gray-800 dark:border-gray-700">
                        <div class="min-w-0 flex-1">
                            <p class="text-sm font-medium text-gray-900 dark:text-gray-200 truncate">{{ file.filename }}</p>
                            <p class="text-xs text-gray-500 dark:text-gray-400">{{ formatFileSize(file.size) }}</p>
                        </div>
                        <a :href="file.download_url" download class="ml-4 px-4 py-2 bg-green-600 text-white font-semibold rounded-lg shadow-md hover:bg-green-700 focus:outline-none focus:ring-2 focus:ring-green-500 focus:ring-opacity-75 transition text-sm"> Download </a>
                    </div>
                </div>

                <!-- No files -->
                <div v-else class="text-center py-4 text-gray-500 dark:text-gray-400">
                    <p>Processing completed, but no result files were generated.</p>
                </div>
            </div>
        </div>
    </div>
</template>
