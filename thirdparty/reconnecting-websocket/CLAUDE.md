# CLAUDE.md — @tealstreet/reconnecting-websocket

Vendored auto-reconnecting WebSocket wrapper. Used as the WS transport for several core internal packages.

- **Used by**: `safe-cex`, `eye-websocket`, `http-over-websocket`, `worker-cli-relay`, scrapers, data services (~6 internal consumers)
- **Build**: `yarn build-force` (tsup); standard `build` is the no-op tsconfig-paths placeholder
- **Why vendored**: this is critical infrastructure for every WebSocket session in the system; vendored so we can carry stability fixes (e.g. CLI relay reconnect cleanup) without an upstream release
- **Patches**: see `git log -- thirdparty/reconnecting-websocket` for the diffs we carry
