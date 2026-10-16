//! Bodies from hyperliquidspot.rateLimiter.ts, including native promise callbacks.
//! Allocation/prototype assembly of the source class remains a separate boundary.
use crate::{
    helper_tasks::{self, HelperObserver, HelperTaskHost},
    ws_ping_lifecycle::WsPingHost,
};

#[derive(Clone, Debug)]
pub enum SpotLimiterCallback<V> {
    Executor { wait_time: V },
    Continue { limiter: V, weight: V },
}

/// Native primitive operations; promise execution/adoption is supplied by the
/// same realm that owns the receiver, timers, captured values and callbacks.
pub trait SpotRateLimiterHost: WsPingHost {
    fn number(&self, value: f64) -> Self::Value;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn binary(
        &mut self,
        operator: &str,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Native construction, including synchronous Promise executor invocation
    /// and conversion of an executor throw into its original rejection value.
    fn construct(
        &mut self,
        constructor: &Self::Value,
        arguments: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    /// Allocate the source anonymous arrow (executor length1, continuation0).
    fn limiter_callback(
        &mut self,
        capture: SpotLimiterCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}

fn method<H: WsPingHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    h.call(&function, receiver, args)
}
fn now<H: WsPingHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let date = h.global("Date")?;
    method(h, &date, "now", &[])
}

pub fn spot_limiter_constructor<H: SpotRateLimiterHost>(
    h: &mut H,
    limiter: &H::Value,
) -> Result<(), H::Error> {
    // Native ES class fields run in declaration order before constructor body.
    for name in ["tokens", "lastRefill", "capacity"] {
        h.define_data(limiter, name, &h.undefined())?;
    }
    h.set(limiter, "capacity", &h.number(1200.0))?;
    let capacity = h.get(limiter, "capacity")?;
    h.set(limiter, "tokens", &capacity)?;
    let now = now(h)?;
    h.set(limiter, "lastRefill", &now)
}

pub fn spot_refill_tokens<H: SpotRateLimiterHost>(
    h: &mut H,
    limiter: &H::Value,
) -> Result<(), H::Error> {
    let now = now(h)?;
    let last_refill = h.get(limiter, "lastRefill")?;
    let elapsed = h.binary("-", &now, &last_refill)?;
    let elapsed_minutes = h.binary("/", &elapsed, &h.number(1000.0 * 60.0))?;
    let refill = h.binary(">=", &elapsed_minutes, &h.number(1.0))?;
    if h.truthy(&refill) {
        let capacity = h.get(limiter, "capacity")?;
        h.set(limiter, "tokens", &capacity)?;
        h.set(limiter, "lastRefill", &now)?;
    }
    Ok(())
}

pub fn run_spot_limiter_callback<H: SpotRateLimiterHost>(
    h: &mut H,
    capture: &SpotLimiterCallback<H::Value>,
    arguments: &[H::Value],
) -> Result<H::Value, H::Error> {
    match capture {
        SpotLimiterCallback::Executor { wait_time } => {
            let timer = h.global("setTimeout")?;
            let resolve = arguments.first().cloned().unwrap_or_else(|| h.undefined());
            h.call(&timer, &h.undefined(), &[resolve, wait_time.clone()])
        }
        SpotLimiterCallback::Continue { limiter, weight } => {
            method(h, limiter, "refillTokens", &[])?;
            method(h, limiter, "waitForToken", &[weight.clone()])
        }
    }
}

pub fn spot_wait_for_token<H>(
    mut h: H,
    limiter: H::Value,
    weight: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotRateLimiterHost + HelperTaskHost<H::Value, H::Error>,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = h.clone();
    helper_tasks::start(&runtime, async move {
        let weight = if h.is_undefined(&weight) {
            h.number(1.0)
        } else {
            weight
        };
        method(&mut h, &limiter, "refillTokens", &[])?;
        let tokens = h.get(&limiter, "tokens")?;
        let sufficient = h.binary(">=", &tokens, &weight)?;
        if h.truthy(&sufficient) {
            let tokens = h.get(&limiter, "tokens")?;
            let remaining = h.binary("-", &tokens, &weight)?;
            h.set(&limiter, "tokens", &remaining)?;
            return Ok(h.undefined());
        }

        let now = now(&mut h)?;
        let last_refill = h.get(&limiter, "lastRefill")?;
        let elapsed = h.binary("-", &now, &last_refill)?;
        let elapsed_seconds = h.binary("/", &elapsed, &h.number(1000.0))?;
        let seconds_left = h.binary("-", &h.number(60.0), &elapsed_seconds)?;
        let wait_time = h.binary("*", &seconds_left, &h.number(1000.0))?;
        let promise_constructor = h.global("Promise")?;
        let executor = h.limiter_callback(SpotLimiterCallback::Executor { wait_time })?;
        let promise = h.construct(&promise_constructor, &[executor])?;
        let then = h.get(&promise, "then")?;
        let continuation = h.limiter_callback(SpotLimiterCallback::Continue { limiter, weight })?;
        h.call(&then, &promise, &[continuation])
    })
}
