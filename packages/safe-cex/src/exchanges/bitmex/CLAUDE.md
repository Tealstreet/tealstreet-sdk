# BitMEX Exchange Integration

This directory contains the BitMEX exchange integration for Tealstreet.

## Exchange Information

| Property        | Value                                                       |
| --------------- | ----------------------------------------------------------- |
| Exchange Name   | `bitmex`                                                    |
| Pretty Name     | BitMEX                                                      |
| Class           | `BitmexExchange` extends `BaseExchange`                     |
| Testnet Support | Configured in code but `supportsTestnet: false` in metadata |
| Position Modes  | One-Way only                                                |
| Hedge Mode      | Not supported (`isHedged` forced to `false`)                |
| CORS Required   | Yes (`requiresCors: true`)                                  |
| WS Tunnel       | Enforced (`enforceWsTunnel: true`)                          |
| Default Quote   | `USD`                                                       |
| Tick Interval   | 20,000ms                                                    |
| Unified Margin  | No                                                          |
| Chaser Support  | Yes                                                         |

## Exchange Wind-Down

BitMEX announced on 2026-07-23 that new account registrations are stopped and the exchange will close on 2026-09-23 at 04:00 UTC.

Tealstreet encodes the official cutoff dates in `bitmex.exchange.ts`:

- `BITMEX_REDUCE_ONLY_START_MS` = 2026-08-26 04:00 UTC. From this time until closure, `formatCreateOrder()` rejects non-`reduceOnly` order placement, and `updateOrder()` rejects amendments to non-reduce-only orders. Reduce-only close orders stay available so users can exit positions.
- `BITMEX_EXCHANGE_CLOSURE_MS` = 2026-09-23 04:00 UTC. From this time, `start()` short-circuits into a loaded/empty closed state, `validateAccount()` returns a closure error, and trading write paths are blocked with an actionable message.

`exchangeMetadata.ts` must keep both `bitmex` and `bitmextestnet` disabled
(`enabled`, `enabledInDev`, and `enableInPreview` all `false`) so the closed
venue is not offered in any add-account environment. Runtime closure guards stay
in place for legacy or manually imported records that still reach the adapter.

Do not remove these guards while the official shutdown schedule is still in effect. If BitMEX changes the dates or reopens service, update the constants, docs page (`apps/documentation/docs/connect/bitmex.mdx`), and the lifecycle regression tests together.

## API Documentation

- **Official API Documentation**: https://www.bitmex.com/api/explorer/
- **Official Closure Announcement**: [BitMEX Exchange to Sunset on 23 September at 04:00 UTC](https://www.bitmex.com/blog/bitmex-closure)
- **CCXT REST Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/bitmex.ts
- **CCXT WebSocket Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/bitmex.ts

## File Structure

| File                             | Description                                                                                                                                                                                        |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bitmex.exchange.ts`             | Main exchange class (`BitmexExchange`) extending `BaseExchange`. Contains all REST trading operations, balance/position parsing, order formatting, and OHLCV fetching.                             |
| `bitmex.api.ts`                  | Axios HTTP client factory with HMAC-SHA256 authentication interceptor, retry logic, and HTTP-over-WebSocket adapter.                                                                               |
| `bitmex.types.ts`                | Constants (URLs, endpoints, recv window), status/side mappings, `Currency` type, and Grafana endpoint lists.                                                                                       |
| `bitmex.ws-private.ts`           | Private WebSocket (`BitmexPrivateWebsocket`): handles authenticated execution, order, position, and margin/balance updates.                                                                        |
| `bitmex.ws-public.ts`            | Public WebSocket (`BitmexPublicWebsocket`): handles instrument updates, an explicit Aggregated orderBookL2 view, and trade-driven OHLCV with a polling fallback.                                    |
| `bitmex.ohlcv-helpers.ts`        | Shared OHLCV helpers used by `bitmex.exchange.ts` (timeframe/resolution mapping, UDF response validation, candle dedup).                                                                           |
| `bitmextestnet.exchange.ts`      | Thin subclass (`BitmexTestnetExchange`) that overrides URLs/WS hosts to point at the BitMEX testnet. Registered separately in the exchange registry.                                               |
| `types.ts`                       | Shared BitMEX-specific types: `BITMEX_ORDER`, `BITMEX_POSITION`, `BITMEX_MARKET`, WS update types (`ORDER_WS_UPDATE`, `POSITION_WS_UPDATE`, `TRADE_WS_UPDATE`), and `SupportedTimeframes` mapping. |

## Authentication

### REST API (`bitmex.api.ts`)

Uses HMAC-SHA256 signing. The interceptor computes signatures on every request:

- **Headers**:
  - `api-key`: The API key from `options.key` when present. Public-data-only
    clients must omit the header entirely; BitMEX treats an empty `api-key`
    header as an authenticated request and rejects public startup endpoints.
  - `api-expires`: A nonce generated from `virtualClock.getCurrentTimeMs() * 1000 + counter`
  - `api-signature`: HMAC-SHA256 hex digest
- **Signature payload**: `METHOD + "/api/v1" + URL_WITH_PARAMS + NONCE + [BODY_JSON]`
  - Example: `"GET/api/v1/position1709472000000000"`
  - Body JSON is appended only for requests with a body (POST/PUT)
- **Nonce generation**: `generateNonce()` uses a microsecond timestamp plus an incrementing counter (mod 1000) to avoid collisions.
  ```typescript
  const generateNonce = () => +virtualClock.getCurrentTimeMs() * 1000 + (nonceCounter++ % 1000);
  ```
- **Module-level counter**: The `nonceCounter` is a module-level variable (not per-instance), so it persists across API client recreations. The `% 1000` modulo prevents overflow while still providing uniqueness within the same millisecond.
- **Query string serialization**: the request interceptor uses `qs.stringify` with `arrayFormat: 'repeat'`.

### WebSocket Authentication (`bitmex.ws-private.ts`)

- Uses `authKeyExpires` operation
- Signature payload: `"GET" + "/realtime" + TIMESTAMP` signed with HMAC-SHA256 (see `sendAuth`)
- Sends: `{ op: "authKeyExpires", args: [apiKey, timestamp, signature] }`
- Re-authenticates every 30 minutes via `setTimeout` (`WS_AUTH_REFRESH_MS`)
- **Subscribe is sent once per socket connection, not per auth.** On open, auth + subscribe are sent immediately (subscription follows auth without waiting for a response). The 30-min refresh calls `sendAuth()` with `subscribe` defaulting `false`, so it re-signs only. A `privateSubscriptionsSent` flag (reset in `onOpen`) gates the subscribe in **both** `sendAuth` branches (HMAC and `remoteSigner`). Re-sending `subscribe` on the still-connected socket triggered BitMEX "already subscribed to topic:…" replies every 30 min — suppressed from users (see quirk #15) but log-noisy, and repeated WS command errors can eventually get the socket dropped. Don't move the subscribe back into the unconditional auth path.

### UDF Endpoints

Requests to `/udf/*` paths skip authentication and use a different base URL (`www.bitmex.com` instead of `api.direct.bitmex.com`). The request interceptor detects UDF paths and strips all auth headers, returning only `Content-Type`.

## Startup Sequence

The `start()` method executes the following sequence:

### Phase 1: Currency and Market Data (sequential)

1. **Fetch currencies** -- `fetchCurrencies()` calls `GET /wallet/assets` to get currency `scale` and `majorCurrency` mappings. Stored in `this.currencies`. Errors are caught and return `{}` (non-fatal).
2. **Disposal check** -- If disposed during fetch, return early.
3. **Fetch markets and tickers** -- `fetchMarketsAndTickers()` calls `GET /instrument/activeAndIndices`. Parses both markets and tickers from the same response. BTC price is extracted from the `.BXBT` index instrument for quanto contract sizing.
4. **Disposal check** -- If disposed during fetch, return early.
5. **Store updates** -- Markets, tickers, and empty orders (with loaded flags) are set in the store.

### Phase 2: WebSocket Connection

6. **Public WebSocket** -- `publicWebsocket.connectAndSubscribe()` connects and subscribes to `instrument:CONTRACTS`.

### Phase 3: Private or Public-Only Mode

**Private mode** (`_startPrivate`):

1. Connect private WebSocket, authenticate, subscribe to private channels.
2. Set `isHedged = false`.
3. Start `tick()` without awaiting it. The first balance and position HOW requests run in the background so a slow tunnel cannot freeze startup.
4. `tick()` schedules its next run after each attempt (interval from `options.extra.tickInterval`, default 20s).
5. Orders arrive through the private WebSocket partial snapshot during normal startup; reconnect recovery uses the REST `fetchOrders()` snapshot path.

**Public-data-only mode** (`_startPublicDataOnly`):

1. Sets empty orders, zero-balance, empty positions with all loaded flags set to true.

### Tick Loop

The `tick()` method runs on an interval:

1. Fetch balance via `fetchBalance()`.
2. If not disposed, fetch positions via `fetchPositions()`.
3. Update store with `addOrUpdatePositionsFull()` and balance.
4. Schedule next tick via `loop()` utility (uses `requestAnimationFrame` when page is visible, `setTimeout` otherwise).

## REST API Endpoints

All endpoints are defined in `ENDPOINTS` constant in `bitmex.types.ts`:

| Constant        | Path                           | Method              | Description                                                  |
| --------------- | ------------------------------ | ------------------- | ------------------------------------------------------------ |
| `ACCOUNT`       | `/user/margin`                 | GET                 | Validate account, fetch margin/balance data                  |
| `BALANCE`       | `/user/margin`                 | GET                 | Fetch multi-currency balance snapshot (same path as ACCOUNT) |
| `MARKETS`       | `/instrument/activeAndIndices` | GET                 | Fetch all active instruments and indices                     |
| `POSITIONS`     | `/position`                    | GET                 | Fetch all open positions                                     |
| `ORDER`         | `/order`                       | GET/POST/PUT/DELETE | Fetch, place, amend, or cancel orders                        |
| `SET_LEVERAGE`  | `/position/leverage`           | POST                | Set leverage for a symbol                                    |
| `SET_ISOLATE`   | `/position/isolate`            | POST                | Toggle isolated/cross margin                                 |
| `WALLET_ASSETS` | `/wallet/assets`               | GET                 | Fetch currency metadata (scale, majorCurrency)               |
| `ORDERBOOK`     | `/orderBook/L2`                | GET                 | Fetch L2 order book (defined but used via WS)                |
| `KLINE`         | `/udf/history`                 | GET                 | OHLCV candle data (UDF format)                               |
| `EXECUTIONS`    | `/execution/tradeHistory`      | GET                 | Primary execution/trade history source                       |
| `USER_EXECUTION_HISTORY` | `/user/executionHistory` | GET              | One-shot fallback for compatible auth/not-found failures     |

Note: `TICKERS` is defined as `/instrument/activeAndIndices` (same as `MARKETS`) but not used separately.

## WebSocket Implementation

### Public WebSocket (`bitmex.ws-public.ts`)

- **URL**: `wss://ws.bitmex.com/realtime` (livenet) / `wss://ws.testnet.bitmex.com/realtime` (testnet)
- **Subscriptions**:
  - `instrument:CONTRACTS` -- Ticker/instrument updates for all contracts
  - `orderBookL2:{symbol}:Aggregated` -- explicit combined-pool L2 order book (subscribed per-symbol on demand, lowercase symbol)
- **Ping**: Sends literal string `"ping"` and expects `"pong"`.
- **Pong handling**: Calls `handlePongEvent()` from `BaseWebSocket` which schedules next ping in 5 seconds and sets a pong miss timeout at 7.5 seconds (1.5x ping interval) that triggers reconnection.
- **OHLCV**: `listenOHLCV()` builds supported native and derived candles from `trade:{symbol}`. Polling seeds the complete current candle and resumes when the trade feed is stale for 12 seconds; unsupported intervals use polling only.
- **Reconnection**: On open, calls `resetOrderbook()` to clear cached order book state, re-subscribes to all active topics via `subscribe()`, and emits `wsReconnected` event (when `ENABLE_RECONNECTING_WEBSOCKET` is enabled).
- **Message routing**: `instrument` table messages are handled inline for ticker updates. Order-book and trade frames extract their exact wire topic once (`table` + row symbol, plus pool for books) and look up only that topic's handlers. Multiple OHLCV intervals share one `trade:{symbol}` wire subscription while retaining interval-specific stats keys; never restore raw-frame substring scans or record stats for unmatched handlers.
- **Order-book pool policy**: Tealstreet explicitly subscribes to Aggregated. BitMEX documents Aggregated as the unauthenticated default, and the public production feed returns `pool: "Aggregated"`. The parser accepts legacy rows with no pool and rejects Primary/Secondary rows so a price-keyed local book never mixes liquidity views.
- **Topic reference counting**: `this.topics` tracks subscriber count per topic. The last disposer sends `{ op: "unsubscribe", args: [...] }` and deletes the zero-refcount topic so reconnect does not revive it.

### Private WebSocket (`bitmex.ws-private.ts`)

- **URL**: Same as public (`wss://ws.bitmex.com/realtime`), but authenticated
- **Proxy URL**: `resolveExchangeRouting(name, options).wsPrivate` selects the region-specific private tunnel host; the direct BitMEX endpoint is only the fallback (see "Regional routing" below).
- **Subscriptions** (sent immediately after auth, same message):
  - `execution` -- Trade executions
  - `margin` -- Balance/margin updates
  - `csastate` -- CSA (Cross-Settle Account) state
  - `order` -- Order lifecycle updates (insert, update, partial, delete)
  - `position` -- Position updates
- **Ping**: Sends literal string `"ping"` via `sendWithLog`, same pattern as public WS.
- **Reconnection**: On open, sends auth and starts ping. The base housekeep listener invokes `fetchAndSetPrivateSnapshot(10)` after reconnect to reconcile orders from REST.

### Reconnection Logic (inherited from `BaseWebSocket`)

Both WebSockets inherit reconnection behavior from `BaseWebSocket`:

- **Pong miss timeout**: If no pong received within 7.5 seconds after ping, triggers `reconnect()`.
- **Last received check**: `trackLastReceived()` monitors message freshness. If no messages for `WS_RECONNECT_THRESHOLD_MS` (20 seconds), triggers reconnection.
- **Reconnection delay**: `WS_MIN_RECONNECTION_DELAY` = 1000 + random(0-2000)ms (jittered).
- **Max reconnection delay**: 5000ms.
- **Close count guard**: `BaseWebSocket` stops a single unhealthy reconnect streak after 100 consecutive close events; healthy traffic resets the streak.
- **WebSocket type**: Currently uses native `WebSocket` (`ENABLE_RECONNECTING_WEBSOCKET = false`), not `ReconnectingWebSocket`.

### Handled Tables (Private WS)

| Table      | Handler               | Description                                                                 |
| ---------- | --------------------- | --------------------------------------------------------------------------- |
| `execution` | Inline in `onMessage` | Tracks observed `maxSlippagePct` for future protected market orders         |
| `margin`   | Inline in `onMessage` | Merges updates into existing balance snapshot, calls `parseBalanceSnapshot` |
| `order`    | `handleOrder()`       | Merges partial order updates, detects fills and cancellation reasons        |
| `position` | `handlePosition()`    | Merges partial position updates, filters by currency                        |

## Order Types and Mappings

The implementation supports these order types mapped to BitMEX `ordType` values:

| Internal Type          | BitMEX `ordType` (sent) | BitMEX `ordType` (received) | Price Field | Description                     |
| ---------------------- | ----------------------- | --------------------------- | ----------- | ------------------------------- |
| `OrderType.Market`     | `Market`                | `Market`                    | none        | Market order                    |
| `OrderType.Limit`      | `Limit`                 | `Limit`                     | `price`     | Limit order                     |
| `OrderType.StopLoss`   | `StopMarket`            | `Stop`                      | `stopPx`    | Stop market order               |
| `OrderType.TakeProfit` | `MarketIfTouched`       | `MarketIfTouched`           | `stopPx`    | Market-if-touched (take profit) |

### Execution Instructions (`execInst`)

Comma-separated values appended to orders by `formatCreateOrder()`:

- `ReduceOnly` -- For reduce-only Market/Limit orders
- `Close` -- For reduce-only Stop/TP orders
- `ParticipateDoNotInitiate` -- Post-only for orders Tealstreet creates. Incoming `PostOnly` and `PostOnlyReprice` values are also mapped as post-only.
- `MarkPrice` / `IndexPrice` / `LastPrice` -- Trigger price type for Stop/TP orders

### Order Status Mapping (`ORDER_STATUS`)

| BitMEX Status     | Internal Status        |
| ----------------- | ---------------------- |
| `New`             | `OrderStatus.Open`     |
| `PartiallyFilled` | `OrderStatus.Open`     |
| `Filled`          | `OrderStatus.Closed`   |
| `DoneForDay`      | `OrderStatus.Open`     |
| `Canceled`        | `OrderStatus.Canceled` |
| `PendingCancel`   | `OrderStatus.Open`     |
| `PendingNew`      | `OrderStatus.Open`     |
| `Rejected`        | `OrderStatus.Canceled` |
| `Expired`         | `OrderStatus.Canceled` |
| `Stopped`         | `OrderStatus.Open`     |
| `Untriggered`     | `OrderStatus.Open`     |
| `Triggered`       | `OrderStatus.Open`     |

### Supported TP/SL Trigger Types

- `OrderStopTriggerType.LastPrice` (default for TP)
- `OrderStopTriggerType.MarkPrice` (default for SL)
- `OrderStopTriggerType.IndexPrice`

## Order Lifecycle

### Order Placement (`placeOrder` / `placeOrders`)

1. `placeOrder()` / `placeOrders()` calls `formatCreateOrder()` to build payloads.
2. Payloads are sent individually (not `/order/bulk`) but batched in groups of 3 via `Promise.all` in `placeOrderBatch()`.
3. Each batch waits for completion before the next.
4. Uses `unlimitedXHR` (no rate limit) for `POST /order`.
5. All orders include `text: 'Tealstreet'` for identification.
6. If any order in a batch fails, the error is thrown after the batch completes (last error wins).

### Bracket Orders (TP/SL with Entry)

When `opts.stopLoss` or `opts.takeProfit` is provided on a `placeOrder` call:

- The SL/TP orders are appended to the same payloads array as the entry order.
- Side is flipped (Buy -> Sell, Sell -> Buy).
- SL uses `ordType: 'StopMarket'` with `execInst: 'ReduceOnly'`.
- TP uses `ordType: 'MarketIfTouched'` with `execInst: 'ReduceOnly'`.
- The `price` field is omitted from SL/TP payloads (only `stopPx` is set).
- All orders (entry + brackets) go through the same batch mechanism.

### Order Amendment (`updateOrder`)

Uses native `PUT /order` with `orderID` and updated fields:

- For stop/TP orders: price goes to `stopPx` field.
- For limit orders: price goes to `price` field.
- Amount amendments use `orderQty` with micro-value conversion applied.
- Uses `unlimitedXHR` (bypasses rate limiter).
- Returns `[order.id]` on success.

### Order Cancellation (`cancelOrders` / `cancelSymbolOrders`)

**Cancel specific orders** (`cancelOrders`):

- Groups orders by symbol.
- Single orders: `DELETE /order` with `orderID` as param.
- Multiple orders per symbol: chunked into batches of 10. Each chunk sends `orderID` as JSON array string.
- Processes sequentially via `forEachSeries` per symbol group.
- Collects errors from responses and throws aggregated error message.

**Cancel all orders for symbol** (`cancelSymbolOrders`):

- `DELETE /order` with `symbol` param.
- Updates store to mark all matching orders as Canceled.

### Order Updates via WebSocket (`BitmexPrivateWebsocket.handleOrder`)

1. On `partial` action: orders are processed as-is (initial snapshot on connect).
2. On `insert`/`update`/`delete` actions:
   - Fill events emitted for `Filled` or `PartiallyFilled` status.
   - Warning events emitted for PostOnly and ReduceOnly cancellations.
3. Updates are merged with existing order state before re-mapping (see State Reconciliation).

## Symbol Format

### XBT vs BTC

BitMEX uses `XBT` internally instead of `BTC`. The `formatCoin()` method converts `XBT` -> `BTC` for display:

```typescript
private formatCoin(coin: string) {
  return coin.replace('XBT', 'BTC');
}
```

### Market ID Format

Internal market IDs follow the pattern: `{base}/{quote}:{margin}` where base/quote have XBT replaced with BTC.

- Example: `XBTUSD` instrument -> `BTC/USD:BTC` market ID
- Example: `XBTUSDT` instrument -> `BTC/USDT:USDT` market ID

### Market Filtering (`fetchMarketsAndTickers`)

Only instruments matching `underlying + quoteCurrency === symbol` are included, plus:

- Non-inverse instruments, OR
- `XBTUSD`, `XBTUSDT`, and `XBTEUR` specifically (inverse exceptions)

## Data Precision & Conversion

### Currency Precision (`fetchCurrencies`)

Fetched from `GET /wallet/assets` at startup. Each currency has:

- `scale`: Integer parsed via `parsePrecision()` -- e.g., scale `8` becomes precision string `"0.00000001"`.
- `majorCurrency`: Mapping to display currency (e.g., `XBt` -> `XBT`).

`convertToRealAmount(currency, amount)` multiplies amount by the precision value:

```typescript
private convertToRealAmount(currency: string | number, amount: string | number) {
  const precision = this.currencies[currency].precision!;
  return multiply(+amount, +precision);
}
```

### Micro Value Conversion

BitMEX uses internal "micro" representations for quantities. The exchange sets:

- `convertInputsToMicro = true` -- Multiply amounts by `underlyingToPositionMultiplier` or `underlyingToSettleMultiplier` before sending.
- `convertOutputsFromMicro = true` -- Divide amounts by the same multiplier when receiving.

**Exception**: Quanto and inverse contracts pass through values unchanged in `convertFromMicroValue()` / `convertToMicroValue()`.

```typescript
// convertFromMicroValue: divide by multiplier
// convertToMicroValue: multiply by multiplier
// Both skip conversion for isQuanto || isInverse
```

### Amount Precision

- **Linear contracts**: `amountPrecision = (1 / multiplier) * lotSize`
- **Quanto/Inverse contracts**: `amountPrecision = 1 * lotSize` (no multiplier)
- Price precision: `tickSize` from instrument data.

### Quanto Contract Sizing (`fetchMarketsAndTickers`)

For quanto instruments (e.g., `ETHUSD`), `contractSize` is computed dynamically:

```
multiplier_btc = instrument.multiplier / 100_000_000 (SATOSHI_PER_BTC)
contractValueInBtc = multiplier_btc * markPrice
contractValueInUsd = contractValueInBtc * btcPrice
contractSize = contractValueInUsd / markPrice
```

Safety check: if `contractSize <= 0`, defaults to `1`.

Tealstreet keeps normalized `Position.contracts`, `Order.amount`, `Order.filled`, and `Order.remaining` in base units (ETH for `ETHUSD`), while BitMEX `orderQty`, `currentQty`, and `leavesQty` are native contract counts. For quanto markets, expose `market.precision.amount` as the base value of one native lot (`contractSize * lotSize`) so app-side sizing rounds in ETH, then convert base size to native contracts before create/amend (`baseAmount / contractSize`, rounded to BitMEX `lotSize`) and convert native contracts back to base when mapping positions/orders. Do not send the base ETH amount directly as `orderQty`; `ETHUSD` has `lotSize: 1`, so a valid one-contract ETH size would round to 0 and omit `orderQty` if converted in the wrong unit.

### Inverse Contract Handling

For inverse contracts (e.g., `XBTUSD`):

- `contracts` (raw qty from `currentQty`) represents USD notional.
- `contractsInBase` is derived as `contracts / referencePrice` (using mark or last price from ticker).
- `notional` is the raw contract value (USD).

### Satoshi Conversion

- `SATOSHI_PER_BTC = 100_000_000` is exported from `bitmex.exchange.ts`.
- `convertToUSD(amount)` multiplies BTC by the `XBTUSDT` ticker's last price.
- `convertToBTC(amount)` divides USD by the `XBTUSDT` ticker's last price.

### Price Rounding

- `adjust()` from `safe-math` is used for rounding prices and amounts to precision.
- Market orders omit the `price` field entirely.
- All price values are sent as strings in order payloads (template literal `\`${price}\``).

## Rate Limiting

- **`xhr`**: Rate-limited to `maxRPS: 3` (3 requests per second) via `axios-rate-limit`.
- **`unlimitedXHR`**: No local rate limit; used for order placement (`placeOrderBatch`) and amendments (`updateOrder`).
- **Retry**: 3 retries on network errors via `axios-retry` with the `isNetworkError` condition.
- **Recv window**: `RECV_WINDOW` is 20,000ms and can be overridden via `options.extra.recvWindow`.
- **Order batching**: `placeOrderBatch()` sends groups of 3 concurrent requests.
- **Cancel batching**: `cancelOrders()` chunks groups of 10 order IDs.
- **HTTP-over-WebSocket**: `createAPI()` installs the HOW adapter through `setInnerAdapterForRemoteSigning()`, so hosted remote signing remains the outer adapter.

## State Reconciliation

### Order State Merging (`BitmexPrivateWebsocket.handleOrder`)

BitMEX WebSocket sends partial updates (not full order objects). The implementation merges incoming updates with the previous order state:

1. Build a map of incoming order updates keyed by `orderID`.
2. For each existing order in the store, if an update exists, merge: `{ ...previousBitmexOrder, ...incomingUpdate }`.
3. The merged objects are then mapped via `mapOrder()` and passed to `store.addOrUpdateOrders()`.

This ensures fields not included in the update (e.g., `symbol`, `side`, `orderQty`) are preserved from the previous state.

### Position State Merging (`BitmexPrivateWebsocket.handlePosition`)

Same merge pattern as orders:

1. Filter incoming positions by `filterBitmexPosition()` (only `XBt`/`USDt` currencies, must have matching market).
2. Build map keyed by `symbol`.
3. Merge with previous position state: `{ ...previousBitmexPosition, ...incomingUpdate }`.
4. Map via `mapPosition()` and update store with `addOrUpdatePositions()`.

### Balance State Merging (`BitmexPrivateWebsocket.onMessage`)

Margin/balance updates are merged with the existing balance snapshot:

1. Start with `this.store.balance?.info` (the raw snapshot array from last full fetch or previous merge).
2. For each currency update, find matching entry by `currency` field and merge, or append if new.
3. The merged array is re-parsed via `parseBalanceSnapshot()`.

### REST vs WebSocket Data

- **Balance and positions**: Fetched via REST in `tick()` loop (every `tickInterval`, default 20s). Also updated in real-time via private WS.
- **Orders**: NOT fetched via REST during normal operation (commented out in `_startPrivate`). Received exclusively via WebSocket `partial` action on connect and subsequent `insert`/`update`/`delete` actions.
- **On reconnect**: `fetchAndSetPrivateSnapshot()` (inherited from `BaseExchange`) re-fetches orders via REST to ensure no orders were missed during disconnection.

## Exchange Metadata

From `exchangeMetadata.ts`:

| Property                          | Value                   |
| --------------------------------- | ----------------------- |
| `supportsTestnet`                 | `false`                 |
| `orderbookTickDelay`              | 60ms                    |
| `tickInterval`                    | 20,000ms                |
| `orderPendingUpdateDelay`         | 2,000ms                 |
| `stopOrderPendingUpdateDelay`     | 2,000ms                 |
| `leverageIsGlobal`                | `true`                  |
| `tvIntraDayMultipliers`           | `['1', '5', '60']`      |
| `supportsChaser`                  | `true`                  |
| `supportsMaxCrossLeverage`        | `false`                 |
| `positionModes`                   | `[PositionMode.OneWay]` |
| `unifiedMargin`                   | `false`                 |
| `enforceWsTunnel`                 | `true`                  |
| `requiresCors`                    | `true`                  |
| `requiresTickersForMarketsLoaded` | `true`                  |
| `latencyDomain`                   | `www.bitmex.com`        |

## Testnet / Demo

### URL Differences

| Purpose            | Livenet                                                                                             | Testnet                                                                                                 |
| ------------------ | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| REST API           | `https://api.direct.bitmex.com/api/v1`                                                              | `https://testnet.bitmex.com/api/v1`                                                                     |
| UDF API            | `https://www.bitmex.com/api`                                                                        | `https://testnet.bitmex.com/api`                                                                        |
| Public WS          | `wss://ws.bitmex.com/realtime`                                                                      | `wss://ws.testnet.bitmex.com/realtime`                                                                  |
| Private WS         | `wss://ws.bitmex.com/realtime`                                                                      | `wss://ws.testnet.bitmex.com/realtime`                                                                  |
| Private WS (proxy) | `wss://fsjnpy143pbz.{jp,asia}.tealstreet.io/bitmex/private` — chosen by `options.proxyRegion`       | N/A                                                                                                     |
| OHLCV Proxy        | `https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy/https://www.bitmex.com/api/udf/history` | `https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy/https://testnet.bitmex.com/api/udf/history` |

### Known Differences

- Livenet REST uses `api.direct.bitmex.com` (direct API) while UDF uses `www.bitmex.com`.
- Testnet uses `testnet.bitmex.com` for all endpoints (REST, UDF, WS).
- Private WS proxy is only available for livenet.
- Despite having testnet URLs configured in code, `supportsTestnet` is `false` in exchange metadata, meaning the UI likely does not expose testnet as an option.

## Regional routing — old keys on Singapore, new keys on Tokyo

BitMEX is the rare exchange where **the user-side and BitMEX-side IP
allowlists both load-bear on the proxy region**, and **switching an
existing account's region silently breaks it**. The current setup:

- **`BitMEXMetadata.privateProxyRegions`** in `exchangeMetadata.ts`
  lists `JA` first, so new accounts default to Tokyo via the
  account-form's `Object.keys(...)[0]` rule.
- **Account-level `proxyRegion`** is plumbed through three places
  that all check `options.proxyRegion === 'JA'`:
  1. CORS proxy URL (`safeCexAxiosCreate` calls `getPrivateProxy(..., forceJa)`).
  2. HOW WS adapter region (`bitmex.api.ts` — creates a JA-pinned
     `axiosHttpOverWebsocketAdapter` for JA accounts; otherwise
     uses the default-region adapter, which falls back to SG for
     BitMEX URLs because `api.direct.bitmex.com` / `www.bitmex.com`
     aren't in `TOKYO_EGRESS_PATTERN`).
  3. Private WS host (`BitmexPrivateWebsocket.connectAndSubscribe` reads
     `resolveExchangeRouting(name, options).wsPrivate`).
- **Old accounts** were stamped with `proxyRegion: 'SG'` by the
  account-form's default-fallback path (`Object.keys(...)[0] || 'SG'`) back when BitMEX had no
  `privateProxyRegions` — so they keep routing via Singapore
  without any migration.

### Why `'JA'` cannot be flipped on existing accounts

The user's BitMEX API key has a per-key IP allowlist that contains
the **Singapore** HAProxy egress IPs from
`@tealstreet/proxy-ips-lists` → `privateProxyIpsSingapore`. The
Tokyo private proxy pool (`privateProxyIpsTokyo`) is a disjoint set
of 5 EIPs, and JA WS tunnel outbound connections also use the
separate Tokyo K8s NAT gateway
`tokyoK8sNatGatewayIp = 18.176.98.246`. Switching `proxyRegion` on
an existing account to `'JA'` without the user re-whitelisting both
the Tokyo private proxy pool and the Tokyo WS/K8s NAT IP on BitMEX
produces signed requests or private WS auth flows that BitMEX can
reject — and the resulting auth errors don't always surface a clear
"wrong IP" message; they can look like a broken API key.

When editing an existing BitMEX account in the UI, the region
selector pre-populates from the stored value. **Do not** auto-flip
old accounts to JA in any migration; the user has to consciously
re-whitelist the Tokyo IPs on their BitMEX API key first.

### BitMEX-side enrollment (already done, but load-bearing)

`api.direct.bitmex.com` is a closed-list peering endpoint with
BitMEX's own network firewall in front. Singapore HAProxy IPs are
enrolled. Tokyo HAProxy IPs (`privateProxyIpsTokyo`) were enrolled
2026-05-11 via BitMEX support — without that enrollment, TCP SYNs
from Tokyo egress to `api.direct.bitmex.com` are silently dropped
(15 s timeout, 0 bytes). The Tokyo K8s NAT gateway
(`tokyoK8sNatGatewayIp`) is a distinct WS tunnel egress IP that users
must whitelist on their BitMEX API keys for JA private WS stability.
If we ever rotate or add a Tokyo EIP or NAT gateway, the new IP must
be sent to BitMEX support and added to the web account setup allowlist
before traffic from it will work reliably.

### Why a missing JA tunnel IP can look partially connected

BitMEX startup does not currently gate "ready to trade" on a proven
private WS auth success. `_startPrivate()` connects/subscribes the
private WS, starts `tick()`, and then logs readiness while public
market data and some authenticated REST paths may still be independently
working. The private WS auth path also sends auth and subscriptions
without waiting for an auth success frame first. If a BitMEX API key is
missing the JA tunnel egress IP, the account can therefore look partly
alive while private WS tables fail to arrive or reconnects churn.

HoW tunnel errors are another source of lost signal: tunnel replies can
carry structured exchange errors, so callers must preserve the exchange
message instead of coercing the object to `[object Object]`. That is
load-bearing for surfacing BitMEX IP allowlist failures to web/CLI
health and validation paths.

## Known Quirks / Implementation Notes

1. **XBT naming**: BitMEX uses `XBT` where most exchanges use `BTC`. All display values convert via `formatCoin()`.

2. **Satoshi-based values**: XBt balance amounts are in satoshis (1 BTC = 100,000,000 satoshis). The `convertToRealAmount` method handles scaling using precision from `/wallet/assets`.

3. **USD conversion**: XBt-denominated balances are converted to USD using the `XBTUSDT` ticker's last price for the combined balance total.

4. **Order text tagging**: All orders placed through Tealstreet include `text: 'Tealstreet'` for identification.

5. **Order bootstrap**: Normal startup relies on the WebSocket `order` partial snapshot. REST `fetchOrders()` is retained for reconnect snapshot replacement and must throw on failure rather than affirming stale orders.

6. **Order cancel chunking**: `cancelOrders()` sends at most 10 order IDs per `DELETE /order` request.

7. **Post-only variants**: `mapOrder()` treats `ParticipateDoNotInitiate`, `PostOnly`, and `PostOnlyReprice` as post-only and recognizes their cancellation text. Tealstreet continues to submit `ParticipateDoNotInitiate`.

8. **ReduceOnly rejection detection**: Canceled orders with text containing `"Canceled: Order had execInst of Close or ReduceOnly but current position is 0"` are tagged with `OrderRejectReason.ReduceOnly`.

9. **WS order state merging**: Private WebSocket merges incoming order updates with previous order state (not full replacement), since BitMEX sends partial updates.

10. **Position WS merging**: Same merge pattern for positions -- incoming updates are merged with previous state before re-parsing.

11. **StopMarket vs Stop**: Orders are placed with `ordType: 'StopMarket'` but received back as `ordType: 'Stop'`; `mapOrder()` maps the latter to `OrderType.StopLoss`.

12. **Fill side mapping**: Private execution updates use the shared `OrderSide` enum directly; there is no BingX type dependency.

13. **MAM (Multi-Asset Margin) override**: A `MAMUSd` row overrides the aggregate only when it carries non-zero collateral/PnL. Zero placeholder rows leave the per-asset aggregate intact.

14. **Position filtering**: `filterBitmexPosition()` accepts `XBt`, `USDt`, and `USD` settlement currencies and requires a matching market in the store.

15. **WS error suppression**: Private WS errors containing `"already"` (for example, duplicate subscription replies) are logged but not emitted as user-facing errors.

16. **Order-book view**: The subscription symbol is lowercase and carries an explicit `Aggregated` pool token. Incoming rows are matched against the canonical market symbol and restricted to Aggregated/legacy-unset pool values.

17. **Order book micro conversion exception**: Order-book sizes skip micro conversion for inverse and quanto instruments.

18. **Ticker mark fallback**: Public instrument updates prefer `markPrice` and fall back to `fairPrice`; the initial market snapshot uses `markPrice`.

19. **Fetch positions creates fake positions**: When `fetchPositions()` is called with specific symbols and some are absent, it creates zero-quantity positions with default metadata to satisfy symbol-scoped callers.

20. **Order values as strings**: Order quantities and prices are serialized as strings in request payloads.

21. **Account identity**: `getAccount()` requests `/user/margin?currency=all` and returns the documented numeric `account` field as a string. It fails clearly if no margin row carries that field.

22. **Execution history compatibility**: `fetchExecutionTradeHistory()` first calls `/execution/tradeHistory`; only a signature/not-found compatibility failure triggers one retry against `/user/executionHistory`. `fetchHistoricalFills()` normalizes trade rows and settlement-currency fees/PnL.

23. **Protected market orders**: `maxSlippagePct` is sent on Market, StopMarket, and MarketIfTouched payloads. The value is seeded from a recent order and refreshed from private `execution`/order data.

## Error Handling

### Error Parsing (`parseError`)

The `parseError` method checks multiple error response formats in priority order:

```typescript
err?.response?.data?.error?.message; // Standard BitMEX API error
err?.response?.data?.msg; // Alternative message field
err?.response?.error?.message; // Direct response error
err?.message; // JS Error object
err?.error?.message; // Nested error object
err?.error; // Raw error string
err; // Fallback to raw value
```

### Error-as-Success Patterns

- `fetchCurrencies()`: Catches all errors, returns `{}` and logs -- startup continues without currency data.
- `fetchBalance()`: Rethrows authentication errors; transient errors return the previous balance snapshot.
- `fetchOpenPositions()`: Rethrows authentication errors; transient errors return the previous position snapshot.
- `fetchOrders()`: **Throws on any error** (auth and transient alike) — it does NOT return stale orders. This is deliberate and load-bearing: `fetchOrders` only feeds wholesale store replacement (`fetchAndSetPrivateSnapshot` on reconnect, `fetchAndSetOrders`). Returning the existing set would replace the store with its own stale data — re-affirming phantom orders (canceled/filled rows the live WS already dropped), falsely marking orders loaded, and defeating the snapshot retry loop (which only retries when `fetchOrders` throws). A leftover phantom order combined with a broadening cancel was the TSBUG-103 vector. **Do not revert `fetchOrders` to the "return `this.store.orders` on transient error" pattern** the sibling fetchers use — orders are reconciled by replacement, positions/balance by merge, so the safe-fallback differs.
- `fetchMarketsAndTickers()`: On error, returns current store markets/tickers.

### Response Error Detection

Methods that consume signed REST responses check for `(data as any).error` because BitMEX can return an error-shaped body on a successful HTTP status.

### Retry Strategy

- REST: 3 retries on network errors only (not HTTP errors) via `axios-retry`.
- WebSocket: Automatic reconnection via `BaseWebSocket` on close or pong miss.
- Tick loop: Errors in `tick()` are caught and logged, then the loop continues.

### Order Cancellation Error Aggregation

`cancelOrders()` collects errors from individual cancel responses and throws one comma-joined error at the end.

## Balance Handling

### Multi-Currency Balance

BitMEX supports multiple settlement currencies. Balance is fetched from `/user/margin?currency=all` and parsed per currency:

- **`USDt`**: Values converted using currency scale (precision from `/wallet/assets`).
- **`XBt`**: Values converted to satoshis then to USD via `XBTUSDT` ticker price.
- **`MAMUSd`** (Multi-Asset Margin): If present, overrides the combined balance with MAM-specific values (converted using USDt scale).

### Per-Currency Assets

Individual currency balances are stored in `balance.currencies` keyed by the major currency name (after `formatCoin` conversion). Each contains `total`, `free`, `upnl`, and `used`.

### Used Margin Calculation

Used margin is `max(marginBalance - availableMargin, initMargin, 0)` in each settlement currency before conversion. This avoids the drift and edge cases of deriving it from `marginUsedPcnt`.

## Position Handling

### Position Mode

BitMEX only supports **one-way mode** (`PositionMode.OneWay`). Hedge mode is explicitly disabled:

```typescript
this.store.setSetting('isHedged', false);
```

### Position Side

Determined by `currentQty`: positive = Long, negative = Short.

### Margin Type

Read from `crossMargin` field: `true` = Cross, `false` = Isolated. Changeable via `POST /position/isolate`.

### Leverage

- Read from position data.
- Set via `POST /position/leverage` with `{ symbol, leverage }`.
- Clamped to market min/max: `min = 1`, `max = 1 / initMargin`.
- Only sends a request if leverage actually changed.
- Response is mapped back to a position and stored.

### Extra Fields

Positions include `extra.contractsNative` (pre-conversion quantity) and `extra.unrealizedPnlNative` (PnL in BTC for XBt positions).

## OHLCV / Candlestick Data

### Supported Timeframes

| Interval | Source |
| -------- | ------ |
| `1m`, `5m`, `1h`, `1d` | Native UDF resolutions `1`, `5`, `60`, `1D` |
| `3m` | Aggregated from `1m` |
| `15m`, `30m` | Aggregated from `5m` |
| `2h`, `4h`, `6h`, `12h` | Aggregated from `1h` |
| `1w` | Aggregated from `1d` |

### Fetching (`fetchOHLCV`)

Uses a **proxy endpoint** rather than direct BitMEX API:

- Livenet: `https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy/https://www.bitmex.com/api/udf/history`
- Testnet: `https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy/https://testnet.bitmex.com/api/udf/history`

Uses native `fetch()` (not axios) for the proxy request. Native-source requests are capped at 1500 candles.

Response is in TradingView UDF format: `{ t: [], o: [], h: [], l: [], c: [], v: [] }`. `fetchOHLCV()` validates the response structure before processing.

### Live Updates (`listenOHLCV`)

Supported native and derived intervals use the public `trade:{symbol}` WebSocket to update the current candle. A 5-second poll seeds the full current bucket (trades may have occurred before subscription), and a 12-second staleness watchdog re-enables polling if live trades stop. Candle timestamps never regress and same-bucket volume is monotonic. Only intervals with no `getBitmexOHLCVPlan()` fall back to polling-only behavior.

## Grafana Monitoring

Endpoints are classified for monitoring:

- **Every-request tracking**: `ORDER`, `SET_LEVERAGE`
- **Periodic tracking**: `BALANCE`, `ACCOUNT`, `POSITIONS`, `WALLET_ASSETS`

## Testing / Validation

### Running Checks

Run from `packages/safe-cex`:

```bash
yarn lint
yarn typecheck
yarn test-ci
```
