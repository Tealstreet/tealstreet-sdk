//! Canonical derived records from closed intervals; absent PnL refuses a record.
use crate::position_intervals::{FundingAttribution, PositionInterval};
use safe_cex_types::{PositionProvenance, PositionRecord, PositionRecordState, PositionSourceAxis};

pub struct FoldPositionAdapterDeps<'a> {
    pub account: &'a str,
    pub resolve_settle_currency:
        &'a mut dyn FnMut(&str) -> Result<String, crate::runtime::ExchangeError>,
    pub resolve_funding: Option<&'a mut dyn FnMut(&PositionInterval) -> FundingAttribution>,
}
pub fn map_folded_interval(
    interval: &PositionInterval,
    deps: &mut FoldPositionAdapterDeps<'_>,
) -> Result<Option<PositionRecord>, crate::runtime::ExchangeError> {
    let Some(closed_at) = interval.closed_at else {
        return Ok(None);
    };
    if interval.closed_size <= 0. || interval.closed_size.is_nan() {
        return Ok(None);
    }
    let Some(realized_pnl) = interval.realized_pnl else {
        return Ok(None);
    };
    let funding = deps
        .resolve_funding
        .as_mut()
        .map(|resolve| resolve(interval));
    let leg = match interval.position_side {
        safe_cex_types::PositionRecordLeg::Long => "long",
        safe_cex_types::PositionRecordLeg::Short => "short",
        safe_cex_types::PositionRecordLeg::Both => "both",
    };
    let key = format!(
        "{}|{}|{leg}|{}|{}",
        deps.account,
        interval.symbol,
        safe_cex_num::js_number_string(interval.opened_at),
        safe_cex_num::js_number_string(closed_at)
    );
    let settle_currency = (deps.resolve_settle_currency)(&interval.symbol)?;
    Ok(Some(PositionRecord {
        key,
        account: deps.account.into(),
        symbol: interval.symbol.clone(),
        settle_currency,
        side: interval.side,
        position_side: interval.position_side,
        state: PositionRecordState::Closed,
        opened_at: interval.complete.then_some(interval.opened_at),
        closed_at: Some(closed_at),
        closed_size: interval.closed_size,
        max_size: Some(interval.max_size),
        leverage: None,
        closed_notional: None,
        venue_id: None,
        avg_entry_price: interval.avg_entry_price,
        avg_exit_price: (interval.avg_exit_price != 0. && !interval.avg_exit_price.is_nan())
            .then_some(interval.avg_exit_price),
        realized_pnl,
        fees: interval.fees,
        funding: funding.as_ref().map_or(0., |f| f.funding),
        funding_attributed: funding.as_ref().is_some_and(|f| f.attributed),
        close_reason: None,
        provenance: PositionProvenance::Derived,
        source_axis: PositionSourceAxis::Fills,
        close_count: 1.,
    }))
}
