//! Order mappings from BackpackPrivateWebSocket.
use safe_cex_types::{OrderSide, OrderStatus, OrderTimeInForce, OrderType};

// ts: BackpackPrivateWebSocket#mapOrderType
pub fn map_order_type(backpack_type: &[u16]) -> OrderType {
    // Unpaired surrogates cannot become one of the recognized ASCII keys.
    match String::from_utf16_lossy(backpack_type)
        .to_lowercase()
        .as_str()
    {
        "market" => OrderType::Market,
        "limit" => OrderType::Limit,
        _ => OrderType::Limit,
    }
}

// ts: BackpackPrivateWebSocket#determineTriggerOrderType
pub fn determine_trigger_order_type<S: PartialEq>(
    tickers: &[(S, Option<f64>, Option<f64>)],
    symbol: &S,
    side: OrderSide,
    trigger_price: Option<f64>,
) -> OrderType {
    let current_price = tickers
        .iter()
        .find(|ticker| &ticker.0 == symbol)
        .and_then(|ticker| ticker.1.or(ticker.2));
    let Some(current_price) = current_price.filter(|price| *price != 0.0 && !price.is_nan()) else {
        return OrderType::StopLoss;
    };
    let Some(trigger_price) = trigger_price else {
        return OrderType::StopLoss;
    };
    if side == OrderSide::Sell {
        if trigger_price < current_price {
            OrderType::StopLoss
        } else {
            OrderType::TakeProfit
        }
    } else if trigger_price > current_price {
        OrderType::StopLoss
    } else {
        OrderType::TakeProfit
    }
}

// ts: BackpackPrivateWebSocket#mapOrderStatus
pub fn map_order_status(backpack_status: &[u16]) -> OrderStatus {
    match String::from_utf16_lossy(backpack_status).as_str() {
        "New" | "PartiallyFilled" => OrderStatus::Open,
        "Filled" => OrderStatus::Closed,
        "Cancelled" | "Expired" => OrderStatus::Canceled,
        _ => OrderStatus::Open,
    }
}

// ts: BackpackPrivateWebSocket#mapTimeInForce
pub fn map_time_in_force(backpack_tif: &[u16]) -> OrderTimeInForce {
    match String::from_utf16_lossy(backpack_tif).as_str() {
        "GTC" => OrderTimeInForce::GoodTillCancel,
        "IOC" => OrderTimeInForce::ImmediateOrCancel,
        "FOK" => OrderTimeInForce::FillOrKill,
        "PO" => OrderTimeInForce::PostOnly,
        _ => OrderTimeInForce::GoodTillCancel,
    }
}
