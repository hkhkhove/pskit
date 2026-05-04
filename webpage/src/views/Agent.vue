<script setup>
import { computed, ref, onMounted, onUnmounted, nextTick } from 'vue';
import { fetchEventSource } from '@microsoft/fetch-event-source';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { nanoid } from 'nanoid';
import AgentContactMapViewer from '../components/AgentContactMapViewer.vue';
import AgentStructureViewer from '../components/AgentStructureViewer.vue';

const STREAM_FLUSH_INTERVAL_MS = 50;
const MAX_UPLOAD_FILES = 5;
const STORAGE_SESSIONS_KEY = 'pskit_agent_sessions_v1';
const STORAGE_ACTIVE_SESSION_KEY = 'pskit_agent_active_session_v1';
const NEW_SESSION_TITLE = 'New Chat';

marked.setOptions({
    gfm: true,
    breaks: false
});

const createWelcomeMessages = () => ([
    {
        role: 'assistant',
        content: 'Hello! I am the PSKit Agent. I can help you analyze protein and nucleic acid structures, predict binding sites, and calculate interaction maps. What can I help you with today?'
    }
]);

const messages = ref(createWelcomeMessages());
const inputMessage = ref('');
const isGenerating = ref(false);
const isUploading = ref(false);
const chatContainer = ref(null);
const fileInput = ref(null);
const sessionId = ref(null);
const sessions = ref([]);
const pendingFiles = ref([]);
const sessionFiles = ref([]);
const sidebarVisible = ref(true);
const markdownCache = new Map();
let abortController = null;
const hasHistorySessions = computed(() => sessions.value.length > 0);

const toolStatusMeta = {
    running: {
        label: 'Running',
        icon: 'spinner',
        badgeClass: 'bg-blue-50 text-blue-700 ring-blue-200 dark:bg-blue-950/40 dark:text-blue-200 dark:ring-blue-800',
        dotClass: 'bg-blue-500'
    },
    waiting: {
        label: 'Queued',
        icon: 'spinner',
        badgeClass: 'bg-amber-50 text-amber-700 ring-amber-200 dark:bg-amber-950/40 dark:text-amber-200 dark:ring-amber-800',
        dotClass: 'bg-amber-500'
    },
    stopped: {
        label: 'Stopped',
        icon: 'stop',
        badgeClass: 'bg-rose-50 text-rose-700 ring-rose-200 dark:bg-rose-950/40 dark:text-rose-200 dark:ring-rose-800',
        dotClass: 'bg-rose-500'
    },
    done: {
        label: 'Completed',
        icon: 'check',
        badgeClass: 'bg-emerald-50 text-emerald-700 ring-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-200 dark:ring-emerald-800',
        dotClass: 'bg-emerald-500'
    }
};

const getToolStatusMeta = (status) => toolStatusMeta[status] || toolStatusMeta.done;

const formatToolArgs = (args) => {
    if (!args) return 'No arguments';
    if (typeof args !== 'string') {
        return JSON.stringify(args, null, 2);
    }
    try {
        return JSON.stringify(JSON.parse(args), null, 2);
    } catch {
        return args;
    }
};

const supportsStructurePreview = (tool) => {
    const name = String(tool?.name || '');
    if (!['predict_binding_sites', 'annotate_binding_pairs', 'annotate_binding_sites'].includes(name)) {
        return false;
    }
    const files = Array.isArray(tool?.files) ? tool.files : [];
    const suffix = name === 'predict_binding_sites' ? '_binding_sites.csv' : '_binding_pairs.csv';
    return files.some((file) => String(file?.filename || file?.path || '').endsWith(suffix));
};

const supportsContactMapPreview = (tool) => {
    if (String(tool?.name || '') !== 'calculate_contact_map') return false;
    const files = Array.isArray(tool?.files) ? tool.files : [];
    return files.some((file) => {
        const name = String(file?.filename || file?.path || '');
        return name.startsWith('contact_map_') && name.endsWith('.json');
    });
};

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
    pendingFiles.value = [];
    sessionFiles.value = [];
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
            hydrated.push({
                role: 'user',
                content: item.content || '',
                files: Array.isArray(item.files) ? item.files : []
            });
        } else if (role === 'assistant') {
            if (Array.isArray(item.toolCalls)) {
                hydrated.push({
                    role: 'assistant',
                    content: item.content || '',
                    toolCalls: item.toolCalls.map((tc) => ({
                        ...tc,
                        status: tc.status || 'done',
                        waitMessage: tc.waitMessage || '',
                        result: tc.result || '',
                        files: Array.isArray(tc.files) ? tc.files : [],
                        expanded: false
                    }))
                });
                continue;
            }

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
                    files: [],
                    expanded: false
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
    sessionFiles.value = [];
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

const isNearChatBottom = (threshold = 96) => {
    const el = chatContainer.value;
    if (!el) return true;
    return el.scrollHeight - el.scrollTop - el.clientHeight <= threshold;
};

const scrollToBottomIfFollowing = async (wasNearBottom) => {
    if (wasNearBottom) {
        await scrollToBottom();
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

const triggerFilePicker = () => {
    if (!isGenerating.value && !isUploading.value) {
        fileInput.value?.click();
    }
};

const handleFileSelection = (event) => {
    const selected = Array.from(event.target.files || []);
    const allowed = selected.filter((file) => /\.(cif|pdb)$/i.test(file.name));
    const remainingSlots = Math.max(0, MAX_UPLOAD_FILES - pendingFiles.value.length);
    pendingFiles.value = [...pendingFiles.value, ...allowed.slice(0, remainingSlots)];
    event.target.value = '';
};

const removePendingFile = (index) => {
    pendingFiles.value.splice(index, 1);
};

const uploadPendingFiles = async () => {
    if (pendingFiles.value.length === 0) return [];
    if (!sessionId.value) {
        createNewSession();
    }

    const formData = new FormData();
    for (const file of pendingFiles.value) {
        formData.append('files', file, file.name);
    }

    isUploading.value = true;
    try {
        const resp = await fetch(`/api/agent/sessions/${encodeURIComponent(sessionId.value)}/files`, {
            method: 'POST',
            body: formData
        });
        if (!resp.ok) {
            const text = await resp.text();
            throw new Error(text || 'Failed to upload files');
        }
        const payload = await resp.json();
        const files = Array.isArray(payload.files) ? payload.files : [];
        pendingFiles.value = [];
        await fetchAllSessionFiles();
        return files;
    } finally {
        isUploading.value = false;
    }
};

const buildAgentMessage = (userText, uploadedFiles) => {
    if (!uploadedFiles.length) return userText;
    const fileLines = uploadedFiles
        .map((file) => `- ${file.filename}: ${file.absolute_path}`)
        .join('\n');
    const prompt = userText.trim() || 'Please analyze the uploaded protein structure file(s).';
    return `${prompt}\n\n[Uploaded protein structure files]\n${fileLines}\nUse these absolute paths directly as pdb_path for tools when appropriate.`;
};

const markPendingToolCallsStopped = (message = 'Stopped by user.') => {
    let changed = false;
    for (const msg of messages.value) {
        if (!msg || msg.role !== 'assistant' || !Array.isArray(msg.toolCalls)) continue;
        for (const tool of msg.toolCalls) {
            if (tool?.status === 'running' || tool?.status === 'waiting') {
                tool.status = 'stopped';
                tool.waitMessage = message;
                changed = true;
            }
        }
    }
    if (changed) {
        scrollToBottom();
    }
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

const normalizeSessionFile = (file) => {
    const filename = file?.filename || String(file?.path || '').split('/').pop() || '';
    const downloadUrl = file?.download_url || '';
    return {
        ...file,
        filename,
        download_url: downloadUrl
    };
};

const fetchAllSessionFiles = async () => {
    if (!sessionId.value) {
        sessionFiles.value = [];
        return [];
    }

    try {
        const resp = await fetch(`/api/agent/sessions/${encodeURIComponent(sessionId.value)}/files`);
        if (!resp.ok) {
            sessionFiles.value = [];
            return [];
        }

        const payload = await resp.json();
        const files = Array.isArray(payload.files)
            ? payload.files.filter((file) => !!file?.path).map(normalizeSessionFile)
            : [];
        sessionFiles.value = files;
        return files;
    } catch (e) {
        console.error('Failed to load session files:', e);
        sessionFiles.value = [];
        return [];
    }
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
    await fetchAllSessionFiles();
};

const sendMessage = async () => {
    if ((!inputMessage.value.trim() && pendingFiles.value.length === 0) || isGenerating.value || isUploading.value) return;
    if (!sessionId.value) {
        createNewSession();
    }

    const userMsg = inputMessage.value;
    inputMessage.value = '';
    upsertSession(sessionId.value, '', { persist: true });
    sidebarVisible.value = true;
    saveActiveSessionToStorage();

    let uploadedFiles = [];
    try {
        uploadedFiles = await uploadPendingFiles();
    } catch (err) {
        messages.value.push({
            role: 'system',
            content: 'Upload error: ' + (err?.message || 'Failed to upload files'),
            isError: true
        });
        return;
    }

    messages.value.push({
        role: 'user',
        content: userMsg.trim() || 'Uploaded protein structure file(s).',
        files: uploadedFiles
    });
    scrollToBottom();

    isGenerating.value = true;
    const agentMsg = buildAgentMessage(userMsg, uploadedFiles);

    const assistantMsgIndex = messages.value.length;
    messages.value.push({
        role: 'assistant',
        content: '',
        toolCalls: []
    });

    let currentAssistantContent = '';
    let pendingAssistantContent = '';
    let flushTimer = null;
    let needsAssistantSectionBreak = false;

    const flushAssistantContent = () => {
        if (!pendingAssistantContent) return;
        const shouldFollow = isNearChatBottom();
        currentAssistantContent += pendingAssistantContent;
        pendingAssistantContent = '';
        messages.value[assistantMsgIndex].content = currentAssistantContent;
        scrollToBottomIfFollowing(shouldFollow);
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

    const queueAssistantText = (content) => {
        if (!content) return;

        if (needsAssistantSectionBreak) {
            const combined = currentAssistantContent + pendingAssistantContent;
            if (combined.trim()) {
                const trailingNewlines = (combined.match(/\n*$/) || [''])[0].length;
                const leadingNewlines = (content.match(/^\n*/) || [''])[0].length;
                pendingAssistantContent += '\n'.repeat(Math.max(0, 2 - trailingNewlines - leadingNewlines));
            }
            needsAssistantSectionBreak = false;
        }

        pendingAssistantContent += content;
        scheduleFlush();
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
                message: agentMsg,
                display_message: userMsg.trim() || 'Uploaded protein structure file(s).',
                uploaded_files: uploadedFiles,
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
                    const shouldFollowScroll = isNearChatBottom();

                    if (event.type === 'text') {
                        queueAssistantText(event.content);
                    }
                    else if (event.type === 'session_title') {
                        if (event.session_id === sessionId.value && event.title) {
                            upsertSession(sessionId.value, event.title, { persist: true });
                        }
                    }
                    else if (event.type === 'tool_call') {
                        forceFlush();
                        messages.value[assistantMsgIndex].toolCalls.push({
                            id: event.tool_call_id || '',
                            name: event.name,
                            args: event.args,
                            status: 'running',
                            waitMessage: '',
                            result: '',
                            files: [],
                            expanded: true
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
                        forceFlush();
                        const calls = messages.value[assistantMsgIndex].toolCalls;
                        if (calls.length > 0) {
                            const byId = calls.find((c) => c.id && c.id === event.tool_call_id);
                            const currentCall = byId || calls[calls.length - 1];
                            currentCall.status = 'done';
                            currentCall.waitMessage = '';
                            currentCall.result = event.content;
                            currentCall.expanded = false;

                            void fetchToolSessionFiles(currentCall.id)
                                .then((toolFiles) => {
                                    if (toolFiles.length > 0) {
                                        currentCall.files = toolFiles;
                                        scrollToBottomIfFollowing(shouldFollowScroll);
                                    }
                                    return fetchAllSessionFiles();
                                })
                                .catch((e) => {
                                    console.error('Failed to fetch session files:', e);
                                });
                        }
                        needsAssistantSectionBreak = true;
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
                        scrollToBottomIfFollowing(shouldFollowScroll);
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
            markPendingToolCallsStopped();
        } else {
            console.error('Chat error:', err);
        }
        isGenerating.value = false;
    }
};

const stopGeneration = () => {
    if (abortController) {
        markPendingToolCallsStopped();
        abortController.abort();
        abortController = null;
    }
    isGenerating.value = false;
};
</script>

<template>
    <div class="flex h-[calc(100vh-4rem)] w-full max-w-7xl mx-auto gap-4 p-3 sm:p-4">
        <aside v-if="hasHistorySessions && sidebarVisible"
            class="hidden md:flex w-72 shrink-0 flex-col overflow-hidden rounded-lg border border-gray-200 bg-white/95 shadow-sm dark:border-gray-700 dark:bg-gray-900/95">
            <div class="border-b border-gray-200 p-3 dark:border-gray-700">
                <button
                    class="flex w-full items-center justify-center gap-2 rounded-md bg-blue-600 px-3 py-2 text-sm font-semibold text-white transition hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
                    @click="createNewSession">
                    <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v14m7-7H5" />
                    </svg>
                    New Chat
                </button>
            </div>
            <div class="flex-1 overflow-y-auto p-2">
                <div v-for="item in sessions" :key="item.id"
                    class="group mb-1 cursor-pointer rounded-md border px-3 py-2.5 transition"
                    :class="item.id === sessionId
                        ? 'border-blue-300 bg-blue-50 text-blue-950 dark:border-blue-800 dark:bg-blue-950/35 dark:text-blue-100'
                        : 'border-transparent text-gray-700 hover:border-gray-200 hover:bg-gray-50 dark:text-gray-300 dark:hover:border-gray-700 dark:hover:bg-gray-800'"
                    @click="switchSession(item.id)">
                    <div class="flex items-start justify-between gap-2">
                        <div class="min-w-0">
                            <div class="truncate text-sm font-medium">{{ item.title || 'New Chat' }}</div>
                            <div class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                                {{ new Date(item.updated_at).toLocaleString() }}
                            </div>
                        </div>
                        <button
                            class="rounded p-1 text-gray-400 opacity-0 transition hover:bg-red-50 hover:text-red-500 group-hover:opacity-100 dark:hover:bg-red-950/40"
                            title="Delete" @click.stop="deleteSession(item.id)">
                            <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                    d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M9 7h6m-5-3h4a1 1 0 011 1v2H9V5a1 1 0 011-1z" />
                            </svg>
                        </button>
                    </div>
                </div>
            </div>
        </aside>

        <div
            class="flex min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-900">
            <div
                class="flex items-center justify-between gap-3 border-b border-gray-200 bg-white/95 px-4 py-3 dark:border-gray-700 dark:bg-gray-900/95 sm:px-5">
                <div class="flex min-w-0 items-center gap-3">
                    <div
                        class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-700 ring-1 ring-blue-100 dark:bg-blue-950/50 dark:text-blue-200 dark:ring-blue-900">
                        <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                        </svg>
                    </div>
                    <div class="min-w-0">
                        <h2 class="text-base font-semibold text-gray-900 dark:text-white">PSKit Agent</h2>
                        <p class="truncate text-xs text-gray-500 dark:text-gray-400">
                            LLM orchestration with PSKit tools · {{ sessionId }}
                        </p>
                    </div>
                </div>
                <button v-if="hasHistorySessions"
                    class="shrink-0 rounded-md border border-gray-200 px-3 py-1.5 text-xs font-medium text-gray-600 transition hover:bg-gray-50 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800"
                    @click="sidebarVisible = !sidebarVisible">
                    {{ sidebarVisible ? 'Hide History' : 'Show History' }}
                </button>
            </div>

            <div ref="chatContainer" class="flex-1 overflow-y-auto bg-gray-50/70 px-3 py-5 dark:bg-gray-950/30 sm:px-5">
                <div class="mx-auto flex w-full flex-col gap-5 transition-[max-width] duration-200"
                    :class="hasHistorySessions && sidebarVisible ? 'max-w-4xl' : 'max-w-6xl'">
                    <div v-for="(msg, index) in messages" :key="index" class="flex flex-col">
                        <div v-if="msg.role === 'user'"
                            class="message-bubble self-end max-w-[88%] rounded-2xl rounded-br-md bg-blue-600 px-4 py-3 text-sm leading-6 text-white shadow-sm sm:max-w-[78%]">
                            <div>{{ msg.content }}</div>
                            <div v-if="msg.files && msg.files.length > 0" class="mt-3 space-y-1.5">
                                <div v-for="(file, fIdx) in msg.files" :key="fIdx"
                                    class="flex items-center justify-between gap-3 rounded-md bg-white/15 px-2.5 py-1.5 text-xs">
                                    <span class="min-w-0 truncate font-mono">{{ file.filename }}</span>
                                    <span class="shrink-0 opacity-80">{{ formatFileSize(file.size || 0) }}</span>
                                </div>
                            </div>
                        </div>

                        <div v-else-if="msg.role === 'system'"
                            class="self-center rounded-md border border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-200">
                            {{ msg.content }}
                        </div>

                        <div v-else class="flex w-full max-w-full items-start gap-3">
                            <div
                                class="mt-1 flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-white text-blue-700 ring-1 ring-gray-200 dark:bg-gray-900 dark:text-blue-200 dark:ring-gray-700">
                                <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                        d="M13 10V3L4 14h7v7l9-11h-7z" />
                                </svg>
                            </div>
                            <div class="flex min-w-0 flex-1 flex-col gap-3">
                                <div v-if="msg.toolCalls && msg.toolCalls.length > 0"
                                    class="tool-trace flex min-w-0 flex-col gap-2">
                                    <div v-for="(tool, tIdx) in msg.toolCalls" :key="tIdx"
                                        class="tool-card relative min-w-0 overflow-hidden rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-900">
                                        <div class="flex items-start justify-between gap-3 p-3">
                                            <button type="button"
                                                class="flex min-w-0 flex-1 items-start gap-3 text-left"
                                                @click="tool.expanded = !tool.expanded">
                                                <span
                                                    class="mt-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-gray-50 ring-1 ring-gray-200 dark:bg-gray-800 dark:ring-gray-700">
                                                    <svg v-if="getToolStatusMeta(tool.status).icon === 'spinner'"
                                                        class="h-3.5 w-3.5 animate-spin"
                                                        :class="tool.status === 'waiting' ? 'text-amber-500' : 'text-blue-500'"
                                                        xmlns="http://www.w3.org/2000/svg" fill="none"
                                                        viewBox="0 0 24 24">
                                                        <circle class="opacity-25" cx="12" cy="12" r="10"
                                                            stroke="currentColor" stroke-width="4" />
                                                        <path class="opacity-75" fill="currentColor"
                                                            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                                                    </svg>
                                                    <svg v-else-if="getToolStatusMeta(tool.status).icon === 'stop'"
                                                        class="h-3.5 w-3.5 text-rose-500" fill="currentColor"
                                                        viewBox="0 0 20 20">
                                                        <rect x="5" y="5" width="10" height="10" rx="2" ry="2" />
                                                    </svg>
                                                    <svg v-else class="h-3.5 w-3.5 text-emerald-500" fill="none"
                                                        stroke="currentColor" viewBox="0 0 24 24">
                                                        <path stroke-linecap="round" stroke-linejoin="round"
                                                            stroke-width="2" d="M5 13l4 4L19 7" />
                                                    </svg>
                                                </span>
                                                <span class="min-w-0">
                                                    <span class="flex min-w-0 flex-wrap items-center gap-2">
                                                        <span
                                                            class="truncate font-mono text-sm font-semibold text-gray-900 dark:text-gray-100">
                                                            {{ tool.name || 'tool_call' }}
                                                        </span>
                                                        <span
                                                            class="inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium ring-1"
                                                            :class="getToolStatusMeta(tool.status).badgeClass">
                                                            <span class="h-1.5 w-1.5 rounded-full"
                                                                :class="getToolStatusMeta(tool.status).dotClass"></span>
                                                            {{ getToolStatusMeta(tool.status).label }}
                                                        </span>
                                                    </span>
                                                    <span
                                                        class="mt-1 block truncate text-xs text-gray-500 dark:text-gray-400">
                                                        Tool call {{ tIdx + 1 }} · {{ tool.expanded ? 'Details visible'
                                                        : 'Click to inspect arguments and output' }}
                                                    </span>
                                                </span>
                                            </button>
                                            <button type="button" :title="tool.expanded ? 'Collapse' : 'Expand'"
                                                class="shrink-0 rounded-md p-1.5 text-gray-500 transition hover:bg-gray-100 hover:text-blue-600 dark:hover:bg-gray-800"
                                                @click="tool.expanded = !tool.expanded">
                                                <svg class="h-4 w-4 transition-transform"
                                                    :class="tool.expanded ? 'rotate-180' : ''" fill="none"
                                                    stroke="currentColor" viewBox="0 0 24 24">
                                                    <path stroke-linecap="round" stroke-linejoin="round"
                                                        stroke-width="2" d="M19 9l-7 7-7-7" />
                                                </svg>
                                            </button>
                                        </div>

                                        <div v-if="tool.status === 'waiting' || tool.status === 'stopped'"
                                            class="mx-3 mb-3 rounded-md px-3 py-2 text-xs" :class="tool.status === 'waiting'
                                                ? 'bg-amber-50 text-amber-700 dark:bg-amber-950/30 dark:text-amber-200'
                                                : 'bg-rose-50 text-rose-700 dark:bg-rose-950/30 dark:text-rose-200'">
                                            {{ tool.waitMessage || (tool.status === 'waiting' ? 'Waiting for available worker slot...' : 'Stopped by user.') }}
                                        </div>

                                        <div v-if="tool.expanded"
                                            class="border-t border-gray-200 bg-gray-50/80 p-3 dark:border-gray-700 dark:bg-gray-950/35">
                                            <div class="grid gap-3 lg:grid-cols-2">
                                                <section class="min-w-0">
                                                    <div
                                                        class="mb-1.5 text-[11px] font-semibold uppercase text-gray-500 dark:text-gray-400">
                                                        Arguments
                                                    </div>
                                                    <pre
                                                        class="tool-details max-h-44 min-w-0 overflow-auto rounded-md border border-gray-200 bg-white p-3 text-xs leading-5 text-gray-700 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200">{{ formatToolArgs(tool.args) }}</pre>
                                                </section>
                                                <section v-if="tool.result" class="min-w-0">
                                                    <div
                                                        class="mb-1.5 text-[11px] font-semibold uppercase text-gray-500 dark:text-gray-400">
                                                        Output
                                                    </div>
                                                    <pre
                                                        class="tool-details max-h-44 min-w-0 overflow-auto rounded-md border border-gray-200 bg-white p-3 text-xs leading-5 text-gray-700 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200">{{ tool.result }}</pre>
                                                </section>
                                            </div>
                                        </div>

                                        <AgentStructureViewer v-if="supportsStructurePreview(tool)" :tool="tool"
                                            :session-files="sessionFiles" />

                                        <AgentContactMapViewer v-if="supportsContactMapPreview(tool)" :tool="tool" />

                                        <div v-if="tool.files && tool.files.length > 0"
                                            class="border-t border-gray-200 bg-white px-3 py-2.5 dark:border-gray-700 dark:bg-gray-900">
                                            <div
                                                class="mb-2 flex items-center gap-2 text-xs font-semibold text-gray-600 dark:text-gray-300">
                                                <svg class="h-4 w-4" fill="none" stroke="currentColor"
                                                    viewBox="0 0 24 24">
                                                    <path stroke-linecap="round" stroke-linejoin="round"
                                                        stroke-width="2"
                                                        d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.586-6.586a4 4 0 00-5.657-5.657l-6.586 6.586a6 6 0 108.485 8.485L20.5 13" />
                                                </svg>
                                                Output files
                                            </div>
                                            <div class="grid gap-1.5 sm:grid-cols-2">
                                                <a v-for="(file, fIdx) in tool.files" :key="fIdx"
                                                    :href="file.download_url" download
                                                    class="flex min-w-0 items-center justify-between gap-3 rounded-md border border-gray-200 px-2.5 py-2 text-xs text-gray-700 transition hover:border-blue-200 hover:bg-blue-50 hover:text-blue-700 dark:border-gray-700 dark:text-gray-300 dark:hover:border-blue-900 dark:hover:bg-blue-950/30 dark:hover:text-blue-200">
                                                    <span class="min-w-0 truncate font-mono">{{ file.filename ||
                                                        file.path }}</span>
                                                    <span class="shrink-0 text-gray-500 dark:text-gray-400">{{
                                                        formatFileSize(file.size || 0) }}</span>
                                                </a>
                                            </div>
                                        </div>
                                    </div>
                                </div>

                                <div v-if="msg.content"
                                    class="message-bubble prose prose-sm max-w-none rounded-2xl rounded-tl-md bg-white px-4 py-3 text-gray-800 shadow-sm ring-1 ring-gray-200 dark:prose-invert dark:bg-gray-900 dark:text-gray-200 dark:ring-gray-700"
                                    v-html="renderMarkdown(msg.content)">
                                </div>

                                <div v-if="isGenerating && index === messages.length - 1 && !msg.content && (!msg.toolCalls || msg.toolCalls.length === 0)"
                                    class="flex items-center gap-1.5 p-2">
                                    <div class="h-2 w-2 animate-bounce rounded-full bg-gray-400"></div>
                                    <div class="h-2 w-2 animate-bounce rounded-full bg-gray-400"
                                        style="animation-delay: 0.2s"></div>
                                    <div class="h-2 w-2 animate-bounce rounded-full bg-gray-400"
                                        style="animation-delay: 0.4s"></div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <div class="border-t border-gray-200 bg-white px-3 py-3 dark:border-gray-700 dark:bg-gray-900 sm:px-5">
                <div v-if="pendingFiles.length > 0" class="mb-3 flex flex-wrap gap-2">
                    <div v-for="(file, fIdx) in pendingFiles" :key="`${file.name}-${fIdx}`"
                        class="flex max-w-full items-center gap-2 rounded-full border border-blue-200 bg-blue-50 px-3 py-1.5 text-xs text-blue-700 dark:border-blue-800 dark:bg-blue-950/35 dark:text-blue-200">
                        <span class="max-w-48 truncate font-mono">{{ file.name }}</span>
                        <span class="shrink-0 text-blue-500 dark:text-blue-300">{{ formatFileSize(file.size || 0)
                            }}</span>
                        <button type="button"
                            class="rounded-full px-1 text-blue-500 hover:bg-white hover:text-red-500 dark:hover:bg-gray-900"
                            @click="removePendingFile(fIdx)">
                            ×
                        </button>
                    </div>
                    <div class="self-center text-xs text-gray-500 dark:text-gray-400">
                        {{ pendingFiles.length }}/{{ MAX_UPLOAD_FILES }}
                    </div>
                </div>
                <form @submit.prevent="sendMessage" class="flex items-center gap-3">
                    <input ref="fileInput" type="file" class="hidden" multiple accept=".cif,.pdb"
                        @change="handleFileSelection">
                    <button type="button" title="Upload PDB/mmCIF files"
                        class="flex h-11 w-11 shrink-0 items-center justify-center rounded-md border border-gray-200 bg-white text-gray-700 transition hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-200 dark:hover:bg-gray-700"
                        :disabled="isGenerating || isUploading" @click="triggerFilePicker">
                        <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.586-6.586a4 4 0 00-5.657-5.657l-6.586 6.586a6 6 0 108.485 8.485L20.5 13" />
                        </svg>
                    </button>
                    <input v-model="inputMessage" type="text"
                        placeholder="Ask PSKit Agent to process structures or predict interactions..."
                        class="min-w-0 flex-1 rounded-md border border-gray-200 bg-gray-50 px-4 py-3 text-sm text-gray-800 transition focus:border-blue-300 focus:bg-white focus:outline-none focus:ring-2 focus:ring-blue-500/30 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-200 dark:focus:border-blue-800 dark:focus:bg-gray-900"
                        :disabled="isGenerating || isUploading">
                    <button v-if="!isGenerating" type="submit"
                        class="flex h-11 w-11 shrink-0 items-center justify-center rounded-md bg-blue-600 text-white transition hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                        :disabled="(!inputMessage.trim() && pendingFiles.length === 0) || isUploading">
                        <svg class="h-5 w-5 rotate-90" fill="currentColor" viewBox="0 0 20 20">
                            <path
                                d="M10.894 2.553a1 1 0 00-1.788 0l-7 14a1 1 0 001.169 1.409l5-1.429A1 1 0 009 15.571V11a1 1 0 112 0v4.571a1 1 0 00.725.962l5 1.428a1 1 0 001.17-1.408l-7-14z" />
                        </svg>
                    </button>
                    <button v-else type="button" title="Stop Generation"
                        class="flex h-11 w-11 shrink-0 items-center justify-center rounded-md bg-red-500 text-white transition hover:bg-red-600 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-2"
                        @click="stopGeneration">
                        <svg class="h-5 w-5" fill="currentColor" viewBox="0 0 20 20">
                            <rect x="5" y="5" width="10" height="10" rx="2" ry="2" />
                        </svg>
                    </button>
                </form>
            </div>
        </div>
    </div>
</template>

<style scoped>
.message-bubble {
    overflow-wrap: anywhere;
    word-break: break-word;
    line-height: 1.65;
}

.prose {
    --tw-prose-body: #1f2937;
    --tw-prose-headings: #111827;
    --tw-prose-links: #2563eb;
    --tw-prose-bold: #111827;
    --tw-prose-counters: #6b7280;
    --tw-prose-bullets: #9ca3af;
    --tw-prose-hr: #e5e7eb;
    --tw-prose-quotes: #374151;
    --tw-prose-quote-borders: #cbd5e1;
    --tw-prose-captions: #6b7280;
    --tw-prose-code: #0f172a;
    --tw-prose-pre-code: #e5e7eb;
    --tw-prose-pre-bg: #0f172a;
    --tw-prose-th-borders: #cbd5e1;
    --tw-prose-td-borders: #e5e7eb;
}

.prose :where(h1, h2, h3, h4):not(:where([class~="not-prose"] *)) {
    margin-top: 1.05em;
    margin-bottom: 0.45em;
    font-weight: 700;
    line-height: 1.25;
}

.prose :where(h1):not(:where([class~="not-prose"] *)) {
    font-size: 1.2rem;
}

.prose :where(h2):not(:where([class~="not-prose"] *)) {
    font-size: 1.08rem;
}

.prose :where(h3, h4):not(:where([class~="not-prose"] *)) {
    font-size: 0.98rem;
}

.prose :where(p):not(:where([class~="not-prose"] *)) {
    margin-top: 0.45em;
    margin-bottom: 0.45em;
}

.prose :where(ul, ol):not(:where([class~="not-prose"] *)) {
    margin-top: 0.45em;
    margin-bottom: 0.55em;
    padding-left: 1.35em;
}

.prose :where(li):not(:where([class~="not-prose"] *)) {
    margin-top: 0.18em;
    margin-bottom: 0.18em;
    padding-left: 0.12em;
}

.prose :where(li > p):not(:where([class~="not-prose"] *)) {
    margin-top: 0.15em;
    margin-bottom: 0.15em;
}

.prose :where(hr):not(:where([class~="not-prose"] *)) {
    margin-top: 0.9em;
    margin-bottom: 0.9em;
}

.prose :where(blockquote):not(:where([class~="not-prose"] *)) {
    margin-top: 0.7em;
    margin-bottom: 0.7em;
    border-left-width: 3px;
    padding: 0.4em 0 0.4em 0.9em;
    color: #475569;
    font-style: normal;
    background: rgba(248, 250, 252, 0.75);
    border-radius: 0 0.375rem 0.375rem 0;
}

.prose :where(table):not(:where([class~="not-prose"] *)) {
    display: block;
    width: 100%;
    max-width: 100%;
    margin-top: 0.75em;
    margin-bottom: 0.75em;
    overflow-x: auto;
    border-collapse: collapse;
    font-size: 0.8125rem;
    line-height: 1.45;
}

.prose :where(thead):not(:where([class~="not-prose"] *)) {
    background: #f8fafc;
}

.prose :where(th):not(:where([class~="not-prose"] *)) {
    border: 1px solid #cbd5e1;
    padding: 0.45rem 0.55rem;
    text-align: left;
    vertical-align: top;
    white-space: nowrap;
}

.prose :where(td):not(:where([class~="not-prose"] *)) {
    border: 1px solid #e5e7eb;
    padding: 0.42rem 0.55rem;
    vertical-align: top;
}

.prose :where(a):not(:where([class~="not-prose"] *)) {
    text-decoration-thickness: 1px;
    text-underline-offset: 2px;
}

.prose :where(code):not(:where([class~="not-prose"] *)) {
    border-radius: 0.3rem;
    background: #f1f5f9;
    padding: 0.08rem 0.28rem;
    color: #0f172a;
    font-weight: 500;
    word-break: break-all;
}

.prose :where(code):not(:where([class~="not-prose"] *))::before,
.prose :where(code):not(:where([class~="not-prose"] *))::after {
    content: "";
}

.message-bubble :deep(*) {
    overflow-wrap: anywhere;
    word-break: break-word;
}

.message-bubble :deep(a),
.message-bubble :deep(code),
.message-bubble :deep(p),
.message-bubble :deep(li),
.message-bubble :deep(td),
.message-bubble :deep(th) {
    max-width: 100%;
    overflow-wrap: anywhere;
    word-break: break-word;
}

.message-bubble :deep(code) {
    white-space: pre-wrap;
}

.message-bubble :deep(pre) {
    max-width: 100%;
    overflow-x: auto;
    white-space: pre-wrap;
}

.tool-card,
.tool-details {
    box-sizing: border-box;
    contain: inline-size;
}

.prose :where(pre):not(:where([class~="not-prose"] *)) {
    margin-top: 0.75em;
    margin-bottom: 0.75em;
    border-radius: 0.5rem;
    background-color: #0f172a;
    color: #e5e7eb;
    padding: 0.85rem;
    line-height: 1.55;
}

.prose :where(pre code):not(:where([class~="not-prose"] *)) {
    background: transparent;
    color: inherit;
    padding: 0;
    white-space: pre-wrap;
    word-break: break-word;
}

:global(.dark) .prose {
    --tw-prose-body: #e5e7eb;
    --tw-prose-headings: #f9fafb;
    --tw-prose-links: #93c5fd;
    --tw-prose-bold: #f9fafb;
    --tw-prose-quotes: #cbd5e1;
}

:global(.dark) .prose :where(blockquote):not(:where([class~="not-prose"] *)) {
    background: rgba(15, 23, 42, 0.65);
    color: #cbd5e1;
}

:global(.dark) .prose :where(thead):not(:where([class~="not-prose"] *)) {
    background: #1f2937;
}

:global(.dark) .prose :where(th):not(:where([class~="not-prose"] *)) {
    border-color: #475569;
}

:global(.dark) .prose :where(td):not(:where([class~="not-prose"] *)) {
    border-color: #334155;
}

:global(.dark) .prose :where(code):not(:where([class~="not-prose"] *)) {
    background: #1f2937;
    color: #e5e7eb;
}
</style>
