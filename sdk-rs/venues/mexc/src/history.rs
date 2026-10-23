//! Bounded paged reads, independent of account reconciliation and store writes.
use crate::{MexcFutures, Pending, REST, sign, wire::*};
use safe_cex_engine::history::{Completion, CursorHistory};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::paging::{self, CursorPage, CursorStep, CursorWalk, WindowOrder};
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, VenueError, num};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
const WINDOW: f64 = 90.0 * 86_400_000.0 - 1.0;
const MAX_PAGES: u32 = 1000;

pub struct Request {
    owner: Box<CursorHistory<Row>>,
    kind: Kind,
    symbol: Option<String>,
    order_id: Option<String>,
    windows: VecDeque<(Option<f64>, Option<f64>)>,
    from: Option<f64>,
    to: Option<f64>,
    page: u32,
    count: u32,
    limit: usize,
    size: u32,
    bad: bool,
}
#[derive(Clone, Copy)]
enum Kind {
    Fills,
    Orders,
    Funding,
    Trades,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Row {
    Execution(Execution),
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
    side: OrderSide,
    commission: f64,
    commission_asset: String,
    position_side: &'static str,
    realized_pnl: f64,
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
    price: f64,
    average_price: f64,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    position_side: &'static str,
    created_time: f64,
    updated_time: f64,
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
    side: OrderSide,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionRow<'a> {
    #[serde(borrow, deserialize_with = "id")]
    id: Str<'a>,
    #[serde(borrow, deserialize_with = "id")]
    order_id: Str<'a>,
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    side: f64,
    #[serde(deserialize_with = "number")]
    vol: f64,
    #[serde(deserialize_with = "number")]
    price: f64,
    #[serde(deserialize_with = "number")]
    fee: f64,
    #[serde(borrow)]
    fee_currency: Str<'a>,
    #[serde(deserialize_with = "number")]
    timestamp: f64,
    #[serde(deserialize_with = "number")]
    profit: f64,
    #[serde(alias = "taker")]
    is_taker: Option<bool>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FundingRow<'a> {
    #[serde(borrow, deserialize_with = "id")]
    id: Str<'a>,
    #[serde(borrow)]
    symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    funding: f64,
    #[serde(deserialize_with = "number")]
    settle_time: f64,
}
#[derive(Deserialize)]
struct TradeRow {
    #[serde(deserialize_with = "number")]
    p: f64,
    #[serde(deserialize_with = "number")]
    v: f64,
    #[serde(rename = "T", deserialize_with = "number")]
    side: f64,
    #[serde(deserialize_with = "number")]
    t: f64,
}
fn position_side(side: f64) -> &'static str {
    if side == 1.0 || side == 4.0 {
        "long"
    } else {
        "short"
    }
}
fn side(side: f64) -> OrderSide {
    if side == 1.0 || side == 2.0 {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}
fn compact(s: &str) -> String {
    s.replace('_', "")
}

impl MexcFutures {
    pub fn start_query(&self, cx: &mut Cx<'_, Pending>, command: CommandId, query: Query) {
        let (kind, symbol, order_id, from, to, limit) = match query {
            Query::OrderByClientId { .. } => {
                Self::fail_local_read(cx, "MEXC client-order-ID lookup is unsupported");
                return;
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (Kind::Fills, symbol, order_id, from_ms, to_ms, limit),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (Kind::Orders, symbol, None, from_ms, to_ms, limit),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                if kind
                    .as_deref()
                    .is_none_or(|k| !k.eq_ignore_ascii_case("FUNDING_FEE"))
                {
                    Self::fail_local_read(cx, "MEXC income supports only kind FUNDING_FEE");
                    return;
                }
                (Kind::Funding, symbol, None, from_ms, to_ms, limit)
            }
            Query::PublicTrades { symbol, limit } => {
                (Kind::Trades, Some(symbol), None, None, None, limit)
            }
        };
        let error = if !matches!(kind, Kind::Trades) && self.credentials.is_none() {
            Some("MEXC private queries require credentials")
        } else if matches!(kind, Kind::Fills) && symbol.is_none() && order_id.is_none() {
            Some("MEXC fills require a symbol or order ID")
        } else if symbol
            .as_ref()
            .is_some_and(|s| cx.store.market(s).is_none())
        {
            Some("Market not found")
        } else if order_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()))
        {
            Some("Invalid MEXC order ID")
        } else if from.is_some_and(|n| !n.is_finite() || n < 0.0)
            || to.is_some_and(|n| !n.is_finite() || n < 0.0)
            || from.zip(to).is_some_and(|(a, b)| a > b)
        {
            Some("Invalid MEXC query time range")
        } else if limit == Some(0) || limit.is_some_and(|l| l > 100_000) {
            Some("MEXC query limit must be 1..100000")
        } else {
            None
        };
        if let Some(error) = error {
            Self::fail_local_read(cx, error);
            return;
        }
        let limit = limit.unwrap_or(100) as usize;
        let (window_from, window_to) = match (from, to) {
            // Whole milliseconds keep newest-first window edges gap-free.
            (Some(a), None) => (Some(a), Some(cx.now.floor())),
            (None, Some(b)) => (Some((b - 7.0 * 86_400_000.0).max(0.0)), Some(b)),
            pair => pair,
        };
        if window_from.zip(window_to).is_some_and(|(a, b)| a > b) {
            Self::fail_local_read(cx, "Invalid MEXC query time range");
            return;
        }
        let mut windows = VecDeque::new();
        if matches!(kind, Kind::Funding | Kind::Trades) || order_id.is_some() {
            windows.push_back((None, None));
        } else if let (Some(a), Some(b)) = (window_from, window_to) {
            // Newest first: a limited read stops with the most recent rows.
            let Some(tiles) = paging::windows(a, b, WINDOW, WindowOrder::NewestFirst, 1000) else {
                Self::fail_local_read(cx, "MEXC query range exceeds 1000 windows");
                return;
            };
            windows.extend(tiles.into_iter().map(|(a, b)| (Some(a), Some(b))));
        } else {
            windows.push_back((window_from, window_to));
        }
        self.query_page(
            cx,
            Request {
                owner: Box::new(CursorHistory::new(
                    Completion::new(command),
                    CursorWalk::starting_at(MAX_PAGES, "1".into()),
                    |_| "MEXC history pagination limit exceeded".into(),
                    |rows| serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string())),
                )),
                kind,
                symbol,
                order_id,
                windows,
                from,
                to,
                page: 1,
                count: 0,
                limit,
                size: limit.min(100) as u32,
                bad: false,
            },
        );
    }
    fn query_page(&self, cx: &mut Cx<'_, Pending>, request: Request) {
        let r = &request;
        let mut params = vec![];
        let path = match r.kind {
            Kind::Trades => {
                params.push(("limit", r.size.to_string()));
                format!(
                    "/api/v1/contract/deals/{}",
                    cx.store.market(r.symbol.as_ref().unwrap()).unwrap().id
                )
            }
            Kind::Fills if r.order_id.is_some() => format!(
                "/api/v1/private/order/deal_details/{}",
                r.order_id.as_ref().unwrap()
            ),
            kind => {
                params.extend([
                    ("page_num", r.page.to_string()),
                    ("page_size", r.size.to_string()),
                ]);
                if let Some(symbol) = r.symbol.as_ref() {
                    params.push(("symbol", cx.store.market(symbol).unwrap().id.clone()));
                }
                if let Some((a, b)) = r.windows.front() {
                    if let Some(a) = a {
                        params.push(("start_time", num::format(*a)));
                    }
                    if let Some(b) = b {
                        params.push(("end_time", num::format(*b)));
                    }
                }
                if matches!(kind, Kind::Orders) {
                    params.push(("states", "3,4,5".into()));
                }
                match kind {
                    Kind::Fills => "/api/v1/private/order/list/order_deals",
                    Kind::Orders => "/api/v1/private/order/list/history_orders",
                    Kind::Funding => "/api/v1/private/position/funding_records",
                    _ => unreachable!(),
                }
                .into()
            }
        };
        let payload = sign::query(params);
        let url = format!("{REST}{path}?{payload}");
        let headers = if matches!(r.kind, Kind::Trades) {
            vec![]
        } else {
            self.credentials.as_ref().unwrap().headers_window(
                cx.now,
                &payload,
                self.options.recv_window,
            )
        };
        let at = if matches!(r.kind, Kind::Funding) {
            ArrayAt::Path(&["data", "resultList"])
        } else {
            ArrayAt::EnvelopeField("data")
        };
        cx.request_streamed(Method::Get, url, headers, at, Pending::History(request));
    }
    pub fn query_item(&self, cx: &Cx<'_, Pending>, r: &mut Request, item: &str) {
        let result = (|| -> Result<(String, f64, Row), String> {
            let bad = |e: serde_json::Error| e.to_string();
            Ok(match r.kind {
                Kind::Fills => {
                    let raw: ExecutionRow<'_> = serde_json::from_str(item).map_err(bad)?;
                    let symbol = compact(&raw.symbol);
                    let m = cx.store.market(&symbol).ok_or("Unknown history market")?;
                    if raw.id.is_empty()
                        || raw.order_id.is_empty()
                        || !(1.0..=4.0).contains(&raw.side)
                    {
                        return Err("Invalid execution".into());
                    }
                    (
                        raw.id.to_string(),
                        raw.timestamp,
                        Row::Execution(Execution {
                            id: raw.id.into_owned(),
                            timestamp: (raw.timestamp / 1000.0).floor(),
                            symbol,
                            order_id: raw.order_id.into_owned(),
                            price: raw.price,
                            qty: raw.vol * m.contract_size.unwrap_or(1.0),
                            side: side(raw.side),
                            commission: raw.fee,
                            commission_asset: raw.fee_currency.into_owned(),
                            position_side: position_side(raw.side),
                            realized_pnl: raw.profit,
                            maker: raw.is_taker.map(|t| !t),
                        }),
                    )
                }
                Kind::Orders => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Average {
                        #[serde(deserialize_with = "number")]
                        deal_avg_price: f64,
                    }
                    let raw: OrderRow<'_> = serde_json::from_str(item).map_err(bad)?;
                    let avg: Average = serde_json::from_str(item).map_err(bad)?;
                    let order = self
                        .map_order(cx, &raw, crate::account::Scope::Regular)
                        .ok_or("Invalid historical order")?;
                    if !(3.0..=5.0).contains(&raw.state) {
                        return Err("Nonterminal historical order".into());
                    }
                    (
                        order.id.clone(),
                        raw.create_time,
                        Row::Order(HistoricalOrder {
                            id: order.id,
                            symbol: order.symbol,
                            side: order.side,
                            kind: order.kind,
                            status: order.status,
                            price: raw.price,
                            average_price: avg.deal_avg_price,
                            amount: order.amount,
                            filled: order.filled,
                            reduce_only: raw.reduce_only || raw.side == 2.0 || raw.side == 4.0,
                            position_side: position_side(raw.side),
                            created_time: raw.create_time,
                            updated_time: raw.update_time,
                        }),
                    )
                }
                Kind::Funding => {
                    let raw: FundingRow<'_> = serde_json::from_str(item).map_err(bad)?;
                    let symbol = compact(&raw.symbol);
                    let m = cx.store.market(&symbol).ok_or("Unknown funding market")?;
                    if raw.id.is_empty() {
                        return Err("Missing funding ID".into());
                    }
                    (
                        raw.id.to_string(),
                        raw.settle_time,
                        Row::Income(Income {
                            symbol,
                            income_type: "FUNDING_FEE",
                            income: raw.funding,
                            asset: m.quote.clone(),
                            timestamp: raw.settle_time,
                            tran_id: raw.id.into_owned(),
                            category: "funding",
                        }),
                    )
                }
                Kind::Trades => {
                    let raw: TradeRow = serde_json::from_str(item).map_err(bad)?;
                    let symbol = r.symbol.as_ref().unwrap();
                    let m = cx.store.market(symbol).unwrap();
                    let qty = raw.v * m.contract_size.unwrap_or(1.0);
                    // This feed exposes no execution ID. An ordinal preserves equal prints.
                    let id = format!("{}:{}:{}:{}:{}", raw.t, raw.p, raw.v, raw.side, r.count);
                    (
                        id.clone(),
                        raw.t,
                        Row::Trade(Trade {
                            id,
                            symbol: symbol.clone(),
                            price: raw.p,
                            qty,
                            quote_qty: qty * raw.p,
                            timestamp: raw.t,
                            side: if raw.side == 1.0 {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            },
                        }),
                    )
                }
            })
        })();
        r.count += 1;
        match result {
            Ok((id, time, row)) => {
                let symbol = match &row {
                    Row::Execution(e) => &e.symbol,
                    Row::Order(o) => &o.symbol,
                    Row::Income(i) => &i.symbol,
                    Row::Trade(t) => &t.symbol,
                };
                if r.from.is_none_or(|a| time >= a)
                    && r.to.is_none_or(|b| time <= b)
                    && r.symbol.as_ref().is_none_or(|s| s == symbol)
                    && r.owner.fresh(id)
                    && r.owner.accepted_len() + r.owner.staged_len() < r.limit
                {
                    r.owner.stage(row);
                }
            }
            Err(_) => r.bad = true,
        }
    }

    pub fn query_response(&self, cx: &mut Cx<'_, Pending>, mut r: Request, response: Response<'_>) {
        let envelope = match Envelope::checked(response.body) {
            Ok(e) if response.status == 200 => e,
            Ok(_) => {
                self.query_failed(cx, r, "MEXC history HTTP failure");
                return;
            }
            Err(e) => {
                self.query_failed(cx, r, &e);
                return;
            }
        };
        if r.bad {
            self.query_failed(cx, r, "Malformed MEXC history row");
            return;
        }
        let has_more = if matches!(r.kind, Kind::Funding) {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Page {
                total_page: u32,
                current_page: u32,
            }
            match envelope
                .payload()
                .and_then(|p| serde_json::from_str::<Page>(p).map_err(|e| e.to_string()))
            {
                Ok(p) if p.current_page == r.page => r.page < p.total_page,
                _ => {
                    self.query_failed(cx, r, "Malformed MEXC funding page");
                    return;
                }
            }
        } else {
            r.count >= r.size
        };
        let read_more = r.owner.accepted_len() + r.owner.staged_len() < r.limit
            && !matches!(r.kind, Kind::Trades)
            && r.order_id.is_none();
        let next = (read_more && has_more).then(|| (r.page + 1).to_string());
        let fresh = r.owner.staged_len();
        match r.owner.advance(CursorPage {
            want_more: next.is_some(),
            next,
            rows: r.count as usize,
            fresh,
        }) {
            CursorStep::More(_) => {
                r.page += 1;
                r.count = 0;
                self.query_page(cx, r);
                return;
            }
            CursorStep::Stalled(_) => {
                r.owner.reject(
                    cx,
                    VenueError::new("MEXC history pagination limit exceeded"),
                );
                return;
            }
            CursorStep::Done => {}
        }
        if read_more && !has_more {
            r.windows.pop_front();
            if !r.windows.is_empty() {
                // Native windows each have their own 1000-page budget. Keep the
                // logical owner, admitted output and output identities intact.
                if !r.owner.restart_stage() {
                    r.owner
                        .reject(cx, VenueError::new("MEXC history window lifecycle failed"));
                    return;
                }
                r.page = 1;
                r.count = 0;
                self.query_page(cx, r);
                return;
            }
        }
        r.owner.finish(cx, Ok(()));
    }
    pub fn query_failed(&self, cx: &mut Cx<'_, Pending>, r: Request, error: &str) {
        r.owner.finish(cx, Err(VenueError::new(error)));
    }
}
