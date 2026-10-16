//! Market and quote reads over shared Store, transport requests and numeric ingress.
use crate::types::{Instrument, InstrumentDetails, TickerResponse};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_number_output, js_truthy,
    numeric_ingress::{
        JsNumberInput, JsNumberOutput, js_deserialize_passthrough, js_from_value_ref,
    },
};
use safe_cex_store::Store;
use safe_cex_transport::provider::{RestBody, RestRequest};
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision, Ticker,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::future::Future;
#[derive(Serialize)]
#[serde(transparent)]
struct Raw {
    value: Value,
}
impl JsNumberOutput for Raw {
    fn check_number_output(&self) {
        js_number_output!(self,struct {} passthrough {value});
    }
}
fn raw(input: &JsNumberInput) -> Result<Value, ExchangeError> {
    input
        .clone()
        .decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value }))
        .map(|r| r.value)
        .map_err(|e| ExchangeError::new(e.to_string()))
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Combined {
    instrument: Instrument,
    instrument_details: InstrumentDetails,
}
impl JsNumberOutput for Combined {
    fn check_number_output(&self) {
        js_number_output!(self,struct {instrument,instrument_details});
    }
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
        if !js_truthy(Some(&raw)) || raw.as_array().is_some_and(Vec::is_empty) {
            return Ok(store.markets().to_vec());
        }
        // ts-bug CHOSEN: declared response validation precedes mapping.
        let rows = raw
            .as_array()
            .ok_or_else(|| ExchangeError::new("Expected combined instrument array"))?;
        let mut markets = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let row: Combined =
                js_from_value_ref(row).map_err(|e| ExchangeError::new(e.to_string()))?;
            let Some(tier) = row.instrument_details.margin_rate.tiers.first() else {
                report(
                    ExchangeError::new("Cannot read properties of undefined (reading 'value')"),
                    logger,
                )?;
                return Ok(store.markets().to_vec());
            };
            markets.push(Market {
                id: row.instrument.symbol.clone(),
                symbol: row.instrument.symbol.clone(),
                pretty_symbol: None,
                base: row.instrument.symbol.replacen("USD", "", 1),
                quote: "USD".into(),
                active: row.instrument_details.trading_status == "FULL",
                precision: MarketPrecision {
                    amount: row.instrument_details.min_order_size_increment,
                    price: row.instrument.price_increment,
                },
                limits: MarketLimits {
                    amount: MarketLimitsAmount {
                        min: row.instrument_details.min_order_size,
                        max: row.instrument_details.max_order_size.value,
                    },
                    leverage: MarketLimitsLeverage {
                        min: 1.0,
                        max: 1.0 / tier.value,
                    },
                    price: None,
                },
                contract_size: Some(row.instrument.lot_size),
                is_expiring: Some(false),
                is_inverse: Some(false),
                is_spot: Some(false),
                is_perpetual: None,
                is_read_only: Some(false),
                info: Some(raw[index].clone()),
                taker_fee: Some(0.0),
                maker_fee: Some(0.0),
                option: None,
            });
        }
        Ok(markets)
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
                    json!({"eventTypes":[{"type":"Quote"}],"symbols":symbols,"account":account}),
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
                return Ok(Some(store.tickers().to_vec()));
            }
        };
        let data = raw(&input)?;
        if data.is_null() {
            report(
                ExchangeError::new("Cannot read properties of null (reading 'events')"),
                logger,
            )?;
            return Ok(Some(store.tickers().to_vec()));
        }
        if data.get("events").is_none_or(Value::is_null) {
            return Ok(None);
        }
        // ts-bug CHOSEN declared records; malformed event shapes refuse.
        let typed: TickerResponse = input
            .decode()
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        let mut tickers = Vec::with_capacity(typed.events.len());
        for (index, event) in typed.events.iter().enumerate() {
            tickers.push(Ticker {
                id: event.symbol.clone(),
                symbol: event.symbol.clone(),
                bid: event.bid,
                ask: event.ask,
                last: event.bid,
                mark: event.bid,
                index: event.bid,
                mid_price: None,
                percentage: 0.0,
                open_interest: 0.0,
                funding_rate: 0.0,
                funding_time: 0.0,
                volume: 0.0,
                quote_volume: 0.0,
                timestamp: None,
                info: Some(data["events"][index].clone()),
            });
        }
        Ok(Some(tickers))
    }
}
pub fn fetch_tickers<F>(store: &dyn Store, fetch: impl FnOnce(Vec<String>) -> F) -> F
where
    F: Future<Output = Result<Option<Vec<Ticker>>, ExchangeError>>,
{
    let symbols = store.markets().iter().map(|m| m.symbol.clone()).collect();
    fetch(symbols)
}
