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
        credential: Option<CredentialSlot>,
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
    /// The engine has retired this tracked Read root at its deadline. Drop
    /// any queued continuation for `owner`. No Cx, so no completion, send or
    /// Store rights; the engine seals the command itself.
    fn read_root_retired(&mut self, owner: Owner) {}
}
```

### `admit_caller_read_root` outcomes

| Receipt state in the Fresh entry | Result | Effect |
| --- | --- | --- |
| Own live receipt, at least 1 ms left | `Ok(root)` | Root deadline is the exact receipt deadline; tracked. |
| Own receipt expired, or under 1 ms left | `Err(Revoked)` | None. The entry stays Fresh for `reject_expired_command(Read)`. |
| Receipt missing | `Err(Rejected)` | One typed Read failure with outcome None, through #1949's `with_command_entry(Read, Err(cause))`. |
| Not Fresh, borrowed callback, or delivery set | `Err(Revoked)` | None. |

The root policy is Read, `RetryPolicy::Never`, with the given credential and
pacing. Legs are minted under it with the existing `http_leg_context` and are
capped at the root deadline (`request_context_registry.rs:342-360`).

## Engine mechanism

1. **Index.** `Io.read_roots: Vec<(f64, CommandId, generation)>` holds tracked
   roots, with one engine timer at the earliest deadline.
   - The timer is armed and cleared like `execution_timer`
     (`execution_cx.rs:20-38`), with no venue `on_timer`.
   - The sweep's empty guard is O(1): `if read_roots.is_empty() { return }`.
   - An entry leaves the index when its command seals for any reason, at the
     `finish_sealed_command` retirement point.
2. **Sweep position.** `expire_read_roots(now)` runs at every public engine
   entry, before any codec or physical timer can lend authority:
   - first in `handle_received`, before `process_socket_work` (3294) and
     therefore before `expire_plan_timer` and `expire_http_timer` (3301);
   - at the top of `http_prepared`, `http_preparation_failed`, `claim_http`,
     `prepare_dispatch` and `drain_actions`.

   Expiry at equality: `now >= deadline`.
3. **Per expired root, in this order:**
   1. **Mark.** Set `expired` on the owner's `Context` phase (a field inside
      the variant, so existing `Context { .. }` matches are unchanged). Once
      set, the following refuse with `Revoked`:
      - `validate_completion_as`, for venue authority seals through the root
        and numeric `done`/`fail` for that id;
      - `validate_read_completion_context`;
      - `validate_read_http_delivery`;
      - `http_leg_context`.

      From here the owner is unsendable and non-publishing.
   2. **Failure-only cleanup.** Each outstanding managed request of that owner
      is found by scanning `managed_http` (by `delivery().context.owner()`, as
      in `expire_plan_calls`). It goes through the existing typed paths:
      - a released request gets `HttpFailed` with kind Timeout;
      - an unreleased one gets `http_preparation_failed(TimedOut)`.

      The venue gets its Pending back and its own cleanup runs, but every
      terminal attempt is inert because of step 1. Native accepted-prefix
      recovery cannot publish. Sent and delivery custody is preserved for
      reconciliation, and nothing is re-admitted or retried. This mirrors
      the plans-only precedent `http_plan_expiring`
      (`request_context_planned.rs:816-829`).
   3. **Queued state.** `venue.read_root_retired(owner)` is called with no Cx
      (model: `discard_unproven_http_command`, `engine.rs:2539`). The venue
      drops parked continuations, such as the Hyperliquid `pump_reads` history
      entries.
   4. **Seal once.** `finish_command(id, Err(Timeout read error), Some(&root))`
      runs with `http_delivery == None`, through an engine-only seal origin
      that is exempt from the step-1 flag. It reaches `finish_sealed_command`
      and emits one `CommandFailed { kind: Timeout, outcome: None }`. The owner
      is then removed, so later generation checks return `Stale` or `Revoked`.
4. **Panics.** Steps 2-4 run under a poison guard like `ExpiryScope`. An
   unwind poisons the engine instead of leaving a half-retired owner.

## Invariants

- **Exactly once.** Root expiry gives exactly one terminal: a strict Timeout
  with outcome None. It publishes no accepted rows, no INCOMPLETE marker and
  no native finalizer.
- **Leg timeout with a live root.** This is unchanged: the venue's native
  page-failure recovery, including accepted-only partials, still applies.
- **Timer order.** Root first and leg first are equivalent. The sweep runs
  before `expire_http_timer`, so a same-instant leg timer finds the owner
  already marked.
- **Late replies.** A late reply or DATA for a retired owner is inert,
  through the existing generation and leg validation.
- **Reused ids.** A reused command id gets a new generation and a new receipt.
  A stale index entry or timer never touches it.
- **Isolation.** Cleanup never completes, re-admits or retries another owner.
- **Known gap.** This PR does not close a gap that predates it: numeric
  `cx.done/fail(id)` after an owner is removed publishes through
  `Seal::Legacy`. Adopting venues must use their root authority only.
  CALLERS documents this.

## Original REDs (after plan review)

These run against existing engine APIs with a probe venue, using the real
`handle_command` receipt and managed Prepare/Bind/Claim. API-shape tests are
labelled as new-API tests, not REDs.

1. **Parked root.** A tracked root parked with no HTTP reaches its deadline.
   There is exactly one `CommandFailed` with kind Timeout and outcome None, and
   no venue callback is needed. The probe's later `finish_with(Err)` with
   accepted rows publishes nothing. (Today: no terminal at all.)
2. **Released leg in flight, root deadline equals the leg deadline.**
   - Root first then leg, and leg first then root: one strict Timeout in each.
   - An accepted earlier page is never published.
   - The probe's failure callback receives its Pending exactly once.
3. **Unreleased leg in preparation.** The same as 2, through
   `on_http_preparation_failed`.
4. **Leg-only timeout with the root live.** Accepted-only partial recovery
   still publishes, as today.
5. **Late DATA and late Bind after expiry.** Inert, with no second terminal.
6. **Reused id.** After expiry, the same id with a new receipt runs to
   completion. The old index entry does nothing.
7. **Isolation.** Another live root and a Plan owner are unaffected. The
   index keeps one engine timer, and the empty guard emits no timer actions.
8. **Public entrypoints.** `http_prepared` or `claim_http` called after the
   deadline expires the root first, so no Bind or release happens.
9. **Missing receipt.** `reject_unstamped_command` gives one typed failure:
   Read with outcome None, Mutation with NotSent. It is Revoked with no effect
   when the receipt is live, and `admit_caller_read_root` agrees. Cases:
   - a plain `handle` at `f64::MAX`;
   - an exhausted sequence;
   - a duplicate of a live legacy id (whose original stamp is preserved).
10. **Expired at admission.** `admit_caller_read_root` returns Revoked with no
    effect, then `reject_expired_command(Read)` gives one Timeout.

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
