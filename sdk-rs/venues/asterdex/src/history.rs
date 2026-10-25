//! Typed rare reads; Aster executions carry cumulative PnL, unlike Binance.
use crate::{AsterDex, Pending, account, sign::Query};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Interval, OrderStatus, OrderType};
use safe_cex_engine::num::parse_decimal;
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{Command, Cx, Query as Read, Response, TimerId};
use safe_cex_venue_binance::wire::{HistoricalOrderRow, UserTrade};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

const MAX_WINDOW_MS: f64 = 7.0 * 24.0 * 3_600_000.0;
const INCOME_CHUNK_DELAY_MS: f64 = 2_000.0;

pub(crate) enum LocalRead<'a> {
    History(Result<&'static str, &'static str>),
    Leverage(&'a str),
    Unsupported(Interval),
}

#[derive(Default)]
pub struct HistoryState {
    delayed: HashMap<TimerId, (Context, DelayedWalk)>,
}

enum DelayedWalk {
    Income(Box<IncomeWalk>),
    Fills(Box<FillWalk>),
}

pub struct FillWalk {
    walk: RangeWalk,
    window: (f64, f64),
    end: f64,
    single_millisecond: bool,
    symbol: String,
    order_id: Option<String>,
    limit: u32,
    collected: Vec<Execution>,
}

pub struct IncomeWalk {
    walk: RangeWalk,
    window: (f64, f64),
    symbol: Option<String>,
    kind: Option<String>,
    limit: u32,
    collected: Vec<Income>,
}

pub enum Request {
    Fills {
        root: Context,
        rows: Vec<Execution>,
        invalid: bool,
        range: Option<Box<FillWalk>>,
    },
    Orders {
        root: Context,
        rows: Vec<HistoricalOrder>,
        invalid: bool,
    },
    Income {
        root: Context,
        rows: Vec<Income>,
        invalid: bool,
        range: Option<Box<IncomeWalk>>,
    },
    Trades {
        root: Context,
        symbol: String,
        rows: Vec<Trade>,
        invalid: bool,
    },
}
impl Request {
    pub(crate) fn root(&self) -> &Context {
        match self {
            Self::Fills { root, .. }
            | Self::Orders { root, .. }
            | Self::Income { root, .. }
            | Self::Trades { root, .. } => root,
        }
    }

    pub(crate) fn reset(&mut self) {
        match self {
            Self::Fills { rows, invalid, .. } => {
                rows.clear();
                *invalid = false
            }
            Self::Orders { rows, invalid, .. } => {
                rows.clear();
                *invalid = false
            }
            Self::Income { rows, invalid, .. } => {
                rows.clear();
                *invalid = false
            }
            Self::Trades { rows, invalid, .. } => {
                rows.clear();
                *invalid = false
            }
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Execution {
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
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
    #[serde(skip)]
    time_ms: f64,
    #[serde(skip)]
    pnl_group: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalOrder {
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
pub struct Income {
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
pub struct Trade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    timestamp: f64,
    side: &'static str,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicTrade<'a> {
    id: u64,
    #[serde(borrow)]
    price: &'a str,
    #[serde(borrow)]
    qty: &'a str,
    #[serde(default, borrow)]
    quote_qty: Option<&'a str>,
    time: f64,
    is_buyer_maker: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NativeId<'a> {
    Number(u64),
    Text(#[serde(borrow)] Cow<'a, str>),
}

impl std::fmt::Display for NativeId<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(value) => value.fmt(formatter),
            Self::Text(value) => value.fmt(formatter),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeRow<'a> {
    #[serde(default, borrow)]
    symbol: Cow<'a, str>,
    #[serde(borrow)]
    income_type: Cow<'a, str>,
    #[serde(borrow)]
    income: Cow<'a, str>,
    #[serde(borrow)]
    asset: Cow<'a, str>,
    time: f64,
    #[serde(borrow)]
    tran_id: NativeId<'a>,
    #[serde(default, borrow)]
    trade_id: Option<Cow<'a, str>>,
}

#[derive(Deserialize)]
struct OrderRow<'a> {
    #[serde(flatten, borrow)]
    order: HistoricalOrderRow<'a>,
    #[serde(rename = "closePosition", default)]
    close_position: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ErrorCode<'a> {
    Number(i64),
    Text(#[serde(borrow)] Cow<'a, str>),
}

#[derive(Deserialize)]
struct ErrorBody<'a> {
    #[serde(borrow)]
    code: ErrorCode<'a>,
    #[serde(default, borrow)]
    msg: Cow<'a, str>,
}

fn native_error(body: &str) -> Option<String> {
    let error = serde_json::from_str::<ErrorBody>(body).ok()?;
    let code = match error.code {
        ErrorCode::Number(code) => code,
        ErrorCode::Text(code) => code.parse().ok()?,
    };
    (code < 0).then(|| format!("Aster API error {code}: {}", error.msg))
}
fn nonzero(text: &str) -> Option<f64> {
    parse_decimal(text).filter(|v| *v != 0.0)
}
fn side(text: &str) -> &'static str {
    if text == "SELL" { "sell" } else { "buy" }
}
fn rail(text: Option<&str>) -> &'static str {
    match text.map(str::to_ascii_uppercase).as_deref() {
        Some("LONG") => "long",
        Some("SHORT") => "short",
        _ => "both",
    }
}
fn category(text: &str) -> &'static str {
    match text {
        "REALIZED_PNL" => "realized_pnl",
        "FUNDING_FEE" => "funding",
        "COMMISSION" => "commission",
        "COMMISSION_REBATE" | "REFERRAL_KICKBACK" | "API_REBATE" | "MARKET_MAKER_REBATE" => {
            "rebate"
        }
        "TRANSFER" | "INTERNAL_TRANSFER" | "AUTO_EXCHANGE" | "CROSS_COLLATERAL_TRANSFER" => {
            "transfer"
        }
        "INSURANCE_CLEAR" => "liquidation",
        _ => "other",
    }
}
fn normalize(rows: &mut [Execution]) {
    let mut indices: Vec<usize> = (0..rows.len()).collect();
    indices.sort_by(|&a, &b| {
        rows[a]
            .time_ms
            .total_cmp(&rows[b].time_ms)
            .then_with(|| {
                rows[a]
                    .id
                    .parse::<u64>()
                    .unwrap_or(0)
                    .cmp(&rows[b].id.parse::<u64>().unwrap_or(0))
            })
            .then(a.cmp(&b))
    });
    let mut previous = HashMap::<(String, String), f64>::new();
    for i in indices {
        let row = &mut rows[i];
        if let Some(cumulative) = row.realized_pnl {
            let old = previous.insert((row.symbol.clone(), row.pnl_group.clone()), cumulative);
            row.realized_pnl = Some(old.map_or(0.0, |p| {
                let delta = cumulative - p;
                if delta.is_finite() { delta } else { 0.0 }
            }));
        }
    }
}
impl AsterDex {
    pub(crate) fn local_read<'a>(&self, command: &'a Command) -> Option<LocalRead<'a>> {
        let Command::Query { query: read, .. } = command else {
            return match command {
                Command::FetchLeverage { symbol, .. } => Some(LocalRead::Leverage(symbol)),
                Command::FetchCandles { interval, .. } if !crate::supported(*interval) => {
                    Some(LocalRead::Unsupported(*interval))
                }
                _ => None,
            };
        };
        if !self.credentials.has_primary() && !matches!(read, Read::PublicTrades { .. }) {
            return Some(LocalRead::History(Ok("[]")));
        }
        let (from, to, bounded, income) = match read {
            Read::Fills { from_ms, to_ms, .. } | Read::OrderHistory { from_ms, to_ms, .. } => (
                *from_ms,
                *to_ms,
                matches!(read, Read::OrderHistory { .. }),
                false,
            ),
            Read::Income { from_ms, to_ms, .. } => (*from_ms, *to_ms, false, true),
            _ => (None, None, false, false),
        };
        if from
            .into_iter()
            .chain(to)
            .any(|value| !value.is_finite() || value < 0.0)
            || from
                .zip(to)
                .is_some_and(|(from, to)| to < from || (income && to == from))
        {
            return Some(LocalRead::History(Err("Invalid Aster history time range")));
        }
        if bounded
            && from
                .zip(to)
                .is_some_and(|(from, to)| to - from > MAX_WINDOW_MS)
        {
            return Some(LocalRead::History(Err(
                "Aster order history requires a time window of at most seven days",
            )));
        }
        match read {
            Read::OrderByClientId { .. } => Some(LocalRead::History(Err(
                "Aster does not support caller client order IDs",
            ))),
            Read::Fills {
                symbol,
                from_ms,
                to_ms,
                ..
            } => {
                if symbol.as_deref().is_none_or(str::is_empty) {
                    return Some(LocalRead::History(Ok("[]")));
                }
                if from_ms.zip(*to_ms).is_some_and(|(_, to)| to + 1.0 <= to) {
                    return Some(LocalRead::History(Err("Invalid Aster history time range")));
                }
                None
            }
            Read::OrderHistory { symbol, .. }
                if self.version == crate::sign::Version::V1
                    && symbol.as_deref().is_none_or(str::is_empty) =>
            {
                Some(LocalRead::History(Ok("[]")))
            }
            _ => None,
        }
    }

    pub(crate) fn start_query(&mut self, cx: &mut Cx<'_, Pending>, read: Read) {
        let Some(root) = crate::request::command_read_root(cx) else {
            return;
        };
        let (endpoint, query, pending, private) = match read {
            Read::OrderByClientId { .. } => unreachable!("local read handled at command entry"),
            Read::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                let symbol = symbol.expect("local read handled at command entry");
                if let Some((from, to)) = from_ms.zip(to_ms) {
                    let limit = limit.unwrap_or(500).clamp(1, 1000);
                    self.next_fill_page(
                        cx,
                        root,
                        Box::new(FillWalk {
                            // RangeWalk needs a positive span. Clip its requests back to
                            // the exact caller end, including a single-millisecond read.
                            walk: RangeWalk::new(
                                from,
                                to.max(from + 1.0),
                                MAX_WINDOW_MS,
                                limit as usize,
                            ),
                            window: (from, to),
                            end: to,
                            single_millisecond: from == to,
                            symbol,
                            order_id: order_id.filter(|id| !id.is_empty()),
                            limit,
                            collected: Vec::new(),
                        }),
                    );
                    return;
                }
                (
                    "userTrades",
                    Query::new()
                        .str("symbol", &symbol)
                        .num("limit", f64::from(limit.unwrap_or(500).clamp(1, 1000)))
                        .opt_str("orderId", order_id.as_deref())
                        .opt_num("startTime", from_ms)
                        .opt_num("endTime", to_ms),
                    Request::Fills {
                        root,
                        rows: Vec::new(),
                        invalid: false,
                        range: None,
                    },
                    true,
                )
            }
            Read::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                "allOrders",
                Query::new()
                    .opt_str("symbol", symbol.as_deref())
                    .num("limit", f64::from(limit.unwrap_or(500).clamp(1, 1000)))
                    .opt_num("startTime", from_ms)
                    .opt_num("endTime", to_ms),
                Request::Orders {
                    root,
                    rows: Vec::new(),
                    invalid: false,
                },
                true,
            ),
            Read::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                if let Some((from, to)) = from_ms.zip(to_ms) {
                    let limit = limit.unwrap_or(1000).clamp(1, 1000);
                    self.next_income_page(
                        cx,
                        root,
                        Box::new(IncomeWalk {
                            walk: RangeWalk::new(from, to, MAX_WINDOW_MS, limit as usize),
                            window: (from, to),
                            symbol,
                            kind,
                            limit,
                            collected: Vec::new(),
                        }),
                    );
                    return;
                }
                (
                    "income",
                    Query::new()
                        .opt_str("symbol", symbol.as_deref())
                        .opt_str("incomeType", kind.as_deref())
                        .opt_num("startTime", from_ms)
                        .opt_num("endTime", to_ms)
                        .num("limit", f64::from(limit.unwrap_or(100).clamp(1, 1000))),
                    Request::Income {
                        root,
                        rows: Vec::new(),
                        invalid: false,
                        range: None,
                    },
                    true,
                )
            }
            Read::PublicTrades { symbol, limit } => (
                "trades",
                Query::new()
                    .str("symbol", &symbol)
                    .num("limit", f64::from(limit.unwrap_or(500).clamp(1, 1000))),
                Request::Trades {
                    root,
                    symbol,
                    rows: Vec::new(),
                    invalid: false,
                },
                false,
            ),
        };
        let path = self.path(endpoint);
        if private {
            self.signed_streamed(cx, &path, query, Pending::History(pending));
        } else {
            self.get_streamed(
                cx,
                &format!("{path}?{}", query.as_str()),
                ArrayAt::Root,
                Pending::History(pending),
            );
        }
    }
    pub(crate) fn on_history_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Fills { rows, invalid, .. } => {
                let Ok(r) = serde_json::from_str::<UserTrade>(item) else {
                    *invalid = true;
                    return;
                };
                // TS normalizes cumulative PnL across all native rows before
                // filtering invalid execution sizes. Keep that baseline intact.
                let price = parse_decimal(&r.price).unwrap_or(f64::NAN);
                let qty = parse_decimal(&r.qty).unwrap_or(f64::NAN);
                rows.push(Execution {
                    id: r.id.to_string(),
                    timestamp: (r.time / 1000.0).floor(),
                    time_ms: r.time,
                    symbol: r.symbol.to_string(),
                    order_id: r.order_id.to_string(),
                    price,
                    qty,
                    side: side(&r.side),
                    commission: r.commission.as_deref().and_then(parse_decimal),
                    commission_asset: r.commission_asset.map(|s| s.into_owned()),
                    position_side: rail(r.position_side.as_deref()),
                    pnl_group: r
                        .position_side
                        .as_deref()
                        .filter(|side| !side.is_empty())
                        .unwrap_or("BOTH")
                        .to_ascii_uppercase(),
                    realized_pnl: r.realized_pnl.as_deref().and_then(parse_decimal),
                    maker: r.maker,
                });
            }
            Request::Orders { rows, invalid, .. } => {
                let Ok(row) = serde_json::from_str::<OrderRow>(item) else {
                    *invalid = true;
                    return;
                };
                let r = row.order;
                let Some(kind) = account::order_type(r.orig_type.as_deref().unwrap_or(&r.kind))
                else {
                    *invalid = true;
                    return;
                };
                let status = match &*r.status {
                    "NEW" | "PARTIALLY_FILLED" => OrderStatus::Open,
                    "FILLED" => OrderStatus::Closed,
                    "CANCELED" | "EXPIRED" | "REJECTED" | "EXPIRED_IN_MATCH" => {
                        OrderStatus::Canceled
                    }
                    _ => {
                        *invalid = true;
                        return;
                    }
                };
                let (Some(amount), Some(filled)) =
                    (parse_decimal(&r.orig_qty), parse_decimal(&r.executed_qty))
                else {
                    *invalid = true;
                    return;
                };
                rows.push(HistoricalOrder {
                    id: r.order_id.to_string(),
                    symbol: r.symbol.to_string(),
                    side: side(&r.side),
                    kind,
                    status,
                    price: nonzero(&r.price),
                    average_price: r.avg_price.as_deref().and_then(nonzero),
                    amount,
                    filled,
                    reduce_only: r.reduce_only || row.close_position,
                    position_side: rail(r.position_side.as_deref()),
                    created_time: r.time,
                    updated_time: (r.update_time > 0.0).then_some(r.update_time),
                });
            }
            Request::Income { rows, invalid, .. } => {
                let Ok(r) = serde_json::from_str::<IncomeRow>(item) else {
                    *invalid = true;
                    return;
                };
                let Some(income) = parse_decimal(&r.income) else {
                    *invalid = true;
                    return;
                };
                rows.push(Income {
                    symbol: r.symbol.to_string(),
                    income_type: r.income_type.to_string(),
                    income,
                    asset: r.asset.to_string(),
                    timestamp: r.time,
                    tran_id: r.tran_id.to_string(),
                    trade_id: r.trade_id.map(|s| s.into_owned()).filter(|s| !s.is_empty()),
                    category: category(&r.income_type),
                });
            }
            Request::Trades {
                symbol,
                rows,
                invalid,
                ..
            } => {
                let Ok(r) = serde_json::from_str::<PublicTrade>(item) else {
                    *invalid = true;
                    return;
                };
                let (Some(price), Some(qty)) = (parse_decimal(r.price), parse_decimal(r.qty))
                else {
                    *invalid = true;
                    return;
                };
                rows.push(Trade {
                    id: r.id.to_string(),
                    symbol: symbol.clone(),
                    price,
                    qty,
                    quote_qty: r.quote_qty.and_then(parse_decimal).unwrap_or(price * qty),
                    timestamp: r.time,
                    side: if r.is_buyer_maker { "sell" } else { "buy" },
                });
            }
        }
    }
    pub(crate) fn on_history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let error = match response {
            Ok(r) => account::venue_error(&r)
                .map(|(_, m)| m)
                .or_else(|| native_error(r.body)),
            Err(e) => Some(e.into()),
        };
        let (root, invalid, result) = match request {
            Request::Fills {
                root,
                rows,
                invalid,
                range: Some(mut range),
            } => {
                if let Some(error) = error {
                    let _ = cx.fail_context(&root, error);
                } else if range.single_millisecond && rows.len() >= range.limit as usize {
                    let _ = cx.fail_context(
                        &root,
                        "Aster fills timestamp saturated; no partial result returned",
                    );
                } else if invalid
                    || rows.iter().any(|row| {
                        row.symbol != range.symbol
                            || row.id.is_empty()
                            || !row.time_ms.is_finite()
                            || row.time_ms.fract() != 0.0
                            || row.time_ms < range.window.0
                            || row.time_ms > range.window.1
                    })
                {
                    let _ = cx.fail_context(&root, "Malformed Aster history response");
                } else {
                    let metadata = rows
                        .iter()
                        .map(|row| PageRow {
                            timestamp: row.time_ms,
                            identity: format!("{}:{}", row.symbol, row.id),
                        })
                        .collect::<Vec<_>>();
                    let fresh = range.walk.page(range.window.0, range.window.1, &metadata);
                    range
                        .collected
                        .extend(rows.into_iter().enumerate().filter_map(|(index, row)| {
                            fresh.binary_search(&index).is_ok().then_some(row)
                        }));
                    if range.single_millisecond {
                        self.finish_fill_range(cx, root, *range);
                    } else {
                        self.next_fill_page(cx, root, range);
                    }
                }
                return;
            }
            Request::Fills {
                root,
                mut rows,
                invalid,
                range: None,
            } => {
                normalize(&mut rows);
                rows.retain(|row| row.price.is_finite() && row.qty.is_finite() && row.qty > 0.0);
                (root, invalid, serde_json::to_string(&rows))
            }
            Request::Orders {
                root,
                mut rows,
                invalid,
            } => {
                rows.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
                (root, invalid, serde_json::to_string(&rows))
            }
            Request::Income {
                root,
                rows,
                invalid,
                range: Some(mut range),
            } => {
                if let Some(error) = error {
                    let _ = cx.fail_context(&root, error);
                } else if invalid {
                    let _ = cx.fail_context(&root, "Malformed Aster history response");
                } else {
                    let metadata = rows
                        .iter()
                        .map(|row| PageRow {
                            timestamp: row.timestamp,
                            identity: format!("{}:{}", row.income_type, row.tran_id),
                        })
                        .collect::<Vec<_>>();
                    let fresh = range.walk.page(range.window.0, range.window.1, &metadata);
                    range
                        .collected
                        .extend(rows.into_iter().enumerate().filter_map(|(index, row)| {
                            fresh.binary_search(&index).is_ok().then_some(row)
                        }));
                    self.next_income_page(cx, root, range);
                }
                return;
            }
            Request::Income {
                root,
                rows,
                invalid,
                range: None,
            } => (root, invalid, serde_json::to_string(&rows)),
            Request::Trades {
                root,
                rows,
                invalid,
                ..
            } => (root, invalid, serde_json::to_string(&rows)),
        };
        let result = if let Some(e) = error {
            Err(e)
        } else if invalid {
            Err("Malformed Aster history response".into())
        } else {
            result.map_err(|e| e.to_string())
        };
        let _ = match result {
            Ok(data) => cx.done_context(&root, data),
            Err(error) => cx.fail_context(&root, error),
        };
    }

    fn next_income_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        root: Context,
        mut range: Box<IncomeWalk>,
    ) {
        match range.walk.step() {
            Step::Fetch { from, to } => {
                range.window = (from, to);
                let query = Query::new()
                    .opt_str("symbol", range.symbol.as_deref())
                    .opt_str("incomeType", range.kind.as_deref())
                    .num("startTime", from)
                    .num("endTime", to)
                    .num("limit", f64::from(range.limit));
                self.signed_streamed(
                    cx,
                    &self.path("income"),
                    query,
                    Pending::History(Request::Income {
                        root,
                        rows: Vec::new(),
                        invalid: false,
                        range: Some(range),
                    }),
                );
            }
            Step::ChunkDone => {
                let timer = cx.set_timer(INCOME_CHUNK_DELAY_MS);
                self.history
                    .delayed
                    .insert(timer, (root, DelayedWalk::Income(range)));
            }
            Step::Finished => {
                let result = if range.walk.truncated {
                    Err(
                        "Aster income range could not be exhausted; no partial result returned"
                            .into(),
                    )
                } else {
                    serde_json::to_string(&range.collected).map_err(|error| error.to_string())
                };
                let _ = match result {
                    Ok(data) => cx.done_context(&root, data),
                    Err(error) => cx.fail_context(&root, error),
                };
            }
        }
    }

    fn next_fill_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        root: Context,
        mut range: Box<FillWalk>,
    ) {
        match range.walk.step() {
            Step::Fetch { from, to } => {
                if range.walk.requests > 1000 {
                    let _ = cx.fail_context(
                        &root,
                        "Aster fills range exceeded request budget; no partial result returned",
                    );
                    return;
                }
                let to = to.min(range.end);
                range.window = (from, to);
                // orderId overrides time search on V3; apply it only after the
                // complete symbol range and its cumulative PnL are normalized.
                let query = Query::new()
                    .str("symbol", &range.symbol)
                    .num("startTime", from)
                    .num("endTime", to)
                    .num("limit", f64::from(range.limit));
                self.signed_streamed(
                    cx,
                    &self.path("userTrades"),
                    query,
                    Pending::History(Request::Fills {
                        root,
                        rows: Vec::new(),
                        invalid: false,
                        range: Some(range),
                    }),
                );
            }
            Step::ChunkDone => {
                let timer = cx.set_timer(INCOME_CHUNK_DELAY_MS);
                self.history
                    .delayed
                    .insert(timer, (root, DelayedWalk::Fills(range)));
            }
            Step::Finished => {
                if range.walk.truncated {
                    let _ = cx.fail_context(
                        &root,
                        "Aster fills range could not be exhausted; no partial result returned",
                    );
                    return;
                }
                self.finish_fill_range(cx, root, *range);
            }
        }
    }

    fn finish_fill_range(&mut self, cx: &mut Cx<'_, Pending>, root: Context, mut range: FillWalk) {
        normalize(&mut range.collected);
        range.collected.retain(|row| {
            row.price.is_finite()
                && row.qty.is_finite()
                && row.qty > 0.0
                && range.order_id.as_ref().is_none_or(|id| row.order_id == *id)
        });
        let _ = match serde_json::to_string(&range.collected) {
            Ok(data) => cx.done_context(&root, data),
            Err(error) => cx.fail_context(&root, error.to_string()),
        };
    }

    pub(crate) fn on_history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some((root, range)) = self.history.delayed.remove(&timer) {
            match range {
                DelayedWalk::Income(range) => self.next_income_page(cx, root, range),
                DelayedWalk::Fills(range) => self.next_fill_page(cx, root, range),
            }
            true
        } else {
            false
        }
    }
}
