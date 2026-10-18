//! Account validation method bodies; HTTP and best-effort balance callbacks are host supplied.
use crate::{spot_types, types, wire};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_string, js_truthy,
    numeric_ingress::{JsNumberInput, js_value_as_f64},
};
use safe_cex_types::{Balance, ValidateAccountResult};
use serde_json::{Value, json};
use std::future::Future;

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationRead {
    pub method: &'static str,
    pub endpoint: &'static str,
    pub body: Option<Value>,
}

fn result(error: impl Into<String>) -> ValidateAccountResult {
    ValidateAccountResult {
        error: error.into().into(),
        balance: None,
    }
}

fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}

fn first(value: Option<&Value>) -> Result<Option<Value>, ExchangeError> {
    match value {
        Some(Value::Array(rows)) => Ok(rows.first().cloned()),
        Some(Value::Object(object)) => Ok(object.get("0").cloned()),
        Some(Value::String(text)) => text
            .encode_utf16()
            .next()
            .map(|unit| {
                String::from_utf16(&[unit]).map(Value::String).map_err(|_| {
                    type_error("Validation error index is outside shared UTF-8 profile")
                })
            })
            .transpose(),
        _ => Ok(None),
    }
}

fn lower(value: &Value, expression: &str) -> Result<String, ExchangeError> {
    value
        .as_str()
        .map(str::to_lowercase)
        .ok_or_else(|| type_error(format!("{expression}.toLowerCase is not a function")))
}

fn status(error: &ExchangeError, expected: f64) -> bool {
    error
        .response_json_view()
        .as_ref()
        .and_then(|r| r.get("status"))
        .and_then(js_value_as_f64)
        == Some(expected)
}

fn futures_failure(error: ExchangeError) -> Result<ValidateAccountResult, ExchangeError> {
    // Captured fields have no getters: status wins before UTF-16 needs UTF-8 representation.
    for (code, text) in [
        (
            401.0,
            "Authentication failed (401). Please check your API key and secret are correct.",
        ),
        (
            403.0,
            "Access forbidden (403). Your API key may lack required permissions or your IP address is not whitelisted.",
        ),
        (
            429.0,
            "Rate limit exceeded (429). Please wait a moment and try again.",
        ),
        (
            400.0,
            "Bad request (400). Please verify your API credentials are in the correct format.",
        ),
    ] {
        if status(&error, code) {
            return Ok(result(text));
        }
    }
    let response = error.response_json_view();
    let data = response.as_ref().and_then(|r| r.get("data"));
    let error_field = data.and_then(|d| d.get("error"));
    let first_error = first(error_field)?;
    let message = first_error
        .as_ref()
        .filter(|v| js_truthy(Some(v)))
        .or_else(|| error_field.filter(|v| js_truthy(Some(v))))
        .or_else(|| {
            data.and_then(|d| d.get("message"))
                .filter(|v| js_truthy(Some(v)))
        });
    let fallback = Value::String(if error.message.is_empty() {
        "Failed to validate account".into()
    } else {
        error.message.clone()
    });
    let message = message.unwrap_or(&fallback);
    let text = lower(message, "errorMsg")?;
    if text.contains("network") || text.contains("timeout") {
        return Ok(result(
            "Network error. Please check your internet connection and try again.",
        ));
    }
    if text.contains("cors") {
        return Ok(result(
            "CORS error. Please enable \"Use Tealstreet IPs\" option.",
        ));
    }
    Ok(result(format!(
        "Validation failed: {}",
        message.as_str().expect("lower requires text")
    )))
}

fn spot_failure(error: ExchangeError) -> ValidateAccountResult {
    let message = if error.message.is_empty() {
        "Failed to validate account"
    } else {
        &error.message
    };
    let lower = message.to_lowercase();
    if status(&error, 401.0) || status(&error, 403.0) {
        return result("Authentication failed. Please check your API key and secret.");
    }
    if status(&error, 429.0) {
        return result("Rate limit exceeded. Please wait and try again.");
    }
    if lower.contains("network") || lower.contains("timeout") {
        return result("Network error. Please check your internet connection.");
    }
    result(format!("Validation failed: {message}"))
}

pub async fn validate_futures_account<F, B>(
    read: impl FnOnce(ValidationRead) -> Result<F, ExchangeError>,
    fetch_balance: impl FnOnce() -> Result<B, ExchangeError>,
) -> Result<ValidateAccountResult, ExchangeError>
where
    F: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>,
    B: Future<Output = Result<Balance, ExchangeError>>,
{
    let outcome: Result<ValidateAccountResult, ExchangeError> = async {
        let endpoint = types::ENDPOINTS.iter().find(|(key, _)| *key == "ACCOUNT").expect("account endpoint").1;
        let data = read(ValidationRead { method: "GET", endpoint, body: None })?.await?.map(wire::decode).transpose().map_err(|e| ExchangeError::new(e.to_string()))?;
        if data.as_ref().and_then(|d| d.get("result")).and_then(Value::as_str) != Some("success") {
            let first_error = first(data.as_ref().and_then(|d| d.get("error")))?;
            let fallback = Value::String("Account validation failed".into());
            let message = first_error.as_ref().filter(|v| js_truthy(Some(v)))
                .or_else(|| data.as_ref().and_then(|d| d.get("result")).filter(|v| js_truthy(Some(v))))
                .unwrap_or(&fallback);
            let lower_error = lower(message, "errorMsg")?;
            for (matches, text) in [
                (lower_error.contains("invalid key") || lower_error.contains("apikey"), "Invalid API key. Please verify your API key is correct and active."),
                (lower_error.contains("invalid nonce") || lower_error.contains("nonce"), "Invalid nonce. Please ensure your system time is synchronized (NTP)."),
                (lower_error.contains("permission") || lower_error.contains("denied"), "API key lacks required permissions. Please ensure General API is set to \"Full access\" (not \"Read only\") on your Kraken Futures API key."),
                (lower_error.contains("signature") || lower_error.contains("authentication"), "Invalid API secret or authentication failed. Please verify your API secret is correct."),
                (lower_error.contains("ip") || lower_error.contains("whitelist"), "IP address not whitelisted. Please add your IP to the API key whitelist on Kraken."),
            ] {
                if matches { return Ok(result(text)); }
            }
            return Ok(result(format!("Kraken API Error: {}", message.as_str().expect("lower requires text"))));
        }
        let accounts = data.as_ref().and_then(|d| d.get("accounts"));
        let count = match accounts {
            Some(Value::Object(object)) => object.len(),
            Some(Value::Array(array)) => array.len(),
            Some(Value::String(text)) => text.encode_utf16().count(),
            _ => 0,
        };
        if !js_truthy(accounts) || count == 0 {
            return Ok(result("No accounts found. API key may lack required permissions or account may be empty."));
        }
        let balance = fetch_balance()?.await.ok();
        Ok(ValidateAccountResult { error: "".into(), balance })
    }.await;
    match outcome {
        Ok(value) => Ok(value),
        Err(error) => futures_failure(error),
    }
}

pub async fn validate_spot_account<F, B>(
    read: impl FnOnce(ValidationRead) -> Result<F, ExchangeError>,
    fetch_balance: impl FnOnce() -> Result<B, ExchangeError>,
) -> Result<ValidateAccountResult, ExchangeError>
where
    F: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>,
    B: Future<Output = Result<Balance, ExchangeError>>,
{
    let outcome: Result<ValidateAccountResult, ExchangeError> = async {
        let endpoint = spot_types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "BALANCE")
            .expect("balance endpoint")
            .1;
        let data = read(ValidationRead {
            method: "POST",
            endpoint,
            body: Some(json!({})),
        })?
        .await?
        .map(wire::decode)
        .transpose()
        .map_err(|e| ExchangeError::new(e.to_string()))?;
        let errors = data.as_ref().and_then(|d| d.get("error"));
        let has_errors = match errors {
            Some(Value::Array(rows)) => !rows.is_empty(),
            Some(Value::String(text)) => !text.is_empty(),
            Some(Value::Object(object)) => js_truthy(object.get("length")),
            _ => false,
        };
        if has_errors {
            let rows = errors
                .and_then(Value::as_array)
                .ok_or_else(|| type_error("data.error.join is not a function"))?;
            let text = rows
                .iter()
                .map(|v| match v {
                    Value::Null => Ok(String::new()),
                    v => js_string(Some(v)).map_err(|e| type_error(e.message)),
                })
                .collect::<Result<Vec<_>, _>>()?
                .join("; ");
            let lower = text.to_lowercase();
            for (pattern, message) in [
                (
                    "invalid key",
                    "Invalid API key. Please verify your API key is correct and active.",
                ),
                (
                    "invalid nonce",
                    "Invalid nonce. Please ensure your system time is synchronized.",
                ),
                (
                    "invalid signature",
                    "Invalid API secret. Please verify your API secret is correct.",
                ),
                ("permission denied", "API key lacks required permissions."),
            ] {
                if lower.contains(pattern) {
                    return Ok(result(message));
                }
            }
            return Ok(result(format!("Kraken Spot API Error: {text}")));
        }
        let balance = fetch_balance()?.await.ok();
        Ok(ValidateAccountResult {
            error: "".into(),
            balance,
        })
    }
    .await;
    match outcome {
        Ok(value) => Ok(value),
        Err(error) => Ok(spot_failure(error)),
    }
}
