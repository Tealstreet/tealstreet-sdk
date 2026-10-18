//! Historical position rows, preserving base units and the venue's negative cost sign.
use safe_cex_types::{
    PositionCloseReason, PositionProvenance, PositionRecord, PositionRecordLeg,
    PositionRecordState, PositionSide, PositionSourceAxis,
};
use serde_json::Value;
fn text(value: Option<&Value>) -> Result<String, String> {
    safe_cex_num::js_string(value).map_err(|e| e.message.into())
}
fn nullish(value: Option<&Value>) -> Result<String, String> {
    value
        .filter(|v| !v.is_null())
        .map(|v| text(Some(v)))
        .unwrap_or_else(|| Ok(String::new()))
}
pub fn num(value: Option<&Value>) -> Result<f64, String> {
    let n = safe_cex_num::parse_float(&nullish(value)?);
    Ok(if n.is_finite() { n } else { 0. })
}
pub struct BlofinPositionAdapterDeps<'a> {
    pub account: &'a str,
    pub resolve_symbol:
        &'a mut dyn FnMut(&str) -> Result<Option<String>, safe_cex_types::StoreReadError>,
    pub resolve_settle_currency:
        &'a mut dyn FnMut(&str) -> Result<Option<String>, safe_cex_types::StoreReadError>,
    pub contract_size: Option<&'a mut dyn FnMut(&str) -> f64>,
}
pub fn map_blofin_position_history(
    row: &Value,
    deps: &mut BlofinPositionAdapterDeps<'_>,
) -> Result<Option<PositionRecord>, crate::reads::MappingError> {
    if row.is_null() {
        return Err("Cannot read properties of null (reading 'instId')".into());
    }
    let instrument = nullish(row.get("instId"))?;
    let Some(symbol) = (deps.resolve_symbol)(&instrument)?.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let side = match nullish(row.get("positionSide"))?.to_lowercase().as_str() {
        "long" => PositionSide::Long,
        "short" => PositionSide::Short,
        _ => return Ok(None),
    };
    let scale = deps
        .contract_size
        .as_mut()
        .map_or(1., |resolve| resolve(&instrument));
    let scale = if scale == 0. || scale.is_nan() {
        1.
    } else {
        scale
    };
    let closed_size = num(row.get("closePositions"))?.abs() * scale;
    if closed_size <= 0. || closed_size.is_nan() {
        return Ok(None);
    }
    let maximum = num(row.get("maxPositions"))?.abs() * scale;
    let fees = -num(row.get("fee"))?;
    let realized_pnl = num(row.get("realizedPnl"))? + fees;
    let closed = num(row.get("updateTime"))?;
    let closed_at = (closed != 0.).then_some(closed);
    let opened = num(row.get("createTime"))?;
    let opened_at = (opened != 0.).then_some(opened);
    let liquidated = num(row.get("liquidationPositions"))?.abs() > 0.;
    let identity = row
        .get("historyId")
        .filter(|v| !v.is_null())
        .map(|v| text(Some(v)))
        .unwrap_or_else(|| {
            Ok(closed_at
                .map(safe_cex_num::js_number_string)
                .unwrap_or_default())
        })?;
    let side_name = if side == PositionSide::Long {
        "long"
    } else {
        "short"
    };
    let key = format!("{}|{symbol}|{side_name}|{identity}", deps.account);
    let settle_currency = (deps.resolve_settle_currency)(&instrument)?
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "USDT".into());
    let leverage = num(row.get("leverage"))?;
    let venue_id = if safe_cex_num::js_truthy(row.get("positionId")) {
        Some(text(row.get("positionId"))?)
    } else {
        None
    };
    let avg_entry_price = num(row.get("openAveragePrice"))?;
    let exit = num(row.get("closeAveragePrice"))?;
    Ok(Some(PositionRecord {
        key,
        account: deps.account.into(),
        symbol,
        settle_currency,
        side,
        position_side: if side == PositionSide::Long {
            PositionRecordLeg::Long
        } else {
            PositionRecordLeg::Short
        },
        state: PositionRecordState::Closed,
        opened_at,
        closed_at,
        closed_size,
        max_size: (maximum > 0.).then_some(maximum),
        leverage: (leverage != 0.).then_some(leverage),
        closed_notional: None,
        venue_id,
        avg_entry_price,
        avg_exit_price: (exit != 0.).then_some(exit),
        realized_pnl,
        fees,
        funding: 0.,
        funding_attributed: false,
        close_reason: Some(if liquidated {
            PositionCloseReason::Liquidation
        } else {
            PositionCloseReason::User
        }),
        provenance: PositionProvenance::Reported,
        source_axis: PositionSourceAxis::ClosedPositions,
        close_count: 1.,
    }))
}
