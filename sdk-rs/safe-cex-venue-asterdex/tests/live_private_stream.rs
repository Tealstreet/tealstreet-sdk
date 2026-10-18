#[path = "support/retained_assert.rs"]
mod retained_assert;
// Shared deterministic host also exposes timer inspection for other suites.
#[allow(dead_code)]
#[path = "support/subscription_runtime.rs"]
mod host;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask, LogSeverity},
    websocket::{
        BaseWebSocket, ReopenResync, WebSocketConnectionOptions, WsEvent, WsParent, WsRuntime,
        WsSocket,
    },
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::{Balance, ExchangeName, ExchangeOptions, Order, Position};
use safe_cex_venue_asterdex::{
    account_helpers::AssetBalanceUpdate,
    private_repairs::RepairParent,
    private_stream::{PrivateFill, PrivateStream, PrivateStreamParent},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    sync::Arc,
    task::{Context, Poll},
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
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, e: &ExchangeError, c: &Value) {
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
#[derive(Debug, Clone)]
enum Event {
    Order(Order),
    Fill(PrivateFill, Vec<Order>),
    Positions(Vec<Position>),
    Account(Value, Balance),
}
struct Parent {
    owner: Rc<BaseExchange>,
    pv: Cell<f64>,
    bv: Cell<f64>,
    events: RefCell<Vec<Event>>,
    logs: RefCell<Vec<(String, LogSeverity)>>,
    requests: RefCell<Vec<Value>>,
    post_error: Option<String>,
    coordinator: Rc<ReopenResync>,
    stream: RefCell<Weak<PrivateStream>>,
    close_expected: Cell<bool>,
    close_flags: RefCell<Vec<Vec<bool>>>,
    mutate_on_positions: Cell<bool>,
}
impl RepairParent for Parent {
    fn position_state_version(&self) -> f64 {
        self.pv.get()
    }
    fn balance_state_version(&self) -> f64 {
        self.bv.get()
    }
    fn fetch_positions(&self) -> ExchangeFuture<'static, Vec<Position>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_orders(&self) -> ExchangeFuture<'static, Vec<Order>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn reconcile_balance_after_private_update(&self, _: f64) -> ExchangeFuture<'static, bool> {
        Box::pin(async { Ok(true) })
    }
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "asterdex"
    }
    fn is_disposed(&self) -> bool {
        self.owner.is_disposed()
    }
    fn options(&self) -> ExchangeOptions {
        self.owner.options()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.owner.store.clone()
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        true
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.coordinator.clone()
    }
    fn fetch_and_set_private_snapshot(&self, n: usize) -> ExchangeFuture<'static, ()> {
        self.requests.borrow_mut().push(json!(["SNAPSHOT", n]));
        Box::pin(async { Ok(()) })
    }
    fn log(&self, m: &str, l: LogSeverity) -> Result<(), ExchangeError> {
        self.logs.borrow_mut().push((m.into(), l));
        self.owner.log(m, l.as_str())
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        if self.close_expected.replace(false) {
            let s = self.stream.borrow().upgrade().unwrap();
            self.close_flags.borrow_mut().push(flags(&s));
        }
        Ok(())
    }
    fn update_latency(&self, _: f64) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_and_log_error(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&Value>,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
}
impl PrivateStreamParent for Parent {
    fn owner(&self) -> Rc<BaseExchange> {
        self.owner.clone()
    }
    fn get_fapi_path(&self, e: &str) -> String {
        format!("/fapi/v3/{e}")
    }
    fn post_listen_key(&self, p: String) -> ExchangeFuture<'static, String> {
        self.requests.borrow_mut().push(json!(["POST", p]));
        let r = self
            .post_error
            .as_ref()
            .map_or_else(|| Ok("native-key".into()), |e| Err(ExchangeError::new(e)));
        Box::pin(async move { r })
    }
    fn put_listen_key(&self, _: String, _: String) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn virtual_time_ms(&self) -> f64 {
        1234.0
    }
    fn mark_position_state_updated(&self) {
        self.pv.set(self.pv.get() + 1.0);
    }
    fn mark_balance_state_updated(&self) -> f64 {
        self.bv.set(self.bv.get() + 1.0);
        self.bv.get()
    }
    fn emit_order(&self, o: &Order) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Order(o.clone()));
        Ok(())
    }
    fn emit_fill(&self, f: &PrivateFill) -> Result<(), ExchangeError> {
        self.events
            .borrow_mut()
            .push(Event::Fill(f.clone(), self.store().orders().to_vec()));
        Ok(())
    }
    fn emit_positions(&self, p: &[Position]) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Positions(p.to_vec()));
        if self.mutate_on_positions.get() {
            self.owner.store.update(
                StoreUpdate {
                    positions: Some(vec![]),
                    ..Default::default()
                },
                false,
            )?;
        }
        Ok(())
    }
    fn emit_account(&self, d: &Value) -> Result<(), ExchangeError> {
        self.events
            .borrow_mut()
            .push(Event::Account(d.clone(), self.store().balance().clone()));
        Ok(())
    }
}
fn flags(s: &PrivateStream) -> Vec<bool> {
    let l = s.listen_key.state();
    let r = s.repairs.state();
    vec![
        l.refresh_timer,
        l.watchdog_timer,
        l.connect_timer,
        r.position_timer,
        r.balance_timer,
        r.order_timer,
    ]
}
fn immediate<T>(mut f: ExchangeFuture<'_, T>) -> T {
    match f
        .as_mut()
        .poll(&mut Context::from_waker(std::task::Waker::noop()))
    {
        Poll::Ready(Ok(v)) => v,
        Poll::Ready(Err(e)) => panic!("native error {e}"),
        Poll::Pending => panic!("unexpected pending direct fixture"),
    }
}
fn fixture(c: &Value) -> (Rc<PrivateStream>, Rc<Parent>, Rc<Runtime>) {
    let host = Rc::new(host::Runtime::default());
    host.wall.set(1000.0);
    let runtime = Rc::new(Runtime {
        host: host.clone(),
        socket: Rc::new(Socket::default()),
        urls: RefCell::new(vec![]),
        fail: c["dialError"].as_str().map(str::to_owned),
        reconnecting: c["reconnecting"].as_bool().unwrap_or(false),
    });
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        serde_json::from_value(c["initial"].clone()).unwrap(),
        false,
    )
    .unwrap();
    let owner = Rc::new(BaseExchange::new(
        ExchangeName::Asterdex,
        serde_json::from_value(
            json!({"key":"","secret":"","testnet":c["testnet"].as_bool().unwrap_or(false)}),
        )
        .unwrap(),
        store,
        host,
        ExchangeSettings::default(),
    ));
    let parent = Rc::new(Parent {
        owner,
        pv: Cell::new(0.0),
        bv: Cell::new(0.0),
        events: RefCell::new(vec![]),
        logs: RefCell::new(vec![]),
        requests: RefCell::new(vec![]),
        post_error: c["postError"].as_str().map(str::to_owned),
        coordinator: Rc::new(ReopenResync::default()),
        stream: RefCell::new(Weak::new()),
        close_expected: Cell::new(false),
        close_flags: RefCell::new(vec![]),
        mutate_on_positions: Cell::new(false),
    });
    let s = PrivateStream::new(42, parent.clone(), runtime.clone());
    *parent.stream.borrow_mut() = Rc::downgrade(&s);
    (s, parent, runtime)
}
fn number(actual: f64, expected: &Value, label: &str) {
    let value = if let Some(marker) = expected.get("number").and_then(Value::as_str) {
        match marker {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => panic!("unknown number marker"),
        }
    } else {
        expected
            .as_f64()
            .unwrap_or_else(|| panic!("expected numeric {label}: {expected}"))
    };
    if value.is_nan() {
        assert!(
            actual.is_nan(),
            "{label}: expected native NaN, got {actual}"
        );
    } else {
        assert_eq!(actual, value, "{label}");
    }
}
fn decoded_json(actual: &Value, expected: &Value, label: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(_)) => number(a.as_f64().unwrap(), expected, label),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{label} own keys"
            );
            for (key, value) in a {
                decoded_json(value, &b[key], &format!("{label}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{label} length");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                decoded_json(a, b, &format!("{label}[{index}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{label}"),
    }
}
fn optional_json(
    actual: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    expected: Option<&Value>,
    label: &str,
) {
    match (actual, expected) {
        (Some(a), Some(e)) => {
            let _ = label;
            retained_assert::assert_value(a, e);
        }
        (None, None) => {}
        _ => panic!("{label} field presence"),
    }
}
fn order(o: &Order, e: &Value, label: &str) {
    assert_eq!(o.id, e["id"].as_str().unwrap(), "{label} id");
    assert_eq!(
        o.status,
        serde_json::from_value::<safe_cex_types::OrderStatus>(e["status"].clone()).unwrap(),
        "{label} status"
    );
    assert_eq!(
        o.r#type,
        serde_json::from_value::<safe_cex_types::OrderType>(e["type"].clone()).unwrap(),
        "{label} type"
    );
    assert_eq!(
        o.side,
        serde_json::from_value::<safe_cex_types::OrderSide>(e["side"].clone()).unwrap(),
        "{label} side"
    );
    assert_eq!(o.symbol, e["symbol"].as_str().unwrap(), "{label} symbol");
    assert_eq!(
        o.reduce_only,
        e["reduceOnly"].as_bool().unwrap(),
        "{label} reduceOnly"
    );
    for (k, v) in [
        ("price", o.price.to_number()),
        ("amount", o.amount),
        ("filled", o.filled),
        ("remaining", o.remaining),
        ("lastUpdate", o.last_update),
    ] {
        number(v, &e[k], &format!("{label} {k}"));
    }
    optional_json(o.info.as_ref(), e.get("info"), &format!("{label} info"));
}
fn position(p: &Position, e: &Value, label: &str) {
    assert_eq!(p.id, e["id"].as_str().unwrap(), "{label} id");
    assert_eq!(p.symbol, e["symbol"].as_str().unwrap(), "{label} symbol");
    assert_eq!(
        p.side,
        serde_json::from_value::<safe_cex_types::PositionSide>(e["side"].clone()).unwrap(),
        "{label} side"
    );
    for (k, v) in [
        ("entryPrice", p.entry_price),
        ("notional", p.notional),
        ("leverage", p.leverage),
        ("unrealizedPnl", p.unrealized_pnl),
        ("contracts", p.contracts),
        ("liquidationPrice", p.liquidation_price),
    ] {
        number(v, &e[k], &format!("{label} {k}"));
    }
    if let Some(v) = p.realized_pnl.into_value() {
        number(v, &e["realizedPnl"], label);
    } else {
        assert!(e.get("realizedPnl").is_none(), "{label} realizedPnl");
    }
    optional_json(p.info.as_ref(), e.get("info"), &format!("{label} info"));
}
fn balance(b: &Balance, e: &Value, label: &str) {
    for (k, v) in [
        ("total", b.total),
        ("free", b.free),
        ("used", b.used),
        ("upnl", b.upnl),
    ] {
        number(v, &e[k], &format!("{label} {k}"));
    }
    optional_json(b.info.as_ref(), e.get("info"), &format!("{label} info"));
    match (&b.currencies, e.get("currencies")) {
        (Some(c), Some(expected)) => {
            let expected = expected.as_object().unwrap();
            assert_eq!(
                c.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>(),
                "{label} currencies keys"
            );
            for (k, v) in c {
                balance(v.as_balance().unwrap(), &expected[k], label);
            }
        }
        (None, None) => {}
        _ => panic!("{label} currency presence"),
    }
}
fn snapshot(s: &PrivateStream, p: &Parent, e: &Value, label: &str) {
    assert_eq!(
        flags(s),
        e["flags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_bool().unwrap())
            .collect::<Vec<_>>(),
        "{label} timers"
    );
    let state = s.listen_key.state();
    assert_eq!(
        state.listen_key,
        e["key"].as_str().unwrap(),
        "{label} listen key"
    );
    number(state.retry_count, &e["retry"], &format!("{label} retry"));
    number(
        s.last_account_event_time(),
        &e["last"],
        &format!("{label} event ordering"),
    );
    number(p.pv.get(), &e["pv"], &format!("{label} position version"));
    number(p.bv.get(), &e["bv"], &format!("{label} balance version"));
    let store = p.store();
    let orders = store.orders();
    let positions = store.positions();
    assert_eq!(
        orders.len(),
        e["orders"].as_array().unwrap().len(),
        "{label} order cardinality"
    );
    for (o, e) in orders.iter().zip(e["orders"].as_array().unwrap()) {
        order(o, e, label);
    }
    assert_eq!(
        positions.len(),
        e["positions"].as_array().unwrap().len(),
        "{label} position cardinality"
    );
    for (p, e) in positions.iter().zip(e["positions"].as_array().unwrap()) {
        position(p, e, label);
    }
    balance(&store.balance(), &e["balance"], label);
}
const NAMES: &[(&str, &str, usize)] = &[
    ("construct-close-dispose", "lifecycle", 4),
    ("connect-production", "connection", 2),
    ("connect-testnet", "connection", 1),
    ("post-failure-retry", "connection", 1),
    ("dial-failure-retry", "connection", 1),
    ("message-routing", "dispatch", 10),
    ("new-close-position", "orders", 1),
    ("partial-overfill", "orders", 1),
    ("filled-removes-before-order-emission", "orders", 1),
    ("nontrade-filled", "orders", 1),
    ("cancel-preserves-fill", "orders", 1),
    ("unknown-status-fallback", "orders", 1),
    ("legacy-report", "orders", 1),
    ("bad-numbers-native-nan", "orders", 1),
    ("balance-single-asset", "balance", 1),
    ("balance-first-collateral", "balance", 1),
    ("balance-multiasset-retains-root", "balance", 1),
    ("balance-repeated-asset", "balance", 1),
    ("account-updates-before-balance", "account", 1),
    ("account-tiny-existing-quantity", "account", 1),
    ("account-flip-removes-opposite", "account", 1),
    ("zero-removes-both-hedges", "account", 1),
    ("account-stale-invalid", "account", 5),
    ("account-multiasset-reconcile", "account", 1),
    ("account-missing-payload-parts", "account", 1),
];
fn source_cases() -> Vec<Value> {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-stream-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 25, "independent private scenario floor");
    for ((name, family, steps), c) in NAMES.iter().zip(&cases) {
        assert_eq!(c["name"], *name);
        assert_eq!(c["family"], *family);
        assert_eq!(c["ops"].as_array().unwrap().len(), *steps);
        assert_eq!(c["snapshots"].as_array().unwrap().len(), steps + 1);
    }
    cases
}
fn family(family: &str) {
    let mut wire_checks = vec![];
    for c in source_cases().iter().filter(|c| c["family"] == family) {
        let name = c["name"].as_str().unwrap();
        let (s, p, r) = fixture(c);
        let keys = s
            .socket
            .message_handlers
            .borrow()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                "ORDER_TRADE_UPDATE",
                "ACCOUNT_UPDATE",
                "listenKeyExpired",
                "executionReport",
                "outboundAccountPosition"
            ],
            "private lifecycle handler assembly"
        );
        snapshot(&s, &p, &c["snapshots"][0], name);
        let mut results = vec![];
        let mut receipt_marks = vec![];
        for (i, op) in c["ops"].as_array().unwrap().iter().enumerate() {
            match op["kind"].as_str().unwrap() {
                "connect" => immediate(s.connect_and_subscribe()),
                "open" => {
                    r.socket.events.borrow_mut().push_back(WsEvent::Open);
                }
                "message" => {
                    let raw = op["raw"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| serde_json::to_string(&op["data"]).unwrap());
                    s.on_message(&safe_cex_transport::socket::SocketFrame::Text(raw))
                        .unwrap();
                    if i < 9 && family == "dispatch" {
                        receipt_marks.push(s.socket.last_message_timestamp());
                    }
                }
                "balance" => {
                    let updates: Vec<AssetBalanceUpdate> = op["updates"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| AssetBalanceUpdate {
                            asset: v["asset"].as_str().unwrap().into(),
                            total: v["total"].as_f64().unwrap(),
                            free: v["free"].as_f64().unwrap(),
                            used: v["used"].as_f64().unwrap(),
                        })
                        .collect();
                    let deltas = op["deltas"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| (v[0].as_str().unwrap().into(), v[1].as_f64().unwrap()))
                        .collect::<Vec<_>>();
                    results.push(s.apply_balance_updates(&updates, &deltas).unwrap());
                }
                "schedule-all" => {
                    s.listen_key.schedule_connect_retry().unwrap();
                    s.repairs.schedule_position_repair().unwrap();
                    s.repairs.schedule_balance_reconcile(p.bv.get()).unwrap();
                    s.repairs.schedule_order_reconcile().unwrap();
                }
                "close" => {
                    p.close_expected.set(true);
                    s.socket.on_close(Some(1000), "").unwrap();
                }
                "dispose" => s.dispose(),
                _ => panic!("unknown step"),
            };
            r.host.pump();
            snapshot(
                &s,
                &p,
                &c["snapshots"][i + 1],
                &format!("private {family} {name} step {i}"),
            );
        }
        if name == "message-routing" {
            let b = p.store().balance().clone();
            assert_eq!(b.total, 10.0, "native outbound balance total");
            assert_eq!(b.free, 8.0, "native outbound balance free");
            assert_eq!(b.used, 2.0, "native outbound balance used");
        }
        // Every decoded native state/event assertion precedes URL/request/frame checks.
        let events = p.events.borrow();
        let expected = c["events"].as_array().unwrap();
        assert_eq!(
            events.len(),
            expected.len(),
            "private {family} {name} event count"
        );
        for (event, e) in events.iter().zip(expected) {
            match event {
                Event::Order(o) => {
                    assert_eq!(e["event"], "order");
                    assert_eq!(e["args"][0], o.id);
                    order(o, &e["args"][1], name);
                }
                Event::Fill(f, _) => {
                    assert_eq!(e["event"], "fill");
                    let e = &e["args"][0];
                    assert_eq!(f.id.as_deref(), e["id"].as_str(), "private orders fill id");
                    assert_eq!(f.order_id.as_deref(), e["orderId"].as_str());
                    assert_eq!(f.symbol.as_deref(), e["symbol"].as_str());
                    assert_eq!(
                        f.side,
                        serde_json::from_value::<safe_cex_types::OrderSide>(e["side"].clone())
                            .unwrap()
                    );
                    number(f.amount, &e["amount"], "private orders fill amount");
                    number(f.price, &e["price"], "private orders fill price");
                }
                Event::Positions(ps) => {
                    assert_eq!(e["event"], "positions");
                    let es = e["args"][0].as_array().unwrap();
                    assert_eq!(ps.len(), es.len());
                    for (p, e) in ps.iter().zip(es) {
                        position(p, e, name);
                    }
                }
                Event::Account(d, at_emit) => {
                    assert_eq!(e["event"], "account");
                    decoded_json(d, &e["args"][0], "native account event");
                    balance(
                        at_emit,
                        &e["balanceAtAccount"],
                        "native outbound balance before emission",
                    );
                }
            }
        }
        assert_eq!(
            results,
            c["results"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_bool().unwrap())
                .collect::<Vec<_>>(),
            "private balance reconciliation"
        );
        if family == "lifecycle" {
            assert_eq!(
                *p.close_flags.borrow(),
                vec![vec![false, false, true, false, false, false]],
                "private lifecycle clear before base close"
            );
            assert!(r.socket.closed.get(), "private lifecycle terminate");
            assert!(!p.is_disposed(), "private lifecycle preserve parent");
        }
        if family == "dispatch" {
            assert_eq!(
                receipt_marks,
                vec![Some(1000.0); 9],
                "private dispatch receive"
            );
            assert_eq!(
                s.socket
                    .message_handlers_stats
                    .borrow()
                    .get("outboundAccountPosition")
                    .unwrap()
                    .last_callbacks_count,
                safe_cex_num::PrototypeScalar::from(Some(1)),
                "private dispatch statistics"
            );
            let logs = p.logs.borrow();
            assert!(logs.iter().any(|(m,_)|m=="Failed to process private message: this.messageHandlers[eventType].forEach is not a function"),"private dispatch inherited catch");
        }
        // Base diagnostics adds its own lifecycle logs; venue-specific logs still match.
        let own_logs: Vec<(String, String)> = r
            .host
            .logs
            .borrow()
            .iter()
            .map(|(severity, message)| (message.clone(), severity.clone()))
            .collect();
        // The oracle injects Base I/O. Exclude only these exact shared-core
        // close/reconnect diagnostics; extra or duplicate venue errors must fail.
        let core_logs = [
            "WebSocket connection disconnected (code: 1000, reason: none), reconnecting...",
            "Explicit reconnect call from ping pong miss or online status change",
            "WebSocket connection disconnected (code: unknown, reason: none), reconnecting...",
            "Reconnecting in 1000ms",
        ];
        let venue_logs: Vec<_> = own_logs
            .into_iter()
            .filter(|(m, _)| !core_logs.contains(&m.as_str()))
            .collect();
        let expected_logs: Vec<_> = c["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l["message"].as_str().unwrap().to_owned(),
                    l["severity"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            venue_logs, expected_logs,
            "private {family} {name} exact venue logs"
        );
        wire_checks.push((
            p.requests.borrow().clone(),
            r.urls.borrow().clone(),
            r.socket.wire.borrow().clone(),
            c.clone(),
        ));
        s.dispose();
        assert!(
            r.host.errors.borrow().is_empty(),
            "native uncaught errors {:?}",
            r.host.errors.borrow()
        );
    } // All cases' decoded native assertions finish before any wire assertion.
    for (requests, urls, wire, c) in wire_checks {
        assert_eq!(
            requests,
            c["requests"].as_array().unwrap().clone(),
            "private connection request"
        );
        assert_eq!(
            urls,
            c["urls"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "private connection URL"
        );
        assert_eq!(
            wire,
            c["wire"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "private connection emitted frame"
        );
    }
}

#[test]
#[ignore = "requires live source oracle"]
fn private_lifecycle_matches_source() {
    family("lifecycle");
}
#[test]
#[ignore = "requires live source oracle"]
fn private_connection_matches_source() {
    family("connection");
}
#[test]
#[ignore = "requires live source oracle"]
fn private_dispatch_matches_source() {
    family("dispatch");
}
#[test]
#[ignore = "requires live source oracle"]
fn private_orders_match_source() {
    family("orders");
}
#[test]
#[ignore = "requires live source oracle"]
fn private_balance_matches_source() {
    family("balance");
}
#[test]
#[ignore = "requires live source oracle"]
fn private_account_matches_source() {
    family("account");
}
#[test]
fn native_positions_listener_can_mutate_store() {
    let c = json!({"initial":{},"reconnecting":false});
    let (s, p, _) = fixture(&c);
    p.mutate_on_positions.set(true);
    s.handle_account_position_update(&json!({"E":1,"a":{"P":[]}}))
        .unwrap();
    assert_eq!(p.pv.get(), 1.0);
    assert_eq!(p.events.borrow().len(), 1);
}

#[test]
#[ignore = "requires live source boundary oracle"]
fn disclosed_native_boundaries_are_observable() {
    let out = std::process::Command::new("node")
        .args([
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/private-stream-oracle.cjs"
            ),
            "--boundaries",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    let (s, p, r) = fixture(
        &json!({"initial":{"balance":{"total":100,"free":80,"used":20,"upnl":7,"currencies":{"USDT":{"total":100,"free":80,"used":20,"upnl":7}}}},"reconnecting":false}),
    );
    assert!(
        s.apply_balance_updates(
            &[AssetBalanceUpdate {
                asset: "__proto__".into(),
                total: 2.0,
                free: 1.0,
                used: 1.0
            }],
            &[]
        )
        .unwrap()
    );
    let current = p.store().balance().clone();
    let currencies = current.currencies.unwrap();
    assert_eq!(
        currencies.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["USDT", "__proto__"],
        "native own currency boundary"
    );
    assert_eq!(
        currencies
            .get("__proto__")
            .unwrap()
            .as_balance()
            .unwrap()
            .total,
        2.0
    );
    assert_eq!(source["currencyKeys"], json!(["USDT"]));
    assert_eq!(source["prototypeTotal"], 2);
    assert_eq!(source["reconcile"], true);
    let prefix = serde_json::to_string(&json!({"e":"unknown","text":""}))
        .unwrap()
        .find("\"text\":\"")
        .unwrap()
        + 8;
    s.on_message(&safe_cex_transport::socket::SocketFrame::Text(
        serde_json::to_string(
            &json!({"e":"unknown","text":format!("{}😀","x".repeat(199-prefix))}),
        )
        .unwrap(),
    ))
    .unwrap();
    let logs = r.host.logs.borrow();
    let message = &logs
        .iter()
        .find(|(_, m)| m.starts_with("Unhandled private event type"))
        .unwrap()
        .1;
    assert_eq!(
        message.encode_utf16().last(),
        Some(0xfffd),
        "native lone cutoff replacement"
    );
    assert_eq!(
        source["lastLogUnit"], 0xd83d,
        "source lone cutoff high surrogate"
    );
    drop(logs);
    let error=s.handle_order_trade_update(&json!({"o":{"i":9,"X":"NEW","s":"BTCUSDT","o":"UNRECOGNIZED","S":"BUY","q":"1","z":"0"}})).unwrap_err();
    assert_eq!(
        error.message,
        "Unknown order type cannot enter native OrderType"
    );
    assert!(p.store().orders().is_empty());
    assert_eq!(source["orderType"], "UNRECOGNIZED");
    s.dispose();
}

#[test]
fn native_dispatch_captures_original_handler_length() {
    let (s, _, _) = fixture(&json!({"initial":{},"reconnecting":false}));
    let calls = Rc::new(RefCell::new(vec![]));
    let handlers = Rc::new(RefCell::new(Vec::<
        safe_cex_exchange::websocket::MessageHandler,
    >::new()));
    let h = handlers.clone();
    let c = calls.clone();
    handlers.borrow_mut().push(Rc::new(move |_| {
        c.borrow_mut().push("first");
        let c = c.clone();
        h.borrow_mut().push(Rc::new(move |_| {
            c.borrow_mut().push("added");
            Ok(())
        }));
        Ok(())
    }));
    s.socket
        .message_handlers
        .borrow_mut()
        .insert("custom".into(), handlers);
    s.on_message(&safe_cex_transport::socket::SocketFrame::Text(
        "{\"e\":\"custom\"}".into(),
    ))
    .unwrap();
    assert_eq!(
        *calls.borrow(),
        vec!["first"],
        "native Array.forEach captures original length"
    );
    s.dispose();
}

#[test]
fn cumulative_fill_is_written_before_emission() {
    let initial = json!({"orders":[{"id":"1","symbol":"BTCUSDT","status":"open","type":"limit","side":"buy","price":10,"amount":5,"filled":0,"remaining":5,"reduceOnly":false,"lastUpdate":1}]});
    let (s, p, _) = fixture(&json!({"initial":initial}));
    s.handle_order_trade_update(&json!({"o":{"i":1,"s":"BTCUSDT","X":"PARTIALLY_FILLED","S":"BUY","q":"5","z":"7","l":"2","L":"12","T":99}})).unwrap();
    let events = p.events.borrow();
    let Event::Fill(_, orders) = &events[0] else {
        panic!("native cumulative fill must emit first")
    };
    assert_eq!(orders.len(), 1, "native cumulative fill retains one order");
    assert_eq!(
        orders[0].filled, 7.0,
        "native cumulative fill is written before emission"
    );
    assert_eq!(
        orders[0].remaining, 0.0,
        "native cumulative fill clamps before emission"
    );
    assert_eq!(
        orders[0].last_update, 99.0,
        "native cumulative fill event time before emission"
    );
    s.dispose();
}

#[test]
fn missing_order_id_does_not_skip_quantity_coercion() {
    for data in [
        json!({"i":null,"z":{"toString":false},"q":"1"}),
        json!({"z":"1","q":{"toString":false}}),
    ] {
        let result =
            safe_cex_venue_asterdex::account_helpers::mirror_cumulative_order_fill(&data, || {
                panic!("clock must remain lazy")
            });
        let error = result.expect_err("native missing ID still evaluates quantity coercion");
        assert_eq!(
            error.name, "TypeError",
            "native missing ID quantity exception class"
        );
    }
}

#[test]
fn null_account_payload_throws_before_emission() {
    let (s, p, _) = fixture(&json!({"initial":{}}));
    let error = s
        .handle_account_update(&Value::Null)
        .expect_err("native null account must throw before emission");
    assert_eq!(error.name, "TypeError", "native null account error class");
    assert_eq!(
        error.message, "Cannot read properties of null (reading 'B')",
        "native null account error property"
    );
    assert!(
        p.events.borrow().is_empty(),
        "native null account must not emit"
    );
    s.dispose();
}
