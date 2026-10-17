//! Account GET at the source transport and error-emission dependency boundaries.
use crate::{read_helpers::is_success_code, types::ENDPOINTS};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{js_string, js_truthy};
use serde_json::Value;
use std::future::Future;

pub fn fetch_balance_and_positions_raw<F, Fut, E>(
    get: F,
    emit_and_log_error: E,
) -> impl Future<Output = Result<Option<Value>, ExchangeError>>
where
    F: FnOnce(&'static str) -> Result<Fut, ExchangeError>,
    Fut: Future<Output = Result<Option<Value>, ExchangeError>>,
    E: FnOnce(Option<&Value>) -> Result<(), ExchangeError>,
{
    let endpoint = ENDPOINTS
        .iter()
        .find(|(key, _)| *key == "ACCOUNT")
        .expect("source endpoint")
        .1;
    let mut emit = Some(emit_and_log_error);
    let request = get(endpoint).map_err(|error| emit_account_error(error, emit.take().unwrap()));
    async move {
        let request = match request {
            Ok(request) => request,
            Err(result) => return result,
        };
        let attempt = async {
            let data = request.await?;
            if !is_success_code(data.as_ref().and_then(|data| data.get("code"))) {
                let message = data.as_ref().and_then(|data| data.get("msg"));
                let fallback = Value::String("Failed to fetch account".into());
                let message = js_string(if js_truthy(message) {
                    message
                } else {
                    Some(&fallback)
                })
                .map_err(|error| {
                    let mut result = ExchangeError::new(error.message);
                    result.name = error.name.into();
                    result
                })?;
                return Err(ExchangeError::new(message));
            }
            Ok(data)
        }
        .await;
        match attempt {
            Ok(data) => Ok(data),
            Err(error) => emit_account_error(error, emit.take().unwrap()),
        }
    }
}

fn emit_account_error(
    error: ExchangeError,
    emit: impl FnOnce(Option<&Value>) -> Result<(), ExchangeError>,
) -> Result<Option<Value>, ExchangeError> {
    let response_message = error
        .response
        .as_ref()
        .and_then(|response| response.get("data"))
        .and_then(|data| data.get("msg"));
    let message = Value::String(error.message.clone());
    emit(if js_truthy(response_message) {
        response_message
    } else {
        Some(&message)
    })?;
    Ok(None)
}

// A falsy response retains the current Store object after await. Holding a
// Store-wide Ref would prevent unrelated writes and balance replacement.
pub fn fetch_balance<'a, F, Fut, W>(
    store: &'a dyn safe_cex_store::Store,
    fetch_account: F,
    mut warn: W,
) -> impl Future<Output = Result<std::sync::Arc<safe_cex_types::Balance>, ExchangeError>> + 'a
where
    F: FnOnce() -> Result<Fut, ExchangeError> + 'a,
    W: FnMut(&str) + 'a,
    Fut: Future<Output = Result<Option<Value>, ExchangeError>> + 'a,
{
    let request = fetch_account();
    async move {
        let data = request?.await?;
        let stored = store.balance_handle();
        let parsed =
            crate::balance::parse_balance(data.as_ref(), &stored, &mut warn).map_err(|error| {
                let mut result = ExchangeError::new(error.message);
                result.name = error.name.into();
                result
            })?;
        Ok(match parsed {
            std::borrow::Cow::Owned(balance) => std::sync::Arc::new(balance),
            std::borrow::Cow::Borrowed(_) => stored,
        })
    }
}
