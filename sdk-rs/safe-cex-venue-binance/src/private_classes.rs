//! Private class assembly over imported Base, timer, HTTP and parent operations.
//! The dependency interfaces expose primitives and named parent operations;
//! private venue methods execute here through the existing body ports.
use crate::{
    api_factory::{RemoteSignCall, SpotAuthBindings},
    class_jobs::{self, NativeFuture, Settlement},
    listen_key::{
        KeepaliveFailure, KeepaliveState, ListenKeyResponse, fetch_binance_listen_key,
        fetch_binance_pm_listen_key, update_binance_listen_key, update_binance_pm_listen_key,
    },
    position_refresh::{
        PositionFetchStarter, PositionFetchState, execute_binance_pm_position_fetch,
        execute_binance_position_fetch, throttle_binance_pm_positions, throttle_binance_positions,
    },
    private_event_value::{EventArray, EventOps, EventValue, number, record, string},
    private_events::{
        PrivateEventBindings, PrivateHandler, PrivateOperation, handle_futures_account_events,
        handle_futures_algo_order_events, handle_futures_order_events, handle_pm_account_events,
        handle_pm_conditional_order_events, handle_pm_order_events, handle_spot_account_events,
        handle_spot_order_events,
    },
    private_session::{
        ConnectBindings, ConnectState, MessageOp, RetryAction, SpotMessageBindings, Venue,
    },
    request_weight::Failure,
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeRuntime, LogSeverity};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type Action = Rc<dyn Fn() -> Result<(), Failure>>;
pub trait NativePrivateSocket {
    fn remove_listener(
        &self,
        event: &'static str,
        callback: &PrivateCallback,
    ) -> Result<(), Failure>;
    fn ready_state(&self) -> Result<f64, Failure>;
    fn close(&self) -> Result<(), Failure>;
    fn add_listener(&self, event: &'static str, action: PrivateCallback) -> Result<(), Failure>;
}
#[derive(Clone)]
pub enum PrivateCallback {
    Message(Rc<dyn Fn(EventValue) -> Result<(), Failure>>),
    Action(Action),
}
impl PrivateCallback {
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Message(a), Self::Message(b)) => Rc::ptr_eq(a, b),
            (Self::Action(a), Self::Action(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
enum WeakCallback {
    Message(std::rc::Weak<dyn Fn(EventValue) -> Result<(), Failure>>),
    Action(std::rc::Weak<dyn Fn() -> Result<(), Failure>>),
}
impl WeakCallback {
    fn new(c: &PrivateCallback) -> Self {
        match c {
            PrivateCallback::Message(f) => Self::Message(Rc::downgrade(f)),
            PrivateCallback::Action(f) => Self::Action(Rc::downgrade(f)),
        }
    }
    fn upgrade(&self) -> Option<PrivateCallback> {
        match self {
            Self::Message(f) => f.upgrade().map(PrivateCallback::Message),
            Self::Action(f) => f.upgrade().map(PrivateCallback::Action),
        }
    }
}
/// Imported BaseWebSocket operations. This is not a venue-method executor.
pub trait PrivateBase {
    fn disposed(&self) -> bool;
    fn force_closed(&self) -> bool;
    fn connected(&self) -> bool;
    fn track(&self) -> Result<(), Failure>;
    fn diagnostic(&self, data: EventValue) -> Result<(), Failure>;
    fn stats(&self, topic: &str) -> Result<(), Failure>;
    fn set_ping_at(&self, time: f64) -> Result<(), Failure>;
    fn diagnostic_send(&self, payload: &str) -> Result<(), Failure>;
    fn pong(&self) -> Result<(), Failure>;
    fn send(&self, payload: String) -> Result<(), Failure>;
    fn send_with_log(&self, payload: String) -> Result<(), Failure> {
        self.send(payload.clone())?;
        self.diagnostic_send(&payload)
    }
    fn create(&self, url: String) -> Result<Rc<dyn NativePrivateSocket>, Failure>;
    fn on_close(&self) -> Result<(), Failure>;
    fn force_close(&self) -> Result<(), Failure>;
    fn reconnect(&self) -> Result<(), Failure>;
    fn dispose(&self) -> Result<(), Failure>;
    fn emit_error(&self, message: String) -> Result<(), Failure>;
    /// Hosts whose socket core drives ping/reconnect call back into these.
    fn install_hooks(&self, _ping: Action, _connect: Action) {}
    /// The core socket backing this base, registered as the exchange's private slot.
    fn core_socket(&self) -> Option<Rc<safe_cex_exchange::websocket::BaseWebSocket>> {
        None
    }
}
/// The source parent methods; a concrete exchange implements this interface.
pub trait PrivateParent {
    fn options(&self) -> Result<EventValue, Failure>;
    fn orders(&self) -> Result<EventArray, Failure>;
    fn positions(&self) -> Result<EventArray, Failure>;
    fn balance(&self) -> Result<EventValue, Failure>;
    fn loaded(&self) -> Result<EventValue, Failure>;
    fn capture_store(&self, operation: PrivateOperation) -> Result<PrivateHandler, Failure>;
    fn fill(&self, payload: EventValue) -> Result<(), Failure>;
    fn asset_price(&self, asset: &EventValue) -> Result<f64, Failure>;
    fn mark_position(&self) -> Result<(), Failure>;
    fn position_version(&self) -> Result<f64, Failure>;
    fn paused(&self) -> Result<bool, Failure>;
    fn orders_stale(&self) -> Result<bool, Failure>;
    fn fetch_positions(&self) -> Result<NativeFuture<EventValue>, Failure>;
    fn fetch_orders(&self) -> Result<NativeFuture<EventValue>, Failure>;
    fn replace_positions(&self, positions: EventValue) -> Result<(), Failure>;
    fn post_key(&self, endpoint: &'static str) -> Result<PinKey, Failure>;
    fn put_key(&self, endpoint: &'static str) -> Result<NativeFuture<()>, Failure>;
    /// The exchange as a core socket parent, for hosts that back the base with BaseWebSocket.
    fn socket_parent(&self) -> Option<Rc<dyn safe_cex_exchange::websocket::WsParent>> {
        None
    }
    fn remote_sign(&self, signer: EventValue) -> Result<RemoteSignCall, Failure>;
    fn log(&self, message: String, severity: LogSeverity) -> Result<(), Failure>;
}
pub type PinKey = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ListenKeyResponse<EventValue>, Failure>>>,
>;
pub trait PrivateTimers {
    fn set(&self, action: CapturedAction, delay: f64) -> Result<EventValue, Failure>;
    fn clear(&self, ticket: EventValue) -> Result<(), Failure>;
}
pub struct PrivateDependencies {
    pub parent: Rc<dyn PrivateParent>,
    pub base_factory: Rc<dyn Fn(Rc<dyn PrivateParent>) -> Result<Rc<dyn PrivateBase>, Failure>>,
    pub timers: Rc<dyn PrivateTimers>,
    pub runtime: Rc<dyn ExchangeRuntime>,
    pub virtual_now: Rc<dyn Fn() -> f64>,
}
pub struct PrivateSocket {
    state: Rc<PrivateState>,
    pub base: Rc<dyn PrivateBase>,
    _callbacks: Vec<PrivateCallback>,
}
impl std::ops::Deref for PrivateSocket {
    type Target = Rc<PrivateState>;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}
pub struct PrivateState {
    prototype_realm: Rc<safe_cex_num::PrototypeRealm>,
    pub venue: Venue,
    pub parent: Rc<dyn PrivateParent>,
    base: std::rc::Weak<dyn PrivateBase>,
    pub connect: ConnectState,
    pub position: Rc<PositionFetchState>,
    pub refresh_timer: RefCell<EventValue>,
    pub orders_timer: RefCell<EventValue>,
    pub keepalive_failing: KeepaliveState,
    pub has_opened: Cell<bool>,
    pub subscription_attempt: Cell<f64>,
    this: std::rc::Weak<PrivateState>,
    callbacks: RefCell<Vec<(&'static str, WeakCallback)>>,
    socket: RefCell<Option<std::rc::Weak<dyn NativePrivateSocket>>>,
    virtual_now: Rc<dyn Fn() -> f64>,
    timers: Rc<dyn PrivateTimers>,
    runtime: Rc<dyn ExchangeRuntime>,
    /// Named replaceable arrow members. Captures retain the currently selected
    /// method, while timer wrappers explicitly resolve the current method at fire.
    pub(crate) update_key: RefCell<Action>,
    pub(crate) fetch_throttled: RefCell<Action>,
    pub(crate) execute_positions: RefCell<Action>,
    pub(crate) throttle_positions: RefCell<Action>,
    pub(crate) connect_method: RefCell<Action>,
}
fn err(e: KeepaliveFailure) -> Failure {
    match e {
        KeepaliveFailure::Error(e) => e,
        KeepaliveFailure::Other(v) => Rc::new(ExchangeError::new(
            safe_cex_num::js_string(v.as_deref()).unwrap_or_else(|_| "undefined".into()),
        )),
    }
}
fn keep(e: Failure) -> KeepaliveFailure {
    KeepaliveFailure::Error(e)
}
#[derive(Clone)]
pub struct CapturedAction {
    function: Action,
    _receiver: Rc<PrivateState>,
    _base: Rc<dyn PrivateBase>,
}
impl CapturedAction {
    pub fn call(&self) -> Result<(), Failure> {
        let _binding = self._receiver.prototype_realm.bind();
        (self.function)()
    }
    pub fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.function, &other.function)
    }
}
impl PrivateSocket {
    pub fn capture_update_key(&self) -> CapturedAction {
        CapturedAction {
            function: self.state.update_key.borrow().clone(),
            _receiver: self.state.clone(),
            _base: self.base.clone(),
        }
    }
    pub fn capture_fetch_throttled(&self) -> CapturedAction {
        CapturedAction {
            function: self.state.fetch_throttled.borrow().clone(),
            _receiver: self.state.clone(),
            _base: self.base.clone(),
        }
    }
    pub fn capture_connect(&self) -> CapturedAction {
        CapturedAction {
            function: self.state.connect_method.borrow().clone(),
            _receiver: self.state.clone(),
            _base: self.base.clone(),
        }
    }

    pub fn new(venue: Venue, d: PrivateDependencies) -> Result<Rc<Self>, Failure> {
        let base = (d.base_factory)(d.parent.clone())?;
        // The default closures receive weak owners only until construction completes;
        // exported callbacks/timers below acquire an owning lease before returning.
        let state = Rc::new_cyclic(|weak: &std::rc::Weak<PrivateState>| {
            let bind = |method: fn(Rc<PrivateState>) -> Result<(), Failure>| {
                let weak = weak.clone();
                Rc::new(move || {
                    method(
                        weak.upgrade()
                            .ok_or_else(|| Rc::new(ExchangeError::new("private owner dropped")))?,
                    )
                }) as Action
            };
            PrivateState {
                prototype_realm: safe_cex_num::PrototypeRealm::current(),
                this: weak.clone(),
                venue,
                parent: d.parent,
                base: Rc::downgrade(&base),
                connect: ConnectState {
                    disposed: crate::class_flags::LiveFlag::read({
                        let base = Rc::downgrade(&base);
                        Rc::new(move || base.upgrade().is_none_or(|base| base.disposed()))
                    }),
                    force_closed: crate::class_flags::LiveFlag::read({
                        let base = Rc::downgrade(&base);
                        Rc::new(move || base.upgrade().is_none_or(|base| base.force_closed()))
                    }),
                    ..Default::default()
                },
                position: Rc::new(PositionFetchState {
                    is_disposed: crate::class_flags::LiveFlag::read({
                        let base = Rc::downgrade(&base);
                        Rc::new(move || base.upgrade().is_none_or(|base| base.disposed()))
                    }),
                    pending_events: Cell::new(false),
                    scheduled: Cell::new(false),
                    in_flight: Cell::new(false),
                    last_fetch_time: Cell::new(0.0),
                    throttle_ms: Cell::new(if venue == Venue::PM { 500.0 } else { 10000.0 }),
                }),
                refresh_timer: RefCell::new(EventValue::Undefined),
                orders_timer: RefCell::new(EventValue::Undefined),
                keepalive_failing: KeepaliveState {
                    is_disposed: crate::class_flags::LiveFlag::read({
                        let base = Rc::downgrade(&base);
                        Rc::new(move || base.upgrade().is_none_or(|base| base.disposed()))
                    }),
                    keepalive_failing: Cell::new(false),
                },
                has_opened: Cell::new(false),
                subscription_attempt: Cell::new(0.0),
                callbacks: Default::default(),
                socket: Default::default(),
                virtual_now: d.virtual_now,
                timers: d.timers,
                runtime: d.runtime,
                update_key: RefCell::new(bind(PrivateState::start_update_key)),
                fetch_throttled: RefCell::new(bind(PrivateState::start_fetch_throttled)),
                execute_positions: RefCell::new(bind(PrivateState::start_execute_positions)),
                throttle_positions: RefCell::new(bind(|s| s.throttled_positions())),
                connect_method: RefCell::new(bind(PrivateState::start_connect)),
            }
        });
        let callbacks = ["message", "close", "open"]
            .into_iter()
            .map(|event| state.callback(event))
            .collect();
        let (ping, connect) = (Rc::downgrade(&state), Rc::downgrade(&state));
        base.install_hooks(
            Rc::new(move || ping.upgrade().map_or(Ok(()), |state| state.ping())),
            Rc::new(move || {
                if let Some(state) = connect.upgrade() {
                    class_jobs::detach(state.runtime.clone(), state.connect_and_subscribe());
                }
                Ok(())
            }),
        );
        Ok(Rc::new(Self {
            state,
            base,
            _callbacks: callbacks,
        }))
    }
}
impl PrivateState {
    fn set_timer(&self, function: Action, delay: f64) -> Result<EventValue, Failure> {
        self.timers.set(
            CapturedAction {
                function,
                _receiver: self.this.upgrade().expect("live timer receiver"),
                _base: self.base(),
            },
            delay,
        )
    }
    fn base(&self) -> Rc<dyn PrivateBase> {
        self.base.upgrade().expect("live imported Base lease")
    }
    fn callback(&self, event: &'static str) -> PrivateCallback {
        if let Some(callback) = self
            .callbacks
            .borrow()
            .iter()
            .filter(|(name, _)| *name == event)
            .find_map(|(_, callback)| callback.upgrade())
        {
            return callback;
        }
        let owner = self.this.upgrade().expect("live receiver");
        let base = self.base();
        let callback = match event {
            "message" => PrivateCallback::Message(Rc::new(move |data| {
                let _base_lease = &base;
                owner.on_message(data)
            })),
            "open" => PrivateCallback::Action(Rc::new(move || {
                let _binding = owner.prototype_realm.bind();
                let _base_lease = &base;
                if owner.venue == Venue::Spot {
                    class_jobs::detach(owner.runtime.clone(), owner.on_open());
                    Ok(())
                } else {
                    owner.on_open_sync()
                }
            })),
            _ => PrivateCallback::Action(Rc::new(move || {
                let _binding = owner.prototype_realm.bind();
                let _base_lease = &base;
                owner.base().on_close()
            })),
        };
        self.callbacks
            .borrow_mut()
            .push((event, WeakCallback::new(&callback)));
        callback
    }

    pub fn connect_and_subscribe(self: &Rc<Self>) -> Settlement<()> {
        let _binding = self.prototype_realm.bind();
        let owner = self.clone();
        let base = self.base();
        class_jobs::start(
            self.runtime.clone(),
            Box::pin(async move {
                let _base_lease = base;
                match owner.venue {
                    Venue::Futures => crate::private_session::connect_futures(owner).await,
                    Venue::PM => crate::private_session::connect_pm(owner).await,
                    Venue::Spot => crate::private_session::connect_spot(owner).await,
                }
            }),
        )
    }
    fn start_connect(owner: Rc<Self>) -> Result<(), Failure> {
        class_jobs::detach(owner.runtime.clone(), owner.connect_and_subscribe());
        Ok(())
    }
    pub fn ping(&self) -> Result<(), Failure> {
        let _binding = self.prototype_realm.bind();
        if !self.base().disposed() {
            self.base().set_ping_at(self.runtime.monotonic_now_ms())?;
            self.base().send_with_log(
                (if self.venue == Venue::Spot {
                    r#"{"id":42,"method":"session.status"}"#
                } else {
                    r#"{"id":42,"method":"LIST_SUBSCRIPTIONS"}"#
                })
                .into(),
            )?;
        }
        Ok(())
    }
    fn on_open_sync(self: &Rc<Self>) -> Result<(), Failure> {
        let _binding = self.prototype_realm.bind();
        if self.base().disposed() {
            return Ok(());
        }
        self.ping()?;
        if self.connect.resync_orders.get() || self.has_opened.get() {
            self.connect.resync_orders.set(false);
            self.throttled_orders()?;
        }
        self.has_opened.set(true);
        Ok(())
    }
    pub fn on_open(self: &Rc<Self>) -> Settlement<()> {
        let _binding = self.prototype_realm.bind();
        let owner = self.clone();
        let base = self.base();
        class_jobs::start(
            self.runtime.clone(),
            Box::pin(async move {
                let _base_lease = base;
                if owner.base().disposed() {
                    return Ok(());
                }
                if owner.venue != Venue::Spot {
                    return owner.on_open_sync();
                }
                let attempt = owner.subscription_attempt.get() + 1.0;
                owner.subscription_attempt.set(attempt);
                let result: Result<(), Failure> = async {
                    let mut b = Auth(owner.clone());
                    let params = class_jobs::reaction(
                        crate::api_factory::build_spot_user_data_params(&mut b),
                    )
                    .await?;
                    if owner.base().disposed() || attempt != owner.subscription_attempt.get() {
                        return Ok(());
                    }
                    owner
                        .base()
                        .send_with_log(crate::public_classes::encode(&record([
                            (
                                "id",
                                string(crate::spot_protocol::BINANCE_SPOT_USER_DATA_REQUEST_ID),
                            ),
                            ("method", string("userDataStream.subscribe.signature")),
                            ("params", params),
                        ]))?)
                }
                .await;
                if let Err(e) = result {
                    owner.base().emit_error(if e.message.is_empty() {
                        "Binance Spot user-data subscription signing failed".into()
                    } else {
                        e.message.clone()
                    })?;
                    owner.base().force_close()?;
                }
                Ok(())
            }),
        )
    }
    pub fn on_message(self: &Rc<Self>, data: EventValue) -> Result<(), Failure> {
        let _binding = self.prototype_realm.bind();
        if self.venue == Venue::Spot {
            return crate::private_session::on_spot_message(|| Ok(data), &Message(self.clone()));
        }
        if self.base().disposed() {
            return Ok(());
        }
        self.base().track()?;
        self.base().diagnostic(data.clone())?;
        let json = if self.venue == Venue::Futures {
            crate::private_session::parse_big_data(data)
        } else {
            crate::private_session::parse_data(data)
        };
        for (event, topic, kind) in [
            ("ACCOUNT_UPDATE", "position", 0),
            ("ORDER_TRADE_UPDATE", "order", 1),
            (
                if self.venue == Venue::PM {
                    "CONDITIONAL_ORDER_TRADE_UPDATE"
                } else {
                    "ALGO_UPDATE"
                },
                if self.venue == Venue::PM {
                    "conditional-order"
                } else {
                    "algo_order"
                },
                2,
            ),
        ] {
            if !json.nullish() && json.read("e")?.same(&string(event)) {
                self.base().stats(topic)?;
                self.handle_events(kind, EventValue::array(vec![json.clone()]))?;
            }
        }
        if !json.nullish() && json.read("id")?.same(&number(42.0)) {
            self.base().pong()?;
        }
        Ok(())
    }
    pub fn handle_events(self: &Rc<Self>, kind: u8, events: EventValue) -> Result<(), Failure> {
        let _binding = self.prototype_realm.bind();
        let EventValue::Array(events) = events else {
            return Err(crate::request_weight::type_error(
                "events.forEach is not a function",
            ));
        };
        let mut b = Events(self.clone());
        match (self.venue, kind) {
            (Venue::Futures, 0) => {
                handle_futures_account_events(&events, &self.position.pending_events, &mut b)
            }
            (Venue::Futures, 1) => handle_futures_order_events(&events, &mut b),
            (Venue::Futures, _) => handle_futures_algo_order_events(&events, &mut b),
            (Venue::PM, 0) => {
                handle_pm_account_events(&events, &self.position.pending_events, &mut b)
            }
            (Venue::PM, 1) => handle_pm_order_events(&events, &mut b),
            (Venue::PM, _) => handle_pm_conditional_order_events(&events, &mut b),
            (Venue::Spot, 0) => handle_spot_account_events(&events, &mut b),
            (Venue::Spot, _) => handle_spot_order_events(&events, &mut b),
        }
    }
    pub fn throttled_orders(self: &Rc<Self>) -> Result<(), Failure> {
        let old = self.orders_timer.borrow().clone();
        if old.truthy() {
            self.timers.clear(old)?;
        }
        let selected = self.fetch_throttled.borrow().clone();
        let ticket = self.set_timer(selected, 10000.0)?;
        *self.orders_timer.borrow_mut() = ticket;
        Ok(())
    }
    fn start_fetch_throttled(owner: Rc<Self>) -> Result<(), Failure> {
        let retained = owner.clone();
        let base = owner.base();
        class_jobs::detach(
            owner.runtime.clone(),
            class_jobs::start(
                owner.runtime.clone(),
                Box::pin(async move {
                    let _base_lease = base;
                    *retained.orders_timer.borrow_mut() = EventValue::Undefined;
                    if !retained.base().disposed() {
                        let _ = async {
                            let orders =
                                class_jobs::reaction(retained.parent.fetch_orders()?).await?;
                            if retained.base().disposed() || retained.parent.orders_stale()? {
                                return Ok::<(), Failure>(());
                            }
                            let update = retained
                                .parent
                                .capture_store(PrivateOperation::UpdateStore)?;
                            let loaded =
                                EventValue::record_object(retained.parent.loaded()?.spread());
                            loaded.write("orders", EventValue::Bool(true));
                            update(vec![record([("orders", orders), ("loaded", loaded)])])
                        }
                        .await;
                    }
                    Ok(())
                }),
            ),
        );
        Ok(())
    }
    pub fn throttled_positions(self: &Rc<Self>) -> Result<(), Failure> {
        let _binding = self.prototype_realm.bind();
        let current = self.clone();
        let execute: PositionFetchStarter<Failure> = Rc::new(move || {
            let selected = current.execute_positions.borrow().clone();
            selected()
        });
        if self.venue == Venue::PM {
            throttle_binance_pm_positions(
                &self.position,
                || Ok(self.runtime.now_ms()),
                |action, delay| self.set_timer(action, delay),
                execute,
            )
        } else {
            throttle_binance_positions(
                &self.position,
                || Ok(self.runtime.now_ms()),
                || self.parent.paused(),
                |action, delay| self.set_timer(action, delay),
                execute,
            )
        }
    }
    fn start_execute_positions(owner: Rc<Self>) -> Result<(), Failure> {
        let retained = owner.clone();
        let base = owner.base();
        class_jobs::detach(
            owner.runtime.clone(),
            class_jobs::start(
                owner.runtime.clone(),
                Box::pin(async move {
                    let _base_lease = base;
                    let state = retained.position.clone();
                    if retained.venue == Venue::PM {
                        execute_binance_pm_position_fetch(
                            &state,
                            || Ok(retained.runtime.now_ms()),
                            || retained.parent.fetch_positions().map(class_jobs::reaction),
                            |rows| retained.parent.replace_positions(rows),
                        )
                        .await;
                    } else {
                        execute_binance_position_fetch(
                            &state,
                            || retained.parent.paused(),
                            || retained.parent.position_version(),
                            || Ok(retained.runtime.now_ms()),
                            || retained.parent.fetch_positions().map(class_jobs::reaction),
                            |rows| retained.parent.replace_positions(rows),
                            || {
                                let method = retained.throttle_positions.borrow().clone();
                                method()
                            },
                        )
                        .await?;
                    }
                    Ok(())
                }),
            ),
        );
        Ok(())
    }
    pub fn schedule_refresh(self: &Rc<Self>, delay: f64) -> Result<(), Failure> {
        let old = self.refresh_timer.borrow().clone();
        self.timers.clear(old)?;
        let owner = self.clone();
        let ticket = self.set_timer(
            Rc::new(move || {
                let method = owner.update_key.borrow().clone();
                method()
            }),
            delay,
        )?;
        *self.refresh_timer.borrow_mut() = ticket;
        Ok(())
    }
    pub async fn fetch_listen_key(self: &Rc<Self>) -> Result<EventValue, Failure> {
        let post = |ep| {
            self.parent
                .post_key(ep)
                .map(|future| async move { class_jobs::reaction(future).await.map_err(keep) })
                .map_err(keep)
        };
        let schedule = |delay| self.schedule_refresh(delay).map_err(keep);
        let value = if self.venue == Venue::PM {
            fetch_binance_pm_listen_key(post, schedule).await
        } else {
            fetch_binance_listen_key(post, schedule).await
        }
        .map_err(err)?;
        Ok(value.unwrap_or(EventValue::Undefined))
    }
    fn start_update_key(owner: Rc<Self>) -> Result<(), Failure> {
        let retained = owner.clone();
        let base = owner.base();
        class_jobs::detach(
            owner.runtime.clone(),
            class_jobs::start(
                owner.runtime.clone(),
                Box::pin(async move {
                    let _base_lease = base;
                    let put = |ep| {
                        retained
                            .parent
                            .put_key(ep)
                            .map(|future| async move {
                                class_jobs::reaction(future).await.map_err(keep)
                            })
                            .map_err(keep)
                    };
                    let schedule = |delay| retained.schedule_refresh(delay).map_err(keep);
                    let warning = |msg: &str| {
                        retained
                            .parent
                            .log(msg.into(), LogSeverity::Warning)
                            .map_err(keep)
                    };
                    let emit = |msg: &str| retained.base().emit_error(msg.into()).map_err(keep);
                    if retained.venue == Venue::PM {
                        update_binance_pm_listen_key(
                            &retained.keepalive_failing,
                            put,
                            schedule,
                            warning,
                            emit,
                        )
                        .await
                    } else {
                        update_binance_listen_key(
                            &retained.keepalive_failing,
                            put,
                            schedule,
                            warning,
                            emit,
                        )
                        .await
                    }
                    .map_err(err)
                }),
            ),
        );
        Ok(())
    }
    pub fn dispose(&self) -> Result<(), Failure> {
        if self.venue != Venue::Spot {
            let retry = self.connect.retry_timer.borrow().clone();
            self.timers.clear(retry)?;
            let refresh = self.refresh_timer.borrow().clone();
            self.timers.clear(refresh)?;
        }
        self.base().dispose()?;
        Ok(())
    }
}
impl ConnectBindings for PrivateState {
    fn state(&self) -> &ConnectState {
        &self.connect
    }
    fn options(&self) -> Result<EventValue, Failure> {
        self.parent.options()
    }
    fn clear_timeout(&self, ticket: EventValue) -> Result<(), Failure> {
        self.timers.clear(ticket)
    }
    fn fetch_native_key(&self) -> Result<crate::private_session::NativeKeyFuture, Failure> {
        let owner = self.this.upgrade().expect("live receiver");
        let task = class_jobs::start(
            self.runtime.clone(),
            Box::pin(async move { owner.fetch_listen_key().await }),
        );
        Ok(Box::pin(class_jobs::reaction(task)))
    }
    fn emit(&self, msg: String) -> Result<(), Failure> {
        self.base().emit_error(msg)
    }
    fn log(&self, msg: String, severity: LogSeverity) -> Result<(), Failure> {
        self.parent.log(msg, severity)
    }
    fn set_timeout(&self, action: RetryAction, delay: f64) -> Result<EventValue, Failure> {
        self.set_timer(action, delay)
    }
    fn start_connect(&self) -> Result<(), Failure> {
        let method = self.connect_method.borrow().clone();
        method()
    }
    fn detach(&self) -> Result<(), Failure> {
        let old = self
            .socket
            .borrow()
            .as_ref()
            .and_then(std::rc::Weak::upgrade);
        crate::private_helpers::detach_binance_private_socket(
            || Ok(old),
            |s, e| {
                let callback = self.callback(e);
                s.remove_listener(e, &callback)
            },
            |s| s.ready_state(),
            |s| s.close(),
        )
    }
    fn create(&self, url: String) -> Result<EventValue, Failure> {
        let socket = self.base().create(url)?;
        *self.socket.borrow_mut() = Some(Rc::downgrade(&socket));
        Ok(record([]))
    }
    fn add_listener(&self, _ws: EventValue, event: &'static str) -> Result<(), Failure> {
        let callback = self.callback(event);
        let socket = self
            .socket
            .borrow()
            .as_ref()
            .and_then(std::rc::Weak::upgrade)
            .expect("created socket retained by imported Base");
        socket.add_listener(event, callback)
    }
}
struct Events(Rc<PrivateState>);
impl PrivateEventBindings for Events {
    fn capture(&mut self, op: PrivateOperation) -> Result<PrivateHandler, Failure> {
        let owner = self.0.clone();
        Ok(match op {
            PrivateOperation::Fill => Rc::new(move |args| owner.parent.fill(args[0].clone())),
            PrivateOperation::ThrottleOrders => Rc::new(move |_| owner.throttled_orders()),
            PrivateOperation::MarkPosition => Rc::new(move |_| owner.parent.mark_position()),
            PrivateOperation::ThrottlePositions => Rc::new(move |_| {
                let method = owner.throttle_positions.borrow().clone();
                method()
            }),
            _ => return owner.parent.capture_store(op),
        })
    }
    fn now(&mut self) -> Result<f64, Failure> {
        Ok((self.0.virtual_now)())
    }
    fn orders(&mut self) -> Result<EventArray, Failure> {
        self.0.parent.orders()
    }
    fn positions(&mut self) -> Result<EventArray, Failure> {
        self.0.parent.positions()
    }
    fn balance(&mut self) -> Result<EventValue, Failure> {
        self.0.parent.balance()
    }
    fn reject_reason(&mut self, data: &EventValue) -> Result<EventValue, Failure> {
        crate::private_events::get_order_reject_reason(data)
    }

    fn asset_price(&mut self, asset: &EventValue) -> Result<f64, Failure> {
        self.0.parent.asset_price(asset)
    }
}
struct Auth(Rc<PrivateState>);
impl SpotAuthBindings for Auth {
    fn options(&mut self) -> Result<EventValue, Failure> {
        self.0.parent.options()
    }
    fn now(&mut self) -> Result<f64, Failure> {
        Ok((self.0.virtual_now)())
    }
    fn capture_remote_sign(&mut self, signer: EventValue) -> Result<RemoteSignCall, Failure> {
        self.0.parent.remote_sign(signer).map(|sign| {
            Rc::new(move |request| {
                sign(request).map(|future| {
                    Box::pin(class_jobs::reaction(future)) as crate::api_factory::ApiFuture
                })
            }) as RemoteSignCall
        })
    }
    fn hmac_sha256_hex(
        &mut self,
        secret: EventValue,
        payload: String,
    ) -> Result<EventValue, Failure> {
        crate::api_factory::utf8_hmac_sha256_hex(secret, payload)
    }
}
struct Message(Rc<PrivateState>);
impl SpotMessageBindings for Message {
    fn disposed(&self) -> bool {
        self.0.base().disposed()
    }
    fn call(&self, op: MessageOp, args: Vec<EventValue>) -> Result<(), Failure> {
        match op {
            MessageOp::Track => self.0.base().track(),
            MessageOp::Diagnostic => self.0.base().diagnostic(args[0].clone()),
            MessageOp::Stats => self.0.base().stats(&args[0].text()?),
            MessageOp::Account => self.0.handle_events(0, args[0].clone()),
            MessageOp::Order => self.0.handle_events(1, args[0].clone()),
            MessageOp::Ping => self.0.ping(),
            MessageOp::Pong => self.0.base().pong(),
            MessageOp::Emit => self.0.base().emit_error(args[0].text()?),
            MessageOp::ForceClose => self.0.base().force_close(),
            MessageOp::Reconnect => self.0.base().reconnect(),
        }
    }
}

// ts: BinancePrivateWebsocket (binance.ws-private.ts)
pub struct BinancePrivateWebsocket(pub Rc<PrivateSocket>);
impl BinancePrivateWebsocket {
    pub fn new(dependencies: PrivateDependencies) -> Result<Self, Failure> {
        PrivateSocket::new(Venue::Futures, dependencies).map(Self)
    }
}
impl std::ops::Deref for BinancePrivateWebsocket {
    type Target = Rc<PrivateSocket>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
// ts: BinancePMPrivateWebsocket
pub struct BinancePMPrivateWebsocket(pub Rc<PrivateSocket>);
impl BinancePMPrivateWebsocket {
    pub fn new(dependencies: PrivateDependencies) -> Result<Self, Failure> {
        PrivateSocket::new(Venue::PM, dependencies).map(Self)
    }
}
impl std::ops::Deref for BinancePMPrivateWebsocket {
    type Target = Rc<PrivateSocket>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
// ts: BinancePrivateWebsocket (binancespot.ws-private.ts)
pub struct BinanceSpotPrivateWebsocket(pub Rc<PrivateSocket>);
impl BinanceSpotPrivateWebsocket {
    pub fn new(dependencies: PrivateDependencies) -> Result<Self, Failure> {
        PrivateSocket::new(Venue::Spot, dependencies).map(Self)
    }
}
impl std::ops::Deref for BinanceSpotPrivateWebsocket {
    type Target = Rc<PrivateSocket>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
