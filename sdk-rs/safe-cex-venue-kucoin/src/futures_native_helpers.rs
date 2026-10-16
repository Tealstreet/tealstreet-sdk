use crate::{
    public_streams::StreamOps,
    spot_private_class::{PrivateClassOps, call, fallback, optional},
};
fn index<V, E>(h: &StreamOps<V, E>, object: V, key: V) -> Result<V, E> {
    (h.intrinsic)("get-index", vec![object, key])
}
fn member<V, E>(
    h: &StreamOps<V, E>,
    receiver: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let function = (h.get)(&receiver, key)?;
    (h.intrinsic)(
        "call-with-this",
        vec![function, receiver, (h.array)(args()?)],
    )
}
fn is<V, E>(h: &StreamOps<V, E>, value: V, path: &str) -> Result<bool, E> {
    Ok((h.equal)(&value, &(h.capture)(path)?))
}
pub fn to_kucoin_symbol<V, E>(h: &StreamOps<V, E>, symbol: V) -> V {
    if (h.truthy)(&symbol) {
        symbol
    } else {
        (h.literal)("")
    }
}
pub fn to_internal_symbol<V: Clone, E>(h: &StreamOps<V, E>, symbol: V) -> Result<V, E> {
    let symbol = call(h, "this.toKucoinSymbol", || Ok(vec![symbol]))?;
    if (h.truthy)(&symbol) {
        call(h, "toBinance", || Ok(vec![symbol, (h.literal)("kucoin")]))
    } else {
        Ok((h.literal)(""))
    }
}
pub fn to_api_symbol<V: Clone, E>(h: &StreamOps<V, E>, symbol: V) -> Result<V, E> {
    if !(h.truthy)(&symbol) {
        return Ok((h.literal)(""));
    }
    let table = (h.capture)("this.store.marketsBySymbol")?;
    let market = index(h, table, symbol.clone())?;
    fallback(h, optional(h, &market, "id")?, || {
        call(h, "this.toKucoinSymbol", || Ok(vec![symbol]))
    })
}
pub fn contract_size_for<V: Clone, E>(h: &StreamOps<V, E>, symbol: V) -> Result<V, E> {
    let size = if (h.truthy)(&symbol) {
        let table = (h.capture)("this.store.marketsBySymbol")?;
        optional(h, &index(h, table, symbol)?, "contractSize")?
    } else {
        symbol
    };
    fallback(h, size, || Ok((h.number)(1.)))
}
pub fn native_amount_for_cancel_replace<V: Clone, E>(
    h: &StreamOps<V, E>,
    order: V,
) -> Result<V, E> {
    let extra = (h.get)(&order, "extra")?;
    let remaining = optional(h, &extra, "remainingNative")?;
    if !(h.nullish)(&remaining) {
        return Ok(remaining);
    }
    call(h, "divide", || {
        Ok(vec![
            (h.get)(&order, "remaining")?,
            call(h, "this.contractSizeFor", || {
                Ok(vec![(h.get)(&order, "symbol")?])
            })?,
        ])
    })
}
fn uppercase<V: Clone, E>(h: &StreamOps<V, E>, value: V) -> Result<V, E> {
    let string = call(h, "String", || {
        Ok(vec![fallback(h, value, || Ok((h.literal)("")))?])
    })?;
    member(h, string, "toUpperCase", || Ok(vec![]))
}
pub fn parse_margin_type<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &StreamOps<V, E>,
    margin: V,
    cross: V,
) -> Result<V, E> {
    let mode = uppercase(h, margin)?;
    if (h.equal)(&mode, &(h.literal)("ISOLATED")) || (h.equal)(&mode, &(h.literal)("FIXED")) {
        return (h.capture)("MarginType.Isolated");
    }
    if (h.equal)(&mode, &(h.literal)("CROSS")) || (h.equal)(&mode, &(h.literal)("CROSSED")) {
        return (h.capture)("MarginType.Cross");
    }
    if (h.equal)(&cross, &(ops.boolean)(false)) {
        return (h.capture)("MarginType.Isolated");
    }
    (h.capture)("MarginType.Cross")
}
pub fn parse_position_mode<V: Clone, E>(h: &StreamOps<V, E>, position: V) -> Result<V, E> {
    let mode = uppercase(h, position)?;
    for name in ["1", "HEDGE_MODE", "HEDGE", "DOUBLE_HOLD"] {
        if (h.equal)(&mode, &(h.literal)(name)) {
            return (h.capture)("PositionMode.Hedged");
        }
    }
    (h.capture)("PositionMode.OneWay")
}
pub fn parse_row_position_mode<V: Clone, E>(h: &StreamOps<V, E>, row: V) -> Result<V, E> {
    if (h.truthy)(&(h.get)(&row, "positionSide")?)
        && !(h.equal)(&(h.get)(&row, "positionSide")?, &(h.literal)("BOTH"))
    {
        return (h.capture)("PositionMode.Hedged");
    }
    call(h, "this.parsePositionMode", || {
        Ok(vec![fallback(h, (h.get)(&row, "positionMode")?, || {
            (h.get)(&row, "holdMode")
        })?])
    })
}
pub fn get_order_side_v2<V: Clone, E>(h: &StreamOps<V, E>, opts: V, mode: V) -> Result<V, E> {
    let mut side = if is(h, (h.get)(&opts, "side")?, "OrderSide.Buy")? {
        "Buy"
    } else {
        "Sell"
    };
    let mut trade = (h.undefined)();
    if is(h, mode, "PositionMode.Hedged")? {
        if (h.truthy)(&(h.get)(&opts, "reduceOnly")?) {
            trade = (h.literal)("Close");
            side = if is(h, (h.get)(&opts, "side")?, "OrderSide.Buy")? {
                "Sell"
            } else {
                "Buy"
            };
        } else {
            trade = (h.literal)("Open");
        }
    }
    Ok((h.array)(vec![(h.literal)(side), trade]))
}
pub fn get_kucoin_margin_mode<V: Clone, E>(h: &StreamOps<V, E>, position: V) -> Result<V, E> {
    if is(h, optional(h, &position, "marginType")?, "MarginType.Cross")? {
        return Ok((h.literal)("CROSS"));
    }
    if is(
        h,
        optional(h, &position, "marginType")?,
        "MarginType.Isolated",
    )? {
        return Ok((h.literal)("ISOLATED"));
    }
    Ok((h.undefined)())
}
pub fn get_kucoin_position_side<V: Clone, E>(
    h: &StreamOps<V, E>,
    opts: V,
    position: V,
) -> Result<V, E> {
    if !is(
        h,
        optional(h, &position, "positionMode")?,
        "PositionMode.Hedged",
    )? {
        return Ok((h.undefined)());
    }
    let closing = (h.truthy)(&(h.get)(&opts, "reduceOnly")?)
        || (h.truthy)(&call(h, "this.isAlgoOrder", || Ok(vec![opts.clone()]))?);
    let buy = is(h, (h.get)(&opts, "side")?, "OrderSide.Buy")?;
    let target = (h.capture)(if closing == buy {
        "PositionSide.Short"
    } else {
        "PositionSide.Long"
    })?;
    Ok((h.literal)(if is(h, target, "PositionSide.Long")? {
        "LONG"
    } else {
        "SHORT"
    }))
}
pub fn map_stop_price_type<V: Clone, E>(h: &StreamOps<V, E>, trigger: V) -> Result<V, E> {
    if is(h, trigger.clone(), "OrderStopTriggerType.MarkPrice")? {
        return Ok((h.literal)("MP"));
    }
    if is(h, trigger, "OrderStopTriggerType.IndexPrice")? {
        return Ok((h.literal)("IP"));
    }
    Ok((h.literal)("TP"))
}
pub fn is_virtual_tpsl_order<V: Clone, E>(h: &StreamOps<V, E>, order: V) -> Result<V, E> {
    let first = member(h, (h.get)(&order, "id")?, "endsWith", || {
        Ok(vec![(h.literal)("__stop_loss")])
    })?;
    fallback(h, first, || {
        member(h, (h.get)(&order, "id")?, "endsWith", || {
            Ok(vec![(h.literal)("__take_profit")])
        })
    })
}
pub fn is_algo_order<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &StreamOps<V, E>,
    opts: V,
) -> Result<V, E> {
    for name in [
        "OrderType.StopLoss",
        "OrderType.TakeProfit",
        "OrderType.TrailingStopLoss",
    ] {
        if is(h, (h.get)(&opts, "type")?, name)? {
            return Ok((ops.boolean)(true));
        }
    }
    Ok((ops.boolean)(false))
}
fn thrown<V, E>(
    ops: &PrivateClassOps<V, E>,
    h: &crate::order_control::OrderControlOps<V, E>,
    message: impl FnOnce() -> Result<Vec<u16>, E>,
) -> Result<E, E> {
    let constructor = (h.value.capture)("Error")?;
    (h.new_error)(constructor, (ops.string_value)(message()?))
}
fn error_text<V, E>(
    ops: &PrivateClassOps<V, E>,
    h: &StreamOps<V, E>,
    prefix: &str,
    opts: &V,
    key: &str,
) -> Result<Vec<u16>, E> {
    let mut units = prefix.encode_utf16().collect::<Vec<_>>();
    units.extend((ops.template)(&(h.get)(opts, key)?)?);
    Ok(units)
}
pub fn get_order_side<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &crate::order_control::OrderControlOps<V, E>,
    opts: V,
    mode: V,
) -> Result<V, E> {
    let v = &h.value;
    if is(v, mode, "PositionMode.OneWay")? {
        return Ok((v.literal)(
            if is(v, (v.get)(&opts, "side")?, "OrderSide.Buy")? {
                "buy_single"
            } else {
                "sell_single"
            },
        ));
    }
    if (v.truthy)(&(v.get)(&opts, "reduceOnly")?) {
        if is(v, (v.get)(&opts, "side")?, "OrderSide.Buy")? {
            return Ok((v.literal)("close_short"));
        }
        if is(v, (v.get)(&opts, "side")?, "OrderSide.Sell")? {
            return Ok((v.literal)("close_long"));
        }
    }
    if is(v, (v.get)(&opts, "side")?, "OrderSide.Buy")? {
        return Ok((v.literal)("open_long"));
    }
    if is(v, (v.get)(&opts, "side")?, "OrderSide.Sell")? {
        return Ok((v.literal)("open_short"));
    }
    Err(thrown(ops, h, || {
        error_text(ops, v, "Unknown order side: ", &opts, "side")
    })?)
}
pub fn get_algo_order_side<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &crate::order_control::OrderControlOps<V, E>,
    opts: V,
) -> Result<V, E> {
    let v = &h.value;
    for (kind, buy, sell) in [
        ("OrderType.TakeProfit", "short", "long"),
        ("OrderType.StopLoss", "short", "long"),
        ("OrderType.TrailingStopLoss", "close_short", "close_long"),
    ] {
        if is(v, (v.get)(&opts, "type")?, kind)? {
            if is(v, (v.get)(&opts, "side")?, "OrderSide.Buy")? {
                return Ok((v.literal)(buy));
            }
            if is(v, (v.get)(&opts, "side")?, "OrderSide.Sell")? {
                return Ok((v.literal)(sell));
            }
        }
    }
    Err(thrown(ops, h, || {
        let mut text = error_text(ops, v, "Unknown algo order side: ", &opts, "type")?;
        text.extend(" - ".encode_utf16());
        text.extend((ops.template)(&(v.get)(&opts, "side")?)?);
        Ok(text)
    })?)
}
pub fn get_order_plan_type<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &crate::order_control::OrderControlOps<V, E>,
    opts: V,
) -> Result<V, E> {
    let v = &h.value;
    for (kind, plan) in [
        ("OrderType.StopLoss", "pos_loss"),
        ("OrderType.TakeProfit", "pos_profit"),
        ("OrderType.TrailingStopLoss", "track_plan"),
    ] {
        if is(v, (v.get)(&opts, "type")?, kind)? {
            return Ok((v.literal)(plan));
        }
    }
    Err(thrown(ops, h, || {
        error_text(ops, v, "Unknown order type: ", &opts, "type")
    })?)
}
pub fn get_kucoin_stop_direction<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &crate::order_control::OrderControlOps<V, E>,
    opts: V,
) -> Result<V, E> {
    let v = &h.value;
    if is(v, (v.get)(&opts, "type")?, "OrderType.StopLoss")? {
        return Ok((v.literal)(
            if is(v, (v.get)(&opts, "side")?, "OrderSide.Buy")? {
                "up"
            } else {
                "down"
            },
        ));
    }
    if is(v, (v.get)(&opts, "type")?, "OrderType.TakeProfit")? {
        return Ok((v.literal)(
            if is(v, (v.get)(&opts, "side")?, "OrderSide.Buy")? {
                "down"
            } else {
                "up"
            },
        ));
    }
    Err(thrown(ops, h, || {
        error_text(
            ops,
            v,
            "Unsupported KuCoin stop direction for order type: ",
            &opts,
            "type",
        )
    })?)
}
pub fn get_position_for_order<V: Clone + 'static, E: 'static>(
    ops: &PrivateClassOps<V, E>,
    h: std::rc::Rc<StreamOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let closing = (h.truthy)(&(h.get)(&opts, "reduceOnly")?)
        || (h.truthy)(&call(&h, "this.isAlgoOrder", || Ok(vec![opts.clone()]))?);
    let buy = is(&h, (h.get)(&opts, "side")?, "OrderSide.Buy")?;
    let target = (h.capture)(if closing == buy {
        "PositionSide.Short"
    } else {
        "PositionSide.Long"
    })?;
    let find = (h.capture)("this.store.positions.find")?;
    let value = h.clone();
    let boolean = ops.boolean.clone();
    let predicate = (ops.arrow)(
        "",
        1,
        std::rc::Rc::new(move |args| {
            let row = args.first().cloned().unwrap_or_else(|| (value.undefined)());
            let equal = (value.equal)(
                &(value.get)(&row, "symbol")?,
                &(value.get)(&opts, "symbol")?,
            );
            Ok(boolean(
                equal && (value.equal)(&(value.get)(&row, "side")?, &target),
            ))
        }),
    );
    (h.invoke)(find, vec![predicate])
}
pub fn get_order_plan_type_and_amount<V: Clone + 'static, E: 'static>(
    ops: &PrivateClassOps<V, E>,
    h: std::rc::Rc<crate::order_control::OrderControlOps<V, E>>,
    opts: V,
) -> Result<V, E> {
    let v = &h.value;
    if is(v, (v.get)(&opts, "type")?, "OrderType.TrailingStopLoss")? {
        return Ok((v.array)(vec![
            (v.literal)("track_plan"),
            (v.get)(&opts, "amount")?,
        ]));
    }
    let mut infinite = true;
    if !(v.equal)(&(v.get)(&opts, "amount")?, &(v.number)(0.)) {
        let base = call(v, "multiply", || {
            Ok(vec![
                (v.get)(&opts, "amount")?,
                call(v, "this.contractSizeFor", || {
                    Ok(vec![(v.get)(&opts, "symbol")?])
                })?,
            ])
        })?;
        let find = (v.capture)("this.store.positions.find")?;
        let value = v.clone();
        let options = opts.clone();
        let boolean = ops.boolean.clone();
        let predicate = (ops.arrow)(
            "",
            1,
            std::rc::Rc::new(move |args| {
                let row = args.first().cloned().unwrap_or_else(|| (value.undefined)());
                if !(value.equal)(
                    &(value.get)(&row, "symbol")?,
                    &(value.get)(&options, "symbol")?,
                ) {
                    return Ok(boolean(false));
                }
                let side = (value.get)(&row, "side")?;
                let target = (value.capture)(
                    if is(&value, (value.get)(&options, "side")?, "OrderSide.Buy")? {
                        "PositionSide.Short"
                    } else {
                        "PositionSide.Long"
                    },
                )?;
                if !(value.equal)(&side, &target) {
                    return Ok(boolean(false));
                }
                (value.intrinsic)("gte", vec![(value.get)(&row, "contracts")?, base.clone()])
            }),
        );
        let found = (v.invoke)(find, vec![predicate])?;
        if (v.truthy)(&found) {
            infinite = false;
        }
    }
    if infinite {
        if is(v, (v.get)(&opts, "type")?, "OrderType.StopLoss")? {
            return Ok((v.array)(vec![(v.literal)("pos_loss"), (v.number)(0.)]));
        }
        if is(v, (v.get)(&opts, "type")?, "OrderType.TakeProfit")? {
            return Ok((v.array)(vec![(v.literal)("pos_profit"), (v.number)(0.)]));
        }
    } else {
        if is(v, (v.get)(&opts, "type")?, "OrderType.StopLoss")? {
            return Ok((v.array)(vec![
                (v.literal)("loss_plan"),
                (v.get)(&opts, "amount")?,
            ]));
        }
        if is(v, (v.get)(&opts, "type")?, "OrderType.TakeProfit")? {
            return Ok((v.array)(vec![
                (v.literal)("profit_plan"),
                (v.get)(&opts, "amount")?,
            ]));
        }
    }
    Err(thrown(ops, &h, || {
        error_text(ops, v, "Unknown order type: ", &opts, "type")
    })?)
}
