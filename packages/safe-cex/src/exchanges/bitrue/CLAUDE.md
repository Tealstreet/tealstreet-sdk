# Bitrue Exchange Integration

## Exchange Information

- **Type**: Centralized Exchange (CEX)
- **Markets**: Perpetual futures (USDT and USDC margined)
- **Position Mode**: One-way only (no hedged mode)
- **Margin Types**: Cross and Isolated (detected per-symbol, not changeable via API)
- **Quote Currency**: USDT (primary), USDC

## API Documentation

- **Official API Documentation**: https://github.com/Bitrue-exchange/bitrue-official-api-docs

## CCXT Reference Implementation

- **REST API Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/bitrue.ts
- **WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/bitrue.ts

## File Structure

| File                   | Purpose                                                                             |
| ---------------------- | ----------------------------------------------------------------------------------- |
| `bitrue.exchange.ts`   | Main exchange class (`BitrueExchange`) extending `BaseExchange`                     |
| `bitrue.api.ts`        | REST API client with HMAC-SHA256 authentication and BigInt-safe HTTP adapter        |
| `bitrue.types.ts`      | TypeScript type definitions, endpoint constants, base URLs, WS types, mappings      |
| `bitrue.utils.ts`      | Utility function `reverseWsSymbol` for converting WS symbol format to internal      |
| `bitrue.ws-private.ts` | Private WebSocket handlers for order updates and account events (listen key based)  |
| `bitrue.ws-public.ts`  | Public WebSocket handlers for tickers, klines, and orderbook (binary/blob messages) |

## Authentication

### REST API (`bitrue.api.ts`)

Signing uses HMAC-SHA256 with the following process:

1. Get current timestamp via `virtualClock.getCurrentTimeMs()`
2. Concatenate: `timestamp + HTTP_METHOD(uppercase) + path(with query string) + body`
3. Remove spaces from the concatenated string (`.replace(' ', '')` -- note: only replaces first space)
4. Sign with `createHmac('sha256', secret).update(concatenated).digest('hex')`

**Request headers** (set in interceptor, `bitrue.api.ts:51-83`):

- `X-CH-APIKEY` - API key
- `X-CH-SIGN` - HMAC-SHA256 hex signature
- `X-CH-TS` - Timestamp in milliseconds
- `recvWindow` - Receive window (default 20,000ms, from `RECV_WINDOW` constant)
- `X-Referer` - Broker ID (`Tealstreet`)
- `Content-Type: application/json`

**Special behaviors**:

- Uses `bigIntAxiosHttpOverWebsocketAdapter` (not standard adapter) because Bitrue returns order IDs as large integers that exceed `Number.MAX_SAFE_INTEGER`
- Proxy URL handling: If URL starts with `http`, splits on `.com` to extract base URL (`bitrue.api.ts:57-59`)
- No response interceptor -- error handling is done at the call site by checking `data.code !== '0'`

### Private WebSocket Authentication (`bitrue.ws-private.ts`)

Bitrue uses a **listen key** approach (similar to Binance) instead of direct API key signing:

1. `POST` to `https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey` to obtain a `listenKey` (`bitrue.ws-private.ts:244-252`)
2. Response format: `{ code: 200, data: { listenKey: "..." } }` -- code must be `200` (number, not string)
3. Connect to WebSocket with `?listenKey={key}` query parameter
4. Listen key is refreshed every 30 minutes via `PUT` to `{LISTEN_KEY_URL}/{listenKey}` (`bitrue.ws-private.ts:254-258`)
5. Refresh is scheduled via `setTimeout(() => this.updateListenKey(listenKey), 30 * 60 * 1000)` on both initial fetch and each refresh

No direct WebSocket signing is needed; authentication is handled entirely through the listen key.

## Startup Sequence

Defined in `bitrue.exchange.ts:102-141` (`start()` method):

1. **Check `publicDataOnly`** -- skips private data if no API keys or `publicDataOnly` flag
2. **`fetchMarkets()`** (sequential) -- fetches `/fapi/v1/contracts` via `unlimitedXHR` (no rate limit)
3. **Create ticker stubs** -- creates empty `Ticker` objects (all zeros) for every market so WS deltas can be applied immediately
4. **Store update**: markets + tickers + `loaded.markets = true` + `loaded.tickers = true`
5. **`publicWebsocket.connectAndSubscribe()`** (non-blocking) -- connects public WS, subscribes to tickers for all markets on open
6. If authenticated:
   - **`privateWebsocket.connectAndSubscribe()`** (non-blocking) -- fetches listen key, connects private WS
   - **`fetchBalanceAndPositions()`** (sequential) -- fetches `/fapi/v2/account`, parses both balance and positions. Calls `fetchPositionMetadata()` for each symbol with open positions.
   - **Store update**: balance + positions + loaded flags
   - **`fetchOrders()`** (sequential) -- Since no symbols are provided, calls `fetchAndEmitOrders()` (asynchronous per-symbol fetch with 100ms delays)
   - **Store update**: orders + `loaded.orders = true`
   - **`tick()`** (starts polling loop)

Note: `fetchOrders()` without symbols triggers `fetchAndEmitOrders()` which iterates all markets asynchronously and returns `[]` immediately (`bitrue.exchange.ts:468-477`).

## REST API Endpoints

All endpoints defined in `ENDPOINTS` constant (`bitrue.types.ts:13-23`):

| Constant       | Path                                                          | Method   | Description                                         |
| -------------- | ------------------------------------------------------------- | -------- | --------------------------------------------------- |
| `MARKETS`      | `/fapi/v1/contracts`                                          | GET      | Fetch available futures contracts                   |
| `TICKERS`      | `/fapi/v1/ticker`                                             | GET      | Fetch ticker data (unused in favor of WS)           |
| `ACCOUNT`      | `/fapi/v2/account`                                            | GET      | Fetch account balance and positions                 |
| `ORDERS`       | `/fapi/v2/openOrders`                                         | GET      | Fetch open orders per symbol (`contractName` param) |
| `PLACE_ORDER`  | `/fapi/v2/order`                                              | POST     | Place a new order                                   |
| `CANCEL_ORDER` | `/fapi/v2/cancel`                                             | POST     | Cancel an order                                     |
| `SET_LEVERAGE` | `/fapi/v2/level_edit`                                         | POST     | Set leverage for a symbol                           |
| `LISTEN_KEY`   | `https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey` | POST/PUT | Obtain/refresh listen key for private WS            |
| `KLINE`        | `/fapi/v1/klines`                                             | GET      | Fetch OHLCV data (unused -- klines fetched via WS)  |

**Base URL**: `https://fapi.bitrue.com` (`bitrue.types.ts:7`)

## WebSocket Implementation

### WebSocket URLs

- **Public**: `wss://futuresws.bitrue.com/kline-api/ws` (with `?requestId=<nanoid>` appended)
- **Private**: `wss://fapiws.bitrue.com/stream` (with `?listenKey=<key>` appended)

### Public WebSocket (`bitrue.ws-public.ts`)

**Connection** (`connectAndSubscribe`, line 32-49):

1. Pre-registers ticker handlers for all markets before connecting (so they are ready when WS opens)
2. Creates WebSocket to public URL with unique `requestId`
3. Registers `onOpen`, `onMessage`, `onClose` handlers

**On open** (`onOpen`, line 51-55):

1. `subscribe()` -- sends sub messages for all tracked topics
2. `ping()` -- no-op (reversed ping/pong)

**Subscribe/unsubscribe format**:

```json
{ "event": "sub", "params": { "channel": "<topic>", "cb_id": "<nanoid>" } }
{ "event": "unsub", "params": { "channel": "<topic>", "cb_id": "<nanoid>" } }
```

**Request/response format** (for kline history):

```json
{ "event": "req", "params": { "channel": "<topic>", "cb_id": "<nanoid>" } }
```

Response: `{ "event_rep": "rep", "channel": "<topic>", "data": [...] }` -- resolved via `resolveRequests` map with 5-second timeout (`bitrue.ws-public.ts:192-208`).

**Public channels**:

| Channel Pattern                      | Description                                            |
| ------------------------------------ | ------------------------------------------------------ |
| `market_{wsSymbol}_ticker`           | Ticker updates (subscribed for all markets on connect) |
| `market_{wsSymbol}_kline_{interval}` | Kline/candlestick data (on demand)                     |
| `market_{wsSymbol}_depth_step0`      | Full orderbook snapshots                               |

**Ping/pong** (reversed from typical exchanges, `bitrue.ws-public.ts:59-63`):

- Server sends `{ "ping": <timestamp> }` message
- Client responds with literal string `"pong"` (not JSON, not wrapped)
- `handlePongEvent(20_000)` called with 20-second timeout for health checking
- The `ping()` method is a no-op

**Binary messages** (`bitrue.ws-public.ts:143-149`): Public WS may send messages as `Blob` objects. These are read via `readBlob()` utility before JSON parsing.

**Message routing** (`onMessage`, line 143-173):

1. If `Blob`, read via `readBlob()`
2. JSON parse the string
3. If `json.ping` -> handle ping event
4. If `json.event_rep === 'rep'` -> resolve pending request callbacks
5. Otherwise -> dispatch to message handlers by `json.channel`, passing `json.tick || json.data`

### Private WebSocket (`bitrue.ws-private.ts`)

**Connection** (`connectAndSubscribe`, line 10-24):

1. Fetch listen key via `POST` to listen key URL
2. Connect to private WS URL with `?listenKey=<key>`
3. Register event handlers

**On open** (`onOpen`, line 26-30):

1. `subscribe()` -- sends `{ "event": "sub", "params": { "channel": "user_account_update" } }`
2. `ping()` -- no-op (reversed ping/pong)

**Ping/pong** (reversed, `bitrue.ws-private.ts:235-239`):

- Server sends `{ "event": "ping" }` message
- Client does NOT respond with pong (only records `pingAt` timestamp)
- No health check timeout is set (unlike public WS)

**Event types handled** (`onMessage`, line 33-67):

| Event                 | Handler                  | Description                                  |
| --------------------- | ------------------------ | -------------------------------------------- |
| `ORDER_TRADE_UPDATE`  | `handleOrderTradeUpdate` | Order fills, new orders, status changes      |
| `CANCEL`              | `handleOrderTradeUpdate` | Order cancellation (same handler)            |
| `ACCOUNT_UPDATE`      | `handleAccountUpdate`    | Account balance changes (handler is a no-op) |
| `user_account_update` | inline check             | Subscription confirmation (checks `status`)  |
| `ping`                | `handlePingEvent`        | Server ping (records timestamp only)         |

**JSON parsing**: Uses `bigIntAsStringJsonParse` to handle large integer order IDs safely (`bitrue.ws-private.ts:39`).

### Reconnection

Both WebSockets use the `BaseWebSocket.onClose()` handler:

- On close, removes event listeners, schedules reconnect after `WS_MIN_RECONNECTION_DELAY`
- Private WS: re-fetches listen key on reconnect (new `connectAndSubscribe()` call)
- Public WS: re-subscribes to all tracked topics and re-registers ticker handlers

## Order Types and Mappings

### Order Type Mapping (`bitrue.types.ts:40-45`)

| Internal Type | Bitrue API Type      |
| ------------- | -------------------- |
| `Limit`       | `LIMIT`              |
| `Market`      | `MARKET`             |
| `StopLoss`    | `STOP_MARKET`        |
| `TakeProfit`  | `TAKE_PROFIT_MARKET` |

**Trailing stop loss**: Not supported.

### Order Placement Fields (`formatCreateOrder`, `bitrue.exchange.ts:653-910`)

| Field              | Description                                      |
| ------------------ | ------------------------------------------------ |
| `contractName`     | Symbol (e.g., `E-BTC-USDT`)                      |
| `leverage`         | Leverage level (from position metadata)          |
| `price`            | Limit price (0 for stop/TP orders)               |
| `triggerPrice`     | Trigger price (for conditional orders only)      |
| `volume`           | Order quantity in contracts                      |
| `type`             | `LIMIT`, `MARKET`, etc.                          |
| `side`             | `BUY` or `SELL`                                  |
| `open`             | Direction: `OPEN` or `CLOSE` (from `reduceOnly`) |
| `positionType`     | 1 = cross, 2 = isolated                          |
| `futureModel`      | Always `normal`                                  |
| `conditionOrder`   | `true` for stop/TP orders                        |
| `clientOrderId`    | UUID for conditional orders                      |
| `triggerType`      | 1 = stop loss, 2 = take profit                   |
| `triggerPriceType` | 1 = last price, 2 = mark price                   |

### Time In Force

Not explicitly sent in order requests. All orders default to GTC. REST API order responses always map to `OrderTimeInForce.GoodTillCancel`.

### Market Order Quote-Denomination (`bitrue.exchange.ts:871-883`)

For non-reduce-only market orders, Bitrue expects quote-denominated `amount` instead of base-denominated `volume`:

1. Get reference price from ticker (`mark || last || bid || ask`)
2. Replace `volume` with `amount` in payload
3. Calculate: `amount = adjust((parseFloat(volume) + market.precision.amount / 3) * referencePrice, 0.0001)`
4. The `precision.amount / 3` buffer prevents rounding-down rejections
5. Floor to `market.info.minOrderMoney` minimum

### Large Order Splitting (`bitrue.exchange.ts:856-868`)

If order amount exceeds `market.limits.amount.max`:

1. Calculate number of lots: `Math.ceil(amount / maxSize)`
2. Calculate remainder: `amount % maxSize`
3. Create array of equal-sized payloads plus one remainder payload
4. Each lot has its own volume string

## Order Lifecycle

### Order Placement (`placeOrder`, `bitrue.exchange.ts:912-921`)

1. Format order(s) via `formatCreateOrder()` (may produce multiple payloads for large orders or attached TP/SL)
2. Execute via `placeOrderBatch()` which chunks payloads into groups of 5, placing each chunk in parallel
3. **Special handling for TP/SL orders**: Returns `[]` and triggers `callMethodInWorker('fetchAndSetOrders')` because stop orders return incorrect IDs from the placement API

### Order Batch Execution (`placeOrderBatch`, `bitrue.exchange.ts:942-964`)

- Chunks payloads into groups of 5
- Each chunk executed in `Promise.all()` (parallel within chunk, sequential between chunks)
- Uses `unlimitedXHR` (no rate limiting)
- Validates response: `data.code !== '0'` triggers error

### Order Modification (Cancel-and-Replace, `updateOrder`, `bitrue.exchange.ts:143-159`)

No native order amendment. Updates implemented as:

1. Merge existing order with update fields (`price`, `amount`)
2. Cancel existing order via `cancelOrders([order])`
3. Place new order via `placeOrder(mergedOrder)`

### Order Cancellation (`cancelOrders`, `bitrue.exchange.ts:1013-1051`)

1. Build cancel payloads with `contractName`, `conditionOrder` flag (true for non-Limit orders), and either `clientOrderId` or `orderId`
2. Chunk into groups of 50
3. Execute each chunk in `Promise.all()` (parallel within chunk)
4. **Critical**: After cancellation, calls `callMethodInWorker('removeOrders', orderIds)` because Bitrue WebSocket does **not** reliably send cancel updates

### Order Fetching (`fetchOrdersForSymbol`, `bitrue.exchange.ts:505-585`)

- Fetches per-symbol via `GET /fapi/v2/openOrders?contractName={symbol}` using `unlimitedXHR`
- Type determination logic for orders without `type` field: uses `triggerType` (1=SL, 2=TP), fallback to `Market`
- Status mapping: `INIT` or `action === 'OPEN'` -> Open, otherwise Closed
- For TP/SL orders: `amount` comes from `executedQty`, `filled` is always 0 (assumed untriggered)

### Async Order Fetch (`fetchAndEmitOrders`, `bitrue.exchange.ts:479-503`)

When called without specific symbols:

1. Iterates all markets sequentially
2. Respects priority symbols (fetched first) via `this.prioritySymbols` set
3. 100ms `sleep()` between each symbol fetch to avoid overwhelming API
4. Cancellable: tracks `inProgressOrderFetchId`, aborts if a newer fetch starts
5. Updates store incrementally via `store.addOrUpdateOrders()`

### Order Status Mapping (WebSocket, `bitrue.ws-private.ts:155-184`)

| `x` (execution type) | `X` (status)  | Internal Status        | Action          |
| -------------------- | ------------- | ---------------------- | --------------- |
| `NEW`                | `INIT`        | `OrderStatus.Open`     | Add to store    |
| `TRADE`              | `FILLED`      | `OrderStatus.Closed`   | Emit fill event |
| `TRADE`              | `PART_FILLED` | `OrderStatus.Open`     | Emit fill event |
| `CANCEL`             | `CANCELED`    | `OrderStatus.Canceled` | Update store    |

Market orders (`type === OrderType.Market`) are silently ignored in WS handler (`bitrue.ws-private.ts:186-188`).

## Symbol Format

- **Internal format**: `E-{BASE}-{QUOTE}` (e.g., `E-BTC-USDT`, `E-ETH-USDC`)
- **WebSocket format**: `e_{base}{quote}` lowercase (e.g., `e_btcusdt`)
- **Market ID**: Same as symbol (e.g., `E-BTC-USDT`)

### Symbol Conversions

- **Internal to WS** (`convertToWsSymbol`, `bitrue.ws-public.ts:26-30`): `E-BTC-USDT` -> takes substring from index 2 (`BTC-USDT`), lowercases, removes hyphens -> `btcusdt`, prepends `e_` -> `e_btcusdt`
- **WS to Internal** (`reverseWsSymbol`, `bitrue.utils.ts:7`): `E_BTCUSDT` -> replaces `_` with `-` -> `E-BTCUSDT` -> inserts `-` before `USD` -> `E-BTC-USDT`
- **Base/quote extraction**: Split symbol on `-`, take index 1 (base) and index 2 (quote) (`bitrue.exchange.ts:594-595`)

## Data Precision & Conversion

### Market Precision (`fetchMarkets`, `bitrue.exchange.ts:587-618`)

- `precision.amount`: `market.multiplier` (contract multiplier, used as step size)
- `precision.price`: `1 / 10 ** market.pricePrecision` (e.g., pricePrecision=2 -> 0.01)
- `limits.amount.min`: `market.minOrderVolume * multiplier`
- `limits.amount.max`: `market.maxMarketVolume * multiplier`
- `limits.leverage.max`: Hardcoded to `100` (TODO comment: "where to fetch this?")

### Price Rounding

Uses `adjust()` utility from `../../utils/safe-math` for rounding prices and amounts to market precision.

### Kline Timezone Conversion (`mapKline`, `bitrue.ws-public.ts:80-105`)

Raw candle `ds` (date string) field is in **Singapore timezone (UTC+8)**. Conversion:

```typescript
dayjs(candle.ds)
  .utcOffset(8 * 60, true)
  .valueOf() / 1000;
```

Outputs timestamp in seconds (divided by 1000).

### UPnL Calculation (`parsePositions`, `bitrue.exchange.ts:416`)

Calculated locally rather than from API:

```
unrealizedPnl = (side === Long ? 1 : -1) * (lastPrice - entryPrice) * volume
```

Uses ticker's last price from store.

## Rate Limiting

- **Rate-limited client** (`xhr`): `maxRPS: 5` (5 requests per second via `axios-rate-limit`, `bitrue.exchange.ts:81`)
- **Unlimited client** (`unlimitedXHR`): No rate limit, used for order placement, market fetching, and leverage detection (`bitrue.exchange.ts:82`)
- **Request timeout**: Configurable via `recvWindow` (default 20,000ms, `bitrue.types.ts:3`)
- **Axios retry**: 3 retries on network errors OR `error sign!` responses (`bitrue.api.ts:12-17,46-49`)
- **Order batch placement**: Chunks of 5 orders placed in parallel (`bitrue.exchange.ts:942-964`)
- **Order cancellation**: Chunks of 50 cancelled in parallel (`bitrue.exchange.ts:1023`)
- **Async order fetch delay**: 100ms `sleep()` between per-symbol fetches (`bitrue.exchange.ts:500`)
- **OHLCV caching**: WS kline responses cached for 10 seconds to avoid redundant requests (`bitrue.exchange.ts:625-627`)

## State Reconciliation

### Orders

- REST fetches per-symbol via `fetchOrdersForSymbol()`, returns full set for that symbol
- WS `handleOrderTradeUpdate()` calls `store.addOrUpdateOrder()` for incremental updates
- WS does NOT reliably send cancel updates -- after `cancelOrders()`, manual `callMethodInWorker('removeOrders')` is required
- TP/SL orders return bad IDs from placement API -- `placeOrder()` returns `[]` and triggers `fetchAndSetOrders()` to get correct IDs

### Positions

- Fetched via `/fapi/v2/account` which returns account data including positions
- Filters for USDT and USDC margin coins (`bitrue.exchange.ts:346`)
- For symbols with open positions, `fetchPositionMetadata()` detects leverage and margin type
- **Closed position detection** (`tick()`, `bitrue.exchange.ts:168-185`): On each tick, compares current positions with previous to detect closures, then refetches orders for those symbols (stops should be removed)
- Store updated via `store.addOrUpdatePositionsFull()` which does a full replacement

### Balance

- Parsed from `/fapi/v2/account` response alongside positions
- Per-coin breakdown: `totalEquity` (total), `accountNormal` (free), `accountLock` (used), `sumOpenRealizedAmount` (upnl)
- USDT balance is primary, USDC is secondary
- Both stored in `balance.currencies` map (`bitrue.exchange.ts:444-466`)

### Ticker

- Empty stubs (all zeros) created on startup for all markets
- Real data filled in by public WS ticker updates
- Orderbook updates also push bid/ask to ticker store (`bitrue.ws-public.ts:234-238`)

## Exchange Metadata

Key metadata values from `exchangeMetadata.ts`:

```
exchangeName: 'bitrue'
prettyName: 'Bitrue'
tvIntraDayMultipliers: ['1', '5', '15', '30', '60']
supportsTestnet: false
requiresCors: true
enforceWsTunnel: true
orderbookTickDelay: 60
tickInterval: 2000
orderPendingUpdateDelay: 2000
stopOrderPendingUpdateDelay: 4000
positionModes: [OneWay]
hasEmptyPositions: false
marginTypeIsGlobal: false
supportedTPSLTriggerTypes: [LastPrice, MarkPrice]
defaultTPTriggerType: LastPrice
defaultSLTriggerType: MarkPrice
supportsChaser: false
supportsPartialStops: true
supportsInfinityStops: false
autoDownSizeReduceOrders: true
latencyDomain: 'fapi.bitrue.com'
```

**Reduce-only downsizing**: Bitrue does not auto-cap reduce-only orders to
the open position server-side, so `autoDownSizeReduceOrders: true` routes
them through the shared `downSizeReduceOrder` web-layer util. A hand-rolled
`Math.min(opts.amount, foundPosition.contracts)` clamp previously lived in
`formatCreateOrder` — it was the codebase's lone exception and was removed
(blind to hedge mode + orders-ahead). See `../CLAUDE.md` → "Reduce-only
order downsizing".

### Leverage (`setLeverage`, `bitrue.exchange.ts:966-1005`)

- Per-symbol leverage setting via `POST /fapi/v2/level_edit`
- Payload: `{ contractName: symbol, leverage: number }`
- Has a `silent` parameter to suppress error logging (used during initial position metadata fetch)
- If leverage exceeds max, error message contains the actual maximum; calling code parses this and retries

### Margin Type (`changeMarginType`, `bitrue.exchange.ts:1007-1011`)

- **NOT implemented** via API
- Throws error: "Cannot change Margin Type on Tealstreet as API does not support it. Change it on bitrue.com"
- Margin type is detected indirectly through `fetchPositionMetadata()` (see below)

### Position Mode

- **One-way only** (`PositionMode.OneWay`)
- No hedged mode support, no API endpoint to switch

## Testnet/Demo

**Not available.** `supportsTestnet: false` in metadata. No testnet URLs are defined.

## Known Quirks / Implementation Notes

1. **BigInt order IDs**: Order IDs exceed `Number.MAX_SAFE_INTEGER` (e.g., `2334151155877155281`) and must be handled as strings throughout. Uses `bigIntAxiosHttpOverWebsocketAdapter` for HTTP and `bigIntAsStringJsonParse` for WS JSON parsing. Order IDs accessed via `update.i?.toString()`.

2. **No REST kline fetch**: OHLCV data is fetched via WebSocket request/response (`event: "req"` / `event_rep: "rep"`), not REST API endpoint, despite `/fapi/v1/klines` being defined in endpoints. If no WS connection exists (non-worker context), falls back to `callMethodInWorker('fetchOHLCV')` (`bitrue.exchange.ts:620-651`).

3. **Leverage detection hack** (`fetchPositionMetadata`, `bitrue.exchange.ts:218-313`): Since no direct query endpoint exists, leverage/margin type is detected by attempting to place an invalid order and parsing the error:
   - Places a dummy CLOSE LIMIT order at price 0 with minimum volume
   - Error `-1165` ("order type inconsistent with user contract configuration 2"): switch to isolated margin, retry
   - Error `-1166` ("leverage value inconsistent"): extract current leverage from error message (last word)
   - No leverage error: set leverage to market max

4. **Missing WS cancel updates**: Bitrue WebSocket does not reliably notify about order cancellations. After `cancelOrders()`, manual store cleanup via `callMethodInWorker('removeOrders')` is required (`bitrue.exchange.ts:1046-1047`).

5. **TP/SL order ID issue**: Stop orders return incorrect IDs from placement API. `placeOrder()` returns `[]` for TP/SL orders and triggers `fetchAndSetOrders()` in the worker to get correct IDs (`bitrue.exchange.ts:915-920`).

6. **Ticker stubs**: On start, empty ticker stubs (all zeros) are created for all markets, with real data filled in by WS (`bitrue.exchange.ts:107-121`).

7. **Closed position detection**: On each tick, compares current positions with previous to detect closures, then refetches orders for those symbols since stops should be auto-removed but WS does not notify (`bitrue.exchange.ts:168-185`).

8. **API response format**: Success responses have `code: '0'` (string, not number). Non-zero codes indicate errors. Listen key endpoint uses `code: 200` (number).

9. **Market order amount**: For non-reduce-only market orders, Bitrue expects quote-denominated `amount` instead of base-denominated `volume`. A buffer of `precision.amount / 3` is added to prevent rounding-down rejections (`bitrue.exchange.ts:871-883`).

10. **Listen key refresh**: Must be refreshed every 30 minutes or the private WebSocket will disconnect. Scheduled via `setTimeout` in both `fetchListenKey()` and `updateListenKey()`.

11. **Reversed ping/pong**: Both public and private WS have reversed ping/pong:
    - **Public**: Server sends `{ ping: <ts> }`, client responds with literal `"pong"` string, health check with 20-second timeout
    - **Private**: Server sends `{ event: "ping" }`, client records timestamp only (no pong response, no health check timeout)

12. **Orderbook format**: Full snapshot on each update (not incremental delta). Bids are in `buys` field, not `bids`. Asks are in `asks` field (`bitrue.types.ts:169-172`).

13. **TP/SL trigger type not in REST**: The REST API does not include `triggerPriceType` in order responses (commented out in code, `bitrue.exchange.ts:569-576`).

14. **Priority symbols**: `listenOHLCV()` adds the symbol to `prioritySymbols` set, which causes `fetchAndEmitOrders()` to fetch that symbol's orders first (`bitrue.exchange.ts:1054-1056`).

15. **WS order multiplier**: In private WS order handler, `remaining` and `filled` quantities are multiplied by `market.info.multiplier` to convert from contract units (`bitrue.ws-private.ts:196-197`).

16. **Enforce WS tunnel**: `enforceWsTunnel: true` in metadata -- requires WebSocket tunnel for browser operation.

## Error Handling

### REST API Errors

- All responses checked for `data.code !== '0'` (string comparison) at call sites
- Error messages extracted from `data.msg` field
- Axios errors: `err?.response?.data?.msg || err?.message` pattern used throughout
- `emitAndLogError()` used to surface errors to user

### Retry Strategy (`bitrue.api.ts:12-17,46-49`)

Custom retry condition (`isErrorSignature`):

- Standard network errors (`isNetworkError` from `axios-retry`)
- OR responses containing `error sign!` in `ret_msg` or `retMsg` field
- 3 retries on either condition

### Specific Error Codes

| Code    | Message Pattern                                                  | Handling                                                   |
| ------- | ---------------------------------------------------------------- | ---------------------------------------------------------- |
| `-1165` | "order type inconsistent with user contract configuration 2"     | Switch to isolated margin, retry (`fetchPositionMetadata`) |
| `-1166` | "leverage value inconsistent with user contract configuration X" | Extract leverage from message, use it                      |
| --      | "Maximum leverage does not exceed X"                             | Extract max leverage, update market, retry with max        |

### WebSocket Error Handling

- Public WS: unknown data types logged to console
- Private WS: unknown event types logged via `console.error('Unknown event', json)`
- Listen key fetch failure throws immediately (prevents WS connection)
- `user_account_update` subscription failure logged via `console.error`

## OHLCV / Kline

**Supported timeframes** (`INTERVAL` mapping, `bitrue.types.ts:29-37`):

| Internal | Bitrue WS |
| -------- | --------- |
| `1m`     | `1min`    |
| `5m`     | `5min`    |
| `15m`    | `15min`   |
| `30m`    | `30min`   |
| `1h`     | `60min`   |
| `1d`     | `1day`    |
| `1w`     | `1week`   |

Only 7 timeframes supported (no 3m, 2h, 3h, 4h, 6h, 12h, 1M).

**Fetch mechanism** (`fetchOHLCV`, `bitrue.exchange.ts:620-651`):

1. If WS connection exists (worker context): send `req` event, wait for `rep` response (5-second timeout)
2. Cache responses for 10 seconds per topic to avoid redundant fetches
3. If WS not connected, waits up to 10 seconds (100ms polls) for readiness
4. If no WS at all (non-worker context): delegates to `callMethodInWorker('fetchOHLCV')`

**Candle timezone**: Raw candle `ds` field is in Singapore timezone (UTC+8). Converted via dayjs `utcOffset(8 * 60, true)`.

## Testing / Validation

No dedicated test files exist for the Bitrue exchange integration.

## Grafana Monitoring

- **Every-call endpoints**: `ENDPOINTS.PLACE_ORDER`
- **Periodic endpoints**: `ENDPOINTS.ORDERS`, `ENDPOINTS.ACCOUNT`
