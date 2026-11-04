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
- **URL providers**: a rejected provider emits a generic error and retries
  through the normal bounded reconnect delay, never ending the loop. Each
  connect is an attempt; a provider settling for a superseded attempt (a
  `reconnect()` while it hung) is ignored, so it can neither open a second
  socket nor unlock the current one.
- **Explicit `WebSocket` transport**: when callers provide it with headers,
  treat it as the header-capable connection path even if the global runtime
  marker is absent or unreliable (as in renderer-like hosts).
- **`reconnectsAtOnce`**: a close the predicate answers true for resets the
  retry count before reconnecting, so that one reconnect has no backoff wait;
  later failures back off as usual. The exchange-server worker uses it for a
  draining box's 4100.

- **Supervisor handshake recovery**: `timeoutConnecting()` retires only a
  connecting attempt through the normal timeout path, preserving retry count and
  backoff. It invalidates pending URL work; close notification still runs when
  there is no native socket or native close throws. The caller owns its age bound.
