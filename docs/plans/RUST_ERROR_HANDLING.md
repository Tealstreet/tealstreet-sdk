# Rust error handling lane (claude-errors)

Executes `RUST_QUALITY.md` "Error handling". Logging moved to lane claude-8azlsz.
Bridge one-error reporting, command deadlines and wasm trap containment land
from `feat/rust-bridge-fixes` (claude-bridge); this lane builds on them.

1. `VenueError` types and plumbing: `engine/src/error.rs` and `Cx::fail`
   (`Notice::CommandFailed`), which wasm and native hosts send as the usual
   `done` with the unchanged `error` text plus `detail`. Partial batches
   (MEXC, Orderly, Polymarket) use `VenueError::partial`. `Cx::done` is
   success-only; `Cx::fail` is the only failure path.
2. `StartupFailed{error}`: terminal startup and auth failure rejects `start()`
   (Bybit startup retry, `untilLoaded`), replacing the BreakoutProp prefix match.
3. TS `ExchangeError` from `detail` (after the bridge PR merges).
4. `outcome: Unknown` at unknown-outcome sites; `isUnknownOutcome` honours it.
5. Shared venue error parsers, one family per PR, replacing `malformed` flags.

All five landed on `feat/rust-rewrite` (2026-10-21).

## WebSocket login-code verification (2026-10-21)

Plan: verify OKX and Bitget terminal startup login codes against current primary
venue tables. Remove obsolete unverified OKX terminal codes; reject fractional
Bitget lookalikes before floating-point conversion. Preserve transient retry,
clock/signature handling, established accounts and native diagnostics. Add
startup/reconnect regressions, run both venues plus Binance and strict gates,
and obtain independent review before green CI and merge.
