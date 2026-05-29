# CLAUDE.md — @tealstreet/http-over-websocket

Tunnels HTTP requests over WebSocket connections, enabling HTTP communication through WebSocket proxies. Provides an Axios adapter and latency tracking.

## Architecture

```
src/
├── HOWWebSocket.ts                       # Core WebSocket protocol (reconnecting, auth, message routing)
├── axiosHttpOverWebsocketAdapter.ts      # Axios adapter for transparently tunneling HTTP via WS
├── types.ts                              # HOWConfig, HOWWebSocketAuthState, LatencyEntry
├── config.ts                             # Configuration (getProxyToken, isTokenExpired, etc.)
└── index.ts                              # Main exports
```

**Protocol**: Client connects via WebSocket to a proxy. HTTP requests are serialized, sent through the WS tunnel, and responses are deserialized back. The Axios adapter makes this transparent to calling code.

Before changing tunnel URL or region selection behavior, read
`docs/proxy-routing-flow.html`. Code remains the source of truth; the HTML is a
manually audited snapshot of the current web → safe-cex → HOW flow. If routing
behavior changes, update or delete that snapshot in the same PR.

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
- Forces HTTP/1.1 via `ALPNProtocols` (HTTP/2 incompatible with WebSocket upgrade)
- Latency checking uses 1-second tick with 10-second stale threshold
- `configureHOW()` must be called before any functionality is used
- Browser uses native WebSocket; Node.js/Bun dynamically imports `ws`
- `getTunnelUrlTicket` is called from the reconnecting-websocket async URL
  provider, so each connection attempt/reconnect gets a fresh `tt` URL ticket.
  Do not move this back to `connectAndSubscribe()`; that would reuse an expired
  ticket on long-lived reconnecting HOW sessions.
- Short-lived Node tools that touch HOW should call `disposeHOWWebSockets()`
  before exit. The SG/JA HOW clients are module-singletons, so exchange-level
  disposal alone does not close their sockets/timers.
- **Inspecting tunneled requests/responses.** REST appears as WS
  frames, not XHRs. Low-level fallback:
  `NEXT_PUBLIC_LOG_TRADETUNNEL_TO_CONSOLE_LOG=1` (gated in
  `axiosHttpOverWebsocketAdapter.ts:12-13`) — requires dev server
  restart since `NEXT_PUBLIC_*` is inlined at build time. Consumer
  apps typically expose a runtime capture; check their CLAUDE.md
  first.
- **Tunnel responses must carry the caller's request `config`.** The adapter
  resolves with `{ ...data, config }` (and attaches `config` to thrown
  `TradeTunnelError`s) so the caller's custom request-config keys and
  headers survive the round-trip. Without this, exchange response/error
  interceptors that read `response.config.__suppressExchangeError`,
  `error.config.__suppressExchangeError`, custom headers, etc. see
  `undefined` and per-call opt-outs silently no-op. The tunnel frame has
  only `{data, status, headers}`; attaching the original `config` is what
  every built-in axios adapter does.
