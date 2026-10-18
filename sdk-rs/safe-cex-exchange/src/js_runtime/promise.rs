//! Native promise: an async body starts eagerly to its first suspension and the
//! runtime owns the rest, independently of whoever later awaits the handle.
use super::{
    jobs::{self, Jobs},
    value::{Js, Thrown, arg},
};
use crate::runtime::ExchangeRuntime;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub type JsFuture<'a> = Pin<Box<dyn Future<Output = Result<Js, Thrown>> + 'a>>;

struct State {
    result: Option<Result<Js, Thrown>>,
    handled: bool,
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
        promise.settle(result);
        promise
    }
    pub(super) fn resolve(runtime: &Rc<dyn ExchangeRuntime>, value: Js) -> Self {
        if let Js::Promise(promise) = value {
            return promise;
        }
        let promise = Self::pending(runtime);
        resolve_promise(promise.clone(), value);
        promise
    }
    pub fn pending(runtime: &Rc<dyn ExchangeRuntime>) -> Self {
        Self(Rc::new(RefCell::new(State {
            result: None,
            handled: false,
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
        let (waiters, jobs, rejected) = {
            let mut state = self.0.borrow_mut();
            if state.result.is_some() {
                return;
            }
            let rejected = result.is_err();
            state.result = Some(result);
            (
                std::mem::take(&mut state.waiters),
                state.jobs.clone(),
                rejected,
            )
        };
        for (_, waiter) in waiters {
            waiter.wake();
        }
        if rejected {
            let owner = self.0.clone();
            jobs.after_checkpoint(Box::new(move || {
                let report = {
                    let state = owner.borrow();
                    if state.handled {
                        None
                    } else {
                        state
                            .result
                            .as_ref()
                            .and_then(|result| result.as_ref().err())
                            .map(|error| (state.runtime.clone(), error.clone()))
                    }
                };
                if let Some((runtime, error)) = report {
                    runtime.report_exception(
                        &error.to_exchange_error(),
                        &serde_json::json!({"source":"unhandledRejection"}).into(),
                    );
                }
            }));
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
            match body.await {
                Ok(value) => resolve_promise(driver, value),
                Err(error) => driver.settle(Err(error)),
            }
        }));
        promise
    }
    pub fn wait(&self) -> PromiseWait {
        let mut wait = self.reaction();
        wait.defer_settled = true;
        wait
    }
    pub(super) fn reaction(&self) -> PromiseWait {
        self.0.borrow_mut().handled = true;
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

// CreateResolvingFunctions shares one lock for resolve/reject, including while
// resolving to a still-pending promise or thenable.
pub(super) fn resolving_functions(promise: Promise) -> (Js, Js, Rc<Cell<bool>>) {
    let called = Rc::new(Cell::new(false));
    let resolved = called.clone();
    let rejected = called.clone();
    let target = promise.clone();
    let resolve = Js::function("", move |_, args| {
        if !resolved.replace(true) {
            resolve_promise(target.clone(), arg(&args, 0));
        }
        Ok(Js::Undefined)
    });
    let reject = Js::function("", move |_, args| {
        if !rejected.replace(true) {
            promise.settle(Err(Thrown::new(arg(&args, 0))));
        }
        Ok(Js::Undefined)
    });
    (resolve, reject, called)
}
fn resolve_promise(promise: Promise, value: Js) {
    if let Js::Promise(other) = &value {
        if promise.ptr_eq(other) {
            promise.settle(Err(Thrown::type_error(
                "Chaining cycle detected for promise",
            )));
            return;
        }
    }
    if !matches!(
        &value,
        Js::Array(_)
            | Js::Object(_)
            | Js::Function(_)
            | Js::Promise(_)
            | Js::Map(_)
            | Js::Set(_)
            | Js::Builtin(_)
    ) {
        promise.settle(Ok(value));
        return;
    }
    // Get then synchronously, but invoke it as a promise job after this stack.
    let then = match value.get("then") {
        Ok(then) if then.is_callable() => then,
        Ok(_) => {
            promise.settle(Ok(value));
            return;
        }
        Err(error) => {
            promise.settle(Err(error));
            return;
        }
    };
    let runtime = promise.runtime();
    super::jobs::for_runtime(&runtime).spawn(Box::pin(async move {
        let (resolve, reject, called) = resolving_functions(promise.clone());
        if let Err(error) = then.call(value, vec![resolve, reject]) {
            if !called.replace(true) {
                promise.settle(Err(error));
            }
        }
    }));
}

/// `await value`: canonical resolution followed by exactly one await boundary.
pub fn await_js(
    runtime: &Rc<dyn ExchangeRuntime>,
    value: Js,
) -> impl std::future::Future<Output = Result<Js, Thrown>> + 'static + use<> {
    let runtime = runtime.clone();
    async move { Promise::resolve(&runtime, value).wait().await }
}

/// Promise.all registers every element's resolution reaction synchronously.
pub fn all(runtime: &Rc<dyn ExchangeRuntime>, values: Vec<Js>) -> Promise {
    combine(runtime, values.into_iter().map(Ok), false)
}

/// Promise.allSettled keeps fulfillment order and original rejection values.
pub fn all_settled(runtime: &Rc<dyn ExchangeRuntime>, values: Vec<Js>) -> Promise {
    combine(runtime, values.into_iter().map(Ok), true)
}

/// Resolve/register each yielded input before advancing the original iterator.
pub(super) fn all_iterable(
    runtime: &Rc<dyn ExchangeRuntime>,
    value: &Js,
    settled: bool,
) -> Promise {
    let iterator = match super::iterator::IteratorRecord::get(value) {
        Ok(iterator) => iterator,
        Err(error) => return Promise::settled(runtime, Err(error)),
    };
    let values = std::iter::from_fn(move || iterator.step_value().transpose());
    combine(runtime, values, settled)
}

pub(super) fn race_iterable(runtime: &Rc<dyn ExchangeRuntime>, value: &Js) -> Promise {
    let result = Promise::pending(runtime);
    let iterator = match super::iterator::IteratorRecord::get(value) {
        Ok(iterator) => iterator,
        Err(error) => {
            result.settle(Err(error));
            return result;
        }
    };
    loop {
        let value = match iterator.step_value() {
            Ok(Some(value)) => value,
            Ok(None) => return result,
            Err(error) => {
                result.settle(Err(error));
                return result;
            }
        };
        let input = Promise::resolve(runtime, value);
        let output = result.clone();
        Promise::spawn(
            runtime,
            Box::pin(async move {
                output.settle(input.wait().await);
                Ok(Js::Undefined)
            }),
        );
    }
}

fn combine(
    runtime: &Rc<dyn ExchangeRuntime>,
    values: impl Iterator<Item = Result<Js, Thrown>>,
    settled: bool,
) -> Promise {
    let result = Promise::pending(runtime);
    // Keep the iteration sentinel until all yielded reactions are registered.
    let remaining = Rc::new(std::cell::Cell::new(1));
    let output = Rc::new(RefCell::new(vec![]));
    for (index, value) in values.enumerate() {
        let value = match value {
            Ok(value) => value,
            Err(error) => {
                result.settle(Err(error));
                return result;
            }
        };
        output.borrow_mut().push(None);
        let input = Promise::resolve(runtime, value);
        remaining.set(remaining.get() + 1);
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
    remaining.set(remaining.get() - 1);
    if remaining.get() == 0 {
        let values = output
            .borrow()
            .iter()
            .map(|value| value.clone().expect("every fulfilled combinator slot"))
            .collect();
        result.settle(Ok(Js::array(values)));
    }
    result
}
