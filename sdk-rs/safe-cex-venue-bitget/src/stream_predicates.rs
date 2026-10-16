//! Pure private-stream field normalization; no socket or lifecycle ownership.
use safe_cex_num::{JsCoercionError, js_string};
use serde_json::Value;

// ts: BitgetPrivateWebsocket.normalizeOrderStatus
pub fn normalize_order_status(value: Option<&Value>) -> Result<String, JsCoercionError> {
    let empty = Value::String(String::new());
    Ok(
        js_string(Some(value.filter(|v| !v.is_null()).unwrap_or(&empty)))?
            .to_lowercase()
            .replace('-', "_"),
    )
}

// ts: BitgetPrivateWebsocket.normalizeChannel
pub fn normalize_channel(value: Option<&Value>) -> Result<String, JsCoercionError> {
    let empty = Value::String(String::new());
    Ok(
        js_string(Some(value.filter(|v| !v.is_null()).unwrap_or(&empty)))?
            .to_lowercase()
            .replace(['-', '_'], ""),
    )
}

// ts: BitgetPrivateWebsocket.getRawOrderStatus
pub fn get_raw_order_status(order: Option<&Value>) -> Result<String, JsCoercionError> {
    let value = order.filter(|v| !v.is_null()).ok_or(JsCoercionError {
        name: "TypeError",
        message: if order.is_none() {
            "Cannot read properties of undefined (reading 'status')"
        } else {
            "Cannot read properties of null (reading 'status')"
        },
    })?;
    let raw = ["status", "state", "planStatus", "orderStatus"]
        .into_iter()
        .find_map(|key| value.get(key).filter(|v| !v.is_null()));
    normalize_order_status(raw)
}

// ts: BitgetPrivateWebsocket.isTerminalFillStatus
pub fn is_terminal_fill_status(normalized_status: &str) -> bool {
    matches!(normalized_status, "triggered" | "filled" | "full_fill")
}
