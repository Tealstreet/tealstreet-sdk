//! Owned public socket; listener and protocol frames share its native state.
use crate::native_value::NativeValue;
use crate::public_control::JsString;
use crate::{
    adapter::BackpackExchange, native_value::Value, orderbook_state, public_control as control,
    public_listeners as listeners, socket_events, startup_recovery,
};
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, ExchangeTask},
    websocket::{BaseWebSocket, WsProtocol},
};
use safe_cex_transport::socket::SocketFrame;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};
fn units(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn text(s: &[u16]) -> Result<String, ExchangeError> {
    String::from_utf16(s).map_err(|e| ExchangeError::new(e.to_string()))
}
use safe_cex_num::JsJsonValue;
pub struct Throttle {
    pub owner: Weak<BackpackPublicWebSocket>,
    pub depth: bool,
    pub last: Cell<Option<f64>>,
    pub pending: RefCell<Option<Value>>,
    pub task: RefCell<Option<safe_cex_exchange::OwnedOperation>>,
}
impl Throttle {
    // Source windows: throttle(handleTickerUpdate, 1000) and throttle(updateTickerFromDepth, 100).
    fn window_ms(&self) -> f64 {
        if self.depth { 100.0 } else { 1000.0 }
    }
    pub fn call(self: &Rc<Self>, data: Value) -> Result<(), ExchangeError> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Public socket disposed"))?;
        let now = owner.runtime.monotonic_now_ms();
        if self
            .last
            .get()
            .is_none_or(|last| now - last >= self.window_ms() || now < last)
        {
            if let Some(task) = self.task.borrow_mut().take() {
                task.cancel();
            }
            self.pending.borrow_mut().take();
            self.last.set(Some(now));
            let mut h = PublicFrame {
                owner: owner.clone(),
            };
            if self.depth {
                socket_events::update_ticker_from_depth(&data, &mut h)
            } else {
                socket_events::create_ticker_handler(
                    Rc::new(RefCell::new(h)),
                    |callback, _, _, _| callback(&data),
                )
            }
        } else {
            *self.pending.borrow_mut() = Some(data);
            if self.task.borrow().is_none() {
                let weak = Rc::downgrade(self);
                let delay = (self.window_ms() - (now - self.last.get().unwrap())).max(0.0) as u64;
                let task = owner.base.start_timeout(delay.max(1), move || {
                    if let Some(throttle) = weak.upgrade() {
                        throttle.task.borrow_mut().take();
                        let pending = throttle.pending.borrow_mut().take();
                        if let Some(data) = pending {
                            return throttle.call(data);
                        }
                    }
                    Ok(())
                })?;
                *self.task.borrow_mut() = Some(task);
            }
            Ok(())
        }
    }
    pub fn cancel(&self) {
        if let Some(task) = self.task.borrow_mut().take() {
            task.cancel();
        }
        self.pending.borrow_mut().take();
        self.last.set(None);
    }
}
// ts: BackpackPublicWebSocket
pub struct BackpackPublicWebSocket {
    pub base: Rc<BaseWebSocket>,
    pub parent: Weak<BackpackExchange>,
    pub runtime: Rc<dyn safe_cex_exchange::websocket::WsRuntime>,
    pub subscriptions: control::SubscriptionState,
    pub listeners: Rc<listeners::ListenerState<ExchangeError, Rc<Throttle>>>,
    pub ping_at: Cell<f64>,
    pub ticker_handler: Rc<Throttle>,
    pub tasks: RefCell<Vec<Box<dyn ExchangeTask>>>,
}
struct Protocol {
    owner: Weak<BackpackPublicWebSocket>,
}
impl WsProtocol for Protocol {
    fn connect_and_subscribe(&self, _: Rc<BaseWebSocket>, _: bool) -> Result<(), ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Public protocol owner gone"))?
            .connect_and_subscribe()
    }
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Public protocol owner gone"))?
            .ping()
    }
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = self
            .owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Public protocol owner gone"))
            .and_then(|o| o.on_open());
        Box::pin(std::future::ready(result))
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Public protocol owner gone"))?
            .on_message(frame)
    }
}
impl BackpackPublicWebSocket {
    pub fn new(parent: Rc<BackpackExchange>) -> Result<Rc<Self>, ExchangeError> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let owner = Rc::new_cyclic(|weak| {
            let base = BaseWebSocket::new(
                NEXT.fetch_add(1, Ordering::Relaxed),
                crate::socket_parent::Parent::new(&parent),
                parent.runtime.clone(),
                Rc::new(Protocol {
                    owner: weak.clone(),
                }),
            );
            Self {
                base,
                runtime: parent.runtime.clone(),
                parent: Rc::downgrade(&parent),
                subscriptions: Rc::new(RefCell::new(control::Subscriptions::default())),
                listeners: Rc::new(listeners::ListenerState::default()),
                ping_at: Cell::new(0.0),
                ticker_handler: Rc::new(Throttle {
                    owner: weak.clone(),
                    depth: false,
                    last: Cell::new(None),
                    pending: RefCell::new(None),
                    task: RefCell::new(None),
                }),
                tasks: RefCell::new(vec![]),
            }
        });
        let listeners = Rc::downgrade(&owner.listeners);
        safe_cex_exchange::websocket::install_native_handler_stats(
            &owner.base,
            Rc::new(move |topic| {
                let listeners = listeners
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("Backpack public registry owner released"))?;
                let units: Vec<u16> = topic.encode_utf16().collect();
                let count = listeners
                    .handlers
                    .borrow()
                    .iter()
                    .find(|(key, _)| key.as_slice() == units.as_slice())
                    .map(|(_, handlers)| handlers.borrow().len());
                Ok(count)
            }),
        );
        Ok(owner)
    }
    pub fn parent(&self) -> Result<Rc<BackpackExchange>, ExchangeError> {
        self.parent
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Backpack parent disposed"))
    }
    pub fn frame(self: &Rc<Self>) -> Rc<RefCell<PublicFrame>> {
        Rc::new(RefCell::new(PublicFrame {
            owner: self.clone(),
        }))
    }
    pub fn listener_frame(self: &Rc<Self>) -> Rc<RefCell<crate::weak_listener::ListenerFrame>> {
        Rc::new(RefCell::new(crate::weak_listener::ListenerFrame {
            owner: Rc::downgrade(self),
            state: Rc::downgrade(&self.listeners),
        }))
    }
    pub fn connect_and_subscribe(&self) -> Result<(), ExchangeError> {
        if !self.base.is_disposed() {
            let testnet = self.parent()?.base.options().testnet == Some(true);
            self.base.start_websocket(
                crate::constants::WSS_URLS
                    .iter()
                    .find(|(k, _)| *k == if testnet { "TESTNET" } else { "PRODUCTION" })
                    .unwrap()
                    .1
                    .into(),
                vec![],
                None,
            );
        }
        Ok(())
    }
    pub fn ping(&self) -> Result<(), ExchangeError> {
        control::ping(
            self.base.is_disposed(),
            || Ok(self.base.is_connected()),
            &self.ping_at,
            || {
                self.base.mark_ping_sent();
                Ok(self.runtime.monotonic_now_ms())
            },
            |m| self.send(m),
        )
    }
    fn send(&self, m: control::Message) -> Result<(), ExchangeError> {
        self.base.send(&text(&m.to_json())?)
    }
    pub fn on_open(self: &Rc<Self>) -> Result<(), ExchangeError> {
        control::on_open(
            self.base.is_disposed(),
            || self.base.track_last_received(),
            || self.reset_and_refetch_order_books(),
            || self.resubscribe(),
            || self.ping(),
        )
    }
    pub fn subscribe_to_order_book(&self, s: &[u16]) -> Result<(), ExchangeError> {
        control::subscribe_to_order_book(&self.subscriptions, s, |m| self.send(m))
    }
    pub fn subscribe_to_ticker(&self, s: &[u16]) -> Result<(), ExchangeError> {
        control::subscribe_to_ticker(&self.subscriptions, s, |m| self.send(m))
    }
    pub fn subscribe_to_trades(&self, s: &[u16]) -> Result<(), ExchangeError> {
        control::subscribe_to_trades(&self.subscriptions, s, |m| self.send(m))
    }
    pub fn subscribe_to_book_ticker(&self, s: &[u16]) -> Result<(), ExchangeError> {
        control::subscribe_to_book_ticker(&self.subscriptions, s, |m| self.send(m))
    }
    pub fn subscribe_to_kline(&self, s: &[u16], i: &[u16]) -> Result<(), ExchangeError> {
        control::subscribe_to_kline(&self.subscriptions, s, i, |m| self.send(m))
    }
    pub fn resubscribe(&self) -> Result<(), ExchangeError> {
        control::resubscribe(&self.subscriptions, |m| self.send(m))
    }
    pub fn unsubscribe(&self, s: control::Streams) -> Result<(), ExchangeError> {
        control::unsubscribe(&self.subscriptions, &s, |m| self.send(m))
    }
    pub fn unsubscribe_from_kline(&self, s: &[u16], i: &[u16]) -> Result<(), ExchangeError> {
        control::unsubscribe_from_kline(s, i, |streams| self.unsubscribe(streams))
    }
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        symbol: JsString,
        interval: JsString,
        callback: listeners::CandleCallback<ExchangeError>,
    ) -> Result<listeners::Action<ExchangeError>, ExchangeError> {
        listeners::listen_ohlcv(self.listener_frame(), symbol, interval, callback)
    }
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: JsString,
        callback: listeners::BookCallback<ExchangeError>,
    ) -> Result<listeners::Action<ExchangeError>, ExchangeError> {
        listeners::listen_order_book(self.listener_frame(), symbol, callback)
    }
    pub fn reset_and_refetch_order_books(&self) -> Result<(), ExchangeError> {
        let keys = self
            .listeners
            .states
            .borrow()
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for key in keys {
            let handlers = self
                .listeners
                .states
                .borrow()
                .iter()
                .find(|(k, _)| k == &key)
                .unwrap()
                .1
                .borrow()
                .terminal_handlers
                .clone();
            let mut state = orderbook_state::create_order_book_state();
            state.terminal_handlers = handlers;
            self.listeners
                .states
                .borrow_mut()
                .iter_mut()
                .find(|(k, _)| k == &key)
                .unwrap()
                .1 = Rc::new(RefCell::new(state));
        }
        let keys = self
            .listeners
            .fetchers
            .borrow()
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();
        for key in keys {
            let fetcher = self
                .listeners
                .fetchers
                .borrow()
                .iter()
                .find(|(k, _)| k == &key)
                .map(|(_, f)| f.clone());
            if let Some(fetcher) = fetcher {
                self.spawn(fetcher());
            }
        }
        Ok(())
    }
    pub fn recover_order_book(
        self: &Rc<Self>,
        id: JsString,
        reason: JsString,
    ) -> Result<(), ExchangeError> {
        startup_recovery::recover_order_book(id, reason, self.frame())
    }
    pub fn terminate_order_book(
        self: &Rc<Self>,
        id: JsString,
        state: listeners::State<ExchangeError>,
        reason: JsString,
    ) -> Result<(), ExchangeError> {
        startup_recovery::terminate_order_book(
            &id,
            &state,
            &reason,
            &mut PublicFrame {
                owner: self.clone(),
            },
        )
    }

    pub fn spawn(&self, task: listeners::Task<(), ExchangeError>) {
        let runtime = self.runtime.clone();
        let report = runtime.clone();
        self.tasks
            .borrow_mut()
            .push(runtime.spawn_local(Box::pin(async move {
                if let Err(e) = task.await {
                    report.report_exception(
                        &e,
                        &serde_json::json!({"operation":"Backpack public socket"}).into(),
                    );
                }
            })));
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.subscriptions
            .borrow_mut()
            .clone_from(&control::Subscriptions::default());
        self.listeners.states.borrow_mut().clear();
        self.listeners.fetchers.borrow_mut().clear();
        for (_, t) in &*self.listeners.throttles.borrow() {
            t.cancel();
        }
        self.listeners.throttles.borrow_mut().clear();
        self.base.close_current_websocket();
        Ok(())
    }
    pub fn on_message(self: &Rc<Self>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        let SocketFrame::Text(s) = frame else {
            return Ok(());
        };
        socket_events::on_public_message(
            s,
            &mut PublicFrame {
                owner: self.clone(),
            },
        )
    }
}
pub struct PublicFrame {
    pub owner: Rc<BackpackPublicWebSocket>,
}
impl listeners::Host for PublicFrame {
    type Error = ExchangeError;
    type Throttle = Rc<Throttle>;
    fn state(&self) -> Rc<listeners::ListenerState<ExchangeError, Rc<Throttle>>> {
        self.owner.listeners.clone()
    }
    fn market(&mut self, symbol: &[u16]) -> Result<Option<JsString>, ExchangeError> {
        let symbol = text(symbol)?;
        Ok(self
            .owner
            .parent()?
            .base
            .store
            .markets()?
            .iter()
            .find(|m| m.symbol == symbol)
            .map(|m| units(&m.id)))
    }
    fn connected(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.base.is_connected())
    }
    fn disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.base.is_disposed())
    }
    fn emit(&mut self, s: JsString) -> Result<(), ExchangeError> {
        self.owner.parent()?.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(text(&s)?),
            None,
            None,
        )
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(units(&e.js_error_string()))
    }
    fn timeout(
        &mut self,
        callback: listeners::Action<ExchangeError>,
        delay: f64,
    ) -> Result<(), ExchangeError> {
        self.owner
            .base
            .start_timeout((delay as u64).max(1), move || callback())?;
        Ok(())
    }
    fn subscribe_kline(&mut self, id: JsString, interval: JsString) -> Result<(), ExchangeError> {
        self.owner.subscribe_to_kline(&id, &interval)
    }
    fn unsubscribe_kline(&mut self, id: JsString, interval: JsString) -> Result<(), ExchangeError> {
        self.owner.unsubscribe_from_kline(&id, &interval)
    }
    fn subscribe_depth(&mut self, id: JsString) -> Result<(), ExchangeError> {
        self.owner.subscribe_to_order_book(&id)
    }
    fn unsubscribe(&mut self, topic: JsString) -> Result<(), ExchangeError> {
        self.owner
            .unsubscribe(Rc::new(RefCell::new(vec![Some(topic)])))
    }
    fn cleanup_stats(&mut self, topic: JsString) -> Result<(), ExchangeError> {
        self.owner
            .base
            .cleanup_message_handler_stats(&text(&topic)?)?;
        Ok(())
    }
    fn date(&mut self, s: JsString) -> Result<f64, ExchangeError> {
        (self.owner.parent()?.date_parse)(s)
    }
    fn snapshot(
        &mut self,
        id: JsString,
    ) -> Result<listeners::Task<listeners::Snapshot, ExchangeError>, ExchangeError> {
        let response = self.owner.parent()?.xhr.request(
            "GET",
            "/api/v1/depth",
            JsJsonValue::Object(vec![("symbol".into(), JsJsonValue::String(text(&id)?))]),
            JsJsonValue::Undefined,
        );
        Ok(Box::pin(async move {
            let v = response.await?;
            let levels = |side: &str| -> Result<Vec<[JsString; 2]>, ExchangeError> {
                let side = v.field(side)?.array()?;
                side.array_items()?
                    .expect("array variant")
                    .iter()
                    .map(|row| {
                        let row = row
                            .as_ref()
                            .ok_or_else(|| ExchangeError::new("Undefined snapshot level"))?
                            .array()?;
                        let row = row.array_items()?.expect("array variant");
                        Ok([
                            row.first()
                                .cloned()
                                .flatten()
                                .unwrap_or(Value::Undefined)
                                .string()?,
                            row.get(1)
                                .cloned()
                                .flatten()
                                .unwrap_or(Value::Undefined)
                                .string()?,
                        ])
                    })
                    .collect()
            };
            let bids = levels("bids")?;
            let asks = levels("asks")?;
            let last = match v.field("lastUpdateId")? {
                Value::Undefined => listeners::LastId::Undefined,
                Value::Null => listeners::LastId::Null,
                Value::Number(n) => listeners::LastId::Number(n),
                Value::String(s) => listeners::LastId::Text(s.to_vec()),
                v => listeners::LastId::Number(v.number()?),
            };
            Ok(listeners::Snapshot { bids, asks, last })
        }))
    }
    fn spawn(&mut self, task: listeners::Task<(), ExchangeError>) -> Result<(), ExchangeError> {
        self.owner.spawn(task);
        Ok(())
    }
    fn apply(
        &mut self,
        book: listeners::Book,
        data: Rc<RefCell<listeners::Delta>>,
    ) -> Result<(), ExchangeError> {
        let levels = data.borrow().levels.clone();
        orderbook_state::apply_order_book_update(&book, &levels, |book, side, level| {
            safe_cex_store::update_order_book::update_order_book(
                &mut book.borrow_mut(),
                side == orderbook_state::BookSide::Bids,
                level,
                true,
            );
            Ok(())
        })
    }
    fn totals(&mut self, book: listeners::Book) -> Result<(), ExchangeError> {
        safe_cex_store::orderbook::calc_order_book_total(&mut book.borrow_mut());
        Ok(())
    }
    fn recover(&mut self, id: JsString, reason: JsString) -> Result<(), ExchangeError> {
        self.owner.recover_order_book(id, reason)
    }
    fn terminate(
        &mut self,
        id: JsString,
        state: listeners::State<ExchangeError>,
        reason: JsString,
    ) -> Result<(), ExchangeError> {
        self.owner.terminate_order_book(id, state, reason)
    }
    fn cancel_throttle(&mut self, t: Rc<Throttle>) -> Result<(), ExchangeError> {
        t.cancel();
        Ok(())
    }
}
impl startup_recovery::Logger for PublicFrame {
    type Error = ExchangeError;
    type LogTarget = ();
    fn log_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn log(
        &mut self,
        _: (),
        message: JsString,
        severity: Option<startup_recovery::Severity>,
    ) -> Result<(), ExchangeError> {
        self.owner.parent()?.base.log(
            &text(&message)?,
            match severity {
                Some(startup_recovery::Severity::Error) => "error",
                Some(startup_recovery::Severity::Warning) => "warning",
                None => "info",
            },
        )
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(units(&e.js_error_string()))
    }
}
impl
    startup_recovery::RecoveryHost<
        Rc<RefCell<listeners::Delta>>,
        listeners::Terminal<ExchangeError>,
    > for PublicFrame
{
    type Fetcher = listeners::Fetcher<ExchangeError>;
    type TerminalEmitter = ();
    fn state(
        &mut self,
        id: &[u16],
    ) -> Result<Option<listeners::State<ExchangeError>>, ExchangeError> {
        Ok(self
            .owner
            .listeners
            .states
            .borrow()
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, s)| s.clone()))
    }
    fn fetcher(&mut self, id: &[u16]) -> Result<Option<Self::Fetcher>, ExchangeError> {
        Ok(self
            .owner
            .listeners
            .fetchers
            .borrow()
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, s)| s.clone()))
    }
    fn disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.base.is_disposed())
    }
    fn terminate(
        &mut self,
        id: JsString,
        state: listeners::State<ExchangeError>,
        reason: JsString,
    ) -> Result<(), ExchangeError> {
        self.owner.terminate_order_book(id, state, reason)
    }
    fn microtask(
        &mut self,
        callback: startup_recovery::Callback<ExchangeError>,
    ) -> Result<(), ExchangeError> {
        self.owner.spawn(Box::pin(async move { callback() }));
        Ok(())
    }
    fn fetch_snapshot(&mut self, f: Self::Fetcher) -> Result<(), ExchangeError> {
        self.owner.spawn(f());
        Ok(())
    }
    fn terminal_emitter(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_terminal(&mut self, _: (), message: JsString) -> Result<(), ExchangeError> {
        listeners::Host::emit(self, message)
    }
    fn terminal_handler(
        &mut self,
        h: Rc<listeners::Terminal<ExchangeError>>,
        _: &[u16],
    ) -> Result<(), ExchangeError> {
        (h.call)()
    }
}
impl socket_events::ErrorSink for PublicFrame {
    type Error = ExchangeError;
    type Emitter = ();
    fn error_emitter(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(units(&e.js_error_string()))
    }
    fn emit_error(&mut self, _: (), s: JsString) -> Result<(), ExchangeError> {
        listeners::Host::emit(self, s)
    }
}
impl socket_events::MessageValue for Value {
    fn truthy(&self) -> bool {
        self.truthy()
    }
    fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    fn equals_number(&self, n: f64) -> bool {
        matches!(self,Value::Number(v)if *v==n)
    }
    fn equals_string(&self, s: &str) -> bool {
        matches!(self,Value::String(v)if v.as_ref()==units(s))
    }
}
impl socket_events::MessageHost<String> for PublicFrame {
    type Value = Value;
    type Dispatcher = socket_events::EventKind;
    fn dispatch_method(
        &mut self,
        k: socket_events::EventKind,
    ) -> Result<Self::Dispatcher, ExchangeError> {
        Ok(k)
    }
    fn is_disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.base.is_disposed())
    }
    fn track_last_received(&mut self) -> Result<(), ExchangeError> {
        self.owner.base.track_last_received()
    }
    fn parse(&mut self, data: &String) -> Result<Value, ExchangeError> {
        serde_json::from_str::<serde_json::Value>(data)
            .map(|v| Value::from_prepared(v.into()))
            .or(Ok(Value::Null))
    }
    fn property(&mut self, v: &Value, k: &str) -> Result<Value, ExchangeError> {
        v.field(k)
    }
    fn stringify(&mut self, v: &Value) -> Result<JsString, ExchangeError> {
        let s = safe_cex_num::js_json_stringify(&v.prepared()?)
            .map_err(|e| ExchangeError::new(e.message))?
            .unwrap_or_else(|| "undefined".into());
        Ok(units(&s))
    }
    fn starts_with(&mut self, v: &Value, prefix: &str) -> Result<bool, ExchangeError> {
        Ok(v.string()?.starts_with(&units(prefix)))
    }
    fn pong(&mut self, interval: f64) -> Result<(), ExchangeError> {
        self.owner.base.handle_pong_event(interval as u64)
    }
    fn dispatch(
        &mut self,
        k: Self::Dispatcher,
        data: Value,
        stream: Option<Value>,
    ) -> Result<(), ExchangeError> {
        match k {
            socket_events::EventKind::Depth => socket_events::handle_depth_update(&data, self),
            socket_events::EventKind::Kline => socket_events::handle_kline_update(
                &data,
                stream.map(|v| v.string()).transpose()?.as_deref(),
                self,
            ),
            socket_events::EventKind::Ticker => self.owner.ticker_handler.call(data),
            socket_events::EventKind::Trade => socket_events::handle_trade_update(&data, self),
            socket_events::EventKind::BookTicker => {
                socket_events::handle_book_ticker_update(&data, self)
            }
            socket_events::EventKind::Order | socket_events::EventKind::Position => Err(
                ExchangeError::new("Private event reached public dispatcher"),
            ),
        }
    }
}
impl socket_events::HandlerHost<Value> for PublicFrame {
    type List = Rc<RefCell<Vec<Rc<listeners::Handler<ExchangeError>>>>>;
    type Handler = Rc<listeners::Handler<ExchangeError>>;
    type Throttle = Rc<Throttle>;
    type ThrottleFactory = ();
    fn throttle_factory(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn symbol(&mut self, v: &Value) -> Result<JsString, ExchangeError> {
        v.field("s")?.string()
    }
    fn interval(&mut self, v: &Value) -> Result<JsString, ExchangeError> {
        v.field("i")?.string()
    }
    fn handlers(&mut self, topic: &[u16]) -> Result<Option<Self::List>, ExchangeError> {
        Ok(self
            .owner
            .listeners
            .handlers
            .borrow()
            .iter()
            .find(|(k, _)| k == topic)
            .map(|(_, v)| v.clone()))
    }
    fn length(&mut self, list: &Self::List) -> Result<usize, ExchangeError> {
        Ok(list.borrow().len())
    }
    fn handler(
        &mut self,
        list: &Self::List,
        i: usize,
    ) -> Result<Option<Self::Handler>, ExchangeError> {
        Ok(list.borrow().get(i).cloned())
    }
    fn record_stats(&mut self, topic: &[u16]) -> Result<(), ExchangeError> {
        self.owner.base.record_message_handler_stats(&text(topic)?)
    }
    fn invoke_handler(
        &mut self,
        handler: Self::Handler,
        data: &Value,
    ) -> Result<(), ExchangeError> {
        let event = data.field("e")?;
        let is_kline = matches!(event,Value::String(s)if s.as_ref()==units("kline"));
        let data = if is_kline {
            let field = |key: &str| -> Result<listeners::Field, ExchangeError> {
                Ok(match data.field(key)? {
                    Value::Null => listeners::Field::Null,
                    Value::Undefined => listeners::Field::Undefined,
                    v => listeners::Field::Text(v.string()?),
                })
            };
            listeners::Data::Kline(listeners::Kline {
                symbol: data.field("s")?.string()?,
                time: data.field("t")?.string()?,
                open: field("o")?,
                high: field("h")?,
                low: field("l")?,
                close: field("c")?,
                volume: field("v")?,
            })
        } else {
            let levels =
                |key: &str| -> Result<Option<orderbook_state::DepthLevels>, ExchangeError> {
                    let v = data.field(key)?;
                    if matches!(v, Value::Null | Value::Undefined) {
                        return Ok(None);
                    }
                    let a = v.array()?;
                    let mut rows = vec![];
                    for row in a.array_items()?.expect("array variant").iter().flatten() {
                        let pair = row.array()?;
                        let pair = pair.array_items()?.expect("array variant");
                        rows.push([
                            pair.first()
                                .cloned()
                                .flatten()
                                .unwrap_or(Value::Undefined)
                                .string()?,
                            pair.get(1)
                                .cloned()
                                .flatten()
                                .unwrap_or(Value::Undefined)
                                .string()?,
                        ]);
                    }
                    Ok(Some(Rc::new(RefCell::new(rows))))
                };
            let number = |key: &str| -> Result<Option<f64>, ExchangeError> {
                let v = data.field(key)?;
                if matches!(v, Value::Undefined) {
                    Ok(None)
                } else {
                    Ok(Some(v.number()?))
                }
            };
            listeners::Data::Delta(Rc::new(RefCell::new(listeners::Delta {
                symbol: data.field("s")?.string()?,
                sequence: orderbook_state::SequenceUpdate {
                    first: number("U")?,
                    last: number("u")?,
                },
                levels: Rc::new(RefCell::new(orderbook_state::DepthUpdate {
                    bids: levels("b")?,
                    asks: levels("a")?,
                })),
            })))
        };
        (handler.call)(data)
    }
    fn is_null(&mut self, data: &Value, k: &str) -> Result<bool, ExchangeError> {
        Ok(matches!(data.field(k)?, Value::Null))
    }
    fn depth_throttle(&mut self, _: (), symbol: &[u16]) -> Result<Rc<Throttle>, ExchangeError> {
        if let Some((_, t)) = self
            .owner
            .listeners
            .throttles
            .borrow()
            .iter()
            .find(|(k, _)| k == symbol)
        {
            return Ok(t.clone());
        }
        let t = Rc::new(Throttle {
            owner: Rc::downgrade(&self.owner),
            depth: true,
            last: Cell::new(None),
            pending: RefCell::new(None),
            task: RefCell::new(None),
        });
        self.owner
            .listeners
            .throttles
            .borrow_mut()
            .push((symbol.to_vec(), t.clone()));
        Ok(t)
    }
    fn invoke_throttle(&mut self, t: Rc<Throttle>, data: &Value) -> Result<(), ExchangeError> {
        t.call(data.clone())
    }
}
impl socket_events::TickerHost<Value> for PublicFrame {
    type Ticker = String;
    fn ticker(&mut self, data: &Value) -> Result<Option<String>, ExchangeError> {
        let symbol = data.field("s")?.utf8()?;
        Ok(self
            .owner
            .parent()?
            .base
            .store
            .tickers()?
            .iter()
            .find(|t| t.symbol == symbol)
            .map(|t| t.id.clone()))
    }
    fn field_text(&mut self, data: &Value, k: &str) -> Result<JsString, ExchangeError> {
        data.field(k)?.string()
    }
    fn levels_len(
        &mut self,
        data: &Value,
        side: socket_events::Side,
    ) -> Result<usize, ExchangeError> {
        let v = data.field(if matches!(side, socket_events::Side::Bids) {
            "b"
        } else {
            "a"
        })?;
        if v.truthy() {
            Ok(v.array()?.get("length")?.to_number()? as usize)
        } else {
            Ok(0)
        }
    }
    fn first_price(
        &mut self,
        data: &Value,
        side: socket_events::Side,
    ) -> Result<JsString, ExchangeError> {
        let a = data
            .field(if matches!(side, socket_events::Side::Bids) {
                "b"
            } else {
                "a"
            })?
            .array()?;
        let row = a.get("0")?;
        let pair = row.array()?;
        let value = pair.get("0")?;
        value.string()
    }
    fn update_ticker(
        &mut self,
        id: String,
        p: socket_events::TickerPatch,
    ) -> Result<(), ExchangeError> {
        let mut patch = safe_cex_store::model_patch::ModelPatch::default();
        for (k, n) in [
            ("bid", p.bid),
            ("ask", p.ask),
            ("last", p.last),
            ("percentage", p.percentage),
            ("volume", p.volume),
            ("quoteVolume", p.quote_volume),
        ] {
            if let Some(n) = n {
                patch.set_number(k, n);
            }
        }
        self.owner
            .parent()?
            .base
            .store
            .update_ticker(&id, &patch)
            .map_err(Into::into)
    }
}
