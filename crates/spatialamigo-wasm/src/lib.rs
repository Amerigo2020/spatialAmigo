//! WebAssembly bindings for spatialAmigo.
//!
//! JSON strings in, JSON strings out: the browser side works with plain
//! objects (`JSON.parse`) and the Rust side keeps full validation.
//!
//! ```js
//! import init, { WasmResolver } from "./pkg/spatialamigo_wasm.js";
//! await init();
//! const r = new WasmResolver(sceneJson);
//! const res = JSON.parse(r.resolve("the mug left of the laptop"));
//! console.log(res.best, res.explanation);
//! ```

use spatialamigo::{Query, Resolver, Scene};
use wasm_bindgen::prelude::*;

fn err(e: spatialamigo::Error) -> JsError {
    JsError::new(&e.to_string())
}

/// Library version.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Parses a sentence with the template parser. `labels_json` is a JSON array of
/// known labels (may be `"[]"`). Returns the query as JSON.
#[wasm_bindgen]
pub fn parse(text: &str, labels_json: &str) -> Result<String, JsError> {
    let labels: Vec<String> = serde_json::from_str(labels_json).map_err(|e| JsError::new(&e.to_string()))?;
    spatialamigo::parse(text, &labels)
        .map(|q| q.to_json())
        .map_err(err)
}

/// Validates a scene and returns its labels as a JSON array.
#[wasm_bindgen]
pub fn scene_labels(scene_json: &str) -> Result<String, JsError> {
    let s = Scene::from_json(scene_json).map_err(err)?;
    Ok(serde_json::to_string(&s.labels()).expect("labels serialize"))
}

/// Resolver bound to one scene.
#[wasm_bindgen]
pub struct WasmResolver {
    inner: Resolver,
}

#[wasm_bindgen]
impl WasmResolver {
    /// Builds a resolver from scene JSON (format v0.1).
    #[wasm_bindgen(constructor)]
    pub fn new(scene_json: &str) -> Result<WasmResolver, JsError> {
        Ok(WasmResolver {
            inner: Resolver::new(Scene::from_json(scene_json).map_err(err)?),
        })
    }

    /// Resolves a sentence, or a structured query if `input` starts with `{`.
    /// Returns the full resolution (candidates, trace, explanation) as JSON.
    pub fn resolve(&self, input: &str) -> Result<String, JsError> {
        let trimmed = input.trim_start();
        let res = if trimmed.starts_with('{') {
            let q = Query::from_json(trimmed).map_err(err)?;
            self.inner.resolve(&q)
        } else {
            self.inner.resolve_text(input)
        };
        res.map(|r| serde_json::to_string(&r).expect("resolution serialize"))
            .map_err(err)
    }

    /// Labels of the scene as a JSON array.
    pub fn labels(&self) -> String {
        serde_json::to_string(&self.inner.scene().labels()).expect("labels serialize")
    }
}

/// One-shot resolve without keeping a resolver.
#[wasm_bindgen]
pub fn resolve(scene_json: &str, input: &str) -> Result<String, JsError> {
    WasmResolver::new(scene_json)?.resolve(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_on_native_target() {
        let scene = include_str!("../../../examples/scenes/tabletop.json");
        let r = WasmResolver::new(scene).ok().unwrap();
        let out: serde_json::Value =
            serde_json::from_str(&r.resolve("the mug left of the laptop").ok().unwrap()).unwrap();
        assert_eq!(out["best"], "mug_2");
        let q = r#"{"target":"mug","constraints":[{"relation":"closest","anchors":["bowl"]}]}"#;
        let out: serde_json::Value = serde_json::from_str(&r.resolve(q).ok().unwrap()).unwrap();
        assert_eq!(out["best"], "mug_1");
        assert!(version().starts_with("0."));
    }
}
