# Arkham Exchange Integration

This directory contains the implementation for Arkham exchange integration in the safe-cex library.

## Availability

**The legacy Arkham CEX integration is disabled in production, development, and
preview.** The legacy `arkm.com/api` endpoints no longer serve the CEX API, and
Arkham's current trading product is decentralized Solana trading. Do not
re-enable the metadata flags until a replacement trading API and its REST and
WebSocket payloads have been independently validated.

The frozen ticker-update defect is intentionally left unfixed while the venue
is disabled: its legacy WebSocket payload cannot be verified against a live
CEX frame. Before any re-enablement, verify the current protocol and then fix
`ArkhamPublicWebSocket.handleTickerUpdate` together with stable ticker IDs in
`ArkhamExchange.fetchTickers`.

## Exchange Information

Arkham (ARKM) is a cryptocurrency exchange offering:

- Spot and perpetual futures trading
- Dynamic leverage through margin schedules (not traditional fixed leverage)
- Tiered fee structure based on trading volume and ARKM token holdings
- Advanced order types including trigger orders (stop loss / take profit)
- HMAC-SHA256 API authentication with base64-encoded secrets

## API Documentation

**Official Documentation**: https://arkm.com/docs

### Key Endpoints

- **REST API Base URL**:
  - Production: `https://arkm.com/api`
  - Testnet: Not available (testnet URL is set to production)

- **WebSocket URL**:
  - Direct: `wss://arkm.com/ws` (unified endpoint, both public and private)
  - Proxy (used in implementation): `wss://fsjnpy143pbz.jp.tealstreet.io/arkm`

### Important API Documentation Sections

1. **API Quick Start**: https://arkm.com/docs#quick-start
2. **Authentication**: https://arkm.com/docs#authentication
3. **REST API Reference**: https://arkm.com/docs#rest-api
4. **WebSocket API**: https://arkm.com/docs#websocket-section
5. **Rate Limits**: https://arkm.com/docs#rate-limits
6. **Order Types**: https://arkm.com/docs#order-types

### Historically Verified Endpoint Corrections

- **User Info**: `/user` (not `/account/info`)
- **Markets**: `/public/pairs` (not `/public/markets`)
- **Active Orders**: `/orders` (not `/orders/active`)
- **Cancel All**: `/orders/cancel/all` (not `/orders/cancel-all`)

## File Structure

```
arkham/
├── arkham.exchange.ts       # Main exchange class (ArkhamExchange)
├── arkham.api.ts             # REST API client with HMAC auth (createAPI)
├── arkham.types.ts           # TypeScript types, endpoints, response types, WS types
├── arkham.ws-public.ts       # Public WebSocket (ArkhamPublicWebSocket)
├── arkham.ws-private.ts      # Private WebSocket (ArkhamPrivateWebSocket)
├── cancel-old-orders.ts      # Standalone utility script to cancel all open orders
├── cancel-order-by-id.ts     # Standalone utility script to cancel a specific order
└── CLAUDE.md                 # This documentation
```

## Authentication

Arkham uses HMAC SHA-256 authentication for all private endpoints.

### Signature Generation (`arkham.api.ts:16-62`)

1. **Timestamp**: `Date.now() * 1000` (current time in microseconds)
2. **Expiry**: `timestamp + 5 * 1000000` (5 seconds in the future, in microseconds)
   - Must not be more than 15 minutes in the future
   - Must not be in the past
3. **Message format**: `{api_key}{expiry}{METHOD}{path}{body}`
   - `path` = URL path, optionally includes query string (appended if params exist)
   - `body` = JSON stringified body for POST requests, empty string for GET
   - **Important**: Query strings ARE included in the signed path (contrary to earlier docs), as seen in `arkham.api.ts:26-31`
4. **Signature process**:
   - Decode API secret from base64: `Buffer.from(options.secret, 'base64')`
   - Create HMAC-SHA256 using decoded secret directly
   - Output as base64: `.digest('base64')`
5. **Headers**:
   - `Arkham-Api-Key`: API key (note: capital 'A' in Api, not 'API')
   - `Arkham-Signature`: Base64-encoded HMAC signature
   - `Arkham-Expires`: Expiry timestamp as string (microseconds)
   - `Arkham-Broker-Id`: `'1002'` (required for all requests)

### Public Endpoints

Endpoints containing `/public/` are skipped by the signing interceptor (`arkham.api.ts:96-97`). All other endpoints require authentication.

### WebSocket Authentication

Both public and private WebSocket classes authenticate through the proxy server (`arkham.ws-public.ts:166-185`, `arkham.ws-private.ts:323-342`):

1. Generate signature for path `/ws` using the same HMAC process as REST
2. Send auth message: `{ type: 'auth', apikey, expires, signature }`
3. Wait for `{ type: 'auth_success' }` response before subscribing to channels

**Note**: The implementation does NOT connect directly to `wss://arkm.com/ws`. Instead, it uses a Tealstreet proxy at `wss://fsjnpy143pbz.jp.tealstreet.io/arkm` that handles the actual connection to Arkham.

## Startup Sequence

The `start()` method (`arkham.exchange.ts:108-144`) executes:

1. **Fetch markets** via `fetchMarkets()` — `GET /public/pairs`
   - Filters to `status === 'listed'` markets only
   - Maps `pairType: 'perp'` to swap/contract type, `'spot'` to spot type
   - Checks `this.isDisposed` after each async step
   - Updates store: `{ markets, loaded.markets: true }`

2. **Fetch tickers** via `fetchTickers()` — `GET /public/tickers`
   - Calculates 24h price change percentage from `price` and `price24hAgo`
   - Includes funding rate, mark price, index price, open interest
   - Updates store: `{ tickers, loaded.tickers: true }`

3. **Connect WebSockets** (parallel with step 4):
   - `this.publicWebsocket.connectAndSubscribe()` — subscribes to tickers for all markets
   - `this.privateWebsocket.connectAndSubscribe()` — subscribes to order_statuses, trigger_orders, positions, balances

4. **Start periodic tick** via `tick()` — first execution is awaited
   - Fetches positions, balance, orders (see Tick section)
   - Schedules subsequent ticks via `loop(() => this.tick(), 10_000)` (10-second interval)

5. **Fetch initial orders** via `fetchOrders()`
   - Fetches regular orders (`GET /orders`) and trigger orders (`GET /trigger-orders`) in parallel
   - Filters regular orders to active statuses: `new`, `booked`, `maker`, `taker`
   - Filters trigger orders to `staged` status only
   - Updates store: `{ orders, loaded.orders: true }`

### Tick Cycle (`arkham.exchange.ts:146-185`)

Runs every 10 seconds:

1. Fetch positions — calculates leverage from `value / initialMargin`
2. Fetch balance — sums USDT/USDC/USD balances
3. Calculate `upnl` from positions, calculate `used` margin from `initialMargin`
4. Fetch orders (regular + trigger orders)
5. On error, logs and continues

## REST API Endpoints

All endpoints defined in `ENDPOINTS` (`arkham.types.ts:16-47`):

### Account Endpoints (Private)

| Endpoint     | Method | Path                 | Description               |
| ------------ | ------ | -------------------- | ------------------------- |
| Account Info | GET    | `/user`              | User info, tier, fees     |
| Balances     | GET    | `/account/balances`  | All asset balances        |
| Positions    | GET    | `/account/positions` | Open positions            |
| Leverage     | GET    | `/account/leverage`  | Current leverage settings |
| Set Leverage | POST   | `/account/leverage`  | Set leverage for symbol   |

### Public Endpoints

| Endpoint   | Method | Path              | Description          |
| ---------- | ------ | ----------------- | -------------------- |
| Markets    | GET    | `/public/pairs`   | All trading pairs    |
| Tickers    | GET    | `/public/tickers` | All ticker data      |
| Ticker     | GET    | `/public/ticker`  | Single ticker        |
| Order Book | GET    | `/public/book`    | L2 order book        |
| Trades     | GET    | `/public/trades`  | Recent public trades |
| Candles    | GET    | `/public/candles` | OHLCV data           |

### Trading Endpoints (Private)

| Endpoint             | Method | Path                         | Description           |
| -------------------- | ------ | ---------------------------- | --------------------- |
| Place Order          | POST   | `/orders/new`                | New regular order     |
| Place Trigger Order  | POST   | `/trigger-orders/new`        | New stop/TP order     |
| Cancel Order         | POST   | `/orders/cancel`             | Cancel single order   |
| Cancel Trigger Order | POST   | `/trigger-orders/cancel`     | Cancel trigger order  |
| Cancel All Orders    | POST   | `/orders/cancel/all`         | Cancel all orders     |
| Cancel All Triggers  | POST   | `/trigger-orders/cancel/all` | Cancel all triggers   |
| List Orders          | GET    | `/orders`                    | Open orders           |
| List Trigger Orders  | GET    | `/trigger-orders`            | Active trigger orders |

## WebSocket Implementation

### Connection Architecture

Both public and private WebSockets connect through a Tealstreet proxy server:

- **Proxy URL**: `wss://fsjnpy143pbz.jp.tealstreet.io/arkm`
- **Native URL** (not used directly): `wss://arkm.com/ws`

The proxy handles the actual Arkham WebSocket connection. Authentication goes through the proxy first (`type: 'auth'`), which then authenticates with Arkham on behalf of the client.

### Public WebSocket (`arkham.ws-public.ts`)

**Message handlers:**

| Channel      | Handler                 | Throttle | Description                 |
| ------------ | ----------------------- | -------- | --------------------------- |
| `ticker`     | `handleTickerUpdate`    | 1000ms   | Price/volume updates        |
| `l2_updates` | `handleOrderbookUpdate` | 100ms    | Incremental L2 book updates |
| `trades`     | `handleTradesUpdate`    | none     | Public trade feed           |
| `candles`    | `handleKlineUpdate`     | none     | Candlestick data            |
| `auth`       | `handleAuthResponse`    | none     | Auth confirmation           |

**Subscription flow:**

1. On connect, send proxy auth message
2. On `auth_success`, call `subscribeToChannels()`:
   - Subscribes to `ticker` for every market in the store
   - Resubscribes to any active orderbook subscriptions (tracked via `activeOrderbookSymbols` Map with reference counting)
   - Resubscribes to any active candle subscriptions (tracked via `activeCandleSubscriptions` Map)

**Subscription message format:**

```json
{
  "method": "subscribe",
  "args": {
    "channel": "l2_updates",
    "params": { "symbol": "BTC_USDT" }
  }
}
```

**Order book state management (`listenOrderBook`, `arkham.ws-public.ts:314-443`):**

- Maintains local state as `bidsMap` and `asksMap` (`Map<string, number>`)
- Processes incremental L2 updates: `size === 0` removes level, else adds/updates
- Filters unreasonable prices: bids <= 1.2x last price, asks >= 0.8x last price
- Sorts and calculates totals via `sortOrderBook()` / `calcOrderBookTotal()`
- Reference-counted subscriptions: only unsubscribes when last listener removed
- Waits for connection with polling (`setTimeout` 100ms retry loop)

**OHLCV listener (`listenOHLCV`, `arkham.ws-public.ts:445-554`):**

- Subscribes to `candles` channel with `symbol` and `duration` params
- Reference-counted like orderbook subscriptions
- Timestamps converted from microseconds to milliseconds: `Math.floor(data.time / 1000)`

### Private WebSocket (`arkham.ws-private.ts`)

**Message handlers:**

| Channel          | Handler                    | Throttle | Description           |
| ---------------- | -------------------------- | -------- | --------------------- |
| `order_statuses` | `handleOrderUpdate`        | 100ms    | Order status changes  |
| `trigger_orders` | `handleTriggerOrderUpdate` | none     | Trigger order changes |
| `positions`      | `handlePositionUpdate`     | 100ms    | Position changes      |
| `balances`       | `handleBalanceUpdate`      | 1000ms   | Balance updates       |
| `auth`           | `handleAuthResponse`       | none     | Auth confirmation     |

**Private channel subscriptions** (sent after auth success, `arkham.ws-private.ts:344-384`):

- `order_statuses`
- `trigger_orders`
- `positions`
- `balances`

**Throttled REST fetch pattern:**

Both order and position WS handlers use a custom throttle mechanism (not lodash throttle) to batch REST fetches:

1. WS event arrives, handler updates store immediately via `addOrUpdateOrder()` / `addOrUpdatePosition()`
2. Sets `hasPendingOrderEvents = true` / `hasPendingPositionEvents = true`
3. Calls `throttledFetchOrders()` / `throttledFetchPositions()`
4. Throttle checks time since last fetch against `orderFetchThrottleMs` / `positionFetchThrottleMs` (both 250ms)
5. If enough time passed, fetches immediately; otherwise schedules via `setTimeout`
6. This ensures the full state snapshot is fetched but not more than once per 250ms

**Balance update filtering (`arkham.ws-private.ts:271-311`):**

- Only processes USD-denominated balances (USDT, USDC, USD)
- Ignores non-USD token updates (e.g., ARKM token balance changes) to prevent zeroing out the balance

### Ping/Pong

- Sends `{ type: 'ping' }` every 30 seconds (both public and private)
- Uses recursive `setTimeout` pattern
- Starts after successful proxy auth (`auth_success`)

### Reconnection

Both WebSocket classes inherit `onClose` from `BaseWebSocket`, which handles:

- Exponential backoff with `WS_MIN_RECONNECTION_DELAY`
- Automatic resubscription on reconnect (via `subscribeToChannels()` / `subscribeToPrivateChannels()`)
- Active subscription tracking with reference counting for seamless reconnection

## Order Types and Mappings

### Order Type Mapping (`arkham.types.ts:49-54`)

| Arkham API | Internal (`OrderType`) |
| ---------- | ---------------------- |
| `limitGtc` | `OrderType.Limit`      |
| `limitIoc` | `OrderType.Limit`      |
| `limitFok` | `OrderType.Limit`      |
| `market`   | `OrderType.Market`     |

### Internal to Arkham (`arkham.exchange.ts:990-998`)

| Internal (`OrderType`) | Time in Force | Arkham API   |
| ---------------------- | ------------- | ------------ |
| `OrderType.Market`     | any           | `'market'`   |
| `OrderType.Limit`      | `'IOC'`       | `'limitIoc'` |
| `OrderType.Limit`      | `'FOK'`       | `'limitFok'` |
| `OrderType.Limit`      | default       | `'limitGtc'` |

### Order Side Mapping (`arkham.types.ts:56-59`)

| Arkham API | Internal (`OrderSide`) |
| ---------- | ---------------------- |
| `'buy'`    | `OrderSide.Buy`        |
| `'sell'`   | `OrderSide.Sell`       |

### Order Status Mapping (`arkham.exchange.ts:1010-1019`)

| Arkham API    | Internal (`OrderStatus`) |
| ------------- | ------------------------ |
| `'new'`       | `OrderStatus.Open`       |
| `'taker'`     | `OrderStatus.Open`       |
| `'booked'`    | `OrderStatus.Open`       |
| `'maker'`     | `OrderStatus.Open`       |
| `'cancelled'` | `OrderStatus.Canceled`   |
| `'closed'`    | `OrderStatus.Closed`     |

### Trigger Order Status Mapping (`arkham.ws-private.ts:398-406`)

| Arkham API    | Internal (`OrderStatus`) |
| ------------- | ------------------------ |
| `'staged'`    | `OrderStatus.Open`       |
| `'triggered'` | `OrderStatus.Closed`     |
| `'cancelled'` | `OrderStatus.Canceled`   |
| `'failed'`    | `OrderStatus.Canceled`   |

## Order Lifecycle

### Regular Orders (`arkham.exchange.ts:573-718`)

1. Resolve market for symbol, get precision values
2. Adjust amount and price using `adjust()` with market precision
3. Determine order type string from `mapOrderType()` (e.g., `'limitGtc'`)
4. Build payload: `{ symbol, side, type, size, price, reduceOnly }`
   - **reduceOnly only for market orders** (`arkham.exchange.ts:700-701`): Arkham rejects `reduceOnly` on limit orders
5. POST to `/orders/new`
6. Return `[data.orderId.toString()]`

### Trigger Orders (Stop Loss / Take Profit) (`arkham.exchange.ts:576-694`)

Arkham uses a separate endpoint for trigger orders:

1. Determine if this is a trigger order: `opts.type === OrderType.StopLoss || OrderType.TakeProfit`
2. **Determine correct `triggerType`** based on position entry price vs trigger price:
   - Find existing position for the symbol
   - Compare trigger price with entry price (or mark price if no position)
   - **CRITICAL**: Arkham's trigger type semantics are **inverted** from what you'd expect:
     - `triggerType='stopLoss'` triggers when price goes **DOWN** (below trigger price)
     - `triggerType='takeProfit'` triggers when price goes **UP** (above trigger price)
   - So for closing a **long** position:
     - SL (sell lower): `triggerType = 'stopLoss'`
     - TP (sell higher): `triggerType = 'takeProfit'`
   - For closing a **short** position:
     - SL (buy higher): `triggerType = 'takeProfit'`
     - TP (buy lower): `triggerType = 'stopLoss'`
3. Get `triggerPriceType` from opts, default to `'mark'` (`arkham.exchange.ts:1022-1036`)
4. Build payload: `{ symbol, side, type: 'market', size, triggerPrice, triggerType, triggerPriceType, reduceOnly }`
5. POST to `/trigger-orders/new`
6. Return `[data.triggerOrderId.toString()]`

### Updating Orders (`arkham.exchange.ts:734-764`)

Arkham does not support order modifications. Cancel-and-replace:

1. Cancel existing order via `cancelOrder(opts.order.id)`
2. Place new order with updated parameters

### Canceling Orders

- **Single regular**: POST `/orders/cancel` with `{ orderId: numericId }` — orderId must be numeric (`parseInt(orderId, 10)`)
- **Single trigger**: POST `/trigger-orders/cancel` with `{ triggerOrderId: numericId, symbol }` — requires symbol
- **Type detection**: Checks store for order type to determine which cancel endpoint to use (`arkham.exchange.ts:766-793`)
- **All orders**: POST `/orders/cancel/all` — can optionally include `{ symbol }` to cancel per-market
- **Batch cancel**: Uses `forEachSeries()` to cancel one at a time sequentially (`arkham.exchange.ts:795-803`)

### Batch Order Placement (`arkham.exchange.ts:721-732`)

Uses `forEachSeries()` (sequential, not parallel) to place orders one at a time. Errors are caught per-order and logged, allowing remaining orders to continue.

## Symbol Format

Arkham uses underscore-separated format:

- **Spot**: `BTC_USDT`, `ETH_USDT`, `ARKM_USDT`
- **Perpetual**: `BTC_USDT_PERP`, `ETH_USDT_PERP`, `SOL_USDT_PERP` — quote is **retained** before the `_PERP` suffix (verified against the API and `packages/safe-cex/src/utils/__testdata__/market-samples.json`).

Symbols are used as-is (no transformation needed). The `market.id` equals `market.symbol`.

**Do not simplify to `BTC_PERP`** — that was an earlier misconception based on
stale fixtures that did not reflect the live API.

## Data Precision & Conversion

### Market Precision (`arkham.exchange.ts:258-262`)

- `precision.amount`: From `minLotSize` or `minSize` (parsed as float)
- `precision.price`: From `minTickPrice` (parsed as float)
- `precision.base` / `precision.quote`: Hardcoded to `8`

### Timestamp Format

Arkham uses **microseconds** for all timestamps:

- REST response timestamps: microseconds (e.g., `time: 1761468361632561`)
- Converted to milliseconds for internal use: `Math.floor(time / 1000)` (`arkham.exchange.ts:938`)
- Auth expiry: microseconds (`Date.now() * 1000 + 5_000_000`)
- OHLCV `start`/`end` params: microseconds (`opts.from * 1000`, `opts.to * 1000`) (`arkham.exchange.ts:383-403`)

### OHLCV Specifics (`arkham.exchange.ts:357-430`)

- Uses `duration` parameter (not `interval`): e.g., `'1m'`, `'5m'`, `'1h'`, `'1d'`
- Maximum 365 candles per request
- Response timestamps in microseconds, converted to ms for Candle output
- Start/end params in microseconds
- If range exceeds 365 candles, logs warning but still makes the request

### Balance Handling (`arkham.exchange.ts:187-221`)

- API returns array of all asset balances
- Filters to USD-denominated: `USDT`, `USDC`, `USD`
- Sums `balance` (total) and `free` fields
- `used` = `total - free`

### Position Value Calculation (`arkham.exchange.ts:482-516`)

- `contracts` = `Math.abs(baseAmount)` (absolute value of `p.base`)
- Side: positive `base` = Long, negative = Short
- Leverage: `value / initialMargin` (dynamic, calculated from position data)
- Notional: `Math.abs(p.quote)`
- Entry price: `p.averageEntryPrice`
- Liquidation price: `0` (Arkham doesn't provide this directly)

## Rate Limiting

### REST API Limits (Tiered)

| Tier | 30-Day Volume (USD) | ARKM Balance | Spot Orders/sec | Perp Orders/sec |
| ---- | ------------------- | ------------ | --------------- | --------------- |
| 1    | < $1M               | < 1,000      | 20              | 40              |
| 2    | $1M - $5M           | 1,000+       | 50              | 100             |
| 3    | $5M - $20M          | 5,000+       | 100             | 500             |
| 4    | $20M - $100M        | 20,000+      | 200             | 2,000           |
| 5    | > $100M             | 100,000+     | 300             | 5,000           |

### Implementation

Uses `axios-rate-limit` with default 20 RPS (lowest tier): `rateLimit(createAPI(opts, this), { maxRPS: 20 })` (`arkham.exchange.ts:68`).

### Retry Strategy

Uses `axios-retry` (`arkham.api.ts:81-85`):

- Retries: 3
- Condition: Network errors only (`isNetworkError`)
- Delay: Linear backoff (`retryCount * 1000` ms)

### HTTP-over-WebSocket Adapter

When not in test environment, the API client uses `axiosHttpOverWebsocketAdapter` for HTTP requests (`arkham.api.ts:88-91`). This routes REST API calls through a WebSocket tunnel for lower latency.

### WebSocket Limits

- Connections per IP: 10
- Subscriptions per connection: 200
- Messages per second: 100

## State Reconciliation

### Position Merging (`arkham.exchange.ts:470-547`)

1. Fetch positions from REST API (`GET /account/positions`)
2. Parse real positions: determine side from `base` sign, calculate leverage
3. Create "fake" positions for all non-spot markets (one-way mode convention):
   - Zero-contract Long positions with stored leverage
   - Ensures every market has a position entry in the store
4. Merge using `uniqBy([...parsedPositions, ...fakePositions], 'id')` — real positions take precedence

**Leverage resolution order** (`arkham.exchange.ts:490-499`):

1. Calculate from `value / initialMargin`
2. If invalid: use `store.fetchedLeverage[symbol]`
3. If not available: use `this.defaultLeverage` (starts at 1)

### Order Merging

WebSocket updates provide immediate optimistic updates via `store.addOrUpdateOrder()`, followed by throttled REST fetch (250ms throttle) for authoritative state. Both regular and trigger orders are merged into a single `orders` array.

### Balance with Margin

Balance `used` is calculated from positions' `initialMargin` in `tick()` (`arkham.exchange.ts:160-169`):

```
usedMargin = sum of all positions' info.initialMargin
free = total - usedMargin
```

Private WebSocket position updates also recalculate and update balance margin (`arkham.ws-private.ts:243-258`).

## Exchange Metadata

### Leverage System

**Arkham uses dynamic margin schedules, NOT traditional fixed leverage:**

- Users set position limits via `/account/leverage` endpoint, not fixed leverage
- Actual leverage is automatically determined by position size
- Seven margin schedules (A through G) with progressive position bands
- Larger positions require higher margins (lower effective leverage)

**API methods:**

- `setLeverage(symbol, leverage)` (`arkham.exchange.ts:832-862`): POSTs to `/account/leverage`, updates `defaultLeverage` and `store.fetchedLeverage`
- `fetchLeverageForSymbol(symbol)` (`arkham.exchange.ts:864-918`): GETs from `/account/leverage`, falls back to `defaultLeverage` if empty response
- `changeMarginType()` (`arkham.exchange.ts:823-830`): No-op, logs warning about dynamic margin system

### Position Mode

Always `PositionMode.OneWay` (`arkham.exchange.ts:80-83`). No hedge mode support.

## Testnet/Demo

**No testnet is available.** Both livenet and testnet URLs point to production (`arkham.types.ts:5-8`):

```typescript
export const BASE_URL = {
  livenet: 'https://arkm.com/api',
  testnet: 'https://arkm.com/api', // No testnet available yet
};
```

All testing must be done on production with real funds. Use small order sizes (e.g., 0.0001 BTC minimum).

## Known Quirks / Implementation Notes

### Order IDs Must Be Numeric

Arkham's API expects `orderId` as a number, not a string. The implementation converts with `parseInt(orderId, 10)` before sending to the API (`arkham.exchange.ts:773`). Using a string causes: `"invalid type for field orderId, expected common.OrderId but got string"`.

### Trigger Order Cancel Requires Symbol

Unlike regular order cancellation (just orderId), trigger order cancellation also requires the symbol parameter (`arkham.exchange.ts:781-784`).

### No Order Modification

`updateOrder()` is cancel-and-replace (`arkham.exchange.ts:734-764`). There is no native order amendment endpoint.

### reduceOnly Only for Market Orders

Arkham rejects `reduceOnly` on limit orders. The implementation only includes `reduceOnly` when `opts.type === OrderType.Market` (`arkham.exchange.ts:700-701`).

### Trigger Type Semantics Inverted from CCXT

Arkham's `triggerType='stopLoss'` means "trigger when price goes DOWN", and `triggerType='takeProfit'` means "trigger when price goes UP". This is the opposite of what many other exchanges use. The implementation handles this inversion automatically based on position entry price vs trigger price (`arkham.exchange.ts:604-669`).

### Proxy WebSocket Server

The implementation does NOT connect directly to `wss://arkm.com/ws`. Both public and private WebSockets connect to `wss://fsjnpy143pbz.jp.tealstreet.io/arkm` (a Tealstreet proxy). Auth is done through the proxy with a custom `{ type: 'auth', apikey, expires, signature }` message format.

### GET Requests Strip Body

The `signRequest()` function removes `data` from GET/HEAD requests to prevent body errors (`arkham.api.ts:56-59`).

### Console.log Statements

Several methods contain `console.log` statements for debugging (e.g., `[LEVERAGE]`, `[new order]`). These are production debug logs, not temporary code (`arkham.exchange.ts:743, 887, 904`).

### Batch Operations are Sequential

Both `placeOrders()` and `cancelOrders()` use `forEachSeries()` from `p-iteration` to process orders one at a time, not in parallel. This avoids rate limit issues but is slower for large batches.

### Ticker `id` Includes Timestamp

Ticker IDs are generated as `${symbol}-${Date.now()}` (`arkham.exchange.ts:324`), making them unique per fetch. This differs from most exchanges that use a stable ID.

### Balance Filters USD Only

The `fetchBalance()` method only considers USDT, USDC, and USD balances (`arkham.exchange.ts:205`). Non-USD crypto balances (like ARKM tokens) are excluded from the total.

### Orderbook Price Filtering

The `listenOrderBook` handler filters orderbook entries to within 20% of last price (`arkham.ws-public.ts:377-379`): asks >= 0.8x last price, bids <= 1.2x last price. This prevents stale or erroneous price levels from appearing.

## Error Handling

### Response Interceptor (`arkham.api.ts:107-134`)

**200-with-error pattern**: Arkham sometimes returns errors with HTTP 200. The interceptor checks for responses containing `{ id, name, message }` structure and converts them to errors: `"Arkham Error [{name}]: {message}"`. Also calls `exchange.emitAndLogError()`.

**HTTP error responses**: Extracts `error.message`, `error.code`, or raw error from `error.response.data.error` and enhances the error message with `"Arkham Error: {message}"`.

### Specific Error Codes

| Code | Description                              |
| ---- | ---------------------------------------- |
| 400  | Bad Request - Invalid parameters         |
| 401  | Unauthorized - Invalid API credentials   |
| 403  | Forbidden - Insufficient permissions     |
| 404  | Not Found - Invalid endpoint or resource |
| 429  | Rate Limit Exceeded                      |
| 500  | Internal Server Error                    |
| 503  | Service Unavailable                      |

### WebSocket Error Handling

- **Proxy errors**: `{ type: 'error', error: ... }` logged as `LogSeverity.Error`
- **Arkham errors**: `{ error: { code, id, name, message } }` logged with name and message
- Known WebSocket error codes from testing: `ParsingRequest` (id: 10022), `InvalidMethod` (id: 20001)

### Graceful Degradation

- `fetchMarkets()` and `fetchTickers()` return empty arrays on error (`arkham.exchange.ts:299, 353`)
- `tick()` catches all errors and continues polling (`arkham.exchange.ts:179-181`)
- `validateAccount()` returns error string instead of throwing (`arkham.exchange.ts:98-106`)
- Batch operations (placeOrders, cancelOrders) catch per-item and continue

## Testing / Validation

### Utility Scripts

- `cancel-old-orders.ts`: Standalone script to cancel all open orders. Run with `tsx`.
- `cancel-order-by-id.ts`: Cancel a specific order by numeric ID. Run with `tsx`.

### Key Validated Behaviors

- `orderId` must be numeric (not string) in cancel requests
- Order type must be `limitGtc` (not `limit`)
- Cancel endpoint is POST, not DELETE
- Minimum BTC size: 0.0001, minimum notional: $1
- Balance API returns array directly (not `{ balances: [] }`)
- Positions API returns array directly (not `{ positions: [] }`)

## External References

- **Official API Documentation**: https://arkm.com/docs
- **Exchange Website**: https://arkm.com
- **Support**: support@arkm.com
- **Status Page**: https://status.arkm.com
- **CCXT REST Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/arkham.ts
- **CCXT WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/arkham.ts
