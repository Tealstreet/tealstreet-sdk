//! Order preparation and replacement bodies over supplied native operations.
//! The host retains callable owners, native aliases/intrinsics and started work.
//! Cloning a host retains the same receiver and runtime; it never clones their state.
use crate::{order_placement::OrderPlacementHost, rest_actions::RestActionFuture};
pub trait OrderPreparationHost:
    OrderPlacementHost<Value: 'static, Error: 'static> + Clone + 'static
{
    /// Native imported times, including its count coercion/limits and callback order.
    fn times<F>(
        &self,
        method: Self::Value,
        count: Self::Value,
        callback: F,
    ) -> Result<Self::Value, Self::Error>
    where
        F: FnMut() -> Result<Self::Value, Self::Error>;
    /// Start an async-function body immediately to its first suspension and retain
    /// its native promise independently of the awaiting caller. Errors reject it.
    fn launch(
        &self,
        body: RestActionFuture<'static, Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn await_value(&self, promise: Self::Value) -> RestActionFuture<'_, Self::Value, Self::Error>;
}
fn s<H: OrderPreparationHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn p<H: OrderPreparationHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.property(v, k)
}
fn opt<H: OrderPreparationHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        p(h, v, k)
    }
}
fn op<H: OrderPreparationHost>(
    h: &H,
    name: &str,
    a: H::Value,
    b: H::Value,
) -> Result<H::Value, H::Error> {
    h.operation(name, vec![a, b])
}
fn greater<H: OrderPreparationHost>(h: &H, a: H::Value, b: H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&op(h, "greater", a, b)?))
}
fn call<H: OrderPreparationHost>(
    h: &H,
    path: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let f = h.capture(path)?;
    h.invoke(f, args)
}
fn template<H: OrderPreparationHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    h.operation("template", vec![v])
}
fn spread<H: OrderPreparationHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    h.operation("objectSpread", vec![v])
}
fn adjust<H: OrderPreparationHost>(
    h: &H,
    value: impl FnOnce() -> Result<H::Value, H::Error>,
    precision: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.capture("adjust")?;
    let v = value()?;
    h.invoke(f, vec![v, precision()?])
}
fn precision<H: OrderPreparationHost>(
    h: &H,
    market: &H::Value,
    k: &str,
) -> Result<H::Value, H::Error> {
    p(h, &p(h, market, "precision")?, k)
}
fn missing<H: OrderPreparationHost>(
    h: &H,
    kind: &str,
    symbol: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Error, H::Error> {
    let f = h.capture("Error")?;
    let msg = h.operation("missing", vec![s(h, kind), symbol()?])?;
    h.new_error(f, msg)
}
fn fail<H: OrderPreparationHost>(h: &H, msg: H::Value) -> Result<H::Error, H::Error> {
    call(h, "emitAndLogError", vec![msg.clone()])?;
    let f = h.capture("Error")?;
    h.new_error(f, msg)
}
async fn await_call<H: OrderPreparationHost>(
    h: &H,
    name: &str,
    args: impl FnOnce() -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.capture(name)?;
    h.start(f, args()?)?.await
}
fn inverse<H: OrderPreparationHost>(
    h: &H,
    constant: &str,
    key: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.capture("inverseObj")?;
    let obj = h.invoke(f, vec![h.receiver(constant)?])?;
    h.key_get(&obj, &key()?)
}
fn tif<H: OrderPreparationHost>(h: &H, opts: &H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&p(h, opts, "timeInForce")?) {
        inverse(h, "TIME_IN_FORCE", || p(h, opts, "timeInForce"))
    } else {
        inverse(h, "TIME_IN_FORCE", || {
            h.receiver("OrderTimeInForce.GoodTillCancel")
        })
    }
}
fn optional_adjust<H: OrderPreparationHost>(
    h: &H,
    opts: &H::Value,
    key: &str,
    pp: &H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&p(h, opts, key)?) {
        template(h, adjust(h, || p(h, opts, key), || Ok(pp.clone()))?)
    } else {
        Ok(h.undefined())
    }
}
fn trigger<H: OrderPreparationHost>(
    h: &H,
    opts: &H::Value,
    key: &str,
) -> Result<H::Value, H::Error> {
    let f = h.capture("getUtaStrategyTriggerBy")?;
    h.invoke(f, vec![p(h, opts, key)?])
}
async fn normal<H: OrderPreparationHost>(
    h: &H,
    opts: H::Value,
    uta: bool,
) -> Result<H::Value, H::Error> {
    if !uta {
        await_call(h, "ensureAccountTypeDetected", || Ok(vec![])).await?;
        if h.truthy(&h.receiver("usesUtaRouting")?) {
            return await_call(h, "formatUtaCreateOrder", || Ok(vec![opts])).await;
        }
    } else if !h.equal(&p(h, &opts, "type")?, &h.receiver("OrderType.Limit")?)
        && !h.equal(&p(h, &opts, "type")?, &h.receiver("OrderType.Market")?)
    {
        return Err(fail(
            h,
            s(
                h,
                "Bitget UTA currently supports only normal market and limit orders.",
            ),
        )?);
    }
    let markets = h.receiver("store.marketsBySymbol")?;
    let market = h.key_get(&markets, &p(h, &opts, "symbol")?)?;
    if !h.truthy(&market) {
        return Err(missing(h, "Market", || p(h, &opts, "symbol"))?);
    }
    let mode = await_call(h, "getOrFetchPositionMode", || {
        Ok(vec![p(h, &opts, "symbol")?])
    })
    .await?;
    let margin = if uta {
        Some(
            await_call(h, "getOrFetchMarginType", || {
                Ok(vec![p(h, &opts, "symbol")?])
            })
            .await?,
        )
    } else {
        None
    };
    let side_fn = h.capture(if uta {
        "getOrderSideUta"
    } else {
        "getOrderSideV2"
    })?;
    let sides = h.invoke(side_fn, vec![opts.clone(), mode.clone()])?;
    let (side, other) = if uta {
        (p(h, &sides, "side")?, p(h, &sides, "posSide")?)
    } else {
        h.pair(sides)?
    };
    let max = p(h, &p(h, &p(h, &market, "limits")?, "amount")?, "max")?;
    let pp = precision(h, &market, "price")?;
    let pa = precision(h, &market, "amount")?;
    let raw_price = p(h, &opts, "price")?;
    let price = if h.truthy(&raw_price)
        && !h.equal(&p(h, &opts, "type")?, &h.receiver("OrderType.Market")?)
    {
        adjust(h, || p(h, &opts, "price"), || Ok(pp.clone()))?
    } else {
        h.undefined()
    };
    let time = tif(h, &opts)?;
    let amount = adjust(h, || p(h, &opts, "amount"), || Ok(pa.clone()))?;
    let omit = h.capture("omitUndefined")?;
    let mut fields = vec![];
    if uta {
        fields.push(("category", h.receiver("apiUtaCategory")?));
    }
    fields.push(("symbol", p(h, &market, if uta { "id" } else { "symbol" })?));
    fields.push((
        if uta { "qty" } else { "size" },
        if h.truthy(&amount) {
            template(h, amount.clone())?
        } else {
            h.undefined()
        },
    ));
    fields.push((
        "price",
        if h.truthy(&price) {
            template(h, price)?
        } else {
            h.undefined()
        },
    ));
    fields.push(("side", side));
    fields.push((if uta { "posSide" } else { "tradeSide" }, other));
    fields.push((
        "orderType",
        inverse(h, "ORDER_TYPE", || p(h, &opts, "type"))?,
    ));
    if uta {
        fields.push((
            "timeInForce",
            if !h.equal(&p(h, &opts, "type")?, &h.receiver("OrderType.Market")?) {
                if h.equal(&time, &s(h, "normal")) {
                    s(h, "gtc")
                } else {
                    time
                }
            } else {
                h.undefined()
            },
        ));
    } else if !h.equal(&p(h, &opts, "type")?, &h.receiver("OrderType.Market")?) {
        fields.push((
            "force",
            if h.equal(&time, &s(h, "normal")) {
                s(h, "gtc")
            } else {
                time
            },
        ));
    }
    fields.push((
        "reduceOnly",
        if h.equal(&mode, &h.receiver("PositionMode.Hedged")?) {
            h.undefined()
        } else if h.truthy(&p(h, &opts, "reduceOnly")?) {
            s(h, if uta { "yes" } else { "YES" })
        } else {
            s(h, if uta { "no" } else { "NO" })
        },
    ));
    if let Some(margin) = margin {
        fields.push((
            "marginMode",
            if h.equal(&margin, &h.receiver("MarginType.Cross")?) {
                s(h, "crossed")
            } else {
                s(h, "isolated")
            },
        ));
    }
    fields.push((
        if uta {
            "takeProfit"
        } else {
            "presetStopSurplusPrice"
        },
        optional_adjust(h, &opts, "takeProfit", &pp)?,
    ));
    fields.push((
        if uta {
            "stopLoss"
        } else {
            "presetStopLossPrice"
        },
        optional_adjust(h, &opts, "stopLoss", &pp)?,
    ));
    if uta {
        fields.push((
            "tpTriggerBy",
            if h.truthy(&p(h, &opts, "takeProfit")?) {
                trigger(h, &opts, "tpTriggerType")?
            } else {
                h.undefined()
            },
        ));
        fields.push((
            "slTriggerBy",
            if h.truthy(&p(h, &opts, "stopLoss")?) {
                trigger(h, &opts, "slTriggerType")?
            } else {
                h.undefined()
            },
        ));
        fields.push((
            "tpOrderType",
            if h.truthy(&p(h, &opts, "takeProfit")?) {
                s(h, "market")
            } else {
                h.undefined()
            },
        ));
        fields.push((
            "slOrderType",
            if h.truthy(&p(h, &opts, "stopLoss")?) {
                s(h, "market")
            } else {
                h.undefined()
            },
        ));
    }
    let req = h.invoke(omit, vec![h.object(fields)])?;
    let split = greater(h, max.clone(), h.number(0.))? && greater(h, amount.clone(), max.clone())?;
    let lots = if split {
        let ceil = h.capture("Math.ceil")?;
        let n = op(h, "divide", amount.clone(), max.clone())?;
        h.invoke(ceil, vec![n])?
    } else {
        h.number(1.)
    };
    let rest = if split {
        adjust(
            h,
            || op(h, "remainder", amount.clone(), max),
            || Ok(pa.clone()),
        )?
    } else {
        h.number(0.)
    };
    let lot = adjust(
        h,
        || {
            let n = op(h, "subtract", amount, rest.clone())?;
            op(h, "divide", n, lots.clone())
        },
        || Ok(pa),
    )?;
    let times = h.capture("times")?;
    let size_key = if uta { "qty" } else { "size" };
    let payloads = h.times(times, lots, || {
        let payload = spread(h, req.clone())?;
        h.define(&payload, size_key, template(h, lot.clone())?)?;
        Ok(payload)
    })?;
    if h.truthy(&rest) {
        let push = h.capture_member(&payloads, "push")?;
        let payload = spread(h, req)?;
        h.define(&payload, size_key, template(h, rest)?)?;
        h.invoke(push, vec![payload])?;
    }
    let client = call(h, "resolveCallerClientOrderId", vec![opts])?;
    let assert = h.capture("assertSingleNativeClientOrderId")?;
    h.invoke(assert, vec![client.clone(), p(h, &payloads, "length")?])?;
    let mut iter = h.iterator(&payloads)?;
    while let Some(payload) = h.next(&mut iter)? {
        let id = if h.nullish(&client) {
            call(h, "uuid", vec![])?
        } else {
            client.clone()
        };
        h.assign(&payload, "clientOid", id)?;
    }
    Ok(payloads)
}
pub async fn format_create_order<H: OrderPreparationHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    normal(h, opts, false).await
}
pub async fn format_uta_create_order<H: OrderPreparationHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    normal(h, opts, true).await
}
pub async fn format_uta_strategy_order<H: OrderPreparationHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.equal(&p(h, &opts, "type")?, &h.receiver("OrderType.StopLoss")?)
        && !h.equal(&p(h, &opts, "type")?, &h.receiver("OrderType.TakeProfit")?)
    {
        return Err(fail(
            h,
            s(
                h,
                "Bitget UTA currently supports only standalone stop-loss and take-profit strategy orders.",
            ),
        )?);
    }
    if !h.truthy(&p(h, &opts, "price")?) {
        let msg = h.operation("strategyPriceRequired", vec![p(h, &opts, "type")?])?;
        return Err(fail(h, msg)?);
    }
    let markets = h.receiver("store.marketsBySymbol")?;
    let market = h.key_get(&markets, &p(h, &opts, "symbol")?)?;
    if !h.truthy(&market) {
        return Err(missing(h, "Market", || p(h, &opts, "symbol"))?);
    }
    let mode = await_call(h, "getOrFetchPositionMode", || {
        Ok(vec![p(h, &opts, "symbol")?])
    })
    .await?;
    let sides = call(
        h,
        "getUtaStrategyOrderSide",
        vec![opts.clone(), mode.clone()],
    )?;
    let side = p(h, &sides, "side")?;
    let pos = p(h, &sides, "posSide")?;
    let raw = await_call(h, "getOrderPlanTypeAndAmount", || Ok(vec![opts.clone()])).await?;
    let (_, raw) = h.pair(raw)?;
    let mut qty = if greater(h, raw.clone(), h.number(0.))? {
        adjust(h, || Ok(raw), || precision(h, &market, "amount"))?
    } else {
        h.number(0.)
    };
    if h.equal(&qty, &h.number(0.)) {
        let positions = await_call(h, "getOrFetchPositionsForSymbol", || {
            Ok(vec![p(h, &opts, "symbol")?])
        })
        .await?;
        let target = if h.equal(&p(h, &opts, "side")?, &h.receiver("OrderSide.Sell")?) {
            h.receiver("PositionSide.Long")?
        } else {
            h.receiver("PositionSide.Short")?
        };
        let hedged = h.equal(&mode, &h.receiver("PositionMode.Hedged")?);
        let find = h.capture_member(&positions, "find")?;
        let position = h.find(find, positions, |p0| {
            if hedged && !h.equal(&p(h, &p0, "side")?, &target) {
                return Ok(false);
            }
            greater(h, p(h, &p0, "contracts")?, h.number(0.))
        })?;
        if h.truthy(&position) && greater(h, p(h, &position, "contracts")?, h.number(0.))? {
            qty = adjust(
                h,
                || p(h, &position, "contracts"),
                || precision(h, &market, "amount"),
            )?;
        }
    }
    let price = template(
        h,
        adjust(
            h,
            || p(h, &opts, "price"),
            || precision(h, &market, "price"),
        )?,
    )?;
    let omit = h.capture("omitUndefined")?;
    let mut fields = vec![
        ("category", h.receiver("apiUtaCategory")?),
        ("symbol", p(h, &market, "id")?),
        ("type", s(h, "tpsl")),
        ("side", side),
        ("posSide", pos),
        (
            "tpslMode",
            if greater(h, qty.clone(), h.number(0.))? {
                s(h, "partial")
            } else {
                s(h, "full")
            },
        ),
        (
            "qty",
            if greater(h, qty.clone(), h.number(0.))? {
                template(h, qty)?
            } else {
                h.undefined()
            },
        ),
        ("reduceOnly", s(h, "yes")),
        ("clientOid", call(h, "uuid", vec![])?),
    ];
    for (key, typ) in [
        ("takeProfit", "OrderType.TakeProfit"),
        ("stopLoss", "OrderType.StopLoss"),
    ] {
        fields.push((
            key,
            if h.equal(&p(h, &opts, "type")?, &h.receiver(typ)?) {
                price.clone()
            } else {
                h.undefined()
            },
        ));
    }
    for (key, typ, input) in [
        ("tpTriggerBy", "OrderType.TakeProfit", "tpTriggerType"),
        ("slTriggerBy", "OrderType.StopLoss", "slTriggerType"),
    ] {
        fields.push((
            key,
            if h.equal(&p(h, &opts, "type")?, &h.receiver(typ)?) {
                trigger(h, &opts, input)?
            } else {
                h.undefined()
            },
        ));
    }
    for (key, typ) in [
        ("tpOrderType", "OrderType.TakeProfit"),
        ("slOrderType", "OrderType.StopLoss"),
    ] {
        fields.push((
            key,
            if h.equal(&p(h, &opts, "type")?, &h.receiver(typ)?) {
                s(h, "market")
            } else {
                h.undefined()
            },
        ));
    }
    h.invoke(omit, vec![h.object(fields)])
}
async fn algo<H: OrderPreparationHost>(
    h: &H,
    opts: H::Value,
    inverse: bool,
) -> Result<H::Value, H::Error> {
    let markets = h.receiver("store.marketsBySymbol")?;
    let market = h.key_get(&markets, &p(h, &opts, "symbol")?)?;
    if !h.truthy(&market) {
        return Err(missing(h, "Market", || p(h, &opts, "symbol"))?);
    }
    let coin = if inverse {
        let f = h.capture("getMarginCoinForSymbol")?;
        h.invoke(f, vec![p(h, &opts, "symbol")?])?
    } else {
        h.receiver("apiMarginCoin")?
    };
    let payload = h.object(vec![
        ("marginCoin", coin),
        ("symbol", p(h, &market, "id")?),
        (
            "clientOid",
            call(h, if inverse { "generateUuid" } else { "uuid" }, vec![])?,
        ),
        ("productType", h.receiver("apiProductType")?),
    ]);
    let mode = await_call(h, "getOrFetchPositionMode", || {
        Ok(vec![p(h, &opts, "symbol")?])
    })
    .await?;
    if h.equal(&mode, &h.receiver("PositionMode.OneWay")?) {
        h.assign(&payload, "reduceOnly", s(h, "YES"))?;
    }
    for (typ, trigger_key) in [
        ("OrderType.TakeProfit", "tpTriggerType"),
        ("OrderType.StopLoss", "slTriggerType"),
    ] {
        if h.equal(&p(h, &opts, "type")?, &h.receiver(typ)?) && h.truthy(&p(h, &opts, "price")?) {
            let price = adjust(
                h,
                || p(h, &opts, "price"),
                || precision(h, &market, "price"),
            )?;
            h.assign(&payload, "triggerPrice", template(h, price)?)?;
            h.assign(
                &payload,
                "holdSide",
                call(
                    h,
                    if inverse {
                        "getInverseAlgoOrderSide"
                    } else {
                        "getAlgoOrderSide"
                    },
                    vec![opts.clone(), mode.clone()],
                )?,
            )?;
            let pair =
                await_call(h, "getOrderPlanTypeAndAmount", || Ok(vec![opts.clone()])).await?;
            let (plan, amount) = h.pair(pair)?;
            h.assign(&payload, "planType", plan)?;
            if h.truthy(&amount) {
                h.assign(
                    &payload,
                    "size",
                    template(
                        h,
                        adjust(h, || Ok(amount), || precision(h, &market, "amount"))?,
                    )?,
                )?;
            }
            if !inverse {
                h.assign(
                    &payload,
                    "triggerType",
                    if h.equal(
                        &p(h, &opts, trigger_key)?,
                        &h.receiver("OrderStopTriggerType.LastPrice")?,
                    ) {
                        s(h, "fill_price")
                    } else {
                        s(h, "mark_price")
                    },
                )?;
            }
        }
    }
    if h.equal(
        &p(h, &opts, "type")?,
        &h.receiver("OrderType.TrailingStopLoss")?,
    ) && h.truthy(&p(h, &opts, "price")?)
    {
        let price = adjust(
            h,
            || p(h, &opts, "price"),
            || precision(h, &market, "price"),
        )?;
        let amount = adjust(
            h,
            || p(h, &opts, "amount"),
            || precision(h, &market, "amount"),
        )?;
        let tickers = h.receiver("store.tickersBySymbol")?;
        let ticker = h.key_get(&tickers, &p(h, &opts, "symbol")?)?;
        if !h.truthy(&ticker) {
            return Err(missing(h, "Ticker", || p(h, &opts, "symbol"))?);
        }
        let distance = adjust(
            h,
            || {
                let max = h.capture("Math.max")?;
                let a = h.invoke(max, vec![p(h, &ticker, "last")?, p(h, &opts, "price")?])?;
                let min = h.capture("Math.min")?;
                let b = h.invoke(min, vec![p(h, &ticker, "last")?, p(h, &opts, "price")?])?;
                op(h, "subtract", a, b)
            },
            || precision(h, &market, "price"),
        )?;
        let round = h.capture("Math.round")?;
        let n = op(h, "multiply", distance, h.number(100.))?;
        let n = op(h, "divide", n, p(h, &ticker, "last")?)?;
        let n = op(h, "multiply", n, h.number(10.))?;
        let n = h.invoke(round, vec![n])?;
        let ratio = op(h, "divide", n, h.number(10.))?;
        h.assign(&payload, "triggerPrice", template(h, price)?)?;
        h.assign(&payload, "size", template(h, amount)?)?;
        if inverse {
            h.assign(
                &payload,
                "side",
                call(h, "getInverseAlgoOrderSide", vec![opts.clone(), mode])?,
            )?;
            h.assign(&payload, "rangeRate", template(h, ratio)?)?;
        } else {
            h.assign(&payload, "planType", s(h, "track_plan"))?;
            let margin = await_call(h, "getOrFetchMarginType", || {
                Ok(vec![p(h, &opts, "symbol")?])
            })
            .await?;
            h.assign(
                &payload,
                "marginMode",
                if h.equal(&margin, &h.receiver("MarginType.Cross")?) {
                    s(h, "crossed")
                } else {
                    s(h, "isolated")
                },
            )?;
            h.assign(&payload, "orderType", s(h, "market"))?;
            h.assign(
                &payload,
                "triggerType",
                if h.equal(
                    &p(h, &opts, "slTriggerType")?,
                    &h.receiver("OrderStopTriggerType.LastPrice")?,
                ) {
                    s(h, "fill_price")
                } else {
                    s(h, "mark_price")
                },
            )?;
            h.assign(
                &payload,
                "side",
                if h.equal(&p(h, &opts, "side")?, &h.receiver("OrderSide.Buy")?) {
                    s(h, "buy")
                } else {
                    s(h, "sell")
                },
            )?;
            if h.equal(&mode, &h.receiver("PositionMode.Hedged")?) {
                h.assign(&payload, "tradeSide", s(h, "close"))?;
            }
            h.assign(&payload, "callbackRatio", template(h, ratio)?)?;
        }
    }
    Ok(payload)
}
pub async fn format_algo_order<H: OrderPreparationHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    algo(h, opts, false).await
}
pub async fn inverse_format_algo_order<H: OrderPreparationHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    algo(h, opts, true).await
}
fn string_predicate<H: OrderPreparationHost>(
    h: &H,
    order: &H::Value,
    method: &str,
    needle: &str,
) -> Result<bool, H::Error> {
    let id = p(h, order, "id")?;
    let f = h.capture_member(&id, method)?;
    Ok(h.truthy(&h.invoke(f, vec![s(h, needle)])?))
}
pub async fn update_virtual_tp_sl_order<H: OrderPreparationHost>(
    h: &H,
    input: H::Value,
    related: H::Value,
) -> Result<H::Value, H::Error> {
    let order = p(h, &input, "order")?;
    let update = p(h, &input, "update")?;
    let markets = h.receiver("store.marketsBySymbol")?;
    let market = h.key_get(&markets, &p(h, &order, "symbol")?)?;
    if !h.truthy(&market) {
        return Err(missing(h, "Market", || p(h, &order, "symbol"))?);
    }
    let f = h.capture_member(&related, "find")?;
    let parent = h.find(f, related.clone(), |v| {
        Ok(!string_predicate(h, &v, "includes", "__")?)
    })?;
    if !h.truthy(&parent) {
        let emit = h.capture("emitAndLogError")?;
        let msg = h.operation("parentMissing", vec![p(h, &order, "id")?])?;
        h.invoke(emit, vec![msg])?;
        let ctor = h.capture("Error")?;
        let msg = h.operation("parentMissing", vec![p(h, &order, "id")?])?;
        return Err(h.new_error(ctor, msg)?);
    }
    let f = h.capture_member(&related, "find")?;
    let stop = h.find(f, related.clone(), |v| {
        string_predicate(h, &v, "endsWith", "__stop_loss")
    })?;
    let f = h.capture_member(&related, "find")?;
    let take = h.find(f, related, |v| {
        string_predicate(h, &v, "endsWith", "__take_profit")
    })?;
    let formatted = await_call(h, "formatCreateOrder", || {
        Ok(vec![spread(h, parent.clone())?])
    })
    .await?;
    let omit = h.capture("omitUndefined")?;
    let payload = spread(h, h.key_get(&formatted, &h.number(0.))?)?;
    h.define(&payload, "price", template(h, p(h, &parent, "price")?)?)?;
    h.define(&payload, "size", template(h, p(h, &parent, "amount")?)?)?;
    h.define(
        &payload,
        "presetStopSurplusPrice",
        if h.truthy(&take) {
            template(h, p(h, &take, "price")?)?
        } else {
            h.undefined()
        },
    )?;
    h.define(
        &payload,
        "presetStopLossPrice",
        if h.truthy(&stop) {
            template(h, p(h, &stop, "price")?)?
        } else {
            h.undefined()
        },
    )?;
    let payload = h.invoke(omit, vec![payload])?;
    if h.has(&update, "amount")? {
        let n = adjust(
            h,
            || p(h, &update, "amount"),
            || precision(h, &market, "amount"),
        )?;
        h.assign(&payload, "size", template(h, n)?)?;
    }
    if h.equal(&p(h, &order, "id")?, &p(h, &parent, "id")?) {
        if h.has(&update, "price")? {
            let n = adjust(
                h,
                || p(h, &update, "price"),
                || precision(h, &market, "price"),
            )?;
            h.assign(&payload, "price", template(h, n)?)?;
        }
    } else if string_predicate(h, &order, "endsWith", "__stop_loss")? {
        if h.has(&update, "price")? {
            let n = adjust(
                h,
                || p(h, &update, "price"),
                || precision(h, &market, "price"),
            )?;
            h.assign(&payload, "presetStopLossPrice", template(h, n)?)?;
        }
    } else if string_predicate(h, &order, "endsWith", "__take_profit")?
        && h.has(&update, "price")?
    {
        let n = adjust(
            h,
            || p(h, &update, "price"),
            || precision(h, &market, "price"),
        )?;
        h.assign(&payload, "presetStopSurplusPrice", template(h, n)?)?;
    }
    await_call(h, "cancelOrders", || Ok(vec![h.array(vec![parent])])).await?;
    await_call(h, "placeOrderBatch", || Ok(vec![h.array(vec![payload])])).await
}
pub async fn update_uta_strategy_order<H: OrderPreparationHost>(
    h: &H,
    input: H::Value,
) -> Result<H::Value, H::Error> {
    let order = p(h, &input, "order")?;
    let update = p(h, &input, "update")?;
    if h.equal(
        &p(h, &order, "type")?,
        &h.receiver("OrderType.TrailingStopLoss")?,
    ) {
        return Err(fail(
            h,
            s(h, "Bitget UTA trailing-stop orders are not supported yet."),
        )?);
    }
    let f = h.capture("formatUtaStrategyOrder")?;
    let original = call(h, "buildUtaStrategyPlaceOptsFromOrder", vec![order.clone()])?;
    let original = h.start(f, vec![original])?.await?;
    let replacement = call(h, "buildUtaStrategyPlaceOptsFromOrder", vec![order.clone()])?;
    if h.has(&update, "price")? {
        h.assign(&replacement, "price", p(h, &update, "price")?)?;
    }
    if h.has(&update, "amount")? {
        h.assign(&replacement, "amount", p(h, &update, "amount")?)?;
    }
    let replacement = await_call(h, "formatUtaStrategyOrder", || Ok(vec![replacement])).await?;
    await_call(h, "cancelUtaStrategyOrders", || {
        Ok(vec![
            h.array(vec![order]),
            h.object(vec![("treatNotFoundAsSuccess", h.boolean(false))]),
        ])
    })
    .await?;
    match await_call(h, "placeUtaStrategyOrders", || {
        Ok(vec![h.array(vec![replacement])])
    })
    .await
    {
        Ok(v) => Ok(v),
        Err(place_err) => {
            let rollback = async {
                await_call(h, "placeUtaStrategyOrders", || {
                    Ok(vec![h.array(vec![original])])
                })
                .await?;
                let f = h.capture("emitAndLogError")?;
                let msg = opt(h, &h.error_value(&place_err), "message")?;
                let msg = if h.truthy(&msg) {
                    msg
                } else {
                    s(h, "place failed")
                };
                let msg = h.operation("restoredMessage", vec![msg])?;
                h.invoke(f, vec![msg])?;
                Ok::<(), H::Error>(())
            }
            .await;
            if let Err(rollback_err) = rollback {
                let f = h.capture("emitAndLogError")?;
                let msg = opt(h, &h.error_value(&place_err), "message")?;
                let msg = if h.truthy(&msg) {
                    msg
                } else {
                    s(h, "place failed")
                };
                let rb = opt(h, &h.error_value(&rollback_err), "message")?;
                let rb = if h.truthy(&rb) {
                    rb
                } else {
                    s(h, "rollback failed")
                };
                let msg = h.operation("rollbackMessage", vec![msg, rb])?;
                h.invoke(f, vec![msg])?;
            }
            Err(place_err)
        }
    }
}
pub async fn resolve_positions_for_plan_type<H: OrderPreparationHost>(
    h: &H,
    symbol: H::Value,
    side: H::Value,
) -> Result<H::Value, H::Error> {
    let positions = h.receiver("store.positions")?;
    let some = h.capture_member(&positions, "some")?;
    let answers = h.some(some, positions, |v| {
        if !h.equal(&p(h, &v, "symbol")?, &symbol) || !h.equal(&p(h, &v, "side")?, &side) {
            return Ok(false);
        }
        greater(h, p(h, &v, "contracts")?, h.number(0.))
    })?;
    if answers {
        return h.receiver("store.positions");
    }
    let cached = h.receiver("_planTypePositionsCache")?;
    if h.truthy(&cached) {
        let now = call(h, "Date.now", vec![])?;
        let elapsed = op(h, "subtract", now, p(h, &cached, "at")?)?;
        if h.truthy(&op(
            h,
            "less",
            elapsed,
            h.receiver("BitgetExchange.POSITIONING_CACHE_TTL_MS")?,
        )?) {
            return p(h, &cached, "value");
        }
    }
    let inflight = h.receiver("_planTypePositionsInflight")?;
    if h.truthy(&inflight) {
        return h
            .await_value(h.receiver("_planTypePositionsInflight")?)
            .await;
    }
    let host = h.clone();
    let symbol = symbol.clone();
    let promise = h.launch(Box::pin(async move {
        let h = &host;
        let result = async {
            let positions = await_call(h, "fetchOpenPositions", || Ok(vec![])).await?;
            if greater(h, p(h, &positions, "length")?, h.number(0.))? {
                let at = call(h, "Date.now", vec![])?;
                h.set_receiver(
                    "_planTypePositionsCache",
                    h.object(vec![("at", at), ("value", positions.clone())]),
                )?;
            }
            Ok::<H::Value, H::Error>(positions)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let f = h.capture("emitAndLogError")?;
                let error = h.error_value(&e);
                let msg = opt(h, &opt(h, &opt(h, &error, "response")?, "data")?, "msg")?;
                let msg = if h.truthy(&msg) {
                    msg
                } else {
                    opt(h, &error, "message")?
                };
                let msg = h.operation("positionsFallbackMessage", vec![symbol.clone(), msg])?;
                h.invoke(f, vec![msg])?;
                Ok(h.array(vec![]))
            }
        }
    }))?;
    h.set_receiver("_planTypePositionsInflight", promise.clone())?;
    let result = h.await_value(promise).await;
    h.set_receiver("_planTypePositionsInflight", h.null())?;
    result
}
