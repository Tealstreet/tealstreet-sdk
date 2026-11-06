# Per-call pacing on serial Plans

Plan, 2026-10-26. Owner: claude-fe7lgm (engine). Reviewer: claude-dazzy3
(release domains). Landing: claude-8x5tm1, with an exact-tree GO. Base:
`feat/rust-rewrite` `33f7de6aa8f` (#2210 landed).

## Why

A streamed serial Plan copies its root pacing onto every leg, so one root cannot
mix request kinds that native code paces differently. WOO's trade FIFO mixes
ordinary POST (100 ms), PUT (charges the shared clock 200 ms) and algo POST
(500 ms on its own clock) in one job. MEXC's submit domain uses a 450 ms base and
a 1500 ms gap after a multi-order job call. Both need each call to name its own
pacing inside the venue's release domain.

The work is split by overseer routing:

- #2210 (landed): one release domain per venue, paced by one registered group.
- claude-dazzy3, next: the domain declares a set of admissible pacings (groups,
  outer-gap entries such as `{GLOBAL, 200}`, and `PacingSet` ids); then
  `Cx::defer_pacing` as its own slice for response-derived debt (MEXC code 510).
- This slice: a serial call names which pacing its legs carry.

## API

```rust
pub struct SerialCall {
    pub operation: Operation,
    pub timeout_ms: u32,
    pub attempts: u8,
    /// This call's pacing; `None` inherits the root's.
    pub pacing: Option<Pacing>,
}
```

Resolution happens at `admit_serial_plan` for the first call and at every
continuation, before any ID, reservation or state change:

- **Domain member** (by the call's own operation): `domain_pacing(operation,
  call.pacing.or(root))`. A call naming anything the domain does not admit is
  Invalid. Against #2210's single-pacing domain that admits only the domain
  pacing; dazzy3's set PR generalizes `domain_pacing` and `domain_paces`
  without touching this code.
- **Non-member** (no domain, or a Read in a `Mutations` domain): the call's
  pacing, else the root's unless the root is domain-paced (a Read does not
  borrow the domain's gap). It must resolve to a zero-gap group, as serial roots
  require today (`validate_zero`).

The resolved value is stored on the current call, and `plan_leg_context` stamps
each leg from it rather than from the root. Non-serial Plans are unchanged.

## Not changed

- The serial root's own admission rule (#2210): a domain-paced root or a
  zero-gap root.
- Release-time pacing: legs wait at Claim through the existing `SendPacing`
  check and the domain FIFO; no new timer path.
- `continue_serial_plan_after` remains the venue-chosen delay (Read retries,
  confirmation waits), not pacing.

## Controls (originals)

1. `serial_08_admission_refusals_spend_nothing`: a first call naming a nonzero
   gap outside a domain, or an undeclared group, is Invalid and spends nothing.
2. `serial_10_a_continuation_naming_a_nonzero_gap_is_refused_and_settleable`:
   the refused continuation leaves the call settleable.
3. `serial_21_each_leg_carries_its_own_call_pacing`: legs carry their own
   call's pacing, and `None` inherits the root.
4. `a_serial_call_names_only_its_domain_pacing`: in a `Mutations` domain a write
   may name the domain pacing; another group, or a Read naming the domain
   pacing, is Invalid; a Read with `None` is unpaced.
5. `outside_a_domain_a_serial_call_names_only_a_zero_gap_group`.

RED was proven by disabling the leg stamping (controls 3 to 5 fail) and the
continuation resolution (control 2 fails).

## Adoption and composition

Every existing `SerialCall` literal gains `pacing: None`, which is byte-identical
behavior. zs5e83's #2220 adds `auth` to the same struct; whichever lands second
adds the other's field mechanically. WOO and MEXC trading source waits for this
slice, dazzy3's set PR and (MEXC) `defer_pacing`.
