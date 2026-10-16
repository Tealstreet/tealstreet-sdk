//! Native promise: an async body starts eagerly to its first suspension and the
//! runtime owns the rest, independently of whoever later awaits the handle.
use super::value::{Js, Thrown};
use safe_cex_exchange::runtime::{ExchangeRuntime, ExchangeTask};
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
    waiters: Vec<Waker>,
    task: Option<Box<dyn ExchangeTask>>,
    runtime: Rc<dyn ExchangeRuntime>,
}

#[derive(Clone)]
pub struct Promise(Rc<RefCell<State>>);

impl Promise {
    pub(crate) fn ptr_eq(&self, other: &Promise) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn settled(runtime: &Rc<dyn ExchangeRuntime>, result: Result<Js, Thrown>) -> Self {
        let promise = Self::pending(runtime);
        promise.0.borrow_mut().result = Some(result);
        promise
    }
    pub(crate) fn pending(runtime: &Rc<dyn ExchangeRuntime>) -> Self {
        Self(Rc::new(RefCell::new(State {
            result: None,
            waiters: vec![],
            task: None,
            runtime: runtime.clone(),
        })))
    }
    pub(crate) fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.borrow().runtime.clone()
    }
    /// Settle once; later settlements are ignored like a resolved promise's.
    pub(crate) fn settle(&self, result: Result<Js, Thrown>) {
        let (waiters, task) = {
            let mut state = self.0.borrow_mut();
            if state.result.is_some() {
                return;
            }
            state.result = Some(result);
            (std::mem::take(&mut state.waiters), state.task.take())
        };
        drop(task);
        for waiter in waiters {
            waiter.wake();
        }
    }
    pub(crate) fn result(&self) -> Option<Result<Js, Thrown>> {
        self.0.borrow().result.clone()
    }
    /// Run `body` to its first suspension now; the runtime drives the remainder.
    /// A fulfilled promise value is adopted, as an async function's return is.
    pub(crate) fn spawn(runtime: &Rc<dyn ExchangeRuntime>, body: JsFuture<'static>) -> Self {
        let promise = Promise::pending(runtime);
        let mut body: JsFuture<'static> = Box::pin(adopt(body));
        if let Poll::Ready(result) = body.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            promise.settle(result);
            return promise;
        }
        let driver = promise.clone();
        let task = runtime.spawn_local(Box::pin(async move {
            let result = body.await;
            driver.settle(result);
        }));
        if promise.0.borrow().result.is_none() {
            promise.0.borrow_mut().task = Some(task);
        }
        promise
    }
    pub(crate) fn wait(&self) -> impl Future<Output = Result<Js, Thrown>> + 'static {
        let state = self.0.clone();
        poll_fn(move |cx| {
            let mut state = state.borrow_mut();
            if let Some(result) = &state.result {
                return Poll::Ready(result.clone());
            }
            if !state.waiters.iter().any(|w| w.will_wake(cx.waker())) {
                state.waiters.push(cx.waker().clone());
            }
            Poll::Pending
        })
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
pub(crate) async fn await_js(value: Js) -> Result<Js, Thrown> {
    let mut value = value;
    while let Js::Promise(promise) = &value {
        let promise = promise.clone();
        value = promise.wait().await?;
    }
    Ok(value)
}

/// Promise.all over already-started promises: fulfillment order, first rejection.
pub(crate) async fn all(values: Vec<Js>) -> Result<Js, Thrown> {
    let mut out = Vec::with_capacity(values.len());
    let promises: Vec<Js> = values;
    // A later rejection that settles first must win, so poll every promise each turn.
    let mut results: Vec<Option<Js>> = vec![None; promises.len()];
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
                        let mut fut = Box::pin(p.wait());
                        let _ = fut.as_mut().poll(cx);
                        pending = true;
                    }
                },
                other => results[i] = Some(other.clone()),
            }
        }
        if pending { Poll::Pending } else { Poll::Ready(Ok(())) }
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
pub(crate) async fn all_settled(values: Vec<Js>) -> Result<Js, Thrown> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        out.push(match await_js(value).await {
            Ok(v) => Js::object(vec![("status", Js::str("fulfilled")), ("value", v)]),
            Err(e) => Js::object(vec![("status", Js::str("rejected")), ("reason", e.value)]),
        });
    }
    Ok(Js::array(out))
}
