//! Owned private socket with bound authentication and reconciliation methods.
use crate::native_value::NativeValue;
use crate::{
    adapter::BackpackExchange,
    native_value::Value,
    private_auth as auth, private_events as events,
    public_control::{self as control, JsString},
    socket_events,
};
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, ExchangeTask},
    websocket::{BaseWebSocket, WsProtocol},
};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{OrderSide, OrderStatus, OrderTimeInForce, OrderType, Position, PositionMode};
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
// ts: BackpackPrivateWebSocket
pub struct BackpackPrivateWebSocket {
    pub base: Rc<BaseWebSocket>,
    pub parent: Weak<BackpackExchange>,
    pub runtime: Rc<dyn safe_cex_exchange::websocket::WsRuntime>,
    pub state: events::PrivateState<Rc<Cell<bool>>>,
    pub ping_at: Cell<f64>,
    pub tasks: RefCell<Vec<Box<dyn ExchangeTask>>>,
}
struct Protocol {
    owner: Weak<BackpackPrivateWebSocket>,
}
impl WsProtocol for Protocol {
    fn connect_and_subscribe(&self, _: Rc<BaseWebSocket>, _: bool) -> Result<(), ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Private protocol owner gone"))?
            .connect_and_subscribe()
    }
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Private protocol owner gone"))?
            .ping()
    }
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let owner = self.owner.upgrade();
        Box::pin(async move {
            owner
                .ok_or_else(|| ExchangeError::new("Private protocol owner gone"))?
                .on_open()
                .await
        })
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Private protocol owner gone"))?
            .on_message(frame)
    }
}
impl BackpackPrivateWebSocket {
    pub fn new(parent: Rc<BackpackExchange>) -> Result<Rc<Self>, ExchangeError> {
        static NEXT: AtomicU64 = AtomicU64::new(1_000_000);
        let native = events::construct(|| Ok::<_, ExchangeError>(()))?;
        Ok(Rc::new_cyclic(|weak| Self {
            base: BaseWebSocket::new(
                NEXT.fetch_add(1, Ordering::Relaxed),
                crate::socket_parent::Parent::new(&parent),
                parent.runtime.clone(),
                Rc::new(Protocol {
                    owner: weak.clone(),
                }),
            ),
            runtime: parent.runtime.clone(),
            parent: Rc::downgrade(&parent),
            state: native.state,
            ping_at: Cell::new(0.0),
            tasks: RefCell::new(vec![]),
        }))
    }
    pub fn parent(&self) -> Result<Rc<BackpackExchange>, ExchangeError> {
        self.parent
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Backpack parent disposed"))
    }
    pub fn frame(self: &Rc<Self>) -> Rc<RefCell<PrivateFrame>> {
        Rc::new(RefCell::new(PrivateFrame {
            owner: self.clone(),
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
            |m| self.base.send(&text(&m.to_json())?),
        )
    }
    pub fn authenticate(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        Box::pin(auth::authenticate(AuthFrame {
            owner: self.clone(),
        }))
    }
    pub fn resubscribe(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let owner = self.clone();
        Box::pin(crate::private_control::resubscribe(
            &self.state.subscriptions,
            move || Ok(owner.authenticate()),
        ))
    }
    pub fn subscribe_to_order_updates(
        self: &Rc<Self>,
        symbol: Option<JsString>,
    ) -> ExchangeFuture<'static, ()> {
        Box::pin(auth::subscribe_to_order_updates(
            symbol.as_deref(),
            AuthFrame {
                owner: self.clone(),
            },
        ))
    }
    pub fn subscribe_to_position_updates(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        Box::pin(auth::subscribe_to_position_updates(AuthFrame {
            owner: self.clone(),
        }))
    }
    pub fn on_open(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let owner = self.clone();
        let prepared = if !self.base.is_disposed() {
            self.base
                .track_last_received()
                .map(|()| self.authenticate())
        } else {
            return Box::pin(std::future::ready(Ok(())));
        };
        Box::pin(async move {
            prepared?.await?;
            owner.spawn(owner.resubscribe());
            owner.ping()
        })
    }
    pub fn on_message(self: &Rc<Self>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        let SocketFrame::Text(s) = frame else {
            return Ok(());
        };
        socket_events::on_private_message(
            s,
            &mut PrivateFrame {
                owner: self.clone(),
            },
        )
    }
    pub fn spawn(&self, future: ExchangeFuture<'static, ()>) {
        let runtime = self.runtime.clone();
        let report = runtime.clone();
        self.tasks
            .borrow_mut()
            .push(runtime.spawn_local(Box::pin(async move {
                if let Err(e) = future.await {
                    report.report_exception(
                        &e,
                        &serde_json::json!({"operation":"Backpack private socket"}).into(),
                    );
                }
            })));
    }
    pub fn throttled_fetch_positions(self: &Rc<Self>) -> Result<(), ExchangeError> {
        events::throttled_fetch_positions(self.state.positions.clone(), self.frame())
    }
    pub fn throttled_fetch_balance(self: &Rc<Self>) -> Result<(), ExchangeError> {
        events::throttled_fetch_balance(self.state.balance.clone(), self.frame())
    }
    pub fn fetch_orders_throttled(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let future = events::fetch_throttled_orders(&self.state.orders_timeout, self.frame());
        let runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> = self.runtime.clone();
        safe_cex_exchange::eager::start_eager(&runtime, async move {
            // The source reconciliation catch is silent; the next event retries.
            let _ = future.await;
            Ok(())
        })
    }
    pub fn throttled_fetch_orders(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let cb = self.clone();
        let timer = self.clone();
        crate::private_control::throttled_fetch_orders(
            &self.state.orders_timeout,
            |v| Ok(v.is_some()),
            &Rc::new(move || cb.fetch_orders_throttled()),
            move |cb, delay| {
                let canceled = Rc::new(Cell::new(false));
                let flag = canceled.clone();
                let cb = cb.clone();
                let sleep = timer.runtime.sleep(delay as u64);
                timer.spawn(Box::pin(async move {
                    sleep.await?;
                    if !flag.get() {
                        cb().await?;
                    }
                    Ok(())
                }));
                Ok(canceled)
            },
        )
    }
    pub fn execute_position_fetch(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        Box::pin(events::execute_position_fetch(
            &self.state.positions,
            self.frame(),
        ))
    }
    pub fn execute_balance_fetch(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let future = events::execute_balance_fetch(&self.state.balance, self.frame());
        let runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> = self.runtime.clone();
        safe_cex_exchange::eager::start_eager(&runtime, async move {
            // The source reconciliation catch is silent; the next event retries.
            let _ = future.await;
            Ok(())
        })
    }
    pub fn unsubscribe(&self, streams: control::Streams) -> Result<(), ExchangeError> {
        crate::private_control::unsubscribe(&self.state.subscriptions, &streams, |m| {
            self.base.send(&text(&m.to_json())?)
        })
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.state
            .subscriptions
            .borrow_mut()
            .clone_from(&control::Subscriptions::default());
        if let Some(token) = self.state.orders_timeout.borrow_mut().take() {
            token.set(true);
        }
        self.base.close_current_websocket();
        Ok(())
    }
    pub fn map_order_type(&self, s: &[u16]) -> OrderType {
        crate::private_order_utils::map_order_type(s)
    }
    pub fn map_order_status(&self, s: &[u16]) -> OrderStatus {
        crate::private_order_utils::map_order_status(s)
    }
    pub fn map_time_in_force(&self, s: &[u16]) -> OrderTimeInForce {
        crate::private_order_utils::map_time_in_force(s)
    }
    pub fn determine_trigger_order_type(
        &self,
        symbol: &JsString,
        side: OrderSide,
        price: Option<f64>,
    ) -> Result<OrderType, ExchangeError> {
        let tickers = self
            .parent()?
            .base
            .store
            .tickers()?
            .iter()
            .map(|t| (units(&t.symbol), Some(t.last), Some(t.mark)))
            .collect::<Vec<_>>();
        Ok(crate::private_order_utils::determine_trigger_order_type(
            &tickers, symbol, side, price,
        ))
    }
}
struct AuthFrame {
    owner: Rc<BackpackPrivateWebSocket>,
}
impl auth::AuthHost for AuthFrame {
    type Error = ExchangeError;
    type Signer = ();
    type Emitter = ();
    type SignatureFuture = ExchangeFuture<'static, auth::AuthSignature>;
    fn has_subscription(&mut self, s: &[u16]) -> Result<bool, ExchangeError> {
        Ok(self.owner.state.subscriptions.borrow().contains(s))
    }
    fn add_subscription(&mut self, s: JsString) -> Result<(), ExchangeError> {
        self.owner.state.subscriptions.borrow_mut().insert(s);
        Ok(())
    }
    fn signature_function(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn key(&mut self) -> Result<JsString, ExchangeError> {
        Ok(units(&self.owner.parent()?.base.options().key))
    }
    fn secret(&mut self) -> Result<JsString, ExchangeError> {
        Ok(units(&self.owner.parent()?.base.options().secret))
    }
    fn generate_signature(
        &mut self,
        _: (),
        key: JsString,
        secret: JsString,
        window: f64,
    ) -> Result<Self::SignatureFuture, ExchangeError> {
        let now = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
        let future = crate::signing::generate_ws_auth_signature(
            key,
            secret,
            Some(window),
            move || Ok::<_, ExchangeError>(now),
            |instruction,
             _: crate::signing::BackpackSigningParams<Value>,
             timestamp,
             window,
             secret| {
                let secret = text(&secret)?;
                let response = crate::signing::sign_request(
                    instruction,
                    &safe_cex_num::JsJsonValue::Object(vec![]),
                    timestamp,
                    window,
                    &secret,
                );
                Ok::<_, ExchangeError>(async move {
                    response.await.map(|s| units(&s)).map_err(|message| {
                        // The crypto adapter retains the source error name in its text.
                        // Reconstruct that error before String(error), avoiding an extra
                        // Error prefix in the source's catch-and-log path.
                        match message.split_once(": ") {
                            Some((name @ ("Error" | "RangeError" | "TypeError"), text)) => {
                                let mut error = ExchangeError::new(text);
                                error.name = name.into();
                                error
                            }
                            _ => ExchangeError::new(message),
                        }
                    })
                })
            },
        );
        Ok(Box::pin(
            async move { Ok(Rc::new(RefCell::new(future.await?))) },
        ))
    }
    fn send_subscription(
        &mut self,
        streams: &[JsString],
        signature: &auth::AuthSignature,
    ) -> Result<(), ExchangeError> {
        self.owner.base.send(&text(
            &auth::AuthMessage::snapshot(streams, signature).to_json(),
        )?)
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(units(&e.js_error_string()))
    }
    fn error_emitter(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_error(&mut self, _: (), message: JsString) -> Result<(), ExchangeError> {
        self.owner.parent()?.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(text(&message)?),
            None,
            None,
        )
    }
}
pub struct PrivateFrame {
    pub owner: Rc<BackpackPrivateWebSocket>,
}
impl socket_events::ErrorSink for PrivateFrame {
    type Error = ExchangeError;
    type Emitter = ();
    fn error_emitter(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(units(&e.js_error_string()))
    }
    fn emit_error(&mut self, _: (), s: JsString) -> Result<(), ExchangeError> {
        self.owner.parent()?.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(text(&s)?),
            None,
            None,
        )
    }
}
impl socket_events::MessageHost<String> for PrivateFrame {
    type Value = Value;
    type Dispatcher = socket_events::EventKind;
    fn dispatch_method(&mut self, k: Self::Dispatcher) -> Result<Self::Dispatcher, ExchangeError> {
        Ok(k)
    }
    fn is_disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.base.is_disposed())
    }
    fn track_last_received(&mut self) -> Result<(), ExchangeError> {
        self.owner.base.track_last_received()
    }
    fn parse(&mut self, s: &String) -> Result<Value, ExchangeError> {
        Ok(serde_json::from_str::<serde_json::Value>(s)
            .map(|v| Value::from_prepared(v.into()))
            .unwrap_or(Value::Null))
    }
    fn property(&mut self, v: &Value, k: &str) -> Result<Value, ExchangeError> {
        v.field(k)
    }
    fn stringify(&mut self, v: &Value) -> Result<JsString, ExchangeError> {
        Ok(units(
            &safe_cex_num::js_json_stringify(&v.prepared()?)
                .map_err(|e| ExchangeError::new(e.message))?
                .unwrap_or_else(|| "undefined".into()),
        ))
    }
    fn starts_with(&mut self, v: &Value, p: &str) -> Result<bool, ExchangeError> {
        Ok(v.string()?.starts_with(&units(p)))
    }
    fn pong(&mut self, n: f64) -> Result<(), ExchangeError> {
        self.owner.base.handle_pong_event(n as u64)
    }
    fn dispatch(
        &mut self,
        k: Self::Dispatcher,
        v: Value,
        _: Option<Value>,
    ) -> Result<(), ExchangeError> {
        match k {
            socket_events::EventKind::Order => events::handle_order_update(&v, self),
            socket_events::EventKind::Position => {
                let future = events::handle_position_update(v, self.owner.frame());
                self.owner.spawn(Box::pin(future));
                Ok(())
            }
            _ => Err(ExchangeError::new(
                "Public event reached private dispatcher",
            )),
        }
    }
}
impl events::UpdateHost<Value> for PrivateFrame {
    type Market = safe_cex_types::Market;
    type FillTarget = ();
    type TriggerTarget = ();
    fn trigger_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn field(&mut self, v: &Value, k: &str) -> Result<Option<JsString>, ExchangeError> {
        match v.field(k)? {
            Value::Undefined => Ok(None),
            v => Ok(Some(v.string()?)),
        }
    }
    fn strict_true(&mut self, v: &Value, k: &str) -> Result<bool, ExchangeError> {
        Ok(matches!(v.field(k)?, Value::Bool(true)))
    }
    fn numeric_field(&mut self, v: &Value, k: &str) -> Result<f64, ExchangeError> {
        v.field(k)?.number()
    }
    fn market(&mut self, s: JsString) -> Result<Option<Self::Market>, ExchangeError> {
        let s = text(&s)?;
        Ok(self
            .owner
            .parent()?
            .base
            .store
            .markets()?
            .iter()
            .find(|m| m.id == s)
            .cloned())
    }
    fn market_symbol(&mut self, m: &Self::Market) -> Result<JsString, ExchangeError> {
        Ok(units(&m.symbol))
    }
    fn trigger_type(
        &mut self,
        _: (),
        s: JsString,
        side: OrderSide,
        p: Option<f64>,
    ) -> Result<OrderType, ExchangeError> {
        self.owner.determine_trigger_order_type(&s, side, p)
    }
    fn add_order(&mut self, u: events::OrderUpdate) -> Result<(), ExchangeError> {
        self.owner
            .parent()?
            .base
            .store
            .add_or_update_order(crate::model_bridge::order(u)?)
            .map_err(Into::into)
    }
    fn fill_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_fill(&mut self, _: (), u: events::FillUpdate) -> Result<(), ExchangeError> {
        use safe_cex_exchange::js_runtime::value::{Js, from_typed};
        self.owner.parent()?.base.emit(
            "fill",
            Js::object(vec![
                ("orderId", Js::str(&text(&u.order_id)?)),
                ("symbol", Js::str(&text(&u.symbol)?)),
                ("side", from_typed(&u.side)?),
                ("price", Js::Number(u.price)),
                ("amount", Js::Number(u.amount)),
                ("timestamp", Js::Number(u.timestamp)),
            ]),
        )
    }
    fn fetch_positions_throttled(&mut self) -> Result<(), ExchangeError> {
        self.owner.throttled_fetch_positions()
    }
    fn fetch_balance_throttled(&mut self) -> Result<(), ExchangeError> {
        self.owner.throttled_fetch_balance()
    }
    fn fetch_orders_throttled(&mut self) -> Result<(), ExchangeError> {
        self.owner.throttled_fetch_orders()
    }
}
impl events::PositionHost<Value> for PrivateFrame {
    type ModeFuture = ExchangeFuture<'static, PositionMode>;
    type StoredPosition = Position;
    type PositionList = Vec<Position>;
    type ReplaceTarget = ();
    fn replace_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn position_mode(&mut self) -> Result<Self::ModeFuture, ExchangeError> {
        Ok(self.owner.parent()?.get_position_mode())
    }
    fn positions(&mut self) -> Result<Vec<Position>, ExchangeError> {
        Ok(self.owner.parent()?.base.store.positions()?.to_vec())
    }
    fn positions_len(&mut self, p: &Vec<Position>) -> Result<usize, ExchangeError> {
        Ok(p.len())
    }
    fn position_at(
        &mut self,
        p: &Vec<Position>,
        i: usize,
    ) -> Result<Option<Position>, ExchangeError> {
        Ok(p.get(i).cloned())
    }
    fn position_id(&mut self, p: &Position) -> Result<JsString, ExchangeError> {
        Ok(units(&p.id))
    }
    fn replace_positions(&mut self, _: (), p: Vec<Position>) -> Result<(), ExchangeError> {
        self.owner
            .parent()?
            .base
            .store
            .update(
                safe_cex_store::StoreUpdate {
                    positions: Some(p),
                    ..Default::default()
                },
                false,
            )
            .map_err(Into::into)
    }
    fn add_position(&mut self, p: events::PositionUpdate) -> Result<(), ExchangeError> {
        self.owner
            .parent()?
            .base
            .store
            .add_or_update_position(crate::model_bridge::private_position(p)?)
            .map_err(Into::into)
    }
}
pub enum Fetched {
    Positions(Vec<Position>),
    Balance(safe_cex_types::Balance),
    Orders(Vec<safe_cex_types::Order>),
}
impl events::FetchHost for PrivateFrame {
    type Error = ExchangeError;
    type Value = Fetched;
    type OrdersTarget = ();
    type FetchFuture = ExchangeFuture<'static, Fetched>;
    fn orders_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn now(&mut self) -> Result<f64, ExchangeError> {
        Ok(self.owner.runtime.now_ms())
    }
    fn disposed(&mut self) -> Result<bool, ExchangeError> {
        Ok(self.owner.base.is_disposed())
    }
    fn execute(&mut self, k: events::FetchKind) -> Result<(), ExchangeError> {
        let f = match k {
            events::FetchKind::Positions => self.owner.execute_position_fetch(),
            events::FetchKind::Balance => self.owner.execute_balance_fetch(),
            events::FetchKind::Orders => self.owner.fetch_orders_throttled(),
        };
        self.owner.spawn(f);
        Ok(())
    }
    fn timeout(
        &mut self,
        cb: events::TimeoutCallback<ExchangeError>,
        delay: f64,
    ) -> Result<(), ExchangeError> {
        let sleep = self.owner.runtime.sleep(delay as u64);
        self.owner.spawn(Box::pin(async move {
            sleep.await?;
            cb()
        }));
        Ok(())
    }
    fn fetch(&mut self, k: events::FetchKind) -> Result<Self::FetchFuture, ExchangeError> {
        let o = self.owner.parent()?;
        Ok(match k {
            events::FetchKind::Positions => {
                let f = o.fetch_positions();
                Box::pin(async move {
                    Ok(Fetched::Positions(
                        f.await?
                            .into_iter()
                            .map(crate::model_bridge::position)
                            .collect::<Result<_, _>>()?,
                    ))
                })
            }
            events::FetchKind::Balance => {
                let f = o.fetch_balance();
                Box::pin(async move { Ok(Fetched::Balance(f.await?)) })
            }
            events::FetchKind::Orders => {
                let f = o.fetch_orders(None);
                Box::pin(
                    async move { Ok(Fetched::Orders(f.await?.into_iter().flatten().collect())) },
                )
            }
        })
    }
    fn update(&mut self, _: events::FetchKind, v: Fetched) -> Result<(), ExchangeError> {
        let mut u = safe_cex_store::StoreUpdate::default();
        match v {
            Fetched::Positions(p) => u.positions = Some(p),
            Fetched::Balance(b) => u.balance = Some(b),
            Fetched::Orders(o) => u.orders = Some(o),
        };
        self.owner
            .parent()?
            .base
            .store
            .update(u, false)
            .map_err(Into::into)
    }
    fn loaded(&mut self) -> Result<Vec<(JsString, bool)>, ExchangeError> {
        crate::adapter_runtime::loaded(&self.owner.parent()?.base)
    }
    fn update_orders(
        &mut self,
        _: (),
        v: Fetched,
        l: Vec<(JsString, bool)>,
    ) -> Result<(), ExchangeError> {
        let Fetched::Orders(o) = v else {
            return Err(ExchangeError::new("Orders response kind mismatch"));
        };
        self.owner
            .parent()?
            .base
            .store
            .update(
                safe_cex_store::StoreUpdate {
                    orders: Some(o),
                    loaded: Some(crate::adapter_runtime::loaded_patch(l)?),
                    ..Default::default()
                },
                false,
            )
            .map_err(Into::into)
    }
}
