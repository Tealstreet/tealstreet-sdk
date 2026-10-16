//! Small variant methods with explicit store and host dependencies. Constructors
//! and lifecycle composition remain separate parity rows.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::Market;
use serde_json::Value;

// Identical strict-number getter in inverse and spot exchange declarations.
pub fn account_type(unified_margin_status: Option<&Value>) -> &'static str {
    if unified_margin_status.and_then(Value::as_f64) == Some(1.) {
        "CONTRACT"
    } else {
        "UNIFIED"
    }
}

pub fn inverse_supported_categories() -> Vec<&'static str> {
    vec!["inverse"]
}

pub fn inverse_get_settle_coins_for_category(
    category: &str,
    unified_margin_status: Option<&Value>,
    emit_and_log_error: impl FnOnce(&str) -> Result<(), ExchangeError>,
) -> Result<Vec<Option<&'static str>>, ExchangeError> {
    if category == "linear" {
        return Ok(if account_type(unified_margin_status) == "UNIFIED" {
            vec![Some("USDT"), Some("USDC")]
        } else {
            vec![Some("USDT")]
        });
    } else if category == "inverse" {
        return Ok(vec![None]);
    }
    let message = format!("Unknown category {category}");
    emit_and_log_error(&message)?;
    Err(ExchangeError::new(message))
}

pub fn inverse_get_category_for_symbol(
    symbol: &str,
    markets: &[Market],
    warn: impl FnOnce(&str),
) -> &'static str {
    let market = markets.iter().find(|market| market.symbol == symbol);
    if market.is_none() {
        if !markets.is_empty() {
            warn(&format!("Market {symbol} not found"));
        }
        return "inverse";
    }
    "inverse"
}

pub fn inverse_get_ticker_symbols_for_public_stream(
    markets: &[Market],
    get_ticker_symbols_for_subscription: impl FnOnce(&[String]) -> Vec<String>,
) -> Vec<String> {
    let inverse_symbols = markets
        .iter()
        .filter(|market| market.quote != "USDT" && market.quote != "USDC")
        .map(|market| market.symbol.clone())
        .collect::<Vec<_>>();
    get_ticker_symbols_for_subscription(&inverse_symbols)
}

pub fn spot_get_ticker_symbols_for_public_stream(
    markets: &[Market],
    get_ticker_symbols_for_subscription: impl FnOnce(&[String]) -> Vec<String>,
) -> Vec<String> {
    let symbols = markets
        .iter()
        .map(|market| market.symbol.clone())
        .collect::<Vec<_>>();
    get_ticker_symbols_for_subscription(&symbols)
}

pub fn hyro_is_finite_numeric(value: Option<&Value>) -> bool {
    if !matches!(value, Some(Value::Number(_) | Value::String(_))) {
        return false;
    }
    // The type guard makes both conversions infallible on inert JSON values.
    let text = safe_cex_num::js_string(value).expect("guarded number or string");
    !safe_cex_num::js_trim(&text).is_empty()
        && safe_cex_num::js_number(value)
            .expect("guarded number or string")
            .is_finite()
}

pub fn hyro_supports_trade_derived_candles() -> bool {
    false
}

pub fn hyro_supported_categories() -> Vec<&'static str> {
    vec!["linear"]
}

pub fn hyro_get_settle_coins_for_category(
    category: &str,
    emit_and_log_error: impl FnOnce(&str) -> Result<(), ExchangeError>,
) -> Result<Vec<&'static str>, ExchangeError> {
    if category == "linear" {
        return Ok(vec!["USDT"]);
    }
    let message = format!("Unknown category {category}");
    emit_and_log_error(&message)?;
    Err(ExchangeError::new(message))
}

pub fn hyro_requires_order_link_id() -> bool {
    true
}
