//! Bybit partial-close aggregation into the shared canonical journal record.
use crate::exchange_state::parse_float;
use safe_cex_num::{js_number_string, js_string};
use serde_json::Value;
use std::collections::HashMap;
pub fn num(value: Option<&Value>) -> Result<f64, String> {
    let number = parse_float(value.filter(|v| !v.is_null()))?;
    Ok(if number.is_finite() { number } else { 0. })
}
fn text(value: Option<&Value>) -> Result<String, String> {
    js_string(value).map_err(|e| e.message.into())
}
fn order_id(row: &Value) -> Result<String, String> {
    row.get("orderId")
        .filter(|v| !v.is_null())
        .map(|v| text(Some(v)))
        .unwrap_or_else(|| Ok(String::new()))
}
fn group_rows(
    rows: &[Value],
    mut resolve_leg: Option<&mut dyn FnMut(&str) -> Option<String>>,
) -> Result<Vec<Vec<Value>>, String> {
    let mut indices = HashMap::<String, usize>::new();
    let mut buckets: Vec<Vec<Value>> = vec![];
    for row in rows {
        if row.is_null() {
            return Err(format!(
                "Cannot read properties of null (reading '{}')",
                if resolve_leg.is_some() {
                    "orderId"
                } else {
                    "symbol"
                }
            ));
        }
        let leg = match resolve_leg.as_mut() {
            Some(resolve) => resolve(&order_id(row)?).unwrap_or_else(|| "both".into()),
            None => "both".into(),
        };
        let key = format!(
            "{}|{}|{}|{}",
            text(row.get("symbol"))?,
            leg,
            text(row.get("side"))?,
            js_number_string(num(row.get("avgEntryPrice"))?)
        );
        if let Some(index) = indices.get(&key) {
            buckets[*index].push(row.clone());
        } else {
            indices.insert(key, buckets.len());
            buckets.push(vec![row.clone()]);
        }
    }
    Ok(buckets)
}
fn sort_run(bucket: &mut [Value]) -> Result<(), String> {
    let mut failure = None;
    bucket.sort_by(|a, b| {
        if failure.is_some() {
            return std::cmp::Ordering::Equal;
        }
        match num(a.get("updatedTime")).and_then(|a| num(b.get("updatedTime")).map(|b| (a, b))) {
            Ok((a, b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
            Err(error) => {
                failure = Some(error);
                std::cmp::Ordering::Equal
            }
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}
#[cfg(test)]
fn buckets(
    rows: &[Value],
    mut resolve: impl FnMut(&str) -> Option<String>,
) -> Result<Vec<Vec<Value>>, String> {
    let mut runs = group_rows(rows, Some(&mut resolve))?;
    for run in &mut runs {
        sort_run(run)?;
    }
    Ok(runs)
}

// Intermediate numeric accumulation, not a second canonical journal model.
pub(crate) struct FoldTotals {
    pub closed_size: f64,
    pub fees: f64,
    pub net_pnl: f64,
    pub exit_value: f64,
    pub closed_notional: f64,
    pub closed_at: f64,
}
pub(crate) fn totals(run: &[Value]) -> Result<FoldTotals, String> {
    let mut totals = FoldTotals {
        closed_size: 0.,
        fees: 0.,
        net_pnl: 0.,
        exit_value: 0.,
        closed_notional: 0.,
        closed_at: 0.,
    };
    for row in run {
        if row.is_null() {
            return Err("Cannot read properties of null (reading 'closedSize')".into());
        }
        let closed = num(row.get("closedSize"))?.abs();
        totals.closed_size += closed;
        totals.fees += num(row.get("openFee"))? + num(row.get("closeFee"))?;
        totals.net_pnl += num(row.get("closedPnl"))?;
        totals.exit_value += closed * num(row.get("avgExitPrice"))?;
        totals.closed_notional += num(row.get("cumExitValue"))?;
        let updated = num(row.get("updatedTime"))?;
        let time = if updated != 0. {
            updated
        } else {
            num(row.get("createdTime"))?
        };
        totals.closed_at = totals.closed_at.max(time);
    }
    Ok(totals)
}
#[cfg(test)]
mod tests;

use safe_cex_types::PositionSide;
use safe_cex_types::position_record::{
    PositionCloseReason, PositionProvenance, PositionRecord, PositionRecordLeg,
    PositionRecordState, PositionSourceAxis,
};

pub struct BybitPositionContext {
    pub opened_at: Option<f64>,
    pub funding: f64,
    pub funding_attributed: bool,
}
pub struct BybitPositionAdapterDeps<'a> {
    pub account: &'a str,
    pub resolve_settle_currency: &'a mut dyn FnMut(&str) -> String,
    pub resolve_leg: Option<&'a mut dyn FnMut(&str) -> Option<PositionRecordLeg>>,
    pub resolve_context:
        Option<&'a mut dyn FnMut(&str, PositionRecordLeg, f64) -> Option<BybitPositionContext>>,
}
fn leg_text(leg: PositionRecordLeg) -> &'static str {
    match leg {
        PositionRecordLeg::Long => "long",
        PositionRecordLeg::Short => "short",
        PositionRecordLeg::Both => "both",
    }
}
fn resolve_leg(
    row: &Value,
    deps: &mut BybitPositionAdapterDeps<'_>,
) -> Result<PositionRecordLeg, String> {
    // Optional-call short circuits before evaluating String(orderId).
    match deps.resolve_leg.as_mut() {
        Some(resolve) => Ok(resolve(&order_id(row)?).unwrap_or(PositionRecordLeg::Both)),
        None => Ok(PositionRecordLeg::Both),
    }
}

pub fn fold_run(
    run: &[Value],
    deps: &mut BybitPositionAdapterDeps<'_>,
) -> Result<Option<PositionRecord>, String> {
    let first = run
        .first()
        .ok_or("Cannot read properties of undefined (reading 'symbol')")?;
    if first.is_null() {
        return Err("Cannot read properties of null (reading 'symbol')".into());
    }
    let symbol = text(
        first
            .get("symbol")
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    if symbol.is_empty() {
        return Ok(None);
    }
    let side = if first.get("side").and_then(Value::as_str) == Some("Sell") {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    let leg = resolve_leg(first, deps)?;
    let total = totals(run)?;
    if total.closed_size <= 0. || total.closed_size.is_nan() {
        return Ok(None);
    }
    let context = deps
        .resolve_context
        .as_mut()
        .and_then(|resolve| resolve(&symbol, leg, total.closed_at));
    let funding = context.as_ref().map_or(0., |c| c.funding);
    let key_id = first
        .get("orderId")
        .filter(|v| !v.is_null())
        .map(|v| text(Some(v)))
        .unwrap_or_else(|| Ok(js_number_string(total.closed_at)))?;
    let key = format!(
        "{}|{symbol}|{}|{key_id}|{}",
        deps.account,
        leg_text(leg),
        js_number_string(total.closed_at)
    );
    let settle_currency = (deps.resolve_settle_currency)(&symbol);
    let leverage = num(first.get("leverage"))?;
    let venue_id = if safe_cex_num::js_truthy(first.get("orderId")) {
        Some(text(first.get("orderId"))?)
    } else {
        None
    };
    let avg_entry_price = num(first.get("avgEntryPrice"))?;
    let reason = text(
        first
            .get("execType")
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let close_reason = match safe_cex_num::js_table_lookup(
        &[
            ("Trade", PositionCloseReason::User),
            ("AdlTrade", PositionCloseReason::Adl),
            ("BustTrade", PositionCloseReason::Liquidation),
            ("Settle", PositionCloseReason::Settlement),
        ],
        &reason,
    ) {
        safe_cex_num::TableHit::Mapped(reason) => *reason,
        safe_cex_num::TableHit::Missing => PositionCloseReason::User,
        safe_cex_num::TableHit::Inherited(_) => {
            return Err(
                "RepresentationError: inherited closeReason is outside PositionCloseReason".into(),
            );
        }
    };
    Ok(Some(PositionRecord {
        key,
        account: deps.account.into(),
        symbol,
        settle_currency,
        side,
        position_side: leg,
        state: PositionRecordState::Closed,
        opened_at: context.as_ref().and_then(|c| c.opened_at),
        closed_at: (total.closed_at != 0.).then_some(total.closed_at),
        closed_size: total.closed_size,
        max_size: None,
        leverage: (leverage != 0.).then_some(leverage),
        closed_notional: (total.closed_notional != 0.).then_some(total.closed_notional),
        venue_id,
        avg_entry_price,
        avg_exit_price: Some(total.exit_value / total.closed_size),
        realized_pnl: total.net_pnl + total.fees - funding,
        fees: total.fees,
        funding,
        funding_attributed: context.as_ref().is_some_and(|c| c.funding_attributed),
        close_reason: Some(close_reason),
        provenance: PositionProvenance::Reported,
        source_axis: if deps.resolve_leg.is_some() {
            PositionSourceAxis::ClosedPositionsOrders
        } else {
            PositionSourceAxis::ClosedPositions
        },
        close_count: run.len() as f64,
    }))
}

pub fn aggregate_bybit_closed_pnl(
    rows: &[Value],
    deps: &mut BybitPositionAdapterDeps<'_>,
) -> Result<Vec<PositionRecord>, String> {
    // This preserves the documented identical-entry reopen collision in the source.
    let runs = match deps.resolve_leg.as_mut() {
        Some(resolve) => group_rows(
            rows,
            Some(&mut |id| resolve(id).map(|leg| leg_text(leg).into())),
        )?,
        None => group_rows(rows, None)?,
    };
    let mut records = Vec::new();
    let mut refusal = None;
    for mut run in runs {
        sort_run(&mut run)?;
        match fold_run(&run, deps) {
            Ok(Some(record)) => records.push(record),
            Ok(None) => {}
            Err(error) if error.starts_with("RepresentationError:") => refusal = Some(error),
            Err(error) => return Err(error),
        }
    }
    if let Some(error) = refusal {
        return Err(error);
    }
    records.sort_by(|a, b| {
        (a.closed_at.unwrap_or(0.) - b.closed_at.unwrap_or(0.))
            .partial_cmp(&0.)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(records)
}
