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
    timers: RefCell<Vec<(f64, Rc<Cell<bool>>)>>,
    pub reports: RefCell<Vec<String>>,
    pub console: RefCell<Vec<(String, String)>>,
}
struct Noop;
impl ExchangeTask for Noop {
    fn cancel(&self) {}
}
struct Sleep(Rc<Cell<bool>>);
impl Future for Sleep {
    type Output = Result<(), ExchangeError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0.get() {
            Poll::Ready(Ok(()))
        } else {
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
            .filter(|(_, f)| !f.get())
            .map(|(t, _)| *t)
            .fold(f64::INFINITY, f64::min);
        if !next.is_finite() {
            return false;
        }
        self.now.set(self.now.get().max(next));
        for (t, f) in timers.iter() {
            if *t <= next {
                f.set(true);
            }
        }
        timers.retain(|(_, f)| !f.get());
        drop(timers);
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
        self.timers
            .borrow_mut()
            .push((self.now.get() + delay_ms as f64, fired.clone()));
        Box::pin(Sleep(fired))
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
