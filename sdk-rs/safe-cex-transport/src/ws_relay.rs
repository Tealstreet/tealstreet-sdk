use std::sync::atomic::{AtomicBool, Ordering};
// One gate per Rust process. JS has one per module realm; that boundary is CHOSEN.
static ENABLED: AtomicBool = AtomicBool::new(false);
pub fn set_ws_relay_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
}
pub fn is_ws_relay_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}
