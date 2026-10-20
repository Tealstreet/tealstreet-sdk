//! Rare cursor-paged reads never mutate the account store.
use crate::{Pending, account::Account};
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::url::encode;
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, json::ArrayAt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Default)]
pub struct History;
pub struct Request {
    command: CommandId,
    query: Query,
    cursor: Option<String>,
    cursors: HashSet<String>,
    seen: HashSet<String>,
    page: u32,
    valid: bool,
    raw_rows: usize,
    fallback: bool,
    lookup_history: bool,
    lookup_complete: bool,
    open_complete: bool,
    rows: Rows,
}
enum Rows {
    Fills(Vec<Execution>),
    Orders(Vec<HistoricalOrder>),
    Income(Vec<Income>),
    Trades(Vec<PublicTrade>),
    Lookup(Vec<safe_cex_engine::model::Order>),
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct Row {
    id: String,
    market: String,
    #[serde(alias = "orderId")]
    order_id: String,
    side: String,
    #[serde(rename = "type")]
    kind: String,
    status: String,
    price: String,
    size: String,
    remaining_size: String,
    avg_fill_price: String,
    fee: String,
    fee_currency: String,
    realized_pnl: String,
    realized_funding: String,
    liquidity: String,
    cancel_reason: String,
    flags: Vec<String>,
    #[serde(deserialize_with = "scalar")]
    created_at: String,
    #[serde(deserialize_with = "scalar")]
    last_updated_at: String,
    client_id: String,
}
fn scalar<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Scalar {
        Text(String),
        Number(u64),
    }
    Scalar::deserialize(d).map(|s| match s {
        Scalar::Text(s) => s,
        Scalar::Number(n) => n.to_string(),
    })
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
    side: OrderSide,
    commission: f64,
    commission_asset: String,
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
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicTrade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    side: OrderSide,
    timestamp: f64,
}
fn finite(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}
fn valid_row(row: &Row, query: &Query, fallback: bool) -> bool {
    if row.id.is_empty()
        || row.market.is_empty()
        || !finite(&row.created_at).is_some_and(|v| v > 0.)
    {
        return false;
    }
    let optional = [
        &row.fee,
        &row.realized_pnl,
        &row.realized_funding,
        &row.avg_fill_price,
        &row.last_updated_at,
    ];
    if optional
        .into_iter()
        .any(|s| !s.is_empty() && finite(s).is_none())
    {
        return false;
    }
    match query {
        Query::Income { .. } => true,
        Query::OrderHistory { .. } => {
            side(&row.side).is_some()
                && finite(&row.size).is_some_and(|n| n > 0.)
                && finite(&row.remaining_size).is_some_and(|n| n >= 0. && n <= number(&row.size))
                && (row.price.is_empty() || finite(&row.price).is_some_and(|n| n >= 0.))
        }
        Query::Fills { .. } | Query::PublicTrades { .. } => {
            side(&row.side).is_some()
                && finite(&row.size).is_some_and(|n| n > 0.)
                && (finite(&row.price).is_some_and(|n| n > 0.)
                    || (fallback && finite(&row.avg_fill_price).is_some_and(|n| n > 0.)))
                && (!fallback
                    || finite(&row.remaining_size)
                        .is_some_and(|n| n >= 0. && n <= number(&row.size)))
        }
        Query::OrderByClientId { .. } => true,
    }
}
fn number(s: &str) -> f64 {
    safe_cex_engine::num::parse(s)
}
fn optional_number(s: &str) -> Option<f64> {
    s.parse().ok().filter(|n: &f64| n.is_finite() && *n != 0.)
}
fn side(s: &str) -> Option<OrderSide> {
    match s {
        "BUY" => Some(OrderSide::Buy),
        "SELL" => Some(OrderSide::Sell),
        _ => None,
    }
}
fn map_order(row: &Row) -> Option<HistoricalOrder> {
    let created_time = number(&row.created_at);
    if row.id.is_empty() || row.market.is_empty() || created_time <= 0. {
        return None;
    }
    let reduce_only = row.flags.iter().any(|f| f == "REDUCE_ONLY");
    let kind = match row.kind.as_str() {
        "MARKET" => OrderType::Market,
        "STOP_LIMIT" | "STOP_MARKET" if !reduce_only => OrderType::Conditional,
        "STOP_LIMIT" | "STOP_MARKET" | "STOP_LOSS_LIMIT" | "STOP_LOSS_MARKET" => {
            OrderType::StopLoss
        }
        "TAKE_PROFIT_LIMIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        _ => OrderType::Limit,
    };
    Some(HistoricalOrder {
        id: row.id.clone(),
        symbol: row.market.clone(),
        side: side(&row.side)?,
        kind,
        status: if matches!(row.status.as_str(), "NEW" | "OPEN" | "UNTRIGGERED") {
            OrderStatus::Open
        } else if !row.cancel_reason.is_empty() {
            OrderStatus::Canceled
        } else {
            OrderStatus::Closed
        },
        price: optional_number(&row.price),
        average_price: optional_number(&row.avg_fill_price),
        amount: number(&row.size),
        filled: (number(&row.size) - number(&row.remaining_size)).max(0.),
        reduce_only,
        created_time,
        updated_time: optional_number(&row.last_updated_at),
    })
}
impl History {
    pub fn query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        command: CommandId,
        query: Query,
        public_only: bool,
    ) {
        if public_only && !matches!(query, Query::PublicTrades { .. }) {
            cx.done(
                command,
                Ok(if matches!(query, Query::OrderByClientId { .. }) {
                    "{\"found\":false,\"definitive\":false}".into()
                } else {
                    "[]".into()
                }),
            );
            return;
        }
        if let Query::OrderByClientId {
            client_order_id, ..
        } = &query
            && (client_order_id.is_empty()
                || client_order_id.chars().count() > 64
                || client_order_id.contains(['\r', '\n']))
        {
            cx.done(command, Err("invalid Paradex client order ID".into()));
            return;
        }
        let bounds = match &query {
            Query::Fills { from_ms, to_ms, .. }
            | Query::OrderHistory { from_ms, to_ms, .. }
            | Query::Income { from_ms, to_ms, .. } => (*from_ms, *to_ms),
            _ => (None, None),
        };
        if bounds
            .0
            .into_iter()
            .chain(bounds.1)
            .any(|v| !v.is_finite() || v < 0.)
            || matches!(bounds, (Some(a),Some(b)) if a>b)
        {
            cx.done(command, Err("invalid Paradex history time range".into()));
            return;
        }
        if matches!(query, Query::Fills { symbol: None, .. }) {
            cx.done(command, Ok("[]".into()));
            return;
        }
        let rows = match query {
            Query::Fills { .. } => Rows::Fills(Vec::new()),
            Query::OrderHistory { .. } => Rows::Orders(Vec::new()),
            Query::Income { .. } => Rows::Income(Vec::new()),
            Query::PublicTrades { .. } => Rows::Trades(Vec::new()),
            Query::OrderByClientId { .. } => Rows::Lookup(Vec::new()),
        };
        let request = Request {
            command,
            query,
            cursor: None,
            cursors: HashSet::new(),
            seen: HashSet::new(),
            page: 0,
            valid: true,
            raw_rows: 0,
            fallback: false,
            lookup_history: false,
            lookup_complete: false,
            open_complete: false,
            rows,
        };
        self.issue(cx, account, request);
    }
    fn issue(&self, cx: &mut Cx<'_, Pending>, account: &mut Account, request: Request) {
        let mut params = Vec::new();
        let (path, symbol, from, to, page_size, public) = match &request.query {
            Query::Fills {
                symbol,
                to_ms,
                limit,
                ..
            } => (
                if request.fallback {
                    "/orders-history"
                } else {
                    "/fills"
                },
                symbol.as_deref(),
                None,
                *to_ms,
                limit.unwrap_or(100).clamp(1, 500),
                false,
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                ..
            } => (
                "/orders-history",
                symbol.as_deref(),
                *from_ms,
                *to_ms,
                100,
                false,
            ),
            Query::Income {
                symbol,
                from_ms,
                to_ms,
                ..
            } => {
                let end = to_ms.unwrap_or(cx.now);
                (
                    "/fills",
                    symbol.as_deref(),
                    Some(from_ms.unwrap_or(end - 30. * 86400000.)),
                    Some(end),
                    500,
                    false,
                )
            }
            Query::PublicTrades { symbol, limit } => (
                "/trades",
                Some(symbol.as_str()),
                None,
                None,
                limit.unwrap_or(100).clamp(1, 500),
                true,
            ),
            Query::OrderByClientId { symbol, .. } => (
                if request.lookup_history {
                    "/orders-history"
                } else {
                    "/orders"
                },
                Some(symbol.as_str()),
                None,
                None,
                100,
                false,
            ),
        };
        if from.into_iter().chain(to).any(|v| !v.is_finite())
            || matches!((from, to), (Some(a),Some(b)) if a>b)
        {
            cx.done(
                request.command,
                Err("invalid Paradex history time range".into()),
            );
            return;
        }
        if !public && account.credentials().is_none() {
            cx.done(
                request.command,
                Err("Paradex private auth is not initialized".into()),
            );
            return;
        }
        if let Some(symbol) = symbol {
            let Some(market) = cx.store.market(symbol) else {
                cx.done(
                    request.command,
                    Ok(if matches!(request.query, Query::OrderByClientId { .. }) {
                        "{\"found\":false,\"definitive\":false}".into()
                    } else {
                        "[]".into()
                    }),
                );
                return;
            };
            params.push(format!("market={}", encode(&market.id)));
        }
        params.push(format!(
            "{}={page_size}",
            if request.fallback {
                "limit"
            } else {
                "page_size"
            }
        ));
        if let Some(from) = from {
            params.push(format!("start_at={}", safe_cex_engine::num::format(from)));
        }
        if let Some(to) = to {
            params.push(format!("end_at={}", safe_cex_engine::num::format(to)));
        }
        if let Some(cursor) = &request.cursor {
            params.push(format!("cursor={}", encode(cursor)));
        }
        let path = format!("{path}?{}", params.join("&"));
        if public {
            cx.request_streamed(
                Method::Get,
                format!("{}{path}", account.rest()),
                Vec::new(),
                ArrayAt::Field("results"),
                Pending::History(request),
            );
        } else {
            account.request(
                cx,
                Method::Get,
                path,
                None,
                Some(ArrayAt::Field("results")),
                Pending::History(request),
            );
        }
    }
    pub fn on_item(&mut self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        request.raw_rows += 1;
        if let Rows::Lookup(rows) = &mut request.rows {
            let Ok(row) = serde_json::from_str::<crate::account::NativeOrder>(item) else {
                request.valid = false;
                return;
            };
            let Query::OrderByClientId {
                symbol,
                client_order_id,
            } = &request.query
            else {
                return;
            };
            if row.market == *symbol
                && row.client_id == *client_order_id
                && request.seen.insert(row.id.clone())
            {
                if let Ok(order) = crate::account::map_order(&row) {
                    rows.push(order);
                } else {
                    request.valid = false;
                }
            }
            return;
        }
        let Ok(mut row) = serde_json::from_str::<Row>(item) else {
            request.valid = false;
            return;
        };
        if !valid_row(&row, &request.query, request.fallback) {
            request.valid = false;
            return;
        }
        let wanted_symbol = match &request.query {
            Query::Fills { symbol, .. }
            | Query::OrderHistory { symbol, .. }
            | Query::Income { symbol, .. } => symbol.as_deref(),
            Query::PublicTrades { symbol, .. } | Query::OrderByClientId { symbol, .. } => {
                Some(symbol.as_str())
            }
        };
        if wanted_symbol.is_some_and(|symbol| symbol != row.market) {
            return;
        }
        if request.fallback && row.order_id.is_empty() {
            row.order_id = row.id.clone();
        }
        if !row.id.is_empty() && !request.seen.insert(row.id.clone()) {
            return;
        }
        match &mut request.rows {
            Rows::Fills(rows) => {
                let Query::Fills {
                    symbol,
                    order_id,
                    from_ms,
                    to_ms,
                    ..
                } = &request.query
                else {
                    return;
                };
                if symbol.as_ref().is_some_and(|s| *s != row.market)
                    || order_id.as_ref().is_some_and(|s| *s != row.order_id)
                {
                    return;
                }
                let timestamp = number(&row.created_at);
                let price = if number(&row.price) > 0. {
                    number(&row.price)
                } else {
                    number(&row.avg_fill_price)
                };
                let qty = if request.fallback {
                    (number(&row.size) - number(&row.remaining_size)).max(0.)
                } else {
                    number(&row.size)
                };
                if timestamp <= 0.
                    || price <= 0.
                    || qty <= 0.
                    || from_ms.is_some_and(|t| timestamp < t)
                    || to_ms.is_some_and(|t| timestamp > t)
                {
                    return;
                }
                let Some(side) = side(&row.side) else {
                    return;
                };
                rows.push(Execution {
                    id: row.id,
                    timestamp: (timestamp / 1000.).floor(),
                    symbol: row.market,
                    order_id: row.order_id,
                    price,
                    qty,
                    side,
                    commission: number(&row.fee).abs(),
                    commission_asset: if row.fee_currency.is_empty() {
                        cx.store
                            .market(symbol.as_deref().unwrap_or(""))
                            .map_or("USD".into(), |m| m.quote.clone())
                    } else {
                        row.fee_currency
                    },
                    realized_pnl: row
                        .realized_pnl
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite()),
                    maker: (!row.liquidity.is_empty()).then(|| row.liquidity == "MAKER"),
                });
            }
            Rows::Orders(rows) => {
                if let Some(order) = map_order(&row) {
                    rows.push(order);
                }
            }
            Rows::Lookup(_) => unreachable!(),
            Rows::Income(rows) => {
                let timestamp = number(&row.created_at);
                if timestamp <= 0. || row.market.is_empty() {
                    return;
                }
                let asset = if row.fee_currency.is_empty() {
                    "USDC"
                } else {
                    &row.fee_currency
                };
                for (value, income_type, suffix, category) in [
                    (
                        number(&row.realized_pnl),
                        "REALIZED_PNL",
                        "pnl",
                        "realized_pnl",
                    ),
                    (
                        -number(&row.fee),
                        if number(&row.fee) > 0. {
                            "COMMISSION"
                        } else {
                            "REBATE"
                        },
                        "fee",
                        if number(&row.fee) > 0. {
                            "commission"
                        } else {
                            "rebate"
                        },
                    ),
                    (
                        number(&row.realized_funding),
                        "FUNDING_FEE",
                        "funding",
                        "funding",
                    ),
                ] {
                    let wanted = match &request.query {
                        Query::Income { kind, .. } => kind.as_deref(),
                        _ => None,
                    };
                    if value != 0. && wanted.is_none_or(|kind| kind == income_type) {
                        rows.push(Income {
                            symbol: row.market.clone(),
                            income_type,
                            income: value,
                            asset: asset.into(),
                            timestamp,
                            tran_id: format!("{}:{suffix}", row.id),
                            trade_id: row.id.clone(),
                            category,
                        });
                    }
                }
            }
            Rows::Trades(rows) => {
                if let Some(side) = side(&row.side) {
                    rows.push(PublicTrade {
                        id: row.id,
                        symbol: row.market,
                        price: number(&row.price),
                        qty: number(&row.size),
                        quote_qty: number(&row.size) * number(&row.price),
                        side,
                        timestamp: number(&row.created_at),
                    });
                }
            }
        }
    }
    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        mut request: Request,
        response: Response<'_>,
    ) {
        if let Err(error) = crate::success(response) {
            self.on_failed(cx, request, &error);
            return;
        }
        if !request.valid {
            self.on_failed(cx, request, "malformed Paradex history row");
            return;
        }
        #[derive(Deserialize)]
        struct Envelope {
            next: Option<String>,
        }
        let Ok(envelope) = serde_json::from_str::<Envelope>(response.body) else {
            self.on_failed(cx, request, "invalid Paradex history envelope");
            return;
        };
        request.page += 1;
        let next = envelope.next.filter(|s| !s.is_empty());
        if matches!(request.query, Query::OrderByClientId { .. }) && !request.lookup_history {
            if matches!(&request.rows, Rows::Lookup(rows) if !rows.is_empty()) {
                self.finish(cx, request);
                return;
            }
            if let Some(next) = &next
                && request.page < 50
            {
                if !request.cursors.insert(next.clone()) {
                    self.on_failed(cx, request, "Paradex repeated open-order cursor");
                    return;
                }
                request.cursor = Some(next.clone());
                self.issue(cx, account, request);
                return;
            }
            request.open_complete = next.is_none();
            request.lookup_history = true;
            request.page = 0;
            request.cursor = None;
            request.cursors.clear();
            self.issue(cx, account, request);
            return;
        }
        if matches!(request.query, Query::Fills { .. })
            && request.raw_rows == 0
            && !request.fallback
            && (next.is_none() || request.page >= 5)
        {
            request.fallback = true;
            request.cursor = None;
            request.page = 0;
            self.issue(cx, account, request);
            return;
        }
        let filled_limit = match request.query {
            Query::Fills { limit, .. } => {
                request.raw_rows >= limit.unwrap_or(100).clamp(1, 500) as usize
            }
            _ => false,
        };
        let cap = match request.query {
            Query::Fills { .. } => 5,
            Query::Income { .. } => 200,
            Query::PublicTrades { .. } => 1,
            _ => 50,
        };
        request.lookup_complete = request.open_complete && request.lookup_history && next.is_none();
        if !filled_limit
            && !request.fallback
            && let Some(next) = next
        {
            if !request.cursors.insert(next.clone()) {
                self.on_failed(cx, request, "Paradex repeated history cursor");
                return;
            }
            if request.page < cap {
                request.cursor = Some(next);
                self.issue(cx, account, request);
                return;
            }
            if cap > 1 {
                cx.log(
                    safe_cex_engine::LogLevel::Warning,
                    "Paradex history reached page cap",
                );
            }
        }
        self.finish(cx, request);
    }
    fn finish(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let limit = match request.query {
            Query::Fills { limit, .. } => Some(limit.unwrap_or(100).clamp(1, 500) as usize),
            Query::OrderHistory { limit, .. } | Query::Income { limit, .. } => {
                limit.filter(|v| *v > 0).map(|v| v as usize)
            }
            Query::PublicTrades { limit, .. } => Some(limit.unwrap_or(100).clamp(1, 500) as usize),
            _ => None,
        };
        let result = match request.rows {
            Rows::Fills(mut rows) => {
                rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
                if let Some(n) = limit {
                    rows.truncate(n);
                }
                serde_json::to_string(&rows)
            }
            Rows::Orders(mut rows) => {
                rows.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
                if let Some(n) = limit {
                    rows.truncate(n);
                }
                serde_json::to_string(&rows)
            }
            Rows::Income(mut rows) => {
                rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
                if let Some(n) = limit {
                    rows.truncate(n);
                }
                serde_json::to_string(&rows)
            }
            Rows::Trades(mut rows) => {
                if let Some(n) = limit {
                    rows.truncate(n);
                }
                serde_json::to_string(&rows)
            }
            Rows::Lookup(rows) => {
                #[derive(Serialize)]
                struct Lookup<'a> {
                    found: bool,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    definitive: Option<bool>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    order: Option<&'a safe_cex_engine::model::Order>,
                }
                serde_json::to_string(&Lookup {
                    found: !rows.is_empty(),
                    definitive: rows.is_empty().then_some(request.lookup_complete),
                    order: rows.first(),
                })
            }
        };
        cx.done(request.command, result.map_err(|e| e.to_string()));
    }
    pub fn on_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        cx.done(request.command, Err(error.into()));
    }
}
