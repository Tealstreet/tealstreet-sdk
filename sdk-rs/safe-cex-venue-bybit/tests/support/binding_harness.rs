//! Scripted wire and runtime driving the production BybitExchange through `Exchange`.
#![allow(
    dead_code,
    reason = "each test binary uses a different part of the harness"
)]
#[path = "../../../test-support/release.rs"]
pub mod release;
mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
use safe_cex_exchange::{
    exchange::Exchange,
    http::{HttpFailure, HttpFuture, HttpHost},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody, TransportFuture,
    TransportProvider, rate_limit::RateLimitClock, socket::TunnelHost,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_bybit::{BybitDependencies, BybitExchange};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll, Waker},
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
    fn console(&self, level: &str, message: &str) {
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
        panic!("Bybit binding E2E never tunnels")
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

/// One request as it reached the wire, after signing.
#[derive(Clone, Debug)]
pub struct Request {
    pub config: RestRequest,
    pub method: String,
    pub url: String,
    pub params: Value,
    pub body: Value,
    pub headers: Value,
}

/// Scripted wire: (METHOD, path) -> queue of bodies; the last one repeats.
#[derive(Default)]
pub struct Wire {
    pub network_failures: RefCell<Vec<String>>,
    pub reject_http_errors: Cell<bool>,
    pub routes: RefCell<Vec<(String, String, VecDeque<(u16, Value)>)>>,
    /// (METHOD, path, query key, query value) routes, checked before the plain ones.
    pub param_routes: RefCell<Vec<(String, String, String, String, (u16, Value))>>,
    pub requests: RefCell<Vec<Request>>,
    /// Hold selected responses after dispatch; release explicitly from the test.
    pub response_gates: RefCell<Vec<(String, Rc<release::Release>)>>,
}
impl Wire {
    pub fn route(&self, method: &str, path: &str, status: u16, body: Value) {
        let mut routes = self.routes.borrow_mut();
        match routes.iter_mut().find(|(m, p, _)| m == method && p == path) {
            Some((_, _, queue)) => queue.push_back((status, body)),
            None => routes.push((method.into(), path.into(), VecDeque::from([(status, body)]))),
        }
    }
    pub fn route_when(&self, method: &str, path: &str, key: &str, value: &str, body: Value) {
        self.param_routes.borrow_mut().push((
            method.into(),
            path.into(),
            key.into(),
            value.into(),
            (200, body),
        ));
    }
    /// A `retCode: 0` envelope around `result`.
    pub fn ok(&self, method: &str, path: &str, result: Value) {
        self.route(
            method,
            path,
            200,
            json!({"retCode": 0, "retMsg": "OK", "result": result, "time": 1}),
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
/// The signed URL's query as a JSON object of strings.
fn query(url: &str) -> Value {
    let Some((_, query)) = url.split_once('?') else {
        return Value::Null;
    };
    Value::Object(
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .map(|(k, v)| (k.to_owned(), Value::String(v.to_owned())))
            .collect(),
    )
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let method = request.method.to_uppercase();
        let path = request.url.split('?').next().unwrap_or_default().to_owned();
        let body = request
            .body
            .as_ref()
            .and_then(|b| b.wire_bytes().ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        self.requests.borrow_mut().push(Request {
            config: request.clone(),
            method: method.clone(),
            url: path.clone(),
            params: request
                .params
                .clone()
                .unwrap_or_else(|| query(&request.url)),
            body,
            headers: json!(request.headers),
        });
        if self.network_failures.borrow().contains(&path) {
            return Box::pin(async move {
                Err(safe_cex_transport::RestFailure::from_transport(
                    safe_cex_transport::TransportError::Network(
                        safe_cex_transport::NetworkFailure::from_code(
                            "scripted network".into(),
                            Some("ERR_NETWORK".into()),
                        ),
                    ),
                    request,
                ))
            });
        }
        let params = request
            .params
            .clone()
            .unwrap_or_else(|| query(&request.url));
        let matched = self
            .param_routes
            .borrow()
            .iter()
            .find(|(m, p, k, v, _)| *m == method && *p == path && params[k.as_str()] == v.as_str())
            .map(|(.., reply)| reply.clone());
        let reply = matched.or_else(|| {
            let mut routes = self.routes.borrow_mut();
            routes
                .iter_mut()
                .find(|(m, p, _)| *m == method && *p == path)
                .map(|(_, _, queue)| {
                    if queue.len() > 1 {
                        queue.pop_front().expect("queued")
                    } else {
                        queue[0].clone()
                    }
                })
        });
        let gate = self
            .response_gates
            .borrow()
            .iter()
            .find(|(gated_path, _)| *gated_path == path)
            .map(|(_, gate)| gate.clone());
        let reject_http_errors = self.reject_http_errors.get();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.wait().await;
            }
            match reply {
                Some((status, body)) if reject_http_errors && status >= 400 => {
                    Err(safe_cex_transport::RestFailure {
                        code: Some("ERR_BAD_REQUEST".into()),
                        transport_error: None,
                        message: format!("Request failed with status code {status}"),
                        response: Some(Box::new(RestResponse {
                            status,
                            reason: None,
                            headers: Default::default(),
                            body: RestResponseBody::Encoded(body.to_string().into_bytes()),
                        })),
                        request: Box::new(request),
                        context: None,
                    })
                }
                Some((status, body)) => Ok(RestResponse {
                    status,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Encoded(body.to_string().into_bytes()),
                }),
                None => Err(safe_cex_transport::RestFailure::pre_transmit(
                    format!("unscripted request {method} {path}"),
                    request,
                )),
            }
        })
    }
}

struct Host;
impl HttpHost for Host {
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { adapter.execute_with_receipt(request).await })
    }
    fn enhance_error(&self, error: HttpFailure, _: &ExchangeName) -> HttpFailure {
        error
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &HttpFailure) {}
    fn mark_start(&self, _: &mut RestRequest) {}
    fn signing_capture(&self, _: &RestRequest) -> bool {
        false
    }
    fn is_development(&self) -> bool {
        false
    }
    fn is_main_window(&self) -> bool {
        false
    }
}

struct Clock(Rc<Cell<f64>>);
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        self.0.get() as u64
    }
    fn sleep_ms(&self, milliseconds: u64) -> TransportFuture<'_, ()> {
        self.0.set(self.0.get() + milliseconds as f64);
        Box::pin(async { Ok(()) })
    }
}

pub struct Harness {
    pub runtime: Rc<Runtime>,
    pub wire: Arc<Wire>,
    pub store: Rc<safe_cex_store::store::DefaultStore>,
    pub exchange: Rc<BybitExchange>,
}

pub fn options() -> ExchangeOptions {
    serde_json::from_value(json!({"key": "fixture-key", "secret": "fixture-secret"}))
        .expect("Bybit options")
}

#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local adapters, as in production"
)]
pub fn harness(options: ExchangeOptions, script: impl FnOnce(&Wire)) -> Harness {
    harness_as(ExchangeName::Bybit, options, script)
}

#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local adapters, as in production"
)]
pub fn harness_as(
    name: ExchangeName,
    options: ExchangeOptions,
    script: impl FnOnce(&Wire),
) -> Harness {
    configured_harness(name, options, None, None, script)
        .expect("BybitExchange constructs from real options")
}

pub fn try_harness_as(
    name: ExchangeName,
    options: ExchangeOptions,
    script: impl FnOnce(&Wire),
) -> Result<Harness, ExchangeError> {
    configured_harness(name, options, None, None, script)
}

pub fn harness_with_signer(
    name: ExchangeName,
    options: ExchangeOptions,
    signer: safe_cex_exchange::js_runtime::base_exchange::HostedSigner,
    script: impl FnOnce(&Wire),
) -> Harness {
    configured_harness(name, options, None, Some(signer), script).expect("hosted Bybit constructs")
}

pub fn harness_with_external(
    options: ExchangeOptions,
    factory: safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory,
    script: impl FnOnce(&Wire),
) -> Harness {
    configured_harness(ExchangeName::Bybit, options, Some(factory), None, script)
        .expect("BybitExchange constructs from real options")
}

#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local adapters, as in production"
)]
fn configured_harness(
    name: ExchangeName,
    options: ExchangeOptions,
    external_public_data: Option<
        safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory,
    >,
    remote_signer: Option<safe_cex_exchange::js_runtime::base_exchange::HostedSigner>,
    script: impl FnOnce(&Wire),
) -> Result<Harness, ExchangeError> {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(START_MS);
    let wire = Arc::new(Wire::default());
    script(&wire);
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let epoch = Arc::new(AtomicU64::new(START_MS as u64));
    let mut clock = VirtualClock::default();
    clock.now = Arc::new(move || epoch.load(Ordering::SeqCst) as f64);
    let exchange = BybitExchange::create(
        name,
        options,
        store.clone(),
        BybitDependencies {
            runtime: runtime.clone(),
            remote_signer,
            wire: wire.clone(),
            http_host: Rc::new(Host),
            rate_limit_clock: Arc::new(Clock(runtime.now.clone())),
            retry_clock: Rc::new(Clock(runtime.now.clone())),
            clock,
            external_public_data,
            relay_enabled: false,
        },
    )?;
    Ok(Harness {
        runtime,
        wire,
        store,
        exchange,
    })
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
            self.runtime.now.set(self.runtime.now.get() + 50.0);
        }
        panic!("venue future did not settle")
    }
    /// Advance virtual time in 50ms steps, running due timers and jobs.
    pub fn advance(&self, milliseconds: f64) {
        let steps = (milliseconds / 50.).ceil() as usize;
        for _ in 0..steps {
            self.runtime.now.set(self.runtime.now.get() + 50.0);
            self.runtime.pump();
        }
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
