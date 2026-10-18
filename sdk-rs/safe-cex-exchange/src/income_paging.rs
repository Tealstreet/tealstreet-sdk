//! Shared span-based income paging with runtime-owned continuation.
use crate::{
    eager::start_eager,
    runtime::{ExchangeFuture, ExchangeRuntime},
};
use indexmap::IndexMap;
use safe_cex_num::js_number_string;
use safe_cex_types::{IncomeCategory, IncomeRecord};
use std::rc::Rc;

#[derive(Debug, Clone, Copy)]
pub struct IncomePageParams {
    pub start_time: f64,
    pub end_time: f64,
    pub limit: f64,
}

pub type IncomePageFetcher<'a> =
    dyn FnMut(IncomePageParams) -> ExchangeFuture<'a, Vec<IncomeRecord>> + 'a;

#[derive(Debug, Clone, Copy)]
pub struct IncomeRangeOptions {
    pub start_time: f64,
    pub end_time: f64,
    pub chunk_ms: f64,
    pub page_limit: f64,
    pub chunk_delay_ms: Option<f64>,
    pub max_pages_per_chunk: Option<f64>,
}

#[derive(Debug)]
pub struct IncomeRangeResult {
    pub records: Vec<IncomeRecord>,
    pub truncated: bool,
    pub exhausted_chunks: f64,
    pub requests: f64,
}

pub fn identify(record: &IncomeRecord) -> String {
    let income = js_number_string(record.income);
    if record.tran_id.is_empty() {
        return format!(
            "{}|{}|{}|{income}",
            record.symbol,
            record.income_type,
            js_number_string(record.timestamp)
        );
    }
    let category = match record.category.as_ref() {
        None => "",
        Some(IncomeCategory::RealizedPnl) => "realized_pnl",
        Some(IncomeCategory::Funding) => "funding",
        Some(IncomeCategory::Commission) => "commission",
        Some(IncomeCategory::Rebate) => "rebate",
        Some(IncomeCategory::Transfer) => "transfer",
        Some(IncomeCategory::Liquidation) => "liquidation",
        Some(IncomeCategory::Other) => "other",
    };
    format!(
        "{}|{category}|{}|{income}",
        record.tran_id, record.income_type
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum PageDirection {
    Ascending,
    Descending,
    Unknown,
}

pub fn page_direction(page: &[IncomeRecord], range_start: f64, range_end: f64) -> PageDirection {
    let mut times = page
        .iter()
        .map(|record| record.timestamp)
        .filter(|time| time.is_finite() && *time >= range_start && *time <= range_end);
    let Some(mut previous) = times.next() else {
        return PageDirection::Unknown;
    };
    let (mut ascending, mut descending) = (true, true);
    for time in times {
        if time < previous {
            ascending = false;
        }
        if time > previous {
            descending = false;
        }
        previous = time;
    }
    if ascending == descending {
        PageDirection::Unknown
    } else if ascending {
        PageDirection::Ascending
    } else {
        PageDirection::Descending
    }
}

pub fn paginate_income_range(
    runtime: Rc<dyn ExchangeRuntime>,
    mut fetch_page: Box<IncomePageFetcher<'static>>,
    opts: IncomeRangeOptions,
    mut delay: Box<dyn FnMut(f64) -> ExchangeFuture<'static, ()>>,
) -> ExchangeFuture<'static, IncomeRangeResult> {
    let body_runtime = runtime.clone();
    start_eager(&runtime, async move {
        let IncomeRangeOptions {
            start_time,
            end_time,
            chunk_ms,
            page_limit,
            ..
        } = opts;
        let max_pages = opts.max_pages_per_chunk.unwrap_or(50.0);
        let mut by_id = IndexMap::new();
        let mut result = IncomeRangeResult {
            records: vec![],
            truncated: false,
            exhausted_chunks: 0.0,
            requests: 0.0,
        };
        if !(end_time > start_time && chunk_ms > 0.0 && page_limit > 0.0) {
            return Ok(result);
        }
        let mut first_chunk = true;
        let mut chunk_start = start_time;
        while chunk_start < end_time {
            let chunk_end = safe_cex_num::math_min(chunk_start + chunk_ms, end_time);
            if !first_chunk
                && let Some(ms) = opts.chunk_delay_ms.filter(|ms| *ms != 0.0 && !ms.is_nan())
            {
                start_eager(&body_runtime, delay(ms)).await?;
            }
            first_chunk = false;
            let mut pending = vec![(chunk_start, chunk_end)];
            let mut pages = 0.0;
            let mut incomplete = false;
            while !pending.is_empty() {
                if pages >= max_pages {
                    incomplete = true;
                    break;
                }
                pages += 1.0;
                let Some((from, to)) = pending.pop() else {
                    break;
                };
                if from > to {
                    continue;
                }
                result.requests += 1.0;
                // Source catch abandons a failed branch and reports incomplete coverage.
                let request = fetch_page(IncomePageParams {
                    start_time: from,
                    end_time: to,
                    limit: page_limit,
                });
                let page = match start_eager(&body_runtime, request).await {
                    Ok(page) => page,
                    Err(_) => {
                        incomplete = true;
                        continue;
                    }
                };
                if page.is_empty() {
                    continue;
                }
                let direction = page_direction(&page, start_time, end_time);
                let page_length = page.len() as f64;
                let (mut oldest, mut newest) = (f64::INFINITY, f64::NEG_INFINITY);
                let (mut inside_window, mut fresh) = (false, false);
                for record in page {
                    let timestamp = record.timestamp;
                    if !timestamp.is_finite() || timestamp < start_time || timestamp > end_time {
                        continue;
                    }
                    if timestamp >= from && timestamp <= to {
                        inside_window = true;
                    }
                    if timestamp < oldest {
                        oldest = timestamp;
                    }
                    if timestamp > newest {
                        newest = timestamp;
                    }
                    if let indexmap::map::Entry::Vacant(entry) = by_id.entry(identify(&record)) {
                        entry.insert(record);
                        fresh = true;
                    }
                }
                if !oldest.is_finite() || !inside_window {
                    incomplete = true;
                    continue;
                }
                if !fresh {
                    if page_length < page_limit {
                        continue;
                    }
                    incomplete = true;
                    if direction != PageDirection::Ascending && oldest - 1.0 >= from {
                        pending.push((from, oldest - 1.0));
                    }
                    if direction != PageDirection::Descending && newest + 1.0 <= to {
                        pending.push((newest + 1.0, to));
                    }
                    continue;
                }
                if direction != PageDirection::Ascending && oldest >= from {
                    pending.push((from, oldest));
                }
                if direction != PageDirection::Descending && newest <= to {
                    pending.push((newest, to));
                }
            }
            if incomplete {
                result.truncated = true;
                result.exhausted_chunks += 1.0;
            }
            chunk_start += chunk_ms;
        }
        result.records = by_id.into_values().collect();
        result.records.sort_by(|a, b| {
            b.timestamp
                .partial_cmp(&a.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(result)
    })
}
