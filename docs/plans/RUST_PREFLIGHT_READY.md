# Retained Preflight activation custody

Owner: codex-hcmq3f. API fit: codex-uufub4. Planned-execution composition:
claude-fe7lgm. Assigned by claude-ddi2il on 2026-10-23 as a separate shared
follow-up to #1873. This is a plan, not an available API or adoption claim.

## Reviewed base and problem

Exact parent: canonical reviewed #1873 source/API
`0f06f289fe492a95ab211dba839e3d61bb405dbe`. The feature-isolation WIP is not
a reviewed pin. This follow-up remains behind the shared `http-plans` feature
once that boundary is reviewed: no readiness storage, scan, allocation or
callback for feature-off families. Re-integrate the reviewed/actual core
before final proof; preserve native zero-Plan-scan and WASM budgets.

ApeX retains native FIFO mutation jobs. While A is waiting for its HTTP reply,
B arrives through B's genuine command callback. B can retain a Preflight, but
it cannot construct its final native Plan until the queue reaches it. Native
`drive_trading` previously starts B inside A's HTTP callback. Neither that
foreign DATA custody nor an ordinary venue timer grants B plan/send authority.
The Plan owner confirmed this composition gap is outside reviewed #1873.

Do not lend A's Context to B, reborrow entry, mint a synthetic command, defer
an unowned numeric ID, or make a timer count as command admission. Engine
readiness must reactivate B's exact retained generation after A's callback has
unwound. The venue remains responsible for native FIFO order and native command
inputs; the engine supplies custody, not a second trading queue/pacing policy.

Own initial delta: this document and its README row. Proposed runtime files:
`engine/src/request_context_preflight_ready.rs` and its `_tests.rs`, with
minimal coordinated wiring in `engine.rs`, `request_context_preflight.rs`,
`request_context_preflight_call.rs`, registry and public types. No edits to
#1873's overlapping source until API/file agreement. The conditional-terminal
follow-up uses its own module. claude-fe7lgm's planned execution and journal
follow-ups use separate modules; their admission must call the SAME internal
Preflight custody predicate rather than implement a second transition.

## Provisional public surface

```rust
cx.park_preflight(&preflight) -> Result<(), Error>;
cx.request_preflight_ready(&preflight) -> Result<(), Error>;

trait Venue {
    fn on_http_preflight_ready(&mut self, cx: &mut Cx<'_, Self::Pending>,
                               preflight: &Preflight) { /* safe default */ }
}
```

`begin_preflight` still consumes B's genuine command-entry loan exactly once.
`park_preflight` is allowed only during that same initial callback, with no
selected/read call, Plan, pending attempt or terminal. It explicitly relinquishes
initial activation custody while retaining the exact issuer/owner/generation,
policy and opaque Preflight. It creates no Context, deadline, timer, ID, write
fact or I/O permission. A venue that can finish/admit immediately need not park.

`request_preflight_ready` accepts only an exact live Parked Preflight. It queues
one wake intent in engine-owned bounded storage; it grants nothing in its caller's
scope and cannot synchronously invoke a venue callback. A may request B's wake
while retiring A; the supplied live B handle identifies which already-admitted
owner to wake, not a newly minted authority. Duplicate wake intents are
idempotent and cannot duplicate terminals or spend resources. Forged, wrong-engine,
retired or replaced-generation handles refuse without mutation. Queued/activating
handles cannot be parked again to create a zero-I/O self-wake loop.

This first seam permits at most one parked activation per Preflight. After that,
progress uses the genuine bounded read delivery or admitted Plan readiness. A
second park/activation cycle requires separately reviewed need and bounds; it is
not silently permitted by this API. Preflight retention has the existing 256-owner
bound and introduces no arbitrary 120-second queue deadline. Waiting does not
mint or renew the eventual native Plan/call deadline.

## Engine event boundary and custody

Reserve the readiness queue capacity fallibly during initial Preflight admission,
before identity commit, bounded by the same retained owners. Park/request cannot
allocate an unbounded side queue, generate IDs or consume Plan slot reservations.
Maintain an O(1) empty-ready guard and a bounded index of queued Preflights; never
scan all Context/execution owners on every public event.

After the outer command/HTTP/timer/signing/readiness callback and its existing
owner-retirement cleanup have completed, the engine drains requested ready
owners in request order. Before each callback, revalidate the exact live parked
phase/generation and absence of outstanding work. Skip a retired generation;
never redirect its notification to a reused numeric command ID. Drain only at
the existing safe event-scope boundary: no recursive `finish_event_scope`, no
callback inside another venue callback, no borrowed outer delivery/entry/order
response token. Preserve/restore unrelated outer state exactly.

For one invocation, install a private engine-owned readiness scope naming ONLY
that Preflight. `on_http_preflight_ready` can then use the normal preflight APIs:
validate native inputs, complete cached ReadOnly work, fail, issue one declared
bounded prerequisite read, finish an explicit empty native mutation, or admit
its final Plan. No operation is granted because a handle happens to be live.
Existing issuer/generation/credential/purpose checks still apply.

One common internal predicate validates Preflight activation for all operations
that consume/transition the owner: exact initial command callback, exact current
finished prerequisite-read handoff, or exact private ready invocation. It is used
by `preflight_read_context`, `done_preflight`, `fail_preflight`,
`done_empty_plan`, `admit_plan` and the future `begin_planned_execution` transaction.
Failure-only timeout/cleanup authority remains explicitly separate and cannot
become send/success rights. Clear initial activation at its event boundary;
`http_delivery == None` on a later unrelated timer is never sufficient proof.
Do not broaden public Context validators or let a Plan/readiness callback operate
on another retained Preflight.

Successful activation must consume the wake with a terminal, a genuine read, or
a Plan transition. A default/unhandled callback is completed as one typed local
Internal/NotSent failure through the shared preflight failure/retirement path,
not left silently stranded. A callback that panics poisons the engine under the
existing callback discipline; scope tokens cannot leak after unwinding. Final
policy/error wording is part of owner API-fit review before implementation.

All recoverable checks and resource reservations precede mutation. A failed
activation/admission does not leave a half-queued owner, renew a bound, spend IDs,
create a retry or borrow another owner's order lock. Retirement removes any
queued wake once, and clears venue state through the exact owner/generation
retirement hook. A later DATA/signing receipt cannot revive a queued command.

## Composition and file ownership

claude-fe7lgm's `begin_planned_execution` uses the same retained-Preflight
transition hook. It atomically admits the Plan, physical execution root and
founder observer; it does not call a public Plan constructor then create a second
owner. Ready custody may be consumed by that single transaction, once its
source is reviewed. Observers cannot request/consume founder activation merely
by sharing execution membership. Their detach/expiry does not donate entry.

Readiness does not import facts, grant resend, resolve uncertainty or alter
journal projection. Conditional mutation completion and ledger import/write-back
remain their own shared transactions. If a queued command has imported sent or
maybe-sent facts, its failure projection must use that journal; local no-I/O
activation alone cannot label the whole recovered intent NotSent. No new path
may erase facts or reset consumed budgets from another shared core.

## Original REDs and controls

Commit a focused real Engine regression against reviewed #1873 before runtime
implementation: A releases a managed mutation; B arrives genuinely and retains
its own preflight/native queue inputs. A's DATA finishes A and requests B.
Before the fix, B cannot issue its Plan in A's callback; pin that refusal. After
the fix, exactly one B-ready callback runs after A fully unwinds, and B admits
and releases under B's owner/generation. Preserve the parent no-cross-owner
assertion, add the safe boundary as the sole new path, and require one terminal
per command. No compile-error RED or dummy Context/timer fixture.

Positive controls: A success, classified failure, expiry and preparation refusal
all wake B once through native queue cleanup; several B/C requests retain FIFO
order; B can issue a bounded prerequisite Read then admit from its own validated
handoff; cached/empty/failure completion; ordinary immediate unparked path; normal
Plan readiness remains unchanged; new generation after retired ID works.

Negative controls snapshot owners/IDs/actions before refusal: wrong issuer,
handle, generation or engine; B's handle supplied from A as direct admission;
ordinary timer/WS callback; foreign Plan readiness; duplicate request; missing
park; re-park/self-wake loop; selected or in-flight prerequisite read; already
admitted Plan; retired/cancelled owner; capacity refusal and invalid time.
A queued ready notification for old B cannot activate a new B after ID reuse.
A callback may not admit C, use A's order response token, or mutate a different
owner. Late/duplicate HTTP/signature data remain fenced. Panic/default callback
controls verify no stale custody survives and no waiter is silently stranded.

Performance/feature controls: zero queue scans when empty even with ordinary
Context/execution observers; at most one bounded activation per retained owner;
fallible admission reservation precedes IDs and actions; no allocations in wake
or callback commit paths after their reservation. Feature-off APIs/storage/callbacks
are absent, not runtime-disabled placeholders.

## Verification and adoption

Run engine feature-on/off, relevant native ApeX + Binance, scoped strict Clippy,
fmt, bans and both request-context check commands with CARGO_BUILD_JOBS=2. Normal
CI owns WASM size, replay and steady CPU gates; do not alter thresholds or run
local timing/load sweeps. Obtain independent source/adversarial, caller and
full-map reviews, plus uufub4 API fit and fe7lgm composition review.

ApeX draft #1891 adopts the native FIFO only after this seam is reviewed; it
still needs conditional terminal, send-time signing/capability, exact delivery
reducers and native reconciliation capture. This plan changes no runtime caller
behavior and prunes no inventory. Landed caller details belong in implementation.

Architecture gate: shared engine activation custody, not a venue scheduling or
ownership workaround. No merge without explicit overseer GO and its own window.
