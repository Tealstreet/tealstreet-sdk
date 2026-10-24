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

The Plan owner's API direction is command-owned admission:

1. At genuine command entry, `begin_preflight(PreflightPolicy { purpose:
   WriteAfterReads, read_credential: None, write_credential: Some(primary) })`.
2. Compute the complete deterministic native job without publishing Store,
   progress, command output or I/O. Existing readiness/error precedence stays
   explicit. A retained queue owner uses the existing preflight parking and
   ready callback, never a replayed numeric command.
3. `admit_plan(&preflight, &steps, PlanPolicy { operation: Mutation,
   credential: Some(primary), pacing: None, retry: Never })` atomically admits
   the complete job. One serial `StepBound` per physical request, wait zero;
   one `CallBound`, timeout 20000, attempts one, `RetryWindow::Shared`, no
   refusal fallback. The declared span is 20000 times the physical call count.
4. Only after successful admission apply native optimistic status changes.
   Obtain each call from `plan_slots` / `plan_leg_context`, then send through
   `request_signed_managed` with the existing OKX Primary dispatch recipe.
   Preserve exact URL, body, demo header and Primary role; no local remapping.
5. Validate exact Plan delivery before decoding or changing the job, Store,
   accepted IDs or progress. Record shared classified write facts and settle
   the exact call. Progress reflects accepted native identities, not speculative
   receipt declarations before dispatch.
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
- Standalone cancellation may report each definitive row refusal and continue
  later regular/algo batches. A dependent replacement never runs after its
  prerequisite cancellation refusal. Mixed accepted/refused results retain
  accepted identities; uncertain rows never authorize sibling continuation.
- Preparation/provider/local submission refusals retain exact cause and
  NotSent where nothing released. A later refusal cannot erase earlier effects.
- The current native all-refused standalone cancellation path emits row
  diagnostics and can finish `[]`. Pin that original behavior separately from
  any shared journal correction; do not silently promise terminal parity.

## Shared and behavior gates before production

1. **Physical-credit bound:** `PlanBound::from_steps` enforces 256 attempts.
   The overseer ruled no new 257-call refusal: native successful inputs stay
   supported. fe7lgm owns sizing the shared increase and memory/WASM/CPU cost.
   One plain input order permits 10000 split lots, or 500 twenty-row calls;
   both protections make 30000 calls (entry, stop, take-profit per lot).
   Important: that bound is per input order. `CommandIn::PlaceOrders.orders`
   is a Vec decoded directly by serde, and `plan_place` has no input-count cap.
   Thus 30000 is not a finite whole-command maximum. Report this distinction
   to the shared owner before choosing a replacement bound. No chunking,
   renewed authority or private limit is authorized.
2. **Independent cancellation:** at this actual base `PlanPolicy` has no
   `independent_calls` field. The prerequisite is #1956 landing (combo-6b),
   carrying #1939; its non-landing frozen pin is `3745a230887`. Obtain its
   actual source and disposition for mixed/all-refused cancellation before implementation.
   Do not copy a proposed flag or simulate journal success in the venue.
3. **Terminal projection:** resolve the native all-refused `done[]` case with
   the shared journal owner and overseer. The owner confirms that an all-refused
   journal currently projects failure, whereas native code can finish `[]`.
   Native continuation alone does not prove matching final outcome. Preserve
   the original witness; no terminal bypass is authorized.

These gates also apply to the Bitget write consumer. Missing shared machinery
is reported to its owner and built once; this branch changes no engine source.

## Original controls and implementation proof

Commit the plan before adding a dedicated `request_context_ordered_writes`
target. Run it against the unchanged production base, zero ignored controls.
Record actual assertion failures independently from working native witnesses.

- Real managed route preparation before signing or a physical write.
- Genuine full-capacity refusal before optimistic status/progress, with every
  unrelated live preparation still pending.
- Duplicate live legacy ID cannot replace a retained job or its terminal.
- Native per-input-order split maximum (10000 lots) and maximum plus one,
  plus two maximum inputs proving that the cap is not whole-command scope.
- Native bracket entry/stop/take-profit order and Primary signed bytes.
- Native all-refused cancellation diagnostics, final output and later sibling
  continuation, plus mixed rows and dependent replacement refusal.
- Network/408/5xx/50004/malformed and wrong row count: no resend or next write;
  native reconciliation still starts. Later accepted IDs survive uncertainty.
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
