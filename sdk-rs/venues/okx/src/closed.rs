//! Venue-reported closed positions: raw `/api/v5/account/positions-history`
//! rows, which the host maps with the TS mappers. `begin`/`end` are ignored by
//! this endpoint; `before`/`after` are close-time bounds, paged by oldest `uTime`.
use crate::{Okx, Pending};
use safe_cex_engine::url::encode;
use safe_cex_engine::{CommandId, Cx, LogLevel, TimerId, num};
use serde::Deserialize;

const PAGE: usize = 100;
const MAX_PAGES: usize = 100;
const PAGE_DELAY_MS: f64 = 100.0;

#[derive(Default)]
pub struct ClosedState {
    delayed: Vec<(TimerId, Request)>,
}

pub struct Request {
    command: CommandId,
    instrument: Option<String>,
    before: Option<f64>,
    after: Option<String>,
    pages: usize,
    count: usize,
    oldest: Option<i64>,
    rows: Vec<String>,
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
                before: from_ms,
                after: to_ms.map(num::format),
                pages: 0,
                count: 0,
                oldest: None,
                rows: Vec::new(),
            },
        );
    }

    fn request_closed_page(&self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        request.pages += 1;
        request.count = 0;
        request.oldest = None;
        let mut path = String::from("/api/v5/account/positions-history?instType=SWAP");
        if let Some(id) = &request.instrument {
            path.push_str(&format!("&instId={}", encode(id)));
        }
        if let Some(before) = request.before {
            path.push_str(&format!("&before={}", num::format(before)));
        }
        if let Some(after) = &request.after {
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
        request.rows.push(item.to_owned());
    }

    pub(crate) fn closed_response(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        if request.count < PAGE {
            return Self::finish_closed(cx, request);
        }
        let Some(oldest) = request.oldest else {
            return Self::finish_closed(cx, request);
        };
        let next = (oldest - 1).to_string();
        if request.after.as_deref() == Some(next.as_str()) {
            return Self::finish_closed(cx, request);
        }
        if request.pages >= MAX_PAGES {
            cx.log(
                LogLevel::Warning,
                format!(
                    "OKX closed positions stopped at the {MAX_PAGES}-page cap; older positions were not fetched"
                ),
            );
            return Self::finish_closed(cx, request);
        }
        request.after = Some(next);
        let timer = cx.set_timer(PAGE_DELAY_MS);
        self.closed.delayed.push((timer, request));
    }

    pub(crate) fn closed_failed(cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        cx.fail(request.command, format!("OKX closed positions: {error}"));
    }

    fn finish_closed(cx: &mut Cx<'_, Pending>, request: Request) {
        cx.done(request.command, format!("[{}]", request.rows.join(",")));
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
