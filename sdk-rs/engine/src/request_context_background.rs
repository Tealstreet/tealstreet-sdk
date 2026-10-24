//! Opaque scheduling custody for finite Read Plans under a Session lifetime.
use super::Session;
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct BackgroundTaskPolicy {
    pub calls: u16,
    pub debounce_ms: f64,
    pub refresh_debounce_ms: f64,
}

/// A scheduling identity, not Context, send or completion authority.
#[derive(Clone, Debug)]
pub struct BackgroundTask {
    pub(crate) issuer: Arc<()>,
    pub(crate) parent: Session,
    pub(crate) slot: usize,
    pub(crate) generation: u64,
}

impl BackgroundTask {
    pub fn same_task(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.issuer, &other.issuer)
            && self.parent == other.parent
            && self.slot == other.slot
            && self.generation == other.generation
    }
}

/// Only its exact readiness callback may consume this one-use loan.
pub struct BackgroundActivation {
    pub(crate) task: BackgroundTask,
    pub(crate) attempt: crate::read_scheduler::Attempt,
}
