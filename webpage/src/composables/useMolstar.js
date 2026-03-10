import { ref, onBeforeUnmount, nextTick, watch } from "vue";
import { ensurePdbeMolstarLoaded, createPdbeMolstarViewer, destroyPdbeMolstarViewer } from "../utils/pdbeMolstar.js";

export const MOLSTAR_COLORS = {
    nonSelected: { r: 190, g: 190, b: 190 },
    highlight: { r: 52, g: 152, b: 219 },
    protein: { r: 52, g: 152, b: 219 },
    nucleic: { r: 231, g: 76, b: 60 },
    focus: { r: 255, g: 235, b: 59 },
};

export function useMolstar() {
    const viewerContainer = ref(null);
    let viewerInstance = null;
    let viewerLastObjectUrl = null;
    let viewerStructureKey = "";
    let idStructureCache = new Map();

    // 自动监听容器 DOM 的存在性
    // 如果 div 没了（被 v-if 删了），就重置 key，确保下次进来能重新触发渲染
    watch(viewerContainer, (newEl, oldEl) => {
        if (!newEl) {
            viewerStructureKey = "";
            void destroyViewer();
            return;
        }

        if (oldEl && viewerInstance?.targetElement && viewerInstance.targetElement !== newEl) {
            viewerStructureKey = "";
            void destroyViewer();
        }
    });

    function revokeViewerObjectUrl() {
        if (!viewerLastObjectUrl) return;
        try {
            URL.revokeObjectURL(viewerLastObjectUrl);
        } catch {
            // ignore
        }
        viewerLastObjectUrl = null;
    }

    async function destroyViewer() {
        const currentViewer = viewerInstance;
        viewerInstance = null;
        viewerStructureKey = "";
        revokeViewerObjectUrl();
        await destroyPdbeMolstarViewer(currentViewer);
    }

    async function initViewer() {
        await ensurePdbeMolstarLoaded();
        await nextTick();

        if (viewerInstance && viewerContainer.value && viewerInstance.targetElement && viewerInstance.targetElement !== viewerContainer.value) {
            await destroyViewer();
        }

        if (!viewerInstance) {
            viewerInstance = createPdbeMolstarViewer();
        }
        return viewerInstance;
    }

    function clearViewer() {
        revokeViewerObjectUrl();
        try {
            viewerInstance?.clear?.();
        } catch {
            // ignore
        }
    }

    onBeforeUnmount(() => {
        idStructureCache.clear();
        void destroyViewer();
    });

    return {
        viewerContainer,
        initViewer,
        getViewerInstance: () => viewerInstance,
        revokeViewerObjectUrl,
        idStructureCache,
        getViewerStructureKey: () => viewerStructureKey,
        setViewerStructureKey: (key) => { viewerStructureKey = key; },
        setViewerLastObjectUrl: (url) => { viewerLastObjectUrl = url; },
        clearViewer,
        destroyViewer,
    };
}