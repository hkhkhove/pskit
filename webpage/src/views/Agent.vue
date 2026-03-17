<script setup>
import { ref, onMounted, nextTick } from 'vue';
import { fetchEventSource } from '@microsoft/fetch-event-source';
import { marked } from 'marked';
import DOMPurify from 'dompurify';

const STREAM_FLUSH_INTERVAL_MS = 50;

const messages = ref([
    { role: 'assistant', content: 'Hello! I am the PSKit Agent. I can help you analyze protein and nucleic acid structures, predict binding sites, and calculate interaction maps. What can I help you with today?' }
]);
const inputMessage = ref('');
const isGenerating = ref(false);
const chatContainer = ref(null);
const sessionId = ref(null);
const markdownCache = new Map();
const knownSessionFiles = new Set();

onMounted(() => {
    sessionId.value = 'session_' + Math.random().toString(36).substring(2, 9) + Date.now();
});

const scrollToBottom = async () => {
    await nextTick();
    if (chatContainer.value) {
        chatContainer.value.scrollTop = chatContainer.value.scrollHeight;
    }
};

const renderMarkdown = (text) => {
    if (!text) return '';
    const cached = markdownCache.get(text);
    if (cached) return cached;
    const rendered = DOMPurify.sanitize(marked(text));
    markdownCache.set(text, rendered);
    return rendered;
};

const formatFileSize = (size) => {
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    return `${(size / (1024 * 1024)).toFixed(1)} MB`;
};

const fetchNewSessionFiles = async () => {
    if (!sessionId.value) return [];
    const resp = await fetch(`/api/agent/sessions/${encodeURIComponent(sessionId.value)}/files`);
    if (!resp.ok) return [];

    const payload = await resp.json();
    const files = Array.isArray(payload.files) ? payload.files : [];

    const newFiles = [];
    for (const file of files) {
        if (!file?.path || knownSessionFiles.has(file.path)) continue;
        knownSessionFiles.add(file.path);
        newFiles.push(file);
    }
    return newFiles;
};

const sendMessage = async () => {
    if (!inputMessage.value.trim() || isGenerating.value) return;

    const userMsg = inputMessage.value;
    inputMessage.value = '';

    messages.value.push({ role: 'user', content: userMsg });
    scrollToBottom();

    isGenerating.value = true;

    const assistantMsgIndex = messages.value.length;
    messages.value.push({
        role: 'assistant',
        content: '',
        toolCalls: []
    });

    let currentAssistantContent = '';
    let pendingAssistantContent = '';
    let flushTimer = null;

    const flushAssistantContent = () => {
        if (!pendingAssistantContent) return;
        currentAssistantContent += pendingAssistantContent;
        pendingAssistantContent = '';
        messages.value[assistantMsgIndex].content = currentAssistantContent;
        scrollToBottom();
    };

    const scheduleFlush = () => {
        if (flushTimer !== null) return;
        flushTimer = window.setTimeout(() => {
            flushTimer = null;
            flushAssistantContent();
        }, STREAM_FLUSH_INTERVAL_MS);
    };

    const forceFlush = () => {
        if (flushTimer !== null) {
            clearTimeout(flushTimer);
            flushTimer = null;
        }
        flushAssistantContent();
    };

    try {
        await fetchEventSource('/api/agent', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                message: userMsg,
                session_id: sessionId.value
            }),
            async onopen(response) {
                if (response.ok) return;
                throw new Error('Failed to connect to agent API');
            },
            onmessage(msg) {
                if (!msg.data) return;
                if (msg.data === '[DONE]') {
                    isGenerating.value = false;
                    return;
                }

                try {
                    const event = JSON.parse(msg.data);

                    if (event.type === 'text') {
                        pendingAssistantContent += event.content;
                        scheduleFlush();
                    }
                    else if (event.type === 'tool_call') {
                        messages.value[assistantMsgIndex].toolCalls.push({
                            name: event.name,
                            args: event.args,
                            status: 'running',
                            files: []
                        });
                    }
                    else if (event.type === 'tool_result') {
                        const calls = messages.value[assistantMsgIndex].toolCalls;
                        if (calls.length > 0) {
                            calls[calls.length - 1].status = 'done';
                            calls[calls.length - 1].result = event.content;

                            const currentCall = calls[calls.length - 1];
                            void fetchNewSessionFiles()
                                .then((newFiles) => {
                                    if (newFiles.length > 0) {
                                        currentCall.files = newFiles;
                                        scrollToBottom();
                                    }
                                })
                                .catch((e) => {
                                    console.error('Failed to fetch session files:', e);
                                });
                        }
                    }
                    else if (event.type === 'error') {
                        messages.value.push({
                            role: 'system',
                            content: 'Error: ' + event.message,
                            isError: true
                        });
                        isGenerating.value = false;
                    }
                    else if (event.type === 'done') {
                        forceFlush();
                        isGenerating.value = false;
                    }

                    if (event.type !== 'text') {
                        scrollToBottom();
                    }
                } catch (e) {
                    console.error('Error parsing message:', e, msg.data);
                }
            },
            onclose() {
                forceFlush();
                isGenerating.value = false;
            },
            onerror(err) {
                console.error('SSE Error:', err);
                forceFlush();
                messages.value.push({
                    role: 'system',
                    content: 'Connection error. Please try again.',
                    isError: true
                });
                isGenerating.value = false;
                throw err;
            }
        });
    } catch (err) {
        console.error('Chat error:', err);
        isGenerating.value = false;
    }
};
</script>

<template>
    <div class="flex flex-col h-[calc(100vh-4rem)] max-w-5xl mx-auto w-full p-4">
        <div class="bg-white dark:bg-gray-800 rounded-xl shadow-md flex-1 overflow-hidden flex flex-col">

            <div
                class="bg-gray-50 dark:bg-gray-900 px-6 py-4 border-b border-gray-200 dark:border-gray-700 flex justify-between items-center">
                <div class="flex items-center space-x-3">
                    <div class="w-10 h-10 rounded-full bg-blue-100 flex items-center justify-center text-blue-600">
                        <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                d="M13 10V3L4 14h7v7l9-11h-7z"></path>
                        </svg>
                    </div>
                    <div>
                        <h2 class="text-lg font-bold text-gray-800 dark:text-white">PSKit Agent</h2>
                        <p class="text-xs text-gray-500">Powered by LLM & PSKit Tools</p>
                    </div>
                </div>
            </div>

            <div ref="chatContainer" class="flex-1 overflow-y-auto p-6 space-y-6 bg-white dark:bg-gray-800">
                <div v-for="(msg, index) in messages" :key="index" class="flex flex-col">

                    <div v-if="msg.role === 'user'"
                        class="self-end max-w-[80%] bg-blue-600 text-white rounded-2xl rounded-tr-sm px-5 py-3 shadow-sm">
                        {{ msg.content }}
                    </div>

                    <div v-else-if="msg.role === 'system'"
                        class="self-center bg-red-100 text-red-700 text-sm px-4 py-2 rounded-lg my-2">
                        {{ msg.content }}
                    </div>

                    <div v-else class="self-start max-w-[85%] flex space-x-3">
                        <div
                            class="w-8 h-8 rounded-full bg-blue-100 flex-shrink-0 flex items-center justify-center mt-1">
                            🤖
                        </div>
                        <div class="flex flex-col space-y-2 w-full">
                            <div v-if="msg.toolCalls && msg.toolCalls.length > 0" class="flex flex-col space-y-2 mb-2">
                                <div v-for="(tool, tIdx) in msg.toolCalls" :key="tIdx"
                                    class="bg-gray-50 dark:bg-gray-700 rounded-lg p-3 text-sm border border-gray-200 dark:border-gray-600 shadow-sm w-full">
                                    <div
                                        class="flex items-center space-x-2 text-gray-700 dark:text-gray-300 font-medium mb-1">
                                        <svg v-if="tool.status === 'running'" class="animate-spin h-4 w-4 text-blue-500"
                                            xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor"
                                                stroke-width="4"></circle>
                                            <path class="opacity-75" fill="currentColor"
                                                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z">
                                            </path>
                                        </svg>
                                        <svg v-else class="h-4 w-4 text-green-500" fill="none" stroke="currentColor"
                                            viewBox="0 0 24 24">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                                d="M5 13l4 4L19 7"></path>
                                        </svg>
                                        <span>Using tool: <span class="text-blue-600 font-mono">{{ tool.name
                                                }}</span></span>
                                    </div>
                                    <div
                                        class="text-xs text-gray-500 font-mono bg-gray-200 dark:bg-gray-800 p-2 rounded mt-1 overflow-x-auto">
                                        {{ tool.args }}
                                    </div>
                                    <div v-if="tool.files && tool.files.length > 0"
                                        class="mt-2 rounded bg-white/60 dark:bg-gray-800/60 border border-gray-200 dark:border-gray-600 p-2">
                                        <div class="text-xs font-semibold text-gray-600 dark:text-gray-300 mb-1">Output files</div>
                                        <div class="space-y-1">
                                            <a v-for="(file, fIdx) in tool.files" :key="fIdx" :href="file.download_url" download
                                                class="flex items-center justify-between text-xs text-blue-600 hover:text-blue-700 hover:underline">
                                                <span class="font-mono truncate max-w-[70%]">{{ file.path }}</span>
                                                <span class="text-gray-500">{{ formatFileSize(file.size || 0) }}</span>
                                            </a>
                                        </div>
                                    </div>
                                </div>
                            </div>

                            <div v-if="msg.content"
                                class="bg-gray-50 dark:bg-gray-700 text-gray-800 dark:text-gray-200 rounded-2xl rounded-tl-sm px-5 py-4 shadow-sm prose prose-sm max-w-none dark:prose-invert"
                                v-html="renderMarkdown(msg.content)">
                            </div>

                            <div v-if="isGenerating && index === messages.length - 1 && !msg.content && (!msg.toolCalls || msg.toolCalls.length === 0)"
                                class="flex space-x-1 p-2">
                                <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce"></div>
                                <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce"
                                    style="animation-delay: 0.2s"></div>
                                <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce"
                                    style="animation-delay: 0.4s"></div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <div class="bg-white dark:bg-gray-900 px-6 py-4 border-t border-gray-200 dark:border-gray-700">
                <form @submit.prevent="sendMessage" class="flex items-center space-x-4">
                    <input v-model="inputMessage" type="text"
                        placeholder="Ask PSKit Agent to process structures or predict interactions..."
                        class="flex-1 bg-gray-100 dark:bg-gray-800 text-gray-800 dark:text-gray-200 rounded-full px-6 py-3 focus:outline-none focus:ring-2 focus:ring-blue-500 transition"
                        :disabled="isGenerating">
                    <button type="submit"
                        class="bg-blue-600 hover:bg-blue-700 text-white rounded-full p-3 transition focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed flex-shrink-0"
                        :disabled="!inputMessage.trim() || isGenerating">
                        <svg class="w-6 h-6 transform rotate-90" fill="currentColor" viewBox="0 0 20 20">
                            <path
                                d="M10.894 2.553a1 1 0 00-1.788 0l-7 14a1 1 0 001.169 1.409l5-1.429A1 1 0 009 15.571V11a1 1 0 112 0v4.571a1 1 0 00.725.962l5 1.428a1 1 0 001.17-1.408l-7-14z">
                            </path>
                        </svg>
                    </button>
                </form>
            </div>

        </div>
    </div>
</template>

<style scoped>
.prose :where(p):not(:where([class~="not-prose"] *)) {
    margin-top: 0.5em;
    margin-bottom: 0.5em;
}

.prose :where(pre):not(:where([class~="not-prose"] *)) {
    background-color: #f1f5f9;
    color: #334155;
}
</style>
