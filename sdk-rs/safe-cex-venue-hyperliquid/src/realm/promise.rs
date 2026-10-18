//! Promises and task driving on the exchange runtime's local executor.
//!
//! CHOSEN: reactions run as runtime tasks, not on a V8 microtask queue, so
//! settlement order between unrelated promises can differ from Node. Ordering
//! within one async body is preserved because each await resumes in sequence.
use super::{
    Realm,
    value::{Kind, PromiseCell, PromiseState, Thrown, Value},
};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub(crate) type Task = Pin<Box<dyn Future<Output = ()>>>;
pub(crate) type ValueFuture = Pin<Box<dyn Future<Output = Result<Value, Thrown>>>>;

struct Await {
    cell: Rc<RefCell<PromiseCell>>,
}
impl Future for Await {
    type Output = Result<Value, Thrown>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut cell = self.cell.borrow_mut();
        cell.handled = true;
        match &cell.state {
            PromiseState::Fulfilled(v) => Poll::Ready(Ok(v.clone())),
            PromiseState::Rejected(v) => Poll::Ready(Err(v.clone())),
            PromiseState::Pending => {
                if !cell.wakers.iter().any(|w| w.will_wake(cx.waker())) {
                    cell.wakers.push(cx.waker().clone());
                }
                Poll::Pending
            }
        }
    }
}

impl Realm {
    pub(crate) fn new_promise(&self) -> (Value, Rc<RefCell<PromiseCell>>) {
        let cell = Rc::new(RefCell::new(PromiseCell::default()));
        let p = self.alloc(
            Kind::Promise(cell.clone()),
            Some(&self.intrinsics().promise_proto),
        );
        (p, cell)
    }
    pub(crate) fn settled_promise(&self, result: Result<Value, Thrown>) -> Value {
        let (p, cell) = self.new_promise();
        settle(&cell, result);
        p
    }
    /// Resolve with adoption: a promise result is followed, not nested.
    pub(crate) fn resolve_promise(&self, cell: &Rc<RefCell<PromiseCell>>, value: Value) {
        if let Some(inner) = value.promise() {
            if Rc::ptr_eq(&inner, cell) {
                let e = self.type_error("Chaining cycle detected for promise #<Promise>");
                settle(cell, Err(e));
                return;
            }
            let cell = cell.clone();
            self.spawn(Box::pin(async move {
                let result = Await { cell: inner }.await;
                settle(&cell, result);
            }));
            return;
        }
        settle(cell, Ok(value));
    }
    /// Await: a promise suspends until settled; any other value resumes with itself.
    pub(crate) fn await_value(&self, value: Value) -> ValueFuture {
        match value.promise() {
            Some(cell) => Box::pin(Await { cell }),
            None => Box::pin(std::future::ready(Ok(value))),
        }
    }
    /// A promise for `body`, started now (its synchronous prefix runs immediately).
    pub(crate) fn spawn_value(&self, body: ValueFuture) -> Value {
        let (p, cell) = self.new_promise();
        let realm = self.clone();
        self.start(Box::pin(async move {
            match body.await {
                Ok(v) => realm.resolve_promise(&cell, v),
                Err(e) => settle(&cell, Err(e)),
            }
        }));
        p
    }
    /// Poll the synchronous prefix now; hand the remainder to the runtime.
    pub(crate) fn start(&self, mut task: Task) {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        if task.as_mut().poll(&mut cx).is_ready() {
            return;
        }
        self.spawn(task);
    }
    pub(crate) fn spawn(&self, task: Task) {
        // The runtime owns the task; dropping the handle does not cancel it.
        let _handle = self.runtime().spawn_local(task);
    }
}

pub(crate) fn settle(cell: &Rc<RefCell<PromiseCell>>, result: Result<Value, Thrown>) {
    let wakers = {
        let mut c = cell.borrow_mut();
        if !matches!(c.state, PromiseState::Pending) {
            return;
        }
        c.state = match result {
            Ok(v) => PromiseState::Fulfilled(v),
            Err(e) => PromiseState::Rejected(e),
        };
        std::mem::take(&mut c.wakers)
    };
    for w in wakers {
        w.wake();
    }
}

impl Realm {
    /// The ExchangeError view of a thrown value: an Error's message (and its
    /// `name` when not plain Error), otherwise String(value).
    pub(crate) fn exchange_error(
        &mut self,
        thrown: &Value,
    ) -> safe_cex_exchange::runtime::ExchangeError {
        let message = if thrown
            .object()
            .is_some_and(|o| matches!(o.borrow().kind, Kind::Error))
        {
            let m = self.get_str(thrown, "message").unwrap_or(Value::Undefined);
            self.to_rust_string(&m).unwrap_or_default()
        } else {
            self.to_rust_string(thrown)
                .unwrap_or_else(|_| "[object]".into())
        };
        let mut error = safe_cex_exchange::runtime::ExchangeError::new(message);
        if thrown.object().is_some() {
            if let Ok(code) = self.get_str(thrown, "code")
                && !code.is_nullish()
            {
                error.code = match self.capture_typed_value(&code, 0) {
                    Ok(code) => Some(code),
                    Err(cause) => return self.exchange_error(&cause),
                };
            }
            if let Ok(response) = self.get_str(thrown, "response")
                && !response.is_nullish()
            {
                let response = match self.capture_typed_value(&response, 0) {
                    Ok(value) => value,
                    Err(cause) => return self.exchange_error(&cause),
                };
                match safe_cex_exchange::js_runtime::value::to_retained_input(&response) {
                    Ok(response) => error = error.with_retained_response(response),
                    Err(cause) => return cause.into(),
                }
            }
        }
        error
    }
    /// Unawaited failures are reported to the runtime, never swallowed.
    pub(crate) fn report_thrown(&mut self, thrown: &Value, context: &str) {
        let error = self.exchange_error(thrown);
        self.runtime()
            .report_exception(&error, &serde_json::json!({ "context": context }).into());
    }
    pub(crate) fn regexp(&self, literal: &str) -> Result<Value, Thrown> {
        let pattern = super::regex::Pattern::literal(literal)
            .map_err(|e| self.error_of("SyntaxError", &e))?;
        Ok(self.alloc(
            Kind::RegExp(Rc::new(pattern)),
            Some(&self.intrinsics().regexp_proto),
        ))
    }
}
