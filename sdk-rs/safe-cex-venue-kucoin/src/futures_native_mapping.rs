use crate::{
    public_streams::StreamOps,
    spot_private_class::{PrivateClassOps, call, fallback},
};
fn member<V: Clone, E>(
    h: &StreamOps<V, E>,
    value: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let function = (h.get)(&value, key)?;
    (h.intrinsic)("call-with-this", vec![function, value, (h.array)(args()?)])
}
fn index<V, E>(
    h: &StreamOps<V, E>,
    table: &str,
    key: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    let object = (h.capture)(table)?;
    (h.intrinsic)("get-index", vec![object, key()?])
}
fn parsed<V, E>(h: &StreamOps<V, E>, value: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    call(h, "parseFloat", || Ok(vec![value()?]))
}
fn float_field<V, E>(h: &StreamOps<V, E>, row: &V, key: &str) -> Result<V, E> {
    parsed(h, || {
        fallback(h, (h.get)(row, key)?, || Ok((h.literal)("0")))
    })
}
fn assign<V: Clone, E>(h: &StreamOps<V, E>, row: &V, key: &str, value: V) -> Result<(), E> {
    (h.intrinsic)("set-index", vec![row.clone(), (h.literal)(key), value])?;
    Ok(())
}
fn attached<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &StreamOps<V, E>,
    order: &V,
    price: V,
    loss: bool,
) -> Result<V, E> {
    let out = (h.object)(vec![]);
    (h.intrinsic)("spread-into", vec![out.clone(), order.clone()])?;
    let mut id = (ops.template)(&(h.get)(order, "id")?)?;
    id.extend(if loss { "__stop_loss" } else { "__take_profit" }.encode_utf16());
    (h.intrinsic)(
        "define-field",
        vec![out.clone(), (h.literal)("id"), (ops.string_value)(id)],
    )?;
    let buy = (h.equal)(&(h.get)(order, "side")?, &(h.capture)("OrderSide.Buy")?);
    (h.intrinsic)(
        "define-field",
        vec![
            out.clone(),
            (h.literal)("side"),
            (h.capture)(if buy {
                "OrderSide.Sell"
            } else {
                "OrderSide.Buy"
            })?,
        ],
    )?;
    (h.intrinsic)(
        "define-field",
        vec![
            out.clone(),
            (h.literal)("type"),
            (h.capture)(if loss {
                "OrderType.StopLoss"
            } else {
                "OrderType.TakeProfit"
            })?,
        ],
    )?;
    let value = parsed(h, || Ok(price))?;
    (h.intrinsic)(
        "define-field",
        vec![out.clone(), (h.literal)("price"), value],
    )?;
    (h.intrinsic)(
        "define-field",
        vec![out.clone(), (h.literal)("reduceOnly"), (ops.boolean)(true)],
    )?;
    Ok(out)
}
pub fn map_order<V: Clone, E>(
    ops: &PrivateClassOps<V, E>,
    h: &StreamOps<V, E>,
    row: V,
) -> Result<V, E> {
    let raw_timestamp = fallback(h, (h.get)(&row, "updateTime")?, || {
        fallback(h, (h.get)(&row, "ts")?, || {
            fallback(h, (h.get)(&row, "orderTime")?, || {
                (h.get)(&row, "createdAt")
            })
        })
    })?;
    let last_update = if (h.truthy)(&raw_timestamp) {
        call(h, "parseInt", || {
            let text = (ops.string_value)((ops.template)(&raw_timestamp)?);
            let slice = member(h, text, "slice", || {
                Ok(vec![(h.number)(0.), (h.number)(13.)])
            })?;
            Ok(vec![slice, (h.number)(10.)])
        })?
    } else {
        call(h, "virtualClock.getCurrentTimeMs", || Ok(vec![]))?
    };
    let raw_type = fallback(h, (h.get)(&row, "orderType")?, || (h.get)(&row, "type"))?;
    let symbol = fallback(
        h,
        call(h, "this.toInternalSymbol", || {
            Ok(vec![(h.get)(&row, "symbol")?])
        })?,
        || (h.get)(&row, "symbol"),
    )?;
    let contract = call(h, "this.contractSizeFor", || Ok(vec![symbol.clone()]))?;
    let size = float_field(h, &row, "size")?;
    let filled = float_field(h, &row, "filledSize")?;
    let first = (h.get)(&row, "remainSize")?;
    let is_null = (h.intrinsic)("strict-null", vec![first])?;
    let remaining =
        if !(h.truthy)(&is_null) && !(h.equal)(&(h.get)(&row, "remainSize")?, &(h.undefined)()) {
            parsed(h, || {
                Ok((ops.string_value)((ops.template)(&(h.get)(
                    &row,
                    "remainSize",
                )?)?))
            })?
        } else {
            call(h, "subtract", || Ok(vec![size.clone(), filled.clone()]))?
        };
    let id = fallback(h, (h.get)(&row, "id")?, || (h.get)(&row, "orderId"))?;
    let status = fallback(
        h,
        index(h, "ORDER_STATUS", || (h.get)(&row, "status"))?,
        || {
            let table = (h.capture)("ORDER_STATUS")?;
            (h.get)(&table, "new")
        },
    )?;
    let ty = fallback(h, index(h, "ORDER_TYPE", || Ok(raw_type.clone()))?, || {
        (h.capture)(if (h.equal)(&raw_type, &(h.literal)("market")) {
            "OrderType.Market"
        } else {
            "OrderType.Limit"
        })
    })?;
    let side = (h.capture)(if (h.equal)(&(h.get)(&row, "side")?, &(h.literal)("buy")) {
        "OrderSide.Buy"
    } else {
        "OrderSide.Sell"
    })?;
    let price = parsed(h, || {
        fallback(h, (h.get)(&row, "price")?, || {
            fallback(h, (h.get)(&row, "orderPrice")?, || Ok((h.literal)("0")))
        })
    })?;
    let amount = call(h, "multiply", || Ok(vec![size, contract.clone()]))?;
    let filled_base = call(h, "multiply", || Ok(vec![filled.clone(), contract.clone()]))?;
    let reduce_only = (ops.boolean)((h.equal)(
        &(h.get)(&row, "reduceOnly")?,
        &(ops.boolean)(true),
    ));
    let remaining_base = call(h, "multiply", || Ok(vec![remaining.clone(), contract]))?;
    let tif = fallback(
        h,
        index(h, "TIME_IN_FORCE", || (h.get)(&row, "timeInForce"))?,
        || (h.capture)("OrderTimeInForce.GoodTillCancel"),
    )?;
    let order = (h.object)(vec![
        ("id", id),
        ("status", status),
        ("symbol", symbol),
        ("type", ty),
        ("side", side),
        ("price", price),
        ("amount", amount),
        ("filled", filled_base),
        ("lastUpdate", last_update),
        ("reduceOnly", reduce_only),
        ("remaining", remaining_base),
        ("timeInForce", tif),
        (
            "extra",
            (h.object)(vec![
                ("remainingNative", remaining),
                ("filledInPlacementUnits", filled),
            ]),
        ),
        ("info", row.clone()),
    ]);
    let stop_true = (h.equal)(&(h.get)(&row, "stop")?, &(ops.boolean)(true));
    let stop = stop_true
        || {
            let string = (h.intrinsic)("typeof-string", vec![(h.get)(&row, "stop")?])?;
            (h.truthy)(&string) && !(h.equal)(&(h.get)(&row, "stop")?, &(h.literal)(""))
        }
        || (h.equal)(&raw_type, &(h.literal)("stop"));
    if stop {
        let converted = call(h, "String", || {
            Ok(vec![fallback(h, (h.get)(&row, "stopType")?, || {
                fallback(h, (h.get)(&row, "stop")?, || Ok((h.literal)("")))
            })?])
        })?;
        let stop_type = member(h, converted, "toLowerCase", || Ok(vec![]))?;
        if ["down", "loss", "sl"]
            .iter()
            .any(|key| (h.equal)(&stop_type, &(h.literal)(key)))
        {
            assign(h, &order, "type", (h.capture)("OrderType.StopLoss")?)?;
        } else if ["up", "profit", "tp"]
            .iter()
            .any(|key| (h.equal)(&stop_type, &(h.literal)(key)))
        {
            assign(h, &order, "type", (h.capture)("OrderType.TakeProfit")?)?;
        }
        let value = float_field(h, &row, "stopPrice")?;
        assign(h, &order, "price", value)?;
    }
    if (h.equal)(&(h.get)(&row, "stop")?, &(ops.boolean)(true))
        && (h.equal)(&(h.get)(&row, "stopPriceType")?, &(h.literal)("TP"))
    {
        assign(
            h,
            &order,
            "type",
            (h.capture)("OrderType.TrailingStopLoss")?,
        )?;
    }
    let orders = (h.array)(vec![order.clone()]);
    let stop_loss = fallback(h, (h.get)(&row, "stopLoss")?, || {
        let buy = (h.equal)(&(h.get)(&order, "side")?, &(h.capture)("OrderSide.Buy")?);
        (h.get)(
            &row,
            if buy {
                "triggerStopDownPrice"
            } else {
                "triggerStopUpPrice"
            },
        )
    })?;
    let take_profit = fallback(h, (h.get)(&row, "takeProfit")?, || {
        let buy = (h.equal)(&(h.get)(&order, "side")?, &(h.capture)("OrderSide.Buy")?);
        (h.get)(
            &row,
            if buy {
                "triggerStopUpPrice"
            } else {
                "triggerStopDownPrice"
            },
        )
    })?;
    if (h.truthy)(&stop_loss) {
        member(h, orders.clone(), "push", || {
            Ok(vec![attached(ops, h, &order, stop_loss, true)?])
        })?;
    }
    if (h.truthy)(&take_profit) {
        member(h, orders.clone(), "push", || {
            Ok(vec![attached(ops, h, &order, take_profit, false)?])
        })?;
    }
    Ok(orders)
}
