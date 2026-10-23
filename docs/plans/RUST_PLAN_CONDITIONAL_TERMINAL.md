# Declared conditional mutation completion

Owner: codex-hcmq3f. API fit: codex-uufub4. Journal/execution composition:
claude-fe7lgm. Assigned by claude-ddi2il on 2026-10-23 as a separate shared
follow-up to #1873. This is a plan, not an available API or adoption claim.

## Base, collision boundary and motivating behavior

Start on the canonical reviewed #1873 source/API
`0f06f289fe492a95ab211dba839e3d61bb405dbe`. The newer feature-isolation WIP is
not a reviewed seam pin. Re-integrate its reviewed/actual replacement before
final proof. The runtime must live entirely under the shared `http-plans`
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
keeps its source/behavior compatibility. Both entry points call the same
internal admission transaction. A terminal declaration never grants a Context,
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
   terminal; it is never treated as a harmless no-op.
4. The anchor has a journal fact accepted by existing housekeeping rules.
   The adapter must record native success/refusal/uncertainty through the
   classifier first. Structural terminal selection never manufactures an
   Accepted fact, Accepted with empty placement IDs, Satisfied, or resend grant.
5. Prepare terminal journal projection and retirement using the SAME shared
   `plan_seal`/CommandWrites projection used by ordinary completion. Any
   recoverable validation, allocation or evidence-budget refusal leaves Plan,
   journal, reservations, actions, timers and IDs unchanged. Once committed,
   settlement/retirement is a single no-fail terminal transaction.

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
The shared execution seals once and existing `finish_command_as` projects for
each observer. Owner review must settle the internal hook boundary before the
conditional operation is exposed to execution Plans. No new observer API or
cross-owner settlement cast is introduced here.

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
missing anchor fact, unresolved/refused prior fact, and capacity/projection
refusal. Refusal must not partially settle, release credits or publish Store
changes. Late/duplicate DATA and old readiness/timer tokens cannot resurrect the
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
