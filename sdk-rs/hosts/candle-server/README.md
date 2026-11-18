# Native candle server

`candle-server` serves the HTTP contract of NX
`mobile-services/candle-server/src/server.ts` at `data.tealstreet.io/candles`,
with no Worker in front: every route answers both under `/candles/` and bare.

| Route | Notes |
| --- | --- |
| `GET /health`, `/healthz` | memory, uptime, residents, limits and the Redis probe (`redisReachable` pages) |
| `GET /api/market-data/symbols` | `exchange`, `query`; first 50 `{symbol, base, active}` |
| `GET /api/market-data/markets` | `exchange`, `query`, `limit` (100 default, at most 10000) |
| `GET /api/market-data/market` | `exchange`, `symbol` |
| `GET /api/market-data/candles` | `exchange`, `symbol`, `interval`, `from`, `to`, `limit` (at most 5000) |

`GET /candles` redirects (308) to `/candles/`. Anything else is
`404 {"error":"not found"}`. Errors are `{error, code}` with 400 `bad-request`,
404 `market-not-found`, 503 `upstream-error` and 503 `service-unavailable`.
The query parameter `verbose` is never honoured. Only a 200 candle response
whose `to` falls before the current candle of its interval carries
`Cache-Control: public, max-age=<CANDLE_CACHE_TTL_SECONDS, default 300>`;
everything else is `no-store`.

Upstream data comes from public-only native engines (`safe-cex-venues`, the
public feed disabled, optional polling paused): one per venue, at most
`CANDLE_MAX_RESIDENT_EXCHANGES` (8) resident, evicted after
`CANDLE_EXCHANGE_IDLE_TTL_MS` (600000) idle. Candles go through the
`candle-cache-owner` binary from `safe-cex-candle-cache` (Redis when
`REDIS_URL` is set, memory otherwise). It runs as a separate process, so that
crate's `arbitrary_precision` serde_json features never unify into the engine.
Build the two packages in separate cargo invocations:

```bash
cargo build --release --locked -p safe-cex-candle-server
cargo build --release --locked -p safe-cex-candle-cache --bin candle-cache-owner
```

The server starts the owner from its own directory, or from
`CANDLE_CACHE_OWNER_BIN`, and exits if the owner dies. It listens on
`0.0.0.0:TEALSTREET_CANDLE_PORT` (23457). `CANDLE_SERVER_CORS_ORIGINS` (`*`) is
read per request. Alerts are stderr lines. At most three engines start at once;
a further cold start answers 503, so a caller cycling exchanges cannot
amplify venue market loads. Per-IP rate limiting belongs to the edge.

The first request for a venue waits for its catalogue, at most 30 s, and stops
waiting at the first failed or empty read while the engine retries behind it.
Until the catalogue loads, `symbols` and `markets` answer 503 and candles skip
the symbol precheck, answering 503 rather than an empty 200.

Known differences from the TS server:
- `/api/market-data/markets` sorts ties with an approximation of ICU
  `localeCompare`.
- `heapMB` reports RSS.
- There is no Slack or Sentry sink.
