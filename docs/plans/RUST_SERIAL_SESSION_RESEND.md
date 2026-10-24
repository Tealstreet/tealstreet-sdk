# Rust Serial Session Resend

Owner: claude-fe7lgm. Consumer: codex-zs5e83 (GRVT write funnel), who
confirms native parity. Ruled: claude-ddi2il approved it as its own PR after
the condition wait. Independent review: claude-dazzy3. Stacks on the
condition wait (`RUST_CONDITION_WAIT.md`), which stacks on the streamed serial
Plan (`RUST_PLAN_SERIAL_STREAM.md`).

## Problem

GRVT native, audited by codex-zs5e83 at `bcaaa4acc56`: a Trade call answered
with code 1000 (authentication refused) calls `account_refresh` and parks the
same Pending. It waits for a login HTTP round trip, then resends the identical
`Step.body` once (`auth_resent`). `trading.rs:963` pins the identical body
after a new cookie and account ID. This can happen between serial calls and on
the current call. Reads, cancels and verification reads follow the same rule:
one authentication resend per call.

The serial Plan cannot express it:

- `plan_session_wait` and `plan_retry_session_wait` refuse a serial slot with
  State (serial RED19b);
- `plan_resend_context` resends at once, with no login round trip between;
- a separate refresh call or a new logical write would change native's
  attempt count and journal, and is not native.

## Design: the same call's second attempt, parked on a login Session

```rust
impl<P> Cx<'_, P> {
    /// From the current serial call's exact DATA custody, after an
    /// authentication refusal: park its one remaining attempt on `session`.
    /// When the Session is ready, the same call resumes as attempt 2 with a
    /// fresh window and fresh credentials. Nothing is spent while parked.
    pub fn serial_plan_session_resend(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        evidence: SessionResend,
        session: &Session,
        pending: P,
    ) -> Result<(), (P, Error)>;
}

pub enum SessionResend {
    /// A Mutation's classify_write RefusedResend grant (one per call).
    Write(ResendGrant),
    /// A Read whose DATA the venue classified as an authentication refusal.
    Read,
}
```

- **Same logical call.** Resuming is attempt 2 of the call that was refused,
  within `SerialCall.attempts = 2` reserved at that call's admission. There is
  no new logical write, no new journal leg, and no fake Read. The venue
  rebuilds its native body; the identical body is the venue's to keep.
- **Authority.** A Mutation needs the resend grant that `classify_write` returned
  for this exact attempt, so the existing journal gate holds: never after
  Unknown, 408 or 5xx. A Read needs only its own validated DATA. A call with
  `attempts: 1`, or one already resent, is refused with RetryForbidden.
- **Resume.** This is the existing Session-ready resume shape (a parked Slot is
  issued as a fresh leg under the same root), delivered through
  `on_http_plan_ready`. The Plan keeps its one reserved Context, and there is
  no live call while parked.
- **Ending a park (ruling).** If re-login fails or the Session retires,
  `on_plan_session_wait_ended` arrives in failure custody. The venue fails
  there with native's error and outcome; there is no re-park and never a
  second resend. A declining hook seals once. A resend answered with code
  1000 again fails the same way. The stops (revoke and the rest) end a parked
  Plan at once.
- **Bound.** A park has no wall-clock of its own, like the serial root. It
  ends through the Session (ready, failure or retirement) or a stop. A
  Session login has its own engine bound.
- **Exclusive.** It excludes a delayed continuation and a condition wait: one
  park per Plan at a time.

## REDs (before source)

1. A Mutation refused with code 1000 (a RefusedResend grant) parks on a
   Session. Login completes, the same call resumes as attempt 2 with fresh
   signing, is accepted, and the chain continues. The journal has one leg with
   two attempts, and the IDs appear once.
2. A Read whose DATA is an authentication refusal parks the same way and
   resumes as attempt 2. The Read leaves no write fact.
3. A second authentication refusal on attempt 2 cannot park again
   (RetryForbidden). The venue fails with the native message.
4. Refusals that spend nothing: no grant; an Unknown, 408 or 5xx fact; a
   foreign or stale delivery; `attempts: 1`; a delayed continuation or
   condition wait already armed; a non-login Session.
5. Login failure, and Session retirement, while parked: each delivers
   `on_plan_session_wait_ended` in failure custody. The venue's `fail_plan`
   projects native's error with the streamed IDs. A re-park attempt there is
   refused (no second resend). A declining hook seals once.
6. A stop while parked ends the Plan with one terminal, the streamed IDs and
   no fence (nothing is live).
7. The order lock is held through the park.

## Gates

Engine-focused tests under `http-plans` and `http-plans,order-locks`; clippy
`-D warnings`; fmt. Plan review, then REDs, then source. WASM bytes reported.
The API is additive.
