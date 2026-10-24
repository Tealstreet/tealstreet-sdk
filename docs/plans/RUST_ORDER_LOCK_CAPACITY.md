# Rust Order-Lock Capacity Refusal

Owner: claude-dazzy3. Reviewer: claude-fe7lgm. Assigned by claude-ddi2il.
Found while reviewing the condition wait (`RUST_CONDITION_WAIT.md`, RED23).
Base: `feat/rust-rewrite` at `142fde8fd04`.

## Problem

For venues with `guard_order_mutations`, engine command entry acquires an
order-lock scope before `on_command` (`engine.rs`, `Event::Command`).
`OrderLocks::acquire` refuses when a scope conflicts, and also when 64 scopes
are already active. Both refusals reach the same branch, which reports:

- "Order amendment or cancellation is already in flight";
- no outcome (`outcome: None`).

For a new placement refused only because 64 order commands are in flight,
that is wrong twice. Nothing amends or cancels it, and the caller cannot tell
that nothing was sent. Placements take a scope only where
`guard_pending_placements` is also set. At this base those venues are AsterDEX,
Bitget, Bitrue, BloFin, BreakoutProp, OKX and Phemex. A Bitget queue of more
than 64 placements reaches this branch (condition-wait RED23).

## Change

`OrderLocks` is `pub(crate)`, so this is an internal change with no engine
public API change.

- `acquire` returns which refusal applied: `Conflict` (the existing overlap
  and uncertain cancel-all fence) or `Capacity` (the 64-scope cap). The
  conflict checks run first, so a command that both conflicts and meets the
  cap keeps today's conflict refusal.
- At entry, an order command (placement, amend or cancel) refused for
  `Capacity` ends through `refuse_before_entry` with:
  - the message "Order command capacity reached: 64 order commands are in
    flight; retry after one finishes";
  - `kind: Internal` and `outcome: NotSent`, the same kind and outcome as the
    engine's request `Capacity` (`Error::venue_error`).
- Unchanged:
  - Every conflict refusal, including a placement behind an in-flight
    cancel-all.
  - The uncertain cancel-all refusal.
  - The 64-entry deferred cancel-all queue (`defer`).
- Nothing is sent and the venue never sees the refused command. The 64
  held scopes are unaffected.

## Ruling (claude-ddi2il)

An amend or cancel refused only by the cap reported "already in flight" too,
which was false in the same way. Ruled: it gets the same capacity refusal and
`NotSent`. A genuine conflict (an amend or cancel behind an in-flight command
on the same order) keeps the existing text with no outcome.

## Original REDs and controls

In `sdk-rs/engine/src/order_lock_capacity_tests.rs` (`order-locks` feature),
with a venue that holds every order command in flight:

1. RED: `original_a_new_placement_at_the_scope_cap_is_not_sent_with_a_capacity_message`.
   With 64 placements held, the 65th gets exactly one terminal: the capacity
   message, `Internal`, `NotSent`. Nothing is sent, the venue sees no 65th
   entry, all 64 scopes stay held and none is taken for 65.
2. Control: capacity frees when a held placement finishes, and the next
   placement enters.
3. Control: a placement behind an in-flight cancel-all on its symbol keeps the
   conflict refusal and no outcome.
4. Control: a placement that both conflicts and meets the cap keeps the
   conflict refusal (precedence).
5. Amend and cancel refused only by the cap take the capacity refusal and
   `NotSent` (flipped by the ruling).
6. Control: an amend behind an in-flight cancel of its order keeps the
   conflict refusal and no outcome.
7. Control: a venue that guards mutations but not pending placements takes
   no scope for placements, so its 65th placement enters.

## Gates

- Engine tests in `order-locks`, `http-plans,order-locks` and default modes.
- Clippy `-D warnings` and fmt.
- Independent review of this plan and the REDs, then the source.
- CALLERS notes the refusal for every venue that guards placements.
