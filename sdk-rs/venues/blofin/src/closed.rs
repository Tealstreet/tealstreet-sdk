//! Venue-reported closed positions: raw `/api/v1/account/positions-history`
//! rows, which the host maps with the TS mappers. Rows dedupe on `historyId`,
//! not `positionId`, and page by the smallest `historyId` seen.
use super::*;
use safe_cex_engine::CommandWait;
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::request_context::{CommandEntryError, Error as RequestError, Operation};
use safe_cex_engine::selected_read::{
    CommandReadError, LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRoot, SelectedRead,
};
use safe_cex_engine::url::encode;
use serde::Deserialize;
use std::borrow::Cow;

const PAGE: usize = 100;
const MAX_PAGES: u32 = 100;

pub struct Request {
    root: ReadRoot,
    leg: Option<ReadLeg>,
    history: ReadCursorHistory<String>,
    instrument: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    count: usize,
    added: usize,
    oldest: Option<String>,
    failed: bool,
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
        if !self.signed() {
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
        // The caller's wait bounds the whole walk: an expired or unstamped
        // receipt is the shared typed refusal, never a defaulted root.
        match cx.command_wait() {
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
            None => {
                let _ = cx.reject_unstamped_command(Operation::Read);
                return;
            }
            Some(CommandWait::Remaining(_)) => {}
        }
        let (selected, message) = self.read_selection();
        let root = match SelectedRead::command_root(
            cx,
            selected,
            LocalRefusal {
                diagnostic: Some(message.into()),
                error: message.into(),
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
        self.request_closed_page(
            cx,
            Request {
                root,
                leg: None,
                history: ReadCursorHistory::new(
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
        let leg = match request.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            // An expired or retired root already has the engine's terminal.
            Err(RequestError::Revoked) => return,
            // Strict: a refused page is never a recoverable partial (shared owner contract).
            Err(reason) => {
                let _ = request.history.reject(cx, reason.read_venue_error());
                return;
            }
        };
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
        let url = format!("{}{path}", self.host());
        let testnet = self.options.testnet;
        if let Err((Pending::Closed(request), reason)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body: None,
                stream: Some(ArrayAt::EnvelopeField("data")),
            },
            |identity| {
                Box::new(crate::dispatch_signing::Http::new(
                    identity.clone(),
                    testnet,
                    Method::Get,
                    path,
                    None,
                ))
            },
            |leg| {
                request.leg = Some(leg);
                Pending::Closed(request)
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
            && !page.fresh(id)
        {
            return;
        }
        request.added += 1;
        page.stage(item.to_owned());
    }

    pub(crate) fn closed_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !request.valid_delivery(cx) {
            return;
        }
        if let Err(error) = check(&response) {
            return Self::closed_failed(cx, request, VenueError::new(error));
        }
        if request.failed {
            return Self::closed_failed(
                cx,
                request,
                VenueError::new("malformed positions-history row"),
            );
        }
        let step = {
            let Some(leg) = &request.leg else { return };
            let Ok(mut page) = request.history.admit(cx, leg.context()) else {
                return;
            };
            page.advance(CursorPage {
                next: request.oldest.take(),
                rows: request.count,
                fresh: request.added,
                want_more: request.count >= PAGE,
            })
        };
        if let Some(leg) = request.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
        match step {
            CursorStep::Done | CursorStep::Stalled(_) => {
                let _ = request.history.finish(cx, Ok(()));
            }
            CursorStep::More(_) => self.request_closed_page(cx, request),
        }
    }

    /// A failed page keeps accepted rows with one INCOMPLETE (shared owner contract);
    /// the native text and any typed kind (Network, Timeout) are kept.
    pub(crate) fn closed_failed(
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        mut error: VenueError,
    ) {
        if let Some(leg) = request.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
        error.message = format!("BloFin closed positions: {}", error.message);
        let _ = request.history.finish(cx, Err(error));
    }
}
