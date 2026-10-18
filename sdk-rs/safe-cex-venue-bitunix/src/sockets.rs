//! Public, futures and private Bitunix sockets on the shared BaseWebSocket runtime.
use crate::{
    adapter::{BitunixVenue, Fault, json as adapter_json, to_value, units, utf8},
    composition::{FuturesSocket, PrivateSocket, PublicSocket},
    session_control::{ConnectionHost, PrivateOpenHost, SocketKind, SocketListener},
};
use safe_cex_exchange::{
    OwnedOperation,
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup, LogSeverity},
    websocket::{BaseWebSocket, ReopenResync, WsParent, WsProtocol, WsRuntime},
};
use safe_cex_num::{
    JsJsonValue, TableHit,
    get_key::{GetKeyObject, GetKeyValue, v},
    js_json_stringify, js_string,
};
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{Candle, ExchangeOptions, Order, OrderBook, OrderBookOrders};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    future::Future,
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll, Waker},
};

pub(crate) type OwnerSlot = Rc<RefCell<Weak<BitunixVenue>>>;
type Parser = Rc<dyn Fn(&Value) -> Result<(), ExchangeError>>;
type Timer = Rc<OwnedOperation>;
enum Kind {
    Public(PublicSocket<(), bool, u64, Timer, bool, String>),
    Futures(FuturesSocket<(), bool, u64, Timer>),
    Private(PrivateSocket<(), bool, Timer>),
}
pub(crate) struct Socket {
    pub(crate) base: Rc<BaseWebSocket>,
    owner: OwnerSlot,
    kind: SocketKind,
    state: RefCell<Kind>,
    self_ref: Weak<Socket>,
    parsers: RefCell<HashMap<u64, Parser>>,
    next_parser: Cell<u64>,
    markets_listener: RefCell<Option<String>>,
}
fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn stringify(value: &JsJsonValue) -> Result<String, ExchangeError> {
    js_json_stringify(value)
        .map_err(crate::adapter::coercion)?
        .ok_or_else(|| ExchangeError::new("Cannot send undefined frame"))
}
/// Drive a ported async body whose awaited hosts all settle synchronously.
fn settle(future: impl Future<Output = Result<(), ExchangeError>>) -> Result<(), ExchangeError> {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => Err(ExchangeError::new(
            "Bitunix socket handler suspended unexpectedly",
        )),
    }
}

struct Parent {
    owner: OwnerSlot,
    resync: Rc<ReopenResync>,
    store: Rc<dyn Store>,
    options: ExchangeOptions,
}
impl Parent {
    fn live(&self) -> Result<Rc<BitunixVenue>, ExchangeError> {
        self.owner
            .borrow()
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Bitunix parent was disposed"))
    }
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "bitunix"
    }
    fn is_disposed(&self) -> bool {
        self.owner
            .borrow()
            .upgrade()
            .is_none_or(|o| o.base.is_disposed())
    }
    fn options(&self) -> ExchangeOptions {
        self.owner
            .borrow()
            .upgrade()
            .map_or_else(|| self.options.clone(), |o| o.base.options())
    }
    fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    fn is_private_socket(&self, socket: &BaseWebSocket) -> bool {
        self.owner
            .borrow()
            .upgrade()
            .is_some_and(|o| std::ptr::eq(o.socket("private").base.as_ref(), socket))
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.resync.clone()
    }
    fn fetch_and_set_private_snapshot(&self, retries: usize) -> ExchangeFuture<'static, ()> {
        let owner = self.live();
        Box::pin(async move {
            let owner = owner?;
            Exchange::fetch_and_set_private_snapshot(owner.as_ref(), retries as u32).await
        })
    }
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.live()?.base.log(message, severity.as_str())
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        self.live()?.base.emit(
            event,
            safe_cex_exchange::js_runtime::value::Js::str(message),
        )
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.live()?.base.sync_socket_health_to_store()
    }
    fn update_latency(&self, latency: f64) -> Result<(), ExchangeError> {
        self.live()?
            .base
            .store
            .update(
                StoreUpdate {
                    latency: Some(latency),
                    ..Default::default()
                },
                false,
            )
            .map_err(Into::into)
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.live()?.base.emit_ws_reconnected()
    }
    fn emit_and_log_error(
        &self,
        error: &str,
        message: Option<&str>,
        extra: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        self.live()?.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(error.into()),
            message,
            extra,
        )
    }
}

struct Protocol {
    owner: Weak<Socket>,
}
impl Protocol {
    fn socket(&self) -> Result<Rc<Socket>, ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Bitunix socket owner gone"))
    }
}
impl WsProtocol for Protocol {
    fn connect_and_subscribe(&self, _: Rc<BaseWebSocket>, _: bool) -> Result<(), ExchangeError> {
        self.socket()?.connect_and_subscribe()
    }
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        self.socket()?.ping()
    }
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = self.socket().and_then(|s| s.on_open());
        Box::pin(std::future::ready(result))
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        self.socket()?.on_message(frame)
    }
    // ts: the sockets register `handleClose = () => { …; this.onClose(); }`, never the
    // base onClose, so processClose sees no close code.
    fn owns_close_handling(&self) -> bool {
        true
    }
    fn on_transport_close(
        &self,
        _: &Rc<BaseWebSocket>,
        _: Option<u16>,
        _: &str,
    ) -> Result<(), ExchangeError> {
        let socket = self.socket()?;
        let kind = socket.kind;
        crate::session_control::connection_close(&mut Conn { socket }, kind)
    }
    // ts: `handleError` closes a live socket, then runs handleClose.
    fn on_error(&self, _: &BaseWebSocket, _: &str) -> Result<(), ExchangeError> {
        let socket = self.socket()?;
        let kind = socket.kind;
        crate::session_control::connection_error(&mut Conn { socket }, kind)
    }
}

impl Socket {
    pub(crate) fn new(
        owner: OwnerSlot,
        parent: &Rc<safe_cex_exchange::exchange::BaseExchange>,
        runtime: Rc<dyn WsRuntime>,
        kind: &'static str,
        secondary: Option<bool>,
        market_symbols: Option<Vec<String>>,
    ) -> Result<Rc<Self>, ExchangeError> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let options = parent.options();
        let route = options.tealstreet_ws_route == Some(true);
        let kind = match kind {
            "public" => SocketKind::Public,
            "private" => SocketKind::Private,
            "futures" => SocketKind::Futures,
            other => {
                return Err(ExchangeError::new(format!(
                    "Unknown Bitunix socket {other}"
                )));
            }
        };
        let state = match kind {
            SocketKind::Public => Kind::Public(crate::composition::new_public_socket(
                (),
                secondary,
                || false,
                market_symbols,
                |()| Ok::<(), ExchangeError>(()),
                |()| Ok(route),
            )?),
            SocketKind::Futures => Kind::Futures(crate::composition::new_futures_socket(
                (),
                |()| Ok::<(), ExchangeError>(()),
                |()| Ok(route),
            )?),
            SocketKind::Private => Kind::Private(crate::composition::new_private_socket(
                (),
                |()| Ok::<(), ExchangeError>(()),
                |()| Ok(route),
            )?),
        };
        let ws_parent = Rc::new(Parent {
            owner: owner.clone(),
            resync: Rc::new(ReopenResync::default()),
            store: parent.store.clone(),
            options,
        });
        let socket = Rc::new_cyclic(|weak: &Weak<Socket>| Self {
            base: BaseWebSocket::new(
                NEXT.fetch_add(1, Ordering::Relaxed),
                ws_parent,
                runtime,
                Rc::new(Protocol {
                    owner: weak.clone(),
                }),
            ),
            owner,
            kind,
            state: RefCell::new(state),
            self_ref: weak.clone(),
            parsers: RefCell::new(HashMap::new()),
            next_parser: Cell::new(1),
            markets_listener: RefCell::new(None),
        });
        Ok(socket)
    }
    fn me(&self) -> Rc<Socket> {
        self.self_ref.upgrade().expect("live Bitunix socket")
    }
    fn runtime(&self) -> Rc<dyn WsRuntime> {
        self.venue()
            .map(|v| v.runtime.clone())
            .expect("Bitunix socket runtime")
    }
    fn venue(&self) -> Result<Rc<BitunixVenue>, ExchangeError> {
        self.owner
            .borrow()
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Bitunix parent was disposed"))
    }
    fn disposed(&self) -> bool {
        self.base.is_disposed()
    }
    fn with<T>(&self, f: impl FnOnce(&mut Kind) -> T) -> T {
        f(&mut self.state.borrow_mut())
    }
    fn counters(&self, field: &str) -> f64 {
        self.with(|k| {
            let (count, last, attempts, max) = match k {
                Kind::Public(s) => (
                    s.message_count,
                    s.last_message_count,
                    s.connection_attempts,
                    s.max_attempts,
                ),
                Kind::Futures(s) => (
                    s.message_count,
                    s.last_message_count,
                    s.connection_attempts,
                    s.max_attempts,
                ),
                Kind::Private(s) => (
                    s.message_count,
                    s.last_message_count,
                    s.connection_attempts,
                    s.max_attempts,
                ),
            };
            match field {
                "messageCount" => count,
                "lastMessageCount" => last,
                "connectionAttempts" => attempts,
                _ => max,
            }
        })
    }
    fn set_counter(&self, field: &str, value: f64) {
        self.with(|k| {
            let (count, last, attempts) = match k {
                Kind::Public(s) => (
                    &mut s.message_count,
                    &mut s.last_message_count,
                    &mut s.connection_attempts,
                ),
                Kind::Futures(s) => (
                    &mut s.message_count,
                    &mut s.last_message_count,
                    &mut s.connection_attempts,
                ),
                Kind::Private(s) => (
                    &mut s.message_count,
                    &mut s.last_message_count,
                    &mut s.connection_attempts,
                ),
            };
            match field {
                "messageCount" => *count = value,
                "lastMessageCount" => *last = value,
                _ => *attempts = value,
            }
        })
    }
    fn route(&self) -> bool {
        self.with(|k| match k {
            Kind::Public(s) => s.route,
            Kind::Futures(s) => s.route,
            Kind::Private(s) => s.route,
        })
    }
    fn set_route(&self, value: bool) {
        self.with(|k| match k {
            Kind::Public(s) => s.route = value,
            Kind::Futures(s) => s.route = value,
            Kind::Private(s) => s.route = value,
        })
    }
    fn peek_ping_timer(&self) -> Option<Timer> {
        self.with(|k| match k {
            Kind::Public(s) => s.ping_timer.clone(),
            Kind::Futures(s) => s.ping_timer.clone(),
            Kind::Private(s) => s.ping_timer.clone(),
        })
    }
    fn take_ping_timer(&self) -> Option<Timer> {
        self.with(|k| match k {
            Kind::Public(s) => s.ping_timer.take(),
            Kind::Futures(s) => s.ping_timer.take(),
            Kind::Private(s) => s.ping_timer.take(),
        })
    }
    fn set_ping_timer(&self, timer: Option<Timer>) {
        self.with(|k| match k {
            Kind::Public(s) => s.ping_timer = timer,
            Kind::Futures(s) => s.ping_timer = timer,
            Kind::Private(s) => s.ping_timer = timer,
        })
    }
    fn send(&self, frame: &JsJsonValue) -> Result<(), ExchangeError> {
        self.base.send(&stringify(frame)?)
    }

    pub(crate) fn connect_and_subscribe(&self) -> Result<(), ExchangeError> {
        let mut host = Conn { socket: self.me() };
        match self.kind {
            SocketKind::Public => crate::session_control::connect_public(&mut host),
            SocketKind::Futures => crate::session_control::connect_futures(&mut host),
            SocketKind::Private => settle(crate::session_control::connect_private(&mut host)),
        }
    }
    pub(crate) fn dispose(&self) -> Result<(), ExchangeError> {
        match self.kind {
            SocketKind::Public => crate::websocket_public::dispose(
                |event, _| {
                    if let Some(id) = self.markets_listener.borrow_mut().take() {
                        self.venue()?.base.off(event, Some(&id));
                    }
                    Ok(())
                },
                || {
                    if let Some(t) = self.take_ping_timer() {
                        t.cancel();
                    }
                    Ok(())
                },
                || self.clear_subscription_queue(),
                || {
                    self.base.dispose();
                    Ok(())
                },
            ),
            _ => {
                self.base.dispose();
                Ok(())
            }
        }
    }
    fn ping(&self) -> Result<(), ExchangeError> {
        let runtime = self.runtime();
        match self.kind {
            SocketKind::Private => crate::websocket_private::ping(
                self.disposed(),
                || Ok(runtime.monotonic_now_ms()),
                |_| {
                    self.base.mark_ping_sent();
                    Ok(())
                },
                |frame| self.base.send_with_log(&stringify(&frame)?),
                || self.base.handle_pong_event(5_000),
                || self.base.track_last_received(),
            ),
            SocketKind::Public => crate::websocket_public::ping(
                self.disposed(),
                || Ok(runtime.monotonic_now_ms()),
                |_| {
                    self.base.mark_ping_sent();
                    Ok(())
                },
                || Ok(runtime.now_ms()),
                |frame| self.base.send_with_log(&stringify(&frame)?),
            ),
            SocketKind::Futures => crate::websocket_futures::ping(
                self.disposed(),
                || Ok(runtime.monotonic_now_ms()),
                |_| {
                    self.base.mark_ping_sent();
                    Ok(())
                },
                || Ok(runtime.now_ms()),
                |frame| self.base.send_with_log(&stringify(&frame)?),
            ),
        }
    }
    fn schedule_ping_loop(&self) -> Result<Timer, ExchangeError> {
        let weak = self.self_ref.clone();
        Ok(Rc::new(self.base.start_timeout(
            15_000,
            move || match weak.upgrade() {
                Some(socket) => socket.ping_loop(),
                None => Ok(()),
            },
        )?))
    }
    fn ping_loop(&self) -> Result<(), ExchangeError> {
        let disposed = self.disposed();
        let run = |ping: &dyn Fn() -> Result<(), ExchangeError>| match self.kind {
            SocketKind::Futures => crate::websocket_futures::ping_loop(
                || Ok(disposed),
                ping,
                |_, _| self.schedule_ping_loop(),
                |t| {
                    self.set_ping_timer(Some(t));
                    Ok(())
                },
            ),
            _ => crate::websocket_public::ping_loop(
                || Ok(disposed),
                ping,
                |_, _| self.schedule_ping_loop(),
                |t| {
                    self.set_ping_timer(Some(t));
                    Ok(())
                },
            ),
        };
        run(&|| self.ping())
    }
    fn on_open(&self) -> Result<(), ExchangeError> {
        match self.kind {
            SocketKind::Public => crate::websocket_public::on_open(
                || Ok(self.disposed()),
                || Ok(self.is_secondary()),
                |channel| self.subscribe_markets_to_channel(channel),
                || {
                    self.base.reset_orderbook();
                    Ok(())
                },
                || self.subscribe(),
                (
                    |event, _| {
                        if let Some(id) = self.markets_listener.borrow_mut().take() {
                            self.venue()?.base.off(event, Some(&id));
                        }
                        Ok(())
                    },
                    |event, _| self.listen_markets_changed(event),
                ),
                (
                    || Ok(self.peek_ping_timer()),
                    |t: &Option<Timer>| t.is_some(),
                    |t: Option<Timer>| {
                        if let Some(t) = t {
                            t.cancel();
                        }
                        Ok(())
                    },
                ),
                (
                    || self.ping(),
                    |_, _| self.schedule_ping_loop(),
                    |t| {
                        self.set_ping_timer(Some(t));
                        Ok(())
                    },
                ),
            ),
            SocketKind::Futures => crate::websocket_futures::on_open(
                || Ok(self.disposed()),
                || {
                    self.base.reset_orderbook();
                    Ok(())
                },
                |frame| self.send(&frame),
                || {
                    Ok(self.with(|k| match k {
                        Kind::Futures(s) => s.topics.iter().map(|(t, c)| (t.clone(), *c)).collect(),
                        _ => vec![],
                    }))
                },
                |topic: String| self.base.send(&topic),
                (
                    || Ok(self.peek_ping_timer()),
                    |t: &Option<Timer>| t.is_some(),
                    |t: Option<Timer>| {
                        if let Some(t) = t {
                            t.cancel();
                        }
                        Ok(())
                    },
                ),
                (
                    || self.ping(),
                    |_, _| self.schedule_ping_loop(),
                    |t| {
                        self.set_ping_timer(Some(t));
                        Ok(())
                    },
                ),
            ),
            SocketKind::Private => {
                crate::session_control::private_on_open(&mut PrivateOpen { socket: self.me() })
            }
        }
    }
    fn on_message(&self, frame: &SocketFrame) -> Result<(), ExchangeError> {
        match self.kind {
            SocketKind::Public => {
                let SocketFrame::Text(data) = frame else {
                    return Ok(());
                };
                self.on_public_message(data.clone())
            }
            SocketKind::Futures => self.on_futures_message(frame),
            SocketKind::Private => {
                let SocketFrame::Text(data) = frame else {
                    return Ok(());
                };
                crate::event_models::on_private_message(
                    &mut PrivateMessages { socket: self.me() },
                    data.clone(),
                )
                .map_err(|e| e.0)
            }
        }
    }
}

// ---- public socket ----
impl Socket {
    fn is_secondary(&self) -> bool {
        self.with(|k| matches!(k, Kind::Public(s) if s.is_secondary))
    }
    fn allowed(&self, symbol: &str) -> bool {
        self.with(|k| match k {
            Kind::Public(s) => s
                .market_symbols
                .as_ref()
                .is_none_or(|m| m.iter().any(|x| x == symbol)),
            _ => true,
        })
    }
    fn track_topic(&self, arg: &JsJsonValue) -> Result<(), ExchangeError> {
        let key = stringify(arg)?;
        self.with(|k| {
            if let Kind::Public(s) = k {
                s.topics.insert(key, 1.0);
            }
        });
        Ok(())
    }
    fn arg(symbol: &str, channel: &str) -> JsJsonValue {
        JsJsonValue::Object(vec![
            ("symbol".into(), JsJsonValue::String(symbol.into())),
            ("ch".into(), JsJsonValue::String(channel.into())),
        ])
    }
    pub(crate) fn set_market_symbols(
        &self,
        symbols: Option<Vec<String>>,
    ) -> Result<(), ExchangeError> {
        crate::websocket_public::set_market_symbols(
            symbols.map(|s| s.into_iter().map(Ok::<_, ExchangeError>)),
            |unique| {
                self.with(|k| {
                    if let Kind::Public(s) = k {
                        s.market_symbols = unique;
                    }
                });
                Ok(())
            },
        )
    }
    fn subscribe_markets_to_channel(&self, channel: &'static str) -> Result<(), ExchangeError> {
        let venue = self.venue()?;
        crate::websocket_public::subscribe_markets_to_channel(
            channel,
            || Ok(venue.base.store.markets()?.to_vec()),
            |m| Ok(self.allowed(&m.symbol)),
            |m| Ok(m.symbol),
            |symbol, channel| Ok(Self::arg(&symbol, channel)),
            |arg, _| self.track_topic(&arg),
        )
    }
    fn subscribe(&self) -> Result<(), ExchangeError> {
        let topics: Vec<String> = self.with(|k| match k {
            Kind::Public(s) => s.topics.keys().cloned().collect(),
            _ => vec![],
        });
        crate::websocket_public::subscribe(
            || Ok(topics),
            |topic| {
                let parsed: Value =
                    serde_json::from_str(topic).map_err(|e| ExchangeError::new(text(e)))?;
                Ok(parsed["ch"]
                    .as_str()
                    .is_some_and(|c| c.starts_with("depth_")))
            },
            |topic| {
                Ok(JsJsonValue::from(
                    serde_json::from_str::<Value>(topic)
                        .map_err(|e| ExchangeError::new(text(e)))?,
                ))
            },
            |args, batch| self.enqueue_subscriptions(args, batch),
        )
    }
    fn enqueue_subscriptions(
        &self,
        args: Vec<JsJsonValue>,
        batch: Option<f64>,
    ) -> Result<(), ExchangeError> {
        crate::websocket_public::enqueue_subscriptions(
            batch,
            || Ok(args.len() as f64),
            |from, to| Ok(args[from as usize..(to as usize).min(args.len())].to_vec()),
            |slice| {
                self.with(|k| {
                    if let Kind::Public(s) = k {
                        s.subscription_queue.push(slice);
                    }
                });
                Ok(())
            },
            || Ok(self.with(|k| matches!(k, Kind::Public(s) if s.subscription_timer.is_some()))),
            || self.drain_subscription_queue(),
        )
    }
    fn drain_subscription_queue(&self) -> Result<(), ExchangeError> {
        crate::websocket_public::drain_subscription_queue(
            || Ok(self.disposed()),
            || Ok(self.with(|k| matches!(k, Kind::Public(s) if s.subscription_queue.is_empty()))),
            || {
                Ok(self.with(|k| match k {
                    Kind::Public(s) if !s.subscription_queue.is_empty() => {
                        Some(s.subscription_queue.remove(0))
                    }
                    _ => None,
                }))
            },
            |args| Ok(!args.is_empty()),
            (
                |op, args| {
                    stringify(&JsJsonValue::Object(vec![
                        ("op".into(), JsJsonValue::String(op.into())),
                        ("args".into(), JsJsonValue::Array(args)),
                    ]))
                },
                |frame| self.base.send(&frame),
            ),
            (
                |_, delay| {
                    let weak = self.self_ref.clone();
                    Ok(Rc::new(self.base.start_timeout(
                        delay as u64,
                        move || match weak.upgrade() {
                            Some(s) => s.drain_subscription_queue(),
                            None => Ok(()),
                        },
                    )?))
                },
                |timer| {
                    self.with(|k| {
                        if let Kind::Public(s) = k {
                            s.subscription_timer = timer;
                        }
                    });
                    Ok(())
                },
            ),
        )
    }
    fn clear_subscription_queue(&self) -> Result<(), ExchangeError> {
        crate::websocket_public::clear_subscription_queue(
            || {
                Ok(self.with(|k| match k {
                    Kind::Public(s) => s.subscription_timer.take(),
                    _ => None,
                }))
            },
            |timer| {
                if let Some(t) = timer {
                    t.cancel();
                }
                Ok(())
            },
            || Ok(()),
            || {
                self.with(|k| {
                    if let Kind::Public(s) = k {
                        s.subscription_queue.clear();
                    }
                });
                Ok(())
            },
        )
    }
    fn listen_markets_changed(&self, event: &'static str) -> Result<(), ExchangeError> {
        let venue = self.venue()?;
        let weak = self.self_ref.clone();
        let id = venue.base.on(
            event,
            Rc::new(move |_, payload| match weak.upgrade() {
                Some(s) => s.on_markets_changed(payload),
                None => Ok(()),
            }),
        );
        *self.markets_listener.borrow_mut() = Some(id);
        Ok(())
    }
    fn on_markets_changed(
        &self,
        payload: &safe_cex_exchange::js_runtime::value::Js,
    ) -> Result<(), ExchangeError> {
        let added = payload.get("added")?.array_values()?.unwrap_or_default();
        let mut tracked = vec![];
        crate::websocket_public::on_markets_changed(
            || Ok(!added.is_empty()),
            || Ok(self.base.has_websocket()),
            || Ok(self.disposed()),
            || Ok(self.is_secondary()),
            || Ok(added.clone()),
            |m| Ok(self.allowed(&utf8(&m.get("symbol")?.to_js_string()?)?)),
            |m, channel| {
                Ok(Self::arg(
                    &utf8(&m.get("symbol")?.to_js_string()?)?,
                    channel,
                ))
            },
            |arg, _| self.track_topic(arg),
            |args| {
                tracked = args;
                Ok(())
            },
        )?;
        self.enqueue_subscriptions(tracked, None)
    }
    fn on_public_message(&self, data: String) -> Result<(), ExchangeError> {
        let venue = self.venue()?;
        let runtime = venue.runtime.clone();
        crate::websocket_public::on_message(
            data,
            || Ok(self.disposed()),
            || self.base.track_last_received(),
            (
                || Ok(self.counters("messageCount")),
                |n| {
                    self.set_counter("messageCount", n);
                    Ok(())
                },
            ),
            (
                |raw: String| {
                    serde_json::from_str::<Value>(&raw).map_err(|e| ExchangeError::new(text(e)))
                },
                |parsed: &Value| Ok(parsed.get("ch").cloned().unwrap_or(Value::Null)),
            ),
            (
                |t: &Value| safe_cex_num::js_truthy(Some(t)),
                |t: &Value, name| t.as_str() == Some(name),
                |t: &Value, prefix| Ok(t.as_str().is_some_and(|s| s.starts_with(prefix))),
            ),
            |t| {
                self.base.record_message_handler_stats(
                    &js_string(Some(t)).map_err(crate::adapter::coercion)?,
                )
            },
            |stream, parsed| self.stream_event(stream, parsed),
            (
                |parsed: &Value, topic: &Value, _| {
                    stringify(&JsJsonValue::Object(vec![
                        (
                            "symbol".into(),
                            JsJsonValue::from(parsed.get("symbol").cloned().unwrap_or(Value::Null)),
                        ),
                        ("ch".into(), JsJsonValue::from(topic.clone())),
                    ]))
                },
                |key: String| {
                    // TS iterates messageHandlers[key]; public listeners are never registered.
                    let handlers = self.with(|k| match k {
                        Kind::Public(s) => s.message_handlers.get(&key).cloned(),
                        _ => None,
                    });
                    handlers.ok_or_else(|| {
                        let mut e = ExchangeError::new("this.messageHandlers[key] is not iterable");
                        e.name = "TypeError".into();
                        e
                    })
                },
                |list: &Vec<u64>, i| Ok(list.get(i).copied()),
                |id, rows: Rc<Vec<Rc<Value>>>| {
                    let parser = self.parsers.borrow().get(&id).cloned();
                    match (parser, rows.first()) {
                        (Some(parser), Some(row)) => parser(row),
                        _ => Ok(()),
                    }
                },
            ),
            |e| {
                runtime.console("error", &e.js_error_string());
                Ok(())
            },
        )
    }
    fn stream_event(&self, stream: &'static str, event: &Value) -> Result<(), ExchangeError> {
        let venue = self.venue()?;
        let symbol = event
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let ticker = || {
            Ok(venue
                .base
                .store
                .tickers_by_symbol()?
                .get(&symbol)
                .map(|t| t.id.clone()))
        };
        let data = || Ok(event.get("data").cloned().unwrap_or(Value::Null));
        let field = |d: &Value, k: &'static str| -> Result<Vec<u16>, ExchangeError> {
            Ok(units(&match v(d.as_object().map(GetKeyObject::Json), k) {
                TableHit::Mapped(GetKeyValue::Json(value)) => {
                    js_string(Some(value)).map_err(crate::adapter::coercion)?
                }
                _ => "undefined".into(),
            }))
        };
        let update = |id: String, fields: &[(&'static str, f64)]| {
            let mut patch = ModelPatch::default();
            for (k, n) in fields {
                patch.set_number(*k, *n);
            }
            venue
                .base
                .store
                .update_ticker(&id, &patch)
                .map_err(ExchangeError::from)
        };
        match stream {
            "ticker" => {
                crate::websocket_public::handle_ticker_stream_event(ticker, data, field, |id, f| {
                    update(id, &f)
                })
            }
            "price" => {
                crate::websocket_public::handle_price_stream_event(ticker, data, field, |id, f| {
                    update(id, &f)
                })
            }
            _ => crate::websocket_public::handle_depth_book_stream_event(
                ticker,
                data,
                |d: &Value, side| {
                    let first = d.get(side).and_then(|s| s.get(0)).and_then(|l| l.get(0));
                    Ok(units(&match first {
                        Some(value) => js_string(Some(value)).map_err(crate::adapter::coercion)?,
                        None => "undefined".into(),
                    }))
                },
                |id, f| update(id, &f),
            ),
        }
    }
}

// ---- futures socket ----
impl Socket {
    fn on_futures_message(&self, frame: &SocketFrame) -> Result<(), ExchangeError> {
        enum Data {
            Text(String),
            Blob(Vec<u8>),
        }
        let data = match frame {
            SocketFrame::Text(t) => Data::Text(t.clone()),
            SocketFrame::Binary(b) => Data::Blob(b.clone()),
            SocketFrame::Close { .. } => return Ok(()),
        };
        let runtime = self.runtime();
        settle(crate::websocket_futures::on_message(
            data,
            || Ok(self.disposed()),
            || self.base.track_last_received(),
            (
                || Ok(self.counters("messageCount")),
                |n| {
                    self.set_counter("messageCount", n);
                    Ok(())
                },
            ),
            |d| matches!(d, Data::Blob(_)),
            |d| {
                let result = match d {
                    Data::Blob(bytes) => {
                        use std::io::Read;
                        let mut out = String::new();
                        flate2::read::ZlibDecoder::new(bytes.as_slice())
                            .read_to_string(&mut out)
                            .map(|_| Data::Text(out))
                            .map_err(|e| ExchangeError::new(text(e)))
                    }
                    Data::Text(t) => Ok(Data::Text(t.clone())),
                };
                std::future::ready(result)
            },
            |label, e| {
                runtime.console("error", &format!("{label} {}", e.message));
                Ok(())
            },
            |d| {
                Ok(match d {
                    Data::Text(t) => serde_json::from_str::<Value>(&t).unwrap_or(Value::Null),
                    Data::Blob(_) => Value::Null,
                })
            },
            (
                |p: &Value| Ok(self.handlers(&Self::channel(p)).is_some()),
                |p: &Value| {
                    let list = self.handlers(&Self::channel(p)).unwrap_or_default();
                    let len = list.len();
                    Ok((list, len))
                },
                |list: &Vec<u64>, i| Ok(list.get(i).copied()),
                |id, p: &Value| {
                    let parser = self.parsers.borrow().get(&id).cloned();
                    match parser {
                        Some(parser) => parser(p),
                        None => Ok(()),
                    }
                },
            ),
        ))
    }
    fn channel(parsed: &Value) -> String {
        match parsed.get("channel") {
            Some(Value::String(s)) => s.clone(),
            Some(other) if !other.is_null() => other.to_string(),
            _ => "undefined".into(),
        }
    }
    fn handlers(&self, handler: &str) -> Option<Vec<u64>> {
        self.with(|k| match k {
            Kind::Futures(s) => s.message_handlers.get(handler).cloned(),
            _ => None,
        })
    }
    fn set_handlers(&self, handler: &str, list: Vec<u64>) {
        self.with(|k| {
            if let Kind::Futures(s) = k {
                s.message_handlers.insert(handler.into(), list);
            }
        })
    }
    fn topic_count(&self, topic: &str) -> f64 {
        self.with(|k| match k {
            Kind::Futures(s) => s.topics.get(topic).copied().unwrap_or(f64::NAN),
            _ => f64::NAN,
        })
    }
    fn set_topic_count(&self, topic: &str, count: f64) {
        self.with(|k| {
            if let Kind::Futures(s) = k {
                s.topics.insert(topic.into(), count);
            }
        })
    }
    fn allocate(&self) -> u64 {
        let id = self.next_parser.get();
        self.next_parser.set(id + 1);
        id
    }
    pub(crate) fn listen_ohlcv(
        &self,
        symbol: String,
        interval: String,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        type Sub = crate::ohlcv_subscription::OhlcvSubscription<Rc<dyn Fn(Candle)>, u64>;
        let slot: Rc<RefCell<Option<Rc<Sub>>>> = Rc::default();
        let mut host = Feed {
            socket: self.me(),
            resubscribe: Resubscribe::Candles(slot.clone()),
        };
        let sub = Rc::new(crate::ohlcv_subscription::listen_ohlcv(
            &mut host,
            Rc::new(RefCell::new(crate::ohlcv_subscription::OhlcvOptions {
                symbol,
                interval,
            })),
            callback,
        )?);
        *slot.borrow_mut() = Some(sub.clone());
        let weak = self.self_ref.clone();
        let parse_sub = Rc::downgrade(&sub);
        self.parsers.borrow_mut().insert(
            sub.parser,
            Rc::new(move |json| match (weak.upgrade(), parse_sub.upgrade()) {
                (Some(socket), Some(sub)) => sub.parse(
                    &mut Feed {
                        socket,
                        resubscribe: Resubscribe::None,
                    },
                    json,
                ),
                _ => Ok(()),
            }),
        );
        let socket = self.me();
        Ok(Box::new(move || {
            socket.parsers.borrow_mut().remove(&sub.parser);
            slot.borrow_mut().take();
            sub.cancel(&mut Feed {
                socket: socket.clone(),
                resubscribe: Resubscribe::None,
            })
        }))
    }
    pub(crate) fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        type Sub = crate::orderbook_subscription::BookSubscription<
            Rc<dyn Fn(OrderBook)>,
            u64,
            OwnedOperation,
        >;
        let slot: Rc<RefCell<Option<Rc<RefCell<Sub>>>>> = Rc::default();
        let mut host = Feed {
            socket: self.me(),
            resubscribe: Resubscribe::Book(slot.clone()),
        };
        let sub = Rc::new(RefCell::new(
            crate::orderbook_subscription::listen_order_book(&mut host, &symbol, callback)?,
        ));
        *slot.borrow_mut() = Some(sub.clone());
        let parser = sub.borrow().parser;
        let weak = self.self_ref.clone();
        let parse_sub = Rc::downgrade(&sub);
        self.parsers.borrow_mut().insert(
            parser,
            Rc::new(move |json| match (weak.upgrade(), parse_sub.upgrade()) {
                (Some(socket), Some(sub)) => {
                    let sub = sub.borrow();
                    sub.parse(
                        &mut Feed {
                            socket,
                            resubscribe: Resubscribe::None,
                        },
                        json,
                    )
                }
                _ => Ok(()),
            }),
        );
        let socket = self.me();
        Ok(Box::new(move || {
            socket.parsers.borrow_mut().remove(&parser);
            slot.borrow_mut().take();
            let mut host = Feed {
                socket: socket.clone(),
                resubscribe: Resubscribe::None,
            };
            sub.borrow_mut().cancel(&mut host)
        }))
    }
}
enum Resubscribe {
    None,
    Candles(
        Rc<
            RefCell<
                Option<Rc<crate::ohlcv_subscription::OhlcvSubscription<Rc<dyn Fn(Candle)>, u64>>>,
            >,
        >,
    ),
    Book(
        Rc<
            RefCell<
                Option<
                    Rc<
                        RefCell<
                            crate::orderbook_subscription::BookSubscription<
                                Rc<dyn Fn(OrderBook)>,
                                u64,
                                OwnedOperation,
                            >,
                        >,
                    >,
                >,
            >,
        >,
    ),
}
struct Feed {
    socket: Rc<Socket>,
    resubscribe: Resubscribe,
}
impl Feed {
    fn retry(&self, delay: f64) -> Result<OwnedOperation, ExchangeError> {
        let socket = Rc::downgrade(&self.socket);
        let retry = match &self.resubscribe {
            Resubscribe::None => {
                return Err(ExchangeError::new(
                    "waitForConnectedAndSubscribe has no subscription",
                ));
            }
            Resubscribe::Candles(slot) => Resubscribe::Candles(slot.clone()),
            Resubscribe::Book(slot) => Resubscribe::Book(slot.clone()),
        };
        self.socket.base.start_timeout(delay as u64, move || {
            let Some(socket) = socket.upgrade() else {
                return Ok(());
            };
            match &retry {
                Resubscribe::Candles(slot) => {
                    let sub = slot.borrow().clone();
                    match sub {
                        Some(sub) => sub.subscribe(&mut Feed {
                            socket,
                            resubscribe: Resubscribe::Candles(slot.clone()),
                        }),
                        None => Ok(()),
                    }
                }
                Resubscribe::Book(slot) => {
                    let sub = slot.borrow().clone();
                    match sub {
                        Some(sub) => sub.borrow_mut().subscribe(&mut Feed {
                            socket,
                            resubscribe: Resubscribe::Book(slot.clone()),
                        }),
                        None => Ok(()),
                    }
                }
                Resubscribe::None => Ok(()),
            }
        })
    }
}
impl crate::ohlcv_subscription::OhlcvHost<Rc<dyn Fn(Candle)>> for Feed {
    type Parser = u64;
    type Ticker = String;
    fn allocate_parser(&mut self) -> u64 {
        self.socket.allocate()
    }
    fn connected(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.socket.base.is_connected())
    }
    fn handlers(&mut self, handler: &str) -> Result<Vec<u64>, ExchangeError> {
        Ok(self.socket.handlers(handler).unwrap_or_default())
    }
    fn set_handlers(&mut self, handler: &str, handlers: Vec<u64>) -> Result<(), ExchangeError> {
        self.socket.set_handlers(handler, handlers);
        Ok(())
    }
    fn topic_count(&mut self, topic: &str) -> Result<f64, ExchangeError> {
        Ok(self.socket.topic_count(topic))
    }
    fn set_topic_count(&mut self, topic: &str, count: f64) -> Result<(), ExchangeError> {
        self.socket.set_topic_count(topic, count);
        Ok(())
    }
    fn send(&mut self, frame: &JsJsonValue) -> Result<(), ExchangeError> {
        self.socket.send(frame)
    }
    fn wait(&mut self, delay: f64) -> Result<(), ExchangeError> {
        self.retry(delay).map(drop)
    }
    fn cleanup_stats(&mut self, handler: &str) -> Result<(), ExchangeError> {
        self.socket.base.cleanup_message_handler_stats(handler)
    }
    fn ticker(&mut self, symbol: &str) -> Result<Option<String>, ExchangeError> {
        let venue = self.socket.venue()?;
        let ticker = venue
            .base
            .store
            .tickers()?
            .iter()
            .find(|t| t.symbol == symbol)
            .map(|t| t.id.clone());
        Ok(ticker)
    }
    fn update_last(&mut self, ticker: String, last: f64) -> Result<(), ExchangeError> {
        let venue = self.socket.venue()?;
        let mut patch = ModelPatch::default();
        patch.set_number("last", last);
        venue
            .base
            .store
            .update_ticker(&ticker, &patch)
            .map_err(ExchangeError::from)
    }
    fn call(&mut self, callback: &Rc<dyn Fn(Candle)>, candle: Candle) -> Result<(), ExchangeError> {
        callback(candle);
        Ok(())
    }
}
impl crate::orderbook_subscription::OrderBookHost<Rc<dyn Fn(OrderBook)>> for Feed {
    type Parser = u64;
    type Timer = OwnedOperation;
    fn precision(&mut self, symbol: &str) -> Result<Option<Value>, ExchangeError> {
        let venue = self.socket.venue()?;
        let market = venue
            .base
            .store
            .markets()?
            .iter()
            .find(|m| m.symbol == symbol)
            .cloned();
        market.map(|m| to_value(&m.precision.price)).transpose()
    }
    fn allocate_parser(&mut self) -> u64 {
        self.socket.allocate()
    }
    fn connected(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.socket.base.is_connected())
    }
    fn handlers(&mut self, handler: &str) -> Result<Vec<u64>, ExchangeError> {
        Ok(self.socket.handlers(handler).unwrap_or_default())
    }
    fn set_handlers(&mut self, handler: &str, parsers: Vec<u64>) -> Result<(), ExchangeError> {
        self.socket.set_handlers(handler, parsers);
        Ok(())
    }
    fn topic_count(&mut self, topic: &str) -> Result<f64, ExchangeError> {
        Ok(self.socket.topic_count(topic))
    }
    fn set_topic_count(&mut self, topic: &str, count: f64) -> Result<(), ExchangeError> {
        self.socket.set_topic_count(topic, count);
        Ok(())
    }
    fn send(&mut self, frame: &JsJsonValue) -> Result<(), ExchangeError> {
        self.socket.send(frame)
    }
    fn wait(&mut self, delay: f64) -> Result<OwnedOperation, ExchangeError> {
        self.retry(delay)
    }
    fn timer_truthy(&mut self, timer: &OwnedOperation) -> bool {
        timer.is_active()
    }
    fn clear(&mut self, timer: OwnedOperation) -> Result<(), ExchangeError> {
        timer.cancel();
        Ok(())
    }
    fn cleanup_stats(&mut self, handler: &str) -> Result<(), ExchangeError> {
        self.socket.base.cleanup_message_handler_stats(handler)
    }
    fn call(
        &mut self,
        callback: &Rc<dyn Fn(OrderBook)>,
        book: Rc<RefCell<crate::orderbook_subscription::OrderBook>>,
    ) -> Result<(), ExchangeError> {
        let side = |levels: &[crate::orderbook_subscription::BookLevel]| {
            levels
                .iter()
                .map(|l| OrderBookOrders {
                    price: l.price,
                    amount: l.amount,
                    total: l.total,
                })
                .collect()
        };
        let snapshot = {
            let book = book.borrow();
            OrderBook {
                bids: side(&book.bids),
                asks: side(&book.asks),
            }
        };
        callback(snapshot);
        Ok(())
    }
}

// ---- connection lifecycle ----
struct Conn {
    socket: Rc<Socket>,
}
impl ConnectionHost for Conn {
    type Url = Option<String>;
    type Socket = ();
    type Timer = Option<Timer>;
    type Error = ExchangeError;
    fn flag(&mut self, field: &'static str) -> Result<bool, ExchangeError> {
        Ok(match field {
            "isDisposed" => self.socket.disposed(),
            "useFallbackUrl" => self.socket.route(),
            "testnet" => self.socket.venue()?.base.options().testnet == Some(true),
            other => return Err(ExchangeError::new(format!("Unknown socket flag {other}"))),
        })
    }
    fn number(&mut self, field: &'static str) -> Result<f64, ExchangeError> {
        Ok(self.socket.counters(field))
    }
    fn set_flag(&mut self, _: &'static str, value: bool) -> Result<(), ExchangeError> {
        self.socket.set_route(value);
        Ok(())
    }
    fn set_number(&mut self, field: &'static str, value: f64) -> Result<(), ExchangeError> {
        self.socket.set_counter(field, value);
        Ok(())
    }
    fn clear_queue(&mut self) -> Result<(), ExchangeError> {
        self.socket.clear_subscription_queue()
    }
    fn fallback(&mut self, scope: &'static str) -> Result<Option<String>, ExchangeError> {
        let venue = self.socket.venue()?;
        let options = venue.base.options();
        let routing = venue.runtime.transport().ws_routing(
            Exchange::name(venue.as_ref()),
            &safe_cex_transport::RoutingOptions::from(&options),
        );
        Ok(match scope {
            "wsPublic" => routing.ws_public.fallback,
            "wsPrivate" => routing.ws_private.fallback,
            _ => routing.ws_business.fallback,
        })
    }
    fn url_truthy(&self, url: &Option<String>) -> bool {
        url.as_ref().is_some_and(|u| !u.is_empty())
    }
    fn base_url(
        &mut self,
        scope: &'static str,
        testnet: bool,
    ) -> Result<Option<String>, ExchangeError> {
        let urls = crate::types::BASE_WS_URL
            .iter()
            .find(|(k, _)| *k == scope)
            .ok_or_else(|| ExchangeError::new(format!("Unknown Bitunix ws scope {scope}")))?
            .1;
        let key = if testnet { "testnet" } else { "livenet" };
        Ok(urls
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, u)| (*u).to_owned()))
    }
    fn proxy_info(&mut self, message: Vec<u16>) -> Result<(), ExchangeError> {
        self.socket.venue()?.base.emit(
            "grafana-info",
            safe_cex_exchange::js_runtime::value::Js::String(message.into()),
        )
    }
    fn create(&mut self, url: &Option<String>) -> Result<(), ExchangeError> {
        let url = url
            .clone()
            .ok_or_else(|| ExchangeError::new("Bitunix websocket URL missing"))?;
        self.socket.base.start_websocket(url, vec![], None);
        Ok(())
    }
    fn assign_socket(&mut self, (): ()) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn socket(&mut self) -> Result<Option<()>, ExchangeError> {
        Ok(self.socket.base.has_websocket().then_some(()))
    }
    // BaseWebSocket dispatches open/message/close/error to the protocol hooks above.
    fn add_listener(
        &mut self,
        _: Option<()>,
        _: &'static str,
        _: SocketListener,
        _: SocketKind,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn closed(&mut self, _: Option<()>) -> Result<bool, ExchangeError> {
        Ok(!self.socket.base.is_connected())
    }
    fn close_socket(&mut self, _: Option<()>) -> Result<(), ExchangeError> {
        self.socket.base.close_current_websocket();
        Ok(())
    }
    fn ping_timer(&mut self) -> Result<Option<Timer>, ExchangeError> {
        Ok(self.socket.take_ping_timer())
    }
    fn clear_timer(&mut self, timer: Option<Timer>) -> Result<(), ExchangeError> {
        if let Some(t) = timer {
            t.cancel();
        }
        Ok(())
    }
    // ts: `this.onClose()` with no event.
    fn on_close(&mut self) -> Result<(), ExchangeError> {
        self.socket.base.on_close(None, "")
    }
}

// ---- private socket ----
struct PrivateOpen {
    socket: Rc<Socket>,
}
impl PrivateOpenHost for PrivateOpen {
    type Timer = Option<Timer>;
    type Login = ExchangeFuture<'static, ()>;
    type Error = ExchangeError;
    fn disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.socket.disposed())
    }
    fn ping(&mut self) -> Result<(), ExchangeError> {
        self.socket.ping()
    }
    fn timer(&mut self) -> Result<Option<Timer>, ExchangeError> {
        Ok(self.socket.peek_ping_timer())
    }
    fn timer_truthy(&self, timer: &Option<Timer>) -> bool {
        timer.is_some()
    }
    fn clear(&mut self, timer: Option<Timer>) -> Result<(), ExchangeError> {
        if let Some(t) = timer {
            t.cancel();
        }
        Ok(())
    }
    fn begin_login(&mut self) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        Ok(self.socket.send_login())
    }
    fn spawn_login(
        &mut self,
        login: Result<ExchangeFuture<'static, ()>, ExchangeError>,
        after: fn(&mut Self) -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        let socket = self.socket.clone();
        // The ping loop starts only after login settles: auth precedes any further traffic.
        self.socket.base.start_message_operation(async move {
            login?.await?;
            after(&mut PrivateOpen { socket })
        });
        Ok(())
    }
    fn schedule_ping(
        &mut self,
        delay: f64,
        continuation: fn(&mut Self) -> Result<(), ExchangeError>,
    ) -> Result<Option<Timer>, ExchangeError> {
        let weak = Rc::downgrade(&self.socket);
        Ok(Some(Rc::new(self.socket.base.start_timeout(
            delay as u64,
            move || match weak.upgrade() {
                Some(socket) => continuation(&mut PrivateOpen { socket }),
                None => Ok(()),
            },
        )?)))
    }
    fn assign_timer(&mut self, timer: Option<Timer>) -> Result<(), ExchangeError> {
        self.socket.set_ping_timer(timer);
        Ok(())
    }
}
impl Socket {
    fn send_login(&self) -> ExchangeFuture<'static, ()> {
        let socket = self.me();
        Box::pin(async move {
            let venue = socket.venue()?;
            let options = venue.base.options();
            let runtime = venue.runtime.clone();
            let (nonce, id) = (runtime.clone(), runtime.clone());
            let signer = venue.base.signers.remote_signer.clone();
            let account_name = venue.remote_account_name.clone();
            let request_account = account_name.clone();
            let name = Exchange::name(venue.as_ref()).clone();
            let report = venue.clone();
            let sender = socket.clone();
            crate::websocket_private::send_login(
                || Ok(safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time()),
                || Ok(nonce.unique_id().replace(['-', '_'], "")),
                || Ok(options.key.clone()),
                || async {
                    safe_cex_store::virtual_clock::VIRTUAL_CLOCK
                        .sync_bybit_client_server_time(None)
                        .await;
                    Ok(())
                },
                |ms| {
                    let f = runtime.sleep(ms as u64);
                    async move { f.await.map_err(|e| e.message) }
                },
                || Ok(signer),
                || Ok(options.secret.clone()),
                || Ok(id.unique_id()),
                |_| Ok(account_name.map_or(JsJsonValue::Undefined, JsJsonValue::String)),
                |signer, payload| async move {
                    let value = adapter_json(&payload).map_err(|e| e.message)?;
                    let request: safe_cex_types::ws_auth::WsAuthRequest =
                        serde_json::from_value(value).map_err(text)?;
                    let response = signer
                        .sign(safe_cex_transport::signing::RemoteSignerRequest::WsAuth {
                            exchange_name: name,
                            account_name: request_account,
                            request,
                        })
                        .await
                        .map_err(text)?;
                    match response {
                        safe_cex_transport::signing::RemoteSignerResponse::WsAuth(r) => {
                            to_value(&r).map_err(|e| e.message)
                        }
                        safe_cex_transport::signing::RemoteSignerResponse::Http(_) => {
                            Err("Remote signer answered ws-auth with an HTTP response".into())
                        }
                    }
                },
                |message| report.emit_error(&message).map_err(|e| e.message),
                |frame| sender.send(&frame).map_err(|e| e.message),
            )
            .await
            .map_err(ExchangeError::new)
        })
    }
    fn private_state<T>(
        &self,
        f: impl FnOnce(&mut PrivateSocket<(), bool, Timer>) -> T,
    ) -> Result<T, ExchangeError> {
        self.with(|k| match k {
            Kind::Private(s) => Ok(f(s)),
            _ => Err(ExchangeError::new("Not a private Bitunix socket")),
        })
    }
    fn throttle_slot<'a>(
        s: &'a mut PrivateSocket<(), bool, Timer>,
        field: &str,
    ) -> &'a mut Option<Timer> {
        match field {
            "balanceThrottleTimeout" => &mut s.balance_timer,
            "ordersThrottleTimeout" => &mut s.orders_timer,
            _ => &mut s.positions_timer,
        }
    }
    fn refresh(&self, kind: &'static str) -> Result<(), ExchangeError> {
        let read =
            |field: &'static str| self.private_state(|s| Self::throttle_slot(s, field).clone());
        let active = |t: &Option<Timer>| t.is_some();
        let clear = |t: Option<Timer>| {
            if let Some(t) = t {
                t.cancel();
            }
            Ok(())
        };
        let handler = |name: &'static str| Ok(name);
        let schedule = |name: &'static str, delay: f64| {
            let weak = self.self_ref.clone();
            Ok(Rc::new(self.base.start_timeout(
                delay as u64,
                move || {
                    if let Some(socket) = weak.upgrade() {
                        let work = socket.fetch_throttled(name);
                        socket.base.start_message_operation(work);
                    }
                    Ok(())
                },
            )?))
        };
        let set = |field: &'static str, timer: Timer| {
            self.private_state(|s| *Self::throttle_slot(s, field) = Some(timer))
        };
        match kind {
            "balance" => crate::websocket_private::throttled_fetch_balance(
                read, active, clear, handler, schedule, set,
            ),
            "orders" => crate::websocket_private::throttled_fetch_orders(
                read, active, clear, handler, schedule, set,
            ),
            _ => crate::websocket_private::throttled_fetch_positions(
                read, active, clear, handler, schedule, set,
            ),
        }
    }
    fn fetch_throttled(&self, name: &'static str) -> ExchangeFuture<'static, ()> {
        let socket = self.me();
        Box::pin(async move {
            let venue = socket.venue()?;
            let reset = |field: &'static str| {
                socket.private_state(|s| *Self::throttle_slot(s, field) = None)
            };
            let disposed = || Ok(socket.disposed());
            let loaded = || Ok(venue.base.store.loaded()?.clone());
            match name {
                "fetchThrottledBalance" => {
                    crate::websocket_private::fetch_throttled_balance(
                        reset,
                        disposed,
                        || venue.fetch_balance(),
                        loaded,
                        |balance, positions, mut loaded, _| {
                            loaded.balance = true;
                            loaded.positions = true;
                            venue.base.store.update(
                                StoreUpdate {
                                    balance: Some(balance),
                                    positions: Some(positions),
                                    loaded: Some(loaded),
                                    ..Default::default()
                                },
                                false,
                            )?;
                            Ok(())
                        },
                    )
                    .await
                }
                "fetchThrottledOrders" => {
                    crate::websocket_private::fetch_throttled_orders(
                        reset,
                        disposed,
                        || venue.fetch_orders(),
                        loaded,
                        |_, orders, mut loaded, _| {
                            loaded.orders = true;
                            venue.base.store.update(
                                StoreUpdate {
                                    orders: Some(orders),
                                    loaded: Some(loaded),
                                    ..Default::default()
                                },
                                false,
                            )?;
                            Ok(())
                        },
                    )
                    .await
                }
                _ => {
                    crate::websocket_private::fetch_throttled_positions(
                        reset,
                        disposed,
                        || venue.fetch_positions(),
                        loaded,
                        |_, positions, mut loaded, _| {
                            loaded.positions = true;
                            venue.base.store.update(
                                StoreUpdate {
                                    positions: Some(positions),
                                    loaded: Some(loaded),
                                    ..Default::default()
                                },
                                false,
                            )?;
                            Ok(())
                        },
                    )
                    .await
                }
            }
        })
    }
}
struct PrivateMessages {
    socket: Rc<Socket>,
}
impl PrivateMessages {
    fn venue(&self) -> Result<Rc<BitunixVenue>, Fault> {
        Ok(self.socket.venue()?)
    }
}
impl crate::event_models::PrivateMessageHost for PrivateMessages {
    type Raw = String;
    type Error = Fault;
    fn disposed(&mut self) -> Result<bool, Fault> {
        Ok(self.socket.disposed())
    }
    fn track(&mut self) -> Result<(), Fault> {
        Ok(self.socket.base.track_last_received()?)
    }
    fn count(&mut self) -> Result<f64, Fault> {
        Ok(self.socket.counters("messageCount"))
    }
    fn set_count(&mut self, value: f64) -> Result<(), Fault> {
        self.socket.set_counter("messageCount", value);
        Ok(())
    }
    fn diagnostic(&mut self, raw: &String) -> Result<(), Fault> {
        Ok(self
            .socket
            .base
            .emit_ws_diagnostic_receive(&Value::String(raw.clone()), &Default::default())?)
    }
    fn parse(&mut self, raw: String) -> Result<Input, Fault> {
        // Source jsonParse maps malformed websocket text to null.
        Ok(Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap_or(Input::Null))
    }
    fn balance_notice(&mut self, json: &Input) -> Result<(), Fault> {
        self.venue()?.runtime.console(
            "info",
            &format!(
                "bitunix: received balance message, but we will fetch it from the exchange {}",
                safe_cex_num::js_retained_json_stringify(json)?.unwrap_or("undefined".into())
            ),
        );
        Ok(())
    }
    fn refresh(&mut self, kind: &'static str) -> Result<(), Fault> {
        Ok(self.socket.refresh(kind)?)
    }
    fn orders(&mut self) -> Result<Vec<Input>, Fault> {
        let orders = self.venue()?.base.store.orders()?.to_vec();
        Ok(orders
            .iter()
            .map(crate::adapter::to_input)
            .collect::<Result<_, _>>()?)
    }
    fn positions(&mut self) -> Result<Vec<Input>, Fault> {
        let positions = self.venue()?.base.store.positions()?.to_vec();
        Ok(positions
            .iter()
            .map(crate::adapter::to_input)
            .collect::<Result<_, _>>()?)
    }
    fn add_orders(&mut self, orders: Vec<Input>) -> Result<(), Fault> {
        let venue = self.venue()?;
        let orders: Vec<Order> = venue.orders_from(orders)?;
        Ok(venue
            .base
            .store
            .add_or_update_orders(orders)
            .map_err(ExchangeError::from)?)
    }
    fn key(&mut self, row: &Input, name: &'static str) -> Result<Option<Input>, Fault> {
        for name in [
            name.to_owned(),
            safe_cex_num::lodash_snake_case(name),
            safe_cex_num::lodash_camel_case(name),
        ] {
            if let Some(value) = row.get(&name) {
                return Ok(Some(value.clone()));
            }
            if safe_cex_num::js_table_lookup::<&str, ()>(&[], &name).is_present() {
                return Ok(None);
            }
        }
        Ok(None)
    }

    fn reduce_only(
        &mut self,
        raw: Option<&Input>,
        client: Option<&Input>,
        fallback: Option<&Input>,
    ) -> Result<bool, Fault> {
        let venue = self.venue()?;
        let state = venue.state.borrow();
        Ok(crate::tpsl_helpers::get_bitunix_reduce_only(
            raw,
            client,
            fallback.is_some_and(Input::is_truthy),
            &state.order_reduce_only_cache,
        )?)
    }
    fn cache_tif(&mut self, client: &Input, tif: Input) -> Result<(), Fault> {
        let client = safe_cex_num::js_retained_string(Some(client))?;
        let tif = safe_cex_num::js_retained_string(Some(&tif))?;
        self.venue()?
            .state
            .borrow_mut()
            .order_tif_cache
            .insert(client, tif);
        Ok(())
    }
    fn cache_position(&mut self, key: String, id: Option<&Input>) -> Result<(), Fault> {
        let venue = self.venue()?;
        match id {
            Some(id) => {
                let id = safe_cex_num::js_retained_string(Some(id))?;
                venue.state.borrow_mut().position_id_cache.insert(key, id);
            }
            None => {
                venue.state.borrow_mut().position_id_cache.remove(&key);
            }
        }
        Ok(())
    }
    fn update_position_info(&mut self, position: &Input, info: Input) -> Result<(), Fault> {
        let id = safe_cex_num::js_retained_string(position.get("id"))?;
        let mut patch = safe_cex_store::model_patch::ModelPatch::default();
        patch.set_native("info", info);
        Ok(self
            .venue()?
            .base
            .store
            .update_position(&id, &patch)
            .map_err(ExchangeError::from)?)
    }
    fn fake_full(&mut self) -> Result<bool, Fault> {
        Ok(self.venue()?.state.borrow().enable_fake_full_tpsl)
    }
    fn leg_id(
        &mut self,
        raw: Option<&Input>,
        position: Option<&Input>,
        leg: &'static str,
        disambiguate: bool,
    ) -> Result<String, Fault> {
        let leg = if leg == "tp" {
            crate::tpsl_helpers::TpslLeg::Tp
        } else {
            crate::tpsl_helpers::TpslLeg::Sl
        };
        Ok(crate::tpsl_helpers::get_tpsl_leg_order_id(
            raw,
            position,
            leg,
            disambiguate,
        )?)
    }
    fn clock(&mut self) -> Result<f64, Fault> {
        Ok(self.venue()?.runtime.now_ms())
    }
    fn local_to_utc(&mut self, value: f64) -> f64 {
        value
    }
    fn handled_pong(&mut self) -> Result<bool, Fault> {
        Ok(self.socket.private_state(|s| s.handled_pong)?)
    }
    fn last_pong(&mut self) -> Result<f64, Fault> {
        Ok(self.socket.private_state(|s| s.last_pong)?)
    }
    fn set_last_pong(&mut self, value: f64) -> Result<(), Fault> {
        Ok(self.socket.private_state(|s| s.last_pong = value)?)
    }
    fn set_handled_pong(&mut self, value: bool) -> Result<(), Fault> {
        Ok(self.socket.private_state(|s| s.handled_pong = value)?)
    }
    fn pong(&mut self) -> Result<(), Fault> {
        Ok(self.socket.base.handle_pong_event(5_000)?)
    }
    fn login_diagnostic(&mut self, message: String) -> Result<(), Fault> {
        Ok(self
            .socket
            .base
            .emit_ws_diagnostic_receive(&Value::String(message), &Default::default())?)
    }
    fn login_error(&mut self, message: &'static str) -> Result<(), Fault> {
        Ok(self.venue()?.emit_error(message)?)
    }
    fn spawn_login_retry(&mut self) -> Result<(), Fault> {
        let socket = self.socket.clone();
        self.socket.base.start_message_operation(async move {
            crate::event_models::retry_login(&mut LoginRetry { socket }).await
        });
        Ok(())
    }
}
struct LoginRetry {
    socket: Rc<Socket>,
}
impl crate::event_models::LoginRetryHost for LoginRetry {
    type Error = ExchangeError;
    async fn sync(&mut self) -> Result<(), ExchangeError> {
        safe_cex_store::virtual_clock::VIRTUAL_CLOCK
            .sync_bybit_client_server_time(None)
            .await;
        Ok(())
    }
    fn sleep(&mut self, ms: f64) -> impl Future<Output = Result<(), ExchangeError>> {
        self.socket.runtime().sleep(ms as u64)
    }
    fn launch_login(&mut self) -> Result<(), ExchangeError> {
        let login = self.socket.send_login();
        self.socket.base.start_message_operation(login);
        Ok(())
    }
}
