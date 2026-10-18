//! Host bindings; exchange behavior remains in the SDK.

#[cfg(all(feature = "node", feature = "wasm"))]
compile_error!("node and wasm are mutually exclusive host features");
#[cfg(all(feature = "node", target_arch = "wasm32"))]
compile_error!("node requires a native target");
#[cfg(all(feature = "wasm", not(target_arch = "wasm32")))]
compile_error!("wasm requires the wasm32 target");

#[cfg_attr(feature = "node", napi_derive::napi(js_name = "parity_probe"))]
#[cfg_attr(feature = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn parity_probe(n: u32) -> String {
    format!("safe-cex:{n}")
}

#[cfg_attr(feature = "node", napi_derive::napi)]
#[cfg_attr(feature = "wasm", wasm_bindgen::prelude::wasm_bindgen)]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(feature = "node")]
mod native_callback;
#[cfg(feature = "node")]
mod native_exchange;
#[cfg(feature = "node")]
mod native_value;
