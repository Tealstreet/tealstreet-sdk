# Receipt-bound stamp floor

Plan, 2026-10-24. Owner codex-9czdjh; design/reviewer claude-m26ps4,
transferred by claude-ddi2il. The consumer is the Bitrue dispatch signer
(plan `664d8ac0032`): every eligible native -1022 signature resend AND transport
reissue uses a stamp strictly after its own prior released stamp, including
fixed/backward clocks. The independent one-signature/three-transport budgets
and all retry authority remain with the admitted owner, not this primitive.

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
  - Any resulting stamp beyond `MAX_SAFE_INTEGER` refuses `InvalidRequest`,
    whether the current clock or `floor + 1` crosses the bound. A non-finite
    current clock is invalid; no rounding, saturation or NaN-to-zero fallback.
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

- codex-9czdjh owns this shared primitive and its separate Bitrue consumer.
- claude-m26ps4 owns design review and handed off `3e3241341db` (plan + new-API
  compile REDs) and `6ec4e604fdb` (source + reported focused GREEN controls).
- Current branch `feat/rust-receipt-stamp-floor` retains both handoff commits,
  based on actual `59f50e4f37b`. Independent review
  answers "per-venue fix of a cross-cutting concern?". The answer is no: it is
  a shared dispatch primitive, and any venue with a monotonic-stamp resend can
  use it.


## Handoff audit and remaining phases

The inherited R1-R4 receipt at `3e3241341db` is explicitly compile-time only:
missing StampFloor/stamp_after/stamp_floor, not a native behavioral regression.
Source `6ec4e604fdb` is inherited, not newly executed evidence by this lane.
Its safe-integer test covers floor overflow but not the final current-clock
result. Add NEW-CONTRACT controls, frozen before the source correction:

- A current clock at MAX_SAFE_INTEGER+1 is refused with None and a genuine floor.
- Non-finite current clocks (NaN and both infinities) are refused, retaining
  finite negative-to-zero and maximum-safe positive working controls.
- stamp_ms itself remains byte-exact and is not redefined by this helper.

Run these assertions first against inherited source (actual assertion REDs),
then perform one final-result validity check and preserve the original assertions.
The floor remains minted exclusively by SigningRelease from a validated released
receipt. It is Copy metadata, not request/owner/identity/retry authority.
No other family opts into stamp_after in this slice; no venue or manifest delta.

Open-PR map: signing_release.rs/signing_dispatch.rs are clear. Engine lib.rs is
claimed by #1747/#1798, but the overseer explicitly granted the one-line export
exception; whoever lands second composes that distinct hunk. No other claimed
source is touched. M26 stopped edits and reviews this takeover pin.

Focused verification: stamp/receipt/dispatch tests and privacy doctests, optional
features off/on, strict scoped clippy/fmt, scanner/bans as applicable. Full CI
covers the shared engine and all venue/WASM/replay inputs. Independent source,
caller and full-map reviews plus m26 review precede readiness; draft PR only,
no merge without an engine window. Add audit/quality receipt without changing
other source, retry semantics or clock behavior.
