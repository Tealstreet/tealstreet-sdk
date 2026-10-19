# Worker FIX-audit ledger

The transferred cohort is the 27 worker rows in `codex-l89ogr.json` from the
de-duplicated gap-census assignment. IDs below are its zero-based row indices,
not a new census. Row 30 was already repaired by merged PR #1158. The other
26 rows transferred to codex-0xekzz without active worker edits.

| Row | Maintained source surface | Resolution | Evidence or next review |
| --- | --- | --- | --- |
| 2 | ChaserOrderManager#tryPlaceOrders | FIX | Canonical eager continuation and await checkpoints; late-stop cancellation survives a dropped waiter. |
| 3 | ChaserOrderManager#emitComplete | FIX | Canonical settlement and retained fill reconciliation after caller abandonment. |
| 4 | chaserOrderManagerInstance | ACCEPT | Pure empty-list allocation; actual module/thread identity and real-engine witness below. |
| 5 | TwapInstance.resume | FIX | Canonical Promise settlement; immediate prefix and rejection profiles below. |
| 6 | TwapInstance.terminate | FIX | Same settlement boundary; synchronous cleanup and partial-commit order preserved. |
| 7 | UpdatePositionsHandler.handleUpdatePositions | FIX | Shared eager boundary; source/native timing profiles below. |
| 8 | UpdatePositionsHandler.handle | FIX | Same operation invoked during dispatch, without an extra spawn. |
| 17 | StoreWorkerPayload | OPEN | Whole worker carrier and report boundary. |
| 18 | WorkerConfig | ACCEPT | Unreachable strict codec removed; actual config/listener preserves the original native values. |
| 19 | splitWorkerLogExtra | OPEN | Retained carrier across callers; no isolated JSON shim. |
| 20 | start | FIX | Native Js ingress/config/dispatch, module-owned listeners and deterministic context teardown below. |
| 21 | ChaserOrderState | OPEN | Native state and output boundary. |
| 22 | defaultChaserState | FIX | Shared shallow-frozen native template; actual constructor snapshots and compiled old-constructor refusal below. |
| 23 | ChaserOrderManager | ACCEPT | Explicit required host composition; actual singleton/engine/Store binding below. |
| 24 | ChaserTerminate | OPEN | Live termination admission. |
| 25 | ChaserCompleteEvent | OPEN | Native event payload. |
| 26 | CallMethodHandler.handleCallMethod | OPEN | Continuation timing repaired below; typed arguments/primitive rejection admission remains. |
| 30 | readStringProperty | ACCEPT | Merged #1158: native Get, getter cause and approved typed String replacement. |
| 71 | ExchangeWorker.start | FIX | Canonical eager startup, source await checkpoints and dropped-waiter continuation below. |
| 72 | PerfFixtureFrame | FIX | Native record/clone/cap/metronome service and actual TS/listener round trip below. |
| 73 | PerfFixture | FIX | Module-owned fixture, dump identity and actual record/replay round trip below. |
| 74 | PerfFixtureConfig | FIX | Native config/modes, interval/loop and full timer teardown below. |
| 75 | ChaserOrderOptions | OPEN | Shared task model and live reads. |
| 76 | TwapOptions | OPEN | Shared task model and live reads. |
| 77 | TwapState | OPEN | Native state/serialization. |
| 78 | CallMethodHandler.handle | OPEN | Entry timing repaired below; malformed/raw envelope admission remains. |
| 79 | OHLCVSubscriptionsHandler.handle | OPEN | Actual listener/subscription timing. |

## Position refresh: rows 7 and 8

`handlers/UpdatePositionsHandler.ts` starts its async arrow immediately: clock,
cooldown, duplicate locks and the exchange fetch run before returning. Its
`await` yields before disposal checks, Store writes and the completion reply,
even for a ready result. Abandoning the caller's Promise does not cancel it.

The native operation uses `safe_cex_exchange::eager::start_eager` once. The
existing canonical Promise await supplies the post-fetch checkpoint; typed
positions remain native across it. The returned future is a waiter, and outer
dispatch discards only that waiter. Runtime teardown still has the existing
request guard to release its own locks and report cancellation. Full and scoped
Store mutators, captured failure-clock cooldown, unfiltered fetch symbols and
post-await disposal suppression retain their existing behavior.

Three named regressions in `tests/store_notification.rs` fail on compiled master
`b973046b6696001f82904d7a8f6403d46bff34f3`: the helper, dispatcher and
ready-result profiles observe zero fetches and zero duplicate-lock replies before polling. The repaired
tests compare 16 maintained-source profiles across direct/dispatch, full/scoped,
held success, held failure, disposal and ready results. They run the actual
handler, canonical scheduler and DefaultStore with only the exchange fetch/time
host supplied. Native leverage bits are checked independently of wire profiles.
A separately compiled control removes the Promise await checkpoint and fails
the ready-result regression; restoring the exact production bytes restores
that fence. The existing 36 source notification cases remain unchanged; Store
commit, subscriber failure and stopped-fanout assertions remain in their native tests.

The typed payload decoder's filtering and UTF-8 input costs remain separately
declared on the manifest row. This timing repair does not close the whole
worker-carrier cohort or claim arbitrary JavaScript input admission.

## TWAP settlement: rows 5 and 6

The maintained `twap/TwapInstance.ts` async arrows (resume at line 249 and
terminate at line 267) have no awaits. State/timer effects already ran before
native method return; the difference was their ready Result futures continuing
an awaiting caller without the source Promise checkpoint. Both now return the
canonical `start_eager` waiter over the already computed result. No body is
re-executed, no bespoke task driver is added, and dropping the waiter cannot undo
synchronous effects. Success, guarded no-op and post rejection all yield once.

Two named `yields_caller_settlement` regressions in `tests/twap_lifecycle.rs`
fail on the compiled master body at `5c3082cee1`: each returns Ready where the
maintained-source oracle requires the caller to return before continuation.
Six source/native profiles run actual TWAP instances and managers with only
the existing terminal clock/timer/post capabilities supplied. They compare
complete prefix events, status, manager membership, retained timer, slice list,
tracked chaser IDs and continuation outcomes. A failed post leaves the state
commit in place and prevents later cleanup/removal exactly as TS does. The
existing 252 interleavings, constructor, clock, price and timeout cases remain.

This closes the two timing findings, not the independent shared options/state
representation review (rows 76/77). The native model still uses typed state
snapshots and explicit timer/post capabilities; no arbitrary JS getter, mutable
array alias or whole browser-worker startup equivalence is claimed here.

## Chaser placement and completion: rows 2 and 3

The maintained manager's async methods (tryPlaceOrders at line 214 and
emitComplete at line 911) start immediately and continue independently of the
caller's Promise. The native prefixes already ran during method entry, but the
returned boxed future owned pending placement/reconciliation and could settle a
ready result in the caller's first poll. Dropping it lost late-order cancellation
or the completion event. Both operations now use the shared start_eager driver;
ready typed leaves cross the canonical await checkpoint before post-await work.
Typed order IDs and fill amounts remain native across that boundary.

Four named regressions in tests/chaser_engine.rs fail against the exact compiled
master body at 0f5dbb345e: two caller-settlement tests, a dropped placement waiter
whose late order must be cancelled after STOP, and a dropped completion waiter
whose pending fill must still reach its event. Eight maintained-source profiles
execute the actual TS manager and instance against native ChaserEngine, manager,
instance, DefaultStore and canonical scheduler; only venue I/O/timers are supplied.
Native fill bits precede finite profile comparisons. The original retry ceilings,
nonfinite sizing, ambiguous placement, cancellation and supervisor cases remain.
An independently compiled removal of the await fences must fail the ready-source
prefix checks even with the eager driver retained; restoring exact bytes recovers.

This closes the assigned continuation/timing findings. It does not classify
arbitrary JS error/receiver admission or the separately open manager, options,
state and event representations (rows 4, 21-25 and 75). The helper-specific Store
snapshot and optional-method capability boundaries keep their own manifest rows.

## CallMethod continuation repair within rows 26 and 78

The actual maintained async arrow invokes the venue member before returning.
Awaited cases post their reply after the source Promise checkpoint; synchronous
cases (including unknown methods and recovery) post during entry. Caller
abandonment leaves the operation running. The native boxed body previously
started only when polled and its reply guard cancelled an abandoned caller.

One canonical start_eager driver now owns dispatch and reply. Awaited typed
leaves cross the canonical await checkpoint on fulfillment and rejection;
removeOrders also yields after its synchronous mutator succeeds. The outer
handler drops only the returned waiter, without spawning another body. Runtime
teardown retains the existing guarded cancellation/report behavior.

The handlers target compares twelve actual-source/native profiles across both
entry points: ready/failed/held leverage, ready/throwing synchronous recovery and
unknown methods. Four named regressions cover immediate entry, caller settlement
and held-operation completion after dropping an unpolled waiter. DATA tests keep
native binary64 fields and own undefined observations, and notification tests
retain committed removal, original subscriber failure and stopped fanout.

This is a bounded repair, not closure of the assigned representation findings.
Rows 26/78 remain OPEN: typed Exchange arguments still reject values TS forwards,
primitive throws are not ExchangeError, and raw dispatcher defaults still differ
from source object spread. The retained carrier review must resolve those costs;
this timing change supplies no authorization for accepting them.

## Manager composition: rows 4 and 23

The maintained manager has no constructor body: its only instance initializer is
an empty array, and its module export constructs that manager once. Allocation
runs no exchange, host callback, timer or Promise. The native accessor delays this
allocation until first access and retains one manager per host thread. Independent
threads model independent worker module realms, rather than a process-global
non-Send Rc. This delay has no caller-visible prefix or continuation to lose.

The explicit host/callback composition category ruling authorizes row 23's
required native bindings. There are no default-success supervision, cancellation
or completion leaves. The existing ChaserEngine implements those bodies; its
ChaserHost supplies terminal venue/runtime capabilities. This classification
covers linking and manager ownership, not arbitrary method replacement, startup
ordering or the independently mapped method behavior.

The real-engine composition test executes the maintained module in two separate
VM realms and native singleton access on separate threads. It compares shared
manager/list identity, independent managers, empty construction without terminal
calls, immediate add registration/logging, stopped-instance cleanup and exactly
one completion. Native size/fill bits are asserted before the finite source
profile. The native path uses ChaserOrderManager, ChaserEngine, BaseExchange,
DefaultStore and the canonical runtime; the supplied host owns only terminal
capabilities. Existing dispatch and engine cases retain reentrant list changes,
listener failures, cancellation, placement and reconciliation floors. A compiled
control rebinds supervision to a fresh manager instead of the registered one;
the named composition assertion fails with an uncleared singleton. Exact
production bytes are restored before the positive replay and required gates.

No worker-carrier or numeric-depth row is closed by this evidence. Rows 21/22,
24/25 and 75 retain their independent state/default/event/options review.

## Startup continuation: row 71 and the timing part of row 20

ExchangeWorker.start awaits the real clock, dynamic loader and startup race.
The native clock prefix already ran during entry, but dropping the returned
future cancelled the remainder; ready leaves also crossed their source awaits
within the caller's poll. Startup now uses canonical start_eager with native
results and shared await checkpoints. The session listener owns its init/cleanup
continuation through the same driver, so dropping its waiter cannot leave a
failed worker in the table. Required runtime access comes from the bootstrap
host; no new executor or per-method task registry is introduced.

Three actual worker/session regressions fail on compiled master 35b178ba1c:
ready-clock loader ordering, abandoned held-clock success/failure, and abandoned
listener completion/failure cleanup. The maintained source oracle runs the real
ExchangeWorker and complete store.webworker module. Native probes use the real
worker, DefaultStore, BaseExchange and canonical runtime with only terminal
venue/import/clock and port capabilities supplied. Seed latency bits are checked
before source profiles. Held loader and venue-start cases also finish after
waiter abandonment. Two independent compiled controls remove the clock await
fence or session eager driver; their named regressions fail, then exact
production bytes are restored before the positive gates. Disposal while the
clock is pending suppresses later construction; existing cancellation, deadline, retained-loser, event wiring,
buffer and forced-snapshot tests remain mandatory.

Row 71's scheduling/ownership finding is repaired. Row 20 stays OPEN for raw
init/config admission and independently declared session lifetime costs; this
batch supplies no JSON/carrier/depth acceptance.

## Native session ingress and ownership: completion of row 20

The installed listener now accepts native Js directly. Config values, perf
configure/dump/replay arguments, buffered messages and opaque handler calls use
that same carrier; there is no JSON admission or fallback adapter. Unknown keys,
array configs, nonfinite values, own undefined and UTF-16 text survive unchanged.
Config uses the canonical in operator and ordinary Get in source order. Registered
callbacks retain the same object, see live list appends, and may change the perf
argument read after the loop. Console receives the original thrown value, not a
name/message reconstruction. Shared options/settings/metadata and Store updates
are decoded only where their existing typed consumers use them. Host identifiers
keep their previously approved typed String boundary. Replay does not decode an
unread exchange constructor, options or Store snapshot.

The module owns its installed sessions, while sessions hold a weak reverse
reference. Dropping the optional start return does not remove a live listener.
The required registration cleanup detaches callbacks during module destruction.
Maps and port state are removed before worker disposal; canonical native base
teardown completes shared exchange cleanup. The required shutdown_context leaf
releases remaining context runtime/replay/timer work, including startup losers.
This implements the coordinator-approved native module drop-equals-dispose
policy and prevents dangling callbacks without creating a module/session cycle.
Teardown errors are reported and do not stop later cleanup stages or contexts.
Source disposal and native teardown share one stage sequence with explicit error
handling: the normal source entry returns the first error; native context cleanup
reports it and continues. Native/actual-source observations preserve the normal
first-handler throw, while module teardown disposes all nine exactly once even
if the first fails. The two reviewed override edges moved with their shared
function bodies; receiver/hook classifications and counts are unchanged.

Compiled baseline 55d99ce37a (fresh master b10cec94d3 plus the now-merged #1203
continuation prerequisite) fails three named real-listener tests: native NaN
admission, array config delivery, and liveness after the start return is dropped.
The actual-source oracle runs maintained ExchangeWorker and store.webworker.
Native probes run WorkerModule/WorkerSession, the real worker, BaseExchange,
DefaultStore and canonical runtime, supplying only terminal host capabilities.
Additional observations pin config setter/callback/console/perf order, original
non-Error cause identity, raw dump payload identity and binary64 bits, unknown-op
fanout to all nine receivers, shared buffered identity and replay short circuit.
A 150-level unread config remains intact; this is a preservation witness, not an
accepted shared numeric-depth limit. Held startup teardown detaches listeners,
disposes the actual exchange, clears timers/tasks, prevents a late snapshot and
releases worker ownership even if a caller still holds the session handle.

Independent compiled controls replace native config or buffering with JSON,
remove module session ownership, omit context shutdown, or move constructor
decoding before replay. All nine controls compiled and failed their named binding tests, including
original-cause reconstruction, partial teardown, swallowed normal-dispose errors
and a perf-dump JSON hop. Exact production bytes were restored before the
positive gates. Existing numeric/field/source
input corpora and worker lifecycle/buffer tests remain in place. The generic
buffer helper keeps its owned-record fixture as a distinct parameter profile;
production Js identity is proved through the actual worker binding.

This closes row 20. It does not classify the separate legacy strict DTOs, worker
logging/report carrier, or raw admission inside the called operation handlers
(rows 17/18/19/26/72-79). Those remain OPEN with their original source cases.

## Config and perf codec retirement: row 18; rows 72-74 stay OPEN

After the native session cutover, a tracked-source Rust caller search finds the
six WorkerConfig/PerfFixtureFrame/PerfFixture/PerfFixtureConfig/PerfFixtureVersion/
PerfFixtureMode codecs only in their declarations, private serde/output helpers
and test consumers. The installed listener, config callbacks, perf arguments,
replay callback and dump result already use canonical Js. No runtime, FFI,
registry or browser binding calls these decoders. Their strict record/literal,
null and bounded-depth differences cannot affect that native session path.
The six codecs, private companions, numeric eligibility and orphan-only tests
are removed; the compiler checks that no real consumer depended on them.
WorkerConfig row 18 is ACCEPT-as-removed, without a depth-bound acceptance.

The native envelope remains independently live: its three public operation
literal inventories, instanceId null rejection, explicit data null, nested
Store/settings/error validation, and all ninety number-bit comparisons across
the original ten spellings and nine encoding routes remain. Config/perf-only
rejection and codec-only numeric/depth tests are retired with their subjects.
The source-only eleven-field optionality census and its refusal controls remain
unchanged, but its six retired config fields are no longer claimed native
record-validation coverage.

A real installed-listener supplement executes maintained store.webworker and
native WorkerModule/WorkerSession. Ten original numeric spellings produce fifteen
config deliveries, ten dump replies and sixty independent native bit assertions.
It retains cfg/perf/frame/msg references, own undefined and UTF16 code units;
non-record perf arguments, unknown mode, version 2 and explicit optional nulls
reach the host exactly as source does. Existing config getter/callback ordering,
array admission, original failure and unread-depth150 binding probes remain.
These observations prove the carrier, not an implementation of imported perf
functions: their terminal calls are explicitly supplied host capabilities.
Two independent compiled production controls replace the configure argument or
dump value with Null; each fails the named native identity assertion. Exact
producer bytes are restored and the same binding probe passes.

Rows 72-74 stay OPEN FIX. Maintained perfFixture.ts implements recording,
structured clone, a 20000-frame cap, timestamps, mode/reset/interval/loop rules,
and timer-based replay. The native host currently requires these callbacks but
has no service implementation. The real app uses record/replay and dump, as
confirmed by the coordinator; no dead-code or host-linking acceptance closes
that behavior. Its own following PR must implement and compare real TS/native
record-to-replay round trips. This batch closes one row: 8 FIX / 4 ACCEPT /
15 OPEN. All other worker ownership remains with codex-0xekzz; codex-l89ogr has
no active worker edits or source snapshot.

## Native perf service: rows 72–74

The source service has real host record/replay callers. Required perf host stubs
are replaced by one module-owned `PerfFixture` using canonical native `Js`.
`worker_context` receives that service and records at the outbound post choke
point before dedicated forwarding or shared-port broadcast. Config and dump
messages use the installed listener; replay init never loads a venue.

The maintained `perfFixture.ts` oracle records three actual outbound log, Store
and fill frames, dumps them, and replays them from a fresh module at 7ms cadence.
The native listener baseline at a11c258daa compiled and failed with zero frames
versus the source's three. The native implementation preserves captured snapshots
across later mutation, shared/cyclic references, own undefined, nonfinite/signed
zero bits and lone-surrogate UTF16. Replay posts each original captured message
reference, ignores atMs, and ends after the final non-looping frame.

Shared `js_runtime::structured_clone` is one platform operation, with no JSON
adapter. Native primitives, ordinary objects, sparse arrays, Map, Set and intrinsic
Errors are checked against Node's real structuredClone, including graph identity,
cycles, getters/deletion, Error cause/hidden fields and original getter throws.
NaN payload bits canonicalize exactly as Node structuredClone does; other numeric
bits remain intact. Functions, symbols, Promises and unsupported native object kinds refuse visibly
with DataCloneError; the recorder drops clone failures exactly as the source does.
The operation does not claim unmodeled browser resource clone/transfer support.

Additional source/native probes cover the cap before getters/clone, dropped
uncloneable records, frames-array identity and replacement on record reset,
interval clamping, fixed-cadence looping and module disposal with retained service
and session handles. The module cancels all pending metronome callbacks and closes
the service, following the approved full-teardown native lifetime policy.

The transferred cohort now has 11 FIX, 4 ACCEPT and 12 OPEN rows.

A compiled missing-NaN-normalization control fails the actual Node clone-bit
comparison; exact restoration passes. The real listener additionally records a
negative NaN with a non-default payload and checks Node's canonical output bits.

## Mutable default template: row 22

The source exports a shallow-frozen default, so edits to its nested ID array
seed later instances without changing earlier constructor snapshots. The native
constant always supplied a new empty array. One thread-local native Js template
now models the worker module realm and uses the existing Object.freeze builtin.
The constructor snapshots it at the existing shared typed state leaf before
reading options/positions; no JSON carrier or second default path is retained.

The compiled original constructor body on master 2a9ed2be9a, with the new template
access seam available, fails constructors_snapshot_the_live_shallow_frozen_default:
its first and second arrays lack the source seed/later IDs. The actual TS module
and native constructor compare empty, seeded and later snapshots, independent
instance mutations, retained template identity, outer freeze and mutable nested
arrays. A second native realm starts empty. The exact original constructor state
initialization is independently restored as a compiled control, then the fixed
bytes are restored for positive replay. Existing 648 source transition scenarios
and owned-instance-array mutation witnesses remain intact.

This closes the mutable default finding only. Rows 21/24/25/75-79 concern distinct
model/dispatcher consumers, so sharing task_types.rs does not make their raw
admission, output or depth questions evidence for acceptance. They remain OPEN.
With merged perf rows 72-74 in #1238, the transferred cohort now has 12 FIX,
4 ACCEPT and 11 OPEN rows.
