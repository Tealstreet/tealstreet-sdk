//! Maintained account reader bodies with explicit transport/store dependencies.
//! Exchange construction and lifecycle composition are separate pending rows.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::MarginType;
use serde_json::{Map, Value};
use std::{future::Future, rc::Rc};

fn response_error(message: Option<&Value>) -> Rc<ExchangeError> {
    // Error(undefined) has an empty message; Error(null) spells "null".
    let message = match message {
        None => String::new(),
        Some(value) => match safe_cex_num::js_string(Some(value)) {
            Ok(message) => message,
            Err(error) => {
                let mut native = ExchangeError::new(error.message);
                native.name = "TypeError".into();
                return Rc::new(native);
            }
        },
    };
    Rc::new(ExchangeError::new(message))
}

pub async fn get_account<F: Future<Output = Result<Map<String, Value>, Rc<ExchangeError>>>>(
    get: impl FnOnce(&str) -> F,
) -> Result<(Option<Value>, Option<Value>), Rc<ExchangeError>> {
    let data = get(crate::types::ENDPOINTS["ACCOUNT"].as_str().unwrap()).await?;
    // No coercion: a string "0" and boolean false are failures in the source.
    if data.get("retCode").and_then(Value::as_f64) != Some(0.) {
        return Err(response_error(data.get("retMsg")));
    }
    let result = data.get("result");
    Ok((
        result.and_then(|value| value.get("userID")).cloned(),
        result.and_then(|value| value.get("inviterID")).cloned(),
    ))
}

pub async fn fetch_margin_account_infos<
    F: Future<Output = Result<Map<String, Value>, Rc<ExchangeError>>>,
>(
    get: impl FnOnce(&str) -> F,
    emit_and_log_error: impl FnOnce(Option<&Value>) -> Result<(), Rc<ExchangeError>>,
    set_margin_type: impl FnOnce(MarginType) -> Result<(), Rc<ExchangeError>>,
    set_unified_margin_status: impl FnOnce(Option<Value>) -> Result<(), Rc<ExchangeError>>,
) -> Result<(), Rc<ExchangeError>> {
    let data = get(crate::types::ENDPOINTS["ACCOUNT_MARGIN"].as_str().unwrap()).await?;
    if data.get("retMsg").and_then(Value::as_str) != Some("OK") {
        emit_and_log_error(data.get("retMsg"))?;
        return Err(response_error(data.get("retMsg")));
    }
    let result = data.get("result");
    let margin_type = if result
        .and_then(|value| value.get("marginMode"))
        .and_then(Value::as_str)
        == Some("ISOLATED_MARGIN")
    {
        MarginType::Isolated
    } else {
        MarginType::Cross
    };
    set_margin_type(margin_type)?;
    set_unified_margin_status(
        result
            .and_then(|value| value.get("unifiedMarginStatus"))
            .cloned(),
    )?;
    Ok(())
}

pub async fn spot_fetch_margin_account_infos<
    F: Future<Output = Result<Map<String, Value>, Rc<ExchangeError>>>,
>(
    get: impl FnOnce(&str) -> F,
    emit_and_log_error: impl FnOnce(Option<&Value>) -> Result<(), Rc<ExchangeError>>,
    set_unified_margin_status: impl FnOnce(Option<Value>) -> Result<(), Rc<ExchangeError>>,
) -> Result<(), Rc<ExchangeError>> {
    let data = get(crate::types::ENDPOINTS["ACCOUNT_MARGIN"].as_str().unwrap()).await?;
    if data.get("retMsg").and_then(Value::as_str) != Some("OK") {
        emit_and_log_error(data.get("retMsg"))?;
        return Err(response_error(data.get("retMsg")));
    }
    set_unified_margin_status(
        data.get("result")
            .and_then(|value| value.get("unifiedMarginStatus"))
            .cloned(),
    )?;
    Ok(())
}

pub async fn fetch_balance<B, R, E, F: Future<Output = Result<R, E>>>(
    fetch_raw_balance: impl FnOnce() -> F,
    is_disposed: impl FnOnce() -> bool,
    stored_balance: impl FnOnce() -> B,
    format_balance: impl FnOnce(R) -> Result<B, E>,
) -> Result<B, E> {
    let raw_balance = fetch_raw_balance().await?;
    if is_disposed() {
        return Ok(stored_balance());
    }
    format_balance(raw_balance)
}

pub async fn hyro_fetch_margin_account_infos<B, E, F: Future<Output = Result<B, E>>>(
    fetch_balance: impl FnOnce() -> F,
    is_disposed: impl FnOnce() -> bool,
    update_balance: impl FnOnce(B) -> Result<(), E>,
) -> Result<(), E> {
    let balance = fetch_balance().await?;
    if !is_disposed() {
        update_balance(balance)?;
    }
    Ok(())
}
