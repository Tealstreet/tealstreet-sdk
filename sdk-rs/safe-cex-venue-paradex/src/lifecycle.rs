//! Exchange disposal marks state before calling either websocket.
use std::cell::Cell;

// ts: ParadexExchange.dispose
pub fn dispose<E>(
    is_disposed: &Cell<bool>,
    dispose_public: impl FnOnce() -> Result<(), E>,
    dispose_private: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    is_disposed.set(true);
    dispose_public()?;
    dispose_private()?;
    Ok(())
}
