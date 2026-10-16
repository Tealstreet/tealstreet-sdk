use super::special_numbers;
use safe_cex_venue_coinbase::starbase_private::*;
use serde_json::Value;
pub fn project_order(order: &CoinbaseStarbasePrivateOrder) -> Value {
    let mut value = serde_json::to_value(order).unwrap();
    if let CoinbaseStarbaseOrderPrice::Number(number) = order.price {
        special_numbers::project_number(&mut value, "price", number);
    }
    for (field, number) in [
        ("averagePrice", order.average_price),
        ("creationTimestampMs", Some(order.creation_timestamp_ms)),
        (
            "lastUpdateTimestampMs",
            Some(order.last_update_timestamp_ms),
        ),
        ("gatewayAmount", order.gateway_amount),
        ("gatewayFilledAmount", order.gateway_filled_amount),
        ("gatewayContracts", order.gateway_contracts),
        ("triggerPrice", order.trigger_price),
    ] {
        if let Some(number) = number {
            special_numbers::project_number(&mut value, field, number);
        }
    }
    value
}
pub fn project_trade(trade: &CoinbaseStarbasePrivateTrade) -> Value {
    let mut value = serde_json::to_value(trade).unwrap();
    for (field, number) in [
        ("sequence", Some(trade.sequence)),
        ("timestampMs", Some(trade.timestamp_ms)),
        ("priceUsd", Some(trade.price_usd)),
        ("markPriceUsd", Some(trade.mark_price_usd)),
        ("indexPriceUsd", Some(trade.index_price_usd)),
        ("gatewayAmount", Some(trade.gateway_amount)),
        ("gatewayContracts", trade.gateway_contracts),
        ("gatewayProfitLoss", trade.gateway_profit_loss),
        ("fee", Some(trade.fee)),
    ] {
        if let Some(number) = number {
            special_numbers::project_number(&mut value, field, number);
        }
    }
    value
}
