//! Sequenced g-API writes. The complete command is validated before dispatch;
//! ambiguous transport failures never cause a mutation to be retried.
mod repair;
use crate::{Pending, PhemexPerpetuals};
pub use repair::RepairRequest;
use safe_cex_engine::Level;
use safe_cex_engine::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict, WriteFact};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    MarginType, Order, OrderIdentityKind, OrderIdentityReceipt, OrderSide, OrderStatus, OrderType,
    PlaceOrderOpts, PositionMode, PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    Context, Operation, Plan, Preflight, PreflightPolicy, PreflightPurpose,
};
use safe_cex_engine::setting_holds::SettingKey;
use safe_cex_engine::text::JoinWith;
pub(crate) use safe_cex_engine::url::encode;
use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
use safe_cex_engine::{Command, CommandId, Cx, ErrorKind, Method, Response, VenueError, num};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
pub(crate) struct State {
    /// The venue answered the current write with a code.
    answered: bool,
    acknowledged: HashMap<String, Order>,
    jobs: HashMap<CommandId, Job>,
    retired: Vec<RetiredWrite>,
    sides: HashMap<String, String>,
    brackets: HashMap<String, Brackets>,
    /// Venue-originated late cancels, sent one at a time so no ack sees another's revision bump.
    late: VecDeque<Step>,
    repair: repair::State,
}
impl State {
    pub(crate) fn placing(&self, symbol: &str) -> bool {
        self.jobs
            .values()
            .any(|job| job.placing.iter().any(|s| s == symbol))
    }
}
#[derive(Clone, Copy)]
struct Brackets {
    tp: Option<StopTriggerType>,
    sl: Option<StopTriggerType>,
}
struct Job {
    plan: Plan,
    slot: Option<Slot>,
    identity: safe_cex_engine::signing::Identity,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    placement_report: Option<WritePlan>,
    placement_refusal: Option<VenueError>,
    settings: bool,
    /// Symbols a `PlaceOrders` job may leave with an unknown outcome.
    placing: Vec<String>,
    /// A step took effect, so a later rejection cannot mean nothing happened.
    effective: bool,
    revision: u64,
    signed_until: Option<f64>,
    live: Option<Request>,
    /// Set while a replacement waits for its accepted cancel to end.
    cancel_wait: Option<safe_cex_engine::request_context::ConditionWait>,
    /// The amended order, once its cancel is proven and its replacement is issued.
    replacing: Option<(String, OrderSide, OrderType)>,
    /// The amended stop's put-back, taken from its replacement when issued.
    protect: Option<Box<StopProtect>>,
    /// The put-back is under way: the replacement was refused.
    restoring: bool,
}
impl Job {
    fn record_placement(&mut self, client_order_id: &Option<String>, result: ElementResult) {
        if let Some(report) = &mut self.placement_report
            && let Some(row) = report.outcomes().iter().find(|row| {
                &row.element.client_order_id == client_order_id
                    && !matches!(row.result, ElementResult::Placed { .. })
            })
        {
            let _ = report.record(row.element.index, result);
        }
    }

    fn gone_note(&self, unknown: bool) -> Option<String> {
        self.replacing
            .as_ref()
            .filter(|_| self.ids.is_empty())
            .map(|(symbol, side, kind)| {
                if matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
                    safe_cex_engine::error::stop_gone_text(symbol, *side, *kind, unknown)
                } else {
                    safe_cex_engine::error::order_gone_text(symbol, unknown)
                }
            })
    }
}
/// One symbol-scoped position read ahead of a command that needs its mode.
pub struct SymbolModeRead {
    pub(crate) preflight: Preflight,
    pub(crate) context: Context,
    command: Command,
    symbol: String,
    tried: Vec<String>,
}
struct RetiredWrite {
    request: Request,
    placing: Vec<String>,
    signed_until: Option<f64>,
}
#[derive(Clone)]
pub struct Request {
    /// `None` for a venue-originated cancel of an order that landed after cancel-all.
    command: Option<CommandId>,
    call: Call,
    revision: u64,
    /// Uncapped raw expiry observed only after an actual signing release.
    signed_until: Option<f64>,
    released_request: Option<safe_cex_engine::RequestId>,
}
pub struct PlannedRequest {
    pub(crate) native: Request,
    plan: Plan,
    slot: Slot,
    pub(crate) context: Context,
}
fn serial_call() -> SerialCall {
    SerialCall {
        auth: Default::default(),
        operation: Operation::Mutation,
        timeout_ms: 20_000,
        attempts: 1,
        pacing: None,
    }
}
const REFUSALS: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Range(11001, 11001),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11038),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11027),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Range(11034, 11036),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11053),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Range(11055, 11056),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11058),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Range(11081, 11082),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11011),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11041),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11043),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11044),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11048),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11085),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(11150),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(20004),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Int(39999),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("Rejected"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};
/// How a failed write ended, which decides when its open orders are read.
pub(crate) enum Outcome {
    /// The venue proved no order was created; no read is owed.
    Rejected,
    /// No proof either way (unproven answer, transport, non-2xx): Phemex may still
    /// accept the write until its signed expiry, so read once that passes.
    Unknown,
}
/// Margin for the client clock running ahead of Phemex's when bounding a late write.
const CLOCK_SKEW_MS: f64 = 5_000.;
/// How long an amend waits for the socket to end the order its cancel moved to PendingCancel.
const CANCEL_CONFIRM_MS: u32 = 10_000;
struct Step {
    method: Method,
    path: &'static str,
    query: String,
    call: Call,
    replacement: Option<Guard>,
}
struct Guard {
    id: String,
    symbol: String,
    /// The amended order's own, which a virtual leg does not share with its parent.
    side: OrderSide,
    kind: OrderType,
    amount: f64,
    filled: f64,
    /// A stop, take-profit or trigger edit: its cancel's final check and put-back.
    stop: Option<Box<StopProtect>>,
}
/// An amended stop's final pre-cancel check, and what puts it back.
struct StopProtect {
    guard: StopGuard,
    /// The original placed again, unchanged, once its replacement is refused.
    restore: VecDeque<Step>,
    /// False when the original cannot be proven again (its entry has fills):
    /// a refused replacement then says the stop was removed.
    restorable: bool,
}
/// The amended order (and its entry, for a leg) as planned, and the
/// replacement trigger's reference.
struct StopGuard {
    order: Order,
    parent: Option<Order>,
    trigger: f64,
    reference: StopTriggerType,
    /// The side a live trigger must stay on; None while nothing can fire at
    /// once (a leg of an unfilled entry).
    below: Option<bool>,
    /// A reduce-only or closing stop needs a position on the side it closes.
    closes: Option<PositionSide>,
}
impl StopGuard {
    /// A refusal is typed NotSent and leaves the order untouched.
    fn validate(&self, store: &safe_cex_engine::Store, now: f64) -> Result<(), VenueError> {
        use safe_cex_engine::protective_amend::{fresh_reference, refused};
        let unchanged = || format!("Your {} order was not changed", self.order.symbol);
        // This cancel's own optimistic Canceling status is not a change.
        let same = |planned: &Order| {
            store.order(&planned.id).is_some_and(|current| {
                current == planned
                    || (current.status == OrderStatus::Canceling && {
                        let mut current = current.clone();
                        current.status = planned.status;
                        current == *planned
                    })
            })
        };
        if self.order.filled != 0.0
            || !same(&self.order)
            || self.parent.as_ref().is_some_and(|parent| !same(parent))
        {
            return Err(refused(Some(format!(
                "{}: it changed or filled before cancellation",
                unchanged()
            ))));
        }
        if let Some(side) = self.closes
            && !store
                .positions()
                .iter()
                .any(|p| p.symbol == self.order.symbol && p.side == side && p.contracts > 0.0)
        {
            return Err(refused(Some(format!(
                "{}: no position for it to protect",
                unchanged()
            ))));
        }
        let label = match self.reference {
            StopTriggerType::MarkPrice => "mark",
            StopTriggerType::IndexPrice => "index",
            StopTriggerType::LastPrice => "last",
        };
        let Some(price) = fresh_reference(store, &self.order.symbol, self.reference, now) else {
            return Err(refused(Some(format!(
                "{}: the {label} price reference is stale",
                unchanged()
            ))));
        };
        if let Some(below) = self.below
            && ((below && self.trigger >= price) || (!below && self.trigger <= price))
        {
            return Err(refused(Some(format!(
                "{}: the new trigger has already crossed the {label} price",
                unchanged()
            ))));
        }
        Ok(())
    }
}
#[derive(Clone)]
enum Call {
    Place {
        order: Box<Order>,
        pos_side: String,
        brackets: Brackets,
    },
    Cancel {
        before: Vec<Order>,
        ids: Vec<String>,
    },
    /// A nuke's venue cancel-all; it names no order, so no row or repair state follows it.
    CancelAccount,
    Settings {
        symbol: String,
        leverage: Option<f64>,
        side: Option<PositionSide>,
        margin: Option<MarginType>,
        mode: Option<PositionMode>,
    },
}
#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    error: Option<Box<RawValue>>,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<Box<RawValue>>,
}

pub(crate) fn unproven_diagnostics(
    response: Response<'_>,
    evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
) {
    if let Ok(envelope) = serde_json::from_str::<crate::wire::Envelope<'_>>(response.body) {
        if let Some(code) = envelope.code {
            evidence.venue_code(&code.to_string());
        }
        if !envelope.msg.is_empty() {
            evidence.venue_message(&envelope.msg);
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    // A gone batch row answers bizError 10002 with every other field null.
    #[serde(default, rename = "orderID", deserialize_with = "nullable_string")]
    order_id: String,
    #[serde(default, rename = "clOrdID")]
    client_id: Option<String>,
    #[serde(default, deserialize_with = "nullable_string")]
    ord_status: String,
    #[serde(default, deserialize_with = "nullable_string")]
    exec_status: String,
    #[serde(default)]
    biz_error: i64,
    #[serde(default, deserialize_with = "crate::wire::optional_number")]
    cum_qty_rq: Option<f64>,
    #[serde(default, deserialize_with = "crate::wire::optional_number")]
    cum_qty: Option<f64>,
    #[serde(default, deserialize_with = "crate::wire::optional_number")]
    exec_qty: Option<f64>,
    #[serde(default)]
    code: i64,
}
fn nullable_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}
#[derive(Deserialize)]
#[serde(untagged)]
enum CancelResult {
    Rows(Vec<Identity>),
    Row(Identity),
}

struct CancelAck {
    accepted: HashSet<String>,
    finalized: HashMap<String, OrderStatus>,
    /// The venue no longer has these: filled or canceled, it cannot say which.
    gone: HashSet<String>,
    fills: Vec<(String, f64)>,
    problem: Option<String>,
    refusals: HashMap<String, NativeCode<'static>>,
}
/// OM_ORDER_NOT_FOUND on a cancel: the order is already gone.
const ORDER_NOT_FOUND: i64 = 10002;
fn gone_ack(ids: &[String]) -> CancelAck {
    CancelAck {
        accepted: ids.iter().cloned().collect(),
        finalized: ids
            .iter()
            .map(|id| (id.clone(), OrderStatus::Canceled))
            .collect(),
        gone: ids.iter().cloned().collect(),
        fills: Vec::new(),
        problem: None,
        refusals: HashMap::new(),
    }
}
fn decode_cancel_ack(raw: &str, ids: &[String]) -> Result<CancelAck, String> {
    let result: CancelResult =
        serde_json::from_str(raw).map_err(|_| "Phemex malformed cancel acknowledgement")?;
    let (rows, array) = match result {
        CancelResult::Rows(rows) => (rows, true),
        CancelResult::Row(row) => (vec![row], false),
    };
    let mut seen = HashSet::new();
    let mut accepted = HashSet::new();
    let mut finalized = HashMap::new();
    let mut gone = HashSet::new();
    let mut problem = None;
    let mut refusals = HashMap::new();
    let mut fills = Vec::new();
    let positional = !array || rows.len() == ids.len();
    for (index, mut row) in rows.into_iter().enumerate() {
        if row.order_id.is_empty()
            && row.biz_error != 0
            && positional
            && let Some(id) = ids.get(index)
        {
            row.order_id.clone_from(id);
        }
        if !ids.contains(&row.order_id) || !seen.insert(row.order_id.clone()) {
            accepted.remove(&row.order_id);
            finalized.remove(&row.order_id);
            refusals.remove(&row.order_id);
            problem = Some("Phemex cancel identity mismatch".to_string());
            continue;
        }
        if row.code == ORDER_NOT_FOUND || row.biz_error == ORDER_NOT_FOUND {
            accepted.insert(row.order_id.clone());
            finalized.insert(row.order_id.clone(), OrderStatus::Canceled);
            gone.insert(row.order_id);
            continue;
        }
        if row.code != 0 || row.biz_error != 0 || row.exec_status.eq_ignore_ascii_case("Rejected") {
            refusals.insert(
                row.order_id.clone(),
                if row.code != 0 {
                    NativeCode::Int(row.code)
                } else if row.biz_error != 0 {
                    NativeCode::Int(row.biz_error)
                } else {
                    NativeCode::Text("Rejected")
                },
            );
            problem = Some(format!(
                "Phemex cancel row rejected: {} {}",
                row.code, row.biz_error
            ));
            continue;
        }
        let filled = row.cum_qty_rq.or(row.cum_qty).or(row.exec_qty);
        if filled.is_some_and(|v| !v.is_finite() || v < 0.0) {
            problem = Some("Phemex invalid cancel cumulative fill".into());
            continue;
        }
        // PendingCancel is accepted but not final: the socket frame ends the order.
        let pending_cancel = row.exec_status.eq_ignore_ascii_case("PendingCancel");
        let status = match row.ord_status.to_ascii_lowercase().as_str() {
            "init" | "created" => None,
            "canceled" | "cancelled" | "deactivated" => Some(OrderStatus::Canceled),
            "filled" => Some(OrderStatus::Closed),
            "new" | "partiallyfilled" if pending_cancel => None,
            _ => {
                problem = Some("Phemex cancel acknowledgement has no confirmed outcome".into());
                continue;
            }
        };
        if let Some(filled) = filled {
            fills.push((row.order_id.clone(), filled));
        }
        accepted.insert(row.order_id.clone());
        if let Some(status) = status {
            finalized.insert(row.order_id, status);
        }
    }
    if seen.is_empty() || array && seen.len() != ids.len() {
        problem = Some("Phemex missing cancel acknowledgement rows".into());
    }
    // A documented bulk object acknowledges the request, but its
    // representative order row proves finality only for that ID.
    if !array && problem.is_none() {
        accepted.extend(ids.iter().cloned());
    }
    Ok(CancelAck {
        accepted,
        finalized,
        gone,
        fills,
        problem,
        refusals,
    })
}

fn reject(cx: &mut Cx<'_, Pending>, preflight: &Preflight, error: VenueError) {
    if let safe_cex_engine::request_context::Owner::Command { id, .. } = preflight.owner() {
        cx.command_error(id, error.message.clone());
    }
    let _ = cx.fail_preflight(preflight, error);
}
/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(safe_cex_engine::Outcome::NotSent)
}
fn query(pairs: Vec<(&str, String)>) -> String {
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={}", encode(&v)))
        .collect::<Vec<_>>()
        .join_with("&")
}
fn positive(v: f64, step: f64) -> Result<f64, String> {
    let n = num::adjust(v, step);
    if v.is_finite() && v > 0.0 && n.is_finite() && n > 0.0 {
        Ok(n)
    } else {
        Err("Phemex quantity/price must be finite and positive after rounding".into())
    }
}
fn trigger(t: StopTriggerType) -> Result<&'static str, String> {
    match t {
        StopTriggerType::MarkPrice => Ok("ByMarkPrice"),
        StopTriggerType::LastPrice => Ok("ByLastPrice"),
        StopTriggerType::IndexPrice => Ok("ByIndexPrice"),
    }
}
fn plain(o: &PlaceOrderOpts) -> bool {
    matches!(o.kind, OrderType::Limit | OrderType::Market)
        && o.stop_loss.is_none()
        && o.take_profit.is_none()
        && o.trigger_price.is_none()
}
pub(crate) fn valid_client(id: &str) -> bool {
    let len: usize = id
        .chars()
        .map(|c| if c as u32 > 0xffff { 2 } else { 1 })
        .sum();
    (1..=40).contains(&len)
        && !id
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
}
/// A row's id until its placement is answered: no venue call can name it.
const CREATING_PREFIX: &str = "__phemex_creating_";
fn native(id: &str) -> Result<&str, String> {
    if id.starts_with(CREATING_PREFIX) {
        return Err(
            "This Phemex order is still being placed. Try again once it has a venue order ID, or use Cancel All, which waits for it"
                .into(),
        );
    }
    let base = id
        .strip_suffix("__stop_loss")
        .or_else(|| id.strip_suffix("__take_profit"))
        .unwrap_or(id);
    if base.is_empty()
        || base.contains("__")
        || base.len() > 128
        || !base
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    {
        Err("Phemex requires an actual venue order ID".into())
    } else {
        Ok(base)
    }
}
fn active(o: &Order) -> bool {
    matches!(
        o.status,
        OrderStatus::Open | OrderStatus::Creating | OrderStatus::Updating | OrderStatus::Canceling
    )
}

impl PhemexPerpetuals {
    fn place_plan(
        &self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
    ) -> Result<VecDeque<Step>, String> {
        if orders.len() > 1000 {
            return Err("Phemex command exceeds 1000 orders".into());
        }
        let mut seen = HashSet::new();
        for o in &orders {
            if o.stp_mode.is_some() {
                return Err("Phemex self-trade prevention is unsupported".into());
            }
            if o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some() {
                return Err("Phemex does not support Bybit execution controls".into());
            }
            if let Some(id) = &o.client_order_id {
                if !plain(o) || !valid_client(id) {
                    return Err(
                        "Phemex caller clientOrderId requires a plain order and 1..40 characters"
                            .into(),
                    );
                }
                if !seen.insert(id.clone()) {
                    return Err("Phemex duplicate clientOrderId in command".into());
                }
                // A late landing of the lost write is canceled by clOrdID alone.
                if self.account.lost_client(id) {
                    return Err(format!(
                        "Phemex clientOrderId {id} belongs to a placement with an unknown outcome; use a new clientOrderId"
                    ));
                }
            }
        }
        let mut steps = VecDeque::new();
        for o in orders {
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found", o.symbol))?;
            if !m.precision.amount.is_finite() || m.precision.amount <= 0.0 {
                return Err("Phemex quantity precision is unknown".into());
            }
            if !m.active || m.is_read_only == Some(true) {
                return Err("Phemex market is not writable".into());
            }
            if o.kind == OrderType::Conditional {
                return Err("Phemex opening trigger orders are disabled until stopPxRp and triggerType are verified".into());
            }
            if o.trigger_price.is_some() && !o.redundant_protective_trigger() {
                return Err("Phemex triggerPrice is unsupported for this order type".into());
            }
            if o.client_order_id.is_some() && o.amount > m.limits.amount.max {
                return Err("Phemex caller clientOrderId cannot span split orders".into());
            }
            let mode = self
                .account
                .mode(&o.symbol)
                .ok_or("Phemex position mode is unknown; await account reconciliation")?;
            let protective = matches!(
                o.kind,
                OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
            );
            let reduce = o.reduce_only == Some(true) || protective;
            let pos_side = self
                .account
                .pos_side(&o.symbol, o.side, reduce)
                .ok_or("Phemex position side is unknown")?;
            let p = o
                .price
                .map(|p| positive(p, m.precision.price))
                .transpose()?;
            if o.kind != OrderType::Market && p.is_none() {
                return Err("Phemex limit/stop/trailing price is required".into());
            }
            let tp = o
                .take_profit
                .map(|p| positive(p, m.precision.price))
                .transpose()?;
            let sl = o
                .stop_loss
                .map(|p| positive(p, m.precision.price))
                .transpose()?;
            let brackets = Brackets {
                tp: tp.map(|_| o.tp_trigger_type.unwrap_or(StopTriggerType::LastPrice)),
                sl: sl.map(|_| o.sl_trigger_type.unwrap_or(StopTriggerType::MarkPrice)),
            };
            if let Some(t) = brackets.tp {
                trigger(t)?;
            }
            if let Some(t) = brackets.sl {
                trigger(t)?;
            }
            let kind = match o.kind {
                OrderType::Market => "Market",
                OrderType::Limit => "Limit",
                OrderType::TakeProfit => "MarketIfTouched",
                _ => "Stop",
            };
            let full_close =
                o.amount == 0.0 && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
            let mut amount = if full_close || o.kind == OrderType::TrailingStopLoss {
                0.0
            } else {
                positive(o.amount, m.precision.amount)?
            };
            let mut offset = None;
            if o.kind == OrderType::TrailingStopLoss {
                let side = if o.side == OrderSide::Buy {
                    PositionSide::Short
                } else {
                    PositionSide::Long
                };
                let position = cx
                    .store
                    .positions()
                    .iter()
                    .find(|p| p.symbol == o.symbol && p.side == side && p.contracts > 0.0)
                    .ok_or("Phemex trailing stop requires a matching open position")?;
                let last = cx
                    .store
                    .ticker(&o.symbol)
                    .map(|t| t.last)
                    .filter(|p| p.is_finite() && *p > 0.0)
                    .ok_or("Phemex trailing stop requires a current ticker")?;
                let distance = positive((last - p.unwrap()).abs(), m.precision.price)?;
                offset = Some(if o.side == OrderSide::Sell {
                    -distance
                } else {
                    distance
                });
                amount = positive(position.contracts, m.precision.amount)?;
            }
            if !full_close && amount < m.limits.amount.min {
                return Err("Phemex quantity below market minimum".into());
            }
            let max = m.limits.amount.max;
            if !max.is_finite() || max <= 0.0 {
                return Err("Phemex market maximum quantity is unknown".into());
            }
            if o.client_order_id.is_some() && amount > max {
                return Err("Phemex caller clientOrderId cannot span split orders".into());
            }
            if offset.is_some() && amount > max {
                return Err("Phemex trailing full-position quantity exceeds maximum".into());
            }
            let units = if full_close {
                1.0
            } else {
                (amount / m.precision.amount).round()
            };
            let nearest_max = num::adjust(max, m.precision.amount);
            let max_units = (nearest_max / m.precision.amount).round()
                - if nearest_max > max { 1.0 } else { 0.0 };
            if o.client_order_id.is_some() && units > max_units {
                return Err("Phemex caller clientOrderId cannot span split orders".into());
            }
            if !units.is_finite()
                || units > 9_007_199_254_740_991.0
                || max_units < 1.0
                || (units / max_units).ceil() > 1000.0
            {
                return Err("Phemex order cannot be safely split".into());
            }
            if !max_units.is_finite() || max_units > 9_007_199_254_740_991.0 {
                return Err("Phemex market maximum cannot be represented safely".into());
            }
            // reference/phemex/phemex.exchange.ts:1372-1384: preserve equal-lot
            // rounding and the remainder even when their total drifts from the input.
            let lots = if amount > max {
                (amount / max).ceil() as usize
            } else {
                1
            };
            let rest = if amount > max {
                num::adjust(amount % max, m.precision.amount)
            } else {
                0.0
            };
            let lot_size = num::adjust((amount - rest) / lots as f64, m.precision.amount);
            let mut quantities = vec![lot_size; lots];
            if rest != 0.0 {
                quantities.push(rest);
            }
            for qty in quantities {
                if qty > max {
                    return Err("Phemex split lot exceeds market maximum".into());
                }
                if !full_close && qty < m.limits.amount.min {
                    return Err("Phemex split remainder below minimum quantity".into());
                }
                let id = match &o.client_order_id {
                    Some(id) => id.clone(),
                    None => format!(
                        "Tealstreet_{}",
                        safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
                    ),
                };
                if o.client_order_id.is_none() && !seen.insert(id.clone()) {
                    return Err("Phemex duplicate generated clientOrderId".into());
                }
                let mut params = vec![
                    ("clOrdID", id.clone()),
                    ("symbol", self.native_symbol(&m.symbol).to_owned()),
                    ("posSide", pos_side.into()),
                    (
                        "side",
                        if o.side == OrderSide::Buy {
                            "Buy"
                        } else {
                            "Sell"
                        }
                        .into(),
                    ),
                    ("ordType", kind.into()),
                ];
                if !full_close {
                    params.push(("orderQtyRq", num::format(qty)));
                }
                if offset.is_none() {
                    if matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                        params.push(("stopPxRp", num::format(p.unwrap())));
                    }
                    if o.kind != OrderType::Market {
                        params.push(("priceRp", num::format(p.unwrap())));
                    }
                    if o.kind == OrderType::Limit {
                        params.push((
                            "timeInForce",
                            match o.time_in_force.unwrap_or(TimeInForce::GoodTillCancel) {
                                TimeInForce::GoodTillCancel => "GoodTillCancel",
                                TimeInForce::ImmediateOrCancel => "ImmediateOrCancel",
                                TimeInForce::FillOrKill => "FillOrKill",
                                TimeInForce::PostOnly => "PostOnly",
                            }
                            .into(),
                        ));
                    }
                    if protective {
                        params.push(("closeOnTrigger", "true".into()));
                    }
                    if mode == PositionMode::Oneway && o.reduce_only == Some(true) && !protective {
                        params.push(("reduceOnly", "true".into()));
                    }
                    if let Some(sl) = sl {
                        params.push(("stopLossRp", num::format(sl)));
                    }
                    if let Some(tp) = tp {
                        params.push(("takeProfitRp", num::format(tp)));
                    }
                    if let Some(t) = brackets.tp {
                        params.push(("tpTrigger", trigger(t)?.into()));
                    }
                    if let Some(t) = brackets.sl {
                        params.push(("slTrigger", trigger(t)?.into()));
                    }
                    if matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                        let t = if o.kind == OrderType::TakeProfit {
                            o.tp_trigger_type.unwrap_or(StopTriggerType::LastPrice)
                        } else {
                            o.sl_trigger_type.unwrap_or(StopTriggerType::MarkPrice)
                        };
                        params.push(("triggerType", trigger(t)?.into()));
                    }
                } else if let Some(offset) = offset {
                    params.push(("pegOffsetValueRp", num::format(offset)));
                    params.push(("pegPriceType", "TrailingStopPeg".into()));
                }
                let row = Order {
                    id: format!("{CREATING_PREFIX}{id}"),
                    parent_id: None,
                    status: OrderStatus::Creating,
                    symbol: o.symbol.clone(),
                    kind: o.kind,
                    side: o.side,
                    price: p.unwrap_or(0.0),
                    amount: qty,
                    filled: 0.0,
                    remaining: qty,
                    reduce_only: reduce,
                    close: o.close,
                    client_order_id: Some(id),
                    time_in_force: o.time_in_force,
                    last_update: cx.now,
                    trigger_type: if o.kind == OrderType::TakeProfit {
                        Some(o.tp_trigger_type.unwrap_or(StopTriggerType::LastPrice))
                    } else if o.kind == OrderType::StopLoss {
                        Some(o.sl_trigger_type.unwrap_or(StopTriggerType::MarkPrice))
                    } else {
                        None
                    },
                    take_profit: tp,
                    stop_loss: sl,
                    extra: None,
                };
                steps.push_back(Step {
                    method: Method::Put,
                    path: "/g-orders/create",
                    query: query(params),
                    call: Call::Place {
                        order: Box::new(row),
                        pos_side: pos_side.into(),
                        brackets,
                    },
                    replacement: None,
                });
                if steps.len() > 1000 {
                    return Err("Phemex command exceeds 1000 native orders".into());
                }
            }
        }
        Ok(steps)
    }

    fn cancel_plan(
        &self,
        cx: &Cx<'_, Pending>,
        ids: Vec<String>,
    ) -> Result<VecDeque<Step>, String> {
        let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
        let mut seen = HashSet::new();
        for id in ids {
            let base = native(&id)?.to_owned();
            if !seen.insert(base.clone()) {
                continue;
            }
            let o = cx
                .store
                .order(&base)
                .or_else(|| cx.store.order(&id))
                .or_else(|| {
                    self.trading
                        .acknowledged
                        .get(&base)
                        .filter(|_| self.live_acknowledged(cx, &base))
                })
                .ok_or(
                    "Phemex cancel requires a cached order to resolve symbol and position side",
                )?;
            // Already over: a terminal row must not stop the live ones being canceled.
            if !active(o) {
                continue;
            }
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or("Phemex cancel market not found")?;
            let pos = if let Some(p) = self
                .trading
                .sides
                .get(&base)
                .filter(|_| self.live_order_metadata(cx, &base))
            {
                p.clone()
            } else if let Some(p) = self.account.order_pos_side(&base) {
                p.to_owned()
            } else {
                self.account
                    .context(&o.symbol)
                    .ok_or("Phemex cancel position mode is unknown")?;
                let mode = self
                    .account
                    .mode(&o.symbol)
                    .ok_or("Phemex cancel position mode is unknown")?;
                if mode == PositionMode::Oneway {
                    "Merged".into()
                } else if (o.side == OrderSide::Buy) != o.reduce_only {
                    "Long".into()
                } else {
                    "Short".into()
                }
            };
            if !matches!(pos.as_str(), "Merged" | "Long" | "Short") {
                return Err("Phemex cancel position side is invalid".into());
            }
            if let Some(g) = groups
                .iter_mut()
                .find(|g| g.0 == self.native_symbol(&m.symbol) && g.1 == pos)
            {
                g.2.push(base);
            } else {
                groups.push((self.native_symbol(&m.symbol).to_owned(), pos, vec![base]));
            }
        }
        let mut steps = VecDeque::new();
        for (symbol, pos, ids) in groups {
            let single = ids.len() == 1;
            for chunk in ids.chunks(10) {
                let before = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        native(&o.id)
                            .ok()
                            .is_some_and(|id| chunk.iter().any(|x| x == id))
                            && active(o)
                    })
                    .cloned()
                    .collect();
                steps.push_back(Step {
                    method: Method::Delete,
                    path: if single {
                        "/g-orders/cancel"
                    } else {
                        "/g-orders"
                    },
                    query: query(vec![
                        ("symbol", symbol.clone()),
                        ("posSide", pos.clone()),
                        ("orderID", chunk.join_with(",")),
                    ]),
                    call: Call::Cancel {
                        before,
                        ids: chunk.to_vec(),
                    },
                    replacement: None,
                });
            }
        }
        Ok(steps)
    }

    /// Untriggered first, so no conditional can trigger into an active order after that cancel.
    /// https://github.com/phemex/phemex-api-docs/blob/master/Public-Hedged-Perpetual-API.md#cancelall
    fn account_cancel_steps() -> VecDeque<Step> {
        ["true", "false"]
            .map(|untriggered| Step {
                method: Method::Delete,
                path: "/g-orders/all",
                query: query(vec![("untriggered", untriggered.into())]),
                call: Call::CancelAccount,
                replacement: None,
            })
            .into()
    }

    fn amend_plan(
        &self,
        cx: &Cx<'_, Pending>,
        id: String,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<VecDeque<Step>, String> {
        if price.is_none() && amount.is_none() {
            return Err("Phemex amendment requires price or amount".into());
        }
        let base = native(&id)?.to_owned();
        let parent = cx
            .store
            .order(&base)
            .ok_or("Phemex replacement parent order not found")?;
        if self
            .account
            .has_unreconciled_fills_for(&parent.symbol, &base)
        {
            return Err(
                "Phemex fills are awaiting order reconciliation; wait before amending".into(),
            );
        }
        if parent.status != OrderStatus::Open {
            return Err("Phemex replacement requires an open parent order".into());
        }
        if matches!(parent.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && parent
                .extra
                .as_ref()
                .is_some_and(|extra| extra.conditional_order_price.is_some())
        {
            return Err(
                "Phemex imported limit-trigger orders cannot be safely canceled and replaced"
                    .into(),
            );
        }
        if parent.kind == OrderType::TrailingStopLoss {
            return Err(
                "Phemex trailing cancel-replace cannot preserve its original offset".into(),
            );
        }
        let child = cx
            .store
            .order(&id)
            .ok_or("Phemex amendment order not found")?;
        let mut tp = parent.take_profit;
        let mut sl = parent.stop_loss;
        let mut types = self
            .trading
            .brackets
            .get(&base)
            .filter(|_| self.live_order_metadata(cx, &base))
            .copied()
            .unwrap_or(Brackets { tp: None, sl: None });
        for c in cx.store.orders().iter().filter(|o| {
            o.parent_id.as_deref() == Some(base.as_str()) || o.id.starts_with(&format!("{base}__"))
        }) {
            if c.kind == OrderType::TakeProfit {
                tp = Some(c.price);
                types.tp = c.trigger_type.or(types.tp);
            }
            if c.kind == OrderType::StopLoss {
                sl = Some(c.price);
                types.sl = c.trigger_type.or(types.sl);
            }
        }
        if tp.is_some() && types.tp.is_none() || sl.is_some() && types.sl.is_none() {
            return Err("Phemex cannot preserve imported bracket trigger type".into());
        }
        let (original_tp, original_sl) = (tp, sl);
        let child_amend = id != base;
        if child_amend && amount.is_some() {
            return Err("Phemex virtual bracket quantity amendment is unsupported".into());
        }
        if child_amend {
            match child.kind {
                OrderType::TakeProfit => tp = price,
                OrderType::StopLoss => sl = price,
                _ => return Err("Phemex invalid virtual bracket".into()),
            }
        }
        // Hedged rows report reduceOnly false, so the row's flag would turn a close's
        // replacement into an opening order on the other side.
        let position_side = self
            .trading
            .sides
            .get(&base)
            .filter(|_| self.live_order_metadata(cx, &base))
            .map(String::as_str)
            .or_else(|| self.account.order_pos_side(&base));
        let reduce_only = match position_side {
            Some("Long") => parent.side == OrderSide::Sell,
            Some("Short") => parent.side == OrderSide::Buy,
            Some(_) => parent.reduce_only,
            None if self.account.mode(&parent.symbol) == Some(PositionMode::Hedged) => {
                return Err("Phemex amendment position side is unknown".into());
            }
            None => parent.reduce_only,
        };
        let opts = PlaceOrderOpts {
            symbol: parent.symbol.clone(),
            kind: parent.kind,
            side: parent.side,
            // A resting stop reports leavesQty 0; zero here would mean a full close.
            amount: amount
                .map(|total| num::left_of_total(total, parent.filled))
                .transpose()?
                .unwrap_or(
                    if matches!(parent.kind, OrderType::StopLoss | OrderType::TakeProfit)
                        && parent.filled == 0.
                    {
                        parent.amount
                    } else {
                        parent.remaining
                    },
                ),
            price: Some(if child_amend {
                parent.price
            } else {
                price.unwrap_or(parent.price)
            }),
            trigger_price: None,
            max_slippage_pct: None,
            stop_loss: sl,
            take_profit: tp,
            reduce_only: Some(reduce_only),
            time_in_force: parent.time_in_force,
            trigger_type: parent.trigger_type,
            tp_trigger_type: if parent.kind == OrderType::TakeProfit {
                Some(
                    parent
                        .trigger_type
                        .ok_or("Phemex cannot preserve standalone take-profit trigger type")?,
                )
            } else {
                types.tp
            },
            sl_trigger_type: if parent.kind == OrderType::StopLoss {
                Some(
                    parent
                        .trigger_type
                        .ok_or("Phemex cannot preserve standalone stop-loss trigger type")?,
                )
            } else {
                types.sl
            },
            client_order_id: None,
            close: parent.close,
            stp_mode: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
        };
        let stop = self.stop_protect(cx, &opts, parent, child, original_tp, original_sl)?;
        let mut replacement = self.place_plan(cx, vec![opts])?;
        if let Some(step) = replacement.front_mut() {
            step.replacement = Some(Guard {
                id: base.clone(),
                symbol: parent.symbol.clone(),
                side: child.side,
                kind: child.kind,
                amount: parent.amount,
                filled: parent.filled,
                stop,
            });
        }
        let mut cancel = self.cancel_plan(cx, vec![base])?;
        cancel.extend(replacement);
        Ok(cancel)
    }

    /// For a stop, take-profit or trigger edit: the final pre-cancel check and
    /// the original's exact put-back (`opts` is the edited replacement).
    fn stop_protect(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        parent: &Order,
        child: &Order,
        original_tp: Option<f64>,
        original_sl: Option<f64>,
    ) -> Result<Option<Box<StopProtect>>, String> {
        let leg = child.id != parent.id;
        if !leg
            && !matches!(
                parent.kind,
                OrderType::StopLoss | OrderType::TakeProfit | OrderType::Conditional
            )
        {
            return Ok(None);
        }
        let m = cx
            .store
            .market(&parent.symbol)
            .ok_or("Phemex amendment market not found")?;
        let trigger = num::adjust(
            if leg {
                if child.kind == OrderType::TakeProfit {
                    opts.take_profit
                } else {
                    opts.stop_loss
                }
            } else {
                opts.price
            }
            .ok_or("Phemex amendment has no trigger")?,
            m.precision.price,
        );
        let stops = matches!(child.kind, OrderType::StopLoss | OrderType::Conditional);
        // A leg acts only once its entry fills; a standalone stop acts at once.
        let live = !leg || parent.filled > 0.0;
        let below = live.then_some(stops == (child.side == OrderSide::Sell));
        let closes =
            (!leg && (opts.reduce_only == Some(true) || opts.close == Some(true))).then(|| {
                if child.side == OrderSide::Sell {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                }
            });
        // The source the replacement is sent with, defaulted as placement does,
        // so the veto checks the price the venue will trigger on.
        let reference = match child.kind {
            OrderType::TakeProfit => opts.tp_trigger_type.unwrap_or(StopTriggerType::LastPrice),
            OrderType::StopLoss => opts.sl_trigger_type.unwrap_or(StopTriggerType::MarkPrice),
            _ => parent.trigger_type.unwrap_or(StopTriggerType::LastPrice),
        };
        let guard = StopGuard {
            order: child.clone(),
            parent: leg.then(|| parent.clone()),
            trigger,
            reference,
            below,
            closes,
        };
        guard
            .validate(cx.store, cx.now)
            .map_err(|error| error.message)?;
        let original = PlaceOrderOpts {
            price: Some(parent.price),
            take_profit: original_tp,
            stop_loss: original_sl,
            amount: if matches!(parent.kind, OrderType::StopLoss | OrderType::TakeProfit)
                && parent.filled == 0.
            {
                parent.amount
            } else {
                parent.remaining
            },
            ..opts.clone()
        };
        let restore = self.place_plan(cx, vec![original])?;
        let restorable = !leg
            && original_tp.is_none()
            && original_sl.is_none()
            && parent.filled == 0.0
            && restore.len() == 1;
        Ok(Some(Box::new(StopProtect {
            guard,
            restore,
            restorable,
        })))
    }
    /// The final-claim re-check of an amended stop's cancel.
    pub(crate) fn stop_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), VenueError> {
        let Pending::PlannedTrade(request) = pending else {
            return Ok(());
        };
        let Call::Cancel { .. } = &request.native.call else {
            return Ok(());
        };
        let Some(job) = request
            .native
            .command
            .and_then(|id| self.trading.jobs.get(&id))
        else {
            return Ok(());
        };
        if job.restoring {
            return Ok(());
        }
        let Some(protect) = job
            .steps
            .front()
            .and_then(|step| step.replacement.as_ref())
            .and_then(|guard| guard.stop.as_ref())
        else {
            return Ok(());
        };
        if self
            .account
            .has_unreconciled_fills_for(&protect.guard.order.symbol, &protect.guard.order.id)
        {
            return Err(safe_cex_engine::protective_amend::refused(Some(
                "Phemex fills are awaiting order reconciliation; order was not changed".into(),
            )));
        }
        protect.guard.validate(store, release.time.raw_ms)
    }

    fn settings_plan(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: String,
        leverage: Option<f64>,
        side: Option<PositionSide>,
        margin: Option<MarginType>,
    ) -> Result<VecDeque<Step>, String> {
        let m = cx
            .store
            .market(&symbol)
            .ok_or("Phemex settings market not found")?;
        let context = self
            .account
            .context(&symbol)
            .ok_or("Phemex current leverage/margin context is unknown")?;
        let margin = margin.unwrap_or(context.margin_type);
        if !m.limits.leverage.min.is_finite()
            || !m.limits.leverage.max.is_finite()
            || m.limits.leverage.min <= 0.0
            || m.limits.leverage.min > m.limits.leverage.max
        {
            return Err("Phemex market leverage limits are invalid".into());
        }
        let changing_leverage = leverage.is_some();
        let side = side.filter(|_| context.mode == PositionMode::Hedged);
        let long = if side == Some(PositionSide::Short) {
            context.long_leverage
        } else {
            leverage.unwrap_or(context.long_leverage)
        };
        let short = if side == Some(PositionSide::Long)
            || context.mode == PositionMode::Hedged && !changing_leverage
        {
            context.short_leverage
        } else {
            leverage.unwrap_or(context.long_leverage)
        };
        if !long.is_finite() || long <= 0.0 || !short.is_finite() || short <= 0.0 {
            return Err("Phemex current side leverage is unknown or invalid".into());
        }
        let long = if changing_leverage && side != Some(PositionSide::Short) {
            long.clamp(m.limits.leverage.min, m.limits.leverage.max)
        } else {
            long
        };
        let short = if changing_leverage && side != Some(PositionSide::Long) {
            short.clamp(m.limits.leverage.min, m.limits.leverage.max)
        } else {
            short
        };
        let signed = |value: f64| {
            if margin == MarginType::Cross {
                -value
            } else {
                value
            }
        };
        let mut params = vec![("symbol", self.native_symbol(&m.symbol).to_owned())];
        if context.mode == PositionMode::Hedged {
            params.push(("longLeverageRr", num::format(signed(long))));
            params.push(("shortLeverageRr", num::format(signed(short))));
        } else {
            params.push(("leverageRr", num::format(signed(long))));
        }
        Ok(VecDeque::from([Step {
            method: Method::Put,
            path: "/g-positions/leverage",
            query: query(params),
            call: Call::Settings {
                symbol,
                leverage: changing_leverage.then_some(if side == Some(PositionSide::Short) {
                    short
                } else {
                    long
                }),
                side,
                margin: Some(margin),
                mode: None,
            },
            replacement: None,
        }]))
    }

    fn live_acknowledged(&self, cx: &Cx<'_, Pending>, id: &str) -> bool {
        self.trading.acknowledged.contains_key(id)
            && !cx
                .store
                .orders()
                .iter()
                .any(|order| order.id == id || order.parent_id.as_deref() == Some(id))
    }
    fn live_order_metadata(&self, cx: &Cx<'_, Pending>, id: &str) -> bool {
        cx.store.order(id).is_some_and(active) || self.live_acknowledged(cx, id)
    }
    /// A symbol this command needs whose position mode was never read: the
    /// account read lists only symbols the account has touched.
    fn unread_symbol(
        &self,
        cx: &Cx<'_, Pending>,
        command: &Command,
        tried: &[String],
    ) -> Option<String> {
        let unread = |symbol: &String, settings: bool| {
            !tried.contains(symbol)
                && cx.store.market(symbol).is_some()
                && if settings {
                    self.account.context(symbol).is_none()
                } else {
                    self.account.mode(symbol).is_none()
                }
        };
        match command {
            Command::PlaceOrders { orders, .. } => orders
                .iter()
                .map(|o| &o.symbol)
                .find(|symbol| unread(symbol, false))
                .cloned(),
            Command::SetLeverage { symbol, .. } | Command::SetMarginType { symbol, .. } => {
                unread(symbol, true).then(|| symbol.clone())
            }
            _ => None,
        }
    }
    fn read_symbol_mode(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
        symbol: String,
        resume: Option<Preflight>,
        tried: Vec<String>,
    ) -> Result<(), Command> {
        let Some((credential, identity)) = self
            .read_slots
            .select(CredentialRole::Mutation)
            .ok()
            .map(|(selection, identity)| {
                (
                    selection.credential(identity.credential.clone()),
                    identity.clone(),
                )
            })
        else {
            return Err(command);
        };
        let preflight = match resume {
            Some(preflight) => preflight,
            None => {
                let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: Some(credential.clone()),
                    write_credential: Some(credential.clone()),
                }) else {
                    return Ok(());
                };
                preflight
            }
        };
        let context = match cx.preflight_read_context(
            &preflight,
            safe_cex_engine::request_context::Policy {
                operation: Operation::Read,
                credential: Some(credential),
                timeout_ms: 20_000,
                pacing: None,
                retry: safe_cex_engine::request_context::RetryPolicy::Never,
            },
            safe_cex_engine::request_context::plan::CallTerms {
                wait_ms: 0,
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: safe_cex_engine::request_context::plan::RetryWindow::FreshAttempt,
            },
        ) {
            Ok(context) => context,
            Err(error) => {
                reject(cx, &preflight, not_sent(error.venue_error().message));
                return Ok(());
            }
        };
        let currency = if symbol.ends_with("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let path = "/g-accounts/accountPositions";
        let query = query(vec![
            ("currency", currency.to_owned()),
            ("symbol", self.native_symbol(&symbol).to_owned()),
        ]);
        let recipe = crate::dispatch_signing::Http::new(
            identity,
            self.options.testnet,
            Method::Get,
            path.into(),
            query.clone(),
            None,
        );
        let url = format!("{}{path}?{query}", self.rest());
        let pending = Pending::SymbolMode(Box::new(SymbolModeRead {
            preflight,
            context: context.clone(),
            command,
            symbol,
            tried,
        }));
        if let Err((Pending::SymbolMode(read), error)) = cx.request_signed_managed(
            context,
            None,
            Method::Get,
            url,
            None,
            Box::new(recipe),
            pending,
        ) {
            self.symbol_mode_failed(cx, *read, &error.venue_error().message);
        }
        Ok(())
    }
    pub(crate) fn symbol_mode_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: SymbolModeRead,
        error: &str,
    ) {
        reject(
            cx,
            &read.preflight,
            not_sent(format!(
                "Phemex could not read the position mode of {}: {error}",
                read.symbol
            )),
        );
    }
    pub(crate) fn symbol_mode_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: SymbolModeRead,
        body: &str,
    ) {
        let SymbolModeRead {
            preflight,
            context,
            command,
            symbol,
            mut tried,
        } = read;
        if cx.finish_preflight_call(&preflight, &context).is_err() {
            return;
        }
        self.install_symbol_mode(cx, &symbol, body);
        tried.push(symbol);
        let _ = self.trade_command_resumed(cx, command, Some(preflight), tried);
    }
    pub(crate) fn trade_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Option<Command> {
        self.trade_command_resumed(cx, command, None, Vec::new())
    }
    fn trade_command_resumed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
        resume: Option<Preflight>,
        tried: Vec<String>,
    ) -> Option<Command> {
        let command = if !self.options.public_data_only
            && !cx.frame_fed()
            && self.credentials.is_some()
            && let Some(symbol) = self.unread_symbol(cx, &command, &tried)
        {
            match self.read_symbol_mode(cx, command, symbol, resume.clone(), tried) {
                Ok(()) => return None,
                Err(command) => command,
            }
        } else {
            command
        };
        let is_settings = matches!(
            command,
            Command::SetLeverage { .. }
                | Command::SetMarginType { .. }
                | Command::SetPositionMode { .. }
        );
        let depends_settings = matches!(
            command,
            Command::PlaceOrders { .. } | Command::AmendOrder { .. }
        );
        let placement = matches!(command, Command::PlaceOrders { .. });
        let cancel_all = match &command {
            Command::CancelSymbolOrders { symbol, .. } => Some(symbol.clone()),
            _ => None,
        };
        let (id, planned) = match command {
            Command::CancelOrdersWithSymbols { command, .. }
            | Command::FetchStpMode { command }
            | Command::SetStpMode { command, .. }
            | Command::FetchAccount { command }
            | Command::FetchOrderBook { command, .. }
            | Command::FetchLeverage { command, .. } => {
                (command, Err("Phemex command is unsupported".into()))
            }
            Command::PlaceOrders { command, orders } => {
                (
                    command,
                    if self
                        .trading
                        .acknowledged
                        .keys()
                        .filter(|id| {
                            !cx.store
                                .orders()
                                .iter()
                                .any(|o| o.id == **id || o.parent_id.as_ref() == Some(*id))
                        })
                        .count()
                        >= 4096
                    {
                        Err("Phemex unobserved placement capacity exceeded; reconcile before placing".into())
                    } else {
                        self.place_plan(cx, orders)
                    },
                )
            }
            Command::CancelOrders { command, ids } => (command, self.cancel_plan(cx, ids)),
            Command::Venue {
                command,
                action: safe_cex_engine::VenueAction::CancelAccountOrders,
            } => (command, Ok(Self::account_cancel_steps())),
            Command::CancelSymbolOrders { command, symbol } => {
                let ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol && active(o))
                    .map(|o| o.id.clone())
                    .chain(
                        self.trading
                            .acknowledged
                            .iter()
                            .filter(|(id, o)| {
                                o.symbol == symbol
                                    && !cx.store.orders().iter().any(|current| {
                                        current.id == ***id
                                            || current.parent_id.as_ref() == Some(*id)
                                    })
                            })
                            .map(|(id, _)| id.clone()),
                    )
                    .collect();
                (
                    command,
                    if cx.store.market(&symbol).is_some() {
                        if !self.account.orders_ready() {
                            Err("Phemex order snapshot is not loaded; cannot cancel every symbol order".into())
                        } else {
                            self.cancel_plan(cx, ids)
                        }
                    } else {
                        Err("Phemex market not found".into())
                    },
                )
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.amend_plan(cx, id, price, amount)),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side,
            } => (
                command,
                self.settings_plan(cx, symbol, Some(leverage), side, None),
            ),
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => (
                command,
                self.settings_plan(
                    cx,
                    symbol,
                    None,
                    None,
                    Some(if isolated {
                        MarginType::Isolated
                    } else {
                        MarginType::Cross
                    }),
                ),
            ),
            Command::SetPositionMode {
                command,
                hedged,
                symbol,
            } => {
                let result = (|| {
                    let symbol = symbol
                        .filter(|s| !s.is_empty())
                        .ok_or("Phemex symbol is required for position mode")?;
                    let market = cx
                        .store
                        .market(&symbol)
                        .ok_or("Phemex position mode market not found")?;
                    if !market.active || market.is_read_only == Some(true) {
                        return Err("Phemex position mode market is not tradable".into());
                    }
                    Ok(VecDeque::from([Step {
                        method: Method::Put,
                        path: "/g-positions/switch-pos-mode-sync",
                        query: query(vec![
                            ("symbol", self.native_symbol(&symbol).to_owned()),
                            (
                                "targetPosMode",
                                if hedged { "Hedged" } else { "OneWay" }.into(),
                            ),
                        ]),
                        call: Call::Settings {
                            symbol,
                            leverage: None,
                            side: None,
                            margin: None,
                            mode: Some(if hedged {
                                PositionMode::Hedged
                            } else {
                                PositionMode::Oneway
                            }),
                        },
                        replacement: None,
                    }]))
                })();
                (command, result)
            }
            other => return Some(other),
        };
        if self.trading.jobs.contains_key(&id) {
            cx.notice(
                Level::Error,
                code::ORDER_REJECTED,
                "Phemex command ID is already in flight",
            );
            return None;
        }
        let selected = self.read_slots.select(CredentialRole::Mutation).ok();
        let credential = selected
            .as_ref()
            .map(|(selection, identity)| selection.credential(identity.credential.clone()));
        if credential.is_none() {
            let error = if self.trading.jobs.len() >= 32 {
                not_sent("Phemex too many active trading commands")
            } else {
                not_sent("Phemex private credentials are required").kind(ErrorKind::AuthInvalid)
            };
            let Ok(context) = cx.admit_http_command(safe_cex_engine::request_context::Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: safe_cex_engine::request_context::RetryPolicy::Never,
            }) else {
                return None;
            };
            let _ = cx.command_error_context(&context, error.message.clone());
            let _ = cx.fail_context(&context, error);
            return None;
        }
        let preflight = match resume {
            Some(preflight) => preflight,
            None => {
                let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: credential.clone(),
                }) else {
                    return None;
                };
                preflight
            }
        };
        if self.trading.jobs.len() >= 32 {
            reject(
                cx,
                &preflight,
                not_sent("Phemex too many active trading commands"),
            );
            return None;
        }
        if self.options.public_data_only || self.credentials.is_none() {
            let error = not_sent("Phemex private credentials are required");
            reject(cx, &preflight, error.kind(ErrorKind::AuthInvalid));
            return None;
        }
        if (is_settings || depends_settings) && self.trading.jobs.values().any(|j| j.settings) {
            reject(
                cx,
                &preflight,
                not_sent("Phemex account settings update is already in flight"),
            );
            return None;
        }
        match planned {
            Err(error) => reject(
                cx,
                &preflight,
                not_sent(error).kind(ErrorKind::InvalidInput),
            ),
            Ok(steps) => {
                if steps.is_empty() {
                    if let Some(symbol) = cancel_all {
                        self.orders_cancel_all(cx, &preflight, &symbol);
                    }
                    if cx.done_empty_plan(&preflight, "[]".into()).is_ok() {
                        self.orders_reconcile(cx);
                    }
                    return None;
                }
                let (_, identity) = selected?;
                let (plan, slot) = match cx.admit_serial_plan(
                    &preflight,
                    serial_call(),
                    SerialPolicy {
                        credential: Some(credential.unwrap()),
                        pacing: None,
                        refusals_complete: false,
                    },
                ) {
                    Ok(admission) => admission,
                    Err(error) => {
                        let _ = cx.fail_preflight(&preflight, error.venue_error());
                        return None;
                    }
                };
                self.trading.acknowledged.retain(|id, _| {
                    !cx.store
                        .orders()
                        .iter()
                        .any(|o| o.id == *id || o.parent_id.as_ref() == Some(id))
                });
                self.trading.sides.retain(|id, _| {
                    cx.store.order(id).is_some_and(active)
                        || self.trading.acknowledged.contains_key(id)
                });
                self.trading.brackets.retain(|id, _| {
                    cx.store.order(id).is_some_and(active)
                        || self.trading.acknowledged.contains_key(id)
                });
                let mut placing = Vec::new();
                for step in steps.iter().filter(|_| placement) {
                    if let Call::Place { order, .. } = &step.call
                        && !placing.contains(&order.symbol)
                    {
                        placing.push(order.symbol.clone());
                    }
                }
                let placement_report = (placement && steps.len() > 1).then(|| {
                    WritePlan::new(
                        steps
                            .iter()
                            .enumerate()
                            .filter_map(|(index, step)| match &step.call {
                                Call::Place { order, .. } => Some(Element {
                                    index,
                                    client_order_id: order.client_order_id.clone(),
                                }),
                                _ => None,
                            })
                            .collect(),
                    )
                });
                self.trading.jobs.insert(
                    id,
                    Job {
                        plan,
                        slot: Some(slot),
                        identity: identity.clone(),
                        steps,
                        ids: Vec::new(),
                        placement_report,
                        placement_refusal: None,
                        settings: is_settings,
                        placing,
                        effective: false,
                        revision: 0,
                        signed_until: None,
                        live: None,
                        cancel_wait: None,
                        replacing: None,
                        protect: None,
                        restoring: false,
                    },
                );
                if let Some(symbol) = cancel_all {
                    self.orders_cancel_all(cx, &preflight, &symbol);
                }
                self.trade_next(cx, id);
            }
        }
        None
    }

    fn note_canceled_replacement(&mut self, cx: &mut Cx<'_, Pending>, id: CommandId) {
        let Some(job) = self.trading.jobs.get_mut(&id) else {
            return;
        };
        let Some(guard) = job
            .steps
            .front_mut()
            .and_then(|step| step.replacement.as_mut())
        else {
            return;
        };
        if !cx
            .store
            .order(&guard.id)
            .is_some_and(|order| order.status == OrderStatus::Canceled)
        {
            return;
        }
        if let Some(protect) = guard.stop.take() {
            job.replacing = Some((guard.symbol.clone(), guard.side, guard.kind));
            job.protect = Some(protect);
        }
        if job.protect.is_some()
            && let Some(gone) = job.gone_note(true)
        {
            let _ = cx.present_plan_failure(
                &job.plan,
                format!("{gone}; check Open Orders and re-place it if missing"),
            );
        }
    }

    fn trade_next(&mut self, cx: &mut Cx<'_, Pending>, id: CommandId) {
        self.note_canceled_replacement(cx, id);
        let Some(job) = self.trading.jobs.get_mut(&id) else {
            return;
        };
        let Some(step) = job.steps.pop_front() else {
            let job = self.trading.jobs.remove(&id).unwrap();
            if job.restoring {
                let what = job.replacing.as_ref().map_or_else(
                    || "order".to_owned(),
                    |(symbol, ..)| format!("{symbol} stop"),
                );
                let mut error = VenueError::new(format!(
                    "Your {what} is unchanged: its replacement was refused, so the original was put back as a new order"
                ));
                error.protection = Some(safe_cex_engine::error::Protection::Restored);
                error.presents = true;
                cx.command_error(id, error.message.clone());
                let _ = cx.fail_plan(&job.plan, error.partial_ids(&job.ids));
            } else if let Some(mut error) = job.placement_refusal {
                error.presents = true;
                if let Some(report) = &job.placement_report {
                    error = report.attach(error);
                }
                let _ = cx.fail_plan(&job.plan, error.partial_ids(&job.ids));
            } else {
                let _ = cx.done_plan(&job.plan, serde_json::to_string(&job.ids).unwrap());
            }
            self.orders_reconcile(cx);
            return;
        };
        if cx.now < self.paused_until {
            if job.protect.is_some() {
                let _ = cx.inherit_plan_uncertainty(&job.plan);
                let error = VenueError::new("Phemex requests are rate limited after cancellation")
                    .kind(ErrorKind::RateLimited)
                    .outcome(safe_cex_engine::Outcome::Unknown)
                    .retry_after_ms(self.paused_until - cx.now);
                self.fail_unissued_trade(cx, id, error);
            } else {
                let message = if step.replacement.as_ref().is_some_and(|guard| {
                    cx.store
                        .order(&guard.id)
                        .is_some_and(|order| order.status == OrderStatus::Canceled)
                }) {
                    "Phemex order was canceled; replacement not sent (rate limited); re-place it"
                } else {
                    "Phemex requests are rate limited; mutation was not sent"
                };
                let job = self.trading.jobs.remove(&id).unwrap();
                cx.command_error(id, message);
                let _ = cx.fail_plan(&job.plan, message);
                self.orders_uncertain(cx, job.placing, None, cx.now);
            }
            return;
        }
        if let Some(g) = &step.replacement
            && (!cx.store.order(&g.id).is_some_and(|o| {
                o.status == OrderStatus::Canceled && o.amount == g.amount && o.filled == g.filled
            }) || self.account.has_unreconciled_fills_for(&g.symbol, &g.id))
        {
            self.fail_unissued_trade(
                cx,
                id,
                VenueError::new("Phemex order changed while canceling; replacement was not sent"),
            );
            return;
        }
        // Ordinary amends retain the original set-at-send bookkeeping: a local
        // fill veto above must not claim that an unissued replacement was refused.
        if let Some(guard) = &step.replacement {
            job.replacing = Some((guard.symbol.clone(), guard.side, guard.kind));
        }
        let Some(slot) = job.slot.take() else {
            self.fail_unissued_trade(
                cx,
                id,
                VenueError::new("Phemex replacement has no send authority"),
            );
            return;
        };
        let plan = job.plan.clone();
        let identity = job.identity.clone();
        let context = match cx.plan_leg_context(&plan, &slot) {
            Ok(context) => context,
            Err(error) => {
                self.fail_unissued_trade(cx, id, error.venue_error());
                return;
            }
        };
        if let Call::Place { order, .. } = &step.call {
            if let Some(identity) = &order.client_order_id {
                cx.command_progress(
                    id,
                    vec![OrderIdentityReceipt {
                        symbol: order.symbol.clone(),
                        kind: OrderIdentityKind::ClientOrderId,
                        identity: identity.clone(),
                    }],
                );
            }
            cx.store.upsert_order((**order).clone());
        }
        self.dispatch_planned(cx, id, step, plan, slot, context, identity);
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_planned(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        step: Step,
        plan: Plan,
        slot: Slot,
        context: Context,
        identity: safe_cex_engine::signing::Identity,
    ) {
        if let Call::Cancel { before, .. } = &step.call {
            for order in before {
                cx.store.set_order_status(&order.id, OrderStatus::Canceling);
            }
        }
        // A 408 or 5xx on a put-back is sealed before this venue sees it:
        // say what is unknown rather than repeat the replacement's refusal.
        if let Call::Place { .. } = &step.call
            && let Some(job) = self.trading.jobs.get(&command)
            && job.restoring
            && let Some((symbol, ..)) = &job.replacing
        {
            let words = format!(
                "Your {symbol} stop's replacement was refused and its put-back's outcome is unknown. Check Open Orders"
            );
            let _ = cx.present_plan_failure(&plan, words);
        }
        self.account_write(cx);
        let revision = self.account.order_revision();
        if let Some(job) = self.trading.jobs.get_mut(&command) {
            job.revision = revision;
        }
        let recipe = crate::dispatch_signing::Http::new(
            identity,
            self.options.testnet,
            step.method,
            step.path.into(),
            step.query.clone(),
            None,
        );
        let url = format!(
            "{}{}{}",
            self.rest(),
            step.path,
            if step.query.is_empty() {
                String::new()
            } else {
                format!("?{}", step.query)
            }
        );
        let native = Request {
            command: Some(command),
            call: step.call,
            revision,
            signed_until: None,
            released_request: None,
        };
        if let Some(job) = self.trading.jobs.get_mut(&command) {
            job.live = Some(native.clone());
        }
        let pending = Pending::PlannedTrade(PlannedRequest {
            native,
            plan,
            slot,
            context: context.clone(),
        });
        if let Err((Pending::PlannedTrade(read), error)) = cx.request_signed_managed(
            context,
            None,
            step.method,
            url,
            None,
            Box::new(recipe),
            pending,
        ) {
            self.planned_trade_failed(cx, read, error.venue_error());
        }
    }
    pub(crate) fn planned_trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: PlannedRequest,
        response: Response<'_>,
    ) {
        if cx
            .validate_plan_delivery(&request.plan, &request.context)
            .is_err()
        {
            return;
        }
        let PlannedRequest {
            native,
            plan,
            slot,
            context,
        } = request;
        self.trade_response_inner(cx, native, response, Some((plan, slot, context)));
    }
    pub(crate) fn planned_trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: PlannedRequest,
        mut error: VenueError,
    ) {
        if request
            .native
            .command
            .and_then(|id| self.trading.jobs.get(&id))
            .is_none_or(|job| {
                job.plan.owner() != request.plan.owner() || job.revision != request.native.revision
            })
        {
            return;
        }
        let released = request.native.signed_until.is_some();
        if released {
            error.outcome = Some(safe_cex_engine::Outcome::Unknown);
        }
        self.finish_failed_trade(cx, request.native, error, released);
    }
    pub(crate) fn retire_trading(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        let mut retired = Vec::new();
        self.trading.jobs.retain(|_, job| {
            if !owner.matches_plan(&job.plan) {
                return true;
            }
            if let Some(request) = job.live.take() {
                retired.push(RetiredWrite {
                    request,
                    placing: std::mem::take(&mut job.placing),
                    signed_until: job.signed_until,
                });
            }
            false
        });
        self.trading.retired.extend(retired);
    }
    pub(crate) fn settle_retired_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.restore_retired_trading(cx) {
            self.account_refresh(cx);
            self.orders_reconcile(cx);
        }
    }
    pub(crate) fn restore_retired_trading(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        let retired = std::mem::take(&mut self.trading.retired);
        let restored = !retired.is_empty();
        for retired in retired {
            self.restore_failed_trade(cx, &retired.request);
            if let Some(released_expiry) = retired.request.signed_until {
                let expiry = retired.signed_until.unwrap_or(released_expiry);
                let lost = match &retired.request.call {
                    Call::Place { order, .. } => Some(&**order),
                    _ => None,
                };
                self.note_uncertain_orders(retired.placing, lost, expiry + CLOCK_SKEW_MS);
            }
        }
        restored
    }
    pub(crate) fn trade_signing_release(
        &mut self,
        id: safe_cex_engine::RequestId,
        request: &mut Request,
        release: safe_cex_engine::SigningRelease,
    ) {
        if request
            .released_request
            .is_some_and(|previous| previous != id)
        {
            return;
        }
        let job = if let Some(command) = request.command {
            let Some(job) = self
                .trading
                .jobs
                .get_mut(&command)
                .filter(|job| job.revision == request.revision)
            else {
                return;
            };
            Some(job)
        } else {
            None
        };
        let Some(expiry) = release.raw_expiry_ms() else {
            return;
        };
        request.released_request = Some(id);
        request.signed_until = Some(request.signed_until.map_or(expiry, |old| old.max(expiry)));
        if let Some(job) = job {
            job.signed_until = Some(job.signed_until.map_or(expiry, |old| old.max(expiry)));
            if let Some(live) = &mut job.live {
                live.released_request = request.released_request;
                live.signed_until = request.signed_until;
            }
        }
    }

    fn trade_response_inner(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
        managed: Option<(Plan, Slot, Context)>,
    ) {
        if let Some(job) = request
            .command
            .and_then(|id| self.trading.jobs.get_mut(&id))
        {
            job.live = None;
        }
        let before_ids = request
            .command
            .and_then(|id| self.trading.jobs.get(&id))
            .map_or(0, |job| job.ids.len());
        self.trading.answered = false;
        let mut resolved = false;
        let mut refusal_code = None;
        let mut known_effects = false;
        let http_failure = (!(200..300).contains(&response.status))
            .then(|| serde_json::from_str::<crate::wire::Envelope<'_>>(response.body).ok())
            .flatten();
        let mut classified_http_error = None;
        let result = (|| {
            if !(200..300).contains(&response.status) {
                return Err(format!("HTTP {}: {}", response.status, response.body));
            }
            let env: Envelope = serde_json::from_str(response.body)
                .map_err(|_| "Phemex malformed write acknowledgement")?;
            if env.error.is_some() {
                return Err(format!("Phemex {} {}", env.code.unwrap_or(-1), env.msg));
            }
            let code = env.code.ok_or("Phemex malformed write acknowledgement")?;
            if code == ORDER_NOT_FOUND
                && let Call::Cancel { before, ids } = &request.call
            {
                let ack = gone_ack(ids);
                known_effects = true;
                self.apply_cancel_ack(cx, before, &ack);
                return Ok(());
            }
            // As its open-order list does, OM_ORDER_NOT_FOUND here means nothing rests.
            if code == ORDER_NOT_FOUND
                && let Call::CancelAccount = &request.call
            {
                return Ok(());
            }
            if code != 0 {
                // Only documented balance/parameter/risk rejects with no order data
                // prove no placement. Duplicate/routing/internal errors stay unknown.
                // https://github.com/phemex/phemex-api-docs/blob/master/TradingErrorCode.md
                let call = &request.call;
                self.trading.answered = true;
                if matches!(call, Call::Place { .. })
                    && env.data.as_ref().is_none_or(|data| data.get() == "null")
                    && matches!(
                        code,
                        11001
                            | 11011
                            | 11027
                            | 11034
                            | 11035
                            | 11036
                            | 11038
                            | 11041
                            | 11043
                            | 11044
                            | 11048
                            | 11053
                            | 11055
                            | 11056
                            | 11058
                            | 11081
                            | 11082
                            | 11150
                            | 20004
                            | 39999
                    )
                {
                    request
                        .command
                        .inspect(|&command| cx.placement_resolved(command));
                    resolved = true;
                    refusal_code = Some(NativeCode::Int(code));
                }
                return Err(format!("Phemex {} {}", code, env.msg));
            }
            match &request.call {
                Call::Place {
                    order,
                    pos_side,
                    brackets,
                } => {
                    let raw = env.data.ok_or("Phemex missing create result")?;
                    let identity: Identity = serde_json::from_str(raw.get())
                        .map_err(|_| "Phemex missing create order identity")?;
                    native(&identity.order_id)?;
                    if identity
                        .client_id
                        .as_ref()
                        .is_some_and(|id| Some(id) != order.client_order_id.as_ref())
                        || identity.code != 0
                    {
                        return Err(
                            "Phemex create acknowledgement identity/rejection mismatch".into()
                        );
                    }
                    if identity.ord_status == "Suspended"
                        && identity.exec_status == "PendingAllocate"
                    {
                        return Err("Phemex order suspended; likely insufficient margin".into());
                    }
                    let row: crate::account_wire::OrderRow = serde_json::from_str(raw.get())
                        .map_err(|_| "Phemex malformed create order")?;
                    if row.symbol.to_ascii_uppercase() != order.symbol
                        || crate::account_wire::order_side(&row.side) != Some(order.side)
                        || row.order_qty.or(row.order_qty_rq).is_none() && order.amount > 0.0
                        || row.order_qty.or(row.order_qty_rq).is_some_and(|q| {
                            order.amount > 0.0
                                    && q != order.amount
                                    // The venue trims an oversize reduce-only order.
                                    && !(order.reduce_only && q > 0.0 && q < order.amount)
                        })
                        || !row.pos_side.is_empty() && row.pos_side != pos_side.as_str()
                        || !matches!(
                            row.kind.as_ref(),
                            "Market" | "Limit" | "Stop" | "MarketIfTouched"
                        )
                    {
                        return Err("Phemex create response does not match submitted order".into());
                    }
                    if row.status.eq_ignore_ascii_case("Rejected") {
                        let kind = match order.kind {
                            OrderType::Market => "Market",
                            OrderType::Limit => "Limit",
                            OrderType::TakeProfit => "MarketIfTouched",
                            _ => "Stop",
                        };
                        // Missing correlation is not evidence that our write failed.
                        if identity.client_id.as_ref() == order.client_order_id.as_ref()
                            && identity.client_id.is_some()
                            && row.pos_side == pos_side.as_str()
                            && row.order_qty.or(row.order_qty_rq) == Some(order.amount)
                            && row.kind == kind
                            && native(&identity.order_id)? == identity.order_id
                        {
                            request
                                .command
                                .inspect(|&command| cx.placement_resolved(command));
                            resolved = true;
                            refusal_code = Some(NativeCode::Text("Rejected"));
                        }
                        return Err(format!(
                            "Phemex order rejected: bizError {}",
                            identity.biz_error
                        ));
                    }
                    if identity.biz_error != 0 {
                        return Err("Phemex create acknowledgement rejection mismatch".into());
                    }
                    // Both documented acknowledgement states are transient, not working orders.
                    let pending = matches!(identity.ord_status.as_str(), "Init" | "Created");
                    if !pending {
                        row.orders(cx.now)
                            .ok_or("Phemex unrecognized create order response")?;
                    }
                    cx.store.remove_orders(&[order.id.as_str()]);
                    if !pending && self.account.order_revision() == request.revision {
                        self.accept_write_order(cx, &row);
                    }
                    if cx.store.order(&identity.order_id).is_none() {
                        let mut route = (**order).clone();
                        route.id = identity.order_id.clone();
                        route.status = OrderStatus::Open;
                        self.trading
                            .acknowledged
                            .insert(identity.order_id.clone(), route);
                    }
                    self.trading
                        .sides
                        .insert(identity.order_id.clone(), pos_side.clone());
                    self.trading
                        .brackets
                        .insert(identity.order_id.clone(), *brackets);
                    let job = request
                        .command
                        .and_then(|command| self.trading.jobs.get_mut(&command))
                        .ok_or("Phemex missing write job")?;
                    job.record_placement(
                        &order.client_order_id,
                        ElementResult::Placed {
                            order_ids: vec![identity.order_id.clone()],
                        },
                    );
                    job.ids.push(identity.order_id);
                }
                Call::Cancel { before, ids } => {
                    // A socket frame may beat this ack: apply_cancel_ack only moves
                    // orders still Canceling, so the newer state stands.
                    let raw = env
                        .data
                        .filter(|raw| raw.get() != "null")
                        .ok_or("Phemex missing cancel acknowledgement")?;
                    let ack = decode_cancel_ack(raw.get(), ids)?;
                    known_effects = !ack.accepted.is_empty();
                    self.apply_cancel_ack(cx, before, &ack);
                    if let Some(error) = ack.problem {
                        return Err(error);
                    }
                }
                // Documented `"data":0`: a bare number that names no order.
                // Code 0 is the acknowledgement; data is not a count (shown live).
                Call::CancelAccount => {}
                Call::Settings {
                    symbol,
                    leverage,
                    side,
                    margin,
                    mode,
                } => {
                    if let Some(l) = leverage {
                        cx.store.update_setting(symbol, |s| {
                            if *side != Some(PositionSide::Short) {
                                s.leverage = *l;
                                s.buy_leverage = Some(*l);
                            }
                            if *side != Some(PositionSide::Long) {
                                s.sell_leverage = Some(*l);
                            }
                        });
                        cx.store.update_positions(|p| {
                            if p.symbol == *symbol && side.is_none_or(|selected| selected == p.side)
                            {
                                p.leverage = *l;
                            }
                        });
                    }
                    if let Some(m) = margin {
                        cx.store
                            .update_setting(symbol, |s| s.margin_type = Some(*m));
                    }
                    self.account
                        .apply_settings(symbol, *leverage, *side, *margin, *mode);
                    // Frame-fed: kept over the source's frames until it re-reads.
                    let held = [
                        leverage.map(|_| SettingKey::Leverage(symbol.clone())),
                        margin.map(|_| SettingKey::MarginType(symbol.clone())),
                        mode.map(|_| SettingKey::PositionMode(symbol.clone())),
                    ];
                    for key in held.into_iter().flatten() {
                        cx.hold_setting(key);
                    }
                    if let Some(mode) = mode {
                        cx.store.update_positions(|p| {
                            if p.symbol == *symbol {
                                p.position_mode = Some(*mode);
                            }
                        });
                        cx.store.set_hedged(self.account.any_hedged());
                    }
                }
            }
            Ok(())
        })();
        if result.is_ok()
            && let Some(command) = request.command
        {
            self.note_canceled_replacement(cx, command);
        }
        if let Some((plan, slot, context)) = &managed {
            let fact = match &result {
                Ok(()) => WriteFact::Accepted {
                    order_ids: request
                        .command
                        .and_then(|id| self.trading.jobs.get(&id))
                        .map_or_else(Vec::new, |job| job.ids[before_ids..].to_vec()),
                },
                Err(message) => {
                    let native = http_failure
                        .as_ref()
                        .map(|envelope| Native {
                            code: envelope.code.map_or(NativeCode::None, NativeCode::Int),
                            message: &envelope.msg,
                        })
                        .or_else(|| refusal_code.map(|code| Native { code, message: "" }));
                    let fact = match cx.classify_write(
                        context,
                        &REFUSALS,
                        native.as_ref(),
                        message.clone(),
                    ) {
                        Ok(fact) => {
                            if resolved {
                                fact
                            } else {
                                fact.unproven()
                            }
                        }
                        Err(error) => {
                            self.finish_failed_trade(cx, request, error.venue_error(), true);
                            return;
                        }
                    };
                    if !(200..300).contains(&response.status) {
                        classified_http_error = Some(fact.error().clone());
                    }
                    WriteFact::Failed {
                        fact,
                        known_effects,
                    }
                }
            };
            if let Err(error) = cx.http_write_fact(context, fact, None) {
                self.finish_failed_trade(cx, request, error.venue_error(), true);
                return;
            }
            if result.is_ok() {
                let has_next = request
                    .command
                    .and_then(|id| self.trading.jobs.get(&id))
                    .is_some_and(|job| !job.steps.is_empty());
                let wait_for_cancel = request
                    .command
                    .and_then(|id| self.trading.jobs.get(&id))
                    .and_then(|job| job.steps.front())
                    .and_then(|step| step.replacement.as_ref())
                    .is_some_and(|g| cx.store.order(&g.id).is_some_and(active));
                if wait_for_cancel {
                    match cx.serial_plan_condition_wait(
                        plan,
                        slot,
                        context,
                        serial_call(),
                        Some(CANCEL_CONFIRM_MS),
                    ) {
                        Ok(wait) => {
                            if let Some(job) = request
                                .command
                                .and_then(|id| self.trading.jobs.get_mut(&id))
                            {
                                job.cancel_wait = Some(wait);
                                job.effective = true;
                            }
                            self.account_write(cx);
                            self.replace_wait_refresh(cx);
                            return;
                        }
                        Err(error) => {
                            self.finish_failed_trade(cx, request, error.venue_error(), false);
                            return;
                        }
                    }
                } else if has_next {
                    match cx.continue_serial_plan(plan, slot, context, serial_call()) {
                        Ok(next) => {
                            if let Some(job) = request
                                .command
                                .and_then(|id| self.trading.jobs.get_mut(&id))
                            {
                                job.slot = Some(next);
                            }
                        }
                        Err(error) => {
                            self.finish_failed_trade(cx, request, error.venue_error(), false);
                            return;
                        }
                    }
                } else if let Err(error) = cx.finish_plan_call(plan, slot, context) {
                    self.finish_failed_trade(cx, request, error.venue_error(), true);
                    return;
                }
            } else if resolved
                && let Err(message) = &result
                && let Call::Place { order, .. } = &request.call
                && let Some(job) = request
                    .command
                    .and_then(|id| self.trading.jobs.get_mut(&id))
                && !job.placing.is_empty()
                && !job.steps.is_empty()
            {
                // TS placeOrder keeps later payloads after a proven native refusal.
                match cx.continue_serial_plan(plan, slot, context, serial_call()) {
                    Ok(next) => {
                        let error = VenueError::new(message.clone())
                            .outcome(safe_cex_engine::Outcome::Rejected);
                        job.record_placement(
                            &order.client_order_id,
                            ElementResult::NotSent {
                                error: Some(Box::new(error.clone())),
                            },
                        );
                        job.placement_refusal = Some(error.clone());
                        job.slot = Some(next);
                        let command = request.command;
                        self.restore_failed_trade(cx, &request);
                        if let Some(command) = command {
                            cx.command_error(command, error.message);
                            self.trade_next(cx, command);
                        }
                        return;
                    }
                    Err(error) => {
                        self.finish_failed_trade(cx, request, error.venue_error(), false);
                        return;
                    }
                }
            } else if resolved
                && let Call::Place { .. } = &request.call
                && let Some(job) = request
                    .command
                    .and_then(|id| self.trading.jobs.get_mut(&id))
                && !job.restoring
                && job
                    .protect
                    .as_ref()
                    .is_some_and(|p| p.restorable && !p.restore.is_empty())
            {
                // A definite refusal proves no replacement exists: put the original back once.
                match cx.continue_serial_plan(plan, slot, context, serial_call()) {
                    Ok(next) => {
                        let restore = job.protect.as_mut().map(|p| std::mem::take(&mut p.restore));
                        job.steps = restore.unwrap_or_default();
                        job.restoring = true;
                        job.slot = Some(next);
                        let command = request.command;
                        self.restore_failed_trade(cx, &request);
                        if let Some(command) = command {
                            self.trade_next(cx, command);
                        }
                        return;
                    }
                    Err(error) => {
                        self.finish_failed_trade(cx, request, error.venue_error(), false);
                        return;
                    }
                }
            } else if let Err(error) = cx.finish_plan_call(plan, slot, context) {
                self.finish_failed_trade(cx, request, error.venue_error(), true);
                return;
            }
        }
        match result {
            Ok(()) => {
                if let Some(job) = request
                    .command
                    .and_then(|command| self.trading.jobs.get_mut(&command))
                {
                    job.effective = true;
                }
                self.account_write(cx);
                self.account_refresh(cx);
                if let Some(command) = request.command {
                    self.trade_next(cx, command);
                }
            }
            Err(error) => {
                if let Some(error) = classified_http_error {
                    self.finish_failed_trade(cx, request, error, true);
                    return;
                }
                let outcome = if resolved {
                    Outcome::Rejected
                } else {
                    Outcome::Unknown
                };
                self.trade_failed(cx, request, error, outcome)
            }
        }
    }

    pub(crate) fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: impl Into<String>,
        outcome: Outcome,
    ) {
        let error = VenueError::new(error).outcome(match outcome {
            Outcome::Rejected => safe_cex_engine::Outcome::Rejected,
            Outcome::Unknown => safe_cex_engine::Outcome::Unknown,
        });
        self.finish_failed_trade(cx, request, error, matches!(outcome, Outcome::Unknown));
    }

    fn fail_unissued_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        mut error: VenueError,
    ) {
        let Some(job) = self.trading.jobs.remove(&id) else {
            return;
        };
        let gone = job.gone_note(error.outcome == Some(safe_cex_engine::Outcome::Unknown));
        if job.protect.is_some()
            && let Some(gone) = &gone
        {
            error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
            error.presents = true;
            error.message = format!("{gone}; please re-place it ({})", error.message);
        }
        if job.effective || !job.ids.is_empty() {
            error.outcome = None;
        }
        self.account_refresh(cx);
        cx.command_error(id, error.message.clone());
        let error = error.partial_ids(&job.ids);
        let gone = gone.filter(|_| error.protection.is_none());
        let _ = match &gone {
            Some(gone) => cx.fail_plan_noted(&job.plan, error, gone),
            None => cx.fail_plan(&job.plan, error),
        };
        self.orders_reconcile(cx);
    }

    fn apply_cancel_ack(&mut self, cx: &mut Cx<'_, Pending>, before: &[Order], ack: &CancelAck) {
        let CancelAck {
            accepted,
            finalized,
            gone,
            fills,
            ..
        } = ack;
        for (id, filled) in fills {
            if cx
                .store
                .order(id)
                .is_some_and(|order| *filled > order.filled)
            {
                cx.store.update_order(id, |order| {
                    order.filled = *filled;
                    order.remaining = (order.amount - filled).max(0.0);
                });
            }
        }
        for o in before {
            let base = native(&o.id).ok();
            let status = base.and_then(|id| finalized.get(id)).copied();
            let accepted = base.is_some_and(|id| accepted.contains(id));
            if cx.store.order(&o.id).is_some_and(|current| {
                current.status == OrderStatus::Canceling
                    && current.amount == o.amount
                    && current.filled >= o.filled
            }) {
                if let Some(status) = status {
                    cx.store.set_order_status(&o.id, status);
                } else if !accepted
                    && cx
                        .store
                        .order(&o.id)
                        .is_some_and(|current| current.filled == o.filled)
                {
                    cx.store.set_order_status(&o.id, o.status);
                }
            }
        }
        for id in accepted {
            self.trading.acknowledged.remove(id);
        }
        for id in finalized.keys() {
            self.account.terminal(id, cx.now);
        }
        for o in before {
            if native(&o.id).is_ok_and(|id| gone.contains(id)) {
                cx.store.remove_orders(&[o.id.as_str()]);
            }
        }
        if !accepted.is_empty() {
            self.account_write(cx);
        }
    }
    /// A socket frame may have ended the order a replacement waits on.
    pub(crate) fn wake_replacements(&mut self, cx: &mut Cx<'_, Pending>) {
        for job in self.trading.jobs.values_mut() {
            let settled = job.steps.front().is_none_or(|step| {
                step.replacement
                    .as_ref()
                    .is_none_or(|g| !cx.store.order(&g.id).is_some_and(active))
            });
            if settled
                && let Some(wait) = &job.cancel_wait
                && cx.release_condition_wait(wait).is_ok()
            {
                job.cancel_wait = None;
            }
        }
    }
    pub(crate) fn replacement_wait_expired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::ConditionOwner,
    ) {
        let safe_cex_engine::request_context::ConditionOwner::Plan(plan) = owner else {
            return;
        };
        let Some(id) = self
            .trading
            .jobs
            .iter()
            .find_map(|(id, job)| (job.plan.owner() == plan.owner()).then_some(*id))
        else {
            return;
        };
        let job = self.trading.jobs.remove(&id).unwrap();
        let error = "Phemex cancel was accepted but not confirmed within 10 s; replacement was not sent. Check the order and any position it protected";
        cx.command_error(id, error);
        let _ = cx.fail_plan(&job.plan, error);
        self.orders_reconcile(cx);
    }
    pub(crate) fn trading_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let Some(id) = self
            .trading
            .jobs
            .iter()
            .find_map(|(id, job)| (job.plan.owner() == plan.owner()).then_some(*id))
        else {
            return;
        };
        let Some(slot) = cx
            .plan_slots(plan)
            .ok()
            .and_then(|slots| slots.into_iter().next())
        else {
            return;
        };
        if let Some(job) = self.trading.jobs.get_mut(&id) {
            job.slot = Some(slot);
        }
        self.trade_next(cx, id);
    }
    fn restore_failed_trade(&mut self, cx: &mut Cx<'_, Pending>, request: &Request) {
        self.reopen_failed_trade(cx, request, false);
    }
    /// `not_sent`: the write never left, so its rows are reopened even if they
    /// changed meanwhile (an amend's vetoed cancel).
    fn reopen_failed_trade(&mut self, cx: &mut Cx<'_, Pending>, request: &Request, not_sent: bool) {
        match &request.call {
            Call::Place { order, .. } => cx.store.remove_orders(&[order.id.as_str()]),
            Call::Cancel { before, .. }
                if not_sent || self.account.order_revision() == request.revision =>
            {
                for o in before {
                    if cx.store.order(&o.id).is_some_and(|current| {
                        current.status == OrderStatus::Canceling
                            && (not_sent
                                || current.amount == o.amount && current.filled == o.filled)
                    }) {
                        cx.store.set_order_status(&o.id, o.status);
                    }
                }
            }
            _ => {}
        }
    }

    fn finish_failed_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
        uncertain: bool,
    ) {
        self.reopen_failed_trade(
            cx,
            &request,
            error.outcome == Some(safe_cex_engine::Outcome::NotSent),
        );
        let Some(command) = request.command else {
            return;
        };
        let mut job = self.trading.jobs.remove(&command);
        self.account_refresh(cx);
        let gone = job.as_ref().and_then(|job| {
            job.gone_note(uncertain || error.outcome == Some(safe_cex_engine::Outcome::Unknown))
        });
        if let Some(job) = &mut job
            && job.placement_report.is_some()
        {
            let unknown = error.outcome == Some(safe_cex_engine::Outcome::Unknown);
            if unknown {
                error.message = "The outcome is unknown. Check Open Orders".into();
                error.presents = true;
                cx.event(
                    Level::Warn,
                    "order.outcome_unknown",
                    &error.message,
                    vec![
                        ("command", (command as f64).into()),
                        ("outcome", "unknown".into()),
                    ],
                );
            }
            if error.outcome == Some(safe_cex_engine::Outcome::Rejected) {
                error.presents = true;
            }
            // Observe the submitted identity; this journal has no dispatch authority.
            if let Call::Place { order, .. } = &request.call {
                let result = if unknown {
                    ElementResult::Unknown {
                        error: Box::new(error.clone()),
                    }
                } else {
                    ElementResult::NotSent {
                        error: Some(Box::new(error.clone())),
                    }
                };
                job.record_placement(&order.client_order_id, result);
            }
            if let Some(report) = &job.placement_report {
                error = report.attach(error);
            }
        }
        if let Some(job) = &job {
            if !uncertain && (job.effective || !job.ids.is_empty()) {
                error.outcome = None;
            }
            error = error.partial_ids(&job.ids);
            // The cancel landed and nothing stands in its place.
            if gone.is_some() && job.protect.is_some() && !uncertain {
                error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
                error.presents = true;
                if !job.restoring
                    && let Some((symbol, ..)) = &job.replacing
                {
                    error.message = format!(
                        "Your {symbol} stop was removed: its replacement was refused, please re-place it ({})",
                        error.message
                    );
                }
            }
        }
        if let Some(job) = &job {
            cx.command_error(command, error.message.clone());
            let _ = match &gone {
                Some(gone) => cx.fail_plan_noted(&job.plan, error, gone),
                None => cx.fail_plan(&job.plan, error),
            };
        }
        if uncertain {
            let expiry = job
                .as_ref()
                .and_then(|job| job.signed_until)
                .or(request.signed_until);
            let Some(expiry) = expiry else {
                // Unreachable for released recipes: expiry is mandatory and the engine
                // projects it before release. Never fabricate a repair bound if violated.
                cx.notice(
                    Level::Error,
                    code::CHILD_LEG_FAILED,
                    "Phemex missing released signing expiry; automatic reconciliation held",
                );
                return;
            };
            let lost = match &request.call {
                Call::Place { order, .. } => Some(&**order),
                _ => None,
            };
            // An answered write cannot still land: its fence clears on the next read.
            let read_at = if std::mem::take(&mut self.trading.answered) {
                cx.now
            } else {
                expiry + CLOCK_SKEW_MS
            };
            self.orders_uncertain(
                cx,
                job.map(|job| job.placing).unwrap_or_default(),
                lost,
                read_at,
            );
        } else {
            self.orders_uncertain(cx, vec![], None, cx.now);
        }
    }

    /// Cancels orders an unknown-outcome placement left open after a cancel-all.
    pub(crate) fn cancel_late(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, ids: Vec<String>) {
        cx.notice(Level::Warn, code::ORDERS_REPLACED,
            format!(
                "Phemex order {} from an unknown-outcome placement landed after cancel-all on {symbol}; canceling it",
                ids.join_with(", ")
            ),
        );
        match self.cancel_plan(cx, ids) {
            Ok(steps) => {
                self.trading.late.extend(steps);
                self.late_next(cx);
            }
            Err(error) => cx.notice(Level::Error, code::CHILD_LEG_FAILED,
                format!(
                    "Phemex could not cancel late-landing orders on {symbol}: {error}; cancel them manually"
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::signing::{Identities, Identity, KeyRef};
    use safe_cex_engine::{Action, CredentialSlot, Event, Notice, SocketId, TimerId, Venue};

    // Fault injection only: a real admitted Plan whose venue-local slot was lost.
    // No test-only hook is exposed to production or integration consumers.
    struct MissingSlot(PhemexPerpetuals);
    impl Venue for MissingSlot {
        type Pending = Pending;
        fn start(&mut self, _: &mut Cx<'_, Pending>) {}
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, _: Command) {
            let identity = Identity::new(KeyRef("fixture".into()), Some(KeyRef("signing".into())));
            let credential = safe_cex_engine::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: identity.credential.clone(),
            };
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential.clone()),
                })
                .unwrap();
            let (plan, _slot) = cx
                .admit_serial_plan(
                    &preflight,
                    serial_call(),
                    SerialPolicy {
                        credential: Some(credential),
                        pacing: None,
                        refusals_complete: false,
                    },
                )
                .unwrap();
            self.0.trading.jobs.insert(
                1,
                Job {
                    plan,
                    slot: None,
                    identity,
                    steps: VecDeque::from([Step {
                        method: Method::Delete,
                        path: "/g-orders/all",
                        query: String::new(),
                        call: Call::CancelAccount,
                        replacement: None,
                    }]),
                    ids: vec![],
                    placement_report: None,
                    placement_refusal: None,
                    settings: false,
                    placing: vec![],
                    effective: false,
                    revision: 0,
                    signed_until: None,
                    live: None,
                    cancel_wait: None,
                    replacing: None,
                    protect: None,
                    restoring: false,
                },
            );
            self.0.trade_next(cx, 1);
            assert!(!self.0.trading.jobs.contains_key(&1));
        }
        fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    #[test]
    fn a_missing_send_slot_terminates_once_without_io() {
        let mut engine = safe_cex_engine::test_support::seeded_engine(MissingSlot(
            PhemexPerpetuals::new(Default::default()),
        ));
        engine
            .set_identities(Identities {
                primary: Some(Identity::new(
                    KeyRef("fixture".into()),
                    Some(KeyRef("signing".into())),
                )),
                secondary: None,
            })
            .unwrap();
        engine.handle(
            1000.,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "old".into(),
                price: Some(67900.),
                amount: None,
            }),
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 1, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1);
        assert_eq!(
            failures[0].message,
            "Phemex replacement has no send authority"
        );
    }
}
