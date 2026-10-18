# Worker FIX-audit ledger

The transferred cohort is the 27 worker rows in `codex-l89ogr.json` from the
de-duplicated gap-census assignment. IDs below are its zero-based row indices,
not a new census. Row 30 was already repaired by merged PR #1158. The other
26 rows transferred to codex-0xekzz without active worker edits.

| Row | Maintained source surface | Resolution | Evidence or next review |
| --- | --- | --- | --- |
| 2 | ChaserOrderManager#tryPlaceOrders | FIX | Canonical eager continuation and await checkpoints; late-stop cancellation survives a dropped waiter. |
| 3 | ChaserOrderManager#emitComplete | FIX | Canonical settlement and retained fill reconciliation after caller abandonment. |
| 4 | chaserOrderManagerInstance | OPEN | Constructed module/receiver composition. |
| 5 | TwapInstance.resume | FIX | Canonical Promise settlement; immediate prefix and rejection profiles below. |
| 6 | TwapInstance.terminate | FIX | Same settlement boundary; synchronous cleanup and partial-commit order preserved. |
| 7 | UpdatePositionsHandler.handleUpdatePositions | FIX | Shared eager boundary; source/native timing profiles below. |
| 8 | UpdatePositionsHandler.handle | FIX | Same operation invoked during dispatch, without an extra spawn. |
| 17 | StoreWorkerPayload | OPEN | Whole worker carrier and report boundary. |
| 18 | WorkerConfig | OPEN | Actual admitted config and host reachability. |
| 19 | splitWorkerLogExtra | OPEN | Retained carrier across callers; no isolated JSON shim. |
| 20 | start | OPEN | Constructed worker session/bootstrap lifecycle. |
| 21 | ChaserOrderState | OPEN | Native state and output boundary. |
| 22 | defaultChaserState | OPEN | Native default ownership and caller mutation. |
| 23 | ChaserOrderManager | OPEN | Constructed manager and receiver contracts. |
| 24 | ChaserTerminate | OPEN | Live termination admission. |
| 25 | ChaserCompleteEvent | OPEN | Native event payload. |
| 26 | CallMethodHandler.handleCallMethod | OPEN | Continuation timing repaired below; typed arguments/primitive rejection admission remains. |
| 30 | readStringProperty | ACCEPT | Merged #1158: native Get, getter cause and approved typed String replacement. |
| 71 | ExchangeWorker.start | OPEN | Real bootstrap construction and startup race. |
| 72 | PerfFixtureFrame | OPEN | Recording/replay carrier reachability. |
| 73 | PerfFixture | OPEN | Recording/replay envelope. |
| 74 | PerfFixtureConfig | OPEN | Actual config admission. |
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
