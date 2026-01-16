cargo build --lib --target wasm32-unknown-unknown --release
wasm-bindgen ../../target/wasm32-unknown-unknown/release/pskit_wasm.wasm --out-dir ../../../pskit-wasm-pkg
