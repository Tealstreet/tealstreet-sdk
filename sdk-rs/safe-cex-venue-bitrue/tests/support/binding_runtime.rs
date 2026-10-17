//! Deterministic runtime and sockets for the production Bitrue binding.
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);

#[derive(Default)]
pub struct Runtime {
    pub now: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Job>>,
    pub sockets: RefCell<Vec<(String, Rc<Socket>)>>,
    pub reports: RefCell<Vec<String>>,
    pub exceptions: RefCell<Vec<(ExchangeError, Value)>>,
    pub console: RefCell<Vec<String>>,
    pub provider: RefCell<Option<Arc<dyn TransportProvider>>>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
impl Runtime {
    pub fn pump(&self) {
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
        Box::pin(std::future::poll_fn(move |_| {
            if now.get() >= at {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let flag = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((future, flag.clone()));
        Box::new(Task(flag))
    }
    fn console(&self, _: &str, message: &str) {
        self.console.borrow_mut().push(message.into());
    }
    fn unique_id(&self) -> String {
        "00000000-0000-4000-8000-000000000000".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, error: &ExchangeError, context: &Value) {
        self.reports.borrow_mut().push(error.message.clone());
        self.exceptions
            .borrow_mut()
            .push((error.clone(), context.clone()));
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
        self.provider
            .borrow()
            .clone()
            .unwrap_or_else(|| Arc::new(safe_cex_transport::DirectTransport))
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("Bitrue binding E2E never tunnels")
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
