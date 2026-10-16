use safe_cex_num::{JsCoercionError, js_string, js_truthy};
use serde_json::Value;

// ts: getBinanceResponseMessage
pub fn get_binance_response_message(payload: Option<&Value>) -> Result<String, JsCoercionError> {
    response_message(payload)
}

// ts: getBinancePMResponseMessage
pub fn get_binance_pm_response_message(payload: Option<&Value>) -> Result<String, JsCoercionError> {
    response_message(payload)
}

fn response_message(payload: Option<&Value>) -> Result<String, JsCoercionError> {
    let Some(Value::Object(record)) = payload else {
        return Ok(String::new());
    };
    let message = record
        .get("msg")
        .filter(|v| js_truthy(Some(v)))
        .or_else(|| record.get("message").filter(|v| js_truthy(Some(v))))
        .or_else(|| record.get("error"));
    let mut parts = Vec::new();
    if let Some(code) = record.get("code") {
        parts.push(format!("code={}", js_string(Some(code))?));
    }
    if js_truthy(message) {
        let text = js_string(message)?;
        if !text.is_empty() {
            parts.push(text);
        }
    }
    Ok(parts.join(" "))
}
