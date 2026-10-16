use safe_cex_num::{JsCoercionError, js_string};
use serde_json::Value;

// ts: getProvider
// ts-bug: getProvider's assertion promises string|undefined but forwards the raw
// extra field, including null/non-strings. Validation happens at the caller.
pub fn get_provider(extra: Option<&Value>) -> Option<&Value> {
    extra.and_then(|extra| extra.get("lighterSignerProvider"))
}

// ts: isLighterInvalidNonceError
pub fn is_lighter_invalid_nonce_error(error: Option<&Value>) -> Result<bool, JsCoercionError> {
    let code = error
        .and_then(|value| value.get("code"))
        .filter(|value| !value.is_null());
    let code = code
        .map(|value| js_string(Some(value)))
        .transpose()?
        .unwrap_or_default();
    let message = error
        .and_then(|value| value.get("message"))
        .filter(|value| !value.is_null())
        .or_else(|| error.filter(|value| !value.is_null()));
    let message = message
        .map(|value| js_string(Some(value)))
        .transpose()?
        .unwrap_or_default()
        .to_lowercase();
    // TS coerces BOTH fields before its final OR, even when code already matches.
    Ok(code == "21104" || message.contains("invalid nonce"))
}
