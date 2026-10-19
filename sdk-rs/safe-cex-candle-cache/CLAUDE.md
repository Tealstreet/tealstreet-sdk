# Candle cache port

The behavioral reference is [CandleCache.ts](../../packages/safe-cex/src/candleCache/CandleCache.ts)
(`getCandleCacheKey` documents account environments and calendar invalidation),
[types.ts](../../packages/safe-cex/src/candleCache/types.ts) and its backend sources.
General package guidance is [safe-cex/CLAUDE.md](../../packages/safe-cex/CLAUDE.md).
Link to these shared sources rather than duplicating their specification.

`CacheRuntime` is host glue: supply an epoch-millisecond clock, cancellable local
sleep futures and a detached local task spawner. Dropping a sleep cancels it;
spawn drives detached tasks independently of the fetch future. A pre-spawn
`HydrationSlot` Drop guard resolves the waiting caller, releases the slot and
drains the queue if a host discards an unpolled or pending task. Explicit timeout
cleanup disarms the same guard, so late completion cannot release twice. The local boxed
backend futures carry no `Send` bound, allowing IndexedDB/JS handles on wasm.
This costs a boxed allocation and dynamic dispatch per operation, requires a
host event-loop adapter, and makes each cache instance local to one executor.
Native adapters supply Tokio timers and detached tasks on a LocalSet; wasm IndexedDB and AsyncStorage use browser handles without Send bounds.

Interval arguments use the existing `AnyTimeframe` union, covering all 27
`timeframeToSeconds` entries including live chart seconds, `8h`, `3d` and calendar
`1M`. Exhaustive matching has no unknown-string/NaN fallback; parse external
strings through the union's serde boundary before calling the cache.
Backend interface methods return owned futures; optional read/clear methods use
`Option<Future>` to preserve method absence and fallback behavior. In-memory
chunks preserve first-timestamp deduplication and hydration/read-through merge
precedence. Persistent null OHLC/volume values decode to NaN at the backend codec; invalid
object-shaped matching candles are skipped individually, so valid cached siblings survive
and fetch fills only the missing gap. Adapter I/O errors still propagate in
read-through mode. Consult the manifest note for unchecked-TS representation
and unknown-field differences.

Run package-local fmt, strict clippy, all-target tests and wasm build, followed
by repository parity extraction/check/report. Gate negative controls are recorded
in `GATE_EVIDENCE.md`.

Timeout bounds the caller's wait, not the backend operation: the load continues
and may later fill memory gaps while preserving existing candles. The preserved
`CandleCache.queuedHydrate` retry guard is marked `ts-bug`: a fallback fetch can
create an empty memory map from only forming candles, preventing future backend
hydration although the key was never marked hydrated. Reset clears this guard.
Both lifecycle paths have clock-driven Rust and TypeScript oracle fixtures and
negative-control evidence in `GATE_EVIDENCE.md`.

A zero `chunk_size` disables cache lookup, hydration and persistence; fetch still
returns exchange candles. Direct key-value zero-width reads return no chunks.
This explicit degradation diverges from TS non-finite chunk/offset arithmetic.

Pre-epoch timestamps retain Euclidean offsets (index -1 becomes chunk -1,
offset 99 for size 100). This deliberately differs from JS negative array
properties, which disappear when persisted; manifests and a round-trip test pin
that choice. Uppercase `1W` is rejected by the finite union; the source alias
bug in `slotSecondsFor` is marked and recorded, without a NaN sentinel.

## Native host decision for P3b

**Decision (Sam): keep the local, non-Send surface on both wasm and native.**
P3b inherits this decision; it should implement host adapters rather than reopen
the core API choice.

The terminal goal of this port is retiring the JavaScript runtime, making the
browser the primary target. Wasm must retain a local surface for JS/IndexedDB
handles. A Send-first native API would require maintaining two API shapes
indefinitely, with the local wasm shape serving the primary target.

The native consumer that wants Send, `safe-cex-server`, is opt-in and barely
used: a device toggle, not the default trader path. See the shared
[root CLAUDE.md](../../CLAUDE.md) exchange-server guidance. Its current use does
not justify the coordinated ownership, synchronization, callback/backend bounds,
queue reservation and concurrency-test migration described below.

`CandleCache`, requests, backends and their futures cannot be moved into
`tokio::spawn` or shared across worker threads. P3b must wrap native/Redis use
with a `LocalSet` or an owner-thread channel adapter. Server workers communicate
with the owner through Send command/result channels where needed. This adds
scheduling/channel latency and requires managing the owner's startup, shutdown
and pending-operation lifecycle. An Arc wrapper alone cannot make the cache
Send. Each operation already boxes a future.

**Reopen only if a second multi-threaded native consumer that is not opt-in
appears.** That changes the benefit of a Send surface enough to reconsider the
migration and the ongoing cost of maintaining two shapes.

Making the native API Send is a coordinated API/state migration, not a future
alias edit: native futures need `Send`; Rc options, state, backends, stores,
exchange sources, fetchers and runtime callbacks need Arc plus Send/Sync bounds;
RefCell state, completion signals and memory backend maps need synchronization;
queued closures need Send. Backend and callback implementers must then satisfy
those bounds. Wasm still needs a local version for JS/IndexedDB handles, via
platform-specific aliases/bounds or separate local and native surfaces.

Thread safety also changes queue correctness: reserve a slot atomically with
queue removal, release locks before waking callers, draining/spawning jobs,
calling hooks or dropping futures, and keep Drop cleanup idempotent. Holding a
mutex while drain invokes a job would deadlock. Native multi-thread cancellation,
cap and reentrancy tests plus compile-time Send assertions are required. No
runtime dependency is necessary for the core change. This is the cost of a
future reconsideration, not work assigned to P3b under the current decision.

Hydration draining has a per-cache reentrancy guard: synchronous eager completion
or task Drop may request another drain, but the active outer loop handles it.
A scoped Drop reset clears the guard on every exit. The eager-spawner regression
queues 1200 keys behind one pending hydration and pins maximum spawn depth to one.

`calendar` is public and its boundary/classification helpers are mapped to
`utils/calendarCandles.ts`; normalized-epoch behavior has its own source mapping.
The cache adaptation takes typed intervals and seconds, so those narrowed utility
APIs are explicitly divergent. P4 utility APIs now also expose CalendarPeriod, chart-seconds classification,
millisecond calendar starts and a cached live-start closure. These match the
source utility units and Date behavior, including TimeClip and Date.UTC years
0..99; they complement rather than replace the narrowed cache APIs. Epoch
candle start/end/count utilities accept strings to preserve unsupported-string
NaN/error behavior, and split_time_range emits inclusive JSON windows. Removing a boundary mapping
now fails the invention gate naming the public implementation.

Slot lookup returns `None` for non-finite inputs/indices, nonpositive widths,
zero-size chunks or indices outside i64. Writes skip those candles before any
float-to-int cast, lookups miss and invalid ranges produce no chunk keys.
Fetcher results still return to the caller; this guard protects cache storage.

Cache key exchange arguments use `safe_cex_types::ExchangeName`, preserving the
TS wire name through serde. External names must decode through that union;
`binance-testnet` cannot masquerade as a live exchange. Testnet remains a
separate optional account flag. Compile-fail tests independently pin both the
exchange and interval guards; canonical live/testnet byte strings remain tested.

`clean_candles` retains first-timestamp deduplication and delegates the source
subtraction comparator to safe-cex-num's canonical V8 TimSort. The former
NaN-last correction is removed. Actual maintained source profiles compare exact
result permutations, including NaN, infinities, signed zero and first duplicates,
through the public helper and constructed cache fetch path.

The 27-interval fixture pins labels as well as durations: each decoded union
must emit its authored literal name, and public keys must preserve exact live
and testnet namespaces for None/false/true, including the two calendar suffixes.
Relabelling 2h as 1h while keeping its width now fails the regression.

Dense sparse chunks are limited to 16,384 slots (roughly 1 MiB per chunk), so
public sizes cannot request multi-GB vectors or overflow integer widths. Larger
sizes produce a recoverable fetch/KV-read error before allocation or backend I/O;
cached lookup misses. Construction remains infallible; the error is surfaced by
the existing async Result APIs. Zero keeps its documented uncached behavior.
This practical limit is deliberately stricter than JS array-length limits and
is declared in the manifests. The maximum accepted size also has a reuse test.

The hydration queue stores data-only jobs (key, interval and completion signal),
not closures owning a CandleCache. A cache owner supplies backend/runtime state
when draining. Cancelling a queued caller and dropping the last cache therefore
releases its backend; no queue-to-cache Rc cycle remains. Active host tasks still
own the cache legitimately until completion or task destruction.

Integration gate contract: each mapped TS file declares its exact implementation
files in `[file].rust_files`; shortened Rust helper names have notes naming the
exact qualified target. Keep these declarations in the extractor's canonical
format. The integrated corpus includes exported value constants (`kind const`)
and, after the 17 P3b runtime-backend rows, contains 6,918 rows. Use the integration branch's hardened parity
and extractor implementation rather than weakening gates for this crate.

P3b runtime adapters mirror [IndexedDB](../../packages/safe-cex-web/CLAUDE.md),
[Redis](../../packages/safe-cex-server/CLAUDE.md) and
[AsyncStorage](../../packages/safe-cex-react-native/CLAUDE.md). IndexedDB caches
its open future through a weak back-reference and detaches all event handlers
on future Drop, including an unpolled future. Reads do not refresh lastAccessed;
opening schedules seven-day eviction after the injected 5-second timer. Errors
are reported and absorbed as in TS; typed records and bounded slots retain the
P3a adaptations. Native filesystem is the declared AsyncStorage alternative,
using reversible hex filenames and staged atomic replacement; a blocking worker owns the staging guard, so cancellation cannot leak staging
files or block the local executor. An already-started write may still complete
after the caller cancels, as with the cache’s detached persistence.

Redis connects lazily, retains separate 1-second read and 15-second bulk
connections, and uses SCAN MATCH/COUNT 500. Lane deadlines include waiting for
connection initialization; concurrent cold reads must not queue repeated 1s
connection waits beyond their own budget. The candle-server owner selects best-effort Redis operations: failed GETs become
individual missing records, preserving successfully loaded siblings; writes,
deletes and scans report errors and degrade. Other native consumers can retain
error propagation. This matches the existing injected service client policy.
The JSON-lines owner bridge adds process/channel scheduling and serialization
cost. Requests remain concurrent on one LocalSet; shutdown aborts pending
requests, releases callback channels, drains detached persistence, then closes
stdout. Native cache futures remain local, as ruled above.

IndexedDB preserves TS's open-error double report and abort-only pending persist/clear
transactions (`ts-bug` markers); eviction handles abort. Reads finish on requests,
not transaction completion, and retain successful cursor prefixes and independent
GET successes before a failure. Rust returns an owned snapshot, so later successes
cannot mutate an already-returned map as JS can. Cursor callback exceptions become
reported errors instead of the TS callback throwing and leaving its Promise pending.
These are declared boundary adaptations, not a claim of identical failure semantics.

Background hydration failure is diagnostic, not a failed candle request: an
opportunistic cache read should not turn a successful network fallback into an
outage. `on_hydration_error(key, error)` observes both initial and post-timeout
rejections exactly once; its default emits stderr/native or console.error/wasm.
TS silently catches these errors; that corrected bug is marked and its silence
is pinned by `scripts/hydration-error-oracle.mjs`. The hook runs outside state
borrows and the initial callback retains the slot Drop guard. An `Err` backend
result contains no partial chunks to merge; backend per-record fallback must
return successful siblings (as the Redis service policy and IndexedDB do).
Read-through mode still propagates a backend `Err` to the caller.

P3b review limits and corrections: key-value reads deliberately skip null candle
ELEMENTS (TS rejects the entire load) and reject null timestamps (TS coerces them
to zero while returning null). These are chosen typed-contract corrections,
marked on load_chunks with source and Rust regressions; generic JSON could
emulate the source but would abandon the typed Candle boundary. IndexedDB's
missing-candles onsuccess exception is corrected to a diagnostic and settled
partial result; the source hangs. Rust null/throwing indexedDB accessors instead
short-circuit without a diagnostic/latch, a declared chosen availability policy.
In TS null enters open and double-reports/latches, but a throwing accessor
escapes the pre-try typeof guard with rejection and no diagnostic or latch.

The 252-response HTTP replay passes BOTH engines through cleanCandles before
serialization, so it cannot detect raw cache ordering or duplicate-emission
drift. The review additionally verified 83 raw owner-protocol sequence checks
against the CSV before cleaning, including the full 86,400-record seconds day
paged in 18 spans, minute/eight-hour archives and February/March monthly candles;
these are complementary evidence, not properties proved by the HTTP comparison.
The SDK-only cargo check verifies isolated build context. Docker release and
linux/amd64 builds remain unverified; the unconditional cargo build stage can
block a candle-server deployment even when the Rust engine is inactive.

Key-value load_all_chunks returns LoadedChunks with usable chunks and refused_keys
on the success path. Wrong-prefix, nonnumeric and nonfinite parseInt results skip;
only finite prefixes outside i64 are refused, retaining the original listed key.
Valid siblings still hydrate. Decimal prefixes and JS trimStart remain faithful;
exact representable i64 keys above 2^53 retain their declared rounding difference.
The cache consumes usable chunks and reports refused keys through the existing diagnostic hook/default; direct backend callers can inspect refusals.
I/O errors still use Result::Err. This does not repair the separate host hydration
error-reporting issue owned by the cache runtime chunk.

CandleCacheChunkRecord uses the shared whole-entry capture before its private map-only
wire decoder and registers both candles and last_accessed exhaustively. It consumes
existing shared Candle/OrderSide eligibility; see
`../parity/MANIFEST-SPEC.md#numeric-ingress-defect`. The numeric fixture requires
539 native bit comparisons across seven fields, eleven source number spellings
and seven ingress routes. Shape refusals remain separately pinned.

The private owner Request decoder requires an object at its own Deserialize boundary,
using the shared shape guard and private remote schema. This preserves direct decimal
and integer -0 decoding in fromMs, toMs, limit and nowMs, and both optional fields'
missing/null behavior. The parent JSON-lines command check remains in place.
The binary tests assert native fields and reject a complete seven-field array through
raw, owned, borrowed and qualified decoding. Run `scripts/request-object-control.py`
to remove only the guard, require the named array-admission failure, and restore the
exact source before the passing rerun.

Cursor write failures abort the active transaction inside the error callback, before the recovered error wakes records(). This applies to clear and eviction; readonly cursor failures still retain the successfully read prefix. Abort failure is attached to the existing reported diagnostic rather than discarded. The cursor_rollback_probe seeds two real chunks, fails the second continuation after deletes have begun, and requires both original chunks plus one report. Its execution coverage is OPEN: it builds and lints but has never run, because the browser environment remains blocked. This does not change the separately declared cancellation/abort-only completion behavior.

The field-identity supplement asserts all seven decoded chunk/Candle numeric fields directly on raw, owned and borrowed ingress, and six fields on the real KV read consumer. This is separate from old persist/load roundtrip fixtures and service archives: symmetric codecs can cancel in re-encoded bytes, so native-field assertions are required. A compiling post-decode open/close swap fails KV consumer native field identity open. No original shared feeder bytes are regenerated.

The narrowed monthly chart operations now use the same Date/TimeClip semantics
as the millisecond helpers. `tests/monthly_behavior.rs` independently covers all
four operations, including original-year remapping before close-month rollover.

The final decision retains typed f64 Candle. KV keeps its JSON null encoding for
nonfinite OHLC/volume; the read codec restores null to native NaN as a cache hit.
IndexedDB writes/reads native numeric fields directly, preserving NaN through
structured clone and accepting previously persisted null numerics as NaN. Timestamp
null, missing required fields and wrong kinds remain structural misses. The revised
38-case structural miss/refetch regression and its compiled error control retain
that policy; null-numeric slots instead have positive real persistence tests.

`tests/runtime_backends.rs` persists NaN in every OHLC/volume field through actual
filesystem/KV storage and reopens it under both read modes, asserting zero venue
fetches and native field values. The compiled Wasm `nan_codec_probe` runs against
real Chrome IndexedDB with 26 profiles (NaN writes, preexisting null numerics,
structural misses, both read modes). Its separate runner executes the maintained
TS backends too; browser availability for this probe does not imply the older
fault/rollback probes were rerun. See GATE_EVIDENCE.md for commands and RED/GREEN.

The eight-row non-UTF-8 FIX audit is recorded in
`../parity/CANDLE_CACHE_FIX_AUDIT.md`. The real cache/KV source/native replay
covers all 27 intervals under both read modes, and the browser probe includes
cancelled persistence ownership. Pre-epoch Euclidean persistence is accepted
because exchanges have no pre-1970 candles; this is not a general source-bug
correction policy. Extreme key rounding and namespace violations remain explicit
and do not arise from real candle dates or prefix-respecting storage.

The remaining specific-claims cohort is recorded in
`../parity/CANDLE_CACHE_SPECIFIC_CLAIMS.md`. Hydration's accepted extra output is
**diagnostic log only**: report the typed backend cause, preserve caller success,
network results, measured settlement order and slot release.
