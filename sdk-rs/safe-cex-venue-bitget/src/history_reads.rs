//! History page bodies, with the existing parsed REST read supplied by the caller.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, js_string, js_trim, js_truthy};
use serde_json::Value;
use std::future::Future;

// ts: BitgetExchange._fetchFillsPage
// The read operation returns xhr.get's already-decoded response.data. Its error
// type is retained, rather than flattened into a string or converted to an empty page.
pub async fn fetch_fills_page<E, F>(
    params: Value,
    read: impl FnOnce(&'static str, Value) -> F,
) -> Result<(Vec<Value>, Option<String>), E>
where
    E: From<JsCoercionError> + From<ExchangeError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let url = crate::protocol::ENDPOINTS
        .iter()
        .find(|entry| entry.name == "ORDER_FILLS")
        .ok_or_else(|| ExchangeError::new("Bitget protocol table is missing ORDER_FILLS"))?
        .path;
    let data = read(url, params).await?;
    let code = data.as_ref().and_then(|data| data.get("code"));
    if js_truthy(code) && js_string(code)? != "00000" {
        let empty = Value::String(String::new());
        let message = data
            .as_ref()
            .and_then(|data| data.get("msg"))
            .filter(|value| !value.is_null())
            .unwrap_or(&empty);
        let message = format!("{} {}", js_string(code)?, js_string(Some(message))?);
        return Err(ExchangeError::new(js_trim(&message)).into());
    }
    let payload = data.as_ref().and_then(|data| data.get("data"));
    let rows = payload
        .and_then(|data| data.get("fillList"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let end_id = payload.and_then(|data| data.get("endId"));
    let end_id = if js_truthy(end_id) {
        Some(js_string(end_id)?)
    } else {
        None
    };
    Ok((rows, end_id))
}

// ts: BitgetExchange._fetchBillPage
// The resolver projects _resolveMarket(symbol)?.id; None is undefined. The read
// consumes native JS-shaped params and returns xhr.get's parsed response.data.
pub async fn fetch_bill_page<E, F>(
    public_data_only: bool,
    product_type: &str,
    opts: Option<&Value>,
    resolve_market_id: impl FnOnce(&Value) -> Result<Option<Value>, E>,
    read: impl FnOnce(&'static str, safe_cex_num::JsJsonValue) -> F,
) -> Result<Vec<safe_cex_types::IncomeRecord>, E>
where
    E: From<JsCoercionError> + From<ExchangeError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    use safe_cex_num::{JsJsonValue, js_number, math_min};
    if public_data_only {
        return Ok(Vec::new());
    }
    let default_limit = Value::from(100);
    let limit = opts
        .and_then(|opts| opts.get("limit"))
        .filter(|value| js_truthy(Some(value)))
        .unwrap_or(&default_limit);
    let mut params = vec![
        (
            "productType".into(),
            JsJsonValue::String(product_type.into()),
        ),
        (
            "limit".into(),
            JsJsonValue::Number(math_min(js_number(Some(limit))?, 100.0)),
        ),
    ];
    if let Some(symbol) = opts
        .and_then(|opts| opts.get("symbol"))
        .filter(|value| js_truthy(Some(value)))
    {
        let id = resolve_market_id(symbol)?;
        let id = id
            .as_ref()
            .filter(|value| !value.is_null())
            .unwrap_or(symbol);
        params.push(("symbol".into(), JsJsonValue::from(id.clone())));
    }
    for field in ["startTime", "endTime"] {
        if let Some(value) = opts
            .and_then(|opts| opts.get(field))
            .filter(|value| js_truthy(Some(value)))
        {
            params.push((field.into(), JsJsonValue::from(value.clone())));
        }
    }
    let url = crate::protocol::ENDPOINTS
        .iter()
        .find(|entry| entry.name == "ACCOUNT_BILL")
        .ok_or_else(|| ExchangeError::new("Bitget protocol table is missing ACCOUNT_BILL"))?
        .path;
    let data = read(url, JsJsonValue::Object(params)).await?;
    let code = data.as_ref().and_then(|data| data.get("code"));
    if js_truthy(code) && js_string(code)? != "00000" {
        let empty = Value::String(String::new());
        let message = data
            .as_ref()
            .and_then(|data| data.get("msg"))
            .filter(|value| !value.is_null())
            .unwrap_or(&empty);
        let message = format!("{} {}", js_string(code)?, js_string(Some(message))?);
        return Err(ExchangeError::new(js_trim(&message)).into());
    }
    let rows = data
        .as_ref()
        .and_then(|data| data.get("data"))
        .and_then(|data| data.get("bills"))
        .and_then(Value::as_array);
    Ok(crate::income::map_bitget_bills(
        rows.map_or(&[], Vec::as_slice),
    )?)
}

// ts: BitgetExchange.fetchIncomeHistory
// The supplied page owns the existing _fetchBillPage operation. error_value
// projects its runtime failure to the thrown JS value (None means undefined).
// Reporting is fallible: a failure inside the source catch escapes that catch.
pub async fn fetch_income_history<O, E, F>(
    opts: O,
    read: impl FnOnce(O) -> F,
    error_value: impl FnOnce(&E) -> Option<Value>,
    emit_and_log_error: impl FnOnce(String) -> Result<(), E>,
) -> Result<Vec<safe_cex_types::IncomeRecord>, E>
where
    E: From<JsCoercionError>,
    F: Future<Output = Result<Vec<safe_cex_types::IncomeRecord>, E>>,
{
    match read(opts).await {
        Ok(rows) => Ok(rows),
        Err(error) => {
            let error = error_value(&error);
            let message = error
                .as_ref()
                .and_then(|error| error.get("message"))
                .filter(|message| !message.is_null())
                .or(error.as_ref());
            let message = format!("Error fetching income history: {}", js_string(message)?);
            emit_and_log_error(message)?;
            Ok(Vec::new())
        }
    }
}

// ts: BitgetExchange.fetchHistoricalFills
// Detection includes the post-await usesUtaRouting getter. Resolver results
// project the existing market operation's id and symbol, without a local store.
// UTA routing delegates to the supplied existing _fetchHistoricalUtaFills operation.
pub async fn fetch_historical_fills<'a, D, U, R>(
    public_data_only: bool,
    opts: &'a safe_cex_types::ExecutionsOptions,
    detect_then_uses_uta: impl FnOnce() -> D,
    uta_fills: impl FnOnce(&'a safe_cex_types::ExecutionsOptions) -> U,
    mut resolve_market: impl FnMut(&str) -> Result<Option<(String, String)>, ExchangeError>,
    mut product_type: impl FnMut() -> Result<String, ExchangeError>,
    mut read_page: impl FnMut(safe_cex_num::JsJsonValue) -> R,
    (mut error_value, mut report): (
        impl FnMut(&ExchangeError) -> Option<Value>,
        impl FnMut(Option<Value>) -> Result<(), ExchangeError>,
    ),
) -> Result<Vec<safe_cex_types::Execution>, ExchangeError>
where
    D: Future<Output = Result<bool, ExchangeError>>,
    U: Future<Output = Result<Vec<safe_cex_types::Execution>, ExchangeError>>,
    R: Future<Output = Result<(Vec<Value>, Option<String>), ExchangeError>>,
{
    use safe_cex_num::{JsJsonValue, js_number_string, math_min};
    if public_data_only {
        return Ok(Vec::new());
    }
    if detect_then_uses_uta().await? {
        return uta_fills(opts).await;
    }
    let venue_symbol = if let Some(symbol) = opts.symbol.as_deref().filter(|s| !s.is_empty()) {
        let Some((id, _)) = resolve_market(symbol)? else {
            return Ok(Vec::new());
        };
        Some(id)
    } else {
        None
    };
    let windows = match (opts.from, opts.to) {
        (Some(from), Some(to)) => safe_cex_candle_cache::calendar::split_time_range(
            from,
            to,
            89.0 * 24.0 * 60.0 * 60.0 * 1000.0,
        )
        .into_iter()
        .map(Some)
        .collect(),
        _ => vec![None],
    };
    let mut executions = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let outcome: Result<(), ExchangeError> = async {
        for window in windows {
            let mut cursor: Option<String> = None;
            for _ in 0..100 {
                let mut params = vec![
                    ("productType".into(), JsJsonValue::String(product_type()?)),
                    (
                        "limit".into(),
                        JsJsonValue::Number(math_min(opts.limit.unwrap_or(100.0), 100.0)),
                    ),
                ];
                if let Some(symbol) = venue_symbol.as_ref().filter(|s| !s.is_empty()) {
                    params.push(("symbol".into(), JsJsonValue::String(symbol.clone())));
                }
                if let Some(window) = &window {
                    for field in ["startTime", "endTime"] {
                        params.push((field.into(), JsJsonValue::from(window[field].clone())));
                    }
                }
                if let Some(cursor) = cursor.as_ref().filter(|s| !s.is_empty()) {
                    params.push(("idLessThan".into(), JsJsonValue::String(cursor.clone())));
                }
                let (rows, end_id) = read_page(JsJsonValue::Object(params)).await?;
                if rows.is_empty() {
                    break;
                }
                let mapped = crate::fills::map_bitget_fills(
                    &rows,
                    &mut crate::fills::BitgetFillDeps {
                        resolve_symbol: &mut |symbol| {
                            Ok(resolve_market(symbol)?.map(|(_, symbol)| symbol))
                        },
                        to_timestamp: &mut |ms| Ok((ms / 1000.0).floor()),
                    },
                )?;
                for execution in mapped {
                    let id = execution.id.clone().unwrap_or_else(|| {
                        format!(
                            "{}|{}|{}|{}",
                            execution.symbol,
                            js_number_string(execution.timestamp),
                            js_number_string(execution.qty),
                            js_number_string(execution.price)
                        )
                    });
                    if seen.insert(id) {
                        executions.push(execution);
                    }
                }
                if end_id.as_ref().is_none_or(String::is_empty) || end_id == cursor {
                    break;
                }
                cursor = end_id;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = outcome {
        let error = error_value(&error);
        let message = error
            .as_ref()
            .and_then(|error| error.get("response"))
            .and_then(|response| response.get("data"))
            .and_then(|data| data.get("msg"))
            .filter(|message| js_truthy(Some(message)))
            .or_else(|| error.as_ref().and_then(|error| error.get("message")))
            .cloned();
        report(message)?;
    }
    Ok(executions)
}

// ts: BitgetExchange._fetchUtaFillsPage
// Same boundary as fetch_fills_page: the read returns xhr.get's decoded
// response.data and its failure propagates unchanged; a failure is never empty.
pub async fn fetch_uta_fills_page<E, F>(
    params: safe_cex_num::JsJsonValue,
    read: impl FnOnce(&'static str, safe_cex_num::JsJsonValue) -> F,
) -> Result<(Vec<Value>, Option<String>), E>
where
    E: From<JsCoercionError> + From<ExchangeError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let url = crate::protocol::ENDPOINTS
        .iter()
        .find(|entry| entry.name == "UTA_ORDER_FILLS")
        .ok_or_else(|| ExchangeError::new("Bitget protocol table is missing UTA_ORDER_FILLS"))?
        .path;
    let data = read(url, params).await?;
    let code = data.as_ref().and_then(|data| data.get("code"));
    if js_truthy(code) && !crate::normalization::is_bitget_success_code(code)? {
        // getBitgetApiErrorMessage: `payload?.msg || fallback` keeps a truthy non-string msg.
        let message = data.as_ref().and_then(|data| data.get("msg"));
        let message = if js_truthy(message) {
            js_string(message)?
        } else {
            "Bitget UTA fills request failed".into()
        };
        return Err(ExchangeError::new(message).into());
    }
    let payload = data.as_ref().and_then(|data| data.get("data"));
    let rows = payload
        .and_then(|data| data.get("list"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let cursor = payload.and_then(|data| data.get("cursor"));
    let cursor = if js_truthy(cursor) {
        Some(js_string(cursor)?)
    } else {
        None
    };
    Ok((rows, cursor))
}

// ts: BitgetExchange._fetchHistoricalUtaFills
// Mirrors the Classic walk with a sub-30-day window, category read per page,
// optional orderId, a cursor param and post-map normalized-symbol filtering.
pub async fn fetch_historical_uta_fills<R>(
    opts: &safe_cex_types::ExecutionsOptions,
    mut resolve_market: impl FnMut(&str) -> Result<Option<(String, String)>, ExchangeError>,
    mut uta_category: impl FnMut() -> Result<String, ExchangeError>,
    mut read_page: impl FnMut(safe_cex_num::JsJsonValue) -> R,
    (mut error_value, mut report): (
        impl FnMut(&ExchangeError) -> Option<Value>,
        impl FnMut(Option<Value>) -> Result<(), ExchangeError>,
    ),
) -> Result<Vec<safe_cex_types::Execution>, ExchangeError>
where
    R: Future<Output = Result<(Vec<Value>, Option<String>), ExchangeError>>,
{
    use safe_cex_num::{JsJsonValue, js_number_string, math_min};
    let normalized_symbol = if let Some(symbol) = opts.symbol.as_deref().filter(|s| !s.is_empty()) {
        let Some((_, symbol)) = resolve_market(symbol)? else {
            return Ok(Vec::new());
        };
        Some(symbol)
    } else {
        None
    };
    let windows = match (opts.from, opts.to) {
        (Some(from), Some(to)) => safe_cex_candle_cache::calendar::split_time_range(
            from,
            to,
            30.0 * 24.0 * 60.0 * 60.0 * 1000.0 - 1.0,
        )
        .into_iter()
        .map(Some)
        .collect(),
        _ => vec![None],
    };
    let mut executions = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let outcome: Result<(), ExchangeError> = async {
        for window in windows {
            let mut cursor: Option<String> = None;
            for _ in 0..100 {
                let mut params = vec![
                    ("category".into(), JsJsonValue::String(uta_category()?)),
                    (
                        "limit".into(),
                        JsJsonValue::Number(math_min(opts.limit.unwrap_or(100.0), 100.0)),
                    ),
                ];
                if let Some(order_id) = opts.order_id.as_ref().filter(|s| !s.is_empty()) {
                    params.push(("orderId".into(), JsJsonValue::String(order_id.clone())));
                }
                if let Some(window) = &window {
                    for field in ["startTime", "endTime"] {
                        params.push((field.into(), JsJsonValue::from(window[field].clone())));
                    }
                }
                if let Some(cursor) = cursor.as_ref().filter(|s| !s.is_empty()) {
                    params.push(("cursor".into(), JsJsonValue::String(cursor.clone())));
                }
                let (rows, next_cursor) = read_page(JsJsonValue::Object(params)).await?;
                if rows.is_empty() {
                    break;
                }
                let mapped = crate::fills::map_bitget_uta_fills(
                    &rows,
                    &mut crate::fills::BitgetFillDeps {
                        resolve_symbol: &mut |symbol| {
                            Ok(resolve_market(symbol)?.map(|(_, symbol)| symbol))
                        },
                        to_timestamp: &mut |ms| Ok((ms / 1000.0).floor()),
                    },
                )?;
                for execution in mapped {
                    if normalized_symbol
                        .as_ref()
                        .is_some_and(|symbol| !symbol.is_empty() && execution.symbol != *symbol)
                    {
                        continue;
                    }
                    let id = execution.id.clone().unwrap_or_else(|| {
                        format!(
                            "{}|{}|{}|{}",
                            execution.symbol,
                            js_number_string(execution.timestamp),
                            js_number_string(execution.qty),
                            js_number_string(execution.price)
                        )
                    });
                    if seen.insert(id) {
                        executions.push(execution);
                    }
                }
                if next_cursor.as_ref().is_none_or(String::is_empty) || next_cursor == cursor {
                    break;
                }
                cursor = next_cursor;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = outcome {
        let error = error_value(&error);
        let message = error
            .as_ref()
            .and_then(|error| error.get("response"))
            .and_then(|response| response.get("data"))
            .and_then(|data| data.get("msg"))
            .filter(|message| js_truthy(Some(message)))
            .or_else(|| error.as_ref().and_then(|error| error.get("message")))
            .cloned();
        report(message)?;
    }
    Ok(executions)
}

// ts: BitgetExchange.fetchHistoricalPositions
pub async fn fetch_historical_positions(
    venue: &dyn safe_cex_exchange::exchange::Exchange,
    folder: &dyn safe_cex_exchange::journal::JournalFolder,
    options: safe_cex_exchange::journal::HistoricalPositionOptions,
) -> Result<Vec<Value>, ExchangeError> {
    safe_cex_exchange::journal::fold_closed_position_records(venue, folder, options, false).await
}
