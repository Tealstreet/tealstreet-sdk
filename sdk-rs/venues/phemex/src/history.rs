//! Bounded rare reads; documented g-futures history uses textual enums, unlike
//! the numeric execution endpoint whose side enum remains unverified.
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::history::{Completion, HistoryRows};
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, TimerId, VenueError, json::ArrayAt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_ROWS: u32 = 200;

#[derive(Default)]
pub struct State {
    active: HashSet<CommandId>,
    deferred: Vec<(TimerId, Request)>,
}

pub struct Request {
    command: CommandId,
    symbol: String,
    native_symbol: String,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    order_id: Option<String>,
    client_id: Option<String>,
    rows: Rows,
    seen: HashSet<String>,
    count: usize,
    invalid: bool,
}

impl Request {
    pub(crate) fn is_lookup(&self) -> bool {
        self.client_id.is_some()
    }
}

enum Rows {
    Fills(Box<HistoryRows<Execution>>),
    Orders(Box<HistoryRows<HistoricalOrder>>),
    Trades(Box<HistoryRows<Trade>>),
    Lookup(Vec<safe_cex_engine::model::Order>),
}

fn encode_rows<T: Serialize>(rows: &[T]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}

fn owned_rows<T: Serialize>(command: CommandId) -> Box<HistoryRows<T>> {
    Box::new(HistoryRows::new(Completion::new(command), encode_rows))
}

fn finish_rows<T: Serialize, P>(
    mut owner: HistoryRows<T>,
    cx: &mut Cx<'_, P>,
    limit: usize,
    time: impl Fn(&T) -> f64,
) {
    // These endpoints have exactly one response. No rows are admitted before
    // its complete HTTP/envelope/row verdict, so a failure can never be partial.
    owner.accept_page();
    owner.finish_with(cx, Ok(()), |mut rows| {
        rows.sort_by(|a, b| time(b).total_cmp(&time(a)));
        rows.truncate(limit);
        encode_rows(&rows)
    });
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Number<'a> {
    Text(&'a str),
    Value(f64),
}
impl Number<'_> {
    fn value(&self) -> Option<f64> {
        let value = match self {
            Self::Text(text) => text.parse().ok()?,
            Self::Value(value) => *value,
        };
        value.is_finite().then_some(value)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillRow<'a> {
    #[serde(rename = "execID", alias = "execId")]
    id: &'a str,
    #[serde(rename = "orderID", alias = "orderId")]
    order_id: &'a str,
    symbol: &'a str,
    side: &'a str,
    trade_type: &'a str,
    transact_time_ns: u64,
    #[serde(borrow)]
    exec_price_rp: Number<'a>,
    #[serde(borrow)]
    exec_qty_rq: Number<'a>,
    #[serde(borrow)]
    exec_fee_rv: Number<'a>,
    currency: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderRow<'a> {
    #[serde(alias = "orderID")]
    order_id: &'a str,
    symbol: &'a str,
    side: &'a str,
    ord_type: &'a str,
    ord_status: &'a str,
    action_time_ns: u64,
    transact_time_ns: u64,
    #[serde(borrow)]
    price_rp: Number<'a>,
    #[serde(borrow)]
    order_qty_rq: Number<'a>,
    #[serde(borrow)]
    cum_qty_rq: Number<'a>,
    reduce_only: bool,
    #[serde(default)]
    stop_direction: &'a str,
    #[serde(default)]
    peg_price_type: &'a str,
    #[serde(default)]
    close_on_trigger: bool,
    #[serde(default, borrow)]
    stop_px_rp: Option<Number<'a>>,
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
    price: f64,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    created_time: f64,
    updated_time: f64,
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

fn side(value: &str) -> Option<&'static str> {
    match value {
        "Buy" => Some("buy"),
        "Sell" => Some("sell"),
        _ => None,
    }
}
fn bounds(from: Option<f64>, to: Option<f64>) -> bool {
    [from, to]
        .into_iter()
        .flatten()
        .all(|v| v.is_finite() && v >= 0.0 && v.fract() == 0.0 && v <= 9_007_199_254_740_991.0)
        && !matches!((from, to), (Some(a), Some(b)) if a > b)
}
fn within(request: &Request, timestamp: f64) -> bool {
    request.from.is_none_or(|from| timestamp >= from) && request.to.is_none_or(|to| timestamp <= to)
}
fn milliseconds(ns: u64) -> f64 {
    // Divide before conversion so large nanosecond integers never round across
    // a millisecond or second boundary.
    (ns / 1_000_000) as f64
}

impl PhemexPerpetuals {
    pub(crate) fn finish_local_history(cx: &mut Cx<'_, Pending>, result: Result<&str, &str>) {
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        match result {
            Ok(data) => {
                let _ = cx.done_context(&context, data.into());
            }
            Err(error) => {
                let _ = cx.fail_context(&context, error);
            }
        }
    }
    pub(crate) fn history_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        if matches!(query, Query::Income { .. }) {
            self.income_query(cx, command, query);
            return;
        }
        let mut client_id = None;
        let (symbol, from, to, limit, order_id, rows) = match query {
            Query::OrderByClientId {
                symbol,
                client_order_id,
            } => {
                if !crate::trading::valid_client(&client_order_id) {
                    Self::finish_local_history(cx, Err("Invalid Phemex client order ID"));
                    return;
                }
                if cx.store.market(&symbol).is_none() {
                    Self::finish_local_history(cx, Ok(r#"{"found":false,"definitive":false}"#));
                    return;
                }
                client_id = Some(client_order_id);
                (
                    Some(symbol),
                    None,
                    None,
                    Some(1),
                    None,
                    Rows::Lookup(Vec::new()),
                )
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                symbol,
                from_ms,
                to_ms,
                limit,
                order_id,
                Rows::Fills(owned_rows(command)),
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                symbol,
                from_ms,
                to_ms,
                limit,
                None,
                Rows::Orders(owned_rows(command)),
            ),
            Query::PublicTrades { symbol, limit } => (
                Some(symbol),
                None,
                None,
                limit,
                None,
                Rows::Trades(owned_rows(command)),
            ),
            Query::Income { .. } => unreachable!(),
        };
        if (self.options.public_data_only || self.credentials.is_none())
            && !matches!(rows, Rows::Trades(_))
        {
            Self::finish_local_history(cx, Err("Phemex private history requires credentials"));
            return;
        }
        let Some(symbol) = symbol.filter(|s| cx.store.market(s).is_some()) else {
            Self::finish_local_history(cx, Err("Phemex history requires a known symbol"));
            return;
        };
        if !bounds(from, to) {
            Self::finish_local_history(cx, Err("Invalid Phemex history time range"));
            return;
        }
        if self.history.active.len() >= 16 || self.history.active.contains(&command) {
            Self::finish_local_history(cx, Err("Too many active Phemex history queries"));
            return;
        }
        let native_symbol = self.native_symbol(&symbol).to_owned();
        if !native_symbol
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            Self::finish_local_history(cx, Err("Unsupported Phemex native history symbol"));
            return;
        }
        self.history.active.insert(command);
        let limit = limit.unwrap_or(100).clamp(1, MAX_ROWS) as usize;
        let request = Request {
            command,
            symbol,
            native_symbol,
            from,
            to,
            limit,
            order_id,
            client_id,
            rows,
            seen: HashSet::new(),
            count: 0,
            invalid: false,
        };
        self.history_send(cx, request);
    }

    fn history_send(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        if cx.now < self.paused_until {
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.history.deferred.push((timer, request));
            return;
        }
        let (path, at) = match request.rows {
            Rows::Lookup(_) => ("/api-data/g-futures/orders/by-order-id", ArrayAt::Root),
            Rows::Fills(_) => ("/api-data/g-futures/trades", ArrayAt::Root),
            Rows::Orders(_) => ("/api-data/g-futures/orders", ArrayAt::Root),
            Rows::Trades(_) => ("/md/v2/trade", ArrayAt::Path(&["result", "trades_p"])),
        };
        let mut query = format!("symbol={}", request.native_symbol);
        let from = request.from;
        let to = request.to;
        let limit = request.limit;
        if let Some(client_id) = &request.client_id {
            query.push_str("&clOrdID=");
            query.push_str(&crate::trading::encode(client_id));
        } else if !matches!(request.rows, Rows::Trades(_)) {
            if let Some(from) = from {
                query.push_str(&format!("&start={from:.0}"));
            }
            if let Some(to) = to {
                query.push_str(&format!("&end={to:.0}"));
            }
            query.push_str(&format!("&offset=0&limit={limit}"));
        }
        self.request(
            cx,
            Method::Get,
            path,
            &query,
            None,
            Pending::History(request),
            Some(at),
        );
    }

    pub(crate) fn history_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if request.invalid {
            return;
        }
        request.count += 1;
        // The public trade endpoint has no requested size. Bound its documented
        // recent snapshot too; refuse oversized pages instead of retaining them.
        let cap = if matches!(request.rows, Rows::Trades(_)) {
            1_000
        } else {
            MAX_ROWS as usize
        };
        if request.count > cap {
            request.invalid = true;
            return;
        }
        if decode(request, item).is_none() {
            request.invalid = true;
        }
    }

    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.history_failed(
                cx,
                request,
                &format!("Phemex history HTTP {}", response.status),
            );
            return;
        }
        // Direct/native hosts can deliver the full body instead of chunks.
        let parsed = if matches!(request.rows, Rows::Trades(_)) {
            #[derive(Deserialize)]
            struct Envelope<'a> {
                #[serde(borrow)]
                result: Recent<'a>,
                error: Option<&'a serde_json::value::RawValue>,
            }
            #[derive(Deserialize)]
            struct Recent<'a> {
                symbol: &'a str,
                #[serde(borrow)]
                trades_p: &'a serde_json::value::RawValue,
            }
            serde_json::from_str::<Envelope<'_>>(response.body)
                .ok()
                .filter(|body| body.error.is_none() && body.result.symbol == request.native_symbol)
                .is_some_and(|body| {
                    safe_cex_engine::json::each::<&serde_json::value::RawValue>(
                        body.result.trades_p.get(),
                        |row| self.history_item(cx, &mut request, row.get()),
                    )
                    .is_ok()
                })
        } else {
            safe_cex_engine::json::each::<&serde_json::value::RawValue>(response.body, |row| {
                self.history_item(cx, &mut request, row.get())
            })
            .is_ok()
        };
        self.history.active.remove(&request.command);
        if !parsed || request.invalid {
            self.history_failed(
                cx,
                request,
                "Malformed or oversized Phemex history response",
            );
            return;
        }
        match request.rows {
            Rows::Lookup(rows) => {
                #[derive(Serialize)]
                struct Lookup<'a> {
                    found: bool,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    order: Option<&'a safe_cex_engine::model::Order>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    definitive: Option<bool>,
                }
                let result = serde_json::to_string(&Lookup {
                    found: !rows.is_empty(),
                    order: rows.first(),
                    definitive: rows.is_empty().then_some(true),
                });
                match result.map_err(|error| error.to_string()) {
                    Ok(data) => cx.done(request.command, data),
                    Err(error) => cx.fail(request.command, error),
                };
            }
            Rows::Fills(owner) => finish_rows(*owner, cx, request.limit, |row| row.timestamp),
            Rows::Orders(owner) => finish_rows(*owner, cx, request.limit, |row| row.created_time),
            Rows::Trades(owner) => finish_rows(*owner, cx, request.limit, |row| row.timestamp),
        }
    }

    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        self.history.active.remove(&request.command);
        match request.rows {
            Rows::Lookup(_) => cx.fail(request.command, error),
            Rows::Fills(owner) => owner.finish(cx, Err(VenueError::new(error))),
            Rows::Orders(owner) => owner.finish(cx, Err(VenueError::new(error))),
            Rows::Trades(owner) => owner.finish(cx, Err(VenueError::new(error))),
        }
    }
    pub(crate) fn history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let Some(index) = self
            .history
            .deferred
            .iter()
            .position(|(id, _)| *id == timer)
        else {
            return false;
        };
        let (_, request) = self.history.deferred.swap_remove(index);
        self.history_send(cx, request);
        true
    }
}

fn decode(request: &mut Request, text: &str) -> Option<()> {
    match request.rows {
        Rows::Lookup(_) => {
            let row: crate::account_wire::OrderRow<'_> = serde_json::from_str(text).ok()?;
            if row.symbol != request.native_symbol
                || Some(row.client_order_id.as_ref()) != request.client_id.as_deref()
                || request.count > 1
                || row.version() == 0
            {
                return None;
            }
            let mut orders = row.orders(milliseconds(row.version()))?;
            if let Rows::Lookup(rows) = &mut request.rows {
                rows.push(orders.remove(0));
            }
        }
        Rows::Fills(_) => {
            let row: FillRow<'_> = serde_json::from_str(text).ok()?;
            if row.trade_type != "Trade" {
                return Some(());
            }
            if row.id.is_empty()
                || row.order_id.is_empty()
                || row.currency.is_empty()
                || row.transact_time_ns == 0
            {
                return None;
            }
            let side = side(row.side)?;
            let price = row.exec_price_rp.value().filter(|v| *v > 0.0)?;
            let qty = row.exec_qty_rq.value().filter(|v| *v > 0.0)?;
            let fee = row.exec_fee_rv.value()?;
            let timestamp = milliseconds(row.transact_time_ns);
            if row.symbol != request.native_symbol
                || !within(request, timestamp)
                || request
                    .order_id
                    .as_deref()
                    .is_some_and(|id| id != row.order_id)
            {
                return Some(());
            }
            if !request.seen.insert(row.id.into()) {
                return Some(());
            }
            if let Rows::Fills(rows) = &mut request.rows {
                rows.stage(Execution {
                    id: row.id.into(),
                    timestamp: (row.transact_time_ns / 1_000_000_000) as f64,
                    symbol: request.symbol.clone(),
                    order_id: row.order_id.into(),
                    price,
                    qty,
                    side,
                    commission: fee.abs(),
                    commission_asset: row.currency.into(),
                });
            }
        }
        Rows::Orders(_) => {
            let row: OrderRow<'_> = serde_json::from_str(text).ok()?;
            let side = side(row.side)?;
            let status = match row.ord_status {
                "New" | "Untriggered" | "PartiallyFilled" => return Some(()),
                "Filled" | "Triggered" => OrderStatus::Closed,
                "Canceled" | "Rejected" => OrderStatus::Canceled,
                _ => return None,
            };
            let trigger = match row.stop_px_rp.as_ref() {
                Some(value) => value.value()?,
                None => 0.0,
            };
            let mut kind = match row.ord_type {
                "Limit" => OrderType::Limit,
                "Market" => OrderType::Market,
                "Stop" | "StopLimit" | "MarketIfTouched" | "LimitIfTouched"
                    if row.reduce_only || row.close_on_trigger =>
                {
                    match (row.stop_direction, side) {
                        ("Rising", "sell") | ("Falling", "buy") => OrderType::TakeProfit,
                        ("Falling", "sell") | ("Rising", "buy") => OrderType::StopLoss,
                        _ => return None,
                    }
                }
                "Stop" | "StopLimit" | "MarketIfTouched" | "LimitIfTouched" => {
                    OrderType::Conditional
                }
                _ => return None,
            };
            if row.peg_price_type == "TrailingStopPeg" {
                kind = OrderType::TrailingStopLoss;
            } else if !row.peg_price_type.is_empty() {
                return None;
            }
            if matches!(
                kind,
                OrderType::Conditional
                    | OrderType::StopLoss
                    | OrderType::TakeProfit
                    | OrderType::TrailingStopLoss
            ) && trigger <= 0.0
            {
                return None;
            }
            let price = row.price_rp.value().filter(|v| *v >= 0.0)?;
            let amount = row
                .order_qty_rq
                .value()
                .filter(|v| *v > 0.0 || *v == 0.0 && row.close_on_trigger)?;
            let filled = row
                .cum_qty_rq
                .value()
                .filter(|v| *v >= 0.0 && (amount == 0.0 || *v <= amount))?;
            if row.order_id.is_empty()
                || row.action_time_ns == 0
                || row.transact_time_ns == 0
                || !trigger.is_finite()
                || trigger < 0.0
            {
                return None;
            }
            let timestamp = milliseconds(row.action_time_ns);
            if row.symbol != request.native_symbol
                || !within(request, timestamp)
                || !request.seen.insert(row.order_id.into())
            {
                return Some(());
            }
            if let Rows::Orders(rows) = &mut request.rows {
                rows.stage(HistoricalOrder {
                    id: row.order_id.into(),
                    symbol: request.symbol.clone(),
                    side,
                    kind,
                    status,
                    price: if trigger > 0.0 { trigger } else { price },
                    amount,
                    filled,
                    reduce_only: row.reduce_only || row.close_on_trigger,
                    created_time: timestamp,
                    updated_time: milliseconds(row.transact_time_ns),
                });
            }
        }
        Rows::Trades(_) => {
            let (time, raw_side, price, qty): (u64, &str, Number<'_>, Number<'_>) =
                serde_json::from_str(text).ok()?;
            let side = side(raw_side)?;
            let price = price.value().filter(|v| *v > 0.0)?;
            let qty = qty.value().filter(|v| *v > 0.0)?;
            if time == 0 || !(price * qty).is_finite() {
                return None;
            }
            // The API has no execution ID; equal tuples can be distinct trades.
            let id = format!("{time}:{}", request.count - 1);
            if let Rows::Trades(rows) = &mut request.rows {
                rows.stage(Trade {
                    id,
                    symbol: request.symbol.clone(),
                    price,
                    qty,
                    quote_qty: price * qty,
                    timestamp: milliseconds(time),
                    side,
                });
            }
        }
    }
    Some(())
}
