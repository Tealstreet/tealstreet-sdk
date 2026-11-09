//! Each command is one Preflight and one streamed serial Plan: ordered,
//! non-retried writes, each call bounded on its own.
use crate::{Okx, Pending, sign, wire};
use safe_cex_engine::classify::{
    BatchItem, CodeTable, Intent, Key, Native, NativeCode, Released, Row, SatisfiedIntent, Verdict,
    WriteFact,
};
use safe_cex_engine::log::Level;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    Context, Credential, HttpOwnerRetirement, Operation, Plan, Policy, Preflight, PreflightPolicy,
    PreflightPurpose, ReconciliationData, RetryPolicy, WriteResult,
};
use safe_cex_engine::setting_holds::SettingKey;
use safe_cex_engine::text::JoinWith;
use safe_cex_engine::write_plan::{
    AppliedEffect, Element, ElementOutcome, ElementOutcomes, ElementResult, WritePlan,
};
use safe_cex_engine::{
    Command, CommandId, CredentialSlot, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

const BROKER: &str = "e847386590ce4dBC";
const UNKNOWN_OUTCOME_TEXT: &str = "The outcome is unknown. Check Open Orders";

/// Every write is one call with its own 20 s bound; only a 50113 signature
/// refusal, which executed nothing, is re-signed and sent once more.
const SERIAL: SerialCall = SerialCall {
    auth: safe_cex_engine::request_context::plan::CallAuth::Preflight,
    operation: Operation::Mutation,
    timeout_ms: 20_000,
    attempts: 2,
    pacing: None,
};

/// OKX's native write codes. 51xxx are trading-rule refusals, and the listed
/// 50xxx are documented pre-processing refusals. Unavailable, timeout, busy and
/// system errors prove nothing, and an unlisted code is not proof of refusal.
pub(crate) const CODES: CodeTable = CodeTable {
    // Rows match first to last, so the exact codes below win over the 51xxx range.
    rows: &[
        // docs-v5: filled, canceled or absent. This proves Cancel only, not
        // which terminal state a later order frame or census will publish.
        Row {
            key: Key::Int(51400),
            verdict: Verdict::Satisfied(SatisfiedIntent::Cancel),
            only: Some(Intent::Cancel),
        },
        // Timed out or down, in the trading range: "does not mean that the request
        // was successful or failed".
        unknown(51149, ErrorKind::Timeout),
        unknown(51054, ErrorKind::Timeout),
        unknown(51412, ErrorKind::Timeout),
        unknown(51767, ErrorKind::VenueUnavailable),
        refused(51000, ErrorKind::InvalidInput),
        // Documented definite refusals (docs-v5 "Error Code"): nothing was placed.
        refused(50002, ErrorKind::InvalidInput),
        refused(50006, ErrorKind::InvalidInput),
        refused(50009, ErrorKind::AuthPermission),
        refused(50027, ErrorKind::AuthPermission),
        refused(54000, ErrorKind::Rejected),
        refused(59000, ErrorKind::Rejected),
        // Balance and margin refusals (place, amend, and account-wide).
        refused(51008, ErrorKind::InsufficientBalance),
        refused(51119, ErrorKind::InsufficientBalance),
        refused(51127, ErrorKind::InsufficientBalance),
        refused(51131, ErrorKind::InsufficientBalance),
        refused(51502, ErrorKind::InsufficientBalance),
        refused(59103, ErrorKind::InsufficientBalance),
        refused(59108, ErrorKind::InsufficientBalance),
        refused(59200, ErrorKind::InsufficientBalance),
        // Order rules the order itself broke: unknown instrument, price out of
        // its limit, size below the minimum or off the lot size, market order
        // value or amount over its cap.
        refused(51001, ErrorKind::InvalidInput),
        refused(51006, ErrorKind::InvalidInput),
        refused(51007, ErrorKind::InvalidInput),
        refused(51020, ErrorKind::InvalidInput),
        refused(51100, ErrorKind::InvalidInput),
        refused(51116, ErrorKind::InvalidInput),
        refused(51118, ErrorKind::InvalidInput),
        refused(51120, ErrorKind::InvalidInput),
        refused(51121, ErrorKind::InvalidInput),
        refused(51122, ErrorKind::InvalidInput),
        refused(51137, ErrorKind::InvalidInput),
        refused(51138, ErrorKind::InvalidInput),
        refused(51201, ErrorKind::InvalidInput),
        refused(51202, ErrorKind::InvalidInput),
        refused(51203, ErrorKind::InvalidInput),
        Row {
            key: Key::Range(51000, 51999),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        refused(50014, ErrorKind::InvalidInput),
        refused(50016, ErrorKind::InvalidInput),
        refused(50011, ErrorKind::RateLimited),
        refused(50061, ErrorKind::RateLimited),
        // The read side's documented key refusals (docs-v5 "Error Code", 501xx),
        // pinned equal by account::write_and_read_key_refusals_agree.
        refused(50101, ErrorKind::AuthInvalid),
        refused(50103, ErrorKind::AuthInvalid),
        refused(50104, ErrorKind::AuthInvalid),
        refused(50105, ErrorKind::AuthInvalid),
        refused(50111, ErrorKind::AuthInvalid),
        Row {
            key: Key::Int(50113),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        refused(50114, ErrorKind::AuthInvalid),
        refused(50119, ErrorKind::AuthInvalid),
        refused(50030, ErrorKind::AuthPermission),
        refused(50100, ErrorKind::AuthPermission),
        refused(50110, ErrorKind::AuthPermission),
        refused(50120, ErrorKind::AuthPermission),
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
        (403, Verdict::Refused(ErrorKind::AuthPermission)),
        (429, Verdict::Refused(ErrorKind::RateLimited)),
    ],
};
const fn unknown(code: i64, kind: ErrorKind) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Unknown(Some(kind)),
        only: None,
    }
}
const fn refused(code: i64, kind: ErrorKind) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Refused(kind),
        only: None,
    }
}
pub(crate) fn native<'a>(code: &'a str, message: &'a str) -> Native<'a> {
    Native {
        code: code.parse().map_or(NativeCode::Text(code), NativeCode::Int),
        message,
    }
}

/// One released Plan call, bound to its exact slot and leg.
pub struct Request {
    command: CommandId,
    slot: Slot,
    context: Context,
    path: &'static str,
}
struct Step {
    query: Option<String>,
    receipts: Vec<OrderIdentityReceipt>,
    path: &'static str,
    body: String,
    expected: usize,
    collect_ids: bool,
    write_index: Option<usize>,
    /// Reporting only: the row whose native acknowledgement is unproven.
    unknown_row: Option<usize>,
}
#[derive(Default)]
pub struct TradingState {
    jobs: HashMap<CommandId, Job>,
    pub acknowledged: HashMap<String, Acknowledged>,
    pub(crate) algo_orders: HashMap<String, ObservedAlgo>,
}
pub struct Acknowledged {
    pub(crate) symbol: String,
    inst_id: String,
    algo: bool,
    pub version: u64,
}
struct Job {
    /// An amend that cancels a stop before placing its replacement.
    replacing_stop: Option<(String, OrderSide, OrderType)>,
    trailing: Option<TrailingAmend>,
    lookup: Option<Box<AmendLookup>>,
    /// Set at genuine entry; the Plan follows its admission.
    preflight: Option<Preflight>,
    plan: Option<Plan>,
    steps: VecDeque<Step>,
    writes: Option<WritePlan>,
    pending_write: Option<usize>,
    ids: Vec<String>,
    finish: Finish,
    optimistic: Vec<(String, OrderStatus)>,
    continue_cancel_errors: bool,
    /// A step took effect, so a later rejection cannot mean nothing happened.
    effective: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum LookupStage {
    Order,
    Ticker,
}
struct AmendLookup {
    stage: LookupStage,
    order: Order,
    price: Option<f64>,
    amount: Option<f64>,
}
#[derive(Clone, PartialEq)]
pub(crate) struct ObservedAlgo {
    pub order: Order,
    pub trigger: bool,
    trigger_price: Option<f64>,
    trailing: Option<TrailingOrder>,
}
impl ObservedAlgo {
    pub(crate) fn new(row: &wire::OrderRow<'_>, order: &Order) -> Option<Self> {
        if !matches!(
            row.ord_type,
            "trigger" | "conditional" | "oco" | "move_order_stop"
        ) {
            return None;
        }
        let trailing = (row.ord_type == "move_order_stop").then(|| TrailingOrder {
            inst_id: row.inst_id.into(),
            td_mode: row.td_mode.into(),
            side: row.side.into(),
            pos_side: row.pos_side.into(),
            ccy: row.ccy.into(),
            tag: row.tag.into(),
            ord_type: "move_order_stop",
            sz: row.sz.into(),
            reduce_only: row.reduce_only,
            algo_cl_ord_id: row.algo_cl_ord_id.into(),
            active_px: row.active_px.into(),
            callback_ratio: row.callback_ratio.into(),
            callback_spread: row.callback_spread.into(),
        });
        Some(Self {
            order: order.clone(),
            trigger: row.ord_type == "trigger",
            trigger_price: row
                .move_trigger_px
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n > 0.),
            trailing,
        })
    }
}
#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrailingOrder {
    inst_id: String,
    td_mode: String,
    side: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pos_side: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    ccy: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    tag: String,
    ord_type: &'static str,
    sz: String,
    reduce_only: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    algo_cl_ord_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    active_px: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    callback_ratio: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    callback_spread: String,
}
impl TrailingOrder {
    fn valid(&self) -> bool {
        let positive = |s: &str| s.parse::<f64>().is_ok_and(|n| n.is_finite() && n > 0.);
        matches!(self.td_mode.as_str(), "cross" | "isolated" | "cash")
            && matches!(self.pos_side.as_str(), "" | "net" | "long" | "short")
            && positive(&self.sz)
            && (self.active_px.is_empty() || positive(&self.active_px))
            && ((positive(&self.callback_spread) && self.callback_ratio.is_empty())
                || (positive(&self.callback_ratio) && self.callback_spread.is_empty()))
    }
}
#[derive(PartialEq)]
enum TrailingPhase {
    Cancel,
    Replace,
    Restore,
}
struct TrailingAmend {
    original: ObservedAlgo,
    replacement: TrailingOrder,
    phase: TrailingPhase,
    restore: Option<Step>,
    refusal: Option<VenueError>,
    quote: Option<(f64, f64)>,
}
enum Finish {
    Ids,
    Amend(String),
    Empty,
    Leverage(String, f64),
    Mode(bool),
}

impl Job {
    fn track_writes(&mut self) {
        if !matches!(self.finish, Finish::Ids) || self.trailing.is_some() || self.lookup.is_some() {
            return;
        }
        let mut elements = Vec::new();
        for step in &mut self.steps {
            if !step.collect_ids || step.receipts.is_empty() {
                return;
            }
            step.write_index = Some(elements.len());
            for receipt in &step.receipts {
                elements.push(Element {
                    index: elements.len(),
                    client_order_id: Some(receipt.identity.clone()),
                });
            }
        }
        self.writes = Some(WritePlan::new(elements));
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OrderPayload {
    #[serde(skip)]
    symbol: String,
    inst_id: String,
    td_mode: &'static str,
    tag: &'static str,
    side: OrderSide,
    ord_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cl_ord_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo_cl_ord_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    close_fraction: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_ord_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_ord_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_ratio: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_spread: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cancel<'a> {
    inst_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ord_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo_id: Option<&'a str>,
}
/// The id a cancel request named.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Requested {
    ord_id: String,
    algo_id: String,
}
impl Requested {
    fn id(&self) -> &str {
        if self.algo_id.is_empty() {
            &self.ord_id
        } else {
            &self.algo_id
        }
    }
}

const SIMPLE_MODE_HEDGE: &str =
    "OKX hedge mode needs a margin account mode; switch it in OKX Settings (Trading mode)";

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ack<'a> {
    #[serde(default)]
    ord_id: &'a str,
    #[serde(default)]
    algo_id: &'a str,
    #[serde(default)]
    s_code: &'a str,
    #[serde(default)]
    s_msg: &'a str,
}

impl Okx {
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (id, result) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, orders)),
            Command::CancelOrders { command, ids } => (command, self.plan_cancel(cx, ids)),
            Command::CancelSymbolOrders { command, symbol } => {
                let mut ids: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.symbol == symbol
                            && matches!(
                                o.status,
                                OrderStatus::Open | OrderStatus::Creating | OrderStatus::Updating
                            )
                    })
                    .map(|o| o.id.clone())
                    .collect();
                ids.extend(
                    self.trading
                        .acknowledged
                        .iter()
                        .filter(|(_, order)| order.symbol == symbol)
                        .map(|(id, _)| id.clone()),
                );
                (command, self.plan_cancel(cx, ids))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            // The engine binds the command when the read is admitted.
            Command::FetchLeverage { command: _, symbol } => {
                self.fetch_leverage(cx, symbol);
                return;
            }
            Command::SetMarginType { .. } => {
                self.refuse_entry(
                    cx,
                    not_sent("Only cross mode is supported").kind(ErrorKind::Unsupported),
                );
                return;
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Leverage<'a> {
                    inst_id: &'a str,
                    lever: String,
                    mgn_mode: &'static str,
                }
                let result = (|| {
                    let m = cx
                        .store
                        .market(&symbol)
                        .ok_or_else(|| format!("unknown market {symbol}"))?;
                    if !leverage.is_finite() {
                        return Err("leverage must be finite".into());
                    }
                    let leverage = num::clamp_leverage(
                        leverage,
                        m.limits.leverage.min,
                        m.limits.leverage.max,
                    )?;
                    let step = step(
                        "/api/v5/account/set-leverage",
                        &Leverage {
                            inst_id: &m.id,
                            lever: num::format(leverage),
                            mgn_mode: "cross",
                        },
                        0,
                        false,
                    )?;
                    Ok(job(vec![step], Finish::Leverage(symbol, leverage)))
                })();
                (command, result)
            }
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Mode {
                    pos_mode: &'static str,
                }
                // OKX keeps a spot-mode (acctLv 1) account one-way: the write would
                // not take, and the next config read would put one-way back.
                let result = if hedged && self.account.account_level == 1 {
                    Err(SIMPLE_MODE_HEDGE.into())
                } else if cx.store.positions().iter().any(|p| p.contracts > 0.0) {
                    Err("Please close all positions before switching position mode".into())
                } else {
                    step(
                        "/api/v5/account/set-position-mode",
                        &Mode {
                            pos_mode: if hedged {
                                "long_short_mode"
                            } else {
                                "net_mode"
                            },
                        },
                        0,
                        false,
                    )
                    .map(|s| job(vec![s], Finish::Mode(hedged)))
                };
                (command, result)
            }
            _ => return,
        };
        if self.credentials.is_none() || !self.account.ready {
            self.refuse_entry(cx, not_sent("OKX account is not ready"));
            return;
        }
        match result {
            Ok(job) => self.start_job(cx, id, job),
            Err(error) => self.refuse_entry(cx, not_sent(error).kind(ErrorKind::InvalidInput)),
        }
    }

    fn primary_credential(&self) -> Option<Credential> {
        self.credentials.as_ref().map(|identity| Credential {
            slot: CredentialSlot::Primary,
            reference: identity.credential.clone(),
        })
    }

    /// Genuine entry, then one serial Plan per command. Native optimistic
    /// status follows a successful admission, never a refused one.
    fn start_job(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, mut job: Job) {
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: job.lookup.as_ref().and_then(|_| self.primary_credential()),
            write_credential: self.primary_credential(),
        }) else {
            return;
        };
        job.track_writes();
        if job.steps.is_empty() {
            let _ = match self.finish_effects(cx, &mut job) {
                Ok(data) => cx.done_empty_plan(&preflight, data),
                Err(error) => cx.fail_preflight(&preflight, VenueError::new(error)),
            };
            return;
        }
        let Some(credential) = self.primary_credential() else {
            let _ = cx.fail_preflight(&preflight, not_sent("OKX account is not ready"));
            return;
        };
        let policy = SerialPolicy {
            credential: Some(credential),
            pacing: None,
            refusals_complete: true,
        };
        let call = next_call(&job);
        let (plan, slot) = match cx.admit_serial_plan(&preflight, call, policy) {
            Ok(admitted) => admitted,
            Err(reason) => {
                let _ = cx.fail_preflight(&preflight, not_sent(reason.venue_error().message));
                return;
            }
        };
        for (order, status) in &mut job.optimistic {
            if let Some(current) = cx.store.order(order) {
                *status = current.status;
            }
            cx.store.set_order_status(
                order,
                if matches!(job.finish, Finish::Amend(_)) {
                    OrderStatus::Updating
                } else {
                    OrderStatus::Canceling
                },
            );
        }
        job.preflight = Some(preflight);
        job.plan = Some(plan);
        self.trading.jobs.insert(command, job);
        self.issue(cx, command, slot);
    }

    /// A write refused before I/O admits a terminal-only Mutation owner, so the
    /// engine caps it at NotSent. A rejected admission already failed the command.
    fn refuse_entry(&self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        let credential = self.credentials.as_ref().map(|identity| Credential {
            slot: CredentialSlot::Primary,
            reference: identity.credential.clone(),
        });
        if let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Mutation,
            credential,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) {
            let _ = cx.fail_context(&context, error);
        }
    }

    fn plan_place(&self, cx: &Cx<'_, Pending>, orders: Vec<PlaceOrderOpts>) -> Result<Job, String> {
        if orders.iter().any(|order| {
            order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
        }) {
            return Err("Bybit execution controls are supported only on Bybit".into());
        }
        if orders
            .iter()
            .any(|order| order.implied_volatility.is_some())
        {
            return Err("Implied volatility is supported only for options".into());
        }
        let mut seen = HashSet::new();
        for o in &orders {
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found on OKX", o.symbol))?;
            if let Some(id) = &o.client_order_id {
                if id.is_empty()
                    || id.len() > 32
                    || !id.bytes().all(|c| c.is_ascii_alphanumeric())
                    || !seen.insert(id)
                {
                    return Err(format!("Invalid or duplicated OKX client order ID {id}"));
                }
                if !matches!(o.kind, OrderType::Limit | OrderType::Market)
                    || o.trigger_price.is_some()
                    || o.stop_loss.is_some()
                    || o.take_profit.is_some()
                    || o.amount > m.limits.amount.max
                {
                    return Err(
                        "Caller client order ID requires exactly one plain native order".into(),
                    );
                }
            }
        }
        let mut regular = Vec::new();
        let mut algos = Vec::new();
        let mut derived = Vec::new();
        for o in orders {
            if is_algo(o.kind) {
                algos.push(o);
                continue;
            }
            let m = cx.store.market(&o.symbol).expect("preflight market");
            if !m.limits.amount.max.is_finite()
                || m.limits.amount.max <= 0.0
                || m.precision.amount <= 0.0
            {
                return Err("invalid market size limits".into());
            }
            let amount = adjusted(o.amount, m.precision.amount, false)?;
            if !amount.is_finite() {
                return Err("Order amount must be finite after adjustment".into());
            }
            let max = lot_floor(m.limits.amount.max, m.precision.amount)?;
            if o.client_order_id.is_some() && amount > max {
                return Err(
                    "Caller client order ID requires exactly one plain native order".into(),
                );
            }
            if max <= 0.0 || amount / max > 10_000.0 {
                return Err("order exceeds safe split bounds".into());
            }
            let bracket =
                o.stop_loss.is_some_and(|v| v != 0.0) || o.take_profit.is_some_and(|v| v != 0.0);
            let mut remaining = amount;
            let mut count = 0;
            while remaining > 0.0 {
                let size = remaining.min(max);
                let payload = self.order_payload(cx, &o, size)?;
                count += 1;
                regular.push(payload);
                remaining = num::adjust(remaining - size, m.precision.amount);
            }
            if o.client_order_id.is_some() && count != 1 {
                return Err(
                    "Caller client order ID requires exactly one plain native order".into(),
                );
            }
            if bracket {
                // Legacy TS derives full-input protection after formatting all entry lots.
                for (kind, price) in [
                    (OrderType::StopLoss, o.stop_loss),
                    (OrderType::TakeProfit, o.take_profit),
                ] {
                    if let Some(price) = price.filter(|v| *v != 0.0) {
                        let mut leg = o.clone();
                        leg.kind = kind;
                        leg.side = o.side.opposite();
                        leg.price = Some(price);
                        leg.client_order_id = None;
                        leg.stop_loss = None;
                        leg.take_profit = None;
                        leg.reduce_only = Some(true);
                        derived.push(leg);
                    }
                }
            }
        }
        algos.extend(derived);
        algos.sort_by_key(|o| match o.kind {
            OrderType::StopLoss => 0,
            OrderType::TrailingStopLoss => 1,
            OrderType::TakeProfit => 2,
            _ => 3,
        });
        // Build every payload before issuing a write, including protective legs.
        let algos = algos
            .into_iter()
            .map(|o| self.order_payload(cx, &o, o.amount))
            .collect::<Result<Vec<_>, _>>()?;
        let mut steps = Vec::new();
        for batch in regular.chunks(20) {
            steps.push(placement_step(
                "/api/v5/trade/batch-orders",
                batch,
                batch.len(),
                true,
            )?);
        }
        for payload in algos {
            steps.push(placement_step(
                "/api/v5/trade/order-algo",
                &payload,
                1,
                true,
            )?);
        }
        Ok(job(steps, Finish::Ids))
    }

    fn order_payload(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        amount: f64,
    ) -> Result<OrderPayload, String> {
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("unknown market {}", o.symbol))?;
        let algo = is_algo(o.kind);
        let size = adjusted(
            amount,
            m.precision.amount,
            algo && o.kind != OrderType::Conditional,
        )?;
        if o.kind == OrderType::Conditional && !size.is_finite() {
            return Err("Conditional amount must be finite after adjustment".into());
        }
        let protective = matches!(
            o.kind,
            OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
        );
        let closing =
            protective || (o.kind != OrderType::Conditional && o.reduce_only.unwrap_or(false));
        let pos_side = if cx.store.hedged() {
            Some(if (o.side == OrderSide::Buy) != closing {
                "long"
            } else {
                "short"
            })
        } else {
            None
        };
        let mut p = OrderPayload {
            symbol: o.symbol.clone(),
            inst_id: m.id.clone(),
            td_mode: "cross",
            tag: BROKER,
            side: o.side,
            ord_type: match o.kind {
                OrderType::Limit if o.time_in_force == Some(TimeInForce::PostOnly) => "post_only",
                OrderType::Limit => "limit",
                OrderType::Market => "market",
                OrderType::Conditional => "trigger",
                OrderType::TrailingStopLoss => "move_order_stop",
                _ => "conditional",
            },
            pos_side,
            ..empty_payload()
        };
        let generated = || generated_order_id(cx);
        if algo {
            p.algo_cl_ord_id = Some(generated()?);
        } else {
            p.cl_ord_id = Some(match &o.client_order_id {
                Some(id) => id.clone(),
                None => generated()?,
            });
        }
        if !algo {
            p.sz = Some(num::format(size));
            p.reduce_only = o.reduce_only.filter(|b| *b);
            if o.kind == OrderType::Limit {
                p.px = Some(num::format(adjusted(
                    o.price.unwrap_or(0.0),
                    m.precision.price,
                    false,
                )?));
            }
        }
        match o.kind {
            OrderType::Conditional => {
                p.sz = Some(num::format(size));
                p.trigger_px = Some(num::format(adjusted(
                    o.trigger_price.unwrap_or(0.0),
                    m.precision.price,
                    false,
                )?));
                // Like TS, a zero price means market execution once triggered.
                p.order_px = Some(if let Some(price) = o.price.filter(|p| *p != 0.0) {
                    num::format(adjusted(price, m.precision.price, false)?)
                } else {
                    "-1".into()
                });
                p.trigger_px_type = Some(trigger_name(o.trigger_type));
            }
            OrderType::StopLoss | OrderType::TakeProfit => {
                if size > 0.0 {
                    p.sz = Some(num::format(size));
                } else {
                    p.close_fraction = Some("1");
                }
                p.reduce_only = Some(true);
                let price =
                    num::format(adjusted(o.price.unwrap_or(0.0), m.precision.price, false)?);
                if o.kind == OrderType::StopLoss {
                    p.sl_trigger_px = Some(price);
                    p.sl_ord_px = Some("-1");
                    p.sl_trigger_px_type = Some(trigger_name(
                        o.sl_trigger_type.or(Some(StopTriggerType::MarkPrice)),
                    ));
                } else {
                    p.tp_trigger_px = Some(price);
                    p.tp_ord_px = Some("-1");
                    p.tp_trigger_px_type = Some(trigger_name(o.tp_trigger_type));
                }
            }
            OrderType::TrailingStopLoss => {
                p.active_px = Some("");
                p.callback_ratio = Some("");
                let last = fresh_last(cx, &o.symbol).ok_or(
                    "OKX trailing stop needs a last price under 5 s old; nothing was placed",
                )?;
                p.callback_spread = Some(num::format(trailing_distance(
                    m,
                    o.side,
                    o.price.unwrap_or(0.0),
                    last,
                    "nothing was placed",
                )?));
                // OKX refuses closeFraction here with 50014 "sz can not be empty".
                if size <= 0.0 {
                    return Err("OKX trailing stop needs a size".into());
                }
                p.sz = Some(num::format(size));
            }
            _ => {}
        }
        Ok(p)
    }

    fn plan_cancel(&self, cx: &Cx<'_, Pending>, ids: Vec<String>) -> Result<Job, String> {
        let mut regular = Vec::new();
        let mut algos = Vec::new();
        let mut seen = HashSet::new();
        let mut optimistic = Vec::new();
        for id in &ids {
            let (inst_id, native, algo) = if let Some(o) = cx.store.order(id) {
                let m = cx
                    .store
                    .market(&o.symbol)
                    .ok_or_else(|| "unknown market".to_owned())?;
                optimistic.push((id.clone(), o.status));
                (m.id.as_str(), native_id(id), is_algo(o.kind))
            } else if let Some(order) = self.trading.acknowledged.get(id) {
                (order.inst_id.as_str(), id.as_str(), order.algo)
            } else {
                return Err(format!(
                    "Order {id} not found; cancellation requires a reconciled order"
                ));
            };
            if !seen.insert((inst_id, native)) {
                continue;
            }
            if algo {
                algos.push(Cancel {
                    inst_id,
                    ord_id: None,
                    algo_id: Some(native),
                });
            } else {
                regular.push(Cancel {
                    inst_id,
                    ord_id: Some(id),
                    algo_id: None,
                });
            }
        }
        let mut steps = Vec::new();
        for batch in regular.chunks(20) {
            steps.push(step(
                "/api/v5/trade/cancel-batch-orders",
                batch,
                batch.len(),
                false,
            )?);
        }
        for batch in algos.chunks(10) {
            steps.push(step(
                "/api/v5/trade/cancel-algos",
                batch,
                batch.len(),
                false,
            )?);
        }
        let mut result = job(steps, Finish::Empty);
        result.optimistic = optimistic;
        result.continue_cancel_errors = true;
        Ok(result)
    }

    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        if let Some(order) = cx.store.order(id)
            && ((cx.frame_fed()
                && matches!(
                    order.kind,
                    OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
                ))
                || (order.kind == OrderType::TrailingStopLoss
                    && safe_cex_engine::protective_amend::fresh_reference(
                        cx.store,
                        &order.symbol,
                        StopTriggerType::LastPrice,
                        cx.now,
                    )
                    .is_none()))
        {
            return lookup_job(cx, order.clone(), price, amount, LookupStage::Order);
        }
        self.plan_amend_known(cx, id, price, amount, None)
    }

    /// `quote` is a last price read for this edit when the store's was stale.
    fn plan_amend_known(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
        quote: Option<f64>,
    ) -> Result<Job, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Amend<'a> {
            inst_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            cxl_on_fail: Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            ord_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            algo_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sz: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_trigger_px_type: Option<&'static str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_trigger_px_type: Option<&'static str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_trigger_px_type: Option<&'static str>,
        }
        let o = cx
            .store
            .order(id)
            .ok_or_else(|| format!("unknown order {id}"))?;
        if o.kind == OrderType::Market
            || !matches!(o.status, OrderStatus::Open | OrderStatus::Updating)
        {
            return Err("Only active limit or algo orders can be amended".into());
        }
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| "unknown market".to_owned())?;
        let oco = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && cx.store.orders().iter().any(|other| {
                other.id != o.id
                    && other.symbol == o.symbol
                    && native_id(&other.id) == native_id(id)
                    && matches!(other.kind, OrderType::StopLoss | OrderType::TakeProfit)
            });
        let mut a = Amend {
            inst_id: &m.id,
            cxl_on_fail: (o.kind != OrderType::Limit).then_some(false),
            ord_id: None,
            algo_id: None,
            new_px: None,
            new_sz: None,
            new_trigger_px: None,
            new_ord_px: None,
            new_trigger_px_type: None,
            new_sl_trigger_px: None,
            new_sl_ord_px: None,
            new_sl_trigger_px_type: None,
            new_tp_trigger_px: None,
            new_tp_ord_px: None,
            new_tp_trigger_px_type: None,
        };
        if let Some(amount) = amount.filter(|_| o.amount != 0.0) {
            a.new_sz = Some(num::format(adjusted(amount, m.precision.amount, false)?));
        }
        let mut result = if o.kind == OrderType::Limit {
            a.ord_id = Some(id);
            if let Some(price) = price {
                if !price.is_finite() || price <= 0.0 {
                    return Err("invalid order price".into());
                }
                a.new_px = Some(num::format(price));
            }
            job(
                vec![step("/api/v5/trade/amend-order", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else if o.kind == OrderType::Conditional
            || self
                .trading
                .algo_orders
                .get(id)
                .is_some_and(|observed| observed.trigger)
        {
            a.algo_id = Some(native_id(id));
            a.new_trigger_px = Some(num::format(adjusted(
                price.unwrap_or(o.price),
                m.precision.price,
                false,
            )?));
            let limit = o
                .extra
                .as_ref()
                .and_then(|e| e.conditional_order_price)
                .unwrap_or(-1.0);
            a.new_ord_px = Some(if limit > 0.0 {
                num::format(num::adjust(limit, m.precision.price))
            } else {
                "-1".into()
            });
            a.new_trigger_px_type = Some(trigger_name(o.trigger_type));
            job(
                vec![step("/api/v5/trade/amend-algos", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else if oco || matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            a.algo_id = Some(native_id(id));
            let price = num::format(adjusted(
                price.unwrap_or(o.price),
                m.precision.price,
                false,
            )?);
            let order_price = o
                .extra
                .as_ref()
                .and_then(|e| e.conditional_order_price)
                .filter(|p| *p > 0.0)
                .map_or_else(
                    || "-1".into(),
                    |p| num::format(num::adjust(p, m.precision.price)),
                );
            if o.kind == OrderType::StopLoss {
                a.new_sl_trigger_px = Some(price);
                a.new_sl_ord_px = (o.amount != 0.0).then_some(order_price);
                a.new_sl_trigger_px_type = Some(trigger_name(o.trigger_type));
            } else {
                a.new_tp_trigger_px = Some(price);
                a.new_tp_ord_px = (o.amount != 0.0).then_some(order_price);
                a.new_tp_trigger_px_type = Some(trigger_name(o.trigger_type));
            }
            job(
                vec![step("/api/v5/trade/amend-algos", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else {
            if o.kind != OrderType::TrailingStopLoss {
                return Err("Unsupported OKX protective replacement".into());
            }
            let original = self.trading.algo_orders.get(id)
                .filter(|native| native.order == *o)
                .cloned().ok_or("OKX trailing parameters are unproven; refresh before editing, the stop is unchanged")?;
            let native = original
                .trailing
                .as_ref()
                .filter(|native| native.valid())
                .ok_or("OKX trailing parameters cannot be restored; the stop is unchanged")?;
            // Until activation there is no trail to measure a dragged level against.
            if price.is_some()
                && wire::pending_activation(&native.active_px).is_some()
                && original.trigger_price.is_none()
            {
                return Err(
                    "OKX trailing stop not active yet; re-place it to change its distance".into(),
                );
            }
            let mut replacement = native.clone();
            if let Some(amount) = amount {
                let size = adjusted(amount, m.precision.amount, false)?;
                if size < m.limits.amount.min || size > m.limits.amount.max {
                    return Err(
                        "OKX trailing size is outside market limits; the stop is unchanged".into(),
                    );
                }
                replacement.sz = num::format(size);
            }
            if let Some(level) = price {
                let last = quote.or_else(|| fresh_last(cx, &o.symbol)).ok_or(
                    "OKX trailing edit needs a last price under 5 s old; the stop is unchanged",
                )?;
                let distance = trailing_distance(m, o.side, level, last, "the stop is unchanged")?;
                // A ratio order stays a ratio: OKX takes one of the two, never both.
                if replacement.callback_ratio.is_empty() {
                    replacement.callback_spread = num::format(distance);
                } else {
                    let ratio = num::adjust(distance / last, TRAILING_RATIO_STEP);
                    if ratio <= 0.0 {
                        return Err(format!(
                            "OKX trailing stop at {level} is too close to the last price {last}; the stop is unchanged"
                        ));
                    }
                    replacement.callback_ratio = num::format(ratio);
                }
            }
            replacement.algo_cl_ord_id = generated_order_id(cx)?;
            let mut restored = native.clone();
            restored.algo_cl_ord_id = generated_order_id(cx)?;
            let restore = trailing_step(&o.symbol, &restored)?;
            let mut result = self.plan_cancel(cx, vec![id.into()])?;
            result
                .steps
                .push_back(trailing_step(&o.symbol, &replacement)?);
            result.trailing = Some(TrailingAmend {
                original,
                replacement,
                phase: TrailingPhase::Cancel,
                restore: Some(restore),
                refusal: None,
                quote: None,
            });
            result.replacing_stop = Some((o.symbol.clone(), o.side, o.kind));
            result.continue_cancel_errors = false;
            result.finish = Finish::Ids;
            result
        };
        if result.optimistic.is_empty() {
            result.optimistic.push((id.into(), o.status));
        }
        Ok(result)
    }

    pub(crate) fn trailing_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), VenueError> {
        let Pending::Trade(request) = pending else {
            return Ok(());
        };
        let Some(amend) = self
            .trading
            .jobs
            .get(&request.command)
            .and_then(|job| job.trailing.as_ref())
        else {
            return Ok(());
        };
        if amend.phase != TrailingPhase::Cancel || request.path != "/api/v5/trade/cancel-algos" {
            return Ok(());
        }
        let unchanged = || {
            not_sent(
                "OKX trailing stop changed or its price/size is unproven; the stop is unchanged",
            )
            .kind(ErrorKind::InvalidInput)
        };
        let source = &amend.original.order;
        let current = store.order(&source.id).ok_or_else(unchanged)?;
        let mut normalized = current.clone();
        normalized.status = source.status;
        if normalized != *source
            || !matches!(
                current.status,
                OrderStatus::Open | OrderStatus::Updating | OrderStatus::Canceling
            )
            || self.trading.algo_orders.get(&source.id) != Some(&amend.original)
        {
            return Err(unchanged());
        }
        let market = store.market(&source.symbol).ok_or_else(unchanged)?;
        let size = amend
            .replacement
            .sz
            .parse::<f64>()
            .map_err(|_| unchanged())?;
        if !amend.replacement.valid()
            || !market.limits.amount.min.is_finite()
            || !market.limits.amount.max.is_finite()
            || market.limits.amount.min < 0.
            || size < market.limits.amount.min
            || size > market.limits.amount.max
            || adjusted(size, market.precision.amount, false).ok() != Some(size)
        {
            return Err(unchanged());
        }
        let now = release.time.raw_ms;
        let store_quote = store
            .reference_seen_at(&source.symbol, StopTriggerType::LastPrice)
            .filter(|seen| *seen <= now)
            .and_then(|seen| {
                safe_cex_engine::protective_amend::fresh_reference(
                    store,
                    &source.symbol,
                    StopTriggerType::LastPrice,
                    now,
                )
                .map(|last| (last, seen))
            });
        let read_quote = amend.quote.filter(|(_, at)| {
            *at <= now && now - at <= safe_cex_engine::protective_amend::FRESH_TICKER_MS
        });
        let (last, _) = match (store_quote, read_quote) {
            (Some(store), Some(read)) => {
                if read.1 > store.1 {
                    read
                } else {
                    store
                }
            }
            (Some(quote), None) | (None, Some(quote)) => quote,
            (None, None) => return Err(unchanged()),
        };
        if amend
            .original
            .trigger_price
            .is_some_and(|trigger| match source.side {
                OrderSide::Sell => last <= trigger,
                OrderSide::Buy => last >= trigger,
            })
        {
            return Err(unchanged());
        }
        // activePx is activation, not a fixed stop trigger; either side of last is valid.
        Ok(())
    }

    /// Mints the next call's leg from its slot and releases it signed. Progress
    /// receipts publish after admission and before the physical release.
    fn issue(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, slot: Slot) {
        let Some(job) = self.trading.jobs.get(&command) else {
            return;
        };
        let plan = job.plan.clone().expect("admitted Plan");
        let context = match cx.plan_leg_context(&plan, &slot) {
            Ok(context) => context,
            Err(reason) => {
                let error = VenueError::new(reason.venue_error().message);
                return self.end(cx, command, None, error, false);
            }
        };
        let step = job.steps.front().expect("issued step");
        cx.command_progress(command, step.receipts.clone());
        self.release(cx, command, slot, context);
    }

    /// Signs and releases the job's front step on `context`.
    fn release(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        slot: Slot,
        context: Context,
    ) {
        let rest = self.rest();
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        let step = job.steps.front().expect("issued step");
        if job.pending_write.is_none()
            && let (Some(expected), Some(writes)) = (step.write_index, &mut job.writes)
        {
            let recorded = (0..step.receipts.len()).try_for_each(|offset| {
                writes.record(
                    expected + offset,
                    ElementResult::Unknown {
                        error: Box::new(unknown_outcome(VenueError::new(UNKNOWN_OUTCOME_TEXT))),
                    },
                )
            });
            if recorded.is_ok() {
                job.pending_write = Some(expected);
            } else {
                job.writes = None;
                cx.notice(
                    safe_cex_engine::Level::Warn,
                    safe_cex_engine::log::code::REQUEST_FAILED,
                    "OKX write outcome reporting is unavailable",
                );
            }
        }
        let read = step.query.is_some();
        let path = step.query.as_ref().map_or_else(
            || step.path.to_owned(),
            |query| format!("{}?{query}", step.path),
        );
        let url = format!("{rest}{path}");
        let recipe = sign::Http::new(self.credentials.clone(), path).demo(self.options.testnet);
        let body = (!read).then(|| step.body.clone());
        let request = Request {
            command,
            slot,
            context: context.clone(),
            path: step.path,
        };
        if let Err((pending, reason)) = cx.request_signed_managed(
            context,
            None,
            if read { Method::Get } else { Method::Post },
            url,
            body,
            Box::new(recipe),
            Pending::Trade(request),
        ) && let Pending::Trade(request) = pending
        {
            self.call_failed(cx, request, not_sent(reason.venue_error().message), false);
        }
    }

    /// Native completion effects and result, once every call has settled.
    fn finish_effects(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        job: &mut Job,
    ) -> Result<String, String> {
        for (id, status) in std::mem::take(&mut job.optimistic) {
            if cx
                .store
                .order(&id)
                .is_some_and(|o| matches!(o.status, OrderStatus::Updating | OrderStatus::Canceling))
            {
                cx.store.set_order_status(&id, status);
            }
        }
        match &job.finish {
            Finish::Leverage(symbol, leverage) => {
                *self
                    .account
                    .leverage_versions
                    .entry(symbol.clone())
                    .or_default() += 1;
                let leverage = *leverage;
                cx.store.update_setting(symbol, |s| {
                    s.leverage = leverage;
                    s.margin_type = Some(MarginType::Cross);
                });
                cx.hold_setting(SettingKey::Leverage(symbol.clone()));
                self.account_after_settings(cx);
                Ok("[]".into())
            }
            Finish::Mode(hedged) => {
                self.account.mode_version += 1;
                cx.store.set_hedged(*hedged);
                cx.hold_setting(SettingKey::Hedged);
                self.account_after_settings(cx);
                Ok("[]".into())
            }
            Finish::Amend(id) => serde_json::to_string(&[id]).map_err(|e| e.to_string()),
            Finish::Ids => serde_json::to_string(&job.ids).map_err(|e| e.to_string()),
            Finish::Empty => Ok("[]".into()),
        }
    }

    fn finish(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Some(mut job) = self.trading.jobs.remove(&request.command) else {
            return;
        };
        let plan = job.plan.clone().expect("admitted Plan");
        let _ = cx.finish_plan_call(&plan, &request.slot, &request.context);
        let _ = match self.finish_effects(cx, &mut job) {
            Ok(data) => match cx.done_plan(&plan, data) {
                Ok(()) => Ok(()),
                Err(_) => cx.fail_plan(&plan, VenueError::new("OKX job ended without its call")),
            },
            Err(error) => cx.fail_plan(&plan, VenueError::new(error)),
        };
    }

    fn amend_source_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status)
            || serde_json::from_str::<wire::Envelope<'_>>(response.body)
                .is_ok_and(|envelope| envelope.code != "0")
        {
            let mut error = crate::account::read_error(
                &response,
                "OKX native order parameters could not be read; the order is unchanged".into(),
            );
            error.outcome = Some(Outcome::NotSent);
            return self.end(cx, request.command, Some(&request), error, false);
        }
        let prepared = (|| -> Result<Job, String> {
            let lookup = self
                .trading
                .jobs
                .get(&request.command)
                .and_then(|job| job.lookup.as_ref())
                .ok_or("OKX amendment source retired")?;
            if cx.store.order(&lookup.order.id) != Some(&lookup.order) {
                return Err("OKX order changed while its native parameters were read; the order is unchanged".into());
            }
            if !(200..300).contains(&response.status) {
                return Err(
                    "OKX native order parameters could not be read; the order is unchanged".into(),
                );
            }
            let envelope: wire::Envelope<'_> = serde_json::from_str(response.body)
                .map_err(|_| "Malformed OKX native order source")?;
            if envelope.code != "0" {
                return Err(format!(
                    "OKX {}: {}; the order is unchanged",
                    envelope.code, envelope.msg
                ));
            }
            if lookup.stage == LookupStage::Ticker {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Quote<'a> {
                    inst_id: &'a str,
                    last: &'a str,
                }
                let quotes: Vec<Quote<'_>> =
                    serde_json::from_str(envelope.data.ok_or("Missing OKX ticker")?.get())
                        .map_err(|_| "Malformed OKX ticker")?;
                let [quote] = quotes.as_slice() else {
                    return Err("OKX ticker must name exactly one instrument".into());
                };
                let market = cx
                    .store
                    .market(&lookup.order.symbol)
                    .ok_or("Unknown OKX market")?;
                let last = quote
                    .last
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite() && *n > 0.)
                    .ok_or("Invalid OKX ticker price")?;
                if quote.inst_id != market.id {
                    return Err("Foreign OKX ticker; the stop is unchanged".into());
                }
                let mut next = self.plan_amend_known(
                    cx,
                    &lookup.order.id,
                    lookup.price,
                    lookup.amount,
                    Some(last),
                )?;
                let trailing = next
                    .trailing
                    .as_mut()
                    .ok_or("OKX ticker has no protective owner")?;
                trailing.quote = Some((last, cx.now));
                return Ok(next);
            }
            let rows: Vec<wire::OrderRow<'_>> = serde_json::from_str(
                envelope
                    .data
                    .ok_or("Missing OKX native order source")?
                    .get(),
            )
            .map_err(|_| "Malformed OKX native order source")?;
            let [row] = rows.as_slice() else {
                return Err("OKX native order source must name exactly one order".into());
            };
            let market = cx
                .store
                .market(&lookup.order.symbol)
                .ok_or("Unknown OKX market")?;
            let wanted_side = if lookup.order.side == OrderSide::Buy {
                "buy"
            } else {
                "sell"
            };
            let admitted = matches!(row.state, "live" | "partially_effective" | "pause")
                && row.algo_id == native_id(&lookup.order.id)
                && row.inst_id == market.id
                && row.side == wanted_side
                && match lookup.order.kind {
                    OrderType::TrailingStopLoss => row.ord_type == "move_order_stop",
                    OrderType::StopLoss => {
                        row.ord_type == "trigger"
                            || matches!(row.ord_type, "conditional" | "oco")
                                && num::parse(row.sl_trigger_px) > 0.
                    }
                    OrderType::TakeProfit => {
                        row.ord_type == "trigger"
                            || matches!(row.ord_type, "conditional" | "oco")
                                && num::parse(row.tp_trigger_px) > 0.
                    }
                    _ => false,
                };
            if !admitted {
                return Err("OKX native order source is foreign, terminal or changed; the order is unchanged".into());
            }
            let order = lookup.order.clone();
            let (price, amount) = (lookup.price, lookup.amount);
            let observed =
                ObservedAlgo::new(row, &order).ok_or("Unproven OKX native algo parameters")?;
            self.trading.algo_orders.insert(order.id.clone(), observed);
            if order.kind == OrderType::TrailingStopLoss
                && safe_cex_engine::protective_amend::fresh_reference(
                    cx.store,
                    &order.symbol,
                    StopTriggerType::LastPrice,
                    cx.now,
                )
                .is_none()
            {
                return lookup_job(cx, order, price, amount, LookupStage::Ticker);
            }
            self.plan_amend_known(cx, &order.id, price, amount, None)
        })();
        let mut next = match prepared {
            Ok(next) => next,
            Err(message) => {
                return self.end(
                    cx,
                    request.command,
                    Some(&request),
                    not_sent(message),
                    false,
                );
            }
        };
        let previous = self
            .trading
            .jobs
            .remove(&request.command)
            .expect("live source Plan");
        let plan = previous.plan.expect("admitted source Plan");
        next.preflight = previous.preflight;
        next.plan = Some(plan.clone());
        for (id, status) in &mut next.optimistic {
            if let Some(current) = cx.store.order(id) {
                *status = current.status;
            }
            cx.store.set_order_status(
                id,
                if matches!(next.finish, Finish::Amend(_)) {
                    OrderStatus::Updating
                } else {
                    OrderStatus::Canceling
                },
            );
        }
        let call = next_call(&next);
        self.trading.jobs.insert(request.command, next);
        match cx.continue_serial_plan(&plan, &request.slot, &request.context, call) {
            Ok(slot) => self.issue(cx, request.command, slot),
            Err(reason) => self.end(
                cx,
                request.command,
                Some(&request),
                not_sent(reason.venue_error().message),
                false,
            ),
        }
    }

    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Some(plan) = self
            .trading
            .jobs
            .get(&request.command)
            .and_then(|job| job.plan.clone())
        else {
            return;
        };
        // Exact released DATA custody before any decode, Store or job change.
        if cx.validate_plan_delivery(&plan, &request.context).is_err() {
            return;
        }
        if self
            .trading
            .jobs
            .get(&request.command)
            .is_some_and(|job| job.lookup.is_some())
        {
            return self.amend_source_response(cx, request, response);
        }
        // OKX 50004 explicitly leaves execution unknown, including HTTP 200/400 replies.
        let parsed = serde_json::from_str::<wire::Envelope<'_>>(response.body);
        if response.status == 408
            || (500..600).contains(&response.status)
            || parsed.as_ref().is_ok_and(|reply| reply.code == "50004")
        {
            let message = format!("OKX HTTP {}: {}", response.status, response.body);
            return self.unproven(cx, request, &message);
        }
        if !(200..300).contains(&response.status) {
            let code = parsed.as_ref().ok().map(|reply| reply.code);
            let message = format!("OKX HTTP {}: {}", response.status, response.body);
            return self.failure(cx, request, code, message, response.status);
        }
        let envelope = match parsed {
            Ok(envelope) => envelope,
            Err(e) => return self.unproven(cx, request, &e.to_string()),
        };
        let Some(acks) = envelope
            .data
            .and_then(|data| serde_json::from_str::<Vec<Ack<'_>>>(data.get()).ok())
        else {
            if matches!(envelope.code, "0" | "2") {
                return self.unproven(cx, request, "Malformed OKX write response");
            }
            let message = format!("OKX {}: {}", envelope.code, envelope.msg);
            return self.failure(cx, request, Some(envelope.code), message, response.status);
        };
        // A refused envelope with no failing rows is one whole native failure.
        if !matches!(envelope.code, "0" | "2") && acks.iter().all(|a| matches!(a.s_code, "" | "0"))
        {
            let message = format!("OKX {}: {}", envelope.code, envelope.msg);
            return self.failure(cx, request, Some(envelope.code), message, response.status);
        }
        let Some(job) = self.trading.jobs.get_mut(&request.command) else {
            return;
        };
        let step = job.steps.front_mut().expect("inflight step");
        step.unknown_row = acks.iter().position(|ack| {
            (!matches!(ack.s_code, "" | "0")
                && matches!(
                    CODES.verdict(&native(ack.s_code, ack.s_msg), Intent::Place),
                    Verdict::Unknown(_)
                ))
                || (matches!(ack.s_code, "" | "0")
                    && ack.ord_id.is_empty()
                    && ack.algo_id.is_empty())
        });
        if let (Some(writes), Some(index)) = (&mut job.writes, job.pending_write)
            && acks.len() == step.expected
        {
            let recorded = placement_results(cx, &request.context, &acks).and_then(|results| {
                results
                    .into_iter()
                    .enumerate()
                    .try_for_each(|(offset, result)| writes.record(index + offset, result))
                    .map_err(|error| VenueError::new(error.to_string()))
            });
            if recorded.is_ok() {
                job.pending_write = None;
            } else {
                job.writes = None;
                job.pending_write = None;
                cx.notice(
                    safe_cex_engine::Level::Warn,
                    safe_cex_engine::log::code::REQUEST_FAILED,
                    "OKX write outcome reporting is unavailable",
                );
            }
        }
        if step.collect_ids && matches!(envelope.code, "0" | "2") {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Identity {
                inst_id: String,
            }
            let identities = if step.path == "/api/v5/trade/batch-orders" {
                serde_json::from_str::<Vec<Identity>>(&step.body)
            } else {
                serde_json::from_str::<Identity>(&step.body).map(|row| vec![row])
            };
            let identities = match identities {
                Ok(rows) if rows.len() == acks.len() => rows,
                _ => {
                    return self.unproven(
                        cx,
                        request,
                        "OKX accepted order identity is unavailable",
                    );
                }
            };
            self.account.revision += 1;
            for (ack, identity) in acks.iter().zip(identities) {
                if !ack.s_code.is_empty() && ack.s_code != "0" {
                    continue;
                }
                let id = if ack.algo_id.is_empty() {
                    ack.ord_id
                } else {
                    ack.algo_id
                };
                if id.is_empty() {
                    continue;
                }
                if !self.account.order_final(id)
                    && let Some(market) = cx.store.market_by_id(&identity.inst_id)
                {
                    self.trading.acknowledged.insert(
                        id.into(),
                        Acknowledged {
                            symbol: market.symbol.clone(),
                            inst_id: identity.inst_id,
                            algo: !ack.algo_id.is_empty(),
                            version: self.account.revision,
                        },
                    );
                }
                job.ids.push(id.into());
            }
        }
        let missing_id = step.collect_ids
            && matches!(envelope.code, "0" | "2")
            && acks.iter().any(|ack| {
                matches!(ack.s_code, "" | "0") && ack.ord_id.is_empty() && ack.algo_id.is_empty()
            });
        if missing_id || acks.iter().any(|ack| ack.s_code == "50004") {
            let message = if missing_id {
                "OKX placement response missing order ID"
            } else {
                "OKX 50004: write execution status is unknown"
            };
            // Preserve proven siblings in this same unproven batch. Recording
            // only an empty Unknown fact loses them in the engine projection.
            // Keep the existing Internal/Unknown classification and halt.
            if step.collect_ids && matches!(envelope.code, "0" | "2") {
                let ids: Vec<String> = acks
                    .iter()
                    .filter(|ack| matches!(ack.s_code, "" | "0"))
                    .map(|ack| {
                        if ack.algo_id.is_empty() {
                            ack.ord_id
                        } else {
                            ack.algo_id
                        }
                    })
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
                    .collect();
                if !ids.is_empty() {
                    let fact =
                        safe_cex_engine::classify::unproven_reply(message).with_order_ids(&ids);
                    let _ = cx.http_write_fact(
                        &request.context,
                        WriteFact::Failed {
                            fact,
                            known_effects: true,
                        },
                        None,
                    );
                    let error = VenueError::new(message).outcome(Outcome::Unknown);
                    return self.end(cx, request.command, Some(&request), error, true);
                }
            }
            return self.unproven(cx, request, message);
        }
        // Placement success needs its new native ID. Cancel identities are
        // matched against the requested rows below, including native refusals.
        if step.expected > 0
            && acks.iter().any(|ack| {
                matches!(ack.s_code, "" | "0")
                    && ack.ord_id.is_empty()
                    && ack.algo_id.is_empty()
                    && (step.collect_ids || ack.s_code.is_empty())
            })
        {
            return self.unproven(
                cx,
                request,
                "OKX success acknowledgement omitted native order identity",
            );
        }
        if step.expected > 0 && acks.len() != step.expected {
            return self.unproven(cx, request, "OKX write response row count mismatch");
        }
        let requested: Vec<Requested> = serde_json::from_str(&step.body).unwrap_or_default();
        if matches!(
            step.path,
            "/api/v5/trade/cancel-batch-orders" | "/api/v5/trade/cancel-algos"
        ) && (requested.len() != step.expected
            || acks.iter().enumerate().any(|(index, ack)| {
                requested.get(index).is_none_or(|named| {
                    named.id().is_empty()
                        || ack.ord_id != named.ord_id
                        || ack.algo_id != named.algo_id
                })
            }))
        {
            return self.unproven(cx, request, "OKX cancel row identity missing or mismatched");
        }
        if step.collect_ids
            && acks.len() == step.expected
            && acks.iter().all(|ack| {
                if ack.s_code.is_empty() || ack.s_code == "0" {
                    !ack.ord_id.is_empty() || !ack.algo_id.is_empty()
                } else {
                    // 51xxx are explicit trading-rule rejections; transport/server
                    // timeout codes do not prove that the venue rejected a write.
                    ack.s_code.starts_with("51")
                }
            })
        {
            cx.placement_resolved(request.command);
        }
        let failed = acks.iter().filter(|a| {
            !matches!(a.s_code, "" | "0") && !(job.continue_cancel_errors && a.s_code == "51400")
        });
        let first_code = failed.clone().next().map(|a| a.s_code);
        // Only 51xxx proves a refused leg, and an accepted leg means something happened.
        let definitive = failed.clone().all(|a| a.s_code.starts_with("51"))
            && !acks.iter().any(|a| {
                (job.continue_cancel_errors && a.s_code == "51400")
                    || (matches!(a.s_code, "" | "0")
                        && (!a.ord_id.is_empty() || !a.algo_id.is_empty()))
            });
        let errors: Vec<_> = failed
            .map(|a| format!("{} (code {})", a.s_msg, a.s_code))
            .collect();
        let (expected, continue_cancel_errors) = (step.expected, job.continue_cancel_errors);
        if expected == 0 && envelope.code != "0" {
            let message = format!("OKX {}: {}", envelope.code, envelope.msg);
            return self.failure(cx, request, Some(envelope.code), message, response.status);
        }
        // Record the call's fact before any continuation or terminal.
        let recorded = if expected > 0 {
            // A cancel row may answer with no id: the request named it.
            let items: Vec<_> = acks
                .iter()
                .enumerate()
                .map(|(index, ack)| {
                    if matches!(ack.s_code, "" | "0") {
                        BatchItem::Accepted(if !ack.algo_id.is_empty() {
                            ack.algo_id
                        } else if !ack.ord_id.is_empty() {
                            ack.ord_id
                        } else {
                            requested.get(index).map_or("", Requested::id)
                        })
                    } else {
                        BatchItem::Native(native(ack.s_code, ack.s_msg))
                    }
                })
                .collect();
            if matches!(
                step.path,
                "/api/v5/trade/cancel-batch-orders" | "/api/v5/trade/cancel-algos"
            ) {
                cx.http_write_applied_batch(
                    &request.context,
                    &CODES,
                    expected,
                    &items,
                    errors.join_with("; "),
                    None,
                )
            } else {
                cx.http_write_batch(
                    &request.context,
                    &CODES,
                    expected,
                    &items,
                    errors.join_with("; "),
                    None,
                )
            }
        } else {
            let accepted = WriteFact::Accepted { order_ids: vec![] };
            cx.http_write_fact(&request.context, accepted, None)
                .map(|_| WriteResult::Accepted { order_ids: vec![] })
        };
        let unproven = match &recorded {
            Ok(WriteResult::PartialUnknown(_)) | Err(_) => true,
            Ok(WriteResult::Failed(error)) => error.outcome == Some(Outcome::Unknown),
            Ok(_) => false,
        };
        let refused = |mut error: VenueError| {
            if !definitive {
                error.outcome = None;
            }
            error
        };
        if unproven {
            let message = if errors.is_empty() {
                format!("OKX {}: {}", envelope.code, envelope.msg)
            } else {
                errors.join_with("; ")
            };
            let mut error = VenueError::new(message).outcome(Outcome::Unknown);
            if matches!(
                step.path,
                "/api/v5/trade/cancel-batch-orders" | "/api/v5/trade/cancel-algos"
            ) {
                error.message = format!("{}. {UNKNOWN_OUTCOME_TEXT}", error.message);
                // Native facts still supply kind/code/partial; retain the venue's guidance.
                error.presents = true;
                error.elements = Some(Box::new(cancel_outcomes(&response, &acks, &requested)));
            }
            return self.end(cx, request.command, Some(&request), error, true);
        }
        let read_after =
            continue_cancel_errors && acks.iter().any(|a| !matches!(a.s_code, "" | "0"));
        // Legacy TS's response interceptor throws on every nonzero envelope.
        if envelope.code != "0" {
            let error = if errors.is_empty() {
                let error = format!("OKX {}: {}", envelope.code, envelope.msg);
                rejection(error, Some(envelope.code), response.status)
            } else {
                refused(rejection(
                    errors.join_with("; "),
                    first_code,
                    response.status,
                ))
            };
            self.end(cx, request.command, Some(&request), error, false);
            if continue_cancel_errors && acks.iter().any(|a| a.s_code == "51400") {
                self.account_refresh(cx);
            }
            return;
        }
        if !errors.is_empty() {
            if !continue_cancel_errors {
                let error = refused(rejection(
                    errors.join_with("; "),
                    first_code,
                    response.status,
                ));
                return self.end(cx, request.command, Some(&request), error, false);
            }
            for error in errors {
                cx.command_error(request.command, error);
            }
        }
        // A gone order's own frame may never come: only a read clears its row.
        let Some(job) = self.trading.jobs.get_mut(&request.command) else {
            return;
        };
        job.steps.pop_front();
        if let Some(trailing) = job.trailing.as_mut()
            && trailing.phase == TrailingPhase::Cancel
        {
            trailing.phase = TrailingPhase::Replace;
        }
        job.effective = true;
        if job.steps.is_empty() {
            let restored = job
                .trailing
                .as_mut()
                .filter(|a| a.phase == TrailingPhase::Restore)
                .and_then(|a| a.refusal.take());
            if let Some(mut refusal) = restored {
                refusal.message = format!(
                    "{}; the original OKX trailing stop was restored",
                    refusal.message
                );
                refusal.protection = Some(safe_cex_engine::error::Protection::Restored);
                refusal.presents = true;
                self.end(cx, request.command, Some(&request), refusal, false);
            } else {
                self.finish(cx, request);
            }
        } else {
            match cx.continue_serial_plan(&plan, &request.slot, &request.context, SERIAL) {
                Ok(slot) => self.issue(cx, request.command, slot),
                Err(reason) => {
                    let error = VenueError::new(reason.venue_error().message);
                    self.end(cx, request.command, Some(&request), error, false);
                }
            }
        }
        if read_after {
            self.account_refresh(cx);
        }
    }

    /// A delivered answer that cannot prove the write: recorded unproven, then
    /// Unknown. Optimistic status stays for the engine's reconcile snapshot.
    fn unproven(&mut self, cx: &mut Cx<'_, Pending>, request: Request, message: &str) {
        let _ = cx.http_write_unproven(&request.context, message);
        let error = VenueError::new(message).outcome(Outcome::Unknown);
        self.end(cx, request.command, Some(&request), error, true);
    }

    /// One whole native failure. A code the table does not prove refused ends
    /// Unknown rather than Rejected.
    fn failure(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        code: Option<&str>,
        message: String,
        status: u16,
    ) {
        let evidence = code
            .filter(|code| !code.is_empty())
            .map(|code| native(code, &message));
        let failure = cx.http_write_failure(
            &request.context,
            &CODES,
            evidence.as_ref(),
            message.clone(),
            None,
        );
        let (verdict, grant) = match failure {
            Ok((verdict, grant)) => (Ok(verdict), grant),
            Err(error) => (Err(error), None),
        };
        // One re-signed resend of a 50113 refusal; the journal refuses a second.
        if let Some(grant) = grant {
            let plan = self
                .trading
                .jobs
                .get(&request.command)
                .and_then(|job| job.plan.clone());
            if let Some(plan) = plan
                && let Ok(context) =
                    cx.plan_resend_context(&plan, &request.slot, &request.context, grant)
            {
                self.release(cx, request.command, request.slot, context);
                return;
            }
        }
        let error = rejection(message, code, status);
        if matches!(verdict, Ok(Verdict::Refused(_) | Verdict::RefusedResend(_))) {
            self.end(cx, request.command, Some(&request), error, false);
        } else {
            let error = error.outcome(Outcome::Unknown);
            self.end(cx, request.command, Some(&request), error, true);
        }
    }

    pub(crate) fn trade_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        self.call_failed(cx, request, error, false);
    }

    pub(crate) fn trade_lost(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        let error = VenueError::new(error).outcome(Outcome::Unknown);
        self.call_failed(cx, request, error, true);
    }

    /// The call failed without a delivered answer: settle it, then end the job.
    fn call_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
        uncertain: bool,
    ) {
        if let Some(plan) = self
            .trading
            .jobs
            .get(&request.command)
            .and_then(|job| job.plan.clone())
        {
            let _ = cx.finish_plan_failed_call(&plan, &request.slot, &request.context);
        }
        self.end(cx, request.command, None, error, uncertain);
    }

    /// Fails the job under its owner. After a step took effect a later
    /// rejection never means nothing happened, and an uncertain write keeps its
    /// optimistic status for the reconcile snapshot.
    fn end(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        current: Option<&Request>,
        mut error: VenueError,
        uncertain: bool,
    ) {
        if uncertain {
            error = unknown_outcome(error);
        }
        if !uncertain
            && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
            && let Some(request) = current
        {
            let restore = self.trading.jobs.get_mut(&command).and_then(|job| {
                let trailing = job.trailing.as_mut()?;
                if trailing.phase != TrailingPhase::Replace {
                    return None;
                }
                let restore = trailing.restore.take()?;
                trailing.phase = TrailingPhase::Restore;
                trailing.refusal = Some(error.clone());
                job.steps.clear();
                job.steps.push_back(restore);
                Some(job.plan.clone().expect("admitted trailing Plan"))
            });
            if let Some(plan) = restore {
                match cx.continue_serial_plan(&plan, &request.slot, &request.context, SERIAL) {
                    Ok(slot) => {
                        self.issue(cx, command, slot);
                        return;
                    }
                    Err(reason) => {
                        error.message =
                            format!("{}; restore admission failed: {reason}", error.message)
                    }
                }
            }
        }
        let Some(mut job) = self.trading.jobs.remove(&command) else {
            return;
        };
        if let (Some(writes), Some(index)) = (&mut job.writes, job.pending_write.take()) {
            let classified = if uncertain {
                error.clone().outcome(Outcome::Unknown)
            } else {
                error.clone()
            };
            let recorded = (0..job.steps.front().map_or(0, |step| step.receipts.len()))
                .try_for_each(|offset| {
                    let result = if matches!(
                        classified.outcome,
                        Some(Outcome::NotSent | Outcome::Rejected)
                    ) && !classified
                        .partial
                        .as_ref()
                        .is_some_and(|partial| !partial.order_ids.is_empty())
                    {
                        ElementResult::NotSent {
                            error: Some(Box::new(classified.clone())),
                        }
                    } else {
                        ElementResult::Unknown {
                            error: Box::new(unknown_outcome(classified.clone())),
                        }
                    };
                    writes.record(index + offset, result)
                });
            if recorded.is_err() {
                job.writes = None;
                cx.notice(
                    safe_cex_engine::Level::Warn,
                    safe_cex_engine::log::code::REQUEST_FAILED,
                    "OKX write outcome reporting is unavailable",
                );
            }
        }
        if !uncertain
            && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
            && current.is_some_and(|request| request.path == "/api/v5/trade/amend-algos")
        {
            error.message = format!("{}; your stop is unchanged by this edit", error.message);
            error.presents = true;
        }
        let trailing_canceled = job
            .trailing
            .as_ref()
            .is_some_and(|a| a.phase != TrailingPhase::Cancel);
        if trailing_canceled
            && error.protection != Some(safe_cex_engine::error::Protection::Restored)
        {
            if !uncertain {
                error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
            }
            error.presents = true;
            error.message = format!(
                "{}; {}",
                error.message,
                if uncertain {
                    "OKX trailing protection is unconfirmed; check Open Orders before placing another stop"
                } else {
                    "The old OKX trailing stop was canceled and could not be replaced; the position is unprotected"
                }
            );
        }
        if job.effective && !uncertain {
            error.outcome = None;
        }
        // The old stop's cancel landed and nothing replaced it: its row must not read Open.
        let mut note = job
            .replacing_stop
            .as_ref()
            .filter(|_| job.effective && job.ids.is_empty() && job.trailing.is_none())
            .map(|(symbol, side, kind)| {
                safe_cex_engine::error::stop_gone_text(symbol, *side, *kind, uncertain)
            });
        let stop_gone = note.is_some();
        if uncertain && let Some(step) = job.steps.front().filter(|step| step.query.is_none()) {
            cx.command_error(command, UNKNOWN_OUTCOME_TEXT);
            let unknown = format!("{UNKNOWN_OUTCOME_TEXT}; unconfirmed request {}", step.body);
            note = Some(match note {
                Some(note) => format!("{note}; {unknown}"),
                None => unknown,
            });
            let indexed_unknown = error.elements.as_ref().and_then(|elements| {
                elements
                    .0
                    .iter()
                    .find(|element| matches!(element.result, ElementResult::Unknown { .. }))
            });
            let identity =
                reporting_identity(step, indexed_unknown.map(|element| element.element.index));
            let mut fields = vec![
                ("command", command.into()),
                (
                    "placedCount",
                    job.ids.iter().map(|_| 1.0).sum::<f64>().into(),
                ),
                // Put the unconfirmed identity before the body so the
                // engine's 512-byte log cap cannot hide a later batch row.
                (
                    "unknownRequest",
                    match &identity {
                        Some(identity) => format!("{identity}; {}", step.body),
                        None => step.body.clone(),
                    }
                    .into(),
                ),
            ];
            if let Some(element) = indexed_unknown
                && let ElementResult::Unknown { error } = &element.result
            {
                fields.push(("index", (element.element.index as f64).into()));
                if let Some(identity) = identity {
                    fields.push(("targetOrderId", identity.into()));
                }
                if let Some(code) = &error.venue_code {
                    fields.push(("venueCode", code.to_string().into()));
                }
            }
            cx.event(
                Level::Warn,
                "order.outcome_unknown",
                UNKNOWN_OUTCOME_TEXT,
                fields,
            );
        }
        // A sibling rests: the caller must be told even though this call failed.
        if !job.ids.is_empty() {
            error = error.partial_ids(&job.ids);
        }
        for (id, status) in job.optimistic {
            if !uncertain
                && cx.store.order(&id).is_some_and(|o| {
                    matches!(o.status, OrderStatus::Updating | OrderStatus::Canceling)
                })
            {
                cx.store.set_order_status(
                    &id,
                    if stop_gone || trailing_canceled {
                        OrderStatus::Canceled
                    } else {
                        status
                    },
                );
            }
        }
        if let Some(writes) = &job.writes {
            error = writes.attach(error);
        }
        let _ = match (&job.plan, &job.preflight) {
            (Some(plan), _) => {
                if let Some(request) = current {
                    let _ = cx.finish_plan_call(plan, &request.slot, &request.context);
                }
                match &note {
                    Some(note) => cx.fail_plan_noted(plan, error, note),
                    None => cx.fail_plan(plan, error),
                }
            }
            (None, Some(preflight)) => cx.fail_preflight(preflight, error),
            (None, None) => Ok(()),
        };
    }

    pub(crate) fn trading_unproven_evidence(
        &self,
        request: &Request,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if let Some(writes) = self
            .trading
            .jobs
            .get(&request.command)
            .and_then(|job| job.writes.as_ref())
        {
            evidence.write_plan(writes);
        }
    }

    /// The engine retired a write owner itself (for example an unproven 408/5xx):
    /// forget the job. Its optimistic status stays for the reconcile snapshot.
    pub(crate) fn retire_trading(&mut self, owner: &HttpOwnerRetirement) {
        self.trading.jobs.retain(|_, job| {
            !job.preflight
                .as_ref()
                .is_some_and(|preflight| owner.matches_preflight(preflight))
        });
    }
}

/// Bounded correlation for a released write: its endpoint. The order snapshot
/// reconciles; OKX has no late reconcile codec.
pub(crate) fn trade_reconciliation(
    request: &Request,
) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
    ReconciliationData::new(request.path.to_owned())
}

/// Passive outcomes for an identity-validated native cancellation batch.
/// Indices match the batch's request rows and its partial.failed_index.
/// Use the engine's released-write verdict, including rate-limit uncertainty.
fn cancel_outcomes(
    response: &Response<'_>,
    acks: &[Ack<'_>],
    requested: &[Requested],
) -> ElementOutcomes {
    ElementOutcomes(
        acks.iter()
            .zip(requested)
            .enumerate()
            .map(|(index, (ack, target))| {
                let verdict = safe_cex_engine::classify::verdict(
                    &CODES,
                    Intent::Cancel,
                    Released::Response(response),
                    Some(&native(ack.s_code, ack.s_msg)),
                );
                let result = if matches!(ack.s_code, "" | "0")
                    || verdict == Verdict::Satisfied(SatisfiedIntent::Cancel)
                {
                    ElementResult::Applied {
                        effect: AppliedEffect::Cancel,
                        target_order_id: Some(target.id().into()),
                        position_key: None,
                    }
                } else {
                    let mut error = VenueError::new(format!(
                        "OKX cancellation target {}: {} (code {})",
                        target.id(),
                        ack.s_msg,
                        ack.s_code
                    ))
                    .venue_code(ack.s_code)
                    .venue_message(ack.s_msg);
                    if let Some(kind) = verdict.kind() {
                        error.kind = Some(kind);
                    }
                    if verdict.is_refusal() {
                        ElementResult::NotSent {
                            error: Some(Box::new(error.outcome(Outcome::Rejected))),
                        }
                    } else {
                        error.outcome = Some(Outcome::Unknown);
                        error.message = format!("{}. {UNKNOWN_OUTCOME_TEXT}", error.message);
                        ElementResult::Unknown {
                            error: Box::new(error),
                        }
                    }
                };
                ElementOutcome {
                    element: Element {
                        index,
                        client_order_id: None,
                    },
                    result,
                }
            })
            .collect(),
    )
}

/// A definitive OKX refusal. Its kind comes from the table (the native code,
/// else a documented status row), `rejected` when neither names one.
fn rejection(message: String, code: Option<&str>, status: u16) -> VenueError {
    let by_code = code.filter(|code| !code.is_empty()).and_then(|code| {
        CODES
            .verdict(&native(code, ""), safe_cex_engine::classify::Intent::Place)
            .kind()
    });
    let by_status = || {
        CODES
            .status
            .iter()
            .find(|(documented, _)| *documented == status)
            .and_then(|(_, verdict)| verdict.kind())
    };
    let kind = by_code.or_else(by_status).unwrap_or(ErrorKind::Rejected);
    // System errors and busy replies do not prove the venue refused the write.
    let outcome = if matches!(code, Some("50001" | "50013" | "50026")) {
        Outcome::Unknown
    } else {
        Outcome::Rejected
    };
    let mut error = VenueError::new(message).kind(kind).outcome(outcome);
    if let Some(code) = code.filter(|code| !code.is_empty()) {
        error = error.venue_code(code);
    }
    error
}

fn unknown_outcome(mut error: VenueError) -> VenueError {
    if error.venue_message.is_none() {
        error.venue_message = Some(error.message.clone().into_boxed_str());
    }
    error.message = UNKNOWN_OUTCOME_TEXT.into();
    error.outcome = Some(Outcome::Unknown);
    // Presentation only; the engine still classifies the retained write facts.
    error.presents = true;
    error
}

fn native_refusal(
    cx: &Cx<'_, Pending>,
    context: &Context,
    ack: &Ack<'_>,
) -> Result<ElementResult, VenueError> {
    let fact = cx
        .classify_write(
            context,
            &CODES,
            Some(&native(ack.s_code, ack.s_msg)),
            ack.s_msg,
        )
        .map_err(|error| error.venue_error())?;
    let error = fact.error().clone();
    Ok(
        if matches!(
            fact.verdict(),
            Verdict::Refused(_) | Verdict::RefusedResend(_)
        ) {
            ElementResult::NotSent {
                error: Some(Box::new(error)),
            }
        } else {
            ElementResult::Unknown {
                error: Box::new(unknown_outcome(error)),
            }
        },
    )
}

fn placement_results(
    cx: &Cx<'_, Pending>,
    context: &Context,
    acks: &[Ack<'_>],
) -> Result<Vec<ElementResult>, VenueError> {
    acks.iter()
        .map(|ack| {
            if matches!(ack.s_code, "" | "0") {
                let id = if ack.algo_id.is_empty() {
                    ack.ord_id
                } else {
                    ack.algo_id
                };
                return Ok(if id.is_empty() {
                    ElementResult::Unknown {
                        error: Box::new(unknown_outcome(
                            VenueError::new(
                                "OKX placement acknowledgement omitted native identity",
                            )
                            .kind(ErrorKind::Internal)
                            .outcome(Outcome::Unknown),
                        )),
                    }
                } else {
                    ElementResult::Placed {
                        order_ids: vec![id.into()],
                    }
                });
            }
            native_refusal(cx, context, ack)
        })
        .collect()
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

fn empty_payload() -> OrderPayload {
    OrderPayload {
        symbol: String::new(),
        inst_id: String::new(),
        td_mode: "cross",
        tag: BROKER,
        side: OrderSide::Buy,
        ord_type: "limit",
        pos_side: None,
        cl_ord_id: None,
        algo_cl_ord_id: None,
        sz: None,
        px: None,
        reduce_only: None,
        close_fraction: None,
        trigger_px: None,
        order_px: None,
        trigger_px_type: None,
        sl_trigger_px: None,
        sl_ord_px: None,
        sl_trigger_px_type: None,
        tp_trigger_px: None,
        tp_ord_px: None,
        tp_trigger_px_type: None,
        active_px: None,
        callback_ratio: None,
        callback_spread: None,
    }
}
fn step<T: Serialize + ?Sized>(
    path: &'static str,
    body: &T,
    expected: usize,
    collect_ids: bool,
) -> Result<Step, String> {
    Ok(Step {
        query: None,
        receipts: Vec::new(),
        path,
        body: serde_json::to_string(body).map_err(|e| e.to_string())?,
        expected,
        collect_ids,
        write_index: None,
        unknown_row: None,
    })
}

/// Presentation metadata only; never a dispatch, validation or retry decision.
fn reporting_identity(step: &Step, index: Option<usize>) -> Option<String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity {
        cl_ord_id: Option<String>,
        algo_cl_ord_id: Option<String>,
        ord_id: Option<String>,
        algo_id: Option<String>,
    }
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Payload {
        Rows(Vec<Identity>),
        Row(Identity),
    }
    let row = match serde_json::from_str::<Payload>(&step.body).ok()? {
        Payload::Rows(rows) => rows
            .into_iter()
            .nth(index.or(step.unknown_row).unwrap_or(0))?,
        Payload::Row(row) => row,
    };
    row.cl_ord_id
        .or(row.algo_cl_ord_id)
        .or(row.ord_id)
        .or(row.algo_id)
}
fn next_call(job: &Job) -> SerialCall {
    if job.lookup.is_some() {
        SerialCall {
            operation: Operation::Read,
            timeout_ms: 5_000,
            attempts: 1,
            ..SERIAL
        }
    } else {
        SERIAL
    }
}
fn lookup_job(
    cx: &Cx<'_, Pending>,
    order: Order,
    price: Option<f64>,
    amount: Option<f64>,
    stage: LookupStage,
) -> Result<Job, String> {
    let (path, query) = match stage {
        LookupStage::Order => (
            "/api/v5/trade/order-algo",
            format!(
                "algoId={}",
                safe_cex_engine::url::encode_component(native_id(&order.id))
            ),
        ),
        LookupStage::Ticker => (
            "/api/v5/market/ticker",
            format!(
                "instId={}",
                safe_cex_engine::url::encode_component(
                    &cx.store
                        .market(&order.symbol)
                        .ok_or("Unknown OKX market")?
                        .id
                )
            ),
        ),
    };
    let mut source = step(path, &(), 0, false)?;
    source.query = Some(query);
    let mut result = job(vec![source], Finish::Empty);
    result.lookup = Some(Box::new(AmendLookup {
        stage,
        order,
        price,
        amount,
    }));
    Ok(result)
}
fn generated_order_id(cx: &Cx<'_, Pending>) -> Result<String, String> {
    safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
        .map(|id| format!("{BROKER}{id}").chars().take(32).collect::<String>())
}
fn trailing_step(symbol: &str, payload: &TrailingOrder) -> Result<Step, String> {
    let mut write = step("/api/v5/trade/order-algo", payload, 1, true)?;
    if !payload.algo_cl_ord_id.is_empty() {
        write.receipts.push(OrderIdentityReceipt {
            symbol: symbol.into(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: payload.algo_cl_ord_id.clone(),
        });
    }
    Ok(write)
}
fn job(steps: Vec<Step>, finish: Finish) -> Job {
    Job {
        preflight: None,
        plan: None,
        steps: steps.into(),
        writes: None,
        pending_write: None,
        ids: Vec::new(),
        finish,
        optimistic: Vec::new(),
        continue_cancel_errors: false,
        effective: false,
        replacing_stop: None,
        trailing: None,
        lookup: None,
    }
}
fn is_algo(kind: OrderType) -> bool {
    !matches!(kind, OrderType::Limit | OrderType::Market)
}
fn native_id(id: &str) -> &str {
    id.strip_suffix("_sl")
        .or_else(|| id.strip_suffix("_tp"))
        .or_else(|| id.strip_suffix("_tsl"))
        .unwrap_or(id)
}
fn trigger_name(value: Option<StopTriggerType>) -> &'static str {
    match value {
        Some(StopTriggerType::MarkPrice) => "mark",
        Some(StopTriggerType::IndexPrice) => "index",
        _ => "last",
    }
}
/// A callbackRatio's precision: 0.0001 is a hundredth of a percent.
const TRAILING_RATIO_STEP: f64 = 0.0001;

fn fresh_last(cx: &Cx<'_, Pending>, symbol: &str) -> Option<f64> {
    safe_cex_engine::protective_amend::fresh_reference(
        cx.store,
        symbol,
        StopTriggerType::LastPrice,
        cx.now,
    )
}

/// Tealstreet sends a trailing stop's price as its stop level (chart drags, `trail`), while
/// OKX trails by a distance from last; Bybit and Binance convert the level the same way.
fn trailing_distance(
    m: &Market,
    side: OrderSide,
    level: f64,
    last: f64,
    outcome: &str,
) -> Result<f64, String> {
    let behind = match side {
        OrderSide::Sell => level < last,
        OrderSide::Buy => level > last,
    };
    if !level.is_finite() || level <= 0.0 || !behind {
        return Err(format!(
            "OKX trailing stop at {level} is not behind the last price {last}; {outcome}"
        ));
    }
    adjusted((last - level).abs(), m.precision.price, false).map_err(|_| {
        format!("OKX trailing stop at {level} is too close to the last price {last}; {outcome}")
    })
}

fn adjusted(value: f64, precision: f64, allow_zero: bool) -> Result<f64, String> {
    if !value.is_finite() || value < 0.0 || !precision.is_finite() || precision <= 0.0 {
        return Err("order amount/price must be finite".into());
    }
    let result = num::adjust(value, precision);
    if result < 0.0 || (result == 0.0 && (!allow_zero || value > 0.0)) {
        Err("Order amount or price is too small".into())
    } else {
        Ok(result)
    }
}

// Integer decimal units avoid floor(0.3 / 0.1) losing an entire lot.
fn lot_floor(max: f64, step: f64) -> Result<f64, String> {
    let max_text = max.to_string();
    let step_text = step.to_string();
    let decimals = |text: &str| text.split_once('.').map_or(0, |(_, tail)| tail.len());
    let places = decimals(&max_text).max(decimals(&step_text));
    let units = |text: &str| -> Option<u128> {
        let value = text.replace('.', "").parse::<u128>().ok()?;
        value.checked_mul(10u128.checked_pow((places - decimals(text)).try_into().ok()?)?)
    };
    let max_units = units(&max_text).ok_or("invalid market size limits")?;
    let step_units = units(&step_text)
        .filter(|n| *n > 0)
        .ok_or("invalid market lot size")?;
    Ok(num::adjust((max_units / step_units) as f64 * step, step))
}

fn placement_step<T: Serialize + ?Sized + PlacementIdentities>(
    path: &'static str,
    body: &T,
    expected: usize,
    collect_ids: bool,
) -> Result<Step, String> {
    let mut write = step(path, body, expected, collect_ids)?;
    write.receipts = body.identities();
    Ok(write)
}
trait PlacementIdentities {
    fn identities(&self) -> Vec<OrderIdentityReceipt>;
}
impl PlacementIdentities for OrderPayload {
    fn identities(&self) -> Vec<OrderIdentityReceipt> {
        self.cl_ord_id
            .iter()
            .chain(self.algo_cl_ord_id.iter())
            .map(|identity| OrderIdentityReceipt {
                symbol: self.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: identity.clone(),
            })
            .collect()
    }
}
impl<const N: usize> PlacementIdentities for [OrderPayload; N] {
    fn identities(&self) -> Vec<OrderIdentityReceipt> {
        self.as_slice().identities()
    }
}
impl PlacementIdentities for [OrderPayload] {
    fn identities(&self) -> Vec<OrderIdentityReceipt> {
        self.iter()
            .flat_map(PlacementIdentities::identities)
            .collect()
    }
}

#[cfg(test)]
mod code_table_tests {
    use super::*;
    use safe_cex_engine::classify::Intent;

    /// assert_table covers the status rows (never Satisfied or RefusedResend) and
    /// shadowing (an exact row after its range panics); OKX adds two of its own.
    #[test]
    fn the_okx_table_satisfies_the_engine_invariants() {
        safe_cex_engine::classify::assert_table(&CODES);
        for row in CODES.rows {
            if matches!(row.verdict, Verdict::Satisfied(_)) {
                assert_eq!(row.key, Key::Int(51400), "{row:?}");
                assert_eq!(row.only, Some(Intent::Cancel), "{row:?}");
            } else {
                assert_eq!(row.only, None, "{row:?}");
            }
            // 50113 (signature) is the one documented re-sign.
            if matches!(row.verdict, Verdict::RefusedResend(_)) {
                assert_eq!(row.key, Key::Int(50113), "{row:?}");
            }
            // No auth or permission code ever proves a write landed.
            if matches!(row.key, Key::Int(code) if (50100..=50199).contains(&code) || code == 50030)
            {
                assert!(!matches!(row.verdict, Verdict::Satisfied(_)), "{row:?}");
            }
        }
        // Every exact 51xxx row sits before the range it refines.
        let range = CODES
            .rows
            .iter()
            .position(|row| row.key == Key::Range(51000, 51999))
            .unwrap();
        for (index, row) in CODES.rows.iter().enumerate() {
            if let Key::Int(code) = row.key
                && (51000..=51999).contains(&code)
            {
                assert!(index < range, "{code} is shadowed by the 51xxx range");
            }
        }
    }

    #[test]
    fn exact_rows_win_over_their_range_and_unlisted_codes_prove_nothing() {
        let verdict = |code| CODES.verdict(&native(code, "native"), Intent::Place);
        assert_eq!(verdict("51000"), Verdict::Refused(ErrorKind::InvalidInput));
        assert_eq!(
            verdict("51008"),
            Verdict::Refused(ErrorKind::InsufficientBalance)
        );
        for code in ["51119", "51127", "51131", "51502", "59200"] {
            assert_eq!(
                verdict(code),
                Verdict::Refused(ErrorKind::InsufficientBalance),
                "{code}"
            );
        }
        for code in [
            "51001", "51006", "51020", "51121", "51122", "51137", "51203",
        ] {
            assert_eq!(
                verdict(code),
                Verdict::Refused(ErrorKind::InvalidInput),
                "{code}"
            );
        }
        // State and risk refusals in the range stay plain rejections.
        assert_eq!(verdict("51004"), Verdict::Refused(ErrorKind::Rejected));
        assert_eq!(verdict("51999"), Verdict::Refused(ErrorKind::Rejected));
        assert_eq!(verdict("50011"), Verdict::Refused(ErrorKind::RateLimited));
        assert_eq!(
            verdict("50113"),
            Verdict::RefusedResend(ErrorKind::AuthInvalid)
        );
        for unproven in ["50001", "50004", "50013", "50026", "52000", "59999"] {
            assert_eq!(verdict(unproven), Verdict::Unknown(None), "{unproven}");
        }
    }
}

#[cfg(test)]
mod code_tests {
    use super::*;
    use safe_cex_engine::classify::Intent;

    #[test]
    fn a_timeout_in_the_trading_range_is_not_a_refusal() {
        for code in ["51149", "51054", "51412", "51767"] {
            assert!(
                matches!(
                    CODES.verdict(&native(code, ""), Intent::Place),
                    Verdict::Unknown(_)
                ),
                "{code}"
            );
        }
        assert!(matches!(
            CODES.verdict(&native("51999", ""), Intent::Place),
            Verdict::Refused(_)
        ));
    }
}
