//! Stateless switch bodies from CryptocomExchange; construction and callers remain separate.
use crate::wire::{CryptocomOrderType, CryptocomRefPriceType, CryptocomTimeInForce};
use safe_cex_types::{OrderStatus, OrderTimeInForce, OrderType};

// Switch targets are primitive strings. Every non-string input shares the default
// represented by None; UTF-16 units preserve all strings without coercion or loss.
fn equals(value: Option<&[u16]>, literal: &str) -> bool {
    value.is_some_and(|value| value.iter().copied().eq(literal.encode_utf16()))
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

pub fn map_order_type(kind: Option<&[u16]>) -> CryptocomOrderType {
    if equals(kind, "market") {
        return CryptocomOrderType::Market;
    }
    if equals(kind, "limit") {
        return CryptocomOrderType::Limit;
    }
    if equals(kind, "stop_market") {
        return CryptocomOrderType::StopLoss;
    }
    if equals(kind, "take_profit_market") {
        return CryptocomOrderType::TakeProfit;
    }
    CryptocomOrderType::Limit
}

pub fn map_cryptocom_order_type(kind: Option<&[u16]>) -> OrderType {
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

pub fn map_time_in_force(tif: Option<&[u16]>) -> CryptocomTimeInForce {
    if equals(tif, "ImmediateOrCancel") {
        return CryptocomTimeInForce::ImmediateOrCancel;
    }
    if equals(tif, "FillOrKill") {
        return CryptocomTimeInForce::FillOrKill;
    }
    if equals(tif, "PostOnly") {
        return CryptocomTimeInForce::PostOnly;
    }
    CryptocomTimeInForce::GoodTillCancel
}

pub fn map_stop_trigger_type(kind: Option<&[u16]>) -> CryptocomRefPriceType {
    if equals(kind, "index_price") {
        return CryptocomRefPriceType::Index;
    }
    if equals(kind, "last_price") {
        return CryptocomRefPriceType::Last;
    }
    CryptocomRefPriceType::Mark
}

pub fn map_cryptocom_time_in_force(tif: Option<&[u16]>) -> OrderTimeInForce {
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
