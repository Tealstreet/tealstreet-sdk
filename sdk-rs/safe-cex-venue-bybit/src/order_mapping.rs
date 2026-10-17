#![cfg_attr(
    not(test),
    expect(dead_code, reason = "Order mapping awaiting class integration")
)]
use crate::{
    exchange_state::{number, parse_float},
    types,
};
use safe_cex_num::{adjust, after_decimal, js_string, js_truthy, math_pow, math_round};
use safe_cex_types::{Order, OrderExtra, OrderSide, OrderType};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::json_field::field;
fn lookup(table: &Value, key: Option<&Value>) -> Result<Option<Value>, String> {
    let key = js_string(key).map_err(|e| e.message)?;
    let entries: Vec<_> = table
        .as_object()
        .into_iter()
        .flat_map(|map| map.iter())
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    match safe_cex_num::js_table_lookup(&entries, &key) {
        safe_cex_num::TableHit::Mapped(value) => Ok(Some((*value).clone())),
        safe_cex_num::TableHit::Missing => Ok(None),
        safe_cex_num::TableHit::Inherited(_) => Err(format!(
            "Bybit typed order cannot represent inherited table key {key}"
        )),
    }
}
fn decode<T: DeserializeOwned>(value: Option<Value>, name: &str) -> Result<T, String> {
    serde_json::from_value(value.ok_or_else(|| format!("Bybit typed Order requires {name}"))?)
        .map_err(|e| e.to_string())
}
fn optional<T: DeserializeOwned>(value: Option<Value>) -> Result<Option<T>, String> {
    value
        .map(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
        .transpose()
}
fn true_flag(value: Option<&Value>) -> bool {
    value == Some(&Value::Bool(true)) || value.and_then(Value::as_str) == Some("true")
}
fn nullable_zero(value: Option<&Value>) -> &Value {
    static ZERO: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| serde_json::json!(0));
    value.filter(|v| !v.is_null()).unwrap_or(&ZERO)
}
fn subtract_raw(a: &Value, b: &Value) -> Result<f64, String> {
    // safe-math subtract inspects decimal spelling before numeric subtraction.
    let precision = |v: &Value| -> Result<f64, String> {
        if let Some(s) = v.as_str() {
            Ok(after_decimal(s))
        } else if v.is_number() {
            Ok(after_decimal(number(Some(v))?))
        } else {
            Err("Bybit order subtraction requires number or string inputs".into())
        }
    };
    let factor = math_pow(10., precision(a)?.max(precision(b)?));
    Ok(math_round((number(Some(a))? - number(Some(b))?) * factor) / factor)
}
fn inverse_remaining(native: f64, price: f64) -> f64 {
    if price > 0. {
        math_round((native / price) * 1e8) / 1e8
    } else {
        native
    }
}

pub(crate) fn map_order(
    row: &Value,
    inverse: bool,
    market_inverse: bool,
    mut now: impl FnMut() -> f64,
) -> Result<Vec<Order>, String> {
    let stop = field(row, "stopOrderType")?;
    let mode = field(row, "tpslMode")?;
    let position_mode = matches!(mode.and_then(Value::as_str), Some("Full" | "Partial"));
    let create = field(row, "createType")?;
    let created_type = if position_mode {
        lookup(&types::POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE, create)?
    } else {
        None
    };
    let is_stop = (js_truthy(stop) && stop.and_then(Value::as_str) != Some("UNKNOWN"))
        || created_type.is_some();
    let trigger = field(row, "triggerPrice")?;
    let raw_price = if is_stop && js_truthy(trigger) {
        trigger
    } else {
        row.get("price")
    };
    let raw_type = created_type.as_ref().or(if is_stop {
        stop
    } else {
        field(row, "orderType")?
    });
    let reduce = field(row, "reduceOnly")?;
    let reduce_only = if inverse {
        true_flag(reduce)
    } else if !js_truthy(reduce) {
        false
    } else {
        reduce
            .and_then(Value::as_bool)
            .ok_or("Bybit typed Order requires boolean reduceOnly")?
    };
    let close = true_flag(field(row, "closeOnTrigger")?);
    let opening = inverse
        && is_stop
        && !reduce_only
        && !close
        && !position_mode
        && (parse_float(Some(nullable_zero(trigger)))? > 0.
            || create.and_then(Value::as_str) == Some("CreateByStopOrder"));
    let kind = if opening {
        OrderType::Conditional
    } else {
        decode(lookup(&types::ORDER_TYPE, raw_type)?, "type")?
    };
    let tif = optional(lookup(&types::ORDER_TIME_IN_FORCE, row.get("timeInForce"))?)?;
    let cum = nullable_zero(field(row, "cumExecQty")?);
    let qty = nullable_zero(row.get("qty"));
    let native = if js_truthy(row.get("leavesQty")) {
        parse_float(row.get("leavesQty"))?
    } else {
        subtract_raw(qty, cum)?
    };
    let price = parse_float(raw_price)?;
    let remaining = if inverse {
        inverse_remaining(native, price)
    } else if market_inverse {
        adjust(native / price, 0.0000001)
    } else {
        native
    };
    let status = decode(
        lookup(&types::ORDER_STATUS, field(row, "orderStatus")?)?,
        "status",
    )?;
    let side = decode(lookup(&types::ORDER_SIDE, row.get("side"))?, "side")?;
    let amount = if matches!(
        raw_type.and_then(Value::as_str),
        Some("StopLoss" | "TakeProfit")
    ) && mode.and_then(Value::as_str) == Some("Full")
    {
        0.
    } else {
        parse_float(Some(qty))?
    };
    let filled = parse_float(Some(cum))?;
    let last_update = now();
    let trigger_type = if matches!(
        kind,
        OrderType::Conditional | OrderType::TakeProfit | OrderType::StopLoss
    ) {
        optional(lookup(
            &types::ORDER_TRIGGER_TYPE,
            field(row, "triggerBy")?,
        )?)?
    } else {
        None
    };
    let extra = OrderExtra {
        remaining_native: Some(native),
        remaining_notional: inverse.then_some(native),
        filled_in_placement_units: None,
        conditional_order_price: None,
        reject_reason: None,
        is_from_chaser: None,
        chaser_id: None,
        chaser_attempts: None,
        chaser_max_attempts: None,
    };
    let base = Order {
        client_order_id: (None).into(),
        id: row
            .get("orderId")
            .and_then(Value::as_str)
            .ok_or("Bybit typed Order requires string id")?
            .into(),
        parent_id: (None).into(),
        status,
        symbol: row
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or("Bybit typed Order requires string symbol")?
            .into(),
        r#type: (kind).into(),
        side,
        price: price.into(),
        amount,
        filled,
        remaining,
        reduce_only: reduce_only.into(),
        close: (inverse.then_some(close)).into(),
        info: (Some(row.clone())).into(),
        time_in_force: (tif).into(),
        last_update,
        trigger_type: if kind == OrderType::Conditional {
            (trigger_type).into()
        } else {
            (None).into()
        },
        tp_trigger_type: if kind == OrderType::TakeProfit {
            (trigger_type).into()
        } else {
            (None).into()
        },
        sl_trigger_type: if kind == OrderType::StopLoss {
            (trigger_type).into()
        } else {
            (None).into()
        },
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (Some(extra)).into(),
    };
    let sl = parse_float(field(row, "stopLoss")?)?;
    let tp = parse_float(field(row, "takeProfit")?)?;
    let mut orders = vec![base.clone()];
    for (value, kind, suffix, key) in [
        (sl, OrderType::StopLoss, "stop_loss", "slTriggerBy"),
        (tp, OrderType::TakeProfit, "take_profit", "tpTriggerBy"),
    ] {
        if value > 0. {
            let mut order = base.clone();
            order.id = format!("{}__{suffix}", base.id);
            order.r#type = (kind).into();
            order.side = if base.side == OrderSide::Buy {
                safe_cex_types::ModelField::Value(OrderSide::Sell)
            } else {
                safe_cex_types::ModelField::Value(OrderSide::Buy)
            };
            order.price = (value).into();
            order.filled = 0.;
            order.remaining = if inverse {
                inverse_remaining(native, value)
            } else {
                base.amount
            };
            order.last_update = now();
            let trigger = optional(lookup(&types::ORDER_TRIGGER_TYPE, field(row, key)?)?)?;
            if kind == OrderType::StopLoss {
                order.sl_trigger_type = (trigger).into();
            } else {
                order.tp_trigger_type = (trigger).into();
            }
            orders.push(order);
        }
    }
    Ok(orders)
}
#[cfg(test)]
mod tests;
