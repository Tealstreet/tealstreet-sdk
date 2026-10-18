use safe_cex_exchange::js_runtime::value::{Js, Str, Thrown, from_json};
use safe_cex_num::JsCoercionError;
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
    let payload = payload.map(from_json).unwrap_or(Js::Undefined);
    let text = native_response_message(&payload).map_err(|error| {
        assert_eq!(error.message(), "Cannot convert object to primitive value");
        JsCoercionError {
            name: "TypeError",
            message: "Cannot convert object to primitive value",
        }
    })?;
    Ok(String::from_utf16(&text).expect("UTF8 input produces UTF8 response text"))
}

pub(crate) fn native_response_message(payload: &Js) -> Result<Str, Thrown> {
    if payload.type_of() != "object" || matches!(payload, Js::Null | Js::Array(_)) {
        return Ok("".encode_utf16().collect::<Vec<_>>().into());
    }
    let mut message = payload.get("msg")?;
    if !message.truthy() {
        message = payload.get("message")?;
        if !message.truthy() {
            message = payload.get("error")?;
        }
    }
    let code = payload.get("code")?;
    let mut text = Vec::new();
    if !code.is_undefined() {
        text.extend("code=".encode_utf16());
        text.extend_from_slice(&code.to_js_string()?);
    }
    if message.truthy() {
        let detail = message.to_js_string()?;
        if !detail.is_empty() {
            if !text.is_empty() {
                text.push(u16::from(b' '));
            }
            text.extend_from_slice(&detail);
        }
    }
    Ok(text.into())
}
