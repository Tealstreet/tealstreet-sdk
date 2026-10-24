# Rust Condition Wait

Owner: claude-fe7lgm. Consumer: claude-4yjlw0 (Bitget slice 3, gate D).
Ruling needed: claude-ddi2il (scope). Independent review: claude-dazzy3.
Stacks on the streamed serial Plan (`RUST_PLAN_SERIAL_STREAM.md`).

## Problem

Bitget's trading queue waits for account detection (Classic or UTA) with a
native 10 s bound (`venues/bitget/src/trading.rs` at `49f7d1857b6`):

- `resume_trading` parks the job and arms `cx.set_timer(10_000)` while
  `!detected` (416-421). Nothing is started.
- Detection completes in the account cycle: `private_detected`
  (`account.rs:286`), reached from the Settings probe's own Background Read
  (`lib.rs:795-806`, and the account arms of `on_http_failed` and
  `on_sign_error`). That callback then calls `resume_trading`. It runs in the
  account read's custody, never the trading command's.
- `trade_timer` (2255-2267) fails the parked job, and every queued command,
  NotSent with "Bitget account detection did not complete".
- The wait recurs after admission. `promote_uta` (`account.rs:1041-1052`, also
  reachable from history and the account cycle) sets `detected = false`, and
  the 40085 replan (`trading.rs:1501-1534`) resumes into the 10 s wait mid-job,
  between serial calls. Promotion requires `job.ids` to be empty, so a
  mid-Plan expiry is still NotSent.

Both the release and the bound live in borrowed callbacks today: a foreign
read's custody and a venue timer. Neither may admit, continue or fail a
Preflight or Plan. A caller receipt cannot supply a 10 s venue bound.

## Design: one scoped wait, two forms

A condition wait parks a Preflight or a serial Plan on a venue condition. It
has a venue-chosen bound and an opaque, generation-checked release handle.
It holds no I/O, no live call and no venue timer.

```rust
impl<P> Cx<'_, P> {
    /// Before admission, in genuine entry or Preflight-ready custody.
    pub fn preflight_condition_wait(
        &mut self,
        preflight: &Preflight,
        bound_ms: Option<u32>,
    ) -> Result<ConditionWait, Error>;

    /// Between serial calls, from the current call's exact custody under the
    /// same checks as `continue_serial_plan` (definitive fact, no
    /// uncertainty). Settles the call and parks the Plan with no live call.
    pub fn serial_plan_condition_wait(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        bound_ms: Option<u32>,
    ) -> Result<ConditionWait, Error>;

    /// From any callback, borrowed included. Grants nothing directly: it
    /// schedules the owner's ready callback in its own custody, in this flush.
    pub fn release_condition_wait(&mut self, wait: &ConditionWait) -> Result<(), Error>;
}
```

- **Release.** `release_condition_wait` checks only the handle's generation.
  The engine then delivers the existing ready hook in proper custody:
  Preflight-ready custody for the first form, and `on_http_plan_ready(plan)`
  for the second. There the venue plans and admits, or continues. This is the
  N7 shape (a Session ready resumes a parked Preflight) with a venue
  condition in place of a Session.
- **Expiry.** One engine timer at `now + bound_ms`. When it fires, a new hook
  `on_condition_wait_expired(owner, error)` runs in failure custody, where the
  venue calls `fail_preflight` or `fail_plan`. The engine never chooses the
  native message. A serial Plan's projection keeps every streamed accepted
  ID, so the projection stays exact even if a venue parks with IDs.
- **Bound.** `bound_ms` is computed by the venue per call, so it need not be
  a constant. Bitget passes `shared_deadline - now` so that every queued
  command fails at native's one shared 10 s deadline. `None` means no engine
  bound, for Bitget's queue behind an active job, which native never bounds
  (ruling needed). A caller receipt never bounds a parked owner: a
  WriteAfterReads Preflight binds no receipt, so caller-wait expiry is not a
  terminal (as serial Plan RED18).
- **Concurrency.** Many owners may each hold one wait at once, up to the
  engine's existing `CAPACITY`; past it, a new wait is Capacity and spends
  nothing.
- **Single use, one per owner.** A handle releases once. A second wait on the
  same owner is State. A stale handle (expired, released, or a reused command
  generation) is Revoked and spends nothing. Release after expiry, and expiry
  after release, are no-ops for the loser.
- **Serial Plan rules.** A parked serial Plan has no live call and keeps its
  one reserved Context. The wait uses the Plan's one timer, so it excludes a
  delayed continuation. The RED19 stops (revoke, Session retirement,
  shutdown, cancel) end a parked Plan at once, because nothing is live.
- **Bitget queue.** Each command queued behind the active job is its own
  WriteAfterReads Preflight, parked with `preflight_condition_wait`. At
  detection it uses the shared remaining bound; behind an active job it uses
  `None`. The venue releases the next one when the active job ends, and its
  ready hook admits it. Declared Plans keep their current surface.

## Preflight form: reuse what exists

The engine already parks a Preflight on a venue condition
(`request_context_preflight_ready.rs`):

- `park_preflight(handle)` relinquishes the genuine initial callback with no
  I/O or time.
- `request_preflight_ready(handle)` may be called from any callback. It queues
  one wake, FIFO, and grants that callback nothing.
- The wake is flushed in the same turn into `on_http_preflight_ready`, in
  Preflight-ready custody. A ready callback that makes no progress is failed.
- `fail_preflight` is allowed in that custody, and from any callback that
  holds no loans (a venue timer), as failure-only cleanup.

So the Preflight form adds no new release or expiry API.
`preflight_condition_wait` is dropped. What is new:

- **Re-park from ready custody.** `park_preflight` today accepts only the
  Initial activation. A woken job that finds `!detected` again (after a
  promotion) must park again. `park_preflight` also accepts Preflight-ready
  custody, and that counts as progress.
- **Bound.** `bound_ms: Some(_)` is an engine timer that, when it fires,
  delivers `on_condition_wait_expired` in failure custody. `None` is the
  existing unbounded park (ruled). Bitget passes its shared remaining bound.
  A venue timer that calls `fail_preflight` stays legal by the existing rule,
  but the engine bound is the ruled path.
- **Native order.** Wakes are FIFO, so releases in queue order run in that
  order.

The serial-Plan form below is new: a Plan with released calls cannot be
failed from a loan-free callback today (`validate_plan_seal_origin`).

## Capacity

Each parked Preflight is a live command owner, so it counts against the
engine-wide limit of 256 concurrent commands (`CAPACITY`), as every migrated
venue's commands already do. Native Bitget's queue has no limit, so a burst
of more than about 256 queued commands becomes a Capacity refusal that
native never had. Ruling needed: accept and disclose, or exempt.

## REDs (before source)

1. A Preflight parks with no I/O. A release from a foreign Background read's
   `on_response` delivers Preflight-ready custody in the same flush, and a
   serial Plan admits there.
2. A parked Preflight expires at exactly `now + bound_ms`, giving one
   `on_condition_wait_expired`. `fail_preflight` there projects NotSent. A
   late release is a no-op.
3. A serial Plan parks between calls after a definitive fact (the 40085 path),
   holds one Context and no live call, then is released from a borrowed read
   and continues in `on_http_plan_ready`.
4. Parking a serial Plan is refused while a fact is uncertain, outside the
   call's custody, or with a delayed continuation armed. Each refusal spends
   nothing.
5. A stale or double release, a second wait on one owner, and a release from a
   reused command generation are each refused and spend nothing.
6. A parked serial Plan expires, then `fail_plan` projects every streamed ID
   (none on the Bitget path) as a failure, with no fence because nothing was
   live.
7. The RED19 stops end a parked Plan or Preflight immediately, with one
   terminal.
8. Three Preflights park at once with computed bounds (10 s, 4 s, 1 s from one
   shared deadline) and all expire at the same instant. Past `CAPACITY`, a new
   wait is Capacity.
9. An unbounded (`None`) park survives past the 120 s caller wait with no
   terminal and is released by the venue in a borrowed callback.
11. Leak control (ruling): every `None` park ends through a release or one of
    the stops. A release in queue order wakes in that order, and engine
    teardown with parked owners leaves no owner, wait or timer.
12. Detection expiry fan-out: in `on_condition_wait_expired` for the active
    owner, the venue requests readiness for every queued Preflight. All of
    them wake in the same flush, in order, and each `fail_preflight` there
    projects NotSent.
13. Re-park from ready custody counts as progress, so no "made no progress"
    failure.
10. A mid-Plan expiry after an earlier released and accepted call: `fail_plan`
    projects the accepted IDs and a failure that is not NotSent (native
    `trade_fail` keeps the IDs once anything was effective).

## Gates

Engine-focused tests under `http-plans` and `http-plans,order-locks`; clippy
`-D warnings`; fmt. Independent review of the plan and REDs, then the source.
WASM bytes reported, linked only by adopters.
