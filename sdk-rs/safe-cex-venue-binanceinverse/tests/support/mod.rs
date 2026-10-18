//! Primitive clock/task/socket/HTTP fixture; venue construction and callbacks stay production.
#![allow(
    dead_code,
    reason = "each socket target selects its own primitive fixture operations"
)]
mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
use safe_cex_exchange::{
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, Thrown},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll},
};
pub const START_MS: f64 = 1_700_000_000_000.0;

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

#[derive(Default)]
pub struct Runtime {
    sleep_wakers: virtual_time::SleepWakers,
    next_id: Cell<u64>,
    pub now: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Job>>,
    pub sockets: RefCell<Vec<(String, Rc<Socket>)>>,
    pub reports: RefCell<Vec<String>>,
    pub logs: RefCell<Vec<String>>,
    /// Optional deterministic entropy stream for session/identity race witnesses.
    pub random_sequence: Cell<Option<u64>>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    pub fn pump(&self) {
        self.sleep_wakers.wake_due(self.now.get());
        for _ in 0..64 {
            let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
            if jobs.is_empty() {
                return;
            }
            let signal = std::sync::Arc::new(virtual_time::WakeSignal::default());
            let waker = signal.waker();
            let mut cx = Context::from_waker(&waker);
            let mut pending = vec![];
            let mut progressed = false;
            for (mut future, cancelled) in jobs {
                if cancelled.get() {
                    continue;
                }
                if future.as_mut().poll(&mut cx).is_pending() {
                    pending.push((future, cancelled));
                } else {
                    progressed = true;
                }
            }
            let added = !self.jobs.borrow().is_empty();
            self.jobs.borrow_mut().extend(pending);
            if !progressed && !added && !signal.take() {
                return;
            }
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
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        let now = self.now.clone();
        let at = now.get() + ms as f64;
        let sleeper = self.sleep_wakers.register(at);
        Box::pin(std::future::poll_fn(move |cx| {
            if now.get() >= at {
                Poll::Ready(Ok(()))
            } else {
                *sleeper.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let flag = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((future, flag.clone()));
        Box::new(Task(flag))
    }
    fn console_values(&self, level: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.logs.borrow_mut().push(format!("{level}: {message}"));
    }
    fn unique_id(&self) -> String {
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        format!("req-{id}")
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        if let Some(next) = self.random_sequence.get() {
            self.random_sequence.set(Some(next.wrapping_add(1)));
            (next % 64) as f64 / 64.0
        } else {
            0.5
        }
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.reports.borrow_mut().push(error.message.clone());
    }
    fn report_raw_exception(&self, error: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {
        self.reports
            .borrow_mut()
            .push(safe_cex_exchange::runtime::exception_text(error));
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}

#[derive(Default)]
pub struct Socket {
    pub sent: RefCell<Vec<String>>,
    pub events: Rc<RefCell<VecDeque<WsEvent>>>,
    pub closed: Cell<bool>,
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
                .map_or(Poll::Pending, |event| Poll::Ready(Ok(event)))
        }))
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(message.into());
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
        false
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("inverse socket fixture never tunnels")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let socket = Rc::new(Socket::default());
        socket.events.borrow_mut().push_back(WsEvent::Open);
        self.sockets.borrow_mut().push((url, socket.clone()));
        Box::pin(async move { Ok(socket as Rc<dyn WsSocket>) })
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
        3000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct Http {
    pub runtime: Rc<Runtime>,
    pub requests: Rc<RefCell<Vec<Js>>>,
    pub replies: Rc<RefCell<VecDeque<Result<Js, Thrown>>>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        self.requests.borrow_mut().push(config.clone());
        let reply = self
            .replies
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Err(Thrown::error("Error", "unscripted inverse HTTP request")));
        Box::pin(async move {
            let data = reply?;
            Ok(Js::object(vec![
                ("data", data),
                ("status", Js::Number(200.)),
                ("headers", Js::object(vec![])),
                ("config", config),
            ]))
        })
    }
}
pub struct Harness {
    pub runtime: Rc<Runtime>,
    pub http: Rc<Http>,
    pub store: Rc<safe_cex_store::store::DefaultStore>,
    pub exchange: Rc<safe_cex_venue_binanceinverse::BinanceInverseExchange>,
}
pub fn fixture(public_only: bool, testnet: bool) -> Harness {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(START_MS);
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        requests: Default::default(),
        replies: Default::default(),
    });
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let options=serde_json::from_value(serde_json::json!({"key":"fixture-key","secret":"fixture-secret","testnet":testnet,"publicDataOnly":public_only,"tuning":{"consoleLogging":false}})).unwrap();
    let exchange = safe_cex_venue_binanceinverse::BinanceInverseExchange::new(
        safe_cex_types::ExchangeName::Binanceinverse,
        options,
        store.clone(),
        safe_cex_venue_binanceinverse::BinanceInverseDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            external_public_data: None,
        },
    )
    .unwrap();
    Harness {
        runtime,
        http,
        store,
        exchange,
    }
}
