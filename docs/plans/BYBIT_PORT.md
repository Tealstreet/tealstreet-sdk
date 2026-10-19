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

This lane ports linear USDT/USDC contracts. Inverse, spot and demo remain separate
lanes. TS stays available as the replay oracle and as the base class of those
unported variants; it cannot yet be deleted without breaking them.

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
