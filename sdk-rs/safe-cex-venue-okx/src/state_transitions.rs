use safe_cex_types::PositionMode;
use serde_json::Value;
use std::future::Future;

pub async fn fetch_account_market_overlay() -> Value {
    Value::Null
}

pub async fn get_or_fetch_position_mode<E, F: Future<Output = Result<(PositionMode, f64), E>>>(
    force: Option<bool>,
    get_config: impl FnOnce(bool) -> F,
) -> Result<PositionMode, E> {
    let (position_mode, _) = get_config(force.unwrap_or(false)).await?;
    Ok(position_mode)
}

pub async fn get_or_fetch_account_level<E, F: Future<Output = Result<(PositionMode, f64), E>>>(
    force: Option<bool>,
    get_config: impl FnOnce(bool) -> F,
) -> Result<f64, E> {
    let (_, account_level) = get_config(force.unwrap_or(false)).await?;
    Ok(account_level)
}

pub async fn change_margin_type<E>(
    emit_and_log_error: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    emit_and_log_error("Only cross mode is supported")?;
    Ok(())
}
