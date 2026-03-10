<script setup>
import Loading from "./Loading.vue";

const props = defineProps({
    title: { type: String, required: true },
    processing: { type: Boolean, default: false },
    runButtonText: { type: String, default: "Run" },
    errorMessage: { type: String, default: "" },
    fileErrors: { type: Array, default: () => [] },
    isTaskView: { type: Boolean, default: false },
    isResultsView: { type: Boolean, default: false },
    hasResults: { type: Boolean, default: false },
});

const emit = defineEmits(["submit"]);

function onSubmit() {
    emit("submit");
}
</script>

<template>
    <div class="mx-auto py-8 px-4" :class="isTaskView || (isResultsView && (hasResults || $slots.status)) ? 'max-w-full' : 'max-w-3xl'">
        <!-- Results View -->
        <div v-show="isTaskView || (isResultsView && (hasResults || $slots.status))" class="w-full">
            <div v-if="$slots.status && !hasResults" class="max-w-3xl mx-auto">
                <slot name="status"></slot>
            </div>

            <!-- Full Results Grid -->
            <div v-else-if="hasResults" :class="$slots.viewer ? 'grid grid-cols-1 gap-6 lg:grid-cols-2' : 'max-w-3xl mx-auto'">
                <!-- Left: structure viewer / visualizer -->
                <div v-if="$slots.viewer" class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                    <slot name="viewer"></slot>
                </div>

                <!-- Right: results / download -->
                <div class="w-full bg-white rounded-lg shadow-xl p-6 dark:bg-gray-900">
                    <slot name="results"></slot>

                    <div v-if="errorMessage" class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                        {{ errorMessage }}
                    </div>

                    <div v-if="fileErrors.length > 0" class="mt-4 rounded-lg border border-yellow-200 bg-yellow-50 p-3 text-sm text-yellow-800 dark:border-yellow-900 dark:bg-yellow-950 dark:text-yellow-200">
                        <div class="font-medium">Failed to process the following file(s):</div>
                        <ul class="mt-2 space-y-1">
                            <li v-for="e in fileErrors" :key="e.source" class="text-xs">
                                <span class="font-semibold">{{ e.source }}</span
                                >: {{ e.message }}
                            </li>
                        </ul>
                    </div>
                </div>
            </div>
        </div>

        <!-- Form View -->
        <form v-show="!(isTaskView || (isResultsView && (hasResults || $slots.status)))" @submit.prevent="onSubmit" class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">{{ title }}</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <slot name="input"></slot>

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <slot name="custom-params"></slot>
            <!-- $slots['custom-params']等价于$slots.custom-params,但是因为‘-’是特殊符号，所以要用对象名['属性名']的写法 -->
            <hr v-if="$slots['custom-params']" class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <button type="submit" class="w-full inline-flex items-center justify-center gap-2 rounded-lg bg-blue-600 px-4 py-2 text-lg text-center font-medium text-white hover:bg-blue-700 transition disabled:opacity-60 disabled:cursor-not-allowed" :disabled="processing" :aria-busy="processing">
                <Loading v-if="processing" class="h-5 w-5 text-white" />
                <span>{{ runButtonText }}</span>
            </button>

            <div v-if="errorMessage" class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                {{ errorMessage }}
            </div>

            <div v-if="fileErrors.length > 0" class="mt-4 rounded-lg border border-yellow-200 bg-yellow-50 p-3 text-sm text-yellow-800 dark:border-yellow-900 dark:bg-yellow-950 dark:text-yellow-200">
                <div class="font-medium">Failed to process the following file(s):</div>
                <ul class="mt-2 space-y-1">
                    <li v-for="e in fileErrors" :key="e.source" class="text-xs">
                        <span class="font-semibold">{{ e.source }}</span
                        >: {{ e.message }}
                    </li>
                </ul>
            </div>
        </form>
    </div>
</template>
