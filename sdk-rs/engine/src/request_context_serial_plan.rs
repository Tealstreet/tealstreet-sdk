//! Streamed serial Plans: one live call, continued from exact custody, with no
//! declared length and no root wall-clock.
use super::*;
use crate::request_context::plan::{CallAuth, CallTerms, RetryWindow, SerialCall, SerialPolicy};
use crate::request_context::{Credential, RetryPolicy};

/// A serial root never expires by clock; it ends through custody or a stop.
/// The Plan timer is armed only for a delayed continuation, never at ROOT.
pub(super) const ROOT: f64 = f64::MAX;
/// The live attempt plus its one classified resend.
pub(super) const JOURNAL_ATTEMPTS: usize = 2;

pub(super) struct Serial {
    pub(super) refusals_complete: bool,
    /// The current call, its `pacing` already resolved: `None` is unpaced.
    call: SerialCall,
    #[cfg(feature = "session-bearer")]
    pub(super) session_seed: Option<(u64, u64)>,
    #[cfg(feature = "session-bearer")]
    pub(super) session_root: Option<crate::request_context::Session>,
    #[cfg(feature = "session-bearer")]
    pub(super) wire_fingerprint: Option<[u8; 32]>,
    /// Read calls sign with the Preflight's read role (or go unsigned), never
    /// with the Plan's write credential.
    read: Option<Credential>,
    /// A pending condition wait: its generation and engine deadline (ROOT when
    /// unbounded). `waits` numbers every wait this Plan has taken.
    #[cfg(feature = "condition-waits")]
    pub(super) condition: Option<(u64, f64)>,
    #[cfg(feature = "condition-waits")]
    pub(super) waits: u64,
    /// A settle wait left no next entry: nothing more can be issued, and its
    /// release only readies the Plan (once, at the stored time) for `done_plan`.
    #[cfg(feature = "condition-waits")]
    pub(super) settled: bool,
    #[cfg(feature = "condition-waits")]
    pub(super) settle_ready: Option<f64>,
}

/// How a continuation's next call becomes eligible.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Next {
    Now,
    After(u32),
    /// Held on a venue condition until `release_condition_wait`.
    #[cfg(feature = "condition-waits")]
    Parked,
}

impl Next {
    #[cfg(feature = "condition-waits")]
    fn parks(self) -> bool {
        self == Self::Parked
    }
    #[cfg(not(feature = "condition-waits"))]
    fn parks(self) -> bool {
        false
    }
}

impl Serial {
    pub(super) fn session_call(&self) -> bool {
        self.call.auth == CallAuth::Session
    }

    fn has_session_root(&self) -> bool {
        #[cfg(feature = "session-bearer")]
        {
            self.session_root.is_some()
        }
        #[cfg(not(feature = "session-bearer"))]
        {
            false
        }
    }

    fn validate_auth(
        call: SerialCall,
        write: &Option<Credential>,
        session_root: bool,
    ) -> Result<(), Error> {
        match call.auth {
            CallAuth::Session if !cfg!(feature = "session-bearer") || !session_root => {
                Err(Error::Invalid)
            }
            CallAuth::Preflight if write.is_none() => Err(Error::Invalid),
            _ => Ok(()),
        }
    }

    #[cfg(feature = "session-bearer")]
    pub(super) fn allows_session_read_resend(&self) -> bool {
        self.call.operation == Operation::Read && self.call.attempts == 2
    }

    #[cfg(feature = "condition-waits")]
    pub(super) fn condition_deadline(&self) -> Option<f64> {
        self.condition.map(|(_, at)| at).filter(|at| *at < ROOT)
    }

    pub(super) fn terms(
        &self,
        write: &Option<Credential>,
    ) -> (Operation, RetryPolicy, Option<Credential>) {
        let credential = if self.session_call() {
            None
        } else {
            match self.call.operation {
                Operation::Read => self.read.clone(),
                Operation::Mutation => write.clone(),
            }
        };
        (self.call.operation, retry_policy(self.call), credential)
    }

    pub(super) fn pacing(&self) -> Option<crate::request_context::Pacing> {
        self.call.pacing
    }

    /// Whether `next` signs: a Read only under the read role.
    fn signs(&self, next: SerialCall, write: &Option<Credential>) -> bool {
        if next.auth == CallAuth::Session {
            return false;
        }
        match next.operation {
            Operation::Read => self.read.is_some(),
            Operation::Mutation => write.is_some(),
        }
    }
}

pub(super) fn wake(timer: TimerId, at_ms: f64) -> Action {
    if at_ms >= ROOT {
        Action::ClearTimer { timer }
    } else {
        Action::SetTimer { timer, at_ms }
    }
}

fn retry_policy(call: SerialCall) -> RetryPolicy {
    if call.attempts > 1 && call.operation == Operation::Mutation {
        RetryPolicy::DefinitiveRefusal
    } else {
        RetryPolicy::Never
    }
}

fn validate_call(call: SerialCall) -> Result<(), Error> {
    if call.timeout_ms == 0
        || !(1..=2).contains(&call.attempts)
        || (!cfg!(feature = "session-bearer")
            && call.operation == Operation::Read
            && call.attempts > 1)
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

impl crate::engine::IoCore {
    /// The call with the pacing its legs carry: its own, else the root's.
    /// Domain members resolve through the domain; any other call, including a
    /// Read in a Mutations domain, stays unpaced and names only zero-gap groups.
    fn resolve_serial_pacing(
        &self,
        root: Option<crate::request_context::Pacing>,
        call: SerialCall,
    ) -> Result<SerialCall, Error> {
        #[cfg(feature = "release-domains")]
        if self.domain_member(call.operation).is_some() {
            let pacing = self.domain_pacing(call.operation, call.pacing.or(root))?;
            return Ok(SerialCall { pacing, ..call });
        }
        #[cfg(feature = "release-domains")]
        let root = root.filter(|root| !self.domain_paces(Some(*root)));
        let pacing = call.pacing.or(root);
        if pacing.is_some_and(|pacing| pacing.minimum_gap_ms != 0.) {
            return Err(Error::Invalid);
        }
        self.http_pacing.validate_zero(pacing)?;
        Ok(SerialCall { pacing, ..call })
    }
}

impl<P> Cx<'_, P> {
    /// Admit a streamed serial Plan with its first call from a WriteAfterReads
    /// Preflight with a write credential.
    pub fn admit_serial_plan(
        &mut self,
        preflight: &Preflight,
        first: SerialCall,
        policy: SerialPolicy,
    ) -> Result<(Plan, Slot), Error> {
        if self
            .io
            .core
            .request_contexts
            .owner_entry(preflight.owner())
            .is_some_and(|owner| matches!(owner.phase, CommandPhase::Planned(_)))
        {
            let Owner::Command { id, .. } = preflight.owner() else {
                return Err(Error::Invalid);
            };
            // One Preflight admits one Plan; only its own entry learns that.
            return Err(if self.io.command_entry_id() == Some(id) {
                Error::State
            } else {
                Error::Revoked
            });
        }
        validate_call(first)?;
        let (intent, _) = self.preflight_plan_origin(preflight)?;
        #[cfg(feature = "session-bearer")]
        let session_root = self
            .io
            .core
            .request_contexts
            .keyless_session(preflight.owner())
            .cloned();
        Serial::validate_auth(first, &intent.write_credential, {
            #[cfg(feature = "session-bearer")]
            {
                session_root.is_some()
            }
            #[cfg(not(feature = "session-bearer"))]
            {
                false
            }
        })?;
        // A domain member is paced by its domain; any other serial Plan stays unpaced.
        #[cfg(feature = "release-domains")]
        let pacing = self
            .io
            .core
            .domain_pacing(Operation::Mutation, policy.pacing)?;
        #[cfg(not(feature = "release-domains"))]
        let pacing = policy.pacing;
        #[cfg(feature = "release-domains")]
        let in_domain = self.io.core.domain_paces(pacing);
        #[cfg(not(feature = "release-domains"))]
        let in_domain = false;
        if !in_domain {
            if pacing.is_some_and(|pacing| pacing.minimum_gap_ms != 0.) {
                return Err(Error::Invalid);
            }
            self.io.core.http_pacing.validate_zero(pacing)?;
        }
        let first = self.io.core.resolve_serial_pacing(pacing, first)?;
        let schedule = Schedule::from_single_call(CallTerms {
            wait_ms: 0,
            timeout_ms: first.timeout_ms,
            attempts: first.attempts.into(),
            retry_window: RetryWindow::FreshAttempt,
        });
        let (plan, _) = self.admit_plan_root(
            preflight,
            PreparedAdmission {
                schedule,
                policy: PlanPolicy {
                    independent_calls: false,
                    operation: Operation::Mutation,
                    credential: policy.credential,
                    pacing,
                    retry: retry_policy(first),
                },
                terminals: Ok(Vec::new()),
                has_barriers: false,
                #[cfg(feature = "http-mutation-barriers")]
                barriers: Ok(Vec::new()),
                #[cfg(feature = "session-bearer")]
                session: None,
            },
            false,
            Some(Serial {
                refusals_complete: policy.refusals_complete,
                call: first,
                #[cfg(feature = "session-bearer")]
                session_seed: None,
                #[cfg(feature = "session-bearer")]
                session_root,
                #[cfg(feature = "session-bearer")]
                wire_fingerprint: None,
                read: intent.read_credential,
                #[cfg(feature = "condition-waits")]
                condition: None,
                #[cfg(feature = "condition-waits")]
                waits: 0,
                #[cfg(feature = "condition-waits")]
                settled: false,
                #[cfg(feature = "condition-waits")]
                settle_ready: None,
            }),
        )?;
        let slot = self
            .io
            .core
            .request_contexts
            .plan_body(&plan)
            .expect("admitted serial Plan")
            .schedule
            .available()
            .next()
            .expect("first serial call");
        Ok((plan, slot))
    }

    /// In the current call's exact DATA custody: settle it and admit the next
    /// call under the same root.
    pub fn continue_serial_plan(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        next: SerialCall,
    ) -> Result<Slot, Error> {
        self.continue_serial(plan, slot, context, next, Next::Now)
    }

    /// As `continue_serial_plan`, but the next call is issued from the Plan
    /// timer's `on_http_plan_ready` after `delay_ms`. It is also the one Read
    /// failure retry: from the current Read call's own failure, Read next only.
    pub fn continue_serial_plan_after(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        delay_ms: u32,
        next: SerialCall,
    ) -> Result<(), Error> {
        if delay_ms == 0 {
            return Err(Error::Invalid);
        }
        self.continue_serial(plan, slot, context, next, Next::After(delay_ms))
            .map(drop)
    }

    #[inline]
    pub(super) fn continue_serial(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        next: SerialCall,
        wake: Next,
    ) -> Result<Slot, Error> {
        self.core().continue_serial(plan, slot, context, next, wake)
    }

    #[inline]
    // Only some feature combinations call this.
    #[cfg_attr(feature = "serial-plans", allow(dead_code))]
    fn commit_serial_continuation(
        &mut self,
        plan: &Plan,
        context: &Context,
        leg: Option<u64>,
        next: SerialCall,
        schedule: Schedule,
        wake: Next,
    ) -> Result<Slot, Error> {
        self.core()
            .commit_serial_continuation(plan, context, leg, next, schedule, wake)
    }
}

impl crate::engine::CoreCx<'_> {
    // Every check precedes the first change: a refusal leaves the current call
    // settleable exactly as it was.
    #[inline(never)]
    pub(super) fn continue_serial(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        next: SerialCall,
        wake: Next,
    ) -> Result<Slot, Error> {
        validate_call(next)?;
        #[cfg(feature = "session-bearer")]
        self.io
            .core
            .request_contexts
            .validate_keyless_session(plan.owner(), &self.io.core.identities)?;
        let body = self
            .io
            .core
            .request_contexts
            .validate_plan(plan, self.now)?;
        let serial = body.serial.as_ref().ok_or(Error::Invalid)?;
        #[cfg(feature = "condition-waits")]
        if serial.settled {
            return Err(Error::Invalid);
        }
        Serial::validate_auth(next, &body.policy.credential, serial.has_session_root())?;
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        let delivery = self.io.core.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) {
            return Err(Error::Revoked);
        }
        match call.stage {
            Stage::Active { failed: false } => self.validate_plan_delivery(plan, context)?,
            // A released Read's failure retries once more as a fresh Read; no
            // DATA or write fact is invented for it.
            Stage::Active { failed: true }
                if !wake.parks() && context.operation() == Operation::Read =>
            {
                if wake == Next::Now && call.released_failure.is_none() {
                    return Err(Error::Revoked);
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
                if next.operation != Operation::Read {
                    return Err(Error::Invalid);
                }
            }
            _ => return Err(Error::Revoked),
        }
        let leg = (context.operation() == Operation::Mutation).then(|| context.leg());
        self.io
            .core
            .request_contexts
            .owner_entry(plan.owner())
            .and_then(|owner| owner.writes.as_ref())
            .ok_or(Error::State)?
            .validate_serial_continuation(leg)?;
        let attempts = u64::from(next.attempts);
        let signed = body
            .serial
            .as_ref()
            .expect("checked serial Plan")
            .signs(next, &body.policy.credential);
        let next = self
            .io
            .core
            .resolve_serial_pacing(body.policy.pacing, next)?;
        if !self.io.core.next_id.available(1 + 3 * attempts)
            || (signed && !self.io.core.signing.available_ids(attempts))
        {
            return Err(Error::Capacity);
        }
        let schedule = Schedule::from_single_call(CallTerms {
            timeout_ms: next.timeout_ms,
            attempts: next.attempts.into(),
            retry_window: RetryWindow::FreshAttempt,
            wait_ms: match wake {
                Next::After(delay_ms) => delay_ms,
                Next::Now => 0,
                #[cfg(feature = "condition-waits")]
                Next::Parked => 0,
            },
        })?;
        self.commit_serial_continuation(plan, context, leg, next, schedule, wake)
    }

    fn commit_serial_continuation(
        &mut self,
        plan: &Plan,
        context: &Context,
        leg: Option<u64>,
        next: SerialCall,
        schedule: Schedule,
        wake: Next,
    ) -> Result<Slot, Error> {
        self.io.retire_http_leg(context);
        self.io
            .core
            .request_contexts
            .contexts
            .remove(&context.leg());
        if let Some(leg) = leg {
            let owner = self
                .io
                .core
                .request_contexts
                .owner_entry_mut(plan.owner())
                .expect("validated serial owner");
            owner
                .writes
                .as_mut()
                .expect("validated serial journal")
                .compact_serial(leg);
            owner
                .leg_intents
                .retain(|(intent_leg, _)| *intent_leg != leg);
        }
        let attempts = u64::from(next.attempts);
        let mut ids = self
            .io
            .reserve_ids(1 + 3 * attempts)
            .expect("checked serial call identities");
        let logical = take(&mut ids, 1).start;
        let call_ids = take(&mut ids, 3 * attempts);
        let body = self
            .io
            .core
            .request_contexts
            .plan_body(plan)
            .expect("validated serial Plan");
        let signed = body
            .serial
            .as_ref()
            .expect("validated serial Plan")
            .signs(next, &body.policy.credential);
        let signing = signed.then(|| {
            self.io
                .core
                .signing
                .reserve_ids(attempts)
                .expect("checked serial signing identities")
        });
        let terms = schedule
            .declarations()
            .next()
            .expect("one declared serial call");
        let handoff = self.io.core.http_delivery.clone();
        let now = self.now;
        let body = self.io.core.request_contexts.plan_body_mut(plan)?;
        body.schedule = schedule;
        body.entries[0] = Entry {
            terms,
            logical,
            ids: call_ids,
            signing,
            credit: Credit::Unissued,
            call: None,
            eligible_at: match wake {
                Next::Now => Some(eligibility(now, 0, ROOT)),
                Next::After(delay_ms) => Some(eligibility(now, delay_ms, ROOT)),
                #[cfg(feature = "condition-waits")]
                Next::Parked => None,
            },
            // An immediate call is issued in this custody; a delayed one
            // wakes the venue through the Plan timer, a parked one on release.
            notified: wake == Next::Now,
            parked: wake.parks(),
        };
        body.handoff = handoff;
        body.serial.as_mut().expect("validated serial Plan").call = next;
        #[cfg(feature = "session-bearer")]
        {
            body.serial
                .as_mut()
                .expect("validated serial Plan")
                .session_seed = None;
        }
        #[cfg(feature = "session-bearer")]
        {
            body.serial.as_mut().expect("serial call").wire_fingerprint = None;
        }
        let slot = body.schedule.available().next().expect("next serial call");
        self.io.core.request_contexts.reserved_contexts += 1;
        self.rearm_plan(plan);
        Ok(slot)
    }
}

#[cfg(feature = "session-bearer")]
impl<P> Cx<'_, P> {
    /// Select the current serial call's one authentication resend. Its bundle
    /// floor is the refused attempt's bound revision, never the cache's now.
    pub fn serial_plan_session_resend(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        evidence: crate::request_context::SessionResend,
    ) -> Result<Context, Error> {
        let (context, seed, _) = self.select_session_resend(
            plan,
            slot,
            context,
            evidence,
            super::session_plan::ResendRoot::Serial,
        )?;
        self.io
            .core
            .request_contexts
            .plan_body_mut(plan)
            .expect("selected serial Plan")
            .serial
            .as_mut()
            .expect("validated serial Plan")
            .session_seed = Some(seed);
        Ok(context)
    }
}
