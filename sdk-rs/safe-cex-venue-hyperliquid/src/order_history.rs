//! Historical-order transitions and mapping; preserve source table branches.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{TableHit, js_string, js_table_lookup, math_max, parse_float};
use safe_cex_types::{HistoricalOrder, OrderSide, OrderStatus, OrderType};
use serde_json::Value;
use std::collections::HashMap;

const SIDE: &[(&str, OrderSide)] = &[("A", OrderSide::Sell), ("B", OrderSide::Buy)];
const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("limit", OrderType::Limit),
    ("market", OrderType::Market),
    ("stop market", OrderType::StopLoss),
    ("stop limit", OrderType::StopLoss),
    ("take profit market", OrderType::TakeProfit),
    ("take profit limit", OrderType::TakeProfit),
];

fn text(value: Option<&Value>) -> Result<String, ExchangeError> {
    match value.filter(|value| !value.is_null()) {
        None => Ok(String::new()),
        Some(value) => js_string(Some(value)).map_err(|error| {
            let mut result = ExchangeError::new(error.message);
            result.name = error.name.into();
            result
        }),
    }
}
pub fn to_number(value: Option<&Value>) -> Result<f64, ExchangeError> {
    let parsed = parse_float(&text(value)?);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}
pub fn to_optional_number(value: Option<&Value>) -> Result<Option<f64>, ExchangeError> {
    let parsed = to_number(value)?;
    Ok((parsed != 0.0).then_some(parsed))
}
pub fn map_status(status: &str) -> OrderStatus {
    match status {
        "open" | "triggered" => OrderStatus::Open,
        "filled" => OrderStatus::Closed,
        _ => OrderStatus::Canceled,
    }
}
pub fn status_rank(status: &str) -> u8 {
    match status {
        "open" | "triggered" => 0,
        "filled" => 2,
        _ => 1,
    }
}
pub fn is_later_state(candidate: &Value, incumbent: &Value) -> Result<bool, ExchangeError> {
    if candidate.is_null() {
        let mut error =
            ExchangeError::new("Cannot read properties of null (reading 'statusTimestamp')");
        error.name = "TypeError".into();
        return Err(error);
    }
    let candidate_time = to_number(candidate.get("statusTimestamp"))?;
    if incumbent.is_null() {
        let mut error =
            ExchangeError::new("Cannot read properties of null (reading 'statusTimestamp')");
        error.name = "TypeError".into();
        return Err(error);
    }
    let incumbent_time = to_number(incumbent.get("statusTimestamp"))?;
    if candidate_time != incumbent_time {
        return Ok(candidate_time > incumbent_time);
    }
    Ok(status_rank(&text(candidate.get("status"))?) > status_rank(&text(incumbent.get("status"))?))
}
pub fn collapse_hyperliquid_order_history(rows: &[Value]) -> Result<Vec<Value>, ExchangeError> {
    let mut indices = HashMap::<String, usize>::new();
    let mut latest: Vec<Value> = Vec::new();
    for row in rows {
        let oid = text(row.get("order").and_then(|order| order.get("oid")))?;
        if oid.is_empty() {
            continue;
        }
        if let Some(index) = indices.get(&oid).copied() {
            if is_later_state(row, &latest[index])? {
                latest[index] = row.clone();
            }
        } else {
            indices.insert(oid, latest.len());
            latest.push(row.clone());
        }
    }
    Ok(latest)
}
pub fn map_hyperliquid_historical_order(
    row: &Value,
    mut resolve_symbol: impl FnMut(&str) -> Result<Option<String>, ExchangeError>,
) -> Result<Option<HistoricalOrder>, ExchangeError> {
    let order = row.get("order");
    let field = |name| order.and_then(|order| order.get(name));
    let id = text(field("oid"))?;
    let coin = text(field("coin"))?;
    let created_time = to_number(field("timestamp"))?;
    if id.is_empty() || coin.is_empty() || created_time == 0.0 {
        return Ok(None);
    }
    let side_key = text(field("side"))?;
    let side_hit = js_table_lookup(SIDE, &side_key);
    if !side_hit.is_truthy_by(|_| true) {
        return Ok(None);
    }
    let amount = to_number(field("origSz"))?;
    let remaining = to_number(field("sz"))?;
    let filled = math_max(0.0, amount - remaining);
    let symbol = resolve_symbol(&coin)?
        .filter(|symbol| !symbol.is_empty())
        .unwrap_or(coin);
    let type_key = text(field("orderType"))?.to_lowercase();
    let type_hit = js_table_lookup(ORDER_TYPE, &type_key);
    let status = map_status(&text(row.get("status"))?);
    let price = to_optional_number(field("limitPx"))?;
    let updated = to_number(row.get("statusTimestamp"))?;
    let side = match side_hit {
        TableHit::Mapped(side) => *side,
        // ts-bug: prototype hits bypass the source !side guard; the typed output refuses the record.
        TableHit::Inherited(member) => {
            return Err(ExchangeError::new(format!(
                "HistoricalOrder.side cannot carry inherited {}",
                member.js_string()
            )));
        }
        TableHit::Missing => return Ok(None),
    };
    let order_type = if type_hit.is_nullish_by(|_| false) {
        OrderType::Limit
    } else {
        match type_hit {
            TableHit::Mapped(kind) => *kind,
            // ts-bug: inherited type is not nullish, so the source ?? fallback does not run.
            TableHit::Inherited(member) => {
                return Err(ExchangeError::new(format!(
                    "HistoricalOrder.type cannot carry inherited {}",
                    member.js_string()
                )));
            }
            TableHit::Missing => OrderType::Limit,
        }
    };
    Ok(Some(HistoricalOrder {
        id,
        symbol,
        side,
        r#type: order_type,
        status,
        price,
        average_price: None,
        amount,
        filled,
        reduce_only: Some(field("reduceOnly") == Some(&Value::Bool(true))),
        position_side: None,
        created_time,
        updated_time: (updated != 0.0).then_some(updated),
        realized_pnl: None,
        fees: None,
        fee_asset: None,
        rebate: None,
        rebate_asset: None,
    }))
}
