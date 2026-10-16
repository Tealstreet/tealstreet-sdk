use crate::request_weight::{Failure, type_error};
use safe_cex_num::{js_string, js_truthy};
use serde_json::Value;

/// Configured Error-instance discrimination and decoded error properties.
/// Native errors retain their original Rc; JSON/non-Error values retain optional
/// message values. Reference-carrying E must clone its handle, preserving the
/// original referent. Properties must describe that original token consistently.
/// Non-Error owned Value messages preserve values only, without object identity.
/// Executable error property accessors remain outside this input.
pub enum ExceptionValue<E = Failure> {
    Error { original: E, properties: Value },
    Other(Option<Value>),
}
pub enum ErrorEmission<E = Failure> {
    Error(E, Option<String>),
    Message(Option<Value>),
}

// ts: emitBinanceExchangeError
pub fn emit_binance_exchange_error<E: Clone>(
    exception: &ExceptionValue<E>,
    emit: impl FnOnce(ErrorEmission<E>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let (properties, original) = match exception {
        ExceptionValue::Error {
            original,
            properties,
        } => (Some(properties), Some(original)),
        ExceptionValue::Other(properties) => (properties.as_ref(), None),
    };
    let response_message = properties
        .and_then(|properties| properties.get("response"))
        .and_then(|response| response.get("data"))
        .and_then(|data| data.get("msg"));
    let message = response_message
        .filter(|message| js_truthy(Some(message)))
        .or_else(|| properties.and_then(|properties| properties.get("message")));
    if let Some(original) = original {
        let prefix = if js_truthy(message) {
            Some(format!(
                "Exchange Error: {}",
                js_string(message).map_err(|error| type_error(error.message))?
            ))
        } else {
            None
        };
        emit(ErrorEmission::Error(original.clone(), prefix))?;
        return Ok(());
    }
    emit(ErrorEmission::Message(message.cloned()))
}
