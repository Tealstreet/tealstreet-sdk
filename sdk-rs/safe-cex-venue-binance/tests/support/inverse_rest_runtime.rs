#![allow(
    dead_code,
    reason = "shared scripted host supports independently scoped inverse binding tests"
)]
mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
// Scripted native HTTP and WS host; no venue operation is mocked.
// Accepted-set instrumentation includes this same host inside the library.
#[cfg(not(test))]
use crate as safe_cex_venue_binance;
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody, TransportProvider,
    socket::TunnelHost,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_binance::inverse::{BinanceInverseDependencies, BinanceInverseExchange};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

#[derive(Default)]
pub struct Runtime {
    pub transport_provider: RefCell<Option<Arc<dyn TransportProvider>>>,
    sleep_wakers: virtual_time::SleepWakers,
    pub now: Rc<Cell<f64>>,
    pub id: RefCell<Option<String>>,
    jobs: RefCell<Vec<Job>>,
    pub sockets: RefCell<Vec<(String, Rc<Socket>)>>,
    pub reports: RefCell<Vec<String>>,
    pub dials: RefCell<Vec<(String, WebSocketConnectionOptions)>>,
    pub headers_capable: Cell<bool>,
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
    pub fn advance(&self, ms: f64) {
        self.now.set(self.now.get() + ms);
        self.pump();
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
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        self.id
            .borrow()
            .clone()
            .unwrap_or_else(|| "00000000-0000-4000-8000-000000000000".into())
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.reports.borrow_mut().push(error.message.clone());
    }
    fn report_raw_exception(
        &self,
        error: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
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
        self.transport_provider
            .borrow()
            .clone()
            .unwrap_or_else(|| Arc::new(safe_cex_transport::DirectTransport))
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("WOO binding E2E never tunnels")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        options: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.dials.borrow_mut().push((url.clone(), options));
        let socket = Rc::new(Socket::default());
        socket.events.borrow_mut().push_back(WsEvent::Open);
        self.sockets.borrow_mut().push((url, socket.clone()));
        Box::pin(async move { Ok(socket as Rc<dyn WsSocket>) })
    }
    fn headers_capable(&self) -> bool {
        self.headers_capable.get()
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

#[derive(Default)]
pub struct Http {
    pub sent: Mutex<Vec<RestRequest>>,
    routes: Mutex<Vec<(String, String, VecDeque<(u16, Value)>)>>,
    encoded: Mutex<Vec<(String, String, Vec<u8>)>>,
}
impl Http {
    pub fn encoded(&self, method: &str, path: &str, body: &[u8]) {
        self.encoded
            .lock()
            .unwrap()
            .push((method.into(), path.into(), body.to_vec()));
    }
    pub fn route(&self, method: &str, path: &str, status: u16, value: Value) {
        let mut routes = self.routes.lock().unwrap();
        if let Some((_, _, queue)) = routes.iter_mut().find(|(m, p, _)| m == method && p == path) {
            queue.push_back((status, value));
        } else {
            routes.push((
                method.into(),
                path.into(),
                VecDeque::from([(status, value)]),
            ));
        }
    }
    pub fn ok(&self, method: &str, path: &str, data: Value) {
        self.route(method, path, 200, data);
    }
    pub fn replace(&self, method: &str, path: &str, data: Value) {
        self.routes
            .lock()
            .unwrap()
            .retain(|(m, p, _)| m != method || p != path);
        self.ok(method, path, data);
    }
    pub fn calls(&self, path: &str) -> Vec<RestRequest> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.url.split('?').next() == Some(path))
            .cloned()
            .collect()
    }
}
impl RestAdapter for Http {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.sent.lock().unwrap().push(request.clone());
        let path = request.url.split('?').next().unwrap();
        if let Some((_, _, body)) = self
            .encoded
            .lock()
            .unwrap()
            .iter()
            .find(|(method, p, _)| request.method.eq_ignore_ascii_case(method) && p == path)
        {
            let response = RestResponse {
                status: 200,
                reason: None,
                headers: [("content-type".into(), "application/json".into())].into(),
                body: RestResponseBody::Encoded(body.clone()),
            };
            return Box::pin(async move { Ok(response) });
        }
        let mut routes = self.routes.lock().unwrap();
        let route = routes
            .iter_mut()
            .find(|(method, p, _)| request.method.eq_ignore_ascii_case(method) && p == path);
        let result = match route {
            Some((_, _, queue)) => {
                let (status, value) = if queue.len() > 1 {
                    queue.pop_front().unwrap()
                } else {
                    queue.front().unwrap().clone()
                };
                Ok(RestResponse {
                    status,
                    reason: None,
                    headers: [("content-type".into(), "application/json".into())].into(),
                    body: RestResponseBody::Encoded(serde_json::to_vec(&value).unwrap()),
                })
            }
            None => panic!("unscripted {} {}", request.method, request.url),
        };
        Box::pin(async move { result })
    }
}
pub struct Harness {
    pub runtime: Rc<Runtime>,
    pub http: Arc<Http>,
    pub store: Rc<safe_cex_exchange::native_store::DefaultStore>,
    pub exchange: Rc<BinanceInverseExchange>,
}
pub fn options() -> ExchangeOptions {
    serde_json::from_value(json!({"key":"synthetic-key","secret":"synthetic-secret","passphrase":"synthetic-passphrase"})).unwrap()
}
pub fn harness(options: ExchangeOptions, script: impl FnOnce(&Http)) -> Harness {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(1700000000000.0);
    runtime.headers_capable.set(true);
    let http = Arc::new(Http::default());
    script(&http);
    let host = safe_cex_exchange::js_runtime::client::rest_http_host(runtime.clone(), http.clone());
    let store = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
    let exchange = BinanceInverseExchange::new(
        ExchangeName::Binanceinverse,
        options,
        store.clone(),
        BinanceInverseDependencies {
            runtime: runtime.clone(),
            http: host,
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
impl Harness {
    pub fn drive<T>(&self, mut future: ExchangeFuture<'_, T>) -> Result<T, ExchangeError> {
        for _ in 0..2000 {
            if let Poll::Ready(result) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                return result;
            }
            self.runtime.pump();
            self.runtime.advance(50.0);
        }
        panic!("venue did not settle")
    }
}
