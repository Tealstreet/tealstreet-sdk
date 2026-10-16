//! Shared results over the existing exchange-owned operation and waiter owners.
use crate::{
    owned_tasks::{OwnedOperation, OwnedTasks},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
    waiters::Waiters,
};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    rc::Rc,
    task::Poll,
};

struct State<T> {
    result: RefCell<Option<Rc<Result<T, ExchangeError>>>>,
    waiters: Waiters,
}
impl<T> State<T> {
    fn settle(&self, value: Result<T, ExchangeError>) {
        let value = Rc::new(value);
        let published = {
            let mut result = self.result.borrow_mut();
            if result.is_none() {
                *result = Some(value.clone());
                true
            } else {
                false
            }
        };
        drop(value);
        if published {
            self.waiters.wake_all();
        }
    }
}
/// A retained result whose work belongs to the exchange, independent of callers.
#[derive(Clone)]
pub struct SharedOperation<T> {
    state: Rc<State<T>>,
    operation: OwnedOperation,
}
impl<T: Clone + 'static> SharedOperation<T> {
    /// Each wait has its own cancellation registration, even for a shared waker.
    pub fn wait(&self) -> ExchangeFuture<'static, T> {
        let state = self.state.clone();
        let registration = state.waiters.register();
        Box::pin(async move {
            poll_fn(move |cx| {
                registration.update(cx.waker());
                let result = state.result.borrow().clone();
                match result {
                    Some(result) => Poll::Ready(result.as_ref().clone()),
                    None => Poll::Pending,
                }
            })
            .await
        })
    }
    pub fn cancel(&self) {
        self.operation.cancel();
    }
    pub fn is_active(&self) -> bool {
        self.operation.is_active()
    }
}
pub(crate) fn start<T, R, Build, Work>(
    owner: &Rc<OwnedTasks>,
    runtime: &R,
    build: Build,
) -> SharedOperation<T>
where
    T: Clone + 'static,
    R: ExchangeRuntime + ?Sized,
    Build: FnOnce(SharedOperation<T>) -> Work,
    Work: Future<Output = Result<T, ExchangeError>> + 'static,
{
    let state = Rc::new(State {
        result: RefCell::new(None),
        waiters: Waiters::default(),
    });
    let work_state = state.clone();
    let cleanup_state = state.clone();
    let operation = owner.start(
        runtime,
        move |operation| {
            let work = build(SharedOperation {
                state: work_state.clone(),
                operation,
            });
            Box::pin(async move {
                work_state.settle(work.await);
            })
        },
        move |cancelled| {
            cleanup_state.settle(Err(ExchangeError::new(if cancelled {
                "Shared operation cancelled by its owner"
            } else {
                "Shared operation completed without a result"
            })));
        },
    );
    SharedOperation { state, operation }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        rc::Weak,
        task::{Context, Waker},
    };
    struct ReentrantValue {
        state: Weak<State<ReentrantValue>>,
        clones: Rc<Cell<usize>>,
    }
    impl Clone for ReentrantValue {
        fn clone(&self) -> Self {
            let state = self.state.upgrade().unwrap();
            let borrow = state.result.borrow_mut();
            self.clones.set(self.clones.get() + 1);
            drop(borrow);
            Self {
                state: self.state.clone(),
                clones: self.clones.clone(),
            }
        }
    }
    #[test]
    fn shared_result_observer_clone_can_reenter_result_state() {
        let state = Rc::new(State {
            result: RefCell::new(None),
            waiters: Waiters::default(),
        });
        let clones = Rc::new(Cell::new(0));
        state.settle(Ok(ReentrantValue {
            state: Rc::downgrade(&state),
            clones: clones.clone(),
        }));
        let operation = SharedOperation {
            state,
            operation: OwnedOperation::inactive(),
        };
        let result = operation
            .wait()
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        assert!(
            matches!(result, Poll::Ready(Ok(_))),
            "shared_native_reentrant_clone_completes"
        );
        assert_eq!(clones.get(), 1, "shared_native_reentrant_clone_count");
    }
}
