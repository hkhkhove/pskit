import JSZip from "jszip";

let _pskitWorker;
let _pskitWorkerReqId = 1;
const _pskitWorkerPending = new Map();

function getPskitWorker() {
    if (_pskitWorker) return _pskitWorker;

    _pskitWorker = new Worker(new URL("../workers/pskitWasm.worker.js", import.meta.url), { type: "module" });

    _pskitWorker.onmessage = (ev) => {
        const msg = ev.data || {};
        const pending = _pskitWorkerPending.get(msg.id);
        if (!pending) return;
        _pskitWorkerPending.delete(msg.id);

        if (msg.ok) pending.resolve(msg);
        else pending.reject(new Error(msg.error || "Worker error"));
    };

    _pskitWorker.onerror = (ev) => {
        const message = ev?.message ? String(ev.message) : "Worker error";
        for (const [, pending] of _pskitWorkerPending) {
            pending.reject(new Error(message));
        }
        _pskitWorkerPending.clear();
    };

    _pskitWorker.onmessageerror = () => {
        for (const [, pending] of _pskitWorkerPending) {
            pending.reject(new Error("Worker message error"));
        }
        _pskitWorkerPending.clear();
    };

    return _pskitWorker;
}

function callPskitWorker(payload, transfer, { timeoutMs = 60000 } = {}) {
    const worker = getPskitWorker();
    const id = _pskitWorkerReqId++;
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
            _pskitWorkerPending.delete(id);
            reject(new Error(`Worker timeout after ${timeoutMs}ms`));
        }, timeoutMs);

        _pskitWorkerPending.set(id, {
            resolve: (v) => {
                clearTimeout(timer);
                resolve(v);
            },
            reject: (e) => {
                clearTimeout(timer);
                reject(e);
            },
        });

        worker.postMessage({ id, ...payload }, transfer);
    });
}

export async function splitComplexInWorker(bytes, format) {
    if (!(bytes instanceof Uint8Array)) {
        throw new Error("bytes must be Uint8Array");
    }
    return await callPskitWorker({ fn: "split_complex", bytes, format }, [bytes.buffer], { timeoutMs: 120000 });
}

export async function splitByChainInWorker(bytes, format) {
    if (!(bytes instanceof Uint8Array)) {
        throw new Error("bytes must be Uint8Array");
    }
    return await callPskitWorker({ fn: "split_by_chain", bytes, format }, [bytes.buffer], { timeoutMs: 120000 });
}

export async function extractFragmentInWorker(bytes, chain_id, start, end, format) {
    if (!(bytes instanceof Uint8Array)) {
        throw new Error("bytes must be Uint8Array");
    }
    if (typeof chain_id !== "string") {
        throw new Error("chain_id must be string");
    }
    if (typeof start !== "number" && start !== null && start !== undefined) {
        throw new Error("start must be number|null|undefined");
    }
    if (typeof end !== "number" && end !== null && end !== undefined) {
        throw new Error("end must be number|null|undefined");
    }
    if (typeof format !== "string") {
        throw new Error("format must be string");
    }

    const msg = await callPskitWorker(
        { fn: "extract_fragment", bytes, chain_id: chain_id, start, end, format },
        [bytes.buffer],
        { timeoutMs: 120000 },
    );
    
    return { bytes: msg.bytes, start: msg.start, end: msg.end };
}

export async function annotateBindingPairsInWorker(bytes, cutoff, format) {
    if (!(bytes instanceof Uint8Array)) {
        throw new Error("bytes must be Uint8Array");
    }
    if (typeof cutoff !== "number" || Number.isNaN(cutoff)) {
        throw new Error("cutoff must be number");
    }
    if (typeof format !== "string") {
        throw new Error("format must be string");
    }

    return await callPskitWorker({ fn: "annotate_binding_pairs", bytes, cutoff, format }, [bytes.buffer], { timeoutMs: 120000 });
}

export async function dMapInWorker(bytes, chain_id, format) {
    if (!(bytes instanceof Uint8Array)) {
        throw new Error("bytes must be Uint8Array");
    }
    if (chain_id !== null && chain_id !== undefined && typeof chain_id !== "string") {
        throw new Error("chain_id must be string|null|undefined");
    }
    if (typeof format !== "string") {
        throw new Error("format must be string");
    }

    return await callPskitWorker(
        { fn: "d_map", bytes, chain_id: chain_id ?? undefined, format },
        [bytes.buffer],
        { timeoutMs: 120000 },
    );
}

export function getFormatFromFileName(fileName) {
    const m = String(fileName).toLowerCase().match(/\.(pdb|cif)$/);
    return m ? m[1] : "pdb";
}

export function stripExtension(fileName) {
    return String(fileName).replace(/\.(pdb|cif)$/i, "");
}

export function sanitizeKey(key) {
    return String(key).replace(/[^a-zA-Z0-9._-]+/g, "_");
}

export function parsePdbIds(raw) {
    if (!raw) return [];
    return String(raw)
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
        .map((s) => s.toLowerCase());
}

export function isValidPdbId(id) {
    return /^[a-z0-9]{4}$/.test(String(id));
}

export async function fetchRcsbFile(id) {
    const upper = String(id).toUpperCase();
    const tryUrls = [
        { url: `https://files.rcsb.org/download/${upper}.cif`, format: "cif", source: `${upper}.cif` },
        { url: `https://files.rcsb.org/download/${upper}.pdb`, format: "pdb", source: `${upper}.pdb` },
    ];

    let lastError = "";
    for (const t of tryUrls) {
        try {
            const resp = await fetch(t.url);
            if (!resp.ok) {
                lastError = `${t.url} return ${resp.status}`;
                continue;
            }
            const buf = await resp.arrayBuffer();
            return { bytes: new Uint8Array(buf), format: t.format, source: t.source, base: upper };
        } catch (e) {
            lastError = e?.message ? String(e.message) : String(e);
        }
    }

    throw new Error(`Download failed: ${upper}(${lastError || "unknown"})`);
}

export function revokeDownloadItems(items) {
    for (const it of items || []) {
        try {
            if (it?.url) URL.revokeObjectURL(it.url);
        } catch {
            // ignore
        }
    }
}

export function groupDownloadItemsBySource(items) {
    const map = new Map();
    for (const r of items || []) {
        const key = r.source || "results";
        if (!map.has(key)) map.set(key, []);
        map.get(key).push(r);
    }
    return Array.from(map.entries()).map(([source, groupItems]) => ({ source, items: groupItems }));
}

export function bytesToDownloadItem({ bytes, filename, source, key }) {
    const blob = new Blob([bytes], { type: "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    return {
        source: source || filename,
        key: key ?? "",
        filename,
        url,
        size: blob.size,
        blob,
    };
}

export function chunksToDownloadItems({ chunks, base, format, source }) {
    const out = [];
    const keys = chunks.keys();

    for (const key of keys) {
        const data = chunks.take(String(key));
        const safeKey = sanitizeKey(key);
        const filename = `${base}.${safeKey}.${format}`;
        out.push(bytesToDownloadItem({ bytes: data, filename, source, key: String(key) }));
    }

    return out;
}

export function workerChunksToDownloadItems({ items, base, format, source }) {
    const out = [];
    for (const it of items || []) {
        const key = String(it.key);
        const safeKey = sanitizeKey(key);
        const filename = `${base}.${safeKey}.${format}`;
        out.push(bytesToDownloadItem({ bytes: it.bytes, filename, source, key }));
    }
    return out;
}

export async function prepareInputsFromFiles(files) {
    const inputs = [];
    for (const f of files || []) {
        const format = getFormatFromFileName(f.name);
        const base = stripExtension(f.name);
        const bytes = new Uint8Array(await f.arrayBuffer());
        inputs.push({ source: f.name, base, format, bytes });
    }
    return inputs;
}

export async function prepareInputsFromPdbIds(ids) {
    const inputs = [];
    for (const id of ids || []) {
        const downloaded = await fetchRcsbFile(id);
        inputs.push({ source: downloaded.source, base: downloaded.base, format: downloaded.format, bytes: downloaded.bytes });
    }
    return inputs;
}

/**
 * Run a batch job sequentially (safe for WASM + UI), collecting download items.
 *
 * @param {{
 *  inputs: Array<{source:string, base:string, format:string, bytes:Uint8Array}>,
 *  processOne: (input:{source:string, base:string, format:string, bytes:Uint8Array}) => any,
 *  toDownloadItems: (result:any, input:{source:string, base:string, format:string, bytes:Uint8Array}) => Array<any>,
 *  onProgress?: (p:{current:number,total:number,current_file:string})=>void,
 *  onError?: (e:{source:string,message:string})=>void,
 *  disposeResult?: (result:any)=>void,
 * }} params
 */
export async function runBatch({ inputs, processOne, toDownloadItems, onProgress, onError, disposeResult }) {
    const downloads = [];
    const errors = [];

    const total = inputs?.length || 0;

    for (let i = 0; i < total; i++) {
        const input = inputs[i];
        onProgress?.({ current: i + 1, total, current_file: input?.source ?? "" });

        let result;
        try {
            result = await processOne(input);
            const produced = await toDownloadItems(result, input);
            for (const it of produced) downloads.push(it);
        } catch (e) {
            const message = e?.message ? String(e.message) : String(e);
            const err = { source: input?.source ?? "", message };
            errors.push(err);
            onError?.(err);
        } finally {
            try {
                disposeResult?.(result);
            } catch {
                // ignore
            }
        }
    }

    return { downloads, errors };
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

export async function downloadGroupedAsZip(grouped, zipName = "results.zip") {
    const zip = new JSZip();
    const usedPaths = new Set();

    for (const group of grouped || []) {
        const folderName = sanitizeKey(stripExtension(group.source || "results")) || "results";
        const folder = zip.folder(folderName) ?? zip;

        for (const r of group.items || []) {
            const buf = await r.blob.arrayBuffer();
            const safeFile = uniqueZipName(r.filename, usedPaths);
            folder.file(safeFile, buf);
        }
    }

    const blob = await zip.generateAsync({ type: "blob" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = zipName;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
}
