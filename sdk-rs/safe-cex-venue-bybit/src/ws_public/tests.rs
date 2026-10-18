use super::*;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    runtime::{ExchangeFuture, ExchangeTask, LogSeverity},
    websocket::{
        ReopenResync, WebSocketConnectionOptions, WsEvent, WsParent, WsProtocol, WsRuntime,
        WsSocket,
    },
};
use safe_cex_store::Store;
use safe_cex_transport::{DirectTransport, TransportProvider, socket::TunnelHost};
use safe_cex_types::ExchangeOptions;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::{Pin, pin},
    sync::Arc,
    task::{Context, Poll, Waker},
};

fn ready<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture operation unexpectedly pending"),
    }
}
#[derive(Default)]
struct Wire(RefCell<Vec<String>>, Cell<usize>);
impl WsSocket for Wire {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(std::future::pending())
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.0.borrow_mut().push(message.into());
        Ok(())
    }
    fn close(&self) {
        self.1.set(self.1.get() + 1);
    }
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
type TaskFuture = Rc<RefCell<Option<Pin<Box<dyn Future<Output = ()>>>>>>;
struct Task(TaskFuture);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.borrow_mut().take();
    }
}
#[derive(Default)]
struct Runtime {
    wire: Rc<Wire>,
    console: RefCell<Vec<(String, String)>>,
    exceptions: RefCell<Vec<ExchangeError>>,
    sleeps: RefCell<Vec<u64>>,
    diagnostics: Cell<bool>,
    monotonic: Cell<f64>,
    tasks: RefCell<Vec<TaskFuture>>,
    inline: Cell<bool>,
    pending_timers: Cell<bool>,
    dials: RefCell<Vec<Value>>,
    provider: RefCell<Option<Arc<dyn TransportProvider>>>,
    relay_host: Rc<dial_tests::PendingTunnelHost>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.monotonic.get()
    }
    fn now_ms(&self) -> f64 {
        1000.
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        self.sleeps.borrow_mut().push(delay);
        if self.pending_timers.get() {
            Box::pin(std::future::pending())
        } else {
            Box::pin(async { Ok(()) })
        }
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let task = Rc::new(RefCell::new(Some(future)));
        if self.inline.get() {
            let mut pending = task.borrow_mut().take().unwrap();
            if pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                *task.borrow_mut() = Some(pending);
            }
        }
        self.tasks.borrow_mut().push(task.clone());
        Box::new(Task(task))
    }
    fn console_values(&self, level: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.console
            .borrow_mut()
            .push((level.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        "fixture".into()
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
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.exceptions.borrow_mut().push(error.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("fixture has no worker")) })
    }
}
impl WsRuntime for Runtime {
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.
    }
    fn transport(&self) -> Arc<dyn TransportProvider> {
        self.provider
            .borrow()
            .clone()
            .unwrap_or_else(|| Arc::new(DirectTransport))
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        self.relay_host.clone()
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let headers = match options.headers {
            Some(safe_cex_exchange::websocket::UpgradeHeaders::Static(headers)) => {
                serde_json::to_value(headers).unwrap()
            }
            None => json!({}),
            _ => panic!("unexpected dynamic headers"),
        };
        self.dials
            .borrow_mut()
            .push(json!({"url":url,"protocols":protocols,"headers":headers}));
        let socket: Rc<dyn WsSocket> = self.wire.clone();
        Box::pin(async move { Ok(socket) })
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1.
    }
    fn diagnostics_enabled(&self) -> bool {
        self.diagnostics.get()
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Parent {
    failure: RefCell<Option<(&'static str, ExchangeError)>>,
    health_writes: Cell<usize>,
    events: RefCell<Vec<(String, String)>>,
    latency: Cell<Option<f64>>,
    options: ExchangeOptions,
    store: Rc<DefaultStore>,
    disposed: Cell<bool>,
    coordinator: Rc<ReopenResync>,
}
impl Parent {
    fn check(&self, site: &str) -> Result<(), ExchangeError> {
        if let Some((target, error)) = &*self.failure.borrow()
            && *target == site
        {
            return Err(error.clone());
        }
        Ok(())
    }
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "bybit"
    }
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn options(&self) -> ExchangeOptions {
        self.options.clone()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        false
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.coordinator.clone()
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        self.events
            .borrow_mut()
            .push(("snapshot".into(), String::new()));
        Box::pin(async { Ok(()) })
    }
    fn log(&self, _: &str, _: LogSeverity) -> Result<(), ExchangeError> {
        self.check("log")
    }
    fn emit(&self, event: &str, data: &str) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push((event.into(), data.into()));
        self.check("emit")
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.health_writes.set(self.health_writes.get() + 1);
        self.store
            .update_socket_health(json!({"publicHealthy": true}).as_object().unwrap())?;
        self.check("health")
    }
    fn update_latency(&self, value: f64) -> Result<(), ExchangeError> {
        self.latency.set(Some(value));
        Ok(())
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_and_log_error(
        &self,
        error: &str,
        _: Option<&str>,
        _: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        self.events
            .borrow_mut()
            .push(("error".into(), error.into()));
        self.check("error")
    }
}
struct Protocol;
impl WsProtocol for Protocol {}

mod exchange_composition;
fn fixture() -> (Rc<BaseWebSocket>, Rc<Parent>, Rc<Runtime>) {
    let parent = Rc::new(Parent {
        failure: RefCell::default(),
        health_writes: Cell::new(0),
        events: RefCell::default(),
        latency: Cell::new(None),
        options: serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        store: Rc::new(DefaultStore::new()),
        disposed: Cell::new(false),
        coordinator: Rc::default(),
    });
    let runtime = Rc::new(Runtime::default());
    runtime.monotonic.set(1000.);
    let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
    ready(socket.create_websocket("wss://stream.bybit.com".into(), vec![], Default::default()))
        .unwrap();
    (socket, parent, runtime)
}
#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn dispatch_and_subscription_match_live_source() {
    let run = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-public-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let fixtures: Value = serde_json::from_slice(&run.stdout).unwrap();
    for (key, count) in [
        ("dispatch", 224),
        ("subscriptions", 24),
        ("sends", 8),
        ("diagnostics", 14),
        ("divergences", 3),
        ("prototypeDispatch", 24),
    ] {
        assert_eq!(
            fixtures[key].as_array().unwrap().len(),
            count,
            "WS-CARDINALITY source {key}"
        );
    }
    for (i, case) in fixtures["dispatch"].as_array().unwrap().iter().enumerate() {
        let (socket, parent, _) = fixture();
        parent.disposed.set(case["disposed"] == true);
        let calls = Rc::new(RefCell::new(Vec::new()));
        for (key, count) in case["handlers"].as_object().unwrap() {
            let callbacks: HandlerList = Rc::default();
            for n in 0..count.as_u64().unwrap() {
                let label = format!("{key}:{n}");
                let output = calls.clone();
                callbacks.borrow_mut().push(Rc::new(move |value| {
                    output
                        .borrow_mut()
                        .push(json!({"handler":label,"value":value}));
                    Ok(())
                }));
            }
            socket
                .message_handlers
                .borrow_mut()
                .insert(key.clone(), callbacks);
        }
        on_message(&socket, case["data"].as_str().unwrap()).unwrap();
        assert_eq!(
            json!(*calls.borrow()),
            case["calls"],
            "callbacks case {i}: {case}"
        );
        let mut keys: Vec<_> = socket
            .message_handlers_stats
            .borrow()
            .keys()
            .cloned()
            .collect();
        keys.sort();
        assert_eq!(json!(keys), case["stats"], "stats case {i}: {case}");
        assert_eq!(socket.diagnostics().initialized, !parent.disposed.get());
        socket.dispose();
    }
    for case in fixtures["sends"].as_array().unwrap() {
        let (socket, parent, runtime) = fixture();
        runtime.diagnostics.set(true);
        parent.disposed.set(case["disposed"] == true);
        match case["method"].as_str().unwrap() {
            "publicPing" => ping(&socket).unwrap(),
            "privatePing" => crate::ws_private::ping(&socket).unwrap(),
            "privateSubscribe" => crate::ws_private::subscribe(&socket).unwrap(),
            "diagnosticSend" => send_ohlcv_diagnostic_payload(
                &socket,
                case["op"].as_str().unwrap(),
                &serde_json::from_value::<Vec<String>>(case["topics"].clone()).unwrap(),
            )
            .unwrap(),
            _ => panic!("unknown send fixture"),
        }
        if case["method"].as_str().unwrap().ends_with("Ping") {
            runtime.monotonic.set(1200.);
            socket.handle_pong_event(1000).unwrap();
            assert_eq!(
                parent.latency.get(),
                if parent.disposed.get() {
                    None
                } else {
                    Some(100.)
                },
                "ping timestamp: {case}"
            );
        }
        let diagnostic_sends = parent
            .events
            .borrow()
            .iter()
            .filter(|(event, _)| event == "grafana-pws-sub")
            .count();
        assert_eq!(
            diagnostic_sends,
            case["diagnosticSends"].as_u64().unwrap() as usize,
            "send diagnostic: {case}"
        );
        assert_eq!(json!(*runtime.wire.0.borrow()), case["messages"]);
        socket.dispose();
    }
    for case in fixtures["diagnostics"].as_array().unwrap() {
        let (socket, parent, runtime) = fixture();
        runtime.diagnostics.set(true);
        emit_ohlcv_diagnostic_receive(&socket, case["data"].as_str().unwrap()).unwrap();
        let emitted = parent
            .events
            .borrow()
            .iter()
            .any(|(event, _)| event == "grafana-pws");
        assert_eq!(
            emitted,
            case["allowed"].as_bool().unwrap(),
            "diagnostic: {case}"
        );
        socket.dispose();
    }
    for case in fixtures["prototypeDispatch"].as_array().unwrap() {
        let (socket, _, _) = fixture();
        on_message(&socket, case["data"].as_str().unwrap()).unwrap();
        // FINDING: the current own-map lookup skips every inherited member.
        // TS positive-arity functions record stats then throw at indexed invocation;
        // zero-arity functions and the prototype object return at the length guard.
        assert!(socket.message_handlers_stats.borrow().is_empty(), "{case}");
        if case["positive"] == true {
            assert_eq!(case["error"]["name"], "TypeError");
            assert_eq!(case["stats"], json!([case["key"]]));
        } else {
            assert!(case.get("error").is_none());
            assert_eq!(case["stats"], json!([]));
        }
        socket.dispose();
    }
    for case in fixtures["divergences"].as_array().unwrap() {
        let (socket, _, _) = fixture();
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        socket.message_handlers.borrow_mut().insert(
            "tickers".into(),
            Rc::new(RefCell::new(vec![Rc::new(move |_| {
                count.set(count.get() + 1);
                Ok(())
            })])),
        );
        on_message(&socket, case["data"].as_str().unwrap()).unwrap();
        assert_eq!(
            calls.get(),
            0,
            "Rust typed-map/scalar-boundary refusal: {case}"
        );
        let has_stats = socket
            .message_handlers_stats
            .borrow()
            .contains_key("tickers");
        assert_eq!(has_stats, case["kind"] != "prototype");
        socket.dispose();
    }
    for case in fixtures["subscriptions"].as_array().unwrap() {
        let (socket, _, runtime) = fixture();
        ready(subscribe(
            &socket,
            case["topics"].as_object().unwrap(),
            runtime.as_ref(),
        ))
        .unwrap();
        assert_eq!(json!(*runtime.wire.0.borrow()), case["messages"]);
        assert_eq!(json!(*runtime.sleeps.borrow()), case["delays"]);
        socket.dispose();
    }
}
#[test]
fn dispatch_retains_captured_lists_but_observes_array_mutations() {
    let (socket, _, _) = fixture();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let exact: HandlerList = Rc::default();
    let prefix: HandlerList = Rc::default();
    let output = calls.clone();
    prefix.borrow_mut().push(Rc::new(move |_| {
        output.borrow_mut().push("prefix");
        Ok(())
    }));
    let output = calls.clone();
    let weak_socket = Rc::downgrade(&socket);
    let weak_exact = Rc::downgrade(&exact);
    exact.borrow_mut().push(Rc::new(move |_| {
        output.borrow_mut().push("exact");
        weak_socket
            .upgrade()
            .unwrap()
            .message_handlers
            .borrow_mut()
            .shift_remove("tickers");
        let output = output.clone();
        weak_exact
            .upgrade()
            .unwrap()
            .borrow_mut()
            .push(Rc::new(move |_| {
                output.borrow_mut().push("appended");
                Ok(())
            }));
        Ok(())
    }));
    socket
        .message_handlers
        .borrow_mut()
        .insert("tickers.BTCUSDT".into(), exact);
    socket
        .message_handlers
        .borrow_mut()
        .insert("tickers".into(), prefix);
    on_message(&socket, r#"{"topic":"tickers.BTCUSDT","data":{}}"#).unwrap();
    assert_eq!(*calls.borrow(), ["exact", "appended", "prefix"]);
    socket.dispose();
}

#[test]
fn frame_numbers_round_once_and_retain_negative_zero() {
    let frame = parse_frame(r#"{"n":-0,"nested":[9007199254740993,1e21]}"#).unwrap();
    assert!(frame["n"].as_f64().unwrap().is_sign_negative());
    assert_eq!(frame["nested"][0].as_f64(), Some(9007199254740992.));
    assert_eq!(frame["nested"][1].as_f64(), Some(1e21));
}

#[test]
fn callback_self_removal_skips_the_shifted_next_element() {
    let (socket, _, _) = fixture();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let handlers: HandlerList = Rc::default();
    let weak = Rc::downgrade(&handlers);
    let output = calls.clone();
    handlers.borrow_mut().push(Rc::new(move |_| {
        output.borrow_mut().push("first");
        weak.upgrade().unwrap().borrow_mut().remove(0);
        Ok(())
    }));
    for name in ["second", "third"] {
        let output = calls.clone();
        handlers.borrow_mut().push(Rc::new(move |_| {
            output.borrow_mut().push(name);
            Ok(())
        }));
    }
    socket
        .message_handlers
        .borrow_mut()
        .insert("tickers".into(), handlers);
    on_message(&socket, r#"{"topic":"tickers.BTCUSDT"}"#).unwrap();
    assert_eq!(*calls.borrow(), ["first", "third"]);
    socket.dispose();
}

mod dial_tests;

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn local_auth_preimage_and_wire_match_live_source() {
    let run = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-auth-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let cases: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(
        cases.as_array().unwrap().len(),
        690,
        "AUTH-CARDINALITY source total"
    );
    for (i, case) in cases.as_array().unwrap().iter().enumerate() {
        let (_, old_parent, _) = fixture();
        let mut parent = Rc::try_unwrap(old_parent).ok().unwrap();
        parent.options.key = "fixture-key".into();
        parent.options.secret = case["secret"].as_str().unwrap().into();
        let parent = Rc::new(parent);
        let runtime = Rc::new(Runtime::default());
        let socket = BaseWebSocket::new(9, parent.clone(), runtime.clone(), Rc::new(Protocol));
        ready(socket.create_websocket("wss://stream.bybit.com".into(), vec![], Default::default()))
            .unwrap();
        parent.disposed.set(case["disposed"] == true);
        let now = case["now"]
            .as_f64()
            .unwrap_or_else(|| case["now"].as_str().unwrap().parse().unwrap());
        let result = ready(crate::ws_private::local_auth::auth_local(&socket, now)).unwrap();
        assert_eq!(result, case["result"] == true, "result {i}");
        assert_eq!(
            json!(*runtime.wire.0.borrow()),
            case["messages"],
            "wire/HMAC {i}: {case}"
        );
        socket.dispose();
    }
}

#[test]
fn handler_failure_aborts_remaining_exact_and_prefix_callbacks() {
    let (socket, _, _) = fixture();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let exact: HandlerList = Rc::default();
    let prefix: HandlerList = Rc::default();
    let output = calls.clone();
    exact.borrow_mut().push(Rc::new(move |_| {
        output.borrow_mut().push("failure");
        Err(ExchangeError::new("fixture callback rejected"))
    }));
    for (list, name) in [(&exact, "remaining"), (&prefix, "prefix")] {
        let output = calls.clone();
        list.borrow_mut().push(Rc::new(move |_| {
            output.borrow_mut().push(name);
            Ok(())
        }));
    }
    socket
        .message_handlers
        .borrow_mut()
        .insert("tickers.BTCUSDT".into(), exact);
    socket
        .message_handlers
        .borrow_mut()
        .insert("tickers".into(), prefix);
    let error = on_message(&socket, r#"{"topic":"tickers.BTCUSDT"}"#).unwrap_err();
    assert_eq!(error.to_string(), "fixture callback rejected");
    assert_eq!(*calls.borrow(), ["failure"]);
    socket.dispose();
}

struct AuthSigner {
    native_extra_data: RefCell<Vec<String>>,
    requests: RefCell<Vec<Value>>,
    response: String,
    wrong_variant: bool,
    pending: bool,
}
impl safe_cex_transport::signing::RemoteSigner for AuthSigner {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse>
    {
        if let safe_cex_transport::signing::RemoteSignerRequest::WsAuth { request, .. } = &request
            && let Some(extra) = request
                .ws_auth
                .as_ref()
                .and_then(|auth| auth.extra_data.as_ref())
        {
            self.native_extra_data.borrow_mut().push(extra.get().into());
        }
        self.requests
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        Box::pin(async move {
            if self.pending {
                return std::future::pending().await;
            }
            if self.wrong_variant {
                return Ok(safe_cex_transport::signing::RemoteSignerResponse::Http(
                    Default::default(),
                ));
            }
            request.decode_response(&self.response)
        })
    }
}
impl safe_cex_exchange::runtime::RemoteSigner for AuthSigner {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> safe_cex_exchange::runtime::Cleanup {
        Box::new(|| {})
    }
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn remote_auth_envelope_and_wire_match_live_source() {
    let run = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-auth-fixtures.mjs"
        ))
        .arg("--remote")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(cases.len(), 1150);
    for (i, case) in cases.iter().enumerate() {
        let (_, old_parent, _) = fixture();
        let mut parent = Rc::try_unwrap(old_parent).ok().unwrap();
        parent.options.key = "fixture-key".into();
        let parent = Rc::new(parent);
        let runtime = Rc::new(Runtime::default());
        let socket = BaseWebSocket::new(9, parent.clone(), runtime.clone(), Rc::new(Protocol));
        ready(socket.create_websocket("wss://stream.bybit.com".into(), vec![], Default::default()))
            .unwrap();
        parent.disposed.set(case["disposed"] == true);
        let signer = Rc::new(AuthSigner {
            native_extra_data: RefCell::default(),
            requests: RefCell::new(vec![]),
            response: case["response"].to_string(),
            wrong_variant: false,
            pending: false,
        });
        let mut base = safe_cex_exchange::exchange::BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            parent.options.clone(),
            parent.store.clone(),
            runtime.clone(),
            Default::default(),
        );
        base.signers.remote_signer = Some(signer.clone());
        let now = case["now"]
            .as_f64()
            .unwrap_or_else(|| case["now"].as_str().unwrap().parse().unwrap());
        let result = ready(crate::ws_private::local_auth::auth(
            &socket,
            &base,
            Some("account".into()),
            now,
        ))
        .unwrap();
        assert_eq!(result, case["result"] == true, "result {i}");
        assert_eq!(
            json!(*runtime.wire.0.borrow()),
            case["messages"],
            "wire {i}"
        );
        let errors: Vec<String> = parent
            .events
            .borrow()
            .iter()
            .filter(|(kind, _)| kind == "error")
            .map(|(_, message)| message.clone())
            .collect();
        assert_eq!(json!(errors), case["errors"], "diagnostics {i}");
        let requests = signer.requests.borrow().clone();
        assert_eq!(json!(requests), case["requests"], "envelope {i}");
        base.dispose().unwrap();
        socket.dispose();
    }
}

#[test]
fn remote_auth_wrong_variant_and_pending_drop_never_transmit() {
    for pending in [false, true] {
        let (socket, parent, runtime) = fixture();
        ready(socket.create_websocket("wss://stream.bybit.com".into(), vec![], Default::default()))
            .unwrap();
        let signer = Rc::new(AuthSigner {
            native_extra_data: RefCell::default(),
            requests: RefCell::new(vec![]),
            response: "{}".into(),
            wrong_variant: !pending,
            pending,
        });
        let mut base = safe_cex_exchange::exchange::BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            parent.options.clone(),
            parent.store.clone(),
            runtime.clone(),
            Default::default(),
        );
        base.signers.remote_signer = Some(signer.clone());
        let mut future = Box::pin(crate::ws_private::local_auth::auth(
            &socket, &base, None, 1000.,
        ));
        let result = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        if pending {
            assert!(result.is_pending());
        } else {
            assert_eq!(
                match result {
                    Poll::Ready(Err(error)) => error.to_string(),
                    _ => panic!("wrong signer kind must refuse"),
                },
                "Bybit WS auth received HTTP signer response"
            );
        }
        drop(future);
        assert_eq!(signer.requests.borrow().len(), 1);
        assert!(runtime.wire.0.borrow().is_empty());
        base.dispose().unwrap();
        socket.dispose();
    }
}

mod api_client;

mod reconnect_sequence;

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn auth_declares_shared_response_and_native_expiry_boundaries() {
    let run = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-auth-fixtures.mjs"
        ))
        .arg("--boundaries")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let fixture: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(fixture["boundaries"].as_array().unwrap().len(), 15);
    for case in fixture["boundaries"].as_array().unwrap() {
        let (socket, parent, runtime) = self::fixture();
        let signer = Rc::new(AuthSigner {
            native_extra_data: RefCell::default(),
            requests: RefCell::default(),
            response: case["responseText"].as_str().unwrap().into(),
            wrong_variant: false,
            pending: false,
        });
        let mut base = safe_cex_exchange::exchange::BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            parent.options.clone(),
            parent.store.clone(),
            runtime.clone(),
            Default::default(),
        );
        base.signers.remote_signer = Some(signer);
        let result = ready(crate::ws_private::local_auth::auth(
            &socket,
            &base,
            Some("account".into()),
            1000.,
        ));
        if case["refused"] == true {
            assert!(
                result.is_err(),
                "shared response refusal: {}",
                case["label"]
            );
            assert!(
                runtime.wire.0.borrow().is_empty(),
                "refusal must not transmit: {}",
                case["label"]
            );
            assert!(
                parent
                    .events
                    .borrow()
                    .iter()
                    .all(|(kind, _)| kind != "error"),
                "decode refusal must not become a venue-error reply"
            );
            assert!(
                case["result"].is_boolean(),
                "source consumes malformed response without decoder refusal"
            );
        } else {
            assert_eq!(result.unwrap(), case["result"] == true, "{}", case["label"]);
            assert_eq!(
                json!(*runtime.wire.0.borrow()),
                case["messages"],
                "full auth wire: {}",
                case["label"]
            );
        }
        base.dispose().unwrap();
        socket.dispose();
    }
    assert_eq!(
        fixture["expires"].as_array().unwrap().len(),
        3,
        "AUTH-CARDINALITY expiry cases"
    );
    for case in fixture["expires"].as_array().unwrap() {
        let (socket, parent, runtime) = self::fixture();
        let signer = Rc::new(AuthSigner {
            native_extra_data: RefCell::default(),
            requests: RefCell::default(),
            response: r#"{"wsAuthSignature":"signed"}"#.into(),
            wrong_variant: false,
            pending: false,
        });
        let mut base = safe_cex_exchange::exchange::BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            parent.options.clone(),
            parent.store.clone(),
            runtime,
            Default::default(),
        );
        base.signers.remote_signer = Some(signer.clone());
        let now = case["now"].as_str().unwrap().parse::<f64>().unwrap();
        assert!(
            ready(crate::ws_private::local_auth::auth(
                &socket, &base, None, now
            ))
            .unwrap()
        );
        assert_eq!(
            signer.native_extra_data.borrow().as_slice(),
            [r#"{"expires":null}"#],
            "host observes null in native RawValue before serialization"
        );
        assert!(
            signer.requests.borrow()[0]["wsAuth"]["extraData"]["expires"].is_null(),
            "Rust raw metadata collapses nonfinite expiry"
        );
        assert!(
            matches!(
                case["kind"].as_str(),
                Some("Infinity" | "-Infinity" | "NaN")
            ),
            "source host sees native nonfinite expiry before serialization"
        );
        assert_eq!(
            case["serialized"], "null",
            "wire comparison alone hides native metadata loss"
        );
        base.dispose().unwrap();
        socket.dispose();
    }
}

mod notification;
