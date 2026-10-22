//! Venue-reported closed positions: raw `/api/v1/account/positions-history`
//! rows, which the host maps with the TS mappers. Rows dedupe on `historyId`,
//! not `positionId`, and page by the smallest `historyId` seen.
use super::*;
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::url::encode;
use serde::Deserialize;
use std::borrow::Cow;

const PAGE: usize = 100;
const MAX_PAGES: u32 = 100;

pub struct Request {
    history: CursorHistory<String>,
    instrument: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    count: usize,
    added: usize,
    oldest: Option<String>,
    failed: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Id<'a> {
    Text(#[serde(borrow)] Cow<'a, str>),
    Number(u64),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity<'a> {
    #[serde(borrow, default)]
    history_id: Option<Id<'a>>,
}

/// TS `String(row.historyId)`: strings and integers both name a row.
fn history_id(item: &str) -> Option<String> {
    match serde_json::from_str::<Identity<'_>>(item)
        .ok()?
        .history_id?
    {
        Id::Text(id) => Some(id.into_owned()),
        Id::Number(id) => Some(id.to_string()),
    }
}

/// TS `oldestCursorId`: numeric ids compared by length, then lexically.
fn older(id: &str, than: &str) -> bool {
    (id.len(), id) < (than.len(), than)
}

impl Blofin {
    pub(crate) fn start_closed_positions(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    ) {
        let completion = Completion::new(command);
        if !self.signed() {
            completion.finish(cx, Ok("[]".into()));
            return;
        }
        let instrument = match symbol {
            Some(symbol) => match cx.store.market(&symbol) {
                Some(market) => Some(market.id.clone()),
                None => {
                    completion.finish(cx, Ok("[]".into()));
                    return;
                }
            },
            None => None,
        };
        self.request_closed_page(
            cx,
            Request {
                history: CursorHistory::new(
                    completion,
                    CursorWalk::new(MAX_PAGES)
                        .end_on_no_new_rows()
                        .end_on_unchanged_cursor(),
                    |reason| match reason {
                        Stall::PageBudget => format!(
                            "BloFin closed positions stopped at the {MAX_PAGES}-page cap; older positions were not fetched"
                        ),
                        _ => "BloFin closed positions stopped at a repeated cursor; older positions were not fetched".into(),
                    },
                    |rows| Ok(format!("[{}]", rows.join(","))),
                ),
                instrument,
                from: from_ms,
                to: to_ms,
                count: 0,
                added: 0,
                oldest: None,
                failed: false,
            },
        );
    }

    fn request_closed_page(&self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        request.count = 0;
        request.added = 0;
        request.oldest = None;
        let mut params = Vec::new();
        if let Some(id) = &request.instrument {
            params.push(format!("instId={}", encode(id)));
        }
        if let Some(from) = request.from {
            params.push(format!("begin={}", num::format(from)));
        }
        if let Some(to) = request.to {
            params.push(format!("end={}", num::format(to)));
        }
        params.push(format!("limit={PAGE}"));
        if let Some(after) = request.history.cursor() {
            params.push(format!("after={}", encode(after)));
        }
        let path = format!("/api/v1/account/positions-history?{}", params.join("&"));
        match self.headers(
            |bytes| cx.fill_signing_entropy(bytes),
            &path,
            Method::Get,
            cx.now,
            "",
        ) {
            Ok(headers) => {
                cx.request_streamed(
                    Method::Get,
                    format!("{}{path}", self.host()),
                    headers,
                    ArrayAt::EnvelopeField("data"),
                    Pending::Closed(request),
                );
            }
            Err(error) => request.history.finish(cx, Err(error.into())),
        }
    }

    pub(crate) fn closed_item(request: &mut Request, item: &str) {
        if serde_json::from_str::<&serde_json::value::RawValue>(item).is_err() {
            request.failed = true;
            return;
        }
        request.count += 1;
        let id = history_id(item);
        let cursor = id.as_deref().map(str::trim).filter(|id| {
            !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_digit())
                && request
                    .oldest
                    .as_deref()
                    .is_none_or(|oldest| older(id, oldest))
        });
        if let Some(cursor) = cursor {
            request.oldest = Some(cursor.to_owned());
        }
        // TS keys the dedupe on the untrimmed `String(historyId)`.
        if let Some(id) = id.filter(|id| !id.is_empty())
            && !request.history.fresh(id)
        {
            return;
        }
        request.added += 1;
        request.history.stage(item.to_owned());
    }

    pub(crate) fn closed_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if let Err(error) = check(&response) {
            return Self::closed_failed(cx, request, &error);
        }
        if request.failed {
            return Self::closed_failed(cx, request, "malformed positions-history row");
        }
        match request.history.advance(CursorPage {
            next: request.oldest.take(),
            rows: request.count,
            fresh: request.added,
            want_more: request.count >= PAGE,
        }) {
            CursorStep::Done | CursorStep::Stalled(_) => return Self::finish_closed(cx, request),
            CursorStep::More(_) => {}
        }
        self.request_closed_page(cx, request);
    }

    pub(crate) fn closed_failed(cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        request
            .history
            .finish(cx, Err(format!("BloFin closed positions: {error}").into()));
    }

    fn finish_closed(cx: &mut Cx<'_, Pending>, request: Request) {
        request.history.finish(cx, Ok(()));
    }
}
