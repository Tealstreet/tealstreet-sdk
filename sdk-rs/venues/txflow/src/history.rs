//! Indexed history is paged independently of live reconciliation.
use crate::{Pending, TxFlow};
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk};
use safe_cex_engine::{CommandId, Cx, Query, Response, VenueError, json::ArrayAt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fills,
    Orders,
    IncomeFills,
    Funding,
    Ledger,
}
/// Pages of the venue's 2000-row limit, up to offset 200000 like the TS
/// reference; the venue serves full pages while it reports more.
fn cursor_walk() -> CursorWalk {
    // Native offsets 0..=200000 in fixed 2000-row steps: 101 pages per stage.
    CursorWalk::starting_at(101, "0".into())
        .stall_on_empty_page()
        .stall_on_no_new_rows()
}

pub struct Request {
    owner: Box<CursorHistory<Row>>,
    kind: Kind,
    offset: u32,
    count: u32,
    fresh: u32,
    valid: bool,
    limit: usize,
    from: Option<f64>,
    to: Option<f64>,
    symbol: Option<String>,
    order: Option<String>,
    income_kind: Option<String>,
}
struct Row {
    time: f64,
    id: String,
    json: String,
}

#[derive(Deserialize)]
struct Envelope {
    success: bool,
    code: i32,
    data: Option<Page>,
    message: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    has_more: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Read<'a> {
    r#type: &'static str,
    user: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_type: Option<[&'static str; 1]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delta_types: Option<[&'static str; 2]>,
    offset: u32,
    limit: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fill<'a> {
    instrument_id: u32,
    tid: u64,
    oid: u64,
    time: u64,
    px: &'a str,
    sz: &'a str,
    side: &'a str,
    fee: &'a str,
    fee_token: &'a str,
    closed_pnl: &'a str,
    crossed: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeOrder<'a> {
    #[serde(borrow)]
    order: OrderBody<'a>,
    status: &'a str,
    status_timestamp: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderBody<'a> {
    instrument_id: u32,
    oid: u64,
    timestamp: u64,
    side: &'a str,
    order_type: &'a str,
    limit_px: &'a str,
    orig_sz: &'a str,
    filled_amount: &'a str,
    reduce_only: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Funding<'a> {
    time: u64,
    coin: &'a str,
    amount: &'a str,
    szi: &'a str,
    funding_rate: &'a str,
    usdc: &'a str,
}
#[derive(Deserialize)]
struct Ledger<'a> {
    time: u64,
    hash: &'a str,
    #[serde(borrow)]
    delta: Delta<'a>,
}
#[derive(Deserialize)]
struct Delta<'a> {
    r#type: &'a str,
    amount: &'a str,
    status: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution<'a> {
    id: String,
    order_id: String,
    timestamp: f64,
    symbol: &'a str,
    price: f64,
    qty: f64,
    side: OrderSide,
    commission: f64,
    commission_asset: &'a str,
    realized_pnl: f64,
    maker: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder<'a> {
    id: String,
    symbol: &'a str,
    side: OrderSide,
    #[serde(rename = "type")]
    kind: OrderType,
    status: OrderStatus,
    price: f64,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: u64,
    updated_time: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income<'a> {
    symbol: &'a str,
    income_type: &'a str,
    income: f64,
    asset: &'a str,
    timestamp: u64,
    tran_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_id: Option<String>,
    category: &'static str,
}
fn number(text: &str) -> Result<f64, String> {
    safe_cex_engine::num::parse_decimal(text)
        .ok_or_else(|| "Malformed TxFlow history number".into())
}
fn side(text: &str) -> Result<OrderSide, String> {
    match text {
        "B" => Ok(OrderSide::Buy),
        "A" => Ok(OrderSide::Sell),
        _ => Err("Malformed TxFlow history side".into()),
    }
}
impl Request {
    fn include(&self, kind: &str) -> bool {
        self.income_kind.as_deref().is_none_or(|v| v == kind)
    }
    fn within(&self, time: u64, symbol: &str) -> bool {
        self.from.is_none_or(|n| time as f64 >= n)
            && self.to.is_none_or(|n| time as f64 <= n)
            && self.symbol.as_deref().is_none_or(|s| s == symbol)
    }
    fn push(&mut self, time: u64, id: String, value: &impl Serialize) {
        self.owner.stage(Row {
            time: time as f64,
            id,
            json: serde_json::to_string(value).unwrap(),
        });
    }
}
impl TxFlow {
    pub(crate) fn query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
        let (kind, symbol, order, from, to, limit, income_kind) = match &query {
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Fills,
                symbol.clone(),
                order_id.clone(),
                *from_ms,
                *to_ms,
                *limit,
                None,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::Orders,
                symbol.clone(),
                None,
                *from_ms,
                *to_ms,
                *limit,
                None,
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => (
                Kind::IncomeFills,
                symbol.clone(),
                None,
                *from_ms,
                *to_ms,
                *limit,
                kind.clone(),
            ),
            Query::PublicTrades { .. } => {
                crate::local_context::complete(cx, safe_cex_engine::request_context::Operation::Read, Err("TxFlow does not document a REST public trades history query; use the trades stream".into()));
                return;
            }
            Query::OrderByClientId { .. } => {
                crate::local_context::complete(
                    cx,
                    safe_cex_engine::request_context::Operation::Read,
                    Err("TxFlow indexed lookup cannot prove non-placement".into()),
                );
                return;
            }
        };
        if !self.options.testnet || self.user().is_none() {
            crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Err("TxFlow testnet account identity required".into()),
            );
            return;
        }
        if from
            .into_iter()
            .chain(to)
            .any(|n| !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0)
            || from.zip(to).is_some_and(|(a, b)| a > b)
            || limit == Some(0)
        {
            crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Err("Invalid TxFlow history range or limit".into()),
            );
            return;
        }
        if symbol
            .as_ref()
            .is_some_and(|s| self.historical_assets.get(s).and_then(|id| *id).is_none())
        {
            crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Err("Unknown or ambiguous TxFlow history symbol".into()),
            );
            return;
        }
        let mut request = Request {
            owner: Box::new(CursorHistory::new(
                Completion::new(command),
                cursor_walk(),
                |_| "Incomplete TxFlow history: stalled page or offset cap".into(),
                encode_rows,
            )),
            kind,
            offset: 0,
            count: 0,
            fresh: 0,
            valid: true,
            limit: limit.map_or(usize::MAX, |n| n as usize),
            from,
            to,
            symbol,
            order,
            income_kind,
        };
        if request.kind == Kind::IncomeFills
            && !request.include("REALIZED_PNL")
            && !request.include("COMMISSION")
        {
            request.kind = Kind::Funding;
        }
        if request.kind == Kind::Funding && !request.include("FUNDING_FEE") {
            request.kind = Kind::Ledger;
        }
        if request.kind == Kind::Ledger
            && (request.symbol.is_some()
                || (!request.include("deposit") && !request.include("withdraw")))
        {
            crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                Ok("[]".into()),
            );
            return;
        }
        self.history_page(cx, request);
    }
    fn history_page(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Some(user) = self.user() else {
            request
                .owner
                .reject(cx, VenueError::new("TxFlow account identity required"));
            return;
        };
        let kind = match request.kind {
            Kind::Fills | Kind::IncomeFills => "userFills",
            Kind::Orders => "historicalOrders",
            Kind::Funding => "userFundings",
            Kind::Ledger => "userNonFundingLedgerUpdates",
        };
        let body = serde_json::to_string(&Read {
            r#type: kind,
            user,
            account_type: (request.kind != Kind::Ledger).then_some(["perp"]),
            coin: if request.kind != Kind::Ledger {
                request
                    .symbol
                    .as_ref()
                    .and_then(|s| self.historical_assets.get(s).and_then(|id| *id))
                    .map(|a| a.to_string())
            } else {
                None
            },
            start_time_ms: request.from.map(|n| n as u64),
            end_time_ms: request.to.map(|n| n as u64),
            delta_types: (request.kind == Kind::Ledger).then_some(["deposit", "withdraw"]),
            // The offset cap bounds it far below u32::MAX.
            offset: request.offset,
            limit: 2000,
        })
        .unwrap();
        self.info(
            cx,
            body,
            ArrayAt::Path(&["data", "data"]),
            Pending::History(Box::new(request)),
        );
    }
    pub(crate) fn history_item(
        &self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        request.count += 1;
        if self.map_history_item(request, item).is_err() {
            request.valid = false;
        }
    }
    fn history_symbol(&self, asset: &str) -> Option<&str> {
        self.historical_coins
            .get(&asset.to_ascii_lowercase())
            .map(String::as_str)
    }
    fn map_history_item(&self, r: &mut Request, item: &str) -> Result<(), String> {
        match r.kind {
            Kind::Fills | Kind::IncomeFills => {
                let f: Fill = serde_json::from_str(item).map_err(|e| e.to_string())?;
                let sym = self
                    .history_symbol(&f.instrument_id.to_string())
                    .ok_or("Unknown TxFlow history instrument")?;
                let key = format!("fill:{}", f.tid);
                if !r.owner.fresh(key) {
                    return Ok(());
                }
                r.fresh += 1;
                if !r.within(f.time, sym)
                    || r.order.as_ref().is_some_and(|id| *id != f.oid.to_string())
                {
                    return Ok(());
                }
                let price = number(f.px)?;
                let qty = number(f.sz)?;
                if price <= 0.0 || qty <= 0.0 {
                    return Err("Malformed TxFlow execution".into());
                }
                let commission = number(f.fee)?;
                let pnl = number(f.closed_pnl)?;
                if r.kind == Kind::Fills {
                    r.push(
                        f.time,
                        f.tid.to_string(),
                        &Execution {
                            id: f.tid.to_string(),
                            order_id: f.oid.to_string(),
                            timestamp: f.time as f64 / 1000.0,
                            symbol: sym,
                            price,
                            qty,
                            side: side(f.side)?,
                            commission,
                            commission_asset: f.fee_token,
                            realized_pnl: pnl,
                            maker: !f.crossed,
                        },
                    );
                } else {
                    for (kind, value, prefix, category, asset) in [
                        ("REALIZED_PNL", pnl, "pnl", "realized_pnl", "USDC"),
                        ("COMMISSION", -commission, "fee", "commission", f.fee_token),
                    ] {
                        if r.include(kind) {
                            let id = format!("{prefix}:{}", f.tid);
                            r.push(
                                f.time,
                                id.clone(),
                                &Income {
                                    symbol: sym,
                                    income_type: kind,
                                    income: value,
                                    asset,
                                    timestamp: f.time,
                                    tran_id: id,
                                    trade_id: Some(f.tid.to_string()),
                                    category,
                                },
                            );
                        }
                    }
                }
            }
            Kind::Orders => {
                let row: NativeOrder = serde_json::from_str(item).map_err(|e| e.to_string())?;
                let o = row.order;
                let sym = self
                    .history_symbol(&o.instrument_id.to_string())
                    .ok_or("Unknown TxFlow history instrument")?;
                let key = format!("order:{}", o.oid);
                if !r.owner.fresh(key) {
                    return Ok(());
                }
                r.fresh += 1;
                if !r.within(o.timestamp, sym) {
                    return Ok(());
                }
                let status = match row.status.to_ascii_lowercase().as_str() {
                    "filled" => OrderStatus::Closed,
                    "open" | "pending" | "partiallyfilled" | "partialfilled" | "triggered" => {
                        OrderStatus::Open
                    }
                    "canceled" | "cancelled" | "partialcanceled" | "partiallycanceled"
                    | "rejected" => OrderStatus::Canceled,
                    _ => return Err("Unknown TxFlow history order status".into()),
                };
                r.push(
                    o.timestamp,
                    o.oid.to_string(),
                    &HistoricalOrder {
                        id: o.oid.to_string(),
                        symbol: sym,
                        side: side(o.side)?,
                        kind: if o.order_type == "Market" {
                            OrderType::Market
                        } else {
                            OrderType::Limit
                        },
                        status,
                        price: number(o.limit_px)?,
                        amount: number(o.orig_sz)?,
                        filled: number(o.filled_amount)?,
                        reduce_only: o.reduce_only,
                        created_time: o.timestamp,
                        updated_time: row.status_timestamp,
                    },
                );
            }
            Kind::Funding => {
                let f: Funding = serde_json::from_str(item).map_err(|e| e.to_string())?;
                let sym = self
                    .history_symbol(f.coin)
                    .ok_or("Unknown TxFlow funding instrument")?;
                let id = format!(
                    "funding:{}:{}:{}:{}:{}:{}",
                    f.coin, f.time, f.szi, f.amount, f.funding_rate, f.usdc
                );
                number(f.szi)?;
                number(f.funding_rate)?;
                number(f.usdc)?;
                if !r.owner.fresh(id.clone()) {
                    return Err("Ambiguous TxFlow funding identity; range may be incomplete".into());
                }
                r.fresh += 1;
                if r.within(f.time, sym) {
                    r.push(
                        f.time,
                        id.clone(),
                        &Income {
                            symbol: sym,
                            income_type: "FUNDING_FEE",
                            income: number(f.amount)?,
                            asset: "USDC",
                            timestamp: f.time,
                            tran_id: id,
                            trade_id: None,
                            category: "funding",
                        },
                    );
                }
            }
            Kind::Ledger => {
                let row: Ledger = serde_json::from_str(item).map_err(|e| e.to_string())?;
                if row.hash.is_empty() {
                    return Err("Missing TxFlow ledger identity".into());
                }
                let id = format!("ledger:{}:{}", row.hash, row.delta.r#type);
                if !r.owner.fresh(id.clone()) {
                    return Ok(());
                }
                r.fresh += 1;
                if r.symbol.is_none()
                    && r.include(row.delta.r#type)
                    && matches!(row.delta.r#type, "deposit" | "withdraw")
                    && r.within(row.time, "")
                {
                    r.push(
                        row.time,
                        id.clone(),
                        &Income {
                            symbol: "",
                            income_type: row.delta.r#type,
                            income: {
                                if row.delta.status != "completed" {
                                    return Err("Unverified TxFlow ledger settlement status".into());
                                }
                                number(row.delta.amount)?
                            } * if row.delta.r#type == "withdraw" {
                                -1.0
                            } else {
                                1.0
                            },
                            asset: "USDC",
                            timestamp: row.time,
                            tran_id: id,
                            trade_id: None,
                            category: "transfer",
                        },
                    );
                }
            }
        }
        Ok(())
    }
    pub(crate) fn history_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        response: Response<'_>,
    ) {
        let valid = serde_json::from_str::<Envelope>(response.body)
            .map_err(|e| e.to_string())
            .and_then(|e| {
                if !(200..300).contains(&response.status) || !e.success || e.code != 0 {
                    return Err(e
                        .message
                        .unwrap_or_else(|| format!("TxFlow history HTTP {}", response.status)));
                }
                if !r.valid {
                    return Err("Malformed TxFlow history row".into());
                }
                e.data.ok_or_else(|| "Missing TxFlow history page".into())
            });
        let page = match valid {
            Ok(p) => p,
            Err(e) => {
                self.history_failed(cx, r, &e);
                return;
            }
        };
        match r.owner.advance(CursorPage {
            next: page.has_more.then(|| (r.offset + 2000).to_string()),
            rows: r.count as usize,
            fresh: r.fresh as usize,
            want_more: page.has_more,
        }) {
            CursorStep::More(_) => {
                r.offset += 2000;
                r.count = 0;
                r.fresh = 0;
                self.history_page(cx, r);
                return;
            }
            CursorStep::Stalled(_) => {
                r.owner.reject(
                    cx,
                    VenueError::new("Incomplete TxFlow history: stalled page or offset cap"),
                );
                return;
            }
            CursorStep::Done => {}
        }
        let next = match r.kind {
            Kind::IncomeFills if r.include("FUNDING_FEE") => Some(Kind::Funding),
            Kind::IncomeFills | Kind::Funding
                if r.symbol.is_none() && (r.include("deposit") || r.include("withdraw")) =>
            {
                Some(Kind::Ledger)
            }
            _ => None,
        };
        if let Some(kind) = next {
            r.kind = kind;
            if !r.owner.restart_stage() {
                r.owner
                    .reject(cx, VenueError::new("TxFlow history stage lifecycle failed"));
                return;
            }
            r.offset = 0;
            r.count = 0;
            r.fresh = 0;
            self.history_page(cx, r);
            return;
        }
        self.history_finish(cx, r, Ok(()));
    }
    fn history_finish(&self, cx: &mut Cx<'_, Pending>, r: Request, result: Result<(), VenueError>) {
        let kind = r.kind;
        let limit = r.limit;
        r.owner.finish_with(cx, result, |mut rows| {
            rows.sort_by(|a, b| b.time.total_cmp(&a.time).then(a.id.cmp(&b.id)));
            if kind == Kind::Fills {
                rows.sort_by(|a, b| {
                    b.time.total_cmp(&a.time).then_with(|| {
                        b.id.parse::<u64>()
                            .unwrap()
                            .cmp(&a.id.parse::<u64>().unwrap())
                    })
                });
            }
            rows.truncate(limit);
            encode_rows(&rows)
        });
    }
    pub(crate) fn history_failed(&self, cx: &mut Cx<'_, Pending>, r: Request, error: &str) {
        self.history_finish(cx, r, Err(VenueError::new(error)));
    }
}
fn encode_rows(rows: &[Row]) -> Result<String, VenueError> {
    Ok(format!(
        "[{}]",
        rows.iter()
            .map(|r| r.json.as_str())
            .collect::<Vec<_>>()
            .join(",")
    ))
}
