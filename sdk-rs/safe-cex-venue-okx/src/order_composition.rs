use crate::{market_account_reads::MarketAccountOps, order_mutations::OrderMutationOps};
use std::rc::Rc;

pub struct CompositionOps<V, E> {
    pub order: Rc<OrderMutationOps<V, E>>,
    pub sort: Rc<dyn Fn(V, Rc<dyn Fn(V, V) -> Result<V, E>>) -> Result<V, E>>,
    pub map_bound: Rc<dyn Fn(V, V) -> Result<V, E>>,
}
fn v<V, E>(h: &CompositionOps<V, E>) -> &MarketAccountOps<V, E> {
    &h.order.value
}
fn call<V, E>(
    h: &CompositionOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = (v(h).capture)(name)?;
    (v(h).invoke)(f, args()?)
}
fn method<V, E>(
    h: &CompositionOps<V, E>,
    o: V,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = (v(h).member)(o, name)?;
    (v(h).invoke)(f, args()?)
}
fn or<V, E>(h: &CompositionOps<V, E>, x: V, next: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    if (v(h).truthy)(&x) { Ok(x) } else { next() }
}
fn optional<V, E>(h: &CompositionOps<V, E>, x: &V, k: &str) -> Result<V, E> {
    if (v(h).nullish)(x) {
        Ok((v(h).undefined)())
    } else {
        (v(h).get)(x, k)
    }
}
fn index<V, E>(h: &CompositionOps<V, E>, o: V, key: V) -> Result<V, E> {
    (v(h).intrinsic)("get-index", vec![o, key])
}
fn table<V, E>(h: &CompositionOps<V, E>, name: &str, key: V) -> Result<V, E> {
    index(h, (v(h).capture)(name)?, key)
}
fn is<V, E>(h: &CompositionOps<V, E>, o: &V, k: &str, name: &str) -> Result<bool, E> {
    Ok((v(h).equal)(&(v(h).get)(o, k)?, &(v(h).capture)(name)?))
}
fn text_is<V, E>(h: &CompositionOps<V, E>, o: &V, k: &str, text: &str) -> Result<bool, E> {
    Ok((v(h).equal)(&(v(h).get)(o, k)?, &(v(h).literal)(text)))
}
fn num<V, E>(h: &CompositionOps<V, E>, x: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    call(h, "parseFloat", || Ok(vec![x()?]))
}
fn fallback_float<V, E>(h: &CompositionOps<V, E>, o: &V, keys: &[&str]) -> Result<V, E> {
    num(h, || {
        for key in keys {
            let x = (v(h).get)(o, key)?;
            if (v(h).truthy)(&x) {
                return Ok(x);
            }
        }
        Ok((v(h).literal)("0"))
    })
}
fn stringify<V, E>(h: &CompositionOps<V, E>, x: V) -> Result<V, E> {
    (v(h).intrinsic)("template-string", vec![x])
}
fn define<V: Clone, E>(h: &CompositionOps<V, E>, o: &V, k: &str, x: V) -> Result<(), E> {
    (v(h).intrinsic)(
        "define-literal-field",
        vec![o.clone(), (v(h).literal)(k), x],
    )?;
    Ok(())
}
fn append<V: Clone, E>(h: &CompositionOps<V, E>, a: V, x: V) -> Result<V, E> {
    (v(h).intrinsic)("array-spread", vec![a, (v(h).array)(vec![x])])
}
fn fresh_error<V, E>(h: &CompositionOps<V, E>, message: V) -> Result<E, E> {
    let ctor = (v(h).capture)("Error")?;
    (v(h).new_error)(ctor, message)
}
fn fail<V: Clone, E>(h: &CompositionOps<V, E>, message: V) -> Result<V, E> {
    call(h, "this.emitAndLogError", || Ok(vec![message.clone()]))?;
    Err(fresh_error(h, message)?)
}
fn market<V, E>(h: &CompositionOps<V, E>, opts: &V) -> Result<V, E> {
    let markets = (v(h).capture)("this.store.marketsBySymbol")?;
    let m = index(h, markets, (v(h).get)(opts, "symbol")?)?;
    if !(v(h).truthy)(&m) {
        let ctor = (v(h).capture)("Error")?;
        let message = (v(h).intrinsic)("market-error-message", vec![(v(h).get)(opts, "symbol")?])?;
        return Err((v(h).new_error)(ctor, message)?);
    }
    Ok(m)
}
fn precision<V, E>(h: &CompositionOps<V, E>, m: &V, k: &str) -> Result<V, E> {
    (v(h).get)(&(v(h).get)(m, "precision")?, k)
}
fn adjust<V, E>(
    h: &CompositionOps<V, E>,
    amount: impl FnOnce() -> Result<V, E>,
    step: V,
) -> Result<V, E> {
    call(h, "adjust", || Ok(vec![amount()?, step]))
}
fn finite<V, E>(h: &CompositionOps<V, E>, x: V) -> Result<bool, E> {
    Ok((v(h).truthy)(&call(h, "Number.isFinite", || Ok(vec![x]))?))
}
fn cmp<V, E>(h: &CompositionOps<V, E>, name: &str, a: V, b: V) -> Result<bool, E> {
    Ok((v(h).truthy)(&(v(h).intrinsic)(name, vec![a, b])?))
}
fn broker_id<V, E>(h: &CompositionOps<V, E>) -> Result<V, E> {
    let first = (v(h).capture)("BROKER_ID")?;
    let id = call(h, "uuid", || Ok(vec![]))?;
    let full = (v(h).intrinsic)("template-concat", vec![first, id])?;
    method(h, full, "slice", || {
        Ok(vec![(v(h).number)(0.), (v(h).number)(32.)])
    })
}
fn opposite_side<V, E>(h: &CompositionOps<V, E>, opts: &V) -> Result<V, E> {
    let inverse = call(h, "inverseObj", || Ok(vec![(v(h).capture)("ORDER_SIDE")?]))?;
    index(h, inverse, (v(h).get)(opts, "side")?)
}

pub async fn place_order<V: Clone + 'static, E: 'static>(
    h: Rc<CompositionOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let p = call(&h, "this.placeOrders", || {
        Ok(vec![(v(&h).array)(vec![opts])])
    })?;
    (v(&h).await_value)(p).await
}
pub async fn place_orders<V: Clone + 'static, E: 'static>(
    h: Rc<CompositionOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    call(&h, "this.assertCallerClientOrderIds", || {
        Ok(vec![opts.clone()])
    })?;
    let mode = call(&h, "this.getOrFetchPositionMode", || Ok(vec![]))?;
    (v(&h).await_value)(mode).await?;
    let partition = (v(&h).capture)("partition")?;
    let ops = h.clone();
    let groups = (h.order.partition)(
        partition,
        opts,
        Rc::new(move |o| {
            call(&ops, "this.isAlgoOrder", || {
                Ok(vec![(v(&ops).get)(&o, "type")?])
            })
        }),
    )?;
    let pair = (v(&h).intrinsic)("destructure-two", vec![groups])?;
    let algo = (v(&h).get)(&pair, "0")?;
    let normal = (v(&h).get)(&pair, "1")?;
    let ops = h.clone();
    let normal_payloads = (v(&h).flat_map)(
        normal.clone(),
        Rc::new(move |o| call(&ops, "this.formatCreateOrder", || Ok(vec![o]))),
    )?;
    let ops = h.clone();
    if (v(&h).some)(
        normal_payloads.clone(),
        Rc::new(move |o| {
            Ok((v(&ops).equal)(
                &num(&ops, || (v(&ops).get)(&o, "sz"))?,
                &(v(&ops).number)(0.),
            ))
        }),
    )? {
        call(&h, "this.emitAndLogError", || {
            Ok(vec![(v(&h).literal)("Total order amount is too small")])
        })?;
        return Ok((v(&h).array)(vec![]));
    }
    let derived = call(&h, "this.deriveAlgoOrdersFromNormalOrdersOpts", || {
        Ok(vec![normal])
    })?;
    let merged = (v(&h).intrinsic)("array-spread", vec![algo, derived])?;
    let sort = (v(&h).member)(merged, "sort")?;
    let ops = h.clone();
    let sorted = (h.sort)(
        sort,
        Rc::new(move |a, b| {
            let a = call(&ops, "this.getAlgoOrderPlacementPriority", || {
                Ok(vec![(v(&ops).get)(&a, "type")?])
            })?;
            let b = call(&ops, "this.getAlgoOrderPlacementPriority", || {
                Ok(vec![(v(&ops).get)(&b, "type")?])
            })?;
            (v(&ops).intrinsic)("subtract", vec![a, b])
        }),
    )?;
    let map = (v(&h).member)(sorted, "map")?;
    let formatter = (v(&h).capture)("this.formatAlgoOrder")?;
    let algo_payloads = (h.map_bound)(map, formatter)?;
    let normal = call(&h, "this.placeOrderBatch", || Ok(vec![normal_payloads]))?;
    let normal = (v(&h).await_value)(normal).await?;
    let normal = (v(&h).intrinsic)("array-spread", vec![normal])?;
    let algo = call(&h, "this.placeAlgoOrderBatch", || Ok(vec![algo_payloads]))?;
    let algo = (v(&h).await_value)(algo).await?;
    (v(&h).intrinsic)("array-spread", vec![normal, algo])
}
pub fn get_reduce_only_trigger_order_type<V: Clone, E>(
    h: &CompositionOps<V, E>,
    o: V,
    m: V,
) -> Result<V, E> {
    let side = table(h, "ORDER_SIDE", (v(h).get)(&o, "side")?)?;
    let trigger = fallback_float(h, &o, &["triggerPx"])?;
    let reference = call(h, "this.resolveTriggerReferencePrice", || {
        Ok(vec![
            m,
            (v(h).get)(&o, "triggerPxType")?,
            fallback_float(h, &o, &["last"])?,
        ])
    })?;
    if !finite(h, trigger.clone())?
        || cmp(h, "less-or-equal", trigger.clone(), (v(h).number)(0.))?
        || !(v(h).truthy)(&reference)
    {
        return (v(h).capture)("OrderType.Conditional");
    }
    let sell = (v(h).equal)(&side, &(v(h).capture)("OrderSide.Sell")?);
    let take = cmp(
        h,
        if sell { "greater-than" } else { "less-than" },
        trigger,
        reference,
    )?;
    (v(h).capture)(if take {
        "OrderType.TakeProfit"
    } else {
        "OrderType.StopLoss"
    })
}
pub fn format_algo_order<V: Clone, E>(h: &CompositionOps<V, E>, opts: V) -> Result<V, E> {
    let m = market(h, &opts)?;
    let p_amount = precision(h, &m, "amount")?;
    let amount = adjust(h, || (v(h).get)(&opts, "amount"), p_amount)?;
    let p_price = precision(h, &m, "price")?;
    let price = if (v(h).truthy)(&(v(h).get)(&opts, "price")?) {
        adjust(h, || (v(h).get)(&opts, "price"), p_price.clone())?
    } else {
        (v(h).intrinsic)("null", vec![])?
    };
    let omit = (v(h).capture)("omitUndefined")?;
    let inst = (v(h).get)(&m, "id")?;
    let id = broker_id(h)?;
    let tag = (v(h).capture)("BROKER_ID")?;
    let side = opposite_side(h, &opts)?;
    let pos = call(h, "this.getPositionSide", || Ok(vec![opts.clone()]))?;
    let kind = (v(h).literal)(if is(h, &opts, "type", "OrderType.Conditional")? {
        "trigger"
    } else if is(h, &opts, "type", "OrderType.TrailingStopLoss")? {
        "move_order_stop"
    } else {
        "conditional"
    });
    let req = (v(h).invoke)(
        omit,
        vec![(v(h).object)(vec![
            ("instId", inst),
            ("tdMode", (v(h).literal)("cross")),
            ("algoClOrdId", id),
            ("tag", tag),
            ("side", side),
            ("posSide", pos),
            ("ordType", kind),
        ])],
    )?;
    if is(h, &opts, "type", "OrderType.Conditional")? {
        let trigger = if (v(h).truthy)(&(v(h).get)(&opts, "triggerPrice")?) {
            adjust(h, || (v(h).get)(&opts, "triggerPrice"), p_price)?
        } else {
            (v(h).number)(0.)
        };
        if !finite(h, trigger.clone())?
            || cmp(h, "less-or-equal", trigger.clone(), (v(h).number)(0.))?
        {
            return fail(
                h,
                (v(h).literal)("Trigger price is required for OKX opening conditional orders"),
            );
        }
        if !finite(h, amount.clone())?
            || cmp(h, "less-or-equal", amount.clone(), (v(h).number)(0.))?
        {
            return fail(h, (v(h).literal)("Order amount is too small"));
        }
        (v(h).set)(&req, "sz", stringify(h, amount.clone())?)?;
        (v(h).set)(&req, "triggerPx", stringify(h, trigger)?)?;
        (v(h).set)(
            &req,
            "orderPx",
            if (v(h).truthy)(&price) {
                stringify(h, price.clone())?
            } else {
                (v(h).literal)("-1")
            },
        )?;
        (v(h).set)(
            &req,
            "triggerPxType",
            call(h, "getOKXTriggerPxType", || {
                Ok(vec![(v(h).get)(&opts, "triggerType")?])
            })?,
        )?;
    }
    if is(h, &opts, "type", "OrderType.StopLoss")? || is(h, &opts, "type", "OrderType.TakeProfit")?
    {
        let unary = (v(h).intrinsic)("unary-plus", vec![amount.clone()])?;
        if !(v(h).equal)(&unary, &(v(h).number)(0.)) {
            (v(h).set)(&req, "sz", stringify(h, amount)?)?;
        } else {
            (v(h).set)(&req, "closeFraction", (v(h).literal)("1"))?;
        }
        (v(h).set)(&req, "reduceOnly", (v(h).boolean)(true))?;
    }
    if is(h, &opts, "type", "OrderType.StopLoss")? {
        (v(h).set)(&req, "slTriggerPx", stringify(h, price.clone())?)?;
        (v(h).set)(&req, "slOrdPx", (v(h).literal)("-1"))?;
        let trigger = call(h, "getOKXTriggerPxType", || {
            let t = (v(h).get)(&opts, "slTriggerType")?;
            Ok(vec![if (v(h).nullish)(&t) {
                (v(h).capture)("OrderStopTriggerType.MarkPrice")?
            } else {
                t
            }])
        })?;
        (v(h).set)(&req, "slTriggerPxType", trigger)?;
    }
    if is(h, &opts, "type", "OrderType.TakeProfit")? {
        (v(h).set)(&req, "tpTriggerPx", stringify(h, price.clone())?)?;
        (v(h).set)(&req, "tpOrdPx", (v(h).literal)("-1"))?;
        (v(h).set)(
            &req,
            "tpTriggerPxType",
            call(h, "getOKXTriggerPxType", || {
                Ok(vec![(v(h).get)(&opts, "tpTriggerType")?])
            })?,
        )?;
    }
    if is(h, &opts, "type", "OrderType.TrailingStopLoss")? {
        (v(h).set)(&req, "activePx", (v(h).literal)(""))?;
        (v(h).set)(&req, "callbackRatio", (v(h).literal)(""))?;
        (v(h).set)(&req, "callbackSpread", stringify(h, price)?)?;
        (v(h).set)(&req, "closeFraction", (v(h).literal)("1"))?;
    }
    Ok(req)
}
fn payload<V: Clone, E>(h: &CompositionOps<V, E>, req: &V, id: &V, size: V) -> Result<V, E> {
    let copy = (v(h).intrinsic)("object-spread", vec![req.clone()])?;
    define(
        h,
        &copy,
        "clOrdId",
        if (v(h).nullish)(id) {
            broker_id(h)?
        } else {
            id.clone()
        },
    )?;
    let size = method(h, size, "toFixed", || Ok(vec![]))?;
    define(h, &copy, "sz", size)?;
    Ok(copy)
}
pub fn format_create_order<V: Clone, E>(h: &CompositionOps<V, E>, opts: V) -> Result<V, E> {
    let m = market(h, &opts)?;
    let max = (v(h).get)(&(v(h).get)(&(v(h).get)(&m, "limits")?, "amount")?, "max")?;
    let p_price = precision(h, &m, "price")?;
    let p_amount = precision(h, &m, "amount")?;
    for (value, message) in [
        (
            max.clone(),
            "maximum order size must be a finite positive number",
        ),
        (
            p_amount.clone(),
            "amount precision must be a finite positive number",
        ),
    ] {
        if !finite(h, value.clone())? || cmp(h, "less-or-equal", value, (v(h).number)(0.))? {
            let text = (v(h).intrinsic)(
                "market-validation-message",
                vec![(v(h).get)(&opts, "symbol")?, (v(h).literal)(message)],
            )?;
            return fail(h, text);
        }
    }
    let amount = adjust(h, || (v(h).get)(&opts, "amount"), p_amount.clone())?;
    if !finite(h, amount.clone())? || cmp(h, "less-or-equal", amount.clone(), (v(h).number)(0.))? {
        let text = (v(h).intrinsic)(
            "amount-validation-message",
            vec![(v(h).get)(&opts, "symbol")?],
        )?;
        return fail(h, text);
    }
    let price = if (v(h).truthy)(&(v(h).get)(&opts, "price")?) {
        adjust(h, || (v(h).get)(&opts, "price"), p_price)?
    } else {
        (v(h).intrinsic)("null", vec![])?
    };
    let omit = (v(h).capture)("omitUndefined")?;
    let inst = (v(h).get)(&m, "id")?;
    let tag = (v(h).capture)("BROKER_ID")?;
    let side = opposite_side(h, &opts)?;
    let kind = if is(h, &opts, "type", "OrderType.Limit")?
        && text_is(h, &opts, "timeInForce", "PostOnly")?
    {
        (v(h).literal)("post_only")
    } else {
        table(h, "REVERSE_ORDER_TYPE", (v(h).get)(&opts, "type")?)?
    };
    let px = if is(h, &opts, "type", "OrderType.Limit")? {
        stringify(h, price)?
    } else {
        (v(h).undefined)()
    };
    let reduce = or(h, (v(h).get)(&opts, "reduceOnly")?, || {
        Ok((v(h).undefined)())
    })?;
    let pos = call(h, "this.getPositionSide", || Ok(vec![opts.clone()]))?;
    let req = (v(h).invoke)(
        omit,
        vec![(v(h).object)(vec![
            ("instId", inst),
            ("tag", tag),
            ("tdMode", (v(h).literal)("cross")),
            ("side", side),
            ("ordType", kind),
            ("px", px),
            ("reduceOnly", reduce),
            ("posSide", pos),
        ])],
    )?;
    let id = call(h, "this.resolveCallerClientOrderId", || {
        Ok(vec![opts.clone()])
    })?;
    if cmp(h, "less-or-equal", amount.clone(), max.clone())? {
        let ctor = (v(h).capture)("BigNumber")?;
        let size = (v(h).intrinsic)("construct", vec![ctor, amount])?;
        return Ok((v(h).array)(vec![payload(h, &req, &id, size)?]));
    }
    let ctor = (v(h).capture)("BigNumber")?;
    let step = (v(h).intrinsic)("construct", vec![ctor, p_amount])?;
    let ctor = (v(h).capture)("BigNumber")?;
    let max = (v(h).intrinsic)("construct", vec![ctor, max])?;
    let max = method(h, max, "dividedToIntegerBy", || Ok(vec![step.clone()]))?;
    let max = method(h, max, "multipliedBy", || Ok(vec![step]))?;
    if (v(h).truthy)(&method(h, max.clone(), "lte", || {
        Ok(vec![(v(h).number)(0.)])
    })?) {
        let text = (v(h).intrinsic)(
            "market-validation-message",
            vec![
                (v(h).get)(&opts, "symbol")?,
                (v(h).literal)("maximum order size is below its amount precision"),
            ],
        )?;
        return fail(h, text);
    }
    let payloads = (v(h).array)(vec![]);
    let ctor = (v(h).capture)("BigNumber")?;
    let mut remaining = (v(h).intrinsic)("construct", vec![ctor, amount])?;
    while (v(h).truthy)(&method(h, remaining.clone(), "gt", || {
        Ok(vec![(v(h).number)(0.)])
    })?) {
        let lot = call(h, "BigNumber.minimum", || {
            Ok(vec![remaining.clone(), max.clone()])
        })?;
        method(h, payloads.clone(), "push", || {
            Ok(vec![payload(h, &req, &id, lot.clone())?])
        })?;
        remaining = method(h, remaining, "minus", || Ok(vec![lot]))?;
    }
    call(h, "this.assertSingleNativeClientOrderId", || {
        Ok(vec![id, (v(h).get)(&payloads, "length")?])
    })?;
    Ok(payloads)
}
pub fn map_orders<V: Clone + 'static, E: 'static>(
    h: Rc<CompositionOps<V, E>>,
    rows: V,
) -> Result<V, E> {
    let ops = h.clone();
    (v(&h).reduce)(
        rows,
        (v(&h).array)(vec![]),
        Rc::new(move |acc, o| {
            let h = &ops;
            let markets = (v(h).capture)("this.store.marketsById")?;
            let m = index(h, markets, (v(h).get)(&o, "instId")?)?;
            if !(v(h).truthy)(&m) {
                return Ok(acc);
            }
            if text_is(h, &o, "ordType", "trigger")? {
                let amount = fallback_float(h, &o, &["sz"])?;
                let size = call(h, "Math.abs", || Ok(vec![amount]))?;
                let price = fallback_float(h, &o, &["triggerPx"])?;
                let limit = fallback_float(h, &o, &["ordPx", "orderPx"])?;
                let reference =
                    fallback_float(h, &o, &["last", "px", "ordPx", "orderPx", "triggerPx"])?;
                let base = call(h, "this.contractsToBase", || {
                    Ok(vec![size.clone(), m.clone(), reference])
                })?;
                let reduce = text_is(h, &o, "reduceOnly", "true")?
                    || (v(h).equal)(&(v(h).get)(&o, "reduceOnly")?, &(v(h).boolean)(true));
                let kind = if reduce {
                    call(h, "this.getReduceOnlyTriggerOrderType", || {
                        Ok(vec![o.clone(), m.clone()])
                    })?
                } else {
                    (v(h).capture)("OrderType.Conditional")?
                };
                let trigger = call(h, "parseOKXTriggerPxType", || {
                    Ok(vec![(v(h).get)(&o, "triggerPxType")?])
                })?;
                let stop = (v(h).equal)(&kind, &(v(h).capture)("OrderType.StopLoss")?);
                let take = if stop {
                    false
                } else {
                    (v(h).equal)(&kind, &(v(h).capture)("OrderType.TakeProfit")?)
                };
                let suffix = if stop {
                    "_sl"
                } else if take {
                    "_tp"
                } else {
                    ""
                };
                let trigger_key = if stop {
                    "slTriggerType"
                } else if take {
                    "tpTriggerType"
                } else {
                    "triggerType"
                };
                let acc = (v(h).intrinsic)("array-spread", vec![acc])?;
                let id = (v(h).intrinsic)(
                    "template-suffix",
                    vec![(v(h).get)(&o, "algoId")?, (v(h).literal)(suffix)],
                )?;
                let status = (v(h).capture)("OrderStatus.Open")?;
                let symbol = (v(h).get)(&m, "symbol")?;
                let side = table(h, "ORDER_SIDE", (v(h).get)(&o, "side")?)?;
                let conditional = if finite(h, limit.clone())?
                    && cmp(h, "greater-than", limit.clone(), (v(h).number)(0.))?
                {
                    limit
                } else {
                    (v(h).undefined)()
                };
                let extra = (v(h).object)(vec![
                    ("remainingNative", size.clone()),
                    ("conditionalOrderPrice", conditional),
                ]);
                let now = call(h, "virtualClock.getCurrentTimeMs", || Ok(vec![]))?;
                let record = (v(h).object)(vec![
                    ("id", id),
                    ("status", status),
                    ("symbol", symbol),
                    ("type", kind),
                    ("side", side),
                    ("price", price),
                    ("amount", size),
                    ("filled", (v(h).number)(0.)),
                    ("remaining", base),
                    ("extra", extra),
                    ("reduceOnly", (v(h).boolean)(reduce)),
                    ("lastUpdate", now),
                    (trigger_key, trigger),
                    (
                        "timeInForce",
                        (v(h).capture)("OrderTimeInForce.GoodTillCancel")?,
                    ),
                    ("info", o),
                ]);
                return append(h, acc, record);
            }
            if text_is(h, &o, "ordType", "oco")? || text_is(h, &o, "ordType", "conditional")? {
                let amount = fallback_float(h, &o, &["sz"])?;
                let size = call(h, "Math.abs", || Ok(vec![amount]))?;
                let reference =
                    fallback_float(h, &o, &["last", "px", "tpTriggerPx", "slTriggerPx"])?;
                let base = call(h, "this.contractsToBase", || {
                    Ok(vec![size.clone(), m.clone(), reference])
                })?;
                let out = (v(h).array)(vec![]);
                for (prefix, suffix, kind, trigger_field) in [
                    ("sl", "_sl", "OrderType.StopLoss", "slTriggerType"),
                    ("tp", "_tp", "OrderType.TakeProfit", "tpTriggerType"),
                ] {
                    let price_key = if prefix == "sl" {
                        "slTriggerPx"
                    } else {
                        "tpTriggerPx"
                    };
                    let type_key = if prefix == "sl" {
                        "slTriggerPxType"
                    } else {
                        "tpTriggerPxType"
                    };
                    if (v(h).truthy)(&num(h, || (v(h).get)(&o, price_key))?) {
                        let push = (v(h).member)(out.clone(), "push")?;
                        let id = (v(h).intrinsic)(
                            "template-suffix",
                            vec![(v(h).get)(&o, "algoId")?, (v(h).literal)(suffix)],
                        )?;
                        let status = (v(h).capture)("OrderStatus.Open")?;
                        let symbol = (v(h).get)(&m, "symbol")?;
                        let kind = (v(h).capture)(kind)?;
                        let side = table(h, "ORDER_SIDE", (v(h).get)(&o, "side")?)?;
                        let price = num(h, || (v(h).get)(&o, price_key))?;
                        let extra = (v(h).object)(vec![("remainingNative", size.clone())]);
                        let reduce = (v(h).intrinsic)(
                            "loose-equal",
                            vec![(v(h).get)(&o, "reduceOnly")?, (v(h).literal)("true")],
                        )?;
                        let now = call(h, "virtualClock.getCurrentTimeMs", || Ok(vec![]))?;
                        let trigger = call(h, "parseOKXTriggerPxType", || {
                            Ok(vec![(v(h).get)(&o, type_key)?])
                        })?;
                        let record = (v(h).object)(vec![
                            ("id", id),
                            ("status", status),
                            ("symbol", symbol),
                            ("type", kind),
                            ("side", side),
                            ("price", price),
                            ("amount", size.clone()),
                            ("filled", (v(h).number)(0.)),
                            ("remaining", base.clone()),
                            ("extra", extra),
                            ("reduceOnly", reduce),
                            ("lastUpdate", now),
                            (trigger_field, trigger),
                            (
                                "timeInForce",
                                (v(h).capture)("OrderTimeInForce.GoodTillCancel")?,
                            ),
                            ("info", o.clone()),
                        ]);
                        (v(h).invoke)(push, vec![record])?;
                    }
                }
                return (v(h).intrinsic)("array-spread", vec![acc, out]);
            }
            if text_is(h, &o, "ordType", "move_order_stop")? {
                let side = table(h, "ORDER_SIDE", (v(h).get)(&o, "side")?)?;
                let positions = (v(h).capture)("this.store.positions")?;
                let ops = h.clone();
                let saved_m = m.clone();
                let saved_side = side.clone();
                let existing = (h.order.find)(
                    positions,
                    Rc::new(move |pos| {
                        let same = (v(&ops).equal)(
                            &(v(&ops).get)(&pos, "symbol")?,
                            &(v(&ops).get)(&saved_m, "symbol")?,
                        );
                        let matches = if same {
                            let sell =
                                (v(&ops).equal)(&saved_side, &(v(&ops).capture)("OrderSide.Sell")?);
                            (v(&ops).equal)(
                                &(v(&ops).get)(&pos, "side")?,
                                &(v(&ops).capture)(if sell {
                                    "PositionSide.Long"
                                } else {
                                    "PositionSide.Short"
                                })?,
                            )
                        } else {
                            false
                        };
                        Ok((v(&ops).boolean)(matches))
                    }),
                )?;
                let spread = num(h, || (v(h).get)(&o, "callbackSpread"))?;
                let movement = if (v(h).truthy)(&existing) {
                    call(h, "subtract", || {
                        let min = call(h, "Math.min", || {
                            Ok(vec![(v(h).get)(&existing, "entryPrice")?, spread.clone()])
                        })?;
                        let max = call(h, "Math.max", || {
                            Ok(vec![(v(h).get)(&existing, "entryPrice")?, spread.clone()])
                        })?;
                        Ok(vec![min, max])
                    })?
                } else {
                    num(h, || (v(h).get)(&o, "moveTriggerPx"))?
                };
                let price = call(h, "adjust", || {
                    let buy = (v(h).equal)(&side, &(v(h).capture)("OrderSide.Buy")?);
                    let price = call(h, if buy { "subtract" } else { "add" }, || {
                        Ok(vec![num(h, || (v(h).get)(&o, "last"))?, movement])
                    })?;
                    Ok(vec![price, precision(h, &m, "price")?])
                })?;
                let acc = (v(h).intrinsic)("array-spread", vec![acc])?;
                let id = (v(h).intrinsic)(
                    "template-suffix",
                    vec![(v(h).get)(&o, "algoId")?, (v(h).literal)("_tsl")],
                )?;
                let status = (v(h).capture)("OrderStatus.Open")?;
                let symbol = (v(h).get)(&m, "symbol")?;
                let kind = (v(h).capture)("OrderType.TrailingStopLoss")?;
                let side = table(h, "ORDER_SIDE", (v(h).get)(&o, "side")?)?;
                let effective_price = or(h, price, || Ok(spread))?;
                let now = call(h, "virtualClock.getCurrentTimeMs", || Ok(vec![]))?;
                let record = (v(h).object)(vec![
                    ("id", id),
                    ("status", status),
                    ("symbol", symbol),
                    ("type", kind),
                    ("side", side),
                    ("price", effective_price),
                    ("amount", (v(h).number)(0.)),
                    ("filled", (v(h).number)(0.)),
                    ("remaining", (v(h).number)(0.)),
                    (
                        "extra",
                        (v(h).object)(vec![("remainingNative", (v(h).number)(0.))]),
                    ),
                    ("reduceOnly", (v(h).boolean)(true)),
                    ("lastUpdate", now),
                    (
                        "timeInForce",
                        (v(h).capture)("OrderTimeInForce.GoodTillCancel")?,
                    ),
                ]);
                return append(h, acc, record);
            }
            let amount = num(h, || (v(h).get)(&o, "sz"))?;
            let filled = num(h, || (v(h).get)(&o, "accFillSz"))?;
            let remaining_native =
                call(h, "subtract", || Ok(vec![amount.clone(), filled.clone()]))?;
            let reference = fallback_float(h, &o, &["px", "avgPx", "fillPx", "last"])?;
            let remaining = call(h, "this.contractsToBase", || {
                Ok(vec![remaining_native.clone(), m.clone(), reference.clone()])
            })?;
            let filled_base = call(h, "this.contractsToBase", || {
                Ok(vec![filled.clone(), m.clone(), reference])
            })?;
            let id = (v(h).get)(&o, "ordId")?;
            let status = table(h, "ORDER_STATUS", (v(h).get)(&o, "state")?)?;
            let symbol = (v(h).get)(&m, "symbol")?;
            let kind = table(h, "ORDER_TYPE", (v(h).get)(&o, "ordType")?)?;
            let side = table(h, "ORDER_SIDE", (v(h).get)(&o, "side")?)?;
            let price = fallback_float(h, &o, &["px"])?;
            let extra = (v(h).object)(vec![
                ("remainingNative", remaining_native),
                ("filledInPlacementUnits", filled),
            ]);
            let reduce = (v(h).boolean)(text_is(h, &o, "reduceOnly", "true")?);
            let now = call(h, "virtualClock.getCurrentTimeMs", || Ok(vec![]))?;
            let tif = (v(h).capture)(if text_is(h, &o, "ordType", "post_only")? {
                "OrderTimeInForce.PostOnly"
            } else {
                "OrderTimeInForce.GoodTillCancel"
            })?;
            let record = (v(h).object)(vec![
                ("id", id),
                ("status", status),
                ("symbol", symbol),
                ("type", kind),
                ("side", side),
                ("price", price),
                ("amount", amount),
                ("filled", filled_base),
                ("remaining", remaining),
                ("extra", extra),
                ("reduceOnly", reduce),
                ("lastUpdate", now),
                ("timeInForce", tif),
                ("info", o),
            ]);
            append(h, acc, record)
        }),
    )
}
pub fn map_positions<V: Clone + 'static, E: 'static>(
    h: Rc<CompositionOps<V, E>>,
    rows: V,
) -> Result<V, E> {
    if !(v(&h).truthy)(&(v(&h).capture)("this._accountPositionMode")?) {
        return Err(fresh_error(
            &h,
            (v(&h).literal)("Position mode not loaded"),
        )?);
    }
    let ops = h.clone();
    (v(&h).reduce)(
        rows,
        (v(&h).array)(vec![]),
        Rc::new(move |acc, p| {
            let h = &ops;
            let markets = (v(h).capture)("this.store.marketsById")?;
            let m = index(h, markets, (v(h).get)(&p, "instId")?)?;
            let tickers = (v(h).capture)("this.store.tickersById")?;
            let ticker = index(h, tickers, (v(h).get)(&p, "instId")?)?;
            if !(v(h).truthy)(&m) {
                return Ok(acc);
            }
            let pos = num(h, || (v(h).get)(&p, "pos"))?;
            let contracts = or(h, pos.clone(), || Ok((v(h).number)(0.)))?;
            let side = (v(h).capture)(if text_is(h, &p, "posSide", "short")? {
                "PositionSide.Short"
            } else if text_is(h, &p, "posSide", "long")? {
                "PositionSide.Long"
            } else if cmp(h, "less-than", pos, (v(h).number)(0.))? {
                "PositionSide.Short"
            } else {
                "PositionSide.Long"
            })?;
            let mark = or(h, optional(h, &ticker, "mark")?, || Ok((v(h).number)(0.)))?;
            let native = call(h, "Math.abs", || Ok(vec![contracts.clone()]))?;
            let reference = or(h, mark.clone(), || fallback_float(h, &p, &["avgPx"]))?;
            let notional = call(h, "this.contractsToNotional", || {
                Ok(vec![native.clone(), m.clone(), mark.clone()])
            })?;
            let base = call(h, "this.contractsToBase", || {
                Ok(vec![native.clone(), m.clone(), reference])
            })?;
            let raw = call(h, "this.toFiniteNumber", || {
                Ok(vec![(v(h).get)(&p, "upl")?])
            })?;
            let pnl = call(h, "this.convertMarginValueToUsd", || {
                Ok(vec![raw, (v(h).get)(&p, "ccy")?, m.clone(), mark])
            })?;
            let id = call(h, "createPositionId", || {
                Ok(vec![
                    (v(h).get)(&m, "symbol")?,
                    side.clone(),
                    (v(h).capture)("this._accountPositionMode")?,
                ])
            })?;
            let positions = (v(h).capture)("this.store.positions")?;
            let ops = h.clone();
            let existing = (h.order.find)(
                positions,
                Rc::new(move |pos| {
                    Ok((v(&ops).boolean)((v(&ops).equal)(
                        &(v(&ops).get)(&pos, "id")?,
                        &id,
                    )))
                }),
            )?;
            let id = call(h, "createPositionId", || {
                Ok(vec![
                    (v(h).get)(&m, "symbol")?,
                    side.clone(),
                    (v(h).capture)("this._accountPositionMode")?,
                ])
            })?;
            let symbol = (v(h).get)(&m, "symbol")?;
            let entry = if (v(h).truthy)(&contracts) {
                num(h, || (v(h).get)(&p, "avgPx"))?
            } else {
                (v(h).number)(0.)
            };
            let leverage = num(h, || {
                or(h, (v(h).get)(&p, "lever")?, || {
                    or(h, optional(h, &existing, "leverage")?, || {
                        Ok((v(h).literal)("0"))
                    })
                })
            })?;
            let pnl = if (v(h).truthy)(&contracts) {
                call(h, "adjust", || Ok(vec![pnl, precision(h, &m, "price")?]))?
            } else {
                (v(h).number)(0.)
            };
            let liq = fallback_float(h, &p, &["liqPx"])?;
            let margin = (v(h).capture)(if text_is(h, &p, "mgnMode", "cross")? {
                "MarginType.Cross"
            } else {
                "MarginType.Isolated"
            })?;
            let mode = or(h, (v(h).capture)("this._accountPositionMode")?, || {
                (v(h).capture)("PositionMode.OneWay")
            })?;
            let record = (v(h).object)(vec![
                ("id", id),
                ("symbol", symbol),
                ("side", side),
                ("entryPrice", entry),
                ("notional", notional),
                ("leverage", leverage),
                ("unrealizedPnl", pnl),
                ("contracts", base),
                ("liquidationPrice", liq),
                ("marginType", margin),
                ("positionMode", mode),
                ("extra", (v(h).object)(vec![("contractsNative", native)])),
                ("info", p),
            ]);
            append(h, acc, record)
        }),
    )
}
fn milliseconds<V: Clone, E>(h: &CompositionOps<V, E>, x: V) -> Result<V, E> {
    if (v(h).nullish)(&x) || !finite(h, x.clone())? {
        return Ok((v(h).undefined)());
    }
    call(h, "Math.floor", || {
        let value = if cmp(
            h,
            "greater-than",
            x.clone(),
            (v(h).number)(1_000_000_000_000.),
        )? {
            x
        } else {
            (v(h).intrinsic)("multiply", vec![x, (v(h).number)(1000.)])?
        };
        Ok(vec![value])
    })
}
fn seconds<V: Clone, E>(h: &CompositionOps<V, E>, x: V) -> Result<V, E> {
    let parsed = call(h, "Number.parseInt", || {
        let string = call(h, "String", || {
            Ok(vec![if (v(h).nullish)(&x) {
                (v(h).literal)("0")
            } else {
                x
            }])
        })?;
        Ok(vec![string, (v(h).number)(10.)])
    })?;
    if !finite(h, parsed.clone())? || cmp(h, "less-or-equal", parsed.clone(), (v(h).number)(0.))? {
        return Ok((v(h).undefined)());
    }
    if cmp(
        h,
        "greater-than",
        parsed.clone(),
        (v(h).number)(1_000_000_000_000.),
    )? {
        call(h, "Math.floor", || {
            Ok(vec![(v(h).intrinsic)(
                "divide",
                vec![parsed, (v(h).number)(1000.)],
            )?])
        })
    } else {
        Ok(parsed)
    }
}
fn string<V, E>(h: &CompositionOps<V, E>, x: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    call(h, "String", || Ok(vec![x()?]))
}
fn fill_rows<V: Clone + 'static, E: 'static>(
    h: Rc<CompositionOps<V, E>>,
    rows: V,
    m: V,
    order_id: V,
) -> Result<V, E> {
    let ops = h.clone();
    (v(&h).reduce)(
        rows,
        (v(&h).array)(vec![]),
        Rc::new(move |acc, row| {
            let h = &ops;
            let sides = (v(h).capture)("ORDER_SIDE")?;
            let side = string(h, || {
                or(h, (v(h).get)(&row, "side")?, || Ok((v(h).literal)("")))
            })?;
            let side = method(h, side, "toLowerCase", || Ok(vec![]))?;
            let side = index(h, sides, side)?;
            if !(v(h).truthy)(&side) {
                return Ok(acc);
            }
            let price = fallback_float(h, &row, &["fillPx", "px", "avgPx"])?;
            let amount = fallback_float(h, &row, &["fillSz", "sz", "accFillSz"])?;
            if !finite(h, price.clone())?
                || !finite(h, amount.clone())?
                || cmp(h, "less-or-equal", price.clone(), (v(h).number)(0.))?
                || cmp(h, "less-or-equal", amount.clone(), (v(h).number)(0.))?
            {
                return Ok(acc);
            }
            let mut time = (v(h).undefined)();
            for key in ["fillTime", "ts", "uTime", "cTime"] {
                let x = (v(h).get)(&row, key)?;
                time = x;
                if (v(h).truthy)(&time) {
                    break;
                }
            }
            let time = seconds(h, time)?;
            if !(v(h).truthy)(&time) {
                return Ok(acc);
            }
            let markets = (v(h).capture)("this.store.marketsById")?;
            let market = index(h, markets, (v(h).get)(&row, "instId")?)?;
            let symbol = or(h, optional(h, &market, "symbol")?, || {
                (v(h).get)(&m, "symbol")
            })?;
            if !(v(h).equal)(&symbol, &(v(h).get)(&m, "symbol")?) {
                return Ok(acc);
            }
            let qty = call(h, "this.contractsToBase", || {
                Ok(vec![amount, m.clone(), price.clone()])
            })?;
            if !finite(h, qty.clone())? || cmp(h, "less-or-equal", qty.clone(), (v(h).number)(0.))?
            {
                return Ok(acc);
            }
            let id = string(h, || {
                or(h, (v(h).get)(&row, "ordId")?, || {
                    or(h, (v(h).get)(&row, "orderId")?, || Ok((v(h).literal)("")))
                })
            })?;
            if (v(h).truthy)(&order_id)
                && (v(h).truthy)(&id)
                && !(v(h).equal)(&id, &string(h, || Ok(order_id.clone()))?)
            {
                return Ok(acc);
            }
            let fee = fallback_float(h, &row, &["fee", "fillFee"])?;
            let commission = if finite(h, fee.clone())? {
                call(h, "Math.abs", || Ok(vec![fee]))?
            } else {
                (v(h).undefined)()
            };
            let pnl = num(h, || {
                let x = (v(h).get)(&row, "fillPnl")?;
                Ok(if (v(h).nullish)(&x) {
                    (v(h).literal)("")
                } else {
                    x
                })
            })?;
            let omit = (v(h).capture)("omitUndefined")?;
            let execution_id = if (v(h).truthy)(&(v(h).get)(&row, "tradeId")?) {
                string(h, || (v(h).get)(&row, "tradeId"))?
            } else if (v(h).truthy)(&(v(h).get)(&row, "billId")?) {
                string(h, || (v(h).get)(&row, "billId"))?
            } else {
                (v(h).undefined)()
            };
            let order = or(h, id, || Ok((v(h).undefined)()))?;
            let asset = or(h, (v(h).get)(&row, "feeCcy")?, || {
                or(h, (v(h).get)(&row, "ccy")?, || Ok((v(h).undefined)()))
            })?;
            let pos_side = if (v(h).truthy)(&(v(h).get)(&row, "posSide")?) {
                call(h, "normalizeExecutionPositionSide", || {
                    Ok(vec![(v(h).get)(&row, "posSide")?])
                })?
            } else {
                (v(h).undefined)()
            };
            let realized = if finite(h, pnl.clone())? {
                pnl
            } else {
                (v(h).undefined)()
            };
            let maker = if (v(h).truthy)(&(v(h).get)(&row, "execType")?) {
                let value = string(h, || (v(h).get)(&row, "execType"))?;
                let value = method(h, value, "toUpperCase", || Ok(vec![]))?;
                (v(h).boolean)((v(h).equal)(&value, &(v(h).literal)("M")))
            } else {
                (v(h).undefined)()
            };
            let record = (v(h).object)(vec![
                ("id", execution_id),
                ("timestamp", time),
                ("symbol", symbol),
                ("orderId", order),
                ("price", price),
                ("qty", qty),
                ("side", side),
                ("commission", commission),
                ("commissionAsset", asset),
                ("positionSide", pos_side),
                ("realizedPnl", realized),
                ("maker", maker),
            ]);
            let record = (v(h).invoke)(omit, vec![record])?;
            method(h, acc.clone(), "push", || Ok(vec![record]))?;
            Ok(acc)
        }),
    )
}
async fn fetch_fill_endpoint<V: Clone + 'static, E: 'static>(
    h: Rc<CompositionOps<V, E>>,
    endpoint: V,
    m: V,
    id: V,
    from: V,
    to: V,
    limit: V,
) -> Result<V, E> {
    let omit = (v(&h).capture)("omitUndefined")?;
    let kind = call(&h, "this.getMarketInstrumentType", || Ok(vec![m.clone()]))?;
    let inst = (v(&h).get)(&m, "id")?;
    let params = (v(&h).invoke)(
        omit,
        vec![(v(&h).object)(vec![
            ("instType", kind),
            ("instId", inst),
            ("ordId", id),
            ("begin", from),
            ("end", to),
            ("limit", limit),
        ])],
    )?;
    let response = call(&h, "this.xhr.get", || {
        Ok(vec![endpoint, (v(&h).object)(vec![("params", params)])])
    })?;
    let response = (v(&h).await_value)(response).await?;
    let data = (v(&h).get)(&(v(&h).get)(&response, "data")?, "data")?;
    if (v(&h).truthy)(&call(&h, "Array.isArray", || Ok(vec![data.clone()]))?) {
        Ok(data)
    } else {
        Ok((v(&h).array)(vec![]))
    }
}
pub async fn fetch_historical_fills<V: Clone + 'static, E: 'static>(
    h: Rc<CompositionOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    if (v(&h).truthy)(&(v(&h).capture)("this.isPublicDataOnly")?) {
        return Ok((v(&h).array)(vec![]));
    }
    let symbol = (v(&h).get)(&opts, "symbol")?;
    let id = (v(&h).get)(&opts, "orderId")?;
    let from = (v(&h).get)(&opts, "from")?;
    let to = (v(&h).get)(&opts, "to")?;
    let limit = (v(&h).get)(&opts, "limit")?;
    let limit = if (v(&h).equal)(&limit, &(v(&h).undefined)()) {
        (v(&h).number)(100.)
    } else {
        limit
    };
    if !(v(&h).truthy)(&symbol) {
        return Ok((v(&h).array)(vec![]));
    }
    let markets = (v(&h).capture)("this.store.marketsBySymbol")?;
    let m = index(&h, markets, symbol)?;
    if !(v(&h).truthy)(&m) {
        return Ok((v(&h).array)(vec![]));
    }
    let from = milliseconds(&h, from)?;
    let to = milliseconds(&h, to)?;
    let limit = call(&h, "Math.min", || {
        Ok(vec![
            call(&h, "Math.max", || Ok(vec![limit, (v(&h).number)(1.)]))?,
            (v(&h).number)(100.),
        ])
    })?;
    let result = async {
        let endpoint = (v(&h).capture)("ENDPOINTS.FILLS")?;
        let recent = fetch_fill_endpoint(
            h.clone(),
            endpoint,
            m.clone(),
            id.clone(),
            from.clone(),
            to.clone(),
            limit.clone(),
        )
        .await?;
        let history = if cmp(
            &h,
            "greater-or-equal",
            (v(&h).get)(&recent, "length")?,
            limit.clone(),
        )? {
            (v(&h).array)(vec![])
        } else {
            let endpoint = (v(&h).capture)("ENDPOINTS.FILLS_HISTORY")?;
            fetch_fill_endpoint(
                h.clone(),
                endpoint,
                m.clone(),
                id.clone(),
                from.clone(),
                to.clone(),
                limit.clone(),
            )
            .await?
        };
        let ctor = (v(&h).capture)("Map")?;
        let dedupe = (v(&h).intrinsic)("construct", vec![ctor])?;
        let recent = fill_rows(h.clone(), recent, m.clone(), id.clone())?;
        let recent = (v(&h).intrinsic)("array-spread", vec![recent])?;
        let history = fill_rows(h.clone(), history, m, id)?;
        let executions = (v(&h).intrinsic)("array-spread", vec![recent, history])?;
        let ops = h.clone();
        let map = dedupe.clone();
        (v(&h).for_of)(
            executions,
            Rc::new(move |execution| {
                if (v(&ops).truthy)(&from) {
                    let time = (v(&ops).get)(&execution, "timestamp")?;
                    let ms = (v(&ops).intrinsic)("multiply", vec![time, (v(&ops).number)(1000.)])?;
                    if cmp(&ops, "less-than", ms, from.clone())? {
                        return Ok(());
                    }
                }
                if (v(&ops).truthy)(&to) {
                    let time = (v(&ops).get)(&execution, "timestamp")?;
                    let ms = (v(&ops).intrinsic)("multiply", vec![time, (v(&ops).number)(1000.)])?;
                    if cmp(&ops, "greater-than", ms, to.clone())? {
                        return Ok(());
                    }
                }
                let key = if (v(&ops).truthy)(&(v(&ops).get)(&execution, "id")?) {
                    (v(&ops).intrinsic)("id-prefix", vec![(v(&ops).get)(&execution, "id")?])?
                } else {
                    let order = or(&ops, (v(&ops).get)(&execution, "orderId")?, || {
                        Ok((v(&ops).literal)("no-order"))
                    })?;
                    let mut key = stringify(&ops, order)?;
                    for field in ["timestamp", "price", "qty", "side"] {
                        key = (v(&ops).intrinsic)(
                            "colon-append",
                            vec![key, (v(&ops).get)(&execution, field)?],
                        )?;
                    }
                    key
                };
                method(&ops, map.clone(), "set", || Ok(vec![key, execution]))?;
                Ok(())
            }),
        )?;
        let values = method(&h, dedupe, "values", || Ok(vec![]))?;
        let array = (v(&h).intrinsic)("array-spread", vec![values])?;
        let sort = (v(&h).member)(array, "sort")?;
        let ops = h.clone();
        let sorted = (h.sort)(
            sort,
            Rc::new(move |a, b| {
                (v(&ops).intrinsic)(
                    "subtract",
                    vec![
                        (v(&ops).get)(&b, "timestamp")?,
                        (v(&ops).get)(&a, "timestamp")?,
                    ],
                )
            }),
        )?;
        method(&h, sorted, "slice", || Ok(vec![(v(&h).number)(0.), limit]))
    }
    .await;
    match result {
        Ok(x) => Ok(x),
        Err(e) => {
            let emit = (v(&h).capture)("this.emitAndLogError")?;
            let error = (v(&h).error_value)(&e);
            let response = optional(&h, &error, "response")?;
            let data = optional(&h, &response, "data")?;
            let msg = or(&h, optional(&h, &data, "msg")?, || {
                optional(&h, &error, "message")
            })?;
            (v(&h).invoke)(emit, vec![msg])?;
            Ok((v(&h).array)(vec![]))
        }
    }
}
