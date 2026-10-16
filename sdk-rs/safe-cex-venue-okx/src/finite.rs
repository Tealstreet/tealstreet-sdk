use safe_cex_num::{JsCoercionError, js_string, parse_float};
use serde_json::Value;

pub fn parse_finite(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = match value.filter(|value| !value.is_null()) {
        Some(value) => js_string(Some(value))?,
        None => String::new(),
    };
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

pub fn sum_finite(values: &[Value]) -> Result<f64, JsCoercionError> {
    values
        .iter()
        .try_fold(0.0, |sum, value| Ok(sum + parse_finite(Some(value))?))
}
