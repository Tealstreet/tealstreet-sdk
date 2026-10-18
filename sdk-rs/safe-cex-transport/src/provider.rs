use crate::routing::{ResolvedRouting, RoutingOptions};
use safe_cex_types::{ExchangeName, ExchangeOptions, retained_json::RetainedJsonInput as Input};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

#[derive(Clone)]
pub struct TransportErrorOrigin(Arc<dyn std::error::Error + Send + Sync>);
impl TransportErrorOrigin {
    pub fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Arc::new(error))
    }
    pub fn error(&self) -> &(dyn std::error::Error + 'static) {
        self.0.as_ref()
    }
}
impl std::fmt::Debug for TransportErrorOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportErrorOrigin")
            .finish_non_exhaustive()
    }
}
impl PartialEq for TransportErrorOrigin {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

// Host futures need not be Send: browser transports may retain JS handles.
pub type TransportFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, TransportError>> + 'a>>;

#[derive(Debug, Clone, PartialEq)]
pub enum TransportError {
    DirectRoutingRequested(ExchangeName),
    UnavailableTransport(String),
    Host(String),
    Rejected(TransportErrorOrigin),
    Timeout,
    Network(NetworkFailure),
    JavaScript(safe_cex_num::JsCoercionError),
}
impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DirectRoutingRequested(exchange) => {
                let name =
                    serde_json::to_string(exchange).expect("ExchangeName serializes to a string");
                write!(
                    f,
                    "[{}] tealstreetProxy/tealstreetTunnel requested but the direct transport is active. Register a transport with setDefaultTransport() or pass ExchangeOptions.transport.",
                    name.trim_matches('"')
                )
            }
            Self::UnavailableTransport(message) | Self::Host(message) => f.write_str(message),
            Self::Network(error) => f.write_str(&error.message),
            Self::Rejected(origin) => std::fmt::Display::fmt(origin.error(), f),
            Self::JavaScript(error) => f.write_str(error.message),
            Self::Timeout => f.write_str("TradeTunnelTimeoutError"),
        }
    }
}
impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rejected(origin) => Some(origin.error()),
            _ => None,
        }
    }
}

/// Originating network error evidence. Retry eligibility is never caller supplied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkFailure {
    message: String,
    code: Option<String>,
}
impl NetworkFailure {
    pub fn from_code(message: String, code: Option<String>) -> Self {
        Self { message, code }
    }
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
    pub(crate) fn retry_allowed(&self) -> bool {
        self.code.as_deref().is_some_and(|code| {
            !code.is_empty()
                && !matches!(code, "ERR_CANCELED" | "ECONNABORTED")
                && !NETWORK_DENIED_CODES.contains(&code)
        })
    }
}
// Installed is-retry-allowed deny list, checked against the live axios-retry predicate.
const NETWORK_DENIED_CODES: &[&str] = &[
    "ENOTFOUND",
    "ENETUNREACH",
    "UNABLE_TO_GET_ISSUER_CERT",
    "UNABLE_TO_GET_CRL",
    "UNABLE_TO_DECRYPT_CERT_SIGNATURE",
    "UNABLE_TO_DECRYPT_CRL_SIGNATURE",
    "UNABLE_TO_DECODE_ISSUER_PUBLIC_KEY",
    "CERT_SIGNATURE_FAILURE",
    "CRL_SIGNATURE_FAILURE",
    "CERT_NOT_YET_VALID",
    "CERT_HAS_EXPIRED",
    "CRL_NOT_YET_VALID",
    "CRL_HAS_EXPIRED",
    "ERROR_IN_CERT_NOT_BEFORE_FIELD",
    "ERROR_IN_CERT_NOT_AFTER_FIELD",
    "ERROR_IN_CRL_LAST_UPDATE_FIELD",
    "ERROR_IN_CRL_NEXT_UPDATE_FIELD",
    "OUT_OF_MEM",
    "DEPTH_ZERO_SELF_SIGNED_CERT",
    "SELF_SIGNED_CERT_IN_CHAIN",
    "UNABLE_TO_GET_ISSUER_CERT_LOCALLY",
    "UNABLE_TO_VERIFY_LEAF_SIGNATURE",
    "CERT_CHAIN_TOO_LONG",
    "CERT_REVOKED",
    "INVALID_CA",
    "PATH_LENGTH_EXCEEDED",
    "INVALID_PURPOSE",
    "CERT_UNTRUSTED",
    "CERT_REJECTED",
    "HOSTNAME_MISMATCH",
];

/// Request data before Axios transformation, or bytes already prepared for the wire.
/// Signing retains native data and its explicit transform provenance.
#[derive(Debug, Clone, PartialEq)]
pub enum RestBody {
    Untransformed(Input),
    Encoded(Vec<u8>),
    Transformed(TransformedData),
}
#[derive(Debug, Clone, PartialEq)]
pub struct TransformedData {
    value: Input,
}
impl TransformedData {
    pub fn value(&self) -> &Input {
        &self.value
    }
}
impl RestBody {
    pub(crate) fn from_remote_adapter_data(value: Input) -> Self {
        Self::Transformed(TransformedData { value })
    }
    pub fn untransformed(&self) -> Result<&Input, TransportError> {
        match self {
            Self::Untransformed(value) => Ok(value),
            Self::Transformed(data) => Ok(&data.value),
            Self::Encoded(_) => Err(TransportError::Host(
                "Signing requires untransformed request data".into(),
            )),
        }
    }
    pub fn with_signed_data(&self, value: Input) -> Result<Self, TransportError> {
        match self {
            Self::Untransformed(_) => Ok(Self::Untransformed(value)),
            Self::Transformed(_) => Ok(Self::Transformed(TransformedData { value })),
            Self::Encoded(_) => Err(TransportError::Host(
                "Signing requires untransformed request data".into(),
            )),
        }
    }
    pub fn transform(&self, content_type: Option<&str>) -> Result<Self, TransportError> {
        match self {
            Self::Encoded(_) | Self::Transformed(_) => Ok(self.clone()),
            Self::Untransformed(Input::Undefined) => Ok(Self::Transformed(TransformedData {
                value: Input::Undefined,
            })),
            Self::Untransformed(value)
                if !matches!(value, Input::Object(_) | Input::Array(_))
                    && !content_type.is_some_and(|value| value.contains("application/json")) =>
            {
                Ok(Self::Transformed(TransformedData {
                    value: value.clone(),
                }))
            }
            Self::Untransformed(_) => {
                let bytes = self.wire_bytes_for_content_type(content_type)?;
                let text = String::from_utf8(bytes)
                    .map_err(|error| TransportError::Host(error.to_string()))?;
                Ok(Self::Transformed(TransformedData {
                    value: Input::Text(text.encode_utf16().collect()),
                }))
            }
        }
    }
    pub fn wire_bytes(&self) -> Result<Vec<u8>, TransportError> {
        self.wire_bytes_for_content_type(None)
    }
    pub fn wire_bytes_for_content_type(
        &self,
        content_type: Option<&str>,
    ) -> Result<Vec<u8>, TransportError> {
        let json = |value: &Input| {
            safe_cex_num::js_retained_json_stringify(value)
                .map(|text| text.unwrap_or_default().into_bytes())
                .map_err(|error| TransportError::Host(error.to_string()))
        };
        match self {
            Self::Encoded(bytes) => Ok(bytes.clone()),
            // Physical HTTP string encoding follows TextEncoder/Buffer UTF-8 replacement.
            Self::Transformed(data) => match &data.value {
                Input::Text(text) => Ok(String::from_utf16_lossy(text).into_bytes()),
                value => json(value),
            },
            Self::Untransformed(value)
                if matches!(value, Input::Object(_) | Input::Array(_))
                    && content_type.is_some_and(|value| {
                        value.contains("application/x-www-form-urlencoded")
                    }) =>
            {
                axios_form_encode(value).map(String::into_bytes)
            }
            Self::Untransformed(Input::Text(text)) => {
                if !content_type.is_some_and(|value| value.contains("application/json")) {
                    return Ok(String::from_utf16_lossy(text).into_bytes());
                }
                // Parse before trimming: BOM is trim whitespace but invalid JSON input.
                if Input::parse_json(text).is_ok() {
                    let text = String::from_utf16_lossy(text);
                    return Ok(safe_cex_num::js_trim(&text).as_bytes().to_vec());
                }
                json(&Input::Text(text.clone()))
            }
            Self::Untransformed(value) => json(value),
        }
    }
}

// Axios toURLEncodedForm defaults: URLSearchParams encoding, bracket paths,
// root flat-array expansion, metadata tokens and null/undefined omission.
fn axios_form_encode(value: &Input) -> Result<String, TransportError> {
    fn composite(value: &Input) -> bool {
        matches!(value, Input::Object(_) | Input::Array(_))
    }
    fn nullish(value: &Input) -> bool {
        matches!(value, Input::Null | Input::Undefined)
    }
    fn depth_error(depth: usize) -> TransportError {
        TransportError::Host(format!(
            "Object is too deeply nested ({depth} levels). Max depth: 100"
        ))
    }
    fn check_json_depth(value: &Input, depth: usize) -> Result<(), TransportError> {
        if composite(value) {
            if depth > 100 {
                return Err(depth_error(depth));
            }
            for (_, child) in crate::native_data::entries(value) {
                check_json_depth(child, depth + 1)?;
            }
        }
        Ok(())
    }
    fn append(
        output: &mut url::form_urlencoded::Serializer<'_, String>,
        key: &str,
        value: &Input,
    ) -> Result<(), TransportError> {
        let text = safe_cex_types::js_coercion::js_retained_string_utf16(value)
            .map_err(|error| TransportError::Host(error.to_string()))?;
        // URLSearchParams converts completed pairs to USVString at this physical boundary.
        output.append_pair(key, &String::from_utf16_lossy(&text));
        Ok(())
    }
    fn visit(
        value: &Input,
        path: &[String],
        output: &mut url::form_urlencoded::Serializer<'_, String>,
    ) -> Result<(), TransportError> {
        if path.len() > 100 {
            return Err(depth_error(path.len()));
        }
        for (original_key, item) in crate::native_data::entries(value) {
            if nullish(item) {
                continue;
            }
            let original_key = String::from_utf16_lossy(&original_key);
            let key = safe_cex_num::js_trim(&original_key);
            if path.is_empty() && composite(item) {
                if key.ends_with("{}") {
                    check_json_depth(item, 1)?;
                    let text = safe_cex_num::js_retained_json_stringify(item)
                        .map_err(|error| TransportError::Host(error.to_string()))?
                        .ok_or_else(|| TransportError::Host("Undefined form metadata".into()))?;
                    output.append_pair(key, &text);
                    continue;
                }
                let array = match item {
                    Input::Array(values) => Some(values),
                    _ => None,
                };
                let flat = array.is_some_and(|items| items.iter().all(|value| !composite(value)));
                if flat || key.ends_with("[]") {
                    let length = crate::native_data::property(item, "length")
                        .and_then(crate::native_data::number);
                    if array.is_some() || length.is_some() {
                        let length = if let Some(array) = array {
                            array.len()
                        } else {
                            let number = length.expect("numeric length selected");
                            if !number.is_finite()
                                || number < 0.0
                                || number.fract() != 0.0
                                || number > u32::MAX as f64
                            {
                                return Err(TransportError::Host("Invalid array length".into()));
                            }
                            number as usize
                        };
                        let array_key = format!("{}[]", key.strip_suffix("[]").unwrap_or(key));
                        if let Some(array) = array {
                            for element in array {
                                if !nullish(element) {
                                    append(output, &array_key, element)?;
                                }
                            }
                        } else {
                            for (index, element) in crate::native_data::entries(item) {
                                let index = String::from_utf16_lossy(&index);
                                if let Ok(i) = index.parse::<usize>()
                                    && i < length
                                    && i.to_string() == index
                                    && !nullish(element)
                                {
                                    append(output, &array_key, element)?;
                                }
                            }
                        }
                        continue;
                    }
                }
            }
            if composite(item) {
                let mut next = path.to_vec();
                next.push(original_key);
                visit(item, &next, output)?;
            } else {
                let rendered = if path.is_empty() {
                    key.to_owned()
                } else {
                    path.iter()
                        .map(String::as_str)
                        .chain(std::iter::once(key))
                        .enumerate()
                        .map(|(index, token)| {
                            let token = token.strip_suffix("[]").unwrap_or(token);
                            if index == 0 {
                                token.to_owned()
                            } else {
                                format!("[{token}]")
                            }
                        })
                        .collect::<String>()
                };
                append(output, &rendered, item)?;
            }
        }
        Ok(())
    }
    let mut output = url::form_urlencoded::Serializer::new(String::new());
    visit(value, &[], &mut output)?;
    Ok(output.finish())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RestRequest {
    pub method: String,
    pub url: String,
    pub base_url: Option<String>,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: Option<RestBody>,
    pub timeout_ms: Option<i64>,
    pub params: Option<serde_json::Value>,
    /// Native values are accepted only on the explicit profile path.
    pub query_params: Option<safe_cex_types::QueryValue>,
    pub query_profile: Option<crate::query::QueryProfile>,
    pub skip_remote_signing: bool,
    pub sign_ms: Option<u64>,
    /// Retained data-only Axios extension properties, including undefined and native
    /// nonfinite numbers. These remain separate from HTTP headers: only the
    /// selected adapter interprets them.
    pub config_extensions:
        indexmap::IndexMap<String, safe_cex_types::retained_json::RetainedJsonInput>,
}
impl RestRequest {
    /// Axios request header assembly: defaults first, then request overrides.
    /// Matching is case-insensitive; the first spelling and last value survive.
    pub fn merge_axios_headers(&mut self, defaults: &indexmap::IndexMap<String, String>) {
        let mut headers = indexmap::IndexMap::<String, String>::new();
        for (name, value) in defaults.iter().chain(self.headers.iter()) {
            let name = safe_cex_num::js_trim(name);
            if name.is_empty() {
                continue;
            }
            let existing = headers
                .keys()
                .find(|key| key.to_lowercase() == name.to_lowercase())
                .cloned();
            let value: String = value
                .chars()
                .filter(|ch| !matches!(*ch as u32, 0..=8 | 10..=31 | 127))
                .collect();
            headers.insert(
                existing.unwrap_or_else(|| name.to_owned()),
                value.trim_matches([' ', '\t']).to_owned(),
            );
        }
        self.headers = headers;
    }

    /// Run only after signing. Retain Axios data for retry and return its wire
    /// projection; null remains null in retry config and means no wire body.
    pub fn transform_axios_request(&mut self) -> Result<Self, TransportError> {
        self.merge_axios_headers(&indexmap::IndexMap::new());
        let content_key = self
            .headers
            .keys()
            .find(|key| key.eq_ignore_ascii_case("content-type"))
            .cloned();
        let object = matches!(&self.body, Some(RestBody::Untransformed(value)) if matches!(value, Input::Object(_) | Input::Array(_)));
        if content_key.is_none() && object {
            self.headers
                .insert("Content-Type".into(), "application/json".into());
        }
        let content_type = self
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.as_str());
        self.body = self
            .body
            .as_ref()
            .map(|body| body.transform(content_type))
            .transpose()?;
        if !self
            .headers
            .keys()
            .any(|key| key.eq_ignore_ascii_case("content-type"))
            && matches!(
                self.method.to_ascii_lowercase().as_str(),
                "post" | "put" | "patch"
            )
        {
            self.headers.insert(
                "Content-Type".into(),
                "application/x-www-form-urlencoded".into(),
            );
        }
        let mut wire = self.clone();
        wire.body = match &self.body {
            None => None,
            Some(RestBody::Transformed(data))
                if matches!(data.value(), Input::Null | Input::Undefined) =>
            {
                None
            }
            Some(body) => Some(RestBody::Encoded(body.wire_bytes()?)),
        };
        Ok(wire)
    }
}
/// Response provenance is retained independently of its native data shape.
#[derive(Debug, Clone, PartialEq)]
pub enum RestResponseBody {
    Encoded(Vec<u8>),
    Untransformed(Input),
    Transformed(Input),
}
impl RestResponseBody {
    pub fn into_data(self) -> Input {
        match self {
            Self::Encoded(bytes) => {
                Input::Text(String::from_utf8_lossy(&bytes).encode_utf16().collect())
            }
            Self::Untransformed(value) | Self::Transformed(value) => value,
        }
    }
    pub fn transform(self, transform: impl FnOnce(Input) -> Input) -> Self {
        Self::Transformed(transform(self.into_data()))
    }
    pub fn try_transform<E>(
        self,
        transform: impl FnOnce(Input) -> Result<Input, E>,
    ) -> Result<Self, E> {
        transform(self.into_data()).map(Self::Transformed)
    }
    pub fn transform_axios_response(self) -> Result<Self, TransportError> {
        let data = match self {
            Self::Encoded(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                Input::Text(
                    text.strip_prefix('\u{feff}')
                        .unwrap_or(&text)
                        .encode_utf16()
                        .collect(),
                )
            }
            Self::Untransformed(value) | Self::Transformed(value) => value,
        };
        let Input::Text(text) = data else {
            return Ok(Self::Transformed(data));
        };
        match Input::parse_json(&text) {
            Ok(parsed) => Ok(Self::Transformed(parsed)),
            Err(_) => Ok(Self::Transformed(Input::Text(text))),
        }
    }
    /// Explicit fallible typed-model projection; never used to deliver native callbacks.
    pub fn json_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        match self {
            Self::Encoded(bytes) => {
                serde_json::from_slice::<Input>(bytes).and_then(Input::into_value)
            }
            Self::Untransformed(value) | Self::Transformed(value) => {
                let mut projected = value.clone().into_value()?;
                crate::how::normalize_json_numbers(&mut projected, false, false);
                Ok(projected)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RestResponse {
    pub status: u16,
    /// Direct HTTP statusText when supplied; HOW deliberately leaves this absent.
    pub reason: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub body: RestResponseBody,
}

/// Successful adapter output with its effective Axios retry configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct RestExecution {
    pub response: RestResponse,
    pub config: RestRequest,
}

/// Response-bearing REST failure shared by adapters and exchange error enhancement.
#[derive(Debug, Clone, PartialEq)]
pub struct RestFailure {
    /// Original Axios error.code, including host/status failures.
    pub code: Option<String>,
    pub transport_error: Option<Box<TransportError>>,
    pub message: String,
    pub response: Option<Box<RestResponse>>,
    pub request: Box<RestRequest>,
    pub context: Option<Box<serde_json::Value>>,
}
impl RestFailure {
    /// Refusal before sending, without inventing network provenance.
    pub fn pre_transmit(message: String, request: RestRequest) -> Self {
        Self {
            code: None,
            message,
            transport_error: None,
            response: None,
            request: Box::new(request),
            context: None,
        }
    }
    pub fn from_transport(error: TransportError, request: RestRequest) -> Self {
        let code = match &error {
            TransportError::Network(origin) => origin.code().map(str::to_owned),
            _ => None,
        };
        Self {
            code,
            message: error.to_string(),
            transport_error: Some(Box::new(error)),
            response: None,
            request: Box::new(request),
            context: None,
        }
    }
    pub fn is_retryable_network(&self) -> bool {
        self.response.is_none()
            && matches!(self.transport_error.as_deref(),
            Some(TransportError::Network(error)) if error.retry_allowed())
    }
}
impl std::fmt::Display for RestFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for RestFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.transport_error
            .as_deref()
            .map(|error| error as &dyn std::error::Error)
    }
}
pub type RestFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, RestFailure>> + 'a>>;

pub trait RestAdapter {
    /// HOW owns frame params; URL adapters consume query values before sending.
    fn query_in_url(&self) -> bool {
        true
    }
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse>;
    /// Terminal adapters retain the input config. Middleware that rewrites or
    /// delegates requests must override this and preserve the inner receipt.
    fn execute_with_receipt(&self, request: RestRequest) -> RestFuture<'_, RestExecution> {
        Box::pin(async move {
            let config = request.clone();
            self.execute(request)
                .await
                .map(|response| RestExecution { response, config })
        })
    }
}
pub trait RestAuthorizer {
    fn authorize<'a>(&'a self, request: &'a mut RestRequest) -> TransportFuture<'a, ()>;
}

pub struct RestTransportContext<'a> {
    pub exchange: &'a ExchangeName,
    pub options: &'a ExchangeOptions,
    pub config: &'a RestRequest,
}
pub struct RestTransport {
    pub base_url: Option<String>,
    pub proxied: bool,
    pub authorize: Option<Arc<dyn RestAuthorizer>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestAdapterVariant {
    Default,
    BigInt,
    Region { region: Option<RestAdapterRegion> },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestAdapterRegion {
    Ja,
    Sg,
    Other(String),
}
pub struct RestAdapterContext<'a> {
    pub exchange: &'a ExchangeName,
    pub options: &'a ExchangeOptions,
    pub variant: RestAdapterVariant,
}

pub trait PrivateProxy {
    fn url(&self) -> &str;
    fn resolve_headers(&self) -> TransportFuture<'_, BTreeMap<String, String>>;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsConnection {
    Relay { headers: BTreeMap<String, String> },
    Headers { headers: BTreeMap<String, String> },
}

// Keep runtime providers outside serializable ExchangeOptions and worker payloads.
pub trait TransportProvider {
    fn name(&self) -> &str;
    fn rest(&self, context: RestTransportContext<'_>) -> Result<RestTransport, TransportError>;
    fn rest_adapter(&self, context: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>>;
    fn proxy_prefix(&self, context: RestTransportContext<'_>) -> String;
    fn public_cors_proxy(&self) -> Option<String>;
    fn private_proxy(&self, exchange: &ExchangeName) -> Option<Arc<dyn PrivateProxy>>;
    fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions;
    fn server_proxy_url(&self, exchange: &ExchangeName) -> Option<String>;
    fn ws_routing(&self, exchange: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting;
    fn ws_connection(&self, url: &str) -> Option<WsConnection>;
}
