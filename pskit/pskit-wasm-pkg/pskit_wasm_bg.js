let wasm;
export function __wbg_set_wasm(val) {
    wasm = val;
}


let cachedUint8ArrayMemory0 = null;

function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });

cachedTextDecoder.decode();

const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedFloat64ArrayMemory0 = null;

function getFloat64ArrayMemory0() {
    if (cachedFloat64ArrayMemory0 === null || cachedFloat64ArrayMemory0.byteLength === 0) {
        cachedFloat64ArrayMemory0 = new Float64Array(wasm.memory.buffer);
    }
    return cachedFloat64ArrayMemory0;
}

function getArrayF64FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getFloat64ArrayMemory0().subarray(ptr / 8, ptr / 8 + len);
}

let WASM_VECTOR_LEN = 0;

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    }
}

function passStringToWasm0(arg, malloc, realloc) {

    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }

    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}
/**
 * @param {Uint8Array} input
 * @param {number} cutoff
 * @param {string} format
 * @returns {BindingPairs}
 */
export function annotate_binding_pairs(input, cutoff, format) {
    const ptr0 = passArray8ToWasm0(input, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(format, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.annotate_binding_pairs(ptr0, len0, cutoff, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return BindingPairs.__wrap(ret[0]);
}

/**
 * @param {Uint8Array} input
 * @param {string} format
 * @returns {Chunks}
 */
export function split_complex(input, format) {
    const ptr0 = passArray8ToWasm0(input, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(format, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.split_complex(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return Chunks.__wrap(ret[0]);
}

/**
 * @param {Uint8Array} input
 * @param {string} format
 * @returns {Chunks}
 */
export function split_by_chain(input, format) {
    const ptr0 = passArray8ToWasm0(input, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(format, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.split_by_chain(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return Chunks.__wrap(ret[0]);
}

function isLikeNone(x) {
    return x === undefined || x === null;
}
/**
 * @param {Uint8Array} input
 * @param {string} chain_id
 * @param {number | null | undefined} start
 * @param {number | null | undefined} end
 * @param {string} format
 * @returns {Fragment}
 */
export function extract_fragment(input, chain_id, start, end, format) {
    const ptr0 = passArray8ToWasm0(input, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(chain_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(format, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.extract_fragment(ptr0, len0, ptr1, len1, isLikeNone(start) ? 0x100000001 : (start) >> 0, isLikeNone(end) ? 0x100000001 : (end) >> 0, ptr2, len2);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return Fragment.__wrap(ret[0]);
}

/**
 * @param {Uint8Array} input
 * @param {string | null | undefined} chain_id
 * @param {string} format
 * @returns {ContactMap}
 */
export function d_map(input, chain_id, format) {
    const ptr0 = passArray8ToWasm0(input, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    var ptr1 = isLikeNone(chain_id) ? 0 : passStringToWasm0(chain_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(format, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.d_map(ptr0, len0, ptr1, len1, ptr2, len2);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return ContactMap.__wrap(ret[0]);
}

const BindingPairsFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_bindingpairs_free(ptr >>> 0, 1));

export class BindingPairs {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(BindingPairs.prototype);
        obj.__wbg_ptr = ptr;
        BindingPairsFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        BindingPairsFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_bindingpairs_free(ptr, 0);
    }
    /**
     * Take the pairs array (consuming).
     * @returns {Array<any> | undefined}
     */
    take_pairs() {
        const ret = wasm.bindingpairs_take_pairs(this.__wbg_ptr);
        return ret;
    }
    /**
     * Take the distances as Float64Array (consuming).
     * @returns {Float64Array | undefined}
     */
    take_distances() {
        const ret = wasm.bindingpairs_take_distances(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) BindingPairs.prototype[Symbol.dispose] = BindingPairs.prototype.free;

const ChunksFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_chunks_free(ptr >>> 0, 1));

export class Chunks {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Chunks.prototype);
        obj.__wbg_ptr = ptr;
        ChunksFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ChunksFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_chunks_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get len() {
        const ret = wasm.chunks_len(this.__wbg_ptr);
        return ret >>> 0;
    }
    constructor() {
        const ret = wasm.chunks_new();
        this.__wbg_ptr = ret >>> 0;
        ChunksFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @returns {Array<any>}
     */
    keys() {
        const ret = wasm.chunks_keys(this.__wbg_ptr);
        return ret;
    }
    /**
     * Take (remove) a chunk by key and return it as JS Uint8Array (owned by JS).
     *
     * This is a "consuming" API: once taken, it is removed from WASM memory.
     * Safe to keep the returned Uint8Array across async boundaries.
     * @param {string} key
     * @returns {Uint8Array}
     */
    take(key) {
        const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.chunks_take(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
}
if (Symbol.dispose) Chunks.prototype[Symbol.dispose] = Chunks.prototype.free;

const ContactMapFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_contactmap_free(ptr >>> 0, 1));

export class ContactMap {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(ContactMap.prototype);
        obj.__wbg_ptr = ptr;
        ContactMapFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ContactMapFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_contactmap_free(ptr, 0);
    }
    /**
     * Take the values as a flat Float64Array (consuming, row-major order).
     * @returns {Float64Array | undefined}
     */
    take_values() {
        const ret = wasm.contactmap_take_values(this.__wbg_ptr);
        return ret;
    }
    /**
     * Take the axis labels (consuming).
     * @returns {Array<any> | undefined}
     */
    take_axis() {
        const ret = wasm.contactmap_take_axis(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) ContactMap.prototype[Symbol.dispose] = ContactMap.prototype.free;

const FragmentFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_fragment_free(ptr >>> 0, 1));

export class Fragment {

    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(Fragment.prototype);
        obj.__wbg_ptr = ptr;
        FragmentFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }

    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        FragmentFinalization.unregister(this);
        return ptr;
    }

    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_fragment_free(ptr, 0);
    }
    /**
     * Take the bytes out as a JS Uint8Array (consuming, avoids double memory).
     * Returns None if already taken.
     * @returns {Uint8Array | undefined}
     */
    take_bytes() {
        const ret = wasm.fragment_take_bytes(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get end() {
        const ret = wasm.chunks_len(this.__wbg_ptr);
        return ret;
    }
    /**
     * @returns {number}
     */
    get start() {
        const ret = wasm.fragment_start(this.__wbg_ptr);
        return ret;
    }
}
if (Symbol.dispose) Fragment.prototype[Symbol.dispose] = Fragment.prototype.free;

export function __wbg___wbindgen_throw_b855445ff6a94295(arg0, arg1) {
    throw new Error(getStringFromWasm0(arg0, arg1));
};

export function __wbg_new_a7442b4b19c1a356(arg0, arg1) {
    const ret = new Error(getStringFromWasm0(arg0, arg1));
    return ret;
};

export function __wbg_new_e17d9f43105b08be() {
    const ret = new Array();
    return ret;
};

export function __wbg_new_from_slice_92f4d78ca282a2d2(arg0, arg1) {
    const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1));
    return ret;
};

export function __wbg_new_from_slice_fde3e31e670b38a6(arg0, arg1) {
    const ret = new Float64Array(getArrayF64FromWasm0(arg0, arg1));
    return ret;
};

export function __wbg_push_df81a39d04db858c(arg0, arg1) {
    const ret = arg0.push(arg1);
    return ret;
};

export function __wbindgen_cast_2241b6af4c4b2941(arg0, arg1) {
    // Cast intrinsic for `Ref(String) -> Externref`.
    const ret = getStringFromWasm0(arg0, arg1);
    return ret;
};

export function __wbindgen_init_externref_table() {
    const table = wasm.__wbindgen_externrefs;
    const offset = table.grow(4);
    table.set(0, undefined);
    table.set(offset + 0, undefined);
    table.set(offset + 1, null);
    table.set(offset + 2, true);
    table.set(offset + 3, false);
    ;
};

