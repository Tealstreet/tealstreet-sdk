//! Condition waits for serial Plans: park between calls on a venue condition,
//! released from any callback, with an optional engine bound.
use super::serial::{Next, ROOT};
use super::*;
use crate::engine::{Engine, Venue};
use crate::request_context::{
    ConditionOwner, ConditionWait,
    plan::{SerialCall, Slot},
};

impl<P> Cx<'_, P> {
    /// Between serial calls, from the current call's exact custody under the
    /// checks of `continue_serial_plan`: settle the call and park the Plan.
    /// `next` is the call issued after release (a replan picks it here).
    pub fn serial_plan_condition_wait(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        next: SerialCall,
        bound_ms: Option<u32>,
    ) -> Result<ConditionWait, Error> {
        if bound_ms == Some(0) {
            return Err(Error::Invalid);
        }
        let body = self.io.request_contexts.validate_plan(plan, self.now)?;
        let serial = body.serial.as_ref().ok_or(Error::Invalid)?;
        if serial.condition.is_some() {
            return Err(Error::State);
        }
        self.continue_serial(plan, slot, context, next, Next::Parked)?;
        let deadline = bound_ms.map_or(ROOT, |ms| self.now + f64::from(ms));
        let serial = self
            .io
            .request_contexts
            .plan_body_mut(plan)?
            .serial
            .as_mut()
            .expect("parked serial Plan");
        serial.waits += 1;
        let wait = serial.waits;
        serial.condition = Some((wait, deadline));
        self.rearm_plan(plan);
        Ok(ConditionWait {
            plan: plan.clone(),
            wait,
        })
    }

    /// From any callback. Only schedules the Plan's `on_http_plan_ready`, after
    /// the current callback returns; a handle releases once.
    pub fn release_condition_wait(&mut self, wait: &ConditionWait) -> Result<(), Error> {
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        let now = self.now;
        let body = self.io.request_contexts.plan_body_mut(&wait.plan)?;
        let serial = body.serial.as_mut().ok_or(Error::Revoked)?;
        if serial.condition.map(|(own, _)| own) != Some(wait.wait) {
            return Err(Error::Revoked);
        }
        serial.condition = None;
        let entry = &mut body.entries[0];
        entry.parked = false;
        entry.eligible_at = Some(now);
        entry.notified = false;
        self.rearm_plan(&wait.plan);
        Ok(())
    }
}

impl<V: Venue> Engine<V> {
    /// A parked serial Plan's engine bound passed: the venue fails it in
    /// failure custody, or the engine seals it once with its projection.
    #[inline(never)]
    pub(in crate::engine) fn expire_plan_condition(&mut self, now: f64, plan: &Plan) {
        let Ok(body) = self.io.request_contexts.plan_body_mut(plan) else {
            return;
        };
        let Some(serial) = body.serial.as_mut() else {
            return;
        };
        if !serial.condition.is_some_and(|(_, at)| at <= now) {
            return;
        }
        serial.condition = None;
        let error = VenueError::new("Condition wait expired").kind(crate::ErrorKind::Timeout);
        self.venue.on_condition_wait_expired(
            &mut Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            },
            &ConditionOwner::Plan(plan.clone()),
            error.clone(),
        );
        if self
            .io
            .request_contexts
            .plan_body(plan)
            .is_ok_and(|body| body.entries[0].parked)
        {
            Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            }
            .fail_plan_owned(plan, error)
            .expect("expired condition-parked Plan");
        }
    }
}
