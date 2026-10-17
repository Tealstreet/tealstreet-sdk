//! MEXC API composition over the shared HTTP, signing and classified retry owners.
use crate::{
    api::*,
    types::{BASE_URL, RECV_WINDOW},
};
use safe_cex_exchange::http::{HttpClient, HttpClientOptions, safe_cex_axios_create};
use safe_cex_num::{JsJsonValue, js_number_string, js_string, js_truthy};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse, TransportError,
    TransportRegistry,
    rate_limit::RateLimitClock,
    retry::{ClassifiedRequest, ReadClassification, only_reads},
};
use safe_cex_types::{ExchangeOptions, QueryValue};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

struct ResponseTransform {
    inner: Arc<dyn RestAdapter>,
}
fn transform_response(response: &mut RestResponse) {
    response.body = response.body.clone().transform(|data| match data {
        safe_cex_types::retained_json::RetainedJsonInput::Text(text) => {
            safe_cex_transport::how::big_int_as_string_json_parse_utf16(&text)
        }
        data => data,
    });
}
impl RestAdapter for ResponseTransform {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(
        &self,
        request: RestRequest,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            match self.inner.execute_with_receipt(request).await {
                Ok(mut execution) => {
                    transform_response(&mut execution.response);
                    Ok(execution)
                }
                Err(mut error) => {
                    if let Some(response) = error.response.as_mut() {
                        transform_response(response)
                    };
                    Err(error)
                }
            }
        })
    }
}
// Preserve the native query tree for signing; JSON projection belongs to wire owners.
fn signing_query(value: &QueryValue) -> JsJsonValue {
    match value {
        QueryValue::Undefined => JsJsonValue::Undefined,
        QueryValue::Null => JsJsonValue::Null,
        QueryValue::Bool(value) => JsJsonValue::Bool(*value),
        QueryValue::Number(value) => JsJsonValue::Number(*value),
        QueryValue::String(value) => JsJsonValue::String(value.clone()),
        QueryValue::Array(values) => JsJsonValue::Array(values.iter().map(signing_query).collect()),
        QueryValue::Object(values) => JsJsonValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), signing_query(value)))
                .collect(),
        ),
    }
}
struct MexcAttempt {
    client: HttpClient,
    options: ExchangeOptions,
    remote: bool,
    current_time_ms: Rc<dyn Fn() -> f64>,
}
impl RestAdapter for MexcAttempt {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(
        &self,
        mut request: RestRequest,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            if is_mexc_public_endpoint(Some(&request.url)) {
                request.skip_remote_signing = true;
            } else if self.options.public_data_only != Some(true)
                && !self.remote
                && !self.options.key.is_empty()
                && !self.options.secret.is_empty()
            {
                let time = js_number_string((self.current_time_ms)());
                let is_get = request.method.eq_ignore_ascii_case("GET");
                let body = if is_get {
                    String::new()
                } else {
                    let data = request
                        .body
                        .as_ref()
                        .map(RestBody::untransformed)
                        .transpose()
                        .map_err(|e| RestFailure::from_transport(e, request.clone()))?;
                    normalize_request_data(
                        &safe_cex_exchange::js_runtime::value::from_retained_input(
                            data.cloned().unwrap_or(
                                safe_cex_types::retained_json::RetainedJsonInput::Undefined,
                            ),
                        ),
                    )
                    .map_err(|e| RestFailure::pre_transmit(e.message(), request.clone()))?
                };
                let param = if is_get {
                    get_query_string(
                        &request
                            .query_params
                            .as_ref()
                            .map(signing_query)
                            .or_else(|| request.params.clone().map(JsJsonValue::from))
                            .unwrap_or(JsJsonValue::Undefined),
                        Some(&request.url),
                    )
                    .map_err(|e| RestFailure::pre_transmit(e.message.into(), request.clone()))?
                } else {
                    body.clone()
                };
                let signature =
                    sign_mexc_request(&self.options.secret, &self.options.key, &time, &param)
                        .map_err(|e| RestFailure::pre_transmit(e, request.clone()))?;
                if !body.is_empty() {
                    request.body = Some(match request.body.as_ref() {
                        Some(previous) => previous
                            .with_signed_data(
                                safe_cex_types::retained_json::RetainedJsonInput::Text(
                                    body.encode_utf16().collect(),
                                ),
                            )
                            .map_err(|e| RestFailure::from_transport(e, request.clone()))?,
                        None => RestBody::Untransformed(
                            safe_cex_types::retained_json::RetainedJsonInput::Text(
                                body.encode_utf16().collect(),
                            ),
                        ),
                    });
                }
                let recv = self
                    .options
                    .extra
                    .as_ref()
                    .and_then(|extra| extra.recv_window)
                    .unwrap_or(RECV_WINDOW as f64);
                // Shared HttpClient merges AxiosHeaders case aliases after these
                // source signing-header assignments.
                for (key, value) in [
                    ("ApiKey", self.options.key.clone()),
                    ("Request-Time", time),
                    ("Signature", signature),
                    ("Recv-Window", js_number_string(recv)),
                    ("Content-Type", "application/json".into()),
                ] {
                    request.headers.insert(key.into(), value);
                }
            }
            self.client.execute_with_receipt(request).await
        })
    }
}
fn response_data(response: &RestResponse) -> Value {
    response.body.json_value().unwrap_or(Value::Null)
}
fn coercion_failure(error: safe_cex_num::JsCoercionError, request: &RestRequest) -> RestFailure {
    RestFailure::pre_transmit(error.message.into(), request.clone())
}
// Source response interceptor after its signature-resend check.
fn reject_envelope(
    execution: safe_cex_transport::RestExecution,
) -> Result<safe_cex_transport::RestExecution, RestFailure> {
    let data = response_data(&execution.response);
    if data.is_object() || data.is_array() {
        let code = data.get("code");
        let rejected = if data.get("success") == Some(&Value::Bool(false)) {
            true
        } else {
            code.map(|v| js_string(Some(v)))
                .transpose()
                .map_err(|e| coercion_failure(e, &execution.config))?
                .is_some_and(|s| s != "0" && s != "200")
        };
        let message = data
            .get("message")
            .filter(|v| js_truthy(Some(v)))
            .or_else(|| data.get("msg"));
        if rejected && js_truthy(message) {
            let text = js_string(message).map_err(|e| coercion_failure(e, &execution.config))?;
            let mut error = RestFailure::pre_transmit(text, execution.config);
            error.response = Some(Box::new(execution.response));
            return Err(error);
        }
    }
    Ok(execution)
}
// ts: signatureRejectedResponse(isMexcSignatureRejection), stamping the one resend.
fn rejected_signature(error: &RestFailure, retried: &Cell<bool>) -> Result<bool, RestFailure> {
    let Some(response) = error.response.as_deref() else {
        return Ok(false);
    };
    if retried.get()
        || !is_mexc_signature_rejection(&response_data(response))
            .map_err(|e| coercion_failure(e, &error.request))?
    {
        return Ok(false);
    }
    retried.set(true);
    Ok(true)
}
struct MexcApi {
    attempt: MexcAttempt,
    clock: Rc<dyn RateLimitClock>,
}
impl RestAdapter for MexcApi {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(
        &self,
        mut request: RestRequest,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            request.method = if request.method.is_empty() {
                "get".into()
            } else {
                request.method.to_ascii_lowercase()
            };
            if request.timeout_ms.is_none() {
                request.timeout_ms = Some(20000);
            }
            self.request(request, Rc::new(Cell::new(false))).await
        })
    }
}
impl MexcApi {
    // ts: xhr.request — axios-retry, then the response interceptor's single re-signed resend.
    fn request(
        &self,
        request: RestRequest,
        retried: Rc<Cell<bool>>,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            let execution = self.retrying(request, retried.clone()).await?;
            if !retried.get()
                && is_mexc_signature_rejection(&response_data(&execution.response))
                    .map_err(|e| coercion_failure(e, &execution.config))?
            {
                retried.set(true);
                return self.request(execution.config, retried).await;
            }
            reject_envelope(execution)
        })
    }
    async fn retrying(
        &self,
        request: RestRequest,
        retried: Rc<Cell<bool>>,
    ) -> Result<safe_cex_transport::RestExecution, RestFailure> {
        let timeout = request.timeout_ms;
        match ReadClassification::Methods.classify(request) {
            ClassifiedRequest::Read(read) => {
                let failure = Rc::new(RefCell::new(None));
                let condition_failure = failure.clone();
                let result = only_reads(read, 3, move |_, result| {
                    let Err(error) = result else {
                        return false;
                    };
                    if error.is_retryable_network() {
                        return true;
                    }
                    rejected_signature(error, &retried).unwrap_or_else(|cause| {
                        *condition_failure.borrow_mut() = Some(cause);
                        false
                    })
                })
                .execute_with_receipt(&self.attempt, self.clock.as_ref())
                .await;
                match failure.take() {
                    Some(cause) => Err(cause),
                    None => result,
                }
            }
            ClassifiedRequest::Write(write) => {
                let started = self.clock.now_ms();
                let error = match write.execute_with_receipt(&self.attempt).await {
                    Err(error) if rejected_signature(&error, &retried)? => error,
                    result => return result,
                };
                // Same axios-retry budget as the read policy: shouldResetTimeout=false.
                let mut next = (*error.request).clone();
                next.timeout_ms = timeout;
                if let Some(timeout) = timeout.filter(|timeout| *timeout != 0)
                    && started != 0
                {
                    let remaining = i128::from(timeout)
                        - (i128::from(self.clock.now_ms()) - i128::from(started));
                    if remaining <= 0 {
                        return Err(error);
                    }
                    next.timeout_ms = Some(remaining.min(i128::from(i64::MAX)) as i64);
                }
                self.clock
                    .sleep_ms(0)
                    .await
                    .map_err(|cause| RestFailure::from_transport(cause, next.clone()))?;
                self.attempt.execute_with_receipt(next).await
            }
        }
    }
}
// ts: createAPI
#[expect(clippy::arc_with_non_send_sync)]
pub fn create_api(
    mut host: HttpClientOptions,
    registry: &TransportRegistry,
    clock: Rc<dyn RateLimitClock>,
    current_time_ms: Rc<dyn Fn() -> f64>,
) -> Result<impl RestAdapter + use<>, TransportError> {
    let options = host.options.clone();
    let remote = host.signer.is_some();
    let network = if options.testnet == Some(true) {
        "testnet"
    } else {
        "livenet"
    };
    host.request.base_url = Some(
        BASE_URL
            .get(network)
            .and_then(Value::as_str)
            .ok_or_else(|| TransportError::Host("Missing MEXC base URL table entry".into()))?
            .into(),
    );
    host.request
        .headers
        .insert("Content-Type".into(), "application/json".into());
    host.request.query_profile = Some(safe_cex_transport::query::QueryProfile {
        arrays: safe_cex_transport::query::QueryArrayFormat::Repeat,
        encode: true,
        key_order: safe_cex_transport::query::QueryKeyOrder::JsRoot,
    });
    host.params_serializer = Some(Arc::new(|params| {
        serialize_mexc_params(&JsJsonValue::from(params.clone()))
            .map_err(|e| TransportError::Host(e.message.into()))
    }));
    host.adapter = Arc::new(ResponseTransform {
        inner: host.adapter,
    });
    let client = safe_cex_axios_create(host, registry)?;
    Ok(MexcApi {
        attempt: MexcAttempt {
            client,
            options,
            remote,
            current_time_ms,
        },
        clock,
    })
}
