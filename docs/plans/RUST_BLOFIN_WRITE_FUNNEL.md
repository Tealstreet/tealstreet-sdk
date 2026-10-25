# BloFin trading write funnel

Base: `feat/rust-rewrite` `9977ccf1aa6` (after S1 #2155 and #2143). Owner:
`claude-kuhwxi` (overseer assignment). Reviewers: `claude-dazzy3` or
`claude-fe7lgm` (source), `claude-4yjlw0`. Landing: `claude-8x5tm1` exact-tree
GO. This executes steps 6 and 7 of the area plan
[RUST_BLOFIN_REQUEST_CONTEXT.md](RUST_BLOFIN_REQUEST_CONTEXT.md) and removes
BloFin's last 8 request-inventory entries, all in `trading.rs`.

## Scope and phases

| Site (`trading.rs`)                                        | Today                                                      | Target                                                                                                                                                       |
| ---------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `:923` entry preflight                                     | `cx.fail(id, NotSent/InvalidInput)` after a throwaway plan | Retained Preflight root first; `fail_preflight` (NotSent) without HTTP                                                                                       |
| `:1178` dequeue re-plan                                    | `cx.fail` on the raw queued command, then `trade_next`     | Each queued command keeps its own parked Preflight; woken by `request_preflight_ready`; a re-plan failure is `fail_preflight`                                |
| `:1360` sequential POST / Verify GET                       | `cx.request` with inline `headers()`                       | One streamed serial Plan per job: Mutation calls Primary-signed by the #2084 recipe; the Verify GET is a Read call signed with the Preflight read credential |
| `:1303` job success                                        | `cx.done(json(ids))`                                       | `finish_plan_call` + `done_plan(json(ids))`; `done_empty_plan` when nothing is admitted                                                                      |
| `:1405` job failure                                        | `effective`-adjusted outcome + `cx.fail`                   | `finish_plan_call` + `fail_plan`; the journal projects the outcome; the local `effective` edit is deleted                                                    |
| `:1391` `matches!(request.step.effect, Effect::Place{..})` | Frozen macro in a Cx-escaping scope                        | Leaves with the conversion; the `orders_revision` bump moves to the failure and retirement paths                                                             |
| `:1242` hedged leverage join                               | Two parallel POSTs, local join                             | Declared Plan with one all-Mutation barrier step (`admit_plan_with_barriers`)                                                                                |
| `:1630` join failure                                       | Local first-error aggregation                              | `fail_plan`; the journal projects (one 503 plus one Accepted is Unknown, IDs kept)                                                                           |

- **W1** (one PR): every site except the hedged-leverage join, six
  occurrences including the macro.
- **W2** (one PR, after W1 lands): the barrier join. BloFin is the named first
  consumer of `RUST_PLAN_MUTATION_PREPARE_BARRIER.md`, whose engine side has
  landed with no venue using it yet.

Splitting keeps each size measurement and review scoped to one engine feature.

## W1 shape (precedent: Bitget trading, `RUST_BITGET_TRADING_CONTEXT.md`)

### Entry and queue

- **Root first.** Every trading command opens its root with
  `begin_preflight(PreflightPolicy{WriteAfterReads, read credential, write:
Primary})` before validation. A local validation refusal is `fail_preflight`
  (Mutation policy, NotSent), never a legacy `fail`.
- **Native FIFO kept.** A command that can't start because a job is running is
  parked with its own Preflight (`park_preflight_for`). When the running job
  ends, the front is woken by `request_preflight_ready`, and
  `on_http_preflight_ready` re-plans it against the current Store. No command
  ever lends another's Context.

### Serial Plan per job

- **Steps.** Each job's steps become calls of one `admit_serial_plan`:
  placements, cancels, verification and settings. Each next call is admitted
  only from the previous call's validated DATA (`continue_serial_plan`), and
  zero pacing is required.
- **Calls.**
  - Mutation calls are `SerialCall{Mutation, 20 s}`, Primary, signed by
    `dispatch_signing::Http`.
  - The Verify GET is `SerialCall{Read}` with the Preflight read credential:
    Secondary else Primary, as GET is today. It stays out of the journal.
    It is never retried: it uses `continue_serial_plan`, never a delayed
    continuation, so the engine's one read retry under `Next::After` does not
    apply. This matches native
    `failed_final_detail_read_is_not_retried_and_never_places`.
  - Every Mutation `SerialCall` keeps `attempts: 1`. BloFin has no native
    single resend of a proven refusal.
- **Native pauses.** The 350 ms (placement and normal or algo cancel) and
  1000 ms (TP/SL cancel) pauses become `continue_serial_plan_after(delay)`
  after the step's DATA. This is the first use after Mutation DATA (Bitget uses
  it only for read retries), so it is pinned by a RED. The step's write fact
  (`http_write_batch`, `http_write_fact` or `http_write_unproven`) is recorded
  before `continue_serial_plan_after`; `validate_serial_continuation` refuses
  otherwise.
- **Unchanged native facts:** exact paths, bodies and batch sizes (20 normal
  place or cancel, 10 TP/SL cancel, 1 algo); lot splitting up to 1000 with
  freshly minted client IDs; broker attribution; caller client-ID rules; and
  receipts through `command_progress` after admission and before release.

### Write facts (`classify_write` / CodeTable)

- **Batch place and cancel** report per-row facts through `http_write_batch`.
- **Unproven releases.** Row-count mismatch, empty or duplicate IDs, an
  unrelated or duplicate cancel identity, a body parse error and multi-position
  shape errors become `http_write_unproven`. They stay Unknown, as today.
- **Released 408, 5xx or transport.** The engine seals Unknown. The venue keeps
  `on_http_owner_retired_preflight` plus `capture_http_reconciliation`, and
  restores Creating and Canceling rows at the next Cx callback, as Bitget does.
- **Delivery.** `validate_plan_delivery` runs before decoding, and a stale or
  mismatched delivery only closes its call.

### Store effects

The margin-change notice, the `orders_revision` bump, Creating upserts and
Canceling marks move from job start to per call, after admission and before
release. `cleanup_job` runs on every terminal (success, failure, retirement).
Commit effects (settings, positions, hedged) apply only after a validated
successful terminal.

### Decisions for review (each is caller-visible)

- **D-a, row and setting refusal codes (4yjlw0; OKX #2117 Option B
  precedent).**
  - Documented pre-execution refusal codes become explicit CodeTable rows with
    verdict Refused. Each row cites BloFin's official error-code reference (URL
    and revision in the table's doc comment), or a TS mapping that already
    treats it as a refusal. The exact list is fixed in the RED commit and
    reviewed there.
  - The table's `unlisted` verdict is **Unknown** (outcome `None`), not
    Refused. On a serial Plan a Rejected fact passes
    `validate_serial_continuation`, so an unlisted refusal must not authorize
    the next batch or step. A caller told Rejected may also resubmit a write
    that actually landed; Unknown reconciles first.
  - Exact-row versus range precedence gets a unit test, as #2117 does.
  - Shape violations stay Unknown. CALLERS records that undocumented codes
    report Unknown.
- **D-b, Verify failure.**
  - After an Accepted cancel, a failed final-detail verification (wrong state,
    fill moved, not exactly one row) is `fail_plan`. The Verify GET is a Read
    call with no write fact, so the Accepted cancel is the only effect
    evidence.
  - The engine already projects this case. In
    `engine/src/request_context.rs` `failure_projection`, a released Accepted
    fact with no uncertainty and not all-rejected gives outcome `None`, never
    NotSent or Rejected. So no venue-side clamp replaces the deleted
    `effective` field.
  - The D-b RED pins the result. If it fails, a minimal clamp returns rather
    than the field.
- **D-c, cancel "gone".**
  - Code 1000 or "does not exist" on a cancel is `Satisfied(Cancel)`. The
    CodeTable row needs `only: Some(Cancel)`, since classify rejects a
    Satisfied row without a matching `only`.
  - Satisfied projects to `WriteResult::Accepted{[]}`, so the engine would
    permit continuing to a dependent replacement. Stopping is therefore a venue
    decision: when a replacement depends on the cancel, the venue stops before
    `continue_serial_plan` ("requires a confirmed cancellation", as today). Its
    RED asserts no replacement Prepare, not just the terminal, the shape of
    #2117's `a_dependent_replacement_never_runs_after_its_cancel_is_refused`.
  - `http_write_fact` on `Satisfied(Cancel)` arms the absent-order fence and
    queues one reconcile. BloFin implements `reconcile_uncertain` to issue its
    fresh order snapshot (the open-order pages) and calls
    `cx.orders_reconciled` on a clean committed snapshot. Otherwise
    cancel-all for that symbol would stay fenced for good (#2117's gap 3). A
    RED drives a "gone" cancel, then the reconcile snapshot across its pages,
    and shows the fence cleared.
  - This is a behaviour change, accepted as the shared absent-order rule:
    BloFin links order-locks, so `Satisfied(Cancel)` runs `fence_absent_orders`,
    which fences the symbol and queues a reconcile read (deferred while a
    placing scope is held). Today's plain-cancel "gone" does neither. The D-c
    RED and the CALLERS entry pin both effects.
- **D-d, progress abort.** A target that filled or closed between steps (Store
  observation) ends the job with `finish_plan_call` + `fail_plan` before the
  next delayed continuation. Pinned by a RED.
- **D-e, partial IDs.** Accepted IDs now ride the journal on failure, which
  fixes ERROR_CLASSIFICATION #12. Today `cx.fail` drops them.

### Orphans removed in W1

- `Blofin::headers` and `sign::signature`, if no other caller remains (reads
  stopped using them in #2143).
- The `effective` field and its edit.
- The `Pending::Trade` legacy routing in `on_request_failed`.
- Each of these is removed in the same change, per the orphan rule.

## W2 shape (barrier join)

- **Plan.** Hedged isolated leverage (long then short) is one declared Plan:
  `admit_plan_with_barriers(preflight, [StepBound{wait_ms: 0, calls: [long,
short]}], PlanPolicy{Mutation, Primary, ..}, [MutationBarrierBound{step: 0}])`,
  with `prepare_group_http_release` returning Unscheduled.
- **Barrier behaviour.** Both legs prepare and sign in declaration order. If
  either preparation fails, there is zero HTTP and the whole command is NotSent
  (today: either header fails, neither is sent).
- **Independence.** `independent_calls` stays off. The join is one setting,
  and a released 408 or 5xx on either leg seals the Plan Unknown.
- **Commit.** Applied only when both legs succeed.

## Size gate

The number to watch is the family delta against the BloFin venue budget
(1,343,488). After S1 the Plan core is shared, so the combined delta should be
small.

Post-S1, a combined increase over +30K goes to the overseer. W1 links
`serial-plans`; W2 adds `http-mutation-barriers`, making BloFin the first crate
with both. Each phase freezes a paired measurement on the actual landing tip
before full source (BloFin venue plus combined, Rust 1.99.0, Binaryen 132,
`BINARYEN_CORES=4`, shipped profiles). Scratch targets are deleted after each
measurement.

## Original controls (RED first, per phase)

W1 (each fails on the base):

- an entry refusal is a Preflight NotSent with no HTTP;
- a queued second command keeps its own root, wakes in FIFO order and fails
  NotSent on a dequeue re-plan refusal;
- each place, cancel, amend and setting call is a managed Prepare (Primary)
  with exact native bytes, and the Verify GET is a managed Read;
- a 350 ms and a 1000 ms pause are `continue_serial_plan_after` continuations:
  the step's write fact is recorded before the continuation, nothing is sent
  before the delay, and exactly one send follows it;
- a maximum lot split (1000 lots, 50 place calls at 20 per batch) streams
  through one Plan, and the journal, receipts and IDs carry the whole split;
  a refusal mid-split keeps the IDs accepted before it (with D-e);
- a pause is admitted only after a released Accepted, Rejected or KnownPartial
  fact. A PartialUnknown batch ends the job rather than waiting out the pause.
- retirement or a released 503 during a pause sends nothing later, clears the
  timer, and gives exactly one root terminal;
- a parked command whose caller wait expires while queued fails once, does not
  block the FIFO, and `request_preflight_ready` wakes the next command;
- D-a: a documented row refusal is Rejected, an unlisted code is Unknown, a
  shape mismatch is Unknown, and exact-row versus range precedence holds;
- D-b: a Verify failure after an Accepted cancel is never NotSent;
- D-c: plain-cancel "gone" is satisfied, fences the symbol and queues one
  reconcile read (deferred under a held placing scope); replacement-cancel
  "gone" stops with no replacement Prepare; the reconcile snapshot clears the
  absent-order fence;
- D-d: a progress abort settles before the next continuation;
- D-e: a failure after accepted rows keeps their IDs;
- a released 503 seals Unknown, retirement restores Creating and Canceling
  rows, and the next queued command proceeds;
- a late or duplicate response after a terminal changes nothing.

W2:

- either leg's preparation failure gives zero HTTP and NotSent, including a
  refusal on the second leg after the first leg prepared;
- both succeed and commit;
- one 503 plus one Accepted gives Unknown with the ID kept and no commit.

Existing trading fixtures keep their assertions; only how they drive the engine
changes, as in #2143.

## Delivery

One PR per phase on `feat/rust-blofin-write-funnel` (W2 on its own branch).
Each lands RED first, then source with the exact inventory prune (W1: 6
occurrences, W2: 2) and a CALLERS note for D-a to D-e. Local gates: the BloFin
suite, clippy `-D warnings`, fmt, `check-bans.sh`, and the paired size. Then one
push and one PR run.
