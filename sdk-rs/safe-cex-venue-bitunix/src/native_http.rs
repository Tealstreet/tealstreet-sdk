//! createAPI lanes: shared HTTP client plus the ported Bitunix sign/receive interceptors.
use safe_cex_exchange::{
    error_reporting::{ExchangeErrorInput, append_http_failure},
    exchange::BaseExchange,
    http::HttpClient,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_num::{JsJsonValue, js_json_stringify, js_number_string};
use safe_cex_transport::{RestAdapter, RestBody, RestRequest, RestResponseBody};
use safe_cex_types::ExchangeOptions;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

pub(crate) type BuildHttp = Rc<dyn Fn(&ExchangeOptions) -> Result<HttpClient, ExchangeError>>;
pub(crate) type InstallTransport = Rc<dyn Fn(&HttpClient) -> Result<(), ExchangeError>>;
type SignFn = fn(
    &Value,
    &Value,
    &mut dyn FnMut(&Value, &'static str, &'static str) -> Result<String, String>,
    &mut dyn FnMut() -> Result<f64, String>,
    &mut dyn FnMut() -> Result<String, String>,
) -> Result<Option<JsJsonValue>, String>;
type ReceiveFn = for<'a> fn(
    &'a Value,
    &mut dyn FnMut(&str) -> Result<Option<Value>, String>,
    &mut dyn FnMut(String, &Value) -> Result<(), String>,
    &mut dyn FnMut(&Value) -> Result<Option<Value>, String>,
    &mut dyn FnMut(&Value) -> bool,
) -> Result<crate::api::Received<'a>, String>;

/// One createAPI client: axios defaults, retry budget and the two interceptors.
pub(crate) struct Lane {
    pub(crate) http: Rc<HttpClient>,
    executor: RefCell<Arc<dyn RestAdapter>>,
    pub(crate) options: ExchangeOptions,
    base_url: String,
    timeout: f64,
    headers: Vec<(String, String)>,
    retries: Cell<u32>,
    rate_limits: RefCell<Vec<Rc<safe_cex_transport::rate_limit::RateLimiter>>>,
    sign: Cell<Option<SignFn>>,
    receive: Cell<Option<ReceiveFn>>,
}
fn wire(value: &JsJsonValue) -> Result<Value, ExchangeError> {
    let text = js_json_stringify(value).map_err(|e| ExchangeError::new(e.message))?;
    text.map_or(Ok(Value::Null), |s| {
        serde_json::from_str(&s).map_err(|e| ExchangeError::new(e.to_string()))
    })
}
fn header_text(value: &JsJsonValue) -> Result<String, ExchangeError> {
    Ok(match value {
        JsJsonValue::String(s) => s.clone(),
        JsJsonValue::Number(n) => js_number_string(*n),
        JsJsonValue::Bool(b) => b.to_string(),
        JsJsonValue::Undefined | JsJsonValue::Null => String::new(),
        other => wire(other)?.to_string(),
    })
}
fn field(config: &JsJsonValue, key: &str) -> Option<JsJsonValue> {
    match config {
        JsJsonValue::Object(fields) => fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone()),
        _ => None,
    }
}
struct HttpExecution {
    inner: Rc<HttpClient>,
}
impl RestAdapter for HttpExecution {
    fn execute(
        &self,
        request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(self.inner.execute(request))
    }
    fn execute_with_receipt(
        &self,
        request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(self.inner.execute_with_receipt(request))
    }
}
struct RateClock {
    runtime: Rc<dyn ExchangeRuntime>,
}
impl safe_cex_transport::rate_limit::RateLimitClock for RateClock {
    fn now_ms(&self) -> u64 {
        self.runtime.monotonic_now_ms() as u64
    }
    fn sleep_ms(&self, ms: u64) -> safe_cex_transport::TransportFuture<'_, ()> {
        let sleep = self.runtime.sleep(ms);
        Box::pin(async move {
            sleep
                .await
                .map_err(|e| safe_cex_transport::TransportError::Host(e.message))
        })
    }
}
impl Lane {
    /// createAPI(options, exchange) over the host-built shared client.
    #[expect(clippy::arc_with_non_send_sync)]
    pub(crate) fn create(
        options: ExchangeOptions,
        build: &BuildHttp,
        install: &InstallTransport,
    ) -> Result<Rc<Self>, ExchangeError> {
        let testnet = options.testnet == Some(true);
        let public_only = options.public_data_only == Some(true);
        crate::api::create_api(
            testnet,
            public_only,
            |config| {
                let base_url = match field(&config, "baseURL") {
                    Some(JsJsonValue::String(s)) => s,
                    _ => return Err(ExchangeError::new("createAPI baseURL must be a string")),
                };
                let timeout = match field(&config, "timeout") {
                    Some(JsJsonValue::Number(n)) => n,
                    _ => return Err(ExchangeError::new("createAPI timeout must be a number")),
                };
                let headers = match field(&config, "headers") {
                    Some(JsJsonValue::Object(h)) => h
                        .iter()
                        .map(|(k, v)| Ok((k.clone(), header_text(v)?)))
                        .collect::<Result<Vec<_>, ExchangeError>>()?,
                    _ => vec![],
                };
                let http = Rc::new(build(&options)?);
                Ok(Rc::new(Lane {
                    executor: RefCell::new(Arc::new(HttpExecution {
                        inner: http.clone(),
                    })),
                    http,
                    options: options.clone(),
                    base_url,
                    timeout,
                    headers,
                    retries: Cell::new(0),
                    rate_limits: RefCell::new(vec![]),
                    sign: Cell::new(None),
                    receive: Cell::new(None),
                }))
            },
            // The request loop evaluates both retry arms itself.
            |lane, _condition, retries| {
                lane.retries.set(retries as u32);
                Ok(())
            },
            |lane| install(&lane.http),
            |lane, handler| {
                lane.sign.set(Some(handler));
                Ok(())
            },
            |lane, handler| {
                lane.receive.set(Some(handler));
                Ok(())
            },
        )
    }
    /// axios-rate-limit maxRPS over this lane's executor.
    #[expect(clippy::arc_with_non_send_sync)]
    pub(crate) fn limit(&self, runtime: Rc<dyn ExchangeRuntime>, max_rps: usize) {
        self.rate_limits.borrow_mut().push(Rc::new(
            safe_cex_transport::rate_limit::RateLimiter::new(
                Arc::new(RateClock { runtime }),
                max_rps,
                1000,
            ),
        ));
    }
}

/// createXhrWithSecondaryKey: GET reads use the secondary-key lane; writes stay primary.
pub(crate) struct Client {
    pub(crate) primary: Rc<Lane>,
    pub(crate) secondary: Rc<Lane>,
    pub(crate) base: Rc<BaseExchange>,
}
fn query_value(v: &Value) -> safe_cex_types::QueryValue {
    use safe_cex_types::QueryValue as Q;
    match v {
        Value::Null => Q::Null,
        Value::Bool(b) => Q::Bool(*b),
        Value::Number(n) => Q::Number(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => Q::String(s.clone()),
        Value::Array(a) => Q::Array(a.iter().map(query_value).collect()),
        Value::Object(o) => Q::Object(o.iter().map(|(k, v)| (k.clone(), query_value(v))).collect()),
    }
}
fn query_profile(value: &Value) -> Result<String, String> {
    safe_cex_transport::query::render_query(
        &query_value(value),
        safe_cex_transport::query::QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::JsRoot,
            arrays: safe_cex_transport::query::QueryArrayFormat::Repeat,
            encode: false,
        },
    )
    .map_err(|e| e.message.to_owned())
}
fn body_value(body: RestResponseBody) -> Result<Value, ExchangeError> {
    body.transform_axios_response()
        .map_err(|error| ExchangeError::new(error.to_string()))?
        .json_value()
        .map_err(|error| ExchangeError::new(error.to_string()))
}

fn signature_rejected(data: &Value) -> Result<bool, ExchangeError> {
    crate::api::is_bitunix_signature_rejection(data).map_err(|e| ExchangeError::new(e.message))
}
impl Client {
    pub(crate) fn get(
        self: &Rc<Self>,
        url: &str,
        config: JsJsonValue,
    ) -> ExchangeFuture<'static, Value> {
        self.request("GET", url, JsJsonValue::Undefined, config)
    }
    pub(crate) fn post(
        self: &Rc<Self>,
        url: &str,
        data: JsJsonValue,
    ) -> ExchangeFuture<'static, Value> {
        self.request("POST", url, data, JsJsonValue::Object(vec![]))
    }
    /// Resolves with axios `response.data`; rejections carry axios-shaped metadata.
    pub(crate) fn request(
        self: &Rc<Self>,
        method: &'static str,
        url: &str,
        data: JsJsonValue,
        config: JsJsonValue,
    ) -> ExchangeFuture<'static, Value> {
        let client = self.clone();
        let url = url.to_owned();
        safe_cex_exchange::eager::start_eager(&self.base.runtime, async move {
            let lane = if method == "GET" {
                client.secondary.clone()
            } else {
                client.primary.clone()
            };
            let params = field(&config, "params")
                .filter(|v| !matches!(v, JsJsonValue::Undefined))
                .map(|v| wire(&v))
                .transpose()?;
            let body = (!matches!(data, JsJsonValue::Undefined))
                .then(|| wire(&data))
                .transpose()?;
            let timeout = match field(&config, "timeout") {
                Some(JsJsonValue::Number(n)) => n,
                _ => lane.timeout,
            };
            let mut headers: Vec<(String, String)> = lane.headers.clone();
            if let Some(JsJsonValue::Object(extra)) = field(&config, "headers") {
                for (k, v) in extra {
                    headers.push((k, header_text(&v)?));
                }
            }
            let flags = json!({
                "__suppressExchangeError": wire(&field(&config, "__suppressExchangeError").unwrap_or(JsJsonValue::Undefined))?,
                "__suppressExchangeRateLimitError": wire(&field(&config, "__suppressExchangeRateLimitError").unwrap_or(JsJsonValue::Undefined))?,
            });
            let mut attempt = 0;
            // The config stamp: one signature re-sign per call, by retry or by resend.
            let mut signature_retried = false;
            loop {
                let limits = lane.rate_limits.borrow().clone();
                for limit in limits.into_iter().rev() {
                    limit
                        .acquire()
                        .await
                        .map_err(|error| ExchangeError::new(error.to_string()))?;
                }
                let mut request = RestRequest {
                    method: method.into(),
                    url: url.clone(),
                    base_url: Some(lane.base_url.clone()),
                    timeout_ms: Some(timeout as i64),
                    params: params.clone(),
                    body: body
                        .clone()
                        .map(safe_cex_types::retained_json::RetainedJsonInput::from_value)
                        .transpose()
                        .map_err(|error| ExchangeError::new(error.to_string()))?
                        .map(RestBody::Untransformed),
                    ..Default::default()
                };
                let mut signed_headers = headers.clone();
                let sign = lane
                    .sign
                    .get()
                    .ok_or_else(|| ExchangeError::new("Bitunix request interceptor missing"))?;
                let config_view = json!({
                    "url": url,
                    "params": params.clone().unwrap_or(Value::Null),
                    "data": body.clone().unwrap_or(Value::Null),
                    "headers": Value::Object(headers.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect()),
                });
                let options = json!({"key": lane.options.key, "secret": lane.options.secret});
                let nonce_runtime = client.base.runtime.clone();
                let signed = sign(
                    &config_view,
                    &options,
                    &mut |p, _, _| query_profile(p),
                    &mut || Ok(safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time()),
                    &mut || Ok(nonce_runtime.unique_id().replace(['-', '_'], "")),
                )
                .map_err(ExchangeError::new)?;
                if let Some(JsJsonValue::Object(fields)) = signed {
                    signed_headers.clear();
                    for (k, v) in fields {
                        if !matches!(v, JsJsonValue::Undefined) {
                            signed_headers.push((k, header_text(&v)?));
                        }
                    }
                }
                for (k, v) in &signed_headers {
                    request.headers.insert(k.clone(), v.clone());
                }
                let executor = lane.executor.borrow().clone();
                let outcome = executor.execute(request.clone()).await;
                let response = match outcome {
                    Err(failure) => {
                        let network = safe_cex_transport::retry::is_read_only_method(&request)
                            && failure.is_retryable_network();
                        let rejected = !signature_retried
                            && match &failure.response {
                                Some(response) => {
                                    signature_rejected(&body_value(response.body.clone())?)?
                                }
                                None => false,
                            };
                        if attempt < lane.retries.get() && (network || rejected) {
                            signature_retried |= rejected;
                            attempt += 1;
                            continue;
                        }
                        let mut error = ExchangeError::from(failure);
                        error.metadata.is_axios_error = true;
                        error.metadata.config = Some(json!({"url": url, "method": method}));
                        return Err(error);
                    }
                    Ok(response) => response,
                };
                let status = response.status;
                let data = body_value(response.body)?;
                // ts: resendOnSignatureRejection in the response interceptor.
                if !signature_retried && signature_rejected(&data)? {
                    signature_retried = true;
                    continue;
                }
                let mut config_out = flags.clone();
                config_out["url"] = Value::String(url.clone());
                config_out["method"] = Value::String(method.to_lowercase());
                let view = json!({"data": data, "status": status, "config": config_out});
                let receive = lane
                    .receive
                    .get()
                    .ok_or_else(|| ExchangeError::new("Bitunix response interceptor missing"))?;
                let base = client.base.clone();
                receive(
                    &view,
                    &mut |name| {
                        Ok(signed_headers
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(name))
                            .map(|(_, v)| Value::String(v.clone())))
                    },
                    &mut |message, response| {
                        base.emit_and_log_error(
                            ExchangeErrorInput::Message(append_http_failure(
                                Some(&Value::String(message)),
                                response,
                            )),
                            None,
                            None,
                        )
                        .map_err(|e| e.message)
                    },
                    // The loop above already resent a rejection, and runs each response once.
                    &mut |_| Ok(None),
                    &mut |_| false,
                )
                .map_err(ExchangeError::new)?;
                return Ok(view["data"].clone());
            }
        })
    }
}
