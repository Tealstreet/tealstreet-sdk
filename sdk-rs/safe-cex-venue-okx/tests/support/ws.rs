use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask, LogSeverity},
    websocket::{
        BaseWebSocket, ReopenResync, WebSocketConnectionOptions, WsEvent, WsParent, WsProtocol,
        WsRuntime, WsSocket,
    },
};
use safe_cex_transport::{DirectTransport, TransportProvider, socket::TunnelHost};
use safe_cex_types::ExchangeOptions;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
type FutureSlot = Rc<RefCell<Option<Pin<Box<dyn Future<Output = ()>>>>>>;
struct Task {
    slot: FutureSlot,
    cancelled: Rc<Cell<bool>>,
}
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.cancelled.set(true);
        let future = self.slot.borrow_mut().take();
        drop(future);
    }
}
type Reentry = Box<dyn Fn() -> Result<(), ExchangeError>>;
#[derive(Default)]
pub struct Io {
    pub sent: RefCell<Vec<String>>,
    pub effects: RefCell<Vec<String>>,
    pub ready: Cell<u8>,
    pub fail_send_at: Cell<Option<usize>>,
    pub reenter: RefCell<Option<Reentry>>,
}
impl WsSocket for Io {
    fn ready_state(&self) -> u8 {
        self.ready.get()
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(std::future::pending())
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.effects.borrow_mut().push(format!("send:{message}"));
        let index = self.sent.borrow().len();
        self.sent.borrow_mut().push(message.into());
        if self.fail_send_at.get() == Some(index) {
            return Err(ExchangeError::new("send failure"));
        }
        let callback = self.reenter.borrow_mut().take();
        if let Some(callback) = callback {
            callback()?;
        }
        Ok(())
    }
    fn close(&self) {
        self.ready.set(3)
    }
    fn terminate(&self) {
        self.ready.set(3)
    }
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
#[derive(Default)]
pub struct Runtime {
    pub io: Rc<Io>,
    jobs: RefCell<Vec<(FutureSlot, Rc<Cell<bool>>)>>,
    pub sleeps: RefCell<Vec<u64>>,
    pub dials: RefCell<Vec<String>>,
    pub pending_dial: Cell<bool>,
    pub inline: Cell<bool>,
    pub monotonic_time: Cell<Option<f64>>,
}
impl Runtime {
    pub fn discard_tasks(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        for (slot, _) in jobs {
            let future = slot.borrow_mut().take();
            drop(future);
        }
    }
    pub fn pump(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        let mut pending = vec![];
        for (slot, cancelled) in jobs {
            let future = slot.borrow_mut().take();
            if let Some(mut future) = future
                && !cancelled.get()
                && future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                && !cancelled.get()
            {
                *slot.borrow_mut() = Some(future);
                pending.push((slot, cancelled));
            }
        }
        self.jobs.borrow_mut().extend(pending);
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.io.effects.borrow_mut().push("clock".into());
        self.monotonic_time.get().unwrap_or(123.)
    }
    fn now_ms(&self) -> f64 {
        123.
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        self.sleeps.borrow_mut().push(delay);
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, mut future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let cancelled = Rc::new(Cell::new(false));
        if self.inline.get()
            && future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        {
            return Box::new(Task {
                slot: Rc::new(RefCell::new(None)),
                cancelled,
            });
        }
        let slot = Rc::new(RefCell::new(Some(future)));
        self.jobs
            .borrow_mut()
            .push((slot.clone(), cancelled.clone()));
        Box::new(Task { slot, cancelled })
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.
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
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("unexpected worker")) })
    }
}
impl WsRuntime for Runtime {
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.
    }
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("test must stay on direct public socket")
    }
    fn dial(
        &self,
        url: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.dials.borrow_mut().push(url);
        let socket = self.io.clone();
        let pending = self.pending_dial.get();
        Box::pin(async move {
            if pending {
                std::future::pending::<()>().await;
            }
            Ok(socket as Rc<dyn WsSocket>)
        })
    }
    fn headers_capable(&self) -> bool {
        true
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
pub struct Parent {
    pub disposed: Cell<bool>,
    pub latencies: RefCell<Vec<f64>>,
    pub health_error: RefCell<Option<ExchangeError>>,
    pub latency_error: RefCell<Option<ExchangeError>>,
    pub log_error: RefCell<Option<ExchangeError>>,
    pub log_io: RefCell<Option<Rc<Io>>>,
    options: ExchangeOptions,
    coordinator: Rc<ReopenResync>,
}
impl Parent {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            disposed: Cell::new(false),
            latencies: RefCell::default(),
            health_error: RefCell::new(None),
            latency_error: RefCell::new(None),
            log_error: RefCell::new(None),
            log_io: RefCell::new(None),
            options: serde_json::from_value(serde_json::json!({"key":"","secret":""})).unwrap(),
            coordinator: Rc::default(),
        })
    }
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "okx"
    }
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn options(&self) -> ExchangeOptions {
        self.options.clone()
    }
    fn store(&self) -> Rc<dyn safe_cex_store::Store> {
        panic!("dispatch must not access market store")
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        false
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.coordinator.clone()
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Err(ExchangeError::new("unexpected snapshot")) })
    }
    fn log(&self, _: &str, _: LogSeverity) -> Result<(), ExchangeError> {
        if let Some(io) = self.log_io.borrow().clone() {
            io.effects.borrow_mut().push("log-listener".into());
        }
        self.log_error.borrow().clone().map_or(Ok(()), Err)
    }
    fn emit(&self, _: &str, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.health_error.borrow().clone().map_or(Ok(()), Err)
    }
    fn update_latency(&self, value: f64) -> Result<(), ExchangeError> {
        self.latencies.borrow_mut().push(value);
        self.latency_error.borrow().clone().map_or(Ok(()), Err)
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_and_log_error(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
}
pub struct Protocol;
impl WsProtocol for Protocol {
    fn ping(&self, _: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        Ok(())
    }
}
pub fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending fixture"),
    }
}
