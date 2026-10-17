//! Native promise: an async body starts eagerly to its first suspension and the
//! runtime owns the rest, independently of whoever later awaits the handle.
use super::{
    jobs::{self, Jobs},
    value::{Js, Thrown},
};
use crate::runtime::ExchangeRuntime;
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub type JsFuture<'a> = Pin<Box<dyn Future<Output = Result<Js, Thrown>> + 'a>>;

struct State {
    result: Option<Result<Js, Thrown>>,
    /// One entry per pending `PromiseWait`, removed by that wait's Drop.
    waiters: Vec<(u64, Waker)>,
    next_waiter: u64,
    jobs: Rc<Jobs>,
    runtime: Rc<dyn ExchangeRuntime>,
}

#[derive(Clone)]
pub struct Promise(Rc<RefCell<State>>);

impl Promise {
    pub fn ptr_eq(&self, other: &Promise) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    pub fn settled(runtime: &Rc<dyn ExchangeRuntime>, result: Result<Js, Thrown>) -> Self {
        let promise = Self::pending(runtime);
        promise.0.borrow_mut().result = Some(result);
        promise
    }
    pub fn pending(runtime: &Rc<dyn ExchangeRuntime>) -> Self {
        Self(Rc::new(RefCell::new(State {
            result: None,
            waiters: vec![],
            next_waiter: 0,
            jobs: jobs::for_runtime(runtime),
            runtime: runtime.clone(),
        })))
    }
    pub fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.borrow().runtime.clone()
    }
    /// Settle once; later settlements are ignored like a resolved promise's.
    pub fn settle(&self, result: Result<Js, Thrown>) {
        let waiters = {
            let mut state = self.0.borrow_mut();
            if state.result.is_some() {
                return;
            }
            state.result = Some(result);
            std::mem::take(&mut state.waiters)
        };
        for (_, waiter) in waiters {
            waiter.wake();
        }
    }
    pub fn result(&self) -> Option<Result<Js, Thrown>> {
        self.0.borrow().result.clone()
    }
    /// Run `body` to its first suspension now; the runtime drives the remainder.
    /// A fulfilled promise value is adopted, as an async function's return is.
    pub fn spawn(runtime: &Rc<dyn ExchangeRuntime>, body: JsFuture<'static>) -> Self {
        let promise = Promise::pending(runtime);
        let driver = promise.clone();
        let jobs = promise.0.borrow().jobs.clone();
        jobs.start(Box::pin(async move {
            driver.settle(adopt(body).await);
        }));
        promise
    }
    pub fn wait(&self) -> PromiseWait {
        let mut wait = self.reaction();
        wait.defer_settled = true;
        wait
    }
    pub(super) fn reaction(&self) -> PromiseWait {
        PromiseWait {
            state: self.0.clone(),
            registration: None,
            defer_settled: false,
        }
    }
}

/// One awaiter of a promise. Dropping it releases its waker without settling
/// or cancelling the promise, which other awaiters and the runtime still own.
pub struct PromiseWait {
    state: Rc<RefCell<State>>,
    registration: Option<u64>,
    defer_settled: bool,
}
impl Future for PromiseWait {
    type Output = Result<Js, Thrown>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let registration = self.registration;
        let owner = self.state.clone();
        let mut state = owner.borrow_mut();
        if let Some(result) = &state.result {
            if self.defer_settled {
                let jobs = state.jobs.clone();
                drop(state);
                self.defer_settled = false;
                jobs.wake(cx.waker().clone());
                return Poll::Pending;
            }
            return Poll::Ready(result.clone());
        }
        self.defer_settled = false;
        let replaced = match registration
            .and_then(|id| state.waiters.iter_mut().find(|(entry, _)| *entry == id))
        {
            Some((_, waker)) if waker.will_wake(cx.waker()) => None,
            Some((_, waker)) => Some(std::mem::replace(waker, cx.waker().clone())),
            None => {
                let id = state.next_waiter;
                state.next_waiter += 1;
                state.waiters.push((id, cx.waker().clone()));
                drop(state);
                self.registration = Some(id);
                return Poll::Pending;
            }
        };
        drop(state);
        // A replaced host waker is destroyed after the borrow is released.
        drop(replaced);
        Poll::Pending
    }
}
impl Drop for PromiseWait {
    fn drop(&mut self) {
        let Some(id) = self.registration else {
            return;
        };
        let removed = {
            let mut state = self.state.borrow_mut();
            let index = state.waiters.iter().position(|(entry, _)| *entry == id);
            index.map(|index| state.waiters.swap_remove(index))
        };
        drop(removed);
    }
}

async fn adopt(body: JsFuture<'static>) -> Result<Js, Thrown> {
    let mut value = body.await?;
    while let Js::Promise(inner) = &value {
        let inner = inner.clone();
        jobs::yield_current().await;
        value = inner.wait().await?;
    }
    Ok(value)
}

/// `await value`: promises are awaited, any other value resolves to itself.
pub async fn await_js(value: Js) -> Result<Js, Thrown> {
    if !matches!(value, Js::Promise(_)) {
        jobs::yield_current().await;
    }
    let mut value = value;
    while let Js::Promise(promise) = &value {
        let promise = promise.clone();
        value = promise.wait().await?;
    }
    Ok(value)
}

/// Promise.all registers every element's resolution reaction synchronously.
pub fn all(runtime: &Rc<dyn ExchangeRuntime>, values: Vec<Js>) -> Promise {
    combine(runtime, values, false)
}

/// Promise.allSettled keeps fulfillment order and original rejection values.
pub fn all_settled(runtime: &Rc<dyn ExchangeRuntime>, values: Vec<Js>) -> Promise {
    combine(runtime, values, true)
}

fn combine(runtime: &Rc<dyn ExchangeRuntime>, values: Vec<Js>, settled: bool) -> Promise {
    let result = Promise::pending(runtime);
    if values.is_empty() {
        result.settle(Ok(Js::array(vec![])));
        return result;
    }
    let remaining = Rc::new(std::cell::Cell::new(values.len()));
    let output = Rc::new(RefCell::new(vec![None; values.len()]));
    for (index, value) in values.into_iter().enumerate() {
        let input = match value {
            Js::Promise(promise) => promise,
            other => Promise::settled(runtime, Ok(other)),
        };
        let result = result.clone();
        let remaining = remaining.clone();
        let output = output.clone();
        Promise::spawn(
            runtime,
            Box::pin(async move {
                let value = match input.wait().await {
                    Ok(value) if settled => {
                        Js::object(vec![("status", Js::str("fulfilled")), ("value", value)])
                    }
                    Ok(value) => value,
                    Err(error) if settled => Js::object(vec![
                        ("status", Js::str("rejected")),
                        ("reason", error.value),
                    ]),
                    Err(error) => {
                        result.settle(Err(error));
                        return Ok(Js::Undefined);
                    }
                };
                output.borrow_mut()[index] = Some(value);
                remaining.set(remaining.get() - 1);
                if remaining.get() == 0 {
                    let values = output
                        .borrow()
                        .iter()
                        .map(|value| value.clone().expect("every fulfilled combinator slot"))
                        .collect();
                    result.settle(Ok(Js::array(values)));
                }
                Ok(Js::Undefined)
            }),
        );
    }
    result
}
