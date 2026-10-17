//! Instrument joining over the two existing raw-read hooks.
use safe_cex_exchange::{js_runtime::value::from_json, runtime::ExchangeError};
use safe_cex_num::{
    js_number_output, js_truthy,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough, js_from_value},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    future::{Future, poll_fn},
    task::Poll,
};

#[derive(Serialize)]
#[serde(transparent)]
struct Raw {
    value: Value,
}
impl JsNumberOutput for Raw {
    fn check_number_output(&self) {
        js_number_output!(self, struct {} passthrough {value});
    }
}
fn raw(input: &JsNumberInput) -> Result<Value, ExchangeError> {
    input
        .clone()
        .decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value }))
        .map(|raw| raw.value)
        .map_err(|e| ExchangeError::new(e.to_string()))
}
fn combine(
    left: Option<JsNumberInput>,
    right: Option<JsNumberInput>,
) -> Result<JsNumberInput, ExchangeError> {
    let left_raw = left.as_ref().map(raw).transpose()?;
    let right_raw = right.as_ref().map(raw).transpose()?;
    if !js_truthy(left_raw.as_ref()) || !js_truthy(right_raw.as_ref()) {
        return js_from_value(json!([])).map_err(|e| ExchangeError::new(e.to_string()));
    }
    let left_raw = left_raw.unwrap();
    let right_raw = right_raw.unwrap();
    let details = right_raw
        .get("instrumentDetails")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ExchangeError::new("instrumentDetails.instrumentDetails.map is not a function")
        })?;
    let mut output = Vec::new();
    for detail in details {
        let instruments = left_raw
            .get("instruments")
            .and_then(Value::as_array)
            .ok_or_else(|| ExchangeError::new("instruments.instruments.find is not a function"))?;
        for instrument in instruments {
            let symbol = from_json(instrument)
                .get("symbol")
                .map_err(|e| e.to_exchange_error())?;
            let requested = from_json(detail)
                .get("symbol")
                .map_err(|e| e.to_exchange_error())?;
            if symbol.strict_equals(&requested) {
                output.push(json!({"instrument":instrument,"instrumentDetails":detail}));
                break;
            }
        }
    }
    js_from_value(Value::Array(output)).map_err(|e| ExchangeError::new(e.to_string()))
}

pub fn fetch_combined_instruments<L, R>(
    instruments: impl FnOnce() -> L,
    details: impl FnOnce() -> R,
) -> impl Future<Output = Result<JsNumberInput, ExchangeError>>
where
    L: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>,
    R: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>,
{
    // Both source calls occur before Promise.all suspends.
    let left = instruments();
    let right = details();
    async move {
        let mut left = std::pin::pin!(left);
        let mut right = std::pin::pin!(right);
        let mut left_value = None;
        let mut right_value = None;
        let (left, right) = poll_fn(|cx| {
            if left_value.is_none() {
                match left.as_mut().poll(cx) {
                    Poll::Ready(Ok(value)) => left_value = Some(value),
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => {}
                }
            }
            if right_value.is_none() {
                match right.as_mut().poll(cx) {
                    Poll::Ready(Ok(value)) => right_value = Some(value),
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => {}
                }
            }
            if left_value.is_some() && right_value.is_some() {
                Poll::Ready(Ok((
                    left_value.take().unwrap(),
                    right_value.take().unwrap(),
                )))
            } else {
                Poll::Pending
            }
        })
        .await?;
        combine(left, right)
    }
}
