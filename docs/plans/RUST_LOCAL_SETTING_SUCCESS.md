# Genuine-entry success for local fixed settings

Owner: codex-uufub4. Plan only; runtime/API review and original REDs precede
implementation. Consumers: codex-9czdjh; reviewer: codex-cxxyqr.
Base: actual Rust landing `6a6845c449104130043055f5d03273f910eab80e`
(#1976 landed). Native branches audited at `6c7f4fcf292d` remain unchanged.

## Problem and scope

A Mutation Context declares a write journal. `CommandWrites::validate_success`
truthfully refuses an empty attempt list, so admission followed by `done_context`
cannot represent a successful local setting that never sends a request.
Authenticated empty plans already use `done_empty_plan`; that path requires a
real write credential and remains unchanged.

The missing case is a native fixed-setting success before credentials are
checked. Preserve these exact behaviors, including keyless/public-only setup:

| Native branch | Store projection | Result |
| --- | --- | --- |
| KuCoin Spot `SetLeverage` | Update an existing symbol setting to leverage 1, buy/sell leverage Some(1); ignore the requested leverage as today | `[]` |
| KuCoin Spot `SetMarginType` / `SetPositionMode` | None; preserve the successful native no-op policy | `[]` |
| Bitrue `SetPositionMode(false)` | `set_hedged(false)` | `[]` |

KuCoin anchors are `spot/trading.rs:369-378,395-398`, before its credential
gate. Bitrue is `trading.rs:841-857`, before planning/signing; `hedged:true`
keeps its existing Unsupported/NotSent refusal. These are three lexical success
sites. No remote request, signer, retry, Read, group or session conversion belongs
in this slice. 9czdjh's adjacent refusal work leaves these sites held unchanged.
The same fixed native branches still succeed when credentials are configured;
this API does not select or inspect credentials.

## Proposed smallest shared API

Provisional spelling, subject to cxxyqr's API review:

```rust
Cx::done_local_setting(
    &mut self,
    result: String,
    project: impl FnOnce(&mut Store),
) -> Result<(), CommandEntryError>
```

This is immediate completion in the current genuine command entry. It accepts
no command id, Context, credential, caller generation, operation override or
claimed write fact. The native venue selects a known local setting branch;
the engine additionally restricts it to the current Setting mutation intent.
It must work with `http-plans` disabled; no Plan feature or credential is needed.
`Ok` means the projection ran and the success terminal was emitted, rather than
meaning admission merely succeeded. Rejected owns the sole typed initial
refusal; Revoked performs no projection or second terminal.

The closure has Store access only, no Cx, completion or engine I/O authority.
Prepare native data/result before calling it; the projection is synchronous and
infallible. Existing engine poison behavior applies to an unexpected panic;
this is not a rollback transaction or fallible Store-update facility.

## Authority transaction

1. Validate a finite callback time, the exact Fresh entry/resource custody,
   Setting intent and absence of any existing owner, journal, released work or
   borrowed delivery/readiness authority. Duplicate or stale custody refuses
   before touching Store. Reuse the shared entry matching rule.
2. Prepare owner capacity, one fresh generation identity and terminal storage
   before any projection. Do not create a Context, Preflight, physical request,
   timer, attempt, Accepted fact or `CommandWrites`. Resource refusal follows
   the existing typed Mutation initial-rejection path (NotSent).
3. Bind a private local-completion origin to that exact command generation in
   the existing command registry. Suspend the entry loan before projection;
   expose no reusable public completion token or retained handle.
4. Run the Store-only projection once and immediately consume the private
   origin through the existing command terminal funnel. Finish the exact
   receipt and order-lock ownership through the same terminal machinery.

Exact Rust storage/phase and reservation hooks require source review before
editing held entry/registry files. Do not weaken `validate_success`, generalize
`Seal::Success` for arbitrary empty journals, or add a separate owner registry.
The trusted local origin cannot finish a bound physical mutation or an observer.
Initial hook scope is `request_context_entry.rs`, a private helper in the existing
command registry, and dedicated engine tests. Any extra shared hook goes back
through review before source edits.

## RED-first evidence and implementation gates

- Existing-API engine control: genuine Mutation admission, no attempts,
  `done_context("[]")` is currently refused. Keep a real released/Accepted
  mutation success as its working control. No missing-method compile failure.
- Native working controls pin each keyless/public-only branch, exact return,
  current projection and zero HTTP/signing actions before cutover.
- New-contract controls, labeled separately: Fresh local Setting success;
  exactly one projection before exactly one terminal; duplicate live id and
  reused generation isolation; borrowed HTTP/Plan/Preflight/socket/timer
  callbacks denied; spent/Bound entry and non-Setting intents denied; capacity,
  identity exhaustion and invalid clock refuse before projection.
- Assert no managed request, Context, timer, journal or attempt is manufactured.
  Preserve native projections on success and Store unchanged on every refusal.
- Run focused engine controls with optional features off and on; preserve the
  authenticated `done_empty_plan` control. Then adopt the three native sites in
  coordinated consumer hunks and prune only their exact legacy inventory.

This draft introduces neither runtime nor consumer changes. First review the
entry-only API and exact generation/seal hook list with cxxyqr and 9czdjh;
commit original REDs before implementing the shared seam.
