# BingX Exchange Integration

This directory contains the BingX exchange integration for Tealstreet, targeting BingX Swap perpetual contracts (stablecoin-margined linear futures).

## Exchange Information

- **Pretty Name**: BingX
- **Exchange Name**: `bingx`
- **Status**: Enabled (`enabled: true`, `enabledInDev: true`)
- **Broker ID**: `Tealstreet` (sent as `X-SOURCE-KEY` header)

## API Documentation

- **BingX API Docs v3 (Swap Market Data - Symbols)**: [https://bingx-api.github.io/docs-v3/#/en/Swap/Market%20Data/USDT-M%20Perp%20Futures%20symbols](https://bingx-api.github.io/docs-v3/#/en/Swap/Market%20Data/USDT-M%20Perp%20Futures%20symbols)
- **BingX API Docs v3 (Swap Market Data - Recent Trades List)**: [https://bingx-api.github.io/docs-v3/#/en/Swap/Market%20Data/Recent%20Trades%20List](https://bingx-api.github.io/docs-v3/#/en/Swap/Market%20Data/Recent%20Trades%20List)
- **BingX API Docs v3 (Changelog)**: [https://bingx-api.github.io/docs-v3/#/en/changes/logs](https://bingx-api.github.io/docs-v3/#/en/changes/logs)
- **Legacy Swap V2 docs (still useful for parity checks)**: [https://bingx-api.github.io/docs/#/swapV2/introduce](https://bingx-api.github.io/docs/#/swapV2/introduce)
- **Legacy authentication guide**: [https://bingx-api.github.io/docs/#/swapV2/authentication.html](https://bingx-api.github.io/docs/#/swapV2/authentication.html)
- **Authoritative API reference repository**: [https://github.com/BingX-API/api-ai-skills](https://github.com/BingX-API/api-ai-skills) (`swap-*` skills target the perpetual Swap API)
- **CCXT REST Reference**: [https://github.com/ccxt/ccxt/blob/master/ts/src/bingx.ts](https://github.com/ccxt/ccxt/blob/master/ts/src/bingx.ts)
- **CCXT WebSocket Reference**: [https://github.com/ccxt/ccxt/blob/master/ts/src/pro/bingx.ts](https://github.com/ccxt/ccxt/blob/master/ts/src/pro/bingx.ts)

## v3 Changelog Alignment (2026-03 Review)

The following items are relevant to this integration:

1. **2026-01-22 changelog update**: BingX states OpenAPI now supports non-crypto trading pairs via `/openApi/swap/v2/quote/contracts`.
2. **Symbols endpoint remains the same path** (`/openApi/swap/v2/quote/contracts`), so `fetchMarkets()` ingestion path remains valid.
3. **Recent Trades List endpoint exists** in v3 docs at `/openApi/swap/v2/quote/trades` (public market trades).

Implications for this codebase:

- The current integration already consumes `/openApi/swap/v2/quote/contracts`, so newly listed non-crypto contracts are discoverable at the exchange layer.
- We do **not** currently define/use a public recent-trades endpoint constant for BingX (only user fills via `USER_TRADES`).
- Symbol conversion fallbacks are now quote-aware for `USDT`/`USDC` via a shared helper (`toBingxSymbol`), reducing symbol-format fragility across REST/WS/cancel flows.

## File Structure

| File                  | Description                                                                                       |
| --------------------- | ------------------------------------------------------------------------------------------------- |
| `bingx.api.ts`        | REST API client: Axios instance creation, HMAC-SHA256 signature, BigInt-safe JSON parsing         |
| `bingx.exchange.ts`   | Main `BingxExchange` class extending `BaseExchange`. Contains all trading logic                   |
| `bingx.types.ts`      | Constants (`BASE_URL`, `ENDPOINTS`, `RECV_WINDOW`), type mappings, WS event types, response types |
| `bingx.ws-private.ts` | Private WebSocket: listen key auth, order/position streaming                                      |
| `bingx.ws-public.ts`  | Public WebSocket: tickers, OHLCV candles, order book depth                                        |

## Authentication

BingX uses HMAC-SHA256 for API request signatures:

### Headers Required

- `X-BX-APIKEY`: API key (set once in Axios defaults)
- `X-SOURCE-KEY`: `'Tealstreet'` (broker ID)
- `Content-Type`: `'application/json, charset=utf-8'` (GET) or `'application/json'` (POST, set per-request)

### Signature Generation

1. Add `timestamp` (milliseconds from `virtualClock.getCurrentTimeMs()`) to parameters
2. Sort all parameters alphabetically using `localeCompare`
3. Create query string with `qs.stringify` (repeat array format, alphabetical sort)
4. Generate HMAC-SHA256 **hex digest** (not base64) using the secret key
5. Append `signature` to parameters

### GET vs POST Differences

- **GET**: All signed params go as query params; `data` is removed from config via `omit(nextConfig, 'data')`
- **POST**: Signed params stay in body; `quantity` and `price` fields are cast to numbers; `Content-Type` is set to `'application/json'`

### BigInt Handling

BingX uses BigInt order IDs that exceed JavaScript's `Number.MAX_SAFE_INTEGER`. The API client uses `bigIntAsStringJsonParse` as `transformResponse` to convert these to strings, avoiding JS rounding errors.

### Listen Key Authentication

- **Endpoint**: POST `/openApi/user/auth/userDataStream`
- **Livenet exception**: Listen key requests are **NOT signed** on livenet
- **Refresh**: Listen key is refreshed via PUT every 30 minutes. A transient failure retries after 30 seconds; an expired/invalid key rebuilds the socket with a newly fetched key.
- Private WS connects with `?listenKey=<key>` query parameter

### Adapter Configuration

The `axiosHttpOverWebsocketAdapter` is enabled for non-public-data-only
clients **unless** `options.tealstreetTunnel === false`.
Browser callers leave the flag undefined and inherit the tunnel; headless
callers (Node/Bun services with direct egress) set it to `false`
explicitly and use direct REST. The previous unconditional behavior crashed in Node where
the tunnel infrastructure isn't reachable.

## Startup Sequence

The `start()` method initializes with a **mixed parallel/sequential** strategy:

1. **Phase 1 -- Parallel market + ticker fetch**:
   - `fetchMarkets()` **and** `fetchTickersRaw()` are dispatched in parallel via `Promise.all`
   - Within the `fetchMarkets()` promise chain, once markets resolve:
     - Store updated with `markets`, `loaded.markets = true`
     - `publicWebsocket.connectAndSubscribe()` starts immediately
   - `fetchTickersRaw()` returns raw data (`{ dailys, prices }`)

2. **Phase 2 -- Format tickers** (sequential):
   - `formatTickers()` merges daily ticker data with premium index data
   - Store updated with `tickers`, `loaded.tickers = true`

3. **Public-data-only mode check**:
   - If `options.publicDataOnly` or no key/secret, marks balance/positions/orders as loaded and returns early

4. **Phase 3 -- Private WS + position mode** (sequential):
   - `privateWebsocket.connectAndSubscribe()` starts (fetches listen key, connects WS)
   - `getOrFetchPositionMode(true)` fetches hedge/one-way mode from API

5. **Phase 4 -- Parallel data fetch** via `Promise.all`:
   - `fetchOrders()` -> updates store with orders, `loaded.orders = true`
   - `fetchPositions()` -> `addOrUpdatePositionsFull()`, `loaded.positions = true`
   - `fetchBalance()` -> updates store with balance, `loaded.balance = true`

6. **Phase 5** -- `tick()` starts the polling loop

Each step checks `this.isDisposed` to allow early exit.

## REST API Endpoints

| Constant               | Path                                   | Method      | Usage                              |
| ---------------------- | -------------------------------------- | ----------- | ---------------------------------- |
| `BALANCE`              | `/openApi/swap/v3/user/balance`        | GET         | Account balance (array response)   |
| `MARKETS`              | `/openApi/swap/v2/quote/contracts`     | GET         | Markets/contracts list             |
| `POSITIONS`            | `/openApi/swap/v2/user/positions`      | GET         | Open positions                     |
| `LEVERAGE`             | `/openApi/swap/v2/trade/leverage`      | GET         | Fetch leverage for symbol          |
| `SET_LEVERAGE`         | `/openApi/swap/v2/trade/leverage`      | POST        | Set leverage for symbol            |
| `TICKERS_24H`          | `/openApi/swap/v2/quote/ticker`        | GET         | 24hr tickers (bid/ask/last/volume) |
| `TICKERS_PRICE`        | `/openApi/swap/v2/quote/premiumIndex`  | GET         | Mark/index prices, funding rates   |
| `HEDGE_MODE`           | `/openApi/swap/v1/positionSide/dual`   | GET/POST    | Get/set hedge mode                 |
| `MARGIN_MODE`          | `/openApi/swap/v2/trade/marginType`    | GET/POST    | Get/set margin type                |
| `OPEN_ORDERS`          | `/openApi/swap/v2/trade/openOrders`    | GET         | Fetch open orders                  |
| `ORDER`                | `/openApi/swap/v2/trade/order`         | POST/DELETE | Place/cancel single order          |
| `CANCEL_REPLACE`       | `/openApi/swap/v1/trade/cancelReplace` | POST        | Atomic order replacement           |
| `BATCH_ORDERS`         | `/openApi/swap/v2/trade/batchOrders`   | POST        | Batch order placement              |
| `CANCEL_SYMBOL_ORDERS` | `/openApi/swap/v2/trade/allOpenOrders` | DELETE      | Cancel all orders for a symbol     |
| `KLINE`                | `/openApi/swap/v3/quote/klines`        | GET         | Kline/candlestick data             |
| `ORDERBOOK`            | `/openApi/swap/v2/quote/depth`         | GET         | Order book depth                   |
| `USER_TRADES`          | `/openApi/swap/v2/trade/fillHistory`   | GET         | User trade/fill history            |
| `LISTEN_KEY`           | `/openApi/user/auth/userDataStream`    | POST/PUT    | Create/refresh listen key          |

### v3-Documented Endpoint Not Yet Wired

| Endpoint        | Path                            | Method | Current Status                                                                                   |
| --------------- | ------------------------------- | ------ | ------------------------------------------------------------------------------------------------ |
| `RECENT_TRADES` | `/openApi/swap/v2/quote/trades` | GET    | Available in docs; not wired yet (future candidate for better seconds-resolution chart fallback) |

### Ticker Fetching

Tickers require **two** REST calls that are merged:

- `TICKERS_24H` provides: `bidPrice`, `askPrice`, `lastPrice`, `priceChangePercent`, `volume`, `quoteVolume`
- `TICKERS_PRICE` provides: `markPrice`, `indexPrice`, `lastFundingRate`, `nextFundingTime`

The `formatTickers()` method joins them by `symbol`.

## WebSocket Implementation

### Public WebSocket (`bingx.ws-public.ts`)

- **URL**: `wss://ws-market-swap.qq-os.com/ws` (livenet and testnet)
- **Connection**: Single connection for all public data
- **Support status**: this `qq-os.com` host and its `market.*` channel grammar are app-internal and are not part of BingX's documented public WebSocket contract. The documented host is `wss://open-api-swap.bingx.com/swap-market`; treat migration/fallback as required before the internal host is retired.

**Subscriptions on connect**:

- `market.contracts` -- All contract tickers (last price, mark price, index price, funding rate, volume)

**On-demand subscriptions**:

- `market.kline.<SYMBOL>.<interval>.trade.utc+8` -- Kline updates (e.g., `market.kline.BTC-USDT.1min.trade.utc+8`)
- `market.depth.<SYMBOL>.step0.level100` -- Order book depth (100 levels)

**Subscription format**: `{ "reqType": "sub", "dataType": "<topic>", "id": "<nanoid>" }`

**Unsubscription format**: `{ "reqType": "unsub", "dataType": "<topic>", "id": "<nanoid>" }`

**Ping mechanism**:

- Sends text `"Ping"` for latency measurement
- Also sends JSON ping: `{"ping":"<nanoid>","time":"<ISO timestamp>"}`

**Pong handling**:

- Server may send `"Ping"` text to which client responds with `"Pong"`
- Server may also send `"Pong"` or JSON `{ pong: ... }` as response to client pings

**Data format**: Messages may arrive as `Blob` objects, decompressed via `readBlob()` utility

**Ticker update fields**:

- `tradePrice` -> `last`
- `fairPrice` -> `mark`
- `indexPrice` -> `index`
- `fundingRate` -> `fundingRate` (divided by 100)
- `changePercentage` -> `percentage`
- `volumeF` -> `volume`
- `valueF` -> `quoteVolume`

**Kline interval mapping**:

```text
1m -> 1min    3m -> 3min    5m -> 5min    15m -> 15min    30m -> 30min
1h -> 1hour   2h -> 2hour   3h -> 3hour   4h -> 4hour     6h -> 6hour
12h -> 12hour 1d -> 1day    1w -> 1week
1M -> 1month  3M -> 3month  1Y -> 1year
```

**Order book**: Full replacement on each update (no incremental delta). Each message contains full bids/asks arrays with `{ price, volume }` objects.

Non-ticker frames use direct lookup by exact `dataType`; only a matched topic records handler statistics and dispatches callbacks.

### Private WebSocket (`bingx.ws-private.ts`)

- **URL**: `wss://open-api-swap.bingx.com/swap-market?listenKey=<key>` (livenet and testnet)
- **Authentication**: Listen key obtained via POST to `LISTEN_KEY` endpoint
- **Listen key response shape**: `listenKey` may be returned as either `data.listenKey` or `data.data.listenKey`
- **Listen key refresh**: Every 30 minutes via PUT with `listenKey` query param. Refresh timers are single-owner and cleared on close/dispose; reconnect open re-arms one timer.
- **Expiry recovery**: A documented `listenKeyExpired` event closes the stale socket, fetches a new key, and creates a new socket URL. Failed recovery retries after 30 seconds.

**Ping mechanism**:

- Sends text `"Ping"`
- Server may respond with `"Pong"` text or JSON `{ pong: ... }`
- Server may also send `"Ping"` text to which client responds with `"Pong"`

**Data format**:

- Messages can arrive as `Blob` or string payloads
- Payloads are parsed with `bigIntAsStringJsonParse` to preserve BigInt order IDs
- Non ping/pong payloads are forwarded to request debugger via `grafana-pws`

**Event types handled**:

#### `listenKeyExpired`

The stale key and socket are discarded immediately. Recovery fetches a fresh key and rebuilds the private connection; retries are bounded to one tracked timer.

#### `ACCOUNT_UPDATE`

Live-verified payload shape:

- `event.a.B`: balance updates (`a`, `wb`, `cw`, `bc`)
- `event.a.P`: position updates (`s`, `pa`, `ep`, `up`, `mt`, `iw`, `ps`, optional `cr`)

Implementation behavior:

- Position rows are extracted from `a.P` first, with compatibility fallback to `a.B`/`a.p` only when rows are position-like
- Symbols are normalized (`ARB-USDT` -> `ARBUSDT`) before store matching
- Position IDs are resolved mode-aware (account mode fetched/cached from REST) and reuse existing IDs to avoid stale one-way/hedged duplicates
- Positions are added/updated in store (`addOrUpdatePosition`), not update-only
- Position updates trigger a throttled immediate REST reconciliation (`fetchPositions` -> `addOrUpdatePositionsFull`)

#### `ORDER_TRADE_UPDATE` and `TRADE_UPDATE`

Live-verified event names for order fills include **both**:

- `ORDER_TRADE_UPDATE`
- `TRADE_UPDATE`

Implementation behavior:

- Events are deduped by `orderId` using cumulative filled quantity (`o.z`)
- Incremental fill is derived from delta of cumulative filled (fallback to `o.l` when cumulative is unavailable)
- Fill events are emitted only on terminal `FILLED` status to avoid duplicate toasts
- `NEW`: adds non-market orders to store
- `CANCELED/EXPIRED`: marks order canceled and clears dedupe state
- `FILLED`: marks order closed and immediately drops its cumulative-fill dedupe entry
- Any positive fill delta schedules throttled immediate position reconciliation

## Order Types and Mappings

### Order Type Mapping

| Internal Type                | BingX API Value                          | Notes                                              |
| ---------------------------- | ---------------------------------------- | -------------------------------------------------- |
| `OrderType.Limit`            | `LIMIT`                                  | Requires `timeInForce`                             |
| `OrderType.Market`           | `MARKET`                                 |                                                    |
| `OrderType.StopLoss`         | `STOP_MARKET` or `TRIGGER_MARKET`        | Uses `TRIGGER_MARKET` when no open position exists |
| `OrderType.TakeProfit`       | `TAKE_PROFIT_MARKET` or `TRIGGER_MARKET` | Same fallback logic                                |
| `OrderType.TrailingStopLoss` | `TRAILING_STOP_MARKET`                   | Uses documented fractional `priceRate`             |

**Important**: `TRIGGER_MARKET` appears before `STOP_MARKET` in the mapping object so that `inverseObj()` will prefer `STOP_MARKET` as the default when converting from internal type to API value.

### Time In Force

| Internal TIF        | BingX Value |
| ------------------- | ----------- |
| `GoodTillCancel`    | `GTC`       |
| `ImmediateOrCancel` | `IOC`       |
| `FillOrKill`        | `FOK`       |
| `PostOnly`          | `PostOnly`  |

### Trigger Type Mapping

| BingX Value      | Internal Type                    |
| ---------------- | -------------------------------- |
| `MARK_PRICE`     | `OrderStopTriggerType.MarkPrice` |
| `CONTRACT_PRICE` | `OrderStopTriggerType.LastPrice` |

### Order Status Mapping

| BingX Status       | Internal Status |
| ------------------ | --------------- |
| `NEW`              | Open            |
| `PARTIALLY_FILLED` | Open            |
| `FILLED`           | Closed          |
| `CANCELED`         | Canceled        |
| `EXPIRED`          | Closed          |

## Order Lifecycle

### Order Placement

1. Orders are formatted via `formatCreateOrder()`
2. Trailing stops are handled separately via `formatCreateTrailingStopLossOrder()`
3. Orders exceeding `maxSize` are split into multiple lots
4. **All orders are placed sequentially** via `placeOrderBatch()`, sending one at a time to `ENDPOINTS.ORDER`
5. Attached TP/SL orders (`opts.stopLoss`, `opts.takeProfit`) are added as additional payloads using `TRIGGER_MARKET` type (since position likely isn't open yet)

**Note**: There is a legacy `placeOrderBatchOld()` method that used actual batch placement, but it is no longer in use.

### Order Update (Atomic Cancel-Replace)

All order updates use BingX's native atomic `cancelReplace` endpoint with `cancelReplaceMode: STOP_ON_FAILURE`:

- The replacement is submitted only if cancellation succeeds, avoiding the prior unprotected cancel/place gap.
- Full-position stop orders preserve `close: true` and omit quantity.
- Multi-payload replacements fail before submission because one old order cannot atomically map to several new orders.

### Order Cancellation

- **Individual cancellation only**: Orders are canceled one at a time via DELETE to `ENDPOINTS.ORDER`. Batch cancellation is disabled due to signature issues.
- Each cancellation failure is logged but processing continues with remaining orders
- Network/API failures are logged and do not abort later ids. Only successful cancellations and benign `80016` (already absent) responses are marked Canceled in the store.
- **Symbol-level cancellation** (`cancelSymbolOrders`): Uses `CANCEL_SYMBOL_ORDERS` endpoint to cancel all orders for a symbol

### TRIGGER_MARKET Fallback

When placing TP/SL orders, the code checks if a corresponding position exists:

- If a position exists on the opposite side with contracts > 0, uses `STOP_MARKET` / `TAKE_PROFIT_MARKET`
- If no position exists, falls back to `TRIGGER_MARKET` type
- If no position and no amount specified, throws an error: `"Cannot open infinite stop loss or take profit order without an open position"`

### StopLoss Reclassification

After fetching orders, `StopLoss` orders are reclassified as `TakeProfit` if the stop price is on the wrong side of the mark price:

- Buy side with `price < mark` -> reclassified as TakeProfit
- Sell side with `price > mark` -> reclassified as TakeProfit

### Full-position vs. partial stops

Current official Swap docs make `closePosition` and `quantity` mutually exclusive. Partial TP/SL orders send an explicit quantity and never `closePosition`. Full-position orders (`close: true` or amount `0`) require an existing opposite-side position and send `closePosition: 'true'` without quantity or `reduceOnly`. Attached stops use the entry quantity and are therefore partial/size-bound. This behavior is contract-verified; the July 2026 audit did not place an unauthorized live order.

## Symbol Format

- **BingX format**: Hyphenated, e.g., `BTC-USDT`, `BTC-USDC`
- **Internal format**: Concatenated, e.g., `BTCUSDT`, `BTCUSDC`
- **Market `id`** stores the BingX format; `symbol` stores the normalized format
- **Conversions**:
  - To internal: `symbol.replace(/-/g, '')` (used in position/order mapping)
  - To BingX: `toBingxSymbol(symbol)` (uses `market.id` when available, otherwise converts known quote suffixes like `USDT`/`USDC`)

### Important Symbol Caveat

- v3 docs/error guidance indicates swap symbols are expected to end in `-USDT` or `-USDC`.
- Shared conversion helper now handles `USDT` and `USDC` suffixes consistently across REST params and public WS topic building.
- New non-crypto contracts can produce symbols that look unusual when normalized (example pattern: `<UNDERLYING>USD-USDT` -> `<UNDERLYING>USDUSDT`), which is expected and not inherently invalid.

## Data Precision & Conversion

### Price/Amount Precision

BingX provides precision as the number of decimal places (e.g., `pricePrecision: 2` means `0.01`). The `_parsePrecision()` method converts this to a multiplier:

```text
pricePrecision=1 -> "0.1"
pricePrecision=2 -> "0.01"
pricePrecision=3 -> "0.001"
```

### Rounding

All prices and amounts are rounded via the `adjust()` utility from `safe-math` before being sent to the API.

### Funding Rate Conversion

BingX returns funding rate as a percentage value; code divides by 100 for internal storage.

### OHLCV Timestamp Conversion

BingX returns kline timestamps in milliseconds; code divides by 1000 for internal format (seconds).

### Balance Fields

```text
total = data.balance
free = data.availableMargin
upnl = data.unrealizedProfit
used = data.usedMargin
```

### Position Notional Calculation

```text
notional = abs(contracts) * entryPrice + unrealizedPnl
```

## Rate Limiting

- **Primary API**: 3 requests/second (`maxRPS: 3`) via `axios-rate-limit`
- **Unlimited API**: No rate limit (used for order placement via `unlimitedXHR`)
- **Recv window**: 20,000ms (20 seconds)
- **Network retry**: 3 retries on network errors via `axios-retry`

## State Reconciliation

### Tick Polling

BingX relies primarily on REST polling for state reconciliation:

- **Every tick** (configurable via `options.extra.tickInterval`, default 2s): Fetch positions via REST -> `addOrUpdatePositionsFull()`
- **Every 4th tick** (offset 0): Fetch balance
- **Every 4th tick** (offset 2, `count + 2`): Fetch orders -> full store replacement

### WS Position Updates

- `ACCOUNT_UPDATE` position rows are consumed from `a.P` (fallback compatibility extraction for variants)
- WS updates can create or update positions in store (`addOrUpdatePosition`)
- Matching is symbol-normalized and mode-aware to prevent stale duplicate IDs between one-way and hedged semantics
- WS updates are followed by throttled immediate REST reconciliation for authoritative cleanup

### WS Order Updates

- `ORDER_TRADE_UPDATE` and `TRADE_UPDATE` events are both supported
- **NEW** status: Adds order to store (except `OrderType.Market` which is transient)
- **CANCELED/EXPIRED**: Marks order as Canceled in store
- **FILLED**: Marks order as Closed in store
- Fill events are deduped by cumulative quantity and emitted on terminal `FILLED`

### Position Mode Caching

Position mode (hedge/one-way) is cached in `_accountPositionMode` to avoid repeated API calls:

- `getOrFetchPositionMode()` returns cached value unless `refetch = true`
- Fetched from `HEDGE_MODE` endpoint: `dualSidePosition: "true"` -> Hedged, `"false"` -> OneWay

## Exchange Metadata

```typescript
{
  requiresPassphrase: false,
  requiresCors: true,
  supportsTestnet: false,
  supportsTradesHistory: false,
  orderbookTickDelay: 60,
  tickInterval: 2_000,
  orderPendingUpdateDelay: 2_000,
  stopOrderPendingUpdateDelay: 4_000,
  hasEmptyPositions: false,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  hasIndependentHedgedLeverageInCrossMargin: true,
  supportsChaser: false,
  supportsPartialStops: true,
  supportsTrailingStopLoss: true,
  supportsInfinityStops: true,
  supportsTunnel: true,
  enforceWsTunnel: true,
  tvMaxGetBarFetches: 20,
  supportedTPSLTriggerTypes: [LastPrice, MarkPrice],
  defaultTPTriggerType: LastPrice,
  defaultSLTriggerType: MarkPrice,
  latencyDomain: 'open-api-swap.bingx.com',
}
```

## Testnet/Demo

- **REST URL**: Same for livenet and testnet (`https://open-api.bingx.com`)
- **WS URLs**: Same for livenet and testnet
- **Metadata**: `supportsTestnet: false` -- testnet is not officially supported
- **Listen key signing**: On livenet, listen key requests skip signing; on testnet, they are signed

## Known Quirks / Implementation Notes

1. **Batch cancellation disabled**: Orders are canceled one by one due to signature issues with batch cancellation. The original batch code is preserved in `placeOrderBatchOld()` but unused.

2. **Order placement is sequential**: Despite having a `BATCH_ORDERS` endpoint, the active `placeOrderBatch()` sends orders one at a time via `ENDPOINTS.ORDER`. The old batch method (`placeOrderBatchOld`) is preserved but unused.

3. **TP/SL classification fix**: StopLoss orders are reclassified as TakeProfit if the stop price is on the wrong side of the mark price.

4. **Funding rate is percentage**: Returned as percentage (divided by 100 in code) in both REST and WS.

5. **OHLCV timestamps**: BingX returns kline timestamps in milliseconds; code divides by 1000 for internal format.

6. **Fill history response format**: Response data can be in `fill_history_orders`, `fill_orders`, or directly as array. All three formats are handled.

7. **Filled time format**: Can be ISO string (e.g., `"2026-01-13T17:13:17.000+08:00"`) or milliseconds.

8. **`TRIGGER_MARKET` type**: Used instead of `STOP_MARKET`/`TAKE_PROFIT_MARKET` when no corresponding position exists (position isn't open yet).

9. **Update order**: Native atomic `cancelReplace` with `STOP_ON_FAILURE`. Full-position stop replacements preserve amount `0`/`close: true`, so the replacement remains a close-all trigger without inventing a stale position quantity.

10. **Trailing stop contract**: Resolves the internal symbol through `marketsBySymbol`, sends the exchange market id, and derives BingX's fractional `priceRate` (`0.05` means 5%). Missing market/ticker/position inputs fail before submission and emit an exchange error.

11. **BingX "lock fail" error**: The old batch order method suppresses `"Msg: lock fail"` errors as non-errors that BingX returns when submitting trigger orders.

12. **Live payload shape differs from old assumptions**: Position rows are delivered in `ACCOUNT_UPDATE.a.P` (while `a.B` is balances), and fills may arrive under both `ORDER_TRADE_UPDATE` and `TRADE_UPDATE`. Adapter logic now handles both and dedupes fill emissions by cumulative quantity.

13. **WS + REST hybrid reconciliation**: WS applies fast local updates, then schedules a throttled immediate REST position refresh to resolve exchange-side eventual consistency.

14. **Market max leverage fallback**: If `maxLongLeverage` is 0 or falsy, defaults to 100.

15. **Market max order size**: Set to `Infinity` with a TODO comment -- actual max not fetched from API.

16. **HOW adapter gated on `tealstreetTunnel`**: The `axiosHttpOverWebsocketAdapter` is enabled when the caller has both a private API path (i.e. not `publicDataOnly`) and the tunnel isn't explicitly disabled. Headless server-side callers set `tealstreetTunnel: false` to take REST directly; browser code leaves it undefined and gets the tunnel.

17. **Public data only mode**: When `publicDataOnly`, no key, or no secret, the exchange skips private WS, orders, positions, and balance fetching.

18. **Error message handling**: Most error handlers use verbose null-safe extraction with fallback to `JSON.stringify(err, null, 2)` for object errors to avoid `[object Object]` in error messages.

19. **Misleading error context strings**: Several `emitAndLogError` calls pass unrelated context strings (e.g., "Failed to update leverage" when handling order placement, "Failed to switch position mode" / "Failed to switch margin type" for batch order errors) -- these are copy-paste artifacts.

20. **Candle data sorting**: `fetchOHLCV` explicitly sorts candles by timestamp after parsing, indicating BingX may not return them in chronological order.

21. **Position side in one-way mode**: When account is not in hedge mode, position side is determined by contract amount sign: `contracts > 0 -> Long`, else `Short`.

22. **Leverage endpoint dual use**: `LEVERAGE` and `SET_LEVERAGE` both point to the same path (`/openApi/swap/v2/trade/leverage`), differentiated by HTTP method (GET vs POST).

23. **v3 changelog (non-crypto pairs)**: non-crypto contract support is announced via `/openApi/swap/v2/quote/contracts` and is compatible with current `fetchMarkets()` ingestion, but downstream UX layers can still depend on ticker availability and symbol fallbacks.

24. **Quote-agnostic symbol conversion hardening**: REST param fallbacks, cancel-by-symbol flows, and public WS topic symbols now use a centralized `toBingxSymbol()` helper (market-id-first, suffix-aware for `USDT`/`USDC`) instead of scattered `USDT`-only replacements.

25. **Public trades endpoint status**: v3 documents `/openApi/swap/v2/quote/trades`, but the adapter does not currently consume it. If we need stronger seconds-resolution behavior on BingX symbols, this is the next place to integrate.

26. **Non-crypto order mode constraint**: BingX can reject non-crypto orders in account One-Way mode with message `non-crypto symbol with one-way mode not support openapi and copy trade`. Order creation now does a preflight mode check for likely TradFi contracts and emits an actionable Hedged-mode requirement error before submission.

## Error Handling

### REST API Error Codes

- **Success**: `code: 0`
- **Error response format**: `{ code: number, msg: string, data: ... }`
- **`80016`**: "order not exist" -- returned when canceling an already-canceled order
- **`109401`**: "user has pending orders" -- returned when trying to change position mode with open orders

### Error Handling Patterns

- Order cancellation failures are logged per-order and processing continues with remaining orders; unsuccessful ids remain Open until authoritative reconciliation.
- `fetchBalance` returns existing store balance on error instead of throwing
- `fetchMarkets` returns existing store markets on error instead of throwing
- `fetchOpenPositions` returns existing store positions on error
- `fetchOrders` returns existing store orders when API returns an error code
- `fetchExecutions` returns empty array on error with `LogSeverity.Warning`

### Network Retry

- `axios-retry` configured with 3 retries on network errors

### BigInt Safety

- Order IDs from BingX can exceed `Number.MAX_SAFE_INTEGER`. The `bigIntAsStringJsonParse` transformer ensures these are parsed as strings throughout the application.

## Position Handling

- Supports both One-Way and Hedge mode via `dualSidePosition`
- In hedge mode, `positionSide` is `LONG`/`SHORT`; in one-way mode it's `BOTH`
- Position side for stop/TP/trailing stop orders is inverted in hedge mode
- Full-position TP/SL uses `closePosition: 'true'` without quantity; partial TP/SL uses quantity without `closePosition`.
- Position mode changes require no open orders (API returns error `109401`)
- After position mode or margin type changes, positions are refetched

## Testing / Validation

Run targeted type checking:

```bash
cd packages/safe-cex && yarn typecheck
```

Key validation points:

- `validateAccount()` tests API key validity by fetching balance
- `getAccount()` extracts the documented v3 balance row's masked `userId`; `fetchBalance()` and `validateAccount()` prefer the USDT row and fall back to the first settlement-asset row.
- Public data only mode for unauthenticated access
