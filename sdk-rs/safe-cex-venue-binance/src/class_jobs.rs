//! Eager invocation and shared settlement for native class async members.
//! Lookup errors remain outside the job. Polling starts the callee's prefix before
//! returning its handle; the runtime then owns suspended work independently of
//! the lifetime/polling of callers' settlement handles.
use crate::request_weight::Failure;
use safe_cex_exchange::runtime::{ExchangeRuntime, ExchangeTask};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
pub type NativeFuture<T> = Pin<Box<dyn Future<Output = Result<T, Failure>>>>;
struct State<T> {
    result: Option<Result<T, Failure>>,
    waiters: Vec<Waker>,
    task: Option<Box<dyn ExchangeTask>>,
    runtime: Rc<dyn ExchangeRuntime>,
    observed: bool,
    report_scheduled: bool,
}
pub struct Settlement<T: Clone + 'static> {
    state: Rc<RefCell<State<T>>>,
}
impl<T: Clone + 'static> Clone for Settlement<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}
impl<T: Clone + 'static> Future for Settlement<T> {
    type Output = Result<T, Failure>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.borrow_mut();
        state.observed = true;
        if let Some(result) = &state.result {
            return Poll::Ready(result.clone());
        }
        if !state.waiters.iter().any(|w| w.will_wake(cx.waker())) {
            state.waiters.push(cx.waker().clone());
        }
        Poll::Pending
    }
}
fn finish<T: Clone + 'static>(state: &Rc<RefCell<State<T>>>, result: Result<T, Failure>) {
    let (waiters, task) = {
        let mut state = state.borrow_mut();
        state.result = Some(result);
        (std::mem::take(&mut state.waiters), state.task.take())
    };
    drop(task);
    schedule_unobserved(state);
    for waiter in waiters {
        waiter.wake();
    }
}
pub fn start<T: Clone + 'static>(
    runtime: Rc<dyn ExchangeRuntime>,
    future: NativeFuture<T>,
) -> Settlement<T> {
    let mut future: NativeFuture<T> =
        Box::pin(safe_cex_num::PrototypeRealm::current().future(future));
    let state = Rc::new(RefCell::new(State {
        result: None,
        waiters: vec![],
        task: None,
        runtime: runtime.clone(),
        observed: false,
        report_scheduled: false,
    }));
    if let Poll::Ready(result) = future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        finish(&state, result);
        return Settlement { state };
    }
    let retained = state.clone();
    let task = runtime.spawn_local(Box::pin(async move {
        let result = poll_fn(move |cx| future.as_mut().poll(cx)).await;
        finish(&retained, result);
    }));
    if state.borrow().result.is_none() {
        state.borrow_mut().task = Some(task);
    } else {
        drop(task)
    }
    Settlement { state }
}

/// Await an already-launched source promise. A first-poll Ready result queues
/// its continuation; Pending hosts must wake through the runtime queue (never
/// poll a task reentrantly inside the request/resolve operation).
pub async fn reaction<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut suspended = false;
    let mut ready = None;
    poll_fn(move |cx| {
        if let Some(value) = ready.take() {
            return Poll::Ready(value);
        }
        match future.as_mut().poll(cx) {
            Poll::Pending => {
                suspended = true;
                Poll::Pending
            }
            Poll::Ready(value) => {
                if suspended {
                    Poll::Ready(value)
                } else {
                    ready = Some(value);
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        }
    })
    .await
}

/// Host analogue of a deliberately unobserved source promise rejection.
pub fn detach<T: Clone + 'static>(runtime: Rc<dyn ExchangeRuntime>, settlement: Settlement<T>) {
    let reporter = runtime.clone();
    drop(start(
        runtime,
        Box::pin(async move {
            if let Err(error) = reaction(settlement).await {
                reporter.report_exception(
                    &error,
                    &serde_json::json!({"operation":"binance detached class job"}).into(),
                );
            }
            Ok(())
        }),
    ));
}

fn schedule_unobserved<T: Clone + 'static>(state: &Rc<RefCell<State<T>>>) {
    let runtime = {
        let mut state = state.borrow_mut();
        if state.observed || state.report_scheduled || !matches!(state.result, Some(Err(_))) {
            return;
        }
        state.report_scheduled = true;
        state.runtime.clone()
    };
    let original = state.clone();
    let reporter = runtime.clone();
    drop(start(
        runtime,
        Box::pin(async move {
            reaction(std::future::ready(())).await;
            let error = {
                let state = original.borrow();
                if state.observed {
                    None
                } else {
                    state
                        .result
                        .as_ref()
                        .and_then(|r| r.as_ref().err())
                        .cloned()
                }
            };
            if let Some(error) = error {
                reporter.report_exception(
                    &error,
                    &serde_json::json!({"operation":"binance unobserved class settlement"}).into(),
                );
            }
            Ok(())
        }),
    ));
}
