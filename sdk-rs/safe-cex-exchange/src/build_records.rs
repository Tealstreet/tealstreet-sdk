//! Read fills after the straddle lookback, fold per symbol, and project the requested window.
use crate::{
    fold_position_adapter::{FoldPositionAdapterDeps, map_folded_interval},
    position_intervals::{
        FundingRow, PositionInterval, PositionIntervalOptions, attribute_funding,
        reconstruct_position_intervals,
    },
    runtime::{ExchangeError, ExchangeFuture},
};
use indexmap::{IndexMap, IndexSet};
use safe_cex_types::{Execution, ExecutionsOptions, IncomeCategory, IncomeRecord, PositionRecord};
const STRADDLE_LOOKBACK_MS: f64 = 14. * 24. * 60. * 60. * 1000.;
pub struct BuildRecordsOptions<'a> {
    pub account: &'a str,
    pub from: f64,
    pub to: f64,
    pub symbol_scoped: bool,
    pub resolve_settle_currency: &'a mut dyn FnMut(&str) -> Result<String, ExchangeError>,
}
#[derive(Debug)]
pub struct BuildRecordsResult {
    pub records: Vec<PositionRecord>,
    pub total_fills: usize,
    pub intervals: Vec<PositionInterval>,
}
/// Native callable projection of the source's structural FoldableExchange parameter.
pub struct FoldableExchange<'a> {
    pub fetch_historical_fills:
        &'a mut dyn FnMut(ExecutionsOptions) -> ExchangeFuture<'a, Vec<Execution>>,
    pub fetch_income_history_range:
        Option<&'a mut dyn FnMut(f64, f64) -> ExchangeFuture<'a, Vec<IncomeRecord>>>,
}
/// A wrapped income failure retains its complete native cause; other failures propagate.
#[derive(Debug)]
pub struct BuildRecordsError {
    pub error: ExchangeError,
    pub cause: Option<Box<ExchangeError>>,
}
fn direct(error: ExchangeError) -> BuildRecordsError {
    BuildRecordsError { error, cause: None }
}
pub async fn build_folded_position_records(
    exchange: &mut FoldableExchange<'_>,
    options: &mut BuildRecordsOptions<'_>,
) -> Result<BuildRecordsResult, BuildRecordsError> {
    let lookback_from = options.from - STRADDLE_LOOKBACK_MS;
    let mut funding_rows = Vec::new();
    let mut symbols = IndexSet::<String>::new();
    let mut income_failure = None;
    if let Some(read) = exchange.fetch_income_history_range.as_mut() {
        match read(lookback_from, options.to).await {
            Ok(items) => {
                for item in items {
                    if !item.symbol.is_empty() {
                        symbols.insert(item.symbol.clone());
                    }
                    if item.category == Some(IncomeCategory::Funding) {
                        funding_rows.push(FundingRow {
                            symbol: item.symbol,
                            timestamp: item.timestamp,
                            income: item.income,
                        });
                    }
                }
            }
            Err(error) => income_failure = Some(error),
        }
    }
    if options.symbol_scoped
        && let Some(cause) = income_failure
    {
        let mut error = ExchangeError::new(
            "journal fold: income read failed on a symbol-scoped venue, so the symbol list that drives the fills fetch is unknown — continuing would report an empty account",
        );
        error.name = cause.name.clone();
        return Err(BuildRecordsError {
            error,
            cause: Some(Box::new(cause)),
        });
    }
    let mut intervals = Vec::new();
    let mut total_fills = 0;
    if options.symbol_scoped {
        for symbol in symbols {
            let fills = (exchange.fetch_historical_fills)(ExecutionsOptions {
                symbol: Some(symbol),
                order_id: None,
                from: Some(lookback_from),
                to: Some(options.to),
                limit: Some(1000.),
            })
            .await
            .map_err(direct)?;
            total_fills += fills.len();
            intervals.extend(
                reconstruct_position_intervals(&fills, &mut PositionIntervalOptions::default())
                    .map_err(|e| direct(ExchangeError::new(e)))?,
            );
        }
    } else {
        let fills = (exchange.fetch_historical_fills)(ExecutionsOptions {
            symbol: None,
            order_id: None,
            from: Some(lookback_from),
            to: Some(options.to),
            limit: Some(1000.),
        })
        .await
        .map_err(direct)?;
        total_fills = fills.len();
        let mut by_symbol = IndexMap::<String, Vec<Execution>>::new();
        for fill in fills {
            by_symbol.entry(fill.symbol.clone()).or_default().push(fill);
        }
        for list in by_symbol.values() {
            intervals.extend(
                reconstruct_position_intervals(list, &mut PositionIntervalOptions::default())
                    .map_err(|e| direct(ExchangeError::new(e)))?,
            );
        }
    }
    let mut records = Vec::new();
    for interval in &intervals {
        if interval.closed_at.is_none_or(|at| at < options.from) {
            continue;
        }
        let mut resolve_funding = |i: &PositionInterval| attribute_funding(i, &funding_rows, false);
        let mut deps = FoldPositionAdapterDeps {
            account: options.account,
            resolve_settle_currency: options.resolve_settle_currency,
            resolve_funding: if funding_rows.is_empty() {
                None
            } else {
                Some(&mut resolve_funding)
            },
        };
        if let Some(record) = map_folded_interval(interval, &mut deps).map_err(direct)? {
            records.push(record);
        }
    }
    Ok(BuildRecordsResult {
        records,
        total_fills,
        intervals,
    })
}
