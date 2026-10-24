# Rust Plan Session Wait

Owner: claude-fe7lgm (Plan owner). Consumer: codex-zs5e83 (Paradex, 36
engine-gated sites). Composes with claude-m26ps4's Session-fenced JIT Bearer
hook (`RUST_SESSION_BEARER_HOOK.md`, feat/rust-session-bearer-hook).

## Problem

A Paradex private Read or write runs as a command Plan (or a retained Preflight
read). Its legs need the venue's login Session to be ready, because the Bearer
is derived from that Session at prepare time. While login is in flight, the leg
must wait. Today it cannot:

- `Cx::session_wait` refuses any owner whose command is not in the plain
  Context phase (`context_phase`, request_context_registry.rs:867), so Plan and
  Preflight owners get `State`.
- Parking an already-issued leg would not help. A Plan leg's Context deadline
  is `timeout × attempts` from issue (`CallTerms::window`), so a login wait can
  outlive it. A write leg is also journal-registered at issue
  (`make_plan_leg_context`). Resuming it would need a new Context under a
  journal entry keyed to the old leg.
- Re-issuing the leg from the `session_ready` drain fails custody:
  `plan_leg_context` requires the handoff delivery, initial custody or Plan-ready
  custody (request_context_planned.rs:543-561). Queueing requires
  `delivery == call.admission.predecessor` (`state.admission`).

## Scope

This API covers command-owned Plan legs and retained Preflight reads, which
are Paradex's history and trading sites. It is not, on its own, clearance for
all 36 gated sites (account 9, history 9, identity 3, trading 15):

- identity can use the existing plain-Context `session_wait`;
- account config, auth and background snapshot lifecycle use Session or plain
  Background paths, and need their own native geometry audit;
- G1 Background Plans stay excluded.

No Paradex site needs to park an already-released call. Already-bound,
unreleased packets use the Bearer generation/revision fence and reprepare, not
this wait. The cached-JWT gate that precedes dispatch today maps to
wait-before-issue/select.

## Design

Park the unit of work **before** it consumes an attempt, and do the issue or
select on resume, inside the custody the resume grants.

### Park points

1. **Unissued Slot.** `Cx::plan_session_wait(plan, slot, session, pending)`.
   - It needs exactly the authority `plan_leg_context` needs: a live command
     Plan, an available and eligible Slot, `Credit::Unissued`, and Plan custody
     (handoff, initial or ready).
   - It marks the entry `parked: Some(generation)`. The credit stays
     `Unissued`, and the Slot is reserved: it is excluded from `plan_slots` and
     from the eligibility pump, and it cannot be issued, settled or terminally
     retired by the venue.
   - No Context, ID, journal entry or attempt is spent.
2. **Failed call awaiting retry.**
   `Cx::plan_retry_session_wait(plan, slot, context, evidence, session, pending)`,
   in the failed delivery's custody. It runs the same checks as
   `plan_retry_context` / `plan_resend_context`: evidence, `selected <
   attempts`, and window remaining. It consumes nothing: the call moves to
   `Stage::Parked { generation }`, and the failed attempt's leg is retired as
   usual. A resend grant is checked here and consumed only at resume.
3. **Retained Preflight read.** `Cx::preflight_session_wait(handle, session,
   pending)` before `preflight_read_context`, and the retry variant after a
   failed read call. The same rules apply. The Preflight keeps its generation.

Execution-owned (physical) Plans are refused with `State`, as are Background
Plans. This is consistent with the G1 `context_phase` ruling.

### Resume

`Cx::session_ready(session)` keeps its signature and return type. For a Plan or
Preflight waiter, the engine does the deferred work at drain time, with the
**current delivery as predecessor**:

- **Slot:** it issues the leg via `make_plan_leg_context`, with resume custody
  standing in for Plan custody. The result is a fresh Context: a new physical
  window from now, capped by the Plan root, which is "the eligible 20 s child".
  Journal registration, the leg intent (if one was declared at park) and the
  first attempt happen only now.
- **Retry:** it runs `select_plan_retry`, consuming exactly one attempt within
  `terms.attempts` and the declared `RetryWindow`, under existing declaration
  and evidence only, with no new grant.
  - FreshAttempt gets a fresh physical deadline.
  - Shared keeps the original call window and is never renewed to 20 s. A
    Shared call whose window lapsed while parked comes back `Failed`.
- Every resumed deadline stays capped by the Plan deadline. That is already
  `min(native, #1949 receipt cap)` for a command root, so a wait can never
  extend the caller's receipt. The Slot reservation is exact, and no credit is
  consumed while parked.
- It returns `Waiter::Ready(context, pending)`, exactly as for a plain waiter.
  The venue queues the leg in the same callback, because custody is bound to
  that delivery.

If the work can't be done at resume (the Plan root has expired, or the window
has less than 1 ms left), the waiter comes back `Waiter::Failed(..)` with a
typed Timeout/NotSent. The Slot is unparked, and the Plan's own expiry or
terminal path decides the command terminal.

The original root (the Plan or Preflight generation), the Slot and the attempt
lineage are preserved: nothing is consumed while parked.

### Fencing

- **Expiry.** The wait timer is at the Plan root (or Preflight) deadline, not a
  leg deadline. Plan root expiry runs first and seals the Plan through the
  common path. The parked entry's pending is then delivered once via
  `on_session_wait_expired`, NotSent. A later wait-timer firing is a no-op.
- **Retire or failed session.** `retire_session` / `session_failed` keep their
  return contract for plain waiters. Plan and Preflight waiters are unparked
  and their pendings queued. Each is delivered **after the current callback
  exits** through one new hook, `Venue::on_plan_session_wait_ended(cx, pending,
  error)`, run in that Plan's (or Preflight's) ready custody. The venue can
  then re-park on a new Session after re-login (the Paradex JWT regeneration
  case), or `fail_plan` / `fail_preflight`.
  - The engine never fails the Plan itself.
  - Each owner is visited once, with storage reserved at park.
  - This is the same queued fan-out shape as Bearer R10, so both use one flush.
- **Retire ordering (shared with Bearer R10).** One Plan can hold both a
  parked unit and another leg's bound unreleased packet on the same Session.
  The shared queued flush runs per owner, in a fixed order:
  1. Bound-packet attempt failures first, through the common
     preparation-failure path. That may seal the Plan.
  2. Then the parked pendings. If the Plan is still unsealed,
     `on_plan_session_wait_ended` runs in its ready custody. If it is sealed,
     the seal has already retired the parked unit, and its pending goes back
     once through `on_session_wait_expired` with NotSent. That runs with no
     custody and grants nothing, but it means a pending is never stranded.
  Storage for both lists is reserved before any effect, and the flush runs
  after the retiring callback exits. The result is exactly one terminal per
  owner and no custody callback after a seal.
- **Projection.** Parking adds no outcome. A wait that ends unissued is NotSent
  for that attempt only. The command terminal still comes from
  `failure_projection`, so prior released attempts and #2016's inherited fact
  stay Unknown.

**Outcome wording.** "Timeout/NotSent" here describes only the unissued
attempt and the pending bookkeeping. The command terminal always comes from
the common projection: a Read has no financial outcome, an initial Mutation is
NotSent, and a prior released attempt or an inherited fact is Unknown. The wait
never overrides an outcome.

### Composition with the Bearer hook (m26ps4)

- A parked unit has no packet, so the Bearer hook never runs for it. After
  resume, the venue queues the leg with the new `bearer: Some(&session)` binding
  on `request_managed`, and prepare reads the hook then.
- One retire flush: Bearer R10 fails bound unreleased packets, and this plan
  ends parked waits. Both are queued, once per owner, after the callback.
- A parked unit is never also a bound packet, because parking requires
  not-queued, so no owner is visited twice.
- If the Session retires after resume but before release, that is Bearer R3/R10
  (packet fence), not this plan.

### G2b groups

A Slot inside a mutation-group barrier range cannot be parked individually
(`State`). Group members are issued and activated together, so a group waits
before issuing any member: park the whole barrier range, or nothing. That is a
follow-up only if a consumer needs it, and Paradex does not.

## Original REDs and controls (on 77f12408947)

The new methods don't exist, so these are compiling controls that pin today's
gaps. None is counted as a RED for a missing method:

- C1: `session_wait(session, plan_leg)` gives `State`, sets no timer and holds
  no wait.
- C2: `session_wait(session, preflight_read_context)` gives `State`.
- C3: from a `session_ready` drain outside Plan custody, `plan_leg_context` on an
  eligible Slot gives `Revoked`.
- C4: a Plan leg issued before a wait longer than `timeout × attempts` cannot be
  queued after it (`validate` gives `TimedOut`). This is the reason to park
  before issue.

New-contract REDs follow the API review:

- N1: a Slot parked, then `session_ready`: one fresh leg with a deadline of
  now + timeout (capped by the root), the same Slot ordinal, `selected == 1`,
  queued in the drain callback, one HTTP.
- N2: a retry parked after a failed attempt: resume consumes exactly one
  attempt, and a parked call never exceeds `terms.attempts`.
- N3: Plan root expiry while parked: one Plan terminal (Timeout), the pending is
  delivered once via `on_session_wait_expired`, and there is no HTTP.
- N4: retire while parked: no HTTP, `on_plan_session_wait_ended` runs once
  after the callback in ready custody, re-park on a new Session succeeds, and
  resume issues the leg.
- N5: the projection is unchanged. A released, unanswered first attempt, then a
  parked retry, then a retire and `fail_plan`, gives Unknown. With #2016, an
  inherited fact plus a parked Slot gives Unknown.
- N6: custody. Parking outside Plan custody, on an execution or Background
  Plan, on a group Slot, twice on one Slot, or with a foreign or stale Session
  gives `Revoked`/`State`/`Duplicate`. Each refusal spends no IDs or timers.
- N4b (re-login ordering, with Bearer R14): retire the old Session and open
  the new one in the same callback, which is outside any readiness scope. After
  that callback, `on_plan_session_wait_ended` runs in ready custody and re-parks
  on the new Session. Its login completes, the drain resumes, and there is one
  HTTP. Opening a Session inside ready scope stays `State` (R14), so a venue
  with no replacement Session can only `fail_plan`.
- N6b (scope, with Bearer R14): in Plan-ready or Preflight-ready scope, parking
  the owner's own unit succeeds, while `open_session`, `retire_session`,
  `session_context` and `session_ready` in that scope are refused or empty.
- N8 (mirrors Bearer R10b): a Plan with one parked Slot plus one bound
  unreleased leg on the same Session, then retire. The bound failure seals the
  Plan (one terminal), there is no `on_plan_session_wait_ended`, the parked
  pending comes back once via `on_session_wait_expired`, and there is no HTTP.
- N8b (mirrors Bearer R10b'): the bound failure does not seal, so the Plan
  survives under its retry policy. `on_plan_session_wait_ended` runs exactly
  once in ready custody after that failure, and `on_session_wait_expired` does
  not run. Note that `session_failed` does not fence bound packets (they keep
  their token unless the revision moves or the hook returns NotReady), so N4
  only ends parked waits.
- N7: Preflight. A parked read resumes as a fresh read context under the same
  Preflight generation, with the predecessor equal to the drain delivery.

## Source review notes (m26ps4, 55ao7t)

- **No timer for a parked Slot.** The Plan's root expiry seals first, and the
  sealed sweep returns the pending with Timeout/NotSent; a Plan the venue
  sealed returns InvalidInput/NotSent. This removes the equal-deadline race.
- **One flush, in place.** `flush_plan_session_waits` runs last in
  `finish_event_scope` and loops until both the sealed sweep and the ended
  queue are empty, consuming entries in place so the storage reserved at park
  survives. A callback inside the flush that ends another wait is served in
  the same flush, and a Plan sealed by one hook returns its sibling parked
  Slots in that flush too.
- **Held until disposition (uufub4 P1).** A Slot whose Session ended stays
  parked until `flush_plan_session_waits` unparks it immediately before its
  disposition. That way, ordinary readiness in the same flush cannot issue it
  first.
- **Outer flush (uufub4 P1, zs5e83).** The flush is called from
  `flush_http_plans`, so it runs at every outer Plan flush: after delivery
  exit, on the Plan-root timer path and at the end of the event scope. A login
  response or a Plan expiry gets its dispositions in that same `handle` call.
- **Bearer order lives inside the loop (m26ps4).** `flush_http_plans` also runs
  at sites with no preceding owner-retirement flush: delivery exit, the
  Plan-root timer and `drain_actions`. A callback inside this loop can also
  retire a Session. So each pass of `flush_plan_session_waits` must drain the
  Bearer bound-failure queue first, then the sealed sweep, then ended.
  Whichever of #2037 and the Bearer hook lands second wires that per-pass call.
- **Resume without a delivery.** A Session proven from a socket message or a
  timer drains with no `http_delivery`. The resumed leg then records no
  predecessor and must be queued in that same callback, which also has no
  delivery. N1 resumes this way.
- **After combo-5b (#1896/#1897):**
  - `session_scope()` stays the first check in session_ready, session_failed,
    retire_session and session_wait, ahead of the Unit loop.
  - `plan_session_wait` does not take `session_scope()`, because it is the ready
    Plan's own operation. Re-park inside `on_plan_session_wait_ended` must target
    an already-live Session, since opening one there is refused. A control will
    pin that re-park in the hook succeeds.
  - The flush guard gains #1896's `http_preflight_ready` (and G1's
    `http_background_activation`, when it lands), so no venue hook runs inside
    another owner's readiness scope. Add a control.
  - Test `SessionPolicy` literals gain `private_egress: None`, with a workspace
    check and check-bans on that composition.

## Delivery phases

1. **Slot park (this change).**
   - `Cx::plan_session_wait(plan, slot, intent, session, pending)`.
   - Resume through `session_ready`, which issues the leg at drain time via
     `make_plan_leg_context(.., resumed)`.
   - Expiry through the wait timer and the sealed-Plan sweep.
   - Retire/failed are queued, and `flush_plan_session_waits` runs at the end of
     the event scope. It routes the ended hook in ready custody, or the
     expiry hook when the Plan is sealed.
   - Covered by N1, N3, N4b, N6 and N8, plus controls C1-C4.
2. **Retry park.** `plan_retry_session_wait` (N2), next.
3. **Preflight park.** `preflight_session_wait` and its retry twin (N7).
4. **Bearer composition.** N8b and R10b' land with or after the Bearer hook's
   packet fence.

## Gates

Engine focused tests in every feature mode; clippy `-D warnings`; fmt. Review by
codex-zs5e83 (consumer fit for the 36 sites) and claude-m26ps4 (Bearer
composition).
