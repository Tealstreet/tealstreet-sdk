use safe_cex_exchange::{runtime::*, websocket::*};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::{
    TransportProvider,
    socket::{SocketFrame, TunnelHost},
};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_binance::public_connect::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;
struct Slot {
    future: RefCell<Option<LocalFuture>>,
    cancelled: Cell<bool>,
}
impl Slot {
    fn poll(&self) {
        let Some(mut future) = self.future.borrow_mut().take() else {
            return;
        };
        if !self.cancelled.get()
            && future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            && !self.cancelled.get()
        {
            *self.future.borrow_mut() = Some(future);
        }
    }
}
struct Task(Rc<Slot>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.cancelled.set(true);
        let future = self.0.future.borrow_mut().take();
        drop(future);
    }
}
#[derive(Default)]
struct Socket {
    events: Rc<RefCell<VecDeque<WsEvent>>>,
    closed: Cell<usize>,
    lifecycle: RefCell<Vec<&'static str>>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        let events = self.events.clone();
        Box::pin(poll_fn(move |_| {
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
        self.lifecycle.borrow_mut().push("close");
        self.closed.set(self.closed.get() + 1);
    }
    fn terminate(&self) {
        self.lifecycle.borrow_mut().push("terminate");
    }
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        true
    }
}
struct Runtime {
    now: Cell<f64>,
    inline: bool,
    pending_dial: bool,
    tasks: RefCell<Vec<Rc<Slot>>>,
    trace: Rc<RefCell<Vec<Value>>>,
    native: Rc<Socket>,
    dial_drops: Rc<Cell<usize>>,
}
impl Runtime {
    fn pump(&self) {
        for _ in 0..4 {
            let tasks = self.tasks.borrow().clone();
            for task in tasks {
                task.poll();
            }
            self.tasks
                .borrow_mut()
                .retain(|slot| slot.future.borrow().is_some());
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, future: LocalFuture) -> Box<dyn ExchangeTask> {
        let slot = Rc::new(Slot {
            future: RefCell::new(Some(future)),
            cancelled: Cell::new(false),
        });
        self.tasks.borrow_mut().push(slot.clone());
        if self.inline {
            slot.poll();
        }
        Box::new(Task(slot))
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
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        panic!("unexpected exception {error}");
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker");
    }
}
struct DialDrop(Rc<Cell<usize>>);
impl Drop for DialDrop {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("unexpected tunnel");
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        assert!(protocols.is_empty());
        assert_eq!(
            options.max_reconnection_delay_ms,
            Some(5000.0),
            "source omitted options retain SDK tuning"
        );
        self.trace.borrow_mut().push(json!(["create", url]));
        let native: Rc<dyn WsSocket> = self.native.clone();
        let pending = self.pending_dial;
        let guard = DialDrop(self.dial_drops.clone());
        Box::pin(async move {
            let _guard = guard;
            if pending {
                std::future::pending::<()>().await;
            }
            Ok(native)
        })
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
        2500.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Parent {
    options_reads: Cell<usize>,
    disposed: Cell<bool>,
    options: ExchangeOptions,
    store: Rc<DefaultStore>,
    delivered: Rc<RefCell<Vec<&'static str>>>,
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "binance"
    }
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn options(&self) -> ExchangeOptions {
        self.options_reads.set(self.options_reads.get() + 1);
        self.options.clone()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        false
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        Rc::new(ReopenResync::default())
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        panic!("unexpected snapshot");
    }
    fn log(&self, message: &str, _: LogSeverity) -> Result<(), ExchangeError> {
        if message.starts_with("WebSocket connection disconnected") {
            self.delivered.borrow_mut().push("close");
        }
        Ok(())
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
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
        error: &str,
        _: Option<&str>,
        _: Option<&Value>,
    ) -> Result<(), ExchangeError> {
        panic!("unexpected socket error: {error}")
    }
}
struct Protocol(Rc<RefCell<Vec<&'static str>>>);
impl WsProtocol for Protocol {
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        self.0.borrow_mut().push("open");
        Box::pin(async { Ok(()) })
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, _: &SocketFrame) -> Result<(), ExchangeError> {
        self.0.borrow_mut().push("message");
        Ok(())
    }
}
fn setup(
    testnet: Option<bool>,
    disposed: bool,
    inline: bool,
    pending_dial: bool,
) -> (Rc<BaseWebSocket>, Rc<Parent>, Rc<Runtime>) {
    let delivered = Rc::new(RefCell::new(vec![]));
    let mut options: ExchangeOptions =
        serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap();
    options.testnet = testnet;
    let parent = Rc::new(Parent {
        options_reads: Cell::new(0),
        disposed: Cell::new(disposed),
        options,
        store: Rc::new(DefaultStore::new()),
        delivered: delivered.clone(),
    });
    let runtime = Rc::new(Runtime {
        now: Cell::new(1000.0),
        inline,
        pending_dial,
        tasks: RefCell::new(vec![]),
        trace: Default::default(),
        native: Default::default(),
        dial_drops: Rc::new(Cell::new(0)),
    });
    let socket = BaseWebSocket::new(
        1,
        parent.clone(),
        runtime.clone(),
        Rc::new(Protocol(delivered)),
    );
    (socket, parent, runtime)
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_connect_matches_source_through_owned_core() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-connect.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 42);
    for (index, case) in cases.iter().enumerate() {
        let (socket, parent, runtime) = setup(
            case["testnet"].as_bool(),
            case["disposed"] == true,
            true,
            false,
        );
        let mut sentinel = ExchangeError::new("fixture market connect");
        sentinel.name = "RangeError".into();
        sentinel.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "market-code"
        )));
        let market = || {
            runtime.trace.borrow_mut().push(json!(["market"]));
            if case["childFails"] == true {
                Err(sentinel.clone())
            } else {
                Ok(())
            }
        };
        let result = match (
            case["variant"].as_str().unwrap(),
            case["name"].as_str().unwrap(),
        ) {
            ("binance", "BinanceMarketWebsocket") => connect_binance_market(&socket),
            ("binancepm", "BinancePMMarketWebsocket") => connect_binance_pm_market(&socket),
            ("binancespot", _) => connect_binance_spot_public(&socket),
            ("binance", _) => connect_binance_public(&socket, market),
            ("binancepm", _) => connect_binance_pm_public(&socket, market),
            _ => panic!("unknown fixture"),
        };
        assert_eq!(
            json!(parent.options_reads.get()),
            case["optionsReads"],
            "public connect disposed guard precedes options read case{index}"
        );
        let error=result.err().map(|error|json!({"same":error==sentinel,"name":error.name,"message":error.message,"code":observed_error_code(&error.code)}));
        assert_eq!(
            json!({"trace":*runtime.trace.borrow(),"error":error}),
            json!({"trace":case["trace"],"error":case["error"]}),
            "public connect order, URL and error case{index}"
        );
        if case["disposed"] != true {
            for event in [
                WsEvent::Open,
                WsEvent::Message(SocketFrame::Text("fixture".into())),
                WsEvent::Close {
                    code: Some(1000),
                    reason: "fixture".into(),
                },
            ] {
                runtime.native.events.borrow_mut().push_back(event);
                runtime.pump();
            }
        }
        assert_eq!(
            json!(*parent.delivered.borrow()),
            case["listeners"],
            "public connect core open/message/close delivery case{index}"
        );
        parent.disposed.set(true);
        socket.dispose();
        runtime.pump();
    }
}

#[test]
fn public_connect_disposal_cancels_unpolled_and_pending_dial() {
    for inline in [false, true] {
        let (socket, parent, runtime) = setup(Some(false), false, inline, true);
        connect_binance_market(&socket).unwrap();
        let registered = runtime.tasks.borrow().clone();
        assert_eq!(
            registered.len(),
            1,
            "CANCELLATION-COVERAGE registered task count inline={inline}"
        );
        let task = &registered[0];
        assert!(
            task.future.borrow().is_some(),
            "CANCELLATION-COVERAGE registered future exists inline={inline}"
        );
        assert_eq!(runtime.trace.borrow().len(), usize::from(inline));
        parent.disposed.set(true);
        socket.dispose();
        assert!(
            task.cancelled.get(),
            "CANCELLATION-COVERAGE retained task cancelled inline={inline}"
        );
        assert!(
            task.future.borrow().is_none(),
            "CANCELLATION-COVERAGE retained future released inline={inline}"
        );
        runtime.pump();
        assert!(
            task.cancelled.get() && task.future.borrow().is_none(),
            "CANCELLATION-COVERAGE retained task stays cancelled after pump inline={inline}"
        );
        assert_eq!(
            runtime.trace.borrow().len(),
            usize::from(inline),
            "no late dial after disposal"
        );
        assert_eq!(
            runtime.dial_drops.get(),
            usize::from(inline),
            "pending dial future released by core disposal"
        );
        assert!(
            runtime
                .tasks
                .borrow()
                .iter()
                .all(|task| task.future.borrow().is_none()),
            "core leaves no pending venue task"
        );
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_dispose_matches_source_child_before_base() {
    use safe_cex_venue_binance::public_dispose::*;
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-dispose.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 16);
    let source_attempts: usize = cases
        .iter()
        .map(|case| case["attempts"].as_array().unwrap().len())
        .sum();
    assert_eq!(
        source_attempts, 32,
        "DISPOSE-CARDINALITY source total attempts"
    );
    let mut native_comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let (socket, parent, runtime) = setup(None, false, true, false);
        if case["connected"] == true {
            connect_binance_market(&socket).unwrap();
        }
        parent.disposed.set(case["disposed"] == true);
        let mut sentinel = ExchangeError::new("fixture market disposal");
        sentinel.name = "RangeError".into();
        sentinel.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "dispose-code"
        )));
        let attempts = case["attempts"].as_array().unwrap();
        assert_eq!(
            attempts.len(),
            2,
            "DISPOSE-CARDINALITY source attempts case {index}"
        );
        for attempt in 0..2 {
            let expected = attempts
                .get(attempt)
                .expect("DISPOSE-CARDINALITY expected/native attempt coverage");
            let child = || {
                runtime.native.lifecycle.borrow_mut().push("market");
                if case["childFails"] == true {
                    Err(sentinel.clone())
                } else {
                    Ok(())
                }
            };
            let result = if case["variant"] == "binance" {
                dispose_binance_public(&socket, child)
            } else {
                dispose_binance_pm_public(&socket, child)
            };
            let error = result.err().map(
                |e| json!({"same":e==sentinel,"name":e.name,"message":e.message,"code":observed_error_code(&e.code)}),
            );
            assert_eq!(
                json!(error),
                expected["error"],
                "case{index} attempt{attempt} error identity"
            );
            assert_eq!(
                json!(*runtime.native.lifecycle.borrow()),
                expected["trace"],
                "case{index} attempt{attempt} child-before-base trace"
            );
            assert_eq!(
                json!(socket.is_force_closed()),
                expected["forceClosed"],
                "case{index} attempt{attempt} force-closed state"
            );
            native_comparisons += 1;
            if case["childFails"] == false {
                runtime.pump();
                assert!(
                    runtime
                        .tasks
                        .borrow()
                        .iter()
                        .all(|task| task.future.borrow().is_none()),
                    "case{index} disposal owns cancellation"
                );
            }
        }
        parent.disposed.set(true);
        socket.dispose();
        runtime.pump();
    }
    assert_eq!(
        native_comparisons, 32,
        "DISPOSE-CARDINALITY native comparisons"
    );
}

#[test]
fn public_dispose_child_failure_retains_pending_parent_until_success() {
    use safe_cex_venue_binance::public_dispose::*;
    for pm in [false, true] {
        let (socket, parent, runtime) = setup(None, false, true, true);
        connect_binance_market(&socket).unwrap();
        let dispose = if pm {
            dispose_binance_pm_public
        } else {
            dispose_binance_public
        };
        let failure: fn() -> Result<(), ExchangeError> =
            || Err(ExchangeError::new("child failure"));
        assert!(dispose(&socket, failure).is_err());
        assert_eq!(
            runtime.dial_drops.get(),
            0,
            "child failure must retain parent dial"
        );
        assert!(!socket.is_force_closed());
        let success: fn() -> Result<(), ExchangeError> = || Ok(());
        dispose(&socket, success).unwrap();
        assert_eq!(
            runtime.dial_drops.get(),
            1,
            "successful retry cancels parent dial once"
        );
        assert!(socket.is_force_closed());
        parent.disposed.set(true);
        runtime.pump();
        assert_eq!(runtime.trace.borrow().len(), 1, "no later dial");
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_stats_native_fields_match_actual_source() {
    use safe_cex_num::{BuiltinIdentity, ProtoMember, PrototypeRealm, PrototypeValue};
    use safe_cex_venue_binance::public_stats::record_binance_handler_stats;
    // An independent fixture table; no source output determines its size/inputs.
    const BUILTINS: [(&str, Option<usize>, ProtoMember); 12] = [
        ("constructor", Some(1), ProtoMember::Constructor),
        ("__defineGetter__", Some(2), ProtoMember::DefineGetter),
        ("__defineSetter__", Some(2), ProtoMember::DefineSetter),
        ("hasOwnProperty", Some(1), ProtoMember::HasOwnProperty),
        ("__lookupGetter__", Some(1), ProtoMember::LookupGetter),
        ("__lookupSetter__", Some(1), ProtoMember::LookupSetter),
        ("isPrototypeOf", Some(1), ProtoMember::IsPrototypeOf),
        (
            "propertyIsEnumerable",
            Some(1),
            ProtoMember::PropertyIsEnumerable,
        ),
        ("toString", Some(0), ProtoMember::ToString),
        ("valueOf", Some(0), ProtoMember::ValueOf),
        ("__proto__", None, ProtoMember::Prototype),
        ("toLocaleString", Some(0), ProtoMember::ToLocaleString),
    ];
    let wrappers = [
        ("binance", "BinanceMarketWebsocket"),
        ("binance", "BinancePublicWebsocket"),
        ("binancepm", "BinancePMMarketWebsocket"),
        ("binancepm", "BinancePMPublicWebsocket"),
        ("binancespot", "BinancePublicWebsocket"),
    ];
    let topics: [&str; 16] = [
        "book.BTC",
        "publicTrade.SOLUSDT",
        "publicTrade.BTCUSDT",
        "publicTrade.ETHUSDT",
        "constructor",
        "__defineGetter__",
        "__defineSetter__",
        "hasOwnProperty",
        "__lookupGetter__",
        "__lookupSetter__",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toString",
        "valueOf",
        "__proto__",
        "toLocaleString",
    ];
    let mut native = vec![];
    for (variant, class_name) in wrappers {
        for topic in topics {
            for own_count in [None, Some(0), Some(1), Some(2)] {
                for own_stats in [false, true] {
                    for time in [1000.0, 6000.0] {
                        let realm = Rc::<PrototypeRealm>::default();
                        let _binding = realm.bind();
                        let (socket, _, runtime) = setup(None, true, false, false);
                        runtime.now.set(time);
                        if let Some(count) = own_count {
                            let callbacks: Vec<MessageHandler> = (0..count)
                                .map(|_| Rc::new(|_: &Value| Ok(())) as MessageHandler)
                                .collect();
                            socket
                                .message_handlers
                                .borrow_mut()
                                .insert(topic.into(), Rc::new(RefCell::new(callbacks)));
                        }
                        if own_stats {
                            socket.message_handlers_stats.borrow_mut().insert(
                                topic.into(),
                                MessageHandlerStat {
                                    last_message_timestamp: 10.0,
                                    last_callbacks_count: safe_cex_num::PrototypeScalar::from(
                                        Some(0),
                                    ),
                                    had_handlers: Some(false),
                                    had_handlers_undefined: false,
                                },
                            );
                        }
                        let builtin = BUILTINS.iter().find(|(name, _, _)| *name == topic);
                        let count = own_count.map(Some).unwrap_or_else(|| {
                            builtin.map(|(_, arity, _)| *arity).unwrap_or(Some(1))
                        });
                        let has = own_count.map(|n| n != 0).unwrap_or_else(|| {
                            builtin.is_some_and(|(_, arity, _)| arity.is_some_and(|n| n != 0))
                        });
                        let skipped = topic == "publicTrade.SOLUSDT";
                        let throws = topic == "__proto__"
                            && !own_stats
                            && count != Some(0)
                            && time == 6000.0;
                        let result = record_binance_handler_stats(&socket, topic);
                        assert_eq!(
                            result.is_err(),
                            throws,
                            "native inherited reconciliation failure at authored topic/time/count"
                        );
                        if let Err(error) = &result {
                            assert_eq!(
                                error.name, "TypeError",
                                "native prototype reconciliation error name"
                            );
                            let expected = if count.is_none() {
                                "Cannot read properties of undefined (reading 'hadHandlers')".into()
                            } else {
                                format!(
                                    "Cannot create property 'lastCallbacksCount' on number '{}'",
                                    safe_cex_num::js_number_string(time)
                                )
                            };
                            assert_eq!(
                                error.message, expected,
                                "native prototype reconciliation error message"
                            );
                        }
                        let own_expected = own_stats || (!skipped && builtin.is_none());
                        assert_eq!(
                            socket.message_handlers_stats.borrow().len(),
                            usize::from(own_expected),
                            "native independent own statistics count"
                        );
                        if own_expected {
                            let stats = socket.message_handlers_stats.borrow();
                            let entry = stats.get(topic).expect("native expected own topic exists");
                            assert_eq!(
                                entry.last_message_timestamp,
                                if skipped { 10.0 } else { time },
                                "native own timestamp before wire"
                            );
                            assert_eq!(
                                entry.last_callbacks_count,
                                safe_cex_num::PrototypeScalar::from(if skipped {
                                    Some(0)
                                } else {
                                    count
                                }),
                                "native own callback count before wire"
                            );
                            assert_eq!(
                                entry.had_handlers,
                                Some(!skipped && has),
                                "native own handler presence before wire"
                            );
                        }
                        if let Some(&(_, _, expected_member)) = builtin.filter(|_| !own_stats) {
                            let PrototypeValue::Reference(hit) = realm.inherited(topic) else {
                                panic!("native inherited member remains a referent")
                            };
                            assert_eq!(
                                hit.member(),
                                BuiltinIdentity::Object(expected_member),
                                "native exact inherited builtin identity before wire"
                            );
                            assert_eq!(
                                matches!(
                                    hit.member(),
                                    BuiltinIdentity::Object(ProtoMember::Prototype)
                                ),
                                topic == "__proto__",
                                "native inherited object/callable kind before wire"
                            );
                            assert!(
                                matches!(hit.get("lastMessageTimestamp"),PrototypeValue::Number(n) if n==time),
                                "native inherited referent receives timestamp before wire"
                            );
                            assert!(
                                match (hit.get("lastCallbacksCount"), count) {
                                    (PrototypeValue::Number(n), Some(expected)) =>
                                        n == expected as f64,
                                    (PrototypeValue::Undefined, None) => true,
                                    _ => false,
                                },
                                "native inherited referent receives callback count before wire"
                            );
                            assert!(
                                match hit.get("hadHandlers") {
                                    PrototypeValue::Boolean(true) => has,
                                    PrototypeValue::Undefined => !has,
                                    _ => false,
                                },
                                "native inherited handler flag before wire"
                            );
                            assert_eq!(
                                realm.enumerable_keys().is_empty(),
                                topic != "__proto__",
                                "native prototype mutation is shared only for prototype-object topic"
                            );
                        }
                        if builtin.is_none() || own_stats || skipped {
                            for (_, _, member) in BUILTINS {
                                assert!(
                                    realm.reference(member).own_fields().is_empty(),
                                    "native own shadow/ordinary topic leaves every inherited referent untouched"
                                );
                            }
                            assert!(
                                realm.enumerable_keys().is_empty(),
                                "native own shadow/ordinary topic leaves Object.prototype untouched"
                            );
                        }
                        assert!(
                            runtime.tasks.borrow().is_empty(),
                            "native statistics creates no background jobs"
                        );
                        let stats: serde_json::Map<String, Value> = socket
                            .message_handlers_stats
                            .borrow()
                            .iter()
                            .map(|(key, entry)| {
                                let mut fields = serde_json::Map::new();
                                fields.insert(
                                    "lastMessageTimestamp".into(),
                                    json!(entry.last_message_timestamp),
                                );
                                if !matches!(
                                    &entry.last_callbacks_count,
                                    safe_cex_num::PrototypeScalar::Undefined
                                ) {
                                    fields.insert(
                                        "lastCallbacksCount".into(),
                                        json!(&entry.last_callbacks_count),
                                    );
                                }
                                if let Some(v) = entry.had_handlers {
                                    fields.insert("hadHandlers".into(), json!(v));
                                }
                                (key.clone(), Value::Object(fields))
                            })
                            .collect();
                        native.push(json!({"variant":variant,"className":class_name,"topic":topic,"count":own_count,"ownStats":own_stats,"time":time,"stats":stats,"error":result.err().map(|e|json!({"name":e.name,"message":e.message}))}));
                    }
                }
            }
        }
    }
    assert_eq!(
        native.len(),
        1280,
        "independent five wrappers by sixteen topics by four counts by two own states by two times"
    );
    // Every native field assertion completed before starting the source oracle.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-stats.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 1280, "authored source case floor");
    for (index, (native, source)) in native.iter().zip(source).enumerate() {
        for key in [
            "variant",
            "className",
            "topic",
            "count",
            "ownStats",
            "time",
            "stats",
            "error",
        ] {
            let mut expected = source[key].clone();
            if key == "time" {
                expected = json!(expected.as_f64().unwrap());
            }
            if key == "stats" {
                for stat in expected.as_object_mut().unwrap().values_mut() {
                    if let Some(time) = stat.get_mut("lastMessageTimestamp") {
                        *time = json!(time.as_f64().unwrap());
                    }
                }
            }
            assert_eq!(native[key], expected, "source stats {key} case{index}");
        }
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_stats_source_forwarding_control() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-stats.mjs"
        ))
        .arg("--skip-wrapper")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("wrapper reaches actual core stats"));
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn prototype_statistics_sequences_preserve_native_values_and_realms_before_source() {
    use safe_cex_num::{ProtoMember, PrototypeRealm, PrototypeScalar, PrototypeValue};
    use safe_cex_venue_binance::{
        order_history_native::map_binance_historical_order_native,
        private_event_value::{EventOps, EventValue, PrivateEnum},
        public_stats::record_binance_handler_stats,
    };
    fn number(value: PrototypeValue, expected: f64, label: &str) {
        assert!(
            matches!(value,PrototypeValue::Number(n) if n==expected),
            "{label}: native numeric value"
        );
    }
    fn fields(reference: &safe_cex_num::BuiltinReference) -> Value {
        Value::Object(
            reference
                .own_fields()
                .into_iter()
                .map(|(key, value)| {
                    (
                        key,
                        match value {
                            PrototypeScalar::Undefined => json!("[undefined]"),
                            PrototypeScalar::Null => Value::Null,
                            PrototypeScalar::Number(n) => json!(n),
                            PrototypeScalar::Boolean(v) => json!(v),
                            PrototypeScalar::String(v) => json!(v),
                        },
                    )
                })
                .collect(),
        )
    }
    fn observed(value: EventValue) -> Option<Value> {
        match value {
            EventValue::Undefined => None,
            EventValue::Number(n) => Some(json!(n)),
            EventValue::Bool(v) => Some(json!(v)),
            EventValue::String(units) => Some(json!(String::from_utf16(&units).unwrap())),
            _ => panic!("native scalar summary cannot hide a referent"),
        }
    }
    fn failure(result: Result<(), ExchangeError>, count: Option<usize>) -> Option<Value> {
        assert_eq!(
            result.is_err(),
            count != Some(0),
            "native later reconciliation distinguishes zero, undefined, and nonzero prototype counts"
        );
        result.err().map(|error| {
            assert_eq!(
                error.name, "TypeError",
                "native later reconciliation error name"
            );
            let expected = if count.is_none() {
                "Cannot read properties of undefined (reading 'hadHandlers')"
            } else {
                "Cannot create property 'lastCallbacksCount' on number '1000'"
            };
            assert_eq!(
                error.message, expected,
                "native later reconciliation exact error message"
            );
            json!({"name":error.name,"message":error.message})
        })
    }
    fn normalize(value: &mut Value) {
        match value {
            Value::Number(n) => *value = json!(n.as_f64().unwrap()),
            Value::Array(values) => values.iter_mut().for_each(normalize),
            Value::Object(values) => values.values_mut().for_each(normalize),
            _ => {}
        }
    }
    let wrappers = [
        ("binance", "BinanceMarketWebsocket"),
        ("binance", "BinancePublicWebsocket"),
        ("binancepm", "BinancePMMarketWebsocket"),
        ("binancepm", "BinancePMPublicWebsocket"),
        ("binancespot", "BinancePublicWebsocket"),
    ];
    let mut native = vec![];
    for (variant, class_name) in wrappers {
        for count in [None, Some(0), Some(1), Some(2)] {
            let realm = Rc::<PrototypeRealm>::default();
            let _binding = realm.bind();
            let (a, _, clock_a) = setup(None, true, false, false);
            let (b, _, clock_b) = setup(None, true, false, false);
            clock_a.now.set(1000.0);
            if let Some(n) = count {
                let callbacks: Vec<MessageHandler> = (0..n)
                    .map(|_| Rc::new(|_: &Value| Ok(())) as MessageHandler)
                    .collect();
                a.message_handlers
                    .borrow_mut()
                    .insert("__proto__".into(), Rc::new(RefCell::new(callbacks)));
            }
            record_binance_handler_stats(&a, "__proto__").unwrap();
            assert!(
                Rc::ptr_eq(&a.prototype_realm, &b.prototype_realm),
                "native sockets share their captured realm"
            );
            let prototype = realm.reference(ProtoMember::Prototype);
            number(
                prototype.get("lastMessageTimestamp"),
                1000.0,
                "shared prototype timestamp",
            );
            assert!(
                match (prototype.get("lastCallbacksCount"), count) {
                    (PrototypeValue::Undefined, None) => true,
                    (PrototypeValue::Number(n), Some(expected)) => n == expected as f64,
                    _ => false,
                },
                "native shared prototype callbacks count"
            );
            let has = count.is_some_and(|n| n != 0);
            assert!(
                match prototype.get("hadHandlers") {
                    PrototypeValue::Boolean(true) => has,
                    PrototypeValue::Undefined => !has,
                    _ => false,
                },
                "native shared prototype handler flag"
            );
            let initial = fields(&prototype);
            clock_a.now.set(1010.0);
            record_binance_handler_stats(&a, "constructor").unwrap();
            let saved_constructor = realm.reference(ProtoMember::Constructor);
            number(
                saved_constructor.get("lastMessageTimestamp"),
                1010.0,
                "constructor referent timestamp",
            );
            number(
                saved_constructor.get("lastCallbacksCount"),
                1.0,
                "constructor inherited arity",
            );
            assert!(
                matches!(
                    saved_constructor.get("hadHandlers"),
                    PrototypeValue::Boolean(true)
                ),
                "native constructor handler flag"
            );
            assert!(
                matches!(saved_constructor.get("prototype"),PrototypeValue::Reference(value) if value==prototype),
                "native constructor prototype aliases shared object"
            );
            assert!(
                matches!(
                    EventValue::record_object(Default::default())
                        .read("lastMessageTimestamp")
                        .unwrap(),
                    EventValue::Number(1000.0)
                ),
                "native fresh ordinary object reads mutated prototype"
            );
            let row = EventValue::from_json(
                json!({"orderId":"12","symbol":"BTCUSDT","time":1,"side":"BUY","origType":"lastMessageTimestamp","type":"MARKET","status":"lastCallbacksCount","positionSide":"hadHandlers"}),
            );
            let historical = map_binance_historical_order_native(&row, &realm)
                .unwrap()
                .unwrap();
            assert!(
                matches!(historical.read("type").unwrap(), EventValue::Number(1000.0)),
                "native inherited historical type is 1000 before source"
            );
            assert!(
                match (historical.read("status").unwrap(), count) {
                    (EventValue::String(units), None)
                        if units.as_ref() == "closed".encode_utf16().collect::<Vec<_>>() =>
                        true,
                    (EventValue::Number(n), Some(expected)) => n == expected as f64,
                    _ => false,
                },
                "native historical inherited status distinguishes nullish fallback and zero"
            );
            assert!(
                match historical.read("positionSide").unwrap() {
                    EventValue::Bool(true) => has,
                    EventValue::Undefined => !has,
                    _ => false,
                },
                "native historical inherited position side is original scalar"
            );
            assert!(
                match historical
                    .read("type")
                    .unwrap()
                    .read("hadHandlers")
                    .unwrap()
                {
                    EventValue::Bool(true) => has,
                    EventValue::Undefined => !has,
                    _ => false,
                },
                "native numeric result boxes through execution realm prototype"
            );
            let fallback_row = EventValue::record_object(row.spread());
            fallback_row.write(
                "origType",
                EventValue::text_value("lastCallbacksCount".into()),
            );
            let fallback = map_binance_historical_order_native(&fallback_row, &realm)
                .unwrap()
                .unwrap()
                .read("type")
                .unwrap();
            assert!(
                match (&fallback, count) {
                    (EventValue::String(units), None)
                        if units.as_ref() == "market".encode_utf16().collect::<Vec<_>>() =>
                        true,
                    (EventValue::Number(n), Some(expected)) => *n == expected as f64,
                    _ => false,
                },
                "native historical type fallback preserves zero and suppresses eager fallback"
            );
            let function_row = EventValue::record_object(row.spread());
            function_row.write("origType", EventValue::text_value("constructor".into()));
            let function_output = map_binance_historical_order_native(&function_row, &realm)
                .unwrap()
                .unwrap()
                .read("type")
                .unwrap();
            assert!(
                matches!(function_output,EventValue::Builtin(ref value) if *value==saved_constructor),
                "native historical callable retains original constructor identity"
            );
            assert!(
                matches!(
                    function_output.read("lastMessageTimestamp").unwrap(),
                    EventValue::Number(1010.0)
                ),
                "native historical callable observes original referent writes"
            );
            let prototype_row = EventValue::record_object(row.spread());
            prototype_row.write("origType", EventValue::text_value("__proto__".into()));
            let prototype_output = map_binance_historical_order_native(&prototype_row, &realm)
                .unwrap()
                .unwrap()
                .read("type")
                .unwrap();
            assert!(
                matches!(prototype_output,EventValue::Builtin(ref value) if *value==prototype),
                "native historical prototype output retains original object identity"
            );
            assert!(
                matches!(
                    safe_cex_venue_binance::private_event_value::native_json(&function_output),
                    safe_cex_num::JsJsonValue::Undefined
                ),
                "native callable keeps its omission view despite enumerable statistics fields"
            );
            let safe_cex_num::JsJsonValue::Object(projected) =
                safe_cex_venue_binance::private_event_value::native_json(&prototype_output)
            else {
                panic!("native prototype object keeps its object serialization view before wire")
            };
            assert_eq!(
                projected.len(),
                if has { 3 } else { 2 },
                "native prototype object retains independently authored defined/undefined own fields before wire"
            );
            assert!(
                projected
                    .iter()
                    .any(|(key, value)| key == "lastMessageTimestamp"
                        && matches!(value, safe_cex_num::JsJsonValue::Number(1000.0))),
                "native prototype object retains timestamp 1000 before wire"
            );
            let mut queued_callback = Box::pin(safe_cex_venue_binance::class_jobs::start(
                clock_a.clone(),
                Box::pin(async {
                    safe_cex_venue_binance::class_jobs::reaction(std::future::ready(())).await;
                    safe_cex_venue_binance::private_event_value::table::<safe_cex_types::OrderType>(
                        &[],
                        EventValue::text_value("lastMessageTimestamp".into()),
                        PrivateEnum::Type,
                    )
                }),
            ));
            let other_realm = Rc::<PrototypeRealm>::default();
            let realm_observations = {
                let _other_binding = other_realm.bind();
                let (other, _, other_clock) = setup(None, true, false, false);
                other_clock.now.set(2000.0);
                record_binance_handler_stats(&other, "constructor").unwrap();
                let other_constructor = other_realm.reference(ProtoMember::Constructor);
                assert_ne!(
                    saved_constructor, other_constructor,
                    "native constructors have distinct realm identities"
                );
                number(
                    saved_constructor.get("lastMessageTimestamp"),
                    1010.0,
                    "saved constructor stays in first realm",
                );
                number(
                    other_constructor.get("lastMessageTimestamp"),
                    2000.0,
                    "second realm constructor timestamp",
                );
                clock_a.pump();
                let queued_value = match queued_callback
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                {
                    Poll::Ready(Ok(value)) => value,
                    other => panic!("native queued callback must settle: {other:?}"),
                };
                assert!(
                    matches!(queued_value, EventValue::Number(1000.0)),
                    "native queued callback reads original table realm: expected Number(1000), observed {queued_value:?}"
                );
                assert!(
                    Rc::ptr_eq(&PrototypeRealm::current(), &other_realm),
                    "native queued callback restores caller realm after polling"
                );
                let saved_mapper = || {
                    map_binance_historical_order_native(&row, &realm)
                        .unwrap()
                        .unwrap()
                };
                assert!(
                    matches!(
                        saved_mapper().read("type").unwrap(),
                        EventValue::Number(1000.0)
                    ),
                    "native saved mapper retains original module table realm while another realm is bound"
                );
                let first_timestamp = match saved_constructor.get("lastMessageTimestamp") {
                    PrototypeValue::Number(n) => n,
                    _ => panic!("native first constructor timestamp remains numeric"),
                };
                let second_timestamp = match other_constructor.get("lastMessageTimestamp") {
                    PrototypeValue::Number(n) => n,
                    _ => panic!("native second constructor timestamp remains numeric"),
                };
                let saved_mapper_type = observed(saved_mapper().read("type").unwrap());
                assert!(
                    Rc::ptr_eq(&PrototypeRealm::current(), &other_realm),
                    "native saved mapper restores caller realm after execution"
                );
                json!({"firstTimestamp":first_timestamp,"secondTimestamp":second_timestamp,"distinct":saved_constructor!=other_constructor,"savedMapperType":saved_mapper_type})
            };
            clock_b.now.set(6000.0);
            let ordinary_error = failure(record_binance_handler_stats(&b, "book.BTC"), count);
            {
                let stats = b.message_handlers_stats.borrow();
                let entry = stats
                    .get("book.BTC")
                    .expect("native ordinary entry commits before reconciliation failure");
                assert_eq!(
                    entry.last_message_timestamp, 6000.0,
                    "native ordinary timestamp commits before failure"
                );
                assert_eq!(
                    entry.last_callbacks_count,
                    safe_cex_num::PrototypeScalar::from(Some(1)),
                    "native ordinary presumed count survives failure"
                );
                assert_eq!(
                    entry.had_handlers,
                    Some(false),
                    "native own false shadows inherited true"
                );
            }
            clock_b.now.set(6001.0);
            record_binance_handler_stats(&b, "book.BTC")
                .expect("native failed reconciliation committed throttle before throw");
            assert_eq!(
                b.message_handlers_stats.borrow()["book.BTC"].last_message_timestamp,
                6001.0,
                "native retry updates current topic while reconciliation is throttled"
            );
            clock_b.now.set(6249.0);
            b.cleanup_message_handler_stats("book.BTC")
                .expect("native cleanup honors independent force throttle");
            assert!(
                b.message_handlers_stats.borrow().is_empty(),
                "native early cleanup deletes own topic"
            );
            clock_b.now.set(6500.0);
            record_binance_handler_stats(&b, "book.BTC")
                .expect("native ordinary reconciliation remains throttled before forced cleanup");
            let cleanup_error = failure(b.cleanup_message_handler_stats("book.BTC"), count);
            assert!(
                b.message_handlers_stats.borrow().is_empty(),
                "native forced cleanup deletes before possible throw"
            );
            number(
                prototype.get("lastMessageTimestamp"),
                1000.0,
                "cleanup preserves inherited shared referent",
            );
            let mut partial = serde_json::Map::new();
            for own_undefined in [false, true] {
                let (probe, _, probe_clock) = setup(None, true, false, false);
                probe_clock.now.set(7000.0);
                probe.message_handlers_stats.borrow_mut().insert(
                    "partial".into(),
                    MessageHandlerStat {
                        last_message_timestamp: 1.0,
                        last_callbacks_count: safe_cex_num::PrototypeScalar::from(Some(9)),
                        had_handlers: None,
                        had_handlers_undefined: own_undefined,
                    },
                );
                let error = failure(record_binance_handler_stats(&probe, "probe.BTC"), count);
                let stats = probe.message_handlers_stats.borrow();
                let entry = stats
                    .get("partial")
                    .expect("native partial own record survives reconciliation failure");
                let expected = if own_undefined || !has { 9 } else { 0 };
                assert_eq!(
                    entry.last_callbacks_count,
                    safe_cex_num::PrototypeScalar::from(Some(expected)),
                    "native own undefined shadows inherited hadHandlers"
                );
                assert_eq!(
                    entry.had_handlers, None,
                    "native reconciliation does not create an own handler flag"
                );
                assert_eq!(
                    entry.had_handlers_undefined, own_undefined,
                    "native optional field retains own undefined versus absence"
                );
                partial.insert(if own_undefined{"explicitUndefined"}else{"absent"}.into(),json!({"count":entry.last_callbacks_count,"ownUndefined":entry.had_handlers_undefined,"error":error}));
            }
            let mut history = serde_json::Map::new();
            for key in ["type", "status", "positionSide"] {
                if let Some(value) = observed(historical.get(key).unwrap()) {
                    history.insert(key.into(), value);
                }
            }
            let constructor_fields = fields(&saved_constructor);
            let prototype_fields = fields(&prototype);
            native.push(json!({"variant":variant,"className":class_name,"count":count,"initial":initial,"constructorFields":constructor_fields,"historical":history,"fallbackType":observed(fallback),"prototypeFields":prototype_fields,"ordinaryError":ordinary_error,"retryError":null,"cleanupEarlyError":null,"cleanupError":cleanup_error,"afterCleanupKeys":b.message_handlers_stats.borrow().keys().collect::<Vec<_>>(),"partial":partial,"realms":realm_observations}));
        }
    }
    assert_eq!(
        native.len(),
        20,
        "authored sequence floor is five wrappers by four handler counts"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/prototype-sequences.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 20, "independent source sequence floor");
    for (index, (mut native, source)) in native.into_iter().zip(source).enumerate() {
        for key in [
            "variant",
            "className",
            "count",
            "initial",
            "constructorFields",
            "historical",
            "fallbackType",
            "prototypeFields",
            "ordinaryError",
            "retryError",
            "cleanupEarlyError",
            "cleanupError",
            "afterCleanupKeys",
            "partial",
            "realms",
        ] {
            let mut expected = source[key].clone();
            normalize(&mut native[key]);
            normalize(&mut expected);
            assert_eq!(native[key], expected, "source sequence {key} case{index}");
        }
    }
}

#[test]
fn prototype_scalar_length_preserves_native_value_and_later_error() {
    use safe_cex_num::{ProtoMember, PrototypeRealm, PrototypeScalar, PrototypeValue};
    use safe_cex_venue_binance::public_stats::record_binance_handler_stats;
    let realm = Rc::new(PrototypeRealm::default());
    let _binding = realm.bind();
    let (socket, _, clock) = setup(None, true, false, false);
    realm
        .reference(ProtoMember::Prototype)
        .set("length", PrototypeScalar::Number(-0.5));
    record_binance_handler_stats(&socket, "__proto__").unwrap();
    let prototype = realm.reference(ProtoMember::Prototype);
    let PrototypeValue::Number(count) = prototype.get("lastCallbacksCount") else {
        panic!("native fractional callback count must remain numeric")
    };
    assert_eq!(
        count.to_bits(),
        (-0.5_f64).to_bits(),
        "native fractional callback count retains -0.5"
    );
    assert!(
        matches!(prototype.get("hadHandlers"), PrototypeValue::Boolean(true)),
        "native fractional callback count remains truthy"
    );
    clock.now.set(6000.0);
    let error = record_binance_handler_stats(&socket, "ordinary").unwrap_err();
    assert_eq!(
        error.name, "TypeError",
        "native scalar length later reconciliation error"
    );
    assert_eq!(
        error.message, "Cannot create property 'lastCallbacksCount' on number '-0.5'",
        "native scalar length later reconciliation message"
    );
    assert_eq!(
        socket.message_handlers_stats.borrow()["ordinary"].last_message_timestamp,
        6000.0,
        "native ordinary write commits before scalar failure"
    );
    clock.now.set(6001.0);
    record_binance_handler_stats(&socket, "ordinary").expect("native failed walk commits throttle");
}

#[test]
fn prototype_scalar_length_matrix_matches_source_after_native_assertions() {
    use safe_cex_num::{
        ProtoMember, PrototypeRealm, PrototypeScalar as Scalar, PrototypeValue as Native,
    };
    use safe_cex_venue_binance::public_stats::record_binance_handler_stats;
    fn scalar(actual: &Scalar, expected: &Scalar) {
        match (actual, expected) {
            (Scalar::Number(a), Scalar::Number(e)) => assert_eq!(
                a.to_bits(),
                e.to_bits(),
                "native scalar callback count bits"
            ),
            (Scalar::String(a), Scalar::String(e)) => {
                assert_eq!(a, e, "native scalar String callback count")
            }
            (Scalar::Boolean(a), Scalar::Boolean(e)) => {
                assert_eq!(a, e, "native scalar Boolean callback count")
            }
            (Scalar::Undefined, Scalar::Undefined) | (Scalar::Null, Scalar::Null) => {}
            _ => panic!("native scalar callback count variant: {actual:?}, expected {expected:?}"),
        }
    }
    fn inherited(actual: Native, expected: &Scalar) {
        match (actual, expected) {
            (Native::Number(a), Scalar::Number(e)) => assert_eq!(
                a.to_bits(),
                e.to_bits(),
                "native inherited scalar callback count bits"
            ),
            (Native::String(a), Scalar::String(e)) => {
                assert_eq!(&a, e, "native inherited String callback count")
            }
            (Native::Boolean(a), Scalar::Boolean(e)) => {
                assert_eq!(&a, e, "native inherited Boolean callback count")
            }
            (Native::Undefined, Scalar::Undefined) | (Native::Null, Scalar::Null) => {}
            (a, e) => {
                panic!("native inherited scalar callback count variant: {a:?}, expected {e:?}")
            }
        }
    }
    fn encode(value: &Scalar) -> Value {
        match value {
            Scalar::Number(n) => json!({"kind":"number","bits":format!("{:016x}",n.to_bits())}),
            Scalar::String(s) => json!({"kind":"string","value":s}),
            Scalar::Boolean(b) => json!({"kind":"boolean","value":b}),
            Scalar::Null => json!({"kind":"null","value":null}),
            Scalar::Undefined => json!({"kind":"undefined"}),
        }
    }
    let values = [
        (
            Scalar::Number(-0.5),
            true,
            Some("Cannot create property 'lastCallbacksCount' on number '-0.5'"),
        ),
        (
            Scalar::Number(0.5),
            true,
            Some("Cannot create property 'lastCallbacksCount' on number '0.5'"),
        ),
        (Scalar::Number(-0.0), false, None),
        (Scalar::Number(f64::NAN), false, None),
        (
            Scalar::Number(f64::INFINITY),
            true,
            Some("Cannot create property 'lastCallbacksCount' on number 'Infinity'"),
        ),
        (
            Scalar::Number(f64::NEG_INFINITY),
            true,
            Some("Cannot create property 'lastCallbacksCount' on number '-Infinity'"),
        ),
        (
            Scalar::Undefined,
            false,
            Some("Cannot read properties of undefined (reading 'hadHandlers')"),
        ),
        (
            Scalar::Null,
            false,
            Some("Cannot read properties of null (reading 'hadHandlers')"),
        ),
        (Scalar::Boolean(false), false, None),
        (
            Scalar::Boolean(true),
            true,
            Some("Cannot create property 'lastCallbacksCount' on boolean 'true'"),
        ),
        (
            Scalar::String("0".into()),
            true,
            Some("Cannot create property 'lastCallbacksCount' on string '0'"),
        ),
        (Scalar::String(String::new()), false, None),
    ];
    let wrappers = [
        ("binance", "BinanceMarketWebsocket"),
        ("binance", "BinancePublicWebsocket"),
        ("binancepm", "BinancePMMarketWebsocket"),
        ("binancepm", "BinancePMPublicWebsocket"),
        ("binancespot", "BinancePublicWebsocket"),
    ];
    let mut native = Vec::new();
    for (variant, class_name) in wrappers {
        for (index, (value, truthy, error_message)) in values.iter().enumerate() {
            for own in [false, true] {
                let realm = Rc::new(PrototypeRealm::default());
                let _binding = realm.bind();
                let (socket, _, clock) = setup(None, true, false, false);
                let prototype = realm.reference(ProtoMember::Prototype);
                prototype.set("length", value.clone());
                if own {
                    socket
                        .message_handlers_stats
                        .borrow_mut()
                        .insert("__proto__".into(), MessageHandlerStat::default());
                }
                record_binance_handler_stats(&socket, "__proto__")
                    .expect("native scalar recording succeeds before reconciliation floor");
                let (recorded, had, had_own) = if own {
                    let stats = socket.message_handlers_stats.borrow();
                    let entry = &stats["__proto__"];
                    scalar(&entry.last_callbacks_count, value);
                    assert_eq!(
                        entry.had_handlers,
                        truthy.then_some(true),
                        "native scalar own handler presence"
                    );
                    assert_eq!(
                        entry.last_message_timestamp, 1000.0,
                        "native scalar own timestamp"
                    );
                    assert!(
                        !entry.had_handlers_undefined,
                        "native scalar own absence is not explicit undefined"
                    );
                    (
                        entry.last_callbacks_count.clone(),
                        entry.had_handlers == Some(true),
                        entry.had_handlers.is_some(),
                    )
                } else {
                    inherited(prototype.get("lastCallbacksCount"), value);
                    if *truthy {
                        assert!(
                            matches!(prototype.get("hadHandlers"), Native::Boolean(true)),
                            "native scalar inherited handler presence"
                        );
                    } else {
                        assert!(
                            matches!(prototype.get("hadHandlers"), Native::Undefined),
                            "native scalar inherited handler absence is exact Undefined"
                        );
                    }
                    let had_own = prototype.own_fields().contains_key("hadHandlers");
                    assert_eq!(
                        had_own, *truthy,
                        "native scalar inherited hadHandlers own property"
                    );
                    assert!(
                        matches!(
                            prototype.get("lastMessageTimestamp"),
                            Native::Number(1000.0)
                        ),
                        "native scalar inherited timestamp"
                    );
                    let count = match prototype.get("lastCallbacksCount") {
                        Native::Undefined => Scalar::Undefined,
                        Native::Null => Scalar::Null,
                        Native::Number(n) => Scalar::Number(n),
                        Native::String(v) => Scalar::String(v),
                        Native::Boolean(v) => Scalar::Boolean(v),
                        other => panic!(
                            "native scalar reference outside configured vocabulary: {other:?}"
                        ),
                    };
                    (
                        count,
                        matches!(prototype.get("hadHandlers"), Native::Boolean(true)),
                        had_own,
                    )
                };
                let expected_error = if own && !matches!(value, Scalar::Undefined | Scalar::Null) {
                    None
                } else {
                    *error_message
                };
                clock.now.set(6000.0);
                let later = record_binance_handler_stats(&socket, "ordinary").err();
                assert_eq!(
                    later.as_ref().map(|e| e.name.as_str()),
                    expected_error.map(|_| "TypeError"),
                    "native scalar later error kind: {variant}/{class_name}/{index}/{own}"
                );
                assert_eq!(
                    later.as_ref().map(|e| e.message.as_str()),
                    expected_error,
                    "native scalar later error message: {variant}/{class_name}/{index}/{own}"
                );
                assert_eq!(
                    socket.message_handlers_stats.borrow()["ordinary"].last_message_timestamp,
                    6000.0,
                    "native scalar ordinary write commits before later failure"
                );
                clock.now.set(6001.0);
                record_binance_handler_stats(&socket, "ordinary")
                    .expect("native scalar failed walk retains throttle");
                clock.now.set(6500.0);
                let cleanup = socket.cleanup_message_handler_stats("ordinary").err();
                assert!(
                    !socket
                        .message_handlers_stats
                        .borrow()
                        .contains_key("ordinary"),
                    "native scalar cleanup deletes before later failure"
                );
                assert_eq!(
                    cleanup.as_ref().map(|e| e.name.as_str()),
                    expected_error.map(|_| "TypeError"),
                    "native scalar cleanup error kind"
                );
                assert_eq!(
                    cleanup.as_ref().map(|e| e.message.as_str()),
                    expected_error,
                    "native scalar cleanup error message"
                );
                let mut json_view = serde_json::Map::new();
                if !matches!(&recorded, Scalar::Undefined) {
                    json_view.insert(
                        "lastCallbacksCount".into(),
                        serde_json::to_value(&recorded).expect("scalar JSON projection"),
                    );
                }
                let had_value = if had {
                    Scalar::Boolean(true)
                } else {
                    Scalar::Undefined
                };
                native.push(json!({"jsonView":json_view,"hadOwn":had_own,"hadValue":encode(&had_value),"variant":variant,"className":class_name,"caseIndex":index,"own":own,"count":encode(&recorded),"had":had,"recordError":null,"laterError":later.map(|e|json!({"name":e.name,"message":e.message})),"ordinaryTime":6000,"retryError":null,"cleanupError":cleanup.map(|e|json!({"name":e.name,"message":e.message})),"ordinaryDeleted":true}));
            }
        }
    }
    assert_eq!(
        native.len(),
        120,
        "native fixed five wrappers by twelve scalars by two record layouts"
    );
    let mut sticky_cases = 0;
    for (value, truthy, _) in &values {
        if *truthy {
            continue;
        }
        let realm = Rc::new(PrototypeRealm::default());
        let _binding = realm.bind();
        let (socket, _, _) = setup(None, true, false, false);
        realm
            .reference(ProtoMember::Prototype)
            .set("length", value.clone());
        socket.message_handlers_stats.borrow_mut().insert(
            "__proto__".into(),
            MessageHandlerStat {
                had_handlers: Some(true),
                ..MessageHandlerStat::default()
            },
        );
        record_binance_handler_stats(&socket, "__proto__").unwrap();
        let stats = socket.message_handlers_stats.borrow();
        let entry = &stats["__proto__"];
        scalar(&entry.last_callbacks_count, value);
        assert_eq!(
            entry.had_handlers,
            Some(true),
            "native scalar falsy length retains sticky true"
        );
        sticky_cases += 1;
    }
    assert_eq!(
        sticky_cases, 6,
        "native authored six falsy scalar sticky cases"
    );
    // All native assertions above precede invocation and comparison of the source oracle.
    let output = std::process::Command::new("node")
        .arg("scripts/prototype-scalar-length.mjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Node required for maintained scalar source oracle");
    assert!(
        output.status.success(),
        "source scalar oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).expect("scalar source records");
    assert_eq!(
        source.len(),
        120,
        "source fixed five wrappers by twelve scalars by two record layouts"
    );
    assert_eq!(
        source, native,
        "source scalar length corpus agrees after all native assertions"
    );
}

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
