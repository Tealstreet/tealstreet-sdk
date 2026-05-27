# Backpack Exchange Integration

## Exchange Information

- **Type**: Centralized Exchange (CEX)
- **Markets**: Perpetual futures (USDC-margined) and spot
- **Position Mode**: Hedged only (both long and short simultaneously)
- **Margin Type**: Cross margin only
- **Quote Currency**: USDC

## API Documentation

**Official Documentation**: https://docs.backpack.exchange/

## CCXT Reference Implementation

- **REST API Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/backpack.ts
- **WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/backpack.ts

## File Structure

| File                     | Purpose                                                                  |
| ------------------------ | ------------------------------------------------------------------------ |
| `backpack.exchange.ts`   | Main exchange class with REST API trading logic, order/position mapping  |
| `backpack.api.ts`        | Axios client setup, request/response interceptors, auth header injection |
| `backpack.signing.ts`    | ED25519 signature generation using `@noble/ed25519`                      |
| `backpack.ws-public.ts`  | Public WebSocket (tickers, trades, depth, klines, bookTicker)            |
| `backpack.ws-private.ts` | Private WebSocket (order updates, position updates)                      |
| `backpack.types.ts`      | All TypeScript interfaces, endpoint constants, instruction strings       |
| `__tests__/`             | Unit tests for API, auth, balance, OHLCV, orders, orderbook              |

## Authentication

Backpack uses **ED25519 signature authentication** for all state-mutating requests.

### Required Headers

- `X-API-Key`: Base64-encoded public key
- `X-Signature`: Base64-encoded ED25519 signature
- `X-Timestamp`: Unix time in milliseconds
- `X-Window`: Validity window in milliseconds (default: 5000ms, max: 60000ms)
- `X-Broker-Id`: Broker identifier (u16 integer) - Tealstreet's broker ID is `1200`

### Signature Generation Process (`backpack.signing.ts`)

1. Alphabetically sort request parameters by key
2. Convert to query string format: `key1=value1&key2=value2` (URI-encoded values, nulls/undefineds skipped)
3. Build payload: `instruction=<type>&<sorted_params>&timestamp=<ts>&window=<win>` (if no params: `instruction=<type>&timestamp=<ts>&window=<win>`)
4. Encode payload as UTF-8 bytes
5. Decode Base64 private key to bytes
6. Sign with ED25519 via `@noble/ed25519` (`signAsync`)
7. Base64-encode the resulting signature bytes

The `sha512Sync` function from `@noble/hashes/sha2.js` is set on `ed25519.etc` at module load time.

### Instruction Types

Each authenticated endpoint maps to a specific instruction string used in signature generation:

| Endpoint                           | Method | Instruction            |
| ---------------------------------- | ------ | ---------------------- |
| `/api/v1/order`                    | GET    | `orderQuery`           |
| `/api/v1/order`                    | POST   | `orderExecute`         |
| `/api/v1/order`                    | DELETE | `orderCancel`          |
| `/api/v1/orders`                   | GET    | `orderQueryAll`        |
| `/api/v1/orders`                   | DELETE | `orderCancelAll`       |
| `/api/v1/capital`                  | GET    | `balanceQuery`         |
| `/api/v1/capital/collateral`       | GET    | `collateralQuery`      |
| `/api/v1/position`                 | GET    | `positionQuery`        |
| `/api/v1/account`                  | GET    | `accountQuery`         |
| `/api/v1/account`                  | PATCH  | `accountUpdate`        |
| `/history/fills`                   | GET    | `fillHistoryQueryAll`  |
| `/history/orders`                  | GET    | `orderHistoryQueryAll` |
| `/wapi/v1/capital/deposits`        | GET    | `depositQueryAll`      |
| `/wapi/v1/capital/withdrawals`     | GET    | `withdrawalQueryAll`   |
| `/wapi/v1/capital/deposit/address` | GET    | `depositAddressQuery`  |

Public endpoints (no auth required): `/api/v1/time`, `/api/v1/status`, `/api/v1/assets`, `/api/v1/markets`, `/api/v1/market`, `/api/v1/ticker`, `/api/v1/tickers`, `/api/v1/depth`, `/api/v1/klines`, `/api/v1/trades`, `/api/v1/trades/history`, `/api/v1/markPrices`, `/api/v1/openInterest`, `/api/v1/fundingRates`. Determined by `isPublicEndpoint()` in `backpack.api.ts:135-154`.

### WebSocket Authentication (`backpack.ws-private.ts:65-86`)

Private WebSocket subscriptions include a `signature` object with `{key, signature, timestamp, window}`. The signature payload for WebSocket is `instruction=subscribe&timestamp=<ts>&window=<win>` (no additional params). Generated via `generateWsAuthSignature()` in `backpack.signing.ts:87-99`.

## Startup Sequence

Defined in `backpack.exchange.ts:110-151` (`start()` method):

1. **`fetchMarkets()`** (sequential) -- fetches `/api/v1/markets`, filters for symbols containing `_` with `visible !== false`, maps to `Market[]`
2. **Store update**: markets + `loaded.markets = true`
3. **`fetchTickers()`** (sequential) -- fetches `/api/v1/tickers` and `/api/v1/markPrices` in parallel (`Promise.all`). Only PERP markets (those with mark price data) are included
4. **Store update**: tickers + `loaded.tickers = true`
5. **`tick()`** (sequential) -- initial data fetch cycle:
   - `fetchAccountInfo()` -- gets account leverage limit from `/api/v1/account`
   - `fetchPositions()` -- gets positions from `/api/v1/position`, creates fake zero-size positions for all perp markets without active positions
   - `fetchBalance()` -- gets collateral/margin info from `/api/v1/capital/collateral`
   - Recalculates uPnL from positions sum
6. **`fetchOrders()`** (sequential) -- gets open orders from `/api/v1/orders`, runs `fixTriggerOrdersWithoutTriggerBy()`
7. **Store update**: orders + `loaded.orders = true`
8. **`publicWebsocket.connectAndSubscribe()`** (non-blocking) -- connects public WebSocket
9. **`privateWebsocket.connectAndSubscribe()`** (non-blocking) -- connects private WebSocket
10. **`subscribeToAllMarkets()`** (non-blocking, 2-second delay) -- subscribes to ticker, trades, orderbook, bookTicker for all markets

The 2-second delay on market subscriptions (`setTimeout` in `subscribeToAllMarkets`, line 155) allows WebSocket connections to establish before sending subscription messages.

## REST API Endpoints

### Market Data (Public)

- `GET /api/v1/markets` - All trading pairs
- `GET /api/v1/ticker` - 24h ticker for specific market
- `GET /api/v1/tickers` - 24h tickers for all markets
- `GET /api/v1/depth` - Order book (up to 5,000 levels per side)
- `GET /api/v1/trades` - Recent trades (max 1,000)
- `GET /api/v1/klines` - Candlestick data (timestamps in **seconds**, not milliseconds)
- `GET /api/v1/markPrices` - Mark prices and funding rates for all perps
- `GET /api/v1/openInterest` - Open interest data
- `GET /api/v1/fundingRates` - Funding rate history

### Trading (Authenticated)

- `POST /api/v1/order` - Place single order (instruction: `orderExecute`)
- `DELETE /api/v1/order` - Cancel single order (instruction: `orderCancel`)
- `DELETE /api/v1/orders` - Cancel all orders for a symbol (instruction: `orderCancelAll`)
- `GET /api/v1/order` - Get open order details (instruction: `orderQuery`)
- `GET /api/v1/orders` - List all open orders (instruction: `orderQueryAll`)

### Account (Authenticated)

- `GET /api/v1/capital` - Account balances (instruction: `balanceQuery`)
- `GET /api/v1/capital/collateral` - Collateral/margin info with net equity (instruction: `collateralQuery`)
- `GET /api/v1/position` - Futures positions (instruction: `positionQuery`)
- `GET /api/v1/account` - Account info including leverage limit (instruction: `accountQuery`)
- `PATCH /api/v1/account` - Update account settings (instruction: `accountUpdate`)
- `GET /history/fills` - Fill history (instruction: `fillHistoryQueryAll`)
- `GET /history/orders` - Order history (instruction: `orderHistoryQueryAll`)

## WebSocket Implementation

### Base URLs

- **Production**: `wss://ws.backpack.exchange`
- **Testnet**: Same as production (no separate testnet)

### Public WebSocket (`backpack.ws-public.ts`)

**Connection**: Creates WebSocket via `createWebsocket()` in `connectAndSubscribe()`. Registers `onOpen`, `onMessage`, `onClose` handlers.

**On open** (`onOpen`, line 48-55):

1. `trackLastReceived()` -- start health monitoring
2. `resetAndRefetchOrderBooks()` -- resets all orderbook states and refetches snapshots
3. `resubscribe()` -- re-sends all tracked subscriptions
4. `ping()` -- starts ping cycle

**Subscriptions**: Tracked in a `Set<string>`. Each `subscribeTo*()` method adds the stream to the set and sends a subscribe message. On reconnect, `resubscribe()` sends all tracked streams in a single message.

**Message format**:

- Subscribe: `{ "method": "SUBSCRIBE", "params": ["stream1", "stream2"] }`
- Unsubscribe: `{ "method": "UNSUBSCRIBE", "params": ["stream1", "stream2"] }`
- Messages: `{ "stream": "<stream_name>", "data": { "e": "<event_type>", ... } }`

**Public streams** (subscribed per-market on startup):

| Stream                      | Format                     | Throttle | Description                  |
| --------------------------- | -------------------------- | -------- | ---------------------------- |
| `ticker.<symbol>`           | `ticker.BTC_USDC_PERP`     | 1000ms   | 24h ticker updates           |
| `depth.<symbol>`            | `depth.BTC_USDC_PERP`      | 100ms    | Order book delta updates     |
| `trade.<symbol>`            | `trade.BTC_USDC_PERP`      | none     | Individual trade stream      |
| `bookTicker.<symbol>`       | `bookTicker.BTC_USDC_PERP` | none     | Best bid/ask updates         |
| `kline.<interval>.<symbol>` | `kline.1m.BTC_USDC_PERP`   | none     | Candlestick data (on demand) |

**Throttling details**:

- `handleDepthUpdate`: lodash `throttle(fn, 100, {leading: true, trailing: true})`
- `handleTickerUpdate`: lodash `throttle(fn, 1000, {leading: true, trailing: true})`
- Trade and bookTicker updates are not throttled

**NaN protection**: All parsed float values are checked with `!Number.isNaN()` before updating store. This prevents overwriting valid data with `NaN` from malformed messages.

### Private WebSocket (`backpack.ws-private.ts`)

**Connection**: Same URL as public. On open, authenticates then subscribes.

**On open** (`onOpen`, line 42-49):

1. `trackLastReceived()`
2. `authenticate()` -- generates WS auth signature and subscribes to `account.orderUpdate` + `account.positionUpdate`
3. `resubscribe()` -- re-authenticates on reconnect (calls `authenticate()` again)
4. `ping()`

**Private streams**:

| Stream                         | Description                   |
| ------------------------------ | ----------------------------- |
| `account.orderUpdate`          | All order status changes      |
| `account.orderUpdate.<symbol>` | Symbol-specific order updates |
| `account.positionUpdate`       | Futures position changes      |

**Order update events** (`e` field): `orderAccepted`, `orderFill`, `orderCancelled`, `orderExpired`, `orderModified`

**Throttled fetches after WebSocket events** (in `backpack.ws-private.ts`):

- **Position fetch** (`throttledFetchPositions`, line 393-423): Min 500ms between fetches. Triggered on `orderFill` events. Uses custom throttle with pending-event tracking.
- **Balance fetch** (`throttledFetchBalance`, line 462-502): Min 500ms between fetches. Triggered on `orderFill` events. Same throttle pattern as positions.
- **Order fetch** (`throttledFetchOrders`, line 436-459): Debounced 10 seconds (`clearTimeout`/`setTimeout`). Triggered on any order update. Batches rapid updates into a single REST fetch.

### Ping/Pong Quirk (both public and private)

Backpack's WebSocket does not support standard PING messages. Sending `{ "method": "PING" }` results in an error response with code 4002 (parse error). The implementation uses this error response as a de facto pong:

- In `onMessage`, error code 4002 is intercepted and calls `handlePongEvent(10_000)` (10-second timeout for next health check)
- `ping()` records `performance.now()` for latency measurement

### Reconnection

Handled by `BaseWebSocket.onClose()`:

- On close, removes event listeners, increments `onCloseCount` (max 100)
- Waits `WS_MIN_RECONNECTION_DELAY` ms before calling `connectAndSubscribe()` again
- Public WS: `resetAndRefetchOrderBooks()` resets all orderbook states and refetches snapshots on reconnect
- Private WS: `resubscribe()` re-authenticates by generating new signatures

## Order Types and Mappings

### API Order Types

Backpack's API supports two native order types: `Limit` and `Market`. Stop-loss and take-profit orders are implemented as **trigger orders** using `Market` type with a `triggerPrice`.

| Tealstreet Type | API `orderType` | Key Fields                                                                  |
| --------------- | --------------- | --------------------------------------------------------------------------- |
| Market          | `Market`        | `quantity`                                                                  |
| Limit           | `Limit`         | `price`, `quantity`, `timeInForce`, `postOnly`                              |
| StopLoss        | `Market`        | `triggerPrice`, `triggerQuantity`, `triggerBy=MarkPrice`, `reduceOnly=true` |
| TakeProfit      | `Market`        | `triggerPrice`, `triggerQuantity`, `triggerBy=LastPrice`, `reduceOnly=true` |

### Time In Force

- `GTC` - Good Till Cancel (default)
- `IOC` - Immediate Or Cancel
- `FOK` - Fill Or Kill
- `PO` - Post Only (sent via `postOnly: true` field alongside `timeInForce: GTC`)

### Order Sides

Backpack uses `Bid` (buy) and `Ask` (sell) instead of standard buy/sell terminology.

### Trigger Orders

- `triggerBy` determines which price triggers the order: `MarkPrice`, `IndexPrice`, or `LastPrice`
- TakeProfit defaults to `triggerBy: 'LastPrice'` (triggers on actual trade execution)
- StopLoss defaults to `triggerBy: 'MarkPrice'` (prevents manipulation)
- Trigger orders use `triggerQuantity` instead of `quantity`

### Attached TP/SL

- Market orders support attached TP/SL via `takeProfitTriggerPrice`/`stopLossTriggerPrice` fields (plus corresponding `*LimitPrice` fields)
- Limit orders with TP/SL place separate trigger orders instead (attached TP/SL only works for immediately-filling orders). The main limit order is placed first, then TP and SL are placed sequentially with error handling per order (`backpack.exchange.ts:695-742`)

### Self-Trade Prevention

Supported values: `RejectTaker`, `RejectMaker`, `RejectBoth`, `Allow`

## Order Lifecycle

### Order Placement (`placeOrder`, `backpack.exchange.ts:592-762`)

1. Look up market, validate it exists
2. Determine `apiOrderType` (`Limit` or `Market`), handle PostOnly via `postOnly: true` + `timeInForce: GTC`
3. Format price and amount to market precision using `formatPrice()` / `formatAmount()` helpers
4. For trigger orders: set `triggerPrice`, `triggerQuantity`, `triggerBy`, and `reduceOnly: true`
5. For limit orders with TP/SL: place main order, then separate TP/SL trigger orders sequentially
6. For market orders with TP/SL: use attached `takeProfitTriggerPrice`/`stopLossTriggerPrice` params
7. Returns array of order IDs

### Order Modification (Cancel-and-Replace, `updateOrder`, `backpack.exchange.ts:807-854`)

Backpack does not support native order amendment. Updates are implemented as:

1. Find existing order in store by ID
2. Validate new amount is positive (before cancelling, to avoid orphaned cancellations)
3. Cancel existing order via `cancelOrder()`
4. Place new order via `placeOrder()` with updated params
5. If replacement placement fails, the original is already cancelled (logged as error)

### Order Cancellation

- **Single order**: `DELETE /api/v1/order` with `{symbol, orderId}` (`backpack.exchange.ts:764-781`)
- **Per-symbol**: `DELETE /api/v1/orders` with `{symbol}` (`backpack.exchange.ts:793-805`)
- **All orders**: Iterates all markets serially via `forEachSeries`, calling cancel-all per symbol (`backpack.exchange.ts:880-896`)
- **Batch cancel**: `cancelOrders()` cancels sequentially via `forEachSeries`, continuing on individual errors (`backpack.exchange.ts:783-791`)

### Trigger Order Fix on Startup (`fixTriggerOrdersWithoutTriggerBy`, `backpack.exchange.ts:423-494`)

On every `fetchOrders()` call (startup and each tick), scans for trigger orders where `triggerBy === null || triggerBy === undefined`:

1. Identifies affected orders (has `triggerPrice` but missing `triggerBy`)
2. For each: determines if TP or SL via `determineTriggerOrderType()` (compares trigger price to current market price)
3. Cancels the old order
4. Recreates with correct `triggerBy` (`LastPrice` for TP, `MarkPrice` for SL)
5. Updates the orders array in-place so the caller gets corrected data
6. Continues with other orders even if one fix fails

### Trigger Order Type Detection (`determineTriggerOrderType`, `backpack.exchange.ts:1015-1032`)

Logic to classify Market+triggerPrice orders as TP or SL:

- **Sell side**: trigger below current price = StopLoss, above = TakeProfit
- **Buy side**: trigger above current price = StopLoss, below = TakeProfit
- Falls back to StopLoss if no current price available

### Order Status Mapping

| Backpack Status   | Internal Status              |
| ----------------- | ---------------------------- |
| `New`             | `OrderStatus.Open`           |
| `PartiallyFilled` | `OrderStatus.Open`           |
| `Filled`          | `OrderStatus.Closed`         |
| `Cancelled`       | `OrderStatus.Canceled`       |
| `Expired`         | `OrderStatus.Canceled`       |
| `TriggerPending`  | `OrderStatus.Open` (default) |

## Symbol Format

Symbols use an underscore-separated format: `{BASE}_{QUOTE}_{TYPE}`

- **Perpetual futures**: `BTC_USDC_PERP`, `ETH_USDC_PERP`, `SOL_USDC_PERP`
- **Spot markets**: `BTC_USDC`, `SOL_USDC`

Markets are parsed by splitting on `_`. The first segment is the base asset, the second is the quote asset. Markets with `marketType === 'PERP'` are treated as perpetual swaps; all others are spot.

Only markets containing `_` and with `visible !== false` are loaded. Only perpetual markets (those with mark price data) are included in the ticker list.

**Default symbol**: `BTC_USDC_PERP` | **Default quote**: `USDC`

## Data Precision & Conversion

### Price Formatting (`formatPrice`, `backpack.exchange.ts:616-623`)

1. Get `market.precision.price` (e.g., `0.01` for 2 decimals)
2. Calculate factor: `1 / precision`
3. Round: `Math.round(price * factor) / factor`
4. Count decimal places from precision string representation
5. Format with `toFixed(decimals)` to avoid floating point representation issues (e.g., `0.1 + 0.2` artifacts)

### Amount Formatting (`formatAmount`, `backpack.exchange.ts:629-633`)

Same algorithm as `formatPrice` but uses `market.precision.amount`.

### Precision Sources (`fetchMarkets`, `backpack.exchange.ts:220-282`)

- `precision.amount`: from `quantityIncrement` or `filters.quantity.stepSize` (fallback `0.001`)
- `precision.price`: from `priceIncrement` or `filters.price.tickSize` (fallback `0.01`)
- Max leverage: calculated from IMF (Initial Margin Fraction): `1 / imfFunction.base`, rounded to integer

### Timestamp Conversions

- WebSocket event timestamps (`E` field): **microseconds** -- divided by 1000 for milliseconds (`backpack.ws-private.ts:226,297`)
- Kline API input: milliseconds divided by 1000 to **seconds** (`backpack.exchange.ts:338-339`)
- Kline start times: ISO date strings (e.g., `"2025-12-15 13:28:00"`) parsed with `+'Z'` suffix for UTC (`backpack.exchange.ts:351`)
- WS kline timestamps: ISO strings (e.g., `"2025-12-15T19:02:00"`) parsed with `+'Z'` suffix (`backpack.ws-public.ts:401`)

## Rate Limiting

- **REST API**: Configured at `maxRPS: 10` (10 requests per second via `axios-rate-limit`, `backpack.exchange.ts:70`)
- **Network retry**: 3 retries on network errors only (via `axios-retry`, not HTTP errors, `backpack.api.ts:30`)
- **Tick interval**: 10 seconds between REST polling cycles (`loop(() => this.tick(), 10_000)`, `backpack.exchange.ts:202`)
- **Metadata tick interval**: 2000ms (`tickInterval: 2000` in exchangeMetadata)
- **Request timeout**: 30 seconds default (configurable via `extra.recvWindow`, `backpack.api.ts:17`)

### WebSocket Throttling

- **Depth updates**: Throttled to 100ms (leading + trailing)
- **Ticker updates**: Throttled to 1000ms (leading + trailing)
- **Position fetch after fill**: Min 500ms interval
- **Balance fetch after fill**: Min 500ms interval
- **Order fetch after update**: Debounced 10 seconds (batches rapid order updates)

## State Reconciliation

### Orders

- REST `fetchOrders()` returns the full set of open orders and replaces the store
- WS `handleOrderUpdate()` calls `store.addOrUpdateOrder()` for incremental updates
- After any WS order update, `throttledFetchOrders()` schedules a full REST refetch 10 seconds later (debounced) to reconcile
- Fill events on WS trigger `throttledFetchPositions()` and `throttledFetchBalance()` for immediate state updates

### Positions

- REST `fetchPositions()` returns all real positions + fake zero-size positions for every perp market without an active position
- Fake positions are created for both Long and Short sides per symbol (hedged mode)
- Actual positions override fakes via `uniqBy([...actualPositions, ...fakePositions], 'id')` (`backpack.exchange.ts:585`)
- WS `handlePositionUpdate()` calls `store.addOrUpdatePosition()` for incremental updates
- Zero-size position updates from WS remove the position from the store (`backpack.ws-private.ts:271-279`)
- Leverage for positions comes from `accountLeverageLimit` (REST) or `1 / imf` (WS position update)

### Orderbook

Snapshot-then-delta approach (`backpack.ws-public.ts:437-556`):

1. Subscribe to `depth.<symbol>` WebSocket stream
2. Initialize buffer state (`OrderBookState`: `updates[]`, `isSnapshotLoaded`, `lastUpdateId`)
3. Fetch full snapshot via REST `GET /api/v1/depth`
4. Drop buffered updates older than snapshot's `lastUpdateId`
5. Apply remaining buffered updates
6. Mark snapshot as loaded, clear buffer
7. Apply subsequent delta updates in real-time via `updateOrderBook()` utility
8. On WebSocket reconnect (`resetAndRefetchOrderBooks`, line 74-91): reset all states and refetch all snapshots

### Balance

- Fetched from `/api/v1/capital/collateral` endpoint
- `total` = `netEquity`, `free` = `netEquityAvailable`, `used` = `netEquityLocked`, `upnl` = `pnlUnrealized`
- In `tick()`, uPnL is recalculated from position sum: `sumBy(positions, 'unrealizedPnl')` (`backpack.exchange.ts:187-188`)

## Exchange Metadata

Key metadata values from `exchangeMetadata.ts`:

```
exchangeName: 'backpack'
defaultQuote: 'USDC'
defaultSymbol: 'BTC_USDC_PERP'
positionModes: [Hedged]
marginTypes: [Cross]
hideMarginTypeSelector: true
hidePositionModeSelector: true
leverageIsGlobal: true
leverageMagnets: [1, 2, 3, 5, 10, 15, 20, 25, 50, 100]
supportsTestnet: false
requiresCors: true
supportsTunnel: false
supportsChaser: false
supportsTrailingStopLoss: false
supportsClosedPnLEndpoint: false
supportsPartialStops: true
supportsInfinityStops: false
supportsMaxCrossLeverage: true
orderbookTickDelay: 100
tickInterval: 2000
candleApiCountLimit: 500
tvIntraDayMultipliers: ['1', '5', '15', '30', '60']
tvMaxGetBarFetches: 10
supportedTPSLTriggerTypes: [LastPrice, MarkPrice]
defaultTPTriggerType: MarkPrice
defaultSLTriggerType: MarkPrice
orderPendingUpdateDelay: 1000
stopOrderPendingUpdateDelay: 1000
```

### Position Mode and Margin Type

Backpack uses **fixed** configurations that cannot be changed via API:

- Always returns `PositionMode.Hedged` from `getPositionMode()`
- Always sets `marginType: MarginType.Cross` on all positions
- Emits user-facing errors when `changePositionMode()` or `changeMarginType()` are called

### Leverage

Account-level leverage limits rather than per-symbol:

- **Endpoint**: `PATCH /api/v1/account` with `leverageLimit` parameter
- **Scope**: Sets the maximum leverage for the entire account/subaccount
- **Global**: `leverageIsGlobal: true` -- setting leverage for any symbol applies account-wide
- **Max leverage calculation**: `1 / imfFunction.base` (e.g., IMF 0.05 = 20x max)
- After setting leverage, positions are refetched to update all with new value (`backpack.exchange.ts:870-871`)

## Testnet/Demo

**Not available.** `supportsTestnet: false` in metadata. The `BASE_URLS.TESTNET` and `WSS_URLS.TESTNET` point to the same production URLs (`backpack.types.ts:3-11`).

## Known Quirks / Implementation Notes

1. **WebSocket Ping/Pong**: Backpack does not support standard PING. Sending `{"method":"PING"}` returns error code 4002, which is used as a pong response for latency measurement.

2. **Trigger order `triggerBy` fix**: On every `fetchOrders()` call, `fixTriggerOrdersWithoutTriggerBy()` scans for trigger orders with null `triggerBy` and recreates them. Without `triggerBy`, orders may not trigger when price crosses the trigger level.

3. **Kline null values**: WS kline updates with `null` OHLCV values (sent on empty candle close) are skipped (`backpack.ws-public.ts:360-361,388-396`).

4. **Kline timestamps**: REST API uses seconds; WS uses ISO date strings. Both require conversion. ISO strings need `+'Z'` suffix for UTC parsing.

5. **Hedged mode fake positions**: Creates zero-size placeholder positions for both Long and Short on every perp market without an active position, so the UI always shows position slots.

6. **CORS required**: `requiresCors: true` -- browser requests need CORS proxy.

7. **HTTP-over-WebSocket**: When `tealstreetTunnel` is enabled, uses `axiosHttpOverWebsocketAdapter` for REST requests (`backpack.api.ts:33-35`).

8. **Market subscription delay**: 2-second `setTimeout` before subscribing to market data streams to allow WebSocket connection to establish (`backpack.exchange.ts:155`).

9. **Position endpoint fallback**: If `/api/v1/position` returns a "not found" error, gracefully returns only fake positions (`backpack.exchange.ts:503-508`).

10. **Kline interval format**: WS kline subscriptions poll with 100ms retry for connection readiness (`waitForConnectedAndSubscribe` pattern used in `listenOHLCV` and `listenOrderBook`).

11. **`percentage` field**: Backpack returns `priceChangePercent` as a decimal (e.g., `0.05` for 5%), which is multiplied by 100 for display (`backpack.exchange.ts:315`, `backpack.ws-public.ts:293`).

12. **Account validation**: `validateAccount()` only checks if the capital endpoint is accessible (successful response = valid key). No dedicated validation endpoint.

## Error Handling

### REST API Error Formats (`backpack.api.ts:72-127`)

Backpack API returns errors in multiple formats, all handled by the response interceptor:

1. **Success response with error object**: `{ "error": { "msg": "..." } }` -- intercepted in success handler
2. **Success response with code+msg**: `{ "code": <number>, "msg": "..." }` -- intercepted in success handler
3. **HTTP error with structured data**: `{ "code": "INVALID_CLIENT_REQUEST", "message": "Quantity decimal too long" }` -- error interceptor
4. **HTTP error with nested error**: `{ "error": { "msg": "..." } }` -- error interceptor
5. **HTTP error with simple msg**: `{ "msg": "..." }` -- error interceptor
6. **HTTP error with string body**: plain text error -- error interceptor
7. **No response data**: original error message used

All errors emit to the user via `exchange.emitter.emit('error', ...)` and are logged with status code and URL for debugging.

### Network Error Retry (`backpack.api.ts:30`)

`axios-retry` configured with 3 retries for network errors only (`isNetworkError`). HTTP errors (4xx, 5xx) are not retried.

### Graceful Degradation in Tick Loop (`backpack.exchange.ts:198-199`)

The `tick()` method wraps all fetches in try/catch and logs errors without crashing. The next tick is always scheduled via `loop()` regardless of errors.

### WebSocket Error Recovery

- Both public and private WS handle parse errors gracefully (logged via `emitAndLogError`)
- Code 4002 errors are treated as pong responses (not logged as errors)
- All other WS errors are logged
- Connection health monitored via `trackLastReceived()` with automatic reconnection on timeout

## Testing / Validation

Test files in `__tests__/`:

- `backpack.auth.test.ts` - Signature generation and verification
- `backpack.api.test.ts` - API client setup and interceptors
- `backpack.fetchBalance.test.ts` - Balance parsing
- `backpack.methods.test.ts` - Exchange method tests
- `backpack.ohlcv.test.ts` - OHLCV fetching
- `backpack.listen-ohlcv.test.ts` - WebSocket OHLCV streaming
- `backpack.order.test.ts` - Order placement and mapping
- `backpack.order-flow.test.ts` - Full order flow integration
- `backpack.orderbook.test.ts` - Orderbook snapshot/delta handling
- `backpack.integration.test.ts` - Integration tests

Manual test files (not in `__tests__/`):

- `test-balance.ts`, `test-balance-live.ts` - Balance testing
- `test-place-order-raw.ts`, `test-cancel-order-raw.ts` - Raw order operations
- `test-order-flow.ts` - Order flow testing
- `manual-test.ts` - General manual testing

## Grafana Monitoring

- **Every-call endpoints**: `ENDPOINTS.ORDER`, `ENDPOINTS.ORDERS`
- **Periodic endpoints**: `ENDPOINTS.CAPITAL`, `ENDPOINTS.POSITIONS`
