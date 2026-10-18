use crate::js_runtime::value::Js;
use safe_cex_types::retained_json::RetainedJsonInput;
use std::{future::Future, pin::Pin, rc::Rc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSeverity {
    Debug,
    Info,
    Warning,
    Error,
}
impl LogSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// Original local-runtime failure retained without flattening its typed fields.
/// Equality compares origin identity; Debug deliberately omits retained request data.
#[derive(Clone)]
pub struct ExchangeErrorOrigin(Rc<dyn std::error::Error>);
impl ExchangeErrorOrigin {
    pub fn new(error: impl std::error::Error + 'static) -> Self {
        Self(Rc::new(error))
    }
    pub fn error(&self) -> &(dyn std::error::Error + 'static) {
        self.0.as_ref()
    }
}
impl std::fmt::Debug for ExchangeErrorOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExchangeErrorOrigin")
            .finish_non_exhaustive()
    }
}
impl PartialEq for ExchangeErrorOrigin {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ExchangeErrorOrigin {}

#[derive(Debug)]
struct RetainedResponseOrigin {
    response: RetainedJsonInput,
    cause: Option<ExchangeErrorOrigin>,
}
impl std::fmt::Display for RetainedResponseOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.cause {
            Some(cause) => std::fmt::Display::fmt(cause.error(), f),
            None => f.write_str("Retained HTTP response"),
        }
    }
}
impl std::error::Error for RetainedResponseOrigin {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(ExchangeErrorOrigin::error)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExchangeErrorMetadata {
    /// Opaque local origin. This makes ExchangeError local-thread, matching its
    /// runtime callbacks and Store subscriber causes; it is not Send or Sync.
    pub origin: Option<ExchangeErrorOrigin>,
    /// Store notification boundaries crossed, ordered from inner to outer.
    /// IDs are Store-local; equal IDs from different Stores need not identify the same subscriber.
    pub store_subscriber_ids: Vec<u64>,
    /// Original local callback cause; retained by reference across Store fanout.
    /// The Rc cause makes ExchangeError neither Send nor Sync.
    /// This local native value deliberately does not cross a worker boundary.
    pub store_subscriber_cause: Option<safe_cex_types::StoreSubscriberError>,
    pub is_axios_error: bool,
    pub request: Option<serde_json::Value>,
    pub stack: Option<String>,
    pub context: Option<serde_json::Value>,
    pub config: Option<serde_json::Value>,
    pub enhanced_message: Option<String>,
    pub url: Option<String>,
    pub method: Option<String>,
    pub original_error: Option<serde_json::Value>,
    pub code: Option<Js>,
}
impl PartialEq for ExchangeErrorMetadata {
    fn eq(&self, other: &Self) -> bool {
        let code_equal = match (&self.code, &other.code) {
            (None, None) => true,
            (Some(Js::Number(a)), Some(Js::Number(b))) => a.to_bits() == b.to_bits(),
            (Some(a), Some(b)) => a.strict_equals(b),
            _ => false,
        };
        code_equal
            && self.origin == other.origin
            && self.store_subscriber_ids == other.store_subscriber_ids
            && self.store_subscriber_cause == other.store_subscriber_cause
            && self.is_axios_error == other.is_axios_error
            && self.request == other.request
            && self.stack == other.stack
            && self.context == other.context
            && self.config == other.config
            && self.enhanced_message == other.enhanced_message
            && self.url == other.url
            && self.method == other.method
            && self.original_error == other.original_error
    }
}
impl Eq for ExchangeErrorMetadata {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExchangeError {
    pub message: String,
    pub name: String,
    pub metadata: Box<ExchangeErrorMetadata>,
}
impl ExchangeError {
    pub fn with_retained_response(mut self, response: RetainedJsonInput) -> Self {
        let cause = self
            .store_subscriber_cause
            .as_ref()
            .filter(|cause| std::error::Error::source(*cause).is_some())
            .cloned()
            .map(ExchangeErrorOrigin::new)
            .or_else(|| self.origin.take())
            .or_else(|| {
                self.store_subscriber_cause
                    .clone()
                    .map(ExchangeErrorOrigin::new)
            });
        self.origin = Some(ExchangeErrorOrigin::new(RetainedResponseOrigin {
            response,
            cause,
        }));
        self
    }
    pub fn retained_response_input(&self) -> Option<RetainedJsonInput> {
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(self);
        while let Some(error) = source {
            if let Some(origin) = error.downcast_ref::<RetainedResponseOrigin>() {
                return Some(origin.response.clone());
            }
            if let Some(failure) = error.downcast_ref::<safe_cex_transport::RestFailure>() {
                return failure.response.as_deref().map(retained_http_response);
            }
            if let Some(cause) = error.downcast_ref::<safe_cex_types::StoreSubscriberError>()
                && let Some(response) = cause.response()
            {
                return Some(response.clone().into());
            }
            source = error.source();
        }
        None
    }
    pub fn response_json_view(&self) -> Option<serde_json::Value> {
        self.retained_response_input().map(|response| {
            response_json_view(&safe_cex_transport::RestResponseBody::Transformed(response))
        })
    }
    // Error.prototype.toString differs from message-only Display at TS String(error) sites.
    pub fn js_error_string(&self) -> String {
        match (self.name.is_empty(), self.message.is_empty()) {
            (true, _) => self.message.clone(),
            (_, true) => self.name.clone(),
            _ => format!("{}: {}", self.name, self.message),
        }
    }
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            name: "Error".into(),
            metadata: Box::default(),
        }
    }
}
impl std::ops::Deref for ExchangeError {
    type Target = ExchangeErrorMetadata;
    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}
impl std::ops::DerefMut for ExchangeError {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.metadata
    }
}
impl std::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ExchangeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Some(origin) = self.origin.as_ref()
            && origin.error().is::<RetainedResponseOrigin>()
        {
            return Some(origin.error());
        }
        self.store_subscriber_cause
            .as_ref()
            .and_then(|cause| std::error::Error::source(cause))
            .or_else(|| self.origin.as_ref().map(ExchangeErrorOrigin::error))
            .or_else(|| {
                self.store_subscriber_cause
                    .as_ref()
                    .map(|cause| cause as &(dyn std::error::Error + 'static))
            })
    }
}
impl From<ExchangeError> for safe_cex_types::StoreSubscriberError {
    fn from(error: ExchangeError) -> Self {
        Self::new(error.message.clone()).with_cause(Rc::new(error))
    }
}
impl From<safe_cex_store::StoreMutationError> for ExchangeError {
    fn from(error: safe_cex_store::StoreMutationError) -> Self {
        // Store's opaque cause retains the original listener error across the
        // bottom-layer boundary without a dependency back into exchange core.
        if let safe_cex_store::StoreMutationError::Notification(notification) = &error
            && let Some(original) = std::error::Error::source(notification.cause())
                .and_then(|cause| cause.downcast_ref::<Self>())
        {
            let mut recovered = original.clone();
            if recovered.store_subscriber_cause.is_none() {
                recovered.store_subscriber_cause = Some(notification.cause().clone());
            }
            recovered
                .store_subscriber_ids
                .push(notification.subscriber_id());
            return recovered;
        }
        let mut projected = Self::new(error.to_string());
        if let safe_cex_store::StoreMutationError::Notification(notification) = &error {
            projected.store_subscriber_cause = Some(notification.cause().clone());
            projected
                .store_subscriber_ids
                .push(notification.subscriber_id());
        }
        projected.origin = Some(ExchangeErrorOrigin::new(error));
        projected
    }
}
impl From<safe_cex_num::JsCoercionError> for ExchangeError {
    fn from(error: safe_cex_num::JsCoercionError) -> Self {
        let mut projected = Self::new(error.message);
        projected.name = error.name.into();
        projected.origin = Some(ExchangeErrorOrigin::new(error));
        projected
    }
}
impl From<safe_cex_transport::RestFailure> for ExchangeError {
    fn from(error: safe_cex_transport::RestFailure) -> Self {
        let mut projected = if let Some(safe_cex_transport::TransportError::Rejected(origin)) =
            error.transport_error.as_deref()
            && let Some(cause) = origin
                .error()
                .downcast_ref::<crate::js_runtime::value::owned::OwnedCallbackError>()
        {
            crate::js_runtime::value::Thrown::new(cause.value().to_js()).to_exchange_error()
        } else {
            let mut projected = Self::new(error.message.clone());
            if let Some(safe_cex_transport::TransportError::JavaScript(cause)) =
                error.transport_error.as_deref()
            {
                projected.name = cause.name.into();
            }
            projected.code = error.code.as_ref().map(|code| Js::str(code));
            projected
        };
        projected.context = error.context.as_deref().cloned();
        projected.origin = Some(ExchangeErrorOrigin::new(error));
        projected
    }
}
fn retained_http_response(response: &safe_cex_transport::RestResponse) -> RetainedJsonInput {
    let data = match &response.body {
        safe_cex_transport::RestResponseBody::Encoded(bytes) => {
            let text = String::from_utf8_lossy(bytes)
                .encode_utf16()
                .collect::<Vec<_>>();
            RetainedJsonInput::parse_json(&text).unwrap_or(RetainedJsonInput::Text(text))
        }
        body => body.clone().into_data(),
    };
    let mut fields = vec![(
        "status".encode_utf16().collect(),
        RetainedJsonInput::Float(f64::from(response.status)),
    )];
    if let Some(reason) = &response.reason {
        fields.push((
            "statusText".encode_utf16().collect(),
            RetainedJsonInput::Text(reason.encode_utf16().collect()),
        ));
    }
    fields.push((
        "headers".encode_utf16().collect(),
        RetainedJsonInput::Object(
            response
                .headers
                .iter()
                .map(|(key, value)| {
                    (
                        key.encode_utf16().collect(),
                        RetainedJsonInput::Text(value.encode_utf16().collect()),
                    )
                })
                .collect(),
        ),
    ));
    fields.push(("data".encode_utf16().collect(), data));
    RetainedJsonInput::Object(fields)
}
// Deliberately lossy legacy JSON error view. The origin retains the authoritative
// RestFailure, including exact UTF-16, native number bits and transform phase.
pub fn response_json_view(body: &safe_cex_transport::RestResponseBody) -> serde_json::Value {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    fn utf8_view(value: Input) -> Input {
        match value {
            Input::Text(units) => {
                Input::Text(String::from_utf16_lossy(&units).encode_utf16().collect())
            }
            Input::Array(values) => Input::Array(values.into_iter().map(utf8_view).collect()),
            Input::Object(fields) => Input::Object(
                fields
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            String::from_utf16_lossy(&key).encode_utf16().collect(),
                            utf8_view(value),
                        )
                    })
                    .collect(),
            ),
            other => other,
        }
    }
    let data = match body {
        safe_cex_transport::RestResponseBody::Encoded(bytes) => {
            let text: Vec<u16> = String::from_utf8_lossy(bytes).encode_utf16().collect();
            // Only physical bytes receive this JSON view. Native strings keep
            // their existing value, and the original encoded body stays owned
            // by the RestFailure even when parsing this secondary view fails.
            Input::parse_json(&text).unwrap_or(Input::Text(text))
        }
        _ => body.clone().into_data(),
    };
    let value = utf8_view(data);
    let json = safe_cex_num::js_retained_json_stringify(&value).expect("retained tree JSON view");
    json.map_or(serde_json::Value::Null, |text| {
        serde_json::from_str(&text).expect("UTF-8 JSON error view")
    })
}

pub type ExchangeFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ExchangeError>> + 'a>>;
pub type Listener = Rc<dyn Fn(&str, &Js) -> Result<(), ExchangeError>>;
/// The actual emitted argument list, including zero arguments and array values.
pub type ArgsListener = Rc<dyn Fn(&str, &[Js]) -> Result<(), ExchangeError>>;
pub type Cleanup = Box<dyn FnOnce()>;
/// Cleanup of an event subscription can itself emit and fail.
pub type ListenerCleanup = Box<dyn FnOnce() -> Result<(), ExchangeError>>;
pub trait ExchangeTask {
    fn cancel(&self);
}
pub trait ExchangeRuntime {
    /// Elapsed milliseconds from a stable origin, on the same clock as sleep.
    /// Wall-clock adjustments must not affect this value.
    fn monotonic_now_ms(&self) -> f64;
    fn now_ms(&self) -> f64;
    /// Schedule on the monotonic clock; even zero delay yields a scheduler turn.
    fn sleep(&self, delay_ms: u64) -> ExchangeFuture<'static, ()>;
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask>;
    /// Original console arguments, including native Error references. Hosts must
    /// observe these values before choosing a terminal or telemetry representation.
    fn console_values(&self, severity: &str, values: &[Js]);
    /// Convenience for the existing typed Rust callers; there is one host path.
    fn console(&self, severity: &str, message: &str) {
        self.console_values(severity, &[Js::str(message)]);
    }
    fn unique_id(&self) -> String;
    fn is_worker(&self) -> bool;
    fn random(&self) -> f64;
    /// Must visibly report on every host; unawaited failures use this callback.
    /// Retained context preserves UTF-16, own undefined and native number bits.
    /// Context stays native until the reporting sink chooses its presentation.
    fn report_exception(
        &self,
        error: &ExchangeError,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    );
    /// Original exception, retaining native Error name/message and reference identity.
    /// A JSON projection here loses non-enumerable Error properties and identity.
    fn report_raw_exception(
        &self,
        error: &Js,
        context: &safe_cex_types::retained_json::RetainedJsonInput,
    );
    fn execute_worker_request(
        &self,
        request: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value>;
}
/// The live RemoteSigner contract extends signing with source availability callbacks.
pub trait RemoteSigner: safe_cex_transport::signing::RemoteSigner {
    fn is_available(&self) -> bool;
    fn on_availability_change(&self, callback: Rc<dyn Fn(bool)>) -> Cleanup;
}
#[derive(Default)]
pub struct ExchangeSigners {
    pub remote_signer: Option<Rc<dyn RemoteSigner>>,
    pub payload_signer: Option<Rc<dyn safe_cex_crypto::PayloadSigner>>,
}

/// Read diagnostic data without invoking user getters or conversion hooks.
fn diagnostic_string(value: &Js, key: &str) -> Option<String> {
    use crate::js_runtime::value::{Prop, utf16};
    let key = utf16(key);
    let mut current = value.clone();
    let mut seen = Vec::new();
    while let Js::Object(object) = current {
        if seen.iter().any(|seen| Rc::ptr_eq(seen, &object)) {
            return None;
        }
        seen.push(object.clone());
        if let Some(property) = object.props.borrow().get(&key) {
            return match property {
                Prop::Data(Js::String(text))
                | Prop::DataDescriptor {
                    value: Js::String(text),
                    ..
                } => Some(String::from_utf16_lossy(text)),
                _ => None,
            };
        }
        let prototype = object.proto.borrow().clone();
        current = prototype?;
    }
    None
}

/// Terminal presentation after the original values reach the console capability.
/// Object inspection never executes user code; native Error data remains visible.
pub fn console_text(values: &[Js]) -> String {
    values
        .iter()
        .map(|value| {
            if let Js::Object(object) = value {
                if object.class == "Error" {
                    let name = diagnostic_string(value, "name").unwrap_or_else(|| "Error".into());
                    let message = diagnostic_string(value, "message").unwrap_or_default();
                    return match (name.is_empty(), message.is_empty()) {
                        (true, _) => message,
                        (_, true) => name,
                        _ => format!("{name}: {message}"),
                    };
                }
            }
            if value.is_object() {
                format!("[{}]", value.type_of())
            } else {
                value.to_display()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Text-only fixture observation after receiving the retained exception. Native
/// production reporting uses console_text; rich sinks should retain Js directly.
pub fn exception_text(error: &Js) -> String {
    if let Js::Object(object) = error
        && object.class == "Error"
    {
        return diagnostic_string(error, "message").unwrap_or_default();
    }
    crate::js_runtime::value::to_json(error)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| console_text(std::slice::from_ref(error)))
}
