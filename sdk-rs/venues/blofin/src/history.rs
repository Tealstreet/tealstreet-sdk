//! Rare reads stream into typed results; they never change the account store.
use super::*;
use safe_cex_engine::history::{Completion, CursorHistory, ReadCompletion, ReadCursorHistory};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::url::encode;
use safe_cex_engine::{ErrorKind, Outcome, VenueError};
use safe_cex_engine::{
    Query,
    model::{OrderStatus, OrderType},
};
use serde::{Deserialize, Serialize};

/// Pages per read before paging that still has a cursor is incomplete.
const MAX_PAGES: u32 = 100;

pub struct Request {
    query: Query,
    history: CursorHistory<Mapped>,
    /// Smallest numeric trade id on the current page: the next `after`.
    last_cursor: String,
    received: usize,
    reached_start: bool,
    failed: bool,
}
pub struct PublicTradesRequest {
    query: Query,
    history: ReadCursorHistory<Mapped>,
    received: usize,
    failed: bool,
}
enum Mapped {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
    Trade(Trade),
}
fn missing_finalizer(_: &[Mapped]) -> Result<String, VenueError> {
    Err(VenueError::new(
        "BloFin history requires a contextual finalizer",
    ))
}
fn encode_result(query: &Query, rows: Vec<Mapped>) -> Result<String, VenueError> {
    let mismatch = || VenueError::new("BloFin history result kind mismatch");
    let result = match query {
        Query::Fills { limit, .. } => {
            let mut rows = rows
                .into_iter()
                .map(|row| match row {
                    Mapped::Fill(row) => Ok(row),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
            rows.truncate(limit.unwrap_or(100).clamp(1, 100) as usize);
            serde_json::to_string(&rows)
        }
        Query::OrderHistory { .. } => {
            let mut rows = rows
                .into_iter()
                .map(|row| match row {
                    Mapped::Order(row) => Ok(row),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            rows.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
            serde_json::to_string(&rows)
        }
        Query::Income { limit, .. } => {
            let mut rows = rows
                .into_iter()
                .map(|row| match row {
                    Mapped::Income(row) => Ok(row),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
            if let Some(limit) = limit {
                rows.truncate(*limit as usize);
            }
            serde_json::to_string(&rows)
        }
        Query::PublicTrades { .. } => {
            let rows = rows
                .into_iter()
                .map(|row| match row {
                    Mapped::Trade(row) => Ok(row),
                    _ => Err(mismatch()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            serde_json::to_string(&rows)
        }
        Query::OrderByClientId { .. } => return Err(mismatch()),
    };
    result.map_err(|error| VenueError::new(error.to_string()))
}
impl Request {
    fn finish(self, cx: &mut Cx<'_, Pending>, result: Result<(), VenueError>) {
        let Self { query, history, .. } = self;
        history.finish_with(cx, result, |rows| encode_result(&query, rows));
    }
    fn reject(self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        self.history.reject(cx, error);
    }
}
impl PublicTradesRequest {
    fn finish(self, cx: &mut Cx<'_, Pending>, result: Result<(), VenueError>) {
        let Self { query, history, .. } = self;
        let _ = history.finish_with(cx, result, |rows| encode_result(&query, rows));
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row<'a> {
    #[serde(borrow)]
    inst_id: Str<'a>,
    #[serde(borrow, default)]
    inst_type: Str<'a>,
    #[serde(borrow, default)]
    actual_side: Str<'a>,
    #[serde(borrow, default, alias = "feeCcy", alias = "feeAsset", alias = "ccy")]
    fee_alias: Str<'a>,

    #[serde(borrow, default)]
    trade_id: Str<'a>,
    #[serde(borrow, default)]
    order_id: Str<'a>,
    #[serde(borrow, default)]
    side: Str<'a>,
    #[serde(borrow, default)]
    position_side: Str<'a>,
    #[serde(borrow, default)]
    order_type: Str<'a>,
    #[serde(borrow, default)]
    state: Str<'a>,
    #[serde(default)]
    price: Number,
    #[serde(default)]
    average_price: Number,
    #[serde(default)]
    size: Number,
    #[serde(default)]
    filled_size: Number,
    #[serde(default, deserialize_with = "optional_number")]
    fill_price: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    fill_size: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    fee: Option<Number>,
    #[serde(default)]
    fill_pnl: Number,
    #[serde(borrow, default)]
    fee_currency: Str<'a>,
    #[serde(default)]
    ts: Number,
    #[serde(default)]
    create_time: Number,
    #[serde(default)]
    update_time: Number,
    #[serde(default)]
    reduce_only: Boolean,
    #[serde(default, deserialize_with = "optional_number")]
    fill_px: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    exec_price: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    executed_price: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    fill_sz: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    trade_qty: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    exec_qty: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    executed_qty: Option<Number>,
    #[serde(default)]
    fill_time: Number,
    #[serde(default)]
    exec_time: Number,
    #[serde(default)]
    trade_time: Number,
    #[serde(default)]
    u_time: Number,
    #[serde(default, deserialize_with = "optional_number")]
    fill_fee: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    exec_fee: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    trade_fee: Option<Number>,
    #[serde(default, deserialize_with = "optional_number")]
    actual_fee: Option<Number>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    commission_asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
    realized_pnl: f64,
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
    position_side: &'static str,
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
    trade_id: String,
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
fn side(value: &str) -> &'static str {
    if value == "sell" { "sell" } else { "buy" }
}
fn position_side(value: &str) -> &'static str {
    match value {
        "net" => "both",
        "short" => "short",
        _ => "long",
    }
}
fn nonzero(value: f64) -> Option<f64> {
    (value != 0.0).then_some(value)
}
fn public_trade(row: &Row<'_>, market: &Market, id: String, time: f64) -> Trade {
    let qty = account::clean(row.size.0 * market.contract_size.unwrap_or(1.0));
    Trade {
        id,
        symbol: market.symbol.clone(),
        price: row.price.0,
        qty,
        quote_qty: qty * row.price.0,
        timestamp: time,
        side: side(&row.side),
    }
}
impl Blofin {
    pub(crate) fn start_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let query = match query {
            Query::PublicTrades { symbol, limit } => {
                self.start_public_trades(cx, command, symbol, limit);
                return;
            }
            query => query,
        };
        let completion = Completion::new(command);
        if !matches!(
            query,
            Query::PublicTrades { .. } | Query::OrderByClientId { .. }
        ) && !self.signed()
        {
            completion.finish(cx, Ok("[]".into()));
            return;
        }
        let (symbol, from, to) = match &query {
            Query::OrderByClientId { .. } => {
                completion.finish(
                    cx,
                    Err(
                        VenueError::new("BloFin client-order-ID lookup is unsupported")
                            .kind(ErrorKind::Unsupported)
                            .outcome(Outcome::NotSent),
                    ),
                );
                return;
            }
            Query::Fills {
                symbol,
                from_ms,
                to_ms,
                ..
            }
            | Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                ..
            }
            | Query::Income {
                symbol,
                from_ms,
                to_ms,
                ..
            } => (symbol.as_deref(), *from_ms, *to_ms),
            Query::PublicTrades { symbol, .. } => (Some(symbol.as_str()), None, None),
        };
        if from.is_some_and(|v| !v.is_finite())
            || to.is_some_and(|v| !v.is_finite())
            || from.zip(to).is_some_and(|(f, t)| f > t)
        {
            completion.finish(
                cx,
                Err(VenueError::new("Invalid BloFin history time range")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent)),
            );
            return;
        }
        if let Some(s) = symbol
            && cx.store.market(s).is_none()
        {
            completion.finish(
                cx,
                Err(VenueError::new(format!("Unknown market {s}"))
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent)),
            );
            return;
        }
        if matches!(&query, Query::Fills { symbol: None, .. }) {
            completion.finish(cx, Ok("[]".into()));
            return;
        }
        self.history_page(
            cx,
            Request {
                query,
                history: CursorHistory::new(
                    completion,
                    CursorWalk::new(MAX_PAGES),
                    |_| "BloFin income pagination did not finish".into(),
                    missing_finalizer,
                ),
                last_cursor: String::new(),
                received: 0,
                reached_start: false,
                failed: false,
            },
        );
    }
    fn history_page(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let (endpoint, symbol, from, to, limit) = match &request.query {
            Query::OrderByClientId { .. } => {
                request.reject(
                    cx,
                    VenueError::new("BloFin client-order-ID lookup is unsupported")
                        .kind(ErrorKind::Unsupported)
                        .outcome(Outcome::NotSent),
                );
                return;
            }
            Query::Fills {
                symbol,
                from_ms,
                to_ms,
                limit,
                ..
            } => (
                "/api/v1/trade/fills-history",
                symbol.as_deref(),
                *from_ms,
                *to_ms,
                limit.unwrap_or(100).clamp(1, 100),
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                "/api/v1/trade/orders-history",
                symbol.as_deref(),
                *from_ms,
                *to_ms,
                limit.unwrap_or(100).clamp(1, 100),
            ),
            Query::Income {
                symbol,
                from_ms,
                to_ms,
                ..
            } => (
                "/api/v1/trade/fills-history",
                symbol.as_deref(),
                *from_ms,
                *to_ms,
                100,
            ),
            Query::PublicTrades { symbol, limit } => (
                "/api/v1/market/trades",
                Some(symbol.as_str()),
                None,
                None,
                limit.unwrap_or(100).clamp(1, 100),
            ),
        };
        let mut params = Vec::new();
        if let Some(s) = symbol.and_then(|s| cx.store.market(s)) {
            params.push(format!("instId={}", encode(&s.id)));
        }
        if let Query::Fills {
            order_id: Some(id), ..
        } = &request.query
        {
            params.push(format!("orderId={}", encode(id)));
        }
        if let Some(from) = from {
            params.push(format!("begin={}", num::format(from)));
        }
        if let Some(to) = to {
            params.push(format!("end={}", num::format(to)));
        }
        params.push(format!("limit={limit}"));
        if let Some(cursor) = request.history.cursor() {
            params.push(format!("after={}", encode(cursor)));
        }
        let path = format!("{endpoint}?{}", params.join("&"));
        let headers = if matches!(request.query, Query::PublicTrades { .. }) {
            Ok(vec![])
        } else {
            self.headers(
                |bytes| cx.fill_signing_entropy(bytes),
                &path,
                Method::Get,
                cx.now,
                "",
            )
        };
        match headers {
            Ok(headers) => {
                cx.request_streamed(
                    Method::Get,
                    format!("{}{path}", self.host()),
                    headers,
                    ArrayAt::EnvelopeField("data"),
                    Pending::History(request),
                );
            }
            Err(e) => request.reject(cx, VenueError::new(e)),
        }
    }
    pub(crate) fn history_item(&self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        let row = match serde_json::from_str::<Row<'_>>(item) {
            Ok(r) => r,
            Err(_) => {
                request.failed = true;
                return;
            }
        };
        request.received += 1;
        if matches!(request.query, Query::OrderHistory { .. }) {
            request.last_cursor = row.order_id.to_string();
        } else if !row.trade_id.is_empty()
            && row.trade_id.bytes().all(|b| b.is_ascii_digit())
            && (request.last_cursor.is_empty()
                || row.trade_id.len() < request.last_cursor.len()
                || (row.trade_id.len() == request.last_cursor.len()
                    && row.trade_id.as_ref() < request.last_cursor.as_str()))
        {
            request.last_cursor = row.trade_id.to_string();
        }
        let market = cx.store.market_by_id(&row.inst_id);
        let symbol = market.map_or_else(|| row.inst_id.to_string(), |m| m.symbol.clone());
        if let Query::Fills {
            symbol: Some(expected),
            ..
        } = &request.query
            && (market.is_none_or(|m| &m.symbol != expected)
                || (!row.inst_type.is_empty() && !row.inst_type.eq_ignore_ascii_case("SWAP")))
        {
            return;
        }
        let raw_time = if matches!(request.query, Query::Fills { .. }) {
            [
                row.fill_time.0,
                row.exec_time.0,
                row.trade_time.0,
                row.u_time.0,
                row.update_time.0,
                row.ts.0,
                row.create_time.0,
            ]
            .into_iter()
            .find(|v| *v > 0.0)
            .unwrap_or(0.0)
        } else if row.ts.0 > 0.0 {
            row.ts.0
        } else {
            row.create_time.0
        };
        if raw_time <= 0.0 || row.inst_id.is_empty() {
            return;
        }
        let time = if matches!(request.query, Query::Fills { .. }) && raw_time < 1e12 {
            raw_time * 1000.0
        } else {
            raw_time
        };
        let (from, to) = match &request.query {
            Query::Fills { from_ms, to_ms, .. }
            | Query::OrderHistory { from_ms, to_ms, .. }
            | Query::Income { from_ms, to_ms, .. } => (*from_ms, *to_ms),
            _ => (None, None),
        };
        if from.is_some_and(|from| time < from) {
            request.reached_start = true;
            return;
        }
        if to.is_some_and(|to| time > to) {
            return;
        }
        let id = if matches!(request.query, Query::OrderHistory { .. }) {
            row.order_id.to_string()
        } else {
            row.trade_id.to_string()
        };
        if id.is_empty()
            || (!matches!(request.query, Query::Fills { .. }) && !request.history.fresh(id.clone()))
        {
            return;
        }
        match &request.query {
            Query::Fills { .. } => {
                if let Query::Fills {
                    order_id: Some(id), ..
                } = &request.query
                    && row.order_id != *id
                {
                    return;
                }
                let Some(market) = market else {
                    return;
                };
                if let Query::Fills {
                    symbol: Some(expected),
                    ..
                } = &request.query
                    && expected != &market.symbol
                {
                    return;
                }
                if !row.inst_type.is_empty() && !row.inst_type.eq_ignore_ascii_case("SWAP") {
                    return;
                }
                let price = [
                    row.fill_price,
                    row.fill_px,
                    row.exec_price,
                    row.executed_price,
                ]
                .into_iter()
                .flatten()
                .next()
                .unwrap_or_default()
                .0;
                let qty = [
                    row.fill_size,
                    row.fill_sz,
                    row.trade_qty,
                    row.exec_qty,
                    row.executed_qty,
                ]
                .into_iter()
                .flatten()
                .next()
                .unwrap_or_default()
                .0
                .abs();
                let side_value = if row.side.is_empty() {
                    row.actual_side.as_ref()
                } else {
                    row.side.as_ref()
                }
                .to_ascii_lowercase();
                if price <= 0.0 || qty <= 0.0 || !matches!(side_value.as_str(), "buy" | "sell") {
                    return;
                }
                if !request.history.fresh(id.clone()) {
                    return;
                }
                let fee = [
                    row.fee,
                    row.fill_fee,
                    row.exec_fee,
                    row.trade_fee,
                    row.actual_fee,
                ]
                .into_iter()
                .flatten()
                .next()
                .unwrap_or_default()
                .0;
                let fee_currency = if row.fee_currency.is_empty() {
                    row.fee_alias.as_ref()
                } else {
                    row.fee_currency.as_ref()
                };
                request.history.stage(Mapped::Fill(Execution {
                    id,
                    timestamp: (time / 1000.0).floor(),
                    symbol: symbol.clone(),
                    order_id: row.order_id.to_string(),
                    price,
                    qty: account::clean(qty * market.contract_size.unwrap_or(1.0)),
                    side: side(&side_value),
                    commission: fee.abs(),
                    commission_asset: (!fee_currency.is_empty()).then(|| fee_currency.to_owned()),
                    position_side: (!row.position_side.is_empty())
                        .then(|| position_side(&row.position_side)),
                    realized_pnl: row.fill_pnl.0,
                }));
            }
            Query::OrderHistory { .. } => {
                if row.create_time.0 <= 0.0 || !matches!(row.side.as_ref(), "buy" | "sell") {
                    return;
                }
                // Sizes are contracts on the wire; like fills, report base units or nothing.
                let Some(contract_size) = market.map(|m| m.contract_size.unwrap_or(1.0)) else {
                    return;
                };
                request.history.stage(Mapped::Order(HistoricalOrder {
                    id,
                    symbol: symbol.clone(),
                    side: side(&row.side),
                    kind: match row.order_type.as_ref() {
                        "market" => OrderType::Market,
                        "trigger" => OrderType::Conditional,
                        _ => OrderType::Limit,
                    },
                    status: match row.state.as_ref() {
                        "live" | "partially_filled" => OrderStatus::Open,
                        "canceled" | "cancelled" => OrderStatus::Canceled,
                        _ => OrderStatus::Closed,
                    },
                    price: nonzero(row.price.0),
                    average_price: nonzero(row.average_price.0),
                    amount: account::clean(row.size.0 * contract_size),
                    filled: account::clean(row.filled_size.0 * contract_size),
                    reduce_only: row.reduce_only.0,
                    position_side: position_side(&row.position_side),
                    created_time: row.create_time.0,
                    updated_time: nonzero(row.update_time.0),
                }));
            }
            Query::Income { .. } => {
                for (kind, value, category, suffix) in [
                    ("REALIZED_PNL", row.fill_pnl.0, "realized_pnl", "pnl"),
                    (
                        if row.fee.unwrap_or_default().0 > 0.0 {
                            "COMMISSION"
                        } else {
                            "REBATE"
                        },
                        -row.fee.unwrap_or_default().0,
                        if row.fee.unwrap_or_default().0 > 0.0 {
                            "commission"
                        } else {
                            "rebate"
                        },
                        "fee",
                    ),
                ] {
                    if value == 0.0 {
                        continue;
                    }
                    if let Query::Income {
                        kind: Some(wanted), ..
                    } = &request.query
                        && !wanted.eq_ignore_ascii_case(kind)
                    {
                        continue;
                    }
                    request.history.stage(Mapped::Income(Income {
                        symbol: symbol.clone(),
                        income_type: kind,
                        income: value,
                        asset: "USDT",
                        timestamp: time,
                        tran_id: format!("{id}:{suffix}"),
                        trade_id: id.clone(),
                        category,
                    }));
                }
            }
            Query::PublicTrades { .. } => {
                let Some(market) = market else {
                    return;
                };
                request
                    .history
                    .stage(Mapped::Trade(public_trade(&row, market, id, time)));
            }
            Query::OrderByClientId { .. } => request.failed = true,
        }
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if let Err(error) = check(&response) {
            request.finish(cx, Err(VenueError::new(error)));
            return;
        }
        if request.failed {
            request.finish(cx, Err(VenueError::new("Malformed BloFin history row")));
            return;
        }
        let (page_limit, enough) = match &request.query {
            Query::Fills { limit, .. } => {
                let count = limit.unwrap_or(100).clamp(1, 100) as usize;
                (
                    count,
                    request.history.accepted_len() + request.history.staged_len() >= count,
                )
            }
            Query::Income { .. } => (100, false),
            _ => (usize::MAX, true),
        };
        let want_more = request.received >= page_limit && !request.reached_start && !enough;
        if want_more && request.last_cursor.is_empty() {
            request.reject(
                cx,
                VenueError::new("BloFin income pagination did not finish"),
            );
            return;
        }
        let page = CursorPage {
            next: Some(std::mem::take(&mut request.last_cursor)),
            rows: request.received,
            fresh: request.received,
            want_more,
        };
        match request.history.advance(page) {
            CursorStep::More(_) => {
                request.received = 0;
                self.history_page(cx, request);
            }
            CursorStep::Done => request.finish(cx, Ok(())),
            CursorStep::Stalled(_) => request.reject(
                cx,
                VenueError::new("BloFin income pagination did not finish"),
            ),
        }
    }
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        request.finish(cx, Err(VenueError::new(error)));
    }

    fn start_public_trades(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
        limit: Option<u32>,
    ) {
        let context = match cx.admit_http_command(Self::read_policy()) {
            Ok(context) => context,
            Err(_) => return,
        };
        let completion = match ReadCompletion::from_context(cx, command, context) {
            Ok(completion) => completion,
            Err(_) => return,
        };
        let Some(market) = cx.store.market(&symbol) else {
            let _ = completion.finish(
                cx,
                Err(VenueError::new(format!("Unknown market {symbol}"))
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent)),
            );
            return;
        };
        let path = format!(
            "/api/v1/market/trades?instId={}&limit={}",
            encode(&market.id),
            limit.unwrap_or(100).clamp(1, 100),
        );
        let request = PublicTradesRequest {
            query: Query::PublicTrades { symbol, limit },
            history: ReadCursorHistory::new(
                completion,
                CursorWalk::new(MAX_PAGES),
                |_| "BloFin income pagination did not finish".into(),
                missing_finalizer,
            ),
            received: 0,
            failed: false,
        };
        match cx.request_managed(
            request.history.root().clone(),
            Method::Get,
            format!("{}{path}", self.host()),
            None,
            Pending::PublicTrades(request),
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::EnvelopeField("data")),
            Err((pending, error)) => self.on_sign_error(cx, pending, error.venue_error()),
        }
    }

    pub(crate) fn public_trades_item(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: &mut PublicTradesRequest,
        item: &str,
    ) {
        let leg = request.history.root().clone();
        let Ok(mut page) = request.history.admit(cx, &leg) else {
            return;
        };
        let row = match serde_json::from_str::<Row<'_>>(item) {
            Ok(row) => row,
            Err(_) => {
                request.failed = true;
                return;
            }
        };
        request.received += 1;
        let time = if row.ts.0 > 0.0 {
            row.ts.0
        } else {
            row.create_time.0
        };
        if time <= 0.0 || row.inst_id.is_empty() || row.trade_id.is_empty() {
            return;
        }
        let id = row.trade_id.to_string();
        if !page.fresh(id.clone()) {
            return;
        }
        let Some(market) = cx.store.market_by_id(&row.inst_id) else {
            return;
        };
        page.stage(Mapped::Trade(public_trade(&row, market, id, time)));
    }

    pub(crate) fn public_trades_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: PublicTradesRequest,
        response: Response<'_>,
    ) {
        let leg = request.history.root().clone();
        if request.history.validate_delivery(cx, &leg).is_err() {
            return;
        }
        if let Err(error) = check(&response) {
            request.finish(cx, Err(VenueError::new(error)));
            return;
        }
        if request.failed {
            request.finish(cx, Err(VenueError::new("Malformed BloFin history row")));
            return;
        }
        let step = {
            let Ok(mut page) = request.history.admit(cx, &leg) else {
                return;
            };
            page.advance(CursorPage {
                next: None,
                rows: request.received,
                fresh: request.received,
                want_more: false,
            })
        };
        match step {
            CursorStep::Done => request.finish(cx, Ok(())),
            CursorStep::More(_) | CursorStep::Stalled(_) => request.finish(
                cx,
                Err(VenueError::new("BloFin income pagination did not finish")),
            ),
        }
    }

    pub(crate) fn public_trades_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: PublicTradesRequest,
        error: VenueError,
    ) {
        request.finish(cx, Err(error));
    }
}
