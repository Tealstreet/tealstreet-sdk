mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
use safe_cex_exchange::runtime::*;
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
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
    sleep_wakers: virtual_time::SleepWakers,
    pub id: RefCell<Option<String>>,
    pub now: Rc<Cell<u64>>,
    pub tasks: RefCell<Vec<Task>>,
    pub discard_spawn: Cell<bool>,
    pub exceptions: RefCell<Vec<(ExchangeError, serde_json::Value)>>,
}
impl Runtime {
    pub fn advance(&self, now: u64) {
        self.now.set(now);
        self.sleep_wakers.wake_due(now as f64);
        for _ in 0..64 {
            let signal = std::sync::Arc::new(virtual_time::WakeSignal::default());
            let waker = signal.waker();
            let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
            let mut pending = Vec::new();
            for mut task in tasks {
                if !task.cancelled.get()
                    && task
                        .future
                        .as_mut()
                        .poll(&mut Context::from_waker(&waker))
                        .is_pending()
                {
                    pending.push(task);
                }
            }
            let added = !self.tasks.borrow().is_empty();
            self.tasks.borrow_mut().extend(pending);
            if !added && !signal.take() {
                return;
            }
        }
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
        let sleeper = self.sleep_wakers.register(deadline as f64);
        Box::pin(poll_fn(move |cx| {
            if now.get() >= deadline {
                Poll::Ready(Ok(()))
            } else {
                *sleeper.borrow_mut() = Some(cx.waker().clone());
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
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        self.id.borrow().clone().unwrap_or_else(|| "id".into())
    }
    fn is_worker(&self) -> bool {
        true
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
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
    runtime.now.set(runtime.now.get() + 9);
    runtime.advance(runtime.now.get());
    assert!(
        !first.take() && !latest.take(),
        "sleep woke before deadline"
    );
    runtime.now.set(runtime.now.get() + 1);
    runtime.advance(runtime.now.get());
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
    runtime.now.set(runtime.now.get() + 10);
    runtime.advance(runtime.now.get());
    assert!(!latest.take(), "dropped sleep must not wake");
}
