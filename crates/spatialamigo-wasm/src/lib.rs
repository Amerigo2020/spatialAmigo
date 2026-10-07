//! WebAssembly bindings for spatialAmigo (milestone M5).

use wasm_bindgen::prelude::*;

/// Library version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
