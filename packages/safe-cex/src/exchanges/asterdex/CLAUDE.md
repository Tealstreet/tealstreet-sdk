# AsterDEX Exchange Integration

## Exchange Information

AsterDEX is a decentralized perpetual futures exchange with Binance-style market semantics.  
This integration now supports a **dual API model**:

- **V1**: API key + HMAC-SHA256 signing (legacy/default path).
- **V3**: API wallet/agent signing (ECDSA over keccak payload with microsecond nonce).

The uplift is intentionally backward-compatible: existing V1 accounts continue to work unchanged while V3 is available for migration.

## API Documentation

- **Futures API docs**: <https://github.com/asterdex/api-docs/blob/master/aster-finance-futures-api.md>
- **V1 vs V3 overview**: <https://github.com/asterdex/api-docs/blob/master/Aster%20API%20Overview.md>
- **CCXT REST reference**: <https://github.com/ccxt/ccxt/blob/master/ts/src/aster.ts>
- **CCXT WS reference**: <https://github.com/ccxt/ccxt/blob/master/ts/src/pro/aster.ts>
- **Main website**: <https://asterdex.com>

## File Structure

- `asterdex.exchange.ts`: main exchange class; resolves API version and routes fapi calls via `getFapiPath()`
- `asterdex.api.ts`: REST client with V1+V3 signing logic, timestamp retry, and multiple rate-limited clients
- `asterdex.types.ts`: exchange types plus version/auth helpers (`resolveAsterDEXApiVersion`, `hasAsterDEXV3AuthContext`, `getAsterDEXFapiPath`)
- `asterdex.virtualClock.ts`: Aster-specific time sync with V1/V3 time endpoint fallback and proxy fallback
- `asterdex.ws-public.ts`: public WebSocket (tickers, depth, trades, mark prices, klines)
- `asterdex.ws-private.ts`: private WebSocket (listen key lifecycle, order/account events)
- `__tests__/asterdex.api.test.ts`: version-selection and deterministic signing fixtures (V1 and V3)

## API Version Selection

Version is resolved in `resolveAsterDEXApiVersion()` using this order:

1. **Explicit override**: `options.extra.asterdexApiVersion` (`'v1' | 'v3'`)
2. **Auto-V3**: enabled only when all V3 auth fields are present and valid:
   - `options.auth.userAddress`
   - `options.auth.signerAddress`
   - valid 64-byte hex private key (`options.auth.privateKey` or `options.secret`)
3. **Fallback**: `v1`

`AsterDEXExchange` stores this as `apiVersion` and all REST calls route through `getFapiPath(endpoint, version?)`.

## Authentication

### V1 (Legacy / Default)

- Header: `X-MBX-APIKEY`
- Params include `recvWindow` + `timestamp`
- Signature: HMAC-SHA256 over query string

### V3 (Wallet / Agent Model)

- Header: `X-MBX-APIKEY`
- Params include `recvWindow`, `timestamp`, plus:
  - `user`
  - `signer`
  - `nonce` (microseconds, monotonic)
- Signature flow:
  1. Convert business params to string values and sort keys
  2. ABI encode `[string payloadJson, address user, address signer, uint256 nonce]`
  3. keccak hash encoded payload
  4. apply Ethereum signed-message hash
  5. sign via secp256k1 ECDSA

### Common Request Behavior

- All private request parameters are sent as query params (including POST/PUT/DELETE).
- Request body is deleted for signed requests.
- Timestamp recovery (`-1021`) resyncs clock and retries once with fresh signature.

### Public Endpoints (No Auth)

Both V1 and V3 public prefixes are treated as public (examples):

- `/fapi/v1/*` public market endpoints
- `/fapi/v3/*` public market endpoints

### Time Synchronization

`AsterDEXVirtualClock`:

- syncs on startup and every 5 minutes
- attempts preferred version first, then fallback:
  - preferred `v1`: `/fapi/v1/time` then `/fapi/v3/time`
  - preferred `v3`: `/fapi/v3/time` then `/fapi/v1/time`
- also attempts proxy fallback via `read-fleet-jp.tealstreet.io`
- falls back to local UTC time if all attempts fail

## Startup Sequence

`start()` flow:

1. resolve API mode (`v1`/`v3`) and initialize APIs
2. sync exchange clock with preferred API version
3. validate account credentials (private mode only)
4. fetch markets + tickers (unless external public data is configured)
5. fetch positions + open orders (private mode only; failures are non-fatal)
6. fetch current position mode via `GET /fapi/{version}/positionSide/dual` and set `store.options.isHedged`
7. connect public WS (unless external public data is configured)
8. connect private WS (private mode only)
9. start 5-second tick loop (private mode only)

Private-mode checks are based on `!isPublicDataOnly` (which now respects V3 auth readiness via overridden `canSign`).

## REST API Endpoints (Version-Aware)

Most endpoints use `/fapi/{apiVersion}/...` where `apiVersion` is `v1` or `v3`.

| Endpoint (logical) | V1 Path | V3 Path | Notes |
| --- | --- | --- | --- |
| account | `/fapi/v4/account` | `/fapi/v3/account` | V1 keeps v4 account endpoint |
| position risk | `/fapi/v1/positionRisk` | `/fapi/v3/positionRisk` | signed |
| open orders | `/fapi/v1/openOrders` | `/fapi/v3/openOrders` | signed |
| order place/cancel | `/fapi/v1/order` | `/fapi/v3/order` | signed |
| exchange info | `/fapi/v1/exchangeInfo` | `/fapi/v3/exchangeInfo` | public |
| leverage bracket | `/fapi/v1/leverageBracket` | `/fapi/v3/leverageBracket` | signed |
| ticker 24h | `/fapi/v1/ticker/24hr` | `/fapi/v3/ticker/24hr` | public |
| klines | `/fapi/v1/klines` | `/fapi/v3/klines` | public |
| depth | `/fapi/v1/depth` | `/fapi/v3/depth` | public |
| trades | `/fapi/v1/trades` | `/fapi/v3/trades` | public |
| user trades | `/fapi/v1/userTrades` | `/fapi/v3/userTrades` | signed |
| leverage | `/fapi/v1/leverage` | `/fapi/v3/leverage` | signed |
| position mode | `/fapi/v1/positionSide/dual` | `/fapi/v3/positionSide/dual` | signed |
| margin type | `/fapi/v1/marginType` | `/fapi/v3/marginType` | signed |
| listen key | `/fapi/v1/listenKey` | `/fapi/v3/listenKey` | private WS lifecycle |
| time | `/fapi/v1/time` | `/fapi/v3/time` | public |

## WebSocket Implementation

### Network Configuration

| Component  | URL                                                    |
| ---------- | ------------------------------------------------------ |
| Public WS  | `wss://fstream.asterdex.com/stream?requestId=<nanoid>` |
| Private WS | `wss://fstream.asterdex.com/ws/<listenKey>`            |

Note: Both share the same host `fstream.asterdex.com` but use different paths.

### Public WebSocket (`asterdex.ws-public.ts`)

**On connect** (`onOpen`, `asterdex.ws-public.ts:86-131`):

1. Sends initial ping
2. Subscribes to aggregate streams only (to avoid disconnections from too many individual subscriptions):
   - `!ticker@arr` - All tickers in batch
   - `!markPrice@arr@1s` - All mark prices at 1-second intervals
3. Resubscribes to any previously-registered on-demand topics (depth, kline streams) from the `topics` map
4. Emits `wsReconnected` if reconnecting WebSocket is enabled
5. Sets `wsReady = true` after subscriptions complete

**Subscription format**:

```json
{ "method": "SUBSCRIBE", "params": [...streams], "id": <incrementing_id> }
```

**On-demand subscriptions**:

- `<symbol>@depth@500ms` - Order book depth updates (500ms throttle)
- `<symbol>@kline_<interval>` - Kline/candlestick updates
- `<symbol>@aggTrade` - Aggregate trade updates

**Ping mechanism** (`asterdex.ws-public.ts:182-188`):

- Sends `{ "id": 42, "method": "LIST_SUBSCRIPTIONS" }` as application-level ping
- Response with `id: 42` is treated as pong

**Message routing** (`onMessage`, `asterdex.ws-public.ts:190-249`):

1. Check for pong response (id === 42)
2. Check for subscription confirmation (result === null)
3. Check for error response
4. Handle stream data with `stream` field:
   - `!markPrice@arr@1s` / `!markPrice@arr`: Iterate array, call `handleMarkPriceUpdate` per item
   - `!ticker@arr`: Iterate array, call `handleTickerUpdate` per item
   - Other streams: Route via `handleStreamData` which maps stream type to event handler
5. Handle batch updates (array format, legacy)
6. Handle single events (object with `.e` field)

**Orderbook subscription** (`listenOrderBook`, `asterdex.ws-public.ts:560-660`):

- Stores callback under multiple key formats (original symbol, lowercase, full stream name, basic depth)
- Tracks in `topics` for reconnection
- Uses retry loop: up to 50 attempts with 100ms intervals to wait for WS connection
- Returns unsubscribe function that cleans up all callback references and topics

**Kline subscription** (`subscribeToKline`, `asterdex.ws-public.ts:663-708`):

- Wait-for-connection with exponential backoff: `100ms * 1.5^(floor(retryCount/10))`, max 2000ms
- If connected but not initialized, queues subscription in `pendingSubscriptions`

**Reconnection behavior** (`onClose`, `asterdex.ws-public.ts:71-84`):

- Resets `wsReady = false` and clears `pendingSubscriptions`
- Does NOT clear `topics` (needed for reconnection resubscription)
- Calls parent class `onClose` via prototype chain

### Private WebSocket (`asterdex.ws-private.ts`)

**Connection** (`connectAndSubscribe`, `asterdex.ws-private.ts:29-46`):

1. Fetches listen key via `POST /fapi/{version}/listenKey` (resolved by `getFapiPath`)
2. Constructs URL: `wss://fstream.asterdex.com/ws/<listenKey>`
3. No explicit subscription needed - events auto-sent once connected

**Listen key management** (`asterdex.ws-private.ts:391-407`):

- Created via `POST /fapi/{version}/listenKey`
- Refreshed every 30 minutes via `PUT /fapi/{version}/listenKey` (key valid for 60 minutes)
- Refresh scheduled via `setTimeout`, re-scheduled after each refresh

**On connect** (`onOpen`):

1. Sends initial ping
2. If `ENABLE_RECONNECTING_WEBSOCKET`, calls `fetchAndSetPrivateSnapshot(10)` to re-fetch orders (10 retry attempts)

**Ping**: Same `{ "id": 42, "method": "LIST_SUBSCRIPTIONS" }` pattern as public WS

**Event types handled**:

| Event                     | Handler                       | Description                                 |
| ------------------------- | ----------------------------- | ------------------------------------------- |
| `ORDER_TRADE_UPDATE`      | `handleOrderTradeUpdate`      | Futures order creates, fills, cancellations |
| `ACCOUNT_UPDATE`          | `handleAccountPositionUpdate` | Position and balance changes                |
| `executionReport`         | `handleExecutionReport`       | Legacy/spot order execution reports         |
| `outboundAccountPosition` | `handleAccountUpdate`         | Legacy account balance updates              |

**ORDER_TRADE_UPDATE handling** (`asterdex.ws-private.ts:138-227`):

- `NEW`: Adds order to store via `addOrUpdateOrder`
- `PARTIALLY_FILLED` / `FILLED`: Emits `fill` event with `ap` (average price) and `l` (last filled qty)
- `FILLED`: Updates status to `Closed`, remaining to 0
- `CANCELED` / `REJECTED` / `EXPIRED`: Updates status to `Canceled`

**ACCOUNT_UPDATE handling** (`asterdex.ws-private.ts:252-389`):

- Processes position updates from `data.a.P` array
- In one-way mode (`ps === 'BOTH'`), determines side from position amount sign
- For non-zero positions: Updates existing position or logs that `fetchPositions()` will create it
- For zero positions: Removes all positions for that symbol (position closed)
- Handles position flip: If new side doesn't match existing, removes opposite side position
- **Delayed sync**: Schedules a 1-second delayed `fetchPositions()` + `fetchOrders()` to ensure full sync after WS update
- Processes balance updates from `data.a.B` array (wallet balance, cross wallet balance)

## Order Types and Mappings

| Internal Type                | AsterDEX API Value     | Notes                                  |
| ---------------------------- | ---------------------- | -------------------------------------- |
| `OrderType.Limit`            | `LIMIT`                | Requires `price` and `timeInForce`     |
| `OrderType.Market`           | `MARKET`               | No `price` or `timeInForce`            |
| `OrderType.StopLoss`         | `STOP_MARKET`          | Requires `stopPrice` and `workingType` |
| `OrderType.TakeProfit`       | `TAKE_PROFIT_MARKET`   | Requires `stopPrice` and `workingType` |
| `OrderType.TrailingStopLoss` | `TRAILING_STOP_MARKET` |                                        |

Additional reverse mappings exist for `STOP` -> `StopLoss`, `TAKE_PROFIT` -> `TakeProfit`, `TRAILING_STOP` -> `TrailingStopLoss` (`asterdex.exchange.ts:1020-1032`).

### Time In Force

| Internal TIF        | AsterDEX Value | Description         |
| ------------------- | -------------- | ------------------- |
| `GoodTillCancel`    | `GTC`          | Good Till Cancel    |
| `ImmediateOrCancel` | `IOC`          | Immediate or Cancel |
| `FillOrKill`        | `FOK`          | Fill or Kill        |
| `PostOnly`          | `GTX`          | Good Till Crossing  |

### Order Status Mapping

| API Status         | Internal Status        |
| ------------------ | ---------------------- |
| `NEW`              | `OrderStatus.Open`     |
| `PARTIALLY_FILLED` | `OrderStatus.Open`     |
| `FILLED`           | `OrderStatus.Closed`   |
| `CANCELED`         | `OrderStatus.Canceled` |
| `REJECTED`         | `OrderStatus.Canceled` |
| `EXPIRED`          | `OrderStatus.Canceled` |

## Order Lifecycle

### Regular Orders (Limit/Market)

1. **Place**: `POST /fapi/{version}/order` via `secondaryAPI` (5 RPS rate limit) with params: `symbol`, `side`, `type`, `quantity`, `price` (limit only), `timeInForce` (non-market), `reduceOnly`
2. **Close position orders**: When `amount === 0` for TP/SL, sets `closePosition='true'` instead of `quantity`, and omits `reduceOnly` (`asterdex.exchange.ts:545-561`)
3. **Update** (`updateOrder`, `asterdex.exchange.ts:639-680`): Uses **cancel-and-replace** pattern (no PUT endpoint). For TP/SL orders, maps `Order.price` to `triggerPrice` since the price field holds the trigger price.
4. **Cancel**: `DELETE /fapi/{version}/order` with `symbol` and `orderId` params via `secondaryAPI`

### Bracket Orders (TP/SL with entry)

When `takeProfit` or `stopLoss` is provided with a limit/market order (`asterdex.exchange.ts:477-526`):

1. Splits into array of orders: main entry + SL + TP
2. TP/SL orders have opposite side, `reduceOnly: true`
3. All placed sequentially via `placeOrders()`

### Batch Operations

`placeOrders` and `cancelOrders` iterate individually (no batch API) following BingX approach (`asterdex.exchange.ts:608-637, 682-714`). Continues on individual failures; only throws if ALL operations fail.

### Stop Price Trigger Types (`asterdex.exchange.ts:583-592`)

- **TakeProfit**: Defaults to `CONTRACT_PRICE` (LastPrice), uses `MARK_PRICE` only if explicitly set to `MarkPrice`
- **StopLoss**: Defaults to `MARK_PRICE` (MarkPrice), uses `CONTRACT_PRICE` only if explicitly set to `LastPrice`

## Symbol Format

- **Format**: Concatenated base + quote: `BTCUSDT`, `ETHUSDT`
- **No transformation needed** - symbols pass through directly (unlike Orderly which uses `PERP_` prefix)
- Market ID is the symbol itself (e.g., `BTCUSDT`)

## Data Precision & Conversion

### Market Precision (`asterdex.exchange.ts:940-974`)

- `pricePrecision` and `quantityPrecision` from `exchangeInfo` are integer digits (e.g., `2` means 2 decimal places)
- Converted to decimal step size via `precisionToString()` utility (e.g., `2` -> `0.01`)
- **Tick size**: Extracted from `PRICE_FILTER` filter's `tickSize` field. Falls back to `market.precision.price` (`asterdex.exchange.ts:1004-1014`)
- **Lot size**: `minQty` and `maxQty` from `LOT_SIZE` filter
- Price rounding uses `adjust()` utility with the tick size, not the decimal precision directly

### Balance Calculation (`asterdex.exchange.ts:177-201`)

- `total`: `totalWalletBalance` from account endpoint (`/fapi/v4/account` on V1, `/fapi/v3/account` on V3)
- `free`: `availableBalance`
- `used`: `totalInitialMargin + totalOpenOrderInitialMargin`
- `upnl`: `totalUnrealizedProfit`

### Leverage Brackets (`asterdex.exchange.ts:341-377`)

- Fetched via signed `GET /fapi/{version}/leverageBracket` (`primaryAPI`) during market loading
- Max leverage is from the first bracket (highest leverage tier) per symbol
- Default leverage: 10 if brackets unavailable

## Rate Limiting

Four API instances with different rate limits (`asterdex.api.ts:294-323`):

- `primaryAPI` (20 req/s): Account info, positions, orders, balance, user trades, leverage brackets
- `secondaryAPI` (5 req/s): Order placement, order cancellation
- `unlimitedAPI` (20 req/s): Time-sensitive operations (same config as primary)
- `publicAPI` (10 req/s): Market data, tickers, klines, depth, public trades

### Retry Logic (`asterdex.api.ts:65-75, 272-282`)

- 3 retries on network errors, 503, and 504 responses
- Delay: `retryCount * 1000` ms (1s, 2s, 3s)
- Uses `axios-retry` library

## State Reconciliation

### Order State

- **REST polling**: Fetched every tick (5 seconds) via `GET /fapi/{version}/openOrders`
- **WebSocket updates**: `ORDER_TRADE_UPDATE` events update individual orders in real-time
  - `NEW` orders are added via `addOrUpdateOrder`
  - Status changes update existing orders via `updateOrder`
- **Reconnection**: `fetchAndSetPrivateSnapshot(10)` called on private WS reconnect

### Position State

- **REST polling**: Fetched every tick via `GET /fapi/{version}/positionRisk`
- **WebSocket updates**: `ACCOUNT_UPDATE` events provide immediate position changes
  - Updates existing positions if found in store
  - Handles position flips (removes opposite side in one-way mode)
  - Handles position close (removes all positions for symbol)
- **Delayed sync**: After WS position update, schedules a 1-second delayed REST fetch for full reconciliation (`asterdex.ws-private.ts:342-368`)
- **Fake positions**: Created for all active non-spot markets with both Long and Short sides, 0 contracts, leverage 10

### Balance State

- Fetched every tick from account endpoint (`/fapi/v4/account` on V1, `/fapi/v3/account` on V3)
- Also updated from `ACCOUNT_UPDATE` WS events (wallet balance, cross wallet balance)

## Exchange Metadata

- **Position mode**: Global account setting fetched from `GET /fapi/{version}/positionSide/dual`; supports One-Way and Hedge.
- **Default margin type**: Cross (`MarginType.Cross`)
- **Position side in one-way mode**: `"BOTH"` - side determined from amount sign
- **Position side in hedge mode**: `LONG`/`SHORT` must be sent with new orders, and `reduceOnly` must not be sent.
- **Leverage**: Per-symbol (unlike Orderly which is account-level)
- **Supports changing position mode**: `changePositionMode(hedged)` via `POST /fapi/{version}/positionSide/dual`
- **Supports changing margin type**: `changeMarginType(isolated, symbol)` via `POST /fapi/{version}/marginType`
- **Timeframe mapping** (`asterdex.exchange.ts:1051-1068`): Supports `1m, 3m, 5m, 15m, 30m, 1h, 2h, 4h, 6h, 12h, 1d, 1w`

## Testnet/Demo

| Component  | Production                          | Testnet                                     |
| ---------- | ----------------------------------- | ------------------------------------------- |
| REST API   | `https://fapi.asterdex.com`         | `https://testnet-fapi.asterdex.com`         |
| Public WS  | `wss://fstream.asterdex.com/stream` | Same (no separate testnet WS URL in config) |
| Private WS | `wss://fstream.asterdex.com/stream` | Same (no separate testnet WS URL in config) |

Note: The testnet REST URL has a `TODO: Verify testnet URL` comment (`asterdex.types.ts:5`). WebSocket URLs are not differentiated for testnet in the current implementation.

## Known Quirks / Implementation Notes

1. **Update order not supported**: AsterDEX has no PUT endpoint for orders. `updateOrder` cancels the old order and places a new one.
1. **Bracket orders**: TP/SL attached to entry orders are placed sequentially as separate orders (non-atomic).
1. **Close-position TP/SL**: `amount=0` uses `closePosition='true'` instead of `quantity`, and omits `reduceOnly`.
1. **Crossed orderbook handling**: depth processing applies multi-step uncrossing/fallback strategies and post-sort safety checks.
1. **Historical trades disabled**: `fetchHistoricalTrades` currently returns an empty array with a warning.
1. **Virtual clock**: uses dedicated Aster clock with proxy fallback and V1/V3 time endpoint fallback.
1. **Orders placed individually**: no batch endpoint usage in `placeOrders`/`cancelOrders`; failures are tolerated per-item.
1. **Trigger defaults**: TP defaults to `CONTRACT_PRICE` and SL defaults to `MARK_PRICE` (Binance-like behavior).
1. **All signed params in query**: POST/PUT/DELETE signed requests use query parameters and clear request body.
1. **Dual signing**: V1 routes use HMAC-SHA256; V3 routes use wallet signature with `user`, `signer`, and microsecond `nonce`.
1. **Binance-style 200-with-error handling**: interceptor treats negative `code` values as API errors.
1. **Ticker bid/ask source**: bid/ask are derived from orderbook streams, not 24h ticker REST payload.
1. **Private WS delayed reconciliation**: `ACCOUNT_UPDATE` schedules delayed REST refresh for positions + orders.
1. **WS error forwarding**: public WS error events route through close/reconnect flow.
1. **Position flip cleanup**: one-way mode updates remove stale opposite-side positions.
1. **O(1) ticker lookup**: uses `tickersBySymbol` map instead of linear scans.
1. **Fake position baseline leverage**: synthetic empty positions use hardcoded leverage `10`.

## Error Handling

### Timestamp Error Recovery (`asterdex.api.ts:198-226`)

- Error code `-1021` with "Timestamp" in message triggers automatic resync
- Resyncs time via `asterDEXVirtualClock.syncClientServerTime()`
- Retries the original request once (uses `__isRetryRequest` flag to prevent infinite loop)
- Strips stale signature params before retry (`timestamp`, `signature`, `recvWindow`, plus `user`/`signer`/`nonce` when present)

### HTTP Status Code Handling (`asterdex.api.ts:248-261`)

- `401`: Logs "Authentication failed. Check API credentials."
- `429`: Logs "Rate limit exceeded. Reducing request frequency."
- `503/504`: Logs "Service temporarily unavailable. Will retry." (also covered by axios-retry)

### Binance-style 200-with-error (`asterdex.api.ts:180-189`)

- Checks `response.data.code < 0` to detect errors returned with successful HTTP status
- Throws error with message: `AsterDEX API Error: <msg> (code: <code>)`

### Operation-level Error Handling

- Batch order placement: Continues placing remaining orders even if one fails; only throws if ALL fail (`asterdex.exchange.ts:608-637`)
- Batch cancel: Same pattern - continues on individual failure (`asterdex.exchange.ts:682-714`)
- Startup: Positions and orders fetch failures are caught individually, startup continues (`asterdex.exchange.ts:114-131`)
- Tick errors: Caught at tick level, logged as warning, tick loop continues (`asterdex.exchange.ts:168-170`)

## Tick Loop

- Runs every 5 seconds (hardcoded `5000` in `loop()` call, `asterdex.exchange.ts:173`)
- **Every tick**: Fetches balance (account endpoint), positions (`/fapi/{version}/positionRisk`), and orders (`/fapi/{version}/openOrders`)
- Only runs if `!isPublicDataOnly` (includes V1 key/secret or valid V3 wallet auth)
- Uses `loop()` utility for scheduling, which handles `isDisposed` check

## Testing / Validation

### Test File: `__tests__/asterdex.api.test.ts`

Current tests validate the production uplift behavior:

- V1 default selection for classic API key/secret configs
- V3 auto-selection only when full wallet auth context is present and private key is valid
- explicit API version override via `options.extra.asterdexApiVersion`
- versioned path builder behavior (`getAsterDEXFapiPath`)
- deterministic V1 HMAC signature fixture
- deterministic V3 wallet-signature fixture (sorted payload + keccak + Ethereum signed hash + ECDSA)

## CCXT Reference

Aster is available in CCXT (`ts/src/aster.ts`). This implementation follows Binance Futures-style request/response patterns while preserving Aster-specific behaviors.
