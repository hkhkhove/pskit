import { ref, computed, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { nanoid } from "nanoid";
import JSZip from "jszip";

export function useRemoteTask(taskName) {
    const route = useRoute();
    const router = useRouter();

    const task_id = ref("");
    const input_method = ref("id");
    const ids = ref("");
    const files = ref([]);
    
    const isLoading = ref(false);
    const submissionError = ref(null);

    // Result display state - only shown after completion
    const showResults = ref(false);
    const resultFiles = ref([]);
    const errorItems = ref({});

    const has_task_id = computed(() => Boolean(task_id.value));
    const is_results_view = computed(() => route.query.view === "results" && has_task_id.value);
    const is_task_view = computed(() => (route.query.view === "status" || route.query.view === "results") && has_task_id.value);

    async function downloadAllAsZip(zipName = "results.zip") {
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
            a.download = zipName;
            document.body.appendChild(a);
            a.click();
            a.remove();
            URL.revokeObjectURL(url);
        } catch (e) {
            console.error("Error creating zip:", e);
        }
    }

    async function submitTask(extraParams = {}) {
        // Validation (can be overridden or extended)
        if (input_method.value === "file") {
            if (files.value.length === 0) {
                submissionError.value = "Please upload at least one structure file (.pdb or .cif).";
                return false;
            }
        } else if (input_method.value === "id") {
            if (ids.value.trim() === "") {
                submissionError.value = "Please enter at least one PDB ID (separated by commas).";
                return false;
            }
        } else {
            submissionError.value = "Please select an input method (ID or file).";
            return false;
        }

        isLoading.value = true;
        submissionError.value = null;
        showResults.value = false;
        resultFiles.value = [];
        errorItems.value = {};

        task_id.value = nanoid();

        const formData = new FormData();
        formData.append("task_id", task_id.value);
        formData.append("task_name", taskName);
        formData.append("input_method", input_method.value);
        formData.append("ids", ids.value);

        // Add extra params
        for (const [key, value] of Object.entries(extraParams)) {
            formData.append(key, value);
        }

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

    async function handleTaskCompleted({ files, errorItems: errors }) {
        resultFiles.value = files.filter((f) => f.filename !== "error.json");
        errorItems.value = errors || {};
        showResults.value = true;

        if (route.query.view !== "results") {
            const q = { ...route.query, view: "results", task_id: task_id.value };
            await router.replace({ query: q });
        }
    }

    function handleTaskFailed(error) {
        showResults.value = false;
        // error usually shown inside TaskStatus component
    }

    watch(
        () => route.query,
        (query) => {
            if (query.view === "results" && query.task_id) {
                task_id.value = query.task_id;
            } else if (query.view === "results" && !query.task_id && !task_id.value) {
                const q = { ...route.query };
                delete q.view;
                router.replace({ query: q });
            }
        },
        { immediate: true },
    );

    return {
        task_id, input_method, ids, files,
        isLoading, submissionError, showResults, resultFiles, errorItems, is_results_view, is_task_view,
        submitTask, handleTaskCompleted, handleTaskFailed, downloadAllAsZip
    };
}
