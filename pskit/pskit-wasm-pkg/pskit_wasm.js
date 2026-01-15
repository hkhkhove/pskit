import * as wasm from "./pskit_wasm_bg.wasm";
export * from "./pskit_wasm_bg.js";
import { __wbg_set_wasm } from "./pskit_wasm_bg.js";
__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
