//! Bybit partial-close aggregation over native receiver values and resolvers.
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::collections::HashMap;

pub struct BybitPositionAdapterDeps(pub Js);
pub fn num(value: &Js) -> Result<f64, Thrown> {
    crate::ledger_mapping::to_number(value)
}
fn nullish(value: Js, fallback: Js) -> Js {
    if value.is_nullish() { fallback } else { value }
}
fn string(value: Js) -> Result<Js, Thrown> {
    Ok(Js::utf16(&value.to_js_string()?))
}
fn key(parts: &[Js]) -> Result<Js, Thrown> {
    let mut units = vec![];
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            units.push(b'|' as u16);
        }
        units.extend_from_slice(&part.to_js_string()?);
    }
    Ok(Js::utf16(&units))
}
fn optional(owner: &Js, property: &str) -> Result<Js, Thrown> {
    if owner.is_nullish() {
        Ok(Js::Undefined)
    } else {
        owner.get(property)
    }
}
fn optional_call(
    deps: &Js,
    property: &str,
    args: impl FnOnce() -> Result<Vec<Js>, Thrown>,
) -> Result<Js, Thrown> {
    let callback = deps.get(property)?;
    if callback.is_nullish() {
        Ok(Js::Undefined)
    } else {
        callback.call(deps.clone(), args()?)
    }
}
fn leg(row: &Js, deps: &Js) -> Result<Js, Thrown> {
    Ok(nullish(
        optional_call(deps, "resolveLeg", || {
            Ok(vec![string(nullish(row.get("orderId")?, Js::str("")))?])
        })?,
        Js::str("both"),
    ))
}
fn each(values: &Js, mut body: impl FnMut(Js) -> Result<(), Thrown>) -> Result<(), Thrown> {
    let iterator = values.iterator()?;
    let next = iterator.get("next")?;
    loop {
        let step = next.call(iterator.clone(), vec![])?;
        if !step.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        if step.get("done")?.truthy() {
            return Ok(());
        }
        if let Err(error) = body(step.get("value")?) {
            let close = iterator.get("return").and_then(|f| {
                if f.is_nullish() {
                    Ok(Js::Undefined)
                } else {
                    f.call(iterator.clone(), vec![])
                }
            });
            // A thrown body completion outranks an IteratorClose failure.
            drop(close);
            return Err(error);
        }
    }
}
fn sort(rows: &mut [Js], property: &str) -> Result<(), Thrown> {
    let mut error = None;
    rows.sort_by(|a, b| {
        if error.is_some() {
            return std::cmp::Ordering::Equal;
        }
        let compared = (|| {
            Ok::<_, Thrown>(
                (num(&a.get(property)?)? - num(&b.get(property)?)?)
                    .partial_cmp(&0.)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        })();
        match compared {
            Ok(order) => order,
            Err(e) => {
                if error.is_none() {
                    error = Some(e);
                }
                std::cmp::Ordering::Equal
            }
        }
    });
    error.map_or(Ok(()), Err)
}
fn close_reason(first: &Js) -> Result<Js, Thrown> {
    let table = Js::object(vec![
        ("Trade", Js::str("user")),
        ("AdlTrade", Js::str("adl")),
        ("BustTrade", Js::str("liquidation")),
        ("Settle", Js::str("settlement")),
    ]);
    if let Js::Object(object) = &table {
        *object.proto.borrow_mut() = Some(Js::Builtin(
            safe_cex_num::PrototypeRealm::current().reference(safe_cex_num::ProtoMember::Prototype),
        ));
    }
    Ok(nullish(
        table.get_property(&string(nullish(first.get("execType")?, Js::str("")))?)?,
        Js::str("user"),
    ))
}
pub fn fold_run(run: &Js, deps: &BybitPositionAdapterDeps) -> Result<Js, Thrown> {
    let deps = &deps.0;
    let first = run.get("0")?;
    let symbol = string(nullish(first.get("symbol")?, Js::str("")))?;
    if !symbol.truthy() {
        return Ok(Js::Null);
    }
    let side = if first.get("side")?.strict_equals(&Js::str("Sell")) {
        "long"
    } else {
        "short"
    };
    let position_side = leg(&first, deps)?;
    let (
        mut closed_size,
        mut fees,
        mut net_pnl,
        mut exit_value,
        mut closed_notional,
        mut closed_at,
    ) = (0., 0., 0., 0., 0., 0.);
    each(run, |row| {
        let closed = num(&row.get("closedSize")?)?.abs();
        closed_size += closed;
        fees += num(&row.get("openFee")?)? + num(&row.get("closeFee")?)?;
        net_pnl += num(&row.get("closedPnl")?)?;
        exit_value += closed * num(&row.get("avgExitPrice")?)?;
        closed_notional += num(&row.get("cumExitValue")?)?;
        let updated = num(&row.get("updatedTime")?)?;
        let time = if updated != 0. {
            updated
        } else {
            num(&row.get("createdTime")?)?
        };
        closed_at = safe_cex_num::math_max(closed_at, time);
        Ok(())
    })?;
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(closed_size > 0.) {
        return Ok(Js::Null);
    }
    let context = optional_call(deps, "resolveContext", || {
        Ok(vec![Js::object(vec![
            ("symbol", symbol.clone()),
            ("positionSide", position_side.clone()),
            ("closedAt", Js::Number(closed_at)),
        ])])
    })?;
    let funding = nullish(optional(&context, "funding")?, Js::Number(0.));
    let funding_attributed = nullish(optional(&context, "fundingAttributed")?, Js::Bool(false));
    let record_key = key(&[
        string(deps.get("account")?)?,
        symbol.clone(),
        string(position_side.clone())?,
        string(nullish(first.get("orderId")?, Js::Number(closed_at)))?,
        Js::Number(closed_at),
    ])?;
    let account = deps.get("account")?;
    let currency = deps
        .get("resolveSettleCurrency")?
        .call(deps.clone(), vec![symbol.clone()])?;
    let opened = optional(&context, "openedAt")?;
    let leverage = num(&first.get("leverage")?)?;
    let order_id = first.get("orderId")?;
    let venue_id = if order_id.truthy() {
        string(first.get("orderId")?)?
    } else {
        Js::Undefined
    };
    let entry_price = num(&first.get("avgEntryPrice")?)?;
    let realized_pnl = net_pnl + fees - funding.to_number()?;
    let reason = close_reason(&first)?;
    let joined = deps.get("resolveLeg")?.truthy();
    let number_or_undefined = |value: f64| {
        if value != 0. && !value.is_nan() {
            Js::Number(value)
        } else {
            Js::Undefined
        }
    };
    Ok(Js::object(vec![
        ("key", record_key),
        ("account", account),
        ("symbol", symbol),
        ("settleCurrency", currency),
        ("side", Js::str(side)),
        ("positionSide", position_side),
        ("state", Js::str("closed")),
        ("openedAt", opened),
        ("closedAt", number_or_undefined(closed_at)),
        ("closedSize", Js::Number(closed_size)),
        ("maxSize", Js::Undefined),
        ("leverage", number_or_undefined(leverage)),
        ("closedNotional", number_or_undefined(closed_notional)),
        ("venueId", venue_id),
        ("avgEntryPrice", Js::Number(entry_price)),
        ("avgExitPrice", Js::Number(exit_value / closed_size)),
        ("realizedPnl", Js::Number(realized_pnl)),
        ("fees", Js::Number(fees)),
        ("funding", funding),
        ("fundingAttributed", funding_attributed),
        ("closeReason", reason),
        ("provenance", Js::str("reported")),
        (
            "sourceAxis",
            Js::str(if joined {
                "closed-positions+orders"
            } else {
                "closed-positions"
            }),
        ),
        ("closeCount", run.get("length")?),
    ]))
}
pub fn aggregate_bybit_closed_pnl(
    rows: &Js,
    deps: &BybitPositionAdapterDeps,
) -> Result<Js, Thrown> {
    let mut indices = HashMap::<Vec<u16>, usize>::new();
    let mut buckets: Vec<Vec<Js>> = vec![];
    each(rows, |row| {
        let position_side = leg(&row, &deps.0)?;
        let group = key(&[
            string(row.get("symbol")?)?,
            string(position_side)?,
            string(row.get("side")?)?,
            Js::Number(num(&row.get("avgEntryPrice")?)?),
        ])?;
        let group = group.to_js_string()?.to_vec();
        if let Some(index) = indices.get(&group) {
            buckets[*index].push(row);
        } else {
            indices.insert(group, buckets.len());
            buckets.push(vec![row]);
        }
        Ok(())
    })?;
    let mut records = vec![];
    for mut bucket in buckets {
        sort(&mut bucket, "updatedTime")?;
        let record = fold_run(&Js::array(bucket), deps)?;
        if record.truthy() {
            records.push(record);
        }
    }
    sort(&mut records, "closedAt")?;
    Ok(Js::array(records))
}
#[cfg(test)]
mod tests;
