//! Independent Spot token limiter and its owned callbacks.
use crate::{
    helper_tasks::{self, HelperObserver},
    spot_account_reads::SpotAccountHost,
    ws_ping_lifecycle::WsPingHost,
};
#[derive(Clone, Debug)]
pub enum ClientCallback<V> {
    RateRefill,
    RateWait,
    RateExecutor { delay: V },
    RateWake { owner: V, weight: V },
}
pub trait SpotClientHost: SpotAccountHost {
    fn client_callback(
        &mut self,
        body: ClientCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    /// Allocate a native class/prototype pair and retain intrinsic metadata/links.
    fn class_pair(
        &mut self,
        name: &str,
        length: u8,
        base: Option<&str>,
    ) -> Result<(Self::Value, Self::Value), Self::Error>;
    fn define_hidden(
        &mut self,
        object: &Self::Value,
        key: &str,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn native_method(
        &mut self,
        name: &str,
        length: u8,
        body: ClientCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    /// Ordinary native construction with newTarget prototype resolution.
    fn allocate_class(
        &mut self,
        prototype: &Self::Value,
        new_target: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
}
fn method<H: WsPingHost>(
    h: &mut H,
    o: &H::Value,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, key)?;
    let a = args(h)?;
    h.call(&f, o, &a)
}
pub struct RateLimiter<V> {
    pub constructor: V,
    pub prototype: V,
}
pub fn rate_limiter_class<H: SpotClientHost>(h: &mut H) -> Result<RateLimiter<H::Value>, H::Error> {
    let (constructor, prototype) = h.class_pair("RateLimiter", 0, None)?;
    for (name, body) in [
        ("refillTokens", ClientCallback::RateRefill),
        ("waitForToken", ClientCallback::RateWait),
    ] {
        let f = h.native_method(name, 0, body)?;
        h.define_hidden(&prototype, name, &f)?
    }
    Ok(RateLimiter {
        constructor,
        prototype,
    })
}
pub fn new_rate_limiter<H: SpotClientHost>(
    h: &mut H,
    class: &RateLimiter<H::Value>,
    target: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let instance = h.allocate_class(&class.prototype, target.unwrap_or(&class.constructor))?;
    for k in ["tokens", "lastRefill", "capacity"] {
        h.define_data(&instance, k, &h.undefined())?
    }
    h.set(&instance, "capacity", &h.number(1200.))?;
    let capacity = h.get(&instance, "capacity")?;
    h.set(&instance, "tokens", &capacity)?;
    let date = h.global("Date")?;
    let now = method(h, &date, "now", |_| Ok(vec![]))?;
    h.set(&instance, "lastRefill", &now)?;
    Ok(instance)
}
pub fn refill_tokens<H: SpotClientHost>(h: &mut H, owner: &H::Value) -> Result<(), H::Error> {
    let date = h.global("Date")?;
    let now = method(h, &date, "now", |_| Ok(vec![]))?;
    let last = h.get(owner, "lastRefill")?;
    let elapsed = h.binary("-", &now, &last)?;
    let elapsed = h.binary("/", &elapsed, &h.number(60000.))?;
    let ready = h.binary(">=", &elapsed, &h.number(1.))?;
    if h.truthy(&ready) {
        let capacity = h.get(owner, "capacity")?;
        h.set(owner, "tokens", &capacity)?;
        h.set(owner, "lastRefill", &now)?
    }
    Ok(())
}
async fn wait_body<H: SpotClientHost>(
    mut h: H,
    owner: H::Value,
    weight: H::Value,
) -> Result<H::Value, H::Error> {
    method(&mut h, &owner, "refillTokens", |_| Ok(vec![]))?;
    let tokens = h.get(&owner, "tokens")?;
    let enough = h.binary(">=", &tokens, &weight)?;
    if h.truthy(&enough) {
        let tokens = h.get(&owner, "tokens")?;
        let remaining = h.binary("-", &tokens, &weight)?;
        h.set(&owner, "tokens", &remaining)?;
        return Ok(h.undefined());
    }
    let date = h.global("Date")?;
    let now = method(&mut h, &date, "now", |_| Ok(vec![]))?;
    let last = h.get(&owner, "lastRefill")?;
    let elapsed = h.binary("-", &now, &last)?;
    let seconds = h.binary("/", &elapsed, &h.number(1000.))?;
    let seconds = h.binary("-", &h.number(60.), &seconds)?;
    let delay = h.binary("*", &seconds, &h.number(1000.))?;
    let ctor = h.global("Promise")?;
    let executor = h.client_callback(ClientCallback::RateExecutor { delay })?;
    let promise = h.construct(&ctor, &[executor])?;
    method(&mut h, &promise, "then", |h| {
        Ok(vec![h.client_callback(ClientCallback::RateWake {
            owner,
            weight,
        })?])
    })
}
pub fn wait_for_token<H>(
    host: H,
    owner: H::Value,
    weight: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotClientHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    let weight = if host.is_undefined(&weight) {
        host.number(1.)
    } else {
        weight
    };
    helper_tasks::start(&runtime, wait_body(host, owner, weight))
}
pub fn run_client_callback<H>(
    h: &mut H,
    receiver: &H::Value,
    body: &ClientCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error>
where
    H: SpotClientHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match body {
        ClientCallback::RateRefill => {
            refill_tokens(h, receiver)?;
            Ok(h.undefined())
        }
        ClientCallback::RateWait => {
            let observer = wait_for_token(h.clone(), receiver.clone(), first);
            Ok(h.spawn_native(Box::pin(observer)))
        }
        ClientCallback::RateExecutor { delay } => {
            let f = h.global("setTimeout")?;
            h.call(&f, &h.undefined(), &[first, delay.clone()])
        }
        ClientCallback::RateWake { owner, weight } => {
            method(h, owner, "refillTokens", |_| Ok(vec![]))?;
            method(h, owner, "waitForToken", |_| Ok(vec![weight.clone()]))
        }
    }
}
