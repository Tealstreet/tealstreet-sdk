//! Bybit's request, rejection, response and environment-probe composition.
use crate::{api_request::client::RequestClient, response::get_bybit_response_meta};
use safe_cex_crypto::PayloadSigner;
use safe_cex_exchange::{
    SharedOperation,
    error_reporting::ExchangeErrorInput,
    exchange::Exchange,
    http::HttpClientOptions,
    js_runtime::value::owned::{OwnedCallbackError, OwnedJsValue},
    runtime::{ExchangeError, ExchangeRuntime},
};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{
    RestAdapter, RestExecution, RestFailure, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError, TransportRegistry,
    provider::TransportErrorOrigin,
    rate_limit::RateLimitClock,
    retry::{ClassifiedRequest, ReadClassification},
};
use safe_cex_types::{ExchangeName, retained_json::RetainedJsonInput as Input};
use serde_json::{Map, Value};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub struct BybitApiOptions {
    pub http: HttpClientOptions,
    pub owner: Weak<dyn Exchange>,
    pub is_demo: bool,
    pub rest_base_url: Option<String>,
    pub clock: VirtualClock,
    pub retry_clock: Rc<dyn RateLimitClock>,
    pub payload_signer: Option<Rc<dyn PayloadSigner>>,
}

#[derive(Clone)]
pub struct BybitApi(Rc<Client>);

#[derive(Clone)]
struct Probe {
    base_url: String,
    diagnostic: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum ProbeResult {
    Mismatch(String),
    NoMismatch,
    Failed,
}
#[derive(Clone, Copy, Default)]
struct Flags {
    clock_retry: bool,
    env_probe: bool,
    // One re-signed resend per request, shared by the rejected and resolved paths.
    signature_retry: bool,
    retries_consumed: usize,
}
struct Execution {
    receipt: RestExecution,
    flags: Flags,
}
struct Client {
    request: RequestClient,
    owner: Weak<dyn Exchange>,
    label: String,
    clock: VirtualClock,
    retry_clock: Rc<dyn RateLimitClock>,
    runtime: Rc<dyn ExchangeRuntime>,
    probe: Option<Probe>,
    env_probe_operation: RefCell<Option<SharedOperation<ProbeResult>>>,
    env_probe_result: RefCell<Option<ProbeResult>>,
}

pub fn create_api(
    mut options: BybitApiOptions,
    registry: &TransportRegistry,
) -> Result<BybitApi, TransportError> {
    let network = if options.http.options.testnet == Some(true) {
        "testnet"
    } else {
        "livenet"
    };
    let name = options.http.exchange.clone();
    let label = serde_json::to_value(&name)
        .expect("exchange name")
        .as_str()
        .expect("exchange string")
        .to_owned();
    let fallback = if options.is_demo {
        &crate::types::DEMO_URL
    } else {
        &crate::types::BASE_URL
    };
    let base_url = options
        .rest_base_url
        .take()
        .unwrap_or_else(|| fallback[network].as_str().expect("Bybit base URL").into());
    let site_headers = if crate::site::is_bybit_site_id_exchange(&label) {
        crate::site::get_bybit_site_headers(
            &serde_json::to_value(&options.http.options)
                .map_err(|error| TransportError::Host(error.to_string()))?,
        )
        .as_object()
        .expect("site headers object")
        .clone()
    } else {
        Map::new()
    };
    let demo = name == ExchangeName::Bybitdemo || options.is_demo;
    let expected = if demo {
        &crate::types::DEMO_URL
    } else {
        &crate::types::BASE_URL
    };
    let can_probe = network == "livenet"
        && matches!(name, ExchangeName::Bybit | ExchangeName::Bybitdemo)
        && expected[network].as_str() == Some(base_url.as_str())
        && options.http.options.bybit_site_id.is_none()
        && options.http.signer.is_none()
        && !options.http.options.key.is_empty()
        && !options.http.options.secret.is_empty();
    let probe = can_probe.then(|| opposite_probe(demo, network));
    options.http.request.base_url = Some(base_url);
    let runtime = options.http.runtime.clone();
    let request = RequestClient::new(
        options.http,
        registry,
        site_headers,
        options.payload_signer,
        options.clock.clone(),
    )?;
    Ok(BybitApi(Rc::new(Client {
        request,
        owner: options.owner,
        label,
        clock: options.clock,
        retry_clock: options.retry_clock,
        runtime,
        probe,
        env_probe_operation: RefCell::new(None),
        env_probe_result: RefCell::new(None),
    })))
}

fn opposite_probe(demo: bool, network: &str) -> Probe {
    let (base_url, key_environment, configured_environment, remediation) = if demo {
        (
            crate::types::BASE_URL[network].as_str().expect("Bybit URL"),
            "live Bybit",
            "Bybit Demo Trading",
            "Create a Demo Trading API key or switch the account to live Bybit.",
        )
    } else {
        (
            crate::types::DEMO_URL[network]
                .as_str()
                .expect("Bybit demo URL"),
            "Bybit Demo Trading",
            "live Bybit",
            "Create a live Bybit API key or switch the account to Bybit Demo Trading.",
        )
    };
    Probe {
        base_url: base_url.into(),
        diagnostic: format!(
            "{}: this key is valid for {key_environment}, but this account is configured for {configured_environment}. {remediation}",
            crate::api::BYBIT_ENV_MISMATCH_MESSAGE_PREFIX
        ),
    }
}

impl RestAdapter for BybitApi {
    fn query_in_url(&self) -> bool {
        self.0.request.query_in_url()
    }
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|receipt| receipt.response)
        })
    }
    fn execute_with_receipt(&self, request: RestRequest) -> RestFuture<'_, RestExecution> {
        Box::pin(async move {
            self.0
                .request(request, Flags::default())
                .await
                .map(|execution| execution.receipt)
        })
    }
}

struct Attempt<'a> {
    client: &'a RequestClient,
    calls: Cell<usize>,
}
impl RestAdapter for Attempt<'_> {
    fn query_in_url(&self) -> bool {
        self.client.query_in_url()
    }
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(&self, request: RestRequest) -> RestFuture<'_, RestExecution> {
        self.calls.set(self.calls.get() + 1);
        self.client.execute_with_receipt(request)
    }
}

impl Client {
    fn request(
        self: &Rc<Self>,
        mut request: RestRequest,
        flags: Flags,
    ) -> RestFuture<'static, Execution> {
        let client = self.clone();
        request.method = if request.method.is_empty() {
            "get".into()
        } else {
            request.method.to_lowercase()
        };
        request.timeout_ms = Some(request.timeout_ms.unwrap_or(20_000));
        Box::pin(async move {
            let attempt = Attempt {
                client: &client.request,
                calls: Cell::new(0),
            };
            let signature_retry = Cell::new(flags.signature_retry);
            // axios-retry merges the request namespace over its default. Keep
            // the comparison numeric: fractional budgets admit the next integer
            // retry count, and a clock/response reentry keeps the consumed count.
            let retries = request
                .config_extensions
                .get("axios-retry")
                .cloned()
                .map(safe_cex_exchange::js_runtime::value::from_retained_input);
            let retries = match retries.filter(|state| state.has_own("retries")) {
                Some(state) => state
                    .get("retries")
                    .and_then(|value| value.to_number())
                    .map_err(|error| {
                        RestFailure::pre_transmit(
                            error.to_exchange_error().to_string(),
                            request.clone(),
                        )
                    })?,
                None => 3.0,
            };
            let receipt = retry_bybit(
                request,
                &attempt,
                retries - flags.retries_consumed as f64,
                &signature_retry,
                client.retry_clock.as_ref(),
            )
            .await?;
            let mut execution = Execution {
                receipt,
                flags: Flags {
                    retries_consumed: flags.retries_consumed + attempt.calls.get() - 1,
                    signature_retry: signature_retry.get(),
                    ..flags
                },
            };
            for _ in 0..attempt.calls.get() {
                execution = client.process_response(execution).await?;
            }
            Ok(execution)
        })
    }

    fn record_auth_failure(&self, request: &RestRequest) -> Result<(), RestFailure> {
        let owner = self.owner.upgrade().ok_or_else(|| {
            RestFailure::pre_transmit(
                "Bybit exchange owner is unavailable".into(),
                request.clone(),
            )
        })?;
        owner.base().record_auth_failure();
        Ok(())
    }
    fn record_auth_success(&self, request: &RestRequest) -> Result<(), RestFailure> {
        let owner = self.owner.upgrade().ok_or_else(|| {
            RestFailure::pre_transmit(
                "Bybit exchange owner is unavailable".into(),
                request.clone(),
            )
        })?;
        owner.base().record_auth_success();
        Ok(())
    }
    fn emit_error(&self, message: &str, request: &RestRequest) -> Result<(), RestFailure> {
        let owner = self.owner.upgrade().ok_or_else(|| {
            RestFailure::pre_transmit(
                "Bybit exchange owner is unavailable".into(),
                request.clone(),
            )
        })?;
        owner
            .base()
            .emit_and_log_error(ExchangeErrorInput::Message(message.into()), None, None)
            .map_err(|error| {
                let mut fields = vec![
                    ("name", OwnedJsValue::string(&error.name)),
                    ("message", OwnedJsValue::string(&error.message)),
                ];
                if let Some(code) = &error.code {
                    let code = match OwnedJsValue::from_js(code) {
                        Ok(code) => code,
                        Err(error) => {
                            return RestFailure::pre_transmit(error.message, request.clone());
                        }
                    };
                    fields.push(("code", code));
                }
                RestFailure::from_transport(
                    TransportError::Rejected(TransportErrorOrigin::new(OwnedCallbackError::new(
                        OwnedJsValue::object(fields),
                    ))),
                    request.clone(),
                )
            })
    }

    async fn get_env_probe_result(self: &Rc<Self>) -> Result<ProbeResult, ExchangeError> {
        let ready = self.env_probe_result.borrow().clone();
        if let Some(result) = ready {
            return Ok(result);
        }
        if self.probe.is_none() {
            *self.env_probe_result.borrow_mut() = Some(ProbeResult::NoMismatch);
            return Ok(ProbeResult::NoMismatch);
        }
        let pending = self.env_probe_operation.borrow().clone();
        let operation = if let Some(operation) = pending {
            operation
        } else {
            let client = self.clone();
            let operation = {
                let owner = self
                    .owner
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("Bybit exchange owner is unavailable"))?;
                owner.base().start_shared_operation(move |operation| {
                    *client.env_probe_operation.borrow_mut() = Some(operation);
                    async move { Ok(client.probe_opposite_environment().await) }
                })
            };
            if self.env_probe_operation.borrow().is_none() {
                *self.env_probe_operation.borrow_mut() = Some(operation.clone());
            }
            operation
        };
        let result = operation.wait().await?;
        *self.env_probe_result.borrow_mut() = Some(result.clone());
        Ok(result)
    }

    async fn probe_opposite_environment(self: &Rc<Self>) -> ProbeResult {
        let Some(probe) = &self.probe else {
            return ProbeResult::NoMismatch;
        };
        let result = self
            .request(
                RestRequest {
                    method: "get".into(),
                    base_url: Some(probe.base_url.clone()),
                    url: crate::types::ENDPOINTS["ACCOUNT"]
                        .as_str()
                        .expect("Bybit ACCOUNT")
                        .into(),
                    params: Some(Value::Object(Map::new())),
                    ..Default::default()
                },
                Flags {
                    env_probe: true,
                    clock_retry: false,
                    signature_retry: false,
                    retries_consumed: 0,
                },
            )
            .await;
        match result {
            Err(_) => ProbeResult::Failed,
            Ok(execution) => {
                let data = response_data(&execution.receipt.response.body);
                if get_bybit_response_meta(&data).is_success {
                    ProbeResult::Mismatch(probe.diagnostic.clone())
                } else {
                    ProbeResult::NoMismatch
                }
            }
        }
    }

    async fn maybe_prepend_env_mismatch(
        self: &Rc<Self>,
        code: Option<f64>,
        message: Option<Vec<u16>>,
        request: &RestRequest,
    ) -> Result<Option<Vec<u16>>, RestFailure> {
        if !matches!(code, Some(10003. | 401. | 403.)) {
            return Ok(message);
        }
        let message = match message {
            Some(message) if !message.is_empty() => message,
            other => return Ok(other),
        };
        let prefix: Vec<_> = crate::api::BYBIT_ENV_MISMATCH_MESSAGE_PREFIX
            .encode_utf16()
            .collect();
        if message.windows(prefix.len()).any(|window| window == prefix) {
            return Ok(Some(message));
        }
        match self
            .get_env_probe_result()
            .await
            .map_err(|error| RestFailure::pre_transmit(error.to_string(), request.clone()))?
        {
            ProbeResult::Mismatch(diagnostic) => {
                let mut prefixed: Vec<_> = format!("{diagnostic} [").encode_utf16().collect();
                prefixed.extend(message);
                prefixed.push(93);
                Ok(Some(prefixed))
            }
            ProbeResult::NoMismatch | ProbeResult::Failed => Ok(Some(message)),
        }
    }

    async fn process_response(
        self: &Rc<Self>,
        mut execution: Execution,
    ) -> Result<Execution, RestFailure> {
        let request = &execution.receipt.config;
        let response = &mut execution.receipt.response;
        let mut data = response_data(&response.body);
        let meta = get_bybit_response_meta(&data);
        if !meta.has_explicit_status && response.status >= 400 {
            let blocked = crate::api_request::blocked_by_cdn(&data);
            let auth = !blocked && matches!(response.status, 401 | 403);
            let message = crate::api_request::http_failure_message(
                response.status,
                response.reason.as_deref(),
                blocked,
            );
            let body = spread_data(&data);
            let message = if !execution.flags.env_probe && auth {
                self.record_auth_failure(request)?;
                self.maybe_prepend_env_mismatch(
                    Some(f64::from(response.status)),
                    Some(message.encode_utf16().collect()),
                    request,
                )
                .await?
            } else {
                Some(message.encode_utf16().collect())
            };
            let mut body = body;
            body.push((
                "retCode".encode_utf16().collect(),
                Input::Float(f64::from(response.status)),
            ));
            data = Input::Object(body);
            set_message(&mut data, message);
            response.body = RestResponseBody::Transformed(data);
            return Ok(execution);
        }
        if execution.flags.env_probe {
            return Ok(execution);
        }
        // Re-signed once before it may count as a bad key: these arrive in bursts right
        // after a reconnect, on keys that work.
        if !execution.flags.signature_retry && crate::api::is_bybit_sign_rejection(&data) {
            return self
                .request(
                    request.clone(),
                    Flags {
                        signature_retry: true,
                        ..execution.flags
                    },
                )
                .await;
        }
        if is_auth_code(meta.ret_code) {
            self.record_auth_failure(request)?;
        } else if meta.is_success
            && request
                .headers
                .get("X-BAPI-SIGN")
                .is_some_and(|value| !value.is_empty())
        {
            self.record_auth_success(request)?;
        }
        if meta.ret_code == Some(10003.) {
            let message = self
                .maybe_prepend_env_mismatch(meta.ret_code, meta.ret_msg.clone(), request)
                .await?;
            set_message(&mut data, message);
            response.body = RestResponseBody::Transformed(data.clone());
        }
        let final_meta = get_bybit_response_meta(&data);
        if data.is_truthy() && final_meta.has_explicit_status && !final_meta.is_success {
            let message = final_meta.ret_msg.as_deref().unwrap_or(&[]);
            if crate::api::is_bybit_clock_sync_ret_msg(message) {
                if !execution.flags.clock_retry {
                    let server = crate::api::parse_bybit_server_timestamp(message);
                    if let Some(server) = server.filter(|_| self.clock.runtime != "native") {
                        self.clock.sync_from_server_time(server);
                    } else {
                        self.clock.sync_bybit_client_server_time(None).await;
                    }
                    return self
                        .request(
                            request.clone(),
                            Flags {
                                clock_retry: true,
                                ..execution.flags
                            },
                        )
                        .await;
                }
                self.emit_error("Local time is out of sync after retry", request)?;
            } else if final_meta.ret_code != Some(0.) {
                let unavailable = raw_response_unavailable(&data).map_err(|error| {
                    RestFailure::from_transport(TransportError::JavaScript(error), request.clone())
                })?;
                if unavailable {
                    self.emit_error("Server was unavailable for a request, retrying...", request)?;
                }
                if !is_auth_code(meta.ret_code) {
                    self.runtime.console(
                        "error",
                        &format!(
                            "{} error {}",
                            self.label,
                            data.json_stringify()
                                .map_err(|fault| RestFailure::from_transport(
                                    TransportError::JavaScript(fault),
                                    request.clone()
                                ))?
                                .unwrap_or_else(|| "undefined".into())
                        ),
                    );
                }
            }
        }
        Ok(execution)
    }
}

fn is_auth_code(code: Option<f64>) -> bool {
    matches!(code, Some(10003. | 10004. | 10005. | 10010. | 33004.))
}
fn response_data(body: &RestResponseBody) -> Input {
    body.clone().into_data()
}

fn spread_data(data: &Input) -> Vec<(Vec<u16>, Input)> {
    match data {
        Input::Object(map) => safe_cex_types::js_coercion::js_own_keys_utf16(
            map.iter().map(|(key, _)| key.as_slice()),
        )
        .into_iter()
        .map(|key| {
            (
                key.to_vec(),
                map.iter()
                    .rev()
                    .find(|(name, _)| name == key)
                    .unwrap()
                    .1
                    .clone(),
            )
        })
        .collect(),
        Input::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| (index.to_string().encode_utf16().collect(), value.clone()))
            .collect(),
        _ => Vec::new(),
    }
}
fn set_message(data: &mut Input, message: Option<Vec<u16>>) {
    if let Some(object) = data.as_object_mut() {
        let key: Vec<_> = "retMsg".encode_utf16().collect();
        if let Some(message) = message {
            if let Some((_, value)) = object.iter_mut().rev().find(|(name, _)| name == &key) {
                *value = Input::Text(message);
            } else {
                object.push((key, Input::Text(message)));
            }
        } else {
            object.retain(|(name, _)| name != &key);
        }
    }
}

fn raw_response_unavailable(data: &Input) -> Result<bool, safe_cex_num::JsCoercionError> {
    let traffic = "There might be too much traffic or a configuration error";
    match data.get("raw_response") {
        Some(Input::Text(text)) => Ok(text
            .windows(traffic.len())
            .any(|window| window.iter().copied().eq(traffic.encode_utf16()))),
        Some(Input::Array(values)) => Ok(values.iter().any(|value| value.text_equals(traffic))),
        Some(value @ Input::Object(_))
            if value
                .get("includes")
                .is_some_and(|method| !method.is_nullish()) =>
        {
            Err(safe_cex_num::JsCoercionError {
                name: "TypeError",
                message: "response.data?.raw_response?.includes is not a function",
            })
        }
        _ => Ok(false),
    }
}

/// axios-retry with shouldRetryBybitRequest: network reads (except instruments-info)
/// retry within the budget; a rejected signature is retried once, writes included,
/// stamping `signature_retry` as signatureRejectedResponse stamps the config.
fn retry_bybit<'a>(
    request: RestRequest,
    adapter: &'a dyn RestAdapter,
    retries: f64,
    signature_retry: &'a Cell<bool>,
    clock: &'a dyn RateLimitClock,
) -> RestFuture<'a, RestExecution> {
    let classification = ReadClassification::Methods;
    // An original write is never network-retried, whatever its failure config claims.
    let original_read = matches!(
        classification.classify(request.clone()),
        ClassifiedRequest::Read(_)
    );
    let mut request = request;
    Box::pin(async move {
        for attempt in 0_usize.. {
            let started = clock.now_ms();
            let result = adapter.execute_with_receipt(request.clone()).await;
            let Err(error) = &result else {
                return result;
            };
            if (attempt as f64).partial_cmp(&retries) != Some(std::cmp::Ordering::Less) {
                return result;
            }
            let network = error.is_retryable_network()
                && original_read
                && matches!(
                    classification.classify((*error.request).clone()),
                    ClassifiedRequest::Read(_)
                );
            let retry = if network {
                error.request.url != "/v5/market/instruments-info"
            } else {
                let rejected = if !signature_retry.get() {
                    if let Some(response) = error.response.as_deref() {
                        crate::api::is_bybit_sign_rejection(&response.body.clone().into_data())
                    } else {
                        false
                    }
                } else {
                    false
                };
                if rejected {
                    signature_retry.set(true);
                }
                rejected
            };
            if !retry {
                return result;
            }
            let mut next = (*error.request).clone();
            next.timeout_ms = request.timeout_ms;
            if let Some(timeout) = request.timeout_ms.filter(|timeout| *timeout != 0)
                && started != 0
            {
                let remaining =
                    i128::from(timeout) - (i128::from(clock.now_ms()) - i128::from(started));
                if remaining <= 0 {
                    return result;
                }
                next.timeout_ms = Some(remaining.min(i128::from(i64::MAX)) as i64);
            }
            request = next;
        }
        unreachable!()
    })
}
