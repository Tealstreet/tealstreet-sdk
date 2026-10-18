mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
// Scripted I/O support; callers allow unused helpers at module scope.
use safe_cex_exchange::{
    exchange::ExchangeSettings,
    http::{HttpClientOptions, HttpFuture, HttpHost},
    js_runtime::base_exchange::BaseExchangeParts,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportFuture, TransportProvider, rate_limit::RateLimitClock, socket::TunnelHost,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_kucoin::kucoin_exchange::{
    KucoinDependencies, KucoinExchange, KucoinSpotExchange,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

/// Virtual-clock runtime: spawned work runs when `pump` polls it.
#[derive(Default)]
pub struct Runtime {
    sleep_wakers: virtual_time::SleepWakers,
    pub now: Rc<Cell<f64>>,
    pub jobs: RefCell<Vec<Job>>,
    pub errors: RefCell<Vec<String>>,
    pub console: RefCell<Vec<(String, String)>>,
    pub dials: RefCell<Vec<String>>,
    pub sockets: RefCell<Vec<Rc<Socket>>>,
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
    fn sleep(&self, d: u64) -> ExchangeFuture<'static, ()> {
        let now = self.now.clone();
        let at = now.get() + d as f64;
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
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let c = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((f, c.clone()));
        Box::new(Task(c))
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.console
            .borrow_mut()
            .push((severity.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        format!("id-{}", self.jobs.borrow().len())
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.375
    }
    fn report_raw_exception(&self, value: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {
        self.errors.borrow_mut().push(safe_cex_exchange::runtime::exception_text(value));
    }
    fn report_exception(&self, e: &ExchangeError, _: &Value) {
        self.errors.borrow_mut().push(e.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("Worker unused in native fixture")) })
    }
}
#[derive(Default)]
pub struct Socket {
    pub events: Rc<RefCell<VecDeque<WsEvent>>>,
    pub sent: RefCell<Vec<String>>,
    closed: Cell<bool>,
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
                .map_or(Poll::Pending, |e| Poll::Ready(Ok(e)))
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
        panic!("fixture refuses tunnel transport")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.dials.borrow_mut().push(url);
        let socket = Rc::new(Socket::default());
        socket.events.borrow_mut().push_back(WsEvent::Open);
        self.sockets.borrow_mut().push(socket.clone());
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
impl RateLimitClock for Runtime {
    fn now_ms(&self) -> u64 {
        self.now.get() as u64
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

/// Scripted venue: replies by `METHOD path`, records every request it receives.
#[derive(Default)]
pub struct Wire {
    pub requests: Mutex<Vec<RestRequest>>,
    pub bodies: Mutex<BTreeMap<String, Value>>,
}
impl RestAdapter for Wire {
    fn execute(&self, r: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(r.clone());
        let path = r.url.split('?').next().unwrap_or_default();
        let key = format!("{} {path}", r.method.to_uppercase());
        let body = self
            .bodies
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .unwrap_or_else(|| json!({"code":"404000","msg":format!("unscripted {key}")}));
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Untransformed(body),
            })
        })
    }
}
impl Wire {
    pub fn script(&self, method: &str, path: &str, body: Value) {
        self.bodies
            .lock()
            .unwrap()
            .insert(format!("{method} {path}"), body);
    }
    /// Requests since the last call, as (METHOD, path, KC-API-KEY header).
    pub fn take_keys(&self) -> Vec<(String, String, Option<String>)> {
        self.requests
            .lock()
            .unwrap()
            .drain(..)
            .map(|r| {
                let path = r.url.split('?').next().unwrap_or_default().to_owned();
                (
                    r.method.to_uppercase(),
                    path,
                    r.headers.get("KC-API-KEY").cloned(),
                )
            })
            .collect()
    }
    /// Requests since the last call, as (METHOD, path, params/body JSON).
    pub fn take(&self) -> Vec<(String, String, Value)> {
        self.requests
            .lock()
            .unwrap()
            .drain(..)
            .map(|r| {
                let data = r
                    .body
                    .as_ref()
                    .and_then(|b| match b {
                        safe_cex_transport::RestBody::Transformed(v) => Some(v.value().clone()),
                        safe_cex_transport::RestBody::Untransformed(v) => Some(v.clone()),
                        safe_cex_transport::RestBody::Encoded(bytes) => {
                            serde_json::from_slice(bytes).ok()
                        }
                    })
                    .unwrap_or(Value::Null);
                let (path, query) = r.url.split_once('?').unwrap_or((&r.url, ""));
                let data = if query.is_empty() {
                    data
                } else {
                    query_object(query)
                };
                (r.method.to_uppercase(), path.to_owned(), data)
            })
            .collect()
    }
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// Query string as a flat object of decoded string values.
fn query_object(query: &str) -> Value {
    Value::Object(
        query
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (decode(k), Value::String(decode(v)))
            })
            .collect(),
    )
}

struct Host;
impl HttpHost for Host {
    fn execute(&self, a: Arc<dyn RestAdapter>, r: RestRequest) -> HttpFuture<'_> {
        Box::pin(async move { a.execute_with_receipt(r).await })
    }
    fn enhance_error(&self, e: RestFailure, _: &ExchangeName) -> RestFailure {
        e
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn note_success(&self, _: &ExchangeName, _: &RestResponse) {}
    fn note_failure(&self, _: &ExchangeName, _: &RestFailure) {}
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

pub struct Fixture {
    pub runtime: Rc<Runtime>,
    pub store: Rc<DefaultStore>,
    pub wire: Arc<Wire>,
}

fn options() -> ExchangeOptions {
    serde_json::from_value(json!({
        "key":"fixture-key","secret":"fixture-secret","passphrase":"fixture-pass",
        "extra":{"tickInterval":1.0e12}
    }))
    .unwrap()
}

/// `options()` plus a configured read-only secondary key.
pub fn secondary_options() -> ExchangeOptions {
    serde_json::from_value(json!({
        "key":"fixture-key","secret":"fixture-secret","passphrase":"fixture-pass",
        "secondary_key":"secondary-key","secondary_secret":"secondary-secret",
        "secondary_passphrase":"secondary-pass",
        "extra":{"tickInterval":1.0e12}
    }))
    .unwrap()
}

#[allow(clippy::arc_with_non_send_sync)]
fn parts(
    name: ExchangeName,
    runtime: &Rc<Runtime>,
    options: ExchangeOptions,
) -> (Fixture, BaseExchangeParts, KucoinDependencies) {
    runtime.now.set(1_700_000_000_000.0);
    let store = Rc::new(DefaultStore::new());
    let wire = Arc::new(Wire::default());
    let adapter = wire.clone();
    let rt = runtime.clone();
    let clock_rt = runtime.clone();
    let mut clock = safe_cex_store::virtual_clock::VirtualClock::default();
    clock.now = Arc::new(|| 1_700_000_000_000.0);
    let deps = KucoinDependencies {
        hosted_signer: None,
        runtime: runtime.clone(),
        http: Rc::new(move |options| HttpClientOptions {
            response_transform: None,
            params_serializer: None,
            exchange: ExchangeName::Kucoin,
            options: options.clone(),
            request: RestRequest::default(),
            provider: None,
            signer: None,
            account_name: None,
            adapter: adapter.clone(),
            host: Rc::new(Host),
            runtime: rt.clone(),
            store: None,
            stats: None,
            logs: None,
            every: vec![],
            periodic: vec![],
            next_id: Arc::new(|| "id".into()),
            signing_now: Arc::new(|| 1_700_000_000_000),
        }),
        registry: Rc::new(safe_cex_transport::TransportRegistry::default()),
        clock,
        retry_clock: clock_rt,
    };
    let parts = BaseExchangeParts {
        name,
        options,
        store: store.clone(),
        settings: ExchangeSettings {
            supports_public_only: true,
            positions_supported: Some(true),
            ..Default::default()
        },
        external_public_data: None,
    };
    (
        Fixture {
            runtime: runtime.clone(),
            store,
            wire,
        },
        parts,
        deps,
    )
}

pub fn futures() -> (Rc<KucoinExchange>, Fixture) {
    futures_with(options())
}

pub fn futures_with(options: ExchangeOptions) -> (Rc<KucoinExchange>, Fixture) {
    let runtime = Rc::new(Runtime::default());
    let (fixture, parts, deps) = parts(ExchangeName::Kucoin, &runtime, options);
    (KucoinExchange::new(parts, deps).unwrap(), fixture)
}

pub fn spot() -> (Rc<KucoinSpotExchange>, Fixture) {
    spot_with(options())
}

pub fn spot_with(options: ExchangeOptions) -> (Rc<KucoinSpotExchange>, Fixture) {
    let runtime = Rc::new(Runtime::default());
    let (fixture, parts, deps) = parts(ExchangeName::Kucoinspot, &runtime, options);
    (KucoinSpotExchange::new(parts, deps).unwrap(), fixture)
}

/// Poll `f` to completion, advancing the virtual clock between turns.
pub fn drive<F: Future>(rt: &Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.pump();
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
    }
    panic!("scripted call must settle");
}

#[allow(dead_code)]
pub fn store(f: &Fixture) -> &dyn Store {
    f.store.as_ref()
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
