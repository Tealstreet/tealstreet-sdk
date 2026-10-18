# Worker FIX-audit ledger

The transferred cohort is the 27 worker rows in `codex-l89ogr.json` from the
de-duplicated gap-census assignment. IDs below are its zero-based row indices,
not a new census. Row 30 was already repaired by merged PR #1158. The other
26 rows transferred to codex-0xekzz without active worker edits.

| Row | Maintained source surface | Resolution | Evidence or next review |
| --- | --- | --- | --- |
| 2 | ChaserOrderManager#tryPlaceOrders | OPEN | Eager entry and cancellation ownership. |
| 3 | ChaserOrderManager#emitComplete | OPEN | Native event and await order. |
| 4 | chaserOrderManagerInstance | ACCEPT | Pure empty-list allocation; actual module/thread identity and real-engine witness below. |
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
| 23 | ChaserOrderManager | ACCEPT | Explicit required host composition; actual singleton/engine/Store binding below. |
| 24 | ChaserTerminate | OPEN | Live termination admission. |
| 25 | ChaserCompleteEvent | OPEN | Native event payload. |
| 26 | CallMethodHandler.handleCallMethod | OPEN | Actual method/Promise dispatch. |
| 30 | readStringProperty | ACCEPT | Merged #1158: native Get, getter cause and approved typed String replacement. |
| 71 | ExchangeWorker.start | OPEN | Real bootstrap construction and startup race. |
| 72 | PerfFixtureFrame | OPEN | Recording/replay carrier reachability. |
| 73 | PerfFixture | OPEN | Recording/replay envelope. |
| 74 | PerfFixtureConfig | OPEN | Actual config admission. |
| 75 | ChaserOrderOptions | OPEN | Shared task model and live reads. |
| 76 | TwapOptions | OPEN | Shared task model and live reads. |
| 77 | TwapState | OPEN | Native state/serialization. |
| 78 | CallMethodHandler.handle | OPEN | Dispatcher entry timing. |
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
