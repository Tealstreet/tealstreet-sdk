//! Source batch orchestration on the live receiver. Category grouping, chunking,
//! concurrent formatting and individual fallback retain the source order.
use crate::{
    binding::{
        host::text,
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, arg, from_json, from_typed, to_json, to_typed},
    },
    order_params::BybitPlaceOrderOpts,
};
use std::{borrow::Borrow, rc::Rc};

fn method(
    rt: &Rt,
    name: &'static str,
    body: fn(Rt, Vec<Js>) -> Result<Js, Thrown>,
) -> Result<(), Thrown> {
    let owned = rt.clone();
    rt.this.define(
        name,
        Js::function(name, move |_, args| body(owned.clone(), args)),
    )
}
fn items(value: &Js) -> Result<Vec<Js>, Thrown> {
    Ok(value
        .array_items()?
        .ok_or_else(|| Thrown::type_error("value is not iterable"))?
        .into_iter()
        .map(|item| item.unwrap_or(Js::Undefined))
        .collect())
}
fn emit(rt: &Rt, value: Js) -> Result<(), Thrown> {
    rt.this.call_method("emitAndLogError", vec![value])?;
    Ok(())
}
fn error(message: &Js) -> Result<Thrown, Thrown> {
    Ok(Thrown::error(
        "Error",
        if message.is_undefined() {
            String::new()
        } else {
            text(message)?
        }
        .as_str(),
    ))
}
fn caught_message(failure: &Thrown) -> Result<String, Thrown> {
    text(&failure.value.get("message")?)
}
fn append(out: &mut Vec<Js>, values: Js) -> Result<(), Thrown> {
    out.extend(items(&values)?);
    Ok(())
}

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    method(rt, "getOrderExecutionParams", |rt, a| {
        let opts: BybitPlaceOrderOpts = to_typed(&arg(&a, 0))?;
        let params = crate::execution_guards::get_order_execution_params(
            &opts,
            |symbol| {
                rt.this
                    .call_method("getCategoryForSymbol", vec![Js::str(symbol)])
                    .and_then(|category| text(&category))
                    .map_err(|e| Rc::new(e.to_exchange_error()))
            },
            |message| emit(&rt, Js::str(message)).map_err(|e| Rc::new(e.to_exchange_error())),
        )
        .map_err(|e| Thrown::from((*e).clone()))?;
        from_typed(&params)
    })?;
    method(rt, "assertOptionOrderPlaceable", |rt, a| {
        super::orders::assert_option_order_placeable(&rt, &to_typed(&arg(&a, 0))?)?;
        Ok(Js::Undefined)
    })?;
    method(rt, "_hasFullSizedPositionOppositeSide", |rt, a| {
        let side = to_typed(&arg(&a, 1))?;
        Ok(Js::Bool(
            venue(&rt)?.has_full_sized_position_opposite_side(
                &text(&arg(&a, 0))?,
                side,
                arg(&a, 2).to_number()?,
            )?,
        ))
    })?;
    method(rt, "formatBybitVenueError", |rt, a| {
        Ok(Js::str(&crate::exchange_errors::format_bybit_venue_error(
            &to_json(&arg(&a, 0))?,
            &to_json(&arg(&a, 1))?,
            Some(&text(&rt.this.get("name")?)?),
        )))
    })?;
    method(rt, "getOrderPositionIdx", |rt, a| {
        let opts = arg(&a, 0);
        Ok(rt.clone().spawn(Box::pin({
            let await_runtime = rt.exchange_runtime();
            async move {
                let mode = await_js(
                    &await_runtime,
                    rt.this
                        .call_method("fetchPositionMode", vec![opts.get("symbol")?])?,
                )
                .await?;
                if mode.strict_equals(&Js::str("oneway")) {
                    return Ok(Js::Number(0.));
                }
                let mut index = if opts.get("side")?.strict_equals(&Js::str("buy")) {
                    1.
                } else {
                    2.
                };
                if opts.get("reduceOnly")?.truthy() {
                    index = if index == 1. { 2. } else { 1. };
                }
                Ok(Js::Number(index))
            }
        })))
    })?;
    method(rt, "placeOrders", place_orders)?;
    method(rt, "placeBatchOrders", place_batch_orders)?;
    method(
        rt,
        "placeBatchOrdersForCategory",
        place_batch_orders_for_category,
    )?;
    method(rt, "processBatch", process_batch)?;
    method(rt, "formatOrderForBatch", format_order_for_batch)?;
    Ok(())
}

// Source-identical category grouping and delays are installed by the common
// receiver. Only the three differing inverse members replace that surface.
pub(crate) fn install_inverse(rt: &Rt) -> Result<(), Thrown> {
    method(rt, "placeOrders", |rt, a| place_orders_body(rt, a, false))?;
    method(rt, "processBatch", |rt, a| process_batch_body(rt, a, false))?;
    method(rt, "formatOrderForBatch", |rt, a| {
        format_order_body(rt, a, false)
    })?;
    Ok(())
}

// ts: BybitExchange.placeOrders
fn place_orders(rt: Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    place_orders_body(rt, a, true)
}
fn place_orders_body(rt: Rt, a: Vec<Js>, primary: bool) -> Result<Js, Thrown> {
    let orders = arg(&a, 0);
    Ok(rt.clone().spawn(Box::pin({
        let await_runtime = rt.exchange_runtime();
        async move {
            if orders.get("length")?.strict_equals(&Js::Number(0.)) {
                return Ok(Js::array(vec![]));
            }
            rt.this
                .call_method("assertCallerClientOrderIds", vec![orders.clone()])?;
            for order in items(&orders)? {
                if primary && crate::options::is_bybit_option_symbol(&text(&order.get("symbol")?)?)
                {
                    rt.this
                        .call_method("assertOptionOrderPlaceable", vec![order])?;
                } else {
                    rt.this
                        .call_method("getOrderExecutionParams", vec![order])?;
                }
            }
            let mut special = vec![];
            let mut regular = vec![];
            for order in items(&orders)? {
                let kind = order.get("type")?;
                let opposite = rt
                    .this
                    .call_method(
                        "_hasFullSizedPositionOppositeSide",
                        vec![
                            order.get("symbol")?,
                            order.get("side")?,
                            order.get("amount")?,
                        ],
                    )?
                    .truthy();
                let opts: BybitPlaceOrderOpts = to_typed(&order)?;
                let base: &safe_cex_types::PlaceOrderOpts = opts.borrow();
                if (opposite
                    && (kind.strict_equals(&Js::str("stop_market"))
                        || kind.strict_equals(&Js::str("take_profit_market"))))
                    || kind.strict_equals(&Js::str("trailing_stop_market"))
                    || crate::order_params::has_bybit_single_order_params(&opts)
                    || (primary && crate::options::is_bybit_option_symbol(&base.symbol))
                {
                    special.push(order);
                } else {
                    regular.push(order);
                }
            }
            let mut ids = vec![];
            for order in special {
                let result = async {
                    await_js(
                        &await_runtime,
                        rt.this.call_method("placeOrder", vec![order])?,
                    )
                    .await
                }
                .await;
                match result {
                    Ok(values) => append(&mut ids, values)?,
                    Err(failure) => emit(
                        &rt,
                        Js::str(&format!(
                            "Failed to place special order: {}",
                            caught_message(&failure)?
                        )),
                    )?,
                }
            }
            if !regular.is_empty() {
                append(
                    &mut ids,
                    await_js(
                        &await_runtime,
                        rt.this
                            .call_method("placeBatchOrders", vec![Js::array(regular)])?,
                    )
                    .await?,
                )?;
            }
            Ok(Js::array(ids))
        }
    })))
}

// ts: BybitExchange.placeBatchOrders (also the exact standalone Inverse body)
fn place_batch_orders(rt: Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    Ok(rt.clone().spawn(Box::pin({
        let await_runtime = rt.exchange_runtime();
        async move {
            let grouped = Js::object(vec![]);
            for order in items(&arg(&a, 0))? {
                let category = rt
                    .this
                    .call_method("getCategoryForSymbol", vec![order.get("symbol")?])?;
                let key = category.to_js_string()?;
                if !grouped.get_key(&key)?.truthy() {
                    grouped.set_key(&key, Js::array(vec![]))?;
                }
                grouped.get_key(&key)?.push(order)?;
            }
            let mut ids = vec![];
            for (category, orders) in grouped.own_entries()? {
                append(
                    &mut ids,
                    await_js(
                        &await_runtime,
                        rt.this.call_method(
                            "placeBatchOrdersForCategory",
                            vec![Js::String(category), orders],
                        )?,
                    )
                    .await?,
                )?;
            }
            Ok(Js::array(ids))
        }
    })))
}

// ts: BybitExchange.placeBatchOrdersForCategory (also the exact Inverse body)
fn place_batch_orders_for_category(rt: Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    Ok(rt.clone().spawn(Box::pin({
        let await_runtime = rt.exchange_runtime();
        async move {
            let category = arg(&a, 0);
            let batches = rt
                .global("chunk")?
                .call(Js::Undefined, vec![arg(&a, 1), rt.this.get("batchLimit")?])?;
            let mut ids = vec![];
            for (index, batch) in items(&batches)?.into_iter().enumerate() {
                if index > 0 {
                    await_js(
                        &await_runtime,
                        rt.global("sleep")?
                            .call(Js::Undefined, vec![Js::Number(1200.)])?,
                    )
                    .await?;
                }
                let result = async {
                    await_js(
                        &await_runtime,
                        rt.this
                            .call_method("processBatch", vec![category.clone(), batch.clone()])?,
                    )
                    .await
                }
                .await;
                match result {
                    Ok(values) => append(&mut ids, values)?,
                    Err(failure) => {
                        emit(
                            &rt,
                            Js::str(&format!(
                                "Failed to place batch orders: {}",
                                caught_message(&failure)?
                            )),
                        )?;
                        for order in items(&batch)? {
                            let result = async {
                                await_js(
                                    &await_runtime,
                                    rt.this.call_method("placeOrder", vec![order])?,
                                )
                                .await
                            }
                            .await;
                            match result {
                                Ok(values) => append(&mut ids, values)?,
                                Err(failure) => emit(
                                    &rt,
                                    Js::str(&format!(
                                        "Failed to place individual order: {}",
                                        caught_message(&failure)?
                                    )),
                                )?,
                            }
                        }
                    }
                }
            }
            Ok(Js::array(ids))
        }
    })))
}

// ts: BybitExchange.processBatch
fn process_batch(rt: Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    process_batch_body(rt, a, true)
}
fn process_batch_body(rt: Rt, a: Vec<Js>, primary: bool) -> Result<Js, Thrown> {
    Ok(rt.clone().spawn(Box::pin(async move {
        let mut pending = vec![];
        for order in items(&arg(&a, 1))? {
            let request = rt
                .this
                .call_method("formatOrderForBatch", vec![order.clone()]);
            pending.push(rt.clone().spawn(Box::pin({
                let await_runtime = rt.exchange_runtime();
                async move {
                    Ok(Js::object(vec![
                        ("order", order),
                        ("request", await_js(&await_runtime, request?).await?),
                    ]))
                }
            })));
        }
        let all = rt
            .global("Promise.all")?
            .call(rt.global("Promise")?, vec![Js::array(pending)])?;
        let mut valid = vec![];
        for entry in items(&await_js(&rt.exchange_runtime(), all).await?)? {
            if !entry.get("request")?.strict_equals(&Js::Null) {
                valid.push(entry);
            }
        }
        if valid.is_empty() {
            return Ok(Js::array(vec![]));
        }
        let request = valid
            .iter()
            .map(|v| v.get("request"))
            .collect::<Result<Vec<_>, _>>()?;
        let data = await_js(
            &rt.exchange_runtime(),
            rt.this.get("xhr")?.call_method(
                "post",
                vec![
                    rt.global("ENDPOINTS")?.get("CREATE_BATCH_ORDER")?,
                    Js::object(vec![
                        ("category", arg(&a, 0)),
                        ("request", Js::array(request)),
                    ]),
                ],
            )?,
        )
        .await?
        .get("data")?;
        if !data.get("retMsg")?.strict_equals(&Js::str("OK")) {
            let message = if primary {
                rt.this
                    .call_method("formatBybitVenueError", vec![data.clone()])?
            } else {
                data.get("retMsg")?
            };
            emit(&rt, message.clone())?;
            return Err(error(&message)?);
        }
        let mut errors = vec![];
        let extra = data.get("retExtInfo")?;
        let extra = if extra.is_nullish() {
            Js::Undefined
        } else {
            extra.get("list")?
        };
        if extra.truthy() {
            for (index, entry) in items(&extra)?.into_iter().enumerate() {
                if !entry.get("code")?.strict_equals(&Js::Number(0.)) {
                    let message = if primary {
                        rt.this.call_method("formatBybitVenueError", vec![entry])?
                    } else {
                        entry.get("msg")?
                    };
                    errors.push(format!("Order {}: {}", index + 1, text(&message)?));
                }
            }
        }
        let result = data.get("result")?;
        let result = if result.is_nullish() {
            Js::Undefined
        } else {
            result.get("list")?
        };
        let mut ids = vec![];
        if result.truthy() {
            for (index, result) in items(&result)?.into_iter().enumerate() {
                let id = result.get("orderId")?;
                if id.truthy() {
                    ids.push(id.clone());
                    if let Some(request) = valid.get(index) {
                        rt.this.call_method(
                            "recordRecentlyPlacedOrderFromOpts",
                            vec![id, request.get("order")?, request.get("request")?],
                        )?;
                    }
                }
            }
        }
        if !errors.is_empty() {
            emit(
                &rt,
                Js::str(&format!("Some orders failed: {}", errors.join(", "))),
            )?;
        }
        Ok(Js::array(ids))
    })))
}

// ts: BybitExchange.formatOrderForBatch — the previously ported request formatter
// receives the actual awaited position index. Request numerics are source string
// fields; the only numeric output, positionIdx, is the canonical 0/1/2 selector.
fn format_order_for_batch(rt: Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    format_order_body(rt, a, true)
}
fn format_order_body(rt: Rt, a: Vec<Js>, primary: bool) -> Result<Js, Thrown> {
    Ok(rt.clone().spawn(Box::pin({
        let await_runtime = rt.exchange_runtime();
        async move {
            let body = async {
                let opts = arg(&a, 0);
                let typed: safe_cex_types::PlaceOrderOpts = to_typed(&opts)?;
                let market = venue(&rt)?
                    .base
                    .store
                    .markets()?
                    .iter()
                    .find(|m| m.symbol == typed.symbol)
                    .cloned();
                let Some(market) = market else {
                    emit(&rt, Js::str(&format!("Market {} not found", typed.symbol)))?;
                    return Ok(Js::Null);
                };
                let position_opts = if typed.r#type == safe_cex_types::OrderType::Conditional {
                    let next = opts.spread_object()?;
                    next.set("reduceOnly", Js::Bool(false))?;
                    next
                } else {
                    opts
                };
                let index = await_js(
                    &await_runtime,
                    rt.this
                        .call_method("getOrderPositionIdx", vec![position_opts])?,
                )
                .await?
                .to_number()?;
                let ticker = venue(&rt)?
                    .base
                    .store
                    .tickers()?
                    .iter()
                    .find(|t| t.symbol == typed.symbol)
                    .cloned();
                let formatter = if primary {
                    crate::order_format::format_order_for_batch
                } else {
                    crate::order_format::format_inverse_order_for_batch
                };
                match formatter(&typed, Some(&market), ticker.as_ref(), index) {
                    Ok(request) => Ok(from_json(&request)),
                    Err(message) => {
                        // The pure body already prefixes caught format failures.
                        emit(&rt, Js::str(&message))?;
                        Ok(Js::Null)
                    }
                }
            }
            .await;
            match body {
                Ok(value) => Ok(value),
                Err(failure) => {
                    emit(
                        &rt,
                        Js::str(&format!(
                            "Failed to format order for batch: {}",
                            caught_message(&failure)?
                        )),
                    )?;
                    Ok(Js::Null)
                }
            }
        }
    })))
}
