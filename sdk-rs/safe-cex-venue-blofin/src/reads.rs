//! Ticker reads against a configured shared REST adapter.
//! `createAPI` owns adapter configuration, signing and retry (not ported here).
use crate::{
    tickers::{TickerInput, format_tickers},
    types::ENDPOINTS,
};
use safe_cex_num::{
    js_truthy,
    numeric_ingress::{js_from_str, js_from_value_ref},
};
use safe_cex_store::Store;
use safe_cex_transport::{RestAdapter, RestRequest, RestResponseBody};
use safe_cex_types::Ticker;
use serde_json::Value;

/// An explicit response representation refusal is distinct from a source catch.
/// The original emitter error is retained without conversion or loss of identity.
#[derive(Debug)]
pub enum ReadError<E> {
    Ingress(serde_json::Error),
    Emitter(E),
}

fn body<E>(body: &RestResponseBody) -> Result<TickerInput, ReadError<E>> {
    // Encoded bodies use the declared JSON-only profile. Native adapter values
    // are already data: do not stringify or parse a transformed string again.
    match body {
        RestResponseBody::Encoded(bytes) => js_from_str(&String::from_utf8_lossy(bytes)),
        RestResponseBody::Untransformed(value) | RestResponseBody::Transformed(value) => value
            .clone()
            .into_value()
            .and_then(|value| js_from_value_ref(&value)),
    }
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
            Value::String("Cannot read properties of null (reading 'data')".into())
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
                .filter(|message| js_truthy(Some(message)))
                .cloned()
                .unwrap_or(Value::String(error.message))
        }
    };
    emit_and_log_error(&message).map_err(ReadError::Emitter)?;
    Ok(Some(TickerInput::from_retained(Value::Array(Vec::new()))))
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
        })
        .map_err(ReadError::Emitter),
        None => {
            emit_and_log_error(&Value::String(
                "Cannot read properties of undefined (reading 'reduce')".into(),
            ))
            .map_err(ReadError::Emitter)?;
            Ok(store.tickers().to_vec())
        }
    }
}
