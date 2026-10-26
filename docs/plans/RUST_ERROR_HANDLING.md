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

## Bitunix follow-up (2026-10-21)

The subsequently landed Bitunix port returned bare command failures.
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

Implemented one classified command failure path with a strict integer native
code decoder. Native network error 10001 and undocumented codes remain Unknown
on writes; documented key/permission/rate/input/business refusals retain their
codes and messages. HTTP status owns classification even when its body carries
a different native error. Malformed batch peers preserve observed IDs and
confirmed cancellations, including malformed ancillary fields and incomplete
acknowledgement counts. Independent later batch legs retain their existing
progression, then finish once with partial IDs. A successful single code-zero
acknowledgement retains the existing generated/client-ID identity fallback.

The two baseline smoke regressions reproduced missing preflight/transport
classification. Focused regressions cover status precedence, malformed native
codes/acknowledgements, partial effects, sticky Unknown and recovery fencing.
Independent adversarial review found and resolved three acknowledgement
preservation gaps. No automatic retries or caller changes are introduced.

Validation: 359 Bitunix/Binance native tests plus the final focused 13
regressions, touched fmt, strict Clippy and bans pass.

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
