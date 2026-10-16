//! Coalesced HIP-3 order retry chains and the owned asynchronous timer callback.
use crate::{
    helper_tasks::{HelperObserver, HelperTaskHost},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
};
#[derive(Clone)]
pub enum OrderRetryCallback<V> {
    Id,
    Timer {
        exchange: V,
        attempt: V,
        max_attempts: V,
    },
}
/// Native callback construction retains the same lexical receiver and runtime.
/// Iterator records implement native next/close operations with original-throw
/// precedence; operators preserve JS coercion, including object-valued attempts.
pub trait OrderRetryHost: SpotAccountHost {
    fn order_retry_callback(
        &mut self,
        c: OrderRetryCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn iterator(&mut self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn next(&mut self, v: &Self::Value) -> Result<Option<Self::Value>, Self::Error>;
    fn close_after_throw(&mut self, v: &Self::Value);
    fn retry_operator(
        &mut self,
        op: &str,
        a: &Self::Value,
        b: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
}
fn method<H: SpotAccountHost>(
    h: &mut H,
    o: &H::Value,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, key)?;
    let args = args(h)?;
    h.call(&f, o, &args)
}
fn call_step<H: OrderRetryHost>(
    h: &mut H,
    e: &H::Value,
    attempt: &H::Value,
) -> Result<(), H::Error> {
    method(h, e, "runRetryChainStep", |h| {
        let next = h.retry_operator("+", attempt, &h.number(1.))?;
        Ok(vec![next])
    })?;
    Ok(())
}
fn end<H: SpotAccountHost>(h: &mut H, e: &H::Value) -> Result<(), H::Error> {
    method(h, e, "endRetryChain", |_| Ok(vec![]))?;
    Ok(())
}
fn size<H: SpotAccountHost>(h: &mut H, e: &H::Value) -> Result<H::Value, H::Error> {
    let set = h.get(e, "_pendingRetryOrderIds")?;
    h.get(&set, "size")
}
fn warning<H: SpotAccountHost>(
    h: &mut H,
    e: &H::Value,
    text: impl FnOnce(&mut H) -> Result<JsString, H::Error>,
) -> Result<(), H::Error> {
    method(h, e, "log", |h| {
        let text = text(h)?;
        let text = h.literal(&text);
        let table = h.imported("LogSeverity")?;
        let warning = h.get(&table, "Warning")?;
        Ok(vec![text, warning])
    })?;
    Ok(())
}
pub fn refresh_hip3_orders_with_retry<H: OrderRetryHost>(
    h: &mut H,
    e: &H::Value,
    ids: &H::Value,
) -> Result<H::Value, H::Error> {
    let iterator = h.iterator(ids)?;
    while let Some(id) = h.next(&iterator)? {
        let result = (|| {
            let pending = h.get(e, "_pendingRetryOrderIds")?;
            method(h, &pending, "add", |_| Ok(vec![id]))?;
            Ok(())
        })();
        if let Err(error) = result {
            h.close_after_throw(&iterator);
            return Err(error);
        }
    }
    let active = h.get(e, "_retryChainActive")?;
    if h.truthy(&active) {
        return Ok(h.undefined());
    }
    h.set(e, "_retryChainActive", &h.boolean(true))?;
    method(h, e, "runRetryChainStep", |h| Ok(vec![h.number(1.)]))?;
    Ok(h.undefined())
}
pub fn run_retry_chain_step<H: OrderRetryHost>(
    h: &mut H,
    e: &H::Value,
    attempt: &H::Value,
) -> Result<H::Value, H::Error> {
    let max_attempts = h.number(5.);
    let values = [300., 700., 1500., 3000., 5000.].map(|v| h.number(v));
    let delays = h.array(&values)?;
    let index = h.retry_operator("-", attempt, &h.number(1.))?;
    let delay = h.index(&delays, &index)?;
    let delay = if h.truthy(&delay) {
        delay
    } else {
        let len = h.get(&delays, "length")?;
        let index = h.retry_operator("-", &len, &h.number(1.))?;
        h.index(&delays, &index)?
    };
    let set = h.global("setTimeout")?;
    let callback = h.order_retry_callback(OrderRetryCallback::Timer {
        exchange: e.clone(),
        attempt: attempt.clone(),
        max_attempts,
    })?;
    let timer = h.call(&set, &h.undefined(), &[callback, delay])?;
    h.set(e, "_retryChainTimer", &timer)?;
    Ok(h.undefined())
}
pub fn run_order_retry_callback<H: OrderRetryHost>(
    h: &mut H,
    c: &OrderRetryCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    match c {
        OrderRetryCallback::Id => {
            let first = args.first().cloned().unwrap_or_else(|| h.undefined());
            h.get(&first, "id")
        }
        OrderRetryCallback::Timer { .. } => {
            unreachable!("asynchronous timer requires retained task")
        }
    }
}
async fn timer_body<H: OrderRetryHost>(
    h: &mut H,
    e: &H::Value,
    attempt: &H::Value,
    max_attempts: &H::Value,
) -> Result<H::Value, H::Error> {
    h.set(e, "_retryChainTimer", &h.null())?;
    let disposed = h.get(e, "isDisposed")?;
    if h.truthy(&disposed) {
        end(h, e)?;
        return Ok(h.undefined());
    }
    let result: Result<(), H::Error> = async {
        let request = method(h, e, "fetchOrders", |_| Ok(vec![]))?;
        let orders = h.await_value(request).await?;
        let constructor = h.global("Set")?;
        let ids = method(h, &orders, "map", |h| {
            Ok(vec![h.order_retry_callback(OrderRetryCallback::Id)?])
        })?;
        let found = h.construct(&constructor, &[ids])?;
        let iterator = h.iterator(&found)?;
        while let Some(id) = h.next(&iterator)? {
            let result = (|| {
                let pending = h.get(e, "_pendingRetryOrderIds")?;
                method(h, &pending, "delete", |_| Ok(vec![id]))?;
                Ok(())
            })();
            if let Err(error) = result {
                h.close_after_throw(&iterator);
                return Err(error);
            }
        }
        let pending_size = size(h, e)?;
        let mut done = h.strict_equal(&pending_size, &h.number(0.));
        if !done {
            let bound = h.retry_operator(">=", attempt, max_attempts)?;
            done = h.truthy(&bound)
        }
        if done {
            let pending_size = size(h, e)?;
            let positive = h.retry_operator(">", &pending_size, &h.number(0.))?;
            if h.truthy(&positive) {
                let bound = h.retry_operator(">=", attempt, max_attempts)?;
                if h.truthy(&bound) {
                    warning(h, e, |h| {
                        let mut text = "HIP-3 order refresh: ".encode_utf16().collect::<Vec<_>>();
                        let size = size(h, e)?;
                        text.extend(h.template_string(&size)?.0);
                        text.extend(" expected orders not found after ".encode_utf16());
                        text.extend(h.template_string(max_attempts)?.0);
                        text.extend(" attempts".encode_utf16());
                        Ok(JsString(text))
                    })?;
                }
            }
            end(h, e)?;
            return Ok(());
        }
        call_step(h, e, attempt)?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        let value = h.error_value(&error);
        warning(h, e, |h| {
            let mut text = "HIP-3 order refresh failed (attempt "
                .encode_utf16()
                .collect::<Vec<_>>();
            text.extend(h.template_string(attempt)?.0);
            text.extend("): ".encode_utf16());
            text.extend(h.template_string(&value)?.0);
            Ok(JsString(text))
        })?;
        let below = h.retry_operator("<", attempt, max_attempts)?;
        if h.truthy(&below) {
            call_step(h, e, attempt)?
        } else {
            end(h, e)?
        }
    }
    Ok(h.undefined())
}
pub fn start_order_retry_callback<H: OrderRetryHost + HelperTaskHost<H::Value, H::Error>>(
    host: &H,
    c: OrderRetryCallback<H::Value>,
) -> HelperObserver<H::Value, H::Error>
where
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let OrderRetryCallback::Timer {
        exchange,
        attempt,
        max_attempts,
    } = c
    else {
        unreachable!("sync callback requires direct invocation")
    };
    let mut h = host.clone();
    crate::helper_tasks::start(host, async move {
        timer_body(&mut h, &exchange, &attempt, &max_attempts).await
    })
}
