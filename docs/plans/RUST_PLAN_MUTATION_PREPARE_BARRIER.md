# Parallel Mutation preparation barrier

G2b plan-first checkpoint, 2026-10-24. Owner codex-hcmq3f; first consumer
claude-4yjlw0's BloFin hedged leverage. Core review codex-uufub4, classification
review claude-dazzy3. Base actual a828ede7ebb54bc9c032ea61365213abac5e3dca.
Plan, original-source REDs and the reviewed shared engine/host implementation
are committed locally in successive checkpoints below. Final CI remains open;
there is no merge permission.
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

Core cleared the exact group-storage and pure-hook artifact against 20549629e52.
The implemented batch insertion prepares all checks, boxes and map capacity
before publishing any record. Four focused controls pass: complete insertion,
duplicate rejection, expiry rejection, and slot/byte capacity including a record
currently delivering its response. Failure preserves prior bytes and records.
The pure group hook defaults to Invalid; single admission is unchanged. Neither
hook is wired to group release yet, so the original barrier REDs remain open.

The exact frozen #1919 pin 2f4a08b449c is integrated at a8a7347711a. Core and
the execution owner cleared the additive admission artifact before application.
Five focused controls now pass: invalid geometry/Read policy, duplicate members,
resolved shared/member pacing and physical-owner refusal spend no resources;
root membership remains exact after an earlier fallback; all three existing
constructors retain empty barriers. The first run stopped at fixture setup
because entropy was omitted; using the existing seeded helper reaches all
assertions. Execution-owned barrier Plans remain explicitly unsupported.

The original async consumer now opts into the explicit barrier declaration;
its wire and terminal assertions are unchanged. The feature-on integration
build currently refuses the two unwired staged primitives as dead code
(Body.barriers and insert_group). No lint exemption is added. Only the focused
unit results above and the feature-off compile are passing at this intermediate
checkpoint; complete integration checks resume once orchestration consumes them.

The owner-cleared bounded protocol and private immutable group-custody hooks
are applied locally after `e9f18dde725`. Token boundary and genuine Plan-custody
unit controls are added. Their first build stops at the two exhaustive
BindGroup arms in resource_ids/test_support, which still require the grouped
handoff; no token/custody execution PASS is claimed at this checkpoint.
The exact orchestration artifact is under core/signer review before editing
its parent hooks. Native and WASM host handoff remain separate implementation
and review gates. These local checkpoints are not pushed or READY.

## Orchestration and host review checkpoint

Exact unapplied orchestration artifact `f2da17c652fb` corrects owner review of
`84deebbd1528`: re-export visibility and Admission qualification compile;
nonfinite claim time refuses before reservation/failure effects. An announced
member whose host binding has not been acknowledged by the exact whole claim
fails through the common Plan projection on Stale, without reprepare or HTTP.
An acknowledged claim may reprepare only its stale member and retain siblings.
Retirement drops queued group packets; a retired admission cannot issue again.
No group path invents a NotSent terminal or erases earlier/inherited uncertainty.

Independent review also corrected route expiry before any recipe activation,
retained bound metadata on every recoverable Stale path, and sticky poison
through all commits and deferred recipe destruction. Scratch archive of exact
base `28e1bb36f3e` plus the corrected artifact passes the engine library check
and strict Clippy with http-plans/order-locks, plus feature-off library check.
These are library results, not test-support, native/WASM or whole-group proof.

Groups intentionally skip the mutable on_http_release hook. An adopter that
stamps Pending there must move that native state before group admission or use
the committed on_signing_release observation; the pure group hook cannot mutate
Pending. Do not opt a family into groups while silently dropping its stamping.

A separate unapplied WASM/JS handoff artifact has 21 passing original/group host
model controls in scratch. It checks every resource before a single claim,
uses preallocated per-member output bounds and scalar status, refuses external
invoke/run/dispose/cleanup/cancel reentry through the last handoff, and delivers
postcommit failures only after sibling sends. Claim/output-copy exceptions are
fail-stop, never a precommit refusal. Exact owner review, Rust ABI controls,
retained-sibling controls, native host and full engine adoption remain open.
Neither artifact is applied to held runtime hooks or claimed READY.

## Applied orchestration checkpoint

Core owner cleared exact artifact `100d5404582c` against `28e1bb36f3e` after
review corrections. The hooks are now applied and formatted. Group-member
preparation callbacks cannot use `finish_plan_failed_call` to retire only an
unreleased member: the exact recorded/released-attempt validator refuses before
any settlement. Released independent failures still use the existing path.

Both original asynchronous REDs are active and GREEN, with assertions retained.
The dedicated integration target passes seven controls: first signature held,
second provider refusal with zero HTTP/auth-health change, reverse successful
replies, stale announced/unbound whole-group failure despite a callback's
partial-completion attempt, inert nonfinite/individual claims, precommit Timeout
with NotSent and no reconciliation, and postcommit independent 503 plus an
accepted sibling preserving its native ID in Unknown. Fifty-two group-filtered
engine unit controls pass, including protocol/custody/storage controls. Strict
Clippy passes for the library and dedicated integration target with Plans/locks,
and for the feature-off library. These are local scoped results, not READY.

Native and WASM host artifacts remain unapplied and separately reviewed. The
JS boundary must defer an entire control/text transport notification before
socket maps, statistics or engine effects. A reentrant borrowed binary payload
or bounded notification overflow makes the host fail-stop after attempting all
handoffs, before replaying notifications. No payload is retained or copied to
manufacture immutability. The upcoming canonical pacing-set resolver must be
consumed before group admission can support aliases; outer zero alone is not
proof of expanded zero pacing. The #2016 inherited-uncertainty group control
remains a named dependency control until that reviewed witness is composed;
this checkpoint creates no substitute witness or hardcoded NotSent projection.

## Reviewed JS handoff applied

Core owner independently reconstructed and cleared exact host artifact
`c0d3afc933ae` with 33/33 original/group Node controls. That artifact is now
applied locally, with formatting only in Rust wrappers. Applied Node controls
also pass 33/33; the native-platform compilation of the WASM ABI wrapper passes
with bitunix and Plans/locks. This is not a wasm32 or browser proof. Host calls
from a bound transport cannot dispose/cancel/reenter the engine between group
claim and the last handoff. Captured socket control/text and timer notifications
are deferred whole; borrowed binary or overflow fails after all handoffs and
before replay. Ordinary binary delivery outside the burst is unchanged.

Two additional genuine engine controls pass: reverse route/provider completion
still draws both 21-byte entropy samples in declaration order, and an exact
bound Stale claim re-prepares only the expired member, retaining the sibling
packet and re-announcing once. The first stale fixture gave both signatures the
same validity window, so both correctly expired; the corrected fixture declares
distinct recipe validity windows to isolate one expired member. Dedicated
integration controls now pass nine with none ignored. Native host and shared
pacing-set composition remain open gates before push/readiness.

## Pending pacing composition and native source review

Reviewed pacing #2025 pin `59109b3ade9` is being merged locally onto `a018f3ccddd`.
The overlap owner requires this merge remain uncommitted until exact review;
there is no freeze or new remote head. The resolved source artifact is
`11d12971e7df` (three engine paths). Both queue checks precede staging and ID
spend; group custody is checked before canonical pacing. The zero-floor helper
now delegates to the shared resolver. The new expanded-alias admission control
passes before any ID/storage mutation.

A genuine composition RED exposed an existing future primitive clock despite a
zero current floor: a prior Read released with outer gap 100, then the zero-floor
group tried to sign at time 1. Source correction `d09267a67c0b` remains unapplied
pending review. It checks the canonical clocks before activating any recipes,
wakes at the minimum of eligibility and retained call/route bounds, and pumps the
whole group on an early timer. Scratch passes all twelve group controls plus all
fifteen pacing controls. Route-expiry/early-wake testing proves no provider call
before eligibility, one expired-route reprepare with the same physical attempt,
and unchanged call deadlines at final release. Strict scoped Clippy and the
feature-off library pass in scratch. The actual uncorrected candidate remains
RED at the new presign assertion; it must not be pushed or marked ready.

Native source artifact `a81781da54b4` is also unapplied pending review. Genuine
Engine+Host controls pass three cases: all bind/readiness checks before two
handoffs with one success, second bind/readiness failure with zero sends and
NotSent, and first transport panic with both handoffs attempted plus one Unknown
retaining accepted IDs. The final fixture uses a real Primary Plan and an inline
fixture provider implementing sign_ready. Early fixture failures at keyless
preflight admission and the absent sign_ready path are not proof. Native strict
library/target Clippy passes. Neither host model nor source review alone is full
G2b CI, wasm32 performance, inherited-uncertainty or landing certification.

## Superseding group-wait custody review

The pacing owner rejected `d09267a67c0b`: with providers parked, another genuine
Background Claim can advance the shared clock. Signed completion then arms the
first member's timer after both members became Permitted; its Prepared-only
wake predicate never announces the group. The owner independently reproduced
that failure with the earlier presign-wait control still GREEN. A second control
executed locally proves the same loss when only the non-first member re-prepares
while the timer's first member remains Permitted. Its expected fresh signing
work is absent on d092; this is an unlanded composition defect, not a claim about
an existing shipped venue.

Replacement source `5cf20cf4314f` remains unapplied in the actual worktree. The
private ordinal-zero Member owns explicit group pacing-wait state, independent
of its local signing phase. Its exact managed timer consumes that state and runs
the validated group pump; member reset/drop clears it, and reaching eligibility
restores the original physical timer deadline. The final Claim path is unchanged.
Both reproduced regressions now pass with the earlier assertions retained.

The pacing owner independently cleared composition artifact `11d12971e7df` plus
replacement `5cf20cf4314f` and controls `64792f958a5d` on full commit
`a018f3ccdddf8184afbb5f74340cae0d70313e5e`: fourteen group controls, fifteen
pacing controls and the expanded-alias no-ID control pass with managed sockets.
The composition hash names a patch, not a Git commit. Local Plans-only execution
also passes thirteen group controls and all fifteen pacing controls; the placing
fence test is compiled only with order-locks. Strict combined, Plans-only and
feature-off library checks pass. Core source review is still pending; the actual
merge remains uncommitted and the rejected correction must not be applied.

Both source owners have now independently cleared the replacement: pacing owner
with managed sockets and Plan core in Plans-only plus combined modes. The exact
`5cf20cf4314f` source correction is applied in the worktree, byte-exact to the
core-reviewed archive. This closes the unlanded composition's phase-dependent
wake defect. The prior rejection and original RED receipts above remain evidence;
`d09267a67c0b` is superseded. Native artifact and final CI/readiness remain separate.

The exact frozen #1939 dependency `73ac9431b1f944b88c9af9c292be632ed81dd9a3`
is genuinely merged, replacing the earlier reviewed/unfrozen 0abc label. The
pacing owner's actual-landing carry `e5fda51769f` is also merged. Both integrations
preserve the reviewed G2b engine and host sources byte for byte; only document
conflicts needed resolution. Neither dependency is described as an actual
landing. Native source review and final integrated CI remain open.

## Native handoff applied after fixture correction

The pacing owner verified committed `68e0f9b702b` preserves every reviewed
engine/host/crypto/registry source from `e2dc8144bbc` byte for byte. The frozen
#1939 dependency and reviewed pacing carry are genuine ancestors. This is a
source-preservation receipt, not a new test run or an actual-landing claim.

Core independently executed native artifact `a81781da54b4` and found a fixture
failure: its six-event drain stopped before the second response End when a
genuine timer appeared first. That run remains two PASS and one assertion FAIL;
the event trace establishes no production defect. Test-only correction
`5e30f3c31a79` waits for two actual preparations and drains a bounded number of
events to the genuine command terminal, handling incidental timers. It adds an
actually armed timer and retains the send-order, two-reply, single-terminal and
accepted-ID/Unknown assertions. No production hunk changed to fix the fixture.

Core independently reran the exact corrected artifact: three genuine Engine+
Host controls PASS, none ignored. All resources bind and pass readiness before
one claim; both handoffs precede callbacks. Precommit bind/readiness failure
sends neither member with the shared NotSent projection. A first-send panic
still attempts the sibling and produces one Unknown with its accepted ID.
The exact native source and fixture are now applied locally, byte-exact to the
reviewed archive. Applied checks pass fourteen group and fifteen pacing engine
controls, eight native managed-HTTP controls (including the three group cases),
and thirty-three JS host controls, all without ignored cases. Native scoped
strict Clippy, formatting and the full bans script pass. Final CI remains a
separate gate; this checkpoint does not claim READY, a family custody cutover
or landing.

## Capability isolation proposal after CI size failure

Exact-head Rust run `40056140003` found compile omissions in four old fixture
adapters. Test-only `c59f38b6c5a` adds one explicit BindGroup panic arm to each;
stripping those three lines restores every parent blob exactly. Workspace test
compilation with all features and twenty-nine affected fixture controls pass,
with scoped strict checks and formatting. Engine and host runtime trees remain
exactly `a01dbc4caee`. The original CI failures are retained as compile failures,
not runtime execution evidence.

The same run measures Bybit web and Node at 1,345,004 bytes against the unchanged
1,343,488-byte limit: 1,516 bytes over. Replay/CPU collection did not run past
that size failure; its missing-output error supplies no CPU diagnosis. The
composition's size attribution is not measured yet. No profile, budget, feature
closure or assertion has been changed to conceal the failure.

Core direction allows a separate default-off Mutation-barrier capability.
Proposed engine feature `http-mutation-barriers` depends on `http-plans` and is
enabled explicitly by real barrier adopters and group proof targets. The chosen
off-build contract is absence of `admit_plan_with_barriers`: an ordinary Plan
consumer cannot declare a barrier. Private admission still rejects any nonempty
barrier declaration before origin validation, credits or IDs, including physical
admission. Uniform and conditional constructors continue passing empty barriers.

Gate the complete runtime: normalized Body membership, Managed member storage,
submission checks, route/signing activation pump, wait ownership, grouped claim,
group-only response insertion and pure release hook. Preserve the common single
Lifecycle prepare/commit extraction, canonical pacing resolver, ordinary Plan
admission, founder custody and journal/seal behavior in off builds. Stable
protocol metadata types may remain for exhaustive host/fixture matches; they
grant no owner, admission or release authority. Off-build pending/claim probes
are inert false/Invalid stubs without clock, staging, credit or ID effects.

Native and WASM hosts expose the same explicit feature forwarding to the engine.
WASM group exports and native group state, wake and handoff code are gated
coherently. Minimal group serialization metadata remains a protocol contract,
without granting release authority. A host without support rejects a group
before resource bind or claim; it never invokes individual release as fallback.
The JS helper already refuses missing group exports before binding resources.
Future family adoption must opt its host and engine into the capability in that
consumer's feature closure; ordinary Bybit/BingX consumers do not opt in.

Before source application, send exact conditional hunks to core and host/Plan
owners. Required checks retain all existing assertions: ordinary http-plans-only
single/Plan controls and feature-off compilation; explicit capability-on group,
native and JS proofs; off-host refusal; expanded pacing alias rejection before
IDs; and same-composed-head Bybit feature-off/on CI size measurement with pinned
profiles/tools. The full capability-on group behavior must remain byte-equivalent
except for conditional compilation. This is a proposal, not an applied feature,
an attribution result, a size fix or permission to freeze.

The exact conditional artifact `aee3c129aed7841a` and supplemental controls
`1d787d111ade8f81` have core source/reconstruction/execution clearance. They are
applied locally. The applied workspace compiles all test targets with all features;
the private off-admission control, twenty-nine group/pacing controls and
thirty-five JS controls pass, with formatting and bans clean. The Plan owner
subsequently cleared the exact conditional hunks, including the pre-origin
refusal and unchanged owner-generic seals, founder and receipt paths.

The isolated native proof at `hosts/native/tests/barrier-feature-unification`
must run twice in CI, once without host support and once with `host-barriers`.
Core independently executed both public `Host::run` modes: with host support
disabled it binds and sends nothing and yields InvalidInput/NotSent; with support
enabled it binds/checks both resources before two sends and one success. A root
workspace all-features run cannot certify the negative because feature
unification would enable the host. Separate same-head Bybit off/on measurements
with unchanged profiles, pinned tools and budget remain required. The proposed
CI plumbing `d4dfc4abfbdd62f8` is now core- and auditor-cleared and applied with
the conditional source in the same change, as the auditor requires. It adds
explicit isolated host-off/on proof invocations and a dispatch-only same-head
Bybit comparison. The enabled measurement is diagnostic while no shipping
build opts into barriers; future adopters still owe their own budget gates.
No size attribution is inferred before that CI measurement completes.

Plan-owner review also clears the unapplied inherited-uncertainty control
`f8290a233e55d73f`: a real Preflight fact gives Unknown on an unreleased group
failure, versus NotSent without the fact, with one terminal and no HTTP. The
reviewed dependency composition is #2020 `be661facc08`, which contains #2016
`6e29a1414ae` and its owner-resolved physical/Command journal propagation.
Execution against that genuine dependency remains separate from the current
capability checks; neither dependency is described as an actual landing.

## Ordinary Plan-expiry guard correction

Capability head `4adac95464c` passed its dedicated barrier CI check, but its
Bybit feature-mode check exposed an ordinary Plan regression: a host failure
at the root deadline cleaned only the first of three pending peers. The
conditional patch had incorrectly gated `expire_plan_before_http_failure`
behind the barrier feature. That hook belongs to every HTTP Plan. Exact
correction `7688542e46e54546` restores only its `http-plans` guard, preserving
the exact preparation check before the common expiry sweep.

The unchanged seven cleanup controls reproduce six PASS / one FAIL without
the correction in the Bybit closure, and seven PASS with it; the barrier-on
working control also passes seven. Corrected scoped strict Clippy passes.
Core source review clears the one-line restoration. No assertion is changed.

The intermediate same-head comparison on `4ad` measured web and Node equally:
off 1,323,702 bytes, on 1,343,943 bytes, versus the fixed 1,343,488-byte budget.
The off build has 19,786 bytes of headroom; enabled is diagnostic and is not
a shipping adopter's budget approval. These measurements do not certify the
corrected source: fresh CI and a same-head comparison remain required.
