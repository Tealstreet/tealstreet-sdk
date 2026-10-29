# CLAUDE.md — @tealstreet/http-over-websocket

Tunnels HTTP requests over WebSocket connections, enabling HTTP communication through WebSocket proxies. Provides an Axios adapter and latency tracking.

## Architecture

```
src/
├── HOWWebSocket.ts                       # Core WebSocket protocol (reconnecting, auth, message routing)
├── axiosHttpOverWebsocketAdapter.ts      # Axios adapter for transparently tunneling HTTP via WS
├── egressRegion.ts                       # Single source: Tokyo-vs-Singapore egress list + isTokyoEgress()
├── tunnelEndpoints.ts                    # PURE: tunnel wss endpoints per region + rpc.relay url helpers
├── RelayWebSocket.ts                     # WebSocket-shaped client for the tunnel's rpc.relay (private exchange WS via the fleet)
├── tradeTunnelBinaryFrame.ts             # Validates and decodes TSB1 binary gzip envelopes
├── tradeTunnelBinaryFrame.test.ts        # Binary-envelope compatibility and validation regressions
├── types.ts                              # HOWConfig, HOWWebSocketAuthState, LatencyEntry
├── config.ts                             # Configuration (getProxyToken, isTokenExpired, etc.)
└── index.ts                              # Main exports
```

**Protocol**: Client connects via WebSocket to a proxy. HTTP requests are serialized, sent through the WS tunnel, and responses are deserialized back. The Axios adapter makes this transparent to calling code.

Before changing tunnel URL or region selection behavior, read
`docs/proxy-routing-flow.html`. Code remains the source of truth; the HTML is a
manually audited snapshot of the current web → safe-cex → HOW flow. If routing
behavior changes, update or delete that snapshot in the same PR.

- **Egress region is single-sourced in `egressRegion.ts`** — a pure, side-effect-free
  module (no `HOWWebSocket` construction, no `configureHOW`) exporting
  `TOKYO_EGRESS_EXCHANGES`, `TOKYO_EGRESS_PATTERN`, and `isTokyoEgress(key)`. The
  adapter derives both its request-time egress (`isTokyoEgress(config.baseURL)`) and
  `getHOWWebSocketForExchange` (`isTokyoEgress(name)`) from it, and **safe-cex's
  routing table** (`packages/safe-cex/src/transport/tealstreet/routing/routingTable.ts`) imports
  `isTokyoEgress` from here to compute each exchange's declared `howEgress`. To change
  where an exchange egresses, edit this list in this one place. Keep it pure — pulling
  it back into the adapter would drag websocket construction / `configureHOW` into
  every pure consumer (safe-cex's routing table) and break their test mocks.
  `routing.equivalence.test.ts` locks name-based and baseURL-based agreement per exchange.
- **`RelayWebSocket` is the client half of the tunnel's `rpc.relay`** (server:
  `infra/tradetunnel/src/relay.rs`; plan: `docs/plans/TRADETUNNEL_WS_RELAY_v1.md`).
  It dials the regional tunnel, sends `rpc.auth` + `rpc.relay <upstream>`, and only
  then reports `open`; sends before that are queued, tunnel chatter is swallowed,
  and every pre-open failure is `error`+`close` with NO `message` — safe-cex's
  `WsDirectFallback` reads that as a silent connect and moves on. It is constructed
  by the vendored `ReconnectingWebSocket` via its `WebSocket` option from a
  `wss://<tunnel>/tunnel?ts-relay=<upstream>` url (`buildRelayUrl`), so exchange
  classes see just another host. The url helpers live in **`tunnelEndpoints.ts`,
  which must stay pure** for the same reason as `egressRegion.ts`: safe-cex's
  routing resolver imports `buildRelayUrl` from it. `RelayWebSocket.ts` itself
  `require`s `ws` at load in Node — never import it from a pure consumer.
- **One tunnel host for every region since AWS_EXIT step 3e**:
  `TOKYO_HOW_EGRESS` and `SINGAPORE_HOW_EGRESS` are both
  `wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel`. `setTunnelEgressUrls` remains
  for tests (the hosted exchange server stopped pinning its own in 2026-10), so
  every consumer reads `tunnelEgressUrl(egress)` at dial time, and the
  module-level `howWebSocketSG`/`howWebSocketJA` singletons take a URL getter
  for that reason. Do not reintroduce a constant read at construction.
- `ReconnectingWebSocket` handles connection resilience
- Token-based authentication with expiry checking
- Latency tracking for tunnel (client↔proxy) and exchange (proxy↔exchange)
- Regional routing support (JA/SG regions)
- Optional simulation mode for testing without tunnel

## Configuration

Must call `configureHOW()` before use:

```typescript
configureHOW({
  getProxyToken: () => Promise<string>,
  bindProxyToken?: () => () => Promise<string>, // per-context hosts; bound at request/socket start
  isTokenExpired: (token: string) => boolean,
  simulateHttpOverWebsocket?: boolean,
  syncServerTime?: (serverTime: number) => void,
});
```

## Commands

```bash
yarn build-force    # Build with tsup (ESM only)
yarn typecheck      # Type checking
yarn test           # Vitest
```

## Gotchas

- **Build disabled by default** — uses tsconfig path aliases in development. Use `build-force` for actual build.
- Non-browser tradetunnel callers must use this package's `HOWWebSocket`, not a
  direct `new WebSocket(...)`. In Node/Bun, `HOWWebSocket.ts` loads a header-capable WebSocket constructor, sends `Origin: https://preview.tealstreet.io` plus a browser-like
  `User-Agent`, and passes that constructor into vendored
  `ReconnectingWebSocket`.
- Vendored `ReconnectingWebSocket` forces HTTP/1.1 via `ALPNProtocols:
['http/1.1']` for Node/Bun constructors (HTTP/2 is incompatible with WS
  upgrade). Keep the UA/Origin headers and ALPN behavior aligned; they are what
  let CLI, `yarn keys`, and exchange-server use HoW outside a browser.
- Latency checking uses 1-second tick with 10-second stale threshold
- `configureHOW()` must be called before any functionality is used
- The Axios adapter's connection/authentication pre-send phase is a real async
  function. Keep the nested `new Promise` limited to the response event bridge;
  async Promise executors swallow thrown authentication failures and leave the
  caller pending.
- The adapter's `prepare(config)` awaits connection/time-sync/authentication
  without sending `rpc.http`. Its one-use `bind(finalConfig)` validates and
  serializes the final packet; `ready()` checks the original deadline and socket
  authentication epoch. `dispatch()` calls raw WebSocket.send synchronously.
  Managed requests accept exact string bodies, reject Axios parameter/body
  transformations, and negotiate raw responses even on the default adapter.
  `prepareLegacy` and the Axios function retain the existing parser/simulation
  behavior and provide no engine release guarantee. An epoch change invalidates
  a managed preparation before dispatch; cancellation never closes a shared peer.
- Axios `config.timeout` is one deadline for the complete tunnel request, not
  only the post-send response wait. Token retrieval, socket/time sync,
  authentication, simulation, and response handling all consume the same
  budget. `config.signal` cancels only its request, including token waits,
  without closing a shared connection or cancelling another request's refresh.
  When the deadline expires, the adapter rejects with
  `TradeTunnelTimeoutError` and must not transmit the now-stale request.
- A `Bad` authentication state is connection-scoped. Concurrent Axios requests
  share one refresh/token-send attempt per `HOWWebSocket` through the adapter's
  weak single-flight map, so one bad-token episode emits one `rpc.auth` frame.
  The single-flight is cleared on both success and failure. Each caller still
  waits under its own complete-request deadline; if it joined an attempt owned
  by a shorter-deadline request, it may elect a new attempt rather than inherit
  the other request's `TradeTunnelTimeoutError` context. Non-timeout refresh
  failures remain shared and propagate to every joined caller. A disposed
  adapter's typed `TradeTunnelDisposedError` is also owner-scoped: surviving
  adapters sharing the default HOW connection elect another refresh instead
  of inheriting that disposal.
- TradeTunnel can auto-authenticate backend-whitelisted client IPs by sending an
  unsolicited successful `rpc.auth` frame after connect. The Axios adapter must
  honor an already-`Good` socket before fetching a JWT; otherwise trusted
  headless tools block on token acquisition even though the server already
  admitted them. Non-whitelisted connections retain the normal JWT flow.
- Browser uses native WebSocket and sends its own UA; Node.js/Bun must use a
  constructor path that accepts headers. Workers pass their fetch-upgrade
  constructor through `CreateAxiosHttpOverWebsocketAdapterOptions.WebSocket`
  and the fourth `RelayWebSocket` constructor argument, preserving required
  WAF upgrade headers and raw binary frames. An injected HOW adapter owns its
  regional clients; its `dispose()` cancels pending requests and closes those
  clients without touching the default runtime singletons.
- Engine Node hosts inject the header-capable constructor from `nativeTransport`
  into HOW and relay, including in pure ESM. It resolves `ws` beside the host's
  declared dependency through `createRequire`; do not rely on bundled dynamic
  `require` shims to load a header-capable socket.
- Short-lived Node tools that touch HOW should call `disposeHOWWebSockets()`
  before exit. The SG/JA HOW clients are module-singletons, so exchange-level
  disposal alone does not close their sockets/timers.
- **Inspecting tunneled requests/responses.** REST appears as WS
  frames, not XHRs. The adapter can emit sanitized `[TradeTunnel]`
  request/response summaries: Node/Bun callers can use `DEBUG=1` /
  `DEBUG=true`, while browser builds still have the low-level
  `NEXT_PUBLIC_LOG_TRADETUNNEL_TO_CONSOLE_LOG=1` fallback (requires dev
  server restart because `NEXT_PUBLIC_*` is inlined at build time). The
  summaries omit headers, redact sensitive keys, keep URL query-key names
  without query values, and truncate large payloads. Consumer apps typically
  expose a runtime capture; check their CLAUDE.md first.
- **Tunnel responses must carry the caller's request `config`.** The adapter
  resolves with `{ ...data, config }` (and attaches `config` to thrown
  `TradeTunnelError`s) so the caller's custom request-config keys and
  headers survive the round-trip. Without this, exchange response/error
  interceptors that read `response.config.__suppressExchangeError`,
  `error.config.__suppressExchangeError`, custom headers, etc. see
  `undefined` and per-call opt-outs silently no-op. The tunnel frame has
  only `{data, status, headers}`; attaching the original `config` is what
  every built-in axios adapter does.
- Custom-parser requests advertise `rpc_version: 4`, `format: "raw"`, gzip, and
  binary-frame support. Compatible servers return a validated `TSB1` binary
  envelope with raw gzip bytes. Version-3 servers return `data_encoded` /
  `data_encoding: "gzip+base64"`; version-2 servers may return only `data_raw`,
  while legacy servers may return both fields. Browser `ArrayBuffer`/`Blob` and
  Node/Bun buffer views are supported, and every older text response remains a
  valid fallback.
