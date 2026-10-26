# Backpack Exchange Integration

## Exchange Information

- **Type**: Centralized Exchange (CEX)
- **Markets**: Perpetual futures (USDC-margined) and spot
- **Position Model**: Backpack exposes one signed net quantity per symbol. Safe-cex uses its `Hedged` internal shape to provide long/short UI slots, but Backpack cannot hold both sides simultaneously upstream.
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

## Authentication

Backpack uses **ED25519 signature authentication** for all state-mutating requests.

### Required Headers

- `X-API-Key`: Base64-encoded public key
- `X-Signature`: Base64-encoded ED25519 signature
- `X-Timestamp`: Unix time in milliseconds
- `X-Window`: Validity window in milliseconds (default: 5000ms, max: 60000ms)
- `X-Broker-Id`: Broker identifier (u16 integer) - Tealstreet's broker ID is `1200`

Both REST and private-WebSocket signatures source their timestamp from the
shared `virtualClock`, not `Date.now()`. This keeps the signed timestamp inside
Backpack's narrow request window when the local system clock is skewed.
REST signatures default safe-cex's `X-Window` to Backpack's documented maximum
of 60000ms, unless `extra.recvWindow` explicitly supplies a smaller value. This
keeps hosted/proxied requests from expiring while the browser remote signer and
proxy path finish dispatching the signed request.

### Signature Generation Process (`backpack.signing.ts`)

1. Alphabetically sort request parameters by key
2. Convert to query string format: `key1=value1&key2=value2` (URI-encoded values, nulls/undefineds skipped)
3. Build payload: `instruction=<type>&<sorted_params>&timestamp=<ts>&window=<win>` (if no params: `instruction=<type>&timestamp=<ts>&window=<win>`)
4. Encode payload as UTF-8 bytes
5. Decode Base64 private key to bytes
6. Sign with ED25519 via `@noble/ed25519` (`signAsync`)
7. Base64-encode the resulting signature bytes

The SHA-512 implementation from `@noble/hashes/sha2.js` is installed as
`ed25519.hashes.sha512` at module load time.

`POST /api/v1/orders` has a distinct signing contract. Its JSON body stays an
array: sort each row independently, prefix each row with
`instruction=orderExecute`, concatenate the row query strings, then append one
shared `timestamp` and `window`. Never object-spread the array into numeric
keys. Backpack's current primary OpenAPI shows a two-row example but publishes
no request `maxItems`; safe-cex therefore caps active batching at exactly two
rows and does not rely on the older, unsupported claim that the maximum is 20.

### Instruction Types

Each authenticated endpoint maps to a specific instruction string used in signature generation:

| Endpoint                           | Method | Instruction            |
| ---------------------------------- | ------ | ---------------------- |
| `/api/v1/order`                    | GET    | `orderQuery`           |
| `/api/v1/order`                    | POST   | `orderExecute`         |
| `/api/v1/order`                    | DELETE | `orderCancel`          |
| `/api/v1/orders`                   | GET    | `orderQueryAll`        |
| `/api/v1/orders`                   | POST   | `orderExecute`         |
| `/api/v1/orders`                   | DELETE | `orderCancelAll`       |
| `/api/v1/capital`                  | GET    | `balanceQuery`         |
| `/api/v1/capital/collateral`       | GET    | `collateralQuery`      |
| `/api/v1/position`                 | GET    | `positionQuery`        |
| `/api/v1/account`                  | GET    | `accountQuery`         |
| `/api/v1/account`                  | PATCH  | `accountUpdate`        |
| `/wapi/v1/history/fills`           | GET    | `fillHistoryQueryAll`  |
| `/wapi/v1/history/orders`          | GET    | `orderHistoryQueryAll` |
| `/wapi/v1/capital/deposits`        | GET    | `depositQueryAll`      |
| `/wapi/v1/capital/withdrawals`     | GET    | `withdrawalQueryAll`   |
| `/wapi/v1/capital/deposit/address` | GET    | `depositAddressQuery`  |

Public endpoints (no auth required): `/api/v1/time`, `/api/v1/status`, `/api/v1/assets`, `/api/v1/markets`, `/api/v1/market`, `/api/v1/ticker`, `/api/v1/tickers`, `/api/v1/depth`, `/api/v1/klines`, `/api/v1/trades`, `/api/v1/trades/history`, `/api/v1/markPrices`, `/api/v1/openInterest`, `/api/v1/fundingRates`. `isPublicEndpoint()` is the runtime source of truth.

### WebSocket Authentication

Private WebSocket subscriptions include a `signature` object with `{key, signature, timestamp, window}`. The signature payload for WebSocket is `instruction=subscribe&timestamp=<ts>&window=<win>` (no additional params). `generateWsAuthSignature()` uses the same virtual-clock timestamp source as REST signing.

## Startup Sequence

Defined by `BackpackExchange.start()`:

1. **`fetchMarkets()`** (sequential) -- fetches `/api/v1/markets`, filters for symbols containing `_` with `visible !== false`, maps to `Market[]`
2. **Store update**: markets + `loaded.markets = true`
3. **`fetchTickers()`** (sequential) -- fetches `/api/v1/tickers` and `/api/v1/markPrices` in parallel (`Promise.all`). **All** markets are included: perps carry mark/index/funding from the mark-price endpoint, spot markets (no mark price) fall back to `last` for mark/index and carry `fundingRate: 0`/`fundingTime: 0`. (Previously spot was filtered out via `markPriceMap.has(t.symbol)`, which made spot markets invisible in the screener / market dropdown since both iterate `store.tickers`.)
4. **Store update**: tickers + `loaded.tickers = true`
5. Open the public WebSocket and schedule lightweight all-market subscriptions after a two-second connection delay.
6. Public-data-only instances mark account state loaded and stop here.
7. Authenticated instances run **`tick()`** (sequential) -- initial data fetch cycle:
   - `fetchAccountInfo()` -- gets account leverage limit from `/api/v1/account`
   - `fetchPositions()` -- gets positions from `/api/v1/position`, creates fake zero-size positions for all perp markets without active positions
   - `fetchBalance()` -- gets collateral/margin info from `/api/v1/capital/collateral`
   - Recalculates uPnL from positions sum
8. **`fetchOrders()`** once more for the explicit startup snapshot, then store it and mark orders loaded.
9. Connect the private WebSocket.

`subscribeToAllMarkets()` subscribes ticker, trades, and bookTicker for every
market after the public socket has had two seconds to establish. Full
`depth.<symbol>` streams are intentionally on-demand: `listenOrderBook()` owns
their subscription refcount plus the matching REST snapshot/delta lifecycle.

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
- `POST /api/v1/orders` - Place one conservative two-row native LIMIT batch (instruction: `orderExecute`; special per-row signing contract)
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
- `GET /wapi/v1/history/fills` - Fill history (instruction: `fillHistoryQueryAll`); `fetchHistoricalFills()` maps this response into Tealstreet execution records
- `GET /wapi/v1/history/orders` - Order history (instruction: `orderHistoryQueryAll`)

Backpack has a real `fetchHistoricalFills()` implementation. Runtime collection
is gated by the capability brand and journal axis support, not by exchange
metadata.

## WebSocket Implementation

### Base URLs

- **Production**: `wss://ws.backpack.exchange`
- **Testnet**: Same as production (no separate testnet)

### Public WebSocket (`backpack.ws-public.ts`)

**Connection**: Creates WebSocket via `createWebsocket()` in `connectAndSubscribe()`. Registers `onOpen`, `onMessage`, `onClose` handlers.

**On open** (`onOpen`):

1. `trackLastReceived()` -- start health monitoring
2. `resetAndRefetchOrderBooks()` -- resets all orderbook states and refetches snapshots
3. `resubscribe()` -- re-sends all tracked subscriptions
4. `ping()` -- starts ping cycle

**Subscriptions**: Tracked in a `Set<string>`. Each `subscribeTo*()` method adds the stream to the set and sends a subscribe message. On reconnect, `resubscribe()` sends all tracked streams in a single message.

**Message format**:

- Subscribe: `{ "method": "SUBSCRIBE", "params": ["stream1", "stream2"] }`
- Unsubscribe: `{ "method": "UNSUBSCRIBE", "params": ["stream1", "stream2"] }`
- Messages: `{ "stream": "<stream_name>", "data": { "e": "<event_type>", ... } }`

**Public streams**:

| Stream                      | Format                     | Throttle                                | Subscription scope                           |
| --------------------------- | -------------------------- | --------------------------------------- | -------------------------------------------- |
| `ticker.<symbol>`           | `ticker.BTC_USDC_PERP`     | 1000ms                                  | Every market at startup                      |
| `depth.<symbol>`            | `depth.BTC_USDC_PERP`      | handler: none; ticker: 100ms per symbol | On demand while an orderbook listener exists |
| `trade.<symbol>`            | `trade.BTC_USDC_PERP`      | none                                    | Every market at startup                      |
| `bookTicker.<symbol>`       | `bookTicker.BTC_USDC_PERP` | none                                    | Every market at startup                      |
| `kline.<interval>.<symbol>` | `kline.1m.BTC_USDC_PERP`   | none                                    | On demand                                    |

**Throttling details**:

- Depth updates dispatch to every active orderbook handler immediately. Only the derived ticker bid/ask write is throttled, independently per symbol, so one market's depth traffic cannot discard another market's deltas.
- Backpack documents depth events as sequential: an aggregate event may span `U..u`, but its `U` must equal the prior event's `u + 1`. After a REST snapshot, buffer deltas and apply only that exact bridge; a mismatch triggers a bounded resnapshot. The recovery streak resets only after a snapshot bridges every buffered delta. After three consecutive non-bridging resnapshots, clear and disable the local book until the socket reconnects; do not retain a stale book, buffer later deltas, or reconnect from the depth handler.
- `handleTickerUpdate`: lodash `throttle(fn, 1000, {leading: true, trailing: true})`
- Trade and bookTicker updates are not throttled

**NaN protection**: All parsed float values are checked with `!Number.isNaN()` before updating store. This prevents overwriting valid data with `NaN` from malformed messages.

### Private WebSocket (`backpack.ws-private.ts`)

**Connection**: Same URL as public. On open, authenticates then subscribes.

**On open** (`onOpen`):

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

- **Position fetch** (`throttledFetchPositions`): Min 500ms between fetches. Triggered on `orderFill` events. Uses custom throttle with pending-event tracking and commits the fetched full position snapshot to the store.
- **Balance fetch** (`throttledFetchBalance`): Min 500ms between fetches. Triggered on `orderFill` events. Uses the same throttle pattern and commits the fetched balance to the store.
- **Order fetch** (`throttledFetchOrders`): The first order update starts one 10-second trailing reconciliation window. Later updates share that timer, so sustained activity cannot postpone the full REST fetch indefinitely.

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

| Tealstreet Type | API `orderType` | Key Fields                                                        |
| --------------- | --------------- | ----------------------------------------------------------------- |
| Market          | `Market`        | `quantity`                                                        |
| Limit           | `Limit`         | `price`, `quantity`, `timeInForce`, `postOnly`                    |
| StopLoss        | `Market`        | `triggerPrice`, `triggerQuantity`, `triggerBy`, `reduceOnly=true` |
| TakeProfit      | `Market`        | `triggerPrice`, `triggerQuantity`, `triggerBy`, `reduceOnly=true` |

### Time In Force

- `GTC` - Good Till Cancel (default)
- `IOC` - Immediate Or Cancel
- `FOK` - Fill Or Kill
- `PO` - Post Only (sent via `postOnly: true` field alongside `timeInForce: GTC`)

### Order Sides

Backpack uses `Bid` (buy) and `Ask` (sell) instead of standard buy/sell terminology.

### Trigger Orders

- `triggerBy` determines which price triggers the order: `MarkPrice`, `IndexPrice`, or `LastPrice`
- Primary TP/SL orders use `opts.tpTriggerType` / `opts.slTriggerType` when supplied; otherwise they use Backpack metadata's advertised defaults (currently `MarkPrice` for both).
- Trigger orders use `triggerQuantity` instead of `quantity`

### Attached TP/SL

- Market orders support attached TP/SL via `takeProfitTriggerPrice`/`stopLossTriggerPrice` fields (plus corresponding `*LimitPrice` and `*TriggerBy` fields); their trigger references use the same explicit-or-metadata-default mapping as primary TP/SL orders.
- Limit orders with TP/SL place separate trigger orders instead (attached TP/SL only works for immediately-filling orders). `placeOrder` submits the main limit order first, then TP and SL sequentially with per-order error handling.

### Self-Trade Prevention

Supported values: `RejectTaker`, `RejectMaker`, `RejectBoth`, `Allow`

## Order Lifecycle

### Order Placement (`placeOrder`)

1. Look up market, validate it exists
2. Determine `apiOrderType` (`Limit` or `Market`), handle PostOnly via `postOnly: true` + `timeInForce: GTC`
3. Format price and amount to market precision using `formatPrice()` / `formatAmount()` helpers
4. For trigger orders: set `triggerPrice`, `triggerQuantity`, the explicit-or-metadata-default `triggerBy`, and `reduceOnly: true`
5. For limit orders with TP/SL: place main order, then separate TP/SL trigger orders sequentially
6. For market orders with TP/SL: use attached `takeProfitTriggerPrice`/`stopLossTriggerPrice` params
7. Returns array of order IDs

`placeOrders()` uses `POST /api/v1/orders` only for adjacent pairs of
independently placeable ordinary LIMIT orders. Both rows must resolve to active,
writable markets, carry finite positive price/amount values within local amount
bounds, and have none of the trigger, TP/SL, or `close` semantics. MARKET,
trigger, TP/SL/composite, invalid/oversize, and one-order-tail rows keep the
existing single path. Both rows and unique positive uint32 `clientId` values are
built before the single batch write. Responses must contain exactly two `Ok`
rows and are correlated by echoed `clientId`, never array position; missing,
duplicate, unknown, malformed, or failed rows, duplicate/empty exchange IDs, or
identity mismatches fail the whole pair without inventing IDs. Batch transport
and decode failures are emitted to the exchange error channel while
`placeOrders()` continues with later inputs as before.

REST order mapping preserves Backpack's documented `reduceOnly` response flag.
For older responses that omit it, trigger orders retain the conservative
reduce-only fallback used before the field was exposed.

### Order Modification (Cancel-and-Replace, `updateOrder`)

Backpack does not support native order amendment. Updates are implemented as:

1. Find existing order in store by ID
2. Validate new amount is positive (before cancelling, to avoid orphaned cancellations)
3. Cancel existing order via `cancelOrder()`
4. Place new order via `placeOrder()` with updated params
5. If replacement placement fails, the original is already cancelled (logged as error)

### Order Cancellation

- **Single order**: `DELETE /api/v1/order` with `{symbol, orderId}`
- **Per-symbol**: `DELETE /api/v1/orders` with `{symbol}`
- **All orders**: Iterates all markets serially via `forEachSeries`, calling cancel-all per symbol
- **Batch cancel**: `cancelOrders()` cancels sequentially via `forEachSeries`, continuing on individual errors

### Trigger Order Fix on Startup (`fixTriggerOrdersWithoutTriggerBy`)

On every `fetchOrders()` call (startup and each tick), scans for trigger orders where `triggerBy === null || triggerBy === undefined`:

1. Identifies affected orders (has `triggerPrice` but missing `triggerBy`)
2. For each: determines if TP or SL via `determineTriggerOrderType()` (compares trigger price to current market price)
3. Cancels the old order
4. Recreates with the metadata default `triggerBy` for the classified TP or SL order
5. Updates the orders array in-place so the caller gets corrected data
6. Continues with other orders even if one fix fails

### Trigger Order Type Detection (`determineTriggerOrderType`)

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

Only markets containing `_` and with `visible !== false` are loaded. **Both** spot and perpetual markets are included in the ticker list (spot tickers use `last` for mark/index and carry no funding).

**Default symbol**: `BTC_USDC_PERP` | **Default quote**: `USDC`

## Data Precision & Conversion

### Price Formatting (`formatPrice`)

1. Get `market.precision.price` (e.g., `0.01` for 2 decimals)
2. Calculate factor: `1 / precision`
3. Round: `Math.round(price * factor) / factor`
4. Count decimal places from precision string representation
5. Format with `toFixed(decimals)` to avoid floating point representation issues (e.g., `0.1 + 0.2` artifacts)

### Amount Formatting (`formatAmount`)

Same algorithm as `formatPrice` but uses `market.precision.amount`.

### Precision Sources (`fetchMarkets`)

- `precision.amount`: from `quantityIncrement` or `filters.quantity.stepSize` (fallback `0.001`)
- `precision.price`: from `priceIncrement` or `filters.price.tickSize` (fallback `0.01`)
- Max leverage: calculated from IMF (Initial Margin Fraction): `1 / imfFunction.base`, rounded to integer

### Timestamp Conversions

- WebSocket event timestamps (`E` field): **microseconds** -- divided by 1000 for milliseconds
- Kline API input: milliseconds divided by 1000 to **seconds**
- Kline start times: ISO date strings (e.g., `"2025-12-15 13:28:00"`) parsed with `+'Z'` suffix for UTC
- WS kline timestamps: ISO strings (e.g., `"2025-12-15T19:02:00"`) parsed with `+'Z'` suffix

## Rate Limiting

- **REST API**: Configured at `maxRPS: 10` (10 requests per second via `axios-rate-limit`)
- **Network retry**: 3 retries on network errors only (via `axios-retry`, not HTTP errors)
- **Tick interval**: 10 seconds between REST polling cycles (`loop(() => this.tick(), 10_000)`)
- **Metadata tick interval**: 2000ms (`tickInterval: 2000` in exchangeMetadata)
- **Request timeout**: 30 seconds default (configurable via `extra.recvWindow`)

### WebSocket Throttling

- **Depth updates**: Orderbook deltas are unthrottled. Only the derived ticker bid/ask side effect is throttled to 100ms per symbol (leading + trailing).
- **Ticker updates**: Throttled to 1000ms (leading + trailing)
- **Position fetch after fill**: Min 500ms interval
- **Balance fetch after fill**: Min 500ms interval
- **Order fetch after update**: One trailing fetch 10 seconds after the first pending update; sustained activity cannot reset the timer

## State Reconciliation

### Orders

- REST `fetchOrders()` returns the full set of open orders and replaces the store
- REST order fetch failures are rethrown, so `tick()` and WS reconciliation retain the previous store snapshot instead of publishing an empty order list
- WS `handleOrderUpdate()` calls `store.addOrUpdateOrder()` for incremental updates
- After any WS order update, `throttledFetchOrders()` schedules a full REST refetch 10 seconds after the first pending update; subsequent events are batched without delaying that deadline
- Fill events on WS trigger `throttledFetchPositions()` and `throttledFetchBalance()`; their REST results are committed immediately after each throttled fetch completes

### Positions

- REST `fetchPositions()` returns all real positions + fake zero-size positions for every perp market without an active position
- Unexpected REST position fetch failures are rethrown, so `tick()` retains the previous position snapshot; the documented endpoint-unavailable response remains the only fallback to fake positions
- Fake positions are created for both Long and Short sides per symbol (hedged mode)
- Actual positions override fakes via `uniqBy([...actualPositions, ...fakePositions], 'id')`
- WS `handlePositionUpdate()` calls `store.addOrUpdatePosition()` for incremental updates
- Backpack exposes one signed `netQuantity` per symbol: positive is long and negative is short. Before an incremental upsert, `handlePositionUpdate()` removes the opposite internal side so a single fill cannot leave both rows active after a side flip.
- A zero-size WS position update has no meaningful side and removes both internal side records for that symbol
- Leverage for positions comes from `accountLeverageLimit` (REST) or `1 / imf` (WS position update)

### Orderbook

Rust `fetchOrderBook` is an independent unsigned `GET /api/v1/depth` query.
It needs neither cached markets nor a live book subscription. It preserves native
side order, raw price strings as IDs, parsed price/amount and `total: 0`; optional
limits are ignored as in TS, and no `lastUpdateId` is returned. Empty and one-sided
books remain valid; malformed shapes/nonfinite values reject the whole query.
Query responses never publish or repair a stream book. Public refusals have owned
typed failures without account-auth evidence or automatic retry; native error
code/message and usable rate-limit hints are retained. Standalone builds opt into
only the shared order-book JSON command decoder.

Snapshot-then-delta approach (`listenOrderBook` in `backpack.ws-public.ts`):

1. Subscribe to `depth.<symbol>` WebSocket stream
2. Initialize buffer state (`OrderBookState`: `updates[]`, `isSnapshotLoaded`, `lastUpdateId`)
3. Fetch full snapshot via REST `GET /api/v1/depth`
4. Drop buffered updates older than snapshot's `lastUpdateId`
5. Apply the first remaining buffered update when its aggregate `U`/`u` range spans `lastUpdateId + 1` (`U <= expected <= u`); after that initial bridge, require each delta's `U` to equal the prior `u + 1`. `lastUpdateId` advances to each applied `u`.
6. Mark snapshot as loaded, clear buffer
7. Apply subsequent delta updates in real-time via `updateOrderBook()` utility
8. On a non-bridging sequence range, stop applying deltas and perform an authoritative REST resnapshot. Recovery is capped at three consecutive non-bridging resnapshots and never reconnects the WebSocket; a successful bridge resets the streak. Reconnect resets each book and refetches snapshots. Terminal failure emits exactly one exchange error for the affected symbol, then clears that symbol's listeners' local books.
9. On WebSocket reconnect, reset all states and refetch all snapshots

### Balance

- Fetched from `/api/v1/capital/collateral` endpoint
- `total` = `netEquity`, `free` = `netEquityAvailable`, `used` = `netEquityLocked`, `upnl` = `pnlUnrealized`
- In `tick()`, uPnL is recalculated from position sum: `sumBy(positions, 'unrealizedPnl')`

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
- After setting leverage, positions are refetched to update all with the new value

## Testnet/Demo

**Not available.** `supportsTestnet: false` in metadata. The `BASE_URLS.TESTNET` and `WSS_URLS.TESTNET` constants point to the same production URLs.

## Known Quirks / Implementation Notes

1. **WebSocket Ping/Pong**: Backpack does not support standard PING. Sending `{"method":"PING"}` returns error code 4002, which is used as a pong response for latency measurement.

2. **Trigger order `triggerBy` fix**: On every `fetchOrders()` call, `fixTriggerOrdersWithoutTriggerBy()` scans for trigger orders with null `triggerBy` and recreates them using the corresponding Backpack metadata default. Without `triggerBy`, orders may not trigger when price crosses the trigger level.

3. **Kline null values**: WS kline updates with `null` OHLCV values (sent on empty candle close) are skipped.

4. **Kline timestamps**: REST API uses seconds; WS uses ISO date strings. Both require conversion. ISO strings need `+'Z'` suffix for UTC parsing.

5. **Hedged mode fake positions**: Creates zero-size placeholder positions for both Long and Short on every perp market without an active position, so the UI always shows position slots.

6. **CORS required**: `requiresCors: true` -- browser requests need CORS proxy.

7. **HTTP-over-WebSocket / hosted signing**: When `tealstreetTunnel` is enabled, the API factory installs the selected adapter through `setInnerAdapterForRemoteSigning()` so hosted remote signing remains the outer layer. Public REST endpoints are tagged with `markSkipRemoteSigning()` so exchange-server does not round-trip to the browser for requests Backpack never signs.

8. **Market subscription delay**: 2-second `setTimeout` before subscribing to market data streams to allow the WebSocket connection to establish.

9. **Position endpoint fallback**: If `/api/v1/position` returns a "not found" error, gracefully returns only fake positions.

10. **Kline interval format**: WS kline subscriptions poll with 100ms retry for connection readiness (`waitForConnectedAndSubscribe` pattern used in `listenOHLCV` and `listenOrderBook`).

11. **`percentage` field**: Backpack returns `priceChangePercent` as a decimal (e.g., `0.05` for 5%), which is multiplied by 100 for display.

12. **Account validation**: `validateAccount()` only checks if the capital endpoint is accessible (successful response = valid key). No dedicated validation endpoint.

## Error Handling

### REST API Error Formats

Backpack API returns errors in multiple formats, all handled by the response interceptor:

1. **Success response with error object**: `{ "error": { "msg": "..." } }` -- intercepted in success handler
2. **Success response with code+msg**: `{ "code": <number>, "msg": "..." }` -- intercepted in success handler
3. **HTTP error with structured data**: `{ "code": "INVALID_CLIENT_REQUEST", "message": "Quantity decimal too long" }` -- error interceptor
4. **HTTP error with nested error**: `{ "error": { "msg": "..." } }` -- error interceptor
5. **HTTP error with simple msg**: `{ "msg": "..." }` -- error interceptor
6. **HTTP error with string body**: plain text error -- error interceptor
7. **No response data**: original error message used

All errors emit to the user via `exchange.emitter.emit('error', ...)` and are logged with status code and URL for debugging.

### Network Error Retry

`axios-retry` configured with 3 retries for network errors only (`isNetworkError`). HTTP errors (4xx, 5xx) are not retried.

### Graceful Degradation in Tick Loop

The `tick()` method wraps all fetches in try/catch and logs errors without crashing. The next tick is always scheduled via `loop()` regardless of errors.

### WebSocket Error Recovery

- Both public and private WS handle parse errors gracefully (logged via `emitAndLogError`)
- Code 4002 errors are treated as pong responses (not logged as errors)
- All other WS errors are logged
- Connection health monitored via `trackLastReceived()` with automatic reconnection on timeout

## Testing / Validation

Fresh deterministic regressions belong in the package-level
`src/__tests__/audit-regressions/` directory. Do not add test or live-probe
scripts beside the exchange runtime implementation.

## Grafana Monitoring

- **Every-call endpoints**: `ENDPOINTS.ORDER`, `ENDPOINTS.ORDERS`
- **Periodic endpoints**: `ENDPOINTS.CAPITAL`, `ENDPOINTS.POSITIONS`

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `clientId` as a number. Backpack types it `uint32`, so the rule accepts only canonical decimal strings `1..4294967295` — the only form whose `String()` is the caller's value. A caller ID routes the order through `placeOrder`, never a native batch pair, because batch pairs mint their own correlation IDs.

## Rust nullable trigger readbacks

REST order quantity can be missing/null only when positive native triggerPrice
and triggerQuantity plus bounded executedQuantity prove the amount. Account,
history, trigger repair and replacement acknowledgements use one admission guard;
blank/malformed present quantities reject. Missing ordinary quantity now rejects
instead of defaulting to zero. Invalid data preserves known orders and cannot
start repair cancellation or remove the old identity on a malformed replacement.
Existing present-quantity precedence, WS q/Y and terminal-before-ACK guards remain.
