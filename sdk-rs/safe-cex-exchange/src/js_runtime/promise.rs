//! Native promise: an async body starts eagerly to its first suspension and the
//! runtime owns the rest, independently of whoever later awaits the handle.
use super::{
    jobs::{self, Jobs},
    value::{Js, Thrown},
};
use crate::runtime::ExchangeRuntime;
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
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

/// Promise.all over already-started promises: fulfillment order, first rejection.
pub async fn all(values: Vec<Js>) -> Result<Js, Thrown> {
    let mut out = Vec::with_capacity(values.len());
    let promises: Vec<Js> = values;
    // A later rejection that settles first must win, so poll every promise each turn.
    let mut results: Vec<Option<Js>> = vec![None; promises.len()];
    // Each pending promise keeps one wait registered across turns.
    let mut waits: Vec<Option<PromiseWait>> = promises.iter().map(|_| None).collect();
    poll_fn(|cx| {
        let mut pending = false;
        for (i, value) in promises.iter().enumerate() {
            if results[i].is_some() {
                continue;
            }
            match value {
                Js::Promise(p) => match p.result() {
                    Some(Ok(v)) => results[i] = Some(v),
                    Some(Err(e)) => return Poll::Ready(Err(e)),
                    None => {
                        let wait = waits[i].get_or_insert_with(|| p.wait());
                        let _ = Pin::new(wait).poll(cx);
                        pending = true;
                    }
                },
                other => results[i] = Some(other.clone()),
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    })
    .await?;
    for value in results {
        let mut value = value.unwrap_or(Js::Undefined);
        while let Js::Promise(p) = &value {
            let p = p.clone();
            value = p.wait().await?;
        }
        out.push(value);
    }
    Ok(Js::array(out))
}

/// Promise.allSettled result records.
pub async fn all_settled(values: Vec<Js>) -> Result<Js, Thrown> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        out.push(match await_js(value).await {
            Ok(v) => Js::object(vec![("status", Js::str("fulfilled")), ("value", v)]),
            Err(e) => Js::object(vec![("status", Js::str("rejected")), ("reason", e.value)]),
        });
    }
    Ok(Js::array(out))
}
