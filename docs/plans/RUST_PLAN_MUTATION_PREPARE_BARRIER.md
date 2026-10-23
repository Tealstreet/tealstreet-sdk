# Parallel Mutation preparation barrier

G2b plan-first checkpoint, 2026-10-24. Owner codex-hcmq3f; first consumer
claude-4yjlw0's BloFin hedged leverage. Core review codex-uufub4, classification
review claude-dazzy3. Base actual a828ede7ebb54bc9c032ea61365213abac5e3dca.
Plan and original-source REDs only; no frozen runtime API or merge permission.
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
a native definitive-refusal resend grant. A later real native 10002 after release
must retain exactly its one permitted grant. Do not re-sign already released
members or reopen a committed step.

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
