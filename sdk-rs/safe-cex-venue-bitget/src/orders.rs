//! Classic v2 and Unified v3 order aliases, evaluated before typed projection.
use crate::protocol::{ORDER_SIDE, ORDER_STATUS, ORDER_TYPE, TIME_IN_FORCE};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, TableHit, js_retained_string as js_string, js_table_lookup};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    ModelField, Order, OrderExtra, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce,
    OrderType,
};

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
fn aliases<'a>(input: &'a Input, keys: &[&str]) -> Option<&'a Input> {
    keys.iter()
        .map(|key| input.get(key))
        .find(|v| truthy(*v))
        .flatten()
        .or_else(|| keys.last().and_then(|key| input.get(key)))
}
fn truthy(value: Option<&Input>) -> bool {
    value.is_some_and(Input::is_truthy)
}
fn float(value: Option<&Input>) -> Result<f64, ExchangeError> {
    safe_cex_num::js_retained_parse_float(value).map_err(coercion)
}
fn lookup<T: Copy>(table: &[(&str, T)], key: Option<&Input>) -> Result<TableHit<T>, ExchangeError> {
    let units =
        safe_cex_types::js_coercion::js_retained_string_utf16(key.unwrap_or(&Input::Undefined))
            .map_err(coercion)?;
    let Ok(key) = String::from_utf16(&units) else {
        return Ok(TableHit::Missing);
    };
    Ok(match js_table_lookup(table, &key) {
        TableHit::Mapped(value) => TableHit::Mapped(*value),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Missing,
    })
}
fn finite(value: Option<&Input>) -> Result<f64, ExchangeError> {
    let number = match value {
        Some(Input::Float(_) | Input::Lexeme(_)) => {
            safe_cex_num::js_retained_number(value).map_err(coercion)?
        }
        Some(Input::Text(_)) => float(value)?,
        _ => return Ok(0.0),
    };
    Ok(if number.is_finite() { number } else { 0.0 })
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

fn raw_side_fallback(raw: Option<&Input>) -> (TableHit<OrderSide>, bool) {
    match raw {
        Some(raw) if raw.text_equals("buy") => (TableHit::Mapped(OrderSide::Buy), false),
        Some(raw) if raw.text_equals("sell") => (TableHit::Mapped(OrderSide::Sell), false),
        None | Some(Input::Undefined) => (TableHit::Missing, false),
        Some(_) => (TableHit::Missing, true),
    }
}

// Private evaluation state, not an alternative public Order model. Inherited
// values must survive until all source branches have had a chance to replace them.
#[derive(Clone)]
struct PendingOrder {
    id: Option<Input>,
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
fn project(order: PendingOrder, info: &Input) -> Result<Order, ExchangeError> {
    Ok(Order {
        client_order_id: (None).into(),
        id: order
            .id
            .as_ref()
            .and_then(Input::as_text)
            .and_then(|units| String::from_utf16(units).ok())
            .ok_or_else(|| {
                failure(
                    "RepresentationError",
                    "mapOrder cannot represent id in shared Order",
                )
            })?,
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
        info: (Some(info.clone())).into(),
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
    input: Option<&Input>,
    api_product_type: &str,
    mut now: impl FnMut() -> f64,
) -> Result<Vec<Order>, ExchangeError> {
    let o = input.filter(|v| !v.is_nullish()).ok_or_else(|| {
        failure(
            "TypeError",
            if input.is_none_or(|value| matches!(value, Input::Undefined)) {
                "Cannot read properties of undefined (reading 'size')"
            } else {
                "Cannot read properties of null (reading 'size')"
            },
        )
    })?;
    let zero = Input::Text(vec![48]);
    let raw_amount = aliases(o, &["size", "sz", "qty", "amount"])
        .filter(|v| truthy(Some(v)))
        .unwrap_or(&zero);
    let raw_filled = aliases(o, &["baseVolume", "filledQty", "accFillSz", "cumExecQty"])
        .filter(|v| truthy(Some(v)))
        .unwrap_or(&zero);
    let empty = Input::Text(Vec::new());
    let symbol = aliases(o, &["symbol", "instId"])
        .filter(|v| truthy(Some(v)))
        .unwrap_or(&empty);
    let symbol = symbol.as_text().ok_or_else(|| {
        failure(
            "TypeError",
            "(o.symbol || o.instId || \"\").replace is not a function",
        )
    })?;
    let symbol = String::from_utf16(symbol).map_err(|_| {
        failure(
            "RepresentationError",
            "mapOrder cannot represent symbol in shared Order",
        )
    })?;
    let symbol = safe_cex_num::js_replace(
        &symbol,
        &format!("_{}", api_product_type.to_uppercase()),
        "",
    );
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
            .filter(|v| truthy(Some(v)))
            .unwrap_or(&zero),
    ))?;
    let amount = float(Some(raw_amount))?;
    let filled = float(Some(raw_filled))?;
    let last_update = now();
    let reduce_text = safe_cex_types::js_coercion::js_retained_string_utf16(
        o.get("reduceOnly").unwrap_or(&Input::Undefined),
    )
    .map_err(coercion)?;
    let reduce_only = ["YES", "yes", "true"]
        .iter()
        .any(|text| reduce_text.iter().copied().eq(text.encode_utf16()));
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
    let is = |key: &str, value: &str| o.get(key).is_some_and(|input| input.text_equals(value));
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
    if truthy(o.get("planType")) {
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
        && (finite(o.get("stopLoss"))? > 0.0 || finite(o.get("takeProfit"))? > 0.0)
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
            let price = finite(o.get(key))?;
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
            if truthy(price) {
                let mut leg = order.clone();
                leg.id = Some(Input::Text(
                    format!(
                        "{}{suffix}",
                        js_string(order.id.as_ref()).map_err(coercion)?
                    )
                    .encode_utf16()
                    .collect(),
                ));
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
    input: Option<&Input>,
    api_product_type: &str,
    mut market_is_inverse: impl FnMut(&str) -> Result<Option<bool>, ExchangeError>,
    mut now: impl FnMut() -> f64,
) -> Result<Vec<Order>, ExchangeError> {
    let o = input.filter(|v| !v.is_nullish()).ok_or_else(|| {
        failure(
            "TypeError",
            if input.is_none_or(|value| matches!(value, Input::Undefined)) {
                "Cannot read properties of undefined (reading 'symbol')"
            } else {
                "Cannot read properties of null (reading 'symbol')"
            },
        )
    })?;
    let raw_symbol = aliases(o, &["symbol", "instId"]);
    let symbol = match raw_symbol {
        Some(Input::Text(s)) => s,
        None | Some(Input::Undefined) => {
            return Err(failure(
                "TypeError",
                "Cannot read properties of undefined (reading 'replace')",
            ));
        }
        Some(Input::Null) => {
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
    let symbol = String::from_utf16(symbol).map_err(|_| {
        failure(
            "RepresentationError",
            "mapOrder cannot represent symbol in shared Order",
        )
    })?;
    let symbol = safe_cex_num::js_replace(
        &symbol,
        &format!("_{}", api_product_type.to_uppercase()),
        "",
    );
    let zero = Input::Text(vec![48]);
    let price = float(Some(
        aliases(o, &["price", "px"])
            .filter(|v| truthy(Some(v)))
            .unwrap_or(&zero),
    ))?;
    let raw_amount = aliases(o, &["size", "sz"])
        .filter(|v| truthy(Some(v)))
        .unwrap_or(&zero);
    let raw_filled = aliases(o, &["baseVolume", "filledQty", "accFillSz"])
        .filter(|v| truthy(Some(v)))
        .unwrap_or(&zero);
    let remaining_native =
        safe_cex_num::subtract(float(Some(raw_amount))?, float(Some(raw_filled))?);
    let market = market_is_inverse(&symbol)?;
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
        Some(Input::Text(s)) => s
            .iter()
            .copied()
            .map(|unit| {
                if (97..=122).contains(&unit) {
                    unit - 32
                } else {
                    unit
                }
            })
            .eq("YES".encode_utf16()),
        Some(value @ Input::Object(_))
            if value.get("toUpperCase").is_some_and(|v| !v.is_nullish()) =>
        {
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
    let is = |key: &str, value: &str| o.get(key).is_some_and(|input| input.text_equals(value));
    if truthy(o.get("planType")) {
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
            if truthy(price) {
                let mut leg = order.clone();
                leg.id = Some(Input::Text(
                    format!(
                        "{}{suffix}",
                        js_string(order.id.as_ref()).map_err(coercion)?
                    )
                    .encode_utf16()
                    .collect(),
                ));
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
