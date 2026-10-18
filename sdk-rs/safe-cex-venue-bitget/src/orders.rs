//! Classic v2 and Unified v3 order aliases, evaluated before typed projection.
use crate::{
    numeric::{FiniteNumberInput, to_finite_number},
    protocol::{ORDER_SIDE, ORDER_STATUS, ORDER_TYPE, TIME_IN_FORCE},
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, TableHit, js_string, js_table_lookup, js_truthy, parse_float};
use safe_cex_types::{
    ModelField, Order, OrderExtra, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce,
    OrderType,
};
use serde_json::Value;

fn failure(name: &str, message: &str) -> ExchangeError {
    ExchangeError {
        name: name.into(),
        message: message.into(),
        metadata: Box::default(),
    }
}
fn coercion(error: JsCoercionError) -> ExchangeError {
    failure(error.name, error.message)
}
fn aliases<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .map(|key| input.get(key))
        .find(|v| js_truthy(*v))
        .flatten()
        .or_else(|| keys.last().and_then(|key| input.get(key)))
}
fn float(value: Option<&Value>) -> Result<f64, ExchangeError> {
    Ok(parse_float(&js_string(value).map_err(coercion)?))
}
fn lookup<T: Copy>(table: &[(&str, T)], key: Option<&Value>) -> Result<TableHit<T>, ExchangeError> {
    Ok(
        match js_table_lookup(table, &js_string(key).map_err(coercion)?) {
            TableHit::Mapped(value) => TableHit::Mapped(*value),
            TableHit::Inherited(member) => TableHit::Inherited(member),
            TableHit::Missing => TableHit::Missing,
        },
    )
}
fn finite(value: Option<&Value>) -> f64 {
    to_finite_number(
        match value {
            Some(Value::String(s)) => FiniteNumberInput::String(s),
            Some(Value::Number(n)) => n
                .as_f64()
                .map(FiniteNumberInput::Number)
                .unwrap_or(FiniteNumberInput::Other),
            _ => FiniteNumberInput::Other,
        },
        0.0,
    )
}
// A missed table lookup assigned in the source object literal is an own undefined.
fn typed<T>(hit: TableHit<T>, field: &str) -> Result<ModelField<T>, ExchangeError> {
    match hit {
        TableHit::Mapped(value) => Ok(ModelField::Value(value)),
        TableHit::Missing => Ok(ModelField::Undefined),
        TableHit::Inherited(_) => Err(failure(
            "RepresentationError",
            &format!("mapOrder cannot represent {field} in shared Order"),
        )),
    }
}

fn raw_side_fallback(raw: Option<&Value>) -> (TableHit<OrderSide>, bool) {
    match raw {
        Some(Value::String(raw)) if raw == "buy" => (TableHit::Mapped(OrderSide::Buy), false),
        Some(Value::String(raw)) if raw == "sell" => (TableHit::Mapped(OrderSide::Sell), false),
        None => (TableHit::Missing, false),
        Some(_) => (TableHit::Missing, true),
    }
}

// Private evaluation state, not an alternative public Order model. Inherited
// values must survive until all source branches have had a chance to replace them.
#[derive(Clone)]
struct PendingOrder {
    id: Option<Value>,
    status: TableHit<OrderStatus>,
    symbol: String,
    kind: TableHit<OrderType>,
    side: TableHit<OrderSide>,
    // `ORDER_SIDE[...] || o.side` keeps an unmapped raw side, which Order cannot carry.
    unmapped_raw_side: bool,
    price: f64,
    amount: f64,
    filled: f64,
    remaining: f64,
    last_update: f64,
    reduce_only: bool,
    time_in_force: TableHit<OrderTimeInForce>,
    tp: Option<OrderStopTriggerType>,
    sl: Option<OrderStopTriggerType>,
    remaining_native: Option<f64>,
}
fn project(order: PendingOrder, info: &Value) -> Result<Order, ExchangeError> {
    Ok(Order {
        client_order_id: (None).into(),
        id: order
            .id
            .as_ref()
            .and_then(Value::as_str)
            .ok_or_else(|| {
                failure(
                    "RepresentationError",
                    "mapOrder cannot represent id in shared Order",
                )
            })?
            .into(),
        parent_id: (None).into(),
        status: typed(order.status, "status")?,
        symbol: order.symbol,
        r#type: typed(order.kind, "type")?,
        side: if order.side == TableHit::Missing && order.unmapped_raw_side {
            return Err(failure(
                "RepresentationError",
                "mapOrder cannot represent side in shared Order",
            ));
        } else {
            typed(order.side, "side")?
        },
        price: (order.price).into(),
        amount: order.amount,
        filled: order.filled,
        remaining: order.remaining,
        reduce_only: order.reduce_only.into(),
        close: (None).into(),
        info: (Some(info.clone().into())).into(),
        time_in_force: typed(order.time_in_force, "timeInForce")?,
        last_update: order.last_update,
        trigger_type: (None).into(),
        tp_trigger_type: (order.tp).into(),
        sl_trigger_type: (order.sl).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (order.remaining_native.map(|remaining_native| OrderExtra {
            remaining_native: Some(remaining_native),
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
    })
}

// ts: BitgetExchange.mapOrder
pub fn map_order(
    input: Option<&Value>,
    api_product_type: &str,
    mut now: impl FnMut() -> f64,
) -> Result<Vec<Order>, ExchangeError> {
    let o = input.filter(|v| !v.is_null()).ok_or_else(|| {
        failure(
            "TypeError",
            if input.is_none() {
                "Cannot read properties of undefined (reading 'size')"
            } else {
                "Cannot read properties of null (reading 'size')"
            },
        )
    })?;
    let zero = Value::String("0".into());
    let raw_amount = aliases(o, &["size", "sz", "qty", "amount"])
        .filter(|v| js_truthy(Some(v)))
        .unwrap_or(&zero);
    let raw_filled = aliases(o, &["baseVolume", "filledQty", "accFillSz", "cumExecQty"])
        .filter(|v| js_truthy(Some(v)))
        .unwrap_or(&zero);
    let empty = Value::String(String::new());
    let symbol = aliases(o, &["symbol", "instId"])
        .filter(|v| js_truthy(Some(v)))
        .unwrap_or(&empty);
    let symbol = symbol.as_str().ok_or_else(|| {
        failure(
            "TypeError",
            "(o.symbol || o.instId || \"\").replace is not a function",
        )
    })?;
    let symbol =
        safe_cex_num::js_replace(symbol, &format!("_{}", api_product_type.to_uppercase()), "");
    let id = aliases(o, &["orderId", "clientOid", "ordId", "id"]).cloned();
    let status = lookup(
        ORDER_STATUS,
        aliases(o, &["status", "state", "planStatus", "orderStatus"]),
    )?;
    let kind = lookup(
        ORDER_TYPE,
        aliases(o, &["orderType", "ordType", "planType"]),
    )?;
    // ts-bug: inherited ORDER_SIDE hits suppress the raw-side fallback.
    let side = lookup(ORDER_SIDE, aliases(o, &["tradeSide", "tS", "side"]))?;
    let (side, unmapped_raw_side) = if side.is_truthy_by(|_| true) {
        (side, false)
    } else {
        raw_side_fallback(o.get("side"))
    };
    let price = float(Some(
        aliases(o, &["price", "px"])
            .filter(|v| js_truthy(Some(v)))
            .unwrap_or(&zero),
    ))?;
    let amount = float(Some(raw_amount))?;
    let filled = float(Some(raw_filled))?;
    let last_update = now();
    let reduce_only = matches!(
        js_string(o.get("reduceOnly")).map_err(coercion)?.as_str(),
        "YES" | "yes" | "true"
    );
    let remaining = safe_cex_num::subtract(float(Some(raw_amount))?, float(Some(raw_filled))?);
    let time_in_force = lookup(TIME_IN_FORCE, aliases(o, &["force", "timeInForce", "tiF"]))?;
    let time_in_force = if time_in_force.is_truthy_by(|_| true) {
        time_in_force
    } else {
        TableHit::Mapped(OrderTimeInForce::GoodTillCancel)
    };
    let mut order = PendingOrder {
        id,
        status,
        symbol,
        kind,
        side,
        unmapped_raw_side,
        price,
        amount,
        filled,
        remaining,
        last_update,
        reduce_only,
        time_in_force,
        tp: None,
        sl: None,
        remaining_native: None,
    };
    let is = |key: &str, value: &str| o.get(key).and_then(Value::as_str) == Some(value);
    let trigger = |last: bool| {
        if last {
            OrderStopTriggerType::LastPrice
        } else {
            OrderStopTriggerType::MarkPrice
        }
    };
    let opposite_raw_side = || {
        if is("side", "buy") {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        }
    };
    if js_truthy(o.get("planType")) {
        order.kind = lookup(ORDER_TYPE, o.get("planType"))?;
        order.price = float(o.get("triggerPrice"))?;
        if order.price == 0.0 || order.price.is_nan() {
            order.price = float(o.get("triggerPx"))?;
        }
        order.reduce_only = true;
        let trigger_type = trigger(is("triggerType", "fill_price"));
        if order.kind == TableHit::Mapped(OrderType::TakeProfit) {
            order.tp = Some(trigger_type);
        } else if order.kind == TableHit::Mapped(OrderType::StopLoss) {
            order.sl = Some(trigger_type);
        }
        if is("posMode", "hedge_mode") {
            order.side = TableHit::Mapped(opposite_raw_side());
        }
    } else if !order.kind.is_truthy_by(|_| true)
        && (finite(o.get("stopLoss")) > 0.0 || finite(o.get("takeProfit")) > 0.0)
    {
        let strategy_side = lookup(ORDER_SIDE, o.get("side"))?;
        order.side = if strategy_side.is_truthy_by(|_| true) {
            strategy_side
        } else if is("posSide", "short") {
            TableHit::Mapped(OrderSide::Buy)
        } else if is("posSide", "long") {
            TableHit::Mapped(OrderSide::Sell)
        } else {
            order.side
        };
        let mut orders = Vec::new();
        for (key, kind, by) in [
            ("stopLoss", OrderType::StopLoss, "slTriggerBy"),
            ("takeProfit", OrderType::TakeProfit, "tpTriggerBy"),
        ] {
            let price = finite(o.get(key));
            if price > 0.0 {
                let mut leg = order.clone();
                leg.kind = TableHit::Mapped(kind);
                leg.price = price;
                leg.reduce_only = true;
                if kind == OrderType::StopLoss {
                    leg.sl = Some(trigger(is(by, "market")));
                } else {
                    leg.tp = Some(trigger(is(by, "market")));
                }
                orders.push(leg);
            }
        }
        return orders.into_iter().map(|order| project(order, o)).collect();
    } else if is("reduceOnly", "YES") && is("posMode", "hedge_mode") {
        order.side = TableHit::Mapped(opposite_raw_side());
    }
    let mut orders = vec![order.clone()];
    if order.kind == TableHit::Mapped(OrderType::Limit) {
        for (keys, suffix, kind) in [
            (
                &["presetStopLossPrice", "stopLossTriggerPrice", "stopLoss"][..],
                "__stop_loss",
                OrderType::StopLoss,
            ),
            (
                &[
                    "presetStopSurplusPrice",
                    "stopSurplusTriggerPrice",
                    "presetTakeProfitPrice",
                    "takeProfit",
                ][..],
                "__take_profit",
                OrderType::TakeProfit,
            ),
        ] {
            let price = aliases(o, keys);
            if js_truthy(price) {
                let mut leg = order.clone();
                leg.id = Some(Value::String(format!(
                    "{}{suffix}",
                    js_string(order.id.as_ref()).map_err(coercion)?
                )));
                leg.side = TableHit::Mapped(if order.side == TableHit::Mapped(OrderSide::Buy) {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                });
                leg.kind = TableHit::Mapped(kind);
                leg.price = float(price)?;
                orders.push(leg);
            }
        }
    }
    orders.into_iter().map(|order| project(order, o)).collect()
}

// ts: BitgetInverseExchange.mapOrder
/// The lookup projects the shared Market.is_inverse field without copying Store state.
pub fn map_inverse_order(
    input: Option<&Value>,
    api_product_type: &str,
    mut market_is_inverse: impl FnMut(&str) -> Option<bool>,
    mut now: impl FnMut() -> f64,
) -> Result<Vec<Order>, ExchangeError> {
    let o = input.filter(|v| !v.is_null()).ok_or_else(|| {
        failure(
            "TypeError",
            if input.is_none() {
                "Cannot read properties of undefined (reading 'symbol')"
            } else {
                "Cannot read properties of null (reading 'symbol')"
            },
        )
    })?;
    let raw_symbol = aliases(o, &["symbol", "instId"]);
    let symbol = match raw_symbol {
        Some(Value::String(s)) => s,
        None => {
            return Err(failure(
                "TypeError",
                "Cannot read properties of undefined (reading 'replace')",
            ));
        }
        Some(Value::Null) => {
            return Err(failure(
                "TypeError",
                "Cannot read properties of null (reading 'replace')",
            ));
        }
        _ => {
            return Err(failure(
                "TypeError",
                "(o.symbol || o.instId).replace is not a function",
            ));
        }
    };
    let symbol =
        safe_cex_num::js_replace(symbol, &format!("_{}", api_product_type.to_uppercase()), "");
    let zero = Value::String("0".into());
    let price = float(Some(
        aliases(o, &["price", "px"])
            .filter(|v| js_truthy(Some(v)))
            .unwrap_or(&zero),
    ))?;
    let raw_amount = aliases(o, &["size", "sz"])
        .filter(|v| js_truthy(Some(v)))
        .unwrap_or(&zero);
    let raw_filled = aliases(o, &["baseVolume", "filledQty", "accFillSz"])
        .filter(|v| js_truthy(Some(v)))
        .unwrap_or(&zero);
    let remaining_native =
        safe_cex_num::subtract(float(Some(raw_amount))?, float(Some(raw_filled))?);
    let market = market_is_inverse(&symbol);
    let remaining = if market == Some(true) && price > 0.0 {
        safe_cex_num::math_round(remaining_native / price * 1e8) / 1e8
    } else {
        remaining_native
    };
    let id = aliases(o, &["orderId", "ordId", "id"]).cloned();
    let status = lookup(ORDER_STATUS, aliases(o, &["status", "state", "planStatus"]))?;
    let kind = lookup(
        ORDER_TYPE,
        aliases(o, &["orderType", "ordType", "planType"]),
    )?;
    // ts-bug: inherited table values suppress the raw-side fallback; retain the
    // hit until the later hedge branch, which may overwrite it, has executed.
    let side = lookup(ORDER_SIDE, aliases(o, &["tradeSide", "tS", "side"]))?;
    let (side, unmapped_raw_side) = if side.is_truthy_by(|_| true) {
        (side, false)
    } else {
        raw_side_fallback(o.get("side"))
    };
    let amount = float(Some(raw_amount))?;
    let filled = float(Some(raw_filled))?;
    let last_update = now();
    // Source calls an optional method; it does NOT call String(reduceOnly).
    let reduce_only = match o.get("reduceOnly") {
        Some(Value::String(s)) => s.to_uppercase() == "YES",
        Some(Value::Object(fields)) if fields.get("toUpperCase").is_some_and(|v| !v.is_null()) => {
            return Err(failure(
                "TypeError",
                "o.reduceOnly?.toUpperCase is not a function",
            ));
        }
        _ => false,
    };
    let time_in_force = lookup(TIME_IN_FORCE, aliases(o, &["force", "timeInForce", "tiF"]))?;
    let time_in_force = if time_in_force.is_truthy_by(|_| true) {
        time_in_force
    } else {
        TableHit::Mapped(OrderTimeInForce::GoodTillCancel)
    };
    let mut order = PendingOrder {
        id,
        status,
        symbol,
        kind,
        side,
        unmapped_raw_side,
        price,
        amount,
        filled,
        remaining,
        last_update,
        reduce_only,
        time_in_force,
        tp: None,
        sl: None,
        remaining_native: Some(remaining_native),
    };
    let is = |key: &str, value: &str| o.get(key).and_then(Value::as_str) == Some(value);
    if js_truthy(o.get("planType")) {
        order.kind = lookup(ORDER_TYPE, o.get("planType"))?;
        order.price = float(o.get("triggerPrice"))?;
        if order.price == 0.0 || order.price.is_nan() {
            order.price = float(o.get("triggerPx"))?;
        }
        order.reduce_only = true;
        if is("posMode", "hedge_mode") {
            order.side = TableHit::Mapped(if is("side", "buy") {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            });
        }
    } else if is("reduceOnly", "YES") && is("posMode", "hedge_mode") {
        order.side = TableHit::Mapped(if is("side", "buy") {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        });
    }
    let mut orders = vec![order.clone()];
    if order.kind == TableHit::Mapped(OrderType::Limit) {
        for (keys, suffix, kind) in [
            (
                &["presetStopLossPrice", "stopLossTriggerPrice"][..],
                "__stop_loss",
                OrderType::StopLoss,
            ),
            (
                &["presetStopSurplusPrice", "stopSurplusTriggerPrice"][..],
                "__take_profit",
                OrderType::TakeProfit,
            ),
        ] {
            let price = aliases(o, keys);
            if js_truthy(price) {
                let mut leg = order.clone();
                leg.id = Some(Value::String(format!(
                    "{}{suffix}",
                    js_string(order.id.as_ref()).map_err(coercion)?
                )));
                leg.side = TableHit::Mapped(if order.side == TableHit::Mapped(OrderSide::Buy) {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                });
                leg.kind = TableHit::Mapped(kind);
                leg.price = float(price)?;
                orders.push(leg);
            }
        }
    }
    orders.into_iter().map(|order| project(order, o)).collect()
}
