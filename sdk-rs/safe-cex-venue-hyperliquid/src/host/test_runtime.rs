//! A single-threaded executor with a virtual clock for realm tests.
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask};
use std::{
    cell::{Cell, RefCell},
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

#[derive(Default)]
pub(crate) struct TestRuntime {
    pub now: Cell<f64>,
    jobs: RefCell<Vec<(Job, Arc<Flag>)>>,
    timers: RefCell<Vec<(f64, Rc<Cell<bool>>, Rc<RefCell<Option<Waker>>>)>>,
    pub reports: RefCell<Vec<String>>,
    pub console: RefCell<Vec<(String, String)>>,
}
struct Noop;
impl ExchangeTask for Noop {
    fn cancel(&self) {}
}
struct Sleep { fired: Rc<Cell<bool>>, waker: Rc<RefCell<Option<Waker>>> }
impl Future for Sleep {
    type Output = Result<(), ExchangeError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.fired.get() {
            Poll::Ready(Ok(()))
        } else {
            *self.waker.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
impl TestRuntime {
    /// Poll every job until none makes progress; then fire due timers and repeat.
    pub(crate) fn drain(&self) {
        for _ in 0..10_000 {
            let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
            if jobs.is_empty() {
                return;
            }
            let mut pending = vec![];
            for (mut job, flag) in jobs {
                flag.0.store(false, Ordering::SeqCst);
                let waker = Waker::from(flag.clone());
                if job
                    .as_mut()
                    .poll(&mut Context::from_waker(&waker))
                    .is_pending()
                {
                    pending.push((job, flag));
                }
            }
            let idle = {
                let mut jobs = self.jobs.borrow_mut();
                jobs.extend(pending);
                jobs.iter().all(|(_, f)| !f.0.load(Ordering::SeqCst))
            };
            // Stop when every remaining job is blocked on something that never woke.
            if idle && !self.fire_timers() {
                return;
            }
        }
        panic!("test runtime did not settle");
    }
    /// Advance the clock to the next timer; true when one fired.
    fn fire_timers(&self) -> bool {
        let mut timers = self.timers.borrow_mut();
        let next = timers
            .iter()
            .filter(|(_, f, _)| !f.get())
            .map(|(t, _, _)| *t)
            .fold(f64::INFINITY, f64::min);
        if !next.is_finite() {
            return false;
        }
        self.now.set(self.now.get().max(next));
        let mut wakers = vec![];
        for (t, f, waker) in timers.iter() {
            if *t <= next {
                f.set(true);
                if let Some(waker) = waker.borrow_mut().take() { wakers.push(waker); }
            }
        }
        timers.retain(|(_, f, _)| !f.get());
        drop(timers);
        for waker in wakers { waker.wake(); }
        for (_, flag) in self.jobs.borrow().iter() {
            flag.0.store(true, Ordering::SeqCst);
        }
        true
    }
}
impl ExchangeRuntime for TestRuntime {
    fn monotonic_now_ms(&self) -> f64 {
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, delay_ms: u64) -> ExchangeFuture<'static, ()> {
        let fired = Rc::new(Cell::new(false));
        let waker = Rc::new(RefCell::new(None));
        self.timers.borrow_mut().push((self.now.get() + delay_ms as f64, fired.clone(), waker.clone()));
        Box::pin(Sleep { fired, waker })
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
        "test".into()
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
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        Box::pin(async { Err(ExchangeError::new("no worker in tests")) })
    }
}

use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, TransportFuture, socket::{TunnelHost, TunnelTask, TunnelSocket, TunnelDialOptions, SocketCapabilities}};
pub(crate) struct NoHttp;
impl RestAdapter for NoHttp {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(async { Err(RestFailure::pre_transmit("unexpected HTTP in scalar host fixture".into(), request)) })
    }
}
struct NoTunnel;
impl TunnelHost for NoTunnel {
    fn spawn_local(&self, _: Job) -> Box<dyn TunnelTask> { panic!("scalar fixture never tunnels") }
    fn capabilities(&self) -> SocketCapabilities { panic!("scalar fixture never tunnels") }
    fn now_ms(&self) -> u64 { 0 }
    fn sleep_until(&self, _: u64) -> TransportFuture<'static, ()> { panic!("scalar fixture never tunnels") }
    fn connect(&self, _: String, _: TunnelDialOptions) -> TransportFuture<'static, Rc<dyn TunnelSocket>> { panic!("scalar fixture never tunnels") }
    fn proxy_token(&self) -> TransportFuture<'_, String> { panic!("scalar fixture never tunnels") }
    fn is_token_expired(&self, _: &str) -> bool { panic!("scalar fixture never tunnels") }
    fn sync_server_time(&self, _: u64) { panic!("scalar fixture never tunnels") }
}

pub(crate) fn host(runtime: Rc<TestRuntime>) -> super::Host {
    super::Host::new(runtime, Arc::new(NoHttp), Rc::new(safe_cex_exchange::native_store::DefaultStore::new()))
}
impl safe_cex_exchange::websocket::WsRuntime for TestRuntime {
    fn transport(&self) -> Arc<dyn safe_cex_transport::TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn safe_cex_transport::socket::TunnelHost> {
        Rc::new(NoTunnel)
    }
    fn dial(&self, _: String, _: Vec<String>, _: safe_cex_exchange::websocket::WebSocketConnectionOptions, _: bool) -> ExchangeFuture<'static, Rc<dyn safe_cex_exchange::websocket::WsSocket>> {
        Box::pin(async { Err(ExchangeError::new("socket unavailable in scalar host fixture")) })
    }
    fn headers_capable(&self) -> bool { false }
    fn enable_reconnecting(&self) -> bool { false }
    fn min_reconnection_delay_ms(&self) -> f64 { 1000. }
    fn library_min_reconnection_delay_ms(&self) -> f64 { 1000. }
    fn diagnostics_enabled(&self) -> bool { false }
    fn lifecycle_logs_enabled(&self) -> bool { false }
}
