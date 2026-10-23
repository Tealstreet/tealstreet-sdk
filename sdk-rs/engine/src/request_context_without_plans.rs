//! Compile-time empty Plan hooks for families without whole-plan admission.
//! Ordinary Context validation and terminal authority stay in the registry.
use super::*;

impl ContextRegistry {
    pub(super) fn reserved_context_count(&self) -> usize {
        0
    }

    pub(super) fn end_plan_admission(&mut self) {}

    pub(super) fn validate_legacy(&self, _context: &Context) -> Result<(), Error> {
        Ok(())
    }

    pub(super) fn validate_owned_context(&self, _context: &Context) -> Result<(), Error> {
        Ok(())
    }

    pub(super) fn record_owned_read_failure(&mut self, _delivery: &Delivery) -> bool {
        false
    }
}

impl<P> Cx<'_, P> {
    pub(super) fn with_owned_read_failure(
        &mut self,
        _error: impl FnOnce() -> VenueError,
        callback: impl FnOnce(&mut Self),
    ) {
        callback(self);
    }

    pub(super) fn finish_owned_read_failure(&mut self, _delivery: &Delivery, _error: VenueError) {}
}
