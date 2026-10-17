use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    socket_lifecycle::SocketSlot,
    websocket::{
        BaseWebSocket, WebSocketConnectionOptions, WsEvent, WsParent, WsProtocol, WsRuntime,
        WsSocket,
    },
    ws_parent::WsExchangeParent,
};
use safe_cex_store::Store;
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::{ExchangeName, ExchangeOptions, Order};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
#[path = "support/store.rs"]
mod store;
type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);
#[derive(Default)]
struct Runtime {
    now: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Job>>,
    io: RefCell<Vec<Rc<Io>>>,
    fail_sleep: Rc<Cell<bool>>,
    inline_spawn: Cell<bool>,
    open_on_dial: Cell<bool>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    fn pump(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        let mut cx = Context::from_waker(Waker::noop());
        let mut pending = vec![];
        for (mut future, cancelled) in jobs {
            if !cancelled.get() && future.as_mut().poll(&mut cx).is_pending() {
                pending.push((future, cancelled));
            }
        }
        self.jobs.borrow_mut().extend(pending);
    }
    fn advance(&self, ms: f64) {
        self.now.set(self.now.get() + ms);
        self.pump();
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        let now = self.now.clone();
        let at = now.get() + ms as f64;
        let fail = self.fail_sleep.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if fail.get() {
                Poll::Ready(Err(ExchangeError::new("cancelled sleep")))
            } else if now.get() >= at {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, mut future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let flag = Rc::new(Cell::new(false));
        if self.inline_spawn.get()
            && future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        {
            return Box::new(Task(flag));
        }
        self.jobs.borrow_mut().push((future, flag.clone()));
        Box::new(Task(flag))
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.0
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}
#[derive(Default)]
struct Io {
    closed: Cell<usize>,
    events: Rc<RefCell<VecDeque<WsEvent>>>,
}
impl WsSocket for Io {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| {
            events
                .borrow_mut()
                .pop_front()
                .map_or(Poll::Pending, |event| Poll::Ready(Ok(event)))
        }))
    }
    fn send(&self, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn close(&self) {
        self.closed.set(self.closed.get() + 1);
    }
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("unused tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let socket = Rc::new(Io::default());
        if self.open_on_dial.get() {
            socket.events.borrow_mut().push_back(WsEvent::Open);
        }
        self.io.borrow_mut().push(socket.clone());
        Box::pin(async move { Ok(socket as Rc<dyn WsSocket>) })
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        3000.0
    }

    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    fetches: Rc<Cell<usize>>,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_orders(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.fetches.set(self.fetches.get() + 1);
        Box::pin(async { Ok(vec![]) })
    }
    fn place_orders<'a>(&'a self, _: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        Box::pin(async { Ok(vec![]) })
    }
}
struct Protocol;
impl WsProtocol for Protocol {
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn connect_and_subscribe(&self, _: Rc<BaseWebSocket>, _: bool) -> Result<(), ExchangeError> {
        Ok(())
    }
}
type Fixture = (
    Rc<dyn Exchange>,
    Rc<Runtime>,
    Rc<store::TestStore>,
    Rc<Cell<usize>>,
);
fn fixture() -> Fixture {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(1_000_000.0);
    let store = Rc::new(store::TestStore::new());
    let fetches = Rc::new(Cell::new(0));
    let options: ExchangeOptions = serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap();
    let venue: Rc<dyn Exchange> = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Binance,
            options,
            store.clone(),
            runtime.clone(),
            ExchangeSettings::default(),
        ),
        capabilities: ExchangeCapabilities::default(),
        fetches: fetches.clone(),
    });
    (venue, runtime, store, fetches)
}
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending future"),
    }
}
fn socket(venue: &Rc<dyn Exchange>, runtime: &Rc<Runtime>, id: u64) -> Rc<BaseWebSocket> {
    let parent = WsExchangeParent::new(venue);
    let socket = BaseWebSocket::new(id, parent, runtime.clone(), Rc::new(Protocol));
    run(socket.create_websocket("wss://venue".into(), vec![], None)).unwrap();
    socket
}

#[test]
fn bridge_is_weak_and_shared_resync_is_per_exchange() {
    let (venue, runtime, _, fetches) = fixture();
    let a = WsExchangeParent::new(&venue);
    let b = WsExchangeParent::new(&venue);
    assert!(Rc::ptr_eq(&a.resync_coordinator(), &b.resync_coordinator()));
    assert_eq!(Rc::strong_count(&venue), 1);
    let future = a.fetch_and_set_private_snapshot(10);
    drop(venue);
    assert!(a.is_disposed());
    run(future).unwrap();
    runtime.pump();
    assert_eq!(fetches.get(), 0);
}
#[test]
fn socket_groups_preserve_original_order_and_business_remains_public() {
    let (venue, runtime, _, _) = fixture();
    let private = socket(&venue, &runtime, 1);
    let public = socket(&venue, &runtime, 2);
    let business = socket(&venue, &runtime, 3);
    venue.base().set_socket(SocketSlot::Private, Some(private));
    venue.base().set_socket(SocketSlot::Public, Some(public));
    venue
        .base()
        .set_socket(SocketSlot::Business, Some(business));
    let ids = |sockets: Vec<Rc<BaseWebSocket>>| sockets.iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(ids(venue.base().get_all_sockets()), [1, 2, 3]);
    assert_eq!(ids(venue.base().get_public_sockets()), [2, 3]);
    assert_eq!(ids(venue.base().get_private_sockets()), [1]);
}
#[test]
fn any_healthy_public_socket_wins_and_durable_diagnostics_survive_recovery() {
    let (venue, runtime, store, _) = fixture();
    let a = socket(&venue, &runtime, 1);
    let b = socket(&venue, &runtime, 2);
    venue.base().set_socket(SocketSlot::Public, Some(a.clone()));
    venue
        .base()
        .set_socket(SocketSlot::PublicSecondary, Some(b.clone()));
    a.track_last_received().unwrap();
    a.record_close_diagnostics(Some(1006), false);
    runtime.now.set(runtime.now.get() + 21000.0);
    b.track_last_received().unwrap();
    b.record_close_diagnostics(Some(1013), false);
    venue.base().sync_socket_health_to_store().unwrap();
    let health = store.socket_health().unwrap();
    assert_eq!(health["publicHealthy"], true);
    assert_eq!(health["publicInitialized"], true);
    assert_eq!(health["publicCloseCode"].as_f64(), Some(1013.0));
    assert_eq!(health["publicCloseCountTotal"].as_f64(), Some(2.0));
    assert_eq!(health["publicCloseStreak"].as_f64(), Some(0.0));
    assert_eq!(health["publicLastMessage"], json!(runtime.now.get()));
}
#[test]
fn watchdog_uses_last_open_grace_after_reconnect() {
    let (venue, runtime, _, _) = fixture();
    let s = socket(&venue, &runtime, 1);
    venue.base().set_socket(SocketSlot::Public, Some(s.clone()));
    s.track_connected();
    s.track_last_received().unwrap();
    runtime.advance(100000.0);
    s.on_close(None, "").unwrap();
    run(s.create_websocket("wss://venue".into(), vec![], None)).unwrap();
    s.track_connected();
    let io = runtime.io.borrow().last().unwrap().clone();
    venue.base().sync_socket_health_to_store().unwrap();
    runtime.advance(10000.0);
    assert_eq!(io.closed.get(), 0);
    runtime.advance(50001.0);
    assert_eq!(io.closed.get(), 1);
}
#[test]
fn online_transition_recovers_once_and_emit_throttle_is_strict() {
    let (venue, runtime, _, _) = fixture();
    let s = socket(&venue, &runtime, 1);
    venue.base().set_socket(SocketSlot::Public, Some(s));
    let io = runtime.io.borrow().last().unwrap().clone();
    venue.base().set_online_status(false).unwrap();
    assert_eq!(io.closed.get(), 0);
    venue.base().set_online_status(true).unwrap();
    assert_eq!(io.closed.get(), 1);
    venue.base().set_online_status(true).unwrap();
    assert_eq!(io.closed.get(), 1);
    let events = Rc::new(Cell::new(0));
    let count = events.clone();
    venue.base().on(
        "ws-reconnected",
        Rc::new(move |_, _| {
            count.set(count.get() + 1);
            Ok(())
        }),
    );
    venue.base().emit_ws_reconnected().unwrap();
    assert_eq!(events.get(), 1);
    runtime.now.set(runtime.now.get() + 5000.0);
    venue.base().emit_ws_reconnected().unwrap();
    assert_eq!(events.get(), 1);
    runtime.now.set(runtime.now.get() + 1.0);
    venue.base().emit_ws_reconnected().unwrap();
    assert_eq!(events.get(), 2);
}
struct AckProtocol {
    ack: Rc<Cell<bool>>,
    completed: Rc<Cell<bool>>,
}
impl WsProtocol for AckProtocol {
    fn on_open(&self, socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let ack = self.ack.clone();
        let completed = self.completed.clone();
        Box::pin(async move {
            socket
                .authenticate_and_subscribe(
                    Box::pin(std::future::poll_fn(move |_| {
                        if ack.get() {
                            Poll::Ready(Ok(()))
                        } else {
                            Poll::Pending
                        }
                    })),
                    move |_| {
                        completed.set(true);
                        Ok(())
                    },
                )
                .await
        })
    }
    fn on_message(
        &self,
        _: &Rc<BaseWebSocket>,
        _: &safe_cex_transport::socket::SocketFrame,
    ) -> Result<(), ExchangeError> {
        self.ack.set(true);
        Ok(())
    }
}
#[test]
fn asynchronous_open_auth_does_not_block_receiving_its_ack() {
    let (venue, runtime, _, _) = fixture();
    let completed = Rc::new(Cell::new(false));
    let protocol = Rc::new(AckProtocol {
        ack: Rc::new(Cell::new(false)),
        completed: completed.clone(),
    });
    let s = BaseWebSocket::new(1, WsExchangeParent::new(&venue), runtime.clone(), protocol);
    run(s.create_websocket("wss://venue".into(), vec![], None)).unwrap();
    let io = runtime.io.borrow().last().unwrap().clone();
    io.events.borrow_mut().push_back(WsEvent::Open);
    runtime.pump();
    runtime.pump();
    assert!(!completed.get());
    io.events.borrow_mut().push_back(WsEvent::Message(
        safe_cex_transport::socket::SocketFrame::Text("auth ack".into()),
    ));
    runtime.pump();
    runtime.pump();
    assert!(completed.get());
}

#[test]
fn replacing_socket_clears_old_optional_close_and_message_evidence() {
    let (venue, runtime, store, _) = fixture();
    let old = socket(&venue, &runtime, 1);
    old.track_last_received().unwrap();
    old.record_close_diagnostics(Some(1006), false);
    venue.base().set_socket(SocketSlot::Public, Some(old));
    venue.base().sync_socket_health_to_store().unwrap();
    assert!(
        store
            .socket_health()
            .unwrap()
            .get("publicCloseCode")
            .is_some()
    );
    assert!(
        store
            .socket_health()
            .unwrap()
            .get("publicLastMessage")
            .is_some()
    );
    let fresh = socket(&venue, &runtime, 2);
    venue.base().set_socket(SocketSlot::Public, Some(fresh));
    venue.base().sync_socket_health_to_store().unwrap();
    let health = store.socket_health().unwrap();
    assert!(health.get("publicCloseCode").is_none());
    assert!(health.get("publicLastMessage").is_none());
    assert_eq!(health["publicCloseCountTotal"].as_f64(), Some(0.0));
}

#[test]
fn private_membership_uses_object_identity_even_when_host_labels_collide() {
    let (venue, runtime, _, _) = fixture();
    let private = socket(&venue, &runtime, 1);
    let public = socket(&venue, &runtime, 1);
    venue
        .base()
        .set_socket(SocketSlot::Private, Some(private.clone()));
    venue
        .base()
        .set_socket(SocketSlot::Public, Some(public.clone()));
    assert!(private.resyncs_private_state_on_reopen());
    assert!(!public.resyncs_private_state_on_reopen());
}

#[test]
fn watchdog_task_discard_and_sleep_error_allow_restart() {
    for mode in ["unpolled", "pending", "sleep-error", "inline-error"] {
        let (venue, runtime, _, _) = fixture();
        let _parent = WsExchangeParent::new(&venue);
        runtime.fail_sleep.set(mode.ends_with("error"));
        runtime.inline_spawn.set(mode == "inline-error");
        venue.base().sync_socket_health_to_store().unwrap();
        if mode == "pending" || mode == "sleep-error" {
            runtime.pump();
        }
        let tasks = std::mem::take(&mut *runtime.jobs.borrow_mut());
        drop(tasks);
        runtime.fail_sleep.set(false);
        runtime.inline_spawn.set(false);
        venue.base().sync_socket_health_to_store().unwrap();
        assert_eq!(
            runtime.jobs.borrow().len(),
            1,
            "watchdog must restart after {mode}"
        );
        runtime.advance(10_000.0);
        assert_eq!(
            runtime.jobs.borrow().len(),
            1,
            "normal completion must install exactly one successor"
        );
        venue.base().dispose().unwrap();
        runtime.pump();
        assert!(runtime.jobs.borrow().is_empty());
    }
}

#[test]
fn actual_native_reconnect_open_emits_for_public_and_private_sockets() {
    struct DialingProtocol(Rc<Cell<usize>>);
    impl WsProtocol for DialingProtocol {
        fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
            let started = self.0.clone();
            Box::pin(async move {
                started.set(started.get() + 1); // Venue subscription/reset prefix.
                std::future::pending::<()>().await; // Private auth may wait for an ACK.
                Ok(())
            })
        }
        fn connect_and_subscribe(
            &self,
            socket: Rc<BaseWebSocket>,
            _: bool,
        ) -> Result<(), ExchangeError> {
            socket.start_websocket("wss://venue".into(), vec![], None);
            Ok(())
        }
    }
    for (slot, inline) in [
        (SocketSlot::Public, false),
        (SocketSlot::Private, false),
        (SocketSlot::Public, true),
        (SocketSlot::Private, true),
    ] {
        let (venue, runtime, _, _) = fixture();
        runtime.inline_spawn.set(inline);
        runtime.open_on_dial.set(inline);
        let started = Rc::new(Cell::new(0));
        let socket = BaseWebSocket::new(
            1,
            WsExchangeParent::new(&venue),
            runtime.clone(),
            Rc::new(DialingProtocol(started.clone())),
        );
        venue.base().set_socket(slot, Some(socket.clone()));
        let events = Rc::new(Cell::new(0));
        let count = events.clone();
        venue.base().on(
            "ws-reconnected",
            Rc::new(move |_, _| {
                {
                    assert_eq!(
                        started.get(),
                        2,
                        "venue open prefix must precede account reconnect event"
                    );
                    count.set(count.get() + 1);
                };
                Ok(())
            }),
        );
        socket.connect_and_subscribe(false).unwrap();
        runtime.pump();
        let initial = runtime.io.borrow().last().unwrap().clone();
        if !inline {
            initial.events.borrow_mut().push_back(WsEvent::Open);
        }
        runtime.pump();
        runtime.pump();
        assert_eq!(events.get(), 0, "first open is not a reconnect");
        initial.events.borrow_mut().push_back(WsEvent::Close {
            code: Some(1000),
            reason: String::new(),
        });
        runtime.pump();
        runtime.advance(10_000.0);
        runtime.pump();
        let replacement = runtime.io.borrow().last().unwrap().clone();
        assert!(!Rc::ptr_eq(&initial, &replacement));
        if !inline {
            replacement.events.borrow_mut().push_back(WsEvent::Open);
        }
        runtime.pump();
        runtime.pump();
        assert_eq!(
            events.get(),
            1,
            "reopened socket must emit account resubscription event"
        );
    }
}

#[test]
fn cleared_socket_diagnostics_preserve_retained_key_order() {
    let (venue, _, store, _) = fixture();
    store
        .update(
            safe_cex_store::StoreUpdate {
                socket_health: Some(json!({
                    "first":1,"publicLastMessage":2,"second":3,"privateLastMessage":4,
                    "third":5,"publicCloseCode":6,"fourth":7,"privateCloseCode":8,"last":9
                })),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    venue.base().sync_socket_health_to_store().unwrap();
    let health = store.socket_health().unwrap();
    let retained: Vec<_> = health
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| ["first", "second", "third", "fourth", "last"].contains(&k.as_str()))
        .map(String::as_str)
        .collect();
    assert_eq!(
        retained,
        ["first", "second", "third", "fourth", "last"],
        "clearing optional diagnostics must preserve retained insertion order"
    );
}

#[test]
fn named_socket_lookup_preserves_empty_slots_and_replacement_identity() {
    fn slots() -> [SocketSlot; 8] {
        [
            SocketSlot::Private,
            SocketSlot::Public,
            SocketSlot::PublicSecondary,
            SocketSlot::PublicInverse,
            SocketSlot::Futures,
            SocketSlot::Business,
            SocketSlot::SecondsTickers,
            SocketSlot::ThirdTickers,
        ]
    }
    let (venue, runtime, _, _) = fixture();
    let sockets: Vec<_> = (0..8).map(|id| socket(&venue, &runtime, id)).collect();
    for (index, slot) in slots().into_iter().enumerate() {
        if index != 1 {
            venue.base().set_socket(slot, Some(sockets[index].clone()));
        }
    }
    for (index, slot) in slots().into_iter().enumerate() {
        let found = venue.base().get_socket(slot);
        if index == 1 {
            assert!(
                found.is_none(),
                "absent public slot must not resolve to secondary"
            );
        } else {
            assert!(
                Rc::ptr_eq(
                    &found.expect("named socket slot must exist"),
                    &sockets[index]
                ),
                "named socket lookup must preserve slot identity at {index}"
            );
        }
    }
    let previous = venue.base().get_socket(SocketSlot::PublicInverse).unwrap();
    venue
        .base()
        .set_socket(SocketSlot::PublicInverse, Some(sockets[1].clone()));
    assert!(Rc::ptr_eq(
        &venue.base().get_socket(SocketSlot::PublicInverse).unwrap(),
        &sockets[1]
    ));
    assert!(Rc::ptr_eq(&previous, &sockets[3]));
    venue.base().set_socket(SocketSlot::PublicInverse, None);
    assert!(venue.base().get_socket(SocketSlot::PublicInverse).is_none());
    assert!(Rc::ptr_eq(
        &venue
            .base()
            .get_socket(SocketSlot::PublicSecondary)
            .unwrap(),
        &sockets[2]
    ));
}

#[test]
fn worker_and_socket_snapshot_entrypoints_dispatch_venue_overrides() {
    struct SnapshotVenue {
        base: BaseExchange,
        caps: ExchangeCapabilities,
        snapshots: Cell<u32>,
        order_scopes: RefCell<Vec<Option<Vec<String>>>>,
    }
    impl Exchange for SnapshotVenue {
        type PlaceOpts = safe_cex_types::PlaceOrderOpts;
        fn name(&self) -> &ExchangeName {
            &ExchangeName::Binance
        }
        fn base(&self) -> &BaseExchange {
            &self.base
        }
        fn capabilities(&self) -> &ExchangeCapabilities {
            &self.caps
        }
        fn fetch_orders(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
            panic!("snapshot dispatch must not bypass venue orchestration")
        }
        fn fetch_and_set_orders(
            &self,
            symbols: Option<Vec<String>>,
        ) -> ExchangeFuture<'_, Vec<Order>> {
            self.order_scopes.borrow_mut().push(symbols);
            Box::pin(async { Ok(vec![]) })
        }
        fn fetch_and_set_private_snapshot(&self, tries: u32) -> ExchangeFuture<'_, ()> {
            self.snapshots.set(tries);
            Box::pin(async { Ok(()) })
        }
    }
    let runtime = Rc::new(Runtime::default());
    let concrete = Rc::new(SnapshotVenue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            runtime,
            ExchangeSettings::default(),
        ),
        caps: ExchangeCapabilities::default(),
        snapshots: Cell::new(0),
        order_scopes: RefCell::new(vec![]),
    });
    let venue: Rc<dyn Exchange> = concrete.clone();
    run(venue.fetch_and_set_orders(Some(vec!["BTC".into()]))).unwrap();
    assert_eq!(
        *concrete.order_scopes.borrow(),
        vec![Some(vec!["BTC".into()])]
    );
    run(venue.fetch_and_set_private_snapshot(3)).unwrap();
    assert_eq!(concrete.snapshots.get(), 3);
    let parent = WsExchangeParent::new(&venue);
    assert_eq!(
        parent.name(),
        "binance",
        "WS parent must dispatch venue name getter"
    );
    run(parent.fetch_and_set_private_snapshot(10)).unwrap();
    assert_eq!(
        concrete.snapshots.get(),
        10,
        "socket resync must call venue snapshot override"
    );
}

#[test]
fn root_base_offline_recovery_eager_dispatch_identity_and_throw_prefix() {
    let (venue, runtime, _, _) = fixture();
    let a = socket(&venue, &runtime, 71);
    let b = socket(&venue, &runtime, 72);
    let c = socket(&venue, &runtime, 73);
    venue
        .base()
        .set_socket(SocketSlot::Private, Some(a.clone()));
    venue.base().set_socket(SocketSlot::Public, Some(b.clone()));
    venue
        .base()
        .set_socket(SocketSlot::PublicSecondary, Some(c.clone()));
    let calls = RefCell::new(vec![]);
    assert!(
        !venue
            .base()
            .recover_after_offline(|s| {
                let index = if Rc::ptr_eq(&s, &a) {
                    0
                } else if Rc::ptr_eq(&s, &b) {
                    1
                } else {
                    assert!(Rc::ptr_eq(&s, &c));
                    2
                };
                calls.borrow_mut().push(index);
                Ok([Some(false), None, Some(true)][index])
            })
            .unwrap()
    );
    assert_eq!(
        *calls.borrow(),
        vec![0, 1, 2],
        "ROOT base native eager all before false"
    );
    let throw_calls = RefCell::new(Vec::<u64>::new());
    let error = venue
        .base()
        .recover_after_offline(|s| {
            throw_calls.borrow_mut().push(s.id);
            if Rc::ptr_eq(&s, &b) {
                Err(ExchangeError::new("recover failed"))
            } else {
                Ok(Some(true))
            }
        })
        .unwrap_err();
    assert_eq!(error.message, "recover failed");
    assert_eq!(
        *throw_calls.borrow(),
        vec![71, 72],
        "ROOT base native throw prefix"
    );
    assert!(
        venue.base().recover_after_offline(|_| Ok(None)).unwrap(),
        "ROOT base native nullish true"
    );
    for slot in [
        SocketSlot::Private,
        SocketSlot::Public,
        SocketSlot::PublicSecondary,
    ] {
        venue.base().set_socket(slot, None);
    }
    assert!(
        venue
            .base()
            .recover_after_offline(|_| panic!("empty snapshot callback"))
            .unwrap()
    );
    let output = std::process::Command::new("node")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/root_rows_oracle.cjs"))
        .arg("base")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["eager"], 3);
    assert_eq!(source["throwPrefix"], 2);
}

mod js_sockets {
    use super::*;
    use safe_cex_exchange::js_runtime::{
        sockets::{JsSocketContext, base_websocket, base_websocket_class, core_of},
        value::{Js, arg},
    };
    use safe_cex_transport::socket::SocketFrame;

    fn counter(count: &Rc<Cell<usize>>) -> Js {
        let count = count.clone();
        Js::function("listener", move |_, _| {
            count.set(count.get() + 1);
            Ok(Js::Undefined)
        })
    }

    #[test]
    fn native_recovery_reads_the_live_receiver_policy() {
        let (venue, runtime, _, _) = fixture();
        let context = JsSocketContext::new(runtime);
        *context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
        let socket = base_websocket(&context, Js::object(vec![]));
        let core = core_of(&context, &socket).unwrap();
        let base = core.base().unwrap();
        assert!(
            base.recover_after_offline(true).unwrap(),
            "the base policy defaults to true"
        );
        socket
            .define("recoversAfterOffline", Js::Bool(false))
            .unwrap();
        assert!(
            !base.recover_after_offline(true).unwrap(),
            "native recovery must read the source receiver false flag"
        );
        socket
            .define("recoversAfterOffline", Js::Bool(true))
            .unwrap();
        assert!(
            base.recover_after_offline(true).unwrap(),
            "policy reads remain live"
        );
        socket
            .define("recoversAfterOffline", Js::Bool(false))
            .unwrap();
        base.dispose();
        assert!(
            base.recover_after_offline(true).unwrap(),
            "disposal precedes the reload policy"
        );
    }

    struct UnusedHttp(Rc<Runtime>);
    impl safe_cex_exchange::js_runtime::client::JsHttpHost for UnusedHttp {
        fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
            self.0.clone()
        }
        fn send(&self, _: Js) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
            panic!("socket timeout witness does not issue HTTP")
        }
    }

    #[test]
    fn source_timeout_handles_cancel_owned_core_timers_without_stale_aliases() {
        use safe_cex_exchange::js_runtime::scope::{Scope, ScopeShared};
        let (venue, runtime, store, _) = fixture();
        let scope = Scope {
            shared: Rc::new(ScopeShared::new(
                runtime.clone(),
                Rc::new(UnusedHttp(runtime.clone())),
                store,
                Js::object(vec![]),
                (),
            )),
            this: Js::Undefined,
        };
        let context = scope.shared.sockets.clone();
        *context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
        let receiver = base_websocket(&context, Js::object(vec![]));
        let pings = Rc::new(Cell::new(0));
        receiver.define("ping", counter(&pings)).unwrap();
        let base = core_of(&context, &receiver).unwrap().base().unwrap();
        base.handle_pong_event(100).unwrap();
        let first = receiver.get("pingTimeoutId").unwrap();
        assert!(
            first.truthy(),
            "source fields must expose the actual native timeout"
        );
        assert!(
            first.same_reference(&receiver.get("pingTimeoutId").unwrap()),
            "reads preserve handle identity"
        );
        base.handle_pong_event(200).unwrap();
        let replacement = receiver.get("pingTimeoutId").unwrap();
        assert!(!first.same_reference(&replacement));
        scope.clear_timeout(&first);
        runtime.advance(200.0);
        assert_eq!(
            pings.get(),
            1,
            "a stale handle must not cancel the replacement"
        );
        base.handle_pong_event(300).unwrap();
        let ping = receiver.get("pingTimeoutId").unwrap();
        let pong = receiver.get("pongMissTimeoutId").unwrap();
        base.track_last_received().unwrap();
        let receive = receiver.get("lastReceivedTimer").unwrap();
        assert!(pong.truthy() && receive.truthy());
        for (field, handle) in [
            ("pingTimeoutId", ping),
            ("pongMissTimeoutId", pong),
            ("lastReceivedTimer", receive),
        ] {
            scope.clear_timeout(&handle);
            scope.clear_timeout(&handle);
            receiver.set(field, Js::Undefined).unwrap();
            assert!(receiver.get(field).unwrap().is_undefined());
        }
        runtime.advance(10_000.0);
        assert_eq!(
            pings.get(),
            1,
            "source cleanup cancels both heartbeat and receive work"
        );
        assert_eq!(
            scope.shared.timers.borrow().len(),
            0,
            "native aliases do not allocate duplicate runtime tasks"
        );
        base.dispose();
    }

    fn live_socket(runtime: &Rc<Runtime>) -> Rc<Io> {
        runtime.pump();
        runtime.io.borrow().last().expect("dialed").clone()
    }

    #[test]
    fn binary_delivery_preserves_bytes_until_explicit_text_conversion() {
        let (venue, runtime, _store, _) = fixture();
        let context = JsSocketContext::new(runtime.clone());
        *context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
        let socket = base_websocket(&context, Js::object(vec![]));
        let ws = socket
            .call_method("createWebsocket", vec![Js::str("wss://venue")])
            .unwrap();
        let received = Rc::new(RefCell::new(Vec::new()));
        let values = received.clone();
        ws.call_method(
            "addEventListener",
            vec![
                Js::str("message"),
                Js::function("capture", move |_, args| {
                    values.borrow_mut().push(arg(&args, 0).get("data")?);
                    Ok(Js::Undefined)
                }),
            ],
        )
        .unwrap();
        let io = live_socket(&runtime);
        io.events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Binary(vec![0, 128, 255, 65])));
        io.events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text("plain".into())));
        runtime.pump();
        let received = received.borrow();
        assert_eq!(received.len(), 2);
        assert!(
            matches!(&received[0], Js::Object(object) if object.class == "Buffer"),
            "binary frame remains a Buffer"
        );
        let bytes = received[0].get("bytes").unwrap().array_values().unwrap();
        assert_eq!(
            bytes
                .iter()
                .map(|v| v.as_f64().unwrap() as u8)
                .collect::<Vec<_>>(),
            vec![0, 128, 255, 65]
        );
        assert_eq!(received[0].get("byteLength").unwrap().as_f64(), Some(4.0));
        assert_eq!(
            received[0]
                .get("buffer")
                .unwrap()
                .get("bytes")
                .unwrap()
                .array_values()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(
            received[0].to_display(),
            "\0��A",
            "Buffer text conversion stays explicit"
        );
        assert!(matches!(&received[1], Js::String(_)));
        assert_eq!(received[1].to_display(), "plain");
    }

    #[test]
    fn redial_drops_the_replaced_handles_listeners_so_one_frame_dispatches_once() {
        let (venue, runtime, _store, _) = fixture();
        let context = JsSocketContext::new(runtime.clone());
        *context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
        let socket = base_websocket(&context, Js::object(vec![]));
        let (messages, closures) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let on_message = counter(&messages);
        // A venue re-registering the same member, plus a fresh closure, per (re)connect.
        for _ in 0..3 {
            let ws = socket
                .call_method("createWebsocket", vec![Js::str("wss://venue")])
                .unwrap();
            for _ in 0..2 {
                ws.call_method(
                    "addEventListener",
                    vec![Js::str("message"), on_message.clone()],
                )
                .unwrap();
            }
            ws.call_method(
                "addEventListener",
                vec![Js::str("message"), counter(&closures)],
            )
            .unwrap();
        }
        let io = live_socket(&runtime);
        io.events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text("frame".into())));
        runtime.pump();
        assert_eq!(
            messages.get(),
            1,
            "same listener registered twice dispatches once"
        );
        assert_eq!(
            closures.get(),
            1,
            "replaced handles' listeners do not dispatch"
        );
    }

    #[test]
    fn listeners_are_removed_by_identity_and_errors_reach_them() {
        let (venue, runtime, _store, _) = fixture();
        let context = JsSocketContext::new(runtime.clone());
        *context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
        let socket = base_websocket(&context, Js::object(vec![]));
        let ws = socket
            .call_method("createWebsocket", vec![Js::str("wss://venue")])
            .unwrap();
        let (kept, removed, errors) = (
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
        );
        let (kept_fn, removed_fn) = (counter(&kept), counter(&removed));
        for f in [&kept_fn, &removed_fn] {
            ws.call_method("addEventListener", vec![Js::str("message"), f.clone()])
                .unwrap();
        }
        ws.call_method("addEventListener", vec![Js::str("error"), counter(&errors)])
            .unwrap();
        ws.call_method("removeEventListener", vec![Js::str("message"), removed_fn])
            .unwrap();
        let io = live_socket(&runtime);
        io.events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text("frame".into())));
        io.events
            .borrow_mut()
            .push_back(WsEvent::Error("boom".into()));
        runtime.pump();
        assert_eq!((kept.get(), removed.get(), errors.get()), (1, 0, 1));
    }

    #[test]
    fn base_surface_has_field_initializers_close_diagnostics_and_prototype_dispose() {
        let (venue, runtime, _store, _) = fixture();
        let context = JsSocketContext::new(runtime.clone());
        *context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
        let socket = base_websocket(&context, Js::object(vec![]));
        for (field, expected) in [
            ("lastCloseTime", "0"),
            ("onCloseCount", "0"),
            ("reconnecting", "false"),
            ("pingSent", "false"),
        ] {
            assert_eq!(socket.get(field).unwrap().to_display(), expected, "{field}");
        }
        assert!(socket.get("pingTimeoutId").unwrap().is_undefined());
        socket
            .call_method("createWebsocket", vec![Js::str("wss://venue")])
            .unwrap();
        socket
            .call_method(
                "recordCloseDiagnostics",
                vec![
                    Js::Number(1006.0),
                    Js::object(vec![("syntheticIfNoSocket", Js::Bool(true))]),
                ],
            )
            .unwrap();
        // A derived arrow field shadows dispose; the class prototype still reaches it.
        assert!(!socket.has_own("dispose"));
        let class = base_websocket_class(&context);
        let dispose = class.get("prototype").unwrap().get("dispose").unwrap();
        dispose.call(socket.clone(), vec![]).unwrap();
        assert!(!socket.get("isConnected").unwrap().truthy());
    }

    #[test]
    fn retry_failed_connect_drives_the_close_path_backoff_into_a_fresh_dial() {
        let (venue, runtime, _store, _) = fixture();
        let context = JsSocketContext::new(runtime.clone());
        *context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
        let socket = base_websocket(&context, Js::object(vec![]));
        let connects = Rc::new(Cell::new(0));
        {
            // The venue's own connect, which the core's reconnect path calls back into.
            let (connects, this) = (connects.clone(), socket.clone());
            socket
                .define(
                    "connectAndSubscribe",
                    Js::function("connectAndSubscribe", move |_, _| {
                        connects.set(connects.get() + 1);
                        this.call_method("createWebsocket", vec![Js::str("wss://venue")])
                    }),
                )
                .unwrap();
        }
        socket
            .call_method(
                "retryFailedConnect",
                vec![Js::str("listen key fetch failed")],
            )
            .unwrap();
        for _ in 0..10 {
            runtime.advance(10_000.0);
        }
        assert!(
            connects.get() >= 1,
            "a failed connect is retried through connectAndSubscribe"
        );
    }

    fn assembled(runtime: &Rc<Runtime>, venue: &Rc<dyn Exchange>) -> Rc<JsSocketContext> {
        let context = JsSocketContext::new(runtime.clone());
        *context.venue.borrow_mut() = Some(Rc::downgrade(venue));
        context
    }

    #[test]
    fn transport_close_reaches_close_listeners_but_not_the_base_on_close_member() {
        let (venue, runtime, _store, _) = fixture();
        let context = assembled(&runtime, &venue);
        let socket = base_websocket(&context, Js::object(vec![]));
        let ws = socket
            .call_method("createWebsocket", vec![Js::str("wss://venue")])
            .unwrap();
        let events = Rc::new(RefCell::new(vec![]));
        {
            let events = events.clone();
            let on_close = Js::function("onClose", move |_, a| {
                let event = arg(&a, 0);
                events.borrow_mut().push((
                    event.get("code")?.to_display(),
                    event.get("reason")?.to_display(),
                    event.get("wasClean")?.to_display(),
                ));
                Ok(Js::Undefined)
            });
            ws.call_method("addEventListener", vec![Js::str("close"), on_close])
                .unwrap();
        }
        let base_member = socket.get("onClose").unwrap();
        ws.call_method("addEventListener", vec![Js::str("close"), base_member])
            .unwrap();
        let io = live_socket(&runtime);
        io.events.borrow_mut().push_back(WsEvent::Close {
            code: Some(4001),
            reason: "bye".into(),
        });
        runtime.pump();
        assert_eq!(
            *events.borrow(),
            [("4001".to_owned(), "bye".to_owned(), "true".to_owned())],
            "the transport close is delivered once, to the derived listener"
        );
        let base = core_of(&context, &socket).unwrap().base().unwrap();
        assert_eq!(
            base.diagnostics().lifetime_close_count,
            1,
            "the core handled the close itself, and the skipped base member did not repeat it"
        );
    }

    #[test]
    fn watchdog_thresholds_come_from_the_receiver_only_when_it_defines_them() {
        let (venue, runtime, _store, _) = fixture();
        let context = assembled(&runtime, &venue);
        let socket = base_websocket(&context, Js::object(vec![]));
        let base = core_of(&context, &socket).unwrap().base().unwrap();
        assert_eq!(base.get_watchdog_stale_threshold_ms(), None);
        assert_eq!(base.get_watchdog_ping_threshold_ms(), None);
        for (member, value) in [
            ("getWatchdogStaleThresholdMs", Js::Number(180_000.0)),
            ("getWatchdogPingThresholdMs", Js::Number(45_000.0)),
        ] {
            socket
                .define(member, Js::function(member, move |_, _| Ok(value.clone())))
                .unwrap();
        }
        assert_eq!(base.get_watchdog_stale_threshold_ms(), Some(180_000.0));
        assert_eq!(base.get_watchdog_ping_threshold_ms(), Some(45_000.0));
        socket
            .define(
                "getWatchdogStaleThresholdMs",
                Js::function("getWatchdogStaleThresholdMs", |_, _| Ok(Js::Undefined)),
            )
            .unwrap();
        assert_eq!(
            base.get_watchdog_stale_threshold_ms(),
            None,
            "undefined ?? default"
        );
    }

    #[test]
    fn treat_any_message_as_pong_and_ping_at_reach_the_core() {
        let (venue, runtime, store, _) = fixture();
        let context = assembled(&runtime, &venue);
        let socket = base_websocket(&context, Js::object(vec![]));
        socket
            .define("treatAnyMessageAsPong", Js::Bool(true))
            .unwrap();
        socket
            .call_method("createWebsocket", vec![Js::str("wss://venue")])
            .unwrap();
        let base = core_of(&context, &socket).unwrap().base().unwrap();
        assert!(base.treat_any_message_as_pong.get());
        // ts: ping() { this.pingAt = performance.now(); ... } then the pong arrives.
        let sent_at = runtime.now.get();
        socket.set("pingAt", Js::Number(sent_at)).unwrap();
        assert_eq!(socket.get("pingAt").unwrap().as_f64(), Some(sent_at));
        runtime.now.set(sent_at + 100.0);
        socket.call_method("handlePongEvent", vec![]).unwrap();
        assert_eq!(store.latency(), 50.0, "latency is half the ping round trip");
    }

    /// A socket whose connectAndSubscribe counts redials and registers `this.onClose`.
    fn closing_socket(
        context: &Rc<JsSocketContext>,
        derived_closes: Option<Rc<Cell<usize>>>,
    ) -> (Js, Rc<Cell<usize>>) {
        let socket = base_websocket(context, Js::object(vec![]));
        if let Some(closes) = derived_closes {
            // ts: a subclass `onClose` arrow field replaces the base member.
            socket
                .define(
                    "onClose",
                    Js::function("onClose", move |_, _| {
                        closes.set(closes.get() + 1);
                        Ok(Js::Undefined)
                    }),
                )
                .unwrap();
        }
        let connects = Rc::new(Cell::new(0));
        let (counter, this) = (connects.clone(), socket.clone());
        socket
            .define(
                "connectAndSubscribe",
                Js::function("connectAndSubscribe", move |_, _| {
                    counter.set(counter.get() + 1);
                    let ws = this.call_method("createWebsocket", vec![Js::str("wss://venue")])?;
                    ws.call_method(
                        "addEventListener",
                        vec![Js::str("close"), this.get("onClose")?],
                    )
                }),
            )
            .unwrap();
        socket.call_method("connectAndSubscribe", vec![]).unwrap();
        (socket, connects)
    }

    fn close_transport_and_wait(runtime: &Rc<Runtime>) {
        let io = live_socket(runtime);
        io.events.borrow_mut().push_back(WsEvent::Close {
            code: Some(1006),
            reason: "gone".into(),
        });
        runtime.pump();
        for _ in 0..10 {
            runtime.advance(10_000.0);
        }
    }

    #[test]
    fn a_derived_on_close_owns_the_transport_close_and_the_core_does_not_reconnect() {
        let (venue, runtime, _store, _) = fixture();
        let context = assembled(&runtime, &venue);
        let derived_closes = Rc::new(Cell::new(0));
        let (socket, connects) = closing_socket(&context, Some(derived_closes.clone()));
        close_transport_and_wait(&runtime);
        assert_eq!(derived_closes.get(), 1, "the derived onClose ran once");
        assert_eq!(
            connects.get(),
            1,
            "the core scheduled no reconnect of its own"
        );
        let base = core_of(&context, &socket).unwrap().base().unwrap();
        assert_eq!(
            base.diagnostics().lifetime_close_count,
            0,
            "no core close handling"
        );
    }

    #[test]
    fn without_a_derived_on_close_the_core_keeps_its_reconnect() {
        let (venue, runtime, _store, _) = fixture();
        let context = assembled(&runtime, &venue);
        let (socket, connects) = closing_socket(&context, None);
        close_transport_and_wait(&runtime);
        assert!(
            connects.get() >= 2,
            "the core reconnected through connectAndSubscribe"
        );
        let base = core_of(&context, &socket).unwrap().base().unwrap();
        assert_eq!(base.diagnostics().lifetime_close_count, 1);
    }
}

struct OnlineRecoveryProtocol;
impl WsProtocol for OnlineRecoveryProtocol {
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _: bool,
    ) -> Result<(), ExchangeError> {
        socket.start_websocket("wss://venue".into(), vec![], None);
        Ok(())
    }
}

fn assert_online_recovery_after_failures(failures: usize) {
    let (venue, runtime, _, _) = fixture();
    let socket = BaseWebSocket::new(
        91,
        WsExchangeParent::new(&venue),
        runtime.clone(),
        Rc::new(OnlineRecoveryProtocol),
    );
    venue
        .base()
        .set_socket(SocketSlot::Public, Some(socket.clone()));
    run(socket.create_websocket("wss://venue".into(), vec![], None)).unwrap();
    for _ in 0..failures {
        socket.on_close(Some(1006), "offline").unwrap();
    }
    assert_eq!(socket.is_reconnect_pause_active(), failures == 8);
    assert_eq!(runtime.io.borrow().len(), 1);
    venue.base().set_online_status(false).unwrap();
    venue.base().set_online_status(true).unwrap();
    runtime.pump();
    assert!(
        !socket.is_reconnect_pause_active(),
        "online recovery clears the two-minute failure pause"
    );
    assert_eq!(
        runtime.io.borrow().len(),
        2,
        "online recovery must bypass the pending reconnect delay after {failures} failures"
    );
    socket.on_close(Some(1006), "new failure").unwrap();
    assert!(
        !socket.is_reconnect_pause_active(),
        "the failure streak restarts after recovery"
    );
    runtime.advance(1999.0);
    runtime.pump();
    assert_eq!(runtime.io.borrow().len(), 2);
    runtime.advance(1.0);
    runtime.pump();
    assert_eq!(
        runtime.io.borrow().len(),
        3,
        "the first retry uses the reset two-second backoff"
    );
}

#[test]
fn online_transition_runs_pending_reconnect_now() {
    assert_online_recovery_after_failures(1);
}

#[test]
fn online_transition_clears_the_two_minute_failure_pause() {
    assert_online_recovery_after_failures(8);
}

#[test]
fn online_transition_clears_the_explicit_reconnect_cap() {
    let (venue, runtime, _, _) = fixture();
    let socket = socket(&venue, &runtime, 92);
    venue
        .base()
        .set_socket(SocketSlot::Public, Some(socket.clone()));
    let io = runtime.io.borrow().last().unwrap().clone();
    for _ in 0..101 {
        socket.reconnect().unwrap();
    }
    assert_eq!(
        io.closed.get(),
        100,
        "the source explicit-reconnect cap is reached"
    );
    venue.base().set_online_status(false).unwrap();
    venue.base().set_online_status(true).unwrap();
    assert_eq!(
        io.closed.get(),
        101,
        "online recovery must bypass and clear the reconnect cap"
    );
    socket.reconnect().unwrap();
    assert_eq!(
        io.closed.get(),
        102,
        "explicit reconnect works again after online recovery"
    );
}
