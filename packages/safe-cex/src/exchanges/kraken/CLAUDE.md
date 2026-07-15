# Kraken Exchange Integrations

This directory contains two separate Kraken exchange integrations:
- **Kraken Futures** (`kraken.*`) — Perpetual futures on `futures.kraken.com`
- **Kraken Spot** (`krakenspot.*`) — Spot trading on `api.kraken.com`

They share a directory but have completely different APIs, auth schemes, WebSocket protocols, and symbol formats. See sections below for each.

---

# Kraken Futures Integration

## Exchange Information

- **Pretty Name**: Kraken Futures
- **Exchange Name (internal)**: `kraken`
- **Type**: Centralized exchange (CEX), perpetual futures only
- **Position Mode**: One-way only (`PositionMode.OneWay`)
- **Margin Types**: Cross and Isolated
- **Status**: Enabled in production and preview

## API Documentation

- **Futures REST API**: https://docs.kraken.com/api/docs/futures-api/trading/send-order
- **Futures WebSocket**: https://docs.kraken.com/api/docs/futures-api/websocket/book
- **Charts API**: https://docs.kraken.com/api-reference/candles/market-candles
- **CCXT Reference Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/krakenfutures.ts
- **CCXT WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/krakenfutures.ts

## File Structure

| File                   | Purpose                                                                        |
| ---------------------- | ------------------------------------------------------------------------------ |
| `kraken.api.ts`        | REST API client with request signing, axios interceptors, nonce generation     |
| `kraken.exchange.ts`   | Main exchange class extending `BaseExchange`                                   |
| `kraken.types.ts`      | Endpoint constants, URL definitions, type mapping records                      |
| `kraken.ws-private.ts` | Private WebSocket: challenge/response auth, orders, positions, balances, fills |
| `kraken.ws-public.ts`  | Public WebSocket: ticker feed, orderbook snapshot+delta                        |

## Authentication

### REST API (`kraken.api.ts`)

Signature algorithm: `base64(HMAC_SHA512(SHA256(postData + nonce + endpointPath), base64Decode(secret)))`

1. Build `postData` from request parameters, form-encoded with `qs.stringify` using RFC 1738 format (`+` for spaces) and alphabetical key sorting (`kraken.api.ts`, `sort` function)
2. Generate nonce: `(virtualClock.getCurrentTimeMs() * 1000) + (nonceCounter++ % 1000)` -- uses a counter to ensure uniqueness even at same millisecond (`kraken.api.ts`)
3. Construct path for signing: `'/api/' + endpoint` (`kraken.api.ts`)
4. Build auth string: `postData + nonce + pathForSigning` (`kraken.api.ts`)
5. SHA-256 hash the auth string (`kraken.api.ts`)
6. Base64-decode the API secret (`kraken.api.ts`)
7. HMAC-SHA512 sign the hash with the decoded secret (`kraken.api.ts`)
8. Set headers: `APIKey`, `Authent` (signature), `Nonce`, `Content-Type: application/x-www-form-urlencoded`

The encoded PostData string used to build `Authent` must be byte-for-byte identical to the encoded body/query that is sent. This is Kraken's post-October-1-2025 signing method; signing a decoded form and then transmitting an encoded form breaks authentication.

**Batch orders**: Use special `json=` format -- the body is `json=` + URL-encoded JSON string (`kraken.api.ts`).

**Public endpoints bypass**: Endpoints in `PUBLIC_ENDPOINTS` array (`/v3/instruments`, `/v3/tickers`, `/v3/orderbook`) skip authentication (`kraken.api.ts`).

### WebSocket Authentication (`kraken.ws-private.ts`)

Challenge/response flow:

1. On WebSocket open, send `{ event: 'challenge', api_key: '<key>' }` (`kraken.ws-private.ts`)
2. Server responds with `{ event: 'challenge', message: '<challenge_string>' }`
3. SHA-256 hash the challenge string (`kraken.ws-private.ts`)
4. Base64-decode the API secret (`kraken.ws-private.ts`)
5. HMAC-SHA512 sign the hash with the decoded secret (`kraken.ws-private.ts`)
6. Store `originalChallenge` and `signedChallenge` for subscription messages
7. Subscribe to private feeds including `api_key`, `original_challenge`, and `signed_challenge` in each subscription message (`kraken.ws-private.ts`)

**Key detail**: Auth does not use a separate `authenticate` event. Instead, credentials are included in each private feed subscription message.

### Time Sync

Uses `virtualClock.getCurrentTimeMs()` for nonce generation. No explicit server time synchronization endpoint is called.

## Startup Sequence

`start()` in `kraken.exchange.ts` executes **sequentially**:

1. `fetchMarkets()` -- GET `/v3/instruments`, filter for `PF_` prefix (perpetual futures), build `Market[]`
2. Store update: set `markets` and `loaded.markets = true`
3. Dispose check (`if (this.isDisposed) return`)
4. `fetchTickers()` -- GET `/v3/tickers`, map to existing markets
5. Store update: set `tickers` and `loaded.tickers = true`
6. Dispose check
7. Create and connect `KrakenPublicWebsocket` -- subscribes to ticker feed for all markets
8. **Conditionally** create `KrakenPrivateWebsocket` -- only if `!publicDataOnly && key && secret` (`kraken.exchange.ts`)
9. Call `this.tick(0)` -- triggers immediate first tick

**No parallel fetches at startup** -- markets must load before tickers (tickers rely on `marketsBySymbol` lookup).

## REST API Endpoints

All endpoints are relative to the base URL. Defined in `kraken.types.ts`.

| Constant               | Path                      | Method  | Auth | Purpose                         |
| ---------------------- | ------------------------- | ------- | ---- | ------------------------------- |
| `MARKETS`              | `/v3/instruments`         | GET     | No   | Fetch tradeable instruments     |
| `TICKERS`              | `/v3/tickers`             | GET     | No   | Ticker data for all markets     |
| `ORDERBOOK`            | `/v3/orderbook`           | GET     | No   | Order book snapshot             |
| `ACCOUNT`              | `/v3/accounts`            | GET     | Yes  | Account balances (cash + flex)  |
| `OPEN_ORDERS`          | `/v3/openorders`          | GET     | Yes  | All open orders                 |
| `POSITIONS`            | `/v3/openpositions`       | GET     | Yes  | Open positions                  |
| `ORDER`                | `/v3/sendorder`           | POST    | Yes  | Place new order                 |
| `EDIT_ORDER`           | `/v3/editorder`           | POST    | Yes  | Modify limit order              |
| `CANCEL_ORDER`         | `/v3/cancelorder`         | POST    | Yes  | Cancel single order             |
| `BATCH_ORDERS`         | `/v3/batchorder`          | POST    | Yes  | Batch operations                |
| `LEVERAGE_PREFERENCES` | `/v3/leveragepreferences` | GET/PUT | Yes  | Get/set leverage                |
| `CANDLES`              | `/api/charts/v1`          | GET     | No   | OHLCV data (separate host path) |

### Charts API

The charts endpoint uses a **different URL structure** than the regular API:

- Regular API base: `https://futures.kraken.com/derivatives/api`
- Charts API: `https://futures.kraken.com/api/charts/v1/{tick_type}/{symbol}/{resolution}`

The `fetchOHLCV` method constructs the full URL directly, bypassing the base URL (`kraken.exchange.ts`).

## WebSocket Implementation

### Public WebSocket (`kraken.ws-public.ts`)

**URL**: `wss://futures.kraken.com/ws/v1` (production) / `wss://demo-futures.kraken.com/ws/v1` (testnet)

**Connection**: Uses `BaseWebSocket.createWebsocket()` which provides automatic reconnection via `@tealstreet/reconnecting-websocket`. The `onClose` handler from `BaseWebSocket` manages reconnection with minimum delay.

**On Open** (`kraken.ws-public.ts`):

1. Subscribe to `ticker` feed for all markets, batched in groups of 50 (`product_ids` array)
2. Re-subscribe to any active orderbook handlers
3. OHLCV re-subscription is a no-op (Kraken Futures does not support WebSocket OHLCV)

**Message Routing** (`kraken.ws-public.ts`):

- `event` field present: subscription confirmations or errors (logged, not processed further)
- `feed: 'ticker'` -> `handleTicker()`: updates bid, ask, last, mark, index, fundingRate, fundingTime, percentage
- `feed: 'book_snapshot'` -> `handleBookSnapshot()`: full orderbook replacement
- `feed: 'book'` -> `handleBookDelta()`: incremental update (single price level)
- `feed: 'ohlc'` -> no-op handler (not supported by Kraken Futures)

**Ticker Updates** (`kraken.ws-public.ts`):

- Extracts fields using multiple fallback key names (e.g., `bid` or `best_bid`)
- Calculates `percentage` from `open24h` and `last` if available
- Only updates fields that are present (undefined fields are skipped)

**Orderbook** (`kraken.ws-public.ts`):

- Maintains an in-memory cache per symbol using `Map<number, number>` for bids and asks
- Snapshot: clears and rebuilds the cache from `bids[]` and `asks[]` arrays (objects with `price` and `qty` fields)
- Delta: updates a single price level; `qty === 0` means remove the level
- Emits: sorted arrays limited to 100 levels, with quantities multiplied by `contractSize`
- Subscription sent immediately if connected, otherwise retries every 100ms

**OHLCV**: Not supported via WebSocket. Returns no-op unsubscribe function from `listenOHLCV`.

### Private WebSocket (`kraken.ws-private.ts`)

**URL**: Same as public (`wss://futures.kraken.com/ws/v1`)

**On Open**: Immediately attempts authentication via challenge/response flow.

**Private Feed Subscriptions** (sent after successful auth, `kraken.ws-private.ts`):

- `fills` -- trade executions
- `open_orders` -- order lifecycle updates
- `open_positions` -- position changes
- `account_balances` -- balance updates

**Message Routing** (`kraken.ws-private.ts`):

- `challenge` event -> perform auth signing
- `error`/`info` with auth message -> log error, set `authenticated = false`
- `subscribed` -> ignored
- `fills` -> `handleFills()`
- `open_orders` / `open_orders_snapshot` -> `handleOpenOrders()`
- `open_positions` / `open_positions_snapshot` -> `handleOpenPositions()`
- `account_balances` / `account_balances_snapshot` / `balances` -> `handleBalances()`

**Reconnection**: Handled by `BaseWebSocket.onClose()`. On reconnect, the full auth flow (challenge -> sign -> subscribe) re-executes since `onOpen` calls `authenticate()`.

**Heartbeat / Ping-Pong**: Managed by `BaseWebSocket.trackLastReceived()`. If no message is received within `WS_RECONNECT_THRESHOLD_MS` (20 seconds), the base class triggers reconnection.

## Order Types and Mappings

### Kraken to Internal

| Kraken `orderType` | Internal `OrderType`                             |
| ------------------ | ------------------------------------------------ |
| `lmt`, `limit`     | `OrderType.Limit`                                |
| `mkt`, `market`    | `OrderType.Market`                               |
| `stp`, `stop`      | `OrderType.StopLoss`                             |
| `take_profit`      | `OrderType.TakeProfit`                           |
| `post`             | `OrderType.Limit` (with `PostOnly` TIF)          |
| `ioc`              | `OrderType.Limit` (with `ImmediateOrCancel` TIF) |
| `fok`              | `OrderType.Limit` (with `FillOrKill` TIF)        |

### Internal to Kraken (placing orders)

| Internal                        | Kraken `orderType` |
| ------------------------------- | ------------------ |
| `OrderType.Limit` + default TIF | `'lmt'`            |
| `OrderType.Limit` + `PostOnly`  | `'post'`           |
| `OrderType.Limit` + `IOC`       | `'ioc'`            |
| `OrderType.Limit` + `FOK`       | `'fok'`            |
| `OrderType.Market`              | `'mkt'`            |
| `OrderType.StopLoss`            | `'stp'`            |
| `OrderType.TakeProfit`          | `'take_profit'`    |

### WebSocket Order Direction

- `direction: 0` or `'buy'` -> `OrderSide.Buy`
- `direction: 1` or `'sell'` -> `OrderSide.Sell`

### WebSocket Fill Side

- `buy: true` -> `OrderSide.Buy` (boolean field)
- `buy: false` -> `OrderSide.Sell`
- Fallback: `side` or `direction` string field

## Order Lifecycle

### Placing Orders (`kraken.exchange.ts`)

1. Map internal order type to Kraken type string
2. Look up market for precision rounding
3. Round `size` via `adjust(amount, market.precision.amount)`
4. Build body: `orderType`, `symbol`, `side`, `size`, `cliOrdId` (prefixed `ts_` + uuid), `broker` code
5. For limit orders: add `limitPrice` (rounded)
6. For stop/TP orders: add `stopPrice`, `reduceOnly: true`, optional `triggerSignal` (`mark`, `index`, or `last`)
7. POST to `/v3/sendorder` via `unlimitedXHR` (bypasses rate limiter)
8. Validate: `result === 'success'` AND `sendStatus.status` is `'placed'` or `'filled'`
9. If original order was Limit with attached TP/SL: derive algo orders via `deriveAlgoOrdersFromNormalOrdersOpts` and place them separately

**Broker tag**: `'AA73 N84G 7YOF J26Y'` is included in all order placements.

### Updating Orders (`kraken.exchange.ts`)

- **Stop/TP orders**: Cancel-and-recreate pattern. Cannot use `editorder` for stops.
  1. Cancel existing order via `cancelOrders()`
  2. Place new order with updated params via `placeOrder()`
- **Limit orders**: Use `/v3/editorder` endpoint with `orderId`, optional `size` and `limitPrice`

### Canceling Orders (`kraken.exchange.ts`)

- **Single order**: POST `/v3/cancelorder` with `order_id`
- **Multiple orders**: POST `/v3/batchorder` with array of `{ order: 'cancel', order_id }` entries

### WebSocket Order Updates (`kraken.ws-private.ts`)

- **Direct cancellation**: `is_cancel: true` with `order_id` at root level (no `order` object) -> mark as `Canceled`
- **Snapshot** (`orders` array or `open_orders_snapshot` feed): replace all orders via `addOrUpdateOrdersFull()`
- **Single update** (`order` object): update individually via `addOrUpdateOrder()`
- **Cancellation in update**: `is_cancel: true` with `orders`/`order` -> mark each as `Canceled`
- After every WS order event, `throttledFetchOrders()` is called (1-second debounce)

### WebSocket Fill Handling (`kraken.ws-private.ts`)

1. Parse fill data: `order_id`, `qty`, `price`, `symbol`, side (from `buy` boolean or `side`/`direction` string)
2. Find existing order by ID and update `filled`/`remaining`
3. If `remaining === 0`, mark order as `Closed`
4. Emit `'fill'` event on parent emitter
5. Trigger `throttledFetchOrders()` for state sync

## Symbol Format

- Perpetual futures use `PF_` prefix: `PF_BTCUSD`, `PF_ETHUSD`
- Only instruments starting with `PF_` are loaded (`kraken.exchange.ts`)
- `prettySymbol` strips the `PF_` prefix for display (`kraken.exchange.ts`)
- Market `id` is `{base}/{quote}` (e.g., `BTC/USD`)
- Market `symbol` is the raw Kraken symbol (e.g., `PF_BTCUSD`)

## Data Precision & Conversion

### Price Precision

- Derived from `tickSize` field in instrument data
- `tickSize` is a decimal value (e.g., `0.1`, `0.5`, `1.0`) used directly as `precision.price`
- Rounded via `adjust(price, market.precision.price)` when placing/editing orders

### Amount Precision

- Derived from `contractValueTradePrecision` field (integer: number of decimal places)
- Value `0` means whole numbers only (precision = `1`), value `4` means `0.0001`
- Converted via `precisionToString(decimalPlaces)` then `parseFloat()`
- Default if not present: `1` (whole numbers)
- Rounded via `adjust(amount, market.precision.amount)` when placing/editing orders

### Contract Size

- `contractSize` from instrument data, defaults to `1`
- Orderbook quantities are multiplied by `contractSize` for display (`kraken.ws-public.ts`)

### Max Leverage

- Calculated from `marginLevels` array in instrument data
- Uses the first tier's `initialMargin`: `maxLeverage = floor(1 / initialMargin)`
- Default fallback: `100`

## Rate Limiting

- REST API: Rate limited via `axios-rate-limit` at **5 RPS** (`kraken.exchange.ts`)
- A separate `unlimitedXHR` client bypasses rate limiting for order operations (place, edit, cancel)
- Network error retry: 3 retries via `axios-retry` on network errors
- Request timeout: 20 seconds
- Charts API: Subject to Kraken's separate rate limits (not specifically configured client-side)

## State Reconciliation

### Orders

- **WebSocket**: Applies real-time updates via `addOrUpdateOrder()` for individual events, `addOrUpdateOrdersFull()` for snapshots
- **REST polling**: `fetchOrders()` is called in the tick cycle every 10 seconds (every 20th tick at 500ms intervals)
- **Throttled sync**: After every WebSocket order/fill event, `throttledFetchOrders()` triggers a REST fetch with 1-second debounce to catch any missed updates (`kraken.ws-private.ts`)
- **Deduplication**: The throttle timer (`ordersThrottleTimeout`) is cleared and reset on each event to avoid excessive requests

### Positions

- **WebSocket**: `handleOpenPositions()` maps positions from `balance` field (positive=long, negative=short), filters for non-zero contracts, and calls `addOrUpdatePositionsFull()`
- **REST polling**: `fetchPositions()` runs every 10 seconds in the tick cycle. `/v3/openpositions` supplies `unrealizedPnl` but does not supply position value or liquidation price, so reconciliation preserves richer WebSocket values and derives notional from size times the current mark only before a WebSocket snapshot exists.
- **Fake positions**: Both REST and WebSocket paths generate zero-contract placeholder positions for all active markets without real positions

### Balances

- **WebSocket**: `handleBalances()` processes cash/flex/auxiliary account structures and calls `store.update({ balance })`
- **REST polling**: `fetchBalance()` runs every 10 seconds in the tick cycle
- **Account types**: Cash (simple balances), Flex (multi-collateral with portfolioValue/availableMargin), and per-market margin accounts (`fi_*`)

### Tick Cycle (`kraken.exchange.ts`)

- Runs every **500ms** (`tickInterval` from metadata)
- On first tick (`count === 0`) and every 20th tick (10 seconds):
  1. `fetchLeverageSettings()` -- fetches leverage preferences first
  2. `Promise.all([fetchOrders(), fetchPositions(), fetchBalance()])` -- parallel fetch
- Uses `loop()` utility for recursive scheduling (avoids `setInterval` browser throttling)
- Stores loaded state flags after each successful fetch

## Exchange Metadata

```typescript
{
  exchangeName: 'kraken',
  prettyName: 'Kraken Futures',
  enabled: true,
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: false,
  positionModes: [PositionMode.OneWay],
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  supportsMaxCrossLeverage: false,  // Leverage slider hidden in cross mode
  supportsPartialStops: true,
  supportsInfinityStops: false,
  supportsTPSLCloseAttribute: false,
  supportsTrailingStopLoss: false,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.LastPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  tickInterval: 500,
  orderbookTickDelay: 60,
  tvMaxGetBarFetches: 10,
  supportsReadOnlyKey: false,
  enforceWsTunnel: true,
  latencyDomain: 'futures.kraken.com',
  features: { kline: true, orderbook: true, orders: true, positions: true, balance: true },
}
```

## Testnet / Demo

- **Testnet**: `supportsTestnet: false` in metadata (testnet toggle hidden in UI)
- **Testnet REST URL**: `https://demo-futures.kraken.com/derivatives/api`
- **Testnet WS URL**: `wss://demo-futures.kraken.com/ws/v1`
- **Code support**: The code fully supports testnet URLs (toggled by `options.testnet`), but the metadata disables it in the UI
- **Charts API testnet**: `https://demo-futures.kraken.com/api/charts/v1/...`
- **Known differences**: None documented; same API structure as production

## Known Quirks / Implementation Notes

1. **OHLCV via polling only**: Kraken Futures does NOT support OHLCV via WebSocket. Candles are fetched via REST polling with recursive `setTimeout` at 5-second intervals (`kraken.exchange.ts`). The `listenOHLCV` method in the public WS returns a no-op.

2. **Orderbook quantities in contracts**: Raw WebSocket orderbook quantities are in contracts, NOT notional. Must multiply by `contractSize` from market data for correct display (`kraken.ws-public.ts`).

3. **Field name differences from other exchanges**:
   - `limitPrice` instead of `price` for limit orders
   - `size` instead of `quantity`
   - `orderId` in REST responses vs `order_id` in WebSocket
   - WebSocket positions use `instrument` not `symbol`, `balance` not `size`
   - WebSocket orders use `direction` (0/1) not `side` (buy/sell)

4. **Form-encoded POST bodies**: All authenticated POST requests use `application/x-www-form-urlencoded` format with RFC 1738 encoding (`+` for spaces). This applies even though `Content-Type: application/json` is common elsewhere.

5. **Alphabetical parameter sorting**: Query string parameters MUST be sorted alphabetically for signature validation (`kraken.api.ts`, `sort` function).

6. **Cancel-and-recreate for TP/SL**: The `editorder` endpoint does not reliably work for stop and take-profit orders. The implementation always cancels and recreates them, preserving the original `triggerSignal` from normalized order state or the raw REST/WS payload. Kraken's exact wire values are `last`, `mark`, and `spot`; the internal `IndexPrice` enum represents Kraken's `spot` reference and must serialize back to `spot`, never `index`. Missing and newly introduced trigger values fail before cancellation so a protective order cannot silently change trigger semantics. If replacement placement fails after cancellation, the thrown error explicitly warns that the original protective order is already gone.

7. **Margin type detection via `maxFixedLeverage`**: If the `maxFixedLeverage` field exists on a position, it is Isolated margin. If undefined, it is Cross margin. This is cached in `marginTypePreferences` Map.

8. **Cross margin leverage**: In cross margin mode, `supportsMaxCrossLeverage: false` means the leverage slider is hidden. Leverage defaults to `market.limits.leverage.max` for cross positions.

9. **Changing margin type**:
   - To **Isolated**: PUT `/v3/leveragepreferences` with `symbol` and `maxLeverage`
   - To **Cross**: PUT `/v3/leveragepreferences` with `symbol` only (no `maxLeverage` parameter)
   - After change: fetches positions to refresh state

10. **Fake positions for all markets**: Both long and short zero-contract positions are created for every active perpetual market without a real position, enabling the UI to display all tradeable symbols (`kraken.exchange.ts`).

11. **Two axios clients**: `xhr` (rate-limited at 5 RPS) for data fetching, `unlimitedXHR` for order operations to avoid delays on critical trading actions.

12. **Candle timestamp conversion**: Kraken charts API returns timestamps in milliseconds; the code divides by 1000 to store in seconds (`kraken.exchange.ts`).

13. **OHLCV cache and change detection**: The polling mechanism caches the last candle and only emits callbacks when timestamp, close, or volume changes (`kraken.exchange.ts`).

14. **Position side from balance sign**: WebSocket position `balance` field is positive for long, negative for short. The `contracts` field is the absolute value (`kraken.ws-private.ts`).

15. **Leverage settings cache**: `leverageSettings` and `marginTypePreferences` are in-memory Maps on the exchange instance. They survive tick cycles but are lost on reconnection/restart.

16. **Two funding-rate fields, only one is fractional**: Kraken Futures publishes `funding_rate` / `fundingRate` **and** `relative_funding_rate`. The first is **quote currency per contract** (e.g. `0.207` = 0.207 USD per PF_XBTUSD contract), NOT a fractional rate. The fractional rate is `relative_funding_rate`, and it exists only on the WS `ticker` feed — `/v3/tickers` does not expose it. Misreading this renders BTC funding as "20.72%" instead of "0.0006%".
    - **WS**: read `relative_funding_rate` directly (`kraken.ws-public.ts::handleTicker`)
    - **REST**: approximate as `fundingRate / last`; the first WS tick per symbol overwrites it with the real value (`kraken.exchange.ts::fetchTickers`)

17. **`/v3/tickers` omits `last` entirely for zero-volume perps**: Illiquid contracts like `PF_PONKEUSD` have live bid/ask/markPrice but no recent trades, so Kraken drops the `last` field from REST ticker responses. The code falls back to `mark` (`last = realLast > 0 ? realLast : mark`) in both REST (`kraken.exchange.ts::fetchTickers`) and WS (`kraken.ws-public.ts::handleTicker`). Without this fallback, the UI renders "-" for a tradeable pair whose fair value is perfectly well known.

18. **REST ticker field names must be read directly, not via `v()` helper**: `/v3/tickers` publishes `markPrice`, `indexPrice`, `vol24h`, `volumeQuote`, `change24h`, `openInterest` in camelCase. The `v(obj, key)` helper (`src/utils/get-key.ts`) tries literal / snake / camel forms of the **key** but does NOT normalize the **haystack** — so `v(t, 'mark')` silently returns `undefined` when the field is `markPrice`. Read these fields directly: `parseFloat(t.markPrice)`, `parseFloat(t.vol24h)`, etc. This was silently broken since the adapter was written: mark/index/volume/quoteVolume on every Kraken REST fetch returned 0 until the first WS tick overwrote them.

## Error Handling

### REST API Response Interceptor (`kraken.api.ts`)

- **Success check**: Responses with `result: 'error'` and an `error` field are thrown as errors
- **Specific error mapping**: Authentication errors, nonce errors, permission errors, and signature errors get user-friendly messages
- **HTTP status handling**: 401 (auth failed), 403 (forbidden/IP whitelist), 429 (rate limit) get specific messages
- **Response preservation**: Kraken-specific errors retain the original axios response on `error.response` (and the original transport error as `cause` for HTTP failures), so account validation and order paths can still inspect status and response bodies after message normalization.
- **Network retry**: 3 automatic retries on network errors via `axios-retry`

### Order Placement Errors (`kraken.exchange.ts`)

Specific error patterns are detected and given friendly messages:

- `invalidSize` -> "Invalid order size... Check minimum order size and precision"
- `insufficientAvailableFunds` -> "Insufficient available funds"
- `authentication`/`apikey` -> "Authentication failed... ensure General API is set to Full access"
- `permission` -> "Permission denied"
- `size`/`quantity`, `price`, `margin`/`balance` -> context-specific messages

### Order Placement Validation

Two-level success check for `sendorder` (`kraken.exchange.ts`):

1. `data.result === 'success'` -- acknowledges receipt
2. `data.sendStatus.status` must be `'placed'` or `'filled'` -- actual order acceptance
   - Other values (e.g., `invalidSize`, `insufficientAvailableFunds`) are rejections

### Account Validation (`kraken.exchange.ts`)

Validates credentials by calling `/v3/accounts`. Checks for:

- Invalid API key, invalid nonce, permission denied, invalid signature, IP whitelist
- HTTP status codes: 401, 403, 429, 400
- Network/timeout errors, CORS errors
- Empty accounts (may indicate missing permissions)

### WebSocket Error Handling

- **Auth errors**: Logged at `LogSeverity.Error`, `authenticated` set to `false` (`kraken.ws-private.ts`)
- **Subscription errors**: Logged via `console.error` in public WS (`kraken.ws-public.ts`)
- **Parse errors**: Caught and logged with raw data for debugging (`kraken.ws-public.ts`)
- **Throttled order fetch failures**: Silent catch, will retry on next event (`kraken.ws-private.ts`)

## Testing / Validation

- [ ] Account validation with API keys (auth flow)
- [ ] Balance fetching: cash account, flex (multi-collateral) account, per-market margin accounts
- [ ] Order placement: limit, market, stop (`stp`), take profit
- [ ] Order modification: edit for limit orders, cancel-and-recreate for TP/SL
- [ ] Order cancellation: single and batch
- [ ] Position tracking: correct OneWay mode, margin type detection (cross/isolated via `maxFixedLeverage`)
- [ ] Margin type switching: cross to isolated and back
- [ ] Leverage setting in isolated mode
- [ ] Orderbook: snapshot + delta updates, quantities multiplied by contractSize
- [ ] OHLCV chart data via REST polling (not WebSocket)
- [ ] Ticker updates via WebSocket (ticker feed)
- [ ] Fake positions generated for all active perpetual markets
- [ ] WebSocket reconnection: public and private re-authentication
- [ ] Throttled order fetching after WebSocket events (1-second debounce)
- [ ] Nonce uniqueness under rapid request scenarios

---

# Kraken Spot Integration

## Exchange Information

- **Pretty Name**: Kraken Spot
- **Exchange Name (internal)**: `krakenspot`
- **Type**: Centralized exchange (CEX), spot trading
- **Position Mode**: One-way only (spot has no true positions)
- **Margin Types**: Isolated only (no margin trading in this integration)
- **Status**: Enabled in dev and preview; preview is a production lane with real users, while prod remains disabled.

## API Documentation

- **Spot REST API**: https://docs.kraken.com/api/docs/guides/spot-rest-intro/
- **Spot WebSocket v2**: https://docs.kraken.com/api/docs/guides/spot-ws-intro/
- **Asset Pairs**: https://docs.kraken.com/api/docs/rest-api/get-tradable-asset-pairs
- **Add Order**: https://docs.kraken.com/api/docs/rest-api/add-order
- **Amend Order**: https://docs.kraken.com/api/docs/rest-api/amend-order/
- **Atomic Amend Guide**: https://docs.kraken.com/exchange/guides/general/amends
- **CCXT Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/kraken.ts

## File Structure

| File                          | Purpose                                                            |
| ----------------------------- | ------------------------------------------------------------------ |
| `krakenspot.api.ts`           | REST API client with Spot HMAC-SHA512 auth                         |
| `krakenspot.exchange.ts`      | Main exchange class extending `BaseExchange`                       |
| `krakenspot.types.ts`         | Endpoint constants, URL definitions, type mappings, asset aliases  |
| `krakenspot.ws-private.ts`    | Private WebSocket v2: token auth, executions, balances             |
| `krakenspot.ws-public.ts`     | Public WebSocket v2: ticker, orderbook, OHLC                      |

## Key Differences from Kraken Futures

| Aspect              | Kraken Futures (`kraken`)                    | Kraken Spot (`krakenspot`)                    |
| ------------------- | -------------------------------------------- | --------------------------------------------- |
| Base URL            | `futures.kraken.com/derivatives/api`         | `api.kraken.com`                              |
| WS Protocol         | v1 event-based                               | v2 method/params JSON                         |
| WS Public URL       | `wss://futures.kraken.com/ws/v1`             | `wss://ws.kraken.com/v2`                      |
| WS Private URL      | Same as public                               | `wss://ws-auth.kraken.com/v2`                 |
| WS Auth             | Challenge/response signing                   | Token-based (REST `GetWebSocketsToken`)       |
| REST Auth Header    | `APIKey` + `Authent` + `Nonce`               | `API-Key` + `API-Sign`                        |
| REST Auth Signing   | `SHA256(postData + nonce + '/api/' + path)`  | `SHA256(nonce + postBody)` then HMAC with path|
| Symbol Format       | `PF_BTCUSD`                                  | `BTC/USD` (wsname)                            |
| Positions           | Real perpetual positions                     | Fake zero-entries (spot has no positions)      |
| Leverage            | 1x-100x                                      | 1x only                                       |
| Funding Rate        | Yes (`relative_funding_rate`)                | No                                            |
| Mark/Index Price    | From exchange                                | Uses last trade price                         |
| OHLC via WS         | Not supported (REST polling)                 | Supported via WS v2 `ohlc` channel            |
| Error Format        | `{ result: "error", error: "..." }`          | `{ error: ["EAPI:...", ...], result: {} }`    |

## Authentication

### REST API (`krakenspot.api.ts`)

Signature algorithm: `base64(HMAC_SHA512(base64_decode(secret), urlPath + SHA256(nonce + postBody)))`

1. Generate nonce: `(virtualClock.getCurrentTimeMs() * 1000) + (nonceCounter++ % 1000)`
2. Build POST body as form-encoded string including nonce
3. SHA-256 hash of `(nonce + postBody)` as UTF-8
4. Base64-decode the API secret
5. Construct HMAC payload: `urlPath (bytes) + sha256Hash (bytes)`
6. HMAC-SHA512 sign the payload with decoded secret
7. Base64-encode the signature
8. Set headers: `API-Key`, `API-Sign`, `Content-Type: application/x-www-form-urlencoded`

### WebSocket Authentication (`krakenspot.ws-private.ts`)

Token-based auth flow:

1. On every socket open/reconnect, call `POST /0/private/GetWebSocketsToken` via REST; never reuse a token cached by the previous connection
2. Receive `{ result: { token: "..." } }`
3. Include `token` in every private channel subscription's `params`
4. Token must be used within 15 minutes; stays valid while any subscription is active

## REST API Endpoints

| Constant          | Path                             | Method | Auth | Purpose               |
| ----------------- | -------------------------------- | ------ | ---- | --------------------- |
| `ASSET_PAIRS`     | `/0/public/AssetPairs`           | GET    | No   | Tradeable pairs       |
| `TICKER`          | `/0/public/Ticker`               | GET    | No   | Ticker data           |
| `OHLC`            | `/0/public/OHLC`                 | GET    | No   | Candlestick data      |
| `ORDERBOOK`       | `/0/public/Depth`                | GET    | No   | Orderbook snapshot    |
| `BALANCE`         | `/0/private/Balance`             | POST   | Yes  | Account balances      |
| `OPEN_ORDERS`     | `/0/private/OpenOrders`          | POST   | Yes  | Open orders           |
| `ADD_ORDER`       | `/0/private/AddOrder`            | POST   | Yes  | Place order           |
| `AMEND_ORDER`     | `/0/private/AmendOrder`          | POST   | Yes  | Atomically amend order |
| `CANCEL_ORDER`    | `/0/private/CancelOrder`         | POST   | Yes  | Cancel order          |
| `WS_TOKEN`        | `/0/private/GetWebSocketsToken`  | POST   | Yes  | WebSocket auth token  |

## WebSocket v2 Channels

### Public (wss://ws.kraken.com/v2)

- **ticker**: Bid, ask, last, volume, change_pct. Trigger: trades or BBO.
- **book**: Level 2 orderbook with snapshot + delta updates. Depth: 10/25/100/500/1000.
- **ohlc**: OHLC candles with snapshot + updates. Intervals: 1/5/15/30/60/240/1440/10080.

Unsupported UI intervals use the nearest lower supported Kraken interval. In particular, both `6h` and `12h` request Kraken's `240`-minute candles; never send the rejected `720` interval.

### Private (wss://ws-auth.kraken.com/v2)

- **executions**: Order lifecycle (new, filled, canceled, expired). Includes fills with cum_qty/avg_price.
- **balances**: Per-asset balance updates with wallet breakdown.

## Symbol Format

- REST pair names: `XXBTZUSD`, `XETHZUSD` (non-standard Kraken format)
- WebSocket v2 symbols: `BTC/USD`, `ETH/USD` (standard format with `/`)
- We use `wsname` from AssetPairs as the canonical symbol (matches WS v2)
- `pairName` (REST key) is stored in `market.info.pairName` for REST API calls
- `altname` (for example `XBTUSD`) is stored from AssetPairs and indexed alongside `wsname`, `pairName`, and `prettySymbol` when reconciling open orders. OpenOrders descriptions use this identity, so matching only canonical `XBT/USD` would orphan BTC orders.

## Asset Name Normalization

Kraken uses non-standard asset codes. The `normalizeAsset()` function in `krakenspot.types.ts` handles:
- `XBT` / `XXBT` -> `BTC`
- `XETH` -> `ETH`
- `ZUSD` -> `USD`
- Many others (see `ASSET_ALIASES` map)

## Spot-Specific Behavior

1. **No positions**: `fetchPositions()` returns fake zero-contract entries for all markets
2. **No leverage**: All markets have `leverage: { min: 1, max: 1 }`
3. **No funding**: `fundingRate` and `fundingTime` are always 0
4. **No mark/index**: Uses `last` trade price as mark and index price
5. **Balance = portfolio**: Total USD balance includes estimated USD value of all held assets
6. **No reduce-only**: All `reduceOnly` fields are false
7. **Client order IDs**: `AddOrder` uses a generated 32-character lowercase hexadecimal short UUID in `cl_ord_id`; it must not send `userref`, because Kraken makes the fields mutually exclusive and `userref` is int32-only. Kraken `txid`s remain the returned, update/cancel, and reconciliation identities.
8. **Native order amendments**: Reconciled plain limit, stop-loss, and take-profit orders use `/0/private/AmendOrder` by canonical Kraken `txid`. Quantity changes send the requested total base-asset quantity as `order_qty`; limit price changes use `limit_price`; trigger changes use `trigger_price`. Post-only limit price changes set `post_only: true`, so an amend that would take liquidity is rejected. The returned `amend_id` identifies the amend transaction only, so `updateOrder` returns the unchanged order `txid`.
9. **Ambiguous edits fail closed**: Market, trailing-stop, stop-limit, take-profit-limit, IOC, and unreconciled orders are rejected before any REST write. Do not restore cancel-first replacement: it can lose a live protective order, and the normalized model cannot unambiguously represent both prices of Kraken's triggered-limit orders.

## Error Handling

Kraken Spot returns errors as arrays: `{ error: ["EAPI:Invalid key", ...], result: {} }`

Common error codes:
- `EAPI:Invalid key` — Invalid API key
- `EAPI:Invalid nonce` — Nonce too low (clock sync issue)
- `EAPI:Invalid signature` — Wrong API secret
- `EGeneral:Permission denied` — Missing permissions
- `EOrder:Insufficient funds` — Not enough balance
- `EOrder:Rate limit exceeded` — Too many orders

## Exchange Metadata

```typescript
{
  exchangeName: 'krakenspot',
  prettyName: 'Kraken Spot',
  enabled: false,          // Disabled in production until fully tested
  enabledInDev: true,
  enableInPreview: true,
  supportsTestnet: false,  // Kraken Spot has no testnet
  positionModes: [PositionMode.OneWay],
  marginTypes: [MarginType.Isolated],
  hideMarginTypeSelector: true,
  hidePositionModeSelector: true,
  doesNotSupportOpenInterest: true,
  enforceWsTunnel: true,
  latencyDomain: 'api.kraken.com',
  tickInterval: 500,
}
```

## Testing / Validation

- [ ] Account validation with Kraken Spot API keys
- [ ] Market loading from AssetPairs endpoint
- [ ] Ticker fetching and WebSocket updates
- [ ] Balance fetching (multi-asset with USD estimation)
- [ ] Order placement: limit, market, stop-loss, take-profit
- [ ] Order modification via native atomic amend and cancellation
- [ ] Orderbook WebSocket subscription
- [ ] OHLC via WebSocket v2 (unlike Futures which uses REST polling)
- [ ] WebSocket token authentication flow
- [ ] WebSocket reconnection and token refresh
