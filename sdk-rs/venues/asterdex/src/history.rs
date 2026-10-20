//! Typed rare reads; Aster executions carry cumulative PnL, unlike Binance.
use crate::{AsterDex, Pending, account, sign::Query};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::{CommandId, Cx, Method, Query as Read, Response, TimerId};
use safe_cex_venue_binance::wire::{HistoricalOrderRow, UserTrade};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

const MAX_WINDOW_MS: f64 = 7.0 * 24.0 * 3_600_000.0;
const INCOME_CHUNK_DELAY_MS: f64 = 2_000.0;

#[derive(Default)]
pub struct HistoryState {
    delayed: HashMap<TimerId, (CommandId, Box<IncomeWalk>)>,
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
        command: CommandId,
        rows: Vec<Execution>,
        invalid: bool,
    },
    Orders {
        command: CommandId,
        rows: Vec<HistoricalOrder>,
        invalid: bool,
    },
    Income {
        command: CommandId,
        rows: Vec<Income>,
        invalid: bool,
        range: Option<Box<IncomeWalk>>,
    },
    Trades {
        command: CommandId,
        symbol: String,
        rows: Vec<Trade>,
        invalid: bool,
    },
}
impl Request {
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
fn number(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}
fn nonzero(text: &str) -> Option<f64> {
    number(text).filter(|v| *v != 0.0)
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
    pub(crate) fn start_query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, read: Read) {
        if self.credentials.is_none() && !matches!(read, Read::PublicTrades { .. }) {
            cx.done(command, Ok("[]".into()));
            return;
        }
        let (from, to, bounded, income) = match &read {
            Read::Fills { from_ms, to_ms, .. } | Read::OrderHistory { from_ms, to_ms, .. } => {
                (*from_ms, *to_ms, true, false)
            }
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
            cx.done(command, Err("Invalid Aster history time range".into()));
            return;
        }
        if bounded
            && from
                .zip(to)
                .is_some_and(|(from, to)| to - from > MAX_WINDOW_MS)
        {
            cx.done(
                command,
                Err(
                    "Aster fills and order history require a time window of at most seven days"
                        .into(),
                ),
            );
            return;
        }
        let (endpoint, query, pending, private) = match read {
            Read::OrderByClientId { .. } => {
                cx.done(
                    command,
                    Err("Aster does not support caller client order IDs".into()),
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
                let Some(symbol) = symbol.filter(|symbol| !symbol.is_empty()) else {
                    cx.done(command, Ok("[]".into()));
                    return;
                };
                (
                    "userTrades",
                    Query::new()
                        .str("symbol", &symbol)
                        .num("limit", f64::from(limit.unwrap_or(500).clamp(1, 1000)))
                        .opt_str("orderId", order_id.as_deref())
                        .opt_num("startTime", from_ms)
                        .opt_num("endTime", to_ms),
                    Request::Fills {
                        command,
                        rows: Vec::new(),
                        invalid: false,
                    },
                    true,
                )
            }
            Read::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => {
                if self.version == crate::sign::Version::V1
                    && symbol.as_deref().is_none_or(str::is_empty)
                {
                    cx.done(command, Ok("[]".into()));
                    return;
                }
                (
                    "allOrders",
                    Query::new()
                        .opt_str("symbol", symbol.as_deref())
                        .num("limit", f64::from(limit.unwrap_or(500).clamp(1, 1000)))
                        .opt_num("startTime", from_ms)
                        .opt_num("endTime", to_ms),
                    Request::Orders {
                        command,
                        rows: Vec::new(),
                        invalid: false,
                    },
                    true,
                )
            }
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
                        command,
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
                        command,
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
                    command,
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
            cx.request_streamed(
                Method::Get,
                format!("{}{path}?{}", self.urls.rest, query.as_str()),
                Vec::new(),
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
                let price = number(&r.price).unwrap_or(f64::NAN);
                let qty = number(&r.qty).unwrap_or(f64::NAN);
                rows.push(Execution {
                    id: r.id.to_string(),
                    timestamp: (r.time / 1000.0).floor(),
                    time_ms: r.time,
                    symbol: r.symbol.to_string(),
                    order_id: r.order_id.to_string(),
                    price,
                    qty,
                    side: side(&r.side),
                    commission: r.commission.as_deref().and_then(number),
                    commission_asset: r.commission_asset.map(|s| s.into_owned()),
                    position_side: rail(r.position_side.as_deref()),
                    pnl_group: r
                        .position_side
                        .as_deref()
                        .filter(|side| !side.is_empty())
                        .unwrap_or("BOTH")
                        .to_ascii_uppercase(),
                    realized_pnl: r.realized_pnl.as_deref().and_then(number),
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
                let (Some(amount), Some(filled)) = (number(&r.orig_qty), number(&r.executed_qty))
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
                let Some(income) = number(&r.income) else {
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
                let (Some(price), Some(qty)) = (number(r.price), number(r.qty)) else {
                    *invalid = true;
                    return;
                };
                rows.push(Trade {
                    id: r.id.to_string(),
                    symbol: symbol.clone(),
                    price,
                    qty,
                    quote_qty: r.quote_qty.and_then(number).unwrap_or(price * qty),
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
        let (command, invalid, result) = match request {
            Request::Fills {
                command,
                mut rows,
                invalid,
            } => {
                normalize(&mut rows);
                rows.retain(|row| row.price.is_finite() && row.qty.is_finite() && row.qty > 0.0);
                (command, invalid, serde_json::to_string(&rows))
            }
            Request::Orders {
                command,
                mut rows,
                invalid,
            } => {
                rows.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
                (command, invalid, serde_json::to_string(&rows))
            }
            Request::Income {
                command,
                rows,
                invalid,
                range: Some(mut range),
            } => {
                if let Some(error) = error {
                    cx.done(command, Err(error));
                } else if invalid {
                    cx.done(command, Err("Malformed Aster history response".into()));
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
                    self.next_income_page(cx, command, range);
                }
                return;
            }
            Request::Income {
                command,
                rows,
                invalid,
                range: None,
            } => (command, invalid, serde_json::to_string(&rows)),
            Request::Trades {
                command,
                rows,
                invalid,
                ..
            } => (command, invalid, serde_json::to_string(&rows)),
        };
        let result = if let Some(e) = error {
            Err(e)
        } else if invalid {
            Err("Malformed Aster history response".into())
        } else {
            result.map_err(|e| e.to_string())
        };
        cx.done(command, result);
    }

    fn next_income_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
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
                        command,
                        rows: Vec::new(),
                        invalid: false,
                        range: Some(range),
                    }),
                );
            }
            Step::ChunkDone => {
                let timer = cx.set_timer(INCOME_CHUNK_DELAY_MS);
                self.history.delayed.insert(timer, (command, range));
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
                cx.done(command, result);
            }
        }
    }

    pub(crate) fn on_history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some((command, range)) = self.history.delayed.remove(&timer) {
            self.next_income_page(cx, command, range);
            true
        } else {
            false
        }
    }
}
