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
- **Legacy API documentation** (for migration work): https://docs.woox.io/#restful-api

## Legacy API lifecycle

WooX scheduled both the legacy REST and WebSocket surfaces used here for deprecation on December 31,
2025 and describes them as no longer actively maintained. Do not add new legacy dependencies. The
phased endpoint inventory, parity gates, rollback strategy, and monitoring requirements live in
`docs/safe_cex_audit_july/woo-v3-migration-plan.md`. Preview and prod are both production lanes, so
write-path migration requires fixture coverage and a credentialed WooX demo lifecycle smoke first.

The V3 foundation is deliberately non-routable: `woo.v3.contracts.ts` holds the typed route map,
response/request contracts, and seven hard-false migration gates, while `woo.v3.request.ts` only
prepares deterministic transport-neutral request descriptors. Neither file is imported by
`woo.api.ts`, `woo.exchange.ts`, or either websocket implementation, and neither owns an Axios,
fetch, or websocket client. Enabling any gate or wiring these modules into runtime is a separate
review requiring the corresponding parity/demo evidence. Concrete endpoint pages and the official
migration table use `/v3/trade/*`; do not follow the introduction page's inconsistent `/trading`
label.

## CCXT Reference Implementation

- **REST API**: https://github.com/ccxt/ccxt/blob/master/ts/src/woo.ts
- **WebSocket**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/woo.ts

## File Structure

- `woo.api.ts` - REST API client with dual V1/V3 HMAC-SHA256 signature methods
- `woo.exchange.ts` - Main exchange class (`WOOXExchange`) extending `BaseExchange`
- `woo.types.ts` - TypeScript type definitions, endpoint constants, type maps
- `woo.v3.contracts.ts` - Non-routable namespaced V3 endpoints, contracts, migration map, and gates
- `woo.v3.request.ts` - Pure V3 request normalization/signing descriptor (no transport or I/O)
- `woo.utils.ts` - Symbol normalization utilities (`PERP_BTC_USDT` <-> `BTCUSDT`), memoized
- `woo.ws-private.ts` - Private WebSocket (HMAC auth, execution reports, algo execution reports)
- `woo.ws-public.ts` - Public WebSocket (tickers, BBOs, mark prices, funding rates, klines, orderbook)

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
2. **Parse tickers**: After both resolve, `parseTickers()` converts raw data, stores tickers, sets `loaded.tickers = true`
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
| `/v3/balances`                          | v3      | GET         | Detailed balances               |
| `/v3/positions`                         | v3      | GET         | Open positions                  |
| `/v3/position/{symbol}`                 | v3      | GET         | Single position                 |
| `/v3/order/{orderId}`                   | v3      | PUT         | Update limit order              |
| `/v3/algo/orders`                       | v3      | GET         | Algo orders (paginated)         |
| `/v3/algo/order/{id}`                   | v3      | PUT/DELETE  | Update / cancel algo order      |
| `/v3/trade/transactionHistory`          | v3      | GET         | Execution history               |
| `/v3/orders/pending`                    | v3      | DELETE      | Cancel all pending orders       |
| `/v3/algo/orders/pending`               | v3      | DELETE      | Cancel all pending algo orders  |
| `/v1/public/info`                       | v1      | GET         | Markets/symbol info             |
| `/v1/public/futures`                    | v1      | GET         | Futures tickers                 |
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

**On connect**, subscribes to four global topics immediately:

1. `tickers` - Last price, volume, quote volume for all symbols
2. `bbos` - Best bid/offer for all symbols
3. `markprices` - Mark prices for all symbols
4. `estfundingrate` - Estimated funding rates and next funding timestamp

Then re-subscribes to any previously active on-demand topics (klines, orderbooks) and resets orderbook state.

**Ping/pong**: Sends `{ "event": "ping" }`. Responds to server-initiated pings with `{ "event": "pong" }` in `handlePingEvent`. Pong receipt triggers `handlePongEvent()` from `BaseWebSocket`, which schedules the next ping after 5 seconds.

**Reconnection**: On reconnect, emits `emitWsReconnected()` if `ENABLE_RECONNECTING_WEBSOCKET` is enabled.

**Message routing**: Uses string matching on raw data (`data.includes(...)`) to route messages to handlers. Matches against both `event":"<topic>` and `topic":"<topic>` patterns. First matching handler wins (break after match).

**Ticker data handling**: Filters to `PERP_` symbols only. Updates are split across multiple topics:

- `tickers`: Updates `last`, `volume`, `quoteVolume`
- `bbos`: Updates `bid`, `ask`
- `markprices`: Updates `mark`
- `estfundingrate`: Updates `fundingRate`, `fundingTime`

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
