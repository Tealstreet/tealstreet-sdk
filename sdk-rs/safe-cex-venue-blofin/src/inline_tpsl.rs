//! Inline TP/SL fallback projection; actual algo records remain authoritative.
use safe_cex_num::{js_trim, js_truthy, parse_float};
use safe_cex_types::{
    Order, OrderExtra, OrderSide, OrderStatus, OrderType,
    js_coercion::{JsCoercionError, js_string},
};
use serde_json::Value;
use std::collections::HashSet;

fn protective_type(kind: OrderType) -> Option<&'static str> {
    match kind {
        OrderType::TakeProfit => Some("take_profit_market"),
        OrderType::StopLoss => Some("stop_market"),
        _ => None,
    }
}
/// Typed orders enter through shared numeric ingress; clock is infallible/non-mutating.
pub fn add_inline_tpsl_fallback_orders(
    orders: &[Order],
    mut now: impl FnMut() -> f64,
) -> Result<Vec<Order>, JsCoercionError> {
    let mut protective = HashSet::new();
    for order in orders {
        let Some(kind) = order.r#type.into_value().and_then(protective_type) else {
            continue;
        };
        let raw_parent = order.info.as_ref().and_then(|info| info.get("orderId"));
        let parent = if js_truthy(raw_parent) {
            js_string(raw_parent)?
        } else {
            String::new()
        };
        let parent = js_trim(&parent);
        if !parent.is_empty() {
            protective.insert(format!("{parent}:{kind}"));
        }
        if let Some((raw_parent, _)) = order.id.split_once("__")
            && !raw_parent.is_empty()
        {
            protective.insert(format!("{raw_parent}:{kind}"));
        }
    }
    let mut fallback = Vec::new();
    for order in orders {
        if order.r#type != OrderType::Limit || order.id.is_empty() {
            continue;
        }
        let info = order.info.as_ref();
        if js_truthy(info.and_then(|v| v.get("tpslId")))
            || info
                .and_then(|v| v.get("orderCategory"))
                .and_then(Value::as_str)
                != Some("normal")
        {
            continue;
        }
        let remaining = if order.remaining.is_finite() && order.remaining > 0.0 {
            order.remaining
        } else {
            order.amount
        };
        if !remaining.is_finite() || remaining <= 0.0 {
            continue;
        }
        let native = order
            .extra
            .as_ref()
            .and_then(|e| e.remaining_native)
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(remaining);
        let side = if order.side == OrderSide::Buy {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };
        for (field, kind, suffix) in [
            ("tpTriggerPrice", OrderType::TakeProfit, "__take_profit"),
            ("slTriggerPrice", OrderType::StopLoss, "__stop_loss"),
        ] {
            let raw = info.and_then(|v| v.get(field)).filter(|v| !v.is_null());
            let price = match raw {
                Some(v) => parse_float(&js_string(Some(v))?),
                None => parse_float(""),
            };
            if price.is_finite() && price > 0.0 {
                let key = format!(
                    "{}:{}",
                    order.id,
                    protective_type(kind).expect("protective type")
                );
                if !protective.contains(&key) {
                    fallback.push(Order {
                        client_order_id: (None).into(),
                        id: format!("{}{suffix}", order.id),
                        status: safe_cex_types::ModelField::Value(OrderStatus::Open),
                        symbol: order.symbol.clone(),
                        r#type: (kind).into(),
                        side: side.into(),
                        price: price.into(),
                        amount: remaining,
                        filled: 0.0,
                        remaining,
                        extra: (Some(OrderExtra {
                            remaining_native: Some(native),
                            filled_in_placement_units: None,
                            remaining_notional: None,
                            conditional_order_price: None,
                            reject_reason: None,
                            is_from_chaser: None,
                            chaser_id: None,
                            chaser_attempts: None,
                            chaser_max_attempts: None,
                        }))
                        .into(),
                        reduce_only: true.into(),
                        last_update: now(),
                        info: (info.cloned()).into(),
                        parent_id: (None).into(),
                        close: (None).into(),
                        time_in_force: (None).into(),
                        trigger_type: (None).into(),
                        tp_trigger_type: (None).into(),
                        sl_trigger_type: (None).into(),
                        take_profit: (None).into(),
                        stop_loss: (None).into(),
                    });
                    protective.insert(key);
                }
            }
        }
    }
    let mut result = orders.to_vec();
    result.extend(fallback);
    Ok(result)
}
