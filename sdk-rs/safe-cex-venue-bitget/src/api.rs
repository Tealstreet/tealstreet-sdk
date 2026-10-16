//! Pure REST error predicates; request signing and client lifecycle follow later.
use crate::protocol::ENDPOINTS;
use serde_json::Value;

// ts: SILENT_ERROR_CODES
const SILENT_ERROR_CODES: &[&str] = &["40084", "40085"];

// ts: isBitgetUtaStrategyOrderNotFound
pub fn is_bitget_uta_strategy_order_not_found(message: Option<&Value>) -> bool {
    message.and_then(Value::as_str).is_some_and(|message| {
        message
            .to_lowercase()
            .contains("corresponding strategy id could not be found")
    })
}

// ts: shouldSilenceBitgetResponseError
pub fn should_silence_bitget_response_error(
    code: &str,
    message: Option<&Value>,
    url: Option<&str>,
) -> bool {
    SILENT_ERROR_CODES.contains(&code)
        || message
            .and_then(Value::as_str)
            .is_some_and(|message| message.to_lowercase().contains("environment is incorrect"))
        || (url.is_some_and(|url| {
            url.contains(
                ENDPOINTS
                    .iter()
                    .find(|entry| entry.name == "UTA_CANCEL_STRATEGY_ORDER")
                    .expect("protocol table contains UTA strategy cancel")
                    .path,
            )
        }) && is_bitget_uta_strategy_order_not_found(message))
}

// ts: shouldSuppressValidationErrorEmission
pub fn should_suppress_validation_error_emission(exchange: &Value) -> bool {
    exchange.get("suppressBitgetValidationErrorEmission") == Some(&Value::Bool(true))
}

// ts: toJsonString
pub fn to_json_string(
    value: &safe_cex_num::JsJsonValue,
) -> Result<Option<String>, safe_cex_num::JsCoercionError> {
    use safe_cex_num::{JsJsonValue, js_json_stringify, js_own_keys, js_replace_all};
    if matches!(value, JsJsonValue::Null | JsJsonValue::Undefined) {
        return Ok(None);
    }
    let Some(mut json) = js_json_stringify(value)? else {
        return Ok(None);
    };
    if let JsJsonValue::Object(entries) = value {
        // Assignment entries can repeat; Object.keys visits each own key once.
        let mut seen = std::collections::HashSet::new();
        let keys = js_own_keys(entries.iter().map(|(key, _)| key.as_str()));
        for key in keys.into_iter().filter(|key| seen.insert(*key)) {
            if let Some(replacement) = key.strip_prefix('_') {
                // ts-bug: toJsonString rewrites serialized VALUES as well as keys,
                // and replacement strings expand $&, $$, $` and $' tokens.
                json = safe_cex_num::js_replace(&json, key, replacement);
            }
        }
    }
    // Arrays and primitive wrappers have no underscore-prefixed own keys.
    js_replace_all(&json, "\"_", "\"").map(Some)
}
