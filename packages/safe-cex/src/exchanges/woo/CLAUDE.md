# WooX Exchange Integration

This directory contains the WooX perpetual futures exchange integration for Tealstreet.

## Exchange Information

- **Type**: Centralized Exchange (CEX)
- **Products supported**: USDT-margined perpetual futures (`PERP_` prefix symbols)
- **Position modes**: One-Way (`ONE_WAY`) and Hedge (`HEDGE_MODE`)
- **Margin types**: Cross supported; Isolated detected but warned as unsupported
- **Account mode requirement**: `FUTURES` mode only (non-futures accounts raise an error)

## API Documentation

- **Current API documentation**: https://developer.woox.io/api-reference/introduction
- **Official migration map**: https://developer.woox.io/api-reference/migration_guide
- **V3 futures statistics**: https://developer.woox.io/api-reference/endpoint/public_data/futures
- **V3 BBO stream**: https://developer.woox.io/api-reference/endpoint/websocket/BBO
- **V3 Trade stream**: https://developer.woox.io/api-reference/endpoint/websocket/TRADE
- **V3 Cancel All After**: https://developer.woox.io/api-reference/endpoint/trading/cancel_all_after
- **Legacy API documentation** (for migration work): https://docs.woox.io/#restful-api
- **Legacy all-market funding rates**: https://docs.woox.io/products/woo-x

## Legacy API lifecycle

WooX scheduled both the legacy REST and WebSocket surfaces used here for deprecation on December 31,
2025 and describes them as no longer actively maintained. Do not expand product capabilities on the
legacy surface; narrow reliability repairs needed while V3 remains gated must be documented and
fixture-backed. The phased endpoint inventory, parity gates, rollback strategy, and monitoring requirements live in
`docs/safe_cex_audit_july/woo-v3-migration-plan.md`. Preview and prod are both production lanes, so
write-path migration requires fixture coverage and a credentialed WooX demo lifecycle smoke first.

The V3 foundation is deliberately non-routable: `woo.v3.contracts.ts` holds the typed route map,
response/request contracts, and eight hard-false migration gates, while `woo.v3.request.ts` only
prepares deterministic transport-neutral request descriptors. `woo.v3.public.ts` adds strict
unknown-input decoders and pure mappers for the five public REST reads. `woo.v3.ticker.ts` purely
composes one decoded futures-stat row with decoded matching BBO and Trade events.
`woo.v3.private.ts` does the
same for account info, asset balances, futures positions, pending regular/algo orders, and transaction
history. `woo.v3.regular-writes.ts` adds strict, pure descriptors and response decoders for regular
futures place, zero-fill native amend, cancel-by-ID, and regular pending-order cancellation. It is
equally inert and accepts no transport, credentials, or mutable store.
`woo.v3.algo-writes.ts` adds the matching inert algo descriptors for partial STOP-market,
explicitly vendor-native trailing-stop, positional TP/SL, zero-fill STOP trigger-amend, cancel-by-ID,
and all-symbol pending cancellation.
`woo.v3.account-writes.ts` adds inert leverage read/write and position-mode descriptors. The leverage
read requires symbol, margin mode, and position mode in its signed query, matches all response scope,
and accepts exactly one positive-integer `BOTH` detail in one-way mode or unique positive-integer
`LONG` and `SHORT` details in hedge mode. The published default-margin-mode response is unusable and
is deliberately excluded. Cross one-way leverage
is descriptor-ready from decoded margin-account provenance; cross-hedge leverage and every isolated
path fail closed. A position-mode request requires one atomic reconciliation ID across decoded
account mode, a complete positions page proving every holding and pending quantity is exact decimal
zero, and complete empty pending regular/algo order pages. Mutation responses prove request
acceptance only, so authoritative readback remains mandatory.
`woo.v3.cancel-all-after.ts` adds an inert VIP-only dead-man-switch descriptor for
`POST /v3/trade/cancelAllAfter`. Its `triggerAfter` is an integer millisecond offset from 0 through
900000 inclusive; 0 cancels the timer, and a repeated request replaces the existing timer. The
strict success decoder accepts exact response/data keys and nonnegative safe-integer server
timestamps. The endpoint cancels both ordinary and algo orders, so it has its own hard-false
`cancelAllAfterDemoLifecycle` gate rather than inheriting either order-family gate. It has no
transport owner, runtime import, or activation; the general REST selector always rejects this route
until an owner is separately designed and validated.
`woo.v3.websocket.ts` adds the matching inert WebSocket channel/topic inventory, listen-key and
subscription descriptors, strict public/private wire decoders, latency-only heartbeat
classification, and pure sequence/reconnect/resubscribe primitives.
`woo.v3.runtime-selector.ts` adds the first inert runtime boundary: an explicit legacy/shadow-v3/v3
REST route selector plus a pure shadow-read comparator. It emits no requests and owns no store.
Legacy remains authoritative in both legacy and shadow modes, shadow plans are limited to GET-to-GET
migrations, every mutation is rejected in shadow mode, and V3 authority is rejected while the
corresponding hard gate is false. Unknown runtime modes fail closed. Routes without active owners
(`balances`, algo pending-order bulk cancellation, and Cancel-All-After) are rejected in every mode.
The already-active `/v3/trade/transactionHistory` route is a no-op in legacy/V3 mode and is rejected
in shadow mode so it can never be double-called. The comparator never passes the authoritative
legacy snapshot to callbacks: callers provide an already-built canonical string, and the only
callback decodes and canonicalizes V3. It returns the original snapshot by identity on matches,
mismatches, and decode/map/canonicalization failures.
`woo.v3.public-parity.ts` supplies the endpoint-specific canonical strings and structured mismatch
diagnostics for the five public GET routes. Markets compare only normalized safe-cex fields and
ignore source-only `info`; books require identical legacy/V3 request symbols before comparing
normalized level order, price, amount, and cumulative total. Both kline routes require exact
legacy/V3 request symbols and normalized intervals, then validate every decoded V3 row's `symbol`,
`type`, and exclusive `endTimestamp = startTimestamp + fixed interval` before comparing exact OHLCV.
`1mon`/`1y` remain blocked because their variable calendar boundaries are not safely expressible as
a fixed duration. All comparisons are exact and retain the legacy object by identity. The ticker
comparator diagnoses the REST statistics shared by
both generations but always returns a typed blocker: legacy REST initializes bid/ask/last from
`24h_close`, while V3 REST publishes no live quote fields. BBO+Trade coherence remains required.
Separate snapshot request timing can naturally produce book/statistic mismatches, so these results
are observation evidence, not proof that a gate may be enabled.
None of these files is imported by
`woo.api.ts`, `woo.exchange.ts`, or either websocket implementation, and none owns an Axios,
fetch, or websocket client. Enabling any gate or wiring these modules into runtime is a separate
review requiring the corresponding parity/demo evidence. Concrete endpoint pages and the official
migration table use `/v3/trade/*`; do not follow the introduction page's inconsistent `/trading`
label.

The V3 WebSocket foundation does not own a socket, credentials, timers, or stores. It covers the two
documented `/v3/public` and `/v3/private?key=<listenKey>` channels, all 12 public topic kinds, and the
five private topic names. The private listen key comes from the normal signed V3 REST descriptor;
creating a new key immediately expires the old one, and WOO does not publish a safe overlap/rotation
sequence. PING/PONG is an optional latency probe, not a keepalive requirement, and the current docs
do not publish server-initiated PING. Command pages use an untyped request-ID placeholder but a
string response example; inert builders emit that documented string subset, while a typed blocker
preserves the unresolved request-ID type instead of declaring numeric IDs invalid.
Commands are channel-bound transport descriptors: their exact JSON wire body is nested separately
from the `public`/`private` URL identity. Mixed-channel subscription plans and acknowledgements fail
closed, and every acknowledgement topic is parsed through the same official topic inventory.

Incremental books must buffer, join a V3 REST snapshot where the first `prevTs` equals the snapshot
timestamp, and then require each next `prevTs` to equal the previous update's `ts`; any mismatch or
reconnect requires a fresh snapshot. The inert REST decoder requires the top-level snapshot timestamp
to be a positive JavaScript-safe integer in milliseconds and preserves it through the V3-specific
mapped book and parity observation. Value-parity canonicalization intentionally excludes timestamp
because legacy and V3 snapshots are separate requests, but the typed V3 result retains it for that
future `prevTs` join. No socket, buffering/recovery coordinator, or runtime activation implements the
join yet, and every V3 migration gate remains false. Private frames decode only to gateway wire models. Do not
replace store state from them until credentialed evidence proves whether account/balance/position
arrays are snapshots or patches, the exhaustive ordinary/algo execution lifecycle, command failure
envelopes, and listen-key rotation behavior. These are typed blockers, so neither WebSocket parity
gate is satisfied by the inert fixture layer.

Public decimal sign/zero validation is lexical, never `Number`-based. This preserves WOO's
decimal-string precision for values smaller than JavaScript can represent. Prices and executable
sizes must contain a nonzero digit; incremental-book quantities allow zero only for documented level
deletion, while signed funding/PnL/holdings remain signed wire values.

The V3 futures REST row is first mapped to `WooV3FuturesStats`, not directly to a safe-cex `Ticker`.
The official response has index/mark/funding/open-interest and rolling 24-hour fields, but no
authoritative best bid, best ask, or last-trade field. The inert ticker composer requires one
strictly decoded `PERP_BASE_QUOTE` futures row, BBO event, and Trade event with matching normalized
identity. It takes bid/ask only from BBO and last only from Trade `px`, never from `24hClose`, and
rejects non-finite, non-positive, overflowed, or underflowed numeric prices. REST mark/index/funding/
open-interest and 24-hour statistics remain authoritative and are retained in typed provenance.
safe-cex `fundingRate` uses V3 `estFundingRate`, matching the active legacy `estfundingrate` stream's
predicted-funding semantics; the distinct `lastFundingRate` remains available only in
`info.futuresStats` provenance and is never a fallback for a missing or malformed estimate.
This is composition evidence only: runtime subscription/coherence and both public gates remain
false. The V3 orderbook page
documents both sides as descending, so the inert mapper sorts into safe-cex's required bid-descending
/ ask-ascending order before calculating cumulative totals. The instruments page has drifted on
whether `status` is present in its example, so the strict decoder treats only that field as optional;
an absent status does not make a documented perpetual inactive.

Authenticated V3 reads must remain fail-closed. Financial values documented as decimal strings are
decoded only from strings; the transaction-history `fee` remains the official JSON-number exception.
Account `freeCollateral` maps to safe-cex `free`, `totalAccountValue` to `total`, and
`totalCollateral - freeCollateral` to `used`; the endpoint has no uPnL field, so the pure mapper takes
the previous known-good uPnL explicitly. Token holdings keep trading/free/frozen, stake, vault, earn,
and interest buckets distinct.

Do not construct a safe-cex `Position` from `/v3/futures/positions` alone: the official response omits
leverage and margin mode, and even disagrees between `data.positions` in the example and `data.rows`
in OpenAPI. The inert decoder accepts exactly one envelope key, and its mapper stops at a
gateway-labelled position read model. Algo reads have similar official drift: `quantity` alternates
between number and decimal string, response examples add `TAKE_PROFIT`/`STOP_LOSS` child enums absent
from the schema, and trailing/container rows do not expose a lossless absolute trigger for safe-cex.
Only decimal-string TP/SL leaves with an explicit `triggerPrice` map; all other valid shapes are
returned as deferred gateway rows. A future comparator must decode/map a complete page before store
replacement and retain the previous known-good slice on any failure or deferred ambiguity.
Transaction rows omit `positionSide`; they map losslessly to `Execution`, while `HistoricalTrade`
is emitted with `both` only for a separately decoded one-way account and is deferred in hedge mode.

Regular V3 write descriptors are futures-only and fail closed. They accept `PERP_BASE_QUOTE`
symbols and decimal-string base quantity, never the spot-only quote `amount`; encode GTC/IOC/FOK/
post-only through WOO's `type`; derive hedge `positionSide` from side plus reduce-only while omitting
it in one-way mode; hard-code `marginMode: CROSS` to match current Woo metadata/runtime support; and
use only JavaScript-safe integer IDs. Isolated writes remain outside this contract. Place responses must echo type,
quantity, price when applicable, and client ID, and must return a previously unknown positive order
ID. Amend/cancel responses do not echo an ID, so their identity evidence is the validated request
target only. Native amend is descriptor-ready only for a decoded `NEW` order with zero executed
quantity, where total and remaining quantity are identical. Partial-fill amend remains a typed
blocker because the endpoint page does not unambiguously define total-versus-remaining request
quantity; it needs a credentialed demo lifecycle before use. `/v3/trade/allOrders` is also deferred
from this regular-only layer because it cancels algo orders as well. These contracts do not satisfy
`regularWriteDemoLifecycle`; a credentialed demo lifecycle is required before any active runtime
imports them, and all migration gates remain literal `false`.

Algo V3 writes are similarly fail-closed. STOP and positional TP/SL descriptors accept only the
officially documented `MARKET_PRICE` trigger, which WOO defines as last price; do not translate the
legacy `MARK_PRICE`, `INDEX_PRICE`, or `LAST_PRICE` labels. Trailing descriptors require an explicit
`WOO_NATIVE_CALLBACK_VALUE` source and never derive callback units from a safe-cex absolute trigger
because the official page does not define that conversion. Positional placement builds at most one
`TAKE_PROFIT` and one `STOP_LOSS` `CLOSE_POSITION` child, but its success rows do not publish the
root/leaf relationship needed for safe-cex IDs, so decoding stops at a typed demo blocker. The edit
page has no `childOrders` or trailing callback fields: positional and trailing edits are blocked,
while only trigger-price changes on decoded untriggered zero-fill `STOP` orders with an existing
`MARKET_PRICE` trigger are descriptor-ready. Quantity and partial-fill edits are also blocked. The
pending-cancel page says symbol/all scope while its query schema publishes IDs and no symbol; only
the no-query all-symbol descriptor is buildable. Current amend/cancel examples return empty `data`,
so decoders preserve the validated request target and never invent a response status or echoed ID.
None of this satisfies `algoWriteDemoLifecycle`.

Account V3 configuration is also fail-closed. `GET /v3/futures/leverage` is the authoritative future
readback contract because position rows omit leverage; its symbol and both modes must match the
request and its leg set must exactly match the requested position mode. `PUT /v3/futures/leverage`
accepts a positive integer and the
official cross setter ignores `positionMode`; safe-cex therefore builds only cross one-way requests
until a credentialed demo proves which leverage state cross-hedge mode changes. Isolated leverage is
outside the current cross-only integration contract. `PUT /v3/futures/positionMode` is buildable only
from the atomic decoded empty-state evidence above. Its response, like the leverage response, contains
only `success` and `timestamp`, so never infer resulting leverage or position mode from it. Read back
the authoritative account/leverage endpoints before considering the state changed. None of this
satisfies `accountWriteDemoLifecycle`.

Cancel-All-After is VIP-only and spans both ordinary and algo order families. The official schema
and prose define `triggerAfter` as an integer millisecond timeout capped at 900000 and use 0 to
cancel the timer. The same page's example is a contradictory quoted, absolute-looking timestamp;
the inert builder follows the typed schema, prose, and migration map and rejects that example shape.
Before activation, a credentialed VIP demo must prove timer creation, replacement, zero-disable,
ordinary+algo cancellation, reconnect/outage behavior, and authoritative readback/observability.
None of this satisfies `cancelAllAfterDemoLifecycle`.

## CCXT Reference Implementation

- **REST API**: https://github.com/ccxt/ccxt/blob/master/ts/src/woo.ts
- **WebSocket**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/woo.ts

## File Structure

- `woo.api.ts` - REST API client with dual V1/V3 HMAC-SHA256 signature methods
- `woo.exchange.ts` - Main exchange class (`WOOXExchange`) extending `BaseExchange`
- `woo.types.ts` - TypeScript type definitions, endpoint constants, type maps
- `woo.v3.contracts.ts` - Non-routable namespaced V3 endpoints, contracts, migration map, and gates
- `woo.v3.private.ts` - Inert strict authenticated-read decoders and lossless/deferred state mappers
- `woo.v3.public.ts` - Inert strict public-REST decoders and pure market/stat/book/candle mappers
- `woo.v3.ticker.ts` - Inert authoritative REST-stats + BBO + Trade ticker composition
- `woo.v3.regular-writes.ts` - Inert strict regular-order request/response contracts and amend blockers
- `woo.v3.algo-writes.ts` - Inert strict algo-order request/response contracts and lifecycle blockers
- `woo.v3.account-writes.ts` - Inert strict leverage read/write and position-mode contracts/evidence blockers
- `woo.v3.cancel-all-after.ts` - Inert strict VIP dead-man-switch descriptor and response decoder
- `woo.v3.request.ts` - Pure V3 request normalization/signing descriptor (no transport or I/O)
- `woo.v3.websocket.ts` - Inert V3 WS URLs/topics, strict decoders, blockers, and recovery primitives
- `woo.v3.runtime-selector.ts` - Inert REST generation selector and legacy-authoritative shadow-read comparator
- `woo.v3.public-parity.ts` - Exact public-read canonical strings, mismatch provenance, and ticker blocker
- `woo.utils.ts` - Symbol normalization utilities (`PERP_BTC_USDT` <-> `BTCUSDT`), memoized
- `woo.ws-private.ts` - Private WebSocket (HMAC auth, execution reports, algo execution reports)
- `woo.ws-public.ts` - Public WebSocket (tickers, BBOs, mark prices, klines, orderbook)

## Authentication

WooX uses **two different HMAC-SHA256 signature methods** depending on API version.

### V1 Signature (`signV1` in `woo.api.ts`)

1. Serialize params alphabetically with `qs.stringify` (sorted via `localeCompare`)
2. Concatenate: `serialized_params|timestamp` (pipe separator)
3. Sign: `HMAC-SHA256(concatenated_string, secret) -> hex`
4. For POST/DELETE/PUT: data is moved to query string, body is set to empty string `''`
5. Content-Type for mutations: `application/x-www-form-urlencoded`
6. **Exception**: `POSITION_MODE` endpoint body is NOT cleared

### V3 Signature (`signV3` in `woo.api.ts`)

1. Concatenate: `timestamp + METHOD + url + json_body` (no separators)
2. If params exist, they are serialized into the URL before signing (then `delete nextConfig.params`)
3. Sign: `HMAC-SHA256(concatenated_string, secret) -> hex`
4. Body stays as JSON for POST requests

### Common Headers

- `x-api-key`: API key
- `x-api-timestamp`: Millisecond timestamp from `virtualClock`
- `x-api-signature`: HMAC-SHA256 hex signature
- `cache-control`: `'no-cache'`

### Route Selection (`createAPI` request interceptor)

The interceptor automatically selects the signing method based on URL:

- URLs containing `v1` use `signV1`
- All other URLs use `signV3`
- URLs containing `'/public/'` skip authentication entirely

This selection applies only to the existing legacy client. The namespaced V3 foundation does not
participate in `createAPI`; its signing fixtures lock the official
`timestamp + METHOD + pathWithQuery + exactJsonBody` contract for a later migration.

### Dual API Keys

WooX supports primary and secondary API keys via `createApiWithSecondaryKey`:

- `_primaryXhr` - Primary API key instance
- `_secondaryXhr` - Secondary API key instance (if `options.secondary_key` is set)
- `xhr` - Auto-switching between primary and secondary
- `_proxyEnforcedXhr` - Separate instance with proxy enforcement for specific endpoints (kline history)

### Broker ID Injection (`createAPI` request interceptor)

- On `PLACE_ORDER` POST: injects `broker_id` field
- On `ALGO_ORDER` POST: injects `brokerId` field (different key name)
- Livenet broker ID: `ab82cb09-cfec-4473-80a3-b740779d0644`
- Testnet broker ID: `e708a644-9ce5-46c6-b50c-74b15f62d8ca`
- Injection is wrapped in try/catch to avoid breaking retry logic

### Private WebSocket Authentication (`WooPrivateWebscoket.onOpen`)

On connection open, sends auth event with HMAC signature:

```json
{
  "event": "auth",
  "params": {
    "apikey": "<api_key>",
    "sign": "HMAC-SHA256('|' + timestamp, secret) -> hex",
    "timestamp": "<ms_timestamp>"
  }
}
```

Note: The sign string is `|<timestamp>` (pipe + timestamp), signed with the secret. Subscriptions are sent only after receiving `{ "event": "auth", "success": true }`; an auth failure emits the server's `message`/`error` and leaves the socket unsubscribed.

## Startup Sequence

The `start()` method orchestrates initialization:

1. **Parallel fetch** (`Promise.all`):
   - `fetchMarkets()` -> on completion, stores markets, sets `loaded.markets = true`, then immediately calls `publicWebsocket.connectAndSubscribe()` (public WS starts as soon as markets are loaded)
   - `fetchTickersRaw()` -> raw ticker data fetched in parallel
2. **Parse tickers**: After both resolve, `parseTickers()` converts raw data, stores tickers, sets `loaded.tickers = true`, and starts the exchange-owned legacy funding refresh timer
3. **Branch by mode**:
   - **Public data only** (no keys or `publicDataOnly` flag): Marks orders/positions/balance as loaded (empty), returns immediately
   - **Private mode**:
     1. Connects private WebSocket (`privateWebsocket.connectAndSubscribe()`)
     2. **Parallel fetch** of initial private data (`Promise.all`):
        - `fetchBalance()` -> stores balance, sets `loaded.balance = true`, also caches `_accountPositionMode`
        - `fetchPositions()` -> stores positions via `addOrUpdatePositionsFull()`, sets `loaded.positions = true`
        - `fetchOrders()` -> fetches both limit and algo orders, stores them, sets `loaded.orders = true`
     3. Starts `tick()` loop
     4. Logs "Ready to trade on WOO X"

**Key difference from many other exchanges**: WooX fetches orders via REST on startup (not just WS snapshot).

### Tick Loop (`tick`)

- Runs on configurable interval via `options.extra.tickInterval`
- Counter-based selective fetching:
  - **Every tick**: Fetches positions
  - **Every 5th tick** (`count % 5 === 0`): Fetches positions AND balance together via `fetchPositionsAndBalance()`
  - **Every 10th tick** (`(count + 5) % 10 === 0`): Fetches orders (both limit and algo) -- offset by 5 to avoid colliding with balance fetch
- On error, logs and continues the loop

## REST API Endpoints

| Endpoint                                | Version | Method      | Description                     |
| --------------------------------------- | ------- | ----------- | ------------------------------- |
| `/v3/accountinfo`                       | v3      | GET         | Account info, balance, mode     |
| `/v3/balances`                          | v3      | GET         | No request owner; metadata only |
| `/v3/positions`                         | v3      | GET         | Open positions                  |
| `/v3/position/{symbol}`                 | v3      | GET         | Single position                 |
| `/v3/order/{orderId}`                   | v3      | PUT         | Update limit order              |
| `/v3/algo/orders`                       | v3      | GET         | Algo orders (paginated)         |
| `/v3/algo/order/{id}`                   | v3      | PUT/DELETE  | Update / cancel algo order      |
| `/v3/trade/transactionHistory`          | v3      | GET         | Execution history               |
| `/v3/orders/pending`                    | v3      | DELETE      | Cancel all pending orders       |
| `/v3/algo/orders/pending`               | v3      | DELETE      | No request owner; metadata only |
| `/v1/public/info`                       | v1      | GET         | Markets/symbol info             |
| `/v1/public/futures`                    | v1      | GET         | Futures tickers                 |
| `/v1/public/funding_rates`              | v1      | GET         | Estimated funding for all perps |
| `/v1/public/orderbook/{symbol}`         | v1      | GET         | Order book snapshot             |
| `/v1/orders`                            | v1      | GET/DELETE  | Limit orders / cancel by symbol |
| `/v1/client/trades`                     | v1      | GET         | Recent historical trades        |
| `/v1/client/hist_trades`                | v1      | GET         | Archived historical trades      |
| `/v1/order`                             | v1      | POST/DELETE | Place / cancel single order     |
| `/v1/kline`                             | v1      | GET         | Kline data (recent)             |
| `https://api-pub.woox.io/v1/hist/kline` | v1      | GET         | Historical kline data           |
| `/v1/client/leverage`                   | v1      | POST        | Set account leverage            |
| `/v1/client/futures_leverage`           | v1      | GET/POST    | Get/set per-symbol leverage     |
| `/v1/client/position_mode`              | v1      | POST        | Change position mode            |

## WebSocket Implementation

### Network Configuration

- **Public (Livenet)**: `wss://wss.woox.io/ws/stream/<applicationId>`
- **Public (Testnet)**: `wss://wss.staging.woox.io/ws/stream/<applicationId>`
- **Private (Livenet)**: `wss://wss.woox.io/v2/ws/private/stream/<applicationId>`
- **Private (Testnet)**: `wss://wss.staging.woox.io/v2/ws/private/stream/<applicationId>`

Both public and private WS URLs are appended with `options.applicationId`.

### Public WebSocket (`woo.ws-public.ts`)

**On connect**, subscribes to three global topics immediately:

1. `tickers` - Last price, volume, quote volume for all symbols
2. `bbos` - Best bid/offer for all symbols
3. `markprices` - Mark prices for all symbols

Then re-subscribes to any previously active on-demand topics (klines, orderbooks) and resets orderbook state.

The official funding WebSocket topic is symbol-scoped (`estfundingrate@{symbol}`), so this integration
does not send the invalid global `estfundingrate` subscription or assume an all-market array message.
Funding is independent of WebSocket connect/reconnect and is refreshed by REST as described below.

**Ping/pong**: Sends `{ "event": "ping" }`. Responds to server-initiated pings with `{ "event": "pong" }` in `handlePingEvent`. Pong receipt triggers `handlePongEvent()` from `BaseWebSocket`, which schedules the next ping after 5 seconds.

**Reconnection**: On reconnect, emits `emitWsReconnected()` if `ENABLE_RECONNECTING_WEBSOCKET` is enabled.

**Message routing**: Uses string matching on raw data (`data.includes(...)`) to route messages to handlers. Matches against both `event":"<topic>` and `topic":"<topic>` patterns. First matching handler wins (break after match).

**Ticker data handling**: Filters to `PERP_` symbols only. Updates are split across multiple topics:

- `tickers`: Updates `last`, `volume`, `quoteVolume`
- `bbos`: Updates `bid`, `ask`
- `markprices`: Updates `mark`

**OHLCV streaming** (`WooPublicWebsocket.listenOHLCV`):

- Primary topic: `<PERP_SYMBOL>@kline_<interval>` (e.g., `PERP_BTC_USDT@kline_1m`)
- Secondary topic: `<PERP_SYMBOL>@trades` (subscribed only if not `publicDataOnly`)
- Primary parser: Uses `data.startTime` for timestamp, builds/updates candle. Rejects candles with timestamps in the past.
- Secondary parser: Constructs candles from trade data using `getCandleStartTime()`. Especially important for sub-second intervals (`opts.interval.endsWith('s')`) where kline WS doesn't fire frequently enough.
- Cleanup: Unsubscribes from both primary and secondary topics when listener is removed

**Orderbook management** (`WooPublicWebsocket.listenOrderBook`):

1. Subscribe to `<PERP_SYMBOL>@orderbookupdate`
2. On first WS message, trigger REST snapshot fetch via `/v1/public/orderbook/<symbol>`
3. Buffer WS updates while snapshot loads
4. After snapshot, filter buffered updates by `ts > data.timestamp` (timestamp-based, not sequence)
5. Apply buffered updates, then process incrementally
6. Uses `updateOrderBook()` utility for insert/update/delete operations
7. WS update data format: `{ data: { bids: [[price, amount], ...], asks: [[price, amount], ...] } }`

### Private WebSocket (`woo.ws-private.ts`)

Class name: `WooPrivateWebscoket` (note: typo in original -- "Webscoket")

**Connection flow**:

1. Connect with applicationId in URL
2. On open: Send auth message, optionally call `fetchAndSetPrivateSnapshot()` if `ENABLE_RECONNECTING_WEBSOCKET`
3. On auth success (`{ "event": "auth", "success": true }`): Call `subscribe()` which subscribes to `executionreport` and `algoexecutionreportv2` topics

**Ping/pong**: Sends `{ "event": "ping" }`. Responds to server pings with `{ "event": "pong" }`. Handles both directions.

**Execution Reports** (`WooPrivateWebscoket.handleExecutionReport`):

- Maps order data using `parent.mapLimitOrder()` (shared with REST order mapping)
- Status handling:
  - `REPLACED`, `NEW`, `PARTIAL_FILLED`: Add or update order in store
  - `CANCELLED`, `FILLED`: Update order status to Canceled/Closed
  - `FILLED`, `PARTIALLY_FILLED`: Emit fill event with `executedPrice` and `executedQuantity`

**Algo Execution Reports** (`WooPrivateWebscoket.handleAlgoExecutionReport`):

- Receives array of algo order updates
- Maps using `parent.mapAlgoOrder()` which returns an array (handles child orders)
- Status handling:
  - `NEW`, `REPLACED`, `PARTIAL_FILLED`: Add or update first mapped order
  - `FILLED`: Emit fill event
  - `CANCELLED`, `FILLED`: Update order status

## Order Types and Mappings

| Internal Type                | WooX API Value  | Notes                                                       |
| ---------------------------- | --------------- | ----------------------------------------------------------- |
| `OrderType.Limit`            | `LIMIT`         | Requires `order_price`                                      |
| `OrderType.Limit`            | `POST_ONLY`     | Mapped as Limit with PostOnly TIF; sent as type `POST_ONLY` |
| `OrderType.Market`           | `MARKET`        |                                                             |
| `OrderType.TakeProfit`       | `TAKE_PROFIT`   | Via algo order API (`/v3/algo/order`)                       |
| `OrderType.StopLoss`         | `STOP_LOSS`     | Via algo order API                                          |
| `OrderType.TrailingStopLoss` | `TRAILING_STOP` | Via algo order API with `callbackValue`                     |

### Trigger Type Mapping

| WooX Value     | Internal Type                    |
| -------------- | -------------------------------- |
| `MARK_PRICE`   | `OrderStopTriggerType.MarkPrice` |
| `MARKET_PRICE` | `OrderStopTriggerType.LastPrice` |

Note: WooX uses `MARKET_PRICE` (not `LAST_PRICE`) to refer to the last/contract price.

### Order Side Mapping

- `BUY` -> `OrderSide.Buy`
- `SELL` -> `OrderSide.Sell`

## Order Lifecycle

### Regular Order Placement (`placeOrder`)

1. If algo order type (StopLoss, TakeProfit, TrailingStopLoss), delegates to `placeAlgoOrder()`
2. For limit/market orders:
   - Resolves position side via `getPositionSideForOrder()` (undefined for one-way, `'LONG'`/`'SHORT'` for hedged)
   - Maps PostOnly TIF to `type: 'POST_ONLY'` (not a separate TIF parameter)
   - Splits into lots if amount exceeds `market.limits.amount.max`
   - Each payload sent via POST to `/v1/order`
3. After main order placement, if `opts.stopLoss` or `opts.takeProfit` are set, places positional algo order with inverted side

**Reduce-only downsizing**: WOO does not auto-cap reduce-only orders to the
open position server-side, so `autoDownSizeReduceOrders: true` is set in
metadata — the shared `downSizeReduceOrder` web-layer util clamps the
amount before it reaches `placeOrder`. `placeOrder` itself does not clamp.
See `../CLAUDE.md` → "Reduce-only order downsizing".

### Algo Order Placement (`placeAlgoOrder`)

Three distinct paths based on order type and amount:

1. **Trailing Stop** -> `placeTrailingStopLossOrder()`
2. **TP/SL with specific amount** (partial) -> `placeStopMarket()` using `algoType: 'STOP'`, `type: 'MARKET'`
3. **TP/SL without amount** (full position) -> `placePositionalAlgoOrder()` using `algoType: 'POSITIONAL_TP_SL'` with `childOrders` array

### Positional TP/SL Orders (`placePositionalAlgoOrder`)

- Uses `algoType: 'POSITIONAL_TP_SL'` with `childOrders` array
- Each child has `type: 'CLOSE_POSITION'` and `algoType: 'STOP_LOSS'` or `'TAKE_PROFIT'`
- Both TP and SL can be placed in a single request as two children
- Posted to `/v3/algo/order`

### Sibling Order Management

When placing a new TP or SL, the code checks for an existing sibling (the other type):

1. Find the existing opposite-type sibling with the same symbol and closing side in the store. The
   side match is required in hedge mode so a long bracket never consumes the short bracket's price.
2. If a sibling exists, cancel the shared parent through `cancelAlgoOrder(sibling, false)` so the old sibling is not recreated.
3. Place one combined positional TP/SL parent with the retained sibling price and the new price.
   Placement failures are emitted and rethrown because the old parent has already been cancelled;
   callers must never interpret an unprotected position as a successful empty result.

When directly cancelling one side of a TP/SL pair (`cancelAlgoOrder`):

1. Cancel the parent algo order
2. If a sibling exists, automatically re-create just the sibling via `placePositionalAlgoOrder()`
3. If the cancel response says "already canceled", silently update order status instead of throwing

### Order Update (`updateOrder`)

Three paths based on order type:

1. **Trailing stop**: Cancel and replace (no native update support)
2. **Algo order** (TP/SL): PUT to `/v3/algo/order/{parentId}` with:
   - If order has amount (partial stop): Updates `triggerPrice` directly
   - If no amount (positional): Updates via `childOrders` array with `algoOrderId` and `triggerPrice`
3. **Limit order**: Direct PUT to `/v3/order/{orderId}` with optional `price` and `quantity` fields (no cancel-replace needed)

### Cancel Operations

- **Single limit order**: DELETE `/v1/order` with `order_id` (parsed to integer) and `symbol`
- **Single algo order**: DELETE `/v3/algo/order/{parentId}`
- **All orders**: DELETE `/v3/orders/pending`
- **Symbol orders**: DELETE `/v1/orders` with `symbol` param
- **Parent ID deduplication**: `cancelOrders()` filters out orders sharing the same `parentId` to avoid cancelling the same algo parent multiple times

### Trailing Stop Loss (`placeTrailingStopLossOrder`)

- Resolves position for the symbol and opposite side
- Calculates `priceDistance` as absolute difference between `ticker.last` and `opts.price`
- Sets `algoType: 'TRAILING_STOP'`, `type: 'MARKET'`
- `callbackValue` is the price distance (string)
- Always uses the full position size (`position.contracts`)
- `reduceOnly: true`

### Large Order Splitting

Orders exceeding `market.limits.amount.max` are automatically split into lots:

- Both regular orders (`placeOrder`) and stop market orders (`placeStopMarket`) support this
- Calculates lot count and remainder, sends via `mapSeries` (sequential)

## Symbol Format

- WooX uses `PERP_BTC_USDT` format (underscore-separated with `PERP_` prefix)
- Internal format: `BTCUSDT` (concatenated)
- **`normalizeSymbol`**: `PERP_BTC_USDT` -> split on `_` -> take [1] and [2] -> `BTCUSDT`
- **`reverseSymbol`**: `BTCUSDT` -> strip `USDT` -> `PERP_<base>_USDT`
- Both functions are memoized with lodash `memoize`
- Market `id` field: `<base>/<quote>:USDT` (e.g., `BTC/USDT:USDT`)
- **Limitation**: `reverseSymbol` always assumes USDT quote -- it strips `USDT` and prepends `PERP_`. This means non-USDT pairs would not work correctly.

## Data Precision & Conversion

### Market Precision

- `precision.amount` from `base_tick` field
- `precision.price` from `quote_tick` field
- `limits.amount.min` from `base_min`, `limits.amount.max` from `base_max`
- Max leverage derived from `base_imr` (initial margin requirement): `maxLeverage = Math.floor(1 / imr)`

### Price & Amount Handling

- Prices and amounts adjusted using `adjust()` utility before order placement
- All API values are native decimals (no scaling/conversion needed)
- Balance values rounded to 2 decimal places: `Math.round(value * 100) / 100`

### uPNL Calculation (`fetchOpenPositions`)

- Computed client-side from entry price and mark price (WooX API does not provide uPNL in account endpoint)
- `priceDiff = max(entryPrice, markPrice) - min(entryPrice, markPrice)`
- Direction: `isLoss` if (Long and mark < entry) or (Short and mark > entry)
- `upnl = priceDiff * contracts * (isLoss ? -1 : 1)`, rounded to 2 decimal places

### Balance Calculation (`fetchBalance`)

- `free = freeCollateral` (rounded to cents)
- `total = totalAccountValue` (rounded to cents)
- `used = totalCollateral - freeCollateral` (rounded to cents)
- `upnl` is NOT from the API -- uses the last computed value from `this.store.balance.upnl`
- In `fetchPositionsAndBalance()`, upnl is recalculated from positions and subtracted from `total` since WooX's `totalAccountValue` already includes uPNL

## Rate Limiting

- **Dual rate limits** (applied in sequence via chained `rateLimit()` calls):
  - 4 requests per 1,000ms
  - 20 requests per 6,000ms
- Timeout: 20,000ms (default `RECV_WINDOW`, can be overridden by `options.extra.recvWindow`)
- Retry: 3 retries on network errors (`isNetworkError` condition)
- Params serialized with alphabetical sort (`localeCompare`)

## State Reconciliation

### Public funding rates

After direct public tickers initialize, the exchange owns one 60-second interval for
`GET /v1/public/funding_rates`. Each valid known `PERP_` row maps `est_funding_rate` to
`Ticker.fundingRate` and `next_funding_time` to `Ticker.fundingTime` through `store.updateTicker`,
but only when the raw venue symbol exactly equals the canonical `PERP_BASE_QUOTE` derived from the
matched ticker; lossy normalization aliases are rejected. Updates preserve ticker object identity and
every unrelated field. A request already in flight causes the
next interval to be skipped, so calls never overlap. Transport errors, invalid envelopes, malformed
rows, non-finite numbers, and nonpositive funding timestamps retain the last-known-good values; valid
rows in a mixed response can still advance. External-public-data mode does not start this direct REST
timer. `dispose` clears WOO-owned funding/fill timers and in-flight state before delegating to
`BaseExchange.dispose()` for the store bridge, watchdog, external provider, remote signer, event,
socket, and store cleanup; sockets are not disposed a second time by WOO. An in-flight response checks
disposal before every row and immediately before each store write. The funding cadence is not coupled
to public WebSocket reconnection.

### Executions and trade history

- `fetchExecutions` reads `/v3/trade/transactionHistory`, normalizes both current camelCase and
  legacy snake_case fields, applies symbol/order/time filters, sorts ascending, and caps the result.
- `fetchHistoricalTrades` queries `/v1/client/trades` and then `/v1/client/hist_trades` when needed,
  deduplicating normalized fills across the two legacy windows.
- These routes are read-only reconciliation paths; the V3 migration target for both legacy trade
  lists is `/v3/trade/transactionHistory`.

### Fill-driven optimistic position sync

The constructor attaches `handleFillForPositionSync` to the exchange `fill` event. For one-way
positions, `applyOptimisticOneWayPositionFromFill` immediately adjusts the local position so the UI
does not wait for polling. `scheduleFillPositionRefresh` then performs bounded, signature-aware REST
reconciliation with retry delays. `dispose` removes the fill listener and clears every pending timer,
in-flight marker, and expected signature. Preserve this lifecycle when changing fill or position code.

### Orders

- **Initial state**: Fetched via REST on startup (both limit orders from `/v1/orders` and algo orders from `/v3/algo/orders`)
- **Paginated fetching**: `fetchOrdersRecursively()` handles pagination, fetching with `status: 'INCOMPLETE'` and recursively loading additional pages when `total > records_per_page * page`
- **WebSocket updates**: Execution reports and algo execution reports incrementally update/add/remove orders
- **Periodic refresh**: Orders refetched every 10th tick via REST, completely replacing the store
- **Algo order nesting**: REST responses may have a second `data` wrapper that `fetchAlgoOrders` unwraps before mapping rows

### Positions

- **REST polling**: Fetched every tick via `fetchOpenPositions()`
- **Missing positions**: When `fetchPositions()` is called with specific symbols, creates "fake" zero-contract positions for missing symbols using `fetchPositionMetadata()` which queries per-symbol leverage
- **Full replacement**: Uses `store.addOrUpdatePositionsFull()` on every tick
- **Position mode caching**: `_accountPositionMode` is cached and updated from `fetchBalance()` response (from `/v3/accountinfo`)
- **After method in worker**: `afterMethodCalledInWorker` syncs `_accountPositionMode` after `changePositionMode` is called

### Balance

- **REST polling**: Fetched every 5th tick via `fetchBalance()`
- **uPNL reconciliation**: Since WooX doesn't provide uPNL in the account endpoint, it's computed from positions and patched into the balance object in `fetchPositionsAndBalance()`
- **Account mode validation**: `fetchBalance()` checks `data.accountMode === 'FUTURES'` and throws if not

## Exchange Metadata

### Grafana Monitoring

**Periodic endpoints**: `ACCOUNT`, `BALANCE`, `POSITIONS`, `ALGO_ORDERS`
**Every-call endpoints**: `ALGO_ORDER`, `POSITION`, `UPDATE_ORDER`, `CANCEL_ORDERS`, `CANCEL_ALGO_ORDERS`, `CANCEL_SYMBOL_ORDERS`, `CANCEL_ORDER`, `PLACE_ORDER`, `LEVERAGE`, `POSITION_MODE`, `FUTURES_LEVERAGE`

### Implementation Capabilities

#### Leverage (`setLeverage`)

- Per-symbol leverage via POST to `/v1/client/futures_leverage`
- Always sends `position_side: 'BOTH'` and `margin_mode: 'CROSS'` regardless of actual mode
- The implementation notes that WooX ignores per-side selection and applies both sides
- No per-side leverage support (commented out hedge mode per-side logic)

#### Position Mode (`changePositionMode`)

- POST to `/v1/client/position_mode` with `position_mode: 'HEDGE_MODE' | 'ONE_WAY'`
- Updates cached `_accountPositionMode`
- Refetches positions after change
- Known error: `error_code: 111000` -- "Position mode cannot be altered while there are pending orders"

#### Position Metadata (`fetchPositionMetadata`)

- For symbols without open positions, creates fake position entries with leverage info
- Queries `/v1/client/futures_leverage` per symbol
- Always queries with `position_mode: 'ONE_WAY'` and `margin_mode: 'CROSS'` regardless of actual mode (comment: "woo is fucked and always uses oneway leverage")
- Returns both Long and Short entries for hedge mode

#### Margin Type

- Detected from position `marginMode` field (`CROSS` or `ISOLATED`)
- Isolated margin triggers a warning: "Only cross margin is supported"
- No `changeMarginType()` method implemented

## Testnet / Demo

- **REST base URL**: `https://api.staging.woox.io`
- **Public WS**: `wss://wss.staging.woox.io/ws/stream/<applicationId>`
- **Private WS**: `wss://wss.staging.woox.io/v2/ws/private/stream/<applicationId>`
- **Testnet broker ID**: `e708a644-9ce5-46c6-b50c-74b15f62d8ca` (labeled "safe-cex" in code)
- URLs selected via `options.testnet` boolean
- Kline history endpoint is NOT testnet-specific (always uses `https://api-pub.woox.io`)

## Known Quirks / Implementation Notes

1. **Dual signature methods**: V1 endpoints use `params|timestamp` format, V3 endpoints use `timestamp+METHOD+url+body` format. The request interceptor auto-selects based on URL.
2. **POST_ONLY as order type**: WooX treats `POST_ONLY` as a separate order type rather than a time-in-force value. Code maps PostOnly TIF to `type: 'POST_ONLY'` when placing orders, and maps `POST_ONLY` type back to `OrderTimeInForce.PostOnly` when reading.
3. **V1 body handling**: For V1 POST/DELETE/PUT, all params are moved to the query string and body is set to empty string `''`, except for the `POSITION_MODE` endpoint.
4. **No uPNL in account endpoint**: WooX does not provide unrealized PnL in the account endpoint. uPNL is computed from positions polling and added to balance. This causes a timing issue where `fetchBalance()` alone returns stale `upnl`.
5. **Historical kline from separate domain**: Historical legacy kline data uses `https://api-pub.woox.io` via `_proxyEnforcedXhr`, while recent kline data uses the main API. `fetchOHLCV` selects it when the requested candle is more than 100 intervals old.
6. **Kline secondary trades topic**: For sub-minute/sub-second candle intervals, `listenOHLCV` subscribes to both `@kline` and `@trades`. Trade data constructs real-time candles when the kline topic is too sparse. The trades topic is used only outside `publicDataOnly` mode.
7. **Positional TP/SL**: TP/SL are placed as `POSITIONAL_TP_SL` algo orders with `childOrders` array containing `STOP_LOSS` and/or `TAKE_PROFIT` children with `type: 'CLOSE_POSITION'`.
8. **Sibling order management**: Directly cancelling one side re-creates the retained sibling. Replacing a TP or SL calls the cancel-only variant and then places one combined bracket, preventing a stale duplicate sibling.
9. **Partial TP/SL (STOP type)**: When a TP/SL order has a specific amount (not full position), it uses `algoType: 'STOP'` with `type: 'MARKET'` instead of `POSITIONAL_TP_SL`.
10. **Trailing stop**: Uses `callbackValue` (price distance from last to stop) and uses the full position size. Uses cancel-and-replace for updates.
11. **Paginated order fetching**: `fetchOrdersRecursively` handles pagination for both limit and algo orders, recursively fetching until all pages are loaded.
12. **Large order splitting**: Orders exceeding `market.limits.amount.max` are automatically split into multiple lots.
13. **Update order supported for limits**: Direct PUT to `/v3/order/{orderId}` for limit orders (no cancel-and-replace needed). Trailing stops use cancel-and-replace. Algo orders use PUT to `/v3/algo/order/{parentId}`.
14. **Public data only mode**: If `publicDataOnly` is true or no API keys, skips private WS and sets all private data as loaded.
15. **Typo in class name**: Private WebSocket class is named `WooPrivateWebscoket` (missing 'k' in "Socket").
16. **Leverage always sends BOTH/CROSS**: Regardless of actual position mode, leverage requests always use `position_side: 'BOTH'` and `margin_mode: 'CROSS'` due to WooX API behavior.
17. **Algo order response nesting**: Algo order fetch and placement responses carry nested `data`; placement IDs come from `data.rows[].orderId`.
18. **Cancel already-canceled handling**: `cancelAlgoOrder()` silently succeeds when the error message includes "already canceled".
19. **Proxy for non-base URLs**: Both V1 and V3 sign functions handle proxy URL injection for URLs starting with `http` (non-base URL, like kline history) via `getProxyForAxios()`.

## Error Handling

### REST API Errors

- **Success field**: API responses include `success: boolean`. Non-success responses trigger error handling.
- **Error message extraction**: Most error handlers check `err?.response?.data?.message` (WooX uses `message` not `msg`)
- **Network retries**: 3 retries on network errors
- **Error propagation**: Most methods catch errors, log via `emitAndLogError()`, and either return fallback data or re-throw

### Specific Error Patterns

- **Account mode check** (`fetchBalance`): Throws `"WOO X account is in ${mode} mode, only FUTURES mode is supported"` if not in FUTURES mode
- **Isolated margin warning** (`fetchOpenPositions`): Emits an error when an isolated-margin position is detected
- **Position mode change** (`changePositionMode`): Error code `111000` / code `-1005` means "Position mode cannot be altered while there are pending orders"
- **Cancel already-canceled** (`cancelAlgoOrder`): Silently handles "already canceled" by updating store status instead of throwing
- **Failed positions fetch** (`fetchOpenPositions`): Checks `success` and throws with `data.message` if false

### Validation

- `validateAccount()` - Tests both primary and secondary API keys (if configured) by calling `/v3/accountinfo`. On failure it prefers WooX's `message` response field, then the legacy `msg` and generic error fallbacks.
- `getAccount()` - Returns `applicationId` and `referrerID` from account info.

## Testing / Validation

No exchange-specific test files exist in this directory. Validation is done through:

- `validateAccount()` - Tests API key validity against `/v3/accountinfo`
- Account mode check in `fetchBalance()` ensures FUTURES mode
- Position metadata queries validate per-symbol leverage configuration
