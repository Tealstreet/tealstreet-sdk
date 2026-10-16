//! Legacy order mapping through the shared Store and source table semantics.
use crate::{
    constants::{ORDER_SIDE, ORDER_TRIGGER_TYPE, ORDER_TYPE},
    utils::normalize_symbol,
};
use safe_cex_num::{
    TableHit,
    get_key::{GetKeyObject, GetKeyUnknownValue, GetKeyValue, v, v_unknown},
    js_string, js_table_lookup, js_truthy,
};
use safe_cex_store::Store;
use safe_cex_types::{Order, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType};
use serde_json::{Map, Value};

// ts: WOOXExchange#isAlgoOrder
pub fn is_algo_order(order_type: Option<&Value>) -> bool {
    matches!(
        order_type.and_then(Value::as_str),
        Some("stop_market" | "take_profit_market" | "trailing_stop_market")
    )
}

fn field<'a>(row: &'a Map<String, Value>, key: &str) -> Result<Option<&'a Value>, String> {
    match v(Some(GetKeyObject::Json(row)), key) {
        TableHit::Mapped(GetKeyValue::Json(value)) => Ok(Some(value)),
        TableHit::Mapped(GetKeyValue::Undefined) | TableHit::Missing => Ok(None),
        TableHit::Inherited(member) => {
            Err(format!("inherited input {key}: {}", member.js_string()))
        }
    }
}
fn text(value: Option<&Value>) -> Result<String, String> {
    js_string(value).map_err(|e| e.message.to_owned())
}
fn or<'a>(first: Option<&'a Value>, second: Option<&'a Value>) -> Option<&'a Value> {
    if js_truthy(first) { first } else { second }
}
fn number(value: Option<&Value>, name: &str) -> Result<f64, String> {
    if !value.is_some_and(Value::is_number) {
        return Err(format!(
            "typed Order.{name} cannot carry the source non-number"
        ));
    }
    safe_cex_num::js_number(value).map_err(|e| e.message.to_owned())
}
fn boolean(value: Option<&Value>) -> Result<bool, String> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| "typed Order.reduceOnly cannot carry the source non-boolean".to_owned())
}
fn required<T: Copy>(hit: TableHit<&T>, name: &str) -> Result<T, String> {
    match hit {
        TableHit::Mapped(value) => Ok(*value),
        TableHit::Inherited(member) => Err(format!(
            "typed Order.{name} refuses inherited {}",
            member.js_string()
        )),
        TableHit::Missing => Err(format!("typed Order.{name} cannot carry undefined")),
    }
}
fn trigger(hit: TableHit<&OrderStopTriggerType>) -> Result<Option<OrderStopTriggerType>, String> {
    match hit {
        TableHit::Missing => Ok(None),
        hit => required(hit, "triggerType").map(Some),
    }
}
fn symbol(value: Option<&Value>, store: &dyn Store) -> Result<Option<String>, String> {
    let normalized = if js_truthy(value) {
        normalize_symbol(
            value
                .and_then(Value::as_str)
                .ok_or_else(|| "TypeError: str.toUpperCase is not a function".to_owned())?,
        )
    } else {
        text(value)?
    };
    if !store.markets_by_symbol().contains_key(&normalized) {
        return Ok(None);
    }
    if !value.is_some_and(Value::is_string) {
        return Err("typed Order.symbol cannot carry the source non-string".to_owned());
    }
    Ok(Some(normalized))
}
// ts: WOOXExchange#mapLimitOrder
pub fn map_limit_order(
    row: &Map<String, Value>,
    store: &dyn Store,
    now: impl FnOnce() -> f64,
) -> Result<Option<Order>, String> {
    let order_id = or(
        or(field(row, "order_id")?, field(row, "orderId")?),
        field(row, "id")?,
    );
    let raw_symbol = field(row, "symbol")?;
    if !js_truthy(order_id) || !js_truthy(raw_symbol) {
        return Ok(None);
    }
    let Some(symbol) = symbol(raw_symbol, store)? else {
        return Ok(None);
    };
    let quantity = number(or(row.get("quantity"), row.get("order_quantity")), "amount")?;
    let filled = if js_truthy(row.get("executed")) {
        number(row.get("executed"), "filled")?
    } else {
        0.0
    };
    let order_type = text(or(row.get("type"), row.get("order_type")))?;
    let side = text(row.get("side"))?;
    Ok(Some(Order {
        client_order_id: None,
        id: text(order_id)?,
        parent_id: None,
        status: if row.get("status").and_then(Value::as_str) == Some("CANCELLED") {
            OrderStatus::Canceled
        } else {
            OrderStatus::Open
        },
        symbol,
        r#type: required(js_table_lookup(ORDER_TYPE, &order_type), "type")?,
        side: required(js_table_lookup(ORDER_SIDE, &side), "side")?,
        price: number(or(row.get("price"), row.get("order_price")), "price")?,
        amount: quantity,
        filled,
        remaining: safe_cex_num::subtract(quantity, filled),
        reduce_only: boolean(field(row, "reduce_only")?)?,
        time_in_force: Some(
            if row.get("type").and_then(Value::as_str) == Some("POST_ONLY") {
                OrderTimeInForce::PostOnly
            } else {
                OrderTimeInForce::GoodTillCancel
            },
        ),
        last_update: now(),
        info: Some(Value::Object(row.clone())),
        close: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }))
}
// ts: WOOXExchange#mapAlgoOrder
pub fn map_algo_order(
    row: &Map<String, Value>,
    store: &dyn Store,
    mut now: impl FnMut() -> f64,
) -> Result<Vec<Order>, String> {
    let Some(symbol) = symbol(row.get("symbol"), store)? else {
        return Ok(Vec::new());
    };
    let singleton = Value::Object(row.clone());
    let children = if js_truthy(row.get("childOrders")) {
        row.get("childOrders")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or_else(|| "TypeError: arr.filter is not a function".to_owned())?
    } else {
        std::slice::from_ref(&singleton)
    };
    // Complete source filtering, including primitive RHS failures, before mapping.
    let mut selected = Vec::new();
    for child in children {
        let trigger_price =
            v_unknown(Some(child), "triggerPrice").map_err(|error| error.message)?;
        if !trigger_price.is_truthy()
            && !matches!(
                v_unknown(Some(child), "algoType").map_err(|error| error.message)?,
                GetKeyUnknownValue::Json(Value::String(value)) if value == "TRAILING_STOP"
            )
        {
            continue;
        }
        if let Value::Object(child) = child {
            selected.push(child);
        }
    }
    let mut orders = Vec::new();
    let mut refusal = None;
    for child in selected {
        let trigger_price = field(child, "triggerPrice")?;
        let algo_type = field(child, "algoType")?;
        let filled = number(field(child, "totalExecutedQuantity")?, "filled")?;
        let price = if js_truthy(trigger_price) {
            number(trigger_price, "price")?
        } else {
            safe_cex_num::subtract(
                number(field(child, "extremePrice")?, "extremePrice")?,
                number(field(child, "callbackValue")?, "callbackValue")?,
            )
        };
        let type_key = text(algo_type)?;
        let type_hit = js_table_lookup(ORDER_TYPE, &type_key);
        // Source uses strict enum equality, not the hit's truthiness.
        let stop = matches!(
            type_hit,
            TableHit::Mapped(OrderType::TrailingStopLoss | OrderType::StopLoss)
        );
        // Evaluate source coercions and clock before refusing closed output
        // fields. A later source error must win over this deferred refusal.
        let id = text(field(child, "algoOrderId")?)?;
        let parent_id = text(or(
            field(row, "rootAlgoOrderId")?,
            field(child, "rootAlgoOrderId")?,
        ))?;
        let trigger_key = text(field(child, "triggerPriceType")?)?;
        let trigger_type = trigger(js_table_lookup(ORDER_TRIGGER_TYPE, &trigger_key));
        let side_key = text(child.get("side"))?;
        let side_hit = js_table_lookup(ORDER_SIDE, &side_key);
        let quantity = number(child.get("quantity"), "amount")?;
        let reduce_only = boolean(field(child, "reduceOnly")?)?;
        let remaining = safe_cex_num::subtract(quantity, filled);
        let last_update = now();
        let mapped: Result<Order, String> = (|| {
            let order_type = required(type_hit, "type")?;
            let trigger_type = trigger_type?;
            let side = required(side_hit, "side")?;
            Ok(Order {
                client_order_id: None,
                id,
                parent_id: Some(parent_id),
                status: OrderStatus::Open,
                symbol: symbol.clone(),
                r#type: order_type,
                side,
                price,
                amount: quantity,
                reduce_only,
                filled,
                remaining,
                last_update,
                info: Some(Value::Object(row.clone())),
                sl_trigger_type: if stop { trigger_type } else { None },
                tp_trigger_type: if stop { None } else { trigger_type },
                time_in_force: None,
                close: None,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: None,
            })
        })();
        match mapped {
            Ok(order) => orders.push(order),
            Err(error) => {
                refusal.get_or_insert(error);
            }
        }
    }
    // Never expose a successful subset when any output record was refused.
    if let Some(error) = refusal {
        return Err(error);
    }
    Ok(orders)
}
