//! Eager source tasks retain execution independently of Promise observers.
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
/// Clone retains the same native realm and runtime state, not an isolated copy.
pub trait HelperTaskHost<V, E>: Clone + 'static {
    /// Poll the synchronous prefix now, retain pending work, and resume microtasks.
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>);
    /// Source async return resolution/adoption, including original rejection values.
    fn settle_return(&self, result: Result<V, E>) -> Pin<Box<dyn Future<Output = Result<V, E>>>>;
}
struct State<V, E> {
    result: Option<Result<V, E>>,
    wakers: Vec<Waker>,
}
#[derive(Clone)]
pub struct HelperObserver<V, E> {
    state: Rc<RefCell<State<V, E>>>,
}
impl<V: Clone, E: Clone> Future for HelperObserver<V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut s = self.state.borrow_mut();
        if let Some(value) = &s.result {
            Poll::Ready(value.clone())
        } else {
            s.wakers.push(cx.waker().clone());
            Poll::Pending
        }
    }
}
pub(crate) fn start<H: HelperTaskHost<V, E>, V: Clone + 'static, E: Clone + 'static>(
    host: &H,
    body: impl Future<Output = Result<V, E>> + 'static,
) -> HelperObserver<V, E> {
    start_impl(host, body, true)
}
/// Retain a body whose final return was already assimilated by its mapped
/// return_await_value operation. Avoid observing the returned object's then twice.
pub(crate) fn start_adopted<H: HelperTaskHost<V, E>, V: Clone + 'static, E: Clone + 'static>(
    host: &H,
    body: impl Future<Output = Result<V, E>> + 'static,
) -> HelperObserver<V, E> {
    start_impl(host, body, false)
}
fn start_impl<H: HelperTaskHost<V, E>, V: Clone + 'static, E: Clone + 'static>(
    host: &H,
    body: impl Future<Output = Result<V, E>> + 'static,
    assimilate_return: bool,
) -> HelperObserver<V, E> {
    let state = Rc::new(RefCell::new(State {
        result: None,
        wakers: vec![],
    }));
    let output = state.clone();
    let resolver = host.clone();
    host.start_task(Box::pin(async move {
        let value = body.await;
        let value = if assimilate_return {
            resolver.settle_return(value).await
        } else {
            value
        };
        let waiters = {
            let mut s = output.borrow_mut();
            s.result = Some(value);
            std::mem::take(&mut s.wakers)
        };
        for w in waiters {
            w.wake();
        }
    }));
    HelperObserver { state }
}

#[cfg(test)]
#[path = "../tests/support/accepted_helper_clone.rs"]
mod accepted_clone;
