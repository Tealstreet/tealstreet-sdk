use safe_cex_exchange::{runtime::*, websocket::*};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::{Candle, ExchangeOptions};
use safe_cex_venue_binance::{
    delegated_methods::CandleOptionsHandle, private_message::*, public_candle_delegate::*,
    public_message::*, spot_public_message::on_spot_public_message,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};

type Trace = Rc<RefCell<Vec<&'static str>>>;
type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
struct Runtime {
    trace: Trace,
    timers: Cell<usize>,
    jobs: RefCell<Vec<LocalFuture>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1250.0
    }
    fn now_ms(&self) -> f64 {
        self.trace.borrow_mut().push("clock");
        1250.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, future: LocalFuture) -> Box<dyn ExchangeTask> {
        self.timers.set(self.timers.get() + 1);
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.0
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
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected background exception")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("unexpected tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("unexpected dial")
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
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Parent {
    trace: Trace,
    disposed: Cell<bool>,
    health: Cell<usize>,
    name: &'static str,
    error: Failure,
    store: Rc<DefaultStore>,
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "binance"
    }
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn options(&self) -> ExchangeOptions {
        serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        true
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        Rc::new(ReopenResync::default())
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        panic!("unexpected resync")
    }
    fn log(&self, _: &str, _: LogSeverity) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.trace.borrow_mut().push("health");
        self.health.set(self.health.get() + 1);
        if self.name == "health-disposes" {
            self.disposed.set(true);
        }
        if self.name == "health-fail" {
            Err((*self.error).clone())
        } else {
            Ok(())
        }
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
        _: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
}
struct Protocol;
impl WsProtocol for Protocol {}
const PUBLIC_CASES: [&str; 14] = [
    "normal",
    "disposed",
    "read-fail",
    "read-fail-disposed",
    "read-disposes",
    "read-revives",
    "health-fail",
    "health-disposes",
    "repeat",
    "dispatch-fail",
    "missing-data",
    "object-data",
    "dispatch-mutates",
    "preinitialized",
];
const PRIVATE_CASES: [&str; 36] = [
    "account",
    "order",
    "special",
    "pong",
    "account-pong",
    "order-pong",
    "special-pong",
    "id-string",
    "unknown",
    "invalid",
    "null",
    "array",
    "primitive",
    "big-id",
    "string-corruption",
    "object-data",
    "array-data",
    "coercion-fail",
    "disposed",
    "read-fail",
    "health-fail",
    "health-disposes",
    "diagnostic-fail",
    "diagnostic-disposes",
    "diagnostic-mutates",
    "account-fail",
    "order-fail",
    "special-fail",
    "pong-fail",
    "account-mutates",
    "order-mutates",
    "callback-disposes",
    "missing-data",
    "read-disposes",
    "read-revives",
    "preinitialized",
];
const DELEGATE_CASES: [&str; 6] = [
    "normal",
    "listen-fail",
    "callback-fail",
    "cleanup-fail",
    "mutate-options",
    "repeat-cleanup",
];
fn failure(message: &str) -> Failure {
    let mut e = ExchangeError::new(message);
    e.name = "RangeError".into();
    e.metadata.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "original-code"
    )));
    Rc::new(e)
}
fn data(value: Value) -> MessageData {
    Rc::new(RefCell::new(value))
}
fn input(name: &str, futures: bool) -> Option<MessageData> {
    let special = if futures {
        "ALGO_UPDATE"
    } else {
        "CONDITIONAL_ORDER_TRADE_UPDATE"
    };
    let event = if name.starts_with("order") {
        "ORDER_TRADE_UPDATE"
    } else if name.starts_with("special") {
        special
    } else if ["pong", "pong-fail", "unknown", "id-string"].contains(&name) {
        "OTHER"
    } else {
        "ACCOUNT_UPDATE"
    };
    let mut object = json!({"e":event,"o":{"i":"small-id","q":"3.6","p":"12.3"},"id":if name.contains("pong")||name.ends_with("mutates") {42} else {7}});
    if name == "id-string" {
        object["id"] = json!("42");
    }
    let value = match name {
        "object-data" | "diagnostic-mutates" => object,
        "array-data" => json!([object.to_string()]),
        "coercion-fail" => json!({"toString":0}),
        "big-id" => json!(
            r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":9007199254740993,"q":"3.6","p":"12.3"},"id":7}"#
        ),
        "string-corruption" => json!(
            r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":"x:1234567890123456,","q":"3.6","p":"12.3"}}"#
        ),
        "invalid" => json!("not-json"),
        "null" => json!("null"),
        "array" => json!("[]"),
        "primitive" => json!("1"),
        "missing-data" => return None,
        _ => json!(object.to_string()),
    };
    Some(data(value))
}
fn fixture(name: &'static str) -> (Rc<BaseWebSocket>, Rc<Parent>, Rc<Runtime>, Trace, Failure) {
    let trace = Rc::new(RefCell::new(vec![]));
    let error = failure("message fixture");
    let parent = Rc::new(Parent {
        trace: trace.clone(),
        disposed: Cell::new(["disposed", "read-fail-disposed", "read-revives"].contains(&name)),
        health: Cell::new(0),
        name,
        error: error.clone(),
        store: Rc::new(DefaultStore::new()),
    });
    let runtime = Rc::new(Runtime {
        trace: trace.clone(),
        timers: Cell::new(0),
        jobs: RefCell::new(vec![]),
    });
    let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
    if name == "preinitialized" {
        socket.track_last_received().unwrap();
        trace.borrow_mut().clear();
        parent.health.set(0);
        runtime.timers.set(0);
    }
    (socket, parent, runtime, trace, error)
}
fn read(
    name: &str,
    parent: &Parent,
    trace: &Trace,
    reads: &Cell<usize>,
    value: &Option<MessageData>,
    error: &Failure,
) -> ReadData {
    trace.borrow_mut().push("read");
    reads.set(reads.get() + 1);
    if name.starts_with("read-fail") {
        return Err(error.clone());
    }
    if name == "read-disposes" {
        parent.disposed.set(true);
    }
    if name == "read-revives" {
        parent.disposed.set(false);
    }
    Ok(value.clone())
}
fn error_fields(
    result: &Result<(), Failure>,
    original: &Failure,
    name: &str,
) -> (Option<&'static str>, Option<String>, Option<String>) {
    match result {
        Ok(()) => (None, None, None),
        Err(e) => {
            // Core health errors are returned by value. This wire category is value
            // provenance, not an Rc identity claim; callback errors are checked below.
            let original_error = e.message == original.message;
            if original_error {
                assert_eq!(
                    observed_error_code(&e.metadata.code),
                    Some(json!("original-code")),
                    "MESSAGE native error metadata"
                );
                if name != "health-fail" {
                    assert!(
                        Rc::ptr_eq(e, original),
                        "MESSAGE native original callback error identity"
                    );
                }
            }
            (
                Some(if original_error {
                    "original"
                } else {
                    "generated"
                }),
                Some(e.name.clone()),
                Some(e.message.clone()),
            )
        }
    }
}
fn assert_health(
    lane: &str,
    index: usize,
    name: &str,
    socket: &BaseWebSocket,
    parent: &Parent,
    runtime: &Runtime,
) {
    let tracked = !name.starts_with("read-fail") && !matches!(name, "disposed" | "read-disposes");
    let initialized = tracked || name == "preinitialized";
    let health = usize::from(tracked && name != "preinitialized");
    let timers = usize::from(tracked && name != "preinitialized" && name != "health-fail");
    assert_eq!(
        socket.diagnostics().initialized,
        initialized,
        "MESSAGE {lane} native initialized case{index}"
    );
    assert_eq!(
        socket.diagnostics().last_message_timestamp,
        initialized.then_some(1250.0),
        "MESSAGE {lane} native last received case{index}"
    );
    assert_eq!(
        parent.health.get(),
        health,
        "MESSAGE {lane} native health count case{index}"
    );
    assert_eq!(
        runtime.timers.get(),
        timers,
        "MESSAGE {lane} native timer count case{index}"
    );
}
fn compare(lane: &str, native: Vec<Value>, count: usize) {
    // All independent native cases/assertions finish before invoking any source oracle.
    assert_eq!(
        native.len(),
        count,
        "MESSAGE independently authored native case floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/message-entry.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        154,
        "MESSAGE independently authored total source floor 70+72+12"
    );
    let source: Vec<_> = source.into_iter().filter(|r| r["lane"] == lane).collect();
    assert_eq!(source.len(), count);
    for (index, (left, right)) in native.iter().zip(source.iter()).enumerate() {
        compare_value(left, right, &format!("MESSAGE {lane} source case{index}"));
    }
}
fn compare_value(left: &Value, right: &Value, label: &str) {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => assert_eq!(
            l.as_f64().unwrap().to_bits(),
            r.as_f64().unwrap().to_bits(),
            "{label} decoded number"
        ),
        (Value::Array(l), Value::Array(r)) => {
            assert_eq!(l.len(), r.len(), "{label} array count");
            for (i, (a, b)) in l.iter().zip(r).enumerate() {
                compare_value(a, b, &format!("{label}/{i}"));
            }
        }
        (Value::Object(l), Value::Object(r)) => {
            assert_eq!(l.len(), r.len(), "{label} object count");
            for (k, v) in l {
                compare_value(v, r.get(k).expect("source field"), &format!("{label}/{k}"));
            }
        }
        _ => assert_eq!(left, right, "{label}"),
    }
}
fn public(lane: &str) {
    let mut native = vec![];
    for (index, &name) in PUBLIC_CASES.iter().enumerate() {
        let (socket, parent, runtime, trace, error) = fixture(name);
        let reads = Cell::new(0);
        let dispatched = Cell::new(0);
        let value = if name == "missing-data" {
            None
        } else if ["object-data", "dispatch-mutates"].contains(&name) {
            Some(data(json!({"e":"bookTicker","q":"3.6","p":"12.3"})))
        } else {
            Some(data(json!(
                "{\"e\":\"bookTicker\",\"q\":\"3.6\",\"p\":\"12.3\"}"
            )))
        };
        let mut result = Ok(());
        for _ in 0..if name == "repeat" { 2 } else { 1 } {
            let reader = || read(name, &parent, &trace, &reads, &value, &error);
            let dispatch = |received: Option<MessageData>| {
                trace.borrow_mut().push("dispatch");
                dispatched.set(dispatched.get() + 1);
                assert_eq!(
                    received.is_some(),
                    value.is_some(),
                    "MESSAGE native undefined data"
                );
                if let (Some(received), Some(value)) = (received, value.as_ref()) {
                    assert!(
                        Rc::ptr_eq(&received, value),
                        "MESSAGE native original data referent"
                    );
                    if received.borrow().is_object() {
                        assert_eq!(
                            received.borrow()["p"].as_str(),
                            Some("12.3"),
                            "MESSAGE native decoded public price"
                        );
                    }
                    if name == "dispatch-mutates" {
                        received.borrow_mut()["q"] = json!("4");
                    }
                }
                if name == "dispatch-fail" {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            };
            result = match lane {
                "futures_market_message" => on_futures_market_message(&socket, reader, dispatch),
                "futures_public_message" => on_futures_public_message(&socket, reader, dispatch),
                "pm_market_message" => on_pm_market_message(&socket, reader, dispatch),
                "pm_public_message" => on_pm_public_message(&socket, reader, dispatch),
                "spot_public_message" => on_spot_public_message(&socket, reader, dispatch),
                _ => panic!("unknown lane"),
            };
            if result.is_err() {
                break;
            }
        }
        let expected = if name.starts_with("read-fail")
            || ["disposed", "read-disposes", "health-fail"].contains(&name)
        {
            0
        } else if name == "repeat" {
            2
        } else {
            1
        };
        assert_eq!(
            dispatched.get(),
            expected,
            "MESSAGE {lane} native dispatch count case{index}"
        );
        assert_eq!(
            reads.get(),
            if name == "repeat" { 2 } else { 1 },
            "MESSAGE native data read count"
        );
        assert_health(lane, index, name, &socket, &parent, &runtime);
        let (failure, error_name, error_message) = error_fields(&result, &error, name);
        assert_eq!(
            result.is_err(),
            name.starts_with("read-fail") || ["health-fail", "dispatch-fail"].contains(&name),
            "MESSAGE native public error floor"
        );
        if name == "dispatch-mutates" {
            assert_eq!(
                value.as_ref().unwrap().borrow()["q"].as_str(),
                Some("4"),
                "MESSAGE native mutation retained"
            );
        }
        native.push(json!({"lane":lane,"name":name,"trace":*trace.borrow(),"calls":[],"reads":reads.get(),"dispatched":dispatched.get(),"health":parent.health.get(),"timers":runtime.timers.get(),"disposed":parent.disposed.get(),"initialized":socket.diagnostics().initialized,"last":socket.diagnostics().last_message_timestamp,"failure":failure,"errorName":error_name,"errorMessage":error_message,"stats":[],"data":value.as_ref().map(|v|v.borrow().clone())}));
    }
    compare(lane, native, 14);
}
struct Methods<'a> {
    lane: &'a str,
    index: usize,
    name: &'static str,
    trace: Trace,
    parent: Rc<Parent>,
    socket: Rc<BaseWebSocket>,
    error: Failure,
    value: Option<MessageData>,
    calls: Vec<Value>,
    previous: Option<MessageData>,
}
impl Methods<'_> {
    fn handle(&mut self, kind: &'static str, events: Vec<MessageData>) -> Result<(), Failure> {
        self.trace.borrow_mut().push(kind);
        assert_eq!(
            events.len(),
            1,
            "MESSAGE {} native payload count case{}",
            self.lane,
            self.index
        );
        let payload = events[0].clone();
        if let Some(previous) = self.previous.as_ref() {
            assert!(
                Rc::ptr_eq(previous, &payload),
                "MESSAGE native repeated JSON referent"
            );
        }
        self.previous = Some(payload.clone());
        let row = payload.borrow();
        assert_eq!(
            row["o"]["q"].as_str(),
            Some(if self.name == "account-mutates" && kind == "order" {
                "4"
            } else {
                "3.6"
            }),
            "MESSAGE {} native decoded quantity case{}",
            self.lane,
            self.index
        );
        assert_eq!(
            row["o"]["p"].as_str(),
            Some(
                if self.name == "order-mutates" && ["algo", "conditional"].contains(&kind) {
                    "14.2"
                } else {
                    "12.3"
                }
            ),
            "MESSAGE {} native decoded price case{}",
            self.lane,
            self.index
        );
        if self.name == "big-id" {
            if self.lane.starts_with("futures") {
                assert_eq!(
                    row["o"]["i"].as_str(),
                    Some("9007199254740993"),
                    "MESSAGE native bigint string ID"
                );
            } else {
                assert_eq!(
                    row["o"]["i"].as_f64(),
                    Some(9007199254740992.0),
                    "MESSAGE native PM rounded ID"
                );
            }
        } else if self.name == "string-corruption" {
            assert_eq!(
                row["o"]["i"].as_str(),
                Some("x:1234567890123456,"),
                "MESSAGE native uncorrupted PM ID"
            );
        } else {
            assert_eq!(
                row["o"]["i"].as_str(),
                Some("small-id"),
                "MESSAGE native original ID"
            );
        }
        let topic = match kind {
            "account" => "position",
            "order" => "order",
            "algo" => "algo_order",
            "conditional" => "conditional-order",
            _ => panic!("unknown handler"),
        };
        let stats = self.socket.message_handlers_stats.borrow();
        let stat = stats.get(topic).expect("MESSAGE stats before callback");
        assert_eq!(
            stat.last_message_timestamp, 1250.0,
            "MESSAGE native stats time before callback"
        );
        assert_eq!(
            stat.last_callbacks_count,
            safe_cex_num::PrototypeScalar::from(Some(1)),
            "MESSAGE native stats count before callback"
        );
        drop(stats);
        let identity = if self.value.as_ref().is_some_and(|v| v.borrow().is_object()) {
            let same = Rc::ptr_eq(self.value.as_ref().unwrap(), &payload);
            assert!(same, "MESSAGE native non-string input identity");
            same
        } else {
            true
        };
        self.calls
            .push(json!({"kind":kind,"payload":row.clone(),"identity":identity}));
        drop(row);
        if self.name == format!("{kind}-fail")
            || (self.name == "special-fail" && ["algo", "conditional"].contains(&kind))
        {
            return Err(self.error.clone());
        }
        if self.name == "account-mutates" && kind == "account" {
            let mut row = payload.borrow_mut();
            row["e"] = json!("ORDER_TRADE_UPDATE");
            row["o"]["q"] = json!("4");
        }
        if self.name == "order-mutates" && kind == "order" {
            let mut row = payload.borrow_mut();
            row["e"] = json!(if self.lane.starts_with("futures") {
                "ALGO_UPDATE"
            } else {
                "CONDITIONAL_ORDER_TRADE_UPDATE"
            });
            row["o"]["p"] = json!("14.2");
        }
        if self.name == "callback-disposes" {
            self.parent.disposed.set(true);
        }
        Ok(())
    }
}
impl PrivateMessageMethods for Methods<'_> {
    fn diagnostic(&mut self, received: Option<&MessageData>) -> Result<(), Failure> {
        self.trace.borrow_mut().push("diagnostic");
        assert_eq!(
            received.is_some(),
            self.value.is_some(),
            "MESSAGE native diagnostic undefined"
        );
        if let (Some(received), Some(value)) = (received, self.value.as_ref()) {
            assert!(
                Rc::ptr_eq(received, value),
                "MESSAGE native original diagnostic data"
            );
        }
        if self.name == "diagnostic-disposes" {
            self.parent.disposed.set(true);
        }
        if self.name == "diagnostic-mutates" {
            received.unwrap().borrow_mut()["e"] = json!("ORDER_TRADE_UPDATE");
        }
        if self.name == "diagnostic-fail" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn account(&mut self, events: Vec<MessageData>) -> Result<(), Failure> {
        self.handle("account", events)
    }
    fn order(&mut self, events: Vec<MessageData>) -> Result<(), Failure> {
        self.handle("order", events)
    }
    fn algo_order(&mut self, events: Vec<MessageData>) -> Result<(), Failure> {
        self.handle("algo", events)
    }
    fn conditional_order(&mut self, events: Vec<MessageData>) -> Result<(), Failure> {
        self.handle("conditional", events)
    }
    fn pong(&mut self) -> Result<(), Failure> {
        self.trace.borrow_mut().push("pong");
        self.calls.push(json!({"kind":"pong"}));
        if self.name == "pong-fail" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
}
fn expected_calls(name: &str, futures: bool) -> Vec<&'static str> {
    let special = if futures { "algo" } else { "conditional" };
    match name {
        "disposed" | "read-fail" | "health-fail" | "read-disposes" | "diagnostic-fail"
        | "invalid" | "null" | "array" | "primitive" | "id-string" | "unknown" | "missing-data" => {
            vec![]
        }
        "object-data" => {
            if futures {
                vec!["account"]
            } else {
                vec![]
            }
        }
        "array-data" => {
            if futures {
                vec![]
            } else {
                vec!["account"]
            }
        }
        "coercion-fail" => vec![],
        "string-corruption" => {
            if futures {
                vec![]
            } else {
                vec!["order"]
            }
        }
        "diagnostic-mutates" => {
            if futures {
                vec!["order", "pong"]
            } else {
                vec![]
            }
        }
        "account-mutates" => vec!["account", "order", "pong"],
        "order-mutates" => vec!["order", special, "pong"],
        "account-pong" => vec!["account", "pong"],
        "order-pong" => vec!["order", "pong"],
        "special-pong" => vec![special, "pong"],
        "order" | "order-fail" | "big-id" => vec!["order"],
        "special" | "special-fail" => vec![special],
        "pong" | "pong-fail" => vec!["pong"],
        _ => vec!["account"],
    }
}
fn private(lane: &str) {
    let futures = lane.starts_with("futures");
    let mut native = vec![];
    for (index, &name) in PRIVATE_CASES.iter().enumerate() {
        let (socket, parent, runtime, trace, error) = fixture(name);
        let reads = Cell::new(0);
        let value = input(name, futures);
        let mut methods = Methods {
            lane,
            index,
            name,
            trace: trace.clone(),
            parent: parent.clone(),
            socket: socket.clone(),
            error: error.clone(),
            value: value.clone(),
            calls: vec![],
            previous: None,
        };
        let reader = || read(name, &parent, &trace, &reads, &value, &error);
        let result = if futures {
            on_futures_private_message(&socket, reader, &mut methods)
        } else {
            on_pm_private_message(&socket, reader, &mut methods)
        };
        let expected = expected_calls(name, futures);
        let kinds: Vec<_> = methods
            .calls
            .iter()
            .map(|c| c["kind"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds.len(),
            expected.len(),
            "MESSAGE {lane} native callback count case{index}"
        );
        assert_eq!(
            kinds, expected,
            "MESSAGE {lane} native callback sequence case{index}"
        );
        assert_eq!(reads.get(), 1, "MESSAGE native one data read");
        assert_health(lane, index, name, &socket, &parent, &runtime);
        let (failure, error_name, error_message) = error_fields(&result, &error, name);
        assert_eq!(
            result.is_err(),
            [
                "read-fail",
                "health-fail",
                "diagnostic-fail",
                "account-fail",
                "order-fail",
                "special-fail",
                "pong-fail"
            ]
            .contains(&name),
            "MESSAGE native private error floor"
        );
        let mut topics: Vec<_> = socket
            .message_handlers_stats
            .borrow()
            .keys()
            .cloned()
            .collect();
        topics.sort();
        let expected_topics: Vec<_> = expected
            .iter()
            .filter(|k| **k != "pong")
            .map(|k| match *k {
                "account" => "position",
                "order" => "order",
                "algo" => "algo_order",
                _ => "conditional-order",
            })
            .collect();
        assert_eq!(
            topics.len(),
            expected_topics.len(),
            "MESSAGE native independent stats count"
        );
        let stats: Vec<_> = topics
            .iter()
            .map(|topic| {
                let map = socket.message_handlers_stats.borrow();
                let s = &map[topic];
                assert_eq!(
                    s.last_callbacks_count,
                    safe_cex_num::PrototypeScalar::from(Some(1))
                );
                assert_eq!(s.had_handlers, Some(false));
                json!([
                    topic,
                    s.last_message_timestamp,
                    s.last_callbacks_count,
                    s.had_handlers
                ])
            })
            .collect();
        native.push(json!({"lane":lane,"name":name,"trace":*trace.borrow(),"calls":methods.calls,"reads":reads.get(),"dispatched":0,"health":parent.health.get(),"timers":runtime.timers.get(),"disposed":parent.disposed.get(),"initialized":socket.diagnostics().initialized,"last":socket.diagnostics().last_message_timestamp,"failure":failure,"errorName":error_name,"errorMessage":error_message,"stats":stats,"data":value.as_ref().map(|v|v.borrow().clone())}));
    }
    compare(lane, native, 36);
}
type Callback = Rc<dyn Fn(Rc<RefCell<Candle>>) -> Result<(), Failure>>;
type Cleanup = Rc<dyn Fn() -> Result<(), Failure>>;
fn delegate(lane: &str) {
    let mut native = vec![];
    for (index, &name) in DELEGATE_CASES.iter().enumerate() {
        let opts: CandleOptionsHandle = Rc::new(RefCell::new(
            serde_json::from_value(
                json!({"symbol":"BTCUSDT","interval":"1m","from":100,"to":200,"limit":7}),
            )
            .unwrap(),
        ));
        let candle = Rc::new(RefCell::new(
            serde_json::from_value::<Candle>(
                json!({"timestamp":1000,"open":11,"high":22,"low":3,"close":12.3,"volume":3.6}),
            )
            .unwrap(),
        ));
        let error = failure("delegate fixture");
        let trace: Trace = Rc::new(RefCell::new(vec![]));
        let listens = Cell::new(0);
        let callbacks = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));
        let (t, n, p, e) = (
            trace.clone(),
            callbacks.clone(),
            candle.clone(),
            error.clone(),
        );
        let callback: Callback = Rc::new(move |c| {
            t.borrow_mut().push("callback");
            n.set(n.get() + 1);
            assert!(
                Rc::ptr_eq(&c, &p),
                "MESSAGE native original candle identity"
            );
            assert_eq!(
                c.borrow().close,
                12.3,
                "MESSAGE native decoded candle close"
            );
            assert_eq!(
                c.borrow().volume,
                3.6,
                "MESSAGE native decoded candle volume"
            );
            c.borrow_mut().close = 20.0;
            if name == "callback-fail" {
                Err(e.clone())
            } else {
                Ok(())
            }
        });
        let (t, n, e) = (trace.clone(), cleanups.clone(), error.clone());
        let cleanup: Cleanup = Rc::new(move || {
            t.borrow_mut().push("cleanup");
            n.set(n.get() + 1);
            if name == "cleanup-fail" {
                Err(e.clone())
            } else {
                Ok(())
            }
        });
        let listen = |o: CandleOptionsHandle, c: Callback| {
            trace.borrow_mut().push("listen");
            listens.set(listens.get() + 1);
            assert!(
                Rc::ptr_eq(&o, &opts),
                "MESSAGE native original options identity"
            );
            assert!(
                Rc::ptr_eq(&c, &callback),
                "MESSAGE native original callback identity"
            );
            assert_eq!(
                o.borrow().limit,
                Some(7.0),
                "MESSAGE {lane} native options limit case{index}"
            );
            assert_eq!(o.borrow().symbol, "BTCUSDT");
            if name == "mutate-options" {
                o.borrow_mut().limit = Some(9.0);
            }
            if name == "listen-fail" {
                return Err(error.clone());
            }
            c(candle.clone())?;
            Ok(cleanup.clone())
        };
        let result = if lane.starts_with("futures") {
            listen_futures_public_ohlcv(opts.clone(), callback.clone(), listen)
        } else {
            listen_pm_public_ohlcv(opts.clone(), callback.clone(), listen)
        };
        let failed = result.is_err();
        let same_cleanup = result.as_ref().is_ok_and(|r| Rc::ptr_eq(r, &cleanup));
        let mut cleanup_failure = None;
        if let Ok(returned) = result.as_ref() {
            assert!(same_cleanup, "MESSAGE native original cleanup identity");
            if let Err(e) = returned() {
                assert!(Rc::ptr_eq(&e, &error));
                cleanup_failure = Some("original");
            } else if name == "repeat-cleanup" {
                returned().unwrap();
            }
        } else {
            assert!(
                Rc::ptr_eq(result.as_ref().err().unwrap(), &error),
                "MESSAGE native original delegate error identity"
            );
        }
        assert_eq!(listens.get(), 1, "MESSAGE native one delegate call");
        assert_eq!(callbacks.get(), usize::from(name != "listen-fail"));
        assert_eq!(
            cleanups.get(),
            if ["listen-fail", "callback-fail"].contains(&name) {
                0
            } else if name == "repeat-cleanup" {
                2
            } else {
                1
            }
        );
        assert_eq!(
            candle.borrow().close,
            if name == "listen-fail" { 12.3 } else { 20.0 },
            "MESSAGE native retained candle mutation"
        );
        assert_eq!(
            opts.borrow().limit,
            Some(if name == "mutate-options" { 9.0 } else { 7.0 })
        );
        native.push(json!({"lane":lane,"name":name,"trace":*trace.borrow(),"listens":listens.get(),"callbacks":callbacks.get(),"cleanups":cleanups.get(),"failure":failed.then_some("original"),"cleanupFailure":cleanup_failure,"sameCleanup":same_cleanup,"opts":*opts.borrow(),"candle":*candle.borrow()}));
    }
    compare(lane, native, 6);
}
macro_rules! source_test {
    ($name:ident,$run:ident) => {
        #[test]
        #[ignore = "requires live TypeScript source oracle"]
        fn $name() {
            $run(stringify!($name));
        }
    };
}
source_test!(futures_market_message, public);
source_test!(futures_public_message, public);
source_test!(pm_market_message, public);
source_test!(pm_public_message, public);
source_test!(spot_public_message, public);
source_test!(futures_private_message, private);
source_test!(pm_private_message, private);
source_test!(futures_candle_delegate, delegate);
source_test!(pm_candle_delegate, delegate);

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
