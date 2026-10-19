//! Rare reads stream typed rows. Private pages walk backwards using exact
//! nanosecond cursors, because many executions can share a millisecond.
use crate::account::{OrderRow, id, side};
use crate::sign::Params;
use crate::{Cryptocom, Pending};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::{CommandId, Cx, Method, Query, TimerId, num};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
enum Kind {
    Fills,
    Orders,
    Income,
    Trades,
}
pub struct Request {
    command: CommandId,
    kind: Kind,
    path: &'static str,
    symbol: Option<String>,
    order_id: Option<String>,
    income_kind: Option<String>,
    from_ns: u64,
    to_ns: u64,
    limit: usize,
    rows: Vec<Row>,
    seen: HashSet<String>,
    page_count: usize,
    oldest: u64,
    invalid: bool,
    retries: u8,
    exact_page: bool,
    committed: usize,
    page_seen: Vec<String>,
}
#[derive(Default)]
pub struct History {
    pub waiting: HashMap<TimerId, Request>,
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
    order_id: String,
    price: f64,
    qty: f64,
    side: &'static str,
    commission: f64,
    commission_asset: String,
    maker: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: &'static str,
    #[serde(rename = "type")]
    kind: safe_cex_engine::model::OrderType,
    status: safe_cex_engine::model::OrderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_price: Option<f64>,
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
fn side_name(s: &str) -> &'static str {
    if side(s) == safe_cex_engine::model::OrderSide::Buy {
        "buy"
    } else {
        "sell"
    }
}
fn exact_ns(raw: Option<&RawValue>) -> Option<u64> {
    raw.and_then(|r| id(r).parse().ok())
}
fn ns(raw: Option<&RawValue>, ms: f64) -> u64 {
    exact_ns(raw).unwrap_or_else(|| (ms as u64).saturating_mul(1_000_000))
}

#[derive(Deserialize)]
struct FillRow<'a> {
    #[serde(borrow)]
    trade_id: &'a RawValue,
    #[serde(borrow)]
    order_id: &'a RawValue,
    instrument_name: &'a str,
    side: &'a str,
    traded_price: &'a str,
    traded_quantity: &'a str,
    #[serde(default)]
    fees: &'a str,
    #[serde(default)]
    fee_instrument_name: &'a str,
    #[serde(default)]
    taker_side: &'a str,
    create_time: f64,
    #[serde(borrow, default)]
    create_time_ns: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct OrderHistoryRow<'a> {
    #[serde(default)]
    avg_price: &'a str,
    #[serde(borrow, default)]
    create_time_ns: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct IncomeRow<'a> {
    #[serde(borrow)]
    journal_id: &'a RawValue,
    instrument_name: &'a str,
    journal_type: &'a str,
    transaction_qty: &'a str,
    #[serde(default)]
    realized_pnl: &'a str,
    #[serde(borrow, default)]
    trade_id: Option<&'a RawValue>,
    event_timestamp_ms: f64,
    #[serde(borrow, default)]
    event_timestamp_ns: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct TradeRow<'a> {
    #[serde(borrow)]
    d: &'a RawValue,
    i: &'a str,
    p: &'a str,
    q: &'a str,
    s: &'a str,
    t: f64,
}

impl Request {
    fn params(&self) -> Params {
        let mut params = Params::default()
            .number("start_time", self.from_ns)
            .number("end_time", self.to_ns)
            .number("limit", 100);
        if let Some(symbol) = &self.symbol {
            params = params.text("instrument_name", symbol);
        }
        if let Some(kind) = &self.income_kind {
            params = params.text("journal_type", kind);
        }
        params
    }
    fn push(&mut self, identity: String, time: u64, row: Row, matches: bool) {
        self.page_count += 1;
        self.oldest = self.oldest.min(time);
        if time >= self.from_ns && time < self.to_ns && self.seen.insert(identity.clone()) {
            self.page_seen.push(identity);
            if matches {
                self.rows.push(row);
            }
        }
    }
}

impl Cryptocom {
    pub(crate) fn query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
        let (kind, path, symbol, order_id, income_kind, from_ms, to_ms, limit) = match query {
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Fills,
                "private/get-trades",
                symbol,
                order_id,
                None,
                from_ms,
                to_ms,
                limit,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Orders,
                "private/get-order-history",
                symbol,
                None,
                None,
                from_ms,
                to_ms,
                limit,
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Income,
                "private/get-transactions",
                symbol,
                None,
                kind,
                from_ms,
                to_ms,
                limit,
            ),
            Query::PublicTrades { symbol, limit } => (
                Kind::Trades,
                "public/get-trades",
                Some(symbol),
                None,
                None,
                None,
                None,
                limit,
            ),
        };
        if !matches!(kind, Kind::Trades) && self.credentials.is_none() {
            cx.done(command, Err("Crypto.com credentials are required".into()));
            return;
        }
        let to = to_ms.unwrap_or(cx.now);
        let from = from_ms.unwrap_or(to - 86_400_000.0);
        if !from.is_finite() || !to.is_finite() || from < 0.0 || to < from {
            cx.done(command, Err("Invalid history time range".into()));
            return;
        }
        let request = Request {
            command,
            kind,
            path,
            symbol,
            order_id,
            income_kind,
            from_ns: (from as u64).saturating_mul(1_000_000),
            to_ns: (to as u64).saturating_mul(1_000_000),
            limit: if matches!(kind, Kind::Income) && from_ms.is_some() && to_ms.is_some() {
                usize::MAX
            } else {
                limit.unwrap_or(if matches!(kind, Kind::Trades) {
                    25
                } else {
                    100
                }) as usize
            },
            rows: vec![],
            seen: HashSet::new(),
            page_count: 0,
            oldest: u64::MAX,
            invalid: false,
            retries: 0,
            exact_page: true,
            committed: 0,
            page_seen: vec![],
        };
        if request.limit == 0 || from == to {
            cx.done(command, Ok("[]".into()));
            return;
        }
        self.history_request(cx, request);
    }
    pub(crate) fn history_request(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        if matches!(request.kind, Kind::Trades) {
            let symbol = request.symbol.as_deref().unwrap_or("");
            let url = format!(
                "{}/public/get-trades?instrument_name={}&count={}",
                self.rest,
                crate::encode(symbol),
                request.limit.min(150)
            );
            cx.request_streamed(
                Method::Get,
                url,
                vec![],
                ArrayAt::Path(&["result", "data"]),
                Pending::History(request),
            );
        } else {
            self.signed(
                cx,
                request.path,
                &request.params(),
                Pending::History(request),
                true,
            );
        }
    }
    pub(crate) fn history_item(&self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        let parsed = match request.kind {
            Kind::Fills => serde_json::from_str::<FillRow>(item).map(|r| {
                request.exact_page &= exact_ns(r.create_time_ns).is_some();
                let time = ns(r.create_time_ns, r.create_time);
                let order = id(r.order_id);
                let matches = request.order_id.as_ref().is_none_or(|id| *id == order);
                let identity = id(r.trade_id);
                let row = Execution {
                    id: identity.clone(),
                    timestamp: (r.create_time / 1000.0).floor(),
                    symbol: r.instrument_name.into(),
                    order_id: order,
                    price: num::parse(r.traded_price),
                    qty: num::parse(r.traded_quantity),
                    side: side_name(r.side),
                    commission: -num::parse(r.fees),
                    commission_asset: r.fee_instrument_name.into(),
                    maker: r.taker_side == "MAKER",
                };
                request.push(identity, time, Row::Fill(row), matches);
            }),
            Kind::Orders => serde_json::from_str::<OrderRow>(item).and_then(|raw| {
                let r: OrderHistoryRow<'_> = serde_json::from_str(item)?;
                request.exact_page &= exact_ns(r.create_time_ns).is_some();
                let order = raw.order(0.0);
                let created = raw.create_time.or(raw.time_created).unwrap_or(0.0);
                let row = HistoricalOrder {
                    id: order.id.clone(),
                    symbol: order.symbol,
                    side: side_name(raw.side),
                    kind: order.kind,
                    status: order.status,
                    price: raw.limit_price.map(num::parse).filter(|p| *p > 0.0),
                    average_price: Some(num::parse(r.avg_price)).filter(|p| *p > 0.0),
                    amount: order.amount,
                    filled: order.filled,
                    reduce_only: order.reduce_only,
                    created_time: created,
                    updated_time: order.last_update,
                };
                request.push(
                    order.id,
                    ns(r.create_time_ns, created),
                    Row::Order(row),
                    true,
                );
                Ok(())
            }),
            Kind::Income => serde_json::from_str::<IncomeRow>(item).map(|r| {
                request.exact_page &= exact_ns(r.event_timestamp_ns).is_some();
                let identity = id(r.journal_id);
                let (category, amount) = match r.journal_type {
                    "REALIZED_PNL" | "TRADING" | "SESSION_SETTLE" | "SESSION_RESET" => {
                        ("realized_pnl", num::parse(r.realized_pnl))
                    }
                    "FUNDING" => ("funding", num::parse(r.transaction_qty)),
                    "TRADE_FEE" | "LIQUIDATION_FEE" => {
                        ("commission", num::parse(r.transaction_qty))
                    }
                    "ONCHAIN_WITHDRAWAL" | "ONCHAIN_DEPOSIT" | "SUBACCOUNT_TX" => {
                        ("transfer", num::parse(r.transaction_qty))
                    }
                    _ => ("other", num::parse(r.transaction_qty)),
                };
                let row = Income {
                    symbol: r.instrument_name.into(),
                    income_type: r.journal_type.into(),
                    income: amount,
                    asset: cx
                        .store
                        .market(r.instrument_name)
                        .map_or_else(|| r.instrument_name.into(), |m| m.quote.clone()),
                    timestamp: r.event_timestamp_ms,
                    tran_id: identity.clone(),
                    trade_id: r.trade_id.map(id).filter(|i| i != "0" && i != "null"),
                    category,
                };
                request.push(
                    identity,
                    ns(r.event_timestamp_ns, r.event_timestamp_ms),
                    Row::Income(row),
                    true,
                );
            }),
            Kind::Trades => serde_json::from_str::<TradeRow>(item).map(|r| {
                let price = num::parse(r.p);
                let qty = num::parse(r.q);
                request.page_count += 1;
                request.rows.push(Row::Trade(Trade {
                    id: id(r.d),
                    symbol: r.i.into(),
                    price,
                    qty,
                    quote_qty: price * qty,
                    timestamp: r.t,
                    side: side_name(r.s),
                }));
            }),
        };
        request.invalid |= parsed.is_err();
    }
    pub(crate) fn history_response(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        if request.invalid {
            cx.done(request.command, Err("invalid history row".into()));
            return;
        }
        if !matches!(request.kind, Kind::Trades)
            && request.page_count > 0
            && request.rows.len() < request.limit
        {
            if request.page_seen.is_empty() && request.page_count < 100 {
                request.rows.truncate(request.limit);
                cx.done(
                    request.command,
                    Ok(serde_json::to_string(&request.rows).unwrap()),
                );
                return;
            }
            if !request.exact_page {
                cx.done(
                    request.command,
                    Err("Crypto.com history page omitted nanosecond cursors".into()),
                );
                return;
            }
            let next = request.oldest.saturating_add(1);
            if request.oldest < request.from_ns { /* range exhausted */
            } else if next >= request.to_ns {
                cx.done(
                    request.command,
                    Err("Crypto.com history cursor did not advance".into()),
                );
                return;
            } else {
                request.committed = request.rows.len();
                request.page_seen.clear();
                request.exact_page = true;
                request.to_ns = next;
                request.page_count = 0;
                request.oldest = u64::MAX;
                request.retries = 0;
                let timer = cx.set_timer(1000.0);
                self.history.waiting.insert(timer, request);
                return;
            }
        }
        request.rows.truncate(request.limit);
        cx.done(
            request.command,
            Ok(serde_json::to_string(&request.rows).unwrap()),
        );
    }
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        error: &str,
        transport: bool,
    ) {
        let retry = if error.contains("40101") {
            request.retries < 1
        } else {
            transport && request.retries < 3
        };
        if retry {
            // A failed page is never appended; preserve earlier complete pages.
            request.rows.truncate(request.committed);
            for identity in request.page_seen.drain(..) {
                request.seen.remove(&identity);
            }
            request.page_count = 0;
            request.oldest = u64::MAX;
            request.invalid = false;
            request.exact_page = true;
            request.retries += 1;
            let timer = cx.set_timer(1000.0);
            self.history.waiting.insert(timer, request);
        } else {
            cx.done(request.command, Err(error.into()));
        }
    }
}
