//! Private timestamp decoder; socket members use the constructed class.
use safe_cex_num::{JsCoercionError, js_number};
use serde_json::Value;

// ts: WooPrivateWebscoket#parseTimestampToSeconds
pub fn parse_timestamp_to_seconds(raw: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    if raw.is_none_or(|value| value.is_null() || value.as_str() == Some("")) {
        return Ok(None);
    }
    let parsed = js_number(raw)?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Ok(None);
    }
    Ok(Some(if parsed >= 1_000_000_000_000.0 {
        (parsed / 1000.0).floor()
    } else {
        parsed.floor()
    }))
}
