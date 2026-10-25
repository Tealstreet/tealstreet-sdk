//! Venue-reported closed positions: raw `/v5/position/closed-pnl` rows, which
//! the host maps with the TS mappers (`mapBybitClosedPnl`, the journal adapter).
use crate::history::windows;
use crate::sign::Query;
use crate::{BybitLinear, Category, Pending, Rest, Then};
use safe_cex_engine::history::{CursorHistory, ReadCompletion, ReadCursorHistory};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{CommandId, Cx, Response, VenueError};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::borrow::Cow;
use std::collections::VecDeque;

const PAGE_LIMIT: f64 = 100.0;
/// TS `MAX_CLOSED_PNL_PAGES`, per window.
const MAX_PAGES: u32 = 20;
/// TS `DEFAULT_EXECUTIONS_LOOKBACK_MS`.
const LOOKBACK_MS: f64 = 7.0 * 24.0 * 3_600_000.0;

pub struct Request {
    history: ReadCursorHistory<String>,
    category: Category,
    symbol: Option<String>,
    windows: VecDeque<(Option<f64>, Option<f64>)>,
    window: (Option<f64>, Option<f64>),
}

#[derive(Deserialize)]
struct Page<'a> {
    #[serde(borrow)]
    result: PageResult<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageResult<'a> {
    #[serde(borrow)]
    list: Vec<&'a RawValue>,
    #[serde(borrow, default)]
    next_page_cursor: Option<Cow<'a, str>>,
}

impl BybitLinear {
    pub(crate) fn start_closed_positions(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: Option<String>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    ) {
        // Every page is a leg of this caller root; its deadline bounds the walk.
        let Some(root) = self.caller_read_root(cx, true) else {
            return;
        };
        let completion = match ReadCompletion::from_context(cx, command, root.clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(&root, error.venue_error());
                return;
            }
        };
        if self.variant.is_spot() {
            let _ = cx.fail_context(&root, VenueError::new("Bybit spot has no closed positions"));
            return;
        }
        let category = if self.variant.is_inverse() {
            Category::Inverse
        } else {
            symbol
                .as_deref()
                .map_or(Category::Linear, Category::for_symbol)
        };
        if category == Category::Option {
            let symbol = symbol.unwrap_or_default();
            let _ = cx.fail_context(
                &root,
                VenueError::new(format!("Bybit has no closed PnL for options ({symbol})")),
            );
            return;
        }
        if self.credentials.is_none() {
            let _ = cx.done_context(&root, "[]".into());
            return;
        }
        let to = to_ms.unwrap_or(cx.now);
        let from = from_ms.unwrap_or(to - LOOKBACK_MS);
        let mut request = Request {
            history: CursorHistory::new(
                completion,
                CursorWalk::new(MAX_PAGES),
                |reason| {
                    match reason {
                Stall::PageBudget => format!("Closed-PnL paging hit the {MAX_PAGES}-page cap; older rows in this window were not fetched"),
                _ => "Closed-PnL paging repeated a cursor; older rows in this window were not fetched".into(),
            }
                },
                |rows| Ok(format!("[{}]", rows.join(","))),
            ),
            category,
            symbol,
            windows: windows(Some(from), Some(to)),
            window: (None, None),
        };
        request.window = request.windows.pop_front().unwrap_or_default();
        self.request_closed_page(cx, request);
    }

    fn request_closed_page(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let leg = match self.page_leg(cx, request.history.root(), true) {
            Ok(leg) => leg,
            Err(error) => {
                let _ = request.history.finish(cx, Err(error));
                return;
            }
        };
        let query = Query::new()
            .str("category", request.category.as_str())
            .opt_str("symbol", request.symbol.as_deref())
            .opt_num("startTime", request.window.0)
            .opt_num("endTime", request.window.1)
            .num("limit", PAGE_LIMIT)
            .opt_str("cursor", request.history.cursor());
        self.queue_read(
            cx,
            leg,
            Rest::get("/v5/position/closed-pnl", query).signed(),
            Then::Closed(Box::new(request)),
        );
    }

    /// A page that failed before any answer: typed by the engine or the signer.
    pub(crate) fn on_closed_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
    ) {
        error.message = format!("bybit closed-PnL fetch failed: {}", error.message);
        let _ = request.history.finish(cx, Err(error));
    }

    /// A page answer, delivered on `leg` (the page or its native re-read).
    pub(crate) fn on_closed_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        leg: &Context,
        response: Response<'_>,
    ) {
        // A superseded attempt is ignored; the live one still holds the owner.
        if request.history.validate_delivery(cx, leg).is_err() {
            return;
        }
        let page = match crate::read_refusal(&response, "bybit closed-PnL fetch failed: ") {
            None => serde_json::from_str::<Page<'_>>(response.body).map_err(|e| {
                VenueError::new(format!(
                    "bybit closed-PnL fetch failed: malformed closed-PnL page: {e}"
                ))
            }),
            Some(error) => Err(error),
        };
        let page = match page {
            Ok(page) => page,
            Err(error) => {
                let _ = request.history.finish(cx, Err(error));
                return;
            }
        };
        let result = page.result;
        let count = result.list.len();
        let cursor = result
            .next_page_cursor
            .filter(|cursor| !cursor.is_empty())
            .map(Cow::into_owned);
        let step = {
            let Ok(mut admitted) = request.history.admit(cx, leg) else {
                return;
            };
            for row in result.list {
                admitted.stage(row.get().to_owned());
            }
            admitted.advance(CursorPage {
                next: cursor,
                rows: count,
                fresh: count,
                want_more: true,
            })
        };
        match step {
            CursorStep::More(_) => self.request_closed_page(cx, request),
            CursorStep::Done | CursorStep::Stalled(_) => match request.windows.pop_front() {
                Some(window) => {
                    request.window = window;
                    if !request.history.restart_stage() {
                        let _ = request.history.finish(cx, Ok(()));
                        return;
                    }
                    self.request_closed_page(cx, request);
                }
                None => {
                    let _ = request.history.finish(cx, Ok(()));
                }
            },
        }
    }
}
