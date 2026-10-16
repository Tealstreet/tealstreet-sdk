use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn poll<F: Future + ?Sized>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(&Waker::noop().clone()))
}
type Work = Pin<Box<dyn Future<Output = ()>>>;
struct Job {
    cancelled: Cell<bool>,
    future: RefCell<Option<Work>>,
}
type CancelHook = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;
struct Task(Rc<Job>, CancelHook);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.cancelled.set(true);
        self.0.future.borrow_mut().take();
        let hook = self.1.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
    }
}
#[derive(Default)]
pub struct Runtime {
    pub cancel_hook: CancelHook,
    pub wall: Cell<f64>,
    pub mono: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Rc<Job>>>,
    sleeps: Rc<RefCell<Vec<Rc<Cell<bool>>>>>,
    pub logs: RefCell<Vec<(String, String)>>,
    pub errors: RefCell<Vec<String>>,
}
impl Runtime {
    pub fn pump(&self) {
        // A bounded deterministic microtask floor, not a claim about arbitrary
        // host scheduler progress. Scenarios require at most four chained awaits.
        for _ in 0..32 {
            let jobs = self.jobs.borrow().clone();
            for job in jobs {
                if job.cancelled.get() {
                    continue;
                }
                let Some(mut future) = job.future.borrow_mut().take() else {
                    continue;
                };
                if poll(future.as_mut()).is_pending() && !job.cancelled.get() {
                    *job.future.borrow_mut() = Some(future);
                }
            }
        }
    }
    pub fn timers(&self) -> usize {
        self.sleeps
            .borrow()
            .iter()
            .filter(|active| active.get())
            .count()
    }
}
struct Sleep {
    mono: Rc<Cell<f64>>,
    deadline: f64,
    active: Rc<Cell<bool>>,
}
impl Future for Sleep {
    type Output = Result<(), ExchangeError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.mono.get() >= self.deadline {
            self.active.set(false);
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}
impl Drop for Sleep {
    fn drop(&mut self) {
        self.active.set(false);
    }
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        self.wall.get()
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.mono.get()
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        let active = Rc::new(Cell::new(true));
        self.sleeps.borrow_mut().push(active.clone());
        Box::pin(Sleep {
            mono: self.mono.clone(),
            deadline: self.mono.get() + delay as f64,
            active,
        })
    }
    fn spawn_local(&self, future: Work) -> Box<dyn ExchangeTask> {
        let job = Rc::new(Job {
            cancelled: Cell::new(false),
            future: RefCell::new(Some(future)),
        });
        self.jobs.borrow_mut().push(job.clone());
        Box::new(Task(job, self.cancel_hook.clone()))
    }
    fn console(&self, severity: &str, message: &str) {
        self.logs
            .borrow_mut()
            .push((severity.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        "clock".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.errors.borrow_mut().push(error.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
