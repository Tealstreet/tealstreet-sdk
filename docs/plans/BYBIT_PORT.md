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
3. Verify CI, merge into `feat/rust-rewrite`, then cut a fresh follow-up branch.

## Progress

- Original public/private/trading/query implementation is complete.
- Merged layout locally; resolved the workspace conflict using `venues/*`
  and the Bybit dependency at `venues/bybit`.
- Path review found the wasm package build script still pointed at the old
  harness location; corrected it to `../../harness/build.sh`.
- All brief gates passed after migration. Fresh 20-second Bybit recording:
  28,860 websocket messages; committed six-second fixture: 8,742 frames,
  912 markets/tickers and two books. Median of three public replay runs:
  TS/Rust main-thread CPU 540.0/385.8 ms (1.40x), steady CPU 347.5/226.6 ms
  (1.53x), retained memory 3.2/2.1 MB (1.50x).
- Bybit private replay: 18 frames, PASS. Binance public: 871 frames,
  CPU 151.6/59.2 ms (2.56x), memory 2.2/1.8 MB (1.19x), PASS.
  Binance private: 14 frames, PASS.
- Bans, Bybit fmt/clippy/tests, wasm clippy, release build and corrected
  package build passed. Engine unit tests: 10; Bybit integration tests: 27
  plus signing/body unit tests. Workers host tests: 2; wasm shell tests: 4.
- Added Bybit public/private CI gates. Updated the former one-venue size
  budget to 1.4 MiB for the measured 1,318,547-byte dual-venue engine.
- Root typecheck/lint passed. Root tests found an existing wasm test in
  the forbidden exchange runtime tree; moved it into `src/__tests__/`
  and reran its boundary and integration checks. Full root gates rerunning.
- PR: https://github.com/Tealstreet/tealstreet-sdk/pull/1298 into
  `feat/rust-rewrite`. Layout review and adversarial findings posted there.
- Root gates now pass: typecheck 44 tasks, lint 28 tasks, tests 27 tasks
  (safe-cex 451 test files / 6,489 tests).
- Adversarial fixes: subscription-ack snapshot refresh with in-flight followup;
  per-symbol position modes and a signed read before writes for unknown flat
  symbols; Partial stop native amendments; no retry after ambiguous batch
  responses; exact split totals; guarded startup pagination/retries; socket
  connection after REST ticker seed; Bybit shell client-ID helpers.
- Targeted tests after fixes: 3 unit + 26 account + 13 public pass; clippy
  passes. Shell wasm regressions: 6 tests pass. Final gates and re-review follow.

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
