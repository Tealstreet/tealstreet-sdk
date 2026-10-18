use super::*;
use safe_cex_venue_apexomni::{
    socket_host::{ApexSocketHost, SocketListener, SocketTask},
    ws_private::ApexOmniPrivateWebsocket,
    ws_public::ApexOmniPublicWebsocket,
};
use std::{cell::Cell, collections::BTreeMap};
fn ws_corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/websockets.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "WebSocket source {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
struct SocketHost {
    runtime: Rc<Host>,
    raw_frames: RefCell<Vec<String>>,
    observe_raw_send: Cell<bool>,
    virtual_calls: Cell<usize>,
    date_calls: Cell<usize>,
    date_offset: Cell<f64>,
    disposed: Cell<bool>,
    connected: Cell<bool>,
    next_timer: Cell<u64>,
    timers: RefCell<BTreeMap<u64, SocketTask>>,
    ping_timer: Cell<Option<u64>>,
    ping_socket: RefCell<Option<std::rc::Weak<ApexOmniPrivateWebsocket>>>,
    private: bool,
    variant: u64,
    name: String,
    open: RefCell<Option<SocketTask>>,
    message: RefCell<Option<Rc<dyn Fn(&str) -> Result<(), ExchangeError>>>>,
}
impl SocketHost {
    fn new(runtime: Rc<Host>, private: bool, variant: u64, name: &str) -> Rc<Self> {
        Rc::new(Self {
            runtime,
            raw_frames: RefCell::new(Vec::new()),
            observe_raw_send: Cell::new(false),
            virtual_calls: Cell::new(0),
            date_calls: Cell::new(0),
            date_offset: Cell::new(0.),
            disposed: Cell::new(false),
            connected: Cell::new(true),
            next_timer: Cell::new(1),
            timers: RefCell::new(BTreeMap::new()),
            ping_timer: Cell::new(None),
            ping_socket: RefCell::new(None),
            private,
            variant,
            name: name.into(),
            open: RefCell::new(None),
            message: RefCell::new(None),
        })
    }
    fn fire(&self) {
        let timer = self.timers.borrow().keys().next().copied();
        if let Some(timer) = timer {
            let task = self.timers.borrow_mut().remove(&timer).unwrap();
            futures::executor::block_on(task()).unwrap();
        }
    }
    fn fire_listeners(&self) {
        if let Some(callback) = self.open.borrow().clone() {
            futures::executor::block_on(callback()).unwrap();
            self.message.borrow().as_ref().unwrap()("{\"op\":\"ping\"}").unwrap();
        }
    }
}
impl ApexSocketHost for SocketHost {
    fn initialize(&self, private: bool, any: bool) -> Result<(), ExchangeError> {
        assert_eq!(private, self.private, "socket kind");
        self.runtime
            .record("initialize", vec![J::Bool(private), J::Bool(any)]);
        Ok(())
    }
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_connected(&self) -> bool {
        self.connected.get()
    }
    fn now_ms(&self) -> f64 {
        self.virtual_calls.set(self.virtual_calls.get() + 1);
        1700001234567.
    }
    fn date_now_ms(&self) -> f64 {
        self.date_calls.set(self.date_calls.get() + 1);
        1700001234567. + self.date_offset.get()
    }
    fn monotonic_now_ms(&self) -> f64 {
        222.25
    }
    fn nanoid(&self) -> String {
        self.runtime.record("nanoid", vec![]);
        "request-id".into()
    }
    fn enable_reconnecting(&self) -> bool {
        self.variant == 1
    }
    fn private_route(
        &self,
        name: &ExchangeName,
        options: &ExchangeOptions,
    ) -> Result<Option<String>, ExchangeError> {
        self.runtime.record(
            "route",
            vec![
                J::from(serde_json::to_value(name).unwrap()),
                obj(vec![
                    ("key", s(&options.key)),
                    ("secret", s(&options.secret)),
                    (
                        "passphrase",
                        options.passphrase.clone().map_or(J::Undefined, s),
                    ),
                    ("testnet", J::Bool(options.testnet == Some(true))),
                    (
                        "publicDataOnly",
                        J::Bool(options.public_data_only == Some(true)),
                    ),
                ]),
            ],
        );
        Ok((self.variant == 1).then(|| "wss://tunnel.example/private".into()))
    }
    fn create_socket(&self, url: &str) -> Result<(), ExchangeError> {
        self.runtime.record("dial", vec![s(url)]);
        Ok(())
    }
    fn add_listener(&self, listener: SocketListener) -> Result<(), ExchangeError> {
        let event = match listener {
            SocketListener::Open(callback) => {
                *self.open.borrow_mut() = Some(callback);
                "open"
            }
            SocketListener::Message(callback) => {
                *self.message.borrow_mut() = Some(callback);
                "message"
            }
            SocketListener::Close => "close",
        };
        self.runtime.record("listener", vec![s(event)]);
        Ok(())
    }
    fn send(&self, frame: &str, logged: bool) -> Result<(), ExchangeError> {
        self.raw_frames.borrow_mut().push(frame.to_owned());
        if self.observe_raw_send.get() {
            return Ok(());
        }
        let mut value = safe_cex_transport::how::json_parse(frame)
            .into_value()
            .unwrap();
        if value["op"] == "login" {
            let args = value["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|arg| {
                    safe_cex_transport::how::json_parse(arg.as_str().unwrap())
                        .into_value()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            value["args"] = json!(args);
        }
        self.runtime
            .record("send", vec![J::from(value), J::Bool(logged)]);
        Ok(())
    }
    fn set_ping_at(&self, at: f64) -> Result<(), ExchangeError> {
        self.runtime.record("pingAt", vec![n(at)]);
        Ok(())
    }
    fn schedule_ping(&self, delay_ms: u64) -> Result<(), ExchangeError> {
        if let Some(timer) = self.ping_timer.get() {
            self.clear_timeout(timer)?;
        }
        let weak = self
            .ping_socket
            .borrow()
            .clone()
            .expect("private ping receiver");
        let timer = self.set_timeout(
            delay_ms,
            Rc::new(move || {
                let socket = weak.upgrade().expect("live private socket");
                Box::pin(async move { socket.ping() })
            }),
        )?;
        self.ping_timer.set(Some(timer));
        Ok(())
    }
    fn track_last_received(&self) -> Result<(), ExchangeError> {
        self.runtime.record("received", vec![]);
        Ok(())
    }
    fn diagnostic_receive(&self, frame: &str) -> Result<(), ExchangeError> {
        self.runtime.record("diagnostic", vec![s(frame)]);
        Ok(())
    }
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.runtime.record("stats", vec![s(topic)]);
        Ok(())
    }
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.runtime.record("cleanup", vec![s(topic)]);
        Ok(())
    }
    fn handle_pong(&self, next: Option<u64>) -> Result<(), ExchangeError> {
        self.runtime.record(
            "pong",
            vec![next.map_or(J::Undefined, |n| J::Number(n as f64))],
        );
        Ok(())
    }
    fn emit_reconnected(&self) -> Result<(), ExchangeError> {
        self.runtime.record("reconnected", vec![]);
        Ok(())
    }
    fn reconnect(&self) -> Result<(), ExchangeError> {
        self.runtime.record("reconnect", vec![]);
        Ok(())
    }
    fn report_error(&self, error: ExchangeError) -> Result<(), ExchangeError> {
        self.runtime.record("error", vec![s(error.message)]);
        Ok(())
    }
    fn warn(&self, message: &str, arg: &J) -> Result<(), ExchangeError> {
        self.runtime.record("warn", vec![s(message), arg.clone()]);
        Ok(())
    }
    fn set_timeout(&self, delay: u64, task: SocketTask) -> Result<u64, ExchangeError> {
        let id = self.next_timer.get();
        self.next_timer.set(id + 1);
        self.runtime
            .record("timeout", vec![n(delay as f64), n(id as f64)]);
        self.timers.borrow_mut().insert(id, task);
        Ok(id)
    }
    fn clear_timeout(&self, id: u64) -> Result<(), ExchangeError> {
        self.runtime.record("clear", vec![n(id as f64)]);
        self.timers.borrow_mut().remove(&id);
        Ok(())
    }
    fn spawn(&self, _future: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError> {
        self.runtime.record("spawn", vec![]);
        Ok(())
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.runtime.record("base:dispose", vec![]);
        self.disposed.set(true);
        if let Some(timer) = self.ping_timer.take() {
            self.clear_timeout(timer)?;
        }
        Ok(())
    }
    fn remote_signer_account(&self) -> Option<J> {
        (self.private && ["auth", "open"].contains(&self.name.as_str()) && self.variant > 0)
            .then(|| s("remote-account"))
    }
    fn remote_sign(&self, request: J) -> ExchangeFuture<'_, J> {
        self.runtime.record("remoteSign", vec![request]);
        Box::pin(async move {
            Ok(if self.variant == 2 {
                obj(vec![("error", s("browser offline"))])
            } else {
                obj(vec![("wsAuthSignature", s("remote-signature"))])
            })
        })
    }
}
fn candle_value(c: Candle) -> J {
    obj(vec![
        ("timestamp", n(c.timestamp)),
        ("open", n(c.open)),
        ("high", n(c.high)),
        ("low", n(c.low)),
        ("close", n(c.close)),
        ("volume", n(c.volume)),
    ])
}
fn book_value(book: OrderBook) -> J {
    let levels = |rows: Vec<OrderBookOrders>| {
        J::Array(
            rows.into_iter()
                .map(|r| {
                    obj(vec![
                        ("price", n(r.price)),
                        ("amount", n(r.amount)),
                        ("total", n(r.total)),
                    ])
                })
                .collect(),
        )
    };
    obj(vec![
        ("bids", levels(book.bids)),
        ("asks", levels(book.asks)),
    ])
}
fn frame(value: Value) -> String {
    serde_json::to_string(&value).unwrap()
}
fn run(privacy: &str, name: &str) {
    let rows = ws_corpus()["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 72, "WebSocket source floor");
    let mut compared = 0;
    for row in rows
        .iter()
        .filter(|r| r["privacy"] == privacy && r["name"] == name)
    {
        let variant = row["variant"].as_u64().unwrap();
        let private = privacy == "private";
        let runtime = Rc::new(Host::default());
        let store = fixture(&runtime);
        if ["positions", "accounts"].contains(&name) {
            let mut p = store.positions().unwrap()[0].clone();
            p.id = "BTC-USDC:short".into();
            p.side = safe_cex_types::ModelField::Value(PositionSide::Short);
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    positions: Some(vec![p]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        *runtime.responses.borrow_mut() = responses();
        let mut options:ExchangeOptions=serde_json::from_value(json!({"key":"key","secret":"secret-é","passphrase":"pass","testnet":variant==1,"publicDataOnly":name=="candle"&&variant==1})).unwrap();
        if variant == 2 {
            options.passphrase = None;
        }
        let parent = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                options,
                store.clone(),
                runtime.clone(),
                Default::default(),
            ),
            runtime.clone(),
        )
        .unwrap();
        let emitter = runtime.clone();
        parent.base.on(
            "fill",
            Rc::new(move |_, data| {
                let data = safe_cex_exchange::js_runtime::value::to_json(data).unwrap();
                emitter.record("emit", vec![s("fill"), J::from(data.clone())]);
                Ok(())
            }),
        );
        let host = SocketHost::new(runtime.clone(), private, variant, name);
        runtime.operations.borrow_mut().clear();
        let input = native::input(&row["input"]);
        let initial_positions = store.positions().unwrap().to_vec();
        let mut result = J::Undefined;
        if private {
            let socket = ApexOmniPrivateWebsocket::new(parent.clone(), host.clone()).unwrap();
            *host.ping_socket.borrow_mut() = Some(Rc::downgrade(&socket));
            if name != "class" {
                runtime.operations.borrow_mut().clear();
            }
            if name == "orders" || name == "accounts" {
                socket
                    .last_connect_time
                    .set(1700001234567. - if variant == 2 { 1000. } else { 6000. });
            }
            if name == "orders" && variant == 2 {
                parent
                    .recently_cancelled_orders
                    .borrow_mut()
                    .insert("order-1".into(), 1700001234567.);
            }
            match name {
                "class" | "subscribe" => socket.subscribe().unwrap(),
                "connect" => {
                    socket.connect_and_subscribe().unwrap();
                    host.fire_listeners();
                }
                "open" => futures::executor::block_on(socket.on_open()).unwrap(),
                "ping" => {
                    socket.ping().unwrap();
                    if variant == 1 {
                        socket.ping().unwrap();
                    }
                }
                "message" => socket
                    .on_message(match &input {
                        J::String(raw) => raw,
                        _ => panic!(),
                    })
                    .unwrap(),
                "auth" => result = J::Bool(futures::executor::block_on(socket.auth()).unwrap()),
                "fills" => socket.handle_fill_topic(&native::capture(&input)).unwrap(),
                "orders" => {
                    socket.handle_order_topic(&native::capture(&input)).unwrap();
                    if variant == 0 {
                        socket.handle_order_topic(&native::capture(&input)).unwrap();
                    }
                }
                "positions" => {
                    if let Err(error) = socket.handle_position_topic(&native::capture(&input)) {
                        result = obj(vec![
                            ("errorName", s(error.name)),
                            ("errorMessage", s(error.message)),
                        ]);
                    }
                }
                "accounts" => socket
                    .handle_ws_accounts_v2_topic(&native::capture(&input))
                    .unwrap(),
                "ordersThrottle" | "positionsThrottle" => {
                    if variant == 1 {
                        runtime.request_errors.borrow_mut().insert(
                            if name == "ordersThrottle" {
                                "/v3/open-orders"
                            } else {
                                "/v3/account"
                            },
                            ExchangeError::new("reconcile unavailable"),
                        );
                    }
                    if variant == 2 {
                        let host = host.clone();
                        *runtime.request_hook.borrow_mut() =
                            Some(Rc::new(move || host.disposed.set(true)));
                    }
                    if name == "ordersThrottle" {
                        socket.throttle_fetch_orders().unwrap();
                        socket.throttle_fetch_orders().unwrap();
                    } else {
                        socket.throttle_fetch_positions().unwrap();
                        socket.throttle_fetch_positions().unwrap();
                    }
                    host.fire();
                }
                "dispose" => {
                    socket.throttle_fetch_orders().unwrap();
                    socket.throttle_fetch_positions().unwrap();
                    socket.ping().unwrap();
                    socket.dispose().unwrap();
                }
                _ => panic!("unknown private body"),
            }
            assert_eq!(
                socket.last_connect_time.get().to_bits(),
                row["lastConnect"].as_f64().unwrap().to_bits(),
                "ws private {name} native lastConnect"
            );
        } else {
            let socket = ApexOmniPublicWebsocket::new(parent.clone(), host.clone()).unwrap();
            if name != "class" && name != "constructor" {
                runtime.operations.borrow_mut().clear();
            }
            if ["connect", "open", "ping"].contains(&name) && variant == 2 {
                host.disposed.set(true);
            }
            match name {
                "class" | "constructor" => {
                    socket.subscribe().unwrap();
                    socket.on_message("pong").unwrap();
                }
                "connect" => {
                    socket.connect_and_subscribe(false).unwrap();
                    host.fire_listeners();
                }
                "open" => socket.on_open().unwrap(),
                "ping" => {
                    socket.ping().unwrap();
                    if variant == 1 {
                        socket.ping().unwrap();
                    }
                }
                "subscribe" => {
                    if variant == 1 {
                        for i in 0..51 {
                            let _cleanup = socket
                                .listen_order_book(&format!("BOOK{i}"), Rc::new(|_| Ok(())))
                                .unwrap();
                        }
                        runtime.operations.borrow_mut().clear();
                    }
                    socket.subscribe().unwrap();
                }
                "message" => socket
                    .on_message(match &input {
                        J::String(raw) => raw,
                        _ => panic!(),
                    })
                    .unwrap(),
                "instrument" => socket.handle_instrument_info_event(&input).unwrap(),
                "candle" => {
                    host.connected.set(variant != 2);
                    let out = runtime.clone();
                    let cleanup = socket
                        .listen_ohlcv(
                            OHLCVOptions {
                                symbol: "BTCUSDC".into(),
                                interval: safe_cex_types::AnyTimeframe::Variant1(Timeframe::M1),
                                from: None,
                                to: None,
                                limit: None,
                            },
                            Rc::new(move |c| {
                                out.record("candle", vec![candle_value(c)]);
                                Ok(())
                            }),
                        )
                        .unwrap();
                    if variant == 2 {
                        cleanup().unwrap();
                        host.connected.set(true);
                        host.fire();
                    }
                    let t = (1700001234567.0_f64 / 60000.).floor() * 60000.;
                    socket.on_message(&frame(json!({"topic":"candle.1.BTCUSDC","data":[{"start":t,"open":"10.25","high":"12.25","low":"9.25","close":"11.25","volume":"7.25"}]}))).unwrap();
                    for (time, p, v) in [
                        (t + 1000., 20, 200),
                        (t - 1000., 21, 210),
                        (t + 61000., 22, 220),
                    ] {
                        socket.on_message(&frame(json!({"topic":"recentlyTrade.H.BTCUSDC","data":[{"T":time,"p":p,"v":v}]}))).unwrap();
                    }
                    socket.on_message(&frame(json!({"topic":"recentlyTrade.H.BTCUSDC","data":[{"T":t+62000.,"p":3,"v":"1.12345678900"}]}))).unwrap();
                    cleanup().unwrap();
                    socket.subscribe().unwrap();
                }
                "book" => {
                    host.connected.set(variant != 2);
                    let out = runtime.clone();
                    let cleanup = socket
                        .listen_order_book(
                            "BTCUSDC",
                            Rc::new(move |b| {
                                out.record("book", vec![book_value(b)]);
                                Ok(())
                            }),
                        )
                        .unwrap();
                    if variant == 2 {
                        cleanup().unwrap();
                        host.connected.set(true);
                        host.fire();
                    }
                    socket.on_message(&frame(json!({"topic":"orderBook200.H.BTCUSDC","type":"snapshot","data":{"a":[["102.25","2.5"],["103.25","3.5"]],"b":[["101.25","4.5"],["100.25","5.5"]]}}))).unwrap();
                    socket.on_message(&frame(json!({"topic":"orderBook200.H.BTCUSDC","type":"delta","data":{"a":[["102.25","0"],["104.25","6.5"]],"b":[["101.25","7.5"]]}}))).unwrap();
                    cleanup().unwrap();
                    socket.subscribe().unwrap();
                }
                _ => panic!("unknown public body"),
            }
        }
        let label = format!("ws {privacy} {name} native");
        native::check_fields(&result, &row["result"], &label);
        native::check_fields(
            &J::Array(runtime.operations.borrow().clone()),
            &row["operations"],
            &label,
        );
        let state = obj(vec![
            (
                "tickers",
                J::Array(store.tickers().unwrap().iter().map(ticker_value).collect()),
            ),
            (
                "positions",
                J::Array(store.positions().unwrap().iter().map(position).collect()),
            ),
            (
                "orders",
                J::Array(store.orders().unwrap().iter().map(order_value).collect()),
            ),
            (
                "executions",
                J::Array(
                    parent
                        .recent_ws_executions
                        .borrow()
                        .iter()
                        .map(execution)
                        .collect(),
                ),
            ),
        ]);
        if private && name == "positionsThrottle" && variant == 2 {
            // P4 explicitly requires a second disposal check after awaiting.
            // Retain the live source observation of late publication; compare
            // every other state axis, and assert the chosen native axis directly.
            assert_eq!(
                store.positions().unwrap().to_vec(),
                initial_positions,
                "disposed timer retains the prior positions"
            );
            let source_positions = row["state"]["values"]
                .as_array()
                .unwrap()
                .iter()
                .find(|field| field[0] == "positions")
                .unwrap();
            assert_eq!(
                source_positions[1]["values"].as_array().unwrap().len(),
                4,
                "source timer still publishes four late positions"
            );
            let J::Object(mut native_fields) = state else {
                panic!()
            };
            native_fields.retain(|(name, _)| name != "positions");
            let mut expected = row["state"].clone();
            expected["values"]
                .as_array_mut()
                .unwrap()
                .retain(|field| field[0] != "positions");
            native::check_fields(&J::Object(native_fields), &expected, &label);
        } else {
            native::check_fields(&state, &row["state"], &label);
        }
        assert_eq!(
            host.timers.borrow().len(),
            row["timers"].as_u64().unwrap() as usize,
            "{label} timer count"
        );
        assert_eq!(
            parent.last_websocket_positions_update.get(),
            row["lastPositions"].as_f64(),
            "{label} lastPositions"
        );
        compared += 1;
    }
    assert_eq!(compared, 3, "{privacy} {name} comparison floor");
}
fn ticker_value(t: &Ticker) -> J {
    obj(vec![
        ("id", s(&t.id)),
        ("symbol", s(&t.symbol)),
        ("bid", n(t.bid)),
        ("ask", n(t.ask)),
        ("last", n(t.last)),
        ("mark", n(t.mark)),
        ("index", n(t.index)),
        ("percentage", n(t.percentage)),
        ("volume", n(t.volume)),
        ("quoteVolume", n(t.quote_volume)),
        ("openInterest", n(t.open_interest)),
        ("fundingRate", n(t.funding_rate)),
        ("fundingTime", n(t.funding_time)),
    ])
}
fn order_value(o: &Order) -> J {
    super::order(o)
}
#[test]
fn ws_public_candle_string_boundary() {
    let J::Array(source) = native::input(&ws_corpus()["chosenCandle"]) else {
        panic!()
    };
    assert_eq!(source.len(), 2, "source accepts string close");
    let J::Object(fields) = &source[1] else {
        panic!()
    };
    assert_eq!(
        fields.iter().find(|(key, _)| key == "close").unwrap().1,
        s("20.25"),
        "source keeps string close"
    );
    let runtime = Rc::new(Host::default());
    let store = fixture(&runtime);
    let options: ExchangeOptions =
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap();
    let parent = ApexOmniExchange::new(
        BaseExchange::new(
            ExchangeName::Apexomni,
            options,
            store,
            runtime.clone(),
            Default::default(),
        ),
        runtime.clone(),
    )
    .unwrap();
    let host = SocketHost::new(runtime.clone(), false, 0, "candle");
    let socket = ApexOmniPublicWebsocket::new(parent, host).unwrap();
    let out = runtime.clone();
    let _cleanup = socket
        .listen_ohlcv(
            OHLCVOptions {
                symbol: "BTCUSDC".into(),
                interval: safe_cex_types::AnyTimeframe::Variant1(Timeframe::M1),
                from: None,
                to: None,
                limit: None,
            },
            Rc::new(move |c| {
                out.record("candle", vec![candle_value(c)]);
                Ok(())
            }),
        )
        .unwrap();
    let t = (1700001234567.0_f64 / 60000.).floor() * 60000.;
    socket.on_message(&frame(json!({"topic":"candle.1.BTCUSDC","data":[{"start":t,"open":"10.25","high":"12.25","low":"9.25","close":"11.25","volume":"7.25"}]}))).unwrap();
    let error=socket.on_message(&frame(json!({"topic":"recentlyTrade.H.BTCUSDC","data":[{"T":t+1000.,"p":"20.25","v":"202.5"}]}))).unwrap_err();
    assert_eq!(
        error.message, "typed Candle.close refuses a non-number trade price",
        "CHOSEN boundary refuses raw string close visibly"
    );
    let callbacks = runtime
        .operations
        .borrow()
        .iter()
        .filter(|op| matches!(op,J::Array(values)if values.first()==Some(&s("candle"))))
        .count();
    assert_eq!(callbacks, 1, "no fabricated numeric close callback");
}
macro_rules! body {
    ($test:ident,$privacy:literal,$name:literal) => {
        #[test]
        fn $test() {
            run($privacy, $name);
        }
    };
}
body!(ws_public_class, "public", "class");
body!(ws_public_constructor, "public", "constructor");
body!(ws_public_connect, "public", "connect");
body!(ws_public_open, "public", "open");
body!(ws_public_ping, "public", "ping");
body!(ws_public_subscribe, "public", "subscribe");
body!(ws_public_message, "public", "message");
body!(ws_public_instrument, "public", "instrument");
body!(ws_public_candle, "public", "candle");
body!(ws_public_book, "public", "book");
body!(ws_private_class, "private", "class");
body!(ws_private_orders_throttle, "private", "ordersThrottle");
body!(
    ws_private_positions_throttle,
    "private",
    "positionsThrottle"
);
body!(ws_private_connect, "private", "connect");
body!(ws_private_open, "private", "open");
body!(ws_private_message, "private", "message");
body!(ws_private_ping, "private", "ping");
body!(ws_private_accounts, "private", "accounts");
body!(ws_private_fills, "private", "fills");
body!(ws_private_orders, "private", "orders");
body!(ws_private_positions, "private", "positions");
body!(ws_private_auth, "private", "auth");
body!(ws_private_subscribe, "private", "subscribe");
body!(ws_private_dispose, "private", "dispose");

#[test]
fn canceled_prefix_effects_precede_later_order_faults() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/socket-order-boundaries.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "APEX-WS-ORDER source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    let expected = [
        ("canceled-null", 1, 1, 1, 0, 0),
        ("canceled-open-null", 2, 2, 2, 0, 1),
        ("open-null", 0, 0, 0, 0, 3),
        ("null-canceled", 0, 0, 0, 0, 3),
        ("empty", 0, 0, 0, 1, 3),
        ("open", 1, 1, 1, 1, 3),
        ("cached-string-null", 0, 0, 0, 0, 3),
        ("cached-numeric", 1, 0, 0, 0, 3),
        ("null-root", 0, 0, 0, 0, 3),
        ("object-root", 0, 0, 0, 0, 3),
        ("number-root", 0, 0, 0, 0, 3),
        ("undefined-root", 0, 0, 0, 0, 3),
        ("cancel-stale", 1, 1, 1, 0, 0),
        ("cancel-young-connect", 1, 1, 1, 0, 0),
    ];
    assert_eq!(rows.len(), 14, "APEX-WS-ORDER independent source floor");
    let mut errors = 0;
    let mut positives = 0;
    let mut refusals = 0;
    for (row, (label, parent_clocks, socket_clocks, dates, timers, order_count)) in
        rows.iter().zip(expected)
    {
        assert_eq!(row["label"], label, "APEX-WS-ORDER authored source order");
        let marker = format!("APEX-WS-ORDER {label}");
        let runtime = Rc::new(Host::default());
        let store = fixture(&runtime);
        let parent = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
                store.clone(),
                runtime.clone(),
                Default::default(),
            ),
            runtime.clone(),
        )
        .unwrap();
        store
            .add_or_update_orders_full(
                parent
                    .map_order(&native::capture(&J::from(
                        corpus()["fixtures"]["order"].clone(),
                    )))
                    .unwrap(),
            )
            .unwrap();
        if label == "cached-string-null" {
            parent
                .recently_cancelled_orders
                .borrow_mut()
                .insert("order-1".into(), 1700001234567.);
        }
        if label == "cached-numeric" {
            parent
                .recently_cancelled_orders
                .borrow_mut()
                .insert("17".into(), 1700001234567.);
        }
        let host = SocketHost::new(runtime.clone(), true, 0, "order-boundaries");
        host.date_offset.set(row["dateOffset"].as_f64().unwrap());
        let socket = ApexOmniPrivateWebsocket::new(parent.clone(), host.clone()).unwrap();
        *host.ping_socket.borrow_mut() = Some(Rc::downgrade(&socket));
        socket
            .last_connect_time
            .set(row["lastConnect"].as_f64().unwrap());
        runtime.operations.borrow_mut().clear();
        runtime.now_calls.set(0);
        let actual = socket.handle_order_topic(&native::capture(&native::input(&row["input"])));
        if label == "cached-numeric" {
            let error = actual.expect_err(&format!("{marker} native whole mapped-order refusal"));
            assert_eq!(error.name, "Error", "{marker} native refusal kind");
            assert_eq!(
                error.message, "shared typed record refuses a non-string field",
                "{marker} native refusal reason"
            );
            assert_eq!(
                row["result"]["ok"], true,
                "{marker} source retains numeric ID through terminal handling"
            );
            refusals += 1;
        } else if row["result"]["ok"] == false {
            let error = actual.expect_err(&format!("{marker} native expected error"));
            assert_eq!(
                error.name,
                row["result"]["name"].as_str().unwrap(),
                "{marker} native error kind"
            );
            assert_eq!(
                error.message,
                row["result"]["message"].as_str().unwrap(),
                "{marker} native error precedence"
            );
            errors += 1;
        } else {
            actual.unwrap_or_else(|error| panic!("{marker} native unexpected error: {error:?}"));
            positives += 1;
        }
        assert_eq!(
            runtime.now_calls.get(),
            0,
            "{marker} parent virtual clock must not read the raw runtime"
        );
        assert_eq!(
            host.virtual_calls.get(),
            socket_clocks,
            "{marker} native socket virtual clocks"
        );
        assert_eq!(host.date_calls.get(), dates, "{marker} native Date clocks");
        assert_eq!(
            row["virtual"],
            if label == "cached-numeric" {
                2
            } else {
                parent_clocks + socket_clocks
            },
            "{marker} source virtual clocks"
        );
        assert_eq!(
            row["dates"],
            if label == "cached-numeric" { 1 } else { dates },
            "{marker} source Date clocks"
        );
        assert_eq!(
            host.timers.borrow().len(),
            timers,
            "{marker} native pending timers"
        );
        assert_eq!(row["timers"], timers, "{marker} source pending timers");
        assert_eq!(
            store.orders().unwrap().len(),
            order_count,
            "{marker} native Store order floor"
        );
        native::check_fields(
            &J::Array(store.orders().unwrap().iter().map(super::order).collect()),
            &row["orders"],
            &format!("{marker} native Store orders"),
        );
        let cache: Vec<_> = parent
            .recently_cancelled_orders
            .borrow()
            .iter()
            .map(|(id, time)| J::Array(vec![s(id), n(*time)]))
            .collect();
        assert_eq!(
            cache.len(),
            if label.starts_with("cached-") { 1 } else { 0 },
            "{marker} independent native cache floor"
        );
        native::check(
            &J::Array(cache),
            &row["cache"],
            &format!("{marker} native cancellation cache"),
        );
        native::check(
            &J::Array(runtime.operations.borrow().clone()),
            &row["operations"],
            &format!("{marker} native operations"),
        );
    }
    assert_eq!(errors, 9, "APEX-WS-ORDER independent matching-error floor");
    assert_eq!(positives, 4, "APEX-WS-ORDER independent positive floor");
    assert_eq!(
        refusals, 1,
        "APEX-WS-ORDER independent priced-refusal floor"
    );
}

fn frame_units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
        .collect()
}

#[test]
fn socket_frame_units_are_observed_before_wire() {
    use safe_cex_num::numeric_ingress::js_from_str;
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    fn property<'a>(value: &'a Input, name: &str) -> Option<&'a Input> {
        let Input::Object(entries) = value else {
            return None;
        };
        entries
            .iter()
            .find(|(key, _)| key.iter().copied().eq(name.encode_utf16()))
            .map(|(_, value)| value)
    }
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/socket-frame-units.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "APEX-WS-FRAME source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    let expected: [(&str, &[u16]); 8] = [
        ("high", &[0xd800]),
        ("low", &[0xdc00]),
        ("high-tail", &[0xd800, 65]),
        ("pair", &[0xd83d, 0xde00]),
        ("scalar", &[233]),
        ("replacement", &[0xfffd]),
        ("literal", &[92, 117, 100, 56, 48, 48]),
        ("unused-extra", &[65, 83, 67, 73, 73]),
    ];
    assert_eq!(rows.len(), 16, "APEX-WS-FRAME independent source floor");
    let mut native_positives = 0;
    let mut native_refusals = 0;
    let mut wire_observations = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let private = index < 8;
        let (label, units) = expected[index % 8];
        let record_refused = index % 8 < 3;
        let view_refused = record_refused || label == "unused-extra";
        let refused = !private && view_refused;
        let privacy = if private { "private" } else { "public" };
        let marker = format!("APEX-WS-FRAME {privacy}/{label}");
        assert_eq!(row["privacy"], privacy, "{marker} authored privacy");
        assert_eq!(row["label"], label, "{marker} authored label");
        assert_eq!(row["result"]["ok"], true, "{marker} source accepted frame");
        assert_eq!(
            frame_units(&row["inputUnits"]),
            units,
            "{marker} source input units"
        );
        let raw = row["raw"].as_str().unwrap();
        let decoded = safe_cex_transport::how::json_parse(raw);
        assert!(
            !matches!(
                decoded,
                safe_cex_types::retained_json::RetainedJsonInput::Null
            ),
            "{marker} native parser retains the complete source frame"
        );
        // Observe retained units before the explicitly fallible UTF-8 frame view.
        let values = property(&decoded, if private { "args" } else { "data" }).unwrap();
        let Input::Array(values) = values else {
            panic!("{marker} native frame array");
        };
        let value = if private {
            &values[0]
        } else {
            property(&values[0], "s").unwrap()
        };
        assert!(
            matches!(value, Input::Text(actual) if actual == units),
            "{marker} retained native input units"
        );
        if label == "unused-extra" {
            assert!(
                matches!(property(&decoded, "ignored"), Some(Input::Text(actual)) if actual == &[0xd800]),
                "{marker} unrelated native lone unit retained"
            );
        }
        let decoded = decoded.into_value();
        if view_refused {
            assert!(
                raw.is_ascii(),
                "{marker} refused source frame is valid ASCII transport"
            );
            assert!(
                decoded.is_err(),
                "{marker} explicit typed frame view refuses whole frame"
            );
            native_refusals += 1;
        } else {
            let decoded = decoded.unwrap();
            let value = if private {
                &decoded["args"][0]
            } else {
                &decoded["data"][0]["s"]
            };
            assert_eq!(
                value.as_str().unwrap().encode_utf16().collect::<Vec<_>>(),
                units,
                "{marker} decoded native input units"
            );
            native_positives += 1;
        }
        let runtime = Rc::new(Host::default());
        let store = fixture(&runtime);
        if !private {
            let ticker = js_from_str::<Ticker>(row["rawTicker"].as_str().unwrap());
            if record_refused {
                let ticker = ticker.unwrap();
                assert_eq!(
                    ticker.id,
                    String::from_utf16_lossy(units),
                    "{marker} typed ticker id"
                );
                assert_eq!(ticker.last.to_bits(), 103f64.to_bits());
                typed_utf16::assert_projection(row["rawTicker"].as_str().unwrap(), &ticker);
            } else {
                let ticker = ticker.unwrap();
                assert_eq!(
                    ticker.id.encode_utf16().collect::<Vec<_>>(),
                    units,
                    "{marker} decoded native ticker id units"
                );
                assert_eq!(
                    ticker.last.to_bits(),
                    103f64.to_bits(),
                    "{marker} decoded native ticker last"
                );
                Store::update(
                    store.as_ref(),
                    StoreUpdate {
                        tickers: Some(vec![ticker]),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            }
        }
        let parent = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
                store.clone(),
                runtime.clone(),
                Default::default(),
            ),
            runtime.clone(),
        )
        .unwrap();
        let host = SocketHost::new(runtime.clone(), private, 0, "frame-units");
        // This corpus observes the canonical peer packet and exact wire below,
        // without passing the test observer through a UTF-8 JSON view.
        host.observe_raw_send.set(true);
        runtime.operations.borrow_mut().clear();
        let result = if private {
            ApexOmniPrivateWebsocket::new(parent, host.clone())
                .unwrap()
                .on_message(raw)
        } else {
            ApexOmniPublicWebsocket::new(parent, host.clone())
                .unwrap()
                .on_message(raw)
        };
        if refused {
            assert!(
                result.unwrap_err().message.contains("invalid utf-16"),
                "{marker} typed frame refusal is visible"
            );
        } else {
            result.unwrap();
        }
        let sent = host.raw_frames.borrow().clone();
        assert_eq!(
            sent.len(),
            if private && !refused { 1 } else { 0 },
            "{marker} native transmission count"
        );
        if private {
            let packet = safe_cex_transport::how::json_parse(&sent[0]);
            assert!(
                matches!(property(&packet, "op"), Some(Input::Text(value)) if value.iter().copied().eq("pong".encode_utf16())),
                "{marker} native peer operation"
            );
            let Some(Input::Array(args)) = property(&packet, "args") else {
                panic!("{marker} native peer args");
            };
            assert_eq!(args.len(), 1, "{marker} native peer argument floor");
            assert!(
                matches!(&args[0], Input::Text(value) if value == units),
                "{marker} decoded native peer argument units"
            );
        }
        if private {
            assert_eq!(
                row["sent"].as_array().unwrap().len(),
                1,
                "{marker} independent source transmission floor"
            );
            assert_eq!(
                row["sent"][0]["op"], "pong",
                "{marker} source peer decoded operation"
            );
            assert_eq!(
                row["sent"][0]["argumentType"], "string",
                "{marker} source peer argument kind"
            );
            assert_eq!(
                frame_units(&row["sent"][0]["argumentUnits"]),
                units,
                "{marker} source peer argument units"
            );
            assert_eq!(
                store.tickers().unwrap()[0].last.to_bits(),
                103f64.to_bits(),
                "{marker} native private Store untouched"
            );
            wire_observations.push((
                marker.clone(),
                sent,
                row["sent"][0]["raw"].as_str().unwrap().to_owned(),
            ));
        } else {
            assert_eq!(
                store.tickers().unwrap()[0].last.to_bits(),
                if refused {
                    103f64.to_bits()
                } else {
                    117.25f64.to_bits()
                },
                "{marker} native ticker last"
            );
            assert_eq!(
                frame_units(&row["ticker"]["idUnits"]),
                units,
                "{marker} source returned ticker id units"
            );
            assert_eq!(
                native::number(&row["ticker"]["last"]["bits"]).to_bits(),
                117.25f64.to_bits(),
                "{marker} source updated ticker"
            );
            if !record_refused {
                assert_eq!(
                    store.tickers().unwrap()[0]
                        .id
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                    units,
                    "{marker} native returned ticker id units"
                );
            }
        }
        let operations:Vec<_>=runtime.operations.borrow().iter().filter(|op|!matches!(op,J::Array(values)if values.first()==Some(&s("send"))||values.first()==Some(&s("initialize")))).cloned().collect();
        native::check(
            &J::Array(operations),
            &row["operations"],
            &format!("{marker} native receive/diagnostic/stats effects"),
        );
    }
    assert_eq!(
        native_refusals, 8,
        "APEX-WS-FRAME independent JSON-view refusal floor"
    );
    assert_eq!(
        native_positives, 8,
        "APEX-WS-FRAME independent JSON-view positive floor"
    );
    // All native decoded inputs, effects, outputs and unit assertions precede wire.
    assert_eq!(
        wire_observations.len(),
        8,
        "APEX-WS-FRAME independent wire floor"
    );
    for (marker, sent, source) in wire_observations {
        assert_eq!(sent, vec![source], "{marker} native exact positive wire");
    }
}

#[path = "../../../test-support/typed_utf16.rs"]
mod typed_utf16;
