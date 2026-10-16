//! MEXC WebSocket protocol values and frame routing helpers.
//!
//! Lifecycle and host I/O are owned by `safe-cex-exchange::BaseWebSocket`;
//! this module contains venue protocol decisions only.

use safe_cex_num::{JsJsonValue, js_json_stringify};
use serde_json::Value;

/// `JSON.stringify({method, param})` in source property order; an absent
/// `param` is omitted while an explicit JSON null is retained.
pub fn subscription_key(method: &str, param: Option<&Value>) -> Result<String, &'static str> {
    let mut fields = vec![("method".into(), JsJsonValue::String(method.into()))];
    if let Some(param) = param {
        fields.push(("param".into(), JsJsonValue::from(param.clone())));
    }
    js_json_stringify(&JsJsonValue::Object(fields))
        .map_err(|_| "subscription JSON serialization failed")?
        .ok_or("subscription JSON serialization returned undefined")
}
