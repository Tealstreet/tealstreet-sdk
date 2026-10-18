//! binanceinverse.exchange.ts order formatting and write methods.
use super::{
    Rt,
    ops::{
        contains, emit, enum_value, error_message, global, install_async, install_sync, optional,
        or, request, store, text,
    },
};
use safe_cex_exchange::js_runtime::{
    promise::{JsFuture, await_js},
    value::{Js, Thrown, arg},
};

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    for (name, body) in [
        (
            "formatCreateOrder",
            format_order as fn(&Rt, &[Js]) -> Result<Js, Thrown>,
        ),
        ("getOrderPositionSide", position_side),
        ("trailingCallbackRate", trailing_rate),
        ("createAttachedStopPayload", attached_stop),
        ("splitPayloadByMaxSize", split),
        ("applyConfirmedPositionMode", apply_mode),
        ("afterMethodCalledInWorker", after_worker),
    ] {
        install_sync(rt, name, body)?;
    }
    for (name, body) in [
        (
            "placeOrder",
            place_order as fn(Rt, Vec<Js>) -> JsFuture<'static>,
        ),
        ("placeOrders", place_orders),
        ("placeOrderPayloads", place_payloads),
        ("cancelOrders", cancel_orders),
        ("cancelSymbolOrders", cancel_symbol),
        ("updateOrder", update_order),
        ("changePositionMode", change_mode),
        ("changeMarginType", change_margin),
        ("setLeverage", set_leverage),
    ] {
        install_async(rt, name, body)?;
    }
    Ok(())
}
fn equal(rt: &Rt, opts: &Js, key: &str, family: &str, value: &str) -> Result<bool, Thrown> {
    Ok(opts
        .get(key)?
        .strict_equals(&enum_value(rt, family, value)?))
}
fn inverse(rt: &Rt, family: &str, key: Js) -> Result<Js, Thrown> {
    global(rt, "inverseObj", vec![rt.global(family)?])?.get_property(&key)
}
fn client_id(rt: &Rt) -> Result<Js, Thrown> {
    safe_cex_exchange::js_runtime::ops::add(&Js::str("x-s3vFFRQp"), &global(rt, "uuid", vec![])?)
}
fn string(value: Js) -> Result<Js, Thrown> {
    Ok(Js::utf16(&value.to_js_string()?))
}
fn format_order(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let opts = arg(a, 0);
    let market = store(rt, "marketsBySymbol")?.get_property(&opts.get("symbol")?)?;
    if !market.truthy() {
        return Err(Thrown::error(
            "Error",
            &format!("Market {} not found", text(&opts.get("symbol")?)?),
        ));
    }
    let side = rt.call_path("getOrderPositionSide", vec![opts.clone()])?;
    let precision = market.get("precision")?;
    let p_price = precision.get("price")?;
    let p_amount = precision.get("amount")?;
    let amount = global(rt, "adjust", vec![opts.get("amount")?, p_amount])?;
    let price = if opts.get("price")?.truthy() && !equal(rt, &opts, "type", "OrderType", "Market")?
    {
        global(rt, "adjust", vec![opts.get("price")?, p_price])?
    } else {
        Js::Undefined
    };
    let reduce_only = if !store(rt, "options")?.get("isHedged")?.truthy() {
        opts.get("reduceOnly")?
    } else {
        Js::Bool(false)
    };
    let time_in_force = if opts.get("timeInForce")?.truthy() {
        inverse(rt, "TIME_IN_FORCE", opts.get("timeInForce")?)?
    } else {
        inverse(
            rt,
            "TIME_IN_FORCE",
            enum_value(rt, "OrderTimeInForce", "GoodTillCancel")?,
        )?
    };
    let stop = equal(rt, &opts, "type", "OrderType", "StopLoss")?
        || equal(rt, &opts, "type", "OrderType", "TakeProfit")?;
    let wants_close = stop && (opts.get("close")?.truthy() || !amount.truthy());
    let opposite = enum_value(
        rt,
        "PositionSide",
        if equal(rt, &opts, "side", "OrderSide", "Buy")? {
            "Short"
        } else {
            "Long"
        },
    )?;
    let mut existing = Js::Undefined;
    for p in store(rt, "positions")?.iterable_values()? {
        if p.get("symbol")?.strict_equals(&opts.get("symbol")?)
            && p.get("side")?.strict_equals(&opposite)
            && p.get("contracts")?.to_number()?.abs() > 0.0
        {
            existing = p;
            break;
        }
    }
    if wants_close && !existing.truthy() && !amount.truthy() {
        let message = "Cannot place full stop order without existing position or specified amount";
        rt.call_path("emitAndLogError", vec![Js::str(message)])?;
        return Err(Thrown::error("Error", message));
    }
    let full_close = wants_close && existing.truthy();
    let limit = equal(rt, &opts, "type", "OrderType", "Limit")?;
    let working = if equal(rt, &opts, "type", "OrderType", "TakeProfit")? {
        if equal(
            rt,
            &opts,
            "tpTriggerType",
            "OrderStopTriggerType",
            "MarkPrice",
        )? {
            "MARK_PRICE"
        } else {
            "CONTRACT_PRICE"
        }
    } else if equal(
        rt,
        &opts,
        "slTriggerType",
        "OrderStopTriggerType",
        "LastPrice",
    )? {
        "CONTRACT_PRICE"
    } else {
        "MARK_PRICE"
    };
    let trailing =
        equal(rt, &opts, "type", "OrderType", "TrailingStopLoss")? && opts.get("price")?.truthy();
    let req = global(
        rt,
        "omitUndefined",
        vec![Js::object(vec![
            ("symbol", opts.get("symbol")?),
            ("positionSide", side),
            ("side", inverse(rt, "ORDER_SIDE", opts.get("side")?)?),
            ("type", inverse(rt, "ORDER_TYPE", opts.get("type")?)?),
            (
                "quantity",
                if full_close || !amount.truthy() {
                    Js::Undefined
                } else {
                    string(amount.clone())?
                },
            ),
            (
                "price",
                if limit {
                    string(price.clone())?
                } else {
                    Js::Undefined
                },
            ),
            (
                "stopPrice",
                if stop && price.truthy() {
                    string(price.clone())?
                } else {
                    Js::Undefined
                },
            ),
            (
                "closePosition",
                if full_close {
                    Js::str("true")
                } else {
                    Js::Undefined
                },
            ),
            (
                "reduceOnly",
                if !full_close && reduce_only.truthy() {
                    Js::str("true")
                } else {
                    Js::Undefined
                },
            ),
            ("workingType", Js::str(working)),
            (
                "timeInForce",
                if limit { time_in_force } else { Js::Undefined },
            ),
            (
                "callbackRate",
                if trailing {
                    string(rt.call_path("trailingCallbackRate", vec![opts.clone()])?)?
                } else {
                    Js::Undefined
                },
            ),
            (
                "activationPrice",
                if trailing {
                    string(price)?
                } else {
                    Js::Undefined
                },
            ),
            ("newClientOrderId", client_id(rt)?),
        ])],
    )?;
    let payloads = rt.call_path("splitPayloadByMaxSize", vec![req, amount, market])?;
    for (key, kind) in [("stopLoss", "StopLoss"), ("takeProfit", "TakeProfit")] {
        if opts.get(key)?.truthy() {
            let payload = rt.call_path(
                "createAttachedStopPayload",
                vec![
                    opts.clone(),
                    enum_value(rt, "OrderType", kind)?,
                    opts.get(key)?,
                ],
            )?;
            payloads.call_method("push", vec![payload])?;
        }
    }
    Ok(payloads)
}
fn position_side(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let opts = arg(a, 0);
    if !store(rt, "options")?.get("isHedged")?.truthy() {
        return Ok(Js::str("BOTH"));
    }
    let mut long = equal(rt, &opts, "side", "OrderSide", "Buy")?;
    if equal(rt, &opts, "type", "OrderType", "StopLoss")?
        || equal(rt, &opts, "type", "OrderType", "TakeProfit")?
        || equal(rt, &opts, "type", "OrderType", "TrailingStopLoss")?
        || opts.get("reduceOnly")?.truthy()
    {
        long = !long;
    }
    Ok(Js::str(if long { "LONG" } else { "SHORT" }))
}
fn trailing_rate(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let opts = arg(a, 0);
    let ticker = store(rt, "tickersBySymbol")?.get_property(&opts.get("symbol")?)?;
    if !ticker.truthy() || !opts.get("price")?.truthy() {
        return Ok(Js::Undefined);
    }
    let high = global(
        rt,
        "Math.max",
        vec![ticker.get("last")?, opts.get("price")?],
    )?
    .to_number()?;
    let low = global(
        rt,
        "Math.min",
        vec![ticker.get("last")?, opts.get("price")?],
    )?
    .to_number()?;
    let rounded = global(
        rt,
        "Math.round",
        vec![Js::Number(
            ((high - low) * 100.0 / ticker.get("last")?.to_number()?) * 10.0,
        )],
    )?
    .to_number()?;
    let clamped = global(
        rt,
        "Math.min",
        vec![Js::Number(10.0), Js::Number(rounded / 10.0)],
    )?;
    global(rt, "Math.max", vec![Js::Number(0.1), clamped])
}
fn attached_stop(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let opts = arg(a, 0);
    let kind = arg(a, 1);
    let trigger = arg(a, 2);
    let market = store(rt, "marketsBySymbol")?.get_property(&opts.get("symbol")?)?;
    let p_price = market.get("precision")?.get("price")?;
    let amount = global(
        rt,
        "adjust",
        vec![opts.get("amount")?, market.get("precision")?.get("amount")?],
    )?;
    let side = enum_value(
        rt,
        "OrderSide",
        if equal(rt, &opts, "side", "OrderSide", "Buy")? {
            "Sell"
        } else {
            "Buy"
        },
    )?;
    let next = opts.spread_object()?;
    next.set("side", side.clone())?;
    next.set("type", kind.clone())?;
    next.set("reduceOnly", Js::Bool(true))?;
    let position_side = rt.call_path("getOrderPositionSide", vec![next])?;
    let working = if kind.strict_equals(&enum_value(rt, "OrderType", "StopLoss")?) {
        if equal(
            rt,
            &opts,
            "slTriggerType",
            "OrderStopTriggerType",
            "LastPrice",
        )? {
            "CONTRACT_PRICE"
        } else {
            "MARK_PRICE"
        }
    } else if equal(
        rt,
        &opts,
        "tpTriggerType",
        "OrderStopTriggerType",
        "MarkPrice",
    )? {
        "MARK_PRICE"
    } else {
        "CONTRACT_PRICE"
    };
    global(
        rt,
        "omitUndefined",
        vec![Js::object(vec![
            ("symbol", opts.get("symbol")?),
            ("positionSide", position_side),
            ("side", inverse(rt, "ORDER_SIDE", side)?),
            ("type", inverse(rt, "ORDER_TYPE", kind)?),
            (
                "quantity",
                if amount.truthy() {
                    string(amount)?
                } else {
                    Js::Undefined
                },
            ),
            (
                "stopPrice",
                string(global(rt, "adjust", vec![trigger, p_price])?)?,
            ),
            (
                "reduceOnly",
                if store(rt, "options")?.get("isHedged")?.truthy() {
                    Js::Undefined
                } else {
                    Js::str("true")
                },
            ),
            ("workingType", Js::str(working)),
            ("newClientOrderId", client_id(rt)?),
        ])],
    )
}
fn split(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let req = arg(a, 0);
    let amount = arg(a, 1);
    let market = arg(a, 2);
    if !amount.truthy() || req.get("closePosition")?.truthy() {
        return Ok(Js::array(vec![req]));
    }
    let max = market.get("limits")?.get("amount")?.get("max")?;
    if !matches!(max, Js::Number(n) if n.is_finite()) || amount.to_number()? <= max.to_number()? {
        return Ok(Js::array(vec![req]));
    }
    let lots = global(
        rt,
        "Math.ceil",
        vec![Js::Number(amount.to_number()? / max.to_number()?)],
    )?;
    let rest = global(
        rt,
        "adjust",
        vec![
            Js::Number(amount.to_number()? % max.to_number()?),
            market.get("precision")?.get("amount")?,
        ],
    )?;
    let size = global(
        rt,
        "adjust",
        vec![
            Js::Number((amount.to_number()? - rest.to_number()?) / lots.to_number()?),
            market.get("precision")?.get("amount")?,
        ],
    )?;
    // Use the source lodash times operation for its own length admission rules.
    let weak = std::rc::Rc::downgrade(&rt.shared);
    let original = req.clone();
    let quantity = size.clone();
    let callback = Js::function("lot", move |_, _| {
        let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
        let next = original.spread_object()?;
        next.set("quantity", string(quantity.clone())?)?;
        next.set("newClientOrderId", client_id(&rt)?)?;
        Ok(next)
    });
    let output = global(rt, "times", vec![lots, callback])?;
    if rest.truthy() {
        let next = req.spread_object()?;
        next.set("quantity", string(rest)?)?;
        next.set("newClientOrderId", client_id(rt)?)?;
        output.call_method("push", vec![next])?;
    }
    Ok(output)
}
fn place_order(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let payloads = rt.call_path("formatCreateOrder", vec![arg(&a, 0)])?;
        await_js(rt.call_path("placeOrderPayloads", vec![payloads])?).await
    })
}
fn place_orders(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let mut payloads = vec![];
        for order in arg(&a, 0).iterable_values()? {
            payloads.extend(
                rt.call_path("formatCreateOrder", vec![order])?
                    .iterable_values()?,
            );
        }
        await_js(rt.call_path("placeOrderPayloads", vec![Js::array(payloads)])?).await
    })
}
fn place_payloads(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let lots = global(&rt, "chunk", vec![arg(&a, 0), Js::Number(5.0)])?;
        let ids = Js::array(vec![]);
        for lot in lots.iterable_values()? {
            if lot.get("length")?.strict_equals(&Js::Number(1.0)) {
                let data =
                    request(&rt, "unlimitedXHR", "post", "ORDER", vec![lot.get("0")?]).await?;
                let id = data.get("orderId")?;
                ids.call_method(
                    "push",
                    vec![if id.is_nullish() {
                        Js::Undefined
                    } else {
                        id.call_method("toString", vec![])?
                    }],
                )?;
            } else {
                let batch = global(&rt, "JSON.stringify", vec![lot])?;
                let data = request(
                    &rt,
                    "unlimitedXHR",
                    "post",
                    "BATCH_ORDERS",
                    vec![Js::object(vec![("batchOrders", batch)])],
                )
                .await?;
                if !data.is_nullish() {
                    let callback_ids = ids.clone();
                    let weak = std::rc::Rc::downgrade(&rt.shared);
                    let callback = Js::function("collectOrderId", move |_, a| {
                        let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
                        let order = arg(&a, 0);
                        if order.get("code")?.truthy() {
                            rt.call_path("emitAndLogError", vec![order.get("msg")?])?;
                        } else {
                            let id = order.get("orderId")?;
                            callback_ids.call_method(
                                "push",
                                vec![if id.is_nullish() {
                                    Js::Undefined
                                } else {
                                    id.call_method("toString", vec![])?
                                }],
                            )?;
                        }
                        Ok(Js::Undefined)
                    });
                    let for_each = data.get("forEach")?;
                    if !for_each.is_nullish() {
                        for_each.call(data, vec![callback])?;
                    }
                }
            }
        }
        ids.call_method("filter", vec![rt.global("Boolean")?])
    })
}
fn cancel_orders(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let result = async {
            let groups = global(&rt, "groupBy", vec![arg(&a, 0), Js::str("symbol")])?;
            for (symbol, orders) in groups.own_entries()? {
                let lots = global(&rt, "chunk", vec![orders, Js::Number(10.0)])?;
                for lot in lots.iterable_values()? {
                    let (endpoint, params) = if lot.get("length")?.strict_equals(&Js::Number(1.0)) {
                        (
                            "ORDER",
                            Js::object(vec![
                                ("symbol", Js::utf16(&symbol)),
                                ("orderId", lot.get("0")?.get("id")?),
                            ]),
                        )
                    } else {
                        let mut ids = vec![];
                        for order in lot.iterable_values()? {
                            ids.push(order.get("id")?);
                        }
                        let joined = Js::array(ids).call_method("join", vec![Js::str(",")])?;
                        (
                            "BATCH_ORDERS",
                            Js::object(vec![
                                ("symbol", Js::utf16(&symbol)),
                                ("orderIdList", Js::str(&format!("[{}]", text(&joined)?))),
                            ]),
                        )
                    };
                    request(
                        &rt,
                        "xhr",
                        "delete",
                        endpoint,
                        vec![Js::object(vec![("params", params)])],
                    )
                    .await?;
                    let mut ids = vec![];
                    for order in lot.iterable_values()? {
                        ids.push(Js::object(vec![("id", order.get("id")?)]));
                    }
                    rt.call_path(
                        "store.updateOrdersStatus",
                        vec![Js::array(ids), enum_value(&rt, "OrderStatus", "Canceled")?],
                    )?;
                }
            }
            Ok::<_, Thrown>(Js::Undefined)
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                if optional(&error.value, "code")?.strict_equals(&Js::Number(-2011.0)) {
                    return Ok(Js::Undefined);
                }
                emit(&rt, &error.value)?;
                Err(error)
            }
        }
    })
}
fn cancel_symbol(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let symbol = arg(&a, 0);
        let result = async {
            request(
                &rt,
                "xhr",
                "delete",
                "CANCEL_SYMBOL_ORDERS",
                vec![Js::object(vec![(
                    "params",
                    Js::object(vec![("symbol", symbol.clone())]),
                )])],
            )
            .await?;
            let mut orders = vec![];
            for order in store(&rt, "orders")?.iterable_values()? {
                if order.get("symbol")?.strict_equals(&symbol) {
                    orders.push(order);
                }
            }
            rt.call_path(
                "store.updateOrdersStatus",
                vec![
                    Js::array(orders),
                    enum_value(&rt, "OrderStatus", "Canceled")?,
                ],
            )?;
            Ok::<_, Thrown>(Js::Undefined)
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                emit(&rt, &error.value)?;
                Err(error)
            }
        }
    })
}
fn update_order(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let input = arg(&a, 0);
        let order = input.get("order")?;
        let update = input.get("update")?;
        let next = Js::object(vec![]);
        for key in ["symbol", "type", "side", "price", "amount"] {
            next.set(key, order.get(key)?)?;
        }
        for key in ["reduceOnly", "close"] {
            next.set(key, or(order.get(key)?, || Ok(Js::Bool(false)))?)?;
        }
        for key in ["tpTriggerType", "slTriggerType"] {
            if contains(&order, key)? {
                next.set(key, order.get(key)?)?;
            }
        }
        for key in ["price", "amount"] {
            if contains(&update, key)? {
                next.set(key, update.get(key)?)?;
            }
        }
        await_js(rt.call_path("cancelOrders", vec![Js::array(vec![order])])?).await?;
        await_js(rt.call_path("placeOrder", vec![next])?).await
    })
}
fn change_mode(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let hedged = arg(&a, 0);
        let symbol = arg(&a, 1);
        let mut occupied = 0;
        for p in store(&rt, "positions")?.iterable_values()? {
            if p.get("contracts")?.to_number()? > 0.0 {
                occupied += 1;
            }
        }
        if occupied > 0 {
            rt.call_path(
                "emitAndLogError",
                vec![Js::str(
                    "Please close all positions before switching position mode",
                )],
            )?;
            return Ok(Js::Undefined);
        }
        let result = async {
            request(
                &rt,
                "xhr",
                "post",
                "HEDGE_MODE",
                vec![Js::object(vec![(
                    "dualSidePosition",
                    Js::str(if hedged.truthy() { "true" } else { "false" }),
                )])],
            )
            .await?;
            rt.call_path(
                "applyConfirmedPositionMode",
                vec![hedged.clone(), symbol.clone()],
            )
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let code = optional(
                    &optional(&optional(&error.value, "response")?, "data")?,
                    "code",
                )?;
                if code.strict_equals(&Js::Number(-4059.0)) || code.strict_equals(&Js::str("-4059"))
                {
                    rt.call_path("applyConfirmedPositionMode", vec![hedged, symbol])?;
                } else {
                    emit(&rt, &error.value)?;
                }
                Ok(Js::Undefined)
            }
        }
    })
}
fn apply_mode(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    let hedged = arg(a, 0);
    let symbol = arg(a, 1);
    let mode = enum_value(
        rt,
        "PositionMode",
        if hedged.truthy() { "Hedged" } else { "OneWay" },
    )?;
    let symbols = if symbol.truthy() {
        vec![symbol]
    } else {
        store(rt, "markets")?
            .iterable_values()?
            .iter()
            .map(|m| m.get("symbol"))
            .collect::<Result<Vec<_>, _>>()?
    };
    let leverage = store(rt, "fetchedLeverage")?.spread_object()?;
    for symbol in &symbols {
        let previous = leverage.get_property(symbol)?;
        let row = previous.spread_object()?;
        for key in ["buyLeverage", "sellLeverage"] {
            let v = optional(&previous, key)?;
            row.set(key, if v.is_nullish() { Js::Number(0.0) } else { v })?;
        }
        row.set("positionMode", mode.clone())?;
        leverage.set_key(&symbol.to_js_string()?, row)?;
    }
    let mut positions = vec![];
    for p in store(rt, "positions")?.iterable_values()? {
        let mut selected = false;
        for symbol in &symbols {
            if p.get("symbol")?.strict_equals(symbol) {
                selected = true;
                break;
            }
        }
        if selected && p.get("contracts")?.to_number()? == 0.0 {
            let row = p.spread_object()?;
            row.set(
                "id",
                global(
                    rt,
                    "createPositionId",
                    vec![p.get("symbol")?, p.get("side")?, mode.clone()],
                )?,
            )?;
            row.set("positionMode", mode.clone())?;
            positions.push(row);
        } else {
            positions.push(p);
        }
    }
    rt.call_path("store.setSetting", vec![Js::str("isHedged"), hedged])?;
    rt.call_path(
        "store.update",
        vec![Js::object(vec![
            ("fetchedLeverage", leverage),
            ("positions", Js::array(positions)),
        ])],
    )?;
    Ok(Js::Undefined)
}
fn after_worker(rt: &Rt, a: &[Js]) -> Result<Js, Thrown> {
    if !arg(a, 1).strict_equals(&Js::str("changePositionMode")) {
        return Ok(Js::Undefined);
    }
    let symbol = arg(a, 3);
    rt.call_path(
        "applyConfirmedPositionMode",
        vec![
            Js::Bool(arg(a, 2).truthy()),
            if symbol.is_string() {
                symbol
            } else {
                Js::Undefined
            },
        ],
    )?;
    Ok(Js::Undefined)
}
fn change_margin(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let isolated = arg(&a, 0);
        let symbol = arg(&a, 1);
        if !symbol.truthy() {
            return Err(Thrown::error("Error", "Symbol is required"));
        }
        if let Err(error) = request(
            &rt,
            "xhr",
            "post",
            "SET_MARGIN_TYPE",
            vec![Js::object(vec![
                ("symbol", symbol),
                (
                    "marginType",
                    Js::str(if isolated.truthy() {
                        "ISOLATED"
                    } else {
                        "CROSSED"
                    }),
                ),
            ])],
        )
        .await
        {
            emit(&rt, &error.value)?;
            return Err(Thrown::error(
                "Error",
                &text(&error_message(&error.value)?)?,
            ));
        }
        Ok(Js::Undefined)
    })
}
fn set_leverage(rt: Rt, a: Vec<Js>) -> JsFuture<'static> {
    Box::pin(async move {
        let symbol = arg(&a, 0);
        let market = store(&rt, "marketsBySymbol")?.get_property(&symbol)?;
        let mut position = Js::Undefined;
        for p in store(&rt, "positions")?.iterable_values()? {
            if p.get("symbol")?.strict_equals(&symbol) {
                position = p;
                break;
            }
        }
        if !market.truthy() {
            return Err(Thrown::error(
                "Error",
                &format!("Market {} not found", text(&symbol)?),
            ));
        }
        if !position.truthy() {
            return Err(Thrown::error(
                "Error",
                &format!("Position {} not found", text(&symbol)?),
            ));
        }
        let limits = market.get("limits")?.get("leverage")?;
        let min = global(&rt, "Math.max", vec![arg(&a, 1), limits.get("min")?])?;
        let leverage = global(&rt, "Math.min", vec![min, limits.get("max")?])?;
        if !position.get("leverage")?.strict_equals(&leverage) {
            let result = async {
                request(
                    &rt,
                    "xhr",
                    "post",
                    "SET_LEVERAGE",
                    vec![Js::object(vec![
                        ("symbol", symbol.clone()),
                        ("leverage", leverage.clone()),
                    ])],
                )
                .await?;
                let mut patches = vec![];
                for key in ["Long", "Short"] {
                    patches.push(Js::array(vec![
                        Js::object(vec![(
                            "id",
                            global(
                                &rt,
                                "createPositionId",
                                vec![symbol.clone(), enum_value(&rt, "PositionSide", key)?],
                            )?,
                        )]),
                        Js::object(vec![("leverage", leverage.clone())]),
                    ]));
                }
                rt.call_path("store.updatePositions", vec![Js::array(patches)])
            }
            .await;
            if let Err(error) = result {
                emit(&rt, &error.value)?;
            }
        }
        Ok(Js::Undefined)
    })
}
