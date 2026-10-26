//! Rare request/response reads. Results go back as JSON in the command's
//! completion, in the shapes the Binance venue returns, and never enter the
//! flush frame. Bybit pages by cursor and refuses windows wider than 7 days.
use crate::account::{order_side, order_status, order_type};
use crate::sign::Query;
use crate::wire::*;
use crate::{BybitLinear, Category, Pending, Rest, Then};
use safe_cex_engine::history::{CursorAdmission, CursorHistory, ReadCompletion, ReadCursorHistory};
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::paging::{self, CursorPage, CursorStep, CursorWalk, Stall, WindowOrder};
use safe_cex_engine::request_context::{Context, Owner};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Query as Read, Response, VenueError};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Bybit treats both window bounds as inclusive and refuses wider ones.
const MAX_WINDOW_MS: f64 = 7.0 * 24.0 * 3_600_000.0 - 1.0;
/// Windows a range may span; a larger range keeps one window for Bybit to reject.
const MAX_WINDOWS: usize = 10_000;
const FILLS_PAGE_LIMIT: u32 = 100;
const ORDERS_PAGE_LIMIT: u32 = 50;
const INCOME_PAGE_LIMIT: u32 = 50;
const TRADES_LIMIT: u32 = 1000;
const MAX_PAGES_PER_WINDOW: u32 = 10;
/// TS walks up to 20 order-history pages of 50.
const MAX_ORDER_PAGES_PER_WINDOW: u32 = 20;

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
    /// Native row identity: equal for page repeats, distinct for hedge legs.
    #[serde(skip)]
    source: String,
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

enum Row {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
    Trade(Trade),
}

#[derive(Clone, Copy)]
enum RowKind {
    Fills,
    Orders,
    Income,
    Trades,
}

impl RowKind {
    fn empty(self) -> Rows {
        match self {
            Self::Fills => Rows::Fills(Vec::new()),
            Self::Orders => Rows::Orders(Vec::new()),
            Self::Income => Rows::Income(Vec::new()),
            Self::Trades => Rows::Trades(Vec::new()),
        }
    }

    fn owner(self, completion: ReadCompletion) -> ReadCursorHistory<Row> {
        let max_pages = if matches!(self, Self::Orders) {
            MAX_ORDER_PAGES_PER_WINDOW
        } else {
            MAX_PAGES_PER_WINDOW
        };
        CursorHistory::new(
            completion,
            CursorWalk::new(max_pages),
            |reason| {
                match reason {
                Stall::PageBudget => "Bybit order history reached its page limit; older orders in this window are omitted".into(),
                _ => "Bybit history pagination is incomplete or did not advance".into(),
            }
            },
            |_| Err("Bybit history requires query finalization".into()),
        )
    }

    fn encode(self, rows: Vec<Row>, cap: usize) -> Result<String, VenueError> {
        let mut shaped = self.empty();
        for row in rows {
            match (&mut shaped, row) {
                (Rows::Fills(rows), Row::Fill(row)) => rows.push(row),
                (Rows::Orders(rows), Row::Order(row)) => rows.push(row),
                (Rows::Income(rows), Row::Income(row)) => rows.push(row),
                (Rows::Trades(rows), Row::Trade(row)) => rows.push(row),
                _ => {
                    return Err(
                        VenueError::new("Bybit history row kind changed").kind(ErrorKind::Internal)
                    );
                }
            }
        }
        shaped.json(cap)
    }
}

enum Rows {
    Fills(Vec<Execution>),
    Orders(Vec<HistoricalOrder>),
    Income(Vec<Income>),
    Trades(Vec<Trade>),
}

impl Rows {
    fn stage(self, owner: &mut CursorAdmission<'_, '_, Row, Pending>) {
        match self {
            Self::Fills(rows) => rows.into_iter().for_each(|row| owner.stage(Row::Fill(row))),
            Self::Orders(rows) => rows
                .into_iter()
                .for_each(|row| owner.stage(Row::Order(row))),
            Self::Income(rows) => rows
                .into_iter()
                .for_each(|row| owner.stage(Row::Income(row))),
            Self::Trades(rows) => rows
                .into_iter()
                .for_each(|row| owner.stage(Row::Trade(row))),
        }
    }

    fn json(&mut self, cap: usize) -> Result<String, VenueError> {
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
                // Exact page repeats collapse. Hedge legs share TS's id (as TS emits
                // them) but differ in `source`, so both survive.
                let mut seen = HashSet::new();
                rows.retain(|row| {
                    seen.insert((
                        row.symbol.clone(),
                        row.tran_id.clone(),
                        row.income_type,
                        row.source.clone(),
                    ))
                });
                if cap > 0 {
                    rows.truncate(cap);
                }
                serde_json::to_string(rows)
            }
            Self::Trades(rows) => serde_json::to_string(rows),
        }
        .map_err(|error| error.to_string().into())
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
    stage_started: bool,
    cap: usize,
    category_rows_start: usize,
    kind: RowKind,
    history: ReadCursorHistory<Row>,
    inverse: bool,
    categories: VecDeque<Category>,
    original_windows: VecDeque<(Option<f64>, Option<f64>)>,
}

impl Walk {
    fn full(&self) -> bool {
        self.cap > 0
            && self.history.accepted_len() + self.history.staged_len() - self.category_rows_start
                >= self.cap
    }

    fn finish(self, cx: &mut Cx<'_, Pending>, result: Result<(), VenueError>) {
        let _ = self
            .history
            .finish_with(cx, result, |rows| self.kind.encode(rows, self.cap));
    }
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
pub(crate) fn windows(from: Option<f64>, to: Option<f64>) -> VecDeque<(Option<f64>, Option<f64>)> {
    let (Some(from), Some(to)) = (from, to) else {
        return VecDeque::from([(from, to)]);
    };
    paging::windows(
        from,
        to,
        MAX_WINDOW_MS,
        WindowOrder::NewestFirst,
        MAX_WINDOWS,
    )
    .map(|windows| {
        windows
            .into_iter()
            .map(|(a, b)| (Some(a), Some(b)))
            .collect()
    })
    // An inverted or too-large range keeps one window for Bybit to reject.
    .unwrap_or_else(|| VecDeque::from([(Some(from), Some(to))]))
}

fn nontrade_execution(kind: &str) -> bool {
    [
        "funding",
        "settle",
        "settlement",
        "delivery",
        "sessionsettlepnl",
    ]
    .iter()
    .any(|name| kind.trim().eq_ignore_ascii_case(name))
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
    // Exactly TS's id, so pre/post-cutover rows key identically; same-instant
    // hedge legs share it there too; `source` tells them from page repeats.
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
    let source = [
        present(&row.id).unwrap_or_default(),
        present(&row.side).unwrap_or_default(),
        present(&row.size).unwrap_or_default(),
    ]
    .join("|");
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
            source: source.clone(),
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
        // Every page is a leg of this caller root; its deadline bounds the walk.
        let signed = !matches!(read, Read::PublicTrades { .. });
        let Some(root) = self.caller_read_root(cx, signed) else {
            return;
        };
        let completion = match ReadCompletion::from_context(cx, command, root.clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(&root, error.venue_error());
                return;
            }
        };
        let symbol = match &read {
            Read::Fills { symbol, .. }
            | Read::OrderHistory { symbol, .. }
            | Read::Income { symbol, .. } => symbol.as_deref(),
            Read::PublicTrades { symbol, .. } | Read::OrderByClientId { symbol, .. } => {
                Some(symbol.as_str())
            }
        };
        if symbol.is_some_and(|symbol| !self.category_enabled(self.category_for(symbol))) {
            let _ = cx.fail_context(&root, VenueError::new("Bybit product is not enabled"));
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
            let _ = cx.done_context(&root, "[]".into());
            return;
        }
        if matches!(read, Read::Income { .. }) && (self.variant.is_spot() || self.variant.is_hyro())
        {
            let _ = cx.fail_context(
                &root,
                VenueError::new("This Bybit variant does not support income history"),
            );
            return;
        }
        let mut walk = match read {
            Read::OrderByClientId { .. } => {
                let _ = cx.fail_context(
                    &root,
                    VenueError::new(
                        "Bybit client order lookup is not supported by the Rust engine",
                    ),
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
                    stage_started: false,
                    cap,
                    kind: RowKind::Fills,
                    history: RowKind::Fills.owner(completion),
                }
            }
            Read::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => {
                // An omitted limit reads the whole window (bounded by the page guard), as TS does.
                let cap = limit.unwrap_or(0) as usize;
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
                    limit: if cap == 0 {
                        ORDERS_PAGE_LIMIT
                    } else {
                        (cap as u32).clamp(1, ORDERS_PAGE_LIMIT)
                    },
                    windows: windows(from_ms, to_ms),
                    window: (None, None),
                    stage_started: false,
                    cap,
                    kind: RowKind::Orders,
                    history: RowKind::Orders.owner(completion),
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
                    stage_started: false,
                    cap,
                    kind: RowKind::Income,
                    history: RowKind::Income.owner(completion),
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
                stage_started: false,
                cap: 0,
                kind: RowKind::Trades,
                history: RowKind::Trades.owner(completion),
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
        let full = walk.full();
        match walk.windows.pop_front().filter(|_| !full) {
            Some(window) => {
                walk.window = window;
                if walk.stage_started && !walk.history.restart_stage() {
                    self.history
                        .reads
                        .remove(&command)
                        .expect("present")
                        .finish(cx, Ok(()));
                    return;
                }
                walk.stage_started = true;
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
                    walk.category_rows_start = walk.history.accepted_len();
                    walk.windows = walk.original_windows.clone();
                    self.select_category(category);
                    self.next_window(cx, command);
                    return;
                }
                self.history
                    .reads
                    .remove(&command)
                    .expect("present")
                    .finish(cx, Ok(()));
            }
        }
    }

    fn request_page(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(walk) = self.history.reads.get(&command) else {
            return;
        };
        let leg = match self.page_leg(cx, walk.history.root(), walk.signed) {
            Ok(leg) => leg,
            Err(error) => {
                if let Some(walk) = self.history.reads.remove(&command) {
                    walk.finish(cx, Err(error));
                }
                return;
            }
        };
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
            .opt_str("cursor", walk.history.cursor());
        let rest = Rest::get(walk.path, query);
        let rest = if walk.signed { rest.signed() } else { rest };
        self.queue_read(cx, leg, rest, Then::History(Request { command }));
    }

    /// The engine retired this command's caller root at its deadline.
    pub(crate) fn retire_history(&mut self, owner: Owner) {
        self.history
            .reads
            .retain(|_, walk| walk.history.root().owner() != owner);
    }

    /// A page answer, delivered on `leg` (the page or its native re-read).
    pub(crate) fn on_history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        leg: &Context,
        response: Response<'_>,
    ) {
        let command = request.command;
        let Some(walk) = self.history.reads.get_mut(&command) else {
            return;
        };
        // A page of an earlier generation or a superseded attempt is ignored.
        if walk.history.validate_delivery(cx, leg).is_err() {
            return;
        }
        let body = match crate::read_refusal(&response, "") {
            None => Ok(response.body),
            Some(error) => Err(error),
        };
        // Decode the whole required list before staging any rows from this page.
        let parsed =
            body.and_then(|body| page(walk, body).map_err(|error| error.to_string().into()));
        let (rows, cursor) = match parsed {
            Ok(page) => page,
            Err(error) => {
                // The owner emits a single incomplete marker on recovery, or a
                // typed failure when no rows were accepted. No legacy duplicate.
                self.history
                    .reads
                    .remove(&command)
                    .expect("present")
                    .finish(cx, Err(error));
                return;
            }
        };
        let Ok(mut admitted) = walk.history.admit(cx, leg) else {
            return;
        };
        rows.stage(&mut admitted);
        let full = walk.full();
        let count = walk.history.staged_len();
        let Ok(mut admitted) = walk.history.admit(cx, leg) else {
            return;
        };
        let step = admitted.advance(CursorPage {
            next: cursor,
            rows: count,
            fresh: count,
            want_more: !full,
        });
        match step {
            CursorStep::More(_) => self.request_page(cx, command),
            CursorStep::Stalled(reason)
                if !(matches!(walk.kind, RowKind::Orders)
                    && walk.cap == 0
                    && reason == Stall::PageBudget) =>
            {
                // Endpoint-policy refusals must never become partial success.
                let _ = self
                    .history
                    .reads
                    .remove(&command)
                    .expect("present")
                    .history
                    .reject(
                        cx,
                        VenueError::new(
                            "Bybit history pagination is incomplete or did not advance",
                        ),
                    );
            }
            CursorStep::Done | CursorStep::Stalled(_) => self.next_window(cx, command),
        }
    }

    /// A page that failed before any answer: typed by the engine or the signer.
    pub(crate) fn on_history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        if let Some(walk) = self.history.reads.remove(&request.command) {
            walk.finish(cx, Err(error));
        }
    }
}

/// Decode one page atomically; only the owner can admit its mapped rows.
fn page(walk: &Walk, body: &str) -> serde_json::Result<(Rows, Option<String>)> {
    fn listed<'a, T: serde::Deserialize<'a>>(
        body: &'a str,
    ) -> serde_json::Result<(Vec<T>, Option<String>)> {
        #[derive(serde::Deserialize)]
        struct Envelope<T> {
            result: RequiredList<T>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RequiredList<T> {
            list: Vec<T>,
            #[serde(default)]
            next_page_cursor: Option<String>,
        }
        let result = serde_json::from_str::<Envelope<T>>(body)?.result;
        Ok((
            result.list,
            result.next_page_cursor.filter(|cursor| !cursor.is_empty()),
        ))
    }
    let mut rows = walk.kind.empty();
    let cursor = match &mut rows {
        Rows::Fills(rows) => {
            let (list, cursor) = listed::<ExecutionRow>(body)?;
            for e in list {
                let time = or_zero(&e.exec_time);
                let (price, qty) = (or_zero(&e.exec_price), or_zero(&e.exec_qty));
                let side = order_side(e.side.as_deref());
                // Funding and settlement rows share this endpoint but are not trades.
                if present(&e.exec_type).is_some_and(nontrade_execution)
                    || side.is_none()
                    || !(price.is_finite() && price > 0.0)
                    || !(qty.is_finite() && qty > 0.0)
                    || !(time.is_finite() && time > 0.0)
                {
                    continue;
                }
                rows.push(Execution {
                    id: e.exec_id.as_deref().unwrap_or_default().to_owned(),
                    timestamp: if time > 100_000_000_000.0 {
                        (time / 1000.0).floor()
                    } else {
                        time.floor()
                    },
                    symbol: e.symbol.into_owned(),
                    order_id: e.order_id.into_owned(),
                    price,
                    qty,
                    side: side_name(side),
                    commission: or_zero(&e.exec_fee).abs(),
                    commission_asset: present(&e.fee_currency).map(str::to_owned),
                    maker: e.is_maker,
                });
            }
            cursor
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
            cursor
        }
        Rows::Income(rows) => {
            let (list, cursor) = listed::<TransactionRow>(body)?;
            if list.is_empty() {
                return Ok((Rows::Income(Vec::new()), None));
            }
            for row in &list {
                incomes(row, rows);
            }
            cursor
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
            None
        }
    };
    Ok((rows, cursor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalization_rejects_a_mismatched_row_instead_of_dropping_it() {
        let row = Row::Trade(Trade {
            id: "trade".into(),
            symbol: "BTCUSDT".into(),
            price: 100.0,
            qty: 1.0,
            quote_qty: 100.0,
            timestamp: 1.0,
            side: "buy",
        });
        let error = RowKind::Fills.encode(vec![row], 1).unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::Internal));
        assert_eq!(error.message, "Bybit history row kind changed");
    }
}
