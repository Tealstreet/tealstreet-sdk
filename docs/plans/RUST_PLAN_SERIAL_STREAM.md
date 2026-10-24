# Rust Streamed Serial Plan

Owner: claude-fe7lgm (Plan owner). Ruling: claude-ddi2il (streamed serial
Plan, no step cap; supersedes the 2,048-attempt cap). Consumers: codex-ih35js
(OKX ordered writes), claude-4yjlw0 (Bitget write slice 3). Independent review:
claude-dazzy3 or claude-4yjlw0. Base: #2037 frozen head `c9691ad9b2a` (6b).

## Problem

OKX's native ordered-write funnel sends one request at a time (`next_write`)
with no retries. A single input order may split into 10,000 lots, which is 500
twenty-row batches. With stop-loss and take-profit brackets it is 30,000 serial
calls, and `PlaceOrders.orders` is uncapped. So the whole command has no finite
native maximum.

A declared Plan cannot represent that:

- `PlanBound` and the write journal stop at `MAX_ATTEMPTS = 256`;
- admission reserves one registry Context per declared call, against the
  engine-wide `CAPACITY = 256`;
- the evidence budget truncates accepted IDs at 4,096.

Any finite cap would refuse real orders that native sends today.

## Design: one Plan, one live call, continued from DATA

The Plan is admitted with one call. Each next call is admitted only from the
previous call's exact DATA custody. There is no declared length.

```rust
impl<P> Cx<'_, P> {
    /// Admit a streamed serial write Plan with its first call. Requires a
    /// WriteAfterReads Preflight with a write credential. The root has no
    /// wall-clock of its own; each call carries its own bound.
    pub fn admit_serial_plan(
        &mut self,
        preflight: &Preflight,
        first: SerialCall,
        policy: SerialPolicy,
    ) -> Result<(Plan, Slot), Error>;

    /// In the exact DATA custody of the current call: settle it, then admit
    /// the next serial call under the same root. Refuses while any write fact
    /// is uncertain, after a stop, or outside that custody.
    pub fn continue_serial_plan(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        next: SerialCall,
    ) -> Result<Slot, Error>;

    /// As `continue_serial_plan`, but the next call is issued from the Plan
    /// timer's callback after `delay_ms` (native read retries).
    pub fn continue_serial_plan_after(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        delay_ms: u32,
        next: SerialCall,
    ) -> Result<(), Error>;
}

pub struct SerialCall {
    pub operation: Operation, // Read or Mutation, under the admitted credential
    pub timeout_ms: u32,      // the per-call bound
    pub attempts: u8,         // 1, or 2 for Bitget's native 40009 resend
}
pub struct SerialPolicy {
    pub credential: Credential,
    pub pacing: Option<Pacing>,
    /// Definitive per-row refusals complete the command (native `done []`);
    /// see Terminal projection.
    pub refusals_complete: bool,
}
```

Everything else is the existing Plan surface, unchanged:
`plan_leg_context(plan, slot)` issues the call; the venue queues it through
`request_signed_managed`; `validate_plan_delivery` gates DATA; write facts go
through `classify_write` with `http_write_fact` or `http_write_batch`; and the
Plan ends with `done_plan` or `fail_plan`. `finish_plan_call` ends the last
call. A native job with nothing to send ends through the existing
`done_empty_plan(preflight, result)`: no serial Plan and no dummy first call.

### Invariants

- **One live call.** At most one call is issued and not yet settled. The Plan
  holds exactly one reserved registry Context for its whole life. That counts
  as one against `CAPACITY`, whatever the number of calls.
- **Continuation is custody, not renewal.** For a Mutation call,
  `continue_serial_plan` requires (Read calls: see Read calls):
  - the current call's exact released delivery, validated as by
    `validate_plan_delivery`;
  - its write fact recorded and definitive: Accepted, Rejected, or a
    KnownPartial batch (every row's outcome proven, mixed accepted and
    refused, as `classify_write` returns for a definitive mixed batch).
    PartialUnknown and unproven Failed stop the chain;
  - the Plan not stopped;
  - no uncertain fact anywhere in the journal.
  It spends the next call's IDs (three, plus one signing ID) at that moment.
  No socket, Session, venue timer or second root is created.
- **Check all, then commit.** `continue_serial_plan` runs every check (custody,
  definitive fact, not stopped, no uncertainty, capacity, pacing) before it
  changes anything. A refused continue leaves the current call exactly as it
  was: still settleable through `finish_plan_call`, then `done_plan` or
  `fail_plan`, with the journal intact. There is no half-settled state.
- **One reservation, reused by generation.** Every call's leg Context comes
  from the single reserved registry slot, generation-bumped at continuation.
  The previous call's delivery and leg Context are then stale, and
  `validate_plan_delivery` rejects them.
- **No pacing wait.** Continuation hands off directly from DATA, so admission
  refuses a policy whose resolved pacing floor is nonzero, as with mutation
  barriers. A zero or absent floor is accepted.
- **Order locks are held for the Plan.** The Command's order-lock management
  starts at the WriteAfterReads Preflight and ends at the Plan terminal, as for
  a declared mutation Plan. Each released call notes a Command write; no call
  acquires or releases its own fence.
- **Nonces per call.** Each continuation reserves its nonce at Bind, like a
  declared leg. A refused continue reserves none.
- **No root wall-clock (ruling, claude-ddi2il).** Native has no whole-job
  bound, so the serial root has no deadline and the caller receipt never caps
  it. Each call is bounded by `now + timeout_ms`. That replaces native's own
  physical request timeout (OKX 20 s; none on Bitget's legacy funnel), and the
  bound now also covers preparation and queue time. Each consumer's CALLERS
  discloses the coverage it actually changes. The chain advances only from
  custody (below), so every interval is bounded. Termination comes from the
  native job being finite and from the stops, not from the per-call bound.
- **Caller-wait expiry mid-chain.** Receipt expiry is not a Plan terminal and
  emits no Timeout. The chain keeps sending, the command reads
  `CommandWait::Expired` as a held command does today, and the order lock and
  slot stay held. The command's one terminal is the Plan's own.
- **Stops.** Credential revoke or rotation, Session retirement, engine
  shutdown and caller cancel each stop the chain at the next custody point; a
  live call is still settled or retired, never orphaned. They project failure
  with every streamed accepted ID, plus Unknown when a released call is live,
  in which case the lock converts to the fence. These and `done_plan` or
  `fail_plan` are the only ways the lock and slot are released.
- **No receipt binding.** Nothing in a serial Plan consumes the caller
  receipt, so the shared `begin_preflight_with_command_wait` stays ReadOnly
  only and unchanged. Bitget's 10 s detection bound is a venue timer, not the
  receipt (claude-dazzy3 R6). A Preflight admits at most one Plan, so a second
  admission is State.
- **Resend.** With `attempts: 2`, one resend is allowed. It comes only from the
  DATA custody of a definitive refusal that `classify_write` proves the venue
  may resend (Bitget 40009), never after Unknown, 408 or 5xx. It goes through
  the existing `plan_resend_context` grant and journal gate. The resend is a
  new Context and a new request: it signs fresh, with a new stamp and a nonce
  reserved at Bind, and never replays bytes. A third attempt is refused.
- **Read calls.** A Read `SerialCall` (amend resolve, metadata and position
  re-reads) never enters the write journal, the accepted IDs or the
  `refusals_complete` accounting. Its two transitions:
  - *Success.* From the Read call's exact validated DATA custody,
    `continue_serial_plan` or `continue_serial_plan_after` continues with no
    write fact required; the DATA itself is the evidence.
  - *Failure retry.* From the Read call's own failure callback
    (`on_request_failed` for the current call's exact failed delivery, after a
    transport or HTTP failure), only `continue_serial_plan_after` is allowed,
    and only with a Read `next`. That fresh Read is the native retry, and the
    venue counts native's 3 tries. The failed call is settled by the
    transition; no DATA or fact is invented. These are refused, spending
    nothing: a stale or another call's failure; a failure of a Mutation call;
    a provider, signing, credential or local refusal (the call never reached
    the venue, so it stops as a NotSent failure); and a Mutation `next`.
- **Delayed continuation.** `continue_serial_plan_after` arms the Plan's one
  timer, which has no other user. When it fires, the venue receives the
  existing `on_http_plan_ready(plan)` and issues the next call from
  `plan_slots` there. It is refused while any fact is uncertain, and that is
  re-checked when it fires. Its custody is single-use and exclusive with a
  live call. It has the same check-all-then-commit rule.
- **Replan.** A serial Plan declares no calls. So a replan (Bitget 40085
  promotion) is just the next continuation with different calls: nothing is
  sealed and there is no second admission or root.
- **Uncertain stops.** An Unknown fact makes `continue_serial_plan` refuse
  with State. The venue then calls `fail_plan`, the journal projects Unknown,
  and the existing uncertain-write fence drives reconciliation. Nothing after
  an uncertain row is sent.
- **Refused admissions spend nothing.** Bad custody, a stale delivery, a
  stopped Plan or capacity are refused before IDs or state change.
- **Feature scope.** `http-plans`. Admitted from a WriteAfterReads Preflight
  only (a Read Plan has no `next_write`); Read calls run inside it. No session wait (`plan_session_wait` on a serial slot is
  State), no mutation-barrier groups, no executions, no `after_refusal`
  fallbacks.

## Journal and evidence: streamed, never truncated

Native reports every accepted ID in order. For a serial Plan, the write
journal compacts each definitive attempt as soon as it settles. Its accepted
IDs are appended to a streamed ID list exactly as received: no dedup, which
matches native's append and needs no set. Its refusal is counted, and the
per-attempt entry is dropped. At most one attempt entry (the live one) exists
at a time.

Evidence budgets for serial Plans:

- `ID_COUNT` (4,096) and `ID_BYTES` (256K) are lifted: the ID list is
  linear in accepted IDs, like native's result.
- `ID_FIELD` (1,024) stays. A reply carrying an ID over it cannot be recorded
  exactly, so that call's fact is Unknown: the chain stops, and the fence
  reconciles. It never sets a silent overflow.
- `TEXT_BYTES` (64K) stays. `rejections()` is bounded: reports are kept in order
  until the text budget is spent, after which a total refusal count and the
  existing truncation marker stand for the rest. Each refusal is visible in
  full to the venue in that call's DATA custody, which is where per-row
  diagnostics belong. Refusal evidence is stream-and-retire: memory for an
  all-refused chain is constant once the budget is spent, never linear in
  calls.

- Memory is linear in accepted IDs, as native's own result already is, and
  O(1) in calls. There is no `ID_COUNT` truncation for serial Plans.
  Declared Plans keep their existing budget unchanged.
- The failure projection (`partial.order_ids`) carries the whole streamed
  list, so a Plan stopped by an uncertain row still reports every accepted ID
  before it. The `[truncated IDs …]` marker never appears for serial Plans.

## Terminal projection: `refusals_complete`

Native standalone cancellation reports each definitively refused row and can
still finish with `done []`. Today a Plan whose writes are not all Accepted
always projects a failure.

With `SerialPolicy.refusals_complete = true`, declared at admission:

- `done_plan(plan, result)` succeeds when every journal fact is Accepted,
  Rejected or a definitive KnownPartial, with nothing Unknown, unreleased or unsettled. Rejected means a
  released call whose venue reply `classify_write` reads as a definitive
  refusal. A NotSent fact (precommit, provider or credential refusal) is not
  Rejected: it forces the failure projection, so an all-NotSent chain never
  becomes `done []`. The
  venue's `result` is the terminal, and the refused rows remain in the journal
  and its rejection reports for the venue's per-row diagnostics.
- Any uncertain fact still forces the failure projection. `done_plan` cannot
  hide uncertainty.

The policy is a permission, not an override. A venue that follows native
rules where a refusal stops the command (for example OKX code 1: reject,
stop, restore) calls `fail_plan`, which keeps the failure projection with the
recorded refusals. Only the native continue-and-finish path (OKX code 2) calls
`done_plan`.

Default (`false`), as for every declared Plan, the projection is unchanged.
No consumer fallback, fabricated acceptance or numeric terminal.

## Rulings and native facts

claude-ddi2il approved all of these. Native facts come from claude-4yjlw0
(`venues/bitget/src/trading.rs` at `49f7d1857b6`); the rules themselves are in
Invariants above.

- **No root wall-clock.** Native's serial queue runs until the last call
  settles. A 10,000-lot UTA order is about 25 min at 150 ms per call, against
  the 120 s default caller wait.
- **One resend (A).** Used only for Bitget's native 40009 single resend; a
  second 40009 is an explicit rejection.
- **Read calls and delayed continuation (B).** Up to 4 GETs per step, each
  retried up to 3 times at 250×n ms.
- **Replan as continuation (C).** This supersedes the earlier
  second-admission wording. Restoring Canceling rows and clearing venue state
  stay venue-side. Native promotes only while no IDs are acknowledged, and
  otherwise fails with them (`fail_plan`).
- **Parking (D).** The 10 s detection wait runs before planning, from a
  native venue timer. The caller receipt cannot provide that bound, so no
  receipt binding is added. What the engine needs for it (holding a
  WriteAfterReads Preflight across callbacks and failing it at 10 s) is open
  with claude-4yjlw0, outside this primitive.
- **Classic partial batch (E).** KnownPartial plus `fail_plan`, with the IDs
  kept.
- Venue-side, no engine ask: the idempotent strategy-missing cancel is
  classified Accepted; the 4,096 unobserved-placement cap; the amend
  cancel-and-replace fallback and UTA rollback as per-step next calls; the
  empty-string ID filter and single-ID fallback.

## Original REDs and controls (before source)

The engine fixture admits a serial Plan from a genuine WriteAfterReads Preflight
and drives real Prepare/Bind/Claim. A RED commit adds inert
`admit_serial_plan` and `continue_serial_plan` that return State, so every RED
fails at an assertion.

1. 600 serial calls with no declared length. Every call is issued only after
   the previous call's DATA, IDs advance by exactly one call's worth each step,
   and only one Context is ever reserved: the registry total stays at
   `contexts + 1`. After each continuation the previous delivery and leg
   Context are rejected by `validate_plan_delivery`.
2. With 255 peer roots live and the registry otherwise full, a 600-call Plan
   completes (one reserved slot), and every peer validates after.
3. Continuing outside custody refuses and spends nothing. Cases: a socket
   callback; a stale previous delivery; a failure callback; before the fact is
   recorded; and a foreign Plan's delivery.
4. An Unknown fact (a released write with an unproven reply) refuses
   continuation with State. `fail_plan` then projects Unknown with every
   earlier accepted ID, and nothing more is sent. The Plan-held order lock
   converts to the uncertain-write fence at that terminal (asserted through
   `order_locks.uncertain`), not merely released.
5. Per-call timeout mid-chain, two cases. (a) The live call is unreleased: one
   Timeout (NotSent) terminal, the call retired, no further issue. (b) The
   live call is released: its fact is Unknown, the projection carries Unknown
   with every streamed accepted ID, and the uncertain-write fence is armed, as
   in RED4, including the order-lock to fence conversion. Continuation is
   refused in both.
6. Accepted IDs: 10,000 IDs over 500 calls appear in exact order, untruncated,
   in both `partial.order_ids` (after an uncertain stop at call 501) and the
   venue's `done_plan` result path. No `[truncated IDs …]` marker.
7. `refusals_complete`: all rows refused gives `done_plan("[]")`, a success;
   `rejections()` keeps reports in order up to `TEXT_BYTES`, then the count
   and marker (a 30,000-refusal case pins the bound). Mixed accepted and
   refused also succeeds. Any Unknown fact still fails. A first call NotSent
   with the flag true still fails. All-refused followed by `fail_plan` (OKX
   code 1) projects failure. With the flag false, all-refused projects the
   existing failure.
8. Admission refusals spend nothing (snapshot before and after each): a Read
   Preflight, a Read admission with `refusals_complete`, no write credential,
   a zero timeout, zero or three attempts, and a nonzero resolved pacing floor,
   each Invalid. Outside genuine entry or ready custody is Revoked. A zero
   pacing floor is admitted (positive control).
9. A max-size control for CPU: 30,000 continued calls (one bracketed max input)
   with per-call work O(1). Time is reported on the PR as the CPU
   certification input, not asserted.
10. A refused continue (stopped Plan, uncertain fact, foreign delivery) leaves
    the current call settleable: `finish_plan_call` then `done_plan` succeeds
    with the journal intact and nothing reserved.
11. An accepted reply with an ID over `ID_FIELD` makes that fact Unknown; the
    chain stops and the fence is armed.
12. A definitive mixed batch (KnownPartial) continues to the next call, then
    `done_plan` succeeds under `refusals_complete`, with that batch's accepted
    IDs in order and its refusals counted. Those refusals share the one
    `TEXT_BYTES` stream-and-retire budget and count. A PartialUnknown batch
    refuses continuation and projects Unknown.
13. A Preflight admits at most one Plan: a second admission is State and
    spends nothing. A caller wait of 100 ms never bounds the chain.
14. A 60,000-call all-refused chain, with KnownPartial batches mixed in:
    retained refusal evidence from both stops growing at `TEXT_BYTES`, the
    count is exact, and `done_plan("[]")` succeeds.
15. An empty native job ends through `done_empty_plan` with no Plan, Context
    or attempt.
16. A KnownPartial batch, then `fail_plan`: failure projected with that batch's
    accepted IDs after all earlier ones.
17. Resend, reads and delays:
    - a two-attempt call resends once after a proven 40009, freshly
      provider-signed (a different signing stamp, not just a new request id),
      and a third attempt is refused;
    - after a released 503, or a released transport failure, a resend is
      refused and spends nothing, the fact is Unknown, the fence is armed and
      `fail_plan` projects;
    - a Read call inside a mutation chain leaves no attempt in the journal
      (`command_writes`) and no refusal, and the IDs are the mutations' only;
    - a delayed continuation arms the Plan timer at exactly `now + delay` and
      issues from `on_http_plan_ready`;
    - Read failures retry at 250, 500 and 750 ms, chained, and the third retry
      succeeds. From each failure, a Mutation `next` and a second delayed
      continuation are refused and spend nothing;
    - a Read's physical timeout at equality still retries, with the accepted
      prefix intact.
    Source-stage additions, with the stops in RED19: an uncertain fact at fire
    time, and stale, foreign, Mutation and provider or local failures.
18. Caller-wait expiry mid-chain: no terminal or caller failure is delivered
    after the 150 ms wait expires, later calls are still accepted, the lock
    stays held through that window, and the command's single terminal is the
    Plan's.
19. Mid-chain stops: credential revoke, Session retirement, engine
    shutdown and caller cancel each stop at the next custody point and project
    the streamed IDs. With a released live call they project Unknown and arm
    the fence.

### RED commit notes

- Tests live in `request_context_serial_plan_tests.rs` as `serial_NN_*`, one
  name prefix per control above. Inert stubs return State, so each RED fails
  at an assertion (the admission check, or RED8's Invalid). RED15 is a GREEN
  control on the existing `done_empty_plan`.
- RED19's stop cases (credential revoke, Session retirement, shutdown, caller
  cancel) are source-stage controls. Their engine entry points are fixed by
  the source, and they are added with it before review.
- claude-dazzy3 reviews each RED19 stop as a RED first: each must fail
  against the source minus its stop handling before it is GREEN.

## Source notes

- **Root.** A serial root's deadline is `f64::MAX`. It is finite for `Context`,
  and it never expires by clock. When the next wake would be the root, the
  Plan timer is cleared rather than armed at an absurd time. It is armed only
  for a delayed continuation.
- **Continuation.** It swaps in a fresh one-call schedule and entry. The
  previous Slot, Context and delivery go stale. It reserves `1 + 3×attempts`
  IDs, plus signing IDs, and returns the one Context reservation, so the Plan
  never holds more than one.
- **Journal.** The journal is reserved for 2 attempts. Each settled write leg
  is compacted into `StreamedWrites`: IDs are appended exactly as received
  and leave the per-attempt ID budget; refusals are counted, and retained
  only while their text fits 64 KB. `refusal_count()` stays exact. A failure
  projection with dropped refusals carries
  `[truncated: N more refusals]`.
- **Read calls sign with the Preflight's read role.** A Read call's Context
  carries the WriteAfterReads Preflight's `read_credential`, never the
  Plan's write credential. Bitget signs in-job GETs with its read key. With no
  read credential, the Read is unsigned and goes through `request_managed`.
  Read-first Plans are admitted (RED17 read retries start with a Read).
- **Stops.** Credential and provider refusal (RED19a) and Session retirement
  (RED19b) are covered. There is no engine entry today for "engine shutdown"
  or "caller cancel", so neither has a control; caller-wait expiry is RED18.
  Re-checking uncertainty when a delayed continuation fires is moot: no call
  is live during the delay, so no new fact can arrive.
- **Carried gaps.** The first consumer that adds an engine entry for
  shutdown or caller cancel owns those RED19 controls. The first nonce-signing
  consumer adds the nonce comparison to its resend REDs; the fixture compares
  signing stamps only. The PR body and that consumer's CALLERS record both.
- **Follow-up (claude-dazzy3).** Admission sizes the first call's signing
  reservation from the write credential. So an unsigned Read-first Plan may
  reserve, or be refused Capacity for, signing IDs it never uses. The first
  consumer touch fixes it, with a control.
- **WASM bytes** (release, base `003501960b1` against the frozen source
  `d4efb3f6b15`): Bybit 1,334,126 to 1,336,171 (+2,045); WOO 1,237,903 to
  1,240,977 (+3,074); Bitget 1,328,522 to 1,331,269 (+2,747). Non-adopters
  pay for the shared journal projection and leg-context paths.
- **Fixture edits from the frozen REDs** (no assertion weakened):
  - commands are BTCUSD PlaceOrders with the order-lock guards on, since a
    balance read takes no lock;
  - the venue declares pacing group 1 at 0 ms;
  - Read calls are queued signed;
  - RED3 fires the venue's own timer;
  - RED7 and RED14 use `drive_from`;
  - RED8's "no write credential" case is a credential mismatch, since a
    WriteAfterReads Preflight cannot exist without a write credential;
  - a provider-refused (unreleased) failure is left to the engine stop.
  Assertions added: exact `refusal_count()` in RED7 and RED14, the
  truncation marker, a provider-refused Read that cannot retry, and a
  Mutation failure that never takes the Read retry.

## Gates

- Engine focused tests in plans, locks, sockets and default feature modes, the
  REDs under `http-plans,order-locks`;
  clippy `-D warnings`; fmt; bans.
- Independent review of plan and REDs, then source.
- CALLERS discloses the new per-call bound (native sets none) and that the
  serial root has no wall-clock.
- CPU certified on the PR with control 9.
- WASM bytes reported for Bybit, WOO and Bitget; the primitive is linked only
  by adopters.
- Consumers (OKX ordered writes, Bitget slice 3) stack on it. Their
  per-venue behaviour stays in their own plans.
