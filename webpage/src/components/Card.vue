<script setup>
import { RouterLink } from "vue-router";
const props = defineProps({
    title: {
        type: String,
        required: true,
    },
    description: {
        type: String,
        default: "",
    },
    image: {
        type: String,
        default: "",
    },
    router: {
        type: String,
        default: "",
    },
});
</script>

<template>
    <!-- sm 640px, md 768px, lg 1024px, xl 1280px, 2xl 1536px -->
    <RouterLink :to="router" :aria-label="title" class="group flex flex-col w-90 my-4 bg-white border border-gray-200 rounded-xl shadow-sm overflow-hidden transition hover:-translate-y-0.5 hover:shadow-md dark:bg-gray-800 dark:border-gray-700">
        <div class="relative w-full h-44 bg-gray-100 dark:bg-gray-700">
            <img v-if="image" :src="image" :alt="title" class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105" loading="lazy" decoding="async" />
            <div v-else class="flex h-full w-full items-center justify-center text-sm text-gray-500 dark:text-gray-300">No image</div>
        </div>

        <div class="flex-1 p-5">
            <h5 class="text-lg font-semibold leading-snug text-gray-900 dark:text-white">
                {{ title }}
            </h5>
            <p v-if="description" class="mt-2 text-sm text-gray-600 dark:text-gray-300 overflow-hidden text-ellipsis [display:-webkit-box] [-webkit-line-clamp:2] [-webkit-box-orient:vertical]" :title="description">
                {{ description }}
            </p>
            <p v-else class="mt-2 text-sm text-gray-500 dark:text-gray-400">Click to open</p>
        </div>
    </RouterLink>
</template>
