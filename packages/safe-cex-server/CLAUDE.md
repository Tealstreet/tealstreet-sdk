# CLAUDE.md — @tealstreet/safe-cex-server

Server-side cache and public-data adapters for `@tealstreet/safe-cex`.
This package is a library, not the deployed exchange-server application.

- **Exports**: `RedisCandleCacheBackend`, `PublicExchangeFactory`, market/ticker
  cache helpers, and the capability/health-filtered `PublicdataTickerBridge`
- **Market cache**: quantized UTC-hour snapshots, three-hour TTL, validated
  latest pointers, in-process refresh locking and latest-snapshot fallback
- **Ticker cache**: bootstrap seeds accepted for 150 seconds, 300-second TTL;
  these seeds are not live trading quotes
- **Key file**: `src/candleCache/RedisCandleCacheBackend.ts`
- **Dependency**: `@tealstreet/safe-cex`
- **Constraint**: runtime-agnostic — accepts a `RedisLikeCandleCacheClient` interface so the backend works with `ioredis`, `redis`, Valkey, or any compatible client
- **Design**: never imports a specific Redis client; consumers wire the connection
- **Validation**: if the cache contract changes here, run the candle-cache tests in `packages/safe-cex` too

The native frame server in `sdk-rs/hosts/native` replaces engine execution and
live frame transport, not these persistence or publicdata adapters. Keep their
exports and policies until an application migration explicitly replaces them.
See `docs/plans/RUST_REWRITE.md`, “Native frame server migration”.

## PublicExchangeFactory Rust lifecycle

- A current-hour Redis hit hydrates a lazy exchange without opening sockets or
  fetching metadata. Consumers that need live feeds explicitly start the exchange.
- A cold Rust cache miss awaits native startup before reading markets. Empty
  successful metadata reads are rejected rather than cached.
- Forced refresh bypasses Redis. If the Rust engine was already started (even
  externally), a fresh public engine re-fetches native metadata. Startup and cache
  persistence must succeed before the handle swaps engines and disposes the old
  one. Failed refresh leaves the last successful handle metadata and engine intact.
- Read `handle.exchange` again after refresh; do not retain its previous engine
  reference. Concurrent metadata refreshes share work. Disposing a handle cancels
  pending native startup and prevents stale completions from publishing a handle.
- Native factory conformance: build WASM, then run
  `corepack yarn -s workspace @tealstreet/safe-cex-server test-wasm`.
