use safe_cex_exchange::runtime::*;
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub struct Task {
    pub cancelled: Rc<Cell<bool>>,
    future: Pin<Box<dyn Future<Output = ()>>>,
}
struct Handle(Rc<Cell<bool>>);
impl ExchangeTask for Handle {
    fn cancel(&self) {
        self.0.set(true);
    }
}
#[derive(Default)]
pub struct Runtime {
    pub id: RefCell<Option<String>>,
    pub now: Rc<Cell<u64>>,
    pub tasks: RefCell<Vec<Task>>,
    pub discard_spawn: Cell<bool>,
    pub exceptions: RefCell<Vec<(ExchangeError, serde_json::Value)>>,
}
impl Runtime {
    pub fn advance(&self, now: u64) {
        self.now.set(now);
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        let mut pending = Vec::new();
        for mut task in tasks {
            if !task.cancelled.get()
                && task
                    .future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            {
                pending.push(task);
            }
        }
        self.tasks.borrow_mut().extend(pending);
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        // The deterministic timer clock is also the origin used by sleep.
        self.now.get() as f64
    }
    fn now_ms(&self) -> f64 {
        self.now.get() as f64
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        let now = self.now.clone();
        let deadline = now.get() + delay;
        Box::pin(poll_fn(move |_| {
            if now.get() >= deadline {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let cancelled = Rc::new(Cell::new(false));
        if self.discard_spawn.get() {
            drop(future);
        } else {
            self.tasks.borrow_mut().push(Task {
                cancelled: cancelled.clone(),
                future,
            });
        }
        Box::new(Handle(cancelled))
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        self.id.borrow().clone().unwrap_or_else(|| "id".into())
    }
    fn is_worker(&self) -> bool {
        true
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, context: &serde_json::Value) {
        self.exceptions
            .borrow_mut()
            .push((error.clone(), context.clone()));
    }
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        Box::pin(async { unreachable!() })
    }
}
