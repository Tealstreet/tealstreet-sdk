//! Streamed serial Plans: one live call, continued from exact custody, with no
//! declared length and no root wall-clock.
use super::*;
use crate::request_context::plan::{CallBound, RetryWindow, SerialCall, SerialPolicy, StepBound};
use crate::request_context::{Credential, RetryPolicy};

/// A serial root never expires by clock; it ends through custody or a stop.
/// The Plan timer is armed only for a delayed continuation, never at ROOT.
pub(super) const ROOT: f64 = f64::MAX;
/// The live attempt plus its one classified resend.
pub(super) const JOURNAL_ATTEMPTS: usize = 2;

pub(super) struct Serial {
    pub(super) refusals_complete: bool,
    call: SerialCall,
    /// Read calls sign with the Preflight's read role (or go unsigned), never
    /// with the Plan's write credential.
    read: Option<Credential>,
}

impl Serial {
    pub(super) fn terms(
        &self,
        write: &Option<Credential>,
    ) -> (Operation, RetryPolicy, Option<Credential>) {
        let credential = match self.call.operation {
            Operation::Read => self.read.clone(),
            Operation::Mutation => write.clone(),
        };
        (self.call.operation, retry_policy(self.call), credential)
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
    if call.attempts > 1 {
        RetryPolicy::DefinitiveRefusal
    } else {
        RetryPolicy::Never
    }
}

fn validate_call(call: SerialCall) -> Result<(), Error> {
    if call.timeout_ms == 0
        || !(1..=2).contains(&call.attempts)
        || (call.attempts > 1 && call.operation != Operation::Mutation)
    {
        return Err(Error::Invalid);
    }
    Ok(())
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
        if policy
            .pacing
            .is_some_and(|pacing| pacing.minimum_gap_ms != 0.)
        {
            return Err(Error::Invalid);
        }
        self.io.http_pacing.validate_zero(policy.pacing)?;
        let calls = [CallBound {
            timeout_ms: first.timeout_ms,
            attempts: first.attempts.into(),
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        }];
        let steps = [StepBound {
            wait_ms: 0,
            calls: &calls,
        }];
        let (plan, _) = self.admit_plan_root(
            preflight,
            AdmissionDeclaration {
                steps: &steps,
                policy: PlanPolicy {
                    independent_calls: false,
                    operation: Operation::Mutation,
                    credential: Some(policy.credential),
                    pacing: policy.pacing,
                    retry: retry_policy(first),
                },
                terminals: &[],
                barriers: &[],
            },
            false,
            Some(Serial {
                refusals_complete: policy.refusals_complete,
                call: first,
                read: intent.read_credential,
            }),
        )?;
        let slot = self
            .io
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
        self.continue_serial(plan, slot, context, next, None)
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
        self.continue_serial(plan, slot, context, next, Some(delay_ms))
            .map(drop)
    }

    // Every check precedes the first change: a refusal leaves the current call
    // settleable exactly as it was.
    #[inline(never)]
    fn continue_serial(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        next: SerialCall,
        delay_ms: Option<u32>,
    ) -> Result<Slot, Error> {
        validate_call(next)?;
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        if body.serial.is_none() {
            return Err(Error::Invalid);
        }
        let index = body.schedule.ordinal(slot)?;
        let call = body.entries[index]
            .call
            .as_ref()
            .filter(|call| call.context == *context)
            .ok_or(Error::Revoked)?;
        let delivery = self.io.http_delivery.as_ref().ok_or(Error::Revoked)?;
        if !call.matches(delivery) {
            return Err(Error::Revoked);
        }
        match call.stage {
            Stage::Active { failed: false } => self.validate_plan_delivery(plan, context)?,
            // A released Read's failure retries once more as a fresh Read; no
            // DATA or write fact is invented for it.
            Stage::Active { failed: true }
                if delay_ms.is_some() && context.operation() == Operation::Read =>
            {
                if !self
                    .io
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
            .request_contexts
            .owner_entry(plan.owner())
            .and_then(|owner| owner.writes.as_ref())
            .ok_or(Error::State)?
            .validate_serial_continuation(leg)?;
        let attempts = u64::from(next.attempts);
        let signed = body.policy.credential.is_some();
        if !self.io.next_id.available(1 + 3 * attempts)
            || (signed && !self.io.signing.available_ids(attempts))
        {
            return Err(Error::Capacity);
        }
        let calls = [CallBound {
            timeout_ms: next.timeout_ms,
            attempts: next.attempts.into(),
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &[],
        }];
        let steps = [StepBound {
            wait_ms: delay_ms.unwrap_or(0),
            calls: &calls,
        }];
        let schedule = Schedule::from_steps(&steps)?;
        self.commit_serial_continuation(plan, context, leg, next, schedule, delay_ms)
    }

    fn commit_serial_continuation(
        &mut self,
        plan: &Plan,
        context: &Context,
        leg: Option<u64>,
        next: SerialCall,
        schedule: Schedule,
        delay_ms: Option<u32>,
    ) -> Result<Slot, Error> {
        self.io.retire_http_leg(context);
        self.io.request_contexts.contexts.remove(&context.leg());
        if let Some(leg) = leg {
            let owner = self
                .io
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
        let signing = self
            .io
            .request_contexts
            .plan_body(plan)
            .expect("validated serial Plan")
            .policy
            .credential
            .is_some()
            .then(|| {
                self.io
                    .signing
                    .reserve_ids(attempts)
                    .expect("checked serial signing identities")
            });
        let terms = schedule
            .declarations()
            .next()
            .expect("one declared serial call");
        let handoff = self.io.http_delivery.clone();
        let now = self.now;
        let body = self.io.request_contexts.plan_body_mut(plan)?;
        body.schedule = schedule;
        body.entries[0] = Entry {
            terms,
            logical,
            ids: call_ids,
            signing,
            credit: Credit::Unissued,
            call: None,
            eligible_at: Some(eligibility(now, delay_ms.unwrap_or(0), ROOT)),
            // An immediate call is issued in this custody; a delayed one
            // wakes the venue through the Plan timer.
            notified: delay_ms.is_none(),
            parked: false,
        };
        body.handoff = handoff;
        body.serial.as_mut().expect("validated serial Plan").call = next;
        let slot = body.schedule.available().next().expect("next serial call");
        self.io.request_contexts.reserved_contexts += 1;
        self.rearm_plan(plan);
        Ok(slot)
    }
}
