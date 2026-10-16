use crate::{
    order_control::OrderControlOps,
    public_streams::StreamOps,
    spot_private_class::{PrivateClassOps, call, fallback, optional},
};
fn is<V, E>(h: &StreamOps<V, E>, value: V, path: &str) -> Result<bool, E> {
    Ok((h.equal)(&value, &(h.capture)(path)?))
}
fn text<V, E>(ops: &PrivateClassOps<V, E>, value: &V) -> Result<V, E> {
    Ok((ops.string_value)((ops.template)(value)?))
}
fn adjusted<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &StreamOps<V, E>,
    value: V,
    precision: V,
) -> Result<V, E> {
    text(ops, &call(h, "adjust", || Ok(vec![value, precision]))?)
}
fn market<V, E>(h: &StreamOps<V, E>, opts: &V) -> Result<V, E> {
    let table = (h.capture)("this.store.marketsBySymbol")?;
    (h.intrinsic)("get-index", vec![table, (h.get)(opts, "symbol")?])
}
fn missing_market<V, E>(
    ops: &PrivateClassOps<V, E>,
    h: &OrderControlOps<V, E>,
    opts: &V,
) -> Result<E, E> {
    let constructor = (h.value.capture)("Error")?;
    let mut units = "Market ".encode_utf16().collect::<Vec<_>>();
    units.extend((ops.template)(&(h.value.get)(opts, "symbol")?)?);
    units.extend(" not found".encode_utf16());
    (h.new_error)(constructor, (ops.string_value)(units))
}
pub fn format_create_order<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    control: &OrderControlOps<V, E>,
    opts: V,
) -> Result<V, E> {
    let h = &control.value;
    let market = market(h, &opts)?;
    if !(h.truthy)(&market) {
        return Err(missing_market(ops, control, &opts)?);
    }
    let side = (h.literal)(if is(h, (h.get)(&opts, "side")?, "OrderSide.Buy")? {
        "buy"
    } else {
        "sell"
    });
    let position = call(h, "this.getPositionForOrder", || Ok(vec![opts.clone()]))?;
    let leverage = fallback(h, optional(h, &position, "leverage")?, || {
        Ok((h.number)(1.))
    })?;
    let price_precision = (h.get)(&(h.get)(&market, "precision")?, "price")?;
    let amount_precision = (h.get)(&(h.get)(&market, "precision")?, "amount")?;
    let amount = call(h, "adjust", || {
        Ok(vec![(h.get)(&opts, "amount")?, amount_precision])
    })?;
    let price = if (h.truthy)(&(h.get)(&opts, "price")?)
        && !is(h, (h.get)(&opts, "type")?, "OrderType.Market")?
    {
        call(h, "adjust", || {
            Ok(vec![(h.get)(&opts, "price")?, price_precision.clone()])
        })?
    } else {
        (h.undefined)()
    };
    let order_type = (h.literal)(if is(h, (h.get)(&opts, "type")?, "OrderType.Market")? {
        "market"
    } else {
        "limit"
    });
    let up = (h.get)(
        &opts,
        if is(h, (h.get)(&opts, "side")?, "OrderSide.Buy")? {
            "takeProfit"
        } else {
            "stopLoss"
        },
    )?;
    let down = (h.get)(
        &opts,
        if is(h, (h.get)(&opts, "side")?, "OrderSide.Buy")? {
            "stopLoss"
        } else {
            "takeProfit"
        },
    )?;
    let attached = call(h, "Boolean", || {
        Ok(vec![fallback(h, up.clone(), || Ok(down.clone()))?])
    })?;
    let mut tif = "GTC";
    for (kind, value) in [
        ("GoodTillCancel", "GTC"),
        ("ImmediateOrCancel", "IOC"),
        ("FillOrKill", "FOK"),
        ("PostOnly", "GTC"),
    ] {
        if is(
            h,
            (h.get)(&opts, "timeInForce")?,
            &format!("OrderTimeInForce.{kind}"),
        )? {
            tif = value;
            break;
        }
    }
    let req = call(h, "omitUndefined", || {
        Ok(vec![(h.object)(vec![
            ("symbol", (h.get)(&market, "id")?),
            ("type", order_type.clone()),
            ("side", side),
            (
                "price",
                if (h.truthy)(&price) {
                    text(ops, &price)?
                } else {
                    (h.undefined)()
                },
            ),
            (
                "size",
                if (h.truthy)(&amount) {
                    text(ops, &amount)?
                } else {
                    (h.undefined)()
                },
            ),
            ("leverage", leverage),
            ("reduceOnly", (h.get)(&opts, "reduceOnly")?),
            (
                "marginMode",
                call(h, "this.getKucoinMarginMode", || Ok(vec![position.clone()]))?,
            ),
            (
                "positionSide",
                call(h, "this.getKucoinPositionSide", || {
                    Ok(vec![opts.clone(), position.clone()])
                })?,
            ),
            (
                "timeInForce",
                if (h.equal)(&order_type, &(h.literal)("limit")) {
                    (h.literal)(tif)
                } else {
                    (h.undefined)()
                },
            ),
            ("clientOid", {
                let caller = call(h, "this.resolveCallerClientOrderId", || {
                    Ok(vec![opts.clone()])
                })?;
                if (h.nullish)(&caller) {
                    call(h, "uuid", || Ok(vec![]))?
                } else {
                    caller
                }
            }),
            (
                "triggerStopUpPrice",
                if (h.truthy)(&up) {
                    adjusted(ops, h, up, price_precision.clone())?
                } else {
                    (h.undefined)()
                },
            ),
            (
                "triggerStopDownPrice",
                if (h.truthy)(&down) {
                    adjusted(ops, h, down, price_precision.clone())?
                } else {
                    (h.undefined)()
                },
            ),
            (
                "stopPriceType",
                if (h.truthy)(&attached) {
                    call(h, "this.mapStopPriceType", || {
                        Ok(vec![fallback(h, (h.get)(&opts, "tpTriggerType")?, || {
                            (h.get)(&opts, "slTriggerType")
                        })?])
                    })?
                } else {
                    (h.undefined)()
                },
            ),
            (
                "postOnly",
                if is(
                    h,
                    (h.get)(&opts, "timeInForce")?,
                    "OrderTimeInForce.PostOnly",
                )? {
                    (ops.boolean)(true)
                } else {
                    (h.undefined)()
                },
            ),
        ])])
    })?;
    Ok((h.array)(vec![req]))
}
pub fn format_algo_order<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    control: &OrderControlOps<V, E>,
    opts: V,
) -> Result<V, E> {
    let h = &control.value;
    let market = market(h, &opts)?;
    if !(h.truthy)(&market) {
        return Err(missing_market(ops, control, &opts)?);
    }
    let full = !(h.truthy)(&(h.get)(&opts, "amount")?);
    let mut fields = vec![
        ("symbol", (h.get)(&market, "id")?),
        ("clientOid", call(h, "uuid", || Ok(vec![]))?),
        (
            "side",
            (h.literal)(if is(h, (h.get)(&opts, "side")?, "OrderSide.Buy")? {
                "buy"
            } else {
                "sell"
            }),
        ),
        ("type", (h.literal)("market")),
        ("reduceOnly", {
            let reduce = (h.get)(&opts, "reduceOnly")?;
            if (h.nullish)(&reduce) {
                (ops.boolean)(true)
            } else {
                reduce
            }
        }),
    ];
    if full {
        fields.push(("closeOrder", (ops.boolean)(true)));
    } else {
        let amount = call(h, "adjust", || {
            Ok(vec![
                (h.get)(&opts, "amount")?,
                (h.get)(&(h.get)(&market, "precision")?, "amount")?,
            ])
        })?;
        fields.push(("size", text(ops, &amount)?));
    }
    let payload = (h.object)(fields);
    for (kind, trigger) in [
        ("OrderType.TakeProfit", "tpTriggerType"),
        ("OrderType.StopLoss", "slTriggerType"),
    ] {
        if is(h, (h.get)(&opts, "type")?, kind)? && (h.truthy)(&(h.get)(&opts, "price")?) {
            let price = call(h, "adjust", || {
                Ok(vec![
                    (h.get)(&opts, "price")?,
                    (h.get)(&(h.get)(&market, "precision")?, "price")?,
                ])
            })?;
            let position = call(h, "this.getPositionForOrder", || Ok(vec![opts.clone()]))?;
            (h.set)(
                &payload,
                "stop",
                call(h, "this.getKucoinStopDirection", || Ok(vec![opts.clone()]))?,
            )?;
            (h.set)(&payload, "stopPrice", text(ops, &price)?)?;
            (h.set)(
                &payload,
                "stopPriceType",
                call(h, "this.mapStopPriceType", || {
                    Ok(vec![(h.get)(&opts, trigger)?])
                })?,
            )?;
            (h.set)(
                &payload,
                "marginMode",
                call(h, "this.getKucoinMarginMode", || Ok(vec![position.clone()]))?,
            )?;
            (h.set)(
                &payload,
                "positionSide",
                call(h, "this.getKucoinPositionSide", || {
                    Ok(vec![opts.clone(), position])
                })?,
            )?;
        }
    }
    if is(h, (h.get)(&opts, "type")?, "OrderType.TrailingStopLoss")? {
        let constructor = (h.capture)("Error")?;
        return Err((control.new_error)(
            constructor,
            (h.literal)(
                "KuCoin Futures trailing stop placement is not exposed in the current REST documentation",
            ),
        )?);
    }
    Ok(payload)
}
