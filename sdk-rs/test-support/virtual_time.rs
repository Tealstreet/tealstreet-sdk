use std::{
    cell::RefCell,
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Wake, Waker},
};

pub type SleepWaker = Rc<RefCell<Option<Waker>>>;
type Sleeper = (f64, Weak<RefCell<Option<Waker>>>);

#[derive(Default)]
pub struct SleepWakers(RefCell<Vec<Sleeper>>);
impl SleepWakers {
    pub fn register(&self, deadline: f64) -> SleepWaker {
        let slot = Rc::new(RefCell::new(None));
        self.0.borrow_mut().push((deadline, Rc::downgrade(&slot)));
        slot
    }
    pub fn wake_due(&self, now: f64) {
        let mut ready = Vec::new();
        self.0.borrow_mut().retain(|(deadline, slot)| {
            let Some(slot) = slot.upgrade() else {
                return false;
            };
            if now >= *deadline
                && let Some(waker) = slot.borrow_mut().take()
            {
                ready.push(waker);
            }
            true
        });
        for waker in ready {
            waker.wake();
        }
    }
}

#[derive(Default)]
pub struct WakeSignal(AtomicBool);
impl WakeSignal {
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
    pub fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }
}
impl Wake for WakeSignal {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }
}
