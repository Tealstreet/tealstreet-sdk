//! BybitExchange.mapOrder / mapPosition over JS rows: field order, spreads and the
//! virtual TP/SL rows follow the source object literals.
use crate::binding::{
    host::{get_key, text},
    rt::Rt,
    value::{Js, Thrown, from_json},
};
use serde_json::Value;

/// `TABLE[key]` over a bybit.types.ts lookup object: own keys only.
pub(crate) fn table(table: &Value, key: &Js) -> Result<Js, Thrown> {
    if key.is_undefined() {
        return Ok(Js::Undefined);
    }
    Ok(table
        .get(text(key)?)
        .map(from_json)
        .unwrap_or(Js::Undefined))
}

fn parse_float(v: &Js) -> Result<f64, Thrown> {
    v.parse_float()
}

fn or_zero(v: Js) -> Js {
    if v.truthy() { v } else { Js::Number(0.) }
}

fn nullish_or(v: Js, fallback: Js) -> Js {
    if v.is_nullish() { fallback } else { v }
}

fn flag(o: &Js, key: &str) -> Result<bool, Thrown> {
    let v = get_key(o, key)?;
    Ok(v.strict_equals(&Js::Bool(true)) || v.strict_equals(&Js::str("true")))
}

/// `{ remaining }` or, on an inverse row priced above zero, `{ remaining, extra }`.
pub(crate) fn remaining_in_store_units(
    symbol: &Js,
    remaining_native: f64,
    price: f64,
) -> Result<Vec<(&'static str, Js)>, Thrown> {
    if !crate::inverse_symbol::is_bybit_inverse_symbol(&text(symbol)?)
        || price.is_nan()
        || price <= 0.
    {
        return Ok(vec![("remaining", Js::Number(remaining_native))]);
    }
    let coin = safe_cex_num::math_round((remaining_native / price) * 1e8) / 1e8;
    Ok(vec![
        ("remaining", Js::Number(coin)),
        (
            "extra",
            Js::object(vec![("remainingNative", Js::Number(remaining_native))]),
        ),
    ])
}

fn now(rt: &Rt) -> Result<Js, Thrown> {
    rt.global("virtualClock.getCurrentTimeMs")?
        .call(Js::Undefined, vec![])
}

// ts: BybitExchange.mapOrder
pub(crate) fn map_order(rt: &Rt, this: &Js, o: &Js) -> Result<Js, Thrown> {
    let types = &crate::types::ORDER_TYPE;
    let stop_order_type = get_key(o, "stopOrderType")?;
    let reduce_only = flag(o, "reduceOnly")?;
    let close_on_trigger = flag(o, "closeOnTrigger")?;
    let tpsl_mode = get_key(o, "tpslMode")?;
    // ts: isBybitPositionTpslMode — a string in BYBIT_POSITION_TPSL_MODES.
    let position_tpsl =
        tpsl_mode.is_string() && matches!(text(&tpsl_mode)?.as_str(), "Full" | "Partial");
    let create_type = get_key(o, "createType")?;
    let create_type_order_type = if position_tpsl {
        table(
            &crate::types::POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE,
            &create_type,
        )?
    } else {
        Js::Undefined
    };
    let is_stop = (stop_order_type.truthy() && !stop_order_type.strict_equals(&Js::str("UNKNOWN")))
        || create_type_order_type.truthy();
    let trigger_price = get_key(o, "triggerPrice")?;
    let o_price = if is_stop {
        if trigger_price.truthy() {
            trigger_price.clone()
        } else {
            o.get("price")?
        }
    } else {
        o.get("price")?
    };
    let o_type = if create_type_order_type.truthy() {
        create_type_order_type.clone()
    } else if is_stop {
        stop_order_type.clone()
    } else {
        get_key(o, "orderType")?
    };
    let opening_trigger = is_stop
        && !reduce_only
        && !close_on_trigger
        && !position_tpsl
        && (nullish_or(trigger_price.clone(), Js::str("0")).parse_float()? > 0.
            || create_type.strict_equals(&Js::str("CreateByStopOrder")));
    let kind = if opening_trigger {
        Js::str("conditional")
    } else {
        table(types, &o_type)?
    };
    let kind_is = |name: &str| kind.strict_equals(&Js::str(name));
    let protective = reduce_only
        || close_on_trigger
        || position_tpsl
        || kind_is("stop_market")
        || kind_is("take_profit_market")
        || kind_is("trailing_stop_market");
    let mapped_side = table(&crate::types::ORDER_SIDE, &o.get("side")?)?;
    let side = if !mapped_side.is_nullish() {
        mapped_side
    } else if protective {
        this.call_method("inferProtectiveOrderSide", vec![o.clone()])?
    } else {
        Js::Undefined
    };
    let time_in_force = table(&crate::types::ORDER_TIME_IN_FORCE, &o.get("timeInForce")?)?;
    let leaves = o.get("leavesQty")?;
    let remaining_native = if leaves.truthy() {
        parse_float(&leaves)?
    } else {
        let qty = nullish_or(o.get("qty")?, Js::Number(0.)).to_number()?;
        let cum = nullish_or(get_key(o, "cumExecQty")?, Js::Number(0.)).to_number()?;
        safe_cex_num::subtract(qty, cum)
    };
    let price = parse_float(&o_price)?;
    let symbol = o.get("symbol")?;
    let inverse = crate::inverse_symbol::is_bybit_inverse_symbol(&text(&symbol)?);
    let amount = if (o_type.strict_equals(&Js::str("StopLoss"))
        || o_type.strict_equals(&Js::str("TakeProfit")))
        && tpsl_mode.strict_equals(&Js::str("Full"))
    {
        0.
    } else {
        parse_float(&nullish_or(o.get("qty")?, Js::Number(0.)))?
    };
    let trigger_by = || table(&crate::types::ORDER_TRIGGER_TYPE, &get_key(o, "triggerBy")?);

    let mut fields: Vec<(&'static str, Js)> = vec![
        ("id", o.get("orderId")?),
        (
            "status",
            table(&crate::types::ORDER_STATUS, &get_key(o, "orderStatus")?)?,
        ),
        ("symbol", symbol.clone()),
        ("type", kind.clone()),
        ("side", side.clone()),
    ];
    fields.extend(remaining_in_store_units(&symbol, remaining_native, price)?);
    // `extra: { remainingNative }` replaces any extra the spread produced, in place.
    let extra = Js::object(vec![("remainingNative", Js::Number(remaining_native))]);
    match fields.iter_mut().find(|(k, _)| *k == "extra") {
        Some(slot) => slot.1 = extra,
        None => fields.push(("extra", extra)),
    }
    fields.extend([
        ("price", Js::Number(price)),
        ("amount", Js::Number(amount)),
        (
            "filled",
            Js::Number(parse_float(&nullish_or(
                get_key(o, "cumExecQty")?,
                Js::Number(0.),
            ))?),
        ),
        ("reduceOnly", Js::Bool(reduce_only)),
        ("close", Js::Bool(close_on_trigger)),
        ("info", o.clone()),
        ("timeInForce", time_in_force),
        ("lastUpdate", now(rt)?),
    ]);
    if kind_is("conditional") {
        fields.push(("triggerType", trigger_by()?));
    }
    if kind_is("take_profit_market") {
        fields.push(("tpTriggerType", trigger_by()?));
    }
    if kind_is("stop_market") {
        fields.push(("slTriggerType", trigger_by()?));
    }
    let first = Js::object(fields);
    let orders = vec![first.clone()];
    let orders = Js::array(orders);

    let sl = parse_float(&get_key(o, "stopLoss")?)?;
    let tp = parse_float(&get_key(o, "takeProfit")?)?;
    let inverse_side = if first.get("side")?.strict_equals(&Js::str("buy")) {
        Js::str("sell")
    } else {
        Js::str("buy")
    };
    let order_id = text(&o.get("orderId")?)?;
    for (value, suffix, kind, trigger_key, trigger_field) in [
        (
            sl,
            "stop_loss",
            "stop_market",
            "slTriggerBy",
            "slTriggerType",
        ),
        (
            tp,
            "take_profit",
            "take_profit_market",
            "tpTriggerBy",
            "tpTriggerType",
        ),
    ] {
        if value.partial_cmp(&0.) != Some(std::cmp::Ordering::Greater) {
            continue;
        }
        let row = first.spread_object()?;
        row.set("id", Js::str(&format!("{order_id}__{suffix}")))?;
        row.set("type", Js::str(kind))?;
        row.set("side", inverse_side.clone())?;
        row.set("price", Js::Number(value))?;
        row.set("filled", Js::Number(0.))?;
        let units = if inverse {
            remaining_in_store_units(&symbol, remaining_native, value)?
        } else {
            vec![("remaining", first.get("amount")?)]
        };
        for (k, v) in units {
            row.set(k, v)?;
        }
        row.set("lastUpdate", now(rt)?)?;
        row.set(
            trigger_field,
            table(&crate::types::ORDER_TRIGGER_TYPE, &get_key(o, trigger_key)?)?,
        )?;
        orders.push(row)?;
    }
    Ok(orders)
}

// ts: BybitExchange.mapPosition
pub(crate) fn map_position(rt: &Rt, this: &Js, p: &Js) -> Result<Js, Thrown> {
    let avg = get_key(p, "avgPrice")?;
    let entry = if avg.truthy() {
        avg
    } else {
        or_zero(get_key(p, "entryPrice")?)
    };
    let entry_price = parse_float(&entry)?;
    let mut contracts = parse_float(&nullish_or(p.get("size")?, Js::Number(0.)))?;
    let index = p.get("positionIdx")?;
    let side = if index.strict_equals(&Js::Number(1.)) {
        Js::str("long")
    } else if index.strict_equals(&Js::Number(2.)) {
        Js::str("short")
    } else {
        let mapped = table(&crate::types::POSITION_SIDE, &p.get("side")?)?;
        if mapped.truthy() {
            mapped
        } else {
            Js::str("long")
        }
    };
    let mut unrealized = parse_float(&or_zero(get_key(p, "unrealisedPnl")?))?;
    let symbol = p.get("symbol")?;
    let symbol_text = text(&symbol)?;
    let store = this.get("store")?;
    let market = store.get("marketsBySymbol")?.get(&symbol_text)?;
    let contracts_native = contracts;
    let mut notional = parse_float(&or_zero(get_key(p, "positionValue")?))?;
    let inverse = crate::inverse_symbol::is_bybit_inverse_symbol(&symbol_text);
    if inverse {
        let ticker = store.get("tickersBySymbol")?.get(&symbol_text)?;
        let mark = if ticker.is_nullish() {
            Js::Undefined
        } else {
            ticker.get("mark")?
        };
        let reference = if mark.truthy() {
            mark.to_number()?
        } else {
            let from_row = parse_float(&or_zero(get_key(p, "markPrice")?))?;
            if from_row != 0. && !from_row.is_nan() {
                from_row
            } else {
                let last = if ticker.is_nullish() {
                    Js::Undefined
                } else {
                    ticker.get("last")?
                };
                if last.truthy() {
                    last.to_number()?
                } else {
                    entry_price
                }
            }
        };
        if reference > 0. {
            contracts = safe_cex_num::math_round((contracts_native / reference) * 1e8) / 1e8;
        }
        notional = contracts_native;
    } else if !symbol_text.ends_with("USDT") {
        let ticker = store.get("tickersBySymbol")?.get(&symbol_text)?;
        if !market.is_nullish()
            && market.get("quote")?.strict_equals(&Js::str("USDC"))
            && ticker.truthy()
        {
            let direction = if side.strict_equals(&Js::str("long")) {
                1.
            } else {
                -1.
            };
            let mark = ticker.get("mark")?.to_number()?;
            unrealized = direction * (mark * contracts - entry_price * contracts);
        }
    }
    let position_mode = if index.is_undefined() {
        Js::Undefined
    } else if index.strict_equals(&Js::Number(0.)) {
        Js::str("oneway")
    } else {
        Js::str("hedged")
    };
    let margin_type = this.get("marginType")?;
    let im = parse_float(&or_zero(get_key(p, "positionIM")?))?;
    let typed_side = match text(&side)?.as_str() {
        "long" => Some(safe_cex_types::PositionSide::Long),
        _ => Some(safe_cex_types::PositionSide::Short),
    };
    let typed_mode = if position_mode.is_undefined() {
        None
    } else if position_mode.strict_equals(&Js::str("oneway")) {
        Some(safe_cex_types::PositionMode::OneWay)
    } else {
        Some(safe_cex_types::PositionMode::Hedged)
    };
    let mut extra = vec![("contractsNative", Js::Number(contracts_native))];
    if inverse {
        extra.push(("unrealizedPnlNative", Js::Number(unrealized)));
    }
    let position = Js::object(vec![
        (
            "id",
            Js::str(&safe_cex_types::model_utils::create_position_id(
                &symbol_text,
                typed_side,
                typed_mode,
            )),
        ),
        ("symbol", symbol.clone()),
        ("side", side),
        ("entryPrice", Js::Number(entry_price)),
        ("notional", Js::Number(notional)),
        ("leverage", Js::Number(parse_float(&p.get("leverage")?)?)),
        ("unrealizedPnl", Js::Number(unrealized)),
        (
            "realizedPnl",
            Js::Number(parse_float(&or_zero(get_key(p, "curRealisedPnl")?))?),
        ),
        ("contracts", Js::Number(contracts)),
        (
            "liquidationPrice",
            Js::Number(parse_float(&or_zero(get_key(p, "liqPrice")?))?),
        ),
        (
            "marginSize",
            if im > 0. {
                Js::Number(im)
            } else {
                Js::Undefined
            },
        ),
        ("positionMode", position_mode),
        ("marginType", margin_type),
        ("extra", Js::object(extra)),
        ("info", p.clone()),
        ("lastUpdate", now(rt)?),
    ]);
    if crate::options::is_bybit_option_symbol(&symbol_text) {
        let greeks = super::options::map_option_greeks(p)?;
        let out = position.spread_object()?;
        out.set("leverage", Js::Number(1.))?;
        out.set("liquidationPrice", Js::Number(0.))?;
        out.set(
            "option",
            if greeks.truthy() {
                Js::object(vec![("greeks", greeks)])
            } else {
                Js::object(vec![])
            },
        )?;
        return Ok(out);
    }
    Ok(position)
}
