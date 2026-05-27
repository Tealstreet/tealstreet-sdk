# Crypto.com Exchange Integration

## Exchange Information

- **Pretty Name**: Crypto.com
- **Exchange Name (internal)**: `cryptocom`
- **Type**: Centralized exchange (CEX), perpetual futures only
- **Position Mode**: Hedged only (`PositionMode.Hedged`) -- both long and short positions can coexist
- **Margin Types**: Cross and Isolated
- **Status**: Disabled in production (`enabled: false`), enabled in dev (`enabledInDev: true`)

## API Documentation

- **Exchange API v1**: https://exchange-docs.crypto.com/exchange/v1/rest-ws/index.html
- **Derivatives API**: https://exchange-docs.crypto.com/derivatives/index.html
- **CCXT REST Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/cryptocom.ts
- **CCXT WebSocket Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/cryptocom.ts

## File Structure

| File                      | Purpose                                                                  |
| ------------------------- | ------------------------------------------------------------------------ |
| `cryptocom.signing.ts`    | HMAC-SHA256 signature generation for REST and WebSocket authentication   |
| `cryptocom.api.ts`        | Axios REST client with request/response interceptors, auth injection     |
| `cryptocom.exchange.ts`   | Main exchange class: order management, market data, position handling    |
| `cryptocom.types.ts`      | All type definitions, endpoint constants, URL mappings, WS message types |
| `cryptocom.ws-public.ts`  | Public WebSocket: tickers, orderbook, trades, candlesticks               |
| `cryptocom.ws-private.ts` | Private WebSocket: orders, positions, balances with auth                 |

## Authentication

### Signature Generation (`cryptocom.signing.ts`)

Algorithm: HMAC-SHA256

1. Sort parameter keys alphabetically (`cryptocom.signing.ts:22`)
2. Concatenate key-value pairs without separators: `key1value1key2value2` (`cryptocom.signing.ts:25`)
3. Build signature payload: `method + id + apiKey + paramString + nonce` (`cryptocom.signing.ts:28`)
4. HMAC-SHA256 sign with API secret, output as hex string (`cryptocom.signing.ts:31`)

### REST API Authentication (`cryptocom.api.ts`)

Public and private endpoints use **different HTTP verbs**. The request
interceptor branches on `PUBLIC_ENDPOINTS`:

1. **Public** (`/public/*`) → **GET** with `params` URL-encoded as query
   string, no request body. Crypto.com's v1 public REST started rejecting
   the old POST envelope shape with HTTP 500 / code 50001 "Internal error";
   only GET is accepted. Don't reintroduce the POST envelope for public
   endpoints — it 500s silently and the cryptic 50001 is the only hint.
2. **Private** (`/private/*`) → **POST** with the signed JSON envelope built
   by `generateAuthenticatedRequest()` (`{id, method, api_key, params, sig,
nonce}`).

Authenticated request body format:

```json
{
  "id": <timestamp_ms>,
  "method": "private/create-order",
  "api_key": "<key>",
  "params": { ... },
  "sig": "<hex_signature>",
  "nonce": <timestamp_ms>
}
```

### WebSocket Authentication (`cryptocom.signing.ts:71-91`, `cryptocom.ws-private.ts:52-63`)

1. Generate auth message via `generateWsAuthMessage()` with method `'public/auth'` and empty params
2. Send as first message after WebSocket open
3. Server responds with `{ method: 'public/auth', code: 0 }` on success
4. After auth success, `isAuthenticated = true` and pending subscriptions are sent via `resubscribe()`

**Key detail**: WebSocket auth uses the same signature algorithm as REST but with method `'public/auth'` and empty params.

### HTTP-over-WebSocket Adapter

Only enabled when `options.tealstreetTunnel` is true (`cryptocom.api.ts:28-29`). Unlike many other exchanges, this is NOT unconditionally applied.

## Startup Sequence

`start()` in `cryptocom.exchange.ts:106-153` executes **sequentially**:

1. `fetchMarkets()` -- GET `/public/get-instruments`, filter for `inst_type === 'PERPETUAL_SWAP' && tradable`
2. Store update: `markets`, `loaded.markets = true`
3. Dispose check
4. `fetchTickers()` -- GET `/public/get-tickers`, filter for `PERP` in ticker `i` field
5. Store update: `tickers`, `loaded.tickers = true`
6. Log market count
7. Connect public WebSocket (`publicWebsocket.connectAndSubscribe()`)
8. Connect private WebSocket (`privateWebsocket.connectAndSubscribe()`)
9. **Wait 1 second** for WebSocket connections (`await new Promise(resolve => setTimeout(resolve, 1000))`)
10. Subscribe to market data for each market: `subscribeToTicker`, `subscribeToOrderBook`, `subscribeToTrades`
11. `await this.tick()` -- first tick fetches positions and balance
12. Dispose check
13. `fetchOrders()` -- separate from tick cycle for initial load
14. Store update: `orders`, `loaded.orders = true`

**Notable**: WebSocket subscriptions are sent per-market individually (not batched). The 1-second delay between WS connect and subscription is a hardcoded wait, not event-driven.

**Notable**: Both public and private WebSockets are always connected (no `publicDataOnly` check like some exchanges).

## REST API Endpoints

All requests are POST with JSON body. Defined in `cryptocom.types.ts:19-56`.

### Public Endpoints

| Constant                   | Method Path                            | Purpose                   |
| -------------------------- | -------------------------------------- | ------------------------- |
| `INSTRUMENTS`              | `/public/get-instruments`              | All tradeable instruments |
| `BOOK`                     | `/public/get-book`                     | Order book snapshot       |
| `TICKERS`                  | `/public/get-tickers`                  | All ticker data           |
| `TICKER`                   | `/public/get-ticker`                   | Single ticker             |
| `TRADES`                   | `/public/get-trades`                   | Recent trades             |
| `CANDLESTICK`              | `/public/get-candlestick`              | OHLCV candlestick data    |
| `VALUATIONS`               | `/public/get-valuations`               | Asset valuations          |
| `EXPIRED_SETTLEMENT_PRICE` | `/public/get-expired-settlement-price` | Settlement prices         |
| `INSURANCE`                | `/public/get-insurance`                | Insurance fund data       |

### Private Endpoints

| Constant             | Method Path                        | Purpose                                 |
| -------------------- | ---------------------------------- | --------------------------------------- |
| `CREATE_ORDER`       | `/private/create-order`            | Place new order                         |
| `AMEND_ORDER`        | `/private/amend-order`             | Modify existing order                   |
| `CANCEL_ORDER`       | `/private/cancel-order`            | Cancel specific order                   |
| `CANCEL_ALL_ORDERS`  | `/private/cancel-all-orders`       | Cancel all (optional instrument filter) |
| `CLOSE_POSITION`     | `/private/close-position`          | Close position                          |
| `ORDER_DETAIL`       | `/private/get-order-detail`        | Single order details                    |
| `OPEN_ORDERS`        | `/private/get-open-orders`         | All open orders                         |
| `ORDER_HISTORY`      | `/private/get-order-history`       | Historical orders                       |
| `TRADES_HISTORY`     | `/private/get-trades`              | Trade history                           |
| `POSITIONS`          | `/private/get-positions`           | Open positions                          |
| `CHANGE_LEVERAGE`    | `/private/change-account-leverage` | Set instrument leverage                 |
| `CHANGE_MARGIN_TYPE` | `/private/change-margin-type`      | Switch cross/isolated                   |
| `BALANCE`            | `/private/user-balance`            | Account balances                        |
| `ACCOUNT_SUMMARY`    | `/private/get-account-summary`     | Detailed account info                   |

**Response unwrapping**: The response interceptor in `cryptocom.api.ts:86-89` unwraps `data.result` so callers receive the inner result object directly, not the full API envelope.

## WebSocket Implementation

### Public WebSocket (`cryptocom.ws-public.ts`)

**URL**: `wss://stream.crypto.com/exchange/v1/market` (production) / `wss://uat-stream.3ona.co/exchange/v1/market` (testnet)

**Connection**: Uses `BaseWebSocket.createWebsocket()`. Reconnection handled by `BaseWebSocket.onClose()`.

**On Open** (`cryptocom.ws-public.ts:37-41`):

- Calls `resubscribe()` to re-send all stored subscriptions from the `subscriptions` Set

**Subscription Management**:

- Each subscription tracked in a `Set<string>` (deduplication)
- Subscriptions use incrementing `messageId` counter
- Format: `{ id, method: 'subscribe', params: { channels: [...] }, nonce }`

**Channel Naming**:

- Ticker: `ticker.<instrument_name>` (e.g., `ticker.BTCUSD-PERP`)
- Order book: `book.<instrument_name>.<depth>` (e.g., `book.BTCUSD-PERP.50`)
- Trades: `trade.<instrument_name>`
- Candlestick: `candlestick.<interval>.<instrument_name>`

**Message Routing** (`cryptocom.ws-public.ts:127-166`):

- Messages with `code` field: response messages (subscription confirmations, errors). Non-zero `code` logged as error.
- Messages with `result.channel`: data updates, routed by channel prefix:
  - `ticker.*` -> `handleTickerUpdate()` (throttled to 1000ms)
  - `trade.*` -> `handleTradeUpdate()` (no-op currently)
  - `book.*` -> `handleBookUpdate()` (throttled to 100ms)
  - `candlestick.*` -> `handleCandlestickUpdate()`

**Ticker Throttling** (`cryptocom.ws-public.ts:168-198`):

- Uses lodash `throttle` at 1000ms with `{ leading: true, trailing: true }`
- Updates bid (`b`), ask (`k`), last (`c`), percentage (`c`), volume (`v`), quoteVolume (`vv`)
- Uses O(1) `marketsById` and `tickersBySymbol` hash lookups

**Orderbook** (`cryptocom.ws-public.ts:200-432`):

- Throttled to 100ms
- Updates best bid/ask in ticker on each book update
- Uses callback-based `messageHandlers` pattern (not direct store updates)
- `listenOrderBook()` uses snapshot + incremental update pattern:
  1. First WS update triggers a REST snapshot fetch (`/public/get-book` with `depth: 50`)
  2. Incoming WS updates are queued in `orderBookInnerState[innerId].updates` while snapshot loads
  3. After snapshot: queued updates are applied, then incremental mode begins
  4. `processOrderBookUpdate()`: quantity `0` removes a level; non-zero updates or adds
  5. After each update: `sortOrderBook()` and `calcOrderBookTotal()` are called
  6. Each listener gets a unique `innerId` via `nanoid()` for independent state tracking

**OHLCV via WebSocket** (`cryptocom.ws-public.ts:243-287`):

- Supported (unlike Kraken). Uses `candlestick.<interval>.<symbol>` channel
- Waits for connection if not yet connected (100ms retry loop)
- Returns cleanup function that unsubscribes and cleans up message handlers

**Unsubscribe** (`cryptocom.ws-public.ts:434-449`):

- Sends `{ method: 'unsubscribe', params: { channels: [...] } }`
- Removes channels from the `subscriptions` Set

### Private WebSocket (`cryptocom.ws-private.ts`)

**URL**: `wss://stream.crypto.com/exchange/v1/user` (production) / `wss://uat-stream.3ona.co/exchange/v1/user` (testnet)

**On Open** (`cryptocom.ws-private.ts:44-49`):

1. Reset `isAuthenticated = false`
2. Send auth message via `generateWsAuthMessage()`

**Authentication Response** (`cryptocom.ws-private.ts:155-158`):

- `method === 'public/auth'` and `code === 0` -> set `isAuthenticated = true`, call `resubscribe()`
- Non-zero `code` -> emit error

**Channel Subscriptions** (available methods, `cryptocom.ws-private.ts:80-141`):

- `user.order` -- order status updates
- `user.positions` -- position updates
- `user.balance` -- balance changes

**Note**: These subscriptions are defined as public methods but are NOT automatically called during startup. They appear to be intended for manual subscription by consumers.

**Message Routing** (`cryptocom.ws-private.ts:143-187`):

- `code` field present -> response handling (auth result, errors)
- `result.channel` present -> data routing:
  - `user.order*` -> `handleOrderUpdate()`: maps to internal `Order`, calls `addOrUpdateOrder()`
  - `user.positions` -> `handlePositionUpdate()`: maps to internal `Position`, handles zero-quantity closure
  - `user.balance` -> `handleBalanceUpdate()`: currently a no-op

**Order Update Handling** (`cryptocom.ws-private.ts:189-216`):

- Processes single order from `data[0]`
- Maps `order_type`, `side`, `order_status`, `time_in_force` to internal types
- Uses `store.addOrUpdateOrder()` for individual updates (not full replacement)

**Position Update Handling** (`cryptocom.ws-private.ts:218-261`):

- Processes single position from `data[0]`
- If `quantity === 0`: removes position from store by filtering it out
- If non-zero: creates/updates position via `store.addOrUpdatePosition()`
- Entry price calculated as `cost / contracts`
- Position ID created with `createPositionId(symbol, side, positionMode)`

**Reconnection**: On reconnect, `onOpen` resets `isAuthenticated` and re-authenticates. After successful auth, `resubscribe()` re-sends all stored subscriptions.

## Order Types and Mappings

### Internal to Crypto.com (`mapOrderType`, `cryptocom.exchange.ts:666-681`)

| Internal               | Crypto.com      |
| ---------------------- | --------------- |
| `OrderType.Market`     | `'MARKET'`      |
| `OrderType.Limit`      | `'LIMIT'`       |
| `OrderType.StopLoss`   | `'STOP_LOSS'`   |
| `OrderType.TakeProfit` | `'TAKE_PROFIT'` |
| _(default)_            | `'LIMIT'`       |

### Crypto.com to Internal (`mapCryptocomOrderType`, `cryptocom.exchange.ts:683-698`)

| Crypto.com                             | Internal               |
| -------------------------------------- | ---------------------- |
| `'MARKET'`                             | `OrderType.Market`     |
| `'LIMIT'`                              | `OrderType.Limit`      |
| `'STOP_LOSS'`, `'STOP_LIMIT'`          | `OrderType.StopLoss`   |
| `'TAKE_PROFIT'`, `'TAKE_PROFIT_LIMIT'` | `OrderType.TakeProfit` |
| _(default)_                            | `OrderType.Limit`      |

**Note**: The distinction between trigger-only (e.g., `STOP_LOSS`) and trigger+limit (e.g., `STOP_LIMIT`) variants is lost in the mapping.

### Time-in-Force Mapping

| Internal            | Crypto.com              |
| ------------------- | ----------------------- |
| `ImmediateOrCancel` | `'IMMEDIATE_OR_CANCEL'` |
| `FillOrKill`        | `'FILL_OR_KILL'`        |
| `PostOnly`          | `'POST_ONLY'`           |
| _(default)_         | `'GOOD_TILL_CANCEL'`    |

### Order Status Mapping

| Crypto.com                              | Internal               |
| --------------------------------------- | ---------------------- |
| `'ACTIVE'`, `'PENDING'`                 | `OrderStatus.Open`     |
| `'FILLED'`                              | `OrderStatus.Closed`   |
| `'CANCELED'`, `'EXPIRED'`, `'REJECTED'` | `OrderStatus.Canceled` |
| _(default)_                             | `OrderStatus.Open`     |

## Order Lifecycle

### Placing Orders (`cryptocom.exchange.ts:420-457`)

1. Look up market by symbol
2. Build `CryptocomOrderRequest` with `instrument_name`, `side` (BUY/SELL), `type`
3. **Market orders**: use `notional` (= `amount * price`) instead of `quantity`. This requires a price estimate.
4. **Limit/other orders**: use `quantity` and `price`
5. Add `time_in_force` if specified
6. POST to `/private/create-order`
7. Return `[data.order_id]`

**No bracket/TP/SL attachment**: TP/SL orders are not derived from the main order. No `deriveAlgoOrdersFromNormalOrdersOpts` call.

### Updating Orders (`cryptocom.exchange.ts:492-517`)

- Uses `/private/amend-order` endpoint (true amend, not cancel-replace)
- Sends `order_id`, `instrument_name`, and updated `quantity`/`price`
- Returns new `order_id` from response

### Canceling Orders

- **Single**: POST `/private/cancel-order` with `instrument_name` and `order_id` (`cryptocom.exchange.ts:459-469`)
- **Multiple**: Sequential cancellation via `forEachSeries` from `p-iteration`, continues on individual failures (`cryptocom.exchange.ts:471-479`)
- **All for symbol**: POST `/private/cancel-all-orders` with `instrument_name` (`cryptocom.exchange.ts:481-490`)
- **All globally**: POST `/private/cancel-all-orders` with empty params (`cryptocom.exchange.ts:535-542`)

### Batch Order Placement (`cryptocom.exchange.ts:566-579`)

Not a true batch API call -- orders are placed sequentially in a `for` loop. Individual failures are caught and logged but do not stop remaining orders.

## Symbol Format

- Perpetual futures: `{BASE}{QUOTE}-PERP` (e.g., `BTCUSD-PERP`, `ETHUSD-PERP`)
- Filtering: Only instruments with `inst_type === 'PERPETUAL_SWAP'` and `tradable === true` are loaded (`fetchMarkets`)
- `market.id` and `market.symbol` are both the raw `symbol` (no remapping)
- All markets marked as: `type: 'swap'`, `swap: true`, `linear: true`, `contract: true`, `contractSize` from `contract_size` (default 1), `inverse: false`, `spot: false`

## Data Precision & Conversion

### Price Precision

- Prefer `price_tick_size` (parsed float) when present, else `Math.pow(10, -quote_decimals)`
- No explicit rounding in `placeOrder()` -- the raw `amount` and `price` from opts are sent directly

### Amount Precision

- Prefer `qty_tick_size` (parsed float) when present, else `Math.pow(10, -quantity_decimals)`
- `precision.base` = `quantity_decimals` (integer), `precision.quote` = `quote_decimals` (integer)

### Amount Limits

- `min` from `qty_tick_size` (the v1 GET schema no longer carries `min_quantity`/`max_quantity`); `max` left at 0 (unbounded)
- Leverage range: `min: 1`, `max` from `max_leverage`

### Entry Price Calculation

- Calculated as `cost / Math.abs(quantity)` from position data (`cryptocom.exchange.ts:357`)
- **Not** directly from an API field

## Rate Limiting

- REST API: Rate limited at **50 RPS** via `axios-rate-limit` (`cryptocom.exchange.ts:64`), conservative from API limit of 100 RPS
- Network error retry: 3 retries via `axios-retry` on network errors
- Request timeout: `options.extra.recvWindow` or **30 seconds** default (`cryptocom.api.ts:15`)
- WebSocket ticker throttle: 1000ms
- WebSocket orderbook throttle: 100ms

## State Reconciliation

### Orders

- **Initial load**: `fetchOrders()` called at end of `start()`, separate from tick cycle
- **Tick cycle**: Re-fetched every 4th tick (~40 seconds)
- **WebSocket**: `handleOrderUpdate()` processes individual order updates via `addOrUpdateOrder()`
- **No throttled REST sync**: Unlike Kraken, there is no throttled fetch after WS events

### Positions

- **Tick cycle**: `fetchPositions()` runs every tick (~10 seconds)
- **WebSocket**: `handlePositionUpdate()` processes individual updates
  - Zero quantity -> removes position from store
  - Non-zero -> creates/updates position
- **Fake positions**: Created for all active perpetual markets without real positions (both long and short), combined with actual positions and deduplicated via `uniqBy(id)` (`cryptocom.exchange.ts:413`)

### Balances

- **Tick cycle**: `fetchBalance()` runs every tick (~10 seconds)
- **WebSocket**: `handleBalanceUpdate()` is a no-op (balance refresh not implemented via WS)
- **uPnL**: Calculated from positions via `sumBy(positions, 'unrealizedPnl')` and injected into balance (`cryptocom.exchange.ts:166`)
- **Balance detection**: Searches for first currency matching `'USD'`, `'USDT'`, or `'USDC'` in that priority order (`cryptocom.exchange.ts:598-599`)

### Tick Cycle (`cryptocom.exchange.ts:155-184`)

- Runs every **10 seconds** via `setTimeout` (not `loop()`)
- **Every tick**: fetch positions, fetch balance, calculate uPnL
- **Every 4th tick** (~40 seconds): also fetch orders
- Uses `setTimeout(() => this.tick(count + 1), 10_000)` for scheduling

## Exchange Metadata

```typescript
{
  exchangeName: 'cryptocom',
  prettyName: 'Crypto.com',
  enabled: false,
  enabledInDev: true,
  enableInPreview: false,
  supportsTestnet: true,
  defaultQuote: 'USD',
  positionModes: [PositionMode.Hedged],
  marginTypes: [MarginType.Cross, MarginType.Isolated],
  supportsMaxCrossLeverage: false,
  supportsPartialStops: true,
  supportsInfinityStops: false,
  supportsTrailingStopLoss: false,
  supportsChaser: false,
  supportedTPSLTriggerTypes: [OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice],
  defaultTPTriggerType: OrderStopTriggerType.MarkPrice,
  defaultSLTriggerType: OrderStopTriggerType.MarkPrice,
  tickInterval: 2000,
  orderbookTickDelay: 100,
  orderPendingUpdateDelay: 1000,
  stopOrderPendingUpdateDelay: 1000,
  candleApiCountLimit: 500,
  tvMaxGetBarFetches: 10,
  tvIntraDayMultipliers: ['1', '5', '15', '30', '60', '240'],
  leverageMagnets: [1, 2, 5, 10, 20, 50, 100, 125, 150],
  hidePositionModeSelector: true,  // Always hedged, no toggle
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  leverageIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: false,
  hasIndependentHedgedLeverageInCrossMargin: false,
  hasEmptyPositions: false,
  supportsTunnel: false,
  supportsWsRoute: false,
  supportsFeeRates: false,
  supportsReadOnlyKey: false,
  supportsClosedPnLEndpoint: false,
  features: { kline: true, orderbook: true, orders: true, positions: true, balance: true },
}
```

## Testnet / Demo

### URLs

| Component | Production                                   | Testnet (UAT)                                 |
| --------- | -------------------------------------------- | --------------------------------------------- |
| REST API  | `https://api.crypto.com/exchange/v1`         | `https://uat-api.3ona.co/exchange/v1`         |
| WS Market | `wss://stream.crypto.com/exchange/v1/market` | `wss://uat-stream.3ona.co/exchange/v1/market` |
| WS User   | `wss://stream.crypto.com/exchange/v1/user`   | `wss://uat-stream.3ona.co/exchange/v1/user`   |

- **Testnet enabled**: `supportsTestnet: true` in metadata
- **Testnet domain**: `3ona.co` (Crypto.com's UAT domain)
- **Selection**: Controlled by `options.testnet` boolean
- **Known differences**: None documented in code; same API structure

## Known Quirks / Implementation Notes

1. **Market orders use `notional`, not `quantity`**: Market orders require `notional = amount * price`, meaning a price estimate is needed to determine order size in dollar terms (`cryptocom.exchange.ts:434-435`). This is different from most exchanges that accept a `quantity` for market orders.

2. **Position mode is always Hedged**: `changePositionMode()` is a no-op that logs a warning. The `hidePositionModeSelector: true` metadata hides the toggle in UI (`cryptocom.exchange.ts:544-547`).

3. **Leverage always defaults to 1**: All positions are initialized with `leverage: 1` because the API response does not include current leverage (`cryptocom.exchange.ts:360`, `cryptocom.exchange.ts:396`). Liquidation price is also hardcoded to `0` (`cryptocom.exchange.ts:368`).

4. **Balance WebSocket is a no-op**: `handleBalanceUpdate()` in the private WebSocket does nothing (`cryptocom.ws-private.ts:263-266`). Balances are only refreshed via REST polling.

   **REST balance response is `result.data[0]`, not `result.position_balances`.** The
   v1 `private/user-balance` envelope is
   `{ result: { data: [{ ...summary, position_balances: [...] }] } }`. After the
   response interceptor unwraps `result`, callers see `{ data: [summary] }` —
   one level deeper than the old shape. The summary carries
   `total_margin_balance` / `total_available_balance` / `total_initial_margin`
   / `total_session_unrealized_pnl` as **strings**; per-asset rows live in
   `position_balances[]` keyed by `instrument_name` (not `currency`), with
   `quantity` / `max_withdrawal_balance` / `market_value` etc. also as
   strings. The previous code read `data.position_balances` at the top level
   and silently returned a zeroed balance — see `fetchBalance` in
   `cryptocom.exchange.ts` for the parser that digs into `data[0]`.

5. **Private WS subscriptions auto-started in `start()`**: `start()` calls
   `subscribeToOrderUpdates()`, `subscribeToPositionUpdates()`, and
   `subscribeToBalanceUpdates()` right after `privateWebsocket.connectAndSubscribe()`.
   The helpers enqueue the channel into the `subscriptions` Set immediately;
   the `public/auth` ack handler calls `resubscribe()` which flushes them.
   Don't move these calls below the 1s `setTimeout` — if you do, brief
   auth-ack races leave the live tick on a 10s REST cadence (orders) /
   10s tick (positions) until the next reconnect.

6. **No price rounding on order placement**: Unlike many exchanges, `placeOrder()` does not call `adjust()` or any precision rounding on `amount` or `price` before sending to the API.

7. **Sequential batch cancellation**: `cancelOrders()` uses `forEachSeries` (sequential iteration) from `p-iteration`, not parallel. Each order is canceled one at a time (`cryptocom.exchange.ts:471-479`).

8. **Ticker data missing mark/index/funding**: The ticker response
   (`i/h/l/a/v/vv/c/b/k/oi/t`) does not carry `mark`, `index`,
   `fundingRate`, or `fundingTime`. These are hardcoded to `0` in
   `fetchTickers` / `handleTickerUpdate`. v1 surfaces them only via
   per-symbol `public/get-valuations?valuation_type=<mark_price|index_price|funding_rate|estimated_funding_rate|funding_hist>`,
   which is one call per (symbol, type) — too expensive to fan out
   across ~250 markets on every tick. **`openInterest` is now
   populated** from the ticker `oi` field — the older "all hardcoded
   to 0" comment was stale.

8b. **`private/get-account-summary` is broken on v1.** Returns 400 /
`50001 ERR_INTERNAL` regardless of params (probed with empty,
`currency: 'USD'`, etc.). `getAccount()` no longer calls it and
instead returns a stable `userId` derived from the API key prefix
(`cdc-<first8chars>`). If you need the real `account_id`, it appears
on each row from `private/get-positions` (empty for empty accounts).

9. **Ticker `c` field used as both `last` and `percentage`** (confirmed bug): In the WS ticker handler at `cryptocom.ws-public.ts:185-186`, `data.c` is assigned to **both** `last` and `percentage`. These cannot both be correct. The candlestick handler in the same file uses `data.c` as `close` (line 256), which strongly suggests `c` is the latest trade price — so `last: data.c` is the correct assignment and `percentage: data.c` is the bug. The result is that Crypto.com tickers display the absolute price in the 24h % column (e.g. `"65123.4%"` for a BTC ticker). The real 24h change field needs to be looked up in the Crypto.com Exchange v1 API docs and substituted in — do NOT fix this from memory.

10. **Orderbook snapshot via REST**: On first book subscription, the implementation fetches a full REST snapshot (`/public/get-book` with `depth: 50`) and queues incoming WS updates until the snapshot loads (`cryptocom.ws-public.ts:303-338`).

11. **Crypto.com uses `instrument_name` everywhere**: Unlike many exchanges that have separate `id`/`symbol` fields, Crypto.com uses `instrument_name` as both `market.id` and `market.symbol`.

12. **Response interceptor unwraps `result`**: The API response interceptor extracts `data.result` and replaces `response.data` with it (`cryptocom.api.ts:87-89`). Callers receive the unwrapped result, not the full `{ id, method, code, result }` envelope.

13. **Grafana monitoring endpoints**: The exchange defines `GRAFANA_ENDPOINTS_EVERY` (order operations) and `GRAFANA_ENDPOINTS_PERIODIC` (data fetches) for monitoring integration (`cryptocom.types.ts:71-84`).

14. **Leverage sent as string**: `setLeverage()` converts leverage to string before sending (`cryptocom.exchange.ts:527`).

15. **HTTP-over-WebSocket adapter conditional**: Only applied when `tealstreetTunnel` is true (`cryptocom.api.ts:28-29`), unlike most exchanges where it is always applied.

16. **Dispose pattern differs**: Both public and private WS have their own `dispose()` methods that clear subscriptions and close the WebSocket. The exchange `dispose()` calls both (`cryptocom.exchange.ts:70-74`).

## Error Handling

### REST API Response Interceptor (`cryptocom.api.ts:75-113`)

- **Success check**: `code === 0` in the response envelope
- **Error handling**: Non-zero `code` -> logs via `emitAndLogError`, throws `Error` with pattern: `"Crypto.com API Error {code}: {message}"`
- **HTTP errors**: Extracts `code` and `message` from `error.response.data`, logs and rethrows
- **No specific error code handling**: All non-zero codes are treated identically (no special cases for rate limits, auth failures, etc.)

### Network Retry

- 3 retries on network errors via `axios-retry` (`cryptocom.api.ts:25`)
- 30-second timeout (or `recvWindow` from options)

### Exchange Method Error Handling

Each exchange method wraps its API call in try/catch:

- `fetchMarkets`, `fetchTickers`, `fetchOHLCV`, `fetchOrderBook`: log at `LogSeverity.Error`, rethrow
- `fetchOrders`, `fetchPositions`: log at `LogSeverity.Error`, return empty array `[]`
- `fetchBalance`: log at `LogSeverity.Error`, rethrow
- `placeOrder`, `cancelOrder`, `updateOrder`, `setLeverage`, `changeMarginType`: log and rethrow

### WebSocket Error Handling

- **Public WS** (`cryptocom.ws-public.ts:139-142`): Non-zero `code` in response messages logged via `emitAndLogError`
- **Private WS** (`cryptocom.ws-private.ts:162-166`): Same pattern for non-zero response codes
- **Auth failure**: Private WS does not explicitly retry auth on failure; relies on reconnection cycle
- **Parse errors**: Caught and logged via `emitAndLogError` in both public and private WS

### Account Validation (`cryptocom.exchange.ts:96-104`)

Simple validation: POST to `/private/user-balance` with empty params. Returns empty string on success, error message on failure.

## Testing / Validation

- [ ] Authentication: REST API signing, WebSocket auth via `public/auth`
- [ ] Market loading: only PERP instruments loaded
- [ ] Ticker data: verify `mark`, `index`, `fundingRate` are populated (currently 0)
- [ ] Order placement: limit orders with quantity, market orders with notional
- [ ] Order amendment via `/private/amend-order`
- [ ] Order cancellation: single, batch (sequential), all for symbol, all global
- [ ] Position tracking: hedged mode, both long and short positions
- [ ] Position closure via WebSocket (quantity = 0 handling)
- [ ] Balance detection: USD/USDT/USDC priority
- [ ] Leverage setting (string conversion)
- [ ] Margin type switching (CROSS/ISOLATED)
- [ ] Orderbook: REST snapshot + WS incremental updates, queue during snapshot load
- [ ] OHLCV via WebSocket candlestick channel
- [ ] Private WS order/position subscriptions (currently not auto-started)
- [ ] Fake positions for all active markets
- [ ] Testnet vs production URL switching
- [ ] WebSocket reconnection and re-authentication
