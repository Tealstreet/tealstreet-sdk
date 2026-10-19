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
- Required gates are being rerun; results and PR review evidence follow.

## Intentional TS fixes

The `gate.mjs` `KNOWN_TS_FIXES` list records the pre-existing Binance fixes
(position margin size from a zero-size placeholder; execution price rather than
order average price), plus this Bybit fix: preserve `PostOnly` rejection for a
cancelled `EC_PostOnlyWillTakeLiquidity` order.

## Scope

This lane ports linear USDT/USDC contracts. Inverse, spot and demo remain separate
lanes. TS stays available as the replay oracle and as the base class of those
unported variants; it cannot yet be deleted without breaking them.
