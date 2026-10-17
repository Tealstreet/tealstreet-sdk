use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    rc::{Rc, Weak},
    task::{Poll, Waker},
};

type Waiter = Weak<RefCell<Option<Waker>>>;
#[derive(Default)]
pub struct Release {
    ready: Rc<Cell<bool>>,
    waiters: RefCell<Vec<Waiter>>,
}
impl Release {
    pub fn set(&self, ready: bool) {
        self.ready.set(ready);
        if !ready {
            return;
        }
        let mut wake = Vec::new();
        self.waiters.borrow_mut().retain(|waiter| {
            let Some(waiter) = waiter.upgrade() else {
                return false;
            };
            if let Some(waker) = waiter.borrow_mut().take() {
                wake.push(waker);
            }
            true
        });
        for waker in wake {
            waker.wake();
        }
    }
    pub fn wait(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        let ready = self.ready.clone();
        let waiter = Rc::new(RefCell::new(None));
        self.waiters.borrow_mut().push(Rc::downgrade(&waiter));
        Box::pin(poll_fn(move |cx| {
            if ready.get() {
                Poll::Ready(())
            } else {
                *waiter.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }))
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
        task::{Context, Wake},
    };
    #[derive(Default)]
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[test]
    fn release_wakes_all_live_requests_and_discards_cancelled_waiters() {
        let release = Release::default();
        let first = Arc::new(Count::default());
        let second = Arc::new(Count::default());
        let cancelled = Arc::new(Count::default());
        let mut one = release.wait();
        let mut two = release.wait();
        let mut dropped = release.wait();
        for (future, signal) in [
            (&mut one, &first),
            (&mut two, &second),
            (&mut dropped, &cancelled),
        ] {
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(&Waker::from(signal.clone())))
                    .is_pending()
            );
        }
        drop(dropped);
        release.set(true);
        assert_eq!(first.0.load(Ordering::Relaxed), 1, "first request wake");
        assert_eq!(second.0.load(Ordering::Relaxed), 1, "second request wake");
        assert_eq!(
            cancelled.0.load(Ordering::Relaxed),
            0,
            "cancelled request wake"
        );
        assert!(
            one.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        );
        assert!(
            two.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready()
        );
    }
}
