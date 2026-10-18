//! Instrument joining over the two existing raw-read hooks.
use safe_cex_exchange::{js_runtime::value::from_retained_input, runtime::ExchangeError};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::{
    future::{Future, poll_fn},
    task::Poll,
};

fn raw(input: &JsNumberInput) -> Result<Input, ExchangeError> {
    input
        .clone()
        .decode()
        .map_err(|error| ExchangeError::new(error.to_string()))
}
fn combine(
    left: Option<JsNumberInput>,
    right: Option<JsNumberInput>,
) -> Result<JsNumberInput, ExchangeError> {
    let left_raw = left.as_ref().map(raw).transpose()?;
    let right_raw = right.as_ref().map(raw).transpose()?;
    if !left_raw.as_ref().is_some_and(Input::is_truthy)
        || !right_raw.as_ref().is_some_and(Input::is_truthy)
    {
        return Ok(Input::Array(Vec::new()).into());
    }
    let left_raw = left_raw.unwrap();
    let right_raw = right_raw.unwrap();
    let details = right_raw
        .get("instrumentDetails")
        .and_then(Input::as_array)
        .ok_or_else(|| {
            ExchangeError::new("instrumentDetails.instrumentDetails.map is not a function")
        })?;
    let mut output = Vec::new();
    for detail in details {
        let instruments = left_raw
            .get("instruments")
            .and_then(Input::as_array)
            .ok_or_else(|| ExchangeError::new("instruments.instruments.find is not a function"))?;
        for instrument in instruments {
            let symbol = from_retained_input(instrument.clone())
                .get("symbol")
                .map_err(|e| e.to_exchange_error())?;
            let requested = from_retained_input(detail.clone())
                .get("symbol")
                .map_err(|e| e.to_exchange_error())?;
            if symbol.strict_equals(&requested) {
                output.push(Input::Object(vec![
                    ("instrument".encode_utf16().collect(), instrument.clone()),
                    ("instrumentDetails".encode_utf16().collect(), detail.clone()),
                ]));
                break;
            }
        }
    }
    Ok(Input::Array(output).into())
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
