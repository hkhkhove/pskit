<script setup>
import { ref, computed, watch, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import JSZip from "jszip";
import InputStructure from "../../components/InputStructure.vue";
import TaskLayout from "../../components/TaskLayout.vue";
import { annotateBindingPairsInWorker, sanitizeKey } from "../../utils/wasmBatch.js";
import { renderPdbeMolstar, applySelectionWithRetry, highlightResidues, molstarFormatFromPskitFormat, createBlobUrlFromBytes } from "../../utils/pdbeMolstar.js";
import { useMolstar, MOLSTAR_COLORS } from "../../composables/useMolstar.js";
import { useWasmTask } from "../../composables/useWasmTask.js";

const route = useRoute();
const router = useRouter();

const { viewerContainer, initViewer, getViewerInstance, revokeViewerObjectUrl, idStructureCache, getViewerStructureKey, setViewerStructureKey, setViewerLastObjectUrl } = useMolstar();
const { input_method, ids, files, processing, error_message, results, file_errors, last_run_input_method, is_results_view, has_results, run_button_text, executeWasmTask } = useWasmTask();

const cutoff = ref("");
const current_index = ref(0);
const selected_row_index = ref(-1);

function cutoffExample() {
    cutoff.value = 3.5;
}

function normalizePdbId(id) {
    const s = String(id || "")
        .trim()
        .toLowerCase();
    return s.length === 4 ? s : s;
}

function bindingSiteParamsFromRows(rows) {
    const protParams = [];
    const naParams = [];
    const seenProt = new Set();
    const seenNa = new Set();

    for (const r of rows || []) {
        const protChain = String(r?.prot_chain ?? "").trim();
        const protResiNum = Number.parseInt(String(r?.prot_resi ?? ""), 10);
        if (protChain && Number.isFinite(protResiNum)) {
            const key = `${protChain}:${protResiNum}`;
            if (!seenProt.has(key)) {
                seenProt.add(key);
                protParams.push({
                    auth_asym_id: protChain,
                    auth_residue_number: protResiNum,
                });
            }
        }

        const naChain = String(r?.na_chain ?? "").trim();
        const naResiNum = Number.parseInt(String(r?.na_resi ?? ""), 10);
        if (naChain && Number.isFinite(naResiNum)) {
            const key = `${naChain}:${naResiNum}`;
            if (!seenNa.has(key)) {
                seenNa.add(key);
                naParams.push({
                    auth_asym_id: naChain,
                    auth_residue_number: naResiNum,
                });
            }
        }
    }

    return { protParams, naParams };
}

function buildColoredSelectionsFromRows(rows) {
    const { protParams, naParams } = bindingSiteParamsFromRows(rows);
    return [...protParams.map((p) => ({ ...p, color: MOLSTAR_COLORS.protein })), ...naParams.map((p) => ({ ...p, color: MOLSTAR_COLORS.nucleic }))];
}

function makeJsonFilename({ base }) {
    const c = sanitizeKey(String(cutoff.value));
    return `${base}.binding_pairs.cutoff_${c}.csv`;
}

function normalizePairsResult(result) {
    const pairs = Array.isArray(result?.pairs) ? result.pairs.map((x) => String(x)) : [];
    const distancesRaw = Array.isArray(result?.distances) ? result.distances : [];
    const distances = distancesRaw.map((x) => Number(x));
    return { pairs, distances };
}

function parseResidueToken(token) {
    const parts = String(token || "").split("-");
    const chain = parts[0] ?? "";
    const resi = parts.length >= 2 ? parts[1] : "";
    const resn = parts.length >= 3 ? parts.slice(2).join("-") : "";
    return { chain, resi, resn };
}

function parsePair(pair) {
    const [left, right] = String(pair || "").split("_");
    const prot = parseResidueToken(left);
    const na = parseResidueToken(right);
    return { prot, na };
}

function toCsvRow(arr) {
    return arr.join(",");
}

function csvTextForResult(res) {
    const header = toCsvRow(["prot_chain", "prot_resi", "prot_resn", "na_chain", "na_resi", "na_resn", "distance"]);
    const body = (res?.rows || []).map((r) => toCsvRow([r.prot_chain, r.prot_resi, r.prot_resn, r.na_chain, r.na_resi, r.na_resn, r.distance.toFixed(3)]));
    return [header, ...body].join("\n") + "\n";
}

function downloadTextFile({ text, filename }) {
    const blob = new Blob([text], { type: "text/csv;charset=utf-8;" });
    const link = document.createElement("a");
    const url = URL.createObjectURL(blob);
    link.setAttribute("href", url);
    link.setAttribute("download", filename);
    link.style.visibility = "hidden";
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
}

function uniqueZipName(filename, usedSet) {
    let safeName = filename;
    let counter = 1;
    while (usedSet.has(safeName)) {
        safeName = `${filename.replace(/\.csv$/, "")}_${counter}.csv`;
        counter++;
    }
    usedSet.add(safeName);
    return safeName;
}

async function runAnnotateBindingPairs() {
    current_index.value = 0;
    selected_row_index.value = -1;
    await executeWasmTask({
        onInputsPrepared: (inputs, lastInputMethod) => {
            idStructureCache.clear();
            if (lastInputMethod === "id") {
                for (const input of inputs || []) {
                    const id = normalizePdbId(input?.base);
                    if (id && input?.bytes) {
                        idStructureCache.set(id, { bytes: input.bytes, format: input.format });
                    }
                }
            }
        },
        processOne: (input) => {
            const bytesForWorker = last_run_input_method.value === "id" ? input.bytes.slice() : input.bytes;
            return annotateBindingPairsInWorker(bytesForWorker, cutoff.value, input.format);
        },
        toDownloadItems: (result, input) => {
            const normalized = normalizePairsResult(result);
            const n = Math.min(normalized.pairs.length, normalized.distances.length);
            const rows = [];
            for (let i = 0; i < n; i++) {
                const pair = normalized.pairs[i];
                const parsed = parsePair(pair);
                rows.push({
                    prot_chain: parsed.prot.chain,
                    prot_resi: parsed.prot.resi,
                    prot_resn: parsed.prot.resn,
                    na_chain: parsed.na.chain,
                    na_resi: parsed.na.resi,
                    na_resn: parsed.na.resn,
                    distance: normalized.distances[i],
                    _raw_pair: pair,
                });
            }

            return [
                {
                    source: input.source,
                    base: input.base,
                    format: input.format,
                    cutoff: cutoff.value,
                    pairs: normalized.pairs,
                    distances: normalized.distances,
                    rows,
                },
            ];
        },
    });
}

const has_multiple_results = computed(() => results.value.length > 1);

const current_result = computed(() => {
    if (!has_results.value) return null;
    const i = Math.min(Math.max(0, current_index.value), results.value.length - 1);
    return results.value[i] ?? null;
});

const current_title = computed(() => {
    if (!current_result.value) return "";
    const i = Math.min(Math.max(0, current_index.value), results.value.length - 1);
    return `${current_result.value.source} (${i + 1}/${results.value.length})`;
});

const can_next_table = computed(() => {
    return has_multiple_results.value && !processing.value;
});

function nextTable() {
    if (!has_multiple_results.value) return;
    current_index.value = (current_index.value + 1) % results.value.length;
    selected_row_index.value = -1;
}

async function focusRowPair(row, rowIndex = -1) {
    const prot_chain = String(row?.prot_chain ?? "").trim();
    const prot_resi = Number.parseInt(String(row?.prot_resi ?? ""), 10);
    const na_chain = String(row?.na_chain ?? "").trim();
    const na_resi = Number.parseInt(String(row?.na_resi ?? ""), 10);

    const viewerInstance = getViewerInstance();
    if (!viewerInstance) return;
    if (!prot_chain || !Number.isFinite(prot_resi) || !na_chain || !Number.isFinite(na_resi)) return;

    selected_row_index.value = typeof rowIndex === "number" ? rowIndex : -1;

    try {
        await highlightResidues(viewerInstance, {
            data: [
                { auth_asym_id: prot_chain, auth_residue_number: prot_resi },
                { auth_asym_id: na_chain, auth_residue_number: na_resi },
            ],
            color: MOLSTAR_COLORS.focus,
            focus: true,
        });
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

const can_download_table = computed(() => {
    return !!current_result.value && !processing.value;
});

const can_download_all_tables = computed(() => {
    return has_results.value && !processing.value;
});

function downloadCurrentTable() {
    if (!current_result.value) return;
    const header = toCsvRow(["prot_chain", "prot_resi", "prot_resn", "na_chain", "na_resi", "na_resn", "distance"]);
    const body = (current_result.value.rows || []).map((r) => toCsvRow([r.prot_chain, r.prot_resi, r.prot_resn, r.na_chain, r.na_resi, r.na_resn, r.distance.toFixed(3)]));
    const text = [header, ...body].join("\n") + "\n";
    const filename = makeJsonFilename({
        base: current_result.value.base || "results",
    });
    downloadTextFile({ text, filename });
}

async function downloadAllTablesZip() {
    if (!can_download_all_tables.value) return;

    try {
        const zip = new JSZip();
        const used = new Set();
        const c = sanitizeKey(String(cutoff.value));

        for (const res of results.value || []) {
            const base = sanitizeKey(String(res?.base || "results")) || "results";
            const filename = `${base}.binding_pairs.cutoff_${c}.csv`;
            const safeName = uniqueZipName(filename, used);
            zip.file(safeName, csvTextForResult(res));
        }

        const blob = await zip.generateAsync({ type: "blob" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "binding_pairs_tables.zip";
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

async function renderMolstarForCurrentResult() {
    const res = current_result.value;
    if (!res) return;
    if (!viewerContainer.value) return;

    try {
        const viewerInstance = await initViewer();

        let nextKey = "";
        if (last_run_input_method.value === "file") {
            const f = (files.value || []).find((x) => x?.name === res.source);
            if (f) {
                nextKey = `file:${f.name}`;
            } else {
                const id = String(res.base || "").trim();
                if (id) nextKey = `id:${normalizePdbId(id)}`;
            }
        } else {
            const id = String(res.base || "").trim();
            if (id) nextKey = `id:${normalizePdbId(id)}`;
        }

        if (!nextKey) return;

        if (getViewerStructureKey() !== nextKey) {
            setViewerStructureKey(nextKey);
            const options = {};
            revokeViewerObjectUrl();

            if (nextKey.startsWith("file:")) {
                const fileName = nextKey.slice("file:".length);
                const f = (files.value || []).find((x) => x?.name === fileName);
                if (!f) return;
                const url = URL.createObjectURL(f);
                setViewerLastObjectUrl(url);
                options.customData = {
                    url,
                    format: molstarFormatFromPskitFormat(res.format),
                    binary: false,
                };
            } else if (nextKey.startsWith("id:")) {
                const id = nextKey.slice("id:".length);
                const cached = idStructureCache.get(id);
                if (cached?.bytes) {
                    const url = createBlobUrlFromBytes(cached.bytes);
                    setViewerLastObjectUrl(url);
                    options.customData = {
                        url,
                        format: molstarFormatFromPskitFormat(cached.format),
                        binary: false,
                    };
                } else if (id) {
                    options.moleculeId = id.toLowerCase();
                }
            }

            await renderPdbeMolstar(viewerInstance, viewerContainer.value, options);
        }

        const selections = buildColoredSelectionsFromRows(res.rows);
        await applySelectionWithRetry(viewerInstance, {
            data: selections,
            nonSelectedColor: MOLSTAR_COLORS.nonSelected,
            focus: false,
            keepRepresentations: true,
        });
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

watch(
    () => results.value.length,
    async () => {
        if (!is_results_view.value) return;
        if (!has_results.value) return;
        await renderMolstarForCurrentResult();
    },
    { flush: "post" },
);

watch(
    () => current_index.value,
    async () => {
        if (!is_results_view.value) return;
        if (!has_results.value) return;
        await renderMolstarForCurrentResult();
    },
    { flush: "post" },
);

watch(
    () => is_results_view.value,
    async (v) => {
        if (!v) return;
        if (!has_results.value) return;
        await nextTick();
        await renderMolstarForCurrentResult();
    },
    { flush: "post" },
);

watch(
    () => route.query.view,
    (v) => {
        if (v !== "results") return;
        if (processing.value) return;
        if (has_results.value) return;
        const q = { ...route.query };
        delete q.view;
        router.replace({ query: q });
    },
    { immediate: true },
);
</script>

<template>
    <TaskLayout title="Nucleic-acid Binding Site Annotation" :processing="processing" :runButtonText="run_button_text" :errorMessage="error_message" :fileErrors="file_errors" :isResultsView="is_results_view" :showResults="has_results" @submit="runAnnotateBindingPairs">
        <template #viewer>
            <div class="flex items-center justify-between gap-3">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Structure</p>
                <div class="text-sm font-semibold text-gray-700 dark:text-gray-300">
                    {{ current_title }}
                </div>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div
                class="w-full h-[720px] relative overflow-hidden rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-900">
                <div ref="viewerContainer" class="w-full h-full relative"></div>
            </div>
            <div v-if="current_result"
                class="mt-3 flex flex-wrap items-center justify-center gap-3 rounded-lg border border-gray-200 bg-white px-3 py-2 text-xs text-gray-600 shadow-sm dark:border-gray-700 dark:bg-gray-900 dark:text-gray-300">
                <div class="flex items-center gap-2">
                    <div class="h-2.5 w-2.5 rounded-sm" style="background-color: rgb(52, 152, 219)"></div>
                    <span>Binding AA</span>
                </div>
                <div class="flex items-center gap-2">
                    <div class="h-2.5 w-2.5 rounded-sm" style="background-color: rgb(231, 76, 60)"></div>
                    <span>Binding NT</span>
                </div>
                <div class="flex items-center gap-2">
                    <div class="h-2.5 w-2.5 rounded-sm" style="background-color: rgb(255, 235, 59)"></div>
                    <span>Selected</span>
                </div>
            </div>
        </template>

        <template #results>
            <div class="flex items-center justify-between gap-3">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Results</p>
                <div class="flex items-center gap-2">
                    <button v-if="has_multiple_results" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_next_table" @click="nextTable">Next</button>
                    <button v-if="has_multiple_results" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_all_tables" @click="downloadAllTablesZip">Download All (ZIP)</button>
                </div>
            </div>

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div v-if="current_result" class="flex flex-col h-[720px] rounded-lg border border-gray-200 dark:border-gray-700">
                <div class="flex justify-between items-center mb-2 px-3 pt-3">
                    <div class="space-y-2">
                        <div class="text-sm font-semibold text-gray-900 dark:text-gray-200">
                            {{ current_title }}
                        </div>
                        <div class="text-xs text-gray-500 dark:text-gray-300">cutoff: {{ current_result.cutoff }} Å, {{ current_result.rows.length }} pairs</div>
                    </div>
                    <button class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_table" @click="downloadCurrentTable">Download (CSV)</button>
                </div>
                <div class="max-h-screen overflow-y-auto">
                    <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                        <thead class="bg-gray-100 dark:bg-gray-700 sticky top-0 z-10">
                            <tr>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">#</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">prot_chain</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">prot_resi</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">prot_resn</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">na_chain</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">na_resi</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">na_resn</th>
                                <th class="px-4 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">distance (Å)</th>
                            </tr>
                        </thead>
                        <tbody class="divide-y divide-gray-200 dark:divide-gray-700">
                            <tr v-for="(r, idx) in current_result.rows" :key="`${idx}-${r._raw_pair || ''}`" class="cursor-pointer transition-colors" :class="idx === selected_row_index ? 'bg-blue-50 dark:bg-blue-900/30 ring-2 ring-blue-400/60 ring-inset' : 'bg-white dark:bg-gray-800 hover:bg-gray-50 dark:hover:bg-gray-700'" :aria-selected="idx === selected_row_index" @click="focusRowPair(r, idx)">
                                <td class="px-4 py-2 text-xs text-gray-700 dark:text-gray-300">
                                    {{ idx + 1 }}
                                </td>
                                <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-200">
                                    {{ r.prot_chain }}
                                </td>
                                <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">
                                    {{ r.prot_resi }}
                                </td>
                                <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">
                                    {{ r.prot_resn }}
                                </td>
                                <td class="px-4 py-2 text-xs font-mono text-gray-900 dark:text-gray-200">
                                    {{ r.na_chain }}
                                </td>
                                <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">
                                    {{ r.na_resi }}
                                </td>
                                <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">
                                    {{ r.na_resn }}
                                </td>
                                <td class="px-4 py-2 text-xs text-gray-900 dark:text-gray-200">
                                    {{ Number.isFinite(r.distance) ? r.distance.toFixed(3) : r.distance }}
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>
        </template>

        <template #input>
            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" :max-files="200" :max-size="500 * 1024 * 1024" />
        </template>

        <template #custom-params>
            <div class="flex flex-row justify-between my-4">
                <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Cutoff (Å)</span>
            </div>

            <div class="w-auto">
                <label class="w-full block mb-2 text-sm font-medium text-gray-900 dark:text-gray-300">nearest atomic distance <span @click="cutoffExample" class="text-xs cursor-pointer hover:text-blue-700 hover:underline font-normal">(e.g., 3.5)</span></label>
                <input type="number" required step="0.1" v-model.number="cutoff" min="0" class="w-full rounded-lg border border-gray-300 bg-gray-50 p-2.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400 focus:border-blue-400 text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:placeholder-gray-400" />
            </div>
        </template>
    </TaskLayout>
</template>
