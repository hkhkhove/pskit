<script setup>
import { ref, computed, onMounted, onBeforeUnmount, watch, nextTick } from "vue";
import { useRoute, useRouter } from "vue-router";
import JSZip from "jszip";
import InputStructure from "../components/InputStructure.vue";
import { parsePdbIds, isValidPdbId, prepareInputsFromFiles, prepareInputsFromPdbIds, runBatch, dMapInWorker, sanitizeKey } from "../utils/wasmBatch.js";

const route = useRoute();
const router = useRouter();

const input_method = ref("file");
const ids = ref("");
const files = ref([]);

const chain_id = ref("");
function chain_id_example() {
    chain_id.value = "A";
}

const processing = ref(false);
const error_message = ref("");
const results = ref([]);
const file_errors = ref([]);
const progress = ref({ current: 0, total: 0, current_file: "" });

const current_index = ref(0);

const canvas_el = ref(null);
const canvas_wrap_el = ref(null);

let last_offscreen = null;
let last_meta = null;
let render_token = 0;

const parsed_ids = computed(() => parsePdbIds(ids.value));
const ids_valid = computed(() => {
    if (parsed_ids.value.length === 0) return false;
    return parsed_ids.value.every(isValidPdbId);
});

const progress_text = computed(() => {
    if (!processing.value || progress.value.total === 0) return "";
    return `(${progress.value.current}/${progress.value.total}) ${progress.value.current_file}`;
});

const is_results_view = computed(() => route.query.view === "results");

const run_button_text = computed(() => {
    if (!processing.value) return "Run";
    // When using PDB IDs, we first download structures in prepareInputsFromPdbIds().
    // During that phase, we have no batch progress yet.
    if (input_method.value === "id" && progress.value.total === 0) return "Downloading PDB files by ID...";
    return progress_text.value ? `Processing... ${progress_text.value}` : "Processing...";
});

function csvFilename(base) {
    const c = chain_id.value.trim();
    const cPart = c ? sanitizeKey(c) : "all";
    // If the rendered matrix is downsampled, append ds{n} for clarity.
    const cur = current_result.value;
    const ds = cur?.original_n && cur.original_n > cur.n ? `.ds${cur.n}` : "";
    return `${base}.d_map.${cPart}.matrix${ds}.csv`;
}

function pngFilename(base) {
    const c = chain_id.value.trim();
    const cPart = c ? sanitizeKey(c) : "all";
    const cur = current_result.value;
    const ds = cur?.original_n && cur.original_n > cur.n ? `.ds${cur.n}` : "";
    return `${base}.d_map.${cPart}.heatmap${ds}.png`;
}

function upperIndex(n, i, j) {
    // Index into packed upper triangle (excluding diagonal), row-major by i then j.
    // i<j, 0-based.
    // prefix = sum_{r=0}^{i-1} (n-r-1) = i*(2n-i-1)/2
    return (i * (2 * n - i - 1)) / 2 + (j - i - 1);
}

function normalizeAxis(axis) {
    return Array.isArray(axis) ? axis.map((x) => String(x)) : [];
}

function normalizeUpperValues(values) {
    if (values instanceof Float64Array) return values;
    return Float64Array.from(Array.from(values || []).map((x) => Number(x)));
}

function valueAtUpper(n, upper, i, j) {
    if (i === j) return 0;
    const ii = i < j ? i : j;
    const jj = i < j ? j : i;
    const k = upperIndex(n, ii, jj);
    const v = Number(upper?.[k] ?? 0);
    return Number.isFinite(v) ? v : 0;
}

function maxFinite(arr) {
    if (!arr || arr.length === 0) return 0;
    let m = 0;
    for (let i = 0; i < arr.length; i++) {
        const v = Number(arr[i]);
        if (Number.isFinite(v) && v > m) m = v;
    }
    return m;
}

function toCsvRow(values) {
    return values
        .map((v) => {
            const s = v === null || v === undefined ? "" : String(v);
            const escaped = s.replaceAll('"', '""');
            return /[\n\r,\"]/g.test(escaped) ? `"${escaped}"` : escaped;
        })
        .join(",");
}

function downloadTextFile({ text, filename, mime = "text/csv;charset=utf-8" }) {
    const blob = new Blob([text], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
}

function uniqueZipName(name, used) {
    const clean = String(name).replace(/[/\\]+/g, "_");
    if (!used.has(clean)) {
        used.add(clean);
        return clean;
    }
    let i = 2;
    while (used.has(`${clean} (${i})`)) i++;
    const finalName = `${clean} (${i})`;
    used.add(finalName);
    return finalName;
}

async function runDMap() {
    error_message.value = "";
    file_errors.value = [];
    progress.value = { current: 0, total: 0, current_file: "" };
    results.value = [];
    current_index.value = 0;

    if (input_method.value === "file") {
        if (files.value.length === 0) {
            error_message.value = "Please upload at least one structure file (.pdb or .cif).";
            return;
        }
    } else if (input_method.value === "id") {
        if (parsed_ids.value.length === 0) {
            error_message.value = "Please enter at least one PDB ID (separated by commas).";
            return;
        }
        if (!ids_valid.value) {
            error_message.value = "PDB ID format is incorrect: must be 4 alphanumeric characters (separated by commas).";
            return;
        }
    } else {
        error_message.value = "Please select an input method (ID or file).";
        return;
    }

    processing.value = true;
    try {
        const inputs = input_method.value === "file" ? await prepareInputsFromFiles(files.value) : await prepareInputsFromPdbIds(parsed_ids.value);
        const chainArg = chain_id.value.trim() ? chain_id.value.trim() : undefined;

        const { downloads, errors } = await runBatch({
            inputs,
            processOne: (input) => dMapInWorker(input.bytes, chainArg, input.format),
            toDownloadItems: (result, input) => {
                const axis = normalizeAxis(result.axis || []);
                const n = axis.length;
                const upper = normalizeUpperValues(result.values || new Float64Array());
                const maxVal = maxFinite(upper);
                return [
                    {
                        source: input.source,
                        base: input.base,
                        chain_id: chainArg ?? "",
                        axis,
                        n,
                        upper_values: upper,
                        vmax: Number.isFinite(maxVal) ? maxVal : 0,
                        original_n: n,
                        downsample_stride: 1,
                    },
                ];
            },
            onProgress: (p) => {
                progress.value = p;
            },
        });

        results.value = downloads;
        file_errors.value = errors;

        // Switch to results view via URL state so the browser back button returns to the form.
        if ((downloads?.length || 0) > 0) {
            const q = { ...route.query, view: "results" };
            await router.push({ query: q });
        }
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    } finally {
        processing.value = false;
        progress.value = { current: 0, total: 0, current_file: "" };
    }
}

const has_results = computed(() => results.value.length > 0);

const has_multiple_results = computed(() => results.value.length > 1);

const current_result = computed(() => {
    if (!has_results.value) return null;
    const i = Math.min(Math.max(0, current_index.value), results.value.length - 1);
    return results.value[i] ?? null;
});

const current_title = computed(() => {
    if (!current_result.value) return "";
    const i = Math.min(Math.max(0, current_index.value), results.value.length - 1);
    return `${current_result.value.source} (${i + 1}/${results.value.length})`;
});

const can_next = computed(() => {
    return has_multiple_results.value && !processing.value;
});

function nextResult() {
    if (!has_multiple_results.value) return;
    current_index.value = (current_index.value + 1) % results.value.length;
}

const can_download = computed(() => {
    return !!current_result.value && !processing.value;
});

const can_download_all = computed(() => {
    return has_results.value && !processing.value;
});

function csvTextForResult(res) {
    const axis = Array.isArray(res?.axis) ? res.axis : [];
    const n = Number(res?.n) || axis.length;
    const upper = res?.upper_values instanceof Float64Array ? res.upper_values : normalizeUpperValues(res?.upper_values);
    const expectUpper = (n * (n - 1)) / 2;
    if (n > 0 && upper.length !== expectUpper && upper.length !== n * n) {
        throw new Error(`CSV export failed: values length mismatch (need ${expectUpper} upper or ${n * n} full, got ${upper.length})`);
    }

    const header = toCsvRow(["", ...axis]);
    const lines = [header];
    for (let i = 0; i < n; i++) {
        const row = [axis[i] ?? ""];
        for (let j = 0; j < n; j++) {
            if (upper.length === n * n) {
                row.push(Number(upper[i * n + j] ?? 0));
            } else {
                row.push(valueAtUpper(n, upper, i, j));
            }
        }
        lines.push(toCsvRow(row));
    }
    return lines.join("\n") + "\n";
}

function downloadCurrent() {
    if (!current_result.value) return;
    try {
        const text = csvTextForResult(current_result.value);
        const filename = csvFilename(current_result.value.base || "results");
        downloadTextFile({ text, filename });
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

function downloadHeatmapPng() {
    if (!current_result.value) return;
    try {
        const off = last_offscreen;
        const meta = last_meta;
        if (!off || !meta) return;

        // Export pure heatmap only (no axes / no legend), at higher pixel resolution.
        // This is strictly 1:1 (square) by design.
        const n = Number(meta?.n) || 0;
        const targetPlot = Math.min(4096, Math.max(1024, n * 16));
        const outW = targetPlot;
        const outH = targetPlot;

        const out = document.createElement("canvas");
        out.width = outW;
        out.height = outH;
        const ctx = out.getContext("2d");
        if (!ctx) return;
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.clearRect(0, 0, outW, outH);

        ctx.imageSmoothingEnabled = false;
        ctx.drawImage(off, 0, 0, n, n, 0, 0, outW, outH);

        out.toBlob(
            (blob) => {
                if (!blob) return;
                const url = URL.createObjectURL(blob);
                const a = document.createElement("a");
                a.href = url;
                a.download = pngFilename(current_result.value.base || "results");
                document.body.appendChild(a);
                a.click();
                a.remove();
                URL.revokeObjectURL(url);
            },
            "image/png",
            1.0
        );
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
}

async function downloadAll() {
    if (!can_download_all.value) return;

    try {
        const zip = new JSZip();
        const used = new Set();
        for (const res of results.value || []) {
            const filename = csvFilename(res?.base || "results");
            const safe = uniqueZipName(filename, used);
            zip.file(safe, csvTextForResult(res));
        }
        const blob = await zip.generateAsync({ type: "blob" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "d_map_tables.zip";
        document.body.appendChild(a);
        a.click();
        a.remove();
        URL.revokeObjectURL(url);
    } catch (e) {
        error_message.value = e?.message ? String(e.message) : String(e);
    }
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

// Sequential colormap: Viridis (matplotlib) - reversed
// Keep the same number of stops for smooth gradients.
const VIRIDIS_STOPS = [
    { t: 0.0, c: [253, 231, 37] },
    { t: 0.125, c: [109, 205, 89] },
    { t: 0.25, c: [53, 183, 121] },
    { t: 0.375, c: [31, 158, 137] },
    { t: 0.5, c: [38, 130, 142] },
    { t: 0.625, c: [49, 104, 142] },
    { t: 0.75, c: [62, 73, 137] },
    { t: 0.875, c: [72, 40, 120] },
    { t: 1.0, c: [68, 1, 84] },
];

function viridisColor(t) {
    // Viridis colormap via linear interpolation between stops.
    const x = clamp01(t);
    for (let i = 0; i < VIRIDIS_STOPS.length - 1; i++) {
        const a = VIRIDIS_STOPS[i];
        const b = VIRIDIS_STOPS[i + 1];
        if (x >= a.t && x <= b.t) {
            const u = (x - a.t) / (b.t - a.t);
            return [Math.round(lerp(a.c[0], b.c[0], u)), Math.round(lerp(a.c[1], b.c[1], u)), Math.round(lerp(a.c[2], b.c[2], u))];
        }
    }
    return VIRIDIS_STOPS[VIRIDIS_STOPS.length - 1].c;
}

function parseAxisEntry(s) {
    const text = String(s ?? "");
    const i1 = text.indexOf("-");
    if (i1 < 0) return { chainId: text, seqId: "", seqName: "" };
    const i2 = text.indexOf("-", i1 + 1);
    if (i2 < 0) return { chainId: text.slice(0, i1), seqId: text.slice(i1 + 1), seqName: "" };
    return { chainId: text.slice(0, i1), seqId: text.slice(i1 + 1, i2), seqName: text.slice(i2 + 1) };
}

function chainSegmentsFromAxisEntries(entries) {
    const segs = [];
    if (!entries || entries.length === 0) return segs;
    let start = 0;
    let cur = String(entries[0]?.chainId ?? "");
    for (let i = 1; i < entries.length; i++) {
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

function drawHeatmapWithAxesAndLegend(ctx, off, meta, cssW, cssH, opts = {}) {
    if (!ctx || !off || !meta) return;
    const n = Number(meta?.n) || 0;
    if (n <= 0) return;

    const showLegend = opts?.showLegend !== false;

    const entries = Array.isArray(meta?.axisEntries) ? meta.axisEntries : Array.isArray(meta?.axis) ? meta.axis.map(parseAxisEntry) : [];
    const segments = chainSegmentsFromAxisEntries(entries);
    const vmax = Number.isFinite(meta?.vmax) ? Number(meta.vmax) : 0;

    // Layout (CSS pixels). Keep the whole chart centered inside the canvas.
    const marginLeft = 64;
    const marginTop = 32;
    const marginRight = 16;
    const legendH = 12;
    // Leave enough room for legend + numeric labels (avoid clipping).
    // If legend is hidden, keep the bottom margin compact.
    const marginBottom = showLegend ? 60 : 16;

    const plot = Math.max(1, Math.floor(Math.min(cssW - marginLeft - marginRight, cssH - marginTop - marginBottom)));
    const chartW = marginLeft + plot + marginRight;
    const chartH = marginTop + plot + marginBottom;
    const xChart = Math.floor((cssW - chartW) / 2);
    const yChart = Math.floor((cssH - chartH) / 2);
    const heatX = xChart + marginLeft;
    const heatY = yChart + marginTop;

    const styleSrc = canvas_wrap_el.value || canvas_el.value?.parentElement || document.body;
    const textColor = getComputedStyle(styleSrc).color || "#111";

    // Heatmap
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(off, 0, 0, n, n, heatX, heatY, plot, plot);

    // Chain boundary separators + segment labels (axes only).
    ctx.save();
    ctx.strokeStyle = textColor;
    ctx.fillStyle = textColor;
    ctx.lineWidth = 1;
    ctx.globalAlpha = 0.9;
    ctx.font = "12px sans-serif";

    // Separator ticks at chain boundaries (only in the margin areas; do not draw over the heatmap).
    ctx.globalAlpha = 0.7;
    const tickLen = 10;
    for (let s = 0; s < segments.length; s++) {
        const seg = segments[s];
        const boundary = seg.start;
        if (boundary <= 0) continue;
        const x = heatX + (boundary / n) * plot;
        const y = heatY + (boundary / n) * plot;

        // Top axis tick
        ctx.beginPath();
        ctx.moveTo(x, heatY - tickLen);
        ctx.lineTo(x, heatY);
        ctx.stroke();

        // Left axis tick
        ctx.beginPath();
        ctx.moveTo(heatX - tickLen, y);
        ctx.lineTo(heatX, y);
        ctx.stroke();
    }

    // Axis labels (top + left) per chain segment: show chain letter only.
    // (Collision detection intentionally removed.)
    ctx.globalAlpha = 0.95;
    for (let s = 0; s < segments.length; s++) {
        const seg = segments[s];
        const label = String(seg.chainId);

        const startFrac = seg.start / n;
        const endFrac = (seg.end + 1) / n;
        const x1 = heatX + startFrac * plot;
        const x2 = heatX + endFrac * plot;
        const y1 = heatY + startFrac * plot;
        const y2 = heatY + endFrac * plot;
        const xc = (x1 + x2) / 2;
        const yc = (y1 + y2) / 2;

        // Top
        ctx.textAlign = "center";
        ctx.textBaseline = "bottom";
        ctx.fillText(label, xc, heatY - 6);

        // Left
        ctx.textAlign = "right";
        ctx.textBaseline = "middle";
        ctx.fillText(label, heatX - 8, yc);
    }

    if (showLegend) {
        // Legend
        const legendX = heatX;
        const legendY = heatY + plot + 20;
        const grad = ctx.createLinearGradient(legendX, 0, legendX + plot, 0);
        for (const st of VIRIDIS_STOPS) {
            grad.addColorStop(st.t, `rgb(${st.c[0]},${st.c[1]},${st.c[2]})`);
        }
        ctx.textAlign = "left";
        ctx.textBaseline = "top";
        ctx.globalAlpha = 1;
        ctx.fillStyle = grad;
        ctx.fillRect(legendX, legendY, plot, legendH);
        ctx.strokeStyle = textColor;
        ctx.globalAlpha = 0.6;
        ctx.strokeRect(legendX, legendY, plot, legendH);
        ctx.globalAlpha = 0.9;
        ctx.fillStyle = textColor;
        ctx.fillText("0", legendX, legendY + legendH + 6);
        ctx.textAlign = "right";
        ctx.fillText(formatLegendNumber(vmax), legendX + plot, legendY + legendH + 6);
    }
    ctx.restore();
}

function paintToVisible(off, meta) {
    const canvas = canvas_el.value;
    if (!canvas || !off || !meta) return;
    const { cssW, cssH, dpr } = setCanvasSizeToCss(canvas);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, cssW, cssH);

    drawHeatmapWithAxesAndLegend(ctx, off, meta, cssW, cssH);
}

async function renderHeatmapCanvas(res) {
    if (!res) return;
    const axis = Array.isArray(res?.axis) ? res.axis : [];
    const n = Number(res?.n) || axis.length;
    const upper = res?.upper_values instanceof Float64Array ? res.upper_values : normalizeUpperValues(res?.upper_values);
    const expectUpper = (n * (n - 1)) / 2;
    if (n <= 0 || axis.length !== n) return;
    if (upper.length !== expectUpper && upper.length !== n * n) return;

    const vmax = Number.isFinite(res?.vmax) ? res.vmax : maxFinite(upper);
    const token = ++render_token;

    // Build pixel image in an offscreen canvas (one pixel per residue-residue cell).
    const off = document.createElement("canvas");
    off.width = n;
    off.height = n;
    const offCtx = off.getContext("2d");
    if (!offCtx) return;

    const img = offCtx.createImageData(n, n);
    const data = img.data;
    const denom = vmax > 0 ? vmax : 1;

    // Initialize pixels so partially rendered frames still show.
    // In the packed-upper path we don't explicitly write the diagonal, so set
    // the default RGB to the color at t=0 (i.e., value 0) instead of black.
    const base = viridisColor(0);
    for (let p = 0; p < data.length; p += 4) {
        data[p + 0] = base[0];
        data[p + 1] = base[1];
        data[p + 2] = base[2];
        data[p + 3] = 255;
    }

    const axisEntries = axis.map(parseAxisEntry);

    // Show progressive rendering: make the offscreen canvas available early.
    last_offscreen = off;
    last_meta = { axis, axisEntries, n, vmax };
    paintToVisible(last_offscreen, last_meta);

    // Fill symmetric pixels from packed upper triangle, chunked to keep UI responsive.
    const rowsPerFrame = 16;
    let i = 0;
    while (i < n) {
        if (token !== render_token) return;

        const iStart = i;
        const iEnd = Math.min(n, i + rowsPerFrame);
        for (; i < iEnd; i++) {
            if (upper.length === n * n) {
                // full matrix path
                for (let j = 0; j < n; j++) {
                    const v = i === j ? 0 : Number(upper[i * n + j] ?? 0);
                    const t = Math.max(0, Math.min(1, v / denom));
                    const [r, g, b] = viridisColor(t);
                    const p = (i * n + j) * 4;
                    data[p + 0] = r;
                    data[p + 1] = g;
                    data[p + 2] = b;
                }
                continue;
            }

            // packed upper (excluding diagonal)
            // k starts at prefix for this row
            let k = (i * (2 * n - i - 1)) / 2;
            for (let j = i + 1; j < n; j++) {
                const v = Number(upper[k++] ?? 0);
                const t = Math.max(0, Math.min(1, v / denom));
                const [r, g, b] = viridisColor(t);

                const p1 = (i * n + j) * 4;
                data[p1 + 0] = r;
                data[p1 + 1] = g;
                data[p1 + 2] = b;

                const p2 = (j * n + i) * 4;
                data[p2 + 0] = r;
                data[p2 + 1] = g;
                data[p2 + 2] = b;
            }

            // diagonal already set; also fill left-of-diagonal in this row from symmetry
            // (it was filled when processing earlier rows)
        }

        // Flush the updated rows to offscreen and repaint the visible canvas for interactivity.
        offCtx.putImageData(img, 0, 0, 0, iStart, n, iEnd - iStart);
        if (token !== render_token) return;
        paintToVisible(last_offscreen, last_meta);

        await new Promise((r) => requestAnimationFrame(r));
    }

    if (token !== render_token) return;
    // Final repaint in case the last frame was skipped.
    paintToVisible(last_offscreen, last_meta);
}

function handleResize() {
    if (last_offscreen && last_meta) paintToVisible(last_offscreen, last_meta);
}

onMounted(() => {
    window.addEventListener("resize", handleResize);
});

onBeforeUnmount(() => {
    window.removeEventListener("resize", handleResize);
});

watch(
    [current_result, is_results_view],
    async ([res, inResultsView]) => {
        if (!inResultsView) return;
        // Wait for the canvas container to appear (it's under v-if).
        await nextTick();
        await renderHeatmapCanvas(res);
    },
    { flush: "post" }
);

// If the user refreshes or directly visits ContactMap?view=results, we may have no
// in-memory results. In that case, fall back to the form view.
watch(
    () => route.query.view,
    (v) => {
        if (v !== "results") return;
        if (processing.value) return;
        if (has_results.value) return;
        const q = { ...route.query };
        delete q.view;
        router.replace({ query: q });
    },
    { immediate: true }
);
</script>

<template>
    <div class="mx-auto py-8 px-4" :class="is_results_view && has_results ? 'max-w-6xl' : 'max-w-3xl'">
        <div v-if="is_results_view && has_results">
            <div class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
                <div class="flex items-center justify-between gap-3">
                    <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Results</p>
                    <div class="flex items-center gap-2">
                        <button v-if="has_multiple_results" class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_next" @click="nextResult">Next</button>
                        <button class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download_all" @click="downloadAll">Download All (ZIP)</button>
                    </div>
                </div>
                <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

                <div class="rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800 max-h-screen overflow-y-auto">
                    <div v-if="current_result" class="space-y-3">
                        <div class="flex justify-between">
                            <div>
                                <div class="text-sm font-semibold text-gray-900 dark:text-gray-200">{{ current_title }}</div>
                                <div class="text-xs text-gray-500 dark:text-gray-300">chain_id: {{ current_result.chain_id || "all" }} · render: {{ current_result.n }}×{{ current_result.n }}</div>
                            </div>
                            <div class="flex items-center gap-2">
                                <button class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download" @click="downloadCurrent">Download (CSV)</button>
                                <button class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm font-medium text-gray-900 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:hover:bg-gray-600" :disabled="!can_download" @click="downloadHeatmapPng">Download Heatmap (PNG)</button>
                            </div>
                        </div>

                        <div ref="canvas_wrap_el" class="w-full h-[600px] flex justify-center text-gray-900 dark:text-gray-200">
                            <canvas ref="canvas_el" class="h-full w-auto max-w-full" style="aspect-ratio: 1 / 1"></canvas>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <form v-else @submit.prevent="runDMap" class="w-full bg-white rounded-lg shadow-xl p-8 dark:bg-gray-900">
            <div class="flex w-full justify-start">
                <p class="text-3xl font-semibold text-gray-900 dark:text-gray-400">Contact Map</p>
            </div>
            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <InputStructure v-model:input_method="input_method" v-model:ids="ids" v-model:files="files" :max-files="200" :max-size="500 * 1024 * 1024" />

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <div class="my-4">
                <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Chain</span>
            </div>

            <div class="w-auto">
                <div>
                    <label class="block mb-2 text-sm font-medium text-gray-900 dark:text-gray-300">Chain ID <span @click="chain_id_example" class="text-xs cursor-pointer hover:text-blue-700 hover:underline font-normal">(e.g., A. Optional, leave empty for all chains)</span></label>
                    <input type="text" v-model="chain_id" class="w-full rounded-lg border border-gray-300 bg-gray-50 p-2.5 text-sm focus:outline-none focus:ring-1 focus:ring-blue-400 focus:border-blue-400 text-gray-900 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:placeholder-gray-400" />
                </div>
            </div>

            <hr class="h-px my-4 bg-gray-200 border-0 dark:bg-gray-700" />

            <button type="submit" class="w-full rounded-lg bg-blue-600 px-4 py-2 text-lg text-center font-medium text-white hover:bg-blue-700">
                {{ run_button_text }}
            </button>

            <div v-if="error_message" class="mt-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950 dark:text-red-300">
                {{ error_message }}
            </div>

            <div v-if="file_errors.length > 0" class="mt-4 rounded-lg border border-yellow-200 bg-yellow-50 p-3 text-sm text-yellow-800 dark:border-yellow-900 dark:bg-yellow-950 dark:text-yellow-200">
                <div class="font-medium">Failed to process the following file(s)</div>
                <ul class="mt-2 space-y-1">
                    <li v-for="e in file_errors" :key="e.source" class="text-xs">
                        <span class="font-semibold">{{ e.source }}</span
                        >: {{ e.message }}
                    </li>
                </ul>
            </div>
        </form>
    </div>
</template>
