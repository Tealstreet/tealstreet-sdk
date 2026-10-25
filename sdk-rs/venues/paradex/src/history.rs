//! Rare cursor-paged reads never mutate the account store.
pub mod public_trades;
use crate::{Pending, account::Account};
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::num::parse_decimal;
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::url::encode;
use safe_cex_engine::{CommandId, Cx, Method, Outcome, Query, Response, VenueError, json::ArrayAt};
use serde::{Deserialize, Serialize};

/// Fills pages before the read falls back to order history or stops.
const FILL_PAGES: u32 = 5;

/// Whether a read with an optional output limit has kept enough rows, and
/// whether it is bounded at all.
fn limit_reached(limit: Option<u32>, kept: usize) -> (bool, bool) {
    match limit.filter(|limit| *limit > 0) {
        Some(limit) => (kept >= limit as usize, true),
        None => (false, false),
    }
}

/// Pages a read may walk before it stops with a cursor pending.
fn page_cap(query: &Query) -> u32 {
    match query {
        Query::Fills { .. } => FILL_PAGES,
        Query::Income { .. } => 200,
        _ => 50,
    }
}

#[derive(Default)]
pub struct History;
pub struct Request {
    query: Query,
    history: Box<CursorHistory<Output>>,
    valid: bool,
    raw_rows: usize,
    fallback: bool,
    lookup_history: bool,
    lookup_complete: bool,
    open_complete: bool,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Output {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
    Trade(PublicTrade),
    Lookup(safe_cex_engine::model::Order),
}
impl Output {
    fn timestamp(&self) -> f64 {
        match self {
            Self::Fill(row) => row.timestamp,
            Self::Order(row) => row.created_time,
            Self::Income(row) => row.timestamp,
            Self::Trade(row) => row.timestamp,
            Self::Lookup(_) => 0.0,
        }
    }
}
fn stall_message(reason: Stall) -> String {
    match reason {
        Stall::PageBudget => "Paradex history reached page cap".into(),
        _ => "Paradex repeated history cursor".into(),
    }
}
fn encode_rows(rows: &[Output]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string()))
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct Row {
    id: String,
    market: String,
    #[serde(alias = "orderId")]
    order_id: String,
    side: String,
    #[serde(rename = "type")]
    kind: String,
    status: String,
    price: String,
    size: String,
    remaining_size: String,
    avg_fill_price: String,
    fee: String,
    fee_currency: String,
    realized_pnl: String,
    realized_funding: String,
    liquidity: String,
    cancel_reason: String,
    flags: Vec<String>,
    #[serde(deserialize_with = "scalar")]
    created_at: String,
    #[serde(deserialize_with = "scalar")]
    last_updated_at: String,
    client_id: String,
}
fn scalar<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Scalar {
        Text(String),
        Number(u64),
    }
    Scalar::deserialize(d).map(|s| match s {
        Scalar::Text(s) => s,
        Scalar::Number(n) => n.to_string(),
    })
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
    side: OrderSide,
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
    side: OrderSide,
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
    asset: String,
    timestamp: f64,
    tran_id: String,
    trade_id: String,
    category: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicTrade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    side: OrderSide,
    timestamp: f64,
}
fn public_trade(row: Row) -> Option<Output> {
    Some(Output::Trade(PublicTrade {
        side: side(&row.side)?,
        id: row.id,
        symbol: row.market,
        price: number(&row.price),
        qty: number(&row.size),
        quote_qty: number(&row.size) * number(&row.price),
        timestamp: number(&row.created_at),
    }))
}
fn valid_row(row: &Row, query: &Query, fallback: bool) -> bool {
    if row.id.is_empty()
        || row.market.is_empty()
        || !parse_decimal(&row.created_at).is_some_and(|v| v > 0.)
    {
        return false;
    }
    let optional = [
        &row.fee,
        &row.realized_pnl,
        &row.realized_funding,
        &row.avg_fill_price,
        &row.last_updated_at,
    ];
    if optional
        .into_iter()
        .any(|s| !s.is_empty() && parse_decimal(s).is_none())
    {
        return false;
    }
    match query {
        Query::Income { .. } => true,
        Query::OrderHistory { .. } => {
            side(&row.side).is_some()
                && parse_decimal(&row.size).is_some_and(|n| n > 0.)
                && parse_decimal(&row.remaining_size)
                    .is_some_and(|n| n >= 0. && n <= number(&row.size))
                && (row.price.is_empty() || parse_decimal(&row.price).is_some_and(|n| n >= 0.))
        }
        Query::Fills { .. } | Query::PublicTrades { .. } => {
            side(&row.side).is_some()
                && parse_decimal(&row.size).is_some_and(|n| n > 0.)
                && (parse_decimal(&row.price).is_some_and(|n| n > 0.)
                    || (fallback && parse_decimal(&row.avg_fill_price).is_some_and(|n| n > 0.)))
                && (!fallback
                    || parse_decimal(&row.remaining_size)
                        .is_some_and(|n| n >= 0. && n <= number(&row.size)))
        }
        Query::OrderByClientId { .. } => true,
    }
}
fn number(s: &str) -> f64 {
    safe_cex_engine::num::parse(s)
}
fn optional_number(s: &str) -> Option<f64> {
    parse_decimal(s).filter(|n| *n != 0.)
}
fn side(s: &str) -> Option<OrderSide> {
    match s {
        "BUY" => Some(OrderSide::Buy),
        "SELL" => Some(OrderSide::Sell),
        _ => None,
    }
}
fn map_order(row: &Row) -> Option<HistoricalOrder> {
    let created_time = number(&row.created_at);
    if row.id.is_empty() || row.market.is_empty() || created_time <= 0. {
        return None;
    }
    let reduce_only = row.flags.iter().any(|f| f == "REDUCE_ONLY");
    let kind = match row.kind.as_str() {
        "MARKET" => OrderType::Market,
        "STOP_LIMIT" | "STOP_MARKET" if !reduce_only => OrderType::Conditional,
        "STOP_LIMIT" | "STOP_MARKET" | "STOP_LOSS_LIMIT" | "STOP_LOSS_MARKET" => {
            OrderType::StopLoss
        }
        "TAKE_PROFIT_LIMIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        _ => OrderType::Limit,
    };
    Some(HistoricalOrder {
        id: row.id.clone(),
        symbol: row.market.clone(),
        side: side(&row.side)?,
        kind,
        status: if matches!(row.status.as_str(), "NEW" | "OPEN" | "UNTRIGGERED") {
            OrderStatus::Open
        } else if !row.cancel_reason.is_empty() {
            OrderStatus::Canceled
        } else {
            OrderStatus::Closed
        },
        price: optional_number(&row.price),
        average_price: optional_number(&row.avg_fill_price),
        amount: number(&row.size),
        filled: (number(&row.size) - number(&row.remaining_size)).max(0.),
        reduce_only,
        created_time,
        updated_time: optional_number(&row.last_updated_at),
    })
}
impl History {
    pub fn query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        command: CommandId,
        query: Query,
        public_only: bool,
    ) {
        if let Query::PublicTrades { symbol, limit } = &query {
            public_trades::start(cx, account, command, symbol.clone(), *limit);
            return;
        }
        if public_only && !matches!(query, Query::PublicTrades { .. }) {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.done_context(
                &context,
                if matches!(query, Query::OrderByClientId { .. }) {
                    "{\"found\":false,\"definitive\":false}".into()
                } else {
                    "[]".into()
                },
            );
            return;
        }
        if let Query::OrderByClientId {
            client_order_id, ..
        } = &query
            && (client_order_id.is_empty()
                || client_order_id.chars().count() > 64
                || client_order_id.contains(['\r', '\n']))
        {
            cx.refuse_command(Operation::Read, "invalid Paradex client order ID");
            return;
        }
        let bounds = match &query {
            Query::Fills { from_ms, to_ms, .. }
            | Query::OrderHistory { from_ms, to_ms, .. }
            | Query::Income { from_ms, to_ms, .. } => (*from_ms, *to_ms),
            _ => (None, None),
        };
        if bounds
            .0
            .into_iter()
            .chain(bounds.1)
            .any(|v| !v.is_finite() || v < 0.)
            || matches!(bounds, (Some(a),Some(b)) if a>b)
        {
            cx.refuse_command(Operation::Read, "invalid Paradex history time range");
            return;
        }
        if matches!(query, Query::Fills { symbol: None, .. }) {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.done_context(&context, "[]".into());
            return;
        }
        let request = Request {
            history: Box::new(CursorHistory::new(
                Completion::new(command),
                CursorWalk::new(page_cap(&query)),
                stall_message,
                encode_rows,
            )),
            query,
            valid: true,
            raw_rows: 0,
            fallback: false,
            lookup_history: false,
            lookup_complete: false,
            open_complete: false,
        };
        self.issue(cx, account, request);
    }
    fn issue(&self, cx: &mut Cx<'_, Pending>, account: &mut Account, mut request: Request) {
        let mut params = Vec::new();
        let (path, symbol, from, to, page_size, public) = match &request.query {
            Query::Fills {
                symbol,
                from_ms,
                to_ms,
                limit,
                ..
            } => (
                if request.fallback {
                    "/orders-history"
                } else {
                    "/fills"
                },
                symbol.as_deref(),
                *from_ms,
                *to_ms,
                limit.unwrap_or(100).clamp(1, 500),
                false,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                ..
            } => (
                "/orders-history",
                symbol.as_deref(),
                *from_ms,
                *to_ms,
                100,
                false,
            ),
            Query::Income {
                symbol,
                from_ms,
                to_ms,
                ..
            } => {
                let end = to_ms.unwrap_or(cx.now);
                (
                    "/fills",
                    symbol.as_deref(),
                    Some(from_ms.unwrap_or(end - 30. * 86400000.)),
                    Some(end),
                    500,
                    false,
                )
            }
            Query::PublicTrades { .. } => return,
            Query::OrderByClientId { symbol, .. } => (
                if request.lookup_history {
                    "/orders-history"
                } else {
                    "/orders"
                },
                Some(symbol.as_str()),
                None,
                None,
                100,
                false,
            ),
        };
        if from.into_iter().chain(to).any(|v| !v.is_finite())
            || matches!((from, to), (Some(a),Some(b)) if a>b)
        {
            self.on_rejected(cx, request, "invalid Paradex history time range");
            return;
        }
        if !public && account.credentials().is_none() {
            self.on_rejected(cx, request, "Paradex private auth is not initialized");
            return;
        }
        if let Some(symbol) = symbol {
            let Some(market) = cx.store.market(symbol) else {
                let result = if matches!(request.query, Query::OrderByClientId { .. }) {
                    "{\"found\":false,\"definitive\":false}"
                } else {
                    "[]"
                };
                request.history.advance(CursorPage {
                    next: None,
                    rows: 0,
                    fresh: 0,
                    want_more: false,
                });
                request
                    .history
                    .finish_with(cx, Ok(()), |_| Ok(result.into()));
                return;
            };
            params.push(format!("market={}", encode(&market.id)));
        }
        params.push(format!(
            "{}={page_size}",
            if request.fallback {
                "limit"
            } else {
                "page_size"
            }
        ));
        if let Some(from) = from {
            params.push(format!("start_at={}", safe_cex_engine::num::format(from)));
        }
        if let Some(to) = to {
            params.push(format!("end_at={}", safe_cex_engine::num::format(to)));
        }
        if let Some(cursor) = request.history.cursor() {
            params.push(format!("cursor={}", encode(cursor)));
        }
        let path = format!("{path}?{}", params.join("&"));
        account.request(
            cx,
            Method::Get,
            path,
            None,
            Some(ArrayAt::Field("results")),
            Pending::History(request),
        );
    }

    pub fn on_item(&mut self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        request.raw_rows += 1;
        if let Query::OrderByClientId {
            symbol,
            client_order_id,
        } = &request.query
        {
            let Ok(row) = serde_json::from_str::<crate::account::NativeOrder>(item) else {
                request.valid = false;
                return;
            };
            if row.market == *symbol
                && row.client_id == *client_order_id
                && request.history.fresh(row.id.clone())
            {
                if let Ok(order) = crate::account::map_order(&row) {
                    request.history.stage(Output::Lookup(order));
                } else {
                    request.valid = false;
                }
            }
            return;
        }
        let Ok(mut row) = serde_json::from_str::<Row>(item) else {
            request.valid = false;
            return;
        };
        if !valid_row(&row, &request.query, request.fallback) {
            request.valid = false;
            return;
        }
        let wanted_symbol = match &request.query {
            Query::Fills { symbol, .. }
            | Query::OrderHistory { symbol, .. }
            | Query::Income { symbol, .. } => symbol.as_deref(),
            Query::PublicTrades { symbol, .. } | Query::OrderByClientId { symbol, .. } => {
                Some(symbol.as_str())
            }
        };
        if wanted_symbol.is_some_and(|symbol| symbol != row.market) {
            return;
        }
        if request.fallback && row.order_id.is_empty() {
            row.order_id = row.id.clone();
        }
        if !row.id.is_empty() && !request.history.fresh(row.id.clone()) {
            return;
        }
        match &request.query {
            Query::Fills { .. } => {
                let Query::Fills {
                    symbol,
                    order_id,
                    from_ms,
                    to_ms,
                    ..
                } = &request.query
                else {
                    return;
                };
                if symbol.as_ref().is_some_and(|s| *s != row.market)
                    || order_id.as_ref().is_some_and(|s| *s != row.order_id)
                {
                    return;
                }
                let timestamp = number(&row.created_at);
                let price = if number(&row.price) > 0. {
                    number(&row.price)
                } else {
                    number(&row.avg_fill_price)
                };
                let qty = if request.fallback {
                    (number(&row.size) - number(&row.remaining_size)).max(0.)
                } else {
                    number(&row.size)
                };
                if timestamp <= 0.
                    || price <= 0.
                    || qty <= 0.
                    || from_ms.is_some_and(|t| timestamp < t)
                    || to_ms.is_some_and(|t| timestamp > t)
                {
                    return;
                }
                let Some(side) = side(&row.side) else {
                    return;
                };
                request.history.stage(Output::Fill(Execution {
                    id: row.id,
                    timestamp: (timestamp / 1000.).floor(),
                    symbol: row.market,
                    order_id: row.order_id,
                    price,
                    qty,
                    side,
                    commission: number(&row.fee).abs(),
                    commission_asset: if row.fee_currency.is_empty() {
                        cx.store
                            .market(symbol.as_deref().unwrap_or(""))
                            .map_or("USD".into(), |m| m.quote.clone())
                    } else {
                        row.fee_currency
                    },
                    realized_pnl: row
                        .realized_pnl
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite()),
                    maker: (!row.liquidity.is_empty()).then(|| row.liquidity == "MAKER"),
                }));
            }
            Query::OrderHistory { .. } => {
                if let Some(order) = map_order(&row) {
                    request.history.stage(Output::Order(order));
                }
            }
            Query::OrderByClientId { .. } => unreachable!(),
            Query::Income { .. } => {
                let timestamp = number(&row.created_at);
                if timestamp <= 0. || row.market.is_empty() {
                    return;
                }
                let asset = if row.fee_currency.is_empty() {
                    "USDC"
                } else {
                    &row.fee_currency
                };
                for (value, income_type, suffix, category) in [
                    (
                        number(&row.realized_pnl),
                        "REALIZED_PNL",
                        "pnl",
                        "realized_pnl",
                    ),
                    (
                        -number(&row.fee),
                        if number(&row.fee) > 0. {
                            "COMMISSION"
                        } else {
                            "REBATE"
                        },
                        "fee",
                        if number(&row.fee) > 0. {
                            "commission"
                        } else {
                            "rebate"
                        },
                    ),
                    (
                        number(&row.realized_funding),
                        "FUNDING_FEE",
                        "funding",
                        "funding",
                    ),
                ] {
                    let wanted = match &request.query {
                        Query::Income { kind, .. } => kind.as_deref(),
                        _ => None,
                    };
                    if value != 0. && wanted.is_none_or(|kind| kind == income_type) {
                        request.history.stage(Output::Income(Income {
                            symbol: row.market.clone(),
                            income_type,
                            income: value,
                            asset: asset.into(),
                            timestamp,
                            tran_id: format!("{}:{suffix}", row.id),
                            trade_id: row.id.clone(),
                            category,
                        }));
                    }
                }
            }
            Query::PublicTrades { .. } => {
                if let Some(row) = public_trade(row) {
                    request.history.stage(row);
                }
            }
        }
    }
    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        mut request: Request,
        response: Response<'_>,
    ) {
        if let Err(error) = crate::success(response) {
            self.on_failed(cx, request, &error);
            return;
        }
        if !request.valid {
            self.on_failed(cx, request, "malformed Paradex history row");
            return;
        }
        #[derive(Deserialize)]
        struct Envelope<'a> {
            next: Option<String>,
            #[serde(borrow, rename = "results")]
            rows: &'a serde_json::value::RawValue,
        }
        let Ok(envelope) = serde_json::from_str::<Envelope<'_>>(response.body) else {
            self.on_failed(cx, request, "invalid Paradex history envelope");
            return;
        };
        if !envelope.rows.get().starts_with('[') {
            self.on_failed(cx, request, "invalid Paradex history list");
            return;
        }
        let next = envelope.next.filter(|s| !s.is_empty());
        if matches!(request.query, Query::OrderByClientId { .. }) && !request.lookup_history {
            if request.history.staged_len() > 0 {
                request.history.advance(CursorPage {
                    next: None,
                    rows: request.raw_rows,
                    fresh: request.raw_rows,
                    want_more: false,
                });
                self.finish(cx, request, Ok(()));
                return;
            }
            let page = CursorPage {
                next: next.clone(),
                rows: request.raw_rows,
                fresh: request.raw_rows,
                want_more: true,
            };
            match request.history.advance_probe(page) {
                CursorStep::More(_) => {
                    self.issue(cx, account, request);
                    return;
                }
                CursorStep::Stalled(Stall::RepeatedCursor) => {
                    request
                        .history
                        .reject(cx, VenueError::new("Paradex repeated open-order cursor"));
                    return;
                }
                CursorStep::Stalled(_) | CursorStep::Done => {}
            }
            request.open_complete = next.is_none();
            request.lookup_history = true;
            if !request.history.restart_stage() {
                request
                    .history
                    .reject(cx, VenueError::new("invalid Paradex lookup stage"));
                return;
            }
            self.issue(cx, account, request);
            return;
        }
        if matches!(request.query, Query::Fills { .. })
            && request.raw_rows == 0
            && !request.fallback
            && (next.is_none() || request.history.pages() + 1 >= FILL_PAGES)
        {
            // Native aggregate fallback ends this empty discovery stage; it is
            // not a partial fill result or a new command/completion owner.
            request.history.advance(CursorPage {
                next: None,
                rows: 0,
                fresh: 0,
                want_more: false,
            });
            request.fallback = true;
            if !request.history.restart_stage() {
                request
                    .history
                    .reject(cx, VenueError::new("invalid Paradex fills stage"));
                return;
            }
            self.issue(cx, account, request);
            return;
        }
        let kept = request.history.accepted_len() + request.history.staged_len();
        let (enough, bounded) = match &request.query {
            Query::Fills { limit, .. } => {
                (kept >= limit.unwrap_or(100).clamp(1, 500) as usize, true)
            }
            Query::OrderHistory { limit, .. } | Query::Income { limit, .. } => {
                limit_reached(*limit, kept)
            }
            Query::OrderByClientId { .. } => (kept > 0, true),
            _ => (false, false),
        };
        request.lookup_complete = request.open_complete && request.lookup_history && next.is_none();
        let single = request.fallback || matches!(request.query, Query::PublicTrades { .. });
        let page = CursorPage {
            next: if single { None } else { next },
            rows: request.raw_rows,
            fresh: request.raw_rows,
            want_more: !enough,
        };
        match request.history.advance(page) {
            CursorStep::More(_) => {
                self.issue(cx, account, request);
                return;
            }
            CursorStep::Stalled(Stall::PageBudget) if bounded => {}
            CursorStep::Stalled(Stall::PageBudget) => {
                request.history.reject(
                    cx,
                    VenueError::new("INCOMPLETE: Paradex history page cap reached"),
                );
                return;
            }
            CursorStep::Stalled(_) => {
                request
                    .history
                    .reject(cx, VenueError::new("Paradex repeated history cursor"));
                return;
            }
            CursorStep::Done => {}
        }
        self.finish(cx, request, Ok(()));
    }
    fn finish(&self, cx: &mut Cx<'_, Pending>, request: Request, result: Result<(), VenueError>) {
        let limit = match request.query {
            Query::Fills { limit, .. } | Query::PublicTrades { limit, .. } => {
                Some(limit.unwrap_or(100).clamp(1, 500) as usize)
            }
            Query::OrderHistory { limit, .. } | Query::Income { limit, .. } => {
                limit.filter(|v| *v > 0).map(|v| v as usize)
            }
            _ => None,
        };
        let lookup = matches!(request.query, Query::OrderByClientId { .. });
        let sort = !matches!(request.query, Query::PublicTrades { .. });
        let definitive = request.lookup_complete;
        request.history.finish_with(cx, result, move |mut rows| {
            if lookup {
                #[derive(Serialize)]
                struct Lookup<'a> {
                    found: bool,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    definitive: Option<bool>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    order: Option<&'a safe_cex_engine::model::Order>,
                }
                let order = rows.first().and_then(|row| match row {
                    Output::Lookup(order) => Some(order),
                    _ => None,
                });
                return serde_json::to_string(&Lookup {
                    found: order.is_some(),
                    definitive: order.is_none().then_some(definitive),
                    order,
                })
                .map_err(|e| VenueError::new(e.to_string()));
            }
            if sort {
                rows.sort_by(|a, b| b.timestamp().total_cmp(&a.timestamp()));
            }
            if let Some(limit) = limit {
                rows.truncate(limit);
            }
            encode_rows(&rows)
        });
    }
    pub fn on_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if matches!(request.query, Query::OrderByClientId { .. }) {
            request.history.reject(cx, VenueError::new(error));
        } else {
            self.finish(cx, request, Err(VenueError::new(error)));
        }
    }
    pub fn on_rejected(&self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        request
            .history
            .reject(cx, VenueError::new(error).outcome(Outcome::NotSent));
    }
}
