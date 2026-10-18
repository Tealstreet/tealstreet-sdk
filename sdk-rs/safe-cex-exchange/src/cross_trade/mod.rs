//! Source: packages/safe-cex/src/crossTrade. Native host assembly is explicit.
pub mod helpers;
pub mod manager;
pub mod place_order;
pub mod types;
use crate::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_types::{
    ActiveChaser, Balance, ExchangeName, Market, OrderSide, PlaceOrderOpts, Position, Ticker,
    object_map::JsObjectMap,
};
use std::{any::Any, cell::Cell, rc::Rc};
pub use types::{
    CrossTradWeightMode, CrossTradeConfig, CrossTradeCopier, CrossTradeExecution,
    default_cross_trade_config, default_cross_trade_copier,
};

#[derive(Debug, Clone, Default)]
// Deserialize below enforces the object boundary.
pub struct CrossTradeStore {
    pub positions_loaded: bool,
    pub positions: Vec<Position>,
    pub markets: Vec<Market>,
    pub tickers: Vec<Ticker>,
    pub tickers_by_symbol: JsObjectMap<Ticker>,
    pub balance: Option<Balance>,
    pub active_chasers: Vec<ActiveChaser>,
}
pub type CrossTradeValue = Rc<dyn Any>;
pub type CrossTradePlaceOrderFn =
    Rc<dyn Fn(CrossTradeOrder) -> ExchangeFuture<'static, Option<CrossTradeValue>>>;
pub type CrossTradeGetExchangeFn =
    Rc<dyn Fn(&str) -> Result<Option<Rc<dyn CrossTradeExchange>>, ExchangeError>>;
pub type CrossTradeLoopFn = Rc<dyn Fn() -> ExchangeFuture<'static, ()>>;
#[derive(Clone)]
pub struct CrossTradeOrder {
    pub copier: CrossTradeCopier,
    pub market: Market,
    pub reference_price: Option<f64>,
    pub account: String,
    pub side: OrderSide,
    pub amount: f64,
    pub reduce_only: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ChaserStart {
    pub symbol: String,
    pub size: f64,
    pub side: OrderSide,
    pub max_attempts: f64,
    pub reduce_only: bool,
    pub delay: f64,
}
pub trait CrossTradeExchange {
    fn name(&self) -> ExchangeName;
    /// Returns a native snapshot; arbitrary JS getters/alias mutation are excluded.
    fn store(&self) -> CrossTradeStore;
    /// Start work eagerly; opaque results retain their native identity.
    fn place_order(
        &self,
        order: PlaceOrderOpts,
    ) -> ExchangeFuture<'static, Option<CrossTradeValue>>;
    fn chaser_terminate(&self, id: &str) -> Result<(), ExchangeError>;
    fn chaser_start(&self, options: ChaserStart) -> Result<(), ExchangeError>;
    fn warning(&self, message: &str) -> Result<(), ExchangeError>;
}
pub trait CrossTradeRuntime {
    fn now_ms(&self) -> f64;
    fn debug(&self, message: &str);
    fn error(&self, message: &str, error: &ExchangeError);
    /// Bind the canonical loop scheduler. Enqueue after the supplied interval;
    /// never invoke the callback inline. Host owns and drives returned work.
    fn schedule_loop(&self, callback: CrossTradeLoopFn, min_interval_ms: u64);
}
#[derive(Clone)]
pub struct CrossTradeLoop {
    pub(crate) disposed: Rc<Cell<bool>>,
    pub(crate) callback: CrossTradeLoopFn,
}
impl CrossTradeLoop {
    pub fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    pub fn run(&self) -> ExchangeFuture<'static, ()> {
        (self.callback)()
    }
}
/// Poll all requests, preserving input order like Promise.all. Requests are
/// created eagerly by callers; native drop/error cancellation is a CHOSEN seam.
pub async fn join_all<T>(
    requests: Vec<ExchangeFuture<'static, T>>,
) -> Result<Vec<T>, ExchangeError> {
    let mut pending: Vec<_> = requests.into_iter().map(Some).collect();
    let mut results: Vec<Option<T>> = (0..pending.len()).map(|_| None).collect();
    std::future::poll_fn(move |cx| {
        for (index, request) in pending.iter_mut().enumerate() {
            if let Some(future) = request {
                match future.as_mut().poll(cx) {
                    std::task::Poll::Ready(Ok(value)) => {
                        results[index] = Some(value);
                        *request = None;
                    }
                    std::task::Poll::Ready(Err(error)) => {
                        return std::task::Poll::Ready(Err(error));
                    }
                    std::task::Poll::Pending => {}
                }
            }
        }
        if pending.iter().all(Option::is_none) {
            std::task::Poll::Ready(Ok(results.iter_mut().map(|v| v.take().unwrap()).collect()))
        } else {
            std::task::Poll::Pending
        }
    })
    .await
}
pub(crate) fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
pub(crate) fn first_price(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    if a.is_some_and(truthy) { a } else { b }
}
pub(crate) fn done<T: 'static>(result: Result<T, ExchangeError>) -> ExchangeFuture<'static, T> {
    Box::pin(async move { result })
}

// A store snapshot is an object boundary; Serde's derived struct visitor also
// accepts positional sequences unless the input is constrained here.
#[derive(serde::Deserialize)]
#[serde(remote = "CrossTradeStore", rename_all = "camelCase")]
struct CrossTradeStoreWire {
    positions_loaded: bool,
    positions: Vec<Position>,
    markets: Vec<Market>,
    tickers: Vec<Ticker>,
    tickers_by_symbol: JsObjectMap<Ticker>,
    balance: Option<Balance>,
    active_chasers: Vec<ActiveChaser>,
}
impl<'de> serde::Deserialize<'de> for CrossTradeStore {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CrossTradeStoreWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}
