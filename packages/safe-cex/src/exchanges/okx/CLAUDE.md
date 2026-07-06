# OKX Exchange Integration

This directory contains the OKX exchange integration for Tealstreet. It focuses exclusively on **linear perpetual futures** (USDT-margined swaps). Spot and inverse contracts are filtered out.

## Exchange Information

- **Exchange**: OKX (formerly OKEx)
- **Type**: Centralized Exchange (CEX)
- **Supported products**: Linear perpetual swaps only (`ctType === 'linear'`)
- **API version**: v5
- **Base URL**: `https://www.okx.com` (`okxeu` uses `https://eea.okx.com`)
- **Latency domain**: `www.okx.com` (`okxeu`: `eea.okx.com`)

## API Documentation

When working with OKX exchange integration, consult the official API documentation:

- **Official API Documentation (v5)**: https://www.okx.com/docs-v5/
- **REST API**: https://www.okx.com/docs-v5/en/#rest-api
- **WebSocket API**: https://www.okx.com/docs-v5/en/#websocket-api

### CCXT Reference Implementation

- **REST API Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/okx.ts
- **WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/okx.ts

## File Structure

| File                 | Class/Export           | Description                                                                                                  |
| -------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------ |
| `okx.types.ts`       | Constants, mappings    | Endpoint paths, broker ID, order status/type/side/interval mappings, WebSocket URLs                          |
| `okx.api.ts`         | `createAPI()`          | REST API client factory with HMAC-SHA256 signing via axios interceptors                                      |
| `okx.exchange.ts`    | `OKXExchange`          | Main exchange class extending `BaseExchange`; all trading logic                                              |
| `okx.ws-public.ts`   | `OKXPublicWebsocket`   | Public WebSocket for market data (tickers, mark price, index, OI, funding, order book)                       |
| `okx.ws-private.ts`  | `OKXPrivateWebsocket`  | Private WebSocket for account, positions, and regular order updates                                          |
| `okx.ws-business.ts` | `OKXBusinessWebsocket` | Business WebSocket for candles (OHLCV), algo orders, and algo-advance updates; extends `OKXPrivateWebsocket` |

## Authentication

OKX requires three credentials: **API key**, **API secret**, and **passphrase**. The passphrase is mandatory and validated at API client creation time (`okx.api.ts:15` -- throws `'OKX requires a passphrase'` if missing).

### REST API Signing (`okx.api.ts:33-58`)

Signing is performed in an axios request interceptor:

1. **Public endpoint bypass**: Requests to URLs containing `'public'` skip signing entirely (`okx.api.ts:34`)
2. **Timestamp**: ISO 8601 format from `virtualClock.getCurrentTime().toISOString()` (e.g., `2024-01-15T12:30:00.000Z`)
3. **String to sign**: Concatenation of `timestamp + METHOD + url + body`
   - `url` includes query parameters (decoded via `qs.stringify`)
   - `body` is JSON-stringified data or empty string
4. **Signature**: HMAC-SHA256 of the string-to-sign using the API secret, output as **base64**
5. **Headers**:
   - `OK-ACCESS-KEY`: API key
   - `OK-ACCESS-SIGN`: Base64 HMAC-SHA256 signature
   - `OK-ACCESS-TIMESTAMP`: ISO 8601 timestamp
   - `OK-ACCESS-PASSPHRASE`: Passphrase
6. **Testnet header**: When `options.testnet` is true, adds `x-simulated-trading: 1` (`okx.api.ts:25`)

### WebSocket Authentication (Private + Business)

Both private and business WebSockets authenticate after connecting via a `login` operation (`okx.ws-private.ts:48-67`, `okx.ws-business.ts:51-69`):

1. **Timestamp**: Unix epoch seconds from `virtualClock.getCurrentTime().unix()`
2. **String to sign**: `timestamp + 'GET' + '/users/self/verify'`
3. **Signature**: HMAC-SHA256 with API secret, output as **base64**
4. **Login payload**:
   ```json
   {
     "op": "login",
     "args": [{ "apiKey": "...", "passphrase": "...", "timestamp": 1234567890, "sign": "base64sig" }]
   }
   ```
5. On successful login (response `code === 0`), the `subscribe()` method is called (`okx.ws-private.ts:101-103`, `okx.ws-business.ts:107-109`)

### Time Sync

The `virtualClock` is used for both REST and WebSocket authentication timestamps. This ensures clock synchronization even if the local system clock drifts.

### Account Validation

Regular OKX validates credentials by fetching `ENDPOINTS.BALANCE` (`/api/v5/account/account-position-risk`). OKX EU overrides `validateAccount()` to fetch `ENDPOINTS.ACCOUNT_CONFIG` (`/api/v5/account/config`) instead, because the OKX EU host can return HTTP 503 on `account-position-risk` during the new-account validation gate even when the key, secret, and passphrase are otherwise valid. Startup still fetches balance and positions through the normal SWAP-scoped balance path after account config has loaded.

## Startup Sequence

Defined in `start()` (`okx.exchange.ts:106-164`):

### Phase 1: Market Data (parallel)

```
Promise.all([
  fetchMarkets()   --> store.update({ markets, loaded.markets: true })
  fetchTickersRaw() --> returns raw ticker data
])
```

- **Markets** (`/api/v5/public/instruments?instType=SWAP`): Fetches all SWAP instruments, filters to `ctType === 'linear'`, and stores
- **Tickers** (`/api/v5/market/tickers?instType=SWAP`): Fetched in parallel with markets

### Phase 2: Parse & Connect Public WS (sequential)

1. Parse raw tickers via `parseTickers()` (requires markets to be loaded first for `marketsById` lookup)
2. Store tickers and set `loaded.tickers: true`
3. Connect public WebSocket (`publicWebsocket.connectAndSubscribe()`)
4. Connect business WebSocket (`businessWebsocket.connectAndSubscribe()`)

### Phase 3: Early exit for public-only mode

If `publicDataOnly` or missing key/secret, log and return. No private connections are made.

### Phase 4: Account Config (sequential, blocks on result)

1. `getOrFetchPositionMode()` -- fetches `/api/v5/account/config` to get `posMode` and `acctLv`
   - Caches results in `_accountPositionMode` and `_accountLevel`
   - **Must complete before positions can be mapped** (position ID generation depends on position mode)

### Phase 5: Private WS + Data (parallel after config)

1. Connect private WebSocket (`privateWebsocket.connectAndSubscribe()`)
2. In parallel:
   - `fetchBalanceAndPositions()` -- fetches balance + positions, stores with `loaded.balance: true, loaded.positions: true`
   - `fetchOrders()` -- fetches all order types (normal + 3 algo types), stores with `loaded.orders: true`

### Phase 6: Start Tick Loop

- Calls `tick()` which recalculates balance on each cycle
- The tick loop runs every `tickInterval` ms (2000ms by default) via the `loop()` utility
- `loop()` uses `requestAnimationFrame` when the tab is visible, falls back to `setTimeout` when hidden

### Disposal Check

Between Phase 1 and Phase 2, `if (this.isDisposed) return;` prevents continuing if the exchange was disposed during async fetches.

## REST API Endpoints

All endpoints are defined in `ENDPOINTS` in `okx.types.ts`:

| Constant               | Path                                    | Method | Description                                                                   |
| ---------------------- | --------------------------------------- | ------ | ----------------------------------------------------------------------------- |
| `ACCOUNT`              | `/api/v5/account/config`                | GET    | Get account configuration (position mode, account level)                      |
| `ACCOUNT_CONFIG`       | `/api/v5/account/config`                | GET    | Same path as ACCOUNT (used for position mode/account level fetch)             |
| `PARTNER`              | `/api/v5/users/partner/if-rebate`       | GET    | Check partner rebate status                                                   |
| `MARKETS`              | `/api/v5/public/instruments`            | GET    | Fetch available instruments (`instType=SWAP`)                                 |
| `TICKERS`              | `/api/v5/market/tickers`                | GET    | Fetch ticker data (`instType=SWAP`)                                           |
| `BALANCE`              | `/api/v5/account/account-position-risk` | GET    | Account balance and position risk (`instType=SWAP`)                           |
| `POSITIONS`            | `/api/v5/account/positions`             | GET    | Fetch open positions (`instType=SWAP`)                                        |
| `KLINE`                | `/api/v5/market/candles`                | GET    | Fetch OHLCV candlestick data                                                  |
| `UNFILLED_ORDERS`      | `/api/v5/trade/orders-pending`          | GET    | Fetch unfilled regular orders (paginated, 100 per page)                       |
| `UNFILLED_ALGO_ORDERS` | `/api/v5/trade/orders-algo-pending`     | GET    | Fetch unfilled algo orders (paginated, 100 per page)                          |
| `FILLS`                | `/api/v5/trade/fills`                   | GET    | Recent fills (primary for `fetchExecutions`)                                  |
| `FILLS_HISTORY`        | `/api/v5/trade/fills-history`           | GET    | Older fills fallback for `fetchExecutions`                                    |
| `PLACE_ORDERS`         | `/api/v5/trade/batch-orders`            | POST   | Place batch orders (up to 20 per batch)                                       |
| `PLACE_ALGO_ORDER`     | `/api/v5/trade/order-algo`              | POST   | Place single algo order                                                       |
| `UPDATE_ORDER`         | `/api/v5/trade/amend-order`             | POST   | Amend an existing order                                                       |
| `CANCEL_ORDERS`        | `/api/v5/trade/cancel-batch-orders`     | POST   | Cancel batch orders (up to 20 per batch)                                      |
| `CANCEL_ALGO_ORDERS`   | `/api/v5/trade/cancel-algos`            | POST   | Cancel algo orders (up to 20 per batch)                                       |
| `SET_LEVERAGE`         | `/api/v5/account/set-leverage`          | POST   | Set leverage for an instrument                                                |
| `LEVERAGE`             | `/api/v5/account/leverage-info`         | GET    | Get leverage info (supports comma-separated instIds, batched in groups of 20) |
| `SET_POSITION_MODE`    | `/api/v5/account/set-position-mode`     | POST   | Switch position mode (`long_short_mode` or `net_mode`)                        |

Execution-history behavior (`fetchExecutions`):

- Fetches `/trade/fills` first, then `/trade/fills-history` if needed
- Converts contract-size amounts to base qty using `contractsToBase(...)`
- Normalizes fill timestamps to Unix seconds and resolves symbol from `instId`

## WebSocket Implementation

OKX uses **three separate WebSocket connections**, all on port 8443.

### URLs

| Type     | Livenet                                | Testnet                                              |
| -------- | -------------------------------------- | ---------------------------------------------------- |
| Public   | `wss://ws.okx.com:8443/ws/v5/public`   | `wss://ws.okx.com:8443/ws/v5/public?brokerId=9999`   |
| Private  | `wss://ws.okx.com:8443/ws/v5/private`  | `wss://ws.okx.com:8443/ws/v5/private?brokerId=9999`  |
| Business | `wss://ws.okx.com:8443/ws/v5/business` | `wss://ws.okx.com:8443/ws/v5/business?brokerId=9999` |

Private and business WebSockets support Tealstreet proxy routing. Both
`connectAndSubscribe` methods resolve their host from the shared routing
resolver instead of hardcoding a URL:

```ts
const route = resolveExchangeRouting(this.parent.name, this.parent.options).wsPrivate; // or .wsBusiness
this.ws = this.createWebsocket(route.tunnelled ? route.host! : this.endpoint);
```

The actual tunnelled host (the `wss://fsjnpy143pbz.*` family) and the
`tealstreetProxy` gating live in `routing/routingTable.ts` + the
`resolveExchangeRouting` resolver — not in the OKX ws classes. To change
where OKX private/business traffic egresses, edit the table.

### Ping/Pong

All three WebSocket types send `'ping'` as a text frame. The server responds with `'pong'` (text frame, not a WebSocket pong frame).

- **Ping interval**: 5 seconds after receiving pong (`base.ws.ts:118`)
- **Pong miss timeout**: 7.5 seconds (1.5x ping interval) -- triggers reconnection (`base.ws.ts:125-128`)
- **Stale message detection**: `trackLastReceived()` runs a 1-second check loop:
  - If no message for >6 seconds (`WS_PING_THRESHOLD_MS`): sends a ping
  - If no message for >20 seconds (`WS_RECONNECT_THRESHOLD_MS`): triggers reconnection

### Reconnection Logic

Reconnection is handled by `BaseWebSocket` (`base.ws.ts`):

- **ReconnectingWebSocket mode** (`ENABLE_RECONNECTING_WEBSOCKET = false` by default): Currently disabled; uses native WebSocket with manual reconnection
- **Native WebSocket reconnection**: On close, waits `WS_MIN_RECONNECTION_DELAY` (1-3 seconds, randomized), then calls `connectAndSubscribe()`
- **Max reconnect count**: 100 attempts before giving up (`base.ws.ts:398-401`, `base.ws.ts:495-498`)
- **Private WS reconnect**: On reconnection, calls `fetchAndSetPrivateSnapshot()` which re-fetches all orders from REST to reconcile state
- **Public WS reconnect**: Emits `wsReconnected` event to notify the application

### Public WebSocket Channels (`okx.ws-public.ts`)

Subscribed per-instrument for all loaded markets on `onOpen`:

| Channel         | Handler                    | Description                                                                            |
| --------------- | -------------------------- | -------------------------------------------------------------------------------------- |
| `tickers`       | `handleTickerEvents`       | Bid, ask, last price, 24h percentage, volume                                           |
| `mark-price`    | `handleMarkPriceEvents`    | Mark price updates                                                                     |
| `index-tickers` | `handleIndexTickerEvents`  | Index price (subscribed with `-SWAP` stripped from instId)                             |
| `open-interest` | `handleOpenInterestEvents` | Open interest in contract currency (`oiCcy`)                                           |
| `funding-rate`  | `handleFundingRateEvents`  | Funding rate and next funding time                                                     |
| `books`         | Order book listener        | Snapshot + incremental updates (subscribed on-demand per symbol via `listenOrderBook`) |

**Message routing** (`okx.ws-public.ts:99-114`): Uses string `includes()` checks on the raw data for fast channel detection before JSON parsing. This avoids parsing every message.

### Private WebSocket Channels (`okx.ws-private.ts`)

Subscribed after successful login:

| Channel                      | Handler               | Description                                                             |
| ---------------------------- | --------------------- | ----------------------------------------------------------------------- |
| `account`                    | `handleAccountTopic`  | Total equity updates for balance calculation                            |
| `positions` (instType: SWAP) | `handlePositionTopic` | Position updates including entry price, leverage, margin, PnL           |
| `orders` (instType: SWAP)    | `handleOrderTopic`    | Regular order status changes (live, partially_filled, filled, canceled) |

**Error event handling** (`okx.ws-private.ts:108-118`): Messages containing `"event":"error"` are parsed and the error message is emitted via `emitAndLogError`.

### Business WebSocket Channels (`okx.ws-business.ts`)

Extends `OKXPrivateWebsocket` class. Subscribed after successful login:

| Channel                         | Handler                        | Description                                                          |
| ------------------------------- | ------------------------------ | -------------------------------------------------------------------- |
| `orders-algo` (instType: SWAP)  | `handleOrderTopic` (inherited) | Algo order updates (TP/SL/conditional/trailing)                      |
| `algo-advance` (instType: SWAP) | `handleOrderTopic` (inherited) | Advanced algo order updates                                          |
| `candle{interval}`              | OHLCV listener                 | Candlestick data (subscribed on-demand per symbol via `listenOHLCV`) |

### Order Book Subscription Pattern (`okx.ws-public.ts:155-269`)

Order book is subscribed on-demand (not at startup):

1. `listenOrderBook(symbol, callback)` waits for markets to be loaded (polls every 100ms)
2. Waits for WebSocket to be connected (polls every 100ms)
3. Subscribes to `books` channel for the specific `instId`
4. **Snapshot** (`action === 'snapshot'`): Replaces entire order book, filtering zero-amount entries
5. **Delta updates** (`action === 'update'`): Applies incremental updates via `updateOrderBook()`
6. **Post-processing**: Filters out invalid prices (bids > last price, asks < last price), sorts, and calculates totals
7. Stores topic for re-subscription on reconnect
8. Returns cleanup function that unsubscribes the channel

### OHLCV Subscription Pattern (`okx.ws-business.ts:143-218`)

Similar on-demand pattern:

1. Waits for markets loaded and WebSocket connected
2. Subscribes to `candle{OKX_INTERVAL}` channel for the specific `instId`
3. Candle data format: array where index 0=timestamp(ms), 1=open, 2=high, 3=low, 4=close, 7=volume
4. Returns cleanup function that unsubscribes

## Order Types and Mappings

### Regular Orders (via batch-orders endpoint)

| Internal Type                    | OKX `ordType` | Notes                        |
| -------------------------------- | ------------- | ---------------------------- |
| `OrderType.Market`               | `market`      | Market order                 |
| `OrderType.Limit`                | `limit`       | Limit order (GTC by default) |
| `OrderType.Limit` + PostOnly TIF | `post_only`   | Post-only limit order        |

### Algo Orders (via order-algo endpoint)

| Internal Type                | OKX `ordType`     | Notes                                               |
| ---------------------------- | ----------------- | --------------------------------------------------- |
| `OrderType.StopLoss`         | `conditional`     | Uses `slTriggerPx`, `slOrdPx=-1` (market execution) |
| `OrderType.TakeProfit`       | `conditional`     | Uses `tpTriggerPx`, `tpOrdPx=-1` (market execution) |
| `OrderType.TrailingStopLoss` | `move_order_stop` | Uses `callbackSpread`, `closeFraction=1`            |

### Fetched Algo Order Types

When fetching orders, three algo types are queried separately (`okx.exchange.ts:497-503`):

- `oco` -- One-cancels-other orders
- `conditional` -- Stop loss / take profit orders
- `move_order_stop` -- Trailing stop loss orders

### Time in Force

| OKX `ordType` | Mapped TIF                        |
| ------------- | --------------------------------- |
| `post_only`   | `OrderTimeInForce.PostOnly`       |
| All others    | `OrderTimeInForce.GoodTillCancel` |

### Stop Trigger Types

Supported trigger types for TP/SL (defined in metadata `supportedTPSLTriggerTypes`):

- `OrderStopTriggerType.LastPrice` (default for TP)
- `OrderStopTriggerType.MarkPrice` (default for SL)

OKX API trigger type values: `'last'`, `'mark'`, `'index'`.

**Trigger type mapping** (`okx.exchange.ts:1035-1051`):

- SL defaults to `'mark'` when no trigger type specified
- TP defaults to `'last'` when no trigger type specified

### Order Status Mapping (`ORDER_STATUS`)

| OKX State          | Internal Status    |
| ------------------ | ------------------ |
| `live`             | `OrderStatus.Open` |
| `partially_filled` | `OrderStatus.Open` |

`filled` and `canceled` states are handled via WebSocket events to update/remove orders.

### Order Type Mapping (`ORDER_TYPE`)

| OKX `ordType`       | Internal Type      |
| ------------------- | ------------------ |
| `market`            | `OrderType.Market` |
| `limit`             | `OrderType.Limit`  |
| `post_only`         | `OrderType.Limit`  |
| `fok`               | `OrderType.Limit`  |
| `ioc`               | `OrderType.Limit`  |
| `optimal_limit_ioc` | `OrderType.Market` |

### Reverse Order Type (`REVERSE_ORDER_TYPE`)

| Internal Type                | OKX `ordType`     |
| ---------------------------- | ----------------- |
| `OrderType.Market`           | `market`          |
| `OrderType.Limit`            | `limit`           |
| `OrderType.StopLoss`         | `trigger`         |
| `OrderType.TakeProfit`       | `trigger`         |
| `OrderType.TrailingStopLoss` | `move_order_stop` |

### Order Side (`ORDER_SIDE`)

| OKX Side | Internal Side    |
| -------- | ---------------- |
| `buy`    | `OrderSide.Buy`  |
| `sell`   | `OrderSide.Sell` |

### Position Side (`POSITION_SIDE`)

| OKX Side | Internal Side        |
| -------- | -------------------- |
| `long`   | `PositionSide.Long`  |
| `short`  | `PositionSide.Short` |

### Candle Intervals (`INTERVAL`)

| Internal | OKX   |
| -------- | ----- |
| `1m`     | `1m`  |
| `3m`     | `3m`  |
| `5m`     | `5m`  |
| `15m`    | `15m` |
| `30m`    | `30m` |
| `1h`     | `1H`  |
| `2h`     | `2H`  |
| `4h`     | `4H`  |
| `6h`     | `6H`  |
| `12h`    | `12H` |
| `1d`     | `1D`  |
| `1w`     | `1W`  |

Note: Minutes use lowercase (e.g., `1m`), hours and days use uppercase (e.g., `1H`, `1D`).

## Order Lifecycle

### Order Placement (`okx.exchange.ts:777-797`)

1. `placeOrders()` ensures position mode is loaded via `getOrFetchPositionMode()`
2. Orders are partitioned into algo and normal orders using `isAlgoOrder()`
3. Normal orders are formatted via `formatCreateOrder()` which may split into multiple lots
4. The `deriveAlgoOrdersFromNormalOrdersOpts()` base method extracts attached TP/SL from normal order opts and creates separate algo orders
5. All algo orders (explicit + derived) are formatted via `formatAlgoOrder()`
6. Normal orders are placed first via `placeOrderBatch()`, then algo orders via `placeAlgoOrderBatch()`

### Order ID Generation

- **Regular orders**: `clOrdId` = `{BROKER_ID}{uuid}` truncated to 32 chars, plus `tag: BROKER_ID` (`okx.exchange.ts:1082-1083`)
- **Algo orders**: `algoClOrdId` = `{BROKER_ID}{uuid}` truncated to 32 chars, plus `tag: BROKER_ID` (`okx.exchange.ts:1015-1016`)
- **Broker ID**: `e847386590ce4dBC` (`okx.types.ts:5`)

### Algo Order ID Suffixes

Algo orders use composite IDs with suffixes (`okx.exchange.ts:816,848,897`):

- Stop loss: `{algoId}_sl`
- Take profit: `{algoId}_tp`
- Trailing stop loss: `{algoId}_tsl`

When cancelling algo orders, the suffix is stripped to get the base `algoId` (`okx.exchange.ts:1179`): `o.id.replace(/_[a-zA-Z]+$/, '')`

### OCO/Conditional Order Mapping (`okx.exchange.ts:806-868`)

A single OKX `oco` or `conditional` order can produce **two** internal orders:

- If `slTriggerPx` is set: creates a StopLoss order with `_sl` suffix
- If `tpTriggerPx` is set: creates a TakeProfit order with `_tp` suffix

### Order Update (Amend) (`okx.exchange.ts:606-659`)

- **Limit orders**: Amended in-place via `ENDPOINTS.UPDATE_ORDER` with `newPx` and/or `newSz`
- **Algo orders**: Cannot be amended in-place. The `updateAlgoOrder()` method:
  1. Finds all related orders sharing the same base algo ID
  2. Cancels them all
  3. Re-places with updated parameters
- **Amount validation**: If adjusted amount is 0, emits error `'Order amount is too small'` and returns empty

### Order Size Splitting (`okx.exchange.ts` around lines 1230–1250)

If an order amount exceeds `market.limits.amount.max`, it is automatically split:

- `maxSize` is the minimum of `maxIcebergSz`, `maxLmtSz`, `maxMktSz`, `maxStopSz`, `maxTriggerSz`, `maxTwapSz`
- Orders are split into `Math.ceil(amount / maxSize)` equal lots
- Remainder lot (if any) is added separately
- **Confirmed bug**: The remainder lot uses `qty` instead of `sz` at
  `okx.exchange.ts:1246`: `payloads.push({ ...req, qty: rest })`. The
  spread preserves `req.sz = ${amount}` (set at line 1232 to the FULL
  order amount), so the resulting payload has both an invalid `qty`
  field (OKX v5 only knows `sz`) AND an `sz` equal to the full
  pre-split amount instead of the remainder. OKX will either reject
  the payload or silently execute at the wrong size — the remainder
  lot path has never worked. See also the Known Quirks entry below.

### Batch Processing

- **Regular orders**: Placed in batches of up to 20 via `PLACE_ORDERS`, processed sequentially between batches (`mapSeries`) (`okx.exchange.ts:1107-1128`)
- **Algo orders**: Placed sequentially one at a time via `PLACE_ALGO_ORDER` (`okx.exchange.ts:1130-1151`)
- **Cancel normal**: Batched in groups of 20, processed sequentially (`okx.exchange.ts:1153-1171`)
- **Cancel algo**: Batched in groups of 20, processed sequentially; suffix stripped from ID (`okx.exchange.ts:1173-1191`)

### Algo Order Special Handling

- **SL/TP with zero amount**: Uses `closeFraction: '1'` instead of `sz` (close entire position) (`okx.exchange.ts:1025-1027`)
- **SL/TP execution**: Always uses market price (`slOrdPx: '-1'` or `tpOrdPx: '-1'`) (`okx.exchange.ts:1033-1034, 1044-1045`)
- **SL/TP are always `reduceOnly: true`** (`okx.exchange.ts:1029`)
- **Trailing stop loss**: Uses `callbackSpread` for distance, `closeFraction: '1'`, empty `activePx` and `callbackRatio` (`okx.exchange.ts:1054-1060`)

### Position Side Logic (`getPositionSide`, `okx.exchange.ts:1199-1217`)

- In **one-way mode** (`PositionMode.OneWay`): Returns `undefined` (omitted from payload)
- In **hedged mode**: Buy = `'long'`, Sell = `'short'`
- **Inverted for close orders**: SL, TP, trailing, and reduce-only orders flip the side (Buy becomes `'short'`, Sell becomes `'long'`)

## Symbol Format

- **OKX instrument ID (`instId`)**: `BTC-USDT-SWAP`, `ETH-USDT-SWAP`
- **Internal symbol**: `BTCUSDT`, `ETHUSDT` (derived by stripping `-SWAP` and removing all `-` from instId, `okx.exchange.ts:198`)
- **Market filter**: Only linear contracts (`ctType === 'linear'`) are included (`okx.exchange.ts:185`)
- **Index ticker subscription**: Uses instId without `-SWAP` suffix (e.g., `BTC-USDT`) (`okx.ws-public.ts:49`)
- **Index ticker response mapping**: Re-appends `-SWAP` to match stored ticker IDs (`okx.ws-public.ts:141`)

## Data Precision & Conversion

### Price and Amount Precision

- **Price precision** (`precision.price`): `tickSz` from instrument data (e.g., `0.01`)
- **Amount precision** (`precision.amount`): `lotSz` from instrument data (e.g., `0.001`)
- **Adjustment**: The `adjust()` utility (from `safe-math`) is used to round values to the correct precision step size

### Contract Size Handling

- **Contract size** (`contractSize`): `ctVal` from instrument data (e.g., `0.01` BTC per contract)
- **Order amounts**: Specified in **contracts** (native units), not base currency
- **Display conversion**: `contracts * contractSize` converts to base currency amount
- **Minimum amount**: `BigNumber(minSz).multipliedBy(ctVal)` (`okx.exchange.ts:208-209`)
- **Position notional**: `contractsNative * markPrice * contractSize` (`okx.exchange.ts:968`)

### BigNumber Usage

- `BigNumber` (from `bignumber.js`) is used for minimum amount calculation to avoid floating point errors (`okx.exchange.ts:208`)
- The `multiply`, `subtract`, `add`, `adjust` functions from `safe-math` are used throughout for safe arithmetic

### Balance Amounts

All balance/position monetary values use `roundUSD()` for display rounding.

### Candle Timestamps

OKX candle timestamps are in **milliseconds**. They are divided by 1000 for internal storage in seconds (`okx.exchange.ts:582`, `okx.ws-business.ts:172`).

### OHLCV Data

- REST candle data: Array format `[timestamp, open, high, low, close, ?, ?, volume]` -- index 7 is volume (`okx.exchange.ts:582`)
- WebSocket candle data: Same array format, index 7 is volume (`okx.ws-business.ts:177`)
- REST results are sorted ascending by timestamp after fetching (`okx.exchange.ts:585`)

## Rate Limiting

### REST API

- **Retry policy**: 3 retries on network errors via `axios-retry` (`okx.api.ts:31`)
- **Receive window (timeout)**: `RECV_WINDOW = 20_000` ms (20 seconds), overridable via `options.extra.recvWindow`

### Leverage Fetch Batching

- Leverage info supports comma-separated `instId` values, batched in groups of 20 (`okx.exchange.ts:471`)
- **250ms sleep** between batches to avoid rate limiting (`okx.exchange.ts:475`)

### Order Batch Limits

- Regular orders: Up to 20 per batch (`okx.exchange.ts:1108`)
- Algo orders: One at a time, processed sequentially (`okx.exchange.ts:1131`)
- Cancel normal orders: Up to 20 per batch (`okx.exchange.ts:1162`)
- Cancel algo orders: Up to 20 per batch (`okx.exchange.ts:1182`)

## State Reconciliation

### WebSocket Order Updates (`okx.ws-private.ts:139-168`)

Orders received via WebSocket are processed individually:

1. Mapped through `mapOrders()` to convert to internal format
2. **`filled` or `canceled`**: Updates order status via `store.updateOrdersStatus()` (removes from active orders)
3. **`live` or `partially_filled`**: Adds or updates via `store.addOrUpdateOrders()`
4. **Fill events**: `filled` and `partially_filled` emit a `'fill'` event with fill price and amount

### WebSocket Position Updates (`okx.ws-private.ts:249-280`)

1. Positions are mapped through `mapPositions()`
2. Updated in store via `store.addOrUpdatePositions()`
3. Balance `used` (from `mmr` sum) and `upnl` are recalculated from the position data

### WebSocket Account Updates (`okx.ws-private.ts:282-292`)

1. `totalEq` from account data is summed for total collateral
2. Balance `free` and `total` are recalculated using current `used` and `upnl` from store

### REST Snapshot on Reconnection

When a private WebSocket reconnects (`base.ws.ts:462-468`):

1. `fetchAndSetPrivateSnapshot()` is called (from base class)
2. This re-fetches all orders from REST API to reconcile any missed WebSocket events
3. Called with up to 10 retry attempts

### Tick Loop Reconciliation (`okx.exchange.ts:166-176`)

- Runs every 2 seconds (`tickInterval: 2_000`)
- Recalculates balance using stored balance info, current positions, and current orders
- Does NOT re-fetch from REST (unlike some exchanges); relies on WebSocket for data freshness

### Recursive Pagination for Order Fetching

Both normal and algo order fetches use recursive pagination (`okx.exchange.ts:506-524, 528-548`):

- Page size: 100 results
- If exactly 100 results returned, fetches next page using `after` cursor (last order's `ordId` or `algoId`)
- Accumulates results across pages

## Exchange Metadata

Key metadata values from `exchangeMetadata.ts`:

| Property                          | Value                                                                 |
| --------------------------------- | --------------------------------------------------------------------- |
| `exchangeName`                    | `okx`                                                                 |
| `prettyName`                      | `OKX`                                                                 |
| `requiresPassphrase`              | `true`                                                                |
| `marginTypes`                     | `[MarginType.Cross]` (cross-only)                                     |
| `positionModes`                   | `[PositionMode.OneWay, PositionMode.Hedged]` (inherited from default) |
| `positionModeIsGlobal`            | `true`                                                                |
| `unifiedMargin`                   | `false`                                                               |
| `supportsTestnet`                 | `false`                                                               |
| `supportsTradesHistory`           | `false`                                                               |
| `supportsChaser`                  | `true`                                                                |
| `supportsTrailingStopLoss`        | `false` (disabled at metadata level despite code support)             |
| `hasEmptyPositions`               | `false`                                                               |
| `orderbookTickDelay`              | `250` ms                                                              |
| `tickInterval`                    | `2_000` ms                                                            |
| `klineCallbackResetThreshold`     | `60`                                                                  |
| `tvMaxGetBarFetches`              | `5`                                                                   |
| `requiresTickersForMarketsLoaded` | `true`                                                                |
| `supportedTPSLTriggerTypes`       | `[LastPrice, MarkPrice]`                                              |
| `defaultTPTriggerType`            | `LastPrice`                                                           |
| `defaultSLTriggerType`            | `MarkPrice`                                                           |
| `privateProxyIps`                 | `privateProxyIpsTokyo`                                                |
| `requiresCors`                    | inherited default                                                     |

### Account Levels (`_accountLevel`)

Fetched from `/api/v5/account/config` (`acctLv` field):

| Level | Mode                  | Description                               |
| ----- | --------------------- | ----------------------------------------- |
| 1     | Spot mode             | Spot trading only                         |
| 2     | Futures mode          | Non-unified margin, per-currency balances |
| 3     | Multi-currency margin | Unified margin across currencies          |
| 4     | Portfolio margin      | Full portfolio margining                  |

The account level affects balance calculation. When `accLevel === 2` (Futures mode), per-currency balance breakdowns are provided in `balance.currencies`. Otherwise, currencies is an empty object.

### Position Modes (`_accountPositionMode`)

Fetched from `/api/v5/account/config` (`posMode` field):

| OKX Value         | Internal Value        | Description                       |
| ----------------- | --------------------- | --------------------------------- |
| `long_short_mode` | `PositionMode.Hedged` | Separate long and short positions |
| `net_mode`        | `PositionMode.OneWay` | Net position (single direction)   |

**Position mode is global** (`positionModeIsGlobal: true`). Switching modes requires **all positions to be closed first** (`okx.exchange.ts:743`).

### Margin Type

OKX in this implementation **only supports cross margin** (`marginTypes: [MarginType.Cross]`). Calling `changeMarginType()` emits the error: `"Only cross mode is supported"` (`okx.exchange.ts:738-739`). All order and leverage payloads hardcode `mgnMode: 'cross'` or `tdMode: 'cross'`.

### Leverage

- **Setting leverage**: POST to `/api/v5/account/set-leverage` with `instId`, `lever` (string), `mgnMode: 'cross'` (`okx.exchange.ts:684-688`)
- **Fetching leverage**: GET to `/api/v5/account/leverage-info` with comma-separated `instId` values (batched in groups of 20 with 250ms sleep between batches) (`okx.exchange.ts:468-495`)
- Leverage is clamped to `[market.limits.leverage.min, market.limits.leverage.max]` (`okx.exchange.ts:681`)
- After setting leverage, positions are re-fetched from the API (`okx.exchange.ts:690-691`)
- On error, positions are still re-fetched and error is re-thrown (`okx.exchange.ts:694-698`)

### Balance Calculation (`formatBalanceUsedAvail`, `okx.exchange.ts:379-456`)

1. **Total collateral**: Sum of `eq * markPrice` across all balance currencies (mark price from tickers for non-stablecoin assets; stablecoins use multiplier of 1)
2. **Used (positions)**: Sum of `imr` (initial margin requirement) across positions
3. **Used (orders)**: Sum of `(remaining * price / leverage) * 1.1` for open limit, non-reduce-only orders (the 1.1 is a "hand wavy" multiplier, `okx.exchange.ts:399`)
4. **Unrealized PnL**: Sum of `upl` across positions
5. **Free**: `totalCollateral - usedPositions - usedOrders`
6. **Total**: `totalCollateral - upnl`
7. **Per-currency breakdown** (Futures mode only, `accLevel === 2`): For each currency, tracks used margin and upnl separately

### Position Mapping (`mapPositions`, `okx.exchange.ts:943-996`)

- **Contracts**: `pos` field, absolute value
- **Side**: Determined from `posSide` (`long`/`short`), or from sign of `pos` in net mode (negative = short)
- **Entry price**: `avgPx` (zero when no contracts)
- **Notional**: `contractsNative * markPrice * contractSize`
- **Leverage**: From position `lever` field, falls back to existing stored position's leverage (`okx.exchange.ts:982`)
- **Liquidation price**: `liqPx` field (defaults to 0 if missing)
- **Margin type**: From `mgnMode` field (`cross` or `isolated`)
- **Position mode**: Derived from `posSide` (`net` = OneWay, otherwise Hedged)
- **Position ID**: Created via `createPositionId(symbol, side, positionMode)`

## Testnet / Demo

- **`supportsTestnet`**: `false` in metadata -- testnet is not officially supported
- **REST URL**: Same base URL (`https://www.okx.com`) with added header `x-simulated-trading: 1` (`okx.api.ts:25`)
- **WebSocket URLs**: Append `?brokerId=9999` query parameter for testnet (`okx.types.ts:10-20`)
- **Limitations**: No specific feature differences documented in code; testnet behavior relies on OKX's simulated trading mode

## Known Quirks / Implementation Notes

- **Trailing stop loss**: Code exists but is disabled at the metadata level (`supportsTrailingStopLoss: false`). The `formatAlgoOrder` and `mapOrders` methods fully implement it.
- **Margin type**: Hardcoded to cross only; attempting to change emits an error.
- **Position mode change**: Requires all positions to be closed first; checked client-side before API call (`okx.exchange.ts:743`).
- **Account config caching**: Position mode and account level are cached in `_accountPositionMode` and `_accountLevel`; use `force = true` parameter to re-fetch. Both are fetched from a single endpoint call (`getOrFetchPositionModeAndAccLevel`).
- **Algo order updates**: Cannot amend algo orders in-place; they are cancelled and re-created.
- **Order book filtering**: Bids above last price and asks below last price are filtered out after each update (`okx.ws-public.ts:221-222`).
- **Recursive order fetching**: Both regular and algo order fetches use recursive pagination (when exactly 100 results are returned, fetch next page using `after` cursor).
- **Business WebSocket inheritance**: `OKXBusinessWebsocket` extends `OKXPrivateWebsocket` to reuse `auth()` and `handleOrderTopic()` methods. However, it overrides `connectAndSubscribe`, `onOpen`, `onMessage`, `ping`, `auth`, and `subscribe` -- meaning most of the parent's methods are replaced.
- **Contract sizes**: Order amounts are in contracts (native size), which are multiplied by `contractSize` (`ctVal`) for display. The `precision.amount` is `lotSz` and `precision.price` is `tickSz`.
- **Position mode in one-way mode**: `fetchPositionMetadata` creates two virtual positions (Long + Short) for each symbol in one-way mode so that both sides are represented in the store (`okx.exchange.ts:291-307`).
- **Balance info passthrough**: `formatBalanceUsedAvail` stores the raw `bal` array in `balance.info` so it can be reused in the tick loop without re-fetching (`okx.exchange.ts:168-173`).
- **Partner check**: The `PARTNER` endpoint (`/api/v5/users/partner/if-rebate`) is defined but not called in any code path within this directory.
- **Order lot splitting bug** (confirmed): When splitting oversized
  orders, the remainder lot uses `qty` instead of `sz` as the key
  (`okx.exchange.ts:1246`: `payloads.push({ ...req, qty: rest })`).
  OKX v5 does not recognize `qty`, and the spread preserves
  `req.sz = ${amount}` (the full pre-split amount set at line 1232),
  so the remainder payload is malformed in two ways simultaneously.
  See "Order Size Splitting" above for details.
- **Stablecoin mark price fallback**: When calculating balance equity notional, if no ticker is found for a currency (e.g., USDT), the mark price defaults to 1 (`okx.exchange.ts:392`).
- **Trailing stop price calculation**: For trailing stop loss orders, the displayed price is calculated relative to the position entry price and last price, with special handling when a position exists (`okx.exchange.ts:871-891`).
- **`afterMethodCalledInWorker`**: When `changePositionMode` is called in a WebWorker, the local `_accountPositionMode` cache is updated to match (`okx.exchange.ts:767-775`).
- **Grafana logging**: `ENABLE_PWS_LOG` is enabled by default. All private and business WebSocket messages are emitted to `grafana-pws` for monitoring. Subscription messages are emitted to `grafana-pws-sub`.
- **Potential follow-up (not confirmed bug)**: Inverse close sizing in the web layer currently prefers `Math.max(extra.contractsNative, notional/contractSize)` in [`getPositionNativeSize.ts`](mdc:apps/web/src/utils/position/getPositionNativeSize.ts) to avoid partial closes. This can theoretically oversize if notional is briefly inflated or stale. Re-evaluate if oversize-close reports appear; consider freshness/tolerance-based bounds instead of raw max/min switching.

## Error Handling

### General Pattern

Most methods follow the same error handling pattern:

1. Catch errors from axios responses
2. Extract message via `err?.response?.data?.msg || err?.message`
3. Call `this.emitAndLogError(message)` to emit error toast + log
4. Return the current store state as fallback (for fetches) or throw (for mutations)

### Specific Error Handling

| Context                                                         | Error Pattern                                       | Behavior                                               |
| --------------------------------------------------------------- | --------------------------------------------------- | ------------------------------------------------------ |
| `validateAccount` (`okx.exchange.ts:89-97`)                     | Any error from `GET /account/account-position-risk` | Returns error message string; does not throw           |
| `fetchMarkets` (`okx.exchange.ts:220-223`)                      | Any fetch error                                     | Emits error, returns current `store.markets`           |
| `fetchTickersRaw` (`okx.exchange.ts:242-245`)                   | Any fetch error                                     | Emits error, returns empty array                       |
| `parseTickers` (`okx.exchange.ts:279-282`)                      | Any parse error                                     | Emits error, returns current `store.tickers`           |
| `fetchBalanceAndOpenPositions` (`okx.exchange.ts:370-376`)      | Any fetch error                                     | Emits error, returns current store balance + positions |
| `formatBalanceUsedAvail` (`okx.exchange.ts:452-455`)            | Any calculation error                               | Emits error, returns current `store.balance`           |
| `placeOrderBatch` (`okx.exchange.ts:1121-1124`)                 | Batch response with `sMsg`                          | Throws error with the message                          |
| `placeAlgoOrderBatch` (`okx.exchange.ts:1144-1147`)             | Response without `algoId`                           | Throws error with `sMsg`                               |
| `cancelNormalOrders` (`okx.exchange.ts:1167-1169`)              | Response with `sMsg` per item                       | Emits error per failed cancellation (does not throw)   |
| `cancelAlgoOrders` (`okx.exchange.ts:1186-1188`)                | Response with `sMsg` per item                       | Emits error per failed cancellation (does not throw)   |
| `setLeverage` (`okx.exchange.ts:693-698`)                       | Any error                                           | Emits error, re-fetches positions, re-throws           |
| `changePositionMode` (`okx.exchange.ts:758-764`)                | Any error                                           | Emits error, re-fetches positions, re-throws           |
| `getOrFetchPositionModeAndAccLevel` (`okx.exchange.ts:722-725`) | Any fetch error                                     | Emits error, re-throws                                 |
| WebSocket error event (`okx.ws-private.ts:108-118`)             | `"event":"error"` message                           | Emits error from `json.msg`                            |

### Retry Strategy

- **REST**: 3 automatic retries on network errors via `axios-retry` (`okx.api.ts:31`)
- **WebSocket reconnection**: Up to 100 reconnect attempts with randomized delay (1-3 seconds)
- **`fetchAndSetPrivateSnapshot`**: Up to 10 retry attempts on WS reconnect (configured in `base.ws.ts:463`)

### No "Order Not Found" Success Pattern

Unlike some exchanges, OKX does **not** treat "order not found" on cancel as success. Cancel errors are emitted per-item via `sMsg` checks.

## HTTP-over-WebSocket

When `options.tealstreetTunnel` is enabled, the axios adapter is replaced with `bigIntAxiosHttpOverWebsocketAdapter` which routes HTTP requests over a WebSocket tunnel (supports BigInt serialization) (`okx.api.ts:61-63`).

## Grafana Monitoring

Endpoints are split into two monitoring groups:

- **Every-request logging** (`GRAFANA_ENDPOINTS_EVERY`): Order placement, cancellation, amendment, leverage operations, position mode changes
- **Periodic logging** (`GRAFANA_ENDPOINTS_PERIODIC`): Balance, positions, unfilled orders, unfilled algo orders

## Testing / Validation

- `validateAccount()` (`okx.exchange.ts:89-97`): Tests API connectivity by fetching balance endpoint. Returns empty string on success, error message on failure.
- No exchange-specific test files exist in this directory.
- Use the shared test infrastructure: `yarn test` from the `safe-cex` package root.
