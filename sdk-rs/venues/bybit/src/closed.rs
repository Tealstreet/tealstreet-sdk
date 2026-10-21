//! Venue-reported closed positions: raw `/v5/position/closed-pnl` rows, which
//! the host maps with the TS mappers (`mapBybitClosedPnl`, the journal adapter).
use crate::history::windows;
use crate::sign::Query;
use crate::{BybitLinear, Category, Pending, Rest, Then, venue_error};
use safe_cex_engine::{CommandId, Cx, LogLevel, Response};
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
    command: CommandId,
    category: Category,
    symbol: Option<String>,
    windows: VecDeque<(Option<f64>, Option<f64>)>,
    window: (Option<f64>, Option<f64>),
    cursor: Option<String>,
    pages: u32,
    rows: Vec<String>,
}

#[derive(Deserialize)]
struct Page<'a> {
    #[serde(borrow, default)]
    result: Option<PageResult<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageResult<'a> {
    #[serde(borrow, default)]
    list: Option<Vec<&'a RawValue>>,
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
        if self.variant.is_spot() {
            cx.fail(command, "Bybit spot has no closed positions");
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
            cx.fail(
                command,
                format!("Bybit has no closed PnL for options ({symbol})"),
            );
            return;
        }
        if self.credentials.is_none() {
            cx.done(command, "[]".into());
            return;
        }
        let to = to_ms.unwrap_or(cx.now);
        let from = from_ms.unwrap_or(to - LOOKBACK_MS);
        let mut request = Request {
            command,
            category,
            symbol,
            windows: windows(Some(from), Some(to)),
            window: (None, None),
            cursor: None,
            pages: 0,
            rows: Vec::new(),
        };
        request.window = request.windows.pop_front().unwrap_or_default();
        self.request_closed_page(cx, request);
    }

    fn request_closed_page(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let query = Query::new()
            .str("category", request.category.as_str())
            .opt_str("symbol", request.symbol.as_deref())
            .opt_num("startTime", request.window.0)
            .opt_num("endTime", request.window.1)
            .num("limit", PAGE_LIMIT)
            .opt_str("cursor", request.cursor.as_deref());
        self.send(
            cx,
            Rest::get("/v5/position/closed-pnl", query).signed(),
            Then::Closed(request),
        );
    }

    pub(crate) fn on_closed_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let page = match &response {
            Ok(response) => match venue_error(response) {
                None => serde_json::from_str::<Page<'_>>(response.body)
                    .map_err(|e| format!("malformed closed-PnL page: {e}")),
                Some((_, message)) => Err(message),
            },
            Err(error) => Err((*error).to_owned()),
        };
        let page = match page {
            Ok(page) => page,
            Err(message) => {
                cx.fail(
                    request.command,
                    format!("bybit closed-PnL fetch failed: {message}"),
                );
                return;
            }
        };
        let result = page.result.unwrap_or(PageResult {
            list: None,
            next_page_cursor: None,
        });
        request.rows.extend(
            result
                .list
                .unwrap_or_default()
                .into_iter()
                .map(|row| row.get().to_owned()),
        );
        request.pages += 1;
        let cursor = result
            .next_page_cursor
            .filter(|cursor| !cursor.is_empty())
            .map(Cow::into_owned);
        match cursor {
            Some(cursor) if request.pages < MAX_PAGES => {
                request.cursor = Some(cursor);
                self.request_closed_page(cx, request);
            }
            cursor => {
                if cursor.is_some() {
                    // Newest first, so the cap drops the oldest rows of this window.
                    cx.log(
                        LogLevel::Warning,
                        format!(
                            "Closed-PnL paging hit the {MAX_PAGES}-page cap; older rows in this window were not fetched"
                        ),
                    );
                }
                match request.windows.pop_front() {
                    Some(window) => {
                        request.window = window;
                        request.cursor = None;
                        request.pages = 0;
                        self.request_closed_page(cx, request);
                    }
                    None => cx.done(request.command, format!("[{}]", request.rows.join(","))),
                }
            }
        }
    }
}
