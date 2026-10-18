//! Stateless response projections used by the WOO exchange.
use crate::utils::{normalize_symbol, reverse_symbol};
use safe_cex_num::{JsCoercionError, js_number, js_trim};
use safe_cex_types::retained_json::RetainedJsonInput;
use serde_json::{Map, Value};

// ts: WOOXExchange#parseFiniteFundingNumber
pub fn parse_finite_funding_number(raw: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    match raw {
        Some(Value::Number(_)) => {}
        Some(Value::String(s)) if !js_trim(s).is_empty() => {}
        _ => return Ok(None),
    }
    Ok(Some(js_number(raw)?).filter(|v| v.is_finite()))
}
// ts: WOOXExchange#getWooResponseMessage
pub fn get_woo_response_message(response: &Map<String, Value>, fallback: &str) -> String {
    [
        response.get("message"),
        response.get("msg"),
        response.get("meta").and_then(|v| v.get("message")),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .find(|s| !js_trim(s).is_empty())
    .unwrap_or(fallback)
    .to_owned()
}
fn timestamp(raw: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    if matches!(raw, None | Some(Value::Null)) || raw.and_then(Value::as_str) == Some("") {
        return Ok(None);
    }
    Ok(Some(js_number(raw)?).filter(|v| v.is_finite() && *v >= 0.0))
}
// ts: WOOXExchange#normalizeTimestampSeconds
pub fn normalize_timestamp_seconds(raw: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    Ok(timestamp(raw)?.map(|v| {
        if v >= 1_000_000_000_000.0 {
            (v / 1000.0).floor()
        } else {
            v.floor()
        }
    }))
}
// ts: WOOXExchange#normalizeTimestampMilliseconds
pub fn normalize_timestamp_milliseconds(
    raw: Option<&Value>,
) -> Result<Option<f64>, JsCoercionError> {
    Ok(timestamp(raw)?.map(|v| {
        if v >= 1_000_000_000_000.0 {
            v.floor()
        } else {
            (v * 1000.0).floor()
        }
    }))
}
// ts: WOOXExchange#normalizeTransactionSymbol
pub fn normalize_transaction_symbol(raw: Option<&Value>) -> Option<String> {
    raw.and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| normalize_symbol(&s.replace('/', "_")))
}
// ts: WOOXExchange#normalizeRequestedSymbol
pub fn normalize_requested_symbol(raw: Option<&str>) -> Option<String> {
    normalize_requested_symbol_with(raw, |stripped| {
        Ok::<_, std::convert::Infallible>(normalize_transaction_symbol(Some(&Value::String(
            stripped.to_owned(),
        ))))
    })
    .unwrap()
    .flatten()
}

pub fn normalize_requested_symbol_with<T, E>(
    raw: Option<&str>,
    mut normalize_transaction: impl FnMut(&str) -> Result<T, E>,
) -> Result<Option<T>, E> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let upper = raw.to_uppercase();
    let mut stripped = upper
        .rsplit(':')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(&upper);
    for suffix in [".P", "_PERP", "-PERP", "PERP"] {
        stripped = stripped.strip_suffix(suffix).unwrap_or(stripped);
    }
    normalize_transaction(stripped).map(Some)
}
// ts: WOOXExchange#getTransactionHistorySymbolCandidates
pub fn get_transaction_history_symbol_candidates(symbol: &str) -> Vec<String> {
    let upper = symbol.to_uppercase();
    let base = upper.strip_suffix("USDT").unwrap_or(&upper);
    let mut result = Vec::new();
    for candidate in [
        reverse_symbol(&upper),
        format!("{base}_USDT_PERP"),
        format!("{base}-USDT-PERP"),
        upper,
    ] {
        if !candidate.is_empty() && !result.contains(&candidate) {
            result.push(candidate);
        }
    }
    result
}

// ts: WOOXExchange#parseTransactionRowsFromResponse
pub fn parse_transaction_rows_from_response(data: &RetainedJsonInput) -> &[RetainedJsonInput] {
    let nested = data.get("data");
    nested
        .and_then(|v| v.get("rows"))
        .and_then(RetainedJsonInput::as_array)
        .or_else(|| data.get("rows").and_then(RetainedJsonInput::as_array))
        .or_else(|| nested.and_then(RetainedJsonInput::as_array))
        .or_else(|| data.as_array())
        .unwrap_or(&[])
}
