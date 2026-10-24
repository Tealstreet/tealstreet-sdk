//! Venue-reported closed positions: raw `/api/v5/account/positions-history`
//! rows, which the host maps with the TS mappers. `begin`/`end` are ignored by
//! this endpoint; `before`/`after` are close-time bounds, paged by oldest `uTime`.
use crate::{Okx, Pending, sign};
use safe_cex_engine::credential_selection::SlotPolicy;
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::request_context::{CommandEntryError, Operation, Owner};
use safe_cex_engine::selected_read::{
    CommandReadError, LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot,
    SelectedRead,
};
use safe_cex_engine::url::encode;
use safe_cex_engine::{CommandId, Cx, Method, TimerId, VenueError, num};
use serde::Deserialize;

const PAGE: usize = 100;
const MAX_PAGES: u32 = 100;
const PAGE_DELAY_MS: f64 = 100.0;

#[derive(Default)]
pub struct ClosedState {
    delayed: Vec<(TimerId, Request)>,
}

pub struct Request {
    root: ReadRoot,
    history: ReadCursorHistory<String>,
    leg: Option<ReadLeg>,
    instrument: Option<String>,
    before: Option<f64>,
    count: usize,
    oldest: Option<i64>,
}

impl Request {
    pub(crate) fn valid_delivery(&self, cx: &Cx<'_, Pending>) -> bool {
        self.leg.as_ref().is_some_and(|leg| {
            self.root.validate_delivery(cx, leg).is_ok()
                && self.history.validate_delivery(cx, leg.context()).is_ok()
        })
    }
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
        if self.credentials.is_none() {
            match cx.admit_caller_read_root(None, None) {
                Ok(root) => {
                    let _ = cx.done_context(&root, "[]".into());
                }
                Err(CommandEntryError::Rejected(_)) => {}
                Err(CommandEntryError::Revoked) => {
                    let _ = cx.reject_expired_command(Operation::Read);
                }
            }
            return;
        }
        let slots = self.read_slots(SlotPolicy {
            secondary_requires_primary: true,
        });
        let selected = SelectedRead::select(&slots, ReadRole::Read { scope: None });
        let root = match SelectedRead::command_root(
            cx,
            selected,
            LocalRefusal {
                diagnostic: None,
                error: VenueError::new("OKX closed positions credential selection failed"),
            },
        ) {
            Ok(root) => root,
            Err(CommandReadError::Entry(CommandEntryError::Revoked)) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
            Err(_) => return,
        };
        let completion = match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => completion,
            Err(reason) => {
                let _ = cx.fail_context(root.context(), reason.read_venue_error());
                return;
            }
        };
        let instrument = match symbol {
            Some(symbol) => match cx.store.market(&symbol) {
                Some(market) => Some(market.id.clone()),
                None => {
                    let _ = completion.finish(cx, Ok("[]".into()));
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
                root,
                history: ReadCursorHistory::new(completion, walk, |reason| match reason {
                    Stall::PageBudget => format!("OKX closed positions stopped at the {MAX_PAGES}-page cap; older positions were not fetched"),
                    _ => "OKX closed positions stopped at a repeated cursor; older positions were not fetched".into(),
                }, |rows| Ok(format!("[{}]", rows.join(",")))),
                leg: None,
                instrument,
                before: from_ms,
                count: 0,
                oldest: None,
            },
        );
    }

    fn request_closed_page(&self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        let leg = match request.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            Err(reason) => {
                let _ = request.history.reject(cx, reason.read_venue_error());
                return;
            }
        };
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
        let url = format!("{}{path}", self.rest());
        let testnet = self.options.testnet;
        if let Err((Pending::Closed(request), reason)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body: None,
                stream: Some(ArrayAt::EnvelopeField("data")),
            },
            |identity| Box::new(sign::Http::new(Some(identity.clone()), path).demo(testnet)),
            |leg| {
                request.leg = Some(leg);
                Pending::Closed(Box::new(request))
            },
        ) {
            let _ = request.history.reject(cx, reason.read_venue_error());
        }
    }

    pub(crate) fn closed_item(cx: &Cx<'_, Pending>, request: &mut Request, item: &str) {
        let Some(leg) = &request.leg else { return };
        if request.root.validate_delivery(cx, leg).is_err() {
            return;
        }
        let Ok(mut page) = request.history.admit(cx, leg.context()) else {
            return;
        };
        request.count += 1;
        let time = serde_json::from_str::<CloseTime<'_>>(item)
            .ok()
            .and_then(|row| row.u_time?.parse::<i64>().ok())
            .filter(|time| *time > 0);
        if let Some(time) = time {
            request.oldest = Some(request.oldest.map_or(time, |oldest| oldest.min(time)));
        }
        page.stage(item.to_owned());
    }

    pub(crate) fn closed_response(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        let next = request.oldest.map(|oldest| (oldest - 1).to_string());
        let Some(leg) = &request.leg else { return };
        let step = {
            let Ok(mut page) = request.history.admit(cx, leg.context()) else {
                return;
            };
            page.advance(CursorPage {
                next,
                rows: request.count,
                fresh: request.count,
                want_more: request.count >= PAGE,
            })
        };
        if let Some(leg) = request.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
        match step {
            CursorStep::Done | CursorStep::Stalled(_) => return Self::finish_closed(cx, request),
            CursorStep::More(_) => {}
        }
        let timer = cx.set_timer(PAGE_DELAY_MS);
        self.closed.delayed.push((timer, request));
    }

    pub(crate) fn closed_sign_failed(
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: safe_cex_engine::VenueError,
    ) {
        if let Some(leg) = request.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
        let _ = request.history.finish(cx, Err(error));
    }

    pub(crate) fn closed_failed(cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        Self::closed_sign_failed(cx, request, format!("OKX closed positions: {error}").into());
    }

    pub(crate) fn closed_http_failed(
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
    ) {
        error.message = format!("OKX closed positions: {}", error.message);
        Self::closed_sign_failed(cx, request, error);
    }

    fn finish_closed(cx: &mut Cx<'_, Pending>, request: Request) {
        let _ = request.history.finish(cx, Ok(()));
    }

    pub(crate) fn retire_closed(&mut self, owner: Owner) {
        self.closed
            .delayed
            .retain(|(_, request)| request.root.context().owner() != owner);
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
