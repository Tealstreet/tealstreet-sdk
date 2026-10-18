//! Source mapper over native values; the typed historical-order adapter remains
//! separately available for callers choosing its closed categorical output.
use crate::{
    private_event_value::{EventOps, EventValue, PrivateEnum, number, string},
    request_weight::Failure,
};
use safe_cex_num::{PrototypeRealm, TableHit, js_table_lookup};
use safe_cex_types::{
    BothPositionSide, ExecutionPositionSide, OrderSide, OrderStatus, OrderType, PositionSide,
};
use std::rc::Rc;
fn text(row: &EventValue, key: &str) -> Result<String, Failure> {
    let value = row.get(key)?;
    if value.nullish() {
        Ok(String::new())
    } else {
        value.text()
    }
}
fn numeric(row: &EventValue, key: &str) -> Result<f64, Failure> {
    let value = row.get(key)?;
    if value.nullish() {
        Ok(0.0)
    } else {
        value.number()
    }
}
fn parsed(row: &EventValue, key: &str) -> Result<f64, Failure> {
    Ok(safe_cex_num::parse_float(&text(row, key)?))
}
fn optional_number(row: &EventValue, key: &str) -> Result<EventValue, Failure> {
    let n = parsed(row, key)?;
    Ok(if n.is_finite() && n != 0.0 {
        number(n)
    } else {
        EventValue::Undefined
    })
}
fn finite_number(row: &EventValue, key: &str) -> Result<EventValue, Failure> {
    let n = parsed(row, key)?;
    Ok(number(if n.is_finite() { n } else { 0.0 }))
}
fn lookup<'a, T>(
    table: &'a [(&str, T)],
    key: &str,
    realm: &Rc<PrototypeRealm>,
    mapped: impl FnOnce(&'a T) -> PrivateEnum,
) -> EventValue {
    match js_table_lookup(table, key) {
        TableHit::Mapped(value) => EventValue::enum_value(mapped(value)),
        TableHit::Inherited(_) | TableHit::Missing => {
            EventValue::from_prototype(realm.inherited(key))
        }
    }
}
// ts: mapBinanceHistoricalOrder
pub fn map_binance_historical_order_native(
    row: &EventValue,
    realm: &Rc<PrototypeRealm>,
) -> Result<Option<EventValue>, Failure> {
    let _binding = realm.bind();
    let id = text(row, "orderId")?;
    let symbol = text(row, "symbol")?;
    let created_time = numeric(row, "time")?;
    if id.is_empty() || symbol.is_empty() || created_time == 0.0 || created_time.is_nan() {
        return Ok(None);
    }
    let side = match text(row, "side")?.to_uppercase().as_str() {
        "BUY" => OrderSide::Buy,
        "SELL" => OrderSide::Sell,
        _ => return Ok(None),
    };
    const TYPES: &[(&str, OrderType)] = &[
        ("LIMIT", OrderType::Limit),
        ("MARKET", OrderType::Market),
        ("STOP", OrderType::StopLoss),
        ("STOP_MARKET", OrderType::StopLoss),
        ("TAKE_PROFIT", OrderType::TakeProfit),
        ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
        ("TRAILING_STOP_MARKET", OrderType::TrailingStopLoss),
    ];
    let mut order_type = lookup(TYPES, &text(row, "origType")?, realm, |v| {
        PrivateEnum::Type(*v)
    });
    if order_type.nullish() {
        order_type = lookup(TYPES, &text(row, "type")?, realm, |v| PrivateEnum::Type(*v));
    }
    if order_type.nullish() {
        order_type = EventValue::enum_value(PrivateEnum::Type(OrderType::Limit));
    }
    const STATUSES: &[(&str, OrderStatus)] = &[
        ("NEW", OrderStatus::Open),
        ("PARTIALLY_FILLED", OrderStatus::Open),
        ("FILLED", OrderStatus::Closed),
        ("CANCELED", OrderStatus::Canceled),
        ("EXPIRED", OrderStatus::Canceled),
        ("REJECTED", OrderStatus::Canceled),
    ];
    let mut status = lookup(STATUSES, &text(row, "status")?, realm, |v| {
        PrivateEnum::Status(*v)
    });
    if status.nullish() {
        status = EventValue::enum_value(PrivateEnum::Status(OrderStatus::Closed));
    }
    let price = optional_number(row, "price")?;
    let average_price = optional_number(row, "avgPrice")?;
    let amount = finite_number(row, "origQty")?;
    let filled = finite_number(row, "executedQty")?;
    let reduce_only = EventValue::Bool(row.read("reduceOnly")?.truthy());
    static SIDES: &[(&str, ExecutionPositionSide)] = &[
        ("BOTH", ExecutionPositionSide::Both(BothPositionSide::Both)),
        ("LONG", ExecutionPositionSide::Side(PositionSide::Long)),
        ("SHORT", ExecutionPositionSide::Side(PositionSide::Short)),
    ];
    let position_side = lookup(
        SIDES,
        &text(row, "positionSide")?,
        realm,
        PrivateEnum::Execution,
    );
    let updated = numeric(row, "updateTime")?;
    Ok(Some(EventValue::object_in_realm(
        realm.clone(),
        [
            ("id".into(), string(id)),
            ("symbol".into(), string(symbol)),
            (
                "side".into(),
                EventValue::enum_value(PrivateEnum::Side(side)),
            ),
            ("type".into(), order_type),
            ("status".into(), status),
            ("price".into(), price),
            ("averagePrice".into(), average_price),
            ("amount".into(), amount),
            ("filled".into(), filled),
            ("reduceOnly".into(), reduce_only),
            ("positionSide".into(), position_side),
            ("createdTime".into(), number(created_time)),
            (
                "updatedTime".into(),
                if updated != 0.0 && !updated.is_nan() {
                    number(updated)
                } else {
                    EventValue::Undefined
                },
            ),
        ]
        .into_iter()
        .collect(),
    )))
}
