//! Honest venue/derived provenance and independence; no duplicate fills fetch.
use crate::{
    eager::start_eager,
    runtime::{ExchangeFuture, ExchangeRuntime},
};
use safe_cex_types::{ClosedPnlEntry, DataProvenance, HistoricalTradeOptions};
use std::rc::Rc;
#[derive(Debug, Clone)]
pub struct ClosedPositionsResult {
    pub entries: Vec<ClosedPnlEntry>,
    pub provenance: DataProvenance,
    pub independently_verifiable: bool,
    pub unavailable_reason: Option<String>,
}
type ClosedPnlFetcher =
    Rc<dyn Fn(HistoricalTradeOptions) -> ExchangeFuture<'static, Vec<ClosedPnlEntry>>>;
#[derive(Clone)]
pub struct ClosedPositionSource {
    pub runtime: Rc<dyn ExchangeRuntime>,
    pub fetch_closed_pnl: ClosedPnlFetcher,
    pub derive_closed_positions: Option<ClosedPnlFetcher>,
    pub closed_positions_share_ledger_source: Option<bool>,
}

pub fn resolve_closed_positions(
    source: &ClosedPositionSource,
    options: HistoricalTradeOptions,
    venue_reported_unavailable: bool,
) -> ExchangeFuture<'static, ClosedPositionsResult> {
    let runtime = source.runtime.clone();
    let source = source.clone();
    start_eager(&runtime, async move {
        if !venue_reported_unavailable {
            return Ok(ClosedPositionsResult {
                entries: start_eager(&source.runtime, (source.fetch_closed_pnl)(options)).await?,
                provenance: DataProvenance::Venue,
                independently_verifiable: true,
                unavailable_reason: None,
            });
        }
        if let Some(derive) = &source.derive_closed_positions {
            let mut entries = start_eager(&source.runtime, derive(options)).await?;
            for entry in &mut entries {
                entry.provenance = DataProvenance::Derived;
            }
            return Ok(ClosedPositionsResult {
                entries,
                provenance: DataProvenance::Derived,
                independently_verifiable: !source
                    .closed_positions_share_ledger_source
                    .unwrap_or(true),
                unavailable_reason: None,
            });
        }
        Ok(ClosedPositionsResult {
            entries: vec![],
            provenance: DataProvenance::Derived,
            independently_verifiable: false,
            unavailable_reason: Some(
                "venue reports no closed positions and no derivation is implemented".into(),
            ),
        })
    })
}
