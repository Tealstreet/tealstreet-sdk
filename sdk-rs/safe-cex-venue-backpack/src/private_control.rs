//! Private socket control bodies; authentication and transport remain supplied operations.
use crate::public_control::{Message, Streams, SubscriptionState};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

// ts: BackpackPrivateWebSocket.ping
pub fn ping<E>(
    is_disposed: bool,
    is_connected: impl FnOnce() -> Result<bool, E>,
    ping_at: &Cell<f64>,
    now: impl FnOnce() -> Result<f64, E>,
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed && is_connected()? {
        ping_at.set(now()?);
        send(Message {
            method: "PING",
            params: None,
        })?;
    }
    Ok(())
}
// ts: BackpackPrivateWebSocket.unsubscribe
pub fn unsubscribe<E>(
    state: &SubscriptionState,
    streams: &Streams,
    send: impl FnOnce(Message) -> Result<(), E>,
) -> Result<(), E> {
    let params = streams.borrow().clone();
    if !params.is_empty() {
        send(Message {
            method: "UNSUBSCRIBE",
            params: Some(params),
        })?;
        let length = streams.borrow().len();
        for index in 0..length {
            let value = streams.borrow().get(index).cloned().flatten();
            if let Some(value) = value {
                state.borrow_mut().remove(&value);
            }
        }
    }
    Ok(())
}
// ts: BackpackPrivateWebSocket#resubscribe
pub fn resubscribe<E, F: Future<Output = Result<(), E>>, A: FnOnce() -> Result<F, E>>(
    state: &SubscriptionState,
    authenticate: A,
) -> impl Future<Output = Result<(), E>> + use<E, F, A> {
    let authentication = if state.borrow().values().is_empty() {
        None
    } else {
        Some(authenticate())
    };
    async move {
        if let Some(authentication) = authentication {
            authentication?.await?;
        }
        Ok(())
    }
}
// ts: BackpackPrivateWebSocket#throttledFetchOrders
pub fn throttled_fetch_orders<T, F, E>(
    timeout: &RefCell<Option<Rc<T>>>,
    is_truthy: impl FnOnce(Option<&Rc<T>>) -> Result<bool, E>,
    callback: &Rc<F>,
    set_timeout: impl FnOnce(&Rc<F>, f64) -> Result<Rc<T>, E>,
) -> Result<(), E> {
    let current = timeout.borrow().clone();
    if is_truthy(current.as_ref())? {
        return Ok(());
    }
    let handle = set_timeout(callback, 10000.0)?;
    *timeout.borrow_mut() = Some(handle);
    Ok(())
}
