//! Virtual-clock runtime, recording sockets and a URL-scripted REST wire for
//! driving a `MexcExchange::new` venue through the `Exchange` trait.
mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    external::ExternalPublicData,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestBody, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportProvider, socket::TunnelHost,
};
use safe_cex_types::{ExchangeName, ExchangeOptions, ExchangeOptionsExternalPublicData};
use safe_cex_venue_mexc::{exchange::MexcExchange, sockets::MexcSocketDependencies};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll},
};

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

pub struct Runtime {
    sleep_wakers: virtual_time::SleepWakers,
    pub diagnostics: Cell<bool>,
    pub now: Rc<Cell<f64>>,
    pub jobs: RefCell<Vec<Job>>,
    pub console: RefCell<Vec<String>>,
    pub errors: RefCell<Vec<String>>,
    pub sent: Rc<RefCell<Vec<String>>>,
    pub random_values: RefCell<VecDeque<f64>>,
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
    fn console_values(&self, _: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.console.borrow_mut().push(message.into());
    }
    fn unique_id(&self) -> String {
        "fixture-id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        self.random_values.borrow_mut().pop_front().unwrap_or(0.375)
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        e: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.errors.borrow_mut().push(e.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("Worker unused in native fixture")) })
    }
}

struct Socket {
    events: RefCell<VecDeque<WsEvent>>,
    sent: Rc<RefCell<Vec<String>>>,
    closed: Cell<bool>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        if self.closed.get() { 3 } else { 1 }
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        match self.events.borrow_mut().pop_front() {
            Some(event) => Box::pin(async move { Ok(event) }),
            None => Box::pin(std::future::pending()),
        }
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
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let socket: Rc<dyn WsSocket> = Rc::new(Socket {
            events: RefCell::new(VecDeque::from([WsEvent::Open])),
            sent: self.sent.clone(),
            closed: Cell::new(false),
        });
        Box::pin(async move { Ok(socket) })
    }
    fn headers_capable(&self) -> bool {
        true
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
        self.diagnostics.get()
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

/// Requests in send order as (METHOD, path, JSON body); responses by path.
#[derive(Default)]
pub struct Wire {
    pub hold_responses: Rc<Cell<bool>>,
    response_wakers: Rc<RefCell<Vec<std::task::Waker>>>,
    pub requests: RefCell<Vec<(String, String, Value)>>,
    pub bodies: RefCell<BTreeMap<String, safe_cex_types::retained_json::RetainedJsonInput>>,
}
impl Wire {
    pub fn release_responses(&self) {
        self.hold_responses.set(false);
        for waker in self.response_wakers.borrow_mut().drain(..) {
            waker.wake();
        }
    }
}
impl RestAdapter for Wire {
    fn execute(&self, r: RestRequest) -> RestFuture<'_, RestResponse> {
        // Record the body as the wire text the transport's JSON.stringify sends.
        let body = match &r.body {
            Some(RestBody::Untransformed(value)) => {
                let text = safe_cex_num::js_retained_json_stringify(value)
                    .unwrap()
                    .unwrap();
                serde_json::from_str(&text).unwrap()
            }
            _ => Value::Null,
        };
        self.requests
            .borrow_mut()
            .push((r.method.to_uppercase(), r.url.clone(), body));
        let response = self
            .bodies
            .borrow()
            .get(&r.url)
            .cloned()
            .unwrap_or_else(|| json!({"success":true,"code":0,"data":[]}).into());
        let held = self.hold_responses.clone();
        let wakers = self.response_wakers.clone();
        Box::pin(async move {
            std::future::poll_fn(move |cx| {
                if held.get() {
                    let mut pending = wakers.borrow_mut();
                    if !pending.iter().any(|w| w.will_wake(cx.waker())) {
                        pending.push(cx.waker().clone());
                    }
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Untransformed(response),
            })
        })
    }
}

pub type External =
    Rc<dyn Fn(&ExchangeOptionsExternalPublicData, Rc<BaseExchange>) -> Rc<dyn ExternalPublicData>>;

pub fn fixture() -> (Rc<MexcExchange>, Rc<Runtime>, Rc<DefaultStore>, Rc<Wire>) {
    fixture_with(
        json!({"key":"fixture-key","secret":"fixture-secret"}),
        Rc::new(|_, _| panic!("no externalPublicData configured")),
    )
}

pub fn fixture_with(
    options: Value,
    external: External,
) -> (Rc<MexcExchange>, Rc<Runtime>, Rc<DefaultStore>, Rc<Wire>) {
    // exchangeMetadata.ts MexcMetadata: supportsPublicOnly.
    fixture_with_settings(options, external, true)
}

pub fn fixture_with_settings(
    options: Value,
    external: External,
    supports_public_only: bool,
) -> (Rc<MexcExchange>, Rc<Runtime>, Rc<DefaultStore>, Rc<Wire>) {
    fixture_with_settings_and_local_time(options, external, supports_public_only, Rc::new(|v| v))
}

pub fn fixture_with_settings_and_local_time(
    options: Value,
    external: External,
    supports_public_only: bool,
    local_to_utc: Rc<dyn Fn(f64) -> f64>,
) -> (Rc<MexcExchange>, Rc<Runtime>, Rc<DefaultStore>, Rc<Wire>) {
    let runtime = Rc::new(Runtime {
        sleep_wakers: virtual_time::SleepWakers::default(),
        diagnostics: Cell::new(false),
        now: Rc::new(Cell::new(1_700_000_000_000.0)),
        jobs: RefCell::default(),
        console: RefCell::default(),
        errors: RefCell::default(),
        sent: Rc::default(),
        random_values: RefCell::default(),
    });
    let store = Rc::new(DefaultStore::new());
    let options: ExchangeOptions = serde_json::from_value(options).unwrap();
    let base = BaseExchange::new(
        ExchangeName::Mexc,
        options,
        store.clone(),
        runtime.clone(),
        // exchangeMetadata.ts MexcMetadata: tickInterval 2_000.
        ExchangeSettings {
            tick_interval: Some(2_000.0),
            supports_public_only,
            ..Default::default()
        },
    );
    let wire = Rc::new(Wire::default());
    let venue = MexcExchange::new(
        base,
        wire.clone(),
        local_to_utc,
        MexcSocketDependencies {
            current_time_ms: {
                let runtime = runtime.clone();
                Rc::new(move || runtime.now_ms())
            },
            runtime: runtime.clone(),
            remote_account_name: None,
            emit_fill: Rc::new(|_| Ok(())),
        },
        external,
    );
    (venue, runtime, store, wire)
}
