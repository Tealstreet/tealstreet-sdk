# Engine performance

Branch: `feat/rust-perf`, based on `origin/feat/rust-rewrite`.

Work is full auto under VENUE_PORT_BRIEF Process: phase reviews, commits,
public/private replay gates, PR to feat/rust-rewrite, adversarial reviews,
green CI, merge and fresh follow-up branch. No stash/rebase/force push.

## Phases

1. Baseline and profiling. Replay committed public recordings, measure total
   CPU/memory, websocket boundary/parse-and-update time and flush frame time.
   Keep profiling optional so production measurements carry no timers.
2. Ordered host batching. Queue socket arrivals to one delivery per macrotask;
   retain socket, text and arrival time. Borrow decoded messages in Rust.
   Flush before non-message events/commands/snapshots, bound memory and preserve
   action ordering. Native host drains ordered ready messages with arrival time.
3. Venue builds. Optional WASM Cargo venue features, one package per family,
   explicit venue-selected web/node loaders, per-family CI budgets. Aliases
   share family artifacts. Remove superseded aggregate imports/artifacts.
4. Gates and landing. Merge landing frequently, run all registered venue public
   and private gates, compare same-machine before/after CPU and retained memory,
   phase/adversarial review, repository checks and green PR CI, then merge.

## Progress

- Read current brief/spec and relevant package guidance. Clean feature branch
  created from landing. Existing host crosses/drains per message; current
  replay yields per frame, which may hide real network bursts. Batching replay
  must use recorded timing fairly for TS and Rust, retain existing correctness.
- Phase work assigned to disjoint native-host and venue-build subagents.

### Initial measurements and phase review

Original aggregate build, original per-frame replay, three-run medians on the
same macOS/Node 26 host (shared machine, absolute CPU varies with contention):

| Venue | TS CPU ms | Rust CPU ms | Ratio | Retained ratio |
| --- | ---: | ---: | ---: | ---: |
| OKX | 649.5 | 595.7 | 1.09x | 1.02x |
| Bybit | 232.6 | 156.6 | 1.49x | 1.35x |
| Binance | 171.6 | 62.2 | 2.76x | 1.12x |

Optional, independently reviewed probes show raw text crossing is not the main
cost: OKX 3.62 ms CPU, Bybit 4.11 ms for a complete fixture. JSON syntax
validation plus crossing is 6.80/7.45 ms; this is a parser floor, not the full
venue decoder. Instrumented wsMessage (crossing + typed parse/state + action
serialization) is 170.5/222.5 ms wall; takeFrame is 3.78/26.13 ms. Instrumented
wall samples are diagnostic and include scheduler noise, not gate CPU results.

Native phase review passed after removing asynchronous admission backpressure
that allowed later HTTP/timer events to overtake an earlier socket arrival.
Native drains a finite ready prefix before commands/flush/shutdown, in bounded
slices. Existing transport inbox remains unbounded, with no copied batch buffer.

JS phase review found and fixed a capacity-flush reentrancy reorder. It also
identified external ArrayBuffer storage omitted by the old memory gate. The
new measurement counts ArrayBuffer delta alongside heap and WASM memory;
original baseline will be remeasured with the same accounting. Scratch is lazy.
The initial batching run improved CPU but regressed memory; gates remain open.

### WASM batching and family-build implementation checkpoint

- Batch wire: little-endian socket u32, UTF-8 byte length u32, arrival f64,
  followed by borrowed UTF-8 text. Whole batch validates before any state change.
- JS queues up to 256 messages / roughly 1 MiB, preserving arrival order across
  sockets; barriers drain first. Large scratch is released after delivery;
  retained scratch is capped at 64 KiB. Disposal ignores late transport events.
- Family modules are selected by venue across browser, Node, harness and Workers;
  aliases share modules, browser initialization is cached per family with retry.
  Isolated clippy, 20 SDK WASM tests, 3 Rust WASM tests, 16 host tests pass.
- Independent reviews passed profiling, native batching and family registration.
  Workers review exposed duplicate/orphan async initialization and commands lost
  during a pending download; guards and deferred-loader regression tests added.
- Exact escaped-string output reservation lowers OKX startup WASM highwater from
  1,441,792 to 1,114,112 bytes (five pages). Same subscription JSON and replay
  output. A first strict-memory recorded-burst gate passes (1.16x CPU, 1.01x
  retained memory) before scratch-retention improvement; broader gates remain.
- Public replay now yields between distinct recorded timestamps, preserving
  equal-timestamp network bursts for both implementations. `--cadence frame`
  keeps the earlier pacing; `--delivery single` isolates batching changes.
  Re-running the original isolated binary with identical cadence and strengthened
  ArrayBuffer accounting for the controlled before/after table.

### Allocation follow-up (reviewed)

Measured Bybit's first frame allocation added five pages by repeatedly growing
its output Vec. Whole-section reservations eliminate that growth; standalone
trace saves four pages overall (1,835,008 -> 1,572,864 bytes after later batches).
Store now offers an allocation-free instrument-ID ticker update, preserving
symbol lookup and dirty masks across reordered/replaced lists. OKX subscriptions
serialize only 100 borrowed topics at a time with identical original ordering;
ticker updates borrow native IDs and stop allocating duplicate set keys.

Two independent static reviews pass; engine's 16 tests and clippy pass. Full
workspace tests/build plus repository typecheck/lint/tests are running.

Final optional diagnostics (recorded cadence, three instrumented runs) observed
OKX 8,904 single calls -> 427 batches, Bybit 8,742 -> 126. Raw text crossing is
~4 ms CPU per fixture, prepared 64-message buffer crossing ~0.1 ms. Combined
instrumented WS wall medians: OKX 240.23 -> 34.17 ms, Bybit 48.84 -> 18.96 ms;
frame calls 4.49 -> 3.24 / 3.64 -> 3.45 ms. Wall samples include scheduler noise;
these are diagnostics, not final uninstrumented CPU gate numbers.

### Four-family integration and adversarial fixes

Merged Crypto.com landing `c63c3e5670` with the lane's family isolation. It also
landed stable async backing-buffer collection and captures CPU before final GC;
preserved both. The controlled before build is now rebuilt from that landing
source with the final replay/gate but original aggregate binary/unbatched host,
so all four venues share the same timing, memory accounting and burst cadence.
Earlier exploratory numbers above are not a final before/after comparison.

Adversarial review found/fixed:

- Native command clock was captured after yielding to drain a ready prefix;
  later arrivals could then carry an earlier time. Capture the selected command
  time first; regression verifies prior/command/later clocks 70/80/90 even when
  dispatch wall time reaches 100.
- Disposal left in-flight command promises pending after late HTTP completions
  were suppressed. Reject/clear them and reject new commands after disposal.
- Disposal from an action callback could leave later socket/timer actions live;
  stop action execution and recheck disposal after every draining barrier.
- Crypto.com profiling used the wrong default symbols. Explicit family defaults
  and a cadence option now match each fixture.

Independent review reproduces the disposal fix and finds no remaining harness
or family-build issues. Native six tests/clippy, 21 JS host tests, 20 SDK WASM
adapter tests and four isolated wasm32 clippy checks pass. Repository-wide
44 typecheck and 28 lint tasks pass; full tests are still running.

Family WASM bytes/budgets after integration: Binance 769031/786432,
Bybit 748585/765952, Crypto.com 617650/638976, OKX 711518/729088.

Follow-up adversarial test found a synchronous invalid WASM command registered an
unreturned pending Promise; rejecting it on dispose could become an unhandled
rejection. Validate first, register before executing returned actions, and clean
up transport/action errors through the returned Promise. The regression fails
before the fix and passes afterward. All 24 JS host tests and six native tests
now pass; independent review reproduces resource-disposal safety.

Full native workspace fmt, bans, all-target clippy, tests and wasm32 workspace
clippy pass after Crypto.com integration. Four-family baseline aggregate build
is complete. Controlled before/after public runs and seven final private tapes
are running; the first baseline OKX run passes (1.08x CPU, 1.70x retained memory)
under stable collection, illustrating why earlier exploratory memory results
must not be mixed with final measurements.

### Benchmark fidelity corrections

Discard the first four-family "controlled" before table: a shared Cargo target
reused current engine artifacts, despite an older WASM host source. Rebuilt the
control in a separate target directory, removed every workspace fingerprint from
cloned third-party caches, and audited all 84 Rust files against pinned landing
`e6e410146e3a` (same Rust behavior as `c63c3e5670`). Build logs prove all seven
workspace crates compiled from baseline paths. Aggregate is exactly 1687019
bytes, SHA256 b85246870021d46ed8295b3c4b0242f7e7d20a27a510d9c761c3e5ef4b28a60a;
it has no batching export or new allocation helpers. Main artifacts unchanged.

Profiling also located dominant *shared harness* CPU: old subscription coverage
scanned 2472 topics for every frame, costing 456.28 ms CPU for the actual OKX
fixture. Candidate indexing costs 6.79 ms with the identical final predicate,
wildcard semantics, insertion/error ordering, duplicate replacement and strict
unsubscribe. Frozen-old-oracle differential checks, 600 deterministic mutations
and the entire fixture pass; all 12 harness tests pass. Nothing is skipped or
normalized. Both control and current runs use this same final harness, stable
GC accounting and recorded cadence; all comparison numbers are being rerun.

Subscription index passed independent review and all 12 harness tests; no checks
or normalized fields changed. The malformed-result edge now rejects its pending
command rather than leaving it unresolved; all 25 host tests pass.

First valid isolated-control/final-harness comparison, three-run medians:
OKX CPU 1.49x -> 1.81x, retained memory 1.60x -> 1.89x (Rust 209.2 -> 167.8 ms;
WASM 1.4 -> 1.0 MB). Other three family comparisons remain in progress. Full
repository verification is green: 44 typecheck tasks, 28 lint, 27 test tasks.

### Five-family integration and remaining margins

Merged WEEX landing at `7e4dee9c96`, preserving authenticated socket upgrade
headers, capability-derived options, 20-second HTTP lifetimes, ticker-symbol
commands and all reviewed batching/lifecycle behavior. Family builds now include
WEEX. Root replay gained an explicit script for diagnostic runs.

The valid four-family comparison showed wider CPU/memory on OKX and Bybit, but
Binance's retained memory regressed slightly and Crypto.com still failed cold
main-thread CPU under machine contention. Do not mark these gates complete.
Profiling Crypto.com identifies streamed startup HTTP as its largest engine
component; controlled after-build measurements remain required.

Reviewed follow-ups (25 engine tests/clippy pass):
- Streamed JSON copies ordinary string runs, preserving the scalar state machine
  at quotes/escapes, UTF-8 checks, full syntax validation and metadata limits.
- Store shares the symbol index when **all** market IDs equal symbols, otherwise
  retains the full original ID map. This preserves duplicate/mixed-ID semantics
  and avoids unnecessary maps/strings on Binance, Bybit and Crypto.com.
- Retained JS batch scratch is capped at 16 KiB (batch count/byte bounds unchanged).
  A focused regression verifies medium scratch is released and small scratch reused.

All five family artifacts and the updated memory profiles are rebuilding. The
next control must pin the five-family landing source and retain an isolated
Cargo target; use the exact final harness on both sides.

Five-family isolated build/checkpoint passes: 34 host tests, 21 SDK WASM tests,
five isolated wasm32 clippy checks, SDK typecheck and loader lint. Package bytes:
Binance 776253, Bybit 755810, Crypto.com 624732, OKX 718732, WEEX 858358;
WEEX budget tightened to 884736. The next landing fetch includes BingX, so final
comparison and registration expand to six families before reporting gates.

Integrated BingX landing `022ab79de5`, retaining its boxed market keys, sorted
ticker lookup, collision-safe string index, full streamed-envelope validation,
compressed socket support and stricter request/subscription/private classification.
Harness has 31 passing regressions; public thresholds remain unchanged. Both
comparison sides now require 100 ms elapsed backing-buffer quiescence, with CPU
measurement ending before collection. Added sixth isolated family registration.
True six-family control rebuilt in its own target: all nine workspace crates
compiled from pinned source, 116 source/manifest/config files audited, aggregate
2576287 bytes, SHA256 309140b910d6905f1b33e662e89f57b791fc1d5039a50d35ed9c1bd2db2f2fa9.
Eight private gates pass before this merge; final nine-gate rerun remains due.

Six-family verification passes: 421 native workspace tests, bans/fmt/all-target
clippy and wasm32 clippy, six isolated-family clippys, 43 JS host/socket/Workers
tests, 23 SDK WASM tests, SDK typecheck/lint. Repository typecheck (44 tasks) and
lint (28 tasks) pass; full repository tests and final replay gates are running.
Fixed budgets calibrated against final shared selectors, bytes/budget:
Binance 792804/817152, Bybit 772749/796672, Crypto.com 641665/661504,
OKX 735378/757760, WEEX 875552/884736, BingX 829184/856064.
Independent review reproduced and verified an incoming compressed-socket loss:
remote close/error now follows preceding decoded messages, raw receive timestamps
survive decoding, local close cancels pending delivery. Native stamps before
inflation. Guarantee covers per-socket decoder order; existing independent async
binary decoders are not serialized with unrelated sockets/HTTP/timers. Text input
retains the engine batch barriers. Focused independent 22-test review passes.
Optional memory tracing skips methods absent from the unbatched control.

Repository rerun fully green: 44 typecheck, 28 lint, 27 test tasks. Final merged
private correctness tapes pass for all nine variants: Binance 14 frames, Bybit18,
Crypto.com21, WEEX20, BingX38, each OKX variant29. No unmatched requests/results.
Follow-up memory review confirms market/ticker tables never append after Store
replacement. Their setters now shrink spare incoming vector capacity before
index construction; duplicate identity, row ordering, deltas and replacement
cleanup are unchanged. Engine38 tests and independent review pass. Six release
families rebuild before final public comparisons; no gate thresholds changed.

Integrated MEXC landing `63d521a860`, expanding to seven isolated families and ten
private variants. Kept the per-family release opt-level 3; the exact incoming
aggregate control uses its committed opt-level s. Isolated baseline7 compiles all
ten internal crates, audits 128 tracked source/manifest/config files, and emits
2416535 bytes (SHA256 27cd96234cc5fdf92aa774f9544d71c93564f1eb7f0008c280bc1a995de9a4ba).
The new shared tape/socket ordinals and virtual clock are retained for MEXC;
its clock advances once per timestamp group so it does not cancel batching.
Existing six venues retain strict typed request matching and existing clocks.
MEXC-specific generated OID/snowflake/window normalization stays venue-scoped.
All45 harness regressions and independent merge/partition review pass.
Store frame reservations include MEXC's optional two leverage columns; exact-size
and no-growth regressions pass (43 engine tests). Trimming table capacity saves
one 64KiB Crypto.com page. JS now partitions bursts at 64KiB, one oversized message
alone, processing all original partitions before any returned transport actions.
This avoids grouped-book peak growth and preserves synchronous callback ordering;
22 host tests pass. Final seven-family builds and comparisons remain in progress.

Under heavy shared-host load, the native integration test exposed an existing
5ms subscription race. Replaced polling and fixed sleeps with observed outbound
subscription/depth-response notifications; shutdown's finite-prefix barrier now
makes the final assertion deterministic. Native unit/integration/clippy pass.
A comparison attempt also exhausted the one-second memory settling timeout.
Only its deadline expands to ten seconds; the identical-readings-for-100ms rule,
hard failure, accounting and CPU-before-GC cutoff are unchanged on both sides.
That failed/aborted comparison attempt is not a final gate measurement.

Seven-family verification passes: 478 Rust tests across 50 suites, all clippy,
fmt/bans; seven isolated wasm clippys; 53 host/socket/Workers tests;23 SDK WASM
checks;44 repository typecheck and28 lint tasks;134 targeted MEXC SDK tests.
Fixed MEXC budget tightened to778240 for measured755047 bytes. Final module bytes:
Binance799297, Bybit778868, Crypto.com647592, OKX741200, WEEX881767,
BingX835350, MEXC755047. All family budgets pass.
Final page traces versus exact baseline7 confirm Binance1507328→1441792
(65536 bytes saved) and Crypto.com786432→655360 (131072 saved). These traces
are allocation evidence, not instrumented retained-heap gate comparisons.
Seven public before/after pairs and final profile medians remain to run; private
suite is passing through the newly built families, including MEXC.

First complete seven-family paired run is diagnostic, not the final table.
All public outputs passed except Crypto.com subscription audit, on both control
and current: TS registered while its socket was CONNECTING, lost sends and retried
only after accelerated replay. The merged readiness helper now waits for live
open before registration, then exact audit readiness before delivering frames.
No subscription validation changed. Deterministic real-Crypto-hook regression and
all49 harness tests pass; a one-run strict Crypto gate also passes.
CPU/retained ratios improved on OKX, Binance, Crypto.com, WEEX and MEXC; Bybit and
BingX retained ratios regressed slightly despite passing gate thresholds. Work
continues until both margins widen. All ten private variants pass on seven-family
artifacts (MEXC21frames, other counts above). Optional CPU profiling now reports
gross per-method thread CPU with a measured proxy/clock floor, labelled explicitly;
normal gates still import no profiler. Final profile CPU runs remain pending.

Memory follow-up passes45 engine tests/fmt/clippy and independent algorithm review:
identity ID/symbol markets reserve3 candidate strings instead of4 and omit the
duplicate string bytes/offset from the frame bound. Mixed native IDs now use
sorted u32 row indexes borrowing market IDs instead of cloned-ID hash entries.
Descending row tie order preserves last duplicate; replacements rebuild indexes.
The existing immutable-identity contract is explicit and debug-asserted. Estimated
BingX wasm allocation saving is at least38195 bytes before allocator overhead;
actual pages and O(log n) mixed-ID lookup CPU remain gated after rebuilding.
BloFin plus MEXC parity just landed at origin `a4d9e2ec84`; integration expands to
eight families before the next single release rebuild. No final gate claim yet.

Integrated eight-venue landing `a4d9e2ec84` (BloFin and MEXC parity follow-up).
Engine51 tests/fmt/clippy and independent Store invariant review pass. Harness56
tests pass: adopted recursive caller IDs and typed per-venue generated/default
query matching; BloFin egress oracles; latest MEXC post-cancel reconciliation.
Crypto live-open/complete subscription readiness and MEXC-only grouped virtual
clock remain. Extracted memory collector now requires at least two equal
comparisons plus100ms quiet, with10s deadline and real timers; CPU still ends
before collection. Deterministic delayed-cleanup tests protect the bounds.
Eight-family current and isolated baseline release builds run with two Cargo
jobs; baseline exact130-file source audit matches a4d9e2ec84, opt-levels unchanged.
Final public comparisons and gross CPU profiles await these artifacts.

Eight-family controlled public comparisons all pass strict output/egress and
CPU/memory thresholds, but retained ratios regress slightly on Bybit, BingX,
BloFin and WEEX; these are diagnostic, not completion numbers. Page traces show
Bybit retains the drained streamed-market page allocation through startup, and
combined Store/book frames can grow the whole output after string metadata is
live. Reviewed fixes adopt the first Bybit page and drop later drained buffers
before startup, and reserve the combined Store+valid-book bound before writing.
Engine52 and Bybit43 tests, fmt and strict clippy pass. Composition regression
proves unchanged bytes with no buffer growth; missing-market/book and market-only
state semantics are covered. Independent plan/implementation review passes.
BloFin isolated budget is783360 bytes (measured759953), replacing its missing
manifest entry. Next integrate Backpack landing before one release rebuild.

Backpack landed at `eff4dcbb0f7bb5d5f19c28d6d9cf263eaf6cf0f5`; integration
adds the ninth optional family, cfg-scoped recvWindow and venue-selected loaders.
The landing extraction of replay Tape is retained while restoring the strict
typed response oracle on existing families, indexed subscription coverage,
recorded-timestamp batching, MEXC-only virtual-clock scope and Crypto live-open
readiness. Backpack signature/write-order/caller-ID checks remain intact.
Host merge review and56 targeted JS tests pass;44 repository typecheck and28
lint tasks pass. Full repository tests run while final release artifacts build.
Baseline9 is exact142-file source-audited aggregate at that landing, built in
its isolated target with original unbatched runtime and original opt=s/host=z:
2863225 bytes, SHA256
`58cb11561450d4544b1f1ad3f1d709048bd578981e9c6a361b070112afda2c4e`.
Final runtime helper copy waits until merge verification completes.

Nine-family integration checks pass: all9 isolated wasm clippys and fixed
web/Node size budgets;67 host/socket/Workers/loader tests;25 SDK WASM tests;
86 harness regressions;44 repository typecheck,28 lint and27 full test tasks
(full tests7m12s). Workspace fmt/bans/native clippy pass; warning-denied Rust
workspace tests and final wasm workspace clippy are still running.
Fresh page traces against exact baseline9 demonstrate Bybit1572864→1179648,
BingX1507328→1441792, BloFin851968→786432, WEEX1572864→1376256,
Binance1507328→1376256 bytes. These are allocation diagnostics, not final
retained ratios or CPU numbers. All12 private fixtures regenerated; correctness
gates passing through MEXC, with OKX variants and WEEX still in progress.
