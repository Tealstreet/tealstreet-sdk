use safe_cex_exchange::{runtime::*, websocket::*};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_binance::public_open::*;
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
        if let Some(hook) = self.hook.borrow().as_ref() {
            hook(message);
        }
        if self.fail.get() {
            let mut e = ExchangeError::new("fixture send");
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
fn snapshot(socket: &BaseWebSocket) -> Value {
    let books: serde_json::Map<String, Value> = socket
        .order_book_inner_state
        .borrow()
        .iter()
        .map(|(key, book)| {
            (
                key.clone(),
                json!({"updates":book.updates,"isSnapshotLoaded":book.is_snapshot_loaded}),
            )
        })
        .collect();
    Value::Object(books)
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_open_matches_source_through_core() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-open.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 480);
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
            now: Cell::new(0.0),
            socket: native.clone(),
            trace: trace.clone(),
        });
        let parent = Rc::new(Parent {
            trace: trace.clone(),
            diagnostic_frames: RefCell::new(vec![]),
            disposed: Cell::new(false),
            options: serde_json::from_value(
                json!({"key":"k","secret":"s","publicDataOnly":c["publicOnly"]}),
            )
            .unwrap(),
            store: Rc::new(DefaultStore::new()),
            latency: RefCell::new(None),
        });
        let socket = BaseWebSocket::new(1, parent.clone(), runtime, Rc::new(Protocol));
        run(socket.create_websocket(
            "wss://fixture.invalid".into(),
            vec![],
            Some(WebSocketConnectionOptions::default()),
        ))
        .unwrap();
        for (key, book) in c["initial"].as_object().unwrap() {
            socket.order_book_inner_state.borrow_mut().insert(
                key.clone(),
                OrderBookInnerState {
                    updates: book["updates"].as_array().unwrap().clone(),
                    is_snapshot_loaded: book["isSnapshotLoaded"] == true,
                },
            );
        }
        parent.disposed.set(c["disposed"] == true);
        let calls = Rc::new(RefCell::new(Vec::<Value>::new()));
        let weak = Rc::downgrade(&socket);
        let send_calls = calls.clone();
        let send_parent = parent.clone();
        let dispose_on_send = c["disposeOnSend"] == true;
        *native.hook.borrow_mut() = Some(Box::new(move |frame| {
            send_calls.borrow_mut().push(
                json!({"kind":"send","frame":frame,"books":snapshot(&weak.upgrade().unwrap())}),
            );
            if dispose_on_send {
                send_parent.disposed.set(true);
            }
        }));
        let subscribe = || {
            calls.borrow_mut().push(json!({"kind":"subscribe","disposed":parent.disposed.get(),"books":snapshot(&socket)}));
            if c["failSubscribe"] == true {
                Err(ExchangeError::new("fixture subscribe"))
            } else {
                Ok(())
            }
        };
        let result = match c["variant"].as_str().unwrap() {
            "market" => open_binance_market(&socket, subscribe),
            "pm-market" => open_binance_pm_market(&socket, subscribe),
            "public" => open_binance_public(&socket, c["publicOnly"] == true, subscribe),
            "pm-public" => open_binance_pm_public(&socket, c["publicOnly"] == true, subscribe),
            "spot" => open_binance_spot_public(&socket, subscribe),
            _ => unreachable!(),
        };
        let error = result
            .err()
            .map(|e| json!({"name":e.name,"message":e.message}));
        assert_eq!(json!(error), c["error"], "open error case {index}");
        assert_eq!(
            json!(*calls.borrow()),
            c["trace"],
            "open operation order case {index}"
        );
        assert_eq!(
            snapshot(&socket),
            c["books"],
            "open final books case {index}"
        );
    }
}
