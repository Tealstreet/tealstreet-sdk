//! Source-bound numeric helpers for exchange snapshots; no lifecycle replica.
use safe_cex_num::{JsCoercionError, js_number, js_string, parse_float};
use serde_json::Value;

pub fn parse_finite_number(value: &Value) -> Result<f64, JsCoercionError> {
    let parsed = if value.is_number() {
        js_number(Some(value))?
    } else {
        // ts: parseFiniteNumber applies ?? before the fallible String conversion.
        let text = if value.is_null() {
            String::new()
        } else {
            js_string(Some(value))?
        };
        parse_float(&text)
    };
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

pub fn calculate_price_change_percentage(price: f64, previous: f64) -> f64 {
    if !price.is_finite() || !previous.is_finite() || previous <= 0.0 {
        0.0
    } else {
        ((price - previous) / previous) * 100.0
    }
}

pub fn is_finite_positive_number(value: &Value) -> Result<bool, JsCoercionError> {
    if !value.is_number() {
        return Ok(false);
    }
    let number = js_number(Some(value))?;
    Ok(number.is_finite() && number > 0.0)
}
