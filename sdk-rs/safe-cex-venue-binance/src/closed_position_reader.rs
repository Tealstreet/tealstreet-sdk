use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{ClosedPnlEntry, HistoricalTradeOptions};
use serde_json::{Map, Value};
use std::{cmp::Ordering, future::Future};

// ts: BinanceExchange.deriveClosedPositions
/// The configured GET and error emitter remain explicit caller operations until
/// concrete exchange composition. All pages are read before source row mapping.
pub async fn derive_binance_closed_positions<G, F>(
    opts: &HistoricalTradeOptions,
    public_data_only: bool,
    now: impl FnOnce() -> f64,
    mut get: G,
    mut emit_error: impl FnMut(Value) -> Result<(), ExchangeError>,
) -> Result<Vec<ClosedPnlEntry>, ExchangeError>
where
    G: FnMut(&'static str, Value) -> F,
    F: Future<Output = Result<Option<Value>, ExchangeError>>,
{
    if public_data_only || opts.symbol.is_empty() {
        return Ok(vec![]);
    }
    let to = opts.to.unwrap_or_else(now);
    let from = opts.from.unwrap_or(to - 30.0 * 24.0 * 60.0 * 60.0 * 1000.0);
    let result: Result<Vec<ClosedPnlEntry>, ExchangeError> = async {
        let mut rows = vec![];
        for window in safe_cex_candle_cache::calendar::split_time_range(
            from,
            to,
            crate::utils::USER_TRADES_MAX_WINDOW_MS,
        ) {
            let mut params = Map::new();
            params.insert("symbol".into(), Value::String(opts.symbol.clone()));
            params.insert("startTime".into(), window["startTime"].clone());
            params.insert("endTime".into(), window["endTime"].clone());
            params.insert("limit".into(), Value::from(1000));
            if let Some(Value::Array(page)) =
                get("/fapi/v1/userTrades", Value::Object(params)).await?
            {
                rows.extend(page);
            }
        }
        let mut entries = vec![];
        for row in &rows {
            match row {
                Value::Null => {
                    let mut error = ExchangeError::new(
                        "Cannot read properties of null (reading 'realizedPnl')",
                    );
                    error.name = "TypeError".into();
                    return Err(error);
                }
                Value::Object(record) => {
                    let mapped = crate::closed_pnl::derive_binance_closed_positions(
                        std::slice::from_ref(record),
                        opts.account_name.as_deref().unwrap_or(""),
                    )
                    .map_err(|error: safe_cex_num::JsCoercionError| {
                        let mut mapped = ExchangeError::new(error.message);
                        mapped.name = error.name.into();
                        mapped
                    })?;
                    entries.extend(mapped);
                }
                // JSON primitives and arrays have no realizedPnl property, so
                // the source toNumber defaults it to zero and skips the row.
                _ => {}
            }
        }
        entries.sort_by(|a, b| {
            b.closed_time
                .partial_cmp(&a.closed_time)
                .unwrap_or(Ordering::Equal)
        });
        Ok(entries)
    }
    .await;
    match result {
        Ok(entries) => Ok(entries),
        Err(error) => {
            let message = error
                .response_json_view()
                .as_ref()
                .and_then(|response| response.get("data"))
                .and_then(|data| data.get("msg"))
                .filter(|message| safe_cex_num::js_truthy(Some(message)))
                .cloned()
                .unwrap_or(Value::String(error.message));
            emit_error(message)?;
            Ok(vec![])
        }
    }
}
