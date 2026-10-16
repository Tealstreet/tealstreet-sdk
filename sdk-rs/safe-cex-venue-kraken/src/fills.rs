//! Function-by-function port of kraken.fills.ts on the JSON wire domain.
use crate::wire::{decode, text};
use safe_cex_num::{
    JsCoercionError, js_date_parse, js_number, js_truthy, numeric_ingress::JsNumberInput,
};
use safe_cex_types::{Execution, OrderSide};
use serde_json::Value;

pub fn to_seconds(
    value: Option<&Value>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<f64, JsCoercionError> {
    let ms = js_date_parse(&text(value)?, local_to_utc);
    Ok(if ms.is_finite() {
        (ms / 1000.0).floor()
    } else {
        0.0
    })
}
pub fn num(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let parsed = js_number(value)?;
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}
pub fn optional_num(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    if value.is_none_or(|v| v.is_null() || v.as_str() == Some("")) {
        return Ok(None);
    }
    let parsed = js_number(value)?;
    Ok(parsed.is_finite().then_some(parsed))
}
pub fn map_kraken_fill(
    input: JsNumberInput,
    resolve_symbol: &mut impl FnMut(&str) -> Result<Option<String>, Box<dyn std::error::Error>>,
    local_to_utc: &mut impl FnMut(f64) -> f64,
) -> Result<Option<Execution>, Box<dyn std::error::Error>> {
    let row = decode(input)?;
    let id = text(row.get("fill_id"))?;
    let venue_symbol = text(row.get("symbol"))?;
    let timestamp = to_seconds(row.get("fillTime"), local_to_utc)?;
    if id.is_empty() || venue_symbol.is_empty() || timestamp == 0.0 {
        return Ok(None);
    }
    let side = text(row.get("side"))?.to_lowercase();
    if side != "buy" && side != "sell" {
        return Ok(None);
    }
    Ok(Some(Execution {
        id: Some(id),
        timestamp,
        symbol: resolve_symbol(&venue_symbol)?.unwrap_or(venue_symbol),
        order_id: if js_truthy(row.get("order_id")) {
            Some(text(row.get("order_id"))?)
        } else {
            None
        },
        price: num(row.get("price"))?,
        qty: num(row.get("size"))?,
        side: if side == "buy" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        },
        maker: Some(text(row.get("fillType"))?.to_lowercase() == "maker"),
        realized_pnl: optional_num(row.get("realized_pnl"))?,
        commission: None,
        commission_asset: None,
        position_side: None,
        venue_reported_revenue: None,
        venue_reported_revenue_asset: None,
    }))
}
pub fn map_kraken_fills(
    input: JsNumberInput,
    resolve_symbol: &mut impl FnMut(&str) -> Result<Option<String>, Box<dyn std::error::Error>>,
    local_to_utc: &mut impl FnMut(f64) -> f64,
) -> Result<Vec<Execution>, Box<dyn std::error::Error>> {
    let rows = input.decode::<Vec<JsNumberInput>>()?;
    let mut fills = Vec::new();
    for row in rows {
        if let Some(fill) = map_kraken_fill(row, resolve_symbol, local_to_utc)? {
            fills.push(fill);
        }
    }
    Ok(fills)
}
