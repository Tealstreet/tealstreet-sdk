use crate::routing::{ResolvedRouting, RoutingOptions};
use safe_cex_types::{ExchangeName, ExchangeOptions};
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
/// ts-bug: Binance signs a JSON-looking string via character indices, unlike an object.
/// Deliberately has no Default: callers must identify the representation explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestBody {
    Untransformed(serde_json::Value),
    Encoded(Vec<u8>),
    Transformed(TransformedData),
}
/// Axios data after the shared transform. Private construction preserves its
/// provenance; this is not an inferred origin for arbitrary encoded bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformedData {
    value: serde_json::Value,
}
impl TransformedData {
    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }
}
impl RestBody {
    /// A remote client already ran its signing/transform pipeline. Its returned
    /// adapter data is authoritative, including objects from an echoing signer.
    pub(crate) fn from_remote_adapter_data(value: serde_json::Value) -> Self {
        Self::Transformed(TransformedData { value })
    }
    /// Signing cannot recover String-versus-Object identity from encoded bytes.
    pub fn untransformed(&self) -> Result<&serde_json::Value, TransportError> {
        match self {
            Self::Untransformed(value) => Ok(value),
            Self::Transformed(data) => Ok(&data.value),
            Self::Encoded(_) => Err(TransportError::Host(
                "Signing requires untransformed request data".into(),
            )),
        }
    }
    /// Retain the Axios transform phase when a signing interceptor returns data.
    /// An opaque encoded body cannot be used as a signing template.
    pub fn with_signed_data(&self, value: serde_json::Value) -> Result<Self, TransportError> {
        match self {
            Self::Untransformed(_) => Ok(Self::Untransformed(value)),
            Self::Transformed(_) => Ok(Self::Transformed(TransformedData { value })),
            Self::Encoded(_) => Err(TransportError::Host(
                "Signing requires untransformed request data".into(),
            )),
        }
    }
    /// Produce the post-transform JS string while its origin is still known.
    /// Retrying this value uses Axios's identity transform, never JSON reparsing.
    pub fn transform(&self, content_type: Option<&str>) -> Result<Self, TransportError> {
        match self {
            Self::Encoded(_) | Self::Transformed(_) => Ok(self.clone()),
            Self::Untransformed(value)
                if !value.is_object()
                    && !value.is_array()
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
                    value: serde_json::Value::String(text),
                }))
            }
        }
    }
    /// Final data transformation without a JSON content type. Strings stay raw;
    /// HTTP boundaries must use wire_bytes_for_content_type with actual headers.
    /// Call only after the signing operation.
    pub fn wire_bytes(&self) -> Result<Vec<u8>, TransportError> {
        self.wire_bytes_for_content_type(None)
    }
    /// Axios stringifySafely after signing. Header names are resolved by the
    /// caller; the content-type value match is case-sensitive, as in Axios.
    pub fn wire_bytes_for_content_type(
        &self,
        content_type: Option<&str>,
    ) -> Result<Vec<u8>, TransportError> {
        match self {
            Self::Encoded(bytes) => Ok(bytes.clone()),
            Self::Transformed(data) => match &data.value {
                serde_json::Value::String(text) => Ok(text.as_bytes().to_vec()),
                value => safe_cex_num::js_json_stringify(&value.clone().into())
                    .map_err(|error| TransportError::Host(error.to_string()))?
                    .map(String::into_bytes)
                    .ok_or_else(|| TransportError::Host("Undefined identity body".into())),
            },
            Self::Untransformed(value)
                if (value.is_object() || value.is_array())
                    && content_type.is_some_and(|value| {
                        value.contains("application/x-www-form-urlencoded")
                    }) =>
            {
                axios_form_encode(value).map(String::into_bytes)
            }
            Self::Untransformed(serde_json::Value::String(text)) => {
                if !content_type.is_some_and(|value| value.contains("application/json")) {
                    return Ok(text.as_bytes().to_vec());
                }
                // Parse BEFORE trimming: a BOM is valid JS trim whitespace but
                // invalid JSON.parse input. RawValue validates without binary64
                // conversion or serde's typed recursion-depth restriction.
                if serde_json::from_str::<Box<serde_json::value::RawValue>>(text).is_ok() {
                    return Ok(safe_cex_num::js_trim(text).as_bytes().to_vec());
                }
                safe_cex_num::js_json_stringify(&serde_json::Value::String(text.clone()).into())
                    .map_err(|error| TransportError::Host(error.to_string()))?
                    .map(String::into_bytes)
                    .ok_or_else(|| TransportError::Host("JSON string encoded as undefined".into()))
            }
            Self::Untransformed(value) => safe_cex_num::js_json_stringify(&value.clone().into())
                .map_err(|error| TransportError::Host(error.to_string()))?
                .map(String::into_bytes)
                .ok_or_else(|| {
                    TransportError::Host("JSON data unexpectedly encoded as undefined".into())
                }),
        }
    }
}

// Axios toURLEncodedForm defaults: URLSearchParams encoding, bracket paths,
// root flat-array expansion, metadata tokens and null/undefined omission.
fn axios_form_encode(value: &serde_json::Value) -> Result<String, TransportError> {
    use serde_json::Value;
    fn depth_error(depth: usize) -> TransportError {
        TransportError::Host(format!(
            "Object is too deeply nested ({depth} levels). Max depth: 100"
        ))
    }
    fn check_json_depth(value: &Value, depth: usize) -> Result<(), TransportError> {
        if value.is_object() || value.is_array() {
            if depth > 100 {
                return Err(depth_error(depth));
            }
            match value {
                Value::Array(items) => {
                    for item in items {
                        check_json_depth(item, depth + 1)?;
                    }
                }
                Value::Object(items) => {
                    for item in items.values() {
                        check_json_depth(item, depth + 1)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn append(
        output: &mut url::form_urlencoded::Serializer<'_, String>,
        key: &str,
        value: &Value,
    ) -> Result<(), TransportError> {
        let text = safe_cex_num::js_string(Some(value))
            .map_err(|error| TransportError::Host(error.to_string()))?;
        output.append_pair(key, &text);
        Ok(())
    }
    fn visit(
        value: &Value,
        path: &[String],
        output: &mut url::form_urlencoded::Serializer<'_, String>,
    ) -> Result<(), TransportError> {
        if path.len() > 100 {
            return Err(depth_error(path.len()));
        }
        let entries: Vec<(String, &Value)> = match value {
            Value::Object(map) => safe_cex_num::js_own_keys(map.keys().map(String::as_str))
                .into_iter()
                .map(|key| (key.to_owned(), &map[key]))
                .collect(),
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect(),
            _ => return Ok(()),
        };
        for (original_key, item) in entries {
            if item.is_null() {
                continue;
            }
            let key = safe_cex_num::js_trim(&original_key);
            if path.is_empty() && (item.is_object() || item.is_array()) {
                if key.ends_with("{}") {
                    check_json_depth(item, 1)?;
                    let text = safe_cex_num::js_json_stringify(&item.clone().into())
                        .map_err(|error| TransportError::Host(error.to_string()))?
                        .ok_or_else(|| TransportError::Host("Undefined form metadata".into()))?;
                    output.append_pair(key, &text);
                    continue;
                }
                let flat = item
                    .as_array()
                    .is_some_and(|items| items.iter().all(|v| !v.is_object() && !v.is_array()));
                if flat || key.ends_with("[]") {
                    let array = item.as_array();
                    let length = item
                        .as_object()
                        .and_then(|map| map.get("length"))
                        .filter(|v| v.is_number());
                    if array.is_some() || length.is_some() {
                        let length = if let Some(array) = array {
                            array.len()
                        } else {
                            let number = length.and_then(Value::as_f64).unwrap_or(f64::NAN);
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
                        // Sparse array-like objects need not allocate their declared length.
                        if let Some(array) = array {
                            for element in array {
                                if !element.is_null() {
                                    append(output, &array_key, element)?;
                                }
                            }
                        } else if let Some(map) = item.as_object() {
                            for index in safe_cex_num::js_own_keys(map.keys().map(String::as_str)) {
                                if let Ok(i) = index.parse::<usize>()
                                    && i < length
                                    && i.to_string() == index
                                    && !map[index].is_null()
                                {
                                    append(output, &array_key, &map[index])?;
                                }
                            }
                        }
                        continue;
                    }
                }
            }
            if item.is_object() || item.is_array() {
                let mut next = path.to_vec();
                // Axios recurses using the original key, before visitor trim.
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
                        .map(|(i, token)| {
                            let token = token.strip_suffix("[]").unwrap_or(token);
                            if i == 0 {
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
        let object = matches!(&self.body, Some(RestBody::Untransformed(value)) if value.is_object() || value.is_array());
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
            Some(RestBody::Transformed(data)) if data.value().is_null() => None,
            Some(body) => Some(RestBody::Encoded(body.wire_bytes()?)),
        };
        Ok(wire)
    }
}
/// Response data at an explicit adapter boundary. There is deliberately no Default.
/// Encoded HTTP bytes and a parser-returned string can have identical serialized
/// bytes while supplying different input to the next response transform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestResponseBody {
    /// Physical response bytes, before text decoding or response transforms.
    Encoded(Vec<u8>),
    /// Native adapter data before a response transform has run.
    Untransformed(serde_json::Value),
    /// Actual output of a response transform; never re-encode to infer its origin.
    Transformed(serde_json::Value),
}
impl RestResponseBody {
    /// Current input for the next transform. Physical bytes become raw UTF-8 text,
    /// not parsed JSON; native adapter data retains its exact value and type.
    pub fn into_data(self) -> serde_json::Value {
        match self {
            Self::Encoded(bytes) => {
                serde_json::Value::String(String::from_utf8_lossy(&bytes).into_owned())
            }
            Self::Untransformed(value) | Self::Transformed(value) => value,
        }
    }

    /// Apply an infallible response transform without changing its input shape.
    pub fn transform(self, transform: impl FnOnce(serde_json::Value) -> serde_json::Value) -> Self {
        Self::Transformed(transform(self.into_data()))
    }

    /// Apply one caller-selected response transform to current adapter data.
    /// A transformed string is still passed to the next transform as a string:
    /// the phase is provenance, not an instruction to skip a source parser.
    pub fn try_transform<E>(
        self,
        transform: impl FnOnce(serde_json::Value) -> Result<serde_json::Value, E>,
    ) -> Result<Self, E> {
        transform(self.into_data()).map(Self::Transformed)
    }

    /// Apply Axios's default response JSON transform to current adapter data.
    pub fn transform_axios_response(self) -> Result<Self, TransportError> {
        let data = match self {
            Self::Encoded(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                serde_json::Value::String(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
            }
            Self::Untransformed(value) | Self::Transformed(value) => value,
        };
        let serde_json::Value::String(text) = data else {
            return Ok(Self::Transformed(data));
        };
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(mut parsed) => {
                crate::how::normalize_json_numbers(&mut parsed, false, false);
                Ok(Self::Transformed(parsed))
            }
            Err(error) => {
                if serde_json::from_str::<Box<serde_json::value::RawValue>>(&text).is_ok() {
                    return Err(TransportError::Host(format!(
                        "Axios response representation refused: {error}"
                    )));
                }
                Ok(Self::Transformed(serde_json::Value::String(text)))
            }
        }
    }

    /// Read the current model value. This parses physical JSON bytes once, but
    /// retains native values unchanged. It does not apply the next Axios/venue
    /// response transform; use transform or try_transform for that observable operation.
    pub fn json_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        match self {
            Self::Encoded(bytes) => {
                serde_json::from_slice::<safe_cex_types::retained_json::RetainedJsonInput>(bytes)
                    .and_then(safe_cex_types::retained_json::RetainedJsonInput::into_value)
            }
            Self::Untransformed(value) | Self::Transformed(value) => Ok(value.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
