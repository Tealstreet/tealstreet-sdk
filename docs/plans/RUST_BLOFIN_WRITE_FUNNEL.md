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
| `:1242` hedged leverage join                               | Two parallel POSTs, local join                             | W1: two serial Mutation calls (long, then short) in the job's Plan. W2: one all-Mutation barrier step (`admit_plan_with_barriers`)                           |
| `:1630` join failure                                       | Local first-error aggregation                              | `fail_plan`; the journal projects (one 503 plus one Accepted is Unknown, IDs kept)                                                                           |

- **W1** (one PR): all 8 occurrences, including the macro, by direct
  cutover (overseer approval, 2026-10-26). The hedged-leverage join runs as
  two serial Mutation calls, long then short, in the job's one streamed Plan.
  Short is admitted only from long's DATA (`continue_serial_plan`) after
  long's Accepted fact is recorded. Keeping the join on legacy dispatch would
  need a mixed Preflight/legacy FIFO, a start timer and a re-plan race, which
  is migration-state code the cutover rule forbids.
- **W1 interim behaviour (caller-visible, W1 only).** Short is sent only after
  long is Accepted: one extra round trip, and a long failure means short is
  never sent. That's strictly more conservative. CALLERS records it as a W1-to-W2
  interim, and the READY note names it to the overseer.
- **W2** (one PR, queued immediately after W1 lands): the barrier join, which
  restores native parallel release. BloFin is the named first consumer of
  `RUST_PLAN_MUTATION_PREPARE_BARRIER.md`, whose engine side has landed with
  no venue using it yet. W2's scope and REDs are unchanged.

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

W2 restores native parallel release for hedged isolated leverage, replacing
W1's serial interim. Every other job keeps W1's serial Plan unchanged.

- **Job split at start.** `start_job` plans as in W1. A job whose planned
  steps are exactly the two hedged leverage legs (`job.join`) admits a declared
  Plan instead of a serial one:
  `admit_plan_with_barriers(preflight, [StepBound{wait_ms: 0, calls: [long,
short]}], PlanPolicy{independent_calls: true, Mutation, Primary, pacing: None,
retry: Never}, [MutationBarrierBound{step: 0}])`. `independent_calls` is set on
  this job only (hcmq3f P2): with it off, the engine seals an unproven 408 or
  5xx at once (`independent_plan_failure` is None) and retires the sibling,
  which cannot keep native's wait-both join. The engine test
  `default_plan_keeps_direct_abort_without_a_native_failure_callback` pins
  exactly that. Every other job keeps W1's serial Plan. Each call is
  `CallBound{timeout_ms: 20_000, attempts: 1, retry_window: FreshAttempt,
after_refusal: []}`.
- **Issue.** From entry custody, take the Plan's two slots in declaration order
  (long, then short), make one `plan_leg_context` each and queue both with
  `request_signed_managed`. The engine prepares and signs both before releasing
  either. Store effects are unchanged: a setting has none before commit.
- **Pure release hook.** BloFin implements `prepare_group_http_release`,
  returning `Unscheduled` for its two set-leverage legs and refusing anything
  else. It touches no state. Its single-request `http_release` stays the default.
- **Barrier behaviour.** If either leg's preparation, signing or deadline fails
  before the barrier opens, there is zero HTTP and the command is NotSent,
  including a refusal on short after long prepared. That's native's "either
  header fails, neither is sent".
- **Join.** Each leg's answer records exactly one fact through W1's
  `write_answer` Setting arm, then `finish_plan_call`. The venue counts settled
  legs and keeps the first error. When both have settled: if there's no error,
  `done_plan` and commit; otherwise `fail_plan` with the first error, no commit,
  and native's join-failure `refresh_account`. A released 408 or 5xx on either
  leg is Unknown by the journal. One 503 plus one Accepted is Unknown with
  long's effect kept and no commit. A refusal of one leg after the other was
  Accepted projects to outcome None, as native's `succeeded` edit did.
- **Retirement.** Unchanged from W1: the engine-sealed join moves to
  `retired`, restores, reconciles and refreshes the account.
- **Join versus seal (dazzy3 P1, fe7lgm P1, 4yjlw0 2, hcmq3f P2).** Under
  `independent_calls`, an engine-recorded failed delivery (released 408 or 5xx,
  transport or deadline) reaches the venue as typed `on_http_failed`. The venue
  records no second fact, calls `finish_plan_failed_call` for that slot, and
  counts the leg settled. Normal DATA records one Setting fact, then
  `finish_plan_call`. Each slot settles once; a duplicate response or failure
  changes nothing. The final `fail_plan` or `done_plan` comes only after both
  slots settle, unless the engine already hard-retired the owner (root expiry),
  which goes through W1's retirement path and advances the FIFO.
  - Either order of one 503 and one Accepted: after the first answer there is
    no terminal and no refresh, and the sibling is still current. After the
    second, there is exactly one Unknown terminal, no commit and one account
    refresh. That matches native's wait-both join, so there's no timing
    difference to record.
- Pre-commit group abort (a preparation, signing or deadline failure before the
  barrier opens) goes through the shared NotSent owner-retirement path and never
  waits for a second callback that cannot come.
- **Which error is reported (fe7lgm P3, 4yjlw0 1).** Native's `finish_join`
  overwrites `joined.error` on each failing leg, so it reports the LAST failing
  leg's text, kept Unknown if any leg was unproven. W2 keeps that rule (the
  last failing fact by arrival), per fe7lgm's "pin whichever native does".
  hcmq3f's review wrote "first error"; that point is returned to hcmq3f. The
  outcome and kind are the journal's projection, never the venue's.
- **Outcome of one refusal plus one Accepted (fe7lgm P2).** This must be the
  journal's projection through `fail_plan`. The engine's `failure_projection`
  gives None when a leg was released, none is uncertain and not all were
  rejected. The RED confirms the observed value at source time; if it isn't
  None, it goes back to fe7lgm rather than shaping the venue error.
- **Commit gate (4yjlw0 3).** Commit only after `done_plan` returns Ok. An
  expired call at join time gives one Timeout terminal and no commit.
- **Settled includes failures (4yjlw0 5).** A leg whose released call fails by
  transport or deadline (`on_http_failed`, `finish_plan_failed_call`) counts as
  settled, so the join never waits on it.
- **Synchronous queue refusal (dazzy3 P2).** If `request_signed_managed`
  refuses short after long was queued, the venue ends the Plan once
  (`fail_plan`). Long's preparation is cancelled with zero HTTP, and the
  outcome is NotSent.
- **Wake custody (dazzy3 P3).** A join queued behind another job admits from
  `on_http_preflight_ready`: `admit_plan_with_barriers` and both
  `plan_leg_context` calls run in that ready custody, exactly as from entry.
- **Feature closure.** BloFin's venue crate links `http-mutation-barriers`, and
  the BloFin WASM and native features forward the host barrier support
  (`hosts/wasm`'s `blofin` feature and the native host's barrier feature). The
  combined bundle therefore links barriers too. A host without group support
  refuses the group before binding, with no individual fallback; the JS
  helper already refuses missing group exports.
- **Removed in W2.** W1's serial interim for the join, and its CALLERS interim
  note, replaced by a restoration note.

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
- a late or duplicate response after a terminal changes nothing;
- W1 serial hedged join: a long 503 or Unknown means short is never prepared and
  the command seals Unknown; a long refusal means short is never prepared and
  Rejected; long Accepted then short refused or 503 keeps long's accepted
  effect in the journal (never NotSent, no commit); both Accepted commits once.
- half-applied join (long Accepted, then short refused, or short 503): the terminal
  keeps long's effect (outcome None, or Unknown for the 503), applies no hedged
  setting commit, and runs native's join-failure `refresh_account` (one RED
  per short outcome);
- re-plan flip: a SetLeverage queued behind another job is planned at its
  preflight-ready wake against the current hedged/isolated state, so a flip
  while queued never issues a stale one- or two-leg plan.

W2 (each fails on W1's serial interim):

- both legs are prepared before either is released, and both are in flight
  before either answers;
- either leg's preparation failure gives zero HTTP and NotSent, including a
  refusal on the second leg after the first leg prepared (an injected signing
  refusal for short);
- both succeed and commit once;
- one 503 plus one Accepted gives Unknown with long's effect kept and no commit;
- one documented refusal plus one Accepted gives outcome None, no commit and the
  account refresh;
- both refused gives Rejected;
- 503 ordering (hcmq3f P2): in both orders, after the first answer there is no
  terminal and no refresh and the sibling is still current; after the second,
  exactly one Unknown terminal, no commit and one refresh;
- a duplicate response or a duplicate failure for a settled leg changes
  nothing, and root expiry retires the job once and the FIFO proceeds;
- reported error: a short refusal before long's Accepted, and after it, gives
  outcome None and the refusal's text and kind. Both refused, in both orders,
  reports the last refusal's text;
- a short transport loss after long's Accepted gives Unknown, no commit, and the
  refresh (a failure counts as settled);
- both legs released and neither answering until the call bound gives one
  Timeout terminal and no commit;
- pre-barrier (4yjlw0 4, with a parked provider for short's signature): the root
  deadline passing gives zero HTTP and one Timeout/NotSent terminal with no
  refresh, and owner retirement gives zero HTTP and one terminal. The
  parked-provider rig lands with the source;
- wake re-plan flip (4yjlw0 6): a SetLeverage queued while net or cross that is
  hedged and isolated at its wake takes the barrier Plan, with both legs in
  flight;
- dazzy3 P2: a synchronous queue refusal of short after long was queued gives
  zero HTTP, one NotSent terminal and long's preparation cancelled. The
  fixture lever is chosen at source time; if none can reach it, this becomes
  a reviewed source pin;
- dazzy3 P3: a join queued behind a placement admits both legs from its ready
  wake, with both in flight before either answers;
- a host without group support binds nothing and sends nothing (the
  isolated native proof, both host modes).
- W1's serial-interim tests (`the_hedged_join_sends_short_only_after_long_is_accepted`,
  the long-503 and long-refusal "short never prepared" REDs, and the
  `trading.rs` interim fixture) are restored to native parallel assertions. That
  is the documented interim ending, not a weakened assertion.

W2 size: BloFin is the first shipping adopter of `http-mutation-barriers`, so
the family and combined deltas are measured on the composed tip. The feature
reaches the combined bundle only through feature unification, so standalone
Bybit and Bitget are measured too and must be unchanged (dazzy3 P4). Bybit's
diagnostic barrier-on build was about +20K, and the post-S1 +30K combined bar
still applies.

Existing trading fixtures keep their assertions; only how they drive the engine
changes, as in #2143.

## Delivery

One PR per phase on `feat/rust-blofin-write-funnel` (W2 on its own branch).
Each lands RED first, then source with the exact inventory prune (W1: 8
occurrences, W2: 2) and a CALLERS note for D-a to D-e. Local gates: the BloFin
suite, clippy `-D warnings`, fmt, `check-bans.sh`, and the paired size. Then one
push and one PR run.
