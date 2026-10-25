//! Route-specific history walkers; query rows never enter the live store.
use crate::wire::{Envelope, Str, number, optional_number};
use crate::{Pending, Woo, compact, native};
use safe_cex_engine::Level;
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, PageRow, RangeWalk, Step};
use safe_cex_engine::request_context::Operation;
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, TimerId, VenueError, num};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_PAGES: u32 = 1000;
const WINDOW: f64 = 30.0 * 86_400_000.0;
const LEDGER_GAP: f64 = 6000.0;

#[derive(Default)]
pub struct History {
    delayed: HashMap<TimerId, Request>,
    ledger_next: f64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Fills,
    LegacyFills,
    Orders,
    Income,
    Trades,
}
impl Route {
    fn label(self) -> &'static str {
        match self {
            Route::Fills | Route::LegacyFills => "fills history",
            Route::Orders => "order history",
            Route::Income => "income history",
            Route::Trades => "public trades",
        }
    }
}
pub type Request = Box<Read>;
pub struct Read {
    owner: CursorHistory<Entry>,
    route: Route,
    symbol: Option<String>,
    symbol_candidate: u8,
    last_fill_error: Option<String>,
    order_id: Option<String>,
    kind: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    window: (Option<f64>, Option<f64>),
    walk: Option<RangeWalk>,
    page: u32,
    size: u32,
    limit: usize,
    complete: bool,
    current: Vec<Entry>,
    count: usize,
    skipped: usize,
}
struct Entry {
    id: String,
    time: f64,
    symbol: String,
    row: Row,
}
fn encode_entries(rows: &[Entry]) -> Result<String, VenueError> {
    serde_json::to_string(&rows.iter().map(|entry| &entry.row).collect::<Vec<_>>())
        .map_err(|error| VenueError::new(error.to_string()))
}

fn next_cursor(r: &Read) -> String {
    // A selected range can legitimately be reread inclusively. Its engine
    // selection ordinal distinguishes that work from a native page cycle.
    format!(
        "{}:{}:{}:{}",
        r.walk.as_ref().map_or(0, |walk| walk.requests),
        r.route as u8,
        r.symbol_candidate,
        r.page
    )
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
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    timestamp: f64,
    symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    price: f64,
    qty: f64,
    side: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    commission: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commission_asset: Option<String>,
    position_side: &'static str,
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
    position_side: &'static str,
    created_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    realized_pnl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fees: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fee_asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rebate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rebate_asset: Option<String>,
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

fn exact_id<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = crate::wire::id(d)?;
    if value.is_empty() {
        Err(serde::de::Error::custom("empty history ID"))
    } else {
        Ok(value)
    }
}
fn optional_id<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let value = crate::wire::id(d)?;
    Ok((!value.is_empty()).then_some(value))
}
// Outer None is absent/null; Some(None) is an explicit empty string. Keep
// them distinct for the TS nullish-coalescing precedence of native ID fields.
fn nullish_id<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    #[derive(Deserialize)]
    struct Id(#[serde(deserialize_with = "optional_id")] Option<String>);
    Option::<&serde_json::value::RawValue>::deserialize(d)?
        .map(|raw| {
            serde_json::from_str::<Id>(raw.get())
                .map(|id| id.0)
                .map_err(serde::de::Error::custom)
        })
        .transpose()
}
fn maker<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flag {
        Bool(bool),
        Number(u8),
        Text(String),
    }
    match Option::<Flag>::deserialize(d)? {
        None => Ok(None),
        Some(Flag::Bool(v)) => Ok(Some(v)),
        Some(Flag::Number(v @ 0..=1)) => Ok(Some(v == 1)),
        Some(Flag::Text(s)) if s == "0" || s == "1" => Ok(Some(s == "1")),
        _ => Err(serde::de::Error::custom("invalid maker flag")),
    }
}
#[derive(Deserialize)]
struct FillRow<'a> {
    #[serde(default, deserialize_with = "nullish_id")]
    id: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullish_id", rename = "tradeId")]
    trade_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullish_id", rename = "trade_id")]
    trade_id_snake: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "nullish_id",
        rename = "orderId",
        alias = "order_id"
    )]
    order_id: Option<Option<String>>,
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(borrow)]
    side: Str<'a>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "executedPrice",
        alias = "executed_price"
    )]
    executed_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "averageExecutedPrice",
        alias = "average_executed_price"
    )]
    average_price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "lastPrice",
        alias = "last_price"
    )]
    last_price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "executedQuantity",
        alias = "executed_quantity"
    )]
    executed_quantity: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    quantity: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    executed: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "lastQty",
        alias = "last_qty"
    )]
    last_quantity: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "executedTimestamp",
        alias = "executed_timestamp"
    )]
    executed_timestamp: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "executedTime",
        alias = "executed_time"
    )]
    executed_time: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "tradeTime",
        alias = "trade_time"
    )]
    trade_time: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    timestamp: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "createdTime",
        alias = "created_time"
    )]
    created_time: Option<f64>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "updatedTime",
        alias = "updated_time"
    )]
    updated_time: Option<f64>,
    #[serde(default, deserialize_with = "optional_number", alias = "total_fee")]
    fee: Option<f64>,
    #[serde(borrow, default, rename = "feeAsset", alias = "fee_asset")]
    fee_asset: Option<Str<'a>>,
    #[serde(
        default,
        deserialize_with = "optional_number",
        rename = "realizedPnl",
        alias = "realized_pnl"
    )]
    realized_pnl: Option<f64>,
    #[serde(
        default,
        deserialize_with = "maker",
        rename = "isMaker",
        alias = "is_maker"
    )]
    maker: Option<bool>,
    #[serde(borrow, default, rename = "positionSide", alias = "position_side")]
    position_side: Option<Str<'a>>,
}
#[derive(Deserialize)]
struct OrderRow<'a> {
    #[serde(deserialize_with = "exact_id")]
    order_id: String,
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(borrow)]
    side: Str<'a>,
    #[serde(borrow, rename = "type")]
    kind: Str<'a>,
    #[serde(borrow)]
    status: Str<'a>,
    #[serde(default, deserialize_with = "optional_number")]
    price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    average_executed_price: Option<f64>,
    #[serde(deserialize_with = "number")]
    quantity: f64,
    #[serde(deserialize_with = "number")]
    executed: f64,
    reduce_only: bool,
    #[serde(borrow, default)]
    position_side: Option<Str<'a>>,
    #[serde(deserialize_with = "number")]
    created_time: f64,
    #[serde(default, deserialize_with = "optional_number")]
    updated_time: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    realized_pnl: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    total_fee: Option<f64>,
    #[serde(borrow, default)]
    fee_asset: Option<Str<'a>>,
    #[serde(default, deserialize_with = "optional_number")]
    total_rebate: Option<f64>,
    #[serde(borrow, default)]
    rebate_asset: Option<Str<'a>>,
}
#[derive(Deserialize)]
struct LedgerRow<'a> {
    #[serde(deserialize_with = "exact_id")]
    id: String,
    #[serde(borrow, rename = "type")]
    kind: Str<'a>,
    #[serde(borrow)]
    token: Str<'a>,
    #[serde(borrow, default)]
    symbol: Option<Str<'a>>,
    #[serde(deserialize_with = "number")]
    amount: f64,
    #[serde(deserialize_with = "number")]
    timestamp: f64,
}
#[derive(Deserialize)]
struct TradeRow<'a> {
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(borrow)]
    side: Str<'a>,
    #[serde(deserialize_with = "number")]
    executed_price: f64,
    #[serde(deserialize_with = "number")]
    executed_quantity: f64,
    #[serde(deserialize_with = "number")]
    executed_timestamp: f64,
}
#[derive(Deserialize)]
struct Meta {
    total: u64,
    records_per_page: u32,
    current_page: u32,
}
#[derive(Deserialize)]
struct Metadata<'a> {
    meta: Option<Meta>,
    #[serde(borrow)]
    data: Option<&'a serde_json::value::RawValue>,
}
fn meta(body: &str) -> Result<Option<Meta>, String> {
    let m: Metadata<'_> = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if m.meta.is_some() {
        return Ok(m.meta);
    }
    #[derive(Deserialize)]
    struct Nested {
        meta: Option<Meta>,
    }
    Ok(m.data
        .and_then(|d| serde_json::from_str::<Nested>(d.get()).ok())
        .and_then(|d| d.meta))
}
fn category(kind: &str) -> &'static str {
    match kind {
        "REALIZED_PNL" => "realized_pnl",
        "FUNDING_FEE" => "funding",
        "TRANSACTION_FEE" | "TRADING_FEE" => "commission",
        "REBATE" | "REFERRAL" => "rebate",
        "WITHDRAW"
        | "DEPOSIT"
        | "FIAT_WITHDRAW"
        | "FIAT_DEPOSIT"
        | "SUB_ACCOUNT_TRANSFER"
        | "CONVERT" => "transfer",
        "LIQUIDATION" => "liquidation",
        _ => "other",
    }
}
fn side(raw: &str) -> Option<(&'static str, &'static str)> {
    let value = raw
        .to_ascii_uppercase()
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Some(match value.as_str() {
        "BUY" => ("buy", "both"),
        "SELL" => ("sell", "both"),
        "OPEN LONG" => ("buy", "long"),
        "CLOSE LONG" => ("sell", "long"),
        "OPEN SHORT" => ("sell", "short"),
        "CLOSE SHORT" => ("buy", "short"),
        _ => return None,
    })
}
fn position(raw: Option<&str>, fallback: &'static str) -> &'static str {
    match raw {
        Some("LONG") => "long",
        Some("SHORT") => "short",
        _ => fallback,
    }
}
fn millis(time: f64) -> f64 {
    if time >= 1_000_000_000_000.0 {
        time.floor()
    } else {
        (time * 1000.0).floor()
    }
}
fn nonzero(value: Option<f64>) -> Option<f64> {
    value.filter(|v| *v != 0.0)
}

// Execution history accepts WOO's prefix/suffix symbols and chart aliases,
// matching the TS venue's normalizeSymbol/normalizeRequestedSymbol boundary.
fn transaction_symbol(raw: &str) -> String {
    let uppercase = raw.to_uppercase();
    let symbol = uppercase
        .rsplit(':')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(&uppercase);
    for separator in ['_', '-'] {
        let parts: Vec<_> = symbol.split(separator).collect();
        if parts.len() >= 3 {
            if matches!(parts[0], "PERP" | "SPOT") {
                return format!("{}{}", parts[1], parts[2]);
            }
            if parts.last() == Some(&"PERP") {
                return format!("{}{}", parts[0], parts[1]);
            }
        }
    }
    if !symbol.contains(['_', '-'])
        && let Some(base) = symbol.strip_suffix("PERP")
    {
        return format!("{base}USDT");
    }
    symbol
        .chars()
        .filter(|c| !matches!(c, '_' | '-' | '.' | '/' | ':'))
        .collect()
}

fn requested_symbol(raw: &str) -> String {
    let uppercase = raw.to_uppercase();
    let mut symbol = uppercase
        .rsplit(':')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(&uppercase);
    for suffix in [".P", "_PERP", "-PERP", "PERP"] {
        symbol = symbol.strip_suffix(suffix).unwrap_or(symbol);
    }
    transaction_symbol(&symbol.replace('/', "_"))
}

impl Woo {
    fn fail_local_history(cx: &mut Cx<'_, Pending>, message: &str) {
        cx.refuse_command(Operation::Read, message);
    }
    pub(crate) fn history_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let (route, symbol, order_id, kind, from, to, limit) = match query {
            Query::OrderByClientId { .. } => {
                Self::fail_local_history(cx, "WOO remote client order lookup is unsupported");
                return;
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (Route::Fills, symbol, order_id, None, from_ms, to_ms, limit),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (Route::Orders, symbol, None, None, from_ms, to_ms, limit),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => (Route::Income, symbol, None, kind, from_ms, to_ms, limit),
            Query::PublicTrades { symbol, limit } => {
                (Route::Trades, Some(symbol), None, None, None, None, limit)
            }
        };
        let symbol = if route == Route::Fills {
            symbol
                .filter(|s| !s.is_empty())
                .map(|s| requested_symbol(&s))
        } else {
            symbol
        };
        if symbol
            .as_ref()
            .is_some_and(|s| cx.store.market(s).is_none())
            || from.is_some_and(|t| !t.is_finite() || t < 0.0)
            || to.is_some_and(|t| !t.is_finite() || t < 0.0)
            || from.zip(to).is_some_and(|(a, b)| a > b)
            || limit.is_some_and(|n| n == 0 || n > 100_000)
        {
            Self::fail_local_history(cx, "Invalid WOO history symbol, range or limit");
            return;
        }
        let complete = route == Route::Income && from.is_some() && to.is_some() && limit.is_none();
        let limit = limit.unwrap_or(if route == Route::Orders || complete {
            100_000
        } else {
            500
        }) as usize;
        let window = (from, to.or(from.map(|_| cx.now)));
        if window.0.zip(window.1).is_some_and(|(a, b)| a > b) {
            Self::fail_local_history(cx, "Invalid WOO history range");
            return;
        }
        let walk = if route == Route::Fills {
            window
                .0
                .zip(window.1)
                .filter(|(a, b)| b > a)
                .map(|(a, b)| RangeWalk::new(a, b, WINDOW, limit.min(500)))
        } else {
            None
        };
        let mut r = Box::new(Read {
            owner: CursorHistory::new(
                Completion::new(command),
                CursorWalk::new(MAX_PAGES),
                |_| "WOO history pagination limit exceeded".into(),
                encode_entries,
            ),
            route,
            symbol,
            symbol_candidate: 0,
            last_fill_error: None,
            order_id,
            kind,
            from,
            to,
            window,
            walk,
            page: 1,
            size: (if route == Route::Fills { 500 } else { 100 }).min(limit as u32),
            limit,
            complete,
            current: vec![],
            count: 0,
            skipped: 0,
        });
        if r.walk.is_some() {
            self.history_next_window(cx, r, 0, None, true);
        } else {
            r.size = r.size.min(limit as u32);
            self.history_page(cx, r);
        }
    }
    fn history_page(&mut self, cx: &mut Cx<'_, Pending>, mut r: Request) {
        if r.route == Route::Income && cx.now < self.history.ledger_next {
            let timer = cx.set_timer(self.history.ledger_next - cx.now);
            self.history.delayed.insert(timer, r);
            return;
        }
        if r.route == Route::Income {
            self.history.ledger_next = cx.now + LEDGER_GAP;
        }
        r.current.clear();
        r.count = 0;
        r.skipped = 0;
        let mut params = vec![];
        if let Some(s) = &r.symbol
            && r.route != Route::Income
        {
            let symbol = if matches!(r.route, Route::Fills | Route::LegacyFills) {
                let base = s.strip_suffix("USDT").unwrap_or(s);
                match r.symbol_candidate {
                    0 => Some(native(cx.store.market(s).unwrap())),
                    1 => Some(format!("{base}_USDT_PERP")),
                    2 => Some(format!("{base}-USDT-PERP")),
                    3 => Some(s.clone()),
                    _ => None,
                }
            } else {
                Some(native(cx.store.market(s).unwrap()))
            };
            if let Some(symbol) = symbol {
                params.push(("symbol".into(), symbol));
            }
        }
        let (path, at) = match r.route {
            Route::Fills => {
                params.push(("limit".into(), r.size.to_string()));
                (
                    "/v3/trade/transactionHistory",
                    ArrayAt::Envelope(&[&["data", "rows"], &["rows"], &["data"]]),
                )
            }
            Route::LegacyFills => {
                params.push(("size".into(), r.size.to_string()));
                if r.page > 1 {
                    params.push(("page".into(), r.page.to_string()));
                }
                (
                    "/v1/client/trades",
                    ArrayAt::Envelope(&[&["data", "rows"], &["rows"], &["data"]]),
                )
            }
            Route::Orders => {
                params.extend([
                    ("size".into(), r.size.to_string()),
                    ("page".into(), r.page.to_string()),
                    ("realized_pnl".into(), "true".into()),
                ]);
                ("/v1/orders", ArrayAt::Field("rows"))
            }
            Route::Income => {
                params.extend([
                    ("size".into(), r.size.to_string()),
                    ("page".into(), r.page.to_string()),
                ]);
                if let Some(k) = &r.kind {
                    params.push(("type".into(), k.clone()));
                }
                (
                    "/v1/client/transaction_history",
                    ArrayAt::Path(&["data", "rows"]),
                )
            }
            Route::Trades => {
                params.push(("limit".into(), r.size.to_string()));
                ("/v1/public/market_trades", ArrayAt::Field("rows"))
            }
        };
        for (key, value) in [
            (
                if r.route == Route::Fills {
                    "startTime"
                } else {
                    "start_t"
                },
                r.window.0,
            ),
            (
                if r.route == Route::Fills {
                    "endTime"
                } else {
                    "end_t"
                },
                r.window.1,
            ),
        ] {
            if let Some(v) = value {
                params.push((key.into(), num::format(v)));
            }
        }
        let command = r.owner.command();
        if r.route == Route::Trades {
            let query = params
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&");
            cx.request_streamed(
                Method::Get,
                format!("{}{path}?{query}", self.rest),
                vec![],
                at,
                Pending::History(r),
            );
        } else {
            match self.signed(cx, Method::Get, path, params, None, Pending::History(r)) {
                Ok(id) => cx.stream_response(id, at),
                Err(error) => cx.fail(command, error),
            }
        }
    }
    fn history_advance(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        more: bool,
        fresh: usize,
        gap: Option<String>,
    ) {
        // Empty fallback transitions count a completed attempt but never carry
        // its rejected native rows into the shared admission boundary.
        let page = CursorPage {
            rows: r.count,
            fresh,
            next: more.then(|| next_cursor(&r)),
            want_more: more,
        };
        match r.owner.advance_with_coverage(page, gap) {
            CursorStep::More(_) => self.history_page(cx, r),
            CursorStep::Done => Self::history_finish(cx, r, Ok(())),
            CursorStep::Stalled(_) => {
                Self::history_rejected(cx, r, "WOO history pagination limit exceeded")
            }
        }
    }
    fn history_next_window(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        fresh: usize,
        gap: Option<String>,
        initial: bool,
    ) {
        loop {
            match r.walk.as_mut().unwrap().step() {
                Step::Fetch { from, to } => {
                    r.window = (Some(from), Some(to));
                    r.page = 1;
                    r.symbol_candidate = 0;
                    r.last_fill_error = None;
                    if initial {
                        self.history_page(cx, r);
                    } else {
                        self.history_advance(cx, r, true, fresh, gap);
                    }
                    return;
                }
                Step::ChunkDone => (),
                Step::Finished => {
                    if r.walk.as_ref().unwrap().truncated {
                        Self::history_rejected(cx, r, "WOO fills range could not be completed");
                    } else {
                        self.history_advance(cx, r, false, fresh, gap);
                    }
                    return;
                }
            }
        }
    }
    pub(crate) fn history_item(&mut self, _cx: &mut Cx<'_, Pending>, r: &mut Request, item: &str) {
        r.count += 1;
        match history_entry(r, item) {
            Ok(Some(row)) => r.current.push(row),
            Ok(None) => (),
            Err(_) => r.skipped += 1,
        }
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) || Envelope::checked(response.body).is_err() {
            self.history_failed(cx, r, "WOO history request rejected");
            return;
        }
        if r.skipped > 0 {
            if r.skipped == r.count {
                Self::history_finish(cx, r, Err(VenueError::new("Malformed WOO history page")));
                return;
            }
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                format!(
                    "WOO {}: skipped {} unmappable row(s)",
                    r.route.label(),
                    r.skipped
                ),
            );
        }
        if r.count == 0
            && (r.route == Route::Fills || (r.route == Route::LegacyFills && r.page == 1))
        {
            self.history_fill_fallback(cx, r, None);
            return;
        }
        let page = match meta(response.body) {
            Ok(m) => m,
            Err(e) => {
                Self::history_finish(cx, r, Err(VenueError::new(e)));
                return;
            }
        };
        if page
            .as_ref()
            .is_some_and(|m| m.current_page != r.page || m.records_per_page == 0)
            && r.route != Route::Fills
        {
            Self::history_rejected(cx, r, "Invalid WOO history page metadata");
            return;
        }
        let mut fresh = 0;
        if r.route == Route::Fills
            && let Some(walk) = r.walk.as_mut()
        {
            let identities = r
                .current
                .iter()
                .map(|e| PageRow {
                    identity: e.id.clone(),
                    timestamp: e.time,
                })
                // Skipped rows still fill the page; the walk ignores non-finite times.
                .chain((0..r.skipped).map(|i| PageRow {
                    identity: format!("skipped:{i}"),
                    timestamp: f64::NAN,
                }))
                .collect::<Vec<_>>();
            let indexes = walk
                .page(r.window.0.unwrap(), r.window.1.unwrap(), &identities)
                .into_iter()
                .collect::<HashSet<_>>();
            for (index, e) in std::mem::take(&mut r.current).into_iter().enumerate() {
                if indexes.contains(&index) && matches_entry(&r, &e) && r.owner.fresh(e.id.clone())
                {
                    r.owner.stage(e);
                    fresh += 1;
                }
            }
        } else {
            for e in std::mem::take(&mut r.current) {
                if r.owner.fresh(e.id.clone()) {
                    fresh += 1;
                    if matches_entry(&r, &e) {
                        r.owner.stage(e);
                    }
                }
            }
        }
        let retained = r.owner.accepted_len() + r.owner.staged_len();
        if retained > r.limit && r.complete {
            Self::history_rejected(
                cx,
                r,
                "WOO income range exceeded the complete-history budget",
            );
            return;
        }
        let gap = (r.skipped > 0).then(|| {
            format!(
                "history is INCOMPLETE: WOO {}: skipped {} unmappable row(s)",
                r.route.label(),
                r.skipped
            )
        });
        if (retained >= r.limit && !r.complete) || r.route == Route::Trades {
            self.history_advance(cx, r, false, fresh, gap);
            return;
        }
        if r.route == Route::Fills {
            if r.walk.is_some() {
                self.history_next_window(cx, r, fresh, gap, false);
            } else {
                self.history_advance(cx, r, false, fresh, gap);
            }
            return;
        }
        let complete = r.count == 0
            || (page.as_ref().is_some_and(|m| {
                u64::from(m.current_page) * u64::from(m.records_per_page) >= m.total
            }));
        if !complete {
            if fresh == 0 {
                Self::history_rejected(cx, r, "WOO history page made no progress");
                return;
            }
            r.page += 1;
            self.history_advance(cx, r, true, fresh, gap);
            return;
        }
        if r.route == Route::LegacyFills && r.walk.is_some() {
            r.route = Route::Fills;
            self.history_next_window(cx, r, fresh, gap, false);
        } else {
            self.history_advance(cx, r, false, fresh, gap);
        }
    }
    fn history_finish(cx: &mut Cx<'_, Pending>, r: Request, result: Result<(), VenueError>) {
        let limit = r.limit;
        r.owner.finish_with(cx, result, |mut rows| {
            rows.sort_by(|a, b| b.time.total_cmp(&a.time).then_with(|| a.id.cmp(&b.id)));
            rows.truncate(limit);
            encode_entries(&rows)
        });
    }
    fn history_rejected(cx: &mut Cx<'_, Pending>, r: Request, error: &str) {
        r.owner.reject(cx, VenueError::new(error));
    }
    // The active TS adapter tries each venue spelling before falling back to
    // the older fills route. These are reads, never retries of ambiguous writes.
    fn history_fill_fallback(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        error: Option<&str>,
    ) {
        if let Some(error) = error {
            r.last_fill_error = Some(error.into());
        }
        if r.symbol.is_some() && r.symbol_candidate < 4 && r.page == 1 {
            r.symbol_candidate += 1;
            self.history_advance(cx, r, true, 0, None);
        } else if r.route == Route::Fills {
            r.route = Route::LegacyFills;
            r.symbol_candidate = 0;
            r.page = 1;
            self.history_advance(cx, r, true, 0, None);
        } else if let Some(error) = r.last_fill_error.take() {
            Self::history_finish(cx, r, Err(VenueError::new(error)));
        } else if r.walk.is_some() {
            r.route = Route::Fills;
            self.history_next_window(cx, r, 0, None, false);
        } else {
            self.history_advance(cx, r, false, 0, None);
        }
    }
    pub(crate) fn history_failed(&mut self, cx: &mut Cx<'_, Pending>, r: Request, error: &str) {
        if matches!(r.route, Route::Fills | Route::LegacyFills) && r.page == 1 {
            self.history_fill_fallback(cx, r, Some(error));
        } else {
            Self::history_finish(cx, r, Err(VenueError::new(error)));
        }
    }
    pub(crate) fn history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let Some(r) = self.history.delayed.remove(&timer) else {
            return false;
        };
        self.history_page(cx, r);
        true
    }
}
fn matches_entry(r: &Request, e: &Entry) -> bool {
    r.from.is_none_or(|a| e.time >= a)
        && r.to.is_none_or(|b| e.time <= b)
        && r.symbol.as_ref().is_none_or(|s| s == &e.symbol)
        && r.kind
            .as_ref()
            .is_none_or(|k| matches!(&e.row,Row::Income(i) if i.income_type==*k))
        && r.order_id
            .as_ref()
            .is_none_or(|id| matches!(&e.row,Row::Fill(f) if f.order_id.as_ref()==Some(id)))
}
fn history_entry(r: &Request, item: &str) -> Result<Option<Entry>, String> {
    let err = |e: serde_json::Error| e.to_string();
    Ok(Some(match r.route {
        Route::Fills | Route::LegacyFills => {
            let row: FillRow<'_> = serde_json::from_str(item).map_err(err)?;
            let symbol = transaction_symbol(&row.symbol.replace('/', "_"));
            if symbol.is_empty() {
                return Ok(None);
            }
            let (side, leg) = side(&row.side).ok_or("Invalid WOO execution side")?;
            let price = row
                .executed_price
                .or(row.price)
                .or(row.average_price)
                .or(row.last_price)
                .filter(|price| *price > 0.0)
                .ok_or("Invalid WOO execution price")?;
            let qty = row
                .executed_quantity
                .or(row.quantity)
                .or(row.executed)
                .or(row.last_quantity)
                .filter(|qty| *qty > 0.0)
                .ok_or("Invalid WOO execution quantity")?;
            let timestamp = [
                row.executed_timestamp,
                row.executed_time,
                row.trade_time,
                row.timestamp,
                row.created_time,
                row.updated_time,
            ]
            .into_iter()
            .flatten()
            .find(|time| *time != 0.0)
            .filter(|time| *time > 0.0)
            .ok_or("Invalid WOO execution timestamp")?;
            let time = millis(timestamp);
            let execution = Execution {
                order_id: row.order_id.unwrap_or_else(|| row.id.clone().flatten()),
                id: row.id.or(row.trade_id).or(row.trade_id_snake).flatten(),
                timestamp: (time / 1000.0).floor(),
                symbol: symbol.clone(),
                price,
                qty,
                side,
                commission: row.fee,
                commission_asset: row.fee_asset.map(|s| s.into_owned()),
                position_side: position(row.position_side.as_deref(), leg),
                realized_pnl: row.realized_pnl,
                maker: row.maker,
            };
            // Id-less historical aggregates remain id-less to callers. Their
            // canonical content gives the pager an internal deduplication key.
            let id = match &execution.id {
                Some(id) => format!("fill:{id}"),
                None => format!(
                    "anonymous:{time}:{}",
                    serde_json::to_string(&execution).map_err(err)?
                ),
            };
            Entry {
                id,
                time,
                symbol,
                row: Row::Fill(execution),
            }
        }
        Route::Orders => {
            let row: OrderRow<'_> = serde_json::from_str(item).map_err(err)?;
            if !row.symbol.starts_with("PERP_") {
                return Ok(None);
            }
            let (side, _) = side(&row.side).ok_or("Invalid WOO order side")?;
            let status = match row.status.as_ref() {
                "NEW" | "INCOMPLETE" | "PARTIAL_FILLED" | "PARTIALLY_FILLED" => OrderStatus::Open,
                "FILLED" | "COMPLETED" => OrderStatus::Closed,
                "CANCELLED" | "CANCELED" | "REJECTED" => OrderStatus::Canceled,
                _ => return Err("Invalid WOO history status".into()),
            };
            let kind = match row.kind.as_ref() {
                "LIMIT" | "IOC" | "FOK" | "POST_ONLY" | "ASK" | "BID" => OrderType::Limit,
                "MARKET" => OrderType::Market,
                _ => return Err("Invalid WOO history order type".into()),
            };
            if row.created_time <= 0.0
                || row.quantity < 0.0
                || row.executed < 0.0
                || row.executed > row.quantity
            {
                return Err("Invalid WOO historical order".into());
            }
            let time = (row.created_time * 1000.0).floor();
            let symbol = compact(&row.symbol);
            let id = row.order_id;
            Entry {
                id: id.clone(),
                time,
                symbol: symbol.clone(),
                row: Row::Order(HistoricalOrder {
                    id,
                    symbol,
                    side,
                    kind,
                    status,
                    price: nonzero(row.price),
                    average_price: nonzero(row.average_executed_price),
                    amount: row.quantity,
                    filled: row.executed,
                    reduce_only: row.reduce_only,
                    position_side: position(row.position_side.as_deref(), "both"),
                    created_time: time,
                    updated_time: row
                        .updated_time
                        .filter(|t| *t > 0.0)
                        .map(|t| (t * 1000.0).floor()),
                    realized_pnl: row.realized_pnl,
                    fees: row.total_fee,
                    fee_asset: row.fee_asset.map(|s| s.into_owned()),
                    rebate: row.total_rebate,
                    rebate_asset: row.rebate_asset.map(|s| s.into_owned()),
                }),
            }
        }
        Route::Income => {
            let row: LedgerRow<'_> = serde_json::from_str(item).map_err(err)?;
            if row.timestamp <= 0.0 || row.kind.is_empty() || row.token.is_empty() {
                return Err("Invalid WOO ledger row".into());
            }
            let symbol = row
                .symbol
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| compact(s.strip_prefix("SPOT_").unwrap_or(s)))
                .unwrap_or_default();
            let id = row.id;
            let kind = row.kind.into_owned();
            Entry {
                id: format!("{id}:{kind}"),
                time: row.timestamp,
                symbol: symbol.clone(),
                row: Row::Income(Income {
                    symbol,
                    income_type: kind.clone(),
                    income: row.amount,
                    asset: row.token.into_owned(),
                    timestamp: row.timestamp,
                    tran_id: id,
                    category: category(&kind),
                }),
            }
        }
        Route::Trades => {
            let row: TradeRow<'_> = serde_json::from_str(item).map_err(err)?;
            let (side, _) = side(&row.side).ok_or("Invalid WOO public trade side")?;
            if !row.symbol.starts_with("PERP_")
                || row.executed_price <= 0.0
                || row.executed_quantity <= 0.0
                || row.executed_timestamp <= 0.0
            {
                return Err("Invalid WOO public trade".into());
            }
            let symbol = compact(&row.symbol);
            let time = (row.executed_timestamp * 1000.0).floor();
            let id = format!(
                "{}:{}:{}:{}:{}",
                time, row.executed_price, row.executed_quantity, side, r.count
            );
            Entry {
                id: id.clone(),
                time,
                symbol: symbol.clone(),
                row: Row::Trade(Trade {
                    id,
                    symbol,
                    price: row.executed_price,
                    qty: row.executed_quantity,
                    quote_qty: row.executed_price * row.executed_quantity,
                    timestamp: time,
                    side,
                }),
            }
        }
    }))
}
