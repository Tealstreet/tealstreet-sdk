//! Source-level Bybit predicates and balance numeric helpers, before venue composition.
use serde_json::{Map, Value};
pub fn string_or_undefined(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|s| !s.is_empty())
}
pub fn number_or_undefined(value: Option<&Value>) -> Option<f64> {
    let number = match value {
        Some(Value::Number(n)) => n.as_f64()?,
        Some(Value::String(_)) => safe_cex_num::js_number(value).ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}
pub fn is_bybit_position_tpsl_mode(mode: Option<&Value>) -> bool {
    matches!(mode.and_then(Value::as_str), Some("Full" | "Partial"))
}
fn normalized_message(message: Option<&Value>) -> Result<String, String> {
    if !safe_cex_num::js_truthy(message) {
        return Ok(String::new());
    }
    safe_cex_num::js_string(message)
        .map(|s| s.to_lowercase())
        .map_err(|e| e.message.into())
}
pub fn is_bybit_order_already_gone_cancel_message(message: Option<&Value>) -> Result<bool, String> {
    let text = normalized_message(message)?;
    Ok(text.contains("order not exists") || text.contains("too late to cancel"))
}
pub fn is_bybit_order_not_modified_message(message: Option<&Value>) -> Result<bool, String> {
    Ok(normalized_message(message)?.contains("not modified"))
}
pub fn parse_bybit_finite_number(value: Option<&Value>) -> Result<Option<f64>, String> {
    let number = if let Some(Value::Number(number)) = value {
        number.as_f64().unwrap_or(f64::NAN)
    } else {
        let text = value
            .filter(|v| !v.is_null())
            .map(|v| safe_cex_num::js_string(Some(v)))
            .transpose()
            .map_err(|e| e.message.to_string())?
            .unwrap_or_default();
        safe_cex_num::parse_float(&text)
    };
    Ok(number.is_finite().then_some(number))
}
pub fn sum_bybit_coin_fields(
    coins: &[Map<String, Value>],
    fields: &[&str],
) -> Result<Option<f64>, String> {
    let mut saw_finite_value = false;
    let mut sum = 0.;
    for coin in coins {
        let mut coin_sum = 0.;
        for field in fields {
            if let Some(value) = parse_bybit_finite_number(coin.get(*field))? {
                saw_finite_value = true;
                coin_sum += value;
            }
        }
        sum += coin_sum;
    }
    Ok(saw_finite_value.then_some(sum))
}
pub fn derive_bybit_available_balance_from_coins(
    coins: &[Map<String, Value>],
) -> Result<Option<f64>, String> {
    let mut saw_finite_value = false;
    let mut total = 0.;
    for coin in coins {
        let wallet = match parse_bybit_finite_number(coin.get("usdValue"))? {
            Some(value) => Some(value),
            None => match parse_bybit_finite_number(coin.get("walletBalance"))? {
                Some(value) => Some(value),
                None => parse_bybit_finite_number(coin.get("equity"))?,
            },
        };
        let Some(wallet) = wallet else {
            continue;
        };
        saw_finite_value = true;
        let used = parse_bybit_finite_number(coin.get("totalPositionIM"))?.unwrap_or(0.)
            + parse_bybit_finite_number(coin.get("totalOrderIM"))?.unwrap_or(0.)
            + parse_bybit_finite_number(coin.get("locked"))?.unwrap_or(0.)
            + parse_bybit_finite_number(coin.get("bonus"))?.unwrap_or(0.);
        total += safe_cex_num::math_max(wallet - used, 0.);
    }
    Ok(saw_finite_value.then_some(total))
}
pub fn first_finite_number(values: &[Option<f64>]) -> f64 {
    values
        .iter()
        .flatten()
        .copied()
        .find(|v| v.is_finite())
        .unwrap_or(0.)
}
