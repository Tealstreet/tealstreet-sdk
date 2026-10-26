//! One bounded physical read chain under a retained preflight generation.
use super::super::owned_call::{Admission, Call, Stage};
use super::*;
use crate::request_context::{RetryPolicy, plan::CallTerms};

pub(in super::super) struct State {
    pub(super) policy: PreflightPolicy,
    pub(super) receipt_cap: Option<(u64, f64)>,
    // At most one entry; storage is reserved fallibly before ID admission.
    pub(super) call: Vec<Call>,
    pub(super) handoff: Option<Delivery>,
    pub(super) activation: super::ready::Activation,
    /// The next read waits on a login Session: nothing may issue, complete or
    /// re-park it until that Session's drain or ended disposition.
    pub(super) session_parked: bool,
    /// Parked by `park_preflight_for`: failable from any callback while it
    /// holds nothing; `condition` is its engine bound's timer.
    #[cfg(feature = "condition-waits")]
    pub(super) condition_park: bool,
    #[cfg(feature = "condition-waits")]
    pub(super) condition: Option<crate::TimerId>,
}

impl State {
    /// The admitted write-after-reads credential, for repair mint custody.
    #[cfg(feature = "repair-mutations")]
    pub(in crate::engine) fn repair_write_credential(
        &self,
    ) -> Option<&crate::request_context::Credential> {
        (self.policy.purpose == PreflightPurpose::WriteAfterReads)
            .then_some(self.policy.write_credential.as_ref())
            .flatten()
    }

    pub(super) fn new(policy: PreflightPolicy) -> Self {
        Self {
            policy,
            receipt_cap: None,
            call: Vec::new(),
            handoff: None,
            activation: super::ready::Activation::Initial,
            session_parked: false,
            #[cfg(feature = "condition-waits")]
            condition_park: false,
            #[cfg(feature = "condition-waits")]
            condition: None,
        }
    }

    pub(super) fn validate_seal(
        &self,
        delivery: Option<&Delivery>,
        failure: bool,
        pending: usize,
        deliveries: usize,
    ) -> Result<(), Error> {
        if let Some(call) = self.call.first() {
            if !failure {
                return Err(Error::Revoked);
            }
            if matches!(call.stage, Stage::Ready) {
                return if delivery == call.admission.predecessor.as_ref()
                    || (delivery.is_none() && pending == 0 && deliveries == 0)
                {
                    Ok(())
                } else {
                    Err(Error::Revoked)
                };
            }
            return match delivery {
                Some(delivery) if call.matches(delivery) => Ok(()),
                None if pending == 0 && deliveries == 0 => Ok(()),
                _ => Err(Error::Revoked),
            };
        }
        if (!failure && self.handoff.is_some() && delivery != self.handoff.as_ref())
            || pending != 0
            || (delivery.is_none() && deliveries != 0)
            || delivery.is_some_and(|delivery| self.handoff.as_ref() != Some(delivery))
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// A read parked on or resumed from a Session never outlives the caller's
    /// original receipt. A read issued in ordinary custody keeps its declared
    /// window, so a valid handoff past the receipt still delivers.
    pub(super) fn read_bound(&self, parked: bool) -> f64 {
        match self.receipt_cap {
            Some((_, cap)) if parked => cap,
            _ => f64::MAX,
        }
    }

    pub(super) fn admits_handoff(&self, delivery: Option<&Delivery>) -> bool {
        delivery == self.handoff.as_ref()
    }
}

impl ContextRegistry {
    pub(in crate::engine) fn preflight_receipt_cap(
        &self,
        preflight: &Preflight,
    ) -> Result<Option<(u64, f64)>, Error> {
        Ok(self.preflight_state(preflight)?.receipt_cap)
    }

    pub(in super::super) fn bind_preflight_receipt(
        &mut self,
        preflight: &Preflight,
        cap: (u64, f64),
    ) -> Result<(), Error> {
        self.preflight_state_mut(preflight)?.receipt_cap = Some(cap);
        Ok(())
    }

    pub(super) fn owner_from_context(&self, context: &Context) -> Result<Preflight, Error> {
        let Owner::Command { id, generation } = context.owner() else {
            return Err(Error::Invalid);
        };
        if !context.issued_by(&self.issuer) {
            return Err(Error::Revoked);
        }
        let handle = Preflight::new(&self.issuer, id, generation);
        self.preflight_owner(&handle)?;
        Ok(handle)
    }

    pub(super) fn read_call(&self, handle: &Preflight, context: &Context) -> Result<&Call, Error> {
        self.preflight_state(handle)?
            .call
            .first()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)
    }

    pub(in super::super::super) fn record_owned_read_failure(
        &mut self,
        delivery: &Delivery,
    ) -> bool {
        if (matches!(delivery.context.owner(), Owner::Command { .. })
            || self.physical_owner(delivery.context.owner()))
            && delivery.context.issued_by(&self.issuer)
            && let Some(owner) = self.owner_entry_mut(delivery.context.owner())
            && let CommandPhase::Planned(state) = &mut owner.phase
        {
            return state.record_failure(delivery);
        }
        if !matches!(delivery.context.owner(), Owner::Command { id, .. }
            if self.commands.get(&id).is_some_and(|owner| matches!(owner.phase, CommandPhase::Preflight(_))))
        {
            return false;
        }
        let Ok(handle) = self.owner_from_context(&delivery.context) else {
            return false;
        };
        let state = self
            .preflight_state_mut(&handle)
            .expect("validated preflight");
        if let Some(call) = state.call.first_mut().filter(|call| call.matches(delivery))
            && let Stage::Active { failed } = &mut call.stage
        {
            *failed = true;
            return true;
        }
        false
    }

    pub(in super::super::super) fn failed_read_owner(
        &self,
        delivery: &Delivery,
    ) -> Option<Preflight> {
        let handle = self.owner_from_context(&delivery.context).ok()?;
        let call = self.read_call(&handle, &delivery.context).ok()?;
        (call.matches(delivery) && matches!(call.stage, Stage::Active { failed: true }))
            .then_some(handle)
    }

    pub(in super::super::super) fn validate_legacy(&self, context: &Context) -> Result<(), Error> {
        if matches!(context.owner(), Owner::Command { id, .. }
            if self.commands.get(&id).is_some_and(|owner| !matches!(owner.phase, CommandPhase::Context { .. })))
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    pub(in super::super::super) fn validate_owned_context(
        &self,
        context: &Context,
    ) -> Result<(), Error> {
        let phase = match context.owner() {
            Owner::Command { id, .. } => self.commands.get(&id).map(|owner| &owner.phase),
            // The execution root shares its physical owner but is no Plan call.
            Owner::Execution { generation } if context.leg() == generation => return Ok(()),
            owner if self.physical_owner(owner) => {
                self.owner_entry(owner).map(|owner| &owner.phase)
            }
            _ => return Ok(()),
        };
        match phase {
            Some(CommandPhase::Preflight(state)) => {
                if state
                    .call
                    .first()
                    .is_some_and(|call| call.context == *context)
                {
                    Ok(())
                } else {
                    Err(Error::Revoked)
                }
            }
            Some(CommandPhase::Planned(state)) => {
                if state.context_current(context) {
                    Ok(())
                } else {
                    Err(Error::Revoked)
                }
            }
            Some(CommandPhase::Retiring(_)) => Err(Error::Revoked),
            _ => Ok(()),
        }
    }

    fn inherited_uncertain(&self, handle: &Preflight) -> bool {
        self.preflight_owner(handle)
            .is_ok_and(|owner| owner.inherited_uncertain)
    }

    pub(super) fn preflight_state(&self, handle: &Preflight) -> Result<&State, Error> {
        let owner = self.preflight_owner(handle)?;
        let CommandPhase::Preflight(state) = &owner.phase else {
            unreachable!()
        };
        Ok(state)
    }

    pub(super) fn preflight_state_mut(&mut self, handle: &Preflight) -> Result<&mut State, Error> {
        self.preflight_owner(handle)?;
        let Owner::Command { id, .. } = handle.owner() else {
            unreachable!()
        };
        let CommandPhase::Preflight(state) = &mut self.commands.get_mut(&id).unwrap().phase else {
            unreachable!()
        };
        Ok(state)
    }

    pub(in super::super::super) fn owned_admission(
        &self,
        context: &Context,
        delivery: Option<&Delivery>,
    ) -> Result<Option<Admission>, Error> {
        if self.physical_owner(context.owner()) {
            let owner = self.owner_entry(context.owner()).expect("physical owner");
            let CommandPhase::Planned(state) = &owner.phase else {
                return Err(Error::Revoked);
            };
            if !context.issued_by(&self.issuer) {
                return Err(Error::Revoked);
            }
            return state.admission(context, delivery);
        }
        let Owner::Command { id, .. } = context.owner() else {
            return Ok(None);
        };
        let Some(owner) = self.commands.get(&id) else {
            return Ok(None);
        };
        if let CommandPhase::Planned(state) = &owner.phase {
            if !context.issued_by(&self.issuer)
                || context.owner()
                    != (Owner::Command {
                        id,
                        generation: owner.generation,
                    })
            {
                return Err(Error::Revoked);
            }
            return state.admission(context, delivery);
        }
        let CommandPhase::Preflight(state) = &owner.phase else {
            return if !matches!(owner.phase, CommandPhase::Context { .. }) {
                Err(Error::Revoked)
            } else {
                Ok(None)
            };
        };
        if !context.issued_by(&self.issuer)
            || context.owner()
                != (Owner::Command {
                    id,
                    generation: owner.generation,
                })
        {
            return Err(Error::Revoked);
        }
        let call = state.call.first().ok_or(Error::Revoked)?;
        if call.context != *context
            || !matches!(call.stage, Stage::Ready)
            || delivery != call.admission.predecessor.as_ref()
        {
            return Err(Error::Revoked);
        }
        Ok(Some(call.admission.clone()))
    }

    pub(in super::super::super) fn commit_owned_admission(&mut self, admission: &Admission) {
        let phase = &mut self
            .owner_entry_mut(admission.context.owner())
            .expect("validated owned admission")
            .phase;
        if let CommandPhase::Planned(state) = phase {
            state.commit(admission);
            return;
        }
        let CommandPhase::Preflight(state) = phase else {
            unreachable!()
        };
        let call = state.call.first_mut().unwrap();
        assert!(call.admission.attempt == admission.attempt && matches!(call.stage, Stage::Ready));
        call.stage = Stage::Active { failed: false };
    }
}

impl<P> Cx<'_, P> {
    #[inline]
    pub(in super::super::super) fn preflight_plan_origin(
        &self,
        handle: &Preflight,
    ) -> Result<(PreflightPolicy, Option<Context>), Error> {
        self.core_ref().preflight_plan_origin(handle)
    }

    pub(in super::super::super) fn with_owned_read_failure(
        &mut self,
        error: impl FnOnce() -> VenueError,
        callback: impl FnOnce(&mut Self),
    ) {
        self.with_read_plan_failure_origin(None, error, callback);
    }

    pub(in super::super::super) fn with_read_plan_failure_origin(
        &mut self,
        origin: Option<super::super::ReleasedReadFailure>,
        error: impl FnOnce() -> VenueError,
        callback: impl FnOnce(&mut Self),
    ) {
        self.io.core.http_history_failure = None;
        let owned = self.io.core.http_delivery.as_ref().is_some_and(|delivery| {
            self.io
                .core
                .request_contexts
                .record_owned_read_failure(delivery)
        });
        if !owned {
            callback(self);
            return;
        }
        self.with_recorded_owned_failure(origin, error, callback);
    }

    fn with_recorded_owned_failure(
        &mut self,
        origin: Option<super::super::ReleasedReadFailure>,
        error: impl FnOnce() -> VenueError,
        callback: impl FnOnce(&mut Self),
    ) {
        let delivery = self
            .io
            .core
            .http_delivery
            .clone()
            .expect("owned failure delivery");
        if self.io.core.http_plan_expiring.is_none()
            && self
                .io
                .core
                .managed_http
                .get(&delivery.attempt.request)
                .is_some_and(|managed| managed.released_delivery(&delivery))
            && let Some(origin) = origin
            && self
                .io
                .core
                .request_contexts
                .record_read_plan_failure(&delivery, origin)
        {
            self.io.core.http_history_failure = Some(delivery.attempt);
        }
        callback(self);
        self.io.core.http_history_failure = None;
        if self
            .io
            .core
            .request_contexts
            .failed_read_owner(&delivery)
            .is_some()
            || self
                .io
                .core
                .request_contexts
                .failed_plan_delivery(&delivery)
        {
            self.finish_owned_read_failure(&delivery, error());
        }
    }

    pub(in super::super::super) fn with_independent_plan_failure(
        &mut self,
        plan: &Plan,
        error: VenueError,
        callback: impl FnOnce(&mut Self, VenueError),
    ) -> Result<(), Error> {
        let owned = self.io.core.http_delivery.as_ref().is_some_and(|delivery| {
            self.io
                .core
                .request_contexts
                .plan_from_context(&delivery.context)
                .is_some_and(|current| current.same_plan(plan))
                && self
                    .io
                    .core
                    .request_contexts
                    .record_owned_read_failure(delivery)
        });
        if !owned {
            return self.fail_plan(plan, error);
        }
        self.with_recorded_owned_failure(None, || error.clone(), |cx| callback(cx, error.clone()));
        Ok(())
    }

    /// `refusal` names a specific issue-time refusal for the Plan's terminal.
    #[inline(never)]
    pub(in super::super::super) fn fail_owned_admission(
        &mut self,
        admission: &Admission,
        reason: Error,
        refusal: Option<&'static str>,
    ) {
        let Ok(Some(current)) = self.owned_http_admission(&admission.context) else {
            return;
        };
        if current.attempt != admission.attempt {
            return;
        }
        if let Some(plan) = self
            .io
            .core
            .request_contexts
            .plan_from_context(&admission.context)
        {
            let error = match refusal {
                Some(refusal) => VenueError::new(refusal).kind(crate::ErrorKind::InvalidInput),
                None => VenueError::new("HTTP plan admission failed").kind(
                    if reason == Error::TimedOut {
                        crate::ErrorKind::Timeout
                    } else {
                        crate::ErrorKind::Internal
                    },
                ),
            };
            if admission.context.operation() == Operation::Mutation {
                let delivery = Delivery {
                    context: admission.context.clone(),
                    attempt: admission.attempt,
                };
                self.io
                    .core
                    .request_contexts
                    .result(&delivery, WriteResult::Failed(error.clone()))
                    .expect("pre-enrolled selected write");
            }
            self.fail_plan_owned(&plan, error)
                .expect("exact selected plan failure");
            return;
        }
        let handle = self
            .io
            .core
            .request_contexts
            .owner_from_context(&admission.context)
            .expect("owned admission");
        let state = self
            .io
            .core
            .request_contexts
            .preflight_state(&handle)
            .expect("owned admission");
        let inherited = self.io.core.request_contexts.inherited_uncertain(&handle);
        let mut error = VenueError::new("HTTP read admission failed")
            .kind(match reason {
                Error::TimedOut => crate::ErrorKind::Timeout,
                Error::Invalid | Error::Revoked | Error::Duplicate | Error::RetryForbidden => {
                    crate::ErrorKind::InvalidInput
                }
                _ => crate::ErrorKind::Internal,
            })
            .outcome(crate::Outcome::NotSent);
        let seal = if state.policy.purpose == PreflightPurpose::WriteAfterReads {
            error = CommandWrites::new(CAPACITY)
                .expect("fixed journal bound")
                .inheriting(inherited)
                .fail(error)
                .expect("empty projection");
            Seal::Failure(error, Vec::new())
        } else {
            Seal::Legacy(Some(error))
        };
        let Owner::Command { id, .. } = handle.owner() else {
            unreachable!()
        };
        self.finish_sealed_command(id, None, seal)
            .expect("owned admission failure");
    }

    #[inline(never)]
    pub(in super::super::super) fn finish_owned_read_failure(
        &mut self,
        delivery: &Delivery,
        mut error: VenueError,
    ) {
        if self.finish_failed_plan_call(delivery, &mut error) {
            return;
        }
        if let Some(handle) = self.io.core.request_contexts.failed_read_owner(delivery)
            && self.now.is_finite()
        {
            if self
                .io
                .core
                .request_contexts
                .read_call(&handle, &delivery.context)
                .is_ok_and(|call| self.now >= call.admission.deadline)
            {
                error.kind = Some(crate::ErrorKind::Timeout);
            }
            self.fail_preflight(&handle, error)
                .expect("exact failed read custody");
        }
    }

    #[inline]
    pub fn validate_preflight_delivery(
        &self,
        handle: &Preflight,
        context: &Context,
    ) -> Result<(), Error> {
        self.core_ref().validate_preflight_delivery(handle, context)
    }

    /// Retire one validated native read. Only this successful handoff permits a
    /// subsequent mode redo, cached read completion or final plan admission.
    pub fn finish_preflight_call(
        &mut self,
        handle: &Preflight,
        context: &Context,
    ) -> Result<(), Error> {
        self.validate_preflight_delivery(handle, context)?;
        let delivery = self
            .io
            .core
            .http_delivery
            .clone()
            .expect("validated delivery");
        self.io.retire_http_leg(context);
        self.io
            .core
            .request_contexts
            .contexts
            .remove(&context.leg());
        let state = self
            .io
            .core
            .request_contexts
            .preflight_state_mut(handle)
            .expect("validated preflight");
        state.call.clear();
        state.handoff = Some(delivery);
        Ok(())
    }

    /// Select one declared retry from the current physical delivery. This never
    /// renews the logical call or starts another preflight generation.
    pub fn preflight_retry_context(
        &mut self,
        handle: &Preflight,
        context: &Context,
        evidence: RetryEvidence,
    ) -> Result<Context, Error> {
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        self.io.core.request_contexts.validate(context, self.now)?;
        let call = self.io.core.request_contexts.read_call(handle, context)?;
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) || !matches!(call.stage, Stage::Active { .. }) {
            return Err(Error::Revoked);
        }
        context.retry(evidence, self.now)?;
        if call.selected >= call.terms.attempts {
            return Err(Error::RetryForbidden);
        }
        call.window.physical_deadline(self.now)?;
        let predecessor = delivery.clone();
        self.io.retire_http_leg(context);
        let call = self
            .io
            .core
            .request_contexts
            .preflight_state_mut(handle)
            .expect("validated preflight")
            .call
            .first_mut()
            .expect("validated call");
        call.select(self.now, Some(predecessor))
            .expect("validated retry window");
        Ok(context.clone())
    }

    /// Admit one bounded prerequisite read, retaining the command generation.
    pub fn preflight_read_context(
        &mut self,
        handle: &Preflight,
        policy: Policy,
        terms: CallTerms,
    ) -> Result<Context, Error> {
        self.validate_preflight_activation(handle)?;
        let state = self.io.core.request_contexts.preflight_state(handle)?;
        if !state.call.is_empty() || !state.admits_handoff(self.io.core.http_delivery.as_ref()) {
            return Err(Error::Revoked);
        }
        self.validate_preflight_read(handle, &policy, terms, false)?;
        self.issue_preflight_read(handle, policy, terms, false)
    }

    #[inline]
    pub(super) fn validate_preflight_read(
        &self,
        handle: &Preflight,
        policy: &Policy,
        terms: CallTerms,
        parked: bool,
    ) -> Result<(), Error> {
        self.core_ref()
            .validate_preflight_read(handle, policy, terms, parked)
    }

    #[inline]
    pub(super) fn issue_preflight_read(
        &mut self,
        handle: &Preflight,
        policy: Policy,
        terms: CallTerms,
        parked: bool,
    ) -> Result<Context, Error> {
        self.core()
            .issue_preflight_read(handle, policy, terms, parked)
    }
}

impl crate::engine::CoreCx<'_> {
    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    #[inline]
    pub(in super::super::super) fn preflight_plan_origin(
        &self,
        handle: &Preflight,
    ) -> Result<(PreflightPolicy, Option<Context>), Error> {
        self.core_ref().preflight_plan_origin(handle)
    }

    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    #[inline]
    pub fn validate_preflight_delivery(
        &self,
        handle: &Preflight,
        context: &Context,
    ) -> Result<(), Error> {
        self.core_ref().validate_preflight_delivery(handle, context)
    }

    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    #[inline]
    pub(super) fn validate_preflight_read(
        &self,
        handle: &Preflight,
        policy: &Policy,
        terms: CallTerms,
        parked: bool,
    ) -> Result<(), Error> {
        self.core_ref()
            .validate_preflight_read(handle, policy, terms, parked)
    }

    /// Issue a validated read in the caller's custody, with the current
    /// delivery (if any) as its predecessor.
    pub(super) fn issue_preflight_read(
        &mut self,
        handle: &Preflight,
        policy: Policy,
        terms: CallTerms,
        parked: bool,
    ) -> Result<Context, Error> {
        #[cfg(feature = "release-domains")]
        let policy = Policy {
            pacing: self.io.core.domain_pacing(Operation::Read, policy.pacing)?,
            ..policy
        };
        let bound = self
            .io
            .core
            .request_contexts
            .preflight_state(handle)?
            .read_bound(parked);
        let window = terms.window(self.now, bound)?;
        self.io.core.request_contexts.prune(self.now);
        if self.io.core.request_contexts.contexts.len()
            + self.io.core.request_contexts.reserved_contexts
            >= CAPACITY
        {
            return Err(Error::Capacity);
        }
        self.io
            .core
            .request_contexts
            .contexts
            .try_reserve(1 + self.io.core.request_contexts.reserved_contexts)
            .map_err(|_| Error::Capacity)?;
        self.io
            .core
            .request_contexts
            .preflight_state_mut(handle)?
            .call
            .try_reserve_exact(1)
            .map_err(|_| Error::Capacity)?;
        let count = u64::from(terms.attempts);
        let signed = policy.credential.is_some();
        if !self.io.core.next_id.available(1 + 3 * count)
            || (signed && !self.io.core.signing.available_ids(count))
        {
            return Err(Error::Capacity);
        }
        let mut ids = self
            .io
            .reserve_ids(1 + 3 * count)
            .expect("preflighted read call");
        let mut context = Context::new(
            handle.owner(),
            ids.start,
            Operation::Read,
            policy.credential,
            self.now,
            window.deadline(),
            policy.pacing,
        )
        .and_then(|context| context.with_retry(policy.retry))
        .expect("validated read policy");
        ids.start += 1;
        context.bind_issuer(&self.io.core.request_contexts.issuer);
        let signing = signed.then(|| {
            self.io
                .core
                .signing
                .reserve_ids(count)
                .expect("preflighted signing call")
        });
        let predecessor = self.io.core.http_delivery.clone();
        let call = Call::open(
            context.clone(),
            terms,
            window,
            ids,
            signing,
            self.now,
            predecessor,
        )
        .expect("validated initial physical window");
        let state = self
            .io
            .core
            .request_contexts
            .preflight_state_mut(handle)
            .expect("validated owner");
        state.call.push(call);
        state.handoff = None;
        self.io
            .core
            .request_contexts
            .contexts
            .insert(context.leg(), context.clone());
        Ok(context)
    }
}

impl crate::engine::CoreRef<'_> {
    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    pub(in super::super::super) fn preflight_plan_origin(
        &self,
        handle: &Preflight,
    ) -> Result<(PreflightPolicy, Option<Context>), Error> {
        self.validate_preflight_activation(handle)?;
        let state = self.io.core.request_contexts.preflight_state(handle)?;
        if let Some(call) = state.call.first() {
            self.validate_preflight_delivery(handle, &call.context)?;
            Ok((state.policy.clone(), Some(call.context.clone())))
        } else if state.admits_handoff(self.io.core.http_delivery.as_ref()) {
            Ok((state.policy.clone(), None))
        } else {
            Err(Error::Revoked)
        }
    }

    /// Check exact response authority before native/cache publication. A live
    /// response still requires native decoding and snapshot freshness checks.
    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    pub fn validate_preflight_delivery(
        &self,
        handle: &Preflight,
        context: &Context,
    ) -> Result<(), Error> {
        self.io.core.request_contexts.validate(context, self.now)?;
        let call = self.io.core.request_contexts.read_call(handle, context)?;
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) || !matches!(call.stage, Stage::Active { failed: false }) {
            return Err(Error::Revoked);
        }
        if self.now >= call.admission.deadline {
            return Err(Error::TimedOut);
        }
        let managed = self
            .io
            .core
            .managed_http
            .get(&delivery.attempt.request)
            .ok_or(Error::Revoked)?;
        if !managed.released_delivery(delivery) {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// The declared read's own terms, checked before any custody is spent.
    // Only some feature combinations call this.
    #[cfg_attr(feature = "http-plans", allow(dead_code))]
    pub(super) fn validate_preflight_read(
        &self,
        handle: &Preflight,
        policy: &Policy,
        terms: CallTerms,
        parked: bool,
    ) -> Result<(), Error> {
        let state = self.io.core.request_contexts.preflight_state(handle)?;
        if policy.operation != Operation::Read
            || policy.credential != state.policy.read_credential
            || terms.wait_ms != 0
            || terms.timeout_ms != policy.timeout_ms
            || (terms.attempts > 1 && policy.retry == RetryPolicy::Never)
        {
            return Err(Error::Invalid);
        }
        #[cfg(feature = "release-domains")]
        let pacing = self.io.core.domain_pacing(Operation::Read, policy.pacing)?;
        #[cfg(not(feature = "release-domains"))]
        let pacing = policy.pacing;
        let window = terms.window(self.now, state.read_bound(parked))?;
        Context::new(
            handle.owner(),
            1,
            Operation::Read,
            policy.credential.clone(),
            self.now,
            window.deadline(),
            pacing,
        )?
        .with_retry(policy.retry)?;
        Ok(())
    }
}
