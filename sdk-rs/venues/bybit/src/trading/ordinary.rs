//! Ordinary (Linear, Inverse, Spot and Demo) trading on the engine's Preflight
//! and Plan core. Each command owns one genuine preflight: optional signed mode
//! reads, then one admitted native plan whose write facts the engine journal
//! seals. Options run here too; a Hyro placement founds or joins one planned
//! execution of its retained intent (`placements`).
use super::acknowledgement::{self as ack, Answer, Row};
use super::codes::{BYBIT, MARGIN_MODE_ALREADY_SET, PROTECTIVE};
use super::partial;
use super::{
    Call, Job, LocalWrite, Planned, check_client_order_ids, describe, is_virtual, mark_canceled,
    not_sent, trade_command_id, validate_stop_triggers,
};
use crate::sign::{self, Query};
use crate::{BybitLinear, Category, Pending, Rest, Then};
use safe_cex_engine::classify::{
    self, BatchItem, CodeTable, Intent, Native, NativeCode, ReadRetry, Released, Verdict, WriteFact,
};
use safe_cex_engine::model::{MarginType, OrderStatus, OrderType};
use safe_cex_engine::request_context::plan::{
    CallBound, CallTerms, RetryWindow, SerialCall, SerialPolicy, Slot, StepBound,
};
use safe_cex_engine::request_context::{
    Context, Credential, Error as ContextError, HttpOwnerRetirement, Operation, Plan, PlanPolicy,
    Policy, Preflight, PreflightPolicy, PreflightPurpose, ReconciliationData, ResendGrant,
    RetryEvidence, RetryPolicy, WriteResult,
};
use safe_cex_engine::setting_holds::SettingKey;
use safe_cex_engine::text::JoinWith;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, TimerId, VenueError,
};
use std::collections::VecDeque;

mod cancellations;
mod placements;
use cancellations::Cancellations;

/// Every native call (and its one fresh-signature resend) has a 20 s window.
const CALL_TIMEOUT_MS: u32 = 20_000;
const CALL_ATTEMPTS: u16 = 2;

/// The continuation a managed ordinary request carries.
pub struct Request {
    command: CommandId,
    context: Context,
}

/// One ordinary trade command between its genuine entry and its seal.
pub(crate) struct Ordinary {
    preflight: Preflight,
    read_credential: Option<Credential>,
    write_credential: Option<Credential>,
    command: Command,
    stage: Stage,
    hyro: Option<HyroCall>,
}

/// A Hyro placement call on its retained intent.
struct HyroCall {
    key: String,
    /// Lots this call's own reads proved absent on both axes.
    proven: Vec<bool>,
    /// The genuine caller receipt bounds lookup work, not the shared execution.
    lookup_deadline: f64,
    execution: Option<safe_cex_engine::engine::ExecutionHandle>,
    /// Every observer root of the execution: the founder's, then joiners'.
    observers: Vec<Context>,
}

enum Stage {
    Idle,
    Mode {
        symbol: String,
        version: u64,
        context: Context,
    },
    Plan(Box<Run>),
    /// A Hyro link-filtered read before admission.
    HyroRead {
        lot: usize,
        axis: crate::hyro::LookupAxis,
        context: Context,
        boundary: (u32, u32),
    },
}

struct Run {
    plan: Plan,
    cancellations: Option<Cancellations>,
    partial: Option<Box<partial::Flow>>,
    /// Logical calls in slot order; a selected fallback goes to the front.
    queue: VecDeque<Leg>,
    active: Option<Active>,
    continue_on_error: bool,
    stop_after_spot_order: bool,
    deferred_error: Option<String>,
    ids: Vec<String>,
    error: Option<VenueError>,
    /// Live rows a cancel-all did not list. The stream can lag a venue-side
    /// removal, so only an order read issued after its answer settles them.
    unlisted: Vec<String>,
    /// That read's category, once issued.
    unlisted_read: Option<Category>,
    /// That read has landed: whether it was complete.
    unlisted_landed: Option<bool>,
}

struct Leg {
    planned: Planned,
    intent: LegIntent,
    /// The Hyro intent lot this leg sends.
    lot: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LegIntent {
    Command,
    /// A position TP/SL setting (D1): attached TP/SL amends, placement and
    /// partial-stop trading-stop legs.
    Setting,
    /// The old order of a position-bound cancel-then-recreate (D3).
    ProtectionCancel,
}

struct Active {
    slot: Slot,
    context: Context,
    leg: Leg,
    local: Option<LocalWrite>,
}

/// What one delivered plan call means for the rest of the plan.
enum Decision {
    Next(Vec<String>),
    ProofPoll(partial::Axis),
    /// Accepted, but these captured rows wait for the plan's order read.
    Unlisted(Vec<String>),
    Failed(VenueError),
    Resend(ResendGrant),
    Replace(Vec<(String, Vec<safe_cex_engine::model::OrderIdentityReceipt>)>),
    /// A Partial stop's replacement was refused: put the old stop back, once.
    Restore(Call),
    /// The engine refused the fact; the plan cannot continue.
    Lost,
}

enum Gate {
    Done,
    Fail(VenueError),
}

fn spot_unknown(mut error: VenueError) -> VenueError {
    error.message = format!(
        "The outcome is unknown. Check Open Orders. {}",
        error.message
    );
    error.outcome = Some(Outcome::Unknown);
    error.presents = true;
    error
}

fn internal(message: &str) -> VenueError {
    VenueError::new(message).kind(ErrorKind::Internal)
}

fn call_bound<'a>(after_refusal: &'a [StepBound<'a>]) -> CallBound<'a> {
    CallBound {
        timeout_ms: CALL_TIMEOUT_MS,
        attempts: CALL_ATTEMPTS,
        retry_window: RetryWindow::FreshAttempt,
        after_refusal,
    }
}

fn partial_call(operation: Operation, attempts: u8) -> SerialCall {
    SerialCall {
        auth: Default::default(),
        operation,
        timeout_ms: CALL_TIMEOUT_MS,
        attempts,
        pacing: None,
    }
}

pub(crate) const HYRO_RECONNECTING: &str =
    "HyroTrader account is reconnecting; wait for private state reconciliation";
const HYRO_RECOVERING: &str = "HyroTrader private state is recovering";

/// Bybit names its own clock in a recv-window refusal; it corrects ours.
pub(crate) fn server_time(message: &str) -> Option<f64> {
    message
        .find("server_timestamp[")
        .map(|at| &message[at + 17..])
        .and_then(|rest| rest.split(']').next())
        .and_then(|digits| digits.parse::<f64>().ok())
}

const MODE_MISMATCH: &str = "position idx not match position mode";

#[derive(serde::Deserialize)]
struct Fields {
    #[serde(rename = "orderLinkId", default)]
    order_link_id: Option<String>,
    #[serde(rename = "orderId", default)]
    order_id: Option<String>,
    #[serde(rename = "request", default)]
    items: Vec<Fields>,
}

/// Our own request body's identity fields.
fn fields(body: &str) -> Fields {
    serde_json::from_str(body).unwrap_or(Fields {
        order_link_id: None,
        order_id: None,
        items: Vec::new(),
    })
}

/// The amend's every field, compared with the stored open order it names: the trigger
/// against the stored trigger and any quantity against the stored size, as sent.
fn trigger_amend_matches_store(store: &safe_cex_engine::Store, body: &str) -> bool {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct TriggerAmend<'a> {
        #[serde(rename = "category")]
        _category: &'a str,
        symbol: &'a str,
        order_id: &'a str,
        trigger_price: &'a str,
        qty: Option<&'a str>,
    }
    let Ok(request) = serde_json::from_str::<TriggerAmend>(body) else {
        return false;
    };
    // Only a trigger-bearing row stores its trigger as its price; a limit's is where it fills.
    store.order(request.order_id).is_some_and(|order| {
        order.status == OrderStatus::Open
            && matches!(
                order.kind,
                OrderType::StopLoss | OrderType::TakeProfit | OrderType::Conditional
            )
            && order.price.is_finite()
            && order.price > 0.0
            && request.symbol == order.symbol
            && request.trigger_price == super::number(order.price)
            && request
                .qty
                .is_none_or(|qty| qty == super::number(order.amount))
    })
}

const TRAILING_PROFIT_UNEDITABLE: &str =
    "A Bybit trailing take-profit cannot be edited here; cancel and recreate it on Bybit";

impl BybitLinear {
    pub(crate) fn ordinary_command(&self, command: &Command) -> bool {
        trade_command_id(command).is_some()
    }

    /// The genuine command entry: one preflight before any check or read.
    pub(crate) fn start_ordinary(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if self.variant.is_hyro() && matches!(command, Command::PlaceOrders { .. }) {
            self.start_hyro(cx, command);
            return;
        }
        let Some(id) = trade_command_id(&command) else {
            return;
        };
        let (policy, read_credential, write_credential) = self.ordinary_preflight();
        // A refused issuance already sealed the command.
        let Ok(preflight) = cx.begin_preflight(policy) else {
            return;
        };
        match self.ordinary_gate(cx, &command) {
            Some(Gate::Done) => {
                let _ = cx.done_empty_plan(&preflight, "[]".into());
                return;
            }
            Some(Gate::Fail(error)) => {
                let _ = cx.fail_preflight(&preflight, error);
                return;
            }
            None => {}
        }
        self.trading.ordinary.insert(
            id,
            Ordinary {
                preflight,
                read_credential,
                write_credential,
                command,
                stage: Stage::Idle,
                hyro: None,
            },
        );
        self.resume_ordinary(cx, id);
    }

    pub(crate) fn ordinary_preflight(
        &self,
    ) -> (PreflightPolicy, Option<Credential>, Option<Credential>) {
        let credential = |key| {
            self.select_key(key)
                .map(|(selection, identity)| selection.credential(identity.credential.clone()))
        };
        let write_credential = credential(crate::KeyUse::Write);
        let read_credential = credential(crate::KeyUse::AccountRead);
        let purpose = if write_credential.is_some() {
            PreflightPurpose::WriteAfterReads
        } else {
            PreflightPurpose::ReadOnly
        };
        let policy = PreflightPolicy {
            purpose,
            read_credential: read_credential.clone(),
            write_credential: write_credential.clone(),
        };
        (policy, read_credential, write_credential)
    }

    /// Native validation before any read or write, in the legacy order.
    // A product the account has not enabled is its configuration (as for reads).
    fn ordinary_gate(&mut self, cx: &mut Cx<'_, Pending>, command: &Command) -> Option<Gate> {
        if let Some(error) = self.config_error.clone() {
            return Some(Gate::Fail(not_sent(error)));
        }
        // A Hyro placement checks this before its first write instead: a retry
        // that only returns already-recovered identities sends nothing.
        if !self.private_ready() && !matches!(command, Command::PlaceOrders { .. }) {
            return Some(Gate::Fail(not_sent(HYRO_RECONNECTING)));
        }
        // Bybit has no edit for a trailing take-profit; a trading-stop
        // amend would replace it with an ordinary full take-profit.
        if let Command::AmendOrder { id, .. } = command
            && self.account.trailing_profit.contains(id)
        {
            return Some(Gate::Fail(not_sent(TRAILING_PROFIT_UNEDITABLE)));
        }
        let symbol = match command {
            Command::CancelSymbolOrders { symbol, .. }
            | Command::SetLeverage { symbol, .. }
            | Command::SetMarginType { symbol, .. } => Some(symbol.clone()),
            Command::AmendOrder { id, .. } => cx.store.order(id).map(|o| o.symbol.clone()),
            _ => None,
        };
        if let Some(symbol) = symbol
            && !self.category_enabled(self.category_for(&symbol))
        {
            return Some(Gate::Fail(disabled_product()));
        }
        if let Command::PlaceOrders { orders, .. } = command {
            for order in orders {
                if !self.category_enabled(self.category_for(&order.symbol)) {
                    return Some(Gate::Fail(disabled_product()));
                }
                let option = self.category_for(&order.symbol) == Category::Option;
                if !option && order.implied_volatility.is_some() {
                    return Some(Gate::Fail(not_sent(
                        "impliedVolatility is supported only on Bybit options",
                    )));
                }
                // The whole batch validates before any read: an invalid option
                // must not cost a mode read for its linear neighbours.
                if option && let Err(error) = self.format_option(cx, order) {
                    return Some(Gate::Fail(not_sent(error)));
                }
            }
            // An unpriced protective order must never become a market order.
            if let Err(error) = validate_stop_triggers(cx, orders) {
                return Some(Gate::Fail(not_sent(error)));
            }
        }
        if let Some((_, result)) = self.variant_control(command) {
            return Some(match result {
                Ok(()) => Gate::Done,
                Err(error) => Gate::Fail(not_sent(error).kind(ErrorKind::Unsupported)),
            });
        }
        if self.credentials.is_none() {
            return Some(Gate::Fail(
                not_sent("Bybit trading needs an API key").kind(ErrorKind::AuthInvalid),
            ));
        }
        if let Command::PlaceOrders { command, orders } = command
            && let Err(message) = check_client_order_ids(cx, orders)
        {
            cx.command_error(*command, message.clone());
            return Some(Gate::Fail(not_sent(message)));
        }
        None
    }

    /// A settle-coin snapshot omits flat symbols; their mode is read first.
    fn unresolved_mode(&self, cx: &Cx<'_, Pending>, command: &Command) -> Option<String> {
        // Spot has no position mode; Hyro is one-way by venue contract.
        if self.variant.is_spot() || self.variant.is_hyro() {
            return None;
        }
        match command {
            Command::PlaceOrders { orders, .. } => orders
                .iter()
                .find(|o| self.symbol_mode(&o.symbol).is_none())
                .map(|o| o.symbol.clone()),
            Command::AmendOrder { id, .. } => cx
                .store
                .order(id)
                .filter(|o| self.symbol_mode(&o.symbol).is_none())
                .map(|o| o.symbol.clone()),
            _ => None,
        }
    }

    /// Read the next unresolved mode, or admit the final native plan.
    fn resume_ordinary(&mut self, cx: &mut Cx<'_, Pending>, id: CommandId) {
        let Some(mut entry) = self.trading.ordinary.remove(&id) else {
            return;
        };
        if let Some(symbol) = self.unresolved_mode(cx, &entry.command) {
            self.select_category(self.category_for(&symbol));
            let read = entry.read_credential.clone();
            let context = cx.preflight_read_context(
                &entry.preflight,
                Policy {
                    operation: Operation::Read,
                    credential: read,
                    timeout_ms: CALL_TIMEOUT_MS,
                    pacing: None,
                    retry: RetryPolicy::DefinitiveRefusal,
                },
                CallTerms {
                    wait_ms: 0,
                    timeout_ms: CALL_TIMEOUT_MS,
                    attempts: CALL_ATTEMPTS,
                    retry_window: RetryWindow::FreshAttempt,
                },
            );
            let context = context
                .ok()
                .filter(|context| self.send_mode_read(cx, id, &symbol, context.clone()));
            let Some(context) = context else {
                let error =
                    internal("Bybit position-mode read was refused").outcome(Outcome::NotSent);
                let _ = cx.fail_preflight(&entry.preflight, error);
                return;
            };
            entry.stage = Stage::Mode {
                symbol,
                version: self.trading.mode_version,
                context,
            };
            self.trading.ordinary.insert(id, entry);
            return;
        }
        let job = match self.plan_job(cx, &entry.command) {
            Some(Ok(job)) => job,
            Some(Err(message)) => {
                cx.command_error(id, message.clone());
                let _ = cx.fail_preflight(&entry.preflight, not_sent(message));
                return;
            }
            None => {
                let _ = cx.fail_preflight(&entry.preflight, not_sent("Unsupported Bybit command"));
                return;
            }
        };
        if job.steps.is_empty() {
            let _ = cx.done_empty_plan(&entry.preflight, "[]".into());
            return;
        }
        let Some(run) = self.admit_ordinary(cx, &entry, job) else {
            let error =
                internal("Bybit trading plan admission was refused").outcome(Outcome::NotSent);
            let _ = cx.fail_preflight(&entry.preflight, error);
            return;
        };
        entry.stage = Stage::Plan(Box::new(run));
        self.continue_ordinary(cx, id, entry);
    }

    /// One serial step per native request; a batch's single-create fallback is
    /// its declared after-refusal branch.
    fn admit_ordinary(&self, cx: &mut Cx<'_, Pending>, entry: &Ordinary, job: Job) -> Option<Run> {
        let amend_virtual =
            matches!(&entry.command, Command::AmendOrder { id, .. } if is_virtual(id));
        let mut legs = VecDeque::new();
        let mut delays = Vec::new();
        for mut step in job.steps {
            if step.requests.len() != 1 {
                return None;
            }
            delays.push(step.delay_ms.max(0.0).min(f64::from(u32::MAX)) as u32);
            let planned = step.requests.pop()?;
            let intent = match planned.call {
                // Every trading-stop leg is a position TP/SL setting (D1).
                Call::TradingStop => LegIntent::Setting,
                Call::Amend if amend_virtual => LegIntent::Setting,
                Call::PartialStop { .. } | Call::RestorePartialStop { .. } => LegIntent::Setting,
                Call::CancelPartialStop { .. } | Call::ClearAttachedLeg { .. } => {
                    LegIntent::ProtectionCancel
                }
                _ => LegIntent::Command,
            };
            legs.push_back(Leg {
                planned,
                intent,
                lot: None,
            });
        }
        let fallbacks: Vec<Vec<CallBound<'_>>> = legs
            .iter()
            .map(|leg| match &leg.planned.call {
                Call::PlaceBatch { fallback } => fallback.iter().map(|_| call_bound(&[])).collect(),
                _ => Vec::new(),
            })
            .collect();
        let branches: Vec<Vec<StepBound<'_>>> = fallbacks
            .iter()
            .map(|calls| {
                calls
                    .chunks(1)
                    .map(|calls| StepBound { wait_ms: 0, calls })
                    .collect()
            })
            .collect();
        let calls: Vec<[CallBound<'_>; 1]> =
            branches.iter().map(|branch| [call_bound(branch)]).collect();
        let steps: Vec<StepBound<'_>> = calls
            .iter()
            .zip(&delays)
            .map(|(calls, wait_ms)| StepBound {
                wait_ms: *wait_ms,
                calls,
            })
            .collect();
        let partial = legs.front().and_then(|leg| match &leg.planned.call {
            Call::CancelPartialStop { guard, .. } => {
                Some(Box::new(partial::Flow::new((**guard).clone())))
            }
            _ => None,
        });
        let plan = if partial.is_some() {
            cx.admit_serial_plan(
                &entry.preflight,
                partial_call(Operation::Mutation, 2),
                SerialPolicy {
                    credential: entry.write_credential.clone(),
                    pacing: None,
                    refusals_complete: false,
                },
            )
            .map(|(plan, _)| plan)
        } else {
            cx.admit_plan(
                &entry.preflight,
                &steps,
                PlanPolicy {
                    // An unproven reply seals the batch (the pre-#2012 semantics this adopts).
                    independent_calls: false,
                    operation: Operation::Mutation,
                    credential: entry.write_credential.clone(),
                    pacing: None,
                    retry: RetryPolicy::DefinitiveRefusal,
                },
            )
        }
        .ok()?;
        let cancellations = Cancellations::new(
            &entry.command,
            &legs
                .iter()
                .flat_map(|leg| match &leg.planned.call {
                    Call::Cancel { ids } => ids.as_slice(),
                    _ => &[],
                })
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        Some(Run {
            plan,
            cancellations,
            partial,
            queue: legs,
            active: None,
            continue_on_error: job.continue_on_error,
            stop_after_spot_order: false,
            deferred_error: job.deferred_error,
            ids: Vec::new(),
            error: None,
            unlisted: Vec::new(),
            unlisted_read: None,
            unlisted_landed: None,
        })
    }

    fn send_mode_read(
        &self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        symbol: &str,
        context: Context,
    ) -> bool {
        let query = Query::new()
            .str("category", self.paths.category)
            .str("symbol", symbol)
            .into_string();
        self.send_ordinary(cx, id, context, Method::Get, "/v5/position/list", query)
    }

    /// The native recipe: today's signed request bytes, a fresh signature per
    /// attempt, and the engine's managed lifecycle.
    fn send_ordinary(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        context: Context,
        method: Method,
        path: &'static str,
        payload: String,
    ) -> bool {
        let Some(identity) = self.signing_identity(method) else {
            return false;
        };
        let mut url = format!("{}{}", self.paths.rest, path);
        if method == Method::Get && !payload.is_empty() {
            url.push('?');
            url.push_str(&payload);
        }
        let step = sign::Http::new(identity.clone(), payload.clone(), self.recv_window)
            .with_header_policy(self.signing_policy.header_policy());
        let body = (method != Method::Get).then(|| payload.clone());
        let pending = Pending {
            category: self.category,
            rest: Rest {
                path,
                payload,
                stream: false,
                signed: true,
                resent: false,
            },
            then: Then::Ordinary(Request {
                command,
                context: context.clone(),
            }),
            read: None,
        };
        cx.request_signed_managed(context, None, method, url, body, Box::new(step), pending)
            .is_ok()
    }

    /// Issue the next eligible slot, or seal a structurally complete plan.
    fn continue_ordinary(&mut self, cx: &mut Cx<'_, Pending>, id: CommandId, mut entry: Ordinary) {
        let Stage::Plan(run) = &mut entry.stage else {
            self.park(id, entry);
            return;
        };
        if run.active.is_some() {
            self.park(id, entry);
            return;
        }
        let slots = match cx.plan_slots(&run.plan) {
            Ok(slots) => slots,
            Err(error) => {
                let error = if error == ContextError::TimedOut {
                    VenueError::new("Bybit trading plan expired").kind(ErrorKind::Timeout)
                } else {
                    internal("Bybit trading plan lost its schedule")
                };
                let _ = cx.fail_plan(&run.plan, error);
                return;
            }
        };
        if slots.is_empty() {
            if run.queue.is_empty() {
                if !run.unlisted.is_empty() {
                    if run.unlisted_read.is_some() {
                        self.park(id, entry);
                        return;
                    }
                    let category = self.category;
                    run.unlisted_read = Some(category);
                    // Parked first: a refused read settles it synchronously.
                    self.park(id, entry);
                    self.reconcile_orders(cx);
                    return;
                }
                // A Hyro result names every lot of the intent, earlier calls' included.
                let ids = match &entry.hyro {
                    Some(call) => self
                        .trading
                        .hyro
                        .intents
                        .get(&call.key)
                        .map_or_else(|| "[]".into(), |intent| intent.ids()),
                    None => serde_json::to_string(&run.ids).unwrap_or_default(),
                };
                let failure = run
                    .error
                    .take()
                    .or_else(|| run.deferred_error.take().map(not_sent));
                let sealed = match failure {
                    Some(error) => cx.fail_plan(&run.plan, error).map(|()| false),
                    None => cx.done_plan(&run.plan, ids.clone()).map(|()| true),
                };
                match sealed {
                    Ok(true) => {
                        if let Some(call) = &entry.hyro {
                            self.settle_hyro(&call.key, Some(ids));
                            self.wake_hyro_waiters(cx);
                        }
                    }
                    Ok(false) => {}
                    Err(_) => {
                        let _ =
                            cx.fail_plan(&run.plan, internal("Bybit trading plan was incomplete"));
                    }
                }
                return;
            }
            // A paced step: `on_http_plan_ready` resumes it.
            self.park(id, entry);
            return;
        }
        let (Ok([slot]), Some(leg)) = (<[Slot; 1]>::try_from(slots), run.queue.pop_front()) else {
            let _ = cx.fail_plan(&run.plan, internal("Bybit trading plan is not serial"));
            return;
        };
        let context = match leg.intent {
            LegIntent::Setting => {
                cx.plan_leg_context_with_intent(&run.plan, &slot, Intent::Setting)
            }
            LegIntent::ProtectionCancel => cx.plan_protection_cancel_leg_context(&run.plan, &slot),
            LegIntent::Command => cx.plan_leg_context(&run.plan, &slot),
        };
        let Ok(context) = context else {
            let _ = cx.fail_plan(
                &run.plan,
                internal("Bybit trading plan refused its next leg"),
            );
            return;
        };
        // A later Hyro leg waits for reconciled private state (legacy text).
        if !self.private_ready() {
            let _ = cx.fail_plan(&run.plan, not_sent(HYRO_RECOVERING));
            return;
        }
        let method = if matches!(leg.planned.call, Call::PartialProof(_)) {
            Method::Get
        } else {
            Method::Post
        };
        if method == Method::Post {
            self.note_hyro_write();
            self.route_body(&leg.planned.body);
        }
        let local = self.local_order_lifecycle().then(|| LocalWrite {
            path: leg.planned.path,
            body: leg.planned.body.clone(),
            before: cx.store.orders().to_vec(),
            socket: self.private.socket,
            snapshot: self.account.orders_snapshot_version,
        });
        match (&entry.hyro, leg.lot) {
            (Some(call), Some(lot)) => self.hyro_leg_sent(cx, id, call, lot, &leg.planned.receipts),
            _ => cx.command_progress(id, leg.planned.receipts.clone()),
        }
        let (path, body, lot) = (leg.planned.path, leg.planned.body.clone(), leg.lot);
        let plan = run.plan.clone();
        if run.partial.is_some()
            && matches!(
                leg.planned.call,
                Call::CancelPartialStop { .. }
                    | Call::PartialProof(_)
                    | Call::PartialStop { .. }
                    | Call::RestorePartialStop { .. }
            )
        {
            let _ = cx.present_plan_failure(
                &plan,
                "The protective edit has an unknown outcome. Check Open Orders before retrying",
            );
        }
        let key = entry.hyro.as_ref().map(|call| call.key.clone());
        run.active = Some(Active {
            slot,
            context: context.clone(),
            leg,
            local,
        });
        self.park(id, entry);
        if !self.send_ordinary(cx, id, context, method, path, body) {
            // A typed pre-send refusal seals the plan in the engine; never hang.
            match &key {
                Some(key) => {
                    self.take_hyro_live(key);
                    self.hyro_leg_unsent(key, lot);
                }
                None => {
                    self.trading.ordinary.remove(&id);
                }
            }
            let _ = cx.fail_plan(&plan, internal("Bybit trading request was refused"));
        }
    }

    /// The request's category drives paths and product state.
    fn route_body(&mut self, body: &str) {
        #[derive(serde::Deserialize)]
        struct Routing<'a> {
            #[serde(borrow)]
            category: Option<&'a str>,
        }
        if let Ok(route) = serde_json::from_str::<Routing>(body)
            && let Some(category) = route.category.and_then(|value| {
                self.categories()
                    .into_iter()
                    .find(|category| category.as_str() == value)
            })
        {
            self.select_category(category);
        }
    }

    pub(crate) fn on_ordinary_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let id = self
            .trading
            .ordinary
            .iter()
            .find_map(|(id, entry)| match &entry.stage {
                Stage::Plan(run) if run.plan.owner() == plan.owner() => Some(*id),
                _ => None,
            });
        if let Some(id) = id
            && let Some(entry) = self.trading.ordinary.remove(&id)
        {
            self.continue_ordinary(cx, id, entry);
            return;
        }
        if let Some((key, id, entry)) = self.take_hyro_plan(plan) {
            self.continue_ordinary(cx, id, entry);
            self.after_hyro(cx, &key, id);
        }
    }

    /// Cleanup only: the engine sealed and retired this generation.
    pub(crate) fn on_ordinary_retired(&mut self, owner: &HttpOwnerRetirement) {
        let mut settled = Vec::new();
        self.trading.ordinary.retain(|id, entry| {
            let retired = owner.matches_preflight(&entry.preflight);
            if let (true, Some(call)) = (retired, &entry.hyro) {
                settled.push((call.key.clone(), *id));
            }
            !retired
        });
        self.retire_hyro(owner, settled);
    }

    pub(crate) fn ordinary_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<ReconciliationData, ContextError> {
        if !matches!(pending.then, Then::Ordinary(_)) {
            return Err(ContextError::Invalid);
        }
        // A Hyro create keeps its link so a late ACK can be correlated;
        // other unknown effects reconcile through fenced order reads.
        let link = (self.variant.is_hyro() && pending.rest.path == "/v5/order/create")
            .then(|| fields(&pending.rest.payload).order_link_id)
            .flatten();
        ReconciliationData::new(link.unwrap_or_default())
    }

    pub(crate) fn on_ordinary_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Some(mut entry) = self.take_ordinary(&request) else {
            return;
        };
        let hyro = entry.hyro.as_ref().map(|call| call.key.clone());
        match &mut entry.stage {
            Stage::Mode { context, .. } if *context == request.context => {
                self.on_mode_response(cx, request.command, entry, &response)
            }
            Stage::HyroRead { context, .. } if *context == request.context => {
                self.on_hyro_read(cx, request.command, entry, &response)
            }
            Stage::Plan(run)
                if run
                    .active
                    .as_ref()
                    .is_some_and(|active| active.context == request.context) =>
            {
                self.on_leg_response(cx, request.command, entry, &response)
            }
            _ => self.park(request.command, entry),
        }
        if let Some(key) = hyro {
            self.after_hyro(cx, &key, request.command);
        }
    }

    /// The entry a continuation belongs to: a command's, or a Hyro execution's.
    fn take_ordinary(&mut self, request: &Request) -> Option<Ordinary> {
        let owner = request.context.owner();
        if let safe_cex_engine::request_context::Owner::Execution { .. } = owner {
            return self.take_hyro_owner(owner);
        }
        self.trading.ordinary.remove(&request.command)
    }

    /// Keep an entry until its next callback; a Hyro plan parks on its intent.
    fn park(&mut self, id: CommandId, entry: Ordinary) {
        if let (Some(call), Stage::Plan(_)) = (&entry.hyro, &entry.stage) {
            let key = call.key.clone();
            self.park_hyro(&key, entry);
            return;
        }
        self.trading.ordinary.insert(id, entry);
    }

    fn on_mode_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        mut entry: Ordinary,
        response: &Response<'_>,
    ) {
        let Stage::Mode {
            symbol,
            version,
            context,
        } = std::mem::replace(&mut entry.stage, Stage::Idle)
        else {
            return;
        };
        if cx
            .validate_preflight_delivery(&entry.preflight, &context)
            .is_err()
        {
            entry.stage = Stage::Mode {
                symbol,
                version,
                context,
            };
            self.trading.ordinary.insert(id, entry);
            return;
        }
        let released = Released::Response(response);
        let mode = match ack::answer(response.body) {
            Ok(Answer::Success { result, .. }) => {
                ack::position_mode(result, &symbol).map_err(not_sent)
            }
            Ok(Answer::Refused { code, message, .. }) => {
                // Bybit's own clock is evidence; read it before sanitizing.
                let server = server_time(&message);
                // Signature diagnostics can contain the complete signed origin.
                let message = crate::signing_rejection(code, &message).unwrap_or(message);
                let native = Native {
                    code: NativeCode::Int(code),
                    message: &message,
                };
                let failure = classify::read(
                    &BYBIT,
                    released,
                    Some(&native),
                    describe((code, message.clone())),
                );
                if failure.retry == Some(ReadRetry::NativeResend)
                    && let Ok(context) = cx.preflight_retry_context(
                        &entry.preflight,
                        &context,
                        RetryEvidence::DefinitiveRefusal,
                    )
                {
                    if let Some(server) = server {
                        Self::observe_clock(cx, server);
                    }
                    if !self.send_mode_read(cx, id, &symbol, context.clone()) {
                        let _ = cx.fail_preflight(
                            &entry.preflight,
                            failure.error.outcome(Outcome::NotSent),
                        );
                        return;
                    }
                    entry.stage = Stage::Mode {
                        symbol,
                        version,
                        context,
                    };
                    self.trading.ordinary.insert(id, entry);
                    return;
                }
                Err(failure.error)
            }
            Err(_) => {
                Err(classify::read(&BYBIT, released, None, "Invalid position mode response").error)
            }
        };
        let mode = match mode {
            Ok(mode) => mode,
            Err(error) => {
                cx.command_error(id, error.message.clone());
                // A read before any write: nothing was sent.
                let _ = cx.fail_preflight(&entry.preflight, error.outcome(Outcome::NotSent));
                return;
            }
        };
        if cx
            .finish_preflight_call(&entry.preflight, &context)
            .is_err()
        {
            let error =
                internal("Bybit position-mode read lost its custody").outcome(Outcome::NotSent);
            let _ = cx.fail_preflight(&entry.preflight, error);
            return;
        }
        // A mode change while the read was out leaves this answer stale.
        if version == self.trading.mode_version {
            self.select_category(self.category_for(&symbol));
            self.account.position_modes.entry(symbol).or_insert(mode);
        }
        self.trading.ordinary.insert(id, entry);
        self.resume_ordinary(cx, id);
    }

    fn on_leg_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        mut entry: Ordinary,
        response: &Response<'_>,
    ) {
        let Stage::Plan(run) = &mut entry.stage else {
            return;
        };
        let active = run.active.take().expect("matched active leg");
        if cx
            .validate_plan_delivery(&run.plan, &active.context)
            .is_err()
        {
            run.active = Some(active);
            self.park(id, entry);
            return;
        }
        let decision = if let Call::PartialProof(axis) = active.leg.planned.call {
            match run
                .partial
                .as_mut()
                .expect("proof belongs to Partial amend")
                .observe(axis, response, cx.now)
            {
                Ok(partial::Observation::Retry(axis)) => Decision::ProofPoll(axis),
                Ok(partial::Observation::Complete) => {
                    if matches!(axis, partial::Axis::Target | partial::Axis::TargetHistory) {
                        let target = run.partial.as_ref().unwrap().target_id().to_owned();
                        self.note_partial_stop_canceled(&target);
                        mark_canceled(
                            cx,
                            std::slice::from_ref(&target),
                            self.local_order_lifecycle(),
                        );
                    }
                    if axis == partial::Axis::Live {
                        let flow = run.partial.as_mut().unwrap();
                        if let Some(history) = flow.history_proof() {
                            run.queue.front_mut().unwrap().planned = history;
                        } else {
                            flow.skip_history();
                            run.queue.pop_front();
                        }
                    }
                    if matches!(axis, partial::Axis::Live | partial::Axis::History)
                        && !run.partial.as_ref().unwrap().restorable()
                        && run.partial.as_ref().unwrap().history_done()
                    {
                        for leg in &mut run.queue {
                            if let Call::PartialStop { restore, .. } = &mut leg.planned.call {
                                *restore = None;
                            }
                        }
                    }
                    Decision::Next(Vec::new())
                }
                Err(failure) => {
                    // A stable prefix lets logs count failed proofs per check.
                    cx.notice(
                        safe_cex_engine::Level::Warn,
                        "partial.proof_failed",
                        format!("[proof-diag] {}", failure.reason),
                    );
                    let _ = cx.inherit_plan_uncertainty(&run.plan);
                    Decision::Failed(failure.error())
                }
            }
        } else {
            self.leg_decision(cx, id, &active, response, run.cancellations.as_mut())
        };
        // The logical replacement has settled (a resend is the same write, still
        // open), so the already-gone mark has served.
        if !matches!(decision, Decision::Resend(_))
            && let Call::PartialStop {
                restore: Some((replaced, _)),
                ..
            } = &active.leg.planned.call
        {
            self.trading.gone_before_cancel.remove(replaced);
        }
        if let (Some(call), Some(lot)) = (&entry.hyro, active.leg.lot) {
            self.hyro_leg_decided(&call.key, lot, &decision);
        }
        let Stage::Plan(run) = &mut entry.stage else {
            return;
        };
        let plan = run.plan.clone();
        match decision {
            Decision::ProofPoll(axis) => {
                let flow = run.partial.as_ref().unwrap();
                let planned = flow.proof(axis);
                let delay_ms = flow.poll_delay_ms(axis);
                run.queue.push_front(Leg {
                    planned,
                    intent: LegIntent::Command,
                    lot: None,
                });
                if cx
                    .continue_serial_plan_after(
                        &plan,
                        &active.slot,
                        &active.context,
                        delay_ms,
                        partial_call(Operation::Read, 1),
                    )
                    .is_err()
                {
                    let _ = cx.inherit_plan_uncertainty(&plan);
                    let _ = cx.fail_plan(
                        &plan,
                        VenueError::new(
                            "The protective cancel could not be confirmed; check Open Orders",
                        )
                        .outcome(Outcome::Unknown),
                    );
                    return;
                }
                self.park(id, entry);
                return;
            }
            Decision::Next(ids) => {
                // The old stop is back: the edit failed, but nothing is unprotected.
                if let Call::RestorePartialStop {
                    stop: (symbol, _, kind),
                    refusal,
                } = &active.leg.planned.call
                {
                    let what = if *kind == safe_cex_engine::model::OrderType::TakeProfit {
                        "take profit"
                    } else {
                        "stop"
                    };
                    let presented = VenueError {
                        kind: Some(safe_cex_engine::error::ErrorKind::Rejected),
                        outcome: Some(Outcome::Rejected),
                        protection: Some(safe_cex_engine::error::Protection::Restored),
                        ..VenueError::new(format!(
                            "Your {symbol} {what} is unchanged: its replacement was refused, so the old {what} was put back as a new order: {refusal}"
                        ))
                    };
                    let _ = cx.finish_plan_call(&plan, &active.slot, &active.context);
                    let _ = cx.fail_plan(&plan, presented);
                    return;
                }
                run.ids.extend(ids);
                let continued = if run.partial.is_some() && !run.queue.is_empty() {
                    let next = &run.queue.front().unwrap().planned.call;
                    let operation = if matches!(next, Call::PartialProof(_)) {
                        Operation::Read
                    } else {
                        Operation::Mutation
                    };
                    cx.continue_serial_plan(
                        &plan,
                        &active.slot,
                        &active.context,
                        partial_call(operation, 1),
                    )
                    .map(|_| ())
                } else {
                    cx.finish_plan_call(&plan, &active.slot, &active.context)
                };
                if continued.is_err() {
                    let _ = cx.fail_plan(&plan, internal("Bybit trading plan lost its call"));
                    return;
                }
                if matches!(active.leg.planned.call, Call::PlaceLot { last: true })
                    && run.stop_after_spot_order
                {
                    let error = run
                        .error
                        .take()
                        .unwrap_or_else(|| internal("Bybit split order lost its refusal"));
                    let _ = cx.fail_plan(&plan, error);
                    return;
                }
            }
            Decision::Unlisted(unlisted) => {
                run.unlisted.extend(unlisted);
                if cx
                    .finish_plan_call(&plan, &active.slot, &active.context)
                    .is_err()
                {
                    let _ = cx.fail_plan(&plan, internal("Bybit trading plan lost its call"));
                    return;
                }
            }
            Decision::Failed(mut error) => {
                if let Some(report) = &mut run.cancellations {
                    error = report.failure(cx, id, &active.leg.planned.call, error);
                }
                if matches!(active.leg.planned.call, Call::CancelPartialStop { .. })
                    && error.outcome != Some(Outcome::Unknown)
                {
                    error.presents = true;
                }
                if matches!(active.leg.planned.call, Call::PartialStop { .. })
                    && error.outcome != Some(Outcome::Unknown)
                {
                    error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
                    error.message = format!("{}; re-place protection on Bybit", error.message);
                }
                let stop_lost = match &active.leg.planned.call {
                    Call::PartialStop {
                        stop: (symbol, side, kind),
                        ..
                    } => Some(safe_cex_engine::error::stop_gone_text(
                        symbol,
                        *side,
                        *kind,
                        error.outcome == Some(Outcome::Unknown),
                    )),
                    // The old stop could not be put back either: say so as loudly as before.
                    Call::RestorePartialStop {
                        stop: (symbol, side, kind),
                        refusal,
                    } => {
                        let gone = safe_cex_engine::error::stop_gone_text(
                            symbol,
                            *side,
                            *kind,
                            error.outcome == Some(Outcome::Unknown),
                        );
                        // A put-back of unknown outcome may have landed: claim neither.
                        let unknown = error.outcome == Some(Outcome::Unknown);
                        let presented = VenueError {
                            protection: (!unknown)
                                .then_some(safe_cex_engine::error::Protection::Unprotected),
                            presents: true,
                            ..VenueError::new(if unknown {
                                format!(
                                    "{gone}: {refusal}; putting it back has an unknown outcome: {}",
                                    error.message
                                )
                            } else {
                                format!(
                                    "{gone}: {refusal}; putting it back failed too: {}",
                                    error.message
                                )
                            })
                        };
                        if entry.hyro.is_none() {
                            cx.command_error(id, error.message.clone());
                        }
                        let _ = cx.fail_plan(&plan, presented);
                        return;
                    }
                    _ => None,
                };
                // An execution's journal reports each failure to every observer.
                if entry.hyro.is_none() {
                    cx.command_error(id, error.message.clone());
                }
                let spot_lot = matches!(active.leg.planned.call, Call::PlaceLot { .. });
                if spot_lot && error.outcome == Some(Outcome::Unknown) {
                    error = spot_unknown(error);
                }
                if spot_lot && matches!(active.leg.planned.call, Call::PlaceLot { last: false }) {
                    run.stop_after_spot_order = true;
                }
                if !run.continue_on_error
                    || (spot_lot
                        && (error.outcome == Some(Outcome::Unknown)
                            || matches!(active.leg.planned.call, Call::PlaceLot { last: true })))
                {
                    let _ = match &stop_lost {
                        Some(note) => cx.fail_plan_noted(&plan, error, note),
                        None => cx.fail_plan(&plan, error),
                    };
                    return;
                }
                run.error.get_or_insert(error);
                if cx
                    .finish_plan_call(&plan, &active.slot, &active.context)
                    .is_err()
                {
                    let _ = cx.fail_plan(&plan, internal("Bybit trading plan lost its call"));
                    return;
                }
            }
            Decision::Resend(grant) => {
                let mut active = active;
                let resent = cx
                    .plan_resend_context(&plan, &active.slot, &active.context, grant)
                    .is_ok_and(|context| {
                        if run.partial.is_some() {
                            active.context = context.clone();
                        }
                        self.send_ordinary(
                            cx,
                            id,
                            context,
                            Method::Post,
                            active.leg.planned.path,
                            active.leg.planned.body.clone(),
                        )
                    });
                if !resent {
                    let _ = cx.fail_plan(&plan, internal("Bybit signature resend was refused"));
                    return;
                }
                cx.notice(
                    safe_cex_engine::Level::Warn,
                    safe_cex_engine::log::code::REQUEST_FAILED,
                    "Bybit rejected a signed request; resending it once",
                );
                let Stage::Plan(run) = &mut entry.stage else {
                    return;
                };
                run.active = Some(active);
                self.park(id, entry);
                return;
            }
            Decision::Replace(fallback) => {
                if cx
                    .replace_plan_call(&plan, &active.slot, &active.context)
                    .is_err()
                {
                    let _ = cx.fail_plan(&plan, internal("Bybit batch fallback was refused"));
                    return;
                }
                // Refused whole: nothing was placed, so each order may go alone.
                for (body, receipts) in fallback.into_iter().rev() {
                    run.queue.push_front(Leg {
                        planned: Planned {
                            receipts,
                            path: "/v5/order/create",
                            body,
                            call: Call::PlaceOne,
                        },
                        intent: LegIntent::Command,
                        lot: None,
                    });
                }
            }
            Decision::Restore(call) => {
                let continued = cx.continue_serial_plan(
                    &plan,
                    &active.slot,
                    &active.context,
                    partial_call(Operation::Mutation, 1),
                );
                if continued.is_err() {
                    let _ = cx.fail_plan(&plan, internal("Bybit stop restore was refused"));
                    return;
                }
                let Call::RestorePartialStop { .. } = &call else {
                    unreachable!("a restore names its stop")
                };
                let body = match &active.leg.planned.call {
                    Call::PartialStop {
                        restore: Some((_, body)),
                        ..
                    } => body.clone(),
                    _ => unreachable!("only a Partial stop restores"),
                };
                run.queue.push_front(Leg {
                    planned: Planned {
                        receipts: Vec::new(),
                        path: "/v5/position/trading-stop",
                        body,
                        call,
                    },
                    intent: LegIntent::Setting,
                    lot: None,
                });
            }
            Decision::Lost => {
                let _ = cx.fail_plan(&plan, internal("Bybit write fact was refused"));
                return;
            }
        }
        self.continue_ordinary(cx, id, entry);
    }

    /// This category's order read has landed. A plan cannot seal inside another
    /// call's delivery, so the cancel-alls waiting on it settle on a zero timer.
    /// Only a read they issued answers them: one landing earlier found no waiter.
    pub(crate) fn unlisted_read_landed(&mut self, cx: &mut Cx<'_, Pending>, confirmed: bool) {
        let category = self.category;
        let mut landed = false;
        for entry in self.trading.ordinary.values_mut() {
            if let Stage::Plan(run) = &mut entry.stage
                && run.unlisted_read == Some(category)
                && run.unlisted_landed.is_none()
            {
                run.unlisted_landed = Some(confirmed);
                landed = true;
            }
        }
        if landed && self.trading.unlisted_timer.is_none() {
            self.trading.unlisted_timer = Some(cx.set_timer(0.0));
        }
    }

    /// Every cancel-all whose read landed reports what the venue still lists,
    /// by symbol and id, or succeeds. False for any other timer.
    pub(crate) fn on_unlisted_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.trading.unlisted_timer != Some(timer) {
            return false;
        }
        self.trading.unlisted_timer = None;
        let landed: Vec<CommandId> = self
            .trading
            .ordinary
            .iter()
            .filter(|(_, entry)| {
                matches!(&entry.stage, Stage::Plan(run) if run.unlisted_landed.is_some())
            })
            .map(|(id, _)| *id)
            .collect();
        for id in landed {
            let Some(mut entry) = self.trading.ordinary.remove(&id) else {
                continue;
            };
            if let Stage::Plan(run) = &mut entry.stage {
                let unlisted = std::mem::take(&mut run.unlisted);
                let confirmed = run.unlisted_landed.take() == Some(true);
                run.unlisted_read = None;
                let left: Vec<&str> = unlisted
                    .iter()
                    .filter(|order| {
                        cx.store.order(order).is_some_and(|order| {
                            !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
                        })
                    })
                    .map(String::as_str)
                    .collect();
                if let Some(first) = left.first().and_then(|order| cx.store.order(order)) {
                    let (count, symbol, list) = (left.len(), &first.symbol, left.join_with(", "));
                    let message = if confirmed {
                        format!("Bybit cancel-all left {count} live order(s) on {symbol}: {list}")
                    } else {
                        format!(
                            "Bybit cancel-all could not confirm {count} order(s) on {symbol}: {list}"
                        )
                    };
                    cx.command_error(id, message.clone());
                    run.error
                        .get_or_insert(VenueError::new(message).outcome(Outcome::Unknown));
                }
            }
            self.continue_ordinary(cx, id, entry);
        }
        true
    }

    /// A leg the engine failed before or after release (provider refusal,
    /// transport loss). Independent orders of one batch (`continue_on_error`)
    /// retire only that leg and continue; otherwise the engine seals the plan.
    pub(crate) fn on_ordinary_leg_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
    ) {
        let Some(mut entry) = self.take_ordinary(&request) else {
            return;
        };
        if let Some(call) = &entry.hyro {
            let key = call.key.clone();
            self.on_hyro_leg_failed(cx, &request, entry, error);
            self.after_hyro(cx, &key, request.command);
            return;
        }
        let Stage::Plan(run) = &mut entry.stage else {
            self.park(request.command, entry);
            return;
        };
        let matches = run
            .active
            .as_ref()
            .is_some_and(|active| active.context == request.context);
        if matches
            && error.outcome == Some(Outcome::NotSent)
            && run.active.as_ref().is_some_and(|active| {
                matches!(active.leg.planned.call, Call::CancelPartialStop { .. })
            })
        {
            // A transport refusal on the cancel-partial-stop leg surfaces as
            // `NotSent`. The venue path has already composed the right
            // user-facing message for the partial put-back; replace the raw
            // transport text on this definite outcome.
            let _ = cx.override_plan_failure_message(&run.plan, error.message.clone());
        }
        if matches
            && run
                .active
                .as_ref()
                .is_some_and(|active| matches!(active.leg.planned.call, Call::PartialProof(_)))
        {
            let _ = cx.inherit_plan_uncertainty(&run.plan);
        }
        if !matches || !run.continue_on_error {
            self.park(request.command, entry);
            return;
        }
        let active = run.active.take().expect("matched active leg");
        if matches!(active.leg.planned.call, Call::PlaceLot { .. })
            && !matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected))
        {
            error = spot_unknown(error);
        }
        if matches!(active.leg.planned.call, Call::PlaceLot { .. })
            && (error.outcome == Some(Outcome::Unknown)
                || (run.stop_after_spot_order
                    && matches!(active.leg.planned.call, Call::PlaceLot { last: true })))
        {
            let failure = if error.outcome == Some(Outcome::Unknown) {
                error
            } else {
                run.error.take().unwrap_or(error)
            };
            let _ = cx.fail_plan(&run.plan, failure);
            return;
        }
        if cx
            .finish_plan_failed_call(&run.plan, &active.slot, &active.context)
            .is_err()
        {
            run.active = Some(active);
            self.park(request.command, entry);
            return;
        }
        cx.command_error(request.command, error.message.clone());
        run.error.get_or_insert(error);
        self.continue_ordinary(cx, request.command, entry);
    }

    pub(crate) fn partial_stop_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), VenueError> {
        let Then::Ordinary(request) = &pending.then else {
            return Ok(());
        };
        let Some(entry) = self.trading.ordinary.get(&request.command) else {
            return Ok(());
        };
        let Stage::Plan(run) = &entry.stage else {
            return Ok(());
        };
        let Some(active) = &run.active else {
            return Ok(());
        };
        if active.context != request.context {
            return Ok(());
        }
        if let Call::CancelPartialStop { guard, .. } = &active.leg.planned.call {
            guard.validate_order(store, false)?;
            guard.validate_original(store, release.time.raw_ms)?;
            guard.validate(store, release.time.raw_ms)?;
        }
        if let Some(flow) = &run.partial {
            match active.leg.planned.call {
                Call::PartialStop { .. } => flow.validate(store, release.time.raw_ms, false)?,
                Call::RestorePartialStop { .. } => {
                    flow.validate(store, release.time.raw_ms, true)?
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Record the classified failure before anything else. `weaken` marks a
    /// refusal the codec cannot prove; `evidence` IDs are reconciliation-only.
    #[allow(clippy::too_many_arguments)]
    fn record_failure(
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        table: &CodeTable,
        native: Option<&Native<'_>>,
        message: String,
        weaken: bool,
        evidence: &[String],
        known_effects: bool,
    ) -> Option<(Verdict, Option<ResendGrant>, VenueError)> {
        let mut fact = cx.classify_write(context, table, native, message).ok()?;
        if weaken {
            fact = fact.unproven();
        }
        if !evidence.is_empty() {
            fact = fact.with_order_ids(evidence);
        }
        let verdict = fact.verdict();
        let error = fact.error().clone();
        let grant = cx
            .http_write_fact(
                context,
                WriteFact::Failed {
                    fact,
                    known_effects,
                },
                None,
            )
            .ok()?;
        Some((verdict, grant, error))
    }

    fn unproven(
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        message: &str,
        evidence: &[String],
    ) -> Decision {
        match Self::record_failure(
            cx,
            context,
            &BYBIT,
            None,
            message.into(),
            true,
            evidence,
            false,
        ) {
            Some((_, _, error)) => Decision::Failed(error),
            None => Decision::Lost,
        }
    }

    fn accepted(cx: &mut Cx<'_, Pending>, context: &Context, order_ids: Vec<String>) -> bool {
        cx.http_write_fact(context, WriteFact::Accepted { order_ids }, None)
            .is_ok()
    }

    fn leg_decision(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        active: &Active,
        response: &Response<'_>,
        cancellations: Option<&mut Cancellations>,
    ) -> Decision {
        let context = &active.context;
        if matches!(
            active.leg.planned.call,
            Call::CancelPartialStop { .. }
                | Call::PartialStop { .. }
                | Call::RestorePartialStop { .. }
        ) && !(200..300).contains(&response.status)
        {
            return match Self::record_failure(cx, context, &PROTECTIVE, None,
                "The protective write has an unknown HTTP outcome; check Open Orders before retrying".into(), true, &[], false) {
                Some((_, _, error)) => Decision::Failed(error),
                None => Decision::Lost,
            };
        }
        match ack::answer(response.body) {
            Err(_) => Self::unproven(cx, context, "Bybit write acknowledgement is malformed", &[]),
            Ok(Answer::Refused {
                code,
                message,
                result,
                ext,
            }) => {
                let http_ok = (200..300).contains(&response.status);
                self.refused_leg(cx, id, active, code, message, result, ext, http_ok)
            }
            Ok(Answer::Success { result, ext }) => {
                self.accepted_leg(cx, active, response, result, ext, cancellations)
            }
        }
    }

    /// A refused mode means the cached one is wrong: the next write re-reads it.
    fn forget_body_modes(&mut self, body: &str) {
        #[derive(serde::Deserialize)]
        struct Symbols {
            #[serde(default)]
            symbol: Option<String>,
            #[serde(default)]
            request: Vec<Symbols>,
        }
        let Ok(body) = serde_json::from_str::<Symbols>(body) else {
            return;
        };
        for symbol in body
            .symbol
            .iter()
            .chain(body.request.iter().flat_map(|row| &row.symbol))
        {
            self.account.position_modes.remove(symbol);
            for product in &mut self.products {
                product.account.position_modes.remove(symbol);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn refused_leg(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        active: &Active,
        code: i64,
        message: String,
        result: Option<&serde_json::value::RawValue>,
        ext: Option<&serde_json::value::RawValue>,
        http_ok: bool,
    ) -> Decision {
        let context = &active.context;
        let planned = &active.leg.planned;
        if message.contains(MODE_MISMATCH) {
            self.forget_body_modes(&planned.body);
        }
        // TS treats this as done: the order already holds what was asked.
        if code == 10001
            && matches!(planned.call, Call::Amend)
            && message == "order not modified"
            && !planned.body.contains("\"triggerPrice\"")
        {
            let target = fields(&planned.body).order_id.unwrap_or_default();
            return if Self::accepted(cx, context, vec![target.clone()]) {
                Decision::Next(vec![target])
            } else {
                Decision::Lost
            };
        }
        // A trigger amend is done only when the whole answer is provably clean (a 2xx
        // naming nothing but its own order, strictly decoded) and every value it carries
        // already equals the stored stop's: compared directly, never masking a refusal.
        if code == 10001
            && matches!(planned.call, Call::Amend)
            && message == "order not modified"
            && planned.body.contains("\"triggerPrice\"")
            && http_ok
            && fields(&planned.body)
                .order_id
                .is_some_and(|own| ack::clean_amend_refusal(result, ext, &own).is_ok())
            && trigger_amend_matches_store(cx.store, &planned.body)
        {
            let target = fields(&planned.body).order_id.unwrap_or_default();
            return if Self::accepted(cx, context, vec![target.clone()]) {
                Decision::Next(vec![target])
            } else {
                Decision::Lost
            };
        }
        // Bybit's own clock is evidence; read it before sanitizing.
        let server = server_time(&message);
        // Signature diagnostics can contain the complete signed origin.
        let mut message = crate::signing_rejection(code, &message).unwrap_or(message);
        if let Call::MarginMode { .. } = planned.call
            && let Some(reasons) = ack::margin_reasons(result)
        {
            message = reasons;
        }
        let native = Native {
            code: NativeCode::Int(code),
            message: &message,
        };
        let table = if matches!(
            planned.call,
            Call::CancelPartialStop { .. }
                | Call::PartialStop { .. }
                | Call::RestorePartialStop { .. }
        ) {
            &PROTECTIVE
        } else if code == 110026 && matches!(planned.call, Call::MarginMode { .. }) {
            &MARGIN_MODE_ALREADY_SET
        } else {
            &BYBIT
        };
        let Ok(probe) = cx.classify_write(context, table, Some(&native), "") else {
            return Decision::Lost;
        };
        let rows = ack::carries_rows(result, ext);
        let evidence = ack::evidence_ids(result);
        let targets: &[String] = match &planned.call {
            Call::Cancel { ids } => ids,
            _ => &[],
        };
        // The order a single-target answer may echo: a partial stop, or the
        // amend's target (an attached TP/SL's parent).
        let own_target = match &planned.call {
            Call::CancelPartialStop { id, .. } => Some(id.clone()),
            Call::Amend | Call::ClearAttachedLeg { .. } => fields(&planned.body).order_id,
            _ => None,
        };
        // A whole answer with item rows or native IDs contradicts itself; an
        // absent-order answer must only echo its own targets.
        let protective_rate_limit = matches!(
            planned.call,
            Call::CancelPartialStop { .. }
                | Call::PartialStop { .. }
                | Call::RestorePartialStop { .. }
        ) && probe.verdict().kind() == Some(ErrorKind::RateLimited);
        let weaken = protective_rate_limit
            || match probe.verdict() {
                Verdict::Satisfied(_) => {
                    matches!(planned.call, Call::CancelAll { .. })
                        || (matches!(planned.call, Call::MarginMode { .. })
                            && ack::margin_applied(result) != Ok(true))
                        || (rows && evidence.is_empty())
                        || evidence.iter().any(|echo| {
                            !targets.contains(echo) && own_target.as_ref() != Some(echo)
                        })
                }
                verdict => verdict.is_refusal() && (rows || !evidence.is_empty()),
            };
        let Some((verdict, grant, error)) = Self::record_failure(
            cx,
            context,
            table,
            Some(&native),
            describe((code, message.clone())),
            weaken,
            &evidence,
            false,
        ) else {
            return Decision::Lost;
        };
        if weaken {
            return Decision::Failed(error);
        }
        // Bybit's clock corrects ours for the next attempt, even when this
        // write gets no resend (an execution-owned Hyro create).
        if let (Verdict::RefusedResend(_), Some(server)) = (verdict, server) {
            Self::observe_clock(cx, server);
        }
        match (verdict, &planned.call) {
            (Verdict::Satisfied(_), call) => {
                self.satisfied_effects(cx, call);
                Decision::Next(Vec::new())
            }
            (Verdict::RefusedResend(_), _) if grant.is_some() => {
                Decision::Resend(grant.expect("checked above"))
            }
            // Definitively refused after its cancel landed: the old stop goes back.
            (
                Verdict::Refused(_) | Verdict::RefusedResend(_),
                Call::PartialStop {
                    stop,
                    restore: Some((replaced, _)),
                },
            ) if !self.trading.gone_before_cancel.contains(replaced) => {
                Decision::Restore(Call::RestorePartialStop {
                    stop: stop.clone(),
                    refusal: error.message.clone(),
                })
            }
            (
                Verdict::Refused(_) | Verdict::RefusedResend(_),
                Call::PartialStop {
                    stop: (symbol, side, kind),
                    restore: None,
                },
            ) => {
                let mut error = error;
                error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
                error.message = format!(
                    "{}: the replacement was refused and standalone provenance is unproven; re-place protection on Bybit. {}",
                    safe_cex_engine::error::stop_gone_text(symbol, *side, *kind, false),
                    error.message
                );
                Decision::Failed(error)
            }
            (Verdict::Refused(_), Call::PlaceBatch { fallback })
                if code == 10001 && !fallback.is_empty() =>
            {
                cx.command_error(
                    id,
                    format!("Failed to place batch orders: {}", error.message),
                );
                Decision::Replace(fallback.clone())
            }
            _ => Decision::Failed(error),
        }
    }

    /// The requested state already holds. An absent order is not a Cancelled
    /// transition: the engine fences it and reconciles with one order read (D2).
    fn satisfied_effects(&mut self, cx: &mut Cx<'_, Pending>, call: &Call) {
        match call {
            Call::Leverage { symbol, leverage } => self.apply_leverage(cx, symbol, *leverage),
            Call::PositionMode { hedged } => self.apply_position_mode(cx, *hedged),
            Call::MarginMode { isolated } => self.apply_margin_mode(cx, *isolated),
            Call::PartialStop { .. } => self.schedule_partial_stop_refresh(cx),
            // Already gone when we canceled it: a refused replacement never puts it back.
            Call::CancelPartialStop { id, .. } => {
                self.trading.gone_before_cancel.insert(id.clone());
            }
            _ => {}
        }
    }

    fn apply_leverage(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, leverage: f64) {
        self.note_leverage_ack();
        cx.store.update_setting(symbol, |s| {
            s.leverage = leverage;
            s.buy_leverage = None;
            s.sell_leverage = None;
        });
        cx.store.update_positions(|p| {
            if p.symbol == symbol {
                p.leverage = leverage;
            }
        });
        cx.hold_setting(SettingKey::Leverage(symbol.to_owned()));
    }

    fn apply_margin_mode(&mut self, cx: &mut Cx<'_, Pending>, isolated: bool) {
        let margin = if isolated {
            MarginType::Isolated
        } else {
            MarginType::Cross
        };
        self.account.margin_type = Some(margin);
        for state in &mut self.products {
            state.account.margin_type = Some(margin);
        }
        cx.store.update_positions(|p| p.margin_type = Some(margin));
        // Account-wide, including already-set replies: publish the requested
        // value now and hold it over older source SETTINGS frames.
        let symbols: Vec<String> = cx
            .store
            .markets()
            .iter()
            .filter(|market| cx.store.setting(&market.symbol).is_some())
            .map(|market| market.symbol.clone())
            .collect();
        for symbol in symbols {
            cx.store
                .update_setting(&symbol, |s| s.margin_type = Some(margin));
            cx.hold_setting(SettingKey::MarginType(symbol));
        }
        self.fetch_positions(cx);
    }

    fn apply_position_mode(&mut self, cx: &mut Cx<'_, Pending>, hedged: bool) {
        cx.store.set_hedged(hedged);
        self.trading.mode_version += 1;
        if cx.frame_fed() {
            self.hold_position_mode(cx, hedged);
            return;
        }
        self.refresh_after_mode_change(cx);
    }

    fn batch_items<'a>(rows: &'a [Row]) -> Vec<BatchItem<'a>> {
        rows.iter()
            .map(|row| match row {
                Row::Accepted(id) => BatchItem::Accepted(id),
                Row::Refused { code, message } => BatchItem::Native(Native {
                    code: NativeCode::Int(*code),
                    message,
                }),
            })
            .collect()
    }

    fn accepted_leg(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        active: &Active,
        response: &Response<'_>,
        result: Option<&serde_json::value::RawValue>,
        ext: Option<&serde_json::value::RawValue>,
        cancellations: Option<&mut Cancellations>,
    ) -> Decision {
        let context = &active.context;
        let planned = &active.leg.planned;
        let local = self.local_order_lifecycle();
        match &planned.call {
            Call::PlaceOne | Call::PlaceLot { .. } => {
                let link = fields(&planned.body).order_link_id;
                let Ok(id) = ack::created(result, link.as_deref()) else {
                    let evidence = ack::evidence_ids(result);
                    return Self::unproven(
                        cx,
                        context,
                        "Bybit create acknowledgement does not prove its order",
                        &evidence,
                    );
                };
                if !Self::accepted(cx, context, vec![id.clone()]) {
                    return Decision::Lost;
                }
                if let Some(local) = &active.local {
                    self.apply_local_write(cx, local, response.body);
                }
                Decision::Next(vec![id])
            }
            Call::PlaceBatch { .. } => {
                let links: Vec<Option<String>> = fields(&planned.body)
                    .items
                    .into_iter()
                    .map(|item| item.order_link_id)
                    .collect();
                let refs: Vec<Option<&str>> = links.iter().map(Option::as_deref).collect();
                let Ok(rows) = ack::created_rows(result, ext, &refs) else {
                    let evidence = ack::evidence_ids(result);
                    return Self::unproven(
                        cx,
                        context,
                        &format!(
                            "Bybit create-batch acknowledgement does not prove its orders ({})",
                            ack::create_diagnostic(result, ext, &refs),
                        ),
                        &evidence,
                    );
                };
                let items = Self::batch_items(&rows);
                // The reason travels in the error: algo callers never see `command_error`.
                let reasons: Vec<String> = rows
                    .iter()
                    .filter_map(|row| match row {
                        Row::Refused { code, message } => Some(describe((*code, message.clone()))),
                        Row::Accepted(_) => None,
                    })
                    .collect();
                let Ok(outcome) = cx.http_write_batch(
                    context,
                    &BYBIT,
                    links.len(),
                    &items,
                    format!("Failed to place batch orders: {}", reasons.join_with("; ")),
                    None,
                ) else {
                    return Decision::Lost;
                };
                if rows.iter().any(
                    |row| matches!(row, Row::Refused { message, .. } if message.contains(MODE_MISMATCH)),
                ) {
                    self.forget_body_modes(&planned.body);
                }
                for (index, row) in rows.iter().enumerate() {
                    if let Row::Refused { code, message } = row {
                        cx.command_error(
                            context_command(context),
                            format!(
                                "Order {}: {}",
                                index + 1,
                                describe((*code, message.clone()))
                            ),
                        );
                    }
                }
                let ids: Vec<Option<String>> = rows
                    .iter()
                    .map(|row| match row {
                        Row::Accepted(id) => Some(id.clone()),
                        Row::Refused { .. } => None,
                    })
                    .collect();
                if !matches!(outcome, WriteResult::Failed(_))
                    && let Some(local) = &active.local
                {
                    self.publish_local_rows(cx, local, &ids);
                }
                match outcome {
                    WriteResult::Accepted { order_ids } => Decision::Next(order_ids),
                    WriteResult::Rejected(error)
                    | WriteResult::KnownPartial(error)
                    | WriteResult::PartialUnknown(error)
                    | WriteResult::Failed(error) => Decision::Failed(error),
                }
            }
            Call::Cancel { ids } if planned.path == "/v5/order/cancel" => {
                let [target] = ids.as_slice() else {
                    return Self::unproven(cx, context, "Bybit cancel has no single target", &[]);
                };
                if ack::targeted(result, target).is_err() {
                    return Self::unproven(
                        cx,
                        context,
                        "Bybit cancel acknowledgement does not name its order",
                        &ack::evidence_ids(result),
                    );
                }
                if !Self::accepted(cx, context, Vec::new()) {
                    return Decision::Lost;
                }
                if let Some(report) = cancellations {
                    report.accepted(target);
                }
                self.note_order_terminal(ids);
                mark_canceled(cx, ids, local);
                Decision::Next(Vec::new())
            }
            Call::Cancel { ids } => {
                let targets: Vec<&str> = ids.iter().map(String::as_str).collect();
                let Ok(ack::Cancelled { rows, foreign }) =
                    ack::cancelled_rows(result, ext, &targets)
                else {
                    return Self::unproven(
                        cx,
                        context,
                        &format!(
                            "Bybit batch cancel could not be confirmed; check open orders on Bybit before retrying ({})",
                            ack::cancel_diagnostic(result, ext, &targets),
                        ),
                        &[],
                    );
                };
                let items = Self::batch_items(&rows);
                let Ok(outcome) = cx.http_write_applied_batch(
                    context,
                    &BYBIT,
                    ids.len(),
                    &items,
                    "Bybit cancel was not confirmed",
                    None,
                ) else {
                    return Decision::Lost;
                };
                if let Some(report) = cancellations
                    && (200..300).contains(&response.status)
                {
                    report.rows(ids, &rows, response);
                }
                if !matches!(outcome, WriteResult::Failed(_)) {
                    let canceled: Vec<String> = rows
                        .iter()
                        .filter_map(|row| match row {
                            Row::Accepted(id) => Some(id.clone()),
                            Row::Refused { .. } => None,
                        })
                        .collect();
                    if !canceled.is_empty() {
                        self.note_order_terminal(&canceled);
                        mark_canceled(cx, &canceled, local);
                    }
                }
                // Bybit also cancelled orders we never named (attached TP/SL, linked legs).
                if foreign > 0 {
                    self.refresh_order_source(cx);
                }
                match outcome {
                    WriteResult::Accepted { .. } => Decision::Next(Vec::new()),
                    WriteResult::Rejected(error)
                    | WriteResult::KnownPartial(error)
                    | WriteResult::PartialUnknown(error)
                    | WriteResult::Failed(error) => Decision::Failed(error),
                }
            }
            Call::CancelPartialStop { id, batch, .. } => {
                let row = if *batch {
                    ack::cancelled_one(result, ext, id)
                } else {
                    ack::targeted(result, id).map(|()| Row::Accepted(id.clone()))
                };
                let Ok(row) = row else {
                    return Self::unproven(
                        cx,
                        context,
                        "Partial stop cancellation was not confirmed: malformed or mismatched acknowledgement",
                        &[],
                    );
                };
                let rows = [row];
                let items = Self::batch_items(&rows);
                let Ok(outcome) = cx.http_write_applied_batch(
                    context,
                    &PROTECTIVE,
                    1,
                    &items,
                    "Partial stop cancellation was rejected",
                    None,
                ) else {
                    return Decision::Lost;
                };
                match outcome {
                    WriteResult::Accepted { .. } => {
                        if !matches!(rows[0], Row::Accepted(_)) {
                            self.trading.gone_before_cancel.insert(id.clone());
                        }
                        Decision::Next(Vec::new())
                    }
                    WriteResult::Rejected(error)
                    | WriteResult::KnownPartial(error)
                    | WriteResult::PartialUnknown(error)
                    | WriteResult::Failed(error) => Decision::Failed(error),
                }
            }
            Call::CancelAll { ids } => {
                let Ok(affected) = ack::cancelled_all(result) else {
                    return Self::unproven(
                        cx,
                        context,
                        "Bybit cancel-all outcome is unknown: missing or malformed affected-order list",
                        &[],
                    );
                };
                // Futures select at most 500 native orders. The affected
                // list, including uncaptured arrivals, proves what it cancelled.
                let unlisted: Vec<String> = ids
                    .iter()
                    .filter(|id| {
                        !affected.contains(id)
                            && cx.store.order(id).is_some_and(|order| {
                                !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
                            })
                    })
                    .cloned()
                    .collect();
                if !Self::accepted(cx, context, Vec::new()) {
                    return Decision::Lost;
                }
                let decision = if unlisted.is_empty() {
                    Decision::Next(Vec::new())
                } else {
                    Decision::Unlisted(unlisted)
                };
                if !affected.is_empty() {
                    self.note_order_terminal(&affected);
                    mark_canceled(cx, &affected, local);
                }
                // Remove captured rows already terminal; never a live, derived
                // or uncaptured row, and never as cancellation proof.
                if local && cx.validate_http_write_housekeeping(context).is_ok() {
                    let terminal: Vec<&str> = ids
                        .iter()
                        .filter(|id| {
                            cx.store.order(id).is_some_and(|order| {
                                matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
                            })
                        })
                        .map(String::as_str)
                        .collect();
                    cx.store.remove_orders(&terminal);
                }
                decision
            }
            Call::ClearAttachedLeg { leg } => {
                let Some(target) = fields(&planned.body).order_id else {
                    return Self::unproven(cx, context, "Bybit amend has no target", &[]);
                };
                if ack::targeted(result, &target).is_err() {
                    return Self::unproven(
                        cx,
                        context,
                        "Bybit amend acknowledgement does not name its order",
                        &ack::evidence_ids(result),
                    );
                }
                if !Self::accepted(cx, context, Vec::new()) {
                    return Decision::Lost;
                }
                cx.store.remove_orders(&[leg.as_str()]);
                Decision::Next(Vec::new())
            }
            Call::Amend => {
                let Some(target) = fields(&planned.body).order_id else {
                    return Self::unproven(cx, context, "Bybit amend has no target", &[]);
                };
                if ack::targeted(result, &target).is_err() {
                    return Self::unproven(
                        cx,
                        context,
                        "Bybit amend acknowledgement does not name its order",
                        &ack::evidence_ids(result),
                    );
                }
                if !Self::accepted(cx, context, vec![target.clone()]) {
                    return Decision::Lost;
                }
                if let Some(local) = &active.local {
                    self.apply_local_write(cx, local, response.body);
                }
                Decision::Next(vec![target])
            }
            Call::TradingStop | Call::PartialStop { .. } | Call::RestorePartialStop { .. } => {
                if !matches!(planned.call, Call::TradingStop)
                    && ack::position_setting(result, ext).is_err()
                {
                    return Self::unproven(
                        cx,
                        context,
                        "The protective setting acknowledgement is ambiguous; check Open Orders",
                        &[],
                    );
                }
                if !Self::accepted(cx, context, Vec::new()) {
                    return Decision::Lost;
                }
                // No order id comes back; the stop's row arrives on the stream.
                if matches!(
                    planned.call,
                    Call::PartialStop { .. } | Call::RestorePartialStop { .. }
                ) {
                    self.schedule_partial_stop_refresh(cx);
                }
                Decision::Next(Vec::new())
            }
            Call::PartialProof(_) => unreachable!("proofs are read deliveries"),
            Call::Leverage { symbol, leverage } => {
                if !Self::accepted(cx, context, Vec::new()) {
                    return Decision::Lost;
                }
                self.apply_leverage(cx, symbol, *leverage);
                Decision::Next(Vec::new())
            }
            Call::MarginMode { isolated } => {
                // Success is retCode 0 with no reasons; the message is not "OK".
                if ack::margin_applied(result) != Ok(true) {
                    return Self::unproven(
                        cx,
                        context,
                        "Bybit margin mode change was not confirmed",
                        &[],
                    );
                }
                if !Self::accepted(cx, context, Vec::new()) {
                    return Decision::Lost;
                }
                self.apply_margin_mode(cx, *isolated);
                Decision::Next(Vec::new())
            }
            Call::PositionMode { hedged } => {
                if !Self::accepted(cx, context, Vec::new()) {
                    return Decision::Lost;
                }
                self.apply_position_mode(cx, *hedged);
                // Symbols with open orders or positions stay on the old mode; retCode is 0.
                #[derive(serde::Deserialize)]
                struct Answer {
                    #[serde(rename = "retMsg")]
                    message: Option<String>,
                }
                if let Some(message) = serde_json::from_str::<Answer>(response.body)
                    .ok()
                    .and_then(|body| body.message)
                    .filter(|message| message.starts_with("Partial symbols"))
                {
                    cx.command_error(context_command(context), message);
                }
                Decision::Next(Vec::new())
            }
        }
    }

    /// Reconcile unknown or absent-order writes with one fresh order read per
    /// product; an in-flight read predates a new seal, so it is not joined.
    pub(crate) fn reconcile_ordinary(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none() {
            return;
        }
        if self.variant.is_hyro() {
            // The lost creates' link reads go first; the snapshot follows.
            self.reconcile_hyro_links(cx);
            self.reconcile_hyro_snapshot(cx);
            return;
        }
        // An engine re-ask joins the post-mark reads already out; a new seal
        // replaces older ones.
        let reask = cx.reconcile_reask();
        let selected = self.category;
        for category in self.categories() {
            self.select_category(category);
            if reask {
                self.fetch_orders(cx);
            } else {
                self.reconcile_orders(cx);
            }
        }
        self.select_category(selected);
    }
}

fn context_command(context: &Context) -> CommandId {
    match context.owner() {
        safe_cex_engine::request_context::Owner::Command { id, .. } => id,
        _ => 0,
    }
}

fn disabled_product() -> VenueError {
    not_sent("Bybit product is not enabled").kind(ErrorKind::InvalidInput)
}
