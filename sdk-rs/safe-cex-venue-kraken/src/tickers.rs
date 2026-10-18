//! Futures ticker read; shared get-key, number coercion and Store views.
use crate::{types, wire};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    get_key::{GetKeyUnknownValue, v_unknown},
    js_string, js_truthy,
    numeric_ingress::JsNumberInput,
    parse_float,
};
use safe_cex_store::Store;
use safe_cex_types::Ticker;
use serde_json::Value;
use std::future::Future;

fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
fn string(value: Option<&Value>) -> Result<String, ExchangeError> {
    js_string(value).map_err(|error| type_error(error.message))
}
fn keyed(row: &Value, key: &str) -> Result<String, ExchangeError> {
    match v_unknown(Some(row), key).map_err(|error| type_error(error.message))? {
        GetKeyUnknownValue::Undefined => string(None),
        GetKeyUnknownValue::Json(value) => string(Some(value)),
        GetKeyUnknownValue::Length(value) => Ok(safe_cex_num::js_number_string(value as f64)),
        GetKeyUnknownValue::ObjectInherited(value) => Ok(value.js_string().into()),
        GetKeyUnknownValue::ArrayInherited(value) => Ok(value.js_string()),
        GetKeyUnknownValue::ArrayPrototype => Ok(String::new()),
    }
}
fn number(text: &str) -> f64 {
    let value = parse_float(text);
    if value == 0.0 || value.is_nan() {
        0.0
    } else {
        value
    }
}

pub async fn fetch_tickers<F>(
    store: &dyn Store,
    read: impl FnOnce(&'static str) -> Result<F, ExchangeError>,
    trace: impl FnOnce(&ExchangeError) -> Result<(), ExchangeError>,
) -> Result<Vec<Ticker>, ExchangeError>
where
    F: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>,
{
    let outcome: Result<Vec<Ticker>, ExchangeError> = async {
        let endpoint = types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "TICKERS")
            .expect("ticker endpoint")
            .1;
        let data = read(endpoint)?
            .await?
            .map(wire::decode)
            .transpose()
            .map_err(|error| ExchangeError::new(error.to_string()))?;
        let empty = Vec::new();
        let rows = match data
            .as_ref()
            .and_then(|data| data.get("tickers"))
            .filter(|value| js_truthy(Some(value)))
        {
            None => &empty,
            Some(Value::Array(rows)) => rows,
            _ => return Err(type_error("tickers.reduce is not a function")),
        };
        let mut tickers = Vec::new();
        for row in rows {
            let symbol = keyed(row, "symbol")?;
            let markets = store.markets_by_symbol();
            let Some(market) = markets.get(&symbol) else {
                continue;
            };
            if row.is_null() {
                return Err(type_error(
                    "Cannot read properties of null (reading 'markPrice')",
                ));
            }
            let mark = number(&string(row.get("markPrice"))?);
            let index = number(&string(row.get("indexPrice"))?);
            let real_last = number(&keyed(row, "last")?);
            let last = if real_last > 0.0 { real_last } else { mark };
            let percentage = number(&string(row.get("change24h"))?);
            let funding_absolute = number(&keyed(row, "fundingRate")?);
            let funding_rate = if last > 0.0 {
                funding_absolute / last
            } else {
                0.0
            };
            tickers.push(Ticker {
                id: market.id.clone(),
                symbol: market.symbol.clone(),
                bid: number(&keyed(row, "bid")?),
                ask: number(&keyed(row, "ask")?),
                last,
                mark: if mark > 0.0 { mark } else { last },
                index: if index > 0.0 { index } else { last },
                mid_price: None,
                timestamp: None,
                percentage,
                funding_rate,
                funding_time: 0.0,
                volume: number(&string(row.get("vol24h"))?),
                quote_volume: number(&string(row.get("volumeQuote"))?),
                open_interest: number(&keyed(row, "openInterest")?),
                info: Some(row.clone().into()),
            });
        }
        Ok(tickers)
    }
    .await;
    match outcome {
        Ok(tickers) => Ok(tickers),
        Err(error) => {
            // ts-bug: trace then return the current Store snapshot, swallowing the read failure.
            trace(&error)?;
            Ok(store.tickers().to_vec())
        }
    }
}
