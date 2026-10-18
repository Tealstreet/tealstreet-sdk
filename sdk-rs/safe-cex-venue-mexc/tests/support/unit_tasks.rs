//! Queue for the existing leaf-oracle unit fixtures. Production binding timing
//! is exercised separately by exchange_trait's actual constructed runtime.
use safe_cex_exchange::runtime::ExchangeTask;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Work = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);
fn with_jobs<T>(run: impl FnOnce(&RefCell<Vec<Work>>) -> T) -> T {
    std::thread_local! { static JOBS: RefCell<Vec<Work>> = const { RefCell::new(Vec::new()) }; }
    JOBS.with(run)
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
pub fn spawn(work: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
    let canceled = Rc::new(Cell::new(false));
    with_jobs(|jobs| jobs.borrow_mut().push((work, canceled.clone())));
    Box::new(Task(canceled))
}
pub fn drive<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..1024 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
            return result;
        }
        let work = with_jobs(|jobs| std::mem::take(&mut *jobs.borrow_mut()));
        for (mut job, canceled) in work {
            if !canceled.get() && job.as_mut().poll(&mut cx).is_pending() {
                with_jobs(|jobs| jobs.borrow_mut().push((job, canceled)));
            }
        }
    }
    panic!("unit future did not settle after 1024 scheduler passes");
}
