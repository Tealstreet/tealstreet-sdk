# Parallel Mutation preparation barrier

G2b plan-first checkpoint, 2026-10-24. Owner codex-hcmq3f; first consumer
claude-4yjlw0's BloFin hedged leverage. Core review codex-uufub4, classification
review claude-dazzy3. Base actual a828ede7ebb54bc9c032ea61365213abac5e3dca.
Plan, original-source REDs and a private Lifecycle extraction checkpoint;
the group runtime remains unimplemented. No merge permission.
G2 mixed-leg runtime stacks on this slice, not the converse.

## Native requirement and scope

BloFin's isolated hedged leverage prepares both Primary POST signatures, long
then short, before enqueueing either request. A failure of the second preparation
therefore sends neither. Independent provider signing must not turn a local
preparation refusal into a one-sided live leverage change. Consumer ruling and
overseer assignment require a shared barrier, not a venue staging queue.

For a declared concurrent step whose calls are ALL Mutation, prepare and sign
every leg in declaration order before the first physical release. Signing entropy
is drawn in declaration order, independent of provider response order. A delayed
or refused provider must not reorder another leg's nonce draw. BloFin's 21-byte
injected nonce remains the recipe's responsibility; no venue randomness loop.

Any preparation, provider, validation or deadline failure BEFORE the barrier
opens releases zero HTTP from the step. Its local write facts are typed NotSent;
whole-command projection still retains earlier released steps, accepted IDs and
Unknown. A later step's refusal cannot make an already-effectful command NotSent.
After release, venue refusals/transport/timeout follow normal journal projection
and #1939 independent reply handling. This is not atomic execution at the venue.
Read calls and serial steps retain per-call preparation/release; mixed Read/write
parallel steps are outside this first barrier contract and must be explicitly
refused at admission if a consumer requests this barrier for them.

## Shared declaration and state

The canonical Plan admission normalizes each step into immutable membership,
operations, call/attempt generations, bounds and the group-preparation requirement.
No runtime callback chooses a group or adds a sibling after admission. The first
slice supports initial all-Mutation parallel steps; retry/refusal fallback group
membership must be declared and validated through the same Schedule traversal,
not inferred from adjacency in the HTTP queue. If a fallback shape cannot retain
that declaration, reject it before IDs rather than quietly using independent
release. No extra venue flag or Mutation cast.

Provisional internal phases:

1. Reserved: exact members and storage/cancellation capacity admitted fallibly.
2. Preparing: route Prepare/Bind and recipe/provider work are tied to those exact
   members. Canonical recipe preparation/entropy draws are sequenced by declared
   member order even if route resources arrive in reverse order. Provider work
   may complete asynchronously; completion order does not change membership.
3. Ready: every member has a valid prepared route, applied exact signing result,
   immutable physical deadline, retained recipe generation and release evidence
   reservation. All applicable issuer/owner/credential/configuration, pacing,
   signature freshness and route-resource checks pass for every member.
4. Committed: a synchronous checked transition authorizes the group for release.
   Before any member release, reserve all fallible per-member reconciliation and
   journal evidence so failure of a later member cannot introduce a new local
   preparation refusal after an earlier member escaped. No per-member on_release
   side effect runs in fallible preparation. Today's on_http_release is FALLIBLE and can mutate Pending or return a
   Scheduled lease; it cannot run unchanged after group commit. G2b requires a
   shared release-admission prepare/commit split: side-effect-free preparation
   over immutable Pending computes the checked admission/lease and reserves
   storage for every member, followed by infallible move-only commit/notification.
   All members are prepared before any Pending/release state mutates. The single
   request path cuts over to the same choke point. An opaque legacy hook whose
   effects cannot be staged is incompatible and refuses group admission; do not
   invoke it optimistically, clone arbitrary venue state, or silently bypass it.
   The exact prepared-admission representation remains core API review work.
5. Retiring/finished: failure before commit invalidates all members first, then
   cancels provider/routes/timers and retires the group through the common Plan
   funnel. No duplicate terminal or late receipt may reopen it.

The host release protocol is one synchronous checked GROUP claim, as required
by core and consumer review. Exact Rust shape is pending API fit: today's individual
claim_http permits a host to interleave awaits/clock advances between two claims.
A boolean 'all signed' in claim_http is insufficient for the stated guarantee if
a later claim can fail a fresh preparation check after the first escaped. The
reviewed seam uses one synchronous checked group claim yielding all requests,
with all final fallible checks before its first release; no independently
claimable member tokens survive the group commit. It cannot weaken
final send validation, extend deadlines or claim physical network atomicity.
The host must finish resource checks before the claim and invoke each transport
send immediately in the documented commit turn, with no await, actor hop or
reentrant engine callback between sends; synchronous transport exceptions after release remain Unknown/effectful,
not rolled back to NotSent. Group claim captures one raw time and validates
every immutable deadline at that boundary. If the host observes expiry before a
member's physical send after commit, it must not invoke transport: report that
committed release as a transport failure under the existing conservative Unknown
projection, preserving any sibling effects. Never renew or reprepare a committed
member. Pre-commit expiry instead stays Timeout/NotSent. This boundary and the
host inline expiry guard require explicit shared-host tests before runtime freeze. Host adaptation is shared engine/host work, not a
venue-only queue. No API spelling is frozen before core review.

Pacing is never waived by a group boolean. First-slice group admission rejects
any resolved nonzero per-member/shared floor that would separate the members'
physical sends. This includes engine-configured floors, not merely the Plan's
pacing field. A future grouped pacing reservation needs its own reviewed contract;
serial/read behavior remains unchanged. Pin refusal before IDs and zero HTTP.

## Freshness, reprepare and retirement

The minimum member deadline and retained root bound cap preparation. A host
that never prepares or a provider that never answers times out before release.
No timer starts at transport release, no root renewal or fresh budget appears.
If one member's prepared resource/signature becomes stale before commit, hold
the entire group, invalidate the stale preparation generation and reprepare
within the same declared bounds. Fresh entropy draws remain deterministic per
member/reprepare generation; late old receipts are inert. If the bound is lost,
fail the whole uncommitted step with zero HTTP. Reprepare is still unreleased
work, not a resend: it consumes neither CallBound physical attempt credits nor
a native definitive-refusal resend grant. Preserve the existing single/serial
control where reprepare followed by a real native 10002 retains its one grant;
this first barrier slice itself admits no group retries. Do not re-sign already
released members or reopen a committed step.

Host resources bind once. When claim finds one current member stale, retain the
exact already-bound resources for unchanged siblings, replace only the stale
member's preparation/signature/resource, and invalidate the old group permit
generation. The replacement BindGroup identifies retained bindings explicitly;
it never calls bind twice on a sibling or silently re-signs the whole group.
No retained resource can be swapped between members or across group generations.

Parent/command retirement or poison invalidates every unreleased member before
cancellation; duplicate Sign/Prepared/Bind/claim events cannot release any. Reuse
past256cycles must not leak group/resource identities. The feature-off and empty
Plan paths add no group scan or retained storage outside http-plans adopters.

## Journal and classification

No Accepted fact is minted by preparation or a successful signature. Existing
journal release facts begin only at real release. Use the shared classification
and central prepare/commit seal for failures; provider errors are not native auth
proof. For a command with no earlier release, a pre-commit group failure is typed
NotSent once. Earlier effects force the ordinary Partial/Unknown projection,
with accepted IDs and native evidence retained. No local outcome ladder. Preserve provider/recipe typed kind but do not count a
pre-release refusal as auth_failures; it is not a venue credential answer.
Before commit, order-lock state stays unsent: no uncertain fence and no
reconcile_uncertain work. After release, a real Unknown member fences normally.

#1939 only governs independently delivered failures/replies after the barrier
opens; it does not provide preparation atomicity. G2c configuration snapshot
retirement is a separate prerequisite for changing credentials during this wait.
A current issuer/owner/credential fence remains required even before G2c exists.
After commit, each member keeps its own ordinary delivery/failure callback and
typed outcome; pre-reserved release storage is not a replacement group summary.
With independent_calls, a delivered 408/5xx follows the same per-member
on_http_failed path; otherwise it reaches the existing fail_plan. A released
sibling cannot become an unissued refusal. Pin two released members, first 503
then sibling Accepted with its native ID: one final Unknown retains that ID and
the native first-error text. A later serial G2 Verify Read in the same Mutation
Plan remains compatible; only the declared parallel barrier is Mutation-only.

## Original REDs and checks

Own isolated test: engine/tests/plan_mutation_prepare_barrier.rs. It uses today's
genuine mutation Preflight/Plan with two Primary legs and an asynchronous provider.
Keep real Prepare/Bind/Claim and exact HTTP body/header oracles. Baseline assertions:

- one successful signature cannot release either leg while its sibling waits;
- first signature success then second provider refusal yields zero physical HTTP;
- both signatures succeed and both requests eventually release, in declaration
  order; reverse reply order still gives one terminal (working compatibility).

The first two are original runtime assertion REDs, not missing API compilation.
After the reviewed seam, change only admission/dispatch protocol to declare the
shared barrier; keep the wire oracle. Additional controls pin reverse route and
provider completions, first/second provider refusal, preparation deadline,
expired signature/resource, deterministic entropy, duplicate receipts/claims,
no-progress submission, prior accepted step then group refusal, accepted+failed
post-release replies and caller-visible Partial IDs. Do not count a placeholder
provider capability fixture as remote family signing proof.

Implementation follows committed plan, core/classifier/consumer review, original
RED commit, shared seam, then native BloFin proof. Runtime hooks overlap frozen
Plan/HTTP/signing and host work; coordinate exact owned source windows and actual
landings first. Keep the common seal/cleanup funnel and immutable declaration.
No inventory prune here. Focused engine tests, feature-off compile, scoped strict
Clippy/fmt/bans/ratchets; full WASM/replay/size/CPU in exact-head CI. Independent
source/caller/full-map reviews; no merge without overseer GO.

Architecture gate: shared parallel mutation preparation and release choke point.
No eager signing, per-venue group registry, fake Context, root renewal or outcome
reimplementation. CALLERS must state the zero-pre-release-HTTP guarantee and its
limit: once transport release begins, no venue execution atomicity is promised.

## Classification and consumer review

Consumer4yjlw0 cleared the plan with one synchronous group claim and stale-member
reprepare only. Classifierdazzy3 cleared projection with required auth-health,
order-lock, reprepare-budget and pre/post-commit expiry controls. Coreuufub4
requires the fallible release-admission split and pacing refusal above; these are
concrete shared source prerequisites, not runtime approval.

## Original-source checkpoint

The unchanged a828 source reproduces both intended assertions through an async
provider, real Prepare/Bind/Claim and HMAC primitive work: signing the first leg
releases it while the second waits, and second-provider refusal cannot undo that
HTTP. The both-succeed compatibility control passes, checking exact bodies,
remote presentation headers, reverse replies and one total terminal. These are
engine lifecycle proofs, not family remote cryptographic/signing parity.

The standard prepared_actions helper deliberately refuses parked external
signing, so this isolated fixture drives the existing public protocol directly.
Its deterministic prepared resource IDs are checked against Bind/claim tokens.
The two original REDs stay owned/ignored pending G2b runtime. Classification
controls for order-lock state, native resend after reprepare, late commit expiry
and release-hook preparation remain implementation gates, not claimed passing.

## Concrete source API proposal (review gate)

This proposal narrows the first implementation to a side-effect-free Unscheduled
Mutation contract. It needs core/consumer approval before runtime edits. It does
not broaden the existing #1919/#1939 or session work, or clear held files.

### Common Lifecycle preparation and commit

Extract the existing Lifecycle::claim checks into two private operations:

```rust
fn prepare_claim(&self, permit: Permit, time: ReleaseTime, pacing: &SendPacing)
    -> Result<ClaimPreparation, Error>;
fn commit_claim(&mut self, ready: PreparedClaim, pacing: &mut SendPacing)
    -> HttpRelease;
```

ClaimPreparation is WaitUntil or an opaque PreparedClaim. PreparedClaim retains
exact attempt, preparation/permit generation, checked route/freshness revision,
release time and validated admission. It is neither Clone nor public, and never
escapes the single synchronous claim call. Installing Admission::Denied or an
invalid lease fails before commit. commit_claim consumes the token with no
fallible validation or venue callback. It updates the release and pacing state
only from already checked data. Allocation failure, journal/reconciliation
capacity and all per-member release flags are prepared before the group commit.
No caller can manufacture or retain a PreparedClaim across events.

The existing single claim remains a compatibility wrapper: prepare_claim,
existing fallible on_http_release, checked admission installation, commit_claim.
It preserves Scheduled leases and all existing callbacks. That wrapper and the
new group claim use ONE Lifecycle validation/commit implementation; no duplicate
clock, permit, freshness, pacing or release logic. A denied single request keeps
its existing settlement behavior. This is not a second venue signing path.

### First group contract and immutable declaration

Add an immutable MutationBarrierBound { step: usize } to the SAME internal
AdmissionDeclaration used by uniform and conditional Plans. Public spelling is
provisionally admit_plan_with_barriers(preflight, steps, policy, barriers).
Validate all declarations before IDs and preserve existing readiness custody.
First slice accepts only top-level steps with at least two Mutation calls, one
attempt each and no per-member fallback; reject other barrier geometry before
reservation rather than silently release independently. Serial surrounding steps
retain their native attempt/fallback policy and earlier journal evidence.

Membership is resolved structurally from the Schedule's actual root step and
retained call entries, not a separate root-first flattened index convention.
Later mixed-leg admission must check EACH bound call's Operation, not merely
PlanPolicy.operation. Read calls cannot opt into this Mutation-only barrier.
Group members keep their original call IDs/windows and all existing owner,
credential and preparation generations. No new timer extends their bounds.

The first consumer explicitly opts into pure Unscheduled group admission through
a shared readonly Venue hook. Its default refuses support:

```rust
fn prepare_group_http_release(&self, pending: &Self::Pending, release: &HttpRelease)
    -> Result<Admission, Error>;
```

Only Unscheduled is accepted for this slice. Scheduled or Denied refuses the
uncommitted group; a family that needs mutable release admission remains held.
The hook grants no Cx/Store/I/O and must not mutate native state through interior
mutability. It cannot call the old mutable on_http_release. Existing families do
not become eligible merely because their old hook happens to return Unscheduled;
BloFin's consumer must explicitly declare its side-effect-free contract. The
existing mutable hook is never bypassed by guessing from a method/endpoint.
Any post-commit Pending notification is a separate infallible hook and cannot
make a release decision, schedule work or return an error. Panic follows sticky
poison and conservative already-committed write projection. Exact notification
signature, including whether the first consumer needs one, remains reviewed.

This narrows the earlier general staged-admission design: it delivers an explicit
Unscheduled contract first, while all single-request consumers continue through
the shared compatibility wrapper. General Scheduled-group prepare tokens need a
later shared scheduler transaction; they are not fabricated from mutable hooks.

### One group host handoff

All routes are prepared and every recipe is prepared in declaration order;
provider results may arrive in either order. No member emits a claimable individual
Bind. Once all members are signed, emit one BindGroup action with an opaque
GroupPermit and the immutable declaration-order packets/preparations. The host
binds every exact prepared resource and checks every resource.ready first. An
unsupported group protocol refuses before claim; there is no individual fallback.

```rust
engine.claim_http_group(now, permit: GroupPermit) -> Result<GroupClaim, Error>
// GroupClaim is WaitUntil or Released(ReleasedGroup).
```

ReleasedGroup is a move-only declaration-order vector of exact request/release
facts for the already bound packets. Storage is reserved before claim commit;
no packet/header cloning or fallible vector construction follows the first
member's commit. An individual claim_http for any grouped permit refuses before
journal/pacing effects. The group claim validates issuer, owner, Plan step,
member order/count and every original preparation generation in one turn, then
prepares all Lifecycle claims, pure venue admissions, journal/reconciliation
records, release flags and result storage. Any refusal before commit invalidates
no current group unless the caller has exact current-group failure authority.
The disposition is explicit:

| Claim condition | Disposition before commit |
| --- | --- |
| Foreign, duplicate, old or retired GroupPermit | Inert structural refusal; no live group, journal, pacing or owner changes. |
| Shared pacing WaitUntil | Keep the group live; no release evidence or pacing is spent. |
| Exact current stale resource, route or signature | Hold the group and reprepare only the stale member within its original bounds, retaining siblings' bound resources. |
| Exact current group's terminal provider, deadline, capacity or admission failure | Invalidate its members and seal through the common Plan funnel once, preserving prior-step effects. |

A bind/resource failure associated with the exact current uncommitted group
also releases neither member and uses that same typed failure path. It gives
whole-command NotSent only when no earlier step released; earlier effects still
project through the journal. An invalid token never grants retirement authority.

First admission rejects any nonzero resolved shared/member pacing floor. A group
with zero floor can still be waiting behind an earlier use of a shared pacing
domain; the same checked shared pacing state decides eligibility. Never waive a
floor or consume member pacing before all checks pass. No host await, actor hop
or reentrant engine callback may occur between returned release facts and the
immediate ordered transport sends. Host resource failure before claim is
NotSent; a synchronous send exception or host-observed expiry after commit is
conservatively Unknown with prior effects retained, as defined above.

Group preparation failure stores provider/native kind without auth-health
feedback; it cannot create an uncertain order-lock fence or consume retry/resend
credits. Signature reprepare consumes only a fresh preparation/signing generation,
not a new physical attempt. These remain mandatory runtime assertions, not
claims proven by the current baseline REDs.

The per-member Mutation order_locks.note_command_write marking belongs in the
Engine commit transaction that calls commit_claim, never prepare_claim. It stays
outside the Lifecycle-only helper. The second-provider-refusal control must assert
unchanged order locks, zero venue reconciliation calls and unchanged auth health.
Final signing-release observation and raw-expiry projection remain one shared
projector; publish each member's release metadata only after the entire group
commits. No partial group's flags or metadata may escape preparation.

### Reviewed direction and remaining source gate

Core, classification and BloFin consumer review cleared the bounded direction
at 956990a1b88. Independent source/full-map review required the disposition table
and one-use sibling binding rules above; caller review confirmed the first
consumer and the unchanged single/serial resend compatibility boundary.
This clears implementation design, not held source files. Send exact Lifecycle,
Plan, signing-release and host extraction hunks to their owners before edits;
retain single Scheduled/error behavior and resolved shared pacing checks.

### Source ownership and next checks

Own modules proposed: request_context_group_claim.rs (bounded membership and
atomic preparation/commit orchestration) and request_context_group_lifecycle.rs
(private Lifecycle preparation token). Source hooks are narrow: Lifecycle::claim,
shared Plan admission, dispatch readiness/entropy ordering, group action
serialization, and the shared native/WASM host protocol. The exact edits in
request_context.rs, engine.rs, http_lifecycle.rs, request_context_planned.rs and
host.mjs are held by live PRs; obtain their owners' hunk agreement or integrate
their actual landing before touching them. G1's session hooks remain separately
held for combo-5. Unheld filenames alone do not clear a runtime/API gate.

After API approval: reproduce and activate the two baseline REDs through the
group protocol, retain the existing single-claim tests unchanged, and add focused
controls for readonly-hook refusal, Scheduled denial, pre-reserved evidence
capacity refusal, mixed-operation/fallback/pacing declaration refusal, reverse
route/provider completion, declaration-order entropy, stale one-member reprepare,
prior-step Partial/Unknown and duplicate/retired group permits. Then the BloFin
native consumer proves its exact long/short wire and failure behavior. No claim
of whole-family custody readiness or removal of legacy inventory in this slice.

### First source checkpoint: single-claim extraction

Actual 60d4b8cf9ebf74aa8064d1c16a5831a6707d50ab is genuinely merged. Core
review cleared exactly the Lifecycle::claim wrapper and the private child module
request_context_group_lifecycle.rs before editing the held source. The wrapper
preserves validation order, mutable admission, Scheduled leases, callback errors,
Denied settlement, pacing and release behavior. ClaimDraft finalizes admission
into a move-only PreparedClaim; both remain internal to the synchronous call.
The commit's debug assertion is diagnostic, not future group-token validation.

Engine order-lock marking, signing receipt observation, metadata projection,
Plan admission and host behavior are untouched. The original two barrier REDs
remain ignored: this extraction alone does not hold or release a group. Exact
group orchestration, coupled Lifecycle/pacing validation and host hunks still
require separate source agreement before edits.

### Current landing integration

Actual a2533f5761f is plain-merged at 20549629e52. Only the README index needed
manual resolution: incoming rows plus this plan's row once. The shared
SigningRelease/HttpRelease expiry projectors are retained. Existing single-claim
controls pass 47/47. The original async barrier assertions still both fail when
explicitly unignored on this integration; the both-succeed compatibility control
passes. This refresh does not claim a group implementation or READY.
