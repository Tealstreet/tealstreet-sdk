use safe_cex_num::{JsCoercionError, js_string, parse_float};
use serde_json::Value;

// ts: toOptionalNumber (binance.order-history.ts)
pub fn to_optional_number(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    let text = js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let parsed = parse_float(&text);
    Ok((parsed.is_finite() && parsed != 0.0).then_some(parsed))
}

// ts: toNumber (binance.order-history.ts)
pub fn to_number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

/// Canonical source mapping retains native fields and the originating realm.
pub fn map_binance_historical_order(
    row: &crate::private_event_value::EventValue,
    realm: &std::rc::Rc<safe_cex_num::PrototypeRealm>,
) -> Result<Option<crate::private_event_value::EventValue>, crate::request_weight::Failure> {
    crate::order_history_native::map_binance_historical_order_native(row, realm)
}
