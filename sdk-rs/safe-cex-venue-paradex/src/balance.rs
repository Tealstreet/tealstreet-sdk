//! fetchBalance's existing xhr, logger and JavaScript error capabilities.
use crate::constants::ENDPOINTS;
use safe_cex_num::{JsCoercionError, js_string, js_truthy, math_max, parse_float};
use safe_cex_types::Balance;
use serde_json::Value;
use std::future::Future;

// ts: ParadexExchange.fetchBalance
pub struct BalanceEffects<L, S, D, N, C> {
    pub log: L,
    pub stringify_error: S,
    pub describe_error: D,
    pub missing_data_error: N,
    pub coercion_error: C,
}

// ts: ParadexExchange.fetchBalance
pub async fn fetch_balance<E, I, F>(
    sdk_init: Option<I>,
    has_jwt_manager: impl FnOnce() -> bool,
    get: impl FnOnce(&'static str) -> Result<F, E>,
    effects: &mut BalanceEffects<
        impl FnMut(String, &'static str) -> Result<(), E>,
        impl FnMut(&E) -> Result<String, E>,
        impl FnMut(&E) -> Result<Option<String>, E>,
        impl FnMut() -> E,
        impl FnMut(JsCoercionError) -> E,
    >,
) -> Result<Balance, E>
where
    I: Future<Output = Result<(), E>>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let result = async {
        if let Some(init) = sdk_init {
            init.await?;
        }
        if !has_jwt_manager() {
            (effects.log)("Cannot fetch balance: JWT manager not initialized".into(), "warning")?;
            return Ok(Balance { total: 0.0, free: 0.0, used: 0.0, upnl: 0.0, info: None, currencies: None, coins: None });
        }
        (effects.log)("Fetching account balance from API...".into(), "debug")?;
        let endpoint = ENDPOINTS.iter().find(|(key, _)| *key == "ACCOUNT").unwrap().1;
        let data = get(endpoint)?.await?;
        let account = data.as_ref().and_then(|v| v.get("data")).filter(|v| !v.is_null()).or(data.as_ref());
        if !js_truthy(account) {
            (effects.log)("Balance API returned invalid data".into(), "error")?;
            return Err((effects.missing_data_error)());
        }
        let account = account.unwrap();
        let read_number = |keys: &[&str]| -> Result<Option<f64>, JsCoercionError> {
            for key in keys {
                let Some(value) = account.get(*key).filter(|v| !v.is_null()) else { continue; };
                let parsed = parse_float(&js_string(Some(value))?);
                if parsed.is_finite() { return Ok(Some(parsed)); }
            }
            Ok(None)
        };
        let total = read_number(&["total_collateral", "account_value", "equity", "margin_balance", "wallet_balance"])
            .map_err(&mut effects.coercion_error)?.unwrap_or(0.0);
        let free = read_number(&["free_collateral", "free", "available_margin", "available_collateral"])
            .map_err(&mut effects.coercion_error)?.unwrap_or(0.0);
        let used = read_number(&["initial_margin_requirement", "position_margin", "used_margin"])
            .map_err(&mut effects.coercion_error)?.unwrap_or_else(|| math_max(total - free, 0.0));
        let text = |key| js_string(account.get(key));
        let response = format!(
            "Balance API response: total_collateral={}, account_value={}, equity={}, free_collateral={}, position_margin={}, initial_margin_requirement={}",
            text("total_collateral").map_err(&mut effects.coercion_error)?,
            text("account_value").map_err(&mut effects.coercion_error)?,
            text("equity").map_err(&mut effects.coercion_error)?,
            text("free_collateral").map_err(&mut effects.coercion_error)?,
            text("position_margin").map_err(&mut effects.coercion_error)?,
            text("initial_margin_requirement").map_err(&mut effects.coercion_error)?,
        );
        (effects.log)(response, "debug")?;
        Ok(Balance { total, free, used, upnl: 0.0, info: None, currencies: None, coins: None })
    }.await;
    match result {
        Ok(balance) => Ok(balance),
        Err(error) => {
            let message = (effects.stringify_error)(&error)?;
            (effects.log)(format!("Error fetching balance: {message}"), "error")?;
            let detail = (effects.describe_error)(&error)?.unwrap_or_else(|| "undefined".into());
            (effects.log)(format!("Balance error details: {detail}"), "debug")?;
            Err(error)
        }
    }
}
