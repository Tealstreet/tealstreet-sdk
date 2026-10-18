//! Scripted transport and runtime for driving the production OkxExchange.
#[path = "../../../test-support/release.rs"]
pub mod release;
mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
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
use safe_cex_venue_okx::binding::exchange::{OkxDependencies, OkxExchange};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

#[derive(Default)]
pub struct Runtime {
    sleep_wakers: virtual_time::SleepWakers,
    pub now: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Job>>,
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
    fn report_raw_exception(&self, error: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {
        self.reports.borrow_mut().push(safe_cex_exchange::runtime::exception_text(error));
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
        panic!("OKX binding E2E never tunnels")
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
}

/// Scripted HTTP: (method, path) -> queue of responses; the last one repeats.
#[derive(Default)]
pub struct Http {
    pub runtime: RefCell<Option<Rc<Runtime>>>,
    pub routes: RefCell<Vec<(String, String, VecDeque<(f64, Value)>)>>,
    pub requests: RefCell<Vec<Request>>,
    pub network_failures: RefCell<HashMap<String, VecDeque<String>>>,
    pub instrument_responses: RefCell<HashMap<(String, String), Value>>,
    pub held_instrument: RefCell<Option<(String, String, Rc<release::Release>)>>,
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
            json!({"code": "0", "msg": "", "data": data}),
        );
    }
    pub fn ok_for_instrument(&self, path: &str, instrument: &str, data: Value) {
        self.instrument_responses.borrow_mut().insert(
            (path.into(), instrument.into()),
            json!({"code":"0","msg":"","data":data}),
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
        self.requests.borrow_mut().push(Request {
            method: method.clone(),
            url: url.clone(),
            params: read("params"),
            // Signed bodies are already JSON text on the wire.
            data: match read("data") {
                Value::String(text) => serde_json::from_str(&text).unwrap_or(Value::String(text)),
                other => other,
            },
            headers: read("headers"),
        });
        if let Some(code) = self
            .network_failures
            .borrow_mut()
            .get_mut(&url)
            .and_then(VecDeque::pop_front)
        {
            let error = Thrown::error("Error", "scripted network failure");
            error
                .value
                .define("code", Js::str(&code))
                .expect("network code");
            error
                .value
                .define("config", config)
                .expect("original request config");
            return Box::pin(async move { Err(error) });
        }
        let held = self
            .held_instrument
            .borrow()
            .as_ref()
            .and_then(|(path, instrument, gate)| {
                (url == *path && read("params")["instType"] == *instrument).then(|| gate.clone())
            });
        let instrument = read("params")["instType"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let selected = self
            .instrument_responses
            .borrow()
            .get(&(url.clone(), instrument))
            .cloned();
        let response = if let Some(body) = selected {
            Some((200.0, body))
        } else {
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
        Box::pin(async move {
            if let Some(gate) = held {
                gate.wait().await;
            }
            result
        })
    }
}

pub struct Harness {
    pub runtime: Rc<Runtime>,
    pub http: Rc<Http>,
    pub store: Rc<safe_cex_store::store::DefaultStore>,
    pub exchange: Rc<OkxExchange>,
}

pub fn options() -> ExchangeOptions {
    serde_json::from_value(
        json!({"key": "fixture-key", "secret": "fixture-secret", "passphrase": "fixture-pass"}),
    )
    .expect("OKX options")
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
    let exchange = OkxExchange::new(
        name,
        options,
        store.clone(),
        OkxDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            external_public_data: None,
        },
    )
    .expect("OkxExchange constructs from real options");
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
