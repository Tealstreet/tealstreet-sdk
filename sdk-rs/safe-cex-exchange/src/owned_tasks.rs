use crate::runtime::{ExchangeError, ExchangeRuntime, ExchangeTask};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll},
};

type Work = Pin<Box<dyn Future<Output = ()>>>;
type OnFinish = Box<dyn FnOnce(bool)>;

/// Weak cancellation authority. Dropping this handle does not cancel the work.
/// Its registry owns the task until completion, cancellation or registry disposal.
#[derive(Clone)]
pub struct OwnedOperation {
    owner: Weak<OwnedTasks>,
    id: u64,
}
impl OwnedOperation {
    pub(crate) fn inactive() -> Self {
        Self {
            owner: Weak::new(),
            id: 0,
        }
    }
    /// Removes the registration before cancelling the host task.
    pub fn cancel(&self) {
        if let Some(owner) = self.owner.upgrade() {
            owner.finish(self.id, true);
        }
    }
    pub fn is_active(&self) -> bool {
        self.owner
            .upgrade()
            .is_some_and(|owner| owner.slots.borrow().contains_key(&self.id))
    }
}

struct Slot {
    terminal: Cell<bool>,
    cancelled: Cell<bool>,
    future: RefCell<Option<Work>>,
    host: RefCell<Option<Box<dyn ExchangeTask>>>,
    cleanup: RefCell<Option<OnFinish>>,
}
impl Slot {
    fn finish(&self, cancelled: bool) {
        if self.terminal.replace(true) {
            return;
        }
        self.cancelled.set(cancelled);
        let future = self.future.borrow_mut().take();
        let host = self.host.borrow_mut().take();
        let cleanup = self.cleanup.borrow_mut().take();
        if cancelled && let Some(host) = host.as_ref() {
            host.cancel();
        }
        drop(future);
        drop(host);
        if let Some(cleanup) = cleanup {
            cleanup(cancelled);
        }
    }
}

#[derive(Default)]
pub(crate) struct OwnedTasks {
    retain_owner: bool,
    closed: Cell<bool>,
    next_id: Cell<u64>,
    slots: RefCell<HashMap<u64, Rc<Slot>>>,
}
impl OwnedTasks {
    pub(crate) fn detached() -> Rc<Self> {
        Rc::new(Self {
            retain_owner: true,
            closed: Cell::new(false),
            next_id: Cell::new(0),
            slots: RefCell::new(HashMap::new()),
        })
    }
    pub(crate) fn start<R: ExchangeRuntime + ?Sized>(
        self: &Rc<Self>,
        runtime: &R,
        build: impl FnOnce(OwnedOperation) -> Work,
        cleanup: impl FnOnce(bool) + 'static,
    ) -> OwnedOperation {
        self.start_inner(runtime, build, cleanup)
    }
    /// Work representing a discarded source promise is retained independently
    /// of an exchange/socket lifecycle owner until the host settles or drops it.
    pub(crate) fn start_detached<R: ExchangeRuntime + ?Sized>(
        runtime: &R,
        future: Work,
    ) -> OwnedOperation {
        Self::detached().start(runtime, |_| future, |_| {})
    }
    fn start_inner<R: ExchangeRuntime + ?Sized>(
        self: &Rc<Self>,
        runtime: &R,
        build: impl FnOnce(OwnedOperation) -> Work,
        cleanup: impl FnOnce(bool) + 'static,
    ) -> OwnedOperation {
        let id = self.next_id.get();
        self.next_id
            .set(id.checked_add(1).expect("owned task ID exhausted"));
        let operation = OwnedOperation {
            owner: Rc::downgrade(self),
            id,
        };
        if self.closed.get() {
            cleanup(true);
            return operation;
        }
        let slot = Rc::new(Slot {
            terminal: Cell::new(false),
            cancelled: Cell::new(false),
            future: RefCell::new(None),
            host: RefCell::new(None),
            cleanup: RefCell::new(Some(Box::new(cleanup))),
        });
        self.slots.borrow_mut().insert(id, slot.clone());
        let future = build(operation.clone());
        if slot.terminal.get() {
            drop(future);
            return operation;
        }
        *slot.future.borrow_mut() = Some(future);
        let host = runtime.spawn_local(Box::pin(Driver {
            operation: operation.clone(),
            slot: slot.clone(),
            owner_lease: self.retain_owner.then(|| self.clone()),
        }));
        if slot.terminal.get() {
            if slot.cancelled.get() {
                host.cancel();
            }
            drop(host);
        } else {
            *slot.host.borrow_mut() = Some(host);
        }
        operation
    }
    pub(crate) fn timer<R: ExchangeRuntime + ?Sized + 'static>(
        self: &Rc<Self>,
        runtime: Rc<R>,
        period_ms: u64,
        repeat: bool,
        mut callback: impl FnMut() -> Result<(), ExchangeError> + 'static,
    ) -> Result<OwnedOperation, ExchangeError> {
        if self.closed.get() {
            return Ok(OwnedOperation::inactive());
        }
        if period_ms == 0 || period_ms > i32::MAX as u64 {
            return Err(ExchangeError::new(
                "timer requires a normalized 1..=2147483647ms delay",
            ));
        }
        let now = runtime.monotonic_now_ms();
        if !now.is_finite() {
            return Err(ExchangeError::new("monotonic clock must be finite"));
        }
        let mut deadline = now + period_ms as f64;
        let task_runtime = runtime.clone();
        Ok(self.start(
            runtime.as_ref(),
            move |operation| {
                Box::pin(async move {
                    let result = async {
                        let sleep_until = |deadline: f64| {
                            let now = task_runtime.monotonic_now_ms();
                            let remaining = deadline - now;
                            if !now.is_finite() || !remaining.is_finite() {
                                return Err(ExchangeError::new(
                                    "monotonic timer deadline must be finite",
                                ));
                            }
                            Ok(task_runtime
                                .sleep(remaining.max(0.0).ceil().min(i32::MAX as f64) as u64))
                        };
                        let mut sleeping = sleep_until(deadline)?;
                        loop {
                            sleeping.await?;
                            if !operation.is_active() {
                                return Ok(());
                            }
                            let captured = task_runtime.monotonic_now_ms();
                            if !captured.is_finite() {
                                return Err(ExchangeError::new("monotonic clock must be finite"));
                            }
                            if captured < deadline {
                                sleeping = sleep_until(deadline)?;
                                continue;
                            }
                            let callback_result = callback();
                            let mut next_sleep = if repeat && operation.is_active() {
                                deadline = captured + period_ms as f64;
                                Some(sleep_until(deadline))
                            } else {
                                None
                            };
                            let next_poll = if let Some(Ok(sleep)) = &mut next_sleep {
                                Some(
                                    std::future::poll_fn(|cx| Poll::Ready(sleep.as_mut().poll(cx)))
                                        .await,
                                )
                            } else {
                                None
                            };
                            // Node reinserts in finally before reporting a callback throw.
                            if let Err(error) = callback_result {
                                task_runtime.report_exception(
                                    &error,
                                    &serde_json::json!({"operation":"owned timer callback"}).into(),
                                );
                            }
                            if !operation.is_active() {
                                return Ok(());
                            }
                            match next_sleep {
                                Some(next_sleep) => sleeping = next_sleep?,
                                None => return Ok(()),
                            }
                            if let Some(Poll::Ready(result)) = next_poll {
                                result?;
                                sleeping = Box::pin(async { Ok(()) });
                            }
                            let mut yield_pending = true;
                            std::future::poll_fn(move |cx| {
                                if std::mem::take(&mut yield_pending) {
                                    cx.waker().wake_by_ref();
                                    Poll::Pending
                                } else {
                                    Poll::Ready(())
                                }
                            })
                            .await;
                        }
                    }
                    .await;
                    if let Err(error) = result {
                        task_runtime.report_exception(
                            &error,
                            &serde_json::json!({"operation":"owned timer"}).into(),
                        );
                    }
                })
            },
            |_| {},
        ))
    }
    fn finish(&self, id: u64, cancelled: bool) {
        let slot = self.slots.borrow_mut().remove(&id);
        if let Some(slot) = slot {
            slot.finish(cancelled);
        }
    }
    pub(crate) fn close(&self) {
        self.closed.set(true);
        let slots = std::mem::take(&mut *self.slots.borrow_mut());
        for slot in slots.into_values() {
            slot.finish(true);
        }
    }
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.slots.borrow().len()
    }
}
impl Drop for OwnedTasks {
    fn drop(&mut self) {
        self.close();
    }
}

struct Driver {
    operation: OwnedOperation,
    slot: Rc<Slot>,
    // Last field: keep the owner alive through Driver::drop's cleanup.
    owner_lease: Option<Rc<OwnedTasks>>,
}
impl Future for Driver {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.slot.terminal.get() {
            this.owner_lease.take();
            return Poll::Ready(());
        }
        let future = this.slot.future.borrow_mut().take();
        let Some(mut future) = future else {
            this.owner_lease.take();
            return Poll::Ready(());
        };
        let result = future.as_mut().poll(cx);
        if result.is_ready() {
            if let Some(owner) = this.operation.owner.upgrade() {
                owner.finish(this.operation.id, false);
            } else {
                this.slot.finish(false);
            }
            this.owner_lease.take();
            Poll::Ready(())
        } else if this.slot.terminal.get() {
            this.owner_lease.take();
            Poll::Ready(())
        } else {
            *this.slot.future.borrow_mut() = Some(future);
            Poll::Pending
        }
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        if let Some(owner) = self.operation.owner.upgrade() {
            owner.finish(self.operation.id, true);
        } else {
            self.slot.finish(true);
        }
    }
}

/// A normalized 1..=2_147_483_647 ms timeout survives exchange disposal.
/// Its callback runs once unless cancelled or discarded by the runtime.
/// Dropping the returned handle does not cancel it.
pub fn start_detached_timeout<R: ExchangeRuntime + ?Sized + 'static>(
    runtime: Rc<R>,
    delay_ms: u64,
    callback: impl FnMut() -> Result<(), ExchangeError> + 'static,
) -> Result<OwnedOperation, ExchangeError> {
    OwnedTasks::detached().timer(runtime, delay_ms, false, callback)
}

#[cfg(test)]
#[path = "owned_tasks_tests.rs"]
mod tests;
