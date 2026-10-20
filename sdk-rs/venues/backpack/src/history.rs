//! Rare reads keep typed output rows in their request continuation. History
//! without server-side time filters walks bounded offset pages, newest first.
use crate::account::{kind, side, status};
use crate::sign::Params;
use crate::wire::*;
use crate::{Backpack, Pending};
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::{CommandId, Cx, Method, Query, Response};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Fills,
    Orders,
    Funding,
    Trades,
}
pub struct Request {
    pub command: CommandId,
    kind: Kind,
    symbol: Option<String>,
    order_id: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    page_size: usize,
    offset: usize,
    page_count: usize,
    oldest: f64,
    rows: Vec<Row>,
    seen: HashSet<String>,
    page_fresh: usize,
    page_hash: u64,
    pages: HashSet<u64>,
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
    created_time: f64,
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
fn side_name(text: &str) -> &'static str {
    if text == "Bid" { "buy" } else { "sell" }
}
impl Request {
    fn accepts(&self, symbol: &str, time: f64) -> bool {
        self.symbol.as_ref().is_none_or(|s| s == symbol)
            && self.from.is_none_or(|t| time >= t)
            && self.to.is_none_or(|t| {
                if self.kind == Kind::Fills {
                    time < t
                } else {
                    time <= t
                }
            })
    }
    fn append(&mut self, id: String, time: f64, row: Row) {
        self.oldest = self.oldest.min(time);
        if self.seen.insert(id) {
            self.page_fresh += 1;
            if self.rows.len() < self.limit {
                self.rows.push(row);
            }
        }
    }
}
impl Backpack {
    pub(crate) fn history_command(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let (kind, symbol, order_id, from, to, limit) = match query {
            Query::OrderByClientId { .. } => {
                cx.done(
                    command,
                    Err("Backpack client-order-ID lookup is unsupported".into()),
                );
                return;
            }
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
                from_ms,
                to_ms,
                limit.unwrap_or(100),
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
                limit.unwrap_or(100),
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                if kind.as_ref().is_some_and(|k| {
                    !matches!(k.to_ascii_uppercase().as_str(), "FUNDING_FEE" | "FUNDING")
                }) {
                    cx.done(
                        command,
                        Err("Backpack income query supports funding payments only".into()),
                    );
                    return;
                }
                (
                    Kind::Funding,
                    symbol,
                    None,
                    from_ms,
                    to_ms,
                    limit.unwrap_or(1000),
                )
            }
            Query::PublicTrades { symbol, limit } => (
                Kind::Trades,
                Some(symbol),
                None,
                None,
                None,
                limit.unwrap_or(1000),
            ),
        };
        if from
            .into_iter()
            .chain(to)
            .any(|t| !t.is_finite() || t < 0.0)
            || from.zip(to).is_some_and(|(a, b)| a > b)
        {
            cx.done(command, Err("Backpack invalid history time range".into()));
            return;
        }
        if symbol
            .as_ref()
            .is_some_and(|s| cx.store.market(s).is_none())
        {
            cx.done(command, Err("Backpack history market not found".into()));
            return;
        }
        let limit = limit.clamp(1, 1000) as usize;
        let page_size =
            if matches!(kind, Kind::Orders | Kind::Funding) && (from.is_some() || to.is_some()) {
                1000
            } else {
                limit
            };
        self.history_request(
            cx,
            Request {
                command,
                kind,
                symbol,
                order_id,
                from,
                to,
                limit,
                page_size,
                offset: 0,
                page_count: 0,
                oldest: f64::INFINITY,
                rows: vec![],
                seen: HashSet::new(),
                page_fresh: 0,
                page_hash: 14695981039346656037,
                pages: HashSet::new(),
                invalid: false,
            },
        );
    }
    fn history_request(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let mut params: Params = [("limit".into(), request.page_size.to_string())].into();
        if let Some(s) = &request.symbol {
            params.insert("symbol".into(), s.clone());
        }
        if let Some(id) = &request.order_id {
            params.insert("orderId".into(), id.clone());
        }
        let path = match request.kind {
            Kind::Fills => {
                if let Some(t) = request.from {
                    params.insert("from".into(), format!("{:.0}", t.floor()));
                }
                if let Some(t) = request.to {
                    params.insert("to".into(), format!("{:.0}", t.ceil()));
                }
                "/wapi/v1/history/fills"
            }
            Kind::Orders => "/wapi/v1/history/orders",
            Kind::Funding => "/wapi/v1/history/funding",
            Kind::Trades => "/api/v1/trades",
        };
        if request.kind == Kind::Trades {
            self.get(cx, path, params, Pending::History(request), true);
        } else {
            params.insert("sortDirection".into(), "Desc".into());
            if request.offset > 0 {
                params.insert("offset".into(), request.offset.to_string());
            }
            let command = request.command;
            if let Err(e) = self.signed(
                cx,
                Method::Get,
                path,
                params,
                None,
                Pending::History(request),
                true,
            ) {
                cx.done(command, Err(e));
            }
        }
    }
    pub(crate) fn history_item(&self, cx: &mut Cx<'_, Pending>, r: &mut Request, item: &str) {
        r.page_count += 1;
        for byte in item.bytes() {
            r.page_hash = (r.page_hash ^ u64::from(byte)).wrapping_mul(1099511628211);
        }
        let result = (|| {
            match r.kind {
                Kind::Fills => {
                    let d: FillRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let time = timestamp(&d.timestamp)?;
                    r.oldest = r.oldest.min(time);
                    if !r.accepts(&d.symbol, time)
                        || r.order_id.as_ref().is_some_and(|id| id != &d.order_id)
                        || cx.store.market(&d.symbol).is_none()
                    {
                        return Ok(());
                    }
                    if !matches!(d.side.as_ref(), "Bid" | "Ask")
                        || d.price <= 0.0
                        || d.quantity <= 0.0
                    {
                        return Err("Backpack invalid fill".into());
                    }
                    let key = format!("{}:{}", d.order_id, d.trade_id);
                    r.append(
                        key,
                        time,
                        Row::Fill(Execution {
                            id: d.trade_id.to_string(),
                            timestamp: (time / 1000.0).floor(),
                            symbol: d.symbol.into_owned(),
                            order_id: d.order_id.into_owned(),
                            price: d.price,
                            qty: d.quantity,
                            side: side_name(&d.side),
                            commission: d.fee.abs(),
                            commission_asset: d.fee_symbol.into_owned(),
                            maker: d.is_maker,
                        }),
                    );
                }
                Kind::Orders => {
                    let d: OrderRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    if !matches!(d.side.as_ref(), "Bid" | "Ask") {
                        return Err("Backpack invalid order side".into());
                    }
                    let time = d.created_at;
                    r.oldest = r.oldest.min(time);
                    if !r.accepts(&d.symbol, time) || cx.store.market(&d.symbol).is_none() {
                        return Ok(());
                    }
                    let order_kind = kind(
                        cx.store,
                        &d.symbol,
                        side(&d.side),
                        &d.order_type,
                        d.trigger_price.as_ref().map(|n| n.0),
                    );
                    let price = d.trigger_price.map_or(d.price, |n| n.0);
                    let amount = d.trigger_quantity.map_or(d.quantity, |n| n.0);
                    let average = d
                        .executed_quote_quantity
                        .map(|q| q.0)
                        .filter(|_| d.executed_quantity > 0.0)
                        .map(|q| q / d.executed_quantity);
                    let id = d.id.into_owned();
                    r.append(
                        id.clone(),
                        time,
                        Row::Order(HistoricalOrder {
                            id,
                            symbol: d.symbol.into_owned(),
                            side: side_name(&d.side),
                            kind: order_kind,
                            status: status(&d.status),
                            price: (price > 0.0).then_some(price),
                            average_price: average,
                            amount,
                            filled: d.executed_quantity,
                            reduce_only: d.reduce_only.unwrap_or(false),
                            created_time: time,
                        }),
                    );
                }
                Kind::Funding => {
                    let d: FundingRow<'_> =
                        serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let time = timestamp(&d.interval_end_timestamp)?;
                    r.oldest = r.oldest.min(time);
                    if !r.accepts(&d.symbol, time) {
                        return Ok(());
                    }
                    let Some(market) = cx.store.market(&d.symbol) else {
                        return Ok(());
                    };
                    let key = format!("{}:{time:.0}", d.symbol);
                    let symbol = d.symbol.into_owned();
                    r.append(
                        key.clone(),
                        time,
                        Row::Income(Income {
                            symbol,
                            income_type: "FUNDING_FEE",
                            income: d.quantity,
                            asset: market.quote.clone(),
                            timestamp: time,
                            tran_id: key,
                            category: "funding",
                        }),
                    );
                }
                Kind::Trades => {
                    let d: PublicTrade = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    r.append(
                        d.id.to_string(),
                        d.timestamp,
                        Row::Trade(Trade {
                            id: d.id.to_string(),
                            symbol: r.symbol.clone().unwrap(),
                            price: d.price,
                            qty: d.quantity,
                            quote_qty: d.quote_quantity,
                            timestamp: d.timestamp,
                            side: if d.is_buyer_maker { "sell" } else { "buy" },
                        }),
                    );
                }
            }
            Ok::<(), String>(())
        })();
        if let Err(e) = result {
            r.invalid = true;
            cx.log(safe_cex_engine::LogLevel::Error, e);
        }
    }
    pub(crate) fn history_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) || r.invalid {
            cx.done(
                r.command,
                Err(format!(
                    "Backpack history read failed: HTTP {}",
                    response.status
                )),
            );
            return;
        }
        let paged = r.kind != Kind::Trades;
        if paged
            && r.page_count >= r.page_size
            && r.rows.len() < r.limit
            && !r.from.is_some_and(|from| r.oldest < from)
        {
            if r.pages.len() >= 49 || !r.pages.insert(r.page_hash) {
                cx.done(
                    r.command,
                    Err("Backpack history is incomplete: paging made no progress".into()),
                );
                return;
            }
            r.offset += r.page_count;
            r.page_count = 0;
            r.page_fresh = 0;
            r.page_hash = 14695981039346656037;
            r.oldest = f64::INFINITY;
            self.history_request(cx, r);
        } else {
            cx.done(r.command, Ok(serde_json::to_string(&r.rows).unwrap()));
        }
    }
}
