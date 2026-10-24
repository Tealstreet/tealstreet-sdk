# OKX ordered write ownership

## Scope and base

Plan-first base: actual `a77b8c37c4208c085ce0b0f2f9ad63079566c507`,
after the selected-history identity consumer #2067 landed. The assignment is
the remaining OKX ordered HTTP write funnel, not account, discovery, history,
closed positions or socket work. Production is held on the gates below.

The checker inventory is six fingerprints / ten occurrences:

| File | Native site | Occurrences |
| --- | --- | ---: |
| `venues/okx/src/trading.rs` | `next_write` request, `2aa67be1` | 1 |
| same | empty setting/cancel completion, `2b6c8768` | 3 |
| same | ID completion, `c5a021da` | 2 |
| same | serialization failure, `c873db6f` | 2 |
| same | failed trade terminal, `5d6691e3` | 1 |
| `venues/okx/src/lib.rs` | final `sign_request` funnel, `2e921890` | 1 |

Expected request inventory: OKX 6F/10O -> 0F/0O, with removal-only changes.
The credential and signing inventories must not grow. Audit every caller of
`sign_request` before deleting it. Shrink the OKX `STAGING_BOOTSTRAPS` new map
and its approved unit-test row in the same implementation; do not retain a
bound permitting the deleted funnel to return. Its current approved new map
contains `2e921890` and the already-retired `b1761cde`.

## Admission and authority

This section is the interim shared consumer pattern for OKX and Bitget. The
canonical streamed engine contract will be fe7lgm's plan on
`feat/rust-plan-serial-stream`; its constructor names and source pin are pending.
Earlier bounded `admit_plan(&preflight, &steps, ...)` clearance does not grant
clearance for that new primitive. The current direction is:

1. At genuine command entry, `begin_preflight(PreflightPolicy { purpose:
   WriteAfterReads, read_credential: None, write_credential: Some(primary) })`.
2. Compute the complete deterministic native job without publishing Store,
   progress, command output or I/O. Native computes planning results first but
   checks readiness before surfacing a planning error: not-ready NotSent wins
   over an invalid planning result. A retained queue owner uses preflight parking and
   ready callback, never a replayed numeric command.
3. Admit one command-owned streamed serial Plan. It has no declared step count
   or step cap: the engine owns a receipt-capped root and one active call at a
   time. Each call has a 20000 ms window, attempts one, Never, and no refusal
   fallback or added wait. Continue only from the previous step's exact DATA
   custody, under the same root. Do not simulate streaming with repeated Plan
   admission. Exact constructors await the shared owner; do not invent names.
4. Only after successful admission apply native optimistic status changes.
   Obtain each call from the reviewed streamed engine API, then send through
   `request_signed_managed` with the existing OKX Primary dispatch recipe.
   Preserve exact URL, body, demo header and Primary role; no local remapping.
5. Validate exact Plan delivery before decoding, response-driven Store/job
   changes or accepting IDs. Record classified facts and settle the exact call.
   Preserve native pre-release client-ID progress receipts, which identify an
   order after an Unknown result: publish after admission and before physical
   release, including the first call. They are not accepted-ID evidence. A
   subsequent call's receipts follow its successful continuation admission.
6. Final completion uses `done_plan`; failure uses the exact Plan authority.
   No numeric fallback, dummy call, per-page root, renewed plan, or venue timer
   creates admission or DATA rights. Retirement removes only the exact owner.

`begin_planned_execution` is not the constructor: no other command joins this
physical operation. It is reserved for joined executions under #1919.
The OKX crate must opt into `http-plans` explicitly at implementation time.

## Native behavior to preserve

- Plain orders batch twenty; cancel orders batch twenty; algo cancels batch
  ten. Protective orders follow each accepted entry before the next entry.
  Native standalone algo ordering, payload formatting, client IDs, decimal lot
  adjustment, accepted IDs and setting updates remain pinned.
- No retries. Network loss, HTTP 408/5xx, code 50004, malformed body or wrong
  row count stop the remaining chain and retain the native reconciliation
  fence. No failed body supplies definitive acceptance or resend proof.
- Standalone cancellation with code `2` may report definitive row refusals and
  continue later regular/algo batches. Code `1` (All operations failed) rejects,
  stops before siblings and restores native optimistic status. A dependent replacement never runs after its
  prerequisite cancellation refusal. Mixed accepted/refused results retain
  accepted identities; uncertain rows never authorize sibling continuation.
- Preparation/provider/local submission refusals retain exact cause and
  NotSent where nothing released. A later refusal cannot erase earlier effects.
- The all-refused code-`2` fixture is an edge-envelope witness, not the usual
  OKX all-failed response. Native emits diagnostics and can finish `[]` on that
  envelope; preserve it separately from the realistic code-`1` Rejected path.

## Shared and behavior gates before production

1. **Streamed serial primitive:** the current ruling is **no step cap**.
   The earlier 2048/2049 ruling and tests are superseded and removed. fe7lgm
   owns the shared primitive: exact-DATA continuation, O(1) resources per active
   step, one Context, receipt-capped elapsed time, and incremental full IDs.
   One plain input order permits 10000 split lots, or 500 twenty-row calls;
   both protections make 30000 calls (entry, stop, take-profit per lot).
   Important: that bound is per input order. `CommandIn::PlaceOrders.orders`
   is a Vec decoded directly by serde, and `plan_place` has no input-count cap.
   Thus 30000 is not a finite whole-command maximum. Report this distinction
   to the shared owner when sizing retention. No chunking, renewed authority
   or private step limit is authorized. Stack the consumer on the reviewed
   engine PR before production and re-review this section against its real API.
2. **Independent cancellation:** at this actual base `PlanPolicy` has no
   `independent_calls` field. The prerequisite is #1956 landing (combo-6b),
   carrying #1939; its non-landing frozen pin is `3745a230887`. Obtain its
   actual source and disposition for mixed/all-refused cancellation before implementation.
   Do not copy a proposed flag or simulate journal success in the venue.
3. **Terminal projection:** preserve native code-`1` rejection and code-`2`
   continuation as separate paths. The code-`2` all-refused edge retains its
   diagnostics then `done[]`. The shared owner proposes an admission-declared
   terminal policy: only when all released facts are definitive, with no Unknown
   or unreleased fact, `done_plan` may return the consumer result while retaining
   refusals. Defaults retain current projection. Exact API names are pending;
   no consumer fallback or fabricated acceptance is authorized. Code `1` still
   terminates via failure, never that success projection.
4. **Accepted-ID retention:** current fixed evidence limits cannot reproduce
   native output. The new primitive must retain and report every accepted ID
   incrementally, without truncation. The originals complete 30000 and 60000
   serial calls with every ID in native order. This shared evidence change
   belongs to the engine owner, not an extra venue journal.

For sizing, if plain inputs yield P unprotected lots, protected inputs yield
L_i lots with b_i enabled protections (one or two), and there are A standalone
algo inputs, the native request count is
`ceil(P / 20) + sum(L_i * (1 + b_i)) + A`.
Every L_i is individually capped at 10000; input count is not capped here.
Cancellation uses ceil(R / 20) + ceil(G / 10) for deduplicated regular/algo IDs.
These formulas describe current job construction, not new admissibility limits.

These gates also apply to the Bitget write consumer. Missing shared machinery
is reported to its owner and built once; this branch changes no engine source.

## Original controls and implementation proof

Commit the plan before adding a dedicated `request_context_ordered_writes`
target. Run it against the unchanged production base, zero ignored controls.
Record actual assertion failures independently from working native witnesses.

- Real managed route preparation before signing or a physical write.
- Genuine full-capacity refusal before optimistic status/progress, with every
  unrelated live preparation still pending.
- Native per-input-order split maximum (10000 lots) and maximum plus one,
  plus two maximum inputs proving that the cap is not whole-command scope.
- One maximum bracketed input completes 30000 calls; two complete 60000.
  Every call pins entry/stop/take-profit order, Primary key, native size/protection
  fields and client-ID progress before release; every final accepted ID survives.
- Code-`1` cancellation stops/rejects/restores; code-`2` all-refused edge reports
  diagnostics, continues and finishes empty. Not-ready wins over planning errors.
- Code 50004 stops/reconciles without resend. The dedicated target does not yet
  execute network/408/5xx/malformed/row-count cases.

The following remain source-stage gates, not executed claims for this target:

- Duplicate live legacy ID cannot replace a retained job or terminal; independent
  cryptographic signed-byte vectors (the bracket controls pin headers/body/order).
- Mixed cancellation rows, dependent replacement refusal, and network/408/5xx/
  malformed/row-count failures: no unauthorized next write or resend, preserve
  earlier accepted IDs and the native reconciliation behavior.
- Delayed provider and route preparation, final Claim at deadline equality,
  both timer/response orders, reused command ID plus stale old delivery, exact
  cleanup, and an unrelated live peer. These are source-stage gates, not claims
  that all can be demonstrated on the legacy source.

Reviewers: claude-4yjlw0 for native signer/write-consumer behavior and
claude-fe7lgm for Plan admission, continuation and journal projection. Obtain
bounded plan/original review, then source review after the gates close.
Run only scoped OKX tests and strict checks locally with CARGO_BUILD_JOBS=4.
CI supplies the full matrix and the exact OKX family WASM bytes. No budget,
profile or CPU-policy change is part of this slice; size delta is unmeasured.

## Executed original checkpoint

On the unchanged `a77b8c37c42` production source, the dedicated target compiles
and runs six tests: **two assertion REDs, four GREEN, zero ignored**. The REDs
are missing managed route preparation and optimistic `Canceling` publication
despite actual shared-capacity exhaustion; every parked peer remains live.
Working controls pin 10000/10001 lots, two maximum inputs in one command,
all-refused cancellation diagnostics/continuation/`done[]`, and code 50004
stopping the protective chain while reconciliation starts without write retry.
These are original-source receipts, not managed implementation proof.

The current streamed-direction checkpoint removes both capacity pins and runs
eleven controls: **two assertion REDs, nine GREEN, zero ignored**. New positives
complete 30000 and 60000 calls with all IDs, pin pre-release progress and wire
order, realistic code-`1` rejection/status restoration and readiness precedence.
The 500-request/10000-ID positive remains. Runtime is 6.15 seconds locally with
one test thread; this is not a managed Plan CPU certification. No production,
existing fixture, Cargo or checker source has changed.
