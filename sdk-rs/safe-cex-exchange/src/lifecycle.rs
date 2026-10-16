use crate::runtime::{Cleanup, ExchangeError};
use std::{cell::RefCell, task::Waker};
pub(crate) struct SignerWaiter {
    pub(crate) identity: std::rc::Rc<()>,
    pub(crate) on_finish: Box<dyn Fn()>,
    pub(crate) result: RefCell<Option<bool>>,
    pub(crate) waker: RefCell<Option<Waker>>,
    pub(crate) settled: std::cell::Cell<bool>,
    pub(crate) on_available: Box<dyn Fn() -> Result<(), ExchangeError>>,
    pub(crate) on_error: Box<dyn Fn(&ExchangeError)>,
    pub(crate) unsubscribe: RefCell<Option<Cleanup>>,
}
impl SignerWaiter {
    pub(crate) fn finish(&self, available: bool) {
        if self.settled.replace(true) {
            return;
        }
        (self.on_finish)();
        let cleanup = self.unsubscribe.borrow_mut().take();
        if let Some(cleanup) = cleanup {
            cleanup();
        }
        if available && let Err(error) = (self.on_available)() {
            // Source settles its latch before emitting, but resolves only after
            // emission. A thrown availability listener leaves that promise pending.
            (self.on_error)(&error);
            return;
        }
        *self.result.borrow_mut() = Some(available);
        let waker = self.waker.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

#[derive(Clone)]
pub(crate) enum WatchdogOwner {
    Venue(std::rc::Weak<dyn crate::exchange::Exchange>),
    Core(std::rc::Weak<crate::exchange::BaseExchange>),
}
impl WatchdogOwner {
    pub(crate) fn with_base<T>(
        &self,
        run: impl FnOnce(&crate::exchange::BaseExchange) -> T,
    ) -> Option<T> {
        match self {
            Self::Venue(venue) => venue.upgrade().map(|venue| run(venue.base())),
            Self::Core(base) => base.upgrade().map(|base| run(&base)),
        }
    }
}

// Task-slot cleanup must run even when the host discards an unpolled future.
pub(crate) struct WatchdogGuard {
    pub(crate) owner: WatchdogOwner,
    pub(crate) generation: u64,
    pub(crate) completed: std::rc::Rc<std::cell::Cell<bool>>,
}
impl WatchdogGuard {
    pub(crate) fn release(&mut self) {
        if self.completed.replace(true) {
            return;
        }
        let _ = self.owner.with_base(|base| {
            if base.watchdog_generation.get() == self.generation {
                let task = base.watchdog.borrow_mut().take();
                drop(task);
            }
        });
    }
}
impl Drop for WatchdogGuard {
    fn drop(&mut self) {
        self.release();
    }
}

pub(crate) struct SignerWaitGuard(pub(crate) std::rc::Rc<SignerWaiter>);
impl Drop for SignerWaitGuard {
    fn drop(&mut self) {
        self.0.finish(false);
    }
}
