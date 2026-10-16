//! Order linkage utilities over the canonical shared Order and an orders snapshot.
use crate::order_ids::{get_virtual_tpsl_leg, get_virtual_tpsl_parent_order_id};
use safe_cex_num::{JsCoercionError, js_string, js_truthy};
use safe_cex_types::{Order, OrderType};
use serde_json::Value;

pub fn get_tpsl_raw_order_id(order: &Order) -> Result<String, JsCoercionError> {
    let info = order.info.as_ref();
    let raw = info
        .and_then(|v| v.get("id"))
        .filter(|v| !v.is_null())
        .or_else(|| info.and_then(|v| v.get("orderId")));
    if let Some(value) = raw {
        if !value.is_null() && !matches!(value,Value::String(s) if s.is_empty()) {
            return js_string(Some(value));
        }
    }
    Ok(order.id.clone())
}

pub fn get_attached_virtual_tpsl_leg(order: &Order, orders: &[Order]) -> Option<&'static str> {
    let leg = get_virtual_tpsl_leg(&order.id)?;
    let parent_id = get_virtual_tpsl_parent_order_id(&order.id);
    let parent = orders.iter().find(|candidate| candidate.id == parent_id)?;
    if parent.r#type != OrderType::TakeProfit && parent.r#type != OrderType::StopLoss {
        Some(leg)
    } else {
        None
    }
}

pub fn is_standalone_tpsl_order(order: &Order, orders: &[Order]) -> bool {
    (order.r#type == OrderType::TakeProfit || order.r#type == OrderType::StopLoss)
        && get_attached_virtual_tpsl_leg(order, orders).is_none()
}

pub fn is_position_tpsl_order(order: &Order) -> Result<bool, JsCoercionError> {
    let raw = order.info.as_ref().and_then(|info| info.get("type"));
    let kind = if js_truthy(raw) {
        js_string(raw)?
    } else {
        String::new()
    };
    Ok(kind.to_uppercase() == "POSITION_TPSL" || order.amount == 0.0)
}
