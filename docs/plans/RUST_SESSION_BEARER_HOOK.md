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

## Ownership

claude-m26ps4 owns the engine hook, binding, fence and REDs. codex-zs5e83
adopts it in Paradex, together with deleting `Cx::sign_request` (row 10).
Independent review answers "per-venue fix of a cross-cutting concern?". The
answer here is no: it is a shared managed-lifecycle hook, and any venue whose
private requests carry a session token can use it.
