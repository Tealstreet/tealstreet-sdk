//! Injected transport/clock/emission keep request interceptors below BaseExchange.
use crate::{
    order_response::{
        EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE, describe_response,
        extract_hyperliquid_response_error, is_only_hyperliquid_terminal_cancel_error, js_string,
        signed_body,
    },
    rate_limit::{RateLimiter, get_hyperliquid_rest_request_weight},
    signing::{sign_l1_action, truthy},
    types::BASE_URLS,
};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse, TransportError,
    TransportFuture,
    retry::{ClassifiedRequest, ReadClassification, only_reads},
};
use safe_cex_types::ExchangeOptions;
use serde_json::Value;
use std::{rc::Rc, sync::Arc};

const RELAY_SIGNABLE_ACTION_TYPES: &[&str] = &[
    "order",
    "cancel",
    "cancelByCloid",
    "modify",
    "batchModify",
    "updateLeverage",
    "updateIsolatedMargin",
    "scheduleCancel",
];
fn request_data(request: &RestRequest) -> Result<Option<Value>, TransportError> {
    match request.body.as_ref() {
        None => Ok(None),
        Some(RestBody::Untransformed(value)) => {
            value.clone().into_value().map(Some).map_err(|error| {
                TransportError::Host(format!("Hyperliquid typed signer input refusal: {error}"))
            })
        }
        Some(RestBody::Transformed(data)) => {
            data.value()
                .clone()
                .into_value()
                .map(Some)
                .map_err(|error| {
                    TransportError::Host(format!("Hyperliquid typed signer input refusal: {error}"))
                })
        }
        Some(RestBody::Encoded(bytes)) => std::str::from_utf8(bytes)
            .map(|text| Some(Value::String(text.into())))
            .map_err(|error| TransportError::Host(error.to_string())),
    }
}
// Match the shared capture's node-depth budget before the UTF-16 raw decoder.
// RawValue validates syntax; this scan counts value nodes without decoding keys
// or string values into UTF-8, including unknown fields and duplicate members.
fn within_request_capture_depth(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut frames = Vec::new();
    let mut value = true;
    let mut offset = 0;
    while let Some(&byte) = bytes.get(offset) {
        match byte {
            b' ' | b'\n' | b'\r' | b'\t' => offset += 1,
            b'"' => {
                if value && frames.len() >= 128 {
                    return false;
                }
                offset += 1;
                while let Some(&c) = bytes.get(offset) {
                    offset += 1;
                    if c == b'\\' {
                        offset += 1;
                    } else if c == b'"' {
                        break;
                    }
                }
                value = false;
            }
            b'{' | b'[' => {
                if frames.len() >= 128 {
                    return false;
                }
                frames.push(byte);
                value = byte == b'[';
                offset += 1;
            }
            b'}' | b']' => {
                frames.pop();
                value = false;
                offset += 1;
            }
            b':' => {
                value = true;
                offset += 1;
            }
            b',' => {
                value = frames.last() == Some(&b'[');
                offset += 1;
            }
            _ => {
                if value && frames.len() >= 128 {
                    return false;
                }
                while bytes.get(offset).is_some_and(|c| {
                    !matches!(c, b' ' | b'\n' | b'\r' | b'\t' | b',' | b'}' | b']')
                }) {
                    offset += 1;
                }
                value = false;
            }
        }
    }
    true
}
pub fn get_request_action_type(data: &Value) -> Option<String> {
    if let Value::String(raw) = data {
        use crate::json_values::JsonValue;
        // Validate syntax first: the budget scanner never interprets malformed
        // JSON, and full-input consumption is still checked by serde.
        serde_json::from_str::<Box<serde_json::value::RawValue>>(raw).ok()?;
        if !within_request_capture_depth(raw) {
            return None;
        }
        let JsonValue::Object(data) = serde_json::from_str::<JsonValue>(raw).ok()? else {
            return None;
        };
        let JsonValue::Object(action) = data.get(&"action".encode_utf16().collect::<Vec<_>>())?
        else {
            return None;
        };
        let JsonValue::String(kind) = action.get(&"type".encode_utf16().collect::<Vec<_>>())?
        else {
            return None;
        };
        return String::from_utf16(&kind.0).ok();
    }
    data.get("action")
        .filter(|v| v.is_object())?
        .get("type")?
        .as_str()
        .map(str::to_owned)
}
fn request_action_type(request: &RestRequest) -> Result<Option<String>, TransportError> {
    if let Some(RestBody::Encoded(bytes)) = &request.body {
        let raw =
            std::str::from_utf8(bytes).map_err(|error| TransportError::Host(error.to_string()))?;
        Ok(get_request_action_type(&Value::String(raw.to_owned())))
    } else {
        Ok(request_data(request)?.and_then(|data| get_request_action_type(&data)))
    }
}
type TelemetryEmitter = dyn Fn(&str, Option<&Value>);
type TelemetryProvider = dyn Fn(&Value, &str) -> Option<Value>;
pub struct ApiClient {
    options: ExchangeOptions,
    signing: bool,
    exchange_client: bool,
    adapter: Arc<dyn RestAdapter>,
    limiter: Rc<RateLimiter>,
    emit: Arc<dyn Fn(&str)>,
    telemetry: Option<Arc<TelemetryProvider>>,
    telemetry_emit: Option<Arc<TelemetryEmitter>>,
}
pub fn create_exchange_api(
    options: ExchangeOptions,
    adapter: Arc<dyn RestAdapter>,
    limiter: Rc<RateLimiter>,
    emit: Arc<dyn Fn(&str)>,
) -> ApiClient {
    ApiClient {
        signing: !options.public_data_only.unwrap_or(false),
        exchange_client: true,
        options,
        adapter,
        limiter,
        emit,
        telemetry: None,
        telemetry_emit: None,
    }
}
pub fn create_info_api(
    options: ExchangeOptions,
    adapter: Arc<dyn RestAdapter>,
    limiter: Rc<RateLimiter>,
    emit: Arc<dyn Fn(&str)>,
) -> ApiClient {
    ApiClient {
        signing: false,
        exchange_client: false,
        options,
        adapter,
        limiter,
        emit,
        telemetry: None,
        telemetry_emit: None,
    }
}
pub fn create_api(
    options: ExchangeOptions,
    adapter: Arc<dyn RestAdapter>,
    limiter: Rc<RateLimiter>,
    emit: Arc<dyn Fn(&str)>,
) -> ApiClient {
    create_exchange_api(options, adapter, limiter, emit)
}
impl ApiClient {
    pub fn with_order_error_telemetry(
        mut self,
        provider: Arc<TelemetryProvider>,
        emit: Arc<TelemetryEmitter>,
    ) -> Self {
        self.telemetry = Some(provider);
        self.telemetry_emit = Some(emit);
        self
    }
    fn emit_order_error(&self, request: &RestRequest, message: &str) -> Result<(), TransportError> {
        let data = request_data(request)?.unwrap_or(Value::Null);
        let context = self
            .telemetry
            .as_ref()
            .and_then(|provider| provider(&data, message));
        if let Some(emit) = &self.telemetry_emit {
            emit(message, context.as_ref());
        } else {
            (self.emit)(message);
        }
        Ok(())
    }

    pub fn build_request(&self, url: &str, body: &Value) -> Result<RestRequest, TransportError> {
        let environment = if self.options.testnet.unwrap_or(false) {
            "TESTNET"
        } else {
            "PRODUCTION"
        };
        let base_url = BASE_URLS.iter().find(|(k, _)| *k == environment).unwrap().1;
        let mut request = RestRequest {
            method: "POST".into(),
            url: url.into(),
            base_url: Some(base_url.into()),
            body: Some(RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(body.clone())
                    .map_err(|error| TransportError::Host(error.to_string()))?,
            )),
            // safeCexAxiosCreate overrides factory recvWindow config before retries.
            timeout_ms: Some(20_000),
            ..Default::default()
        };
        request.headers.insert(
            "Content-Type".into(),
            "application/json, charset=utf-8".into(),
        );
        Ok(request)
    }
    pub fn prepare_request(
        &self,
        mut request: RestRequest,
        capture: bool,
    ) -> TransportFuture<'_, RestRequest> {
        Box::pin(async move {
            let data = request_data(&request)?.unwrap_or(Value::Null);
            // Axios executes request interceptors in reverse registration order.
            if !capture {
                let weight = get_hyperliquid_rest_request_weight(&request.url, &data)
                    .map_err(TransportError::JavaScript)?;
                self.limiter.wait_for_token(weight).await?
            }
            if request.url == "/info"
                && (self.signing
                    || !self.options.public_data_only.unwrap_or(false)
                    || !self.is_exchange_client())
            {
                request.skip_remote_signing = true;
            }
            // Encoded bytes lack signing provenance. Preserve the native fail-closed
            // guard for signable JSON rather than infer an untransformed template.
            // Pacing and telemetry above observe the original string, as Axios does.
            let mut body = match &request.body {
                Some(RestBody::Encoded(bytes)) => serde_json::from_slice::<
                    safe_cex_types::retained_json::RetainedJsonInput,
                >(bytes)
                .and_then(safe_cex_types::retained_json::RetainedJsonInput::into_value)
                .unwrap_or(Value::Null),
                _ => data,
            };
            if !self.signing
                || self.options.secret.is_empty()
                || !body.is_object()
                || signed_body(&body)
                || !body.get("action").is_some_and(truthy)
                || body.get("nonce").is_none()
            {
                return Ok(request);
            }
            let action = &body["action"];
            let action_type = action
                .get("type")
                .filter(|v| !v.is_null())
                .map(|value| safe_cex_num::js_string(Some(value)))
                .transpose()
                .map_err(TransportError::JavaScript)?
                .unwrap_or_default();
            if !RELAY_SIGNABLE_ACTION_TYPES.contains(&action_type.as_str()) {
                (self.emit)(&format!(
                    "Refusing to sign unexpected Hyperliquid action \"{action_type}\""
                ));
                return Ok(request);
            }
            // Source evaluates createWalletFromPrivateKey before signL1Action.
            crate::signing::validate_wallet_key(&self.options.secret)
                .map_err(TransportError::Host)?;
            let vault = body
                .get("vaultAddress")
                .filter(|v| truthy(v))
                .map(js_string);
            let original_body = request
                .body
                .as_ref()
                .ok_or_else(|| TransportError::Host("Signing requires request data".into()))?;
            original_body.untransformed()?;
            let signature = sign_l1_action(
                &self.options.secret,
                action,
                vault.as_deref(),
                &body["nonce"],
                !self.options.testnet.unwrap_or(false),
            )
            .map_err(|cause| {
                TransportError::Rejected(safe_cex_transport::provider::TransportErrorOrigin::new(
                    cause,
                ))
            })?;
            body.as_object_mut().unwrap().insert(
                "signature".into(),
                serde_json::to_value(signature).map_err(|e| TransportError::Host(e.to_string()))?,
            );
            request.body = Some(
                original_body.with_signed_data(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(body)
                        .map_err(|error| TransportError::Host(error.to_string()))?,
                )?,
            );
            Ok(request)
        })
    }
    fn is_exchange_client(&self) -> bool {
        self.exchange_client
    }
    pub fn check_response(
        &self,
        request: &RestRequest,
        response: RestResponse,
        capture: bool,
    ) -> Result<RestResponse, TransportError> {
        let data = match &response.body {
            safe_cex_transport::RestResponseBody::Encoded(bytes) if bytes.is_empty() => Value::Null,
            _ => response
                .body
                .clone()
                .transform_axios_response()?
                .json_value()
                .map_err(|error| TransportError::Host(error.to_string()))?,
        };
        let is_exchange = request.url == "/exchange";
        let failed = is_exchange && response.status >= 400;
        let body_error = extract_hyperliquid_response_error(&data);
        if let Some(error) = body_error {
            let msg = if failed {
                format!("{error} (HTTP {})", response.status)
            } else {
                error.clone()
            };
            let action = request_action_type(request)?;
            if !(is_exchange
                && matches!(action.as_deref(), Some("cancel" | "cancelByCloid"))
                && is_only_hyperliquid_terminal_cancel_error(&error))
            {
                self.emit_order_error(request, &msg)?
            }
            return Err(TransportError::Host(msg));
        }
        if failed {
            let body = describe_response(&data);
            let detail = if data.is_null() {
                String::new()
            } else {
                format!(
                    ": {}",
                    String::from_utf16(&body.encode_utf16().take(300).collect::<Vec<_>>())
                        .map_err(|_| TransportError::Host(
                            "Hyperliquid response diagnostic exceeds native UTF-8 representation"
                                .into()
                        ))?
                )
            };
            let msg = format!("Hyperliquid API error (HTTP {}){detail}", response.status);
            (self.emit)(&msg);
            return Err(TransportError::Host(msg));
        }
        if is_exchange && data.is_null() && !capture {
            let msg = format!(
                "{EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE} (HTTP {})",
                response.status
            );
            (self.emit)(&msg);
            return Err(TransportError::Host(msg));
        }
        Ok(response)
    }
    pub fn execute(&self, request: RestRequest, capture: bool) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            let adapter = Attempt {
                client: self,
                capture,
            };
            // ts-bug: isHyperliquidRead compares only the relative path before ?, so absolute /exchange URLs retry.
            match ReadClassification::Hyperliquid.classify(request) {
                ClassifiedRequest::Read(request) => {
                    only_reads(request, 3, |_, result| {
                        result
                            .as_ref()
                            .err()
                            .is_some_and(RestFailure::is_retryable_network)
                    })
                    .execute(&adapter, self.limiter.clock())
                    .await
                }
                ClassifiedRequest::Write(request) => request.execute(&adapter).await,
            }
        })
    }
}
struct Attempt<'a> {
    client: &'a ApiClient,
    capture: bool,
}
impl RestAdapter for Attempt<'_> {
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
            let original = request.clone();
            let request = self
                .client
                .prepare_request(request, self.capture)
                .await
                .map_err(|error| RestFailure::from_transport(error, original))?;
            let execution = self
                .client
                .adapter
                .execute_with_receipt(request.clone())
                .await?;
            let response = execution.response;
            let config = execution.config;
            let response = self
                .client
                .check_response(&config, response.clone(), self.capture)
                .map_err(|error| RestFailure {
                    code: None,
                    message: error.to_string(),
                    transport_error: Some(Box::new(error)),
                    response: Some(Box::new(response)),
                    request: Box::new(config.clone()),
                    context: None,
                })?;
            Ok(safe_cex_transport::RestExecution { response, config })
        })
    }
}
