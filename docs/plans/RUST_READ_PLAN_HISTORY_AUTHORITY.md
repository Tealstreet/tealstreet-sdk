# Command-owned Read-Plan history authority

Plan only, 2026-10-24. Owner codex-uufub4. Required reviewers are
claude-fe7lgm (Plan ownership and terminals), codex-cxxyqr (history admission and
recovery), and consumer codex-zs5e83. Base: actual landing `ce278e0b4a9`.
This checkpoint changes documentation only; signatures below are provisional.

## Problem and scope

`ReadCompletion::from_context` admits the original Context-phase Read root.
`validate_completion_as` deliberately refuses Plan-phase owners. A command-owned
Read Plan therefore has no history authority coupling its accepted rows to its
exact Plan, Slot, Context and released Attempt. Casting a Plan leg to the old
Read root, or retaining numeric Completion, would escape Plan retirement.

Provide one bridge on the existing Plan owner and history algorithms. The first
slice accepts only command-owned Read Plans carrying #1949's exact receipt cap.
Mutation, physical Execution, Session and Background Plans are excluded. Existing
plain-root history remains unchanged. Native declarations, page/retry budgets,
selection, cursor/range geometry and pacing are unchanged.

Lighter's funding/trade walks need this bridge. Paradex additionally waits for its
Bearer/session prerequisites; BloFin background history waits for G1/G2 and is
not admitted through this command-only slice. No consumer migration is included.
A consumer audit at `17e93c3038e` confirms that all intended Lighter remote
history families enter through the genuine Command::Query callback. The current
numeric Completion retains no receipt cap: adoption must call
`begin_preflight_with_command_wait` there before any page, signing or wait and
retain that exact cap across Trades/Funding stage changes. Missing or expired
receipts use shared refusal; no uncapped or default-timeout exception is allowed.

## Existing authority and dependencies

- Compose runtime with frozen #1919 `2f4a08b449c`, including its prepared terminal
  funnel and owner lookup. Use `owner_entry(plan.owner())`, never a command-id
  map lookup or a second owner registry. That lookup is not on this document's
  landing base: the runtime branch must genuinely stack on #1919 first.
- Every terminal uses `prepare_plan_terminal` and `commit_plan_terminal`.
  #1894's `done_mutation_plan_after_data` supplies the precedent for exact DATA
  anchoring and retirement of an unissued remainder; it grants no Read cast.
- The landed `done_read_plan_after_data` already defines the early eligibility
  rule: reject with State if another entry has a live call or Credit::Issued,
  and retire only the unissued remainder. Reuse that rule, not its current
  settle-before-seal ordering, for the new bound-history terminal.
- Preserve #1896's readiness-owner validation. A callback cannot borrow a
  different ready command, delivery or Plan to admit rows or finish.
- The live caller bound is #1949's private `Body.receipt_cap` and the Plan's
  immutable deadline. #1976's tracked Context roots exclude Plans: do not create
  a parallel tracked root or copy its timer. Existing Plan expiry remains strict.
- Closed #1928 proposals supply custody invariants only. Its obsolete fixture
  and unmerged runtime are not a dependency or evidence of an implemented API.

The shared entry shim, G1 and G2b have other owners. This plan claims no runtime
hunks until the reviewers agree on the exact history, Plan-terminal and callback
proof patches. In particular, preserve their admission and release transactions.

## Proposed public shape

Add a Plan-bound authority alongside the existing Context-bound authority:

```rust
PlanReadCompletion::from_plan(cx: &mut Cx<'_, P>, plan: &Plan)
    -> Result<PlanReadCompletion, Error>;

// Parallel engine-owned history types reuse the existing row/cursor/range logic.
PlanReadHistoryRows<T>::admit(&mut self, cx: &mut Cx<'_, P>,
    slot: &Slot, context: &Context) -> Result<PlanHistoryPage<'_, T, P>, Error>;
```

The constructor validates issuer, exact owner/generation, Read operation,
retained receipt cap and genuine Plan activation/handoff custody. On success it
sets a one-way history-binding flag on the existing Body and returns one opaque,
non-Clone history authority. It spends no IDs and creates no journal or second
registry. A second binding or unsupported/stale origin refuses without effects.
The command is derived from the validated owner, not selected by the caller.

A Plan handle is Clone: consuming a handle cannot enforce history ownership.
Once bound, all public terminal routes (`done_plan`,
`done_read_plan_after_data`, `fail_plan`) refuse outside the history funnel,
including when given an earlier clone. Public `finish_plan_call` also cannot
settle a history-bound page independently of acceptance. Enforce this on the
shared validation paths, not just in the new wrapper. Ordinary send/selection
rights remain subject to their existing Plan checks. Private history rejection,
default failure and engine expiry retain exact validated failure-only access;
there is no public unchecked terminal or way to unbind the Body. Preserve
`fail_plan`'s existing successful no-op while `http_plan_expiring` is set: native
failure callbacks may unwrap it during the engine sweep. The binding check must
not turn that inert call into a panic or an independent seal.

The page guard exclusively borrows both history and mutable Cx, with no Cx
accessor. Its private binding contains Plan identity, Slot identity, Context and
the engine-observed current Attempt. Admission requires exact live DATA delivery;
status, a Context clone or a venue-supplied Attempt alone cannot establish it.
Row, identity and coverage operations reuse the current history algorithms.
Cursor/range adapters have the same guard boundary rather than an unguarded
alternate staging path. Exact spellings and lifetimes require API review.

## Atomic page acceptance

Staging belongs to one exact page binding. A failed page, reset, retry or fresh
attempt discards staging and its tentative identities/coverage. Accepted rows
from earlier pages remain owned by the same Plan history state. A sibling,
superseded attempt or reused Slot cannot mutate either set.

Page acceptance prepares row/identity/coverage storage and validates both history
state and `finish_plan_call` eligibility before any commit. A private move-only
prepared page then commits accepted rows and settles that Slot synchronously.
No callback, Cx access, await or fallible allocation separates those effects.
A preparation refusal leaves accepted rows, staging, Slot and credits unchanged.
Unexpected unwind during the commit poisons the engine.

The ordinary nonterminal operation accepts a page and settles its Slot together.
The terminal DATA operation is distinct: it prepares the final accepted page and
calls `prepare_plan_terminal` BEFORE any `finish_plan_call` effect could release
its DATA anchor, settle the Slot or consume credit/handoff state.
It retires unissued suffix contexts/credits in that same terminal transaction.
Early completion refuses if an issued sibling remains unresolved; it consumes
neither that sibling's credits nor its delivery authority. The exact eligibility
and retirement validator is a required Plan-core source review point. No
schedule or credit geometry may be shrunk to evade it. This is a direct cutover
for bound history, not a wrapper around the existing Read terminal: that helper
currently calls `finish_plan_call` before seal preparation. Unbound Plan callers
retain their existing APIs; a bound history consumer cannot select that path.

## Transport-only accepted-prefix recovery

Add one private Io-scoped failure proof, constructed only from an exact released
managed Read attempt's Network failure or physical lifecycle Timeout. Bind the
proof to issuer, command generation, Plan, latest Slot, Context and Attempt.
The actual engine source of failure determines eligibility, not VenueError text
or its kind alone. A prepared-but-unreleased request supplies no proof.

Record a private typed cause on the exact Plan Call, alongside its current
Attempt. `Stage::Active { failed: bool }` is insufficient. A lifecycle helper
records ReleasedNetwork on the admitted `Event::HttpFailed` path before
`with_owned_read_failure` lends the callback. `expire_http_timer` records
ReleasedPhysicalTimeout only after observing an actual lifecycle release.
The callback-scoped witness is minted from that recorded cause and exact call.
`http_preparation_failed` records an unreleased failure even when its error kind
is Timeout; a logical-deadline-derived Timeout in failed-call settlement never
becomes a physical proof. The shared HttpFailed tail also receives integrity
Internal failures: only the exact `io.http_failure_kind == Network` path may
record ReleasedNetwork. Record ReleasedPhysicalTimeout in expire_http_timer
before event dispatch so a shared tail cannot overwrite it. Neither site records
recovery authority while `http_plan_expiring` is set, including when the Plan
sweep enters the released physical-timeout branch. Root expiry therefore stays
strict independently of a later deadline check. Integrating these recording
sites is a source gate.

Preparation, provider, integrity, native status/codec failure and root expiry
remain strict even after accepted pages. HTTP408/5xx classification does not
become a physical transport proof. The current raw time must be strictly before
both the Plan deadline and retained caller deadline. Equality is expired; an
already admitted Plan is not expired early merely because less than 1ms remains.

The proof is available only in its exact typed failure callback and is cleared
on callback exit and unrelated/nested callback boundaries. It cannot be cloned,
stored by a venue, borrowed by a readiness callback or reused after settlement.
Every consuming operation revalidates current generation, latest attempt and
caller liveness. It grants completion preparation only, never send or retry.
Install that proof and invoke the exact Read failure callback before the default
Read-Plan failure tail can auto-seal. This adds no Mutation independent-failure
policy and lends no broad callback or registry authority.

While the exclusive guard owns accepted storage and Cx, mint a private non-Clone
AcceptedPrefix witness only from a nonempty accepted Vec, bound to the exact
Plan. Pending/staged rows and serialized output cannot mint it. Recovery consumes
both that witness and the exact released-failure proof into a prepared terminal.
Empty accepted history fails strictly. Native final shaping may filter accepted
rows; do not reparse JSON or require a nonempty output string to prove custody.

## One terminal transaction

Prepare accepted-row ownership, encoding, optional INCOMPLETE marker, all output
storage and Plan retirement before committing. Encoding errors fail strictly
before a marker or success can be published. Keep the existing engine-owned
history encoding policy; add no venue terminal/finalizer callback and no public
prepared token. A pure typed encoder cannot access Cx or complete an owner.

The prepared terminal becomes another input to the existing Plan funnel. Commit
the one correlated marker and DATA terminal, exact Slot/remainder retirement and
history consumption under the same poison scope, with no fallible work after
the first committed effect. No second emitter or numeric fallback is permitted.
Strict rejection also uses that funnel and discards staging. Root expiry emits
no accepted rows, INCOMPLETE marker or successful encoding/finalization.

## Evidence and implementation gates

1. Obtain Plan-core and history API review of this document before runtime edits.
2. Commit original assertions separately. Drive real Plan admission and managed
   Prepare/Bind/Claim. Keep the old Plan-to-ReadCompletion refusal as a passing
   compatibility control, never as a desired cast. A labelled old numeric-history
   adapter can expose late marker/terminal publication after a real Plan seal;
   it is an unsafe existing-API control, not a migrated native consumer. Missing
   methods and compilation failures are not RED evidence.
3. New-contract tests accompany the reviewed bridge: exact page admission and
   atomic Slot settlement; wrong issuer/Plan/Slot/Context/Attempt; stale and
   duplicate delivery; staged-prefix reset; allocation refusal with no effects;
   early DATA completion with unissued suffix and exact credit reclamation.
   Pin all three cloned-handle terminal denials and independent Slot settlement
   denial after binding. A refused terminal preparation after DATA must retain
   the Slot, credit, leg, handoff and accepted/staged history unchanged.
4. Independently exercise released Network and physical Timeout recovery with
   nonempty accepted storage, one marker and one terminal. Pair each with empty
   accepted storage, staged-only data, unreleased/provider/preparation/native
   status failure, unrelated/nested callback, latest-attempt and same-id reuse
   denials. Keep valid live-root recovery as the working control.
   Separately deny an engine-stamped logical-deadline Timeout and an unreleased
   preparation Timeout; neither may be mistaken for the recorded physical cause.
   Deny an integrity Internal HttpFailed after accepted pages too.
5. Pin caller/Plan expiry in both timer orders, fractional predeadline liveness
   and equality, late DATA/Bind, finalizer-count zero on expiry versus one on
   valid recovery, and no marker outside exact command membership. In both root
   timer orders, a native callback unwrapping fail_plan during the expiry sweep
   must remain inert and successful. Check the
   optional final shaping that filters all accepted rows without losing proof.
6. Test strict typed encoding refusal, pure guard refusal, reserve-all terminal
   storage, panic/fail-stop behavior, and unchanged Context history. Review exact
   runtime source with fe7lgm/cxxyqr before caller adoption. Run scoped feature-on
   and feature-off controls plus strict checks; fresh CI owns complete suites
   and family size/replay gates. No venue inventory is pruned by this engine PR.
