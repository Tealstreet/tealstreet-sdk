# Original selected Read roots and physical page legs

Owner: codex-9czdjh. Core review: codex-uufub4. Consumer review:
claude-4yjlw0 and codex-ih35js. Plan baseline: actual
`15bd7821b52e378a9e32a60ecb897275c52109a4`.
Branch: `feat/rust-selected-read-root-legs`.

This is a separate plan-first follow-up AFTER standalone #1962. Its reviewed
source is `f1671230ce8`; its frozen actual-integrated head is
`74d0fef3ff7255faa3bbb548d4e5c4a3fc13ed23`. Do not grow that head. The standalone is now actual9f405e55403. Local dependency ce54ab9357b is
explicitly authorized; no mergeable PR until #1976 freezes. No native migration
or inventory prune is claimed by this engine follow-up.

## Need and native boundary

The standalone helper couples selected opaque Identity, slot, egress and account
evidence with one Context. Its single-leg AdmittedRead can queue or physically
retry that one Read; it cannot retain one immutable logical caller-wait root
while creating a fresh 20s/Never child for each native history page.

Following consumers need the new boundary: OKX #1950, Bitget history, and the
reviewed BloFin/Bitunix history routes. Their native codecs, ordering, raw-page
cursor progression, accepted-row recovery and page limits remain consumer work.
They must neither reselect a key per page nor use same-leg retry() to create a
new page. A native re-send after a failed/discarded page IS permitted: its
existing eligible dequeue derives a fresh root.leg, including when the cursor
is unchanged. Bitget's native 250/500/750ms retries and 40085 discard/re-queue
therefore retain one original root/snapshot while receiving new physical legs.
This is distinct from http_retry_context's same-leg physical resend; ReadLeg
exposes no retry(). Native budgets/classification/eligibility remain existing
shared/caller policy, not a new helper timer or retry loop.
Degraded-read policy, when applicable, is chosen ONCE before root admission;
later account-state changes cannot swap Primary/Secondary midway through a walk.
This helper supplies no Plan/Slot history authority or mixed-leg Plan binding.
Consumers retaining Plan ownership must wait for that owner's opaque seam.

First scope: signed plain command Read roots and physical children. Background,
Session, Execution and Plan roots are not admitted by this new constructor.
Standalone command/background single-leg APIs remain unchanged. Unsigned and
public history is OUT OF SCOPE: Bitget/OKX public candles and BloFin PublicTrades
have no selected Identity and keep plain admit_http_command plus
http_leg_context under their existing shared Read/history APIs. Do not route a
public read through command_root(Err(CredentialUnavailable)) or LocalRefusal.

## Dependencies and collision map

- #1962 must land first; integrate its ACTUAL source before implementation.
  Current open-PR map has 57 PRs and claims engine selected_read.rs and
  engine/tests/selected_read.rs exclusively through frozen #1962. Both are held.
  This phase creates only this plan and its shared README index row.
- #1945 is reviewed/frozen at `15df705c0b5`. command_wait() is a caller observation,
  not admission. Consumers obtain the genuine Fresh-entry wait before root
  creation: Remaining(ms) becomes the explicit root timeout; Expired goes to
  reject_expired_command(Read) with no admission; None grants no new authority.
  Missing/unavailable wait handling must follow the cause-aware core contract,
  not a guessed 120s default or native command-ID fallback.
- claude-fe7lgm's SEPARATE #1945 follow-up owns shared Context Read-root expiry
  and cause-aware wait refusal. No reviewed runtime pin is recorded yet. This
  is an implementation/consumer gate: helper must not implement a deadline
  ladder, timer, local expiry finalizer or inferred recovery cause to bypass it.
- hcmq3f owns G2/G2c trusted subject/configuration snapshot provenance. A private
  immutable selection snapshot here preserves existing Context generation and
  refs; it cannot create missing trusted account-subject/retirement evidence.
- cxxyqr owns ReadCompletion/history admission. Final root completion and
  accepted-prefix recovery remain there; no local Completion or Plan-to-Read
  cast. Coordinate exact physical leg/attempt history admission with its owner.
- Refresh open-PR and unpublished claims after standalone lands. Do not edit
  shared registry/lifecycle/entry/history sources or any consumer source in this
  slice. Shared docs retain incoming first and own appendix once.

## Proposed types and signatures for review

These are a proposal, not exported/frozen APIs. New contract controls and exact
signature review precede any runtime edit. Existing aliases refer to #1962.

```rust
pub struct ReadLegPolicy {
    pub timeout_ms: u32, // consumer declares 20_000 for these native page calls
    pub pacing: Option<Pacing>,
}
pub struct ReadRoot { /* private original Context + immutable selection */ }
pub struct ReadLeg { /* private child Context + same selection/root identity */ }

impl SelectedRead {
    pub fn command_root<P>(cx: &mut Cx<'_, P>,
        selected: Result<Self, CredentialUnavailable>,
        missing: LocalRefusal) -> Result<ReadRoot, CommandReadError>;
}
impl ReadRoot {
    pub fn context(&self) -> &Context;
    pub fn leg<P>(&self, cx: &mut Cx<'_, P>, policy: ReadLegPolicy)
        -> Result<ReadLeg, Error>;
    pub fn validate_delivery<P>(&self, cx: &Cx<'_, P>, leg: &ReadLeg)
        -> Result<(), Error>;
}
impl ReadLeg {
    pub fn context(&self) -> &Context;
    pub fn queue<P>(self, cx: &mut Cx<'_, P>, request: ReadRequest,
        recipe: impl FnOnce(&Identity) -> Box<dyn SignStep>,
        pending: impl FnOnce(Self) -> P) -> Result<RequestId, (P, Error)>;
}
```

ReadRoot/ReadLeg have private fields and no Clone. There is no constructor from
external Context, public root/leg cast, or leg()/retry() on ReadLeg. A prior child
cannot be supplied as a parent through this wrapper API. Borrowed context()
exposes existing core Context authority: these type restrictions do not prove a
holder cannot call generic Cx methods directly. Do not claim global non-derivability. command_root admits exactly once through genuine
command entry, reusing the standalone missing-selection diagnostic/refusal path.
It returns the original admitted-root wrapper, not a queued HTTP attempt. Its
Context is Read, with the chosen credential, root RetryNever and no root pacing.

Move the admitted SelectedRead into one private immutable shared snapshot (an
Arc is a representation proposal). Each leg retains that exact snapshot and its
original-root identity; no independently supplied Selection/Identity/role/slot.
The snapshot is not a new issuer, owner registry or G2 proof. Root lifetime is
existing engine authority, not Arc strong-count or Rust Drop behavior.

At the consumer's actual eligible dequeue, root.leg calls existing
http_leg_context(original_root, Policy{Read, original credential,
explicit native physical timeout, pacing, RetryNever}). It never derives from
previous_leg, inherits no earlier page's shortened bound, and caps each raw
child deadline at the immutable root deadline. Native queue eligibility and
pacing remain in their existing schedulers. No timer, allocator, reservation,
queue, retry scheduler or page-count budget is introduced here.

The proposed Error return is the existing typed issuance error, preserved
exactly. If fe7's reviewed cause-aware contract requires another public type or
validator, revise and re-review these signatures before source. Do not infer
root versus physical timeout from formatted text or invent a helper enum.

Queueing mirrors #1962: derive the recipe from the retained opaque Identity,
move the non-Clone leg into native pending, register optional streaming and
account-evidence tagging on the exact managed request. Queue refusal returns
that original pending, leg and exact typed error; it emits no synthetic root
success/failure or numeric terminal. Ordered recipe/demo headers and deferred
provider preparation remain unchanged. Native pending reconstruction must retain
all page cursor, freshness and row-admission fields.

validate_delivery first rejects an unrelated root/snapshot, then delegates to
the exact core root/leg HTTP delivery validator. That proves delivery custody,
not body validity or permission to publish malformed/native-refused data.
History progress/row admission still binds the exact physical Attempt via its
shared guard. Drop borrowed guards before mutable Cx, publication, retirement
or terminal operations as required by the history API.

The caller retains ReadRoot for the entire walk and binds ReadCompletion to its
original Context. Final root completion and child DATA admission are distinct.
Accepted-page cleanup uses existing attempt cleanup/pruning or exact child-leg
retirement, never done_context/fail_context on a child (those terminate the
command). Only the original root supplies logical completion. Child cleanup must
leave that root live; its typed done/fail terminal or fe7's reviewed root-expiry
operation must revoke all children through shared engine authority.
finish_http_context(root) only retires that root Context leg; it is NOT command
settlement or a substitute for cascading child revocation. Shared core determines
whether a failure is a live-root physical timeout eligible for accepted-prefix
recovery or an expired root that cannot recover. This helper chooses neither
native recovery nor completion, and supplies no local finalizer.

## Reviewed new-contract controls before runtime

Missing type/method compilation failures are NOT original RED evidence. Existing
http_leg_context already caps parent deadlines; preserve those GREEN controls.
This new type/custody boundary uses clearly labeled NEW-CONTRACT tests after
core and both consumer reviews. If an actual existing consumer drift bug is
found independently, commit its compiling semantic RED separately; do not create
a deliberately wrong reselecting fixture to manufacture one.

Use genuine Engine callbacks and real Prepare/provider/Bind/Claim. Proposed
controls (assertions reviewed before implementation):

1. Fresh caller-wait root creation, duplicate/foreign callback rejection and
   missing-selection diagnostic-before-one-terminal preserve #1962 behavior.
   Expired/None wait uses the reviewed core refusal, no Context/HTTP/Store effect.
2. Compile/API privacy controls prove only ReadRoot exposes wrapper leg
   derivation; ReadLeg offers no grandchild/retry method and no external Context
   can be wrapped. Borrowed Context still has existing generic Cx authority;
   these controls do not establish global non-derivability or global denial of
   child-based terminal methods. They are type/privacy controls, not semantic REDs.
3. Root lifetime 120s with 20s pages: page one queued at 0 has deadline20s;
   after page acceptance plus child cleanup (without root terminal), page two
   at15s has deadline35s, not20s. It has a
   distinct leg/Attempt but the same original owner/generation, credential,
   selected signing refs, egress and evidence. Root deadline never changes.
4. A late page with under20s remaining caps at the original raw root deadline;
   fractional bounds cannot be rounded up into renewed send rights. At root
   expiry no new leg/Prepare/provider/Bind/DATA can be admitted. Terminal/recovery
   assertions consume the actual fe7 expiry/cause-aware seam, not a test-local fix.
5. Change a fixture's available Slots/degraded flag after root admission: later
   pages still use the root snapshot. Two genuine roots with equal textual refs
   and numeric IDs in different engines cannot exchange legs. Root A cannot
   validate root B's physical response. No fabricated Context/Selection/Attempt.
6. Queue/preparation/provider refusal preserves native pending+exact error and
   leaves root versus leg settlement with the caller. Retirement of one completed
   child leaves the original root available for its next native page; retiring
   the original root through typed done_context/fail_context, or fe7's reviewed
   root-expiry operation, denies queued/late child work. Calling
   finish_http_context(root) is explicitly NOT this operation. Neither child
   cleanup nor losing a Rust wrapper is a command retirement primitive.
   Deliver a native re-detection answer (Bitget40085), discard its staged page,
   retire that child's attempt/leg, and re-queue through the existing native
   timer/eligibility path. Its next dequeue mints a fresh root.leg with the SAME
   selected snapshot and root deadline. Inject a late old response: no old rows
   publish, and during a genuine newer-leg delivery root.validate_delivery on
   the retained retired-leg wrapper refuses it. No fabricated delivery callback.
7. Streaming and non-streamed pages retain raw native row order, exact URL/body/
   ordered headers, selected provider refs and physical delivery identity.
   Early prefix, stale/duplicate data, cross-generation/late stream staging and
   first/later-page failure recovery use real shared history admission under
   its owner-reviewed contract. Distinguish a live-root physical Timeout from
   root expiry using the actual shared cause-aware path before any recovery.

## Execution and landing sequence

1. Commit plan/index on a fresh branch; report to overseer and request exact
   signature/new-contract review from uufub4, 4yjlw0 and ih35js. Preserve both
   frozen #1962 and #1947 heads while their single CI watchers finish.
2. Wait for standalone actual landing and reviewed expiry/cause-aware core pin;
   refresh claims, genuinely integrate actuals, then commit reviewed new-contract
   controls before source. No runtime or consumer edit while a seam is open.
3. Extend only selected_read.rs and isolated engine root/leg tests in the
   post-standalone owner window, preserving existing single-leg APIs. This slice
   has no venue/source/manifest prune and introduces no cross-cutting venue copy.
4. Focused engine, feature-off/on and affected-native/Binance gates, scoped
   strict Clippy/fmt/bans and both ratchets; three independent reviews plus core
   and consumer reviews; full exact-head WASM/replay/size CI. Local jobs=2, no
   local release WASM/replay/timing sweep. Record CALLERS/AUDITS/QUALITY only for
   implemented behavior and actual consumer consequences, not plan status.
5. Separate engine-window PR after #1962; merge only after its own green CI,
   actual integration, fresh separate own-file/full-incoming guard and a
   head-matched merge. Consumer adoptions follow under native-owner review.

Architecture gate: one shared root/leg composition using existing engine
custody. No local selection policy, expiry finalizer, history settlement,
credential fallback or new owner/scheduler machinery.

Core review at c06c is bounded direction/API-fit only. The exact constructor
signature must compose fe7's reviewed receipt/root-expiry seam before source;
new-contract tests proceed only at that reviewed dependency pin. Wrapper privacy
and borrowed low-level Context authority are separate claims.

Consumer review at c06c (4yjlw0): signature direction fits Bitget's Request{root}
plus pending leg and query_item streaming. The subsequent native re-send,
unsigned-history boundary and 40085 discard/re-queue controls above refine the
proposal; core/consumer review of the exact final dependency composition remains
required. No runtime or new-contract execution follows from that review alone.

## Actual dependency composition (implementation checkpoint)

Integrated actual landing87bf458ba90 (contains actual1962) and the explicitly
authorized local ce54ab9357b root-expiry source by plain merges. Source merged
without conflicts; docs retain current landing plus dependency additions.
#1976 is not frozen, so this is local implementation evidence only.

The exact command_root signature above now omits ReadRootPolicy. It calls
Cx::admit_caller_read_root(selected credential, None), which captures the genuine
raw receipt deadline itself without rounding or caller override. The caller
observes command_wait before admission and uses shared reject_expired_command
or reject_unstamped_command when appropriate. Rejected is already terminal;
Revoked grants no fallback. Missing selection uses the same None-credential
tracked root, optional checked diagnostic and single typed fail as the standalone.
ReadRootPolicy and a guessed remaining-wait conversion are not exported.

Only selected_read.rs, isolated selected_read_legs.rs controls and owned docs
change in this slice. Root and children share one immutable Arc<SelectedRead>:
no new identity selector, owner registry, timer, local expiry or finalizer.
New children are Read/Never, with explicit physical policy, derived only from
the original root. ReadRoot::validate_delivery first checks Arc snapshot
identity, then delegates exact root/leg/attempt custody to the core validator.
The borrowed Context remains existing generic core authority.

Tests are NEW-CONTRACT specifications committed before source, not missing-API
compile failures claimed as original REDs. They cover exact fractional receipt,
page-relative 20-second bounds, immutable selected provider refs/route/headers,
physical versus root timeout, diagnostic/refusal custody, wrong root/engine,
discard-and-resend with stale delivery, pending preservation and streaming.
Shared history admission remains core-owned; no local accepted-prefix finalizer
is introduced. No production consumer source or manifest is changed.
