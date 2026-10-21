# Bybit linear port checkpoint

Target: `feat/rust-rewrite`; lane: `feat/rust-bybit`. Original port: `332622a5ff`.

## Phases

1. Merge the rewrite layout, move the crate into `sdk-rs/venues/bybit`,
   preserve the shared engine/harness changes, and audit the paths.
   Verify bans, Bybit fmt/clippy/tests, wasm clippy, release wasm build,
   new Bybit public recording/fixture and private replay, and both Binance replays.
2. Push and open the PR into `feat/rust-rewrite`. Run independent adversarial
   reviews of account/trading and public/engine/harness behavior. Post the
   findings, fix actionable issues with regression coverage, rerun affected
   gates, and repeat reviews until passing or diminishing returns.
3. Verify CI status, merge into `feat/rust-rewrite`, then cut a fresh follow-up branch.

## Progress

- Original public/private/trading/query implementation is complete.
- Merged layout locally; resolved the workspace conflict using `venues/*`
  and the Bybit dependency at `venues/bybit`.
- Path review found the wasm package build script still pointed at the old
  harness location; corrected it to `../../harness/build.sh`.
- All brief gates passed after migration. Fresh 20-second Bybit recording:
  28,860 websocket messages; committed six-second fixture: 8,742 frames,
  912 markets/tickers and two books. Median of three public replay runs:
  TS/Rust main-thread CPU 463.1/294.3 ms (1.57x), steady CPU 303.0/171.8 ms
  (1.76x), retained memory 3.2/2.1 MB (1.49x).
- Bybit private replay: 18 frames, PASS. Binance public: 871 frames,
  CPU 254.4/108.4 ms (2.35x), memory 2.2/1.8 MB (1.18x), PASS.
  Binance private: 14 frames, PASS.
- Bans, Bybit fmt/clippy/tests, wasm clippy, release build and corrected
  package build passed. Engine unit tests: 10. Final Bybit tests: 3 unit, 27 account/trading,
  13 public. Workers host tests: 2; wasm shell tests: 6.
- Added Bybit public/private CI gates. Updated the former one-venue size
  budget to 1.4 MiB for the measured 1,332,666-byte dual-venue engine.
- Moved an existing wasm test outside the forbidden exchange runtime tree;
  its boundary and integration checks pass.
- PR: https://github.com/Tealstreet/tealstreet-sdk/pull/1298 into
  `feat/rust-rewrite`. Layout review and adversarial findings posted there.
- Root typecheck (44 tasks) and lint (28 tasks) pass. The post-migration
  root suite passed all 27 tasks. Final broad reruns hit unrelated import
  timeouts: the last two-worker run passed 6,488/6,489 safe-cex unit tests,
  with the existing metadata construction test exceeding its fixed 60 seconds.
  Its isolated rerun passed all five tests in 22.46 seconds. An earlier final
  safe-cex test-ci run passed all 451 files / 6,489 tests.
- Adversarial fixes: subscription-ack snapshot refresh with in-flight followup;
  per-symbol position modes and a signed read before writes for unknown flat
  symbols; Partial stop native amendments; no retry after ambiguous batch
  responses; exact split totals; guarded startup pagination/retries; socket
  connection after REST ticker seed; Bybit shell client-ID helpers.
- Targeted tests after fixes: 3 unit + 27 account + 13 public pass; clippy
  passes. Shell wasm regressions: 6 tests pass. All four final replay gates
  pass after the fixes. Both reviewers pass the final implementation; the private
  reviewer independently reran the mode-switch race regression.
- GitHub checks on runtime commit `f04b620712`: both TS CI runs, both secret
  scans, Rust native fmt/clippy/tests, wasm32 and replay gate pass. The banned
  patterns job remained queued when checked; that gate passes locally.
  Final documentation checkpoint CI may still be pending.

## Intentional TS fixes

The `gate.mjs` `KNOWN_TS_FIXES` list records the pre-existing Binance fixes
(position margin size from a zero-size placeholder; execution price rather than
order average price), plus these Bybit fixes: preserve `PostOnly` rejection for a
cancelled `EC_PostOnlyWillTakeLiquidity` order; preserve exact split quantity;
never resend an ambiguously failed batch as single creates; install the ticker
seed before its socket snapshot; amend Partial stops by ID even without a mark
price (the TS fallback could create a duplicate).

## Scope

This document began with linear USDT/USDC contracts. The native registry now
also selects inverse, spot, demo and Hyro variants, with optional inverse/option
categories on linear accounts. Explicit refresh follows each instance's enabled
categories. TS remains the replay oracle and must follow the shared deletion
blockers before removal; this refresh does not authorize deleting variants.

The shared harness now compares normalized outbound reads and subscriptions.
It requires the native execution/wallet feeds in addition to TS's order/position
feeds. One optional TS-only read, `GET /v5/user/query-api`, is explicitly excluded
from request-set parity: it caches API-key/VIP UI metadata, which has no engine
frame representation. Native keeps its conservative non-VIP ten-order batch
limit; the private fixture's `No VIP` setting uses ten in both implementations.
This metadata omission is intentional and is not classified as a TS bug. Every
trading request, other read, authentication identity and subscription remains
checked; unmatched tape requests still fail.
The shared outgoing-subscription gate also records the existing private
protocol improvement: Rust requires `execution` and `wallet` on every connection.
TS infers fills from cumulative order updates and waits for REST wallet reads.
The declared exception adds those two required topics to the TS oracle; it does
not copy Rust subscriptions or hide missing native topics.

## Demo-key diagnostic (2026-10-21)

On a startup fee-read 10003 for built-in live linear/demo, diagnose the opposite
environment with one signed `/v5/user/query-api` GET using the same rejected
credential, including a secondary read key. Only HTTP success and a native integer
code zero add the TS environment-mismatch hint. Fractional, underflowed, floating
zero and quoted codes retain the original rejection. Preserve the native failure,
credential role and deactivation policy; bound the probe to two seconds and
retire timeout/late-response races before the terminal notice.

Exclude regional/site, custom endpoints, testnet, Hyro, inverse, spot, public-only
and remote signing. Forward eligibility markers through the SDK and registry.
Test both directions, signatures/credential roles, negative probes, coalescing,
timeouts and exclusions. Run focused Bybit/Binance/registry/TS gates, independent
review and current-head CI, then land with the lane's separate merge guard.

Implemented the bounded diagnostic with explicit role/signature, timeout/late
response, coalescing and exclusion regressions. The baseline smoke test failed
because no opposite-host request was dispatched. Validation passes: 528
Bybit/Binance native tests, the focused registry construction regression, two
TS site-forwarding cases, package typecheck, touched ESLint, Rust fmt/strict
Clippy and bans. Registry checks use the Bybit-only feature build and select
the relevant construction test; unselected venue construction cases require
the full feature matrix in CI.

## Explicit market refresh and Hyro supervision (2026-10-21)

The native `refreshMarkets` command stages every enabled category before one
catalogue commit. Public Trading/PreLaunch pages are bounded by cursor progress,
a page budget and a 30 s deadline. Concurrent callers share the walk. Required
Trading-option terms are strict; a failed category or malformed row retains the
whole prior catalogue. Spot has no cursor/limit, options use documented `All`,
and Hyro retains its own USDT perpetual filter.

Account state, orders, settings, books, candles, tickers and known symbol fees
survive discovery. Metadata/classification/product indices are rebuilt together;
new symbols subscribe on their existing category socket. New symbol fees remain
unknown until startup fee discovery after a restart. Refresh does no signing,
account reconciliation or socket restart. A degraded optional startup category
must recover through restart before refresh can proceed.

SDK refresh coalesces callers, flushes native state before comparing one
added/removed delta, and emits `markets:changed` when membership changes. Worker
requests execute on the authoritative engine with read timeout semantics.
`fetchMarkets`/`fetchTickers` remain cached reads. Unsupported engines keep their
cached refresh behavior. Shared consumers must refresh the direct source and
restart provider/accounts under the existing catalogue-identity contract.

Hyro's early-ping finding was stale: 5 s watch wakes retain the absolute 10 s
ping deadline. An existing-code regression verifies early, consumed and active
cleared timer callbacks cannot ping early or rearm after close. Demo-key
mismatch diagnostics remain separate in #1590.

Primary instrument contract: https://bybit-exchange.github.io/docs/v5/market/instrument
Independent adversarial review is clear after strict option-term validation and
corrected inverse-category fixture/preconditions.

Startup and explicit refresh share one Instrument decoder and one metadata
publication traversal to avoid retaining duplicate generated code. Their row
admission and error policies remain separate; refresh keeps its strict envelope
and atomic catalogue validation. CI measures the optimized WASM and replay costs.

Refresh shares startup's discovery cooldown helper: HTTP 200 `retCode:10006`
fails the coalesced callers once and retains the catalogue. A later explicit
refresh is refused until the later valid reset timestamp or `Retry-After` hint; success
headers alone do not pause discovery. Refresh does not retry the failed walk.

A completed refresh moves its staged rows and product metadata into the commit
assembly. Each category/status list is consumed once, preserving duplicate
precedence and atomic publication while avoiding redundant deep clones. The
catalogue copy retained alongside the published store still owns its rows.
