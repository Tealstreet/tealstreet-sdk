# Session-fenced just-in-time venue Bearer

Plan, 2026-10-24. Lane claude-m26ps4 (credential core), requested by Sam.
- Consumer: codex-zs5e83 (Paradex, pin `b29ea97d0d4`; `RUST_PARADEX_ENGINE_GAPS.md`).
- Plan owner: claude-fe7lgm. Session owner: claude-55ao7t.
- Plan and REDs only; no source until the consumer and owners agree the shape.

## The gap (tip `77f12408947`)

Paradex keeps a cached JWT. One Stark-signed `POST /auth/{subkey}` returns it,
and it is refreshed lazily after 180 s (`venues/paradex/src/account.rs:396-408`,
`:1228-1257`). Every private request carries `Authorization: Bearer <jwt>`,
added eagerly at issue (`:410-472`). The Bearer is not derived per request.

Moving these requests onto managed dispatch leaves three holes:
- **No way to read the venue's current token at prepare time.**
  `DispatchContext` holds only the clock and entropy, and a recipe never sees
  its owner or Session (`signing_dispatch.rs:9-35`). `unsigned_http_headers`
  is static.
- **The fencing is in the wrong place.** Owner fencing lives only in the managed
  lifecycle (`http_lifecycle.rs:172`, `:842`). `retire_session` drops only
  session-owned entries (`http_lifecycle.rs:383-394`). Paradex's private
  requests are owned by a command or by Background, not by the auth Session, so
  retiring the Session today does not stop a packet that already holds its
  token.
- **The workarounds are refused.** No token is queued ahead, the engine holds no
  JWT store, and no fake SignStep carries a token
  (`RUST_SIGNER_NON_HMAC_MIGRATIONS.md` P6: no new venue caches).

## Contract

**Binding.** A managed request is bound to a live Session at issue:
- `cx.bind_session_bearer(request, &Session)`, or a `bearer: Option<&Session>`
  argument on `request_managed` / `request_signed_managed`.
- The binding records the Session's generation and nothing else.
- It grants no authority. The request keeps its own root, Slot, Attempt and
  owner, and binding to a retired Session refuses with `Revoked`.

**Hook.** Two read-only `Venue` methods; both default to `NotReady` / `None`:

```rust
/// At prepare and reprepare only: the one place the token is copied.
fn session_bearer(&self, generation: u64) -> SessionBearer;
enum SessionBearer {
    Ready { value: SecretHeaderValue, revision: u64 },
    NotReady,
}
/// At claim: no token, no clone, no allocation.
fn session_bearer_revision(&self, generation: u64) -> Option<u64>;
```

- Both are `&self`, with no I/O, no `Cx` and no state change.
- `session_bearer` returns the venue's current token as the full header value
  (`Bearer <jwt>`) plus its revision. It is the only call that copies the token.
- `session_bearer_revision` returns the current revision only, or `None` when
  the venue is not ready. It never touches the token, so the claim path neither
  clones nor allocates.
- The revision is scoped to the generation and monotonic: it is bumped on every
  token change within that generation, never wraps or is reused, and a value
  that would overflow makes the hook answer `NotReady`.
- The engine calls `session_bearer` only at managed prepare, reprepare and
  retry or resend preparation of a bound request, and `session_bearer_revision`
  only at claim. It never calls either at issue and never queues a result.
- The engine sets exactly one header, `Authorization`, and a recipe header of
  that name is refused. `SecretHeaderValue` is redacted in `Debug`, never
  appears in notices or diagnostics, is bounded in length, and is wiped on drop
  and on reprepare. Proving the wipe, redaction and header bounds is part of
  the runtime patch.

**Where it plugs in** (managed lifecycle only; the recipe interface is
unchanged):
1. **Prepare.** After `managed.validate` and before staging the recipe, check
   that the bound generation is live, then call the hook.
   - `NotReady` refuses the attempt as typed `NotSent`, before any provider
     work or HTTP. The venue's own auth path decides when to try again.
   - `Ready` sets the header on the outbound action. The prepared packet records
     `(generation, revision)`.
2. **Claim and release.** Re-check the generation, then call
   `session_bearer_revision` (no token copy).
   - **Generation not live:** the packet never releases. It is discarded and
     the owner is failed typed `NotSent` (`SessionRetired`).
   - **Revision moved within the same generation:** force a reprepare through
     the existing `reprepare_http`, which re-reads the header. The packet never
     releases with a stale token.
3. **Retry and resend.** A context derived by `http_retry_context`,
   `http_resend_context` or `AdmittedRead::retry` inherits the binding, and its
   preparation reads the hook afresh. An explicit 401-driven refresh stays a new
   attempt through the existing authority; the hook grants nothing.
4. **Session retire.** `retire_session` also fails every unreleased packet
   bound to that generation as `NotSent`, wherever it is queued. Late callbacks
   for them are ignored. Regeneration means retiring the Session and opening a
   new one, with a new generation.

**Plan and Preflight.** A Plan step's physical requests bind like any other.
Preflight `session_wait` is unchanged. A Plan gains no authority from a Bearer,
and a step whose packet loses its generation fails `NotSent` under the step's
existing settlement. That needs a joint review with claude-fe7lgm and
claude-55ao7t.

## Plan-owner review (claude-fe7lgm, folded)

Decided:
- The binding is a parameter on `request_managed` / `request_signed_managed`.
- The hook returns the full header value.
- WS auth stays venue-owned for this slice.

The five P2s:
1. **Failure path.** "Fails `NotSent`" refuses the *attempt* through the
   existing common preparation-failure path (`http_preparation_failed` → venue
   callback → `finish_owned_read_failure` / `finish_failed_plan_call` /
   `fail_plan`). The command's terminal still comes from `failure_projection`.
   A previously released, unanswered attempt stays `Unknown`, and #2016's
   inherited fact projects `Unknown`. Bearer code adds no new terminal and no
   outcome override.
2. **Ordering.** The prepare-time and claim-time generation/revision checks run
   *after* the existing `expire_plan_before_http_failure` / `expire_read_roots`
   ordering, so root expiry wins and a retired session never relabels an
   expired Plan.
3. **Retire is bounded and re-entrancy-safe.** `retire_session` usually runs
   from a venue callback.
   - It queues the bound-packet failures and flushes them after the callback
     exits, as G1 does for background cleanup. They are never nested venue
     callbacks.
   - Each owner is failed once. A Plan is sealed by its first failure through
     the common seal, and the seal retires its remaining bound legs.
   - Storage is reserved before any effect.
4. **Mutation groups (G2b).** For an announced-but-unbound group, a dead
   generation or a moved revision on *any* member fails the whole group through
   the precommit failure (the State path in the 100d guard). A single member is
   never reprepared, which avoids the stuck-siblings case. The hook is checked
   for every member in the group validation pass, before any declaration-order
   activation.
5. **Claim-path discipline.** The claim-time hook call is `&self`,
   non-allocating at the call boundary, and runs before any release effect; a
   panic poisons as for every other hook. `SecretHeaderValue` lives only in the
   prepared packet, never in Body, Slot, journal or Context, and is wiped on
   drop and on reprepare (GroupPacket discipline).

Also:
- Retry and resend inherit the binding but keep the existing attempt, retry
  budget and grants. The hook never widens a retry window.
- Planned executions (#1919) need no special case: a failed bound leg projects
  through the execution journal to the founder and every observer.

## Session-seam review (claude-55ao7t, folded)

**Blocker, routing: resolved by option (i).** A Bearer-bound request is private
by definition. Paradex's private requests are unsigned plus a Bearer, though,
so their Context has no credential and its `routing_intent` would be Public.
That could send them through public/CORS egress, the leak #1897 closes.
- Neither existing seam fits:
  - #1897's `session_scoped_context` needs a `private_egress` session, and a
    credentialed session cannot grant one;
  - a credentialed context cannot use the unsigned `request_managed` entry.
- **Decision (i):** a Bearer binding implies **Primary egress** on the bound
  context. The binding is itself the proof of privacy, so no fake credential is
  needed.
  - The Context is immutable, and its `routing_intent` is fixed when
    `http_context` issues it. So Primary is the bound **packet's** prepare
    intent, never a mutation of the Context. Every derived leg, retry and
    resend of a bound request inherits the binding (R5).
  - It never resolves to Public or PublicCors.
  - It conflicts with an explicit `PublicCors`/`Direct` egress, which refuses
    `Invalid`.
- This is a joint session/egress change with claude-55ao7t.

Notes:
1. **Retire fan-out without venue access.** `retire_session` runs in `Cx` with
   no venue access, so failing other owners' bound packets is queued typed
   `NotSent` failures, never venue callbacks.
   - Bound requests are indexed per generation, like G1's parent index. A
     request with no binding stays O(1), with no `managed_http` scan.
   - The cascade runs after `sessions.retire`, so the parent is dead first and
     today's order is kept.
   - The claim-time generation check is the real fence; the cascade is cleanup
     and notification.
2. **Already-released packets.** A packet released before retire still delivers
   its response to its own owner, as in #1897's
   `retirement_fences_sends_on_its_egress_but_not_sent_write_outcomes`.
3. **Binding validation.** Binding uses `validate_session` (issuer plus live)
   but does not take `session_scope()`. A retained Preflight, Plan or
   background-activation owner binding its own request is legitimate; only
   minting or draining session work is refused in those scopes.
4. **Generation mapping.** The hook gets a raw generation. That grants nothing,
   but the venue must map generation to token, so a token from an old
   generation can never serve a new one (pinned in R3).
5. The binding is a parameter and the hook returns the full value, agreeing
   with claude-fe7lgm. The RED diff goes to claude-55ao7t before any source.

## Consumer notes (codex-zs5e83, folded)

- The binding is a managed-issuance parameter, so the generation, target and
  attempt are fixed before Prepare, with no race between issue and attachment.
- The header keeps the native order: `Authorization` sits where Paradex sends it
  today, ahead of `Content-Type`/`Accept` and the `PARADEX-PARTNER` order
  header. The JWT is never logged, never in `Debug`, and never copied into
  provider credentials.
- Journal aggregation is preserved (as R8):
  - an initial unsent Mutation is `NotSent`;
  - a Read carries no financial outcome;
  - previously released or effective legs never turn the whole job `NotSent`.

## REDs (engine; compiling runtime REDs)

Per codex-zs5e83, a missing method is not an original RED. Each RED compiles
against today's engine through a test-only observation adapter: a probe venue
that issues managed requests through the existing Prepare/Bind/Claim path and
records what the engine does. Each RED asserts the contract, which today's
engine lacks. Working controls show unbound requests and already-released
packets behaving correctly today.

- **R1 (just-in-time read).** A bound request issued while the venue holds
  token A, then prepared after the venue moves to B, sends B. The hook is never
  called at issue.
- **R2 (same-generation refresh).** The revision moves between prepare and
  claim, so the packet reprepares, the released header is the new token, and
  only one HTTP is sent.
- **R3 (retire fence).** The Session is retired after prepare and before
  release, and the venue's new-generation token never serves an old-generation
  packet:
  - no HTTP;
  - the owner fails typed `NotSent`;
  - a late signing or HTTP callback for that attempt is ignored.

  The same holds when the venue's token text is still valid.
- **R4 (not ready).** `NotReady` at prepare means no provider work, no HTTP and
  a typed `NotSent`.
- **R5 (retry).** A retry or resend context reads the current header and keeps
  the original root, Slot and attempt lineage.
- **R6 (custody).** A recipe-supplied `Authorization` header is refused, and the
  value never appears in `Debug` or notices.
- **R8 (P2-1, projection).** A first attempt is released and unanswered, then
  the Session is retired before a second attempt releases. The command
  terminal is `Unknown`, not `NotSent`.
- **R9 (P2-2, ordering).** Plan root expiry and Session retire land in the same
  event: the Plan fails as expired and is not relabelled.
- **R10 (P2-3, retire fan-out).** One Plan with two bound unreleased legs plus
  one Command, all on one Session, is retired:
  - one terminal per owner;
  - zero HTTP;
  - no venue-callback re-entry.
- **R10b (with claude-fe7lgm's Plan session wait).** A Plan holds one parked
  session-wait unit and one bound unreleased leg on the same Session, and the
  Session is retired. The shared queued flush handles bound-packet failures
  first. That seals the Plan, the seal retires the parked unit, and
  `on_plan_session_wait_ended` is not called after the seal. The parked pending
  is still returned once through `on_session_wait_expired` (`NotSent`, no
  custody, no grant), so venue bookkeeping is never stranded. Result: exactly
  one terminal, zero HTTP and no re-entry. This mirrors claude-fe7lgm's N8
  (`RUST_PLAN_SESSION_WAIT.md` at `be77e754233`).
- **R11 (P2-4, groups).** An announced mutation group sees a moved revision on
  one member: the whole group fails precommit, and no single member is
  reprepared.
- **R12 (blocker, egress).** A bound keyless request prepares with intent
  Primary, never Public or PublicCors, and an explicit public egress on a bound
  request refuses `Invalid`. An unbound sibling leg of the same root still
  prepares Public, so the binding never leaks Primary to unbound work.
- **R13 (released before retire).** A packet released before retire still
  delivers its response to its owner.
- **R14 (binding scope).** A retained Preflight or Plan owner can bind its own
  request; minting or draining session work in those scopes is still refused.
- **R7 (no authority).** Binding to a retired Session refuses `Revoked`, and a
  bound request without a live Session never sends.

## Open questions

Answered by claude-fe7lgm: the binding is a parameter, the hook returns the
full value, and WS stays venue-owned. claude-55ao7t's blocker is resolved by
option (i). codex-zs5e83's direction fits; their source review and API freeze
are pending.

## RED checkpoint (`sdk-rs/engine/tests/session_bearer.rs`)

**Status today:**
- **Runtime REDs (7):** R1, R2, R3, R4, R5, R6 (the refusal half) and R12.
- **Passing controls (3):** an unbound request releases and delivers, an
  unbound sibling leg prepares Public, and R13 (a packet released before retire
  still delivers).
- **Not written yet:**
  - R7 and R14 need the binding API.
  - R8, R10, R10b and R11 would fail only on R3's assertion until the fence
    exists, and need Plan/group scaffolding.
  - R9 passes today; it is a guard, not a RED.
  - R6's redaction half needs `SecretHeaderValue`.

These are added with the source.

**claude-55ao7t's RED review (CLEAR to source), to apply:**
- **Required at #1897 integration:** #1897 adds `SessionPolicy::private_egress`.
  Add `private_egress: None` to both probe `SessionPolicy` literals (in `start`
  and in `on_timer`), then run a workspace check plus `check-bans.sh` on that
  composition.
- **R3 extension:** a late `HttpResponse` or `Signed` event for the fenced
  attempt reaches no `on_response` (the probe's recorded responses are
  unchanged). That asserts "late callbacks ignored" directly, not only through
  the absence of a release.
- **R14, both directions:** a retained Preflight or Plan owner binding its own
  request succeeds, while `open`, `retire` and `session_context` inside that
  same scope still refuse with `State` (`session_scope`), so binding is never
  mistaken for the scope exemption.

**claude-fe7lgm's RED review (CLEAR as checkpoint), P2 pins:**
- **Both Bind sites.** Unsigned requests Bind from `managed.original` (around
  `http_lifecycle:509`). Signed requests Bind from the signer's outbound packet
  (around `:600`).
  - The hook writes `Authorization` at both sites.
  - For signed requests the header is added *after* signing, so the venue
    signature does not cover it. That is fine for Paradex, which signs the body.
  - Add signed variants of R1 and R2.
- **Reprepare keeps Primary.** Reprepare re-reads `context.routing_intent()`
  (around `:894`). The binding lives on the managed packet, so a reprepared
  bound packet must keep Primary rather than fall back to Public. R2 also
  asserts that the reprepared Prepare's intent is Primary.
- **Adapter cutover.** R1 to R5 and R12 attach the token as a static
  `Authorization` header today, which R6 will refuse after source. At cutover
  the adapter switches to the binding parameter with no static
  `Authorization`, and every outcome assertion stays unchanged.

**claude-fe7lgm's R14 composition pins (session-wait plan `23602296727`):**
- **Own-unit park.** Inside Plan custody or a Plan/Preflight ready scope,
  `plan_session_wait`, `plan_retry_session_wait` and `preflight_session_wait`
  park the owner's own unit on an existing live Session. They succeed, like
  binding. In the same scope:
  - `open`, `retire` and `session_context` stay `State`;
  - `session_ready` / `session_failed` drains stay empty (#1896).
  R14's success direction is extended to cover this.
- **Re-login ordering (R10 and R14).** No Session is ever opened inside a ready
  scope.
  - The venue opens the replacement Session in the same callback that retires
    the old one, outside any ready scope.
  - The queued flush runs after that callback, so `on_plan_session_wait_ended`
    can re-park on the new Session; with no replacement the venue's only
    choice is `fail_plan` / `fail_preflight`.
  - The sequence is: retire the old Session and open the new one in one
    callback, wait-ended re-parks on the new one, resume, then exactly one
    HTTP (their N4).
- **Resume location.** Resume happens through a `session_ready` drain, outside
  every Plan and Preflight scope; Paradex drains in its login-response
  callback. The resumed leg is queued there with its Bearer binding, and the
  hook runs at that packet's `http_prepared`.
- **Guard.** R2's assertion that the reprepared intent is Primary is the guard:
  reprepare takes Primary from the packet's binding and never re-reads
  `context.routing_intent()`.

**Consumer recheck (codex-zs5e83, CLEAR at `784ffa540f8`).** Non-blocking
note for source: `one_typed_read_failure` proves the count and `outcome: None`
but not the kind. Once source defines the cause, pin `kind.is_some()` or the
exact typed cause.

**Gate status: plan and REDs are CLEAR from all three reviewers.** Source
started on Sam's go (phase 1: `402fd90851f`, `824ebf5889c`).

**Superseded: Read outcome `None` (codex-zs5e83 accepted, phase 1).** The
`outcome: None` note above and R3/R4 at `784ffa540f8` are superseded. Both
fences use the shared `http_preparation_failed`, which marks every unreleased
attempt `NotSent`, Reads included. That is the existing seam for any Read
preparation failure, so changing it would change every venue's projection.
- R3/R4 pin exactly one typed failure, `Some(NotSent)`, and no outbound.
- Causes are engine preparation causes, never auth evidence. A retired
  generation fails `Revoked` (`InvalidInput`), and venue `NotReady` fails
  `State` (`Internal`). R3/R4 pin the health counters unchanged.
- A recipe `Authorization` header is refused only on bound requests. Unbound
  requests keep their own headers.
- A whole-command terminal still comes from `failure_projection`, so prior
  released attempts and receipt/journal aggregation dominate (R8, phase 2).

**Source review, session seam (claude-55ao7t, CLEAR at `86e27534785`).** The
fence flush loops in place until empty. A pin covers a failure callback that
retires a second Session in the same flush. Composition with combo-5b
(#1896/#1897), which lands first:
1. `retire_session` order: `session_scope` → `validate_session` → Bearer
   reservation → `drain_waits` → … → `sessions.retire` → G1 background
   cascade → Bearer fence. The reservation stays ahead of every mutation.
2. #1897's engine tests take the `None` bearer argument at integration.
3. A bound request with explicit `Egress::Session` (#1897's
   `session_scoped_context`) is private, so it is allowed as Primary. Only
   public egress refuses, and a control pins it at integration.
4. R14's refusal half lands once `session_scope` is on this base.

**Source review, Plan owner (claude-fe7lgm, CLEAR at `86e27534785`).**
- A refused binding fails through `fail_owned_admission`, because a selected
  Plan or Preflight call cannot be un-selected. The lower-effect path is the
  venue's: check that the Session is live, or park with `plan_session_wait`,
  before issuing a bound leg.
- Gate for G2b group composition: `claim_http_group` must check every member's
  bearer in its validation pass. A dead or moved member fails the whole group
  at precommit, never through a single-member reprepare.
- Phase 2: drain `bearer_fenced` at the top of each `flush_plan_session_waits`
  pass and at the tail of `flush_http_plans`. That covers delivery exit, the
  Plan timer and `drain_actions`, which do not run `finish_event_scope`.
- P3, phase 2: a pending Bind removed from `io.actions` before the host drains
  it is not zeroized; wipe it on that removal. The fence flush already drains
  in place.

**Token custody scope (codex-zs5e83, accepted).**
- Engine-owned copies, all wiped:
  - the `SecretHeaderValue`, on drop;
  - an overflowing header copy, on refusal;
  - a refused signed packet;
  - every action still in the undrained queue or the transient dispatch
    queue, on drop (unwinding included), via the `Wiping` queue wrapper.
- Host-owned copies: once `drain_actions` hands over the Bind, disposal
  follows the existing signer-header discipline. There is no new host or
  JS zeroization guarantee.

**Phase 2, part 1 (R8, R9, R10 and the outer-path gate).**
- R8 cannot be a same-leg retry of an unanswered write: `Context::retry`
  refuses `ReadFailure` for a Mutation, and a resend needs a `ResendGrant`
  from a `RefusedResend` answer. R8 is pinned as a released, unanswered write
  followed by a fenced sibling write on the same command, which is `Unknown`.
  A control shows the same writes with none released are `NotSent`.
  - A separate guard covers a resend after a definitive refusal, fenced
    before release. Its `Unknown` is `failure_projection`'s existing
    conservative rule, not R8 evidence.
- R9 is a guard: expiry and retire in one event fail the Plan as `Timeout`.
- R10: one Plan with two bound legs, plus a plain Command. Exactly one
  terminal per owner, no HTTP, and callback depth 1.
- Gate: `flush_http_plans` drains `bearer_fenced` at its tail (both feature
  variants), covering start, delivery exit, the Plan timer and
  `drain_actions`. Its test retires inside a Plan-ready callback. When
  `session_scope` (#1896/#1897) lands that retire refuses `State`, so the test
  moves to another outer trigger, such as a retire in `start`.
- Still waiting on dependencies: R10b/R10b' (#2037), R11 and the group gate
  (G2b).
- Composition pin (claude-fe7lgm, CLEAR at `0644cd619e8`): whichever of #2037
  and this lands second makes the tail of `flush_http_plans` read, in order:
  1. `flush_bearer_fences(now)`;
  2. `flush_plan_session_waits(now)`, whose loop also drains Bearer fences at
     the top of each pass;
  3. `io.session_resumed.clear()`.
  Bound failures, which may seal, always come before parked dispositions.

**Engine facts the source must follow:**
- **The real prepare point.** `queue_managed` emits `HttpAction::Prepare` at
  issue, with headers snapshotted from static slices (`http_lifecycle.rs:180`).
  So the contract's "prepare" is `http_prepared`: after `managed.validate`, the
  hook writes the header into the Bind's clone of `managed.original`.
- **Where Primary is decided.** Routing intent comes from
  `context.routing_intent()` at queue time (`http_lifecycle.rs:283`), so a
  bound packet's Primary intent is decided in `queue_managed`.
- **Retire.** `retire_http_owner` removes only Session-owned entries; R3 proves
  that command-owned packets are not fenced today.
- **Header rule.** `validate_unsigned` has no header-name rule, so the
  `Authorization` refusal is new.

## Ownership

claude-m26ps4 owns the engine hook, binding, fence and REDs. codex-zs5e83
adopts it in Paradex, together with deleting `Cx::sign_request` (row 10).
Independent review answers "per-venue fix of a cross-cutting concern?". The
answer here is no: it is a shared managed-lifecycle hook, and any venue whose
private requests carry a session token can use it.

**Phase 2, part 2 (after combo-5b, tip `219d4bc6725`).**
- Integration: the 21 managed-request calls that landed in 5b take the `None`
  bearer argument, and the keyless Bearer probe Sessions take
  `private_egress: None`.
- Gate moved: `session_scope` now refuses a retire inside a Plan-ready
  callback, so the outer-path gate retires in `start` instead. `start` runs no
  `finish_event_scope`, and the fenced Background read fails typed `NotSent`
  before `start` returns; its queued Prepare is followed by a Cancel, and
  routing it binds nothing. Without the `flush_http_plans` tail drain the gate
  fails in both feature variants.
- R14 refusal half: inside a Plan-ready scope, `open_session`,
  `session_context`, `session_scoped_context` and `retire_session` each refuse
  `State`. The Session stays
  live, and a later bound request still prepares Primary with the current
  token.
- R14 refusal half, Preflight (independent-review P3): the Preflight-ready
  callback gets the same refusals, and `session_scoped_context` is probed in
  both scopes. Narrowing `session_scope` to Plan-ready only fails this test.
- `Egress::Session` control: a bound request whose root comes from
  `session_scoped_context` on a private-egress Session binds and prepares
  Primary with the token. Only explicit public egress refuses.
  `Context::egress` is crate-private, so an integration test cannot assert
  the egress directly; `session_scoped_context` refuses `Invalid` for a
  Session without private egress, so the setup forces it.
- Independent review CLEAR at `a24df874c8f` (not a per-venue fix of a
  cross-cutting concern); its Preflight-scope P3 is folded above.- Session seam (claude-55ao7t) CLEAR at `3a0590d6606`. Forward pin for G1
  (codex-zs5e83): whichever of G1's background-parent cascade and this branch
  composes second keeps `retire_session` ending `sessions.retire` → G1
  background cascade → Bearer fence. Both are queued cleanup, so the order is
  pinned rather than load-bearing.- Consumer source/composition (codex-zs5e83) CLEAR at `3a0590d6606`: it ran
  25/25 default and 31/31 `http-plans` itself, and confirmed the start gate
  fails in both variants without the tail drain. Part 2 is fully cleared
  (independent, claude-55ao7t, claude-fe7lgm, codex-zs5e83).

**Stacked on #2037 (overseer, draft PR).** The branch merges #2037's pushed
head `c768f28b6d0` so the draft is mergeable onto it. That merge applies the
pinned composition: the `flush_http_plans` tail runs `flush_bearer_fences`,
then `flush_plan_session_waits`, then `session_resumed.clear()`; each
session-wait pass ends with a Bearer drain, because the expired-wait callback
has no ready custody and may retire a Session; `retire_session` keeps the
reservation ahead of `drain_waits`. Held from READY until #2037 (with N7 and
its cap fix) and G2b land; R10b, R10b' (including the parked-Preflight case)
and R11 follow then.

Restacked on #2037's 6b head `c9691ad9b2a` (N7, its cap fix, mutation
barriers and the core-size maps):
- The signed pre-check (a dead or NotReady Bearer refuses before provider
  work) moved into `resume_prepared_signing`, after any pacing wait, so it runs
  when signing actually begins.
- `take_managed_with_cancel` keeps the per-generation Bearer index clean
  beside the group BindGroup removal.
- Fail-closed until R11: with `http-mutation-barriers`, a Bearer-bound
  request that would join a mutation group is refused `Invalid` at issue,
  because the group claim does not yet check each member's Bearer.
- Independent review of the restack (`926f4bf523f`): CLEAR, not a per-venue
  fix of a cross-cutting concern. Two P3 tests are added at the R11 refresh:
  the group-member refusal (refused before any effect, reservation dropped),
  and a signed bound request that waits on pacing while its Session retires
  or goes NotReady (the pre-check refuses when signing begins).

## Header bundle (claude-ddi2il ruling, 2026-10-25)

GRVT's private writes carry a login-bound header set: `X-Ts-Grvt-Session`,
`Cookie: rm=true;gravity=…` and `X-Grvt-Account-Id`. The hook is generalized
before landing rather than adding a second, parallel one.
- `SessionBearer::Ready { headers: SessionHeaders, revision }`.
- `SessionHeaders` holds 1 to `MAX_SESSION_HEADERS` (4) distinct
  (case-insensitive) venue-declared names with `SecretHeaderValue` values.
  Transport-owned and hop-by-hop names (Host, Content-Length,
  Transfer-Encoding, Connection, Content-Type, Keep-Alive,
  Proxy-Authorization, Proxy-Authenticate, Proxy-Connection, TE, Trailer,
  Upgrade, Expect) and
  invalid names are refused. `SessionHeaders::authorization`
  is the single-header case, so Paradex and BreakoutProp DXAPI are unchanged.
- One revision covers the whole set. All existing fences apply to it:
  - a just-in-time read at prepare;
  - claim-time Revoked/Stale and a reprepare;
  - retry inheritance;
  - the retire cascade;
  - `Wiping` on drop.
- `attach_session_headers` refuses, on a bound request, any recipe or signer
  header whose name collides with a bundle name. It inserts every entry, then
  re-checks the count and byte limits over the whole outbound packet (recipe,
  signer and Session headers together, as the host chains them), and wipes
  all inserted copies on refusal. The issue-time refusal of a recipe
  `Authorization` on bound requests stays; other bundle names are unknown
  until the just-in-time read, so their collisions are refused NotSent at Bind.
- Ordering: the bundle is prepended, ahead of recipe headers (so GRVT's
  `Content-Type` follows it, where native sends it first). This is the general
  policy; a venue's native-wire test pins the order it needs.
- Venue obligation (GRVT audit, codex-zs5e83):
  - A rotation within one engine Session is a new revision; the stale packet
    is reprepared and the old revision is never released.
  - A failed login retires the Session; the replacement is a new generation.
  - While a refresh has cleared the venue's cache, `session_bearer` answers
    NotReady and a bound prepare fails typed. So the venue parks writes
    behind the login Session (`session_wait`/`plan_session_wait`) until it is
    Ready, rather than issuing into NotReady.
  - The venue's revision is strictly monotonic and overflow fails closed.

Controls (`session_bearer.rs`). Mutation checks:
- dropping the recipe-header clause of `attach_session_headers`'s collision
  check fails `a_recipe_header_colliding_with_the_bundle_is_refused`;
- dropping its `signer_headers` clause fails
  `a_signed_header_colliding_with_the_bundle_is_refused` (the Signer site);
- passing no signer headers to the post-insert limit check fails
  `a_signed_bundle_counts_signer_headers_against_the_header_limit` and both
  `http_lifecycle` budget unit tests.
- A GRVT-style bundle attaches together and refreshes together on a revision
  move, unsigned and signed.
- A recipe `Cookie` colliding with the bundle is refused NotSent before Bind,
  with health unchanged.
- A signing step writing `Cookie` on the packet or among the signer headers
  is refused NotSent before the signed Bind.
- A two-header bundle overflowing a crowded recipe is refused NotSent.
- Signed: a packet at exactly 32 headers across packet, signer and Session
  headers binds; one more signer header fails NotSent. Unit tests pin the
  count and byte budgets across recipe and signer headers and that a refusal
  leaves both vectors unchanged.
- `SessionHeaders::new` refuses an empty set, 5 entries, duplicate names,
  every transport and hop-by-hop name and an invalid name, and redacts its `Debug` output.

Results: 32 default and 38 with barriers; engine lib 870 (1075 with features); clippy
`-D warnings` in both modes.

## Phase 3: NotReady park and recipe-controlled order (PLAN, 2026-10-25)

Rulings: claude-ddi2il (engine-wide Bind-path wait, native parity; preserve
native header order through shared assembly). Split agreed with claude-fe7lgm:
this phase owns the unreleased-call park; fe7lgm's serial resend keys its
attempt-2 park on the same revision counter. Reviewers: claude-dazzy3 (plan,
source), claude-fe7lgm and codex-zs5e83 (consumers). No source before the plan
is CLEAR. It changes existing engine behaviour (R4), so the freeze runs one
workspace-wide all-targets check.

### Gap

GRVT's `account_refresh` clears the venue's cookie/account cache while the
engine Session stays Ready, and native calls wait, first calls included. Today:
- a call reaching Bind while the hook answers NotReady fails NotSent (R4);
- a call Prepared or Bound before the refresh sees revision None at claim,
  goes Stale, re-prepares, reads NotReady at Bind and fails NotSent.
A failure arrives too late for any owner (serial Plan, declared legs,
Preflight reads, single admitted requests) to reissue the same call, so the
fix belongs at Bind, once, for every owner kind.

### Contract

1. Park. A managed, unreleased call whose Bind read answers NotReady, at
   either Bind site (unsigned at route readiness, signed after signing), does
   not fail. It parks on its own live Session generation, inside its existing
   call window, and is not handed to the host. Its wake condition is a
   `floor`: the highest revision it must not bind again. Only the claim fence
   raises it (to the bound revision, when it finds the bundle moved or
   cleared), as does an owner's seed. Any other Stale (route expiry, signing
   freshness, clock revision) leaves it, so the call re-binds at the unchanged
   revision. (Review P1, from the independent reviewer, zs5e83 and fe7lgm: a
   draft raised it on every Bind read, which parked ordinary Stale reprepares
   until they timed out.)
2. Claim. A packet whose claim-time revision is None or moved stays on the
   existing Stale path (no release, reprepare); its next Bind read parks it if
   NotReady. Nothing claims or releases a parked call.
3. Wake. The venue announces a bundle with `cx.session_ready(&session)`. That
   call is valid on an already-Ready Session purely to announce a new revision,
   and is a no-op when nothing waits on that generation. It queues a wake that
   the engine drains after the venue callback returns (the `bearer_fenced`
   pattern). In park (FIFO) order, one at a time, the engine re-reads
   `session_bearer_revision` and re-prepares through the existing
   `reprepare_http` the first parked call whose revision is Some(r) above its
   `floor` (any Some when it has none); the rest stay parked. Each re-prepare
   may run callbacks, so membership and the revision are re-checked every
   step, and any re-prepare, however reached, removes the call's park entry.
   Re-prepare spends no attempt,
   needs no ResendGrant, touches no auth ledger, opens no login and resets no
   attempt counter. Order is FIFO among parked calls only (zs5e83 P2). The
   wake drains after the venue callback returns, so a waiter that same
   `session_ready` hands back may issue and Prepare first. The engine gives no
   cross-owner order today (host Prepare completion is unordered) and does not
   claim one here; each owner sequences its own calls (a serial Plan has one
   live call, GRVT's FIFO queue one write at a time). A mixed control pins the
   observed behaviour. No call binds inside the announcing callback: Bind
   needs the host's `http_prepared` event, so a call the venue issues in that
   callback, like a woken one, binds only after its own Prepare completes
   (dazzy3 B1: documented, not held).
   A woken call that reads NotReady again re-parks with its `floor` unchanged
   and needs a newer announce (dazzy3 B4).
   Pacing: the wake re-prepare keeps the call's pacing position exactly as
   the Stale reprepare does, with no fresh slot and no double count (dazzy3 B2).
4. Bind side effects. Re-prepare is today's Stale reprepare. The old signing
   attempt is cancelled and a fresh one is allocated at the next Bind; a nonce
   reservation is released exactly as a Stale reprepare releases it. No new
   reservation semantics. The parked attempt was never claimed, so a re-sign
   reusing or exceeding its nonce is safe either way. P8 pins whichever the
   Stale path gives (dazzy3 B5).
5. Expiry. The call keeps its window timer. Expiry while parked is the
   existing unreleased Timeout, NotSent. Health is unchanged and no auth-kind
   count moves (kind Timeout).
6. Retire. A retire fences parked calls exactly like other Bearer-bound
   unreleased calls (`bearer_fenced`): Revoked, NotSent, kind InvalidInput,
   health unchanged. The park entry is removed with the call.
7. Groups. With `http-mutation-barriers`, a Bearer-bound request that would
   join a mutation group is already refused at issue (until R11), so no group
   member can park. Its test needs a barrier Mutation Plan and moves to the
   R11 refresh after #2037, which owns group-claim custody.
8. Capacity. Parked calls stay in the managed-call table; the park list is
   bounded by it. A park reserves its entry with `try_reserve` and, if it
   cannot, fails the call NotSent (Capacity). An announce records its
   generation with `try_reserve`; without room, parked calls stay parked to
   their window. The wake walks the list in place and never allocates.
9. Typed cause. None is added. NotReady never reaches the venue. Retired and
   header refusals keep their existing non-auth kinds (InvalidInput). No
   `VenueError` field and no accessor.
10. Serial composition (fe7lgm P2). One counter: the call's `floor`. A
    pub(crate) seed lets an engine owner issue a call with `floor` preset. At
    Bind, a Ready read whose revision is <= `floor` parks exactly like
    NotReady, with the same wake rule.
    The serial resend issues attempt 2 at once, seeded with the refused
    attempt's revision r. If the venue has not yet cleared its cache, attempt 2
    parks instead of leaving with the stale bundle, which closes the race. This
    is then the only Session park: the serial resend adds no wait path of its
    own. The seed is not public; venues cannot set it.

Venue obligations (documented, not engine-enforced): the revision moves on
every change to any bundle value AND on every successful refresh, even when
the values are unchanged (dazzy3: a seeded attempt 2 waits for a revision
above the refused one, so an identical re-login must still advance it); `session_bearer` and
`session_bearer_revision` read the same counter; and the venue calls
`session_ready` after every refresh. A refresh that is never announced leaves
parked calls to expire at their window.

### Venue-declared header order (shared assembly)

Native order differs per venue but not per request: Paradex sends
`Authorization` before `Content-Type` at its one Session-bearing site, while
BreakoutProp and GRVT send `Content-Type` first. Today's bundle-first insertion
already changes BreakoutProp's order. Shape (dazzy3 B6, replacing the
per-recipe proposal):
- `SessionPlacement::{First, After(&'static str)}`, declared with the bundle:
  `SessionHeaders::placed(self, SessionPlacement)`, default `First`. Header
  order is a venue-wide wire property and the hook already owns the bundle,
  so one declaration covers signed and unsigned requests. No new request entry
  point or `SignStep` method. Per-request variance waits for a consumer.
- `After(name)` inserts the bundle immediately after the first packet header
  with that name (case-insensitive), or first when the request carries none.
  It is a shared assembly rule inside `attach_session_headers`, not a venue
  insertion. Collision, limit and wipe rules are unchanged.
- Consumers: GRVT `After("Content-Type")`, BreakoutProp `After("Content-Type")`
  (restoring native), Paradex `First`. Each is pinned by a native-wire test in
  its own slice; moving BreakoutProp is a native-restoring wire change that
  needs its slice owner and that RED in the same PR.

### REDs (`session_bearer.rs`, written before source)

- P1 Mutation and P2 Read (zs5e83): bound at revision r, refresh (cache
  cleared) before claim, then `session_ready` at r+1. Each re-prepares and binds
  the r+1 bundle; the attempt count is unchanged and no failure is delivered.
- P3 first call: issued while NotReady, it parks; `session_ready` at r binds it.
- P4 no wake: `session_ready` with the revision still at its `floor` leaves it
  parked, and a later newer revision wakes it.
- P5 FIFO: two parked calls re-prepare in park order.
- P6 expiry: a parked call fails Timeout, NotSent, at its window, with health
  and auth counts unchanged.
- P7 retire while parked: Revoked, NotSent, InvalidInput, health unchanged.
- P8 signed: a call parked after signing re-signs with a fresh signing attempt
  on wake.
- P9 barriers: no parked call carries a group (R11 refusal still holds).
- P10 still-Ready announce: `session_ready` on a Ready Session with no parked
  calls and no waiters is a no-op; one announcing the SAME revision
  re-prepares nothing.
- P11 seeded (fe7lgm): a call seeded with `floor` = r that binds while the hook
  is still Ready at r parks, then wakes and binds at r+1. No attempt is spent
  beyond the seeded call itself. Variant: a refresh with an unchanged cookie
  still announces r+1 and wakes the seeded call.
- P12 mixed order (zs5e83): parked call A; the same `session_ready` returns
  waiter B, whose callback queues B. Both prepare and bind on the new
  revision, and A keeps its attempt. The control records the observed
  Prepare order and asserts no cross-owner guarantee beyond FIFO among
  parked calls.
- R4 is rewritten: NotReady at Bind parks, then times out (the old immediate
  failure is gone). Adopters (dazzy3 B3): no venue implements
  `session_bearer` yet; only `session_bearer.rs` does. The unlanded consumer
  pins (zs5e83's Paradex b29ea97d0d4, BreakoutProp DXAPI, GRVT) move their
  NotReady expectation to park-then-Timeout. Parity: native waits with no
  timeout, so the call window is a new bound, disclosed per venue in CALLERS
  (GRVT's 20 s is ruled).
- P4 also: a woken call that reads NotReady again re-parks with its `floor`
  unchanged and wakes only on a newer announce.
- P1 runs with a paced policy and asserts the wake keeps the pacing position
  (no fresh slot, no double count).
- P8 also pins the nonce a nonce-signed recipe gets on re-sign, matching the
  Stale path.
- P13 announcing callback (dazzy3 B1): the venue issues a new bound call in
  the same callback that calls `session_ready`. Neither binds inside the
  callback; both bind after their own Prepare on the new revision.
- Order: O1 `After("Content-Type")` puts the bundle after Content-Type
  (unsigned and signed); O2 the default stays First; O3 `After` with no anchor
  is First. For a no-body GET (no Content-Type) the GRVT and BreakoutProp
  slices assert native's actual order for that request, not just the engine
  rule (dazzy3 B7); O4 collision and limits still apply after placement.

### Phase 3 source (local, unpushed)

- `Bearer.floor` is raised only by the claim fence (bundle moved or cleared)
  and kept across reprepares. `admit(floor, answer)` binds a Ready answer
  above `floor` (any when None); anything else parks. The pub(crate) seed setter is fe7lgm's: it lands with
  its only consumer (`serial_plan_session_resend`, plan 92f1ff3dcd9), so no
  caller-less API ships here.
- All three NotReady sites park instead of failing: the unsigned Bind, the
  signed pre-signing check (before any provider work) and the signed Bind
  (packet wiped, recipe kept for the re-sign). `park_bearer` re-arms the call
  timer at its deadline and appends to `bearer_parked` in park order.
- `session_ready` records the generation in `bearer_woken` (try_reserve).
  `flush_bearer_fences`, which already runs at every post-callback point, now
  drains fences and wakes until both are empty. A wake walks `bearer_parked`
  in place, one re-prepare at a time, re-reading the revision each step.
  `reprepare_http` removes the park entry on every path.
- The wake loop terminates: each step removes one entry, and nothing inside it
  can re-park. A park happens only while handling the host's Prepared event or
  a signing result, never within `reprepare_http`. A control pins a venue whose
  revision and bundle hooks disagree (an obligation breach): each announce
  re-prepares the call once and it re-parks (dazzy3).
- `attach_session_headers` inserts at `SessionPlacement` (`First`, or right
  after the first matching header, else first). Overflow wipes exactly the
  inserted range.

Review fixes (independent reviewer, zs5e83 and fe7lgm were NOT CLEAR on
a3162839b0b):
- the floor split (P1);
- the in-place wake walk with reserved entries (P2);
- park-entry removal on every reprepare (fe7lgm P2b);
- tests:
  - an ordinary Stale re-binds at the same revision (unsigned and signed);
  - a nested announce does not re-prepare a woken call twice;
  - the signed-Bind park re-signs with a fresh attempt (P8b);
  - an announce at the fenced revision stays parked (P4b);
  - a call that left the park by a host Stale is not woken again;
  - P5 parks in reverse issue order.

Deferred:
- P9 (above).
- P11 is an `admit` unit test until fe7lgm's seed setter lands with
  `serial_plan_session_resend`.
- The default `session_bearer` answers NotReady, so a Session-bound call on a
  venue without the hook now waits out its window instead of failing at once.
  No venue binds a Session yet; CALLERS gets this note at the freeze.

Results: session_bearer 52 default / 58 with barriers; engine lib 871 / 1076;
clippy `-D warnings` clean in default, barrier and lib-only modes.

Mutations:
- the claim fence not raising the floor fails P4b;
- the Bind read raising it (the old bug) fails the ordinary-Stale test;
- admit ignoring the floor fails P11;
- a wake ignoring the revision fails P4;
- a wake that never re-prepares fails eight P tests;
- no removal in reprepare fails the host-Stale test;
- ignoring placement fails O1.

## Phase 4: post-6b refresh (PLAN, 2026-10-25)

#2037 (`da45fa2f614`) and G2b (#1956) landed inside combo-6b. The branch
merged the actual tip `97e91565d78` (`eef3c47a926`). Conflicts: README (both
rows kept) and OKX history (the tip's refactor taken). The only manual non-doc
hunks pass `None` at the 26 managed call sites that landed since Phase 1 added
the `bearer` argument. Bearer 53/59, engine lib 880/1089, and
`cargo check --workspace --all-targets` are clean. Next, the base moves to
`feat/rust-rewrite` (ruling: claude-ddi2il). No push until this phase is
reviewed.

Items, each a test written before any source change:
- **R10b.** A Plan holds one parked session-wait unit and one bound unreleased
  leg on the same Session, and the Session is retired:
  - the shared flush fails the bound packet first, which seals the Plan;
  - the seal retires the parked unit;
  - `on_plan_session_wait_ended` is not called after the seal;
  - the parked pending comes back once through `on_session_wait_expired`
    (NotSent, no custody, no grant).

  Exactly one terminal, zero HTTP, no re-entry.
- **R10b'.** A Preflight parked on Session S, S retired, and the ended hook
  binds S: exactly one terminal. A control re-parks on a live Session.
- **P3 (restack review):**
  - a Bearer-bound group member is refused at issue before any effect, with
    its reservation dropped;
  - a signed bound call waiting on pacing whose Session retires refuses
    Revoked when signing begins;
  - the same call whose bundle goes NotReady now parks (phase 3) instead of
    refusing.
- **P9.** No parked call carries a group: covered by the issue-time refusal
  test above.
- **R11, decision requested.** R11 would let Bearer-bound requests join
  mutation groups by checking every member's revision in the group claim: a
  moved revision on one member fails the whole group precommit, and no single
  member is re-prepared. No consumer needs Bearer-bound groups today:
  - GRVT's writes are a serial Plan (one live call);
  - Paradex and BreakoutProp DXAPI issue no grouped mutations.

  Recommendation: keep the issue-time refusal as the contract, pinned by the
  P3 test, and implement R11 with the first consumer that needs a bound
  group. The alternative is to implement R11 now: the group claim checks each
  member's `Bearer::current` and fails the group precommit on any moved or
  cleared revision. Reviewers rule; nothing changes until they do.

Gates before freeze: the workspace check after the final tip merge, the
CALLERS note for R4 (a venue without the hook now waits out the window), and
one PR run on the frozen head.
