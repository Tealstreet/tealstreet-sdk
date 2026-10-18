//! DXtrade's public "websocket" is a polling handler table. No wire socket is
//! opened by these methods. The inherited socket lifecycle remains a host binding.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{
    AnyTimeframe, Candle, ExtendedTimeframe, OHLCVOptions, OrderBook, OrderBookOrders,
    SecondsTimeframe, Ticker, Timeframe,
};
use std::{cell::RefCell, rc::Rc};

pub type TickerHandler = Rc<dyn Fn(&Ticker) -> Result<(), ExchangeError>>;
pub type HandlerList = Rc<RefCell<Vec<TickerHandler>>>;
pub type HandlerTable = Rc<RefCell<Vec<(String, HandlerList)>>>;
pub type Unsubscribe = Rc<dyn Fn() -> Result<(), ExchangeError>>;
pub type CandleCallback = Rc<dyn Fn(Rc<RefCell<Candle>>) -> Result<(), ExchangeError>>;
pub type BookCallback = Rc<dyn Fn(Rc<RefCell<OrderBook>>) -> Result<(), ExchangeError>>;

/// Required inherited BaseWebSocket operations, never default-success hooks.
/// A native adapter binds cleanup/dispose to the real shared socket and exposes
/// this same typed handler table for its statistics reconciliation.
pub trait PollingAncestor {
    fn now_ms(&self) -> f64;
    fn cleanup_message_handler_stats(&self, topic: &str) -> Result<(), ExchangeError>;
    fn dispose(&self) -> Result<(), ExchangeError>;
}
pub struct BreakoutpropPublicWebsocket {
    pub ancestor: Rc<dyn PollingAncestor>,
    pub message_handlers: HandlerTable,
}
impl BreakoutpropPublicWebsocket {
    pub fn new(
        construct_ancestor: impl FnOnce(HandlerTable) -> Result<Rc<dyn PollingAncestor>, ExchangeError>,
    ) -> Result<Rc<Self>, ExchangeError> {
        let message_handlers: HandlerTable = Rc::default();
        let ancestor = construct_ancestor(message_handlers.clone())?;
        Ok(Rc::new(Self {
            ancestor,
            message_handlers,
        }))
    }
    pub fn handlers(&self, topic: &str) -> Option<HandlerList> {
        self.message_handlers
            .borrow()
            .iter()
            .find(|(key, _)| key == topic)
            .map(|(_, list)| list.clone())
    }
    fn replace(&self, topic: String, callbacks: Vec<TickerHandler>) {
        let list = Rc::new(RefCell::new(callbacks));
        let mut table = self.message_handlers.borrow_mut();
        if let Some((_, old)) = table.iter_mut().find(|(key, _)| key == &topic) {
            *old = list;
        } else {
            table.push((topic, list));
        }
    }
    fn register(self: &Rc<Self>, topic: String, parser: TickerHandler) -> Unsubscribe {
        let mut callbacks = self
            .handlers(&topic)
            .map(|list| list.borrow().clone())
            .unwrap_or_default();
        callbacks.push(parser.clone());
        self.replace(topic.clone(), callbacks);
        let socket = self.clone();
        Rc::new(move || {
            let callbacks: Vec<TickerHandler> = socket
                .handlers(&topic)
                .map(|list| {
                    list.borrow()
                        .iter()
                        .filter(|f| !Rc::ptr_eq(f, &parser))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let empty = callbacks.is_empty();
            socket.replace(topic.clone(), callbacks);
            if empty {
                socket.ancestor.cleanup_message_handler_stats(&topic)?;
            }
            Ok(())
        })
    }
    // ts: listenOHLCV. Destructure once; allocate a fresh candle on every tick.
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        opts: OHLCVOptions,
        callback: CandleCallback,
    ) -> Unsubscribe {
        let seconds = match opts.interval {
            AnyTimeframe::Variant1(Timeframe::M1) => 60.,
            AnyTimeframe::Variant1(Timeframe::M3) => 180.,
            AnyTimeframe::Variant1(Timeframe::M5) => 300.,
            AnyTimeframe::Variant1(Timeframe::M15) => 900.,
            AnyTimeframe::Variant1(Timeframe::M30) => 1800.,
            AnyTimeframe::Variant1(Timeframe::H1) => 3600.,
            AnyTimeframe::Variant1(Timeframe::H2) => 7200.,
            AnyTimeframe::Variant1(Timeframe::H4) => 14400.,
            AnyTimeframe::Variant1(Timeframe::H6) => 21600.,
            AnyTimeframe::Variant1(Timeframe::H12) => 43200.,
            AnyTimeframe::Variant1(Timeframe::D1) => 86400.,
            AnyTimeframe::Variant1(Timeframe::W1) => 604800.,
            AnyTimeframe::Variant0(SecondsTimeframe::S1) => 1.,
            AnyTimeframe::Variant0(SecondsTimeframe::S5) => 5.,
            AnyTimeframe::Variant0(SecondsTimeframe::S15) => 15.,
            AnyTimeframe::Variant0(SecondsTimeframe::S30) => 30.,
            AnyTimeframe::Variant2(ExtendedTimeframe::H3) => 10800.,
            AnyTimeframe::Variant2(ExtendedTimeframe::H5) => 18000.,
            AnyTimeframe::Variant2(ExtendedTimeframe::H7) => 25200.,
            AnyTimeframe::Variant2(ExtendedTimeframe::H8) => 28800.,
            AnyTimeframe::Variant2(ExtendedTimeframe::H9) => 32400.,
            AnyTimeframe::Variant2(ExtendedTimeframe::H10) => 36000.,
            AnyTimeframe::Variant2(ExtendedTimeframe::H11) => 39600.,
            AnyTimeframe::Variant2(ExtendedTimeframe::H24) => 86400.,
            AnyTimeframe::Variant2(ExtendedTimeframe::D2) => 172800.,
            AnyTimeframe::Variant2(ExtendedTimeframe::D3) => 259200.,
            AnyTimeframe::Variant2(ExtendedTimeframe::MN1) => 2592000.,
        };
        let ancestor = self.ancestor.clone();
        let parser: TickerHandler = Rc::new(move |ticker| {
            let candle = Candle {
                timestamp: (ancestor.now_ms() / 1000. / seconds).floor() * seconds,
                open: ticker.last,
                high: ticker.last,
                low: ticker.last,
                close: ticker.last,
                volume: 0.,
                last_side: None,
            };
            callback(Rc::new(RefCell::new(candle)))
        });
        self.register(format!("poll_kline.{}", opts.symbol), parser)
    }
    // ts: listenOrderBook. Keep the same book object; replace both two-row arrays.
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: String,
        callback: BookCallback,
    ) -> Unsubscribe {
        let book = Rc::new(RefCell::new(OrderBook {
            bids: vec![],
            asks: vec![],
        }));
        let parser: TickerHandler = Rc::new(move |ticker| {
            let level = |price| OrderBookOrders {
                price,
                amount: 1.,
                total: 1.,
            };
            {
                let mut book = book.borrow_mut();
                book.bids = vec![level(ticker.bid), level(ticker.bid)];
                book.asks = vec![level(ticker.ask), level(ticker.ask)];
            }
            callback(book.clone())
        });
        self.register(format!("poll_book.{symbol}"), parser)
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.ancestor.dispose()
    }
}
