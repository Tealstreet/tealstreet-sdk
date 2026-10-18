//! BlofinExchange.mapOrders, preserving branch and per-output clock order.
use safe_cex_num::{
    TableHit, js_table_lookup, js_truthy, multiply, numeric_ingress::JsNumberOutput, parse_float,
    subtract,
};
use safe_cex_store::Store;
use safe_cex_types::{
    Order, OrderExtra, OrderSide, OrderStatus, OrderTimeInForce, OrderType,
    js_coercion::{JsCoercionError, js_string},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrdersInput {
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    value: Value,
}
impl OrdersInput {
    pub(crate) fn from_retained(value: Value) -> Self {
        Self { value }
    }
}
impl JsNumberOutput for OrdersInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value });
    }
}
fn error(name: &'static str, message: &'static str) -> JsCoercionError {
    JsCoercionError { name, message }
}
fn representation() -> JsCoercionError {
    error(
        "RepresentationError",
        "Source order is outside the shared typed Order model",
    )
}
fn number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    Ok(parse_float(&js_string(value)?))
}
fn zero_number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    if js_truthy(value) {
        number(value)
    } else {
        Ok(0.0)
    }
}
fn lookup(table: &Value, key: &str) -> Option<String> {
    let entries: Vec<_> = table
        .as_object()
        .expect("constant table")
        .iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect();
    match js_table_lookup(&entries, key) {
        TableHit::Mapped(v) => v.as_str().map(str::to_owned),
        _ => None,
    }
}
fn side(value: Option<&str>) -> Result<OrderSide, JsCoercionError> {
    match value {
        Some("buy") => Ok(OrderSide::Buy),
        Some("sell") => Ok(OrderSide::Sell),
        _ => Err(representation()),
    }
}
fn status(value: Option<&str>) -> Result<OrderStatus, JsCoercionError> {
    match value {
        Some("open") => Ok(OrderStatus::Open),
        _ => Err(representation()),
    }
}
fn kind(value: Option<&str>) -> Result<OrderType, JsCoercionError> {
    match value {
        Some("limit") => Ok(OrderType::Limit),
        Some("market") => Ok(OrderType::Market),
        _ => Err(representation()),
    }
}
fn order_extra(remaining: f64, filled: Option<f64>) -> OrderExtra {
    OrderExtra {
        remaining_native: Some(remaining),
        filled_in_placement_units: filled,
        remaining_notional: None,
        conditional_order_price: None,
        reject_reason: None,
        is_from_chaser: None,
        chaser_id: None,
        chaser_attempts: None,
        chaser_max_attempts: None,
    }
}
/// Clock is an infallible, non-mutating host callback, invoked once per emitted order.
pub fn map_orders(
    data: &OrdersInput,
    store: &dyn Store,
    now: impl FnMut() -> f64,
) -> Result<Vec<Order>, JsCoercionError> {
    map_orders_with_projection(data, store, now, false)
}
/// The private consumer reconciles terminal status immediately and never commits
/// unknown statuses. Materialize that transient source-undefined field without
/// rewriting raw info or weakening the standalone mapOrders boundary.
pub fn map_orders_for_private_topic(
    data: &OrdersInput,
    store: &dyn Store,
    now: impl FnMut() -> f64,
) -> Result<Vec<Order>, JsCoercionError> {
    map_orders_with_projection(data, store, now, true)
}
fn map_orders_with_projection(
    data: &OrdersInput,
    store: &dyn Store,
    mut now: impl FnMut() -> f64,
    private_topic: bool,
) -> Result<Vec<Order>, JsCoercionError> {
    let rows = data.value.as_array().ok_or_else(|| {
        error(
            "TypeError",
            if data.value.is_null() {
                "Cannot read properties of null (reading 'reduce')"
            } else {
                "orders.reduce is not a function"
            },
        )
    })?;
    let mut result = Vec::new();
    for row in rows {
        // Reuse the previously ported helper with the same retained input contract.
        let payload = crate::utility::UtilityInput::from_retained(row.clone());
        if crate::utility::is_known_non_swap_instrument(&payload)? {
            continue;
        }
        let key = js_string(row.get("instId"))?;
        let markets = store.markets_by_id();
        let Some(market) = markets.get(&key) else {
            if matches!(
                js_table_lookup::<&str, ()>(&[], &key),
                TableHit::Inherited(_)
            ) {
                // Stock inherited values have no info. Source evaluates these
                // two coercions before failing at market.info.contractValue.
                zero_number(row.get("triggerPrice"))?;
                js_string(row.get("side"))?;
                return Err(error(
                    "TypeError",
                    "Cannot read properties of undefined (reading 'contractValue')",
                ));
            }
            continue;
        };
        let trigger_price = zero_number(row.get("triggerPrice"))?;
        let reduce = row
            .get("reduceOnly")
            .is_some_and(|v| *v == true || v.as_str() == Some("true"));
        let opening = row.get("orderType").and_then(Value::as_str) == Some("trigger")
            && js_truthy(row.get("algoId"))
            && !reduce
            && trigger_price.is_finite()
            && trigger_price > 0.0;
        let algo = js_truthy(row.get("tpslId")) && !opening;
        let order_side = lookup(&crate::types::ORDER_SIDE, &js_string(row.get("side"))?);
        let inverse = if order_side.as_deref() == Some("buy") {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };
        let contract = if market
            .contract_size
            .is_some_and(|v| v != 0.0 && !v.is_nan())
        {
            market.contract_size.unwrap()
        } else {
            let info = market.info.as_ref().ok_or_else(|| {
                error(
                    "TypeError",
                    "Cannot read properties of undefined (reading 'contractValue')",
                )
            })?;
            if info.is_null() {
                return Err(error(
                    "TypeError",
                    "Cannot read properties of null (reading 'contractValue')",
                ));
            }
            let parsed = number(info.get("contractValue"))?;
            if parsed != 0.0 && !parsed.is_nan() {
                parsed
            } else {
                1.0
            }
        };
        let amount_native = zero_number(row.get("size"))?;
        let amount = multiply(amount_native, contract);
        // Template contains only shared-model absent fields; each branch assigns source fields.
        let make = |id: String,
                    status: OrderStatus,
                    r#type: OrderType,
                    side: OrderSide,
                    price: f64,
                    filled,
                    remaining,
                    extra,
                    reduce_only: bool,
                    time_in_force: Option<OrderTimeInForce>,
                    last_update| Order {
            client_order_id: (None).into(),
            id,
            parent_id: (None).into(),
            status: status.into(),
            symbol: market.symbol.clone(),
            r#type: r#type.into(),
            side: side.into(),
            price: price.into(),
            amount,
            filled,
            remaining,
            reduce_only: reduce_only.into(),
            close: (None).into(),
            info: (Some(row.clone())).into(),
            time_in_force: time_in_force.into(),
            last_update,
            trigger_type: (None).into(),
            tp_trigger_type: (None).into(),
            sl_trigger_type: (None).into(),
            take_profit: (None).into(),
            stop_loss: (None).into(),
            extra: (Some(extra)).into(),
        };
        if opening {
            let id = js_string(row.get("algoId"))?;
            let status = match row.get("state").and_then(Value::as_str) {
                Some("canceled" | "order_failed") => OrderStatus::Canceled,
                Some("effective") => OrderStatus::Closed,
                _ => OrderStatus::Open,
            };
            let side = side(order_side.as_deref())?;
            result.push(make(
                id,
                status,
                OrderType::Conditional,
                side,
                trigger_price,
                0.0,
                amount,
                order_extra(amount_native, None),
                false,
                None,
                now(),
            ));
        }
        for (field, suffix, r#type) in [
            ("tpTriggerPrice", "__take_profit", OrderType::TakeProfit),
            ("slTriggerPrice", "__stop_loss", OrderType::StopLoss),
        ] {
            if algo && js_truthy(row.get(field)) {
                let id = format!("{}{suffix}", js_string(row.get("tpslId"))?);
                let side = if js_truthy(row.get("orderId")) {
                    inverse
                } else {
                    side(order_side.as_deref())?
                };
                let price = number(row.get(field))?;
                result.push(make(
                    id,
                    OrderStatus::Open,
                    r#type,
                    side,
                    price,
                    0.0,
                    amount,
                    order_extra(amount_native, None),
                    true,
                    None,
                    now(),
                ));
            }
        }
        if js_truthy(row.get("orderId"))
            && row.get("orderCategory").and_then(Value::as_str) == Some("normal")
            && !algo
        {
            let filled_native = zero_number(row.get("filledSize"))?;
            let filled = multiply(filled_native, contract);
            let remaining = subtract(amount, filled);
            let remaining_native = subtract(amount_native, filled_native);
            let id = row
                .get("orderId")
                .and_then(Value::as_str)
                .ok_or_else(representation)?
                .to_owned();
            let raw_state = js_string(row.get("state"))?;
            let mapped_status = lookup(&crate::types::ORDER_STATUS, &raw_state);
            let status = if private_topic && mapped_status.is_none() {
                match raw_state.as_str() {
                    "filled" | "effective" => OrderStatus::Closed,
                    "canceled" | "order_failed" => OrderStatus::Canceled,
                    _ => OrderStatus::Open, // Uncommitted: no live/partial branch uses it.
                }
            } else {
                status(mapped_status.as_deref())?
            };
            let r#type = kind(
                lookup(&crate::types::ORDER_TYPE, &js_string(row.get("orderType"))?).as_deref(),
            )?;
            let side =
                side(lookup(&crate::types::ORDER_SIDE, &js_string(row.get("side"))?).as_deref())?;
            let price = number(row.get("price"))?;
            let reduce = row.get("reduceOnly").and_then(Value::as_str) == Some("true");
            let time = if row.get("orderType").and_then(Value::as_str) == Some("post_only") {
                OrderTimeInForce::PostOnly
            } else {
                OrderTimeInForce::GoodTillCancel
            };
            result.push(make(
                id,
                status,
                r#type,
                side,
                price,
                filled,
                remaining,
                order_extra(remaining_native, Some(filled_native)),
                reduce,
                Some(time),
                now(),
            ));
        }
    }
    Ok(result)
}
