//! Explicit native replay clock. Production runtimes never construct this host.
use crate::runtime::{ExchangeError, ExchangeFuture};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    task::Waker,
};

struct Waiter {
    id: u64,
    at: f64,
    ready: Cell<bool>,
    finished: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}
struct Guard(Rc<Waiter>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.finished.set(true);
    }
}
// Keep this lifetime-bearing guard visible to the compiler's type inventory.
struct Advancing<'a>(&'a Cell<bool>);
impl Drop for Advancing<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
pub struct ReplayClock {
    epoch: f64,
    time: Cell<f64>,
    active: Cell<bool>,
    advancing: Cell<bool>,
    next: Cell<u64>,
    waits: RefCell<Vec<Weak<Waiter>>>,
}
pub(crate) fn new(epoch: f64) -> Rc<ReplayClock> {
    Rc::new(ReplayClock {
        epoch,
        time: Cell::new(0.0),
        active: Cell::new(false),
        advancing: Cell::new(false),
        next: Cell::new(0),
        waits: RefCell::default(),
    })
}
pub fn elapsed(clock: &ReplayClock) -> f64 {
    clock.time.get()
}
pub(crate) fn now(clock: &ReplayClock) -> f64 {
    (clock.epoch + elapsed(clock)).floor()
}
pub fn activate(clock: &ReplayClock) -> Result<(), ExchangeError> {
    if clock.active.replace(true) {
        return Err(ExchangeError::new("replay already began"));
    }
    Ok(())
}
fn set_time(clock: &ReplayClock, time: f64) {
    clock.time.set(time);
    safe_cex_store::virtual_clock::set_thread_host_time(Some(now(clock)));
}
pub(crate) fn sleep(clock: &Rc<ReplayClock>, delay: u64) -> ExchangeFuture<'static, ()> {
    let id = clock.next.get();
    clock.next.set(id + 1);
    let wait = Rc::new(Waiter {
        id,
        at: elapsed(clock) + delay as f64,
        ready: Cell::new(false),
        finished: Cell::new(false),
        waker: RefCell::default(),
    });
    clock.waits.borrow_mut().push(Rc::downgrade(&wait));
    let clock = clock.clone();
    let guard = Guard(wait.clone());
    // Startup uses real scheduling but fixed Date/performance inputs. On
    // activation, outstanding sleeps retain their logical deadlines.
    Box::pin(async move {
        let _guard = guard;
        let mut real = std::pin::pin!(tokio::time::sleep(std::time::Duration::from_millis(delay)));
        std::future::poll_fn(|cx| {
            if wait.ready.get() || (!clock.active.get() && real.as_mut().poll(cx).is_ready()) {
                wait.finished.set(true);
                return std::task::Poll::Ready(Ok(()));
            }
            *wait.waker.borrow_mut() = Some(cx.waker().clone());
            std::task::Poll::Pending
        })
        .await
    })
}
/// Advance deadlines in (deadline, registration) order, one callback turn at
/// a time. A cancelled timer's Weak entry cannot keep the clock busy.
pub async fn advance(clock: &ReplayClock, target: f64) -> Result<(), ExchangeError> {
    if !clock.active.get() || !target.is_finite() || target < elapsed(clock) {
        return Err(ExchangeError::new(
            "replay time must be active, finite and monotonic",
        ));
    }
    if clock.advancing.replace(true) {
        return Err(ExchangeError::new(
            "replay clock advance is already running",
        ));
    }
    let _guard = Advancing(&clock.advancing);
    let mut turns = 0;
    loop {
        clock
            .waits
            .borrow_mut()
            .retain(|wait| wait.strong_count() > 0);
        let next = clock
            .waits
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|wait| !wait.finished.get() && wait.at <= target)
            .min_by(|a, b| a.at.total_cmp(&b.at).then(a.id.cmp(&b.id)));
        let Some(wait) = next else { break };
        turns += 1;
        if turns > 100_000 {
            return Err(ExchangeError::new("replay timer loop did not quiesce"));
        }
        set_time(clock, wait.at.max(elapsed(clock)));
        wait.ready.set(true);
        if let Some(waker) = wait.waker.borrow_mut().take() {
            waker.wake();
        }
        // The awakened local task completes its current callback before the
        // advancing task can run again on the single-thread LocalSet.
        while !wait.finished.get() {
            tokio::task::yield_now().await;
        }
        tokio::task::yield_now().await;
    }
    set_time(clock, target);
    Ok(())
}
