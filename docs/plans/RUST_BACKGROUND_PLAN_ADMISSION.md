# Generation-bound background Plan admission

Plan-first G1 checkpoint, 2026-10-24. Owner codex-zs5e83 (handover from
codex-hcmq3f); first consumer is the BloFin custody cutover, native source review codex-9czdjh;
Plan API fit codex-uufub4, session cascade fit claude-55ao7t. Base is actual
`a828ede7ebb54bc9c032ea61365213abac5e3dca`. This phase adds docs and isolated
original-source tests only. No runtime API or whole-family readiness is claimed.

## Native contract and smallest consumer

BloFin S3-S6 is one production custody cutover. The first proof consumer is two
serial signed settings reads, margin-mode then position-mode, bound to the
account generation. This is a small proof shape; the existing native independent
startup reads need not become serial in the production conversion. Each call
gets an explicit 20,000 ms bound. The account lifetime has no deadline.

The full account geometry, source checked against the a828 landing and the
pinned RUST_BLOFIN_DISPATCH_SIGNER.md on feat/rust-blofin-signer-migration:

- Positions then balance share one busy cycle, with their existing separate
  revision/publication fences. Settings publish only under settings_revision;
  both loaded settings gate bootstrap. Native permanent-key checks and the
  two-answer 152409 rule remain codec/auth evidence, not transport refusal.
- Orders are one aggregate pending -> tpsl -> algo traversal, one final Store,
  source and reconciliation publication gated on orders_revision. Every nonempty
  page continues, including short pages. Each family has at most 50 responses;
  unchanged cursor or a nonempty page 49 fails. Three families mean at most
  150 calls per traversal. A successful revision-stale census still performs
  its existing startup-prefix/queued-execution release, independently of Store
  publication; do not silently discard that native side effect.
- Three whole-traversal attempts have 1 s / 2 s shared delays; a failed attempt's
  rows are discarded. Polling, startup retries and stale-refresh scheduling have
  no total lifetime. Native 418/429 pauses and ordinary poll/debounce intervals
  stay declared through the shared scheduler, never a new venue retry loop.
- Secondary serves private reads when configured, else Primary. An absent
  Secondary passphrase inherits Primary; an explicit empty value refuses.
  Identity/slot/egress is selected once, never from the HTTP method.

Bitunix is a second consumer: startup balance/positions, ordinary orders then
TPSL as one commit pair, 500 ms balance ticks, a full refresh every 32 ticks,
unbounded retry and the native 10 s stall policy. Its committed inventory/geometry
pin will be added by codex-9czdjh before native adoption; no Bitunix API is assumed.

## Shared lifetime and proposed API

Do not introduce a second account-lifetime registry. Session core #1832 reviewed
`7e86cea72e0` can anchor the account generation, with its actual landing integrated
before runtime proof. Opening this anchor is I/O-free: no login, listen key,
session token or #1842 prerequisite wait is required for a no-session venue.
#1842 reviewed `b6b9e3d698c` applies only when a native prerequisite really exists.
The public name BackgroundGeneration is a typed read-lifetime view over that
existing Session anchor, not another numeric generation counter. Its exact Rust
wrapper/constructor is pending session/core API fit. No deadline or dummy Context
is minted for the lifetime. Construct the view only through validate_session
(exact issuer and live generation), never from a raw generation. I/O-free
open_session(None, None) still consumes one of the shared 64 Session slots;
retire before reopen on a configuration change.

Proposed spelling, not an available production API:

```rust
// Register an I/O-free task under the account lifetime; shared scheduler wakes it.
cx.register_background_task(&account_generation, task_policy) -> Result<BackgroundTask, Error>
// Inside exact on_background_task_ready(task, activation), consume its one-use loan.
cx.admit_background_plan(&activation, declaration) -> Result<Plan, Error>
cx.finish_background_plan_after_data(&plan, &slot, &delivery) -> Result<(), Error>
cx.fail_background_plan(&plan, error) -> Result<(), Error>
```

The declaration uses the SAME internal Plan admission transaction, bounds,
scheduler, slot checks and Prepare/Bind/Claim path as command Plans. It fixes
Read semantics and the child's own immutable selected credential; Mutation is
rejected. The anchor's Session credential governs only Session-owned operations,
not child Plans. A
new retained child Owner::Background generation holds each finite Plan phase,
so settings, orders and account cycles can coexist under one account lifetime.
It is not a Session-owned Context: existing Session completion_delivery excludes
that owner, and session mutation rules must not acquire command journal rights.

Every child records its exact issuer and parent session generation. Handle,
slot, delivery, ready callback, retry admission, Prepare, Bind and Claim validate
both the child and the live parent. Registration creates an engine-issued task
bound to the parent and its own task generation, with declared startup, poll and
retry policy. The shared scheduler turns a legitimate initial demand, poll,
invalidation or eligible retry into an opaque BackgroundActivation. Only the
exact on_background_task_ready callback holds its loan. Admission validates the
issuer, parent, task, activation and still-live demand before IDs; all fallible
preparation precedes the one-use activation consumption and admission commit.
Failure cannot leave half-consumed custody or resources. Only a successful
admission commit consumes an attempt. The active one-use demand/loan is cleared
before entering the callback, with its private validation record retained, so
reentrant or foreign callbacks cannot claim it. Selecting a leg without submitting
it is not progress. No progress retires
that activation through the shared typed failure path, without a command notice.

A live handle or delivery=None is NOT admission custody. A command callback,
another Plan's delivery and unrelated or stale timers cannot consume the loan.
Existing generic readiness checks need the explicit background-activation case;
no #1896 source stack is needed for the plan/baseline tests, but its foreign-owner
and socket-tail guards must compose at integration. Background readiness never
borrows command/FIFO activation. No background activation method is claimed to
exist on a828 or the Session parent pin.

Every child carries its OWN immutable Selection/Identity/egress binding from the
shared helper, independent of the lifetime anchor's Credential. Bitget can read
positions/orders on Secondary while a declared degraded balance child uses
Primary; BloFin can read on Secondary while other generation work uses Primary.
This never calls the Session-owned-context credential admission path to change
that Session credential. The generation anchors a trusted configured-identity
snapshot; selection resolves a declared per-child read-policy variant once at
admission. Changed configuration (including Secondary added/removed) retires and
reopens the generation. No runtime override or token swap is permitted.

Children retain their selected egress, including Secondary, rather than inherit
the Session's Primary route. The Selection helper is a typed placeholder until
codex-9czdjh freezes its exact signature, with claude-4yjlw0 reviewing. Today's
Selection/Credential does not itself prove subject/config-generation provenance;
that trusted snapshot and revocation contract remains an explicit shared gate. No slot_for(method), raw key carrier or
second account-lifetime registry is introduced.

Session live is not JWT/login ready. A generic task prerequisite needs an
explicit task-readiness bridge; no-Context task activation cannot borrow #1842's
Context wait. BloFin's I/O-free anchor does not require that bridge.

Session prerequisites finish BEFORE Plan admission. The #1842 gate accepts only
Context-phase owners, so an admitted child Plan must be refused by that gate. This needs an explicit
Background-owner phase arm: #1842 currently returns context_phase=true for every
non-Command owner, and hold_wait only holds Commands. Pin the new Plan-phase
Background refusal with the same oracle as a_planned_command_is_refused_a_session_wait_instead_of_parking.
If a future Context-phase child can wait, its prune hold must also be declared;
this phase does not grant that path.

## Finite traversal attempts, not 450 reserved calls

First-consumer review chose one separately admitted traversal per attempt. Each
Plan declares at most 150 uniform serial "next native request" slots, each
with one physical attempt and no per-call fallback, at 20 s per call. A family
transition consumes the next generic slot; no fixed Pending/TPSL/Algo blocks are
skipped. Exact Read early completion retires the whole remainder ONLY after the
entire three-family census, never just after one family's final page. A full traversal
fits the current 256-attempt capacity; 3 * 150 does not. Do not lower native page
or retry limits, increase a write journal budget, or add credit recycling.

Compose with the existing shared read scheduler/backoff as a logical task:

1. Admission captures a bounded whole-traversal attempt policy (three attempts,
   delays 1 s / 2 s) and an immutable Read-only declaration template. The task
   carries retry scheduling metadata, not a deadline-bearing Context or an
   immortal Plan. The task, its outstanding demand, shared scheduler registration,
   backoff wait and timer are registered under the exact parent Session even
   between attempts when there is no child Plan. Its typed bridge is part of G1.
2. Failure retires the exact attempt. Only AFTER all its HTTP, signing and
   cleanup retire does the shared scheduler admit a fresh Plan generation for
   the next attempt. The 1 s / 2 s wait is scheduler-owned. Old replies cannot
   advance or publish into that Plan. The native callback resets attempt-local
   page buffers, cursor/source rows and family progress before its first send.
3. No failed-attempt page enters the successful census. Only the successful
   attempt's exact current DATA may perform the one final native publication.
   Native revision/census rules remain separate from transport validity.
4. Three is a maximum, not an unconditional retry sequence. BloFin retries only
   when attempt < 3 AND auth_failures == 0. Typed native/auth retry eligibility
   can stop the task; provider refusal is not auth evidence. Auth-delayed ordinary
   polling is separate. Do not retain substring classification or introduce a
   second transport policy. The shared bridge holds the actual admission count:
   read_scheduler::RetryPolicy.max_steps caps backoff growth, not attempt count.
5. Attempt limits are held by the shared task across fresh Plans, not reset by
   each Plan. After three failed traversals, retire that task; a later genuine
   poll/invalidation is a new cycle. Startup's indefinite retry and ordinary
   polling remain shared scheduling across bounded cycles, never root renewal.
   The existing scheduler's automatic replacement of expired active tokens must
   be fenced until the old Plan's exact cleanup/retirement completes; it cannot
   reserve the next attempt early.

Each Plan keeps the existing derived immutable root (at most 150 * 20 s for the
full serial geometry); there is no extra arbitrary account/aggregate lifetime
bound, and no root renewal. The 20 s physical cap starts at leg selection/
admission before Prepare/Bind, never at transport release. A stuck host therefore
expires through the shared preparation timer. Queueing, signing, pacing and
reprepare cannot extend either bound. This clarifies the consumer's "no whole-
lifetime deadline": the current Plan API still has its derived root.

Read-only cycle replacement gives no permission to reset write evidence or
resend an uncertain mutation. Mixed/write Plans cannot use this retry bridge.
There is no parent-series deadline/accounting primitive or fairness claim.

## Retirement is an explicit behavior change

Native reject_startup clears retry/poll/socket work but DOES NOT retire pending
HTTP, busy state or debounce. G1 changes that deliberately. retire_session first
marks the generation dead and invalidates every registered task, demand, backoff
wait/timer and child before cleanup; then it drains child preparations, provider work, requests and owned timers through the existing
cancellation/retirement machinery. No I/O can release between invalidation and
cleanup. Retirement during the 1 s / 2 s gap must cancel task/wait state even
with no admitted Plan. It preserves unrelated scheduler group pauses. Late responses, duplicate receipts, old timer events and same-ID reuse
cannot reach Store reducers, auth observers, retry scheduling or publication.

Already released HTTP may have reached the venue; cancellation is not an unsent
claim. These are Read cycles: no command outcome, command terminal, order write
journal or Accepted fact is fabricated. discard_managed is silent today and cannot clear native busy state. Queue a
typed retired-child notification to the venue, without success/DATA/send rights,
so cleanup clears only the matching native busy/attempt state exactly once.
The parent-dead checks must still permit private failure-only seal/cleanup in
the common funnel; retirement cannot strand a child merely because its parent
is already dead. Panic uses the existing sticky poison;
no successor activates before predecessor cleanup has completed.

The parent->child cascade does NOT exist in #1832 today. Implement it at the
session registry choke point, with claude-55ao7t reviewing that hook. A separate
child owner cannot rely on today's owner==Session retirement filter. Do not
edit the frozen parent PR to obtain the hook. Validate parent liveness once in
validate_handle; inherited validate_send and managed_response_live then fence
queue/Prepare/Sign/Bind/Claim/reprepare/DATA without new per-site checks. Replace
retire_http_owner's equality-only lookup with an indexed parent-child cascade.
These hooks collide with #1897 session_context.rs/request_context_registry.rs;
runtime must use the combo-5 landing or coordinate order with claude-55ao7t.

## Source scope and implementation sequence

Own new runtime modules: request_context_background_plan.rs and, if required,
request_context_background_retry.rs. Own test file: tests/background_plan.rs.
The runtime integration touches owner-generic Plan entry/seal, background owner
lookup, managed release validation, session retirement and shared scheduling.
Those hooks overlap #1919/#1939, #1832/#1842/#1833 and #1896. They stay untouched
in this plan/RED phase; coordinate exact hooks before runtime edits. Reuse the
one central prepare/commit seal funnel; no parallel cleanup/projection path.

1. Commit this inventory/API plan and README row, then consumer/core review.
2. Add parent-compilable consumer REDs for missing structural/generation
   custody, plus working command-entry denial and Context-deadline controls.
   The baseline may use the current Context-only path, but not an arbitrarily
   short 20 s root: that API can manually span two 20 s legs with a 40 s parent.
   Pin missing declared credits, task activation or retirement fencing instead.
   Upgrade only fixture admission/selection to the reviewed API later, retaining
   transport/assertion oracles. A missing method compile error, fake command,
   new venue counter or a deliberately bad deadline is not RED evidence.
3. Implement the reviewed lifetime/admission/cascade hooks and shared retry
   composition; turn those assertions green, with real Prepare/Bind/Claim.
4. Test native first consumers; prune only real native conversions in their own
   venue PRs. This engine slice changes no legacy inventory entry.

Required controls: two 20 s serial windows under one derived bound; separate
simultaneous cycles; wrong issuer/parent/generation/slot/delivery; retirement in
Prepare/Sign/Bind/released phases and between-attempt backoff; stale task wake
after reopen; auth eligibility stops retries; late receipts/replies/timers and >256 repeated
cycles without identity reuse/resource leak; no command notice; released Read failure without a money verdict (a local
pre-send NotSent remains a truthful phase fact); the 150-page capacity, short/nonempty/page49/cursor native
rules; three attempts with exact retirement before shared delays; failure pages
never carry; one publication and revision-stale census side effects retained.

Gates: focused engine tests with http-plans/test-support, feature-off compile,
scoped strict Clippy/fmt, BloFin+Binance native checks on adoption, both ratchets
and bans. Default-off http-plans must exclude all new storage/callbacks and
feature-off builds retain behavior. Pin the O(1) empty task/child-index path.
Full WASM/size/replay/CPU belongs to exact-head CI. Independent source,
caller and full-map reviews answer the architecture gate. No merge without GO.

## Caller contract

Account retirement now cancels and fences pending child reads and their retries;
legacy late callbacks that used to survive reject_startup do not publish. There
is no financial outcome or synthetic command terminal. Startup/polling remains
unbounded across finite cycles; native census publication and retry/page limits
are preserved. Architecture gate: shared background custody and scheduling,
not a venue-local lifetime, retry or signing implementation.

## Review checkpoint

Consumer claude-4yjlw0 cleared bdf688's geometry and timing. Core codex-uufub4
bounded API fit cleared the max150 one-attempt traversal with task-owned max3
admissions/1s/2s across full retirements, not a recycled450-call envelope. Session
owner claude-55ao7t approved the lifetime shape and identified the explicit gate,
issuer, parent cascade, 64-anchor capacity and cleanup notification requirements
above. These are design reviews, not runtime clearance or a frozen API claim.

## Original-source controls

On unchanged a828 source, background_plan compiles and proves two Context legs
can span real 20 s windows under a 40 s parent. Genuine command Preflight is
refused in start and unrelated timer callbacks, without any synthetic terminal.
Two assertions nevertheless fail through real Prepare/Bind/Claim: the unrelated
timer can borrow a live background handle to release extra HTTP, and a completed
two-call cycle retains authority to release a third call. This is evidence of
the missing finite Plan/task custody, not a defect claim against the documented
legacy Context API. Fixture admission/selection will move to G1 while keeping
those wire assertions; no venue counter or fabricated command enforces them.

These two original REDs are ignored under codex-zs5e83's G1 runtime adoption.
Session cascade, the #1842 non-Command phase hole and retired-child cleanup
notification need additional controls on the integrated Session API, which is
not present on this baseline. A missing method/compile failure is not counted.

## Runtime takeover and composition checkpoint

Handover is exact `0a014330647b97f2b6c3e06b4b5bbc1b90e09336`:
plan `bdf688ceb30`, Session refinements `fd6e2037bb0`, and the original tests.
The actual landing `a2533f5761f21afab4bc9037cfa6a187029b63e4` is genuinely
integrated at `0a89fe77910`; only the README index needed keep-both resolution.
On this tree, the unchanged `background_plan --include-ignored` target gives
one working control and two intended assertion failures, zero ignored. Both
fail at the original extra-HTTP assertions through Prepare/Bind/Claim.

Plan owner claude-fe7lgm directed composition on reviewed #1919
`2f4a08b449cb69316cf9d657f7c12f9e696c50a0`. That dependency is integrated
at `35653ec28b8`, with no engine source conflict; the CALLERS conflict retains
the actual landing's selected-root section followed by the dependency's
retained-readiness section. This is a reviewed dependency stack, not an actual
#1919 landing or G1 source clearance. Reintegrate actual prerequisites before
final CI and any merge window.

### Shared implementation boundary

The next source review must cover these exact changes; names below remain
proposed until the Plan and Session owners clear this checkpoint.

| Path | G1 hook and preservation boundary |
| --- | --- |
| `engine/src/request_context_background_plan.rs` (new) | Bounded task declarations, issuer-bound task/activation handles, parent/task/child indexes and activation state. No second Session registry. |
| `engine/src/request_context_background_retry.rs` (new, if needed) | Shared task scheduling and bounded traversal admissions, composed with `read_scheduler`; no venue callback owns a retry timer. |
| `engine/src/request_context_planned.rs` | Factor declaration/resource preparation from the command-specific origin and commit. Background admission uses that same transaction, Schedule, slot/attempt reservation, journal and prepare/commit terminal funnel. |
| `engine/src/request_context_registry.rs` | Resolve registered Plan-backed `Owner::Background` through the existing physical owner table. Preserve plain legacy Background owners. Fence live parent/task in handle and Plan lookup, and count physical pending/delivery retirement. Preserve LocalSetting and tracked command-root paths. |
| `engine/src/request_context_preflight_call.rs` | Include Plan-backed Background owners in owned admission, delivery/failure bookkeeping and legacy-API denial. A generic `Scope::Background` issuance cannot attach to the child owner. |
| `engine/src/request_context_owner.rs`, `request_context.rs` | Opaque task/activation declaration and exports behind `http-plans`; no public Preflight constructor or fake command founder. |
| `engine/src/session_context.rs` | Validate issuer, index registrations under the exact Session, invalidate before cascade and refuse Session waits on a Background Plan. Keep ordinary Session credential/egress and all ready-callback guards. |
| `engine/src/engine.rs` | Scoped activation callback, shared task wake, typed child retirement and poison handling at existing event/retirement boundaries; feature-off has no task state/callbacks. |
| `engine/src/read_scheduler.rs` | Narrow checked retirement barrier if the existing scheduler cannot retain an active task until physical cleanup; preserve ordinary scheduler behavior. |
| `engine/tests/background_plan.rs` and dedicated shared targets | Retain original transport assertions; change admission/selection fixture only after the reviewed API exists. Add capacity, Session cascade, retry and custody controls. |

The existing physical table is reused for Background Plan phase/accounting;
task entries contain only their parent binding, immutable declaration/selection,
bounded cycle count, scheduling state and optional current child. Task, parent
and child indexes use bounded vectors rather than another owner-keyed map.
Capacity must bound live task registrations and retiring children, including
backoff intervals without a child. No child can be replaced while its previous
generation still holds pending delivery or cleanup custody.

`validate_handle` alone is insufficient. `plan_body`, Plan readiness and slot
selection do not necessarily pass through Context validation. Their common
lookup must also reject a dead parent or retired task. A separate private
failure-only lookup permits preparation and commitment of retirement after
the parent is dead, without restoring ordinary validation, success or send
rights. The existing `prepare_owner_retirement` and physical retirement funnel
remain responsible for exact pending/delivery cleanup and one typed callback.

Command-only assumptions that must be explicitly changed include
`validate_legacy`, `owned_admission`, `register_attempt`, `context_phase`,
`physical_owner`, owner entry lookup, Plan terminal dispatch and physical
retirement bookkeeping. Plain legacy Background Reads keep their existing API;
only registered Plan-backed Background owners take the owned path. There is
no command notice, observer group or synthetic command receipt. A Background
owner never satisfies command-ready founder custody and never uses a capped
caller-receipt admission mode.

Background pending accounting must change atomically with owner lookup:
`register_attempt` currently skips all Background Reads while `retire_attempt`
decrements retained owners. Letting only the latter resolve a new physical
Background owner would underflow its pending count on first retirement.

### Admission, callback and retirement transactions

Registration validates the exact live Session issuer before reserving task
storage or IDs. It captures a checked owned declaration template and immutable
selected identity/slot/egress once. The current `SelectedRead` and `Selection`
types do not prove trusted setup/config-generation provenance: G2c remains an
explicit coordination gate, not a claim that constructing Slots solves it.
Neither the anchor's primary credential nor an HTTP method selects the child.

The activation contains no public constructor, Clone, numeric scope, Context or
Preflight. The engine removes scheduled demand before entering the exact ready
callback and retains a private issuer/task/activation validation record. All
declaration checks, deadline arithmetic, context and physical-owner capacity,
task/child/retirement index reservations, timer/action storage and HTTP/signing
ID availability precede consuming that record and incrementing the task's
successful-admission count. Failure before commit issues no Plan and consumes
no traversal attempt. Selecting a call without submitting it is still no
progress; the scoped callback guard retires it with a typed failure.

Admission one, two and three each create a fresh finite Read Plan with at most
150 one-attempt calls, not a 450-credit envelope. Completion or failure first
seals the current Plan through the common terminal funnel; all physical work
and the typed native cleanup retire before task scheduling can proceed. Only
then may an eligible failure schedule the declared 1s/2s delay. Task-side
admission count is independent of `RetryPolicy::max_steps`. The scheduler's
current automatic expiry replacement must not run ahead of this barrier.
The same barrier applies to `pause`, `superseded`, `cancel` and `retire`, which
currently discard active scheduler tokens. In particular, a pause during
Prepare/Sign/Bind must invalidate admission without freeing replacement demand
before child cleanup. Add an externally retired token state and a private
exact-token cleanup acknowledgement usable after its deadline; ordinary
`finish` refuses expired tokens and cannot serve as that acknowledgement.
Exact 1s/2s policy uses the shared scheduler's identity sampling, not randomized
delay. Retry eligibility is checked again at activation after the wait.
After the third failure or an ineligible auth/native failure, the cycle ends;
a later genuine poll/invalidation may start another cycle. It cannot turn an
old activation into a fourth attempt or reset Mutation evidence.

Session retirement first makes the parent/task/activation bindings dead, then
drains only indexed matching children and task waits through shared retirement.
The task remains fenced during cleanup and no successor can activate until
that cleanup completes. A parent-dead child can fail/retire but never publish
DATA or succeed. Panic keeps the existing sticky poison. The empty-index path
must return without scanning legacy Contexts or all live Session entries.

### Verification before source freeze

Besides retaining the two original wire REDs and 40s/two-20s control, test:

- issuer/parent/task/activation substitution, replay and unrelated callbacks;
  admitted but unsubmitted selection; fourth-attempt denial;
- atomic capacity/ID refusal before activation consumption, including another
  active child, retiring child and the 150-call reservation boundary;
- parent retirement at Prepare, parked Sign, Bind, Claim, released response and
  between-attempt backoff; no release/publication after invalidation;
- strict Background Plan refusal at `session_wait`, command-ready and legacy
  completion/admission paths, with plain Background positive controls;
- full cleanup before 1s/2s retries, auth eligibility, accepted-page isolation,
  exact one typed retirement, no command terminal and no successor on panic;
- more than 256 completed cycles with no resource/identity reuse leak,
  simultaneous independent children, and an O(1) idle-index counter;
- feature-off compilation plus `http-plans,test-support` and order-locks
  combinations, focused Session/Plan/readiness/scheduler controls and strict
  touched-crate Clippy/fmt. Replay, WASM size and timing remain CI gates.

Source/API audit is required from claude-fe7lgm and codex-uufub4, with the
Session cascade hook checked by claude-55ao7t. The BloFin caller review remains
codex-9czdjh plus HMAC owner claude-4yjlw0. No BloFin production, inventory prune,
G2 mixed-leg readiness or G2c provenance clearance is implied by this checkpoint.

Independent planning audits found the parent-lookup and legacy-phase bypasses,
Background pending-accounting mismatch, and scheduler pause/expiry cleanup
barriers above. Those findings are accepted into this plan before source.
Global/per-parent task, wake and cleanup capacities and the exact G2c identity
snapshot API still require owner review; existing Session open and scheduler
register calls do not by themselves prove the new pre-ID atomicity contract.

Plan-owner review of `4a219db9602` against exact #1919 `2f4a08b449c` is
direction-CLEAR. The following explicit requirements are part of implementation:

- The shared physical table is currently keyed by a bare generation. Store or
  check the full Owner variant in every lookup, including owner entry, mutable
  owner entry, `physical_owner`, index membership and finish. Even a matching
  numeric generation must not let Background resolve Execution or the reverse.
- Terminal preparation has three owner cases. Background takes no execution
  sweep or observer fan-out, but does reserve cleanup through the same
  `prepare_owner_retirement` transaction. Commit and `finish_physical` must
  support its Planned-to-Retiring transition, release reserved Context credits,
  remove the Plan index and enqueue the exact typed retirement token.
- `physical_plan_timer` must find Background Plan timers so retirement clears
  them. `founded_by` stays Execution-only; Background never inherits a ready
  command's loan. No capped Preflight can enter Background admission.
- Pending registration and retirement change together. Use bounded vectors
  and `assert!(a == b)` on handles to avoid unnecessary WASM formatting cost.

Task/wake capacity and G2c snapshot fit remain for uufub4 and the Session owner;
the Plan-owner review does not clear those contracts. The final #1919 frozen
pin is still to be announced after its pending CPU certification job.

The follow-up geometry review by codex-uufub4 is CLEAR: at most 256 total
registered tasks including retiring/backoff/cleanup state, under the existing
64 Sessions. A parent may occupy the whole global task pool: this introduces no
per-parent fairness promise, and a capacity test must pin that choice. Each task
has at most one demand, child, activation and cleanup. Registration promises no
Plan credits; active and reserved slots still share the one 256-credit pool.
The initial template is uniform serial Read/Never, 1..150 calls at 20s, with
at most three successful traversal admissions and exact 1s/2s retry sampling.
SelectedRead is captured once; G2c remains a separate native-adoption gate.

Session-owner review APPROVES the hook shape with two required controls:
the new activation marker participates in `session_scope` before any Session
operation consumes resources, with the existing eight-variant readiness attack
matrix; and a child of a private-egress anchor must use its own selected egress,
never `Egress::Session`. Children use neither `session_context` nor
`session_scoped_context`. In `retire_session`, the indexed cascade begins after
`sessions.retire(generation)`, when the parent is already dead, and queues typed
cleanup without calling the venue inside Cx. Plain Background waits stay valid;
Plan-backed Background waits are refused without changing hold accounting.

These are plan/geometry reviews, not runtime acceptance. Send the exact
constructor/activation/retirement and Session hook patch back to fe7lgm,
uufub4 and claude-55ao7t before source freeze. Their review must also cover
explicit refusal/decline disposition without spinning or stranded demand, and
the original two RED assertions must be active after fixture conversion.
