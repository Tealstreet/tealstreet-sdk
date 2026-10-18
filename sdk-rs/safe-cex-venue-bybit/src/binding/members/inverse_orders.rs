//! Dedicated Inverse writes: integer USD contracts, raw venue diagnostics and
//! the source's filtered-response/original-payload correspondence.
use crate::binding::{
    host::{get_key, text},
    promise::await_js,
    rt::{Rt, venue},
    value::{Js, Thrown, arg, from_json, to_typed},
};
use safe_cex_types::{OrderType, PlaceOrderOpts};
use std::borrow::Borrow;

fn method(
    rt: &Rt,
    name: &'static str,
    body: fn(Rt, Vec<Js>) -> Result<Js, Thrown>,
) -> Result<(), Thrown> {
    let rt = rt.clone();
    rt.this
        .clone()
        .define(name, Js::function(name, move |_, a| body(rt.clone(), a)))
}
fn emit(rt: &Rt, value: Js) -> Result<(), Thrown> {
    rt.this.call_method("emitAndLogError", vec![value])?;
    Ok(())
}
fn error(value: &Js) -> Result<Thrown, Thrown> {
    Ok(Thrown::error(
        "Error",
        &if value.is_undefined() {
            String::new()
        } else {
            text(value)?
        },
    ))
}
fn number(value: f64) -> Js {
    Js::str(&safe_cex_num::js_number_string(value))
}
async fn post(rt: &Rt, endpoint: &str, payload: Js) -> Result<Js, Thrown> {
    let endpoint = rt.global("ENDPOINTS")?.get(endpoint)?;
    await_js(
        &rt.exchange_runtime(),
        rt.this
            .get("xhr")?
            .call_method("post", vec![endpoint, payload])?,
    )
    .await?
    .get("data")
}
fn items(value: &Js) -> Result<Vec<Js>, Thrown> {
    value
        .array_values()?
        .ok_or_else(|| Thrown::type_error("value is not iterable"))
}

pub(crate) fn install(rt: &Rt) -> Result<(), Thrown> {
    method(rt, "placeOrder", place_order)?;
    method(rt, "getStopOrderPositionIdx", |rt, a| {
        let opts = arg(&a, 0).spread_object()?;
        opts.delete(&"reduceOnly".encode_utf16().collect::<Vec<_>>())?;
        Ok(rt.clone().spawn(Box::pin(async move {
            let index = await_js(
                &rt.exchange_runtime(),
                rt.this.call_method("getOrderPositionIdx", vec![opts])?,
            )
            .await?;
            Ok(if index.strict_equals(&Js::Number(0.)) {
                Js::Number(0.)
            } else if index.strict_equals(&Js::Number(1.)) {
                Js::Number(2.)
            } else if index.strict_equals(&Js::Number(2.)) {
                Js::Number(1.)
            } else {
                Js::Undefined
            })
        })))
    })?;
    method(rt, "placeStopLossOrTakeProfit", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            trading_stop(&rt, arg(&a, 0), false).await
        })))
    })?;
    method(rt, "placeTrailingStopLoss", |rt, a| {
        Ok(rt.clone().spawn(Box::pin(async move {
            trading_stop(&rt, arg(&a, 0), true).await
        })))
    })?;
    method(rt, "recordRecentlyPlacedOrderFromOpts", |rt, a| {
        crate::spot_order_writes::record_with_remaining(
            &rt,
            &rt.this,
            arg(&a, 0),
            arg(&a, 1),
            arg(&a, 2),
            |_, qty, price| {
                let remaining =
                    super::inverse_mapping::remaining(qty.to_number()?, price.to_number()?);
                Ok(vec![
                    ("remaining", remaining.get("remaining")?),
                    ("extra", remaining.get("extra")?),
                ])
            },
        )?;
        Ok(Js::Undefined)
    })?;
    method(rt, "recordRecentlyPlacedOrder", |rt, a| {
        super::lifecycle::record_variant_recently_placed_order(&rt, arg(&a, 0))
    })?;
    method(rt, "applyLocalOrderUpdateAfterRestWrite", |rt, a| {
        if !rt
            .this
            .call_method("shouldUseLocalRestOrderLifecycleFallback", vec![])?
            .truthy()
        {
            return Ok(Js::Undefined);
        }
        let order = arg(&a, 0);
        let update = arg(&a, 1);
        let amount = update.get("amount")?;
        let native = if matches!(&amount, Js::Number(_)) {
            let filled = order.get("filled")?;
            Js::Number(safe_cex_num::math_max(
                0.,
                amount.to_number()?
                    - if filled.truthy() {
                        filled.to_number()?
                    } else {
                        0.
                    },
            ))
        } else {
            let extra = order.get("extra")?;
            let native = if extra.is_nullish() {
                Js::Undefined
            } else {
                extra.get("remainingNative")?
            };
            if native.is_nullish() {
                order.get("remaining")?
            } else {
                native
            }
        };
        let price = update.get("price")?;
        let price = if price.is_nullish() {
            order.get("price")?
        } else {
            price
        };
        let next = order.spread_object()?;
        for (key, value) in update.own_entries()? {
            next.define_key(&key, value)?;
        }
        let remaining = super::inverse_mapping::remaining(native.to_number()?, price.to_number()?);
        next.define("remaining", remaining.get("remaining")?)?;
        let extra = order.get("extra")?.spread_object()?;
        extra.define("remainingNative", native.clone())?;
        extra.define("remainingNotional", native)?;
        next.define("extra", extra)?;
        next.define(
            "lastUpdate",
            rt.global("virtualClock.getCurrentTimeMs")?
                .call(Js::Undefined, vec![])?,
        )?;
        rt.this
            .get("store")?
            .call_method("addOrUpdateOrders", vec![Js::array(vec![next])])?;
        Ok(Js::Undefined)
    })?;
    Ok(())
}

// ts: BybitInverseExchange.placeOrder
fn place_order(rt: Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let opts = arg(&a, 0);
    let client_id = rt
        .this
        .call_method("resolveCallerClientOrderId", vec![opts.clone()])?;
    Ok(rt.clone().spawn(Box::pin(async move {
        let category = rt
            .this
            .call_method("getCategoryForSymbol", vec![opts.get("symbol")?])?;
        let execution = rt
            .this
            .call_method("getOrderExecutionParams", vec![opts.clone()])?;
        let kind = opts.get("type")?;
        let opposite = rt
            .this
            .call_method(
                "_hasFullSizedPositionOppositeSide",
                vec![opts.get("symbol")?, opts.get("side")?, opts.get("amount")?],
            )?
            .truthy();
        if opposite
            && (kind.strict_equals(&Js::str("stop_market"))
                || kind.strict_equals(&Js::str("take_profit_market")))
        {
            return await_js(
                &rt.exchange_runtime(),
                rt.this
                    .call_method("placeStopLossOrTakeProfit", vec![opts])?,
            )
            .await;
        }
        if kind.strict_equals(&Js::str("trailing_stop_market")) {
            return await_js(
                &rt.exchange_runtime(),
                rt.this.call_method("placeTrailingStopLoss", vec![opts])?,
            )
            .await;
        }
        let typed: crate::order_params::BybitPlaceOrderOpts = to_typed(&opts)?;
        let base: &PlaceOrderOpts = typed.borrow();
        let market = venue(&rt)?
            .base
            .store
            .markets()?
            .iter()
            .find(|m| m.symbol == base.symbol)
            .cloned()
            .ok_or_else(|| Thrown::error("Error", &format!("Market {} not found", base.symbol)))?;
        let position_opts = opts.spread_object()?;
        if base.r#type == OrderType::Conditional {
            position_opts.set("reduceOnly", Js::Bool(false))?;
        }
        let index = await_js(
            &rt.exchange_runtime(),
            rt.this
                .call_method("getOrderPositionIdx", vec![position_opts])?,
        )
        .await?
        .to_number()?;
        let inverse = market.is_inverse.unwrap_or(false);
        let amount = if inverse {
            let mut amount = safe_cex_num::math_round(base.amount);
            let min = market.limits.amount.min;
            let min = if min != 0. && !min.is_nan() { min } else { 1. };
            if amount < min {
                amount = min;
            }
            let max = safe_cex_num::numeric_value_to_number(&market.limits.amount.max);
            if max != 0. && !max.is_nan() && amount > max {
                amount = max;
            }
            rt.this.call_method(
                "log",
                vec![Js::str(&format!(
                    "BybitInverse order: input amount={}, USD contracts={}, symbol={}",
                    safe_cex_num::js_number_string(base.amount),
                    safe_cex_num::js_number_string(amount),
                    base.symbol
                ))],
            )?;
            amount
        } else {
            safe_cex_num::adjust(base.amount, market.precision.amount)
        };
        let ticker = venue(&rt)?
            .base
            .store
            .tickers()?
            .iter()
            .find(|t| t.symbol == base.symbol)
            .cloned();
        let formatted = match crate::order_format::format_inverse_order_for_batch(
            base,
            Some(&market),
            ticker.as_ref(),
            index,
        ) {
            Ok(value) => value,
            Err(message) => {
                let message = message
                    .strip_prefix("Failed to format order for batch: ")
                    .unwrap_or(&message);
                if message.starts_with("Trigger price is required") {
                    emit(&rt, Js::str(message))?;
                }
                return Err(Thrown::error(
                    if message.starts_with("Cannot read properties") {
                        "TypeError"
                    } else {
                        "Error"
                    },
                    message,
                ));
            }
        };
        let request = from_json(&formatted);
        request.delete(&"orderLinkId".encode_utf16().collect::<Vec<_>>())?;
        request.set("category", category)?;
        if !execution.get("bboSideType")?.is_undefined() {
            request.delete(&"price".encode_utf16().collect::<Vec<_>>())?;
        }
        for (key, value) in execution.own_entries()? {
            request.set_key(&key, value)?;
        }
        let max = safe_cex_num::numeric_value_to_number(&market.limits.amount.max);
        let lots = if amount > max {
            safe_cex_num::divide(amount, max, None).ceil()
        } else {
            1.
        };
        let rest = if amount > max {
            if inverse {
                (amount % max).floor()
            } else {
                safe_cex_num::adjust(amount % max, market.precision.amount)
            }
        } else {
            0.
        };
        let divided = safe_cex_num::divide(safe_cex_num::subtract(amount, rest), lots, None);
        let size = if inverse {
            divided.floor()
        } else {
            safe_cex_num::adjust(divided, market.precision.amount)
        };
        let original_request = request.clone();
        let repeated_id = client_id.clone();
        let payloads = rt.global("times")?.call(
            Js::Undefined,
            vec![
                Js::Number(lots),
                Js::function("inverseOrderLot", move |_, args| {
                    let payload = original_request.spread_object()?;
                    if arg(&args, 0).to_number()? > 0. {
                        for key in ["stopLoss", "takeProfit", "slTriggerBy", "tpTriggerBy"] {
                            payload.delete(&key.encode_utf16().collect::<Vec<_>>())?;
                        }
                    }
                    payload.set("qty", number(size))?;
                    if !repeated_id.is_undefined() {
                        payload.set("orderLinkId", repeated_id.clone())?;
                    }
                    Ok(payload)
                }),
            ],
        )?;
        if rest != 0. && !rest.is_nan() {
            let remainder = request.spread_object()?;
            remainder.set("qty", number(rest))?;
            payloads.call_method("push", vec![remainder])?;
        }
        rt.this.call_method(
            "assertSingleNativeClientOrderId",
            vec![client_id.clone(), payloads.get("length")?],
        )?;
        let payloads = items(&payloads)?;
        let mut responses = vec![];
        for payload in &payloads {
            match post(&rt, "CREATE_ORDER", payload.clone()).await {
                Ok(data) => {
                    if !data.is_undefined() {
                        responses.push(data);
                    }
                }
                Err(failure) => {
                    let response = failure.value.get("response")?;
                    let data = if response.is_nullish() {
                        Js::Undefined
                    } else {
                        response.get("data")?
                    };
                    let message = if data.is_nullish() {
                        Js::Undefined
                    } else {
                        data.get("retMsg")?
                    };
                    let message = if message.truthy() {
                        message
                    } else {
                        failure.value.get("message")?
                    };
                    emit(&rt, message)?;
                    if payloads.len() == 1
                        && !client_id.is_undefined()
                        && rt
                            .global("isUnknownOutcome")?
                            .call(Js::Undefined, vec![failure.value.clone()])?
                            .truthy()
                    {
                        return Err(failure);
                    }
                }
            }
        }
        let mut failure = Js::str("");
        for data in &responses {
            let message = get_key(data, "retMsg")?;
            if !message.strict_equals(&Js::str("OK")) {
                emit(&rt, message.clone())?;
                failure = message;
            }
        }
        if !failure.strict_equals(&Js::str("")) {
            return Err(error(&failure)?);
        }
        let mut ids = vec![];
        for (index, data) in responses.into_iter().enumerate() {
            let id = data.get("result")?.get("orderId")?;
            if id.truthy() {
                if let Some(payload) = payloads.get(index) {
                    rt.this.call_method(
                        "recordRecentlyPlacedOrderFromOpts",
                        vec![id.clone(), opts.clone(), payload.clone()],
                    )?;
                }
            }
            ids.push(id);
        }
        Ok(Js::array(ids))
    })))
}

// ts: BybitInverseExchange.placeStopLossOrTakeProfit/placeTrailingStopLoss.
// This source has no post-write snapshot timer.
async fn trading_stop(rt: &Rt, opts: Js, trailing: bool) -> Result<Js, Thrown> {
    let base: PlaceOrderOpts = to_typed(&opts)?;
    let parent = venue(rt)?;
    let market = parent
        .base
        .store
        .markets()?
        .iter()
        .find(|m| m.symbol == base.symbol)
        .cloned();
    let ticker = parent
        .base
        .store
        .tickers()?
        .iter()
        .find(|t| t.symbol == base.symbol)
        .cloned();
    drop(parent);
    let market = market.ok_or_else(|| {
        Thrown::error(
            "Error",
            &format!(
                "{} {} not found",
                if trailing { "Ticker" } else { "Market" },
                base.symbol
            ),
        )
    })?;
    let distance = if trailing {
        let ticker = ticker
            .ok_or_else(|| Thrown::error("Error", &format!("Ticker {} not found", base.symbol)))?;
        let price = base.price.unwrap_or(f64::NAN);
        Some(safe_cex_num::adjust(
            safe_cex_num::math_max(ticker.last, price) - safe_cex_num::math_min(ticker.last, price),
            market.precision.price,
        ))
    } else {
        None
    };
    let category = rt
        .this
        .call_method("getCategoryForSymbol", vec![Js::str(&base.symbol)])?;
    let request = Js::object(vec![
        ("category", category),
        ("symbol", Js::str(&base.symbol)),
        (
            "positionIdx",
            await_js(
                &rt.exchange_runtime(),
                rt.this
                    .call_method("getStopOrderPositionIdx", vec![opts.clone()])?,
            )
            .await?,
        ),
    ]);
    if let Some(distance) = distance {
        request.set("trailingStop", number(distance))?;
    } else {
        let amount = safe_cex_num::adjust(base.amount, market.precision.amount);
        let sized = amount != 0. && !amount.is_nan();
        request.set("tpslMode", Js::str(if sized { "Partial" } else { "Full" }))?;
        let (field, trigger, size, last_default) = if base.r#type == OrderType::StopLoss {
            ("stopLoss", "slTriggerBy", "slSize", false)
        } else {
            ("takeProfit", "tpTriggerBy", "tpSize", true)
        };
        request.set(field, Js::str(&text(&opts.get("price")?)?))?;
        use safe_cex_types::OrderStopTriggerType as T;
        let trigger_kind = if last_default {
            base.tp_trigger_type
        } else {
            base.sl_trigger_type
        };
        request.set(
            trigger,
            Js::str(
                if (last_default && trigger_kind != Some(T::MarkPrice))
                    || (!last_default && trigger_kind == Some(T::LastPrice))
                {
                    "LastPrice"
                } else {
                    "MarkPrice"
                },
            ),
        )?;
        if sized {
            request.set(size, number(amount))?;
        }
    }
    let data = post(rt, "SET_TRADING_STOP", request).await?;
    let message = data.get("retMsg")?;
    let normalized = if message.truthy() {
        text(&message)?
    } else {
        String::new()
    };
    if normalized.to_lowercase().contains("not modified") {
        return Ok(Js::array(vec![]));
    }
    if !message.strict_equals(&Js::str("OK")) {
        emit(rt, message.clone())?;
        return Err(error(&message)?);
    }
    Ok(Js::array(vec![]))
}
