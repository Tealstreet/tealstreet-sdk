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

## Design: the same call's second attempt, seeded on the refused revision

The wait itself is #2069 phase 3 (claude-m26ps4, `e04cf305e87`). An unreleased
managed call whose Bind read is NotReady, or Ready at a revision no newer than
its `seen`, parks inside its own window. It re-prepares the same call when
`session_ready` announces a newer revision. A pub(crate) minimum-revision seed
sets `seen`. This PR adds no Session park of its own.

```rust
impl<P> Cx<'_, P> {
    /// From the current serial call's exact DATA custody, after an
    /// authentication refusal: issue the same call's second attempt now,
    /// seeded with the refused attempt's bundle revision. Its Bind parks (#2069)
    /// until a newer revision, so it never goes out with the refused bundle.
    pub fn serial_plan_session_resend(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        evidence: SessionResend,
    ) -> Result<Context, Error>;
}

pub enum SessionResend {
    /// A Mutation's classify_write RefusedResend grant (one per call).
    Write(ResendGrant),
    /// A Read whose DATA the venue classified as an authentication refusal.
    Read,
}
```

- **Same logical call.** The returned Context is attempt 2 of the refused call,
  within `SerialCall.attempts = 2` reserved at admission. The venue queues it
  through `request_signed_managed` or `request_managed` as for any attempt.
  There is no new logical write, no new journal leg and no fake Read. The
  venue rebuilds its identical native body.
- **Seed.** Attempt 2 carries `seen = r`, the bundle revision the refused
  attempt bound with. If Bind still reads Ready at `r` (the venue has not yet
  cleared its cache), the call parks rather than sending the refused bundle.
  It wakes on a revision newer than `r`. One counter is shared with #2069.
- **Authority.** A Mutation needs the RefusedResend grant `classify_write`
  returned for this exact attempt; the GRVT codec declares parsed code 1000 at
  any status (ruling below). A Read needs only its own validated DATA. A call
  with `attempts: 1`, or one whose second attempt is already spent, is
  RetryForbidden.
- **While parked.** Attempt 2 is the Plan's one live call (unreleased), so the
  chain cannot continue, and a delayed continuation or condition wait cannot
  be armed. The order lock stays held.
- **Endings (ruling: never a second resend).** Each of these is #2069's
  existing outcome for an unreleased call:
  - the window expires while parked: Timeout, NotSent;
  - the Session retires: Revoked, NotSent;
  - attempt 2 answered with code 1000 again: terminal with native's error,
    since there is no third attempt.
  The serial Plan then stops as for any unreleased or definitive failure, with
  the streamed IDs and no fence (nothing was released by the parked attempt).
- **No separate wait or cleanup.** Because the only park is #2069's, there is
  no Session waiter entry, no `on_plan_session_wait_ended` path and no Pending
  hand-back for this API. `plan_session_wait` and `plan_retry_session_wait` on
  a serial slot stay State.

## Review pins (claude-dazzy3)

- **Read with two attempts (changes the serial source on this branch).** A Read
  may declare `attempts: 2` with RetryPolicy Never. Its second attempt is
  reachable only through `serial_plan_session_resend`. `plan_resend_context`
  needs a write grant, and the Read-failure continuation admits a new call,
  never attempt 2.
- **One second attempt per call.** Attempt 2 is spent by either the immediate
  resend (Bitget 40009) or the Session resend, never both.
- **Write proof.** When attempt 2 succeeds, attempt 1's refusal is history:
  absent from `rejections()` and from the `refusals_complete` accounting.
- **Credential generation.** Attempt 2 binds the bundle its Bind reads, which
  #2069's revision fence guards: a claim-time revision move is Stale, then
  re-prepare.

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
  with no resend. Ruled (b), scoped to the GRVT codec; the shared policy is not
  loosened, and Bitget and others keep the strict grant.
- **Wait for a newer header revision, not merely Ready.** GRVT's refresh keeps
  the same engine Session Ready and only moves the header revision. #2069
  phase 3 owns the wait for every call (the Bind park), and this PR seeds
  attempt 2 with the refused revision.
- **Journal owns the terminal.** The shared write journal projects the
  terminal even where native `trade_finish` strips fields.

## REDs (before source)

Written against #2069 phase 3 (its park and seed), stacked after it.

1. A Mutation refused with code 1000 (RefusedResend): `serial_plan_session_resend`
   returns attempt 2, seeded with `r`. It binds while the bundle is still Ready
   at `r`, so it parks with nothing sent. `session_ready` announces `r+1`, it
   re-prepares, sends the identical body with the new bundle, is accepted, and
   the chain continues. The journal has one leg with two attempts, and the IDs
   appear once. Attempt 1 was a definitive refusal and is absent from
   `rejections()` and `refusals_complete` accounting.
2. The same for a Read (validated DATA). The Read leaves no write fact.
3. Attempt 2 answered with code 1000 again: terminal with native's error;
   a further `serial_plan_session_resend` is RetryForbidden.
4. Refusals that spend nothing: no grant; an Unknown fact; a foreign or stale
   delivery; `attempts: 1`; an already spent second attempt.
5. Window expiry while parked: Timeout, NotSent, and the chain stops with the
   streamed IDs and no fence.
6. Session retirement while parked: Revoked, NotSent, and the chain stops.
7. The order lock is held through the park.
8. A Read with `attempts: 2` is admitted; `plan_resend_context` and the
   Read-failure continuation cannot reach its second attempt.
9. One second attempt per call: after an immediate resend a Session resend is
   RetryForbidden, and the reverse.
10. Control: `plan_session_wait` and `plan_retry_session_wait` on a serial slot
    stay State.
11. An immediate Read-failure continuation: a released Read failure, then
    `continue_serial_plan` with a Read `next`, issues at once (no timer). A
    Mutation `next`, an unreleased failure, or a Mutation call's failure is
    refused, spending nothing. Six chained immediate retries then succeed.
12. Ruling controls (GRVT codec, venue-declared, disclosed in CALLERS): 408 and
    503 with parsed code 1000 each resend once and are accepted; a 408 or 503
    with an unparsed body or a non-1000 code is Unknown for a Mutation and
    refuses the resend; a second code 1000 is terminal. The shared grant policy
    is unchanged.
13. Seed gate: attempt 2 does not send while Ready at `r`, and an announce at the
    same `r` does not wake it.

## Gates

Engine-focused tests under `http-plans` and `http-plans,order-locks`; clippy
`-D warnings`; fmt. Plan review, then REDs, then source. WASM bytes reported.
The API is additive. It depends on #2069 phase 3 (the Bind park and the
minimum-revision seed), so it stacks after it.
