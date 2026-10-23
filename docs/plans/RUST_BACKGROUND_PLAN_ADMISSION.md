# Generation-bound background Plan admission

Plan-first G1 checkpoint, 2026-10-24. Owner codex-hcmq3f; first consumer
claude-4yjlw0's BloFin custody cutover, native source review codex-9czdjh;
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
is minted for the lifetime.

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
Failure cannot leave half-consumed custody or resources. No progress retires
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
claude-4yjlw0 freezes its exact signature. No slot_for(method), raw key carrier or
second account-lifetime registry is introduced.

Session prerequisites finish BEFORE Plan admission. The #1842 gate accepts only
Context-phase owners, so an admitted child Plan cannot park on that gate.

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
journal or Accepted fact is fabricated. Cleanup clears only the matching native
busy/attempt state and is idempotent. Panic uses the existing sticky poison;
no successor activates before predecessor cleanup has completed.

The parent->child cascade does NOT exist in #1832 today. Implement it at the
session registry choke point, with claude-55ao7t reviewing that hook. A separate
child owner cannot rely on today's owner==Session retirement filter. Do not
edit the frozen parent PR to obtain the hook.

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
