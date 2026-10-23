# Engine-owned expiry for caller-wait Read roots

Owner: claude-fe7lgm. This is #1945's follow-up, assigned by claude-ddi2il on
2026-10-24 under codex-uufub4's split ruling:
- the root-expiry funnel is here;
- capped Read Plans and the one cause-aware receipt lookup stay in #1949.

Stacked on #1949 (`feat/rust-plan-receipt-cap`, `db28bac1ee4`), which stacks on
#1945. Reviewers: codex-uufub4 (engine custody) and codex-cxxyqr (history
consumer). This is a plan; failing tests follow review.

## Why

A plain `admit_http_command` root has no engine expiry. Engine facts at
`15df705c0b5`:
- **Timers.** Only per-leg managed HTTP timers exist
  (`http_lifecycle.rs:316`). An entry-owned root is `retained()`, so the
  registry keeps it until a terminal (`request_context_registry.rs:127-135`).
- **Completion.** Completion validation has no deadline check
  (`validate_completion_as`, 777-846), so `ReadCursorHistory::finish_with(Err)`
  on an expired root still publishes accepted rows and an INCOMPLETE marker.

Five history venues (Hyperliquid, OKX, Bitget, WEEX books, and others) park
work behind native queues. Without an engine funnel, each would need its own
root timer and its own owner map. The overseer ruled for one engine funnel
instead.

## Scope

- **Covered.** Only explicitly tracked ORIGINAL command Read roots, issued in
  the genuine Fresh entry from the command's exact retained receipt. There is
  no widening to Background, Mutation, Execution or Plan owners. Plans already
  have their own expiry (`engine.rs:4312-4326`).
- **Untracked roots.** A root admitted any other way (plain
  `admit_http_command`) keeps today's behaviour exactly.
- **Public refusal.** This PR also exposes #1949's internal cause-aware
  missing-receipt refusal as a public venue API. It adds no second lookup: it
  calls #1949's `Io::command_receipt` and its existing `with_command_entry`
  emitter.

## API (exact names for review)

```rust
impl Cx<'_, P> {
    /// Issue this command's original Read root with the caller's exact
    /// receipt deadline (raw f64, no integer rounding), tracked for
    /// engine-owned expiry. Fresh entry only.
    pub fn admit_caller_read_root(
        &mut self,
        credential: Option<Credential>, // slot plus the trusted KeyRef
        pacing: Option<Pacing>,
    ) -> Result<Context, CommandEntryError>;

    /// A genuine Fresh entry whose receipt is missing (exhausted sequence,
    /// duplicate live legacy id, unstampable clock) ends with one typed
    /// refusal: Read gets outcome None, Mutation gets NotSent. Ok means that
    /// refusal was emitted. Revoked, with no effect, when a receipt exists or
    /// the entry is not Fresh.
    pub fn reject_unstamped_command(
        &mut self,
        operation: Operation,
    ) -> Result<(), CommandEntryError>;
}

trait Venue {
    /// The engine has retired this tracked Read root at its deadline.
    /// `pending` holds the continuations of the owner's outstanding managed
    /// requests, already discarded from transport. Drop them and any queued
    /// continuation for `owner`. No Cx, so no completion, send, timer or Store
    /// rights; the engine seals the command itself.
    fn read_root_retired(&mut self, owner: Owner, pending: Vec<Self::Pending>) {}
}
```

### `admit_caller_read_root` outcomes

| Receipt state in the Fresh entry | Result | Effect |
| --- | --- | --- |
| Own live receipt, at least 1 ms left | `Ok(root)` | Root deadline is the exact receipt deadline; tracked. |
| Own receipt expired, or under 1 ms left | `Err(Revoked)` | None. The entry stays Fresh for `reject_expired_command(Read)`. |
| Receipt missing | `Err(Rejected)` | One typed Read failure with outcome None, through #1949's `with_command_entry(Read, Err(cause))`. |
| Not Fresh, borrowed callback, or delivery set | `Err(Revoked)` | None. |
| Index or timer reservation refused | `Err(Rejected)` | One typed Read failure (Capacity, outcome None). No root, owner, index entry, timer or credit is left behind. |

The guard is #1949's `fresh_command_entry`, which requires an untouched
resource counter and no delivery. The receipt comes from #1949's
`command_receipt`, through the same matching rule.

The root policy is Read, `RetryPolicy::Never`, with the given credential and
pacing. Its deadline is the receipt's raw f64, issued through a registry
variant `issue_command_root_at(.., deadline)`. It is never floored to a u32
timeout and re-derived. Legs are minted under it with the existing
`http_leg_context` and are capped at the root deadline
(`request_context_registry.rs:342-360`).

**Plain Read only.** A tracked owner stays a plain Read owner. A later
Mutation sibling, a replacement Context or ownership conversion (to a
Preflight, Plan or Execution) is refused with `Invalid`. The refusal comes
before any id, journal or phase change, so a Timeout seal can never erase a
released write.

**Transaction order** (refusal-atomic; nothing is spent before every check
passes):
1. **Effect-free checks:** the guard; the receipt; the Credential's own
   validation; a raw deadline that is finite and later than now; free
   registry owner capacity; and the plain-Read compatibility of any existing
   owner.
2. **Fallible storage reservations:**
   - an index slot, with `read_roots` preallocated to owner capacity, 256;
   - the timer action slot (`actions.try_reserve(2)`, for a clear plus an
     arm);
   - the registry's owner and context storage.
3. **IDs, the only spend:** the root identity and the timer id through one
   `reserve_ids(2)`. A refusal here, or at any earlier step, leaves no id,
   owner, index entry, timer or credit. It is a typed rejection through
   `with_command_entry`.
4. **Infallible commit:** issue the root, push the index entry and re-arm the
   timer. After step 3 nothing can fail. The issuer path used here has no
   fallible branch left, because its checks moved into step 1.

## Engine mechanism

1. **Index.**
   - `Io.read_roots: Vec<(f64, CommandId, generation)>` holds the entries,
     bounded by owner capacity.
   - One engine timer `(TimerId, at)` covers the earliest deadline, with an
     exact generation per arm. It is re-armed like `execution_timer`
     (`execution_cx.rs:20-38`): the old id is cleared and a new id armed when
     the earliest deadline changes. It is never in `io.timers`, so it never
     reaches a venue's `on_timer`.
   - The sweep's empty guard is O(1).
   - Every terminal of a tracked owner removes its entry by exact
     (id, generation) at the `finish_sealed_command` retirement point. That
     covers venue success, venue failure, a typed rejection and the funnel.
2. **Sweep position.** `expire_read_roots(now)` runs at every timed public
   entry, before anything that can stamp, call a codec or lend authority:
   - **`handle` and `handle_command`:** after `handle_command`'s argument and
     clock validation (invalid input still has no effect) and BEFORE the
     external receipt stamp. A same-id command arriving at or after an old
     root's deadline therefore finds the old owner already sealed. It gets a
     new sequence and generation instead of being stamped as a duplicate.
   - **`handle_received`:** first, before `process_socket_work` (3294), the
     plan and physical HTTP timers (3298, 3301) and every codec callback.
   - **Other timed entries:** the top of `start`, `prepare_dispatch`,
     `apply_public_feed`, `http_prepared`, `http_preparation_failed`,
     `claim_http`, `snapshot_frame`, `account_snapshot_frame`, `take_frame` and
     `take_market_frame`.
   - **`drain_actions`:** it has no clock, so it sweeps at the last observed
     engine time (`io.dispatch_now`), never a wall clock.

   Expiry at equality: `now >= deadline`.
3. **Expired fence.** An `expired` field inside the owner's `Context` phase is
   checked in the registry's shared `validate`/`validate_handle` and in
   `validate_completion_as`. Every path through them refuses with `Revoked`:
   - issuance (`http_leg_context`, siblings), queueing
     (`queue_managed_admitted`), retry and re-sign;
   - signing apply, `http_prepared` bind, final `claim_http` release;
   - publication and companion events (`validate_read_completion_context`);
   - history admission (`validate_read_http_delivery`);
   - numeric `done`/`fail` for the id, refused while the owner is retained.

   This includes children selected before the mark.
4. **Barrier.** At one sweep boundary:
   1. Mark ALL due roots first, before the first cleanup step. Cleanup for
      root A therefore can never publish for a due root B.
   2. Then, per root, run an engine-owned cleanup scope (analogous to #1932's
      plan sweep and `ExpiryScope`).
5. **Cleanup** has no venue Cx and no borrowed entry, Plan, socket, order or
   Read-transport proof. Per root, in order:
   1. **Reserve first.** Before marking or removing anything, count the
      owner's outstanding managed requests by exact id and generation, and
      `try_reserve` the move-only `Vec<Pending>` for that count plus the
      Cancel actions. A reservation failure poisons the engine, which is the
      existing no-half-state rule for engine-owned sweeps. It never runs a
      partial cleanup.
   2. **Discard.** `discard_managed` runs for each such request. It cancels
      that exact generation's signing state, preparation, streams, response
      buffers and leg timer, emits `HttpAction::Cancel`, and moves the
      `Pending` into the reserved vector. Any sent evidence is retained by the
      existing reconciliation capture; a Read has no write journal. Nothing is
      re-admitted, retried or completed.
   2. `venue.read_root_retired(owner, pending)` hands back those `Pending`s
      plus the owner, with no Cx. The venue drops native state and queued
      continuations. It cannot send, complete or touch the Store.
   3. Seal once with a private engine capability: a `RootExpirySeal` token,
      constructed only inside the sweep and bound to the exact issuer, root
      and generation. It is never lent to a venue. `finish_sealed_command`
      then emits `CommandFailed { kind: Timeout, outcome: None }` with
      `http_delivery == None`, and removes the owner and the index entry.
6. **Panics.** The whole sweep, covering marking, every root's discard, hook
   and seal, runs in one poison scope. An unwind poisons the engine, so a
   half-retired owner can never survive.

## Invariants

- **Exactly once.** Root expiry gives exactly one terminal: a strict Timeout
  with outcome None. It publishes no accepted rows, no INCOMPLETE marker and
  no native finalizer.
- **Leg timeout with a live root.** This is unchanged: the venue's native
  page-failure recovery, including accepted-only partials, still applies.
- **Timer order.** Root first and leg first are equivalent. The sweep runs
  before `expire_http_timer`, so a same-instant leg timer finds the owner
  already sealed. Its request was already discarded, so no venue failure
  callback runs for it.
- **Late replies.** A late reply or DATA for a retired owner is inert,
  through the existing generation and leg validation.
- **Reused ids.** A reused command id gets a new generation and a new receipt.
  A stale index entry or timer never touches it.
- **Isolation.** Cleanup never completes, re-admits or retries another owner.
- **Consumer handoff.** An adopting venue's `read_root_retired` must:
  - release any venue-local queue or busy bookkeeping that its failure
    callbacks used to own;
  - drop the returned `Pending`s and the parked continuations for that exact
    `Owner`;
  - never finalize numerically.

  Adopting-venue tests show the shared native queue continuing for a live,
  unrelated query after an expiry. This is per-venue cleanup; it grants no
  Store rights.
- **Known gap.** This PR does not close a gap that predates it: numeric
  `cx.done/fail(id)` after an owner is removed publishes through
  `Seal::Legacy`. Adopting venues must use their root authority only.
  CALLERS documents this.

## Original REDs (after plan review)

These run against existing engine APIs with a probe venue, using the real
`handle_command` receipt and managed Prepare/Bind/Claim. API-shape tests are
labelled as new-API tests, not REDs. The probe counts finalizer invocations
and records which command each INCOMPLETE marker belongs to.

1. **Parked root.** A tracked root parked with no HTTP reaches its deadline.
   There is exactly one `CommandFailed` with kind Timeout and outcome None. The
   finalizer count is 0, and there are no markers for that command. (Today: no
   terminal at all.)
1b. **Fractional deadline.** A root whose absolute deadline is fractional keeps
   it exactly. At `deadline - 0.5 ms` an already-admitted root is still live:
   there is no root Timeout, and leg-only accepted-only recovery stays
   allowed. At equality the result is a strict Timeout with no finalizer and no
   marker, in both timer orders. The sub-millisecond `Expired` rule belongs
   only to Fresh initial capture (RED 10). It never retires a live root early.
2. **Released leg in flight, root deadline equals the leg deadline.**
   - Both delivery orders: the leg `TimerFired` and the deadline sweep.
   - One strict Timeout, with the earlier accepted page unpublished and a
     finalizer count of 0.
   - The probe receives its `Pending` once, through the hook, and no failure
     callback runs.
3. **Unreleased leg in preparation.** The same as 2. A late `http_prepared`
   bind, or `claim_http`, after the deadline releases nothing.
4. **Leg-only timeout with the root live.** Accepted-only partial recovery
   publishes, with a finalizer count of 1 and one marker for that command,
   as today.
5. **Late reply and late DATA after expiry.** Inert, with no second terminal.
6. **Same id, no `TimerFired`.** The id arrives at the deadline exactly, and
   again past it:
   - the old root gets one strict Timeout;
   - the new command gets a new receipt and generation and runs to completion;
   - the old timer, when it fires later, is inert.
7. **Cross-owner.**
   - Two roots due at the same boundary: the cleanup for A cannot publish or
     send for B, and each gets exactly one Timeout.
   - A live third root and a Plan owner are untouched.
   - There is one engine timer, and the empty guard emits no timer actions.
8. **Public entrypoints.** `http_prepared`, `claim_http`, `prepare_dispatch`,
   a frame call, `apply_public_feed` and `drain_actions` (at
   `io.dispatch_now`) each expire a due root first.
9. **Missing receipt.** `reject_unstamped_command` gives one typed failure:
   Read with outcome None, Mutation with NotSent. It is Revoked with no effect
   when the receipt is live, and `admit_caller_read_root` agrees. Cases:
   - a plain `handle` at `f64::MAX`;
   - an exhausted sequence;
   - a duplicate of a live legacy id (whose original stamp is preserved).
10. **Expired at admission.** With under 1 ms left, or at or past the
    deadline, `admit_caller_read_root` returns Revoked with no effect and the
    entry stays Fresh. `reject_expired_command(Read)` is then the only route,
    and it gives one Timeout.
11. **Plain Read only.** A Mutation sibling, a replacement or a conversion on
    a tracked owner is refused before any id or journal change. The root
    still expires normally.
12. **Reservation refusal.** Forced index, timer or id capacity refusal gives
    one typed rejection and leaves nothing tracked: no owner, timer or credit.
    A fresh admission immediately afterwards succeeds.

## Files

- `sdk-rs/engine/src/request_context_entry.rs`: the two Cx APIs, using
  #1949's `command_receipt`.
- `request_context_registry.rs`: the `expired` field and the validation
  checks.
- `engine.rs`: the index, timer, sweep call sites, the seal and the Venue
  hook.
- `http_lifecycle.rs`: failure-only cleanup reusing the expiry paths.
- A new `engine/src/read_root_expiry_tests.rs`.
- This plan, the README row and CALLERS.

There are no venue adoptions here. Hyperliquid history (and the OKX, Bitget
and WEEX history slices) consume it in their own PRs. WASM size is measured
against the Bybit venue budget before the PR is ready.

Architecture gate: yes, for engine custody (a new engine-owned terminal). The
API is reviewed by codex-uufub4 before REDs.
