//! Live order mapper outputs stay in the runtime value domain. The separate
//! typed mapper is a closed-model facade and must not gate execution reports.
use crate::binding::{
    rt::Rt,
    value::{Js, Thrown},
};

// ts: WOOXExchange.mapLimitOrder
pub(crate) fn map_limit_order(rt: &Rt, row: Js) -> Result<Js, Thrown> {
    let v = |key: &str| {
        rt.global("v")?
            .call(Js::Undefined, vec![row.clone(), Js::str(key)])
    };
    let order_id = v("order_id")?;
    let order_id = if order_id.truthy() {
        order_id
    } else {
        v("orderId")?
    };
    let order_id = if order_id.truthy() {
        order_id
    } else {
        v("id")?
    };
    let raw_symbol = v("symbol")?;
    if !order_id.truthy() || !raw_symbol.truthy() {
        return Ok(Js::Null);
    }
    let symbol = rt
        .global("normalizeSymbol")?
        .call(Js::Undefined, vec![raw_symbol])?;
    let market = rt
        .this
        .get("store")?
        .get("marketsBySymbol")?
        .get_key(&symbol.to_js_string()?)?;
    if !market.truthy() {
        return Ok(Js::Null);
    }
    let quantity = row.get("quantity")?;
    let quantity = if quantity.truthy() {
        quantity
    } else {
        row.get("order_quantity")?
    };
    let executed = row.get("executed")?;
    let executed = if executed.truthy() {
        executed
    } else {
        Js::Number(0.0)
    };
    let time_in_force = if row.get("type")?.strict_equals(&Js::str("POST_ONLY")) {
        "PostOnly"
    } else {
        "GoodTillCancel"
    };
    let status = if row.get("status")?.strict_equals(&Js::str("CANCELLED")) {
        "canceled"
    } else {
        "open"
    };
    let id = Js::utf16(&order_id.to_js_string()?);
    let kind = row.get("type")?;
    let kind = if kind.truthy() {
        kind
    } else {
        row.get("order_type")?
    };
    let kind = rt.global("ORDER_TYPE")?.get_key(&kind.to_js_string()?)?;
    let side = row.get("side")?;
    let side = rt.global("ORDER_SIDE")?.get_key(&side.to_js_string()?)?;
    let price = row.get("price")?;
    let price = if price.truthy() {
        price
    } else {
        row.get("order_price")?
    };
    let reduce_only = v("reduce_only")?;
    let remaining = rt
        .global("subtract")?
        .call(Js::Undefined, vec![quantity.clone(), executed.clone()])?;
    Ok(Js::object(vec![
        ("id", id),
        ("status", Js::str(status)),
        ("symbol", symbol),
        ("type", kind),
        ("side", side),
        ("price", price),
        ("amount", quantity),
        ("reduceOnly", reduce_only),
        ("timeInForce", Js::str(time_in_force)),
        ("filled", executed),
        ("remaining", remaining),
        ("lastUpdate", Js::Number(rt.now())),
        ("info", row),
    ]))
}
