//! Bitunix wire-value helpers. Capture incoming JSON through shared numeric
//! ingress's retained passthrough before constructing these Value arguments.
use safe_cex_num::{JsCoercionError, js_number, js_trim};
use serde_json::Value;

fn absent(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Null))
        || matches!(value, Some(Value::String(text)) if text.is_empty())
}

/// The caller supplies the TS default fallback (0) or its explicit value.
pub fn parse_finite_number(value: Option<&Value>, fallback: f64) -> Result<f64, JsCoercionError> {
    if absent(value) {
        return Ok(fallback);
    }
    let parsed = js_number(value)?;
    Ok(if parsed.is_finite() { parsed } else { fallback })
}

pub fn is_api_trading_supported(value: Option<&Value>) -> bool {
    if absent(value) {
        return true;
    }
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(_)) => js_number(value).expect("Number coercion cannot throw") != 0.0,
        Some(Value::String(value)) => {
            let normalized = js_trim(value).to_lowercase();
            !["false", "0", "no", "n", "disabled", "unsupported"].contains(&normalized.as_str())
        }
        _ => true,
    }
}

pub fn has_finite_trigger_price(value: Option<&Value>) -> Result<bool, JsCoercionError> {
    if absent(value) {
        return Ok(false);
    }
    Ok(js_number(value)?.is_finite())
}
