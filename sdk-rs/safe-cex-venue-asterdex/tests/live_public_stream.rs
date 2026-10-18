#[allow(dead_code)]
#[path = "support/subscription_runtime.rs"]
mod host;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask, LogSeverity},
    websocket::{
        BaseWebSocket, ReopenResync, WebSocketConnectionOptions, WsEvent, WsParent, WsRuntime,
        WsSocket,
    },
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    TransportProvider,
    socket::{SocketFrame, TunnelHost},
};
use safe_cex_types::{ExchangeName, ExchangeOptions, OrderBook, Ticker, Timeframe};
use safe_cex_venue_asterdex::{
    public_stream::{PublicDataListeners, PublicStream, PublicStreamParent},
    public_subscriptions::{BookCleanup, PublicSubscriptionState},
    ws_helpers::{PublicKlineCandle, PublicStreamTradeEvent},
};
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
#[derive(Default)]
struct Socket {
    events: Rc<RefCell<VecDeque<WsEvent>>>,
    wire: RefCell<Vec<String>>,
    closed: Cell<bool>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        if self.closed.get() { 3 } else { 1 }
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(std::future::poll_fn(move |_| {
            events
                .borrow_mut()
                .pop_front()
                .map_or(Poll::Pending, |e| Poll::Ready(Ok(e)))
        }))
    }
    fn send(&self, text: &str) -> Result<(), ExchangeError> {
        self.wire.borrow_mut().push(text.into());
        Ok(())
    }
    fn close(&self) {
        self.closed.set(true);
    }
    fn terminate(&self) {
        self.closed.set(true);
    }
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        true
    }
}
struct Runtime {
    host: Rc<host::Runtime>,
    socket: Rc<Socket>,
    urls: RefCell<Vec<String>>,
    fail: Option<String>,
    reconnecting: bool,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.host.monotonic_now_ms()
    }
    fn now_ms(&self) -> f64 {
        self.host.now_ms()
    }
    fn sleep(&self, d: u64) -> ExchangeFuture<'static, ()> {
        self.host.sleep(d)
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.host.spawn_local(f)
    }
    fn console_values(&self, s: &str, m: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.host.console_values(s, m)
    }
    fn unique_id(&self) -> String {
        self.host.unique_id()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        e: &ExchangeError,
        c: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.host.report_exception(e, c)
    }
    fn execute_worker_request(&self, r: Value) -> ExchangeFuture<'static, Value> {
        self.host.execute_worker_request(r)
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::direct::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("direct fixture must not use relay")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.urls.borrow_mut().push(url);
        let result = self.fail.as_ref().map_or_else(
            || Ok(self.socket.clone() as Rc<dyn WsSocket>),
            |e| Err(ExchangeError::new(e)),
        );
        Box::pin(async move { result })
    }
    fn headers_capable(&self) -> bool {
        true
    }
    fn enable_reconnecting(&self) -> bool {
        self.reconnecting
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
#[derive(Clone)]
enum Event {
    Ticker(String, Value, Ticker),
    Trade(PublicStreamTradeEvent),
}
struct Parent {
    owner: Rc<BaseExchange>,
    disposed: Cell<bool>,
    events: RefCell<Vec<Event>>,
    coordinator: Rc<ReopenResync>,
    reconnected: Cell<usize>,
    close_expected: Cell<bool>,
    close_states: RefCell<Vec<(bool, usize)>>,
    stream: RefCell<Weak<PublicStream>>,
}
impl WsParent for Parent {
    fn emit_and_log_error(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn name(&self) -> &str {
        "asterdex"
    }
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn options(&self) -> ExchangeOptions {
        self.owner.options()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.owner.store.clone()
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        false
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.coordinator.clone()
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        panic!("public socket must not resync private state")
    }
    fn log(&self, _: &str, _: LogSeverity) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        if self.close_expected.replace(false) {
            let state = self
                .stream
                .borrow()
                .upgrade()
                .unwrap()
                .subscriptions
                .state();
            self.close_states
                .borrow_mut()
                .push((state.ready, state.pending.len()));
        }
        Ok(())
    }
    fn update_latency(&self, _: f64) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.reconnected.set(self.reconnected.get() + 1);
        Ok(())
    }
}
impl PublicStreamParent for Parent {
    fn owner(&self) -> Rc<BaseExchange> {
        self.owner.clone()
    }
    fn emit_ticker(
        &self,
        kind: &str,
        symbol: &Value,
        ticker: &Ticker,
    ) -> Result<(), ExchangeError> {
        self.events
            .borrow_mut()
            .push(Event::Ticker(kind.into(), symbol.clone(), ticker.clone()));
        Ok(())
    }
    fn emit_trade(&self, trade: &PublicStreamTradeEvent) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Trade(trade.clone()));
        Ok(())
    }
}
fn run<F: Future>(future: F) -> F::Output {
    let mut f = Box::pin(future);
    let w = Waker::noop().clone();
    for _ in 0..32 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(&w)) {
            return v;
        }
    }
    panic!("32-poll host floor exceeded");
}
fn ticker(actual: &Ticker, expected: &Value, label: &str) {
    assert_eq!(
        actual.id,
        expected["id"].as_str().unwrap(),
        "{label} native ticker id"
    );
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "{label} native ticker symbol"
    );
    for (key, value) in [
        ("bid", actual.bid),
        ("ask", actual.ask),
        ("last", actual.last),
        ("mark", actual.mark),
        ("index", actual.index),
        ("percentage", actual.percentage),
        ("openInterest", actual.open_interest),
        ("fundingRate", actual.funding_rate),
        ("fundingTime", actual.funding_time),
        ("volume", actual.volume),
        ("quoteVolume", actual.quote_volume),
    ] {
        assert_eq!(
            value,
            expected[key].as_f64().unwrap(),
            "{label} native ticker {key}"
        );
    }
}
fn state(actual: &PublicSubscriptionState, expected: &Value, label: &str) {
    let topics: Vec<(String, Vec<String>)> =
        serde_json::from_value(expected["topics"].clone()).unwrap();
    assert_eq!(actual.topics, topics, "{label} native topics");
    assert_eq!(
        actual
            .books
            .iter()
            .map(|b| b.0.as_str())
            .collect::<Vec<_>>(),
        expected["books"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        "{label} native books"
    );
    assert_eq!(
        actual.klines,
        serde_json::from_value::<Vec<String>>(expected["klines"].clone()).unwrap(),
        "{label} native klines"
    );
    assert_eq!(
        actual.pending.len(),
        expected["pending"].as_u64().unwrap() as usize,
        "{label} native pending subscriptions"
    );
    assert_eq!(
        actual.ready,
        expected["ready"].as_bool().unwrap(),
        "{label} native socket readiness"
    );
    assert_eq!(
        actual.subscription_id,
        expected["subscriptionId"].as_f64().unwrap(),
        "{label} native subscription ID"
    );
    assert_eq!(
        actual.listener_id,
        expected["listenerId"].as_f64().unwrap(),
        "{label} native listener ID"
    );
}
const NAMES: [(&str, &str, usize); 16] = [
    ("construct-open-close", "assembly", 3),
    ("connect-testnet", "assembly", 1),
    ("connect-dial-failure", "assembly", 1),
    ("connect-disposed", "assembly", 2),
    ("envelope-arrays-single-legacy", "dispatch", 4),
    ("message-numeric-pong", "dispatch", 1),
    ("message-string-pong", "dispatch", 1),
    ("message-guards", "dispatch", 10),
    ("stream-depth-and-kline", "dispatch", 8),
    ("batch-throw-stops-later-items", "dispatch", 1),
    ("reentrant-ticker-removal", "dispatch", 1),
    ("reentrant-mark-removal", "dispatch", 1),
    ("disposed-message", "dispatch", 2),
    ("delegate-cleanup", "delegates", 9),
    ("delegate-missing-socket", "delegates", 3),
    ("delegate-cleanup-current-slot", "delegates", 5),
];
fn cases() -> Vec<Value> {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-stream-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source boundary: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 16, "independent public scenario floor");
    for ((name, family, ops), c) in NAMES.iter().zip(&cases) {
        assert_eq!(c["name"], *name, "independent public scenario name");
        assert_eq!(c["family"], *family, "independent public scenario family");
        assert_eq!(
            c["ops"].as_array().unwrap().len(),
            *ops,
            "independent public operation floor"
        );
        assert_eq!(
            c["snapshots"].as_array().unwrap().len(),
            ops + 1,
            "independent public snapshot floor"
        );
    }
    cases
}
fn compare(family: &str) {
    let mut wire_checks = Vec::new();
    for c in cases().into_iter().filter(|c| c["family"] == family) {
        let name = c["name"].as_str().unwrap();
        let host = Rc::new(host::Runtime::default());
        host.mono.set(1000.0);
        host.wall.set(1000.0);
        let socket = Rc::new(Socket::default());
        let runtime = Rc::new(Runtime {
            host: host.clone(),
            socket: socket.clone(),
            urls: RefCell::new(vec![]),
            fail: if c["dialError"] == true {
                Some("dial down".into())
            } else {
                None
            },
            reconnecting: false,
        });
        let store = Rc::new(DefaultStore::new());
        let first: Ticker = serde_json::from_value(c["snapshots"][0]["ticker"].clone()).unwrap();
        Store::update(
            &*store,
            StoreUpdate {
                tickers: Some(vec![first]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let owner = Rc::new(BaseExchange::new(
            ExchangeName::Asterdex,
            serde_json::from_value(json!({"key":"","secret":"","testnet":c["testnet"]==true}))
                .unwrap(),
            store.clone(),
            runtime.clone(),
            ExchangeSettings::default(),
        ));
        let parent = Rc::new(Parent {
            owner,
            disposed: Cell::new(false),
            events: RefCell::new(vec![]),
            coordinator: Rc::new(ReopenResync::default()),
            reconnected: Cell::new(0),
            close_expected: Cell::new(false),
            close_states: RefCell::new(vec![]),
            stream: RefCell::new(Weak::new()),
        });
        if c["removeTickerOnUpdate"] == true {
            let weak = Rc::downgrade(&store);
            let remove = Cell::new(true);
            let _unsubscribe = Store::subscribe(
                &*store,
                Box::new(move |_, _| {
                    if remove.replace(false) {
                        if let Some(store) = weak.upgrade() {
                            Store::update(
                                &*store,
                                StoreUpdate {
                                    tickers: Some(vec![]),
                                    ..Default::default()
                                },
                                false,
                            )
                            .unwrap();
                        }
                    }
                    Ok(())
                }),
            );
        }
        let s = PublicStream::new(1, parent.clone(), runtime.clone());
        parent.stream.replace(Rc::downgrade(&s));
        let methods: [fn(&PublicStream, &Value) -> Result<(), ExchangeError>; 5] = [
            PublicStream::handle_ticker_update,
            PublicStream::handle_mark_price_update,
            PublicStream::handle_trade_update,
            PublicStream::handle_depth_snapshot,
            PublicStream::handle_kline_update,
        ];
        assert_eq!(
            c["nullErrors"].as_array().unwrap().len(),
            5,
            "independent public null error floor"
        );
        for (i, method) in methods.iter().enumerate() {
            let error =
                method(&s, &Value::Null).expect_err("native public null payload must throw");
            assert_eq!(
                error.name,
                c["nullErrors"][i]["name"].as_str().unwrap(),
                "native public null error class {i}"
            );
            assert_eq!(
                error.message,
                c["nullErrors"][i]["message"].as_str().unwrap(),
                "native public null property {i}"
            );
        }

        let delegates = Rc::new(PublicDataListeners::default());
        delegates.websocket.replace(Some(s.clone()));
        let keys = s
            .socket
            .message_handlers
            .borrow()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            ["24hrTicker", "aggTrade", "markPriceUpdate", "kline"],
            "{name} native constructor handler keys"
        );
        assert_eq!(
            c["handlerKeys"],
            json!(["24hrTicker", "aggTrade", "markPriceUpdate", "kline"]),
            "source independent constructor keys"
        );
        let books = Rc::new(RefCell::new(Vec::<OrderBook>::new()));
        let candles = Rc::new(RefCell::new(Vec::<PublicKlineCandle>::new()));
        let mut book_cleanup: Option<BookCleanup> = None;
        let mut candle_cleanup: Option<BookCleanup> = None;
        let check = |i: usize| {
            let expected = &c["snapshots"][i];
            state(&s.subscriptions.state(), expected, name);
            if expected.get("ticker").is_none() {
                assert!(
                    store.tickers().unwrap().is_empty(),
                    "{name} native removed ticker"
                );
            } else {
                ticker(&store.tickers().unwrap()[0], &expected["ticker"], name);
            }
            assert_eq!(
                parent.events.borrow().len(),
                expected["eventCount"].as_u64().unwrap() as usize,
                "{name} native emitted count"
            );
            assert_eq!(
                books.borrow().len(),
                expected["bookCount"].as_u64().unwrap() as usize,
                "{name} native book callback count"
            );
            assert_eq!(
                candles.borrow().len(),
                expected["candleCount"].as_u64().unwrap() as usize,
                "{name} native candle callback count"
            );
        };
        check(0);
        for (i, op) in c["ops"].as_array().unwrap().iter().enumerate() {
            match op["kind"].as_str().unwrap() {
                "connect" => run(s.connect_and_subscribe()).unwrap(),
                "open" => {
                    socket.events.borrow_mut().push_back(WsEvent::Open);
                }
                "close" => {
                    parent.close_expected.set(true);
                    s.socket.on_close(Some(1000), "").unwrap();
                }
                "parent-disposed" => parent.disposed.set(true),
                "message" => {
                    let text = op["raw"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| op["data"].to_string());
                    s.on_message(&SocketFrame::Text(text)).unwrap();
                }
                "book" => {
                    let books = books.clone();
                    book_cleanup = Some(
                        delegates
                            .listen_order_book(
                                "BTCUSDT",
                                Rc::new(move |book| {
                                    books.borrow_mut().push(book);
                                    Ok(())
                                }),
                            )
                            .unwrap(),
                    );
                }
                "ohlcv" => {
                    let candles = candles.clone();
                    candle_cleanup = Some(
                        delegates
                            .listen_ohlcv(
                                "BTCUSDT",
                                safe_cex_types::AnyTimeframe::Variant1(Timeframe::M1),
                                Rc::new(move |candle| {
                                    candles.borrow_mut().push(candle);
                                    Ok(())
                                }),
                            )
                            .unwrap(),
                    );
                }
                "remove-book" => book_cleanup.as_ref().unwrap()().unwrap(),
                "remove-ohlcv" => candle_cleanup.as_ref().unwrap()().unwrap(),
                "drop-slot" => {
                    delegates.websocket.take();
                }
                "replace-slot" => {
                    delegates.websocket.replace(Some(PublicStream::new(
                        2,
                        parent.clone(),
                        runtime.clone(),
                    )));
                }
                _ => unreachable!(),
            };
            host.pump();
            check(i + 1);
        }
        assert_eq!(
            s.socket.last_message_timestamp().unwrap_or(0.0),
            c["received"].as_f64().unwrap(),
            "{name} native receipt timestamp"
        );
        assert_eq!(
            parent.close_states.borrow().as_slice(),
            c["closes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v[0].as_bool().unwrap(), v[1].as_u64().unwrap() as usize))
                .collect::<Vec<_>>(),
            "{name} native close hook state"
        );
        assert_eq!(
            parent.reconnected.get(),
            c["reconnected"].as_u64().unwrap() as usize,
            "{name} native reconnect emission"
        );
        assert_eq!(
            parent.events.borrow().len(),
            c["events"].as_array().unwrap().len(),
            "{name} native event count"
        );
        for (actual, expected) in parent
            .events
            .borrow()
            .iter()
            .zip(c["events"].as_array().unwrap())
        {
            match actual {
                Event::Ticker(kind, symbol, t) => {
                    assert_eq!(
                        kind,
                        expected["kind"].as_str().unwrap(),
                        "{name} native ticker event"
                    );
                    assert_eq!(symbol, &expected["args"][0], "{name} native event symbol");
                    ticker(t, &expected["args"][1], name);
                }
                Event::Trade(t) => {
                    let e = &expected["args"][0];
                    assert_eq!(expected["kind"], "trade");
                    assert_eq!(t.symbol, e["symbol"].as_str().unwrap());
                    assert_eq!(
                        t.price,
                        e["price"].as_f64().unwrap(),
                        "{name} native trade price"
                    );
                    assert_eq!(t.qty, e["qty"].as_f64().unwrap());
                    assert_eq!(t.timestamp, e["timestamp"].as_f64().unwrap());
                    assert_eq!(t.side, e["side"].as_str().unwrap());
                }
            }
        }
        for (actual, expected) in books.borrow().iter().zip(c["books"].as_array().unwrap()) {
            for (levels, key) in [(&actual.bids, "bids"), (&actual.asks, "asks")] {
                assert_eq!(levels.len(), expected[key].as_array().unwrap().len());
                for (level, e) in levels.iter().zip(expected[key].as_array().unwrap()) {
                    assert_eq!(level.price, e["price"].as_f64().unwrap());
                    assert_eq!(level.amount, e["amount"].as_f64().unwrap());
                    assert_eq!(level.total, e["total"].as_f64().unwrap());
                }
            }
        }
        for (actual, e) in candles
            .borrow()
            .iter()
            .zip(c["candles"].as_array().unwrap())
        {
            assert_eq!(
                actual.timestamp.as_ref().and_then(Value::as_f64),
                e.get("timestamp").and_then(Value::as_f64),
                "{name} native candle timestamp"
            );
            for (key, v) in [
                ("open", actual.open),
                ("high", actual.high),
                ("low", actual.low),
                ("close", actual.close),
                ("volume", actual.volume),
            ] {
                assert_eq!(v, e[key].as_f64().unwrap(), "{name} native candle {key}");
            }
        }
        let pong_expected = if matches!(name, "message-guards" | "message-numeric-pong") {
            1
        } else {
            0
        };
        assert_eq!(c["pongs"], pong_expected, "independent public pong floor");
        assert_eq!(
            s.socket
                .message_handlers_stats
                .borrow()
                .contains_key("pong"),
            pong_expected == 1,
            "{name} native numeric pong guard"
        );
        let actual_frames: Vec<Value> = socket
            .wire
            .borrow()
            .iter()
            .map(|text| serde_json::from_str(text).unwrap())
            .collect();
        let expected_frames: Vec<Value> = c["wire"]
            .as_array()
            .unwrap()
            .iter()
            .map(|text| serde_json::from_str(text.as_str().unwrap()).unwrap())
            .collect();
        assert_eq!(
            actual_frames, expected_frames,
            "{name} decoded native subscription frames"
        );
        for (a, e) in socket
            .wire
            .borrow()
            .iter()
            .zip(c["wire"].as_array().unwrap())
        {
            wire_checks.push((
                a.clone(),
                e.as_str().unwrap().to_owned(),
                format!("{name} subscription wire"),
            ));
        }
        let urls = runtime.urls.borrow();
        assert_eq!(
            urls.len(),
            c["urls"].as_array().unwrap().len(),
            "{name} native dial count"
        );
        for (a, e) in urls.iter().zip(c["urls"].as_array().unwrap()) {
            let (endpoint, query) = a.split_once('?').expect("public request id");
            let (expected_endpoint, expected_query) = e.as_str().unwrap().split_once('?').unwrap();
            assert_eq!(endpoint, expected_endpoint, "{name} native endpoint");
            let (key, value) = query.split_once('=').unwrap();
            assert_eq!(key, "requestId", "{name} native request-id name");
            assert_eq!(value, "clock", "{name} native request-id value");
            assert_eq!(
                expected_query, "requestId=clock",
                "independent source request-id expectation"
            );
            wire_checks.push((
                a.clone(),
                e.as_str().unwrap().to_owned(),
                format!("{name} URL wire"),
            ));
        }
        s.dispose();
        if let Some(replacement) = delegates.websocket.borrow().as_ref() {
            replacement.dispose();
        }
    }
    for (a, e, label) in wire_checks {
        assert_eq!(a, e, "{label}");
    }
}
#[test]
#[ignore = "live unmodified TypeScript public assembly"]
fn public_assembly_matches_source() {
    compare("assembly");
}
#[test]
#[ignore = "live unmodified TypeScript public assembly"]
fn public_dispatch_matches_source() {
    compare("dispatch");
}
#[test]
#[ignore = "live unmodified TypeScript exchange delegates"]
fn public_delegates_match_source() {
    compare("delegates");
}
