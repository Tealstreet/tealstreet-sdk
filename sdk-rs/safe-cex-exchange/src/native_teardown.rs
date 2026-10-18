//! Deterministic native handle destruction, independent of source dispose overrides.
use crate::{exchange::BaseExchange, runtime::ExchangeError};

/// Silence final cleanup errors and complete shared teardown even if venue cleanup fails.
///
/// Every native venue `Drop` calls this operation. The venue closure clears its
/// own state and tasks; the core closes shared work, signers, sockets, listeners
/// and Store. This native lifetime policy is not a source `super.dispose()` call.
pub fn native_final_drop(
    base: &BaseExchange,
    venue_cleanup: impl FnOnce() -> Result<(), ExchangeError>,
) {
    base.set_disposed_flag(true);
    let _ = venue_cleanup();
    let _ = base.dispose();
}
