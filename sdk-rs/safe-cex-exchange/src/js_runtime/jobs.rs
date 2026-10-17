use crate::{
    owned_tasks::OwnedTasks,
    runtime::{ExchangeRuntime, ExchangeTask},
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

type Work = Pin<Box<dyn Future<Output = ()>>>;
type Queues = Vec<(Weak<dyn ExchangeRuntime>, Weak<Jobs>)>;
type Current = Option<(Rc<Jobs>, Waker)>;
fn with_queues<R>(callback: impl FnOnce(&RefCell<Queues>) -> R) -> R {
    std::thread_local! {
        static QUEUES: RefCell<Queues> = const { RefCell::new(Vec::new()) };
    }
    QUEUES.with(callback)
}
fn with_current<R>(callback: impl FnOnce(&RefCell<Current>) -> R) -> R {
    std::thread_local! {
        static CURRENT: RefCell<Current> = const { RefCell::new(None) };
    }
    CURRENT.with(callback)
}

#[derive(Default)]
struct Signal {
    next: AtomicU64,
    driver: Mutex<Option<Waker>>,
}
impl Signal {
    fn wake_driver(&self) {
        let driver = self.driver.lock().expect("microtask driver lock").clone();
        if let Some(driver) = driver {
            driver.wake();
        }
    }
}
struct Ready {
    order: Mutex<Option<u64>>,
    signal: Arc<Signal>,
}
impl Ready {
    fn enqueue(&self) {
        let mut order = self.order.lock().expect("microtask readiness lock");
        if order.is_none() {
            *order = Some(self.signal.next.fetch_add(1, Ordering::Relaxed));
        }
        drop(order);
        self.signal.wake_driver();
    }
}
impl Wake for Ready {
    fn wake(self: Arc<Self>) {
        self.enqueue();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.enqueue();
    }
}
struct Job {
    ready: Arc<Ready>,
    work: RefCell<Option<Work>>,
    deadline: Option<f64>,
    cancelled: Cell<bool>,
}

pub(super) struct Jobs {
    runtime: Rc<dyn ExchangeRuntime>,
    signal: Arc<Signal>,
    jobs: RefCell<Vec<Rc<Job>>>,
    checks: RefCell<Vec<Box<dyn FnOnce()>>>,
    running: Cell<bool>,
    draining: Cell<bool>,
}

pub(super) fn for_runtime(runtime: &Rc<dyn ExchangeRuntime>) -> Rc<Jobs> {
    with_queues(|queues| {
        let mut queues = queues.borrow_mut();
        queues.retain(|(runtime, jobs)| runtime.strong_count() > 0 && jobs.strong_count() > 0);
        if let Some(jobs) = queues.iter().find_map(|(owner, jobs)| {
            owner
                .upgrade()
                .filter(|owner| Rc::ptr_eq(owner, runtime))
                .and_then(|_| jobs.upgrade())
        }) {
            return jobs;
        }
        let jobs = Rc::new(Jobs {
            runtime: runtime.clone(),
            signal: Arc::default(),
            jobs: RefCell::default(),
            checks: RefCell::default(),
            running: Cell::new(false),
            draining: Cell::new(false),
        });
        queues.push((Rc::downgrade(runtime), Rc::downgrade(&jobs)));
        jobs
    })
}

struct Enter(Option<(Rc<Jobs>, Waker)>);
impl Drop for Enter {
    fn drop(&mut self) {
        with_current(|current| *current.borrow_mut() = self.0.take());
    }
}
struct Draining<'a>(&'a Cell<bool>);
impl Drop for Draining<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl Jobs {
    pub(super) fn poll<F: Future + ?Sized>(
        self: &Rc<Self>,
        work: Pin<&mut F>,
        cx: &mut Context<'_>,
    ) -> Poll<F::Output> {
        let previous = with_current(|current| {
            current
                .borrow_mut()
                .replace((self.clone(), cx.waker().clone()))
        });
        let _enter = Enter(previous);
        work.poll(cx)
    }
    pub(super) fn spawn(self: &Rc<Self>, work: Work) {
        self.enqueue(work, None, false);
    }
    pub(super) fn start(self: &Rc<Self>, work: Work) {
        self.enqueue(work, None, true);
    }
    pub(super) fn timer(self: &Rc<Self>, deadline: f64, work: Work) -> Box<dyn ExchangeTask> {
        let job = self.enqueue(work, Some(deadline), false);
        Box::new(Task {
            owner: Rc::downgrade(self),
            job: Rc::downgrade(&job),
        })
    }
    pub(super) fn after_checkpoint(self: &Rc<Self>, check: Box<dyn FnOnce()>) {
        self.checks.borrow_mut().push(check);
        self.ensure_driver();
        self.signal.wake_driver();
    }
    fn ensure_driver(self: &Rc<Self>) {
        if (!self.jobs.borrow().is_empty() || !self.checks.borrow().is_empty())
            && !self.running.replace(true)
        {
            OwnedTasks::start_detached(self.runtime.as_ref(), Box::pin(Driver(self.clone())));
        }
    }
    fn enqueue(self: &Rc<Self>, work: Work, deadline: Option<f64>, eager: bool) -> Rc<Job> {
        let ready = Arc::new(Ready {
            order: Mutex::new(None),
            signal: self.signal.clone(),
        });
        let job = Rc::new(Job {
            ready: ready.clone(),
            work: RefCell::new(Some(work)),
            deadline,
            cancelled: Cell::new(false),
        });
        self.jobs.borrow_mut().push(job.clone());
        if eager {
            self.run(job.clone());
        } else {
            ready.enqueue();
        }
        self.ensure_driver();
        job
    }
    fn next(&self, timers: bool) -> Option<Rc<Job>> {
        self.jobs
            .borrow()
            .iter()
            .filter(|job| job.deadline.is_some() == timers)
            .filter_map(|job| {
                let order = *job.ready.order.lock().expect("microtask readiness lock");
                order.map(|order| (order, job.clone()))
            })
            .min_by(|(left, a), (right, b)| {
                a.deadline
                    .unwrap_or(0.0)
                    .total_cmp(&b.deadline.unwrap_or(0.0))
                    .then(left.cmp(right))
            })
            .map(|(_, job)| job)
    }
    fn run(self: &Rc<Self>, job: Rc<Job>) {
        job.ready
            .order
            .lock()
            .expect("microtask readiness lock")
            .take();
        let work = job.work.borrow_mut().take();
        let Some(mut work) = work else {
            return;
        };
        let waker = Waker::from(job.ready.clone());
        match self.poll(work.as_mut(), &mut Context::from_waker(&waker)) {
            Poll::Pending if !job.cancelled.get() => *job.work.borrow_mut() = Some(work),
            _ => {
                self.jobs
                    .borrow_mut()
                    .retain(|entry| !Rc::ptr_eq(entry, &job));
                drop(work);
            }
        }
    }
    pub(super) fn drain(self: &Rc<Self>) {
        if self.draining.replace(true) {
            return;
        }
        let _draining = Draining(&self.draining);
        loop {
            while let Some(job) = self.next(false) {
                self.run(job);
            }
            let checks = self.checks.take();
            if checks.is_empty() {
                break;
            }
            for check in checks {
                check();
            }
        }
    }
    pub(super) fn wake(self: &Rc<Self>, waker: Waker) {
        let active = with_current(|current| {
            current
                .borrow()
                .as_ref()
                .is_some_and(|(jobs, active)| Rc::ptr_eq(jobs, self) && active.will_wake(&waker))
        });
        if active {
            waker.wake();
            return;
        }
        self.spawn(Box::pin(async move {
            waker.wake();
        }));
    }
}

struct Driver(Rc<Jobs>);
impl Future for Driver {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        *self.0.signal.driver.lock().expect("microtask driver lock") = Some(cx.waker().clone());
        self.0.drain();
        if let Some(job) = self.0.next(true) {
            self.0.run(job);
            self.0.drain();
        }
        if self.0.next(true).is_some() {
            cx.waker().wake_by_ref();
        }
        if self.0.jobs.borrow().is_empty() && self.0.checks.borrow().is_empty() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.0.running.set(false);
        self.0
            .signal
            .driver
            .lock()
            .expect("microtask driver lock")
            .take();
        let jobs = self.0.jobs.take();
        let checks = self.0.checks.take();
        drop(jobs);
        drop(checks);
    }
}

struct Task {
    owner: Weak<Jobs>,
    job: Weak<Job>,
}
impl ExchangeTask for Task {
    fn cancel(&self) {
        if let (Some(owner), Some(job)) = (self.owner.upgrade(), self.job.upgrade()) {
            job.cancelled.set(true);
            let work = job.work.borrow_mut().take();
            owner
                .jobs
                .borrow_mut()
                .retain(|entry| !Rc::ptr_eq(entry, &job));
            drop(work);
            owner.signal.wake_driver();
        }
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn checkpoint(runtime: Rc<dyn ExchangeRuntime>) {
    let jobs = with_queues(|queues| {
        queues.borrow().iter().find_map(|(owner, jobs)| {
            owner
                .upgrade()
                .filter(|owner| Rc::ptr_eq(owner, &runtime))
                .and_then(|_| jobs.upgrade())
        })
    });
    let Some(jobs) = jobs else {
        return;
    };
    let nested = with_current(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(|(current, _)| Rc::ptr_eq(current, &jobs))
    });
    if !nested {
        jobs.drain();
    }
}

pub(super) async fn yield_current() {
    let jobs = with_current(|current| current.borrow().clone());
    if let Some((jobs, _)) = jobs {
        let mut queued = false;
        std::future::poll_fn(|cx| {
            if queued {
                return Poll::Ready(());
            }
            queued = true;
            jobs.wake(cx.waker().clone());
            Poll::Pending
        })
        .await;
    }
}
