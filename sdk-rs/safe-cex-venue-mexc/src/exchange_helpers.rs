//! Stateless helpers used by MEXC exchange orchestration.
use crate::{types::ENDPOINTS, utils::to_number};
use safe_cex_num::{JsCoercionError, js_object_or_array_entry, js_string, math_max};
use safe_cex_types::{
    Order, OrderSide, OrderStopTriggerType, OrderType, PlaceOrderOpts, Position, PositionSide,
};
use serde_json::Value;

fn submit_error_fields(error: Option<&Value>) -> (Option<&Value>, Option<&Value>) {
    let data = error
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("data"));
    fn nonnull(value: Option<&Value>) -> Option<&Value> {
        value.filter(|v| !v.is_null())
    }
    let code =
        nonnull(data.and_then(|v| v.get("code"))).or_else(|| error.and_then(|v| v.get("code")));
    let message = nonnull(data.and_then(|v| v.get("message")))
        .or_else(|| nonnull(data.and_then(|v| v.get("msg"))))
        .or_else(|| nonnull(error.and_then(|v| v.get("message"))));
    (code, message)
}

// ts: MexcExchange.isNativeBatchAccessDeniedError
pub fn is_native_batch_access_denied_error(error: Option<&Value>) -> Result<bool, JsCoercionError> {
    let (code, message) = submit_error_fields(error);
    let code = to_number(code, Some(f64::NAN))?;
    let message = message
        .map(|v| js_string(Some(v)))
        .transpose()?
        .unwrap_or_default()
        .to_lowercase();
    Ok(code == 511.0
        || message.contains("do not have access")
        || message.contains("access to the interface")
        || message.contains("market maker"))
}

// ts: MexcExchange.noteRegularOrderSubmitError. State is supplied by the caller,
// matching the source field; the paced submission queue remains a pending row.
pub fn note_regular_order_submit_error(
    cooldown_until: &mut f64,
    error: Option<&Value>,
    now: &dyn Fn() -> f64,
) -> Result<(), JsCoercionError> {
    let (code, message) = submit_error_fields(error);
    let code = to_number(code, Some(f64::NAN))?;
    let message = message
        .map(|v| js_string(Some(v)))
        .transpose()?
        .unwrap_or_default();
    if code == 510.0 || message.to_lowercase().contains("requests are too frequent") {
        *cooldown_until = math_max(*cooldown_until, now() + 1_500.0);
    }
    Ok(())
}

// ts: extractData
pub fn extract_data(payload: Option<&Value>) -> Option<&Value> {
    payload
        .and_then(|v| v.get("data"))
        .filter(|v| !v.is_null())
        .or(payload)
}
// ts: extractRows
pub fn extract_rows<'a>(payload: Option<&'a Value>, keys: &[String]) -> &'a [Value] {
    use safe_cex_num::get_key::{GetKeyUnknownValue, array_lookup};
    let data = extract_data(payload);
    if let Some(rows) = data.and_then(Value::as_array) {
        return rows;
    }
    let rows_at = |owner: &'a Value, key: &str| -> Option<&'a [Value]> {
        if let Value::Array(array) = owner {
            return match array_lookup(array, key) {
                GetKeyUnknownValue::Json(Value::Array(rows)) => Some(rows.as_slice()),
                // Keep the shared JSON model's pristine, immutable prototype.
                // Realm-owned later prototype mutations are an explicit cost.
                GetKeyUnknownValue::ArrayPrototype => Some(&[]),
                _ => None,
            };
        }
        js_object_or_array_entry(owner, key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
    };
    for key in keys {
        if let Some(rows) = data.and_then(|owner| rows_at(owner, key)) {
            return rows;
        }
        if let Some(rows) = data
            .and_then(|owner| owner.get("data"))
            .and_then(|owner| rows_at(owner, key))
        {
            return rows;
        }
    }
    &[]
}
// ts: MexcExchange.nativePositionId
pub fn native_position_id(position: &Position) -> Result<String, JsCoercionError> {
    let info = position.info.as_ref();
    let value = info
        .and_then(|value| value.get("positionId"))
        .filter(|value| !value.is_nullish())
        .or_else(|| info.and_then(|value| value.get("id")));
    value
        .filter(|value| !value.is_nullish())
        .map(|value| safe_cex_num::js_retained_string(Some(value)))
        .unwrap_or_else(|| Ok(String::new()))
}

// ts: MexcExchange.findOpenPosition
pub fn find_open_position<'a>(
    symbol: &str,
    side: PositionSide,
    positions: &'a [Position],
) -> Option<&'a Position> {
    positions.iter().find(|position| {
        position.symbol == symbol && position.side == side && position.contracts > 0.0
    })
}
// ts: MexcExchange.planOrderTriggerType
pub fn plan_order_trigger_type(opts: &PlaceOrderOpts) -> u8 {
    let closes_long = opts.side == OrderSide::Sell;
    if (opts.r#type == OrderType::TakeProfit) == closes_long {
        1
    } else {
        2
    }
}
// ts: MexcExchange.stopOrderTrend
pub fn stop_order_trend(opts: &PlaceOrderOpts) -> u8 {
    let trigger = if opts.r#type == OrderType::TakeProfit {
        opts.tp_trigger_type
    } else {
        opts.sl_trigger_type
    };
    match trigger {
        Some(OrderStopTriggerType::MarkPrice) => 2,
        Some(OrderStopTriggerType::IndexPrice) => 3,
        _ => 1,
    }
}
// ts: MexcExchange.isRegularPlaceOrder
pub fn is_regular_place_order(opts: &PlaceOrderOpts) -> bool {
    !matches!(
        opts.r#type,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}
// ts: MexcExchange.getReplacementOrderAmount
pub fn get_replacement_order_amount(order: &Order) -> Result<f64, JsCoercionError> {
    // The typed numeric field passes through toNumber: only finite positive
    // values win; -0, NaN and infinities continue to the raw venue aliases.
    if let Some(amount) = order
        .extra
        .as_ref()
        .and_then(|extra| extra.remaining_native)
        .filter(|n| n.is_finite() && *n > 0.0)
    {
        return Ok(amount);
    }
    let info = order.info.as_ref();
    let raw = ["vol", "quantity", "origQty"]
        .iter()
        .find_map(|key| info.and_then(|v| v.get(*key)).filter(|v| !v.is_nullish()));
    let native = if raw.is_none_or(|value| value.is_nullish() || value.text_equals("")) {
        f64::NAN
    } else {
        safe_cex_num::js_retained_parse_float(raw)?
    };
    Ok(if native.is_finite() && native > 0.0 {
        native
    } else {
        order.amount
    })
}
// ts: MexcExchange.isStopLikeOrder
pub fn is_stop_like_order(order: &Order) -> bool {
    matches!(
        order.r#type,
        safe_cex_types::ModelField::Value(OrderType::StopLoss)
            | safe_cex_types::ModelField::Value(OrderType::TakeProfit)
    ) || order
        .info
        .as_ref()
        .and_then(|v| v.get("__mexcPlanOrder"))
        .is_some_and(|value| value.is_truthy())
        || order
            .info
            .as_ref()
            .and_then(|v| v.get("__mexcStopOrder"))
            .is_some_and(|value| value.is_truthy())
}
// ts: MexcExchange.isStopOrder
pub fn is_stop_order(order: &Order) -> bool {
    order
        .info
        .as_ref()
        .and_then(|v| v.get("__mexcStopOrder"))
        .is_some_and(|value| value.is_truthy())
        || (matches!(
            order.r#type,
            safe_cex_types::ModelField::Value(OrderType::StopLoss)
                | safe_cex_types::ModelField::Value(OrderType::TakeProfit)
        ) && order.amount == 0.0
            && order.remaining == 0.0)
}
// ts: MexcExchange.isPlanOrder
pub fn is_plan_order(order: &Order) -> bool {
    is_stop_like_order(order) && !is_stop_order(order)
}
// ts: MexcExchange.orderListEndpoint
pub fn order_list_endpoint(symbol: Option<&str>) -> String {
    let endpoint = ENDPOINTS["OPEN_ORDERS"].as_str().unwrap_or("");
    symbol
        .filter(|s| !s.is_empty())
        .map_or_else(|| endpoint.into(), |symbol| format!("{endpoint}/{symbol}"))
}

// Shield maps are private exchange state. Their iteration order is never read
// by the source; all reads and writes address one suffix-normalized ID.
use std::collections::BTreeMap;

// ts: MexcExchange.markStopLikeOrderSeen
pub fn mark_stop_like_order_seen(placed: &mut BTreeMap<String, f64>, order_id: &str) {
    if !order_id.is_empty() {
        placed.remove(order_id.split("__").next().unwrap_or_default());
    }
}

// ts: MexcExchange.notePlacedStopLikeOrder
pub fn note_placed_stop_like_order(
    placed: &mut BTreeMap<String, f64>,
    order_id: &str,
    now: &dyn Fn() -> f64,
) {
    if !order_id.is_empty() {
        placed.insert(
            order_id.split("__").next().unwrap_or_default().into(),
            now(),
        );
    }
}

fn shield_contains(
    shield: &mut BTreeMap<String, f64>,
    order_id: &str,
    now: &dyn Fn() -> f64,
    ttl: f64,
) -> bool {
    let id = order_id.split("__").next().unwrap_or_default();
    let Some(at) = shield.get(id).copied() else {
        return false;
    };
    if now() - at > ttl {
        shield.remove(id);
        return false;
    }
    true
}

// ts: MexcExchange.isRecentlyPlacedStopLikeOrder
pub fn is_recently_placed_stop_like_order(
    placed: &mut BTreeMap<String, f64>,
    order: &Order,
    now: &dyn Fn() -> f64,
) -> bool {
    shield_contains(placed, &order.id, now, 20_000.0)
}

// ts: MexcExchange.noteRecentlyCanceledStopLikeOrders
pub fn note_recently_canceled_stop_like_orders(
    canceled: &mut BTreeMap<String, f64>,
    placed: &mut BTreeMap<String, f64>,
    orders: &[Order],
    now: &dyn Fn() -> f64,
) {
    let timestamp = now();
    for order in orders {
        let id = order.id.split("__").next().unwrap_or_default();
        canceled.insert(id.into(), timestamp);
        placed.remove(id);
    }
}

// ts: MexcExchange.isRecentlyCanceledStopLikeOrderId
pub fn is_recently_canceled_stop_like_order_id(
    canceled: &mut BTreeMap<String, f64>,
    order_id: &str,
    now: &dyn Fn() -> f64,
) -> bool {
    !order_id.is_empty() && shield_contains(canceled, order_id, now, 15_000.0)
}

// ts: MexcExchange.isRecentlyCanceledStopLikeOrder
pub fn is_recently_canceled_stop_like_order(
    canceled: &mut BTreeMap<String, f64>,
    order: &Order,
    now: &dyn Fn() -> f64,
) -> bool {
    is_recently_canceled_stop_like_order_id(canceled, &order.id, now)
}

// ts: MexcExchange.filterRecentlyCanceledStopLikeOrders
pub fn filter_recently_canceled_stop_like_orders(
    canceled: &mut BTreeMap<String, f64>,
    orders: &[Order],
    now: &dyn Fn() -> f64,
) -> Vec<Order> {
    orders
        .iter()
        .filter(|order| {
            !is_stop_like_order(order)
                || !is_recently_canceled_stop_like_order(canceled, order, now)
        })
        .cloned()
        .collect()
}

// ts: MexcExchange.isRecentlyRejectedRegularOrder
pub fn is_recently_rejected_regular_order(
    rejected: &mut BTreeMap<String, f64>,
    order_id: &str,
    now: &dyn Fn() -> f64,
) -> bool {
    // Unlike the cancellation predicate, the source does not guard an empty ID.
    shield_contains(rejected, order_id, now, 30_000.0)
}
