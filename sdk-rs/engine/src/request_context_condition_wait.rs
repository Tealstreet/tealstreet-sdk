//! Condition waits: park a WriteAfterReads Preflight or a serial Plan on a
//! venue condition, released from any callback, with an optional engine bound.
use super::*;
use crate::request_context::{ConditionWait, plan::Slot};

impl<P> Cx<'_, P> {
    /// Park a Preflight in genuine entry or Preflight-ready custody. `None` is
    /// unbounded; `Some(ms)` expires into `on_condition_wait_expired`. Release
    /// through `request_preflight_ready`.
    pub fn park_preflight_for(
        &mut self,
        preflight: &Preflight,
        bound_ms: Option<u32>,
    ) -> Result<(), Error> {
        let _ = (preflight, bound_ms);
        Err(Error::State)
    }

    /// Between serial calls, from the current call's exact custody under the
    /// checks of `continue_serial_plan`: settle the call and park the Plan.
    pub fn serial_plan_condition_wait(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        bound_ms: Option<u32>,
    ) -> Result<ConditionWait, Error> {
        let _ = (plan, slot, context, bound_ms);
        Err(Error::State)
    }

    /// From any callback. Only schedules the Plan's `on_http_plan_ready`.
    pub fn release_condition_wait(&mut self, wait: &ConditionWait) -> Result<(), Error> {
        let _ = wait;
        Err(Error::State)
    }
}
