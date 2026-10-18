//! Concrete public socket owners. Core BaseWebSocket owns host I/O/lifecycle;
//! this owner binds venue methods, listeners, topics and its market sibling.
//! Typed Store updates use native f64 patches. JavaScript object mutation and
//! prototype-statistics side effects remain explicit representation differences.
use crate::{
    private_event_value::{EventOps, EventValue},
    public_dispatch::{CoreInvoke, Handler, Handlers, dispatch_binance_message},
    public_listeners::{
        Action, Book, Job, ListenerHost, ListenerState, Parser, Snapshot, SyncState, Unsubscribe,
        Venue, listen_futures_book, listen_futures_market, listen_pm_book, listen_pm_market,
        listen_spot_book, listen_spot_candles,
    },
    request_weight::Failure,
};
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{BaseWebSocket, MessageHandler, WsParent, WsProtocol, WsRuntime},
};
use safe_cex_store::model_patch::ModelPatch;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{Candle, object_map::JsObjectMap};
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicKind {
    FuturesMarket,
    FuturesPublic,
    PMMarket,
    PMPublic,
    SpotPublic,
}
/// Only imported host/helper operations; no venue-method invocation callback.
pub struct PublicDependencies {
    pub parent: Rc<dyn WsParent>,
    pub runtime: Rc<dyn WsRuntime>,
    pub operations: Rc<dyn ListenerHost>,
}
pub struct PublicSocket {
    state: Rc<PublicState>,
    socket: Rc<BaseWebSocket>,
}
impl std::ops::Deref for PublicSocket {
    type Target = Rc<PublicState>;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}
pub struct PublicState {
    pub kind: PublicKind,
    pub parent: Rc<dyn WsParent>,
    pub listeners: ListenerState,
    base: OnceCell<Weak<BaseWebSocket>>,
    pub market_websocket: OnceCell<Rc<PublicSocket>>,
    this: Weak<PublicState>,
    foreign: RefCell<
        Vec<(
            Weak<dyn Fn(&serde_json::Value) -> Result<(), ExchangeError>>,
            Weak<dyn Fn(EventValue) -> Result<(), Failure>>,
        )>,
    >,
    bridges: RefCell<
        Vec<(
            Weak<dyn Fn(EventValue) -> Result<(), Failure>>,
            Weak<dyn Fn(&serde_json::Value) -> Result<(), ExchangeError>>,
        )>,
    >,
    frames: RefCell<Vec<(EventValue, Option<Failure>)>>,
    pub stream_methods: RefCell<JsObjectMap<Parser>>,
    ticker_captures: RefCell<Vec<Rc<crate::ticker_capture::Capture>>>,
    operations: Rc<dyn ListenerHost>,
}
struct Protocol(Rc<PublicState>);
struct FrameGuard<'a>(&'a RefCell<Vec<(EventValue, Option<Failure>)>>);
impl Drop for FrameGuard<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().pop();
    }
}
fn failure(message: impl Into<String>) -> Failure {
    Rc::new(ExchangeError::new(message))
}
fn error(e: Failure) -> ExchangeError {
    (*e).clone()
}
impl PublicSocket {
    pub fn connect_and_subscribe(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.socket.connect_and_subscribe(false)
    }
    pub fn base(&self) -> &Rc<BaseWebSocket> {
        &self.socket
    }
    pub fn new(kind: PublicKind, id: u64, d: PublicDependencies) -> Rc<Self> {
        let owner = Rc::new_cyclic(|this| PublicState {
            kind,
            parent: d.parent.clone(),
            listeners: ListenerState::default(),
            base: OnceCell::new(),
            market_websocket: OnceCell::new(),
            this: this.clone(),
            foreign: Default::default(),
            bridges: Default::default(),
            frames: Default::default(),
            stream_methods: Default::default(),
            ticker_captures: Default::default(),
            operations: d.operations.clone(),
        });
        let protocol = Rc::new(Protocol(owner.clone()));
        let base = BaseWebSocket::new(id, d.parent, d.runtime.clone(), protocol);
        assert!(owner.base.set(Rc::downgrade(&base)).is_ok());
        let defaults: &[&str] = match kind {
            PublicKind::FuturesMarket | PublicKind::PMMarket => &["24hrTicker", "markPriceUpdate"],
            PublicKind::SpotPublic => &["24hrTicker", "bookTicker"],
            _ => &["bookTicker"],
        };
        for topic in defaults {
            let retained = owner.clone();
            let topic = *topic;
            let parser: Parser = Rc::new(move |events| retained.handle_stream(topic, events));
            owner.replace_handlers(topic.into(), vec![parser]);
        }
        let sibling_kind = match kind {
            PublicKind::FuturesPublic => Some(PublicKind::FuturesMarket),
            PublicKind::PMPublic => Some(PublicKind::PMMarket),
            _ => None,
        };
        if let Some(kind) = sibling_kind {
            assert!(
                owner
                    .market_websocket
                    .set(Self::new(
                        kind,
                        id + 1,
                        PublicDependencies {
                            parent: owner.parent.clone(),
                            runtime: d.runtime,
                            operations: owner.operations.clone()
                        }
                    ))
                    .is_ok()
            );
        }
        Rc::new(Self {
            state: owner,
            socket: base,
        })
    }
}
impl PublicState {
    pub fn base(&self) -> Rc<BaseWebSocket> {
        self.base
            .get()
            .expect("installed base")
            .upgrade()
            .expect("live socket lease")
    }
    pub fn connect_and_subscribe(self: &Rc<Self>) -> Result<(), ExchangeError> {
        use crate::public_connect::{
            connect_binance_market, connect_binance_pm_market, connect_binance_pm_public,
            connect_binance_public, connect_binance_spot_public,
        };
        match self.kind {
            PublicKind::FuturesMarket => connect_binance_market(&self.base()),
            PublicKind::PMMarket => connect_binance_pm_market(&self.base()),
            PublicKind::SpotPublic => connect_binance_spot_public(&self.base()),
            PublicKind::FuturesPublic => connect_binance_public(&self.base(), || {
                self.market_websocket.get().unwrap().connect_and_subscribe()
            }),
            PublicKind::PMPublic => connect_binance_pm_public(&self.base(), || {
                self.market_websocket.get().unwrap().connect_and_subscribe()
            }),
        }
    }
    pub fn subscribe(&self) -> Result<(), ExchangeError> {
        let keys = self
            .listeners
            .topics
            .borrow()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        crate::public_subscriptions::subscribe_binance_public(
            keys,
            |topic| self.listeners.topics.borrow().get(topic).copied(),
            |frame| self.base().send(frame),
        )
    }
    pub fn reset_orderbook(&self) {
        for inner in self.listeners.inner.borrow().values() {
            *inner.updates.borrow_mut() = vec![];
            inner.snapshot_loaded.set(false);
        }
    }
    pub fn on_open(&self) -> Result<(), ExchangeError> {
        if self.base().is_disposed() {
            return Ok(());
        }
        match self.kind {
            PublicKind::FuturesMarket | PublicKind::PMMarket => self.base().send(
                r#"{"id":1,"method":"SUBSCRIBE","params":["!ticker@arr","!markPrice@arr@1s"]}"#,
            )?,
            PublicKind::SpotPublic => self
                .base()
                .send(r#"{"id":1,"method":"SUBSCRIBE","params":["!ticker@arr","!bookTicker"]}"#)?,
            _ => {
                if self.parent.options().public_data_only != Some(true) {
                    self.base()
                        .send(r#"{"id":1,"method":"SUBSCRIBE","params":["!bookTicker"]}"#)?
                }
            }
        }
        if matches!(
            self.kind,
            PublicKind::FuturesPublic | PublicKind::PMPublic | PublicKind::SpotPublic
        ) {
            self.reset_orderbook()
        }
        self.subscribe()
    }
    fn bridge(&self, parser: Parser) -> MessageHandler {
        if let Some(core) = self.foreign.borrow().iter().find_map(|(core, native)| {
            native
                .upgrade()
                .filter(|p| Rc::ptr_eq(p, &parser))
                .and_then(|_| core.upgrade())
        }) {
            return core;
        }

        if let Some(found) = self.bridges.borrow().iter().find_map(|(p, core)| {
            p.upgrade()
                .filter(|p| Rc::ptr_eq(p, &parser))
                .and_then(|_| core.upgrade())
        }) {
            return found;
        }
        let owner = self.this.upgrade().expect("live public state");
        let native = parser.clone();
        let core: MessageHandler = Rc::new(move |fallback| {
            let events = owner
                .frames
                .borrow()
                .last()
                .map(|f| f.0.clone())
                .unwrap_or_else(|| crate::private_session::native(fallback.clone()));
            let result = native(events);
            if let Err(e) = &result {
                if let Some(frame) = owner.frames.borrow_mut().last_mut() {
                    frame.1 = Some(e.clone());
                }
            }
            result.map_err(error)
        });
        self.bridges
            .borrow_mut()
            .push((Rc::downgrade(&parser), Rc::downgrade(&core)));
        core
    }
    fn invoke_core(&self, callback: MessageHandler, events: EventValue) -> Result<(), Failure> {
        self.frames.borrow_mut().push((events, None));
        let guard = FrameGuard(&self.frames);
        let is_native = self.bridges.borrow().iter().any(|(_, core)| {
            core.upgrade()
                .is_some_and(|core| Rc::ptr_eq(&core, &callback))
        });
        let foreign = self.frames.borrow().last().unwrap().0.clone();
        let result = if is_native {
            callback(&serde_json::Value::Null)
        } else {
            match core_json(&foreign) {
                Ok(value) => callback(&value),
                Err(e) => {
                    self.frames.borrow_mut().last_mut().unwrap().1 = Some(e.clone());
                    Err(error(e))
                }
            }
        };
        let original = self.frames.borrow().last().and_then(|f| f.1.clone());
        drop(guard);
        result.map_err(|e| original.unwrap_or_else(|| Rc::new(e)))
    }
    fn dispatch_handlers(&self) -> Handlers {
        let retained = self.this.upgrade().expect("live state");
        let invoke: CoreInvoke =
            Rc::new(move |callback, events| retained.invoke_core(callback, events));
        let map = self
            .base()
            .message_handlers
            .borrow()
            .iter()
            .map(|(key, list)| (key.clone(), Handler::CoreList(list.clone(), invoke.clone())))
            .collect();
        Handlers {
            own: RefCell::new(map),
            prototype: Default::default(),
            realm: self.base().prototype_realm.clone(),
        }
    }
    pub fn on_message(&self, data: &str) -> Result<(), Failure> {
        let _binding = self.base().prototype_realm.bind();
        if self.base().is_disposed() {
            return Ok(());
        }
        self.base().track_last_received().map_err(Rc::new)?;
        dispatch_binance_message(&self.dispatch_handlers(), data, |topic| {
            crate::public_stats::record_binance_handler_stats(&self.base(), topic).map_err(Rc::new)
        })
    }
    pub fn handle_stream(&self, topic: &str, events: EventValue) -> Result<(), Failure> {
        let _binding = self.base().prototype_realm.bind();
        let overridden = self.stream_methods.borrow().get(topic).cloned();
        if let Some(method) = overridden {
            return method(events);
        }
        let EventValue::Array(events) = events else {
            return Err(crate::request_weight::type_error("events is not an array"));
        };
        let store = self.parent.store();
        let existing = self
            .ticker_captures
            .borrow()
            .iter()
            .find(|capture| capture.belongs_to(&store))
            .cloned();
        let captured = if let Some(existing) = existing {
            existing
        } else {
            let capture = crate::ticker_capture::Capture::new(store);
            self.ticker_captures.borrow_mut().push(capture.clone());
            capture
        };
        let dictionary = captured.dictionary();
        let capture = || Ok(dictionary);
        let lookup = |dictionary: &crate::ticker_capture::Dictionary, symbol: &str| {
            Ok(crate::ticker_capture::lookup(dictionary, symbol))
        };
        let get_update = || {
            let store = self.parent.store();
            Ok(
                move |ticker: crate::ticker_capture::Target,
                      fields: crate::public_ticker_events::TickerFields| {
                    let mut patch = ModelPatch::default();
                    for (field, value) in fields {
                        patch.set_number(field.name(), value)
                    }
                    {
                        let id = ticker.id();
                        store.update_ticker(&id, &patch)
                    }
                    .map_err(|e| failure(e.to_string()))
                },
            )
        };
        use crate::public_ticker_events::{
            handle_futures_book_events, handle_futures_mark_events, handle_futures_ticker_events,
            handle_pm_book_events, handle_pm_mark_events, handle_pm_ticker_events,
        };
        match (self.kind, topic) {
            (PublicKind::SpotPublic, "24hrTicker") => {
                crate::spot_ticker_events::handle_spot_ticker_events(
                    &events, capture, lookup, get_update,
                )
            }
            (PublicKind::SpotPublic, _) => crate::spot_ticker_events::handle_spot_book_events(
                &events, capture, lookup, get_update,
            ),
            (PublicKind::PMMarket, "24hrTicker") => {
                handle_pm_ticker_events(&events, capture, lookup, get_update)
            }
            (PublicKind::PMMarket, _) => {
                handle_pm_mark_events(&events, capture, lookup, get_update)
            }
            (PublicKind::PMPublic, _) => {
                handle_pm_book_events(&events, capture, lookup, get_update)
            }
            (PublicKind::FuturesMarket, "24hrTicker") => {
                handle_futures_ticker_events(&events, capture, lookup, get_update)
            }
            (PublicKind::FuturesMarket, _) => {
                handle_futures_mark_events(&events, capture, lookup, get_update)
            }
            _ => handle_futures_book_events(&events, capture, lookup, get_update),
        }
    }
    fn lease(self: &Rc<Self>) -> Rc<PublicSocket> {
        Rc::new(PublicSocket {
            state: self.clone(),
            socket: self.base(),
        })
    }
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        opts: EventValue,
        callback: Rc<dyn Fn(Candle) -> Result<(), Failure>>,
    ) -> Result<Unsubscribe, Failure> {
        match self.kind {
            PublicKind::FuturesPublic | PublicKind::PMPublic => self
                .market_websocket
                .get()
                .unwrap()
                .listen_ohlcv(opts, callback),
            PublicKind::FuturesMarket => listen_futures_market(self.lease(), opts, callback),
            PublicKind::PMMarket => listen_pm_market(self.lease(), opts, callback),
            PublicKind::SpotPublic => listen_spot_candles(self.lease(), opts, callback),
        }
    }
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: String,
        callback: Rc<dyn Fn(Book) -> Result<(), Failure>>,
    ) -> Result<Unsubscribe, Failure> {
        let unsubscribe = match self.kind {
            PublicKind::FuturesPublic => listen_futures_book(self.clone(), symbol, callback),
            PublicKind::PMPublic => listen_pm_book(self.clone(), symbol, callback),
            PublicKind::SpotPublic => listen_spot_book(self.clone(), symbol, callback),
            _ => Err(failure("market socket has no depth listener")),
        }?;
        let socket = self.base();
        Ok(Rc::new(move || {
            let _lease = &socket;
            unsubscribe()
        }))
    }
    pub fn dispose(&self) {
        if let Some(market) = self.market_websocket.get() {
            market.dispose()
        }
        self.base().dispose()
    }
}
impl WsProtocol for Protocol {
    fn connect_and_subscribe(
        &self,
        _socket: Rc<BaseWebSocket>,
        _: bool,
    ) -> Result<(), ExchangeError> {
        self.0.connect_and_subscribe()
    }
    fn on_open(&self, _socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let owner = self.0.clone();
        Box::pin(async move { owner.on_open() })
    }
    fn on_message(
        &self,
        _socket: &Rc<BaseWebSocket>,
        frame: &SocketFrame,
    ) -> Result<(), ExchangeError> {
        let SocketFrame::Text(data) = frame else {
            return Ok(());
        };
        self.0.on_message(data).map_err(error)
    }
}
impl ListenerHost for PublicState {
    fn state(&self) -> &ListenerState {
        &self.listeners
    }
    fn handlers(&self, key: &str) -> Option<Vec<Parser>> {
        let list = self.base().message_handlers.borrow().get(key).cloned()?;
        Some(
            list.borrow()
                .iter()
                .map(|callback| {
                    let known = self.bridges.borrow().iter().find_map(|(native, core)| {
                        core.upgrade()
                            .filter(|core| Rc::ptr_eq(core, callback))
                            .and_then(|_| native.upgrade())
                    });
                    if let Some(known) = known {
                        return known;
                    }
                    let known = self.foreign.borrow().iter().find_map(|(core, native)| {
                        core.upgrade()
                            .filter(|core| Rc::ptr_eq(core, callback))
                            .and_then(|_| native.upgrade())
                    });
                    if let Some(known) = known {
                        return known;
                    }
                    let original = callback.clone();
                    let native: Parser =
                        Rc::new(move |events| original(&core_json(&events)?).map_err(Rc::new));
                    self.foreign
                        .borrow_mut()
                        .push((Rc::downgrade(callback), Rc::downgrade(&native)));
                    native
                })
                .collect(),
        )
    }
    fn replace_handlers(&self, key: String, parsers: Vec<Parser>) {
        let list = parsers.into_iter().map(|p| self.bridge(p)).collect();
        self.base()
            .message_handlers
            .borrow_mut()
            .insert(key, Rc::new(RefCell::new(list)));
    }
    fn delete_handlers(&self, key: &str) {
        self.base().message_handlers.borrow_mut().shift_remove(key);
    }
    fn disposed(&self) -> bool {
        self.base().is_disposed()
    }
    fn connected(&self) -> bool {
        self.base().is_connected()
    }
    fn send(&self, payload: EventValue) -> Result<(), Failure> {
        self.base().send(&encode(&payload)?).map_err(Rc::new)
    }
    fn set_timeout(&self, callback: Action, delay: f64) -> Result<EventValue, Failure> {
        let realm = self.base().prototype_realm.clone();
        self.operations.set_timeout(
            Rc::new(move || {
                let _binding = realm.bind();
                callback()
            }),
            delay,
        )
    }
    fn clear_timeout(&self, ticket: EventValue) -> Result<(), Failure> {
        self.operations.clear_timeout(ticket)
    }
    fn cleanup(&self, handler: &str) -> Result<(), Failure> {
        self.base().cleanup_message_handler_stats(handler)?;
        Ok(())
    }
    fn nanoid(&self) -> Result<String, Failure> {
        self.operations.nanoid()
    }
    fn get_snapshot(&self, venue: Venue, symbol: &str, limit: f64) -> Result<Snapshot, Failure> {
        self.operations.get_snapshot(venue, symbol, limit)
    }
    fn spawn(&self, job: Job) {
        let socket = self.base();
        let realm = socket.prototype_realm.clone();
        self.operations.spawn(Box::pin(realm.future(async move {
            let _lease = socket;
            job.await
        })))
    }
    fn reset_sync(&self, sync: SyncState) -> Result<(), Failure> {
        self.operations.reset_sync(sync)
    }
    fn load(&self, book: Book, data: EventValue, sync: SyncState) -> Result<(), Failure> {
        self.operations.load(book, data, sync)
    }
    fn buffered(
        &self,
        updates: Vec<EventValue>,
        id: EventValue,
    ) -> Result<Vec<EventValue>, Failure> {
        self.operations.buffered(updates, id)
    }
    fn apply(
        &self,
        book: Book,
        data: EventValue,
        sync: SyncState,
        previous: bool,
    ) -> Result<crate::protocol_types::BinanceOrderBookApplyResult, Failure> {
        self.operations.apply(book, data, sync, previous)
    }
    fn total(&self, book: Book) -> Result<(), Failure> {
        self.operations.total(book)
    }
}
impl ListenerHost for PublicSocket {
    fn state(&self) -> &ListenerState {
        ListenerHost::state(self.state.as_ref())
    }
    fn handlers(&self, key: &str) -> Option<Vec<Parser>> {
        ListenerHost::handlers(self.state.as_ref(), key)
    }
    fn replace_handlers(&self, key: String, parsers: Vec<Parser>) {
        ListenerHost::replace_handlers(self.state.as_ref(), key, parsers)
    }
    fn delete_handlers(&self, key: &str) {
        ListenerHost::delete_handlers(self.state.as_ref(), key)
    }
    fn disposed(&self) -> bool {
        ListenerHost::disposed(self.state.as_ref())
    }
    fn connected(&self) -> bool {
        ListenerHost::connected(self.state.as_ref())
    }
    fn send(&self, payload: EventValue) -> Result<(), Failure> {
        ListenerHost::send(self.state.as_ref(), payload)
    }
    fn set_timeout(&self, callback: Action, delay: f64) -> Result<EventValue, Failure> {
        ListenerHost::set_timeout(self.state.as_ref(), callback, delay)
    }
    fn clear_timeout(&self, ticket: EventValue) -> Result<(), Failure> {
        ListenerHost::clear_timeout(self.state.as_ref(), ticket)
    }
    fn cleanup(&self, handler: &str) -> Result<(), Failure> {
        ListenerHost::cleanup(self.state.as_ref(), handler)
    }
    fn nanoid(&self) -> Result<String, Failure> {
        ListenerHost::nanoid(self.state.as_ref())
    }
    fn get_snapshot(&self, venue: Venue, symbol: &str, limit: f64) -> Result<Snapshot, Failure> {
        ListenerHost::get_snapshot(self.state.as_ref(), venue, symbol, limit)
    }
    fn spawn(&self, job: Job) {
        ListenerHost::spawn(self.state.as_ref(), job)
    }
    fn reset_sync(&self, sync: SyncState) -> Result<(), Failure> {
        ListenerHost::reset_sync(self.state.as_ref(), sync)
    }
    fn load(&self, book: Book, data: EventValue, sync: SyncState) -> Result<(), Failure> {
        ListenerHost::load(self.state.as_ref(), book, data, sync)
    }
    fn buffered(
        &self,
        updates: Vec<EventValue>,
        id: EventValue,
    ) -> Result<Vec<EventValue>, Failure> {
        ListenerHost::buffered(self.state.as_ref(), updates, id)
    }
    fn apply(
        &self,
        book: Book,
        data: EventValue,
        sync: SyncState,
        previous: bool,
    ) -> Result<crate::protocol_types::BinanceOrderBookApplyResult, Failure> {
        ListenerHost::apply(self.state.as_ref(), book, data, sync, previous)
    }
    fn total(&self, book: Book) -> Result<(), Failure> {
        ListenerHost::total(self.state.as_ref(), book)
    }
}
pub(crate) fn encode(value: &EventValue) -> Result<String, Failure> {
    safe_cex_num::js_json_stringify(&crate::private_event_value::native_json(value))
        .map_err(|e| failure(e.message))?
        .ok_or_else(|| failure("payload is undefined"))
}

// ts: BinanceMarketWebsocket
pub struct BinanceMarketWebsocket(pub Rc<PublicSocket>);
impl BinanceMarketWebsocket {
    pub fn new(id: u64, d: PublicDependencies) -> Self {
        Self(PublicSocket::new(PublicKind::FuturesMarket, id, d))
    }
}
impl std::ops::Deref for BinanceMarketWebsocket {
    type Target = Rc<PublicSocket>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
// ts: BinancePublicWebsocket (binance.ws-public.ts)
pub struct BinancePublicWebsocket(pub Rc<PublicSocket>);
impl BinancePublicWebsocket {
    pub fn new(id: u64, d: PublicDependencies) -> Self {
        Self(PublicSocket::new(PublicKind::FuturesPublic, id, d))
    }
}
impl std::ops::Deref for BinancePublicWebsocket {
    type Target = Rc<PublicSocket>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
// ts: BinancePMMarketWebsocket
pub struct BinancePMMarketWebsocket(pub Rc<PublicSocket>);
impl BinancePMMarketWebsocket {
    pub fn new(id: u64, d: PublicDependencies) -> Self {
        Self(PublicSocket::new(PublicKind::PMMarket, id, d))
    }
}
impl std::ops::Deref for BinancePMMarketWebsocket {
    type Target = Rc<PublicSocket>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
// ts: BinancePMPublicWebsocket
pub struct BinancePMPublicWebsocket(pub Rc<PublicSocket>);
impl BinancePMPublicWebsocket {
    pub fn new(id: u64, d: PublicDependencies) -> Self {
        Self(PublicSocket::new(PublicKind::PMPublic, id, d))
    }
}
impl std::ops::Deref for BinancePMPublicWebsocket {
    type Target = Rc<PublicSocket>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
// ts: BinancePublicWebsocket (binancespot.ws-public.ts)
pub struct BinanceSpotPublicWebsocket(pub Rc<PublicSocket>);
impl BinanceSpotPublicWebsocket {
    pub fn new(id: u64, d: PublicDependencies) -> Self {
        Self(PublicSocket::new(PublicKind::SpotPublic, id, d))
    }
}
impl std::ops::Deref for BinanceSpotPublicWebsocket {
    type Target = Rc<PublicSocket>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Foreign core callbacks expose an immutable JSON carrier. Reject unsupported
/// native graph values explicitly rather than silently collapsing them to null.
fn core_json(value: &EventValue) -> Result<serde_json::Value, Failure> {
    use serde_json::Value as V;
    Ok(match value {
        EventValue::Null => V::Null,
        EventValue::Bool(v) => V::Bool(*v),
        EventValue::String(_) => V::String(value.text()?),
        EventValue::Number(v) => V::Number(serde_json::Number::from_f64(*v).ok_or_else(|| {
            crate::request_weight::type_error(
                "core JSON callback cannot represent nonfinite native number",
            )
        })?),
        EventValue::Array(v) => V::Array(
            v.borrow()
                .iter()
                .map(|v| core_json(v.as_ref().unwrap_or(&EventValue::Undefined)))
                .collect::<Result<_, _>>()?,
        ),
        EventValue::Object(_) => V::Object(
            value
                .spread()
                .into_iter()
                .map(|(key, value)| Ok((key, core_json(&value)?)))
                .collect::<Result<_, Failure>>()?,
        ),
        _ => {
            return Err(crate::request_weight::type_error(
                "core JSON callback requires JSON graph values",
            ));
        }
    })
}
