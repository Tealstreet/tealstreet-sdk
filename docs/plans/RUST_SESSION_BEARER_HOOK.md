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

**Hook.** A read-only `Venue` method with a default of `NotReady`:

```rust
fn session_bearer(&self, generation: u64) -> SessionBearer;
enum SessionBearer {
    Ready { value: SecretHeaderValue, revision: u64 },
    NotReady,
}
```

- It is `&self`, with no I/O, no `Cx` and no state change. It returns the
  venue's current token, rendered as the header value, and a revision that the
  venue bumps on every token change.
- The engine calls it only at managed prepare, reprepare and retry or resend
  preparation of a bound request. It never calls it at issue and never queues
  the result.
- The engine sets exactly one header, `Authorization`, and a recipe header of
  that name is refused. `SecretHeaderValue` is redacted in `Debug` and never
  appears in notices or diagnostics.

**Where it plugs in** (managed lifecycle only; the recipe interface is
unchanged):
1. **Prepare.** After `managed.validate` and before staging the recipe, check
   that the bound generation is live, then call the hook.
   - `NotReady` refuses the attempt as typed `NotSent`, before any provider
     work or HTTP. The venue's own auth path decides when to try again.
   - `Ready` sets the header on the outbound action. The prepared packet records
     `(generation, revision)`.
2. **Claim and release.** Re-check the generation, then call the hook to get
   the current revision.
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

## REDs (engine; compile REDs where the API is new)

- **R1 (just-in-time read).** A bound request issued while the venue holds
  token A, then prepared after the venue moves to B, sends B. The hook is never
  called at issue.
- **R2 (same-generation refresh).** The revision moves between prepare and
  claim, so the packet reprepares, the released header is the new token, and
  only one HTTP is sent.
- **R3 (retire fence).** The Session is retired after prepare and before
  release:
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
- **R7 (no authority).** Binding to a retired Session refuses `Revoked`, and a
  bound request without a live Session never sends.

## Open questions for the consumer and owners

- Binding API: `bind_session_bearer(request, &Session)` after issue, or a
  parameter on `request_managed`. I prefer the parameter, so a request can
  never be issued unbound and then bound.
- Header name: Paradex needs only `Authorization: Bearer`. Should the hook
  return the full value (`Bearer <jwt>`), or should the engine prefix `Bearer `?
  The plan says the full value, so the engine stays scheme-agnostic.
- WS auth (`send_auth` with `bearer`) stays venue-owned socket work for now;
  this plan covers REST only.

## Ownership

claude-m26ps4 owns the engine hook, binding, fence and REDs. codex-zs5e83
adopts it in Paradex, together with deleting `Cx::sign_request` (row 10).
Independent review answers "per-venue fix of a cross-cutting concern?". The
answer here is no: it is a shared managed-lifecycle hook, and any venue whose
private requests carry a session token can use it.
