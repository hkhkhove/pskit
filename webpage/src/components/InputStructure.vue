<script setup>
import { ref, computed, watch } from "vue";
const props = defineProps({
    multiple: {
        type: Boolean,
        default: true,
        required: false,
    },
    maxFiles: {
        type: Number,
        default: 20,
        required: false,
    },
    maxSize: {
        type: Number,
        default: 100 * 1024 * 1024, // 100 MB
        required: false,
    },
});

const input_method = defineModel("input_method");
const ids = defineModel("ids");
const ids_valid = computed(() => {
    if (!ids.value) return true;
    if (!props.multiple) {
        return /^[a-zA-Z0-9]{4}$/.test(ids.value.trim());
    }
    const valid = ids.value
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
        .every((s) => /^[a-zA-Z0-9]{4}$/.test(s));
    return valid;
});
const ids_error_message = computed(() => {
    if (ids_valid.value) return "";
    if (!props.multiple) {
        return "The PDB ID must be exactly four alphanumeric characters. Only a single ID is permitted.";
    } else {
        return "Each PDB ID must be exactly 4 alphanumeric characters, separated by commas.";
    }
});
const files = defineModel("files");
const fileLimitWarning = ref("");

const totalSize = computed(() => {
    return files.value.reduce((sum, file) => sum + file.size, 0);
});

const formatSize = (bytes) => {
    if (bytes < 1024) return bytes + " B";
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
    return (bytes / (1024 * 1024)).toFixed(1) + " MB";
};

const id_disabled = computed(() => input_method.value !== "id");
const file_disabled = computed(() => input_method.value !== "file");

const dragging = ref(false);

const dragOver = () => {
    dragging.value = true;
};

const dragLeave = () => {
    dragging.value = false;
};

const isDuplicate = (file) => {
    return files.value.some((f) => f.name === file.name && f.size === file.size);
};

function isValidFileType(file) {
    return /\.(pdb|cif)$/i.test(file.name);
}

function ids_example() {
    if (props.multiple) ids.value = "8w2s,8xhv";
    else ids.value = "8w2s";
}

function ids_example_text() {
    if (props.multiple) {
        return "(e.g., 8w2s,8xhv)";
    } else {
        return "(e.g., 8w2s)";
    }
}

const handleFiles = (event) => {
    if (file_disabled.value) return;
    fileLimitWarning.value = "";
    let selectedFiles = Array.from(event.target.files);
    selectedFiles = selectedFiles.filter(isValidFileType);
    if (!props.multiple && selectedFiles.length > 0) {
        files.value = [selectedFiles[0]];
    } else {
        let currentSize = totalSize.value;
        for (const file of selectedFiles) {
            if (files.value.length >= props.maxFiles) {
                fileLimitWarning.value = `Maximum ${props.maxFiles} files allowed. Some files were not added.`;
                break;
            }
            if (currentSize + file.size > props.maxSize) {
                fileLimitWarning.value = `Total size limit (${formatSize(props.maxSize)}) exceeded. Some files were not added.`;
                break;
            }
            if (!isDuplicate(file)) {
                files.value.push(file);
                currentSize += file.size;
            }
        }
    }
    event.target.value = "";
};

const handleDrop = (event) => {
    if (file_disabled.value) return;
    dragging.value = false;
    fileLimitWarning.value = "";
    let droppedFiles = Array.from(event.dataTransfer.files);
    droppedFiles = droppedFiles.filter(isValidFileType);
    if (!props.multiple && droppedFiles.length > 0) {
        files.value = [droppedFiles[0]];
    } else {
        let currentSize = totalSize.value;
        for (const file of droppedFiles) {
            if (files.value.length >= props.maxFiles) {
                fileLimitWarning.value = `Maximum ${props.maxFiles} files allowed. Some files were not added.`;
                break;
            }
            if (currentSize + file.size > props.maxSize) {
                fileLimitWarning.value = `Total size limit (${formatSize(props.maxSize)}) exceeded. Some files were not added.`;
                break;
            }
            if (!isDuplicate(file)) {
                files.value.push(file);
                currentSize += file.size;
            }
        }
    }
};

const removeFile = (index) => {
    files.value.splice(index, 1);
};

watch(input_method, (newVal, oldVal) => {
    if (newVal === "id") {
        files.value = [];
    } else if (newVal === "file") {
        ids.value = "";
    }
});

defineExpose({
    ids_valid,
});
</script>

<template>
    <div class="w-full">
        <div class="my-4">
            <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Input Structure</span>
        </div>
        <!-- Input id -->
        <div class="block">
            <div class="my-4 flex items-center">
                <input id="id" type="radio" value="id" v-model="input_method" class="h-4 w-4 accent-blue-600" />
                <div class="ms-2 flex items-center gap-2">
                    <label for="id" class="text-sm font-medium text-gray-900 dark:text-gray-300">
                        Input the PDB IDs of the structures. <span @click="ids_example" class="text-xs cursor-pointer hover:text-blue-700 hover:underline font-normal"> {{ ids_example_text() }} </span></label
                    >
                </div>
            </div>
            <input :disabled="id_disabled" type="text" v-model="ids" :class="{ 'cursor-not-allowed opacity-50 bg-gray-200': id_disabled }" class="w-full rounded-lg border border-gray-300 bg-gray-50 p-2.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400 focus:border-blue-400 text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:placeholder-gray-400" />
            <p v-if="!ids_valid" class="mt-2 text-sm text-red-600 dark:text-red-500">{{ ids_error_message }}</p>
        </div>

        <div class="flex flex-row flex-wrap">
            <!-- Upload files -->
            <div class="block w-full">
                <div class="my-4 flex items-center">
                    <input id="file" type="radio" value="file" v-model="input_method" class="h-4 w-4 accent-blue-600" />
                    <label for="file" class="ms-2 text-sm font-medium text-gray-900 dark:text-gray-300"><span class="font-semibold">Or</span> upload the structure files.</label>
                </div>
                <div class="flex items-center justify-center" @dragover.prevent="dragOver" @dragleave.prevent="dragLeave" @drop.prevent="handleDrop">
                    <input :disabled="file_disabled" id="dropzone-file" type="file" class="hidden" :multiple="multiple" accept=".pdb,.cif" @change="handleFiles" />
                    <label for="dropzone-file" :class="{ 'cursor-not-allowed opacity-50 bg-gray-200': file_disabled, 'bg-gray-100': dragging }" class="flex h-64 w-full flex-col items-center justify-center rounded-lg border-2 border-dashed border-gray-300 bg-gray-50 hover:bg-gray-50 dark:border-gray-600 dark:bg-gray-700 dark:hover:border-gray-500 dark:hover:bg-gray-600">
                        <div class="flex flex-col items-center justify-center pt-5 pb-6">
                            <svg class="mb-4 h-8 w-8 text-gray-500 dark:text-gray-400" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 20 16">
                                <path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 13h3a3 3 0 0 0 0-6h-.025A5.56 5.56 0 0 0 16 6.5 5.5 5.5 0 0 0 5.207 5.021C5.137 5.017 5.071 5 5 5a4 4 0 0 0 0 8h2.167M10 15V6m0 0L8 8m2-2 2 2" />
                            </svg>
                            <p class="mb-2 text-sm text-gray-500 dark:text-gray-400"><span class="font-semibold">Click to upload</span> or drag and drop</p>
                            <p class="text-xs text-gray-500 dark:text-gray-400">.pdb or .cif</p>
                        </div>
                    </label>
                </div>
                <p v-if="fileLimitWarning" class="mt-2 text-sm text-amber-600 dark:text-amber-500">{{ fileLimitWarning }}</p>
                <p v-if="multiple" class="mt-1 text-xs text-gray-500 dark:text-gray-400">{{ files.length }}/{{ maxFiles }} files, {{ formatSize(totalSize) }}/{{ formatSize(maxSize) }}</p>
            </div>

            <!-- Display files -->
            <div v-if="files.length > 0" class="w-full mt-4 ms-4 pr-4 max-h-64 overflow-y-auto">
                <ul>
                    <li v-for="(file, index) in files" :key="index">
                        <div class="flex py-1 items-center justify-between hover:bg-gray-100 dark:hover:bg-gray-700">
                            <span class="text-sm font-medium text-gray-900 dark:text-gray-300">{{ file.name }}</span>
                            <button type="button" @click="removeFile(index)" class="text-red-500 hover:text-red-700 ml-2">
                                <svg class="w-5 h-5" viewBox="0 -5 32 32" version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:sketch="http://www.bohemiancoding.com/sketch/ns">
                                    <g id="Page-1" stroke="none" stroke-width="2" fill="currentColor" fill-rule="evenodd" sketch:type="MSPage">
                                        <g id="Icon-Set" sketch:type="MSLayerGroup" transform="translate(-516.000000, -1144.000000)" fill="currentColor">
                                            <path
                                                d="M538.708,1151.28 C538.314,1150.89 537.676,1150.89 537.281,1151.28 L534.981,1153.58 L532.742,1151.34 C532.352,1150.95 531.718,1150.95 531.327,1151.34 C530.936,1151.73 530.936,1152.37 531.327,1152.76 L533.566,1154.99 L531.298,1157.26 C530.904,1157.65 530.904,1158.29 531.298,1158.69 C531.692,1159.08 532.331,1159.08 532.725,1158.69 L534.993,1156.42 L537.232,1158.66 C537.623,1159.05 538.257,1159.05 538.647,1158.66 C539.039,1158.27 539.039,1157.63 538.647,1157.24 L536.408,1155.01 L538.708,1152.71 C539.103,1152.31 539.103,1151.68 538.708,1151.28 L538.708,1151.28 Z M545.998,1162 C545.998,1163.1 545.102,1164 543.996,1164 L526.467,1164 L518.316,1154.98 L526.438,1146 L543.996,1146 C545.102,1146 545.998,1146.9 545.998,1148 L545.998,1162 L545.998,1162 Z M543.996,1144 L526.051,1144 C525.771,1143.98 525.485,1144.07 525.271,1144.28 L516.285,1154.22 C516.074,1154.43 515.983,1154.71 515.998,1154.98 C515.983,1155.26 516.074,1155.54 516.285,1155.75 L525.271,1165.69 C525.467,1165.88 525.723,1165.98 525.979,1165.98 L525.979,1166 L543.996,1166 C546.207,1166 548,1164.21 548,1162 L548,1148 C548,1145.79 546.207,1144 543.996,1144 L543.996,1144 Z"
                                                id="delete"
                                                sketch:type="MSShapeGroup"></path>
                                        </g>
                                    </g>
                                </svg>
                            </button>
                        </div>
                    </li>
                </ul>
            </div>
        </div>
    </div>
</template>
