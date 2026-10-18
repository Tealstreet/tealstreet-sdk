//! Scripted AsterDEX transport and deterministic scheduler.
mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody, TransportProvider,
    socket::TunnelHost,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

pub struct Runtime {
    sleep_wakers: virtual_time::SleepWakers,
    pub now: Rc<Cell<f64>>,
    pub jobs: RefCell<Vec<Job>>,
    pub console: RefCell<Vec<String>>,
    pub errors: RefCell<Vec<String>>,
    pub sent: Rc<RefCell<Vec<String>>>,
    pub random_values: RefCell<VecDeque<f64>>,
    pub urls: RefCell<Vec<String>>,
    pub sockets: RefCell<Vec<(String, Rc<Socket>)>>,
    pub http_options: RefCell<Vec<safe_cex_types::ExchangeOptions>>,
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
    fn console(&self, _: &str, message: &str) {
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
    fn report_raw_exception(&self, _: &Value, _: &Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, e: &ExchangeError, _: &Value) {
        self.errors.borrow_mut().push(e.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("Worker unused in native fixture")) })
    }
}

pub struct Socket {
    pub events: Rc<RefCell<VecDeque<WsEvent>>>,
    sent: Rc<RefCell<Vec<String>>>,
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
        panic!("fixture refuses tunnel transport")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.urls.borrow_mut().push(url.clone());
        let socket = Rc::new(Socket {
            events: Rc::new(RefCell::new(VecDeque::from([WsEvent::Open]))),
            sent: self.sent.clone(),
            closed: Cell::new(false),
        });
        self.sockets.borrow_mut().push((url, socket.clone()));
        Box::pin(async move { Ok(socket as Rc<dyn WsSocket>) })
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
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct Wire {
    pub sequence: RefCell<BTreeMap<String, VecDeque<(u16, Value)>>>,
    pub requests: RefCell<Vec<RestRequest>>,
    pub responses: RefCell<BTreeMap<String, Value>>,
    pub statuses: RefCell<BTreeMap<String, u16>>,
}
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let path = request.url.split('?').next().unwrap_or_default();
        let path = path
            .split_once("://")
            .map_or(path, |(_, rest)| rest.find('/').map_or("/", |i| &rest[i..]));
        let data = self
            .responses
            .borrow()
            .get(path)
            .cloned()
            .unwrap_or(json!([]));
        let status = self.statuses.borrow().get(path).copied().unwrap_or(200);
        let (status, data) = self
            .sequence
            .borrow_mut()
            .get_mut(path)
            .and_then(VecDeque::pop_front)
            .unwrap_or((status, data));
        self.requests.borrow_mut().push(request);
        Box::pin(async move {
            Ok(RestResponse {
                status,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(data).unwrap(),
                ),
            })
        })
    }
}
pub fn runtime_fixture() -> Rc<Runtime> {
    Rc::new(Runtime {
        sleep_wakers: virtual_time::SleepWakers::default(),
        now: Rc::new(Cell::new(1700000000000.0)),
        jobs: RefCell::default(),
        console: RefCell::default(),
        errors: RefCell::default(),
        sent: Rc::default(),
        random_values: RefCell::default(),
        urls: RefCell::default(),
        sockets: RefCell::default(),
        http_options: RefCell::default(),
    })
}
pub fn fixture(
    options: Value,
) -> (
    Rc<safe_cex_venue_asterdex::native_exchange::AsterDEXExchange>,
    Rc<Runtime>,
    Rc<safe_cex_store::store::DefaultStore>,
    Arc<Wire>,
) {
    use safe_cex_venue_asterdex::native_exchange::{AsterDEXDependencies, AsterDEXExchange};
    let runtime = runtime_fixture();
    #[expect(clippy::arc_with_non_send_sync)]
    let wire = Arc::new(Wire::default());
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let rt = runtime.clone();
    let transport = wire.clone();
    let venue = AsterDEXExchange::new(
        safe_cex_types::ExchangeName::Asterdex,
        serde_json::from_value(options).unwrap(),
        store.clone(),
        AsterDEXDependencies {
            runtime: runtime.clone(),
            http: Rc::new(move |options| {
                rt.http_options.borrow_mut().push(options.clone());
                Ok(safe_cex_exchange::js_runtime::client::rest_http_host(
                    rt.clone(),
                    transport.clone(),
                ))
            }),
            external_public_data: None,
        },
    )
    .unwrap();
    (venue, runtime, store, wire)
}
pub fn drive<F: Future>(rt: &Runtime, f: F) -> F::Output {
    let mut future = std::pin::pin!(f);
    for _ in 0..128 {
        rt.pump();
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        rt.now.set(rt.now.get() + 100.0);
    }
    panic!("scripted AsterDEX call must settle");
}
