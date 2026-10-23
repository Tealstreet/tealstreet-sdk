//! Venue-reported closed positions: raw `/api/v5/account/positions-history`
//! rows, which the host maps with the TS mappers. `begin`/`end` are ignored by
//! this endpoint; `before`/`after` are close-time bounds, paged by oldest `uTime`.
use crate::{Okx, Pending};
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::url::encode;
use safe_cex_engine::{CommandId, Cx, TimerId, num};
use serde::Deserialize;

const PAGE: usize = 100;
const MAX_PAGES: u32 = 100;
const PAGE_DELAY_MS: f64 = 100.0;

#[derive(Default)]
pub struct ClosedState {
    delayed: Vec<(TimerId, Request)>,
}

pub struct Request {
    history: CursorHistory<String>,
    instrument: Option<String>,
    before: Option<f64>,
    count: usize,
    oldest: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloseTime<'a> {
    #[serde(borrow, default)]
    u_time: Option<&'a str>,
}

impl Okx {
    pub(crate) fn start_closed_positions(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    ) {
        let completion = Completion::new(command);
        if self.credentials.is_none() {
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
        let walk = match to_ms {
            Some(to) => CursorWalk::starting_at(MAX_PAGES, num::format(to)),
            None => CursorWalk::new(MAX_PAGES),
        }
        .end_on_unchanged_cursor();
        self.request_closed_page(
            cx,
            Request {
                history: CursorHistory::new(completion, walk, |reason| match reason {
                    Stall::PageBudget => format!("OKX closed positions stopped at the {MAX_PAGES}-page cap; older positions were not fetched"),
                    _ => "OKX closed positions stopped at a repeated cursor; older positions were not fetched".into(),
                }, |rows| Ok(format!("[{}]", rows.join(",")))),
                instrument,
                before: from_ms,
                count: 0,
                oldest: None,
            },
        );
    }

    fn request_closed_page(&self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        request.count = 0;
        request.oldest = None;
        let mut path = String::from("/api/v5/account/positions-history?instType=SWAP");
        if let Some(id) = &request.instrument {
            path.push_str(&format!("&instId={}", encode(id)));
        }
        if let Some(before) = request.before {
            path.push_str(&format!("&before={}", num::format(before)));
        }
        if let Some(after) = request.history.cursor() {
            path.push_str(&format!("&after={after}"));
        }
        path.push_str(&format!("&limit={PAGE}"));
        self.list(cx, &path, true, Pending::Closed(request));
    }

    pub(crate) fn closed_item(request: &mut Request, item: &str) {
        request.count += 1;
        let time = serde_json::from_str::<CloseTime<'_>>(item)
            .ok()
            .and_then(|row| row.u_time?.parse::<i64>().ok())
            .filter(|time| *time > 0);
        if let Some(time) = time {
            request.oldest = Some(request.oldest.map_or(time, |oldest| oldest.min(time)));
        }
        request.history.stage(item.to_owned());
    }

    pub(crate) fn closed_response(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        let next = request.oldest.map(|oldest| (oldest - 1).to_string());
        match request.history.advance(CursorPage {
            next,
            rows: request.count,
            fresh: request.count,
            want_more: request.count >= PAGE,
        }) {
            CursorStep::Done | CursorStep::Stalled(_) => return Self::finish_closed(cx, request),
            CursorStep::More(_) => {}
        }
        let timer = cx.set_timer(PAGE_DELAY_MS);
        self.closed.delayed.push((timer, request));
    }

    pub(crate) fn closed_sign_failed(
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: safe_cex_engine::VenueError,
    ) {
        request.history.finish(cx, Err(error));
    }

    pub(crate) fn closed_failed(cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        request
            .history
            .finish(cx, Err(format!("OKX closed positions: {error}").into()));
    }

    fn finish_closed(cx: &mut Cx<'_, Pending>, request: Request) {
        request.history.finish(cx, Ok(()));
    }

    pub(crate) fn closed_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let Some(index) = self.closed.delayed.iter().position(|(id, _)| *id == timer) else {
            return false;
        };
        let (_, request) = self.closed.delayed.swap_remove(index);
        self.request_closed_page(cx, request);
        true
    }
}
