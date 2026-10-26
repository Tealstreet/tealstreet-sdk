//! Rare reads use exact native IDs and bounded, streamed pages. Funding income
//! includes Settled records only: Accrued rows are provisional and must not be
//! counted a second time when the same payment settles. Order timestamps retain
//! the SDK's millisecond contract; execution/income/trade timestamps are seconds.
//! Order history requests all lifecycle states and filters open rows locally:
//! the venue's COMPLETED shortcut omits REJECTED orders.
use crate::wire::{Id, N, checked};
use crate::{Orderly, Pending, symbol};
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Error as RequestError, Operation, Policy, RetryPolicy,
};
use safe_cex_engine::selected_read::{
    CommandReadError, LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, CommandWait, Cx, Method, Query, Response, TimerId, VenueError};
use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

const PAGE_SIZE: usize = 500;
const MAX_PAGES: u32 = 50;
const MAX_RESULTS: u32 = 1000;
const MAX_TIME: f64 = 253402300799000.0;
// Funding history permits twenty starts per sixty seconds per account. Reserve
// slots across commands and page continuations; the extra millisecond avoids
// treating both ends of a sixty-second window as twenty-one starts.
const FUNDING_SPACING: f64 = 3001.0;

const READ_POLICY: Policy = Policy {
    operation: Operation::Read,
    credential: None,
    timeout_ms: 20_000,
    pacing: None,
    retry: RetryPolicy::Never,
};

/// The caller's wait bounds the whole walk: an expired or unstamped receipt is
/// the shared typed refusal, never a defaulted root.
fn command_waits(cx: &mut Cx<'_, Pending>) -> bool {
    match cx.command_wait() {
        Some(CommandWait::Remaining(_)) => true,
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Read);
            false
        }
        None => {
            let _ = cx.reject_unstamped_command(Operation::Read);
            false
        }
    }
}

fn reject_local_history(cx: &mut Cx<'_, Pending>, error: impl Into<VenueError>) {
    match cx.command_wait() {
        Some(CommandWait::Remaining(_)) => {}
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Read);
            return;
        }
        None => {
            let _ = cx.reject_unstamped_command(Operation::Read);
            return;
        }
    }
    let Ok(context) = cx.admit_http_command(READ_POLICY) else {
        return;
    };
    let _ = cx.fail_context(&context, error);
}

#[derive(Default)]
pub struct HistoryState {
    delayed: HashMap<TimerId, Request>,
    next_funding_at: f64,
    last_funding_start: Option<f64>,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fills,
    Orders,
    Funding,
    Trades,
}
pub struct Request {
    owner: Box<ReadCursorHistory<Row>>,
    /// The signed root; public trades read under the command Context itself.
    root: Option<ReadRoot>,
    leg: Option<ReadLeg>,
    /// Mapped rows of the page in flight, staged only after its delivery
    /// validates at the response.
    page_rows: Vec<(String, Row)>,
    kind: Kind,
    symbol: Option<String>,
    native: Option<String>,
    order_id: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    algo: bool,
    page: u32,
    page_count: usize,
    page_new: usize,
    stage_count: usize,
    seen: HashSet<String>,
    occurrences: HashMap<String, u32>,
    invalid: Option<String>,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Row {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
    Trade(Trade),
}
impl Row {
    fn time(&self) -> f64 {
        match self {
            Self::Fill(r) => r.timestamp * 1000.0,
            Self::Order(r) => r.created_time,
            Self::Income(r) => r.timestamp * 1000.0,
            Self::Trade(r) => r.timestamp * 1000.0,
        }
    }
    fn id(&self) -> &str {
        match self {
            Self::Fill(r) => &r.id,
            Self::Order(r) => &r.id,
            Self::Income(r) => &r.tran_id,
            Self::Trade(r) => &r.id,
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    realized_pnl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maker: Option<bool>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: &'static str,
    #[serde(rename = "type")]
    kind: OrderType,
    status: OrderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_time: Option<f64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: &'static str,
    income: f64,
    asset: &'static str,
    timestamp: f64,
    tran_id: String,
    category: &'static str,
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
struct ExecutionWire<'a> {
    id: Id,
    order_id: Id,
    #[serde(borrow)]
    symbol: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    executed_price: N,
    executed_quantity: N,
    executed_timestamp: N,
    fee: N,
    #[serde(borrow)]
    fee_asset: Cow<'a, str>,
    realized_pnl: Option<N>,
    #[serde(default, deserialize_with = "maker")]
    is_maker: Option<bool>,
}
fn maker<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Maker {
        Bool(bool),
        Number(u8),
    }
    match Option::<Maker>::deserialize(d)? {
        None => Ok(None),
        Some(Maker::Bool(b)) => Ok(Some(b)),
        Some(Maker::Number(0)) => Ok(Some(false)),
        Some(Maker::Number(1)) => Ok(Some(true)),
        _ => Err(serde::de::Error::custom("Orderly invalid maker flag")),
    }
}
#[derive(Deserialize)]
struct OrderWire<'a> {
    #[serde(default)]
    order_id: Id,
    #[serde(default)]
    algo_order_id: Id,
    #[serde(default, borrow)]
    symbol: Cow<'a, str>,
    #[serde(default, borrow)]
    side: Cow<'a, str>,
    #[serde(rename = "type", default, borrow)]
    kind: Cow<'a, str>,
    #[serde(default, borrow)]
    algo_type: Cow<'a, str>,
    #[serde(default, borrow)]
    status: Cow<'a, str>,
    #[serde(default, borrow)]
    algo_status: Cow<'a, str>,
    #[serde(default, alias = "root_algo_order_status", borrow)]
    root_algo_status: Cow<'a, str>,
    price: Option<N>,
    trigger_price: Option<N>,
    average_executed_price: Option<N>,
    quantity: Option<N>,
    total_executed_quantity: Option<N>,
    executed: Option<N>,
    reduce_only: Option<bool>,
    created_time: Option<N>,
    updated_time: Option<N>,
    #[serde(default, borrow)]
    child_orders: Vec<OrderWire<'a>>,
}
#[derive(Clone, Copy, Default)]
struct OrderContext<'a> {
    symbol: &'a str,
    side: &'a str,
    status: &'a str,
    created: Option<N>,
    updated: Option<N>,
    reduce_only: Option<bool>,
}
#[derive(Deserialize)]
struct FundingWire<'a> {
    #[serde(borrow)]
    symbol: Cow<'a, str>,
    funding_rate: N,
    mark_price: N,
    funding_fee: N,
    #[serde(borrow)]
    payment_type: Cow<'a, str>,
    #[serde(borrow)]
    status: Cow<'a, str>,
    #[serde(default, borrow)]
    margin_mode: Cow<'a, str>,
    created_time: N,
}
#[derive(Deserialize)]
struct TradeWire<'a> {
    #[serde(borrow)]
    symbol: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    executed_price: N,
    executed_quantity: N,
    executed_timestamp: N,
}
#[derive(Deserialize)]
struct Page {
    meta: Option<Meta>,
    #[serde(rename = "rows")]
    _rows: Vec<serde::de::IgnoredAny>,
}
#[derive(Deserialize)]
struct Meta {
    total: u64,
    current_page: Option<u32>,
    records_per_page: Option<usize>,
}
fn time(value: f64) -> Result<f64, String> {
    if (0.0..=MAX_TIME).contains(&value) && value.fract() == 0.0 {
        Ok(value)
    } else {
        Err("Orderly invalid history timestamp".into())
    }
}
fn native_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || !id.bytes().all(|c| c.is_ascii_digit())
        || id.starts_with('0')
        || id.parse::<u64>().ok().is_none_or(|id| id == 0)
    {
        Err("Orderly history order ID must be a canonical positive uint64".into())
    } else {
        Ok(())
    }
}
fn side(raw: &str) -> Result<&'static str, String> {
    match raw {
        "BUY" => Ok("buy"),
        "SELL" => Ok("sell"),
        _ => Err("Orderly invalid history side".into()),
    }
}
impl Request {
    fn reject_owner(mut self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        self.release(cx);
        let _ = self.owner.reject(cx, error);
    }
    fn accepts(&self, symbol: &str, timestamp: f64) -> bool {
        self.symbol.as_ref().is_none_or(|s| s == symbol)
            && self.from.is_none_or(|t| timestamp >= t)
            && self.to.is_none_or(|t| timestamp <= t)
    }
    fn observe(&mut self, key: String) {
        if self.seen.insert(key) {
            self.page_new += 1;
        }
    }
    fn append(&mut self, key: String, row: Row) {
        self.page_rows.push((key, row));
    }
    /// The Context this page was delivered under.
    fn delivery(&self) -> Option<&Context> {
        match &self.root {
            Some(_) => self.leg.as_ref().map(ReadLeg::context),
            None => Some(self.owner.root()),
        }
    }
    pub(crate) fn valid_delivery(&self, cx: &Cx<'_, Pending>) -> bool {
        let root = match (&self.root, &self.leg) {
            (Some(root), Some(leg)) => root.validate_delivery(cx, leg).is_ok(),
            (Some(_), None) => false,
            (None, _) => true,
        };
        root && self
            .delivery()
            .is_some_and(|leg| self.owner.validate_delivery(cx, leg).is_ok())
    }
    pub(crate) fn release(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(leg) = self.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
    }
    /// Stage the validated page's rows in native order, under the limit.
    fn stage_page(&mut self, cx: &Cx<'_, Pending>) -> Result<(), RequestError> {
        let leg = self.delivery().ok_or(RequestError::Revoked)?.clone();
        let mut page = self.owner.admit(cx, &leg)?;
        for (key, row) in self.page_rows.drain(..) {
            if page.fresh(key) && self.stage_count < self.limit {
                self.stage_count += 1;
                page.stage(row);
            }
        }
        Ok(())
    }
    fn advance(
        &mut self,
        cx: &Cx<'_, Pending>,
        page: CursorPage,
    ) -> Result<CursorStep, RequestError> {
        let leg = self.delivery().ok_or(RequestError::Revoked)?.clone();
        Ok(self.owner.admit(cx, &leg)?.advance(page))
    }
    fn unpaged(&self) -> bool {
        self.kind == Kind::Trades || (self.kind == Kind::Fills && self.order_id.is_some())
    }
}
impl Orderly {
    pub(crate) fn history_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let (kind, symbol, order_id, from, to, limit) = match query {
            Query::OrderByClientId { .. } => {
                reject_local_history(cx, "Orderly client-order lookup is not supported");
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
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit.unwrap_or(100),
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Orders,
                symbol,
                None,
                from_ms,
                to_ms,
                limit.unwrap_or(100),
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                if kind.as_ref().is_some_and(|k| {
                    !matches!(k.to_ascii_uppercase().as_str(), "FUNDING" | "FUNDING_FEE")
                }) {
                    reject_local_history(
                        cx,
                        "Orderly income supports settled funding payments only",
                    );
                    return;
                }
                (
                    Kind::Funding,
                    symbol,
                    None,
                    from_ms,
                    to_ms,
                    limit.unwrap_or(1000),
                )
            }
            Query::PublicTrades { symbol, limit } => (
                Kind::Trades,
                Some(symbol),
                None,
                None,
                None,
                limit.unwrap_or(100),
            ),
        };
        let validation = (|| {
            if limit == 0 || from.zip(to).is_some_and(|(a, b)| a > b) {
                return Err("Orderly invalid history range or limit".to_owned());
            }
            for t in [from, to].into_iter().flatten() {
                time(t)?;
            }
            if let Some(id) = &order_id {
                native_id(id)?;
            }
            if kind != Kind::Trades && !self.signed() {
                return Err("Orderly private history requires credentials".into());
            }
            symbol
                .as_ref()
                .map(|s| {
                    self.native
                        .get(s)
                        .cloned()
                        .ok_or_else(|| "Orderly unknown history symbol".to_owned())
                })
                .transpose()
        })();
        let native = match validation {
            Ok(native) => native,
            Err(e) => {
                reject_local_history(cx, e);
                return;
            }
        };
        if !command_waits(cx) {
            return;
        }
        let root = if kind == Kind::Trades {
            None
        } else {
            match SelectedRead::command_root(
                cx,
                self.read_selection(),
                LocalRefusal {
                    diagnostic: None,
                    error: VenueError::new("Orderly private history requires credentials"),
                },
            ) {
                Ok(root) => Some(root),
                Err(CommandReadError::Entry(CommandEntryError::Revoked)) => {
                    let _ = cx.reject_expired_command(Operation::Read);
                    return;
                }
                Err(_) => return,
            }
        };
        let context = match &root {
            Some(root) => root.context().clone(),
            None => match cx.admit_http_command(READ_POLICY) {
                Ok(context) => context,
                Err(_) => return,
            },
        };
        let completion = match ReadCompletion::from_context(cx, command, context.clone()) {
            Ok(completion) => completion,
            Err(reason) => {
                let _ = cx.fail_context(&context, reason.read_venue_error());
                return;
            }
        };
        self.history_request(
            cx,
            Request {
                owner: Box::new(ReadCursorHistory::new(
                    completion,
                    CursorWalk::starting_at(MAX_PAGES, "regular:1".into()),
                    |_| "Orderly incomplete or repeated history pagination".into(),
                    |rows| serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string())),
                )),
                root,
                leg: None,
                page_rows: Vec::new(),
                kind,
                symbol,
                native,
                order_id,
                from,
                to,
                limit: limit.min(if kind == Kind::Trades {
                    PAGE_SIZE as u32
                } else {
                    MAX_RESULTS
                }) as usize,
                algo: false,
                page: 1,
                page_count: 0,
                page_new: 0,
                stage_count: 0,
                seen: HashSet::new(),
                occurrences: HashMap::new(),
                invalid: None,
            },
        );
    }
    fn history_request(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        if request.kind == Kind::Funding {
            if self.history.delayed.len() >= 100 {
                self.history_failed(
                    cx,
                    request,
                    VenueError::new("funding history queue is full"),
                );
                return;
            }
            let at = self.history.next_funding_at.max(cx.now);
            self.history.next_funding_at = at + FUNDING_SPACING;
            if at > cx.now {
                let timer = cx.set_timer(at - cx.now);
                self.history.delayed.insert(timer, request);
                return;
            }
        }
        self.history_send(cx, request);
    }
    fn history_send(&mut self, cx: &mut Cx<'_, Pending>, mut read: Request) {
        if read.kind == Kind::Funding {
            if let Some(earliest) = self.history.last_funding_start.map(|t| t + FUNDING_SPACING)
                && cx.now < earliest
            {
                let at = earliest.max(self.history.next_funding_at);
                self.history.next_funding_at = at + FUNDING_SPACING;
                let timer = cx.set_timer(at - cx.now);
                self.history.delayed.insert(timer, read);
                return;
            }
            self.history.last_funding_start = Some(cx.now);
        }
        let mut path = match read.kind {
            Kind::Fills if read.order_id.is_some() => {
                format!("/v1/order/{}/trades", read.order_id.as_ref().unwrap())
            }
            Kind::Fills => "/v1/trades".into(),
            Kind::Orders => format!("/v1/{}orders", if read.algo { "algo/" } else { "" }),
            Kind::Funding => "/v1/funding_fee/history".into(),
            Kind::Trades => "/v1/public/market_trades".into(),
        };
        if !read.unpaged() || read.kind == Kind::Trades {
            let mut params = vec![];
            if let Some(native) = &read.native {
                params.push(format!("symbol={native}"));
            }
            if let Some(from) = read.from {
                params.push(format!("start_t={from:.0}"));
            }
            if let Some(to) = read.to {
                params.push(format!("end_t={to:.0}"));
            }
            if read.kind == Kind::Trades {
                params.push(format!("limit={}", read.limit));
            } else {
                params.push(format!("page={}", read.page));
                params.push(format!("size={PAGE_SIZE}"));
            }
            path.push('?');
            path.push_str(&params.join("&"));
        }
        let origin = self.rest();
        let url = format!("{origin}{path}");
        let rows = ArrayAt::Path(&["data", "rows"]);
        let Some(root) = &read.root else {
            let context = read.owner.root().clone();
            match cx.request_managed(
                context,
                None,
                Method::Get,
                url,
                None,
                Pending::History(read),
            ) {
                Ok(id) => cx.stream_response(id, rows),
                Err((Pending::History(read), reason)) => {
                    let _ = read.owner.reject(cx, reason.venue_error());
                }
                Err(_) => unreachable!(),
            }
            return;
        };
        let leg = match root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            // An expired or retired root already has the engine's terminal.
            Err(RequestError::Revoked) => return,
            Err(reason) => return read.reject_owner(cx, reason.read_venue_error()),
        };
        let account = self.options.application_id.clone().unwrap_or_default();
        if let Err((Pending::History(read), reason)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body: None,
                stream: Some(rows),
            },
            |identity| {
                Box::new(crate::sign::Http::new(
                    identity.clone(),
                    &account,
                    Method::Get,
                    origin,
                    &path,
                ))
            },
            |leg| {
                read.leg = Some(leg);
                Pending::History(read)
            },
        ) {
            read.reject_owner(cx, reason.read_venue_error());
        }
    }
    pub(crate) fn history_item(&self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        if !request.valid_delivery(cx) {
            return;
        }
        request.page_count += 1;
        if request.invalid.is_some() {
            return;
        }
        if request.page_count > PAGE_SIZE {
            request.invalid = Some("Orderly oversized history page".into());
            return;
        }
        let result = self.history_map_item(request, item);
        if let Err(e) = result {
            request.invalid = Some(e);
        }
    }
    fn history_map_item(&self, request: &mut Request, item: &str) -> Result<(), String> {
        match request.kind {
            Kind::Fills => {
                let r: ExecutionWire<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                native_id(&r.id.0)?;
                native_id(&r.order_id.0)?;
                request.observe(r.id.0.clone());
                let timestamp = time(r.executed_timestamp.0)?;
                let side = side(&r.side)?;
                if r.executed_price.0 <= 0.0
                    || r.executed_quantity.0 <= 0.0
                    || r.fee_asset.is_empty()
                {
                    return Err("Orderly invalid historical execution".into());
                }
                let Some(symbol) = symbol(&r.symbol) else {
                    return Ok(());
                };
                if !request.accepts(&symbol, timestamp)
                    || request
                        .order_id
                        .as_ref()
                        .is_some_and(|id| id != &r.order_id.0)
                {
                    return Ok(());
                }
                request.append(
                    r.id.0.clone(),
                    Row::Fill(Execution {
                        id: r.id.0,
                        order_id: r.order_id.0,
                        symbol,
                        timestamp: timestamp / 1000.0,
                        price: r.executed_price.0,
                        qty: r.executed_quantity.0,
                        side,
                        commission: r.fee.0,
                        commission_asset: r.fee_asset.into_owned(),
                        realized_pnl: r.realized_pnl.map(|n| n.0),
                        maker: r.is_maker,
                    }),
                );
            }
            Kind::Orders => {
                let r: OrderWire<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                let raw_id = if request.algo {
                    &r.algo_order_id.0
                } else {
                    &r.order_id.0
                };
                native_id(raw_id)?;
                request.observe(raw_id.clone());
                let mut nodes = 0;
                self.history_walk(request, &r, OrderContext::default(), 0, &mut nodes)?;
            }
            Kind::Funding => {
                let r: FundingWire<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                let timestamp = time(r.created_time.0)?;
                let mode = match r.margin_mode.as_ref() {
                    "" | "CROSS" => "CROSS",
                    "ISOLATED" => "ISOLATED",
                    _ => return Err("Orderly invalid funding margin mode".into()),
                };
                let income = match r.payment_type.as_ref() {
                    "Pay" => -r.funding_fee.0.abs(),
                    "Receive" => r.funding_fee.0.abs(),
                    _ => return Err("Orderly invalid funding payment type".into()),
                };
                if r.mark_price.0 <= 0.0 {
                    return Err("Orderly invalid funding mark price".into());
                }
                let key = format!(
                    "{}:{mode}:{timestamp:.0}:{}:{}:{}:{}",
                    r.symbol, r.payment_type, r.funding_fee.0, r.funding_rate.0, r.mark_price.0
                );
                request.observe(format!("{key}:{}", r.status));
                match r.status.as_ref() {
                    "Accrued" => return Ok(()),
                    "Settled" => {}
                    _ => return Err("Orderly invalid funding settlement status".into()),
                }
                let Some(symbol) = symbol(&r.symbol) else {
                    return Ok(());
                };
                if request.accepts(&symbol, timestamp) {
                    request.append(
                        key.clone(),
                        Row::Income(Income {
                            symbol,
                            income_type: "FUNDING_FEE",
                            income,
                            asset: "USDC",
                            timestamp: timestamp / 1000.0,
                            tran_id: format!("orderly-funding:{key}"),
                            category: "funding",
                        }),
                    );
                }
            }
            Kind::Trades => {
                let r: TradeWire<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                let timestamp = time(r.executed_timestamp.0)?;
                let side = side(&r.side)?;
                let price = r.executed_price.0;
                let qty = r.executed_quantity.0;
                if price <= 0.0 || qty <= 0.0 || !(price * qty).is_finite() {
                    return Err("Orderly invalid public trade".into());
                }
                let Some(symbol) = symbol(&r.symbol) else {
                    return Ok(());
                };
                if !request.accepts(&symbol, timestamp) {
                    return Ok(());
                }
                let base = format!("orderly-trade:{symbol}:{timestamp:.0}:{side}:{price}:{qty}");
                let occurrence = request.occurrences.entry(base.clone()).or_default();
                let key = format!("{base}:{occurrence}");
                *occurrence += 1;
                request.append(
                    key.clone(),
                    Row::Trade(Trade {
                        id: key,
                        symbol,
                        price,
                        qty,
                        quote_qty: price * qty,
                        timestamp: timestamp / 1000.0,
                        side,
                    }),
                );
            }
        }
        Ok(())
    }
    fn history_walk<'a>(
        &self,
        request: &mut Request,
        row: &'a OrderWire<'_>,
        parent: OrderContext<'a>,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<(), String> {
        *nodes += 1;
        if depth > 8 || *nodes > 1000 {
            return Err("Orderly historical algo tree exceeds depth or child budget".into());
        }
        let context = OrderContext {
            symbol: if row.symbol.is_empty() {
                parent.symbol
            } else {
                &row.symbol
            },
            side: if row.side.is_empty() {
                parent.side
            } else {
                &row.side
            },
            status: if request.algo && !row.algo_status.is_empty() {
                &row.algo_status
            } else if !row.status.is_empty() {
                &row.status
            } else if !row.root_algo_status.is_empty() {
                &row.root_algo_status
            } else {
                parent.status
            },
            created: row.created_time.or(parent.created),
            updated: row.updated_time.or(parent.updated),
            reduce_only: row
                .reduce_only
                .or_else(|| (row.algo_type == "POSITIONAL_TP_SL").then_some(true))
                .or(parent.reduce_only),
        };
        let container = matches!(
            row.algo_type.as_ref(),
            "POSITIONAL_TP_SL" | "TP_SL" | "BRACKET"
        );
        // BRACKET's entry is itself a native LIMIT/MARKET order. Positional
        // wrappers carry exits only; retain their nearest metadata for leaves.
        let entry = row.algo_type == "BRACKET"
            && matches!(
                row.kind.as_ref(),
                "LIMIT" | "MARKET" | "POST_ONLY" | "IOC" | "FOK"
            )
            && row.quantity.is_some();
        if !container || entry {
            self.history_order(request, row, context)?;
        }
        for child in &row.child_orders {
            self.history_walk(request, child, context, depth + 1, nodes)?;
        }
        Ok(())
    }
    fn history_order(
        &self,
        read: &mut Request,
        row: &OrderWire<'_>,
        context: OrderContext<'_>,
    ) -> Result<(), String> {
        let id = if read.algo {
            &row.algo_order_id.0
        } else {
            &row.order_id.0
        };
        native_id(id)?;
        let Some(symbol) = symbol(context.symbol) else {
            return Ok(());
        };
        let timestamp = time(
            context
                .created
                .ok_or("Orderly history order missing creation time")?
                .0,
        )?;
        let updated = context.updated.map(|n| time(n.0)).transpose()?;
        let status = crate::account::status(context.status)?;
        if status == OrderStatus::Open || !read.accepts(&symbol, timestamp) {
            return Ok(());
        }
        let protective = matches!(
            row.algo_type.as_ref(),
            "STOP_LOSS" | "TAKE_PROFIT" | "TRAILING_STOP"
        ) || row.kind == "CLOSE_POSITION";
        let reduce_only = context.reduce_only.unwrap_or(protective);
        let kind = match if read.algo && row.algo_type != "BRACKET" {
            row.algo_type.as_ref()
        } else {
            row.kind.as_ref()
        } {
            "LIMIT" | "POST_ONLY" | "IOC" | "FOK" => OrderType::Limit,
            "MARKET" => OrderType::Market,
            "STOP" if reduce_only => OrderType::StopLoss,
            "STOP" => OrderType::Conditional,
            "STOP_LOSS" => OrderType::StopLoss,
            "TAKE_PROFIT" => OrderType::TakeProfit,
            "TRAILING_STOP" => OrderType::TrailingStopLoss,
            _ => return Err("Orderly unsupported historical order type".into()),
        };
        let amount = row
            .quantity
            .ok_or("Orderly history order missing quantity")?
            .0;
        let filled = row
            .total_executed_quantity
            .or(row.executed)
            .map_or(0.0, |n| n.0);
        if amount < 0.0 || filled < 0.0 || (!read.algo && filled > amount) {
            return Err("Orderly invalid history order quantity".into());
        }
        let price = row.trigger_price.or(row.price).map(|n| n.0);
        let average_price = row.average_executed_price.map(|n| n.0);
        if price.is_some_and(|n| n < 0.0) || average_price.is_some_and(|n| n < 0.0) {
            return Err("Orderly invalid historical order price".into());
        }
        let side = side(context.side)?;
        read.append(
            format!("{}:{id}", read.algo),
            Row::Order(HistoricalOrder {
                id: id.clone(),
                symbol,
                side,
                kind,
                status,
                price: price.filter(|n| *n > 0.0),
                average_price: average_price.filter(|n| *n > 0.0),
                amount,
                filled,
                reduce_only,
                created_time: timestamp,
                updated_time: updated,
            }),
        );
        Ok(())
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !request.valid_delivery(cx) {
            // A stale or mismatched delivery still closes its own leg.
            request.release(cx);
            return;
        }
        let result = checked(response).and_then(|r| {
            let data = r.data.ok_or("Orderly history response missing data")?;
            serde_json::from_str::<Page>(data.get()).map_err(|e| e.to_string())
        });
        if let Some(error) = request.invalid.take() {
            self.history_failed(cx, request, VenueError::new(error));
            return;
        }
        let page = match result {
            Ok(page) => page,
            Err(e) => {
                self.history_failed(cx, request, VenueError::new(e));
                return;
            }
        };
        match request.stage_page(cx) {
            Ok(()) => {}
            Err(RequestError::Revoked) => return request.release(cx),
            Err(reason) => return request.reject_owner(cx, reason.read_venue_error()),
        }
        if let Some(meta) = &page.meta
            && (meta.current_page.is_some_and(|p| p != request.page)
                || meta
                    .records_per_page
                    .is_some_and(|n| n == 0 || n > PAGE_SIZE)
                || meta.total < request.seen.len() as u64)
        {
            self.history_rejected(cx, request, "Orderly inconsistent history pagination");
            return;
        }
        let more = if request.unpaged() {
            false
        } else {
            page.meta.map_or(request.page_count == PAGE_SIZE, |m| {
                (request.seen.len() as u64) < m.total
            })
        };
        let continue_stage = more && request.stage_count < request.limit;
        if continue_stage && (request.page_count == 0 || request.page_new == 0) {
            self.history_rejected(
                cx,
                request,
                "Orderly incomplete or repeated history pagination",
            );
            return;
        }
        let next_stage = !continue_stage && request.kind == Kind::Orders && !request.algo;
        // One walk covers both native order endpoints: restarting here would
        // silently double the existing fifty-page command budget.
        let next = if continue_stage {
            let stage = if request.algo { "algo" } else { "regular" };
            let page = request.page + 1;
            Some(format!("{stage}:{page}"))
        } else if next_stage {
            Some("algo:1".into())
        } else {
            None
        };
        let step = match request.advance(
            cx,
            CursorPage {
                want_more: next.is_some(),
                next,
                rows: request.page_count,
                fresh: request.page_new,
            },
        ) {
            Ok(step) => step,
            // A retired root already has the engine's terminal; anything else is strict.
            Err(RequestError::Revoked) => return request.release(cx),
            Err(reason) => return request.reject_owner(cx, reason.read_venue_error()),
        };
        match step {
            CursorStep::More(_) => {
                request.release(cx);
                if next_stage {
                    request.algo = true;
                    request.page = 1;
                    request.stage_count = 0;
                    request.seen.clear();
                } else {
                    request.page += 1;
                }
                request.page_count = 0;
                request.page_new = 0;
                self.history_request(cx, request);
            }
            CursorStep::Done => self.history_finish(cx, request, Ok(())),
            CursorStep::Stalled(_) => self.history_rejected(
                cx,
                request,
                if next_stage {
                    "Orderly history pagination budget exhausted"
                } else {
                    "Orderly incomplete or repeated history pagination"
                },
            ),
        }
    }
    fn history_finish(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        result: Result<(), VenueError>,
    ) {
        request.release(cx);
        let limit = request.limit;
        let _ = request.owner.finish_with(cx, result, |mut rows| {
            rows.sort_by(|a, b| {
                b.time()
                    .total_cmp(&a.time())
                    .then_with(|| b.id().cmp(a.id()))
            });
            rows.truncate(limit);
            serde_json::to_string(&rows).map_err(|e| VenueError::new(e.to_string()))
        });
    }
    fn history_rejected(&self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        request.reject_owner(
            cx,
            VenueError::new(format!("Orderly history read failed: {error}")),
        );
    }
    /// A failed page keeps accepted rows with one INCOMPLETE; a typed kind is kept.
    pub(crate) fn history_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
    ) {
        error.message = format!("Orderly history read failed: {}", error.message);
        self.history_finish(cx, request, Err(error));
    }
    /// A signing refusal is strict even after accepted rows, as native headers were.
    pub(crate) fn history_refused(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        request.reject_owner(cx, error);
    }
    pub(crate) fn history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(request) = self.history.delayed.remove(&timer) {
            self.history_send(cx, request);
            true
        } else {
            false
        }
    }
}
