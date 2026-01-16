let _wasmPromise;

async function getWasm() {
    if (!_wasmPromise) {
        _wasmPromise = import("@local/pskit-wasm");
    }
    return await _wasmPromise;
}

function assertUint8Array(v, name) {
    if (!(v instanceof Uint8Array)) throw new Error(`${name} must be Uint8Array`);
}

function assertString(v, name) {
    if (typeof v !== "string") throw new Error(`${name} must be string`);
}

function assertNumber(v, name) {
    if (typeof v !== "number" || Number.isNaN(v)) throw new Error(`${name} must be number`);
}

function normalizeOptionalString(v) {
    if (v === null || v === undefined) return undefined;
    return String(v);
}

function chunksToItems(chunks) {
    const keys = chunks.keys();
    const items = [];
    const transfer = [];
    for (const k of keys) {
        const key = String(k);
        const outBytes = chunks.take(key);
        items.push({ key, bytes: outBytes });
        transfer.push(outBytes.buffer);
    }
    return { items, transfer };
}

const handlers = {
    split_complex: (wasm, msg) => {
        const bytes = msg.bytes;
        const format = msg.format;
        assertUint8Array(bytes, "bytes");
        assertString(format, "format");

        const chunks = wasm.split_complex(bytes, format);
        try {
            const { items, transfer } = chunksToItems(chunks);
            return { payload: { ok: true, kind: "chunks", items }, transfer };
        } finally {
            try {
                chunks.free();
            } catch {
                // ignore
            }
        }
    },

    split_by_chain: (wasm, msg) => {
        const bytes = msg.bytes;
        const format = msg.format;
        assertUint8Array(bytes, "bytes");
        assertString(format, "format");

        const chunks = wasm.split_by_chain(bytes, format);
        try {
            const { items, transfer } = chunksToItems(chunks);
            return { payload: { ok: true, kind: "chunks", items }, transfer };
        } finally {
            try {
                chunks.free();
            } catch {
                // ignore
            }
        }
    },

    extract_fragment: (wasm, msg) => {
        const bytes = msg.bytes;
        const chain_id = msg.chain_id;
        const start = msg.start;
        const end = msg.end;
        const format = msg.format;
        assertUint8Array(bytes, "bytes");
        assertString(chain_id, "chain_id");
        assertString(format, "format");

        const out = wasm.extract_fragment(bytes, chain_id, start, end, format);

        try {
            const outBytes = out.take_bytes();
            const startOut = out.start;
            const endOut = out.end;
            assertUint8Array(outBytes, "result");
            return {
                payload: { ok: true, kind: "fragment", bytes: outBytes, start: startOut, end: endOut },
                transfer: [outBytes.buffer],
            };
        } finally {
            try {
                out.free?.();
            } catch {
                // ignore
            }
        }
    },

    annotate_binding_pairs: (wasm, msg) => {
        const bytes = msg.bytes;
        const cutoff = msg.cutoff;
        const format = msg.format;
        assertUint8Array(bytes, "bytes");
        assertNumber(cutoff, "cutoff");
        assertString(format, "format");

        const out = wasm.annotate_binding_pairs(bytes, cutoff, format);

        try {
            const pairsRaw = out.take_pairs();
            const distancesRaw = out.take_distances();
            const pairs = Array.from(pairsRaw || []).map((x) => String(x));
            const distances = Array.from(distancesRaw || []).map((x) => Number(x));
            return { payload: { ok: true, kind: "binding_pairs", pairs, distances }, transfer: [] };
        } finally {
            try {
                out.free();
            } catch {
                // ignore
            }
        }

    },

    d_map: (wasm, msg) => {
        const bytes = msg.bytes;
        const chain_id = msg.chain_id;
        const format = msg.format;
        assertUint8Array(bytes, "bytes");
        assertString(format, "format");

        const cm = wasm.d_map(bytes, chain_id, format);
        try {
            const axisArr = cm.take_axis();
            const axis = Array.from(axisArr || []).map((x) => String(x));

            const rawValues = cm.take_values();
            let values;
            if (rawValues instanceof Float64Array) values = rawValues;
            else if (Array.isArray(rawValues)) values = Float64Array.from(rawValues.map((x) => Number(x)));
            else values = Float64Array.from(Array.from(rawValues || []).map((x) => Number(x)));

            return { payload: { ok: true, kind: "contact_map", axis, values }, transfer: [values.buffer] };
        } finally {
            try {
                cm.free();
            } catch {
                // ignore
            }
        }
    },
};

self.onmessage = async (ev) => {
    const msg = ev.data || {};
    const { id, fn } = msg;

    try {
        if (!id) throw new Error("Missing request id");

        const wasm = await getWasm();

        const h = handlers[String(fn || "")];
        if (!h) throw new Error(`Unsupported fn: ${fn}`);

        const { payload, transfer } = h(wasm, msg);
        self.postMessage({ id, ...payload }, transfer || []);
        return;
    } catch (e) {
        const error = e?.message ? String(e.message) : String(e);
        self.postMessage({ id, ok: false, error });
    }
};
