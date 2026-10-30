//! A host-driven executor for the LOCAL worker's wasm algos: no runtime, no
//! threads. The host advances its clock (`advance`), which fires due sleeps
//! and polls every woken task until all of them wait again, and schedules its
//! own timer for `next_deadline`, exactly as it drives the engine.
use crate::executor::{Executor, LocalFuture, TaskControl, TaskHandle};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

/// Woken task ids; shared with wakers, which must be `Send + Sync`.
type ReadyQueue = Arc<Mutex<VecDeque<usize>>>;

struct TaskWaker {
    id: usize,
    ready: ReadyQueue,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.ready
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push_back(self.id);
    }
}

#[derive(Default)]
struct TaskState {
    finished: Cell<bool>,
    aborted: Cell<bool>,
}

struct Slot {
    future: Option<LocalFuture<()>>,
    state: Rc<TaskState>,
    waker: Waker,
}

struct Abort {
    id: usize,
    state: Rc<TaskState>,
    ready: ReadyQueue,
}

impl TaskControl for Abort {
    fn abort(&self) {
        if !self.state.finished.get() {
            self.state.aborted.set(true);
            self.ready
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push_back(self.id);
        }
    }
    fn is_finished(&self) -> bool {
        self.state.finished.get()
    }
}

#[derive(Default)]
struct Timers {
    next: u64,
    /// (due at, sequence) -> the waiting sleep's waker.
    due: BTreeMap<(Ms, u64), Option<Waker>>,
}

/// An `f64` millisecond total-ordered for the timer map.
#[derive(Clone, Copy)]
struct Ms(f64);
impl PartialEq for Ms {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Ms {}
impl PartialOrd for Ms {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ms {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

struct Inner {
    now: Cell<f64>,
    /// Wall clock minus the monotonic clock.
    wall_offset: Cell<f64>,
    tasks: RefCell<Vec<Option<Slot>>>,
    free: RefCell<Vec<usize>>,
    ready: ReadyQueue,
    timers: RefCell<Timers>,
}

#[derive(Clone)]
pub struct HostExecutor(Rc<Inner>);

impl HostExecutor {
    /// `now_ms` is the host's monotonic clock and `wall_ms` the epoch time now.
    pub fn new(now_ms: f64, wall_ms: f64) -> Self {
        Self(Rc::new(Inner {
            now: Cell::new(now_ms),
            wall_offset: Cell::new(wall_ms - now_ms),
            tasks: RefCell::default(),
            free: RefCell::default(),
            ready: ReadyQueue::default(),
            timers: RefCell::default(),
        }))
    }

    /// Move the clock to `now_ms` (never backwards), fire every sleep due by
    /// then, and run until every task waits again.
    pub fn advance(&self, now_ms: f64, wall_ms: f64) {
        let inner = &self.0;
        if now_ms > inner.now.get() {
            inner.now.set(now_ms);
        }
        inner.wall_offset.set(wall_ms - inner.now.get());
        loop {
            let due = {
                let mut timers = inner.timers.borrow_mut();
                let first = timers.due.keys().next().copied();
                match first {
                    Some(key) if key.0.0 <= inner.now.get() => timers.due.remove(&key),
                    _ => None,
                }
            };
            match due {
                Some(waker) => {
                    if let Some(waker) = waker {
                        waker.wake();
                    }
                }
                None => break,
            }
        }
        self.run_until_stalled();
    }

    /// The earliest pending sleep, for the host's timer.
    pub fn next_deadline(&self) -> Option<f64> {
        self.0.timers.borrow().due.keys().next().map(|key| key.0.0)
    }

    /// Poll woken tasks until none is ready.
    pub fn run_until_stalled(&self) {
        let inner = &self.0;
        loop {
            let next = inner
                .ready
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .pop_front();
            let Some(id) = next else { break };
            // Take the future out so a task may spawn or wake while polled.
            let taken = inner.tasks.borrow_mut().get_mut(id).and_then(|slot| {
                let slot = slot.as_mut()?;
                Some((slot.future.take()?, slot.state.clone(), slot.waker.clone()))
            });
            let Some((mut future, state, waker)) = taken else {
                continue;
            };
            let done = state.aborted.get()
                || future
                    .as_mut()
                    .poll(&mut Context::from_waker(&waker))
                    .is_ready();
            if done {
                state.finished.set(true);
                inner.tasks.borrow_mut()[id] = None;
                inner.free.borrow_mut().push(id);
                drop(future);
            } else if let Some(Some(slot)) = inner.tasks.borrow_mut().get_mut(id) {
                slot.future = Some(future);
            }
        }
    }

    /// Spawned tasks not yet finished, for leak tests.
    pub fn live_tasks(&self) -> usize {
        self.0.tasks.borrow().iter().flatten().count()
    }
}

impl Executor for HostExecutor {
    fn spawn(&self, task: LocalFuture<()>) -> TaskHandle {
        let inner = &self.0;
        let id = inner.free.borrow_mut().pop().unwrap_or_else(|| {
            let mut tasks = inner.tasks.borrow_mut();
            tasks.push(None);
            tasks.len() - 1
        });
        let state = Rc::new(TaskState::default());
        let waker = Waker::from(Arc::new(TaskWaker {
            id,
            ready: inner.ready.clone(),
        }));
        inner.tasks.borrow_mut()[id] = Some(Slot {
            future: Some(task),
            state: state.clone(),
            waker: waker.clone(),
        });
        waker.wake();
        TaskHandle::new(Abort {
            id,
            state,
            ready: inner.ready.clone(),
        })
    }

    fn sleep(&self, ms: f64) -> LocalFuture<()> {
        let inner = self.0.clone();
        let at = inner.now.get() + ms.max(0.0);
        let key = {
            let mut timers = inner.timers.borrow_mut();
            timers.next += 1;
            let key = (Ms(at), timers.next);
            timers.due.insert(key, None);
            key
        };
        Box::pin(Sleep { inner, key })
    }

    fn monotonic_ms(&self) -> f64 {
        self.0.now.get()
    }

    fn now_ms(&self) -> f64 {
        self.0.now.get() + self.0.wall_offset.get()
    }
}

struct Sleep {
    inner: Rc<Inner>,
    key: (Ms, u64),
}

impl std::future::Future for Sleep {
    type Output = ();
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut timers = self.inner.timers.borrow_mut();
        match timers.due.get_mut(&self.key) {
            // Fired (removed by `advance`) or already due.
            None => Poll::Ready(()),
            Some(_) if self.key.0.0 <= self.inner.now.get() => {
                timers.due.remove(&self.key);
                Poll::Ready(())
            }
            Some(waker) => {
                *waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        // An abandoned sleep (a timeout won, a task aborted) holds no timer.
        if let Ok(mut timers) = self.inner.timers.try_borrow_mut() {
            timers.due.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{self, Elapsed};

    /// Run `f` with `executor` installed on this thread.
    fn with<T>(executor: &HostExecutor, f: impl FnOnce() -> T) -> T {
        let previous = executor::set_current(Some(Rc::new(executor.clone())));
        let out = f();
        executor::set_current(previous);
        out
    }

    #[test]
    fn sleeps_fire_in_time_order_as_the_host_advances() {
        let host = HostExecutor::new(0.0, 1_000_000.0);
        let log = Rc::new(RefCell::new(Vec::new()));
        with(&host, || {
            for (name, ms) in [("late", 300), ("early", 100)] {
                let log = log.clone();
                executor::spawn(async move {
                    executor::sleep_ms(ms).await;
                    log.borrow_mut().push((name, executor::now_ms()));
                });
            }
            host.run_until_stalled();
            assert!(log.borrow().is_empty(), "nothing is due yet");
            assert_eq!(host.next_deadline(), Some(100.0));
            host.advance(150.0, 1_000_150.0);
            assert_eq!(*log.borrow(), vec![("early", 1_000_150.0)]);
            host.advance(300.0, 1_000_300.0);
            assert_eq!(log.borrow().len(), 2);
        });
        assert_eq!(host.live_tasks(), 0);
        assert_eq!(host.next_deadline(), None);
    }

    #[test]
    fn an_aborted_task_never_resumes_and_releases_its_timer() {
        let host = HostExecutor::new(0.0, 0.0);
        let resumed = Rc::new(Cell::new(false));
        with(&host, || {
            let flag = resumed.clone();
            let task = executor::spawn(async move {
                executor::sleep_ms(50).await;
                flag.set(true);
            });
            host.run_until_stalled();
            task.abort();
            host.advance(100.0, 100.0);
            assert!(task.is_finished());
        });
        assert!(!resumed.get());
        assert_eq!(host.live_tasks(), 0);
        assert_eq!(
            host.next_deadline(),
            None,
            "the aborted sleep's timer is gone"
        );
    }

    #[test]
    fn timeout_and_instant_follow_the_host_clock() {
        let host = HostExecutor::new(10.0, 0.0);
        let result = Rc::new(RefCell::new(None));
        with(&host, || {
            let out = result.clone();
            executor::spawn(async move {
                let start = executor::Instant::now();
                let pending = executor::timeout(200, std::future::pending::<()>()).await;
                *out.borrow_mut() = Some((pending, start.elapsed().as_millis()));
            });
            // A spawned task first runs when the host runs the executor.
            host.run_until_stalled();
            host.advance(150.0, 0.0);
            assert!(result.borrow().is_none());
            host.advance(210.0, 0.0);
        });
        assert_eq!(*result.borrow(), Some((Err(Elapsed), 200)));
    }

    #[test]
    fn a_task_woken_by_another_runs_in_the_same_advance() {
        let host = HostExecutor::new(0.0, 0.0);
        let notify = Rc::new(tokio::sync::Notify::new());
        let woke = Rc::new(Cell::new(false));
        with(&host, || {
            let (wait, flag) = (notify.clone(), woke.clone());
            executor::spawn(async move {
                wait.notified().await;
                flag.set(true);
            });
            let signal = notify.clone();
            executor::spawn(async move {
                executor::sleep_ms(5).await;
                signal.notify_one();
            });
            host.run_until_stalled();
            assert!(!woke.get());
            host.advance(5.0, 0.0);
        });
        assert!(woke.get());
    }
}
