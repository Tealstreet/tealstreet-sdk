//! Unsigned public history retains one caller root across physical pages.
use crate::history::{Atom, iso, side, stall_message, timestamp};
use crate::sign::form;
use crate::wire::number;
use crate::{Kraken, Pending, public_context};
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::OrderSide;
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::request_context::{CommandEntryError, Context, Operation};
use safe_cex_engine::{Command, Cx, Method, Query, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub struct Request {
    history: ReadCursorHistory<Trade>,
    current: Option<Context>,
    symbol: String,
    limit: usize,
    count: usize,
    fresh: usize,
    oldest: f64,
    failed: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Trade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    timestamp: f64,
    side: OrderSide,
}
#[derive(Deserialize)]
struct TradeWire<'a> {
    #[serde(default, borrow)]
    trade_id: Atom<'a>,
    #[serde(default, borrow)]
    uid: Cow<'a, str>,
    #[serde(borrow)]
    time: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    size: f64,
    #[serde(deserialize_with = "number")]
    price: f64,
}
#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(default, borrow)]
    result: Cow<'a, str>,
    #[serde(default, borrow)]
    error: Cow<'a, str>,
    #[serde(default, borrow)]
    status: Cow<'a, str>,
    #[serde(default, borrow)]
    reason: Cow<'a, str>,
    #[serde(default)]
    history: Option<serde::de::IgnoredAny>,
}
impl Request {
    pub(crate) fn delivery(&self, cx: &Cx<'_, Pending>, child: &Context) -> bool {
        self.current.as_ref() == Some(child) && self.history.validate_delivery(cx, child).is_ok()
    }
    fn finish(self, cx: &mut Cx<'_, Pending>, result: Result<(), VenueError>) {
        let limit = self.limit;
        let _ = self.history.finish_with(cx, result, move |mut rows| {
            rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
            rows.truncate(limit);
            serde_json::to_string(&rows).map_err(|error| VenueError::new(error.to_string()))
        });
    }
}
impl Kraken {
    pub(crate) fn public_trades_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: &Command,
    ) -> bool {
        let Command::Query {
            command,
            query: Query::PublicTrades { symbol, limit },
        } = command
        else {
            return false;
        };
        let root = match cx.admit_caller_read_root(None, None) {
            Ok(root) => root,
            Err(CommandEntryError::Rejected(_)) => return true,
            Err(CommandEntryError::Revoked) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return true;
            }
        };
        let to = cx.now;
        let from = (to - 7.0 * 86_400_000.0).max(0.0);
        if !from.is_finite()
            || !to.is_finite()
            || from < 0.0
            || to < from
            || to > 253_402_300_799_999.0
        {
            let _ = cx.fail_context(
                &root,
                "Kraken history range must contain valid milliseconds in ascending order",
            );
            return true;
        }
        let completion = match ReadCompletion::from_context(cx, *command, root.clone()) {
            Ok(completion) => completion,
            Err(reason) => {
                let _ = cx.fail_context(&root, reason.venue_error());
                return true;
            }
        };
        self.public_trades_request(
            cx,
            Request {
                history: ReadCursorHistory::new(
                    completion,
                    CursorWalk::new(50)
                        .stall_on_empty_page()
                        .stall_on_no_new_rows(),
                    stall_message,
                    |rows| {
                        serde_json::to_string(rows)
                            .map_err(|error| VenueError::new(error.to_string()))
                    },
                ),
                current: None,
                symbol: symbol.clone(),
                limit: limit.filter(|n| *n > 0).map_or(100, |n| n as usize),
                count: 0,
                fresh: 0,
                oldest: f64::INFINITY,
                failed: None,
            },
        );
        true
    }
    fn public_trades_request(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        let child = match cx.http_leg_context(
            request.history.root(),
            public_context::policy(Operation::Read),
        ) {
            Ok(child) => child,
            Err(reason) => {
                self.public_trades_preparation_failed(cx, request, None, reason.venue_error());
                return;
            }
        };
        let symbol = request.symbol.to_ascii_lowercase();
        let mut params = vec![("symbol", symbol)];
        if let Some(cursor) = request.history.cursor() {
            params.push(("lastTime", cursor.to_owned()));
        }
        let url = format!(
            "{}/derivatives/api/v3/history?{}",
            self.host(),
            form(&params)
        );
        request.count = 0;
        request.fresh = 0;
        request.oldest = f64::INFINITY;
        request.failed = None;
        request.current = Some(child.clone());
        let pending = Pending::PublicTrades(Box::new(request), child.clone());
        match cx.request_managed(child, Method::Get, url, None, pending) {
            Ok(id) => cx.stream_response(id, ArrayAt::Field("history")),
            Err((Pending::PublicTrades(request, child), reason)) => {
                self.public_trades_preparation_failed(
                    cx,
                    *request,
                    Some(child),
                    reason.venue_error(),
                );
            }
            Err(_) => unreachable!(),
        }
    }
    pub(crate) fn public_trades_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        child: &Context,
        text: &str,
    ) {
        if !request.delivery(cx, child) || request.failed.is_some() {
            return;
        }
        if let Err(error) = Self::public_trade_row(cx, request, child, text) {
            request.failed = Some(error);
        }
    }
    fn public_trade_row(
        cx: &Cx<'_, Pending>,
        request: &mut Request,
        child: &Context,
        text: &str,
    ) -> Result<(), String> {
        let row: TradeWire<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let time = timestamp(&row.time).ok_or("Kraken trade has an invalid ISO timestamp")?;
        let id = if row.uid.is_empty() {
            row.trade_id.0.into_owned()
        } else {
            row.uid.into_owned()
        };
        let native_side = side(&row.side)?;
        if row.price <= 0.0 || row.size <= 0.0 || !(row.price * row.size).is_finite() {
            return Err("Kraken trade has invalid economics".into());
        }
        if !time.is_finite() || time < 0.0 || id.is_empty() {
            return Err("Kraken history row has an invalid timestamp or identity".into());
        }
        let Ok(mut admitted) = request.history.admit(cx, child) else {
            return Ok(());
        };
        request.count += 1;
        request.oldest = request.oldest.min(time);
        if admitted.fresh(id.clone()) {
            request.fresh += 1;
            admitted.stage(Trade {
                id,
                symbol: request.symbol.clone(),
                price: row.price,
                qty: row.size,
                quote_qty: row.price * row.size,
                timestamp: time,
                side: native_side,
            });
        }
        Ok(())
    }
    pub(crate) fn public_trades_http(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        child: Context,
        response: Response<'_>,
    ) {
        if !request.delivery(cx, &child) {
            let _ = cx.finish_http_context(&child);
            return;
        }
        let checked = (|| {
            let envelope: Envelope<'_> = serde_json::from_str(response.body)
                .map_err(|error| format!("invalid envelope: {error}"))?;
            if !(200..300).contains(&response.status)
                || !envelope.error.is_empty()
                || !(envelope.result.is_empty() || envelope.result == "success")
                || !(envelope.status.is_empty() || envelope.status == "success")
            {
                return Err(format!(
                    "HTTP {} {} {} {} {}",
                    response.status,
                    envelope.result,
                    envelope.status,
                    envelope.error,
                    envelope.reason
                ));
            }
            if envelope.history.is_none() {
                return Err("required result array is absent".into());
            }
            if let Some(error) = request.failed.take() {
                return Err(error);
            }
            Ok(())
        })();
        if let Err(error) = checked {
            self.public_trades_failed(
                cx,
                request,
                child,
                VenueError::new(format!("Kraken history failed: {error}")),
            );
            return;
        }
        let next = if request.count < 100
            || request.history.accepted_len() + request.history.staged_len() >= request.limit
        {
            None
        } else {
            Some(iso(request.oldest.floor() + 1.0))
        };
        let page = CursorPage {
            next,
            rows: request.count,
            fresh: request.fresh,
            want_more: true,
        };
        let step = match request.history.admit(cx, &child) {
            Ok(mut admitted) => admitted.advance_with_coverage(page, None),
            Err(_) => {
                let _ = cx.finish_http_context(&child);
                return;
            }
        };
        let _ = cx.finish_http_context(&child);
        request.current = None;
        match step {
            CursorStep::More(_) => self.public_trades_request(cx, request),
            CursorStep::Done => request.finish(cx, Ok(())),
            CursorStep::Stalled(reason) => {
                let _ = request
                    .history
                    .reject(cx, VenueError::new(stall_message(reason)));
            }
        }
    }
    pub(crate) fn public_trades_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        child: Option<Context>,
        error: VenueError,
    ) {
        if let Some(child) = child {
            if request.current.as_ref() != Some(&child) {
                return;
            }
            let _ = cx.finish_http_context(&child);
        }
        let _ = request.history.reject(cx, error);
    }
    pub(crate) fn public_trades_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        child: Context,
        error: VenueError,
    ) {
        // Released failures retain engine attempt custody even after child timeout.
        if request.current.as_ref() != Some(&child)
            || cx
                .validate_read_root_context(request.history.root())
                .is_err()
        {
            return;
        }
        let _ = cx.finish_http_context(&child);
        if !request.history.discard_staged_page() {
            return;
        }
        request.finish(cx, Err(error));
    }
}
