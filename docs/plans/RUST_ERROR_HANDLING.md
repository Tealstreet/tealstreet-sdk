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

OKX terminal key/passphrase codes are 60005, 60024 and 60032 in the current
[primary WS error table](https://www.okx.com/docs-v5/en/#error-code-websocket-error-code).
60001/60003 are absent there and no longer trigger terminal key rejection.
Bitget's [primary WS table](https://www.bitget.com/docs/uta/error-code/websocket)
lists 30011/30012 for key/passphrase rejection. Only exact integer numeric tokens
or canonical quoted codes admit those meanings; fractional lookalikes cannot
be rounded into permanent credential failure. Generic signature/clock/rate and
unknown codes retain reconnect behavior. Established-account and success paths
are unchanged. Startup and reconnect regressions cover numeric/quoted near-integer
fractions; independent review is clear.
