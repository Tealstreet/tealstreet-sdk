//! Legacy order-type discriminator; live mapping uses binding::hosts::order_mapping.
use serde_json::Value;

// ts: WOOXExchange#isAlgoOrder
pub fn is_algo_order(order_type: Option<&Value>) -> bool {
    matches!(
        order_type.and_then(Value::as_str),
        Some("stop_market" | "take_profit_market" | "trailing_stop_market")
    )
}
