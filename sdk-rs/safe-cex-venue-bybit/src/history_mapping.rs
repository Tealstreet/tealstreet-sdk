//! Native order-history fallback mapping, distinct from raw execution fills.
use crate::primary_fills::{each_row, first, lower, truthy_first};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::collections::HashSet;

fn fee_detail(globals: &Js, detail: Js) -> Result<Js, Thrown> {
    if !detail.truthy() {
        return Ok(Js::Undefined);
    }
    let parsed = if detail.is_string() {
        match (|| globals.get("JSON")?.call_method("parse", vec![detail]))() {
            Ok(parsed) => parsed,
            Err(_) => return Ok(Js::Undefined),
        }
    } else {
        detail
    };
    if !parsed.truthy() || parsed.type_of() != "object" {
        return Ok(Js::Undefined);
    }
    let values = globals.get("Object")?.call_method("values", vec![parsed])?;
    let mut sum = 0.;
    each_row(&values, |value| {
        let fee = value.parse_float()?;
        if fee.is_finite() {
            sum += fee.abs();
        }
        Ok(())
    })?;
    Ok(if sum > 0. {
        Js::Number(sum)
    } else {
        Js::Undefined
    })
}
pub fn map_order_history_rows_to_executions(
    globals: &Js,
    rows: &Js,
    symbol: Js,
    order_id: Js,
    from: Js,
    to: Js,
) -> Result<Js, Thrown> {
    let mut mapped = Vec::new();
    each_row(rows, |row| {
        let row_id = first(&row, &["orderId"], Js::str(""))?;
        let row_id = Js::String(row_id.to_js_string()?);
        if order_id.truthy()
            && row_id.truthy()
            && !row_id.strict_equals(&Js::String(order_id.to_js_string()?))
        {
            return Ok(());
        }
        let status = lower(first(&row, &["orderStatus"], Js::str(""))?, false)?;
        if status.truthy()
            && !status
                .call_method("includes", vec![Js::str("filled")])?
                .truthy()
        {
            return Ok(());
        }
        let qty = first(&row, &["cumExecQty", "execQty", "qty"], Js::str("0"))?.parse_float()?;
        if !qty.is_finite() || qty <= 0. {
            return Ok(());
        }
        let mut price =
            first(&row, &["avgPrice", "avgFilledPrice", "price"], Js::str("0"))?.parse_float()?;
        if !price.is_finite() || price <= 0. {
            let value = first(
                &row,
                &["cumExecValue", "cumExecAmt", "cumExecAmount"],
                Js::str("0"),
            )?
            .parse_float()?;
            if value.is_finite() && value > 0. && qty > 0. {
                price = value / qty;
            }
        }
        if !price.is_finite() || price <= 0. {
            return Ok(());
        }
        let side = lower(first(&row, &["side"], Js::str(""))?, false)?;
        let buy = side.strict_equals(&Js::str("buy"));
        if !buy && !side.strict_equals(&Js::str("sell")) {
            return Ok(());
        }
        let time = first(
            &row,
            &["updatedTime", "createdTime", "execTime", "transactionTime"],
            Js::Undefined,
        )?
        .to_number()?;
        if !time.is_finite() || time <= 0. {
            return Ok(());
        }
        let time = if time > 100000000000. {
            time
        } else {
            time * 1000.
        };
        if !from.is_undefined() && time < from.to_number()? {
            return Ok(());
        }
        if !to.is_undefined() && time > to.to_number()? {
            return Ok(());
        }
        let direct_fee =
            first(&row, &["cumExecFee", "execFee", "fee"], Js::str(""))?.parse_float()?;
        let commission = if direct_fee.is_finite() {
            Js::Number(direct_fee.abs())
        } else {
            fee_detail(globals, row.get("cumFeeDetail")?)?
        };
        let mut fields = vec![
            ("timestamp", Js::Number((time / 1000.).floor())),
            ("symbol", truthy_first(&row, &["symbol"], symbol.clone())?),
            (
                "orderId",
                if row_id.truthy() {
                    row_id
                } else {
                    Js::Undefined
                },
            ),
            ("price", Js::Number(price)),
            ("qty", Js::Number(qty)),
            ("side", Js::str(if buy { "buy" } else { "sell" })),
            ("commission", commission),
            (
                "commissionAsset",
                truthy_first(
                    &row,
                    &["feeCurrency", "feeCcy", "settleCoin", "quoteCoin"],
                    Js::Undefined,
                )?,
            ),
        ];
        fields.retain(|(_, value)| !value.is_undefined());
        mapped.push(Js::object(fields));
        Ok(())
    })?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for execution in mapped {
        let mut units = Vec::new();
        for (i, key) in ["orderId", "timestamp", "side", "price", "qty"]
            .iter()
            .enumerate()
        {
            if i != 0 {
                units.push(b':' as u16);
            }
            let value = execution.get(key)?;
            let value = if *key == "orderId" && !value.truthy() {
                Js::str("")
            } else {
                value
            };
            units.extend_from_slice(&value.to_js_string()?);
        }
        if seen.insert(units) {
            out.push(execution);
        }
    }
    out.sort_by(|a, b| {
        b.get("timestamp")
            .unwrap()
            .to_number()
            .unwrap()
            .total_cmp(&a.get("timestamp").unwrap().to_number().unwrap())
    });
    Ok(Js::array(out))
}
#[cfg(test)]
mod tests;
