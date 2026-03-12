import { ref, computed, onBeforeUnmount } from "vue";
import { useRoute, useRouter } from "vue-router";
import { parsePdbIds, isValidPdbId, prepareInputsFromFiles, prepareInputsFromPdbIds, runBatch, revokeDownloadItems, groupDownloadItemsBySource, downloadGroupedAsZip } from "../utils/wasmBatch.js";

export function useWasmTask() {
    const route = useRoute();
    const router = useRouter();

    const input_method = ref("id");
    const ids = ref("");
    const files = ref([]);
    const processing = ref(false);
    const error_message = ref("");
    const results = ref([]);
    const file_errors = ref([]);
    const progress = ref({ current: 0, total: 0, current_file: "" });
    const last_run_input_method = ref("id");

    const parsed_ids = computed(() => parsePdbIds(ids.value));
    const ids_valid = computed(() => {
        if (parsed_ids.value.length === 0) return false;
        return parsed_ids.value.every(isValidPdbId);
    });

    const is_results_view = computed(() => route.query.view === "results");
    const has_results = computed(() => results.value.length > 0);
    const grouped_results = computed(() => groupDownloadItemsBySource(results.value));
    const can_download_all = computed(() => has_results.value && !processing.value);

    const progress_text = computed(() => {
        if (!processing.value || progress.value.total === 0) return "";
        return `(${progress.value.current}/${progress.value.total}) ${progress.value.current_file}`;
    });

    const run_button_text = computed(() => {
        if (!processing.value) return "Run";
        if (input_method.value === "id" && progress.value.total === 0) return "Downloading PDB files by ID...";
        return progress_text.value ? `Processing... ${progress_text.value}` : "Processing...";
    });

    function resetTaskState() {
        error_message.value = "";
        file_errors.value = [];
        progress.value = { current: 0, total: 0, current_file: "" };
        revokeDownloadItems(results.value);
        results.value = [];
    }

    async function executeWasmTask({ processOne, toDownloadItems, onInputsPrepared }) {
        resetTaskState();

        if (input_method.value === "file") {
            if (files.value.length === 0) {
                error_message.value = "Please upload at least one structure file (.pdb or .cif).";
                return;
            }
        } else if (input_method.value === "id") {
            if (parsed_ids.value.length === 0) {
                error_message.value = "Please enter at least one PDB ID (separated by commas).";
                return;
            }
            if (!ids_valid.value) {
                error_message.value = "PDB ID format is incorrect: must be 4 alphanumeric characters (separated by commas).";
                return;
            }
        } else {
            error_message.value = "Please select an input method (ID or file).";
            return;
        }

        last_run_input_method.value = input_method.value;
        processing.value = true;
        try {
            const inputs = input_method.value === "file" 
                ? await prepareInputsFromFiles(files.value) 
                : await prepareInputsFromPdbIds(parsed_ids.value);

            if (onInputsPrepared) {
                await onInputsPrepared(inputs, last_run_input_method.value);
            }

            const { downloads, errors } = await runBatch({
                inputs,
                processOne,
                toDownloadItems,
                onProgress: (p) => {
                    progress.value = p;
                },
            });

            results.value = downloads;
            file_errors.value = errors;

            if ((downloads?.length || 0) > 0) {
                const q = { ...route.query, view: "results" };
                await router.push({ query: q });
            }
        } catch (e) {
            error_message.value = e?.message ? String(e.message) : String(e);
        } finally {
            processing.value = false;
            progress.value = { current: 0, total: 0, current_file: "" };
        }
    }

    async function downloadAllAsZip(zipName = "results.zip") {
        if (!can_download_all.value) return;
        try {
            await downloadGroupedAsZip(grouped_results.value, zipName);
        } catch (e) {
            error_message.value = e?.message ? String(e.message) : String(e);
        }
    }

    onBeforeUnmount(() => {
        revokeDownloadItems(results.value);
    });

    return {
        input_method, ids, files, processing, error_message, results, file_errors, progress, last_run_input_method,
        parsed_ids, ids_valid, is_results_view, has_results, grouped_results, can_download_all,
        progress_text, run_button_text, executeWasmTask, downloadAllAsZip, resetTaskState
    };
}
