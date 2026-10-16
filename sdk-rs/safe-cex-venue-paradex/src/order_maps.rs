//! Order mapper lookups retain stock inherited-member identities.
use crate::constants::ORDER_TYPE;
use safe_cex_num::{TableHit, js_table_lookup};
use safe_cex_types::{OrderStatus, OrderType};

// ts: ParadexExchange#mapTimeInForce
pub fn map_time_in_force(time_in_force: Option<&str>) -> TableHit<&'static str> {
    let Some(value) = time_in_force.filter(|value| !value.is_empty()) else {
        return TableHit::Mapped("GTC");
    };
    let mapping = [
        ("GoodTillCancel", "GTC"),
        ("GTC", "GTC"),
        ("ImmediateOrCancel", "IOC"),
        ("IOC", "IOC"),
        ("PostOnly", "POST_ONLY"),
        ("POST_ONLY", "POST_ONLY"),
        ("GTT", "GTC"),
        ("FOK", "IOC"),
    ];
    match js_table_lookup(&mapping, value) {
        TableHit::Mapped(value) => TableHit::Mapped(*value),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Mapped("GTC"),
    }
}

// ts: ParadexExchange#mapOrderType
pub fn map_order_type(kind: OrderType) -> &'static str {
    match kind {
        OrderType::Market => "MARKET",
        OrderType::Limit => "LIMIT",
        OrderType::Conditional => "STOP_LIMIT",
        OrderType::StopLoss => "STOP_MARKET",
        OrderType::TakeProfit => "TAKE_PROFIT_MARKET",
        OrderType::TrailingStopLoss => "STOP_MARKET",
    }
}

// ts: ParadexExchange#reverseMapOrderType
pub fn reverse_map_order_type(kind: &str, reduce_only: Option<bool>) -> TableHit<OrderType> {
    if !reduce_only.unwrap_or(true) && matches!(kind, "STOP_LIMIT" | "STOP_MARKET") {
        return TableHit::Mapped(OrderType::Conditional);
    }
    match js_table_lookup(ORDER_TYPE, kind) {
        TableHit::Mapped(value) => TableHit::Mapped(*value),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Mapped(OrderType::Limit),
    }
}

// ts: ParadexExchange#mapOrderStatus
pub fn map_order_status(status: &str) -> TableHit<OrderStatus> {
    let mapping = [
        ("NEW", OrderStatus::Open),
        ("OPEN", OrderStatus::Open),
        ("PENDING", OrderStatus::Open),
        ("UNTRIGGERED", OrderStatus::Open),
        ("FILLED", OrderStatus::Closed),
        ("CANCELED", OrderStatus::Canceled),
        ("REJECTED", OrderStatus::Canceled),
    ];
    match js_table_lookup(&mapping, status) {
        TableHit::Mapped(value) => TableHit::Mapped(*value),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Mapped(OrderStatus::Open),
    }
}
