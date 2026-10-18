//! A scripted host for the production binding: a local executor with a
//! virtual clock, scripted sockets, and a REST adapter answering by route.
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
    DirectTransport, RestAdapter, RestBody, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportFuture, TransportProvider,
    socket::{SocketCapabilities, TunnelDialOptions, TunnelHost, TunnelSocket, TunnelTask},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

type Job = Pin<Box<dyn Future<Output = ()>>>;
struct Flag(AtomicBool);
impl Wake for Flag {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::SeqCst)
    }
}
struct Noop;
impl ExchangeTask for Noop {
    fn cancel(&self) {}
}
struct Sleep(Rc<Cell<bool>>, virtual_time::SleepWaker);
impl Future for Sleep {
    type Output = Result<(), ExchangeError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0.get() {
            Poll::Ready(Ok(()))
        } else {
            *self.1.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// A scripted socket: records sends, delivers queued inbound frames.
#[derive(Default)]
pub struct Wire {
    pub url: RefCell<String>,
    pub sent: RefCell<Vec<String>>,
    pub inbound: RefCell<VecDeque<WsEvent>>,
    pub waker: RefCell<Option<Waker>>,
    pub open: Cell<bool>,
    pub closes: Cell<usize>,
}
struct NextEvent(Rc<Wire>);
impl Future for NextEvent {
    type Output = Result<WsEvent, ExchangeError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.0.inbound.borrow_mut().pop_front() {
            Some(e) => Poll::Ready(Ok(e)),
            None => {
                *self.0.waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
pub struct WireSocket(pub Rc<Wire>);
impl WsSocket for WireSocket {
    fn ready_state(&self) -> u8 {
        if self.0.open.get() { 1 } else { 3 }
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(NextEvent(self.0.clone()))
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.0.sent.borrow_mut().push(message.into());
        Ok(())
    }
    fn close(&self) {
        self.0.closes.set(self.0.closes.get() + 1);
        self.0.open.set(false);
    }
    fn terminate(&self) {
        self.0.open.set(false);
    }
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
impl Wire {
    pub fn push(&self, event: WsEvent) {
        if matches!(event, WsEvent::Open) {
            self.open.set(true);
        }
        self.inbound.borrow_mut().push_back(event);
        if let Some(w) = self.waker.borrow_mut().take() {
            w.wake();
        }
    }
}

struct NoTunnel;
impl TunnelHost for NoTunnel {
    fn spawn_local(&self, _: Job) -> Box<dyn TunnelTask> {
        panic!("tests never tunnel")
    }
    fn capabilities(&self) -> SocketCapabilities {
        panic!("tests never tunnel")
    }
    fn now_ms(&self) -> u64 {
        0
    }
    fn sleep_until(&self, _: u64) -> TransportFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn connect(
        &self,
        _: String,
        _: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        panic!("tests never tunnel")
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        Box::pin(async { Ok(String::new()) })
    }
    fn is_token_expired(&self, _: &str) -> bool {
        false
    }
    fn sync_server_time(&self, _: u64) {}
}

#[derive(Default)]
pub struct Host {
    sleep_wakers: virtual_time::SleepWakers,
    pub now: Cell<f64>,
    jobs: RefCell<Vec<(Job, Arc<Flag>)>>,
    timers: RefCell<Vec<(f64, Rc<Cell<bool>>)>>,
    pub reports: RefCell<Vec<String>>,
    pub console: RefCell<Vec<(String, String)>>,
    pub sockets: RefCell<Vec<Rc<Wire>>>,
}
impl Host {
    /// Run every runnable job to quiescence without advancing the clock.
    pub fn settle(&self) {
        self.sleep_wakers.wake_due(self.now.get());
        for _ in 0..100_000 {
            let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
            if jobs.is_empty() {
                return;
            }
            let mut pending = vec![];
            let mut progressed = false;
            for (mut job, flag) in jobs {
                if !flag.0.swap(false, Ordering::SeqCst) {
                    pending.push((job, flag));
                    continue;
                }
                progressed = true;
                let waker = Waker::from(flag.clone());
                if job
                    .as_mut()
                    .poll(&mut Context::from_waker(&waker))
                    .is_pending()
                {
                    pending.push((job, flag));
                }
            }
            self.jobs.borrow_mut().extend(pending);
            if !progressed {
                return;
            }
        }
        panic!("scripted host did not settle");
    }
    /// Jump the clock to the earliest pending timer and fire it; false if none.
    pub fn fire_next_timer(&self) -> bool {
        let next = self
            .timers
            .borrow()
            .iter()
            .filter(|(_, f)| !f.get())
            .map(|(t, _)| *t)
            .fold(f64::INFINITY, f64::min);
        if !next.is_finite() {
            return false;
        }
        self.now.set(self.now.get().max(next));
        for (t, f) in self.timers.borrow().iter() {
            if *t <= next {
                f.set(true);
            }
        }
        self.timers.borrow_mut().retain(|(_, f)| !f.get());
        for (_, flag) in self.jobs.borrow().iter() {
            flag.0.store(true, Ordering::SeqCst);
        }
        self.settle();
        true
    }
    /// Advance the virtual clock by `ms`, firing due timers, settling between each.
    pub fn advance(&self, ms: f64) {
        let target = self.now.get() + ms;
        loop {
            self.settle();
            let next = self
                .timers
                .borrow()
                .iter()
                .filter(|(t, f)| !f.get() && *t <= target)
                .map(|(t, _)| *t)
                .fold(f64::INFINITY, f64::min);
            if !next.is_finite() {
                break;
            }
            self.now.set(self.now.get().max(next));
            for (t, f) in self.timers.borrow().iter() {
                if *t <= next {
                    f.set(true);
                }
            }
            self.timers.borrow_mut().retain(|(_, f)| !f.get());
            for (_, flag) in self.jobs.borrow().iter() {
                flag.0.store(true, Ordering::SeqCst);
            }
        }
        self.now.set(target);
        self.settle();
    }
    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        let mut future = Box::pin(future);
        let flag = Arc::new(Flag(AtomicBool::new(true)));
        let waker = Waker::from(flag.clone());
        for _ in 0..10_000 {
            if let Poll::Ready(v) = future.as_mut().poll(&mut Context::from_waker(&waker)) {
                return v;
            }
            self.settle();
            if !flag.0.swap(false, Ordering::SeqCst) && !self.fire_next_timer() {
                panic!("future is blocked with no runnable job or pending timer");
            }
        }
        panic!("future did not resolve on the scripted host");
    }
}
impl ExchangeRuntime for Host {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        1_700_000_000_000.0 + self.now.get()
    }
    fn sleep(&self, delay_ms: u64) -> ExchangeFuture<'static, ()> {
        let fired = Rc::new(Cell::new(false));
        self.timers
            .borrow_mut()
            .push((self.now.get() + delay_ms as f64, fired.clone()));
        Box::pin(Sleep(
            fired,
            self.sleep_wakers.register(self.now.get() + delay_ms as f64),
        ))
    }
    fn spawn_local(&self, future: Job) -> Box<dyn ExchangeTask> {
        self.jobs
            .borrow_mut()
            .push((future, Arc::new(Flag(AtomicBool::new(true)))));
        Box::new(Noop)
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.console
            .borrow_mut()
            .push((severity.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        "binding-test".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, error: &ExchangeError, context: &Value) {
        self.reports
            .borrow_mut()
            .push(format!("{} {context}", error.message));
    }
    fn report_raw_exception(&self, error: &safe_cex_exchange::js_runtime::value::Js, _: &Value) {
        self.reports
            .borrow_mut()
            .push(safe_cex_exchange::runtime::exception_text(error));
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("no worker in tests")) })
    }
}
impl WsRuntime for Host {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        Rc::new(NoTunnel)
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let wire = Rc::new(Wire::default());
        *wire.url.borrow_mut() = url;
        self.sockets.borrow_mut().push(wire.clone());
        let socket: Rc<dyn WsSocket> = Rc::new(WireSocket(wire));
        Box::pin(async move { Ok(socket) })
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

/// Answers POSTs by `type` (info) or `action.type` (exchange); records bodies.
pub struct Routes {
    pub routes: RefCell<Vec<(String, Value)>>,
    pub requests: RefCell<Vec<(String, Value)>>,
}
impl Routes {
    // The transport takes Arc<dyn RestAdapter>; this single-threaded fixture is local.
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn new(routes: Vec<(&str, Value)>) -> Arc<Self> {
        Arc::new(Self {
            routes: RefCell::new(routes.into_iter().map(|(k, v)| (k.into(), v)).collect()),
            requests: RefCell::default(),
        })
    }
    pub fn route_of(url: &str, body: &Value) -> String {
        if url.ends_with("/exchange") {
            format!(
                "exchange:{}",
                body["action"]["type"].as_str().unwrap_or("?")
            )
        } else {
            format!("info:{}", body["type"].as_str().unwrap_or("?"))
        }
    }
    pub fn count(&self, route: &str) -> usize {
        self.requests
            .borrow()
            .iter()
            .filter(|(r, _)| r == route)
            .count()
    }
}
impl RestAdapter for Routes {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let body = match &request.body {
            Some(RestBody::Untransformed(v)) => v.clone(),
            Some(RestBody::Transformed(d)) => d.value().clone(),
            Some(RestBody::Encoded(bytes)) => serde_json::from_slice(bytes).unwrap_or(Value::Null),
            None => Value::Null,
        };
        let route = Self::route_of(&request.url, &body);
        self.requests.borrow_mut().push((route.clone(), body));
        let answer = self
            .routes
            .borrow()
            .iter()
            .find(|(k, _)| *k == route)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| json!({"__unrouted": route}));
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(serde_json::to_vec(&answer).unwrap()),
            })
        })
    }
}

#[test]
fn pending_sleep_registers_the_latest_waker_and_cancels_on_drop() {
    let runtime = Host::default();
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
    runtime.advance(0.0);
    assert!(
        !first.take() && !latest.take(),
        "sleep woke before deadline"
    );
    runtime.now.set(runtime.now.get() + 1.0);
    runtime.advance(0.0);
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
    runtime.advance(0.0);
    assert!(!latest.take(), "dropped sleep must not wake");
}
