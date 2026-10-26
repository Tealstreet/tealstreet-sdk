# Retire an unissued independent Plan call

Plan, 2026-10-26. Owner: claude-fe7lgm (engine). Reviewers: claude-4yjlw0
(consumer, Bitrue B2) and claude-dazzy3 (core). Landing: claude-8x5tm1, with an
exact-tree GO. Base: `feat/rust-rewrite` `174f6a4c90c`.

## Why

Bitrue B2 (#2173, Q1) moves cancel batches onto a complete Plan with
`independent_calls`, one step per target, as BingX does. Native Bitrue skips a
child target at dispatch time when its Store row is already terminal
(`trading.rs` `trade_step`): a confirmed parent cancel, or a stream update,
closed it. A complete Plan declares every step up front, and today nothing can
retire a declared step the venue decides not to send:

- `done_read_plan_after_data` is Read-only;
- `done_mutation_plan_after_data` completes only at a declared terminal, with no
  other call issued;
- `done_plan` requires `schedule.is_complete()`, so the cursor must pass every
  step.

The skip can happen after a sibling's failure or because of a stream update, so
the retirement cannot be tied to a sibling's DATA.

## API

```rust
impl<P> Cx<'_, P> {
    /// Retire the current unissued call of an independent complete Plan that
    /// the venue decides not to send. No journal fact; its reservations return.
    pub fn retire_unissued_plan_call(&mut self, plan: &Plan, slot: &Slot) -> Result<(), Error>;
}
```

The feature is `http-plans`, the same as the complete Plan API. The function is
generic over `Cx<P>`, so families that never call it link nothing.

## Admission (every check precedes any change)

1. `validate_plan(plan, now)` passes: a live, unexpired owner and an uncancelled
   Plan. An unbound Plan only (`validate_unbound_plan`).
2. `policy.independent_calls` and a complete, declared Plan. A serial Plan is
   `State`; a dependent Plan is `Invalid`. Skipping in a dependent Plan would
   change what its later steps mean.
3. The slot is the schedule's current `Available` call: its entry is `Unissued`,
   has no call, is not parked, and is eligible (`eligible_at <= now`). This is
   the same moment and order at which the venue could have issued it. Any other
   slot (Waiting, Active, Complete, Branch) is `Revoked`.
4. The slot is not a member of a declared mutation barrier group (`Invalid`).
   Barrier groups are all-or-nothing.
5. The same issue custody as `plan_leg_context` (`plan_issue_custody`): only
   where the venue could have issued this call.

## Effects (one transaction, after all checks)

- `Schedule::retire_unissued(slot)`: Available becomes Complete. The call's
  reserved attempts and every unselected fallback credit are released, and the
  sequence advances exactly as `settle` does.
- Entry credit becomes `Released`, and `reserved_contexts -= 1`, which is what
  issuing would have consumed. Seal-time `unissued()` then no longer counts it.
- No journal registration, write fact, intent or delivery: the leg never
  existed. Terminal projection sees only legs that were issued.
- `refresh_availability(now)` and `rearm_plan(plan)`, as after `finish_plan_call`.
  The next step becomes eligible on its own `wait_ms` from now.

The venue then continues as after a finished call: it issues the next Available
slot, or calls `done_plan` / `fail_plan` once the schedule is complete. The
engine never completes the Plan by itself, and never infers a result.

## Not in scope

- No Read Plan or serial Plan retirement, and no retiring an issued call (use
  the existing finish and failure paths).
- No change to independent failure handling, barrier groups or terminal
  projection.
- No venue adoption: Bitrue B2 is the first consumer, in its own PR.

## Original controls (RED first)

The Plans fixture in `request_context_planned_tests.rs` uses genuine admission,
issue and finish. Until source, the API is an inert stub returning `Invalid`.

1. A three-step independent cancel Plan: issue and finish step 0, retire step 1
   at its eligible moment, then issue and finish step 2. `done_plan` succeeds
   with one terminal, exactly two physical calls and two journal legs. After the
   seal, reserved contexts are back to the baseline.
2. Retiring the LAST step completes the schedule, and `done_plan` succeeds.
3. After a sibling's released failure, retiring the next step and then
   `fail_plan` projects the same outcome as without the retirement. The retired
   step adds no fact.
4. Reservation: retirement frees exactly one reserved context immediately, and
   the next step's issue succeeds.
5. Refusals each leave state byte-identical (same available slots, reserved
   counts and journal):
   - a dependent Plan (`Invalid`);
   - a serial Plan (`State`);
   - a Waiting slot, an issued slot or a twice-retired slot (`Revoked`);
   - a slot before its `eligible_at` (`Revoked`);
   - a barrier-group member (`Invalid`);
   - outside issue custody (`Revoked`);
   - an expired owner (refused, nothing changed).

## Gates

Engine tests and strict clippy in `[]`, `http-plans`, `serial-plans`,
`http-mutation-barriers` and the full set; fmt; bans; ratchets. Paired
Bybit/Bitget/combined sizes on the actual tip are expected +0, because no family
calls the API. CI records family sizes; combined is measured locally.
