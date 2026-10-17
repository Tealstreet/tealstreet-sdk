use safe_cex_exchange::{runtime::*, websocket::*};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_binance::spot_open::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
fn run<T>(f: impl Future<Output = T>) -> T {
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
type SendHook = Box<dyn Fn(&str)>;
struct Socket {
    hook: RefCell<Option<SendHook>>,
    sent: RefCell<Vec<String>>,
    trace: Rc<RefCell<Vec<Value>>>,
    fail: Cell<bool>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(std::future::pending())
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.trace
            .borrow_mut()
            .push(json!({"kind":"send","frame":message}));
        self.sent.borrow_mut().push(message.into());
        if let Some(hook) = self.hook.borrow().as_ref() {
            hook(message);
        }
        if self.fail.get() {
            let mut e = ExchangeError::new("fixture send");
            e.name = "RangeError".into();
            e.metadata.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                "send-code"
            )));
            Err(e)
        } else {
            Ok(())
        }
    }
    fn close(&self) {
        self.trace.borrow_mut().push(json!({"kind":"close"}));
    }
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
struct Runtime {
    diagnostics: bool,
    now: Cell<f64>,
    socket: Rc<Socket>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        Box::new(Task)
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "fixture".into()
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
        let s: Rc<dyn WsSocket> = self.socket.clone();
        Box::pin(async move { Ok(s) })
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
        self.diagnostics
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Parent {
    trace: Rc<RefCell<Vec<Value>>>,
    diagnostic_frames: RefCell<Vec<String>>,
    disposed: Cell<bool>,
    options: ExchangeOptions,
    store: Rc<DefaultStore>,
    latency: RefCell<Option<f64>>,
    emit_failure: RefCell<Option<ExchangeError>>,
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "binance"
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
        true
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        Rc::new(ReopenResync::default())
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        panic!("unexpected snapshot")
    }
    fn log(&self, _: &str, _: LogSeverity) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        if event == "grafana-pws-sub" {
            self.trace.borrow_mut().push(json!({"kind":"diagnostic"}));
            self.diagnostic_frames.borrow_mut().push(message.into());
        }
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn update_latency(&self, n: f64) -> Result<(), ExchangeError> {
        *self.latency.borrow_mut() = Some(n);
        Ok(())
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_and_log_error(
        &self,
        message: &str,
        _: Option<&str>,
        _: Option<&Value>,
    ) -> Result<(), ExchangeError> {
        self.trace
            .borrow_mut()
            .push(json!({"kind":"error","message":message}));
        self.emit_failure.borrow().clone().map_or(Ok(()), Err)
    }
}
struct Protocol;
impl WsProtocol for Protocol {}
fn params(outcome: &str) -> Result<safe_cex_num::JsJsonValue, ExchangeError> {
    use safe_cex_num::JsJsonValue as J;
    Ok(match outcome {
        "object" => {
            json!({"apiKey":"key","recvWindow":5000,"timestamp":100,"signature":"sig"}).into()
        }
        "undefined" => J::Undefined,
        "null" => J::Null,
        "array" => json!([1, null, "💠"]).into(),
        "text" => J::String("value".into()),
        "nan" => J::Object(vec![("timestamp".into(), J::Number(f64::NAN))]),
        "infinity" => J::Object(vec![("timestamp".into(), J::Number(f64::INFINITY))]),
        "bigint" => J::Object(vec![("signature".into(), J::BigInt(10.into()))]),
        "failure" => return Err(ExchangeError::new("fixture signer")),
        "blankFailure" => return Err(ExchangeError::new("")),
        _ => unreachable!(),
    })
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn spot_open_matches_source_at_pending_boundary() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-open.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 640);
    for (index, c) in cases.into_iter().enumerate() {
        let trace = Rc::new(RefCell::new(vec![]));
        let native = Rc::new(Socket {
            hook: RefCell::new(None),
            sent: RefCell::new(vec![]),
            trace: trace.clone(),
            fail: Cell::new(c["failSend"] == true),
        });
        let runtime = Rc::new(Runtime {
            diagnostics: false,
            now: Cell::new(100.0),
            socket: native,
        });
        let parent = Rc::new(Parent {
            trace: trace.clone(),
            diagnostic_frames: RefCell::new(vec![]),
            disposed: Cell::new(false),
            options: serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
            store: Rc::new(DefaultStore::new()),
            latency: RefCell::new(None),
            emit_failure: RefCell::new(None),
        });
        let socket = BaseWebSocket::new(1, parent.clone(), runtime, Rc::new(Protocol));
        run(socket.create_websocket(
            "wss://fixture.invalid".into(),
            vec![],
            Some(WebSocketConnectionOptions::default()),
        ))
        .unwrap();
        parent.disposed.set(c["disposed"] == true);
        let attempt = Cell::new(match c["initial"].as_str().unwrap() {
            "zero" => 0.0,
            "large" => 9007199254740992.0,
            "nan" => f64::NAN,
            "infinity" => f64::INFINITY,
            _ => unreachable!(),
        });
        let ready = Cell::new(false);
        let build = || {
            trace.borrow_mut().push(json!({"kind":"build"}));
            async {
                std::future::poll_fn(|_| {
                    if ready.get() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                })
                .await;
                params(c["outcome"].as_str().unwrap())
            }
        };
        let mut future = Box::pin(open_binance_spot_private(&socket, &attempt, build));
        let polled = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        assert_eq!(
            matches!(&polled, Poll::Ready(Ok(()))),
            c["disposed"] == true,
            "initial poll {index}"
        );
        assert_eq!(
            polled.is_pending(),
            c["disposed"] != true,
            "active signer must remain pending case {index}"
        );
        assert_eq!(
            json!({"trace":*trace.borrow(),"attempt":safe_cex_num::js_number_string(attempt.get())}),
            c["before"],
            "before await case {index}: {c}"
        );
        match c["action"].as_str().unwrap() {
            "dispose" => parent.disposed.set(true),
            "advance" => attempt.set(attempt.get() + 1.0),
            "reset" => attempt.set(0.0),
            _ => {}
        }
        ready.set(true);
        if polled.is_pending() {
            assert!(
                matches!(
                    future
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop())),
                    Poll::Ready(Ok(()))
                ),
                "caught source failure must resolve successfully case {index}"
            );
        }
        assert_eq!(
            json!({"trace":*trace.borrow(),"attempt":safe_cex_num::js_number_string(attempt.get()),"forceClosed":socket.is_force_closed()}),
            json!({"trace":c["trace"],"attempt":c["attempt"],"forceClosed":c["forceClosed"]}),
            "spot onOpen case {index}: {c}"
        );
    }
}

fn connected_fixture() -> (Rc<BaseWebSocket>, Rc<RefCell<Vec<Value>>>, Rc<Parent>) {
    let trace = Rc::new(RefCell::new(vec![]));
    let native = Rc::new(Socket {
        hook: RefCell::new(None),
        sent: RefCell::new(vec![]),
        trace: trace.clone(),
        fail: Cell::new(false),
    });
    let runtime = Rc::new(Runtime {
        diagnostics: false,
        now: Cell::new(100.0),
        socket: native,
    });
    let parent = Rc::new(Parent {
        trace: trace.clone(),
        diagnostic_frames: RefCell::new(vec![]),
        disposed: Cell::new(false),
        options: serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
        store: Rc::new(DefaultStore::new()),
        latency: RefCell::new(None),
        emit_failure: RefCell::new(None),
    });
    let socket = BaseWebSocket::new(1, parent.clone(), runtime, Rc::new(Protocol));
    run(socket.create_websocket(
        "wss://fixture.invalid".into(),
        vec![],
        Some(WebSocketConnectionOptions::default()),
    ))
    .unwrap();
    (socket, trace, parent)
}

#[test]
fn dropped_pending_signer_retains_attempt_and_allows_next_open() {
    let (socket, trace, _) = connected_fixture();
    let attempt = Cell::new(0.0);
    let mut abandoned = Box::pin(open_binance_spot_private(&socket, &attempt, || {
        std::future::pending::<Result<safe_cex_num::JsJsonValue, ExchangeError>>()
    }));
    assert!(
        abandoned
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(attempt.get(), 1.0, "pending signer advances attempt");
    drop(abandoned);
    assert_eq!(attempt.get(), 1.0, "drop retains attempt");
    assert!(
        trace.borrow().is_empty(),
        "drop does not send, log or close"
    );
    run(open_binance_spot_private(&socket, &attempt, || async {
        params("object")
    }))
    .unwrap();
    assert_eq!(attempt.get(), 2.0, "next invocation advances normally");
    assert_eq!(trace.borrow().len(), 1, "next invocation is not latched");
    assert_eq!(trace.borrow()[0]["kind"], "send");
    assert!(!socket.is_force_closed());
}

#[test]
fn overlapping_signers_only_latest_success_sends_but_stale_failure_closes() {
    for first_fails in [false, true] {
        for latest_finishes_first in [false, true] {
            let (socket, trace, _) = connected_fixture();
            let attempt = Cell::new(0.0);
            let first_ready = Cell::new(false);
            let latest_ready = Cell::new(false);
            let mut first = Box::pin(open_binance_spot_private(&socket, &attempt, || async {
                std::future::poll_fn(|_| {
                    if first_ready.get() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                })
                .await;
                params(if first_fails { "failure" } else { "object" })
            }));
            let mut latest = Box::pin(open_binance_spot_private(&socket, &attempt, || async {
                std::future::poll_fn(|_| {
                    if latest_ready.get() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                })
                .await;
                params("object")
            }));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(first.as_mut().poll(&mut cx).is_pending());
            assert!(latest.as_mut().poll(&mut cx).is_pending());
            assert_eq!(attempt.get(), 2.0);
            if latest_finishes_first {
                latest_ready.set(true);
                assert!(matches!(latest.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
                first_ready.set(true);
                assert!(matches!(first.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
            } else {
                first_ready.set(true);
                assert!(matches!(first.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
                latest_ready.set(true);
                assert!(matches!(latest.as_mut().poll(&mut cx), Poll::Ready(Ok(()))));
            }
            let kinds: Vec<String> = trace
                .borrow()
                .iter()
                .map(|v| v["kind"].as_str().unwrap().into())
                .collect();
            let expected = if first_fails {
                if latest_finishes_first {
                    vec!["send", "error", "close"]
                } else {
                    vec!["error", "close", "send"]
                }
            } else {
                vec!["send"]
            };
            assert_eq!(
                kinds, expected,
                "overlap first_fails={first_fails} latest_first={latest_finishes_first}"
            );
            assert_eq!(socket.is_force_closed(), first_fails);
        }
    }
}

#[test]
fn catch_emitter_failure_escapes_unchanged_and_skips_force_close() {
    for emitter_throws in [false, true] {
        for outcome in ["failure", "blankFailure", "bigint"] {
            let (socket, trace, parent) = connected_fixture();
            let mut sentinel = ExchangeError::new("original emitter failure");
            sentinel.name = "SyntaxError".into();
            sentinel.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                "emitter-code"
            )));
            sentinel.response = Some(json!({"data":{"msg":"emitter-response"}}));
            if emitter_throws {
                *parent.emit_failure.borrow_mut() = Some(sentinel.clone());
            }
            let attempt = Cell::new(0.0);
            let result = run(open_binance_spot_private(&socket, &attempt, || async {
                params(outcome)
            }));
            if emitter_throws {
                assert_eq!(
                    result,
                    Err(sentinel),
                    "catch preserves original emitter error: {outcome}"
                );
            } else {
                assert_eq!(result, Ok(()), "ordinary signing catch resolves: {outcome}");
            }
            assert_eq!(
                socket.is_force_closed(),
                !emitter_throws,
                "force close occurs only after successful error emission: {outcome}"
            );
            let trace = trace.borrow();
            let kinds: Vec<_> = trace
                .iter()
                .map(|event| event["kind"].as_str().unwrap())
                .collect();
            assert_eq!(
                kinds,
                if emitter_throws {
                    vec!["error"]
                } else {
                    vec!["error", "close"]
                },
                "catch emitter precedes host close: {outcome}"
            );
            if outcome == "blankFailure" {
                assert_eq!(
                    trace[0]["message"],
                    "Binance Spot user-data subscription signing failed"
                );
            }
        }
    }
}
