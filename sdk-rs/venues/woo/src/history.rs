//! Route-specific history walkers; query rows never enter the live store.
use crate::wire::{Envelope, Str, number, optional_number};
use crate::{Pending, Woo, compact, native};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, TimerId, num};
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
pub type Request = Box<Read>;
pub struct Read {
    command: CommandId,
    route: Route,
    symbol: Option<String>,
    order_id: Option<String>,
    kind: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    window: (Option<f64>, Option<f64>),
    walk: Option<RangeWalk>,
    page: u32,
    pages: u32,
    size: u32,
    limit: usize,
    complete: bool,
    seen: HashSet<String>,
    rows: Vec<Entry>,
    current: Vec<Entry>,
    count: usize,
    bad: bool,
}
struct Entry {
    id: String,
    time: f64,
    symbol: String,
    row: Row,
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
    #[serde(deserialize_with = "exact_id", alias = "tradeId", alias = "trade_id")]
    id: String,
    #[serde(
        default,
        deserialize_with = "optional_id",
        rename = "orderId",
        alias = "order_id"
    )]
    order_id: Option<String>,
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(borrow)]
    side: Str<'a>,
    #[serde(
        deserialize_with = "number",
        rename = "executedPrice",
        alias = "executed_price",
        alias = "price"
    )]
    price: f64,
    #[serde(
        deserialize_with = "number",
        rename = "executedQuantity",
        alias = "executed_quantity",
        alias = "quantity"
    )]
    qty: f64,
    #[serde(
        deserialize_with = "number",
        rename = "executedTimestamp",
        alias = "executed_timestamp",
        alias = "timestamp"
    )]
    timestamp: f64,
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

impl Woo {
    pub(crate) fn history_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let (route, symbol, order_id, kind, from, to, limit) = match query {
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
        if symbol
            .as_ref()
            .is_some_and(|s| cx.store.market(s).is_none())
            || from.is_some_and(|t| !t.is_finite() || t < 0.0)
            || to.is_some_and(|t| !t.is_finite() || t < 0.0)
            || from.zip(to).is_some_and(|(a, b)| a > b)
            || limit.is_some_and(|n| n == 0 || n > 100_000)
        {
            cx.done(
                command,
                Err("Invalid WOO history symbol, range or limit".into()),
            );
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
            cx.done(command, Err("Invalid WOO history range".into()));
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
            command,
            route,
            symbol,
            order_id,
            kind,
            from,
            to,
            window,
            walk,
            page: 1,
            pages: 0,
            size: (if route == Route::Fills { 500 } else { 100 }).min(limit as u32),
            limit,
            complete,
            seen: HashSet::new(),
            rows: vec![],
            current: vec![],
            count: 0,
            bad: false,
        });
        if r.walk.is_some() {
            self.history_next_window(cx, r);
        } else {
            r.size = r.size.min(limit as u32);
            self.history_page(cx, r);
        }
    }
    fn history_page(&mut self, cx: &mut Cx<'_, Pending>, mut r: Request) {
        if r.pages >= MAX_PAGES {
            cx.done(
                r.command,
                Err("WOO history pagination limit exceeded".into()),
            );
            return;
        }
        if r.route == Route::Income && cx.now < self.history.ledger_next {
            let timer = cx.set_timer(self.history.ledger_next - cx.now);
            self.history.delayed.insert(timer, r);
            return;
        }
        if r.route == Route::Income {
            self.history.ledger_next = cx.now + LEDGER_GAP;
        }
        r.pages += 1;
        r.current.clear();
        r.count = 0;
        r.bad = false;
        let mut params = vec![];
        if let Some(s) = &r.symbol
            && r.route != Route::Income
        {
            params.push(("symbol".into(), native(cx.store.market(s).unwrap())));
        }
        let (path, at) = match r.route {
            Route::Fills => {
                params.push(("limit".into(), r.size.to_string()));
                (
                    "/v3/trade/transactionHistory",
                    ArrayAt::EnvelopeFieldOrNested("data", "rows"),
                )
            }
            Route::LegacyFills => {
                params.extend([
                    ("size".into(), r.size.to_string()),
                    ("page".into(), r.page.to_string()),
                ]);
                ("/v1/client/trades", ArrayAt::Field("rows"))
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
        let command = r.command;
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
                Err(error) => cx.done(command, Err(error)),
            }
        }
    }
    fn history_next_window(&mut self, cx: &mut Cx<'_, Pending>, mut r: Request) {
        loop {
            match r.walk.as_mut().unwrap().step() {
                Step::Fetch { from, to } => {
                    r.window = (Some(from), Some(to));
                    self.history_page(cx, r);
                    return;
                }
                Step::ChunkDone => (),
                Step::Finished => {
                    if r.walk.as_ref().unwrap().truncated {
                        cx.done(
                            r.command,
                            Err("WOO fills range could not be completed".into()),
                        );
                    } else {
                        Self::history_finish(cx, r);
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
            Err(_) => r.bad = true,
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
        if r.bad {
            cx.done(r.command, Err("Malformed WOO history row".into()));
            return;
        }
        if r.route == Route::Fills && r.count == 0 {
            r.route = Route::LegacyFills;
            r.page = 1;
            self.history_page(cx, r);
            return;
        }
        let page = match meta(response.body) {
            Ok(m) => m,
            Err(e) => {
                cx.done(r.command, Err(e));
                return;
            }
        };
        if page
            .as_ref()
            .is_some_and(|m| m.current_page != r.page || m.records_per_page == 0)
            && r.route != Route::Fills
        {
            cx.done(r.command, Err("Invalid WOO history page metadata".into()));
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
                .collect::<Vec<_>>();
            let indexes = walk
                .page(r.window.0.unwrap(), r.window.1.unwrap(), &identities)
                .into_iter()
                .collect::<HashSet<_>>();
            for (index, e) in std::mem::take(&mut r.current).into_iter().enumerate() {
                if indexes.contains(&index) && matches_entry(&r, &e) && r.seen.insert(e.id.clone())
                {
                    r.rows.push(e);
                    fresh += 1;
                }
            }
        } else {
            for e in std::mem::take(&mut r.current) {
                if r.seen.insert(e.id.clone()) {
                    fresh += 1;
                    if matches_entry(&r, &e) {
                        r.rows.push(e);
                    }
                }
            }
        }
        if r.rows.len() > r.limit && r.complete {
            cx.done(
                r.command,
                Err("WOO income range exceeded the complete-history budget".into()),
            );
            return;
        }
        if (r.rows.len() >= r.limit && !r.complete) || r.route == Route::Trades {
            Self::history_finish(cx, r);
            return;
        }
        if r.route == Route::Fills {
            if r.walk.is_some() {
                self.history_next_window(cx, r);
            } else {
                Self::history_finish(cx, r);
            }
            return;
        }
        let complete = r.count == 0
            || (page.as_ref().is_some_and(|m| {
                u64::from(m.current_page) * u64::from(m.records_per_page) >= m.total
            }));
        if !complete {
            if fresh == 0 {
                cx.done(r.command, Err("WOO history page made no progress".into()));
                return;
            }
            r.page += 1;
            self.history_page(cx, r);
            return;
        }
        if r.route == Route::LegacyFills && r.walk.is_some() {
            r.route = Route::Fills;
            self.history_next_window(cx, r);
        } else {
            Self::history_finish(cx, r);
        }
    }
    fn history_finish(cx: &mut Cx<'_, Pending>, mut r: Request) {
        r.rows
            .sort_by(|a, b| b.time.total_cmp(&a.time).then_with(|| a.id.cmp(&b.id)));
        r.rows.truncate(r.limit);
        let rows = r.rows.into_iter().map(|r| r.row).collect::<Vec<_>>();
        cx.done(
            r.command,
            serde_json::to_string(&rows).map_err(|e| e.to_string()),
        );
    }
    pub(crate) fn history_failed(&mut self, cx: &mut Cx<'_, Pending>, mut r: Request, error: &str) {
        if r.route == Route::Fills {
            r.route = Route::LegacyFills;
            r.page = 1;
            self.history_page(cx, r);
        } else {
            cx.done(r.command, Err(error.into()));
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
            if !row.symbol.starts_with("PERP_") {
                return Ok(None);
            }
            let (side, leg) = side(&row.side).ok_or("Invalid WOO execution side")?;
            if row.price <= 0.0 || row.qty <= 0.0 || row.timestamp <= 0.0 {
                return Err("Invalid WOO execution".into());
            }
            let symbol = compact(&row.symbol);
            let time = millis(row.timestamp);
            let id = row.id;
            Entry {
                id: id.clone(),
                time,
                symbol: symbol.clone(),
                row: Row::Fill(Execution {
                    id,
                    timestamp: (time / 1000.0).floor(),
                    symbol,
                    order_id: row.order_id,
                    price: row.price,
                    qty: row.qty,
                    side,
                    commission: row.fee,
                    commission_asset: row.fee_asset.map(|s| s.into_owned()),
                    position_side: position(row.position_side.as_deref(), leg),
                    realized_pnl: row.realized_pnl,
                    maker: row.maker,
                }),
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
