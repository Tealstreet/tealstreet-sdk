//! Futures ticker read; shared get-key, number coercion and Store views.
use crate::types;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_num::{js_retained_parse_float, js_retained_string};
use safe_cex_store::Store;
use safe_cex_types::Ticker;
use safe_cex_types::retained_json::RetainedJsonInput as Input;

use std::future::Future;

fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
fn string(value: Option<&Input>) -> Result<String, ExchangeError> {
    js_retained_string(value).map_err(|error| type_error(error.message))
}
fn keyed(row: &Input, key: &str) -> Result<Input, ExchangeError> {
    use safe_cex_exchange::js_runtime::{
        library::in_operator,
        value::{from_retained_input, to_retained_input},
    };
    let row = from_retained_input(row.clone());
    if row.is_nullish() {
        return Ok(Input::Undefined);
    }
    for spelling in [
        key.to_owned(),
        safe_cex_num::lodash_snake_case(key),
        safe_cex_num::lodash_camel_case(key),
    ] {
        if in_operator(&row, &spelling).map_err(|cause| cause.to_exchange_error())? {
            return row
                .get(&spelling)
                .and_then(|value| to_retained_input(&value))
                .map_err(|cause| cause.to_exchange_error());
        }
    }
    Ok(Input::Undefined)
}
fn number(value: Option<&Input>) -> Result<f64, ExchangeError> {
    let value = js_retained_parse_float(value).map_err(|cause| type_error(cause.message))?;
    if value == 0.0 || value.is_nan() {
        Ok(0.0)
    } else {
        Ok(value)
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
            .map(JsNumberInput::decode::<Input>)
            .transpose()
            .map_err(|error| ExchangeError::new(error.to_string()))?;
        let empty = Vec::new();
        let rows = match data
            .as_ref()
            .and_then(|data| data.get("tickers"))
            .filter(|value| value.is_truthy())
        {
            None => &empty,
            Some(Input::Array(rows)) => rows,
            _ => return Err(type_error("tickers.reduce is not a function")),
        };
        let mut tickers = Vec::new();
        for row in rows {
            let symbol = string(Some(&keyed(row, "symbol")?))?;
            let markets = store.markets_by_symbol()?;
            let Some(market) = markets.get(&symbol) else {
                continue;
            };
            if row.is_null() {
                return Err(type_error(
                    "Cannot read properties of null (reading 'markPrice')",
                ));
            }
            let mark = number(row.get("markPrice"))?;
            let index = number(row.get("indexPrice"))?;
            let real_last = number(Some(&keyed(row, "last")?))?;
            let last = if real_last > 0.0 { real_last } else { mark };
            let percentage = number(row.get("change24h"))?;
            let funding_absolute = number(Some(&keyed(row, "fundingRate")?))?;
            let funding_rate = if last > 0.0 {
                funding_absolute / last
            } else {
                0.0
            };
            tickers.push(Ticker {
                id: market.id.clone(),
                symbol: market.symbol.clone(),
                bid: number(Some(&keyed(row, "bid")?))?,
                ask: number(Some(&keyed(row, "ask")?))?,
                last,
                mark: if mark > 0.0 { mark } else { last },
                index: if index > 0.0 { index } else { last },
                mid_price: None,
                timestamp: None,
                percentage,
                funding_rate,
                funding_time: 0.0,
                volume: number(row.get("vol24h"))?,
                quote_volume: number(row.get("volumeQuote"))?,
                open_interest: number(Some(&keyed(row, "openInterest")?))?,
                info: Some(row.clone()),
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
            Ok(store.tickers()?.to_vec())
        }
    }
}
