//! Streamed serial Plans: one live call, continued from exact DATA custody,
//! with no declared length and no root wall-clock.
use super::*;
use crate::request_context::plan::{SerialCall, SerialPolicy, Slot};

impl<P> Cx<'_, P> {
    /// Admit a streamed serial Plan with its first call from a WriteAfterReads
    /// Preflight with a write credential.
    pub fn admit_serial_plan(
        &mut self,
        preflight: &Preflight,
        first: SerialCall,
        policy: SerialPolicy,
    ) -> Result<(Plan, Slot), Error> {
        let _ = (preflight, first, policy);
        Err(Error::State)
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
        let _ = (plan, slot, context, next);
        Err(Error::State)
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
        let _ = (plan, slot, context, delay_ms, next);
        Err(Error::State)
    }
}
