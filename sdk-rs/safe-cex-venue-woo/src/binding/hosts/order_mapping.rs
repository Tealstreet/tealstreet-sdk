//! Live order mapper outputs preserve the source runtime value domain, including
//! undefined fields, copied values and original order references.
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

// ts: WOOXExchange.mapAlgoOrder
pub(crate) fn map_algo_order(rt: &Rt, row: Js) -> Result<Js, Thrown> {
    let symbol = rt
        .global("normalizeSymbol")?
        .call(Js::Undefined, vec![row.get("symbol")?])?;
    let market = rt
        .this
        .get("store")?
        .get("marketsBySymbol")?
        .get_key(&symbol.to_js_string()?)?;
    if !market.truthy() {
        return Ok(Js::array(Vec::new()));
    }
    let children = row.get("childOrders")?;
    let children = if children.truthy() {
        children
    } else {
        Js::array(vec![row.clone()])
    };
    let filter = children.get("filter")?;
    if !filter.is_callable() {
        return Err(Thrown::type_error("arr.filter is not a function"));
    }
    let filter_rt = rt.clone();
    let filter_child = Js::function("", move |_, args| {
        let child = super::super::value::arg(&args, 0);
        if !child.is_nullish() && !child.is_object() {
            return Err(Thrown::type_error(&format!(
                "Cannot use 'in' operator to search for 'triggerPrice' in {}",
                child.to_display()
            )));
        }
        let v = |key: &str| {
            filter_rt
                .global("v")?
                .call(Js::Undefined, vec![child.clone(), Js::str(key)])
        };
        let trigger_price = v("triggerPrice")?;
        Ok(if trigger_price.truthy() {
            trigger_price
        } else {
            Js::Bool(v("algoType")?.strict_equals(&Js::str("TRAILING_STOP")))
        })
    });
    // The complete filter runs before any mapper, coercion or clock call.
    let filtered = filter.call(children, vec![filter_child])?;
    let map = filtered.get("map")?;
    if !map.is_callable() {
        return Err(Thrown::type_error("arr.filter(...).map is not a function"));
    }
    let rt = rt.clone();
    let map_child = Js::function("", move |_, args| {
        let child = super::super::value::arg(&args, 0);
        let v = |key: &str| {
            rt.global("v")?
                .call(Js::Undefined, vec![child.clone(), Js::str(key)])
        };
        let filled = v("totalExecutedQuantity")?;
        let price = v("triggerPrice")?;
        let price = if price.truthy() {
            price
        } else {
            rt.global("subtract")?
                .call(Js::Undefined, vec![v("extremePrice")?, v("callbackValue")?])?
        };
        let kind = rt
            .global("ORDER_TYPE")?
            .get_key(&v("algoType")?.to_js_string()?)?;
        let id = Js::utf16(&v("algoOrderId")?.to_js_string()?);
        let parent_id = rt
            .global("v")?
            .call(Js::Undefined, vec![row.clone(), Js::str("rootAlgoOrderId")])?;
        let parent_id = if parent_id.truthy() {
            parent_id
        } else {
            v("rootAlgoOrderId")?
        };
        let parent_id = Js::utf16(&parent_id.to_js_string()?);
        let trigger = rt
            .global("ORDER_TRIGGER_TYPE")?
            .get_key(&v("triggerPriceType")?.to_js_string()?)?;
        let stop = kind.strict_equals(&Js::str("trailing_stop_market"))
            || kind.strict_equals(&Js::str("stop_market"));
        let side = rt
            .global("ORDER_SIDE")?
            .get_key(&child.get("side")?.to_js_string()?)?;
        let quantity = child.get("quantity")?;
        let reduce_only = v("reduceOnly")?;
        let remaining = rt
            .global("subtract")?
            .call(Js::Undefined, vec![child.get("quantity")?, filled.clone()])?;
        Ok(Js::object(vec![
            ("id", id),
            ("parentId", parent_id),
            ("status", Js::str("open")),
            ("symbol", symbol.clone()),
            ("type", kind),
            (
                if stop {
                    "slTriggerType"
                } else {
                    "tpTriggerType"
                },
                trigger,
            ),
            ("side", side),
            ("price", price),
            ("amount", quantity),
            ("reduceOnly", reduce_only),
            ("filled", filled),
            ("remaining", remaining),
            ("lastUpdate", Js::Number(rt.now())),
            ("info", row.clone()),
        ]))
    });
    map.call(filtered, vec![map_child])
}
