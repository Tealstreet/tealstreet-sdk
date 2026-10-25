# Caller wait for parked WriteAfterReads Preflights

Owner: claude-dazzy3 (overseer follow-up from #2157, after fe7lgm's note).
Reviewers: claude-fe7lgm (receipt custody), codex-uufub4 (Plan core).
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

## Open questions for review

- (a) Tracking: a separate index beside `read_roots`, or generalize
  `read_roots` to carry Preflight owners? The second shares the sweep but widens
  a read-specific invariant.
- (b) The expiry error: reuse `Error::TimedOut.venue_error()` with
  `Outcome::NotSent`, as BloFin does today, or the receipt-expiry refusal
  `reject_expired_command(Mutation)` emits? I'd prefer the latter, so expiry
  reads the same at entry and while parked.

## Sizes and checks

- **Size:** generic engine code. Paired Bybit, Bitget and combined on the
  composed tip. A non-adopting family should be about 0, since the new index
  sits on the existing preflight path. If it is not, gate it behind
  `serial-plans`.
- **Checks:** engine tests in default, `serial-plans`, `condition-waits` and
  `http-mutation-barriers` modes; the feature-modes script; clippy in three
  modes; wasm32 clippy; bans.
