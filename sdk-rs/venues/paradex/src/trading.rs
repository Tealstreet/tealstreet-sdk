use crate::{
    Pending,
    account::{Account, AmendTarget, NativeOrder},
    hosted::{Item, ModifyStatement, OrderStatement},
    sign::Credentials,
    sign::dispatch::{Orders, Preimage, SignedBody},
    wire::StrictNumber as Number,
};
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, TimeInForce};
use safe_cex_engine::num;
use safe_cex_engine::request_context::{Context, Operation as HttpOperation, Policy, RetryPolicy};
use safe_cex_engine::text::JoinWith;
use safe_cex_engine::url::encode;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, OrderIdentityKind, OrderIdentityReceipt, Outcome,
    Response, SignStep, TimerId, VenueError,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

const UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

#[derive(Serialize)]
struct Placement {
    market: String,
    side: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    instruction: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    flags: Option<[&'static str; 1]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stp: Option<&'static str>,
    signature: String,
    signature_timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
}
impl SignedBody for Placement {
    fn preimage(&self) -> Preimage<'_> {
        Preimage {
            market: &self.market,
            side: self.side,
            kind: self.kind,
            size: &self.size,
            price: self.price.as_deref(),
            id: None,
        }
    }
    fn item(&self, now_ms: u64) -> Item<'_> {
        Item::Order(OrderStatement {
            timestamp_ms: now_ms,
            market: &self.market,
            side: self.side,
            order_type: self.kind,
            size: &self.size,
            price: self.price.as_deref(),
            trigger_price: self.trigger_price.as_deref(),
            instruction: self.instruction,
            flags: self.flags,
            stp: self.stp,
            client_id: self.client_id.as_deref(),
        })
    }
    fn sign(&mut self, signature: String, timestamp: u64) {
        self.signature = signature;
        self.signature_timestamp = timestamp;
    }
}
/// The frozen unsigned body and its dispatch-time Stark step.
fn signed<T: SignedBody + 'static>(
    account: &Account,
    signer: &Credentials,
    method: Method,
    path: &str,
    legs: Vec<T>,
    batch: bool,
) -> Result<(String, Box<dyn SignStep>), String> {
    let step = Orders::new(
        signer,
        account.testnet(),
        method,
        format!("{}{path}", account.rest()),
        format!("/v1{path}"),
        legs,
        batch,
    )
    .map_err(|_| "Paradex order body is invalid".to_owned())?;
    Ok((step.unsigned_body().to_owned(), Box::new(step)))
}
fn placement_receipt(payload: &Placement) -> Option<OrderIdentityReceipt> {
    payload
        .client_id
        .as_ref()
        .map(|identity| OrderIdentityReceipt {
            symbol: payload.market.clone(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: identity.clone(),
        })
}
#[derive(Clone)]
enum Kind {
    Place {
        expected: Vec<Option<String>>,
        /// The bracket leg this request places, `None` for an entry.
        leg: Option<&'static str>,
        batch: bool,
        /// The stop this placement replaces after its cancel: symbol, side and kind.
        replaces: Option<Box<(String, OrderSide, OrderType)>>,
    },
    Amend {
        id: String,
        target: Box<AmendTarget>,
    },
    Cancel(Vec<String>),
    /// An amend's cancel of a live stop, re-checked at the final send (`StopGuard`).
    GuardedCancel(Box<StopGuard>),
    CancelProof,
    Empty,
    Quotes(String),
    Leverage {
        symbol: String,
        leverage: f64,
    },
}
impl Kind {
    fn placement_labels(&self) -> Vec<String> {
        match self {
            Self::Place { expected, leg, .. } => expected
                .iter()
                .map(|id| {
                    id.clone()
                        .unwrap_or_else(|| leg.unwrap_or("entry").to_owned())
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

struct Operation {
    receipts: Vec<OrderIdentityReceipt>,
    /// The write's dispatch-time Stark step. It keeps the field name of the
    /// payload hash it replaced, so the legacy request sites stay unchanged
    /// until PR-3 moves them to the funnel.
    hash: Option<Box<dyn SignStep>>,
    method: Method,
    path: String,
    body: Option<String>,
    kind: Kind,
}
struct Job {
    command: CommandId,
    preparing: Option<Preparation>,
    operations: VecDeque<Operation>,
    results: Vec<String>,
    errors: Vec<VenueError>,
    attempt_all: bool,
    placement: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
    /// A refused batch or protective child cannot resolve as a successful partial placement.
    leg_refused: bool,
    /// The cancelled stop placed again, unchanged, once its replacement is refused.
    restore: Option<Operation>,
    /// The stop being put back: symbol, side and kind.
    restoring: Option<(String, OrderSide, OrderType)>,
}
impl Job {
    /// Joined failures retain the first Unknown leg's diagnostics and classification.
    /// Without an Unknown leg, an outcome is shared only when every failure agrees.
    fn failure(mut self) -> VenueError {
        let unknown = self
            .errors
            .iter()
            .position(|e| e.outcome == Some(Outcome::Unknown));
        let mut error = if self.errors.len() == 1 {
            self.errors.pop().expect("one failure")
        } else {
            let outcome = self.errors[0].outcome;
            let messages: Vec<_> = self.errors.iter().map(|e| e.message.as_str()).collect();
            let message = messages.join_with("; ");
            if let Some(index) = unknown {
                let mut error = self.errors.remove(index);
                error.message = message;
                error
            } else {
                let error = VenueError::new(message);
                match outcome {
                    Some(outcome) if self.errors.iter().all(|e| e.outcome == Some(outcome)) => {
                        error.outcome(outcome)
                    }
                    _ => error,
                }
            }
        };
        if unknown.is_some() {
            error.outcome = Some(Outcome::Unknown);
        } else if (self.effective || !self.results.is_empty())
            && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
        {
            error.outcome = None;
        }
        // Accepted orders stay visible beside the failure.
        if self.results.is_empty() {
            error
        } else {
            error.partial_ids(&self.results)
        }
    }
}
struct Preparation {
    orders: Vec<PlaceOrderOpts>,
    symbols: VecDeque<String>,
    quotes: HashMap<String, Quote>,
    price_bands: HashMap<String, f64>,
}
#[derive(Clone, Copy)]
struct Quote {
    bid: Option<f64>,
    ask: Option<f64>,
    fair: Option<f64>,
}
pub struct Request {
    progress: Option<(CommandId, Vec<OrderIdentityReceipt>)>,
    /// As `Operation::hash`: the dispatch-time Stark step.
    hash: Option<Box<dyn SignStep>>,
    job: u64,
    kind: Kind,
}
impl Request {
    pub(crate) fn report_progress(&self, cx: &mut Cx<'_, Pending>) {
        if let Some((command, receipts)) = &self.progress {
            cx.command_progress(*command, receipts.clone());
        }
    }
    pub(crate) fn take_signed(&mut self) -> Option<Box<dyn SignStep>> {
        self.hash.take()
    }
    pub(crate) fn stop_guard(&self) -> Option<&StopGuard> {
        match &self.kind {
            Kind::GuardedCancel(guard) => Some(guard),
            _ => None,
        }
    }
    pub(crate) fn amend_id(&self) -> Option<&str> {
        if let Kind::Amend { id, .. } = &self.kind {
            Some(id)
        } else {
            None
        }
    }
}
#[derive(Default)]
pub struct Trading {
    next: u64,
    jobs: HashMap<u64, Job>,
    amend_locks: HashSet<String>,
    /// Stops an amend is replacing, by original id: the job holding their put-back.
    replacing: HashMap<String, u64>,
    /// Stops put back under a new id after a refused replacement: (original id,
    /// new id, symbol). A later cancel of the original or its market removes them.
    restored: VecDeque<(String, String, String)>,
    cancel_proofs: HashMap<u64, CancelProof>,
}
const CANCEL_PROOF_MS: f64 = 5_000.;
const CANCEL_PROOF_POLLS: u8 = 3;
const CANCEL_PROOF_WINDOW_MARGIN_MS: u64 = 60_000;
struct CancelProof {
    guard: Box<StopGuard>,
    deadline: f64,
    expires: TimerId,
    retry: Option<TimerId>,
    polls: u8,
}
/// How many put-back stops are remembered for a later cancel to follow.
const RESTORED_KEPT: usize = 64;

fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn instruction(tif: Option<TimeInForce>) -> &'static str {
    match tif {
        Some(TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill) => "IOC",
        Some(TimeInForce::PostOnly) => "POST_ONLY",
        _ => "GTC",
    }
}
fn native_batch(o: &PlaceOrderOpts) -> bool {
    o.kind == OrderType::Limit
        && o.trigger_price.is_none()
        && o.stop_loss.is_none()
        && o.take_profit.is_none()
        && o.tp_trigger_type.is_none()
        && o.sl_trigger_type.is_none()
        && o.close != Some(true)
        && o.time_in_force != Some(TimeInForce::FillOrKill)
}
/// A wrong-side leg would trigger at once or never; reject it rather than place an unprotected entry.
fn bracket_sides(o: &PlaceOrderOpts) -> Result<(), String> {
    let Some(entry) = o.price else {
        return Ok(());
    };
    let long = o.side == OrderSide::Buy;
    for (leg, price, above) in [
        ("take-profit", o.take_profit, long),
        ("stop-loss", o.stop_loss, !long),
    ] {
        if let Some(price) = price.filter(|&p| if above { p <= entry } else { p >= entry }) {
            let relation = if above { "above" } else { "below" };
            let side = if long { "buy" } else { "sell" };
            return Err(format!(
                "Paradex {leg} {price} must be {relation} the {side} entry price {entry}"
            ));
        }
    }
    Ok(())
}
fn client_id(cx: &Cx<'_, Pending>) -> Result<String, String> {
    safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
        .map(|id| format!("ts_{id}"))
}
/// An unsigned placement; its signature is drawn at dispatch.
fn prepare(
    cx: &Cx<'_, Pending>,
    o: &PlaceOrderOpts,
    child: bool,
    execution: Option<f64>,
) -> Result<Placement, String> {
    if !positive(o.amount) {
        return Err("Paradex order requires a finite positive amount".into());
    }
    let kind = if execution.is_some() {
        "LIMIT"
    } else {
        match o.kind {
            OrderType::Limit => "LIMIT",
            OrderType::Market => "MARKET",
            OrderType::Conditional => "STOP_LIMIT",
            OrderType::StopLoss => "STOP_MARKET",
            OrderType::TakeProfit => "TAKE_PROFIT_MARKET",
            OrderType::TrailingStopLoss => {
                return Err("Paradex trailing orders are unsupported".into());
            }
        }
    };
    if o.kind == OrderType::Conditional {
        if !o.price.is_some_and(positive) || !o.trigger_price.is_some_and(positive) {
            return Err(
                "Paradex opening trigger requires positive execution and trigger prices".into(),
            );
        }
        if o.reduce_only == Some(true) || o.stop_loss.is_some() || o.take_profit.is_some() {
            return Err(
                "Paradex opening trigger cannot be reduce-only or carry attached children".into(),
            );
        }
    }
    if o.kind == OrderType::Limit && !o.price.is_some_and(positive) {
        return Err("Paradex limit order requires a finite positive price".into());
    }
    let stop = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
    let trigger = if o.kind == OrderType::Conditional {
        o.trigger_price
    } else if stop {
        o.price
    } else {
        o.trigger_price
    };
    if stop && !trigger.is_some_and(positive) {
        return Err("Paradex stop requires a finite positive trigger price".into());
    }
    let size = num::format(o.amount);
    let price = matches!(kind, "LIMIT" | "STOP_LIMIT")
        .then(|| num::format(execution.or(o.price).expect("validated execution price")));
    Ok(Placement {
        market: o.symbol.clone(),
        side: side(o.side),
        kind,
        size,
        price,
        trigger_price: trigger.map(num::format),
        instruction: if execution.is_some() {
            "IOC"
        } else {
            instruction(o.time_in_force)
        },
        flags: (o.reduce_only == Some(true) || stop).then_some(["REDUCE_ONLY"]),
        stp: (!child).then_some("EXPIRE_TAKER"),
        signature: String::new(),
        signature_timestamp: 0,
        client_id: if child {
            None
        } else {
            Some(
                o.client_order_id
                    .clone()
                    .map_or_else(|| client_id(cx), Ok)?,
            )
        },
    })
}
fn validate_caller_ids(orders: &[PlaceOrderOpts], cx: &Cx<'_, Pending>) -> Result<(), String> {
    let mut seen = HashSet::new();
    for o in orders {
        let market = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("unknown market {}", o.symbol))?;
        if let Some(id) = &o.client_order_id
            && (id.is_empty()
                || id.chars().count() > 64
                || id.contains(['\n', '\r'])
                || !seen.insert(id)
                || !matches!(o.kind, OrderType::Limit | OrderType::Market)
                || o.trigger_price.is_some()
                || o.stop_loss.is_some()
                || o.take_profit.is_some()
                || o.close == Some(true)
                || (market.limits.amount.max > 0. && o.amount > market.limits.amount.max))
        {
            return Err("Paradex cannot honor this caller client order ID exactly once".into());
        }
    }
    Ok(())
}
fn place_operations(
    orders: &[PlaceOrderOpts],
    cx: &Cx<'_, Pending>,
    account: &Account,
    quotes: &HashMap<String, Quote>,
    bands: &HashMap<String, f64>,
    require_quotes: bool,
) -> Result<VecDeque<Operation>, String> {
    if orders
        .iter()
        .any(|o| o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some())
    {
        return Err("Bybit BBO and TickSize controls are not supported by Paradex".into());
    }
    if orders.iter().any(|o| o.stp_mode.is_some()) {
        return Err("Caller STP controls are not supported by Paradex".into());
    }
    validate_caller_ids(orders, cx)?;
    let signer = account
        .credentials()
        .ok_or("Paradex signer is not initialized")?;
    let mut operations = VecDeque::new();
    let mut chunk = Vec::new();
    let flush =
        |chunk: &mut Vec<Placement>, operations: &mut VecDeque<Operation>| -> Result<(), String> {
            if chunk.is_empty() {
                return Ok(());
            }
            let expected = chunk.iter().map(|p| p.client_id.clone()).collect();
            let receipts = chunk.iter().filter_map(placement_receipt).collect();
            let (body, step) = signed(
                account,
                signer,
                Method::Post,
                "/orders/batch",
                std::mem::take(chunk),
                true,
            )?;
            operations.push_back(Operation {
                receipts,
                hash: Some(step),
                method: Method::Post,
                path: "/orders/batch".into(),
                body: Some(body),
                kind: Kind::Place {
                    expected,
                    leg: None,
                    batch: true,
                    replaces: None,
                },
            });
            Ok(())
        };
    orders.iter().try_for_each(bracket_sides)?;
    for o in orders {
        let execution = if capped(o) {
            if !o.max_slippage_pct.is_some_and(f64::is_finite)
                || !matches!(o.time_in_force, None | Some(TimeInForce::ImmediateOrCancel))
            {
                return Err(
                    "Paradex capped market orders require finite slippage and IOC time-in-force"
                        .into(),
                );
            }
            match quotes.get(&o.symbol) {
                Some(quote) => Some(capped_price(o, *quote, bands.get(&o.symbol).copied(), cx)?),
                None if require_quotes => {
                    return Err("Paradex capped market quote is unavailable".into());
                }
                None => None,
            }
        } else {
            None
        };
        if native_batch(o) {
            chunk.push(prepare(cx, o, false, execution)?);
            if chunk.len() == 10 {
                flush(&mut chunk, &mut operations)?;
            }
            continue;
        }
        flush(&mut chunk, &mut operations)?;
        let payload = prepare(cx, o, false, execution)?;
        let expected = vec![payload.client_id.clone()];
        let receipts = placement_receipt(&payload).into_iter().collect();
        let (body, step) = signed(
            account,
            signer,
            Method::Post,
            "/orders",
            vec![payload],
            false,
        )?;
        operations.push_back(Operation {
            receipts,
            hash: Some(step),
            method: Method::Post,
            path: "/orders".into(),
            body: Some(body),
            kind: Kind::Place {
                expected,
                leg: None,
                batch: false,
                replaces: None,
            },
        });
        for (kind, leg, price) in [
            (OrderType::TakeProfit, "take-profit", o.take_profit),
            (OrderType::StopLoss, "stop-loss", o.stop_loss),
        ] {
            if let Some(price) = price {
                if !positive(price) {
                    return Err("Paradex bracket price must be finite and positive".into());
                }
                let mut child = o.clone();
                child.kind = kind;
                child.side = o.side.opposite();
                child.price = Some(price);
                child.trigger_price = None;
                child.client_order_id = None;
                child.take_profit = None;
                child.stop_loss = None;
                child.reduce_only = Some(true);
                child.time_in_force = None;
                let mut payload = prepare(cx, &child, true, None)?;
                if kind == OrderType::StopLoss {
                    payload.kind = "STOP_LOSS_MARKET";
                }
                let (body, step) = signed(
                    account,
                    signer,
                    Method::Post,
                    "/orders",
                    vec![payload],
                    false,
                )?;
                operations.push_back(Operation {
                    receipts: Vec::new(),
                    hash: Some(step),
                    method: Method::Post,
                    path: "/orders".into(),
                    body: Some(body),
                    kind: Kind::Place {
                        expected: vec![None],
                        leg: Some(leg),
                        batch: false,
                        replaces: None,
                    },
                });
            }
        }
    }
    flush(&mut chunk, &mut operations)?;
    Ok(operations)
}

fn local_refusal(cx: &mut Cx<'_, Pending>, operation: HttpOperation, error: VenueError) {
    if let Ok(context) = cx.admit_http_command(Policy {
        operation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) {
        let _ = cx.fail_context(&context, error);
    }
}

impl Trading {
    pub fn command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        command: Command,
        price_bands: &HashMap<String, f64>,
    ) -> bool {
        let local_operation = match &command {
            Command::FetchStpMode { .. }
            | Command::FetchAccount { .. }
            | Command::FetchOrderBook { .. }
            | Command::FetchLeverage { .. } => HttpOperation::Read,
            _ => HttpOperation::Mutation,
        };
        if let Command::CancelOrdersWithSymbols { .. }
        | Command::FetchStpMode { .. }
        | Command::SetStpMode { .. }
        | Command::FetchAccount { .. }
        | Command::FetchOrderBook { .. } = &command
        {
            local_refusal(
                cx,
                local_operation,
                not_sent("This query or control is not supported by Paradex")
                    .kind(ErrorKind::Unsupported),
            );
            return true;
        }
        if let Command::FetchLeverage { symbol, .. } = &command {
            self.fetch_leverage(cx, account, symbol);
            return true;
        }
        if let Command::PlaceOrders { command, orders } = &command
            && orders.iter().any(capped)
        {
            if let Err(error) =
                place_operations(orders, cx, account, &HashMap::new(), price_bands, false)
            {
                cx.fail(*command, not_sent(error));
                return true;
            }
            let mut seen = HashSet::new();
            let symbols = orders
                .iter()
                .filter(|o| capped(o) && seen.insert(o.symbol.clone()))
                .map(|o| o.symbol.clone())
                .collect();
            self.next = self.next.wrapping_add(1);
            let job = self.next;
            self.jobs.insert(
                job,
                Job {
                    command: *command,
                    preparing: Some(Preparation {
                        orders: orders.clone(),
                        symbols,
                        quotes: HashMap::new(),
                        price_bands: price_bands.clone(),
                    }),
                    operations: VecDeque::new(),
                    results: vec![],
                    errors: vec![],
                    leg_refused: false,
                    attempt_all: true,
                    placement: true,
                    effective: false,
                    restore: None,
                    restoring: None,
                },
            );
            self.advance(cx, account, job);
            return true;
        }
        let mut restore = None;
        let mut amended = None;
        // A cancel of a stop being replaced must not see it put back afterwards.
        let (cancelled_ids, cancelled_symbol) = match &command {
            Command::CancelOrders { ids, .. } => (ids.clone(), None),
            Command::CancelSymbolOrders { symbol, .. } => (Vec::new(), Some(symbol.clone())),
            _ => (Vec::new(), None),
        };
        for (id, job) in &self.replacing {
            let symbol = account
                .native_order(id)
                .map(|raw| raw.market.as_str())
                .or_else(|| {
                    self.cancel_proofs
                        .get(job)
                        .map(|proof| proof.guard.symbol.as_str())
                });
            if (cancelled_ids.contains(id)
                || (cancelled_symbol.is_some() && symbol == cancelled_symbol.as_deref()))
                && let Some(state) = self.jobs.get_mut(job)
            {
                state.restore = None;
            }
        }
        let followed: Vec<String> = self
            .restored
            .iter()
            .filter(|(old, _, symbol)| {
                cancelled_ids.contains(old) || cancelled_symbol.as_deref() == Some(symbol.as_str())
            })
            .map(|(_, new, _)| new.clone())
            .collect();
        self.restored.retain(|(_, new, _)| !followed.contains(new));
        let (command, operations, attempt_all, placement) = match command {
            Command::PlaceOrders { command, orders } => (
                command,
                place_operations(&orders, cx, account, &HashMap::new(), price_bands, true),
                true,
                true,
            ),
            Command::CancelOrders { command, mut ids } => {
                ids.extend(
                    followed
                        .iter()
                        .filter(|new| !ids.contains(new))
                        .cloned()
                        .collect::<Vec<_>>(),
                );
                let mut seen = HashSet::new();
                let result = if ids
                    .iter()
                    .any(|id| id.trim().is_empty() || !seen.insert(id))
                {
                    Err("Paradex cancellation requires unique nonempty IDs".into())
                } else {
                    let mut operations = VecDeque::new();
                    for chunk in ids.chunks(2) {
                        operations.push_back(Operation {
                            receipts: Vec::new(),
                            hash: None,
                            method: Method::Delete,
                            path: if chunk.len() == 2 {
                                "/orders/batch".into()
                            } else {
                                format!("/orders/{}", encode(&chunk[0]))
                            },
                            body: if chunk.len() == 2 {
                                #[derive(Serialize)]
                                struct Cancel<'a> {
                                    order_ids: &'a [String],
                                }
                                Some(serde_json::to_string(&Cancel { order_ids: chunk }).unwrap())
                            } else {
                                None
                            },
                            kind: Kind::Cancel(chunk.to_vec()),
                        });
                    }
                    Ok(operations)
                };
                (command, result, true, false)
            }
            Command::CancelSymbolOrders { command, symbol } => (
                command,
                cx.store
                    .market(&symbol)
                    .ok_or_else(|| "Paradex cancel-all requires a known nonempty market".to_owned())
                    .map(|market| {
                        // The venue's cancel-all leaves untriggered stops, which can still
                        // fire; only a delete by id clears them.
                        let mut operations: VecDeque<_> = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|order| {
                                order.symbol == symbol
                                    && matches!(
                                        order.kind,
                                        OrderType::StopLoss
                                            | OrderType::TakeProfit
                                            | OrderType::Conditional
                                    )
                            })
                            .map(|order| order.id.clone())
                            .chain(followed.iter().cloned())
                            .fold(Vec::new(), |mut ids, id| {
                                if !ids.contains(&id) {
                                    ids.push(id);
                                }
                                ids
                            })
                            .into_iter()
                            .map(|id| Operation {
                                receipts: Vec::new(),
                                hash: None,
                                method: Method::Delete,
                                path: format!("/orders/{}", encode(&id)),
                                body: None,
                                kind: Kind::Cancel(vec![id]),
                            })
                            .collect();
                        operations.push_back(Operation {
                            receipts: Vec::new(),
                            hash: None,
                            method: Method::Delete,
                            path: format!("/orders?market={}", encode(&market.id)),
                            body: None,
                            kind: Kind::Empty,
                        });
                        operations
                    }),
                // A failed stop delete must not skip the sweep of orders the store never saw.
                true,
                false,
            ),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                let result = (|| {
                    if !leverage.is_finite() {
                        return Err("Paradex leverage must be finite".into());
                    }
                    let m = cx.store.market(&symbol).ok_or("unknown Paradex market")?;
                    if !m.limits.leverage.min.is_finite() || !m.limits.leverage.max.is_finite() {
                        return Err("Paradex leverage limits must be finite".into());
                    }
                    let min = m.limits.leverage.min.max(1.).ceil();
                    let max = m.limits.leverage.max.max(1.).floor();
                    if !min.is_finite()
                        || !max.is_finite()
                        || min > max
                        || max > f64::from(u32::MAX)
                    {
                        return Err(
                            "Paradex leverage limits must contain a supported integer".into()
                        );
                    }
                    let leverage = leverage.floor().clamp(min, max);
                    #[derive(Serialize)]
                    struct Leverage {
                        leverage: u32,
                        margin_type: &'static str,
                    }
                    Ok(VecDeque::from([Operation {
                        receipts: Vec::new(),
                        hash: None,
                        method: Method::Post,
                        path: format!("/account/margin/{}", encode(&symbol)),
                        body: Some(
                            serde_json::to_string(&Leverage {
                                leverage: leverage as u32,
                                margin_type: "CROSS",
                            })
                            .unwrap(),
                        ),
                        kind: Kind::Leverage { symbol, leverage },
                    }]))
                })();
                (command, result, false, false)
            }
            Command::SetMarginType { isolated, .. } => {
                if isolated {
                    local_refusal(
                        cx,
                        HttpOperation::Mutation,
                        not_sent("Paradex isolated account routing is unsupported")
                            .kind(ErrorKind::Unsupported),
                    );
                } else {
                    let _ = cx.done_local_setting("[]".into(), |_| {});
                }
                return true;
            }
            Command::SetPositionMode { hedged, .. } => {
                if hedged {
                    local_refusal(
                        cx,
                        HttpOperation::Mutation,
                        not_sent("Paradex supports one-way mode only").kind(ErrorKind::Unsupported),
                    );
                } else {
                    let _ = cx.done_local_setting("[]".into(), |_| {});
                }
                return true;
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (
                command,
                self.amend(cx, account, &id, price, amount)
                    .map(|(operations, put_back)| {
                        if put_back.is_some() {
                            amended = Some(id.clone());
                        }
                        restore = put_back;
                        operations
                    }),
                false,
                true,
            ),
            _ => return false,
        };
        if account.credentials().is_none() {
            local_refusal(
                cx,
                local_operation,
                not_sent("Paradex private account is not initialized"),
            );
            return true;
        }
        match operations {
            Ok(operations) => {
                self.next = self.next.wrapping_add(1);
                let job = self.next;
                self.jobs.insert(
                    job,
                    Job {
                        command,
                        preparing: None,
                        operations,
                        results: Vec::new(),
                        errors: Vec::new(),
                        leg_refused: false,
                        attempt_all,
                        placement,
                        effective: false,
                        restore,
                        restoring: None,
                    },
                );
                if let Some(id) = amended {
                    // A second edit of this stop waits until this one settles.
                    self.amend_locks.insert(id.clone());
                    self.replacing.insert(id, job);
                }
                self.advance(cx, account, job);
            }
            Err(error) => cx.fail(command, not_sent(error)),
        }
        true
    }
    fn advance(&mut self, cx: &mut Cx<'_, Pending>, account: &mut Account, job: u64) {
        if let Some(preparation) = self
            .jobs
            .get_mut(&job)
            .and_then(|state| state.preparing.as_mut())
        {
            if let Some(symbol) = preparation.symbols.pop_front() {
                cx.request(
                    Method::Get,
                    format!("{}/orderbook/{}?depth=5", account.rest(), encode(&symbol)),
                    Vec::new(),
                    None,
                    Pending::Trade(Request {
                        progress: None,
                        hash: None,
                        job,
                        kind: Kind::Quotes(symbol),
                    }),
                );
                return;
            }
            let preparation = self
                .jobs
                .get_mut(&job)
                .and_then(|state| state.preparing.take())
                .expect("pending plan");
            match place_operations(
                &preparation.orders,
                cx,
                account,
                &preparation.quotes,
                &preparation.price_bands,
                true,
            ) {
                Ok(operations) => {
                    self.jobs.get_mut(&job).expect("pending job").operations = operations
                }
                Err(error) => {
                    let state = self.jobs.remove(&job).expect("pending job");
                    cx.fail(state.command, not_sent(error));
                    return;
                }
            }
        }
        let Some(state) = self.jobs.get_mut(&job) else {
            return;
        };
        if self.cancel_proofs.contains_key(&job)
            && !state
                .operations
                .front()
                .is_some_and(|operation| matches!(operation.kind, Kind::CancelProof))
        {
            return;
        }
        if let Some(operation) = state.operations.pop_front() {
            if let Kind::Amend { id, target } = &operation.kind {
                if let Err(error) = account.begin_amend(id, (**target).clone()) {
                    // An amend job has a single operation, so nothing took effect yet.
                    let state = self.jobs.remove(&job).expect("pending amendment");
                    cx.fail(state.command, not_sent(error));
                    return;
                }
                self.amend_locks.insert(id.clone());
            }
            if !matches!(operation.kind, Kind::CancelProof) {
                account.bump_orders(cx);
                account.bump_positions();
            }
            account.request(
                cx,
                operation.method,
                operation.path,
                operation.body,
                None,
                Pending::Trade(Request {
                    progress: (!operation.receipts.is_empty())
                        .then_some((state.command, operation.receipts)),
                    hash: operation.hash,
                    job,
                    kind: operation.kind,
                }),
            );
        } else {
            let state = self.jobs.remove(&job).unwrap();
            let amend_locks = &mut self.amend_locks;
            self.replacing.retain(|id, held| {
                let keep = *held != job;
                if !keep {
                    amend_locks.remove(id);
                }
                keep
            });
            // An accepted placement reports success only when no leg's outcome is
            // unknown; a lost request may have taken effect.
            let unknown = state
                .errors
                .iter()
                .any(|e| e.outcome == Some(Outcome::Unknown));
            if state.errors.is_empty()
                || (state.placement && !state.results.is_empty() && !unknown && !state.leg_refused)
            {
                let result = if state.placement {
                    serde_json::to_string(&state.results).map_err(|e| e.to_string())
                } else {
                    Ok("[]".into())
                };
                match result {
                    Ok(data) => cx.done(state.command, data),
                    Err(error) => cx.fail(state.command, error),
                };
            } else {
                let command = state.command;
                cx.fail(command, state.failure());
            }
        }
    }
    fn begin_cancel_proof(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        job: u64,
        guard: Box<StopGuard>,
    ) {
        let expires = cx.set_timer(CANCEL_PROOF_MS);
        self.cancel_proofs.insert(
            job,
            CancelProof {
                guard,
                deadline: cx.now + CANCEL_PROOF_MS,
                expires,
                retry: None,
                polls: 0,
            },
        );
        self.poll_cancel_proof(cx, account, job);
    }
    fn poll_cancel_proof(&mut self, cx: &mut Cx<'_, Pending>, account: &mut Account, job: u64) {
        let Some(proof) = self.cancel_proofs.get_mut(&job) else {
            return;
        };
        if !cx.now.is_finite() || cx.now >= proof.deadline || proof.polls >= CANCEL_PROOF_POLLS {
            self.fail_cancel_proof(cx, account, job, "target cancellation was not confirmed");
            return;
        }
        proof.polls += 1;
        let raw = &proof.guard.raw;
        let start_at = raw
            .created_at
            .parse::<u64>()
            .unwrap_or(0)
            .saturating_sub(CANCEL_PROOF_WINDOW_MARGIN_MS);
        let mut path = format!(
            "/orders-history?market={}&status=CLOSED&page_size=100&start_at={}&end_at={}",
            encode(&raw.market),
            start_at,
            num::format((cx.protocol_now() + CANCEL_PROOF_WINDOW_MARGIN_MS as f64).floor())
        );
        if !raw.client_id.is_empty() {
            path.push_str(&format!("&client_id={}", encode(&raw.client_id)));
        }
        let Some(state) = self.jobs.get_mut(&job) else {
            return;
        };
        state.operations.push_front(Operation {
            receipts: Vec::new(),
            hash: None,
            method: Method::Get,
            path,
            body: None,
            kind: Kind::CancelProof,
        });
        self.advance(cx, account, job);
    }
    fn clear_cancel_proof(&mut self, cx: &mut Cx<'_, Pending>, job: u64) {
        if let Some(proof) = self.cancel_proofs.remove(&job) {
            cx.clear_timer(proof.expires);
            if let Some(timer) = proof.retry {
                cx.clear_timer(timer);
            }
        }
    }
    fn fail_cancel_proof(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        job: u64,
        cause: &str,
    ) {
        let Some(proof) = self.cancel_proofs.get(&job) else {
            return;
        };
        let symbol = &proof.guard.symbol;
        let mut error = unproven(format!(
            "The outcome of your {symbol} protection update is unknown. Check Open Orders and your position before replacing protection: {cause}"
        ));
        error.presents = true;
        self.clear_cancel_proof(cx, job);
        if let Some(state) = self.jobs.get_mut(&job) {
            state.operations.clear();
            state.restore = None;
            state.errors.push(error);
            self.advance(cx, account, job);
        }
    }
    fn cancel_proof_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        job: u64,
        response: Response<'_>,
    ) {
        let Some(proof) = self.cancel_proofs.get(&job) else {
            return;
        };
        if !cx.now.is_finite() || cx.now >= proof.deadline {
            self.fail_cancel_proof(cx, account, job, "target cancellation proof expired");
            return;
        }
        if crate::success(response).is_err() {
            self.fail_cancel_proof(cx, account, job, "target cancellation read failed");
            return;
        }
        if cancel_proven(response.body, &proof.guard.raw)
            && proof.guard.prices_safe(cx.store, cx.now).is_ok()
            && account.native_order(&proof.guard.raw.id).is_none_or(|row| {
                row.size == proof.guard.raw.size
                    && row.remaining_size == proof.guard.raw.remaining_size
                    && row.trigger_price == proof.guard.raw.trigger_price
                    && row.price == proof.guard.raw.price
            })
            && cx.store.order(&proof.guard.raw.id).is_none_or(|row| {
                row.filled == 0. && row.status != safe_cex_engine::model::OrderStatus::Closed
            })
            && self
                .jobs
                .get(&job)
                .is_some_and(|state| state.restore.is_some())
        {
            self.clear_cancel_proof(cx, job);
            self.advance(cx, account, job);
            return;
        }
        if proof.polls >= CANCEL_PROOF_POLLS {
            self.fail_cancel_proof(
                cx,
                account,
                job,
                "target cancellation was not confirmed unfilled",
            );
            return;
        }
        let timer = cx.set_timer(1_000.);
        self.cancel_proofs.get_mut(&job).unwrap().retry = Some(timer);
    }
    pub(crate) fn on_timer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        timer: TimerId,
    ) -> bool {
        let Some((&job, proof)) = self
            .cancel_proofs
            .iter()
            .find(|(_, proof)| proof.expires == timer || proof.retry == Some(timer))
        else {
            return false;
        };
        if proof.expires == timer {
            self.fail_cancel_proof(cx, account, job, "target cancellation proof timed out");
        } else {
            self.cancel_proofs.get_mut(&job).unwrap().retry = None;
            self.poll_cancel_proof(cx, account, job);
        }
        true
    }
    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        request: Request,
        response: Response<'_>,
    ) {
        if matches!(request.kind, Kind::CancelProof) {
            self.cancel_proof_response(cx, account, request.job, response);
            return;
        }
        if let Kind::GuardedCancel(guard) = &request.kind
            && ((200..300).contains(&response.status) && crate::success(response).is_ok()
                || response.status == 400 && response.body.contains("ORDER_ID_NOT_FOUND"))
        {
            self.begin_cancel_proof(cx, account, request.job, guard.clone());
            return;
        }
        if let Kind::Quotes(symbol) = &request.kind {
            let result = crate::success(response).and_then(|()| decode_quote(response.body));
            match result {
                Ok(mut quote) => {
                    quote.fair = snapshot_fair(cx, symbol);
                    if let Some(preparation) = self
                        .jobs
                        .get_mut(&request.job)
                        .and_then(|state| state.preparing.as_mut())
                    {
                        preparation.quotes.insert(symbol.clone(), quote);
                    }
                    self.advance(cx, account, request.job);
                }
                // A read before any write: nothing was sent.
                Err(error) => {
                    let error = VenueError {
                        outcome: Some(Outcome::NotSent),
                        ..answer_failure(response.status, error)
                    };
                    self.complete(cx, account, request, Err(error), false)
                }
            }
            return;
        }
        if let Kind::Place {
            expected, batch, ..
        } = &request.kind
        {
            let result = crate::success(response)
                .map_err(|error| write_failure(response.status, error))
                .and_then(|()| decode_placement(response.body, expected, *batch).map_err(unproven));
            let result = match result {
                Ok(PlacementResult {
                    ids,
                    error: Some(error),
                }) => {
                    if let Some(job) = self.jobs.get_mut(&request.job) {
                        job.results.extend(ids);
                    }
                    // Explicit per-order refusals without a documented code table.
                    Err(VenueError::new(error))
                }
                Ok(PlacementResult { ids, error: None }) => Ok(ids),
                Err(error) => Err(error),
            };
            self.complete(cx, account, request, result, false);
            return;
        }
        // Plain cancel idempotency grants no replacement authority.
        let kind = &request.kind;
        let already_gone = matches!(kind, Kind::Cancel(ids) if ids.len() == 1)
            && response.status == 400
            && response.body.contains("ORDER_ID_NOT_FOUND");
        let result = crate::success(response).and_then(|()| match &request.kind {
            Kind::Amend { id, .. } => {
                let row: Identity = serde_json::from_str(response.body)
                    .map_err(|_| "invalid Paradex amend response")?;
                if row.id != *id {
                    Err("Paradex amend did not confirm stable order ID".into())
                } else {
                    Ok(vec![id.clone()])
                }
            }
            Kind::Cancel(ids) if ids.len() == 2 => {
                decode_cancels(response.body, ids).map(|()| Vec::new())
            }
            Kind::Leverage { symbol, leverage } => {
                account.set_leverage(cx, symbol, *leverage);
                // Frame-fed: keep this write's leverage until the source's frames agree.
                cx.hold_setting(safe_cex_engine::setting_holds::SettingKey::Leverage(
                    symbol.clone(),
                ));
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        });
        let result = if already_gone {
            Ok(Vec::new())
        } else {
            result.map_err(|error| match crate::success(response) {
                Err(_) => write_failure(response.status, error),
                Ok(()) => unproven(error),
            })
        };
        if result.is_ok() && matches!(request.kind, Kind::Amend { .. }) {
            account.amend_ack(cx, response.body);
        }
        let rejected = ((400..500).contains(&response.status)
            && !matches!(response.status, 408 | 429))
            || ((200..300).contains(&response.status) && crate::success(response).is_err());
        self.complete(cx, account, request, result, rejected && !already_gone);
    }
    pub fn on_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        request: Request,
        error: &str,
    ) {
        if matches!(request.kind, Kind::CancelProof) {
            self.fail_cancel_proof(cx, account, request.job, "target cancellation read failed");
            return;
        }
        let error = VenueError::new(error).kind(ErrorKind::Network);
        let error = match request.kind {
            // Reads: the quote read precedes the placement, which was never sent.
            Kind::Quotes(_) => error.outcome(Outcome::NotSent),
            _ => error.outcome(Outcome::Unknown),
        };
        self.complete(cx, account, request, Err(error), false);
    }
    /// Failed before it was sent: authentication or signing never completed.
    /// Refused before transport with the account's own typed reason.
    pub fn on_refused(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        request: Request,
        error: VenueError,
    ) {
        if matches!(request.kind, Kind::CancelProof) {
            self.fail_cancel_proof(cx, account, request.job, "target cancellation read failed");
            return;
        }
        self.complete(
            cx,
            account,
            request,
            Err(error.outcome(Outcome::NotSent)),
            true,
        );
    }
    pub fn on_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        request: Request,
        error: &str,
    ) {
        if matches!(request.kind, Kind::CancelProof) {
            self.fail_cancel_proof(cx, account, request.job, "target cancellation read failed");
            return;
        }
        self.complete(cx, account, request, Err(not_sent(error)), true);
    }
    fn complete(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        request: Request,
        result: Result<Vec<String>, VenueError>,
        rejected: bool,
    ) {
        let kind = &request.kind;
        let is_quotes = matches!(kind, Kind::Quotes(_));
        if !is_quotes {
            account.bump_orders(cx);
            account.bump_positions();
        }
        if let Kind::Amend { id, .. } = &request.kind {
            self.amend_locks.remove(id);
            account.finish_amend(id, cx.now, rejected);
        }
        let Some(job) = self.jobs.get_mut(&request.job) else {
            return;
        };
        match result {
            Ok(ids) => {
                if !is_quotes {
                    job.effective = true;
                }
                job.results.extend(ids);
                if matches!(
                    request.kind,
                    Kind::Place {
                        leg: Some(STOP_RESTORE),
                        ..
                    }
                ) && let Some((symbol, ..)) = &job.restoring
                {
                    if let (Some(new), Some((old, _))) = (
                        job.results.last(),
                        self.replacing
                            .iter()
                            .find(|(_, held)| **held == request.job),
                    ) {
                        if self.restored.len() >= RESTORED_KEPT {
                            self.restored.pop_front();
                        }
                        self.restored
                            .push_back((old.clone(), new.clone(), symbol.clone()));
                    }
                    let mut error = VenueError::new(format!(
                        "Your {symbol} stop is unchanged: its replacement was refused, so the old stop was put back as a new order"
                    ));
                    error.protection = Some(safe_cex_engine::error::Protection::Restored);
                    error.presents = true;
                    job.errors = vec![error];
                }
            }
            Err(mut error) => {
                if error.outcome == Some(Outcome::Unknown) {
                    error = ambiguous_write(error);
                }
                let gone_stop = match &request.kind {
                    Kind::Place {
                        replaces: Some(stop),
                        ..
                    } => {
                        // Only a definite refusal proves no replacement exists.
                        if error.outcome == Some(Outcome::Rejected)
                            && let Some(restore) = job.restore.take()
                        {
                            job.operations.clear();
                            job.operations.push_front(restore);
                            job.restoring = Some((**stop).clone());
                            None
                        } else {
                            Some((**stop).clone())
                        }
                    }
                    Kind::Place {
                        leg: Some(STOP_RESTORE),
                        ..
                    } => job.restoring.take(),
                    _ => None,
                };
                if let Some((symbol, side, kind)) = gone_stop {
                    // Only a definite refusal proves the order absent; a 429 may have placed it.
                    let unknown = error.outcome != Some(Outcome::Rejected);
                    let gone = safe_cex_engine::error::stop_gone_text(&symbol, side, kind, unknown);
                    error.message = if error.message.is_empty() {
                        gone
                    } else {
                        format!("{gone}: {}", error.message)
                    };
                    error.protection =
                        (!unknown).then_some(safe_cex_engine::error::Protection::Unprotected);
                    error.presents = true;
                    if matches!(
                        request.kind,
                        Kind::Place {
                            leg: Some(STOP_RESTORE),
                            ..
                        }
                    ) {
                        // The restore's words replace the refused replacement's.
                        job.errors.clear();
                    }
                }
                if let Kind::Place {
                    leg,
                    expected,
                    batch,
                    ..
                } = &request.kind
                {
                    let unknown = error.outcome == Some(Outcome::Unknown);
                    if unknown && job.placement {
                        let unknown_labels = request.kind.placement_labels();
                        let journal = serde_json::json!({
                            "placed":job.results,
                            "unknown":unknown_labels,
                            "notSent":job.operations.iter().flat_map(|op| op.kind.placement_labels()).collect::<Vec<_>>()
                        });
                        error.message = format!(
                            "{}; placed order IDs: {}; unknown placements: {}",
                            error.message, journal["placed"], journal["unknown"]
                        );
                        cx.event(
                            Level::Error,
                            "order.outcome_unknown",
                            error.message.clone(),
                            vec![
                                ("command", job.command.into()),
                                ("outcome", "unknown".into()),
                                ("journal", journal.to_string().into()),
                            ],
                        );
                    }
                    let message = match leg {
                        Some(leg) => format!("Paradex TP/SL {leg} placement failed: {error}"),
                        None => {
                            format!("Paradex placement failed for client IDs {expected:?}: {error}")
                        }
                    };
                    let code = if leg.is_some() {
                        code::CHILD_LEG_FAILED
                    } else {
                        code::ORDER_REJECTED
                    };
                    // TS awaits batch flushes outside its serial-entry catch. A bracket's
                    // TP and SL share one try, so either failure skips its remaining children.
                    // Sean/daxx's Unknown exception also withholds independent later entries.
                    if *batch || error.outcome == Some(Outcome::Unknown) {
                        job.operations.clear();
                    } else {
                        while job.operations.front().is_some_and(|op| {
                            matches!(
                                op.kind,
                                Kind::Place {
                                    leg: Some("take-profit" | "stop-loss"),
                                    ..
                                }
                            )
                        }) {
                            job.operations.pop_front();
                        }
                    }
                    if leg.is_some() || *batch {
                        job.leg_refused = true;
                    }
                    job.errors.push(error);
                    // Only when the command fails on this entry does its caller name it; a toast
                    // would double that. Beside an accepted order, this line is the only report.
                    let fails_command =
                        leg.is_none() && job.results.is_empty() && job.operations.is_empty();
                    let level = if fails_command {
                        Level::Warn
                    } else {
                        Level::Error
                    };
                    if !unknown || !job.placement {
                        cx.notice(level, code, message);
                    }
                } else {
                    job.errors.push(error);
                    if !job.attempt_all || is_quotes {
                        job.preparing = None;
                        job.operations.clear();
                    }
                }
            }
        }
        self.advance(cx, account, request.job);
    }
    /// A command Read on the JWT Session; it never enters the write queue.
    fn fetch_leverage(&mut self, cx: &mut Cx<'_, Pending>, account: &mut Account, symbol: &str) {
        let Ok(context) = cx.admit_http_command(Policy {
            operation: HttpOperation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        if account.credentials().is_none() {
            let _ = cx.fail_context(
                &context,
                not_sent("Paradex private account is not initialized"),
            );
            return;
        }
        account.read(
            cx,
            context.clone(),
            "/account/margin",
            None,
            Pending::Leverage(context, symbol.to_owned()),
        );
    }
    pub fn leverage_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        context: &Context,
        symbol: &str,
        response: Response<'_>,
    ) {
        #[derive(Deserialize)]
        struct Row {
            market: String,
            leverage: f64,
        }
        #[derive(Deserialize)]
        struct Margins {
            configs: Vec<Row>,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Rails {
            buy_leverage: f64,
            sell_leverage: f64,
        }
        let margins = crate::success(response)
            .map_err(|error| answer_failure(response.status, error))
            .and_then(|()| {
                serde_json::from_str::<Margins>(response.body)
                    .map_err(|_| VenueError::new("invalid Paradex margin settings"))
            });
        // A read: no write outcome to classify.
        let margins = match margins {
            Ok(margins) => margins,
            Err(error) => {
                let _ = cx.fail_context(
                    context,
                    VenueError {
                        outcome: None,
                        ..error
                    },
                );
                return;
            }
        };
        let leverage = margins
            .configs
            .iter()
            .find(|m| m.market == symbol)
            .map(|m| m.leverage)
            .filter(|v| positive(*v))
            .unwrap_or(10.);
        account.set_leverage(cx, symbol, leverage);
        let leverage = cx.store.setting(symbol).map_or(10., |s| s.leverage);
        match serde_json::to_string(&Rails {
            buy_leverage: leverage,
            sell_leverage: leverage,
        }) {
            Ok(data) => {
                let _ = cx.done_context(context, data);
            }
            Err(error) => {
                let _ = cx.fail_context(context, VenueError::new(error.to_string()));
            }
        }
    }
    fn amend(
        &self,
        cx: &Cx<'_, Pending>,
        account: &Account,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<(VecDeque<Operation>, Option<Operation>), String> {
        if self.amend_locks.contains(id) {
            return Err("Paradex amendment is awaiting confirmation and reconciliation".into());
        }
        let raw = account
            .native_order(id)
            .ok_or("Paradex amend requires exactly reconciled native order")?;
        let order = cx
            .store
            .orders()
            .iter()
            .find(|o| o.id == id)
            .ok_or("Paradex amend requires open order")?;
        let limit = raw.kind == "LIMIT" && matches!(raw.status.as_str(), "NEW" | "OPEN");
        let trigger = is_trigger(&raw.kind) && raw.status == "UNTRIGGERED";
        if (!limit && !trigger)
            || !raw.on_behalf_of_account.is_empty()
            || raw.market != order.symbol
            || raw.side != side(order.side)
        {
            return Err("Paradex amend shape or isolated routing is unsupported".into());
        }
        let mapped = crate::account::map_order(raw)?;
        if mapped.kind != order.kind
            || mapped.reduce_only != order.reduce_only
            || (raw.reduce_only.is_none() && raw.flags.is_none())
            || (!matches!(
                raw.instruction.as_str(),
                "GTC" | "IOC" | "POST_ONLY" | "RPI"
            ) && raw.post_only.is_none()
                && !matches!(raw.time_in_force.as_str(), "GTT" | "FOK" | "IOC"))
        {
            return Err("Paradex amend immutable attributes are not reconciled".into());
        }
        let m = cx
            .store
            .market(&raw.market)
            .ok_or("unknown Paradex amend market")?;
        let size = raw
            .size
            .parse::<f64>()
            .map_err(|_| "invalid Paradex size")?;
        let remaining = raw
            .remaining_size
            .parse::<f64>()
            .map_err(|_| "invalid Paradex remaining size")?;
        if size <= 0.
            || remaining < 0.
            || remaining > size
            || size != order.amount
            || remaining != order.remaining
            || size - remaining != order.filled
        {
            return Err("Paradex amend size is not reconciled".into());
        }
        let raw_price = if trigger && raw.price.is_empty() {
            0.
        } else {
            raw.price
                .parse::<f64>()
                .map_err(|_| "invalid Paradex execution price")?
        };
        let raw_trigger = if trigger {
            raw.trigger_price
                .parse::<f64>()
                .map_err(|_| "invalid Paradex trigger price")?
        } else {
            0.
        };
        if !size.is_finite()
            || !remaining.is_finite()
            || !raw_price.is_finite()
            || raw_price < 0.
            || (limit && (raw_price <= 0. || raw_price != order.price))
            || (trigger && (!positive(raw_trigger) || raw_trigger != order.price))
        {
            return Err("Paradex amend existing prices or sizes are not reconciled".into());
        }
        let update_price = price;
        let size = num::adjust(amount.unwrap_or(size), m.precision.amount);
        let price = if limit {
            num::adjust(price.unwrap_or(raw_price), m.precision.price)
        } else if raw.kind.ends_with("_MARKET") {
            // The venue refuses a price here, and a row can carry one.
            0.
        } else {
            raw_price
        };
        let trigger_price =
            trigger.then(|| num::adjust(update_price.unwrap_or(raw_trigger), m.precision.price));
        if !positive(size)
            || size <= order.filled
            || !price.is_finite()
            || price < 0.
            || (limit && price <= 0.)
            || trigger_price.is_some_and(|p| !positive(p))
        {
            return Err("Paradex amendment size or price invalid".into());
        }
        if trigger {
            return replace_trigger(cx, account, raw, order, id, size, price, trigger_price);
        }
        if id.is_empty() {
            return Err("Cannot sign a Paradex amendment without an order ID".into());
        }
        let signer = account
            .credentials()
            .ok_or("Paradex signer is not initialized")?;
        let path = format!("/orders/{}", encode(id));
        let (body, step) = signed(
            account,
            signer,
            Method::Put,
            &path,
            vec![Modify {
                id: id.into(),
                market: raw.market.clone(),
                side: raw.side.clone(),
                kind: raw.kind.clone(),
                size: num::format(size),
                price: num::format(price),
                trigger_price: trigger_price.map(num::format),
                signature: String::new(),
                signature_timestamp: 0,
            }],
            false,
        )?;
        Ok((
            VecDeque::from([Operation {
                receipts: Vec::new(),
                hash: Some(step),
                method: Method::Put,
                path,
                body: Some(body),
                kind: Kind::Amend {
                    id: id.into(),
                    target: Box::new(AmendTarget::new(
                        raw,
                        &num::format(size),
                        &num::format(price),
                        trigger_price.map(num::format).as_deref(),
                    )?),
                },
            }]),
            None,
        ))
    }
}
/// Live Paradex answers ORDER_IS_NOT_OPEN to a native modify of an untriggered
/// stop (#2770, 2026-11-02, despite its docs), so the stop is cancelled and placed
/// anew: the cancel is re-checked at its final send and a refused replacement
/// puts the original back once. Do not switch this to `PUT /orders/{id}`.
#[allow(clippy::too_many_arguments)]
fn replace_trigger(
    cx: &Cx<'_, Pending>,
    account: &Account,
    raw: &NativeOrder,
    order: &safe_cex_engine::model::Order,
    id: &str,
    size: f64,
    price: f64,
    trigger_price: Option<f64>,
) -> Result<(VecDeque<Operation>, Option<Operation>), String> {
    let unchanged = || format!("Your {} stop was not changed", order.symbol);
    let restore = restore_operation(cx, account, raw).ok_or_else(|| {
        format!(
            "{}: its exact native parameters are not reconciled",
            unchanged()
        )
    })?;
    let opts = PlaceOrderOpts {
        symbol: order.symbol.clone(),
        kind: order.kind,
        side: order.side,
        amount: size,
        price: if order.kind == OrderType::Conditional {
            Some(price)
        } else {
            trigger_price
        },
        trigger_price,
        reduce_only: Some(order.reduce_only),
        trigger_type: order.trigger_type,
        sl_trigger_type: order.trigger_type,
        tp_trigger_type: order.trigger_type,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        time_in_force: None,
        client_order_id: None,
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    };
    let mut operations = place_operations(
        std::slice::from_ref(&opts),
        cx,
        account,
        &HashMap::new(),
        &HashMap::new(),
        false,
    )?;
    for operation in &mut operations {
        if let Kind::Place { leg, replaces, .. } = &mut operation.kind {
            *leg = Some(STOP_REPLACEMENT);
            *replaces = Some(Box::new((order.symbol.clone(), order.side, order.kind)));
        }
    }
    let guard = StopGuard {
        raw: raw.clone(),
        symbol: order.symbol.clone(),
        side: order.side,
        kind: order.kind,
        trigger: trigger_price.ok_or("Paradex stop replacement has no trigger")?,
    };
    guard
        .validate(account, cx.store, cx.now)
        .map_err(|error| error.message)?;
    operations.push_front(Operation {
        receipts: Vec::new(),
        hash: None,
        method: Method::Delete,
        path: format!("/orders/{}", encode(id)),
        body: None,
        kind: Kind::GuardedCancel(Box::new(guard)),
    });
    Ok((operations, Some(restore)))
}
/// The original native stop as a new order: every field from its reconciled row,
/// with a fresh client id. None when a field has no exact native form.
fn restore_operation(
    cx: &Cx<'_, Pending>,
    account: &Account,
    raw: &NativeOrder,
) -> Option<Operation> {
    // Only shapes the signer admits (hosted::OrderType): a market stop carries
    // REDUCE_ONLY, a stop limit carries no flag. Anything else is refused
    // before its cancel rather than put back as something else.
    let kind = match raw.kind.as_str() {
        "STOP_MARKET" => "STOP_MARKET",
        "STOP_LIMIT" => "STOP_LIMIT",
        "STOP_LOSS_MARKET" => "STOP_LOSS_MARKET",
        "TAKE_PROFIT_MARKET" => "TAKE_PROFIT_MARKET",
        _ => return None,
    };
    let instruction = match raw.instruction.as_str() {
        "GTC" => "GTC",
        "IOC" => "IOC",
        "POST_ONLY" => "POST_ONLY",
        "RPI" => "RPI",
        _ => return None,
    };
    let flags = raw.flags.as_deref().unwrap_or_default();
    let reduce_only = raw.reduce_only.unwrap_or(false) || flags.iter().any(|f| f == "REDUCE_ONLY");
    if flags.iter().any(|f| f != "REDUCE_ONLY")
        || raw.size.is_empty()
        || raw.size != raw.remaining_size
        || raw.trigger_price.is_empty()
    {
        return None;
    }
    let limit = kind.ends_with("_LIMIT");
    if (limit && (raw.price.is_empty() || reduce_only)) || (!limit && !reduce_only) {
        return None;
    }
    let payload = Placement {
        market: raw.market.clone(),
        side: if raw.side == "BUY" { "BUY" } else { "SELL" },
        kind,
        size: raw.size.clone(),
        price: limit.then(|| raw.price.clone()),
        trigger_price: Some(raw.trigger_price.clone()),
        instruction,
        flags: reduce_only.then_some(["REDUCE_ONLY"]),
        // The only self-trade mode this SDK's Stark signer will sign with a
        // client id (hosted::Stp): the put-back is placed as every stop is.
        stp: Some("EXPIRE_TAKER"),
        signature: String::new(),
        signature_timestamp: 0,
        client_id: Some(client_id(cx).ok()?),
    };
    let receipts = placement_receipt(&payload).into_iter().collect();
    let expected = vec![payload.client_id.clone()];
    let signer = account.credentials()?;
    let (body, step) = signed(
        account,
        signer,
        Method::Post,
        "/orders",
        vec![payload],
        false,
    )
    .ok()?;
    Some(Operation {
        receipts,
        hash: Some(step),
        method: Method::Post,
        path: "/orders".into(),
        body: Some(body),
        kind: Kind::Place {
            expected,
            leg: Some(STOP_RESTORE),
            batch: false,
            replaces: None,
        },
    })
}
fn cancel_proven(body: &str, original: &NativeOrder) -> bool {
    #[derive(Deserialize)]
    struct Page {
        results: Vec<NativeOrder>,
    }
    let Ok(page) = serde_json::from_str::<Page>(body) else {
        return false;
    };
    let mut targets = page.results.iter().filter(|row| row.id == original.id);
    let Some(row) = targets.next() else {
        return false;
    };
    fn positive_decimal(text: &str) -> Option<(&str, &str)> {
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        if whole.is_empty()
            || !whole
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let value = (
            whole.trim_start_matches('0'),
            fraction.trim_end_matches('0'),
        );
        (!value.0.is_empty() || !value.1.is_empty()).then_some(value)
    }
    let same_quantity = |a: &str, b: &str| {
        positive_decimal(a)
            .zip(positive_decimal(b))
            .is_some_and(|(a, b)| a == b)
    };
    targets.next().is_none()
        && row.market == original.market
        && row.side == original.side
        && row.kind == original.kind
        && row.status == "CLOSED"
        && row.cancel_reason == "USER_CANCELED"
        && same_quantity(&row.size, &original.size)
        && same_quantity(&row.remaining_size, &original.size)
        && row.trigger_price == original.trigger_price
        && row.client_id == original.client_id
        && row.created_at == original.created_at
        && row
            .last_updated_at
            .parse::<u64>()
            .ok()
            .is_some_and(|updated| {
                original
                    .created_at
                    .parse::<u64>()
                    .ok()
                    .is_some_and(|created| updated >= created)
                    && updated >= original.last_updated_at.parse::<u64>().unwrap_or(0)
            })
        && row.flags == original.flags
        && row.reduce_only == original.reduce_only
        && row.instruction == original.instruction
        && (!row.kind.ends_with("_LIMIT") || row.price == original.price)
}
/// The amended stop's original, re-checked when its cancel is finally sent.
#[derive(Clone)]
pub(crate) struct StopGuard {
    raw: NativeOrder,
    symbol: String,
    side: OrderSide,
    kind: OrderType,
    /// The replacement's rounded native trigger.
    trigger: f64,
}
impl StopGuard {
    /// The original is unchanged and the replacement trigger is on the safe side
    /// of both a fresh mark and a fresh last price, so it cannot fire at once.
    pub(crate) fn validate(
        &self,
        account: &Account,
        store: &safe_cex_engine::Store,
        now: f64,
    ) -> Result<(), VenueError> {
        use safe_cex_engine::protective_amend::refused;
        let unchanged = || format!("Your {} stop was not changed", self.symbol);
        let current = account.native_order(&self.raw.id);
        if current.is_none_or(|row| {
            row.status != self.raw.status
                || row.size != self.raw.size
                || row.remaining_size != self.raw.remaining_size
                || row.trigger_price != self.raw.trigger_price
                || row.price != self.raw.price
                || row.last_updated_at != self.raw.last_updated_at
        }) {
            return Err(refused(Some(format!(
                "{}: the original order changed before cancellation",
                unchanged()
            ))));
        }
        self.prices_safe(store, now)
    }
    fn prices_safe(&self, store: &safe_cex_engine::Store, now: f64) -> Result<(), VenueError> {
        use safe_cex_engine::model::StopTriggerType;
        use safe_cex_engine::protective_amend::{fresh_reference, refused};
        let unchanged = || format!("Your {} stop was not changed", self.symbol);
        // A stop entry (Conditional) fires like a stop loss: a buy above the
        // market, a sell below it; a take profit fires the other way.
        let stops = matches!(self.kind, OrderType::StopLoss | OrderType::Conditional);
        let below = stops == (self.side == OrderSide::Sell);
        for (reference, label) in [
            (StopTriggerType::MarkPrice, "mark"),
            (StopTriggerType::LastPrice, "last"),
        ] {
            let Some(price) = fresh_reference(store, &self.symbol, reference, now) else {
                return Err(refused(Some(format!(
                    "{}: a fresh {label} price is required before cancellation",
                    unchanged()
                ))));
            };
            if (below && self.trigger >= price) || (!below && self.trigger <= price) {
                return Err(refused(Some(format!(
                    "{}: the replacement would trigger immediately against the {label} price",
                    unchanged()
                ))));
            }
        }
        Ok(())
    }
}
const STOP_REPLACEMENT: &str = "stop replacement";
const STOP_RESTORE: &str = "original stop";
#[derive(Serialize)]
struct Modify {
    id: String,
    market: String,
    side: String,
    #[serde(rename = "type")]
    kind: String,
    size: String,
    // The venue refuses a price on a market trigger; the signature still covers 0.
    #[serde(skip_serializing_if = "unpriced")]
    price: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    signature: String,
    signature_timestamp: u64,
}
fn unpriced(price: &str) -> bool {
    price == "0"
}
impl SignedBody for Modify {
    fn preimage(&self) -> Preimage<'_> {
        Preimage {
            market: &self.market,
            side: &self.side,
            kind: &self.kind,
            size: &self.size,
            price: Some(&self.price),
            id: Some(&self.id),
        }
    }
    fn item(&self, now_ms: u64) -> Item<'_> {
        Item::Modify(ModifyStatement {
            id: &self.id,
            timestamp_ms: now_ms,
            market: &self.market,
            side: &self.side,
            order_type: &self.kind,
            size: &self.size,
            price: &self.price,
            trigger_price: self.trigger_price.as_deref(),
        })
    }
    fn sign(&mut self, signature: String, timestamp: u64) {
        self.signature = signature;
        self.signature_timestamp = timestamp;
    }
}
fn capped(order: &PlaceOrderOpts) -> bool {
    order.kind == OrderType::Market && order.max_slippage_pct.is_some()
}
fn decode_quote(body: &str) -> Result<Quote, String> {
    #[derive(Deserialize)]
    struct Book {
        bids: Vec<[Number; 2]>,
        asks: Vec<[Number; 2]>,
    }
    let book: Book =
        serde_json::from_str(body).map_err(|_| "Invalid Paradex capped market orderbook")?;
    Ok(Quote {
        fair: None,
        bid: book
            .bids
            .first()
            .map(|level| level[0].0)
            .filter(|p| positive(*p)),
        ask: book
            .asks
            .first()
            .map(|level| level[0].0)
            .filter(|p| positive(*p)),
    })
}
fn round_price(price: f64, step: f64, up: bool) -> f64 {
    let step = if positive(step) { step } else { 0.000001 };
    let ratio = price / step;
    num::adjust(
        if up {
            (ratio - f64::EPSILON).ceil()
        } else {
            (ratio + f64::EPSILON).floor()
        } * step,
        step,
    )
}
fn snapshot_fair(cx: &Cx<'_, Pending>, symbol: &str) -> Option<f64> {
    let ticker = cx.store.ticker(symbol)?;
    let midpoint =
        (positive(ticker.bid) && positive(ticker.ask)).then_some((ticker.bid + ticker.ask) / 2.);
    [
        Some(ticker.mark),
        Some(ticker.index),
        ticker.mid_price,
        Some(ticker.last),
        midpoint,
    ]
    .into_iter()
    .flatten()
    .find(|p| positive(*p))
}
fn capped_price(
    order: &PlaceOrderOpts,
    quote: Quote,
    band: Option<f64>,
    cx: &Cx<'_, Pending>,
) -> Result<f64, String> {
    let buy = order.side == OrderSide::Buy;
    let touch = if buy { quote.ask } else { quote.bid }
        .filter(|p| positive(*p))
        .ok_or("Paradex capped market order needs a live touch price")?;
    let fair = quote.fair.unwrap_or(touch);
    let cap = order.max_slippage_pct.expect("capped market").max(0.) / 100.;
    let band_limit = band
        .filter(|b| positive(*b))
        .map(|width| fair * if buy { 1. + width } else { 1. - width });
    let mut raw = fair * if buy { 1. + cap } else { 1. - cap };
    if let Some(limit) = band_limit {
        raw = if buy { raw.min(limit) } else { raw.max(limit) };
    }
    let touch_inside =
        band_limit.is_none_or(|limit| if buy { touch <= limit } else { touch >= limit });
    if touch_inside && if buy { raw < touch } else { raw > touch } {
        raw = touch;
    }
    let step = cx
        .store
        .market(&order.symbol)
        .map_or(0.000001, |m| m.precision.price);
    let mut price = round_price(raw, step, buy);
    if let Some(limit) = band_limit {
        if buy && price > limit {
            price = round_price(limit, step, false);
        } else if !buy && price < limit {
            price = round_price(limit, step, true);
        }
    }
    if !positive(price) {
        return Err("Paradex capped market IOC price is invalid".into());
    }
    Ok(price)
}

fn is_trigger(kind: &str) -> bool {
    matches!(
        kind,
        "STOP_LIMIT"
            | "STOP_MARKET"
            | "STOP_LOSS_LIMIT"
            | "STOP_LOSS_MARKET"
            | "TAKE_PROFIT_LIMIT"
            | "TAKE_PROFIT_MARKET"
    )
}
#[derive(Deserialize)]
struct Identity {
    id: String,
    #[serde(default)]
    client_id: Option<String>,
}
struct PlacementResult {
    ids: Vec<String>,
    error: Option<String>,
}
fn decode_placement(
    body: &str,
    expected: &[Option<String>],
    batch: bool,
) -> Result<PlacementResult, String> {
    #[derive(Deserialize)]
    struct Failure {
        #[serde(default)]
        error: String,
        #[serde(default)]
        message: String,
    }
    #[derive(Deserialize)]
    struct Batch {
        #[serde(default)]
        orders: Vec<Identity>,
        #[serde(default)]
        // A success row answers null in its error slot.
        errors: Vec<Option<Failure>>,
    }
    let (rows, errors) = if batch {
        let response: Batch =
            serde_json::from_str(body).map_err(|_| "invalid Paradex batch placement envelope")?;
        (
            response.orders,
            response.errors.into_iter().flatten().collect(),
        )
    } else {
        (
            vec![
                serde_json::from_str::<Identity>(body)
                    .map_err(|_| "invalid Paradex placement response")?,
            ],
            Vec::new(),
        )
    };
    let mut found = HashMap::new();
    let mut seen_ids = HashSet::new();
    for row in rows {
        if row.id.trim().is_empty() || !seen_ids.insert(row.id.clone()) {
            return Err("Paradex placement missing or repeated canonical ID".into());
        }
        let key = if expected == [None] {
            None
        } else {
            row.client_id
        };
        if !expected.contains(&key) || found.insert(key, row.id).is_some() {
            return Err("Paradex placement unknown or repeated client identity".into());
        }
    }
    let missing: Vec<_> = expected
        .iter()
        .filter(|id| !found.contains_key(*id))
        .collect();
    let error = if !errors.is_empty() {
        if errors.len() != missing.len()
            || errors
                .iter()
                .any(|e| e.error.is_empty() && e.message.is_empty())
        {
            return Err("Paradex batch success/error counts are inconsistent".into());
        }
        Some(format!(
            "Paradex batch rejected client IDs {missing:?}: {}",
            errors
                .iter()
                .map(|e| format!("{}: {}", e.error, e.message))
                .collect::<Vec<_>>()
                .join_with("; ")
        ))
    } else if !missing.is_empty() {
        return Err("Paradex placement missing client results".into());
    } else {
        None
    };
    Ok(PlacementResult {
        ids: expected.iter().filter_map(|id| found.remove(id)).collect(),
        error,
    })
}
fn decode_cancels(body: &str, ids: &[String]) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Row {
        id: Option<String>,
        status: String,
    }
    #[derive(Deserialize)]
    struct Batch {
        results: Vec<Row>,
    }
    let batch: Batch =
        serde_json::from_str(body).map_err(|_| "invalid Paradex cancellation response")?;
    let mut seen = HashSet::new();
    for row in batch.results {
        let id = row.id.ok_or("uncorrelated Paradex cancellation result")?;
        if !ids.contains(&id)
            || !seen.insert(id)
            || !matches!(
                row.status.as_str(),
                "QUEUED_FOR_CANCELLATION" | "ALREADY_CLOSED"
            )
        {
            return Err("Paradex batch cancellation unknown, duplicate or rejected ID".into());
        }
    }
    if seen.len() != ids.len() {
        return Err("Paradex batch cancellation missing IDs".into());
    }
    Ok(())
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// A 2xx answer that does not prove what happened.
fn unproven(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

/// A refused answer. 408 and 5xx do not prove the venue refused it; a 2xx
/// error envelope is an explicit refusal without a documented code table.
fn answer_failure(status: u16, message: String) -> VenueError {
    let error = VenueError::new(message);
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    match status {
        200..=299 => error,
        401 => rejected(error, ErrorKind::AuthInvalid),
        403 => rejected(error, ErrorKind::AuthPermission),
        // The amend fence treats a 429 as possibly applied; match it.
        429 => error.kind(ErrorKind::RateLimited),
        408 => error.outcome(Outcome::Unknown),
        400..=499 => rejected(error, ErrorKind::Rejected),
        _ => error.outcome(Outcome::Unknown),
    }
}

/// Legacy dispatched writes do not pass the engine's HTTP proof boundary.
/// Weaken the failure before job aggregation or protective restore decisions;
/// keep read failures and definitive native refusals unchanged.
fn write_failure(status: u16, message: String) -> VenueError {
    let error = answer_failure(status, message);
    if matches!(status, 408 | 429 | 500..) {
        ambiguous_write(error)
    } else {
        error
    }
}

fn ambiguous_write(mut error: VenueError) -> VenueError {
    error
        .venue_message
        .get_or_insert_with(|| error.message.into_boxed_str());
    error.message = UNKNOWN_TEXT.into();
    error.outcome(Outcome::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_results_are_correlated_by_client_identity_not_position() {
        let expected = vec![Some("first".into()), Some("second".into())];
        assert_eq!(
            decode_placement(
                r#"{"orders":[{"id":"2","client_id":"second"},{"id":"1","client_id":"first"}]}"#,
                &expected,
                true
            )
            .unwrap()
            .ids,
            ["1", "2"]
        );
        for body in [
            r#"{"orders":[{"id":"1","client_id":"first"}]}"#,
            r#"{"orders":[{"id":"1","client_id":"first"},{"id":"2","client_id":"first"}]}"#,
            r#"{"orders":[{"id":"1","client_id":"first"},{"id":"1","client_id":"second"}]}"#,
            r#"{"orders":[{"id":"1","client_id":"other"},{"id":"2","client_id":"second"}]}"#,
            r#"{"orders":[{"id":"1","client_id":"first"}],"errors":[{}]}"#,
            r#"{"orders":[],"errors":[{"error":"REJECTED"}]}"#,
        ] {
            assert!(decode_placement(body, &expected, true).is_err(), "{body}");
        }
    }
    #[test]
    fn cancel_results_require_every_exact_id_and_successful_status() {
        let ids = vec!["first".into(), "second".into()];
        assert!(decode_cancels(r#"{"results":[{"id":"second","status":"ALREADY_CLOSED"},{"id":"first","status":"QUEUED_FOR_CANCELLATION"}]}"#, &ids).is_ok());
        for body in [
            r#"{"results":[{"id":"first","status":"QUEUED_FOR_CANCELLATION"}]}"#,
            r#"{"results":[{"id":"first","status":"NOT_FOUND"},{"id":"second","status":"ALREADY_CLOSED"}]}"#,
            r#"{"results":[{"status":"NOT_FOUND"},{"id":"second","status":"ALREADY_CLOSED"}]}"#,
            r#"{"results":[{"id":"first","status":"ALREADY_CLOSED"},{"id":"first","status":"ALREADY_CLOSED"}]}"#,
        ] {
            assert!(decode_cancels(body, &ids).is_err(), "{body}");
        }
    }
    #[test]
    fn placement_never_accepts_wrong_or_empty_identity() {
        let expected = vec![Some("caller".into())];
        assert!(decode_placement(r#"{"id":"1","client_id":"caller"}"#, &expected, false).is_ok());
        for body in [
            r#"{"id":"1"}"#,
            r#"{"id":"1","client_id":"another"}"#,
            r#"{"id":" ","client_id":"caller"}"#,
        ] {
            assert!(decode_placement(body, &expected, false).is_err());
        }
    }
}
