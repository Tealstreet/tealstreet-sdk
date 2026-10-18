# Derived mirror parity

Risk and returns belong in `safe-cex-store` because the risk engine consumes
`Market`, `Ticker`, `Order`, and `Position` store shapes. No new crate or duplicate
domain shapes are introduced.

The behavioral authority is [safe-cex/CLAUDE.md](../../packages/safe-cex/CLAUDE.md),
especially “Risk, PnL-at-price, ROE and break-even — where each one actually lives”.
The [measured position evidence](../../docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md)
constrains what venue fields can honestly mean; a conserved aggregate PnL does
not establish side, quantity, round-trip count, fee convention, or completeness.

| Calculation | P1d scope and trust boundary |
| --- | --- |
| Risk / PnL at price | Port `utils/risk.ts` faithfully: profit/loss order walks, fees, secondary positions, and crosshair pivots. This source has no reciprocal inverse branch. Its linear arithmetic does not establish inverse correctness. |
| ROE / ROI | Port both distinct functions faithfully. `calculateReturns` uses venue unrealized PnL over entry times contracts (Open table); its zero override truthiness defect must remain. `calculateRelativeReturns` uses the entry-to-exit move (Closed and Recent tables, inverted by CLI `$roe<N>`). They agree only when venue PnL equals the same linear price move times size. |
| Native sizing | Owned by P1b, outside this chunk. The shared native sizing definition handles inverse using fixed quote notional; this sizing rule does not establish any inverse price/PnL rule. |
| Break-even | Outside P1d and already untrustworthy: `processTrades` supplies gross traded volume as the divisor instead of remaining size, lacks coverage checks, and has hedge/flip/dedupe defects. Order paths use entry price; no repair or replacement formula belongs here. |

Inverse price/PnL remains **UNSOLVED / REFUSED** for financial interpretation.
Every inverse mapper derives base contracts from the mark, so entry times
contracts equals notional times entry divided by mark and drifts with price.
Bybitinverse and bitgetinverse additionally leave unrealized PnL in base currency.
The raw risk and return mirrors serve conformance; their numeric outputs are not evidence that
inverse ROE or PnL is solved and must not be used to remove a consumer refusal.

The known reciprocal defects in the Open table worst-case-loss column and
`createStopLossOrder` turn a $100,000 BTCUSD position stopped 10% away into about
$0.11 rather than $10,000. Those symbols are outside P1d; `utils/risk.ts` carries
no such branch, so adding one would invent behavior rather than port it.
Any later port of those symbols must reproduce and annotate the defect before
a separate deliberate fix. P1d preserves defects actually present in its source.

Instrument classification is metadata-only. Unknown is distinct from crypto;
unrecognized Bybit values remain visible in the reason. The shared description
and preflight copy say “may require” an agreement and never certify eligibility.
Canonical architecture and venue evidence remain in the linked docs above.

The classifier retains inherited member identity through the shared ProtoMember
tag carried by AssetClass::PrototypeProperty. This is the existing integration
choice, with producer witnesses and costs recorded in
[the source manifest](../parity/manifest/instrument-class/bybit.instrumentClass.ts.toml)
and [its audit](../parity/evidence/orphan-bitget-findings/README.md).
It does not carry an executable function or mutable prototype referent.

## Clock runtime bridge and the preserved timer defect

`VirtualClock` holds the offset and clock lifecycle in `safe-cex-store`.
`<&'static VirtualClock>::default()` accesses the one module singleton; ordinary
`VirtualClock::default()` makes an isolated host-configurable instance. Cloned
handles share state. `get_current_time_ms` is host epoch milliseconds plus the
cached offset; `get_current_time` returns Date-compatible clipped milliseconds
instead of allocating a Dayjs object. The native default supplies system epoch
milliseconds. Wasm hosts must inject `now` before use because `SystemTime` is not
a browser clock.

HTTP and timers are explicit host hooks. The venue hook must resolve the Bybit
public time URL through the transport's public CORS proxy, enforce the TypeScript
5000ms timeout, parse the venue's date shape, and return epoch milliseconds.
This core does not implement native HTTP, proxy selection, or a timer executor.
A fallible async source represents a rejected JS promise as `Err`; panicking
host callbacks are outside that bridge contract. The default scheduler is inert;
a host must supply timer scheduling/cancellation to obtain automatic polling.
Generic sync prefers the registered source, then venue time, then local time;
Bybit sync reverses the first two. Native mode mirrors Node/Bun clearing, while
worker mode preserves pushed corrections on ordinary sync. Listener identity,
live notification iteration, and shared in-flight sync are retained.

**Preserved defect: `VirtualClock.scheduleNextSync`.** It schedules after
`TIME_SYNC_INTERVAL_MS = 300000`, but its callback treats elapsed time greater
than `90000` as sleep. A normally punctual five-minute callback therefore crosses
the threshold by 210000ms. A delayed callback after sleep crosses it too. Before
awaiting `syncClientServerTime`, it clears `server`, `client`, and `offsetMs`.
`getCurrentTimeMs` consequently returns the local clock until a correction is
installed. The Rust timer path reproduces this, marked `// ts-bug:` and with a
divergent manifest note. Its deterministic worker regression advances exactly
300000ms and observes the pushed offset vanish without any actual sleep.

This is reachable in a **browser tab** after `start`: successful sync reinstalls
the offset, but concurrent timestamp readers during the awaited request see the
local clock. If both time sources fail, the fallback is local and the correction
remains zero. In a **browser worker** that starts the timer, the callback also
clears its pushed offset, and `syncClientServerTimeInner` returns immediately
because no window exists; it stays cleared until another main-thread push or
explicit venue correction. Reachability requires `start` in that worker; this
port does not claim every worker starts it. In **Node/Bun**, sync intentionally
clears offsets and skips remote fetching, so the timer is reachable but clearing
an already-zero offset adds no browser-style drift failure.

Signed requests use this clock (see the timestamp guidance in
[exchanges/CLAUDE.md](../../packages/safe-cex/src/exchanges/CLAUDE.md)). A browser
whose local clock differs enough from venue time can therefore send timestamps
outside the venue receive window during this interval, potentially observing
signature/timestamp rejection, including after a laptop wakes. That is a
mechanistic risk, **not a measured production attribution**: no live rejection
was captured by this port. A TypeScript fix needs a separate decision and review;
the parity port must retain the defect so its conformance tests can detect it.

Incremental fills convert cumulative order totals into positive frame deltas;
replays never update the baseline or refresh eviction order. The captured cache
preserves insertion order, retaining prior value when the venue omits it. A
restart intentionally reports the entire cumulative quantity once. Rust callers
pass the TypeScript default capacity 500 explicitly. The 90 source-oracle frames
include replay, eviction, zero/negative/NaN capacities, and NaN value propagation.

## Type-independent store extension

Under D1, `safe-cex-types` is the generator source for the declarations in
`packages/safe-cex/src/types.ts`. RequestOutcome and RequestOutcomeCounts are
therefore owned by safe-cex-types, with TS derives: conceptual store ownership
cannot override the generated surface. A free manifest path does not make a
store-crate declaration available to the generator. The store imports these
canonical definitions and owns only their mutation and snapshot behavior.

SocketDiagnosticsSource remains beside summarizeSocketDiagnostics in
safe-cex-store: it is a non-exported local input interface in exchanges/base.ts,
not part of types.ts's generated surface. Exact shape exemptions cover that
interface and the anonymous summary record, with no extra transport or telemetry.

`summarizeSocketDiagnostics` keeps lifetime close counts separate from streak
counts. A recovered socket can have a zero streak and hundreds of lifetime
closes; merging those meanings either hides recovery evidence or makes a
lifetime reconnect cap permanently silence a long-lived session. Latest close
code selection is strict by timestamp, retaining the first socket on ties, and
close metadata survives recovery. Reconnect pause is a boolean, never a
remaining-time countdown that would emit store deltas every watchdog tick.
There is no telemetry client: the store snapshot is the only channel out.

The Rust summary retains a missing code as `None` internally, corresponding to
TypeScript's own property with `undefined`; JSON omits that key. JSON numeric
tokens may differ (`0.0` versus `0`), so fixtures compare number values while
requiring exact object keys, array order, strings and booleans. Counts use the
same plain IEEE-754 addition as the TypeScript source.

The five outcomes are CLOSED. This struct
is structured-cloned across the worker boundary and persisted to IndexedDB;
free-text venue reasons can contain client IPs and signed query strings and
would copy those into both stores. Widening also changes a counter primary key,
RPC signature, route normalizer and rollup group-by.

Recording increments one f64 field in place, without allocation or notification.
The throttled snapshot ship carries the counters. HTTP status alone selects the
outcome: no response means network, an answered request without status means
other; 418/429 rateLimit, 401/403 auth, [200,300) ok. No error-pattern regex or
telemetry client runs here. Signing-capture synthetic responses are excluded.
Rust's helper accepts the host-evaluated capture flag and a typed optional store;
it cannot encounter the source's missing-method/throw cases. Axios interceptor
installation and config predicate integration remain pending, explicitly noted
on the divergent helper row.

DefaultStore is a permanent partial implementation, currently requestStats,
socketHealth and orders. serialize/reset/class/constructor rows disclose that coverage;
this is not a full StoreData snapshot or warm-boot replacement. requestStats is
included in supported snapshots and omitted when absent, never converted to
null. Counts do not reset on reconnect or socket updates. Source reset() DOES
reset counts: monotonicity holds between explicit resets/restarts, and consumers
must treat a decrease as a new baseline rather than a negative delta. Listener
identity deduplicates and notification walks the live registration order without
holding a lock during callbacks; recording never enters that path.


Terminal order memory is opt-in per venue: only an explicit final venue event
calls markOrdersTerminal. A mapped Closed status or ordinary removal never
infers memory; "triggered" is never final, nor is "expired" on a venue that
expires stops as they fire. The Rust method accepts existing OrderStatus and
rejects values outside Closed/Canceled to represent the narrowed TS input type.

Remembered IDs protect REST/add/upsert/full/symbol replacement for strictly
less than 60,000 ms; a backward clock retains them and NaN expires them, exactly
like the source comparison. Re-marking refreshes insertion order; the oldest ID
is evicted beyond 1,000. Missing current terminal rows cause stale reopenings
to be dropped. Single stale add/upsert never bumps or notifies; a nonempty batch
can still bump while substituting a current terminal row. Symbol replacement
filters incoming rows before removing the old rows. Reset clears terminal memory.
Direct status updates remain unguarded, and worker replacements bypass memory
because their source store already enforced it. Worker IDs are adopted verbatim,
even when lower; equal IDs suppress incoming orders. This update method is an
orders-only projection, explicitly divergent; all other StoreUpdate work remains
pending. The shared virtualClock supplies time by default; the now host hook
supports deterministic clocks without another time model.

// ts-bug: addOrUpdateOrders preserves an additional source defect: its optimized
batch index picks the LAST existing duplicate ID when both lists have more than
one row, whereas the small-batch linear search picks the FIRST. The manifest row
is divergent-style, marking deliberate bug parity rather than an improvement.
Order upserts preserve absent ModelField properties and overwrite own undefined
properties, across all 15 carried fields. Rust owns order values and snapshot
JSON copies rather than exposing mutable shared TS object references; valid
mutation results match the live source, but reference aliasing outside the store
is not promised. Snapshot coverage remains partial: updateIds.positions is the
unimplemented default zero, and this cannot replace a full warm-boot snapshot.


The P1a integration merge now supplies the canonical request types and the
literal-derived variants Ok, RateLimit, Auth, Network and Other. The former
pending imports resolve and the store gates pass against the merged types.

## Sorted order-book updates

Both updateOrderBook functions use real OrderBook/OrderBookOrders definitions
from safe-cex-types and live in safe-cex-store because they mutate store shapes.
Bids/asks are represented by a bids boolean; Rust callers pass the TS upsert=true
default explicitly. Binary search maintains descending bids and ascending asks
without a trailing sort. Exact floating comparisons preserve duplicate-price,
NaN and infinity behavior rather than imposing a Rust total ordering.

Existing levels change amount only, retaining their previous total. Object-form
insertions retain the caller's total; primitive insertions set total to zero and
construct a level only for that insertion. Zero and negative-zero amounts delete
existing levels and skip absent ones; upsert=false still permits updates/deletes.
No total calculation is introduced: rendering totals elsewhere intentionally use
plain +, and this port must not make them precise. Rust owns inserted values;
external mutation through a shared TypeScript object alias is not promised.
The 480 frozen source-snapshot sequence frames check return indices and complete snapshots
for both sides, both forms, duplicates, insertion/update/deletion and upsert modes.


## Risk and the two returns

The implemented engine preserves separate profit and loss walks, full-stop and
trailing-stop behavior, maker/taker fee choices, adjusted entry, negative sizes,
secondary positions, primary-over-secondary overwrite, and first-profile
crosshair pivots. Hedge crosshairs return None when more than one side is open.
Missing/zero/NaN last prices skip risk. Fee fallback and Math.min retain TS
truthiness and NaN/signed-zero behavior. Precomputed symbol maps borrow real
source rows, last duplicate wins; Rust's borrow rules require releasing the cache
before mutating those rows. OrdersRiskProfile belongs in safe-cex-types under D1;
its mutable generated surface and optional fields match the oracle.

// ts-bug: calculateReturns preserves zero (-0 and NaN too) override fallback to
the full position UPnL, even when a missing 1m/5m baseline supplied zero. The
manifest is divergent-style deliberately; the implementation does not repair it.
Open positions uses calculateReturns: venue UPnL / (entryPrice * contracts).
Closed and Recent uses calculateRelativeReturns: signed exit-entry move / entry.
CLI $roe<N> inverts the latter. They are distinct even for the same position,
and agree only when venue UPnL equals the corresponding linear move times size.
Neither raw API has venue provenance, so neither can enforce a venue-specific
refusal internally. They are conformance mirrors, not a financial interpretation
API. Inverse remains UNSOLVED / REFUSED at the consumer boundary: contracts are
mark-derived, entry*contracts drifts from quote notional, and some venues leave
UPnL in base currency. No consumer refusal was removed and no reciprocal formula
was invented. The out-of-scope approximately $0.11 inverse defects remain as
recorded above, not a new branch in this source.

// ts-bug: risk's ordinary-object array buckets throw on prototype keys such as
constructor/__proto__/toString. Rust returns catchable RiskError when an
open symbol or processed order ID hits such a key. No panic or silently cleaned
ordinary entry reaches the worker; it can log and clear the entire risk dictionary. The corresponding manifest rows record it.

All risk multiply calls use safe-cex-num's float-faithful implementation. Its
[NUMERIC-PARITY.md](../safe-cex-num/NUMERIC-PARITY.md) documents measured V8 modes,
Math.pow last-bit differences, the split arithmetic gate and unmeasured
JavaScriptCore. This port neither substitutes precise decimal arithmetic nor
promises cross-runtime bit identity beyond that evidence. Display formatting
uses en-US decimal halfExpand over JS shortest number spelling, grouped integer
parts, exact NaN/Infinity strings and source prefix-zero behavior; precision
bounds preserve Intl's range failures.

The live derived gate executes maintained TS in Node and compares 104 risk cases,
832 crosshair prices, 90 finite plus 10 nonfinite return pairs, and 126 formatter
cases. NaN/infinities/-0 are encoded before JSON rather than lost to null.
Generated oracle output stays in memory; no additional corpus dump is committed.
Run: cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-store --test derived
-- --include-ignored --nocapture. Default tests remain hermetic and cover the
load-bearing stop/fee/hedge/truthiness/prototype/locale invariants.


The crosshair entry scans admit NaN prices and the TS subtraction comparator
then returns NaN (treated as zero by Array.sort). Rust's standard sort can panic
on this nontransitive relation. A stable binary-insertion path preserves comparator
semantics without that Rust-only failure. The 64-row live Node reproducer is
covered; ordering of larger nontransitive arrays varies with JS engine sorting
algorithms and is not universally promised. calculateRiskAtPrice is explicitly
CHOSEN divergent: a pinned V8 sorter could mirror larger nontransitive arrays, but this implementation retains the bounded stable insertion profile. Finite numeric sort order remains exact.


The existing Rust TypeScript parity CI job runs all store targets, including
both live gates, explicitly with --locked and its isolated compiler/dayjs via
SAFE_CEX_TYPESCRIPT_PATH. Source-only edits to every oracle trigger the workflow. Local CI-equivalent commands passed;
no hosted CI execution is claimed for this branch's commits.


Round 1 gate correction: the instrument (114), clock (19), fills (90), book
(480), request (40), socket (16), and terminal (32) Rust fixture comparisons
are **frozen snapshot tests**, not live execution. Their original reports
incorrectly called these live. `live_snapshots.rs` now runs every maintained
`.mjs` generator with `--check`: live TS must reproduce the committed snapshot
exactly, and the ordinary Rust tests compare against that same snapshot. Thus
source drift and Rust drift each fail the composed gate. Every corpus pins its
scenario/frame counts. Snapshot generators never rewrite files in this gate.
The derived gate separately compares in-memory live output to Rust, pins all
five corpus lengths plus per-case crosshair counts, and prints iterated counts.

The source loader transpiles relative TS modules recursively, including the
instrument registry and clock runtime helper. Clock uses real dayjs from the
locked isolated dependencies; venue transport is an injected failing host and
makes no network requests. Those 19 frames cover runtime/offset/timer behavior,
not live venue transport. Instrument frames compare nonCrypto and display copy.

JSON is a known binary64 boundary: workspace serde_json enables `float_roundtrip`
for both live and frozen corpora and production consumers. A regression pins
106.60000000000001 to bits 405aa66666666667. Special values remain explicitly
encoded; default JSON still cannot represent NaN, infinity, or negative zero.

The unreachable `positionCountBySymbol` prototype intermediate differs: TS's
plain object can compute Object + 1 into a string whereas Rust counts 1. The
risk bucket access already throws for those inherited keys, so this intermediate
cannot affect a successful result; no source bug was repaired.

Run the composed gates with:
`cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-store --all-targets -- --include-ignored --nocapture`.
CI runs this command with isolated TypeScript/dayjs, and both path filters cover
all maintained oracle sources and their relative runtime dependencies.

P4 completes DefaultStore's collections, worker patches, packed ticker updates,
versions, settings and warm-boot snapshot. `DefaultStore` implements
`Store` directly: `Rc::new(DefaultStore::new())` coerces to `Rc<dyn Store>`.
Interior mutability belongs to DefaultStore; private StoreState holds its algorithms.
Each getter's `Ref` must be dropped before calling a mutator. Contract callbacks
receive an owned snapshot and run after the mutation borrow is released, so
callbacks may read or reenter. Dictionary views hold copies with alias tracking
that mirrors TS references, including duplicate IDs and retained renamed keys.
These copies and callback snapshots have a memory/allocation cost. Legacy
orders-only APIs remain available on the same state for the existing consumers.

The live full-store oracle uses current source and installed lodash, compares
49 transition frames plus the fully populated typed stub, and preserves the
existing key-set/value comparator. It does **not** detect object-key reordering;
array order and numeric bit patterns remain checked. Typed `Ticker` requires
numeric fields: a malformed packed tuple with fewer values than mask bits is
rejected by an assertion instead of introducing JS `undefined` into a ticker.
Serialize intentionally preserves the source omission of userData and zkLinkConfig.

The facade risk recovery oracle runs 48 inherited-key error cases, 61 positive
reachability controls, 14 formatting cases and three own-key order cases against
the actual TypeScript risk helper and OrdersRiskProfilePostprocessor. Its Rust
half uses a real DefaultStore and verifies that explicit clearing notifies
subscribers with an empty dictionary. This is the caller recovery contract;
production worker wiring remains a separate FINDING until that owner integrates it.
Two mutations compile and fail named assertions: silently accepting inherited
buckets loses the catchable error, and restoring a panic aborts the recovery.
A source mutation removing the postprocessor clear also fails the named prior-risk
clearing assertion. Restoring all three returns the oracle to green.

## Dictionary write reference mechanisms

The object-row adapter uses canonical ProtoMember identities rather than a new
prototype-name table. The facade oracle executes actual assignment statements
from KuCoin Spot fetchBalance and actual guard/mutation statements from GRVT
fetchBalance in isolated Node VM contexts: 60 cases, including every inherited
name, ordinary/numeric keys, and own data-property shadows.

Ordinary assignment and shadowing inserts reproduce own entries. KuCoin's missing
`__proto__` whole-object assignment instead creates no own key, replaces only
that dictionary's prototype, and silently loses the currency from serialization.
GRVT's nested mutation bypasses initialization because the inherited value is
truthy; it invokes no setter, retains prototype identity, and writes NaN fields
on Object.prototype, visible through fresh unrelated objects. Callable inherited
referents receive their own fields too. These are distinct reference mechanisms.

Rust's safe operation creates an own local entry under the literal key and
retains its balance. This is a CHOSEN security refusal with a FINDING, not a
Rust impossibility or reference parity: a prototype chain is expressible and
deliberately declined. Returned reports expose both the source mechanism and
the safe outcome; callers must preserve that declaration on their venue rows.
This adapter does not claim to repair Store dynamic-map reads after a source
prototype replacement, or scalar/falsy-entry branches.

## Fallible notification contract

All notifying Store mutation paths propagate StoreMutationError, retaining the
original patch, notification or invalid-operation error. Callbacks share one
bottom-layer fallible signature with PublicDataStore. Opaque failing subscriber
identity is created only by the production fanout. The first callback failure
stops all later callbacks after the state mutation has committed; the Store
does not catch, log, continue or substitute success. The caller may catch and
emit, as the actual BinancePM changePositionMode method does.

The owned source gate executes the real TypeScript DefaultStore constructor and
that actual caller method for eight cases. Native DefaultStore exercises first
failure, skipped second subscriber, unchanged-value no-notification, committed
setting and caller-emitter failure. The native caller is the recovery pattern,
not the still-separately-owned Binance runtime implementation. Compiling controls
that continue fanout and that swallow the error fail the named trace assertion;
restoration passes. The 49-case whole-store live differential, reentrant callback
test, contract tests and all 33 scoped units pass. Host and wasm strict clippy
are measured separately. Core's exchange.rs update-event subscription and its
void Listener/emit boundary remain an explicitly routed integration dependency;
no full integration readiness or hosted CI execution is claimed.

### Risk profile enumeration repair

The actual-source derived corpus now compares each profile key vector before
normalizing object order. Its 104 risk scenarios cover three shapes: full-stop
(including the leading flag), ordinary size-last, and loss-side limit where
positionSize precedes fee. The shared serializer uses Rust-only construction
metadata, omitted from both bindings and output. Private record decoding keeps
map-only shape and existing optional non-null fields; metadata defaults false
on decode. This repairs risk-engine construction, not arbitrary incoming JSON
object order. A static-size-last mutation compiled and failed the named
RISK-PROFILE source branch field order assertion; restoring the branch flag
restored green. Existing values and crosshair bit comparisons remain in the gate.

### Seeded-chain inspection (Bitget, 2026-10-13)

Replayed all 318 actual-source cases in Bitget's ticker-handler-capture.mjs,
which executes both real handler bodies and DefaultStore constructor/update.
In each poisoned-dictionary-write case, a seeded ticker whose symbol is
__proto__ replaces the ticker dictionary's prototype. A later lookup for
symbol inherits the string __proto__ from that ticker; the string has no id.
The handler then calls updateTicker with that string, whose missing id coerces
to the ordinary key undefined, and updates the separate sentinel ticker's last
price to 9. The original poison ticker remains at 0 and both subscribers run.
This is an initial prototype replacement followed by a wrong-record update,
not a mutation of the inherited referent's own numeric fields. It is outside
5a9137e650's stock-prototype classifier domain and remains a FINDING; neither
TableHit stock identities nor dictionary_write repairs arbitrary replaced-chain
reads. No fourth write effect or local prototype replica was introduced.

Replay: node sdk-rs/safe-cex-venue-bitget/scripts/ticker-handler-capture.mjs.
Evidence uses the production Store path; replacing it with Object.fromEntries
would hide the prototype assignment. This inspection is not a native parity
claim or a claim that existing Store dynamic dictionary reads are repaired.

### Canonical bucket audit, four shared rows

updateInArray remains CHOSEN for known-field rejection and unknown-field omission,
not for relocation; Rename/Split provenance follows the bucket. The numeric patch
witness remains separate from this accepted-set difference. addOrUpdateOrders
remains CHOSEN for deliberately preserved last-versus-first duplicate selection:
indexed path only when both surviving incoming and stored order counts exceed
one. The actual-source bucket witness covers all four threshold combinations.
calculateReturns remains CHOSEN for preserved +0/-0/NaN truthiness fallback; the
same witness captures override bits before serialization and reads native roe/roi.
AssetClass remains CHOSEN for a single prototype marker losing member identity,
while preserving labels/copy; bybit's whole-result mapper remains FINDING.
No row moved to ported: four remained divergent with CHOSEN. Parsed three-file
scope stays 77 ported / 9 divergent / 0 pending; net change 0/0/0.
