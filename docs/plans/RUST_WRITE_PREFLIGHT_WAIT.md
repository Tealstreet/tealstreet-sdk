# Caller wait for parked WriteAfterReads Preflights

Owner: claude-dazzy3 (overseer follow-up from #2157, after fe7lgm's note).
Reviewers: claude-fe7lgm (receipt custody, plan CLEAR), codex-uufub4 (Plan core, CLEAR after P2).
Base: `feat/rust-rewrite` `f895d0d7018`. **Plan only; no source.**

## Problem

`begin_preflight_with_command_wait` binds the caller's receipt cap only for
ReadOnly Preflights (`RUST_PLAN_RECEIPT_CAP.md`, first slice). A trading
command behind a live job parks a WriteAfterReads Preflight with
`park_preflight`. That arms no timer and binds no receipt.

BloFin W1 (#2157) therefore records `command_wait()` at entry and checks it
only when the command is woken. A command whose wait expires while it is still
queued gets no terminal until the job ahead of it ends. Its NotSent comes late
and comes from the venue, not the engine.

## Ruled boundaries kept

- `RUST_PLAN_SERIAL_STREAM.md`: a caller receipt never bounds a serial Plan.
  Receipt expiry mid-chain is not a Plan terminal (RED18). This change caps only
  the **Preflight phase**: entry, parked or queued, any Preflight reads, up to
  Plan admission.
- `RUST_CONDITION_WAIT.md`: `park_preflight_for` bounds stay venue-computed and
  unchanged. Its "a WriteAfterReads Preflight binds no receipt" stays true for
  any Preflight that doesn't opt in.
- No fallback. A missing, foreign or expired receipt is the shared typed
  refusal, never an uncapped Preflight or a default timeout.

## Change

1. **Entry.** `begin_preflight_with_command_wait` also accepts
   `PreflightPurpose::WriteAfterReads`:
   - the same receipt capture as ReadOnly;
   - `with_command_entry(Operation::Mutation, ...)`;
   - a missing receipt is `reject_command_entry(Operation::Mutation, ...)`;
   - an expired capture is Revoked with no effects;
   - it binds `receipt_cap` on the new Preflight.
2. **Parked expiry, engine-owned.**
   - `park_preflight` on a capped WriteAfterReads Preflight enters a tracked
     index with one timer at the cap. This mirrors `read_roots`: same sweep
     discipline, prepare-then-mark, and one seal per owner.
   - At or after the cap, the engine fails the Preflight NotSent with a typed
     Timeout. It then calls the venue's existing Preflight-retirement hook, so
     BloFin's `trading_retired` drops it from its FIFO and wakes the next one.
   - Leaving the park untracks it and clears the timer. Leaving means
     `request_preflight_ready` admission, a Preflight terminal, or engine
     retirement.
   - `request_preflight_ready` on an expired Preflight is Revoked. An early timer
     re-arms for the exact cap. Equality expires, in both event orders.
3. **Admission.** `admit_serial_plan`, `admit_plan_with_barriers` and Mutation
   `admit_plan_owned` from a capped WriteAfterReads Preflight need
   `now < cap`, else the same typed Timeout NotSent. The admitted Plan does
   **not** inherit the cap (the serial ruling). The current
   `receipt_cap.is_some() && operation != Read` refusal in
   `request_context_planned.rs` narrows: it still refuses a capped ReadOnly
   Preflight on a Mutation route, and lets a capped WriteAfterReads Preflight
   admit with the cap dropped.
4. **Preflight reads.** A capped WriteAfterReads Preflight's reads use the
   existing `read_bound` rule: Session-parked reads are capped, and a read
   issued in ordinary custody keeps its own window.

## Venue follow-up (separate PR)

BloFin replaces its `command_wait()`-at-entry deadline and wake-time check with
`begin_preflight_with_command_wait(WriteAfterReads)`. The queued deadline field
and its check are deleted, per the orphan rule. Its "expired parked command
fails NotSent at wake" original becomes "fails at the cap, while still queued,
and the FIFO advances".

## Originals (engine, RED on the base)

1. A WriteAfterReads `begin_preflight_with_command_wait` succeeds. Today it is
   Revoked. A missing receipt is the Mutation typed refusal; an expired capture
   is Revoked with no effects.
2. A capped parked WriteAfterReads Preflight fails NotSent/Timeout at exactly
   the cap, with no wake. It fails once, the venue retirement hook fires once,
   and a later `request_preflight_ready` is Revoked.
3. Equality in both orders: the cap timer and a ready wake at the same instant
   give expiry first, with no admission.
4. Admission before the cap: the Plan is admitted and runs past the cap with no
   Timeout (the serial ruling's RED18 kept).
5. Admission at or after the cap is NotSent/Timeout, and nothing is consumed.
6. GREEN controls:
   - an uncapped `park_preflight` stays timer-free;
   - a ReadOnly capped Preflight is unchanged;
   - a capped ReadOnly Preflight on a Mutation route is still refused;
   - `park_preflight_for` is unchanged.

## Review dispositions (fe7lgm, CLEAR on receipt custody)

- (a) **Separate index.** It shares the sweep mechanics through one small
  helper: the timer table, prepare-then-mark, early re-arm and equality.
  `read_roots` keeps its read-only invariant: a tracked caller-wait root stays
  Read, and Mutation is refused under it. Its seal is a read-root terminal,
  whereas this one is a Preflight NotSent plus the venue retirement hook. The
  new index is gated with the Preflight code (`http-plans`).
- (b) **Expiry error.** Exactly the refusal that
  `reject_expired_command(Operation::Mutation)` emits, so expiry reads the same
  at entry and while parked: outcome NotSent, kind Timeout, with no journal
  entry or write fact.
- **P1, expiry custody.** It fires from top-level timer handling, outside any
  command entry and any delivery, as `expire_read_roots` does. It seals once,
  then calls `on_http_owner_retired_preflight` once with no Cx authority to
  admit work for the dead command. RED: a venue that calls admit or
  `request_preflight_ready` on the expired Preflight inside the hook gets
  Revoked.
- **P2, order locks.** A parked trading command holds its order-lock scope from
  `Event::Command`. Cap expiry releases it through the normal command finish;
  an unsent command gets no fence. RED: the next queued command on that symbol
  is not refused as a conflict.
- **P3, entry.** Binding the cap on WriteAfterReads keeps `fresh_command_entry`
  and consumes the one entry loan exactly as ReadOnly does. RED: a second begin
  in the same callback is Revoked and allocates nothing.

## Review dispositions (uufub4, Plan core)

- **P2, inherited uncertainty.** A sendless Preflight may carry
  `inherit_preflight_uncertainty`, and `seal_preflight` already projects
  Unknown for it (`inherited_uncertainty_seals_a_sendless_write_preflight_unknown`).
  So cap expiry does not use a universal NotSent. It seals through the existing
  owner-aware mutation seal and projection:
  - an uninherited owner gives Timeout/NotSent;
  - an inherited owner stays Unknown, keeping the existing fence and
    reconciliation disposition.
  No new execution fact is introduced. Original:
  `cap_expiry_keeps_the_owner_aware_mutation_seal`, which covers both cases.
- **Admission.** The cap is checked, then dropped from both the Body and the
  deadline before the Plan is admitted. RED18, the read-only refusal and the
  nonce and barrier checks are all retained.
- The separate expiry index and the seal-before-hook order, with no Cx, are
  confirmed. Early, non-finite, duplicate, equality, current-generation and
  capacity controls remain source gates. The hook already takes no Cx, so P1 is
  tested by handing the expired handle back for a wake, not with a fabricated
  hook Cx.

## Originals (`engine/src/write_preflight_wait_tests.rs`, executed on f895d0d7018)

`--features http-plans,order-locks`: **6 assertion REDs, 1 GREEN, 0 ignored.**
- REDs:
  - `a_write_preflight_binds_the_caller_cap_and_spends_its_one_entry`: today the
    entry is Revoked;
  - `a_parked_capped_write_preflight_fails_at_its_cap_without_a_wake`;
  - `a_hook_cannot_wake_the_preflight_that_just_expired`;
  - `a_ready_wake_at_the_cap_loses_to_expiry_in_either_order`;
  - `cap_expiry_keeps_the_owner_aware_mutation_seal`;
  - `cap_expiry_releases_the_order_lock_for_the_next_command_on_the_symbol`.
  Each of the last five fails because no cap timer exists today.
- GREEN: `an_uncapped_parked_write_preflight_stays_timer_free`.
- The existing pin `plan_receipt_cap_tests` `Mode::RefuseWrite` (a WriteAfterReads
  wait is Revoked) is the ruled behaviour change. The source commit flips it
  explicitly; it is not silently carried.

## Sizes and checks

- **Size:** generic engine code. Paired Bybit, Bitget and combined on the
  composed tip. A non-adopting family should be about 0, since the new index
  sits on the existing preflight path. If it is not, gate it behind
  `serial-plans`.
- **Checks:** engine tests in default, `serial-plans`, `condition-waits` and
  `http-mutation-barriers` modes; the feature-modes script; clippy in three
  modes; wasm32 clippy; bans.
