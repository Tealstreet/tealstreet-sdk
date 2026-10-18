use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{runtime::*, websocket::*};
use safe_cex_store::Store;
use safe_cex_transport::{
    TransportProvider,
    socket::{SocketFrame, TunnelHost},
};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_mexc::ws_public::{MexcPublicProtocol, on_message};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};
type LocalFuture = Pin<Box<dyn Future<Output = ()>>>;
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
struct Runtime {
    clocks: Cell<usize>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1250.0
    }
    fn now_ms(&self) -> f64 {
        self.clocks.set(self.clocks.get() + 1);
        1250.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, future: LocalFuture) -> Box<dyn ExchangeTask> {
        drop(future);
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

struct Parent(Rc<DefaultStore>);
impl WsParent for Parent {
    fn name(&self) -> &str {
        "mexc"
    }
    fn is_disposed(&self) -> bool {
        false
    }
    fn options(&self) -> ExchangeOptions {
        serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.0.clone()
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        false
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        Rc::default()
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
        _: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
}
const KEY: &str = "trade.BTCUSDT";
fn frame() -> SocketFrame {
    SocketFrame::Text(r#"{"channel":"push.deal","symbol":"BTC_USDT"}"#.into())
}
fn route(
    protocol: &MexcPublicProtocol,
    socket: &Rc<BaseWebSocket>,
    calls: &Cell<usize>,
) -> Result<(), ExchangeError> {
    calls.set(calls.get() + 1);
    on_message(protocol, socket, &frame())
}
#[test]
fn mexc_public_handler_arrays_match_source_reentrant_routes() {
    // Expectations are fixed independently of either receiver or JSON output.
    let expected: [(&str, &[&str], Option<usize>, usize, usize, usize); 7] = [
        ("add", &["A", "B", "A", "B", "C"], Some(2), 3, 2, 0),
        ("remove", &["A", "B", "A"], Some(2), 1, 2, 0),
        ("recursive", &["A", "A", "B", "C", "B"], Some(2), 3, 2, 0),
        ("throw", &["A"], Some(2), 3, 1, 0),
        ("duplicates", &["A", "C"], Some(4), 2, 1, 0),
        ("missing", &[], None, 0, 1, 1),
        ("remove-all", &[], Some(1), 0, 1, 2),
    ];
    let mut native = Vec::new();
    let mut total_callbacks = 0;
    let mut total_routes = 0;
    for (name, wanted, old_len, current_len, route_count, cleanup_count) in expected {
        let runtime = Rc::new(Runtime {
            clocks: Cell::new(0),
        });
        let protocol = Rc::new(MexcPublicProtocol::new(Rc::new(|v| v)));
        let socket = BaseWebSocket::new(
            1,
            Rc::new(Parent(Rc::new(DefaultStore::new()))),
            runtime.clone(),
            protocol.clone(),
        );
        let trace = Rc::new(RefCell::new(Vec::new()));
        let b: MessageHandler = {
            let trace = trace.clone();
            Rc::new(move |_| {
                trace.borrow_mut().push("B");
                Ok(())
            })
        };
        let c: MessageHandler = {
            let trace = trace.clone();
            Rc::new(move |_| {
                trace.borrow_mut().push("C");
                Ok(())
            })
        };
        let calls = Rc::new(Cell::new(0));
        let cleanup_clocks = Rc::new(Cell::new(0));
        let mut original_error = ExchangeError::new("handler failure");
        original_error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "HANDLER_ORIGIN"
        )));
        original_error = original_error.with_retained_response((json!({"sentinel": 42})).into());
        let a: MessageHandler = {
            let (trace, b, c) = (trace.clone(), b.clone(), c.clone());
            let (protocol, socket) = (Rc::downgrade(&protocol), Rc::downgrade(&socket));
            let (calls, runtime, cleanup_clocks) =
                (calls.clone(), runtime.clone(), cleanup_clocks.clone());
            let error = original_error.clone();
            let changed = Cell::new(false);
            Rc::new(move |_| {
                trace.borrow_mut().push("A");
                if !changed.get() && matches!(name, "add" | "remove" | "recursive" | "throw") {
                    changed.set(true);
                    let (protocol, socket) =
                        (protocol.upgrade().unwrap(), socket.upgrade().unwrap());
                    if name == "remove" {
                        let before = runtime.clocks.get();
                        protocol.remove_handler(&socket, KEY, &b)?;
                        cleanup_clocks.set(cleanup_clocks.get() + runtime.clocks.get() - before);
                    } else {
                        protocol.add_handler(&socket, KEY, c.clone())?;
                    }
                    if name == "recursive" {
                        route(&protocol, &socket, &calls)?;
                    }
                    if name == "throw" {
                        return Err(error.clone());
                    }
                }
                Ok(())
            })
        };
        if name != "missing" {
            protocol.add_handler(&socket, KEY, a.clone()).unwrap();
        }
        if matches!(
            name,
            "add" | "remove" | "recursive" | "throw" | "duplicates"
        ) {
            protocol.add_handler(&socket, KEY, b.clone()).unwrap();
        }
        if name == "duplicates" {
            protocol.add_handler(&socket, KEY, b.clone()).unwrap();
            protocol.add_handler(&socket, KEY, c).unwrap();
        }
        let old = socket.message_handlers.borrow().get(KEY).cloned();
        // A pre-existing stats entry makes omitted cleanup observable.
        socket
            .message_handlers_stats
            .borrow_mut()
            .insert(KEY.into(), MessageHandlerStat::default());
        let before = runtime.clocks.get();
        if name == "duplicates" || name == "missing" {
            protocol
                .remove_handler(&socket, KEY, &b)
                .expect("native handler removal succeeds");
        }
        if name == "remove-all" {
            protocol
                .remove_handler(&socket, KEY, &a)
                .expect("native handler removal succeeds");
            protocol
                .remove_handler(&socket, KEY, &a)
                .expect("native handler removal succeeds");
        }
        cleanup_clocks.set(runtime.clocks.get() - before);
        let result = route(&protocol, &socket, &calls);
        if matches!(name, "add" | "remove") {
            route(&protocol, &socket, &calls).unwrap();
        }
        // Native callback order, retained array identities, cardinalities and error metadata FIRST.
        assert_eq!(
            trace.borrow().as_slice(),
            wanted,
            "MEXC captured native handler callbacks {name}"
        );
        assert_eq!(
            old.as_ref().map(|v| v.borrow().len()),
            old_len,
            "MEXC retained native old array {name}"
        );
        let current = socket
            .message_handlers
            .borrow()
            .get(KEY)
            .cloned()
            .expect("native own registry key");
        assert!(
            old.as_ref().is_none_or(|old| !Rc::ptr_eq(old, &current)),
            "MEXC native replaces array {name}"
        );
        assert_eq!(
            current.borrow().len(),
            current_len,
            "MEXC current native array {name}"
        );
        assert_eq!(
            calls.get(),
            route_count,
            "MEXC native route-call floor {name}"
        );
        // Each isolated cleanup calls the clock once; nonempty removal does not.
        assert_eq!(
            cleanup_clocks.get(),
            cleanup_count,
            "MEXC native cleanup calls {name}"
        );
        assert_eq!(
            socket.message_handlers_stats.borrow().contains_key(KEY),
            cleanup_count == 0,
            "MEXC cleanup removes native stats {name}"
        );
        let error = if name == "throw" {
            let error = result.unwrap_err();
            assert_eq!(
                error, original_error,
                "MEXC native same structured handler error"
            );
            Some(error)
        } else {
            result.unwrap();
            None
        };
        total_callbacks += trace.borrow().len();
        total_routes += calls.get();
        native.push(json!({"name":name,"log":*trace.borrow(),"oldLength":old_len,"currentLength":current_len,
            "routes":calls.get(),"cleanup":cleanup_clocks.get(),"errorCode":error.and_then(|e|observed_error_code(&e.code).clone())}));
    }
    assert_eq!(
        total_callbacks, 16,
        "MEXC independent native callback-call floor"
    );
    assert_eq!(total_routes, 10, "MEXC independent native route-call floor");
    // Stock inherited non-array entries throw before changing the own registry.
    for removing in [false, true] {
        let protocol = Rc::new(MexcPublicProtocol::new(Rc::new(|v| v)));
        let socket = BaseWebSocket::new(
            2,
            Rc::new(Parent(Rc::new(DefaultStore::new()))),
            Rc::new(Runtime {
                clocks: Cell::new(0),
            }),
            protocol.clone(),
        );
        let handler: MessageHandler = Rc::new(|_| Ok(()));
        let error = if removing {
            protocol
                .remove_handler(&socket, "constructor", &handler)
                .unwrap_err()
        } else {
            protocol
                .add_handler(&socket, "constructor", handler)
                .unwrap_err()
        };
        assert_eq!(
            error.name, "TypeError",
            "MEXC inherited registry key throws before write"
        );
        assert!(
            !socket.message_handlers.borrow().contains_key("constructor"),
            "MEXC failed inherited operation leaves registry unchanged"
        );
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public_handler_array_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SOURCE actual-method oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"].as_array().unwrap().len(),
        7,
        "SOURCE independent scenario floor"
    );
    assert_eq!(source["calls"], 16, "SOURCE callback-call floor");
    assert_eq!(source["routes"], 10, "SOURCE route-call floor");
    assert_eq!(
        source["inheritedConstructorErrors"], 2,
        "SOURCE inherited-key rejection floor"
    );
    assert_eq!(
        json!(native),
        source["cases"],
        "secondary source/native trace comparison"
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

#[test]
fn retained_handler_frames_cross_the_production_core_registry() {
    let protocol = Rc::new(MexcPublicProtocol::new(Rc::new(|v| v)));
    let socket = BaseWebSocket::new(
        1,
        Rc::new(Parent(Rc::new(DefaultStore::new()))),
        Rc::new(Runtime {
            clocks: Cell::new(0),
        }),
        protocol.clone(),
    );
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let callback: MessageHandler = Rc::new(move |_| {
        seen.set(seen.get() + 1);
        Ok(())
    });
    protocol
        .add_handler(&socket, "trade.BTCUSDT", callback)
        .unwrap();
    for marker in [r"\ud800", r"\udfff", r"\ud83e\udd80"] {
        let frame = format!(
            r#"{{"channel":"push.deal","symbol":"BTC_USDT","unused":{{"{marker}":["{marker}"]}},"overflow":1e400,"zero":-0}}"#
        );
        on_message(&protocol, &socket, &SocketFrame::Text(frame)).unwrap();
    }
    on_message(
        &protocol,
        &socket,
        &SocketFrame::Text(r#"{"channel":"push.deal","symbol":"BTC_USDT","neighbor":true}"#.into()),
    )
    .unwrap();
    assert_eq!(
        calls.get(),
        4,
        "source-valid native graphs reach MEXC callbacks"
    );
    socket.dispose();
}

#[cfg(test)]
mod retained_handler_support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-exchange/tests/support/retained_handlers.rs"
    ));
}
#[test]
fn shared_core_handlers_retain_the_source_graph_and_native_fields() {
    let protocol = Rc::new(MexcPublicProtocol::new(Rc::new(|v| v)));
    let socket = BaseWebSocket::new(
        1,
        Rc::new(Parent(Rc::new(DefaultStore::new()))),
        Rc::new(Runtime {
            clocks: Cell::new(0),
        }),
        protocol.clone(),
    );
    let cases = retained_handler_support::source_frames("mexc", "publicWebsocket");
    let (handlers, calls) = retained_handler_support::pair(&cases, false);
    socket
        .message_handlers
        .borrow_mut()
        .insert("trade.BTCUSDT".into(), handlers);
    for case in cases {
        on_message(
            &protocol,
            &socket,
            &SocketFrame::Text(case["frame"].as_str().unwrap().into()),
        )
        .unwrap();
    }
    assert_eq!(calls.get(), 8);
    socket.dispose();
}
