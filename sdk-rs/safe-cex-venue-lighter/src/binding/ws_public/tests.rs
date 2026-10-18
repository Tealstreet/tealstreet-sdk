//! `LighterPublicWebsocket` constructed from the realm's globals over a scripted
//! WsRuntime: subscribe frames, ping, ticker/order-book ingress and the close
//! classification that picks the reconnect delay.
use super::super::{
    exchange::{LighterDependencies, build_realm},
    realm::Realm,
};
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch},
    js_runtime::{
        client::JsHttpHost,
        promise::{JsFuture, Promise},
        value::{Js, Thrown, arg, from_json, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_store::{Store, virtual_clock::VIRTUAL_CLOCK};
use safe_cex_transport::{
    TransportProvider, TransportRegistry,
    socket::{SocketFrame, TunnelHost},
};
use safe_cex_types::{ExchangeName, ExchangeOptions, Order, PlaceOrderOpts};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll},
};

use super::super::test_time::{SleepWakers, WakeSignal};

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

#[derive(Default)]
struct Io {
    sent: RefCell<Vec<String>>,
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
    fn send(&self, data: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(data.into());
        Ok(())
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct Runtime {
    now: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Job>>,
    sleepers: SleepWakers,
    sockets: RefCell<Vec<Rc<Io>>>,
    dials: RefCell<Vec<String>>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    fn pump(&self) {
        let signal = Arc::new(WakeSignal::default());
        let waker = signal.waker();
        for _ in 0..64 {
            let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
            if jobs.is_empty() {
                return;
            }
            let mut cx = Context::from_waker(&waker);
            let mut pending = vec![];
            let mut progressed = false;
            for (mut future, cancelled) in jobs {
                if cancelled.get() {
                    progressed = true;
                    continue;
                }
                if future.as_mut().poll(&mut cx).is_pending() {
                    pending.push((future, cancelled));
                } else {
                    progressed = true;
                }
            }
            let spawned = !self.jobs.borrow().is_empty();
            self.jobs.borrow_mut().extend(pending);
            self.sleepers.wake_due(self.now.get());
            if !progressed && !spawned && !signal.take() {
                return;
            }
        }
    }
    fn advance(&self, ms: f64) {
        self.now.set(self.now.get() + ms);
        self.sleepers.wake_due(self.now.get());
        self.pump();
    }
    fn socket(&self) -> Rc<Io> {
        self.sockets
            .borrow()
            .last()
            .cloned()
            .expect("a dialed socket")
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
        let sleep_waker = self.sleepers.register(at);
        Box::pin(std::future::poll_fn(move |cx| {
            if now.get() >= at {
                Poll::Ready(Ok(()))
            } else {
                *sleep_waker.borrow_mut() = Some(cx.waker().clone());
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
        0.25
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected exception {context:?}: {}", error.message);
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
        panic!("public socket tests dial directly")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let socket = Rc::new(Io::default());
        self.dials.borrow_mut().push(url);
        self.sockets.borrow_mut().push(socket.clone());
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
struct Http(Rc<Runtime>);
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        Box::pin(async { Err(Thrown::error("Error", "public socket tests send no HTTP")) })
    }
}

struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_orders(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn place_orders<'a>(&'a self, _: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        Box::pin(async { Ok(vec![]) })
    }
}

type Calls = Rc<RefCell<Vec<Value>>>;

struct Fixture {
    runtime: Rc<Runtime>,
    realm: Realm,
    socket: Js,
    logs: Calls,
    rest: Calls,
    book_requests: Calls,
    ticker_updates: Calls,
    emitter: Calls,
    _venue: Rc<dyn Exchange>,
}

fn json_of(value: &Js) -> Value {
    to_json(value).unwrap_or_else(|e| panic!("json: {}", e.message()))
}

/// A function recording its JSON arguments, returning `result(args)`.
fn recorder(calls: &Calls, result: impl Fn(&[Js]) -> Js + 'static) -> Js {
    let calls = calls.clone();
    Js::function("", move |_, args| {
        calls.borrow_mut().push(Value::Array(
            args.iter()
                .map(|a| to_json(a).unwrap_or(Value::Null))
                .collect(),
        ));
        Ok(result(&args))
    })
}

fn market() -> Value {
    json!({"id": "1", "symbol": "ETH/USD", "base": "ETH", "quote": "USD", "active": true})
}

fn ticker() -> Value {
    json!({
        "id": "1", "symbol": "ETH/USD", "bid": 0, "ask": 0, "last": 0, "mark": 0, "index": 0,
        "percentage": 0, "openInterest": 0, "fundingRate": 0, "fundingTime": 0, "volume": 0,
        "quoteVolume": 0
    })
}

fn fixture() -> Fixture {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(1_000_000.0);
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key": "k", "secret": "s"})).unwrap();
    let store: Rc<dyn Store> = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
    let deps = LighterDependencies {
        runtime: runtime.clone(),
        http: Rc::new(Http(runtime.clone())),
        registry: Rc::new(TransportRegistry::default()),
        signing_entropy: None,
    };
    let realm = build_realm(&ExchangeName::Lighter, &options, &store, &deps)
        .unwrap_or_else(|e| panic!("realm: {}", e.message()));
    let venue: Rc<dyn Exchange> = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Lighter,
            options,
            store,
            runtime.clone(),
            ExchangeSettings::default(),
        ),
        capabilities: ExchangeCapabilities::default(),
    });
    *realm.0.sockets.venue.borrow_mut() = Some(Rc::downgrade(&venue));

    let calls = || Calls::default();
    let (logs, rest, book_requests, ticker_updates, emitter) =
        (calls(), calls(), calls(), calls(), calls());
    let settled = {
        let runtime: Rc<dyn ExchangeRuntime> = runtime.clone();
        move |value: Value| Js::Promise(Promise::settled(&runtime, Ok(from_json(&value))))
    };
    let book_payload = settled.clone();
    let stats_payload = settled;
    let store = Js::object(vec![
        ("markets", from_json(&json!([market()]))),
        ("marketsBySymbol", from_json(&json!({"ETH/USD": market()}))),
        ("tickers", from_json(&json!([ticker()]))),
        ("positions", Js::array(vec![])),
        ("updateTicker", recorder(&ticker_updates, |_| Js::Undefined)),
    ]);
    let on_calls = emitter.clone();
    let off_calls = emitter.clone();
    let parent = Js::object(vec![
        ("name", Js::str("lighter")),
        ("options", from_json(&json!({"key": "k", "secret": "s"}))),
        ("store", store),
        (
            "emitter",
            Js::object(vec![
                (
                    "on",
                    Js::function("on", move |_, a| {
                        on_calls
                            .borrow_mut()
                            .push(json!(["on", json_of(&arg(&a, 0))]));
                        Ok(Js::Undefined)
                    }),
                ),
                (
                    "off",
                    Js::function("off", move |_, a| {
                        off_calls
                            .borrow_mut()
                            .push(json!(["off", json_of(&arg(&a, 0))]));
                        Ok(Js::Undefined)
                    }),
                ),
            ]),
        ),
        ("isDisposed", Js::Bool(false)),
        (
            "lighterDeploymentProfile",
            from_json(&json!({"supportsWsProxy": false, "wsUrl": "wss://lighter.test/stream"})),
        ),
        (
            "orderApi",
            Js::object(vec![(
                "getOrderBookOrders",
                recorder(&book_requests, move |_| {
                    book_payload(json!({
                        "bids": [{"price": "100", "remaining_base_amount": "2"}],
                        "asks": [{"price": "101", "remaining_base_amount": "1"}]
                    }))
                }),
            )]),
        ),
        (
            "lighterApiClient",
            Js::object(vec![(
                "get",
                recorder(&rest, move |_| stats_payload(json!({}))),
            )]),
        ),
        ("log", recorder(&logs, |_| Js::Undefined)),
        (
            "syncSocketHealthToStore",
            Js::function("", |_, _| Ok(Js::Undefined)),
        ),
        (
            "emitAndLogError",
            Js::function("", |_, _| Ok(Js::Undefined)),
        ),
    ]);
    let class = realm.global("LighterPublicWebsocket").unwrap();
    let socket = class
        .construct(vec![parent], None)
        .unwrap_or_else(|e| panic!("construct: {}", e.message()));
    Fixture {
        runtime,
        realm,
        socket,
        logs,
        rest,
        book_requests,
        ticker_updates,
        emitter,
        _venue: venue,
    }
}

impl Fixture {
    fn call(&self, member: &str, args: Vec<Js>) -> Js {
        self.socket
            .call_method(member, args)
            .unwrap_or_else(|e| panic!("{member}: {}", e.message()))
    }
    fn field(&self, name: &str) -> Value {
        json_of(&self.socket.get(name).unwrap())
    }
    fn deliver(&self, event: WsEvent) {
        self.runtime.socket().events.borrow_mut().push_back(event);
        self.runtime.pump();
    }
    fn open(&self) {
        self.call("connectAndSubscribe", vec![]);
        self.runtime.pump();
        self.deliver(WsEvent::Open);
    }
    fn frames(&self) -> Vec<Value> {
        self.runtime
            .socket()
            .sent
            .borrow()
            .iter()
            .map(|f| serde_json::from_str(f).unwrap())
            .collect()
    }
    fn log_lines(&self) -> Vec<String> {
        self.logs
            .borrow()
            .iter()
            .map(|args| {
                format!(
                    "{}|{}",
                    args[0].as_str().unwrap_or(""),
                    args[1].as_str().unwrap_or("")
                )
            })
            .collect()
    }
}

#[test]
fn every_name_the_public_socket_bodies_resolve_is_bound() {
    let f = fixture();
    let names = [
        "Array.from",
        "Array.isArray",
        "BaseWebSocket",
        "Boolean",
        "Date.now",
        "ENDPOINTS.EXCHANGE_STATS",
        "Error",
        "JSON.stringify",
        "LighterPublicWebsocket",
        "LogSeverity.Warning",
        "Map",
        "Math.max",
        "Math.min",
        "Math.round",
        "Number",
        "Number.isFinite",
        "Number.parseInt",
        "Object.entries",
        "Object.keys",
        "Object.values",
        "Set",
        "String",
        "WS_MIN_RECONNECTION_DELAY",
        "add",
        "buildLighterCandleTopic",
        "buildLighterTradesTopic",
        "calcOrderBookTotal",
        "calculateLighterPublicReconnectDelay",
        "classifyLighterPublicClose",
        "clearInterval",
        "clearTimeout",
        "extractLighterTickerMarketIdFromChannel",
        "flatten",
        "getCandleStartTime",
        "jsonParse",
        "nanoid",
        "normalizeLighterFundingRate",
        "parseFloat",
        "parseLighterWsBookSidePrice",
        "parseLighterWsNumber",
        "performance.now",
        "resolveLighterInterval",
        "resolveTransportRouting",
        "setInterval",
        "setTimeout",
        "shouldAttemptLighterOhlcvRecovery",
        "shouldDispatchLighterPublicMessage",
        "shouldPromoteLighterOhlcvRecoveryToProxy",
        "updateOrderBook",
        "virtualClock.getCurrentTimeMs",
    ];
    let missing: Vec<&str> = names
        .into_iter()
        .filter(|name| f.realm.global(name).map_or(true, |v| v.is_undefined()))
        .collect();
    assert!(missing.is_empty(), "unbound globals: {missing:?}");
}

#[test]
fn constructor_runs_base_then_derived_fields_and_statics() {
    let f = fixture();
    let class = f.realm.global("LighterPublicWebsocket").unwrap();
    assert_eq!(
        json_of(&class.get("ORDER_BOOK_STALE_TIMEOUT_MS").unwrap()),
        json!(90000.0)
    );
    assert_eq!(
        json_of(&class.get("OHLCV_DIRECT_ROUTE_NO_DATA_TIMEOUT_MS").unwrap()),
        json!(120000.0)
    );
    assert_eq!(
        f.field("topics"),
        json!({"marketStatsAll": "market_stats/all"})
    );
    assert_eq!(f.field("bboTickerTopicPrefix"), json!("bbo_ticker_"));
    assert!(f.socket.get("connectAndSubscribe").unwrap().is_callable());
    assert!(
        f.socket.get("reconnect").unwrap().is_callable(),
        "BaseWebSocket members"
    );
    assert_eq!(f.field("isConnected"), json!(false));
    let Js::Object(instance) = &f.socket else {
        panic!("instance is an object")
    };
    let prototype = class.get("prototype").unwrap();
    let proto = instance.proto.borrow().clone().expect("instance prototype");
    assert!(proto.same_reference(&prototype), "instance prototype");
}

#[test]
fn unhydrated_tickers_fall_back_to_rest_polling() {
    let f = fixture();
    f.open();
    assert_eq!(f.rest.borrow().len(), 1, "primed once on open");
    f.runtime.advance(8000.0);
    assert_eq!(
        f.rest.borrow().len(),
        2,
        "watchdog started the REST fallback"
    );
    assert!(
        f.socket
            .get("tickerRestFallbackInterval")
            .unwrap()
            .is_object()
    );
    f.runtime.advance(3000.0);
    assert_eq!(f.rest.borrow().len(), 3, "interval poll");
}

#[test]
fn open_subscribes_topics_primes_tickers_and_pings() {
    let f = fixture();
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    f.open();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let dial = f.runtime.dials.borrow()[0].clone();
    assert!(
        dial.starts_with("wss://lighter.test/stream?requestId="),
        "dial url: {dial}"
    );
    assert!(!dial.contains("readonly"), "dial url: {dial}");
    let frames = f.frames();
    assert!(
        frames.contains(&json!({"type": "subscribe", "channel": "market_stats/all"})),
        "subscribe market stats: {frames:?}"
    );
    assert!(
        frames.contains(&json!({"type": "subscribe", "channel": "ticker/1"})),
        "subscribe bbo ticker: {frames:?}"
    );
    let pings: Vec<_> = frames.iter().filter(|f| f["type"] == "ping").collect();
    assert_eq!(pings.len(), 1, "one ping on open");
    let timestamp = pings[0]["timestamp"]
        .as_f64()
        .expect("native ping timestamp");
    assert!(
        timestamp >= before && timestamp <= after,
        "ping uses the synced virtual clock"
    );
    assert_eq!(
        *pings[0],
        json!({"type":"ping", "timestamp":timestamp as u64})
    );
    assert_eq!(f.field("topics")["bbo_ticker_1"], json!("ticker/1"));
    assert_eq!(f.field("pingAt"), json!(1_000_000.0), "performance.now");
    assert_eq!(
        *f.rest.borrow(),
        vec![json!(["/api/v1/exchangeStats", {"__suppressExchangeError": true}])],
        "primed ticker REST fallback"
    );
    assert_eq!(
        *f.emitter.borrow(),
        vec![
            json!(["off", "markets:changed"]),
            json!(["on", "markets:changed"])
        ]
    );
    assert_eq!(f.field("hasEverConnected"), json!(true));
    assert_eq!(f.field("openedSinceLastConnect"), json!(true));
}

#[test]
fn market_stats_message_updates_the_store_ticker() {
    let f = fixture();
    f.open();
    let frame = json!({
        "channel": "market_stats:all",
        "type": "update/market_stats",
        "market_stats": {"1": {
            "symbol": "ETH/USD", "market_id": 1, "mark_price": "2500.5",
            "index_price": "2500", "last_trade_price": "2501", "current_funding_rate": "0.0012"
        }}
    });
    f.deliver(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    assert_eq!(f.field("hasReceivedMessageSinceConnect"), json!(true));
    let updates = f.ticker_updates.borrow().clone();
    assert_eq!(updates.len(), 1, "ticker updates: {updates:?}");
    assert_eq!(updates[0][0]["id"], json!("1"));
    let patch = &updates[0][1];
    assert_eq!(patch["mark"], json!(2500.5), "mark: {patch}");
    assert_eq!(patch["index"], json!(2500.0), "index: {patch}");
    assert_eq!(patch["last"], json!(2501.0), "last: {patch}");
    assert_eq!(
        patch["fundingRate"],
        json!(0.0001),
        "normalized funding: {patch}"
    );
}

#[test]
fn order_book_snapshot_from_rest_then_websocket_delta() {
    let f = fixture();
    f.open();
    let books = Calls::default();
    let callback = recorder(&books, |_| Js::Undefined);
    let stop = f.call("listenOrderBook", vec![Js::str("ETH/USD"), callback]);
    f.runtime.pump();
    assert!(
        f.frames()
            .contains(&json!({"type": "subscribe", "channel": "order_book/1"})),
        "subscribe order book"
    );
    assert_eq!(
        *f.book_requests.borrow(),
        vec![json!([1.0, 100.0, {"__suppressExchangeError": true}])],
        "REST order book request"
    );
    let snapshot = books
        .borrow()
        .last()
        .cloned()
        .expect("REST snapshot emitted");
    assert_eq!(
        snapshot[0]["bids"][0]["price"],
        json!(100.0),
        "snapshot: {snapshot}"
    );
    assert_eq!(
        snapshot[0]["bids"][0]["amount"],
        json!(2.0),
        "snapshot: {snapshot}"
    );
    assert_eq!(
        snapshot[0]["asks"][0]["price"],
        json!(101.0),
        "snapshot: {snapshot}"
    );
    let delta = json!({
        "channel": "order_book:1",
        "type": "update/order_book",
        "order_book": {"bids": [{"price": "100.5", "size": "3"}], "asks": []}
    });
    f.deliver(WsEvent::Message(SocketFrame::Text(delta.to_string())));
    let book = books.borrow().last().cloned().unwrap();
    let bids: Vec<f64> = book[0]["bids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["price"].as_f64().unwrap())
        .collect();
    assert!(
        bids.contains(&100.5) && bids.contains(&100.0),
        "delta merged: {book}"
    );
    stop.call(Js::Undefined, vec![]).unwrap();
    assert_eq!(f.field("orderBookRestFallbackIntervals"), json!({}));
}

/// The stop closure filters its parser out by `f !== parser` on AccountValue
/// identity, which binding/value.rs does not keep for functions (a fresh
/// HostThrownValue per crossing), so the handler is never removed.
#[test]
fn order_book_stop_removes_its_parser_and_unsubscribes() {
    let f = fixture();
    f.open();
    let stop = f.call(
        "listenOrderBook",
        vec![
            Js::str("ETH/USD"),
            Js::function("", |_, _| Ok(Js::Undefined)),
        ],
    );
    f.runtime.pump();
    stop.call(Js::Undefined, vec![]).unwrap();
    assert_eq!(f.field("messageHandlers")["order_book:1"], json!([]));
    let frames = f.frames();
    assert!(
        frames.contains(&json!({"type": "unsubscribe", "channel": "order_book/1"})),
        "unsubscribe on stop: {frames:?}"
    );
}

#[test]
fn abnormal_close_counts_a_failure_and_reconnects_after_backoff() {
    let f = fixture();
    f.open();
    f.deliver(WsEvent::Message(SocketFrame::Close {
        code: 1006,
        reason: String::new(),
        was_clean: false,
    }));
    assert_eq!(f.field("failedConnections"), json!(1.0));
    assert_eq!(f.field("reconnecting"), json!(true));
    let logs = f.log_lines();
    assert!(
        logs.contains(&"[Lighter] Public WS closed (code 1006)|warning".to_string()),
        "close log: {logs:?}"
    );
    assert!(
        logs.contains(&"[Lighter] Public WS reconnecting in 1500ms...|info".to_string()),
        "backoff log: {logs:?}"
    );
    assert_eq!(f.runtime.dials.borrow().len(), 1);
    f.runtime.advance(1499.0);
    assert_eq!(
        f.runtime.dials.borrow().len(),
        1,
        "no reconnect before the delay"
    );
    f.runtime.advance(1.0);
    assert_eq!(
        f.runtime.dials.borrow().len(),
        2,
        "reconnected after the delay"
    );
    assert_eq!(f.field("reconnecting"), json!(false));
}

#[test]
fn rate_limited_close_waits_the_rate_limit_delay() {
    let f = fixture();
    f.open();
    f.deliver(WsEvent::Message(SocketFrame::Close {
        code: 1013,
        reason: "server busy".into(),
        was_clean: false,
    }));
    let logs = f.log_lines();
    assert!(
        logs.contains(&"[Lighter] Public WS reconnecting in 30000ms...|info".to_string()),
        "rate-limit log: {logs:?}"
    );
    assert!(
        logs.contains(
            &"[Lighter] Public WS closed (code 1013, reason: server busy)|warning".to_string()
        ),
        "close log: {logs:?}"
    );
}

#[test]
fn normal_close_resets_failures() {
    let f = fixture();
    f.open();
    f.socket.set("failedConnections", Js::Number(3.0)).unwrap();
    f.deliver(WsEvent::Message(SocketFrame::Text(
        json!({"channel": "market_stats:all", "market_stats": {}}).to_string(),
    )));
    f.socket.set("failedConnections", Js::Number(3.0)).unwrap();
    f.deliver(WsEvent::Message(SocketFrame::Close {
        code: 1000,
        reason: String::new(),
        was_clean: true,
    }));
    assert_eq!(f.field("failedConnections"), json!(0.0));
}

#[test]
fn dispose_stops_the_rest_fallback_interval() {
    let f = fixture();
    f.open();
    f.runtime.advance(8000.0);
    assert_eq!(f.rest.borrow().len(), 2, "REST fallback running");
    f.call("dispose", vec![]);
    assert_eq!(f.field("tickerRestFallbackInterval"), Value::Null);
    assert_eq!(f.field("ohlcvNoDataTimeouts"), json!({}));
    f.runtime.advance(60_000.0);
    assert_eq!(f.rest.borrow().len(), 2, "no REST poll after dispose");
}

/// js_runtime `Scope::clear_timeout` drops the task without `cancel()`, so on
/// a host where drop does not cancel, a cleared timeout still fires.
#[test]
fn dispose_clears_the_ticker_watchdog_timeout() {
    let f = fixture();
    f.open();
    f.call("dispose", vec![]);
    assert_eq!(f.field("tickerRestFallbackWatchdog"), Value::Null);
    f.runtime.advance(60_000.0);
    assert_eq!(f.rest.borrow().len(), 1, "cleared watchdog never fires");
}

#[test]
fn public_callback_iterator_closes_and_retains_original_throw() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public-iterator-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "actual source public iterator: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    let mut observations = vec![];
    for fails_close in [false, true] {
        let f = fixture();
        let original = Js::object(vec![("tag", Js::str("callback"))]);
        let failure = original.clone();
        let callbacks = Js::new_set();
        callbacks
            .call_method(
                "add",
                vec![Js::function("callback", move |_, _| {
                    Err(Thrown::new(failure.clone()))
                })],
            )
            .unwrap();
        let iterator = callbacks.call_method("values", vec![]).unwrap();
        let closes = Rc::new(Cell::new(0));
        let counter = closes.clone();
        iterator
            .define(
                "return",
                Js::function("return", move |_, _| {
                    counter.set(counter.get() + 1);
                    if fails_close {
                        Err(Thrown::type_error("close"))
                    } else {
                        Ok(Js::object(vec![("done", Js::Bool(true))]))
                    }
                }),
            )
            .unwrap();
        f.socket
            .get("messageHandlers")
            .unwrap()
            .define("order_book:1", iterator)
            .unwrap();
        let result = f
            .socket
            .call_method(
                "onMessage",
                vec![Js::object(vec![(
                    "data",
                    Js::str("{\"channel\":\"order_book:1\"}"),
                )])],
            )
            .unwrap_err();
        observations.push(json!({"failsClose":fails_close,"closes":closes.get(),"original":result.value.same_reference(&original)}));
    }
    assert_eq!(
        json!(observations),
        source,
        "Lighter public callback IteratorClose runs once and original throw wins over close failure"
    );
}

#[test]
fn public_ingress_pair_uses_iterator_and_closes_after_two_values() {
    for invalid_return in [false, true] {
        let f = fixture();
        let ops = crate::binding::ingress::ops(&f.realm);
        let values = Js::new_set();
        for value in [Js::str("topic"), Js::Number(23.0), Js::str("unused")] {
            values.call_method("add", vec![value]).unwrap();
        }
        let iterator = values.call_method("values", vec![]).unwrap();
        let closes = Rc::new(Cell::new(0));
        let counter = closes.clone();
        iterator
            .define(
                "return",
                Js::function("return", move |_, _| {
                    counter.set(counter.get() + 1);
                    Ok(if invalid_return {
                        Js::Number(0.0)
                    } else {
                        Js::object(vec![])
                    })
                }),
            )
            .unwrap();
        let result = (ops.tuple_two)(&iterator);
        if invalid_return {
            assert!(
                result.is_err(),
                "pair IteratorClose refuses primitive return"
            );
        } else {
            let (first, second) = result.unwrap();
            assert!(
                first.strict_equals(&Js::str("topic")),
                "public pair consumes the iterator's first value"
            );
            assert!(
                second.strict_equals(&Js::Number(23.0)),
                "public pair consumes the iterator's second value"
            );
        }
        assert_eq!(
            closes.get(),
            1,
            "pair closes before exhausting its unused tail"
        );
    }
}

#[test]
fn public_rest_parsers_retain_native_rows_arrays_and_source_read_order() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-payloads-oracle.cjs"
        ))
        .output()
        .expect("maintained public payload parser arrows");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected["native"].as_array().unwrap().len(), 2);
    assert_eq!(expected["reads"].as_array().unwrap().len(), 2);
    let f = fixture();
    let mut native = vec![];
    for kind in ["array", "object"] {
        let row = Js::object(vec![
            ("price", Js::Number(f64::NAN)),
            ("amount", Js::Number(f64::INFINITY)),
            ("zero", Js::Number(-0.)),
            ("label", Js::utf16(&[0xd800])),
            ("missing", Js::Undefined),
        ]);
        row.set("cycle", row.clone()).unwrap();
        let bids = Js::holey_array(vec![None, Some(row.clone()), Some(Js::Undefined)]);
        let asks = Js::array(vec![]);
        let tickers = if kind == "array" {
            bids.clone()
        } else {
            Js::object(vec![("2", row.clone()), ("1", Js::Undefined)])
        };
        let payload = Js::object(vec![
            (
                "order_book",
                Js::object(vec![("bids", bids.clone()), ("asks", asks.clone())]),
            ),
            ("order_book_stats", tickers.clone()),
        ]);
        let book = f
            .socket
            .call_method("parseRestOrderBookLevels", vec![payload.clone()]);
        let items = f.socket.call_method("parseRestTickerItems", vec![payload]);
        let (book, items) = match (book, items) {
            (Ok(book), Ok(items)) => (book, items),
            (book, items) => {
                native.push(json!({"kind":kind,
                    "bookError":book.err().map(|error| error.message()),
                    "tickerError":items.err().map(|error| error.message()),
                }));
                row.delete(&"cycle".encode_utf16().collect::<Vec<_>>())
                    .unwrap();
                continue;
            }
        };
        let entries = items
            .array_items()
            .expect("fixture array property reads")
            .unwrap();
        let observed = entries
            .iter()
            .flatten()
            .find(|v| !v.is_undefined())
            .unwrap();
        let tags: Vec<_> = entries
            .iter()
            .map(|v| match v {
                None => "hole",
                Some(v) if v.same_reference(&row) => "row",
                Some(v) if v.is_undefined() => "undefined",
                _ => "other",
            })
            .collect();
        native.push(json!({"kind":kind,
            "bidsIdentity":book.get("bids").unwrap().same_reference(&bids),
            "asksIdentity":book.get("asks").unwrap().same_reference(&asks),
            "tickerIdentity":items.same_reference(&tickers),
            "bidHole":book.get("bids").unwrap().array_items().expect("fixture array property reads").unwrap()[0].is_none(),
            "tags":tags,
            "rowIdentity":entries.iter().flatten().filter(|v| !v.is_undefined()).all(|v| v.same_reference(&row)),
            "nan":observed.get("price").unwrap().to_number().unwrap().is_nan(),
            "infinity":observed.get("amount").unwrap().to_number().unwrap() == f64::INFINITY,
            "zero":observed.get("zero").unwrap().to_number().unwrap().to_bits() == (-0_f64).to_bits(),
            "units":observed.get("label").unwrap().to_js_string().unwrap().as_ref(),
            "ownUndefined":observed.own_keys().contains(&"missing".encode_utf16().collect()) && observed.get("missing").unwrap().is_undefined(),
            "cycle":observed.get("cycle").unwrap().same_reference(&row),
        }));
        row.delete(&"cycle".encode_utf16().collect::<Vec<_>>())
            .unwrap();
    }
    let mut reads = vec![];
    for method in ["parseRestOrderBookLevels", "parseRestTickerItems"] {
        let trace = Rc::new(RefCell::new(vec![]));
        let row = Js::object(vec![]);
        let bids = Js::array(vec![row.clone()]);
        let book = Js::object(vec![]);
        for (name, value) in [("bids", bids), ("asks", Js::array(vec![]))] {
            let trace = trace.clone();
            book.define_accessor(
                name,
                Some(Rc::new(move |_| {
                    trace.borrow_mut().push(name);
                    Ok(value.clone())
                })),
                None,
            )
            .expect("fresh extensible accessor target");
        }
        let payload = Js::object(vec![
            ("order_book", book),
            ("order_book_stats", Js::array(vec![row])),
        ]);
        for name in ["data", "result"] {
            let trace = trace.clone();
            payload
                .define_accessor(
                    name,
                    Some(Rc::new(move |_| {
                        trace.borrow_mut().push(name);
                        Ok(Js::object(vec![]))
                    })),
                    None,
                )
                .expect("fresh extensible accessor target");
        }
        f.call(method, vec![payload]);
        reads.push(json!({"method":method,"trace":*trace.borrow()}));
    }
    assert_eq!(json!({"native":native,"reads":reads}), expected);
}
