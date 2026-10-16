//! WOO historical-order projection; fractional seconds and zero PnL are load-bearing.
use safe_cex_types::{
    BothPositionSide, ExecutionPositionSide, HistoricalOrder, OrderSide, OrderStatus, OrderType,
    PositionSide,
};
use serde_json::{Map, Value};
const ORDER_TYPE: &[(&str, OrderType)] = &[
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("IOC", OrderType::Limit),
    ("FOK", OrderType::Limit),
    ("POST_ONLY", OrderType::Limit),
    ("ASK", OrderType::Limit),
    ("BID", OrderType::Limit),
];
const ORDER_STATUS: &[(&str, OrderStatus)] = &[
    ("NEW", OrderStatus::Open),
    ("INCOMPLETE", OrderStatus::Open),
    ("PARTIAL_FILLED", OrderStatus::Open),
    ("FILLED", OrderStatus::Closed),
    ("COMPLETED", OrderStatus::Closed),
    ("CANCELLED", OrderStatus::Canceled),
    ("CANCELED", OrderStatus::Canceled),
    ("REJECTED", OrderStatus::Canceled),
];
fn string_or_empty(value: Option<&Value>) -> Result<String, String> {
    match value {
        None | Some(Value::Null) => Ok(String::new()),
        value => safe_cex_num::js_string(value).map_err(|e| e.message.to_owned()),
    }
}
fn parsed(value: Option<&Value>) -> Result<f64, String> {
    Ok(safe_cex_num::parse_float(&string_or_empty(value)?))
}
// ts: toMilliseconds
// ts-bug: finite seconds can overflow to Infinity after the pre-scaling check.
pub fn to_milliseconds(value: Option<&Value>) -> Result<f64, String> {
    let n = parsed(value)?;
    Ok(if n.is_finite() && n > 0.0 {
        (n * 1000.0).floor()
    } else {
        0.0
    })
}
// ts: toOptionalNumber
pub fn to_optional_number(value: Option<&Value>) -> Result<Option<f64>, String> {
    let n = parsed(value)?;
    Ok((n.is_finite() && n != 0.0).then_some(n))
}
// ts: hasValue
pub fn has_value(value: Option<&Value>) -> Result<bool, String> {
    if matches!(value, None | Some(Value::Null)) || value.is_some_and(|v| v.as_str() == Some("")) {
        return Ok(false);
    }
    Ok(parsed(value)?.is_finite())
}
// ts: toNumber
pub fn to_number(value: Option<&Value>) -> Result<f64, String> {
    let n = parsed(value)?;
    Ok(if n.is_finite() { n } else { 0.0 })
}
// ts: mapWooHistoricalOrder
pub fn map_woo_historical_order(
    row: &Map<String, Value>,
    resolve_symbol: impl Fn(&str) -> Result<Option<String>, String>,
) -> Result<Option<HistoricalOrder>, String> {
    let id = string_or_empty(row.get("order_id"))?;
    let market_id = string_or_empty(row.get("symbol"))?;
    let created_time = to_milliseconds(row.get("created_time"))?;
    if id.is_empty() || market_id.is_empty() || created_time == 0.0 {
        return Ok(None);
    }
    let side = string_or_empty(row.get("side"))?.to_uppercase();
    if side != "BUY" && side != "SELL" {
        return Ok(None);
    }
    let position_side = string_or_empty(row.get("position_side"))?.to_uppercase();
    let symbol = resolve_symbol(&market_id)?
        .filter(|s| !s.is_empty())
        .unwrap_or(market_id);
    let kind = string_or_empty(row.get("type"))?.to_uppercase();
    let status = string_or_empty(row.get("status"))?.to_uppercase();
    let optional_present = |key: &str| -> Result<Option<f64>, String> {
        if has_value(row.get(key))? {
            Ok(Some(to_number(row.get(key))?))
        } else {
            Ok(None)
        }
    };
    let asset = |key: &str| -> Result<Option<String>, String> {
        row.get(key)
            .filter(|v| safe_cex_num::js_truthy(Some(v)))
            .map(|v| safe_cex_num::js_string(Some(v)).map_err(|e| e.message.to_owned()))
            .transpose()
    };
    Ok(Some(HistoricalOrder {
        id,
        symbol,
        side: if side == "BUY" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        },
        r#type: ORDER_TYPE
            .iter()
            .find(|(key, _)| *key == kind)
            .map(|(_, v)| *v)
            .unwrap_or(OrderType::Limit),
        status: ORDER_STATUS
            .iter()
            .find(|(key, _)| *key == status)
            .map(|(_, v)| *v)
            .unwrap_or(OrderStatus::Closed),
        price: to_optional_number(row.get("price"))?,
        average_price: to_optional_number(row.get("average_executed_price"))?,
        amount: to_number(row.get("quantity"))?,
        filled: to_number(row.get("executed"))?,
        reduce_only: Some(
            row.get("reduce_only") == Some(&Value::Bool(true))
                || string_or_empty(row.get("reduce_only"))?.to_lowercase() == "true",
        ),
        position_side: Some(match position_side.as_str() {
            "LONG" => ExecutionPositionSide::Side(PositionSide::Long),
            "SHORT" => ExecutionPositionSide::Side(PositionSide::Short),
            _ => ExecutionPositionSide::Both(BothPositionSide::Both),
        }),
        created_time,
        updated_time: Some(to_milliseconds(row.get("updated_time"))?).filter(|v| *v != 0.0),
        realized_pnl: optional_present("realized_pnl")?,
        fees: optional_present("total_fee")?,
        fee_asset: asset("fee_asset")?,
        rebate: optional_present("total_rebate")?,
        rebate_asset: asset("rebate_asset")?,
    }))
}
