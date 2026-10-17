//! Futures and spot wire-order projection into the shared typed order model.
use crate::{spot_types, types};
use safe_cex_num::{
    TableHit, js_string, js_table_lookup, js_truthy, multiply, parse_float, parse_int, subtract,
};
use safe_cex_store::Store;
use safe_cex_types::{Order, OrderExtra, OrderSide, OrderStatus, OrderTimeInForce, OrderType};
use serde_json::{Map, Value};

fn first<'a>(row: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .filter_map(|key| row.get(*key))
        .find(|v| js_truthy(Some(v)))
}
fn number(value: Option<&Value>) -> Result<f64, String> {
    Ok(parse_float(&js_string(value).map_err(|e| e.message)?))
}
fn number_or_zero(row: &Map<String, Value>, keys: &[&str]) -> Result<f64, String> {
    match first(row, keys) {
        Some(v) => number(Some(v)),
        None => Ok(0.0),
    }
}
fn string(value: Option<&Value>, field: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("KuCoin {field} must be a string"))
}
// These tables contain only nonempty enum strings, so every mapped value is truthy.
fn lookup<T: Copy>(
    table: &[(&str, T)],
    value: Option<&Value>,
    fallback: impl FnOnce() -> T,
) -> Result<TableHit<T>, String> {
    let key = js_string(value).map_err(|e| e.message)?;
    let hit = js_table_lookup(table, &key);
    if !hit.is_truthy_by(|_| true) {
        return Ok(TableHit::Mapped(fallback()));
    }
    Ok(match hit {
        TableHit::Mapped(value) => TableHit::Mapped(*value),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Missing,
    })
}
fn typed<T>(hit: TableHit<T>, field: &str) -> Result<T, String> {
    match hit {
        TableHit::Mapped(value) => Ok(value),
        TableHit::Inherited(member) => Err(format!(
            "KuCoin typed Order.{field} refuses inherited {member:?}; whole record refused"
        )),
        TableHit::Missing => Err(format!("KuCoin typed Order.{field} is missing")),
    }
}
fn base_order(id: String, symbol: String, row: &Map<String, Value>) -> Order {
    Order {
        client_order_id: (None).into(),
        id,
        parent_id: (None).into(),
        status: safe_cex_types::ModelField::Value(OrderStatus::Open),
        symbol,
        r#type: safe_cex_types::ModelField::Value(OrderType::Limit),
        side: safe_cex_types::ModelField::Value(OrderSide::Buy),
        price: (0.0).into(),
        amount: 0.0,
        filled: 0.0,
        remaining: 0.0,
        reduce_only: false.into(),
        close: (None).into(),
        info: (Some(Value::Object(row.clone()))).into(),
        time_in_force: (None).into(),
        last_update: 0.0,
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    }
}
// ts: KucoinExchange.mapOrder
// ts-bug: KucoinExchange.mapOrder inherited table values survive branching but are refused at the closed typed output boundary.
pub fn map_order(
    row: &Map<String, Value>,
    store: &dyn Store,
    now_ms: f64,
) -> Result<Vec<Order>, String> {
    let id = string(first(row, &["id", "orderId"]), "order id")?;
    let raw_symbol = string(row.get("symbol"), "symbol")?;
    let symbol = crate::sizing::to_internal_symbol(Some(&raw_symbol));
    let symbol = if symbol.is_empty() {
        raw_symbol
    } else {
        symbol
    };
    let contract_size = crate::sizing::contract_size_for(Some(&symbol), store);
    let raw_type = first(row, &["orderType", "type"]);
    let size = number_or_zero(row, &["size"])?;
    let filled = number_or_zero(row, &["filledSize"])?;
    let remaining = match row.get("remainSize").filter(|v| !v.is_null()) {
        Some(v) => number(Some(v))?,
        None => subtract(size, filled),
    };
    let mut order = base_order(id, symbol, row);
    order.last_update = match first(row, &["updateTime", "ts", "orderTime", "createdAt"]) {
        Some(v) => {
            let text = js_string(Some(v)).map_err(|e| e.message)?;
            parse_int(
                &String::from_utf16_lossy(&text.encode_utf16().take(13).collect::<Vec<_>>()),
                Some(10),
            )
        }
        None => now_ms,
    };
    let status = lookup(types::ORDER_STATUS, row.get("status"), || OrderStatus::Open)?;
    let mut order_type = lookup(types::ORDER_TYPE, raw_type, || {
        if raw_type.and_then(Value::as_str) == Some("market") {
            OrderType::Market
        } else {
            OrderType::Limit
        }
    })?;
    order.side = if row.get("side").and_then(Value::as_str) == Some("buy") {
        safe_cex_types::ModelField::Value(OrderSide::Buy)
    } else {
        safe_cex_types::ModelField::Value(OrderSide::Sell)
    };
    order.price = (number_or_zero(row, &["price", "orderPrice"])?).into();
    order.amount = multiply(size, contract_size);
    order.filled = multiply(filled, contract_size);
    order.remaining = multiply(remaining, contract_size);
    order.reduce_only = (row.get("reduceOnly") == Some(&Value::Bool(true))).into();
    let time_in_force = lookup(types::TIME_IN_FORCE, row.get("timeInForce"), || {
        OrderTimeInForce::GoodTillCancel
    })?;
    order.extra = (Some(OrderExtra {
        conditional_order_price: None,
        remaining_native: Some(remaining),
        filled_in_placement_units: Some(filled),
        remaining_notional: None,
        reject_reason: None,
        is_from_chaser: None,
        chaser_id: None,
        chaser_attempts: None,
        chaser_max_attempts: None,
    }))
    .into();
    let stop = row.get("stop");
    if stop == Some(&Value::Bool(true))
        || stop.and_then(Value::as_str).is_some_and(|s| !s.is_empty())
        || raw_type.and_then(Value::as_str) == Some("stop")
    {
        let stop_type = first(row, &["stopType", "stop"]);
        let stop_type = match stop_type {
            Some(v) => js_string(Some(v)).map_err(|e| e.message)?,
            None => String::new(),
        }
        .to_lowercase();
        match stop_type.as_str() {
            "down" | "loss" | "sl" => order_type = TableHit::Mapped(OrderType::StopLoss),
            "up" | "profit" | "tp" => order_type = TableHit::Mapped(OrderType::TakeProfit),
            _ => {}
        }
        order.price = (number_or_zero(row, &["stopPrice"])?).into();
    }
    if stop == Some(&Value::Bool(true))
        && row.get("stopPriceType").and_then(Value::as_str) == Some("TP")
    {
        order_type = TableHit::Mapped(OrderType::TrailingStopLoss);
    }
    let buy = order.side == OrderSide::Buy;
    let loss = first(
        row,
        &[
            "stopLoss",
            if buy {
                "triggerStopDownPrice"
            } else {
                "triggerStopUpPrice"
            },
        ],
    );
    let profit = first(
        row,
        &[
            "takeProfit",
            if buy {
                "triggerStopUpPrice"
            } else {
                "triggerStopDownPrice"
            },
        ],
    );
    let mut orders = vec![order.clone()];
    for (value, kind, suffix) in [
        (loss, OrderType::StopLoss, "__stop_loss"),
        (profit, OrderType::TakeProfit, "__take_profit"),
    ] {
        if let Some(value) = value {
            let mut attached = order.clone();
            attached.id.push_str(suffix);
            attached.side = if buy {
                safe_cex_types::ModelField::Value(OrderSide::Sell)
            } else {
                safe_cex_types::ModelField::Value(OrderSide::Buy)
            };
            attached.r#type = (kind).into();
            attached.price = (number(Some(value))?).into();
            attached.reduce_only = true.into();
            orders.push(attached);
        }
    }
    // Finish all source branches/coercions first: attached orders can override
    // type, and their numeric coercion may fail before a typed output exists.
    // The Result<Vec<Order>> boundary refuses the entire batch if its parent
    // or any shared field still contains an inherited value.
    let status = typed(status, "status")?;
    let parent_type = typed(order_type, "type")?;
    let time_in_force = typed(time_in_force, "timeInForce")?;
    for mapped in &mut orders {
        mapped.status = (status).into();
        mapped.time_in_force = (Some(time_in_force)).into();
    }
    orders[0].r#type = (parent_type).into();
    Ok(orders)
}
// ts: KucoinSpotExchange.mapOrder
// ts-bug: KucoinSpotExchange.mapOrder inherited table values survive branching but are refused at the closed typed output boundary.
pub fn map_spot_order(row: &Map<String, Value>, now_ms: f64) -> Result<Order, String> {
    let size = number_or_zero(row, &["size", "originSize"])?;
    let filled = number_or_zero(row, &["dealSize", "filledSize"])?;
    let remaining = match row.get("remainSize").filter(|v| !v.is_null()) {
        Some(v) => number(Some(v))?,
        None => {
            let raw = size - filled;
            let max = if raw.is_nan() {
                f64::NAN
            } else if raw > 0.0 {
                raw
            } else {
                0.0
            };
            parse_float(&safe_cex_num::js_number_string(max))
        }
    };
    let time_in_force = if row.get("postOnly") == Some(&Value::Bool(true)) {
        TableHit::Mapped(OrderTimeInForce::PostOnly)
    } else {
        lookup(
            spot_types::KUCOIN_SPOT_TIME_IN_FORCE,
            row.get("timeInForce"),
            || OrderTimeInForce::GoodTillCancel,
        )?
    };
    let status = lookup(
        spot_types::KUCOIN_SPOT_ORDER_STATUS,
        row.get("status"),
        || {
            if js_truthy(row.get("active")) {
                OrderStatus::Open
            } else {
                OrderStatus::Closed
            }
        },
    )?;
    let order_type = lookup(
        spot_types::KUCOIN_SPOT_ORDER_TYPE,
        first(row, &["type", "orderType"]),
        || OrderType::Limit,
    )?;
    let side = if row.get("side").and_then(Value::as_str) == Some("sell") {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    };
    let price = number_or_zero(row, &["price", "matchPrice"])?;
    let timestamp = match first(row, &["lastUpdatedAt", "orderTime", "createdAt"]) {
        Some(v) => js_string(Some(v)).map_err(|e| e.message)?,
        None => safe_cex_num::js_number_string(now_ms),
    };
    let last_update = parse_int(&timestamp, Some(10));
    // Refuse typed identities only after every source expression has run.
    let mut order = base_order(
        string(first(row, &["id", "orderId"]), "order id")?,
        string(row.get("symbol"), "symbol")?,
        row,
    );
    order.side = (side).into();
    order.price = (price).into();
    order.amount = size;
    order.filled = filled;
    order.remaining = remaining;
    order.last_update = last_update;
    order.status = (typed(status, "status")?).into();
    order.r#type = (typed(order_type, "type")?).into();
    order.time_in_force = (Some(typed(time_in_force, "timeInForce")?)).into();
    Ok(order)
}
