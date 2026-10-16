//! History read orchestration; authenticated page fetching is a separate row.
use safe_cex_types::{IncomeHistoryOptions, IncomeRecord};
use std::future::Future;

// ts: ParadexExchange.fetchIncomeHistoryRange
pub async fn fetch_income_history_range<E, F>(
    start_time: f64,
    end_time: f64,
    income_type: Option<&str>,
    fetch_income_history: impl FnOnce(IncomeHistoryOptions) -> Result<F, E>,
) -> Result<Vec<IncomeRecord>, E>
where
    F: Future<Output = Result<Vec<IncomeRecord>, E>>,
{
    let records = fetch_income_history(IncomeHistoryOptions {
        start_time: Some(start_time),
        end_time: Some(end_time),
        income_type: income_type.map(str::to_owned),
        symbol: None,
        limit: None,
    })?
    .await?;
    // Source truthiness: missing and empty string both return the complete list.
    if let Some(kind) = income_type.filter(|kind| !kind.is_empty()) {
        Ok(records
            .into_iter()
            .filter(|record| record.income_type == kind)
            .collect())
    } else {
        Ok(records)
    }
}
