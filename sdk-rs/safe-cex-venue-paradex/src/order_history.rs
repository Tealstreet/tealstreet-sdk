//! Source correspondence: paradex.order-history.ts; terminal status uses cancel_reason.
use safe_cex_num::{JsCoercionError, js_string, math_max, parse_float};
use safe_cex_types::{HistoricalOrder, OrderSide, OrderStatus, OrderType};
use serde_json::{Map, Value};

const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP_LIMIT", OrderType::StopLoss),
    ("STOP_MARKET", OrderType::StopLoss),
    ("TAKE_PROFIT_LIMIT", OrderType::TakeProfit),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
    ("STOP_LOSS_LIMIT", OrderType::StopLoss),
    ("STOP_LOSS_MARKET", OrderType::StopLoss),
];

// ts: toOptionalNumber
pub fn to_optional_number(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    let text = js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let parsed = parse_float(&text);
    Ok((parsed.is_finite() && parsed != 0.0).then_some(parsed))
}

// ts: toNumber
pub fn to_number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

// ts: mapParadexHistoricalOrder
pub fn map_paradex_historical_order(
    row: &Map<String, Value>,
) -> Result<Option<HistoricalOrder>, JsCoercionError> {
    let text = |key| {
        js_string(
            row.get(key)
                .filter(|v| !v.is_null())
                .or(Some(&Value::String(String::new()))),
        )
    };
    let id = text("id")?;
    let symbol = text("market")?;
    let created_time = to_number(row.get("created_at"))?;
    if id.is_empty() || symbol.is_empty() || created_time == 0.0 {
        return Ok(None);
    }
    let side = text("side")?.to_uppercase();
    if side != "BUY" && side != "SELL" {
        return Ok(None);
    }

    let amount = to_number(row.get("size"))?;
    let remaining = to_number(row.get("remaining_size"))?;
    let filled = math_max(0.0, amount - remaining);

    let raw_status = text("status")?.to_uppercase();
    let cancel_reason = text("cancel_reason")?;
    let status = if matches!(raw_status.as_str(), "NEW" | "OPEN" | "UNTRIGGERED") {
        OrderStatus::Open
    } else if !cancel_reason.is_empty() {
        OrderStatus::Canceled
    } else {
        OrderStatus::Closed
    };

    // Map ALL flags before includes: a later throwing flag is observable even
    // when an earlier flag already says REDUCE_ONLY.
    let flags = match row.get("flags") {
        Some(Value::Array(flags)) => flags
            .iter()
            .map(|f| js_string(Some(f)).map(|s| s.to_uppercase()))
            .collect::<Result<Vec<_>, _>>()?,
        _ => Vec::new(),
    };
    let reduce_only = flags.iter().any(|f| f == "REDUCE_ONLY");
    let raw_type = text("type")?.to_uppercase();
    let kind = if !reduce_only && matches!(raw_type.as_str(), "STOP_LIMIT" | "STOP_MARKET") {
        OrderType::Conditional
    } else {
        // Source uppercases before lookup: no stock Object.prototype spelling
        // is reachable. The oracle checks all twelve spellings through this path.
        ORDER_TYPE
            .iter()
            .find(|(key, _)| *key == raw_type)
            .map(|(_, v)| *v)
            .unwrap_or(OrderType::Limit)
    };
    let price = to_optional_number(row.get("price"))?;
    let average_price = to_optional_number(row.get("avg_fill_price"))?;
    let updated_time = to_number(row.get("last_updated_at"))?;
    Ok(Some(HistoricalOrder {
        id,
        symbol,
        side: if side == "BUY" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        },
        r#type: kind,
        status,
        price,
        average_price,
        amount,
        filled,
        reduce_only: Some(reduce_only),
        created_time,
        updated_time: (updated_time != 0.0).then_some(updated_time),
        position_side: None,
        realized_pnl: None,
        fees: None,
        fee_asset: None,
        rebate: None,
        rebate_asset: None,
    }))
}
