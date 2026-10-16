//! Numeric helpers from the historical-order and income readers.
//! Inputs are retained JSON plus undefined, captured before constructing Value.
use safe_cex_num::{JsCoercionError, js_string, parse_float};
use serde_json::Value;

/// bitunix.order-history.ts::toOptionalNumber; zero means absent, including -0.
pub fn order_to_optional_number(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    let text = match value {
        None | Some(Value::Null) => String::new(),
        _ => js_string(value)?,
    };
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() && parsed != 0.0 {
        Some(parsed)
    } else {
        None
    })
}

/// bitunix.order-history.ts::toNumber; finite signed zero is retained.
pub fn order_to_number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = match value {
        None | Some(Value::Null) => String::new(),
        _ => js_string(value)?,
    };
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

/// bitunix.income.ts::toNumber; kept separate to track its source declaration.
pub fn income_to_number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = match value {
        None | Some(Value::Null) => String::new(),
        _ => js_string(value)?,
    };
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}
