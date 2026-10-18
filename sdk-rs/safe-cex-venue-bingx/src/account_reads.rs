//! Account reads over the already-configured request/response boundary.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_num::{js_number_string, js_retained_string};
use safe_cex_transport::RestRequest;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde::Deserialize;
use serde_json::{Value, json};
use std::future::Future;

fn type_error(message: &str) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}

pub async fn fetch_primary_balance_data<F, R>(request: F) -> Result<Input, ExchangeError>
where
    F: FnOnce(RestRequest) -> R,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    let input = request(RestRequest {
        method: "GET".into(),
        url: crate::constants::ENDPOINTS[0].1.into(),
        ..Default::default()
    })
    .await?;
    let data = input
        .decode_with(Input::deserialize)
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    if data.is_null() {
        return Err(type_error(
            "Cannot read properties of null (reading 'code')",
        ));
    }
    let code = data.get("code");
    if code.and_then(Input::as_f64) != Some(0.0) {
        let message = if data.get("msg").is_some_and(Input::is_truthy) {
            js_retained_string(data.get("msg"))
        } else {
            js_retained_string(code)
                .map(|code| format!("BingX balance request failed with code {code}"))
        }
        .map_err(|error| type_error(error.message))?;
        return Err(ExchangeError::new(message));
    }
    let rows = match data.get("data") {
        None | Some(Input::Null | Input::Undefined) => &[][..],
        Some(Input::Array(rows)) => rows.as_slice(),
        _ => return Err(type_error("data.data?.find is not a function")),
    };
    let mut selected = None;
    for row in rows {
        if row.is_null() {
            return Err(type_error(
                "Cannot read properties of null (reading 'asset')",
            ));
        }
        if row.get("asset").is_some_and(|v| v.text_equals("USDT")) {
            selected = Some(row);
            break;
        }
    }
    let balance = selected.or_else(|| rows.first());
    if !balance.is_some_and(Input::is_truthy) {
        return Err(ExchangeError::new(
            "BingX balance response did not include an account balance",
        ));
    }
    Ok(balance.expect("truthy balance exists").clone())
}

pub async fn get_account<F, R>(request: F) -> Result<Value, ExchangeError>
where
    F: FnOnce(RestRequest) -> R,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    let balance = fetch_primary_balance_data(request).await?;
    let user_id = match balance.get("userId") {
        Some(value) if matches!(value, Input::Float(_) | Input::Lexeme(_)) => {
            js_number_string(Input::as_f64(value).expect("JSON number"))
        }
        value => js_retained_string(value).map_err(|error| type_error(error.message))?,
    };
    Ok(json!({"userId":user_id}))
}
