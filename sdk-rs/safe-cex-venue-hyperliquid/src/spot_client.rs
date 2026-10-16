//! Spot API callbacks and the independent spot token limiter.
use crate::{
    helper_tasks::{self, HelperObserver},
    spot_account_reads::SpotAccountHost,
    spot_json::JsString,
    ws_ping_lifecycle::WsPingHost,
};
#[derive(Clone, Debug)]
pub enum ClientCallback<V> {
    Serialize,
    Mark,
    Throttle,
    Response { exchange: V },
    ErrorPredicate,
    ErrorField,
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
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&text(s))
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
fn free<H: SpotAccountHost>(
    h: &mut H,
    name: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.imported(name)?;
    let a = args(h)?;
    h.call(&f, &h.undefined(), &a)
}
fn optional<H: SpotAccountHost>(h: &mut H, v: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(v) {
        Ok(h.undefined())
    } else {
        h.get(v, key)
    }
}
fn path<H: SpotAccountHost>(h: &mut H, v: &H::Value, keys: &[&str]) -> Result<H::Value, H::Error> {
    let mut v = v.clone();
    for k in keys {
        v = optional(h, &v, k)?
    }
    Ok(v)
}
fn field<H: WsPingHost>(h: &mut H, o: &H::Value, k: &str, v: &H::Value) -> Result<(), H::Error> {
    h.define_data(o, k, v)
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
pub fn create_api<H: SpotClientHost>(
    h: &mut H,
    options: &H::Value,
    exchange: &H::Value,
) -> Result<H::Value, H::Error> {
    let urls = h.imported("spot.BASE_URLS")?;
    let testnet = h.get(options, "testnet")?;
    let url = h.get(
        &urls,
        if h.truthy(&testnet) {
            "TESTNET"
        } else {
            "PRODUCTION"
        },
    )?;
    let xhr = free(h, "safeCexAxiosCreate", |h| {
        let config = h.object()?;
        field(h, &config, "baseURL", &url)?;
        let timeout = path(h, options, &["extra", "recvWindow"])?;
        field(h, &config, "timeout", &timeout)?;
        let serializer = h.object()?;
        let cb = h.client_callback(ClientCallback::Serialize)?;
        field(h, &serializer, "serialize", &cb)?;
        field(h, &config, "paramsSerializer", &serializer)?;
        let headers = h.object()?;
        field(
            h,
            &headers,
            "Content-Type",
            &lit(h, "application/json, charset=utf-8"),
        )?;
        field(h, &config, "headers", &headers)?;
        Ok(vec![config, options.clone(), exchange.clone()])
    })?;
    free(h, "retry", |h| {
        let config = h.object()?;
        field(h, &config, "retries", &h.number(3.))?;
        let condition = free(h, "networkErrorOnRead", |h| {
            Ok(vec![h.imported("isHyperliquidRead")?])
        })?;
        field(h, &config, "retryCondition", &condition)?;
        Ok(vec![xhr.clone(), config])
    })?;
    let public = h.get(options, "publicDataOnly")?;
    if !h.truthy(&public) {
        free(h, "installTransportAdapter", |h| {
            Ok(vec![xhr.clone(), options.clone(), h.get(exchange, "name")?])
        })?;
    }
    for body in [ClientCallback::Mark, ClientCallback::Throttle] {
        let interceptors = h.get(&xhr, "interceptors")?;
        let request = h.get(&interceptors, "request")?;
        method(h, &request, "use", |h| Ok(vec![h.client_callback(body)?]))?;
    }
    let interceptors = h.get(&xhr, "interceptors")?;
    let response = h.get(&interceptors, "response")?;
    method(h, &response, "use", |h| {
        Ok(vec![h.client_callback(ClientCallback::Response {
            exchange: exchange.clone(),
        })?])
    })?;
    Ok(xhr)
}
fn error_branch<H: SpotClientHost>(
    h: &mut H,
    exchange: &H::Value,
    msg: H::Value,
) -> Result<H::Value, H::Error> {
    method(h, exchange, "emitAndLogError", |_| Ok(vec![msg.clone()]))?;
    let ctor = h.global("Error")?;
    let error = h.construct(&ctor, &[msg])?;
    Err(h.throw_value(error))
}
fn chain<H: SpotAccountHost>(
    h: &mut H,
    mut v: H::Value,
    keys: &[&str],
) -> Result<Option<H::Value>, H::Error> {
    for key in keys {
        if h.is_nullish(&v) {
            return Ok(None);
        }
        v = h.get(&v, key)?;
    }
    Ok(Some(v))
}
async fn response_body<H: SpotClientHost>(
    mut h: H,
    response: H::Value,
    exchange: H::Value,
) -> Result<H::Value, H::Error> {
    for nested in [false, true] {
        let data = h.get(&response, "data")?;
        let keys: &[&str] = if nested {
            &["response", "data", "status"]
        } else {
            &["status"]
        };
        let status = path(&mut h, &data, keys)?;
        if h.strict_equal(&status, &lit(&h, "err")) {
            let data = h.get(&response, "data")?;
            let keys: &[&str] = if nested {
                &["response", "data", "response"]
            } else {
                &["response"]
            };
            let msg = path(&mut h, &data, keys)?;
            let msg = h.template_string(&msg)?;
            let msg = h.literal(&msg);
            return error_branch(&mut h, &exchange, msg);
        }
    }
    for nested in [false, true] {
        let data = h.get(&response, "data")?;
        let keys: &[&str] = if nested {
            &["response", "data", "statuses"]
        } else {
            &["statuses"]
        };
        let present = path(&mut h, &data, keys)?;
        if !h.truthy(&present) {
            continue;
        }
        let data = h.get(&response, "data")?;
        let Some(statuses) = chain(&mut h, data, keys)? else {
            continue;
        };
        let some = method(&mut h, &statuses, "some", |h| {
            Ok(vec![h.client_callback(ClientCallback::ErrorPredicate)?])
        })?;
        if h.truthy(&some) {
            let data = h.get(&response, "data")?;
            let Some(statuses) = chain(&mut h, data, keys)? else {
                let undefined = h.undefined();
                return error_branch(&mut h, &exchange, undefined);
            };
            let filtered = method(&mut h, &statuses, "filter", |h| {
                Ok(vec![h.client_callback(ClientCallback::ErrorPredicate)?])
            })?;
            let errors = method(&mut h, &filtered, "map", |h| {
                Ok(vec![h.client_callback(ClientCallback::ErrorField)?])
            })?;
            let errors = method(&mut h, &errors, "join", |h| Ok(vec![lit(h, ", ")]))?;
            return error_branch(&mut h, &exchange, errors);
        }
    }
    Ok(response)
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
        ClientCallback::Serialize => {
            let qs = h.imported("qs")?;
            method(h, &qs, "stringify", |h| {
                let options = h.object()?;
                field(h, &options, "arrayFormat", &lit(h, "repeat"))?;
                Ok(vec![first, options])
            })
        }
        ClientCallback::Mark => {
            let next = h.object()?;
            h.spread_into(&next, &first)?;
            let url = h.get(&next, "url")?;
            let endpoints = h.imported("spot.ENDPOINTS")?;
            let info = h.get(&endpoints, "INFO")?;
            if h.strict_equal(&url, &info) {
                free(h, "markSkipRemoteSigning", |_| Ok(vec![next.clone()]))?;
            }
            Ok(next)
        }
        ClientCallback::Throttle => {
            let mut runtime = h.clone();
            Ok(h.spawn_native(Box::pin(async move {
                let capture = free(&mut runtime, "isSigningCaptureRequest", |_| {
                    Ok(vec![first.clone()])
                })?;
                if runtime.truthy(&capture) {
                    return Ok(first);
                }
                let limiter = runtime.imported("hyperliquidRestRateLimiter")?;
                let result = method(&mut runtime, &limiter, "waitForToken", |h| {
                    Ok(vec![free(h, "getHyperliquidRestRequestWeight", |_| {
                        Ok(vec![first.clone()])
                    })?])
                })?;
                runtime.await_value(result).await?;
                Ok(first)
            })))
        }
        ClientCallback::Response { exchange } => {
            Ok(h.spawn_native(Box::pin(response_body(h.clone(), first, exchange.clone()))))
        }
        ClientCallback::ErrorPredicate => {
            let function = h.global("Boolean")?;
            let error = h.get(&first, "error")?;
            h.call(&function, &h.undefined(), &[error])
        }
        ClientCallback::ErrorField => h.get(&first, "error"),
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
