//! Rare reads stream typed rows. Private pages walk backwards using exact
//! nanosecond cursors, because many executions can share a millisecond.
use crate::account::{OrderRow, id, side};
use crate::sign::Params;
use crate::{Cryptocom, Pending};
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, Cx, Method, Query, TimerId, VenueError, num};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
enum Kind {
    Fills,
    Orders,
    Income,
}
pub struct Request {
    root: ReadRoot,
    leg: Option<ReadLeg>,
    kind: Kind,
    path: &'static str,
    symbol: Option<String>,
    order_id: Option<String>,
    income_kind: Option<String>,
    from_ns: u64,
    to_ns: u64,
    limit: usize,
    history: Box<ReadCursorHistory<Row>>,
    seen: HashSet<String>,
    page_count: usize,
    oldest: u64,
    invalid: bool,
    retries: u8,
    exact_page: bool,
    page_seen: Vec<String>,
}
#[derive(Default)]
pub struct History {
    pub waiting: HashMap<TimerId, Request>,
    next_by_path: HashMap<&'static str, f64>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Row {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: &'static str,
    commission: f64,
    commission_asset: String,
    maker: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: &'static str,
    #[serde(rename = "type")]
    kind: safe_cex_engine::model::OrderType,
    status: safe_cex_engine::model::OrderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: f64,
    updated_time: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: String,
    income: f64,
    asset: String,
    timestamp: f64,
    tran_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_id: Option<String>,
    category: &'static str,
}
pub(crate) fn millis_ns(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    (value.trunc() as u64)
        .checked_mul(1_000_000)?
        .checked_add((value.fract() * 1_000_000.0).round() as u64)
}

fn side_name(s: &str) -> &'static str {
    if side(s) == safe_cex_engine::model::OrderSide::Buy {
        "buy"
    } else {
        "sell"
    }
}
fn exact_ns(raw: Option<&RawValue>) -> Option<u64> {
    raw.and_then(|r| id(r).parse().ok())
}
fn ns(raw: Option<&RawValue>, ms: f64) -> u64 {
    exact_ns(raw).unwrap_or_else(|| (ms as u64).saturating_mul(1_000_000))
}

#[derive(Deserialize)]
struct FillRow<'a> {
    #[serde(borrow)]
    trade_id: &'a RawValue,
    #[serde(borrow)]
    order_id: &'a RawValue,
    instrument_name: &'a str,
    side: &'a str,
    traded_price: &'a str,
    traded_quantity: &'a str,
    #[serde(default)]
    fees: Option<&'a str>,
    #[serde(default)]
    fee_instrument_name: Option<&'a str>,
    #[serde(default)]
    taker_side: Option<&'a str>,
    create_time: f64,
    #[serde(borrow, default)]
    create_time_ns: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct OrderHistoryRow<'a> {
    #[serde(default)]
    avg_price: Option<&'a str>,
    #[serde(borrow, default)]
    create_time_ns: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct IncomeRow<'a> {
    #[serde(borrow)]
    journal_id: &'a RawValue,
    instrument_name: &'a str,
    journal_type: &'a str,
    transaction_qty: &'a str,
    #[serde(default)]
    realized_pnl: Option<&'a str>,
    #[serde(borrow, default)]
    trade_id: Option<&'a RawValue>,
    event_timestamp_ms: f64,
    #[serde(borrow, default)]
    event_timestamp_ns: Option<&'a RawValue>,
}

impl Request {
    fn params(&self) -> Params {
        let mut params = Params::default()
            .number("start_time", self.from_ns)
            .number("end_time", self.to_ns)
            .number("limit", 100);
        if let Some(symbol) = &self.symbol {
            params = params.text("instrument_name", symbol);
        }
        if let Some(kind) = &self.income_kind {
            params = params.text("journal_type", kind);
        }
        params
    }
    pub(crate) fn root(&self) -> &Context {
        self.root.context()
    }
    pub(crate) fn delivered(&self, cx: &Cx<'_, Pending>) -> bool {
        self.leg
            .as_ref()
            .is_some_and(|leg| self.root.validate_delivery(cx, leg).is_ok())
    }
    fn retire_leg(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(leg) = self.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
    }
    fn push(&mut self, cx: &Cx<'_, Pending>, identity: String, time: u64, row: Row, matches: bool) {
        self.page_count += 1;
        self.oldest = self.oldest.min(time);
        if time >= self.from_ns && time < self.to_ns && self.seen.insert(identity.clone()) {
            self.page_seen.push(identity);
            if matches
                && let Some(leg) = &self.leg
                && let Ok(mut page) = self.history.admit(cx, leg.context())
            {
                page.stage(row);
            }
        }
    }
}

impl Cryptocom {
    pub(crate) fn query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
        let (kind, path, symbol, order_id, income_kind, from_ms, to_ms, limit) = match query {
            Query::OrderByClientId { .. } => {
                let Ok(context) = cx.admit_http_command(crate::read_policy()) else {
                    return;
                };
                let _ = cx.fail_context(
                    &context,
                    "Crypto.com client order lookup is not supported by the Rust engine",
                );
                return;
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Fills,
                "private/get-trades",
                symbol,
                order_id,
                None,
                from_ms,
                to_ms,
                limit,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Orders,
                "private/get-order-history",
                symbol,
                None,
                None,
                from_ms,
                to_ms,
                limit,
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Income,
                "private/get-transactions",
                symbol,
                None,
                kind,
                from_ms,
                to_ms,
                limit,
            ),
            Query::PublicTrades { symbol, limit } => {
                self.public_trades_start(cx, command, symbol, limit);
                return;
            }
        };
        if self.credentials.is_none() {
            let Ok(context) = cx.admit_http_command(crate::read_policy()) else {
                return;
            };
            let _ = cx.fail_context(&context, "Crypto.com credentials are required");
            return;
        }
        let to = to_ms.unwrap_or(cx.now);
        let from = from_ms.unwrap_or(to - 86_400_000.0);
        if !from.is_finite() || !to.is_finite() || from < 0.0 || to < from {
            let Ok(context) = cx.admit_http_command(crate::read_policy()) else {
                return;
            };
            let _ = cx.fail_context(&context, "Invalid history time range");
            return;
        }
        let (Some(from_ns), Some(to_ns)) = (millis_ns(from), millis_ns(to)) else {
            let Ok(context) = cx.admit_http_command(crate::read_policy()) else {
                return;
            };
            let _ = cx.fail_context(&context, "History time range exceeds nanosecond bounds");
            return;
        };
        let limit = if matches!(kind, Kind::Income) && from_ms.is_some() && to_ms.is_some() {
            usize::MAX
        } else {
            limit.unwrap_or(100) as usize
        };
        if limit == 0 || from_ns == to_ns {
            let Ok(context) = cx.admit_http_command(crate::read_policy()) else {
                return;
            };
            let _ = cx.done_context(&context, "[]".into());
            return;
        }
        if matches!(
            cx.command_wait(),
            Some(safe_cex_engine::CommandWait::Expired)
        ) {
            let _ = cx.reject_expired_command(safe_cex_engine::request_context::Operation::Read);
            return;
        }
        let Ok(root) = SelectedRead::command_root(
            cx,
            self.selected_read(),
            LocalRefusal {
                diagnostic: None,
                error: VenueError::new("Crypto.com credentials are required"),
            },
        ) else {
            return;
        };
        let completion = match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(root.context(), error.read_venue_error());
                return;
            }
        };
        let request = Request {
            root,
            leg: None,
            kind,
            path,
            symbol,
            order_id,
            income_kind,
            from_ns,
            to_ns,
            limit,
            history: Box::new(ReadCursorHistory::new(
                completion,
                CursorWalk::starting_at(u32::MAX, to_ns.to_string()),
                |_| "Crypto.com history page safety bound exhausted".into(),
                |rows| {
                    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
                },
            )),
            seen: HashSet::new(),
            page_count: 0,
            oldest: u64::MAX,
            invalid: false,
            retries: 0,
            exact_page: true,
            page_seen: vec![],
        };
        self.history_request(cx, request);
    }
    pub(crate) fn history_request(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        if cx.validate_read_root_context(request.root()).is_err() {
            return;
        }
        let next = self
            .history
            .next_by_path
            .entry(request.path)
            .or_insert(cx.now);
        if *next > cx.now {
            let timer = cx.set_timer(*next - cx.now);
            self.history.waiting.insert(timer, request);
            return;
        }
        let leg = match request.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                self.history_reject(cx, request, error.read_venue_error());
                return;
            }
        };
        *next = cx.now
            + match request.kind {
                Kind::Fills | Kind::Orders => 1000.0,
                Kind::Income => 100.0,
            };
        let path = request.path;
        let params = request.params();
        let url = format!("{}/{path}", self.rest);
        if let Err((Pending::History(request), error)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Post,
                url: url.clone(),
                body: None,
                stream: Some(safe_cex_engine::json::ArrayAt::Path(&["result", "data"])),
            },
            |identity| {
                Box::new(crate::sign::Envelope::http(
                    identity.clone(),
                    path,
                    params,
                    &url,
                ))
            },
            |leg| {
                Pending::History(Request {
                    leg: Some(leg),
                    ..request
                })
            },
        ) {
            self.history_reject(cx, request, error.read_venue_error());
        }
    }
    pub(crate) fn history_reject(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: VenueError,
    ) {
        request.retire_leg(cx);
        let _ = request.history.reject(cx, error);
    }
    pub(crate) fn history_item(&self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        if !request.delivered(cx) {
            return;
        }
        let parsed = match request.kind {
            Kind::Fills => serde_json::from_str::<FillRow>(item).map(|r| {
                request.exact_page &= exact_ns(r.create_time_ns).is_some();
                let time = ns(r.create_time_ns, r.create_time);
                let order = id(r.order_id);
                let matches = request.order_id.as_ref().is_none_or(|id| *id == order);
                let identity = id(r.trade_id);
                let row = Execution {
                    id: identity.clone(),
                    timestamp: (r.create_time / 1000.0).floor(),
                    symbol: r.instrument_name.into(),
                    order_id: order,
                    price: num::parse(r.traded_price),
                    qty: num::parse(r.traded_quantity),
                    side: side_name(r.side),
                    commission: -r.fees.map_or(0.0, num::parse),
                    commission_asset: r.fee_instrument_name.unwrap_or_default().into(),
                    maker: r.taker_side == Some("MAKER"),
                };
                request.push(cx, identity, time, Row::Fill(row), matches);
            }),
            Kind::Orders => serde_json::from_str::<OrderRow>(item).and_then(|raw| {
                let r: OrderHistoryRow<'_> = serde_json::from_str(item)?;
                request.exact_page &= exact_ns(r.create_time_ns).is_some();
                let order = raw.order(0.0, None);
                let created = raw.create_time.or(raw.time_created).unwrap_or(0.0);
                let row = HistoricalOrder {
                    id: order.id.clone(),
                    symbol: order.symbol,
                    side: side_name(raw.side),
                    kind: order.kind,
                    status: order.status,
                    price: raw.limit_price.map(num::parse).filter(|p| *p > 0.0),
                    average_price: r.avg_price.map(num::parse).filter(|p| *p > 0.0),
                    amount: order.amount,
                    filled: order.filled,
                    reduce_only: order.reduce_only,
                    created_time: created,
                    updated_time: order.last_update,
                };
                request.push(
                    cx,
                    order.id,
                    ns(r.create_time_ns, created),
                    Row::Order(row),
                    true,
                );
                Ok(())
            }),
            Kind::Income => serde_json::from_str::<IncomeRow>(item).map(|r| {
                request.exact_page &= exact_ns(r.event_timestamp_ns).is_some();
                let identity = id(r.journal_id);
                let (category, amount) = match r.journal_type {
                    "REALIZED_PNL" | "TRADING" | "SESSION_SETTLE" | "SESSION_RESET" => {
                        ("realized_pnl", r.realized_pnl.map_or(0.0, num::parse))
                    }
                    "FUNDING" => ("funding", num::parse(r.transaction_qty)),
                    "TRADE_FEE" | "LIQUIDATION_FEE" => {
                        ("commission", num::parse(r.transaction_qty))
                    }
                    "ONCHAIN_WITHDRAWAL" | "ONCHAIN_DEPOSIT" | "SUBACCOUNT_TX" | "DEPOSIT"
                    | "WITHDRAW" | "ROLLBACK_DEPOSIT" | "ROLLBACK_WITHDRAW" => {
                        ("transfer", num::parse(r.transaction_qty))
                    }
                    _ => ("other", num::parse(r.transaction_qty)),
                };
                let row = Income {
                    symbol: r.instrument_name.into(),
                    income_type: r.journal_type.into(),
                    income: amount,
                    asset: cx
                        .store
                        .market(r.instrument_name)
                        .map_or_else(|| r.instrument_name.into(), |m| m.quote.clone()),
                    timestamp: r.event_timestamp_ms,
                    tran_id: identity.clone(),
                    trade_id: r.trade_id.map(id).filter(|i| i != "0" && i != "null"),
                    category,
                };
                request.push(
                    cx,
                    identity,
                    ns(r.event_timestamp_ns, r.event_timestamp_ms),
                    Row::Income(row),
                    true,
                );
            }),
        };
        request.invalid |= parsed.is_err();
    }
    pub(crate) fn history_response(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        if request.invalid {
            self.history_finish(cx, request, Err(VenueError::new("invalid history row")));
            return;
        }
        let mut next_bound = None;
        if request.page_count > 0
            && request.history.accepted_len() + request.history.staged_len() < request.limit
            // A short page of only prior identities proves native exhaustion,
            // even when its rows omit exact cursors. Full pages may move older.
            && (!request.page_seen.is_empty() || request.page_count >= 100)
        {
            if !request.exact_page {
                let _ = request.history.reject(
                    cx,
                    VenueError::new("Crypto.com history page omitted nanosecond cursors"),
                );
                return;
            }
            let next = request.oldest.saturating_add(1);
            if request.oldest < request.from_ns { /* range exhausted */
            } else if next >= request.to_ns {
                let _ = request.history.reject(
                    cx,
                    VenueError::new("Crypto.com history cursor did not advance"),
                );
                return;
            } else {
                next_bound = Some(next);
            }
        }
        let Some(leg) = request.leg.as_ref() else {
            return;
        };
        let Ok(mut page) = request.history.admit(cx, leg.context()) else {
            return;
        };
        let step = page.advance(CursorPage {
            next: next_bound.map(|bound| bound.to_string()),
            rows: request.page_count,
            fresh: request.page_seen.len(),
            want_more: next_bound.is_some(),
        });
        request.retire_leg(cx);
        match step {
            CursorStep::More(_) => {
                let Some(next) = next_bound else {
                    let _ = request.history.reject(
                        cx,
                        VenueError::new("Crypto.com missing native history bound"),
                    );
                    return;
                };
                request.page_seen.clear();
                request.exact_page = true;
                request.to_ns = next;
                request.page_count = 0;
                request.oldest = u64::MAX;
                request.retries = 0;
                let timer = cx.set_timer(1000.0);
                self.history.waiting.insert(timer, request);
            }
            CursorStep::Done => self.history_finish(cx, request, Ok(())),
            CursorStep::Stalled(_) => {
                let _ = request.history.reject(
                    cx,
                    VenueError::new("Crypto.com history page safety bound exhausted"),
                );
            }
        }
    }
    fn history_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        result: Result<(), VenueError>,
    ) {
        request.retire_leg(cx);
        let limit = request.limit;
        let _ = request.history.finish_with(cx, result, move |mut rows| {
            rows.truncate(limit);
            serde_json::to_string(&rows).map_err(|error| VenueError::new(error.to_string()))
        });
    }
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: VenueError,
        recovery: crate::reads::FailureRecovery,
    ) {
        request.retire_leg(cx);
        if cx.validate_read_root_context(request.root()).is_err() {
            return;
        }
        if matches!(recovery, crate::reads::FailureRecovery::Strict) {
            let _ = request.history.reject(cx, error);
            return;
        }
        if recovery.retry(&error.message, request.retries) {
            // A failed page is never appended; preserve earlier complete pages.
            if !request.history.discard_staged_page() {
                let _ = request
                    .history
                    .reject(cx, VenueError::new("Crypto.com history page reset failed"));
                return;
            }
            for identity in request.page_seen.drain(..) {
                request.seen.remove(&identity);
            }
            request.page_count = 0;
            request.oldest = u64::MAX;
            request.invalid = false;
            request.exact_page = true;
            request.retries += 1;
            let timer = cx.set_timer(1000.0);
            self.history.waiting.insert(timer, request);
        } else {
            self.history_finish(cx, request, Err(error));
        }
    }
}
