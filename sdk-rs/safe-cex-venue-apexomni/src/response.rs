//! Retained REST model reads. Strict named strings cross UTF-8 only when consumed.
use crate::exchange::failure;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
pub(crate) fn get<'a>(value: &'a Input, key: &str) -> &'a Input {
    value.get(key).unwrap_or(&Input::Undefined)
}
pub(crate) fn property<'a>(value: &'a Input, key: &str) -> Result<&'a Input, ExchangeError> {
    if value.is_nullish() {
        let mut error = failure(format!(
            "Cannot read properties of {} (reading '{key}')",
            if value.is_null() { "null" } else { "undefined" }
        ));
        error.name = "TypeError".into();
        return Err(error);
    }
    Ok(get(value, key))
}
pub(crate) fn v<'a>(value: &'a Input, key: &str) -> Result<&'a Input, ExchangeError> {
    if value.is_nullish() || value.is_array() {
        return Ok(get(value, key));
    }
    if value.is_object() {
        for candidate in [
            key.to_owned(),
            safe_cex_num::get_key::s(key),
            safe_cex_num::get_key::c(key),
        ] {
            if let Some(value) = value.get(&candidate) {
                return Ok(value);
            }
        }
        return Ok(get(value, key));
    }
    let mut error = failure(format!(
        "Cannot use 'in' operator to search for '{key}' in {}",
        text(value)?
    ));
    error.name = "TypeError".into();
    Err(error)
}
pub(crate) fn truthy(value: &Input) -> bool {
    value.is_truthy()
}
pub(crate) fn numeric(value: &Input) -> Result<f64, ExchangeError> {
    safe_cex_num::js_retained_number(Some(value)).map_err(|e| {
        let mut out = failure(e.message.into());
        out.name = e.name.into();
        out
    })
}
pub(crate) fn float(value: &Input) -> Result<f64, ExchangeError> {
    safe_cex_num::js_retained_parse_float(Some(value)).map_err(|e| {
        let mut out = failure(e.message.into());
        out.name = e.name.into();
        out
    })
}
pub(crate) fn text(value: &Input) -> Result<String, ExchangeError> {
    safe_cex_num::js_retained_string(Some(value)).map_err(|e| {
        let mut out = failure(e.message.into());
        out.name = e.name.into();
        out
    })
}
pub(crate) fn field_string(value: &Input) -> Result<String, ExchangeError> {
    value
        .as_text()
        .ok_or_else(|| failure("shared typed record refuses a non-string field".into()))
        .and_then(|units| {
            String::from_utf16(units).map_err(|_| {
                failure("shared typed record refuses a lone-surrogate string field".into())
            })
        })
}
pub(crate) fn array(value: &Input) -> Result<&[Input], String> {
    value.as_array().ok_or_else(|| "expected array".into())
}
pub(crate) fn or_zero(value: &Input) -> Input {
    if value.is_truthy() {
        value.clone()
    } else {
        Input::Float(0.)
    }
}
pub(crate) fn nullish_zero(value: &Input) -> Input {
    if value.is_nullish() {
        Input::Float(0.)
    } else {
        value.clone()
    }
}

/// The warning is an explicit text boundary; preserve native kind/bits in Debug
/// and render readable UTF-16 keys/text with the one canonical JSON writer.
pub(crate) fn diagnostic(value: &Input) -> Result<String, ExchangeError> {
    Ok(format!(
        "{value:?}; JSON={}",
        value
            .json_stringify()
            .map_err(ExchangeError::from)?
            .unwrap_or_else(|| "undefined".into())
    ))
}
