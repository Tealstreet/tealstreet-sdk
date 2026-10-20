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
