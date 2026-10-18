//! Shared prepared-value operations used by venue bodies.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue as J, JsPrimitive, js_number_primitive};

pub(crate) fn get<'a>(value: &'a J, key: &str) -> &'a J {
    static MISSING: J = J::Undefined;
    match value {
        J::Object(fields) => fields
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map_or(&MISSING, |(_, value)| value),
        _ => &MISSING,
    }
}
pub(crate) fn property<'a>(value: &'a J, key: &str) -> Result<&'a J, ExchangeError> {
    if matches!(value, J::Undefined | J::Null) {
        let mut error = ExchangeError::new(format!(
            "Cannot read properties of {} (reading '{key}')",
            if matches!(value, J::Null) {
                "null"
            } else {
                "undefined"
            }
        ));
        error.name = "TypeError".into();
        return Err(error);
    }
    Ok(get(value, key))
}

pub(crate) fn v<'a>(
    value: &'a J,
    key: &str,
) -> Result<&'a J, safe_cex_exchange::runtime::ExchangeError> {
    if matches!(value, J::Undefined | J::Null) {
        return Ok(get(value, key));
    }
    if matches!(value, J::Array(_)) {
        return Ok(get(value, key));
    }
    if let J::Object(fields) = value {
        for candidate in [
            key.to_owned(),
            safe_cex_num::get_key::s(key),
            safe_cex_num::get_key::c(key),
        ] {
            if let Some((_, value)) = fields.iter().rev().find(|(name, _)| name == &candidate) {
                return Ok(value);
            }
        }
        return Ok(get(value, key));
    }
    let mut error = safe_cex_exchange::runtime::ExchangeError::new(format!(
        "Cannot use 'in' operator to search for '{key}' in {}",
        string(value).map_err(safe_cex_exchange::runtime::ExchangeError::new)?
    ));
    error.name = "TypeError".to_owned();
    Err(error)
}
pub(crate) fn truthy(value: &J) -> bool {
    match value {
        J::Undefined | J::Null => false,
        J::Bool(value) => *value,
        J::Number(value) => *value != 0.0 && !value.is_nan(),
        J::String(value) => !value.is_empty(),
        J::BigInt(value) => *value != 0.into(),
        J::Object(_) | J::Array(_) => true,
    }
}
pub(crate) fn number(value: &J) -> Result<f64, String> {
    let primitive = match value {
        J::Undefined => JsPrimitive::Undefined,
        J::Null => JsPrimitive::Null,
        J::Bool(value) => JsPrimitive::Boolean(*value),
        J::Number(value) => JsPrimitive::Number(*value),
        J::String(value) => JsPrimitive::String(value.clone()),
        J::BigInt(value) => JsPrimitive::BigInt(value.clone()),
        _ => JsPrimitive::String(string(value)?),
    };
    js_number_primitive(&primitive).map_err(|error| error.message.to_owned())
}
pub(crate) fn string(value: &J) -> Result<String, String> {
    crate::signer_utils::assertion_text(value)
}
pub(crate) fn array(value: &J) -> Result<&[J], String> {
    if let J::Array(values) = value {
        Ok(values)
    } else {
        Err("expected array".into())
    }
}
pub(crate) fn object(fields: impl IntoIterator<Item = (&'static str, J)>) -> J {
    J::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

// Conversion between the existing prepared-data carrier and shared JS heap.
// This retains undefined/BigInt/nonfinite values until their actual wire boundary.
pub(crate) fn js(value: &J) -> safe_cex_exchange::js_runtime::value::Js {
    use safe_cex_exchange::js_runtime::value::Js;
    use std::rc::Rc;
    match value {
        J::Undefined => Js::Undefined,
        J::Null => Js::Null,
        J::Bool(v) => Js::Bool(*v),
        J::Number(v) => Js::Number(*v),
        J::String(v) => Js::str(v),
        J::BigInt(v) => Js::BigInt(Rc::new(v.clone())),
        J::Array(v) => Js::array(v.iter().map(js).collect()),
        J::Object(v) => Js::object(v.iter().map(|(k, v)| (k.as_str(), js(v))).collect()),
    }
}
