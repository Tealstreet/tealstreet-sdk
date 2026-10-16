//! Per-future registrations for shared local operations. Waker identity is not
//! waiter identity: two independently cancellable futures may share a waker.
use std::{cell::RefCell, rc::Rc, task::Waker};

type Entry = (Rc<()>, Waker);
#[derive(Clone, Default)]
pub(crate) struct Waiters(Rc<RefCell<Vec<Entry>>>);
impl Waiters {
    pub(crate) fn register(&self) -> Registration {
        Registration {
            waiters: self.clone(),
            identity: Rc::new(()),
        }
    }
    pub(crate) fn wake_all(&self) {
        let entries = std::mem::take(&mut *self.0.borrow_mut());
        for (_, waker) in entries {
            waker.wake();
        }
    }
}
pub(crate) struct Registration {
    waiters: Waiters,
    identity: Rc<()>,
}
impl Registration {
    pub(crate) fn update(&self, waker: &Waker) {
        let replacement = waker.clone();
        let previous = {
            let mut entries = self.waiters.0.borrow_mut();
            if let Some((_, current)) = entries
                .iter_mut()
                .find(|(id, _)| Rc::ptr_eq(id, &self.identity))
            {
                if current.will_wake(waker) {
                    return;
                }
                Some(std::mem::replace(current, replacement))
            } else {
                entries.push((self.identity.clone(), replacement));
                None
            }
        };
        // Custom waker destruction may re-enter the operation.
        drop(previous);
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        let removed = {
            let mut entries = self.waiters.0.borrow_mut();
            entries
                .iter()
                .position(|(id, _)| Rc::ptr_eq(id, &self.identity))
                .map(|index| entries.remove(index))
        };
        drop(removed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::Wake,
    };
    #[derive(Default)]
    struct Counter(AtomicUsize);
    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn cancelling_one_registration_retains_same_waker_sibling() {
        let waiters = Waiters::default();
        let first = waiters.register();
        let second = waiters.register();
        let counter = Arc::new(Counter::default());
        let waker = Waker::from(counter.clone());
        first.update(&waker);
        second.update(&waker);
        drop(first);
        waiters.wake_all();
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            1,
            "cancelled waiter must not remove same-waker sibling"
        );
        drop(second);
    }
    #[test]
    fn replacement_and_drop_release_wakers_without_settling_shared_work() {
        let waiters = Waiters::default();
        let registration = waiters.register();
        let old = Arc::new(Counter::default());
        let weak = Arc::downgrade(&old);
        let old_waker = Waker::from(old);
        registration.update(&old_waker);
        drop(old_waker);
        let new = Arc::new(Counter::default());
        let new_weak = Arc::downgrade(&new);
        let new_waker = Waker::from(new);
        registration.update(&new_waker);
        drop(new_waker);
        assert!(
            weak.upgrade().is_none(),
            "re-poll must release replaced waker"
        );
        drop(registration);
        assert!(
            new_weak.upgrade().is_none(),
            "waiter drop must release its waker before shared completion"
        );
        waiters.wake_all();
    }
}
