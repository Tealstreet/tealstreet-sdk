# P3a verification evidence

Branch: `feat/rust-p3a-candlecache`. Implementation: `af1b9b7851`.
Behavioral reference: [safe-cex/CLAUDE.md](../../packages/safe-cex/CLAUDE.md).
Measurements below were run on 2026-10-11, then repeated after restoring every
negative mutation. All final commands exited 0; no requested gate was skipped.

Rust commands use `--manifest-path sdk-rs/Cargo.toml -p safe-cex-candle-cache`.
Node/Yarn commands ran from the repository root with the installed Yarn 1.22.22
script under `/Users/sam/.volta/tools/image/yarn/`, launched by
`/opt/homebrew/bin/node`; the default Volta Yarn shim recursed and was stopped.
The existing locked `smol-toml@1.4.2` dependency was linked from Yarn's cache
into the extractor's ignored local `node_modules` after an initial missing-module
failure. No JavaScript dependency declaration or lockfile changed.

| Gate | Final command | Positive outcome | Deliberate negative mutation | Negative exit |
| --- | --- | --- | --- | --- |
| Formatting | `cargo fmt … --check` | 0 | Append unformatted private function to `cache.rs` | 1 |
| Strict clippy | `cargo clippy … --all-targets -- -D warnings` | 0 | Insert unused local in `CandleCache::get_version` | 101 |
| Native tests | `cargo test … --all-targets` | 12 passed, 0 failed | Invert the optional testnet-key condition | 101: testnet namespace assertion failed |
| wasm | `cargo build … --target wasm32-unknown-unknown` | 0 | Add a wasm-only `compile_error!` | 101 |
| TS coverage | `yarn parity:extract --check` | 0 changes, 0 added, 0 orphaned | Remove `cleanCandles` manifest row | 1: missing row named |
| Invention | `yarn parity:check` | 0 | Add unmanifested public Rust function | 1: invented item named |
| Status | `yarn parity:check` | 0 | Map `cleanCandles` to nonexistent Rust function | 1: unresolved item named |
| Report input | `yarn parity:report` | 0 | Append malformed TOML table | 1: unclosed table reported |

The initial strict-clippy control using `true == false` did not fail (exit 0);
it was reverted and replaced with the unused-variable control above. The report
is a reporting command, not a completion/status validator; its negative control
proves rejection of malformed manifest input. The separate status gate proves
rejection of unresolved mappings.

## Coverage and representation

The initial P3a report showed **42 / 43 ported, 43 distinct Rust items, 1 divergent,
0 not-applicable** for this crate. All five source manifest files are reconciled;
the barrel has 0 rows. The divergence is `KeyValueCandleCacheBackend.loadChunks`:
matching records use the existing typed `Candle` JSON boundary and discard extra
fields; TypeScript casts unchecked data. Wrong-chunk timestamp filtering happens
before decoding, malformed JSON and non-array records are skipped. The initial
implementation rejected invalid matching typed data; the PR 7201 fix below
skips that candle individually. The divergence has a manifest note and tests.

The tests port the source cache, key-value, testnet-key and calendar cases, using
a fake clock/local executor and recording backends. They additionally assert
empty-key hydration happens once, absent backend methods choose the correct
fallback, no-backend read-through still fetches, read-through failures propagate,
hydration failures release the concurrency slot, timeout releases queued work,
late data preserves fresh memory while filling gaps, sparse chunks are cloned,
and failed parallel writes/deletes leave siblings running. A read-only sub-agent
review identified sibling cancellation and missing backend-capability cases;
these were fixed and the follow-up found no significant core parity defects.
The reviewer could not run cargo from its PATH; the successful commands above
were run by the implementer with the installed absolute cargo path.

`CandleCache::queued_hydrate` mirrors TypeScript's success, rejection and timeout
accounting. TypeScript's rejection handler already calls `finish`; no rejection-path slot
leak or `ts-bug` marker was required. The timeout retry finding below is separate.

Runtime-specific IndexedDB, Redis and React Native storage adapters were excluded
as required, along with transport integration. `CacheRuntime` supplies local
spawning, cancellable timers and epoch-millisecond time. Backend futures are
boxed, owned and non-`Send`, costing allocation/dynamic dispatch and requiring a
host event-loop adapter on wasm; JS-backed adapters need no `Send` workaround.
Each cache lives on one executor. No workspace dependency declarations changed;
only the crate's workspace member and lockfile entry were added.

## Throwing hydration at the concurrency cap

The additional Rust regression
`throwing_hydrations_fill_cap_then_release_queued_hydration` starts three fetches
with a cap of two before polling spawned tasks. Both initial backend futures
return `Err`, corresponding to TypeScript thrown/rejected backend operations.
It asserts only two hydration tasks start, their errors spawn the queued third
hydration, and the third backend call succeeds. All requests complete without
clock advancement or timeout callbacks. The complete native suite now passes
**13 tests**; fmt and strict all-target clippy also pass.

Removing the Rust `hydration_running` decrement makes this regression fail with
exit 101: the queued hydration is never spawned. After restoring the decrement,
the regression and the whole native suite pass.

The TypeScript oracle fixture is
`fixtures/HydrationThrowProof.test.ts`. To execute it, copy it temporarily beside
`packages/safe-cex/src/candleCache/CandleCache.ts`, run package-local
`yarn test-unit src/candleCache/HydrationThrowProof.test.ts --maxWorkers=1`, and
remove the copy. It uses two literal synchronous `throw new Error(...)` backend
implementations, a cap of two, one queued successful hydration, and fake timers
which are never advanced. The source's async `hydrateFromBackend` converts those
throws into rejections; `queuedHydrate.catch` calls `finish`, whose guarded
counter decrement drains the queue.

Actual oracle outcome: exit 0, 1 test passed; third backend call ran,
`hydrationRunning` reached 0 and the queue emptied. Removing TypeScript's
`this.hydrationRunning--` made the same test fail (exit 1), because only two
backend calls ran. Both temporary TypeScript mutations and the fixture copy were
removed; TypeScript source is unchanged. Neither implementation leaks slots for
backend errors. The later timeout retry finding is separate from that error path.


## Timeout lifecycle and the preserved retry finding

Clock-driven regression `timeout_releases_cap_and_empty_key_retries_before_late_writer`
sets a cap of one and a 10 ms timeout. At 9 ms the first backend operation still
holds the slot. At 11 ms the queued second key hydrates, the timed-out fetch
returns, and the timeout callback fires once. An empty-fetch retry of the timed-out
key makes a third backend read while the original read remains unresolved:
timeout alone does not mark the key hydrated. Resolving that original read after
the newer hydration inserts its stale candle into an empty slot. A fourth key
still hydrates, proving late completion does not decrement the slot twice.

The TypeScript oracle fixture `fixtures/HydrationTimeoutProof.test.ts` runs the
same flow with fake time, a deferred backend promise, backend-call assertions and
explicit checks of `hydratedCacheKeys`, `hydrationRunning` and the queue. Execute
it by temporarily copying it beside the source `CandleCache.ts` and running
package-local `yarn test-unit src/candleCache/HydrationTimeoutProof.test.ts --maxWorkers=1`.
The temporary copy is removed after execution; maintained TypeScript is unchanged.

Both languages **continue** backend work after timing out the caller's wait.
Neither cancels the old operation or rejects its late result. `mergeChunks` with
`existingCandlesWin: true` protects present memory candles, but lets stale data
fill gaps; the existing late-overlap regression checks this precedence. Late
successful completion also marks the key hydrated. This is the source's explicit
continuation behavior, not a claim that abandoned work cannot write.

**Production finding:** `timeout_memory_map_suppresses_retry_even_when_no_closed_candle_cached`
and the oracle fixture's second case show a timed-out key is not always
rehydratable. Fetching only a forming candle creates a memory map with zero
cached chunks. The key remains absent from `hydratedCacheKeys`, but three retries
make no backend calls because `queuedHydrate` returns on the map's presence.
If the original load never resolves, this blocks backend hydration until reset.
Both tests also prove reset permits the next backend read. The Rust guard is
unchanged, now has a `ts-bug: CandleCache.queuedHydrate` marker, and its manifest
row is `divergent` with the preserved defect explained. No cancellation policy
or source behavior was silently fixed. Current progress is **41 / 43 ported,
43 distinct items, 2 divergent** (this finding and the typed JSON boundary).

| Mutation | TypeScript negative exit | Rust negative exit | Failed assertion |
| --- | --- | --- | --- |
| Omit timeout slot release/drain | 1 | 101 | Queued second backend read never starts |
| Mark timed-out key hydrated prematurely | 1 | 101 | False hydrated-state check / retry backend read |
| Suppress the old late completion | 1 | 101 | Expected stale gap candle is absent |
| Remove the existing-memory-map retry guard | 1 | 101 | Preserved source bug expects no extra read |

All four mutations were reverted in each language. Oracle baseline and restored
runs both pass **2 tests** (exit 0). Native baseline and restored targeted runs
both pass **3 timeout tests** (exit 0), including the earlier fresh-memory overlap
test. These controls distinguish slot leaks, premature completion, late-write
policy, and the actual retry suppression finding rather than treating all four
as one success-path assertion.

Final restored gates: all-target native suite **15 passed**, fmt check, strict
all-target clippy, wasm build, extraction check (0 changes), invention/status
check, and report all exit 0. No requested gate was skipped.

## PR 7201: interval corruption blocker

All interval-bearing public cache requests, fetcher options, key builders and
lookups now use `safe_cex_types::AnyTimeframe`. The internal calendar table
exhaustively matches its three constituent enums and all **27** TS duration
entries, including `1s`, `5s`, `15s`, `30s`, `8h` and `3d`. There is no unknown
string or NaN sentinel branch; unsupported external strings fail union decoding.
Aliases outside the union (`1W`, `W`, `M`) are also rejected, avoiding the old
unannotated alias correction. Calendar `1M` uses `ExtendedTimeframe::MN1`.

`every_ts_timeframe_has_finite_slots_and_cache_round_trips` checks the TS-pinned
duration, exact finite fetch request, two distinct persisted candles outside
chunk zero, and read-back through a fresh cache for every interval. A compile-fail
doctest verifies a raw `&str` cannot be passed to the public key builder.
Replacing the `8h` width with the old NaN sentinel failed the regression with
exit 101; restoring the table passed. The native suite passed **16 tests** and
the compile-fail doctest passed. Strict all-target clippy also passed.

## PR 7201: discarded hydration tasks

`HydrationSlot` owns completion/counter cleanup **before** the async task is
passed to the host spawner. Its Drop path wakes the waiting fetch, decrements
once, and drains queued work. Normal completion and timeout explicitly finish
and disarm that same guard before any late hydration await. Cleanup no longer
requires a host keep-alive promise or a timer surviving task destruction.

Two regressions cover six immediately discarded unpolled jobs followed by a
healthy job, and a pending backend task discarded while another key is queued.
They assert fallback callers complete, queued work starts, the discarded key
can retry, and the injected clock never advances. Emptying only the guard's
Drop implementation failed both tests with exit 101 (unresolved caller and no
queued task). Restoring it passed both, plus all **18 native tests** and strict
all-target clippy. Existing timeout/late-completion tests still pass.

## PR 7201: malformed stored candles

Typed decoding failure now skips only the offending candle instead of returning
an error from the entire backend load and `CandleCache::fetch`. Valid siblings
in the same chunk and other chunks remain available. The manifest still records
the narrower typed-JSON divergence: Rust skips malformed candles and discards
unknown fields; TypeScript can retain unchecked shapes.

`malformed_stored_candle_keeps_valid_siblings_and_fetches_only_its_gap` drives
the public fetch path in both read-through and hydrate-once modes. A missing
volume field affects the middle of three candles spanning two chunks. Both
modes return all three candles, request only the middle candle's exact range,
and persist the repaired chunk. Restoring the old decoding `?` failed this
regression with exit 101 at `CandleCache::fetch` (missing field `volume`).
Restoring the skip passed all **19 native tests**. An independent read-only
review also passed those tests and the compile-fail doctest.

Final restored gates: package fmt check, strict all-target clippy, the
compile-fail doctest, wasm build, extraction check (0 changes), invention/status
check, and parity report all exit 0. Candle-cache remains **41 / 43 ported,
43 distinct items, 2 divergent**: this typed boundary and the preserved TS
timeout retry-suppression finding. Each of the three PR fixes has a regression
that failed with exit 101 when its defect was reintroduced.

## PR 7201: zero chunk size

Zero chunk size explicitly disables caching; normalized exchange fetches still
return candles. Cached lookup returns no hits and hydration/persistence are
bypassed. Direct key-value zero-width reads return an empty batch. Manifest
notes mark the intentional degradation from TS non-finite slot arithmetic.
Removing the fetch bypass failed the zero-size regression with exit 101 at
cache division; separately removing the backend guard failed with exit 101 at
backend division. Both mutations were restored; all 20 tests and strict clippy
pass. This choice avoids both a panic and silently coercing zero to another size.

## PR 7201: documented boundary and native decisions

Pre-epoch candles retain Euclidean sparse offsets: index -1 maps to chunk -1,
offset 99 for size 100. A public fetch/persist/fresh-cache round trip pins this
intentional divergence from JS negative array properties. Replacing Euclidean
remainder with JS-style signed remainder failed the regression with exit 101;
the mutation was restored. The timestamp-normalization heuristic is unchanged:
this fixture supplies pre-epoch seconds, and checks canonical millisecond storage.

The earlier AnyTimeframe change already removed uppercase `1W` support. Its
source slot-width bug now has a `ts-bug: slotSecondsFor` marker and divergent
manifest note: Rust rejects the alias, rather than returning 604800 or non-finite
slots. Existing interval tests reject `1W`; lowercase `1w` round-trips.
`get_candle_cache_key` now carries the account/environment and calendar
invalidation rationale in Rustdoc and links the actual shared TS symbol; crate
references now point to candle-cache sources instead of unrelated package prose.

The native API remains local. CLAUDE.md records the Redis owner-thread/LocalSet
adapter cost and the complete Send migration (ownership, future/callback bounds,
synchronization, atomic queue reservation and cancellation tests). A compile-fail
doctest pins the current inability to move CandleCache across threads. All 21
unit tests, both compile-fail doctests, strict clippy and manifest checks pass.

## PR 7201: clear-all proof

The key-value clear test now seeds two fresh keys inside the cache namespace
before `clear(None)`, and requires the exact unrelated namespace entry to
survive. A no-op clear-all mutation failed that assertion with exit 101 and was
restored. The zero-size regression also records store get/list counters and
asserts both remain zero, making its no-cache-I/O claim directly executable.

## PR 7201: concrete backend API and final gates

All four operations on both concrete backends are public inherent methods, so
callers need no backend-trait import. The external integration fixture omits
that import deliberately. Making the eight methods private again failed its
compilation with exit 101 (eight private-method errors); restoration passed.
Independent read-only review found no blocker and passed the same suite.

Final state: 21 unit tests, 1 external API integration test, and 2 compile-fail
doctests pass. Package fmt, strict all-target clippy, wasm build, extraction
check (0 changes), invention/status check and report all exit 0. Candle-cache
now reports 37 / 43 ported, 43 distinct Rust items, 6 divergent: existing typed
JSON and timeout retry findings, plus zero-width lookup/fetch, pre-epoch slots,
and rejected uppercase week alias. These notes expose the deliberate choices
instead of presenting them as exact TS parity.

## Merge gate F1: eager drain recursion

A per-cache reentrancy flag suppresses nested drain calls; a scoped Drop reset
clears it on every exit. The public-fetch regression queues 1200 keys behind one
pending hydration with an ordinary eager spawner. Releasing it starts all 1200
queued reads at maximum spawn depth one; another request then starts normally.
Removing the guard reproduced stack overflow/SIGABRT in the child test binary
(cargo exit 101). Restoration passes 22 unit tests plus the API integration test
and strict clippy. This control uses completed tasks, not discarded tasks.

## Merge gate F2: visible calendar implementation

The calendar module and its implemented boundary/classification/normalization
functions are public, with five calendar source rows reconciled as narrowed
seconds-only typed APIs and epoch normalization mapped to its source. Five
unused TS calendar rows remain explicitly pending; no nonexistent helpers are
claimed. Removing chartCandleStartTime's mapping failed parity-check exit 1,
naming the now-visible Rust start implementation. Restoring it passes. The
extractor schema in this checkout accepts a single rs string, so mappings use
one actual public helper per source row rather than unsupported arrays.

## Merge gate F4: non-finite write slots

get_chunk_info returns an optional slot and checks finite timestamp, width and
quotient plus representable integer bounds before casting. Cache writes skip
invalid slots; lookups miss and range chunk enumeration returns empty. In both
backend modes the regression returns NaN and both infinities from a fetcher,
asserts no persisted keys, then proves a valid candle caches and reuses normally.
Removing slot validation failed exit 101 because invalid records were persisted;
the mutation was restored. Fetch results themselves are not sanitized.

## Merge gate F10: exchange namespace guard

The public key builder now takes ExchangeName and uses its canonical serde wire
name, alongside AnyTimeframe and Option<bool>. Forged testnet-prefixed exchange
names fail union decoding; None/false/true still pin historical and testnet key
bytes. The raw-exchange compile-fail fixture uses a valid typed interval, while
the existing raw-interval fixture now uses a valid typed exchange, so neither
can pass because of the other guard. Restoring &str made the exchange fixture
compile successfully and therefore fail the doctest gate with exit 101.
The mutation was restored. The F2 control was also rerun changing only the
chartCandleStartTime mapping; parity-check exited 1 naming only public start.

Final merge-gate checks: 24 unit tests, 1 external API integration test and
3 compile-fail doctests pass. Independent read-only review passed that suite
and found no blocker in the four requested changes. Package fmt, strict
all-target clippy, wasm build, extraction check (0 changes), invention/status
check and report all exit 0. The public calendar subset remains explicitly
narrowed; unused utility behavior is still pending. No merge was performed.

## Historical round 3: actual NaN sort permutations (superseded)

The historical NaN-last correction below is superseded by the canonical V8
sorter cutover in `../parity/CANDLE_CACHE_SPECIFIC_CLAIMS.md`. Current tests
compare exact maintained-source result permutations rather than requiring a
source-incompatible total order. Historical execution receipts remain below.

With authorization to correct the source behavior, clean_candles now uses a
total numeric order with NaN last. First-timestamp deduplication is unchanged,
including signed zero and different NaN payloads. The manifest explicitly marks
the correction divergent from TS's non-transitive subtraction comparator.
The supplied descending 11-element fixture returns numeric timestamps 1..10
then NaN, not two separately sorted numeric runs. Another fixture checks both
infinities and signed-zero retention; 2000 deterministic mixed permutations
compare the actual finite output against a sorted integer-key map retaining
first payloads, and require the first NaN last. Restoring the old comparator
failed both new tests with cargo exit 101; the mutation was restored.

## Round 3: literal interval labels and namespaces

The existing independent 27-entry TS fixture now asserts the emitted label is
its authored literal, and checks 81 public keys for None/false/true using those
literals and independently specified 1w/1M suffixes. This complements duration,
fetch-range, persistence and fresh-cache round-trip assertions. Mutating only
H2's label from 2h to 1h, leaving 7200 seconds intact, failed exit 101 at the
literal label assertion. Restoration passes. This prevents a label-only change
from silently merging different interval namespaces.

Final round-3 scoped checks: 26 unit tests, 1 API integration test, 3 compile-fail
doctests, fmt, strict all-target clippy, wasm build, extraction check (0 changes),
invention/status check and report all pass. Independent read-only review also
passed the package tests and found no blocker in the two changes. Report now
shows 37 / 49 ported, 49 distinct Rust items, 12 divergent, including the declared
NaN ordering correction. No merge performed.

This pass implements the requested blocker and label proof only. The other
newly supplied findings remain open: huge chunk allocation, queued Rc cycle,
28-day month slot regression, dedicated concurrency-cap regression, and
normalization threshold regression. They are not claimed as resolved here.

## Remaining finding 1: bounded chunk allocation

The dense sparse representation is capped at 16384 slots, approximately 1 MiB
per chunk. Oversized cache fetches and direct KV reads return a recoverable
error before allocation or store access; cached lookup misses. This stricter
limit is explicitly divergent from JS array limits. Tests reject usize::MAX,
16385 and u32::MAX, record zero store/fetcher calls, and prove size16384 caches
and reuses. Removing validation failed exit101 on a capacity-overflow panic
for usize::MAX, safely exposing the missing guard without attempting a multi-GB
allocation. Restored validation passes. Zero behavior is retained.

## Remaining finding 2: queued hydration ownership

State now owns data-only HydrationJob entries, never a closure retaining the
whole CandleCache. The start helper supplies cache/backend/runtime ownership
only when a job starts. A Drop-sentinel regression queues at a zero cap (no
legitimate active host task), asserts the caller is pending and backend live,
then cancels the caller and drops the last cache; the backend sentinel fires.
Reintroducing a strong CandleCache field in queued jobs failed exit101 because
the sentinel stayed alive. Restoration passes; active task-drop and eager-drain
regressions remain green. This fixes Rust Rc ownership, not source GC behavior.

## Remaining finding 3: shortest-month slots

The month fixture persists February/March pairs in 1970, 1987, 2010, 2027 and
2050. It requires both canonical timestamps in raw KV records, two distinct
28-day sparse entries, and both candles from a fresh cache with zero exchange
fallback. This includes 2010-02-01 through 2010-03-01. Widening only cache slots
to 30 days failed exit101 because February was overwritten in 1970. Restoring
28 days passes all five year fixtures; candle stepping remains calendar-aware.

## Remaining finding 4: concurrency admission and drain cap

Twenty pending hydrations at cap1 start exactly one backend read. Releasing one
at a time starts exactly one replacement and active/peak backend work stays at
one throughout; all20 callers finish with no clock advancement. Removing the
initial admission check failed exit101 (20 reads instead of one). Independently
removing the drain cap failed exit101 (20 reads instead of two after the first
completion). Both checks were restored and the targeted test passes.

## Remaining finding 5: seconds venue and normalization threshold

A seconds-stamped venue probe supplies seconds request bounds and candle
payloads, pins the exact millisecond fetch range, verifies canonical persisted
candles, and reuses them from both the same cache and a fresh cache without
exchange fallback. Literal threshold checks cover below/equal/above 1e12,
fractional seconds and millisecond flooring. Lowering the threshold to 1e9
failed the venue probe exit101; making comparison inclusive and independently
removing millisecond floor each failed the boundary test exit101. All three
mutations were restored. The existing TS threshold heuristic is unchanged.

All five remaining findings above are now addressed; the earlier round-3
open-findings paragraph records the historical state, not current outstanding
work. Final checks: 32 unit tests, 1 API integration test and 3 compile-fail
doctests pass, as do fmt, strict all-target clippy, wasm build, extraction
check (0 changes), invention/status check and report. Independent read-only
review passed the same suite and found no blocker. Report:36/49 ported,
49 distinct items,13 divergent, including the declared dense-chunk limit.
All targeted negative mutations were reverted; no merge performed.

## Round 4: relative month close and FIFO draining

The calendar regression now pins 1M close from 2010-02-01 16:12:34 UTC to
2010-03-01 16:12:34 UTC with literal epoch-second values. Removing only the
intra-day remainder term failed exit101: actual close was midnight rather than
16:12:34. The existing cap1/20-key regression now records actual backend start
keys and requires cap-0 through cap-19 in order. Changing pop_front to pop_back
failed exit101: keys started cap-0, cap-19 through cap-1. Both mutations were
restored. Independent read-only review checked UTC literals and passed both
 targeted tests; no issues found.

Final checks pass:32 unit tests,1 API integration test,3 compile-fail doctests,
fmt, strict all-target clippy, wasm build, extraction check(0 changes),
invention/status check and report. Both assertions are in one commit, with no
production behavior change and no merge performed.

## Integration with gate hardening and crypto

Merged origin/feat/safe-cex-rust in 4883234894 without conflicts. The parity and
extractor directories match the integration branch exactly. Six owned manifests
needed exact per-file rust_files provenance; six shortened calendar/normalize
helpers needed qualified rename targets in their notes. No mappings, statuses
or candle-cache runtime behavior changed. Extraction canonicalized those arrays.

Extraction check passes with 6,801 rows across 465 TS files, including 542
exported constants, and zero manifest changes. Invention/status gates pass with
239 public Rust items; parity report also succeeds. Removing the normalize
provenance declaration fails exit 1 with the exact undeclared file; removing
its qualified rename note fails exit 1 with the exact required target. Both
mutations were restored and the full gates passed again.

Validation: candle-cache 32 unit tests, 1 API integration test and 3 compile-fail
doctests; crypto 19 unit tests; parity 61 integration tests; extractor 32 tests.
Formatting, strict all-target clippy for the three crates, and the candle-cache
wasm build pass. Independent read-only audit verifies the six declarations
exactly equal their mapped file sets, with no unused provenance or behavior
changes. This integrates the base branch; PR 7201 itself has not been merged.

## Merge-down to 951b0e208b

The sole textual conflict was exempt.toml. A Python tomllib parser checked both
index stages and the resolved file: 66 local entries, 75 integration entries,
92 merged entries (17 local-only), exact union, no duplicate rs values, no blank
reasons, and identical records for shared targets. Integration's new layer gate
also required classifying candle-cache as a service and declaring its direct
shared-types dependency; no validator logic changed.

Validation passes: full workspace locked all-target tests, candle-cache's three
compile-fail doctests, workspace fmt and strict all-target clippy, candle-cache
wasm build, extraction with zero changes (6801 rows), invention/status check
(327 public items), report, 20 layer tests and routing equivalence (1128 cases
across 47 exchanges).

## P3b runtime backends and local host

IndexedDB uses real web-sys requests/transactions and structured records on
wasm. A real Chrome probe verifies persistence, fresh-backend reads, selective
clear, injected-clock age eviction, cancellation handler cleanup and pending
open owner release. A wasm AsyncStorage probe calls actual JS Promise methods,
reopens storage and preserves an unrelated key across prefix clear. Filesystem
round-trip verifies reversible key encoding, namespace isolation and atomic
replacement; a blocking worker owns its staging cleanup guard.

Native Redis tests run against an isolated Redis 8.2.1 process, including all
ignored tests: persist, fresh-backend reuse, SCAN, per-key clear and namespace
clear. Restarting Redis proves both cached read and bulk connections recover.
Removing only bulk connection invalidation fails that restart test exit 101.
The eager 1200-key hydration regression now passes through the AsyncStorage
adapter and still pins max spawn depth 1; removing the inherited drain guard
fails exit 101 with SIGABRT from stack overflow. That control was restored.

The local non-Send core is unchanged. The native owner uses a LocalSet and
command/result channels, aborts pending requests at shutdown and drains tracked
persistence. Redis inherits that same tracked runtime, including detached
Promise.all sibling cleanup. No Arc/Send migration was introduced.

Scoped fmt, native strict all-target clippy, wasm strict clippy and wasm build
pass; tests pass (32 cache unit, 1 existing integration, 3 runtime integration,
3 compile-fail doctests). Browser and AsyncStorage probes pass. Negative
controls: malformed formatting exit 1, unused Rust variable strict clippy exit
101, missing wasm cursor feature exit 101, deleted IndexedDB cursor deletion
exit 1 with the exact persistent-clear expectation, Redis restart exit 101.
All controls are restored.

Three contiguous narrow backend roots reconcile 17 new rows. All are declared
adaptations/divergences, not invented exemptions; the one new exemption is
native CacheRuntime host glue. Removing the IndexedDB options row fails
extraction exit 1 naming its addition; removing that file's rust_files fails
check and report exit 1 naming exact missing provenance. Restored extraction,
invention/status check and report pass (6918 rows, 393 public Rust items).
Extractor tests 33/33, package typecheck and strict lint pass.

P3b lifecycle follow-up: native reads include connection-initialization lock wait
in the 1s deadline (15s bulk), and best-effort policy applies per Redis operation,
retaining good GET siblings. Removing the outer deadline and the per-GET fallback
separately fails their paused-clock and real Redis ACL tests (exit 101). A reply
Drop guard answers native requests even if their task is discarded before polling;
removing its send fails the lifecycle test with 0 replies instead of 20 (exit 101).
All 33 unit tests, 1 API test, 1 owner lifecycle test and 4 runtime integration
tests pass, including the isolated Redis tests.

The real Chrome probe now pins the 5000ms eviction delay, six-day retention and
eight-day eviction. Its fault injection retains a cursor prefix after request
error and an independent successful GET after its sibling errors. An independently
transpiled, unmodified TS IndexedDB implementation sees the same injected events.
Both implementations preserve persist/clear abort-only pending behavior. The TS
oracle also pins callback-exception pending behavior; Rust deliberately reports
that error and returns the prefix, with a ts-bug marker and divergent notes.
Owned Rust read snapshots cannot be mutated by later successes after return.
Four separate mutations (abort handling, prefix loss, age default and eviction
schedule) fail the browser probe exit 1. Restored browser/AsyncStorage/cancellation
and fault/oracle probes pass. Scoped native and wasm strict clippy pass.

Hydration diagnostics: corrected the inherited silent catch, retaining successful
network fallback. `on_hydration_error` receives exact key/error once for initial
and late failures, with native stderr/wasm console default. A capped two-request
injected-clock test verifies both paths, queue progress and successful candles;
deleting each report call separately fails exit 101 with the missing message.
The actual TS implementation runs independently under Bun and confirms silence
for both errors while releasing slots and returning fallback candles. Adding a
warning to its exact hydration catch fails that oracle exit 1. An earlier control
hit the unrelated reset catch and survived; it was replaced with the precise
hydration catch control. All TS/Rust controls were restored. Final native suite:
34 unit, 1 API, 1 owner lifecycle, 4 runtime tests (ignored Redis included), and
3 compile-fail doctests pass; native strict clippy and fmt pass.

P3b service exit criterion: actual Bun HTTP service and unmodified TS cache are
the oracle; the only replaced seam is the upstream exchange callback serving an
unmodified public Binance BTCUSDT 1m day (2025-01-15 UTC, 1440 candles). Rust
runs as the real LocalSet owner subprocess. Five overlapping/repeated/full-day
requests per engine run in memory, cold Redis and fresh-process warm Redis:
30 actual HTTP responses are byte-identical. Both warm engines make zero venue
calls (the harness rejects any warm upstream request). Output SHA-256 is
9d8615acf779cb9d5055713ecf6357e1a1612cbc25de29742314905cfdd40d9d.
Changing Rust returned close prices fails replay exit 1 with a byte difference;
dropping actual native Redis SETs fails exit 1 at the forbidden warm upstream
request (HTTP 502). Both controls are restored. The ready/close race is pinned
by the JS lifecycle test; removing the post-ready closing check fails exit 1.
Service typecheck, strict scoped lint, 9 tests and Bun production bundle pass.

Both authorized Dockerfiles package the binary without setting the activation
variable. Ignore-rule parser assertions verify Rust sources and the Bun bundle
map are included, while local Cargo output and dependencies are excluded;
remote allow rules precede output/secret excludes. Docker CLI is unavailable,
so image builds were NOT RUN. No image push, deployment or credential change.
Default Bun cache wiring remains covered by its original lane/health tests.
Final extraction check (zero changes), invention/status check and report pass:
6918 rows / 393 public Rust items. Extractor tests 33/33, typecheck and strict
lint pass. Final wasm build/strict clippy and all four browser probes pass,
including the independent TS oracle. No PR was created.

P3b oracle extension: the original single-interval replay met the exit criterion,
but did not cover a forming tail or HTTP error bodies. The extended corpus uses
unmodified public Binance CSVs: the complete 86,400-record spot 1s day (gzip
container only), the original 1,440-record futures 1m day, a three-record futures
8h day, and genuine February/March 2023 futures 1mo archives decoded as 1M.
2025 monthly files were unavailable; the earlier two-month window crosses a
real 28-day boundary without approximating a month from fixed-width candles.
Sources and CSV hashes are in fixtures/README.md. The seconds day is paged
through the service's existing 5000-candle limit. Monthly persistence explicitly
requires the service's calendar `1M-cal` namespace.

42 scenarios per engine/mode, 252 actual HTTP responses, pass comparison of
status and exact response body text (not parsed/re-serialized bodies). Final
corpus SHA256 fd9c9454027b3185f4897d06d43b22c6c96987dc7780b74940303a11231c6ac7.
Warm archive queries make zero upstream calls. Two forming-tail reads intentionally
both call upstream, pinning no write before closing; the three retention/error
upstream probes also intentionally call upstream. A modeled gap outside the
recorded retention window returns 200/empty; unknown symbol 404, thrown venue
error 502 and invalid interval 400 all enter the same byte comparison.

Limits: zero chunk_size is only the crate regression
zero_chunk_size_degrades_to_exchange_without_cache_io: the HTTP service has no
chunk-size option, and no query parameter was invented. The service uses
read-through, so the forming-tail replay does not itself exercise the inherited
HydrateOnce empty-memory-map retry bug; the existing unit/TS oracle pins that
separate lifecycle. The service supplies its existing TS-generated cache key to
the native owner; Rust key generation remains independently pinned by the
27-label/81-key regressions and earlier 3807-key matrix.

Four negative controls each fail replay exit 1 for the intended assertion:
1s width changed to 2s (missing expected candles); forming-write exclusion removed
(forming retry hit cache); calendar suffix changed (required namespace absent);
upstream error identity discarded (502 became 503). All restored; final extended
replay, service typecheck/lint, extraction (zero changes), check/report pass.
Read-only agent review found no blockers; its parsed-body caveat was eliminated
by capturing exact body text, and fixture provenance/cleanup documentation was
updated. Docker build/deploy status is unchanged: image builds not run.

Final child-environment isolation sets disabled alert/telemetry and memory Redis
settings to explicit empty strings, preventing Bun dotenv auto-loading from
reintroducing them. This exposed an introduced owner mismatch: REDIS_URL=""
was memory-only in Bun but attempted Redis URL parsing in Rust. The process
regression empty_redis_url_starts_memory_owner_like_bun failed exit 101 before
the owner guard and passes after it, alongside the original shutdown test.
The native owner now accepts unset or empty URL as memory-only, retaining Redis
for nonempty URLs. Native strict clippy passes; full replay is green with this
stronger isolation. No service configuration parameter was added.

## P3b review repair (2026-10-12)

The shared-Redis key-value path has two chosen typed-boundary corrections now
marked `ts-bug` on `load_chunks` and declared on its manifest row. The maintained
source oracle `scripts/kv-null-oracle.mjs` proves null elements reject the WHOLE
TS load while a null timestamp is retained at index zero. Rust's named
`null_stored_elements_keep_siblings_and_null_timestamps_are_rejected` proves
healthy siblings survive and both malformed entries are rejected. Independently
inserting a source null-element skip and a null-timestamp skip each fails the
oracle at exit 1 (missing TypeError rejection / expected two chunks).

The Chrome wasm probe injects an actual createObjectStore exception. Rust must
abort versionchange, reject opening, double-report and latch unavailable; a
second operation must not retry/report again. Replacing the repair with ignored
createObjectStore failure compiles and fails the browser probe at exit 1:
`upgrade must reject open and double-report`. An initial attempted control with
an early return was rejected by compile-time unreachable-code checks and was
not used as behavioral evidence. The corrected mutation was restored, wasm
rebuilt, and the complete Chrome probe passed.

The same probe now injects a missing-candles GET result: Rust settles, reports
once and retains its healthy sibling; maintained TS stays pending without a
report. Null indexedDB independently double-reports/latches in TS. A throwing
accessor instead escapes its pre-try typeof guard, rejecting with no report or
latch. Rust's chosen availability guard returns empty for both and permits a
later restored factory. These are distinct cases, not one claimed behavior.

All eleven IndexedDB notes now identify their own operation. Private `records`
cursor mode, prefix deletion and transaction abort-watch policies are named on
loadAllChunks, clear and evictOldChunks. The pre-epoch persistence choice is
also declared on persistChunks, where it materializes.

Accepted-set follow-on: the public CandleCacheChunkRecord decoder accepted
positional arrays. It now requires an object with a private remote companion;
qualified-call regression rejects [[],0] and accepts its object counterpart.
The private native owner request boundary has its own object-only process
regression: this is service hardening, not a TypeScript parity divergence.

Evidence limit: server.ts runs cleanCandles on BOTH engines, masking raw order
and duplicate-emission defects in the 252-response HTTP comparison. The reviewer
separately reported 83 exact raw owner-protocol sequence checks against archived
CSV, including the full seconds day, minute/eight-hour day and February/March
monthly pair, with zero warm upstream calls. Those are reviewer evidence, not
properties established by the HTTP replay. Docker release/linux-amd64 remain
unverified; the unconditional Rust build stage can block an inactive-engine
service deployment. SDK-only cargo check establishes only isolated build context.

CI dependency follow-up: both Node scripts now load TypeScript through the
existing safe-cex-store isolated loader instead of directly resolving root
node_modules. With SAFE_CEX_TYPESCRIPT_PATH set to a TypeScript-only temporary
directory, both the null-record oracle and complete Chrome wasm probe pass.
The Chrome harness itself still requires Playwright/Chrome as declared; this
change does not claim a new browser CI job or Docker verification.

The Rust null-record oracle test now runs two child Node controls with in-memory
maintained-source mutations (null-element and null-timestamp skips). It requires
exit 1 AND the intended AssertionError text; missing compiler/module or a silently
unexecuted oracle cannot satisfy the control. Baseline and both controls pass
with the isolated compiler path; source files are never mutated by this test.
This Node-dependent test remains explicitly ignored in default Cargo invocation;
run --include-ignored for the cache oracle suite. No browser/Redis CI execution
is claimed by this harness-only follow-up.

P4 candle utilities add the nine previously pending rows, complementing the
existing narrowed cache helpers. Current-source fixtures cover all27 interval
labels, invalid/case-sensitive aliases, seconds/ms threshold edges, exact
boundary ends, Date TimeClip and year0..99 remapping, closure cache transitions,
range guard priority and inclusive window advancement. Numeric results compare
exact bits including -0 and NaN equivalence; emitted JSON window fields compare
numeric bits. All7 independent negative/restored controls pass in
`/tmp/p4-calendar-<gate>-<phase>.log`: formatting, dead code, wrong boundary-end
addition (live test failure), wasm compile error, deleted end-time row and
missing mapped targets. Final tests/clippy/wasm repeat covers both candle and
numeric after exposing the shared JS-number formatter; fmt and parity pass.

Calendar isolated review at f7dad7f5f3 found inherited Object.prototype interval
keys pass TS's width truthiness check before coercion produces NaN. Preserve
that source bug with marker and row note. The live corpus now includes every
Object.prototype own property name. Treating these as unsupported instead
fails specifically current_source_calendar_utilities; restored passes
(`/tmp/p4-calendar-prototype-{negative,restored}.log`).


P3b repair merge-down retains both parent CI step multiplicities (51/63 -> 63),
parsed exemption union 111/244 -> 245 with both path sets, no duplicates and no
blank reasons. Extractor conflicts retain the three grouped runtime roots plus
the integration JSON parser root and both test additions; all 37 extractor tests
pass. Layer audit 78/78 passes; scoped native and wasm all-target checks pass.
All 50 native tests including ignored source oracles and Redis runtime checks
pass with the isolated Redis process. Strict native clippy passes.
LoadedChunks signatures reach Redis, filesystem, AsyncStorage and IndexedDB;
cache hydration/read-through inspect usable chunks and report refused keys through
the existing mandatory-default diagnostic. Suppressing refusal diagnostics fails
overflow_refusal_does_not_prevent_valid_sibling_hydration at the explicit named
observability assertion (exit 101); restored positive passes.
The null-element/null-timestamp TS bug declarations now have valid symbol-named
ts-bug: markers. The two Rust/source tests and source mutation children pass.
Every owned cache/backend/narrowed-calendar divergent note has an explicit bucket
and choices name the available alternative rather than asserting FORCED.
Fresh wasm build succeeds, but the browser rerun was blocked before any probe
execution: installed Chrome exited SIGKILL during launch. This is not a fresh
IndexedDB pass; the earlier checked-in upgrade rejection control remains evidence.

## Key-value derived-slot guard

`invalid_backend_slot_arithmetic_never_becomes_saturated_cache_hit` rejects NaN,
positive/negative zero and finite-overflow widths before integer casting. Valid
minute and infinite-width (derived slot zero) controls still load their candle.
The maintained TypeScript null-record oracle additionally executes these width
cases against the original backend with injected GET records. Both pass.
Disabling the Rust index guard compiles and fails that named regression with
`invalid derived index must never report a cache hit: width NaN, key 0` (exit 101).
The guard was restored and the regression passed. Scoped host and wasm all-target
clippy pass independently under strict workspace lints. Finite out-of-i64 slots
are an explicitly chosen bounded-storage refusal, not a claim of JS equivalence.

## IndexedDB LoadedChunks execution attempt

The browser probe now seeds two usable records (including a parseInt suffix),
a finite out-of-i64 refused key, nonnumeric/nonfinite suffixes and another
namespace. The compiled wasm probe asserts both usable siblings and the exact
refused key. It builds and passes scoped wasm all-target clippy.
Execution remains **unverified**: installed Chromium hangs, Canary exits SIGKILL,
and a freshly downloaded Playwright headless shell fails before opening a page
with `bootstrap_check_in ... Permission denied (1100)` at Mach port rendezvous.
This is not a successful browser run or a closed LoadedChunks coverage gap.
The harness uses Playwright's bundled browser by default; CANDLE_CACHE_BROWSER
can select an installed executable. No browser security setting was changed.

## Cursor write rollback repair — execution remains OPEN

records() previously caught a cursor callback failure and returned before the write transaction completion, leaving already queued deletes free to commit. cursor_failure now aborts a write transaction from both request-error and callback-error paths while the callback is active, before sending the recovered error. Readonly failure continues to preserve its prefix; abort failure is retained in the diagnostic. clear and evictOldChunks own the manifest declarations.

The browser probe now has a separate cursor_rollback_probe: two stored chunks, failure on the second real IDBCursor.continue after deletion begins, then exact original-chunk equality and one report. Removing the abort should allow partial deletion and fail that assertion, but this mutation has NOT been executed. The probe is prepared, compiled and wasm-linted only. No browser launch was retried. LoadedChunks and this new rollback execution coverage remain OPEN, not inferred from the earlier Chrome run and not closed by wasm clippy.

## Malformed persisted Candle residual (superseded numeric-null policy)

The later NaN codec decision below supersedes this section for null OHLC/volume.
Its historical 48-case result is not the current structural-case count.

The delegated shared-type decision keeps the six numeric Candle fields strict.
`malformed_cached_candle_is_a_miss_and_refetches_without_error` exercises the real
KV backend through `CandleCache::fetch`: all six required fields are independently
missing, null, strings or booleans, in both ReadThrough and HydrateOnce (48 cases).
It requires a successful exact-slot fetch, no hydration error, persistence of the
replacement and a subsequent cache hit without another fetch. The existing
`malformed_stored_candle_keeps_valid_siblings_and_fetches_only_its_gap` separately
covers healthy siblings and partial-gap fetching.

`scripts/retained-candle-source.cjs` executes the maintained KV and IndexedDB
methods. TypeScript retains malformed objects and native clone values; the Rust
typed refusal remains an accepted divergence, not a lossless cache claim. The
IndexedDB source witness injects transactions and uses native structuredClone;
it does not execute a browser database. No null-to-NaN substitution is made.

`scripts/malformed-cache-control.py` changes only the real KV Candle-decoder
failure branch to return an error. It requires compilation and a named request
failure, restores exact source bytes, then requires all 48 cases to pass.


## Persistent NaN codec: real backends, compiled RED/GREEN

The final codec decision keeps Candle typed f64, keeps KV's existing JSON-null
write representation, restores null OHLC/volume to NaN on read, and preserves
native numeric fields through IndexedDB structured clone. Timestamp null,
missing required fields and wrong kinds remain structural misses.

Test-only commit `1737642256` ran against the unchanged production bodies.
`filesystem_nan_candles_reload_as_cache_hits_without_a_venue_fetch` compiled and
failed at `NAN-ROUNDTRIP open`: one venue request instead of zero. The existing
reopen test and new structural-miss control passed. Logs:
`/tmp/port-txflow-candle-kv-red.log` and `...-kv-green.log`.
The compiled Wasm probe also failed against actual Chrome IndexedDB at
`NAN-CACHE-IDB nan/volume/mode0`: one venue request instead of zero; see
`/tmp/port-txflow-candle-idb-red.log` and `...-idb-green.log`.

With the codec fix, actual filesystem/KV persistence and reopen pass ten profiles
(five fields, both read modes), independently asserting stored JSON null, native
field values, direct backend slots and zero venue fetches. Structural controls
still refetch exactly once. Actual browser IndexedDB passes 26 profiles: five
native NaN fields, five preexisting null fields and three structural misses,
each under both read modes. The browser runner independently executes the
maintained TS KV and IndexedDB writes, showing JSON null and native NaN
respectively. It uses real browser transactions, not a replacement backend.

Reproduce from the repository root (run cargo commands sequentially):

```sh
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" cargo +1.99.0 test --manifest-path sdk-rs/Cargo.toml -p safe-cex-candle-cache --jobs 1 --test runtime_backends filesystem_
cargo +1.99.0 build --manifest-path sdk-rs/Cargo.toml -p safe-cex-candle-cache --jobs 1 --target wasm32-unknown-unknown --example indexed_db_probe
wasm-bindgen sdk-rs/target/wasm32-unknown-unknown/debug/examples/indexed_db_probe.wasm --target web --out-dir sdk-rs/target/nan-codec-probe-green
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" CANDLE_CACHE_PLAYWRIGHT="$PWD/sdk-rs/target/candle-cache-probe-deps/node_modules/playwright/index.mjs" CANDLE_CACHE_BROWSER='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' node sdk-rs/safe-cex-candle-cache/scripts/nan-browser-probe.mjs sdk-rs/target/nan-codec-probe-green
python3 sdk-rs/safe-cex-candle-cache/scripts/malformed-cache-control.py
```

Playwright is a task-local ignored dependency; select an installed browser with
CANDLE_CACHE_BROWSER. The structural control compiled with only decoder refusal
changed to an error, failed the named CACHE-MALFORMED assertion (exit 101), then
restored exact source bytes and passed all 38 current structural cases. This
positive browser result does not close the older rollback/fault probe coverage.

Readiness on the merged master base: scoped native all-target tests pass 60,
with six explicit external-service/source-oracle ignores; the library source
oracle was separately enabled and all 42 library tests pass. Native and wasm
all-target Clippy both pass with -D warnings, and changed Rust files pass fmt.
Full-resolution scoped parity passes 7,459 source rows / 80 public cache items
with the existing 19 scoped exemptions; extractor check changes zero manifests.
Required safe-cex-exchange all-features cancellable_future_inventory passes six,
with the explicit discovery export ignored; no producer pin refresh is needed.
Logs use `/tmp/port-txflow-candle-{all-targets,lib-all,clippy-native,clippy-wasm,parity,extract,cancellation}.log`.
