//! Rare history reads return JSON; list bodies stream into typed continuations.
use crate::account::order_side;
use crate::sign::Params;
use crate::wire::{Num, Str, data, time_ms};
use crate::{BitunixFutures, Pending};
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::{Cx, LogLevel, Method, Query, Response};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
const TRADES: &str = "/api/v1/futures/trade/get_history_trades";
const ORDERS: &str = "/api/v1/futures/trade/get_history_orders";
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fills,
    Orders,
    Income,
}
pub struct Request {
    pub command: u32,
    kind: Kind,
    symbol: Option<String>,
    order_id: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    cursor: Option<f64>,
    limit: usize,
    page: u32,
    count: usize,
    added: usize,
    oldest: f64,
    reached_start: bool,
    invalid: bool,
    seen: HashSet<String>,
    checkpoint: [usize; 3],
    fills: Vec<Execution>,
    orders: Vec<HistoricalOrder>,
    income: Vec<Income>,
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
    side: OrderSide,
    commission: f64,
    commission_asset: String,
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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row<'a> {
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    trade_id: Str<'a>,
    #[serde(
        borrow,
        default,
        deserialize_with = "crate::wire::text",
        alias = "ordId"
    )]
    order_id: Str<'a>,
    #[serde(
        borrow,
        default,
        deserialize_with = "crate::wire::text",
        alias = "instId",
        alias = "contract",
        alias = "instrument"
    )]
    symbol: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    side: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    order_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    status: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    ctime: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    mtime: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    time: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    ts: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    trade_time: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    create_time: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    updated_time: Str<'a>,
    #[serde(default)]
    price: Option<Num>,
    #[serde(default)]
    avg_price: Option<Num>,
    #[serde(default)]
    fill_price: Option<Num>,
    #[serde(default)]
    qty: Option<Num>,
    #[serde(default)]
    trade_qty: Option<Num>,
    #[serde(default)]
    fill_size: Option<Num>,
    #[serde(default)]
    fee: Option<Num>,
    #[serde(default)]
    exec_fee: Option<Num>,
    #[serde(default, alias = "realizedPNL")]
    realized_pnl: Num,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    margin_coin: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    fee_currency: Str<'a>,
    #[serde(borrow, default, deserialize_with = "crate::wire::text")]
    fee_coin: Str<'a>,
    #[serde(borrow, default)]
    reduce_only: Option<crate::wire::Supported<'a>>,
}
fn millis(v: f64) -> f64 {
    if v >= 1e18 {
        (v / 1e6).floor()
    } else if v >= 1e15 {
        (v / 1000.0).floor()
    } else if v >= 1e12 {
        v.floor()
    } else {
        v.floor() * 1000.0
    }
}
fn optional(v: f64) -> Option<f64> {
    if v != 0.0 && v.is_finite() {
        Some(v)
    } else {
        None
    }
}
impl BitunixFutures {
    pub(crate) fn query(&mut self, cx: &mut Cx<'_, Pending>, command: u32, query: Query) {
        if matches!(query, Query::PublicTrades { .. }) {
            cx.done(command, Err("Not implemented".into()));
            return;
        }
        if self.credentials.is_none() {
            cx.done(command, Ok("[]".into()));
            return;
        }
        let (kind, symbol, order_id, from, to, limit) = match query {
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Fills,
                symbol,
                order_id,
                from_ms.map(millis),
                to_ms.map(millis),
                limit.unwrap_or(100).clamp(1, 100) as usize,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Orders,
                symbol,
                None,
                from_ms,
                to_ms,
                limit.filter(|l| *l > 0).map_or(usize::MAX, |l| l as usize),
            ),
            Query::Income {
                symbol,
                kind: _,
                from_ms,
                to_ms,
                limit: _,
            } => (Kind::Income, symbol, None, from_ms, to_ms, usize::MAX),
            _ => unreachable!(),
        };
        if kind == Kind::Fills && symbol.as_ref().is_none_or(|s| cx.store.market(s).is_none()) {
            cx.done(command, Ok("[]".into()));
            return;
        }
        if [from, to].into_iter().flatten().any(|v| !v.is_finite()) {
            cx.done(command, Err("Bitunix history time must be finite".into()));
            return;
        }
        let read = Request {
            command,
            kind,
            symbol,
            order_id,
            from,
            to,
            cursor: to,
            limit,
            page: 0,
            count: 0,
            added: 0,
            oldest: f64::INFINITY,
            reached_start: false,
            invalid: false,
            seen: HashSet::new(),
            checkpoint: [0; 3],
            fills: vec![],
            orders: vec![],
            income: vec![],
        };
        self.history_page(cx, read);
    }
    fn history_page(&mut self, cx: &mut Cx<'_, Pending>, mut read: Request) {
        let mut params = Params::default()
            .opt_str("symbol", read.symbol.as_deref())
            .opt_num("startTime", read.from)
            .opt_num(
                "endTime",
                if read.kind == Kind::Orders {
                    read.cursor
                } else {
                    read.to
                },
            );
        let limit = if read.kind == Kind::Fills {
            read.limit
        } else {
            100
        };
        params = params.num("limit", limit as f64);
        if read.kind == Kind::Fills {
            params = params.opt_str("orderId", read.order_id.as_deref());
        }
        if read.kind == Kind::Income && read.page > 0 {
            params = params.num("skip", f64::from(read.page * 100));
        }
        read.checkpoint = [read.fills.len(), read.orders.len(), read.income.len()];
        read.count = 0;
        read.added = 0;
        read.oldest = f64::INFINITY;
        read.reached_start = false;
        read.invalid = false;
        self.signed_request(
            cx,
            Method::Get,
            if read.kind == Kind::Orders {
                ORDERS
            } else {
                TRADES
            },
            params,
            None,
            Pending::History(read),
        );
    }
    pub(crate) fn history_item(&self, cx: &Cx<'_, Pending>, read: &mut Request, item: &str) {
        read.count += 1;
        let row: Row<'_> = match serde_json::from_str(item) {
            Ok(r) => r,
            Err(_) => {
                read.invalid = true;
                return;
            }
        };
        let created = time_ms(&row.ctime);
        let timestamp = if read.kind == Kind::Fills {
            let time = [
                &row.ctime,
                &row.mtime,
                &row.time,
                &row.ts,
                &row.trade_time,
                &row.create_time,
                &row.updated_time,
            ]
            .into_iter()
            .find(|s| !s.is_empty())
            .map_or(0.0, |s| time_ms(s));
            millis(time)
        } else {
            created
        };
        if created > 0.0 {
            read.oldest = read.oldest.min(created);
        }
        let id = if read.kind == Kind::Orders {
            row.order_id.as_ref()
        } else {
            row.trade_id.as_ref()
        };
        // Raw ids, including rows later rejected by a mapper, drive paging progress.
        if read.kind != Kind::Fills && !id.is_empty() && !read.seen.insert(id.to_owned()) {
            return;
        }
        if read.kind == Kind::Orders && !id.is_empty() {
            read.added += 1;
        }
        if read
            .from
            .is_some_and(|from| timestamp > 0.0 && timestamp < from)
        {
            read.reached_start = true;
            return;
        }
        if read.to.is_some_and(|to| timestamp > to) {
            return;
        }
        if read.kind == Kind::Income {
            read.added += 1;
        }
        match read.kind {
            Kind::Fills => {
                let symbol = if row.symbol.is_empty() {
                    read.symbol.as_deref().unwrap_or("")
                } else {
                    &row.symbol
                };
                if symbol
                    .bytes()
                    .filter(u8::is_ascii_alphanumeric)
                    .map(|b| b.to_ascii_uppercase())
                    .ne(read
                        .symbol
                        .as_deref()
                        .unwrap_or("")
                        .bytes()
                        .filter(u8::is_ascii_alphanumeric)
                        .map(|b| b.to_ascii_uppercase()))
                    || !matches!(row.side.to_ascii_uppercase().as_str(), "BUY" | "SELL")
                {
                    return;
                }
                let price = row
                    .price
                    .or(row.avg_price)
                    .or(row.fill_price)
                    .unwrap_or_default()
                    .0;
                let qty = row
                    .qty
                    .or(row.trade_qty)
                    .or(row.fill_size)
                    .unwrap_or_default()
                    .0;
                if price <= 0.0
                    || qty <= 0.0
                    || timestamp <= 0.0
                    || read
                        .order_id
                        .as_ref()
                        .is_some_and(|id| !row.order_id.is_empty() && id != row.order_id.as_ref())
                {
                    return;
                }
                let side = order_side(&row.side);
                let key = if row.trade_id.is_empty() {
                    format!("{}:{timestamp}:{price}:{qty}:{side:?}", row.order_id)
                } else {
                    row.trade_id.to_string()
                };
                if !read.seen.insert(key) {
                    return;
                }
                let asset = if !row.fee_currency.is_empty() {
                    row.fee_currency.as_ref()
                } else if !row.fee_coin.is_empty() {
                    &row.fee_coin
                } else if !row.margin_coin.is_empty() {
                    &row.margin_coin
                } else {
                    cx.store.market(symbol).map_or("USDT", |m| m.quote.as_str())
                };
                read.fills.push(Execution {
                    id: if row.trade_id.is_empty() {
                        None
                    } else {
                        Some(row.trade_id.to_string())
                    },
                    timestamp: (timestamp / 1000.0).floor(),
                    symbol: read.symbol.clone().unwrap_or_default(),
                    order_id: if row.order_id.is_empty() {
                        None
                    } else {
                        Some(row.order_id.to_string())
                    },
                    price,
                    qty,
                    side,
                    commission: row.fee.or(row.exec_fee).unwrap_or_default().0.abs(),
                    commission_asset: asset.to_owned(),
                });
            }
            Kind::Orders => {
                if row.order_id.is_empty()
                    || row.symbol.is_empty()
                    || created <= 0.0
                    || !matches!(row.side.to_ascii_uppercase().as_str(), "BUY" | "SELL")
                {
                    return;
                }
                let status = match row.status.to_ascii_uppercase().as_str() {
                    "INIT" | "NEW" | "PART_FILLED" => OrderStatus::Open,
                    "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
                    _ => OrderStatus::Closed,
                };
                read.orders.push(HistoricalOrder {
                    id: row.order_id.to_string(),
                    symbol: row.symbol.to_string(),
                    side: order_side(&row.side),
                    kind: if row.order_type.eq_ignore_ascii_case("MARKET") {
                        OrderType::Market
                    } else {
                        OrderType::Limit
                    },
                    status,
                    price: optional(row.price.unwrap_or_default().0),
                    average_price: optional(row.avg_price.unwrap_or_default().0),
                    amount: row.qty.unwrap_or_default().0,
                    filled: row.trade_qty.unwrap_or_default().0,
                    reduce_only: row
                        .reduce_only
                        .as_ref()
                        .and_then(crate::wire::Supported::boolean)
                        .unwrap_or(false),
                    created_time: created,
                    updated_time: optional(time_ms(&row.mtime)),
                });
            }
            Kind::Income => {
                if created <= 0.0 || row.symbol.is_empty() {
                    return;
                }
                let id = if row.trade_id.is_empty() {
                    format!("{created}:{}", row.symbol)
                } else {
                    row.trade_id.to_string()
                };
                let asset = if row.margin_coin.is_empty() {
                    "USDT"
                } else {
                    &row.margin_coin
                };
                for (income, kind, category, suffix) in [
                    (row.realized_pnl.0, "REALIZED_PNL", "realized_pnl", "pnl"),
                    (
                        -row.fee.unwrap_or_default().0,
                        if row.fee.unwrap_or_default().0 > 0.0 {
                            "COMMISSION"
                        } else {
                            "REBATE"
                        },
                        if row.fee.unwrap_or_default().0 > 0.0 {
                            "commission"
                        } else {
                            "rebate"
                        },
                        "fee",
                    ),
                ] {
                    if income != 0.0 {
                        read.income.push(Income {
                            symbol: row.symbol.to_string(),
                            income_type: kind,
                            income,
                            asset: asset.to_string(),
                            timestamp: created,
                            tran_id: format!("{id}:{suffix}"),
                            trade_id: id.clone(),
                            category,
                        });
                    }
                }
            }
        }
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut read: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) || data(response.body).is_err() || read.invalid {
            self.history_failed(cx, read, "Bitunix history read failed");
            return;
        }
        let cap = if read.kind == Kind::Orders { 50 } else { 100 };
        let next = read.kind != Kind::Fills
            && read.count >= 100
            && read.added > 0
            && !read.reached_start
            && read.page + 1 < cap
            && (read.kind != Kind::Orders
                || read.oldest.is_finite() && read.from.is_none_or(|from| read.oldest > from));
        if next {
            read.page += 1;
            if read.kind == Kind::Orders {
                read.cursor = Some(read.oldest);
            }
            self.history_page(cx, read);
            return;
        }
        if read.page + 1 >= cap {
            cx.log(
                LogLevel::Warning,
                "Bitunix history reached the page cap; older rows were not fetched",
            );
        }
        self.history_done(cx, read);
    }
    pub(crate) fn history_failed(&self, cx: &mut Cx<'_, Pending>, mut read: Request, error: &str) {
        read.fills.truncate(read.checkpoint[0]);
        read.income.truncate(read.checkpoint[2]);
        read.orders.clear();
        cx.log(LogLevel::Warning, error);
        self.history_done(cx, read);
    }
    fn history_done(&self, cx: &mut Cx<'_, Pending>, mut read: Request) {
        let result = match read.kind {
            Kind::Fills => {
                read.fills
                    .sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
                read.fills.truncate(read.limit);
                serde_json::to_string(&read.fills)
            }
            Kind::Orders => {
                read.orders
                    .sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
                read.orders.truncate(read.limit);
                serde_json::to_string(&read.orders)
            }
            Kind::Income => {
                read.income
                    .sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
                serde_json::to_string(&read.income)
            }
        };
        cx.done(read.command, result.map_err(|e| e.to_string()));
    }
}
