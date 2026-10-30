//! Frame-fed engines: the browser's main-thread engine for one account.
//!
//! It holds the key and signs and sends every manual write itself, in one leg
//! on the account's route. Its account state arrives as frames from a data
//! source: the LOCAL worker, or the COSIGNED or DELEGATED exchange server. It
//! opens no socket, makes no account read or poll, and runs no reconcile reads;
//! it loads only its venue's public instrument metadata at start.
use crate::Command;

/// Refusal for a write that arrives before the venue's metadata loads.
pub const MARKETS_NOT_LOADED: &str = "markets not loaded";
/// Refusal for a command a frame-fed engine leaves to its data source.
pub const DATA_SOURCE_ONLY: &str = "a frameFed engine executes only writes";

/// How long after a fence was raised a source frame must have been written to
/// lift it: the venue's own propagation to the source's private stream.
pub const FENCE_GRACE_MS: f64 = 3_000.0;
/// The same grace when the host has not measured the source's clock offset:
/// it also absorbs that much clock skew between the source and this engine.
pub const UNSYNCED_FENCE_GRACE_MS: f64 = 30_000.0;

/// The commands a frame-fed engine executes itself.
pub fn is_write(command: &Command) -> bool {
    matches!(
        command,
        Command::PlaceOrders { .. }
            | Command::CancelOrders { .. }
            | Command::CancelOrdersWithSymbols { .. }
            | Command::AmendOrder { .. }
            | Command::AmendOrderWithSymbol { .. }
            | Command::CancelSymbolOrders { .. }
            | Command::SetLeverage { .. }
            | Command::SetMarginType { .. }
            | Command::SetPositionMode { .. }
    )
}

/// Why a frame-fed engine refuses `command`, if it does. A refreshMarkets
/// reload is the one non-write it runs.
pub fn refusal(command: &Command, markets_loaded: bool) -> Option<&'static str> {
    if is_write(command) {
        return (!markets_loaded).then_some(MARKETS_NOT_LOADED);
    }
    match command {
        Command::Venue {
            action: crate::VenueAction::RefreshMarkets,
            ..
        } => None,
        _ => Some(DATA_SOURCE_ONLY),
    }
}
