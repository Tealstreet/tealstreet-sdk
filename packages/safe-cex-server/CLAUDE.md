# CLAUDE.md — @tealstreet/safe-cex-server

Server-side candle-cache backend for `@tealstreet/safe-cex`. Stores chart data in Redis (or any Redis-compatible store).

- **Single export**: `RedisCandleCacheBackend`
- **Key file**: `src/candleCache/RedisCandleCacheBackend.ts`
- **Dependency**: `@tealstreet/safe-cex`
- **Constraint**: runtime-agnostic — accepts a `RedisLikeCandleCacheClient` interface so the backend works with `ioredis`, `redis`, Valkey, or any compatible client
- **Design**: never imports a specific Redis client; consumers wire the connection
- **Validation**: if the cache contract changes here, run the candle-cache tests in `packages/safe-cex` too
