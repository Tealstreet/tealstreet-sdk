//! Ticker reads against a configured shared REST adapter.
//! `createAPI` owns adapter configuration, signing and retry (not ported here).
use crate::{
    tickers::{TickerInput, format_tickers},
    types::ENDPOINTS,
};
use safe_cex_store::Store;
use safe_cex_transport::{RestAdapter, RestRequest, RestResponseBody};
use safe_cex_types::{Ticker, retained_json::RetainedJsonInput as Input};
use serde_json::Value;

/// An explicit response representation refusal is distinct from a source catch.
/// The original emitter error is retained without conversion or loss of identity.
#[derive(Debug)]
pub enum ReadError<E> {
    Store(safe_cex_types::StoreReadError),
    Ingress(serde_json::Error),
    Emitter(E),
}

/// Model mapping can reject a closed field or fail while reading live Store data.
#[derive(Debug)]
pub enum MappingError {
    Coercion(safe_cex_num::JsCoercionError),
    Store(safe_cex_types::StoreReadError),
    Input(String),
}
impl From<safe_cex_num::JsCoercionError> for MappingError {
    fn from(error: safe_cex_num::JsCoercionError) -> Self {
        Self::Coercion(error)
    }
}
impl From<safe_cex_types::StoreReadError> for MappingError {
    fn from(error: safe_cex_types::StoreReadError) -> Self {
        Self::Store(error)
    }
}
impl From<String> for MappingError {
    fn from(error: String) -> Self {
        Self::Input(error)
    }
}
impl From<&str> for MappingError {
    fn from(error: &str) -> Self {
        Self::Input(error.into())
    }
}
impl std::fmt::Display for MappingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coercion(error) => formatter.write_str(error.message),
            Self::Store(error) => error.fmt(formatter),
            Self::Input(error) => formatter.write_str(error),
        }
    }
}
impl From<MappingError> for safe_cex_exchange::runtime::ExchangeError {
    fn from(error: MappingError) -> Self {
        match error {
            MappingError::Coercion(error) => error.into(),
            MappingError::Store(error) => error.into(),
            MappingError::Input(error) => Self::new(error),
        }
    }
}

fn body<E>(body: &RestResponseBody) -> Result<TickerInput, ReadError<E>> {
    // Parse only encoded wire bytes. Native transformed data remains canonical
    // Input through TickerInput and the final model info capture.
    match body {
        RestResponseBody::Encoded(bytes) => Input::parse_json(
            &String::from_utf8_lossy(bytes)
                .encode_utf16()
                .collect::<Vec<_>>(),
        ),
        RestResponseBody::Untransformed(value) | RestResponseBody::Transformed(value) => {
            Ok(value.clone())
        }
    }
    .map(TickerInput::from_retained)
    .map_err(ReadError::Ingress)
}

/// `None` is JavaScript undefined, distinct from a present JSON null.
pub async fn fetch_tickers_raw<E>(
    xhr: &dyn RestAdapter,
    mut emit_and_log_error: impl FnMut(&Value) -> Result<(), E>,
) -> Result<Option<TickerInput>, ReadError<E>> {
    let request = RestRequest {
        method: "GET".into(),
        url: ENDPOINTS["TICKERS"]
            .as_str()
            .expect("static endpoint")
            .into(),
        ..Default::default()
    };
    let message = match xhr.execute(request).await {
        Ok(response) => {
            let data = body::<E>(&response.body)?;
            if !data.retained().is_null() {
                return Ok(data
                    .retained()
                    .get("data")
                    .cloned()
                    .map(TickerInput::from_retained));
            }
            Input::from(Value::String(
                "Cannot read properties of null (reading 'data')".into(),
            ))
        }
        Err(error) => {
            let response = error
                .response
                .as_ref()
                .map(|r| body::<E>(&r.body))
                .transpose()?;
            response
                .as_ref()
                .and_then(|r| r.retained().get("msg"))
                .filter(|message| message.is_truthy())
                .cloned()
                .unwrap_or_else(|| Input::from(Value::String(error.message)))
        }
    };
    // The existing generic reporter consumes an explicit JSON-view message.
    let message = message.into_value().map_err(ReadError::Ingress)?;
    emit_and_log_error(&message).map_err(ReadError::Emitter)?;
    Ok(Some(TickerInput::from_retained(Input::Array(Vec::new()))))
}

pub async fn fetch_tickers<E>(
    xhr: &dyn RestAdapter,
    store: &dyn Store,
    mut emit_and_log_error: impl FnMut(&Value) -> Result<(), E>,
) -> Result<Vec<Ticker>, ReadError<E>> {
    let raw_tickers = fetch_tickers_raw(xhr, &mut emit_and_log_error).await?;
    match raw_tickers {
        Some(data) => format_tickers(&data, store, |message| {
            emit_and_log_error(&Value::String(message.into()))
        }),
        None => {
            emit_and_log_error(&Value::String(
                "Cannot read properties of undefined (reading 'reduce')".into(),
            ))
            .map_err(ReadError::Emitter)?;
            Ok(store.tickers()?.to_vec())
        }
    }
}

impl<E> From<safe_cex_types::StoreReadError> for ReadError<E> {
    fn from(error: safe_cex_types::StoreReadError) -> Self {
        Self::Store(error)
    }
}
