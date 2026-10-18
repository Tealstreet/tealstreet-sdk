//! Market and quote reads over shared Store, transport requests and numeric ingress.
use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown, from_retained_input, to_retained_input, to_typed},
    runtime::ExchangeError,
};
use safe_cex_num::{js_truthy, numeric_ingress::JsNumberInput};
use safe_cex_store::Store;
use safe_cex_transport::provider::{RestBody, RestRequest};
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision, Ticker,
};
use serde_json::{Value, json};
use std::future::Future;
fn raw(
    input: &JsNumberInput,
) -> Result<safe_cex_types::retained_json::RetainedJsonInput, ExchangeError> {
    input
        .clone()
        .decode()
        .map_err(|error| ExchangeError::new(error.to_string()))
}
fn market_property<T: serde::de::DeserializeOwned>(source: &Js, key: &str) -> Result<T, Thrown> {
    to_typed(&source.get(key)?)
}
fn report(
    error: ExchangeError,
    logger: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    let message = error
        .response
        .as_ref()
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("msg"));
    logger(if js_truthy(message) {
        message.cloned().unwrap()
    } else {
        Value::String(error.message)
    })
}
pub fn fetch_markets<F>(
    combined: impl FnOnce() -> F,
    store: &dyn Store,
    logger: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Vec<Market>, ExchangeError>>
where
    F: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    let pending = combined();
    async move {
        // Source await is outside the catch: raw rejection must propagate unchanged.
        let input = pending.await?;
        let raw = raw(&input)?;
        if !raw.is_truthy() || raw.as_array().is_some_and(<[_]>::is_empty) {
            return Ok(store.markets()?.to_vec());
        }
        let mapped = (|| -> Result<Vec<Market>, Thrown> {
            let rows = raw
                .as_array()
                .ok_or_else(|| Thrown::type_error("data.map is not a function"))?;
            let mut markets = Vec::with_capacity(rows.len());
            for row in rows {
                let row_value = from_retained_input(row.clone());
                let instrument = row_value.get("instrument")?;
                let details = row_value.get("instrumentDetails")?;
                let raw_symbol = instrument.get("symbol")?;
                let base = to_typed(
                    &raw_symbol.call_method("replace", vec![Js::str("USD"), Js::str("")])?,
                )?;
                let symbol: String = to_typed(&raw_symbol)?;
                markets.push(Market {
                    runtime: Default::default(),
                    id: symbol.clone(),
                    symbol,
                    pretty_symbol: None,
                    base,
                    quote: "USD".into(),
                    active: details
                        .get("tradingStatus")?
                        .strict_equals(&Js::str("FULL")),
                    precision: MarketPrecision {
                        base: None,
                        quote: None,
                        amount: market_property(&details, "minOrderSizeIncrement")?,
                        price: market_property(&instrument, "priceIncrement")?,
                    },
                    limits: MarketLimits {
                        cost: None,
                        amount: MarketLimitsAmount {
                            min: market_property(&details, "minOrderSize")?,
                            max: market_property(&details.get("maxOrderSize")?, "value")?,
                        },
                        leverage: MarketLimitsLeverage {
                            min: 1.0,
                            max: 1.0
                                / details
                                    .get("marginRate")?
                                    .get("tiers")?
                                    .get("0")?
                                    .get("value")?
                                    .to_number()?,
                        },
                        price: None,
                    },
                    contract_size: Some(market_property(&instrument, "lotSize")?),
                    is_expiring: Some(false),
                    is_inverse: Some(false),
                    is_spot: Some(false),
                    is_perpetual: None,
                    is_read_only: Some(false),
                    info: Some(row.clone()),
                    taker_fee: Some(0.0),
                    maker_fee: Some(0.0),
                    option: None,
                });
            }
            Ok(markets)
        })();
        match mapped {
            Ok(markets) => Ok(markets),
            Err(error) => {
                report(error.to_exchange_error(), logger)?;
                Ok(store.markets()?.to_vec())
            }
        }
    }
}
pub fn fetch_tickers_for_symbols<S, A, R>(
    symbols: Vec<String>,
    session: impl FnOnce() -> S,
    account: impl FnOnce() -> A,
    request: impl FnOnce(RestRequest) -> R,
    store: &dyn Store,
    logger: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Option<Vec<Ticker>>, ExchangeError>>
where
    S: Future<Output = Result<String, ExchangeError>>,
    A: Future<Output = Result<String, ExchangeError>>,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    let pending = session();
    async move {
        let response = async {
            pending.await?;
            let account = account().await?;
            request(RestRequest {
                method: "POST".into(),
                url: "marketdata".into(),
                body: Some(RestBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"eventTypes":[{"type":"Quote"}],"symbols":symbols,"account":account})).map_err(|error|ExchangeError::new(error.to_string()))?,
                )),
                ..RestRequest::default()
            })
            .await
        }
        .await;
        let input = match response {
            Ok(input) => input,
            Err(error) => {
                report(error, logger)?;
                return Ok(Some(store.tickers()?.to_vec()));
            }
        };
        let data = raw(&input)?;
        let mapped: Result<_, Thrown> = (|| {
            let events = from_retained_input(data).get("events")?;
            if events.is_nullish() {
                return Ok(None);
            }
            let events = events
                .array_values()
                .ok_or_else(|| Thrown::type_error("data.events?.map is not a function"))?;
            let mut fields = Vec::with_capacity(events.len());
            for event in &events {
                fields.push((
                    event.get("symbol")?,
                    event.get("bid")?,
                    event.get("ask")?,
                    to_retained_input(event)?,
                ));
            }
            Ok(Some(fields))
        })();
        let fields = match mapped {
            Ok(Some(fields)) => fields,
            Ok(None) => return Ok(None),
            Err(error) => {
                report(error.to_exchange_error(), logger)?;
                return Ok(Some(store.tickers()?.to_vec()));
            }
        };
        let mut tickers = Vec::with_capacity(fields.len());
        for (symbol, bid, ask, info) in fields {
            let symbol: String = to_typed(&symbol)?;
            let bid = to_typed(&bid)?;
            let ask = to_typed(&ask)?;
            tickers.push(Ticker {
                id: symbol.clone(),
                symbol,
                bid,
                ask,
                last: bid,
                mark: bid,
                index: bid,
                mid_price: None,
                percentage: 0.0,
                open_interest: 0.0,
                funding_rate: 0.0,
                funding_time: 0.0,
                volume: 0.0,
                quote_volume: 0.0,
                timestamp: None,
                info: Some(info),
            });
        }
        Ok(Some(tickers))
    }
}
pub fn fetch_tickers<F>(
    store: &dyn Store,
    fetch: impl FnOnce(Vec<String>) -> F,
) -> Result<F, ExchangeError>
where
    F: Future<Output = Result<Option<Vec<Ticker>>, ExchangeError>>,
{
    let symbols = store.markets()?.iter().map(|m| m.symbol.clone()).collect();
    Ok(fetch(symbols))
}
