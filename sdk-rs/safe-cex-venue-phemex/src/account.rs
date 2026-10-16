//! Account read body; the caller supplies the configured xhr and error emitter.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, js_string};
use safe_cex_types::ExchangeAccount;
use serde_json::{Value, json};
use std::future::Future;

// ts: PhemexExchange.getAccount
// The read returns xhr.get response.data, after the caller's shared Axios pipeline.
// The error type is unchanged across the await and error-emitter boundaries.
pub async fn get_account<E, F>(
    read: impl FnOnce(&'static str, Value) -> F,
    emit_and_log_error: impl FnOnce(&str) -> Result<(), E>,
) -> Result<ExchangeAccount, E>
where
    E: From<ExchangeError> + From<JsCoercionError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let endpoint = crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == "ACCOUNT")
        .expect("Phemex ACCOUNT constant is present")
        .1;
    let data = read(endpoint, json!({"currency": "USDT"})).await?;
    let user_id = data
        .as_ref()
        .and_then(|value| value.get("data"))
        .and_then(|value| value.get("account"))
        .and_then(|value| value.get("userID"));
    let Some(user_id) = user_id.filter(|value| !value.is_null()) else {
        let message = "Phemex account-position response did not include a user ID";
        emit_and_log_error(message)?;
        return Err(ExchangeError::new(message).into());
    };
    Ok(ExchangeAccount {
        user_id: js_string(Some(user_id))?,
        sub_id: None,
        affiliate_id: None,
    })
}
