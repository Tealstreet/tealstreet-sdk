//! Guarded cancel/read/create edits; each physical leg belongs to one serial Plan.
use super::*;
use safe_cex_engine::classify::{
    CodeTable, Fact, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact,
};
use safe_cex_engine::credential_selection::{CredentialRole, Selection};
use safe_cex_engine::request_context::{
    Context, Error, HttpOwnerRetirement, HttpRelease, Operation, Plan, Preflight, PreflightPolicy,
    PreflightPurpose, ReconciliationData,
    plan::{CallAuth, SerialCall, SerialPolicy, Slot},
};
use safe_cex_engine::signing::Identity;

const WRITE: SerialCall = SerialCall {
    auth: CallAuth::Preflight,
    operation: Operation::Mutation,
    timeout_ms: 20_000,
    attempts: 2,
    pacing: None,
};
const READ: SerialCall = SerialCall {
    operation: Operation::Read,
    attempts: 1,
    ..WRITE
};
const RESTORE: SerialCall = SerialCall {
    attempts: 1,
    ..WRITE
};
const fn refusal(code: &'static str, kind: ErrorKind) -> Row {
    Row {
        key: Key::Text(code),
        verdict: Verdict::Refused(kind),
        only: None,
    }
}
// KuCoin Classic Futures common errors: 400005 is an invalid signature;
// 300003 is insufficient balance. Unlisted and rate-limit codes prove no outcome.
const TABLE: CodeTable = CodeTable {
    rows: &[
        refusal("400001", ErrorKind::AuthInvalid),
        refusal("400002", ErrorKind::AuthInvalid),
        refusal("400003", ErrorKind::AuthInvalid),
        refusal("400004", ErrorKind::AuthInvalid),
        Row {
            key: Key::Text("400005"),
            verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid),
            only: None,
        },
        refusal("400006", ErrorKind::AuthPermission),
        refusal("400007", ErrorKind::AuthPermission),
        refusal("400100", ErrorKind::Rejected),
        refusal("300003", ErrorKind::Rejected),
        Row {
            key: Key::Text("429000"),
            verdict: Verdict::Unknown(Some(ErrorKind::RateLimited)),
            only: None,
        },
        Row {
            key: Key::Text("404"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: Some(Intent::Cancel),
        },
        Row {
            key: Key::Text("404000"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: Some(Intent::Cancel),
        },
    ],
    success: &[Key::Text("200000")],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum At {
    Cancel,
    Proof,
    Place,
    Restore,
}
#[derive(Default)]
pub(crate) struct State {
    runs: HashMap<CommandId, Run>,
    references: HashMap<String, [Option<(f64, f64)>; 3]>,
}
struct Run {
    preflight: Preflight,
    plan: Option<Plan>,
    auth: (Selection, Identity),
    read: (Selection, Identity),
    original: Order,
    related: Vec<Order>,
    guards: Vec<StopGuard>,
    input: Command,
    deadline: f64,
    timer: Option<TimerId>,
    next: Option<(At, Step)>,
    replacement: Option<Replacement>,
    placement: Option<Step>,
    stop: Option<Box<StopProtect>>,
    cancelled: bool,
    restore_taken: bool,
}
pub struct Request {
    command: CommandId,
    plan: Plan,
    slot: Slot,
    context: Context,
    at: At,
    step: Step,
    resent: bool,
}
impl Request {
    pub(crate) fn reconciliation(&self) -> Result<ReconciliationData, Error> {
        ReconciliationData::new(self.command.to_string())
    }
}
impl State {
    pub(crate) fn observe(&mut self, symbol: &str, source: StopTriggerType, price: f64, now: f64) {
        let slot = match source {
            StopTriggerType::LastPrice => 0,
            StopTriggerType::MarkPrice => 1,
            StopTriggerType::IndexPrice => 2,
        };
        self.references.entry(symbol.to_owned()).or_default()[slot] = Some((price, now));
    }
    fn reference(
        &self,
        store: &safe_cex_engine::Store,
        guard: &StopGuard,
        now: f64,
    ) -> Result<(), VenueError> {
        let slot = match guard.reference {
            StopTriggerType::LastPrice => 0,
            StopTriggerType::MarkPrice => 1,
            StopTriggerType::IndexPrice => 2,
        };
        let observed = self
            .references
            .get(&guard.order.symbol)
            .and_then(|fields| fields[slot]);
        let ticker = store.ticker(&guard.order.symbol);
        let current = ticker.map(|t| match guard.reference {
            StopTriggerType::LastPrice => t.last,
            StopTriggerType::MarkPrice => t.mark,
            StopTriggerType::IndexPrice => t.index,
        });
        if !observed.is_some_and(|(price, seen)| {
            positive(price) && seen <= now && now - seen <= 5_000.0 && current == Some(price)
        }) {
            return Err(not_sent("trigger-source receipt is stale or missing"));
        }
        Ok(())
    }
    pub(crate) fn retain_listed(&mut self, store: &safe_cex_engine::Store) {
        self.references
            .retain(|symbol, _| store.market(symbol).is_some());
    }
    pub(crate) fn retired(&mut self, owner: &HttpOwnerRetirement) {
        self.runs
            .retain(|_, run| !owner.matches_preflight(&run.preflight));
    }
}
pub(super) fn guarded(store: &safe_cex_engine::Store, id: &str) -> bool {
    store.order(id).is_some_and(|o| {
        advanced(o.kind)
            || o.kind == OrderType::Conditional
            || o.id.contains("__")
            || o.take_profit.is_some()
            || o.stop_loss.is_some()
            || store
                .orders()
                .iter()
                .any(|leg| leg.id.starts_with(&format!("{}__", o.id)))
    })
}
pub(super) fn handles(store: &safe_cex_engine::Store, command: &Command) -> bool {
    matches!(command, Command::AmendOrder { id, .. } if guarded(store, id))
}

fn unknown(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}
fn protection_order(run: &Run) -> &Order {
    run.stop
        .as_ref()
        .map_or(&run.original, |stop| &stop.guard.order)
}
fn protection_name(order: &Order) -> (&'static str, &'static str) {
    let side = if order.side == OrderSide::Buy {
        "short"
    } else {
        "long"
    };
    let kind = if order.kind == OrderType::TakeProfit {
        "take profit"
    } else {
        "stop"
    };
    (side, kind)
}
fn gone(run: &Run, message: impl Into<String>, restored: bool) -> VenueError {
    let order = protection_order(run);
    let symbol = &order.symbol;
    let (side, kind) = protection_name(order);
    let words = if restored {
        format!(
            "Your {symbol} {side} position's {kind} is unchanged: its replacement was refused, so the original was put back as a new order"
        )
    } else {
        format!(
            "Your {symbol} {side} position has no {kind}: its {kind} was removed; please re-place it: {}",
            message.into()
        )
    };
    let mut error = VenueError::new(words);
    error.protection = Some(if restored {
        safe_cex_engine::error::Protection::Restored
    } else {
        safe_cex_engine::error::Protection::Unprotected
    });
    error.presents = true;
    error
}
fn words(run: &Run, at: At) -> String {
    let order = protection_order(run);
    let symbol = &order.symbol;
    let (side, kind) = protection_name(order);
    match at {
        At::Cancel => format!(
            "Your {symbol} {side} position's {kind} edit outcome is unknown; check Open Orders before editing again"
        ),
        At::Proof => format!(
            "Your {symbol} {side} position's {kind} cancel was sent but could not be confirmed; its outcome is unknown and no replacement was sent. Check Open Orders"
        ),
        At::Place => format!(
            "Your {symbol} {side} position's {kind} replacement outcome is unknown. Check Open Orders before re-placing protection"
        ),
        At::Restore => format!(
            "Your {symbol} {side} position's {kind} put-back outcome is unknown. Check Open Orders before re-placing protection"
        ),
    }
}

fn call(at: At) -> SerialCall {
    match at {
        At::Proof => READ,
        At::Restore => RESTORE,
        _ => WRITE,
    }
}
impl Kucoin {
    pub(crate) fn protective_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        input: Command,
    ) {
        let auth = self
            .slots()
            .select(CredentialRole::Mutation)
            .ok()
            .map(|(s, i)| (s, i.clone()));
        let Some(auth) = auth else {
            let Ok(root) = cx.admit_http_command(crate::local_mutation_command_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &root,
                not_sent("KuCoin credentials are required").kind(ErrorKind::AuthInvalid),
            );
            return;
        };
        let read = self
            .slots()
            .select(CredentialRole::Read { scope: None })
            .ok()
            .map(|(s, i)| (s, i.clone()))
            .unwrap_or_else(|| auth.clone());
        let Ok(preflight) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: Some(read.0.authority(&read.1)),
            write_credential: Some(auth.0.authority(&auth.1)),
        }) else {
            return;
        };
        if cx.frame_fed() {
            let _ = cx.fail_preflight(&preflight, not_sent("Your KuCoin stop was not changed: frame-fed prices have no local trigger-source receipt"));
            return;
        }
        let Command::AmendOrder { id, .. } = &input else {
            unreachable!()
        };
        let original = cx.store.order(id).unwrap().clone();
        if let Err(error) = cx.park_preflight(&preflight) {
            let _ = cx.fail_preflight(&preflight, error.venue_error());
            return;
        }
        let parent_id = original.id.split("__").next().unwrap();
        let related = cx
            .store
            .orders()
            .iter()
            .filter(|order| {
                order.id == parent_id || order.id.starts_with(&format!("{parent_id}__"))
            })
            .cloned()
            .collect();
        let symbol = original.symbol.clone();
        let timer = cx.set_timer(20.0);
        self.trading.protective.runs.insert(
            command,
            Run {
                preflight,
                plan: None,
                auth,
                read,
                original,
                related,
                guards: vec![],
                input,
                deadline: cx.now + 10_000.0,
                timer: Some(timer),
                next: None,
                replacement: None,
                placement: None,
                stop: None,
                cancelled: false,
                restore_taken: false,
            },
        );
        if !self.account_metadata_ready(cx, &symbol) {
            self.account_metadata(cx, &symbol);
        }
    }
    pub(crate) fn protective_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let Some(command) = self
            .trading
            .protective
            .runs
            .iter()
            .find_map(|(id, run)| (run.timer == Some(timer)).then_some(*id))
        else {
            return false;
        };
        let mut run = self.trading.protective.runs.remove(&command).unwrap();
        run.timer = None;
        if cx.now >= run.deadline {
            let _ = cx.fail_preflight(
                &run.preflight,
                not_sent("KuCoin account metadata unavailable"),
            );
        } else if self.account_metadata_ready(cx, &run.original.symbol) {
            let preflight = run.preflight.clone();
            self.trading.protective.runs.insert(command, run);
            if let Err(error) = cx.request_preflight_ready(&preflight) {
                let _ = cx.fail_preflight(&preflight, error.venue_error());
            }
        } else {
            run.timer = Some(cx.set_timer(20.0));
            self.trading.protective.runs.insert(command, run);
        }
        true
    }
    pub(crate) fn protective_ready(&mut self, cx: &mut Cx<'_, Pending>, preflight: &Preflight) {
        let Some(command) = self
            .trading
            .protective
            .runs
            .iter()
            .find_map(|(id, r)| (r.preflight.owner() == preflight.owner()).then_some(*id))
        else {
            return;
        };
        let mut run = self.trading.protective.runs.remove(&command).unwrap();
        let result = (|| {
            if run
                .related
                .iter()
                .any(|order| cx.store.order(&order.id) != Some(order))
            {
                return Err("KuCoin stop changed or filled before cancellation".to_string());
            }
            let Command::AmendOrder {
                id, price, amount, ..
            } = &run.input
            else {
                unreachable!()
            };
            let (parent, opts) = self.replacement(cx, id, *price, *amount)?;
            let stop_limit = native_stop_limit(&parent);
            run.placement = Some(self.format_placement(cx, &opts, stop_limit)?);
            let leg = parent.id != run.original.id;
            let targets: Vec<_> = if advanced(run.original.kind) {
                vec![run.original.clone()]
            } else {
                run.related
                    .iter()
                    .filter(|order| order.id != parent.id && advanced(order.kind))
                    .cloned()
                    .collect()
            };
            if targets.is_empty() {
                return Err(
                    "KuCoin attached protection shape is unavailable; order was not changed".into(),
                );
            }
            let mut first = None;
            for target in run.related.iter().filter(|order| advanced(order.kind)) {
                if target.filled != 0.0 && target.id == parent.id {
                    return Err("KuCoin stop already filled; it was not changed".into());
                }
                let mut stop = self
                    .stop_protect(cx, &target.id, &parent, &opts)?
                    .ok_or("KuCoin protection metadata unavailable")?;
                stop.guard.below = Some(
                    matches!(target.kind, OrderType::StopLoss | OrderType::Conditional)
                        == (target.side == OrderSide::Sell),
                );
                stop.guard.closes = Some(if target.side == OrderSide::Sell {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                });
                stop.guard
                    .validate(cx.store, cx.now)
                    .map_err(|e| e.message)?;
                run.guards.push(stop.guard.clone());
                if target.id == targets[0].id {
                    first = Some(stop);
                }
            }
            let mut stop = first.ok_or("KuCoin protection shape changed before cancellation")?;
            stop.restorable &= !leg && run.original.parent_id.is_none() && run.related.len() == 1;
            let filled = parent
                .extra
                .as_ref()
                .and_then(|e| e.filled_in_placement_units)
                .unwrap_or(parent.filled / crate::account::contract_size(cx.store, &parent.symbol));
            let step = cancel(&parent.id, &api_symbol(cx.store, &parent.symbol));
            run.replacement = Some(Replacement {
                amended: (run.original.side, run.original.kind),
                id: parent.id,
                options: opts,
                stop_limit,
                filled,
                polls: 0,
                read_retries: 0,
            });
            run.stop = Some(stop);
            Ok(step)
        })();
        let step = match result {
            Ok(step) => step,
            Err(error) => {
                let _ = cx.fail_preflight(preflight, not_sent(error));
                return;
            }
        };
        let (plan, slot) = match cx.admit_serial_plan(
            preflight,
            WRITE,
            SerialPolicy {
                credential: Some(run.auth.0.authority(&run.auth.1)),
                pacing: None,
                // A definitive family miss may precede a confirmed replacement.
                refusals_complete: true,
            },
        ) {
            Ok(value) => value,
            Err(error) => {
                let _ = cx.fail_preflight(preflight, error.venue_error());
                return;
            }
        };
        run.plan = Some(plan.clone());
        self.trading.protective.runs.insert(command, run);
        self.protective_issue(cx, command, plan, slot, At::Cancel, step);
    }
    pub(crate) fn protective_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        let Some(command) = self.trading.protective.runs.iter().find_map(|(id, run)| {
            run.plan
                .as_ref()
                .filter(|p| p.owner() == plan.owner())
                .map(|_| *id)
        }) else {
            return;
        };
        let next = self
            .trading
            .protective
            .runs
            .get_mut(&command)
            .unwrap()
            .next
            .take();
        let Some((at, step)) = next else { return };
        match cx
            .plan_slots(plan)
            .and_then(|s| s.into_iter().next().ok_or(Error::State))
        {
            Ok(slot) => self.protective_issue(cx, command, plan.clone(), slot, at, step),
            Err(error) => {
                let _ = cx.fail_plan(plan, error.venue_error());
            }
        }
    }
    fn protective_issue(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        plan: Plan,
        slot: Slot,
        at: At,
        step: Step,
    ) {
        let context = match at {
            At::Cancel => cx.plan_protection_cancel_leg_context(&plan, &slot),
            At::Proof => cx.plan_leg_context(&plan, &slot),
            _ => cx.plan_leg_context_with_intent(&plan, &slot, Intent::Place),
        };
        match context {
            Ok(context) => self.protective_queue(
                cx,
                Request {
                    command,
                    plan,
                    slot,
                    context,
                    at,
                    step,
                    resent: false,
                },
            ),
            Err(error) => {
                let _ = cx.fail_plan(&plan, error.venue_error());
            }
        }
    }
    fn protective_queue(&mut self, cx: &mut Cx<'_, Pending>, pending: Request) {
        let Some(run) = self.trading.protective.runs.get(&pending.command) else {
            return;
        };
        let plan = pending.plan.clone();
        if let Err(error) = cx.present_plan_failure(&plan, words(run, pending.at)) {
            let _ = cx.fail_plan(&plan, error.venue_error());
            return;
        }
        let auth = if pending.at == At::Proof {
            &run.read
        } else {
            &run.auth
        };
        let recipe = crate::sign::dispatch::Http::new(
            auth.1.clone(),
            pending.step.method,
            crate::REST,
            pending.step.path.clone(),
            pending.step.body.clone(),
            self.options
                .broker
                .as_ref()
                .map(crate::sign::BrokerOptions::descriptor),
        );
        if let Some(receipt) = &pending.step.receipt {
            cx.command_progress(pending.command, vec![receipt.clone()]);
        }
        if let Err((Pending::Protective(pending), error)) = cx.request_signed_managed(
            pending.context.clone(),
            None,
            pending.step.method,
            format!("{}{}", crate::REST, crate::encoded_path(&pending.step.path)),
            pending.step.body.clone(),
            Box::new(recipe),
            Pending::Protective(pending),
        ) {
            self.protective_failed(cx, pending, error.venue_error());
        }
    }
    pub(crate) fn protective_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Request,
        release: &HttpRelease,
    ) -> Result<(), VenueError> {
        if pending.at == At::Proof {
            return Ok(());
        }
        let run = self
            .trading
            .protective
            .runs
            .get(&pending.command)
            .filter(|r| {
                r.plan
                    .as_ref()
                    .is_some_and(|p| p.owner() == pending.plan.owner())
            })
            .ok_or_else(|| not_sent("KuCoin protective edit was retired"))?;
        let stop = run
            .stop
            .as_ref()
            .ok_or_else(|| not_sent("KuCoin stop guard unavailable"))?;
        if pending.at == At::Cancel {
            let parent = &run.replacement.as_ref().unwrap().id;
            let related: Vec<_> = store
                .orders()
                .iter()
                .filter(|order| order.id == *parent || order.id.starts_with(&format!("{parent}__")))
                .collect();
            if related.len() != run.related.len()
                || run.related.iter().any(|order| !related.contains(&order))
            {
                let mut error = not_sent(
                    "Your KuCoin order was not changed: it changed or filled before cancellation (protection group changed)",
                );
                error.presents = true;
                return Err(error);
            }
            return run
                .guards
                .iter()
                .try_for_each(|guard| {
                    guard.validate(store, release.time.raw_ms).and_then(|_| {
                        self.trading
                            .protective
                            .reference(store, guard, release.time.raw_ms)
                            .map_err(|mut error| {
                                error.message = format!(
                                    "Your {} order was not changed: {}",
                                    guard.order.symbol, error.message
                                );
                                error
                            })
                    })
                })
                .map_err(|mut error| {
                    error.presents = true;
                    error
                });
        }
        let parent = &run.replacement.as_ref().unwrap().id;
        if store
            .orders()
            .iter()
            .any(|order| order.id == *parent || order.id.starts_with(&format!("{parent}__")))
            || self.account_order_filled(parent) > run.replacement.as_ref().unwrap().filled
        {
            return Err(gone(
                run,
                "new order or fill invalidated cancellation proof",
                false,
            )
            .outcome(Outcome::NotSent));
        }
        for guard in &run.guards {
            if let Some(side) = guard.closes
                && !store
                    .positions()
                    .iter()
                    .any(|p| p.symbol == guard.order.symbol && p.side == side && p.contracts > 0.0)
            {
                return Err(
                    gone(run, "no position remains on its protected side", false)
                        .outcome(Outcome::NotSent),
                );
            }
            let trigger = if pending.at == At::Restore {
                stop.original_trigger
            } else {
                guard.trigger
            };
            guard
                .validate_trigger(store, release.time.raw_ms, trigger)
                .and_then(|_| {
                    self.trading
                        .protective
                        .reference(store, guard, release.time.raw_ms)
                })
                .map_err(|error| gone(run, error.message, false).outcome(Outcome::NotSent))?;
        }
        Ok(())
    }
    pub(crate) fn protective_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        error: VenueError,
    ) {
        if !self
            .trading
            .protective
            .runs
            .get(&pending.command)
            .is_some_and(|run| {
                run.plan
                    .as_ref()
                    .is_some_and(|plan| plan.owner() == pending.plan.owner())
            })
        {
            return;
        }
        let run = self
            .trading
            .protective
            .runs
            .remove(&pending.command)
            .unwrap();
        if pending.at == At::Proof {
            self.protective_proof_error(cx, &pending, run, error.message);
            return;
        }
        let error = if run.cancelled
            && error.outcome == Some(Outcome::NotSent)
            && error.protection.is_none()
        {
            gone(&run, error.message, false)
        } else {
            error
        };
        let mut error = error;
        if error.outcome == Some(Outcome::NotSent) {
            error.presents = true;
        }
        let _ = cx.fail_plan(&pending.plan, error);
    }
    fn protective_proof_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: &Request,
        mut run: Run,
        message: String,
    ) {
        let replacement = run.replacement.as_mut().unwrap();
        if replacement.read_retries < 2 {
            replacement.read_retries += 1;
            let step = confirm_cancel(
                replacement.clone(),
                &api_symbol(cx.store, &run.original.symbol),
            );
            self.protective_next(cx, pending, run, At::Proof, step, 100);
        } else {
            let _ = cx.inherit_plan_uncertainty(&pending.plan);
            let _ = cx.fail_plan(&pending.plan, unknown(format!("KuCoin cancel was sent but could not be confirmed: {message}. Check Open Orders")));
        }
    }
    fn protective_next(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: &Request,
        mut run: Run,
        at: At,
        step: Step,
        delay: u32,
    ) {
        run.next = Some((at, step));
        self.trading.protective.runs.insert(pending.command, run);
        if let Err(error) = cx.continue_serial_plan_after(
            &pending.plan,
            &pending.slot,
            &pending.context,
            delay,
            call(at),
        ) {
            let _ = cx.fail_plan(&pending.plan, error.venue_error());
        }
    }
    pub(crate) fn protective_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Request,
        status: u16,
        body: &str,
    ) {
        if cx
            .validate_plan_delivery(&pending.plan, &pending.context)
            .is_err()
        {
            return;
        }
        let Some(mut run) = self.trading.protective.runs.remove(&pending.command) else {
            return;
        };
        let envelope = serde_json::from_str::<Envelope<'_>>(body).ok();
        let success =
            (200..300).contains(&status) && envelope.as_ref().is_some_and(|e| e.code == "200000");
        if pending.at == At::Proof {
            self.protective_proof(cx, pending, run, envelope.filter(|_| success));
            return;
        }
        let placed = if matches!(pending.at, At::Place | At::Restore) && success {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Created {
                order_id: String,
            }
            envelope
                .as_ref()
                .and_then(|e| e.data)
                .and_then(|d| serde_json::from_str::<Created>(d.get()).ok())
                .filter(|d| {
                    !d.order_id.is_empty() && d.order_id != run.replacement.as_ref().unwrap().id
                })
                .map(|d| d.order_id)
        } else {
            None
        };
        if success && (pending.at == At::Cancel || placed.is_some()) {
            let ids: Vec<String> = placed.clone().into_iter().collect();
            if let Err(error) = cx.http_write_fact(
                &pending.context,
                WriteFact::Accepted {
                    order_ids: ids.clone(),
                },
                None,
            ) {
                let _ = cx.fail_plan(&pending.plan, error.venue_error());
                return;
            }
            if pending.at == At::Cancel {
                let step = confirm_cancel(
                    run.replacement.as_ref().unwrap().clone(),
                    &api_symbol(cx.store, &run.original.symbol),
                );
                self.protective_next(cx, &pending, run, At::Proof, step, 20);
            } else {
                if let Some(id) = placed {
                    self.account_placed_order(cx, &id, pending.step.body.as_deref().unwrap());
                }
                if let Err(error) =
                    cx.finish_plan_call(&pending.plan, &pending.slot, &pending.context)
                {
                    let _ = cx.fail_plan(&pending.plan, error.venue_error());
                    return;
                }
                if pending.at == At::Restore {
                    let _ = cx.fail_plan(&pending.plan, gone(&run, "", true));
                } else {
                    if let Err(error) = cx.done_plan(&pending.plan, json(&ids)) {
                        let _ = cx.fail_plan(&pending.plan, error.venue_error());
                    }
                }
            }
            return;
        }
        let message = envelope.as_ref().map_or_else(
            || format!("Invalid KuCoin response (HTTP {status})"),
            |e| format!("KuCoin API Error {}: {} (HTTP {status})", e.code, e.msg),
        );
        let native = envelope.as_ref().map(|e| Native {
            code: NativeCode::Text(e.code),
            message: e.msg,
        });
        let fact = match cx.classify_write(&pending.context, &TABLE, native.as_ref(), message) {
            Ok(fact) => fact,
            Err(error) => {
                let _ = cx.fail_plan(&pending.plan, error.venue_error());
                return;
            }
        };
        let fact: Fact = if status == 429 || success {
            fact.unproven()
        } else {
            fact
        };
        let error = fact.error().clone();
        let refused = fact.verdict().is_refusal();
        let grant = match cx.http_write_fact(
            &pending.context,
            WriteFact::Failed {
                fact,
                known_effects: false,
            },
            None,
        ) {
            Ok(grant) => grant,
            Err(error) => {
                let _ = cx.fail_plan(&pending.plan, error.venue_error());
                return;
            }
        };
        if let Some(grant) = grant
            && pending.at != At::Restore
            && !pending.resent
        {
            pending.resent = true;
            match cx.plan_resend_context(&pending.plan, &pending.slot, &pending.context, grant) {
                Ok(context) => {
                    pending.context = context;
                    self.trading.protective.runs.insert(pending.command, run);
                    self.protective_queue(cx, pending);
                }
                Err(error) => {
                    let _ = cx.fail_plan(&pending.plan, error.venue_error());
                }
            }
            return;
        }
        if refused
            && status == 404
            && pending.at == At::Cancel
            && matches!(
                pending.step.call,
                Call::Cancel {
                    fallback: false,
                    ..
                }
            )
        {
            let id = run.replacement.as_ref().unwrap().id.clone();
            let step = Step {
                receipt: None,
                method: Method::Delete,
                path: format!("/api/v1/stopOrders/{}", encode(&id)),
                body: None,
                call: Call::Cancel { id, fallback: true },
            };
            // The native order-family fallback has no pacing delay.
            match cx.continue_serial_plan(&pending.plan, &pending.slot, &pending.context, WRITE) {
                Ok(slot) => {
                    self.trading.protective.runs.insert(pending.command, run);
                    self.protective_issue(
                        cx,
                        pending.command,
                        pending.plan,
                        slot,
                        At::Cancel,
                        step,
                    );
                }
                Err(error) => {
                    let _ = cx.fail_plan(&pending.plan, error.venue_error());
                }
            }
        } else if refused
            && pending.at == At::Place
            && !run.restore_taken
            && run.stop.as_ref().is_some_and(|s| s.restorable)
        {
            run.restore_taken = true;
            let restore = run.stop.as_ref().unwrap().restore.clone();
            self.protective_next(cx, &pending, run, At::Restore, restore, 20);
        } else {
            let error = if refused && run.cancelled {
                gone(
                    &run,
                    if pending.at == At::Restore {
                        format!(
                            "its replacement and the put-back were both refused ({})",
                            error.message
                        )
                    } else {
                        error.message
                    },
                    false,
                )
            } else {
                error
            };
            let _ = cx.fail_plan(&pending.plan, error);
        }
    }
    fn protective_proof(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        mut run: Run,
        envelope: Option<Envelope<'_>>,
    ) {
        #[derive(Clone, Copy, Deserialize)]
        struct Fill(#[serde(deserialize_with = "safe_cex_engine::de::number")] f64);
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Proof {
            #[serde(default, alias = "orderId")]
            id: String,
            #[serde(default)]
            is_active: Option<bool>,
            #[serde(default)]
            cancel_exist: Option<bool>,
            #[serde(default)]
            status: String,
            #[serde(default)]
            filled_size: Option<Fill>,
            #[serde(default)]
            deal_size: Option<Fill>,
            #[serde(default)]
            size: Option<Fill>,
        }
        let proof = envelope
            .and_then(|e| e.data)
            .and_then(|d| serde_json::from_str::<Proof>(d.get()).ok());
        let replacement = run.replacement.as_mut().unwrap();
        let proof = match proof
            .filter(|p| p.id == replacement.id && p.filled_size.or(p.deal_size).is_some())
        {
            Some(proof) => proof,
            None => {
                self.protective_proof_error(
                    cx,
                    &pending,
                    run,
                    "cancellation proof is invalid or missing the exact target/fills".into(),
                );
                return;
            }
        };
        let filled = proof.filled_size.or(proof.deal_size).unwrap().0;
        if !filled.is_finite()
            || filled < 0.0
            || proof
                .filled_size
                .zip(proof.deal_size)
                .is_some_and(|(a, b)| a.0 != b.0)
        {
            self.protective_proof_error(cx, &pending, run, "invalid final fill quantity".into());
            return;
        }
        if filled
            < replacement
                .filled
                .max(self.account_order_filled(&replacement.id))
        {
            self.protective_proof_error(
                cx,
                &pending,
                run,
                "cancellation proof has stale fills".into(),
            );
            return;
        }
        replacement.read_retries = 0;
        let cancelled = proof.is_active == Some(false)
            && (proof.cancel_exist == Some(true)
                || matches!(proof.status.as_str(), "canceled" | "cancelled"));
        let fully_filled = proof.is_active == Some(false)
            && proof.size.is_some_and(|n| positive(n.0) && filled >= n.0);
        if !cancelled && !fully_filled {
            if replacement.polls >= 30 {
                let _ = cx.inherit_plan_uncertainty(&pending.plan);
                let _ = cx.fail_plan(&pending.plan, unknown(words(&run, At::Proof)));
                return;
            }
            replacement.polls += 1;
            let step = confirm_cancel(
                replacement.clone(),
                &api_symbol(cx.store, &run.original.symbol),
            );
            self.protective_next(cx, &pending, run, At::Proof, step, 100);
            return;
        }
        self.account_order_terminal(cx, &replacement.id);
        run.cancelled = true;
        if filled > replacement.filled {
            let _ = cx.fail_plan(
                &pending.plan,
                gone(&run, "the target filled during cancellation", false),
            );
            return;
        }
        let step = run
            .placement
            .take()
            .expect("the validated replacement was captured before cancellation");
        self.protective_next(cx, &pending, run, At::Place, step, 20);
    }
}
