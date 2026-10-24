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
  `session_context` and `retire_session` each refuse `State`. The Session stays
  live, and a later bound request still prepares Primary with the current
  token.
- `Egress::Session` control: a bound request whose root comes from
  `session_scoped_context` on a private-egress Session binds and prepares
  Primary with the token. Only explicit public egress refuses.
