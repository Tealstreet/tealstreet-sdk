//! LighterPrivateWebsocket resolved from the real globals of a realm built the
//! way LighterNativeExchange::new builds it, driven over a scripted WsRuntime.
use super::super::{
    realm::{LighterExtra, Realm},
    value::Microtasks,
};
use crate::ws_policy::{calculate_lighter_private_reconnect_delay, classify_lighter_private_close};
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch},
    js_runtime::{
        client::JsHttpHost,
        globals::{install_builtins, install_enums},
        library::install_library,
        promise::{JsFuture, Promise},
        scope::ScopeShared,
        store,
        value::{Js, Thrown, from_json, from_typed, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_store::{store::DefaultStore, virtual_clock::VIRTUAL_CLOCK};
use safe_cex_transport::{TransportProvider, socket::SocketFrame, socket::TunnelHost};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    sync::Arc,
    task::{Context, Poll, Waker},
};

const START: f64 = 1_700_000_000_000.0;
const URL: &str = "wss://lighter.test/stream";

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

#[derive(Default)]
struct Runtime {
    now: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Job>>,
    sockets: RefCell<Vec<(String, Rc<Io>)>>,
    reports: RefCell<Vec<String>>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
impl Runtime {
    fn pump(&self) {
        for _ in 0..256 {
            let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
            if jobs.is_empty() {
                return;
            }
            let mut cx = Context::from_waker(Waker::noop());
            let mut pending = vec![];
            let mut progressed = false;
            for (mut future, cancelled) in jobs {
                if cancelled.get() || future.as_mut().poll(&mut cx).is_ready() {
                    progressed = true;
                } else {
                    pending.push((future, cancelled));
                }
            }
            let spawned = !self.jobs.borrow().is_empty();
            self.jobs.borrow_mut().extend(pending);
            if !progressed && !spawned {
                return;
            }
        }
    }
    fn advance(&self, ms: f64) {
        self.now.set(self.now.get() + ms);
        self.pump();
    }
    fn socket(&self, index: usize) -> Rc<Io> {
        self.sockets.borrow()[index].1.clone()
    }
    fn deliver(&self, index: usize, event: WsEvent) {
        self.socket(index).events.borrow_mut().push_back(event);
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
        let mut turned = false;
        Box::pin(std::future::poll_fn(move |_| {
            let ready = turned && now.get() >= at;
            turned = true;
            if ready {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let flag = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((future, flag.clone()));
        Box::new(Task(flag))
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.0
    }
    fn report_raw_exception(&self, error: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {
        self.reports.borrow_mut().push(safe_cex_exchange::runtime::exception_text(error));
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.reports.borrow_mut().push(error.to_string());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        unreachable!("no tunnelled route is configured in these tests")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let socket = Rc::new(Io::default());
        self.sockets.borrow_mut().push((url, socket.clone()));
        Box::pin(async move { Ok(socket as Rc<dyn WsSocket>) })
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1500.0
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

#[derive(Default)]
struct Io {
    events: Rc<RefCell<VecDeque<WsEvent>>>,
    sent: RefCell<Vec<Value>>,
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
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        let frame = serde_json::from_str(message).unwrap_or(Value::String(message.into()));
        self.sent.borrow_mut().push(frame);
        Ok(())
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}

struct Http(Rc<Runtime>);
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        Box::pin(async { Err(Thrown::error("Error", "no HTTP in this test")) })
    }
}

/// The `dyn Exchange` the socket core reports to (WsExchangeParent).
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn place_orders<'a>(&'a self, _: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        Box::pin(async { Ok(vec![]) })
    }
}

type Calls = Rc<RefCell<Vec<(String, Value)>>>;

struct World {
    runtime: Rc<Runtime>,
    store: Rc<DefaultStore>,
    calls: Calls,
    socket: Js,
    _venue: Rc<dyn Exchange>,
    _realm: Realm,
}

fn render(value: &Js) -> Value {
    to_json(value).unwrap_or_else(|_| Value::String(value.to_display()))
}

fn recorder(calls: &Calls, name: &'static str, reply: impl Fn(&[Js]) -> Js + 'static) -> Js {
    let calls = calls.clone();
    Js::function(name, move |_, args| {
        let rendered = Value::Array(args.iter().map(render).collect());
        calls.borrow_mut().push((name.to_owned(), rendered));
        Ok(reply(&args))
    })
}

fn options() -> ExchangeOptions {
    serde_json::from_value(json!({"key": "k", "secret": "s"})).unwrap()
}

fn mapped_order(raw: &Js) -> Js {
    let id = raw.get("order_index").unwrap().to_display();
    from_json(&json!([{
        "id": id, "status": "open", "symbol": "ETH/USDC:USDC", "type": "limit",
        "side": "buy", "price": 2000.5, "amount": 1.5, "filled": 0, "remaining": 1.5,
        "reduceOnly": false, "lastUpdate": 0
    }]))
}

fn parent(world: &Rc<Runtime>, store_impl: &Rc<DefaultStore>, calls: &Calls, token: Js) -> Js {
    let runtime: Rc<dyn ExchangeRuntime> = world.clone();
    let settled = move |value: Js| Js::Promise(Promise::settled(&runtime, Ok(value)));
    let store = store::facade(store_impl.clone());
    store
        .define(
            "updatePositions",
            recorder(calls, "store.updatePositions", |_| Js::Undefined),
        )
        .unwrap();
    let (resolve_orders, resolve_balance) = (settled.clone(), settled.clone());
    Js::object(vec![
        ("name", Js::str("lighter")),
        ("options", from_typed(&options()).unwrap()),
        ("isDisposed", Js::Bool(false)),
        (
            "lighterDeploymentProfile",
            from_json(&json!({"wsUrl": URL, "network": "mainnet", "supportsWsProxy": false})),
        ),
        ("log", recorder(calls, "log", |_| Js::Undefined)),
        (
            "emitter",
            Js::object(vec![("emit", recorder(calls, "emit", |_| Js::Undefined))]),
        ),
        ("store", store),
        ("recentlyCancelledOrders", Js::new_set()),
        (
            "getPrivateWsAccountIndex",
            recorder(calls, "getPrivateWsAccountIndex", |_| Js::Number(7.0)),
        ),
        (
            "getPrivateWsAuthToken",
            recorder(calls, "getPrivateWsAuthToken", move |_| token.clone()),
        ),
        (
            "invalidateAccountSnapshotCache",
            recorder(calls, "invalidateAccountSnapshotCache", |_| Js::Undefined),
        ),
        (
            "shouldSkipOrderReconciliationFetch",
            recorder(calls, "shouldSkipOrderReconciliationFetch", |_| {
                Js::Bool(false)
            }),
        ),
        (
            "fetchOrders",
            recorder(calls, "fetchOrders", move |_| {
                resolve_orders(Js::array(vec![]))
            }),
        ),
        (
            "fetchAndSetBalance",
            recorder(calls, "fetchAndSetBalance", move |_| {
                resolve_balance(Js::Undefined)
            }),
        ),
        (
            "mapOrder",
            recorder(calls, "mapOrder", |a| mapped_order(&a[0])),
        ),
        (
            "mapPosition",
            recorder(calls, "mapPosition", |a| {
                let symbol = a[0].get("symbol").unwrap();
                Js::object(vec![("id", Js::str("ETH-long")), ("symbol", symbol)])
            }),
        ),
        (
            "syncSocketHealthToStore",
            recorder(calls, "syncSocketHealthToStore", |_| Js::Undefined),
        ),
    ])
}

fn world_with_token(token: impl FnOnce(&Rc<dyn ExchangeRuntime>) -> Js) -> World {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(START);
    let store = Rc::new(DefaultStore::new());
    let exchange_runtime: Rc<dyn ExchangeRuntime> = runtime.clone();
    let ws: Rc<dyn WsRuntime> = runtime.clone();
    let shared = Rc::new_cyclic(|weak: &Weak<_>| {
        let globals = Js::object(vec![]);
        install_builtins(&globals, weak.clone());
        install_enums(&globals);
        install_library(&globals, weak.clone());
        ScopeShared::new(
            ws.clone(),
            Rc::new(Http(runtime.clone())),
            store.clone(),
            globals,
            LighterExtra {
                microtasks: Microtasks::new(exchange_runtime.clone()),
                supers: Default::default(),
                registry: Default::default(),
            },
        )
    });
    let realm = Realm(shared.clone());
    super::super::socket_globals::install(&realm).unwrap();
    super::install(&realm).unwrap();
    let venue: Rc<dyn Exchange> = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Lighter,
            options(),
            store.clone(),
            exchange_runtime.clone(),
            ExchangeSettings::default(),
        ),
        capabilities: ExchangeCapabilities::default(),
    });
    *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&venue));
    let calls: Calls = Rc::default();
    let parent = parent(&runtime, &store, &calls, token(&exchange_runtime));
    let class = realm.global("LighterPrivateWebsocket").unwrap();
    let socket = class.call(Js::Undefined, vec![parent]).unwrap();
    World {
        runtime,
        store,
        calls,
        socket,
        _venue: venue,
        _realm: realm,
    }
}

fn world() -> World {
    world_with_token(|runtime| Js::Promise(Promise::settled(runtime, Ok(Js::str("tok-1")))))
}

impl World {
    fn connect(&self) {
        self.socket
            .call_method("connectAndSubscribe", vec![])
            .unwrap();
        self.runtime.pump();
    }
    fn open(&self) {
        self.connect();
        self.runtime.deliver(0, WsEvent::Open);
    }
    fn message(&self, index: usize, payload: Value) {
        let frame = SocketFrame::Text(payload.to_string());
        self.runtime.deliver(index, WsEvent::Message(frame));
    }
    fn sent(&self, index: usize) -> Vec<Value> {
        self.runtime.socket(index).sent.borrow().clone()
    }
    fn called(&self, name: &str) -> Vec<Value> {
        self.calls
            .borrow()
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, a)| a.clone())
            .collect()
    }
    fn logs(&self) -> Vec<String> {
        self.called("log")
            .iter()
            .map(|a| a[0].as_str().unwrap_or_default().to_owned())
            .collect()
    }
}

fn ping_frame(at: f64) -> Value {
    json!({"type": "ping", "timestamp": at as u64})
}

#[test]
fn class_global_is_derived_from_base_websocket() {
    let world = world();
    let class = world._realm.global("LighterPrivateWebsocket").unwrap();
    assert_eq!(
        class
            .get("RATE_LIMIT_FORCE_TOAST_AFTER_MS")
            .unwrap()
            .as_f64(),
        Some(60_000.0)
    );
    let base_dispose = world
        ._realm
        .global("BaseWebSocket")
        .unwrap()
        .get("prototype")
        .unwrap()
        .get("dispose")
        .unwrap();
    assert!(base_dispose.is_callable());
    let Js::Object(socket) = &world.socket else {
        panic!("socket is an object")
    };
    let derived = socket.proto.borrow().clone().unwrap();
    let Js::Object(derived) = derived else {
        panic!("derived prototype is an object")
    };
    let base = derived.proto.borrow().clone().unwrap();
    assert!(
        base.get("dispose").unwrap().same_reference(&base_dispose),
        "instance -> LighterPrivateWebsocket.prototype -> BaseWebSocket.prototype"
    );
    assert!(
        world
            .socket
            .get("useReadonlyFallback")
            .unwrap()
            .strict_equals(&Js::Bool(false))
    );
    assert!(world.socket.get("onMessage").unwrap().is_callable());
}

#[test]
fn connect_dials_the_profile_url_and_open_subscribes_account_all_then_orders_with_token() {
    let world = world();
    world.connect();
    let urls: Vec<String> = world
        .runtime
        .sockets
        .borrow()
        .iter()
        .map(|(u, _)| u.clone())
        .collect();
    assert_eq!(
        urls,
        vec![URL.to_owned()],
        "connectAndSubscribe dials wsUrl"
    );
    assert_eq!(
        world.logs(),
        vec![format!(
            "[Lighter][PWS] Connecting (direct, attempt #1): {URL}"
        )]
    );
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    world.runtime.deliver(0, WsEvent::Open);
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let frames = world.sent(0);
    let timestamp = frames[0]["timestamp"]
        .as_f64()
        .expect("native ping timestamp");
    assert!(
        timestamp >= before && timestamp <= after,
        "ping uses the synced virtual clock"
    );
    assert_eq!(
        frames,
        vec![
            ping_frame(timestamp),
            json!({"type": "subscribe", "channel": "account_all/7"}),
            json!({"type": "subscribe", "channel": "account_all_orders/7", "auth": "tok-1"}),
        ],
        "onOpen pings, subscribes account_all, then account_all_orders with the token"
    );
    assert_eq!(world.called("getPrivateWsAuthToken"), vec![json!([60.0])]);
    assert!(
        world
            .socket
            .get("hasPendingAccountAllOrdersSubscribe")
            .unwrap()
            .truthy()
    );
    assert!(world.socket.get("diagnosticsInterval").unwrap().is_object());
    assert!(
        world.runtime.reports.borrow().is_empty(),
        "{:?}",
        world.runtime.reports
    );
}

#[test]
fn account_all_payload_updates_orders_positions_and_throttled_balance() {
    let world = world();
    world.open();
    world.message(
        0,
        json!({
            "channel": "account_all:7",
            "type": "update/account_all",
            "orders": {"0": [{"order_index": 41, "status": "open", "price": "2000.5"}]},
            "positions": {"0": {"symbol": "ETH", "size": "1.5", "sign": 1}},
            "account": {"collateral": "100"}
        }),
    );
    let orders: Vec<String> = world.store.orders().iter().map(|o| o.id.clone()).collect();
    assert_eq!(
        orders,
        vec!["41"],
        "handleOrderTopic adds the mapped open order"
    );
    assert_eq!(world.called("mapOrder")[0][0]["order_index"], json!(41.0));
    let pair = json!({"id": "ETH-long", "symbol": "ETH"});
    assert_eq!(
        world.called("store.updatePositions"),
        vec![json!([[[pair.clone(), pair]]])],
        "handlePositionTopic pushes [mapped, mapped] pairs"
    );
    assert_eq!(world.called("invalidateAccountSnapshotCache").len(), 1);
    assert!(
        world.called("fetchAndSetBalance").is_empty(),
        "balance is throttled"
    );
    world.runtime.advance(500.0);
    assert_eq!(
        world.called("fetchAndSetBalance"),
        vec![json!([{"allowRecentAccountSnapshot": false, "includeAccountLimits": false}])],
        "throttleFetchBalance(500) reloads the balance"
    );
    assert_eq!(
        world.called("fetchOrders").len(),
        1,
        "REST order reconciliation before ack"
    );
    assert_eq!(
        world
            .socket
            .get("accountStreamMessageCount")
            .unwrap()
            .as_f64(),
        Some(1.0)
    );
    assert!(
        world.runtime.reports.borrow().is_empty(),
        "{:?}",
        world.runtime.reports
    );
}

/// Observe dispatch timing on the host scheduler, independently of the synced payload clock.
fn ping_count_until(world: &World, until: f64) -> usize {
    while world.runtime.now.get() < START + until {
        world
            .runtime
            .advance((START + until - world.runtime.now.get()).min(1_000.0));
    }
    world.sent(0).iter().filter(|f| f["type"] == "ping").count()
}

#[test]
fn pong_schedules_the_next_ping_at_the_ping_interval() {
    let world = world();
    world.open();
    world.message(0, json!({"type": "pong"}));
    assert_eq!(
        ping_count_until(&world, 5_000.0),
        1,
        "only onOpen's ping so far"
    );
    let before_deadline = ping_count_until(&world, 9_999.0);
    assert!(
        ping_count_until(&world, 10_000.0) > before_deadline,
        "pong arms the next ping at getPingIntervalMs()"
    );
}

#[test]
fn close_frame_reconnects_after_the_classified_backoff_delay() {
    let world = world();
    world.open();
    let close = SocketFrame::Close {
        code: 1006,
        reason: String::new(),
        was_clean: false,
    };
    world.runtime.deliver(0, WsEvent::Message(close));
    let classification = classify_lighter_private_close(Some(1006.0), "", false, true);
    let delay = calculate_lighter_private_reconnect_delay(
        1500.0,
        1.0,
        classification.should_backoff,
        classification.is_rate_limited,
        false,
        30_000.0,
    );
    assert_eq!(
        world.socket.get("failedHandshakes").unwrap().as_f64(),
        Some(1.0)
    );
    let expected = format!("[Lighter][PWS] Reconnecting in {}ms...", delay.round());
    assert!(world.logs().contains(&expected), "{:?}", world.logs());
    world.runtime.advance(delay - 1.0);
    assert_eq!(
        world.runtime.sockets.borrow().len(),
        1,
        "no redial before the delay"
    );
    world.runtime.advance(1.0);
    assert_eq!(
        world.runtime.sockets.borrow().len(),
        2,
        "onClose redials after the delay"
    );
    assert_eq!(world.called("syncSocketHealthToStore").len(), 1);
}

// ts-bug: a rejected auth token only logs and skips account_all_orders.
#[test]
fn token_failure_logs_and_skips_the_orders_subscription() {
    let world = world_with_token(|runtime| {
        Js::Promise(Promise::settled(
            runtime,
            Err(Thrown::error("Error", "token down")),
        ))
    });
    world.open();
    let channels: Vec<Value> = world.sent(0).iter().map(|f| f["channel"].clone()).collect();
    assert_eq!(channels, vec![Value::Null, json!("account_all/7")]);
    let logs = world.logs();
    assert!(
        logs.contains(
            &"[Lighter][PWS] Auth token unavailable, subscribing without auth.".to_owned()
        )
    );
    assert!(logs.contains(
        &"[Lighter][PWS] Skipping account_all_orders subscription because auth token is unavailable."
            .to_owned()
    ));
    assert!(
        !world
            .socket
            .get("hasPendingAccountAllOrdersSubscribe")
            .unwrap()
            .truthy()
    );
}

#[test]
fn production_private_position_loop_consumes_live_iterators() {
    for shape in ["set", "set-values", "map-values", "array"] {
        let world = world();
        let position = |id| Js::object(vec![("id", Js::str(id)), ("size", Js::str("1"))]);
        let (a, b, c) = (position("a"), position("b"), position("c"));
        let source = match shape {
            "map-values" => {
                let map = Js::new_map();
                map.call_method("set", vec![Js::str("a"), a]).unwrap();
                map.call_method("set", vec![Js::str("b"), b.clone()])
                    .unwrap();
                map
            }
            "array" => Js::array(vec![a, b.clone()]),
            _ => {
                let set = Js::new_set();
                set.call_method("add", vec![a]).unwrap();
                set.call_method("add", vec![b.clone()]).unwrap();
                set
            }
        };
        let input = if shape.ends_with("values") {
            source.call_method("values", vec![]).unwrap()
        } else {
            source.clone()
        };
        let seen = Rc::new(RefCell::new(Vec::new()));
        let trace = seen.clone();
        let parent = world.socket.get("parent").unwrap();
        parent
            .define(
                "mapPosition",
                Js::function("mapPosition", move |_, args| {
                    let id = args[0].get("id")?.to_display();
                    trace.borrow_mut().push(id.clone());
                    if id == "a" {
                        match shape {
                            "map-values" => {
                                source.call_method("delete", vec![Js::str("b")])?;
                                source.call_method("set", vec![Js::str("c"), c.clone()])?;
                            }
                            "array" => {
                                source.set("1", c.clone())?;
                            }
                            _ => {
                                source.call_method("delete", vec![b.clone()])?;
                                source.call_method("add", vec![c.clone()])?;
                            }
                        }
                    }
                    Ok(Js::Undefined)
                }),
            )
            .unwrap();
        world
            .socket
            .call_method("handlePositionTopic", vec![input])
            .unwrap();
        assert_eq!(
            &*seen.borrow(),
            &["a", "c"],
            "{shape}: live source for-of trace"
        );
        assert!(
            world.store.positions().is_empty(),
            "nonzero unmapped rows do not synthesize closes"
        );
    }
}

#[test]
fn production_private_position_loop_closes_only_after_a_body_throw() {
    for mode in ["body", "next", "done", "value"] {
        let world = world();
        let iterator = Js::new_set().call_method("values", vec![]).unwrap();
        let closes = Rc::new(Cell::new(0));
        let close_count = closes.clone();
        let original = Js::str("original failure");
        let receiver = iterator.clone();
        iterator
            .define(
                "return",
                Js::function("return", move |this, _| {
                    assert!(this.same_reference(&receiver), "IteratorClose receiver");
                    close_count.set(close_count.get() + 1);
                    Err(Thrown::error(
                        "Error",
                        "close failure must not replace the body throw",
                    ))
                }),
            )
            .unwrap();
        let failure = original.clone();
        iterator
            .define(
                "next",
                Js::function("next", move |_, _| {
                    if mode == "next" {
                        return Err(Thrown::new(failure.clone()));
                    }
                    let step = Js::object(vec![
                        ("done", Js::Bool(false)),
                        ("value", Js::object(vec![("size", Js::str("1"))])),
                    ]);
                    if mode == "done" || mode == "value" {
                        let failure = failure.clone();
                        step.define_accessor(
                            mode,
                            Some(Rc::new(move |_| Err(Thrown::new(failure.clone())))),
                            None,
                        );
                    }
                    Ok(step)
                }),
            )
            .unwrap();
        let parent = world.socket.get("parent").unwrap();
        parent
            .define(
                "mapPosition",
                Js::function("mapPosition", |_, _| {
                    Err(Thrown::error("Error", "mapping failure"))
                }),
            )
            .unwrap();
        let failure = original.clone();
        parent
            .define(
                "log",
                Js::function("log", move |_, _| Err(Thrown::new(failure.clone()))),
            )
            .unwrap();
        let error = world
            .socket
            .call_method("handlePositionTopic", vec![iterator])
            .unwrap_err();
        assert!(
            error.value.strict_equals(&original),
            "{mode}: original throw wins"
        );
        assert_eq!(
            closes.get(),
            usize::from(mode == "body"),
            "{mode}: IteratorClose boundary"
        );
    }
}

#[test]
fn production_private_position_close_spread_accepts_iterator_values() {
    let world = world();
    // Use the actual Store mutator rather than this fixture's dispatch recorder.
    let native_store = store::facade(world.store.clone());
    world
        .socket
        .get("store")
        .unwrap()
        .define(
            "updatePositions",
            native_store.get("updatePositions").unwrap(),
        )
        .unwrap();
    world
        .store
        .add_or_update_position(
            serde_json::from_value(json!({
                "id":"p", "symbol":"ETH", "side":"long", "entryPrice":10,
                "notional":10, "leverage":2, "unrealizedPnl":0, "contracts":1,
                "liquidationPrice":1, "marginSize":120
            }))
            .unwrap(),
        )
        .unwrap();
    let pair = Js::array(vec![
        Js::object(vec![("id", Js::str("p"))]),
        Js::object(vec![
            ("contracts", Js::Number(0.0)),
            ("marginSize", Js::Undefined),
        ]),
    ]);
    let closes = Js::new_set();
    closes.call_method("add", vec![pair]).unwrap();
    let values = closes.call_method("values", vec![]).unwrap();
    world
        .socket
        .define(
            "resolveExplicitlyClosedPositionUpdates",
            Js::function("resolveExplicitlyClosedPositionUpdates", move |_, _| {
                Ok(values.clone())
            }),
        )
        .unwrap();
    world
        .socket
        .get("parent")
        .unwrap()
        .define(
            "mapPosition",
            Js::function("mapPosition", |_, _| Ok(Js::Undefined)),
        )
        .unwrap();
    let revision = world.store.update_ids().positions;
    world
        .socket
        .call_method(
            "handlePositionTopic",
            vec![Js::array(vec![Js::object(vec![("size", Js::str("0"))])])],
        )
        .unwrap();
    let positions = world.store.positions();
    assert_eq!(
        positions[0].contracts, 0.0,
        "spread must deliver its close pair to the actual Store"
    );
    assert!(positions[0].margin_size.is_undefined());
    assert_eq!(world.store.update_ids().positions, revision + 1.0);
}

#[test]
fn orders_flattening_calls_the_live_owner_helper_after_its_type_guard() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-state-oracle.cjs"
        ))
        .output()
        .expect("maintained Lighter flattening arrows");
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    let world = world();
    let payload = Js::object(vec![]);
    let expected = Js::array(vec![]);
    let calls = Rc::new(RefCell::new(vec![]));
    let observed = calls.clone();
    let (owner, input, result) = (world.socket.clone(), payload.clone(), expected.clone());
    world
        .socket
        .set(
            "flattenByMarketPayload",
            Js::function("input", move |this, args| {
                observed.borrow_mut().push(json!({
                    "receiver":this.same_reference(&owner),
                    "payload":args[0].same_reference(&input),
                    "include":args[1].truthy(),
                }));
                Ok(result.clone())
            }),
        )
        .unwrap();
    let result = world
        .socket
        .call_method("flattenOrdersByMarket", vec![payload])
        .unwrap();
    let string = world
        .socket
        .call_method("flattenOrdersByMarket", vec![Js::str("x")])
        .unwrap();
    let function = world
        .socket
        .call_method(
            "flattenOrdersByMarket",
            vec![Js::function("input", |_, _| Ok(Js::Undefined))],
        )
        .unwrap();
    assert_eq!(
        json!({
            "same":result.same_reference(&expected), "calls":*calls.borrow(),
            "stringLength":string.array_values().unwrap().len(),
            "functionLength":function.array_values().unwrap().len(),
        }),
        source["flattenOrdersDispatch"]
    );
}
