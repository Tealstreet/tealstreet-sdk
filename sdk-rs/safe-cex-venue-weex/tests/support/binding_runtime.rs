mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
// Scripted native HTTP and WS host; no venue operation is mocked.
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody, TransportProvider,
    socket::TunnelHost,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_weex::{WeexDependencies, WeexExchange};
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
        Arc::new(safe_cex_transport::DirectTransport)
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
    held: Mutex<Option<Arc<Hold>>>,
    pub sent: Mutex<Vec<RestRequest>>,
    routes: Mutex<Vec<(String, String, VecDeque<(u16, RestResponseBody)>)>>,
}
#[derive(Default)]
pub struct Hold {
    released: std::sync::atomic::AtomicBool,
    wakers: Mutex<Vec<Waker>>,
}
impl Hold {
    pub fn release(&self) {
        self.released.store(true, std::sync::atomic::Ordering::SeqCst);
        for waker in self.wakers.lock().unwrap().drain(..) { waker.wake(); }
    }
}
impl Http {
    pub fn hold_responses(&self) -> Arc<Hold> {
        let hold=Arc::new(Hold::default());
        *self.held.lock().unwrap()=Some(hold.clone());
        hold
    }
    pub fn route(&self, method: &str, path: &str, status: u16, value: Value) {
        self.route_body(
            method,
            path,
            status,
            RestResponseBody::Encoded(serde_json::to_vec(&value).unwrap()),
        );
    }
    pub fn route_body(&self, method: &str, path: &str, status: u16, value: RestResponseBody) {
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
    pub fn replace_body(&self, path: &str, body: RestResponseBody) {
        self.routes.lock().unwrap().retain(|(_, p, _)| p != path);
        self.route_body("GET", path, 200, body);
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
                    body: value,
                })
            }
            None => panic!("unscripted {} {}", request.method, request.url),
        };
        let hold=self.held.lock().unwrap().clone();
        let mut result=Some(result);
        Box::pin(std::future::poll_fn(move |cx| {
            if let Some(hold)=&hold {
                let mut wakers=hold.wakers.lock().unwrap();
                if !hold.released.load(std::sync::atomic::Ordering::SeqCst) {
                    wakers.push(cx.waker().clone());
                    return Poll::Pending;
                }
            }
            Poll::Ready(result.take().expect("terminal HTTP polled after settlement"))
        }))
    }
}
pub struct Harness {
    pub runtime: Rc<Runtime>,
    pub http: Arc<Http>,
    pub store: Rc<safe_cex_store::store::DefaultStore>,
    pub exchange: Rc<WeexExchange>,
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
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let exchange = WeexExchange::new(
        ExchangeName::Weex,
        options,
        store.clone(),
        WeexDependencies {
            runtime: runtime.clone(),
            http: host,
            external_public_data: None,
            local_wall_time_to_utc: Rc::new(|ms| ms),
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
    pub fn venue(&self) -> &dyn Exchange {
        self.exchange.as_ref()
    }
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

#[test]
fn pending_sleep_registers_the_latest_waker_and_cancels_on_drop() {
    let runtime = Runtime::default();
    let first = std::sync::Arc::new(virtual_time::WakeSignal::default());
    let latest = std::sync::Arc::new(virtual_time::WakeSignal::default());
    let mut future = runtime.sleep(10);
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&first.waker()))
            .is_pending()
    );
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&latest.waker()))
            .is_pending()
    );
    runtime.now.set(runtime.now.get() + 9.0);
    runtime.pump();
    assert!(
        !first.take() && !latest.take(),
        "sleep woke before deadline"
    );
    runtime.now.set(runtime.now.get() + 1.0);
    runtime.pump();
    assert!(
        !first.take() && latest.take(),
        "expired sleep must wake latest waiter"
    );
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&latest.waker()))
            .is_ready()
    );
    let mut cancelled = runtime.sleep(10);
    assert!(
        cancelled
            .as_mut()
            .poll(&mut Context::from_waker(&latest.waker()))
            .is_pending()
    );
    drop(cancelled);
    runtime.now.set(runtime.now.get() + 10.0);
    runtime.pump();
    assert!(!latest.take(), "dropped sleep must not wake");
}
