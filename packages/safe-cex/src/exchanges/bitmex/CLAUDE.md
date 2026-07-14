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

## API Documentation

- **Official API Documentation**: https://www.bitmex.com/api/explorer/
- **CCXT REST Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/bitmex.ts
- **CCXT WebSocket Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/bitmex.ts

## File Structure

| File                             | Description                                                                                                                                                                                        |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bitmex.exchange.ts`             | Main exchange class (`BitmexExchange`) extending `BaseExchange`. Contains all REST trading operations, balance/position parsing, order formatting, and OHLCV fetching.                             |
| `bitmex.api.ts`                  | Axios HTTP client factory with HMAC-SHA256 authentication interceptor, retry logic, and HTTP-over-WebSocket adapter.                                                                               |
| `bitmex.types.ts`                | Constants (URLs, endpoints, recv window), status/side mappings, `Currency` type, and Grafana endpoint lists.                                                                                       |
| `bitmex.ws-private.ts`           | Private WebSocket (`BitmexPrivateWebsocket`): handles authenticated streams for orders, positions, and margin/balance updates.                                                                     |
| `bitmex.ws-public.ts`            | Public WebSocket (`BitmexPublicWebsocket`): handles instrument (ticker) updates, order book L2 streams, and OHLCV polling.                                                                         |
| `bitmex.ohlcv-helpers.ts`        | Shared OHLCV helpers used by `bitmex.exchange.ts` (timeframe/resolution mapping, UDF response validation, candle dedup).                                                                           |
| `bitmextestnet.exchange.ts`      | Thin subclass (`BitmexTestnetExchange`) that overrides URLs/WS hosts to point at the BitMEX testnet. Registered separately in the exchange registry.                                               |
| `types.ts`                       | Shared BitMEX-specific types: `BITMEX_ORDER`, `BITMEX_POSITION`, `BITMEX_MARKET`, WS update types (`ORDER_WS_UPDATE`, `POSITION_WS_UPDATE`, `TRADE_WS_UPDATE`), and `SupportedTimeframes` mapping. |

## Authentication

### REST API (`bitmex.api.ts`)

Uses HMAC-SHA256 signing. The interceptor computes signatures on every request:

- **Headers**:
  - `api-key`: The API key from `options.key`
  - `api-expires`: A nonce generated from `virtualClock.getCurrentTimeMs() * 1000 + counter`
  - `api-signature`: HMAC-SHA256 hex digest
- **Signature payload**: `METHOD + "/api/v1" + URL_WITH_PARAMS + NONCE + [BODY_JSON]`
  - Example: `"GET/api/v1/position1709472000000000"`
  - Body JSON is appended only for requests with a body (POST/PUT)
- **Nonce generation**: Microsecond timestamp + incrementing counter (mod 1000) to avoid collisions (`bitmex.api.ts:14-16`)
  ```typescript
  const generateNonce = () => +virtualClock.getCurrentTimeMs() * 1000 + (nonceCounter++ % 1000);
  ```
- **Module-level counter**: The `nonceCounter` is a module-level variable (not per-instance), so it persists across API client recreations. The `% 1000` modulo prevents overflow while still providing uniqueness within the same millisecond.
- **Query string serialization**: Uses `qs.stringify` with `arrayFormat: 'repeat'` for params (`bitmex.api.ts:24`).

### WebSocket Authentication (`bitmex.ws-private.ts`)

- Uses `authKeyExpires` operation
- Signature payload: `"GET" + "/realtime" + TIMESTAMP` signed with HMAC-SHA256 (see `sendAuth`)
- Sends: `{ op: "authKeyExpires", args: [apiKey, timestamp, signature] }`
- Re-authenticates every 30 minutes via `setTimeout` (`WS_AUTH_REFRESH_MS`)
- **Subscribe is sent once per socket connection, not per auth.** On open, auth + subscribe are sent immediately (subscription follows auth without waiting for a response). The 30-min refresh calls `sendAuth()` with `subscribe` defaulting `false`, so it re-signs only. A `privateSubscriptionsSent` flag (reset in `onOpen`) gates the subscribe in **both** `sendAuth` branches (HMAC and `remoteSigner`). Re-sending `subscribe` on the still-connected socket triggered BitMEX "already subscribed to topic:…" replies every 30 min — suppressed from users (see quirk #15) but log-noisy, and repeated WS command errors can eventually get the socket dropped. Don't move the subscribe back into the unconditional auth path.

### UDF Endpoints

Requests to `/udf/*` paths skip authentication and use a different base URL (`www.bitmex.com` instead of `api.direct.bitmex.com`). The interceptor detects UDF paths and strips all auth headers, returning only `Content-Type` (`bitmex.api.ts:49-57`).

## Startup Sequence

The `start()` method (`bitmex.exchange.ts:109-143`) executes the following sequence:

### Phase 1: Currency and Market Data (sequential)

1. **Fetch currencies** -- `fetchCurrencies()` calls `GET /wallet/assets` to get currency `scale` and `majorCurrency` mappings. Stored in `this.currencies`. Errors are caught and return `{}` (non-fatal).
2. **Disposal check** -- If disposed during fetch, return early.
3. **Fetch markets and tickers** -- `fetchMarketsAndTickers()` calls `GET /instrument/activeAndIndices`. Parses both markets and tickers from the same response. BTC price is extracted from the `.BXBT` index instrument for quanto contract sizing.
4. **Disposal check** -- If disposed during fetch, return early.
5. **Store updates** -- Markets, tickers, and empty orders (with loaded flags) are set in the store.

### Phase 2: WebSocket Connection

6. **Public WebSocket** -- `publicWebsocket.connectAndSubscribe()` connects and subscribes to `instrument:CONTRACTS`.

### Phase 3: Private or Public-Only Mode

**Private mode** (`_startPrivate`, `bitmex.exchange.ts:145-168`):

1. Connect private WebSocket, authenticate, subscribe to private channels.
2. Set `isHedged = false`.
3. Run initial `tick()` -- fetches balance (`GET /user/margin?currency=all`) then positions (`GET /position`) sequentially.
4. After tick completes, begin tick loop (interval from `options.extra.tickInterval`, default 20s).
5. Note: Order fetching via REST is commented out (`bitmex.exchange.ts:158-167`). Orders are received via WebSocket `partial` action on connect instead.

**Public-data-only mode** (`_startPublicDataOnly`, `bitmex.exchange.ts:170-187`):

1. Sets empty orders, zero-balance, empty positions with all loaded flags set to true.

### Tick Loop

The `tick()` method (`bitmex.exchange.ts:189-213`) runs on an interval:

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

Note: `TICKERS` is defined as `/instrument/activeAndIndices` (same as `MARKETS`) but not used separately.

## WebSocket Implementation

### Public WebSocket (`bitmex.ws-public.ts`)

- **URL**: `wss://ws.bitmex.com/realtime` (livenet) / `wss://ws.testnet.bitmex.com/realtime` (testnet)
- **Subscriptions**:
  - `instrument:CONTRACTS` -- Ticker/instrument updates for all contracts
  - `orderBookL2:{symbol}` -- L2 order book (subscribed per-symbol on demand, lowercase symbol)
- **Ping**: Sends literal string `"ping"`, expects `"pong"` response (`bitmex.ws-public.ts:57-59`)
- **Pong handling**: Calls `handlePongEvent()` from `BaseWebSocket` which schedules next ping in 5 seconds and sets a pong miss timeout at 7.5 seconds (1.5x ping interval) that triggers reconnection.
- **OHLCV**: Not via WebSocket; uses polling (`listenOHLCV` polls `fetchOHLCV` every 5 seconds)
- **Reconnection**: On open, calls `resetOrderbook()` to clear cached order book state, re-subscribes to all active topics via `subscribe()`, and emits `wsReconnected` event (when `ENABLE_RECONNECTING_WEBSOCKET` is enabled).
- **Message routing**: `instrument` table messages are handled inline for ticker updates. All other messages are routed through `messageHandlers` map by matching the table name against topic prefixes (`bitmex.ws-public.ts:97-107`).
- **Topic reference counting**: `this.topics` tracks subscriber count per topic. Topics are unsubscribed only when count reaches zero (`bitmex.ws-public.ts:185-192`).

### Private WebSocket (`bitmex.ws-private.ts`)

- **URL**: Same as public (`wss://ws.bitmex.com/realtime`), but authenticated
- **Proxy URL**: depends on `options.proxyRegion` — `JA` → `wss://fsjnpy143pbz.jp.tealstreet.io/bitmex/private`, otherwise `wss://fsjnpy143pbz.asia.tealstreet.io/bitmex/private` (see "Regional routing" below)
- **Subscriptions** (sent immediately after auth, same message):
  - `execution` -- Trade executions
  - `margin` -- Balance/margin updates
  - `csastate` -- CSA (Cross-Settle Account) state
  - `order` -- Order lifecycle updates (insert, update, partial, delete)
  - `position` -- Position updates
- **Ping**: Sends literal string `"ping"` via `sendWithLog`, same pattern as public WS.
- **Reconnection**: On open, sends auth, starts ping, and if `ENABLE_RECONNECTING_WEBSOCKET` is true, calls `this.parent.fetchAndSetPrivateSnapshot()` to re-fetch orders from REST (`bitmex.ws-private.ts:34-36`).

### Reconnection Logic (inherited from `BaseWebSocket`)

Both WebSockets inherit reconnection behavior from `BaseWebSocket`:

- **Pong miss timeout**: If no pong received within 7.5 seconds after ping, triggers `reconnect()`.
- **Last received check**: `trackLastReceived()` monitors message freshness. If no messages for `WS_RECONNECT_THRESHOLD_MS` (20 seconds), triggers reconnection.
- **Reconnection delay**: `WS_MIN_RECONNECTION_DELAY` = 1000 + random(0-2000)ms (jittered).
- **Max reconnection delay**: 5000ms.
- **Close count guard**: After 100 consecutive onClose events, stops attempting reconnection (`base.ws.ts:398-400`).
- **WebSocket type**: Currently uses native `WebSocket` (`ENABLE_RECONNECTING_WEBSOCKET = false`), not `ReconnectingWebSocket`.

### Handled Tables (Private WS)

| Table      | Handler               | Description                                                                 |
| ---------- | --------------------- | --------------------------------------------------------------------------- |
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

Comma-separated values appended to orders (`bitmex.exchange.ts:1075-1103`):

- `ReduceOnly` -- For reduce-only Market/Limit orders
- `Close` -- For reduce-only Stop/TP orders
- `ParticipateDoNotInitiate` -- Post-only for Limit orders
- `MarkPrice` / `IndexPrice` / `LastPrice` -- Trigger price type for Stop/TP orders

### Order Status Mapping (`bitmex.types.ts:47-60`)

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

### Order Placement (`bitmex.exchange.ts:1020-1160`)

1. `placeOrder()` / `placeOrders()` calls `formatCreateOrder()` to build payloads.
2. Payloads are sent individually (not `/order/bulk`) but batched in groups of 3 via `Promise.all` in `placeOrderBatch()`.
3. Each batch waits for completion before the next.
4. Uses `unlimitedXHR` (no rate limit) for `POST /order`.
5. All orders include `text: 'Tealstreet'` for identification.
6. If any order in a batch fails, the error is thrown after the batch completes (last error wins).

### Bracket Orders (TP/SL with Entry)

When `opts.stopLoss` or `opts.takeProfit` is provided on a `placeOrder` call (`bitmex.exchange.ts:1112-1132`):

- The SL/TP orders are appended to the same payloads array as the entry order.
- Side is flipped (Buy -> Sell, Sell -> Buy).
- SL uses `ordType: 'StopMarket'` with `execInst: 'ReduceOnly'`.
- TP uses `ordType: 'MarketIfTouched'` with `execInst: 'ReduceOnly'`.
- The `price` field is omitted from SL/TP payloads (only `stopPx` is set).
- All orders (entry + brackets) go through the same batch mechanism.

### Order Amendment (`bitmex.exchange.ts:976-1018`)

Uses native `PUT /order` with `orderID` and updated fields:

- For stop/TP orders: price goes to `stopPx` field.
- For limit orders: price goes to `price` field.
- Amount amendments use `orderQty` with micro-value conversion applied.
- Uses `unlimitedXHR` (bypasses rate limiter).
- Returns `[order.id]` on success.

### Order Cancellation (`bitmex.exchange.ts:910-974`)

**Cancel specific orders** (`cancelOrders`):

- Groups orders by symbol.
- Single orders: `DELETE /order` with `orderID` as param.
- Multiple orders per symbol: chunked into batches of 10. Each chunk sends `orderID` as JSON array string.
- Processes sequentially via `forEachSeries` per symbol group.
- Collects errors from responses and throws aggregated error message.

**Cancel all orders for symbol** (`cancelSymbolOrders`):

- `DELETE /order` with `symbol` param.
- Updates store to mark all matching orders as Canceled.

### Order Updates via WebSocket (`bitmex.ws-private.ts:40-110`)

1. On `partial` action: orders are processed as-is (initial snapshot on connect).
2. On `insert`/`update`/`delete` actions:
   - Fill events emitted for `Filled` or `PartiallyFilled` status.
   - Warning events emitted for PostOnly and ReduceOnly cancellations.
3. Updates are merged with existing order state before re-mapping (see State Reconciliation).

## Symbol Format

### XBT vs BTC

BitMEX uses `XBT` internally instead of `BTC`. The `formatCoin()` method converts `XBT` -> `BTC` for display (`bitmex.exchange.ts:1180-1182`):

```typescript
private formatCoin(coin: string) {
  return coin.replace('XBT', 'BTC');
}
```

### Market ID Format

Internal market IDs follow the pattern: `{base}/{quote}:{margin}` where base/quote have XBT replaced with BTC.

- Example: `XBTUSD` instrument -> `BTC/USD:BTC` market ID
- Example: `XBTUSDT` instrument -> `BTC/USDT:USDT` market ID

### Market Filtering (`bitmex.exchange.ts:370-374`)

Only instruments matching `underlying + quoteCurrency === symbol` are included, plus:

- Non-inverse instruments, OR
- `XBTUSD` and `XBTUSDT` specifically (inverse exceptions)

## Data Precision & Conversion

### Currency Precision (`bitmex.exchange.ts:215-231`)

Fetched from `GET /wallet/assets` at startup. Each currency has:

- `scale`: Integer parsed via `parsePrecision()` -- e.g., scale `8` becomes precision string `"0.00000001"`.
- `majorCurrency`: Mapping to display currency (e.g., `XBt` -> `XBT`).

`convertToRealAmount(currency, amount)` multiplies amount by the precision value (`bitmex.exchange.ts:1167-1170`):

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

**Exception**: Quanto and inverse contracts pass through values unchanged (`bitmex.exchange.ts:584-601`).

```typescript
// convertFromMicroValue: divide by multiplier
// convertToMicroValue: multiply by multiplier
// Both skip conversion for isQuanto || isInverse
```

### Amount Precision

- **Linear contracts**: `amountPrecision = (1 / multiplier) * lotSize` (`bitmex.exchange.ts:383-389`)
- **Quanto/Inverse contracts**: `amountPrecision = 1 * lotSize` (no multiplier)
- Price precision: `tickSize` from instrument data.

### Quanto Contract Sizing (`bitmex.exchange.ts:391-401`)

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

- `SATOSHI_PER_BTC = 100_000_000` (exported constant, `bitmex.exchange.ts:58`)
- `convertToUSD(amount)`: Multiplies BTC amount by `XBTUSDT` ticker's last price (`bitmex.exchange.ts:1172-1174`).
- `convertToBTC(amount)`: Divides USD amount by `XBTUSDT` ticker's last price (`bitmex.exchange.ts:1176-1178`).

### Price Rounding

- `adjust()` from `safe-math` is used for rounding prices and amounts to precision.
- Market orders omit `price` field entirely (`bitmex.exchange.ts:1039`).
- All price values are sent as strings in order payloads (template literal `\`${price}\``).

## Rate Limiting

- **`xhr`**: Rate-limited to `maxRPS: 3` (3 requests per second) via `axios-rate-limit` (`bitmex.exchange.ts:78`).
- **`unlimitedXHR`**: No rate limit, used for order placement (`placeOrderBatch`) and order amendments (`updateOrder`) (`bitmex.exchange.ts:79`).
- **Retry**: 3 retries on network errors via `axios-retry` with `isNetworkError` condition (`bitmex.api.ts:38`).
- **Recv window**: 20,000ms timeout (`RECV_WINDOW`, `bitmex.types.ts:3`). Can be overridden via `options.extra.recvWindow`.
- **Order batching**: Orders placed in groups of 3 concurrent requests (`bitmex.exchange.ts:1136`).
- **Cancel batching**: Cancels chunked in groups of 10 order IDs per request (`bitmex.exchange.ts:932`).
- **HTTP-over-WebSocket**: The axios adapter is set to `axiosHttpOverWebsocketAdapter` (`bitmex.api.ts:35`), which may route REST calls over WebSocket when the WS tunnel is active.

## State Reconciliation

### Order State Merging (`bitmex.ws-private.ts:96-109`)

BitMEX WebSocket sends partial updates (not full order objects). The implementation merges incoming updates with the previous order state:

1. Build a map of incoming order updates keyed by `orderID`.
2. For each existing order in the store, if an update exists, merge: `{ ...previousBitmexOrder, ...incomingUpdate }`.
3. The merged objects are then mapped via `mapOrder()` and passed to `store.addOrUpdateOrders()`.

This ensures fields not included in the update (e.g., `symbol`, `side`, `orderQty`) are preserved from the previous state.

### Position State Merging (`bitmex.ws-private.ts:112-138`)

Same merge pattern as orders:

1. Filter incoming positions by `filterBitmexPosition()` (only `XBt`/`USDt` currencies, must have matching market).
2. Build map keyed by `symbol`.
3. Merge with previous position state: `{ ...previousBitmexPosition, ...incomingUpdate }`.
4. Map via `mapPosition()` and update store with `addOrUpdatePositions()`.

### Balance State Merging (`bitmex.ws-private.ts:367-385`)

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
| `supportsTradesHistory`           | `true`                  |
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
| OHLCV Proxy        | `https://read-fleet-jp.tealstreet.io/api/exchange_proxy/https://www.bitmex.com/api/udf/history` | `https://read-fleet-jp.tealstreet.io/api/exchange_proxy/https://testnet.bitmex.com/api/udf/history` |

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
  1. CORS proxy URL (`safeCexAxiosCreate.ts:148`,
     `getPrivateProxy(..., forceJa)`).
  2. HOW WS adapter region (`bitmex.api.ts` — creates a JA-pinned
     `axiosHttpOverWebsocketAdapter` for JA accounts; otherwise
     uses the default-region adapter, which falls back to SG for
     BitMEX URLs because `api.direct.bitmex.com` / `www.bitmex.com`
     aren't in `TOKYO_EGRESS_PATTERN`).
  3. Private WS host (`bitmex.ws-private.ts` —
     `wss://fsjnpy143pbz.{jp,asia}.tealstreet.io/bitmex/private`).
- **Old accounts** were stamped with `proxyRegion: 'SG'` by the
  account-form's default-fallback path (`useAccountForm.ts:120-122`,
  `Object.keys(...)[0] || 'SG'`) back when BitMEX had no
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

5. **Commented-out order fetching**: The `_startPrivate` method has order fetching commented out (`bitmex.exchange.ts:158-167`). The comment says "Binance" but this is BitMEX code -- likely a copy-paste artifact. Orders are received via WebSocket `partial` action on connect instead.

6. **Order cancel chunking**: When canceling multiple orders, they are chunked into batches of 10 per `DELETE /order` request (`bitmex.exchange.ts:932`).

7. **PostOnly rejection detection**: Canceled orders with text containing `"Canceled: Order had execInst of ParticipateDoNotInitiate"` are tagged with `OrderRejectReason.PostOnly`.

8. **ReduceOnly rejection detection**: Canceled orders with text containing `"Canceled: Order had execInst of Close or ReduceOnly but current position is 0"` are tagged with `OrderRejectReason.ReduceOnly`.

9. **WS order state merging**: Private WebSocket merges incoming order updates with previous order state (not full replacement), since BitMEX sends partial updates.

10. **Position WS merging**: Same merge pattern for positions -- incoming updates are merged with previous state before re-parsing.

11. **StopMarket vs Stop**: Orders are placed with `ordType: 'StopMarket'` but received back as `ordType: 'Stop'`. The `mapOrder` function handles `'Stop'` for mapping to `OrderType.StopLoss` (`bitmex.exchange.ts:706`).

12. **ORDER_SIDE import**: `bitmex.ws-private.ts` imports `ORDER_SIDE` from `bingx.types` (not `bitmex.types`) for the fill event emission (`bitmex.ws-private.ts:10`). This is a cross-exchange dependency. The BingX `ORDER_SIDE` uses `BUY`/`SELL` string constants (for the fill event payload), while the BitMEX `ORDER_SIDE` maps `Buy`/`Sell` to `OrderSide` enum values.

13. **MAM (Multi-Asset Margin) override**: If a `MAMUSd` currency entry is found in the balance snapshot, it overrides the entire combined balance calculation. MAM values are converted using `USDt` scale (`bitmex.exchange.ts:344-356`).

14. **Position filtering**: Only positions with `currency` of `XBt` or `USDt` are processed. Additionally, the position must have a matching market in the store (`bitmex.exchange.ts:580-582`).

15. **WS error suppression**: Private WS errors containing `"already"` (e.g., "already subscribed") are logged as warnings but not emitted as error events (`bitmex.ws-private.ts:161-163`).

16. **Order book lowercase symbol**: The orderBookL2 topic uses lowercase symbol (`orderBookL2:${symbol.toLowerCase()}`), but the data is then filtered by exact symbol match (`bitmex.ws-public.ts:114,122`).

17. **Order book micro conversion exception**: Order book sizes skip micro conversion for inverse and quanto instruments, unlike other data (`bitmex.ws-public.ts:205-208`).

18. **Ticker field mapping inconsistency**: The public WS maps `fairPrice` to `mark` and `markPrice` to `mark` depending on context. The initial fetch uses `markPrice` while the WS update checks for `row.mark` and maps from `fairPrice` (`bitmex.ws-public.ts:91`).

19. **Fetch positions creates fake positions**: When `fetchPositions()` is called with specific symbols and some are not found in open positions, fake zero-quantity positions are created with default metadata (cross margin, max leverage) to satisfy the caller's expectations (`bitmex.exchange.ts:490-501`).

20. **Order amount as string**: Order quantities are sent as strings in the payload (`orderQty: amount ? \`${amount}\` : undefined`), not numbers (`bitmex.exchange.ts:1065`). Same for prices.

## Error Handling

### Error Parsing (`bitmex.exchange.ts:1184-1194`)

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

- `fetchCurrencies()`: Catches all errors, returns `{}` and logs -- startup continues without currency data (`bitmex.exchange.ts:226-230`).
- `fetchBalance()`: On error, returns `this.store.balance` (previous balance), does not throw (`bitmex.exchange.ts:298-301`).
- `fetchOpenPositions()`: On error, returns `this.store.positions` (previous positions), does not throw (`bitmex.exchange.ts:575-577`).
- `fetchOrders()`: **Throws on any error** (auth and transient alike) — it does NOT return stale orders. This is deliberate and load-bearing: `fetchOrders` only feeds wholesale store replacement (`fetchAndSetPrivateSnapshot` on reconnect, `fetchAndSetOrders`). Returning the existing set would replace the store with its own stale data — re-affirming phantom orders (canceled/filled rows the live WS already dropped), falsely marking orders loaded, and defeating the snapshot retry loop (which only retries when `fetchOrders` throws). A leftover phantom order combined with a broadening cancel was the TSBUG-103 vector. **Do not revert `fetchOrders` to the "return `this.store.orders` on transient error" pattern** the sibling fetchers use — orders are reconciled by replacement, positions/balance by merge, so the safe-fallback differs.
- `fetchMarketsAndTickers()`: On error, returns current store markets/tickers (`bitmex.exchange.ts:457-459`).

### Response Error Detection

Many methods check for `(data as any).error` on successful HTTP responses, since BitMEX can return 200 with error bodies (`bitmex.exchange.ts:218,295,365,511,691,874,898,964`).

### Retry Strategy

- REST: 3 retries on network errors only (not HTTP errors) via `axios-retry` (`bitmex.api.ts:38`).
- WebSocket: Automatic reconnection via `BaseWebSocket` on close or pong miss.
- Tick loop: Errors in `tick()` are caught and logged, then the loop continues (`bitmex.exchange.ts:207-210`).

### Order Cancellation Error Aggregation

`cancelOrders()` collects errors from individual cancel responses into an array and throws a single aggregated error message at the end, joined by commas (`bitmex.exchange.ts:948-949`).

## Balance Handling

### Multi-Currency Balance

BitMEX supports multiple settlement currencies. Balance is fetched from `/user/margin?currency=all` and parsed per currency:

- **`USDt`**: Values converted using currency scale (precision from `/wallet/assets`).
- **`XBt`**: Values converted to satoshis then to USD via `XBTUSDT` ticker price.
- **`MAMUSd`** (Multi-Asset Margin): If present, overrides the combined balance with MAM-specific values (converted using USDt scale).

### Per-Currency Assets

Individual currency balances are stored in `balance.currencies` keyed by the major currency name (after `formatCoin` conversion). Each contains `total`, `free`, `upnl`, and `used`.

### Used Margin Calculation

Used margin is computed as `availableMargin * marginUsedPcnt` (not directly from a dedicated field), which is an approximation (`bitmex.exchange.ts:321-323`).

## Position Handling

### Position Mode

BitMEX only supports **one-way mode** (`PositionMode.OneWay`). Hedge mode is explicitly disabled:

```typescript
this.store.setSetting('isHedged', false);
```

### Position Side

Determined by `currentQty`: positive = Long, negative = Short (`bitmex.exchange.ts:640`).

### Margin Type

Read from `crossMargin` field: `true` = Cross, `false` = Isolated. Changeable via `POST /position/isolate`.

### Leverage

- Read from position data.
- Set via `POST /position/leverage` with `{ symbol, leverage }`.
- Clamped to market min/max: `min = 1`, `max = 1 / initMargin` (`bitmex.exchange.ts:865`).
- Only sends request if leverage actually changed (`bitmex.exchange.ts:867`).
- Response is mapped back to a position and stored.

### Extra Fields

Positions include `extra.contractsNative` (pre-conversion quantity) and `extra.unrealizedPnlNative` (PnL in BTC for XBt positions).

## OHLCV / Candlestick Data

### Supported Timeframes

| Interval | BitMEX Resolution |
| -------- | ----------------- |
| `1m`     | `1`               |
| `5m`     | `5`               |
| `1h`     | `60`              |
| `1d`     | `1D`              |

### Fetching (`bitmex.exchange.ts:791-848`)

Uses a **proxy endpoint** rather than direct BitMEX API:

- Livenet: `https://read-fleet-jp.tealstreet.io/api/exchange_proxy/https://www.bitmex.com/api/udf/history`
- Testnet: `https://read-fleet-jp.tealstreet.io/api/exchange_proxy/https://testnet.bitmex.com/api/udf/history`

Uses native `fetch()` (not axios) for the proxy request. Max limit capped at 1500 candles (`bitmex.exchange.ts:798`).

Response is in TradingView UDF format: `{ t: [], o: [], h: [], l: [], c: [], v: [] }`. Validates response structure before processing (`bitmex.exchange.ts:827-830`).

### Live Updates (`bitmex.ws-public.ts:216-264`)

OHLCV live updates use **polling** (not WebSocket). `listenOHLCV` polls `fetchOHLCV` every 5 seconds and calls the callback only when the candle data changes (compares `timestamp`, `close`, and `volume`). Fetches the latest 2 candles per poll and uses the most recent one.

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
