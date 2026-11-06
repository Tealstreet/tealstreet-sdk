//! Where algo tasks run and what time they see. The native server uses tokio
//! (its account thread's `LocalSet`); the LOCAL worker uses a host-driven
//! executor whose clock and timers the host advances, as it does the engine's.
//! Algos reach it through these free functions, so they never name tokio.
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Duration;

pub type LocalFuture<T> = Pin<Box<dyn Future<Output = T>>>;

/// Spawns `!Send` tasks, sleeps, and keeps a monotonic clock and the wall
/// clock, both in milliseconds.
pub trait Executor {
    fn spawn(&self, task: LocalFuture<()>) -> TaskHandle;
    fn sleep(&self, ms: f64) -> LocalFuture<()>;
    fn monotonic_ms(&self) -> f64;
    /// Epoch milliseconds.
    fn now_ms(&self) -> f64;
}

/// Aborts a spawned task; finished tasks are dropped from their scope.
pub trait TaskControl {
    fn abort(&self);
    fn is_finished(&self) -> bool;
}

pub struct TaskHandle(Box<dyn TaskControl>);

impl TaskHandle {
    pub fn new(control: impl TaskControl + 'static) -> Self {
        Self(Box::new(control))
    }
    pub fn abort(&self) {
        self.0.abort();
    }
    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<dyn Executor>>> = const { RefCell::new(None) };
}

/// Install this thread's executor; returns the one it replaces. Unset, the
/// native build uses tokio.
pub fn set_current(executor: Option<Rc<dyn Executor>>) -> Option<Rc<dyn Executor>> {
    CURRENT.with(|current| std::mem::replace(&mut *current.borrow_mut(), executor))
}

fn current() -> Rc<dyn Executor> {
    if let Some(executor) = CURRENT.with(|current| current.borrow().clone()) {
        return executor;
    }
    #[cfg(feature = "tokio-executor")]
    {
        tokio_executor::shared()
    }
    #[cfg(not(feature = "tokio-executor"))]
    panic!("no algo executor installed on this thread")
}

pub fn spawn(task: impl Future<Output = ()> + 'static) -> TaskHandle {
    current().spawn(Box::pin(task))
}

/// Wall-clock epoch milliseconds.
pub fn now_ms() -> f64 {
    current().now_ms()
}

pub async fn sleep_ms(ms: u64) {
    current().sleep(ms as f64).await;
}

/// `future`, or `Err(Elapsed)` once `ms` pass first. The future is polled first.
pub async fn timeout<F: Future>(ms: u64, future: F) -> Result<F::Output, Elapsed> {
    let mut future = std::pin::pin!(future);
    let mut sleep = current().sleep(ms as f64);
    std::future::poll_fn(|cx: &mut Context<'_>| {
        if let Poll::Ready(output) = future.as_mut().poll(cx) {
            return Poll::Ready(Ok(output));
        }
        sleep.as_mut().poll(cx).map(|()| Err(Elapsed))
    })
    .await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elapsed;

/// A point on the executor's monotonic clock.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Instant(f64);

impl Instant {
    pub fn now() -> Self {
        Self(current().monotonic_ms())
    }
    pub fn elapsed(&self) -> Duration {
        Duration::from_secs_f64((Self::now().0 - self.0).max(0.0) / 1000.0)
    }
}

#[cfg(feature = "tokio-executor")]
mod tokio_executor {
    use super::{Executor, LocalFuture, TaskControl, TaskHandle};
    use std::rc::Rc;
    use std::time::Duration;

    struct Tokio {
        /// tokio's clock (paused in tests) measured from this executor's start.
        base: tokio::time::Instant,
    }

    impl TaskControl for tokio::task::AbortHandle {
        fn abort(&self) {
            tokio::task::AbortHandle::abort(self);
        }
        fn is_finished(&self) -> bool {
            tokio::task::AbortHandle::is_finished(self)
        }
    }

    impl Executor for Tokio {
        fn spawn(&self, task: LocalFuture<()>) -> TaskHandle {
            TaskHandle::new(tokio::task::spawn_local(task).abort_handle())
        }
        fn sleep(&self, ms: f64) -> LocalFuture<()> {
            Box::pin(tokio::time::sleep(Duration::from_secs_f64(
                ms.max(0.0) / 1000.0,
            )))
        }
        fn monotonic_ms(&self) -> f64 {
            self.base.elapsed().as_secs_f64() * 1000.0
        }
        fn now_ms(&self) -> f64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1000.0)
        }
    }

    thread_local! {
        static SHARED: Rc<dyn Executor> = Rc::new(Tokio {
            base: tokio::time::Instant::now(),
        });
    }

    pub(super) fn shared() -> Rc<dyn Executor> {
        SHARED.with(Rc::clone)
    }
}

#[cfg(all(test, feature = "tokio-executor"))]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test(start_paused = true)]
    async fn timeout_returns_the_future_or_elapses_on_the_executor_clock() {
        assert_eq!(timeout(10, async { 7 }).await, Ok(7));
        let start = Instant::now();
        assert_eq!(
            timeout(250, std::future::pending::<()>()).await,
            Err(Elapsed)
        );
        assert_eq!(start.elapsed(), Duration::from_millis(250));
    }

    struct Fixed {
        spawned: Cell<usize>,
    }
    struct Done;
    impl TaskControl for Done {
        fn abort(&self) {}
        fn is_finished(&self) -> bool {
            true
        }
    }
    impl Executor for Fixed {
        fn spawn(&self, _: LocalFuture<()>) -> TaskHandle {
            self.spawned.set(self.spawned.get() + 1);
            TaskHandle::new(Done)
        }
        fn sleep(&self, _: f64) -> LocalFuture<()> {
            Box::pin(std::future::ready(()))
        }
        fn monotonic_ms(&self) -> f64 {
            42.0
        }
        fn now_ms(&self) -> f64 {
            1_700_000_000_000.0
        }
    }

    #[test]
    fn an_installed_executor_replaces_tokio_for_this_thread() {
        let fixed = Rc::new(Fixed {
            spawned: Cell::new(0),
        });
        let previous = set_current(Some(fixed.clone()));
        assert!(spawn(async {}).is_finished());
        assert_eq!(fixed.spawned.get(), 1);
        assert_eq!(now_ms(), 1_700_000_000_000.0);
        assert_eq!(Instant::now(), Instant(42.0));
        set_current(previous);
    }
}
