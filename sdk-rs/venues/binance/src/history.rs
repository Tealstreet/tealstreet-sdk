//! Rare request/response reads under the caller's Read root. Results go back
//! as JSON in the command's completion and never enter the flush frame.
use crate::account::{order_side, order_type, venue_error};
use crate::sign::Query;
use crate::wire::{HistoricalOrderRow, IncomeRow, PublicTrade, UserTrade};
use crate::{BinanceFutures, Pending};
use safe_cex_engine::credential_selection::PrimaryReason;
use safe_cex_engine::history::{HistoryRows, ReadCompletion, ReadHistoryRows, ReadRangeHistory};
use safe_cex_engine::model::{Order, OrderStatus};
use safe_cex_engine::paging::{self, PageRow, RangeWalk, Step, WindowOrder};
use safe_cex_engine::request_context::{Context, Error, Operation, Owner};
use safe_cex_engine::selected_read::{
    LocalRefusal, ReadLeg, ReadLegPolicy, ReadRequest, ReadRole, ReadRoot, SelectedRead,
};
use safe_cex_engine::{CommandId, CommandWait, Cx, Method, Outcome, Query as Read, Response};
use safe_cex_engine::{TimerId, VenueError, json, num};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, VecDeque};

const PAGE_LIMIT: u32 = 1000;
/// `/fapi/v1/userTrades` and `/fapi/v1/income` reject wider windows.
const MAX_WINDOW_MS: f64 = 7.0 * 24.0 * 3_600_000.0;
/// Income costs weight 30; chunks are paced to stay inside the weight budget.
const INCOME_CHUNK_DELAY_MS: f64 = 2_000.0;
/// Each physical page ends at this bound; the caller's wait bounds the walk.
const PAGE_TIMEOUT_MS: u32 = 20_000;

/// One physical page under its query's original root.
pub struct Request {
    command: CommandId,
    leg: Leg,
}

enum Root {
    Signed(ReadRoot),
    Public(Context),
}
impl Root {
    fn context(&self) -> &Context {
        match self {
            Self::Signed(root) => root.context(),
            Self::Public(root) => root,
        }
    }
}
enum Leg {
    Signed(ReadLeg),
    Public(Context),
}
impl Leg {
    fn context(&self) -> &Context {
        match self {
            Self::Signed(leg) => leg.context(),
            Self::Public(leg) => leg,
        }
    }
}

struct Owned {
    root: Root,
    read: InFlight,
}

enum InFlight {
    Lookup {
        symbol: String,
        client_order_id: String,
        completion: ReadCompletion,
    },
    Fills {
        symbol: String,
        order_id: Option<String>,
        limit: u32,
        windows: VecDeque<(Option<f64>, Option<f64>)>,
        history: ReadHistoryRows<Execution>,
    },
    Orders(ReadHistoryRows<HistoricalOrder>),
    Income {
        symbol: Option<String>,
        kind: Option<String>,
        history: Box<IncomeHistory>,
        limit: u32,
        delay: Option<TimerId>,
    },
    Trades {
        symbol: String,
        history: ReadHistoryRows<Trade>,
    },
}

fn encode<T: Serialize>(rows: &[T]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}
fn owned<T: Serialize>(completion: ReadCompletion) -> ReadHistoryRows<T> {
    HistoryRows::new(completion, encode)
}
fn income_incomplete(chunks: u32) -> String {
    format!(
        "Income history is INCOMPLETE: {chunks} chunk(s) could not be walked to completion. Any total derived from it is a lower bound."
    )
}

/// Native rows of one page, parsed before the delivery is admitted.
fn rows<'a, W: Deserialize<'a>, T>(
    body: &'a str,
    mut row: impl FnMut(W) -> Option<T>,
) -> Result<Vec<T>, VenueError> {
    let mut rows = Vec::new();
    json::each::<W>(body, |native| rows.extend(row(native)))
        .map_err(|error| VenueError::new(format!("Malformed Binance history: {error}")))?;
    Ok(rows)
}
/// Whole-page admission is the only path which mutates accepted coverage.
fn accept<T>(
    history: &mut ReadHistoryRows<T>,
    cx: &Cx<'_, Pending>,
    leg: &Context,
    rows: Vec<T>,
) -> Result<(), Error> {
    let mut page = history.admit(cx, leg)?;
    for row in rows {
        page.stage(row);
    }
    page.accept_page();
    Ok(())
}

#[allow(clippy::large_enum_variant)] // Boxed whole in `InFlight::Income`.
enum IncomeHistory {
    Single(ReadHistoryRows<Income>),
    Range(ReadRangeHistory<Income>),
}
impl IncomeHistory {
    fn validate(&self, cx: &Cx<'_, Pending>, leg: &Context) -> Result<(), Error> {
        match self {
            Self::Single(history) => history.validate_delivery(cx, leg),
            Self::Range(history) => history.validate_delivery(cx, leg),
        }
    }
    fn accept(
        &mut self,
        cx: &Cx<'_, Pending>,
        leg: &Context,
        rows: Vec<Income>,
    ) -> Result<(), Error> {
        match self {
            Self::Single(history) => accept(history, cx, leg, rows),
            Self::Range(history) => {
                let mut page = history.admit(cx, leg)?;
                for row in rows {
                    let facts = PageRow {
                        timestamp: row.timestamp,
                        identity: format!(
                            "{}|{}|{}|{}",
                            row.tran_id, row.category, row.income_type, row.income
                        ),
                    };
                    page.stage(row, facts);
                }
                page.accept_page();
                Ok(())
            }
        }
    }
    fn end(self, cx: &mut Cx<'_, Pending>, result: Result<(), VenueError>, strict: bool) {
        let _ = match (self, result) {
            (Self::Single(history), Err(error)) if strict => history.reject(cx, error),
            (Self::Range(history), Err(error)) if strict => history.reject(cx, error),
            (Self::Single(history), result) => history.finish(cx, result),
            (Self::Range(history), result) => history.finish_with(cx, result, |mut rows| {
                rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
                encode(&rows)
            }),
        };
    }
}
impl InFlight {
    fn validate(&self, cx: &Cx<'_, Pending>, leg: &Context) -> Result<(), Error> {
        match self {
            Self::Lookup { completion, .. } => completion.validate_delivery(cx, leg),
            Self::Fills { history, .. } => history.validate_delivery(cx, leg),
            Self::Orders(history) => history.validate_delivery(cx, leg),
            Self::Income { history, .. } => history.validate(cx, leg),
            Self::Trades { history, .. } => history.validate_delivery(cx, leg),
        }
    }
    /// A strict ending never publishes the accepted prefix.
    fn end(self, cx: &mut Cx<'_, Pending>, result: Result<(), VenueError>, strict: bool) {
        let _ = match (self, result) {
            (Self::Lookup { completion, .. }, Err(error)) => completion.finish(cx, Err(error)),
            (Self::Lookup { .. }, Ok(())) => unreachable!("a lookup ends with its response"),
            (Self::Income { history, delay, .. }, result) => {
                if let Some(timer) = delay {
                    cx.clear_timer(timer);
                }
                history.end(cx, result, strict);
                Ok(())
            }
            (Self::Fills { history, .. }, Err(error)) if strict => history.reject(cx, error),
            (Self::Orders(history), Err(error)) if strict => history.reject(cx, error),
            (Self::Trades { history, .. }, Err(error)) if strict => history.reject(cx, error),
            (Self::Fills { history, .. }, result) => history.finish(cx, result),
            (Self::Orders(history), result) => history.finish_with(cx, result, |mut rows| {
                rows.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
                encode(&rows)
            }),
            (Self::Trades { history, .. }, result) => history.finish(cx, result),
        };
    }
}

#[derive(Default)]
pub struct HistoryState {
    reads: HashMap<CommandId, Owned>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookupRow<'a> {
    #[serde(flatten, borrow)]
    order: HistoricalOrderRow<'a>,
    #[serde(borrow)]
    client_order_id: std::borrow::Cow<'a, str>,
    #[serde(default)]
    time_in_force: &'a str,
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
    id: String,
    /// Seconds.
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
    kind: safe_cex_engine::model::OrderType,
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

fn side_name(side: &str) -> &'static str {
    match order_side(side) {
        safe_cex_engine::model::OrderSide::Buy => "buy",
        safe_cex_engine::model::OrderSide::Sell => "sell",
    }
}

fn hedge_side(side: Option<&str>) -> Option<&'static str> {
    Some(match side?.to_ascii_lowercase().as_str() {
        "long" => "long",
        "short" => "short",
        _ => "both",
    })
}

fn income_category(kind: &str) -> &'static str {
    match kind.to_ascii_uppercase().as_str() {
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

fn nonzero(text: &str) -> Option<f64> {
    Some(num::parse(text)).filter(|v| *v != 0.0)
}

impl BinanceFutures {
    /// Local refusals and empty answers end through a command-owned Read context.
    fn refuse(cx: &mut Cx<'_, Pending>, error: impl Into<VenueError>) {
        if let Ok(context) = Self::read_context(cx, None) {
            let _ = cx.fail_context(&context, error);
        }
    }
    fn answer(cx: &mut Cx<'_, Pending>, data: String) {
        if let Ok(context) = Self::read_context(cx, None) {
            let _ = cx.done_context(&context, data);
        }
    }

    /// Admits the caller's Read root, fixed to one identity for the whole query.
    /// `None` is the unsigned public root.
    fn begin(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        role: Option<ReadRole>,
        read: impl FnOnce(ReadCompletion) -> InFlight,
    ) -> bool {
        match cx.command_wait() {
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return false;
            }
            None => {
                let _ = cx.reject_unstamped_command(Operation::Read);
                return false;
            }
            Some(CommandWait::Remaining(_)) => {}
        }
        let root = match role {
            None => cx.admit_caller_read_root(None, None).ok().map(Root::Public),
            Some(role) => SelectedRead::command_root(
                cx,
                SelectedRead::select(&self.credentials, role),
                LocalRefusal {
                    diagnostic: None,
                    error: VenueError::new("Binance history signing is unavailable")
                        .outcome(Outcome::NotSent),
                },
            )
            .ok()
            .map(Root::Signed),
        };
        let Some(root) = root else {
            return false;
        };
        match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => {
                let read = read(completion);
                self.history.reads.insert(command, Owned { root, read });
                true
            }
            Err(error) => {
                let _ = cx.fail_context(root.context(), error.read_venue_error());
                false
            }
        }
    }

    pub(crate) fn start_query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, read: Read) {
        if self.is_spot() || (self.is_inverse() && !matches!(read, Read::PublicTrades { .. })) {
            if matches!(
                read,
                Read::OrderByClientId { .. } | Read::PublicTrades { .. }
            ) {
                Self::refuse(cx, "Binance Spot query is not supported");
            } else {
                let profile = if self.is_spot() { "Spot" } else { "COIN-M" };
                Self::refuse(cx, format!("Binance {profile} history is not supported"));
            }
            return;
        }
        if let Read::OrderByClientId {
            symbol,
            client_order_id,
        } = read
        {
            self.lookup(cx, command, symbol, client_order_id);
            return;
        }
        if !self.credentials.has_primary() && !matches!(read, Read::PublicTrades { .. }) {
            Self::answer(cx, "[]".into());
            return;
        }
        let signed = Some(ReadRole::Read { scope: None });
        match read {
            Read::OrderByClientId { .. } => unreachable!(),
            Read::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                // userTrades requires a symbol.
                let Some(symbol) = symbol else {
                    Self::answer(cx, "[]".into());
                    return;
                };
                let windows = match (from_ms, to_ms) {
                    (Some(from), Some(to)) => split(from, to),
                    _ => VecDeque::from([(from_ms, to_ms)]),
                };
                if self.begin(cx, command, signed, |completion| InFlight::Fills {
                    symbol,
                    order_id,
                    limit: limit.unwrap_or(PAGE_LIMIT).clamp(1, PAGE_LIMIT),
                    windows,
                    history: owned(completion),
                }) {
                    self.next_fills_window(cx, command);
                }
            }
            Read::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => {
                let Some(symbol) = symbol else {
                    Self::answer(cx, "[]".into());
                    return;
                };
                if !self.begin(cx, command, signed, |completion| {
                    InFlight::Orders(owned(completion))
                }) {
                    return;
                }
                let query = Query::new()
                    .str("symbol", &symbol)
                    .opt_num("startTime", from_ms)
                    .opt_num("endTime", to_ms)
                    .num(
                        "limit",
                        f64::from(limit.unwrap_or(PAGE_LIMIT).min(PAGE_LIMIT)),
                    );
                self.history_request(cx, command, self.paths.all_orders, query);
            }
            Read::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                let walk = match (from_ms, to_ms) {
                    (Some(from), Some(to)) => {
                        Some(RangeWalk::new(from, to, MAX_WINDOW_MS, PAGE_LIMIT as usize))
                    }
                    _ => None,
                };
                let limit = limit.unwrap_or(PAGE_LIMIT).clamp(1, PAGE_LIMIT);
                let single = walk.is_none();
                if !self.begin(cx, command, signed, |completion| InFlight::Income {
                    symbol,
                    kind,
                    history: Box::new(match walk {
                        Some(walk) => IncomeHistory::Range(ReadRangeHistory::new(
                            completion,
                            walk,
                            income_incomplete,
                            encode,
                        )),
                        None => IncomeHistory::Single(owned(completion)),
                    }),
                    limit,
                    delay: None,
                }) {
                    return;
                }
                if single {
                    self.income_request(cx, command, from_ms, to_ms);
                } else {
                    self.next_income_page(cx, command);
                }
            }
            Read::PublicTrades { symbol, limit } => {
                let limit = limit.unwrap_or(PAGE_LIMIT).min(PAGE_LIMIT);
                let url = format!(
                    "{}{}?symbol={symbol}&limit={limit}",
                    self.urls.rest, self.paths.trades
                );
                if self.begin(cx, command, None, |completion| InFlight::Trades {
                    symbol,
                    history: owned(completion),
                }) {
                    self.send_page(cx, command, url, Query::new());
                }
            }
        }
    }

    fn lookup(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
        client_order_id: String,
    ) {
        let max = if self.options.portfolio_margin {
            32
        } else {
            36
        };
        if client_order_id.is_empty()
            || client_order_id.len() > max
            || !client_order_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
        {
            Self::refuse(cx, "Invalid Binance client order ID");
            return;
        }
        if cx.store.market(&symbol).is_none() {
            Self::answer(cx, UNKNOWN.into());
            return;
        }
        // Secondary GET snapshots may belong to another account. Their cached
        // client IDs cannot establish primary placement identity.
        if !self.credentials.secondary_supplied()
            && let Some(order) = cx.store.orders().iter().find(|order| {
                order.symbol == symbol && order.client_order_id.as_deref() == Some(&client_order_id)
            })
        {
            match found(order.clone(), None) {
                Ok(data) => Self::answer(cx, data),
                Err(error) => Self::refuse(cx, error),
            }
            return;
        }
        if !self.credentials.has_primary() || self.options.public_data_only {
            Self::refuse(cx, "Binance client order lookup requires credentials");
            return;
        }
        let query = Query::new()
            .str("symbol", &symbol)
            .str("origClientOrderId", &client_order_id);
        // Placement identity belongs to the primary account, even when reads use a secondary key.
        let primary = Some(ReadRole::Primary {
            reason: PrimaryReason::ClientOrderLookup,
            scope: None,
        });
        if self.begin(cx, command, primary, |completion| InFlight::Lookup {
            symbol,
            client_order_id,
            completion,
        }) {
            self.history_request(cx, command, self.paths.order, query);
        }
    }

    fn lookup_response(
        &self,
        symbol: String,
        client_order_id: String,
        response: Response<'_>,
    ) -> Result<String, String> {
        if let Ok(error) = serde_json::from_str::<crate::wire::ErrorBody<'_>>(response.body) {
            if error.code == -2013
                && ((200..300).contains(&response.status) || response.status == 400)
            {
                return Ok(ABSENT.into());
            }
            if error.code != 0 {
                return Err(format!("Binance {}: {}", error.code, error.msg));
            }
        }
        if !(200..300).contains(&response.status) {
            return Err(format!("Binance HTTP {}", response.status));
        }
        let native: LookupRow<'_> = serde_json::from_str(response.body)
            .map_err(|e| format!("Malformed Binance lookup: {e}"))?;
        let row = native.order;
        if row.symbol != symbol || native.client_order_id.as_ref() != client_order_id {
            return Err("Binance lookup identity mismatch".into());
        }
        let status = match row.status.as_ref() {
            "NEW" | "PARTIALLY_FILLED" => OrderStatus::Open,
            "FILLED" => OrderStatus::Closed,
            "CANCELED" | "EXPIRED" | "EXPIRED_IN_MATCH" | "REJECTED" => OrderStatus::Canceled,
            _ => return Err("Invalid Binance lookup status".into()),
        };
        let parse = |text: &str| {
            text.parse::<f64>()
                .ok()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| "Invalid Binance lookup number".to_owned())
        };
        let amount = parse(&row.orig_qty)?;
        let filled = parse(&row.executed_qty)?;
        if row.order_id == 0
            || !matches!(row.side.as_ref(), "BUY" | "SELL")
            || amount <= 0.0
            || filled > amount
            || !row.time.is_finite()
            || row.time <= 0.0
            || !row.update_time.is_finite()
            || row.update_time < 0.0
        {
            return Err("Invalid Binance lookup order".into());
        }
        let kind = row
            .orig_type
            .as_deref()
            .and_then(order_type)
            .or_else(|| order_type(&row.kind))
            .ok_or_else(|| "Invalid Binance lookup type".to_owned())?;
        let order = Order {
            id: if self.options.portfolio_margin {
                client_order_id.clone()
            } else {
                row.order_id.to_string()
            },
            parent_id: None,
            status,
            symbol,
            kind,
            side: order_side(&row.side),
            price: parse(&row.price)?,
            amount,
            filled,
            remaining: (amount - filled).max(0.0),
            reduce_only: row.reduce_only,
            close: None,
            client_order_id: Some(client_order_id),
            time_in_force: crate::account::time_in_force(native.time_in_force),
            last_update: if row.update_time > 0.0 {
                row.update_time
            } else {
                row.time
            },
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        let info = serde_json::from_str::<&RawValue>(response.body).map_err(|e| e.to_string())?;
        found(order, Some(info))
    }

    fn history_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        path: &str,
        query: Query,
    ) {
        let url = format!("{}{path}", self.urls.account_rest);
        self.send_page(cx, command, url, query);
    }

    /// One physical page, minted from the original root at send time.
    fn send_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        url: String,
        query: Query,
    ) {
        let Some(owned) = self.history.reads.get(&command) else {
            return;
        };
        let pending = |leg| Pending::History(Box::new(Request { command, leg }));
        let queued = match &owned.root {
            Root::Signed(root) => {
                match root.leg(
                    cx,
                    ReadLegPolicy {
                        timeout_ms: PAGE_TIMEOUT_MS,
                        pacing: None,
                    },
                ) {
                    Ok(leg) => leg.queue(
                        cx,
                        ReadRequest {
                            method: Method::Get,
                            url,
                            body: None,
                            stream: None,
                        },
                        |identity| Box::new(self.signed_recipe(identity.clone(), query)),
                        |leg| pending(Leg::Signed(leg)),
                    ),
                    Err(error) => return self.history_admission_failed(cx, command, error),
                }
            }
            Root::Public(root) => match cx.http_leg_context(root, crate::reads::policy(None)) {
                Ok(leg) => cx.request_managed(
                    leg.clone(),
                    None,
                    Method::Get,
                    url,
                    None,
                    pending(Leg::Public(leg)),
                ),
                Err(error) => return self.history_admission_failed(cx, command, error),
            },
        };
        if let Err((pending, error)) = queued {
            if let Pending::History(request) = pending {
                let _ = cx.finish_http_context(request.leg.context());
            }
            self.history_admission_failed(cx, command, error);
        }
    }

    /// A revoked root is retired by the engine, which already owns its terminal.
    fn history_admission_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        error: Error,
    ) {
        let Some(owned) = self.history.reads.remove(&command) else {
            return;
        };
        if error == Error::Revoked
            && cx.validate_read_root_context(owned.root.context()) == Err(Error::Revoked)
        {
            if let InFlight::Income {
                delay: Some(timer), ..
            } = owned.read
            {
                cx.clear_timer(timer);
            }
            return;
        }
        owned.read.end(cx, Err(error.read_venue_error()), true);
    }

    pub(crate) fn retire_history(&mut self, owner: Owner) {
        self.history
            .reads
            .retain(|_, owned| owned.root.context().owner() != owner);
    }

    fn next_fills_window(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(Owned {
            read:
                InFlight::Fills {
                    symbol,
                    order_id,
                    limit,
                    windows,
                    ..
                },
            ..
        }) = self.history.reads.get_mut(&command)
        else {
            return;
        };
        let Some((from, to)) = windows.pop_front() else {
            if let Some(owned) = self.history.reads.remove(&command) {
                owned.read.end(cx, Ok(()), false);
            }
            return;
        };
        let query = Query::new()
            .str("symbol", symbol)
            .num("limit", f64::from(*limit))
            .opt_str("orderId", order_id.as_deref())
            .opt_num("startTime", from)
            .opt_num("endTime", to);
        self.history_request(cx, command, self.paths.user_trades, query);
    }

    fn income_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        from: Option<f64>,
        to: Option<f64>,
    ) {
        let Some(Owned {
            read:
                InFlight::Income {
                    symbol,
                    kind,
                    limit,
                    ..
                },
            ..
        }) = self.history.reads.get(&command)
        else {
            return;
        };
        let query = Query::new()
            .num("limit", f64::from(*limit))
            .opt_str("symbol", symbol.as_deref())
            .opt_str("incomeType", kind.as_deref())
            .opt_num("startTime", from)
            .opt_num("endTime", to);
        self.history_request(cx, command, self.paths.income, query);
    }

    fn next_income_page(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(Owned {
            read: InFlight::Income { history, delay, .. },
            ..
        }) = self.history.reads.get_mut(&command)
        else {
            return;
        };
        let IncomeHistory::Range(history) = history.as_mut() else {
            return;
        };
        match history.step() {
            Step::Fetch { from, to } => self.income_request(cx, command, Some(from), Some(to)),
            Step::ChunkDone => *delay = Some(cx.set_timer(INCOME_CHUNK_DELAY_MS)),
            Step::Finished => {
                if let Some(owned) = self.history.reads.remove(&command) {
                    owned.read.end(cx, Ok(()), false);
                }
            }
        }
    }

    /// The chunk gap holds no leg while the root's wait keeps running.
    pub(crate) fn on_history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        let Some((command, root)) =
            self.history
                .reads
                .iter_mut()
                .find_map(|(command, owned)| match &mut owned.read {
                    InFlight::Income { delay, .. } if *delay == Some(timer) => {
                        *delay = None;
                        Some((
                            *command,
                            cx.validate_read_root_context(owned.root.context()),
                        ))
                    }
                    _ => None,
                })
        else {
            return;
        };
        match root {
            Ok(()) => self.next_income_page(cx, command),
            Err(error) => self.history_admission_failed(cx, command, error),
        }
    }

    /// Only the current leg of a live root may publish or end its query.
    fn history_current(&self, cx: &Cx<'_, Pending>, request: &Request) -> bool {
        let Some(owned) = self.history.reads.get(&request.command) else {
            return false;
        };
        let custody = match (&owned.root, &request.leg) {
            (Root::Signed(root), Leg::Signed(leg)) => root.validate_delivery(cx, leg),
            (Root::Public(root), Leg::Public(leg)) => cx.validate_read_http_delivery(root, leg),
            _ => Err(Error::Revoked),
        };
        custody.is_ok() && owned.read.validate(cx, request.leg.context()).is_ok()
    }

    /// Transport failures and timeouts keep the accepted prefix; a provider
    /// refusal is strict.
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
        strict: bool,
    ) {
        // A failure carries no body, so its root's owner decides custody. One leg per
        // query is in flight, so a failed leg of this root is the current one.
        let current = self
            .history
            .reads
            .get(&request.command)
            .is_some_and(|owned| owned.root.context().owner() == request.leg.context().owner());
        let _ = cx.finish_http_context(request.leg.context());
        if current && let Some(owned) = self.history.reads.remove(&request.command) {
            owned.read.end(cx, Err(error), strict);
        }
    }

    pub(crate) fn on_history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let current = self.history_current(cx, &request);
        let command = request.command;
        let leg = request.leg.context();
        let owned = match self.history.reads.remove(&command) {
            Some(owned) if current => owned,
            stale => {
                if let Some(owned) = stale {
                    self.history.reads.insert(command, owned);
                }
                let _ = cx.finish_http_context(leg);
                return;
            }
        };
        let Owned { root, mut read } = owned;
        if let InFlight::Lookup {
            symbol,
            client_order_id,
            completion,
        } = read
        {
            let _ = cx.finish_http_context(leg);
            let result = self
                .lookup_response(symbol, client_order_id, response)
                .map_err(VenueError::new);
            let _ = completion.finish(cx, result);
            return;
        }
        let page = match venue_error(&response) {
            None => Ok(response.body),
            Some((_, message)) => Err(VenueError::new(message)),
        }
        .and_then(|body| {
            Ok(match &mut read {
                InFlight::Lookup { .. } => unreachable!(),
                InFlight::Fills { history, .. } => {
                    let rows = rows::<UserTrade, _>(body, |t| {
                        Some(Execution {
                            id: t.id.to_string(),
                            timestamp: (t.time / 1000.0).floor(),
                            symbol: t.symbol.into_owned(),
                            order_id: t.order_id.to_string(),
                            price: num::parse(&t.price),
                            qty: num::parse(&t.qty),
                            side: side_name(&t.side),
                            commission: t.commission.as_deref().map_or(0.0, num::parse),
                            commission_asset: t.commission_asset.map(|a| a.into_owned()),
                            position_side: hedge_side(t.position_side.as_deref()),
                            realized_pnl: t.realized_pnl.as_deref().map(num::parse),
                            maker: t.maker,
                        })
                    })?;
                    accept(history, cx, leg, rows)
                }
                InFlight::Orders(history) => {
                    let rows = rows::<HistoricalOrderRow, _>(body, |o| {
                        if o.time == 0.0 || !matches!(o.side.as_ref(), "BUY" | "SELL") {
                            return None;
                        }
                        let kind = o
                            .orig_type
                            .as_deref()
                            .and_then(order_type)
                            .or_else(|| order_type(&o.kind))
                            .unwrap_or(safe_cex_engine::model::OrderType::Limit);
                        let status = match o.status.as_ref() {
                            "NEW" | "PARTIALLY_FILLED" => OrderStatus::Open,
                            "CANCELED" | "EXPIRED" | "EXPIRED_IN_MATCH" | "REJECTED" => {
                                OrderStatus::Canceled
                            }
                            _ => OrderStatus::Closed,
                        };
                        Some(HistoricalOrder {
                            id: o.order_id.to_string(),
                            symbol: o.symbol.into_owned(),
                            side: side_name(&o.side),
                            kind,
                            status,
                            price: nonzero(&o.price),
                            average_price: o.avg_price.as_deref().and_then(nonzero),
                            amount: num::parse(&o.orig_qty),
                            filled: num::parse(&o.executed_qty),
                            reduce_only: o.reduce_only,
                            position_side: hedge_side(o.position_side.as_deref()),
                            created_time: o.time,
                            updated_time: (o.update_time > 0.0).then_some(o.update_time),
                        })
                    })?;
                    accept(history, cx, leg, rows)
                }
                InFlight::Income { history, .. } => {
                    let rows = rows::<IncomeRow, _>(body, |r| {
                        Some(Income {
                            symbol: r.symbol.into_owned(),
                            category: income_category(&r.income_type),
                            income_type: r.income_type.into_owned(),
                            income: num::parse(&r.income),
                            asset: r.asset.into_owned(),
                            timestamp: r.time,
                            tran_id: r.tran_id.to_string(),
                            trade_id: r.trade_id.map(|t| t.into_owned()).filter(|t| !t.is_empty()),
                        })
                    })?;
                    history.accept(cx, leg, rows)
                }
                InFlight::Trades { symbol, history } => {
                    let rows = rows::<PublicTrade, _>(body, |t| {
                        Some(Trade {
                            id: t.id.to_string(),
                            symbol: symbol.clone(),
                            price: num::parse(&t.price),
                            qty: num::parse(&t.qty),
                            quote_qty: num::parse(&t.quote_qty),
                            timestamp: t.time,
                            // A buyer-maker trade was an aggressive sell.
                            side: if t.is_buyer_maker { "sell" } else { "buy" },
                        })
                    })?;
                    accept(history, cx, leg, rows)
                }
            })
        });
        let _ = cx.finish_http_context(leg);
        match page {
            Err(error) => read.end(cx, Err(error), false),
            Ok(Err(error)) => {
                self.history.reads.insert(command, Owned { root, read });
                self.history_admission_failed(cx, command, error);
            }
            Ok(Ok(())) => {
                let ranged = matches!(
                    &read,
                    InFlight::Income { history, .. } if matches!(**history, IncomeHistory::Range(_))
                );
                if !ranged && !matches!(read, InFlight::Fills { .. }) {
                    return read.end(cx, Ok(()), false);
                }
                self.history.reads.insert(command, Owned { root, read });
                if ranged {
                    self.next_income_page(cx, command);
                } else {
                    self.next_fills_window(cx, command);
                }
            }
        }
    }
}

/// Split `[from, to]` into windows the venue accepts, oldest first.
fn split(from: f64, to: f64) -> VecDeque<(Option<f64>, Option<f64>)> {
    match paging::windows(
        from,
        to,
        MAX_WINDOW_MS,
        WindowOrder::OldestFirst,
        usize::MAX,
    ) {
        Some(windows) => windows
            .into_iter()
            .map(|(a, b)| (Some(a), Some(b)))
            .collect(),
        // A range the tiling refuses (inverted, non-finite or past 2^53 ms)
        // keeps one window for the venue to reject.
        None => VecDeque::from([(Some(from), Some(to))]),
    }
}
