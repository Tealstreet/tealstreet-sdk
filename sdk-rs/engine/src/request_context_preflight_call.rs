//! One bounded physical read chain under a retained preflight generation.
use super::super::owned_call::{Admission, Call, Stage};
use super::*;
use crate::TimerId;
use crate::request_context::{RetryPolicy, plan::CallTerms};

pub(in super::super) struct State {
    pub(super) policy: PreflightPolicy,
    pub(super) receipt_cap: Option<(u64, f64)>,
    // At most one entry; storage is reserved fallibly before ID admission.
    call: Vec<Call>,
    handoff: Option<Delivery>,
}

impl State {
    pub(super) fn new(policy: PreflightPolicy) -> Self {
        Self {
            policy,
            receipt_cap: None,
            call: Vec::new(),
            handoff: None,
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

    fn admits_handoff(&self, delivery: Option<&Delivery>) -> bool {
        delivery == self.handoff.as_ref()
    }
}

impl ContextRegistry {
    pub(in super::super) fn preflight_receipt_cap(
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

    fn owner_from_context(&self, context: &Context) -> Result<Preflight, Error> {
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

    fn read_call(&self, handle: &Preflight, context: &Context) -> Result<&Call, Error> {
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
        if let Owner::Command { id, generation } = delivery.context.owner()
            && delivery.context.issued_by(&self.issuer)
            && let Some(owner) = self
                .commands
                .get_mut(&id)
                .filter(|owner| owner.generation == generation)
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
        let Owner::Command { id, .. } = context.owner() else {
            return Ok(());
        };
        match self.commands.get(&id).map(|owner| &owner.phase) {
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

    fn preflight_state(&self, handle: &Preflight) -> Result<&State, Error> {
        let owner = self.preflight_owner(handle)?;
        let CommandPhase::Preflight(state) = &owner.phase else {
            unreachable!()
        };
        Ok(state)
    }

    fn preflight_state_mut(&mut self, handle: &Preflight) -> Result<&mut State, Error> {
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
        let Owner::Command { id, .. } = admission.context.owner() else {
            unreachable!()
        };
        let phase = &mut self.commands.get_mut(&id).unwrap().phase;
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
    pub(in super::super::super) fn preflight_plan_origin(
        &self,
        handle: &Preflight,
    ) -> Result<(PreflightPolicy, Option<Context>), Error> {
        let state = self.io.request_contexts.preflight_state(handle)?;
        if let Some(call) = state.call.first() {
            self.validate_preflight_delivery(handle, &call.context)?;
            Ok((state.policy.clone(), Some(call.context.clone())))
        } else if state.admits_handoff(self.io.http_delivery.as_ref()) {
            Ok((state.policy.clone(), None))
        } else {
            Err(Error::Revoked)
        }
    }

    pub(in super::super::super) fn with_owned_read_failure(
        &mut self,
        error: impl FnOnce() -> VenueError,
        callback: impl FnOnce(&mut Self),
    ) {
        let owned =
            self.io.http_delivery.as_ref().is_some_and(|delivery| {
                self.io.request_contexts.record_owned_read_failure(delivery)
            });
        if !owned {
            callback(self);
            return;
        }
        let delivery = self
            .io
            .http_delivery
            .clone()
            .expect("owned failure delivery");
        callback(self);
        if self
            .io
            .request_contexts
            .failed_read_owner(&delivery)
            .is_some()
            || self.io.request_contexts.failed_plan_delivery(&delivery)
        {
            self.finish_owned_read_failure(&delivery, error());
        }
    }

    #[inline(never)]
    pub(in super::super::super) fn fail_owned_admission(
        &mut self,
        admission: &Admission,
        reason: Error,
    ) {
        let Ok(Some(current)) = self
            .io
            .request_contexts
            .owned_admission(&admission.context, self.io.http_delivery.as_ref())
        else {
            return;
        };
        if current.attempt != admission.attempt {
            return;
        }
        if let Some(plan) = self
            .io
            .request_contexts
            .plan_from_context(&admission.context)
        {
            let error =
                VenueError::new("HTTP plan admission failed").kind(if reason == Error::TimedOut {
                    crate::ErrorKind::Timeout
                } else {
                    crate::ErrorKind::Internal
                });
            if admission.context.operation() == Operation::Mutation {
                let delivery = Delivery {
                    context: admission.context.clone(),
                    attempt: admission.attempt,
                };
                self.io
                    .request_contexts
                    .result(&delivery, WriteResult::Failed(error.clone()))
                    .expect("pre-enrolled selected write");
            }
            self.fail_plan(&plan, error)
                .expect("exact selected plan failure");
            return;
        }
        let handle = self
            .io
            .request_contexts
            .owner_from_context(&admission.context)
            .expect("owned admission");
        let state = self
            .io
            .request_contexts
            .preflight_state(&handle)
            .expect("owned admission");
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
        if let Some(handle) = self.io.request_contexts.failed_read_owner(delivery)
            && self.now.is_finite()
        {
            if self
                .io
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

    /// Check exact response authority before native/cache publication. A live
    /// response still requires native decoding and snapshot freshness checks.
    pub fn validate_preflight_delivery(
        &self,
        handle: &Preflight,
        context: &Context,
    ) -> Result<(), Error> {
        self.io.request_contexts.validate(context, self.now)?;
        let call = self.io.request_contexts.read_call(handle, context)?;
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) || !matches!(call.stage, Stage::Active { failed: false }) {
            return Err(Error::Revoked);
        }
        if self.now >= call.admission.deadline {
            return Err(Error::TimedOut);
        }
        let managed = self
            .io
            .managed_http
            .get(&delivery.attempt.request)
            .ok_or(Error::Revoked)?;
        if !managed.released_delivery(delivery) {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    /// Retire one validated native read. Only this successful handoff permits a
    /// subsequent mode redo, cached read completion or final plan admission.
    pub fn finish_preflight_call(
        &mut self,
        handle: &Preflight,
        context: &Context,
    ) -> Result<(), Error> {
        self.validate_preflight_delivery(handle, context)?;
        let delivery = self.io.http_delivery.clone().expect("validated delivery");
        self.io.retire_http_leg(context);
        self.io.request_contexts.contexts.remove(&context.leg());
        let state = self
            .io
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
        self.io.request_contexts.validate(context, self.now)?;
        let call = self.io.request_contexts.read_call(handle, context)?;
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::Revoked)?;
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
        let state = self.io.request_contexts.preflight_state(handle)?;
        if !state.call.is_empty() || !state.admits_handoff(self.io.http_delivery.as_ref()) {
            return Err(Error::Revoked);
        }
        if policy.operation != Operation::Read
            || policy.credential != state.policy.read_credential
            || terms.wait_ms != 0
            || terms.timeout_ms != policy.timeout_ms
            || (terms.attempts > 1 && policy.retry == RetryPolicy::Never)
        {
            return Err(Error::Invalid);
        }
        let window = terms.window(self.now, f64::MAX)?;
        Context::new(
            handle.owner(),
            1,
            Operation::Read,
            policy.credential.clone(),
            self.now,
            window.deadline(),
            policy.pacing,
        )?
        .with_retry(policy.retry)?;
        self.io.request_contexts.prune(self.now);
        if self.io.request_contexts.contexts.len() + self.io.request_contexts.reserved_contexts
            >= CAPACITY
        {
            return Err(Error::Capacity);
        }
        self.io
            .request_contexts
            .contexts
            .try_reserve(1 + self.io.request_contexts.reserved_contexts)
            .map_err(|_| Error::Capacity)?;
        self.io
            .request_contexts
            .preflight_state_mut(handle)?
            .call
            .try_reserve_exact(1)
            .map_err(|_| Error::Capacity)?;
        let count = u64::from(terms.attempts);
        let signed = policy.credential.is_some();
        if !self.io.next_id.available(1 + 3 * count)
            || (signed && !self.io.signing.available_ids(count))
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
        context.bind_issuer(&self.io.request_contexts.issuer);
        let signing = signed.then(|| {
            self.io
                .signing
                .reserve_ids(count)
                .expect("preflighted signing call")
        });
        let predecessor = self.io.http_delivery.clone();
        let mut call = Call {
            context: context.clone(),
            terms,
            window,
            ids,
            signing,
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
        call.select(self.now, predecessor)
            .expect("validated initial physical window");
        let state = self
            .io
            .request_contexts
            .preflight_state_mut(handle)
            .expect("validated owner");
        state.call.push(call);
        state.handoff = None;
        self.io
            .request_contexts
            .contexts
            .insert(context.leg(), context.clone());
        Ok(context)
    }
}
