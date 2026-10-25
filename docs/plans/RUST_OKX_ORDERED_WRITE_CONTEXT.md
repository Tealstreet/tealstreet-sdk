# OKX ordered write ownership

## Status (2026-10-25, claude-4yjlw0 takes over from codex-ih35js)

- **Base and REDs.** The branch is `feat/rust-okx-ordered-write` from landing `b0008964fbb`, with ih35js's plan and originals merged unchanged.
  - The branch has since merged tip `7ada358c446`, which includes S0 (#2103) and #2111. It has no engine edits, so no S0 rewrite applied; only `docs/plans/README.md` conflicted.
  - On that tip, `request_context_ordered_writes` still runs 13 controls: **two assertion REDs, eleven GREEN, zero ignored**.
  - The REDs are `write_requires_managed_route_preparation_before_provider_or_release` and `admission_capacity_refusal_precedes_optimistic_store_change`.
- **Engine prerequisites landed.**
  - The streamed serial Plan landed as #2092 (`ff219538b30`): no step cap, no root clock, incremental full IDs, and `SerialPolicy.refusals_complete`.
  - Condition waits (#2094) also landed, but OKX does not need them: it has no detection or mode wait and no in-job read.
  - `fail_plan_noted` (#2110) is not needed either, since OKX has no protection-rollback suffix.
- **Gates 1, 2 and 4 below are closed by #2092.** Gate 2's independent cancellation is not needed on the serial path:
  - Each call settles before the next.
  - `continue_serial_plan` accepts a definitive Rejected or KnownPartial fact, so a code-`2` cancel with refused rows continues to its sibling batches.
  - An all-refused code-`2` edge finishes `done []` through `refusals_complete`.
  - Source stage pins all three with GREENs.
- **New source-stage gaps,** found against the landed managed API:
  1. OKX implements no `capture_http_reconciliation`, and the engine refuses managed mutations without it (`engine.rs` default returns Invalid). It must return bounded correlation data, as Bitget's does.
  2. `request_signed_managed` takes no headers. Legacy writes add `Content-Type: application/json`, so the OKX recipe's `apply` must add it, as Bitget's recipe does. The demo header already rides `unsigned_http_headers`.
  3. An Unknown managed write arms the engine's absent-order fence, which clears only on `cx.orders_reconciled(...)` (dazzy3). OKX's native reconciliation fence must be wired as Bybit's `reconcile_uncertain` is: record the uncertainty on the Unknown, then call `orders_reconciled` on the committed order snapshot. Otherwise one lost write leaves later absent-order handling fenced for good.
- **`STAGING_BOOTSTRAPS` no longer exists in code,** so that requirement below is void.
- **No queue.** OKX starts each command's job at once and runs jobs concurrently, ordered only within a job. That stays native: one Preflight and one serial Plan per command, with no `park_preflight` FIFO.

### Size (measured, features only; local raw, function names excluded; CI factor about 1.0)

The base is tip `b0008964fbb`. The OKX family is 1,118,920 bytes in CI, against the 1,343,488 venue budget (224,568 of room).

| Change | Bytes |
| --- | ---: |
| OKX family: `serial-plans` (implies `http-plans`) linked, no use | **+50,393** |
| Combined, OKX lands first (first `serial-plans` adopter) | **+11,939** |
| Combined, OKX lands after Bitget slice 3's `condition-waits` | **0** |
| Combined, Bitget slice 3's features alone, for reference | +60,650 |

- **Estimated use cost.** Use adds about +30K to +60K on the OKX family. That is Bitget's analogue (+76K at engine s) minus the reads, the queue and the condition waits OKX lacks.
- **Expected family total.** About +80K to +110K, so OKX lands around 1.20M to 1.23M, inside its budget.
- **Expected combined total.** About +12K plus OKX venue code if OKX goes first, or about 0 plus OKX venue code after Bitget slice 3.
- **No budget, profile or CPU-policy change** is part of this slice.

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
The credential and signing inventories must not grow. `sign_request` has one
caller (`trading.rs:820`); deleting it also removes one lib.rs `read_credentials`
credential entry. (The former `STAGING_BOOTSTRAPS` map no longer exists.)

## Admission and authority

This follows the landed streamed serial Plan (#2092, `ff219538b30`), the same
consumer shape as Bitget slice 3 (#2122). Earlier bounded
`admit_plan(&preflight, &steps, ...)` clearance does not apply to it.

1. At genuine command entry, `begin_preflight(PreflightPolicy { purpose:
   WriteAfterReads, read_credential: None, write_credential: Some(primary) })`.
2. Compute the complete deterministic native job without publishing Store,
   progress, command output or I/O. Native computes planning results first but
   checks readiness before surfacing a planning error: not-ready NotSent wins
   over an invalid planning result. A retained queue owner uses preflight parking and
   ready callback, never a replayed numeric command.
3. Admit one command-owned streamed serial Plan. It has no declared step count
   or step cap: the engine owns one root and one active call at a time. The
   root has no whole-chain wall-clock, as the latest overseer ruling requires.
   Each call has a 20000 ms window, attempts one, Never, and no refusal
   fallback or added wait. Continue only from the previous step's exact DATA
   custody, under the same root. Do not simulate streaming with repeated Plan
   admission. The landed constructors are `admit_serial_plan(preflight,
   SerialCall { operation: Mutation, timeout_ms: 20000, attempts: 1 },
   SerialPolicy { credential: primary, pacing: None, refusals_complete: true })`
   and `continue_serial_plan(plan, slot, context, next)`. Empty native jobs use
   `done_empty_plan`, with no dummy first call.
4. Only after successful admission apply native optimistic status changes.
   Obtain each call from the landed streamed engine API, then send through
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
The OKX crate opts into `serial-plans`, which implies `http-plans`, at implementation time.

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

## Shared and behavior gates (all closed by #2092)

Each gate is checked against the landed engine, `request_context_serial_plan.rs` and `request_context.rs`.

1. **No step cap.** It is closed. `continue_serial` admits each next call against the Plan alone, with O(1) resources per active step, one Context and no declared count.
   - One plain input order permits 10,000 split lots (500 twenty-row calls), and both protections make 30,000 calls per input. `CommandIn::PlaceOrders.orders` has no input-count cap, so there is no finite whole-command maximum. That stays native.
   - No chunking, renewed authority or private step limit is used.
2. **Continuation on definitive facts.** It is closed. `continue_serial` gates on `WriteJournal::validate_serial_continuation` (`request_context.rs` near :2271). The latest released attempt must be Accepted, Rejected or KnownPartial, and the Plan must be neither sealed nor `serial_uncertain`.
   - So a definitive code-`2` row refusal continues to the sibling batches, and an Unknown stops the chain.
   - No `independent_calls` field is needed, and the #1956 prerequisite is void.
   - Because the engine PERMITS continuing after a Rejected fact, two stops are venue decisions with their own REDs (below): code `1` stops, and a dependent replacement never runs after its prerequisite cancel's refusal.
3. **Terminal projection.** It is closed. `SerialPolicy.refusals_complete` is the all-refused `done []` path for the code-`2` edge, and it never overrides `fail_plan`. Code `1` still ends through `fail_plan`, never that success projection.
4. **Accepted-ID retention.** It is closed. The serial Plan reports full IDs incrementally, with no truncation. The originals complete 30,000 and 60,000 serial calls with every ID in native order.
5. **No root clock.** It is closed. A serial Plan has no whole-chain deadline, and caller-wait expiry alone is not a serial terminal. Each physical call keeps its 20 s window. `native_bracket_continues_and_finishes_after_caller_receipt_deadline` pins this.

For sizing, if plain inputs yield P unprotected lots, protected inputs yield
L_i lots with b_i enabled protections (one or two), and there are A standalone
algo inputs, the native request count is
`ceil(P / 20) + sum(L_i * (1 + b_i)) + A`.
Every L_i is individually capped at 10000; input count is not capped here.
Cancellation uses ceil(R / 20) + ceil(G / 10) for deduplicated regular/algo IDs.
These formulas describe current job construction, not new admissibility limits.

This branch changes no engine source; any missing shared machinery goes to its owner.

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

Source-stage REDs, added before source (dazzy3, plan review of `dbf746ef6bf`):

- **Code-1 stops.** A cancel answered with code `1` ('All operations failed') calls `fail_plan`. No `continue_serial_plan` follows, so no sibling batch is sent, and native optimistic status is restored.
- **A dependent replacement after a refused prerequisite cancel issues nothing.** The replacement is never prepared or sent, and the terminal keeps the cancel's refusal.
- **The fence clears on reconcile.** A lost create (Unknown) is followed by the order reconcile snapshot. `orders_reconciled` clears the engine's absent-order fence, and later absent-order handling is no longer fenced.

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

Owner: claude-4yjlw0. Reviewer: claude-dazzy3 or claude-fe7lgm (whichever is
free) for Plan admission, continuation and journal projection. Obtain
bounded plan/original review, then source review after the gates close.
Run only scoped OKX tests and strict checks locally with CARGO_BUILD_JOBS=4.
CI supplies the full matrix and the exact OKX family WASM bytes. No budget,
profile or CPU-policy change is part of this slice; see the measured size above.

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

Additive native-parity controls confirm that the receipt deadline does not cap
the native bracket chain, accepted IDs preserve duplicates (`repeated` three
times), and successive writes release at the same response time without a
venue-added step wait. Native appends `job.ids` in ACK order; no ID dedup is
performed. Readiness now explicitly asserts the requested market is absent,
so the not-ready result demonstrates precedence over a real planning error.
The refreshed dedicated target executes thirteen controls: **two original
assertion REDs, eleven GREEN, zero ignored**; scoped strict Clippy and fmt pass.

## Source-stage REDs checkpoint

`request_context_ordered_writes` adds dazzy3's three source-stage REDs. Run against the unchanged base, all three fail at their managed-preparation assertion, as intended.

To confirm the behaviour each one pins, the same three were run once with that assertion disabled (not committed):

- `a_managed_code_one_cancel_fails_the_plan_without_continuing` passes natively. The code-1 stop and restore are native parity.
- `a_dependent_replacement_never_runs_after_its_cancel_is_refused` passes natively. A refused prerequisite cancel issues no replacement.
- `a_lost_create_fences_cancel_all_until_the_reconcile_snapshot` fails natively, at 'a fresh reconcile read'. A lost create does fence cancel-all, but OKX never issues the reconcile read, because it has no `reconcile_uncertain` hook. That is gap 3, so the fence never clears.

The target now runs 16 controls: 5 REDs, 11 GREEN, 0 ignored.
