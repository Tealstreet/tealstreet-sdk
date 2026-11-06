### Managed socket route exhaustion

A managed feed whose `reject_route` connect finds no route left now retires
with the new `RetireReason::RouteExhausted` instead of `Closed`. The host dialled
nothing, so it adds no `ws.closed` count and schedules
no automatic reconnect: the feed stays down, as the legacy
`on_route_exhausted` path leaves Bitget's private socket. Real closes keep the
`Closed` path unchanged.

Callers affected: SDK venues only. A venue that matches `RetireReason`
exhaustively must add the variant (Binance and Bybit do, with a notice arm that
cannot fire, since neither requests `reject_route`). A venue moving a
`reject_route` socket onto managed feeds must handle
`LifecycleEvent::Retired { reason: RouteExhausted }` where its legacy path handled
`on_route_exhausted`. No NX or server obligation, and no deletion prerequisite.
