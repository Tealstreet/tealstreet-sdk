# Rust error handling lane (claude-errors)

Executes `RUST_QUALITY.md` "Error handling" and "Logging and observability".
Bridge one-error reporting, command deadlines and wasm trap containment land
from `feat/rust-bridge-fixes` (claude-bridge); this lane builds on them.

1. `VenueError` types and plumbing: `engine/src/error.rs`, `Cx::done` and
   `Cx::deactivate(error)`, wasm and native hosts send `detail` beside the
   unchanged `error` text; partial batches become `VenueError::partial`.
2. `StartupFailed{error}`: terminal startup and auth failure rejects `start()`
   (Bybit startup retry, `untilLoaded`), replacing the BreakoutProp prefix match.
3. TS `ExchangeError` from `detail` (after the bridge PR merges).
4. `outcome: Unknown` at unknown-outcome sites; `isUnknownOutcome` honours it.
5. Shared venue error parsers, one family per PR, replacing `malformed` flags.
6. Logging facade, then venue batches.
