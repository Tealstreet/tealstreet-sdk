//! Typed async call boundary over the canonical JavaScript Promise driver.
use crate::{
    js_runtime::{
        promise::Promise,
        value::{Js, Thrown},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use std::{cell::RefCell, future::Future, rc::Rc};

/// Run the synchronous prefix now and let the runtime own its continuation.
///
/// The returned future is only a waiter. Dropping it does not cancel the body.
/// `T` stays native and moves exactly once; no Clone or serialization is needed.
pub fn start_eager<T: 'static>(
    runtime: &Rc<dyn ExchangeRuntime>,
    body: impl Future<Output = Result<T, ExchangeError>> + 'static,
) -> ExchangeFuture<'static, T> {
    let result = Rc::new(RefCell::new(None));
    let output = result.clone();
    let promise = Promise::spawn(
        runtime,
        Box::pin(async move {
            let value = body.await.map_err(Thrown::from)?;
            *output.borrow_mut() = Some(value);
            Ok(Js::Undefined)
        }),
    );
    Box::pin(async move {
        promise
            .wait()
            .await
            .map_err(|error| error.to_exchange_error())?;
        let value = result
            .borrow_mut()
            .take()
            .expect("typed eager result is stored before promise fulfillment");
        Ok(value)
    })
}
