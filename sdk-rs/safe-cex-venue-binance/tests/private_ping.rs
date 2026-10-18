use safe_cex_exchange::{runtime::*, websocket::*};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_binance::private_ping::*;
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
struct Socket {
    sent: RefCell<Vec<String>>,
    trace: Rc<RefCell<Vec<&'static str>>>,
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
        self.trace.borrow_mut().push("send");
        self.sent.borrow_mut().push(message.into());
        if self.fail.get() {
            let mut e = ExchangeError::new("fixture socket send");
            e.metadata.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                "fixture-code"
            )));
            Err(e)
        } else {
            Ok(())
        }
    }
    fn close(&self) {}
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
    trace: Rc<RefCell<Vec<&'static str>>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.trace.borrow_mut().push("clock");
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
        _: &serde_json::Value,
    ) {
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
    trace: Rc<RefCell<Vec<&'static str>>>,
    diagnostic_frames: RefCell<Vec<String>>,
    disposed: Cell<bool>,
    options: ExchangeOptions,
    store: Rc<DefaultStore>,
    latency: RefCell<Option<f64>>,
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
            self.trace.borrow_mut().push("diagnostic");
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
        _: &str,
        _: Option<&str>,
        _: Option<&Value>,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
}
struct Protocol;
impl WsProtocol for Protocol {}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn private_ping_matches_live_source_through_core() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-ping.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 384);
    for (index, c) in cases.iter().enumerate() {
        let trace = Rc::new(RefCell::new(vec![]));
        let native = Rc::new(Socket {
            sent: RefCell::new(vec![]),
            trace: trace.clone(),
            fail: Cell::new(c["fail"] == true),
        });
        let runtime = Rc::new(Runtime {
            diagnostics: c["diagnostics"] == true,
            now: Cell::new(c["initial"].as_f64().unwrap()),
            socket: native.clone(),
            trace: trace.clone(),
        });
        let parent = Rc::new(Parent {
            trace: trace.clone(),
            diagnostic_frames: RefCell::new(vec![]),
            disposed: Cell::new(false),
            options: serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
            store: Rc::new(DefaultStore::new()),
            latency: RefCell::new(None),
        });
        let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
        run(socket.create_websocket(
            "wss://fixture.invalid".into(),
            vec![],
            Some(WebSocketConnectionOptions::default()),
        ))
        .unwrap();
        socket.mark_ping_sent();
        trace.borrow_mut().clear();
        runtime.now.set(c["time"].as_f64().unwrap());
        parent.disposed.set(c["disposed"] == true);
        let result = match c["variant"].as_str().unwrap() {
            "binance" => ping_binance_private(&socket),
            "binancepm" => ping_binance_pm_private(&socket),
            "binancespot" => ping_binance_spot_private(&socket),
            _ => unreachable!(),
        };
        let error = result
            .err()
            .map(|e| json!({"name":e.name,"message":e.message,"code":observed_error_code(&e.metadata.code)}));
        assert_eq!(json!(error), c["error"], "ping error case {index}");
        assert_eq!(
            json!(*native.sent.borrow()),
            c["sent"],
            "ping frame case {index}"
        );
        assert_eq!(
            json!(*trace.borrow()),
            c["trace"],
            "ping order case {index}"
        );
        assert_eq!(
            json!(*parent.diagnostic_frames.borrow()),
            c["diagnosticFrames"],
            "ping diagnostics case {index}"
        );
        runtime.now.set(10000.0);
        socket
            .handle_pong_event(5000)
            .expect("fixture pong notification succeeds");
        assert_eq!(
            *parent.latency.borrow(),
            c["latency"].as_f64(),
            "ping stored timestamp case {index}"
        );
    }
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
