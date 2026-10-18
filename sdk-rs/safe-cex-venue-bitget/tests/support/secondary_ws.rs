// In-process I/O fixture adapted from the existing OKX socket fixture.
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsEvent, WsRuntime, WsSocket},
};
use safe_cex_transport::{DirectTransport, TransportProvider, socket::TunnelHost};
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
    pub events: Rc<RefCell<std::collections::VecDeque<WsEvent>>>,
    pub fail_send_at: Cell<Option<usize>>,
    pub reenter: RefCell<Option<Reentry>>,
}
impl WsSocket for Io {
    fn ready_state(&self) -> u8 {
        self.ready.get()
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
    ids: Cell<u64>,
    pub exceptions: RefCell<Vec<ExchangeError>>,
    jobs: RefCell<Vec<(FutureSlot, Rc<Cell<bool>>)>>,
    pub sleeps: RefCell<Vec<u64>>,
    pub dials: RefCell<Vec<String>>,
    pub pending_dial: Rc<Cell<bool>>,
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
        let id = self.ids.get() + 1;
        self.ids.set(id);
        format!("listener-{id}")
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
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.exceptions.borrow_mut().push(error.clone());
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
        let pending = self.pending_dial.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if pending.get() {
                Poll::Pending
            } else {
                Poll::Ready(Ok(socket.clone() as Rc<dyn WsSocket>))
            }
        }))
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
