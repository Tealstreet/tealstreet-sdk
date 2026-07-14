# CLAUDE.md — @tealstreet/reconnecting-websocket

Vendored auto-reconnecting WebSocket wrapper. Used as the WS transport for several core internal packages.

- **Used by**: `safe-cex`, `eye-websocket`, `http-over-websocket`, scrapers, data services (~5 internal consumers)
- **Build**: `yarn build-force` (tsup); standard `build` is the no-op tsconfig-paths placeholder
- **Why vendored**: this is critical infrastructure for every WebSocket session in the system; vendored so we can carry stability fixes (e.g. reconnect cleanup) without an upstream release
- **Patches**: see `git log -- thirdparty/reconnecting-websocket` for the diffs we carry
- **Fresh header providers**: a synchronous provider failure emits a generic
  error (never the provider text), clears the connect lock, and retries through
  the normal bounded reconnect delay. Keep that disposal-safe behavior when
  changing connection setup.
- **Explicit `WebSocket` transport**: when callers provide it with headers,
  treat it as the header-capable connection path even if the global runtime
  marker is absent or unreliable (as in renderer-like hosts).
