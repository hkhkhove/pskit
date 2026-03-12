<script setup>
import { ref, computed, watch, onUnmounted } from "vue";
import { useRoute } from "vue-router";
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

const status = ref("");
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
const notFoundRetryCount = ref(0); //防止服务器还没来得及索引任务导致查询失败
const copiedUrl = ref(false);
let pollInterval = null;

const MAX_NOT_FOUND_RETRIES = 5;

const isPending = computed(() => status.value === "pending");
const isProcessing = computed(() => status.value === "processing");
const isCompleted = computed(() => status.value === "completed");
const isFailed = computed(() => status.value === "failed");

const statusLabel = computed(() => {
    if (isPending.value) return "Pending";
    if (isProcessing.value) return "Processing";
    if (isCompleted.value) return "Completed";
    if (isFailed.value) return "Failed";
    return "Fetching status...";
});
const statusColor = computed(() => {
    if (isPending.value) return "text-violet-600";
    if (isProcessing.value) return "text-blue-600";
    if (isCompleted.value) return "text-green-600";
    if (isFailed.value) return "text-red-600";
    return "text-gray-600";
});

// 过滤掉 error.json，只显示其他文件
const displayFiles = computed(() => resultFiles.value.filter((f) => f.filename !== "error.json"));

// Expose for parent component access
// defineExpose({
//     status,
//     resultFiles,
//     isCompleted,
//     isFailed,
//     isPending,
//     isProcessing,
// });

async function fetchTaskStatus() {
    let shouldKeepLoading = false;
    try {
        const response = await fetch(`/api/tasks/${props.taskId}`);
        if (!response.ok) {
            const errorText = await response.text();
            if (response.status === 404 && notFoundRetryCount.value < MAX_NOT_FOUND_RETRIES) {
                notFoundRetryCount.value += 1;
                shouldKeepLoading = true;
                console.log(`Task not found (attempt ${notFoundRetryCount.value}/${MAX_NOT_FOUND_RETRIES}). Retrying...`);
                return;
            }
            throw new Error(`[${response.status}]: ${errorText || "Unknown error"}`);
        }
        const data = await response.json();
        notFoundRetryCount.value = 0;
        fetchError.value = null;

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
            emit("completed", {
                files: resultFiles.value,
                errorItems: errorItems.value,
            });
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
        if (shouldKeepLoading) return;
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
    if (!pollInterval) {
        pollInterval = setInterval(fetchTaskStatus, 3000);
    }
}

function stopPolling() {
    polling.value = false;
    if (pollInterval) {
        clearInterval(pollInterval);
        pollInterval = null;
    }
}

function formatDateTime(dateStr) {
    if (!dateStr) return "N/A";
    return new Date(dateStr).toLocaleString();
}

const currentTaskUrl = computed(() => {
    if (typeof window === "undefined") return "";

    const url = new URL(window.location.href);
    return url.toString();
});

async function copyCurrentUrl() {
    if (!currentTaskUrl.value) return;
    try {
        await navigator.clipboard.writeText(currentTaskUrl.value);
        copiedUrl.value = true;
        setTimeout(() => {
            copiedUrl.value = false;
        }, 2000);
    } catch (error) {
        console.error("Failed to copy task URL:", error);
        copiedUrl.value = false;
    }
}

watch(
    () => props.taskId,
    (newId) => {
        if (newId) {
            if (pollInterval) {
                clearInterval(pollInterval);
                pollInterval = null;
            }
            polling.value = false;
            fetchError.value = null;
            isLoading.value = true;
            notFoundRetryCount.value = 0;
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
                    <span class="font-semibold" :class="statusColor">
                        {{ statusLabel }}
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
                <div class="mt-1 flex items-center justify-center gap-2 text-sm text-gray-400 dark:text-gray-500">
                    <p>You can return later using this page link.</p>
                    <button type="button" class="rounded-md border border-blue-200 bg-white px-2.5 text-xs font-medium text-blue-700 transition hover:bg-blue-50 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-200 dark:hover:bg-blue-900/40" @click="copyCurrentUrl">
                        {{ copiedUrl ? "Copied" : "Copy Link" }}
                    </button>
                </div>
            </div>

            <!-- Task Failed -->
            <div v-else-if="isFailed" class="p-4 rounded-lg bg-red-100 border border-red-400 text-red-800 dark:bg-red-900/30 dark:border-red-800 dark:text-red-300">
                <p>
                    <strong>Task Failed: </strong>
                    {{ errorMessage || "An unknown error occurred." }}
                </p>
            </div>

            <!-- <div v-else-if="isCompleted"></div> -->
        </div>
    </div>
</template>
