//! Position history uses positive costs and venue-reported per-position funding.
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
pub struct BitunixPositionAdapterDeps<'a> {
    pub account: &'a str,
    pub resolve_settle_currency: &'a mut dyn FnMut(&str) -> String,
    pub realized_is_net_of_funding: bool,
}
pub fn map_bitunix_position_history(
    row: &Value,
    deps: &mut BitunixPositionAdapterDeps<'_>,
) -> Result<Option<PositionRecord>, String> {
    if row.is_null() {
        return Err("Cannot read properties of null (reading 'symbol')".into());
    }
    let symbol = nullish(row.get("symbol"))?;
    if symbol.is_empty() {
        return Ok(None);
    }
    let side = match nullish(row.get("side"))?.to_uppercase().as_str() {
        "BUY" => PositionSide::Long,
        "SELL" => PositionSide::Short,
        _ => return Ok(None),
    };
    let closed_size = num(row.get("qty"))?.abs();
    if closed_size <= 0. || closed_size.is_nan() {
        return Ok(None);
    }
    let maximum = num(row.get("maxQty"))?.abs();
    let fees = num(row.get("fee"))?;
    let funding = num(row.get("funding"))?;
    let realized_pnl = num(row.get("realizedPNL"))? + fees
        - if deps.realized_is_net_of_funding {
            funding
        } else {
            0.
        };
    let opened = num(row.get("ctime"))?;
    let opened_at = (opened != 0.).then_some(opened);
    let closed = num(row.get("mtime"))?;
    let closed_at = (closed != 0.).then_some(closed);
    let hedged = nullish(row.get("positionMode"))?.to_uppercase() == "HEDGE";
    let liquidated = num(row.get("liqQty"))?.abs() > 0.;
    let identity = row
        .get("positionId")
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
    let settle_currency = (deps.resolve_settle_currency)(&symbol);
    let leverage = num(row.get("leverage"))?;
    let venue_id = if safe_cex_num::js_truthy(row.get("positionId")) {
        Some(text(row.get("positionId"))?)
    } else {
        None
    };
    let avg_entry_price = num(row.get("entryPrice"))?;
    let exit = num(row.get("closePrice"))?;
    Ok(Some(PositionRecord {
        key,
        account: deps.account.into(),
        symbol,
        settle_currency,
        side,
        position_side: if !hedged {
            PositionRecordLeg::Both
        } else if side == PositionSide::Long {
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
        funding,
        funding_attributed: true,
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
