//! One unsigned response, with native dequeue/retry policy under a caller root.
use crate::{Cryptocom, Pending, account, history::millis_ns, wire::Envelope};
use safe_cex_engine::classify::{self, CodeTable, Key, Native, NativeCode, Released, Row, Verdict};
use safe_cex_engine::history::{ReadCompletion, ReadHistoryRows};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Error, Operation, Pacing, Policy,
};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Response, TimerId, VenueError, num};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::HashMap;

pub(crate) const PACING: Pacing = Pacing {
    group: 1,
    minimum_gap_ms: 10.0,
};
const ERRORS: CodeTable = CodeTable {
    rows: &[Row {
        key: Key::Int(40101),
        verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
        only: None,
    }],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};
fn policy() -> Policy {
    Policy {
        pacing: Some(PACING),
        ..crate::read_policy()
    }
}
#[derive(Default)]
pub(crate) struct State {
    pub(crate) waiting: HashMap<TimerId, Request>,
    next: f64,
}
pub struct Request {
    symbol: String,
    limit: usize,
    rows: ReadHistoryRows<Trade>,
    leg: Option<Context>,
    invalid: bool,
    retries: u8,
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
    side: &'static str,
}
#[derive(Deserialize)]
struct WireTrade<'a> {
    #[serde(borrow)]
    d: &'a RawValue,
    i: &'a str,
    p: &'a str,
    q: &'a str,
    s: &'a str,
    t: f64,
}
impl Request {
    pub(crate) fn root(&self) -> &Context {
        self.rows.root()
    }
    pub(crate) fn delivered(&self, cx: &Cx<'_, Pending>) -> bool {
        self.leg
            .as_ref()
            .is_some_and(|leg| self.rows.validate_delivery(cx, leg).is_ok())
    }
    pub(crate) fn item(&mut self, cx: &Cx<'_, Pending>, item: &str) {
        let Some(leg) = &self.leg else {
            return;
        };
        let Ok(mut page) = self.rows.admit(cx, leg) else {
            return;
        };
        match serde_json::from_str::<WireTrade>(item) {
            Ok(row) => {
                let price = num::parse(row.p);
                let qty = num::parse(row.q);
                page.stage(Trade {
                    id: account::id(row.d),
                    symbol: row.i.into(),
                    price,
                    qty,
                    quote_qty: price * qty,
                    timestamp: row.t,
                    side: if account::side(row.s) == safe_cex_engine::model::OrderSide::Buy {
                        "buy"
                    } else {
                        "sell"
                    },
                });
            }
            Err(_) => self.invalid = true,
        }
    }
}

fn response_failure(response: &Response<'_>, message: String) -> VenueError {
    let envelope = serde_json::from_str::<Envelope>(response.body).ok();
    let native = envelope.as_ref().filter(|e| e.code != 0).map(|e| Native {
        code: NativeCode::Int(e.code),
        message: e.message,
    });
    classify::read(
        &ERRORS,
        Released::Response(response),
        native.as_ref(),
        message,
    )
    .error
}
impl Cryptocom {
    pub(crate) fn public_trades_start(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
        limit: Option<u32>,
    ) {
        let to = cx.now;
        let from = to - 86_400_000.0;
        let invalid = if !from.is_finite() || !to.is_finite() || from < 0.0 || to < from {
            Some("Invalid history time range")
        } else if millis_ns(from).is_none() || millis_ns(to).is_none() {
            Some("History time range exceeds nanosecond bounds")
        } else {
            None
        };
        if invalid.is_some() || limit == Some(0) {
            let Ok(context) = cx.admit_http_command(crate::read_policy()) else {
                return;
            };
            if let Some(error) = invalid {
                let _ = cx.fail_context(&context, error);
            } else {
                let _ = cx.done_context(&context, "[]".into());
            }
            return;
        }
        let root = match cx.admit_caller_read_root(None, Some(PACING)) {
            Ok(root) => root,
            Err(CommandEntryError::Revoked) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
            Err(CommandEntryError::Rejected(_)) => return,
        };
        let completion = match ReadCompletion::from_context(cx, command, root.clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(&root, error.read_venue_error());
                return;
            }
        };
        self.public_trades_request(
            cx,
            Request {
                symbol,
                limit: limit.unwrap_or(25) as usize,
                rows: ReadHistoryRows::new(completion, |rows| {
                    serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string()))
                }),
                leg: None,
                invalid: false,
                retries: 0,
            },
        );
    }
    pub(crate) fn public_trades_request(&mut self, cx: &mut Cx<'_, Pending>, mut trade: Request) {
        if let Err(error) = cx.validate_http_context(trade.root()) {
            if error != Error::Revoked {
                let _ = trade.rows.reject(cx, error.read_venue_error());
            }
            return;
        }
        if self.public_trades.next > cx.now {
            let timer = cx.set_timer(self.public_trades.next - cx.now);
            self.public_trades.waiting.insert(timer, trade);
            return;
        }
        let leg = match cx.http_leg_context(trade.root(), policy()) {
            Ok(leg) => leg,
            Err(error) => {
                let _ = trade.rows.reject(cx, error.read_venue_error());
                return;
            }
        };
        let url = format!(
            "{}/public/get-trades?instrument_name={}&count={}",
            self.rest,
            crate::encode(&trade.symbol),
            trade.limit.min(150)
        );
        trade.leg = Some(leg.clone());
        match cx.request_managed(
            leg,
            Method::Get,
            url,
            None,
            Pending::PublicTrades(Box::new(trade)),
        ) {
            Ok(id) => {
                self.public_trades.next = cx.now + 10.;
                cx.stream_response(id, ArrayAt::Path(&["result", "data"]));
            }
            Err((Pending::PublicTrades(trade), error)) => {
                Self::public_trades_reject(cx, *trade, error.read_venue_error())
            }
            Err(_) => unreachable!(),
        }
    }
    pub(crate) fn public_trades_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut trade: Request,
        response: Response<'_>,
        message: Option<String>,
    ) {
        if !trade.delivered(cx) {
            return;
        }
        if let Some(message) = message {
            let error = response_failure(&response, message);
            self.public_trades_failed(cx, trade, error, false);
            return;
        }
        if trade.invalid {
            Self::public_trades_reject(cx, trade, VenueError::new("invalid history row"));
            return;
        }
        let Some(leg) = trade.leg.take() else {
            return;
        };
        let Ok(mut page) = trade.rows.admit(cx, &leg) else {
            return;
        };
        page.accept_page();
        let _ = cx.finish_http_context(&leg);
        let limit = trade.limit;
        let _ = trade.rows.finish_with(cx, Ok(()), |mut rows| {
            rows.truncate(limit);
            serde_json::to_string(&rows).map_err(|e| VenueError::new(e.to_string()))
        });
    }
    pub(crate) fn public_trades_stream_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        trade: Request,
        response: Response<'_>,
    ) {
        if !trade.delivered(cx) {
            return;
        }
        let message = format!(
            "response did not contain the expected list: {}",
            response.body
        );
        let error = response_failure(&response, message);
        self.public_trades_failed(cx, trade, error, true);
    }
    pub(crate) fn public_trades_reject(
        cx: &mut Cx<'_, Pending>,
        mut trade: Request,
        error: VenueError,
    ) {
        if cx.validate_read_root_context(trade.root()).is_err() {
            return;
        }
        if let Some(leg) = trade.leg.take() {
            let _ = cx.finish_http_context(&leg);
        }
        let _ = trade.rows.reject(cx, error);
    }
    pub(crate) fn public_trades_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut trade: Request,
        error: VenueError,
        transport: bool,
    ) {
        if let Err(reason) = cx.validate_http_context(trade.root()) {
            if reason != Error::Revoked {
                let _ = trade.rows.reject(cx, reason.read_venue_error());
            }
            return;
        }
        if let Some(leg) = trade.leg.take() {
            let _ = cx.finish_http_context(&leg);
        }
        // Native compatibility: 40101 text historically caps even transport at
        // one retry. Remove this minimum check only with a caller policy change.
        let legacy_session = error
            .venue_code
            .as_deref()
            .into_iter()
            .chain(error.venue_message.as_deref())
            .chain(error.venue_code.is_none().then_some(error.message.as_str()))
            .any(|part| part.contains("40101"));
        let retry = if legacy_session {
            trade.retries < 1
        } else {
            transport && trade.retries < 3
        };
        if retry {
            trade.rows.discard_staged_page();
            trade.invalid = false;
            trade.retries += 1;
            let timer = cx.set_timer(1000.);
            self.public_trades.waiting.insert(timer, trade);
        } else {
            let _ = trade.rows.finish(cx, Err(error));
        }
    }
}
