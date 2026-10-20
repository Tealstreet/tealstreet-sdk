# Delivery and per-family builds after the replay-audit split

PR [#1335](https://github.com/Tealstreet/tealstreet-sdk/pull/1335) merged into
`feat/rust-rewrite` as `0a0b3453e9` with all 36 CI checks green and three fresh
adversarial reviews posted. Its fixed-binary GRVT audit comparison reduced Rust
replay main/steady CPU by 28.1%/37.5%, with identical output. TS improved too;
the main ratio narrowed and Rust retention increased 4,552 bytes. That result
is a harness improvement, not completion of this engine task.

Fresh branch `feat/rust-perf-delivery` starts at that landing commit and merges
the complete saved `feat/rust-perf` WIP (`8e7e8ea805`). Landing now includes
TxFlow and the native frame-session adapter. Preserve all historical failures,
controls and exact source labels; the first PR's nine original 25-family
aggregate bindings are archived at `/tmp/perf-replay-pr-built-artifacts`.

## Integration phase

Resolve and review engine/wire, native host/server, replay harness and WASM/SDK
scopes. Retain incoming native book deltas, command tracking, adapter snapshots,
backpressure/cancellation and no-retry behavior. Retain the bounded ordered
text/binary queue, per-message timestamps, finite-prefix control/flush barriers,
allocation optimizations and per-family loaders. The landed GRVT audit stays
byte-identical. TxFlow preserves testnet validation, account/vault identity and
signing. Add its 26th canonical feature, package exports and literal loaders;
keep earlier Cargo feature names and supported build CLI aliases working.

Landing's clock/readiness policy takes precedence: all public fixtures and
explicit virtual-private/polling profiles use its deterministic clock; detached
snapshot reconstruction remains inside both runners' CPU/memory window. Keep
latest last-active socket selection with the allocation-free scan. A final
before/after comparison must share these exact helpers on both sides. Old
25-profile numbers do not measure this method. The historical19 driver has only
a trailing-space cleanup; its measured source and hashes remain in Git history.

Run formatting/bans, strict native all-target and WASM workspace Clippy, all Rust
tests, serialized harness and focused host/wire tests, repository typecheck/lint/
tests. Review each scope independently, fix failures and commit the merge.
Native's queued-timer test assertion was adapted to its event wrapper; no runtime
contract was weakened. No performance replay runs alongside these checks.

## Release and comparison phase

Derive the current complete roster from CI: 26 family builds, 33 public profiles
and 58 private variants. Build each family once, retain all 234 bindings and
verify matching web/Node WASM. Keep existing 21 size caps. Derive the five missing
Coinbase, Polymarket, GRVT, BreakoutProp and TxFlow caps from actual bytes plus
3%, rounded up to KiB; record any unexpected old-cap breach before changing it.
Run isolated feature Clippy, real WASM SDK/host/consumer and server-adapter tests,
all supplementary parity suites and all private gates before public timings.

Freeze the selected source and build a matching original landing control. Share
final helpers, fixtures, TS references, Node/dependencies and clock/cadence on both
sides; preserve original unbatched host and aggregate binary in the control.
Use a separately declared balanced comparison with no direction-based retries,
retaining every unfavorable sample and failure. Check CPU and retained memory
for every public profile and report exact before/after numbers. Source/dependency
endpoint checks and small per-run artifact/fixture guards should be explicit;
avoid rebuilding a large generic measurement framework.

If Bitrue or another gate remains open, declare a bounded experiment supported
by profiling before editing/measuring it. The prior opt3 experiment failed and
restored Bitrue z exactly; do not silently retry it or relabel its data. Existing
scheduled singleton encoding remains unchanged until a separate declared trial.

Split another independently useful PR if it can land sooner. Every PR targets
`feat/rust-rewrite`, includes actual adversarial review comments and merges only
with green CI, then cuts a fresh follow-up branch. The full task remains open
until batching, per-family builds and all requested gate margins are verified.

## First integration checkpoint

The saved WIP integrates into `0a0b3453e9`: all 2,480 Rust tests in 197 groups
pass, with zero failures or ignored tests. Formatting, bans and strict native
all-target/WASM workspace Clippy pass. Repository checks pass 44 typecheck,
28 lint and 27 test tasks; serialized harness 624, focused host 118, consumer
mock 220 and smoke unit 10 tests pass. Independent engine/wire, harness, native
server and WASM/JS batch reviews pass. The loader review found two smoke
callers missing the newly required venue; both now pass it. Their actual-WASM
integration remains part of the release checks. Failed native API, mock
typecheck and synthetic signed-request fixture attempts remain in
`RUST_PERF_DELIVERY26_INTEGRATION.json.gz`, alongside passing evidence.

Origin advanced to `86ed5e3d2e` (Lighter variants) during these checks. Commit
this checked merge, then integrate that landing before release: still 26
families, now 34 public profiles and 61 private variants. No final release or
performance numbers are claimed for this checkpoint.

## Latest landing integration

Merge `02e9db0e8b`: Lighter Robinhood shares the Lighter binary, including its
remote signer/provider options; CI normalizes its family selector while keeping
separate public, private and testnet gates. Incoming GRVT futures fixture and
product assertions remain intact. Build CLI accepts the Robinhood alias.
Independent alias, conflict and constructor reviews pass.

All 2,502 Rust tests in 200 groups pass (zero failures/ignored), as do fmt/bans
and strict native all-target/WASM workspace Clippy. Latest repository checks
pass 44 typecheck, 28 lint and 27 test tasks; serialized harness 630 and focused
host 118 pass. Mocked loader tests cannot transform missing newly registered
family export files yet (four failures retained); rerun after release creates
all 26 packages. No assertions or thresholds were relaxed. The original
`86ed5e3d2e` control is building separately; all 264 production Rust/config/host/
build entries are byte-identical to `02e9db0e8b`, whose changes affect GRVT
fixtures/tests/docs. Final shared fixtures/helpers will match in both arms.
