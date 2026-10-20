//! Rare request/response reads. Results go back as JSON in the command's
//! completion, in the shapes the Binance venue returns, and never enter the
//! flush frame. Bybit pages by cursor and refuses windows wider than 7 days.
use crate::account::{order_side, order_status, order_type};
use crate::sign::Query;
use crate::wire::*;
use crate::{BybitLinear, Category, Pending, Rest, Then, venue_error};
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::{CommandId, Cx, LogLevel, Query as Read, Response};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Bybit treats both window bounds as inclusive and refuses wider ones.
const MAX_WINDOW_MS: f64 = 7.0 * 24.0 * 3_600_000.0 - 1.0;
const FILLS_PAGE_LIMIT: u32 = 100;
const ORDERS_PAGE_LIMIT: u32 = 50;
const INCOME_PAGE_LIMIT: u32 = 50;
const TRADES_LIMIT: u32 = 1000;
const MAX_PAGES_PER_WINDOW: u32 = 10;

pub struct Request {
    command: CommandId,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    /// Seconds.
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
    side: &'static str,
}

enum Rows {
    Fills(Vec<Execution>),
    Orders(Vec<HistoricalOrder>),
    Income(Vec<Income>),
    Trades(Vec<Trade>),
}

impl Rows {
    fn len(&self) -> usize {
        match self {
            Self::Fills(rows) => rows.len(),
            Self::Orders(rows) => rows.len(),
            Self::Income(rows) => rows.len(),
            Self::Trades(rows) => rows.len(),
        }
    }

    fn json(&mut self, cap: usize) -> String {
        match self {
            Self::Fills(rows) => {
                rows.sort_by(|left, right| right.timestamp.total_cmp(&left.timestamp));
                let mut seen = HashSet::new();
                rows.retain(|row| seen.insert((row.symbol.clone(), row.id.clone())));
                if cap > 0 {
                    rows.truncate(cap);
                }
                serde_json::to_string(rows)
            }
            Self::Orders(rows) => {
                rows.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
                let mut seen = HashSet::new();
                rows.retain(|row| seen.insert((row.symbol.clone(), row.id.clone())));
                if cap > 0 {
                    rows.truncate(cap);
                }
                serde_json::to_string(rows)
            }
            Self::Income(rows) => {
                rows.sort_by(|left, right| right.timestamp.total_cmp(&left.timestamp));
                let mut seen = HashSet::new();
                rows.retain(|row| {
                    seen.insert((row.symbol.clone(), row.tran_id.clone(), row.income_type))
                });
                if cap > 0 {
                    rows.truncate(cap);
                }
                serde_json::to_string(rows)
            }
            Self::Trades(rows) => serde_json::to_string(rows),
        }
        .unwrap_or_default()
    }
}

/// A cursor-paged read over one or more time windows.
struct Walk {
    path: &'static str,
    signed: bool,
    params: Vec<(&'static str, String)>,
    limit: u32,
    windows: VecDeque<(Option<f64>, Option<f64>)>,
    window: (Option<f64>, Option<f64>),
    cursor: Option<String>,
    pages: u32,
    cap: usize,
    category_rows_start: usize,
    rows: Rows,
    inverse: bool,
    categories: VecDeque<Category>,
    original_windows: VecDeque<(Option<f64>, Option<f64>)>,
}

#[derive(Default)]
pub struct HistoryState {
    reads: HashMap<CommandId, Walk>,
}

fn side_name(side: Option<OrderSide>) -> &'static str {
    match side {
        Some(OrderSide::Sell) => "sell",
        _ => "buy",
    }
}

/// `[from, to]` as windows Bybit accepts, newest first.
fn windows(from: Option<f64>, to: Option<f64>) -> VecDeque<(Option<f64>, Option<f64>)> {
    let (Some(from), Some(to)) = (from, to) else {
        return VecDeque::from([(from, to)]);
    };
    let mut windows = VecDeque::new();
    let mut end = to;
    loop {
        let start = from.max(end - MAX_WINDOW_MS);
        windows.push_back((Some(start), Some(end)));
        if start <= from {
            break;
        }
        end = start - 1.0;
    }
    windows
}

fn nonzero(value: &Option<Str<'_>>) -> Option<f64> {
    Some(float_of(value)).filter(|v| v.is_finite() && *v != 0.0)
}

/// Bybit's transaction type as an income category, and its Binance-style name.
fn income_category(kind: &str) -> &'static str {
    match kind {
        "TRADE" | "DELIVERY" => "realized_pnl",
        "SETTLEMENT" => "funding",
        "LIQUIDATION" => "liquidation",
        "TRANSFER_IN" | "TRANSFER_OUT" => "transfer",
        "FEE_REFUND" => "rebate",
        _ => "other",
    }
}

fn income_type(category: &str) -> &'static str {
    match category {
        "realized_pnl" => "REALIZED_PNL",
        "funding" => "FUNDING_FEE",
        "commission" => "COMMISSION",
        "rebate" => "REBATE",
        "transfer" => "TRANSFER",
        "liquidation" => "LIQUIDATION",
        _ => "OTHER",
    }
}

/// One transaction row carries independent cash-flow, funding and fee terms;
/// each becomes its own record with a suffixed id so none is deduped away.
fn incomes(row: &TransactionRow<'_>, out: &mut Vec<Income>) {
    let timestamp = or_zero(&row.transaction_time).trunc();
    if timestamp <= 0.0 {
        return;
    }
    let kind = present(&row.kind).unwrap_or_default();
    let category = income_category(kind);
    let symbol = present(&row.symbol).unwrap_or_default();
    let asset = present(&row.currency).unwrap_or("USDT");
    let trade_id = present(&row.trade_id);
    let base: Vec<&str> = [
        trade_id.or(present(&row.order_id)).unwrap_or_default(),
        symbol,
        kind,
        present(&row.transaction_time).unwrap_or_default(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect();
    let base = base.join("-");
    let mut push = |income: f64, category: &'static str, suffix: &str| {
        out.push(Income {
            symbol: symbol.to_owned(),
            income_type: income_type(category),
            income,
            asset: asset.to_owned(),
            timestamp,
            tran_id: format!("{base}:{suffix}"),
            trade_id: trade_id.map(str::to_owned),
            category,
        });
    };
    let (flow, funding, fee) = (
        or_zero(&row.cash_flow),
        or_zero(&row.funding),
        or_zero(&row.fee),
    );
    if flow != 0.0 {
        push(flow, category, "flow");
    }
    if funding != 0.0 {
        push(funding, "funding", "funding");
    }
    if fee != 0.0 {
        push(-fee, if fee > 0.0 { "commission" } else { "rebate" }, "fee");
    }
}

impl BybitLinear {
    pub(crate) fn start_query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, read: Read) {
        let symbol = match &read {
            Read::Fills { symbol, .. }
            | Read::OrderHistory { symbol, .. }
            | Read::Income { symbol, .. } => symbol.as_deref(),
            Read::PublicTrades { symbol, .. } | Read::OrderByClientId { symbol, .. } => {
                Some(symbol.as_str())
            }
        };
        if symbol.is_some_and(|symbol| !self.category_enabled(self.category_for(symbol))) {
            cx.done(command, Err("Bybit product is not enabled".into()));
            return;
        }
        let categories = if symbol.is_none() {
            self.categories()
        } else {
            vec![self.category]
        };
        if self.credentials.is_none()
            && !matches!(
                read,
                Read::PublicTrades { .. } | Read::OrderByClientId { .. }
            )
        {
            cx.done(command, Ok("[]".into()));
            return;
        }
        if matches!(read, Read::Income { .. }) && (self.variant.is_spot() || self.variant.is_hyro())
        {
            cx.done(
                command,
                Err("This Bybit variant does not support income history".into()),
            );
            return;
        }
        let mut walk = match read {
            Read::OrderByClientId { .. } => {
                cx.done(
                    command,
                    Err("Bybit client order lookup is not supported by the Rust engine".into()),
                );
                return;
            }
            Read::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                let cap = limit.unwrap_or(50) as usize;
                let mut params = vec![("category", self.paths.category.to_owned())];
                params.extend(symbol.map(|s| ("symbol", s)));
                params.extend(order_id.map(|id| ("orderId", id)));
                Walk {
                    inverse: self.category == Category::Inverse,
                    categories: VecDeque::new(),
                    original_windows: VecDeque::new(),
                    category_rows_start: 0,
                    path: "/v5/execution/list",
                    signed: true,
                    params,
                    limit: (cap as u32).clamp(1, FILLS_PAGE_LIMIT),
                    windows: windows(from_ms, to_ms),
                    window: (None, None),
                    cursor: None,
                    pages: 0,
                    cap,
                    rows: Rows::Fills(Vec::new()),
                }
            }
            Read::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => {
                let cap = limit.unwrap_or(50) as usize;
                let mut params = vec![("category", self.paths.category.to_owned())];
                params.extend(symbol.map(|s| ("symbol", s)));
                Walk {
                    inverse: self.category == Category::Inverse,
                    categories: VecDeque::new(),
                    original_windows: VecDeque::new(),
                    category_rows_start: 0,
                    path: "/v5/order/history",
                    signed: true,
                    params,
                    limit: (cap as u32).clamp(1, ORDERS_PAGE_LIMIT),
                    windows: windows(from_ms, to_ms),
                    window: (None, None),
                    cursor: None,
                    pages: 0,
                    cap,
                    rows: Rows::Orders(Vec::new()),
                }
            }
            Read::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                let cap = limit.unwrap_or(1000) as usize;
                // The transaction log serves unified accounts only.
                let mut params = vec![
                    ("accountType", "UNIFIED".to_owned()),
                    ("category", self.paths.category.to_owned()),
                ];
                params.extend(symbol.map(|s| ("symbol", s)));
                params.extend(kind.map(|kind| {
                    let venue = match kind.as_str() {
                        "REALIZED_PNL" | "COMMISSION" => "TRADE".to_owned(),
                        "FUNDING_FEE" => "SETTLEMENT".to_owned(),
                        _ => kind,
                    };
                    ("type", venue)
                }));
                Walk {
                    inverse: self.category == Category::Inverse,
                    categories: VecDeque::new(),
                    original_windows: VecDeque::new(),
                    category_rows_start: 0,
                    path: "/v5/account/transaction-log",
                    signed: true,
                    params,
                    limit: (cap as u32).clamp(1, INCOME_PAGE_LIMIT),
                    windows: windows(from_ms, to_ms),
                    window: (None, None),
                    cursor: None,
                    pages: 0,
                    cap,
                    rows: Rows::Income(Vec::new()),
                }
            }
            Read::PublicTrades { symbol, limit } => Walk {
                inverse: self.category == Category::Inverse,
                categories: VecDeque::new(),
                original_windows: VecDeque::new(),
                category_rows_start: 0,
                path: "/v5/market/recent-trade",
                signed: false,
                params: vec![
                    ("category", self.paths.category.to_owned()),
                    ("symbol", symbol),
                ],
                limit: limit.unwrap_or(500).clamp(
                    1,
                    if self.variant.is_spot() {
                        60
                    } else {
                        TRADES_LIMIT
                    },
                ),
                windows: VecDeque::from([(None, None)]),
                window: (None, None),
                cursor: None,
                pages: 0,
                cap: 0,
                rows: Rows::Trades(Vec::new()),
            },
        };
        walk.original_windows = walk.windows.clone();
        walk.categories = categories
            .into_iter()
            .filter(|category| *category != self.category && *category != Category::Spot)
            .collect();
        self.history.reads.insert(command, walk);
        self.next_window(cx, command);
    }

    fn next_window(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(walk) = self.history.reads.get_mut(&command) else {
            return;
        };
        let full = walk.cap > 0 && walk.rows.len() - walk.category_rows_start >= walk.cap;
        match walk.windows.pop_front().filter(|_| !full) {
            Some(window) => {
                walk.window = window;
                walk.cursor = None;
                walk.pages = 0;
                self.request_page(cx, command);
            }
            None => {
                if let Some(category) = walk.categories.pop_front() {
                    if let Some((_, value)) =
                        walk.params.iter_mut().find(|(key, _)| *key == "category")
                    {
                        *value = category.as_str().into();
                    }
                    walk.inverse = category == Category::Inverse;
                    walk.category_rows_start = walk.rows.len();
                    walk.windows = walk.original_windows.clone();
                    self.select_category(category);
                    self.next_window(cx, command);
                    return;
                }
                let mut walk = self.history.reads.remove(&command).expect("present");
                cx.done(command, Ok(walk.rows.json(walk.cap)));
            }
        }
    }

    fn request_page(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(walk) = self.history.reads.get(&command) else {
            return;
        };
        let query = walk
            .params
            .iter()
            .fold(Query::new(), |query, (name, value)| query.str(name, value))
            .opt_num("startTime", walk.window.0)
            .opt_num("endTime", walk.window.1)
            .num("limit", f64::from(walk.limit))
            .opt_str("cursor", walk.cursor.as_deref());
        let rest = Rest::get(walk.path, query);
        let rest = if walk.signed { rest.signed() } else { rest };
        self.send(cx, rest, Then::History(Request { command }));
    }

    pub(crate) fn on_history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let command = request.command;
        let body = match &response {
            Ok(response) => match venue_error(response) {
                None => Ok(response.body),
                Some((_, message)) => Err(message),
            },
            Err(error) => Err((*error).to_owned()),
        };
        let Some(walk) = self.history.reads.get_mut(&command) else {
            return;
        };
        let parsed = body.and_then(|body| page(walk, body).map_err(|e| e.to_string()));
        let cursor = match parsed {
            Ok(cursor) => cursor,
            Err(message) => {
                cx.log(
                    LogLevel::Error,
                    format!("Bybit history ({}): {message}", walk.path),
                );
                self.history.reads.remove(&command);
                cx.done(command, Err(message.into()));
                return;
            }
        };
        walk.pages += 1;
        let full = walk.cap > 0 && walk.rows.len() - walk.category_rows_start >= walk.cap;
        if !full
            && cursor.as_ref().is_some_and(|cursor| {
                walk.cursor.as_ref() == Some(cursor) || walk.pages >= MAX_PAGES_PER_WINDOW
            })
        {
            self.history.reads.remove(&command);
            cx.done(
                command,
                Err("Bybit history pagination is incomplete or did not advance".into()),
            );
            return;
        }
        match cursor.filter(|_| !full) {
            Some(cursor) => {
                walk.cursor = Some(cursor);
                self.request_page(cx, command);
            }
            None => {
                walk.pages = 0;
                self.next_window(cx, command);
            }
        }
    }
}

/// Map one page into the walk's rows; returns the next cursor, if any.
fn page(walk: &mut Walk, body: &str) -> serde_json::Result<Option<String>> {
    fn listed<'a, T: serde::Deserialize<'a>>(
        body: &'a str,
    ) -> serde_json::Result<(Vec<T>, Option<String>)> {
        let listed = serde_json::from_str::<Listed<T>>(body)?;
        let _ = listed.ret_code;
        Ok(listed
            .result
            .map(|r| (r.list, r.next_page_cursor.filter(|c| !c.is_empty())))
            .unwrap_or_default())
    }
    match &mut walk.rows {
        Rows::Fills(rows) => {
            let (list, cursor) = listed::<ExecutionRow>(body)?;
            for e in list {
                let time = or_zero(&e.exec_time);
                rows.push(Execution {
                    id: e.exec_id.as_deref().unwrap_or_default().to_owned(),
                    timestamp: if time > 100_000_000_000.0 {
                        (time / 1000.0).floor()
                    } else {
                        time.floor()
                    },
                    symbol: e.symbol.into_owned(),
                    order_id: e.order_id.into_owned(),
                    price: or_zero(&e.exec_price),
                    qty: or_zero(&e.exec_qty),
                    side: side_name(order_side(e.side.as_deref())),
                    commission: or_zero(&e.exec_fee).abs(),
                    commission_asset: present(&e.fee_currency).map(str::to_owned),
                    maker: e.is_maker,
                });
            }
            Ok(cursor)
        }
        Rows::Orders(rows) => {
            let (list, cursor) = listed::<OrderRow>(body)?;
            for o in list {
                let created = or_zero(&o.created_time).trunc();
                let Some(side) = order_side(o.side.as_deref()) else {
                    continue;
                };
                if created == 0.0 {
                    continue;
                }
                let kind = present(&o.stop_order_type)
                    .and_then(order_type)
                    .or_else(|| present(&o.order_type).and_then(order_type))
                    .unwrap_or(OrderType::Limit);
                let updated = or_zero(&o.updated_time).trunc();
                rows.push(HistoricalOrder {
                    id: o.order_id.into_owned(),
                    symbol: o.symbol.into_owned(),
                    side: side_name(Some(side)),
                    kind,
                    // A historical order is no longer working whatever its spelling.
                    status: order_status(&o.order_status).unwrap_or(OrderStatus::Closed),
                    price: nonzero(&o.price),
                    average_price: nonzero(&o.avg_price),
                    amount: or_zero(&o.qty),
                    filled: or_zero(&o.cum_exec_qty),
                    reduce_only: o.reduce_only,
                    position_side: match o.position_idx {
                        Some(0) => Some("both"),
                        Some(1) => Some("long"),
                        Some(2) => Some("short"),
                        _ => None,
                    },
                    created_time: created,
                    updated_time: (updated > 0.0).then_some(updated),
                });
            }
            Ok(cursor)
        }
        Rows::Income(rows) => {
            let (list, cursor) = listed::<TransactionRow>(body)?;
            if list.is_empty() {
                return Ok(None);
            }
            for row in &list {
                incomes(row, rows);
            }
            Ok(cursor)
        }
        Rows::Trades(rows) => {
            let (list, _) = listed::<RecentTrade>(body)?;
            let symbol = walk
                .params
                .iter()
                .find(|(name, _)| *name == "symbol")
                .map(|(_, s)| s.clone())
                .unwrap_or_default();
            for t in list {
                let price = or_zero(&t.price);
                let qty = or_zero(&t.size);
                rows.push(Trade {
                    id: t.exec_id.as_deref().unwrap_or_default().to_owned(),
                    symbol: symbol.clone(),
                    price,
                    qty,
                    quote_qty: if walk.inverse { qty } else { price * qty },
                    timestamp: or_zero(&t.time),
                    side: side_name(order_side(t.side.as_deref())),
                });
            }
            Ok(None)
        }
    }
}
