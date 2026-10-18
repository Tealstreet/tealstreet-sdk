//! Source ticker reduction over the shared Store and numeric ingress contracts.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{js_string, js_truthy, parse_float};
use safe_cex_store::Store;
use safe_cex_types::Ticker;
use serde_json::Value;
use std::future::Future;

fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
fn numeric(value: Option<&Value>) -> Result<f64, ExchangeError> {
    js_string(value)
        .map(|text| parse_float(&text))
        .map_err(|error| {
            let mut result = ExchangeError::new(error.message);
            result.name = error.name.into();
            result
        })
}

// ts: PhemexExchange.parseTickers
pub fn parse_tickers(
    data: Option<&Value>,
    store: &(impl Store + ?Sized),
) -> Result<Vec<Ticker>, ExchangeError> {
    let result = data.and_then(|value| value.get("result"));
    if !js_truthy(data) || !js_truthy(result) {
        return Ok(store.tickers().to_vec());
    }
    let rows = result
        .and_then(Value::as_array)
        .ok_or_else(|| type_error("data.result.reduce is not a function"))?;
    let mut tickers = Vec::new();
    for row in rows {
        if row.is_null() {
            return Err(type_error(
                "Cannot read properties of null (reading 'symbol')",
            ));
        }
        let raw_symbol = row.get("symbol");
        let symbol = match raw_symbol {
            None | Some(Value::Null) => crate::utilities::format_symbol_in(None),
            Some(Value::String(value)) => crate::utilities::format_symbol_in(Some(value)),
            _ => return Err(type_error("symbol?.toUpperCase is not a function")),
        };
        let markets = store.markets_by_symbol();
        let Some(market) = markets.get(&symbol) else {
            continue;
        };
        let raw_symbol = raw_symbol.and_then(Value::as_str).ok_or_else(|| {
            type_error(format!(
                "Cannot read properties of {} (reading 'endsWith')",
                if raw_symbol.is_some() {
                    "null"
                } else {
                    "undefined"
                }
            ))
        })?;
        if !raw_symbol.ends_with("USDT") && !raw_symbol.ends_with("USDC") {
            continue;
        }
        tickers.push(Ticker {
            id: market.id.clone(),
            symbol,
            bid: numeric(row.get("bidRp"))?,
            ask: numeric(row.get("askRp"))?,
            last: numeric(row.get("lastRp"))?,
            mark: numeric(row.get("markRp"))?,
            index: Some(numeric(row.get("indexRp"))?),
            percentage: ((numeric(row.get("lastRp"))? - numeric(row.get("openRp"))?)
                / numeric(row.get("openRp"))?)
                * 100.0,
            open_interest: numeric(row.get("openInterestRv"))?,
            funding_rate: numeric(row.get("fundingRateRr"))?,
            funding_time: 0.0,
            volume: numeric(row.get("volumeRq"))?,
            quote_volume: numeric(row.get("volumeRq"))? * numeric(row.get("lastRp"))?,
            mid_price: None,
            info: None,
            timestamp: None,
        });
    }
    Ok(tickers)
}

// ts: PhemexExchange.fetchTickers
pub async fn fetch_tickers<E, F>(
    store: &(impl Store + ?Sized),
    read: impl FnOnce(&'static str) -> F,
    emit_and_log_error: impl FnOnce(Option<&Value>) -> Result<(), E>,
) -> Result<Vec<Ticker>, E>
where
    E: From<ExchangeError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let data = crate::ticker_reads::fetch_tickers_raw(read, emit_and_log_error).await?;
    Ok(parse_tickers(Some(&data), store)?)
}

#[cfg(test)]
mod accepted_native_contract {
    use super::*;

    #[test]
    fn type_error_constructor_preserves_name_and_message() {
        for text in ["not an object", "雪🚀"] {
            let error = type_error(text);
            assert_eq!(error.name, "TypeError");
            assert_eq!(error.message, text);
            let owned = type_error(text.to_owned());
            assert_eq!(owned.name, "TypeError");
            assert_eq!(owned.message, text);
        }
    }
}
