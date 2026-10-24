//! Classic futures queries: numbered pages for fills/orders, exact ledger offsets.
use crate::{Kucoin, Pending};
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::paging::{self, CursorPage, CursorStep, CursorWalk, WindowOrder};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Outcome, Query, TimerId, VenueError};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, VecDeque};

// These returning branches perform no transport or private remote read.
pub(crate) fn local_read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

const DAY: u64 = 86_400_000;
/// Requests per read, and windows per range, before a read is refused.
const MAX_REQUESTS: u32 = 10_000;
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fills,
    Orders,
    Income,
    Trades,
}
#[derive(Default)]
pub struct History {
    waiting: HashMap<TimerId, Request>,
    pub(crate) next_at: f64,
}
pub struct Request {
    owner: Box<CursorHistory<Row>>,
    kind: Kind,
    symbol: Option<String>,
    order: Option<String>,
    filter: Option<String>,
    start: u64,
    from: u64,
    to: u64,
    chunk_end: u64,
    /// Windows still to read after the current one, newest first.
    windows: VecDeque<(u64, u64)>,
    /// The range reaches past the newest windows the request budget allows.
    clipped: bool,
    page: u32,
    offset: Option<u64>,
    limit: usize,
    explicit_range: bool,
    page_rows: Vec<Row>,
    minimum_offset: Option<u64>,
    invalid: bool,
    retries: u8,
    requests: u32,
    page_count: u64,
    window_count: u64,
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
    #[serde(skip)]
    time_ns: u128,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: String,
    commission: f64,
    commission_asset: String,
    maker: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: String,
    #[serde(rename = "type")]
    kind: &'static str,
    status: &'static str,
    price: f64,
    average_price: f64,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: f64,
    updated_time: f64,
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
    side: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillRow<'a> {
    symbol: &'a str,
    trade_id: &'a str,
    order_id: &'a str,
    side: &'a str,
    liquidity: &'a str,
    #[serde(borrow)]
    price: &'a RawValue,
    #[serde(borrow)]
    size: &'a RawValue,
    #[serde(borrow)]
    fee: &'a RawValue,
    fee_currency: &'a str,
    #[serde(borrow)]
    trade_time: Option<&'a RawValue>,
    created_at: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderRow<'a> {
    id: &'a str,
    symbol: &'a str,
    side: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(borrow)]
    price: &'a RawValue,
    #[serde(borrow)]
    size: &'a RawValue,
    #[serde(borrow)]
    deal_size: &'a RawValue,
    #[serde(borrow, default)]
    avg_deal_price: Option<&'a RawValue>,
    #[serde(borrow, default)]
    deal_value: Option<&'a RawValue>,
    #[serde(default)]
    cancel_exist: bool,
    #[serde(default)]
    reduce_only: bool,
    created_at: u64,
    #[serde(default)]
    updated_at: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeRow<'a> {
    time: u64,
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(borrow)]
    amount: &'a RawValue,
    #[serde(borrow)]
    offset: &'a RawValue,
    currency: &'a str,
    status: &'a str,
    #[serde(default)]
    remark: &'a str,
}
#[derive(Deserialize)]
struct TradeRow<'a> {
    #[serde(rename = "tradeId")]
    trade_id: &'a str,
    #[serde(borrow)]
    ts: &'a RawValue,
    #[serde(borrow)]
    size: &'a RawValue,
    #[serde(borrow)]
    price: &'a RawValue,
    side: &'a str,
}
#[derive(Deserialize)]
struct Envelope<'a> {
    code: &'a str,
    #[serde(default)]
    msg: &'a str,
    #[serde(borrow)]
    data: Option<&'a RawValue>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    current_page: u32,
    total_page: u32,
    total_num: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ledger {
    has_more: bool,
}
fn number(value: &RawValue) -> f64 {
    safe_cex_engine::num::parse(value.get().trim_matches('"'))
}
fn integer(value: &RawValue) -> Option<u64> {
    value.get().trim_matches('"').parse().ok()
}
fn millis(value: f64) -> Option<u64> {
    (value.is_finite() && (0.0..=9_007_199_254_740_991.0).contains(&value) && value.fract() == 0.0)
        .then_some(value as u64)
}
impl Row {
    fn in_range(&self, start: u64, end: u64) -> bool {
        if let Self::Fill(row) = self {
            // Output seconds cannot round-trip exact millisecond boundaries.
            (u128::from(start) * 1_000_000..=u128::from(end) * 1_000_000).contains(&row.time_ns)
        } else {
            self.time() >= start as f64 && self.time() <= end as f64
        }
    }
    fn key(&self) -> String {
        match self {
            Self::Fill(r) => r.id.clone(),
            Self::Order(r) => r.id.clone(),
            Self::Income(r) => format!("{}:{}", r.tran_id, r.income_type),
            Self::Trade(r) => r.id.clone(),
        }
    }
    fn time(&self) -> f64 {
        match self {
            Self::Fill(r) => r.timestamp * 1000.0,
            Self::Order(r) => r.created_time,
            Self::Income(r) => r.timestamp,
            Self::Trade(r) => r.timestamp,
        }
    }
}
impl Kucoin {
    pub(crate) fn history_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        if matches!(&query, Query::Income { kind: Some(kind), .. } if kind == "FUNDING_FEE") {
            self.funding_command(cx, command, query);
            return;
        }
        let (kind, symbol, order, filter, from, to, limit) = match query {
            Query::OrderByClientId { .. } => {
                let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                    return;
                };
                let _ = cx.fail_context(
                    &context,
                    VenueError::new("Client ID query is not supported by this venue")
                        .kind(ErrorKind::Unsupported)
                        .outcome(Outcome::NotSent),
                );
                return;
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (Kind::Fills, symbol, order_id, None, from_ms, to_ms, limit),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (Kind::Orders, symbol, None, None, from_ms, to_ms, limit),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => (Kind::Income, symbol, None, kind, from_ms, to_ms, limit),
            Query::PublicTrades { symbol, limit } => {
                (Kind::Trades, Some(symbol), None, None, None, None, limit)
            }
        };
        if order.as_ref().is_some_and(|id| {
            id.is_empty()
                || id.len() > 100
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        }) {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Invalid KuCoin order ID")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        if kind != Kind::Trades && self.credentials.is_none() {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("KuCoin credentials required")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        if let Some(s) = &symbol
            && cx.store.market(s).is_none()
        {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new(format!("Unknown KuCoin market {s}"))
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        let end = to.unwrap_or_else(|| cx.now.floor());
        let start = from.unwrap_or((end - DAY as f64).max(0.0));
        let (Some(start), Some(end)) = (millis(start), millis(end)) else {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Invalid KuCoin query time range")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        };
        if start > end {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Invalid KuCoin query time range")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        let explicit_range = from.is_some() && to.is_some();
        let width = if kind == Kind::Income {
            DAY - 1
        } else {
            7 * DAY - 1
        };
        // Only the newest windows the request budget could ever reach are
        // built; a read that walks past them without reaching its limit fails,
        // as the request guard did when windows were generated lazily.
        let reachable = end.saturating_sub(u64::from(MAX_REQUESTS) * (width + 1) - 1);
        let clipped = start < reachable;
        // Millisecond integers below 2^53 tile exactly as f64.
        let Some(tiles) = paging::windows(
            start.max(reachable) as f64,
            end as f64,
            width as f64,
            WindowOrder::NewestFirst,
            MAX_REQUESTS as usize,
        ) else {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Invalid KuCoin query time range")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        };
        let mut windows: VecDeque<(u64, u64)> = tiles
            .into_iter()
            .map(|(a, b)| (a as u64, b as u64))
            .collect();
        let (first_start, first_end) = windows.pop_front().unwrap_or((start, end));
        let request = Request {
            owner: Box::new(CursorHistory::new(
                Completion::new(command),
                CursorWalk::new(MAX_REQUESTS),
                |_| "KuCoin query exceeded page safety bound".into(),
                |rows| serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string())),
            )),
            kind,
            symbol,
            order,
            filter,
            start: first_start,
            from: start,
            to: end,
            chunk_end: first_end,
            windows,
            page: 1,
            offset: None,
            limit: limit.unwrap_or(if kind == Kind::Trades { 100 } else { 1000 }) as usize,
            explicit_range,
            clipped,
            page_rows: Vec::new(),
            minimum_offset: None,
            invalid: false,
            retries: 0,
            requests: 0,
            page_count: 0,
            window_count: 0,
        };
        if request.limit == 0 {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.done_context(&context, "[]".into());
            return;
        }
        self.history_schedule(cx, request);
    }
    fn history_schedule(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        self.history_schedule_after(cx, request, 0.0);
    }
    fn history_schedule_after(&mut self, cx: &mut Cx<'_, Pending>, request: Request, delay: f64) {
        let at = self.history.next_at.max(cx.now + delay);
        self.history.next_at = at + 350.0;
        if at > cx.now {
            let timer = cx.set_timer(at - cx.now);
            self.history.waiting.insert(timer, request);
        } else {
            self.history_send(cx, request);
        }
    }
    fn history_send(&mut self, cx: &mut Cx<'_, Pending>, mut r: Request) {
        r.page_count = 0;
        r.page_rows.clear();
        r.minimum_offset = None;
        r.invalid = false;
        r.requests += 1;
        if r.requests > MAX_REQUESTS {
            r.owner.reject(
                cx,
                VenueError::new("KuCoin query exceeded page safety bound"),
            );
            return;
        }
        let path = match r.kind {
            Kind::Fills => "/api/v1/fills",
            Kind::Orders => "/api/v1/orders",
            Kind::Income => "/api/v1/transaction-history",
            Kind::Trades => "/api/v1/trade/history",
        };
        let mut params = Vec::new();
        if let Some(symbol) = &r.symbol {
            let id = &cx.store.market(symbol).unwrap().id;
            if r.kind != Kind::Income {
                params.push(format!("symbol={id}"));
            }
        }
        if r.kind == Kind::Orders {
            params.push("status=done".into());
        }
        if let Some(id) = &r.order {
            params.push(format!("orderId={id}"));
        }
        if r.kind != Kind::Trades {
            let (wire_start, wire_end) = if r.kind == Kind::Income && r.start == r.chunk_end {
                if r.start > 0 {
                    (r.start - 1, r.chunk_end)
                } else {
                    (0, 1)
                }
            } else {
                (r.start, r.chunk_end)
            };
            params.push(format!("startAt={wire_start}&endAt={wire_end}"));
            if r.kind == Kind::Income {
                params.push("currency=USDT&forward=false&maxCount=50".into());
                if let Some(offset) = r.offset {
                    params.push(format!("offset={offset}"));
                }
            } else {
                params.push(format!("currentPage={}&pageSize=50", r.page));
            }
        }
        let endpoint = format!("{path}?{}", params.join("&"));
        let at = match r.kind {
            Kind::Income => ArrayAt::Path(&["data", "dataList"]),
            Kind::Trades => ArrayAt::Path(&["data"]),
            _ => ArrayAt::Path(&["data", "items"]),
        };
        self.signed_streamed(cx, Method::Get, &endpoint, None, at, Pending::History(r));
    }
    pub(crate) fn history_item(&mut self, cx: &mut Cx<'_, Pending>, r: &mut Request, item: &str) {
        r.page_count += 1;
        let result = (|| -> Result<(), String> {
            let row = match r.kind {
                Kind::Fills => {
                    let v: FillRow = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let Some(m) = cx.store.markets().iter().find(|m| m.id == v.symbol) else {
                        return Ok(());
                    };
                    let (time_ns, timestamp) = if let Some(t) = v.trade_time {
                        let ns = integer(t).ok_or("invalid tradeTime")?;
                        (u128::from(ns), ns as f64 / 1e9)
                    } else {
                        (
                            u128::from(v.created_at) * 1_000_000,
                            v.created_at as f64 / 1000.0,
                        )
                    };
                    Row::Fill(Execution {
                        id: v.trade_id.into(),
                        timestamp,
                        time_ns,
                        symbol: m.symbol.clone(),
                        order_id: v.order_id.into(),
                        price: number(v.price),
                        qty: crate::account::base_amount(
                            number(v.size),
                            m.contract_size.unwrap_or(1.0),
                        ),
                        side: v.side.into(),
                        commission: number(v.fee),
                        commission_asset: v.fee_currency.into(),
                        maker: v.liquidity == "maker",
                    })
                }
                Kind::Orders => {
                    let v: OrderRow = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let Some(m) = cx.store.markets().iter().find(|m| m.id == v.symbol) else {
                        return Ok(());
                    };
                    let filled = crate::account::base_amount(
                        number(v.deal_size),
                        m.contract_size.unwrap_or(1.0),
                    );
                    let avg = v
                        .avg_deal_price
                        .map(number)
                        .filter(|n| *n > 0.0)
                        .unwrap_or_else(|| {
                            if filled > 0.0 {
                                v.deal_value.map(number).unwrap_or(0.0) / filled
                            } else {
                                0.0
                            }
                        });
                    Row::Order(HistoricalOrder {
                        id: v.id.into(),
                        symbol: m.symbol.clone(),
                        side: v.side.into(),
                        kind: if v.kind == "market" {
                            "market"
                        } else {
                            "limit"
                        },
                        status: if v.cancel_exist { "canceled" } else { "closed" },
                        price: number(v.price),
                        average_price: avg,
                        amount: crate::account::base_amount(
                            number(v.size),
                            m.contract_size.unwrap_or(1.0),
                        ),
                        filled,
                        reduce_only: v.reduce_only,
                        created_time: v.created_at as f64,
                        updated_time: if v.updated_at > 0 {
                            v.updated_at
                        } else {
                            v.created_at
                        } as f64,
                    })
                }
                Kind::Income => {
                    let v: IncomeRow = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    // Pending settlement rows use the sentinel offset -1; only
                    // completed records can advance the stable history cursor.
                    if v.status != "Completed" {
                        return Ok(());
                    }
                    let offset = integer(v.offset).ok_or("invalid ledger offset")?;
                    r.minimum_offset = Some(r.minimum_offset.map_or(offset, |old| old.min(offset)));
                    let (kind, category) = match v.kind.to_ascii_lowercase().as_str() {
                        "realisedpnl" | "realizedpnl" => ("REALIZED_PNL", "realized_pnl"),
                        "fundingfee" => ("FUNDING_FEE", "funding"),
                        "transferin" | "transferout" => ("TRANSFER", "transfer"),
                        "tradefee" | "fee" => ("COMMISSION", "commission"),
                        _ => (v.kind, "other"),
                    };
                    let symbol = cx
                        .store
                        .markets()
                        .iter()
                        .find(|m| m.id == v.remark)
                        .map_or_else(String::new, |m| m.symbol.clone());
                    if r.symbol.as_ref().is_some_and(|s| s != &symbol)
                        || r.filter.as_ref().is_some_and(|s| s != kind)
                    {
                        return Ok(());
                    }
                    Row::Income(Income {
                        symbol,
                        income_type: kind.into(),
                        income: number(v.amount),
                        asset: v.currency.into(),
                        timestamp: v.time as f64,
                        tran_id: offset.to_string(),
                        category,
                    })
                }
                Kind::Trades => {
                    let v: TradeRow = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let m = cx.store.market(r.symbol.as_ref().unwrap()).unwrap();
                    let price = number(v.price);
                    let qty =
                        crate::account::base_amount(number(v.size), m.contract_size.unwrap_or(1.0));
                    Row::Trade(Trade {
                        id: v.trade_id.into(),
                        symbol: m.symbol.clone(),
                        price,
                        qty,
                        quote_qty: qty * price,
                        timestamp: integer(v.ts).ok_or("invalid trade ts")? as f64 / 1e6,
                        side: v.side.into(),
                    })
                }
            };
            // Native trade time can cross a REST window's integer-ms boundary.
            let (start, end) = if r.kind == Kind::Fills {
                (r.from, r.to)
            } else {
                (r.start, r.chunk_end)
            };
            if r.kind == Kind::Trades || row.in_range(start, end) {
                r.page_rows.push(row);
            }
            Ok(())
        })();
        if result.is_err() {
            r.invalid = true;
        }
    }
    pub(crate) fn history_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
    ) {
        error.outcome = None;
        request.owner.reject(cx, error);
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        status: u16,
        body: &str,
    ) {
        let envelope = serde_json::from_str::<Envelope>(body);
        let valid = (200..300).contains(&status)
            && !r.invalid
            && envelope
                .as_ref()
                .is_ok_and(|e| e.code == "200000" && e.data.is_some());
        if !valid {
            let rejected = envelope.as_ref().is_ok_and(|e| e.code == "400005");
            if (status == 0 || status == 429 || status >= 500 || rejected)
                && r.retries < if rejected { 1 } else { 3 }
            {
                r.retries += 1;
                let delay = 1000.0 * r.retries as f64;
                self.history_schedule_after(cx, r, delay);
                return;
            }
            let message = envelope.map_or_else(
                |_| "Malformed KuCoin query response".into(),
                |e| format!("KuCoin {}: {}", e.code, e.msg),
            );
            self.history_finish(cx, r, Err(VenueError::new(message)));
            return;
        }
        let data = envelope.unwrap().data.unwrap();
        let more = if r.kind == Kind::Trades {
            false
        } else if r.kind == Kind::Income {
            match serde_json::from_str::<Ledger>(data.get()) {
                Ok(page) => page.has_more,
                Err(error) => {
                    self.history_finish(cx, r, Err(VenueError::new(error.to_string())));
                    return;
                }
            }
        } else {
            let page = match serde_json::from_str::<Page>(data.get()) {
                Ok(page) => page,
                Err(error) => {
                    self.history_finish(cx, r, Err(VenueError::new(error.to_string())));
                    return;
                }
            };
            if page.current_page != r.page
                || page.total_page > 10000
                || page.total_num > 0 && page.total_page == 0
                || page.current_page < page.total_page && r.page_count == 0
                || page.current_page >= page.total_page
                    && r.window_count + r.page_count < page.total_num
            {
                r.owner
                    .reject(cx, VenueError::new("Invalid KuCoin page census"));
                return;
            }
            page.current_page < page.total_page
        };
        r.window_count += r.page_count;
        let mut fresh = 0;
        for row in r.page_rows.drain(..) {
            if r.owner.fresh(row.key()) {
                r.owner.stage(row);
                fresh += 1;
            }
        }
        let capped = !(r.kind == Kind::Income && r.explicit_range)
            && r.owner.accepted_len() + r.owner.staged_len() >= r.limit;
        let mut next = None;
        if more && !capped {
            if r.kind == Kind::Income {
                let Some(offset) = r
                    .minimum_offset
                    .filter(|n| r.offset.is_none_or(|old| *n < old))
                else {
                    r.owner
                        .reject(cx, VenueError::new("KuCoin ledger cursor did not advance"));
                    return;
                };
                r.offset = Some(offset);
            } else {
                r.page += 1;
            }
            next = Some(format!(
                "{}:{}:{}:{:?}",
                r.start, r.chunk_end, r.page, r.offset
            ));
        } else if !capped && let Some((start, end)) = r.windows.pop_front() {
            r.start = start;
            r.chunk_end = end;
            r.page = 1;
            r.window_count = 0;
            r.offset = None;
            next = Some(format!("{start}:{end}:1:None"));
        } else if !capped && r.clipped {
            r.owner
                .reject(cx, VenueError::new("KuCoin history range is too large"));
            return;
        }
        match r.owner.advance(CursorPage {
            next,
            rows: r.page_count as usize,
            fresh,
            want_more: true,
        }) {
            CursorStep::More(_) => {
                r.retries = 0;
                self.history_schedule(cx, r);
            }
            CursorStep::Done => self.history_finish(cx, r, Ok(())),
            CursorStep::Stalled(_) => r.owner.reject(
                cx,
                VenueError::new("KuCoin query exceeded page safety bound"),
            ),
        }
    }
    fn history_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        r: Request,
        result: Result<(), VenueError>,
    ) {
        let limit = r.limit;
        let uncapped = r.kind == Kind::Income && r.explicit_range;
        r.owner.finish_with(cx, result, |mut rows| {
            rows.sort_by(|a, b| {
                b.time()
                    .total_cmp(&a.time())
                    .then_with(|| a.key().cmp(&b.key()))
            });
            if !uncapped {
                rows.truncate(limit);
            }
            serde_json::to_string(&rows).map_err(|e| VenueError::new(e.to_string()))
        });
    }
    pub(crate) fn history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(r) = self.history.waiting.remove(&timer) {
            self.history_send(cx, r);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
    use safe_cex_engine::{Action, Engine, Event, Notice, Response};
    fn engine() -> Engine<Kucoin> {
        let mut e = safe_cex_engine::test_support::seeded_engine(Kucoin::new(Options {
            key: Some("fixture-key".into()),
            secret: Some("fixture-secret".into()),
            passphrase: Some("fixture-passphrase".into()),
            ..Default::default()
        }));
        e.store_mut().set_markets(vec![Market {
            id: "XBTUSDTM".into(),
            symbol: "BTCUSDT".into(),
            pretty_symbol: None,
            is_perpetual: Some(true),
            is_read_only: None,
            is_spot: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 1.0,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 1.0,
                    max: 1000000.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 100.0,
                },
                price: None,
            },
            contract_size: Some(0.001),
            is_expiring: None,
            is_inverse: None,
            maker_fee: None,
            taker_fee: None,
        }]);
        e
    }
    fn query(e: &mut Engine<Kucoin>, query: Query) -> safe_cex_engine::HttpRequest {
        e.handle(
            2_000_000_000_000.0,
            Event::Command(safe_cex_engine::Command::Query { command: 77, query }),
        );
        e.drain_actions()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .unwrap()
    }
    fn reply(e: &mut Engine<Kucoin>, r: safe_cex_engine::HttpRequest, body: &str) -> Vec<Action> {
        e.handle(
            2_000_000_000_000.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        e.drain_actions().collect()
    }
    fn done(actions: &[Action]) -> &str {
        actions
            .iter()
            .find_map(|a| {
                if let Action::Notice(Notice::CommandDone { result: s, .. }) = a {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap()
    }
    #[test]
    fn client_id_lookup_fails_explicitly_without_network_io() {
        let mut e = safe_cex_engine::test_support::seeded_engine(Kucoin::new(Options::default()));
        e.handle(
            1740640088002.5,
            Event::Command(safe_cex_engine::Command::Query {
                command: 77,
                query: Query::OrderByClientId {
                    symbol: "BTCUSDT".into(),
                    client_order_id: "caller-id".into(),
                },
            }),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Notice(Notice::CommandFailed { command: 77, error })
                if error.legacy_text() == "Client ID query is not supported by this venue"
        ));
    }
    #[test]
    fn documented_fill_converts_lots_and_nanosecond_time() {
        let mut e = engine();
        let r = query(
            &mut e,
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(1740640000000.0),
                to_ms: Some(1740641000000.0),
                limit: Some(10),
            },
        );
        let actions = reply(
            &mut e,
            r,
            r#"{"code":"200000","data":{"currentPage":1,"totalPage":1,"totalNum":1,"items":[{"symbol":"XBTUSDTM","tradeId":"1828954878212","orderId":"284486580251463680","side":"buy","liquidity":"taker","price":"86275.1","size":1,"fee":"0.05176506","feeCurrency":"USDT","tradeTime":1740640088244000000,"createdAt":1740640088427}]}}"#,
        );
        let rows: Vec<ExecutionCheck> = serde_json::from_str(done(&actions)).unwrap();
        assert_eq!(rows[0].qty, 0.001);
        assert_eq!(rows[0].timestamp, 1740640088.244);
        assert_eq!(rows[0].commission, 0.05176506);
    }
    #[derive(Deserialize)]
    struct ExecutionCheck {
        qty: f64,
        timestamp: f64,
        commission: f64,
    }
    #[test]
    fn fill_range_uses_exact_native_time_before_seconds_conversion() {
        const AT: u64 = 1_740_640_088_002;
        for nanos in [true, false] {
            let mut e = engine();
            let r = query(
                &mut e,
                Query::Fills {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from_ms: Some(AT as f64),
                    to_ms: Some(AT as f64),
                    limit: Some(10),
                },
            );
            let items = [-1_i128, 0, 1]
                .into_iter()
                .map(|delta| {
                    let created = (i128::from(AT) + delta) as u64;
                    let time = if nanos {
                        format!(",\"tradeTime\":{}", i128::from(AT) * 1_000_000 + delta)
                    } else {
                        String::new()
                    };
                    format!(
                        r#"{{"symbol":"XBTUSDTM","tradeId":"{delta}","orderId":"order","side":"buy","liquidity":"maker","price":"100","size":1,"fee":"0","feeCurrency":"USDT","createdAt":{created}{time}}}"#
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let body = format!(
                r#"{{"code":"200000","data":{{"currentPage":1,"totalPage":1,"totalNum":3,"items":[{items}]}}}}"#
            );
            let actions = reply(&mut e, r, &body);
            #[derive(Deserialize)]
            struct Id {
                id: String,
            }
            let rows: Vec<Id> = serde_json::from_str(done(&actions)).unwrap();
            assert_eq!(rows.len(), 1, "nanosecond timestamp: {nanos}");
            assert_eq!(rows[0].id, "0");
        }
    }
    #[test]
    fn a_huge_range_builds_only_reachable_windows_and_a_limited_read_still_succeeds() {
        // to = MAX_SAFE_INTEGER must not tile ~10^8 windows before the first request.
        let mut e = engine();
        let r = query(
            &mut e,
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(0.0),
                to_ms: Some(9_007_199_254_740_991.0),
                limit: Some(1),
            },
        );
        assert!(r.url.contains("endAt=9007199254740991"), "{}", r.url);
        // An old start with no end reaches its limit in the newest window.
        let mut e = engine();
        let r = query(
            &mut e,
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(0.0),
                to_ms: None,
                limit: Some(1),
            },
        );
        let body = r#"{"code":"200000","data":{"hasMore":false,"dataList":[{"time":1999999999000,"type":"RealisedPNL","amount":1,"fee":0,"accountEquity":1,"status":"Completed","remark":"XBTUSDTM","offset":5,"currency":"USDT"}]}}"#;
        let actions = reply(&mut e, r, body);
        assert!(done(&actions).contains("1999999999"), "{actions:?}");
    }
    /// Pins the exact request sequence of a windowed, page-numbered read:
    /// 7-day windows newest first, the first walked over two pages. Paging
    /// refactors keep it.
    #[test]
    fn windowed_fill_request_sequence_is_pinned() {
        const END: u64 = 1_740_640_088_002;
        let mut e = engine();
        let mut r = query(
            &mut e,
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some((END - 15 * DAY) as f64),
                to_ms: Some(END as f64),
                limit: Some(1000),
            },
        );
        let fill = |id: &str, created: u64| {
            format!(
                r#"{{"symbol":"XBTUSDTM","tradeId":"{id}","orderId":"order","side":"buy","liquidity":"maker","price":"100","size":1,"fee":"0","feeCurrency":"USDT","createdAt":{created},"tradeTime":{}}}"#,
                u128::from(created) * 1_000_000
            )
        };
        let pages = [
            (1, 2, 2, fill("a", END - 1)),
            (2, 2, 2, fill("b", END - 2)),
            (1, 1, 0, String::new()),
            (1, 1, 0, String::new()),
        ];
        let mut sent = Vec::new();
        for (index, (page, total_pages, total, items)) in pages.iter().enumerate() {
            sent.push(r.url.split_once("/api/").unwrap().1.to_owned());
            let body = format!(
                r#"{{"code":"200000","data":{{"currentPage":{page},"totalPage":{total_pages},"totalNum":{total},"items":[{items}]}}}}"#
            );
            let actions = reply(&mut e, r, &body);
            if index + 1 == pages.len() {
                assert!(!done(&actions).is_empty());
                break;
            }
            let (timer, at) = actions
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                    _ => None,
                })
                .unwrap();
            e.handle(at, Event::TimerFired { timer });
            r = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::Http(r) => Some(r),
                    _ => None,
                })
                .unwrap();
        }
        assert_eq!(
            sent,
            [
                "v1/fills?symbol=XBTUSDTM&startAt=1740035288003&endAt=1740640088002&currentPage=1&pageSize=50",
                "v1/fills?symbol=XBTUSDTM&startAt=1740035288003&endAt=1740640088002&currentPage=2&pageSize=50",
                "v1/fills?symbol=XBTUSDTM&startAt=1739430488003&endAt=1740035288002&currentPage=1&pageSize=50",
                "v1/fills?symbol=XBTUSDTM&startAt=1739344088002&endAt=1739430488002&currentPage=1&pageSize=50",
            ]
        );
    }
    #[test]
    fn fill_native_time_survives_millisecond_window_boundaries() {
        const AT: u64 = 1_740_640_088_002;
        let end = AT + 7 * DAY;
        for newer_page in [false, true] {
            let mut e = engine();
            let r = query(
                &mut e,
                Query::Fills {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from_ms: Some(AT as f64),
                    to_ms: Some(end as f64),
                    limit: Some(1000),
                },
            );
            assert!(r.url.contains(&format!("startAt={}&endAt={end}", AT + 1)));
            let fill = |id: &str, created: u64, nanos: u128| {
                format!(
                    r#"{{"symbol":"XBTUSDTM","tradeId":"{id}","orderId":"order","side":"buy","liquidity":"maker","price":"100","size":1,"fee":"0","feeCurrency":"USDT","createdAt":{created},"tradeTime":{nanos}}}"#
                )
            };
            let row = fill(
                "boundary",
                AT + u64::from(newer_page),
                u128::from(AT) * 1_000_000 + 500_000,
            );
            let mut first = vec![
                fill("before", AT + 1, u128::from(AT) * 1_000_000 - 1),
                fill("after", end, u128::from(end) * 1_000_000 + 1),
            ];
            if newer_page {
                first.push(row.clone());
            }
            let body = format!(
                r#"{{"code":"200000","data":{{"currentPage":1,"totalPage":1,"totalNum":{},"items":[{}]}}}}"#,
                first.len(),
                first.join(",")
            );
            let actions = reply(&mut e, r, &body);
            let (timer, at) = actions
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                    _ => None,
                })
                .unwrap();
            e.handle(at, Event::TimerFired { timer });
            let r = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::Http(r) => Some(r),
                    _ => None,
                })
                .unwrap();
            assert!(r.url.contains(&format!("startAt={AT}&endAt={AT}")));
            let second = if newer_page { "" } else { &row };
            let body = format!(
                r#"{{"code":"200000","data":{{"currentPage":1,"totalPage":1,"totalNum":{},"items":[{second}]}}}}"#,
                usize::from(!newer_page)
            );
            let actions = reply(&mut e, r, &body);
            #[derive(Deserialize)]
            struct Id {
                id: String,
            }
            let rows: Vec<Id> = serde_json::from_str(done(&actions)).unwrap();
            assert_eq!(rows.len(), 1, "createdAt in newer window: {newer_page}");
            assert_eq!(rows[0].id, "boundary");
        }
    }
    #[test]
    fn default_query_ranges_accept_fractional_native_clock() {
        for query in [
            Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: None,
            },
            Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
            Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        ] {
            let mut e = engine();
            e.handle(
                1_740_640_088_002.5,
                Event::Command(safe_cex_engine::Command::Query { command: 77, query }),
            );
            let actions = e.drain_actions().collect::<Vec<_>>();
            let request = actions
                .iter()
                .find_map(|a| {
                    if let Action::Http(r) = a {
                        Some(r)
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| panic!("{actions:?}"));
            if !request.url.contains("/trade/history") {
                assert!(request.url.contains("endAt=1740640088002"));
            }
        }
    }
    #[test]
    fn explicit_fractional_query_ranges_remain_invalid() {
        for (from_ms, to_ms) in [(Some(1000.5), Some(2000.0)), (Some(1000.0), Some(2000.5))] {
            let mut e = engine();
            e.handle(
                2000.5,
                Event::Command(safe_cex_engine::Command::Query {
                    command: 77,
                    query: Query::Fills {
                        symbol: None,
                        order_id: None,
                        from_ms,
                        to_ms,
                        limit: None,
                    },
                }),
            );
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
            );
        }
    }
    #[test]
    fn ledger_offset_is_exact_and_does_not_depend_on_row_order() {
        let mut e = engine();
        let r = query(
            &mut e,
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(1000.0),
                to_ms: Some(2000.0),
                limit: Some(1),
            },
        );
        let actions = reply(
            &mut e,
            r,
            r#"{"code":"200000","data":{"dataList":[{"time":1500,"type":"RealisedPNL","amount":2,"offset":9007199254740999,"currency":"USDT","status":"Completed","remark":"XBTUSDTM"},{"time":1400,"type":"TransferIn","amount":-0.1,"offset":9007199254740993,"currency":"USDT","status":"Completed","remark":"XBTUSDTM"}],"hasMore":true}}"#,
        );
        let (timer, at) = actions
            .iter()
            .find_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a {
                    Some((*timer, *at_ms))
                } else {
                    None
                }
            })
            .unwrap();
        e.handle(at, Event::TimerFired { timer });
        let request = e
            .drain_actions()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(request.url.contains("offset=9007199254740993"));
        let actions = reply(
            &mut e,
            request,
            r#"{"code":"200000","data":{"dataList":[],"hasMore":false}}"#,
        );
        let text = done(&actions);
        assert!(text.contains("9007199254740993"));
        assert!(text.contains("9007199254740999"));
    }
    #[test]
    fn malformed_page_rejects_instead_of_committing_partial_success() {
        let mut e = engine();
        let r = query(
            &mut e,
            Query::OrderHistory {
                symbol: None,
                from_ms: Some(1000.0),
                to_ms: Some(2000.0),
                limit: None,
            },
        );
        let actions = reply(
            &mut e,
            r,
            r#"{"code":"200000","data":{"currentPage":2,"totalPage":2,"totalNum":10,"items":[]}}"#,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
    #[test]
    fn public_trades_need_no_credentials() {
        let mut e = safe_cex_engine::test_support::seeded_engine(Kucoin::new(Options::default()));
        e.store_mut()
            .set_markets(engine().store().markets().to_vec());
        let r = query(
            &mut e,
            Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(1),
            },
        );
        let actions = reply(
            &mut e,
            r,
            r#"{"code":"200000","data":[{"tradeId":"1697915257909","ts":1729242032152000000,"size":1,"price":"67878","side":"sell"}]}"#,
        );
        assert!(done(&actions).contains("\"qty\":0.001"));
    }
    #[test]
    fn ledger_single_millisecond_window_uses_valid_native_range() {
        let mut e = engine();
        let r = query(
            &mut e,
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(1000.0),
                to_ms: Some(1000.0),
                limit: None,
            },
        );
        assert!(r.url.contains("startAt=999&endAt=1000"));
    }
    #[test]
    fn order_id_cannot_inject_signed_query_fields() {
        let mut e = engine();
        e.handle(
            2000.0,
            Event::Command(safe_cex_engine::Command::Query {
                command: 1,
                query: Query::Fills {
                    symbol: None,
                    order_id: Some("a&symbol=other".into()),
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
    #[test]
    fn pending_settlement_sentinel_does_not_corrupt_completed_ledger_cursor() {
        let mut e = engine();
        let r = query(
            &mut e,
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: Some(1000.0),
                to_ms: Some(2000.0),
                limit: None,
            },
        );
        let actions = reply(
            &mut e,
            r,
            r#"{"code":"200000","data":{"dataList":[{"time":1500,"type":"RealisedPNL","amount":0,"offset":-1,"currency":"USDT","status":"Pending","remark":"XBTUSDTM"},{"time":1400,"type":"RealisedPNL","amount":2,"offset":7,"currency":"USDT","status":"Completed","remark":"XBTUSDTM"}],"hasMore":false}}"#,
        );
        let result = done(&actions);
        assert!(result.contains("\"tranId\":\"7\""));
        assert!(!result.contains("-1"));
    }
}
