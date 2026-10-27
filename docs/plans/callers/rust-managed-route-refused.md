### Managed socket route refusal

A managed venue reports a single route's pre-login egress refusal (Bitget's
40018/40024) as `Control::Auth(AuthEvidence::RouteRefused(error))`. The driver
retires that epoch with `RetireReason::RouteRefused` and retries. The engine
then sets `reject_route` on the venue's next `connect_socket` spec, so the host
leaves the route; the spec must carry a feed context for the host to act on it.

The first refusal of a streak redials at once, matching TS's immediate route
switch. Later refusals in the same streak wait ordinary backoff, so a host that
ignores `rejectRoute` cannot loop faster than backoff. An accepted login ends
the streak. A refusal on the last route ends in `RouteExhausted` (see
`rust-managed-route-refusal.md`), and that feed stays down.

Callers affected: SDK venues only. A venue that matches `RetireReason`
exhaustively must add `RouteRefused` (Binance and Bybit do, with a notice arm
that cannot fire, since neither reports route refusals). This evidence raises
no `AuthFailure` lifecycle event; the venue sees `Retired { reason: RouteRefused }`.
Hosts are unchanged: they already honour `rejectRoute` and emit
`WsRouteExhausted` on both paths. No NX or server obligation, and no deletion
prerequisite.
