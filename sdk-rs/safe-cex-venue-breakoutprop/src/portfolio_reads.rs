//! Portfolio read orchestration; the raw request hook is supplied by the venue host.
use crate::portfolio::{format_balance, format_open_positions, format_orders, format_positions};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_number_output, js_truthy,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough},
};
use serde::Serialize;
#[derive(Serialize)]
#[serde(transparent)]
struct RawTruthiness {
    value: serde_json::Value,
}
impl JsNumberOutput for RawTruthiness {
    fn check_number_output(&self) {
        js_number_output!(self, struct {} passthrough {value});
    }
}
fn truthy_input(input: Option<JsNumberInput>) -> Result<Option<JsNumberInput>, ExchangeError> {
    let Some(input) = input else {
        return Ok(None);
    };
    let raw = input
        .clone()
        .decode_with(|d| js_deserialize_passthrough(d).map(|value| RawTruthiness { value }))
        .map_err(|e| ExchangeError::new(e.to_string()))?;
    Ok(if js_truthy(Some(&raw.value)) {
        Some(input)
    } else {
        None
    })
}
use safe_cex_store::Store;
use safe_cex_types::{Balance, Order, Position};
use std::future::Future;

pub fn fetch_balance<F, E>(
    fetch_portfolio_raw: impl FnOnce() -> F,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
    format_error: impl FnOnce(ExchangeError) -> E,
) -> impl Future<Output = Result<Balance, E>>
where
    F: Future<Output = Result<Option<JsNumberInput>, E>>,
{
    let pending = fetch_portfolio_raw();
    async move {
        let data = pending.await?;
        (|| {
            let Some(data) = truthy_input(data)? else {
                return Ok(store.balance().clone());
            };
            format_balance(&data, store, local_to_utc, report)
        })()
        .map_err(format_error)
    }
}

pub fn fetch_positions<F, E>(
    fetch_portfolio_raw: impl FnOnce() -> F,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    symbols: Option<&[String]>,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
    format_error: impl FnOnce(ExchangeError) -> E,
) -> impl Future<Output = Result<Vec<Position>, E>>
where
    F: Future<Output = Result<Option<JsNumberInput>, E>>,
{
    let pending = fetch_portfolio_raw();
    async move {
        let data = pending.await?;
        (|| {
            let Some(data) = truthy_input(data)? else {
                return Ok(store.positions().to_vec());
            };
            format_positions(&data, store, symbols, local_to_utc, report)
        })()
        .map_err(format_error)
    }
}

pub fn fetch_open_positions<F, E>(
    fetch_portfolio_raw: impl FnOnce() -> F,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
    format_error: impl FnOnce(ExchangeError) -> E,
) -> impl Future<Output = Result<Option<Vec<Position>>, E>>
where
    F: Future<Output = Result<Option<JsNumberInput>, E>>,
{
    let pending = fetch_portfolio_raw();
    async move {
        let data = pending.await?;
        (|| {
            let Some(data) = truthy_input(data)? else {
                return Ok(Some(store.positions().to_vec()));
            };
            format_open_positions(&data, store, local_to_utc, report)
        })()
        .map_err(format_error)
    }
}

pub fn fetch_orders<F, E>(
    fetch_portfolio_raw: impl FnOnce() -> F,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
    format_error: impl FnOnce(ExchangeError) -> E,
) -> impl Future<Output = Result<Option<Vec<Order>>, E>>
where
    F: Future<Output = Result<Option<JsNumberInput>, E>>,
{
    let pending = fetch_portfolio_raw();
    async move {
        let data = pending.await?;
        (|| {
            let Some(data) = truthy_input(data)? else {
                return Ok(Some(store.orders().to_vec()));
            };
            format_orders(&data, store, local_to_utc, report)
        })()
        .map_err(format_error)
    }
}
