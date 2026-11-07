# Coinbase derivatives foundation (S0)

`drb::Transport` is the shared HTTP/main-WebSocket JSON-RPC transport owned by
`Coinbase::drb`. `drb_auth::Session` holds auth state for exactly one `Channel`
(HTTP or one socket ID). The existing venue flows are migrated by the separate
public, account and trading streams; S0 does not switch legacy routes itself.

Integration contract:

- HTTP takes the caller's engine-issued `request_context::Context` and an optional
  engine `request_context::Session` handle. Private methods require that session;
  its `Venue::session_bearer` callback supplies the current token at preparation,
  and `session_bearer_revision` fences the same revision at release.
  Retain the context with the pending request, validate its delivery, record any
  Mutation result, and finish the context through the engine lifecycle. Production
  writes use genuine command/Plan authority; public reads use Background contexts.
  No raw bearer strings or legacy `Cx::request` boundary are used by drb.
- HTTP returns `(RpcId, engine::RequestId)`; retain the RPC ID in the caller's
  pending context. Forward status/body and `cx.now` to `response_at` and transport
  loss to `failed`. WebSocket sends return an RPC ID; route responses by that ID.
- Subscription notifications are consumed by stream modules, not `response`.
- Call `expire` on a timer and `disconnected` when a socket closes. These return
  uncertain completions and never queue resends. Correlate each ID to its caller.
- Authenticate with fresh `Session::params`, send `public/auth`, then pass its
  decoded result and request-start time to `Session::accept`. Schedule refresh
  using `refresh_at`, gate private sends with `needs_refresh`, and invalidate the
  socket session on close and invalidate rejected credentials. Invalidation keeps
  the authentication-start watermark, so older completions cannot revive the cache.
  The HTTP session-bearer callback can use `authorization`; WS authenticates
  in-band. Refresh always exchanges a fresh CDP JWT using `coinbase_cdp`; no refresh-token grant is assumed.
- Share one transport across streams so HTTP and WS consume the same credit
  pools. Local admission errors mean the request was not sent. A caller may
  schedule new admission later; released writes must never be resent implicitly.
- Use `to_instrument` / `from_instrument` only for legacy INTX perpetual aliases.
  Exact tradable names still come from `public/get_instruments`.

Protocol sources:

- https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/guides/derivatives/technical
- https://docs.cdp.coinbase.com/api-reference/coinbase-deribit-app-api/adv-starbase-openapi.json
- https://docs.deribit.com/articles/rate-limits
- https://docs.deribit.com/articles/errors

Rate admission starts at 5 matching requests/second with a burst of 20, and
20 general queries/second with a burst of 100. Instrument discovery,
subscriptions, transaction logs and position moves have dedicated credit pools;
position moves also enforce the rolling weekly cap. Supply the effective minimum
account/currency/trading/cancel limits to `set_matching_limits` when known.
Mass quotes have separate quotas and are rejected locally in S0. Server-side
quotas include other clients and remain authoritative.

`response_at` checks the 30-second deadline and starts rate-limit cooldown at
receipt time. The original `response` signature remains available for interface
consumers without a clock; its cooldown conservatively starts at the request
deadline. It requires the caller to expire pending requests on time. Correlation
is bounded to 256 pending IDs. A missing/mismatched ID, duplicate envelope field,
unlisted error, malformed reply, timeout or transport loss yields `Unknown`.
Native refusal codes never grant a resend. Error text/data are available in
`Failure::rpc_error`; Debug output omits those untrusted fields.

Auth refresh uses the earlier of `expires_in` and the transport policy (15 minutes
for HTTP, 49 minutes for WS), with a 10% margin capped at 30 seconds. Clock
regression and non-finite time fail closed. Cache tokens are erased on drop or
invalidation. Gateway JWTs use `iss: cdp`, a fresh nonce, and a 120-second lifetime
for both existing CDP signer algorithms. Existing spot JWT claims are preserved.

The interface was published first in `af5df7922`; implementation follows on the
same branch. All protocol and signer tests use offline fixtures and synthetic
keys. No private live calls or orders are used.

Validation: `cargo test --locked -j 2 -p safe-cex-venue-coinbase` from `sdk-rs`,
plus scoped Clippy and a `wasm32-unknown-unknown` check. The foundation suite was
run RED against the interface stubs before implementation; additional RED tests
caught legacy ES256 issuer reuse and delayed-response cooldown timing.
