//! Account-wide physical release budgets; no venue-owned timers or retry authority.
use safe_cex_engine::request_context::{Pacing, PacingSet};

/// Restore the legacy transport's maxRPS=5 for single writes.
pub(crate) const WRITE: Pacing = Pacing {
    group: 2,
    minimum_gap_ms: 200.0,
};
/// A whole batch may consume ten orders; different commands share that budget.
pub(crate) const BATCH: Pacing = Pacing {
    group: 3,
    minimum_gap_ms: 1200.0,
};
pub(crate) const BATCH_SET: PacingSet = PacingSet {
    group: 4,
    constraints: &[WRITE, BATCH],
};
pub(crate) const BATCH_WRITE: Pacing = Pacing {
    group: 4,
    minimum_gap_ms: 0.0,
};
pub(crate) const ADMITTED: [Pacing; 2] = [WRITE, BATCH_WRITE];
