use crate::account::{clean, first, is_algo, is_hedged, margin, plan_type};
use crate::wire::{Envelope, Identifier, Number, SymbolConfig, UtaSettings};
use crate::{BitgetFutures, Pending, paths, sign};
use safe_cex_engine::Level;
use safe_cex_engine::classify::{
    BatchItem, CodeTable, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::plan::{SerialCall, SerialPolicy, Slot};
use safe_cex_engine::request_context::{
    ConditionOwner, ConditionWait, Context, Credential, HttpOwnerRetirement, Operation, Plan,
    Policy, Preflight, PreflightPolicy, PreflightPurpose, ReconciliationData, ResendGrant,
    RetryPolicy,
};
use safe_cex_engine::{
    Command, CommandId, CredentialSlot, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Each call's own bound; the streamed Plan has no whole-job deadline.
const CALL_MS: u32 = 20_000;
/// Native wait for Classic/UTA account detection (scoped engine condition wait).
const DETECTION_MS: u32 = 10_000;
const DETECTION: &str = "Bitget account detection did not complete";

/// Bitget's native write codes: 40009 earns the one fresh resend, 40010, 40725
/// and 45001 prove nothing, the read side's key refusals keep their auth kind
/// (pinned by account::write_and_read_key_refusals_agree), and any other code is
/// a pre-execution refusal.
pub(crate) const CODES: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(40009),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        refused(40001, ErrorKind::AuthInvalid),
        refused(40006, ErrorKind::AuthInvalid),
        refused(40011, ErrorKind::AuthInvalid),
        refused(40012, ErrorKind::AuthInvalid),
        refused(40037, ErrorKind::AuthInvalid),
        refused(40014, ErrorKind::AuthPermission),
        refused(40018, ErrorKind::AuthPermission),
        // Balance and margin refusals (cross-checked against ccxt's Bitget map).
        refused(40711, ErrorKind::InsufficientBalance),
        refused(40712, ErrorKind::InsufficientBalance),
        refused(40762, ErrorKind::InsufficientBalance),
        refused(43012, ErrorKind::InsufficientBalance),
        // Order rules the order itself broke: unknown symbol or parameter value,
        // parameter verification (including the six-digit 400171), wrong quantity
        // or price, size precision, minimum and maximum order value, and a stop
        // price on the wrong side of the mark.
        refused(40034, ErrorKind::InvalidInput),
        refused(400171, ErrorKind::InvalidInput),
        refused(40108, ErrorKind::InvalidInput),
        refused(40706, ErrorKind::InvalidInput),
        refused(40808, ErrorKind::InvalidInput),
        refused(40917, ErrorKind::InvalidInput),
        refused(43011, ErrorKind::InvalidInput),
        refused(45110, ErrorKind::InvalidInput),
        refused(45113, ErrorKind::InvalidInput),
        refused(45122, ErrorKind::InvalidInput),
        Row {
            key: Key::Int(40010),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Int(40725),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Int(45001),
            verdict: Verdict::Unknown(None),
            only: None,
        },
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Refused(ErrorKind::Rejected),
    status: &[
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
        (403, Verdict::Refused(ErrorKind::AuthPermission)),
        (429, Verdict::Refused(ErrorKind::RateLimited)),
    ],
};

const fn refused(code: i64, kind: ErrorKind) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Refused(kind),
        only: None,
    }
}

#[derive(Default)]
pub struct Trading {
    acknowledged: HashMap<String, (String, Planned)>,
    /// Commands behind the active job, each a Preflight parked with no I/O.
    queue: VecDeque<Queued>,
    active: Option<Job>,
    metadata_at: HashMap<String, f64>,
    /// A released write whose Plan the engine sealed itself (unproven status):
    /// restore and reconcile at the next callback that holds Cx.
    retired_write: Option<Planned>,
}

struct Queued {
    preflight: Preflight,
    intent: Command,
    /// Detection expired while queued: fail NotSent in its ready custody.
    detection_failed: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Condition {
    Detection,
    Positions,
}
impl Condition {
    fn unmet(self) -> &'static str {
        match self {
            Self::Detection => DETECTION,
            Self::Positions => "Bitget position snapshot did not complete",
        }
    }
}

enum Parked {
    Preflight(Condition),
    Plan(ConditionWait, Condition),
}

struct Job {
    id: CommandId,
    preflight: Preflight,
    plan: Option<Plan>,
    parked: Option<Parked>,
    /// A read retry armed on the Plan's own timer.
    retry: Option<Request>,
    /// The write call in flight, kept for an engine-sealed retirement.
    live: Option<Planned>,
    /// '; <note>' appended to the projected failure (a failed protection rollback).
    note: Option<String>,
    intent: Command,
    original: Option<Order>,
    metadata: VecDeque<String>,
    waiting_positions: bool,
    positions_checked: bool,
    planned: bool,
    calls: VecDeque<Planned>,
    fallback: Option<VecDeque<Planned>>,
    rollback: Option<Planned>,
    error: Option<VenueError>,
    ids: Vec<String>,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}

pub struct Request {
    command: CommandId,
    slot: Slot,
    context: Context,
    call: Planned,
    method: Method,
    path: String,
    body: Option<String>,
    stream_revision: u64,
    signature_retry: bool,
    read_retries: u8,
}

/// One call as native would send it: exact method, path and body.
struct Outgoing {
    call: Planned,
    method: Method,
    path: String,
    body: Option<String>,
}

enum Step {
    Wait(Condition),
    Call(Box<Outgoing>),
    Fail(VenueError),
    Finish,
}

/// Where the active job is driven from: Preflight custody (entry or ready),
/// the current call's DATA, or a Plan-ready callback holding the next slot.
enum At<'a> {
    Entry,
    Data(&'a Slot, &'a Context),
    Ready(Slot),
}

/// What a write's native answer proves, for its journal fact.
enum Answer {
    Accepted(Vec<String>),
    Batch {
        expected: usize,
        accepted: Vec<String>,
        failures: Vec<(Option<i64>, String)>,
    },
    Unproven,
    Coded,
}

#[derive(Clone)]
struct Planned {
    path: &'static str,
    payload: Payload,
    effect: Effect,
}

#[derive(Clone)]
enum Effect {
    Metadata(String),
    ResolveAmend {
        id: String,
        price: f64,
        amount: f64,
        client: String,
    },
    Place {
        expected: usize,
    },
    Cancel {
        orders: Vec<Order>,
        idempotent_missing: bool,
    },
    Amend {
        id: String,
        price: f64,
        amount: f64,
        client: String,
    },
    Leverage {
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    },
    Margin {
        symbol: String,
        margin: MarginType,
    },
    Mode(bool),
}

#[derive(Clone, Serialize)]
#[serde(untagged)]
enum Leverage {
    Number(f64),
    Text(String),
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelId {
    order_id: String,
}

// Native request fields; variants are selected by the planned endpoint.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    #[serde(skip_serializing_if = "Option::is_none")]
    trailing_stop_params: Option<Vec<TrailingParams>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_callback_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    product_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin_coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_client_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hold_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    force: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tpsl_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preset_stop_surplus_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preset_stop_loss_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leverage: Option<Leverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hold_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_list: Option<Vec<Payload>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id_list: Option<Vec<CancelId>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    kind: Option<&'static str>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrailingParams {
    activation_price: String,
    activation_type: &'static str,
    trail_type: &'static str,
    trail_variance: String,
    pre_order_type: &'static str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ReplyData<'a> {
    #[serde(default, borrow)]
    order_id: Identifier<'a>,
    #[serde(default, borrow)]
    client_oid: Identifier<'a>,
    #[serde(default, borrow)]
    success_list: Vec<ReplyItem<'a>>,
    #[serde(default)]
    failure_list: Vec<Failure>,
    #[serde(default, alias = "state", alias = "orderStatus")]
    status: String,
    #[serde(default, alias = "filledQty", alias = "cumExecQty")]
    base_volume: Number,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplyItem<'a> {
    #[serde(borrow)]
    order_id: Identifier<'a>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Failure {
    #[serde(default)]
    error_msg: String,
    #[serde(default)]
    error_code: Number,
}
#[derive(Deserialize)]
struct Reply<'a> {
    code: Number,
    #[serde(default)]
    msg: String,
    #[serde(borrow)]
    data: Option<ReplyData<'a>>,
}

impl BitgetFutures {
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        // The owed refresh allocates I/O, so it waits until this command has used
        // its own genuine admission.
        let refresh = self.restore_retired_write(cx);
        self.plan_command(cx, command);
        if refresh {
            self.refresh_orders(cx, false);
        }
    }

    fn plan_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let observed: Vec<_> = self
            .trading
            .acknowledged
            .keys()
            .filter(|id| {
                cx.store
                    .orders()
                    .iter()
                    .any(|o| o.id == **id || o.parent_id.as_ref() == Some(*id))
                    || self.order_terminal(id)
            })
            .cloned()
            .collect();
        for id in observed {
            self.trading.acknowledged.remove(&id);
        }
        if command_id(&command).is_none() {
            return;
        }
        let validation = if !self.private_enabled() {
            Err("Bitget credentials are required".to_owned())
        } else {
            self.preflight(cx, &command)
        };
        if let Err(error) = validation {
            self.refuse_entry(cx, not_sent(error));
            return;
        }
        // In-job reads sign with the static Read role; writes with Primary.
        let (read_credential, _) = self.read_selection(paths::ACCOUNT);
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential,
            write_credential: self.primary_credential(),
        }) else {
            return;
        };
        if self.trading.active.is_some() || !self.trading.queue.is_empty() {
            if let Err(reason) = cx.park_preflight_for(&preflight, None) {
                let _ = cx.fail_preflight(&preflight, not_sent(reason.venue_error().message));
                return;
            }
            self.trading.queue.push_back(Queued {
                preflight,
                intent: command,
                detection_failed: false,
            });
            return;
        }
        self.start_job(cx, preflight, command);
    }

    fn primary_credential(&self) -> Option<Credential> {
        self.options
            .credentials
            .as_ref()
            .filter(|identity| identity.validate().is_ok())
            .map(|identity| Credential {
                slot: CredentialSlot::Primary,
                reference: identity.credential.clone(),
            })
    }

    /// A write refused before I/O admits a terminal-only Mutation owner, so the
    /// engine caps it at NotSent. A rejected admission already failed the command.
    fn refuse_entry(&self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        if let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Mutation,
            credential: self.primary_credential(),
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) {
            let _ = cx.fail_context(&context, error);
        }
    }

    fn preflight(&self, cx: &Cx<'_, Pending>, command: &Command) -> Result<(), String> {
        if let Command::PlaceOrders { orders, .. } = command {
            let mut ids = HashSet::new();
            for o in orders {
                if o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some() {
                    return Err("Bybit order controls are not supported by Bitget".into());
                }
                let market = cx
                    .store
                    .market(&o.symbol)
                    .ok_or_else(|| format!("Market {} not found", o.symbol))?;
                if o.kind == OrderType::Conditional
                    || (o.trigger_price.is_some() && !o.redundant_protective_trigger())
                {
                    return Err("Bitget opening conditional orders are unsupported".into());
                }
                if !o.amount.is_finite() || o.amount < 0.0 {
                    return Err("invalid Bitget order amount".into());
                }
                if o.kind != OrderType::Market
                    && !o.price.is_some_and(|v| {
                        v.is_finite()
                            && num::adjust(v, market.precision.price).is_finite()
                            && num::adjust(v, market.precision.price) > 0.0
                    })
                {
                    return Err("Bitget order requires a positive price".into());
                }
                for value in [o.stop_loss, o.take_profit].into_iter().flatten() {
                    if !value.is_finite() || value <= 0.0 {
                        return Err("invalid Bitget bracket price".into());
                    }
                }
                if let Some(id) = &o.client_order_id
                    && (id.is_empty()
                        || id.len() > 32
                        || !id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b".:/_-".contains(&b))
                        || !ids.insert(id)
                        || is_algo(o.kind)
                        || num::adjust(o.amount, market.precision.amount) <= 0.0
                        || o.take_profit.is_some()
                        || o.stop_loss.is_some()
                        || (market.limits.amount.max > 0.0
                            && (o.amount > market.limits.amount.max
                                || num::adjust(o.amount, market.precision.amount)
                                    > floor_lot(
                                        market.limits.amount.max,
                                        market.precision.amount,
                                    ))))
                {
                    return Err(
                        "Bitget clientOrderId cannot be honored verbatim on one native order"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }

    fn start_job(&mut self, cx: &mut Cx<'_, Pending>, preflight: Preflight, intent: Command) {
        // Frame-fed: settings and hedge mode arrive as frames, never read here.
        let metadata = symbols(cx, &intent)
            .into_iter()
            .filter(|_| !cx.frame_fed())
            .filter(|symbol| {
                cx.store.setting(symbol).is_none()
                    || (!cx
                        .store
                        .positions()
                        .iter()
                        .any(|p| &p.symbol == symbol && p.contracts > 0.0)
                        && !self
                            .trading
                            .metadata_at
                            .get(symbol)
                            .is_some_and(|at| cx.now - *at < 5_000.0))
            })
            .collect();
        let job = Job {
            id: command_id(&intent).expect("trade command"),
            preflight,
            plan: None,
            parked: None,
            retry: None,
            live: None,
            note: None,
            original: if let Command::AmendOrder { id, .. } = &intent {
                cx.store.order(id).cloned()
            } else {
                None
            },
            intent,
            metadata,
            waiting_positions: false,
            positions_checked: false,
            planned: false,
            calls: VecDeque::new(),
            fallback: None,
            rollback: None,
            error: None,
            ids: vec![],
            effective: false,
        };
        self.drive(cx, job, At::Entry);
    }

    /// The native job order, without I/O except the borrowed position read.
    #[inline(never)]
    fn next_step(&mut self, cx: &mut Cx<'_, Pending>, job: &mut Job) -> Step {
        if !self.account.detected && !(job.planned && job.calls.is_empty()) {
            return Step::Wait(Condition::Detection);
        }
        if job.waiting_positions {
            if self.positions_in_flight() {
                return Step::Wait(Condition::Positions);
            }
            job.waiting_positions = false;
            job.positions_checked = true;
        }
        if let Some(symbol) = job.metadata.pop_front() {
            if cx.store.market(&symbol).is_none() {
                return Step::Fail(not_sent(format!("Market {symbol} not found")));
            }
            let call = Planned {
                path: paths::ACCOUNT,
                payload: Payload {
                    symbol: Some(symbol.clone()),
                    ..Payload::default()
                },
                effect: Effect::Metadata(symbol),
            };
            return self.call_step(cx, call);
        }
        if !job.planned {
            if !job.positions_checked && needs_position_read(cx, &job.intent, self.account.uta) {
                self.refresh_positions(cx);
                if self.positions_in_flight() {
                    job.waiting_positions = true;
                    return Step::Wait(Condition::Positions);
                }
                job.positions_checked = true;
            }
            if let Err(error) = self.plan_job(cx, job) {
                return Step::Fail(not_sent(error));
            }
            self.warn_stop_downgrade(cx, job);
            job.planned = true;
        }
        match job.calls.pop_front() {
            Some(call) => self.call_step(cx, call),
            None => Step::Finish,
        }
    }

    fn call_step(&self, cx: &Cx<'_, Pending>, call: Planned) -> Step {
        let read = call.read();
        let path = match &call.effect {
            Effect::Metadata(symbol) => {
                if self.account.uta {
                    paths::SETTINGS.to_owned()
                } else {
                    let coin = match self.margin_coin(cx, symbol) {
                        Ok(coin) => coin,
                        Err(error) => return Step::Fail(not_sent(error)),
                    };
                    format!(
                        "{}?symbol={symbol}&productType={}&marginCoin={}",
                        paths::ACCOUNT,
                        self.product(),
                        coin
                    )
                }
            }
            Effect::ResolveAmend { client, .. } => format!(
                "{}?symbol={}&productType={}&clientOid={}",
                paths::ORDER_DETAIL,
                call.payload.symbol.as_deref().unwrap_or_default(),
                self.product(),
                sign::encode(client)
            ),
            _ => call.path.to_owned(),
        };
        let body = (!read).then(|| serde_json::to_string(&call.payload).expect("Bitget payload"));
        let method = if read { Method::Get } else { Method::Post };
        Step::Call(Box::new(Outgoing {
            call,
            method,
            path,
            body,
        }))
    }

    /// Advances the active job from `at`: admit, continue, park, or settle.
    #[inline(never)]
    fn drive(&mut self, cx: &mut Cx<'_, Pending>, mut job: Job, at: At<'_>) {
        let current = match &at {
            At::Data(slot, context) => Some((*slot, *context)),
            _ => None,
        };
        match self.next_step(cx, &mut job) {
            Step::Call(outgoing) => {
                let next = serial(outgoing.call.operation());
                let slot = match at {
                    At::Entry => {
                        let Some(credential) = self.primary_credential() else {
                            return self.end(
                                cx,
                                job,
                                None,
                                not_sent("Bitget credentials are required"),
                            );
                        };
                        let policy = SerialPolicy {
                            credential: Some(credential),
                            pacing: None,
                            refusals_complete: true,
                        };
                        match cx.admit_serial_plan(&job.preflight, next, policy) {
                            Ok((plan, slot)) => {
                                job.plan = Some(plan);
                                slot
                            }
                            Err(reason) => {
                                let error = not_sent(reason.venue_error().message);
                                return self.end(cx, job, None, error);
                            }
                        }
                    }
                    At::Data(slot, context) => {
                        let plan = job.plan.clone().expect("live Plan");
                        match cx.continue_serial_plan(&plan, slot, context, next) {
                            Ok(slot) => slot,
                            Err(reason) => {
                                let error = VenueError::new(reason.venue_error().message);
                                return self.end(cx, job, current, error);
                            }
                        }
                    }
                    At::Ready(slot) => slot,
                };
                self.issue(cx, job, slot, *outgoing);
            }
            Step::Wait(condition) => {
                let bound = (condition == Condition::Detection).then_some(DETECTION_MS);
                match at {
                    At::Entry => match cx.park_preflight_for(&job.preflight, bound) {
                        Ok(()) => {
                            job.parked = Some(Parked::Preflight(condition));
                            self.trading.active = Some(job);
                        }
                        Err(reason) => {
                            let error = not_sent(reason.venue_error().message);
                            self.end(cx, job, None, error);
                        }
                    },
                    At::Data(slot, context) => {
                        let plan = job.plan.clone().expect("live Plan");
                        let next = serial(job.next_operation());
                        match cx.serial_plan_condition_wait(&plan, slot, context, next, bound) {
                            Ok(wait) => {
                                job.parked = Some(Parked::Plan(wait, condition));
                                self.trading.active = Some(job);
                            }
                            Err(reason) => {
                                let error = VenueError::new(reason.venue_error().message);
                                self.end(cx, job, current, error);
                            }
                        }
                    }
                    // A released condition is issued in this same flush; it cannot park again.
                    At::Ready(_) => self.end(cx, job, None, not_sent(condition.unmet())),
                }
            }
            Step::Fail(error) => self.end(cx, job, current, error),
            Step::Finish => self.finish(cx, job, current),
        }
    }

    /// Queues one Plan call, applying native optimistic status after admission.
    #[inline(never)]
    fn issue(&mut self, cx: &mut Cx<'_, Pending>, job: Job, slot: Slot, outgoing: Outgoing) {
        let Outgoing {
            call,
            method,
            path,
            body,
        } = outgoing;
        let plan = job.plan.clone().expect("admitted Plan");
        let context = if narrowable(&job.intent, call.intent()) {
            cx.plan_leg_context_with_intent(&plan, &slot, call.intent())
        } else {
            cx.plan_leg_context(&plan, &slot)
        };
        let context = match context {
            Ok(context) => context,
            Err(reason) => {
                let error = VenueError::new(reason.venue_error().message);
                return self.end(cx, job, None, error);
            }
        };
        let request = Request {
            command: job.id,
            slot,
            context,
            call,
            method,
            path,
            body,
            stream_revision: self.order_revision_clock(),
            signature_retry: false,
            read_retries: 0,
        };
        self.send_call(cx, job, request);
    }

    #[inline(never)]
    fn send_call(&mut self, cx: &mut Cx<'_, Pending>, mut job: Job, request: Request) {
        job.live = (!request.call.read()).then(|| request.call.clone());
        if let Effect::Cancel { orders, .. } = &request.call.effect {
            for order in orders {
                cx.store.set_order_status(&order.id, OrderStatus::Canceling);
            }
        }
        let identity = if request.call.read() {
            self.read_selection(&request.path).1
        } else {
            self.signing_identity(Method::Post, &request.path).cloned()
        };
        let recipe = sign::Http::new(identity, request.path.clone());
        let url = [paths::REST, request.path.as_str()].concat();
        cx.command_progress(job.id, request.call.identities());
        let (context, method, body) = (
            request.context.clone(),
            request.method,
            request.body.clone(),
        );
        self.trading.active = Some(job);
        if let Err((pending, reason)) = cx.request_signed_managed(
            context,
            None,
            method,
            url,
            body,
            Box::new(recipe),
            Pending::Trade(Box::new(request)),
        ) && let Pending::Trade(request) = pending
        {
            self.trade_call_failed(cx, *request, reason.venue_error(), false);
        }
    }

    /// Fails the job under its owner; after a write took effect it is never
    /// Rejected or NotSent. Then wakes the next queued command.
    #[inline(never)]
    fn end(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut job: Job,
        current: Option<(&Slot, &Context)>,
        mut error: VenueError,
    ) {
        // A rollback that cannot finish, whatever stops it, keeps the original
        // refusal and the protection note.
        if job.error.is_some() {
            let (original, note) = with_rollback(job.error.take(), error);
            error = original;
            job.note = note;
        }
        if (job.effective || !job.ids.is_empty())
            && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
        {
            error.outcome = None;
        }
        match (&job.plan, job.note.take()) {
            (None, note) => {
                if let Some(note) = note {
                    error.message = format!("{}; {note}", error.message);
                }
                let failed = cx.fail_preflight(&job.preflight, error);
                debug_assert!(failed.is_ok(), "Preflight terminal stranded");
            }
            (Some(plan), note) => {
                if let Some((slot, context)) = current {
                    let finished = cx.finish_plan_call(plan, slot, context);
                    debug_assert!(finished.is_ok(), "Plan call seal stranded");
                }
                // The rollback note is safety text: a protection order may remain.
                let failed = match note {
                    Some(note) => cx.fail_plan_noted(plan, error, &note),
                    None => cx.fail_plan(plan, error),
                };
                debug_assert!(failed.is_ok(), "Plan terminal stranded");
            }
        }
        self.next_job(cx);
    }

    #[inline(never)]
    fn finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut job: Job,
        current: Option<(&Slot, &Context)>,
    ) {
        if let Some(error) = job.error.take() {
            return self.end(cx, job, current, error);
        }
        let ids = serde_json::to_string(&job.ids).expect("order ids");
        match &job.plan {
            None => {
                let ended = cx.done_empty_plan(&job.preflight, ids);
                debug_assert!(ended.is_ok(), "empty Plan terminal stranded");
            }
            Some(plan) => {
                if let Some((slot, context)) = current {
                    let finished = cx.finish_plan_call(plan, slot, context);
                    debug_assert!(finished.is_ok(), "Plan call seal stranded");
                }
                if cx.done_plan(plan, ids).is_err() {
                    let failed =
                        cx.fail_plan(plan, VenueError::new("Bitget job ended without its call"));
                    debug_assert!(failed.is_ok(), "Plan terminal stranded");
                }
            }
        }
        self.next_job(cx);
    }

    /// The active job ended: wake the oldest queued Preflight (FIFO).
    #[inline(never)]
    fn next_job(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(queued) = self.trading.queue.front() {
            let _ = cx.request_preflight_ready(&queued.preflight);
        }
    }

    /// From a borrowed callback: release the active job's wait once its
    /// condition holds. Never issues I/O itself.
    pub(crate) fn wake_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        self.settle_retired_write(cx);
        let detected = self.account.detected;
        let positions = self.positions_in_flight();
        let Some(job) = self.trading.active.as_mut() else {
            return;
        };
        let holds = |condition: &Condition| match condition {
            Condition::Detection => detected,
            Condition::Positions => !positions,
        };
        match &job.parked {
            Some(Parked::Preflight(condition))
                if holds(condition) && cx.request_preflight_ready(&job.preflight).is_ok() =>
            {
                job.parked = None;
            }
            Some(Parked::Plan(wait, condition))
                if holds(condition) && cx.release_condition_wait(wait).is_ok() =>
            {
                job.parked = None;
            }
            _ => {}
        }
    }

    pub(crate) fn trading_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &Preflight,
    ) {
        if let Some(mut job) = self.trading.active.take() {
            if job.preflight.owner() == preflight.owner() {
                job.parked = None;
                return self.drive(cx, job, At::Entry);
            }
            self.trading.active = Some(job);
            // Woken behind a live job: park again, still in FIFO order.
            let _ = cx.park_preflight_for(preflight, None);
            return;
        }
        let front = self
            .trading
            .queue
            .front()
            .is_some_and(|queued| queued.preflight.owner() == preflight.owner());
        if !front {
            let _ = cx.park_preflight_for(preflight, None);
            return;
        }
        let queued = self.trading.queue.pop_front().expect("queued front");
        if queued.detection_failed {
            let _ = cx.fail_preflight(&queued.preflight, not_sent(DETECTION));
            self.next_job(cx);
            return;
        }
        self.start_job(cx, queued.preflight, queued.intent);
    }

    pub(crate) fn trading_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let Some(mut job) = self.trading.active.take() else {
            return;
        };
        let slot = job
            .plan
            .as_ref()
            .filter(|own| own.owner() == plan.owner())
            .and_then(|_| cx.plan_slots(plan).ok())
            .and_then(|slots| slots.into_iter().next());
        let Some(slot) = slot else {
            self.trading.active = Some(job);
            return;
        };
        if let Some(retry) = job.retry.take() {
            return self.reissue(cx, job, slot, retry);
        }
        job.parked = None;
        self.drive(cx, job, At::Ready(slot));
    }

    /// Detection did not complete within its bound: the job and every queued
    /// command fail NotSent, each queued one in its own ready custody.
    pub(crate) fn trading_condition_expired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &ConditionOwner,
    ) {
        let ours = self.trading.active.as_ref().is_some_and(|job| match owner {
            ConditionOwner::Preflight(preflight) => job.preflight.owner() == preflight.owner(),
            ConditionOwner::Plan(plan) => job
                .plan
                .as_ref()
                .is_some_and(|own| own.owner() == plan.owner()),
        });
        if !ours {
            return;
        }
        for queued in &mut self.trading.queue {
            queued.detection_failed = true;
        }
        let job = self.trading.active.take().expect("active job");
        self.end(cx, job, None, not_sent(DETECTION));
    }

    /// The engine retired a trading owner itself: forget it and name the next
    /// queued Preflight to wake.
    pub(crate) fn trading_retired(&mut self, owner: &HttpOwnerRetirement) -> Option<Preflight> {
        self.trading
            .queue
            .retain(|queued| !owner.matches_preflight(&queued.preflight));
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| owner.matches_preflight(&job.preflight))
        {
            let job = self.trading.active.take().expect("retired job");
            self.trading.retired_write = job.live;
            return self
                .trading
                .queue
                .front()
                .map(|queued| queued.preflight.clone());
        }
        None
    }

    /// Native restore and reconcile for a write the engine sealed as Unknown.
    pub(crate) fn settle_retired_write(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.restore_retired_write(cx) {
            self.refresh_orders(cx, false);
        }
    }

    /// The I/O-free half of the settle: exact Store restoration. Returns whether
    /// the order refresh is still owed.
    fn restore_retired_write(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        let Some(call) = self.trading.retired_write.take() else {
            return false;
        };
        restore_canceling(cx, &call);
        if matches!(call.effect, Effect::Place { .. }) {
            self.account.orders_version = self.account.orders_version.wrapping_add(1);
        }
        true
    }

    fn plan_job(&self, cx: &Cx<'_, Pending>, job: &mut Job) -> Result<(), String> {
        match &job.intent {
            Command::PlaceOrders { orders, .. } => {
                if self.trading.acknowledged.len() >= 4096 {
                    return Err(
                        "Bitget unobserved placement capacity exceeded; reconcile before placing"
                            .into(),
                    );
                }
                job.calls = self.place_calls(cx, orders)?.into();
            }
            Command::CancelOrders { ids, .. } => {
                let native = ids
                    .iter()
                    .map(|id| {
                        cx.store
                            .order(id)
                            .map(native_id)
                            .or_else(|| {
                                self.trading
                                    .acknowledged
                                    .contains_key(id)
                                    .then(|| id.clone())
                            })
                            .ok_or_else(|| {
                                format!(
                                    "Order {id} not found; cancellation requires a reconciled order"
                                )
                            })
                    })
                    .collect::<Result<HashSet<_>, String>>()?;
                let orders: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| native.contains(&native_id(o)))
                    .cloned()
                    .collect();
                job.calls = self.cancel_calls(cx, &orders, true)?.into();
                for (_, call) in ids
                    .iter()
                    .filter_map(|id| self.trading.acknowledged.get(id))
                {
                    if call.path.is_empty() {
                        return Err(
                            "Bitget cancellation needs the acknowledged order's native identity"
                                .into(),
                        );
                    }
                    job.calls.push_back(call.clone());
                }
            }
            Command::CancelSymbolOrders { symbol, .. } => {
                let orders: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == *symbol)
                    .cloned()
                    .collect();
                job.calls = self.cancel_calls(cx, &orders, true)?.into();
                for (_, call) in self
                    .trading
                    .acknowledged
                    .values()
                    .filter(|(s, _)| s == symbol)
                {
                    if call.path.is_empty() {
                        return Err(
                            "Bitget cancellation needs the acknowledged order's native identity"
                                .into(),
                        );
                    }
                    job.calls.push_back(call.clone());
                }
            }
            Command::AmendOrder {
                id, price, amount, ..
            } => {
                let (id, price, amount) = (id.clone(), *price, *amount);
                self.plan_amend(cx, job, &id, price, amount)?;
            }
            Command::SetLeverage {
                symbol,
                leverage,
                side,
                ..
            } => {
                let m = cx.store.market(symbol).ok_or("unknown market")?;
                if !leverage.is_finite() || *leverage <= 0.0 {
                    return Err("invalid leverage".into());
                }
                let min = leverage_limit(m.limits.leverage.min, 1.0);
                let max = leverage_limit(m.limits.leverage.max, 20.0);
                let leverage = num::clamp_leverage(*leverage, min, max)?;
                let isolated_hedge = cx.store.hedged()
                    && cx
                        .store
                        .setting(symbol)
                        .is_some_and(|s| s.margin_type == Some(MarginType::Isolated));
                let requested_side = side.unwrap_or(PositionSide::Long);
                let side = isolated_hedge.then_some(requested_side);
                let side_text = (isolated_hedge || (self.inverse() && cx.store.hedged()))
                    .then_some(requested_side)
                    .map(|side| match side {
                        PositionSide::Long => "long",
                        PositionSide::Short => "short",
                    });
                let mut p = self.classic(cx, symbol)?;
                p.leverage = Some(Leverage::Number(leverage));
                let path = if self.account.uta && isolated_hedge {
                    p = Payload {
                        category: Some(self.product().to_uppercase()),
                        symbol: Some(symbol.clone()),
                        coin: Some(self.coin().into()),
                        leverage: Some(Leverage::Text(num::format(leverage))),
                        pos_side: side_text,
                        ..Payload::default()
                    };
                    paths::UTA_SET_LEVERAGE
                } else {
                    p.hold_side = side_text;
                    paths::SET_LEVERAGE
                };
                job.calls.push_back(Planned {
                    path,
                    payload: p,
                    effect: Effect::Leverage {
                        symbol: symbol.clone(),
                        leverage,
                        side,
                    },
                });
            }
            Command::SetMarginType {
                symbol, isolated, ..
            } => {
                let margin = if *isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                let mut p = self.classic(cx, symbol)?;
                p.margin_mode = Some(margin_text(margin));
                let path = if self.account.uta {
                    p = Payload {
                        category: Some(self.product().to_uppercase()),
                        symbol: Some(symbol.clone()),
                        margin_mode: Some(margin_text(margin)),
                        leverage: Some(Leverage::Text(num::format(
                            cx.store.setting(symbol).map_or(1.0, |s| s.leverage),
                        ))),
                        ..Payload::default()
                    };
                    paths::UTA_SET_LEVERAGE
                } else {
                    paths::SET_MARGIN
                };
                job.calls.push_back(Planned {
                    path,
                    payload: p,
                    effect: Effect::Margin {
                        symbol: symbol.clone(),
                        margin,
                    },
                });
            }
            Command::SetPositionMode { hedged, .. } => {
                let mode = if *hedged {
                    "hedge_mode"
                } else {
                    "one_way_mode"
                };
                let payload = if self.account.uta {
                    Payload {
                        hold_mode: Some(mode),
                        ..Payload::default()
                    }
                } else {
                    Payload {
                        product_type: Some(self.product().into()),
                        pos_mode: Some(mode),
                        ..Payload::default()
                    }
                };
                job.calls.push_back(Planned {
                    path: if self.account.uta {
                        paths::UTA_SET_MODE
                    } else {
                        paths::SET_MODE
                    },
                    payload,
                    effect: Effect::Mode(*hedged),
                });
            }
            _ => return Err("unsupported Bitget trading command".into()),
        }
        Ok(())
    }

    fn warn_stop_downgrade(&self, cx: &mut Cx<'_, Pending>, job: &Job) {
        let orders = match &job.intent {
            Command::PlaceOrders { orders, .. } => orders.clone(),
            Command::AmendOrder { amount, .. } => job
                .original
                .as_ref()
                .map(|order| {
                    let mut opts = place_opts(order);
                    opts.amount = amount.unwrap_or(order.amount);
                    vec![opts]
                })
                .unwrap_or_default(),
            _ => vec![],
        };
        for order in orders {
            if order.amount <= 0.0
                || !matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            {
                continue;
            }
            let no_sized_plan = self
                .plan_size(cx, &order)
                .is_ok_and(|(_, size)| size <= 0.0);
            let uta_fallback_size = cx
                .store
                .positions()
                .iter()
                .find(|position| {
                    position.symbol == order.symbol
                        && (!cx.store.hedged() || position.side == protected_position(order.side))
                        && position.contracts > 0.0
                })
                .map_or(0.0, |position| {
                    cx.store.market(&order.symbol).map_or(0.0, |market| {
                        num::adjust(position.contracts, market.precision.amount)
                    })
                });
            let downgraded = no_sized_plan
                && (!self.account.uta || uta_fallback_size <= 0.0)
                && job.calls.iter().any(|call| {
                    call.payload.symbol.as_deref() == Some(order.symbol.as_str())
                        && (matches!(
                            call.payload.plan_type.as_deref(),
                            Some("pos_loss" | "pos_profit")
                        ) || call.payload.tpsl_mode == Some("full"))
                });
            if downgraded {
                let positions = cx
                    .store
                    .positions()
                    .iter()
                    .filter(|position| position.symbol == order.symbol)
                    .map(|position| format!("{:?}:{}", position.side, position.contracts))
                    .collect::<Vec<_>>()
                    .join(", ");
                cx.notice(Level::Warn, code::COMMAND_UNSUPPORTED, format!(
                    "Sized {} stop became a full-position stop: no open {:?} position to size requested {} against, positions seen [{}]",
                    order.symbol, protected_position(order.side), order.amount,
                    if positions.is_empty() { "none" } else { &positions },
                ));
            }
        }
    }

    fn classic(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Result<Payload, String> {
        Ok(Payload {
            symbol: Some(symbol.into()),
            product_type: Some(self.product().into()),
            margin_coin: Some(self.margin_coin(cx, symbol)?.into()),
            ..Payload::default()
        })
    }

    fn place_calls(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<Vec<Planned>, String> {
        let mut calls = vec![];
        let mut groups: Vec<(String, Vec<Payload>)> = vec![];
        for o in orders.iter().filter(|o| !is_algo(o.kind)) {
            let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
            let total = num::adjust(o.amount, m.precision.amount);
            // TS drops an order that rounds below one lot. Explicit caller ids
            // were rejected in preflight because dropping cannot honor them.
            if total <= 0.0 {
                continue;
            }
            let max = if m.limits.amount.max > 0.0 && m.limits.amount.max.is_finite() {
                floor_lot(m.limits.amount.max, m.precision.amount)
            } else {
                total
            };
            if max <= 0.0 {
                return Err("Bitget maximum size is smaller than one lot".into());
            }
            let mut left = total;
            let count = (total / max).ceil() as usize;
            if o.client_order_id.is_some() && count > 1 {
                return Err("Bitget caller clientOrderId would split across native orders".into());
            }
            if count > 10_000 {
                return Err("Bitget order exceeds the split bound".into());
            }
            while left > 0.0 {
                let size = left.min(max);
                left = clean(left - size);
                let payload = self.normal_payload(cx, o, size)?;
                if self.account.uta {
                    calls.push(Planned {
                        path: paths::UTA_PLACE,
                        payload,
                        effect: Effect::Place { expected: 1 },
                    });
                } else if let Some((_, group)) =
                    groups.iter_mut().find(|(symbol, _)| symbol == &o.symbol)
                {
                    group.push(payload);
                } else {
                    groups.push((o.symbol.clone(), vec![payload]));
                }
            }
        }
        for (symbol, orders) in groups {
            for chunk in orders.chunks(50) {
                let mut payload = self.classic(cx, &symbol)?;
                payload.margin_mode = Some(margin_text(
                    cx.store
                        .setting(&symbol)
                        .and_then(|s| s.margin_type)
                        .unwrap_or(MarginType::Cross),
                ));
                payload.order_list = Some(chunk.to_vec());
                calls.push(Planned {
                    path: paths::BATCH_PLACE,
                    payload,
                    effect: Effect::Place {
                        expected: chunk.len(),
                    },
                });
            }
        }
        for o in orders.iter().filter(|o| is_algo(o.kind)) {
            let (path, payload) = if self.account.uta {
                (paths::UTA_STRATEGY, self.strategy_payload(cx, o)?)
            } else {
                (
                    if o.kind == OrderType::TrailingStopLoss {
                        paths::PLACE_TRAILING
                    } else {
                        paths::PLACE_TPSL
                    },
                    self.algo_payload(cx, o)?,
                )
            };
            calls.push(Planned {
                path,
                payload,
                effect: Effect::Place { expected: 1 },
            });
        }
        Ok(calls)
    }

    fn normal_payload(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        amount: f64,
    ) -> Result<Payload, String> {
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let reduce = o.reduce_only.unwrap_or(false);
        let hedge = cx.store.hedged();
        let mut p = Payload {
            client_oid: Some(o.client_order_id.clone().map_or_else(|| mint(cx), Ok)?),
            price: o
                .price
                .filter(|_| o.kind != OrderType::Market)
                .map(|v| number(v, m.precision.price)),
            side: Some(side_text(if hedge && reduce && !self.account.uta {
                o.side.opposite()
            } else {
                o.side
            })),
            order_type: Some(if o.kind == OrderType::Limit {
                "limit"
            } else {
                "market"
            }),
            ..Payload::default()
        };
        if self.account.uta {
            p.category = Some(self.product().to_uppercase());
            p.symbol = Some(o.symbol.clone());
            p.qty = Some(num::format(amount));
            p.pos_side = hedge.then_some(if reduce {
                protected_side(o.side)
            } else {
                opening_side(o.side)
            });
            p.margin_mode = Some(margin_text(
                cx.store
                    .setting(&o.symbol)
                    .and_then(|s| s.margin_type)
                    .unwrap_or(MarginType::Cross),
            ));
            p.reduce_only = (!hedge).then_some(if reduce { "yes" } else { "no" });
            p.time_in_force = (o.kind == OrderType::Limit).then_some(tif(o.time_in_force));
            p.take_profit = o.take_profit.map(|v| number(v, m.precision.price));
            p.stop_loss = o.stop_loss.map(|v| number(v, m.precision.price));
            p.tp_trigger_by = o.take_profit.map(|_| uta_trigger(o.tp_trigger_type));
            p.sl_trigger_by = o.stop_loss.map(|_| uta_trigger(o.sl_trigger_type));
            p.tp_order_type = o.take_profit.map(|_| "market");
            p.sl_order_type = o.stop_loss.map(|_| "market");
        } else {
            p.size = Some(num::format(amount));
            p.trade_side = hedge.then_some(if reduce { "close" } else { "open" });
            p.force = (o.kind == OrderType::Limit).then_some(tif(o.time_in_force));
            p.reduce_only = (!hedge).then_some(if reduce { "YES" } else { "NO" });
            p.preset_stop_surplus_price = o.take_profit.map(|v| number(v, m.precision.price));
            p.preset_stop_loss_price = o.stop_loss.map(|v| number(v, m.precision.price));
        }
        Ok(p)
    }

    fn plan_size(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<(String, f64), String> {
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let is_tp = o.kind == OrderType::TakeProfit;
        let position = cx.store.positions().iter().find(|p| {
            p.symbol == o.symbol && p.side == protected_position(o.side) && p.contracts > 0.0
        });
        let capacity = position.map_or(0.0, |p| {
            floor_lot(
                if self.inverse() {
                    p.notional
                } else {
                    p.contracts
                },
                m.precision.amount,
            )
        });
        let requested = num::adjust(o.amount, m.precision.amount);
        let size = if o.amount == 0.0 || capacity < m.precision.amount {
            0.0
        } else {
            requested.min(capacity)
        };
        Ok((
            if size > 0.0 {
                if is_tp { "profit_plan" } else { "loss_plan" }
            } else if is_tp {
                "pos_profit"
            } else {
                "pos_loss"
            }
            .into(),
            size,
        ))
    }

    fn algo_payload(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<Payload, String> {
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let (plan, size) = self.plan_size(cx, o)?;
        let mut p = self.classic(cx, &o.symbol)?;
        p.client_oid = Some(mint(cx)?);
        p.reduce_only = (!cx.store.hedged()).then_some("YES");
        p.trigger_price = o.price.map(|v| number(v, m.precision.price));
        p.trigger_type = Some(trigger(if o.kind == OrderType::TakeProfit {
            o.tp_trigger_type.or(o.trigger_type)
        } else {
            o.sl_trigger_type.or(o.trigger_type)
        }));
        if o.kind == OrderType::TrailingStopLoss {
            let (size, _, ratio) = self.trailing_values(cx, o, None)?;
            p.size = Some(num::format(size));
            p.plan_type = Some("track_plan".into());
            p.side = Some(side_text(if cx.store.hedged() {
                o.side.opposite()
            } else {
                o.side
            }));
            p.trade_side = cx.store.hedged().then_some("close");
            p.order_type = Some("market");
            p.margin_mode = Some(margin_text(
                cx.store
                    .setting(&o.symbol)
                    .and_then(|s| s.margin_type)
                    .unwrap_or(MarginType::Cross),
            ));
            p.callback_ratio = Some(num::format(ratio));
        } else {
            p.plan_type = Some(plan);
            p.size = (size > 0.0).then(|| num::format(size));
            p.hold_side = Some(if cx.store.hedged() {
                protected_side(o.side)
            } else if o.side == OrderSide::Sell {
                "buy"
            } else {
                "sell"
            });
        }
        Ok(p)
    }

    fn trailing_values(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        retained: Option<(&str, f64)>,
    ) -> Result<(f64, f64, f64), String> {
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let stop = o
            .price
            .filter(|p| p.is_finite() && *p > 0.0)
            .ok_or("Bitget trailing stop requires a positive trigger price")?;
        let size = num::adjust(o.amount, m.precision.amount);
        if !size.is_finite() || size <= 0.0 {
            return Err("Bitget trailing stop requires an explicit positive size".into());
        }
        if let Some((kind, variance)) = retained {
            if !variance.is_finite()
                || variance <= 0.0
                || !matches!(kind, "ratio" | "distance")
                || (!self.account.uta && (kind != "ratio" || variance > 10.0))
            {
                return Err(
                    "Bitget native trailing parameters are invalid; refresh before editing".into(),
                );
            }
            return Ok((size, variance, variance));
        }
        let ticker = cx
            .store
            .ticker(&o.symbol)
            .filter(|t| t.last.is_finite() && t.last > 0.0)
            .ok_or("Bitget trailing stop requires a positive ticker")?;
        let distance = num::adjust((ticker.last - stop).abs(), m.precision.price);
        if !distance.is_finite() || distance <= 0.0 {
            return Err("Bitget trailing distance must be at least one price tick".into());
        }
        let ratio = num::adjust(distance * 100.0 / ticker.last, 0.1);
        if !self.account.uta && (!ratio.is_finite() || ratio <= 0.0 || ratio > 10.0) {
            return Err(
                "Bitget trailing callback percentage must be greater than zero and at most 10"
                    .into(),
            );
        }
        Ok((size, distance, ratio))
    }

    fn strategy_payload(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
    ) -> Result<Payload, String> {
        self.strategy_payload_with_trailing(cx, o, None)
    }

    fn strategy_payload_with_trailing(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        retained: Option<(&str, f64)>,
    ) -> Result<Payload, String> {
        if o.kind == OrderType::TrailingStopLoss {
            let (size, distance, _) = self.trailing_values(cx, o, retained)?;
            let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
            return Ok(Payload {
                category: Some(self.product().to_uppercase()),
                symbol: Some(o.symbol.clone()),
                kind: Some("trailing_stop"),
                side: Some(side_text(o.side)),
                pos_side: cx.store.hedged().then_some(protected_side(o.side)),
                qty: Some(num::format(size)),
                reduce_only: (!cx.store.hedged()).then_some("yes"),
                margin_mode: Some(margin_text(
                    cx.store
                        .setting(&o.symbol)
                        .and_then(|s| s.margin_type)
                        .unwrap_or(MarginType::Cross),
                )),
                // UTA only accepts clientOid for tpsl. Distance avoids an
                // undocumented ratio unit conversion and keeps chart prices exact.
                trailing_stop_params: Some(vec![TrailingParams {
                    activation_price: number(
                        o.price.ok_or("trailing price is required")?,
                        m.precision.price,
                    ),
                    activation_type: uta_trigger(o.sl_trigger_type.or(o.trigger_type)),
                    trail_type: if retained.is_some_and(|(kind, _)| kind == "ratio") {
                        "ratio"
                    } else {
                        "distance"
                    },
                    trail_variance: num::format(distance),
                    pre_order_type: "market",
                }]),
                ..Payload::default()
            });
        }
        if !matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            return Err("unsupported Bitget UTA strategy type".into());
        }
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let (_, mut size) = self.plan_size(cx, o)?;
        if size == 0.0 {
            size = cx
                .store
                .positions()
                .iter()
                .find(|p| {
                    p.symbol == o.symbol
                        && (!cx.store.hedged() || p.side == protected_position(o.side))
                        && p.contracts > 0.0
                })
                .map_or(0.0, |p| num::adjust(p.contracts, m.precision.amount));
        }
        let mut p = Payload {
            category: Some(self.product().to_uppercase()),
            symbol: Some(o.symbol.clone()),
            kind: Some("tpsl"),
            side: Some(side_text(o.side)),
            pos_side: cx.store.hedged().then_some(protected_side(o.side)),
            tpsl_mode: Some(if size > 0.0 { "partial" } else { "full" }),
            qty: (size > 0.0).then(|| num::format(size)),
            reduce_only: Some("yes"),
            client_oid: Some(mint(cx)?),
            ..Payload::default()
        };
        if o.kind == OrderType::TakeProfit {
            p.take_profit = o.price.map(|v| number(v, m.precision.price));
            p.tp_trigger_by = Some(uta_trigger(o.tp_trigger_type.or(o.trigger_type)));
            p.tp_order_type = Some("market");
        } else {
            p.stop_loss = o.price.map(|v| number(v, m.precision.price));
            p.sl_trigger_by = Some(uta_trigger(o.sl_trigger_type.or(o.trigger_type)));
            p.sl_order_type = Some("market");
        }
        Ok(p)
    }

    fn cancel_calls(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[Order],
        idempotent_missing: bool,
    ) -> Result<Vec<Planned>, String> {
        let mut calls = vec![];
        let mut normal: Vec<(String, Vec<Order>)> = vec![];
        let mut seen = HashSet::new();
        for order in orders {
            let id = native_id(order);
            let strategy = is_algo(order.kind)
                && (order.parent_id.is_none()
                    || self
                        .account
                        .plans
                        .get(&order.id)
                        .is_some_and(|p| p == "uta_tpsl"));
            if !seen.insert((strategy, id.clone())) {
                continue;
            }
            if self.account.uta {
                let payload = Payload {
                    category: (!strategy).then(|| self.product().to_uppercase()),
                    order_id: (!order
                        .client_order_id
                        .as_ref()
                        .is_some_and(|client| client == &id))
                    .then_some(id),
                    client_oid: order.client_order_id.clone(),
                    ..Payload::default()
                };
                calls.push(Planned {
                    path: if strategy {
                        paths::UTA_CANCEL_STRATEGY
                    } else {
                        paths::UTA_CANCEL
                    },
                    payload,
                    effect: Effect::Cancel {
                        orders: related(orders, order),
                        idempotent_missing: strategy && idempotent_missing,
                    },
                });
            } else if strategy {
                let mut payload = self.classic(cx, &order.symbol)?;
                if order.kind == OrderType::TrailingStopLoss {
                    payload.order_id_list = Some(vec![CancelId { order_id: id }]);
                } else {
                    payload.order_id = Some(id);
                }
                payload.plan_type = Some(
                    self.account
                        .plans
                        .get(&order.id)
                        .and_then(|v| plan_type(v))
                        .unwrap_or_else(|| fallback_plan(order))
                        .into(),
                );
                calls.push(Planned {
                    path: paths::CANCEL_PLAN,
                    payload,
                    effect: Effect::Cancel {
                        orders: vec![order.clone()],
                        idempotent_missing: false,
                    },
                });
            } else if let Some((_, rows)) = normal.iter_mut().find(|(s, _)| s == &order.symbol) {
                rows.push(order.clone());
            } else {
                normal.push((order.symbol.clone(), vec![order.clone()]));
            }
        }
        let mut normal_calls = vec![];
        for (symbol, rows) in normal {
            for chunk in rows.chunks(50) {
                let mut p = self.classic(cx, &symbol)?;
                p.order_id_list = Some(
                    chunk
                        .iter()
                        .map(|o| CancelId {
                            order_id: native_id(o),
                        })
                        .collect(),
                );
                normal_calls.push(Planned {
                    path: paths::BATCH_CANCEL,
                    payload: p,
                    effect: Effect::Cancel {
                        orders: chunk.iter().flat_map(|o| related(orders, o)).collect(),
                        idempotent_missing: false,
                    },
                });
            }
        }
        normal_calls.extend(calls);
        Ok(normal_calls)
    }

    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        job: &mut Job,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<(), String> {
        let order = cx
            .store
            .order(id)
            .cloned()
            .ok_or("Bitget order not found")?;
        if matches!(
            order.status,
            OrderStatus::Closed | OrderStatus::Canceled | OrderStatus::Canceling
        ) {
            return Err("Bitget order is terminal or canceling; refresh before amending".into());
        }
        let m = cx.store.market(&order.symbol).ok_or("unknown market")?;
        let next_price = num::adjust(price.unwrap_or(order.price), m.precision.price);
        let next_amount = num::adjust(amount.unwrap_or(order.amount), m.precision.amount);
        if !next_price.is_finite()
            || next_price <= 0.0
            || !next_amount.is_finite()
            || next_amount < 0.0
        {
            return Err("invalid Bitget amendment".into());
        }
        let linked: Vec<_> = cx
            .store
            .orders()
            .iter()
            .filter(|o| native_id(o) == native_id(&order))
            .cloned()
            .collect();
        let standalone = is_algo(order.kind)
            && (order.parent_id.is_none()
                || self
                    .account
                    .plans
                    .get(&order.id)
                    .is_some_and(|p| p == "uta_tpsl"));
        if standalone && !self.account.uta && order.kind == OrderType::TrailingStopLoss {
            let mut opts = place_opts(&order);
            opts.price = Some(next_price);
            opts.amount = next_amount;
            let retained = if price.is_none() {
                Some(self.account.trailing.get(&order.id)
                    .map(|(kind, variance)| (kind.as_str(), *variance))
                    .ok_or("Bitget native trailing parameters are unavailable; refresh before editing")?)
            } else {
                None
            };
            let (_, _, ratio) = self.trailing_values(cx, &opts, retained)?;
            job.calls.push_back(Planned {
                path: "/api/v2/mix/order/modify-plan-order",
                payload: Payload {
                    product_type: Some(self.product().into()),
                    order_id: Some(native_id(&order)),
                    new_size: amount.map(|_| num::format(next_amount)),
                    new_trigger_price: price.map(|_| num::format(next_price)),
                    new_callback_ratio: Some(num::format(ratio)),
                    ..Payload::default()
                },
                effect: Effect::Place { expected: 1 },
            });
            return Ok(());
        }
        if standalone && !self.account.uta {
            let mut p = self.classic(cx, &order.symbol)?;
            p.order_id = Some(native_id(&order));
            p.trigger_price = price.map(|_| num::format(next_price));
            p.size = Some(if amount.is_some_and(|a| a > 0.0) {
                num::format(next_amount)
            } else if order.amount > 0.0 {
                number(order.amount, m.precision.amount)
            } else {
                String::new()
            });
            job.calls.push_back(Planned {
                path: paths::MODIFY_TPSL,
                payload: p,
                effect: Effect::Place { expected: 1 },
            });
            return Ok(());
        }
        let mut opts = place_opts(&order);
        opts.client_order_id = None;
        let attached = linked.iter().find(|o| o.kind == OrderType::Limit);
        if let Some(parent) = attached {
            opts = place_opts(parent);
            opts.client_order_id = None;
            opts.stop_loss = linked
                .iter()
                .find(|o| o.kind == OrderType::StopLoss)
                .map(|o| if o.id == id { next_price } else { o.price });
            opts.take_profit = linked
                .iter()
                .find(|o| o.kind == OrderType::TakeProfit)
                .map(|o| if o.id == id { next_price } else { o.price });
            opts.price = Some(if parent.id == id {
                next_price
            } else {
                parent.price
            });
            opts.amount = amount.unwrap_or(parent.amount);
        } else {
            opts.price = Some(next_price);
            opts.amount = next_amount;
        }
        let mut replacement = if standalone && self.account.uta {
            vec![]
        } else {
            self.place_calls(cx, &[opts.clone()])?
        };
        if standalone && self.account.uta {
            let retained = if order.kind == OrderType::TrailingStopLoss {
                Some(self.account.trailing.get(&order.id)
                    .map(|(kind, variance)| (kind.as_str(), *variance))
                    .ok_or("Bitget native trailing parameters are unavailable; refresh before editing")?)
            } else {
                None
            };
            let mut original =
                self.strategy_payload_with_trailing(cx, &place_opts(&order), retained)?;
            let mut next = self.strategy_payload_with_trailing(
                cx,
                &opts,
                if price.is_none() { retained } else { None },
            )?;
            if linked.len() > 1 {
                for related in &linked {
                    let original_price = number(related.price, m.precision.price);
                    let changed = if related.id == id {
                        number(next_price, m.precision.price)
                    } else {
                        original_price.clone()
                    };
                    if related.kind == OrderType::StopLoss {
                        original.stop_loss = Some(original_price);
                        next.stop_loss = Some(changed);
                        original.sl_order_type = Some("market");
                        next.sl_order_type = Some("market");
                        original.sl_trigger_by = Some(uta_trigger(related.trigger_type));
                        next.sl_trigger_by = Some(uta_trigger(related.trigger_type));
                    } else if related.kind == OrderType::TakeProfit {
                        original.take_profit = Some(original_price);
                        next.take_profit = Some(changed);
                        original.tp_order_type = Some("market");
                        next.tp_order_type = Some("market");
                        original.tp_trigger_by = Some(uta_trigger(related.trigger_type));
                        next.tp_trigger_by = Some(uta_trigger(related.trigger_type));
                    }
                }
            }
            replacement = vec![Planned {
                path: paths::UTA_STRATEGY,
                payload: next,
                effect: Effect::Place { expected: 1 },
            }];
            job.rollback = Some(Planned {
                path: paths::UTA_STRATEGY,
                payload: original,
                effect: Effect::Place { expected: 1 },
            });
        }
        let mut fallback: VecDeque<_> = self.cancel_calls(cx, &linked, false)?.into();
        fallback.extend(replacement);
        if !standalone
            && order.kind == OrderType::Limit
            && attached.is_some_and(|_| linked.len() == 1)
            && !self.account.uta
        {
            if next_amount <= 0.0 {
                return Err("Bitget amended size must be positive".into());
            }
            let client = mint(cx)?;
            let payload = Payload {
                symbol: Some(order.symbol.clone()),
                product_type: Some(self.product().into()),
                order_id: Some(order.id.clone()),
                new_client_oid: Some(client.clone()),
                new_price: Some(num::format(next_price)),
                new_size: Some(num::format(next_amount)),
                ..Payload::default()
            };
            job.calls.push_back(Planned {
                path: paths::MODIFY_ORDER,
                payload,
                effect: Effect::Amend {
                    id: order.id,
                    price: next_price,
                    amount: next_amount,
                    client,
                },
            });
            job.fallback = Some(fallback);
        } else {
            job.calls = fallback;
        }
        Ok(())
    }

    /// One call's DATA, validated as this Plan's exact delivery before decode.
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Some(mut job) = self.trading.active.take() else {
            return;
        };
        let delivered = job.id == request.command
            && job
                .plan
                .as_ref()
                .is_some_and(|plan| cx.validate_plan_delivery(plan, &request.context).is_ok());
        if !delivered {
            self.trading.active = Some(job);
            return;
        }
        let current = Some((&request.slot, &request.context));
        let read = request.call.read();
        if serde_json::from_str::<Envelope<serde::de::IgnoredAny>>(response.body)
            .ok()
            .is_some_and(|r| r.code.0 == Some(40085.0))
            && self.promote_uta(cx)
        {
            restore_canceling(cx, &request.call);
            // A 5xx proves nothing: the call's fact is Unknown and reconciles.
            let reply = serde_json::from_str::<Reply>(response.body).ok();
            let message = reply.as_ref().map_or_else(String::new, |reply| {
                format!(
                    "Bitget {}: {} (HTTP {})",
                    reply.code.get(),
                    reply.msg,
                    response.status
                )
            });
            let native = native_code(reply.as_ref(), response.status);
            let proven = read
                || self
                    .record_once(cx, &request, native, Answer::Coded, &message, &mut false)
                    .0;
            if job.ids.is_empty() && proven {
                job.calls.clear();
                job.planned = false;
                job.positions_checked = false;
                job.waiting_positions = false;
                job.metadata = symbols(cx, &job.intent).into();
                job.fallback = None;
                job.rollback = None;
                self.trading.metadata_at.clear();
                return self.drive(cx, job, At::Data(&request.slot, &request.context));
            }
            let error = if proven {
                VenueError::new(format!(
                    "Bitget switched to UTA after partial success; acknowledged IDs {:?}",
                    job.ids
                ))
            } else {
                VenueError::new("Bitget requires UTA (HTTP status unproven)")
                    .outcome(Outcome::Unknown)
            };
            self.end(cx, job, current, error);
            if !proven {
                self.refresh_orders(cx, false);
            }
            return;
        }
        if let Effect::Metadata(symbol) = &request.call.effect {
            let resolved = if self.account.uta {
                let parsed = (response.status == 200)
                    .then(|| serde_json::from_str::<Envelope<UtaSettings>>(response.body).ok())
                    .flatten()
                    .and_then(|e| e.checked().ok());
                match parsed {
                    Some(settings) if !settings.hold_mode.is_empty() => {
                        self.apply_settings(cx, &settings);
                        if cx.store.setting(symbol).is_none() {
                            cx.store.update_setting(symbol, |s| {
                                s.leverage = 1.0;
                                s.margin_type = Some(MarginType::Cross);
                            });
                        }
                        Ok(())
                    }
                    _ => Err("Bitget UTA metadata could not be resolved"),
                }
            } else {
                let parsed = (response.status == 200)
                    .then(|| serde_json::from_str::<Envelope<SymbolConfig>>(response.body).ok())
                    .flatten()
                    .and_then(|e| e.checked().ok());
                match parsed {
                    Some(row) if !row.pos_mode.is_empty() && !row.margin_mode.is_empty() => {
                        self.apply_symbol_config(cx, symbol, &row);
                        Ok(())
                    }
                    _ => Err("Bitget position mode or margin type could not be resolved"),
                }
            };
            return match resolved {
                Ok(()) => {
                    self.trading.metadata_at.insert(symbol.clone(), cx.now);
                    self.drive(cx, job, At::Data(&request.slot, &request.context))
                }
                Err(error) => self.end(cx, job, current, not_sent(error)),
            };
        }
        let status = response.status;
        let parsed = serde_json::from_str::<Reply>(response.body).map_err(|e| e.to_string());
        if let Effect::ResolveAmend { id, .. } = &request.call.effect {
            let ready = parsed.as_ref().is_ok_and(|reply| {
                status == 200
                    && reply.code.0 == Some(0.0)
                    && reply.data.as_ref().is_some_and(|d| {
                        !d.order_id.text().is_empty() && d.order_id.text().as_ref() != id
                    })
            });
            if !ready && request.read_retries < 3 {
                return self.retry_read(cx, job, request);
            }
        }
        let explicit_rejection = parsed.as_ref().is_ok_and(|reply| {
            status < 500
                && status != 408
                && !reply
                    .code
                    .0
                    .is_some_and(|code| [40010.0, 40725.0, 45001.0].contains(&code))
                && (status >= 400 || reply.code.0.is_some_and(|code| code != 0.0))
        });
        // The one native resend: a fresh Context re-signed under the same owner.
        if !read
            && parsed
                .as_ref()
                .is_ok_and(|reply| reply.code.0 == Some(40009.0))
            && !request.signature_retry
        {
            let message = format!("Bitget 40009: sign signature error (HTTP {status})");
            let native = native_code(parsed.as_ref().ok(), status);
            let (_, grant) =
                self.record_once(cx, &request, native, Answer::Coded, &message, &mut false);
            let plan = job.plan.clone().expect("live Plan");
            if let Some(context) = grant.and_then(|grant| {
                cx.plan_resend_context(&plan, &request.slot, &request.context, grant)
                    .ok()
            }) {
                let request = Request {
                    context,
                    signature_retry: true,
                    ..request
                };
                return self.send_call(cx, job, request);
            }
            return self.call_settled(cx, job, request, parsed, status, explicit_rejection, true);
        }
        self.call_settled(cx, job, request, parsed, status, explicit_rejection, false)
    }

    /// The native answer of a call that will not be resent: record its write
    /// fact once, then fall back, roll back, fail, or continue as native.
    #[allow(clippy::too_many_arguments)]
    #[inline(never)]
    fn call_settled(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut job: Job,
        request: Request,
        parsed: Result<Reply<'_>, String>,
        status: u16,
        explicit_rejection: bool,
        mut recorded: bool,
    ) {
        let current = Some((&request.slot, &request.context));
        let native = native_code(parsed.as_ref().ok(), status);
        let mut error = match &parsed {
            Ok(reply) if status == 200 && reply.code.0 == Some(0.0) => None,
            Ok(reply) => Some(format!(
                "Bitget {}: {} (HTTP {status})",
                reply.code.get(),
                reply.msg
            )),
            Err(error) => Some(error.clone()),
        };
        if let Effect::Cancel { idempotent_missing: true, .. } = &request.call.effect
            && parsed.as_ref().is_ok_and(|reply| reply.msg == "Failed to stop the strategy because the corresponding strategy ID could not be found") { error = None; }
        let mut answer = if error.is_none() {
            Answer::Accepted(vec![])
        } else {
            Answer::Coded
        };
        if error.is_some()
            && request.call.is_amend()
            && explicit_rejection
            && let Some(fallback) = job.fallback.take()
        {
            let message = error.clone().unwrap_or_default();
            self.record_once(cx, &request, native, answer, &message, &mut recorded);
            job.calls = fallback;
            return self.drive(cx, job, At::Data(&request.slot, &request.context));
        }
        let mut write_noted = false;
        // A 2xx success answer that does not account for every order.
        let mut unproven = false;
        if error.is_none()
            && let Effect::Cancel { orders, .. } = &request.call.effect
        {
            let data = parsed.as_ref().ok().and_then(|r| r.data.as_ref());
            let retained = orders
                .iter()
                .any(|o| self.trading.acknowledged.contains_key(&native_id(o)));
            let mut accepted: HashSet<_> = data.map_or_else(HashSet::new, |d| {
                d.success_list
                    .iter()
                    .map(|i| i.order_id.text().into_owned())
                    .collect()
            });
            if let Some(data) = data {
                let native = data.order_id.text();
                let client = data.client_oid.text();
                for order in orders {
                    if !native.is_empty() && native == native_id(order)
                        || !client.is_empty()
                            && request.call.payload.client_oid.as_deref() == Some(client.as_ref())
                    {
                        accepted.insert(native_id(order));
                    }
                }
            }
            for order in orders {
                if accepted.contains(&native_id(order)) {
                    self.trading.acknowledged.remove(&native_id(order));
                    self.mark_canceled(cx, &order.id);
                    if !write_noted {
                        self.note_write(cx);
                        write_noted = true;
                    }
                }
            }
            if let Some(failures) = data
                .map(|d| failure_messages(&d.failure_list))
                .filter(|f| !f.is_empty())
            {
                error = Some(format!(
                    "Bitget cancel failure: {}",
                    failure_text(&failures)
                ));
                answer = Answer::Batch {
                    expected: orders.len(),
                    accepted: accepted.into_iter().collect(),
                    failures,
                };
            } else if (request.call.path == paths::BATCH_CANCEL || retained)
                && orders.iter().any(|o| !accepted.contains(&native_id(o)))
            {
                unproven = true;
                answer = Answer::Unproven;
                error = Some("Bitget batch cancel did not acknowledge every order".into());
            }
        }
        if error.is_none() {
            let data = parsed.as_ref().ok().and_then(|r| r.data.as_ref());
            if let Effect::Place { expected } = request.call.effect {
                let mut ids: Vec<_> = data.map_or_else(Vec::new, |d| {
                    d.success_list
                        .iter()
                        .map(|i| i.order_id.text().into_owned())
                        .filter(|s| !s.is_empty())
                        .collect()
                });
                if ids.is_empty()
                    && let Some(d) = data
                {
                    let native = d.order_id.text();
                    let client = d.client_oid.text();
                    let id = if !native.is_empty() { native } else { client };
                    if !id.is_empty() {
                        ids.push(id.into_owned());
                    }
                }
                answer = Answer::Accepted(ids.clone());
                if let Some(failures) = data
                    .map(|d| failure_messages(&d.failure_list))
                    .filter(|f| !f.is_empty())
                {
                    error = Some(format!(
                        "Bitget batch failure after ids {ids:?}: {}",
                        failure_text(&failures)
                    ));
                    answer = Answer::Batch {
                        expected,
                        accepted: ids.clone(),
                        failures,
                    };
                } else if ids.len() != expected {
                    unproven = true;
                    answer = Answer::Unproven;
                    error = Some(format!(
                        "Bitget acknowledged {} of {expected} orders; ids {ids:?}",
                        ids.len()
                    ));
                }
                if !ids.is_empty() {
                    self.acknowledge(cx, &request, data, &ids);
                    self.note_write(cx);
                    write_noted = true;
                }
                if error.is_none() {
                    self.retain_trailing(&request, &ids);
                }
                job.ids.extend(ids);
            }
        }
        if let Some(message) = error {
            self.record_once(cx, &request, native, answer, &message, &mut recorded);
            let error = match &parsed {
                _ if unproven => VenueError::new(message).outcome(Outcome::Unknown),
                Ok(reply) if explicit_rejection => {
                    // The table's kind for the native code, else its documented
                    // status row; a 401/403/429 carries no code (`native_code`).
                    // Place reads the row any intent would: every row is unscoped
                    // (pinned in code_table_tests).
                    let kind = native
                        .as_ref()
                        .and_then(|native| CODES.verdict(native, Intent::Place).kind())
                        .or_else(|| {
                            CODES
                                .status
                                .iter()
                                .find(|(documented, _)| *documented == status)
                                .and_then(|(_, verdict)| verdict.kind())
                        })
                        .unwrap_or(ErrorKind::Rejected);
                    let error = VenueError::new(message).kind(kind);
                    let error = match reply.code.0 {
                        Some(code) => error.venue_code(code.to_string()),
                        None => error,
                    };
                    error.outcome(Outcome::Rejected)
                }
                // Explicit per-order refusals in a 2xx answer, without a documented code table.
                Ok(reply) if status == 200 && reply.code.0 == Some(0.0) => VenueError::new(message),
                // 408, 5xx, malformed answers and Bitget's ambiguous codes.
                _ => VenueError::new(message).outcome(Outcome::Unknown),
            };
            if request.call.is_place() {
                self.account.orders_version = self.account.orders_version.wrapping_add(1);
            }
            restore_canceling(cx, &request.call);
            if explicit_rejection
                && request.call.is_place()
                && job.error.is_none()
                && let Some(rollback) = job.rollback.take()
            {
                job.error = Some(error);
                job.calls.clear();
                job.calls.push_back(rollback);
                return self.drive(cx, job, At::Data(&request.slot, &request.context));
            }
            self.end(cx, job, current, error);
            self.refresh_orders(cx, false);
            return;
        }
        match &request.call.effect {
            Effect::Cancel { orders, .. } => {
                for order in orders {
                    self.mark_canceled(cx, &order.id);
                }
            }
            Effect::Amend {
                id,
                price,
                amount,
                client,
            } => {
                let next = parsed
                    .as_ref()
                    .ok()
                    .and_then(|reply| reply.data.as_ref())
                    .map(|d| d.order_id.text().into_owned())
                    .filter(|s| !s.is_empty() && s != id);
                if let Some(next) = next {
                    self.commit_amend(
                        cx,
                        id,
                        &next,
                        (
                            *price,
                            *amount,
                            job.original.clone(),
                            request.stream_revision,
                        ),
                        client,
                    );
                    answer = Answer::Accepted(vec![next.clone()]);
                    job.ids.push(next);
                } else {
                    // Frame-fed too: this read is part of the amend's own sequence, as LOCAL.
                    job.calls.push_front(Planned {
                        path: paths::ORDER_DETAIL,
                        payload: Payload {
                            symbol: request.call.payload.symbol.clone(),
                            ..Payload::default()
                        },
                        effect: Effect::ResolveAmend {
                            id: id.clone(),
                            price: *price,
                            amount: *amount,
                            client: client.clone(),
                        },
                    });
                }
            }
            Effect::ResolveAmend {
                id,
                price,
                amount,
                client,
            } => {
                let next = parsed
                    .as_ref()
                    .ok()
                    .and_then(|reply| reply.data.as_ref())
                    .map(|d| d.order_id.text().into_owned())
                    .filter(|s| !s.is_empty() && s != id);
                let Some(next) = next else {
                    let error = VenueError::new(
                        "Bitget amendment identity is not available; refresh before further edits",
                    );
                    self.end(cx, job, current, error);
                    self.refresh_orders(cx, false);
                    return;
                };
                let newer_stream = self.order_revision(&next) > request.stream_revision;
                self.commit_amend(
                    cx,
                    id,
                    &next,
                    (
                        *price,
                        *amount,
                        job.original.clone(),
                        request.stream_revision,
                    ),
                    client,
                );
                if !self.order_terminal(&next)
                    && !newer_stream
                    && let Some(data) = parsed.as_ref().ok().and_then(|r| r.data.as_ref())
                {
                    let inverse = self.inverse();
                    cx.store.update_order(&next, |o| {
                        if !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled) {
                            if let Some(filled) = data.base_volume.0 {
                                o.filled = filled;
                                amend_remaining(o, inverse);
                            }
                            if let Some(status) = crate::account::order_status(&data.status) {
                                o.status = status;
                            }
                        }
                    });
                    if crate::account::order_status(&data.status)
                        .is_some_and(|s| matches!(s, OrderStatus::Closed | OrderStatus::Canceled))
                    {
                        self.mark_canceled(cx, &next);
                    }
                }
                job.ids.push(next);
            }
            effect @ (Effect::Leverage { .. } | Effect::Margin { .. } | Effect::Mode(_)) => {
                self.apply_setting(cx, effect)
            }
            _ => {}
        }
        if !request.call.read() {
            self.record_once(cx, &request, native, answer, "", &mut recorded);
        }
        if !write_noted && request.call.path != paths::ORDER_DETAIL {
            self.note_write(cx);
        }
        if request.call.path != paths::ORDER_DETAIL {
            job.effective = true;
        }
        self.drive(cx, job, At::Data(&request.slot, &request.context));
    }

    /// Records a write's journal fact exactly once; reads keep none. Returns
    /// whether a coded answer proves a refusal, and its resend grant.
    #[inline(never)]
    fn record_once(
        &self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        native: Option<Native<'_>>,
        answer: Answer,
        message: &str,
        recorded: &mut bool,
    ) -> (bool, Option<ResendGrant>) {
        if *recorded || request.call.read() {
            return (false, None);
        }
        *recorded = true;
        let context = &request.context;
        let message = message.to_owned();
        let fact = match answer {
            Answer::Accepted(order_ids) => WriteFact::Accepted { order_ids },
            Answer::Batch {
                expected,
                accepted,
                failures,
            } => {
                let mut items: Vec<_> = accepted
                    .iter()
                    .map(|id| BatchItem::Accepted(id.as_str()))
                    .collect();
                items.extend(failures.iter().map(|(code, message)| {
                    BatchItem::Native(Native {
                        code: code.map_or(NativeCode::None, NativeCode::Int),
                        message: message.as_str(),
                    })
                }));
                let _ = cx.http_write_batch(context, &CODES, expected, &items, message, None);
                return (false, None);
            }
            Answer::Unproven => {
                let _ = cx.http_write_unproven(context, message);
                return (false, None);
            }
            Answer::Coded => match cx.classify_write(context, &CODES, native.as_ref(), message) {
                Ok(fact) => WriteFact::Failed {
                    fact,
                    known_effects: false,
                },
                Err(_) => return (false, None),
            },
        };
        let refusal =
            matches!(&fact, WriteFact::Failed { fact, .. } if fact.verdict().is_refusal());
        let grant = cx.http_write_fact(context, fact, None).ok().flatten();
        (refusal, grant)
    }

    /// Native placement custody: each acknowledged ID keeps its own cancel call
    /// until the stream observes it.
    fn acknowledge(
        &mut self,
        cx: &Cx<'_, Pending>,
        request: &Request,
        data: Option<&ReplyData<'_>>,
        ids: &[String],
    ) {
        let p = &request.call.payload;
        let symbol = p.symbol.clone().unwrap_or_default();
        for id in ids {
            if cx
                .store
                .orders()
                .iter()
                .any(|o| o.id == *id || o.parent_id.as_ref() == Some(id))
                || self.order_terminal(id)
            {
                continue;
            }
            let strategy = matches!(
                request.call.path,
                paths::PLACE_TPSL | paths::PLACE_TRAILING | paths::UTA_STRATEGY
            );
            let mut payload = if self.account.uta {
                Payload {
                    category: (!strategy).then(|| self.product().to_uppercase()),
                    order_id: Some(id.clone()),
                    ..Payload::default()
                }
            } else {
                let mut payload = Payload {
                    symbol: Some(symbol.clone()),
                    product_type: p.product_type.clone(),
                    margin_coin: p.margin_coin.clone(),
                    ..Payload::default()
                };
                if strategy {
                    payload.plan_type = p.plan_type.clone();
                    if request.call.path == paths::PLACE_TRAILING {
                        payload.order_id_list = Some(vec![CancelId {
                            order_id: id.clone(),
                        }]);
                    } else {
                        payload.order_id = Some(id.clone());
                    }
                } else {
                    payload.order_id_list = Some(vec![CancelId {
                        order_id: id.clone(),
                    }]);
                }
                payload
            };
            if p.client_oid.as_deref() == Some(id)
                && data.is_some_and(|d| d.order_id.text().is_empty())
            {
                // Native identity was not returned: only UTA accepts a client-id cancel here.
                if self.account.uta {
                    payload.order_id = None;
                    payload.client_oid = Some(id.clone());
                }
            }
            let kind = if strategy {
                if request.call.path == paths::PLACE_TRAILING {
                    OrderType::TrailingStopLoss
                } else if p.plan_type.as_deref().is_some_and(|t| t.contains("profit")) {
                    OrderType::TakeProfit
                } else {
                    OrderType::StopLoss
                }
            } else {
                OrderType::Limit
            };
            let order = Order {
                id: id.clone(),
                symbol: symbol.clone(),
                kind,
                parent_id: None,
                status: OrderStatus::Open,
                side: OrderSide::Buy,
                price: 0.0,
                amount: 0.0,
                filled: 0.0,
                remaining: 0.0,
                reduce_only: false,
                close: None,
                client_order_id: None,
                time_in_force: None,
                last_update: cx.now,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: None,
            };
            let unresolved = !self.account.uta
                && p.client_oid.as_deref() == Some(id)
                && data.is_some_and(|d| d.order_id.text().is_empty());
            self.trading.acknowledged.insert(
                id.clone(),
                (
                    symbol.clone(),
                    Planned {
                        path: if unresolved {
                            ""
                        } else if self.account.uta {
                            if strategy {
                                paths::UTA_CANCEL_STRATEGY
                            } else {
                                paths::UTA_CANCEL
                            }
                        } else if strategy {
                            paths::CANCEL_PLAN
                        } else {
                            paths::BATCH_CANCEL
                        },
                        payload,
                        effect: Effect::Cancel {
                            orders: vec![order],
                            idempotent_missing: true,
                        },
                    },
                ),
            );
        }
    }

    fn retain_trailing(&mut self, request: &Request, ids: &[String]) {
        let p = &request.call.payload;
        let trailing = p
            .callback_ratio
            .as_ref()
            .or(p.new_callback_ratio.as_ref())
            .and_then(|v| v.parse().ok())
            .map(|v| ("ratio".to_owned(), v))
            .or_else(|| {
                p.trailing_stop_params
                    .as_ref()
                    .and_then(|rows| rows.first())
                    .and_then(|v| {
                        v.trail_variance
                            .parse()
                            .ok()
                            .map(|variance| (v.trail_type.to_owned(), variance))
                    })
            });
        if let Some(trailing) = trailing {
            for id in ids {
                if self.order_revision(id) <= request.stream_revision {
                    self.account.trailing.insert(id.clone(), trailing.clone());
                }
            }
        }
    }

    /// A confirmed setting write, through one Store closure per table.
    fn apply_setting(&mut self, cx: &mut Cx<'_, Pending>, effect: &Effect) {
        self.account.config_at = Some(cx.now);
        match effect {
            Effect::Mode(hedged) => cx.store.set_hedged(*hedged),
            Effect::Leverage { symbol, .. } | Effect::Margin { symbol, .. } => {
                cx.store.update_setting(symbol, |s| match effect {
                    Effect::Leverage { leverage, side, .. } => {
                        let leverage = *leverage;
                        if *side != Some(PositionSide::Short) {
                            s.leverage = leverage;
                            s.buy_leverage = Some(leverage);
                        }
                        if *side != Some(PositionSide::Long) {
                            s.sell_leverage = Some(leverage);
                        }
                    }
                    Effect::Margin { margin, .. } => s.margin_type = Some(*margin),
                    _ => {}
                })
            }
            _ => {}
        }
        cx.store.update_positions(|p| match effect {
            Effect::Leverage {
                symbol,
                leverage,
                side,
            } if &p.symbol == symbol && side.is_none_or(|side| p.side == side) => {
                p.leverage = *leverage
            }
            Effect::Margin { symbol, margin } if &p.symbol == symbol => {
                p.margin_type = Some(*margin)
            }
            Effect::Mode(hedged) => {
                p.position_mode = Some(if *hedged {
                    PositionMode::Hedged
                } else {
                    PositionMode::Oneway
                })
            }
            _ => {}
        });
    }

    fn apply_symbol_config(&self, cx: &mut Cx<'_, Pending>, symbol: &str, row: &SymbolConfig) {
        cx.store.set_hedged(is_hedged(&row.pos_mode));
        let kind = margin(&row.margin_mode);
        let leverage = if kind == MarginType::Cross {
            row.crossed_margin_leverage.or(1.0)
        } else {
            first(&[row.isolated_long_lever, row.isolated_short_lever]).unwrap_or(1.0)
        };
        cx.store.update_setting(symbol, |s| {
            s.leverage = leverage;
            s.margin_type = Some(kind);
            if kind == MarginType::Isolated && is_hedged(&row.pos_mode) {
                if let Some(long) = row.isolated_long_lever.0 {
                    s.buy_leverage = Some(long);
                }
                if let Some(short) = row.isolated_short_lever.0 {
                    s.sell_leverage = Some(short);
                }
                // Legacy scalar settings represent the long rail.
                s.leverage = s.buy_leverage.unwrap_or(leverage);
            } else {
                s.buy_leverage = Some(leverage);
                s.sell_leverage = Some(leverage);
            }
        });
    }

    /// A call that failed before or after release. A released read may retry
    /// natively (250·n ms, three times) on the Plan's own timer.
    #[inline(never)]
    pub(crate) fn trade_call_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
        released: bool,
    ) {
        let Some(mut job) = self.trading.active.take() else {
            return;
        };
        if job.id != request.command || job.plan.is_none() {
            self.trading.active = Some(job);
            return;
        }
        let plan = job.plan.clone().expect("live Plan");
        if request.call.read() {
            if released && request.read_retries < 3 {
                let delay = 250 * (u32::from(request.read_retries) + 1);
                if cx
                    .continue_serial_plan_after(
                        &plan,
                        &request.slot,
                        &request.context,
                        delay,
                        serial(Operation::Read),
                    )
                    .is_ok()
                {
                    job.retry = Some(Request {
                        read_retries: request.read_retries + 1,
                        ..request
                    });
                    self.trading.active = Some(job);
                    return;
                }
            }
            let resolve = request.call.is_resolve();
            let error = if resolve {
                error.outcome(Outcome::Unknown)
            } else if self.account.uta {
                not_sent("Bitget UTA metadata could not be resolved")
            } else {
                not_sent("Bitget position mode or margin type could not be resolved")
            };
            self.end(cx, job, None, error);
            if resolve {
                self.refresh_orders(cx, false);
            }
            return;
        }
        restore_canceling(cx, &request.call);
        if released || request.signature_retry {
            // A released write, or a resend after the first attempt reached the venue.
            error.outcome = Some(Outcome::Unknown);
        }
        if released && request.call.is_place() {
            self.account.orders_version = self.account.orders_version.wrapping_add(1);
        }
        let _ = cx.finish_plan_failed_call(&plan, &request.slot, &request.context);
        self.end(cx, job, None, error);
        if released {
            self.refresh_orders(cx, false);
        }
    }

    #[inline(never)]
    fn retry_read(&mut self, cx: &mut Cx<'_, Pending>, mut job: Job, request: Request) {
        let plan = job.plan.clone().expect("live Plan");
        let delay = 250 * (u32::from(request.read_retries) + 1);
        match cx.continue_serial_plan_after(
            &plan,
            &request.slot,
            &request.context,
            delay,
            serial(Operation::Read),
        ) {
            Ok(()) => {
                job.retry = Some(Request {
                    read_retries: request.read_retries + 1,
                    ..request
                });
                self.trading.active = Some(job);
            }
            Err(reason) => {
                let current = Some((&request.slot, &request.context));
                let error = VenueError::new(reason.venue_error().message).outcome(Outcome::Unknown);
                self.end(cx, job, current, error);
                self.refresh_orders(cx, false);
            }
        }
    }

    /// Issues a read retry on the slot the Plan's timer handed back.
    #[inline(never)]
    fn reissue(&mut self, cx: &mut Cx<'_, Pending>, job: Job, slot: Slot, retry: Request) {
        let plan = job.plan.clone().expect("live Plan");
        match cx.plan_leg_context(&plan, &slot) {
            Ok(context) => {
                let request = Request {
                    slot,
                    context,
                    ..retry
                };
                self.send_call(cx, job, request);
            }
            Err(reason) => {
                let error = VenueError::new(reason.venue_error().message);
                self.end(cx, job, None, error);
            }
        }
    }

    fn commit_amend(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        old: &str,
        next: &str,
        values: (f64, f64, Option<Order>, u64),
        client: &str,
    ) {
        let (price, amount, fallback, stream_revision) = values;
        let original = cx.store.order(old).cloned().or(fallback);
        let newer_stream = self.order_revision(old) > stream_revision;
        if old != next {
            self.mark_canceled(cx, old);
        }
        if self.order_terminal(next)
            || (old == next
                && original.as_ref().is_some_and(|o| {
                    matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                }))
        {
            return;
        }
        if let Some(mut order) = original {
            if old != next && cx.store.order(next).is_some() {
                return;
            }
            // Old-ID events describe the canceled order, not its replacement.
            if old != next || !newer_stream {
                order.price = price;
                order.amount = amount;
                amend_remaining(&mut order, self.inverse());
            }
            order.id = next.to_owned();
            order.client_order_id = Some(client.to_owned());
            order.status = OrderStatus::Open;
            self.mark_opened(cx.now, next);
            cx.store.upsert_order(order);
        }
    }
}

fn command_id(command: &Command) -> Option<u32> {
    match command {
        Command::PlaceOrders { command, .. }
        | Command::CancelOrders { command, .. }
        | Command::CancelSymbolOrders { command, .. }
        | Command::AmendOrder { command, .. }
        | Command::SetLeverage { command, .. }
        | Command::SetMarginType { command, .. }
        | Command::SetPositionMode { command, .. } => Some(*command),
        _ => None,
    }
}
fn symbols(cx: &Cx<'_, Pending>, command: &Command) -> Vec<String> {
    let mut result = match command {
        Command::PlaceOrders { orders, .. } => orders.iter().map(|o| o.symbol.clone()).collect(),
        Command::AmendOrder { id, .. } => cx
            .store
            .order(id)
            .map(|o| vec![o.symbol.clone()])
            .unwrap_or_default(),
        Command::SetLeverage { symbol, .. } | Command::SetMarginType { symbol, .. } => {
            vec![symbol.clone()]
        }
        _ => vec![],
    };
    let mut seen = HashSet::new();
    result.retain(|s| seen.insert(s.clone()));
    result
}
fn needs_position_read(cx: &Cx<'_, Pending>, command: &Command, uta: bool) -> bool {
    if let Command::PlaceOrders { orders, .. } = command {
        orders.iter().any(|o| {
            is_algo(o.kind)
                && o.kind != OrderType::TrailingStopLoss
                && (o.amount > 0.0 || uta)
                && !cx.store.positions().iter().any(|p| {
                    p.symbol == o.symbol
                        && p.side == protected_position(o.side)
                        && p.contracts > 0.0
                })
        })
    } else if uta && let Command::AmendOrder { id, .. } = command {
        cx.store.order(id).is_some_and(|o| {
            is_algo(o.kind)
                && o.kind != OrderType::TrailingStopLoss
                && !cx.store.positions().iter().any(|p| {
                    p.symbol == o.symbol
                        && p.side == protected_position(o.side)
                        && p.contracts > 0.0
                })
        })
    } else {
        false
    }
}
fn mint(cx: &Cx<'_, Pending>) -> Result<String, String> {
    safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
}
fn side_text(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "buy"
    } else {
        "sell"
    }
}
fn opening_side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "long"
    } else {
        "short"
    }
}
fn protected_side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "short"
    } else {
        "long"
    }
}
fn protected_position(side: OrderSide) -> PositionSide {
    if side == OrderSide::Buy {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
fn margin_text(margin: MarginType) -> &'static str {
    if margin == MarginType::Cross {
        "crossed"
    } else {
        "isolated"
    }
}
fn tif(value: Option<TimeInForce>) -> &'static str {
    match value.unwrap_or(TimeInForce::GoodTillCancel) {
        TimeInForce::GoodTillCancel => "gtc",
        TimeInForce::ImmediateOrCancel => "ioc",
        TimeInForce::FillOrKill => "fok",
        TimeInForce::PostOnly => "post_only",
    }
}
fn trigger(value: Option<StopTriggerType>) -> &'static str {
    if value == Some(StopTriggerType::LastPrice) {
        "fill_price"
    } else {
        "mark_price"
    }
}
fn uta_trigger(value: Option<StopTriggerType>) -> &'static str {
    if value == Some(StopTriggerType::LastPrice) {
        "market"
    } else {
        "mark"
    }
}
fn amend_remaining(order: &mut Order, inverse: bool) {
    let native = clean((order.amount - order.filled).max(0.0));
    order.remaining = if inverse && order.price > 0.0 {
        (native / order.price * 100_000_000.0).round() / 100_000_000.0
    } else {
        native
    };
    if inverse {
        order
            .extra
            .get_or_insert_with(OrderExtra::default)
            .remaining_native = Some(native);
    }
}
fn number(v: f64, step: f64) -> String {
    num::format(num::adjust(v, step))
}
fn floor_lot(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        clean(((v / step) + 1e-10).floor() * step)
    } else {
        v
    }
}
fn native_id(order: &Order) -> String {
    order.parent_id.clone().unwrap_or_else(|| order.id.clone())
}
fn related(orders: &[Order], order: &Order) -> Vec<Order> {
    let id = native_id(order);
    orders
        .iter()
        .filter(|o| native_id(o) == id)
        .cloned()
        .collect()
}
fn fallback_plan(order: &Order) -> &'static str {
    match (order.kind, order.amount > 0.0) {
        (OrderType::TakeProfit, true) => "profit_plan",
        (OrderType::TakeProfit, false) => "pos_profit",
        (OrderType::TrailingStopLoss, _) => "track_plan",
        (_, true) => "loss_plan",
        _ => "pos_loss",
    }
}
fn place_opts(order: &Order) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: order.symbol.clone(),
        kind: order.kind,
        side: order.side,
        amount: order.amount,
        price: Some(order.price),
        trigger_price: None,
        stop_loss: order.stop_loss,
        take_profit: order.take_profit,
        reduce_only: Some(order.reduce_only),
        time_in_force: order.time_in_force,
        trigger_type: order.trigger_type,
        tp_trigger_type: order.trigger_type,
        sl_trigger_type: order.trigger_type,
        client_order_id: None,
        close: order.close,
        stp_mode: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}

impl Planned {
    fn identities(&self) -> Vec<OrderIdentityReceipt> {
        let mut receipts = Vec::new();
        match self.effect {
            Effect::Place { .. } => self.payload.placement_identities(None, &mut receipts),
            Effect::Amend { .. } => {
                if let (Some(symbol), Some(identity)) =
                    (&self.payload.symbol, &self.payload.new_client_oid)
                {
                    receipts.push(OrderIdentityReceipt {
                        symbol: symbol.clone(),
                        kind: OrderIdentityKind::ClientOrderId,
                        identity: identity.clone(),
                    });
                }
            }
            _ => {}
        }
        receipts
    }
}
impl Payload {
    fn placement_identities(
        &self,
        outer_symbol: Option<&str>,
        receipts: &mut Vec<OrderIdentityReceipt>,
    ) {
        let symbol = self.symbol.as_deref().or(outer_symbol);
        if let (Some(symbol), Some(identity)) = (symbol, &self.client_oid) {
            receipts.push(OrderIdentityReceipt {
                symbol: symbol.into(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: identity.clone(),
            });
        }
        if let Some(orders) = &self.order_list {
            for order in orders {
                order.placement_identities(symbol, receipts);
            }
        }
    }
}

fn leverage_limit(value: f64, fallback: f64) -> f64 {
    if value == 0.0 { fallback } else { value }
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

fn serial(operation: Operation) -> SerialCall {
    SerialCall {
        auth: Default::default(),
        operation,
        timeout_ms: CALL_MS,
        // Mutations carry the one native 40009 resend; reads never resend.
        attempts: if matches!(operation, Operation::Mutation) {
            2
        } else {
            1
        },
        pacing: None,
    }
}

/// The engine's leg narrowing: an amend's replacement Place or Setting, and a
/// placement's Cancel or Setting. Every other leg keeps the command's intent.
fn narrowable(command: &Command, intent: Intent) -> bool {
    matches!(
        (command, intent),
        (Command::AmendOrder { .. }, Intent::Place | Intent::Setting)
            | (
                Command::PlaceOrders { .. },
                Intent::Cancel | Intent::Setting
            )
    )
}

/// The reply's native code as classifier evidence; Bitget codes are integers.
/// Native classifies 401, 403 and 429 by status, so those carry none.
fn native_code<'a>(reply: Option<&'a Reply<'a>>, status: u16) -> Option<Native<'a>> {
    if matches!(status, 401 | 403 | 429) {
        return None;
    }
    let reply = reply?;
    let code = reply.code.0.filter(|code| code.fract() == 0.0)?;
    Some(Native {
        code: NativeCode::Int(code as i64),
        message: &reply.msg,
    })
}

/// The failed rows' messages, as the error text has always read.
fn failure_text(failures: &[(Option<i64>, String)]) -> String {
    failures
        .iter()
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Each failed row's message and integer code, when Bitget sends one.
fn failure_messages(failures: &[Failure]) -> Vec<(Option<i64>, String)> {
    failures
        .iter()
        .map(|e| {
            let code = e.error_code.0.filter(|code| code.fract() == 0.0);
            (code.map(|code| code as i64), e.error_msg.clone())
        })
        .collect()
}

fn restore_canceling(cx: &mut Cx<'_, Pending>, call: &Planned) {
    if let Effect::Cancel { orders, .. } = &call.effect {
        for order in orders {
            cx.store.update_order(&order.id, |current| {
                if current.status == OrderStatus::Canceling {
                    current.status = order.status;
                }
            });
        }
    }
}

/// A failed protection rollback keeps the original refusal and returns the
/// native '; protection rollback failed: …' suffix as a projection note; a lost
/// or unproven rollback makes the result Unknown, with its kind.
fn with_rollback(original: Option<VenueError>, error: VenueError) -> (VenueError, Option<String>) {
    match original {
        None => (error, None),
        Some(original) => (
            VenueError {
                kind: if error.outcome == Some(Outcome::Unknown) {
                    error.kind
                } else {
                    original.kind
                },
                outcome: error.outcome.filter(|o| *o == Outcome::Unknown),
                ..original
            },
            Some(format!("protection rollback failed: {}", error.message)),
        ),
    }
}

/// Bounded correlation for a released write: its endpoint. Bitget has no late
/// reconcile codec; the order snapshot reconciles.
pub(crate) fn trade_reconciliation(
    request: &Request,
) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
    ReconciliationData::new(request.path.clone())
}

impl Job {
    /// The operation the job issues next once a condition wait releases.
    fn next_operation(&self) -> Operation {
        if !self.metadata.is_empty() || self.calls.front().is_some_and(Planned::read) {
            Operation::Read
        } else {
            Operation::Mutation
        }
    }
}

impl Planned {
    fn is_place(&self) -> bool {
        matches!(self.effect, Effect::Place { .. })
    }

    fn is_amend(&self) -> bool {
        matches!(self.effect, Effect::Amend { .. })
    }

    fn is_resolve(&self) -> bool {
        matches!(self.effect, Effect::ResolveAmend { .. })
    }

    fn read(&self) -> bool {
        matches!(
            self.effect,
            Effect::Metadata(_) | Effect::ResolveAmend { .. }
        )
    }

    fn operation(&self) -> Operation {
        if self.read() {
            Operation::Read
        } else {
            Operation::Mutation
        }
    }

    fn intent(&self) -> Intent {
        match self.effect {
            Effect::Metadata(_) | Effect::ResolveAmend { .. } => Intent::Read,
            Effect::Place { .. } => Intent::Place,
            Effect::Cancel { .. } => Intent::Cancel,
            Effect::Amend { .. } => Intent::Amend,
            Effect::Leverage { .. } | Effect::Margin { .. } | Effect::Mode(_) => Intent::Setting,
        }
    }
}

#[cfg(test)]
mod code_table_tests {
    use super::*;

    /// assert_table covers the status rows (never Satisfied or RefusedResend);
    /// Bitget pins 40009 as its one documented re-sign and every row unscoped.
    #[test]
    fn the_bitget_table_satisfies_the_engine_invariants() {
        safe_cex_engine::classify::assert_table(&CODES);
        for row in CODES.rows {
            assert_eq!(row.only, None, "{row:?}");
            if matches!(row.verdict, Verdict::RefusedResend(_)) {
                assert_eq!(row.key, Key::Int(40009), "{row:?}");
            }
            assert!(!matches!(row.verdict, Verdict::Satisfied(_)), "{row:?}");
        }
    }
}
