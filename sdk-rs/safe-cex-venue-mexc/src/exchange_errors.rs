//! Venue rejection decoding, including filter-before-format error precedence.
use crate::{
    exchange_helpers::extract_rows,
    utils::{to_integer_string, to_number},
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, js_string, js_truthy};
use serde_json::Value;
pub(crate) fn coercion(error: JsCoercionError) -> ExchangeError {
    let mut result = ExchangeError::new(error.message);
    result.name = error.name.into();
    result
}
// ts: MexcExchange.assertCommonSuccess
pub fn assert_common_success(payload: Option<&Value>, action: &str) -> Result<(), ExchangeError> {
    let code = payload
        .and_then(|v| v.get("code"))
        .filter(|v| !v.is_null())
        .map(|v| to_number(Some(v), Some(f64::NAN)))
        .transpose()
        .map_err(coercion)?;
    if js_truthy(payload)
        && payload.and_then(|v| v.get("success")) != Some(&Value::Bool(false))
        && code == Some(0.0)
    {
        return Ok(());
    }
    let fallback = Value::String("unknown error".into());
    let detail = ["message", "msg", "code"]
        .iter()
        .filter_map(|key| payload.and_then(|v| v.get(*key)))
        .find(|v| js_truthy(Some(v)))
        .unwrap_or(&fallback);
    Err(ExchangeError::new(format!(
        "{action} failed: {}",
        js_string(Some(detail)).map_err(coercion)?
    )))
}
// ts: MexcExchange.assertRegularCancelSucceeded
pub fn assert_regular_cancel_succeeded(payload: Option<&Value>) -> Result<(), ExchangeError> {
    assert_common_success(payload, "MEXC order cancellation")?;
    let mut failed = Vec::new();
    // Filtering completes before orderId/message coercion: a later null row
    // throws before an earlier failed row's unstringifiable order ID.
    for row in extract_rows(payload, &[]) {
        if row.is_null() {
            let mut e = ExchangeError::new("Cannot read properties of null (reading 'errorCode')");
            e.name = "TypeError".into();
            return Err(e);
        }
        if to_number(row.get("errorCode"), Some(0.0)).map_err(coercion)? != 0.0 {
            failed.push(row)
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    let mut messages = Vec::new();
    for row in failed {
        let id = to_integer_string(row.get("orderId")).map_err(coercion)?;
        let id = if id.is_empty() { "unknown" } else { &id };
        let detail = ["errorMsg", "message", "msg"]
            .iter()
            .filter_map(|key| row.get(*key))
            .find(|v| js_truthy(Some(v)))
            .or_else(|| row.get("errorCode"));
        messages.push(format!("{id}: {}", js_string(detail).map_err(coercion)?));
    }
    Err(ExchangeError::new(format!(
        "MEXC order cancellation failed: {}",
        messages.join(", ")
    )))
}
