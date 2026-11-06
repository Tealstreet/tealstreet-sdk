# Crypto.com Exchange Integration

## Exchange Information

- **Pretty Name**: Crypto.com
- **Exchange Name (internal)**: `cryptocom`
- **Type**: Centralized exchange (CEX), perpetual futures only
- **Position Mode**: Hedged only (`PositionMode.Hedged`) -- both long and short positions can coexist
- **Margin Types**: Cross and Isolated upstream; the selector is hidden until
  per-order `ISOLATED_MARGIN` / `isolation_id` creation is implemented.
- **Status**: Disabled in production (`enabled: false`), enabled in dev (`enabledInDev: true`), enabled on preview (`enableInPreview: true`)

## US Perpetual Futures Watchpoint

Crypto.com publicly advertises a US perpetual futures product at
<https://crypto.com/us/perpetual-futures>, with Foris DAX Inc. introducing
customers to Foris DAX FCM, LLC for CFTC-regulated perpetual futures on CDNA.
The page mentions up to 5x leverage, isolated risk per trade, TradingView, and
API access.

This reference does **not** support that US rail today. The current adapter is
for the existing Crypto.com Exchange v1 API under `/exchange/v1` and should not
be assumed to cover US FCM accounts, onboarding state, credentials, endpoints,
instrument ids, margin semantics, funding schedule, or order behavior. Treat US
perpetuals as a considered future support path only, likely requiring a fresh
endpoint/account audit and possibly a separate metadata variant if the rail is
not wire-compatible with the existing Exchange v1 integration.

## API Documentation

- **Exchange API v1**: <https://exchange-developer.crypto.com/exchange/v1/>
- **CCXT REST Reference**: <https://github.com/ccxt/ccxt/blob/master/ts/src/cryptocom.ts>
- **CCXT WebSocket Reference**: <https://github.com/ccxt/ccxt/blob/master/ts/src/pro/cryptocom.ts>

## File Structure

| File                      | Purpose                                                                  |
| ------------------------- | ------------------------------------------------------------------------ |
| `cryptocom.signing.ts`    | HMAC-SHA256 signature generation for REST and WebSocket authentication   |
| `cryptocom.api.ts`        | Axios REST client with request/response interceptors, auth injection     |
| `cryptocom.exchange.ts`   | Main exchange class: order management, market data, position handling    |
| `cryptocom.types.ts`      | All type definitions, endpoint constants, URL mappings, WS message types |
| `cryptocom.ws-public.ts`  | Public WebSocket: tickers, valuations, orderbook, trades, candlesticks   |
| `cryptocom.ws-private.ts` | Private WebSocket: orders, positions, balances with auth                 |

## Authentication

### Signature Generation (`cryptocom.signing.ts`)

Algorithm: HMAC-SHA256

1. Recursively serialize parameters to a maximum depth of three: object keys
   are sorted, array elements are concatenated without delimiters, and null is
   serialized as `null`.
2. Build signature payload: `method + id + apiKey + paramString + nonce`.
3. HMAC-SHA256 sign with API secret, output as hex string.

Request ids and nonces use `virtualClock.getCurrentTimeMs()` so exchange time
synchronization also protects Crypto.com's ±60-second nonce window.

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

Axios re-enters the request interceptors on a retry with the envelope it already
sent. The interceptor unwraps it and re-signs its `params` with a fresh nonce —
never nests it, and never resends the stale `sig`.

A `40101` ("Not authenticated, or key/signature incorrect") is re-signed and resent
once, writes included (`isCryptocomSignatureRejection`), so a genuinely bad key costs
one extra request before the error surfaces. The result unwrap is idempotent: an
axios-retry resend runs the response interceptor twice. Private write
endpoints (`create/amend/cancel/close/change-leverage`) are not retried on
network/CORS errors: the exchange may have accepted the mutation even if the
browser could not read the response.

### WebSocket Authentication

1. Generate auth message via `generateWsAuthMessage()` with method `'public/auth'` and empty params
2. Send as first message after WebSocket open
3. Server responds with `{ method: 'public/auth', code: 0 }` on success
4. After auth success, `isAuthenticated = true` and pending subscriptions are sent via `resubscribe()`

**Key detail**: WebSocket auth uses the same signature algorithm as REST but with method `'public/auth'` and empty params.

### HTTP-over-WebSocket Adapter

Only enabled when `options.tealstreetTunnel` is true. Unlike many other
exchanges, this is not unconditionally applied.

### REST Proxy Routing

`CryptocomMetadata.requiresCors` forces browser REST calls through Tealstreet's
exchange proxy. `supportsTunnel` and `supportsWsRoute` are false today, so
Crypto.com does not use the HTTP-over-WebSocket tunnel or TradeTunnel WS route
for normal browser validation.

The metadata declares both private proxy regions explicitly:

- `JA` → `privateProxyIpsTokyo`
- `SG` → `privateProxyIpsSingapore`

`JA` is intentionally listed first so dev-only account setup defaults to the
Tokyo REST pool while infra wires and health-checks Crypto.com on the Asia pool.
Before production enablement, both proxy pools should forward
`https://api.crypto.com/exchange/v1` and
`https://uat-api.3ona.co/exchange/v1` with correct SNI/Host handling.

For local UAT while proxy routes are being wired, the web app has a
non-production developer override:

```js
localStorage.setItem(
  'dev-overrides',
  JSON.stringify({ ...JSON.parse(localStorage.getItem('dev-overrides') || '{}'), cryptocomDirectRestInDev: true }),
);
location.reload();
```

This forces Crypto.com REST direct from the browser so georestriction/CORS
failures can be observed directly. Do not use it as production behavior.

## Startup Sequence

`start()` executes **sequentially**:

1. `fetchMarkets()` -- GET `/public/get-instruments`, filter for `inst_type === 'PERPETUAL_SWAP' && tradable`
2. Store update: `markets`, `loaded.markets = true`
3. Dispose check
4. `fetchTickers()` -- GET `/public/get-tickers`, filter for `PERP` in ticker `i` field
5. Store update: `tickers`, `loaded.tickers = true`
6. Log market count
7. Connect the public WebSocket unless external public data is active.
8. Return with private stores marked loaded when `publicDataOnly` is active.
9. Connect the private WebSocket and enqueue `user.order`,
   `user.advance.order`, `user.positions`, and `user.balance` before auth.
10. Wait one second for the connection-rate window.
11. Batch-subscribe ticker channels for all markets, then mark, index, and
    current-funding channels while staying under the local 350-subscription
    public-session cap; order books, trades, and candles remain lazy per
    consumer.
12. Run the first REST position/balance/order reconciliation.

External public data mode does not open the market WebSocket. Public-only mode
does not open the user WebSocket.

## REST API Endpoints

Public endpoints are GET requests with query parameters. Private endpoints are
signed POST requests with JSON bodies. Endpoint constants live in
`cryptocom.types.ts`.

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

| Constant                 | Method Path                           | Purpose                                 |
| ------------------------ | ------------------------------------- | --------------------------------------- |
| `CREATE_ORDER`           | `/private/create-order`               | Place new order                         |
| `ADVANCED_CREATE_ORDER`  | `/private/advanced/create-order`      | Place standalone TP/SL trigger order    |
| `ADVANCED_AMEND_ORDER`   | `/private/advanced/amend-order`       | Amend standalone TP/SL trigger order    |
| `ADVANCED_CANCEL_ORDER`  | `/private/advanced/cancel-order`      | Cancel standalone TP/SL trigger order   |
| `ADVANCED_OPEN_ORDERS`   | `/private/advanced/get-open-orders`   | Fetch open TP/SL trigger orders         |
| `AMEND_ORDER`            | `/private/amend-order`                | Modify existing order                   |
| `CANCEL_ORDER`           | `/private/cancel-order`               | Cancel specific order                   |
| `CANCEL_ALL_ORDERS`      | `/private/cancel-all-orders`          | Cancel all (optional instrument filter) |
| `CLOSE_POSITION`         | `/private/close-position`             | Close position                          |
| `ORDER_DETAIL`           | `/private/get-order-detail`           | Single order details                    |
| `OPEN_ORDERS`            | `/private/get-open-orders`            | All open orders                         |
| `ORDER_HISTORY`          | `/private/get-order-history`          | Historical orders                       |
| `TRADES_HISTORY`         | `/private/get-trades`                 | Trade history                           |
| `POSITIONS`              | `/private/get-positions`              | Open positions                          |
| `CHANGE_LEVERAGE`        | `/private/change-account-leverage`    | Set instrument leverage                 |
| `BALANCE`                | `/private/user-balance`               | Account balances                        |
| `ACCOUNTS`               | `/private/get-accounts`               | Current account identity                |

**Response unwrapping**: The response interceptor unwraps `data.result` so
callers receive the inner result object directly, not the full API envelope.

## WebSocket Implementation

### Public WebSocket (`cryptocom.ws-public.ts`)

**URL**: `wss://stream.crypto.com/exchange/v1/market` (production) / `wss://uat-stream.3ona.co/exchange/v1/market` (testnet)

**Connection**: Uses `BaseWebSocket.createWebsocket()`. Reconnection handled by `BaseWebSocket.onClose()`.

Both market and user sockets immediately answer `public/heartbeat` with
`public/respond-heartbeat` using the same request id. Crypto.com closes sockets
that do not respond within five seconds. All subscription nonces use the
synchronized virtual clock.

Crypto.com heartbeat traffic is server-initiated. The client does not send an
active ping frame; instead both sockets treat any inbound message as liveness
and use a relaxed quiet-session watchdog (30s idle ping marker, 90s reconnect)
so quiet or geo-degraded direct routes do not churn every base 20 seconds.

**On Open**:

- Waits one second for Crypto.com's pro-rated connection rate limit to reset,
  then calls `resubscribe()` for all stored subscriptions.

**Subscription Management**:

- Each subscription tracked in a `Set<string>` (deduplication)
- Subscriptions use incrementing `messageId` counter
- Format: `{ id, method: 'subscribe', params: { channels: [...] }, nonce }`
- Startup and reconnect frames use a conservative local limit of 100 channels.
  The socket keeps a local 350-subscription cap to stay below Crypto.com's
  400-subscription market-data session limit while leaving room for active
  chart orderbook/candle streams.

**Channel Naming**:

- Ticker: `ticker.<instrument_name>` (e.g., `ticker.BTCUSD-PERP`)
- Mark: `mark.<instrument_name>`
- Index: `index.<underlying_symbol>` (e.g., `index.BTCUSD-INDEX`, not the
  perpetual's `BTCUSD-PERP` id)
- Current settling funding rate: `funding.<instrument_name>`
- Order book: `book.<instrument_name>.<depth>` (e.g., `book.BTCUSD-PERP.50`)
- Trades: `trade.<instrument_name>`
- Candlestick: `candlestick.<interval>.<instrument_name>`

**Message Routing**:

- Messages with `code` field: response messages (subscription confirmations, errors). Non-zero `code` logged as error.
- Messages with `result.channel`: data updates, routed by channel prefix:
  - `ticker` -> `handleTickerUpdate()` (throttled to 1000ms)
  - `trade` -> `handleTradeUpdate()` (no-op currently)
  - `book` / `book.update` -> `handleBookUpdate()` (throttled to 100ms)
  - `candlestick` -> `handleCandlestickUpdate()`
  - `mark` / `index` / `funding` -> finite guarded ticker updates

Data frames record channel-level `messageHandlersStats` (`ticker`, `mark`,
`funding`, `user.order`, etc.) for the connection status popover. Do not remove
these stats updates even though Crypto.com mostly routes through store updates
rather than per-topic callbacks.

**Those keys are knowingly non-compliant with the stats-key invariant** in
`../CLAUDE.md` → "`messageHandlerStats` checks SUBSCRIPTIONS, not the
connection": the recorded key is the bare `result.channel` while handlers live
at `book.{instrument_name}` / `update.subscription`, so `book` and
`candlestick` can never be reconciled and a torn-down orderbook or candle
subscription stays on the popover until it ages into a warning. The per-symbol
fix is blocked, not merely unwritten — read `apps/web/CLAUDE.md` →
"Per-symbol stats keys can produce an INFINITE staleness threshold" first.

Crypto.com's `funding` value is the decimal rate settling at the end of the
current hour, so it maps to `Ticker.fundingRate` without scaling. The distinct
`estimatedfunding` channel is a next-interval estimate; safe-cex has no
`nextFundingRate` field, so startup does not subscribe to it and forwarded
frames never overwrite the current rate. The valuation timestamp is an update
time, not the next settlement time, so `fundingTime` remains unchanged.

**Ticker Throttling**:

- Uses lodash `throttle` at 1000ms with `{ leading: true, trailing: true }`
- Updates bid (`b`), ask (`k`), last (`a`), percentage (`c * 100`), volume
  (`v`), quote volume (`vv`), and open interest (`oi`).
- Uses O(1) `marketsById` and `tickersBySymbol` hash lookups

**Orderbook**:

- Throttled to 100ms.
- Updates best bid/ask in the ticker on each book update.
- Uses callback-based `messageHandlers` (not direct store updates).
- `book.{instrument}.50` is left in the documented default `SNAPSHOT` mode.
  Every frame replaces both sides wholesale before sorting and calculating
  totals. Do not apply these frames as deltas: absent levels must disappear or
  the book accumulates phantom liquidity.

**OHLCV via WebSocket**:

- Supported (unlike Kraken). Uses `candlestick.<interval>.<symbol>` channel
- Safe-cex intervals are mapped to the case-sensitive exchange enum (`1d` →
  `1D`, `1w` → `7D`); unsupported `3m` and `6h` requests fail before network
  I/O. Dispatch uses the full `result.subscription`, not the generic
  `result.channel` value.
- Waits for connection if not yet connected (100ms retry loop)
- Returns cleanup function that unsubscribes and cleans up message handlers

**Unsubscribe**:

- Sends `{ method: 'unsubscribe', params: { channels: [...] } }`
- Removes channels from the `subscriptions` Set

### Private WebSocket (`cryptocom.ws-private.ts`)

**URL**: `wss://stream.crypto.com/exchange/v1/user` (production) / `wss://uat-stream.3ona.co/exchange/v1/user` (testnet)

**On Open**:

1. Reset `isAuthenticated = false`
2. Wait one second for the pro-rated connection rate limit to reset.
3. Send auth message via `generateWsAuthMessage()`.

**Authentication Response**:

- `method === 'public/auth'` and `code === 0` -> set `isAuthenticated = true`, call `resubscribe()`
- Non-zero `code` -> emit error

**Channel Subscriptions**:

- `user.order` -- regular order status updates
- `user.advance.order` -- advanced TP/SL lifecycle updates
- `user.positions` -- position updates
- `user.balance` -- balance changes

All four channels are enqueued automatically during startup and flushed after
the auth acknowledgement.

**Message Routing**:

- `code` field present -> response handling (auth result, errors)
- `result.channel` present -> data routing:
  - `user.order*` -> `handleOrderUpdate()`: maps to internal `Order`, calls `addOrUpdateOrder()`
  - `user.positions` -> `handlePositionUpdate()`: maps to internal `Position`, handles zero-quantity closure
  - `user.balance` -> `handleBalanceUpdate()`: applies the account summary and
    collateral rows immediately

**Order Update Handling**:

- Processes every order in a batched `data[]` frame with an indexed loop.
- Maps `order_type`, `side`, `order_status`, `time_in_force` to internal types
- Uses `store.addOrUpdateOrder()` for individual updates (not full replacement)

**Position Update Handling**:

- Processes every position in a batched `data[]` frame with an indexed loop.
- If `quantity === 0`: removes position from store by filtering it out
- If non-zero: creates/updates position via `store.addOrUpdatePosition()`
- Entry price calculated as `cost / contracts`
- Live frames populate leverage, liquidation price, mark price, and isolated
  margin identity. The current REST `private/get-positions` schema does not
  carry leverage or liquidation price, so its recovery snapshot retains the
  conservative `1` / `0` fallbacks until WebSocket enrichment arrives.
- Position ID created with `createPositionId(symbol, side, positionMode)`

**Reconnection**: On reconnect, `onOpen` resets `isAuthenticated` and re-authenticates. After successful auth, `resubscribe()` re-sends all stored subscriptions.

## Order Types and Mappings

### Internal to Crypto.com (`mapOrderType`)

| Internal               | Crypto.com      |
| ---------------------- | --------------- |
| `OrderType.Market`     | `'MARKET'`      |
| `OrderType.Limit`      | `'LIMIT'`       |
| `OrderType.StopLoss`   | `'STOP_LOSS'`   |
| `OrderType.TakeProfit` | `'TAKE_PROFIT'` |
| _(default)_            | `'LIMIT'`       |

### Crypto.com to Internal (`mapCryptocomOrderType`)

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

### Placing Orders

1. Look up market by symbol
2. Build `CryptocomOrderRequest` with `instrument_name`, `side` (BUY/SELL), `type`
3. Send `quantity` for both MARKET sides; `notional` is not used because it is
   only valid for MARKET BUY by cost.
4. Send `price` for LIMIT orders.
5. Add `REDUCE_ONLY` to `exec_inst` when requested.
6. Add `time_in_force` only for LIMIT orders; omit UI default TIF on MARKET
   orders.
7. POST to `/private/create-order` and return `[data.order_id]`.

Standalone stop-loss and take-profit orders use
`/private/advanced/create-order` with `ref_price` and `ref_price_type`; their
lifecycle is reconciled through `user.advance.order` and the advanced open-order
snapshot. Advanced orders are amended and canceled through the matching
advanced endpoints. Parent-attached bracket derivation is not implemented.

### Updating Orders

- Uses `/private/amend-order` endpoint (true amend, not cancel-replace)
- Sends both mandatory `new_quantity` and `new_price`, backfilling the
  unchanged field from the existing order.
- Returns new `order_id` from response

### Canceling Orders

- **Regular**: POST `/private/cancel-order` with `instrument_name` and `order_id`.
- **Advanced trigger**: POST `/private/advanced/cancel-order` with `order_id`.
- **Multiple**: Sequential cancellation via `forEachSeries`; individual
  failures do not stop remaining cancels.
- **All for symbol**: POST `/private/cancel-all-orders` with `instrument_name`.
- **All globally**: POST `/private/cancel-all-orders` with empty params.

### Batch Order Placement

Not a true batch API call -- orders are placed sequentially in a `for` loop. Individual failures are caught and logged but do not stop remaining orders.

## Symbol Format

- Perpetual futures: `{BASE}{QUOTE}-PERP` (e.g., `BTCUSD-PERP`, `ETHUSD-PERP`)
- Filtering: Only instruments with `inst_type === 'PERPETUAL_SWAP'` and `tradable === true` are loaded (`fetchMarkets`)
- `market.id` and `market.symbol` are both the raw `symbol` (no remapping)
- `market.info.underlyingSymbol` retains the API's distinct index instrument
  name for `index.<underlying_symbol>` subscriptions and frame routing.
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

- Calculated as `cost / Math.abs(quantity)` from position data.
- **Not** directly from an API field

## Rate Limiting

- The client enforces a conservative aggregate **50 RPS** ceiling with
  `axios-rate-limit`. Upstream public endpoints allow 100 requests/second per
  method/IP, while private limits are per method/API key (create/cancel 15 per
  100ms, most other private methods 3 per 100ms, and trade/order history 1 per
  second).
- Network error retry: 3 retries via `axios-retry` on network errors for reads;
  private write endpoints are deliberately not retried
- Request timeout: `options.extra.recvWindow` or **30 seconds** default.
- WebSocket ticker throttle: 1000ms
- WebSocket orderbook throttle: 100ms

## State Reconciliation

### Orders

- **Initial load**: `fetchOrders()` called at end of `start()`, separate from tick cycle
- **Tick cycle**: Re-fetched every 4th tick (~40 seconds)
- **WebSocket**: `handleOrderUpdate()` processes every order in each batched
  frame via `addOrUpdateOrder()`.
- **No throttled REST sync**: Unlike Kraken, there is no throttled fetch after WS events

### Positions

- **Tick cycle**: `fetchPositions()` runs every tick (~10 seconds)
- **WebSocket**: `handlePositionUpdate()` processes every entry in batched updates
  - Zero quantity -> removes position from store
  - Non-zero -> creates/updates position
- **Fake positions**: Created for all active perpetual markets without real
  positions (both long and short), combined with actual positions and
  deduplicated via `uniqBy(id)`.

### Balances

- **Tick cycle**: `fetchBalance()` runs every tick (~10 seconds)
- **WebSocket**: `handleBalanceUpdate()` maps `data[0]` totals and
  `position_balances[]` directly into the store for immediate updates.
- **uPnL**: REST reconciliation calculates it from positions via
  `sumBy(positions, 'unrealizedPnl')`; live balance frames use the exchange
  summary value.

### Tick Cycle

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
  hideMarginTypeSelector: true,    // isolated creation is not implemented
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

1. **Position mode is always hedged**: `changePositionMode()` is a no-op and
   metadata hides the toggle.
2. **REST position recovery is less detailed than WebSocket state**: current
   `private/get-positions` rows omit leverage, liquidation price, and mark
   price. REST snapshots use `1` / `0` fallbacks; `user.positions` enriches
   these fields live.
3. **REST and WebSocket balance shape**: the useful summary is `data[0]`, with
   collateral rows nested under `position_balances[]`. All numeric values are
   strings at the boundary.
4. **Private subscriptions auto-start**: order, advanced-order, position, and
   balance channels are queued before auth and flushed on the auth ack.
5. **No precision adjustment in order writes**: callers must provide values at
   market precision; `placeOrder()` does not call `adjust()`.
6. **Sequential batch cancellation**: `cancelOrders()` intentionally continues
   one order at a time after individual failures.
7. **Ticker valuations use dedicated public streams**: startup always
   batch-subscribes `ticker` for all markets, then adds `mark`, `index`, and
   current `funding` as capacity allows under the local 350-subscription cap.
   Invalid or non-positive price values are ignored; funding may be negative or
   zero. Index topics use each market's distinct `underlying_symbol` when it
   matches the documented `*-INDEX` shape (for example, `BTCUSD-INDEX`).
   `estimatedfunding` is deliberately not mapped onto the current-rate field.
8. **Account identity**: `getAccount()` uses the current
   `private/get-accounts` endpoint and returns `this_account.uuid`; the removed
   `private/get-account-summary` path must not be restored.
9. **Default orderbook mode is full snapshot**: every book frame replaces the
   entire local top 50. Applying these frames as deltas creates stale levels.
10. **Crypto.com uses `instrument_name` as both market id and symbol**, except
    index frames, whose instrument is the distinct underlying index symbol.
11. **Response interceptor unwraps `result`**: callers receive the result
    object, not the full `{ id, method, code, result }` envelope.
12. **HTTP-over-WebSocket is conditional**: only enabled when
    `tealstreetTunnel` is true.

## Error Handling

### REST API Response Interceptor

- **Success check**: `code === 0` in the response envelope
- **Error handling**: Non-zero `code` -> logs via `emitAndLogError`, throws `Error` with pattern: `"Crypto.com API Error {code}: {message}"`
- **HTTP errors**: Extracts `code` and `message` from `error.response.data`, logs and rethrows
- **No specific error code handling**: All non-zero codes are treated identically (no special cases for rate limits, auth failures, etc.)

### Network Retry

- 3 retries on network errors via `axios-retry`.
- 30-second timeout (or `recvWindow` from options)

### Exchange Method Error Handling

Each exchange method wraps its API call in try/catch:

- `fetchMarkets`, `fetchTickers`, `fetchOHLCV`, `fetchOrderBook`: log at `LogSeverity.Error`, rethrow
- `fetchOrders`, `fetchPositions`: log at `LogSeverity.Error`, return empty array `[]`
- `fetchBalance`: log at `LogSeverity.Error`, rethrow
- `placeOrder`, `cancelOrder`, `updateOrder`, `setLeverage`, `changeMarginType`: log and rethrow

### WebSocket Error Handling

- **Public WS**: Non-zero `code` in response messages logged via `emitAndLogError`
- **Private WS**: Same pattern for non-zero response codes
- **Auth failure**: Private WS does not explicitly retry auth on failure; relies on reconnection cycle
- **Parse errors**: Caught and logged via `emitAndLogError` in both public and private WS

### Account Validation

Validation posts to `/private/user-balance` with empty params and returns the
mapped balance on success or a narrow error string on failure.

## Testing / Validation

Fresh audit regressions live in `src/__tests__/audit-regressions/` and cover:

- recursive request signing and virtual-clock nonces;
- MARKET quantity, REDUCE_ONLY, regular amend, and advanced TP/SL write paths;
- mandatory heartbeat replies and one-second reconnect pacing;
- full-snapshot orderbook replacement;
- batched order/position frames, leverage/liquidation enrichment, and live balance;
- REST and WebSocket candlestick interval mapping and dispatch;
- batched public valuation subscriptions and mark/index/current-funding frames;
- hidden/fail-closed legacy margin switching and real account identity.

Before enabling Crypto.com outside development, run UAT end to end with real
credentials for create/amend/cancel (regular and advanced), reconnect recovery,
position/balance state, and isolated-position behavior.

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `client_oid`, rule 1-36 characters.

## Rust engine

`engine: 'rust'` selects `sdk-rs/venues/cryptocom` through the wasm host.
The engine streams nested REST lists and stores account settings separately
from open positions. Failed reads retain state; snapshots and stream frames
respect terminal order/position timestamp fences. Execution fills come from
`user.trade`, deduplicated by trade id, with each execution's own price.
Caller `client_oid` survives incoming order mapping; native amend preserves it.
The Rust caller-ID validator uses a conservative 36-byte ceiling.

Rust deliberately corrects these TS paths: post-only uses `exec_inst` plus
`GOOD_TILL_CANCEL`; leverage uses account identity and an integer on the global
account endpoint; unsupported attached brackets and opening triggers reject
before writes. Default snapshot books replace both sides. Public ticker/book
updates process every symbol rather than sharing the TS handler throttles.
History reads use exact nanosecond cursors, overlap/deduplicate boundaries and
walk ranged income to exhaustion. Settlement PnL uses `realized_pnl`; historical
requested limit prices stay separate from triggers. The replay harness records
these corrections under `KNOWN_TS_FIXES`.

Native REST `fetchOrderBook` uses unsigned GET `/public/get-book` with the raw
URL-encoded instrument name and fixed depth50 on live/UAT; caller limit is ignored
as in TS. It needs no market seed, credentials or subscription and never changes
the live book, ticker or account stores. The required numeric code0 response uses
only the first book; raw price IDs, native row order and zero quantities survive,
with total0 and no update ID. Missing/empty/null data or a null first book is empty;
an explicitly null result or malformed financial levels fails atomically. Missing
result uses outer data, while a present result takes precedence. Native/HTTP/read
transport failures are owned typed NotSent errors; public auth-looking errors
never invalidate account keys. Rate hints are preserved under the shared cap and
no automatic retries or stream recovery are introduced. The standalone Crypto.com
family explicitly enables its JSON order-book command decoder.
