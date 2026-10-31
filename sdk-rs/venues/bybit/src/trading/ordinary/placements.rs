//! Hyro placements (H2). A retained intent has at most one physical planned
//! execution; identical callers join it. A retry first re-reads every lot
//! that may have been sent, by link, on both order axes.
use super::{
    CALL_ATTEMPTS, CALL_TIMEOUT_MS, Decision, Gate, HyroCall, Leg, LegIntent, Ordinary, Request,
    Run, Stage, call_bound, internal,
};
use crate::hyro::{self, Found, Intent, LotState, MAX_SENDS, Phase};
use crate::trading::{Call, Planned, not_sent};
use crate::{BybitLinear, Pending};
use safe_cex_engine::engine::Window;
use safe_cex_engine::request_context::plan::{CallTerms, RetryWindow, StepBound};
use safe_cex_engine::request_context::{
    HttpOwnerRetirement, Operation, Owner, Plan, PlanPolicy, Policy, RetryPolicy,
};
use safe_cex_engine::{Command, CommandId, Cx, Method, Outcome, Response, VenueError};
use std::collections::VecDeque;

const BEING_RECONCILED: &str =
    "HyroTrader order intent is being reconciled; retry once its reconciliation finishes";

impl BybitLinear {
    /// The genuine entry of a Hyro placement: join the intent's live
    /// execution, or found one after re-reading every possibly-sent lot.
    pub(super) fn start_hyro(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Command::PlaceOrders {
            command: id,
            orders,
        } = &command
        else {
            return;
        };
        let id = *id;
        let checked = self
            .hyro_intent_key(cx, orders)
            .map_err(not_sent)
            .and_then(|key| match self.ordinary_gate(cx, &command) {
                Some(Gate::Fail(error)) => Err(error),
                _ => Ok(key),
            });
        self.drain_hyro_late();
        if let Ok(key) = &checked
            && self.hyro_live(cx, key)
        {
            self.join_hyro(cx, id, key);
            return;
        }
        self.wake_hyro_waiters(cx);
        let (policy, read_credential, write_credential) = self.ordinary_preflight();
        let Ok(preflight) = cx.begin_preflight(policy) else {
            return;
        };
        let key = match checked {
            Ok(key) => key,
            Err(error) => {
                let _ = cx.fail_preflight(&preflight, error);
                return;
            }
        };
        if let Some(intent) = self.trading.hyro.intents.get(&key)
            && matches!(intent.phase, Phase::Reading(_))
        {
            self.wait_for_hyro_founder(cx, id, &key, preflight);
            return;
        }
        if !self.trading.hyro.intents.contains_key(&key) {
            let lots = match self.plan_hyro_lots(cx, orders) {
                Ok(lots) => lots,
                Err(message) => {
                    let _ = cx.fail_preflight(&preflight, not_sent(message));
                    return;
                }
            };
            let digest = safe_cex_crypto::hex(&safe_cex_crypto::sha256(key.as_bytes()));
            self.trading.hyro.intents.insert(
                key.clone(),
                Intent {
                    digest,
                    lots,
                    phase: Phase::Idle,
                    founding: 0,
                },
            );
        }
        let intent = self
            .trading
            .hyro
            .intents
            .get_mut(&key)
            .expect("inserted above");
        // An earlier call's lot may be live or may have reached the venue: no
        // seal of this one, a remint refusal included, is Rejected or NotSent.
        if intent.placed() {
            let _ = cx.inherit_preflight_uncertainty(&preflight);
        }
        if let Err(message) = intent
            .lots
            .iter_mut()
            .try_for_each(|lot| hyro::remint(cx, lot))
        {
            let _ = cx.fail_preflight(&preflight, not_sent(message));
            return;
        }
        let receipts = intent.receipts();
        if !receipts.is_empty() {
            cx.command_progress(id, receipts);
        }
        let proven = vec![false; intent.lots.len()];
        self.found_hyro(&key, id);
        self.trading.ordinary.insert(
            id,
            Ordinary {
                preflight,
                read_credential,
                write_credential,
                command,
                stage: Stage::Idle,
                hyro: Some(HyroCall {
                    key: key.clone(),
                    proven,
                    execution: None,
                    observers: Vec::new(),
                }),
            },
        );
        self.resume_hyro(cx, id);
        self.after_hyro(cx, &key, id);
    }

    /// An identical caller observes the running execution; nothing is sent for it.
    fn join_hyro(&mut self, cx: &mut Cx<'_, Pending>, id: CommandId, key: &str) {
        // Only called once `hyro_live` saw this intent's running execution.
        let live = self.trading.hyro.intents.get(key).and_then(|intent| {
            let Phase::Live(entry) = &intent.phase else {
                return None;
            };
            let execution = entry.hyro.as_ref()?.execution.clone()?;
            let lots = u32::try_from(intent.lots.len()).unwrap_or(u32::MAX);
            Some((execution, intent.digest.clone(), intent.receipts(), lots))
        });
        let (_, _, write_credential) = self.ordinary_preflight();
        let lots = live.as_ref().map_or(1, |(.., lots)| *lots);
        let policy = Policy {
            operation: Operation::Mutation,
            credential: write_credential,
            timeout_ms: CALL_TIMEOUT_MS
                .saturating_mul(u32::from(CALL_ATTEMPTS))
                .saturating_mul(lots),
            pacing: None,
            retry: RetryPolicy::DefinitiveRefusal,
        };
        // A refused admission already sealed the command.
        let Ok(root) = cx.admit_http_command(policy) else {
            return;
        };
        let Some((execution, digest, receipts, _)) = live else {
            debug_assert!(false, "a Hyro join without a live execution");
            let error = internal("Hyro execution was lost").outcome(Outcome::NotSent);
            let _ = cx.fail_context(&root, error);
            return;
        };
        match cx.join_execution(&execution, &root, &digest) {
            Ok(_) => {
                if !receipts.is_empty() {
                    cx.command_progress(id, receipts);
                }
                if let Some(intent) = self.trading.hyro.intents.get_mut(key)
                    && let Phase::Live(entry) = &mut intent.phase
                    && let Some(call) = &mut entry.hyro
                {
                    call.observers.push(root);
                }
            }
            Err(_) => {
                let _ = cx.fail_context(&root, not_sent(BEING_RECONCILED));
            }
        }
    }

    /// Whether the intent's execution is still running; a sealed one is retired here.
    fn hyro_live(&mut self, cx: &Cx<'_, Pending>, key: &str) -> bool {
        let Some(intent) = self.trading.hyro.intents.get_mut(key) else {
            return false;
        };
        let Phase::Live(entry) = &intent.phase else {
            return false;
        };
        let Stage::Plan(run) = &entry.stage else {
            return false;
        };
        if cx.plan_slots(&run.plan).is_ok() {
            return true;
        }
        intent.phase = Phase::Idle;
        self.settle_hyro(key, None);
        false
    }

    /// Retire every execution the engine already sealed (expiry, unproven ACK).
    pub(crate) fn retire_sealed_hyro(&mut self, cx: &Cx<'_, Pending>) {
        let keys: Vec<String> = self.trading.hyro.intents.keys().cloned().collect();
        for key in keys {
            self.hyro_live(cx, &key);
        }
    }

    /// The next possibly-sent lot's link read, or this call's admission.
    fn resume_hyro(&mut self, cx: &mut Cx<'_, Pending>, id: CommandId) {
        let Some(mut entry) = self.trading.ordinary.remove(&id) else {
            return;
        };
        let Some(call) = &entry.hyro else {
            debug_assert!(false, "a Hyro resume without its intent");
            let _ = cx.fail_preflight(&entry.preflight, lost_intent());
            return;
        };
        let Some(intent) = self.trading.hyro.intents.get(&call.key) else {
            let _ = cx.fail_preflight(&entry.preflight, lost_intent());
            return;
        };
        let next = intent
            .lots
            .iter()
            .enumerate()
            .find(|(index, lot)| lot.maybe_sent() && !call.proven[*index]);
        if let Some((index, lot)) = next {
            if lot.stop.is_some() {
                let error = hyro::unknown(
                    &lot.link,
                    "position-stop outcome requires account reconciliation",
                );
                let _ = cx.fail_preflight(&entry.preflight, error);
                return;
            }
            if !self.send_hyro_link_read(cx, id, &mut entry, index, false) {
                let error = internal("Bybit order read was refused").outcome(Outcome::NotSent);
                let _ = cx.fail_preflight(&entry.preflight, error);
                return;
            }
            self.trading.ordinary.insert(id, entry);
            return;
        }
        let mut legs = VecDeque::new();
        for (index, lot) in intent.lots.iter().enumerate() {
            match &lot.state {
                LotState::Accepted(_) => {}
                LotState::Absent if lot.stop.is_some() || lot.sends < MAX_SENDS => {
                    legs.push_back(hyro_leg(lot, index));
                }
                LotState::Absent => {
                    let error = hyro::unknown(
                        &lot.link,
                        "submission could not be confirmed after complete reconciliation",
                    );
                    let _ = cx.fail_preflight(&entry.preflight, error);
                    return;
                }
                LotState::Unknown | LotState::Rejected => {
                    let error = hyro::unknown(&lot.link, "the order identity is unresolved");
                    let _ = cx.fail_preflight(&entry.preflight, error);
                    return;
                }
            }
        }
        let key = call.key.clone();
        if let Some(intent) = self.trading.hyro.intents.get_mut(&key) {
            intent.phase = Phase::Idle;
        }
        if !legs.is_empty() && !self.private_ready() {
            let _ = cx.fail_preflight(&entry.preflight, not_sent(super::HYRO_RECONNECTING));
            return;
        }
        if legs.is_empty() {
            let ids = intent_ids(self, &key);
            if cx.done_empty_plan(&entry.preflight, ids.clone()).is_ok() {
                self.settle_hyro(&key, Some(ids));
            }
            return;
        }
        self.admit_hyro(cx, id, entry, legs);
    }

    fn send_hyro_link_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        entry: &mut Ordinary,
        lot: usize,
        history: bool,
    ) -> bool {
        let Some(link) = entry
            .hyro
            .as_ref()
            .and_then(|call| self.trading.hyro.intents.get(&call.key))
            .and_then(|intent| intent.lots.get(lot))
            .map(|lot| lot.link.clone())
        else {
            return false;
        };
        let context = cx.preflight_read_context(
            &entry.preflight,
            Policy {
                operation: Operation::Read,
                credential: entry.read_credential.clone(),
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
        let Ok(context) = context else {
            return false;
        };
        let path = if history {
            "/v5/order/history"
        } else {
            "/v5/order/realtime"
        };
        let query = hyro::link_query(&link).into_string();
        if !self.send_ordinary(cx, id, context.clone(), Method::Get, path, query) {
            return false;
        }
        entry.stage = Stage::HyroRead {
            lot,
            history,
            context,
            boundary: self.hyro_boundary(),
        };
        true
    }

    pub(super) fn on_hyro_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        mut entry: Ordinary,
        response: &Response<'_>,
    ) {
        let Stage::HyroRead {
            lot,
            history,
            context,
            boundary,
        } = std::mem::replace(&mut entry.stage, Stage::Idle)
        else {
            return;
        };
        if cx
            .validate_preflight_delivery(&entry.preflight, &context)
            .is_err()
        {
            entry.stage = Stage::HyroRead {
                lot,
                history,
                context,
                boundary,
            };
            self.trading.ordinary.insert(id, entry);
            return;
        }
        let Some(key) = entry.hyro.as_ref().map(|call| call.key.clone()) else {
            debug_assert!(false, "a Hyro read without its intent");
            let _ = cx.fail_preflight(&entry.preflight, lost_intent());
            return;
        };
        let Some((link, symbol)) = self
            .trading
            .hyro
            .intents
            .get(&key)
            .and_then(|intent| intent.lots.get(lot))
            .map(|lot| (lot.link.clone(), lot.symbol.clone()))
        else {
            let _ = cx.fail_preflight(&entry.preflight, lost_intent());
            return;
        };
        let found = self.hyro_found(cx, &symbol, &link, Ok(response));
        if let Found::Incomplete(cause) = found {
            let _ = cx.fail_preflight(&entry.preflight, hyro::unknown(&link, cause));
            return;
        }
        if cx
            .finish_preflight_call(&entry.preflight, &context)
            .is_err()
        {
            let error = internal("Bybit order read lost its custody");
            let _ = cx.fail_preflight(&entry.preflight, error);
            return;
        }
        self.drain_hyro_late();
        self.apply_hyro_found(cx, &key, &link, &found, boundary);
        match found {
            Found::Absent if !history => {
                if !self.send_hyro_link_read(cx, id, &mut entry, lot, true) {
                    let error = internal("Bybit order read was refused").outcome(Outcome::NotSent);
                    let _ = cx.fail_preflight(&entry.preflight, error);
                    return;
                }
                self.trading.ordinary.insert(id, entry);
                return;
            }
            Found::Absent => {
                if let Some(call) = &mut entry.hyro {
                    call.proven[lot] = true;
                }
            }
            Found::Rejected => {
                let reminted = self
                    .trading
                    .hyro
                    .intents
                    .get_mut(&key)
                    .and_then(|intent| intent.lots.get_mut(lot))
                    .map(|lot| hyro::remint(cx, lot));
                if let Some(Err(message)) = reminted {
                    let _ = cx.fail_preflight(&entry.preflight, not_sent(message));
                    return;
                }
            }
            Found::Accepted { .. } | Found::Incomplete(_) => {}
        }
        self.trading.ordinary.insert(id, entry);
        self.resume_hyro(cx, id);
    }

    /// One serial step per lot this call may send, as one shared execution.
    fn admit_hyro(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        mut entry: Ordinary,
        legs: VecDeque<Leg>,
    ) {
        let Some(digest) = entry
            .hyro
            .as_ref()
            .and_then(|call| self.trading.hyro.intents.get(&call.key))
            .map(|intent| intent.digest.clone())
        else {
            debug_assert!(false, "a Hyro admission without its intent");
            let _ = cx.fail_preflight(&entry.preflight, lost_intent());
            return;
        };
        let calls = [call_bound(&[])];
        let steps: Vec<StepBound<'_>> = legs
            .iter()
            .map(|_| StepBound {
                wait_ms: 0,
                calls: &calls,
            })
            .collect();
        let admitted = cx.begin_planned_execution(
            &entry.preflight,
            &steps,
            PlanPolicy {
                independent_calls: false,
                operation: Operation::Mutation,
                credential: entry.write_credential.clone(),
                pacing: None,
                retry: RetryPolicy::DefinitiveRefusal,
            },
            Window::UntilTerminal,
            &digest,
        );
        let Ok((plan, execution, founder)) = admitted else {
            let error =
                internal("Bybit trading plan admission was refused").outcome(Outcome::NotSent);
            let _ = cx.fail_preflight(&entry.preflight, error);
            return;
        };
        if let Some(call) = &mut entry.hyro {
            call.execution = Some(execution);
            call.observers.push(founder);
        }
        entry.stage = Stage::Plan(Box::new(Run {
            plan,
            queue: legs,
            active: None,
            continue_on_error: false,
            unlisted: Vec::new(),
            unlisted_read: None,
            unlisted_landed: None,
            ids: Vec::new(),
            error: None,
        }));
        self.continue_ordinary(cx, id, entry);
    }

    /// Every create counts toward the lot's two sends; its identity reaches
    /// every observer the first time it is dispatched.
    pub(super) fn hyro_leg_sent(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        founder: CommandId,
        call: &HyroCall,
        lot: usize,
        receipts: &[safe_cex_engine::OrderIdentityReceipt],
    ) {
        let Some(lot) = self
            .trading
            .hyro
            .intents
            .get_mut(&call.key)
            .and_then(|intent| intent.lots.get_mut(lot))
        else {
            return;
        };
        let first = lot.sends == 0;
        lot.sends = lot.sends.saturating_add(1);
        lot.state = LotState::Unknown;
        lot.reconcile = true;
        lot.doubted |= lot.pending;
        lot.pending = true;
        if !first || receipts.is_empty() {
            return;
        }
        // Outside one of the execution's deliveries only the founder observes it.
        let published = call
            .execution
            .as_ref()
            .is_some_and(|execution| cx.execution_progress(execution, receipts.to_vec()).is_ok());
        if !published {
            cx.command_progress(founder, receipts.to_vec());
        }
    }

    /// The leg was refused before any request was queued.
    pub(super) fn hyro_leg_unsent(&mut self, key: &str, lot: Option<usize>) {
        if let Some(lot) = lot.and_then(|lot| {
            self.trading
                .hyro
                .intents
                .get_mut(key)
                .and_then(|intent| intent.lots.get_mut(lot))
        }) {
            lot.sends = lot.sends.saturating_sub(1);
            lot.state = LotState::Absent;
            lot.answered();
        }
    }

    pub(super) fn hyro_leg_decided(&mut self, key: &str, lot: usize, decision: &Decision) {
        let Some(lot) = self
            .trading
            .hyro
            .intents
            .get_mut(key)
            .and_then(|intent| intent.lots.get_mut(lot))
        else {
            return;
        };
        match decision {
            Decision::Next(ids) => {
                lot.state = LotState::Accepted(ids.first().cloned().unwrap_or_default());
                lot.answered();
            }
            // An observed acceptance outranks a contradictory later answer.
            _ if matches!(lot.state, LotState::Accepted(_)) => {}
            // A refusal of this send cannot rule out an earlier unanswered one
            // landing later: the lot stays Unknown on its link.
            Decision::Failed(error) if error.outcome == Some(Outcome::Rejected) => {
                lot.pending = false;
                lot.state = if lot.doubted {
                    LotState::Unknown
                } else {
                    LotState::Rejected
                };
                lot.reconcile = lot.doubted;
            }
            // A signature refusal proves the create was not placed; its resend replaces it.
            Decision::Resend(_) => {}
            _ => lot.state = LotState::Unknown,
        }
    }

    /// A lost create seals the call Unknown at once, naming its link; the
    /// reconcile read and a caller retry resolve it (R1(a), Q3).
    pub(super) fn on_hyro_leg_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        entry: Ordinary,
        error: VenueError,
    ) {
        let Stage::Plan(run) = &entry.stage else {
            self.park(request.command, entry);
            return;
        };
        let (Some(call), Some(lot)) = (
            entry.hyro.as_ref(),
            run.active
                .as_ref()
                .filter(|active| active.context == request.context)
                .and_then(|active| active.leg.lot),
        ) else {
            self.park(request.command, entry);
            return;
        };
        let key = call.key.clone();
        if error.outcome == Some(Outcome::NotSent) {
            // Refused before release: this send never happened (legacy sign failure).
            self.hyro_leg_unsent(&key, Some(lot));
            self.park(request.command, entry);
            return;
        }
        let link = self
            .trading
            .hyro
            .intents
            .get(&key)
            .and_then(|intent| intent.lots.get(lot))
            .filter(|lot| lot.stop.is_none())
            .map(|lot| lot.link.clone());
        let Some(link) = link else {
            self.park(request.command, entry);
            return;
        };
        // The journal keeps the transport text; each observer gets this report,
        // then its own terminal naming the link, with the journal's outcome.
        let error = hyro::unknown(&link, &error.message);
        for root in &call.observers {
            if let Owner::Command { id, .. } = root.owner() {
                cx.command_error(id, error.message.clone());
            }
            let _ = cx.detach_execution_observer(root, error.clone());
        }
        let plan = run.plan.clone();
        let _ = cx.fail_plan(&plan, error);
    }

    /// After a Hyro callback: an entry neither parked nor reading has sealed;
    /// any founding that sealed wakes its waiters.
    pub(super) fn after_hyro(&mut self, cx: &mut Cx<'_, Pending>, key: &str, founder: CommandId) {
        self.settle_sealed_hyro(key, founder);
        self.wake_hyro_waiters(cx);
    }

    fn settle_sealed_hyro(&mut self, key: &str, founder: CommandId) {
        let reading = self.trading.ordinary.contains_key(&founder);
        let Some(intent) = self.trading.hyro.intents.get_mut(key) else {
            return;
        };
        match intent.phase {
            Phase::Live(_) => return,
            Phase::Reading(command) if command != founder || reading => return,
            _ => {}
        }
        intent.phase = Phase::Idle;
        self.settle_hyro(key, None);
    }

    pub(super) fn take_hyro_owner(&mut self, owner: Owner) -> Option<Ordinary> {
        let key = self
            .trading
            .hyro
            .intents
            .iter()
            .find_map(|(key, intent)| match &intent.phase {
                Phase::Live(entry)
                    if matches!(&entry.stage, Stage::Plan(run) if run.plan.owner() == owner) =>
                {
                    Some(key.clone())
                }
                _ => None,
            })?;
        self.take_hyro_live(&key)
    }

    pub(super) fn take_hyro_plan(&mut self, plan: &Plan) -> Option<(String, CommandId, Ordinary)> {
        let entry = self.take_hyro_owner(plan.owner())?;
        let call = entry.hyro.as_ref()?;
        let id = match entry.command {
            Command::PlaceOrders { command, .. } => command,
            _ => return None,
        };
        Some((call.key.clone(), id, entry))
    }

    pub(super) fn take_hyro_live(&mut self, key: &str) -> Option<Ordinary> {
        let intent = self.trading.hyro.intents.get_mut(key)?;
        match std::mem::replace(&mut intent.phase, Phase::Idle) {
            Phase::Live(entry) => Some(*entry),
            phase => {
                intent.phase = phase;
                None
            }
        }
    }

    pub(super) fn park_hyro(&mut self, key: &str, entry: Ordinary) {
        if let Some(intent) = self.trading.hyro.intents.get_mut(key) {
            intent.phase = Phase::Live(Box::new(entry));
        }
    }

    /// Cleanup only: a retired founder read or physical Plan leaves its intent idle.
    pub(super) fn retire_hyro(
        &mut self,
        owner: &HttpOwnerRetirement,
        readers: Vec<(String, CommandId)>,
    ) {
        let mut settled = Vec::new();
        for (key, command) in readers {
            if let Some(intent) = self.trading.hyro.intents.get_mut(&key)
                && matches!(intent.phase, Phase::Reading(reader) if reader == command)
            {
                intent.phase = Phase::Idle;
                settled.push(key);
            }
        }
        for (key, intent) in &mut self.trading.hyro.intents {
            if let Phase::Live(entry) = &intent.phase
                && let Stage::Plan(run) = &entry.stage
                && owner.matches_plan(&run.plan)
            {
                intent.phase = Phase::Idle;
                settled.push(key.clone());
            }
        }
        for key in settled {
            self.settle_hyro(&key, None);
        }
        self.retire_hyro_waiters(owner);
    }
}

/// The retained intent vanished mid-call; the inherited fact still projects
/// Unknown when an earlier call may have sent.
fn lost_intent() -> VenueError {
    internal("Hyro intent was lost").outcome(Outcome::NotSent)
}

fn intent_ids(venue: &BybitLinear, key: &str) -> String {
    venue
        .trading
        .hyro
        .intents
        .get(key)
        .map_or_else(|| "[]".into(), Intent::ids)
}

fn hyro_leg(lot: &hyro::Lot, index: usize) -> Leg {
    let (planned, intent) = match (&lot.stop, lot.body()) {
        (Some(stop), _) => (
            Planned {
                receipts: Vec::new(),
                path: "/v5/position/trading-stop",
                body: stop.clone(),
                call: Call::TradingStop,
            },
            LegIntent::Setting,
        ),
        (None, body) => (
            Planned {
                receipts: lot.receipt().into_iter().collect(),
                path: "/v5/order/create",
                body: body.unwrap_or_default(),
                call: Call::PlaceOne,
            },
            LegIntent::Command,
        ),
    };
    Leg {
        planned,
        intent,
        lot: Some(index),
    }
}
