// Scripted transport and runtime for driving the production BitgetExchange.
use safe_cex_exchange::js_runtime::{
    client::JsHttpHost,
    promise::JsFuture,
    value::{Js, Thrown, from_json, to_json},
};
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_bitget::{BitgetDependencies, BitgetExchange};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    sync::Arc,
    task::{Context, Poll, Waker},
};

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);
type Sleeper = (f64, Weak<RefCell<Option<Waker>>>);

#[derive(Default)]
pub struct Runtime {
    pub now: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Job>>,
    sleepers: RefCell<Vec<Sleeper>>,
    pub sockets: RefCell<Vec<(String, Rc<Socket>)>>,
    pub reports: RefCell<Vec<String>>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    pub fn pump(&self) {
        let mut ready = vec![];
        self.sleepers.borrow_mut().retain(|(deadline, sleeper)| {
            let Some(sleeper) = sleeper.upgrade() else {
                return false;
            };
            if self.now.get() >= *deadline
                && let Some(waker) = sleeper.borrow_mut().take()
            {
                ready.push(waker);
            }
            true
        });
        for waker in ready {
            waker.wake();
        }
        for _ in 0..64 {
            let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
            if jobs.is_empty() {
                return;
            }
            let mut cx = Context::from_waker(Waker::noop());
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
            if !progressed && !added {
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
        let sleeper = Rc::new(RefCell::new(None));
        self.sleepers
            .borrow_mut()
            .push((at, Rc::downgrade(&sleeper)));
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
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "00000000-0000-4000-8000-000000000000".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.reports.borrow_mut().push(error.message.clone());
    }
    fn report_raw_exception(&self, error: &Value, _: &Value) {
        self.reports.borrow_mut().push(error.to_string());
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
        panic!("Bitget binding E2E never tunnels")
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

/// One recorded HTTP request after the request interceptors ran.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub params: Value,
    pub data: Value,
    pub headers: Value,
    pub signing_clock_at_send: f64,
}

/// Scripted HTTP: (method, path) -> queue of responses; the last one repeats.
#[derive(Default)]
pub struct Http {
    pub runtime: RefCell<Option<Rc<Runtime>>>,
    pub routes: RefCell<Vec<(String, String, VecDeque<(f64, Value)>)>>,
    pub requests: RefCell<Vec<Request>>,
    pub advance_after_send_ms: Cell<f64>,
    pub advance_signing_clock_after_send_ms: Cell<f64>,
}
impl Http {
    pub fn route(&self, method: &str, path: &str, status: f64, body: Value) {
        let mut routes = self.routes.borrow_mut();
        match routes.iter_mut().find(|(m, p, _)| m == method && p == path) {
            Some((_, _, queue)) => queue.push_back((status, body)),
            None => routes.push((method.into(), path.into(), VecDeque::from([(status, body)]))),
        }
    }
    pub fn ok(&self, method: &str, path: &str, data: Value) {
        self.route(
            method,
            path,
            200.0,
            json!({"code": "00000", "msg": "success", "requestTime": 1, "data": data}),
        );
    }
    pub fn calls(&self, path: &str) -> Vec<Request> {
        self.requests
            .borrow()
            .iter()
            .filter(|r| r.url == path)
            .cloned()
            .collect()
    }
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.borrow().clone().expect("harness runtime")
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let read = |k: &str| {
            config
                .get(k)
                .ok()
                .map(|v| to_json(&v).unwrap_or(Value::Null))
                .unwrap_or(Value::Null)
        };
        let method = read("method").as_str().unwrap_or("get").to_uppercase();
        let url = read("url").as_str().unwrap_or_default().to_owned();
        let signing_clock_at_send =
            safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
        self.requests.borrow_mut().push(Request {
            method: method.clone(),
            url: url.clone(),
            params: read("params"),
            data: read("data"),
            headers: read("headers"),
            signing_clock_at_send,
        });
        if let Some(runtime) = self.runtime.borrow().as_ref() {
            runtime
                .now
                .set(runtime.now.get() + self.advance_after_send_ms.get());
        }
        let signing_advance = self.advance_signing_clock_after_send_ms.get();
        if signing_advance != 0.0 {
            safe_cex_store::virtual_clock::VIRTUAL_CLOCK
                .sync_from_server_time(signing_clock_at_send + signing_advance);
        }
        let response = {
            let mut routes = self.routes.borrow_mut();
            routes
                .iter_mut()
                .find(|(m, p, _)| *m == method && *p == url)
                .map(|(_, _, queue)| {
                    if queue.len() > 1 {
                        queue.pop_front().expect("queued")
                    } else {
                        queue[0].clone()
                    }
                })
        };
        let result = match response {
            Some((status, body)) => Ok(Js::object(vec![
                ("status", Js::Number(status)),
                ("data", from_json(&body)),
                ("headers", Js::object(vec![])),
            ])),
            None => Err(Thrown::error(
                "Error",
                &format!("unscripted request {method} {url}"),
            )),
        };
        Box::pin(async move { result })
    }
}

pub struct Harness {
    pub runtime: Rc<Runtime>,
    pub http: Rc<Http>,
    pub store: Rc<safe_cex_store::store::DefaultStore>,
    pub exchange: Rc<BitgetExchange>,
}

pub fn options() -> ExchangeOptions {
    serde_json::from_value(
        json!({"key": "fixture-key", "secret": "fixture-secret", "passphrase": "fixture-pass"}),
    )
    .expect("Bitget options")
}

pub fn harness(
    name: ExchangeName,
    options: ExchangeOptions,
    script: impl FnOnce(&Http),
) -> Harness {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(1_700_000_000_000.0);
    let http = Rc::new(Http::default());
    *http.runtime.borrow_mut() = Some(runtime.clone());
    script(&http);
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let exchange = BitgetExchange::new(
        name,
        options,
        store.clone(),
        BitgetDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            external_public_data: None,
        },
    )
    .expect("BitgetExchange constructs from real options");
    Harness {
        runtime,
        http,
        store,
        exchange,
    }
}

impl Harness {
    /// Poll a venue future, pumping runtime jobs and advancing virtual time.
    pub fn drive<T>(&self, future: ExchangeFuture<'_, T>) -> Result<T, ExchangeError> {
        let mut future = future;
        for _ in 0..2000 {
            if let Poll::Ready(v) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                return v;
            }
            self.runtime.pump();
            self.runtime.advance(50.0);
        }
        panic!("venue future did not settle")
    }
    pub fn venue(&self) -> &dyn Exchange {
        self.exchange.as_ref()
    }
}
