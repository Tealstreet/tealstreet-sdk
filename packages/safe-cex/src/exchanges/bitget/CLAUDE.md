# Bitget Exchange Integration

This directory contains the Bitget exchange integration for Tealstreet, supporting three contract variants: USDT-margined linear, USDC-margined linear, and coin-margined inverse perpetual futures.

## Exchange Information

- **Pretty Name**: Bitget
- **Exchange Names**: `bitget` (USDT-M), `bitgetusdc` (USDC-M), `bitgetinverse` (Inverse)
- **Status**: `bitget` and `bitgetusdc` enabled; `bitgetinverse` disabled (`enabled: false`, `enableInPreview: false`)
- **Broker ID**: `Tealstreet` (sent as `X-CHANNEL-API-CODE` header)

## API Documentation

- **Official API Documentation**: https://www.bitget.com/api-doc/
- **Contract/Futures API**: https://www.bitget.com/api-doc/contract/intro
- **CCXT REST Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/bitget.ts
- **CCXT WebSocket Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/bitget.ts

## File Structure

| File                            | Description                                                                                                         |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `bitget.api.ts`                 | REST API client: Axios instance creation, HMAC-SHA256 request signing, response interceptors                        |
| `bitget.exchange.ts`            | Main `BitgetExchange` class (USDT-M linear) extending `BaseExchange`. Contains all trading logic                    |
| `bitget.types.ts`               | Constants (`BASE_URL`, `ENDPOINTS`, `BROKER_ID`, `RECV_WINDOW`), interval/status/side/type mappings, response types |
| `bitget.ws-private.ts`          | Private WebSocket: authentication, order/position/account streaming with throttled REST reconciliation              |
| `bitget.ws-public.ts`           | Primary public WebSocket: ticker snapshots (1/3 of markets), OHLCV candles, order book (books channel)              |
| `bitget.ws-secondary-public.ts` | Secondary public WebSocket: handles another 1/3 of market tickers. Two instances run (index 1 and 2)                |
| `bitgetinverse.exchange.ts`     | `BitgetInverseExchange` class: coin-margined inverse futures, overrides balance/position/order methods              |
| `bitgetusdc.exchange.ts`        | `BitgetUSDCExchange` class: minimal override, changes `productType` to `usdc-futures` and `marginCoin` to `USDC`    |

## Authentication

Authentication uses **HMAC-SHA256** with Base64 encoding. Three credentials are required: API key, secret, and **passphrase** (`requiresPassphrase: true` in metadata).

### REST API Signing (`bitget.api.ts`)

Headers set on every request:

- `ACCESS-KEY`: API key
- `ACCESS-PASSPHRASE`: Passphrase (set once in Axios defaults)
- `ACCESS-SIGN`: HMAC-SHA256 signature (per-request via interceptor)
- `ACCESS-TIMESTAMP`: Current virtual clock timestamp in milliseconds
- `X-CHANNEL-API-CODE`: Broker ID (`"Tealstreet"`)
- `Content-Type`: `application/json`

Signature construction (`bitget.api.ts:38-68`):

```
toSign = timestamp + METHOD + url(?queryString) + jsonBody
signature = base64(HMAC-SHA256(secret, toSign))
```

The `toJsonString()` helper (`bitget.api.ts:92-105`) strips leading underscores from JSON keys (keys starting with `_` have the underscore removed in the serialized output). The `$` character is also stripped from symbol parameters before signing and sending (`bitget.api.ts:47,56`).

### WebSocket Authentication (`bitget.ws-private.ts:536-590`)

```
timestamp = unix epoch seconds
sign = base64(HMAC-SHA256(secret, timestamp + "GET/user/verify"))
```

Sent as `{ op: "login", args: [{ apiKey, passphrase, timestamp, sign }] }`. On successful login, private channels are subscribed by parsing the ack structurally (`event: "login"`, `code: 0`/`"0"`). Do not match the login payload as an exact string; Bitget can include extra fields such as `connId`.

### Time Synchronization

Both REST and WS use `virtualClock` for timestamps. REST uses `virtualClock.getCurrentTimeMs()` (milliseconds), WS auth uses `virtualClock.getCurrentTime().unix()` (seconds).

## Startup Sequence

The `start()` method (`bitget.exchange.ts:144-191`) initializes in this **strict sequential** order:

1. **`fetchMarkets()`** -- REST call to get available contracts; updates store with `markets`, sets `loaded.markets = true`
2. **`publicWebsocket.connectAndSubscribe()`** -- starts primary public WS (ticker for 1/3 of markets) immediately after markets load
3. **`fetchTickers()`** -- REST call to get initial ticker data (bid/ask/last/index/funding); updates store with `tickers`, sets `loaded.tickers = true`
4. **Three WS connections started in parallel** (non-awaited):
   - `secondsTickersWebsocket.connectAndSubscribe()` -- secondary public WS for 2nd third of tickers
   - `thirdTickersWebsocket.connectAndSubscribe()` -- secondary public WS for 3rd third of tickers
   - `privateWebsocket.connectAndSubscribe()` -- private WS (auth + subscribe to orders/positions/account)
5. **`tick()`** -- first polling cycle: fetches positions (every tick), balance (every 2nd tick)
6. **`fetchOrders()`** -- fetches all three order types sequentially:
   - `fetchNormalOrders()` (limit/market orders, paginated)
   - `fetchAlgoOrders()` (TP/SL, paginated with `planType: 'profit_loss'`)
   - `fetchTrailingStopOrders()` (trailing stops, paginated with `planType: 'track_plan'`)
7. Store updated with `orders`, sets `loaded.orders = true`

Each step checks `this.isDisposed` to allow early exit if the exchange is disposed during initialization.

**Note**: There is no `publicDataOnly` mode -- Bitget always requires authentication.

## REST API Endpoints

All endpoints use the V2 API prefix (`/api/v2/mix/`):

### Market Data

| Constant          | Path                                 | Usage                              |
| ----------------- | ------------------------------------ | ---------------------------------- |
| `MARKETS`         | `/api/v2/mix/market/contracts`       | Fetch available contracts          |
| `TICKERS`         | `/api/v2/mix/market/tickers`         | Fetch all ticker prices            |
| `KLINE`           | `/api/v2/mix/market/candles`         | Fetch OHLCV candlestick data       |
| `SYMBOL_LEVERAGE` | `/api/v2/mix/market/symbol-leverage` | Fetch leverage brackets per symbol |

### Account & Positions

| Constant                     | Path                                   | Usage                                                |
| ---------------------------- | -------------------------------------- | ---------------------------------------------------- |
| `ACCOUNT_SPOT`               | `/api/v2/spot/account/info`            | Fetch spot account info (for userId)                 |
| `ACCOUNT`                    | `/api/v2/mix/account/account`          | Fetch single account details (leverage, margin mode) |
| `BALANCE`                    | `/api/v2/mix/account/accounts`         | Fetch all account balances                           |
| `POSITIONS` / `POSITIONS_V2` | `/api/v2/mix/position/all-position`    | Fetch all open positions                             |
| `POSITION_LEVERAGE`          | `/api/v2/mix/position/single-position` | Fetch single position details                        |
| `FETCH_SINGLE_ACCOUNT`       | `/api/v2/mix/account/account`          | Fetch per-symbol account metadata                    |

### Order Management

| Constant            | Path                                    | Usage                                              |
| ------------------- | --------------------------------------- | -------------------------------------------------- |
| `PLACE_ORDER`       | `/api/v2/mix/order/place-order`         | Place single order (not used; batch is preferred)  |
| `BATCH_ORDERS`      | `/api/v2/mix/order/batch-place-order`   | Place batch orders (up to 50 per symbol)           |
| `UPDATE_ORDER`      | `/api/v2/mix/order/modify-order`        | Modify a normal order                              |
| `ORDERS`            | `/api/v2/mix/order/orders-pending`      | Fetch pending normal orders (paginated, limit 200) |
| `ORDER_HISTORY`     | `/api/v2/mix/order/orders-history`      | Fetch historical filled orders                     |
| `CANCEL_ORDERS`     | `/api/v2/mix/order/batch-cancel-orders` | Batch cancel normal orders                         |
| `CANCEL_ALL_ORDERS` | `/api/v2/mix/order/cancel-all-orders`   | Cancel all orders                                  |

### Algo/Stop Orders

| Constant                    | Path                                    | Usage                     |
| --------------------------- | --------------------------------------- | ------------------------- |
| `PLACE_ALGO_ORDER`          | `/api/v2/mix/order/place-tpsl-order`    | Place TP/SL algo order    |
| `UPDATE_ALGO_ORDER`         | `/api/v2/mix/order/modify-tpsl-order`   | Modify TP/SL algo order   |
| `ALGO_ORDERS`               | `/api/v2/mix/order/orders-plan-pending` | Fetch pending algo orders |
| `CANCEL_ALGO_ORDER`         | `/api/v2/mix/order/cancel-plan-order`   | Cancel an algo order      |
| `PLACE_TRAILING_STOP_ORDER` | `/api/v2/mix/order/place-plan-order`    | Place trailing stop order |

### Account Settings

| Constant              | Path                                    | Usage                                              |
| --------------------- | --------------------------------------- | -------------------------------------------------- |
| `SET_LEVERAGE`        | `/api/v2/mix/account/set-leverage`      | Set leverage for a symbol (Classic + UTA fallback) |
| `SET_LEVERAGE_V3`     | `/api/v3/account/set-leverage`          | UTA per-side ISO leverage (`posSide` honored)      |
| `ACCOUNT_SETTINGS_V3` | `/api/v3/account/settings`              | UTA-only probe used for account-type detection     |
| `SET_POSITION_MODE`   | `/api/v2/mix/account/set-position-mode` | Set hedge or one-way mode                          |
| `SET_MARGIN_MODE`     | `/api/v2/mix/account/set-margin-mode`   | Set cross or isolated margin                       |

## WebSocket Implementation

The implementation uses **four WebSocket connections** simultaneously:

### 1. Public WebSocket (`bitget.ws-public.ts`)

- **URL**: `wss://ws.bitget.com/v2/ws/public` (or fallback: `wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public`)
- **Channels subscribed**:
  - `ticker` for 1/3 of all markets (index `i % 3 === 0`)
  - `candle{interval}` for active chart symbol (subscribed on-demand via `listenOHLCV`)
  - `books` for active order book symbol (subscribed on-demand via `listenOrderBook`)
- **instType**: Derived from `this.parent.apiProductType.toUpperCase()` for every subscribe and unsubscribe frame, so each variant uses its own product type.
- **On-demand subscriptions**: Candle and order-book listeners are reference-counted. Cleanup is idempotent, cancels a pending pre-connect subscription, and deletes the final topic key before sending its unsubscribe so reconnects cannot revive a stopped stream.
- Handles snapshot/update actions for order books with incremental updates

### 2. Secondary Public WebSocket #1 (`bitget.ws-secondary-public.ts`, index=1)

- **URL**: Same as public WebSocket
- **Channels**: `ticker` for 1/3 of all markets (index `i % 3 === 1`)
- Ticker-only stream to distribute load across connections

### 3. Secondary Public WebSocket #2 (`bitget.ws-secondary-public.ts`, index=2)

- **URL**: Same as public WebSocket
- **Channels**: `ticker` for 1/3 of all markets (index `i % 3 === 2`)
- Ticker-only stream for the remaining third of markets

**Note**: Both public WS classes derive `instType` from `this.parent.apiProductType.toUpperCase()`, matching the private WS. This keeps ticker, candle, and orderbook subscriptions on the selected variant's product type.

### 4. Private WebSocket (`bitget.ws-private.ts`)

- **URL**: `wss://ws.bitget.com/v2/ws/private` (or fallback: `wss://fsjnpy143pbz.asia.tealstreet.io/bitget/v2/private`)
- **Authentication**: Required before subscribing (login op)
- **Channels subscribed**:
  - `account` with `coin: 'default'` - Balance/account updates
  - `positions` - Position changes (snapshot + incremental updates)
  - `orders` - Normal order updates (new, partial-fill, filled, cancel)
  - `orders-algo` - Algo/stop order updates (live, triggered, cancel)

Normal-order fill handling recognizes the documented `partially_filled` status. Fill events use `baseVolume` (latest fill quantity), falling back to `accBaseVolume` when the latest-fill field is absent.

### Reconnection & Fallback URL Logic

All WebSocket connections implement a **fallback URL mechanism** (`bitget.ws-public.ts:26-29`, `bitget.ws-private.ts:16-19`, `bitget.ws-secondary-public.ts:23-27`):

- Each connection tracks `messageCount` and `lastMessageCount`
- On each `connectAndSubscribe()` call, if both `lastMessageCount` and `messageCount` are 0 (no messages received since last connection), `connectionAttempts` increments
- After `MAX_ATTEMPTS` (2) consecutive connections with zero messages, `useFallbackUrl` is set to `true`
- On switch to fallback, a `grafana-info` event is emitted for monitoring
- `messageCount` is reset to 0 on every new connection

### Ping/Pong Heartbeat

- All WS connections send literal string `"ping"` as heartbeat
- Server responds with literal string `"pong"`
- Handled in `onMessage` before JSON parsing (`data === 'pong'`)
- `pingAt` tracks the time for latency measurement via `handlePongEvent()`

### Private WS Reconnection

When `ENABLE_RECONNECTING_WEBSOCKET` is enabled, the private WS `onOpen` handler (`bitget.ws-private.ts:63-71`) triggers `executeOrdersFetch()` to reconcile order state after reconnection.

### Order Book Updates

The public WS `listenOrderBook` (`bitget.ws-public.ts:269-372`) handles incremental order book updates:

- `action: "snapshot"` -- clears and replaces the entire book
- `action: "update"` -- applies incremental changes via `updateOrderBook()` utility
- After each update, book is sorted and totals recalculated

## Order Types and Mappings

### Normal Orders

- **Limit** (`limit`): Standard limit orders with optional `presetStopSurplusPrice` and `presetStopLossPrice` (virtual TP/SL)
- **Market** (`market`): Market orders

### Algo/Stop Orders

- **Position-level TP** (`pos_profit`): Take profit on entire position (amount = 0)
- **Position-level SL** (`pos_loss`): Stop loss on entire position (amount = 0)
- **Sized TP** (`profit_plan`): Take profit with specific amount
- **Sized SL** (`loss_plan`): Stop loss with specific amount
- **Trailing Stop** (`track_plan`): Trailing stop loss with `callbackRatio` (percentage distance)

### Virtual TP/SL on Limit Orders

When a limit order has `presetStopLossPrice` or `presetStopSurplusPrice`, `mapOrder()` (`bitget.exchange.ts:1487-1506`) creates additional virtual orders with suffixed IDs:

- `{orderId}__stop_loss` for the SL component
- `{orderId}__take_profit` for the TP component

These virtual orders require special handling in cancel/update operations (the parent limit order is canceled and re-placed with updated preset prices via `updateVirtualTpSlOrder`, `bitget.exchange.ts:1211-1256`).

### Plan Type Mapping (WS short codes to REST values)

```
psl -> pos_loss       |  ptp -> pos_profit
sl  -> loss_plan      |  tp  -> profit_plan
track -> track_plan
```

The `getCancelPlanType()` method (`bitget.exchange.ts:1639-1658`) maps both WS short codes and REST values to the correct `planType` for cancellation, with a fallback to `getOrderPlanTypeAndAmount()` computed from position state.

### Trigger Types

- `mark_price` (default): Triggers on mark price
- `fill_price`: Triggers on last/fill price (mapped to `OrderStopTriggerType.LastPrice`)

### Order Status Mapping (`bitget.types.ts:89-106`)

```
init, new, not_trigger, partial_fill, partially_fill, partially_filled, live -> Open
triggered, full_fill, filled, executed -> Closed
fail_trigger, cancel, cancelled, canceled, fail_execute -> Canceled
```

### Order Side Mapping (`bitget.types.ts:123-140`)

Supports many variations: `open_long`, `open_short`, `close_long`, `close_short`, `reduce_close_*`, `offset_close_*`, `burst_close_*`, `delivery_close_*`, `buy_single`, `sell_single`, `buy`, `sell`.

Outbound V2 order payloads use the documented lowercase `buy`/`sell` and `open`/`close` values. Keep inbound aliases separate from this wire contract.

### Time In Force Mapping

```
normal / gtc -> GoodTillCancel
post_only -> PostOnly
fok -> FillOrKill
ioc -> ImmediateOrCancel
```

## Order Lifecycle

### Order Placement

1. **Normal orders** are formatted via `formatCreateOrder()` (`bitget.exchange.ts:1027-1083`):
   - Position mode is fetched to determine `side`/`tradeSide` fields
   - Orders exceeding `maxSize` are split into multiple lots
   - Plain single-payload limit/market orders use caller `clientOrderId` as `clientOid` when provided
   - Caller `clientOid` is validated against the classic v2 charset `^[0-9A-Za-z_:#\-+\s]{1,32}$` (the adapter calls classic `/api/v2/mix/...` endpoints, so the UTA regex does not apply). Bitget keeps NO ID-embedded prefix — attribution rides the `X-CHANNEL-API-CODE: Tealstreet` header — so the caller budget stays 32.
   - Split orders keep generated unique `clientOid` values only when no caller ID is supplied; caller IDs on splits or attached TP/SL brackets reject before any request is sent (`BaseExchange.rejectPreflight`)
   - Placed via `placeOrderBatch()` using `BATCH_ORDERS` endpoint (up to 50 per symbol per batch)
2. **Algo orders** are formatted via `formatAlgoOrder()` (`bitget.exchange.ts:1085-1148`):
   - Placed one at a time via `placeAlgoOrders()`
   - Caller `clientOrderId` values reject pre-flight — algo payloads always use generated `clientOid`. In `placeOrders`, formatting of both groups completes (and any rejection propagates) before anything is sent
   - Trailing stops route to `PLACE_TRAILING_STOP_ORDER`, others to `PLACE_ALGO_ORDER`
   - Trailing stops compute `callbackRatio` as percentage distance from current price and use the documented `place-plan-order` contract

### Order Update (Amend vs Cancel-Replace)

- **Normal orders** (`updateOrder`, `bitget.exchange.ts:1258-1291`): Always **cancel + re-place** (no server-side amend). The existing order is canceled, then a new order is placed with updated parameters.
- **Virtual TP/SL orders** (`updateVirtualTpSlOrder`, `bitget.exchange.ts:1211-1256`): Cancels the parent limit order and re-places it with updated preset prices. All related orders (parent + `__stop_loss` + `__take_profit`) are found by splitting on `__`.
- **Standalone algo orders** (`updateAlgoOrder`, `bitget.exchange.ts:1293-1334`): Uses the `UPDATE_ALGO_ORDER` endpoint for in-place modification. When `size` is empty string, it indicates a position-level stop (no specific quantity).
  - If updating amount to 0, preserves the order's current size to avoid converting sized stops to position-level
  - Position-level orders (amount=0) require `payload.size = ''`

### Order Cancellation

- **Normal orders** (`cancelNormalOrders`, `bitget.exchange.ts:922-951`): Virtual TPSL suffixes are stripped to get the actual `orderId`; orders are deduped by ID, grouped by symbol, chunked into lots of 50.
- **Algo orders** (`cancelAlgoOrders`, `bitget.exchange.ts:953-969`): Canceled one at a time; requires correct `planType` from `getCancelPlanType()`.
- **Cancel all** (`cancelAllOrders`, `bitget.exchange.ts:902-911`): Single POST to `CANCEL_ALL_ORDERS`.

### Plan Type Determination (`bitget.exchange.ts:1661-1686`)

The `getOrderPlanTypeAndAmount()` method determines whether an algo order is position-level or sized:

- If `amount === 0`, the order is position-level (`pos_loss`/`pos_profit`)
- If `amount !== 0` and a matching position is found with `contracts >= amount`, the order is a sized stop (`loss_plan`/`profit_plan`)
- If `amount !== 0` but no matching position with sufficient contracts is found, the order falls back to position-level (`pos_loss`/`pos_profit`)
- Trailing stops always use `track_plan`

## Symbol Format

- **USDT-M (default)**: Symbols like `BTCUSDT`, `ETHUSDT`. Product type: `usdt-futures` (testnet: `susdt-futures`)
- **USDC-M**: Same symbol format but with USDC quote. Product type: `usdc-futures` (testnet: `susdc-futures`)
- **Inverse**: Symbols like `BTCUSD`, `ETHUSD` (quote is `USD`). Product type: `coin-futures` (testnet: `scoin-futures`)
- **Internal suffix stripping**: API responses include suffixed symbols like `BTCUSDT_USDT-FUTURES`; these are stripped via `.replace('_${productType.toUpperCase()}', '')`
- **`$` stripping**: The API interceptor strips `$` from symbol params/body before signing (used internally for chart kline exclusions). The kline endpoint is excluded from this stripping (`bitget.api.ts:46`).
- **Market resolution** (`_resolveMarket`, `bitget.exchange.ts:452-475`): Supports multiple input formats (`BTCUSDT`, `BTC-PERP`, `BTC-USDT`, `BTC`) by normalizing separators and trying suffix variations.

## Data Precision & Conversion

### Price Precision (`bitget.exchange.ts:563-564`)

Calculated from API fields `pricePlace` and `priceEndStep`:

```typescript
priceDecimals = 10 / 10 ** (parseFloat(m.pricePlace) + 1);
pricePrecision = priceDecimals * parseFloat(m.priceEndStep);
```

For example, `pricePlace=2, priceEndStep=5` yields `0.1 * 5 = 0.5` (price moves in 0.5 increments).

### Amount Precision

Uses `sizeMultiplier` directly from the API as `precision.amount`.

### Rounding

All prices and amounts are rounded via the `adjust()` utility from `safe-math` before being sent to the API.

### Inverse Contract Conversion (`bitgetinverse.exchange.ts:227-249`)

For inverse contracts, USD contract sizes are converted to crypto amounts for display:

```
cryptoAmount = contractsNative / referencePrice
```

The `referencePrice` is taken from ticker mark price, ticker last price, or entry price (in that priority). Results are rounded to 8 decimal places. The native USD value is stored in `extra.contractsNative`.

### Balance Calculation (`bitget.exchange.ts:279-285`)

```
used = unionTotalMargin - unionAvailable
free = unionAvailable
total = unionTotalMargin - unrealizedPL
upnl = unrealizedPL
```

### Realized PnL Calculation (`bitget.exchange.ts:432-435`)

```
realizedPnl = achievedProfits + totalFee - deductedFee
```

## Rate Limiting

- **Rate-limited XHR**: `maxRPS: 3` (3 requests per second) for general API calls
- **Unlimited XHR**: A separate Axios instance (`unlimitedXHR`) without rate limiting, used for:
  - Batch order placement (`BATCH_ORDERS`)
  - Order cancellation (`CANCEL_ORDERS`, `CANCEL_ALL_ORDERS`, `CANCEL_ALGO_ORDER`)
  - Algo order placement and updates
- **Batch size**: Orders are chunked into groups of 50 per symbol for batch operations
- **Recv window**: 20,000ms default timeout (`RECV_WINDOW`), configurable via `options.extra.recvWindow`
- **Network retry**: 3 retries on network errors via `axios-retry` (`bitget.api.ts:36`)
- **Tunnel support**: When `options.tealstreetTunnel` is set, the `axiosHttpOverWebsocketAdapter` is used (`bitget.api.ts:85-87`)

## State Reconciliation

### Order Reconciliation (`bitget.ws-private.ts:323-385`)

The private WS triggers throttled REST fetches (500ms debounce) after receiving order events. The `executeOrdersFetch()` method implements sophisticated reconciliation:

1. **Sequence tracking**: A `fetchSeq` counter (`ordersFetchRequestSeq`) ensures stale responses from superseded fetches are discarded (`bitget.ws-private.ts:325,329`)
2. **Tombstone cache** (`terminalOrderTombstones`, 10s TTL): When an order transitions to a terminal state (canceled/filled) via WS, its ID is tombstoned. REST fetches that return this order as "open" within the TTL window are filtered out (`bitget.ws-private.ts:342-345`)
3. **WS-open order grace** (`recentWsOpenOrders`, 5s TTL): Orders recently opened via WS are preserved even if REST doesn't yet return them, preventing premature removal (`bitget.ws-private.ts:374-378`)
4. **Algo order preservation**: WS-delivered algo orders (TP/SL/trailing stops) that REST doesn't return are preserved since Bitget's `orders-plan-pending` doesn't always return TPSL orders placed via `place-tpsl-order` (`bitget.ws-private.ts:366-379`)
5. **Terminal state protection**: If the local store has an order in terminal state (Canceled/Closed/Canceling) but REST returns it as Open, the REST version is filtered out (`bitget.ws-private.ts:353-359`)
6. **Cache pruning**: Both caches are pruned when size exceeds 2000 entries or on each reconciliation (`bitget.ws-private.ts:387-398`)

### Position Reconciliation (`bitget.ws-private.ts:220-295`)

- WS `positions` channel sends both `snapshot` and incremental updates
- `snapshot` action triggers `addOrUpdatePositionsFull()` which replaces the full position set (clearing stale positions that are omitted) (`bitget.ws-private.ts:239`)
- Incremental updates call `addOrUpdatePosition()` for each position individually (`bitget.ws-private.ts:242-244`)
- Empty snapshots (no positions) are applied immediately to clear UI (`bitget.ws-private.ts:228-229`)
- After WS events, `throttledFetchPositions()` schedules a REST refetch (500ms debounce) for authoritative data
- `liquidationPrice` is parsed from both REST and WS payloads. If Bitget
  sends the field blank/missing for an existing open position, preserve the
  previous nonzero value until an explicit numeric value arrives; an explicit
  `"0"` is still treated as authoritative.

### Tick Polling Reconciliation (`bitget.exchange.ts:193-237`)

- **Every tick** (2s): `fetchPositions()` via REST -> `addOrUpdatePositionsFull()`
- **Every 2nd tick**: `fetchBalance()` via REST
- **Every 4th tick**: `fetchNormalOrders()` via REST, but only virtual TP/SL orders (the `__stop_loss` / `__take_profit` orders) are taken from the REST response; existing non-virtual orders in the store are preserved because WS updates are more current
- **Tick interval**: 2,000ms (from metadata `tickInterval`)

## Exchange Metadata

```typescript
{
  requiresPassphrase: true,
  requiresCors: true,
  supportsTestnet: false,           // Note: testnet product types exist but feature is disabled
  supportsTrailingStopLoss: false,  // Implementation exists but not exposed
  supportsTradesHistory: false,
  orderbookTickDelay: 60,
  tickInterval: 2_000,
  stopOrderPendingUpdateDelay: 4_000,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  autoDownSizeReduceOrders: true,
  supportsChaser: true,
  supportsTunnel: true,
  supportsWsRoute: true,
  tvMaxGetBarFetches: 5,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  supportedTPSLTriggerTypes: [MarkPrice],      // Only mark price trigger type
  latencyDomain: 'api.bitget.com',
}
```

### Variant Metadata Differences

- **BitgetUSDC**: Same as Bitget, `defaultQuote: 'USDC'`
- **BitgetInverse**: `enabled: false`, `enableInPreview: false`, `requiresTickersForMarketsLoaded: true`

## Testnet/Demo

Testnet support exists in code but is **disabled** in metadata (`supportsTestnet: false`).

### URL Differences

- **REST**: Same base URL (`https://api.bitget.com`) for both livenet and testnet
- **Product types** change:
  - USDT-M: `susdt-futures` (testnet) vs `usdt-futures` (livenet)
  - USDC-M: `susdc-futures` (testnet) vs `usdc-futures` (livenet)
  - Inverse: `scoin-futures` (testnet) vs `coin-futures` (livenet)
- **Margin coins** change: `SUSDT`/`SUSDC` (testnet) vs `USDT`/`USDC` (livenet)

### Environment Mismatch Detection

`validateAccount()` (`bitget.exchange.ts:113-134`) and `fetchOpenPositions()` (`bitget.exchange.ts:400-412`) detect the `"environment is incorrect"` API error and emit a helpful message: `"API key environment mismatch: Your API key appears to be for [mode], but Tealstreet is configured for [mode]. Please check your Testnet setting."`

## Known Quirks / Implementation Notes

1. **Three public WebSocket connections for tickers**: Markets are split into thirds (`i % 3`) across three separate WebSocket connections to stay within Bitget's per-connection subscription limits. Both public WS classes derive `instType` from the parent exchange, so USDT-M, USDC-M, and future enabled variants subscribe to their matching product type.

2. **Virtual TP/SL orders on limits**: Bitget allows `presetStopSurplusPrice`/`presetStopLossPrice` on limit orders, but these do not stream via the private WebSocket. They are polled every 4th tick and mapped to virtual orders with `__stop_loss`/`__take_profit` ID suffixes.

3. **Order update = cancel + re-place**: Normal order updates are implemented by canceling the existing order and placing a new one (not a true server-side amend). Similarly, virtual TP/SL updates cancel the parent order and re-place it with updated preset prices.

4. **Algo order update endpoint**: The `UPDATE_ALGO_ORDER` endpoint (`modify-tpsl-order`) is used for in-place modification of standalone TP/SL orders. When `size` is empty string, it indicates a position-level stop.

5. **Hedge mode side inversion**: In hedge mode, Bitget inverts the `side` field for reduce-only orders. The WS handler explicitly re-inverts this for both `orders` and `orders-algo` channels. In `orders-algo` (`bitget.ws-private.ts:180`), the inversion is applied unconditionally for hedge mode; in `orders` (`bitget.ws-private.ts:130-134`), only for `reduceOnly === 'yes'`.

6. **REST lag vs WS**: Bitget's REST API can lag behind WebSocket updates by several seconds. The implementation maintains tombstone caches (10s TTL for terminal orders, 5s grace for new WS orders) to prevent stale REST data from overwriting newer WS state.

7. **Bitget REST caches responses**: The REST API caches responses for a few seconds server-side. The tick cycle avoids overwriting limit orders from REST since WS updates are more current.

8. **`toJsonString` underscore stripping**: The `toJsonString()` helper in `bitget.api.ts` strips leading underscores from JSON keys (e.g., `_key` becomes `key` in the serialized output). This is used as a workaround for certain API fields.

9. **Position mode is "global"**: `positionModeIsGlobal: true` in metadata means changing position mode (hedge/one-way) affects all contracts of that product type, which triggers a full position refetch.

10. **Margin type is per-symbol**: `marginTypeIsGlobal: false` means cross/isolated margin can be set independently per symbol.

11. **Trailing stop not exposed**: `supportsTrailingStopLoss: false` remains the product gate. The dormant placement path follows the current `place-plan-order` contract (`planType: track_plan`, `callbackRatio`, market execution) so enabling it does not revive the removed legacy endpoint.

12. **`alwaysCalculateUnrealizedPnlFromLastPrice: true`**: The frontend calculates unrealized PnL from last price rather than trusting the exchange-provided value.

13. **Pagination**: Normal orders, algo orders, and trailing stop orders all use cursor-based pagination with `idLessThan` and `endId`, limited to 200 per page with a safety cap of 20 iterations.

14. **Commented-out leverage fetching**: `fetchCurrentLeverage()` and `fetchLeverageBrackets()` are fully implemented but commented out (`bitget.exchange.ts:593-667`). Leverage is instead obtained from position data.

15. **OHLCV timestamp division**: Bitget returns candle timestamps in milliseconds; code divides by 1000 (`bitget.exchange.ts:882`) for internal format (seconds).

16. **Ticker `change24h` is a ratio**: The `change24h` field from the API is a decimal ratio (e.g., 0.05 for 5%), so it is multiplied by 100 for percentage display (`bitget.ws-public.ts:184`).

17. **`mapPosition` defensive guards**: The `mapPosition` method (`bitget.exchange.ts:1510-1570`) has defensive null/undefined checks and logs warnings when required fields (`total`, `openPriceAvg`) are missing, returning a zeroed-out position for invalid payloads.

18. **One-way mode isolated leverage mismatch warning**: `fetchPositionMetadata()` (`bitget.exchange.ts:326-332`) warns if isolated long and short leverages differ in one-way mode.

19. **Response interceptor error logging**: The response interceptor in `bitget.api.ts` fires `emitAndLogError` (user-facing toast) when `code !== '00000'` and data has no `data` field; it does not throw, so the caller still receives the response. Codes in the file-scope `SILENT_ERROR_CODES` set are skipped — currently `'40084'` ("Classic Account mode"), because `_detectAccountType` deliberately probes a v3 endpoint expecting that exact response on Classic accounts. **Add new codes to that set when you have a probe / fallback path that legitimately expects an error**; do not relax the interceptor itself.

20. **Trailing stop orders response format**: `fetchTrailingStopOrders()` handles both array and `{ entrustedList }` response formats (`bitget.exchange.ts:824-828`).

21. **UTA vs Classic — per-side ISO leverage routing**: The v2 endpoint `set-leverage` silently couples long and short leverage on **all** accounts (Bitget removed per-side support from v2). Per-side ISO leverage in hedge mode only works through the v3 UTA endpoint (`SET_LEVERAGE_V3`) with `posSide`. UTA accounts are detected by `_detectAccountType()` (`bitget.exchange.ts`), a one-time probe of `ACCOUNT_SETTINGS_V3` during `start()` — 200 → `isUta=true`, 40084 (`Classic Account mode`) → `isUta=false`. The probe runs in parallel with the first `tick()` so it adds zero serial startup latency. **Do NOT** auto-detect from `BALANCE` / `account/account` responses — `assetMode` and `unionAvailable` are populated on Classic too (verified against the `bg e2e` Classic account). Every future v3-routing method must read `this.isUta`; do not reintroduce trial-and-error fallbacks in setLeverage. The `40084` reply is a **feature signal**, not an error — it lives in `SILENT_ERROR_CODES` in `bitget.api.ts` so the response interceptor does not surface it as a toast on every Classic-account login. If you add another v3-probing pattern, extend that set (see quirk 19).

22. **Order placement resolves position-mode + margin-type via the account endpoint, NOT via an open position.** `formatCreateOrder` / `placeOrderBatch` need `posMode` + `marginMode` to build the `side`/`tradeSide`/`marginMode` fields, and bitget is the **only** adapter that gates _every_ order on resolving these (woo/bitmex derive side without it). `getOrFetchPositionMode` / `getOrFetchMarginType` therefore go through `resolveSymbolPositioning(symbol)`, which prefers an existing position but **falls back to `fetchPositionMetadata([symbol])`** (a GET to `FETCH_SINGLE_ACCOUNT` carrying `posMode`/`marginMode` with no open position required). **This fallback is load-bearing in hosted-exchange-server mode:** user order writes run on the never-`start()`ed main-thread mirror (see `apps/web/src/controllers/CLAUDE.md` → dual-instance model), whose `store.loaded.positions` is `false`, so `getOrFetchPositionsForSymbol` returns `[]` for a flat symbol. The previous code **raw-threw** there ("No positions found …") — and because the throw bypassed `emitAndLogError`, orders silently never POSTed (no toast, no log, position never opened → e2e "expected 0 to equal 1"). Do **not** revert these methods to throwing on an empty position list.

23. **`formatCreateOrder` lot-split must guard `maxSize > 0`.** The large-order split is `lots = (maxSize > 0 && amount > maxSize) ? ceil(amount / maxSize) : 1`. The `maxSize > 0` guard is load-bearing: on the hosted-server **mirror**, the streamed market can carry `limits.amount.max === 0`, and `ceil(amount / 0) = Infinity` — lodash **`times(Infinity)` returns `[]`**, so `formatCreateOrder` produced **zero payloads** and the order silently never POSTed (no throw, no error; e2e "expected 0 to equal 1"). The same zeroed `maxSize` would surface as `rest = amount % 0 = NaN`, so `rest` is guarded identically. Real users in worker mode have a real `maxSize` and never hit this. **Root cause not yet fixed:** why the mirror's bitget market gets `limits.amount.max === 0` (server→mirror market serialization) is a separate follow-up — the guard makes order placement correct regardless, but if you touch market hydration, check whether the mirror's `limits` survive the stream.

## Error Handling

### REST API Error Codes

- **Error response format**: `{ code: string, msg: string, data: ... }`. Code `"00000"` or `0` indicates success.
- **`"environment is incorrect"`**: API key/testnet mismatch -- detected in `validateAccount()` and `fetchOpenPositions()` with a user-friendly message.
- **Batch order failures**: `data.failureList` items are logged individually via `emitAndLogError()` and throw (`bitget.exchange.ts:1172-1175`).

### Network Retry

- `axios-retry` configured with 3 retries on network errors (`bitget.api.ts:36`)

### WebSocket Error Recovery

- Private WS events that fail to parse are silently skipped (JSON parse returns null)
- Position/order fetch failures in throttled reconciliation are caught silently (`bitget.ws-private.ts:292-294, 383-384`)
- `executeOrdersFetch` checks `fetchSeq` to discard stale responses from superseded fetches

### Error Emission Pattern

Most methods follow the pattern:

```typescript
catch (err: any) {
  this.emitAndLogError(err?.response?.data?.msg || err?.message);
  throw err;  // Re-throw for callers
}
```

Some methods (like `tick()`) catch errors without re-throwing to prevent tick loop interruption.

## Exchange Variants

### `BitgetExchange` (USDT-M Linear) - `bitget`

- **Product type**: `usdt-futures` / `susdt-futures` (testnet)
- **Margin coin**: `USDT` / `SUSDT` (testnet)
- **Status**: Enabled, production-ready
- The base implementation; all other variants extend this class

### `BitgetUSDCExchange` (USDC-M Linear) - `bitgetusdc`

- **Product type**: `usdc-futures` / `susdc-futures` (testnet)
- **Margin coin**: `USDC` / `SUSDC` (testnet)
- **Status**: Enabled
- Minimal override (`bitgetusdc.exchange.ts`): only changes `apiProductType` and `apiMarginCoin` getters
- Uses type assertion (`as any`) since the getter return types don't match the base class
- Inherits all trading logic, WebSocket handlers, and order management from base class

### `BitgetInverseExchange` (Coin-Margined) - `bitgetinverse`

- **Product type**: `coin-futures` / `scoin-futures` (testnet)
- **Margin coin**: Per-symbol base asset (e.g., `BTC` for `BTCUSD`, `ETH` for `ETHUSD`)
- **Status**: Disabled (`enabled: false`), not yet in preview
- Extensive overrides needed because inverse contracts use the base asset as margin:
  - `fetchMarkets()`: Filters by `quoteCoin === 'USD'` instead of `'USDT'`; sets `isInverse: true`
  - `fetchBalance()`: Aggregates multi-currency balances (BTC, ETH, etc.) with USD conversion via tickers; populates `coins` field
  - `fetchOpenPositions()`: Fetches per-marginCoin, converts USD contract sizes to crypto amounts (`contractsNative / referencePrice`)
  - `mapOrder()`: Converts USD remaining to crypto for display; stores `extra.remainingNative`
  - `cancelAllOrders()`: Iterates over all margin coins
  - `cancelNormalOrders()` / `cancelAlgoOrders()`: Uses per-symbol `getMarginCoinForSymbol()`
  - `placeOrderBatch()`: Uses per-symbol margin coin
  - `setLeverage()` / `changeMarginType()`: Uses per-symbol margin coin
  - Has its own `generateUuid()` helper instead of using the shared `uuid()` utility

## Timeframe / Interval Mapping

```
1m -> "1m"    3m -> "3m"     5m -> "5m"     15m -> "15m"    30m -> "30m"
1h -> "1H"    2h -> "2H"     4h -> "4H"     6h -> "6Hutc"   12h -> "12Hutc"
1d -> "1Dutc" 1w -> "1Wutc"
```

Note: 6h and above use UTC-anchored intervals (`Hutc`, `Dutc`, `Wutc`).

## Order Side Logic

### Normal Orders (V2 API) (`bitget.exchange.ts:1572-1584`)

- **One-way mode**: `side` = `Buy`/`Sell`, no `tradeSide`
- **Hedged mode (open)**: `tradeSide` = `Open`, `side` = direction
- **Hedged mode (close/reduce)**: `tradeSide` = `Close`, side is **inverted** (Buy becomes Sell, Sell becomes Buy)

### Algo/Stop Orders (`bitget.exchange.ts:1586-1623`)

- **Hedged mode**: `holdSide` = `long`/`short` (position being protected)
- **One-way mode**: `holdSide` = `buy`/`sell`
- **Trailing stop**: Uses `side` plus `tradeSide: close` in hedge mode, with `buy`/`sell` order directions.

## Testing / Validation

Run targeted type checking:

```bash
cd packages/safe-cex && yarn typecheck
```

Key validation points:

- `validateAccount()` tests API key validity by fetching balance
- `getAccount()` uses the V2 spot account-info endpoint and maps its camelCase user, parent, and inviter IDs
- Environment mismatch detection in both `validateAccount()` and `fetchOpenPositions()`
