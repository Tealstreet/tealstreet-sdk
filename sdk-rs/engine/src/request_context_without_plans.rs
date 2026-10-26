//! Compile-time empty Plan hooks for families without whole-plan admission.
//! Ordinary Context validation and terminal authority stay in the registry.
use super::*;

impl ContextRegistry {
    pub(in super::super) fn reserved_context_count(&self) -> usize {
        0
    }

    pub(in super::super) fn end_plan_admission(&mut self) {}

    pub(in super::super) fn validate_legacy(&self, _context: &Context) -> Result<(), Error> {
        Ok(())
    }

    pub(in super::super) fn validate_owned_context(&self, _context: &Context) -> Result<(), Error> {
        Ok(())
    }

    pub(in super::super) fn record_owned_read_failure(&mut self, _delivery: &Delivery) -> bool {
        false
    }
}

impl<P> Cx<'_, P> {
    pub(in crate::engine) fn validate_preflight_ready_owner(
        &self,
        _owner: Owner,
    ) -> Result<(), Error> {
        Ok(())
    }

    pub(in super::super) fn with_owned_read_failure(
        &mut self,
        _error: impl FnOnce() -> VenueError,
        callback: impl FnOnce(&mut Self),
    ) {
        callback(self);
    }

    pub(in super::super) fn finish_owned_read_failure(
        &mut self,
        _delivery: &Delivery,
        _error: VenueError,
    ) {
    }
}

impl<V: crate::Venue> super::super::Engine<V> {
    pub(in crate::engine) fn flush_preflight_readiness(&mut self, _now: f64) {}
}
