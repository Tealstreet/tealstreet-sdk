# Rust error handling lane (claude-errors)

Executes `RUST_QUALITY.md` "Error handling". Logging moved to lane claude-8azlsz.
Bridge one-error reporting, command deadlines and wasm trap containment land
from `feat/rust-bridge-fixes` (claude-bridge); this lane builds on them.

1. `VenueError` types and plumbing: `engine/src/error.rs` and `Cx::fail`
   (`Notice::CommandFailed`), which wasm and native hosts send as the usual
   `done` with the unchanged `error` text plus `detail`. Partial batches
   (MEXC, Orderly, Polymarket) use `VenueError::partial`. `Cx::done` keeps its
   `Err(String)` path so open PRs compile; the last family-migration PR (5)
   removes it, leaving `Cx::fail` as the only failure path.
2. `StartupFailed{error}`: terminal startup and auth failure rejects `start()`
   (Bybit startup retry, `untilLoaded`), replacing the BreakoutProp prefix match.
3. TS `ExchangeError` from `detail` (after the bridge PR merges).
4. `outcome: Unknown` at unknown-outcome sites; `isUnknownOutcome` honours it.
5. Shared venue error parsers, one family per PR, replacing `malformed` flags.
