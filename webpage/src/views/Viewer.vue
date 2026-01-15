<script setup>
import { onMounted, ref, computed, onBeforeUnmount, watch, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import InputStructure from "../components/InputStructure.vue";
import { ensurePdbeMolstarLoaded, createPdbeMolstarViewer, renderPdbeMolstar } from "../utils/pdbeMolstar.js";

const route = useRoute();
const router = useRouter();

const viewerContainer = ref(null);
let viewerInstance = null;

const input_method = ref("id");
const id = ref("");
const file = ref([]);
const file_object_url = ref("");
watch(
    () => file.value,
    (nextFiles) => {
        if (file_object_url.value) {
            URL.revokeObjectURL(file_object_url.value);
            file_object_url.value = "";
        }
        if (nextFiles && nextFiles.length > 0) {
            file_object_url.value = URL.createObjectURL(nextFiles[0]);
        }
    },
    { deep: true }
);

const options = ref({});

const is_viewer_view = computed(() => route.query.view === "viewer");

function buildViewerOptions() {
    if (input_method.value === "file") {
        const f = file.value?.[0];
        if (!f || !file_object_url.value) return null;
        const ext = String(f.name || "")
            .split(".")
            .pop();
        const format = ext ? String(ext).toLowerCase() : "";
        return {
            customData: {
                url: file_object_url.value,
                format,
                binary: false,
            },
        };
    }

    if (input_method.value === "id") {
        const pdbId = String(id.value || "").trim();
        if (!pdbId) return null;
        return { moleculeId: pdbId.toLowerCase() };
    }

    return null;
}

async function goToFormView() {
    const q = { ...route.query };
    delete q.view;
    await router.replace({ query: q });
}

async function handleViewClick() {
    const nextOptions = buildViewerOptions();
    if (!nextOptions) {
        alert("Please input the PDB ID or upload the structure files (.pdb or .cif). ");
        return;
    }
    options.value = nextOptions;

    // Switch to viewer view via URL state so the browser back button returns to the form.
    const q = { ...route.query, view: "viewer" };
    await router.push({ query: q });
}

async function renderCurrent() {
    if (!viewerInstance || !viewerContainer.value) return;
    const nextOptions = buildViewerOptions();
    if (!nextOptions) return;
    options.value = nextOptions;
    renderPdbeMolstar(viewerInstance, viewerContainer.value, options.value);
}

onMounted(async () => {
    await ensurePdbeMolstarLoaded();
    viewerInstance = createPdbeMolstarViewer();
});

onBeforeUnmount(async () => {
    try {
        await viewerInstance?.clear?.();
    } catch {
        // ignore
    }
    viewerInstance = null;

    if (file_object_url.value) {
        URL.revokeObjectURL(file_object_url.value);
        file_object_url.value = "";
    }
});

watch(
    () => route.query.view,
    async (v) => {
        if (v !== "viewer") return;
        // If the user refreshes / visits with ?view=viewer but has no input state, fall back to form.
        if (!buildViewerOptions()) {
            await goToFormView();
            return;
        }
        await nextTick();
        await renderCurrent();
    },
    { immediate: true }
);
</script>
<template>
    <!-- Form view -->
    <div v-if="!is_viewer_view" class="max-w-3xl mx-auto py-8 px-4">
        <div class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">
                    Molecule Viewer
                    <sup class="ml-1 text-[10px] font-semibold text-gray-500 dark:text-gray-400">[1]</sup>
                </p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />
            <InputStructure :multiple="false" v-model:input_method="input_method" v-model:ids="id" v-model:files="file" />

            <details class="mt-4 rounded-lg border border-gray-200 bg-gray-50 px-4 py-3 dark:border-gray-700 dark:bg-gray-800/60" id="viewer-references">
                <summary class="cursor-pointer select-none text-sm font-semibold text-gray-700 dark:text-gray-200">References</summary>
                <ol class="mt-3 list-decimal list-inside space-y-2 text-xs text-gray-600 dark:text-gray-300">
                    <li>
                        Sehnal D, Bittrich S, Deshpande M, et al. Mol* Viewer: modern web app for 3D visualization and analysis of large biomolecular structures. <span class="italic">Nucleic Acids Research</span>. 2021.
                        <a class="ml-1 text-blue-600 hover:underline dark:text-blue-400" href="https://doi.org/10.1093/nar/gkab314" target="_blank" rel="noreferrer">doi:10.1093/nar/gkab314</a>
                    </li>
                </ol>
            </details>

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />
            <button @click="handleViewClick" class="w-full rounded-lg bg-blue-600 px-4 py-2 text-lg text-center font-medium text-white hover:bg-blue-700">View</button>
        </div>
    </div>

    <!-- Viewer view (full screen) -->
    <div v-else class="h-screen w-full">
        <!-- <div class="absolute left-0 top-0 z-10 w-full px-4 py-3">
            <div class="mx-auto flex max-w-6xl items-center justify-between gap-3 rounded-lg border border-gray-200 bg-white/90 px-4 py-2 shadow-sm backdrop-blur dark:border-gray-700 dark:bg-gray-900/80">
                <div class="text-sm font-semibold text-gray-900 dark:text-gray-200">Mol* Viewer</div>
                <button @click="goToFormView" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600">← Back</button>
            </div>
        </div> -->
        <div class="h-full w-full">
            <div ref="viewerContainer" style="width: 100%; height: 100%"></div>
        </div>
    </div>
</template>
