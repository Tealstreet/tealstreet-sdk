//! Rare reads stage streamed pages; income completeness follows the shared span walker.
use crate::account_wire::{OrderRow, id};
use crate::sign::encode;
use crate::wire::{Str, number, optional_number, timestamp};
use crate::{Pending, Task, WeexFutures};
use safe_cex_engine::Level;
use safe_cex_engine::history::{Completion, CursorHistory, RangeHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorWalk, PageRow, RangeWalk, Step, WindowOrder};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, Method, Outcome, Query, Response, TimerId, VenueError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

const DAY: f64 = 86_400_000.0;
const INCOME_WINDOW: f64 = 100.0 * DAY;
const PAGE_DELAY: f64 = 100.0;
const INCOME_BUDGET: u32 = 10_000;

#[derive(Default)]
pub struct State {
    reads: HashMap<CommandId, Read>,
}
enum Read {
    List {
        pending: VecDeque<Fetch>,
        owner: Box<CursorHistory<ListRow>>,
        kind: Kind,
        missing_fill_id: bool,
        order_id: Option<String>,
        symbol: Option<String>,
        delay: Option<TimerId>,
    },
    Income {
        symbol: Option<String>,
        kind: Option<String>,
        limit: u32,
        owner: Box<IncomeHistory>,
        delay: Option<TimerId>,
        deferred: Option<(Option<f64>, Option<f64>)>,
    },
}
enum IncomeHistory {
    Range(RangeHistory<Income>),
    Single(CursorHistory<Income>),
}

struct Fetch {
    path: &'static str,
    query: String,
    kind: Kind,
    window: (Option<f64>, Option<f64>),
}
#[derive(Clone, Copy)]
enum Kind {
    Fills,
    Normal,
    Algo,
    Trades,
    Income,
}
pub struct Request {
    command: CommandId,
    kind: Kind,
    window: (Option<f64>, Option<f64>),
    order_id: Option<String>,
    symbol: Option<String>,
    rows: Rows,
    raw_rows: usize,
    invalid: bool,
    missing_fill_id: bool,
}
enum Rows {
    Fills(Vec<Execution>),
    Orders(Vec<HistoricalOrder>),
    Trades(Vec<Trade>),
    Income(Vec<Income>),
}
impl Rows {
    fn new(kind: Kind) -> Self {
        match kind {
            Kind::Fills => Self::Fills(vec![]),
            Kind::Normal | Kind::Algo => Self::Orders(vec![]),
            Kind::Trades => Self::Trades(vec![]),
            Kind::Income => Self::Income(vec![]),
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum ListRow {
    Fill(Execution),
    Order(HistoricalOrder),
    Trade(Trade),
}
impl ListRow {
    fn time(&self) -> f64 {
        match self {
            Self::Fill(row) => row.timestamp,
            Self::Order(row) => row.created_time,
            Self::Trade(row) => row.timestamp,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlgoMetadata<'a> {
    #[serde(borrow)]
    orders: &'a serde_json::value::RawValue,
    #[serde(default)]
    has_more: bool,
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
#[serde(rename_all = "camelCase")]
struct FillRow<'a> {
    #[serde(borrow, deserialize_with = "id", alias = "tradeId", alias = "fillId")]
    id: Str<'a>,
    #[serde(borrow, deserialize_with = "id")]
    order_id: Str<'a>,
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(borrow, alias = "orderSide")]
    side: Str<'a>,
    #[serde(deserialize_with = "number")]
    price: f64,
    #[serde(deserialize_with = "number", alias = "fillSize")]
    qty: f64,
    #[serde(deserialize_with = "number", alias = "createdTime")]
    time: f64,
    #[serde(default, deserialize_with = "number", alias = "fillFee")]
    commission: f64,
    #[serde(borrow, default, alias = "coin")]
    commission_asset: Option<Str<'a>>,
    #[serde(borrow, default)]
    position_side: Option<Str<'a>>,
    #[serde(default, deserialize_with = "optional_number", alias = "realizePnl")]
    realized_pnl: Option<f64>,
    #[serde(default)]
    maker: Option<bool>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalRow<'a> {
    #[serde(flatten, borrow)]
    order: OrderRow<'a>,
    #[serde(default, deserialize_with = "optional_number")]
    avg_price: Option<f64>,
    #[serde(borrow, default)]
    position_side: Option<Str<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeRow<'a> {
    #[serde(
        borrow,
        default,
        deserialize_with = "id",
        alias = "tranId",
        alias = "id"
    )]
    bill_id: Str<'a>,
    #[serde(borrow, default)]
    symbol: Str<'a>,
    #[serde(borrow)]
    income_type: Str<'a>,
    #[serde(deserialize_with = "number")]
    income: f64,
    #[serde(borrow, alias = "coin")]
    asset: Str<'a>,
    #[serde(deserialize_with = "number", alias = "createdTime")]
    time: f64,
    #[serde(borrow, default, deserialize_with = "optional_id")]
    trade_id: Option<Str<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeMetadata<'a> {
    #[serde(default, borrow)]
    items: Option<&'a serde_json::value::RawValue>,
    has_next_page: Option<bool>,
    next_key: Option<serde::de::IgnoredAny>,
}
fn optional_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Str<'de>>, D::Error> {
    #[derive(Deserialize)]
    struct Id<'a>(#[serde(borrow, deserialize_with = "id")] Str<'a>);
    Ok(Option::<Id>::deserialize(d)?.map(|v| v.0))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeRow<'a> {
    #[serde(borrow, deserialize_with = "id")]
    id: Str<'a>,
    #[serde(deserialize_with = "number")]
    price: f64,
    #[serde(deserialize_with = "number")]
    qty: f64,
    #[serde(deserialize_with = "number")]
    quote_qty: f64,
    #[serde(deserialize_with = "number")]
    time: f64,
    is_buyer_maker: bool,
}

fn side(value: &str) -> Option<&'static str> {
    match value {
        "BUY" => Some("buy"),
        "SELL" => Some("sell"),
        _ => None,
    }
}
fn hedge(value: Option<&str>) -> Option<&'static str> {
    value.map(|v| match v {
        "LONG" => "long",
        "SHORT" => "short",
        _ => "both",
    })
}
fn category(value: &str) -> &'static str {
    match value.to_ascii_uppercase().as_str() {
        "REALIZED_PNL" | "DELIVERED_SETTELMENT" => "realized_pnl",
        "FUNDING_FEE" => "funding",
        "COMMISSION" | "OPTIONS_PREMIUM_FEE" | "POSITION_LIMIT_INCREASE_FEE" => "commission",
        "COMMISSION_REBATE" | "REFERRAL_KICKBACK" | "API_REBATE" => "rebate",
        "TRANSFER"
        | "INTERNAL_TRANSFER"
        | "CROSS_COLLATERAL_TRANSFER"
        | "COIN_SWAP_DEPOSIT"
        | "COIN_SWAP_WITHDRAW"
        | "AUTO_EXCHANGE" => "transfer",
        "INSURANCE_CLEAR" => "liquidation",
        _ => "other",
    }
}
fn income_identity(row: &Income) -> String {
    if row.tran_id.is_empty() {
        format!(
            "fallback|{}|{}|{}|{}",
            row.symbol, row.income_type, row.timestamp, row.income
        )
    } else {
        format!(
            "id|{}|{}|{}|{}|{}|{}",
            row.tran_id,
            row.symbol,
            row.asset,
            row.income_type,
            row.income,
            row.trade_id.as_deref().unwrap_or("")
        )
    }
}
fn query(parts: &[(&str, Option<String>)]) -> String {
    let mut parts: Vec<_> = parts
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| (*k, v)))
        .collect();
    parts.sort_by_key(|(k, _)| *k);
    parts
        .into_iter()
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}
fn time(value: Option<f64>) -> Option<String> {
    value.map(|v| format!("{:.0}", v.floor()))
}
fn valid_range(from: Option<f64>, to: Option<f64>) -> bool {
    from.into_iter()
        .chain(to)
        .all(|v| v.is_finite() && (0.0..=9_007_199_254_740_991.0).contains(&v))
        && !matches!((from,to),(Some(a),Some(b)) if a>b)
}
fn windows(from: Option<f64>, to: Option<f64>, span: f64) -> Vec<(Option<f64>, Option<f64>)> {
    if let (Some(from), Some(to)) = (from, to) {
        if (to - from) / span > 1_000.0 {
            return vec![];
        }
        return safe_cex_engine::paging::windows(from, to, span, WindowOrder::OldestFirst, 1000)
            .unwrap_or_default()
            .into_iter()
            .map(|(a, b)| (Some(a), Some(b)))
            .collect();
    }
    vec![(from, to)]
}

impl WeexFutures {
    pub(crate) fn history_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        read: Query,
    ) {
        if self.credentials.primary.is_none()
            && !matches!(
                read,
                Query::PublicTrades { .. } | Query::OrderByClientId { .. }
            )
        {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.done_context(&context, "[]".into());
            return;
        }
        let mut requests = VecDeque::new();
        let (kind, order_id, symbol) = match read {
            Query::OrderByClientId { .. } => {
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                let _ = cx.fail_context(
                    &context,
                    "WEEX client order lookup is not supported by the Rust engine",
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
                if !valid_range(from_ms, to_ms) {
                    let Ok(context) = cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    }) else {
                        return;
                    };
                    let _ = cx.fail_context(&context, "invalid WEEX fill range");
                    return;
                }
                for window in windows(from_ms, to_ms, 7.0 * DAY) {
                    requests.push_back(Fetch {
                        path: "/capi/v3/userTrades",
                        kind: Kind::Fills,
                        window,
                        query: query(&[
                            ("symbol", symbol.clone()),
                            ("orderId", order_id.clone()),
                            ("startTime", time(window.0)),
                            ("endTime", time(window.1)),
                            (
                                "limit",
                                Some(limit.unwrap_or(100).clamp(1, 100).to_string()),
                            ),
                        ]),
                    });
                }
                (Kind::Fills, order_id, symbol)
            }
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => {
                if !valid_range(from_ms, to_ms) {
                    let Ok(context) = cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    }) else {
                        return;
                    };
                    let _ = cx.fail_context(&context, "invalid WEEX order range");
                    return;
                }
                for window in windows(from_ms, to_ms, 90.0 * DAY) {
                    let parts = [
                        ("symbol", symbol.clone()),
                        ("startTime", time(window.0)),
                        ("endTime", time(window.1)),
                        (
                            "limit",
                            Some(limit.unwrap_or(100).clamp(1, 100).to_string()),
                        ),
                        ("page", Some("0".into())),
                    ];
                    requests.push_back(Fetch {
                        path: "/capi/v3/order/history",
                        kind: Kind::Normal,
                        window,
                        query: query(&parts),
                    });
                    requests.push_back(Fetch {
                        path: "/capi/v3/allAlgoOrders",
                        kind: Kind::Algo,
                        window,
                        query: query(&parts[..4]),
                    });
                }
                (Kind::Normal, None, symbol)
            }
            Query::PublicTrades { symbol, limit } => {
                requests.push_back(Fetch {
                    path: "/capi/v3/market/trades",
                    kind: Kind::Trades,
                    window: (None, None),
                    query: query(&[
                        ("symbol", Some(symbol.clone())),
                        (
                            "limit",
                            Some(limit.unwrap_or(100).clamp(1, 1000).to_string()),
                        ),
                    ]),
                });
                (Kind::Trades, None, Some(symbol))
            }
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                if !valid_range(from_ms, to_ms) {
                    let Ok(context) = cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    }) else {
                        return;
                    };
                    let _ = cx.fail_context(&context, "invalid WEEX income range");
                    return;
                }
                if matches!((from_ms,to_ms),(Some(from),Some(to)) if (to-from)/INCOME_WINDOW>f64::from(INCOME_BUDGET))
                {
                    let Ok(context) = cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    }) else {
                        return;
                    };
                    let _ = cx.fail_context(&context, "WEEX income range exceeds chunk budget");
                    return;
                }
                let ranged = matches!((from_ms,to_ms),(Some(a),Some(b)) if b>a);
                let owner = if let (Some(a), Some(b)) = (from_ms, to_ms)
                    && b > a
                {
                    IncomeHistory::Range(RangeHistory::new(
                        Completion::new(command), RangeWalk::new(a,b,INCOME_WINDOW,100),
                        |gaps| format!("WEEX income history is INCOMPLETE: {gaps} chunk(s) could not be walked to completion; totals are a lower bound"),
                        |rows| serde_json::to_string(rows).map_err(|e|VenueError::new(e.to_string())),
                    ).with_request_budget(INCOME_BUDGET, "WEEX income history is INCOMPLETE: lifetime request budget exhausted; totals are a lower bound"))
                } else {
                    IncomeHistory::Single(CursorHistory::new(
                        Completion::new(command),
                        CursorWalk::new(1),
                        |_| "WEEX income history page budget exhausted".into(),
                        |rows| {
                            serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string()))
                        },
                    ))
                };
                self.history.reads.insert(
                    command,
                    Read::Income {
                        symbol,
                        kind,
                        limit: if ranged {
                            100
                        } else {
                            limit.unwrap_or(100).clamp(1, 100)
                        },
                        owner: Box::new(owner),
                        delay: None,
                        deferred: None,
                    },
                );
                if ranged {
                    self.history_next_income(cx, command);
                } else {
                    self.history_income_request(cx, command, from_ms, to_ms);
                }
                return;
            }
        };
        if requests.is_empty() {
            let Ok(context) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) else {
                return;
            };
            let _ = cx.fail_context(&context, "WEEX history range exceeds request budget");
            return;
        }
        self.history.reads.insert(
            command,
            Read::List {
                pending: requests,
                owner: Box::new(CursorHistory::new(
                    Completion::new(command),
                    CursorWalk::new(1),
                    |_| "WEEX history stage page budget exhausted".into(),
                    |rows| {
                        serde_json::to_string(rows)
                            .map_err(|error| VenueError::new(error.to_string()))
                    },
                )),
                kind,
                missing_fill_id: false,
                order_id,
                symbol,
                delay: None,
            },
        );
        self.history_next_list(cx, command);
    }
    fn history_next_list(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(Read::List {
            pending,
            order_id,
            symbol,
            delay,
            ..
        }) = self.history.reads.get_mut(&command)
        else {
            return;
        };
        if !pending.is_empty() && cx.now < self.paused_until {
            *delay = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        let Some(fetch) = pending.pop_front() else {
            self.finish_list(cx, command, Ok(()), false);
            return;
        };
        let stream = Some(if matches!(fetch.kind, Kind::Algo) {
            ArrayAt::Field("orders")
        } else {
            ArrayAt::Root
        });
        let request = Request {
            command,
            kind: fetch.kind,
            window: fetch.window,
            order_id: order_id.clone(),
            symbol: symbol.clone(),
            rows: Rows::new(fetch.kind),
            raw_rows: 0,
            invalid: false,
            missing_fill_id: false,
        };
        if matches!(fetch.kind, Kind::Trades) {
            self.get(cx, fetch.path, &fetch.query, Task::History(request), stream);
        } else if !self.signed_request(
            cx,
            Method::Get,
            fetch.path,
            &fetch.query,
            None,
            Task::History(request),
            stream,
        ) {
            self.finish_list(
                cx,
                command,
                Err(VenueError::new("WEEX history signing unavailable").outcome(Outcome::NotSent)),
                true,
            );
        }
    }
    fn history_income_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        from: Option<f64>,
        to: Option<f64>,
    ) {
        let Some(Read::Income {
            symbol,
            kind,
            limit,
            deferred,
            delay,
            ..
        }) = self.history.reads.get_mut(&command)
        else {
            return;
        };
        if cx.now < self.paused_until {
            *deferred = Some((from, to));
            *delay = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            asset: Option<&'a str>,
            symbol: &'a str,
            income_type: &'a str,
            start_time: Option<u64>,
            end_time: Option<u64>,
            limit: u32,
        }
        let body = serde_json::to_string(&Body {
            asset: (symbol.as_deref().unwrap_or("").is_empty()
                && kind.as_deref().unwrap_or("").is_empty())
            .then_some(""),
            symbol: symbol.as_deref().unwrap_or(""),
            income_type: kind.as_deref().unwrap_or(""),
            start_time: from.map(|v| v.floor() as u64),
            end_time: to.map(|v| v.floor() as u64),
            limit: *limit,
        })
        .expect("validated income bounds");
        let request = Request {
            command,
            kind: Kind::Income,
            window: (from, to),
            order_id: None,
            symbol: None,
            rows: Rows::Income(vec![]),
            raw_rows: 0,
            invalid: false,
            missing_fill_id: false,
        };
        if !self.signed_request(
            cx,
            Method::Post,
            "/capi/v3/account/income",
            "",
            Some(body),
            Task::History(request),
            Some(ArrayAt::Field("items")),
        ) {
            self.finish_income(
                cx,
                command,
                Err(VenueError::new("WEEX history signing unavailable").outcome(Outcome::NotSent)),
                true,
            );
        }
    }
    fn history_next_income(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(Read::Income { owner, delay, .. }) = self.history.reads.get_mut(&command) else {
            return;
        };
        let IncomeHistory::Range(owner) = owner.as_mut() else {
            return;
        };
        if cx.now < self.paused_until {
            *delay = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        match owner.step() {
            Step::Fetch { from, to } => {
                self.history_income_request(cx, command, Some(from), Some(to))
            }
            Step::ChunkDone => *delay = Some(cx.set_timer(PAGE_DELAY)),
            Step::Finished => self.finish_income(cx, command, Ok(()), false),
        }
    }
    fn finish_income(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        result: Result<(), VenueError>,
        strict: bool,
    ) {
        let Some(Read::Income { owner, delay, .. }) = self.history.reads.remove(&command) else {
            return;
        };
        if let Some(timer) = delay {
            cx.clear_timer(timer);
        }
        match (*owner, strict, result) {
            (IncomeHistory::Range(owner), true, Err(error)) => owner.reject(cx, error),
            (IncomeHistory::Single(owner), true, Err(error)) => owner.reject(cx, error),
            (IncomeHistory::Range(owner), _, result) => {
                owner.finish_with(cx, result, encode_income)
            }
            (IncomeHistory::Single(owner), _, result) => {
                owner.finish_with(cx, result, encode_income)
            }
        }
    }
    fn finish_list(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        result: Result<(), VenueError>,
        strict: bool,
    ) {
        let Some(Read::List {
            owner, kind, delay, ..
        }) = self.history.reads.remove(&command)
        else {
            return;
        };
        if let Some(timer) = delay {
            cx.clear_timer(timer);
        }
        match (strict, result) {
            (true, Err(error)) => owner.reject(cx, error),
            (_, result) => owner.finish_with(cx, result, |rows| encode_list(kind, rows)),
        }
    }
    fn missing_fill_identity(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(Read::List {
            missing_fill_id, ..
        }) = self.history.reads.get_mut(&command)
        else {
            return;
        };
        if *missing_fill_id {
            return;
        }
        *missing_fill_id = true;
        cx.event(
            Level::Warn,
            code::ROW_SKIPPED,
            "WEEX fill history row skipped: missing id (id/tradeId/fillId)",
            vec![
                ("command", safe_cex_engine::Value::Num(f64::from(command))),
                ("field", safe_cex_engine::Value::Str("id".into())),
            ],
        );
    }
    pub(crate) fn history_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) -> bool {
        request.raw_rows += 1;
        let ok = match &mut request.rows {
            Rows::Fills(rows) => serde_json::from_str::<FillRow<'_>>(item)
                .map(|r| {
                    let t = timestamp(r.time);
                    if request.order_id.as_deref().is_some_and(|v| v != r.order_id)
                        || request.symbol.as_deref().is_some_and(|v| v != r.symbol)
                        || request.window.0.is_some_and(|v| t < v)
                        || request.window.1.is_some_and(|v| t > v)
                    {
                        return;
                    }
                    if r.id.is_empty() {
                        request.missing_fill_id = true;
                        return;
                    }
                    if let Some(side) = side(&r.side)
                        && r.price > 0.0
                        && r.qty.abs() > 0.0
                        && t > 0.0
                    {
                        rows.push(Execution {
                            id: r.id.into_owned(),
                            timestamp: (t / 1000.0).floor(),
                            symbol: r.symbol.into_owned(),
                            order_id: r.order_id.into_owned(),
                            price: r.price,
                            qty: r.qty.abs(),
                            side,
                            commission: r.commission.abs(),
                            commission_asset: r.commission_asset.map(|v| v.into_owned()),
                            position_side: hedge(r.position_side.as_deref()),
                            realized_pnl: r.realized_pnl,
                            maker: r.maker,
                        });
                    }
                })
                .is_ok(),
            Rows::Orders(rows) => serde_json::from_str::<HistoricalRow<'_>>(item)
                .map(|r| {
                    let normal = if matches!(request.kind, Kind::Algo) {
                        r.order.algo().into_iter().next()
                    } else {
                        r.order.normal()
                    };
                    if let Some(order) = normal
                        && r.order.create_time > 0.0
                    {
                        let created = timestamp(r.order.create_time);
                        if request.symbol.as_deref().is_some_and(|v| v != order.symbol)
                            || request.window.0.is_some_and(|v| created < v)
                            || request.window.1.is_some_and(|v| created > v)
                        {
                            return;
                        }
                        rows.push(HistoricalOrder {
                            id: order.id,
                            symbol: order.symbol,
                            side: side(&r.order.side).expect("validated order side"),
                            kind: order.kind,
                            status: if r.order.status == "TRIGGERED" {
                                OrderStatus::Closed
                            } else {
                                order.status
                            },
                            price: (r.order.price != 0.0).then_some(r.order.price),
                            average_price: r.avg_price.filter(|v| *v != 0.0),
                            amount: order.amount,
                            filled: order.filled,
                            reduce_only: order.reduce_only,
                            position_side: hedge(r.position_side.as_deref()),
                            created_time: created,
                            updated_time: (r.order.update_time > 0.0)
                                .then(|| timestamp(r.order.update_time)),
                        });
                    }
                })
                .is_ok(),
            Rows::Trades(rows) => serde_json::from_str::<TradeRow<'_>>(item)
                .map(|r| {
                    let t = timestamp(r.time);
                    if r.price > 0.0 && r.qty.abs() > 0.0 && t > 0.0 {
                        rows.push(Trade {
                            id: r.id.into_owned(),
                            symbol: request.symbol.clone().unwrap_or_default(),
                            price: r.price,
                            qty: r.qty.abs(),
                            quote_qty: r.quote_qty,
                            timestamp: t,
                            side: if r.is_buyer_maker { "sell" } else { "buy" },
                        });
                    }
                })
                .is_ok(),
            Rows::Income(rows) => serde_json::from_str::<IncomeRow<'_>>(item)
                .map(|r| {
                    rows.push(Income {
                        symbol: r.symbol.into_owned(),
                        category: category(&r.income_type),
                        income_type: r.income_type.into_owned(),
                        income: r.income,
                        asset: r.asset.into_owned(),
                        timestamp: timestamp(r.time),
                        tran_id: r.bill_id.into_owned(),
                        trade_id: r.trade_id.map(|v| v.into_owned()).filter(|v| !v.is_empty()),
                    })
                })
                .is_ok(),
        };
        request.invalid |= !ok;
        ok
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) || request.invalid {
            let error = if request.invalid {
                "malformed WEEX history row".into()
            } else {
                format!("WEEX history HTTP {}: {}", response.status, response.body)
            };
            self.history_failed(cx, request, &error);
            return;
        }
        let has_more = if matches!(request.kind, Kind::Income) {
            match serde_json::from_str::<IncomeMetadata>(response.body) {
                Ok(metadata)
                    if metadata
                        .items
                        .is_some_and(|list| list.get().trim_start().starts_with('[')) =>
                {
                    metadata.has_next_page == Some(true) || metadata.next_key.is_some()
                }
                _ => {
                    self.history_failed(cx, request, "malformed WEEX income pagination metadata");
                    return;
                }
            }
        } else if matches!(request.kind, Kind::Algo) {
            match serde_json::from_str::<AlgoMetadata<'_>>(response.body) {
                Ok(metadata) if metadata.orders.get().trim_start().starts_with('[') => {
                    metadata.has_more
                }
                _ => {
                    self.history_failed(cx, request, "malformed WEEX algo history response");
                    return;
                }
            }
        } else {
            if !serde_json::from_str::<&serde_json::value::RawValue>(response.body)
                .is_ok_and(|raw| raw.get().trim_start().starts_with('['))
            {
                self.history_failed(cx, request, "malformed WEEX history row list");
                return;
            }
            false
        };
        let command = request.command;
        let Some(read) = self.history.reads.get_mut(&command) else {
            return;
        };
        match read {
            Read::List { owner, pending, .. } => {
                match request.rows {
                    Rows::Fills(page) => {
                        for row in page {
                            if owner.fresh(format!("{}|{}", row.symbol, row.id)) {
                                owner.stage(ListRow::Fill(row));
                            }
                        }
                    }
                    Rows::Orders(page) => {
                        for row in page {
                            if owner.fresh(format!("{}|{}", row.symbol, row.id)) {
                                owner.stage(ListRow::Order(row));
                            }
                        }
                    }
                    Rows::Trades(page) => {
                        for row in page {
                            owner.stage(ListRow::Trade(row));
                        }
                    }
                    Rows::Income(_) => unreachable!("list response"),
                }
                let count = owner.staged_len();
                let gap = if request.missing_fill_id {
                    Some("WEEX fill history is INCOMPLETE: row missing native fill id".into())
                } else if has_more {
                    Some(
                        "WEEX order history is INCOMPLETE: venue advertises additional algo orders"
                            .into(),
                    )
                } else {
                    None
                };
                owner.advance_with_coverage(
                    CursorPage {
                        next: None,
                        rows: request.raw_rows,
                        fresh: count,
                        want_more: false,
                    },
                    gap,
                );
                if !pending.is_empty() && !owner.restart_stage() {
                    self.finish_list(
                        cx,
                        command,
                        Err(VenueError::new("WEEX history stage did not complete")),
                        true,
                    );
                    return;
                }
                if request.missing_fill_id {
                    self.missing_fill_identity(cx, command);
                }
                self.history_next_list(cx, command);
            }
            Read::Income { owner, delay, .. } => {
                let Rows::Income(page) = request.rows else {
                    unreachable!("income page");
                };
                let gap = has_more.then(|| {
                    "WEEX income history is INCOMPLETE: venue advertises additional cursor rows"
                        .to_owned()
                });
                match owner.as_mut() {
                    IncomeHistory::Range(owner) => {
                        for row in page {
                            let facts = PageRow {
                                timestamp: row.timestamp,
                                identity: income_identity(&row),
                            };
                            owner.stage(row, facts);
                        }
                        owner.accept_page_with_coverage(gap);
                        *delay = Some(cx.set_timer(PAGE_DELAY));
                    }
                    IncomeHistory::Single(owner) => {
                        let count = page.len();
                        for row in page {
                            owner.stage(row);
                        }
                        owner.advance_with_coverage(
                            CursorPage {
                                next: None,
                                rows: count,
                                fresh: count,
                                want_more: false,
                            },
                            gap,
                        );
                        self.finish_income(cx, command, Ok(()), false);
                    }
                }
            }
        }
    }
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        self.history_end(cx, request, VenueError::new(error), false);
    }
    /// A provider refusal is strict even after accepted pages, never partial success.
    pub(crate) fn history_refused(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        self.history_end(cx, request, error, true);
    }
    fn history_end(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
        strict: bool,
    ) {
        let command = request.command;
        if matches!(self.history.reads.get(&command), Some(Read::Income { .. })) {
            self.finish_income(cx, command, Err(error), strict);
        } else if matches!(self.history.reads.get(&command), Some(Read::List { .. })) {
            self.finish_list(cx, command, Err(error), strict);
        }
    }
    pub(crate) fn history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let command = self
            .history
            .reads
            .iter_mut()
            .find_map(|(command, read)| match read {
                Read::Income { delay, .. } | Read::List { delay, .. } if *delay == Some(timer) => {
                    *delay = None;
                    Some(*command)
                }
                _ => None,
            });
        if let Some(command) = command {
            match self.history.reads.get_mut(&command) {
                Some(Read::List { .. }) => self.history_next_list(cx, command),
                Some(Read::Income { deferred, .. }) => {
                    if let Some((from, to)) = deferred.take() {
                        self.history_income_request(cx, command, from, to);
                    } else {
                        self.history_next_income(cx, command);
                    }
                }
                None => {}
            }
            true
        } else {
            false
        }
    }
}

fn encode_income(mut rows: Vec<Income>) -> Result<String, VenueError> {
    rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
    serde_json::to_string(&rows).map_err(|error| VenueError::new(error.to_string()))
}

fn encode_list(kind: Kind, mut rows: Vec<ListRow>) -> Result<String, VenueError> {
    if !matches!(kind, Kind::Trades) {
        rows.sort_by(|a, b| b.time().total_cmp(&a.time()));
    }
    serde_json::to_string(&rows).map_err(|error| VenueError::new(error.to_string()))
}

#[cfg(test)]
#[path = "../tests/support/history_preparation.rs"]
mod preparation_tests;
