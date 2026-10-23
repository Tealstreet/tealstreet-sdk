# Bitget UTA private-WS routing parity (TS #8093)

Plan, 2026-10-23, claude-4yjlw0. Stacked on master-merge #1869 (`cc0d25e820a`).
That PR brings TS #8093's routing table onto landing: the `privateUta` WS group,
proxyConditional `/bitget/v2/private` and `/bitget/v3/private`,
`relayUpstream`/`relayWhenTunnelled`, and the TS 40018/40024 egress-refusal walk.
No #1817 dependency.

## Gap

The Rust engine opens both Bitget classic (v2) and UTA (v3) private sockets on
the `private` channel. The host routes by channel, so:

- A UTA socket keeps landing's `directUrls` bypass. It connects direct even when
  TS routes UTA through `/bitget/v3/private` or its relay.
- A login refused for the egress IP (40018 IP-bound key, 40024 restricted
  location) is reported as auth (`AuthPermission`) with no route walk. TS tries
  the next egress first, and reports only after every route is refused.

## Design (shared choke points, not a Bitget-only router)

Revised after codex-5c57jl's socket-API review and claude-ddi2il's disposition,
which rejected a fixed refusal count.

1. **Engine `SocketContext.reject_route: bool`**, beside `recover` and identical
   under both #1771 options.
   - It is a one-connect edge: the venue sets it only from the current socket's
     own epoch, and only for pre-login 40018/40024 reply evidence.
   - Stale or duplicate evidence never sets it.
   - Absent or false keeps today's behaviour and wire. One spelling,
     `reject_route` / `rejectRoute`, is declared for TS, WASM and native, and
     all literals and fixtures are updated.
2. **Hosts own the chain.** On a `reject_route` connect, TS calls
   `WsDirectFallback.rejectCurrent()` and native advances `attempts.candidate`.
   - The host advances at most once, clamps to the actual chain, and resets
     silence without a second silent advance in the same connect.
   - `reject_route` takes precedence over `recover`; `recover` alone keeps
     today's jump-to-last.
3. **Typed exhaustion feedback.** When a `reject_route` connect has no remaining
   candidate, the host does not dial. It emits `Event::WsRouteExhausted { socket }`.
   - Only a host can produce this event: it is not a close and carries no peer
     close code, so it cannot collide with a peer close.
   - It is enqueued after `connect` returns, as an ordinary ingress event. It is
     never re-entrant, and the engine drops it for a stale socket.
   - The engine calls `Venue::on_route_exhausted`. The default maps to today's
     close handling, so venues that don't opt in are unchanged.
4. **URL-scoped private routes.** Private route rules are data-driven, mirroring
   the public `productRoutes` / `public_ws_routes`.
   - Bitget declares `/v3/ws/private` -> `wsPrivateUta`.
   - The UTA `directUrls` bypass and #1869's tunnelled `directUrls` carry are
     removed.
   - The serialized native plan gains `private_ws_routes`, and its fixture is
     refreshed.
5. **Bitget venue, exact TS #8093 behaviour.**
   - **Refusal:** pre-login 40018/40024 reconnects with `reject_route` after
     the supervisor's first backoff step, with TS's "trying the next route"
     warning and no error report. Backoff (not an immediate dial) bounds a host
     that ignores `reject_route` and never reports exhaustion.
   - **Exhaustion:** `on_route_exhausted` for the current private socket reports
     the native egress refusal once per streak as an `AUTH_REJECTED` error
     notice, never a startup failure or deactivation (REST startup has its own
     egress). As in TS, the socket then keeps retrying the last route, paced by
     backoff, so a later whitelist fix recovers; a login ends the streak.
   - **Non-terminal until exhaustion:** 40018/40024 are not a permanent auth
     failure until then.
   - **Separate counters:** carried over from TS. These are
     `MAX_UNAUTHENTICATED_TUNNEL_CLOSES = 2` (a tunnel route that opens and
     closes before any login answer) and `MAX_FAILED_TUNNEL_CONNECTS = 6`
     (a tunnel route that never opens). Each sends `reject_route` once its
     threshold is met.
   - **Resets:** a login answered on a route resets these counters, and an
     authenticated login resets everything.

## Tests (original REDs first)

- **TS host.** Proxied UTA uses `/bitget/v3/private`. Direct UTA walks to the v3
  relay and never touches v2 (REDs committed).
  - `reject_route` advances one candidate. On an exhausted chain it emits typed
    exhaustion without dialling.
  - `reject_route` wins over `recover`. A peer close with a 4xxx code is not
    exhaustion. Exhaustion arrives after `connect` returns.
- **Native host.** The same cases against the serialized plan.
- **Bitget venue.**
  - 40018 or 40024 before login reconnects with `reject_route` and reports
    nothing.
  - Exhaustion reports once per streak, then redials the last route with
    backoff; a later login succeeds with no second report. That holds for a
    two-candidate chain (relay off) and a three-candidate chain.
  - One relay strike does not exhaust the relay.
  - The 2 and 6 counter thresholds hold.
  - Stale or duplicate exhaustion and refusals from an old epoch are inert.
  - Other login codes are unchanged.
- **Fixtures.** The routing snapshot and native-matrix fixture are refreshed;
  only Bitget rows change.

## Ownership and gates

Socket core owner codex-5c57jl (#1771) cleared the `reject_route` name and
shape (no #1771 edits). Its approval of the shared patch needs a reviewed pin
covering provenance and ordering. The fixed-count variant was rejected by the
overseer.
Architecture gate: the route walk lives in the shared host/engine choke points.
The venue supplies only native refusal evidence. CALLERS: a custom host must
honour `reject_route` or Bitget UTA/egress parity is lost; no SDK option changes.
