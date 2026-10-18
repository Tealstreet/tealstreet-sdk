//! History has its own terminal status map and requires contract-to-base conversion.
use crate::income::{JsCoercionError, nullish_string};
use safe_cex_types::*;
use serde_json::Value;

pub fn to_number(value: &Value) -> Result<f64, JsCoercionError> {
    crate::income::to_number(value)
}
pub fn to_optional_number(value: &Value) -> Result<Option<f64>, JsCoercionError> {
    let parsed = crate::income::parse_float(&nullish_string(value, "")?);
    Ok((parsed.is_finite() && parsed != 0.0).then_some(parsed))
}

pub fn map_okx_historical_order<E: From<JsCoercionError>>(
    row: &serde_json::Map<String, Value>,
    market: Option<&Market>,
    to_base: impl Fn(f64, &Market, Option<f64>) -> Result<f64, E>,
) -> Result<Option<HistoricalOrder>, E> {
    let id = nullish_string(row.get("ordId").unwrap_or(&Value::Null), "")?;
    let instrument = nullish_string(row.get("instId").unwrap_or(&Value::Null), "")?;
    let created_time = safe_cex_num::parse_int(
        &nullish_string(row.get("cTime").unwrap_or(&Value::Null), "0")?,
        Some(10),
    );
    if id.is_empty() || instrument.is_empty() || created_time == 0.0 || created_time.is_nan() {
        return Ok(None);
    }
    let Some(market) = market else {
        return Ok(None);
    };
    let side = match nullish_string(row.get("side").unwrap_or(&Value::Null), "")?
        .to_lowercase()
        .as_str()
    {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => return Ok(None),
    };
    let price = to_optional_number(row.get("px").unwrap_or(&Value::Null))?;
    let average_price = to_optional_number(row.get("avgPx").unwrap_or(&Value::Null))?;
    let reference_price = average_price.or(price);
    let position_side = match nullish_string(row.get("posSide").unwrap_or(&Value::Null), "")?
        .to_lowercase()
        .as_str()
    {
        "long" => ExecutionPositionSide::Side(PositionSide::Long),
        "short" => ExecutionPositionSide::Side(PositionSide::Short),
        _ => ExecutionPositionSide::Both(BothPositionSide::Both),
    };
    // ts-bug: mapOkxHistoricalOrder's plain-object lookups inherit non-enum prototype values.
    // Shared lookup retains the inherited branch; typed output refusal is deferred until source work finishes.
    let type_key = nullish_string(row.get("ordType").unwrap_or(&Value::Null), "")?.to_lowercase();
    let status_key = nullish_string(row.get("state").unwrap_or(&Value::Null), "")?.to_lowercase();
    let type_table = [
        ("market", OrderType::Market),
        ("limit", OrderType::Limit),
        ("post_only", OrderType::Limit),
        ("fok", OrderType::Limit),
        ("ioc", OrderType::Limit),
        ("optimal_limit_ioc", OrderType::Market),
        ("mmp", OrderType::Limit),
        ("mmp_and_post_only", OrderType::Limit),
    ];
    let status_table = [
        ("live", OrderStatus::Open),
        ("partially_filled", OrderStatus::Open),
        ("filled", OrderStatus::Closed),
        ("canceled", OrderStatus::Canceled),
        ("mmp_canceled", OrderStatus::Canceled),
    ];
    let type_hit = safe_cex_num::js_table_lookup(&type_table, &type_key);
    let status_hit = safe_cex_num::js_table_lookup(&status_table, &status_key);
    // Source ?? falls back only on Missing; inherited values do not take it.
    let r#type = match type_hit {
        safe_cex_num::TableHit::Mapped(value) => Some(*value),
        safe_cex_num::TableHit::Missing => Some(OrderType::Limit),
        safe_cex_num::TableHit::Inherited(_) => None,
    };
    let status = match status_hit {
        safe_cex_num::TableHit::Mapped(value) => Some(*value),
        safe_cex_num::TableHit::Missing => Some(OrderStatus::Closed),
        safe_cex_num::TableHit::Inherited(_) => None,
    };
    let amount = to_base(
        to_number(row.get("sz").unwrap_or(&Value::Null))?,
        market,
        reference_price,
    )?;
    let filled = to_base(
        to_number(row.get("accFillSz").unwrap_or(&Value::Null))?,
        market,
        reference_price,
    )?;
    let reduce_only =
        nullish_string(row.get("reduceOnly").unwrap_or(&Value::Null), "")?.to_lowercase() == "true";
    let updated = safe_cex_num::parse_int(
        &nullish_string(row.get("uTime").unwrap_or(&Value::Null), "0")?,
        Some(10),
    );
    let (Some(r#type), Some(status)) = (r#type, status) else {
        return Err(JsCoercionError {
            name: "RepresentationError",
            message: "OKX inherited history field cannot cross the closed typed output boundary",
        }
        .into());
    };
    Ok(Some(HistoricalOrder {
        id,
        symbol: market.symbol.clone(),
        side,
        r#type,
        status,
        price,
        average_price,
        amount,
        filled,
        reduce_only: Some(reduce_only),
        position_side: Some(position_side),
        created_time,
        updated_time: (updated != 0.0 && !updated.is_nan()).then_some(updated),
        realized_pnl: None,
        fees: None,
        fee_asset: None,
        rebate: None,
        rebate_asset: None,
    }))
}
