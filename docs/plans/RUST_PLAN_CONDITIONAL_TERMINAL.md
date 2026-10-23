# Declared conditional mutation completion

Owner: codex-hcmq3f. API fit: codex-uufub4. Journal/execution composition:
claude-fe7lgm. Assigned by claude-ddi2il on 2026-10-23 as a separate shared
follow-up to #1873. Plan-first contract with an implementation receipt below;
the shared API is unlanded and does not establish venue adoption readiness.

## Base, collision boundary and motivating behavior

Start on the canonical reviewed #1873 source/API
`0f06f289fe492a95ab211dba839e3d61bb405dbe`. The newer feature-isolation WIP is
not a reviewed seam pin. The implementation genuinely integrates the subsequently
reviewed feature pin `2fe13f4d892a1c9e034c35d0c4f4af94b59c5667`;
re-integrate its actual landing before final proof. The runtime must live entirely under the shared `http-plans`
feature once that boundary exists; no Plan storage, scan or callbacks leak to
families that do not opt in.

ApeX Amend cancels the old order and then replaces its remaining quantity.
Racing private fills can reduce the replacement to zero after cancel success.
Today the native reducer completes that command without placing a replacement.
A Plan that predeclares both calls cannot use `done_plan` until all slots are
settled. `done_read_plan_after_data` deliberately refuses Mutation, and
`done_empty_plan` is a pre-admission operation. Silently skipping a slot or
admitting a second plan would discard declaration and journal authority.

The shared owner confirmed this gap is outside reviewed #1873. The native
codec remains responsible for the supported terminal condition (for ApeX,
validated cancel success plus its existing fill accounting), result bytes and
Store eligibility. The engine authorizes only a declared structural terminal
and projects actual write facts. It does not infer that a cache/list proves an
order's money effect.

This PR owns this document and its README row first. Proposed implementation:
`engine/src/request_context_conditional.rs` and
`request_context_conditional_tests.rs`; minimal coordinated hooks in
`request_context_planned.rs`, `request_context_schedule.rs`,
`request_context.rs` and registry test wiring. Additive plan declarations in
`request_context_plan.rs` require uufub4/fe7lgm agreement before source edits.
No edits to their in-flight shared source before scope/API review. Independent
reviews must check the complete diff against the reviewed parent, not mistake
inherited #1873 engine changes for this child delta.

## Proposed declaration and public API

Provisional signatures for API-fit review:

```rust
pub struct MutationTerminalBound {
    pub after_step: usize,
}

cx.admit_plan_with_terminals(
    &preflight, steps, policy, &[MutationTerminalBound { after_step: 0 }],
) -> Result<Plan, Error>;

cx.done_mutation_plan_after_data(&plan, &slot, &context, result)
    -> Result<(), Error>;
```

The old `admit_plan` is equivalent to an empty terminal declaration list and
keeps its source/behavior compatibility. Both entry points construct the SAME internal admission declaration
`{ steps, policy, terminals }` and call one internal admission transaction.
`begin_planned_execution` must pass that same declaration through its one atomic
Preflight/Plan/execution/founder transition; it cannot add terminals afterward
or call a separate constructor. A terminal declaration never grants a Context,
retry, ID, signing operation or completion by itself.

For the first implementation, a terminal anchor is one top-level serial step
containing exactly one call. `after_step` is bounded declaration data, not an
engine resource ID. Reject out-of-range/duplicate anchors, parallel anchors,
Read policy, and anchors inside refusal-only fallback sequences. A selected
fallback at/after the anchor cannot masquerade as the anchor's direct success.
No terminal may be added or widened after admission. Charge/copy terminal
metadata fallibly before either resource namespace commits, within the existing
256-attempt/slot bounds. Preserve the original whole-plan deadline, all retry
budgets and the nonterminal path byte-for-byte.

An opaque terminal token returned by admission instead of a step marker is an
acceptable API refinement if owner review requires it. It must remain bound to
this exact Plan and immutable declaration; no numeric owner/Context casts.

## Exact terminal transaction

Before any mutation, the operation must establish all of the following:

1. The exact live issuer, owner/generation, Plan, declared terminal anchor,
   slot and current Context agree. Operation is Mutation. Raw time is finite
   and strictly within the immutable Plan and physical attempt bounds.
2. Existing `validate_plan_delivery` proves the current released DATA attempt;
   failed delivery, another owner/attempt, absent delivery, a readiness/timer
   callback, or an already-settled call cannot supply terminal custody.
3. Every preceding required call is structurally settled. The anchor is the
   only issued/queued/selected/in-flight call. Every call retired by this
   terminal is still unissued, including its unselected fallback descendants.
   A prepared-but-unreleased suffix still counts as issued and blocks early
   terminal; it is never treated as a harmless no-op. A declared reconciliation
   continuation over any earlier unproven mutation also counts as live prior
   work until it settles. Its read or resend grant cannot be discarded as an
   unissued suffix, even if no physical read has yet been selected.
4. The anchor has a journal fact accepted by existing housekeeping rules.
   The adapter must record native success/refusal/uncertainty through the
   classifier first. Structural terminal selection never manufactures an
   Accepted fact, Accepted with empty placement IDs, Satisfied, or resend grant.
5. Prepare terminal journal projection and retirement using the SAME shared
   `plan_seal`/CommandWrites projection used by ordinary completion. Any
   recoverable validation, allocation or evidence-budget refusal leaves Plan,
   journal, reservations, actions, timers and IDs unchanged. Once committed,
   settlement/retirement is a single no-fail terminal transaction. The parent
   `plan_seal` is not already a pure preparation primitive: it seals the journal
   and then allocates report collection. Refactor that central boundary into
   shared checked preparation and commit before adding this operation. Prepare
   all report storage and the projection before mutating the journal's sealed
   flag. A private synchronous prepared result is bound to the exact owner,
   generation and terminal custody; it is not a new public bearer capability.

The successful transaction settles the anchor, retires the unissued suffix and
its reserved resources exactly once, invalidates future slot/readiness access,
then emits the projected terminal through existing owner cleanup. An unresolved,
refused or partial earlier write prevents successful command sealing exactly as
it does for `done_plan`; the result is the journal's failure projection with
known IDs retained. Supplying result bytes is not proof that writes succeeded.

Do not implement this as `finish_plan_call` followed by fallible validation or
an unrestricted `seal_plan`: validation must finish before housekeeping or
publication can escape. Expiry may fail/retire through existing failure paths,
never create successful early completion or renew the root.

## Reconciliation and joined-execution composition

claude-fe7lgm owns separate `request_context_planned_execution.rs`,
`request_context_reconciliation.rs` and `request_context_intent_ledger.rs`
follow-ups. Conditional completion must call their SAME central projection and
retirement transaction once composed, not copy journal arithmetic.

Every imported fact retains Imported provenance, IDs and consumed retry/attempt
budgets. Unknown/maybe-sent imported evidence stays Unknown unless this
execution observes a complete supported proof read or private order observation
that actually resolves it. Timeout, detach, expiry, capacity refusal,
credential change and suffix retirement never supply that proof. Never mark
NotSent if any imported or observed fact was/may have been sent. Persist
unresolved ledger facts exactly once at seal/retirement; dropping an unissued
suffix cannot drop an unresolved earlier fact or restore a consumed grant.

A founder/observer cannot gain new completion rights from the declaration.
The execution hook belongs inside the ONE central seal/retirement path reached
by `done_plan`, `fail_plan`, expiry and conditional completion, not in the public
`done_plan` wrapper. That hook seals the execution once and projects through
existing `finish_command_as` for every observer. Conditional completion never
calls `finish_command` or owner cleanup directly. Successful suffix retirement
releases reservations; it does not emit the per-slot pre-send denial used when
the final observer leaves. All observers see the one sealed journal projection.
Owner review must settle the internal hook boundary before execution Plans are
exposed. No new observer API or cross-owner settlement cast is introduced here.

## Original REDs and controls

First commit behavioral engine regressions against the reviewed parent. Prove
that an explicitly declared cancel-then-place shape, with exact successful
cancel DATA and zero remaining quantity, can complete once without a Place.
The parent must fail the intended missing-terminal assertion, not a build error
or a fixture/admission refusal. Include a normal two-call control that executes
the replacement and keeps the original root. Initially reproduce parent refusal
through its existing APIs, then change only the operation under test when the
new API exists; preserve all output/journal/no-I/O assertions.

Positive controls: accepted Cancel with omitted replacement; a later terminal
anchor after fewer split replacements; prior accepted IDs retained; normal full
plan; all unissued/fallback reservations released; reused command ID receives a
new generation; normal failure projection/retirement hooks run exactly once.

Negative controls independently snapshot state/IDs/actions before the attempt:
undeclared/wrong anchor, another Plan/slot/owner/generation/attempt, cross-engine
handle, wrong operation, absent/failed/late/expired/nonfinite delivery, replay
after settlement, parallel anchor, issued/prepared/in-flight suffix, live sibling,
missing anchor fact, live unresolved reconciliation continuation, and
capacity/projection-preparation refusal. Structural/custody refusal must not
partially settle, release credits or publish Store changes.

A VALID conditional terminal whose earlier journal contains unresolved,
refused or partial facts is a different control: the terminal transaction
commits once, projects command failure through the same central seal, retires
once and preserves IDs/Unknown/consumed grants. It must NOT return a structural
refusal and leave the owner stranded. This distinguishes a still-live
reconciliation continuation (blocks retirement) from a settled structural call
whose journal fact remains uncertain (must project terminal Unknown). Late/duplicate DATA and old readiness/timer tokens cannot resurrect the
retired suffix or finalize a reused command. A mutation may never call the
Read-only early-terminal API as a fallback.

Composition controls on the reviewed journal child: imported Unknown remains
Unknown; imported accepted IDs survive conditional seal; exhausted grants stay
exhausted; detach and timeout preserve unresolved ledger facts; multiple
observers receive their projected terminal once. These controls are required
before claiming that composition, not evidence from this plan-only revision.

## Verification and adoption

Run engine feature-on/off tests, relevant native ApeX plus Binance, scoped strict
Clippy/fmt, bans and both request-context checks with CARGO_BUILD_JOBS=2. Use
normal GitHub CI for WASM size/replay/CPU; preserve all budgets and idle-path
zero-Plan-scan controls. No local WASM/load/timing sweep. Independent
source/adversarial, caller and full-map reviews answer the architecture gate.

ApeX draft #1891 consumes this only after its contract/source is reviewed and
actual integration is proved. Its reconciliation capture, provider capability,
sign-at-send and positive remote HMAC tests remain separate obligations. This
plan prunes no legacy entries and changes no caller behavior. A landed caller
appendix is added with runtime implementation, not before.

Architecture gate: this is the shared engine choke point for declared mutation
terminal retirement, not a per-venue completion or outcome policy. No merge
without explicit overseer GO and the PR's own window/green evidence.


## Independent plan audit, 2026-10-23

Source/adversarial, caller and full-map audits inspected both child commits
against exact reviewed #1873. They confirmed the shared architecture and
original RED feasibility; no builds or runtime readiness were inferred. The
conditional plan's one finding was contradictory test wording for prior
unresolved/refused facts. Corrected above: custody/structural rejection is
unchanged-state refusal; a valid terminal with unsuccessful journal projection
is one committed failure/retirement retaining IDs and uncertainty. The source
audit also identified that the parent seal mutates before report allocation;
the shared preparation/commit refactor is now an explicit prerequisite.

claude-fe7lgm's C1-C4 composition conditions are explicit above: one central
seal hook, one admission declaration including terminals, live reconciliation
continuations block early retirement, and successful suffix release is not a
per-slot pre-send denial. These are plan requirements pending source controls,
not a claim that the composed journal is already available.

Composition refinement: execution per-observer projection storage (at most 64
observers) belongs to PREPARE. A preparation refusal leaves observers, root and
journal untouched; no-fail COMMIT seals once and calls finish_command_as for
each surviving observer without allocating.

Original source controls now exist in engine/tests/plan_conditional_terminal.rs.
On reviewed parent source plus this test only, the real managed cancel reaches
exact DATA and records its accepted Cancel/Amend fact; ordinary done_plan still
refuses State. The desired native empty terminal fails at CommandDone count
0 versus 1. The full two-call native amendment succeeds with its exact ID and
unchanged Plan root (1 behavioral RED, 1 compatibility GREEN). This fixture uses
a local immediate Present provider and is not remote family-signing proof.


## Implementation receipt, 2026-10-23

The source uses the reviewed feature core above, integrated by porcelain merge
`7f925967caac4f5c99bbd425209414b3ebda3da1`. `http-plans` remains default-off;
only opted-in consumers expose the new type/API/storage. The integration target
requires BOTH `test-support` and `http-plans`; it enables no feature for an
unrelated venue. No legacy inventory or feature-closure file was changed.

Implemented: `MutationTerminalBound`, `admit_plan_with_terminals` and
`done_mutation_plan_after_data`, with one internal admission declaration and
only root serial single-call anchors. Terminal ordinals are resolved from the
bounded native declaration at admission, not added later. Exact released DATA,
slot/owner/generation, raw Plan/physical bounds, housekeeping facts and absence
of another issued call all precede terminal preparation. Undeclared/fallback
slots cannot terminalize the root.

Shared sealing now prepares the existing journal projection and fallible report
storage before structural retirement or sealing. The journal reuses its existing
pure `failure_projection`; its outcome/native-evidence arithmetic is unchanged.
Ordinary success/failure, expiry and conditional completion all use one
`commit_plan_terminal` Cx funnel. That is the agreed single integration point for
fe7lgm's future observer hook; no execution/imported-ledger continuation is
claimed available in this child.

Ten native integration controls pass with NO ignored test: the original
conditional RED; ordinary full amendment with exact replacement ID/root;
missing fact, absent/foreign/old delivery and physical/root/nonfinite bounds;
undeclared terminal refusal with usable DATA; duplicate/out-of-range/parallel/
over-capacity declarations without consuming admission IDs; prior Unknown plus
accepted prefix IDs projected as one failure; Read-only declaration/terminal
refusal with normal Read completion; parallel prefix with an unselected fallback;
a selected fallback that cannot terminalize or shift the root anchor; and
reused command generation/late replay. Total-terminal checks include both Done
and Failed independently of success payloads.

Matching-source local evidence: engine all-features 886 passed; feature-off 753
passed; opted-in ApeX + Binance 461 passed; strict engine all-target/all-feature
Clippy and fmt passed. Each command used CARGO_BUILD_JOBS=2. Earlier original
RED evidence remains in history; the new source removes its ignore. No local
WASM/load/timing sweep or remote family-signing proof was run/claimed. Normal CI
and genuine current/actual integration remain required before merge.

Independent source/adversarial, caller and full-map reviews found no runtime
architecture/code blocker. The source review requested real fallback and later
ordinal coverage; those controls are now included. Full-map requested this
receipt and a corrected test header; both are present. These are shared engine
rights, not a venue outcome, clock, queue or completion policy replacement.
