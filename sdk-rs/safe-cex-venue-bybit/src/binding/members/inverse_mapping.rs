//! Dedicated inverse-class mappers. USD contracts and native PnL follow the
//! standalone source class, rather than the opt-in inverse path in BybitExchange.
use crate::binding::{
    host::{get_key, text},
    rt::Rt,
    value::{Js, Thrown},
};

fn fallback(value: Js, other: Js) -> Js {
    if value.truthy() { value } else { other }
}
fn nullish(value: Js, other: Js) -> Js {
    if value.is_nullish() { other } else { value }
}
fn now(rt: &Rt) -> Result<Js, Thrown> {
    rt.global("virtualClock.getCurrentTimeMs")?
        .call(Js::Undefined, vec![])
}
fn table(rt: &Rt, name: &str, key: Js) -> Result<Js, Thrown> {
    rt.global(name)?.get_key(&key.to_js_string()?)
}

// ts: BybitInverseExchange.remainingInStoreUnits
pub(crate) fn remaining(native: f64, price: f64) -> Js {
    let value = if price > 0. {
        safe_cex_num::math_round((native / price) * 1e8) / 1e8
    } else {
        native
    };
    Js::object(vec![
        ("remaining", Js::Number(value)),
        (
            "extra",
            Js::object(vec![
                ("remainingNative", Js::Number(native)),
                ("remainingNotional", Js::Number(native)),
            ]),
        ),
    ])
}

// ts: BybitInverseExchange.mapPosition
pub(crate) fn map_position(rt: &Rt, p: &Js) -> Result<Js, Thrown> {
    let entry = fallback(
        get_key(p, "avgPrice")?,
        fallback(get_key(p, "entryPrice")?, Js::Number(0.)),
    )
    .parse_float()?;
    let native = nullish(p.get("size")?, Js::Number(0.)).parse_float()?;
    let mut contracts = native;
    let index = p.get("positionIdx")?;
    let side = if index.strict_equals(&Js::Number(1.)) {
        Js::str("long")
    } else if index.strict_equals(&Js::Number(2.)) {
        Js::str("short")
    } else {
        fallback(table(rt, "POSITION_SIDE", p.get("side")?)?, Js::str("long"))
    };
    let pnl = fallback(get_key(p, "unrealisedPnl")?, Js::Number(0.)).parse_float()?;
    let symbol = p.get("symbol")?;
    let store = rt.this.get("store")?;
    let market = store
        .get("marketsBySymbol")?
        .get_key(&symbol.to_js_string()?)?;
    let mut notional = fallback(get_key(p, "positionValue")?, Js::Number(0.)).parse_float()?;
    if !market.is_nullish() && market.get("isInverse")?.truthy() {
        let ticker = store
            .get("tickersBySymbol")?
            .get_key(&symbol.to_js_string()?)?;
        let (mark, last) = if ticker.is_nullish() {
            (Js::Undefined, Js::Undefined)
        } else {
            (ticker.get("mark")?, ticker.get("last")?)
        };
        let reference = fallback(mark, fallback(last, Js::Number(entry)));
        if reference.truthy() && reference.to_number()? > 0. {
            let reference = reference.to_number()?;
            contracts = safe_cex_num::math_round((native / reference) * 1e8) / 1e8;
            if native > 0. {
                rt.this.call_method("log", vec![Js::str(&format!(
                    "BybitInverse position: {} - USD contracts: {}, BTC price: {}, BTC amount: {}",
                    text(&symbol)?, safe_cex_num::js_number_string(native),
                    safe_cex_num::js_number_string(reference), safe_cex_num::js_number_string(contracts),
                ))])?;
            }
        }
        notional = native;
    }
    let mode = if index.is_undefined() {
        Js::Undefined
    } else if index.strict_equals(&Js::Number(0.)) {
        Js::str("oneway")
    } else {
        Js::str("hedged")
    };
    let margin_type = rt.this.get("marginType")?;
    let im = fallback(get_key(p, "positionIM")?, Js::Number(0.)).parse_float()?;
    // Use the source helper's live JS argument semantics, including an inherited
    // side-table referent. The typed facade owns its eventual output refusal.
    let id = rt.global("createPositionId")?.call(
        Js::Undefined,
        vec![symbol.clone(), side.clone(), mode.clone()],
    )?;
    Ok(Js::object(vec![
        ("id", id),
        ("symbol", symbol),
        ("side", side),
        ("entryPrice", Js::Number(entry)),
        ("notional", Js::Number(notional)),
        ("leverage", Js::Number(p.get("leverage")?.parse_float()?)),
        ("unrealizedPnl", Js::Number(pnl)),
        (
            "realizedPnl",
            Js::Number(fallback(get_key(p, "curRealisedPnl")?, Js::Number(0.)).parse_float()?),
        ),
        ("contracts", Js::Number(contracts)),
        (
            "liquidationPrice",
            Js::Number(fallback(get_key(p, "liqPrice")?, Js::Number(0.)).parse_float()?),
        ),
        (
            "marginSize",
            if im > 0. {
                Js::Number(im)
            } else {
                Js::Undefined
            },
        ),
        ("positionMode", mode),
        ("marginType", margin_type),
        (
            "extra",
            Js::object(vec![
                ("contractsNative", Js::Number(native)),
                ("unrealizedPnlNative", Js::Number(pnl)),
            ]),
        ),
        ("info", p.clone()),
        ("lastUpdate", now(rt)?),
    ]))
}

// ts: BybitInverseExchange.mapOrder
pub(crate) fn map_order(rt: &Rt, o: &Js) -> Result<Js, Thrown> {
    let stop = get_key(o, "stopOrderType")?;
    let flag = |key: &str| -> Result<bool, Thrown> {
        let v = get_key(o, key)?;
        Ok(v.strict_equals(&Js::Bool(true)) || v.strict_equals(&Js::str("true")))
    };
    let reduce = flag("reduceOnly")?;
    let close = flag("closeOnTrigger")?;
    let mode = get_key(o, "tpslMode")?;
    let position_tpsl = mode.is_string() && matches!(text(&mode)?.as_str(), "Full" | "Partial");
    let create_type = get_key(o, "createType")?;
    let create_order_type = if position_tpsl {
        table(
            rt,
            "POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE",
            create_type.clone(),
        )?
    } else {
        Js::Undefined
    };
    let is_stop =
        (stop.truthy() && !stop.strict_equals(&Js::str("UNKNOWN"))) || create_order_type.truthy();
    let trigger = get_key(o, "triggerPrice")?;
    let price = if is_stop {
        fallback(trigger.clone(), o.get("price")?)
    } else {
        o.get("price")?
    }
    .parse_float()?;
    let source_type = if create_order_type.truthy() {
        create_order_type
    } else if is_stop {
        stop
    } else {
        get_key(o, "orderType")?
    };
    let opening = is_stop
        && !reduce
        && !close
        && !position_tpsl
        && (nullish(trigger.clone(), Js::str("0")).parse_float()? > 0.
            || create_type.strict_equals(&Js::str("CreateByStopOrder")));
    let kind = if opening {
        Js::str("conditional")
    } else {
        table(rt, "ORDER_TYPE", source_type.clone())?
    };
    let time_in_force = table(rt, "ORDER_TIME_IN_FORCE", o.get("timeInForce")?)?;
    let leaves = o.get("leavesQty")?;
    let native = if leaves.truthy() {
        leaves.parse_float()?
    } else {
        safe_cex_num::subtract(
            nullish(o.get("qty")?, Js::Number(0.)).to_number()?,
            nullish(get_key(o, "cumExecQty")?, Js::Number(0.)).to_number()?,
        )
    };
    let side = table(rt, "ORDER_SIDE", o.get("side")?)?;
    let units = rt.this.call_method(
        "remainingInStoreUnits",
        vec![Js::Number(native), Js::Number(price)],
    )?;
    let order = Js::object(vec![
        ("id", o.get("orderId")?),
        (
            "status",
            table(rt, "ORDER_STATUS", get_key(o, "orderStatus")?)?,
        ),
        ("symbol", o.get("symbol")?),
        ("type", kind.clone()),
        ("side", side.clone()),
    ]);
    for (key, value) in units.own_entries()? {
        order.set_key(&key, value)?;
    }
    for (key, value) in [
        ("price", Js::Number(price)),
        (
            "amount",
            Js::Number(
                if (source_type.strict_equals(&Js::str("StopLoss"))
                    || source_type.strict_equals(&Js::str("TakeProfit")))
                    && mode.strict_equals(&Js::str("Full"))
                {
                    0.
                } else {
                    nullish(o.get("qty")?, Js::Number(0.)).parse_float()?
                },
            ),
        ),
        (
            "filled",
            Js::Number(nullish(get_key(o, "cumExecQty")?, Js::Number(0.)).parse_float()?),
        ),
        ("reduceOnly", Js::Bool(reduce)),
        ("close", Js::Bool(close)),
        ("info", o.clone()),
        ("timeInForce", time_in_force),
        ("lastUpdate", now(rt)?),
    ] {
        order.set(key, value)?;
    }
    for (value, field) in [
        ("conditional", "triggerType"),
        ("take_profit_market", "tpTriggerType"),
        ("stop_market", "slTriggerType"),
    ] {
        if kind.strict_equals(&Js::str(value)) {
            order.set(
                field,
                table(rt, "ORDER_TRIGGER_TYPE", get_key(o, "triggerBy")?)?,
            )?;
        }
    }
    let orders = Js::array(vec![order.clone()]);
    let inverse_side = if side.strict_equals(&Js::str("buy")) {
        Js::str("sell")
    } else {
        Js::str("buy")
    };
    for (source, suffix, kind, trigger, field) in [
        (
            "stopLoss",
            "stop_loss",
            "stop_market",
            "slTriggerBy",
            "slTriggerType",
        ),
        (
            "takeProfit",
            "take_profit",
            "take_profit_market",
            "tpTriggerBy",
            "tpTriggerType",
        ),
    ] {
        let price = get_key(o, source)?.parse_float()?;
        if price.partial_cmp(&0.) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        let row = order.spread_object()?;
        row.set(
            "id",
            Js::str(&format!("{}__{suffix}", text(&o.get("orderId")?)?)),
        )?;
        row.set("type", Js::str(kind))?;
        row.set("side", inverse_side.clone())?;
        row.set("price", Js::Number(price))?;
        row.set("filled", Js::Number(0.))?;
        let units = rt.this.call_method(
            "remainingInStoreUnits",
            vec![Js::Number(native), Js::Number(price)],
        )?;
        for (key, value) in units.own_entries()? {
            row.set_key(&key, value)?;
        }
        row.set("lastUpdate", now(rt)?)?;
        row.set(
            field,
            table(rt, "ORDER_TRIGGER_TYPE", get_key(o, trigger)?)?,
        )?;
        orders.push(row)?;
    }
    Ok(orders)
}
