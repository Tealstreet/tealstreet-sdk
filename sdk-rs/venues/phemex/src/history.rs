//! Bounded rare reads; documented g-futures history uses textual enums, unlike
//! the numeric execution endpoint whose side enum remains unverified.
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::credential_selection::PrimaryReason;
use safe_cex_engine::history::{ReadCompletion, ReadHistoryRows};
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::request_context::{Context, Operation, Owner, Policy, RetryPolicy};
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, TimerId, VenueError, json::ArrayAt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const MAX_ROWS: u32 = 200;

#[derive(Default)]
pub struct State {
    pub(super) active: HashSet<CommandId>,
    pub(super) deferred: Vec<(TimerId, Deferred)>,
    owners: HashMap<CommandId, Owner>,
}

pub(super) enum Deferred {
    Signed(Box<Request>),
    PublicTrades(Box<crate::public_trades::Read>),
}

pub struct Request {
    root: ReadRoot,
    leg: Option<ReadLeg>,
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
    pub(crate) fn validate_delivery(
        &self,
        cx: &Cx<'_, Pending>,
    ) -> Result<(), safe_cex_engine::request_context::Error> {
        self.root.validate_delivery(
            cx,
            self.leg
                .as_ref()
                .ok_or(safe_cex_engine::request_context::Error::Revoked)?,
        )
    }
    fn is_public_trade(&self) -> bool {
        matches!(self.rows, Rows::Trades(_))
    }
    pub(crate) fn is_lookup(&self) -> bool {
        self.client_id.is_some()
    }
}

enum Kind {
    Fills,
    Orders,
    Trades,
    Lookup,
}

enum Rows {
    Fills(Box<ReadHistoryRows<Execution>>),
    Orders(Box<ReadHistoryRows<HistoricalOrder>>),
    Trades(Box<ReadHistoryRows<Trade>>),
    Lookup(Vec<safe_cex_engine::model::Order>),
}

fn encode_rows<T: Serialize>(rows: &[T]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}

fn owned_rows<T: Serialize>(completion: ReadCompletion) -> Box<ReadHistoryRows<T>> {
    Box::new(ReadHistoryRows::new(completion, encode_rows))
}

fn finish_rows<T: Serialize, P>(
    mut owner: ReadHistoryRows<T>,
    cx: &mut Cx<'_, P>,
    leg: &Context,
    limit: usize,
    time: impl Fn(&T) -> f64,
) {
    // These endpoints have exactly one response. No rows are admitted before
    // its complete HTTP/envelope/row verdict, so a failure can never be partial.
    let Ok(mut page) = owner.admit(cx, leg) else {
        return;
    };
    page.accept_page();
    let _ = owner.finish_with(cx, Ok(()), |mut rows| {
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
pub(super) struct Trade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    pub(super) timestamp: f64,
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
        if self.credentials.is_none()
            && self.read_credentials.is_none()
            && let Query::PublicTrades { symbol, limit } = query
        {
            self.fetch_public_trades(cx, command, symbol, limit);
            return;
        }
        let mut client_id = None;
        let (symbol, from, to, limit, order_id, kind) = match query {
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
                (Some(symbol), None, None, Some(1), None, Kind::Lookup)
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (symbol, from_ms, to_ms, limit, order_id, Kind::Fills),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (symbol, from_ms, to_ms, limit, None, Kind::Orders),
            Query::PublicTrades { symbol, limit } => {
                (Some(symbol), None, None, limit, None, Kind::Trades)
            }
            Query::Income { .. } => unreachable!(),
        };
        if (self.options.public_data_only || self.credentials.is_none())
            && !matches!(kind, Kind::Trades)
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
        let role = if matches!(kind, Kind::Lookup) {
            ReadRole::Primary {
                reason: PrimaryReason::ClientOrderLookup,
                scope: None,
            }
        } else {
            ReadRole::Read { scope: None }
        };
        let slots = if matches!(kind, Kind::Trades) {
            &self.public_read_slots
        } else {
            &self.read_slots
        };
        let Ok(root) = SelectedRead::command_root(
            cx,
            SelectedRead::select(slots, role),
            LocalRefusal {
                diagnostic: None,
                error: VenueError::new("Phemex private history requires credentials"),
            },
        ) else {
            return;
        };
        let completion = match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(root.context(), error.read_venue_error());
                return;
            }
        };
        let rows = match kind {
            Kind::Fills => Rows::Fills(owned_rows(completion)),
            Kind::Orders => Rows::Orders(owned_rows(completion)),
            Kind::Trades => Rows::Trades(owned_rows(completion)),
            Kind::Lookup => Rows::Lookup(Vec::new()),
        };
        self.history.owners.insert(command, root.context().owner());
        self.history.active.insert(command);
        let limit = limit.unwrap_or(100).clamp(1, MAX_ROWS) as usize;
        let request = Request {
            root,
            leg: None,
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

    fn history_send(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        if cx
            .validate_read_root_context(request.root.context())
            .is_err()
        {
            return;
        }
        if cx.now < self.paused_until {
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.history
                .deferred
                .push((timer, Deferred::Signed(Box::new(request))));
            return;
        }
        let (path, at) = match request.rows {
            Rows::Lookup(_) => ("/api-data/g-futures/orders/by-order-id", ArrayAt::Root),
            Rows::Fills(_) => ("/api-data/g-futures/trades", ArrayAt::Root),
            Rows::Orders(_) => ("/api-data/g-futures/orders", ArrayAt::Root),
            Rows::Trades(_) => ("/md/v2/trade", ArrayAt::Path(&["result", "trades_p"])),
        };
        let native_symbol = &request.native_symbol;
        let mut query = format!("symbol={native_symbol}");
        let from = request.from;
        let to = request.to;
        let limit = request.limit;
        if let Some(client_id) = &request.client_id {
            query.push_str("&clOrdID=");
            query.push_str(&crate::trading::encode(client_id));
        } else if !request.is_public_trade() {
            if let Some(from) = from {
                query.push_str(&format!("&start={from:.0}"));
            }
            if let Some(to) = to {
                query.push_str(&format!("&end={to:.0}"));
            }
            query.push_str(&format!("&offset=0&limit={limit}"));
        }
        let leg = match request.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: 20_000,
                pacing: None,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                self.history_failed(cx, request, error.read_venue_error());
                return;
            }
        };
        let testnet = self.options.testnet;
        let url = format!("{}{path}?{query}", self.rest());
        if let Err((Pending::History(request), error)) = leg.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url,
                body: None,
                stream: Some(at),
            },
            |identity| {
                Box::new(crate::dispatch_signing::Http::new(
                    identity.clone(),
                    testnet,
                    Method::Get,
                    path.into(),
                    query,
                    None,
                ))
            },
            |leg| {
                request.leg = Some(leg);
                Pending::History(request)
            },
        ) {
            self.history_failed(cx, request, error.read_venue_error());
        }
    }

    pub(crate) fn history_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if request.invalid || request.validate_delivery(cx).is_err() {
            return;
        }
        request.count += 1;
        // The public trade endpoint has no requested size. Bound its documented
        // recent snapshot too; refuse oversized pages instead of retaining them.
        let cap = if request.is_public_trade() {
            1_000
        } else {
            MAX_ROWS as usize
        };
        if request.count > cap {
            request.invalid = true;
            return;
        }
        if decode(cx, request, item).is_none() {
            request.invalid = true;
        }
    }

    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if request.validate_delivery(cx).is_err() {
            return;
        }
        if !(200..300).contains(&response.status) {
            self.history_failed(
                cx,
                request,
                format!("Phemex history HTTP {}", response.status),
            );
            return;
        }
        // Direct/native hosts can deliver the full body instead of chunks.
        let parsed = if request.is_public_trade() {
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
        if !parsed || request.invalid {
            self.history_failed(
                cx,
                request,
                "Malformed or oversized Phemex history response",
            );
            return;
        }
        self.retire_history(request.root.context().owner());
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
                    Ok(data) => {
                        let _ = cx.done_context(request.root.context(), data);
                    }
                    Err(error) => {
                        let _ = cx.fail_context(request.root.context(), error);
                    }
                };
            }
            Rows::Fills(owner) => finish_rows(
                *owner,
                cx,
                request.leg.as_ref().unwrap().context(),
                request.limit,
                |row| row.timestamp,
            ),
            Rows::Orders(owner) => finish_rows(
                *owner,
                cx,
                request.leg.as_ref().unwrap().context(),
                request.limit,
                |row| row.created_time,
            ),
            Rows::Trades(owner) => finish_rows(
                *owner,
                cx,
                request.leg.as_ref().unwrap().context(),
                request.limit,
                |row| row.timestamp,
            ),
        }
    }

    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: impl Into<VenueError>,
    ) {
        let mut error = error.into();
        if !request.is_lookup() && error.kind.is_none() {
            error.kind = Some(safe_cex_engine::ErrorKind::Internal);
        }
        if let Some(leg) = request.leg {
            let _ = cx.finish_http_context(leg.context());
        }
        if cx
            .validate_read_root_context(request.root.context())
            .is_ok()
        {
            self.retire_history(request.root.context().owner());
            let _ = cx.fail_context(request.root.context(), error);
        }
    }
    pub(crate) fn retire_history(&mut self, owner: Owner) {
        self.history
            .deferred
            .retain(|(_, deferred)| match deferred {
                Deferred::Signed(read) => read.root.context().owner() != owner,
                Deferred::PublicTrades(_) => true,
            });
        let commands: Vec<_> = self
            .history
            .owners
            .iter()
            .filter_map(|(id, current)| (*current == owner).then_some(*id))
            .collect();
        for id in commands {
            self.history.owners.remove(&id);
            self.history.active.remove(&id);
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
        let (_, deferred) = self.history.deferred.swap_remove(index);
        match deferred {
            Deferred::Signed(request) => self.history_send(cx, *request),
            Deferred::PublicTrades(read) => self.send_public_trades(cx, read),
        }
        true
    }
}

fn decode(cx: &Cx<'_, Pending>, request: &mut Request, text: &str) -> Option<()> {
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
                rows.admit(cx, request.leg.as_ref()?.context())
                    .ok()?
                    .stage(Execution {
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
                rows.admit(cx, request.leg.as_ref()?.context())
                    .ok()?
                    .stage(HistoricalOrder {
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
            let row = decode_public_trade(text, &request.symbol, request.count - 1)?;
            if let Rows::Trades(rows) = &mut request.rows {
                rows.admit(cx, request.leg.as_ref()?.context())
                    .ok()?
                    .stage(row);
            }
        }
    }
    Some(())
}

pub(super) fn decode_public_trade(text: &str, symbol: &str, index: usize) -> Option<Trade> {
    let (time, raw_side, price, qty): (u64, &str, Number<'_>, Number<'_>) =
        serde_json::from_str(text).ok()?;
    let side = side(raw_side)?;
    let price = price.value().filter(|v| *v > 0.0)?;
    let qty = qty.value().filter(|v| *v > 0.0)?;
    if time == 0 || !(price * qty).is_finite() {
        return None;
    }
    // The API has no execution ID; equal tuples can be distinct trades.
    Some(Trade {
        id: format!("{time}:{index}"),
        symbol: symbol.into(),
        price,
        qty,
        quote_qty: price * qty,
        timestamp: milliseconds(time),
        side,
    })
}
