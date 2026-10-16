//! ts: createPolymarketAPI. HTTP creation, qs, retry and Promise primitives
//! are bound dependencies; interceptor control flow and per-client state are native.
use crate::{
    account_reads::{returned, start, wait},
    exchange::ExchangeHost,
};
use std::{cell::Cell, future::Future, rc::Rc};
/// Intrinsics used by the source factory and its three installed callbacks.
pub trait FactoryHost: ExchangeHost {
    fn table(&self, name: &'static str) -> Result<Self::Value, Self::Error>;
    fn function(
        &self,
        body: Rc<dyn Fn(Vec<Self::Value>) -> Result<Self::Value, Self::Error>>,
    ) -> Self::Value;
    fn apply(
        &self,
        callable: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn is_string_value(&self, value: &Self::Value) -> bool;
    fn starts_with(&self, text: &Self::Value, prefix: Self::Value) -> Result<bool, Self::Error>;
    fn qs_comma(&self, params: Self::Value) -> Result<Self::Value, Self::Error>;
    fn json_stringify(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn sleep(&self, millis: f64) -> Result<Rc<Self::Operation>, Self::Error>;
    fn promise_reject(&self, error: Self::Value) -> Result<Self::Value, Self::Error>;
    fn inherit(&self, prototype: Self::Value) -> Result<Self::Value, Self::Error>;
}
fn property<H: FactoryHost>(h: &H, table: &'static str, key: &str) -> Result<H::Value, H::Error> {
    h.field(&h.table(table)?, key)
}
fn optional<H: FactoryHost>(h: &H, value: H::Value, key: &str) -> Result<H::Value, H::Error> {
    if h.nullish(&value) {
        Ok(h.undefined())
    } else {
        h.field(&value, key)
    }
}
fn get<H: FactoryHost>(h: &H, args: &[H::Value], index: usize) -> H::Value {
    args.get(index).cloned().unwrap_or_else(|| h.undefined())
}
fn text<H: FactoryHost>(h: &H, prefix: &str, value: &H::Value) -> Result<H::Value, H::Error> {
    let mut units = prefix.encode_utf16().collect::<Vec<_>>();
    units.extend(h.string(value)?);
    Ok(h.string_value(units))
}
fn capture<H: FactoryHost>(h: &H, xhr: &H::Value, kind: &str) -> Result<H::Callable, H::Error> {
    let interceptors = h.field(xhr, "interceptors")?;
    let group = h.field(&interceptors, kind)?;
    h.capture_value(group, "use")
}
/// The returned client is the exact factory result, not a reconstructed client.
pub fn create_polymarket_api<H: FactoryHost>(
    h: Rc<H>,
    options: H::Value,
    exchange: H::Value,
    credentials: H::Value,
    wallet_address: H::Value,
) -> Result<H::Value, H::Error> {
    let base_url = property(h.as_ref(), "POLYMARKET_CONFIG", "REST_BASE")?;
    let create = h.capture("imports", "safeCexAxiosCreate")?;
    let timeout = optional(
        h.as_ref(),
        optional(h.as_ref(), options.clone(), "extra")?,
        "recvWindow",
    )?;
    let timeout = if h.nullish(&timeout) {
        h.number(30000.)
    } else {
        timeout
    };
    let serialize = {
        let host = h.clone();
        h.function(Rc::new(move |args| {
            host.qs_comma(get(host.as_ref(), &args, 0))
        }))
    };
    let params = h.object(vec![("serialize", serialize)])?;
    let headers = h.object(vec![
        ("Content-Type", h.text("application/json")),
        ("Accept", h.text("application/json")),
    ])?;
    let config = h.object(vec![
        ("baseURL", base_url),
        ("timeout", timeout),
        ("paramsSerializer", params),
        ("headers", headers),
    ])?;
    let xhr = h.call_sync(create, vec![config, options, exchange.clone()])?;
    let retry = h.capture("imports", "retry")?;
    let condition = h.capture("imports", "networkErrorOnRead")?;
    let posted = h.capture("imports", "readsAndPosted")?;
    let reads = h.call_sync(
        posted,
        vec![
            property(h.as_ref(), "ENDPOINTS", "ORDER_BOOKS")?,
            property(h.as_ref(), "ENDPOINTS", "MIDPOINTS")?,
        ],
    )?;
    let condition = h.call_sync(condition, vec![reads])?;
    let delay = {
        let host = h.clone();
        h.retry_delay(Rc::new(move |count| {
            host.multiply(count, host.number(1000.))
        }))?
    };
    let retry_opts = h.object(vec![
        ("retries", h.number(3.)),
        ("retryCondition", condition),
        ("retryDelay", delay),
    ])?;
    h.call_sync(retry, vec![xhr.clone(), retry_opts])?;
    let last_request = Rc::new(Cell::new(0.));
    let request_use = capture(h.as_ref(), &xhr, "request")?;
    let request = {
        let host = h.clone();
        let exchange = exchange.clone();
        h.async_method(Rc::new(move |args| {
            Box::pin(request_interceptor(
                host.clone(),
                get(host.as_ref(), &args, 0),
                exchange.clone(),
                credentials.clone(),
                wallet_address.clone(),
                last_request.clone(),
            ))
        }))
    };
    h.call_sync(request_use, vec![request])?;
    let response_use = capture(h.as_ref(), &xhr, "response")?;
    let success = {
        let host = h.clone();
        h.function(Rc::new(move |args| {
            response_success(host.as_ref(), get(host.as_ref(), &args, 0))
        }))
    };
    let failure = {
        let host = h.clone();
        h.function(Rc::new(move |args| {
            response_failure(
                host.as_ref(),
                get(host.as_ref(), &args, 0),
                exchange.clone(),
            )
        }))
    };
    h.call_sync(response_use, vec![success, failure])?;
    Ok(xhr)
}
fn request_interceptor<H: FactoryHost>(
    host: Rc<H>,
    config: H::Value,
    exchange: H::Value,
    credentials: H::Value,
    wallet_address: H::Value,
    last_request: Rc<Cell<f64>>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let h = host.clone();
        async move {
            let url = h.field(&config, "url")?;
            let url = if h.truthy(&url) { url } else { h.text("") };
            let mut public = vec![];
            for key in [
                "MARKETS",
                "SIMPLIFIED_MARKETS",
                "ORDER_BOOK",
                "ORDER_BOOKS",
                "PRICE",
                "PRICES",
                "MIDPOINT",
                "MIDPOINTS",
                "SPREAD",
                "SPREADS",
                "LAST_TRADE_PRICE",
                "PRICE_HISTORY",
                "TIME",
                "DERIVE_API_KEY",
            ] {
                public.push(property(h.as_ref(), "ENDPOINTS", key)?);
            }
            let mut is_public = false;
            for endpoint in public {
                if h.starts_with(&url, endpoint)? {
                    is_public = true;
                    break;
                }
            }
            let now = h.date_now()?;
            let rate = property(
                h.as_ref(),
                "RATE_LIMITS",
                if is_public {
                    "PUBLIC_RPS"
                } else {
                    "PRIVATE_RPS"
                },
            )?;
            let minimum = 1000. / h.to_number(&rate)?;
            if now - last_request.get() < minimum {
                let delay = minimum - (now - last_request.get());
                wait(h.sleep(delay)).await?;
            }
            last_request.set(h.date_now()?);
            if !is_public {
                let credentials = h.apply(credentials, vec![])?;
                if !h.truthy(&credentials) {
                    let log = h.capture_value(exchange.clone(), "log")?;
                    h.invoke_log(
                        log,
                        &"[Polymarket API] No credentials available for private endpoint"
                            .encode_utf16()
                            .collect::<Vec<_>>(),
                        "error",
                    )?;
                    return Err(h.new_error("Polymarket credentials not initialized"));
                }
                let address = h.apply(wallet_address, vec![])?;
                if !h.truthy(&address) {
                    return Err(h.new_error("Polymarket wallet address not initialized"));
                }
                let method = h.field(&config, "method")?;
                let method = if h.nullish(&method) {
                    h.undefined()
                } else {
                    h.uppercase(method)?
                };
                let method = if h.truthy(&method) {
                    method
                } else {
                    h.text("GET")
                };
                let path = h.field(&config, "url")?;
                let path = if h.truthy(&path) { path } else { h.text("") };
                let body = if h.is_undefined(&h.field(&config, "data")?) {
                    h.undefined()
                } else {
                    let data = h.field(&config, "data")?;
                    if h.is_string_value(&data) {
                        h.field(&config, "data")?
                    } else {
                        h.json_stringify(h.field(&config, "data")?)?
                    }
                };
                let sign = h.capture("imports", "buildL2Headers")?;
                let headers = h.call_sync(
                    sign,
                    vec![credentials, address, method.clone(), path.clone(), body],
                )?;
                for (key, value) in h.entries(&headers)? {
                    let headers = h.field(&config, "headers")?;
                    let set = h.capture_value(headers, "set")?;
                    h.call_sync(set, vec![key, value])?;
                }
                let log = h.capture_value(exchange, "log")?;
                let mut message = h.string(&text(
                    h.as_ref(),
                    "[Polymarket API] Added L2 headers for ",
                    &method,
                )?)?;
                message.push(32);
                message.extend(h.string(&path)?);
                h.invoke_log(log, &message, "debug")?;
            }
            returned(h.as_ref(), config).await
        }
    })
}
fn response_success<H: FactoryHost>(h: &H, response: H::Value) -> Result<H::Value, H::Error> {
    let error = optional(h, h.field(&response, "data")?, "error")?;
    if h.truthy(&error) {
        let data = h.field(&response, "data")?;
        let message = h.field(&data, "message")?;
        let message = if h.truthy(&message) {
            message
        } else {
            h.field(&h.field(&response, "data")?, "error")?
        };
        let constructor = h.capture_constructor("Error")?;
        let message = text(h, "Polymarket Error: ", &message)?;
        let error = h.construct(constructor, vec![message])?;
        h.assign(&error, "isAxiosError", h.boolean(true))?;
        h.assign(&error, "response", response.clone())?;
        h.assign(&error, "config", h.field(&response, "config")?)?;
        h.promise_reject(error)
    } else {
        Ok(response)
    }
}
fn response_failure<H: FactoryHost>(
    h: &H,
    error: H::Value,
    exchange: H::Value,
) -> Result<H::Value, H::Error> {
    let data = optional(h, h.field(&error, "response")?, "data")?;
    if h.truthy(&data) {
        let data = h.field(&h.field(&error, "response")?, "data")?;
        let message = h.field(&data, "message")?;
        let message = if h.truthy(&message) {
            message
        } else {
            let e = h.field(&data, "error")?;
            if h.truthy(&e) {
                e
            } else {
                h.text("Unknown error")
            }
        };
        let log = h.capture_value(exchange, "log")?;
        let message_text = text(h, "[Polymarket API] Error: ", &message)?;
        h.invoke_log(log, &h.string(&message_text)?, "error")?;
        let enhanced = h.inherit(error)?;
        let define = h.capture("Object", "defineProperty")?;
        let descriptor = h.object(vec![
            ("value", text(h, "Polymarket Error: ", &message)?),
            ("writable", h.boolean(false)),
            ("enumerable", h.boolean(true)),
        ])?;
        h.call_sync(
            define,
            vec![enhanced.clone(), h.text("message"), descriptor],
        )?;
        h.promise_reject(enhanced)
    } else {
        h.promise_reject(error)
    }
}
