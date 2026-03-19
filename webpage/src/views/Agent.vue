<script setup>
import { computed, ref, onMounted, onUnmounted, nextTick } from 'vue';
import { fetchEventSource } from '@microsoft/fetch-event-source';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { nanoid } from 'nanoid';

const STREAM_FLUSH_INTERVAL_MS = 50;
const TOOL_RESULT_COLLAPSE_CHARS = 800;
const STORAGE_SESSIONS_KEY = 'pskit_agent_sessions_v1';
const STORAGE_ACTIVE_SESSION_KEY = 'pskit_agent_active_session_v1';
const NEW_SESSION_TITLE = 'New Chat';

const createWelcomeMessages = () => ([
    {
        role: 'assistant',
        content: 'Hello! I am the PSKit Agent. I can help you analyze protein and nucleic acid structures, predict binding sites, and calculate interaction maps. What can I help you with today?'
    }
]);

const messages = ref(createWelcomeMessages());
const inputMessage = ref('');
const isGenerating = ref(false);
const chatContainer = ref(null);
const sessionId = ref(null);
const sessions = ref([]);
const sidebarVisible = ref(true);
const markdownCache = new Map();
let abortController = null;
const hasHistorySessions = computed(() => sessions.value.length > 0);

const saveSessionsToStorage = () => {
    const persisted = sessions.value
        .filter((s) => !s.isDraft)
        .map((s) => ({
            id: s.id,
            title: s.title,
            updated_at: s.updated_at
        }));
    localStorage.setItem(STORAGE_SESSIONS_KEY, JSON.stringify(persisted));
};

const saveActiveSessionToStorage = () => {
    if (!sessionId.value) return;
    localStorage.setItem(STORAGE_ACTIVE_SESSION_KEY, sessionId.value);
};

const sortSessions = () => {
    sessions.value.sort((a, b) => (b.updated_at || '').localeCompare(a.updated_at || ''));
};

const upsertSession = (id, titleHint = '', options = {}) => {
    const now = new Date().toISOString();
    const normalizedHint = String(titleHint || '').trim();
    const title = normalizedHint || NEW_SESSION_TITLE;
    const draftMode = options.draft === true;
    const persistNow = options.persist === true;
    const existing = sessions.value.find((s) => s.id === id);

    if (existing) {
        existing.updated_at = now;
        if (normalizedHint) {
            existing.title = title;
        }
        if (persistNow) {
            existing.isDraft = false;
        }
    } else {
        sessions.value.push({
            id,
            title,
            updated_at: now,
            isDraft: draftMode
        });
    }

    sortSessions();
    if (persistNow || !draftMode) {
        saveSessionsToStorage();
    }
};

const createNewSession = () => {
    if (isGenerating.value) {
        stopGeneration();
    }

    // Keep at most one draft chat in the list.
    sessions.value = sessions.value.filter((s) => !s.isDraft);

    sessionId.value = nanoid();
    messages.value = createWelcomeMessages();
    upsertSession(sessionId.value, NEW_SESSION_TITLE, { draft: true });
    sidebarVisible.value = true;
    saveActiveSessionToStorage();
    scrollToBottom();
};

const hydrateMessagesFromHistory = (history) => {
    const hydrated = [];
    for (let i = 0; i < history.length; i += 1) {
        const item = history[i];
        if (!item || typeof item !== 'object') continue;
        const role = item.role;

        if (role === 'user') {
            hydrated.push({ role: 'user', content: item.content || '' });
        } else if (role === 'assistant') {
            const toolCallsRaw = Array.isArray(item.tool_calls) ? item.tool_calls : [];
            const toolCalls = toolCallsRaw.map((tc) => {
                const fn = tc?.function || {};
                return {
                    id: tc?.id || '',
                    name: fn?.name || '',
                    args: typeof fn?.arguments === 'string'
                        ? fn.arguments
                        : JSON.stringify(fn?.arguments || {}),
                    status: 'done',
                    result: '',
                    showFullResult: false,
                    files: []
                };
            });

            // The saved history stores tool messages right after assistant.tool_calls.
            if (toolCalls.length > 0) {
                const byId = new Map(toolCalls.map((tc, idx) => [tc.id, idx]));
                let j = i + 1;
                while (j < history.length && history[j]?.role === 'tool') {
                    const toolMsg = history[j] || {};
                    const toolCallId = toolMsg.tool_call_id || '';
                    const result = toolMsg.content || '';

                    if (toolCallId && byId.has(toolCallId)) {
                        toolCalls[byId.get(toolCallId)].result = result;
                    } else {
                        // Fallback for malformed IDs: fill first empty tool result in order.
                        const firstEmpty = toolCalls.find((tc) => !tc.result);
                        if (firstEmpty) firstEmpty.result = result;
                    }
                    j += 1;
                }
                i = j - 1;
            }

            hydrated.push({
                role: 'assistant',
                content: item.content || '',
                toolCalls
            });
        }
    }
    return hydrated.length > 0 ? hydrated : createWelcomeMessages();
};

const loadSessionHistory = async (id) => {
    try {
        const resp = await fetch(`/api/agent/sessions/${encodeURIComponent(id)}/history`);
        if (!resp.ok) {
            messages.value = createWelcomeMessages();
            return;
        }
        const history = await resp.json();
        messages.value = hydrateMessagesFromHistory(Array.isArray(history) ? history : []);
        await attachFilesToHydratedToolCalls();
    } catch (e) {
        console.error('Failed to load session history:', e);
        messages.value = createWelcomeMessages();
    } finally {
        scrollToBottom();
    }
};

const switchSession = async (id) => {
    if (!id || id === sessionId.value) return;
    if (isGenerating.value) {
        stopGeneration();
    }
    sessionId.value = id;
    saveActiveSessionToStorage();
    await loadSessionHistory(id);
};

const initializeSessions = async () => {
    let storedSessions = [];
    try {
        storedSessions = JSON.parse(localStorage.getItem(STORAGE_SESSIONS_KEY) || '[]');
    } catch (e) {
        console.error('Failed to parse local sessions:', e);
    }

    sessions.value = Array.isArray(storedSessions)
        ? storedSessions
            .filter((s) => s && typeof s.id === 'string')
            .map((s) => ({ ...s, isDraft: false }))
        : [];
    sortSessions();
    sidebarVisible.value = true;

    const preferredId = localStorage.getItem(STORAGE_ACTIVE_SESSION_KEY);
    const validPreferred = preferredId && sessions.value.some((s) => s.id === preferredId);
    if (validPreferred) {
        sessionId.value = preferredId;
        await loadSessionHistory(preferredId);
    } else if (sessions.value.length > 0) {
        sessionId.value = sessions.value[0].id;
        saveActiveSessionToStorage();
        await loadSessionHistory(sessionId.value);
    } else {
        createNewSession();
    }
};

const deleteSession = async (id) => {
    if (!id) return;
    if (!window.confirm('Delete this chat history? This action cannot be undone.')) return;

    try {
        await fetch(`/api/agent/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });
    } catch (e) {
        console.error('Failed to delete session from server:', e);
    }

    sessions.value = sessions.value.filter((s) => s.id !== id);
    saveSessionsToStorage();

    if (sessionId.value === id) {
        const next = sessions.value[0]?.id;
        if (next) {
            await switchSession(next);
        } else {
            localStorage.removeItem(STORAGE_ACTIVE_SESSION_KEY);
            createNewSession();
        }
    }
};

onMounted(() => {
    void initializeSessions();
});

onUnmounted(() => {
    if (abortController) {
        abortController.abort();
    }
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

const isLongToolResult = (text) => {
    return String(text || '').length > TOOL_RESULT_COLLAPSE_CHARS;
};

const getToolResultPreview = (text) => {
    const raw = String(text || '');
    if (raw.length <= TOOL_RESULT_COLLAPSE_CHARS) return raw;
    return raw.slice(0, TOOL_RESULT_COLLAPSE_CHARS) + '\n...';
};

const toggleToolResultExpanded = (tool) => {
    tool.showFullResult = !tool.showFullResult;
};

const fetchToolSessionFiles = async (toolCallId) => {
    if (!sessionId.value) return [];
    if (!toolCallId || !String(toolCallId).trim()) return [];

    const params = new URLSearchParams({ tool_call_id: String(toolCallId).trim() });
    const resp = await fetch(`/api/agent/sessions/${encodeURIComponent(sessionId.value)}/files?${params.toString()}`);
    if (!resp.ok) return [];

    const payload = await resp.json();
    const files = Array.isArray(payload.files) ? payload.files : [];
    return files
        .filter((file) => !!file?.path)
        .map((file) => {
            const filename = file?.filename || String(file.path || '').split('/').pop() || '';
            const toolCall = file?.tool_call_id || String(toolCallId).trim();
            return {
                ...file,
                filename,
                tool_call_id: toolCall,
                download_url: `/api/agent/sessions/${encodeURIComponent(sessionId.value)}/${encodeURIComponent(toolCall)}/${encodeURIComponent(filename)}`
            };
        });
};

const attachFilesToHydratedToolCalls = async () => {
    const pending = [];
    for (const msg of messages.value) {
        if (!msg || msg.role !== 'assistant' || !Array.isArray(msg.toolCalls)) continue;
        for (const toolCall of msg.toolCalls) {
            if (!toolCall?.id) continue;
            pending.push(
                fetchToolSessionFiles(toolCall.id)
                    .then((toolFiles) => {
                        toolCall.files = toolFiles;
                    })
                    .catch((e) => {
                        console.error('Failed to load tool files for history:', e);
                        toolCall.files = [];
                    })
            );
        }
    }

    if (pending.length > 0) {
        await Promise.all(pending);
    }
};

const sendMessage = async () => {
    if (!inputMessage.value.trim() || isGenerating.value) return;
    if (!sessionId.value) {
        createNewSession();
    }

    const userMsg = inputMessage.value;
    inputMessage.value = '';
    upsertSession(sessionId.value, '', { persist: true });
    sidebarVisible.value = true;
    saveActiveSessionToStorage();

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

    if (abortController) {
        abortController.abort();
    }
    //在组件卸载或发送新消息时取消之前的SSE连接，避免内存泄漏和重复消息
    abortController = new AbortController();

    try {
        await fetchEventSource('/api/agent', {
            method: 'POST',
            signal: abortController.signal,
            openWhenHidden: true, //允许在标签页不可见时保持连接，不然一回到标签页会重新发信息到后端
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
                    else if (event.type === 'session_title') {
                        if (event.session_id === sessionId.value && event.title) {
                            upsertSession(sessionId.value, event.title, { persist: true });
                        }
                    }
                    else if (event.type === 'tool_call') {
                        messages.value[assistantMsgIndex].toolCalls.push({
                            id: event.tool_call_id || '',
                            name: event.name,
                            args: event.args,
                            status: 'running',
                            waitMessage: '',
                            result: '',
                            showFullResult: false,
                            files: []
                        });
                    }
                    else if (event.type === 'tool_status') {
                        const calls = messages.value[assistantMsgIndex].toolCalls;
                        if (calls.length > 0) {
                            const byId = calls.find((c) => c.id && c.id === event.tool_call_id);
                            const currentCall = byId || calls[calls.length - 1];
                            if (event.status) {
                                currentCall.status = event.status;
                            }
                            currentCall.waitMessage = event.message || '';
                        }
                    }
                    else if (event.type === 'tool_result') {
                        const calls = messages.value[assistantMsgIndex].toolCalls;
                        if (calls.length > 0) {
                            const byId = calls.find((c) => c.id && c.id === event.tool_call_id);
                            const currentCall = byId || calls[calls.length - 1];
                            currentCall.status = 'done';
                            currentCall.waitMessage = '';
                            currentCall.result = event.content;
                            if (!isLongToolResult(currentCall.result)) {
                                currentCall.showFullResult = true;
                            }

                            void fetchToolSessionFiles(currentCall.id)
                                .then((toolFiles) => {
                                    if (toolFiles.length > 0) {
                                        currentCall.files = toolFiles;
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
                        upsertSession(sessionId.value, '', { persist: true });
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
        if (err.name === 'AbortError') {
            console.log('Generation stopped by user');
        } else {
            console.error('Chat error:', err);
        }
        isGenerating.value = false;
    }
};

const stopGeneration = () => {
    if (abortController) {
        abortController.abort();
        abortController = null;
    }
    isGenerating.value = false;
};
</script>

<template>
    <div class="flex h-[calc(100vh-4rem)] max-w-7xl mx-auto w-full p-4 gap-4">
        <aside v-if="hasHistorySessions && sidebarVisible"
            class="w-80 bg-white dark:bg-gray-800 rounded-xl shadow-md border border-gray-200 dark:border-gray-700 flex flex-col">
            <div class="px-4 py-4 border-b border-gray-200 dark:border-gray-700">
                <button
                    class="w-full bg-blue-600 hover:bg-blue-700 text-white rounded-lg px-4 py-2 text-sm font-medium transition"
                    @click="createNewSession">
                    New Chat
                </button>
            </div>
            <div class="flex-1 overflow-y-auto p-2 space-y-2">
                <div v-for="item in sessions" :key="item.id"
                    class="group rounded-lg border px-3 py-2 cursor-pointer transition"
                    :class="item.id === sessionId
                        ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20'
                        : 'border-gray-200 dark:border-gray-700 hover:border-blue-300 hover:bg-gray-50 dark:hover:bg-gray-700/40'" @click="switchSession(item.id)">
                    <div class="flex items-start justify-between gap-2">
                        <div class="min-w-0">
                            <div class="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">{{ item.title ||
                                'New Chat' }}</div>
                            <div class="text-xs text-gray-500 mt-1">{{ new Date(item.updated_at).toLocaleString() }}
                            </div>
                        </div>
                        <button class="opacity-0 group-hover:opacity-100 text-gray-400 hover:text-red-500 transition"
                            title="Delete" @click.stop="deleteSession(item.id)">
                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                    d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M9 7h6m-5-3h4a1 1 0 011 1v2H9V5a1 1 0 011-1z" />
                            </svg>
                        </button>
                    </div>
                </div>
            </div>
        </aside>

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
                        <p class="text-xs text-gray-500">Powered by LLM & PSKit Tools · {{ sessionId }}</p>
                    </div>
                </div>
                <button v-if="hasHistorySessions"
                    class="text-xs px-3 py-1.5 rounded-md border border-gray-300 dark:border-gray-600 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition"
                    @click="sidebarVisible = !sidebarVisible">
                    {{ sidebarVisible ? 'Hide History' : 'Show History' }}
                </button>
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

                    <div v-else class="self-start max-w-[96%] flex space-x-3 w-full">
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
                                        <svg v-if="tool.status === 'running' || tool.status === 'waiting'"
                                            :class="tool.status === 'waiting' ? 'animate-spin h-4 w-4 text-amber-500' : 'animate-spin h-4 w-4 text-blue-500'"
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
                                    <div v-if="tool.status === 'waiting'" class="text-xs text-amber-600 mt-1">
                                        {{ tool.waitMessage || 'Waiting for available worker slot...' }}
                                    </div>
                                    <div v-if="tool.result"
                                        class="mt-2 rounded bg-white/80 dark:bg-gray-900/50 border border-gray-200 dark:border-gray-600 p-2">
                                        <div class="flex items-center justify-between mb-1">
                                            <div class="text-xs font-semibold text-gray-600 dark:text-gray-300">Tool
                                                output</div>
                                            <button v-if="isLongToolResult(tool.result)"
                                                @click="toggleToolResultExpanded(tool)"
                                                class="text-xs text-blue-600 hover:text-blue-700 hover:underline">
                                                {{ tool.showFullResult ? 'Hide' : 'Show all' }}
                                            </button>
                                        </div>
                                        <pre
                                            class="text-xs text-gray-700 dark:text-gray-200 font-mono whitespace-pre-wrap break-words overflow-auto rounded bg-gray-100 dark:bg-gray-800 p-2 max-h-56">{{ tool.showFullResult ? tool.result : getToolResultPreview(tool.result) }}</pre>
                                    </div>
                                    <div v-if="tool.files && tool.files.length > 0"
                                        class="mt-2 rounded bg-white/60 dark:bg-gray-800/60 border border-gray-200 dark:border-gray-600 p-2">
                                        <div class="text-xs font-semibold text-gray-600 dark:text-gray-300 mb-1">Output
                                            files</div>
                                        <div class="space-y-1">
                                            <a v-for="(file, fIdx) in tool.files" :key="fIdx" :href="file.download_url"
                                                download
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
                    <button v-if="!isGenerating" type="submit"
                        class="bg-blue-600 hover:bg-blue-700 text-white rounded-full p-3 transition focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed flex-shrink-0"
                        :disabled="!inputMessage.trim()">
                        <svg class="w-6 h-6 transform rotate-90" fill="currentColor" viewBox="0 0 20 20">
                            <path
                                d="M10.894 2.553a1 1 0 00-1.788 0l-7 14a1 1 0 001.169 1.409l5-1.429A1 1 0 009 15.571V11a1 1 0 112 0v4.571a1 1 0 00.725.962l5 1.428a1 1 0 001.17-1.408l-7-14z">
                            </path>
                        </svg>
                    </button>
                    <button v-else type="button" @click="stopGeneration" title="Stop Generation"
                        class="bg-red-500 hover:bg-red-600 text-white rounded-full p-3 transition focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2 flex-shrink-0">
                        <svg class="w-6 h-6" fill="currentColor" viewBox="0 0 20 20">
                            <rect x="5" y="5" width="10" height="10" rx="2" ry="2"></rect>
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
