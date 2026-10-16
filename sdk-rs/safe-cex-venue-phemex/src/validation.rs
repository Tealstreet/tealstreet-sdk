//! validateAccount's configured-read body, including its caught error projection.
use safe_cex_num::{JsCoercionError, js_string, js_truthy, parse_float};
use safe_cex_types::{AccountValidationError, Balance, ValidateAccountResult};
use serde_json::{Value, json};
use std::future::Future;

// ts: PhemexExchange.validateAccount
// Read supplies parsed xhr response.data or the caught JSON error value.
// Notification failures from the catch body's emitter escape unchanged.
pub async fn validate_account<E, F>(
    read: impl FnOnce(&'static str, Value) -> F,
    emit_and_log_error: impl FnOnce(Option<&Value>) -> Result<(), E>,
) -> Result<ValidateAccountResult, E>
where
    F: Future<Output = Result<Option<Value>, Value>>,
{
    let endpoint = crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == "ACCOUNT")
        .expect("Phemex ACCOUNT constant is present")
        .1;
    let attempt = match read(endpoint, json!({"currency": "USDT"})).await {
        Ok(data) => balance(data.as_ref())
            .map_err(|error| json!({"name":error.name,"message":error.message})),
        Err(error) => Err(error),
    };
    match attempt {
        Ok(balance) => Ok(ValidateAccountResult {
            error: "".into(),
            balance,
        }),
        Err(error) => {
            let data = error.get("response").and_then(|v| v.get("data"));
            let message = data.and_then(|v| v.get("message"));
            let message = if js_truthy(message) {
                message
            } else {
                error.get("message")
            };
            emit_and_log_error(message)?;
            let msg = data.and_then(|v| v.get("msg"));
            let msg = if js_truthy(msg) {
                msg
            } else {
                error.get("message")
            };
            let error = if js_truthy(msg) {
                msg.expect("truthy value exists").clone()
            } else {
                Value::String("Invalid API key or secret".into())
            };
            Ok(ValidateAccountResult {
                error: AccountValidationError::from_value(error),
                balance: None,
            })
        }
    }
}

fn balance(data: Option<&Value>) -> Result<Option<Balance>, JsCoercionError> {
    let account = data
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("account"));
    let total = parse_float(&js_string(account.and_then(|v| v.get("accountBalanceRv")))?);
    if !total.is_finite() {
        return Ok(None);
    }
    let used = parse_float(&js_string(
        account.and_then(|v| v.get("totalUsedBalanceRv")),
    )?);
    let used = if used.is_nan() || used == 0.0 {
        0.0
    } else {
        used
    };
    Ok(Some(Balance {
        total,
        used,
        free: total - used,
        upnl: 0.0,
        info: None,
        currencies: None,
        coins: None,
    }))
}
