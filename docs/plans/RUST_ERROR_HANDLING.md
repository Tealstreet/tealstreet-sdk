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

## Bitunix follow-up plan (2026-10-21)

The subsequently landed Bitunix port still returns bare command failures.
Add one strict native-code parser and classified failures while retaining native
message text. HTTP status takes precedence over body codes; dispatched writes
with transport loss, HTTP 408/5xx or malformed acknowledgement remain Unknown.
Preflight/signing failures are NotSent. Read-only replacement reconciliation
must not imply that a new write was sent.

Track effective legs separately from returned order IDs, clear aggregate
Rejected/NotSent after an effective leg, and retain Unknown. Batch failures
must use one `Cx::fail` with accepted IDs rather than logs followed by success.
Only definitive rejection may enter existing recovery/write fallbacks.

Regress preflight, transport and HTTP/native precedence, malformed codes,
partial batches/cancels, successful settings before failure and recovery races.
Run focused Bitunix/Binance tests, touched fmt/strict Clippy and bans; obtain an
independent review, then land on green CI under the lane's merge guard.
