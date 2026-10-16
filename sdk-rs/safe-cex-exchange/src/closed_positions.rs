//! Honest venue/derived provenance and independence; no duplicate fills fetch.
use crate::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_types::{ClosedPnlEntry, DataProvenance, HistoricalTradeOptions};
#[derive(Debug, Clone)]
pub struct ClosedPositionsResult {
    pub entries: Vec<ClosedPnlEntry>,
    pub provenance: DataProvenance,
    pub independently_verifiable: bool,
    pub unavailable_reason: Option<String>,
}
type ClosedPnlFetcher<'a> =
    Box<dyn Fn(HistoricalTradeOptions) -> ExchangeFuture<'a, Vec<ClosedPnlEntry>> + 'a>;
pub struct ClosedPositionSource<'a> {
    pub fetch_closed_pnl: ClosedPnlFetcher<'a>,
    pub derive_closed_positions: Option<ClosedPnlFetcher<'a>>,
    pub closed_positions_share_ledger_source: Option<bool>,
}

pub async fn resolve_closed_positions(
    source: &ClosedPositionSource<'_>,
    options: HistoricalTradeOptions,
    venue_reported_unavailable: bool,
) -> Result<ClosedPositionsResult, ExchangeError> {
    if !venue_reported_unavailable {
        return Ok(ClosedPositionsResult {
            entries: (source.fetch_closed_pnl)(options).await?,
            provenance: DataProvenance::Venue,
            independently_verifiable: true,
            unavailable_reason: None,
        });
    }
    if let Some(derive) = &source.derive_closed_positions {
        let mut entries = derive(options).await?;
        for entry in &mut entries {
            entry.provenance = DataProvenance::Derived;
        }
        return Ok(ClosedPositionsResult {
            entries,
            provenance: DataProvenance::Derived,
            independently_verifiable: !source.closed_positions_share_ledger_source.unwrap_or(true),
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
}
