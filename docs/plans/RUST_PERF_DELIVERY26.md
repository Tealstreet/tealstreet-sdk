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

## Release checkpoint and fixed measurement method

All 26 isolated release builds and isolated WASM Clippy pass. There are 234
matching browser/Node bindings; prior 198 family/aggregate artifacts remain
archived. All 52 size checks pass. Five new caps use actual release bytes plus
3%, rounded up KiB. One reviewed exception updates Binance from 846,848 to
960,512 bytes: its old measured 799,014-byte engine supported USD-M/PM, while
the current 932,417-byte family also includes Spot/COIN-M and subsequent
lifecycle fixes (2,216 net runtime lines added). Shared boundary changes also
contribute; no exact byte attribution is claimed. The other 20 existing caps
remain fixed. The old cap breach and measurements are retained in the proof.

Actual WASM checks pass: SDK 411 tests/13 files, host 154 tests, supplementary
parity and scripted lifecycle 20 tests. Rust-to-JS BOOK_DELTA and real native
WebSocket session adapter integrations each pass. Earlier four loader transform
failures from missing family artifacts are retained; the complete built test
run passes without changing assertions/timeouts. Source, tool, artifact and
log hashes are in `RUST_PERF_DELIVERY26_RELEASE.json.gz`.

Before any new performance child, freeze this fixed collection: 61 candidate
private gates once each (`--runs 1 --books ''`), then exactly one official
three-run public gate per arm for each of 34 profiles, in CI roster order.
Alternate control/candidate order by profile (17 AB, 17 BA); each gate uses
recorded cadence and the identical final helpers/TS/dependencies/fixtures.
This provides global order balance, with three samples per arm/profile; ranges
and TS drift will be reported, with no statistical significance claim.

The original control is physical `86ed5e3d2e`, whose 264 production
Rust/config/host/build entries match `02e9db0e8b` exactly. Its original immediate
host, aggregate loader and 11,179,592-byte aggregate browser/Node binary remain
unchanged. Both arms share current helpers and fixtures, including the latest
GRVT futures fixture and snapshot construction inside the measurement window.
Normal gates have profiling disabled.

Retain every numerical threshold or narrower-margin failure and continue the
fixed roster. Stop on operational/parity/invalid-metric failures; retain partial
raw outputs and logs. No numerical retry, extra warmup, profile or tuning run is
allowed within this collection. Compare all same-implementation public outputs
across runs/arms exactly after removing only top-level metrics. Freeze full
runtime/dependency inventories before/after the whole collection, with smaller
helper/host/selected-binding/fixture guards per gate. All owned compiler/test
jobs must be finished before measurements. This checkpoint passes prerequisites;
the requested final public/private gate and margin results remain pending.

## Replay reconciliation before public measurement

The first fixed collection stopped on its first private gate (Breakout): four
zero-leverage placeholders appeared only in the Rust comparison view. Two
private replay children ran; no public gate or comparison ran. The failure,
raw-output hashes and passing endpoint audits remain in
`RUST_PERF_DELIVERY26_RECONCILIATION.json.gz`; original config/proofs are unchanged.

The merge had retained an old BloFin-only comparison filter. Restore the exact
landing generic scalar leverage >0 filter, preserving independent buy/sell rows
and native consumer placeholders. A fresh remaining-diff review also found two
landing rules to restore: unconditional Tape interception of SDK fetch calls,
and the explicit 15-second startup/subscription timeout instead of the helper's
30-second default. Response headers remain preserved.

All 632 serialized harness tests pass. Two actual-function VM regressions cover
scalar/independent leverage and ordinary-venue fetch interception with Request
cloning, overrides, response metadata and bodyless statuses. Independent reviews
pass. Engine/Rust source and all 234 WASM artifacts are unchanged, retaining
prior Rust/SDK/host/parity/size validation. Refresh this one shared helper in the
control, preserve the original failed collection, then freeze a fresh config and
run the same declared 61-private/34-public protocol. This is a correctness repair
before public sampling; no numerical retries or performance tuning occurred.
