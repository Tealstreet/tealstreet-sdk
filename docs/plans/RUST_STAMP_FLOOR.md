# Receipt-bound stamp floor

Plan, 2026-10-24. Lane claude-m26ps4 (dispatch signing), approved by
claude-ddi2il. The consumer is codex-9czdjh's Bitrue dispatch signer (plan
`dea54abae38`): a native -1022 signature resend must use a timestamp strictly
after the stamp the venue already received, even when the clock is fixed or
moves backward.

## Today

- `DispatchContext::stamp_ms()` is `now_ms.floor()` (`time::wire_stamp_ms`). It
  is the current protocol time, so a fixed or backward clock can repeat or
  regress a stamp.
- `SigningRelease` carries the genuine released `SignReceipt`. The engine
  alone constructs it, at a final signing release.
- No shared primitive turns "the previous released stamp" into a minimum for
  the next preparation. A venue would have to cache stamps or headers, or run
  its own clock allocator, and all three are refused.

## Contract

- **`StampFloor`**: an opaque `Copy` newtype with no public constructor. Its
  only source is `SigningRelease::stamp_floor() -> Option<StampFloor>`, taken
  from the genuine released `receipt.stamp_ms`. A venue cannot mint one from a
  parsed response, a header or a clock.
- **`DispatchContext::stamp_after(floor: Option<StampFloor>) -> Result<f64,
  SignError>`** returns `max(0, stamp_ms(), floor + 1)`.
  - It is non-negative by contract. Receipts already refuse negative stamps
    for every family, so nothing can depend on one, and Bitrue natively clamps
    its stamp to 0.
  - With `None` it is `max(0, stamp_ms())`: equal to `stamp_ms()` on every
    valid clock.
  - `floor + 1` beyond `MAX_SAFE_INTEGER` refuses `InvalidRequest`. There is
    no rounding or saturation.
- `stamp_ms()` is unchanged, so no other family's stamps move.
- **The floor carries no authority.** It only constrains the stamp. Resend
  authority still comes from the venue's genuine refusal classification plus
  the engine retry context (`http_retry_context` with
  `RetryEvidence::DefinitiveRefusal`, or `AdmittedRead::retry`). That keeps the
  original Selection, account, generation and egress.
- **Use.** A recipe for a resend is constructed with
  `Option<StampFloor>` from the `SigningRelease` of the attempt being resent,
  and calls `dispatch.stamp_after(floor)` in every `prepare`. Every stale
  re-preparation applies the floor again, so the stamp stays at or above
  floor + 1 and still advances with the clock. A fresh request passes `None`.

## REDs (shown failing first; compile REDs, since the API is new)

- R1: `stamp_after(None)` equals `stamp_ms()` on a valid clock, and is 0 on a
  negative clock (`-1.5`).
- R2: fixed and backward clocks. With a floor of 1000, a clock at 1000.4 or at
  900 gives 1001, and a clock at 1500 gives 1500. A floor of 5 on a negative
  clock gives 6.
- R3: the safe-integer edge. A floor at `MAX_SAFE_INTEGER` refuses
  `InvalidRequest`; a floor of `MAX_SAFE_INTEGER - 1` gives `MAX_SAFE_INTEGER`.
- R4: `SigningRelease::stamp_floor()` is `Some` exactly when its receipt has a
  stamp, and carries that stamp.
- R5 (privacy): a `compile_fail` doctest shows `StampFloor` cannot be
  constructed outside the engine.

## Ownership and order

- claude-m26ps4: the engine primitive, its REDs and the docs.
- codex-9czdjh: the Bitrue recipe that consumes it, kept in their own PR.
- Branch `feat/rust-stamp-floor`, off tip `59f50e4f37b`. Independent review
  answers "per-venue fix of a cross-cutting concern?". The answer is no: it is
  a shared dispatch primitive, and any venue with a monotonic-stamp resend can
  use it.
