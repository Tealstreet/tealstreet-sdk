//! Optional fixture probes, excluded from production venue builds.
use serde::de::IgnoredAny;
use wasm_bindgen::prelude::*;

/// Includes wasm-bindgen's UTF-8 encoding, allocation, copy and call overhead.
#[wasm_bindgen(js_name = profileAcceptText)]
pub fn accept_text(text: &str) -> usize {
    text.len()
}

/// Includes the byte-array crossing used by websocket batches.
#[wasm_bindgen(js_name = profileAcceptBytes)]
pub fn accept_bytes(bytes: &[u8]) -> usize {
    bytes.len()
}

/// JSON validation floor; this intentionally does not model typed venue parsing.
#[wasm_bindgen(js_name = profileValidateJson)]
pub fn validate_json(text: &str) -> bool {
    serde_json::from_str::<IgnoredAny>(text).is_ok()
}
