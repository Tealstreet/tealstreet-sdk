//! Independent complete Plans on a login Session root: each call declares its
//! own auth, so cancels and protective siblings keep going past an uncertain one.
use super::*;
use crate::request_context::Session;
use crate::request_context::plan::CallAuth;

/// The Plan's root and its entries' declared auth, frozen wires and resend
/// seeds, all indexed by schedule ordinal.
pub(super) struct SessionLegs {
    pub(super) root: Session,
    auth: Vec<CallAuth>,
    pub(super) wire: Vec<Option<[u8; 32]>>,
    seed: Vec<Option<(u64, u64)>>,
}

impl SessionLegs {
    pub(super) fn session_call(&self, index: usize) -> bool {
        self.auth[index] == CallAuth::Session
    }
}

/// Which Session root a resend selects under.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ResendRoot {
    Serial,
    Complete,
}

/// Without fallbacks, schedule ordinals run steps in order, calls within each.
fn declared_auth(steps: &[StepBound<'_>], auth: &[&[CallAuth]]) -> Result<Vec<CallAuth>, Error> {
    if auth.len() != steps.len() {
        return Err(Error::Invalid);
    }
    let mut flat = Vec::new();
    for (step, calls) in steps.iter().zip(auth) {
        if calls.len() != step.calls.len()
            || step.calls.iter().any(|call| !call.after_refusal.is_empty())
        {
            return Err(Error::Invalid);
        }
        flat.try_reserve(calls.len()).map_err(|_| Error::Capacity)?;
        flat.extend_from_slice(calls);
    }
    Ok(flat)
}

fn empty<T: Clone>(count: usize, value: T) -> Result<Vec<T>, Error> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::Capacity)?;
    values.resize(count, value);
    Ok(values)
}

impl<P> Cx<'_, P> {
    /// Admit an independent Mutation Plan on this Preflight's live login
    /// Session root. `auth[i][j]` declares `steps[i].calls[j]`: a Session call
    /// sends unsigned on the root's egress, a Preflight call signs with the
    /// write credential. Fallback steps are refused.
    pub fn admit_session_plan(
        &mut self,
        preflight: &Preflight,
        steps: &[StepBound<'_>],
        auth: &[&[CallAuth]],
        policy: PlanPolicy,
    ) -> Result<Plan, Error> {
        let auth = declared_auth(steps, auth)?;
        if !policy.independent_calls || policy.operation != Operation::Mutation {
            return Err(Error::Invalid);
        }
        if policy.credential.is_none() && auth.contains(&CallAuth::Preflight) {
            return Err(Error::Invalid);
        }
        let registry = &self.io.core.request_contexts;
        let root = registry
            .keyless_session(preflight.owner())
            .ok_or(Error::Invalid)?
            .clone();
        registry.validate_keyless_session(preflight.owner(), &self.io.core.identities)?;
        let count = auth.len();
        let session = SessionLegs {
            root,
            auth,
            wire: empty(count, None)?,
            seed: empty(count, None)?,
        };
        let mut prepared = self.prepare_admission(AdmissionDeclaration {
            steps,
            policy,
            terminals: &[],
            barriers: &[],
        })?;
        prepared.session = Some(session);
        self.admit_plan_root(preflight, prepared, false, None)
            .map(|(plan, _)| plan)
    }

    /// Select a complete Plan Session call's one authentication resend: the
    /// same body, reread under the refused attempt's bound revision.
    pub fn plan_session_resend(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        evidence: crate::request_context::SessionResend,
    ) -> Result<Context, Error> {
        let (context, seed, index) =
            self.select_session_resend(plan, slot, context, evidence, ResendRoot::Complete)?;
        self.io
            .core
            .request_contexts
            .plan_body_mut(plan)
            .expect("selected Session Plan")
            .session
            .as_mut()
            .expect("validated Session Plan")
            .seed[index] = Some(seed);
        Ok(context)
    }

    /// The shared resend rules: an immutable body, the current Session
    /// revision and no third send. The caller stores the returned seed.
    pub(super) fn select_session_resend(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        evidence: crate::request_context::SessionResend,
        root: ResendRoot,
    ) -> Result<(Context, (u64, u64), usize), Error> {
        self.validate_plan_delivery(plan, context)?;
        let registry = &self.io.core.request_contexts;
        registry.validate_keyless_session(plan.owner(), &self.io.core.identities)?;
        let body = registry.validate_plan(plan, self.now)?;
        let rooted = match root {
            ResendRoot::Serial => body.serial.is_some(),
            ResendRoot::Complete => body.session.is_some(),
        };
        if !rooted {
            return Err(Error::State);
        }
        let index = body.schedule.ordinal(slot)?;
        if body
            .session
            .as_ref()
            .is_some_and(|session| !session.session_call(index))
        {
            return Err(Error::Invalid);
        }
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        if call.terms.attempts != 2 || call.selected >= 2 {
            return Err(Error::RetryForbidden);
        }
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) {
            return Err(Error::Revoked);
        }
        let seed = self.io.serial_session_revision(delivery)?;
        match evidence {
            crate::request_context::SessionResend::Write(grant)
                if context.operation() == Operation::Mutation =>
            {
                self.http_resend_context(context, grant)?;
            }
            crate::request_context::SessionResend::Read
                if context.operation() == Operation::Read => {}
            _ => return Err(Error::Invalid),
        }
        let context = self.select_plan_retry(plan, slot, context)?;
        Ok((context, seed, index))
    }
}

impl ContextRegistry {
    /// A selected Session resend's bound generation and revision floor.
    pub(in super::super::super) fn plan_session_seed(
        &self,
        context: &Context,
    ) -> Option<(u64, u64)> {
        let plan = self.plan_from_context(context)?;
        let body = self.plan_body(&plan).ok()?;
        match body.session.as_ref() {
            Some(session) => session.seed[body.entry_of(context)?],
            None => body.serial.as_ref()?.session_seed,
        }
    }
}
