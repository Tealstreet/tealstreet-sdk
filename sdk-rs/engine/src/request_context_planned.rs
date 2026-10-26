//! Immutable native-plan admission and resources under one command generation.
use super::owned_call::{Admission, Call, Stage};
use super::*;
use crate::request_context::plan::{CallTerms, Schedule, Slot, StepBound};
use crate::{Action, TimerId};
#[path = "request_context_conditional.rs"]
mod conditional;
use conditional::{AdmissionDeclaration, terminal_ordinals};
#[cfg(feature = "http-mutation-barriers")]
#[path = "request_context_group_plan.rs"]
mod mutation_group;
#[cfg(feature = "http-mutation-barriers")]
pub(in crate::engine) use mutation_group::MutationGroupKey;
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
    #[cfg(feature = "http-mutation-barriers")]
    barriers: Vec<Range<usize>>,
    entries: Vec<Entry>,
    handoff: Option<Delivery>,
    initial_custody: bool,
    history_bound: bool,
    history_latest_release: Option<Attempt>,
    #[cfg(feature = "serial-plans")]
    serial: Option<Box<serial::Serial>>,
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
    /// Reserved behind a login Session; issued only at its resume.
    parked: bool,
}

// Preparation grants no authority; consume errors in the original admission
// order so stale ownership still outranks an invalid declaration.
#[cfg(feature = "serial-plans")]
struct PreparedAdmission {
    policy: PlanPolicy,
    schedule: Result<Schedule, Error>,
    terminals: Result<Vec<usize>, Error>,
    has_barriers: bool,
    #[cfg(feature = "http-mutation-barriers")]
    barriers: Result<Vec<Range<usize>>, Error>,
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
    #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
    pub(super) fn session_root(&self) -> Option<&crate::request_context::Session> {
        self.body.first()?.serial.as_ref()?.session_root.as_ref()
    }
    /// A Mutation Plan's admitted credential, for repair mint custody.
    #[cfg(feature = "repair-mutations")]
    pub(in crate::engine) fn repair_write_credential(
        &self,
    ) -> Option<&crate::request_context::Credential> {
        let policy = &self.body.first()?.policy;
        (policy.operation == Operation::Mutation)
            .then_some(policy.credential.as_ref())
            .flatten()
    }

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
        note: Option<&str>,
    ) -> Result<conditional::PreparedPlanSeal, Error> {
        self.validate_plan_seal_origin(plan, now, delivery)?;
        let owner = self.owner_entry(plan.owner()).expect("live Plan owner");
        let mut reports = Vec::new();
        let (journal, legacy) = if let Some(writes) = owner.writes.as_ref() {
            let prepared = writes.prepare_seal(error, note)?;
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
            let error = error.map(|mut error| {
                if let Some(note) = note {
                    crate::request_context::note_unprojected(&mut error, note);
                }
                error
            });
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

    pub(in super::super) fn independent_plan_failure(
        &self,
        delivery: &Delivery,
        fallback: VenueError,
    ) -> Option<VenueError> {
        let plan = self.plan_from_context(&delivery.context)?;
        let body = self.plan_body(&plan).ok()?;
        if !body.policy.independent_calls || body.policy.operation != Operation::Mutation {
            return None;
        }
        Some(
            self.owner_entry(plan.owner())
                .and_then(|owner| owner.writes.as_ref())
                .and_then(|writes| writes.failed_attempt_error(delivery.attempt.request).ok())
                .unwrap_or(fallback),
        )
    }

    fn validate_unbound_plan(&self, plan: &Plan) -> Result<(), Error> {
        if self.plan_body(plan)?.history_bound {
            return Err(Error::State);
        }
        Ok(())
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
                barriers: &[],
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
                barriers: &[],
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
        // Before any ID, reservation or context: an undeclared venue must not
        // admit a group its compiled-out paths would never pump.
        #[cfg(feature = "http-mutation-barriers")]
        if !declaration.barriers.is_empty() && !self.io.core.mutation_barriers_declared {
            return Err(Error::Invalid);
        }
        #[cfg(feature = "serial-plans")]
        {
            let AdmissionDeclaration {
                steps,
                policy,
                terminals,
                barriers,
            } = declaration;
            // Effective domain pacing first, so barrier validation sees it.
            #[cfg(feature = "release-domains")]
            let policy = PlanPolicy {
                pacing: self
                    .io
                    .core
                    .domain_pacing(policy.operation, policy.pacing)?,
                ..policy
            };
            // An atomic group releases together, so it cannot take one FIFO
            // turn per member; refused before any Plan ID is spent.
            #[cfg(all(feature = "release-domains", feature = "http-mutation-barriers"))]
            if !barriers.is_empty() && self.io.core.domain_member(policy.operation).is_some() {
                return Err(Error::Invalid);
            }
            let prepared = PreparedAdmission {
                schedule: Schedule::from_steps(steps),
                terminals: terminal_ordinals(steps, &policy, terminals),
                has_barriers: !barriers.is_empty(),
                #[cfg(feature = "http-mutation-barriers")]
                barriers: mutation_group::normalize(
                    steps,
                    &policy,
                    barriers,
                    &self.io.core.http_pacing,
                ),
                policy,
            };
            self.admit_plan_root(preflight, prepared, physical, None)
        }
        #[cfg(not(feature = "serial-plans"))]
        self.admit_plan_root(preflight, declaration, physical)
    }

    /// A serial Plan shares the whole declared admission transaction, with its
    /// own unbounded root and streamed journal.
    fn admit_plan_root(
        &mut self,
        preflight: &Preflight,
        #[cfg(feature = "serial-plans")] prepared: PreparedAdmission,
        #[cfg(not(feature = "serial-plans"))] declaration: AdmissionDeclaration<'_, '_, '_>,
        physical: bool,
        #[cfg(feature = "serial-plans")] serial: Option<serial::Serial>,
    ) -> Result<(Plan, Option<(u64, Context)>), Error> {
        let extra = if physical { 2 } else { 0 };
        #[cfg(feature = "serial-plans")]
        let PreparedAdmission {
            policy,
            schedule,
            terminals,
            has_barriers,
            #[cfg(feature = "http-mutation-barriers")]
            barriers,
        } = prepared;
        #[cfg(not(feature = "serial-plans"))]
        let AdmissionDeclaration {
            steps,
            policy,
            terminals,
            barriers,
        } = declaration;
        #[cfg(not(feature = "serial-plans"))]
        let has_barriers = !barriers.is_empty();
        if (!cfg!(feature = "http-mutation-barriers") || physical || self.io.core.nonce_unscheduled)
            && has_barriers
        {
            return Err(Error::Invalid);
        }
        let (intent, old_read) = self.preflight_plan_origin(preflight)?;
        #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
        if self
            .io
            .core
            .request_contexts
            .keyless_session(preflight.owner())
            .is_some()
            && serial
                .as_ref()
                .is_none_or(|serial| serial.session_root.is_none())
        {
            return Err(Error::Invalid);
        }
        let (operation, credential) = match intent.purpose {
            PreflightPurpose::ReadOnly => (Operation::Read, intent.read_credential),
            PreflightPurpose::WriteAfterReads => (Operation::Mutation, intent.write_credential),
        };
        if (policy.independent_calls && operation != Operation::Mutation)
            || policy.operation != operation
            || policy.credential != credential
        {
            return Err(Error::Invalid);
        }
        let receipt_cap = self
            .io
            .core
            .request_contexts
            .preflight_receipt_cap(preflight)?;
        #[cfg(not(feature = "write-preflight-wait"))]
        if receipt_cap.is_some() && operation != Operation::Read {
            return Err(Error::Invalid);
        }
        if receipt_cap.is_some_and(|(_, cap)| !self.now.is_finite() || cap - self.now < 1.) {
            return Err(Error::TimedOut);
        }
        // A write Plan never inherits the caller cap: it bounded the Preflight only.
        #[cfg(feature = "write-preflight-wait")]
        let receipt_cap = receipt_cap.filter(|_| operation == Operation::Read);
        #[cfg(feature = "serial-plans")]
        let schedule = schedule?;
        #[cfg(not(feature = "serial-plans"))]
        let schedule = Schedule::from_steps(steps)?;
        #[cfg(feature = "serial-plans")]
        let terminals = terminals?;
        #[cfg(not(feature = "serial-plans"))]
        let terminals = terminal_ordinals(steps, &policy, terminals)?;
        #[cfg(all(feature = "http-mutation-barriers", feature = "serial-plans"))]
        let barriers = barriers?;
        #[cfg(all(feature = "http-mutation-barriers", not(feature = "serial-plans")))]
        let barriers =
            mutation_group::normalize(steps, &policy, barriers, &self.io.core.http_pacing)?;
        let bound = schedule.bound();
        let native_deadline = bound.deadline(self.now)?;
        #[cfg(feature = "serial-plans")]
        let deadline = if serial.is_some() {
            serial::ROOT
        } else {
            receipt_cap.map_or(native_deadline, |(_, cap)| native_deadline.min(cap))
        };
        #[cfg(not(feature = "serial-plans"))]
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
        #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
        let session_read = serial
            .as_ref()
            .is_some_and(serial::Serial::allows_session_read_resend);
        #[cfg(not(all(feature = "serial-plans", feature = "session-bearer")))]
        let session_read = false;
        if policy.retry == crate::request_context::RetryPolicy::Never
            && !session_read
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
                parked: false,
            });
        }
        for slot in schedule.available() {
            let index = schedule.ordinal(&slot)?;
            let at = eligibility(self.now, entries[index].terms.wait_ms, deadline);
            entries[index].eligible_at = Some(at);
            entries[index].notified = false;
        }
        #[cfg(feature = "serial-plans")]
        let writes = match &serial {
            Some(serial) => Some(
                CommandWrites::new_reserved(serial::JOURNAL_ATTEMPTS, count)?
                    .streamed_with(serial.refusals_complete),
            ),
            None if operation == Operation::Mutation => {
                Some(CommandWrites::new_reserved(attempts, count)?)
            }
            None => None,
        };
        #[cfg(not(feature = "serial-plans"))]
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
        self.io.core.request_contexts.prune(self.now);
        let old_slot = old_read.as_ref().is_some_and(|context| {
            self.io
                .core
                .request_contexts
                .contexts
                .contains_key(&context.leg())
        }) as usize;
        let future = self
            .io
            .core
            .request_contexts
            .reserved_contexts
            .checked_add(count)
            .ok_or(Error::Capacity)?;
        if self
            .io
            .core
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
            .core
            .request_contexts
            .contexts
            .try_reserve(future + extra)
            .map_err(|_| Error::Capacity)?;
        self.io
            .core
            .timers
            .try_reserve(1)
            .map_err(|_| Error::Capacity)?;
        self.io
            .core
            .request_contexts
            .plan_index
            .try_reserve(1)
            .map_err(|_| Error::Capacity)?;
        let io_count = 1 + count as u64 + 3 * attempts as u64 + extra as u64;
        let signed = policy.credential.is_some();
        if !self.io.core.next_id.available(io_count)
            || (signed && !self.io.core.signing.available_ids(attempts as u64))
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
                .core
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
            root.bind_issuer(&self.io.core.request_contexts.issuer);
            root
        });
        body.push(Body {
            handle: handle.clone(),
            receipt_cap,
            policy,
            timer,
            schedule,
            terminals,
            #[cfg(feature = "http-mutation-barriers")]
            barriers,
            entries,
            handoff: self.io.core.http_delivery.clone(),
            initial_custody: self.io.core.http_delivery.is_none()
                && self.io.core.http_plan_ready.is_none(),
            history_bound: false,
            history_latest_release: None,
            #[cfg(feature = "serial-plans")]
            serial: serial.map(Box::new),
        });
        if let Some(context) = old_read {
            self.io.retire_http_leg(&context);
            self.io
                .core
                .request_contexts
                .contexts
                .remove(&context.leg());
        }
        let Owner::Command { id, generation } = preflight.owner() else {
            unreachable!()
        };
        let registry = &mut self.io.core.request_contexts;
        let owner = registry.commands.get_mut(&id).expect("validated preflight");
        assert_eq!(owner.generation, generation);
        let execution = match (moved, founder_root) {
            (Some((identity, _)), Some(root)) => {
                let physical = CommandOwner {
                    generation: identity,
                    phase: CommandPhase::Planned(State { body }),
                    last_deadline: deadline,
                    // The founder's inherited fact moves with its journal, so
                    // every observer's projection keeps it.
                    writes: writes.map(|writes| writes.inheriting(owner.inherited_uncertain)),
                    mutation_declared: owner.mutation_declared,
                    credentialed: owner.credentialed,
                    pending: 0,
                    deliveries: 0,
                    waits: 0,
                    intent: owner.intent,
                    leg_intents,
                    inherited_uncertain: owner.inherited_uncertain,
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
                owner.writes = writes.map(|writes| writes.inheriting(owner.inherited_uncertain));
                owner.leg_intents = leg_intents;
                owner.last_deadline = deadline;
                registry.plan_index.push(preflight.owner());
                None
            }
        };
        registry.reserved_contexts = future;
        self.io.core.timers.insert(timer, ());
        self.rearm_plan(&handle);
        Ok((handle, execution))
    }

    #[inline]
    pub fn plan_slots(&self, plan: &Plan) -> Result<Vec<Slot>, Error> {
        self.core_ref().plan_slots(plan)
    }
}

impl crate::engine::CoreCx<'_> {
    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    #[inline]
    pub fn plan_slots(&self, plan: &Plan) -> Result<Vec<Slot>, Error> {
        self.core_ref().plan_slots(plan)
    }
}

impl crate::engine::CoreRef<'_> {
    /// Structural availability only; issuing a slot requires its exact Plan,
    /// original eligibility and one-use reserved physical admission.
    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    pub fn plan_slots(&self, plan: &Plan) -> Result<Vec<Slot>, Error> {
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(body.entries.len())
            .map_err(|_| Error::Capacity)?;
        for slot in body.schedule.available() {
            let index = body.schedule.ordinal(&slot)?;
            let entry = &body.entries[index];
            if !entry.parked && entry.eligible_at.is_some_and(|at| at <= self.now) {
                slots.push(slot);
            }
        }
        Ok(slots)
    }
}

impl<P> Cx<'_, P> {
    /// Consume one declared slot using only resources reserved at plan admission.
    /// Park an unissued, eligible Slot on a login Session before it consumes an
    /// attempt. `session_ready` issues its leg with a fresh window under the
    /// same Plan root and Slot; nothing is spent while parked.
    /// Known gap: the parked unit keeps only `intent`, so a protection-cancel leg
    /// resumes as a plain cancel. The first venue to combine a Plan session wait with
    /// protection-cancel legs must carry that flag through the parked unit, RED first.
    pub fn plan_session_wait(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        intent: Option<crate::classify::Intent>,
        session: &crate::request_context::Session,
        pending: P,
    ) -> Result<(), (P, Error)> {
        let checked = (|| {
            let generation = self.io.core.request_contexts.validate_session(session)?;
            if !matches!(plan.owner(), Owner::Command { .. }) {
                return Err(Error::State);
            }
            let body = self
                .io
                .core
                .request_contexts
                .validate_plan(plan, self.now)?;
            #[cfg(feature = "serial-plans")]
            if body.serial.is_some() {
                return Err(Error::State);
            }
            let index = body.schedule.ordinal(slot)?;
            let entry = &body.entries[index];
            if entry.parked
                || entry.credit != Credit::Unissued
                || !body
                    .schedule
                    .available()
                    .any(|candidate| body.schedule.ordinal(&candidate) == Ok(index))
                || !entry.eligible_at.is_some_and(|at| at <= self.now)
                || !self.plan_issue_custody(plan, body)
            {
                return Err(Error::Revoked);
            }
            if self.io.session_waits.len() >= super::CAPACITY {
                return Err(Error::Capacity);
            }
            self.io.session_waits.reserve_ended()?;
            let parked = self.io.session_waits.parked_units();
            self.io
                .core
                .session_resumed
                .try_reserve(parked + 1)
                .map_err(|_| Error::Capacity)?;
            Ok((generation, index))
        })();
        let (generation, index) = match checked {
            Ok(checked) => checked,
            Err(error) => return Err((pending, error)),
        };
        self.io
            .core
            .request_contexts
            .plan_body_mut(plan)
            .expect("validated parked Plan")
            .entries[index]
            .parked = true;
        self.io
            .session_waits
            .park_slot(generation, plan.clone(), slot.clone(), intent, pending);
        Ok(())
    }

    /// Park a failed Read call's declared retry on a login Session, in the
    /// failed delivery's custody, without selecting the next attempt. The
    /// resume selects it under the existing evidence, budget and window.
    pub fn plan_retry_session_wait(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        evidence: RetryEvidence,
        session: &crate::request_context::Session,
        pending: P,
    ) -> Result<(), (P, Error)> {
        let checked = (|| {
            let generation = self.io.core.request_contexts.validate_session(session)?;
            if !matches!(plan.owner(), Owner::Command { .. }) {
                return Err(Error::State);
            }
            // Only a Read retry; a write keeps its classified resend grant.
            if evidence == RetryEvidence::DefinitiveRefusal
                || context.operation() != Operation::Read
            {
                return Err(Error::RetryForbidden);
            }
            context.retry(evidence, self.now)?;
            let body = self
                .io
                .core
                .request_contexts
                .validate_plan(plan, self.now)?;
            #[cfg(feature = "serial-plans")]
            if body.serial.is_some() {
                return Err(Error::State);
            }
            self.io.core.request_contexts.validate(context, self.now)?;
            let index = body.schedule.ordinal(slot)?;
            let entry = &body.entries[index];
            let call = entry
                .call
                .as_ref()
                .filter(|call| call.context == *context)
                .ok_or(Error::Revoked)?;
            // The failed delivery's own callback, or a re-park of an ended
            // retry from the Plan's ready custody.
            let delivered = self.io.core.http_delivery.as_ref().is_some_and(|delivery| {
                call.matches(delivery) && matches!(call.stage, Stage::Active { .. })
            });
            let reparked =
                matches!(call.stage, Stage::Parked) && self.plan_issue_custody(plan, body);
            if entry.parked || !(delivered || reparked) {
                return Err(Error::Revoked);
            }
            if call.selected >= call.terms.attempts {
                return Err(Error::RetryForbidden);
            }
            call.window.physical_deadline(self.now)?;
            if self.io.session_waits.len() >= super::CAPACITY {
                return Err(Error::Capacity);
            }
            self.io.session_waits.reserve_ended()?;
            let parked = self.io.session_waits.parked_units();
            self.io
                .core
                .session_resumed
                .try_reserve(parked + 1)
                .map_err(|_| Error::Capacity)?;
            Ok((generation, index))
        })();
        let (generation, index) = match checked {
            Ok(checked) => checked,
            Err(error) => return Err((pending, error)),
        };
        self.io.retire_http_leg(context);
        let entry = &mut self
            .io
            .core
            .request_contexts
            .plan_body_mut(plan)
            .expect("validated parked Plan")
            .entries[index];
        entry.parked = true;
        entry.call.as_mut().expect("validated call").stage = Stage::Parked;
        self.io.session_waits.park_retry(
            generation,
            plan.clone(),
            slot.clone(),
            context.clone(),
            pending,
        );
        Ok(())
    }

    /// Select a parked retry's next attempt at its Session's resume, under the
    /// drain's delivery, inside the existing budget and declared window.
    pub(in crate::engine) fn resume_plan_retry(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<Context, Error> {
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        self.io.core.request_contexts.validate(context, self.now)?;
        let index = body.schedule.ordinal(slot)?;
        let entry = &body.entries[index];
        let call = entry
            .call
            .as_ref()
            .filter(|call| call.context == *context && matches!(call.stage, Stage::Parked))
            .ok_or(Error::Revoked)?;
        if !entry.parked {
            return Err(Error::Revoked);
        }
        if call.selected >= call.terms.attempts {
            return Err(Error::RetryForbidden);
        }
        call.window.physical_deadline(self.now)?;
        let predecessor = self.io.core.http_delivery.clone();
        let body = self.io.core.request_contexts.plan_body_mut(plan)?;
        body.schedule
            .retry(slot)
            .expect("remaining declared credit");
        let entry = &mut body.entries[index];
        entry.parked = false;
        entry
            .call
            .as_mut()
            .expect("validated call")
            .select(self.now, predecessor)
            .expect("validated retry");
        self.note_session_resume(plan, context);
        Ok(context.clone())
    }

    /// Issue a parked Slot's leg at its Session's resume, under the drain's
    /// delivery, with a window that starts now.
    pub(in crate::engine) fn resume_plan_slot(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        intent: Option<crate::classify::Intent>,
    ) -> Result<Context, Error> {
        let leg = self.make_plan_leg_context(plan, slot, intent, false, true)?;
        self.note_session_resume(plan, &leg);
        Ok(leg)
    }

    #[inline]
    fn note_session_resume(&mut self, plan: &Plan, leg: &Context) {
        self.core().note_session_resume(plan, leg)
    }

    pub fn plan_leg_context(&mut self, plan: &Plan, slot: &Slot) -> Result<Context, Error> {
        self.make_plan_leg_context(plan, slot, None, false, false)
    }

    /// Narrow a write leg using the common engine intent rules before its first
    /// journal registration. No method, endpoint or transport can infer intent.
    pub fn plan_leg_context_with_intent(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        intent: crate::classify::Intent,
    ) -> Result<Context, Error> {
        self.make_plan_leg_context(plan, slot, Some(intent), false, false)
    }

    /// The Plan form of `http_write_protection_cancel_leg`: an amend's cancel
    /// leg of a position-bound cancel-then-recreate, declared before the leg's
    /// journal registration. Never declare it for any other replacement.
    pub fn plan_protection_cancel_leg_context(
        &mut self,
        plan: &Plan,
        slot: &Slot,
    ) -> Result<Context, Error> {
        self.make_plan_leg_context(
            plan,
            slot,
            Some(crate::classify::Intent::Cancel),
            true,
            false,
        )
    }

    /// Handoff, initial or ready custody over this Plan's next issue.
    fn plan_issue_custody(&self, plan: &Plan, body: &Body) -> bool {
        self.io
            .core
            .http_delivery
            .as_ref()
            .is_some_and(|delivery| body.handoff.as_ref() == Some(delivery))
            || (body.initial_custody
                && self.io.core.http_delivery.is_none()
                && self.io.core.http_plan_ready.is_none()
                // A ready founder's own physical Plan counts as the ready owner.
                && self.validate_preflight_ready_owner(plan.owner()).is_ok())
            || (self.io.core.http_delivery.is_none()
                && self
                    .io
                    .core
                    .http_plan_ready
                    .as_ref()
                    .is_some_and(|ready| ready.same_plan(plan)))
    }

    /// A parked Slot's resume stands in for Plan custody; nothing else may
    /// issue a parked Slot.
    fn make_plan_leg_context(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        intent: Option<crate::classify::Intent>,
        protection_cancel: bool,
        resumed: bool,
    ) -> Result<Context, Error> {
        #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
        let session_egress = self
            .io
            .core
            .request_contexts
            .keyless_session_egress(plan.owner())?;
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        let index = body.schedule.ordinal(slot)?;
        let entry = &body.entries[index];
        if entry.credit != Credit::Unissued
            || entry.parked != resumed
            || !body
                .schedule
                .available()
                .any(|candidate| body.schedule.ordinal(&candidate) == Ok(index))
            || !entry.eligible_at.is_some_and(|at| at <= self.now)
            || !(resumed || self.plan_issue_custody(plan, body))
        {
            return Err(Error::Revoked);
        }
        let window = entry.terms.window(self.now, plan.deadline())?;
        #[cfg(feature = "serial-plans")]
        let (operation, retry, credential) = body.serial.as_ref().map_or_else(
            || {
                (
                    body.policy.operation,
                    body.policy.retry,
                    body.policy.credential.clone(),
                )
            },
            |serial| serial.terms(&body.policy.credential),
        );
        // A domain Plan's calls are members by their own operation: a Read call
        // in a Mutations domain is not paced.
        #[cfg(feature = "release-domains")]
        let pacing = match self.io.core.release_domain {
            Some(domain) if body.policy.pacing == Some(domain.pacing) => {
                self.io.core.domain_pacing(operation, None)?
            }
            _ => body.policy.pacing,
        };
        #[cfg(all(feature = "serial-plans", not(feature = "release-domains")))]
        let pacing = body.policy.pacing;
        #[cfg(feature = "serial-plans")]
        let mut context = Context::new(
            plan.owner(),
            entry.logical,
            operation,
            credential,
            self.now,
            window.deadline(),
            pacing,
        )?
        .with_retry(retry)?;
        #[cfg(not(feature = "serial-plans"))]
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
        #[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
        {
            context = context.with_egress(session_egress)?;
        }
        context.bind_issuer(&self.io.core.request_contexts.issuer);
        let call = Call::open(
            context.clone(),
            entry.terms,
            window,
            entry.ids.clone(),
            entry.signing.clone(),
            self.now,
            self.io.core.http_delivery.clone(),
        )?;
        let owner = self
            .io
            .core
            .request_contexts
            .owner_entry_mut(plan.owner())
            .unwrap();
        let narrowed = match intent {
            Some(intent) if owner.prepare_leg_intent(&context, intent, protection_cancel)? => {
                Some(intent)
            }
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
        let body = self.io.core.request_contexts.plan_body_mut(plan).unwrap();
        body.schedule.start(slot).expect("available plan slot");
        body.entries[index].notified = true;
        body.entries[index].parked = false;
        body.entries[index].credit = Credit::Issued;
        body.entries[index].call = Some(call);
        self.io.core.request_contexts.reserved_contexts -= 1;
        self.io
            .core
            .request_contexts
            .contexts
            .insert(context.leg(), context.clone());
        Ok(context)
    }

    #[inline]
    pub fn validate_plan_delivery(&self, plan: &Plan, context: &Context) -> Result<(), Error> {
        self.core_ref().validate_plan_delivery(plan, context)
    }

    pub(crate) fn bind_read_plan_history(&mut self, plan: &Plan) -> Result<Plan, Error> {
        self.validate_preflight_ready_owner(plan.owner())?;
        let Owner::Command { id, .. } = plan.owner() else {
            return Err(Error::Invalid);
        };
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        if body.policy.operation != Operation::Read || body.receipt_cap.is_none() {
            return Err(Error::Invalid);
        }
        if body.history_bound
            || body
                .entries
                .iter()
                .any(|entry| entry.credit != Credit::Unissued || entry.call.is_some())
        {
            return Err(Error::State);
        }
        if self.io.core.http_owner_poison
            || self.io.core.http_plan_expiring.is_some()
            || self
                .io
                .core
                .http_plan_ready
                .as_ref()
                .is_some_and(|ready| !ready.same_plan(plan))
        {
            return Err(Error::Revoked);
        }
        if self.io.core.http_delivery.is_some() {
            self.io.core.request_contexts.validate_plan_seal_origin(
                plan,
                self.now,
                self.io.core.http_delivery.as_ref(),
            )?;
        } else if !(self.io.entry_bound(id)
            || self
                .io
                .core
                .http_preflight_ready
                .as_ref()
                .is_some_and(|ready| ready.owner() == plan.owner())
            || self
                .io
                .core
                .http_plan_ready
                .as_ref()
                .is_some_and(|ready| ready.same_plan(plan)))
        {
            return Err(Error::Revoked);
        }
        self.io
            .core
            .request_contexts
            .plan_body_mut(plan)?
            .history_bound = true;
        Ok(plan.clone())
    }

    /// Settle a call after validating DATA and recording write facts. Structural
    /// progress grants no effect or successful command outcome by itself.
    pub fn finish_plan_call(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<(), Error> {
        self.io.core.request_contexts.validate_unbound_plan(plan)?;
        self.finish_plan_call_inner(plan, slot, context)
    }

    fn finish_plan_call_inner(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<(), Error> {
        self.validate_plan_delivery(plan, context)?;
        let body = self.io.core.request_contexts.plan_body(plan)?;
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        if context.operation() == Operation::Mutation {
            self.io
                .core
                .request_contexts
                .owner_entry(plan.owner())
                .unwrap()
                .writes
                .as_ref()
                .ok_or(Error::State)?
                .validate_housekeeping(call.admission.attempt.request)?;
        }
        self.io.retire_http_leg(context);
        self.io
            .core
            .request_contexts
            .contexts
            .remove(&context.leg());
        let body = self.io.core.request_contexts.plan_body_mut(plan)?;
        body.schedule.settle(slot)?;
        body.entries[index].call = None;
        body.entries[index].credit = Credit::Released;
        body.handoff = self.io.core.http_delivery.clone();
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

    #[inline]
    fn select_plan_retry(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<Context, Error> {
        self.core().select_plan_retry(plan, slot, context)
    }

    /// Only a structurally complete plan may request success. Journal facts
    /// convert known-partial, refused or unresolved writes into terminal failure.
    pub fn done_plan(&mut self, plan: &Plan, result: String) -> Result<(), Error> {
        self.validate_preflight_ready_owner(plan.owner())?;
        self.io.core.request_contexts.validate_unbound_plan(plan)?;
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        if !body.schedule.is_complete() {
            return Err(Error::State);
        }
        self.seal_plan(plan, Some(result), None, None)
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
        self.io.core.request_contexts.validate_unbound_plan(plan)?;
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
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
        self.seal_plan(plan, Some(result), None, None)
    }

    /// Failure-only retirement; expired authority never creates fresh sends.
    pub fn fail_plan(&mut self, plan: &Plan, error: impl Into<VenueError>) -> Result<(), Error> {
        self.fail_plan_checked(plan, error.into(), None)
    }

    /// `fail_plan` whose `note` is appended to the projected terminal message.
    /// The note never selects the error or changes its kind, outcome or IDs.
    pub fn fail_plan_noted(
        &mut self,
        plan: &Plan,
        error: impl Into<VenueError>,
        note: &str,
    ) -> Result<(), Error> {
        self.fail_plan_checked(plan, error.into(), Some(note))
    }

    fn fail_plan_checked(
        &mut self,
        plan: &Plan,
        error: VenueError,
        note: Option<&str>,
    ) -> Result<(), Error> {
        if !self
            .io
            .core
            .http_plan_expiring
            .as_ref()
            .is_some_and(|current| current.same_plan(plan))
        {
            self.io.core.request_contexts.validate_unbound_plan(plan)?;
        }
        self.fail_plan_noted_owned(plan, error, note)
    }

    pub(in crate::engine) fn fail_plan_owned(
        &mut self,
        plan: &Plan,
        error: VenueError,
    ) -> Result<(), Error> {
        self.fail_plan_noted_owned(plan, error, None)
    }

    fn fail_plan_noted_owned(
        &mut self,
        plan: &Plan,
        error: VenueError,
        note: Option<&str>,
    ) -> Result<(), Error> {
        if self
            .io
            .core
            .http_plan_expiring
            .as_ref()
            .is_some_and(|current| current.same_plan(plan))
        {
            self.io.core.request_contexts.validate_plan_seal_origin(
                plan,
                self.now,
                self.io.core.http_delivery.as_ref(),
            )?;
            return Ok(());
        }
        self.seal_plan(plan, None, Some(error), note)
    }

    fn seal_plan(
        &mut self,
        plan: &Plan,
        result: Option<String>,
        error: Option<VenueError>,
        note: Option<&str>,
    ) -> Result<(), Error> {
        let prepared = self.prepare_plan_terminal(plan, error, note, result.as_deref())?;
        self.commit_plan_terminal(plan, result, prepared)
    }

    fn prepare_plan_terminal(
        &mut self,
        plan: &Plan,
        error: Option<VenueError>,
        note: Option<&str>,
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
        let mut prepared = self.io.core.request_contexts.prepare_plan_seal(
            plan,
            self.now,
            self.io.core.http_delivery.as_ref(),
            error,
            note,
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
            let observers = self.io.core.request_contexts.planned_observer_count(plan);
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

    #[inline]
    fn commit_plan_terminal(
        &mut self,
        plan: &Plan,
        result: Option<String>,
        prepared: conditional::PreparedPlanSeal,
    ) -> Result<(), Error> {
        self.core().commit_plan_terminal(plan, result, prepared)
    }

    #[inline]
    fn rearm_plan(&mut self, plan: &Plan) {
        self.core().rearm_plan(plan)
    }
}

impl crate::engine::CoreCx<'_> {
    /// A resume drained with no delivery is the resumed attempt's custody for
    /// the rest of this event only.
    fn note_session_resume(&mut self, plan: &Plan, leg: &Context) {
        if self.io.core.http_delivery.is_some() {
            return;
        }
        let body = self
            .io
            .core
            .request_contexts
            .plan_body(plan)
            .expect("resumed Plan");
        let request = body
            .entries
            .iter()
            .find_map(|entry| entry.call.as_ref().filter(|call| call.context == *leg))
            .expect("resumed call")
            .admission
            .attempt
            .request;
        self.io.core.session_resumed.push(request);
    }

    /// Exact released DATA authority before any native or Store mutation.
    // Only some feature combinations call this.
    #[cfg_attr(
        all(feature = "http-plans", not(feature = "serial-plans")),
        allow(dead_code)
    )]
    #[inline]
    pub fn validate_plan_delivery(&self, plan: &Plan, context: &Context) -> Result<(), Error> {
        self.core_ref().validate_plan_delivery(plan, context)
    }

    fn select_plan_retry(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<Context, Error> {
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        self.io.core.request_contexts.validate(context, self.now)?;
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
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
                .core
                .request_contexts
                .owner_entry_mut(plan.owner())
                .unwrap()
                .writes
                .as_mut()
                .ok_or(Error::State)?
                .retry(previous, next)?;
        }
        self.io.retire_http_leg(context);
        let body = self.io.core.request_contexts.plan_body_mut(plan)?;
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
        let (owner, seal) = self
            .io
            .core
            .request_contexts
            .commit_plan_seal(plan, prepared);
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
            .core
            .request_contexts
            .plan_body(plan)
            .expect("live plan timer");
        let timer = body.timer;
        let at_ms = body.next_wake();
        self.io.core.actions.retain(|action| !matches!(action, Action::SetTimer { timer: previous, .. } if *previous == timer));
        #[cfg(feature = "serial-plans")]
        self.io.core.actions.push(serial::wake(timer, at_ms));
        #[cfg(not(feature = "serial-plans"))]
        self.io.core.actions.push(Action::SetTimer { timer, at_ms });
    }
}

impl crate::engine::CoreRef<'_> {
    pub fn validate_plan_delivery(&self, plan: &Plan, context: &Context) -> Result<(), Error> {
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        self.io.core.request_contexts.validate(context, self.now)?;
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
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
            .core
            .managed_http
            .get(&delivery.attempt.request)
            .is_some_and(|managed| managed.released_delivery(delivery))
        {
            return Err(Error::Revoked);
        }
        Ok(())
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
    #[cfg(feature = "condition-waits")]
    fn next_wake(&self) -> f64 {
        let wake = self
            .entries
            .iter()
            .filter(|entry| entry.credit == Credit::Unissued && !entry.notified && !entry.parked)
            .filter_map(|entry| entry.eligible_at)
            .fold(self.handle.deadline(), f64::min);
        self.serial
            .as_ref()
            .and_then(|serial| serial.condition_deadline())
            .map_or(wake, |at| wake.min(at))
    }
    #[cfg(not(feature = "condition-waits"))]
    fn next_wake(&self) -> f64 {
        self.entries
            .iter()
            .filter(|entry| entry.credit == Credit::Unissued && !entry.notified && !entry.parked)
            .filter_map(|entry| entry.eligible_at)
            .fold(self.handle.deadline(), f64::min)
    }
}

impl ContextRegistry {
    pub(in super::super) fn plan_registered(&self, plan: &Plan) -> bool {
        self.plan_body(plan).is_ok()
    }

    /// Return a parked Slot to ordinary availability; a sealed Plan is a no-op.
    pub(in super::super) fn unpark_plan_slot(&mut self, plan: &Plan, slot: &Slot) {
        if let Ok(body) = self.plan_body_mut(plan)
            && let Ok(index) = body.schedule.ordinal(slot)
        {
            body.entries[index].parked = false;
        }
    }

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
            .core
            .request_contexts
            .plan_from_context(&delivery.context)
        else {
            return false;
        };
        let body = self.io.core.request_contexts.plan_body(&plan).unwrap();
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
        self.fail_plan_owned(&plan, error.clone())
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
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context.same_issuer(context) && call.context == *context)
            .ok_or(Error::Revoked)?;
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) || !matches!(call.stage, Stage::Active { failed: true }) {
            return Err(Error::Revoked);
        }
        if context.operation() == Operation::Mutation {
            self.io
                .core
                .request_contexts
                .owner_entry(plan.owner())
                .unwrap()
                .writes
                .as_ref()
                .ok_or(Error::State)?
                .validate_failed_attempt(delivery.attempt.request)?;
            #[cfg(feature = "http-mutation-barriers")]
            if body.barriers.iter().any(|range| range.contains(&index)) {
                self.io
                    .core
                    .request_contexts
                    .owner_entry(plan.owner())
                    .unwrap()
                    .writes
                    .as_ref()
                    .ok_or(Error::State)?
                    .validate_housekeeping(delivery.attempt.request)?;
            }
        }
        self.io.retire_http_leg(context);
        self.io
            .core
            .request_contexts
            .contexts
            .remove(&context.leg());
        let body = self.io.core.request_contexts.plan_body_mut(plan)?;
        body.schedule.settle(slot)?;
        body.entries[index].call = None;
        body.entries[index].credit = Credit::Released;
        body.handoff = self.io.core.http_delivery.clone();
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
                    && !entry.parked
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
        if let Ok(body) = io.core.request_contexts.plan_body(plan) {
            let (timer, at_ms) = (body.timer, body.next_wake());
            io.core.actions.retain(|action| !matches!(action, Action::SetTimer { timer: previous, .. } if *previous == timer));
            #[cfg(feature = "serial-plans")]
            io.core.actions.push(serial::wake(timer, at_ms));
            #[cfg(not(feature = "serial-plans"))]
            io.core.actions.push(Action::SetTimer { timer, at_ms });
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
    pub(in super::super) fn command_writes(&self, command: CommandId) -> Option<&CommandWrites> {
        self.commands.get(&command)?.writes.as_ref()
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
        let body = self.io.core.request_contexts.plan_body(plan)?;
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
            .core
            .request_contexts
            .owner_entry_mut(plan.owner())
            .unwrap()
            .writes
            .as_mut()
            .ok_or(Error::State)?
            .replace(previous, &children)?;
        self.io.retire_http_leg(context);
        self.io
            .core
            .request_contexts
            .contexts
            .remove(&context.leg());
        let body = self.io.core.request_contexts.plan_body_mut(plan)?;
        body.schedule
            .replace(slot)
            .expect("validated declared replacement");
        body.entries[index].call = None;
        body.entries[index].credit = Credit::Released;
        body.handoff = self.io.core.http_delivery.clone();
        body.refresh_availability(self.now);
        self.rearm_plan(plan);
        Ok(())
    }

    /// Raise a live Mutation Plan's failure projection to Unknown, for an
    /// effect the venue can no longer prove (such as a failed protective
    /// census after an accepted placement). Monotonic; it grants no send,
    /// retry or success authority and records no write fact.
    pub fn inherit_plan_uncertainty(&mut self, plan: &Plan) -> Result<(), Error> {
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        if body.policy.operation != Operation::Mutation {
            return Err(Error::Invalid);
        }
        let owner = self
            .io
            .core
            .request_contexts
            .owner_entry_mut(plan.owner())
            .ok_or(Error::Revoked)?;
        owner
            .writes
            .as_mut()
            .ok_or(Error::State)?
            .inherit_uncertain();
        owner.inherited_uncertain = true;
        Ok(())
    }

    /// Retire the current unissued call of an independent complete Plan that
    /// the venue decides not to send. No journal fact; its reservations return.
    /// Admitted only where the call could have been issued; every check
    /// precedes the first change. Retiring every call leaves the Plan unable to
    /// succeed: a venue whose targets are all closed uses `done_empty_plan`
    /// before admission. See docs/plans/RUST_PLAN_RETIRE_UNISSUED.md.
    pub fn retire_unissued_plan_call(&mut self, plan: &Plan, slot: &Slot) -> Result<(), Error> {
        self.io.core.request_contexts.validate_unbound_plan(plan)?;
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        #[cfg(feature = "serial-plans")]
        if body.serial.is_some() {
            return Err(Error::State);
        }
        if !body.policy.independent_calls {
            return Err(Error::Invalid);
        }
        let index = body.schedule.ordinal(slot)?;
        #[cfg(feature = "http-mutation-barriers")]
        if body.barriers.iter().any(|group| group.contains(&index)) {
            return Err(Error::Invalid);
        }
        let entry = &body.entries[index];
        if entry.credit != Credit::Unissued
            || entry.call.is_some()
            || entry.parked
            || !body
                .schedule
                .available()
                .any(|candidate| body.schedule.ordinal(&candidate) == Ok(index))
            || !entry.eligible_at.is_some_and(|at| at <= self.now)
            || !self.plan_issue_custody(plan, body)
        {
            return Err(Error::Revoked);
        }
        let body = self.io.core.request_contexts.plan_body_mut(plan)?;
        body.schedule
            .retire_unissued(slot)
            .expect("checked available slot");
        body.entries[index].credit = Credit::Released;
        body.refresh_availability(self.now);
        self.io.core.request_contexts.reserved_contexts -= 1;
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
            self.io
                .core
                .request_contexts
                .contexts
                .remove(&context.leg());
        }
        self.finish_sealed_command(id, Some(result), Seal::Success)
    }
}

impl ContextRegistry {
    pub(in crate::engine) fn record_read_plan_release(&mut self, delivery: &Delivery) {
        let Some(plan) = self.plan_from_context(&delivery.context) else {
            return;
        };
        let Ok(body) = self.plan_body_mut(&plan) else {
            return;
        };
        if !body.history_bound {
            return;
        }
        if body.entries.iter().any(|entry| {
            entry
                .call
                .as_ref()
                .is_some_and(|call| call.matches(delivery))
        }) {
            body.history_latest_release = Some(delivery.attempt);
        }
    }

    pub(in crate::engine) fn record_read_plan_failure(
        &mut self,
        delivery: &Delivery,
        origin: super::ReleasedReadFailure,
    ) -> bool {
        let Some(plan) = self.plan_from_context(&delivery.context) else {
            return false;
        };
        let Ok(body) = self.plan_body_mut(&plan) else {
            return false;
        };
        let history = body.history_bound;
        #[cfg(feature = "serial-plans")]
        let serial = body.serial.is_some();
        #[cfg(not(feature = "serial-plans"))]
        let serial = false;
        if (!history && !serial)
            || (history && body.history_latest_release != Some(delivery.attempt))
        {
            return false;
        }
        let Some(call) = body
            .entries
            .iter_mut()
            .find_map(|entry| entry.call.as_mut().filter(|call| call.matches(delivery)))
        else {
            return false;
        };
        if !matches!(call.stage, Stage::Active { failed: true }) {
            return false;
        }
        call.released_failure = Some(origin);
        history
    }
}

impl<P> Cx<'_, P> {
    pub(crate) fn validate_read_plan_history_failure(
        &self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
    ) -> Result<Attempt, Error> {
        self.validate_preflight_ready_owner(plan.owner())?;
        if self.io.core.http_owner_poison
            || self.io.core.http_plan_poison
            || self.io.core.http_plan_expiring.is_some()
        {
            return Err(Error::Revoked);
        }
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        if !body.history_bound
            || !matches!(plan.owner(), Owner::Command { .. })
            || body.policy.operation != Operation::Read
            || body.receipt_cap.is_none()
        {
            return Err(Error::Invalid);
        }
        let index = body.schedule.ordinal(slot)?;
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| {
                call.context.same_issuer(context)
                    && call.context == *context
                    && call.matches(delivery)
            })
            .ok_or(Error::Revoked)?;
        if !matches!(call.stage, Stage::Active { failed: true })
            || call.released_failure.is_none()
            || body.history_latest_release != Some(delivery.attempt)
            || self.io.core.http_history_failure != Some(delivery.attempt)
        {
            return Err(Error::Revoked);
        }
        Ok(delivery.attempt)
    }
}

#[path = "request_context_history_plan.rs"]
mod history;

#[cfg(feature = "serial-plans")]
#[path = "request_context_serial_plan.rs"]
mod serial;

#[cfg(feature = "condition-waits")]
#[path = "request_context_condition_wait.rs"]
mod condition_wait;

#[cfg(all(test, not(feature = "http-mutation-barriers")))]
#[path = "request_context_group_disabled_tests.rs"]
mod disabled_barrier_tests;

#[cfg(all(test, feature = "serial-plans"))]
#[path = "request_context_schedule_admission_tests.rs"]
mod schedule_admission_tests;

#[cfg(all(feature = "serial-plans", feature = "session-bearer"))]
impl ContextRegistry {
    pub(in crate::engine) fn prepare_keyless_wire(
        &self,
        context: &Context,
        method: crate::Method,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> Result<Option<[u8; 32]>, Error> {
        if self.keyless_session(context.owner()).is_none() {
            return Ok(None);
        }
        let Some(plan) = self.plan_from_context(context) else {
            return Ok(None);
        };
        let current = self.plan_body(&plan)?;
        let Some(serial) = current.serial.as_ref().filter(|s| s.session_root.is_some()) else {
            return Ok(None);
        };
        let fingerprint = super::keyless_session::wire_fingerprint(method, url, headers, body);
        if serial
            .wire_fingerprint
            .is_some_and(|expected| expected != fingerprint)
        {
            return Err(Error::Invalid);
        }
        Ok(Some(fingerprint))
    }

    pub(in crate::engine) fn commit_keyless_wire(&mut self, owner: Owner, fingerprint: [u8; 32]) {
        let CommandPhase::Planned(state) =
            &mut self.owner_entry_mut(owner).expect("queued owner").phase
        else {
            unreachable!("queued serial Plan");
        };
        state.body[0]
            .serial
            .as_mut()
            .expect("queued keyless serial")
            .wire_fingerprint = Some(fingerprint);
    }
}
