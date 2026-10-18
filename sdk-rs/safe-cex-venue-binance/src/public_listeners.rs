//! Public listener bodies at current bound timer/send/HTTP/book-helper operations.
//! Dense ARRAY parser envelopes, native event graphs, ASCII symbols and numeric
//! topic counts; data properties only. Buffer element Rc identity is preserved;
//! helper input ARRAY-container identity remains outside the supplied boundary.
//! Concrete BaseWebSocket/dispatch/class wiring, response getters, host scheduling,
//! discarded-job ownership and drop/unwind cleanup remain separate integrations.
use crate::{
    orderbook::BinanceOrderBookSyncState,
    private_event_value::{EventOps, EventValue, number, string},
    protocol_types::BinanceOrderBookApplyResult,
    request_weight::{Failure, type_error},
};
use safe_cex_types::{Candle, OrderBook, object_map::JsObjectMap};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
};
pub type Parser = Rc<dyn Fn(EventValue) -> Result<(), Failure>>;
pub type Action = Rc<dyn Fn() -> Result<(), Failure>>;
pub type Unsubscribe = Action;
pub type Book = Rc<RefCell<OrderBook>>;
pub type SyncState = Rc<RefCell<BinanceOrderBookSyncState>>;
pub type Snapshot = Pin<Box<dyn Future<Output = Result<EventValue, Failure>>>>;
pub type Job = Pin<Box<dyn Future<Output = Result<(), Failure>>>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Venue {
    Futures,
    PM,
    Spot,
}
pub struct InnerState {
    pub updates: RefCell<Vec<EventValue>>,
    pub snapshot_loaded: Cell<bool>,
}
impl Default for InnerState {
    fn default() -> Self {
        Self {
            updates: Default::default(),
            snapshot_loaded: Cell::new(false),
        }
    }
}
pub struct ListenerState {
    pub disposed: Cell<bool>,
    pub connected: Cell<bool>,
    pub handlers: RefCell<JsObjectMap<Vec<Parser>>>,
    pub topics: RefCell<JsObjectMap<f64>>,
    pub inner: RefCell<JsObjectMap<Rc<InnerState>>>,
    pub next_subscription_id: Cell<f64>,
}
impl Default for ListenerState {
    fn default() -> Self {
        Self {
            disposed: Cell::new(false),
            connected: Cell::new(false),
            handlers: Default::default(),
            topics: Default::default(),
            inner: Default::default(),
            next_subscription_id: Cell::new(2.0),
        }
    }
}
/// Each method resolves the CURRENT bound operation when invoked. `get_snapshot`
/// starts HTTP synchronously and yields already-decoded response.data on settlement.
/// spawn owns/polls the discarded async job; synchronous GET throws become job Err.
pub trait ListenerHost {
    fn state(&self) -> &ListenerState;
    fn disposed(&self) -> bool {
        self.state().disposed.get()
    }
    fn connected(&self) -> bool {
        self.state().connected.get()
    }
    fn handlers(&self, key: &str) -> Option<Vec<Parser>> {
        self.state().handlers.borrow().get(key).cloned()
    }
    fn replace_handlers(&self, key: String, parsers: Vec<Parser>) {
        self.state().handlers.borrow_mut().insert(key, parsers);
    }
    fn delete_handlers(&self, key: &str) {
        self.state().handlers.borrow_mut().remove(key);
    }
    fn send(&self, payload: EventValue) -> Result<(), Failure>;
    fn set_timeout(&self, callback: Action, delay: f64) -> Result<EventValue, Failure>;
    fn clear_timeout(&self, ticket: EventValue) -> Result<(), Failure>;
    fn cleanup(&self, handler: &str) -> Result<(), Failure>;
    fn nanoid(&self) -> Result<String, Failure>;
    fn get_snapshot(&self, venue: Venue, symbol: &str, limit: f64) -> Result<Snapshot, Failure>;
    fn spawn(&self, job: Job);
    fn reset_sync(&self, sync: SyncState) -> Result<(), Failure>;
    fn load(&self, book: Book, data: EventValue, sync: SyncState) -> Result<(), Failure>;
    fn buffered(
        &self,
        updates: Vec<EventValue>,
        last_id: EventValue,
    ) -> Result<Vec<EventValue>, Failure>;
    fn apply(
        &self,
        book: Book,
        data: EventValue,
        sync: SyncState,
        require_previous: bool,
    ) -> Result<BinanceOrderBookApplyResult, Failure>;
    fn total(&self, book: Book) -> Result<(), Failure>;
}
fn payload(method: &str, topic: &str, id: f64) -> EventValue {
    EventValue::record_object(
        [
            ("method".into(), string(method)),
            ("params".into(), EventValue::array(vec![string(topic)])),
            ("id".into(), number(id)),
        ]
        .into_iter()
        .collect(),
    )
}
fn add_handler(host: &dyn ListenerHost, handler: &str, parser: Parser) {
    let mut next = host.handlers(handler).unwrap_or_default();
    next.push(parser);
    host.replace_handlers(handler.into(), next);
}
fn remove_handler(
    host: &dyn ListenerHost,
    handler: &str,
    parser: &Parser,
    delete_empty: bool,
) -> usize {
    let next: Vec<_> = host
        .handlers(handler)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !Rc::ptr_eq(p, parser))
        .collect();
    let n = next.len();
    if delete_empty && n == 0 {
        host.delete_handlers(handler)
    } else {
        host.replace_handlers(handler.into(), next)
    }
    n
}
fn topic_add(host: &dyn ListenerHost, topic: &str) {
    let mut map = host.state().topics.borrow_mut();
    let n = map
        .get(topic)
        .copied()
        .filter(|n| *n != 0.0 && !n.is_nan())
        .unwrap_or(0.0)
        + 1.0;
    map.insert(topic.into(), n);
}
fn legacy_topic_remove(host: &dyn ListenerHost, topic: &str) {
    let mut map = host.state().topics.borrow_mut();
    if let Some(n) = map.get(topic).copied() {
        if n <= 1.0 {
            map.remove(topic);
        } else {
            map.insert(topic.into(), n - 1.0);
        }
    }
}
fn lower_symbol(symbol: &EventValue) -> Result<String, Failure> {
    match symbol {
        EventValue::String(s) => Ok(EventValue::String(s.clone()).text()?.to_lowercase()),
        v if v.nullish() => Err(type_error(format!(
            "Cannot read properties of {} (reading 'toLowerCase')",
            v.text()?
        ))),
        _ => Err(type_error("opts.symbol.toLowerCase is not a function")),
    }
}
fn next_id(host: &dyn ListenerHost) -> f64 {
    let s = host.state();
    let id = s.next_subscription_id.get();
    s.next_subscription_id.set(id + 1.0);
    id
}
struct CandleListener {
    host: Rc<dyn ListenerHost>,
    parser: Parser,
    topic: String,
    handler: String,
    guarded: bool,
    active: Cell<bool>,
    unsubscribed: Cell<bool>,
    timer: RefCell<Option<EventValue>>,
}
impl CandleListener {
    fn wait(self: &Rc<Self>) -> Result<(), Failure> {
        if self.guarded && (!self.active.get() || self.host.disposed()) {
            return Ok(());
        }
        if self.host.connected() {
            add_handler(self.host.as_ref(), &self.handler, self.parser.clone());
            topic_add(self.host.as_ref(), &self.topic);
            let id = if self.guarded {
                next_id(self.host.as_ref())
            } else {
                2.0
            };
            self.host.send(payload("SUBSCRIBE", &self.topic, id))?;
        } else {
            let this = self.clone();
            let ticket = self.host.set_timeout(Rc::new(move || this.wait()), 100.0)?;
            if self.guarded {
                *self.timer.borrow_mut() = Some(ticket);
            }
        }
        Ok(())
    }
    fn unsubscribe(&self) -> Result<(), Failure> {
        if self.guarded {
            if self.unsubscribed.replace(true) {
                return Ok(());
            }
            self.active.set(false);
            let ticket = self.timer.borrow().clone();
            if let Some(t) = ticket.filter(EventValue::truthy) {
                self.host.clear_timeout(t)?;
                *self.timer.borrow_mut() = None;
            }
        }
        let n = remove_handler(
            self.host.as_ref(),
            &self.handler,
            &self.parser,
            self.guarded,
        );
        if self.guarded {
            let mut topics = self.host.state().topics.borrow_mut();
            let old = topics
                .get(&self.topic)
                .copied()
                .filter(|n| *n != 0.0 && !n.is_nan())
                .unwrap_or(0.0);
            let next = old - 1.0;
            if next > 0.0 {
                topics.insert(self.topic.clone(), next);
            } else {
                topics.remove(&self.topic);
            }
        } else {
            legacy_topic_remove(self.host.as_ref(), &self.topic);
        }
        if self.host.connected() && n == 0 {
            let id = if self.guarded {
                next_id(self.host.as_ref())
            } else {
                2.0
            };
            let p = payload("UNSUBSCRIBE", &self.topic, id);
            self.host.cleanup(&self.handler)?;
            self.host.send(p)?;
        }
        Ok(())
    }
}
fn candles(
    host: Rc<dyn ListenerHost>,
    opts: EventValue,
    callback: Rc<dyn Fn(Candle) -> Result<(), Failure>>,
    guarded: bool,
) -> Result<Unsubscribe, Failure> {
    let topic = format!(
        "{}@kline_{}",
        lower_symbol(&opts.read("symbol")?)?,
        opts.read("interval")?.text()?
    );
    let handler = format!(
        "kline.{}.{}",
        opts.read("interval")?.text()?,
        opts.read("symbol")?.text()?
    );
    let parser: Parser = Rc::new(move |events| {
        let json = events.read("0")?;
        if opts.read("symbol")?.same(&json.read("k")?.read("s")?)
            && json.read("k")?.read("i")?.same(&opts.read("interval")?)
        {
            let k = json.read("k")?;
            callback(Candle {
                timestamp: k.read("t")?.number()? / 1000.0,
                open: k.read("o")?.float()?,
                high: k.read("h")?.float()?,
                low: k.read("l")?.float()?,
                close: k.read("c")?.float()?,
                volume: k.read("v")?.float()?,
                last_side: None,
            })?;
        }
        Ok(())
    });
    let this = Rc::new(CandleListener {
        host,
        parser,
        topic,
        handler,
        guarded,
        active: Cell::new(true),
        unsubscribed: Cell::new(false),
        timer: Default::default(),
    });
    this.wait()?;
    Ok(Rc::new(move || this.unsubscribe()))
}
// ts: BinanceMarketWebsocket.listenOHLCV
pub fn listen_futures_market(
    host: Rc<dyn ListenerHost>,
    opts: EventValue,
    callback: Rc<dyn Fn(Candle) -> Result<(), Failure>>,
) -> Result<Unsubscribe, Failure> {
    candles(host, opts, callback, true)
}
// ts: BinancePMMarketWebsocket.listenOHLCV
pub fn listen_pm_market(
    host: Rc<dyn ListenerHost>,
    opts: EventValue,
    callback: Rc<dyn Fn(Candle) -> Result<(), Failure>>,
) -> Result<Unsubscribe, Failure> {
    candles(host, opts, callback, false)
}
// ts: BinancePublicWebsocket.listenOHLCV (Spot)
pub fn listen_spot_candles(
    host: Rc<dyn ListenerHost>,
    opts: EventValue,
    callback: Rc<dyn Fn(Candle) -> Result<(), Failure>>,
) -> Result<Unsubscribe, Failure> {
    candles(host, opts, callback, false)
}
struct DepthListener {
    host: Rc<dyn ListenerHost>,
    venue: Venue,
    symbol: String,
    topic: String,
    handler: String,
    book: Book,
    inner: Rc<InnerState>,
    sync: SyncState,
    fetching: Cell<bool>,
    timer: RefCell<Option<EventValue>>,
    callback: Rc<dyn Fn(Book) -> Result<(), Failure>>,
}
impl DepthListener {
    fn reset(&self, initial: Option<EventValue>) -> Result<(), Failure> {
        self.book.borrow_mut().asks = [].into();
        self.book.borrow_mut().bids = [].into();
        *self.inner.updates.borrow_mut() = initial.filter(EventValue::truthy).into_iter().collect();
        self.inner.snapshot_loaded.set(false);
        self.host.reset_sync(self.sync.clone())
    }
    fn fetch(self: &Rc<Self>) {
        if self.fetching.replace(true) {
            return;
        }
        let request = self.host.get_snapshot(self.venue, &self.symbol, 1000.0);
        if let Err(error) = request {
            self.fetching.set(false);
            self.host.spawn(Box::pin(async move { Err(error) }));
            return;
        }
        let this = self.clone();
        self.host.spawn(Box::pin(async move {
            let result = async {
                let data = request?.await?;
                if this.host.disposed() {
                    return Ok(());
                }
                this.host
                    .load(this.book.clone(), data.clone(), this.sync.clone())?;
                let updates = this.inner.updates.borrow().clone();
                let buffered = this.host.buffered(updates, data.read("lastUpdateId")?)?;
                this.inner.snapshot_loaded.set(true);
                self::clear_updates(&this.inner);
                for update in buffered {
                    let result = this.host.apply(
                        this.book.clone(),
                        update.clone(),
                        this.sync.clone(),
                        this.venue != Venue::Spot,
                    )?;
                    if result == BinanceOrderBookApplyResult::Gap {
                        this.reset(Some(update))?;
                        let next = this.clone();
                        this.host.set_timeout(
                            Rc::new(move || {
                                next.fetch();
                                Ok(())
                            }),
                            0.0,
                        )?;
                        return Ok(());
                    }
                }
                this.host.total(this.book.clone())?;
                (this.callback)(this.book.clone())?;
                Ok(())
            }
            .await;
            this.fetching.set(false);
            result
        }));
    }
    fn parse(self: &Rc<Self>, events: EventValue) -> Result<(), Failure> {
        let data = events.read("0")?;
        if !data.read("s")?.same(&string(&self.symbol)) {
            return Ok(());
        }
        if !self.inner.snapshot_loaded.get() && self.inner.updates.borrow().is_empty() {
            self.reset(Some(data))?;
            self.fetch();
            return Ok(());
        }
        if !self.inner.snapshot_loaded.get() {
            self.inner.updates.borrow_mut().push(data);
            return Ok(());
        }
        let result = self.host.apply(
            self.book.clone(),
            data.clone(),
            self.sync.clone(),
            self.venue != Venue::Spot,
        )?;
        if result == BinanceOrderBookApplyResult::Gap {
            self.reset(Some(data))?;
            self.fetch();
            return Ok(());
        }
        if result == BinanceOrderBookApplyResult::Ignored {
            return Ok(());
        }
        self.host.total(self.book.clone())?;
        (self.callback)(self.book.clone())?;
        Ok(())
    }
    fn wait(self: &Rc<Self>, parser: Parser) -> Result<(), Failure> {
        if self.host.connected() {
            add_handler(self.host.as_ref(), &self.handler, parser);
            topic_add(self.host.as_ref(), &self.topic);
            self.host.send(payload("SUBSCRIBE", &self.topic, 3.0))?;
        } else {
            let this = self.clone();
            let p = parser.clone();
            let ticket = self
                .host
                .set_timeout(Rc::new(move || this.wait(p.clone())), 100.0)?;
            *self.timer.borrow_mut() = Some(ticket);
        }
        Ok(())
    }
    fn unsubscribe(&self, parser: &Parser) -> Result<(), Failure> {
        remove_handler(self.host.as_ref(), &self.handler, parser, false);
        self.reset(None)?;
        let ticket = self.timer.borrow().clone();
        if let Some(t) = ticket.filter(EventValue::truthy) {
            self.host.clear_timeout(t)?;
            *self.timer.borrow_mut() = None;
        }
        legacy_topic_remove(self.host.as_ref(), &self.topic);
        if self.host.connected() {
            let empty = self
                .host
                .handlers(&self.handler)
                .ok_or_else(|| {
                    type_error("Cannot read properties of undefined (reading 'length')")
                })?
                .is_empty();
            if empty {
                let p = payload("UNSUBSCRIBE", &self.topic, 3.0);
                self.host.cleanup(&self.handler)?;
                self.host.send(p)?;
            }
        }
        Ok(())
    }
}
fn clear_updates(inner: &InnerState) {
    *inner.updates.borrow_mut() = vec![];
}
fn depth(
    host: Rc<dyn ListenerHost>,
    venue: Venue,
    symbol: String,
    callback: Rc<dyn Fn(Book) -> Result<(), Failure>>,
) -> Result<Unsubscribe, Failure> {
    let early_topic = (venue == Venue::Spot).then(|| {
        (
            format!("{}@depth", symbol.to_lowercase()),
            format!("depthUpdate.{symbol}"),
        )
    });
    let id = host.nanoid()?;
    let inner = {
        let mut map = host.state().inner.borrow_mut();
        if !map.contains_key(&id) {
            map.insert(id.clone(), Rc::new(InnerState::default()));
        }
        map.get(&id).expect("inserted inner").clone()
    };
    let (topic, handler) = early_topic.unwrap_or_else(|| {
        (
            format!("{}@depth", symbol.to_lowercase()),
            format!("depthUpdate.{symbol}"),
        )
    });
    let this = Rc::new(DepthListener {
        host,
        venue,
        symbol,
        topic,
        handler,
        book: Rc::new(RefCell::new(OrderBook {
            bids: vec![],
            asks: vec![],
        })),
        inner,
        sync: Default::default(),
        fetching: Cell::new(false),
        timer: Default::default(),
        callback,
    });
    let c = this.clone();
    let parser: Parser = Rc::new(move |events| c.parse(events));
    this.wait(parser.clone())?;
    Ok(Rc::new(move || this.unsubscribe(&parser)))
}
// ts: BinancePublicWebsocket.listenOrderBook (Futures)
pub fn listen_futures_book(
    host: Rc<dyn ListenerHost>,
    symbol: String,
    callback: Rc<dyn Fn(Book) -> Result<(), Failure>>,
) -> Result<Unsubscribe, Failure> {
    depth(host, Venue::Futures, symbol, callback)
}
// ts: BinancePMPublicWebsocket.listenOrderBook
pub fn listen_pm_book(
    host: Rc<dyn ListenerHost>,
    symbol: String,
    callback: Rc<dyn Fn(Book) -> Result<(), Failure>>,
) -> Result<Unsubscribe, Failure> {
    depth(host, Venue::PM, symbol, callback)
}
// ts: BinancePublicWebsocket.listenOrderBook (Spot)
pub fn listen_spot_book(
    host: Rc<dyn ListenerHost>,
    symbol: String,
    callback: Rc<dyn Fn(Book) -> Result<(), Failure>>,
) -> Result<Unsubscribe, Failure> {
    depth(host, Venue::Spot, symbol, callback)
}
/// Constructor body only: super/implicit derived-field initialization is supplied
/// by `super_and_fields`. Whole class/virtual dispatch integration is not claimed.
pub struct PublicParts<B, M> {
    pub base: B,
    pub market_websocket: M,
}
// ts: BinancePublicWebsocket.constructor
pub fn construct_futures_public<P, B, M, E>(
    parent: Rc<P>,
    super_and_fields: impl FnOnce(Rc<P>) -> Result<B, E>,
    market: impl FnOnce(Rc<P>) -> Result<M, E>,
) -> Result<PublicParts<B, M>, E> {
    let base = super_and_fields(parent.clone())?;
    let market_websocket = market(parent)?;
    Ok(PublicParts {
        base,
        market_websocket,
    })
}
// ts: BinancePMPublicWebsocket.constructor
pub fn construct_pm_public<P, B, M, E>(
    parent: Rc<P>,
    super_and_fields: impl FnOnce(Rc<P>) -> Result<B, E>,
    market: impl FnOnce(Rc<P>) -> Result<M, E>,
) -> Result<PublicParts<B, M>, E> {
    let base = super_and_fields(parent.clone())?;
    let market_websocket = market(parent)?;
    Ok(PublicParts {
        base,
        market_websocket,
    })
}
