//! Axios-shaped client: request interceptors (LIFO), host send, response
//! interceptors (FIFO), axios-retry and axios-rate-limit spacing.
use super::{
    promise::{JsFuture, Promise, await_js},
    value::{Js, Thrown, arg, from_json, from_retained_input, to_json, to_retained_input},
};
use crate::runtime::ExchangeRuntime;
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use std::{cell::RefCell, rc::Rc};

pub mod factory;

/// What a client needs from its host: the runtime owning launched requests and
/// the wire send, which receives the config after the request interceptors ran
/// and resolves the Axios response object `{ data, status, headers, config }`.
pub trait JsHttpHost {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime>;
    fn send(&self, config: Js) -> JsFuture<'static>;
    /// Bind a fresh host to the exchange whose createAPI clients use it.
    fn configure_exchange(&self, _name: safe_cex_types::ExchangeName) -> Result<(), Thrown> {
        Ok(())
    }
    /// Hosted account capability shared by JS HTTP and BaseExchange/WS auth.
    fn hosted_signer(&self) -> Option<super::base_exchange::HostedSigner> {
        None
    }
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
    retry_options: Option<Js>,
    /// Each rate-limit registration owns one window and one request interceptor.
    windows: Vec<(f64, f64, Vec<f64>)>,
}

/// Low-level Axios client; source safeCexAxiosCreate routing lives in [`factory`].
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
    let retry_runtime = host.runtime();
    let _ = client.define(
        "__configureRetry",
        Js::function("__configureRetry", move |_, a| {
            let opts = arg(&a, 0);
            let options = opts.clone();
            let runtime = retry_runtime.clone();
            let interceptor = Js::function("axiosRetryState", move |_, a| {
                let config = arg(&a, 0);
                current_retry_state(&config, &options, runtime.as_ref(), true)?;
                Ok(config)
            });
            let mut s = retry_state.borrow_mut();
            s.retry_options = Some(opts);
            // axios-retry installs this request interceptor before the venue's
            // signing interceptor. LIFO dispatch retains that source ordering.
            s.requests.push((interceptor, Js::Undefined));
            Ok(Js::Undefined)
        }),
    );
    let limit_state = state;
    let limit_host = host.clone();
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
            let index = limit_state.borrow().windows.len();
            limit_state
                .borrow_mut()
                .windows
                .push((window.0, window.1, vec![]));
            let weak = Rc::downgrade(&limit_state);
            let host = limit_host.clone();
            let interceptor = Js::function("axiosRateLimit", move |_, a| {
                let config = arg(&a, 0);
                let state = weak.upgrade().expect("active client rate-limit state");
                let host = host.clone();
                let runtime = host.runtime();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        throttle(&host, &state, index).await?;
                        Ok(config)
                    }),
                )))
            });
            // Axios executes request interceptors in reverse registration order.
            limit_state
                .borrow_mut()
                .requests
                .push((interceptor, Js::Undefined));
            Ok(Js::Undefined)
        }),
    );
    client
}

fn merged_config(defaults: &Js, request: &Js) -> Result<Js, Thrown> {
    let config = defaults.spread_object()?;
    let headers = axios_headers()?;
    for (k, v) in defaults.get("headers")?.spread_object()?.own_entries()? {
        set_header(&headers, Js::utf16(&k), v, Js::Undefined)?;
    }
    for (k, v) in request.get("headers")?.spread_object()?.own_entries()? {
        set_header(&headers, Js::utf16(&k), v, Js::Undefined)?;
    }
    for (k, v) in request.own_entries()? {
        config.define_key(&k, v)?;
    }
    config.define("headers", headers)?;
    Ok(config)
}

// The source request interceptors receive AxiosHeaders, not a plain record.
// Methods live on its prototype so own-entry enumeration remains wire data.
fn axios_headers() -> Result<Js, Thrown> {
    let prototype = Js::object(vec![
        (
            "set",
            Js::function("set", |this, a| {
                let name = arg(&a, 0);
                if name.is_object() {
                    for (key, value) in name.own_entries()? {
                        set_header(&this, Js::utf16(&key), value, arg(&a, 1))?;
                    }
                } else if !name.is_nullish() {
                    set_header(&this, name, arg(&a, 1), arg(&a, 2))?;
                }
                Ok(this)
            }),
        ),
        (
            "get",
            Js::function("get", |this, a| {
                if arg(&a, 1).truthy() {
                    return Err(Thrown::type_error(
                        "AxiosHeaders.get parser overload is not bound",
                    ));
                }
                header_key(&this, &arg(&a, 0))?.map_or(Ok(Js::Undefined), |key| this.get_key(&key))
            }),
        ),
        (
            "has",
            Js::function("has", |this, a| {
                if arg(&a, 1).truthy() {
                    return Err(Thrown::type_error(
                        "AxiosHeaders.has matcher overload is not bound",
                    ));
                }
                Ok(Js::Bool(match header_key(&this, &arg(&a, 0))? {
                    Some(key) => !this.get_key(&key)?.is_undefined(),
                    None => false,
                }))
            }),
        ),
    ]);
    let headers = Js::object_with_class("AxiosHeaders", vec![]);
    if let Js::Object(object) = &headers {
        *object.proto.borrow_mut() = Some(prototype);
    }
    Ok(headers)
}

/// Source AxiosHeaders.from/new: preserve the shared header prototype and
/// case-insensitive normalization rather than projecting through a JSON object.
pub fn axios_headers_from(input: &Js) -> Result<Js, Thrown> {
    let headers = axios_headers()?;
    for (name, value) in input.spread_object()?.own_entries()? {
        set_header(&headers, Js::utf16(&name), value, Js::Undefined)?;
    }
    Ok(headers)
}

fn normalized_header(name: &Js) -> Result<Vec<u16>, Thrown> {
    if !name.truthy() {
        return Ok(vec![]);
    }
    Ok(name
        .call_method("toString", vec![])?
        .call_method("trim", vec![])?
        .call_method("toLowerCase", vec![])?
        .to_js_string()?
        .to_vec())
}
fn header_key(headers: &Js, name: &Js) -> Result<Option<super::value::Str>, Thrown> {
    let normalized = normalized_header(name)?;
    if normalized.is_empty() {
        return Ok(None);
    }
    for key in headers.own_keys().into_iter().rev() {
        if normalized_header(&Js::utf16(&key))? == normalized {
            return Ok(Some(key));
        }
    }
    Ok(None)
}
fn normalized_header_value(value: Js) -> Result<Js, Thrown> {
    if value.is_nullish() || value.strict_equals(&Js::Bool(false)) {
        return Ok(value);
    }
    if let Some(values) = value.array_values()? {
        return values
            .into_iter()
            .map(normalized_header_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Js::array);
    }
    let units = value.to_js_string()?;
    let units: Vec<u16> = units
        .iter()
        .copied()
        .filter(|c| !matches!(*c,0..=8|10..=31|127))
        .collect();
    let start = units
        .iter()
        .position(|c| !matches!(*c, 9 | 32))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|c| !matches!(*c, 9 | 32))
        .map_or(start, |i| i + 1);
    Ok(Js::utf16(&units[start..end]))
}
fn set_header(headers: &Js, name: Js, value: Js, rewrite: Js) -> Result<(), Thrown> {
    let normalized = normalized_header(&name)?;
    if normalized.is_empty() {
        return Ok(());
    }
    let existing = header_key(headers, &name)?;
    let current = existing
        .as_ref()
        .map(|key| headers.get_key(key))
        .transpose()?
        .unwrap_or(Js::Undefined);
    if existing.is_none()
        || current.is_undefined()
        || rewrite.strict_equals(&Js::Bool(true))
        || (rewrite.is_undefined() && !current.strict_equals(&Js::Bool(false)))
    {
        let key = match existing {
            Some(key) => key,
            None => name
                .call_method("toString", vec![])?
                .call_method("trim", vec![])?
                .to_js_string()?,
        };
        headers.define_key(&key, normalized_header_value(value)?)?;
    }
    Ok(())
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

/// axios-retry's namespace is source-visible state on the effective config.
/// Replays retain per-request overrides and counter stamps; each request resets
/// its wall-clock start after the earlier LIFO request interceptors complete.
fn current_retry_state(
    config: &Js,
    options: &Js,
    runtime: &dyn ExchangeRuntime,
    reset_start: bool,
) -> Result<Js, Thrown> {
    let current = Js::object(vec![
        ("retries", Js::Number(3.0)),
        ("shouldResetTimeout", Js::Bool(false)),
    ]);
    for source in [options.clone(), config.get("axios-retry")?] {
        for (key, value) in source.own_entries()? {
            current.define_key(&key, value)?;
        }
    }
    let count = current.get("retryCount")?;
    if !count.truthy() {
        current.define("retryCount", Js::Number(0.0))?;
    }
    if reset_start || !current.get("lastRequestTime")?.truthy() {
        current.define("lastRequestTime", Js::Number(runtime.now_ms()))?;
    }
    config.set("axios-retry", current.clone())?;
    Ok(current)
}

fn dispatch(
    host: Host,
    state: Rc<RefCell<ClientState>>,
    defaults: Js,
    mut request: Js,
) -> JsFuture<'static> {
    Box::pin(async move {
        loop {
            match send_once(&host, &state, &defaults, request.clone()).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let options = state.borrow().retry_options.clone();
                    let Some(options) = options else {
                        return Err(error);
                    };
                    let config = error.value.get("config")?;
                    if !config.is_object() {
                        return Err(error);
                    }
                    let runtime = host.runtime();
                    let retry_state =
                        current_retry_state(&config, &options, runtime.as_ref(), false)?;
                    let condition = retry_state.get("retryCondition")?;
                    let within_budget = super::ops::less_than(
                        &retry_state.get("retryCount")?,
                        &retry_state.get("retries")?,
                    )?;
                    let retry = if within_budget && condition.is_callable() {
                        let decision = condition.call(Js::Undefined, vec![error.value.clone()])?;
                        if matches!(decision, Js::Null | Js::Object(_) | Js::Promise(_)) {
                            // axios-retry treats a resolved non-false result as
                            // approval, while a rejected async predicate refuses.
                            await_js(&runtime, decision)
                                .await
                                .is_ok_and(|value| !value.strict_equals(&Js::Bool(false)))
                        } else {
                            decision.truthy()
                        }
                    } else {
                        false
                    };
                    if !retry {
                        return Err(error);
                    }
                    // axios-retry resubmits the effective failed config. This
                    // retains interceptor changes and the predicate's one-shot
                    // signature-retry stamp across request re-signing.
                    let count = super::ops::add(&retry_state.get("retryCount")?, &Js::Number(1.0))?;
                    retry_state.set("retryCount", count.clone())?;
                    let delay = retry_state.get("retryDelay")?;
                    let delay = if delay.is_undefined() {
                        0.0
                    } else {
                        delay
                            .call(Js::Undefined, vec![count, error.value.clone()])?
                            .to_number()?
                    };
                    let timeout = config.get("timeout")?;
                    let started = retry_state.get("lastRequestTime")?;
                    if !retry_state.get("shouldResetTimeout")?.truthy()
                        && timeout.truthy()
                        && started.truthy()
                    {
                        let remaining = timeout.to_number()?
                            - (runtime.now_ms() - started.to_number()?)
                            - delay;
                        if remaining <= 0.0 {
                            return Err(error);
                        }
                        config.set("timeout", Js::Number(remaining))?;
                    }
                    // The runtime owns this sleep through the already-spawned
                    // request Promise; losing an observer does not cancel replay.
                    let delay_ms = if delay.is_finite() && delay > 0.0 {
                        delay as u64
                    } else {
                        0
                    };
                    runtime.sleep(delay_ms).await?;
                    request = config;
                }
            }
        }
    })
}

/// Axios' `promise.then(onFulfilled, onRejected)` link: a missing handler passes
/// the settlement through; a rejection handler's return recovers the chain.
async fn chain_step(
    runtime: &Rc<dyn ExchangeRuntime>,
    interceptor: &Interceptor,
    current: Result<Js, Thrown>,
) -> Result<Js, Thrown> {
    let (on_fulfilled, on_rejected) = interceptor;
    match current {
        Ok(value) if on_fulfilled.is_callable() => {
            await_js(runtime, on_fulfilled.call(Js::Undefined, vec![value])?).await
        }
        Err(error) if on_rejected.is_callable() => {
            await_js(runtime, on_rejected.call(Js::Undefined, vec![error.value])?).await
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
        current = chain_step(&host.runtime(), interceptor, current).await;
    }
    current = match current {
        Ok(config) => dispatch_request(host, config).await,
        Err(error) => Err(error),
    };
    let responses: Vec<Interceptor> = state.borrow().responses.clone();
    for interceptor in &responses {
        current = chain_step(&host.runtime(), interceptor, current).await;
    }
    current
}

async fn dispatch_request(host: &Host, config: Js) -> Result<Js, Thrown> {
    let adapter = config.get("adapter")?;
    let custom_adapter = adapter.is_callable();
    let response = if custom_adapter {
        await_js(
            &host.runtime(),
            adapter.call(Js::Undefined, vec![config.clone()])?,
        )
        .await?
    } else {
        host.send(config.clone()).await?
    };
    if !response.get("config")?.is_object() {
        response.define("config", config.clone())?;
    }
    transform_response(&response, &config)?;
    // Axios custom adapters settle their own responses. HOW deliberately
    // fulfills every HTTP status; only the default HTTP host settles here.
    if !custom_adapter {
        let status = response.get("status")?.to_number().unwrap_or(200.0);
        if !(200.0..300.0).contains(&status) {
            return Err(axios_error(&response, &config)?);
        }
    }
    Ok(response)
}

fn transform_response(response: &Js, config: &Js) -> Result<(), Thrown> {
    let transforms = config.get("transformResponse")?;
    if transforms.is_nullish() {
        return Ok(());
    }
    let transforms = transforms
        .array_values()?
        .unwrap_or_else(|| vec![transforms]);
    let mut data = response.get("data")?;
    for transform in transforms {
        data = transform.call(
            config.clone(),
            vec![data, response.get("headers")?, response.get("status")?],
        )?;
    }
    response.set("data", data)
}

async fn throttle(
    host: &Host,
    state: &Rc<RefCell<ClientState>>,
    index: usize,
) -> Result<(), Thrown> {
    loop {
        let runtime = host.runtime();
        let now = runtime.monotonic_now_ms();
        let wait = {
            let mut state = state.borrow_mut();
            let (max, per, sent) = &mut state.windows[index];
            if *max <= 0.0 {
                None
            } else {
                sent.retain(|time| now - time < *per);
                if sent.len() as f64 >= *max {
                    Some(*per - (now - sent[0]))
                } else {
                    sent.push(now);
                    None
                }
            }
        };
        match wait {
            None => return Ok(()),
            // A failed host sleep rejects the request instead of spinning on the window.
            Some(ms) => runtime.sleep(ms.max(1.0) as u64).await?,
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
        body: RestResponseBody::Untransformed(to_retained_input(&response.get("data")?)?),
    })
}

/// A JS Axios client as a typed `RestAdapter`: requests run the client's whole
/// pipeline (interceptors, signing, retry). A rejection keeps its JS value so
/// [`JsRestAdapter::thrown`] can rethrow it unchanged.
pub struct JsRestAdapter {
    runtime: Rc<dyn ExchangeRuntime>,
    xhr: Js,
    rejection: RefCell<Option<Thrown>>,
}

impl JsRestAdapter {
    pub fn new(runtime: Rc<dyn ExchangeRuntime>, xhr: Js) -> Self {
        Self {
            runtime,
            xhr,
            rejection: RefCell::default(),
        }
    }
    async fn send(&self, request: &RestRequest) -> Result<RestResponse, Thrown> {
        let config = Js::object(vec![]);
        for (name, value) in &request.config_extensions {
            config.define(name, from_retained_input(value.clone()))?;
        }
        // Typed parameters remain authoritative over an extension of that name.
        if let Some(params) = &request.params {
            config.define("params", from_json(params))?;
        }
        let url = Js::str(&request.url);
        // Axios normalizes the verb; RestRequest callers send "GET" as often as "get".
        let method = request.method.to_lowercase();
        let pending = if method == "post" {
            let data = match &request.body {
                Some(RestBody::Untransformed(value)) => from_retained_input(value.clone()),
                Some(RestBody::Transformed(data)) => from_retained_input(data.value().clone()),
                Some(RestBody::Encoded(bytes)) => {
                    let text = String::from_utf8_lossy(bytes);
                    safe_cex_types::retained_json::RetainedJsonInput::parse_json(
                        &text.encode_utf16().collect::<Vec<_>>(),
                    )
                    .map(from_retained_input)
                    .unwrap_or_else(|_| Js::str(&text))
                }
                None => Js::Undefined,
            };
            self.xhr.call_method("post", vec![url, data, config])?
        } else {
            self.xhr.call_method(&method, vec![url, config])?
        };
        rest_response(&await_js(&self.runtime, pending).await?)
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
                    let code = optional(&thrown.value, "code").ok().and_then(|value| {
                        if let Js::String(units) = value {
                            Some(units)
                        } else {
                            None
                        }
                    });
                    let message = thrown.message();
                    // RestFailure.code is the existing UTF-8 diagnostic view.
                    // The owned cause is authoritative for the native string.
                    let mut fields = vec![(
                        "message",
                        super::value::owned::OwnedJsValue::string(&message),
                    )];
                    if let Some(code) = &code {
                        fields.push((
                            "code",
                            super::value::owned::OwnedJsValue::String(code.to_vec()),
                        ));
                    }
                    let cause = super::value::owned::OwnedCallbackError::new(
                        super::value::owned::OwnedJsValue::object(fields),
                    );
                    let failure = RestFailure {
                        code: code.as_ref().map(|units| String::from_utf16_lossy(units)),
                        message,
                        transport_error: Some(Box::new(
                            safe_cex_transport::TransportError::Rejected(
                                safe_cex_transport::provider::TransportErrorOrigin::new(cause),
                            ),
                        )),
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
    remote: Option<super::base_exchange::HostedSigner>,
    exchange: RefCell<Option<safe_cex_types::ExchangeName>>,
}

pub fn rest_http_host(
    runtime: Rc<dyn ExchangeRuntime>,
    adapter: std::sync::Arc<dyn RestAdapter>,
) -> Rc<dyn JsHttpHost> {
    rest_http_host_signed(runtime, adapter, None)
}

/// The production wire host with one hosted account's signer. Scope assembly
/// supplies the exchange name before any client sends a request.
pub fn rest_http_host_signed(
    runtime: Rc<dyn ExchangeRuntime>,
    adapter: std::sync::Arc<dyn RestAdapter>,
    remote: Option<super::base_exchange::HostedSigner>,
) -> Rc<dyn JsHttpHost> {
    Rc::new(RestHttpHost {
        runtime,
        adapter,
        remote,
        exchange: RefCell::new(None),
    })
}

struct TransportSigner(Rc<dyn crate::runtime::RemoteSigner>);
impl safe_cex_transport::signing::RemoteSigner for TransportSigner {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse>
    {
        self.0.sign(request)
    }
    fn sign_query(
        &self,
        request: safe_cex_transport::query_signing::HttpSigningRequest,
    ) -> safe_cex_transport::TransportFuture<
        '_,
        safe_cex_transport::query_signing::HttpSigningResponse,
    > {
        self.0.sign_query(request)
    }
}

/// Adapt the local availability-aware signer to the transport's Arc capability.
#[allow(clippy::arc_with_non_send_sync)]
pub fn transport_signer(
    signer: Rc<dyn crate::runtime::RemoteSigner>,
) -> std::sync::Arc<dyn safe_cex_transport::signing::RemoteSigner> {
    std::sync::Arc::new(TransportSigner(signer))
}

#[allow(clippy::arc_with_non_send_sync)]
fn signed_adapter(
    runtime: Rc<dyn ExchangeRuntime>,
    adapter: std::sync::Arc<dyn RestAdapter>,
    remote: &super::base_exchange::HostedSigner,
    exchange: safe_cex_types::ExchangeName,
    serializer: Option<&crate::http::ParamsSerializer>,
) -> std::sync::Arc<dyn RestAdapter> {
    let clock = runtime.clone();
    std::sync::Arc::new(safe_cex_transport::signing::create_remote_signing_adapter(
        transport_signer(remote.0.clone()),
        exchange,
        remote.1.clone(),
        std::sync::Arc::new(move || runtime.unique_id()),
        std::sync::Arc::new(move || clock.now_ms().max(0.0) as u64),
        crate::http::remote_output_adapter(adapter, serializer),
    ))
}

fn optional_text(config: &Js, key: &str) -> Result<Option<String>, Thrown> {
    let value = config.get(key)?;
    Ok(if value.is_nullish() {
        None
    } else {
        Some(String::from_utf16_lossy(&value.to_js_string()?))
    })
}

fn js_params_serializer(config: &Js) -> Result<Js, Thrown> {
    let serializer = config.get("paramsSerializer")?;
    if serializer.is_callable() {
        Ok(serializer)
    } else if serializer.is_object() {
        super::scope::bound_member(&serializer, "serialize")
    } else {
        Ok(Js::Undefined)
    }
}

#[allow(clippy::arc_with_non_send_sync)]
fn hosted_params_serializer(config: &Js) -> Result<Option<crate::http::ParamsSerializer>, Thrown> {
    let serialize = js_params_serializer(config)?;
    Ok(serialize.is_callable().then(|| {
        std::sync::Arc::new(move |params: &serde_json::Value| {
            serialize
                .call(Js::Undefined, vec![from_json(params)])
                .and_then(|query| query.to_js_string())
                .map(|query| String::from_utf16_lossy(&query))
                .map_err(|error| safe_cex_transport::TransportError::Host(error.message()))
        }) as crate::http::ParamsSerializer
    }))
}

fn rest_request(config: &Js, serialize_query: bool) -> Result<RestRequest, Thrown> {
    rest_request_data(config, serialize_query, false)
}

fn rest_request_data(
    config: &Js,
    serialize_query: bool,
    before_signing: bool,
) -> Result<RestRequest, Thrown> {
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
        // A JS string is still typed adapter data at hosted-signing ingress.
        // Only the direct terminal path has finished with that provenance.
        Js::String(_) if before_signing => Some(RestBody::Untransformed(
            super::value::to_retained_input(&data)?,
        )),
        Js::String(text) => Some(RestBody::Encoded(
            String::from_utf16_lossy(text).into_bytes(),
        )),
        other => Some(RestBody::Untransformed(super::value::to_retained_input(
            other,
        )?)),
    };
    let mut url = optional_text(config, "url")?.unwrap_or_default();
    let mut params = config.get("params")?;
    // Axios: a configured paramsSerializer (function or `{ serialize }`) owns the query.
    let serialize = js_params_serializer(config)?;
    if serialize_query && !params.is_nullish() {
        let query = if serialize.is_callable() {
            let query = serialize.call(Js::Undefined, vec![params])?;
            String::from_utf16_lossy(&query.to_js_string()?)
        } else {
            safe_cex_transport::query::axios_default_query(&super::value::to_js_json(&params)?)
                .map_err(|error| match error {
                    safe_cex_transport::TransportError::JavaScript(error) => {
                        Thrown::error(error.name, error.message)
                    }
                    error => Thrown::error("AxiosError", &error.to_string()),
                })?
        };
        if !query.is_empty() {
            if let Some(hash) = url.find('#') {
                url.truncate(hash);
            }
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str(&query);
        }
        params = Js::Undefined;
    }
    // Keep adapter-owned data properties without serializing executable Axios
    // capabilities. The source config itself stays live for response hooks.
    let mut config_extensions = indexmap::IndexMap::new();
    for (key, value) in config.own_entries()? {
        let key = String::from_utf16(&key)
            .map_err(|_| Thrown::type_error("lone UTF-16 surrogate at a typed boundary"))?;
        if matches!(
            key.as_str(),
            "method"
                | "url"
                | "baseURL"
                | "headers"
                | "data"
                | "params"
                | "timeout"
                | "adapter"
                | "paramsSerializer"
                | "transformRequest"
                | "transformResponse"
                | "safeCexSkipRemoteSigning"
        ) || value.is_callable()
        {
            continue;
        }
        config_extensions.insert(key, to_retained_input(&value)?);
    }
    let timeout = config.get("timeout")?;
    // ts: markSkipRemoteSigning tags a public request a hosted signer need not see.
    let skip_remote_signing = config.get("safeCexSkipRemoteSigning")?.truthy();
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
        skip_remote_signing,
        config_extensions,
        ..Default::default()
    })
}

/// Axios' default transformResponse: JSON text parses, anything else stays text.
fn response_data(body: RestResponseBody) -> Js {
    match body {
        RestResponseBody::Encoded(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            safe_cex_types::retained_json::RetainedJsonInput::parse_json(
                &text.encode_utf16().collect::<Vec<_>>(),
            )
            .map(super::value::from_retained_input)
            .unwrap_or_else(|_| Js::str(&text))
        }
        RestResponseBody::Untransformed(value) | RestResponseBody::Transformed(value) => {
            super::value::from_retained_input(value)
        }
    }
}

fn effective_config(config: &Js, request: RestRequest) -> Result<Js, Thrown> {
    let result = config.spread_object()?;
    for (name, value) in &request.config_extensions {
        result.set(name, from_retained_input(value.clone()))?;
    }
    result.set("method", Js::str(&request.method))?;
    result.set("url", Js::str(&request.url))?;
    result.set(
        "baseURL",
        request.base_url.as_deref().map_or(Js::Undefined, Js::str),
    )?;
    let headers = Js::object(vec![]);
    for (name, value) in request.headers {
        headers.define(&name, Js::str(&value))?;
    }
    result.set("headers", headers)?;
    result.set(
        "params",
        request.params.as_ref().map_or(Js::Undefined, from_json),
    )?;
    let data = match request.body {
        None => Js::Undefined,
        Some(RestBody::Untransformed(value)) => super::value::from_retained_input(value),
        Some(RestBody::Transformed(value)) => {
            super::value::from_retained_input(value.value().clone())
        }
        Some(RestBody::Encoded(bytes)) => Js::str(&String::from_utf8_lossy(&bytes)),
    };
    result.set("data", data)?;
    Ok(result)
}

fn js_response(response: RestResponse, config: Js) -> Js {
    let headers = Js::object(vec![]);
    for (name, value) in &response.headers {
        let _ = headers.define(name, Js::str(value));
    }
    // Venue-specific bigint parsers must see the original tokens before the
    // ordinary Axios JSON projection rounds them through binary64.
    let data = match response.body {
        RestResponseBody::Encoded(bytes)
            if config
                .get("responseType")
                .is_ok_and(|value| value.to_display() == "text")
                || config
                    .get("transformResponse")
                    .is_ok_and(|value| !value.is_undefined()) =>
        {
            Js::str(&String::from_utf8_lossy(&bytes))
        }
        body => response_data(body),
    };
    Js::object(vec![
        ("data", data),
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
    fn configure_exchange(&self, name: safe_cex_types::ExchangeName) -> Result<(), Thrown> {
        if self.remote.is_none() {
            return Ok(());
        }
        let mut exchange = self.exchange.borrow_mut();
        if exchange.as_ref().is_some_and(|current| current != &name) {
            return Err(Thrown::error(
                "Error",
                "HTTP host already belongs to another exchange",
            ));
        }
        *exchange = Some(name);
        Ok(())
    }
    fn hosted_signer(&self) -> Option<super::base_exchange::HostedSigner> {
        self.remote.clone()
    }
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let adapter = self.adapter.clone();
        let remote = self.remote.clone();
        let exchange = self.exchange.borrow().clone();
        let runtime = self.runtime.clone();
        Box::pin(async move {
            let adapter = if let Some(remote) = &remote {
                signed_adapter(
                    runtime,
                    adapter,
                    remote,
                    exchange.ok_or_else(|| {
                        Thrown::error("Error", "hosted HTTP host has no exchange")
                    })?,
                    hosted_params_serializer(&config)?.as_ref(),
                )
            } else {
                adapter
            };
            let request = rest_request_data(
                &config,
                remote.is_none() && adapter.query_in_url(),
                remote.is_some(),
            )?;
            match adapter.execute_with_receipt(request).await {
                Ok(execution) => {
                    let config = if remote.is_some() {
                        effective_config(&config, execution.config)?
                    } else {
                        config
                    };
                    Ok(js_response(execution.response, config))
                }
                // A status failure still carries the response; the client classifies it.
                Err(RestFailure {
                    response: Some(response),
                    request,
                    ..
                }) => {
                    let config = if remote.is_some() {
                        effective_config(&config, *request)?
                    } else {
                        config
                    };
                    Ok(js_response(*response, config))
                }
                Err(failure) => {
                    if let Some(safe_cex_transport::TransportError::Rejected(origin)) =
                        failure.transport_error.as_deref()
                        && let Some(cause) = origin
                            .error()
                            .downcast_ref::<super::value::owned::OwnedCallbackError>()
                    {
                        return Err(Thrown::new(cause.value().to_js()));
                    }
                    let config = if remote.is_some() {
                        effective_config(&config, *failure.request)?
                    } else {
                        config
                    };
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
