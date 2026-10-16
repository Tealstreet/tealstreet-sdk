//! Private-account metadata and validation bodies with explicit host dependencies.
use safe_cex_exchange::runtime::ExchangeError;
use serde_json::{Map, Value};
use std::{future::Future, rc::Rc};

fn native_error(name: &str, message: impl Into<String>) -> Rc<Value> {
    Rc::new(serde_json::json!({"name":name,"message":message.into()}))
}
fn string(value: Option<&Value>) -> Result<String, Rc<Value>> {
    safe_cex_num::js_string(value).map_err(|e| native_error("TypeError", e.message))
}
fn error_from_message(value: Option<&Value>) -> Rc<Value> {
    match value {
        None => native_error("Error", ""),
        Some(value) => match string(Some(value)) {
            Ok(message) => native_error("Error", message),
            Err(error) => error,
        },
    }
}
fn caught_field(error: &Value, last: &str) -> Option<Value> {
    error
        .get("response")
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get(last))
        .filter(|v| safe_cex_num::js_truthy(Some(v)))
        .or_else(|| error.get("message"))
        .cloned()
}
fn caught_result(error: &Value) -> Option<Value> {
    caught_field(error, "msg")
        .filter(|v| safe_cex_num::js_truthy(Some(v)))
        .or_else(|| Some(Value::String("Invalid API key or secret".into())))
}
fn includes_timestamp(value: Option<&Value>) -> Result<bool, Rc<Value>> {
    const NEEDLE: &str = "timestamp and recv_window param";
    match value {
        Some(Value::String(message)) => Ok(message.contains(NEEDLE)),
        Some(Value::Array(values)) => Ok(values.iter().any(|v| v.as_str() == Some(NEEDLE))),
        Some(Value::Null) => Err(native_error(
            "TypeError",
            "Cannot read properties of null (reading 'includes')",
        )),
        None => Err(native_error(
            "TypeError",
            "Cannot read properties of undefined (reading 'includes')",
        )),
        _ => Err(native_error(
            "TypeError",
            "data.retMsg.includes is not a function",
        )),
    }
}

/// Tuple preserves the anonymous result's error and optional balance independently.
#[allow(clippy::too_many_arguments)]
async fn validate_account_impl<
    B,
    Request: Future<Output = Result<Map<String, Value>, Rc<Value>>>,
    Balance: Future<Output = Result<B, Rc<Value>>>,
>(
    get: impl FnOnce(&str) -> Request,
    fetch_balance: impl FnOnce() -> Result<Balance, Rc<Value>>,
    mut emit_and_log_error: impl FnMut(Option<Value>) -> Result<(), Rc<Value>>,
    balance_inside_validation_try: bool,
) -> Result<(Option<Value>, Option<B>), Rc<Value>> {
    let validation: Result<Option<Option<Value>>, Rc<Value>> = async {
        let data = get(crate::types::ENDPOINTS["ACCOUNT_MARGIN"].as_str().unwrap()).await?;
        if data.get("retMsg").and_then(Value::as_str) != Some("OK") {
            emit_and_log_error(data.get("retMsg").cloned())?;
            if includes_timestamp(data.get("retMsg"))? {
                return Ok(Some(Some(Value::String(
                    "Check your computer time and date".into(),
                ))));
            }
            return Ok(Some(data.get("retMsg").cloned()));
        }
        Ok(None)
    }
    .await;
    match validation {
        Err(error) => {
            emit_and_log_error(caught_field(&error, "message"))?;
            return Ok((caught_result(&error), None));
        }
        Ok(Some(error)) => return Ok((error, None)),
        Ok(None) => {}
    }
    let balance = match fetch_balance() {
        Ok(request) => request.await.ok(),
        Err(error) if balance_inside_validation_try => {
            emit_and_log_error(caught_field(&error, "message"))?;
            return Ok((caught_result(&error), None));
        }
        Err(_) => None,
    };
    Ok((Some(Value::String(String::new())), balance))
}

pub async fn validate_account<B, Request, Balance>(
    get: impl FnOnce(&str) -> Request,
    fetch_balance: impl FnOnce() -> Result<Balance, Rc<Value>>,
    emit_and_log_error: impl FnMut(Option<Value>) -> Result<(), Rc<Value>>,
) -> Result<(Option<Value>, Option<B>), Rc<Value>>
where
    Request: Future<Output = Result<Map<String, Value>, Rc<Value>>>,
    Balance: Future<Output = Result<B, Rc<Value>>>,
{
    validate_account_impl(get, fetch_balance, emit_and_log_error, false).await
}

pub async fn validate_derived_account<B, Request, Balance>(
    get: impl FnOnce(&str) -> Request,
    fetch_balance: impl FnOnce() -> Result<Balance, Rc<Value>>,
    emit_and_log_error: impl FnMut(Option<Value>) -> Result<(), Rc<Value>>,
) -> Result<(Option<Value>, Option<B>), Rc<Value>>
where
    Request: Future<Output = Result<Map<String, Value>, Rc<Value>>>,
    Balance: Future<Output = Result<B, Rc<Value>>>,
{
    validate_account_impl(get, fetch_balance, emit_and_log_error, true).await
}

pub async fn hyro_validate_account<B, F: Future<Output = Result<B, Rc<ExchangeError>>>>(
    fetch_balance: impl FnOnce() -> Result<F, Rc<ExchangeError>>,
    emit_and_log_error: impl FnOnce(&str) -> Result<(), Rc<ExchangeError>>,
) -> Result<(String, Option<B>), Rc<ExchangeError>> {
    let result = match fetch_balance() {
        Ok(future) => future.await,
        Err(error) => Err(error),
    };
    match result {
        Ok(balance) => Ok((String::new(), Some(balance))),
        Err(error) => {
            emit_and_log_error(&error.message)?;
            Ok((error.message.clone(), None))
        }
    }
}

pub async fn fetch_api_key_info<F: Future<Output = Result<Rc<Value>, Rc<Value>>>>(
    get: impl FnOnce(&str) -> F,
    emit_and_log_error: impl FnOnce(Option<Value>) -> Result<(), Rc<Value>>,
    set_api_key_info: impl FnOnce(Option<Rc<Value>>) -> Result<(), Rc<Value>>,
) -> Result<Option<Rc<Value>>, Rc<Value>> {
    let data = get(crate::types::ENDPOINTS["ACCOUNT"].as_str().unwrap()).await?;
    let code =
        crate::json_field::field(&data, "retCode").map_err(|m| native_error("TypeError", m))?;
    let code = safe_cex_num::js_number(code).map_err(|e| native_error("TypeError", e.message))?;
    if code != 0. {
        let message =
            crate::json_field::field(&data, "retMsg").map_err(|m| native_error("TypeError", m))?;
        emit_and_log_error(message.cloned())?;
        return Err(error_from_message(message));
    }
    let info = data.get("result").cloned().map(Rc::new);
    set_api_key_info(info.clone())?;
    Ok(info)
}

#[allow(clippy::too_many_arguments)]
async fn cache_api_key_info_impl<F: Future<Output = Result<(), Rc<Value>>>>(
    fetch_api_key_info: impl FnOnce() -> Result<F, Rc<Value>>,
    mut cached_info: impl FnMut() -> Option<Rc<Value>>,
    update_batch_limit: impl FnOnce() -> Result<(), Rc<Value>>,
    update_user_data: impl FnOnce(&str, Option<Rc<Value>>) -> Result<(), Rc<Value>>,
    batch_limit: impl FnOnce() -> f64,
    mut log: impl FnMut(&str) -> Result<(), Rc<Value>>,
    user_data_key: &str,
) -> Result<(), Rc<Value>> {
    let operation = async {
        fetch_api_key_info()?.await?;
        if safe_cex_num::js_truthy(cached_info().as_deref()) {
            update_batch_limit()?;
            update_user_data(user_data_key, cached_info())?;
        }
        let info = cached_info();
        let vip = info.as_deref().and_then(|v| v.get("vipLevel"));
        let vip = if safe_cex_num::js_truthy(vip) {
            string(vip)?
        } else {
            "Unknown".into()
        };
        log(&format!(
            "API key info cached. VIP Level: {vip}, Batch Limit: {}",
            safe_cex_num::js_number_string(batch_limit())
        ))?;
        Ok::<(), Rc<Value>>(())
    }
    .await;
    if operation.is_err() {
        log("Failed to fetch API key info, continuing without it")?;
    }
    Ok(())
}

pub async fn fetch_and_cache_api_key_info<F: Future<Output = Result<(), Rc<Value>>>>(
    fetch: impl FnOnce() -> Result<F, Rc<Value>>,
    cached: impl FnMut() -> Option<Rc<Value>>,
    update_batch: impl FnOnce() -> Result<(), Rc<Value>>,
    update_user: impl FnOnce(&str, Option<Rc<Value>>) -> Result<(), Rc<Value>>,
    batch_limit: impl FnOnce() -> f64,
    log: impl FnMut(&str) -> Result<(), Rc<Value>>,
) -> Result<(), Rc<Value>> {
    cache_api_key_info_impl(
        fetch,
        cached,
        update_batch,
        update_user,
        batch_limit,
        log,
        "bybitApiKeyInfo",
    )
    .await
}

pub async fn fetch_and_cache_inverse_api_key_info<F: Future<Output = Result<(), Rc<Value>>>>(
    fetch: impl FnOnce() -> Result<F, Rc<Value>>,
    cached: impl FnMut() -> Option<Rc<Value>>,
    update_batch: impl FnOnce() -> Result<(), Rc<Value>>,
    update_user: impl FnOnce(&str, Option<Rc<Value>>) -> Result<(), Rc<Value>>,
    batch_limit: impl FnOnce() -> f64,
    log: impl FnMut(&str) -> Result<(), Rc<Value>>,
) -> Result<(), Rc<Value>> {
    cache_api_key_info_impl(
        fetch,
        cached,
        update_batch,
        update_user,
        batch_limit,
        log,
        "bybitinverseApiKeyInfo",
    )
    .await
}

/// VIP is the source's declared string field; getters preserve every reread.
pub fn update_batch_limit_based_on_vip_level<E>(
    mut vip_level: impl FnMut() -> Result<Option<String>, E>,
    account_type: impl FnOnce() -> Result<String, E>,
    mut batch_limit: impl FnMut() -> f64,
    set_batch_limit: impl FnOnce(f64) -> Result<(), E>,
    log: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    if vip_level()?.as_deref().is_none_or(str::is_empty) {
        return Ok(());
    }
    let mut found = None;
    for entry in crate::rate_limits::BYBIT_RATE_LIMIT_TABLE.iter() {
        use crate::rate_limits::BybitVipLevel::*;
        let level = match entry.level {
            NoVIP => "No VIP",
            VIP1 => "VIP-1",
            VIP2 => "VIP-2",
            VIP3 => "VIP-3",
            VIP4 => "VIP-4",
            VIP5 => "VIP-5",
            VIPSupreme => "VIP-Supreme",
            PRO1 => "PRO-1",
            PRO2 => "PRO-2",
            PRO3 => "PRO-3",
            PRO4 => "PRO-4",
            PRO5 => "PRO-5",
            PRO6 => "PRO-6",
        };
        if Some(level) == vip_level()?.as_deref() {
            found = Some(entry);
            break;
        }
    }
    let Some(entry) = found else {
        let vip = vip_level()?.unwrap_or_else(|| "undefined".into());
        return log(&format!("No rate limit entry found for VIP level: {vip}"));
    };
    let rate = if account_type()? == "UNIFIED" {
        entry.futures.uta_pro
    } else {
        entry.futures.classic_and_uta
    };
    if rate >= 20. {
        set_batch_limit(20.)?;
        let vip = vip_level()?.unwrap_or_else(|| "undefined".into());
        log(&format!(
            "VIP level {vip} allows {} req/s for futures. Setting batch limit to 20.",
            safe_cex_num::js_number_string(rate)
        ))?;
    } else {
        let vip = vip_level()?.unwrap_or_else(|| "undefined".into());
        log(&format!(
            "VIP level {vip} allows {} req/s for futures. Keeping batch limit at {}.",
            safe_cex_num::js_number_string(rate),
            safe_cex_num::js_number_string(batch_limit())
        ))?;
    }
    Ok(())
}
