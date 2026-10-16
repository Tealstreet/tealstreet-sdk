//! Public feed routing and listeners against bound JS data, Store and timer operations.
use crate::public_lifecycle::MarketIdentity;
use indexmap::IndexMap;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Candle, Market, OrderBook, OrderBookOrders, Position};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub type FeedHandler<V> = Rc<dyn Fn(V) -> Result<(), ExchangeError>>;
pub enum HandlerSlot<V> {
    Hole,
    Undefined,
    Callback(FeedHandler<V>),
}
impl<V> Clone for HandlerSlot<V> {
    fn clone(&self) -> Self {
        match self {
            Self::Hole => Self::Hole,
            Self::Undefined => Self::Undefined,
            Self::Callback(f) => Self::Callback(f.clone()),
        }
    }
}
pub type HandlerArray<V> = Rc<RefCell<Vec<HandlerSlot<V>>>>;
pub type FeedCleanup = Rc<dyn Fn() -> Result<(), ExchangeError>>;
pub type MarketRef = Rc<RefCell<Market>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedKind {
    Book,
    Candle,
    Ticker,
}

pub struct FeedRegistry<V> {
    pub handlers: RefCell<IndexMap<String, HandlerArray<V>>>,
    pub topics: RefCell<IndexMap<String, f64>>,
}
impl<V> Default for FeedRegistry<V> {
    fn default() -> Self {
        Self {
            handlers: RefCell::new(IndexMap::new()),
            topics: RefCell::new(IndexMap::new()),
        }
    }
}
impl<V> FeedRegistry<V> {
    fn append(&self, topic: &str, parser: FeedHandler<V>) {
        let mut values: Vec<HandlerSlot<V>> = self
            .handlers
            .borrow()
            .get(topic)
            .map(|v| {
                v.borrow()
                    .iter()
                    .map(|slot| match slot {
                        HandlerSlot::Hole => HandlerSlot::Undefined,
                        other => other.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        values.push(HandlerSlot::Callback(parser));
        self.handlers
            .borrow_mut()
            .insert(topic.into(), Rc::new(RefCell::new(values)));
        let previous = self.topics.borrow().get(topic).copied().unwrap_or(0.0);
        let previous = if previous == 0.0 || previous.is_nan() {
            0.0
        } else {
            previous
        };
        self.topics
            .borrow_mut()
            .insert(topic.into(), previous + 1.0);
    }
    fn remove(&self, topic: &str, parser: &FeedHandler<V>) {
        let values = self
            .handlers
            .borrow()
            .get(topic)
            .map(|v| {
                v.borrow()
                    .iter()
                    .filter(|slot| match slot {
                        HandlerSlot::Hole => false,
                        HandlerSlot::Undefined => true,
                        HandlerSlot::Callback(f) => !Rc::ptr_eq(f, parser),
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        self.handlers
            .borrow_mut()
            .insert(topic.into(), Rc::new(RefCell::new(values)));
        let count = self.topics.borrow().get(topic).copied().unwrap_or(f64::NAN);
        self.topics.borrow_mut().insert(topic.into(), count - 1.0);
    }
}

#[derive(Clone)]
pub struct TickerChanges<V> {
    pub symbol: Option<V>,
    pub info: Option<V>,
    pub mark: Option<f64>,
    pub index: Option<f64>,
    pub last: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub funding_rate: Option<f64>,
    pub open_interest: Option<f64>,
    pub volume: Option<f64>,
    pub quote_volume: Option<f64>,
    pub percentage: Option<f64>,
    pub funding_time: Option<f64>,
}
impl<V> Default for TickerChanges<V> {
    fn default() -> Self {
        Self {
            symbol: None,
            info: None,
            mark: None,
            index: None,
            last: None,
            bid: None,
            ask: None,
            funding_rate: None,
            open_interest: None,
            volume: None,
            quote_volume: None,
            percentage: None,
            funding_time: None,
        }
    }
}

/// Value handles retain source identity; property/coercion hooks bind the existing JS runtime ABI.
pub trait PublicFeedHost: 'static {
    type Value: Clone + 'static;
    fn property(&self, value: &Self::Value, key: &str) -> Result<Self::Value, ExchangeError>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn undefined(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn string_type(&self, value: &Self::Value) -> bool;
    fn number_type(&self, value: &Self::Value) -> bool;
    fn string(&self, value: &Self::Value) -> Result<String, ExchangeError>;
    fn number(&self, value: &Self::Value) -> Result<f64, ExchangeError>;
    fn float(&self, value: &Self::Value) -> Result<f64, ExchangeError>;
    fn integer(&self, value: &Self::Value) -> Result<f64, ExchangeError>;
    fn string_value(&self, value: &str) -> Self::Value;
    fn split(&self, value: &Self::Value, separator: &str) -> Result<Vec<String>, ExchangeError>;
    /// Invoke the captured callee with its receiver and a retainable native projector.
    /// Preserve the runtime map operation, arguments, holes and arbitrary return value.
    fn call_map(
        &self,
        method: Self::Value,
        receiver: Self::Value,
        projector: LevelProjector<Self::Value>,
    ) -> Result<Self::Value, ExchangeError>;
    /// Perform Number < opaque value, including a fresh relational coercion on every call.
    fn index_less_than(&self, index: f64, limit: &Self::Value) -> Result<bool, ExchangeError>;
    /// Construct ordered own data fields without narrowing nonfinite or signed-zero Numbers.
    fn level_value(&self, price: f64, amount: f64, total: f64) -> Self::Value;
    /// Admit dense own-data numeric levels without executing getters or Proxy operations.
    /// Unsupported output visibly refuses the whole callback after both map invocations.
    fn book_levels(&self, value: Self::Value) -> Result<Vec<OrderBookOrders>, ExchangeError>;
    fn empty_array(&self) -> Self::Value;
    fn json_parse(&self, data: &str) -> Result<Option<Self::Value>, ExchangeError>;
    fn disposed(&self) -> bool;
    fn connected(&self) -> bool;
    fn track_received(&self) -> Result<(), ExchangeError>;
    fn dispatch(&self, kind: FeedKind, message: Self::Value) -> Result<(), ExchangeError>;
    fn emit_error(&self, message: &str) -> Result<(), ExchangeError>;
    fn registry(&self) -> &FeedRegistry<Self::Value>;
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError>;
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError>;
    fn market_by_instrument(
        &self,
        instrument: &Self::Value,
    ) -> Result<Option<Self::Value>, ExchangeError>;
    fn market_by_symbol(&self, symbol: &str) -> Result<Option<MarketRef>, ExchangeError>;
    fn current_ticker(&self, id: &Self::Value) -> Result<Option<Self::Value>, ExchangeError>;
    fn positions(&self) -> Result<Rc<RefCell<Vec<Option<Rc<RefCell<Position>>>>>>, ExchangeError>;
    fn update_ticker(
        &self,
        id: &Self::Value,
        changes: &TickerChanges<Self::Value>,
    ) -> Result<(), ExchangeError>;
    fn sync_upnl(&self) -> Result<(), ExchangeError>;
    fn map_interval(&self, interval: &str) -> Result<String, ExchangeError>;
    fn subscribe(
        &self,
        kind: FeedKind,
        instrument: &str,
        interval: Option<&str>,
    ) -> Result<f64, ExchangeError>;
    fn unsubscribe(&self, kind: FeedKind, id: f64) -> Result<(), ExchangeError>;
    fn schedule(&self, callback: FeedCleanup, delay: f64) -> Result<(), ExchangeError>;
}
fn or<H: PublicFeedHost>(
    host: &H,
    value: &H::Value,
    lite: &str,
    full: &str,
) -> Result<H::Value, ExchangeError> {
    let first = host.property(value, lite)?;
    if host.truthy(&first) {
        Ok(first)
    } else {
        host.property(value, full)
    }
}
fn nullish<H: PublicFeedHost>(
    host: &H,
    value: &H::Value,
    lite: &str,
    full: &str,
) -> Result<H::Value, ExchangeError> {
    let first = host.property(value, lite)?;
    if host.nullish(&first) {
        host.property(value, full)
    } else {
        Ok(first)
    }
}
fn float_or_zero<H: PublicFeedHost>(
    host: &H,
    value: &H::Value,
    lite: &str,
    full: &str,
) -> Result<f64, ExchangeError> {
    let value = or(host, value, lite, full)?;
    host.float(&if host.truthy(&value) {
        value
    } else {
        host.string_value("0")
    })
}
fn is_string<H: PublicFeedHost>(
    host: &H,
    value: &H::Value,
    expected: &str,
) -> Result<bool, ExchangeError> {
    Ok(host.string_type(value) && host.string(value)? == expected)
}

/// ts: GrvtPublicWebSocket.onMessage. Tracking is outside the parse/dispatch catch.
pub fn on_message<H: PublicFeedHost>(host: &H, data: &str) -> Result<(), ExchangeError> {
    if host.disposed() {
        return Ok(());
    }
    host.track_received()?;
    let result: Result<(), ExchangeError> = (|| {
        let Some(json) = host.json_parse(data)? else {
            return Ok(());
        };
        if !host.truthy(&json) {
            return Ok(());
        }
        let result = host.property(&json, "result")?;
        if host.truthy(&result) && host.truthy(&host.property(&json, "id")?) {
            return Ok(());
        }
        if host.truthy(&host.property(&json, "error")?) {
            let code = host.property(&host.property(&json, "error")?, "code")?;
            if host.number_type(&code) && host.number(&code)? == 1100.0 {
                return Ok(());
            }
            let code = host.property(&host.property(&json, "error")?, "code")?;
            let code = if host.number_type(&code) {
                host.string(&host.property(&host.property(&json, "error")?, "code")?)?
            } else {
                "unknown".into()
            };
            host.emit_error(&format!("WebSocket error (code={code})"))?;
            return Ok(());
        }
        let stream = or(host, &json, "s", "stream")?;
        if host.truthy(&stream) {
            if is_string(host, &stream, "v1.book.s")? {
                host.dispatch(FeedKind::Book, json)?;
            } else if is_string(host, &stream, "v1.candle")? {
                host.dispatch(FeedKind::Candle, json)?;
            } else if is_string(host, &stream, "v1.ticker.s")?
                || is_string(host, &stream, "v1.ticker.d")?
            {
                host.dispatch(FeedKind::Ticker, json)?;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        host.emit_error(&format!(
            "Error parsing WebSocket message: {}",
            error.js_error_string()
        ))?;
    }
    Ok(())
}
fn dispatch_handlers<H: PublicFeedHost>(
    host: &H,
    topic: &str,
    feed: H::Value,
) -> Result<(), ExchangeError> {
    let handlers = host.registry().handlers.borrow().get(topic).cloned();
    if let Some(handlers) = handlers
        && !handlers.borrow().is_empty()
    {
        host.record_stats(topic)?;
        let length = handlers.borrow().len();
        for index in 0..length {
            let slot = handlers
                .borrow()
                .get(index)
                .cloned()
                .unwrap_or(HandlerSlot::Hole);
            match slot {
                HandlerSlot::Callback(callback) => callback(feed.clone())?,
                HandlerSlot::Hole => {}
                HandlerSlot::Undefined => {
                    let mut error = ExchangeError::new("handler is not a function");
                    error.name = "TypeError".into();
                    return Err(error);
                }
            }
        }
    }
    Ok(())
}

/// ts: GrvtPublicWebSocket#handleKlineUpdate. forEach retains the selected live handler array.
pub fn handle_kline_update<H: PublicFeedHost>(
    host: &H,
    message: H::Value,
) -> Result<(), ExchangeError> {
    let result: Result<(), ExchangeError> = (|| {
        let feed = or(host, &message, "f", "feed")?;
        if !host.truthy(&feed) {
            return Ok(());
        }
        let instrument = or(host, &feed, "i", "instrument")?;
        if !host.truthy(&instrument) {
            return Ok(());
        }
        let selector = or(host, &message, "s1", "selector")?;
        if !host.truthy(&selector) {
            return Ok(());
        }
        let parts = host.split(&selector, "@")?;
        if parts.len() != 2 {
            return Ok(());
        }
        let interval = parts[1]
            .split('-')
            .next()
            .expect("split always returns a part");
        let topic = format!("kline.{interval}.{}", host.string(&instrument)?);
        dispatch_handlers(host, &topic, feed)
    })();
    if let Err(error) = result {
        host.emit_error(&format!(
            "Error handling kline update: {}",
            error.js_error_string()
        ))?;
    }
    Ok(())
}

/// ts: GrvtPublicWebSocket#handleOrderbookUpdate.
pub fn handle_orderbook_update<H: PublicFeedHost>(
    host: &H,
    message: H::Value,
) -> Result<(), ExchangeError> {
    let result: Result<(), ExchangeError> = (|| {
        let feed = or(host, &message, "f", "feed")?;
        if !host.truthy(&feed) {
            return Ok(());
        }
        let instrument = or(host, &feed, "i", "instrument")?;
        if !host.truthy(&instrument) {
            return Ok(());
        }
        dispatch_handlers(
            host,
            &format!("orderbook.{}", host.string(&instrument)?),
            feed,
        )
    })();
    if let Err(error) = result {
        host.emit_error(&format!(
            "Error handling orderbook update: {}",
            error.js_error_string()
        ))?;
    }
    Ok(())
}

/// ts: GrvtPublicWebSocket#handleTickerUpdate. Delta presence is distinct from truthiness.
pub fn handle_ticker_update<H: PublicFeedHost>(
    host: &H,
    message: H::Value,
) -> Result<(), ExchangeError> {
    let result: Result<(), ExchangeError> = (|| {
        let feed = or(host, &message, "f", "feed")?;
        if !host.truthy(&feed) {
            return Ok(());
        }
        let instrument = or(host, &feed, "i", "instrument")?;
        if !host.truthy(&instrument) {
            return Ok(());
        }
        let Some(market) = host.market_by_instrument(&instrument)? else {
            return Ok(());
        };
        if !host.truthy(&market) {
            return Ok(());
        }
        let mut changes = TickerChanges {
            symbol: Some(host.property(&market, "symbol")?),
            info: Some(feed.clone()),
            ..Default::default()
        };
        for (lite, full, slot) in [
            ("mp", "mark_price", &mut changes.mark),
            ("ip", "index_price", &mut changes.index),
            ("lp", "last_price", &mut changes.last),
            ("bb", "best_bid_price", &mut changes.bid),
            ("ba", "best_ask_price", &mut changes.ask),
            ("fr2", "funding_rate", &mut changes.funding_rate),
            ("oi", "open_interest", &mut changes.open_interest),
        ] {
            let raw = nullish(host, &feed, lite, full)?;
            if !host.undefined(&raw) {
                let value = host.float(&raw)?;
                *slot = Some(if lite == "fr2" { value / 100.0 } else { value });
            }
        }
        let buy = nullish(host, &feed, "bv", "buy_volume_24h_b")?;
        let sell = nullish(host, &feed, "sv", "sell_volume_24h_b")?;
        if !host.undefined(&buy) || !host.undefined(&sell) {
            let buy = if host.undefined(&buy) {
                0.0
            } else {
                host.float(&buy)?
            };
            let sell = if host.undefined(&sell) {
                0.0
            } else {
                host.float(&sell)?
            };
            changes.volume = Some(buy + sell);
            if let Some(last) = changes.last {
                changes.quote_volume = Some((buy + sell) * last);
            }
        }
        let open = nullish(host, &feed, "op", "open_price")?;
        if !host.undefined(&open)
            && let Some(last) = changes.last
        {
            let open = host.float(&open)?;
            if open > 0.0 {
                changes.percentage = Some(((last - open) / open) * 100.0);
            }
        }
        let funding = nullish(host, &feed, "nf", "next_funding_time")?;
        if !host.undefined(&funding) {
            let milliseconds = if host.string_type(&funding) {
                host.integer(&funding)? / 1000000.0
            } else {
                host.number(&funding)?
            };
            changes.funding_time = Some(milliseconds / 1000.0);
        }
        if changes.last.is_none()
            && let Some(mark) = changes.mark
            && mark > 0.0
        {
            let id = host.property(&market, "id")?;
            let ticker = host
                .current_ticker(&id)?
                .filter(|ticker| !host.nullish(ticker));
            let first = ticker
                .as_ref()
                .map(|ticker| host.property(ticker, "last"))
                .transpose()?;
            let fallback = if first.as_ref().is_none_or(|last| !host.truthy(last)) {
                true
            } else {
                let second =
                    host.property(ticker.as_ref().expect("first value had ticker"), "last")?;
                host.number_type(&second) && host.number(&second)? == 0.0
            };
            if fallback {
                changes.last = Some(mark);
            }
        }
        let id = host.property(&market, "id")?;
        host.update_ticker(&id, &changes)?;
        let positions = host.positions()?;
        let length = positions.borrow().len();
        for index in 0..length {
            let position = positions.borrow().get(index).cloned().flatten();
            if let Some(position) = position {
                let symbol = position.borrow().symbol.clone();
                if !is_string(host, &host.property(&market, "symbol")?, &symbol)?
                    || position.borrow().contracts.abs() <= 0.0
                    || position.borrow().contracts.is_nan()
                {
                    continue;
                }
                host.sync_upnl()?;
                break;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        host.emit_error(&format!(
            "Error handling ticker update: {}",
            error.js_error_string()
        ))?;
    }
    Ok(())
}
fn wait_and_subscribe<H: PublicFeedHost>(
    host: Rc<H>,
    kind: FeedKind,
    instrument: String,
    interval: Option<String>,
    topic: String,
    parser: FeedHandler<H::Value>,
    subscription: Rc<Cell<Option<f64>>>,
) -> Result<(), ExchangeError> {
    if host.connected() {
        host.registry().append(&topic, parser);
        subscription.set(Some(host.subscribe(
            kind,
            &instrument,
            interval.as_deref(),
        )?));
    } else {
        let next = host.clone();
        host.schedule(
            Rc::new(move || {
                wait_and_subscribe(
                    next.clone(),
                    kind,
                    instrument.clone(),
                    interval.clone(),
                    topic.clone(),
                    parser.clone(),
                    subscription.clone(),
                )
            }),
            100.0,
        )?;
    }
    Ok(())
}
fn cleanup<H: PublicFeedHost>(
    host: Rc<H>,
    kind: FeedKind,
    topic: String,
    parser: FeedHandler<H::Value>,
    subscription: Rc<Cell<Option<f64>>>,
) -> FeedCleanup {
    Rc::new(move || {
        host.registry().remove(&topic, &parser);
        if host.connected()
            && host
                .registry()
                .handlers
                .borrow()
                .get(&topic)
                .map(|h| h.borrow().len())
                .unwrap_or(0)
                == 0
        {
            host.cleanup_stats(&topic)?;
            if let Some(id) = subscription.get() {
                host.unsubscribe(kind, id)?;
            }
        }
        Ok(())
    })
}

/// ts: GrvtPublicWebSocket.listenOHLCV. Parser callback runs before candle-driven ticker updates.
pub fn listen_ohlcv<H: PublicFeedHost>(
    host: Rc<H>,
    symbol: &str,
    interval: &str,
    callback: Rc<dyn Fn(Candle) -> Result<(), ExchangeError>>,
) -> Result<FeedCleanup, ExchangeError> {
    let Some(market) = host.market_by_symbol(symbol)? else {
        host.emit_error(&format!("Market with symbol {symbol} not found"))?;
        return Ok(Rc::new(|| Ok(())));
    };
    let instrument = market.borrow().id().to_owned();
    let mapped = host.map_interval(interval)?;
    let topic = format!("kline.{mapped}.{instrument}");
    let parser_host = host.clone();
    let parser_instrument = instrument.clone();
    let parser: FeedHandler<H::Value> = Rc::new(move |feed| {
        let current = or(parser_host.as_ref(), &feed, "i", "instrument")?;
        if is_string(parser_host.as_ref(), &current, &parser_instrument)? {
            let open_time = or(parser_host.as_ref(), &feed, "ot", "open_time")?;
            let timestamp = if parser_host.string_type(&open_time) {
                parser_host.integer(&open_time)? / 1000000000.0
            } else {
                parser_host.number(&open_time)? / 1000.0
            };
            let close = float_or_zero(parser_host.as_ref(), &feed, "c", "close")?;
            let open = float_or_zero(parser_host.as_ref(), &feed, "o", "open")?;
            let high = float_or_zero(parser_host.as_ref(), &feed, "h", "high")?;
            let low = float_or_zero(parser_host.as_ref(), &feed, "l", "low")?;
            let volume = or(parser_host.as_ref(), &feed, "vb", "volume_b")?;
            let volume = if parser_host.truthy(&volume) {
                volume
            } else {
                parser_host.property(&feed, "volume")?
            };
            let volume = parser_host.float(&if parser_host.truthy(&volume) {
                volume
            } else {
                parser_host.string_value("0")
            })?;
            callback(Candle {
                timestamp,
                open,
                high,
                low,
                close,
                volume,
                last_side: None,
            })?;
            if close > 0.0 {
                let id = market.borrow().id().to_owned();
                parser_host.update_ticker(
                    &parser_host.string_value(&id),
                    &TickerChanges {
                        last: Some(close),
                        ..Default::default()
                    },
                )?;
            }
        }
        Ok(())
    });
    let subscription = Rc::new(Cell::new(None));
    wait_and_subscribe(
        host.clone(),
        FeedKind::Candle,
        instrument,
        Some(interval.into()),
        topic.clone(),
        parser.clone(),
        subscription.clone(),
    )?;
    Ok(cleanup(host, FeedKind::Candle, topic, parser, subscription))
}
pub type LevelProjector<V> = Rc<dyn Fn(V, V, V) -> Result<V, ExchangeError>>;

fn map_levels<H: PublicFeedHost>(host: Rc<H>, raw: H::Value) -> Result<H::Value, ExchangeError> {
    let method = host.property(&raw, "map")?;
    let projector_host = host.clone();
    let projector: LevelProjector<H::Value> = Rc::new(move |level, index, array| {
        let price = projector_host.float(&or(projector_host.as_ref(), &level, "p", "price")?)?;
        let amount = projector_host.float(&or(projector_host.as_ref(), &level, "s", "size")?)?;
        let mut total = amount;
        let mut prior_index = 0.0;
        while projector_host.index_less_than(prior_index, &index)? {
            let key = safe_cex_num::js_number_string(prior_index);
            let prior = projector_host.property(&array, &key)?;
            let size = projector_host.property(&prior, "s")?;
            let size = if projector_host.truthy(&size) {
                size
            } else {
                let current = projector_host.property(&array, &key)?;
                projector_host.property(&current, "size")?
            };
            total += projector_host.float(&size)?;
            prior_index += 1.0;
        }
        Ok(projector_host.level_value(price, amount, total))
    });
    host.call_map(method, raw, projector)
}

/// ts: GrvtPublicWebSocket.listenOrderBook. Cumulative totals start at the current level.
pub fn listen_order_book<H: PublicFeedHost>(
    host: Rc<H>,
    symbol: &str,
    callback: Rc<dyn Fn(OrderBook) -> Result<(), ExchangeError>>,
) -> Result<FeedCleanup, ExchangeError> {
    let Some(market) = host.market_by_symbol(symbol)? else {
        host.emit_error(&format!("Market with symbol {symbol} not found"))?;
        return Ok(Rc::new(|| Ok(())));
    };
    let instrument = market.borrow().id().to_owned();
    let topic = format!("orderbook.{instrument}");
    let parser_host = host.clone();
    let parser_instrument = instrument.clone();
    let parser: FeedHandler<H::Value> = Rc::new(move |feed| {
        let current = or(parser_host.as_ref(), &feed, "i", "instrument")?;
        if is_string(parser_host.as_ref(), &current, &parser_instrument)? {
            let bids = or(parser_host.as_ref(), &feed, "b", "bids")?;
            let asks = or(parser_host.as_ref(), &feed, "a", "asks")?;
            let bids = if parser_host.truthy(&bids) {
                bids
            } else {
                parser_host.empty_array()
            };
            let asks = if parser_host.truthy(&asks) {
                asks
            } else {
                parser_host.empty_array()
            };
            let bids = map_levels(parser_host.clone(), bids)?;
            let asks = map_levels(parser_host.clone(), asks)?;
            let bids = parser_host.book_levels(bids)?;
            let asks = parser_host.book_levels(asks)?;
            callback(OrderBook { bids, asks })?;
        }
        Ok(())
    });
    let subscription = Rc::new(Cell::new(None));
    wait_and_subscribe(
        host.clone(),
        FeedKind::Book,
        instrument,
        None,
        topic.clone(),
        parser.clone(),
        subscription.clone(),
    )?;
    Ok(cleanup(host, FeedKind::Book, topic, parser, subscription))
}
