use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use serde_json::Value;

// ts: getProvider
// ts-bug: getProvider's assertion promises string|undefined but forwards the raw
// extra field, including null/non-strings. Validation happens at the caller.
pub fn get_provider(extra: Option<&Value>) -> Option<&Value> {
    extra.and_then(|extra| extra.get("lighterSignerProvider"))
}

fn string_value(value: Js) -> Result<Js, Thrown> {
    Ok(if matches!(value, Js::SymbolIterator) {
        Js::str("Symbol(Symbol.iterator)")
    } else {
        Js::String(value.to_js_string()?)
    })
}

// ts: isLighterInvalidNonceError
pub fn is_lighter_invalid_nonce_error(error: Option<&Js>) -> Result<bool, Thrown> {
    let error = error.unwrap_or(&Js::Undefined);
    let code = if error.is_nullish() {
        Js::Undefined
    } else {
        error.get("code")?
    };
    let code = string_value(if code.is_nullish() { Js::str("") } else { code })?;
    let message = if error.is_nullish() {
        Js::Undefined
    } else {
        error.get("message")?
    };
    let message = if message.is_nullish() {
        error.clone()
    } else {
        message
    };
    let message = string_value(if message.is_nullish() {
        Js::str("")
    } else {
        message
    })?
    .call_method("toLowerCase", vec![])?;
    Ok(code.strict_equals(&Js::str("21104"))
        || message
            .call_method("includes", vec![Js::str("invalid nonce")])?
            .truthy())
}
