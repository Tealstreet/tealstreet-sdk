use safe_cex_types::PositionMode;
use serde_json::{Map, Value};
use std::cell::Cell;

pub fn is_supported_eu_market_instrument(market: &Map<String, Value>) -> bool {
    if market.get("instType").and_then(Value::as_str) == Some("FUTURES") {
        return market.get("ruleType").and_then(Value::as_str) == Some("xperp")
            && matches!(
                market.get("ctType").and_then(Value::as_str),
                Some("linear" | "inverse")
            );
    }
    crate::exchange::is_supported_market_instrument(market)
}

pub fn after_method_called_in_worker(
    account_position_mode: &Cell<Option<PositionMode>>,
    method: &[u16],
    first_argument: Option<bool>,
) {
    if method
        .iter()
        .copied()
        .eq("changePositionMode".encode_utf16())
    {
        account_position_mode.set(Some(if first_argument == Some(true) {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        }));
    }
}
