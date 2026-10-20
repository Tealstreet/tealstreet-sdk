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

## TxFlow nonce matching reconciliation

The fresh collection passed 23 private profiles, then stopped at TxFlow TS
replay with eight unmatched writes. Public collection still had zero children.
Its signed request had a fresh monotonically increasing nonce; stricter shared
request matching retained it while the tape used a fixed signing nonce.

Add a TxFlow-only descriptor projection for the top-level nonce on POST
`https://rpc.txflow-testnet.xyz/exchange` action envelopes. Signature, freshness,
monotonicity and replay checks still examine the untouched original body before
matching. Caller IDs, economic fields, vault/confirmation, nested nonce values
and other routes/venues remain exact. Two actual counterexamples proved
RED→GREEN, seven targeted protocol tests and all 634 harness tests pass, and
independent source review passes. The repaired TxFlow private gate passes
(35 frames, two markets/tickers); its CPU/memory remain informational.

`RUST_PERF_DELIVERY26_TXFLOW.json.gz` preserves the failed collection and fix
proofs. Check each remaining 37 private profiles once diagnostically to identify
any additional correctness integration failures before the final clean fixed
61-private/34-public run. Keep every diagnostic result. No public sampling,
Rust changes, rebuild, numerical retry or tuning occurred in this repair phase.


## Spot-product integration before the final collection

The bounded remaining-private diagnostic passed all 37 profiles without retries.
Together with the preceding 23 passes and repaired TxFlow gate, all existing
61 profiles have correctness evidence. No public performance sample was taken.
The diagnostic report is `/tmp/perf-delivery26-private-remaining-diagnostic/report.json`
(SHA-256 `4e188a5a6a0911e0bf09a2eaf1baeca0820675024be9efb31efbb2b822f824e1`).

Merge landing `f4ea9e06ab11321c0509bd75d05c0486c7820e0f` (Spot products)
before freezing measurements. Preserve both Spot constructors and normalize
`krakenspot`/`hyperliquidspot` to their existing Kraken/Hyperliquid packages in
build CLI, host/SDK loaders and CI. The roster expands to 36 public profiles
and 63 private variants, still 26 families. One newly added SDK mock needed the
required `wsMessages` method; the initial typecheck failure is retained. The two
new alias checks initially fail against the previous release, also retained;
rebuild all families before their actual WASM validation.

Repeat affected workspace/repository checks, archive the current 234 bindings
and original nine aggregate bindings, then build and validate all 26 families.
Record any existing size-budget breach before considering a scope-based change.
Run actual SDK/host WASM checks, existing parity/lifecycle integrations, both new
Spot parity suites and native wire/session checks. Independent merge and release
reviews remain required. These checks do not establish performance margins.

Use the independently audited physical original `f4ea9e06ab` control at
`/tmp/rust-perf-baseline26spot`: 409 Rust/config and 33 protected files match Git,
29 archive compile paths are recorded, and its original browser/Node aggregate
WASM is 11,398,568 bytes (SHA-256
`fc1cbdbe820146671f17dcdce066ad79f011614d922ef6a5b30d551b37fa6163`).
Its unbatched host, original loader/build and nine physical artifacts stay fixed.
Share final helper/fixture/TS/dependency bytes only after the candidate is checked.
Retain all earlier controls and failures under their original source labels.

The final protocol supersedes the earlier 34/61 roster: run all 63 candidate
private gates once, then one official three-run gate per arm for each of 36
public profiles, alternating AB/BA by CI order (18 each). Reuse the immutable
61 private tapes and generate only the two new Spot tapes. Retain every numerical
failure and narrower margin; stop on operational/parity/invalid-metric failures.
No directional retries, extra warmups or profiling occur during the collection.
Freeze source, dependencies, tools, helpers and artifacts before/after the cohort,
with smaller per-gate guards. Report exact before/after CPU and memory numbers;
all requested final performance gates remain pending.

The integration checkpoint passes 2,569 Rust tests/204 groups (zero failures or
ignored), fmt/bans and strict native/WASM workspace Clippy. Repository checks
pass 44 typecheck, 28 lint and 27 test tasks; serialized harness passes 644.
Independent merge review passes against both parents. The retained check and
control/diagnostic evidence is `RUST_PERF_DELIVERY26_SPOT_INTEGRATION.json.gz`.
The main Cargo target is free for the declared release pass.


## Spot release checkpoint

All 26 fresh family releases and isolated strict WASM Clippy pass; 234 exact
bindings have matching browser/Node WASM hashes. The prior 234 bindings and nine
aggregate bindings are archived physically (243 files); 415 Rust/config/build
input hashes stayed fixed. Native BOOK_DELTA wire and session adapter checks pass.
Actual WASM SDK checks pass 423 tests/13 files and host checks pass 154 tests.
The existing supplementary/lifecycle suites pass 20 tests.

Eight new Spot parity tests initially imported the historical aggregate package
through `spot-host-rig.mjs`, so they rejected the new venue names. Select the
family with the shared registry inside the synchronous rig constructor. All eight
actual Spot tests then pass; no assertion, timeout or runtime engine behavior was
changed. Preserve the failed run and refresh only this shared helper in the
control under a new proof before freezing comparisons.

Kraken exceeds its old 949,248-byte cap: the previous 905,892-byte futures engine
is now 1,084,647 bytes with a separate Spot state machine and 2,273 added runtime
lines. Two independent reviews support an explicit functional-scope cap of
1,118,208 bytes (actual plus 3%, rounded up KiB). Keep the breach proof and all
other 25 caps fixed; no exact attribution of binary growth is claimed.
Hyperliquid is 1,080,671 bytes, within its existing 1,090,560-byte cap. All 52 size
checks now pass. TxFlow's WASM hash changed at the same size; current hashes stay
in the new release proof, without claiming byte identity to the prior release.

`RUST_PERF_DELIVERY26_SPOT_RELEASE.json.gz` records release, original-control,
roster, size and real-WASM check evidence, including failures. The final 63-private
and 36-profile before/after performance collection is still unexecuted.


## Host ordering review before public sampling

Fresh adversarial source review at `46624f485a` found two P2 issues: a later
invalid timestamp can reject a partition after earlier partitions drained
unexecuted actions; and an asynchronously decoded raw frame can enter at time
17 after a timer/control at 18. Decode overtaking existed before this branch,
but preserving the older raw timestamp introduces backward engine event time.

Intentionally stop the collection for correctness repair: 19 private gates
passed, the twentieth (Kraken) replay received SIGINT, and zero public samples
ran. Full endpoint/per-gate guards pass (30,769 unique files/303 rows). This is
an explicit review stop, not a venue correctness or numerical performance
failure. `RUST_PERF_DELIVERY26_HOST_REVIEW.json.gz` retains configuration, runner,
raw outputs and interruption evidence; all earlier cohorts remain immutable.

Plan: validate/coerce every captured batch timestamp before any partition
crosses WASM. Reserve asynchronous raw arrivals synchronously in a host-wide
FIFO; fill those slots after decode, then dispatch the ready prefix. Queue
commands, timers, HTTP, open/close and flush events behind earlier reservations,
keeping their captured timestamps. Preserve fast ready-text batching, limits,
action/reentrancy order, cancellation and disposal. Synchronous snapshot/account
reads expose committed state; an unresolved asynchronous payload cannot be
included in a synchronous read. Propagate the optional hook through native,
Workers and SDK relay transports. Add RED-to-GREEN counterexamples for delayed
decode, cross-socket/control order, failures/disposal, and partition atomicity.

Audit the analogous native multi-thread producer-side decode ordering before
finalizing scope. If confirmed, enqueue raw frames before decoding and resolve
at their actor inbox position, preserving routing health, failure close codes,
finite-prefix fairness and batch bounds. No clock clamping. Independently
review repairs, run affected source/actual-WASM checks, commit, refresh shared
controls only where needed, and declare a fresh final collection. No public
performance measurements or numerical tuning occurred in this repair phase.


## Ordered ingress repair checkpoint

The JS fix validates every captured timestamp before any partition crossing.
Asynchronous raw frames/close callbacks reserve host-wide FIFO positions;
ready text keeps its direct batching path. Later commands, timers, HTTP, opens,
closes and flushes wait behind unresolved input, retaining captured times.
Synchronous snapshot/account reads expose committed prefix state. Cancellation,
decode failures and disposal settle reservations/waiters. Available whole HTTP
responses enter atomically before later commands; streamed chunks retain their
observed readiness. Consecutive ready decoded messages still share one crossing.
Native/Workers/SDK relay propagation and public declarations/docs are updated.

Native queues owned raw input before expensive decoding, resolves it at the
actor/observer inbox position, and applies routing health there. Decoded limits,
finite-prefix counts and cached over-cap payloads remain correct. Local socket
ownership is rechecked after intervening actions without decoding twice;
remote task exit preserves final queued fills. Malformed data closes once with
1007 and suppresses stale queued input. This fixes expensive decode overtaking;
it does not promise globally ordered wall clocks for arbitrary racing producers.

Four JS regressions and the deterministic two-worker native ordering case
proved RED before the fixes. Native's first full check also caught an existing
`tokio::spawn` compatibility regression: an inline async capture required a
Send-only decoder to be Sync. Capture the pending boolean first instead, keeping
selection behavior and the decoder contract. All failed logs/proofs are retained.

Independent JS and native reviews pass. All 2,575 Rust tests/204 groups pass
(zero failures/ignored), as do fmt/bans and strict native/WASM workspace Clippy.
Repository checks pass 44 typecheck, 28 lint, 27 test tasks; harness 644.
Actual WASM checks pass SDK 423/13 files, host 167 and parity/lifecycle 28 tests.
All eight repaired JS source hashes and 234 binary artifacts stayed unchanged
through these checks. `RUST_PERF_DELIVERY26_INGRESS_REPAIR.json.gz` records proof.

Origin advanced to `5e92f93a8a` (PR #1341 product semantics, Bybit/OKX products)
during verification. Commit this checked repair, then merge that landing and
review integration before final release/measurement. Build a matching physical
original control and preserve every prior control/cohort with its source label.
No public performance samples ran in this repair phase; the full task remains
pending the final expanded gates and before/after margins.


## Product-semantics integration checkpoint

Merge `5e92f93a8a` into checked ingress repair `f2fd8ac2c6`. Preserve incoming
typed classifications, option terms/queries/Greeks, native filled quantities,
combined Bybit accounts and global/EU OKX product discovery/reconciliation.
Retain canonical family features and aliases, activate semantic command/store
features only through Bybit, and retain Breakout's cold subscription outline.
Keep both batch ABIs and profiling boundaries. OKX retains native index fanout,
subscription dedup and empty-quote guards; borrowed ticker-ID mutation and
first-insert-only mark/index cache writes remain. Restore landing's startup
subscription serialization where its dedup/catalog behavior supersedes the old
linear-only batching. New product integration callers select the Bybit package.

Review caught a missing test command counter and new frame sections bypassing
allocation bounds. Repair both. Feature-gated hints/string reserves now include
classifications, derivative products, position Greeks and the second order tail
(including the native placeholder when only filled placement units exist).
Meaningful RED cases exceeded capacity on an empty classification header and
filled-only order; seven targeted tests now pass, covering Unicode, absent/empty
metadata, odd/even/257 rows, independent sections and reserved/unreserved parity.
Independent Store and remaining product integration reviews pass.

Three harness tests needed current contracts: use the real subscription oracle,
include inverse/global/EU products from the recorded catalogs, and sign an actual
native OKX request. Runtime guards remain unchanged; missing/wrong product,
precise unsubscribed book/ticker, typed quantity, write-once and bad-signature
negatives remain or are stronger. Retain the initial 646/649 failed run and the
passing 103 targeted /649 full harness checks.

All 2,625 Rust tests/206 groups pass (zero failures/ignored), as do formatting,
bans and strict native all-target/WASM workspace Clippy. Repository checks pass
44 typecheck, 28 lint and 27 test tasks. Fresh release/actual-WASM checks are next.
`RUST_PERF_DELIVERY26_PRODUCTS_INTEGRATION.json.gz` contains source/check evidence.

The final roster supersedes 36/63: 37 public profiles (new OKXEU), 63 private
variants. Regenerate only the four changed OKX tapes; reuse the other 59 exact
tapes/logs, with current generator/dependency hashes checked. Keep the original
fixed collection method, now 19 AB /18 BA by CI index (one unavoidable profile
imbalance). One official three-run public gate per arm/profile, no numerical
retries/warmups/profiling; retain numerical failures and stop on operational or
parity failures. Original source is physical `5e92f93a8a`, with 412 Rust/config
and 33 protected files, 29 fresh archive compile traces and nine aggregate
bindings. Its browser/Node WASM is 11,609,430 bytes, SHA-256
`8a8f67fb74f5fb7981651a84028985ba2382a6e538151090fa57dbcb0ed94300`.
Independent audit passes; no explicit internal-cache deletion proof is claimed.
Freeze final shared helpers/TS/dependencies only after candidate checks/release.
Prior controls and stopped cohorts remain immutable. Full performance goals and
PR landing remain unfinished; no public samples ran in this integration phase.

## Product release and final collection readiness

All 26 fresh family releases and isolated feature Clippy checks pass. The native
fixture, frame-wire and session-adapter checks pass. Independent physical audit
verifies 418 source/build inputs, 234 bindings and 243 preserved archive files;
original control source/binaries and historical proofs remain unchanged.
Actual release tests pass SDK 423 (13 files), host 167, and 36 combined parity,
lifecycle, spot and derivative-product checks. All 52 family size checks pass.

Preserve the failed original-cap check. Two independently reviewed exceptions
use actual bytes plus 3%, rounded up to KiB: Bybit 1,140,161 bytes gets a
1,174,528 cap for incoming options/combined accounts and product features;
Breakout 703,407 bytes gets a 724,992 cap. Breakout adds no product: landing
explicitly trades subscription parser bytes for startup CPU, alongside shared
command/order schema changes. This explains the accepted scope, without exact
byte attribution or claiming a measured CPU win. Other 24 caps remain fixed.
`RUST_PERF_DELIVERY26_PRODUCTS_RELEASE.json.gz` preserves proofs, passing logs
and the failed size log. Runtime/source files are unchanged since `86d816d98c`.

Freeze the reviewed 37-public/63-private method and dependencies now. Review the
actual frozen configuration, then run private checks followed by the fixed
three-run public comparisons with all compilers and other tests quiet. Keep
all numerical failures and report narrower margins; no directional retries.
CPU/memory goals and PR landing remain pending.

## Product collection negative checkpoint

Frozen configuration `3578989c2b0f01cd92deee894c28662acbd25f366de834b10dae0e26408a6e22`
passed independent review. All 63 private gates pass. Twelve public pairs
completed; the 13th pair stopped at KuCoin candidate because all three retained
memory totals were invalid negatives (-493,896/-493,976/-518,000 bytes).
All final endpoint/per-gate hash checks pass. Preserve the entire configuration,
runner, report, logs and 282 replay outputs in
`RUST_PERF_DELIVERY26_PRODUCTS_NEGATIVE.json.gz`. No numerical retries occurred.

Memory margins improve in every completed pair, but CPU margins narrow in ten
of twelve; Bitget and Backpack widen both. Breakout fails the CPU gate (ratio
2.98 to 0.83), mostly because TS drops 44.376 to 12.745 ms between arms while
Rust changes 14.890 to 15.374 ms. This is not evidence of a 3.6x Rust slowdown.
Polymarket is also too close to claim improvement. Do not call these goals met.
Release optimization profiles are identical between candidate and control.

Next bounded phase: diagnose harness baseline lifetime with a strict invariant
(no negative allowance/clamping), and audit singleton host delivery against the
original reusable byte path. No new scored children until a source-backed repair
plan and independent audit. Preserve all old inputs/results; declare any new
collection after source changes and checks. Full task and PR remain unfinished.

## Parity landing and bounded delivery/method repair

Merge landing `df2d6a4a03` (PR #1340). Preserve secondary-read credentials and
primary-only trading/history routes, atomic financial reconciliation, lookup
contracts, and Phemex balance totals. Canonical family features and batching APIs
remain. The incoming OKX cached ticker row/readiness lookup supersedes the older
index-target cache and borrowed lookup optimization; use its complete library.
Transport retains raw arrival reservations/timestamps and binary forwarding;
requests tests keep both parent additions. Independent integration review passes.
A new wrapper fixture needed the required batch method; retain the initial
failed typecheck and its corrected rerun. Checks pass 2,731 Rust tests/209 groups,
formatting/bans and strict native/WASM workspace Clippy; repository tasks pass
44 typecheck, 28 lint and 27 tests. Full harness/host checks are running.

Restore the original reusable UTF-8 byte path for scheduled text singletons;
retain deferred delivery, raw clocks, FIFO barriers, optional legacy fallback
and real multi-message batches. Four focused regressions fail before this
one-line repair. Host tests pass 71, covering Unicode/lone surrogates, negative
zero, scratch bounds/reuse, reentrancy, disposal and exact error identity.
Independent source/evidence review passes. No CPU benefit is claimed yet.

A single no-engine KuCoin fixture probe did not reproduce the heap release;
the precise old owner remains unproven. Introduce explicit strong fixture
ownership as a shared methodology correction: actual input buffer, decoded
source, split lines and parsed records live through both GC samples and final
serialization, with idempotent success/error cleanup. Parsing, initialization
after baseline, CPU cutoffs, exact memory arithmetic and negative rejection
remain unchanged. This retains more common input storage and may alter GC
pressure; apply identically to both runtimes and both arms. All 32 focused
memory/measurement/fixture tests pass. A deliberately weak-owner mutant fails
its real-GC lifetime oracle; this proves the oracle, not the old heap diagnosis.
Independent review passes. Declare one fixed KuCoin validity check after fresh
release/checks with owned compilers quiet; retain its outcome without retry.

The new CI roster is 39 public profiles/63 private variants, including distinct
OKX inverse and EEA X-Perp fixtures. Identify public cases by profile ID rather
than venue, preserving exact engine selector/books and fixed 20 AB/19 BA order.
Regenerate only five changed private generators (GRVT and four OKX variants),
reusing 58 tapes after content/dependency checks. Source/profile changes require
a fresh collection; all prior negative inputs/results remain immutable.
Original physical control is `df2d6a4a03`, with 429 Rust/config inputs and 33
protected paths verified against Git. Its 29 internal crates compile freshly
from the archive after explicit isolated workspace-artifact invalidation;
nine aggregate bindings have equal browser/Node WASM, 11,732,506 bytes, SHA-256
`e09563111217316847f13819b019b5a12089ce110530d1e020a64f07c00adc3d`.
Independent original-control audit passes. New release, actual-WASM validation,
KuCoin validity check, all final performance gates and PR landing remain pending.

## Parity release checkpoint and failed validity check

Fresh release and all 26 isolated feature Clippy checks pass; all 435 source
inputs, 234 bindings, 243 archived files, tools and command logs pass independent
physical audit. SDK actual WASM passes 426/13 files, host 172, parity/lifecycle/
product/reference 39, and full harness 656. An SDK observer initially missed the
new byte entry (425/426 pass); instrument that actual entry while keeping every
output, crossing and socket-error assertion. Independent review passes. Final
repository typecheck/lint pass 44/28 tasks; 27 repository test tasks passed.

Retain the initial cap failure. Three independently reviewed functional growth
exceptions use actual plus 3%, rounded up to KiB: OKX 854,460 bytes/cap 880,640;
Phemex 804,542/cap 829,440; GRVT 1,029,587/cap 1,060,864. Incoming changes expand
primary/read identity and atomic financial/lookup handling, Phemex income ledger
and history, and GRVT authoritative cancellation/session/heartbeat validation.
No precise byte attribution or performance win is inferred. Other 23 caps stay
fixed; all 52 final size checks pass.

The single declared KuCoin validity trial failed: control gate passes, candidate
retained total is -490,352 bytes (heap 19,674,888 to 17,925,040, buffers +276,456,
WASM 983,040). Explicit fixture ownership raises both heaps while leaving the
roughly 1.75 MB release intact; it did NOT resolve the actual problem. Preserve
all four raw replay outputs, scripts/logs/counters, failed SDK/typecheck/caps and
passing source/release evidence in `RUST_PERF_DELIVERY26_PARITY_CHECKPOINT.json.gz`.
All guarded runtime inputs and artifacts remain unchanged. No numerical retry
or final 39/63 collection occurred. Diagnose the actual retained heap owner next
with one declared unscored diagnostic; raw heap dumps remain local and use a
sanitized environment. Never clamp, permit invalid public negatives, or move
implementation imports/initialization ahead of the memory baseline to hide it.
Full performance goals and PR landing remain pending.

## Exact heap owner and fixture representation correction

One unscored, sanitized actual KuCoin Rust diagnostic preserves logical output
and all 868 guarded inputs. Two local-only heap snapshots identify the precise
owner: 16,154 still-reachable root `record.url` strings shrink in place from
320 to 24 bytes, forwarding to three equal canonical strings. Their storage
falls 4,781,584 bytes. Stable record/root edges remain; the snapshot/native-node
accounting exactly matches the post-snapshot heap change. Independent offline
review confirms every URL edge, count and accounting identity. Neither object
loss, code flushing nor external migration explains that change. Snapshot GC
also affects counters, so this diagnostic remains unscored and invalid memory
is still rejected. Raw heap dumps never enter Git; redacted evidence is in
`RUST_PERF_DELIVERY26_URL_OWNER.json.gz`.

Share identical primitive root URL values while parsing the common fixture,
before baseline sampling. Use a temporary per-acquisition Map, explicitly drop
its binding before publishing the lease, and preserve exact strings including
empty/invalid/signed/query/case/encoding distinctions. Do not parse/canonicalize
URLs or touch body/text/nonstring fields. Both runtimes and both arms receive
this helper. URL-sharing CPU/GC preparation moves outside the replay window;
this is a disclosed shared-method correction, not an engine optimization.
The earlier fixture lease remains an ownership contract and did not fix the
actual representation change. Tests prove values, while heap graphs prove
physical storage. Final source review passes, focused tests pass 33, and all
657 source-frozen harness tests pass.

Next: refresh only the two changed shared helper/test files while preserving
all original source/binaries/proofs. Declare one fixed corrected-method KuCoin
validity pair, retaining its result without retry; then freeze the 39/63 full
collection if valid. No Rust/binary or host production code changed, so reuse
verified releases and source checks with hashes rechecked. Full CPU/memory
requirements, final gates and PR landing remain unfinished.

## Corrected-method validity and final collection declaration

The single corrected-method KuCoin pair passes both official gates with exact
logical output. Retained memory is valid: control 5,666,996 bytes, candidate
4,345,912 bytes; TS is 21,896,706/21,763,418 bytes. Single-run CPU values are
unscored and do not establish an engine improvement. The negative original and
failed lease-only trials stay preserved. `RUST_PERF_DELIVERY26_URL_VALIDITY.json.gz`
contains the four outputs, logs, checks and readiness evidence.

Original control remains `df2d6a4a03`; candidate Rust/bindings and host production
source remain `098c150ebf`. Common helpers now match `5de776b440`, including the
exact-URL representation correction. Only two shared files changed in the
control; old bytes are backed up. Independent review verifies all 175 helpers
against Git and all original 429/33/9 protected inputs/proofs unchanged.
No Rust rebuild or repeated SDK/source checks are necessary: their production
inputs and all 234 release artifacts are rehashed unchanged. Final harness657,
SDK426, host172, parity39, Rust2731/209, root44/28/27, familyClippy26 and size52
checks pass, with all earlier failures retained.

Freeze 39 public profiles/63 private variants. Public profile IDs distinguish
OKX inverse and EEA X-Perp cases from their default venue profiles. Use fixed
20 AB/19 BA order, one official three-run public gate per arm/profile, after
all 63 private gates once. That is 594 replay children (126 private +468 public).
Keep full input/dependency guards, per-message recorded cadence and delivery;
no profiling, numerical retries or additional warmups inside this collection.
Retain numerical failures and narrower margins; stop on operational/parity/
invalid-metric failures. Report all three-run ranges and TS drift alongside
medians without statistical-significance claims. Full goals and PR are pending.

## Collection stopped before public samples; landing refresh

The frozen corrected-method collection stopped on its first private gate,
Breakout TS final memory collection: 92 samples, 84 backing-buffer transitions,
10,058 ms elapsed against the unchanged 10-second deadline. No replay outputs,
private passes, or public samples exist. Endpoint and per-gate guards pass.
Preserve report, log, frozen configuration and runner in
`RUST_PERF_DELIVERY26_URL_COLLECTION_STOP.json.gz`; do not retry or call this a
venue performance failure. Independent source audits found no proven ongoing
polling bug: polling/rate-limit timers remain virtual during final collection.
No diagnostic child, timeout adjustment or additional heap snapshot ran.

Landing has advanced with production changes including KuCoin spot, Paradex
signing and host APIs. Merge it before collecting final PR evidence. All older
source-bound results remain historical. Reconcile host batching and family
builds, review the merge, verify affected checks and fresh artifacts, then use
the current CI venue roster. Keep an explicit unresolved-gate record rather
than hiding failures behind successful-only summaries. Open a reviewable PR as
soon as the implementation and required checks support it; final landing still
requires green CI and the requested correctness reviews. Full performance
requirements remain unfulfilled.

## Current landing integration plan (27 packages)

Resolve the latest landing merge while preserving engine reservation hints,
raw-arrival FIFO, native cancellation, and incoming pending signing, symbol-aware
commands and prediction feature gates. Keep KuCoin futures separate from the
new optional Spot module: register `venue-kucoinspot`, browser/Node exports,
literal browser imports and family registry. The provisional Spot budget is
708,608 bytes (landed 687,701 plus 3%, rounded to KiB); measure the new build
before claiming this cap passes. Other caps remain unchanged.

Incoming build profiles retain Paradex host/engine O2 and Bitrue venue O3;
default and selected builds must use identical per-family commands. These are
new source-bound comparisons, not evidence for older compiler experiments.
Signing completions, rejection and timeout must join the same raw-input FIFO
with their captured completion time. Three regressions reproduce overtaking
before that integration repair; all 80 host tests pass afterward.

Finish in committable phases: (1) independent merge reviews plus source,
typecheck, lint and repository tests; (2) fresh 27 release packages, actual-WASM
SDK/host/parity checks and measured caps; (3) current-roster gates and honest
before/after evidence, followed by PR correctness reviews and green CI. Preserve
the completed 26-package artifacts before rebuilding. Keep operational failures
visible and do not adjust collector limits or retry numerical outcomes. Draft
PR publication may precede final measurements to make the implementation
reviewable; readiness and merge still require the outstanding checks.

## PR 1344 review repairs and current checks

Draft PR https://github.com/Tealstreet/tealstreet-sdk/pull/1344 targets
`feat/rust-rewrite`. Three independent adversarial source reviews are posted.
The native reviewer found P2: a signing result captured at 18s but consumed at
25s could bypass its 20s deadline. Fix native and JS signing completion to read
the actual dispatch clock after earlier FIFO socket work. Websocket arrival
times remain recorded. This supersedes the merge plan's captured signing-clock
claim and the first JS review's no-findings result for that case. Independent
fix review passes. The actual native-engine regression fails with stale time
and passes with the fix: 19.5s permits only 500ms HTTP time, 20s/25s emit one
signing failure and no HTTP, duplicates do nothing. All three JS variants
likewise fail stale-clock assertions and all 80 host tests pass after repair.

Current merged-source harness665, typecheck44, lint28 and repository27 tasks
pass. Preserve initial typecheck failure: update incoming test loader calls to
select Paradex and mock the new host interfaces without changing assertions.
Formatting, bans and full native Clippy passed before the deadline repair;
intentionally interrupt the subsequent workspace test compile to fix reviewed
source coherently. Preserve its SIGINT and reason, then rerun fixed source.
`RUST_PERF_DELIVERY27_REVIEW_CHECKPOINT.json.gz` retains this evidence.

Initial CI is not green: size breaches include Bitrue 850,462/805,888,
Hyperliquid 1,095,063/1,090,560 (also Spot), and provisional KuCoin Spot
720,853/708,608. Hyro steady CPU is 4.3ms versus TS 3.8ms; retain this failure.
Before any cap exception, remove the two delegated signing exports from the
26 packages that cannot request them: only Paradex calls `Cx::sign_request`.
Make declarations optional and assert actual method presence only in Paradex
builds. Independent source review passes; one fresh 27-package build must
establish actual exports and remaining sizes. No compiler profile trials,
threshold changes or CPU improvement claims accompany this capability gate.

The current CI roster is 40 public/64 private, fixed 20 AB/20 BA. Preserve old
39/63 IDs/order and append KuCoin Spot. Content checks require changed Bitrue
and new KuCoin Spot private generators; reuse 62 verified tapes. Public tapes
changed for Bitrue, KuCoin and Paradex, plus new Spot. Source-bound old numbers
remain historical. Final real-WASM checks, measurements, wider margins and
green CI remain pending.

## Current CI evidence and bounded collection policy

At `a5814effbc`, CI passes actual-WASM SDK436/15 files, host182, product16,
frame-wire1 and native session1. Native test/Clippy and wasm32 jobs also pass.
The workflow remains red: four package families exceed caps (Binance966515,
Bitrue848368, Hyperliquid1092969, KuCoinSpot718820), and Hyro steady CPU fails.
The unused signing export gate saves 2094 bytes on Bitrue/Hyperliquid and 2033
on Spot compared with initial CI. These are size observations, not CPU wins.
Review only four explicit actual-plus-3%-rounded-KiB caps after fresh physical
Node/web verification: 996352/874496/1126400/741376 respectively. Other 23 caps
stay fixed; do not perform more compiler trials. Preserve both CI failures and
the initial SDK failure (434/436; missing Paradex loader argument, now fixed).

All 64 private tapes are prepared: two offline generators (changed Bitrue and
new KuCoin Spot) pass; 62 prior tapes/logs are byte-verified unchanged. The
40-profile roster preserves prior ordering and appends Spot, with 20 AB/20 BA.
Use Node22.23.3 for the next source-bound collection, matching CI's major
version, with one executable/hash for both arms. This explicitly changes the
Node26 method; no existing numerical outcome is retried or relabelled.

Declare independent-case continuation before any new public samples: execute
each private case and each fixed public arm once. Preserve operational, parity,
audit, missing-output and invalid-metric cases as unresolved, never passing or
contributing a comparison. Compare only fully valid pairs with identical
per-implementation logical hashes across all six samples. Any unresolved case,
missing coverage or failed objective prevents overall pass. Source/config/
binding drift remains globally fatal. Check and clean the owned process group
after every gate; leaked descendants make that case unresolved, and inability
to confirm cleanup stops the whole collection. Pure counterexamples pass101;
no engine children or configuration freeze have run. Independent review of
the final runner remains required. `RUST_PERF_DELIVERY27_ROSTER_CI.json.gz`
retains the metadata, scripts, CI numeric receipts and full CI status.

CI now retains offline replay JSON and isolated family packages for three days,
including failures, with attempt-specific artifact names. Exclude tapes,
environment files, source and aggregate bindings. This improves reviewable
evidence and does not add publishing or live-account automation. Local full
Rust verification and isolated current-control/release builds are still active;
the PR remains draft and the full performance task remains unfinished.

## Verified 27-family release checkpoint; merge consumer landing before scoring

Source `a5814effbc` now passes the complete local Rust checks: 2,813 tests in
212 groups, formatting, banned patterns, strict native workspace Clippy and
strict WASM workspace Clippy. All 27 isolated family Clippy checks pass. The
release produces 243 physical family files; independent review verifies their
bytes, 27 equal Node/web binaries, 470 unchanged selected source inputs and
243 archived prior artifacts. Under pinned Node22.23.3, actual WASM SDK436,
host182 and offline parity44 pass. These integration replays are unscored;
no final public/private performance cohort was started.

Keep the size gate failure. Physical local sizes are Binance966505,
Bitrue848358, Hyperliquid1092744 and KuCoinSpot718746; other23 families fit.
They differ slightly from CI, so the earlier exact-match condition does not
hold and no cap edits are authorized by that conditional review. Preserve the
failed size log. The original control's 435/34 protected inputs,9 aggregate
bindings and181 refreshed common files pass independent checks, but remain
unscored historical inputs. `RUST_PERF_DELIVERY27_RELEASE_CHECKPOINT.json.gz`
retains the source/release/control proofs, actual tests, failures and reviewed
134-counterexample collection policy. Its SHA is
`20291867dc6f8074ee8fc51628ac616f577e0bf044a147e47064a431a27923c0`.

Landing advanced to `fc809a9d85` (#1342): public-feed ownership, consumer
startup and packaged loader proofs change production sources. Next phase:
merge it before scoring; preserve shared-aware book reservation and ticker
revisions, the bounded ingress/signing contracts, and Paradex's incoming
host-z/engine2 profile. Adapt CLI/MCP/React Native consumers to venue-family
artifacts, with no aggregate runtime fallback, and prove packaged startup
outside the checkout. Use the incoming guarded URL resolver as the sole
socket URL cache. Run phase review, required package checks and fresh actual
family builds. Prepare a new exact landing control in an isolated target;
only then freeze the common helpers/Node22 inputs and execute the declared
40-public/64-private collection once. The draft PR1344 remains unfinished;
CI must pass before merge into `feat/rust-rewrite`.

## Landing consumer merge verification

Merge `fc809a9d85` preserving received-event ordering, signer wall-clock budgets,
shared public-feed ownership and compact store indexes. Snapshot/frame capacity
prepasses now use the same policy-aware books as their writers; ticker revisions
survive replacement/shrinking. Keep incoming Paradex host-z/engine2 profile in
both isolated and family-loop builds.

CLI embeds 27 literal family assets and initializes the selected family through
the shared browser cache. MCP copies/loads canonical family CJS/WASM pairs;
its isolated probe checks Binance/Coinbase independence and aliases, and missing
selected assets must fail. React Native receives a canonical family and caches
initialization independently; new unit tests cover concurrency, retry isolation
and unsupported names. These packaged proofs still require fresh artifacts.

Independent phase reviews found two P2 issues and verified their source fixes:
the new host public-feed test referenced stale aggregate bindings (now Phemex's
family), and URL caching missed mutable custom query/JSON normalization rules.
A stateful listener-routing regression fails before the resolver repair; full
Tape/build103 and harness669 pass afterwards. Keep one guarded default URL cache.
The broader consumer loader review has no remaining P1/P2 source findings.

Merged physical sources pass Rust2840/218groups, formatting, bans, strict native
all-target Clippy and strict WASM workspace Clippy, with447 inputs unchanged.
Repository typecheck44, lint28 and test27 tasks pass; mobile cache3 tests pass.
Preserve the initial formatting failure and initial CLI file-import type error:
Bun supplies asset paths while generated WASM declarations describe exports;
keep the narrow cast at the file-read boundary. No thresholds changed.

The new original control is exact `fc809a9d85`:442Rust/config,35 protected
inputs,29 fresh internal compile paths and9 physical aggregate bindings. Its
Node/web WASM is identical at12172135bytes; an independent physical audit passes.
No original-control or candidate performance samples were collected. Next:
archive the previous243 family files plus9 historical aggregate files, rebuild
all27 families, run actual SDK/host/consumer/offline integration checks and
size review, then freeze the40/64 comparison with the declared no-retry policy.

## Actual family/consumer checkpoint and lazy codec repair

The `aa96322d86` release passes all27 isolated builds/Clippy checks. Independent
physical audit verifies1002 source inputs,252 archived files and243 fresh family
files with matching Node/web binaries. Preserve the failed size gate:26 families
exceed the earlier caps after incoming shared-feed functionality; Paradex fits.
No budgets changed. For final-source size review, independently conditionally
reviewed policy is to raise only verified breaches to actual+3%, rounded up to
KiB; retain fitting caps, publish old/actual/new, and require CI to pass. This is
separate from the unused earlier four-family CI-exact condition and changes no
CPU/memory gate or compiler profile. Final-source physical evidence is required.

One actual15-command integration run passes14 commands: SDK448/17files,
host185, offline parity44, server11, MCP5, CLI2, MCP build and isolated packaged
smoke. React Native fails its codec-free native-registry import: the host's
module-level TextEncoder construction runs before the probe restores codecs.
Preserve this failure. Repair the host to reuse its existing lazy per-host
encoder only for text delivery. Binary-only batches instantiate no encoder.
The unchanged React Native actual tests then pass2/2; full host tests pass186,
including a new isolated import-without-codecs regression. Independent review
finds no remaining issue. No full-suite or performance pass is inferred from
this targeted repair. `RUST_PERF_DELIVERY28_RELEASE_CHECKPOINT.json.gz` retains
all evidence, SHA `c52952d4cd3e5d157b060957fb4002256425744de5d29736414ef286b9e6fc85`.

The target advanced to startup/broker landing `9c34020741`. Merge it before
scoring: canonical BloFin option gating, KuCoin cancellation scope, broker
contracts, Bitrue time-in-force correction and startup signing-clock reset after
module setup. Preserve existing CPU/memory cutoffs and readiness continuity;
apply the clock correction identically to both arms. Only KuCoin futures needs
one new private tape;63 can be hash-reused,40/64 roster/order unchanged.
An exact new original control is already built and independently audited:
446Rust/config,35 protected inputs,29 fresh internal compiles,9 bindings,
Node/web WASM12174466bytes identical. Old controls and failed samples remain
untouched. New final cohort remains unstarted; PR1344 remains draft.

## Startup/broker merge checkpoint

Merge production landing `9c34020741` plus docs-only commits through `702a62050c`.
Preserve broker attribution, KuCoin full-ID cancellation continuation and explicit
account-scope cancel-all, and Bitrue's corrected private time-in-force mapping.
The new BloFin WASM field uses canonical `venue-blofin` gating. Preserve batching,
optional ingress APIs, family isolation and the lazy-codec native import repair.

Apply incoming startup signing-clock resets after module/crypto setup, before
startup; TS clock installation follows `loadTs`. Retain the original measurement
CPU/wall start, initial heap and final pre-GC cutoffs, fixture lease and readiness
continuity. Independent merge review passes. Update only the clock test's const
to let extraction anchor, keeping all its runtime assertions. Full harness670,
typecheck44, lint28 and repository test27 tasks pass. Preserve the first typecheck
failure from the incoming broker test fake missing `wsMessages`; add that stub
without changing assertions. Full Rust451-input verification is still running;
formatting, bans and native all-target Clippy pass so far.

Prepare one new KuCoin futures private tape against the committed merged source;
reuse63 verified tapes and the exact40/64 roster. Original control29 is exact
`9c34020741`, independently physically audited; the newer landing commits alter
only `RUST_AUDITS.md` and `RUST_QUALITY.md`. Shared-helper refresh and configuration
remain drafts, with no final gate children or frozen collection. Rebuild current
families and complete actual integrations, final size review and source checks
before freezing the existing Node22 no-retry collection protocol.

Full source29 verification now passes:2,857 Rust tests across222groups, no
failures or ignored tests, formatting, banned-pattern checks, native all-target
Clippy and full WASM Clippy. All451 Rust/build inputs remain unchanged at
`7bc9e30d32`. The shared control refresh is applied and independently audited:
181 shared files match the candidate, while446 original Rust/config inputs,
35 protected inputs,9 aggregate bindings and1,804 original package-backup files
remain intact. Only the expected packages/node_modules aliases are present.

The final-source27-family rebuild and27 isolated Clippy checks pass. Independent
physical review verifies1,008 source inputs,243 fresh artifacts,252 archived
artifacts, all31 command receipts and identical Node/web WASM per family.
Release proof SHA `725379d643a2852301e35a6deb1c2ae1598953e3dbf838b5f4f9745b322bf014`;
independent audit SHA `c0aef86eb03fe06316ce1290e150e99f75f4ab39d6efd344b177e3c5917d5a2e`.
Sizes range758,842–1,237,634bytes;26 old caps fail and Paradex fits. Preserve the
official failing size-check log before separately reviewed cap rebaselining.

A new adversarial review finds a P2 in multi-message packing: encode-only
TextEncoder implementations throw, and incomplete encodeInto results truncate
text. The singleton path already handles both cases. Add full encode() fallback
to packing when encodeInto is missing or does not consume the complete string.
Four queued two-text/mixed regressions fail before the fix and pass afterward,
with zero skips, preserving Unicode, binary bytes, ordering and arrival clocks.
The finding is posted on PR1344. Only host.mjs and host.test.mjs change; all Rust
and build inputs and243 generated bindings remain byte-identical. Keep the
immutable physical build proof at7bc and record the committed JavaScript input
overlay explicitly rather than relabeling old build command receipts.

Actual WASM integration, final size review and the fixed40/64 measurement
collection remain next. No current-cohort performance result is available yet.

Final JavaScript input freeze SHA
`4e7a8c2a96889cdeb4d4131f934080c71895e91f1dfcc9c17eb1b4a91160b0a1`
pins `8150295a7c` over the unchanged7bc Rust build. Independent physical overlay
audit SHA `5906590285a1fe7842c867ae6fe755d4e044ebcec3b822bc4abfa0e2e81af8e5`
confirms exactly2 JavaScript differences,1,006 other inputs unchanged and all243
bindings unchanged. Original build command provenance stays7bc.

Apply the independently reviewed cap formula only to26 measured breaches;
Paradex stays unchanged. The official size check now passes54 Node/web entries.
Publish every old/actual/new value in `RUST_PERF_FAMILY_SIZES.md`; preserve the
original failed check. Size proof SHA
`fac8acd15de47824d08df018608f2a5f9bb56328c0e84c9f4caf489dae4cb8ef`.
The actual15-command integration suite is running once against frozen current
inputs; the fixed40/64 performance collection has not started.

All15 actual integration commands now pass, with22 successful tool/test receipts:
SDK449 tests, host190, offline parity44, consumer20, MCP build and isolated
packaged smoke. No retries, skipped tests or source/artifact drift. Proof SHA
`21ab591c2a939decb24ae44e544bdea599361afdcc30734ed3dd612e6923942d`.
Readiness verification passes, SHA
`1639135bcdf4e24cadcdb7d42f1dd8e81252ee58bccbc1760c0d61e475f104e1`.
`RUST_PERF_DELIVERY29_RELEASE_CHECKPOINT.json.gz` retains99 evidence files,
including original failures, source/release/control audits and complete actual
integration logs; SHA
`341655e524338c548346357b1e71a4af0b420be9a31a6b6328df8655742794f7`.

Next execute exactly one declared Node22 collection with the existing reviewed
runner and immutable40-public/64-private roster. Keep20 AB and20 BA pairs with
three public runs per arm, no numeric retries, unchanged output/CPU/memory/GC/
timeout requirements, and independent-case continuation that preserves invalid
results as unresolved. All build/test/profiler writers must be quiet. Freeze
actual configuration, independently inspect its rows, then launch. Performance
acceptance and green CI remain pending.
