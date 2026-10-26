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
- **Native sessions**: `ServerSessionFactory` explicitly selects native SCXF
  sessions or delegates to the existing `PublicExchangeFactory` handle. Native
  account targets come from the trusted application resolver; connection failures
  never fall back to a second TS engine. `NativeFrameSession` serializes async
  snapshot/flush/notice delivery with bounded queues; gaps/backpressure close the
  session. Commands retain the native JSON protocol and are never replayed.
- **Dependency**: `@tealstreet/safe-cex`
- **Constraint**: runtime-agnostic — accepts a `RedisLikeCandleCacheClient` interface so the backend works with `ioredis`, `redis`, Valkey, or any compatible client
- **Design**: never imports a specific Redis client; consumers wire the connection
- **Validation**: if the cache contract changes here, run the candle-cache tests in `packages/safe-cex` too

The native frame server in `sdk-rs/hosts/native` replaces engine execution and
live frame transport, not these persistence or publicdata adapters. Keep their
exports and policies until an application migration explicitly replaces them.
See `docs/plans/RUST_REWRITE.md`, “Native frame server migration”.

The session adapter and adoption example are in `src/frameSession/README.md`;
deployed application routes, authorization and shell features remain caller-owned.

## PublicExchangeFactory Rust lifecycle

- A current-hour Redis hit hydrates a lazy exchange without opening sockets or
  fetching metadata. Consumers that need live feeds explicitly start the exchange.
- Rust public catalogs with nondefault product/deployment selectors use a
  profile-qualified Redis namespace for snapshots, latest fallback and locks.
  The profile shares the public-feed identity (environment, product flags,
  Bybit site, Hyro mode and sorted Polymarket slugs). Default native catalogs
  and existing TS callers retain their original keys. Semantic `Market.info`
  and option terms survive serialization and warm hydration. The resolved
  exchange class selects this policy, including registry-default Rust Orderly
  brokers without an explicit engine flag; a resolved TS exchange keeps TS keys.
- A cold Rust cache miss awaits native startup before reading markets. Empty
  successful metadata reads are rejected rather than cached. Native startup is
  bounded by `nativeStartupTimeoutMs` (30 seconds by default), including venue
  retry loops; timeout disposes the candidate and permits stale Redis fallback.
  A cold fallback receives a fresh lazy engine, so explicit `exchange.start()`
  can recover when the venue becomes available. A concurrent successful refresh
  takes precedence over a previously pending stale-fallback read.
- Forced refresh bypasses Redis. If the Rust engine was already started (even
  externally) or its store holds cached markets, a fresh public engine re-fetches
  native metadata. Startup and cache
  persistence must succeed before the handle swaps engines and disposes the old
  one. Failed refresh leaves the last successful handle metadata and engine intact.
- Read `handle.exchange` again after refresh; do not retain its previous engine
  reference. Concurrent metadata refreshes in the same UTC hour share work; a
  request in the next hour waits and refreshes its own bucket. Disposing a handle cancels
  pending native startup and prevents stale completions from publishing a handle.
- Shared ports are rejected in factory options because one port cannot survive
  engine replacement. Attach a `RustPublicFeedProvider` to the returned direct
  public engine outside the factory; refresh disposes that old provider source.
- Native factory conformance: build WASM, then run
  `corepack yarn -s workspace @tealstreet/safe-cex-server test-wasm`.
