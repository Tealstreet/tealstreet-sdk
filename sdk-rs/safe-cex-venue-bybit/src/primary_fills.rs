#![cfg_attr(
    not(test),
    expect(dead_code, reason = "Primary fill mapper awaits fetch orchestration")
)]
//! USER_TRADES is the fills axis, not closed-PnL partial-close records.
use crate::{
    api::truthy,
    exchange_state::{number, parse_float},
};
use safe_cex_num::{js_string, js_trim};
use safe_cex_types::{Execution, OrderSide};
use serde_json::Value;
fn first<'a>(row: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|k| row.get(*k).filter(|v| !v.is_null()))
}
fn text(value: Option<&Value>) -> Result<String, String> {
    match value {
        None => Ok(String::new()),
        Some(v) => js_string(Some(v)).map_err(|e| e.message.into()),
    }
}
fn optional_string(row: &Value, key: &str) -> Result<Option<String>, String> {
    row.get(key)
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("Bybit typed Execution requires string {key}"))
        })
        .transpose()
}
pub(crate) fn append_primary_fills(
    rows: &[Value],
    symbol: &str,
    executions: &mut Vec<Execution>,
) -> Result<(), String> {
    for row in rows {
        if row.is_null() {
            return Err("Cannot read properties of null (reading 'execType')".into());
        }
        let kind = text(first(row, &["execType"]))?;
        if matches!(
            js_trim(&kind).to_lowercase().as_str(),
            "funding" | "settle" | "settlement" | "delivery" | "sessionsettlepnl"
        ) {
            continue;
        }
        let qty =
            parse_float(first(row, &["execQty", "closedSize", "qty"]).or(Some(&Value::Null)))?;
        let mut price =
            parse_float(first(row, &["execPrice", "avgPrice", "price"]).or(Some(&Value::Null)))?;
        if !price.is_finite() || price <= 0. {
            let value =
                parse_float(first(row, &["execValue", "closedPnl"]).or(Some(&Value::Null)))?;
            if value.is_finite() && value > 0. && qty.is_finite() && qty > 0. {
                price = value / qty;
            }
        }
        let side = text(first(row, &["side", "orderSide"]))?;
        let side = match js_trim(&side).to_lowercase().as_str() {
            "buy" => Some(OrderSide::Buy),
            "sell" => Some(OrderSide::Sell),
            _ => None,
        };
        if !qty.is_finite() || !price.is_finite() || qty <= 0. || price <= 0. || side.is_none() {
            continue;
        }
        let raw_time = number(first(
            row,
            &["execTime", "tradeTime", "updatedTime", "createdTime"],
        ))?;
        if !raw_time.is_finite() || raw_time <= 0. {
            continue;
        }
        let timestamp = if raw_time > 100_000_000_000. {
            (raw_time / 1000.).floor()
        } else {
            raw_time.floor()
        };
        let id = optional_string(row, "execId")?;
        let symbol = match row.get("symbol").filter(|v| truthy(v)) {
            Some(v) => v
                .as_str()
                .ok_or("Bybit typed Execution requires string symbol")?,
            None => symbol,
        };
        let order_id = optional_string(row, "orderId")?;
        let commission = match first(row, &["execFee", "fee"]) {
            Some(v) => parse_float(Some(v))?,
            None => 0.,
        }
        .abs();
        let asset = ["feeCurrency", "feeCoin", "quoteCoin"]
            .iter()
            .find_map(|k| row.get(*k).filter(|v| truthy(v)));
        let asset = asset
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or("Bybit typed Execution requires string commissionAsset")
            })
            .transpose()?;
        // A later throwing row leaves earlier appended fills intact, as the page catch does in TS.
        executions.push(Execution {
            id,
            timestamp,
            symbol: symbol.into(),
            order_id,
            price,
            qty,
            side: side.ok_or("unreachable side filter")?,
            commission: Some(commission),
            commission_asset: asset,
            position_side: None,
            realized_pnl: None,
            maker: row.get("isMaker").and_then(Value::as_bool),
            venue_reported_revenue: None,
            venue_reported_revenue_asset: None,
        });
    }
    Ok(())
}
#[cfg(test)]
mod tests;
