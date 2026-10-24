//! Funding has its own ledger. Inclusive time cursors preserve boundary rows.
use crate::history::local_read_policy;
use crate::wire::Envelope;
use crate::{Kucoin, Pending};
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Outcome, Query, TimerId, VenueError};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::HashMap;

const DAY: u64 = 86_400_000;
const WINDOW: u64 = 60 * DAY;
const PAGE_LIMIT: usize = 50;
const MAX_REQUESTS: u32 = 10_000;

#[derive(Default)]
pub struct Funding {
    waiting: HashMap<TimerId, Request>,
}
pub struct Request {
    owner: Box<CursorHistory<Income>>,
    symbol: String,
    api_symbol: String,
    from: u64,
    start: u64,
    end: u64,
    window_end: u64,
    limit: usize,
    explicit_range: bool,
    page: Vec<Income>,
    page_minimum: Option<u64>,
    page_count: usize,
    invalid: bool,
    retries: u8,
    requests: u32,
}
impl Request {
    fn wire_bounds(&self) -> (u64, u64) {
        if self.start == self.end {
            if self.start > 0 {
                (self.start - 1, self.end)
            } else {
                (0, 1)
            }
        } else {
            (self.start, self.end)
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: &'static str,
    income: f64,
    asset: String,
    timestamp: u64,
    tran_id: String,
    category: &'static str,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FundingRow<'a> {
    #[serde(borrow)]
    id: &'a RawValue,
    symbol: &'a str,
    #[serde(borrow)]
    time_point: &'a RawValue,
    #[serde(borrow)]
    funding: &'a RawValue,
    settle_currency: &'a str,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    has_more: bool,
}
fn millis(value: f64) -> Option<u64> {
    (value.is_finite() && (0.0..=9_007_199_254_740_991.0).contains(&value) && value.fract() == 0.0)
        .then_some(value as u64)
}
fn scalar(value: &RawValue) -> Result<&str, String> {
    let text = value.get();
    if text.starts_with('"') {
        serde_json::from_str(text).map_err(|_| "Invalid KuCoin funding scalar".into())
    } else {
        Ok(text)
    }
}
fn integer(value: &RawValue) -> Result<u64, String> {
    scalar(value)?
        .parse()
        .map_err(|_| "Invalid KuCoin funding integer".into())
}

impl Kucoin {
    pub(crate) fn funding_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let Query::Income {
            symbol,
            kind,
            from_ms,
            to_ms,
            limit,
        } = query
        else {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Unsupported KuCoin funding query")
                    .kind(ErrorKind::Unsupported)
                    .outcome(Outcome::NotSent),
            );
            return;
        };
        if kind.as_deref() != Some("FUNDING_FEE") {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Unsupported KuCoin funding query")
                    .kind(ErrorKind::Unsupported)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        if self.credentials.is_none() {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("KuCoin credentials required")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        let Some(symbol) = symbol.filter(|s| !s.is_empty()) else {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("KuCoin funding history requires a symbol")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        };
        let Some(market) = cx.store.market(&symbol) else {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new(format!("Unknown KuCoin market {symbol}"))
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        };
        let end = to_ms.unwrap_or_else(|| cx.now.floor());
        let start = from_ms.unwrap_or((end - DAY as f64).max(0.0));
        let (Some(from), Some(end)) = (millis(start), millis(end)) else {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Invalid KuCoin funding time range")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        };
        if from > end {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Invalid KuCoin funding time range")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        let limit = limit.unwrap_or(1000) as usize;
        if limit == 0 {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.done_context(&context, "[]".into());
            return;
        }
        self.funding_schedule(
            cx,
            Request {
                owner: Box::new(CursorHistory::new(
                    Completion::new(command),
                    CursorWalk::new(MAX_REQUESTS),
                    |_| "KuCoin funding exceeded page safety bound".into(),
                    |rows| serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string())),
                )),
                symbol,
                api_symbol: market.id.clone(),
                from,
                start: from.max(end.saturating_sub(WINDOW - 1)),
                end,
                window_end: end,
                limit,
                explicit_range: from_ms.is_some() && to_ms.is_some(),
                page: vec![],
                page_minimum: None,
                page_count: 0,
                invalid: false,
                retries: 0,
                requests: 0,
            },
            0.0,
        );
    }
    fn funding_schedule(&mut self, cx: &mut Cx<'_, Pending>, request: Request, delay: f64) {
        let at = self.history.next_at.max(cx.now + delay);
        self.history.next_at = at + 350.0;
        if at > cx.now {
            let timer = cx.set_timer(at - cx.now);
            self.funding.waiting.insert(timer, request);
        } else {
            self.funding_send(cx, request);
        }
    }
    fn funding_send(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        request.requests += 1;
        if request.requests > MAX_REQUESTS {
            request.owner.reject(
                cx,
                VenueError::new("KuCoin funding exceeded page safety bound"),
            );
            return;
        }
        request.page.clear();
        request.page_minimum = None;
        request.page_count = 0;
        request.invalid = false;
        let (start, end) = request.wire_bounds();
        let path = format!(
            "/api/v1/funding-history?symbol={}&startAt={start}&endAt={end}&reverse=true&forward=false&maxCount={PAGE_LIMIT}",
            request.api_symbol
        );
        self.signed_streamed(
            cx,
            Method::Get,
            &path,
            None,
            ArrayAt::Path(&["data", "dataList"]),
            Pending::Funding(request),
        );
    }
    pub(crate) fn funding_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        request.page_count += 1;
        if request.page_count > PAGE_LIMIT {
            request.invalid = true;
            return;
        }
        let result = (|| -> Result<(), String> {
            let row: FundingRow =
                serde_json::from_str(item).map_err(|_| "Invalid KuCoin funding row")?;
            let id = integer(row.id)?;
            let time = integer(row.time_point)?;
            let income = scalar(row.funding)?
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or("Invalid KuCoin funding amount")?;
            let (start, end) = request.wire_bounds();
            if id == 0
                || row.symbol != request.api_symbol
                || row.settle_currency.is_empty()
                || time < start
                || time > end
                || time > 9_007_199_254_740_991
            {
                return Err("Invalid KuCoin funding row scope".into());
            }
            request.page_minimum = Some(request.page_minimum.map_or(time, |old| old.min(time)));
            if time >= request.start && time <= request.end {
                request.page.push(Income {
                    symbol: request.symbol.clone(),
                    income_type: "FUNDING_FEE",
                    income,
                    asset: row.settle_currency.into(),
                    timestamp: time,
                    tran_id: id.to_string(),
                    category: "funding",
                });
            }
            Ok(())
        })();
        request.invalid |= result.is_err();
    }
    pub(crate) fn funding_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
    ) {
        error.outcome = None;
        request.owner.reject(cx, error);
    }
    pub(crate) fn funding_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        status: u16,
        body: &str,
    ) {
        let envelope = serde_json::from_str::<Envelope>(body);
        let rejected = envelope.as_ref().is_ok_and(|e| e.code == "400005");
        let valid = (200..300).contains(&status)
            && !request.invalid
            && envelope
                .as_ref()
                .is_ok_and(|e| e.code == "200000" && e.data.is_some());
        if !valid {
            if (status == 0 || status == 429 || status >= 500 || rejected)
                && request.retries < if rejected { 1 } else { 3 }
            {
                request.retries += 1;
                let delay = 1000.0 * request.retries as f64;
                self.funding_schedule(cx, request, delay);
            } else {
                let error = envelope.map_or_else(
                    |_| "Malformed KuCoin funding response".into(),
                    |e| format!("KuCoin funding {}: {}", e.code, e.msg),
                );
                self.funding_finish(cx, request, Err(VenueError::new(error)));
            }
            return;
        }
        let envelope = envelope.unwrap();
        let page = match serde_json::from_str::<Page>(envelope.data.unwrap().get()) {
            Ok(page) => page,
            Err(_) => {
                self.funding_finish(
                    cx,
                    request,
                    Err(VenueError::new("Malformed KuCoin funding page")),
                );
                return;
            }
        };
        // Never subtract one from an incomplete timestamp boundary: it may hide
        // additional settlements at exactly the same millisecond.
        if page.has_more && request.page_minimum.is_none_or(|time| time >= request.end) {
            request.owner.reject(
                cx,
                VenueError::new(
                    "KuCoin funding cursor cannot prove complete same-timestamp coverage",
                ),
            );
            return;
        }
        let mut fresh = 0;
        for row in request.page.drain(..) {
            if request.owner.fresh(row.tran_id.clone()) {
                request.owner.stage(row);
                fresh += 1;
            }
        }
        let capped = !request.explicit_range
            && request.owner.accepted_len() + request.owner.staged_len() >= request.limit;
        let next_bounds = if !capped
            && page.has_more
            && request
                .page_minimum
                .is_some_and(|time| time >= request.start)
        {
            request.end = request.page_minimum.unwrap();
            Some((request.start, request.end))
        } else if !capped && request.start > request.from {
            request.window_end = request.start - 1;
            request.end = request.window_end;
            request.start = request.from.max(request.end.saturating_sub(WINDOW - 1));
            Some((request.start, request.end))
        } else {
            None
        };
        let next = next_bounds.map(|(start, end)| format!("{start}:{end}"));
        match request.owner.advance(CursorPage {
            next,
            rows: request.page_count,
            fresh,
            want_more: true,
        }) {
            CursorStep::More(_) => {
                request.retries = 0;
                self.funding_schedule(cx, request, 0.0);
            }
            CursorStep::Done => self.funding_finish(cx, request, Ok(())),
            CursorStep::Stalled(_) => request.owner.reject(
                cx,
                VenueError::new("KuCoin funding exceeded page safety bound"),
            ),
        }
    }
    fn funding_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        result: Result<(), VenueError>,
    ) {
        let limit = request.limit;
        let uncapped = request.explicit_range;
        request.owner.finish_with(cx, result, |mut rows| {
            rows.sort_by(|a, b| {
                b.timestamp
                    .cmp(&a.timestamp)
                    .then_with(|| a.tran_id.cmp(&b.tran_id))
            });
            if !uncapped {
                rows.truncate(limit);
            }
            serde_json::to_string(&rows).map_err(|e| VenueError::new(e.to_string()))
        });
    }
    pub(crate) fn funding_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(request) = self.funding.waiting.remove(&timer) {
            self.funding_send(cx, request);
            true
        } else {
            false
        }
    }
}
