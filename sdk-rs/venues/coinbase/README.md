# Coinbase derivatives foundation (S0)

`drb::Transport` is the shared HTTP/main-WebSocket JSON-RPC transport owned by
`Coinbase::drb`. `drb_auth::Session` holds auth state for exactly one `Channel`
(HTTP or one socket ID). The existing venue flows are migrated by the separate
public, account and trading streams; S0 does not switch legacy routes itself.

Integration contract:

- HTTP returns `(RpcId, engine::RequestId)`; retain the RPC ID in the caller's
  pending context. Forward the response status/body to `response` and transport
  loss to `failed`. WebSocket sends return an RPC ID; route responses by that ID.
- Subscription notifications are consumed by stream modules, not `response`.
- Call `expire` on a timer and `disconnected` when a socket closes. These return
  uncertain completions and never queue resends. Correlate each ID to its caller.
- Authenticate with fresh `Session::params`, send `public/auth`, then pass its
  decoded result and request-start time to `Session::accept`. Schedule refresh
  using `refresh_at`, gate private sends with `needs_refresh`, and invalidate the
  socket session on close. HTTP uses `authorization`; WS authenticates in-band.
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

The first commit publishes compiling interface stubs. Fixture implementation
follows on the same branch. No private live calls or orders are used.
