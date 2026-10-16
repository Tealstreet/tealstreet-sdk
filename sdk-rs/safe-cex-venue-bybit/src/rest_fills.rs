#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Historical fill orchestration awaiting class wiring"
    )
)]
use crate::{api::truthy, primary_fills::append_primary_fills};
use safe_cex_num::{js_number_string, js_string};
use safe_cex_types::{Execution, ExecutionsOptions};
use serde_json::{Value, json};
use std::future::Future;
const RANGE: f64 = 604_799_999.;
const LOOKBACK: f64 = 604_800_000.;

pub(crate) fn query_windows(from: Option<f64>, to: Option<f64>) -> Result<Vec<Value>, String> {
    let window = |from: Option<f64>, to: Option<f64>| {
        let mut m = serde_json::Map::new();
        if let Some(v) = from {
            m.insert("startTime".into(), json!(v));
        }
        if let Some(v) = to {
            m.insert("endTime".into(), json!(v));
        }
        Value::Object(m)
    };
    if from.into_iter().chain(to).any(|n| !n.is_finite()) {
        return Err("Bybit query bounds must be finite".into());
    }
    let (Some(from), Some(to)) = (from, to) else {
        return Ok(vec![window(from, to)]);
    };
    if to - from <= RANGE {
        return Ok(vec![window(Some(from), Some(to))]);
    }
    let mut windows = vec![];
    let mut end = to;
    while end > from {
        let start = from.max(end - RANGE);
        if start >= end {
            return Err("Bybit query window cannot advance at this numeric magnitude".into());
        }
        windows.push(window(Some(start), Some(end)));
        if start <= from {
            break;
        }
        end = start - 1.;
    }
    Ok(windows)
}

pub(crate) async fn fetch_historical_fills<G, GF, F, FF>(
    opts: &ExecutionsOptions,
    public_only: bool,
    category: &str,
    name: &str,
    mut get: G,
    mut fallback: F,
    mut diagnostic: impl FnMut(&str, &str),
) -> Result<Vec<Execution>, String>
where
    G: FnMut(Value) -> GF,
    GF: Future<Output = Result<Value, String>>,
    F: FnMut(Option<f64>) -> FF,
    FF: Future<Output = Result<Vec<Execution>, String>>,
{
    if public_only {
        return Ok(vec![]);
    }
    let Some(symbol) = opts.symbol.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(vec![]);
    };
    let limit = opts.limit.unwrap_or(50.);
    let effective_limit = if limit == 0. || limit.is_nan() {
        50.
    } else {
        limit
    };
    let windows = query_windows(opts.from, opts.to)?;
    let mut executions = vec![];
    let mut primary_rows = 0;
    for window in &windows {
        if executions.len() as f64 >= effective_limit {
            break;
        }
        let mut cursor = None;
        for depth in 0..=5 {
            if depth == 5 {
                diagnostic(
                    "warning",
                    "fetchHistoricalFills reached max recursion depth of 5",
                );
                break;
            }
            let mut params =
                json!({"category":category,"symbol":symbol,"limit":effective_limit.min(100.)});
            if let Some(order_id) = opts.order_id.as_deref().filter(|s| !s.is_empty()) {
                params["orderId"] = order_id.into();
            }
            if let Some(map) = window.as_object() {
                for (k, v) in map {
                    params[k] = v.clone();
                }
            }
            if let Some(c) = cursor.take() {
                params["cursor"] = c;
            }
            let result = match get(params).await {
                Ok(data) => data,
                Err(error) => {
                    diagnostic("warning", &format!("Error fetching executions: {error}"));
                    break;
                }
            };
            let mapped = (|| -> Result<Option<Value>, String> {
                if !result.is_null() && !result.is_object() && !result.is_array() {
                    let text = js_string(Some(&result)).map_err(|e| e.message.to_owned())?;
                    return Err(format!(
                        "Cannot use 'in' operator to search for 'retMsg' in {text}"
                    ));
                }
                let msg = crate::json_field::field(&result, "retMsg")?.filter(|v| truthy(v));
                let msg = match msg {
                    Some(v) => js_string(Some(v)).map_err(|e| e.message.to_owned())?,
                    None => "UNKNOWN".into(),
                };
                if msg != "OK" {
                    diagnostic("warning", &format!("Failed to fetch executions: {msg}"));
                    return Ok(None);
                }
                let rows = result
                    .get("result")
                    .and_then(|r| r.get("list"))
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                primary_rows += rows.len();
                append_primary_fills(rows, symbol, &mut executions)?;
                Ok(result
                    .get("result")
                    .and_then(|r| r.get("nextPageCursor"))
                    .filter(|v| truthy(v))
                    .cloned())
            })();
            match mapped {
                Err(e) => {
                    diagnostic("warning", &format!("Error fetching executions: {e}"));
                    break;
                }
                Ok(next) => {
                    if executions.len() as f64 >= effective_limit || next.is_none() {
                        break;
                    }
                    cursor = next;
                }
            }
        }
    }
    if !executions.is_empty() {
        return Ok(executions);
    }
    let warning = format!(
        "fetchHistoricalFills primary path produced no valid fills for {symbol}; falling back to /v5/order/history (rawRows={primary_rows}, windows={}, limit={}, hasOrderId={})",
        windows.len(),
        js_number_string(limit),
        opts.order_id.as_ref().is_some_and(|s| !s.is_empty())
    );
    diagnostic("warning", &warning);
    diagnostic("console", &format!("[{name}] {warning}"));
    let result = async {
        let initial = fallback(None).await?;
        // Inverse has a single 30-day fallback. Only linear has the short-then-
        // extended recovery branch; even its empty-result diagnostics differ.
        if !initial.is_empty()
            || (category != "inverse"
                && (opts.from.is_some()
                    || opts.order_id.as_ref().is_some_and(|s| !s.is_empty())))
        {
            return Ok(initial);
        }
        let extended = 30. * 24. * 60. * 60. * 1000.;
        if category != "inverse" && opts.to.is_none() && LOOKBACK < extended {
            let warning = format!("fetchHistoricalFills short fallback returned no rows for {symbol}; retrying 30d order-history lookup for recovery");
            diagnostic("warning", &warning);
            diagnostic("console", &format!("[{name}] {warning}"));
            return fallback(Some(extended)).await;
        }
        diagnostic("warning", &format!("fetchHistoricalFills fallback returned no rows for {symbol}"));
        Ok(initial)
    }.await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            diagnostic(
                "warning",
                &format!("Failed order-history fallback fetch executions: {e}"),
            );
            Ok(executions)
        }
    }
}
#[cfg(test)]
mod tests;
