use crate::js_runtime::value::Js;
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
    pub response: Option<serde_json::Value>,
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
            && self.response == other.response
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
            projected.response = notification.cause().response().cloned();
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
        if let Some(safe_cex_transport::TransportError::Rejected(origin)) =
            error.transport_error.as_deref()
            && let Some(cause) = origin
                .error()
                .downcast_ref::<crate::js_runtime::value::owned::OwnedCallbackError>()
        {
            let mut projected =
                crate::js_runtime::value::Thrown::new(cause.value().to_js()).to_exchange_error();
            projected.origin = Some(ExchangeErrorOrigin::new(error));
            return projected;
        }
        let mut projected = Self::new(error.message.clone());
        if let Some(safe_cex_transport::TransportError::JavaScript(cause)) =
            error.transport_error.as_deref()
        {
            projected.name = cause.name.into();
        }
        projected.code = error.code.as_ref().map(|code| Js::str(code));
        projected.context = error.context.as_deref().cloned();
        projected.response = error.response.as_ref().map(|response| {
            // Existing JSON consumers receive a view. The retained RestFailure
            // remains the native authority for request, response and transport.
            let data = match &response.body {
                safe_cex_transport::RestResponseBody::Encoded(bytes) => {
                    serde_json::from_slice(bytes).unwrap_or_else(|_| {
                        serde_json::Value::String(String::from_utf8_lossy(bytes).into_owned())
                    })
                }
                safe_cex_transport::RestResponseBody::Untransformed(value)
                | safe_cex_transport::RestResponseBody::Transformed(value) => value.clone(),
            };
            let mut view = serde_json::Map::new();
            view.insert("status".into(), response.status.into());
            if let Some(reason) = &response.reason {
                view.insert("statusText".into(), reason.clone().into());
            }
            view.insert(
                "headers".into(),
                serde_json::to_value(&response.headers).expect("string response headers serialize"),
            );
            view.insert("data".into(), data);
            serde_json::Value::Object(view)
        });
        projected.origin = Some(ExchangeErrorOrigin::new(error));
        projected
    }
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
    /// Must visibly report on every host (console, telemetry or an error event).
    /// Production hosts must not swallow this callback: owned asynchronous work
    /// uses it for unawaited failures independently of transport diagnostics.
    fn report_exception(&self, error: &ExchangeError, context: &serde_json::Value);
    /// Original exception, retaining native Error name/message and reference identity.
    /// A JSON projection here loses non-enumerable Error properties and identity.
    fn report_raw_exception(&self, error: &Js, context: &serde_json::Value);
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
