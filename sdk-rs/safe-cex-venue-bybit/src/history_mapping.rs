//! Bybit order-history fallback mapping, distinct from raw execution fills.
use crate::{
    api::truthy,
    exchange_state::{number, parse_float},
};
use safe_cex_num::{js_number_string, js_string};
use safe_cex_types::{Execution, OrderSide};
use serde_json::Value;
use std::collections::HashSet;

fn first<'a>(row: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|key| row.get(*key).filter(|v| !v.is_null()))
}
fn text(value: Option<&Value>) -> Result<String, String> {
    match value {
        None => Ok(String::new()),
        Some(value) => js_string(Some(value)).map_err(|e| e.message.into()),
    }
}
fn fee_detail(detail: Option<&Value>) -> Result<Option<f64>, String> {
    let Some(detail) = detail.filter(|v| truthy(v)) else {
        return Ok(None);
    };
    let parsed;
    let detail = if let Some(raw) = detail.as_str() {
        parsed = match serde_json::from_str::<Value>(raw) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        &parsed
    } else {
        detail
    };
    let values: Vec<&Value> = match detail {
        Value::Array(values) => values.iter().collect(),
        Value::Object(values) => crate::api_request::ordered_entries(values)
            .into_iter()
            .map(|(_, v)| v)
            .collect(),
        _ => return Ok(None),
    };
    let mut sum = 0.;
    for value in values {
        let fee = parse_float(Some(value))?;
        if fee.is_finite() {
            sum += fee.abs();
        }
    }
    Ok((sum > 0.).then_some(sum))
}

pub fn map_order_history_rows_to_executions(
    rows: &[Value],
    symbol: &str,
    order_id: Option<&str>,
    from: Option<f64>,
    to: Option<f64>,
) -> Result<Vec<Execution>, String> {
    let mut mapped = vec![];
    for row in rows {
        if row.is_null() {
            return Err("Cannot read properties of null (reading 'orderId')".into());
        }
        let row_order_id = text(first(row, &["orderId"]))?;
        if order_id
            .is_some_and(|id| !id.is_empty() && !row_order_id.is_empty() && row_order_id != id)
        {
            continue;
        }
        let status = text(first(row, &["orderStatus"]))?.to_lowercase();
        // ts-bug: mapOrderHistoryRowsToExecutions accepts any filled substring, including unfilled.
        if !status.is_empty() && !status.contains("filled") {
            continue;
        }
        let qty = parse_float(first(row, &["cumExecQty", "execQty", "qty"]))?;
        if !qty.is_finite() || qty <= 0. {
            continue;
        }
        let mut price = parse_float(first(row, &["avgPrice", "avgFilledPrice", "price"]))?;
        if !price.is_finite() || price <= 0. {
            let value = parse_float(first(row, &["cumExecValue", "cumExecAmt", "cumExecAmount"]))?;
            if value.is_finite() && value > 0. && qty > 0. {
                price = value / qty;
            }
        }
        if !price.is_finite() || price <= 0. {
            continue;
        }
        let side = text(first(row, &["side"]))?.to_lowercase();
        let side = match side.as_str() {
            "buy" => OrderSide::Buy,
            "sell" => OrderSide::Sell,
            _ => continue,
        };
        let timestamp = number(first(
            row,
            &["updatedTime", "createdTime", "execTime", "transactionTime"],
        ))?;
        if !timestamp.is_finite() || timestamp <= 0. {
            continue;
        }
        let timestamp = if timestamp > 100_000_000_000. {
            timestamp
        } else {
            timestamp * 1000.
        };
        if from.is_some_and(|from| timestamp < from) || to.is_some_and(|to| timestamp > to) {
            continue;
        }
        let direct_fee = parse_float(first(row, &["cumExecFee", "execFee", "fee"]))?;
        let commission = if direct_fee.is_finite() {
            Some(direct_fee.abs())
        } else {
            fee_detail(row.get("cumFeeDetail"))?
        };
        let symbol = match row.get("symbol").filter(|v| truthy(v)) {
            Some(value) => value
                .as_str()
                .ok_or("Bybit typed Execution requires string symbol")?,
            None => symbol,
        };
        let asset = ["feeCurrency", "feeCcy", "settleCoin", "quoteCoin"]
            .into_iter()
            .find_map(|key| row.get(key).filter(|v| truthy(v)));
        let asset = asset
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("Bybit typed Execution requires string commissionAsset")
            })
            .transpose()?;
        mapped.push(Execution {
            id: None,
            timestamp: (timestamp / 1000.).floor(),
            symbol: symbol.to_owned(),
            order_id: (!row_order_id.is_empty()).then_some(row_order_id),
            price,
            qty,
            side,
            commission,
            commission_asset: asset,
            position_side: None,
            realized_pnl: None,
            maker: None,
            venue_reported_revenue: None,
            venue_reported_revenue_asset: None,
        });
    }
    let mut seen = HashSet::new();
    mapped.retain(|e| {
        seen.insert(format!(
            "{}:{}:{}:{}:{}",
            e.order_id.as_deref().unwrap_or(""),
            js_number_string(e.timestamp),
            if e.side == OrderSide::Buy {
                "buy"
            } else {
                "sell"
            },
            js_number_string(e.price),
            js_number_string(e.qty)
        ))
    });
    mapped.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
    Ok(mapped)
}

#[cfg(test)]
mod tests;
