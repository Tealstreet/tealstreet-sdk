use crate::{
    public_data::EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS,
    public_data_host::{
        MarketBatch, PublicDataCleanup, PublicDataParent, PublicDataSnapshot, PublicDataUpdate,
        TickerBatch,
    },
};
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_types::{Candle, OHLCVOptions, OrderBook};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub type PublicDataObserver = Rc<dyn Fn(PublicDataSnapshot) -> Result<(), ExchangeError>>;
pub type OrderbookObserver = Rc<dyn Fn(&OrderBook) -> Result<(), ExchangeError>>;
pub type CandleObserver = Rc<dyn Fn(&Candle) -> Result<(), ExchangeError>>;
/// The source supplies its actual current array handles, including in-place deltas.
/// Existing PublicDataSource's owned StoreData callback cannot express this identity.
pub trait InProcessSource {
    fn snapshot(&self) -> Result<PublicDataSnapshot, ExchangeError>;
    fn subscribe(&self, callback: PublicDataObserver) -> Result<PublicDataCleanup, ExchangeError>;
    fn listen_order_book(
        &self,
        symbol: String,
        callback: OrderbookObserver,
    ) -> Result<PublicDataCleanup, ExchangeError>;
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: CandleObserver,
    ) -> Result<PublicDataCleanup, ExchangeError>;
}
pub struct InProcessPublicDataProvider {
    parent: Rc<dyn PublicDataParent>,
    source: Rc<dyn InProcessSource>,
    unsubscribe_store: RefCell<Option<PublicDataCleanup>>,
    is_disposed: Cell<bool>,
    initialized: Cell<bool>,
    last_received: Cell<f64>,
}
impl InProcessPublicDataProvider {
    pub fn new(parent: Rc<dyn PublicDataParent>, source: Rc<dyn InProcessSource>) -> Rc<Self> {
        Rc::new(Self {
            parent,
            source,
            unsubscribe_store: RefCell::new(None),
            is_disposed: Cell::new(false),
            initialized: Cell::new(false),
            last_received: Cell::new(0.0),
        })
    }
    pub fn connect(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.is_disposed.get() {
            return Ok(());
        }
        self.log(
            "Connecting to in-process public data source",
            LogSeverity::Info,
        )?;
        let data = self.source.snapshot()?;
        if data.loaded.borrow().tickers && !data.tickers.borrow().is_empty() {
            self.note_data_observed();
            let mut loaded = self.parent.loaded();
            loaded.tickers = true;
            self.parent.update(PublicDataUpdate {
                tickers: Some(data.tickers),
                markets: None,
                loaded: Some(loaded),
            })?;
        }
        if data.loaded.borrow().markets && !data.markets.borrow().is_empty() {
            self.note_data_observed();
            let markets = self.parent.adapt_markets(data.markets)?;
            let mut loaded = self.parent.loaded();
            loaded.markets = true;
            self.parent.update(PublicDataUpdate {
                tickers: None,
                markets: Some(markets),
                loaded: Some(loaded),
            })?;
        }
        // Reset per connection, independently of seeded arrays.
        let last_tickers: RefCell<Option<TickerBatch>> = RefCell::new(None);
        let last_markets: RefCell<Option<MarketBatch>> = RefCell::new(None);
        let weak = Rc::downgrade(self);
        let cleanup = self.source.subscribe(Rc::new(move |data| {
            let Some(provider) = weak.upgrade() else {
                return Ok(());
            };
            if provider.is_disposed.get() {
                return Ok(());
            }
            let has_tickers = !data.tickers.borrow().is_empty();
            let has_markets = !data.markets.borrow().is_empty();
            if !has_tickers && !has_markets {
                return Ok(());
            }
            provider.note_data_observed();
            let tickers_changed = has_tickers
                && last_tickers.borrow().as_ref().is_none_or(|last| {
                    !Rc::ptr_eq(last, &data.tickers)
                        || last.borrow().len() != data.tickers.borrow().len()
                });
            let markets_changed = has_markets
                && last_markets.borrow().as_ref().is_none_or(|last| {
                    !Rc::ptr_eq(last, &data.markets)
                        || last.borrow().len() != data.markets.borrow().len()
                });
            if !tickers_changed && !markets_changed {
                return Ok(());
            }
            if tickers_changed {
                *last_tickers.borrow_mut() = Some(data.tickers.clone());
            }
            if markets_changed {
                *last_markets.borrow_mut() = Some(data.markets.clone());
            }
            let markets = if markets_changed {
                Some(provider.parent.adapt_markets(data.markets)?)
            } else {
                None
            };
            let mut loaded = provider.parent.loaded();
            if tickers_changed {
                loaded.tickers = true;
            }
            if markets_changed {
                loaded.markets = true;
            }
            provider.parent.update(PublicDataUpdate {
                tickers: tickers_changed.then_some(data.tickers),
                markets,
                loaded: Some(loaded),
            })
        }))?;
        *self.unsubscribe_store.borrow_mut() = Some(cleanup);
        self.log(
            "Connected to in-process public data source",
            LogSeverity::Info,
        )
    }
    pub fn subscribe_orderbook(
        &self,
        symbol: String,
        callback: OrderbookObserver,
    ) -> Result<PublicDataCleanup, ExchangeError> {
        self.source.listen_order_book(symbol, callback)
    }
    pub fn subscribe_candle(
        &self,
        options: OHLCVOptions,
        callback: CandleObserver,
    ) -> Result<PublicDataCleanup, ExchangeError> {
        self.source.listen_ohlcv(options, callback)
    }
    pub fn is_connected(&self) -> bool {
        !self.is_disposed.get() && self.unsubscribe_store.borrow().is_some()
    }
    pub fn is_initialized(&self) -> bool {
        self.initialized.get()
    }
    pub fn is_healthy(&self) -> bool {
        if !self.is_connected() || self.last_received.get() == 0.0 {
            return false;
        }
        self.parent.now_ms() - self.last_received.get()
            < EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS as f64
    }
    pub fn last_message_timestamp(&self) -> Option<f64> {
        (self.last_received.get() > 0.0).then(|| self.last_received.get())
    }
    fn note_data_observed(&self) {
        self.initialized.set(true);
        self.last_received.set(self.parent.now_ms());
    }
    fn unsubscribe_store(&self) -> Result<(), ExchangeError> {
        let cleanup = self.unsubscribe_store.borrow().clone();
        if let Some(cleanup) = cleanup {
            cleanup()?;
            *self.unsubscribe_store.borrow_mut() = None;
        }
        Ok(())
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.is_disposed.set(true);
        self.unsubscribe_store()?;
        self.log(
            "Disposed in-process public data provider",
            LogSeverity::Info,
        )
    }
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.parent.emit_log(
            format!("[{}:InProcess] {message}", self.parent.name()),
            severity,
        )
    }
}
