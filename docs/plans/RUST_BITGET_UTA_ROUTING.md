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

1. **Engine `SocketContext.reject_route`.** A generic per-connect hint beside
   `recover`. It means: the venue has evidence that the previous route's egress
   was refused (a reply, not silence), so advance one candidate. No venue picks
   URLs; route policy stays in the host.
2. **Hosts honour it.** TS `transport/engine.ts` calls
   `WsDirectFallback.rejectCurrent()` before `resolveUrl`. The native
   `hosts/native/src/routing.rs` advances `attempts.candidate` by one. Both reset
   the silent counter, matching `rejectCurrent`.
3. **URL-scoped private routes.** Private sockets get the same data-driven
   URL-rule list public sockets already use (`productRoutes` /
   `public_ws_routes`). Bitget declares `/v3/ws/private` -> `wsPrivateUta`. The
   `private` route's UTA `directUrls` bypass, and #1869's tunnelled `directUrls`
   carry in `resolveExchangeRouting`, are then removed. The serialized native
   routing plan gains `private_ws_routes`; the generated fixture is refreshed.
4. **Bitget venue.** On a pre-authentication `event:error` with code 40018 or
   40024, the venue does not report. It reconnects the private socket with
   `reject_route`, keeps the existing supervisor backoff, and reports the
   existing typed error once only after `ROUTE_CANDIDATES` (3: host, relay,
   fallback) consecutive egress refusals in one unauthenticated streak. An
   authenticated login resets the streak. Other login errors keep today's
   classification. Known delta: a two-candidate route (tunnelled host plus relay)
   reports one attempt later than TS, because the host does not report
   exhaustion back. That is recorded, not hidden.
5. **Pre-login tunnel closes.** TS also advances after repeated closes before
   login on tunnel routes. The host's existing silent-connect walk already
   advances on connects with no message. Closes after open but before login are
   counted by the venue and signalled with the same `reject_route` hint, using
   TS's thresholds.

## Tests (original REDs first)

- TS host: a Rust private socket to `wss://ws.bitget.com/v3/ws/private` under
  `tealstreetProxy` resolves to `/bitget/v3/private` (today: direct). A
  `reject_route` connect advances host to relay to fallback.
- Native host: the same two cases against the serialized plan.
- Bitget venue: 40018 before login reconnects with `reject_route` and reports
  nothing. Three consecutive refusals report once. Success resets the streak.
  40024 is handled the same way. Other codes are unchanged.
- The routing snapshot and the native-matrix fixture are refreshed. Only Bitget
  rows change.

## Ownership and gates

The socket lifecycle core belongs to codex-5c57jl (#1771), so the
`SocketContext` hint and host walk are agreed with it before edits.
Architecture gate: the route walk lives in the shared host/engine choke points.
The venue supplies only native refusal evidence. CALLERS: a custom host must
honour `reject_route` or Bitget UTA/egress parity is lost; no SDK option changes.
