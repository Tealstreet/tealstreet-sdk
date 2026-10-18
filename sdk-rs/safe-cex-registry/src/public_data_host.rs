//! Native alias handles keep source array identity visible to provider bodies.
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_types::{Market, StoreDataLoaded, Ticker};
use std::{cell::RefCell, rc::Rc};

pub type TickerBatch = Rc<RefCell<Vec<Ticker>>>;
pub type MarketBatch = Rc<RefCell<Vec<Market>>>;
pub type PublicDataCleanup = Rc<dyn Fn() -> Result<(), ExchangeError>>;
#[derive(Clone)]
pub struct PublicDataSnapshot {
    pub tickers: TickerBatch,
    pub markets: MarketBatch,
    pub loaded: Rc<RefCell<StoreDataLoaded>>,
}
pub struct PublicDataUpdate {
    pub tickers: Option<TickerBatch>,
    pub markets: Option<MarketBatch>,
    pub loaded: Option<StoreDataLoaded>,
}
/// Parent bindings must preserve batch aliases. An owned-Vec Store adapter
/// would leave filtered ticker deltas stale; no such adapter is supplied here.
pub trait PublicDataParent {
    fn name(&self) -> String;
    fn now_ms(&self) -> f64;
    fn loaded(&self) -> Result<StoreDataLoaded, ExchangeError>;
    fn tickers(&self) -> Result<TickerBatch, ExchangeError>;
    fn adapt_markets(&self, markets: MarketBatch) -> Result<MarketBatch, ExchangeError>;
    fn update(&self, update: PublicDataUpdate) -> Result<(), ExchangeError>;
    fn emit_log(&self, message: String, severity: LogSeverity) -> Result<(), ExchangeError>;
}
