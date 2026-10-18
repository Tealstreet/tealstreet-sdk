//! GRVT Axios construction and session/error interceptors against bound JS primitives.
use crate::{
    builder_workflow::JsResult,
    session_host::{SessionHost, method, optional, session_template},
};
use safe_cex_exchange::runtime::LogSeverity;
use std::{cell::Cell, rc::Rc};

pub trait ApiHost: SessionHost {
    fn base_urls(&self) -> Self::Value;
    fn create_axios(
        &self,
        config: Self::Value,
        options: Self::Value,
        exchange: Self::Value,
    ) -> JsResult<Self::Value>;
    fn read_retry_condition(&self) -> JsResult<Self::Value>;
    fn install_retry(
        &self,
        instance: Self::Value,
        options: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    fn severity(&self, severity: LogSeverity) -> Self::Value;
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gateway {
    Market,
    Edge,
    Trading,
}
impl Gateway {
    fn suffix(self) -> &'static str {
        match self {
            Self::Market => "MDG",
            Self::Edge => "EDGE",
            Self::Trading => "TDG",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::Market => "GRVT API Error",
            Self::Edge => "GRVT Edge API Error",
            Self::Trading => "GRVT Trading API Error",
        }
    }
}
fn extras<H: SessionHost>(h: &H, exchange: &H::Value, key: &str) -> JsResult<H::Value> {
    h.get(&h.get(exchange, "apiExtras")?, key)
}
fn optional_extras<H: SessionHost>(h: &H, exchange: &H::Value, key: &str) -> JsResult<H::Value> {
    optional(h, h.get(exchange, "apiExtras")?, key)
}
fn ensure_extras<H: SessionHost>(h: &H, exchange: &H::Value) -> JsResult<H::Value, ()> {
    if !h.truthy(&h.get(exchange, "apiExtras")?) {
        h.set(exchange, "apiExtras", h.object(vec![]))?;
    }
    Ok(())
}
fn log<H: ApiHost>(
    h: &H,
    exchange: &H::Value,
    message: impl FnOnce() -> JsResult<H::Value>,
    severity: LogSeverity,
) -> JsResult<H::Value, ()> {
    let function = h.get(exchange, "log")?;
    if !h.nullish(&function) {
        h.call(
            function,
            exchange.clone(),
            vec![message()?, h.severity(severity)],
        )?;
    }
    Ok(())
}
fn lite<H: SessionHost>(h: &H, data: &H::Value) -> JsResult<H::Value, bool> {
    Ok(h.truthy(&optional(h, data.clone(), "c")?) && h.truthy(&optional(h, data.clone(), "m")?))
}
fn lite_message<H: SessionHost>(h: &H, data: &H::Value, gateway: Gateway) -> JsResult<H::Value> {
    session_template!(h; h.string(gateway.prefix()), h.string(": "), h.get(data, "m")?, h.string(" (code: "), h.get(data, "c")?, h.string(")"))
}
fn cookie<H: ApiHost>(
    h: Rc<H>,
    response: &H::Value,
    exchange: &H::Value,
    gateway: Gateway,
) -> JsResult<H::Value, ()> {
    let raw = h.get(&h.get(response, "headers")?, "set-cookie")?;
    if h.truthy(&raw) {
        let cookies = if h.is_array(&raw) {
            raw
        } else {
            h.array(vec![raw])
        };
        let find = h.get(&cookies, "find")?;
        let host = h.clone();
        let predicate = h.function(Rc::new(move |cookie| {
            method(
                host.as_ref(),
                cookie,
                "startsWith",
                vec![host.string("gravity=")],
            )
        }));
        let selected = h.call(find, cookies, vec![predicate])?;
        if h.truthy(&selected) {
            let pieces = method(h.as_ref(), selected, "split", vec![h.string(";")])?;
            let first = h.get(&pieces, "0")?;
            let parts = method(h.as_ref(), first, "split", vec![h.string("=")])?;
            let value = h.get(&parts, "1")?;
            ensure_extras(h.as_ref(), exchange)?;
            h.set(&h.get(exchange, "apiExtras")?, "sessionCookie", value)?;
            if gateway == Gateway::Edge {
                log(
                    h.as_ref(),
                    exchange,
                    || Ok(h.string("GRVT session cookie refreshed from Set-Cookie response")),
                    LogSeverity::Debug,
                )?;
            }
        }
    }
    Ok(())
}
fn fulfilled<H: ApiHost>(
    h: Rc<H>,
    response: H::Value,
    exchange: &H::Value,
    gateway: Gateway,
) -> JsResult<H::Value> {
    let data = h.get(&response, "data")?;
    if lite(h.as_ref(), &data)? {
        return Ok(h.reject(h.error(lite_message(h.as_ref(), &data, gateway)?)?));
    }
    if gateway == Gateway::Edge {
        let proxy = h.get(&h.get(&response, "headers")?, "x-ts-grvt-session")?;
        if h.truthy(&proxy) {
            ensure_extras(h.as_ref(), exchange)?;
            h.set(&h.get(exchange, "apiExtras")?, "sessionCookie", proxy)?;
            log(
                h.as_ref(),
                exchange,
                || Ok(h.string("GRVT session cookie refreshed from proxy response")),
                LogSeverity::Debug,
            )?;
        } else {
            cookie(h.clone(), &response, exchange, gateway)?;
        }
    } else {
        cookie(h.clone(), &response, exchange, gateway)?;
    }
    let account = h.get(&h.get(&response, "headers")?, "x-grvt-account-id")?;
    if h.truthy(&account) {
        ensure_extras(h.as_ref(), exchange)?;
        h.set(&h.get(exchange, "apiExtras")?, "accountId", account)?;
        if gateway == Gateway::Edge {
            log(
                h.as_ref(),
                exchange,
                || Ok(h.string("GRVT account ID refreshed from authentication response")),
                LogSeverity::Debug,
            )?;
        }
    }
    Ok(response)
}
fn wrapped_error<H: ApiHost>(
    h: &H,
    error: H::Value,
    status: H::Value,
    data: H::Value,
    gateway: Gateway,
) -> JsResult<H::Value> {
    let mut message = h.string(gateway.prefix());
    if lite(h, &data)? {
        message = lite_message(h, &data, gateway)?;
    } else if h.truthy(&optional(
        h,
        optional(h, data.clone(), "error")?,
        "message",
    )?) {
        message = session_template!(h; h.string(gateway.prefix()), h.string(": "), h.get(&h.get(&data, "error")?, "message")?)?;
        if gateway == Gateway::Trading && h.truthy(&h.get(&h.get(&data, "error")?, "code")?) {
            message = session_template!(h; message, h.string(" (code: "), h.get(&h.get(&data, "error")?, "code")?, h.string(")"))?;
        }
    } else if h.truthy(&optional(h, data.clone(), "message")?) {
        message = session_template!(h; h.string(gateway.prefix()), h.string(": "), h.get(&data, "message")?)?;
    } else if h.is_string(&data) {
        message = session_template!(h; h.string(gateway.prefix()), h.string(": "), data)?;
    }
    let custom = h.error(session_template!(h; message, h.string(" ("), status, h.string(")"))?)?;
    h.set(&custom, "originalError", error.clone())?;
    h.set(&custom, "response", h.get(&error, "response")?)?;
    Ok(h.reject(custom))
}
fn rejected<H: ApiHost>(h: &H, error: H::Value, gateway: Gateway) -> JsResult<H::Value> {
    if h.truthy(&h.get(&error, "response")?) {
        let status = h.get(&h.get(&error, "response")?, "status")?;
        let data = h.get(&h.get(&error, "response")?, "data")?;
        wrapped_error(h, error, status, data, gateway)
    } else if h.truthy(&h.get(&error, "request")?) {
        Ok(h.reject(h.error(h.string(&format!("{}: No response received", gateway.prefix())))?))
    } else {
        Ok(h.reject(error))
    }
}
fn request<H: ApiHost>(h: &H, config: H::Value, exchange: &H::Value) -> JsResult<H::Value> {
    for (slot, header, adapter) in [
        ("sessionCookie", "X-Ts-Grvt-Session", "grvtCookie"),
        ("accountId", "X-Grvt-Account-Id", "grvtAccountId"),
    ] {
        if h.truthy(&optional_extras(h, exchange, slot)?) {
            let setter = optional(h, h.get(&config, "headers")?, "set")?;
            if h.is_function(&setter) {
                let headers = h.get(&config, "headers")?;
                let setter = h.get(&headers, "set")?;
                h.call(
                    setter,
                    headers,
                    vec![h.string(header), extras(h, exchange, slot)?],
                )?;
            } else {
                let headers = h.get(&config, "headers")?;
                h.set(&headers, header, extras(h, exchange, slot)?)?;
            }
            let value = if slot == "sessionCookie" {
                session_template!(h; h.string("rm=true; gravity="), extras(h, exchange, slot)?)?
            } else {
                extras(h, exchange, slot)?
            };
            h.set(&config, adapter, value)?;
        }
    }
    Ok(config)
}
async fn trading_rejected<H: ApiHost>(
    h: Rc<H>,
    error: H::Value,
    exchange: H::Value,
    instance: H::Value,
    reauth: Rc<Cell<bool>>,
) -> JsResult<H::Value> {
    if !h.truthy(&h.get(&error, "response")?) {
        return if h.truthy(&h.get(&error, "request")?) {
            Ok(h.reject(h.error(h.string("GRVT Trading API Error: No response received"))?))
        } else {
            Ok(h.reject(error))
        };
    }
    let status = h.get(&h.get(&error, "response")?, "status")?;
    let data = h.get(&h.get(&error, "response")?, "data")?;
    let mut code = optional(h.as_ref(), data.clone(), "c")?;
    if !h.truthy(&code) {
        code = optional(h.as_ref(), data.clone(), "code")?;
    }
    if !h.truthy(&code) {
        code = optional(
            h.as_ref(),
            optional(h.as_ref(), data.clone(), "error")?,
            "code",
        )?;
    }
    if h.equals_number(&code, 1000.0)?
        && !reauth.get()
        && h.truthy(&h.get(&error, "config")?)
        && !h.truthy(&h.get(&h.get(&error, "config")?, "_retry")?)
    {
        reauth.set(true);
        log(
            h.as_ref(),
            &exchange,
            || Ok(h.string("GRVT authentication error 1000; refreshing session")),
            LogSeverity::Warning,
        )?;
        let attempt: JsResult<H::Value, Option<H::Value>> = async {
            if h.truthy(&h.get(&exchange, "apiExtras")?) {
                h.set(&h.get(&exchange, "apiExtras")?, "sessionCookie", h.undefined())?;
                h.set(&h.get(&exchange, "apiExtras")?, "accountId", h.undefined())?;
            }
            if h.truthy(&optional(h.as_ref(), h.get(&exchange, "grvtClient")?, "cookie")?) { h.set(&h.get(&exchange, "grvtClient")?, "cookie", h.null())?; }
            if h.is_function(&h.get(&exchange, "authenticate")?) {
                let authenticate = h.get(&exchange, "authenticate")?;
                let auth = h.call(authenticate, exchange.clone(), vec![])?;
                let auth = h.await_value(auth).await?;
                if h.truthy(&auth) {
                    log(h.as_ref(), &exchange, || Ok(h.string("GRVT session refresh succeeded; retrying request")), LogSeverity::Debug)?;
                    h.set(&h.get(&error, "config")?, "_retry", h.boolean(true))?;
                    for (slot, header, adapter) in [("sessionCookie", "X-Ts-Grvt-Session", "grvtCookie"), ("accountId", "X-Grvt-Account-Id", "grvtAccountId")] {
                        if h.truthy(&optional_extras(h.as_ref(), &exchange, slot)?) {
                            let headers = h.get(&h.get(&error, "config")?, "headers")?;
                            h.set(&headers, header, extras(h.as_ref(), &exchange, slot)?)?;
                            let config = h.get(&error, "config")?;
                            let value = if slot == "sessionCookie" { session_template!(h; h.string("rm=true; gravity="), extras(h.as_ref(), &exchange, slot)?)? } else { extras(h.as_ref(), &exchange, slot)? };
                            h.set(&config, adapter, value)?;
                        }
                    }
                    reauth.set(false);
                    return Ok(Some(h.call(instance, h.undefined(), vec![h.get(&error, "config")?])?));
                }
            }
            Ok(None)
        }.await;
        let result = match attempt {
            Err(auth_error) => log(
                h.as_ref(),
                &exchange,
                || session_template!(h; h.string("GRVT session refresh failed: "), auth_error),
                LogSeverity::Warning,
            )
            .map(|()| None),
            success => success,
        };
        reauth.set(false);
        if let Some(value) = result? {
            return Ok(value);
        }
    }
    wrapped_error(h.as_ref(), error, status, data, Gateway::Trading)
}
fn create<H: ApiHost>(
    h: Rc<H>,
    options: H::Value,
    exchange: H::Value,
    gateway: Gateway,
) -> JsResult<H::Value> {
    let testnet = h.get(&options, "testnet")?;
    let key = format!(
        "{}_{}",
        if h.truthy(&testnet) {
            "TESTNET"
        } else {
            "PRODUCTION"
        },
        gateway.suffix()
    );
    let url = h.get(&h.base_urls(), &key)?;
    let mut fields = vec![
        ("baseURL", url),
        (
            "headers",
            h.object(vec![("Content-Type", h.string("application/json"))]),
        ),
        ("timeout", h.number(30000.0)),
    ];
    if gateway != Gateway::Market {
        fields.push(("withCredentials", h.boolean(true)));
    }
    let instance = h.create_axios(h.object(fields), options, exchange.clone())?;
    let retry_options = h.object(vec![
        ("retries", h.number(5.0)),
        ("retryCondition", h.read_retry_condition()?),
    ]);
    h.install_retry(instance.clone(), retry_options)?;
    let reauth = Rc::new(Cell::new(false));
    if gateway == Gateway::Trading {
        let interceptors = h.get(&instance, "interceptors")?;
        let request_manager = h.get(&interceptors, "request")?;
        let use_method = h.get(&request_manager, "use")?;
        let host = h.clone();
        let parent = exchange.clone();
        let on_fulfilled = h.function(Rc::new(move |config| {
            request(host.as_ref(), config, &parent)
        }));
        let host = h.clone();
        let on_rejected = h.function(Rc::new(move |error| Ok(host.reject(error))));
        h.call(use_method, request_manager, vec![on_fulfilled, on_rejected])?;
    }
    let interceptors = h.get(&instance, "interceptors")?;
    let response_manager = h.get(&interceptors, "response")?;
    let use_method = h.get(&response_manager, "use")?;
    let host = h.clone();
    let parent = exchange.clone();
    let on_fulfilled = h.function(Rc::new(move |response| {
        fulfilled(host.clone(), response, &parent, gateway)
    }));
    let host = h.clone();
    let client = instance.clone();
    let on_rejected = h.function(Rc::new(move |error| {
        if gateway == Gateway::Trading {
            let h = host.clone();
            Ok(host.promise(host.start(Box::pin(trading_rejected(
                h,
                error,
                exchange.clone(),
                client.clone(),
                reauth.clone(),
            )))))
        } else {
            rejected(host.as_ref(), error, gateway)
        }
    }));
    h.call(
        use_method,
        response_manager,
        vec![on_fulfilled, on_rejected],
    )?;
    Ok(instance)
}
/// ts: createAPI.
pub fn create_api<H: ApiHost>(
    h: Rc<H>,
    options: H::Value,
    exchange: H::Value,
) -> JsResult<H::Value> {
    create(h, options, exchange, Gateway::Market)
}
/// ts: createEdgeAPI.
pub fn create_edge_api<H: ApiHost>(
    h: Rc<H>,
    options: H::Value,
    exchange: H::Value,
) -> JsResult<H::Value> {
    create(h, options, exchange, Gateway::Edge)
}
/// ts: createTradingAPI.
pub fn create_trading_api<H: ApiHost>(
    h: Rc<H>,
    options: H::Value,
    exchange: H::Value,
) -> JsResult<H::Value> {
    create(h, options, exchange, Gateway::Trading)
}
