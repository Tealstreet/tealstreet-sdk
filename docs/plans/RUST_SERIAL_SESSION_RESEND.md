# Rust Serial Session Resend

Owner: codex-zs5e83 (transferred by claude-ddi2il). Reviewer: claude-fe7lgm;
auditor: claude-8x5tm1. Consumer: GRVT write funnel. The design below is the
reviewed plan at 6208560a514. Ruled: claude-ddi2il approved it as its own PR after
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
its `floor`, parks inside its own window. It re-prepares the same call when
`session_ready` announces a newer revision. A pub(crate) minimum-revision seed
sets `floor`. This PR adds no Session park of its own.

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
- **Seed.** Attempt 2 carries `floor = r`, the bundle revision the refused
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
  These are the unreleased operation's outcomes. The command terminal follows
  its Plan journal, retaining earlier effects, IDs or uncertainty. The parked
  attempt adds no uncertainty because it was never released.
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

## Engine path for the status-exempt grant (claude-dazzy3 S1)

Today a 408 or 5xx write never reaches the venue codec in DATA custody.
`ResponseProof::from_status` (`http_evidence.rs`) marks it Unproven. The
managed write path (`http_lifecycle.rs`, write response completion) records
evidence through `unproven_http_evidence` and fails the Plan directly, and
`classify` rule 3 (`unproven_status`) returns Unknown before any code table.

The grant therefore needs one named, venue-declared exemption:

- A new Venue method with a default: `fn status_exempt_refusal(&self, pending:
  &Self::Pending, response: Response<'_>) -> bool` (default `false`). It is
  consulted only when `from_status` is Unproven, and BEFORE
  `unproven_http_evidence` is recorded. GRVT returns `true` only when its full
  error envelope parses (the JSON object with `code == 1000`); a substring match
  is not enough, since a proxy or CDN 503 can carry arbitrary HTML.
- When it returns `true`, that delivery is treated as native-codec evidence:
  `on_response` runs in DATA custody, and `classify_write` skips rule 3 for
  that delivery only (engine-internal, per-delivery state). The venue's table
  maps 1000 to `RefusedResend`, so the grant comes from the ordinary journal
  gate.
- Any other body (a different code, or unparsable) leaves the hook `false`,
  and today's path is unchanged: Unknown, fence and reconcile, no grant.
- It is additive: a defaulted trait method, with no change to `Row`,
  `CodeTable` or `Verdict`. Only GRVT overrides it, and CALLERS discloses it as
  venue-declared.

## Immediate Read-failure continuation (claude-dazzy3 S2)

`continue_serial_plan` (immediate) from failure custody mirrors the
`continue_serial_plan_after` failure rules exactly:

- it requires the current Read call's own released failure (an unreleased or
  provider refusal stays the engine stop);
- it allows only a Read `next`; a Mutation `next` is Invalid;
- a second continuation from the same failure callback is refused;
- every refusal spends nothing.

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
    refused, spending nothing. Five immediate retries (six total calls) then succeed.
12. Ruling controls (GRVT codec, venue-declared, disclosed in CALLERS): 408 and
    503 with parsed code 1000 each resend once and are accepted; a 408 or 503
    with an unparsed body or a non-1000 code is Unknown for a Mutation and
    refuses the resend; a second code 1000 is terminal. The shared grant policy
    is unchanged.
13. Seed gate: attempt 2 does not send while Ready at `r`, and an announce at the
    same `r` does not wake it.
14. Status-exempt grant: a 503 whose body parses as code 1000 reaches
    `on_response`, gets the RefusedResend grant and resends once. A 408 or 503
    with any other code stays Unknown with no grant and the fence armed. A 503
    whose 1000 body fails to parse stays Unknown, and so does a 503 HTML page
    that merely mentions 1000. An exempted delivery leaves no unproven
    evidence and no fence; a non-exempt one keeps today's exact evidence. A
    venue that does not override the hook keeps today's path for 1000 at 503.
15. Immediate continuation mirrors continue_after: an unreleased failure stops;
    a Mutation `next` and a second continuation from one callback are refused,
    spending nothing.

## Gates

Engine-focused tests under `serial-plans,session-bearer,condition-waits` and
the same set with `order-locks`; clippy
`-D warnings`; fmt. Plan review, then REDs, then source. WASM bytes reported.
The API is additive. It depends on #2069 phase 3's SOURCE, not just its plan
(the Bind park and the pub(crate) minimum-revision seed), so the REDs wait for
that source and the PR stacks after it.


## Takeover and original-control checkpoint

The implementation is engine scope. GRVT production, credential-only setup
(#2132), and G3/G6 stay outside this PR; claude-m26ps4 owns those seams.
The branch plain-merges the landing tip before work and before paired size
measurement. Measure combined, Bybit and Bitget on that same composed tree;
record both baseline and candidate artifacts, and delete each scratch target
following its measurement. No default feature is enabled by this change.

The original checkpoint adds an inert `serial_plan_session_resend` declaration
returning `State`, its evidence enum, and the default-false Venue hook. These
make the assertions compile without pretending the source exists. It changes
no existing admission, retry, classifier, status handling or Bearer behavior.
`request_context_serial_session_tests.rs` drives real command admission and
Prepare/Bind/Claim; the test recipe uses a provider callback with dummy values.
An original is counted RED only after compilation and a behavioral assertion.
Seeded-floor/wake, timer, retirement, body, journal and lock suffixes behind the
first selected-attempt assertion remain gated until source; they are not
execution evidence at this checkpoint. Read attempts=2 first asserts admission.
Strict 408/503 negatives and existing immediate-resend paths are positive
controls against the unchanged runtime. Production follows original review.


### Original execution receipt

On the merged landing base 34999f973f9, the 24 new controls compile with
`serial-plans,condition-waits,session-bearer,order-locks`: 13 assertion REDs,
11 GREENs, zero ignored. The 37 existing serial Plan controls pass separately.
Scoped clippy with warnings denied, rustfmt and local bans/ratchets are required
at this freeze. This is an originals-only draft, not runtime implementation or
READY evidence. The runtime stub returns State without consuming a grant.

First RED assertions: Read attempts=2 admission is Invalid; ordinary released
Read failure cannot immediately continue (Revoked, including a Mutation-next
probe which expects Invalid); the new API returns State instead of the required
selection/refusal; parsed 408/503 auth refusals do not reach DATA. No compile
error, missing method or fixture panic counts as a RED. The second-auth-refusal
Mutation probe observes the absence of a second journal grant and treats that
as RetryForbidden; it never manufactures or copies an opaque grant.

Later source review must additionally pin foreign-grant/slot combinations,
Read generic-retry exclusion after attempts=2 admission, prior journal effects
before a parked operation expires, and signed-vs-unsigned revision movement.
Those are explicit source gates, not claimed coverage from the original run.
