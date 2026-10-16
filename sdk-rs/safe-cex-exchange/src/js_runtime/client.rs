//! Axios-shaped client: request interceptors (LIFO), host send, response
//! interceptors (FIFO), axios-retry and axios-rate-limit spacing.
use super::{
    promise::{JsFuture, Promise, await_js},
    value::{Js, Thrown, arg, from_json, to_json},
};
use crate::runtime::ExchangeRuntime;
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use std::{cell::RefCell, rc::Rc};

/// What a client needs from its host: the runtime owning launched requests and
/// the wire send, which receives the config after the request interceptors ran
/// and resolves the Axios response object `{ data, status, headers, config }`.
pub trait JsHttpHost {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime>;
    fn send(&self, config: Js) -> JsFuture<'static>;
}

type Host = Rc<dyn JsHttpHost>;

fn spawn(host: &Host, body: JsFuture<'static>) -> Js {
    Js::Promise(Promise::spawn(&host.runtime(), body))
}

/// One `interceptors.*.use(onFulfilled, onRejected)` registration.
type Interceptor = (Js, Js);

#[derive(Default)]
struct ClientState {
    requests: Vec<Interceptor>,
    responses: Vec<Interceptor>,
    retries: u32,
    retry_condition: Option<Js>,
    /// axios-rate-limit windows `(maxRequests, perMilliseconds, sent)`; a chained
    /// limiter adds a window and a request waits until every window has room.
    windows: Vec<(f64, f64, Vec<f64>)>,
}

/// safeCexAxiosCreate(config): a client object whose verbs run the full pipeline.
pub fn create(host: Host, defaults: Js) -> Js {
    let state = Rc::new(RefCell::new(ClientState::default()));
    let client = Js::object_with_class("Axios", vec![("defaults", defaults.clone())]);
    let use_fn = |list: fn(&mut ClientState) -> &mut Vec<Interceptor>| {
        let state = state.clone();
        Js::function("use", move |_, a| {
            let mut state = state.borrow_mut();
            let list = list(&mut state);
            list.push((arg(&a, 0), arg(&a, 1)));
            Ok(Js::Number((list.len() - 1) as f64))
        })
    };
    let _ = client.define(
        "interceptors",
        Js::object(vec![
            (
                "request",
                Js::object(vec![("use", use_fn(|s| &mut s.requests))]),
            ),
            (
                "response",
                Js::object(vec![("use", use_fn(|s| &mut s.responses))]),
            ),
        ]),
    );
    let verb = |method: &'static str, has_data: bool| {
        let host = host.clone();
        let state = state.clone();
        let defaults = defaults.clone();
        Js::function(method, move |_, a| {
            let url = arg(&a, 0);
            let (data, config) = if has_data {
                (arg(&a, 1), arg(&a, 2))
            } else {
                (Js::Undefined, arg(&a, 1))
            };
            let request = config.spread_object()?;
            request.define("url", url)?;
            request.define("method", Js::str(method))?;
            if has_data {
                request.define("data", data)?;
            }
            Ok(spawn(
                &host,
                dispatch(host.clone(), state.clone(), defaults.clone(), request),
            ))
        })
    };
    let _ = client.define("get", verb("get", false));
    let _ = client.define("delete", verb("delete", false));
    let _ = client.define("post", verb("post", true));
    let _ = client.define("put", verb("put", true));
    let request_host = host.clone();
    let request_state = state.clone();
    let request_defaults = defaults.clone();
    let _ = client.define(
        "request",
        Js::function("request", move |_, a| {
            let config = arg(&a, 0).spread_object()?;
            Ok(spawn(
                &request_host,
                dispatch(
                    request_host.clone(),
                    request_state.clone(),
                    request_defaults.clone(),
                    config,
                ),
            ))
        }),
    );
    let retry_state = state.clone();
    let _ = client.define(
        "__configureRetry",
        Js::function("__configureRetry", move |_, a| {
            let opts = arg(&a, 0);
            let mut s = retry_state.borrow_mut();
            s.retries = opts.get("retries")?.to_number()? as u32;
            let condition = opts.get("retryCondition")?;
            s.retry_condition = condition.is_callable().then_some(condition);
            Ok(Js::Undefined)
        }),
    );
    let limit_state = state;
    let _ = client.define(
        "__configureRateLimit",
        Js::function("__configureRateLimit", move |_, a| {
            let opts = arg(&a, 0);
            let rps = opts.get("maxRPS")?;
            let window = if rps.is_undefined() {
                (
                    opts.get("maxRequests")?.to_number()?,
                    opts.get("perMilliseconds")?.to_number()?,
                )
            } else {
                (rps.to_number()?, 1000.0)
            };
            limit_state
                .borrow_mut()
                .windows
                .push((window.0, window.1, vec![]));
            Ok(Js::Undefined)
        }),
    );
    client
}

fn merged_config(defaults: &Js, request: &Js) -> Result<Js, Thrown> {
    let config = defaults.spread_object()?;
    let headers = defaults.get("headers")?.spread_object()?;
    for (k, v) in request.get("headers")?.spread_object()?.own_entries()? {
        headers.define_key(&k, v)?;
    }
    for (k, v) in request.own_entries()? {
        config.define_key(&k, v)?;
    }
    config.define("headers", headers)?;
    Ok(config)
}

fn axios_error(response: &Js, config: &Js) -> Result<Thrown, Thrown> {
    let status = response.get("status")?;
    let error = super::value::error_object(
        "AxiosError",
        &format!("Request failed with status code {}", status.to_display()),
    );
    error.define("response", response.clone())?;
    error.define("config", config.clone())?;
    error.define("isAxiosError", Js::Bool(true))?;
    error.define("status", status)?;
    Ok(Thrown::new(error))
}

fn dispatch(
    host: Host,
    state: Rc<RefCell<ClientState>>,
    defaults: Js,
    request: Js,
) -> JsFuture<'static> {
    Box::pin(async move {
        let mut attempt = 0;
        loop {
            match send_once(&host, &state, &defaults, request.clone()).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let (retries, condition) = {
                        let s = state.borrow();
                        (s.retries, s.retry_condition.clone())
                    };
                    let retry = attempt < retries
                        && match &condition {
                            Some(condition) => condition
                                .call(Js::Undefined, vec![error.value.clone()])?
                                .truthy(),
                            None => false,
                        };
                    if !retry {
                        return Err(error);
                    }
                    attempt += 1;
                }
            }
        }
    })
}

/// Axios' `promise.then(onFulfilled, onRejected)` link: a missing handler passes
/// the settlement through; a rejection handler's return recovers the chain.
async fn chain_step(interceptor: &Interceptor, current: Result<Js, Thrown>) -> Result<Js, Thrown> {
    let (on_fulfilled, on_rejected) = interceptor;
    match current {
        Ok(value) if on_fulfilled.is_callable() => {
            await_js(on_fulfilled.call(Js::Undefined, vec![value])?).await
        }
        Err(error) if on_rejected.is_callable() => {
            await_js(on_rejected.call(Js::Undefined, vec![error.value])?).await
        }
        settled => settled,
    }
}

/// One axios chain: request interceptors (LIFO), the adapter, response
/// interceptors (FIFO), all links of one promise chain.
async fn send_once(
    host: &Host,
    state: &Rc<RefCell<ClientState>>,
    defaults: &Js,
    request: Js,
) -> Result<Js, Thrown> {
    let mut current = Ok(merged_config(defaults, &request)?);
    let requests: Vec<Interceptor> = state.borrow().requests.iter().rev().cloned().collect();
    for interceptor in &requests {
        current = chain_step(interceptor, current).await;
    }
    current = match current {
        Ok(config) => dispatch_request(host, state, config).await,
        Err(error) => Err(error),
    };
    let responses: Vec<Interceptor> = state.borrow().responses.clone();
    for interceptor in &responses {
        current = chain_step(interceptor, current).await;
    }
    current
}

async fn dispatch_request(
    host: &Host,
    state: &Rc<RefCell<ClientState>>,
    config: Js,
) -> Result<Js, Thrown> {
    throttle(host, state).await;
    let response = host.send(config.clone()).await?;
    if !response.get("config")?.is_object() {
        response.define("config", config.clone())?;
    }
    let status = response.get("status")?.to_number().unwrap_or(200.0);
    if !(200.0..300.0).contains(&status) {
        return Err(axios_error(&response, &config)?);
    }
    Ok(response)
}

async fn throttle(host: &Host, state: &Rc<RefCell<ClientState>>) {
    loop {
        let runtime = host.runtime();
        let now = runtime.monotonic_now_ms();
        let wait = {
            let mut s = state.borrow_mut();
            let mut wait: Option<f64> = None;
            for (max, per, sent) in s.windows.iter_mut() {
                if *max <= 0.0 {
                    continue;
                }
                sent.retain(|t| now - t < *per);
                if sent.len() as f64 >= *max {
                    let ms = *per - (now - sent[0]);
                    wait = Some(wait.map_or(ms, |w| w.max(ms)));
                }
            }
            if wait.is_none() {
                for (_, _, sent) in s.windows.iter_mut() {
                    sent.push(now);
                }
            }
            wait
        };
        match wait {
            None => return,
            Some(ms) => {
                let _ = runtime.sleep(ms.max(1.0) as u64).await;
            }
        }
    }
}

fn optional(v: &Js, key: &str) -> Result<Js, Thrown> {
    if v.is_nullish() {
        Ok(Js::Undefined)
    } else {
        v.get(key)
    }
}

fn rest_response(response: &Js) -> Result<RestResponse, Thrown> {
    let status = response.get("status")?.to_number().unwrap_or(200.0);
    Ok(RestResponse {
        status: if status.is_finite() { status as u16 } else { 0 },
        reason: None,
        headers: Default::default(),
        body: RestResponseBody::Untransformed(to_json(&response.get("data")?)?),
    })
}

/// A JS Axios client as a typed `RestAdapter`: requests run the client's whole
/// pipeline (interceptors, signing, retry). A rejection keeps its JS value so
/// [`JsRestAdapter::thrown`] can rethrow it unchanged.
pub struct JsRestAdapter {
    xhr: Js,
    rejection: RefCell<Option<Thrown>>,
}

impl JsRestAdapter {
    pub fn new(xhr: Js) -> Self {
        Self {
            xhr,
            rejection: RefCell::default(),
        }
    }
    async fn send(&self, request: &RestRequest) -> Result<RestResponse, Thrown> {
        let config = Js::object(
            request
                .params
                .iter()
                .map(|params| ("params", from_json(params)))
                .collect(),
        );
        let url = Js::str(&request.url);
        // Axios normalizes the verb; RestRequest callers send "GET" as often as "get".
        let method = request.method.to_lowercase();
        let pending = if method == "post" {
            let data = match &request.body {
                Some(body) => {
                    let bytes = body
                        .wire_bytes()
                        .map_err(|e| Thrown::error("Error", &e.to_string()))?;
                    let text = String::from_utf8_lossy(&bytes);
                    from_json(
                        &serde_json::from_str(&text)
                            .unwrap_or(serde_json::Value::String(text.into_owned())),
                    )
                }
                None => Js::Undefined,
            };
            self.xhr.call_method("post", vec![url, data, config])?
        } else {
            self.xhr.call_method(&method, vec![url, config])?
        };
        rest_response(&await_js(pending).await?)
    }
    /// The body's `RestFailure` as JS: this adapter's own rejection as-is, else a host Error.
    pub fn thrown(&self, failure: RestFailure) -> Thrown {
        if let Some(original) = self.rejection.borrow_mut().take() {
            return original;
        }
        let name = failure
            .context
            .as_deref()
            .and_then(|c| c.get("name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Error")
            .to_owned();
        Thrown::error(&name, &failure.message)
    }
}

impl RestAdapter for JsRestAdapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            match self.send(&request).await {
                Ok(response) => Ok(response),
                Err(thrown) => {
                    let response = optional(&thrown.value, "response")
                        .ok()
                        .filter(|r| !r.is_nullish())
                        .and_then(|r| rest_response(&r).ok());
                    let code = optional(&thrown.value, "code")
                        .ok()
                        .filter(Js::is_string)
                        .map(|c| String::from_utf16_lossy(&c.to_js_string().unwrap_or_default()));
                    let failure = RestFailure {
                        code,
                        message: thrown.message(),
                        transport_error: None,
                        response: response.map(Box::new),
                        request: Box::new(request),
                        context: None,
                    };
                    *self.rejection.borrow_mut() = Some(thrown);
                    Err(failure)
                }
            }
        })
    }
}

/// The production `JsHttpHost`: the post-interceptor config becomes a
/// `RestRequest` for a terminal adapter. Status handling stays with the client.
struct RestHttpHost {
    runtime: Rc<dyn ExchangeRuntime>,
    adapter: std::sync::Arc<dyn RestAdapter>,
}

pub fn rest_http_host(
    runtime: Rc<dyn ExchangeRuntime>,
    adapter: std::sync::Arc<dyn RestAdapter>,
) -> Rc<dyn JsHttpHost> {
    Rc::new(RestHttpHost { runtime, adapter })
}

fn optional_text(config: &Js, key: &str) -> Result<Option<String>, Thrown> {
    let value = config.get(key)?;
    Ok(if value.is_nullish() {
        None
    } else {
        Some(String::from_utf16_lossy(&value.to_js_string()?))
    })
}

fn rest_request(config: &Js) -> Result<RestRequest, Thrown> {
    let mut headers = indexmap::IndexMap::new();
    let config_headers = config.get("headers")?;
    if !config_headers.is_nullish() {
        for (name, value) in config_headers.own_entries()? {
            if !value.is_nullish() {
                headers.insert(
                    String::from_utf16_lossy(&name),
                    String::from_utf16_lossy(&value.to_js_string()?),
                );
            }
        }
    }
    let data = config.get("data")?;
    let body = match &data {
        Js::Undefined | Js::Null => None,
        Js::String(text) => Some(RestBody::Encoded(
            String::from_utf16_lossy(text).into_bytes(),
        )),
        other => Some(RestBody::Untransformed(to_json(other)?)),
    };
    let mut url = optional_text(config, "url")?.unwrap_or_default();
    let mut params = config.get("params")?;
    // Axios: a configured paramsSerializer (function or `{ serialize }`) owns the query.
    let serializer = config.get("paramsSerializer")?;
    let serialize = if serializer.is_callable() {
        serializer
    } else if serializer.is_object() {
        super::scope::bound_member(&serializer, "serialize")?
    } else {
        Js::Undefined
    };
    if serialize.is_callable() && !params.is_nullish() {
        let query = serialize.call(Js::Undefined, vec![params])?;
        let query = String::from_utf16_lossy(&query.to_js_string()?);
        if !query.is_empty() {
            if let Some(hash) = url.find('#') {
                url.truncate(hash);
            }
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str(&query);
        }
        params = Js::Undefined;
    }
    let timeout = config.get("timeout")?;
    Ok(RestRequest {
        method: optional_text(config, "method")?
            .unwrap_or_else(|| "get".into())
            .to_lowercase(),
        url,
        base_url: optional_text(config, "baseURL")?,
        headers,
        body,
        timeout_ms: timeout
            .as_f64()
            .filter(|ms| ms.is_finite() && *ms > 0.0)
            .map(|ms| ms as i64),
        params: if params.is_nullish() {
            None
        } else {
            Some(to_json(&params)?)
        },
        ..Default::default()
    })
}

/// Axios' default transformResponse: JSON text parses, anything else stays text.
fn response_data(body: RestResponseBody) -> Js {
    match body {
        RestResponseBody::Encoded(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            serde_json::from_str(&text).map_or_else(|_| Js::str(&text), |v| from_json(&v))
        }
        RestResponseBody::Untransformed(value) | RestResponseBody::Transformed(value) => {
            from_json(&value)
        }
    }
}

fn js_response(response: RestResponse, config: Js) -> Js {
    let headers = Js::object(vec![]);
    for (name, value) in &response.headers {
        let _ = headers.define(name, Js::str(value));
    }
    Js::object(vec![
        ("data", response_data(response.body)),
        ("status", Js::Number(f64::from(response.status))),
        (
            "statusText",
            Js::str(response.reason.as_deref().unwrap_or_default()),
        ),
        ("headers", headers),
        ("config", config),
    ])
}

impl JsHttpHost for RestHttpHost {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let adapter = self.adapter.clone();
        Box::pin(async move {
            let request = rest_request(&config)?;
            match adapter.execute(request).await {
                Ok(response) => Ok(js_response(response, config)),
                // A status failure still carries the response; the client classifies it.
                Err(RestFailure {
                    response: Some(response),
                    ..
                }) => Ok(js_response(*response, config)),
                Err(failure) => {
                    let error = super::value::error_object("AxiosError", &failure.message);
                    if let Some(code) = &failure.code {
                        error.define("code", Js::str(code))?;
                    }
                    error.define("config", config)?;
                    error.define("isAxiosError", Js::Bool(true))?;
                    Err(Thrown::new(error))
                }
            }
        })
    }
}
