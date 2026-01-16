<script setup>
import { ref } from "vue";

const openAnnotate = ref(false);
const openFeature = ref(false);
const openSplit = ref(false);
const openAbout = ref(false);
const toggleDarkMode = () => {};
</script>

<template>
    <div class="flex flex-col min-h-screen bg-gray-100 dark:bg-gray-800">
        <header>
            <nav class="min-w-sm bg-white shadow-sm dark:bg-gray-900 dark:text-gray-400">
                <div class="max-w-7xl min-w-sm flex flex-wrap items-center justify-between mx-auto p-4">
                    <!-- logo container -->
                    <router-link to="/" class="flex flex-col">
                        <div class="flex items-center">
                            <svg viewBox="0 20 120 80" fill="none" xmlns="http://www.w3.org/2000/svg" class="flex-shrink-0">
                                <!-- DNA Helix -->
                                <path d="M20 40C35 40 45 80 60 80C75 80 85 40 100 40" stroke="#94A3B8" stroke-width="6" stroke-linecap="round" opacity="0.3" />
                                <path d="M20 80C35 80 45 40 60 40C75 40 85 80 100 80" stroke="#94A3B8" stroke-width="6" stroke-linecap="round" opacity="0.3" />
                                <!-- Protein Ribbon -->
                                <path d="M15 60C15 60 40 20 60 60C80 100 105 60 105 60" stroke="#0F52BA" stroke-width="8" stroke-linecap="round" fill="none" filter="url(#glow)" />
                            </svg>
                            <div class="text-4xl font-bold tracking-tighter text-[#1E293B] flex items-baseline leading-none dark:text-gray-400"><span>PS</span><span class="font-light text-[#0F52BA]">kit</span></div>
                        </div>
                    </router-link>

                    <!-- menu -->
                    <div class="w-auto text-gray-700 dark:text-gray-400">
                        <ul class="flex flex-row font-medium p-0 mt-0 space-x-10">
                            <!-- Annotate Dropdown -->
                            <!-- Binding Dropdown -->
                            <li class="relative" @mouseenter="openAnnotate = true" @mouseleave="openAnnotate = false">
                                <button class="flex items-center space-x-2 hover:text-blue-700 dark:hover:text-blue-400">
                                    Binding
                                    <svg class="w-2.5 h-2.5 ms-2.5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 10 6">
                                        <path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 4 4 4-4" />
                                    </svg>
                                </button>
                                <div v-show="openAnnotate" class="absolute w-40 left-1/2 transform -translate-x-1/2 font-normal z-50">
                                    <div class="mt-2 bg-white divide-y divide-gray-100 rounded-lg shadow-sm dark:bg-gray-700 dark:divide-gray-600">
                                        <ul class="block py-2 text-sm text-center">
                                            <li><router-link to="/binding/annotate" class="block px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-blue-400">Annotate</router-link></li>
                                            <li><router-link to="/binding/predict" class="block px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-blue-400">Predict</router-link></li>
                                        </ul>
                                    </div>
                                </div>
                            </li>
                            <!-- Features Dropdown -->
                            <li class="relative" @mouseenter="openFeature = true" @mouseleave="openFeature = false">
                                <button class="flex items-center space-x-2 hover:text-blue-700 dark:hover:text-blue-400">
                                    Features
                                    <svg class="w-2.5 h-2.5 ms-2.5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 10 6">
                                        <path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 4 4 4-4" />
                                    </svg>
                                </button>
                                <div v-show="openFeature" class="absolute w-40 left-1/2 transform -translate-x-1/2 font-normal z-50">
                                    <div class="mt-2 bg-white divide-y divide-gray-100 rounded-lg shadow-sm dark:bg-gray-700 dark:divide-gray-600">
                                        <ul class="block py-2 text-sm text-center text-gray-700 dark:text-gray-400">
                                            <li><router-link to="/features/structural" class="block px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-blue-400">Structural</router-link></li>
                                            <li><router-link to="/features/language-model" class="block px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-blue-400">Language Model</router-link></li>
                                        </ul>
                                    </div>
                                </div>
                            </li>
                            <!-- Tools Dropdown -->
                            <li class="relative" @mouseenter="openSplit = true" @mouseleave="openSplit = false">
                                <button class="flex items-center space-x-2 hover:text-blue-700 dark:hover:text-blue-400">
                                    Tools
                                    <svg class="w-2.5 h-2.5 ms-2.5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 10 6">
                                        <path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 4 4 4-4" />
                                    </svg>
                                </button>
                                <div v-show="openSplit" class="absolute w-40 left-1/2 transform -translate-x-1/2 font-normal z-50">
                                    <div class="mt-2 bg-white divide-y divide-gray-100 rounded-lg shadow-sm dark:bg-gray-700 dark:divide-gray-600">
                                        <ul class="block py-2 text-sm text-center text-gray-700 dark:text-gray-400">
                                            <li><router-link to="/tools/split" class="block px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-blue-400">Split Complex</router-link></li>
                                            <li><router-link to="/tools/extract" class="block px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-blue-400">Extract Fragment</router-link></li>
                                        </ul>
                                    </div>
                                </div>
                            </li>

                            <li><router-link to="/contact-map" class="hover:text-blue-700 dark:hover:text-blue-400">Contact Map</router-link></li>
                            <li><router-link to="/viewer" class="hover:text-blue-700 dark:hover:text-blue-400">Viewer</router-link></li>
                            <li class="relative" @mouseenter="openAbout = true" @mouseleave="openAbout = false">
                                <button class="flex items-center space-x-2 hover:text-blue-700 dark:hover:text-blue-400">
                                    About
                                    <svg class="w-2.5 h-2.5 ms-2.5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 10 6">
                                        <path stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m1 1 4 4 4-4" />
                                    </svg>
                                </button>
                                <div v-show="openAbout" class="absolute w-40 left-1/2 transform -translate-x-1/2 font-normal z-50">
                                    <div class="mt-2 bg-white divide-y divide-gray-100 rounded-lg shadow-sm dark:bg-gray-700 dark:divide-gray-600">
                                        <ul class="block py-2 text-sm text-center text-gray-700 dark:text-gray-400">
                                            <li>
                                                <a href="https://github.com/hkhkhove/pskit" class="flex items-center justify-center space-x-3 px-4 py-2 hover:bg-gray-100 dark:hover:bg-gray-600 dark:hover:text-blue-400">
                                                    <svg height="16" aria-hidden="true" viewBox="0 0 24 24" version="1.1" width="16" data-view-component="true" class="octicon octicon-mark-github v-align-middle">
                                                        <path
                                                            d="M12 1C5.9225 1 1 5.9225 1 12C1 16.8675 4.14875 20.9787 8.52125 22.4362C9.07125 22.5325 9.2775 22.2025 9.2775 21.9137C9.2775 21.6525 9.26375 20.7862 9.26375 19.865C6.5 20.3737 5.785 19.1912 5.565 18.5725C5.44125 18.2562 4.905 17.28 4.4375 17.0187C4.0525 16.8125 3.5025 16.3037 4.42375 16.29C5.29 16.2762 5.90875 17.0875 6.115 17.4175C7.105 19.0812 8.68625 18.6137 9.31875 18.325C9.415 17.61 9.70375 17.1287 10.02 16.8537C7.5725 16.5787 5.015 15.63 5.015 11.4225C5.015 10.2262 5.44125 9.23625 6.1425 8.46625C6.0325 8.19125 5.6475 7.06375 6.2525 5.55125C6.2525 5.55125 7.17375 5.2625 9.2775 6.67875C10.1575 6.43125 11.0925 6.3075 12.0275 6.3075C12.9625 6.3075 13.8975 6.43125 14.7775 6.67875C16.8813 5.24875 17.8025 5.55125 17.8025 5.55125C18.4075 7.06375 18.0225 8.19125 17.9125 8.46625C18.6138 9.23625 19.04 10.2125 19.04 11.4225C19.04 15.6437 16.4688 16.5787 14.0213 16.8537C14.42 17.1975 14.7638 17.8575 14.7638 18.8887C14.7638 20.36 14.75 21.5425 14.75 21.9137C14.75 22.2025 14.9563 22.5462 15.5063 22.4362C19.8513 20.9787 23 16.8537 23 12C23 5.9225 18.0775 1 12 1Z"></path>
                                                    </svg>
                                                    <span>Source Code</span>
                                                </a>
                                            </li>
                                        </ul>
                                    </div>
                                </div>
                            </li>
                        </ul>
                    </div>
                </div>
            </nav>
        </header>
        <main>
            <router-view />
        </main>
        <footer class="mt-auto text-center p-4">
            <aside class="text-sm text-gray-700 dark:text-gray-500">
                <p>
                    Copyright © 2025 BioAILab |
                    <a href="https://beian.miit.gov.cn/" target="_blank" class="hover:underline">粤ICP备2025419210号</a>
                </p>
            </aside>
        </footer>
    </div>
</template>

<style scoped>
/* 可以在此处添加微调样式 */
</style>
