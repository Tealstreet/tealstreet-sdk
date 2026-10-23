//! Native cursor reads; response lists stream and never enter the flush frame.
use crate::wire::{Envelope, Identifier, Number};
use crate::{BitgetFutures, Pending, sign};
use safe_cex_engine::paging::{self, WindowOrder};
use safe_cex_engine::request_context::{CommandEntryError, Context, Operation, Owner};
use safe_cex_engine::{CommandWait, Cx, Method, Query, Response, TimerId, json::ArrayAt, num};
use serde::{Deserialize, Serialize, de::IgnoredAny};
use std::collections::{HashSet, VecDeque};

const DAY: f64 = 86_400_000.0;
const MAX_PAGES: u32 = 1000;
const ARRAYS: &[&[&str]] = &[
    &["data", "fillList"],
    &["data", "entrustedList"],
    &["data", "bills"],
    &["data", "list"],
    &["data"],
];

#[derive(Default)]
pub struct History {
    waiting: Vec<(TimerId, Request)>,
}

pub struct Request {
    /// The command's original caller-wait Read root; the engine expires it.
    root: Context,
    /// The physical page in flight, retired once its page is settled.
    leg: Option<Context>,
    detect_deadline: f64,
    query: Query,
    uta: bool,
    windows: VecDeque<(Option<f64>, Option<f64>)>,
    window: (Option<f64>, Option<f64>),
    cursor: Option<String>,
    cursors: HashSet<String>,
    seen: HashSet<String>,
    pages: u32,
    retries: u8,
    rows: Vec<Row>,
    page: Vec<Row>,
    page_count: usize,
    invalid: bool,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Row {
    Fill(Execution),
    Order(HistoricalOrder),
    Income(Income),
    Trade(Trade),
}
impl Row {
    fn identity(&self) -> String {
        match self {
            Self::Fill(r) => format!("{}|{}", r.symbol, r.id),
            Self::Order(r) => r.id.clone(),
            Self::Income(r) => r.tran_id.clone(),
            Self::Trade(r) => r.id.clone(),
        }
    }
    fn timestamp(&self) -> f64 {
        match self {
            Self::Fill(r) => r.timestamp * 1000.0,
            Self::Order(r) => r.created_time,
            Self::Income(r) => r.timestamp,
            Self::Trade(r) => r.timestamp,
        }
    }
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
    kind: safe_cex_engine::model::OrderType,
    status: safe_cex_engine::model::OrderStatus,
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
struct Income {
    symbol: String,
    income_type: String,
    income: f64,
    asset: String,
    timestamp: f64,
    tran_id: String,
    category: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fee: Option<f64>,
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

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Native<'a> {
    #[serde(borrow)]
    symbol: &'a str,
    #[serde(borrow)]
    category: &'a str,
    #[serde(borrow)]
    trade_id: Identifier<'a>,
    #[serde(borrow)]
    exec_id: Identifier<'a>,
    #[serde(borrow)]
    exec_link_id: Identifier<'a>,
    #[serde(borrow)]
    order_id: Identifier<'a>,
    #[serde(borrow)]
    bill_id: Identifier<'a>,
    #[serde(borrow)]
    side: &'a str,
    #[serde(borrow)]
    trade_side: &'a str,
    #[serde(borrow)]
    pos_side: &'a str,
    #[serde(borrow)]
    hold_side: &'a str,
    #[serde(borrow)]
    pos_mode: &'a str,
    #[serde(borrow)]
    hold_mode: &'a str,
    #[serde(borrow)]
    trade_scope: &'a str,
    #[serde(borrow)]
    order_type: &'a str,
    #[serde(borrow)]
    status: &'a str,
    #[serde(borrow)]
    state: &'a str,
    #[serde(borrow)]
    order_status: &'a str,
    #[serde(borrow)]
    reduce_only: &'a str,
    #[serde(borrow)]
    business_type: &'a str,
    #[serde(borrow)]
    coin: &'a str,
    #[serde(borrow)]
    fee_detail: Vec<Fee<'a>>,
    price: Number,
    exec_price: Number,
    price_avg: Number,
    avg_price: Number,
    size: Number,
    qty: Number,
    base_volume: Number,
    exec_qty: Number,
    cum_exec_qty: Number,
    profit: Option<Number>,
    exec_pnl: Number,
    amount: Number,
    fee: Option<Number>,
    c_time: Number,
    u_time: Number,
    created_time: Number,
    updated_time: Number,
    ts: Number,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fee<'a> {
    #[serde(default, borrow)]
    fee_coin: &'a str,
    #[serde(default)]
    total_fee: Number,
    #[serde(default)]
    fee: Number,
}
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Cursor<'a> {
    #[serde(borrow)]
    cursor: Option<Identifier<'a>>,
    #[serde(borrow)]
    end_id: Option<Identifier<'a>>,
}

// https://www.bitget.com/docs/catalog/account/assets-balance#get-financial-records
#[derive(Deserialize)]
struct UtaBill<'a> {
    #[serde(borrow)]
    id: Identifier<'a>,
    #[serde(default, borrow)]
    category: &'a str,
    #[serde(default, borrow)]
    symbol: &'a str,
    #[serde(borrow)]
    coin: &'a str,
    #[serde(rename = "type", borrow)]
    kind: &'a str,
    amount: Number,
    #[serde(default)]
    fee: Option<Number>,
    ts: Number,
}

impl BitgetFutures {
    pub(crate) fn start_query(&mut self, cx: &mut Cx<'_, Pending>, query: Query) {
        let public = matches!(query, Query::PublicTrades { .. });
        let private = !public && self.private_enabled();
        // History is never the degraded balance read, so the read key applies.
        let credential = private.then(|| self.read_credential("")).flatten();
        let root = match cx.admit_caller_read_root(credential, None) {
            Ok(root) => root,
            // An unstamped receipt or admission refusal already published its failure.
            Err(CommandEntryError::Rejected(_)) => return,
            Err(CommandEntryError::Revoked) => {
                if cx.command_wait() == Some(CommandWait::Expired) {
                    let _ = cx.reject_expired_command(Operation::Read);
                }
                return;
            }
        };
        if !public && !private {
            let _ = cx.fail_context(&root, "Bitget credentials are required");
            return;
        }
        let (from, to) = match &query {
            Query::OrderByClientId { .. } => {
                let _ = cx.fail_context(&root, "Bitget client-order-id query is not supported");
                return;
            }
            Query::Fills { from_ms, to_ms, .. }
            | Query::OrderHistory { from_ms, to_ms, .. }
            | Query::Income { from_ms, to_ms, .. } => (*from_ms, *to_ms),
            Query::PublicTrades { .. } => (None, None),
        };
        if from
            .into_iter()
            .chain(to)
            .any(|n| !n.is_finite() || n < 0.0)
            || matches!((from,to),(Some(a),Some(b)) if a>b)
        {
            let _ = cx.fail_context(&root, "invalid Bitget history range");
            return;
        }
        // Supplying one boundary still produces a bounded native request.
        let from = if !public
            && from.is_none()
            && let Some(end) = to
        {
            Some((end - 30.0 * DAY + 1.0).max(0.0))
        } else {
            from
        };
        let to = if !public && from.is_some() {
            Some(to.unwrap_or(cx.now))
        } else {
            to
        };
        if matches!((from,to),(Some(a),Some(b)) if a>b) {
            let _ = cx.fail_context(&root, "invalid Bitget history range");
            return;
        }
        let mut windows: VecDeque<_> = if let (Some(a), Some(b)) = (from, to) {
            match paging::windows(a, b, 30.0 * DAY - 1.0, WindowOrder::OldestFirst, 1000) {
                Some(windows) => windows
                    .into_iter()
                    .map(|(a, b)| (Some(a), Some(b)))
                    .collect(),
                None => {
                    let _ = cx.fail_context(&root, "Bitget history range exceeds 1000 windows");
                    return;
                }
            }
        } else {
            VecDeque::from([(from, to)])
        };
        let window = windows.pop_front().expect("one window");
        let request = Request {
            root,
            leg: None,
            detect_deadline: cx.now + 10_000.0,
            query,
            uta: self.account.uta,
            windows,
            window,
            cursor: None,
            cursors: HashSet::new(),
            seen: HashSet::new(),
            pages: 0,
            retries: 0,
            rows: vec![],
            page: vec![],
            page_count: 0,
            invalid: false,
        };
        if !public && !self.account.detected {
            self.history.waiting.push((cx.set_timer(100.0), request));
        } else {
            self.query_request(cx, request);
        }
    }
    fn query_request(&mut self, cx: &mut Cx<'_, Pending>, mut r: Request) {
        let public = matches!(r.query, Query::PublicTrades { .. });
        r.uta = self.account.uta;
        let (endpoint, symbol, order, limit) = match &r.query {
            Query::OrderByClientId { .. } => unreachable!("start_query refuses client-ID lookups"),
            Query::Fills {
                symbol,
                order_id,
                limit,
                ..
            } => (
                if r.uta {
                    "/api/v3/trade/fills"
                } else {
                    "/api/v2/mix/order/fills"
                },
                symbol.as_deref(),
                order_id.as_deref(),
                *limit,
            ),
            Query::OrderHistory { symbol, limit, .. } => (
                if r.uta {
                    "/api/v3/trade/history-orders"
                } else {
                    "/api/v2/mix/order/orders-history"
                },
                symbol.as_deref(),
                None,
                *limit,
            ),
            Query::Income { limit, .. } => (
                if r.uta {
                    "/api/v3/account/financial-records"
                } else {
                    "/api/v2/mix/account/bill"
                },
                None,
                None,
                *limit,
            ),
            Query::PublicTrades { symbol, limit } => (
                "/api/v2/mix/market/fills",
                Some(symbol.as_str()),
                None,
                *limit,
            ),
        };
        let mut params = vec![
            (
                if r.uta && !public {
                    "category"
                } else {
                    "productType"
                },
                if r.uta && !public {
                    self.product().to_ascii_uppercase()
                } else {
                    self.product().into()
                },
            ),
            ("limit", limit.unwrap_or(100).clamp(1, 100).to_string()),
        ];
        if let Some(symbol) = symbol
            && !(r.uta && matches!(r.query, Query::Fills { .. }))
        {
            params.push(("symbol", symbol.into()));
        }
        if let Some(order) = order {
            params.push(("orderId", order.into()));
        }
        if let Some(a) = r.window.0 {
            params.push(("startTime", num::format(a)));
        }
        if let Some(b) = r.window.1 {
            params.push(("endTime", num::format(b)));
        }
        if let Some(c) = &r.cursor {
            params.push((if r.uta { "cursor" } else { "idLessThan" }, c.clone()));
        }
        let path = format!(
            "{endpoint}?{}",
            params
                .iter()
                .map(|(k, v)| format!("{k}={}", sign::encode(v)))
                .collect::<Vec<_>>()
                .join("&")
        );
        let at = if r.uta && matches!(r.query, Query::Income { .. }) {
            ArrayAt::Path(&["data", "list"])
        } else {
            ArrayAt::Envelope(ARRAYS)
        };
        // Each physical page is minted at its own dequeue, capped by the root.
        let leg =
            match cx.http_leg_context(&r.root, crate::read_policy(r.root.credential().cloned())) {
                Ok(leg) => leg,
                Err(error) => {
                    let _ = cx.fail_context(&r.root, error.read_venue_error());
                    return;
                }
            };
        r.leg = Some(leg.clone());
        let url = format!("{}{path}", crate::paths::REST);
        let queued = if public {
            cx.request_managed(leg, Method::Get, url, None, Pending::History(Box::new(r)))
        } else {
            let recipe = sign::Http::new(self.credential_identity(r.root.credential()), path);
            cx.request_signed_managed(
                leg,
                Method::Get,
                url,
                None,
                Box::new(recipe),
                Pending::History(Box::new(r)),
            )
        };
        match queued {
            Ok(id) => cx.stream_response(id, at),
            Err((Pending::History(r), error)) => {
                self.history_sign_failed(cx, *r, error.venue_error())
            }
            Err(_) => unreachable!("history queues only its own continuation"),
        }
    }
    pub(crate) fn query_item(&self, cx: &Cx<'_, Pending>, r: &mut Request, item: &str) {
        if r.uta && matches!(r.query, Query::Income { .. }) {
            self.query_uta_bill(r, item);
            return;
        }
        let Ok(n) = serde_json::from_str::<Native<'_>>(item) else {
            r.invalid = true;
            return;
        };
        r.page_count += 1;
        let symbol = n.symbol.to_ascii_uppercase();
        if symbol.is_empty() && !matches!(r.query, Query::Income { .. }) {
            r.invalid = true;
            return;
        }
        if !n.category.is_empty() && !n.category.eq_ignore_ascii_case(self.product()) {
            return;
        }
        let requested = match &r.query {
            Query::OrderByClientId { .. } => {
                r.invalid = true;
                return;
            }
            Query::Fills { symbol, .. }
            | Query::OrderHistory { symbol, .. }
            | Query::Income { symbol, .. } => symbol.as_deref(),
            Query::PublicTrades { symbol, .. } => Some(symbol.as_str()),
        };
        if requested.is_some_and(|s| !s.eq_ignore_ascii_case(&symbol)) {
            return;
        }
        match &r.query {
            Query::OrderByClientId { .. } => {
                r.invalid = true;
            }
            Query::Fills { order_id, .. } => {
                if order_id
                    .as_ref()
                    .is_some_and(|id| n.order_id.text().as_ref() != id)
                {
                    return;
                }
                if !r.uta && n.profit.is_some_and(|profit| profit.0.is_none()) {
                    r.invalid = true;
                    return;
                }
                let id = if r.uta {
                    if !n.exec_id.text().is_empty() {
                        n.exec_id.text()
                    } else {
                        n.exec_link_id.text()
                    }
                } else {
                    n.trade_id.text()
                };
                let price = if r.uta {
                    n.exec_price.get()
                } else {
                    n.price.get()
                };
                let qty = if r.uta {
                    n.exec_qty.get()
                } else {
                    n.base_volume.get()
                };
                let time = if r.uta {
                    n.created_time.get()
                } else {
                    n.c_time.get()
                };
                let Some((position_side, side)) = fill_direction(&n, r.uta) else {
                    r.invalid = true;
                    return;
                };
                if id.is_empty() || symbol.is_empty() || time <= 0.0 || price <= 0.0 || qty <= 0.0 {
                    r.invalid = true;
                    return;
                }
                if n.fee_detail.iter().any(|f| {
                    if r.uta {
                        f.fee.0.or(f.total_fee.0).is_none()
                    } else {
                        f.total_fee.0.is_none()
                    }
                }) {
                    r.invalid = true;
                    return;
                }
                let fee = n
                    .fee_detail
                    .iter()
                    .map(|f| {
                        if r.uta {
                            f.fee.0.or(f.total_fee.0).unwrap_or(0.0)
                        } else {
                            f.total_fee.get()
                        }
                    })
                    .sum::<f64>();
                let commission_asset = if let Some(asset) = n
                    .fee_detail
                    .first()
                    .map(|f| f.fee_coin)
                    .filter(|s| !s.is_empty())
                {
                    asset
                } else if self.inverse() {
                    let Some(market) = cx.store.market(&symbol) else {
                        r.invalid = true;
                        return;
                    };
                    market.base.as_str()
                } else {
                    self.coin()
                };
                r.page.push(Row::Fill(Execution {
                    id: id.into_owned(),
                    timestamp: (time / 1000.0).floor(),
                    symbol,
                    order_id: n.order_id.text().into_owned(),
                    price,
                    qty,
                    side,
                    commission: if r.uta { fee } else { -fee },
                    commission_asset: commission_asset.into(),
                    position_side,
                    realized_pnl: if r.uta {
                        n.exec_pnl.0
                    } else {
                        Some(n.profit.map(Number::get).unwrap_or(0.0))
                    },
                    maker: match n.trade_scope {
                        "maker" => Some(true),
                        "taker" => Some(false),
                        _ => None,
                    },
                }));
            }
            Query::OrderHistory { .. } => {
                let amount = if r.uta { n.qty.0 } else { n.size.0 };
                let filled = if r.uta {
                    n.cum_exec_qty.0
                } else {
                    n.base_volume.0
                };
                if !amount.is_some_and(|v| v > 0.0)
                    || !filled.is_some_and(|v| v >= 0.0 && v <= amount.unwrap_or(0.0) + 1e-10)
                {
                    r.invalid = true;
                    return;
                }
                let state = if r.uta {
                    n.order_status
                } else if !n.status.is_empty() {
                    n.status
                } else {
                    n.state
                };
                let Some(status) = crate::account::order_status(state) else {
                    r.invalid = true;
                    return;
                };
                let Some(kind) = crate::account::order_type(n.order_type) else {
                    r.invalid = true;
                    return;
                };
                let Some((position_side, side)) = order_direction(&n, r.uta) else {
                    r.invalid = true;
                    return;
                };
                let time = if r.uta {
                    n.created_time.get()
                } else {
                    n.c_time.get()
                };
                if n.order_id.text().is_empty() || symbol.is_empty() || time <= 0.0 {
                    r.invalid = true;
                    return;
                }
                r.page.push(Row::Order(HistoricalOrder {
                    id: n.order_id.text().into_owned(),
                    symbol,
                    side,
                    kind,
                    status,
                    price: n.price.0.filter(|p| *p > 0.0),
                    average_price: if r.uta { n.avg_price.0 } else { n.price_avg.0 }
                        .filter(|p| *p > 0.0),
                    amount: if r.uta { n.qty.get() } else { n.size.get() },
                    filled: if r.uta {
                        n.cum_exec_qty.get()
                    } else {
                        n.base_volume.get()
                    },
                    reduce_only: n.reduce_only.eq_ignore_ascii_case("yes")
                        || is_closing_trade(n.trade_side),
                    position_side,
                    created_time: time,
                    updated_time: if r.uta { n.updated_time.0 } else { n.u_time.0 },
                }));
            }
            Query::Income { kind, .. } => {
                let time = n.c_time.get();
                let id = n.bill_id.text();
                if n.amount.0.is_none() || n.fee.is_some_and(|fee| fee.0.is_none()) {
                    r.invalid = true;
                    return;
                }
                if id.is_empty() || n.business_type.is_empty() || time <= 0.0 {
                    r.invalid = true;
                    return;
                }
                let trade = matches!(
                    n.business_type,
                    "buy" | "sell" | "open_long" | "open_short" | "close_long" | "close_short"
                );
                let category = match n.business_type {
                    "contract_settle_fee" => "funding",
                    "trans_from_exchange"
                    | "trans_to_exchange"
                    | "transfer_in"
                    | "transfer_out" => "transfer",
                    _ => "other",
                };
                let parts = if trade {
                    vec![
                        (
                            "pnl",
                            "realized_pnl",
                            n.business_type.to_owned(),
                            n.amount.get(),
                        ),
                        (
                            "fee",
                            "commission",
                            format!("{}_fee", n.business_type),
                            n.fee.map(Number::get).unwrap_or(0.0),
                        ),
                    ]
                } else {
                    vec![(
                        category,
                        category,
                        n.business_type.to_owned(),
                        n.amount.get() + n.fee.map(Number::get).unwrap_or(0.0),
                    )]
                };
                for (suffix, category, income_type, income) in parts {
                    if income != 0.0 && kind.as_ref().is_none_or(|k| k == &income_type) {
                        r.page.push(Row::Income(Income {
                            symbol: symbol.clone(),
                            income_type,
                            income,
                            asset: if n.coin.is_empty() {
                                self.coin()
                            } else {
                                n.coin
                            }
                            .into(),
                            timestamp: time,
                            tran_id: format!("{id}:{suffix}"),
                            category,
                            fee: None,
                        }));
                    }
                }
            }
            Query::PublicTrades { symbol, .. } => {
                let Some(side) = side(n.side) else {
                    r.invalid = true;
                    return;
                };
                if n.trade_id.text().is_empty()
                    || n.price.get() <= 0.0
                    || n.size.get() <= 0.0
                    || n.ts.get() <= 0.0
                {
                    r.invalid = true;
                    return;
                }
                r.page.push(Row::Trade(Trade {
                    id: n.trade_id.text().into_owned(),
                    symbol: symbol.clone(),
                    price: n.price.get(),
                    qty: n.size.get(),
                    quote_qty: crate::account::clean(n.price.get() * n.size.get()),
                    timestamp: n.ts.get(),
                    side,
                }));
            }
        }
    }
    fn query_uta_bill(&self, r: &mut Request, item: &str) {
        let Ok(n) = serde_json::from_str::<UtaBill<'_>>(item) else {
            r.invalid = true;
            return;
        };
        r.page_count += 1;
        if !n.category.is_empty() && !n.category.eq_ignore_ascii_case(self.product()) {
            return;
        }
        let Query::Income { symbol, kind, .. } = &r.query else {
            return;
        };
        if symbol
            .as_ref()
            .is_some_and(|s| !s.eq_ignore_ascii_case(n.symbol))
            || kind.as_ref().is_some_and(|k| k != n.kind)
        {
            return;
        }
        let id = n.id.text();
        if id.is_empty()
            || n.kind.is_empty()
            || n.coin.is_empty()
            || n.ts.get() <= 0.0
            || n.amount.0.is_none()
            || n.fee.is_some_and(|fee| fee.0.is_none())
        {
            r.invalid = true;
            return;
        }
        // Amount is the venue's signed ledger change. Keep its fee as metadata;
        // the v3 schema does not establish that adding it again is a new delta.
        r.page.push(Row::Income(Income {
            symbol: n.symbol.to_ascii_uppercase(),
            income_type: n.kind.into(),
            income: n.amount.get(),
            asset: n.coin.into(),
            timestamp: n.ts.get(),
            tran_id: format!("{id}:{}", uta_income_category(n.kind)),
            category: uta_income_category(n.kind),
            fee: n.fee.and_then(|f| f.0),
        }));
    }
    pub(crate) fn query_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        response: Response<'_>,
    ) {
        let leg = r.leg.take().expect("a delivered page has its leg");
        let delivered = cx.validate_read_http_delivery(&r.root, &leg);
        let _ = cx.finish_http_context(&leg);
        if let Err(error) = delivered {
            let _ = cx.fail_context(&r.root, error.read_venue_error());
            return;
        }
        if serde_json::from_str::<Envelope<IgnoredAny>>(response.body)
            .ok()
            .is_some_and(|e| e.code.0 == Some(40085.0))
            && self.promote_uta(cx)
        {
            r.page.clear();
            r.page_count = 0;
            r.invalid = false;
            r.detect_deadline = cx.now + 10_000.0;
            self.history.waiting.push((cx.set_timer(100.0), r));
            return;
        }
        let valid = serde_json::from_str::<Envelope<IgnoredAny>>(response.body)
            .map_err(|e| e.to_string())
            .and_then(Envelope::checked);
        if response.status != 200 || valid.is_err() || r.invalid {
            let error = valid
                .err()
                .unwrap_or_else(|| format!("Bitget malformed history or HTTP {}", response.status));
            self.query_failed(cx, r, &error);
            return;
        }
        let public = matches!(r.query, Query::PublicTrades { .. });
        let cursor = if public {
            None
        } else {
            match serde_json::from_str::<Envelope<Cursor<'_>>>(response.body)
                .ok()
                .and_then(|e| e.data)
            {
                Some(c) => c
                    .cursor
                    .or(c.end_id)
                    .map(|id| id.text().into_owned())
                    .filter(|s| !s.is_empty()),
                None => {
                    self.query_failed(cx, r, "Bitget invalid history cursor");
                    return;
                }
            }
        };
        let cursor = cursor.filter(|_| r.page_count > 0);
        r.page_count = 0;
        for row in r.page.drain(..) {
            if r.seen.insert(row.identity()) {
                r.rows.push(row);
            }
        }
        r.retries = 0;
        r.pages += 1;
        if let Some(cursor) = cursor {
            if !r.cursors.insert(cursor.clone()) {
                let _ = cx.fail_context(
                    &r.root,
                    "Bitget history cursor did not advance; result incomplete",
                );
                return;
            }
            if r.pages >= MAX_PAGES {
                let _ = cx.fail_context(
                    &r.root,
                    "Bitget history page cap reached; result incomplete",
                );
                return;
            }
            r.cursor = Some(cursor);
            self.history.waiting.push((cx.set_timer(100.0), r));
            return;
        }
        if let Some(window) = r.windows.pop_front() {
            r.window = window;
            r.cursor = None;
            r.cursors.clear();
            self.history.waiting.push((cx.set_timer(100.0), r));
            return;
        }
        r.rows
            .sort_by(|a, b| b.timestamp().total_cmp(&a.timestamp()));
        let _ = match serde_json::to_string(&r.rows).map_err(|e| e.to_string()) {
            Ok(data) => cx.done_context(&r.root, data),
            Err(error) => cx.fail_context(&r.root, error),
        };
    }
    pub(crate) fn query_failed(&mut self, cx: &mut Cx<'_, Pending>, mut r: Request, error: &str) {
        if let Some(leg) = r.leg.take() {
            let _ = cx.finish_http_context(&leg);
        }
        if r.retries >= 3 {
            let _ = cx.fail_context(&r.root, format!("Bitget history incomplete: {error}"));
            return;
        }
        r.retries += 1;
        r.page.clear();
        r.page_count = 0;
        r.invalid = false;
        self.history
            .waiting
            .push((cx.set_timer(250.0 * f64::from(r.retries)), r));
    }
    pub(crate) fn history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(i) = self.history.waiting.iter().position(|(t, _)| *t == timer) {
            let (_, r) = self.history.waiting.swap_remove(i);
            if !matches!(r.query, Query::PublicTrades { .. }) && !self.account.detected {
                if cx.now >= r.detect_deadline {
                    let _ = cx.fail_context(&r.root, "Bitget account detection did not complete");
                    return;
                }
                self.history.waiting.push((cx.set_timer(100.0), r));
            } else {
                self.query_request(cx, r);
            }
        }
    }
    /// A provider or queue refusal before the page left: strict, typed, no retry.
    pub(crate) fn history_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        error: safe_cex_engine::VenueError,
    ) {
        if let Some(leg) = r.leg.take() {
            let _ = cx.finish_http_context(&leg);
        }
        let _ = cx.fail_context(&r.root, error);
    }
    /// The engine expired this root and sealed its command; drop every parked
    /// page of it. Their timers fire later and find nothing.
    pub(crate) fn history_root_retired(&mut self, owner: Owner) {
        self.history
            .waiting
            .retain(|(_, r)| r.root.owner() != owner);
    }
}
fn side(s: &str) -> Option<&'static str> {
    if s.eq_ignore_ascii_case("buy") {
        Some("buy")
    } else if s.eq_ignore_ascii_case("sell") {
        Some("sell")
    } else {
        None
    }
}
// https://www.bitget.com/docs/uta/enum#type-financial-records
fn uta_income_category(kind: &str) -> &'static str {
    match kind {
        "CONTRACT_MAIN_SETTLE_FEE_USER_IN"
        | "CONTRACT_MAIN_SETTLE_FEE_USER_OUT"
        | "MARGIN_SETTLE_FEE_USER_IN"
        | "MARGIN_SETTLE_FEE_USER_OUT"
        | "RWA_CROSS_SETTLE_FEE_USER_IN"
        | "RWA_CROSS_SETTLE_FEE_USER_OUT"
        | "RWA_FIXED_SETTLE_FEE_USER_IN"
        | "RWA_FIXED_SETTLE_FEE_USER_OUT"
        | "RWA_CONTRACT_MAIN_SETTLE_FEE_USER_IN"
        | "RWA_CONTRACT_MAIN_SETTLE_FEE_USER_OUT" => "funding",
        "ORDER_PLF_FEE_OUT" => "commission",
        "TRANSFER_IN"
        | "TRANSFER_OUT"
        | "RESERVE_TRANSFER_IN"
        | "RESERVE_TRANSFER_OUT"
        | "LIQ_TRANSFER_IN"
        | "LIQ_TRANSFER_OUT"
        | "ON_CHAIN_TRANSFER_REFUND"
        | "ON_CHAIN_TRANSFER_OUT"
        | "MT5_TRANSFER_IN"
        | "MT5_REFUND_IN"
        | "MT5_TRANSFER_OUT"
        | "TRACE_TRANSFER_USER_OUT"
        | "TRACE_TRANSFER_USER_IN"
        | "TRACE_TRANSFER_REFUND_IN"
        | "FINANCIAL_TRANSFER_OUT"
        | "FINANCIAL_TRANSFER_IN"
        | "CONVERT_TRANSFER_IN"
        | "CONVERT_TRANSFER_OUT"
        | "BGPAY_TRANSFER_OUT"
        | "BGPAY_REFUND_IN"
        | "WITHDRAW_TRANSFER_OUT"
        | "WITHDRAW_TRANSFER_IN" => "transfer",
        "BURST_CLOSE_LONG"
        | "BURST_CLOSE_SHORT"
        | "BURST_BUY_SSM"
        | "BURST_SELL_SSM"
        | "FIXED_BURST_CLOSE_LONG"
        | "FIXED_BURST_CLOSE_SHORT"
        | "FIXED_BURST_BUY_SSM"
        | "FIXED_BURST_SELL_SSM"
        | "LIQ_FEE" => "liquidation",
        _ => "other",
    }
}
fn is_closing_trade(trade: &str) -> bool {
    trade.starts_with("close") || trade.ends_with("_close_long") || trade.ends_with("_close_short")
}
fn fill_direction(n: &Native<'_>, uta: bool) -> Option<(&'static str, &'static str)> {
    let raw = side(n.side)?;
    let trade = n.trade_side.to_ascii_lowercase();
    if !uta && trade.ends_with("_single") {
        return Some(("both", raw));
    }
    let explicit = match if n.pos_side.is_empty() {
        n.hold_side
    } else {
        n.pos_side
    } {
        "long" => Some("long"),
        "short" => Some("short"),
        _ => None,
    }
    .or_else(|| {
        if trade.contains("long") {
            Some("long")
        } else if trade.contains("short") {
            Some("short")
        } else {
            None
        }
    });
    let opening = trade.starts_with("open");
    let closing = is_closing_trade(&trade);
    if uta {
        let leg = explicit.or_else(|| {
            if opening {
                Some(if raw == "buy" { "long" } else { "short" })
            } else if closing {
                Some(if raw == "sell" { "long" } else { "short" })
            } else {
                None
            }
        })?;
        return Some((leg, raw));
    }
    if !opening && !closing {
        return None;
    }
    let leg = explicit.unwrap_or(if raw == "buy" { "long" } else { "short" });
    Some((
        leg,
        if (leg == "long") == opening {
            "buy"
        } else {
            "sell"
        },
    ))
}
fn order_direction(n: &Native<'_>, uta: bool) -> Option<(&'static str, &'static str)> {
    if n.pos_mode == "one_way_mode" || n.hold_mode == "one_way_mode" {
        return Some(("both", side(n.side)?));
    }
    fill_direction(n, uta)
}
