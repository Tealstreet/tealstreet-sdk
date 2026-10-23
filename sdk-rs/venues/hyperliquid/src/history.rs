//! Rare reads return JSON; no history rows enter the live store.
use crate::{Hyperliquid, Pending, account::side, wire::*};
use safe_cex_engine::history::{Completion, CursorHistory, HistoryRows};
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Outcome, Query, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fills,
    Orders,
    IncomeFills,
    IncomeFunding,
    Trades,
    Lookup,
}
#[derive(Serialize)]
#[serde(untagged)]
enum HistoryRow {
    Execution(Execution),
    Order(OrderState),
    Income(Income),
    Trade(Trade),
}
pub struct Request {
    history: Box<CursorHistory<HistoryRow>>,
    kind: Kind,
    query: Query,
    from: f64,
    to: f64,
    cursor: f64,
    newest: f64,
    added: usize,
    raw_count: u32,
    seen: HashSet<String>,
    valid: bool,
}
impl Request {
    pub(crate) fn streamed(&self) -> bool {
        self.kind != Kind::Lookup
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    #[serde(skip)]
    time_ms: f64,
    #[serde(skip)]
    tid: u64,
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: OrderSide,
    commission: f64,
    commission_asset: String,
    realized_pnl: f64,
    maker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    venue_reported_revenue: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    venue_reported_revenue_asset: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: OrderSide,
    #[serde(rename = "type")]
    kind: OrderType,
    status: OrderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_time: Option<f64>,
}
#[derive(Serialize)]
struct OrderState {
    #[serde(flatten)]
    order: HistoricalOrder,
    #[serde(skip)]
    time: f64,
    #[serde(skip)]
    rank: u8,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_id: Option<String>,
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
    side: OrderSide,
}
#[derive(Deserialize)]
struct Funding<'a> {
    time: f64,
    #[serde(default, borrow)]
    hash: &'a str,
    #[serde(borrow)]
    delta: FundingDelta<'a>,
}
#[derive(Deserialize)]
struct FundingDelta<'a> {
    #[serde(borrow)]
    coin: &'a str,
    #[serde(deserialize_with = "number")]
    usdc: f64,
}
#[derive(Deserialize)]
struct PublicTrade<'a> {
    #[serde(borrow)]
    coin: &'a str,
    #[serde(borrow)]
    side: &'a str,
    #[serde(deserialize_with = "number")]
    px: f64,
    #[serde(deserialize_with = "number")]
    sz: f64,
    time: f64,
    tid: u64,
}
impl Hyperliquid {
    pub(crate) fn start_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        if let Query::OrderByClientId {
            symbol,
            client_order_id,
        } = &query
        {
            let cached = cx.store.orders().iter().position(|o| {
                o.symbol == *symbol && o.client_order_id.as_deref() == Some(client_order_id)
            });
            if cached.is_some()
                || !crate::account::valid_cloid(client_order_id)
                || self.user().is_none()
            {
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                let result = if let Some(index) = cached {
                    #[derive(Serialize)]
                    struct Found<'a> {
                        found: bool,
                        order: &'a safe_cex_engine::model::Order,
                    }
                    serde_json::to_string(&Found {
                        found: true,
                        order: &cx.store.orders()[index],
                    })
                    .unwrap()
                } else {
                    "{\"found\":false,\"definitive\":false}".into()
                };
                let _ = cx.done_context(&context, result);
                return;
            }
        }
        if self.spot {
            let mut completion = Completion::new(command);
            match query {
                Query::OrderByClientId { .. } => {
                    completion.finish(cx, Ok("{\"found\":false,\"definitive\":false}".into()));
                }
                Query::PublicTrades { .. } => {
                    completion.finish(
                        cx,
                        Err(VenueError::new(
                            "Hyperliquid spot public trades query is not supported",
                        )
                        .kind(ErrorKind::Unsupported)
                        .outcome(Outcome::NotSent)),
                    );
                }
                Query::Fills { .. } | Query::OrderHistory { .. } | Query::Income { .. } => {
                    completion.incomplete("history is INCOMPLETE: Hyperliquid spot adapter has no native history read");
                    HistoryRows::<()>::new(completion, |rows| {
                        serde_json::to_string(rows)
                            .map_err(|error| VenueError::new(error.to_string()))
                    })
                    .finish(cx, Ok(()));
                }
            }
            return;
        }
        let (kind, from, to) = match &query {
            Query::OrderByClientId { .. } => (Kind::Lookup, None, None),
            Query::Fills { from_ms, to_ms, .. } => (Kind::Fills, *from_ms, *to_ms),
            Query::OrderHistory { from_ms, to_ms, .. } => (Kind::Orders, *from_ms, *to_ms),
            Query::Income {
                kind,
                from_ms,
                to_ms,
                ..
            } => (
                if kind.as_deref() == Some("FUNDING_FEE") {
                    Kind::IncomeFunding
                } else {
                    Kind::IncomeFills
                },
                *from_ms,
                *to_ms,
            ),
            Query::PublicTrades { .. } => (Kind::Trades, None, None),
        };
        if kind != Kind::Trades && (self.options.public_data_only || self.user().is_none()) {
            cx.fail(command, "Hyperliquid account identity is missing");
            return;
        }
        let to = to.unwrap_or(cx.now).floor();
        let from = from
            .unwrap_or(if matches!(kind, Kind::Orders | Kind::Lookup) {
                0.0
            } else {
                (to - 30.0 * 86_400_000.0).max(0.0)
            })
            .floor();
        if !from.is_finite()
            || !to.is_finite()
            || from < 0.0
            || to > 9_007_199_254_740_991.0
            || from > to
        {
            cx.fail(command, "Invalid history time range");
            return;
        }
        let request = Request {
            history: Box::new(CursorHistory::new(
                Completion::new(command),
                CursorWalk::new(200),
                stall_message,
                encode_rows,
            )),
            kind,
            query,
            from,
            to,
            cursor: from,
            newest: from,
            added: 0,
            raw_count: 0,
            seen: HashSet::new(),
            valid: true,
        };
        self.history_page(cx, request);
    }
    fn history_page(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Read<'a> {
            r#type: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            user: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            coin: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            start_time: Option<u64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            end_time: Option<u64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            aggregate_by_time: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            oid: Option<&'a str>,
        }
        let kind = match request.kind {
            Kind::Fills | Kind::IncomeFills => "userFillsByTime",
            Kind::Orders => "historicalOrders",
            Kind::IncomeFunding => "userFunding",
            Kind::Trades => "recentTrades",
            Kind::Lookup => "orderStatus",
        };
        let coin = if let Query::PublicTrades { symbol, .. } = &request.query {
            cx.store.market(symbol).map(|m| m.id.as_str())
        } else {
            None
        };
        if request.kind == Kind::Trades && coin.is_none() {
            cx.fail(request.history.command(), "Market not found");
            return;
        }
        let ranged = matches!(
            request.kind,
            Kind::Fills | Kind::IncomeFills | Kind::IncomeFunding
        );
        let body = serde_json::to_string(&Read {
            r#type: kind,
            user: (request.kind != Kind::Trades)
                .then(|| self.user())
                .flatten(),
            coin,
            start_time: ranged.then_some(request.cursor as u64),
            end_time: ranged.then_some(request.to as u64),
            aggregate_by_time: matches!(request.kind, Kind::Fills | Kind::IncomeFills)
                .then_some(false),
            oid: if let Query::OrderByClientId {
                client_order_id, ..
            } = &request.query
            {
                Some(client_order_id)
            } else {
                None
            },
        })
        .unwrap();
        self.history_read(cx, body, request);
    }
    pub(crate) fn history_item(&self, request: &mut Request, item: &str) {
        request.raw_count += 1;
        match request.kind {
            Kind::Lookup => {}
            Kind::Fills | Kind::IncomeFills => {
                let Ok(fill) = serde_json::from_str::<FillRow>(item) else {
                    request.valid = false;
                    return;
                };
                // Historical queries keep their established zero defaults and
                // gross realized-PnL convention, independent of live metadata.
                let fee = fill.fee.unwrap_or(0.);
                let closed_pnl = fill.closed_pnl.unwrap_or(0.);
                if fill.time < request.from || fill.time > request.to {
                    return;
                }
                if !request.seen.insert(fill.tid.to_string()) {
                    return;
                }
                request.added += 1;
                request.newest = request.newest.max(fill.time);
                let symbol = self.symbol(&fill.coin).unwrap_or(&fill.coin).to_owned();
                if !symbol_matches(&request.query, &symbol) {
                    return;
                }
                if request.kind == Kind::Fills {
                    if let Query::Fills {
                        order_id: Some(id), ..
                    } = &request.query
                        && *id != fill.oid.to_string()
                    {
                        return;
                    }
                    if fill.px <= 0.0 || fill.sz <= 0.0 || !["A", "B"].contains(&fill.side.as_ref())
                    {
                        return;
                    }
                    let asset = if fill.fee_token.is_empty() {
                        "USDC"
                    } else {
                        &fill.fee_token
                    };
                    request.history.stage(HistoryRow::Execution(Execution {
                        time_ms: fill.time,
                        tid: fill.tid,
                        id: fill.tid.to_string(),
                        timestamp: (fill.time / 1000.0).floor(),
                        symbol,
                        order_id: fill.oid.to_string(),
                        price: fill.px,
                        qty: fill.sz,
                        side: side(&fill.side),
                        commission: fee,
                        commission_asset: asset.into(),
                        realized_pnl: closed_pnl,
                        maker: !fill.crossed,
                        venue_reported_revenue: fill.builder_fee.map(f64::abs),
                        venue_reported_revenue_asset: fill.builder_fee.map(|_| asset.to_owned()),
                    }));
                } else {
                    let asset = if fill.fee_token.is_empty() {
                        "USDC"
                    } else {
                        &fill.fee_token
                    };
                    for (kind, value, category, suffix) in [
                        ("REALIZED_PNL", closed_pnl, "realized_pnl", "pnl"),
                        (
                            if fee < 0.0 { "REBATE" } else { "COMMISSION" },
                            -fee,
                            if fee < 0.0 { "rebate" } else { "commission" },
                            "fee",
                        ),
                    ] {
                        if value == 0.0 || !income_matches(&request.query, kind) {
                            continue;
                        }
                        request.history.stage(HistoryRow::Income(Income {
                            symbol: symbol.clone(),
                            income_type: kind,
                            income: value,
                            asset: asset.into(),
                            timestamp: fill.time,
                            tran_id: format!("{}:{suffix}", fill.tid),
                            trade_id: Some(fill.tid.to_string()),
                            category,
                        }));
                    }
                }
            }
            Kind::IncomeFunding => {
                let Ok(row) = serde_json::from_str::<Funding>(item) else {
                    request.valid = false;
                    return;
                };
                if row.time < request.from || row.time > request.to {
                    return;
                }
                let id = format!("{}:{}:{}", row.hash, row.delta.coin, row.time);
                if !request.seen.insert(id) {
                    return;
                }
                request.added += 1;
                request.newest = request.newest.max(row.time);
                let symbol = self.symbol(row.delta.coin).unwrap_or(row.delta.coin);
                if row.delta.usdc != 0.0 && symbol_matches(&request.query, symbol) {
                    request.history.stage(HistoryRow::Income(Income {
                        symbol: symbol.into(),
                        income_type: "FUNDING_FEE",
                        income: row.delta.usdc,
                        asset: "USDC".into(),
                        timestamp: row.time,
                        tran_id: format!(
                            "{}:{}:funding",
                            if row.hash.is_empty() {
                                row.time.to_string()
                            } else {
                                row.hash.into()
                            },
                            row.delta.coin
                        ),
                        trade_id: None,
                        category: "funding",
                    }));
                }
            }
            Kind::Orders => {
                let Ok(row) = serde_json::from_str::<OrderUpdate>(item) else {
                    request.valid = false;
                    return;
                };
                let wire = &row.order;
                if !["A", "B"].contains(&wire.side.as_ref()) || wire.timestamp <= 0.0 {
                    return;
                }
                let symbol = self.symbol(&wire.coin).unwrap_or(&wire.coin).to_owned();
                let status = if row.status == "filled" {
                    OrderStatus::Closed
                } else if row.status == "open" || row.status == "triggered" {
                    OrderStatus::Open
                } else {
                    OrderStatus::Canceled
                };
                let rank = if status == OrderStatus::Closed {
                    2
                } else if status == OrderStatus::Open {
                    0
                } else {
                    1
                };
                let id = wire.oid.to_string();
                let kind = match wire.order_type.to_ascii_lowercase().as_str() {
                    "market" => OrderType::Market,
                    "stop limit" | "stop market" => OrderType::StopLoss,
                    "take profit limit" | "take profit market" => OrderType::TakeProfit,
                    _ => OrderType::Limit,
                };
                request.history.stage(HistoryRow::Order(OrderState {
                    time: row.status_timestamp,
                    rank,
                    order: HistoricalOrder {
                        id,
                        symbol,
                        side: side(&wire.side),
                        kind,
                        status,
                        price: (wire.limit_px != 0.0).then_some(wire.limit_px),
                        amount: wire.orig_sz,
                        filled: (wire.orig_sz - wire.sz).max(0.0),
                        reduce_only: wire.reduce_only,
                        created_time: wire.timestamp,
                        updated_time: (row.status_timestamp != 0.0).then_some(row.status_timestamp),
                    },
                }));
            }
            Kind::Trades => {
                let Ok(row) = serde_json::from_str::<PublicTrade>(item) else {
                    request.valid = false;
                    return;
                };
                let symbol = self.symbol(row.coin).unwrap_or(row.coin).to_owned();
                if let Query::PublicTrades { symbol: target, .. } = &request.query
                    && *target != symbol
                {
                    return;
                }
                let key = format!("{}:{}:{}", row.coin, row.time, row.tid);
                if !request.seen.insert(key) {
                    return;
                }
                request.history.stage(HistoryRow::Trade(Trade {
                    id: row.tid.to_string(),
                    symbol,
                    price: row.px,
                    qty: row.sz,
                    quote_qty: row.px * row.sz,
                    timestamp: row.time,
                    side: side(row.side),
                }));
            }
        }
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) || !request.valid {
            self.history_failed(
                cx,
                request,
                &format!(
                    "Invalid Hyperliquid history response (HTTP {})",
                    response.status
                ),
            );
            return;
        }
        self.history_weight(cx, request.raw_count, request.streamed());
        if request.kind == Kind::Lookup {
            #[derive(Deserialize)]
            struct Lookup<'a> {
                #[serde(borrow)]
                status: &'a str,
                #[serde(default, borrow)]
                order: Option<OrderUpdate<'a>>,
            }
            let result = serde_json::from_str::<Lookup>(response.body)
                .map_err(|_| "Malformed Hyperliquid order lookup".to_owned())
                .and_then(|row| {
                    if row.status == "unknownOid" {
                        return Ok("{\"found\":false,\"definitive\":false}".into());
                    }
                    if row.status != "order" {
                        return Err("Malformed Hyperliquid order lookup status".into());
                    }
                    let row = row.order.ok_or("Missing lookup order")?;
                    if let Some(mut order) = self.map_order(cx, &row.order)
                        && symbol_matches(&request.query, &order.symbol)
                    {
                        order.status = if row.status == "open" || row.status == "triggered" {
                            OrderStatus::Open
                        } else if row.status == "filled" {
                            OrderStatus::Closed
                        } else {
                            OrderStatus::Canceled
                        };
                        #[derive(Serialize)]
                        struct Found {
                            found: bool,
                            order: safe_cex_engine::model::Order,
                        }
                        Ok(serde_json::to_string(&Found { found: true, order }).unwrap())
                    } else {
                        Ok("{\"found\":false,\"definitive\":false}".into())
                    }
                });
            match result {
                Ok(data) => cx.done(request.history.command(), data),
                Err(error) => cx.fail(request.history.command(), error),
            };
            return;
        }
        let ranged = matches!(
            request.kind,
            Kind::Fills | Kind::IncomeFills | Kind::IncomeFunding
        );
        let cap = if request.kind == Kind::IncomeFunding {
            500
        } else {
            2000
        };
        let inclusive_gap = ranged && request.added == 0 && request.raw_count >= cap;
        let gap = if inclusive_gap {
            Some("Hyperliquid history stalled on a full inclusive page; requested range may be incomplete".to_owned())
        } else if request.kind == Kind::Orders && request.raw_count >= 2000 {
            Some("Hyperliquid order history reached its rolling retention limit".to_owned())
        } else {
            None
        };
        // Inclusive physical bounds may repeat while new native IDs still advance.
        // Output filtering must not determine progress or manufacture completeness.
        let more = ranged && request.added > 0;
        let next = more.then(|| format!("{}:{}", request.newest, request.seen.len()));
        let step = request.history.advance_with_coverage(
            CursorPage {
                next,
                rows: request.raw_count as usize,
                fresh: request.added,
                want_more: more,
            },
            gap,
        );
        if let CursorStep::More(_) = step {
            request.cursor = request.newest;
            request.added = 0;
            request.raw_count = 0;
            self.history_page(cx, request);
            return;
        }
        if request.kind == Kind::IncomeFills
            && matches!(&request.query, Query::Income { kind: None, .. })
        {
            if !request.history.restart_stage() {
                request.history.reject(
                    cx,
                    VenueError::new("Hyperliquid income stage restarted before completion"),
                );
                return;
            }
            request.kind = Kind::IncomeFunding;
            request.cursor = request.from;
            request.newest = request.from;
            request.added = 0;
            request.seen.clear();
            request.raw_count = 0;
            self.history_page(cx, request);
            return;
        }
        self.history_finish(cx, request, Ok(()));
    }
    fn history_finish(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        result: Result<(), VenueError>,
    ) {
        let Request {
            history,
            kind,
            query,
            from,
            to,
            ..
        } = request;
        history.finish_with(cx, result, |rows| {
            encode_query(kind, &query, from, to, rows)
        });
    }
    pub(crate) fn history_failed(&self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if request.kind == Kind::Lookup {
            cx.fail(request.history.command(), error);
        } else {
            self.history_finish(cx, request, Err(VenueError::new(error)));
        }
    }
}
fn symbol_matches(query: &Query, symbol: &str) -> bool {
    let wanted = match query {
        Query::Fills { symbol, .. }
        | Query::OrderHistory { symbol, .. }
        | Query::Income { symbol, .. } => symbol.as_deref(),
        Query::PublicTrades { symbol, .. } | Query::OrderByClientId { symbol, .. } => {
            Some(symbol.as_str())
        }
    };
    wanted.is_none_or(|s| s == symbol)
}
fn income_matches(query: &Query, kind: &str) -> bool {
    if let Query::Income { kind: wanted, .. } = query {
        wanted.as_deref().is_none_or(|k| k == kind)
    } else {
        true
    }
}

fn stall_message(reason: Stall) -> String {
    match reason {
        Stall::PageBudget => "Hyperliquid history reached the page cap; requested range may be incomplete",
        _ => "Hyperliquid history stalled on a full inclusive page; requested range may be incomplete",
    }.into()
}
fn encode_rows(rows: &[HistoryRow]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}
fn encode_query(
    kind: Kind,
    query: &Query,
    from: f64,
    to: f64,
    rows: Vec<HistoryRow>,
) -> Result<String, VenueError> {
    let limit = match query {
        Query::Fills { limit, .. } => Some(limit.unwrap_or(50).clamp(1, 500) as usize),
        Query::OrderHistory { limit, .. }
        | Query::Income { limit, .. }
        | Query::PublicTrades { limit, .. } => limit.map(|n| n as usize),
        Query::OrderByClientId { .. } => None,
    };
    let result = match kind {
        Kind::Fills => {
            let mut output: Vec<_> = rows
                .into_iter()
                .filter_map(|row| match row {
                    HistoryRow::Execution(row) => Some(row),
                    _ => None,
                })
                .collect();
            output.sort_by(|a, b| b.time_ms.total_cmp(&a.time_ms).then(b.tid.cmp(&a.tid)));
            if let Some(limit) = limit {
                output.truncate(limit);
            }
            serde_json::to_string(&output)
        }
        Kind::Orders => {
            let mut states: HashMap<String, OrderState> = HashMap::new();
            for row in rows {
                let HistoryRow::Order(row) = row else {
                    continue;
                };
                if states.get(&row.order.id).is_some_and(|old| {
                    old.time > row.time || (old.time == row.time && old.rank >= row.rank)
                }) {
                    continue;
                }
                states.insert(row.order.id.clone(), row);
            }
            let mut output: Vec<_> = states
                .into_values()
                .map(|s| s.order)
                .filter(|o| {
                    symbol_matches(query, &o.symbol)
                        && o.created_time >= from
                        && o.created_time <= to
                })
                .collect();
            output.sort_by(|a, b| {
                b.created_time
                    .total_cmp(&a.created_time)
                    .then(b.id.cmp(&a.id))
            });
            if let Some(limit) = limit {
                output.truncate(limit);
            }
            serde_json::to_string(&output)
        }
        Kind::IncomeFills | Kind::IncomeFunding => {
            let mut output: Vec<_> = rows
                .into_iter()
                .filter_map(|row| match row {
                    HistoryRow::Income(row) => Some(row),
                    _ => None,
                })
                .collect();
            output.sort_by(|a, b| {
                b.timestamp
                    .total_cmp(&a.timestamp)
                    .then(a.tran_id.cmp(&b.tran_id))
            });
            if let Some(limit) = limit {
                output.truncate(limit);
            }
            serde_json::to_string(&output)
        }
        Kind::Trades => {
            let mut output: Vec<_> = rows
                .into_iter()
                .filter_map(|row| match row {
                    HistoryRow::Trade(row) => Some(row),
                    _ => None,
                })
                .collect();
            output.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
            if let Some(limit) = limit {
                output.truncate(limit);
            }
            serde_json::to_string(&output)
        }
        Kind::Lookup => unreachable!("client-ID lookup uses its separate scalar completion"),
    };
    result.map_err(|error| VenueError::new(error.to_string()))
}
