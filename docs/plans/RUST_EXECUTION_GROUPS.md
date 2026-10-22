# Shared execution groups and observer completion

Status: initial local plan, before implementation or a frozen public API. Owner:
uufub4. Start from request follow-on503d5641d45, which genuinely includes actual
RequestContext1741 and landing1765. Fixture1781 remains the immediate landing
priority after1775. Do not merge this work during that window.

## Problem and common owner

Hyro joins identical order instructions into one execution but retains numeric
waiters and completes them in venue loops (bybit/src/hyro.rs Group.waiters,
start_hyro_place, advance_hyro and pause_hyro). Metadata1747 joins WEEX/BingX
settings reads; a first command cannot own physical work on behalf of later
commands if cancelling that first command revokes the physical request.

Build one engine execution-group authority for both. Reuse RequestContext's
managed preparation, provider, final claim, release facts, response custody and
outcome journal. A group is neither command0 nor a Background+Mutation escape.
Do not duplicate the executor, completion registry, nonce/resource allocator,
retry classifier or metadata waiter mechanism in a venue.

The native adapter declares equivalent work and codec facts. Engine admission
must also match operation, credential slot/reference and account/subject scope;
the same URL or a venue string key alone cannot authorize joining. Hyro's
current key serializes the complete order instructions, so initial write joins
observe the same ordered effects. Arbitrary subsets or unrelated writes are out
of the first reference and must reject rather than infer leg attribution.

## Authority and clock rules

- An opaque execution identity owns physical legs independently of individual
  command observers. Each observer retains the engine issuer and exact existing
  command generation. Reusing CommandId cannot rebind an old observer.
- Establish the execution's raw deadline once. Each observer has its own earlier
  or equal deadline; neither a join nor validated read progress extends the
  execution bound. Every continuation, including BingX's second read, inherits
  that exact bound. Protocol calibration affects signing stamps only.
- The initial API audit must settle the independently authorized execution bound
  before coding: it cannot be derived by lengthening the first command Context.
  Expiring a short-lived first observer must leave a longer-lived valid observer
  attached without increasing the already-established execution deadline.
- Detaching one observer retires only that observer. Explicit whole-group abort
  is distinct. With no observers left, revoke unsent work; already-released writes
  retain uncertainty/evidence and bounded late-response custody. No expiry or
  cancellation creates new signing, retry or financial authority.
- A group delivery authorizes only its registered live observers. It must not
  clear the single-command http_delivery fence and then loop over numeric done
  calls. Validate group, physical attempt, observer generation and terminal state
  before any publication or completion; consume each completion exactly once.
- A shared write journal owns physical release/result facts once. Observer
  projection must retain prior accepted IDs and prior-release uncertainty even
  when the observer joined after send. A failed later signer/transport attempt
  cannot project NotSent for an observed write that already reached the venue.
- Read liveness and publication freshness remain separate. A superseded but live
  metadata read can return the approved cached fallback without permission to
  update Store/native caches. Reuse the shared snapshot/metadata fences; do not
  add another venue epoch or treat publication denial as a network failure.

## Proposed seam for API review

Names below describe responsibilities and are not a published API:

| Operation | Required authority and effect |
| --- | --- |
| Begin execution | Validated execution policy and semantic scope; reserve identity, bounded group capacity and initial observer atomically. No physical action on refusal. |
| Join observer | Opaque live group plus issued command Context of matching semantics; retain exact generation/deadline, snapshot existing release/evidence facts, no new network attempt. |
| Issue group leg | Group-bound Context/attempt through the existing managed path; normal read admission, route, signing, pacing and final claim still run once per physical release. |
| Detach observer | Exact observer token/generation; finish its approved cancellation/expiry outcome once, leave other observers and already-sent evidence intact. |
| Complete execution | Exact active group delivery plus typed native result; validate the publication fence, seal one physical journal, fan out result/error to each still-authorized observer once. |
| Abort execution | Explicit group authority; retire exact physical work and observers with outcomes derived from actual release evidence. |

Prefer opaque handles with private fields over raw IDs. Reuse the common checked
resource reservation work owned by5c57jl; no wrapping counters or partial group
registration. Keep stored observer count, group count, payload/evidence and
terminal retention bounded. Capacity refusal must leave an existing execution
and its sent facts unchanged. Retain exact order IDs; over-limit evidence is
conservatively Unknown, never a truncated identity or success.

Do not copy a full mutable journal for each observer. Audit a single shared fact
store with generation-bound projections, including per-observer reports and
retirement, against the existing command evidence budgets. The API must define
whether a late write observer is admitted after a terminal or uncertain result;
initially reject terminal joins and require explicit reconciliation authority
before any new physical retry. Native absence assertions alone are insufficient.

## Acceptance matrix

Each row must drive the real managed release path where it concerns I/O; pure
state-machine tests precede native consumer adoption.

| Case | Required observation |
| --- | --- |
| Two joined metadata command generations | One physical request; success or error completes each live owner once. |
| First observer cancelled or expired | Second valid observer survives; cancelling the first does not revoke physical work on its behalf. |
| Follower cancelled | No later callback, timer or join resurrects it. |
| Reused numeric command ID | Old generation cannot finish, error or publish for the new generation. |
| Late join | Existing execution deadline is unchanged; no new attempt, response lease renewal or duplicated pacing charge. |
| BingX two-stage metadata | Second leg inherits the original execution bound and live observer membership. |
| Pause or read admission denial | Exact group and observers retire once with typed pre-send outcomes; no fabricated transport failure or duplicate backoff. |
| Preparation, binding or provider refusal | No physical send; observer outcomes still include any earlier released write evidence. |
| Timeout or transport loss after release | Write observers remain Unknown; no unsent projection or automatic replay. |
| Stale delivery | No Store/native-cache mutation and no completion of a newer group or caller generation. |
| Sticky write or superseded metadata read | Approved cached fallback may complete a live read; stale publication remains denied. |
| Identical Hyro commands join | One physical create sequence, original lots/client identities preserved, progress and terminal result delivered once to every live observer. |
| Hyro join after release, then failed retry | Prior release/IDs survive; signer or host failure remains Unknown. |
| Earlier accepted Hyro leg plus later refusal | Known partial effects and exact IDs survive every observer projection; no first-error retention or whole Rejected. |
| Duplicate/reordered completion or old timer | No duplicate receipt, command event, journal mutation or completion. |
| Wrong issuer, credential, subject or operation | Join fails atomically without changing the original group or its observers. |
| Group/observer/evidence capacity exhausted | Existing work/facts survive, no leaked identity or action, no manufactured NotSent for sent work. |
| Last observer detached | Unsent work is revoked; released write evidence and late custody follow existing bounded lifecycle policy. |

## Initial plan audit refinements

Hyro currently drains and completes waiters on a pause or signing failure while
retaining its immutable plan, orderLinkIds and uncertainty. An explicit later
caller can resume it. Preserve that behavior with separate states for suspended
intent, sealed observer and terminal physical execution. An expired execution
cannot be reopened or have its deadline extended. A later caller needs a new,
independently bounded execution plus an engine-validated continuation of the
retained identity/evidence, not a new order identity or reset sent flags. This
continuation/reconciliation authority must be audited before Hyro adoption;
metadata reads do not need a retained write intent. Native absence or a matching
string key alone cannot mint it. Test pause, all observers terminal, then explicit
new caller both before and after the old execution cap; no automatic replay.

Initial observer composition is exclusive whole-command equivalence. Admission
rejects a command that already has an independent physical leg or membership in
another group. An empty declared Mutation root is not itself an independent
physical write. Do not seal a command's unrelated sibling journal, overwrite
its prior evidence or claim support for arbitrary multi-group commands. Test a
partially sent independent command attempting to join, and an owner attempting
to observe two groups: rejection leaves original work and outcome authority
unchanged. Future composition must aggregate under the existing journal rather
than weakening this boundary.

Audit fan-out capacity before publication. Reserve bounded observer/report/result
bookkeeping at admission; validate every surviving generation before publishing
or marking terminal. A stale observer or a refused join must not erase shared
facts, strand another valid observer, or borrow its completion authority. Account
for bounded progress/terminal queues explicitly before implementation; capacity
pressure cannot drop native IDs or turn unknown effects into success. A duplicate
join of the same owner generation is idempotent with no second receipt, capacity
charge or completion. A late join may receive validated retained progress, but
that notification must not pretend another physical send occurred.

After all observers detach, unsent execution has no publication authority.
Released writes retain only the existing fenced late-evidence/reconciliation
rights: no renewed command completion or publication for a new same-key group.
Add real-release controls for detach while provider parked, duplicate join,
late-join progress, all-detach followed by same-key/new-generation execution, and
the old ACK arriving after that replacement. Financial snapshot admission and
command/group liveness remain separate checks.

## Committable sequence and ownership

1. Independently audit this plan and the concrete authority/deadline/API choices
   against metadata1747 and Hyro. Commit the exact API and allocation/rollback
   contract before production changes; no adapter promises from these names.
2. Original RED engine tests for generation-bound joins, independent expiry,
   prior-release projections, atomic capacity and terminal fan-out. Implement the
   pure shared group owner, then adversarial audit it.
3. Extend the existing managed HTTP owner/delivery path once. Prove the parked
   provider, final-handoff, timeout and duplicate-event cases without a second
   executor or a completion bypass. Keep ordinary single-owner behavior exact.
4. Metadata read reference with cxxyqr after actual private signer prerequisites;
   preserve cached-fallback/publication policy and exact two-stage deadlines.
5. Hyro write reference after ordinary Bybit and the shared reconciliation-proven
   retry contract. Retire numeric waiter completion and prove progress/IDs,
   coalescing and no duplicate order under lost ACKs. No whole-Bybit/cutover claim
   until both reference and legacy-site obligations are actually complete.

Coordinate engine/history Completion separately with cxxyqr; group reads must not
borrow numeric legacy completion. Metadata Store fields and freshness policy stay
with that lane. Shared session Context is a separate item after the response-proof
guard: session upkeep has no financial effect or command authority and must not
be implemented by this execution-group work.

Architecture gate: No. The proposed engine group is the shared owner for two
different consumers; neither may create its own observer/completion mechanism.
