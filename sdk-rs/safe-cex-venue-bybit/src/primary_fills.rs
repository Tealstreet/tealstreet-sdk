//! USER_TRADES mapping retains native fields until the public Execution projection.
use safe_cex_exchange::js_runtime::value::{Js, Thrown};

pub(crate) fn first(row: &Js, keys: &[&str], fallback: Js) -> Result<Js, Thrown> {
    for key in keys {
        let value = row.get(key)?;
        if !value.is_nullish() {
            return Ok(value);
        }
    }
    Ok(fallback)
}
pub(crate) fn truthy_first(row: &Js, keys: &[&str], fallback: Js) -> Result<Js, Thrown> {
    for key in keys {
        let value = row.get(key)?;
        if value.truthy() {
            return Ok(value);
        }
    }
    Ok(fallback)
}
pub(crate) fn lower(value: Js, trim: bool) -> Result<Js, Thrown> {
    let value = Js::String(value.to_js_string()?);
    let value = if trim {
        value.call_method("trim", vec![])?
    } else {
        value
    };
    value.call_method("toLowerCase", vec![])
}
pub(crate) fn each_row(
    rows: &Js,
    mut body: impl FnMut(Js) -> Result<(), Thrown>,
) -> Result<(), Thrown> {
    let iterator = rows.iterator()?;
    let next = iterator.get("next")?;
    loop {
        let step = next.call(iterator.clone(), vec![])?;
        if !step.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        if step.get("done")?.truthy() {
            return Ok(());
        }
        if let Err(original) = body(step.get("value")?) {
            // IteratorClose preserves a throwing body's original completion.
            let close = iterator.get("return").and_then(|method| {
                if method.is_nullish() {
                    Ok(Js::Undefined)
                } else {
                    method.call(iterator.clone(), vec![])
                }
            });
            drop(close);
            return Err(original);
        }
    }
}
pub(crate) fn append_primary_fills(
    rows: &Js,
    symbol: &Js,
    executions: &Js,
    inverse: bool,
) -> Result<(), Thrown> {
    each_row(rows, |row| {
        let kind = lower(first(&row, &["execType"], Js::str(""))?, true)?;
        if [
            "funding",
            "settle",
            "settlement",
            "delivery",
            "sessionsettlepnl",
        ]
        .iter()
        .any(|s| kind.strict_equals(&Js::str(s)))
        {
            return Ok(());
        }
        let qty = first(&row, &["execQty", "closedSize", "qty"], Js::str("0"))?.parse_float()?;
        let mut price =
            first(&row, &["execPrice", "avgPrice", "price"], Js::str("0"))?.parse_float()?;
        if !price.is_finite() || price <= 0. {
            let value = first(&row, &["execValue", "closedPnl"], Js::str("0"))?.parse_float()?;
            if value.is_finite() && value > 0. && qty.is_finite() && qty > 0. {
                price = value / qty;
            }
        }
        let side = lower(first(&row, &["side", "orderSide"], Js::str(""))?, true)?;
        let buy = side.strict_equals(&Js::str("buy"));
        let sell = side.strict_equals(&Js::str("sell"));
        if !qty.is_finite() || !price.is_finite() || qty <= 0. || price <= 0. || (!buy && !sell) {
            return Ok(());
        }
        let raw_time = first(
            &row,
            &["execTime", "tradeTime", "updatedTime", "createdTime"],
            Js::Undefined,
        )?
        .to_number()?;
        if !raw_time.is_finite() || raw_time <= 0. {
            return Ok(());
        }
        let timestamp = if raw_time > 100000000000. {
            (raw_time / 1000.).floor()
        } else {
            raw_time.floor()
        };
        let id = row.get("execId")?;
        let id = if inverse {
            if id.truthy() {
                Js::String(id.to_js_string()?)
            } else {
                Js::Undefined
            }
        } else {
            id
        };
        let symbol = truthy_first(&row, &["symbol"], symbol.clone())?;
        let order_id = row.get("orderId")?;
        let commission = first(&row, &["execFee", "fee"], Js::str("0"))?
            .parse_float()?
            .abs();
        let asset = row.get("feeCurrency")?;
        let asset = if asset.truthy() {
            asset
        } else {
            let coin = row.get("feeCoin")?;
            if coin.truthy() {
                coin
            } else {
                row.get("quoteCoin")?
            }
        };
        let maker = row.get("isMaker")?;
        let maker = if matches!(maker, Js::Bool(_)) {
            maker
        } else {
            Js::Undefined
        };
        executions.call_method(
            "push",
            vec![Js::object(vec![
                ("id", id),
                ("timestamp", Js::Number(timestamp)),
                ("symbol", symbol),
                ("orderId", order_id),
                ("price", Js::Number(price)),
                ("qty", Js::Number(qty)),
                ("side", Js::str(if buy { "buy" } else { "sell" })),
                ("commission", Js::Number(commission)),
                ("commissionAsset", asset),
                ("maker", maker),
            ])],
        )?;
        Ok(())
    })
}
#[cfg(test)]
mod tests;
