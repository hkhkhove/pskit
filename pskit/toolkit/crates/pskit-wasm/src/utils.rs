use crate::{annotate, contact, split};
use js_sys::{Array, Uint8Array};
use std::collections::HashMap;
use std::io::Cursor;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Chunks {
    parts: HashMap<String, Vec<u8>>,
}

#[wasm_bindgen]
impl Chunks {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Chunks {
        Chunks {
            parts: HashMap::new(),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    #[wasm_bindgen]
    pub fn keys(&mut self) -> Array {
        let arr = Array::new();
        for k in self.parts.keys() {
            arr.push(&JsValue::from_str(k));
        }
        arr
    }

    /// Take (remove) a chunk by key and return it as JS Uint8Array (owned by JS).
    ///
    /// This is a "consuming" API: once taken, it is removed from WASM memory.
    /// Safe to keep the returned Uint8Array across async boundaries.
    #[wasm_bindgen]
    pub fn take(&mut self, key: &str) -> Result<Uint8Array, JsValue> {
        let v = self
            .parts
            .remove(key)
            .ok_or_else(|| js_sys::Error::new(&format!("Key not exists: {key}")))?;

        // Copy into a JS-owned Uint8Array.
        Ok(Uint8Array::from(v.as_slice()))
    }
}

#[wasm_bindgen]
pub struct Fragment {
    bytes: Vec<u8>,
    start: isize,
    end: isize,
}

#[wasm_bindgen]
impl Fragment {
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
    pub fn start(&self) -> isize {
        self.start
    }
    pub fn end(&self) -> isize {
        self.end
    }
}

#[wasm_bindgen]
pub struct ContactMap {
    axis: Vec<String>,
    values: Vec<f64>,
}

#[wasm_bindgen]
impl ContactMap {
    #[wasm_bindgen]
    pub fn axis(&self) -> Array {
        let arr = Array::new();
        for res_name in &self.axis {
            arr.push(&JsValue::from_str(res_name));
        }
        arr
    }

    #[wasm_bindgen]
    pub fn values(&self) -> Vec<f64> {
        self.values.clone()
    }
}

#[wasm_bindgen]
pub struct BindingPairs {
    pairs: Vec<String>,
    distances: Vec<f64>,
}

#[wasm_bindgen]
impl BindingPairs {
    #[wasm_bindgen]
    pub fn pairs(&self) -> Array {
        let arr = Array::new();
        for pair in &self.pairs {
            arr.push(&JsValue::from_str(pair));
        }
        arr
    }

    #[wasm_bindgen]
    pub fn distances(&self) -> Vec<f64> {
        self.distances.clone()
    }
}

#[wasm_bindgen]
pub fn split_complex(input: &[u8], format: &str) -> Result<Chunks, JsValue> {
    let cursor = Cursor::new(input);
    let parts = split::split_complex(cursor, format).map_err(|e| JsValue::from_str(&e))?;
    Ok(Chunks { parts })
}

#[wasm_bindgen]
pub fn split_by_chain(input: &[u8], format: &str) -> Result<Chunks, JsValue> {
    let cursor = Cursor::new(input);
    let parts = split::split_by_chain(cursor, format).map_err(|e| JsValue::from_str(&e))?;
    Ok(Chunks { parts })
}

#[wasm_bindgen]
pub fn extract_fragment(
    input: &[u8],
    chain_id: String,
    start: Option<isize>,
    end: Option<isize>,
    format: &str,
) -> Result<Fragment, JsValue> {
    let cursor = Cursor::new(input);
    let (bytes, start, end) = split::extract_fragment(cursor, chain_id, start, end, format)
        .map_err(|e| JsValue::from_str(&e))?;
    Ok(Fragment { bytes, start, end })
}

#[wasm_bindgen]
pub fn d_map(input: &[u8], chain_id: Option<String>, format: &str) -> Result<ContactMap, JsValue> {
    let cursor = Cursor::new(input);
    let (axis, values) =
        contact::d_map(cursor, chain_id, format).map_err(|e| JsValue::from_str(&e))?;

    Ok(ContactMap {
        axis,
        values: values.into_iter().flatten().collect(),
    })
}

#[wasm_bindgen]
pub fn annotate_binding_pairs(
    input: &[u8],
    cutoff: f64,
    format: &str,
) -> Result<BindingPairs, JsValue> {
    let cursor = Cursor::new(input);
    let pairs = annotate::compute_binding_pairs(cursor, cutoff, format)
        .map_err(|e| JsValue::from_str(&e))?;
    let (pairs, distances): (Vec<String>, Vec<f64>) = pairs.into_iter().unzip();

    Ok(BindingPairs { pairs, distances })
}
