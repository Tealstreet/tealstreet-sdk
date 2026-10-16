//! Stateless switches from CryptocomPrivateWebSocket; consumers remain separate.
use safe_cex_types::{OrderStatus, OrderTimeInForce, OrderType};

// String switches compare UTF-16 units without coercion. All non-strings share
// the default branch and are represented by None.
fn equals(value: Option<&[u16]>, literal: &str) -> bool {
    value.is_some_and(|value| value.iter().copied().eq(literal.encode_utf16()))
}

pub fn map_order_type(kind: Option<&[u16]>) -> OrderType {
    if equals(kind, "MARKET") {
        return OrderType::Market;
    }
    if equals(kind, "LIMIT") {
        return OrderType::Limit;
    }
    if equals(kind, "STOP_LOSS") || equals(kind, "STOP_LIMIT") {
        return OrderType::StopLoss;
    }
    if equals(kind, "TAKE_PROFIT") || equals(kind, "TAKE_PROFIT_LIMIT") {
        return OrderType::TakeProfit;
    }
    OrderType::Limit
}

pub fn map_order_status(status: Option<&[u16]>) -> OrderStatus {
    if equals(status, "ACTIVE") || equals(status, "PENDING") {
        return OrderStatus::Open;
    }
    if equals(status, "FILLED") {
        return OrderStatus::Closed;
    }
    if equals(status, "CANCELED") || equals(status, "EXPIRED") || equals(status, "REJECTED") {
        return OrderStatus::Canceled;
    }
    OrderStatus::Open
}

pub fn map_time_in_force(tif: Option<&[u16]>) -> OrderTimeInForce {
    if equals(tif, "IMMEDIATE_OR_CANCEL") {
        return OrderTimeInForce::ImmediateOrCancel;
    }
    if equals(tif, "FILL_OR_KILL") {
        return OrderTimeInForce::FillOrKill;
    }
    if equals(tif, "POST_ONLY") {
        return OrderTimeInForce::PostOnly;
    }
    OrderTimeInForce::GoodTillCancel
}
