# CLAUDE.md — @tealstreet/http-over-websocket

Tunnels HTTP requests over WebSocket connections, enabling HTTP communication through WebSocket proxies. Provides an Axios adapter and latency tracking.

## Architecture

```
src/
├── HOWWebSocket.ts                       # Core WebSocket protocol (reconnecting, auth, message routing)
├── axiosHttpOverWebsocketAdapter.ts      # Axios adapter for transparently tunneling HTTP via WS
├── egressRegion.ts                       # Single source: Tokyo-vs-Singapore egress list + isTokyoEgress()
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
  routing table** (`packages/safe-cex/src/routing/routingTable.ts`) imports
  `isTokyoEgress` from here to compute each exchange's declared `howEgress`. To change
  where an exchange egresses, edit this list in this one place. Keep it pure — pulling
  it back into the adapter would drag websocket construction / `configureHOW` into
  every pure consumer (safe-cex's routing table) and break their test mocks.
  `routing.equivalence.test.ts` locks name-based and baseURL-based agreement per exchange.
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
- Axios `config.timeout` is one deadline for the complete tunnel request, not
  only the post-send response wait. Token retrieval, socket/time sync,
  authentication, simulation, and response handling all consume the same
  budget. When it expires, the adapter rejects with
  `TradeTunnelTimeoutError` and must not transmit the now-stale request.
- Browser uses native WebSocket and sends its own UA; Node.js/Bun must use a
  constructor path that accepts headers.
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
