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
    /// WriteAfterReads Preflight with a write credential. The Plan root's
    /// deadline is the caller's receipt deadline.
    pub fn admit_serial_plan(
        &mut self,
        preflight: &Preflight,
        first: SerialCall,
        policy: SerialPolicy,
    ) -> Result<(Plan, Slot), Error>;

    /// In the exact DATA custody of the current call: settle it, then admit
    /// the next serial call under the same root. Refuses while any write fact
    /// is uncertain, after the root deadline, or outside that custody.
    pub fn continue_serial_plan(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        next: SerialCall,
    ) -> Result<Slot, Error>;
}

pub struct SerialCall { pub timeout_ms: u32 } // one physical attempt, Never
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
through `http_write_result` and `classify_write`; and the Plan ends with
`done_plan` or `fail_plan`. `finish_plan_call` ends the last call.

### Invariants

- **One live call.** At most one call is issued and not yet settled. The Plan
  holds exactly one reserved registry Context for its whole life. That counts
  as one against `CAPACITY`, whatever the number of calls.
- **Continuation is custody, not renewal.** `continue_serial_plan` requires:
  - the current call's exact released delivery, validated as by
    `validate_plan_delivery`;
  - its write fact recorded and definitive (Accepted or Rejected);
  - the root still live;
  - no uncertain fact anywhere in the journal.
  It spends the next call's IDs (three, plus one signing ID) at that moment.
  No timer, socket, Session, retry grant or second root is created.
- **Check all, then commit.** `continue_serial_plan` runs every check (custody,
  definitive fact, live root, no uncertainty, capacity, pacing) before it
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
- **Wall-clock.** The root deadline is the caller's receipt deadline, captured
  at admission and never extended. This is native parity only if native's
  whole funnel is bounded by the same receipt. Open, for consumers: OKX and
  Bitget must confirm native's overall bound for a 30,000-call input before the
  REDs freeze. If native has none, this invariant refuses real orders, as the
  cap did. Each call's own bound is
  `min(now + timeout_ms, root)`. Root expiry seals the Plan through the
  existing Plan timer.
- **Uncertain stops.** An Unknown fact makes `continue_serial_plan` refuse
  with State. The venue then calls `fail_plan`, the journal projects Unknown,
  and the existing uncertain-write fence drives reconciliation. Nothing after
  an uncertain row is sent.
- **Refused admissions spend nothing.** Bad custody, a stale delivery, an
  expired root or capacity are refused before IDs or state change.
- **Feature scope.** `http-plans`. Mutation only (a Read Plan has no
  `next_write`). No session wait (`plan_session_wait` on a serial slot is
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
  diagnostics belong.

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

- `done_plan(plan, result)` succeeds when every journal fact is Accepted or
  Rejected, with nothing Unknown, unreleased or unsettled. Rejected means a
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
   earlier accepted ID, and nothing more is sent.
5. Root expiry mid-chain, two cases. (a) The live call is unreleased: one
   Timeout (NotSent) terminal, the call retired, no further issue. (b) The
   live call is released: its fact is Unknown, the projection carries Unknown
   with every streamed accepted ID, and the uncertain-write fence is armed, as
   in RED4. An expired root refuses continuation in both.
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
8. Admission refusals spend nothing: a Read Preflight, a Read admission with
   `refusals_complete`, no write credential, a zero timeout, a nonzero resolved
   pacing floor, and outside genuine entry or ready custody.
10. A refused continue (expired root, uncertain fact, foreign delivery) leaves
    the current call settleable: `finish_plan_call` then `done_plan` succeeds
    with the journal intact and nothing reserved.
11. An accepted reply with an ID over `ID_FIELD` makes that fact Unknown; the
    chain stops and the fence is armed.
9. A max-size control for CPU: 30,000 continued calls (one bracketed max input)
   with per-call work O(1). Time is reported on the PR as the CPU
   certification input, not asserted.

## Gates

- Engine focused tests in plans, locks, sockets and default feature modes, the
  REDs under `http-plans,order-locks`;
  clippy `-D warnings`; fmt; bans.
- Independent review of plan and REDs, then source.
- CPU certified on the PR with control 9.
- WASM bytes reported for Bybit, WOO and Bitget; the primitive is linked only
  by adopters.
- Consumers (OKX ordered writes, Bitget slice 3) stack on it. Their
  per-venue behaviour stays in their own plans.
