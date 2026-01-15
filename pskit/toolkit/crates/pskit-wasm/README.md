# pskit-wasm

This project is a WebAssembly module for processing protein structures. It provides an interface to the core functionality of the `pskit-core` library, which includes modules for annotating, mapping, and splitting protein structures.

## Overview

The `pskit-wasm` module allows you to leverage the capabilities of the `pskit-core` library in a web environment. By compiling the core library to WebAssembly, you can efficiently perform protein structure analysis directly in the browser.

## Features

-   **Protein Annotation**: Functions to annotate protein structures with relevant information.
-   **Structure Mapping**: Tools to map protein structures for various analyses.
-   **Structure Splitting**: Methods to split protein structures into manageable components.

## Getting Started

cargo build --lib --target wasm32-unknown-unknown --release
wasm-bindgen ../../target/wasm32-unknown-unknown/release/pskit_wasm.wasm --out-dir ../../pkg --target bundler
