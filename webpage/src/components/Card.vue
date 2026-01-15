<script setup>
import { RouterLink } from "vue-router";
const props = defineProps({
    title: {
        type: String,
        required: true,
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
    <RouterLink :to="router" class="flex flex-col w-90 h-80 my-4 p-6 bg-white border border-gray-200 rounded-lg shadow-sm text-center hover:bg-gray-100 dark:bg-gray-800 dark:border-gray-700 dark:hover:bg-gray-700">
        <h5 class="mb-4 text-xl tracking-tight text-gray-900 dark:text-white">{{ title }}</h5>
        <div class="flex-1 flex items-center justify-center overflow-hidden">
            <img v-if="image" :src="image" :alt="title" class="max-h-full max-w-full object-contain rounded-md" />
        </div>
    </RouterLink>
</template>
