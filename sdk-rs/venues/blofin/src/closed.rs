//! Venue-reported closed positions: raw `/api/v1/account/positions-history`
//! rows, which the host maps with the TS mappers. Rows dedupe on `historyId`,
//! not `positionId`, and page by the smallest `historyId` seen.
use super::*;
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::url::encode;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashSet;

const PAGE: usize = 100;
const MAX_PAGES: u32 = 100;

pub struct Request {
    command: CommandId,
    instrument: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    after: Option<String>,
    page: u32,
    count: usize,
    added: usize,
    oldest: Option<String>,
    seen: HashSet<String>,
    rows: Vec<String>,
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
        if !self.signed() {
            cx.done(command, "[]".into());
            return;
        }
        let instrument = match symbol {
            Some(symbol) => match cx.store.market(&symbol) {
                Some(market) => Some(market.id.clone()),
                None => {
                    cx.done(command, "[]".into());
                    return;
                }
            },
            None => None,
        };
        self.request_closed_page(
            cx,
            Request {
                command,
                instrument,
                from: from_ms,
                to: to_ms,
                after: None,
                page: 0,
                count: 0,
                added: 0,
                oldest: None,
                seen: HashSet::new(),
                rows: Vec::new(),
                failed: false,
            },
        );
    }

    fn request_closed_page(&self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        request.page += 1;
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
        if let Some(after) = &request.after {
            params.push(format!("after={}", encode(after)));
        }
        let path = format!("/api/v1/account/positions-history?{}", params.join("&"));
        match self.headers(&path, Method::Get, cx.now, "") {
            Ok(headers) => {
                cx.request_streamed(
                    Method::Get,
                    format!("{}{path}", self.host()),
                    headers,
                    ArrayAt::EnvelopeField("data"),
                    Pending::Closed(request),
                );
            }
            Err(error) => cx.fail(request.command, error),
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
            && !request.seen.insert(id)
        {
            return;
        }
        request.added += 1;
        request.rows.push(item.to_owned());
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
        if request.count == 0 || request.added == 0 || request.count < PAGE {
            return Self::finish_closed(cx, request);
        }
        let next = request.oldest.take().unwrap_or_default();
        if next.is_empty() || request.after.as_deref() == Some(next.as_str()) {
            return Self::finish_closed(cx, request);
        }
        if request.page >= MAX_PAGES {
            cx.event(Level::Warn, code::HISTORY_INCOMPLETE,
                format!(
                    "BloFin closed positions stopped at the {MAX_PAGES}-page cap; older positions were not fetched"
                ),
                vec![("command", safe_cex_engine::Value::Num(f64::from(request.command)))],
            );
            return Self::finish_closed(cx, request);
        }
        request.after = Some(next);
        self.request_closed_page(cx, request);
    }

    pub(crate) fn closed_failed(cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        cx.fail(request.command, format!("BloFin closed positions: {error}"));
    }

    fn finish_closed(cx: &mut Cx<'_, Pending>, request: Request) {
        cx.done(request.command, format!("[{}]", request.rows.join(",")));
    }
}
