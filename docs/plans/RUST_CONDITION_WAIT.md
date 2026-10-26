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
    /// Park a WriteAfterReads Preflight in genuine entry or Preflight-ready
    /// custody. `None` is unbounded; `Some(ms)` expires into
    /// `on_condition_wait_expired`. Release with `request_preflight_ready`.
    pub fn park_preflight_for(
        &mut self,
        preflight: &Preflight,
        bound_ms: Option<u32>,
    ) -> Result<(), Error>;

    /// Between serial calls, from the current call's exact custody under the
    /// same checks as `continue_serial_plan` (definitive fact, no
    /// uncertainty). Settles the call and parks the Plan with no live call.
    /// `next` is issued after release, so a replan picks its re-read here.
    pub fn serial_plan_condition_wait(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        next: SerialCall,
        bound_ms: Option<u32>,
    ) -> Result<ConditionWait, Error>;

    /// Settle-only: from the same custody, settle the current call and park
    /// with NO next entry, for proof that arrives only on a stream. Bounded
    /// (`bound_ms > 0`) and one-shot; release readies `done_plan`, expiry fails.
    pub fn serial_plan_settle_wait(
        &mut self,
        plan: &Plan,
        slot: &Slot,
        context: &Context,
        bound_ms: u32,
    ) -> Result<ConditionWait, Error>;

    /// From any callback, borrowed included. Grants nothing directly: it
    /// schedules the Plan's `on_http_plan_ready`, in this flush.
    pub fn release_condition_wait(&mut self, wait: &ConditionWait) -> Result<(), Error>;
}

pub struct ConditionWait { /* Plan, wait generation */ }
pub enum ConditionOwner { Preflight(Preflight), Plan(Plan) }

trait Venue {
    /// Failure custody; a hook that returns without a terminal is sealed once
    /// as NotSent.
    fn on_condition_wait_expired(&mut self, cx, owner: &ConditionOwner, error: VenueError) {}
}
```

The existing `park_preflight` keeps its signature (Initial custody,
unbounded); every addition is a new item, with a default Venue hook.

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
- **Socket release unexercised.** RED19 releases from a timer and refuses a
  foreign-engine handle. The release path does not depend on the callback
  kind, and Bitget detection is HTTP. The first consumer that releases from a
  socket `on_message` adds that control.
- **Stops without an engine entry.** Shutdown and caller cancel have no
  engine entry point today (serial Plan RED19 covers revoke, provider refusal
  and Session only). Whoever adds those entry points owns their controls for
  parked owners too.
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

## Review pins (claude-dazzy3)

- **Sequential waits.** "One per owner" means at once. Once a release has been
  delivered, the same owner may wait again (Bitget re-parks after a
  promotion), with one Context throughout.
- **No re-entry.** A release, even from the callback that parked or from any
  callback of the same owner, only queues. Ready is delivered after the
  current callback returns, in the same flush.
- **Declining expiry hook.** If `on_condition_wait_expired` neither fails nor
  re-parks, the engine seals once with a NotSent projection (the
  declining-handler precedent). An owner is never left parked.
- **Timer precedence.** A parked WriteAfterReads Preflight has no other engine
  timer (no receipt binding), so the bound is its only timer. The serial form
  occupies the Plan's single timer and is refused while a delayed
  continuation is armed. Either way there is exactly one terminal.
- **Locks.** The order-lock scope is acquired at engine command entry, before
  `on_command`, so before `begin_preflight` and before any park; it is not
  taken at ready. Bitget already sets `guard_order_mutations` at
  `49f7d1857b6`, so this is today's native behaviour, unchanged:
  - same-symbol placements hold separate scopes that do not conflict;
  - an overlapping order ID, or a symbol-wide cancel over a held scope, is
    refused before entry ("already in flight"), or deferred for
    CancelSymbolOrders behind placements;
  - the existing 64 active-scope limit applies.
  A parked owner keeps its scope from entry to its terminal, and its expiry
  arms no fence, because nothing is live.
- **Authority.** The Preflight form releases through the issuer-bound
  Preflight handle. The serial form's handle carries the same issuer and
  command generation, so another engine's or generation's handle is Revoked.
  A release is accepted from any callback: `on_response`, socket
  `on_message` or a timer.
- **Validation.** `bound_ms: Some(0)` is Invalid and spends nothing. A bound is
  accepted only on WriteAfterReads Preflights and serial Plans; a ReadOnly
  `park_preflight` stays as today, unbounded, until a consumer needs a bound.

## Capacity

Each parked Preflight is a live command owner, so it counts against the
engine-wide limit of 256 concurrent commands (`CAPACITY`), as every migrated
venue's commands already do. Native Bitget's queue has no limit, so a burst
of more than about 256 queued commands becomes a Capacity refusal that
native never had. Ruled (claude-ddi2il): accept and disclose. The refusal is
Capacity, NotSent, before any send, with no exemption. Bitget's CALLERS notes
the limit.

For Bitget placements the binding limit is lower: 64 active order-lock scopes
(`order_locks.rs`, `acquire`). The 65th queued placement is refused at entry,
before any send, with the order-lock capacity refusal from #2095
(Internal, NotSent) (RED23). Bitget already sets
`guard_order_mutations` at `49f7d1857b6`, so this is existing entry
behaviour, not new with the migration. It is still disclosed in Bitget's
CALLERS against native's unbounded queue. The 256 owner limit binds
non-placement commands.

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
    them wake in the same flush, in queue order. Each owner gets exactly one
    terminal, NotSent, carrying the native message.
13. Re-park from ready custody counts as progress, so no "made no progress"
    failure.
14. With 256 commands parked, the 257th is refused Capacity, NotSent, before
    any send, and the parked 256 are unaffected.
15. Sequential waits: park, release, ready, continue, park again, release,
    done, with one Context throughout.
16. A release from the parking callback itself does not re-enter. Ready
    arrives after that callback returns, in the same flush.
17. A declining expiry hook gives one engine NotSent seal, with nothing left
    parked.
18. Under order-locks, a parked owner holds its lock and a later command
    queues behind it. Expiry releases the lock through the terminal, with no
    fence.
19. Releases from a socket `on_message` and from a timer callback succeed. A
    handle from a reused generation or another engine is Revoked.
20. `bound_ms: Some(0)` and a bound on a ReadOnly Preflight are Invalid and
    spend nothing.
22. Under order-locks, two same-symbol placements: the second parks with
    `None` behind the first. Both scopes are held from entry. The first
    completes and releases only its own scope; the second wakes, admits and
    completes. Lock state is asserted at each step, with no fence.
21. In RED2 and RED6, the native message ("Bitget account detection did not
    complete") passes through unchanged. A stop racing a queued, undelivered
    release gives one terminal and no ready.
10. A mid-Plan expiry after an earlier released and accepted call: `fail_plan`
    projects the accepted IDs and a failure that is not NotSent (native
    `trade_fail` keeps the IDs once anything was effective).

## Source notes

- **Preflight form.** `park_preflight_for` parks from Initial or
  Preflight-ready custody, setting a condition-park flag. A bound arms one
  engine timer, tracked per Preflight. Ready delivery clears the timer and
  the flag. A re-park counts as progress. A condition-parked Preflight that
  holds nothing may be failed from any callback, so a stop racing a queued
  release gives one terminal and no ready. A declined expiry seals once:
  "Condition wait expired", Timeout, NotSent.
- **Serial form.** It is `continue_serial` with `Next::Parked`, which runs
  every continuation check and parks the named `next` call. The wait lives in
  `Serial.condition` (generation, deadline) and folds into the Plan's one
  timer. Release checks issuer and generation, then unparks and makes the
  call eligible now, so delivery is the normal flush into
  `on_http_plan_ready`. A declined expiry seals through `fail_plan`: the
  streamed IDs are projected, with no fence.
- **WASM bytes** (release, against the frozen serial Plan `d4efb3f6b15`):
  Bybit 1,336,171 to 1,338,378 (+2,207); WOO 1,240,977 (+0); Bitget 1,331,269
  (+0). Bybit pays for the Preflight-ready paths it already links.
- **Changes from the frozen REDs.** `serial_plan_condition_wait` gains `next`
  (Bitget's 40085 replan re-reads after release), and the tests pass
  `mutation()`. RED24 is a new control: a parked Plan issues the Read it named.
  RED23 pins the existing entry refusal; it had expected NotSent, which that
  shared refusal never carried, so changing it would alter every venue that
  guards order mutations. The fixture fixes are RED11 (answer every prepared
  call in a batch) and RED14 (entry refusals counted apart from parks).

## Settle-only waits

`serial_plan_settle_wait` is for a venue that proves a write only from its
stream and has no unsigned call to name as `next` (Lighter's grouped TP/SL:
its only exact order listing is signed). It settles the current call under
`finish_plan_call`'s checks, marks the Plan settled and parks it with no
entry, so the schedule is complete and nothing can be issued. Release
(`release_condition_wait`) readies the Plan once through
`on_http_plan_ready`; the venue then calls `done_plan`. Expiry reaches
`on_condition_wait_expired` once; the venue raises the Plan with
`inherit_plan_uncertainty` and fails it, or a declining hook is sealed by the
engine with the streamed IDs. While parked, `done_plan` is refused (State);
after the settle, `continue_serial_plan`, `serial_plan_condition_wait`,
`plan_leg_context` and a second settle are refused (Invalid), as is a settle
on a non-serial (Read-policy) Plan. A release after expiry or completion is
Revoked. The caller wait never completes it, as for other parked Plans.

## Feature gate (overseer ruling)

The condition wait is linked only by adopting families, behind the
non-default engine feature `condition-waits` (it implies `serial-plans`).
With it off, `fail_preflight`, the readiness flush, Plan wakes and the timer
expiry path compile exactly as before, so non-adopting families keep their
pre-#2092 WASM sizes. `Venue::on_condition_wait_expired` exists only with the
feature, because its `ConditionOwner` argument does. The adopter (Bitget)
enables the feature in its own slice, with its own budget review. CI covers
it through the explicit control mode in `scripts/test-http-plan-feature-modes.py`.

## Gates

Engine-focused tests under `http-plans` and `http-plans,order-locks`; clippy
`-D warnings`; fmt. Independent review of the plan and REDs, then the source.
WASM bytes reported, linked only by adopters.
