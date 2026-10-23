//! Rare reads return Binance-compatible JSON without touching the store.
use crate::sign::{Credentials, Params};
use crate::{Pending, wire};
use safe_cex_engine::de::optional_number;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::time;
use safe_cex_engine::{Cx, Method, Query, Response, TimerId};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

const WEEK: f64 = 7.0 * 24.0 * 3_600_000.0;
const PAGE_DELAY: f64 = 200.0;
const MAX_FILL_PAGES: u32 = 1000;

pub struct Request {
    command: u32,
    window: (f64, f64),
    rows: Vec<Row>,
    error: Option<String>,
}

#[derive(Clone, Copy)]
enum Kind {
    Fills,
    Orders,
    Income,
    Trades,
}

struct Read {
    kind: Kind,
    symbol: Option<String>,
    order_id: Option<String>,
    income_type: Option<String>,
    limit: u32,
    from: Option<f64>,
    to: Option<f64>,
    walk: Option<RangeWalk>,
    fills: Option<FillPages>,
    rows: Vec<Row>,
    timer: Option<TimerId>,
}

struct FillPages {
    from: f64,
    to: f64,
    end: f64,
    page: u32,
    seen: HashSet<String>,
}

#[derive(Default)]
pub struct History {
    reads: HashMap<u32, Read>,
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
    fn timestamp(&self) -> f64 {
        match self {
            Self::Fill(row) => row.raw_time,
            Self::Order(row) => row.created_time,
            Self::Income(row) => row.timestamp,
            Self::Trade(row) => row.timestamp,
        }
    }

    fn identity(&self) -> String {
        match self {
            Self::Fill(row) => row.id.clone(),
            Self::Order(row) => row.id.clone(),
            Self::Income(row) => format!(
                "{}|{}|{}|{}",
                row.tran_id, row.income_type, row.income, row.timestamp
            ),
            Self::Trade(row) => row.id.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    #[serde(skip)]
    raw_time: f64,
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
    position_side: Option<&'static str>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
    created_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_time: Option<f64>,
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
struct Envelope<T> {
    code: i64,
    #[serde(default)]
    msg: String,
    data: Option<T>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FillData {
    Direct(Vec<FillRow>),
    History { fill_history_orders: Vec<FillRow> },
    Orders { fill_orders: Vec<FillRow> },
}

impl FillData {
    fn is_empty(&self) -> bool {
        match self {
            Self::Direct(rows)
            | Self::History {
                fill_history_orders: rows,
            }
            | Self::Orders { fill_orders: rows } => rows.is_empty(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillRow {
    #[serde(
        default,
        alias = "fillId",
        alias = "id",
        deserialize_with = "identifier"
    )]
    trade_id: String,
    #[serde(alias = "order_id", deserialize_with = "identifier")]
    order_id: String,
    #[serde(default)]
    symbol: String,
    side: String,
    #[serde(alias = "filledPrice", deserialize_with = "wire::number")]
    price: f64,
    #[serde(
        alias = "volume",
        alias = "filledQty",
        deserialize_with = "wire::number"
    )]
    qty: f64,
    #[serde(alias = "filledTime", alias = "filledTm", deserialize_with = "text")]
    time: String,
    #[serde(default, alias = "fee", deserialize_with = "wire::number")]
    commission: f64,
    #[serde(default, alias = "currency")]
    commission_asset: Option<String>,
    #[serde(default)]
    position_side: Option<String>,
    #[serde(default, alias = "realizedPNL", deserialize_with = "optional_number")]
    realized_pnl: Option<f64>,
    #[serde(default)]
    maker: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderRow {
    #[serde(deserialize_with = "identifier")]
    order_id: String,
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    kind: String,
    status: String,
    #[serde(deserialize_with = "wire::number")]
    price: f64,
    #[serde(default, deserialize_with = "wire::number")]
    avg_price: f64,
    #[serde(deserialize_with = "wire::number")]
    orig_qty: f64,
    #[serde(deserialize_with = "wire::number")]
    executed_qty: f64,
    #[serde(default, deserialize_with = "crate::account_wire::boolean")]
    reduce_only: bool,
    #[serde(default)]
    position_side: Option<String>,
    #[serde(deserialize_with = "wire::number")]
    time: f64,
    #[serde(default, deserialize_with = "wire::number")]
    update_time: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeRow {
    #[serde(default)]
    symbol: String,
    income_type: String,
    #[serde(deserialize_with = "wire::number")]
    income: f64,
    asset: String,
    #[serde(deserialize_with = "wire::number")]
    time: f64,
    #[serde(deserialize_with = "identifier")]
    tran_id: String,
    #[serde(default, deserialize_with = "optional_text")]
    trade_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeRow {
    #[serde(deserialize_with = "identifier")]
    id: String,
    #[serde(deserialize_with = "wire::number")]
    price: f64,
    #[serde(deserialize_with = "wire::number")]
    qty: f64,
    #[serde(deserialize_with = "wire::number")]
    quote_qty: f64,
    #[serde(deserialize_with = "wire::number")]
    time: f64,
    buyer_maker: bool,
}

fn local_result(cx: &mut Cx<'_, Pending>, result: Result<&str, &str>) {
    let Ok(context) = cx.admit_http_command(Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    match result {
        Ok(value) => {
            let _ = cx.done_context(&context, value.into());
        }
        Err(error) => {
            let _ = cx.fail_context(&context, error);
        }
    }
}

impl History {
    pub fn query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        command: u32,
        query: Query,
    ) {
        if credentials.is_none()
            && !matches!(
                query,
                Query::PublicTrades { .. } | Query::OrderByClientId { .. }
            )
        {
            local_result(cx, Ok("[]"));
            return;
        }
        let mut read = Read {
            kind: Kind::Trades,
            symbol: None,
            order_id: None,
            income_type: None,
            limit: 1000,
            from: None,
            to: None,
            walk: None,
            fills: None,
            rows: Vec::new(),
            timer: None,
        };
        match query {
            Query::OrderByClientId { .. } => {
                local_result(
                    cx,
                    Err("BingX client order lookup is not supported by the Rust engine"),
                );
                return;
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                let Some(symbol) = symbol else {
                    local_result(cx, Ok("[]"));
                    return;
                };
                read.kind = Kind::Fills;
                read.symbol = Some(symbol);
                read.order_id = order_id;
                read.limit = limit.unwrap_or(100).clamp(1, 1000);
                // This endpoint requires both timestamps, unlike other rare reads.
                read.to = Some(to_ms.unwrap_or(cx.now));
                read.from = Some(from_ms.unwrap_or(read.to.unwrap_or(cx.now) - WEEK));
            }
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => {
                read.kind = Kind::Orders;
                read.symbol = symbol;
                read.from = from_ms;
                read.to = to_ms;
                read.limit = limit.unwrap_or(500).clamp(1, 1000);
            }
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                read.kind = Kind::Income;
                read.symbol = symbol;
                read.income_type = kind;
                read.from = from_ms;
                read.to = to_ms;
                read.limit = limit.unwrap_or(100).clamp(1, 1000);
            }
            Query::PublicTrades { symbol, limit } => {
                read.symbol = Some(symbol);
                read.limit = limit.unwrap_or(500).clamp(1, 1000);
            }
        }
        if !matches!(read.kind, Kind::Trades) {
            if read.from.is_some() && read.to.is_none() {
                read.to = Some(cx.now);
            }
            if read.to.is_some() && read.from.is_none() {
                read.from = read.to.map(|to| to - WEEK);
            }
        }
        if let (Some(from), Some(to)) = (read.from, read.to) {
            if !from.is_finite() || !to.is_finite() || from > to {
                local_result(cx, Err("invalid history time range"));
                return;
            }
            if matches!(read.kind, Kind::Fills) {
                read.fills = Some(FillPages {
                    from,
                    to: (from + WEEK).min(to),
                    end: to,
                    page: 1,
                    seen: HashSet::new(),
                });
            } else if from < to {
                read.walk = Some(RangeWalk::new(from, to, WEEK, read.limit as usize));
            }
        }
        self.reads.insert(command, read);
        self.next(cx, credentials, command);
    }

    fn next(&mut self, cx: &mut Cx<'_, Pending>, credentials: Option<&Credentials>, command: u32) {
        let Some(read) = self.reads.get_mut(&command) else {
            return;
        };
        let (from, to) = if let Some(fills) = &read.fills {
            (Some(fills.from), Some(fills.to))
        } else if let Some(walk) = &mut read.walk {
            loop {
                match walk.step() {
                    Step::Fetch { from, to } => break (Some(from), Some(to)),
                    Step::ChunkDone => continue,
                    Step::Finished => {
                        let truncated = walk.truncated;
                        self.finish(cx, command, truncated);
                        return;
                    }
                }
            }
        } else {
            (read.from, read.to)
        };
        let mut params = Params::new();
        if let Some(symbol) = &read.symbol {
            params = params.str("symbol", &native_symbol(symbol));
        }
        if let Some(order) = &read.order_id {
            params = params.str("orderId", order);
        }
        if let Some(kind) = &read.income_type {
            params = params.str("incomeType", kind);
        }
        let (path, start, end, limit) = match read.kind {
            Kind::Fills => (
                "/openApi/swap/v2/trade/fillHistory",
                "startTs",
                "endTs",
                "pageSize",
            ),
            Kind::Orders => (
                "/openApi/swap/v2/trade/allOrders",
                "startTime",
                "endTime",
                "limit",
            ),
            Kind::Income => (
                "/openApi/swap/v2/user/income",
                "startTime",
                "endTime",
                "limit",
            ),
            Kind::Trades => (
                "/openApi/swap/v2/quote/trades",
                "startTime",
                "endTime",
                "limit",
            ),
        };
        params = params.num(limit, f64::from(read.limit));
        if let Some(fills) = &read.fills {
            params = params.num("pageIndex", f64::from(fills.page));
        }
        if let Some(from) = from {
            params = params.num(start, from);
        }
        if let Some(to) = to {
            params = params.num(end, to);
        }
        let auth = if matches!(read.kind, Kind::Trades) {
            None
        } else {
            credentials
        };
        let request = Pending::History(Request {
            command,
            window: (from.unwrap_or(0.0), to.unwrap_or(0.0)),
            rows: Vec::new(),
            error: None,
        });
        let at = if matches!(read.kind, Kind::Fills) {
            ArrayAt::EnvelopeFieldOrNestedAny("data", &["fill_orders", "fill_history_orders"])
        } else {
            ArrayAt::EnvelopeField("data")
        };
        crate::issue(cx, auth, Method::Get, path, params, Some(at), request);
    }

    pub fn on_item(&mut self, _cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        let Some(read) = self.reads.get(&request.command) else {
            return;
        };
        let row = match read.kind {
            Kind::Orders => serde_json::from_str::<OrderRow>(item)
                .map_err(|e| e.to_string())
                .and_then(map_order),
            Kind::Income => serde_json::from_str::<IncomeRow>(item)
                .map(|r| {
                    Row::Income(Income {
                        symbol: symbol(&r.symbol),
                        category: income_category(&r.income_type),
                        income_type: r.income_type,
                        income: r.income,
                        asset: r.asset,
                        timestamp: r.time,
                        tran_id: r.tran_id,
                        trade_id: r.trade_id.filter(|s| !s.is_empty()),
                    })
                })
                .map_err(|e| e.to_string()),
            Kind::Trades => serde_json::from_str::<TradeRow>(item)
                .map(|r| {
                    Row::Trade(Trade {
                        id: r.id,
                        symbol: symbol(read.symbol.as_deref().unwrap_or_default()),
                        price: r.price,
                        qty: r.qty,
                        quote_qty: r.quote_qty,
                        timestamp: r.time,
                        side: if r.buyer_maker { "sell" } else { "buy" },
                    })
                })
                .map_err(|e| e.to_string()),
            Kind::Fills => serde_json::from_str::<FillRow>(item)
                .map_err(|e| e.to_string())
                .and_then(|row| map_fill(row, read.symbol.as_deref().unwrap_or_default())),
        };
        match row {
            Ok(row) => request.rows.push(row),
            Err(error) => request.error = Some(error),
        }
    }

    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _credentials: Option<&Credentials>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.on_failed(
                cx,
                request,
                &format!("history HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        let Some(read) = self.reads.get_mut(&request.command) else {
            return;
        };
        if matches!(read.kind, Kind::Fills) {
            let result = serde_json::from_str::<Envelope<FillData>>(response.body)
                .map_err(|e| e.to_string())
                .and_then(|response| {
                    if response.code != 0 {
                        return Err(format!("{}: {}", response.code, response.msg));
                    }
                    response.data.ok_or_else(|| "missing fill data".to_owned())
                })
                .and_then(|data| {
                    if data.is_empty() {
                        Ok(())
                    } else {
                        Err("streamed fill metadata retained unexpected rows".into())
                    }
                });
            if let Err(error) = result {
                request.error = Some(error);
            }
        } else {
            let result = serde_json::from_str::<Envelope<de::IgnoredAny>>(response.body)
                .map_err(|e| e.to_string())
                .and_then(|response| {
                    if response.code != 0 {
                        return Err(format!("{}: {}", response.code, response.msg));
                    }
                    response
                        .data
                        .ok_or_else(|| "missing history data".to_owned())
                });
            if let Err(error) = result {
                request.error = Some(error);
            }
        }
        if let Some(error) = request.error.take() {
            self.on_failed(cx, request, &error);
            return;
        }
        if let Some(fills) = &mut read.fills {
            let full = request.rows.len() >= read.limit as usize;
            if request
                .rows
                .iter()
                .any(|row| row.timestamp() < fills.from || row.timestamp() > fills.to)
            {
                self.on_failed(
                    cx,
                    request,
                    "fill history returned executions outside its requested window",
                );
                return;
            }
            let before = read.rows.len();
            read.rows.extend(
                request
                    .rows
                    .drain(..)
                    .filter(|row| fills.seen.insert(row.identity())),
            );
            if full {
                if read.rows.len() == before || fills.page >= MAX_FILL_PAGES {
                    self.on_failed(cx, request, "fill history is incomplete: native pagination made no progress or reached its page budget");
                    return;
                }
                fills.page += 1;
            } else if fills.to < fills.end {
                let next = fills.to + 1.0;
                if next <= fills.from {
                    self.on_failed(cx, request, "fill history time range cannot advance");
                    return;
                }
                fills.from = next;
                fills.to = (next + WEEK).min(fills.end);
                fills.page = 1;
            } else {
                self.finish(cx, request.command, false);
                return;
            }
            read.timer = Some(cx.set_timer(PAGE_DELAY));
        } else if let Some(walk) = &mut read.walk {
            let page: Vec<PageRow> = request
                .rows
                .iter()
                .map(|row| PageRow {
                    timestamp: row.timestamp(),
                    identity: row.identity(),
                })
                .collect();
            let fresh = walk.page(request.window.0, request.window.1, &page);
            let mut rows: Vec<_> = request.rows.into_iter().map(Some).collect();
            read.rows
                .extend(fresh.into_iter().filter_map(|index| rows[index].take()));
            read.timer = Some(cx.set_timer(PAGE_DELAY));
        } else {
            // With only a time filter, a full page at one millisecond cannot
            // prove completeness. Fills use their native page cursor instead.
            let saturated = read.from.is_some()
                && read.from == read.to
                && request.rows.len() >= read.limit as usize;
            read.rows.extend(request.rows);
            self.finish(cx, request.command, saturated);
        }
    }

    fn finish(&mut self, cx: &mut Cx<'_, Pending>, command: u32, truncated: bool) {
        let Some(mut read) = self.reads.remove(&command) else {
            return;
        };
        if truncated {
            cx.fail(
                command,
                "history is incomplete: time window could not be fully paged",
            );
            return;
        }
        if !matches!(read.kind, Kind::Trades) {
            read.rows
                .sort_by(|a, b| b.timestamp().total_cmp(&a.timestamp()));
        }
        match serde_json::to_string(&read.rows).map_err(|e| e.to_string()) {
            Ok(data) => cx.done(command, data),
            Err(error) => cx.fail(command, error),
        };
    }

    pub fn on_timer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        timer: TimerId,
    ) {
        let command = self.reads.iter_mut().find_map(|(command, read)| {
            if read.timer == Some(timer) {
                read.timer = None;
                Some(*command)
            } else {
                None
            }
        });
        if let Some(command) = command {
            self.next(cx, credentials, command);
        }
    }

    pub fn on_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: safe_cex_engine::VenueError,
    ) {
        if let Some(timer) = self.reads.get(&request.command).and_then(|read| read.timer) {
            cx.clear_timer(timer);
        }
        self.fail_read(cx, request, &error);
    }

    pub fn on_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        self.fail_read(cx, request, &error.into());
    }

    /// Native and signer failures share one command completion.
    fn fail_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &safe_cex_engine::VenueError,
    ) {
        self.reads.remove(&request.command);
        cx.fail(request.command, error.to_owned());
    }
}

fn map_fill(row: FillRow, fallback_symbol: &str) -> Result<Row, String> {
    if row.trade_id.trim().is_empty() {
        return Err("missing native fill identifier".into());
    }
    let timestamp = parse_time(&row.time).ok_or_else(|| "invalid fill timestamp".to_owned())?;
    Ok(Row::Fill(Execution {
        raw_time: timestamp,
        id: row.trade_id,
        timestamp: (timestamp / 1000.0).floor(),
        symbol: if row.symbol.is_empty() {
            fallback_symbol.to_owned()
        } else {
            symbol(&row.symbol)
        },
        order_id: row.order_id,
        price: row.price,
        qty: row.qty,
        side: side(&row.side)?,
        commission: row.commission.abs(),
        commission_asset: row.commission_asset.unwrap_or_else(|| "USDT".into()),
        position_side: hedge_side(row.position_side.as_deref()),
        realized_pnl: row.realized_pnl,
        maker: row.maker,
    }))
}

fn map_order(row: OrderRow) -> Result<Row, String> {
    let kind = match row.kind.as_str() {
        "MARKET" => OrderType::Market,
        "LIMIT" => OrderType::Limit,
        "STOP" | "TRIGGER_LIMIT" | "STOP_LIMIT" | "TAKE_PROFIT" | "TAKE_PROFIT_LIMIT" => {
            OrderType::Conditional
        }
        "STOP_MARKET" | "TRIGGER_MARKET" => OrderType::StopLoss,
        "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        "TRAILING_STOP_MARKET" | "TRAILING_TP_SL" => OrderType::TrailingStopLoss,
        _ => return Err(format!("unknown order type {}", row.kind)),
    };
    let status = match row.status.as_str() {
        "NEW" | "OPEN" | "WORKING" | "PENDING" | "PARTIALLY_FILLED" => OrderStatus::Open,
        "FILLED" => OrderStatus::Closed,
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
        _ => return Err(format!("unknown order status {}", row.status)),
    };
    Ok(Row::Order(HistoricalOrder {
        id: row.order_id,
        symbol: symbol(&row.symbol),
        side: side(&row.side)?,
        kind,
        status,
        price: (row.price != 0.0).then_some(row.price),
        average_price: (row.avg_price != 0.0).then_some(row.avg_price),
        amount: row.orig_qty,
        filled: row.executed_qty,
        reduce_only: row.reduce_only,
        position_side: hedge_side(row.position_side.as_deref()),
        created_time: row.time,
        updated_time: (row.update_time != 0.0).then_some(row.update_time),
    }))
}

fn side(value: &str) -> Result<&'static str, String> {
    match value.to_ascii_uppercase().as_str() {
        "BUY" => Ok("buy"),
        "SELL" => Ok("sell"),
        _ => Err(format!("invalid side {value}")),
    }
}

fn hedge_side(value: Option<&str>) -> Option<&'static str> {
    match value? {
        "LONG" | "long" => Some("long"),
        "SHORT" | "short" => Some("short"),
        "BOTH" | "both" => Some("both"),
        _ => None,
    }
}

fn income_category(value: &str) -> &'static str {
    match value {
        "REALIZED_PNL" => "realized_pnl",
        "FUNDING_FEE" => "funding",
        "TRADING_FEE" | "COMMISSION" => "commission",
        "TRANSFER" => "transfer",
        "INSURANCE_CLEAR" | "ADL" => "liquidation",
        _ => "other",
    }
}

fn symbol(value: &str) -> String {
    value.replace('-', "")
}

fn native_symbol(value: &str) -> String {
    if value.contains('-') {
        return value.to_owned();
    }
    for quote in ["USDT", "USDC"] {
        if let Some(base) = value.strip_suffix(quote) {
            return format!("{base}-{quote}");
        }
    }
    value.to_owned()
}

fn text<'de, D: Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    struct Text;
    impl Visitor<'_> for Text {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string or integer")
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
            Ok(value.to_owned())
        }
        fn visit_u64<E: de::Error>(self, value: u64) -> Result<String, E> {
            Ok(value.to_string())
        }
        fn visit_i64<E: de::Error>(self, value: i64) -> Result<String, E> {
            Ok(value.to_string())
        }
    }
    de.deserialize_any(Text)
}

fn identifier<'de, D: Deserializer<'de>>(de: D) -> Result<String, D::Error> {
    let value = text(de)?;
    if value.trim().is_empty() {
        return Err(de::Error::custom("empty native identifier"));
    }
    Ok(value)
}

fn optional_text<'de, D: Deserializer<'de>>(de: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    struct Text(#[serde(deserialize_with = "text")] String);
    Option::<Text>::deserialize(de).map(|value| value.map(|value| value.0))
}

/// RFC3339 offsets are part of the instant; never consult the host timezone.
fn parse_time(value: &str) -> Option<f64> {
    if !value.contains('T') {
        return value.parse::<f64>().ok().filter(|v| v.is_finite());
    }
    let (date, time) = value.split_once('T')?;
    let mut date = date.split('-');
    let year = date.next()?.parse::<i32>().ok()?;
    if !(0..=9999).contains(&year) {
        return None;
    }
    let month = date.next()?.parse::<u32>().ok()?;
    let day = date.next()?.parse::<u32>().ok()?;
    if date.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    let last = time::days_in_month(i64::from(year), i64::from(month))?;
    if day == 0 || i64::from(day) > last {
        return None;
    }
    let (clock, offset) = if let Some(clock) = time.strip_suffix('Z') {
        (clock, 0.0)
    } else {
        let at = time.find(['+', '-'])?;
        let (clock, offset) = time.split_at(at);
        let mut parts = offset[1..].split(':');
        let hours = parts.next()?.parse::<u32>().ok()?;
        let minutes = parts.next()?.parse::<u32>().ok()?;
        if parts.next().is_some() || hours > 23 || minutes > 59 {
            return None;
        }
        let seconds = f64::from(hours * 3600 + minutes * 60);
        (
            clock,
            if offset.starts_with('-') {
                -seconds
            } else {
                seconds
            },
        )
    };
    let mut clock = clock.split(':');
    let hour = clock.next()?.parse::<u32>().ok()?;
    let minute = clock.next()?.parse::<u32>().ok()?;
    let second = clock.next()?.parse::<f64>().ok()?;
    if clock.next().is_some() || hour > 23 || minute > 59 || !(0.0..60.0).contains(&second) {
        return None;
    }
    let days = time::days_from_civil(i64::from(year), i64::from(month), i64::from(day));
    Some((days as f64 * 86400.0 + f64::from(hour * 3600 + minute * 60) + second - offset) * 1000.0)
}
