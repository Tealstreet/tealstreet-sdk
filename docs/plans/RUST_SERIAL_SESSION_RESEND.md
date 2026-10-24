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

## Review pins (claude-dazzy3)

- **Read with two attempts (changes the serial source on this branch).**
  `validate_call` today refuses `attempts > 1` for a Read, and
  `retry_policy` maps 2 attempts to DefinitiveRefusal. Here a Read may declare
  `attempts: 2` with RetryPolicy Never. Its second attempt is reachable only
  through `serial_plan_session_resend`. `plan_resend_context` needs a write
  grant, so it refuses a Read. The Read-failure retry through
  `continue_serial_plan_after` admits a new call, never attempt 2.
- **One second attempt per call.** Attempt 2 is spent by either the immediate
  resend (Bitget 40009) or the Session resend, never both. After either, the
  other is RetryForbidden.
- **Write proof.** The engine trusts only the RefusedResend grant, so the
  venue codec must classify GRVT code 1000 through `classify_write` as a
  definitive refusal with nothing executed. When attempt 2 succeeds, attempt
  1's refusal is history: absent from `rejections()` and from the
  `refusals_complete` accounting.
- **Credential generation.** The resumed attempt binds to the Session's
  post-login generation, so it cannot reuse a stale cookie or account ID. A
  ready from an older Session generation is refused.
- **Session state at park.** The Session must be login-pending or ready when
  parking; an idle Session that never started is Invalid. Parking on a ready
  Session resumes in the same flush, after the current callback returns
  (no re-entry).
- **Existing guards.** `plan_session_wait` and `plan_retry_session_wait` on a
  serial slot stay State (serial RED19b). This API is the only exception.
- **Cleanup.** After a stop or a login failure while parked, the Session's
  waiters hold nothing for the Plan, and the parked Pending is handed back
  exactly once.

## GRVT native facts (codex-zs5e83, `bcaaa4acc56`)

- **When.** An authentication refresh happens only on a DATA error with
  `wire::code == 1000` in `on_response`, never on a transport failure. That
  matches the DATA-custody precondition.
- **Immediate Read network retries (changes the serial source on this
  branch).** A GRVT Read retries a transport failure immediately, up to 6
  attempts (`pending.attempts < 5`). The serial Plan allows a Read-failure
  continuation only through `continue_serial_plan_after` with a delay greater
  than 0. Here `continue_serial_plan` (immediate) also accepts the current
  Read call's own released failure, with a Read `next` under the same checks.
  No 1 ms delay is invented, and no DATA is fabricated. Each network retry is
  a new serial call; the 6-attempt count is the venue's ledger.
- **Separate ledgers.** Native allows one authentication resend per Step
  (`auth_resent`), carried across transport retries; the network-retry
  counter is not reset by it. The engine allows at most one Session resend per
  serial call. A Step spanning several calls (transport retries) keeps
  `auth_resent` in venue state and never asks for a second Session resend; a
  second 1000 fails as native does.
- **Body parity.** The resend keeps the original nonce, expiration, client IDs
  and digest. Only the cookie and account headers change. The venue's recipe
  signs the same immutable canonical payload again through the provider:
  there is no signed-bytes cache and no fabricated grant. The engine's own
  per-attempt signing reservation does not touch the venue's payload nonce.

## Rulings since review (claude-ddi2il, via codex-zs5e83)

- **Explicit auth refusal grants the single resend at any HTTP status.** GRVT
  parses code 1000 as an explicit venue authentication refusal that proves
  nothing executed, including on a 408 or 503. This PR adds a narrow
  venue-declared grant: the venue codec classifies that parsed refusal as
  `RefusedResend` through `classify_write`, whatever the status. This is not a
  generic 408/5xx widening. Any other code on a 408 or 5xx stays Unknown,
  with no resend.
- **Wait for a newer header revision, not merely Ready.** GRVT's refresh keeps
  the same engine Session Ready and only moves the header revision (#2069).
  The park records the revision the refused attempt used, and resumes only
  when `SessionBearer::Ready` carries a newer one. A Ready Session with the old
  or no revision does not resume it. New calls admitted mid-refresh wait at
  Bind through #2069's NotReady wait (proposed to claude-m26ps4), not here.
- **Journal owns the terminal.** The shared write journal projects the
  terminal even where native `trade_finish` strips fields.

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
8. A Read with `attempts: 2` is admitted. Its second attempt cannot come from
   `plan_resend_context` or from a `continue_serial_plan_after` failure
   retry; only the Session resend reaches it.
9. One second attempt per call: after an immediate resend, a Session resend
   is RetryForbidden, and after a Session resend, an immediate resend is
   RetryForbidden.
10. The resumed attempt carries the Session's post-login generation, and a
    ready from an older generation is refused.
11. Parking on an idle Session is Invalid. Parking on a ready Session resumes
    in the same flush, after the parking callback returns.
12. Control: `plan_session_wait` and `plan_retry_session_wait` on a serial slot
    stay State.
13. After a stop or a login failure while parked, the Session holds no waiter
    for the Plan, and the Pending is handed back exactly once.

14. An immediate Read-failure continuation: a released Read failure, then
    `continue_serial_plan` with a Read `next`, issues at once in that failure
    custody (no timer). A Mutation `next`, an unreleased failure, or a
    Mutation call's failure is refused, spending nothing. Six chained
    immediate retries then succeed.

15. A Mutation answered 503 (and 408) with parsed code 1000: the venue grant
    makes it RefusedResend, the park resumes it once, and it is accepted. The
    same status with any other code is Unknown and refuses the resend.
16. Revision gate: parking while the Session is Ready on the refused
    attempt's revision does not resume it; a newer revision resumes it once.

RED1 also asserts that attempt 1's fact is a definitive refusal (not Unknown)
and that, once attempt 2 succeeds, it is absent from `rejections()` and the
`refusals_complete` accounting.

## Gates

Engine-focused tests under `http-plans` and `http-plans,order-locks`; clippy
`-D warnings`; fmt. Plan review, then REDs, then source. WASM bytes reported.
The API is additive.
