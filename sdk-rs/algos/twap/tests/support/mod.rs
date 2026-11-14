use std::rc::Rc;
use std::time::Duration;

pub const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

struct Clock {
    start: tokio::time::Instant,
}

impl safe_cex_algos::executor::Executor for Clock {
    fn spawn(
        &self,
        task: safe_cex_algos::executor::LocalFuture<()>,
    ) -> safe_cex_algos::executor::TaskHandle {
        safe_cex_algos::executor::TaskHandle::new(tokio::task::spawn_local(task).abort_handle())
    }
    fn sleep(&self, ms: f64) -> safe_cex_algos::executor::LocalFuture<()> {
        Box::pin(tokio::time::sleep(Duration::from_secs_f64(
            ms.max(0.0) / 1000.0,
        )))
    }
    fn monotonic_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
    fn now_ms(&self) -> f64 {
        1_700_000_000_000.0 + self.start.elapsed().as_secs_f64() * 1000.0
    }
}

pub struct ClockGuard(Option<Rc<dyn safe_cex_algos::executor::Executor>>);

impl ClockGuard {
    pub fn new() -> Self {
        Self(safe_cex_algos::executor::set_current(Some(Rc::new(
            Clock {
                start: tokio::time::Instant::now(),
            },
        ))))
    }
}

impl Drop for ClockGuard {
    fn drop(&mut self) {
        safe_cex_algos::executor::set_current(self.0.take());
    }
}
