use safe_cex_venue_coinbase::starbase_auth::{
    CoinbaseStarbaseLocalFuture, CoinbaseStarbasePromiseSpawner,
};
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};
#[derive(Clone, Default)]
pub struct Microtasks {
    tasks: Rc<RefCell<Vec<Option<CoinbaseStarbaseLocalFuture<()>>>>>,
    wakes: Arc<Mutex<VecDeque<usize>>>,
}
struct Signal {
    id: usize,
    wakes: Arc<Mutex<VecDeque<usize>>>,
}
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.lock().unwrap().push_back(self.id);
    }
}
impl Microtasks {
    pub fn spawner(&self) -> CoinbaseStarbasePromiseSpawner {
        let this = self.clone();
        Rc::new(move |future| {
            let id = {
                let mut tasks = this.tasks.borrow_mut();
                let id = tasks.len();
                tasks.push(Some(future));
                id
            };
            this.poll_task(id);
        })
    }
    fn poll_task(&self, id: usize) {
        let Some(mut future) = self.tasks.borrow_mut()[id].take() else {
            return;
        };
        let waker = Waker::from(Arc::new(Signal {
            id,
            wakes: self.wakes.clone(),
        }));
        if future
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
        {
            self.tasks.borrow_mut()[id] = Some(future);
        }
    }
    pub fn tick(&self) -> bool {
        let id = self.wakes.lock().unwrap().pop_front();
        if let Some(id) = id {
            self.poll_task(id);
            true
        } else {
            false
        }
    }
    pub fn drain(&self) {
        for _ in 0..1000 {
            if !self.tick() {
                return;
            }
        }
        panic!("local microtask enumeration stopped at 1000 without quiescence");
    }
    pub fn enqueue(&self, action: impl FnOnce() + 'static) {
        (self.spawner())(Box::pin(Job {
            deferred: false,
            action: Some(Box::new(action)),
        }));
    }
}
struct Job {
    deferred: bool,
    action: Option<Box<dyn FnOnce()>>,
}
impl Future for Job {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if !self.deferred {
            self.deferred = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        self.action.take().unwrap()();
        Poll::Ready(())
    }
}
pub fn completed<T: Clone>(
    mut future: safe_cex_venue_coinbase::starbase_auth::CoinbaseStarbasePromise<T>,
) -> T {
    // Inspect a fulfilled completion without executing a producer; the first poll
    // schedules the mandatory await suspension, the second reads its native value.
    for _ in 0..2 {
        if let Poll::Ready(value) =
            Pin::new(&mut future).poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
    panic!("completion promise still pending")
}
pub fn is_pending<T: Clone>(
    future: &mut safe_cex_venue_coinbase::starbase_auth::CoinbaseStarbasePromise<T>,
) -> bool {
    Pin::new(future)
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending()
}
