<script setup>
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";

const props = defineProps({
    tool: {
        type: Object,
        required: true,
    },
});

const isOpen = ref(true);
const isLoading = ref(false);
const errorMessage = ref("");
const canvasEl = ref(null);
const canvasWrapEl = ref(null);
const mapMeta = ref(null);

let lastOffscreen = null;
let lastMeta = null;
let renderToken = 0;
let resizeObserver = null;

const resultJson = computed(() => {
    const files = Array.isArray(props.tool?.files) ? props.tool.files : [];
    return files.find((file) => {
        const name = String(file?.filename || file?.path || "");
        return name.startsWith("contact_map_") && name.endsWith(".json");
    }) || null;
});

const canOfferPreview = computed(() => String(props.tool?.name || "") === "calculate_contact_map" && !!resultJson.value);

const previewSubtitle = computed(() => {
    if (!mapMeta.value) return "Load contact map preview";
    const mode = mapMeta.value.mode === "knn" ? "KNN" : "Distance";
    return `${mode} map · ${mapMeta.value.n} x ${mapMeta.value.n}`;
});

function parseToolArgs() {
    const args = props.tool?.args;
    if (!args) return {};
    if (typeof args === "object") return args;
    try {
        return JSON.parse(args);
    } catch {
        return {};
    }
}

function flattenUpperValues(values) {
    if (!Array.isArray(values)) return Float64Array.from([]);
    if (values.every((row) => Array.isArray(row))) {
        return Float64Array.from(values.flat().map((value) => Number(value)));
    }
    return Float64Array.from(values.map((value) => Number(value)));
}

function maxFinite(arr) {
    let m = 0;
    for (let i = 0; i < arr.length; i += 1) {
        const v = Number(arr[i]);
        if (Number.isFinite(v) && v > m) m = v;
    }
    return m;
}

function setCanvasSizeToCss(canvas) {
    const dpr = window.devicePixelRatio || 1;
    const cssW = canvas.clientWidth || 0;
    const cssH = canvas.clientHeight || 0;
    const w = Math.max(1, Math.floor(cssW * dpr));
    const h = Math.max(1, Math.floor(cssH * dpr));
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    return { cssW, cssH, dpr };
}

function lerp(a, b, t) {
    return a + (b - a) * t;
}

function clamp01(x) {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    return x;
}

const RWB_STOPS = [
    { t: 0.0, c: [178, 24, 43] },
    { t: 0.125, c: [214, 96, 77] },
    { t: 0.25, c: [244, 165, 130] },
    { t: 0.375, c: [253, 219, 199] },
    { t: 0.5, c: [247, 247, 247] },
    { t: 0.625, c: [209, 229, 240] },
    { t: 0.75, c: [146, 197, 222] },
    { t: 0.875, c: [67, 147, 195] },
    { t: 1.0, c: [33, 102, 172] },
];

function rwbColor(t) {
    const x = clamp01(t);
    for (let i = 0; i < RWB_STOPS.length - 1; i += 1) {
        const a = RWB_STOPS[i];
        const b = RWB_STOPS[i + 1];
        if (x >= a.t && x <= b.t) {
            const u = (x - a.t) / (b.t - a.t);
            return [
                Math.round(lerp(a.c[0], b.c[0], u)),
                Math.round(lerp(a.c[1], b.c[1], u)),
                Math.round(lerp(a.c[2], b.c[2], u)),
            ];
        }
    }
    return RWB_STOPS[RWB_STOPS.length - 1].c;
}

function parseAxisEntry(s) {
    const text = String(s ?? "");
    const i1 = text.indexOf("-");
    if (i1 < 0) return { chainId: text, seqId: "", seqName: "" };
    const i2 = text.indexOf("-", i1 + 1);
    if (i2 < 0) return { chainId: text.slice(0, i1), seqId: text.slice(i1 + 1), seqName: "" };
    return {
        chainId: text.slice(0, i1),
        seqId: text.slice(i1 + 1, i2),
        seqName: text.slice(i2 + 1),
    };
}

function chainSegmentsFromAxisEntries(entries) {
    const segs = [];
    if (!entries || entries.length === 0) return segs;
    let start = 0;
    let cur = String(entries[0]?.chainId ?? "");
    for (let i = 1; i < entries.length; i += 1) {
        const c = String(entries[i]?.chainId ?? "");
        if (c !== cur) {
            segs.push({ chainId: cur, start, end: i - 1 });
            start = i;
            cur = c;
        }
    }
    segs.push({ chainId: cur, start, end: entries.length - 1 });
    return segs;
}

function formatLegendNumber(v) {
    const x = Number(v);
    if (!Number.isFinite(x)) return "";
    if (x === 0) return "0";
    const ax = Math.abs(x);
    if (ax >= 1000 || ax < 0.01) return x.toExponential(2);
    return String(Number(x.toFixed(3)));
}

function drawHeatmapWithAxesAndLegend(ctx, off, meta, cssW, cssH) {
    if (!ctx || !off || !meta) return;
    const n = Number(meta?.n) || 0;
    if (n <= 0) return;

    const entries = Array.isArray(meta?.axisEntries) ? meta.axisEntries : [];
    const segments = chainSegmentsFromAxisEntries(entries);
    const vmax = Number.isFinite(meta?.vmaxColor) ? Number(meta.vmaxColor) : 0;
    const vmin = Number.isFinite(meta?.vminColor) ? Number(meta.vminColor) : 0;

    const marginLeft = 54;
    const marginTop = 28;
    const marginRight = 14;
    const marginBottom = 52;
    const legendH = 10;
    const plot = Math.max(1, Math.floor(Math.min(cssW - marginLeft - marginRight, cssH - marginTop - marginBottom)));
    const chartW = marginLeft + plot + marginRight;
    const chartH = marginTop + plot + marginBottom;
    const xChart = Math.floor((cssW - chartW) / 2);
    const yChart = Math.floor((cssH - chartH) / 2);
    const heatX = xChart + marginLeft;
    const heatY = yChart + marginTop;

    const styleSrc = canvasWrapEl.value || canvasEl.value?.parentElement || document.body;
    const textColor = getComputedStyle(styleSrc).color || "#111";

    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(off, 0, 0, n, n, heatX, heatY, plot, plot);

    ctx.save();
    ctx.strokeStyle = textColor;
    ctx.fillStyle = textColor;
    ctx.lineWidth = 1;
    ctx.font = "12px sans-serif";
    ctx.globalAlpha = 0.7;

    const tickLen = 9;
    for (const seg of segments) {
        if (seg.start <= 0) continue;
        const x = heatX + (seg.start / n) * plot;
        const y = heatY + (seg.start / n) * plot;
        ctx.beginPath();
        ctx.moveTo(x, heatY - tickLen);
        ctx.lineTo(x, heatY);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(heatX - tickLen, y);
        ctx.lineTo(heatX, y);
        ctx.stroke();
    }

    ctx.globalAlpha = 0.95;
    for (const seg of segments) {
        const label = String(seg.chainId);
        const startFrac = seg.start / n;
        const endFrac = (seg.end + 1) / n;
        const xc = (heatX + startFrac * plot + heatX + endFrac * plot) / 2;
        const yc = (heatY + startFrac * plot + heatY + endFrac * plot) / 2;
        ctx.textAlign = "center";
        ctx.textBaseline = "bottom";
        ctx.fillText(label, xc, heatY - 6);
        ctx.textAlign = "right";
        ctx.textBaseline = "middle";
        ctx.fillText(label, heatX - 8, yc);
    }

    const legendX = heatX;
    const legendY = heatY + plot + 18;
    const grad = ctx.createLinearGradient(legendX, 0, legendX + plot, 0);
    for (const st of RWB_STOPS) {
        grad.addColorStop(st.t, `rgb(${st.c[0]},${st.c[1]},${st.c[2]})`);
    }
    ctx.globalAlpha = 1;
    ctx.fillStyle = grad;
    ctx.fillRect(legendX, legendY, plot, legendH);
    ctx.strokeStyle = textColor;
    ctx.globalAlpha = 0.6;
    ctx.strokeRect(legendX, legendY, plot, legendH);
    ctx.globalAlpha = 0.9;
    ctx.fillStyle = textColor;
    ctx.textAlign = "left";
    ctx.textBaseline = "top";
    ctx.fillText(formatLegendNumber(vmin), legendX, legendY + legendH + 6);
    ctx.textAlign = "right";
    ctx.fillText(formatLegendNumber(vmax), legendX + plot, legendY + legendH + 6);
    ctx.restore();
}

function paintToVisible(off, meta) {
    const canvas = canvasEl.value;
    if (!canvas || !off || !meta) return;
    const { cssW, cssH, dpr } = setCanvasSizeToCss(canvas);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssW, cssH);
    drawHeatmapWithAxesAndLegend(ctx, off, meta, cssW, cssH);
}

async function renderHeatmap(payload) {
    const axis = Array.isArray(payload?.axis) ? payload.axis.map((x) => String(x)) : [];
    const n = axis.length;
    const upper = flattenUpperValues(payload?.values);
    const expectUpper = (n * (n - 1)) / 2;
    if (n <= 0) throw new Error("Contact map JSON has no axis entries.");
    if (upper.length !== expectUpper && upper.length !== n * n) {
        throw new Error(`Contact map values length mismatch: expected ${expectUpper} upper values, got ${upper.length}.`);
    }

    const vmax = maxFinite(upper);
    const denom = vmax > 0 ? vmax : 1;
    const token = ++renderToken;
    const off = document.createElement("canvas");
    off.width = n;
    off.height = n;
    const offCtx = off.getContext("2d");
    if (!offCtx) throw new Error("Canvas is unavailable.");

    const img = offCtx.createImageData(n, n);
    const data = img.data;
    const base = rwbColor(0);
    for (let p = 0; p < data.length; p += 4) {
        data[p + 0] = base[0];
        data[p + 1] = base[1];
        data[p + 2] = base[2];
        data[p + 3] = 255;
    }

    const axisEntries = axis.map(parseAxisEntry);
    lastOffscreen = off;
    lastMeta = { axis, axisEntries, n, vmax, vminColor: 0, vmaxColor: vmax };
    mapMeta.value = { n, vmax, mode: parseToolArgs().mode || "d" };
    paintToVisible(lastOffscreen, lastMeta);

    const rowsPerFrame = 24;
    let i = 0;
    while (i < n) {
        if (token !== renderToken) return;
        const iStart = i;
        const iEnd = Math.min(n, i + rowsPerFrame);
        for (; i < iEnd; i += 1) {
            if (upper.length === n * n) {
                for (let j = 0; j < n; j += 1) {
                    const v = i === j ? 0 : Number(upper[i * n + j] ?? 0);
                    const [r, g, b] = rwbColor(v / denom);
                    const p = (i * n + j) * 4;
                    data[p + 0] = r;
                    data[p + 1] = g;
                    data[p + 2] = b;
                }
                continue;
            }

            let k = (i * (2 * n - i - 1)) / 2;
            for (let j = i + 1; j < n; j += 1) {
                const v = Number(upper[k++] ?? 0);
                const [r, g, b] = rwbColor(v / denom);
                const p1 = (i * n + j) * 4;
                data[p1 + 0] = r;
                data[p1 + 1] = g;
                data[p1 + 2] = b;
                const p2 = (j * n + i) * 4;
                data[p2 + 0] = r;
                data[p2 + 1] = g;
                data[p2 + 2] = b;
            }
        }

        offCtx.putImageData(img, 0, 0, 0, iStart, n, iEnd - iStart);
        paintToVisible(lastOffscreen, lastMeta);
        await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    paintToVisible(lastOffscreen, lastMeta);
}

async function renderPreview() {
    if (!isOpen.value || !canOfferPreview.value) return;
    isLoading.value = true;
    errorMessage.value = "";
    try {
        const response = await fetch(resultJson.value.download_url);
        if (!response.ok) throw new Error("Failed to load contact map JSON.");
        const payload = await response.json();
        await nextTick();
        await renderHeatmap(payload);
    } catch (error) {
        errorMessage.value = error?.message || String(error);
    } finally {
        isLoading.value = false;
    }
}

function togglePreview() {
    isOpen.value = !isOpen.value;
}

function handleResize() {
    if (lastOffscreen && lastMeta) paintToVisible(lastOffscreen, lastMeta);
}

watch(
    () => isOpen.value,
    async (open) => {
        if (open) await renderPreview();
    },
);

watch(
    () => [props.tool?.id, props.tool?.files?.length],
    async () => {
        mapMeta.value = null;
        errorMessage.value = "";
        lastOffscreen = null;
        lastMeta = null;
        if (isOpen.value) await renderPreview();
    },
);

watch(
    () => canOfferPreview.value,
    async (canPreview) => {
        if (canPreview && isOpen.value) await renderPreview();
    },
    { immediate: true },
);

onBeforeUnmount(() => {
    window.removeEventListener("resize", handleResize);
    resizeObserver?.disconnect();
    resizeObserver = null;
    renderToken += 1;
});

watch(canvasWrapEl, (el) => {
    resizeObserver?.disconnect();
    resizeObserver = null;

    if (!el) return;
    window.addEventListener("resize", handleResize);
    resizeObserver = new ResizeObserver(() => {
        handleResize();
    });
    resizeObserver.observe(el);
    if (lastOffscreen && lastMeta) paintToVisible(lastOffscreen, lastMeta);
});
</script>

<template>
    <div v-if="canOfferPreview" class="border-t border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
        <button type="button"
            class="flex w-full items-center justify-between gap-3 px-3 py-2.5 text-left transition hover:bg-gray-50 dark:hover:bg-gray-800"
            @click="togglePreview">
            <span class="flex min-w-0 items-center gap-2">
                <span
                    class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-blue-50 text-blue-700 ring-1 ring-blue-100 dark:bg-blue-950/40 dark:text-blue-200 dark:ring-blue-900">
                    <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                            d="M4 5h16M4 12h16M4 19h16M5 4v16M12 4v16M19 4v16" />
                    </svg>
                </span>
                <span class="min-w-0">
                    <span class="block truncate text-xs font-semibold text-gray-800 dark:text-gray-100">Contact map preview</span>
                    <span class="block truncate text-xs text-gray-500 dark:text-gray-400">{{ previewSubtitle }}</span>
                </span>
            </span>
            <span class="flex shrink-0 items-center gap-2">
                <span class="hidden text-xs text-gray-500 dark:text-gray-400 sm:inline">Distance heatmap</span>
                <svg class="h-4 w-4 text-gray-500 transition-transform" :class="isOpen ? 'rotate-180' : ''"
                    fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
            </span>
        </button>

        <div v-if="isOpen" class="border-t border-gray-200 bg-gray-50/80 p-3 dark:border-gray-700 dark:bg-gray-950/35">
            <div class="overflow-hidden rounded-lg border border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-900">
                <div class="flex flex-wrap items-center justify-between gap-3 border-b border-gray-200 px-3 py-2 dark:border-gray-700">
                    <div class="min-w-0">
                        <div class="truncate text-sm font-semibold text-gray-900 dark:text-gray-100">Contact map</div>
                        <div class="truncate text-xs text-gray-500 dark:text-gray-400">
                            {{ resultJson?.filename || resultJson?.path || 'Contact map JSON' }}
                        </div>
                    </div>
                    <div class="flex items-center gap-2 text-xs text-gray-600 dark:text-gray-300">
                        <span>Near</span>
                        <span class="h-2 w-24 rounded-full contact-map-gradient"></span>
                        <span>Far</span>
                    </div>
                </div>

                <div ref="canvasWrapEl"
                    class="relative flex h-[320px] justify-center bg-white text-gray-900 dark:bg-gray-950 dark:text-gray-200 sm:h-[420px]">
                    <canvas ref="canvasEl" class="h-full w-auto max-w-full" style="aspect-ratio: 1 / 1"></canvas>
                    <div v-if="isLoading"
                        class="absolute inset-0 flex items-center justify-center bg-white/80 backdrop-blur-sm dark:bg-gray-950/80">
                        <div
                            class="flex items-center gap-2 rounded-md border border-gray-200 bg-white px-3 py-2 text-sm text-gray-700 shadow-sm dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200">
                            <svg class="h-4 w-4 animate-spin text-blue-600" fill="none" viewBox="0 0 24 24">
                                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor"
                                    stroke-width="4"></circle>
                                <path class="opacity-75" fill="currentColor"
                                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                            </svg>
                            Loading contact map
                        </div>
                    </div>
                    <div v-if="errorMessage && !isLoading"
                        class="absolute inset-0 flex items-center justify-center bg-white/90 p-4 text-center dark:bg-gray-950/90">
                        <div
                            class="max-w-md rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-900 dark:bg-amber-950/40 dark:text-amber-200">
                            {{ errorMessage }}
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>
</template>

<style scoped>
.contact-map-gradient {
    background: linear-gradient(90deg, rgb(178, 24, 43), rgb(247, 247, 247), rgb(33, 102, 172));
}
</style>
