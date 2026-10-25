//! Typed, streamed rare reads. Native derivative quantities are contract counts.
use crate::{Okx, Pending, sign};
use safe_cex_engine::Level;
use safe_cex_engine::credential_selection::{PrimaryReason, SlotPolicy};
use safe_cex_engine::history::ReadCompletion;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Order, OrderExtra, OrderSide, OrderStatus, OrderType, TimeInForce};
use safe_cex_engine::num::parse_decimal;
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Operation, Owner, Policy, RetryPolicy,
};
use safe_cex_engine::selected_read::{
    CommandReadError, LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot,
    SelectedRead,
};
use safe_cex_engine::url::encode;
use safe_cex_engine::{CommandId, Cx, Method, Query, Response, TimerId, num};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::HashMap;

const PAGE: usize = 100;
const RECENT_MS: f64 = 6.0 * 24.0 * 3_600_000.0;

#[derive(Default)]
pub struct HistoryState {
    delayed: Vec<(TimerId, Request)>,
}

pub struct Request {
    lookup: Option<String>,
    completion: ReadCompletion,
    selected: Option<ReadRoot>,
    leg: Option<HistoryLeg>,
    endpoint: &'static str,
    symbol: Option<String>,
    instrument: Option<String>,
    inst_type: &'static str,
    next_type: bool,
    order_id: Option<String>,
    income_kind: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    pages: usize,
    rows_seen: usize,
    /// Rows committed before the page in flight; a failed page rolls back to it.
    accepted: usize,
    count: usize,
    after: Option<String>,
    oldest: Option<String>,
    invalid: bool,
    indices: HashMap<String, usize>,
    rows: Rows,
}

enum HistoryLeg {
    Public(Context),
    Signed(ReadLeg),
}
impl HistoryLeg {
    fn context(&self) -> &Context {
        match self {
            Self::Public(context) => context,
            Self::Signed(leg) => leg.context(),
        }
    }
}

enum Rows {
    Fills(Vec<Execution>),
    Orders(Vec<HistoricalOrder>),
    Income(Vec<Income>),
    Trades(Vec<Trade>),
}

#[derive(Serialize)]
struct Found<'a> {
    found: bool,
    order: LookupOrder<'a>,
}
#[derive(Serialize)]
struct LookupOrder<'a> {
    #[serde(flatten)]
    order: Order,
    #[serde(skip_serializing_if = "Option::is_none")]
    info: Option<&'a RawValue>,
}
fn found(order: Order, info: Option<&RawValue>) -> Result<String, String> {
    serde_json::to_string(&Found {
        found: true,
        order: LookupOrder { order, info },
    })
    .map_err(|e| e.to_string())
}
const UNKNOWN: &str = r#"{"found":false,"definitive":false}"#;
const ABSENT: &str = r#"{"found":false,"definitive":true}"#;

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
    side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    commission: Option<f64>,
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
    side: String,
    #[serde(rename = "type")]
    kind: &'static str,
    status: &'static str,
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
    #[serde(skip)]
    bill: String,
    symbol: String,
    income_type: &'static str,
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
    side: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FillRow<'a> {
    inst_id: &'a str,
    trade_id: &'a str,
    bill_id: &'a str,
    ord_id: &'a str,
    side: &'a str,
    fill_px: &'a str,
    fill_sz: &'a str,
    fill_time: &'a str,
    ts: &'a str,
    fee: &'a str,
    fee_ccy: &'a str,
    pos_side: &'a str,
    fill_pnl: &'a str,
    exec_type: &'a str,
    px: &'a str,
    avg_px: &'a str,
    sz: &'a str,
    acc_fill_sz: &'a str,
    u_time: &'a str,
    c_time: &'a str,
    order_id: &'a str,
    fill_fee: &'a str,
    ccy: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OrderRow<'a> {
    inst_id: &'a str,
    ord_id: &'a str,
    side: &'a str,
    ord_type: &'a str,
    state: &'a str,
    px: &'a str,
    avg_px: &'a str,
    sz: &'a str,
    acc_fill_sz: &'a str,
    #[serde(borrow)]
    reduce_only: Option<Bool<'a>>,
    pos_side: &'a str,
    c_time: &'a str,
    u_time: &'a str,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Bool<'a> {
    Flag(bool),
    Text(&'a str),
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct BillRow<'a> {
    inst_id: &'a str,
    bill_id: &'a str,
    trade_id: &'a str,
    #[serde(rename = "type")]
    kind: &'a str,
    bal_chg: &'a str,
    fee: &'a str,
    pnl: &'a str,
    ccy: &'a str,
    ts: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TradeRow<'a> {
    inst_id: &'a str,
    trade_id: &'a str,
    side: &'a str,
    px: &'a str,
    sz: &'a str,
    ts: &'a str,
}

fn first<'a>(values: &[&'a str]) -> &'a str {
    values.iter().copied().find(|v| !v.is_empty()).unwrap_or("")
}
fn fill_bound(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite()).map(|v| {
        if v > 1_000_000_000_000.0 {
            v.floor()
        } else {
            (v * 1000.0).floor()
        }
    })
}
fn fill_seconds(value: f64) -> f64 {
    if value > 1_000_000_000_000.0 {
        (value / 1000.0).floor()
    } else {
        value.floor()
    }
}
fn positive(value: &str) -> Option<f64> {
    parse_decimal(value).filter(|v| *v > 0.0)
}
fn text(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.into())
}
fn hedge(value: &str) -> &'static str {
    match value {
        "long" => "long",
        "short" => "short",
        _ => "both",
    }
}
fn category(kind: &str) -> &'static str {
    match kind {
        "1" | "6" | "12" => "transfer",
        "2" => "realized_pnl",
        "5" | "9" => "liquidation",
        "8" => "funding",
        _ => "other",
    }
}
fn income_type(category: &str) -> &'static str {
    match category {
        "realized_pnl" => "REALIZED_PNL",
        "funding" => "FUNDING_FEE",
        "commission" => "COMMISSION",
        "rebate" => "REBATE",
        "transfer" => "TRANSFER",
        "liquidation" => "LIQUIDATION",
        _ => "OTHER",
    }
}
fn numeric_id(value: &str) -> &str {
    let value = value.trim_start_matches('0');
    if value.is_empty() { "0" } else { value }
}
fn id_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (numeric_id(a), numeric_id(b));
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

impl Request {
    pub(crate) fn root(&self) -> &Context {
        self.completion.root()
    }
    pub(crate) fn valid_delivery(&self, cx: &Cx<'_, Pending>) -> bool {
        self.leg.as_ref().is_some_and(|leg| {
            let selected = match (&self.selected, leg) {
                (Some(root), HistoryLeg::Signed(leg)) => root.validate_delivery(cx, leg).is_ok(),
                (None, HistoryLeg::Public(_)) => true,
                _ => false,
            };
            selected && self.completion.validate_delivery(cx, leg.context()).is_ok()
        })
    }
    pub(crate) fn is_lookup(&self) -> bool {
        self.lookup.is_some()
    }
    fn within(&self, timestamp: f64) -> bool {
        self.from.is_none_or(|v| timestamp >= v) && self.to.is_none_or(|v| timestamp <= v)
    }
    fn cursor(&mut self, value: &str) {
        if !value.is_empty()
            && value.bytes().all(|b| b.is_ascii_digit())
            && self
                .oldest
                .as_deref()
                .is_none_or(|old| id_cmp(value, old).is_lt())
        {
            self.oldest = Some(value.into());
        }
    }
    fn len(&self) -> usize {
        match &self.rows {
            Rows::Fills(rows) => rows.len(),
            Rows::Orders(rows) => rows.len(),
            Rows::Income(rows) => rows.len(),
            Rows::Trades(rows) => rows.len(),
        }
    }
    /// Drops the rows a failed page appended. A row it replaced shares its key
    /// with an accepted one: the same venue row, re-delivered.
    fn roll_back_page(&mut self) {
        let accepted = self.accepted;
        match &mut self.rows {
            Rows::Fills(rows) => rows.truncate(accepted),
            Rows::Orders(rows) => rows.truncate(accepted),
            Rows::Income(rows) => rows.truncate(accepted),
            Rows::Trades(rows) => rows.truncate(accepted),
        }
        self.indices.retain(|_, index| *index < accepted);
    }
    fn put<T>(indices: &mut HashMap<String, usize>, rows: &mut Vec<T>, key: String, row: T) {
        if let Some(index) = indices.get(&key) {
            rows[*index] = row;
        } else {
            indices.insert(key, rows.len());
            rows.push(row);
        }
    }
    fn finish(mut self, cx: &mut Cx<'_, Pending>) {
        let json = match &mut self.rows {
            Rows::Fills(rows) => {
                rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
                rows.truncate(self.limit);
                serde_json::to_string(rows)
            }
            Rows::Orders(rows) => {
                rows.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
                rows.truncate(self.limit);
                serde_json::to_string(rows)
            }
            Rows::Income(rows) => {
                rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
                let mut bills = std::collections::HashSet::new();
                for row in rows.iter() {
                    if bills.len() >= self.limit && !bills.contains(&row.bill) {
                        break;
                    }
                    bills.insert(row.bill.clone());
                }
                rows.retain(|row| bills.contains(&row.bill));
                serde_json::to_string(rows)
            }
            Rows::Trades(rows) => serde_json::to_string(rows),
        };
        {
            let _ = self.completion.finish(
                cx,
                json.map_err(|e| safe_cex_engine::VenueError::new(e.to_string())),
            );
        };
    }
}

impl Okx {
    pub(crate) fn start_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        if let Query::OrderByClientId {
            symbol,
            client_order_id,
        } = query
        {
            self.start_lookup(cx, command, symbol, client_order_id);
            return;
        }
        let public = matches!(query, Query::PublicTrades { .. });
        let keyless = self.credentials.is_none() && !public;
        let empty_fills = matches!(&query, Query::Fills { symbol: None, .. });
        let (endpoint, symbol, order_id, income_kind, from, to, limit, rows) = match query {
            Query::OrderByClientId { .. } => unreachable!(),
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => (
                "/api/v5/trade/fills",
                symbol,
                order_id,
                None,
                fill_bound(from_ms),
                fill_bound(to_ms),
                limit.unwrap_or(100).clamp(1, 100) as usize,
                Rows::Fills(Vec::new()),
            ),
            Query::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => (
                "/api/v5/trade/orders-history",
                symbol,
                None,
                None,
                from_ms,
                to_ms,
                limit.filter(|v| *v > 0).map_or(usize::MAX, |v| v as usize),
                Rows::Orders(Vec::new()),
            ),
            Query::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => (
                if from_ms.is_some_and(|from| cx.now - from > RECENT_MS) {
                    "/api/v5/account/bills-archive"
                } else {
                    "/api/v5/account/bills"
                },
                symbol,
                None,
                kind,
                from_ms,
                to_ms,
                limit.filter(|v| *v > 0).map_or(usize::MAX, |v| v as usize),
                Rows::Income(Vec::new()),
            ),
            Query::PublicTrades { symbol, limit } => (
                "/api/v5/market/trades",
                Some(symbol),
                None,
                None,
                None,
                None,
                limit.unwrap_or(100).clamp(1, 500) as usize,
                Rows::Trades(Vec::new()),
            ),
        };
        let instrument = symbol
            .as_ref()
            .and_then(|s| cx.store.market(s))
            .map(|m| m.id.clone());
        let local = if keyless || empty_fills {
            Some(Ok("[]"))
        } else if [from, to].into_iter().flatten().any(|v| !v.is_finite())
            || from.zip(to).is_some_and(|(from, to)| from > to)
        {
            Some(Err("invalid OKX history time bounds"))
        } else if symbol.is_some() && instrument.is_none() {
            Some(Ok("[]"))
        } else {
            None
        };
        let (root, selected) = if !public && local.is_none() {
            let slots = self.read_slots(SlotPolicy {
                secondary_requires_primary: true,
            });
            let selected = SelectedRead::select(&slots, ReadRole::Read { scope: None });
            match SelectedRead::command_root(
                cx,
                selected,
                LocalRefusal {
                    diagnostic: None,
                    error: safe_cex_engine::VenueError::new(
                        "OKX history credential selection failed",
                    ),
                },
            ) {
                Ok(root) => (root.context().clone(), Some(root)),
                Err(CommandReadError::Entry(CommandEntryError::Revoked)) => {
                    let _ = cx.reject_expired_command(Operation::Read);
                    return;
                }
                Err(_) => return,
            }
        } else {
            match cx.admit_caller_read_root(None, None) {
                Ok(root) => (root, None),
                Err(CommandEntryError::Rejected(_)) => return,
                Err(CommandEntryError::Revoked) => {
                    let _ = cx.reject_expired_command(Operation::Read);
                    return;
                }
            }
        };
        if let Some(local) = local {
            match local {
                Ok(data) => {
                    let _ = cx.done_context(&root, data.into());
                }
                Err(error) => {
                    let _ = cx.fail_context(&root, error);
                }
            }
            return;
        }
        let inst_type = instrument
            .as_deref()
            .map_or("SWAP", |id| self.instrument_type(id));
        let next_type = self.options.eea && instrument.is_none();
        let completion = match ReadCompletion::from_context(cx, command, root.clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(&root, error.read_venue_error());
                return;
            }
        };
        self.history_request(
            cx,
            Request {
                lookup: None,
                completion,
                selected,
                leg: None,
                endpoint,
                symbol,
                instrument,
                inst_type,
                next_type,
                order_id,
                income_kind,
                from,
                to,
                limit,
                rows,
                pages: 0,
                rows_seen: 0,
                accepted: 0,
                count: 0,
                after: None,
                oldest: None,
                invalid: false,
                indices: HashMap::new(),
            },
        );
    }

    fn start_lookup(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
        client_order_id: String,
    ) {
        let instrument = cx.store.market(&symbol).map(|market| market.id.clone());
        let local = if client_order_id.is_empty()
            || client_order_id.len() > 32
            || !client_order_id.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            Some(Err("Invalid OKX client order ID".into()))
        } else if instrument.is_none() {
            Some(Ok(UNKNOWN.into()))
        }
        // Secondary GET snapshots may belong to another account. Their cached
        // client IDs cannot establish primary placement identity.
        else if self.options.read_credentials.is_none()
            && let Some(order) = cx.store.orders().iter().find(|order| {
                order.symbol == symbol && order.client_order_id.as_deref() == Some(&client_order_id)
            })
        {
            Some(found(order.clone(), None))
        } else if self.credentials.is_none() || self.options.public_data_only {
            Some(Err("OKX client order lookup requires credentials".into()))
        } else {
            None
        };
        let (root, selected) = if local.is_none() {
            let slots = self.read_slots(SlotPolicy {
                secondary_requires_primary: true,
            });
            let selected = SelectedRead::select(
                &slots,
                ReadRole::Primary {
                    reason: PrimaryReason::ClientOrderLookup,
                    scope: None,
                },
            );
            match SelectedRead::command_root(
                cx,
                selected,
                LocalRefusal {
                    diagnostic: None,
                    error: safe_cex_engine::VenueError::new(
                        "OKX client order lookup requires credentials",
                    ),
                },
            ) {
                Ok(root) => (root.context().clone(), Some(root)),
                Err(CommandReadError::Entry(CommandEntryError::Revoked)) => {
                    let _ = cx.reject_expired_command(Operation::Read);
                    return;
                }
                Err(_) => return,
            }
        } else {
            match cx.admit_caller_read_root(None, None) {
                Ok(root) => (root, None),
                Err(CommandEntryError::Rejected(_)) => return,
                Err(CommandEntryError::Revoked) => {
                    let _ = cx.reject_expired_command(Operation::Read);
                    return;
                }
            }
        };
        if let Some(local) = local {
            match local {
                Ok(data) => {
                    let _ = cx.done_context(&root, data);
                }
                Err(error) => {
                    let _ = cx.fail_context(&root, error);
                }
            };
            return;
        }
        let instrument = instrument.expect("remote lookup has a market");
        let path = format!(
            "/api/v5/trade/order?instId={}&clOrdId={}",
            encode(&instrument),
            encode(&client_order_id)
        );
        let completion = match ReadCompletion::from_context(cx, command, root.clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(&root, error.read_venue_error());
                return;
            }
        };
        let read = Request {
            lookup: Some(client_order_id),
            completion,
            selected,
            leg: None,
            endpoint: "/api/v5/trade/order",
            symbol: Some(symbol),
            instrument: Some(instrument),
            inst_type: "SWAP",
            next_type: false,
            order_id: None,
            income_kind: None,
            from: None,
            to: None,
            limit: 1,
            pages: 0,
            rows_seen: 0,
            accepted: 0,
            count: 0,
            after: None,
            oldest: None,
            invalid: false,
            indices: HashMap::new(),
            rows: Rows::Orders(Vec::new()),
        };
        let Some(leg) = self.history_leg(cx, &read) else {
            return;
        };
        self.send_history(cx, read, leg, &path, false);
    }

    pub(crate) fn history_lookup_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        r: Request,
        response: Response<'_>,
    ) {
        if !r.valid_delivery(cx) {
            return;
        }
        let result = (|| {
            let envelope: crate::wire::Envelope<'_> = serde_json::from_str(response.body)
                .map_err(|e| format!("Malformed OKX lookup: {e}"))?;
            if envelope.code == "51603" && (200..300).contains(&response.status) {
                return Ok(ABSENT.into());
            }
            if envelope.code != "0" {
                return Err(format!("OKX {}: {}", envelope.code, envelope.msg));
            }
            if !(200..300).contains(&response.status) {
                return Err(format!("OKX HTTP {}", response.status));
            }
            let raw = envelope
                .data
                .ok_or_else(|| "Missing OKX lookup data".to_owned())?;
            let rows: Vec<&RawValue> = serde_json::from_str(raw.get())
                .map_err(|e| format!("Malformed OKX lookup list: {e}"))?;
            if rows.is_empty() {
                return Ok(UNKNOWN.into());
            }
            if rows.len() != 1 {
                return Err("Ambiguous OKX lookup rows".into());
            }
            let row: crate::wire::OrderRow<'_> = serde_json::from_str(rows[0].get())
                .map_err(|e| format!("Malformed OKX lookup row: {e}"))?;
            if Some(row.inst_id) != r.instrument.as_deref()
                || Some(row.cl_ord_id) != r.lookup.as_deref()
            {
                return Err("OKX lookup identity mismatch".into());
            }
            let status = match row.state {
                "live" | "partially_filled" => OrderStatus::Open,
                "filled" => OrderStatus::Closed,
                "canceled" | "mmp_canceled" => OrderStatus::Canceled,
                _ => return Err("Invalid OKX lookup status".into()),
            };
            let parse = |text: &str| {
                text.parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .ok_or_else(|| "Invalid OKX lookup number".to_owned())
            };
            let amount = parse(row.sz)?;
            let filled_native = parse(row.acc_fill_sz)?;
            if row.ord_id.is_empty()
                || !matches!(row.side, "buy" | "sell")
                || amount <= 0.0
                || filled_native > amount
            {
                return Err("Invalid OKX lookup order".into());
            }
            let kind = match row.ord_type {
                "market" | "optimal_limit_ioc" => OrderType::Market,
                "limit" | "post_only" | "fok" | "ioc" => OrderType::Limit,
                _ => return Err("Invalid OKX lookup order type".into()),
            };
            let symbol = r
                .symbol
                .as_deref()
                .ok_or_else(|| "Missing OKX lookup symbol".to_owned())?;
            let market = cx
                .store
                .market(symbol)
                .ok_or_else(|| "Unknown OKX lookup symbol".to_owned())?;
            let price = if row.px.is_empty() {
                0.0
            } else {
                parse(row.px)?
            };
            let reference_text = first(&[row.px, row.avg_px, row.fill_px, row.last]);
            let reference = if reference_text.is_empty() {
                0.0
            } else {
                parse(reference_text)?
            };
            let filled = self.contracts_to_base(cx, market, filled_native, reference);
            let remaining_native = (amount - filled_native).max(0.0);
            let remaining = self.contracts_to_base(cx, market, remaining_native, reference);
            if !filled.is_finite() || !remaining.is_finite() {
                return Err("Invalid OKX lookup converted size".into());
            }
            let order = Order {
                id: row.ord_id.into(),
                parent_id: None,
                status,
                symbol: symbol.into(),
                kind,
                side: if row.side == "buy" {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                },
                price,
                amount,
                filled,
                remaining,
                reduce_only: row.reduce_only,
                close: None,
                client_order_id: Some(row.cl_ord_id.into()),
                time_in_force: Some(match row.ord_type {
                    "post_only" => TimeInForce::PostOnly,
                    "fok" => TimeInForce::FillOrKill,
                    "ioc" | "optimal_limit_ioc" => TimeInForce::ImmediateOrCancel,
                    _ => TimeInForce::GoodTillCancel,
                }),
                last_update: if row.u_time.is_empty() {
                    cx.now
                } else {
                    parse(row.u_time)?
                },
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: Some(OrderExtra {
                    remaining_native: Some(remaining_native),
                    filled_in_placement_units: Some(filled_native),
                    ..OrderExtra::default()
                }),
            };
            found(order, Some(rows[0]))
        })();
        match result {
            Ok(data) => {
                let _ = cx.done_context(r.root(), data);
            }
            Err(error) => {
                let _ = cx.fail_context(r.root(), error);
            }
        };
    }

    fn history_request(&self, cx: &mut Cx<'_, Pending>, mut r: Request) {
        let Some(leg) = self.history_leg(cx, &r) else {
            return;
        };
        r.count = 0;
        r.oldest = None;
        r.pages += 1;
        r.accepted = r.len();
        let page_limit = match r.rows {
            Rows::Fills(_) | Rows::Trades(_) => r.limit,
            _ => PAGE,
        };
        let mut path = format!("{}?", r.endpoint);
        if !matches!(r.rows, Rows::Trades(_)) {
            path.push_str(&format!("instType={}&", r.inst_type));
        }
        if let Some(id) = &r.instrument {
            path.push_str(&format!("instId={id}&"));
        }
        if let Some(id) = &r.order_id {
            // IDs are opaque. Percent encode rather than allowing query injection.
            path.push_str(&format!("ordId={}&", encode(id)));
        }
        for (key, value) in [("begin", r.from), ("end", r.to)] {
            if let Some(value) = value {
                path.push_str(&format!("{key}={}&", num::format(value)));
            }
        }
        if matches!(r.rows, Rows::Income(_)) {
            let kind = match r.income_kind.as_deref() {
                Some("REALIZED_PNL" | "COMMISSION" | "REBATE") => Some("2"),
                Some("FUNDING_FEE") => Some("8"),
                _ => None,
            };
            if let Some(kind) = kind {
                path.push_str(&format!("type={kind}&"));
            }
        }
        path.push_str(&format!("limit={page_limit}"));
        if let Some(after) = &r.after {
            path.push_str(&format!("&after={after}"));
        }
        self.send_history(cx, r, leg, &path, true);
    }

    pub(crate) fn history_item(&self, cx: &mut Cx<'_, Pending>, r: &mut Request, item: &str) {
        if !r.valid_delivery(cx) {
            return;
        }
        r.count += 1;
        match r.rows {
            Rows::Fills(_) => {
                let Ok(row) = serde_json::from_str::<FillRow<'_>>(item) else {
                    r.invalid = true;
                    return;
                };
                let Some(m) = cx
                    .store
                    .market_by_id(row.inst_id)
                    .or_else(|| r.symbol.as_ref().and_then(|symbol| cx.store.market(symbol)))
                else {
                    return;
                };
                let order_id = first(&[row.ord_id, row.order_id]);
                let (Some(price), Some(size), Some(time)) = (
                    positive(first(&[row.fill_px, row.px, row.avg_px])),
                    positive(first(&[row.fill_sz, row.sz, row.acc_fill_sz])),
                    positive(first(&[row.fill_time, row.ts, row.u_time, row.c_time])),
                ) else {
                    return;
                };
                let time_ms = if time > 1_000_000_000_000.0 {
                    time
                } else {
                    time * 1000.0
                };
                if r.symbol.as_deref() != Some(m.symbol.as_str())
                    || !matches!(row.side, "buy" | "sell")
                    || !r.within(time_ms)
                    || r.order_id
                        .as_deref()
                        .is_some_and(|id| !order_id.is_empty() && id != order_id)
                {
                    return;
                }
                let qty = self.contracts_to_base(cx, m, size, price);
                if !qty.is_finite() || qty <= 0.0 {
                    return;
                }
                let id = text(row.trade_id).or_else(|| text(row.bill_id));
                let timestamp = fill_seconds(time);
                let key = id.as_ref().map(|id| format!("id:{id}")).unwrap_or_else(|| {
                    format!("{}:{timestamp}:{price}:{qty}:{}", order_id, row.side)
                });
                let execution = Execution {
                    id,
                    timestamp,
                    symbol: m.symbol.clone(),
                    order_id: text(order_id),
                    price,
                    qty,
                    side: row.side.into(),
                    commission: {
                        let value = first(&[row.fee, row.fill_fee]);
                        parse_decimal(if value.is_empty() { "0" } else { value }).map(f64::abs)
                    },
                    commission_asset: text(first(&[row.fee_ccy, row.ccy])),
                    position_side: (!row.pos_side.is_empty()).then(|| hedge(row.pos_side)),
                    realized_pnl: parse_decimal(row.fill_pnl),
                    maker: (!row.exec_type.is_empty())
                        .then(|| row.exec_type.eq_ignore_ascii_case("M")),
                };
                if let Rows::Fills(rows) = &mut r.rows {
                    Request::put(&mut r.indices, rows, key, execution);
                }
            }
            Rows::Orders(_) => {
                let Ok(row) = serde_json::from_str::<OrderRow<'_>>(item) else {
                    r.invalid = true;
                    return;
                };
                r.cursor(row.ord_id);
                let Some(m) = cx.store.market_by_id(row.inst_id) else {
                    return;
                };
                let Some(time) = positive(row.c_time) else {
                    return;
                };
                if row.ord_id.is_empty()
                    || !matches!(row.side, "buy" | "sell")
                    || !r.within(time)
                    || r.symbol.as_deref().is_some_and(|s| s != m.symbol)
                {
                    return;
                }
                let order = HistoricalOrder {
                    id: row.ord_id.into(),
                    symbol: m.symbol.clone(),
                    side: row.side.into(),
                    kind: if matches!(row.ord_type, "market" | "optimal_limit_ioc") {
                        "market"
                    } else {
                        "limit"
                    },
                    status: match row.state {
                        "live" | "partially_filled" => "open",
                        "canceled" | "mmp_canceled" => "canceled",
                        _ => "closed",
                    },
                    price: positive(row.px),
                    average_price: positive(row.avg_px),
                    amount: self.contracts_to_base(
                        cx,
                        m,
                        num::parse(row.sz),
                        positive(row.avg_px)
                            .or_else(|| positive(row.px))
                            .unwrap_or(0.0),
                    ),
                    filled: self.contracts_to_base(
                        cx,
                        m,
                        num::parse(row.acc_fill_sz),
                        positive(row.avg_px)
                            .or_else(|| positive(row.px))
                            .unwrap_or(0.0),
                    ),
                    reduce_only: matches!(
                        row.reduce_only,
                        Some(Bool::Flag(true) | Bool::Text("true"))
                    ),
                    position_side: hedge(row.pos_side),
                    created_time: time,
                    updated_time: positive(row.u_time),
                };
                if let Rows::Orders(rows) = &mut r.rows {
                    Request::put(&mut r.indices, rows, row.ord_id.into(), order);
                }
            }
            Rows::Income(_) => {
                let Ok(row) = serde_json::from_str::<BillRow<'_>>(item) else {
                    r.invalid = true;
                    return;
                };
                r.cursor(row.bill_id);
                let Some(timestamp) = positive(row.ts) else {
                    return;
                };
                if !r.within(timestamp) {
                    return;
                }
                let symbol = cx
                    .store
                    .market_by_id(row.inst_id)
                    .map_or(row.inst_id, |m| m.symbol.as_str());
                if r.symbol.as_deref().is_some_and(|s| s != symbol) {
                    return;
                }
                let (fee, pnl, balance) = (
                    num::parse(row.fee),
                    num::parse(row.pnl),
                    num::parse(row.bal_chg),
                );
                let residual = balance - fee - pnl;
                let bill_id = if row.bill_id.is_empty() {
                    row.ts
                } else {
                    row.bill_id
                };
                let cat = category(row.kind);
                for (amount, category, suffix, include) in [
                    (pnl, cat, "pnl", pnl != 0.0),
                    (
                        fee,
                        if fee < 0.0 { "commission" } else { "rebate" },
                        "fee",
                        fee != 0.0,
                    ),
                    (
                        residual,
                        cat,
                        "adj",
                        residual.abs() > 1e-12f64.max(1e-9 * balance.abs()),
                    ),
                ] {
                    if !include {
                        continue;
                    }
                    let record = Income {
                        bill: bill_id.into(),
                        symbol: symbol.into(),
                        income_type: income_type(category),
                        income: amount,
                        asset: row.ccy.into(),
                        timestamp,
                        tran_id: format!("{bill_id}:{suffix}"),
                        trade_id: text(row.trade_id),
                        category,
                    };
                    if let Rows::Income(rows) = &mut r.rows {
                        Request::put(&mut r.indices, rows, record.tran_id.clone(), record);
                    }
                }
            }
            Rows::Trades(_) => {
                let Ok(row) = serde_json::from_str::<TradeRow<'_>>(item) else {
                    r.invalid = true;
                    return;
                };
                let Some(m) = cx.store.market_by_id(row.inst_id) else {
                    return;
                };
                let (Some(price), Some(size), Some(timestamp)) =
                    (positive(row.px), positive(row.sz), positive(row.ts))
                else {
                    return;
                };
                if row.trade_id.is_empty()
                    || !matches!(row.side, "buy" | "sell")
                    || r.symbol.as_deref() != Some(m.symbol.as_str())
                {
                    return;
                }
                let qty = self.contracts_to_base(cx, m, size, price);
                if !qty.is_finite() || qty <= 0.0 || !(price * qty).is_finite() {
                    return;
                }
                let trade = Trade {
                    id: row.trade_id.into(),
                    symbol: m.symbol.clone(),
                    price,
                    qty,
                    quote_qty: price * qty,
                    timestamp,
                    side: row.side.into(),
                };
                if let Rows::Trades(rows) = &mut r.rows {
                    Request::put(&mut r.indices, rows, row.trade_id.into(), trade);
                }
            }
        }
    }

    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        _response: Response<'_>,
    ) {
        if !r.valid_delivery(cx) {
            return;
        }
        if r.invalid {
            self.history_failed(cx, r, "malformed OKX history list item");
            return;
        }
        r.rows_seen += r.count;
        if matches!(r.rows, Rows::Fills(_)) {
            if r.endpoint == "/api/v5/trade/fills" && r.count < r.limit {
                r.endpoint = "/api/v5/trade/fills-history";
                self.history_request(cx, r);
            } else {
                r.finish(cx);
            }
            return;
        }
        let (cap, enough) = match &r.rows {
            Rows::Orders(_) => (50, false),
            Rows::Income(_) => (200, r.rows_seen >= r.limit),
            Rows::Trades(_) => {
                r.finish(cx);
                return;
            }
            Rows::Fills(_) => unreachable!(),
        };
        if r.count < PAGE || enough {
            if r.next_type {
                r.next_type = false;
                r.inst_type = "FUTURES";
                r.after = None;
                r.oldest = None;
                r.pages = 0;
                r.rows_seen = 0;
                self.history_request(cx, r);
                return;
            }
            r.finish(cx);
            return;
        }
        if r.pages >= cap {
            if self.options.eea && r.instrument.is_none() {
                let _ = cx.fail_context(
                    r.root(),
                    "OKX composite history hit page cap; incomplete source",
                );
                return;
            }
            cx.event(
                Level::Warn,
                code::HISTORY_INCOMPLETE,
                format!("OKX history hit {cap}-page cap; older rows were not fetched"),
                vec![(
                    "command",
                    safe_cex_engine::Value::Num(f64::from(r.completion.command())),
                )],
            );
            r.finish(cx);
            return;
        }
        // Decimal-string comparison avoids precision loss for 64-bit OKX IDs.
        if r.oldest.as_deref().is_none_or(|next| {
            r.after
                .as_deref()
                .is_some_and(|old| !id_cmp(next, old).is_lt())
        }) {
            if self.options.eea && r.instrument.is_none() {
                let _ = cx.fail_context(
                    r.root(),
                    "OKX composite history cursor did not advance; incomplete source",
                );
                return;
            }
            cx.event(
                Level::Warn,
                code::HISTORY_INCOMPLETE,
                "OKX history cursor did not advance; older rows were not fetched",
                vec![(
                    "command",
                    safe_cex_engine::Value::Num(f64::from(r.completion.command())),
                )],
            );
            r.finish(cx);
            return;
        }
        r.after = r.oldest.take();
        let timer = cx.set_timer(100.0);
        self.history.delayed.push((timer, r));
    }

    /// A failed page never claims absence. With accepted rows a query returns
    /// them plus one incomplete marker; with none it rejects. Lookups and EEA
    /// composite reads stay strict, as their caps and stalls are.
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut r: Request,
        error: impl Into<safe_cex_engine::VenueError>,
    ) {
        let mut error = error.into();
        error.message = format!("OKX history: {}", error.message);
        if cx
            .command_error_context(r.root(), error.message.clone())
            .is_err()
        {
            return;
        }
        r.roll_back_page();
        let composite = self.options.eea && r.instrument.is_none();
        if r.is_lookup() || composite || r.accepted == 0 {
            let _ = cx.fail_context(r.root(), error);
            return;
        }
        r.completion
            .incomplete(format!("history is INCOMPLETE: {}", error.message));
        r.finish(cx);
    }
    pub(crate) fn history_sign_failed(
        cx: &mut Cx<'_, Pending>,
        r: Request,
        error: safe_cex_engine::VenueError,
    ) {
        let _ = cx.fail_context(r.root(), error);
    }
    pub(crate) fn retire_history(&mut self, owner: Owner) {
        self.history
            .delayed
            .retain(|(_, read)| read.root().owner() != owner);
    }
    fn history_leg(&self, cx: &mut Cx<'_, Pending>, read: &Request) -> Option<HistoryLeg> {
        let leg = if let Some(root) = &read.selected {
            root.leg(
                cx,
                ReadLegPolicy {
                    timeout_ms: 20_000,
                    pacing: None,
                },
            )
            .map(HistoryLeg::Signed)
        } else {
            cx.http_leg_context(
                read.root(),
                Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .map(HistoryLeg::Public)
        };
        match leg {
            Ok(leg) => Some(leg),
            Err(error) => {
                let _ = cx.fail_context(read.root(), error.read_venue_error());
                None
            }
        }
    }
    fn send_history(
        &self,
        cx: &mut Cx<'_, Pending>,
        mut read: Request,
        leg: HistoryLeg,
        path: &str,
        streamed: bool,
    ) {
        let url = format!("{}{path}", self.rest());
        let public = matches!(&leg, HistoryLeg::Public(_));
        let sent = match leg {
            HistoryLeg::Signed(leg) => {
                let testnet = self.options.testnet;
                leg.queue(
                    cx,
                    ReadRequest {
                        method: Method::Get,
                        url,
                        body: None,
                        stream: streamed.then_some(ArrayAt::EnvelopeField("data")),
                    },
                    |identity| {
                        Box::new(sign::Http::new(Some(identity.clone()), path.into()).demo(testnet))
                    },
                    |leg| {
                        read.leg = Some(HistoryLeg::Signed(leg));
                        Pending::History(Box::new(read))
                    },
                )
            }
            HistoryLeg::Public(leg) => {
                read.leg = Some(HistoryLeg::Public(leg.clone()));
                if self.options.testnet {
                    cx.request_managed_with_headers(
                        leg,
                        None,
                        Method::Get,
                        url,
                        sign::DEMO_HEADERS,
                        None,
                        Pending::History(Box::new(read)),
                    )
                } else {
                    cx.request_managed(
                        leg,
                        None,
                        Method::Get,
                        url,
                        None,
                        Pending::History(Box::new(read)),
                    )
                }
            }
        };
        match sent {
            Ok(id) if streamed && public => cx.stream_response(id, ArrayAt::EnvelopeField("data")),
            Ok(_) => {}
            Err((Pending::History(read), error)) => {
                Self::history_sign_failed(cx, *read, error.read_venue_error())
            }
            Err(_) => unreachable!("history submission retains its pending"),
        }
    }

    pub(crate) fn history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let Some(index) = self.history.delayed.iter().position(|(id, _)| *id == timer) else {
            return false;
        };
        let (_, read) = self.history.delayed.swap_remove(index);
        self.history_request(cx, read);
        true
    }
}
