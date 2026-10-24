//! Rare reads keep typed output rows in their request continuation. History
//! without server-side time filters walks bounded offset pages, newest first.
use crate::account::{kind, side, status};
use crate::sign::Params;
use crate::wire::*;
use crate::{Backpack, Pending};
use safe_cex_engine::Level;
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::history::{ReadCompletion, ReadCursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, OffsetStep, OffsetWalk, Stall};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, CommandWait, Cx, ErrorKind, Method, Query, Response, VenueError};
use serde::Serialize;

/// Requests per read before paging that still reports more is incomplete.
const MAX_PAGES: u32 = 50;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fills,
    Orders,
    Funding,
    Trades,
}
pub struct Request {
    owner: Box<ReadCursorHistory<Row>>,
    root: Root,
    leg: Option<Leg>,
    kind: Kind,
    symbol: Option<String>,
    order_id: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    page_size: usize,
    walk: OffsetWalk,
    page_count: usize,
    oldest: f64,
    page_fresh: usize,
    page_hash: u64,
    skipped_fills: Vec<String>,
    page_skipped_fills: Vec<String>,
    invalid: bool,
}
enum Root {
    Private(ReadRoot),
    Public(Context),
}
impl Root {
    fn context(&self) -> &Context {
        match self {
            Self::Private(root) => root.context(),
            Self::Public(root) => root,
        }
    }
}
enum Leg {
    Private(ReadLeg),
    Public(Context),
}
impl Leg {
    fn context(&self) -> &Context {
        match self {
            Self::Private(leg) => leg.context(),
            Self::Public(leg) => leg,
        }
    }
}
#[derive(Serialize)]
#[serde(untagged)]
enum Row {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
    Trade(Trade),
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
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: &'static str,
    income: f64,
    asset: String,
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
fn side_name(text: &str) -> &'static str {
    if text == "Bid" { "buy" } else { "sell" }
}
impl Request {
    fn accepts(&self, symbol: &str, time: f64) -> bool {
        self.symbol.as_ref().is_none_or(|s| s == symbol)
            && self.from.is_none_or(|t| time >= t)
            && self.to.is_none_or(|t| {
                if self.kind == Kind::Fills {
                    time < t
                } else {
                    time <= t
                }
            })
    }
    fn delivery_current(&self, cx: &Cx<'_, Pending>) -> bool {
        let Some(leg) = &self.leg else {
            return false;
        };
        let selected = match (&self.root, leg) {
            (Root::Private(root), Leg::Private(leg)) => root.validate_delivery(cx, leg),
            (Root::Public(root), Leg::Public(leg)) => cx.validate_read_http_delivery(root, leg),
            _ => return false,
        };
        selected.is_ok() && self.owner.validate_delivery(cx, leg.context()).is_ok()
    }
    fn retire_leg(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(leg) = self.leg.take() {
            let _ = cx.finish_http_context(leg.context());
        }
    }
    fn append(&mut self, cx: &Cx<'_, Pending>, id: String, time: f64, row: Row) {
        let Some(leg) = &self.leg else {
            return;
        };
        let stage = self.owner.accepted_len() + self.owner.staged_len() < self.limit;
        let Ok(mut page) = self.owner.admit(cx, leg.context()) else {
            return;
        };
        self.oldest = self.oldest.min(time);
        if page.fresh(id) {
            self.page_fresh += 1;
            if stage {
                page.stage(row);
            }
        }
    }
}
fn encode_rows(rows: &[Row]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}
fn stalled_page(_: Stall) -> String {
    "Backpack history is incomplete: paging made no progress".into()
}
impl Backpack {
    fn reject_history_query(cx: &mut Cx<'_, Pending>, message: &str) {
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
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        let _ = cx.fail_context(&context, message);
    }

    pub(crate) fn history_command(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let (kind, symbol, order_id, from, to, limit) = match query {
            Query::OrderByClientId { .. } => {
                Self::reject_history_query(cx, "Backpack client-order-ID lookup is unsupported");
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
                    !matches!(k.to_ascii_uppercase().as_str(), "FUNDING_FEE" | "FUNDING")
                }) {
                    Self::reject_history_query(
                        cx,
                        "Backpack income query supports funding payments only",
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
                limit.unwrap_or(1000),
            ),
        };
        if from
            .into_iter()
            .chain(to)
            .any(|t| !t.is_finite() || t < 0.0)
            || from.zip(to).is_some_and(|(a, b)| a > b)
        {
            Self::reject_history_query(cx, "Backpack invalid history time range");
            return;
        }
        if symbol
            .as_ref()
            .is_some_and(|s| cx.store.market(s).is_none())
        {
            Self::reject_history_query(cx, "Backpack history market not found");
            return;
        }
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
        let root = if kind == Kind::Trades {
            let Ok(root) = cx.admit_caller_read_root(None, None) else {
                return;
            };
            Root::Public(root)
        } else {
            if let Some(Err(error)) = self.options.read_credentials_status {
                Self::reject_history_query(cx, error.message());
                return;
            }
            let primary = self
                .account
                .credentials
                .as_ref()
                .and(self.options.identity.clone());
            let slots = Slots::new(
                primary,
                self.options.read_identity.clone(),
                SlotPolicy::default(),
            );
            if self.options.read_credentials_status.is_some()
                && self.options.read_identity.is_none()
            {
                Self::reject_history_query(
                    cx,
                    "Backpack managed read signing identity unavailable",
                );
                return;
            }
            let Ok(root) = SelectedRead::command_root(
                cx,
                SelectedRead::select(&slots, ReadRole::Read { scope: None }),
                LocalRefusal {
                    diagnostic: None,
                    error: "Backpack credentials unavailable".into(),
                },
            ) else {
                return;
            };
            Root::Private(root)
        };
        let completion = match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(root.context(), error.read_venue_error());
                return;
            }
        };
        let limit = limit.clamp(1, 1000) as usize;
        let page_size =
            if matches!(kind, Kind::Orders | Kind::Funding) && (from.is_some() || to.is_some()) {
                1000
            } else {
                limit
            };
        self.history_request(
            cx,
            Request {
                owner: Box::new(ReadCursorHistory::new(
                    completion,
                    CursorWalk::new(MAX_PAGES),
                    stalled_page,
                    encode_rows,
                )),
                root,
                leg: None,
                kind,
                symbol,
                order_id,
                from,
                to,
                limit,
                page_size,
                walk: OffsetWalk::new(MAX_PAGES),
                page_count: 0,
                oldest: f64::INFINITY,
                page_fresh: 0,
                page_hash: 14695981039346656037,
                skipped_fills: vec![],
                page_skipped_fills: vec![],
                invalid: false,
            },
        );
    }
    fn history_request(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let mut params: Params = [("limit".into(), request.page_size.to_string())].into();
        if let Some(s) = &request.symbol {
            params.insert("symbol".into(), s.clone());
        }
        if let Some(id) = &request.order_id {
            params.insert("orderId".into(), id.clone());
        }
        let path = match request.kind {
            Kind::Fills => {
                if let Some(t) = request.from {
                    params.insert("from".into(), format!("{:.0}", t.floor()));
                }
                if let Some(t) = request.to {
                    params.insert("to".into(), format!("{:.0}", t.ceil()));
                }
                "/wapi/v1/history/fills"
            }
            Kind::Orders => "/wapi/v1/history/orders",
            Kind::Funding => "/wapi/v1/history/funding",
            Kind::Trades => "/api/v1/trades",
        };
        if request.kind != Kind::Trades {
            params.insert("sortDirection".into(), "Desc".into());
            if request.walk.offset() > 0 {
                params.insert("offset".into(), request.walk.offset().to_string());
            }
        }
        let leg = match &request.root {
            Root::Private(root) => root
                .leg(
                    cx,
                    ReadLegPolicy {
                        timeout_ms: 20_000,
                        pacing: None,
                    },
                )
                .map(Leg::Private),
            Root::Public(root) => cx
                .http_leg_context(
                    root,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .map(Leg::Public),
        };
        let leg = match leg {
            Ok(leg) => leg,
            Err(error) => {
                Self::history_finish(cx, request, Err(error.read_venue_error()));
                return;
            }
        };
        let query = crate::sign::query(&params);
        let url = format!("https://api.backpack.exchange{path}?{query}");
        let queued = match leg {
            Leg::Private(leg) => {
                let declaration = crate::sign::Request {
                    method: Method::Get,
                    path: path.into(),
                    rows: vec![params],
                    body: None,
                    window: self
                        .account
                        .credentials
                        .as_ref()
                        .map_or(60000, |primary| primary.window),
                };
                leg.queue(
                    cx,
                    ReadRequest {
                        method: Method::Get,
                        url,
                        body: None,
                        stream: Some(ArrayAt::Root),
                    },
                    |identity| Box::new(crate::sign::Http::new(identity.clone(), declaration)),
                    |leg| {
                        let mut request = request;
                        request.leg = Some(Leg::Private(leg));
                        Pending::History(Box::new(request))
                    },
                )
            }
            Leg::Public(context) => {
                let mut request = request;
                request.leg = Some(Leg::Public(context.clone()));
                let queued = cx.request_managed_with_headers(
                    context,
                    Method::Get,
                    url,
                    &[("X-Broker-Id", "1200")],
                    None,
                    Pending::History(Box::new(request)),
                );
                if let Ok(id) = queued {
                    cx.stream_response(id, ArrayAt::Root);
                }
                queued
            }
        };
        if let Err((pending, error)) = queued {
            if let Pending::History(request) = pending {
                Self::history_finish(cx, *request, Err(error.read_venue_error()));
            } else {
                unreachable!("history queue retains its own owner");
            }
        }
    }
    pub(crate) fn history_item(&self, cx: &mut Cx<'_, Pending>, r: &mut Request, item: &str) {
        if !r.delivery_current(cx) {
            return;
        }
        r.page_count += 1;
        for byte in item.bytes() {
            r.page_hash = (r.page_hash ^ u64::from(byte)).wrapping_mul(1099511628211);
        }
        let result = (|| {
            match r.kind {
                Kind::Fills => {
                    let d: FillRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let time = timestamp(&d.timestamp)?;
                    r.oldest = r.oldest.min(time);
                    if !r.accepts(&d.symbol, time)
                        || r.order_id.as_ref().is_some_and(|id| id != &d.order_id)
                        || cx.store.market(&d.symbol).is_none()
                    {
                        return Ok(());
                    }
                    if !matches!(d.side.as_ref(), "Bid" | "Ask")
                        || d.price <= 0.0
                        || d.quantity <= 0.0
                    {
                        return Err("Backpack invalid fill".into());
                    }
                    let key = format!("{}:{}", d.order_id, d.trade_id);
                    r.append(
                        cx,
                        key,
                        time,
                        Row::Fill(Execution {
                            id: d.trade_id.to_string(),
                            timestamp: (time / 1000.0).floor(),
                            symbol: d.symbol.into_owned(),
                            order_id: d.order_id.into_owned(),
                            price: d.price,
                            qty: d.quantity,
                            side: side_name(&d.side),
                            commission: d.fee.abs(),
                            commission_asset: d.fee_symbol.into_owned(),
                            maker: d.is_maker,
                        }),
                    );
                }
                Kind::Orders => {
                    let d: OrderRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let quantity = d.base_quantity()?;
                    if !matches!(d.side.as_ref(), "Bid" | "Ask") {
                        return Err("Backpack invalid order side".into());
                    }
                    let time = d.created_at;
                    r.oldest = r.oldest.min(time);
                    if !r.accepts(&d.symbol, time) || cx.store.market(&d.symbol).is_none() {
                        return Ok(());
                    }
                    let order_kind = kind(
                        cx.store,
                        &d.symbol,
                        side(&d.side),
                        &d.order_type,
                        d.trigger_price.as_ref().map(|n| n.0),
                    );
                    let price = d.trigger_price.map_or(d.price, |n| n.0);
                    let amount = d.trigger_quantity.map_or(quantity, |n| n.0);
                    let average = d
                        .executed_quote_quantity
                        .map(|q| q.0)
                        .filter(|_| d.executed_quantity > 0.0)
                        .map(|q| q / d.executed_quantity);
                    let id = d.id.into_owned();
                    r.append(
                        cx,
                        id.clone(),
                        time,
                        Row::Order(HistoricalOrder {
                            id,
                            symbol: d.symbol.into_owned(),
                            side: side_name(&d.side),
                            kind: order_kind,
                            status: status(&d.status),
                            price: (price > 0.0).then_some(price),
                            average_price: average,
                            amount,
                            filled: d.executed_quantity,
                            reduce_only: d.reduce_only.unwrap_or(false),
                            created_time: time,
                        }),
                    );
                }
                Kind::Funding => {
                    let d: FundingRow<'_> =
                        serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let time = timestamp(&d.interval_end_timestamp)?;
                    r.oldest = r.oldest.min(time);
                    if !r.accepts(&d.symbol, time) {
                        return Ok(());
                    }
                    let Some(market) = cx.store.market(&d.symbol) else {
                        return Ok(());
                    };
                    let key = format!("{}:{time:.0}", d.symbol);
                    let symbol = d.symbol.into_owned();
                    r.append(
                        cx,
                        key.clone(),
                        time,
                        Row::Income(Income {
                            symbol,
                            income_type: "FUNDING_FEE",
                            income: d.quantity,
                            asset: market.quote.clone(),
                            timestamp: time,
                            tran_id: key,
                            category: "funding",
                        }),
                    );
                }
                Kind::Trades => {
                    let d: PublicTrade = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    r.append(
                        cx,
                        d.id.to_string(),
                        d.timestamp,
                        Row::Trade(Trade {
                            id: d.id.to_string(),
                            symbol: r.symbol.clone().unwrap(),
                            price: d.price,
                            qty: d.quantity,
                            quote_qty: d.quote_quantity,
                            timestamp: d.timestamp,
                            side: if d.is_buyer_maker { "sell" } else { "buy" },
                        }),
                    );
                }
            }
            Ok::<(), String>(())
        })();
        match result {
            // One undecodable execution must not hide the rest of the history.
            Err(_) if r.kind == Kind::Fills => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Id {
                    trade_id: u64,
                }
                r.page_skipped_fills.push(
                    serde_json::from_str::<Id>(item)
                        .map_or_else(|_| "unknown".into(), |d| d.trade_id.to_string()),
                );
            }
            Err(e) => {
                r.invalid = true;
                let _ = cx.command_error_context(r.root.context(), e);
            }
            Ok(()) => {}
        }
    }
    pub(crate) fn history_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        response: Response<'_>,
    ) {
        if !r.delivery_current(cx) {
            r.retire_leg(cx);
            let _ = cx.finish_http_context(r.root.context());
            return;
        }
        if !(200..300).contains(&response.status) || r.invalid {
            self.history_failed(
                cx,
                r,
                VenueError::new(format!(
                    "Backpack history read failed: HTTP {}",
                    response.status
                )),
            );
            return;
        }
        let more = r.kind != Kind::Trades
            && r.page_count >= r.page_size
            && r.owner.accepted_len() + r.owner.staged_len() < r.limit
            && !r.from.is_some_and(|from| r.oldest < from);
        let gap = (!r.page_skipped_fills.is_empty())
            .then(|| "Backpack history is incomplete: malformed fill rows were skipped".into());
        // Native page identity, not an increasing offset, proves progress.
        let step = {
            let Some(leg) = &r.leg else {
                return;
            };
            let Ok(mut page) = r.owner.admit(cx, leg.context()) else {
                r.retire_leg(cx);
                return;
            };
            page.advance_with_coverage(
                CursorPage {
                    next: more.then(|| r.page_hash.to_string()),
                    rows: r.page_count,
                    fresh: r.page_fresh,
                    want_more: more,
                },
                gap,
            )
        };
        r.skipped_fills.append(&mut r.page_skipped_fills);
        r.retire_leg(cx);
        if matches!(step, CursorStep::More(_)) {
            // Only the owner can authorize continuation. This existing helper
            // supplies raw-row offset arithmetic after that decision; at most
            // 49 calls occur before the owner's 50-page cap.
            if !matches!(
                r.walk.advance(r.page_count, r.page_count, true),
                OffsetStep::More(_)
            ) {
                let _ = r.owner.reject(
                    cx,
                    VenueError::new("Backpack history offset invariant failed")
                        .kind(ErrorKind::Internal),
                );
                return;
            }
            r.page_count = 0;
            r.page_fresh = 0;
            r.page_hash = 14695981039346656037;
            r.oldest = f64::INFINITY;
            self.history_request(cx, r);
        } else {
            Self::history_finish(cx, r, Ok(()));
        }
    }
    fn history_finish(cx: &mut Cx<'_, Pending>, mut r: Request, result: Result<(), VenueError>) {
        r.retire_leg(cx);
        if cx
            .validate_read_completion_context(r.root.context())
            .is_err()
        {
            let _ = cx.finish_http_context(r.root.context());
            return;
        }
        if (result.is_ok() || r.owner.accepted_len() > 0) && !r.skipped_fills.is_empty() {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                format!(
                    "Skipped {} malformed Backpack fill rows: {}",
                    r.skipped_fills.len(),
                    r.skipped_fills.join(", ")
                ),
            );
        }
        let _ = r.owner.finish(cx, result);
    }
    pub(crate) fn history_failed(&self, cx: &mut Cx<'_, Pending>, r: Request, error: VenueError) {
        Self::history_finish(cx, r, Err(error));
    }
}
