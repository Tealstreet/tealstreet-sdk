//! Immutable native-plan admission and resources under one command generation.
use super::owned_call::{Admission, Call, Stage};
use super::*;
use crate::request_context::plan::{CallTerms, Schedule, Slot, StepBound};
use crate::{Action, TimerId};
#[path = "request_context_conditional.rs"]
mod conditional;
use conditional::{AdmissionDeclaration, terminal_ordinals};
use std::ops::Range;

pub(super) struct State {
    // One fallibly prepared cell keeps the legacy CommandOwner entry small.
    body: Vec<Body>,
}

struct Body {
    handle: Plan,
    receipt_cap: Option<(u64, f64)>,
    policy: PlanPolicy,
    timer: TimerId,
    schedule: Schedule,
    terminals: Vec<usize>,
    entries: Vec<Entry>,
    handoff: Option<Delivery>,
    initial_custody: bool,
}

struct Entry {
    terms: CallTerms,
    logical: u64,
    ids: Range<u64>,
    signing: Option<Range<u64>>,
    credit: Credit,
    call: Option<Call>,
    eligible_at: Option<f64>,
    notified: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Credit {
    Unissued,
    Issued,
    Released,
}

fn take(ids: &mut Range<u64>, count: u64) -> Range<u64> {
    let start = ids.start;
    let end = start
        .checked_add(count)
        .filter(|&end| end <= ids.end)
        .expect("complete plan resource reservation");
    ids.start = end;
    start..end
}

impl State {
    pub(super) fn context_current(&self, context: &Context) -> bool {
        self.body[0].entries.iter().any(|entry| {
            entry
                .call
                .as_ref()
                .is_some_and(|call| call.context == *context)
        })
    }
    pub(super) fn admission(
        &self,
        context: &Context,
        delivery: Option<&Delivery>,
    ) -> Result<Option<Admission>, Error> {
        let call = self.body[0]
            .entries
            .iter()
            .find_map(|entry| entry.call.as_ref().filter(|call| call.context == *context))
            .ok_or(Error::Revoked)?;
        if !matches!(call.stage, Stage::Ready) || delivery != call.admission.predecessor.as_ref() {
            return Err(Error::Revoked);
        }
        Ok(Some(call.admission.clone()))
    }
    pub(super) fn commit(&mut self, admission: &Admission) {
        let call = self.body[0]
            .entries
            .iter_mut()
            .find_map(|entry| {
                entry
                    .call
                    .as_mut()
                    .filter(|call| call.context == admission.context)
            })
            .expect("selected plan call");
        assert!(call.admission.attempt == admission.attempt && matches!(call.stage, Stage::Ready));
        call.stage = Stage::Active { failed: false };
    }
    pub(super) fn record_failure(&mut self, delivery: &Delivery) -> bool {
        if let Some(call) = self.body[0]
            .entries
            .iter_mut()
            .find_map(|entry| entry.call.as_mut().filter(|call| call.matches(delivery)))
            && let Stage::Active { failed } = &mut call.stage
        {
            *failed = true;
            return true;
        }
        false
    }
    pub(super) fn handle(&self) -> Plan {
        self.body[0].handle.clone()
    }
    pub(super) fn timer(&self) -> TimerId {
        self.body[0].timer
    }
    pub(super) fn unissued(&self) -> usize {
        self.body[0]
            .entries
            .iter()
            .filter(|entry| entry.credit == Credit::Unissued)
            .count()
    }
}

impl ContextRegistry {
    pub(super) fn reserved_context_count(&self) -> usize {
        self.reserved_contexts
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(in super::super) fn plan_scan_visits(&self) -> usize {
        self.plan_scan_visits.get()
    }

    // Shared checked preparation precedes journal sealing or slot retirement.
    #[inline(never)]
    fn prepare_plan_seal(
        &self,
        plan: &Plan,
        now: f64,
        delivery: Option<&Delivery>,
        error: Option<VenueError>,
    ) -> Result<conditional::PreparedPlanSeal, Error> {
        self.validate_plan_seal_origin(plan, now, delivery)?;
        let owner = self.owner_entry(plan.owner()).expect("live Plan owner");
        let mut reports = Vec::new();
        let (journal, legacy) = if let Some(writes) = owner.writes.as_ref() {
            let prepared = writes.prepare_seal(error)?;
            if prepared.is_failure() {
                for message in writes.reports() {
                    reports.try_reserve(1).map_err(|_| Error::Capacity)?;
                    let mut retained = String::new();
                    retained
                        .try_reserve_exact(message.len())
                        .map_err(|_| Error::Capacity)?;
                    retained.push_str(message);
                    reports.push(retained);
                }
            }
            (Some(prepared), None)
        } else {
            (None, error)
        };
        Ok(conditional::PreparedPlanSeal {
            owner: plan.owner(),
            journal,
            legacy,
            reports,
            fanout: Vec::new(),
            sealed: Vec::new(),
            retired: Vec::new(),
        })
    }

    fn validate_plan_seal_origin(
        &self,
        plan: &Plan,
        now: f64,
        delivery: Option<&Delivery>,
    ) -> Result<(), Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        let body = self.plan_body(plan)?;
        let owner = self.owner_entry(plan.owner()).expect("live Plan owner");
        let admitted = delivery.is_none() && owner.deliveries == 0
            || delivery.is_some_and(|delivery| {
                body.handoff.as_ref() == Some(delivery)
                    || body.entries.iter().any(|entry| {
                        entry.call.as_ref().is_some_and(|call| {
                            call.matches(delivery)
                                || (matches!(call.stage, Stage::Ready)
                                    && call.admission.predecessor.as_ref() == Some(delivery))
                        })
                    })
            });
        if !admitted {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    fn commit_plan_seal(
        &mut self,
        plan: &Plan,
        prepared: conditional::PreparedPlanSeal,
    ) -> (Owner, Seal) {
        self.plan_body(plan).expect("prepared current Plan");
        assert_eq!(plan.owner(), prepared.owner);
        let owner = self
            .owner_entry_mut(prepared.owner)
            .expect("prepared current owner");
        let seal = match prepared.journal {
            Some(journal) => match owner
                .writes
                .as_mut()
                .expect("prepared write journal")
                .commit_seal(journal)
            {
                Some(error) => Seal::Failure(error, prepared.reports),
                None => Seal::Success,
            },
            None => Seal::Legacy(prepared.legacy),
        };
        (prepared.owner, seal)
    }

    pub(super) fn initial_plan_custody(&self, handle: &Plan) -> Result<bool, Error> {
        Ok(self.plan_body(handle)?.initial_custody)
    }

    fn plan_body(&self, handle: &Plan) -> Result<&Body, Error> {
        if !handle.issued_by(&self.issuer) {
            return Err(Error::Revoked);
        }
        if !matches!(handle.owner(), Owner::Command { .. }) && !self.physical_owner(handle.owner())
        {
            return Err(Error::Invalid);
        }
        let owner = self.owner_entry(handle.owner()).ok_or(Error::Revoked)?;
        let CommandPhase::Planned(state) = &owner.phase else {
            return Err(Error::Revoked);
        };
        let body = &state.body[0];
        if body.handle.deadline() != handle.deadline()
            || body
                .receipt_cap
                .is_some_and(|(receipt, cap)| receipt == 0 || handle.deadline() > cap)
        {
            return Err(Error::Revoked);
        }
        Ok(body)
    }

    fn plan_body_mut(&mut self, handle: &Plan) -> Result<&mut Body, Error> {
        self.plan_body(handle)?;
        let CommandPhase::Planned(state) = &mut self.owner_entry_mut(handle.owner()).unwrap().phase
        else {
            unreachable!()
        };
        Ok(&mut state.body[0])
    }

    pub(in super::super) fn plan_from_context(&self, context: &Context) -> Option<Plan> {
        if !context.issued_by(&self.issuer) {
            return None;
        }
        if !matches!(context.owner(), Owner::Command { .. })
            && !self.physical_owner(context.owner())
        {
            return None;
        }
        let owner = self.owner_entry(context.owner())?;
        let CommandPhase::Planned(state) = &owner.phase else {
            return None;
        };
        state
            .context_current(context)
            .then(|| state.body[0].handle.clone())
    }

    fn validate_plan(&self, handle: &Plan, now: f64) -> Result<&Body, Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        let body = self.plan_body(handle)?;
        if now >= body.handle.deadline() {
            return Err(Error::TimedOut);
        }
        Ok(body)
    }
}

impl<P> Cx<'_, P> {
    /// Admit only the final native plan. Mode/precondition reads retain their
    /// original generation and do not consume this checked whole-plan deadline.
    pub fn admit_plan(
        &mut self,
        preflight: &Preflight,
        steps: &[StepBound<'_>],
        policy: PlanPolicy,
    ) -> Result<Plan, Error> {
        self.admit_declared_plan(
            preflight,
            AdmissionDeclaration {
                steps,
                policy,
                terminals: &[],
            },
        )
    }

    fn admit_declared_plan(
        &mut self,
        preflight: &Preflight,
        declaration: AdmissionDeclaration<'_, '_, '_>,
    ) -> Result<Plan, Error> {
        self.admit_plan_owned(preflight, declaration, false)
            .map(|(plan, _)| plan)
    }

    /// The planned-execution admission: the same declaration path, with no
    /// conditional terminals (none are declared for execution Plans yet).
    pub(in super::super) fn admit_physical_plan(
        &mut self,
        preflight: &Preflight,
        steps: &[StepBound<'_>],
        policy: PlanPolicy,
    ) -> Result<(Plan, Option<(u64, Context)>), Error> {
        self.admit_plan_owned(
            preflight,
            AdmissionDeclaration {
                steps,
                policy,
                terminals: &[],
            },
            true,
        )
    }

    /// `physical` moves the admitted Plan, its slots and journal from the
    /// founder to a new execution-owned physical owner, and turns the founder
    /// into an entry-owned observer root bounded by the Plan root. Returns
    /// that execution identity and founder root. One transaction either way.
    fn admit_plan_owned(
        &mut self,
        preflight: &Preflight,
        declaration: AdmissionDeclaration<'_, '_, '_>,
        physical: bool,
    ) -> Result<(Plan, Option<(u64, Context)>), Error> {
        let extra = if physical { 2 } else { 0 };
        let AdmissionDeclaration {
            steps,
            policy,
            terminals,
        } = declaration;
        let (intent, old_read) = self.preflight_plan_origin(preflight)?;
        let (operation, credential) = match intent.purpose {
            PreflightPurpose::ReadOnly => (Operation::Read, intent.read_credential),
            PreflightPurpose::WriteAfterReads => (Operation::Mutation, intent.write_credential),
        };
        if policy.operation != operation || policy.credential != credential {
            return Err(Error::Invalid);
        }
        let receipt_cap = self.io.request_contexts.preflight_receipt_cap(preflight)?;
        if receipt_cap.is_some() && operation != Operation::Read {
            return Err(Error::Invalid);
        }
        if receipt_cap.is_some_and(|(_, cap)| !self.now.is_finite() || cap - self.now < 1.) {
            return Err(Error::TimedOut);
        }
        let schedule = Schedule::from_steps(steps)?;
        let terminals = terminal_ordinals(steps, &policy, terminals)?;
        let bound = schedule.bound();
        let native_deadline = bound.deadline(self.now)?;
        let deadline = receipt_cap.map_or(native_deadline, |(_, cap)| native_deadline.min(cap));
        Context::new(
            preflight.owner(),
            1,
            operation,
            policy.credential.clone(),
            self.now,
            deadline,
            policy.pacing,
        )?
        .with_retry(policy.retry)?;
        let count = bound.logical_calls();
        let attempts = bound.physical_attempts();
        if policy.retry == crate::request_context::RetryPolicy::Never
            && schedule.declarations().any(|terms| terms.attempts > 1)
        {
            return Err(Error::Invalid);
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(count)
            .map_err(|_| Error::Capacity)?;
        for terms in schedule.declarations() {
            entries.push(Entry {
                terms,
                logical: 0,
                ids: 0..0,
                signing: None,
                credit: Credit::Unissued,
                call: None,
                eligible_at: None,
                notified: false,
            });
        }
        for slot in schedule.available() {
            let index = schedule.ordinal(&slot)?;
            let at = eligibility(self.now, entries[index].terms.wait_ms, deadline);
            entries[index].eligible_at = Some(at);
            entries[index].notified = false;
        }
        let writes = if operation == Operation::Mutation {
            Some(CommandWrites::new_reserved(attempts, count)?)
        } else {
            None
        };
        let mut leg_intents = Vec::new();
        if operation == Operation::Mutation {
            leg_intents
                .try_reserve_exact(count)
                .map_err(|_| Error::Capacity)?;
        }
        let mut body = Vec::new();
        body.try_reserve_exact(1).map_err(|_| Error::Capacity)?;
        self.io.request_contexts.prune(self.now);
        let old_slot = old_read.as_ref().is_some_and(|context| {
            self.io
                .request_contexts
                .contexts
                .contains_key(&context.leg())
        }) as usize;
        let future = self
            .io
            .request_contexts
            .reserved_contexts
            .checked_add(count)
            .ok_or(Error::Capacity)?;
        if self
            .io
            .request_contexts
            .contexts
            .len()
            .checked_sub(old_slot)
            .and_then(|len| len.checked_add(future))
            .and_then(|len| len.checked_add(extra))
            .is_none_or(|total| total > CAPACITY)
        {
            return Err(Error::Capacity);
        }
        self.io
            .request_contexts
            .contexts
            .try_reserve(future + extra)
            .map_err(|_| Error::Capacity)?;
        self.io.timers.try_reserve(1).map_err(|_| Error::Capacity)?;
        self.io
            .request_contexts
            .plan_index
            .try_reserve(1)
            .map_err(|_| Error::Capacity)?;
        let io_count = 1 + count as u64 + 3 * attempts as u64 + extra as u64;
        let signed = policy.credential.is_some();
        if !self.io.next_id.available(io_count)
            || (signed && !self.io.signing.available_ids(attempts as u64))
        {
            return Err(Error::Capacity);
        }
        // All fallible declaration/storage checks precede either namespace.
        let mut ids = self
            .io
            .reserve_ids(io_count)
            .expect("preflighted whole plan");
        let timer = TimerId(take(&mut ids, 1).start);
        let logical = take(&mut ids, count as u64);
        let mut signing = signed.then(|| {
            self.io
                .signing
                .reserve_ids(attempts as u64)
                .expect("preflighted whole signing plan")
        });
        for (index, entry) in entries.iter_mut().enumerate() {
            entry.logical = logical.start + index as u64;
            entry.ids = take(&mut ids, 3 * u64::from(entry.terms.attempts));
            entry.signing = signing
                .as_mut()
                .map(|ids| take(ids, u64::from(entry.terms.attempts)));
        }
        // The execution identity and the founder's observer root leg.
        let moved = physical.then(|| (take(&mut ids, 1).start, take(&mut ids, 1).start));
        let handle = match moved {
            Some((identity, _)) => Plan::with_owner(
                preflight,
                Owner::Execution {
                    generation: identity,
                },
                deadline,
            ),
            None => Plan::new(preflight, deadline),
        };
        let founder_root = moved.map(|(_, leg)| {
            let mut root = Context::new(
                preflight.owner(),
                leg,
                operation,
                policy.credential.clone(),
                self.now,
                deadline,
                policy.pacing,
            )
            .and_then(|root| root.with_retry(policy.retry))
            .expect("validated with the Plan root above");
            root.bind_issuer(&self.io.request_contexts.issuer);
            root
        });
        body.push(Body {
            handle: handle.clone(),
            receipt_cap,
            policy,
            timer,
            schedule,
            terminals,
            entries,
            handoff: self.io.http_delivery.clone(),
            initial_custody: self.io.http_delivery.is_none() && self.io.http_plan_ready.is_none(),
        });
        if let Some(context) = old_read {
            self.io.retire_http_leg(&context);
            self.io.request_contexts.contexts.remove(&context.leg());
        }
        let Owner::Command { id, generation } = preflight.owner() else {
            unreachable!()
        };
        let registry = &mut self.io.request_contexts;
        let owner = registry.commands.get_mut(&id).expect("validated preflight");
        assert_eq!(owner.generation, generation);
        let execution = match (moved, founder_root) {
            (Some((identity, _)), Some(root)) => {
                let physical = CommandOwner {
                    generation: identity,
                    phase: CommandPhase::Planned(State { body }),
                    last_deadline: deadline,
                    writes,
                    mutation_declared: owner.mutation_declared,
                    credentialed: owner.credentialed,
                    pending: 0,
                    deliveries: 0,
                    waits: 0,
                    intent: owner.intent,
                    leg_intents,
                };
                // The founder keeps its own pending/delivery accounting and
                // becomes an observer: it holds no journal or send right.
                // A planned founder is never a tracked caller-wait root.
                owner.phase = CommandPhase::Context {
                    root: root.clone(),
                    entry_owned: true,
                    tracked: false,
                    expired: false,
                };
                owner.last_deadline = deadline;
                registry.contexts.insert(root.leg(), root.clone());
                registry.physical.insert(identity, physical);
                registry.plan_index.push(handle.owner());
                Some((identity, root))
            }
            _ => {
                owner.phase = CommandPhase::Planned(State { body });
                owner.writes = writes;
                owner.leg_intents = leg_intents;
                owner.last_deadline = deadline;
                registry.plan_index.push(preflight.owner());
                None
            }
        };
        registry.reserved_contexts = future;
        self.io.timers.insert(timer, ());
        self.rearm_plan(&handle);
        Ok((handle, execution))
    }

    /// Structural availability only; issuing a slot requires its exact Plan,
    /// original eligibility and one-use reserved physical admission.
    pub fn plan_slots(&self, plan: &Plan) -> Result<Vec<Slot>, Error> {
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(body.entries.len())
            .map_err(|_| Error::Capacity)?;
        for slot in body.schedule.available() {
            let index = body.schedule.ordinal(&slot)?;
            if body.entries[index]
                .eligible_at
                .is_some_and(|at| at <= self.now)
            {
                slots.push(slot);
            }
        }
        Ok(slots)
    }
}

impl<P> Cx<'_, P> {
    /// Consume one declared slot using only resources reserved at plan admission.
    pub fn plan_leg_context(&mut self, plan: &Plan, slot: &Slot) -> Result<Context, Error> {
        self.make_plan_leg_context(plan, slot, None)
    }

    /// Narrow a write leg using the common engine intent rules before its first
    /// journal registration. No method, endpoint or transport can infer intent.
    pub fn plan_leg_context_with_intent(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        intent: crate::classify::Intent,
    ) -> Result<Context, Error> {
        self.make_plan_leg_context(plan, slot, Some(intent))
    }

    fn make_plan_leg_context(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        intent: Option<crate::classify::Intent>,
    ) -> Result<Context, Error> {
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        let index = body.schedule.ordinal(slot)?;
        let entry = &body.entries[index];
        if entry.credit != Credit::Unissued
            || !body
                .schedule
                .available()
                .any(|candidate| body.schedule.ordinal(&candidate) == Ok(index))
            || !entry.eligible_at.is_some_and(|at| at <= self.now)
            || !(self
                .io
                .http_delivery
                .as_ref()
                .is_some_and(|delivery| body.handoff.as_ref() == Some(delivery))
                || (body.initial_custody
                    && self.io.http_delivery.is_none()
                    && self.io.http_plan_ready.is_none()
                    // A ready founder's own physical Plan counts as the ready owner.
                    && self.validate_preflight_ready_owner(plan.owner()).is_ok())
                || (self.io.http_delivery.is_none()
                    && self
                        .io
                        .http_plan_ready
                        .as_ref()
                        .is_some_and(|ready| ready.same_plan(plan))))
        {
            return Err(Error::Revoked);
        }
        let window = entry.terms.window(self.now, plan.deadline())?;
        let mut context = Context::new(
            plan.owner(),
            entry.logical,
            body.policy.operation,
            body.policy.credential.clone(),
            self.now,
            window.deadline(),
            body.policy.pacing,
        )?
        .with_retry(body.policy.retry)?;
        context.bind_issuer(&self.io.request_contexts.issuer);
        let mut call = Call {
            context: context.clone(),
            terms: entry.terms,
            window,
            ids: entry.ids.clone(),
            signing: entry.signing.clone(),
            selected: 0,
            admission: Admission {
                context: context.clone(),
                attempt: Attempt {
                    request: RequestId(0),
                    generation: 0,
                },
                timer: TimerId(0),
                signing: None,
                deadline: window.deadline(),
                predecessor: None,
            },
            stage: Stage::Ready,
        };
        call.select(self.now, self.io.http_delivery.clone())?;
        let owner = self
            .io
            .request_contexts
            .owner_entry_mut(plan.owner())
            .unwrap();
        let narrowed = match intent {
            Some(intent) if owner.prepare_leg_intent(&context, intent, false)? => Some(intent),
            _ => None,
        };
        // Journal enrollment precedes headers, recipe preparation and all queue
        // refusals. Narrowing is checked first, but committed only after the
        // fallible registration succeeds; no callback can intervene.
        if context.operation() == Operation::Mutation {
            owner
                .writes
                .as_mut()
                .expect("admitted write journal")
                .register(call.admission.attempt.request, context.leg())?;
        }
        if let Some(intent) = narrowed {
            assert!(
                owner.leg_intents.len() < owner.leg_intents.capacity(),
                "reserved Plan intent backing"
            );
            owner.leg_intents.push((context.leg(), intent));
        }
        let body = self.io.request_contexts.plan_body_mut(plan).unwrap();
        body.schedule.start(slot).expect("available plan slot");
        body.entries[index].notified = true;
        body.entries[index].credit = Credit::Issued;
        body.entries[index].call = Some(call);
        self.io.request_contexts.reserved_contexts -= 1;
        self.io
            .request_contexts
            .contexts
            .insert(context.leg(), context.clone());
        Ok(context)
    }

    /// Exact released DATA authority before any native or Store mutation.
    pub fn validate_plan_delivery(&self, plan: &Plan, context: &Context) -> Result<(), Error> {
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        self.io.request_contexts.validate(context, self.now)?;
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::Revoked)?;
        let call = body
            .entries
            .iter()
            .find_map(|entry| entry.call.as_ref().filter(|call| call.context == *context))
            .ok_or(Error::Revoked)?;
        if !call.matches(delivery) || !matches!(call.stage, Stage::Active { failed: false }) {
            return Err(Error::Revoked);
        }
        if self.now >= call.admission.deadline {
            return Err(Error::TimedOut);
        }
        if !self
            .io
            .managed_http
            .get(&delivery.attempt.request)
            .is_some_and(|managed| managed.released_delivery(delivery))
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// Settle a call after validating DATA and recording write facts. Structural
    /// progress grants no effect or successful command outcome by itself.
    pub fn finish_plan_call(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<(), Error> {
        self.validate_plan_delivery(plan, context)?;
        let body = self.io.request_contexts.plan_body(plan)?;
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        if context.operation() == Operation::Mutation {
            self.io
                .request_contexts
                .owner_entry(plan.owner())
                .unwrap()
                .writes
                .as_ref()
                .ok_or(Error::State)?
                .validate_housekeeping(call.admission.attempt.request)?;
        }
        self.io.retire_http_leg(context);
        self.io.request_contexts.contexts.remove(&context.leg());
        let body = self.io.request_contexts.plan_body_mut(plan)?;
        body.schedule.settle(slot)?;
        body.entries[index].call = None;
        body.entries[index].credit = Credit::Released;
        body.handoff = self.io.http_delivery.clone();
        body.refresh_availability(self.now);
        self.rearm_plan(plan);
        Ok(())
    }

    /// Select a declared Read retry without extending its owner or root. A
    /// write refusal must use the opaque classified resend-grant operation.
    pub fn plan_retry_context(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        evidence: RetryEvidence,
    ) -> Result<Context, Error> {
        if evidence == RetryEvidence::DefinitiveRefusal {
            return Err(Error::RetryForbidden);
        }
        context.retry(evidence, self.now)?;
        self.select_plan_retry(plan, slot, context)
    }

    /// Consume the existing classifier's exact-attempt write grant within the
    /// declared call window. The journal alone owns one-resend consumption.
    pub fn plan_resend_context(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        grant: ResendGrant,
    ) -> Result<Context, Error> {
        self.http_resend_context(context, grant)?;
        self.select_plan_retry(plan, slot, context)
    }

    fn select_plan_retry(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<Context, Error> {
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        self.io.request_contexts.validate(context, self.now)?;
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) || !matches!(call.stage, Stage::Active { .. }) {
            return Err(Error::Revoked);
        }
        if call.selected >= call.terms.attempts {
            return Err(Error::RetryForbidden);
        }
        call.window.physical_deadline(self.now)?;
        let next = RequestId(call.ids.start);
        let previous = delivery.attempt.request;
        let predecessor = delivery.clone();
        if context.operation() == Operation::Mutation {
            self.io
                .request_contexts
                .owner_entry_mut(plan.owner())
                .unwrap()
                .writes
                .as_mut()
                .ok_or(Error::State)?
                .retry(previous, next)?;
        }
        self.io.retire_http_leg(context);
        let body = self.io.request_contexts.plan_body_mut(plan)?;
        body.schedule
            .retry(slot)
            .expect("remaining declared credit");
        body.entries[index]
            .call
            .as_mut()
            .unwrap()
            .select(self.now, Some(predecessor))
            .expect("validated retry");
        Ok(context.clone())
    }

    /// Only a structurally complete plan may request success. Journal facts
    /// convert known-partial, refused or unresolved writes into terminal failure.
    pub fn done_plan(&mut self, plan: &Plan, result: String) -> Result<(), Error> {
        self.validate_preflight_ready_owner(plan.owner())?;
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        if !body.schedule.is_complete() {
            return Err(Error::State);
        }
        self.seal_plan(plan, Some(result), None)
    }

    /// Complete a Read from the current validated DATA call, retiring only
    /// unissued remainder. Call before `finish_plan_call`; another selected
    /// call, write plan, failed delivery or expired authority cannot use this.
    /// This grants no cache or Store publication eligibility.
    pub fn done_read_plan_after_data(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        result: String,
    ) -> Result<(), Error> {
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        if body.policy.operation != Operation::Read {
            return Err(Error::Invalid);
        }
        self.validate_plan_delivery(plan, context)?;
        let index = body.schedule.ordinal(slot)?;
        if !body.entries[index]
            .call
            .as_ref()
            .is_some_and(|call| call.context == *context)
        {
            return Err(Error::Revoked);
        }
        if body.entries.iter().enumerate().any(|(other, entry)| {
            other != index && (entry.call.is_some() || entry.credit == Credit::Issued)
        }) {
            return Err(Error::State);
        }
        // All recoverable refusals precede settlement. The existing DATA
        // settlement records exact handoff custody for the terminal operation.
        self.finish_plan_call(plan, slot, context)?;
        self.seal_plan(plan, Some(result), None)
    }

    /// Failure-only retirement; expired authority never creates fresh sends.
    pub fn fail_plan(&mut self, plan: &Plan, error: impl Into<VenueError>) -> Result<(), Error> {
        if self
            .io
            .http_plan_expiring
            .as_ref()
            .is_some_and(|current| current.same_plan(plan))
        {
            self.io.request_contexts.validate_plan_seal_origin(
                plan,
                self.now,
                self.io.http_delivery.as_ref(),
            )?;
            return Ok(());
        }
        self.seal_plan(plan, None, Some(error.into()))
    }

    fn seal_plan(
        &mut self,
        plan: &Plan,
        result: Option<String>,
        error: Option<VenueError>,
    ) -> Result<(), Error> {
        let prepared = self.prepare_plan_terminal(plan, error, result.as_deref())?;
        self.commit_plan_terminal(plan, result, prepared)
    }

    fn prepare_plan_terminal(
        &mut self,
        plan: &Plan,
        error: Option<VenueError>,
        result: Option<&str>,
    ) -> Result<conditional::PreparedPlanSeal, Error> {
        // Ready-loop custody first: no effect before the owner is confirmed.
        self.validate_preflight_ready_owner(plan.owner())?;
        let physical = matches!(plan.owner(), Owner::Execution { .. });
        // Observers already past their own bound expire against the live
        // journal before any terminal preparation (host callbacks seal first).
        if physical {
            self.sweep_executions();
        }
        let mut prepared = self.io.request_contexts.prepare_plan_seal(
            plan,
            self.now,
            self.io.http_delivery.as_ref(),
            error,
        )?;
        if physical {
            // Every observer's terminal and reports, cloned before the seal.
            let projection = match (&prepared.journal, &prepared.legacy) {
                (Some(journal), _) => journal
                    .failure()
                    .cloned()
                    .map_or_else(|| Ok(result.unwrap_or_default().to_owned()), Err),
                (None, Some(error)) => Err(error.clone()),
                (None, None) => Ok(result.unwrap_or_default().to_owned()),
            };
            let observers = self.io.request_contexts.planned_observer_count(plan);
            prepared
                .fanout
                .try_reserve_exact(observers)
                .map_err(|_| Error::Capacity)?;
            prepared
                .sealed
                .try_reserve_exact(observers)
                .map_err(|_| Error::Capacity)?;
            // Each observer's reports and terminal notice, plus the
            // execution timer rearm (clear and set).
            let notices = observers
                .checked_mul(prepared.reports.len() + 1)
                .and_then(|n| n.checked_add(2))
                .ok_or(Error::Capacity)?;
            // One reservation covers the notices and the owner's cleanup.
            prepared.retired = self.io.prepare_owner_retirement(plan.owner(), notices)?;
            for _ in 0..observers {
                let mut reports = Vec::new();
                if projection.is_err() {
                    reports
                        .try_reserve_exact(prepared.reports.len())
                        .map_err(|_| Error::Capacity)?;
                    reports.extend(prepared.reports.iter().cloned());
                }
                prepared.fanout.push((projection.clone(), reports));
            }
        }
        Ok(prepared)
    }

    // One Cx-level terminal funnel, including conditional completion. Shared
    // execution observer projection hooks attach here, never re-run a journal.
    fn commit_plan_terminal(
        &mut self,
        plan: &Plan,
        result: Option<String>,
        mut prepared: conditional::PreparedPlanSeal,
    ) -> Result<(), Error> {
        let fanout = std::mem::take(&mut prepared.fanout);
        let sealed = std::mem::take(&mut prepared.sealed);
        let retired = std::mem::take(&mut prepared.retired);
        let (owner, seal) = self.io.request_contexts.commit_plan_seal(plan, prepared);
        let Owner::Command { id, .. } = owner else {
            // The physical owner has no command: its observers take the
            // terminals and storage prepared above, moved without allocation.
            self.finish_physical_plan(owner, retired);
            self.seal_planned_execution(plan, fanout, sealed);
            return Ok(());
        };
        self.finish_sealed_command(id, result, seal)
    }

    fn rearm_plan(&mut self, plan: &Plan) {
        let body = self
            .io
            .request_contexts
            .plan_body(plan)
            .expect("live plan timer");
        let timer = body.timer;
        let at_ms = body.next_wake();
        self.io.actions.retain(|action| !matches!(action, Action::SetTimer { timer: previous, .. } if *previous == timer));
        self.io.actions.push(Action::SetTimer { timer, at_ms });
    }
}

impl Body {
    fn refresh_availability(&mut self, now: f64) {
        for slot in self.schedule.available() {
            let index = self.schedule.ordinal(&slot).expect("own available slot");
            let entry = &mut self.entries[index];
            if entry.eligible_at.is_none() {
                entry.eligible_at = Some(eligibility(
                    now,
                    entry.terms.wait_ms,
                    self.handle.deadline(),
                ));
                entry.notified = false;
            }
        }
    }
    fn next_wake(&self) -> f64 {
        self.entries
            .iter()
            .filter(|entry| entry.credit == Credit::Unissued && !entry.notified)
            .filter_map(|entry| entry.eligible_at)
            .fold(self.handle.deadline(), f64::min)
    }
}

impl ContextRegistry {
    pub(in super::super) fn plan_timer(&self, command: CommandId) -> Option<TimerId> {
        let CommandPhase::Planned(state) = &self.commands.get(&command)?.phase else {
            return None;
        };
        Some(state.timer())
    }

    pub(in super::super) fn physical_plan_timer(&self, owner: Owner) -> Option<TimerId> {
        if !self.physical_owner(owner) {
            return None;
        }
        let CommandPhase::Planned(state) = &self.owner_entry(owner)?.phase else {
            return None;
        };
        Some(state.timer())
    }
}

impl<P> Cx<'_, P> {
    pub(in super::super) fn finish_failed_plan_call(
        &mut self,
        delivery: &Delivery,
        error: &mut VenueError,
    ) -> bool {
        let Some(plan) = self
            .io
            .request_contexts
            .plan_from_context(&delivery.context)
        else {
            return false;
        };
        let body = self.io.request_contexts.plan_body(&plan).unwrap();
        let Some(call) = body
            .entries
            .iter()
            .find_map(|entry| entry.call.as_ref().filter(|call| call.matches(delivery)))
        else {
            return false;
        };
        if !matches!(call.stage, Stage::Active { failed: true }) {
            return false;
        }
        if self.now >= call.admission.deadline {
            error.kind = Some(crate::ErrorKind::Timeout)
        }
        self.fail_plan(&plan, error.clone())
            .expect("exact failed plan custody");
        true
    }
}

impl<P> Cx<'_, P> {
    /// Retire one failed physical call while continuing independently declared
    /// work. This cannot publish effects, erase evidence or authorize a retry.
    pub fn finish_plan_failed_call(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<(), Error> {
        if context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context.same_issuer(context) && call.context == *context)
            .ok_or(Error::Revoked)?;
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) || !matches!(call.stage, Stage::Active { failed: true }) {
            return Err(Error::Revoked);
        }
        if context.operation() == Operation::Mutation {
            self.io
                .request_contexts
                .owner_entry(plan.owner())
                .unwrap()
                .writes
                .as_ref()
                .ok_or(Error::State)?
                .validate_failed_attempt(delivery.attempt.request)?;
        }
        self.io.retire_http_leg(context);
        self.io.request_contexts.contexts.remove(&context.leg());
        let body = self.io.request_contexts.plan_body_mut(plan)?;
        body.schedule.settle(slot)?;
        body.entries[index].call = None;
        body.entries[index].credit = Credit::Released;
        body.handoff = self.io.http_delivery.clone();
        body.refresh_availability(self.now);
        self.rearm_plan(plan);
        Ok(())
    }
}

impl ContextRegistry {
    pub(in super::super) fn has_plans(&self) -> bool {
        !self.plan_index.is_empty()
    }

    pub(in super::super) fn end_plan_admission(&mut self) {
        for index in 0..self.plan_index.len() {
            #[cfg(any(test, feature = "test-support"))]
            self.plan_scan_visits.set(self.plan_scan_visits.get() + 1);
            let owner = self
                .owner_entry_mut(self.plan_index[index])
                .expect("indexed Plan owner");
            if let CommandPhase::Planned(state) = &mut owner.phase {
                state.body[0].initial_custody = false;
            }
        }
    }

    pub(in super::super) fn plan_scope_clear(&self) -> bool {
        self.delivery_depth == 0
    }

    pub(in super::super) fn next_plan_ready(&mut self, now: f64) -> Option<Plan> {
        for index in 0..self.plan_index.len() {
            #[cfg(any(test, feature = "test-support"))]
            self.plan_scan_visits.set(self.plan_scan_visits.get() + 1);
            let owner = self
                .owner_entry_mut(self.plan_index[index])
                .expect("indexed Plan owner");
            let CommandPhase::Planned(state) = &mut owner.phase else {
                continue;
            };
            let body = &mut state.body[0];
            if now >= body.handle.deadline() {
                return Some(body.handle.clone());
            }
            let mut due = false;
            for entry in &mut body.entries {
                if entry.credit == Credit::Unissued
                    && !entry.notified
                    && entry.eligible_at.is_some_and(|at| at <= now)
                {
                    entry.notified = true;
                    due = true;
                }
            }
            if due {
                return Some(body.handle.clone());
            }
        }
        None
    }

    pub(in super::super) fn plan_for_timer(&self, timer: TimerId) -> Option<Plan> {
        self.plan_index.iter().find_map(|plan_owner| {
            let owner = self.owner_entry(*plan_owner).expect("indexed Plan owner");
            #[cfg(any(test, feature = "test-support"))]
            self.plan_scan_visits.set(self.plan_scan_visits.get() + 1);
            let CommandPhase::Planned(state) = &owner.phase else {
                return None;
            };
            let body = &state.body[0];
            (body.timer == timer).then(|| body.handle.clone())
        })
    }

    pub(in super::super) fn rearm_live_plan<P>(io: &mut super::super::Io<P>, plan: &Plan) {
        if let Ok(body) = io.request_contexts.plan_body(plan) {
            let (timer, at_ms) = (body.timer, body.next_wake());
            io.actions.retain(|action| !matches!(action, Action::SetTimer { timer: previous, .. } if *previous == timer));
            io.actions.push(Action::SetTimer { timer, at_ms });
        }
    }
}

// The root is finite and later than now. A tiny positive wait at a wide raw
// clock rounds up, never backwards or into a fallible post-settlement operation.
fn eligibility(now: f64, wait_ms: u32, root: f64) -> f64 {
    if wait_ms == 0 {
        return now;
    }
    let span = f64::from(wait_ms);
    let mut at = now + span;
    if at <= now {
        at = now.next_up()
    } else {
        let virtual_span = at - now;
        let residual = (now - (at - virtual_span)) + (span - virtual_span);
        if residual > 0. {
            at = at.next_up()
        }
    }
    at.min(root)
}

impl ContextRegistry {
    pub(in super::super) fn failed_plan_delivery(&self, delivery: &Delivery) -> bool {
        self.plan_from_context(&delivery.context)
            .and_then(|plan| self.plan_body(&plan).ok())
            .is_some_and(|body| {
                body.entries.iter().any(|entry| {
                    entry.call.as_ref().is_some_and(|call| {
                        call.matches(delivery)
                            && matches!(call.stage, Stage::Active { failed: true })
                    })
                })
            })
    }
    #[cfg(test)]
    pub(in super::super) fn plan_context_storage(&self) -> (usize, usize, usize) {
        (
            self.contexts.len(),
            self.contexts.capacity(),
            self.reserved_contexts,
        )
    }
}

impl<P> Cx<'_, P> {
    /// Select a predeclared replacement sequence only after the existing journal
    /// proves an effects-free refusal. All descendants were reserved on admission.
    pub fn replace_plan_call(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<(), Error> {
        if context.operation() != Operation::Mutation {
            return Err(Error::Invalid);
        }
        self.validate_plan_delivery(plan, context)?;
        context.retry(RetryEvidence::DefinitiveRefusal, self.now)?;
        let body = self.io.request_contexts.plan_body(plan)?;
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        let slots = body.schedule.replacement_slots(slot)?;
        let mut children = Vec::new();
        children
            .try_reserve_exact(slots.len())
            .map_err(|_| Error::Capacity)?;
        for slot in &slots {
            children.push(body.entries[body.schedule.ordinal(slot)?].logical);
        }
        let previous = call.admission.attempt.request;
        self.io
            .request_contexts
            .owner_entry_mut(plan.owner())
            .unwrap()
            .writes
            .as_mut()
            .ok_or(Error::State)?
            .replace(previous, &children)?;
        self.io.retire_http_leg(context);
        self.io.request_contexts.contexts.remove(&context.leg());
        let body = self.io.request_contexts.plan_body_mut(plan)?;
        body.schedule
            .replace(slot)
            .expect("validated declared replacement");
        body.entries[index].call = None;
        body.entries[index].credit = Credit::Released;
        body.handoff = self.io.http_delivery.clone();
        body.refresh_availability(self.now);
        self.rearm_plan(plan);
        Ok(())
    }

    /// Explicit empty native plan. No root, Context, physical attempt or accepted
    /// write fact is manufactured for a successful no-op.
    pub fn done_empty_plan(&mut self, preflight: &Preflight, result: String) -> Result<(), Error> {
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        let (_, old_read) = self.preflight_plan_origin(preflight)?;
        let Owner::Command { id, .. } = preflight.owner() else {
            return Err(Error::Invalid);
        };
        if let Some(context) = old_read {
            self.io.retire_http_leg(&context);
            self.io.request_contexts.contexts.remove(&context.leg());
        }
        self.finish_sealed_command(id, Some(result), Seal::Success)
    }
}
