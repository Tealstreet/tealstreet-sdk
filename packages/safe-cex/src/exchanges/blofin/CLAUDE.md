# BloFin Exchange Integration

This directory contains the BloFin exchange integration for Tealstreet. BloFin is a centralized crypto exchange offering linear perpetual futures. The API closely resembles OKX in structure and conventions.

## Exchange Information

| Property                                    | Value                |
| ------------------------------------------- | -------------------- |
| `exchangeName`                              | `blofin`             |
| `prettyName`                                | `Blofin`             |
| `requiresPassphrase`                        | `true`               |
| `requiresApplicationId`                     | `false`              |
| `enabled`                                   | `true`               |
| `supportsTestnet`                           | `false`              |
| `orderbookTickDelay`                        | `250`                |
| `requiresCors`                              | `true`               |
| `hasEmptyPositions`                         | `false`              |
| `supportsTunnel`                            | `false`              |
| `positionModeIsGlobal`                      | `true`               |
| `marginTypeIsGlobal`                        | `true`               |
| `tvMaxGetBarFetches`                        | `20`                 |
| `supportsChaser`                            | `true`               |
| `supportsPartialStops`                      | `true`               |
| `alwaysCalculateUnrealizedPnlFromLastPrice` | `true`               |
| `fetchLeverageOnMarginSymbolChange`         | `true`               |
| `latencyDomain`                             | `openapi.blofin.com` |

## API Documentation

- **Official API Docs**: https://docs.blofin.com/index.html#overview

## File Structure

- `blofin.api.ts` - REST API client with Axios, HMAC-SHA256 auth interceptor, 20s timeout, 3 network-error retries
- `blofin.exchange.ts` - Main exchange class (`BlofinExchange`) extending `BaseExchange`; implements all trading, position, and account operations
- `blofin.types.ts` - Constants (endpoints, broker ID, base URLs, intervals), mapping tables (order type/side/status), and TypeScript response types
- `blofin.ws-private.ts` - Private WebSocket for account, positions, orders, and algo-order events
- `blofin.ws-public.ts` - Public WebSocket for tickers, funding rates, candlesticks, and order books

## Authentication

### Signature Algorithm

BloFin uses **HMAC-SHA256** with base64 encoding. Implemented in `blofin.api.ts`:

1. Build the string to sign by concatenating (no separators): `url + method + timestamp + nonce [+ body]`
   - `url` includes query string (decoded via `qs.stringify`)
   - `method` is uppercase (`GET`, `POST`)
   - `timestamp` is milliseconds from virtual clock
   - `nonce` is a UUID v4
   - `body` is JSON-stringified request data (only for POST requests)
2. Compute HMAC-SHA256 using the API secret as key
3. Hex-encode the digest, then base64-encode the hex string
4. Public endpoints (URLs containing `public`) skip authentication

### Request Headers

All authenticated requests include these headers:

| Header              | Value                            |
| ------------------- | -------------------------------- |
| `ACCESS-KEY`        | API key                          |
| `ACCESS-SIGN`       | Base64(hex(HMAC-SHA256(toSign))) |
| `ACCESS-NONCE`      | UUID v4 nonce                    |
| `ACCESS-TIMESTAMP`  | Millisecond timestamp            |
| `ACCESS-PASSPHRASE` | Account passphrase (required)    |

### Time Sync

Timestamps come from `virtualClock`, which is synchronized with the exchange's server time. The `+` unary operator is used to convert the dayjs time to a numeric millisecond timestamp (see `blofin.api.ts`).

**Signer-failure branch does NOT say "auth failed".** The `remoteSigner.sign` error branch in `blofin.ws-private.ts auth` emits `Remote WS signing unavailable: …`, not `Remote WS auth failed: …`. The latter wording matches `health/errorPatterns.ts`'s `auth_invalid` (case-insensitive substring), which the web controller treats as a terminal API-key rejection and suppresses automatic account reloads for five minutes — the very reload a signer-less tab needs to re-pair. See `../CLAUDE.md` → "Remote-signer WS auth" and the bybit template in `bybit/bybit.ws-private.ts`.

### WebSocket Authentication

Private WebSocket authenticates via a `login` operation after connection:

```json
{
  "op": "login",
  "args": [{
    "apiKey": "<key>",
    "passphrase": "<passphrase>",
    "timestamp": <ms>,
    "sign": "<signature>",
    "nonce": "<uuid>"
  }]
}
```

The signature signs the string `/users/self/verify` + `GET` + `timestamp` + `nonce` using the same HMAC-SHA256 + base64(hex) method (see `blofin.ws-private.ts auth`).

## Startup Sequence

The `start()` method follows this initialization order:

1. **Phase 1 (parallel)**: Fetch markets and raw tickers simultaneously via `Promise.all`
   - Once markets resolve: store is updated with `markets` and `loaded.markets = true`, then public WebSocket connects immediately (`publicWebsocket.connectAndSubscribe()`)
   - Raw tickers resolve independently
2. **Dispose check**: If disposed during Phase 1, return early
3. **Format tickers**: Convert raw tickers using market data, store with `loaded.tickers = true`
4. **Public-only check**: If `publicDataOnly` or missing key/secret, return here (no private data)
5. **Private WebSocket**: Connect and subscribe (`privateWebsocket.connectAndSubscribe()`)
6. **Phase 2 (parallel)**: Fetch margin type and position mode simultaneously (both forced refetch with `refetch = true`)
7. **Phase 3 (parallel)**: Fetch balance+positions and orders simultaneously
   - Balance/positions stored with `loaded.balance = true, loaded.positions = true`
   - Orders stored with `loaded.orders = true`
8. **Start tick loop**: Calls `this.tick()` to begin periodic polling

Key dependency: Public WebSocket starts as soon as markets load, without waiting for tickers. Private WebSocket starts before private REST data is fetched. Margin type and position mode must resolve before balance/positions/orders.

## REST API Endpoints

All endpoints are prefixed with `/api/v1/`. Defined in `ENDPOINTS` object in `blofin.types.ts`:

### Public

| Constant  | Path                         | Method | Description                  |
| --------- | ---------------------------- | ------ | ---------------------------- |
| `MARKETS` | `/api/v1/market/instruments` | GET    | Fetch all instruments        |
| `TICKERS` | `/api/v1/market/tickers`     | GET    | Fetch all ticker data        |
| `KLINE`   | `/api/v1/market/candles`     | GET    | Fetch OHLCV candlestick data |

### Private - Account

| Constant            | Path                                  | Method | Description                                     |
| ------------------- | ------------------------------------- | ------ | ----------------------------------------------- |
| `BALANCE`           | `/api/v1/asset/balances`              | GET    | Account balances (param: `accountType=futures`) |
| `POSITIONS`         | `/api/v1/account/positions`           | GET    | Open positions                                  |
| `LEVERAGE`          | `/api/v1/account/batch-leverage-info` | GET    | Batch leverage info (up to 20 symbols)          |
| `SET_LEVERAGE`      | `/api/v1/account/set-leverage`        | POST   | Set leverage for instrument                     |
| `MARGIN_MODE`       | `/api/v1/account/margin-mode`         | GET    | Get current margin mode                         |
| `SET_MARGIN_MODE`   | `/api/v1/account/set-margin-mode`     | POST   | Set margin mode (cross/isolated)                |
| `POSITION_MODE`     | `/api/v1/account/position-mode`       | GET    | Get current position mode                       |
| `SET_POSITION_MODE` | `/api/v1/account/set-position-mode`   | POST   | Set position mode (net/long_short)              |

### Private - Trading

| Constant                  | Path                                | Method | Description                                              |
| ------------------------- | ----------------------------------- | ------ | -------------------------------------------------------- |
| `PLACE_ORDERS`            | `/api/v1/trade/order`               | POST   | Place single order                                       |
| `PLACE_ORDERS_BATCH`      | `/api/v1/trade/batch-orders`        | POST   | Place batch orders (max 20)                              |
| `PLACE_TPSL_ORDER`        | `/api/v1/trade/order-tpsl`          | POST   | Place TP/SL algo order                                   |
| `PLACE_TRIGGER_ORDER`     | `/api/v1/trade/order-algo`          | POST   | Place opening trigger/algo order                         |
| `CANCEL_ORDERS`           | `/api/v1/trade/cancel-batch-orders` | POST   | Cancel batch orders (max 20)                             |
| `CANCEL_ALGO_ORDER`       | `/api/v1/trade/cancel-tpsl`         | POST   | Cancel TP/SL orders (max 10)                             |
| `CANCEL_TRIGGER_ORDER`    | `/api/v1/trade/cancel-algo`         | POST   | Cancel one opening trigger/algo order                    |
| `UNFILLED_ORDERS`         | `/api/v1/trade/orders-pending`      | GET    | Get pending normal orders                                |
| `UNFILLED_ALGO_ORDERS`    | `/api/v1/trade/orders-tpsl-pending` | GET    | Get pending TP/SL orders                                 |
| `UNFILLED_TRIGGER_ORDERS` | `/api/v1/trade/orders-algo-pending` | GET    | Read externally placed opening triggers                  |
| `FILLS`                   | `/api/v1/trade/fills-history`       | GET    | User fill history (primary for `fetchHistoricalFills`)   |
| `ORDERS_HISTORY`          | `/api/v1/trade/orders-history`      | GET    | Filled order history fallback for `fetchHistoricalFills` |

Execution-history behavior (`fetchHistoricalFills`):

- REST-first only (no in-memory session cache fallback)
- Calls `fills-history` with `begin`/`end` millisecond window
- Falls back to `orders-history` (`state=filled`) when primary returns no rows or is unsupported
- Normalizes timestamps to Unix seconds
- Handles multiple BloFin payload variants for side/price/qty/orderId/fee mapping:
  - side: `side`, `actualSide`
  - price: `fillPrice`, `fillPx`, `averagePrice`, `avgFillPrice`, `avgPx`, `execPrice`, `executedPrice`, `price`
  - qty (native contracts): `fillSize`, `filledSize`, `actualSize`, `fillSz`, `tradeQty`, `execQty`, `executedQty`, `size`, `qty`, `amount`
  - order id: `orderId`, `ordId`, `algoId`, `tpslId`
  - fee: `fee`, `fillFee`, `execFee`, `tradeFee`, `actualFee`
- Converts native contract qty to base qty via `contractSize`
- Uses absolute values for signed qty/fee payloads before emitting `Execution`

## WebSocket Implementation

### Connection URLs

| Type    | Livenet                               | Demo trading                                       |
| ------- | ------------------------------------- | -------------------------------------------------- |
| Public  | `wss://openapi.blofin.com/ws/public`  | `wss://demo-trading-openapi.blofin.com/ws/public`  |
| Private | `wss://openapi.blofin.com/ws/private` | `wss://demo-trading-openapi.blofin.com/ws/private` |

### Keepalive / Ping-Pong

Both WebSocket connections use text-based `ping`/`pong` frames (not JSON). The flow:

1. On open, `ping()` is called immediately from `onOpen` in both `blofin.ws-public.ts` and `blofin.ws-private.ts`
2. `pong` response triggers `handlePongEvent()` in `BaseWebSocket`, which schedules next ping in 5 seconds
3. If pong is not received within 7.5 seconds (1.5x ping interval), the base class triggers a reconnect
4. `WS_RECONNECT_THRESHOLD_MS` (20 seconds) defines the maximum time without any message before considering the connection unhealthy

### Reconnection Logic

Reconnection is handled by `BaseWebSocket`:

- Uses `ReconnectingWebSocket` when `ENABLE_RECONNECTING_WEBSOCKET` is enabled
- On reconnect, public WS calls `emitWsReconnected()` to notify the system
- On reconnect, private WS waits for a successful login frame before calling
  `parent.fetchAndSetPrivateSnapshot()`; rejected keys must not trigger private REST snapshots.
- The `onClose` handler is inherited from `BaseWebSocket` and handles reconnect timing
- The public socket arms a 20-second first-message watchdog on initial connect and after a close. If no frame arrives, it emits one forced warning explaining that REST history may still load while live charts/order books will not update. Reconnect churn does not repeat the warning; any inbound frame clears it and permits a future outage to warn once again.

### Public Channels

| Channel            | Description                                                            | Subscription Scope           |
| ------------------ | ---------------------------------------------------------------------- | ---------------------------- |
| `tickers`          | 24h ticker updates (bid, ask, last, volume)                            | Per instrument (all markets) |
| `funding-rate`     | Funding rate and next funding time                                     | Per instrument (all markets) |
| `candle<interval>` | OHLCV candlestick updates                                              | Per instrument (on demand)   |
| `books`            | Full order book with incremental updates (snapshot + update actions)   | Per instrument (on demand)   |
| `books5`           | Top-5 order book snapshots (used for mid-market price in candle chart) | Per instrument (on demand)   |

Initial all-market ticker/funding subscriptions and incremental
`markets:changed` additions are serialized into UTF-8-size-aware batches. Every
complete subscribe frame, including its JSON envelope, stays at or below
BloFin's documented 4,096-byte multi-channel limit.

**Message routing** (see `blofin.ws-public.ts onMessage`): The handler extracts `channel` and `instId` once via targeted `indexOf` against the raw frame, then dispatches via direct Map lookup — broadcast handlers keyed by channel alone (`tickers`, `funding-rate`) and per-instrument handlers keyed by `${channel}.${instId}` (`candle*`, `books`, `books5`). The slice-to-quote extraction naturally disambiguates `books` from `books5` (no trailing-quote workaround needed) and yields the exact channel name for interval-specific `candle*` keys. Subscribe-ack frames are short-circuited by a single `'"event":"subscribe"'` indexOf before any handler runs. See `../CLAUDE.md` → "Public WebSocket Hot-Path Patterns" for the cross-cutting rationale.

**Orderbook parser hot-path conventions**: the primary parser uses indexed for-loops over `snapshot.bids` / `snapshot.asks` — never `.reduce` with `[...acc, x]` (quadratic) or `sides.forEach`. The `secondaryParser` (books5 top-N) writes new levels in place over the existing `orderBook.bids` / `orderBook.asks` arrays via index assignment, only `push`ing past the current length. **No trailing `sortOrderBook`** — `updateOrderBook` does sorted insertion via binary search and BloFin snapshots arrive pre-sorted (the post-filter cross-level cleanup `asks.filter(a => a.price >= lastPrice)` preserves order).

The primary `books` parser also enforces BloFin's per-instrument sequence
contract: a snapshot establishes `seqId`, and every delta's `prevSeqId` must
match it. Missing or discontinuous sequence values cause the delta to be
dropped, the local book to be cleared, and one `books` unsubscribe/subscribe
cycle to request a fresh snapshot. Further deltas are ignored until that
snapshot arrives.

### Private Channels

| Channel       | Description                                                   |
| ------------- | ------------------------------------------------------------- |
| `account`     | Account balance updates (`totalEquity`)                       |
| `positions`   | Position changes (all instruments)                            |
| `orders`      | Normal order status updates (live, filled, canceled)          |
| `orders-algo` | TP/SL and opening-trigger updates from the documented channel |

Private channels subscribe without `instType` parameter (receives all instruments). Channels are subscribed individually (one `subscribe` message per channel) rather than batched (see `blofin.ws-private.ts subscribe`).

**Auth flow** (see `blofin.ws-private.ts auth` / `onMessage`):

1. On open: `auth()` is called, then `ping()`
2. When login succeeds (message equals `{"event":"login","code":"0","msg":""}`): `subscribe()` is called
3. Message routing uses string `includes()` checks, same as public WS

## Order Types and Mappings

### Normal Orders

| Internal Type                | BloFin API Value | Notes                   |
| ---------------------------- | ---------------- | ----------------------- |
| `OrderType.Market`           | `market`         | Market order            |
| `OrderType.Limit`            | `limit`          | Limit order             |
| `OrderType.Limit` (PostOnly) | `post_only`      | Limit with PostOnly TIF |

Additional BloFin order types mapped to internal types:

- `fok` (Fill or Kill) -> `OrderType.Limit`
- `ioc` (Immediate or Cancel) -> `OrderType.Limit`
- `optimal_limit_ioc` -> `OrderType.Market`

### Algo Orders (TP/SL)

- `OrderType.StopLoss` -> BloFin `trigger` type, uses `slTriggerPrice` and `slOrderPrice=-1` (market execution)
- `OrderType.TakeProfit` -> BloFin `trigger` type, uses `tpTriggerPrice` and `tpOrderPrice=-1` (market execution)
- The legacy reverse-map entry for `OrderType.TrailingStopLoss` is dormant:
  metadata advertises no trailing-stop support, no formatter builds a trailing
  payload, and the `move_order_stop` value has not been validated against
  BloFin's schema. Do not treat it as an implemented order type.

### Opening Trigger Orders

`fetchOrders` includes BloFin's documented
`GET /api/v1/trade/orders-algo-pending?orderType=trigger` family, paginated by
`algoId`. These externally placed orders map to `OrderType.Conditional` using
their `triggerPrice`, and `supportsOpeningTriggerOrders` is enabled. Placement
uses the dedicated `POST /api/v1/trade/order-algo` object contract, with
positive native-contract size, explicit trigger price, `orderPrice: -1` for
market execution, and optional attached TP/SL protection. Cancellation uses
the single-object `POST /api/v1/trade/cancel-algo` contract with exact `algoId`
response correlation. BloFin publishes no amend endpoint for this family, so
edits preflight a full replacement before cancellation while preserving raw
margin mode, position side, trigger reference, execution price, native
remaining size, and attached protection. Missing provenance fails before I/O.

### Algo Order Batching

BloFin does NOT support batch algo order placement. Algo orders are placed one at a time inside `placeAlgoOrderBatch`. Normal orders (including inline TP/SL fields) stay on batch placement up to 20 per request for scaled-order performance.

## Order Lifecycle

### Virtual TP/SL Order IDs

TP/SL orders attached to a normal order are stored with composite IDs:

- `{tpslId|orderId}__take_profit` for take-profit
- `{tpslId|orderId}__stop_loss` for stop-loss

This allows updating/canceling individual TP or SL legs while maintaining parent relationship.

### Attached TP/SL Representation

BloFin can report attached TP/SL in two different ways:

1. As dedicated algo records with `tpslId` (from `orders-algo` / `orders-tpsl-pending`)
2. As inline `tpTriggerPrice` / `slTriggerPrice` fields on the parent normal order record

The adapter handles both:

- `mapOrders` emits protective legs **only** from algo records (when `tpslId` exists), since they are the authoritative cancel target. It deliberately does NOT synthesize virtual TP/SL from normal-order records, even when those records carry inline trigger prices — that would double-emit when both records arrive.
- `fetchOrdersWithRetry` post-processes merged order snapshots with `addInlineTpSlFallbackOrders`, synthesizing `__take_profit` / `__stop_loss` children from normal records only when no linked algo leg is present.
- This avoids duplicate TP/SL legs while preserving UI visibility/cancel-edit behavior even when BloFin does not materialize a separate algo record.

**Transient WS-only window**: Because `mapOrders` no longer synthesizes virtual TP/SL from normal records, a brief window can exist where a WS `orders` event delivers the parent normal order record before the matching `orders-algo` record arrives — during that window the UI shows the Limit without its protective leg. This self-heals via the debounced (~2s) full order refetch triggered by private WS order events (`blofin.ws-private.ts handleOrdersTopic`), and the next `tick` REST sweep will run `addInlineTpSlFallbackOrders` for any normal-only records that still lack an algo sibling.

### Update Order (Cancel-Replace)

`updateOrder` follows a cancel-and-replace pattern:

1. Extract the real ID (strip `__` suffix) and find all related orders (parent + TP/SL children)
2. If there are related orders (compound order):
   - Reconstruct the full order with parent's parameters and existing TP/SL prices
   - Apply the price/amount update to the correct leg (parent, stop_loss, or take_profit)
3. Cancel the appropriate orders (algo orders use `cancelAlgoOrders`, normal use `cancelOrders`)
4. Place a new order with the updated parameters via `placeOrder`

Opening trigger edits use their own bounded cancel-replace path. They never
flow through normal-order or TP/SL endpoints, and the complete replacement
payload is constructed and validated before the old `algoId` is canceled.

### Cancel Orders with Preserve Logic

`cancelOrders` has special logic for preserving the opposite TP/SL leg:

1. When canceling a single TP (or SL) that has amount=0 (full position):
   - Check if an opposite SL (or TP) exists with amount=0
   - If so, mark it for preservation
2. Replace all virtual child order IDs with their parent order IDs, dedup
3. Partition into algo and normal orders, cancel each set
4. If an order was marked for preservation, wait 500ms then re-place it

### Reduce-Only Handling

Controlled by `USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY` flag (currently `false`, see `blofin.types.ts`). When enabled, reduce-only orders use the single-order endpoint (`/api/v1/trade/order`) instead of the batch endpoint. Non-reduce normal orders (with or without attached TP/SL) remain batched.

### Large Order Splitting

`formatNormalOrder` splits orders exceeding `maxSize`:

- Calculates number of lots needed: `Math.ceil(amount / maxSize)`
- Distributes evenly with remainder as a final partial lot
- Each split gets its own `clientOrderId`
- When TP/SL is requested, payload includes both legacy top-level trigger fields (`slTriggerPrice` / `tpTriggerPrice`) and an `attachAlgoOrders` compatibility object so BloFin schema variants keep attached protection.

### Combined TP/SL for Position Orders

`formatAlgoOrder`: When placing a TP or SL with amount=0 (full position), the code checks for existing opposite algo orders on the same symbol/side. If found, it cancels them and combines both TP and SL into a single API request by setting both `tpTriggerPrice`/`tpOrderPrice` and `slTriggerPrice`/`slOrderPrice` on one payload.

### Standalone TP/SL Targeting

Standalone TP/SL orders use `/api/v1/trade/order-tpsl` and must send an explicit
`positionSide` target: `net` in one-way mode, or the existing `long` / `short`
leg in hedge mode. Full-position TP/SL uses `size: "-1"` and
`reduceOnly: "true"`; do not reintroduce the undocumented `closeFraction` field.

BloFin's product UI now exposes a **Multi-Trade / Multi-Position** mode. Treat
it as a hedged-mode-only extension: one-way/net mode is not the shape at issue;
the risk appears when hedge mode can hold multiple same-direction positions for
the same instrument. This is a third-rail account shape for Tealstreet today.
The public futures API docs still expose only `positionSide` on
`/trade/order-tpsl`; Tealstreet's BloFin adapter keys live positions by
`symbol + side + positionMode`, so it cannot represent or target multiple
BloFin `positionId`s for the same symbol and side.

Live testing showed support is spotty enough to wall off:

- `/trade/order-tpsl` returned `102143` even after including the visible
  position row's raw `positionId`, so do not retry with an undocumented
  `positionId` field.
- Reduce-only market close can acknowledge the order while the Multi-Position
  row remains open, so close-position flows are not reliable either.

A future implementation could try to reverse-engineer BloFin's web client or
private API shape, but that would be fragile and would still require a broader
position-store/UI rework to preserve venue-native position identity, select
between same-side rows, and thread that identity through TP/SL, reduce-only, and
close flows. Current recommendation: keep Multi-Position mode walled off, treat
`102143` as an unsupported account mode, and tell the user to disable
Multi-Position mode on BloFin before trading from Tealstreet.

## Symbol Format

- **Market ID (instId)**: `BTC-USDT` (hyphen-separated, no `-SWAP` suffix in API responses despite some historical patterns)
- **Internal Symbol**: `BTCUSDT` (derived by stripping `-SWAP` suffix if present, then removing all hyphens)
- **Instrument Type**: Only `instType: 'SWAP'` instruments are supported. Spot payloads are intentionally hidden until BloFin spot support is implemented.
- **Contract Type**: Only `linear` swap contracts are supported (filtered in `fetchMarkets`) Spot support is properly implemented in BloFin but still needs to be wired up properly on our side.
- Conversion helpers: `symbolToMarketId()` and `marketIdToSymbol()` on the exchange class

## Data Precision & Conversion

### Contract Size Model

BloFin uses a contract-based system similar to OKX:

- Each instrument has a `contractValue` (e.g., 0.001 BTC per contract)
- `lotSize` defines the minimum contract increment
- `precision.amount` stores the native lot step (`lotSize`)
- `contractSize` stores base-per-contract (`contractValue`)
- Order payload `size` is sent/parsed as native contracts and rounded to `lotSize` via `adjust()`
- The API `size` field represents number of contracts, not base currency amount
- For algo orders with amount 0 (full position), `size=-1` is sent

### Price Rounding

Prices are rounded to `market.precision.price` (derived from `tickSize`) using the `adjust()` utility from `safe-math.ts`.

### Amount Conversion in mapOrders

`mapOrders`: API `size` and `filledSize` are contract counts. They are converted to base currency by multiplying with `contractValue`:

```
amount = parseFloat(o.size) * contractValue
filled = parseFloat(o.filledSize) * contractValue
```

Execution history follows the same conversion rule (`qty = nativeQty * contractSize`). Small native fills can produce tiny-but-valid base quantities (for example `0.2` contracts on `BTC-USDT` with `contractValue=0.001` -> `0.0002` BTC), so downstream UIs should avoid coarse fixed-decimal rounding that can display them as `0.0`.

BloFin's public `Order.filled` remains base quantity. `Order.extra.filledInPlacementUnits` preserves the same cumulative native-contract value accepted by `PlaceOrderOpts.amount`, so chaser remainder arithmetic does not mix base quantity with placement units.

### Book snapshots

`books5` replaces the price range covered by its top levels, preserving only
levels deeper than that range. Replacing the first N array entries loses sorted
price order and can duplicate a price after the top of book moves. The Rust
engine uses the same merge and requires matching `prevSeqId` for full-book
updates; a gap clears sequence state and requests a fresh snapshot.

### Order Book Amount Conversion

Order book amounts from WS are contract counts. Keep them in native contracts in `blofin.ws-public.ts`; web orderbook/DOM grouping utilities convert to base or quote exactly once using `contractSize`.

## Rate Limiting

- **REST timeout**: 20 seconds (higher than typical to accommodate batch leverage requests, see `blofin.api.ts`)
- **Retry**: 3 retries on network errors via `axios-retry` (see `blofin.api.ts`)
- **Normal order batches**: Max 20 per request
- **Algo cancel batches**: Max 10 per request, with 1-second sleep between batches (see `cancelAlgoOrders`)
- **Leverage fetch batches**: Max 20 symbols per request, processed serially via `mapSeries`
- **No explicit rate limiter**: Unlike some exchanges, no `axios-rate-limit` is applied
- **Upstream limits**: BloFin documents 500 REST requests/minute per IP, 30
  trade requests/10 seconds per user ID, one new WebSocket connection/second
  per IP, and at most 4,096 bytes of channels per subscription request. Public
  all-market subscriptions are chunked below that frame limit.

## State Reconciliation

### Position-Order Consistency

- **Tick loop** (`tick`): Runs every 10 seconds. Fetches balance+positions and orders independently -- one failing does not block the other.
- **Order preservation on failure**: If `fetchOrders` returns empty but the store had orders, a warning is logged but the empty result is accepted. Each normal, TPSL, and trigger order family is an atomic paginated snapshot: a first-page or later-page transport/API failure, an invalid/non-advancing cursor, or the pagination safety cap rejects the whole attempt instead of returning partial rows. On complete failure (all 3 retries exhausted), existing store orders are returned to prevent data loss (see `fetchOrdersWithRetry`).
- **Order update guard** in tick: Only updates orders if new fetch returned results or store was already empty.

### WebSocket vs REST

- **Private WS order events**: Processed immediately for individual order state updates (add/update for live/partially_filled, status change for filled/canceled). Then triggers a debounced (2-second) full order re-fetch to ensure state synchronization (see `blofin.ws-private.ts handleOrdersTopic`).
- **Position updates**: WS position events apply via `addOrUpdatePositionsFull` so closed-symbol records (with their leverage / marginType / side / positionMode) are preserved while contracts / notional / upnl / liq are zeroed. The same call also recalculates `used` margin and `upnl` from the position data (see `blofin.ws-private.ts`).
- **Account updates**: WS account events update `totalEquity` -> `free` and `total` balance fields.
- **REST tick parity**: The 10s `tick` uses `addOrUpdatePositionsFull` (not a wholesale `store.update({ positions })`) so REST and WS share the same preserve-on-update semantics. Critical for keeping closed-symbol leverage available to the place-order % slider's no-position fallback. See the "Leverage" subsection under "Position & Margin Configuration" for the full rationale.
- **Reconnection snapshot**: On private WS reconnect, `fetchAndSetPrivateSnapshot()` runs only after the private login success frame, so rejected keys do not trigger a retry-heavy private REST snapshot.
- **Blank liquidation price**: `mapPositions` preserves the existing
  `liquidationPrice` for a matching open position when BloFin sends the field
  blank/missing. BloFin payload examples include `liquidationPrice: ""`, and
  treating that as zero hides the chart/table liquidation display until the
  next settled payload.

### Fake Positions

`fetchPositions` with specific symbols: If a requested symbol has no open position, metadata (leverage, margin type, position mode) is fetched via `fetchPositionMetadata` and a "fake" position with `contracts=0` is created. Results are deduplicated with `uniqBy`. While auth-failure backoff is latched, skip fake-position metadata entirely and return the existing store slice; rejected keys cannot produce useful leverage/mode readback.

### Position Conflict Detection

`mapPositions`: Detects when isolated and cross positions (or mixed net/hedged) exist simultaneously for the same symbol, and emits an error instructing the user to close one on the BloFin site.

## Testnet/Demo

Demo trading is wired at the API level but **disabled in exchange metadata** (`supportsTestnet: false`).
BloFin documents a separate demo environment, so dormant testnet routing selects
`https://demo-trading-openapi.blofin.com` for REST and that host's `/ws/public`
and `/ws/private` sockets. It does not add the obsolete
`x-simulated-trading` header or append a `brokerId=9999` query parameter to
production hosts. Do not enable the metadata flag without validating the full
account and order lifecycle against demo trading.

## Known Quirks / Implementation Notes

1. **OKX-like API**: The API structure (endpoints, WebSocket format, field names) closely mirrors OKX. Comments in the code reference OKX patterns.
2. **No batch algo orders**: Unlike normal orders which support batch (up to 20), algo (TP/SL) orders must be placed individually.
3. **Algo cancel delay**: After canceling algo orders in batches of 10, there is a 1-second sleep between batches (see `cancelAlgoOrders`).
4. **Order fetch retry**: `fetchOrders` has built-in retry logic (3 attempts with exponential backoff `sleep(1000 * attempt)`). On complete failure, returns existing store orders to prevent data loss (see `fetchOrdersWithRetry`).
5. **Recursive order pagination (cursor-until-empty)**: `fetchNormalOrders`, `fetchAlgoOrders`, and read-only `fetchTriggerOrders` page via the `after` cursor (last `orderId`, `tpslId`, or `algoId`) until an **empty page** comes back — _not_ until a page smaller than the requested `limit`. BloFin's pending-order endpoints default to small pages even when a larger limit is requested, so the old "stop when `length !== PAGE_SIZE`" check could silently drop later orders. Loops are bounded by `BLOFIN_MAX_ORDER_PAGES` (50). Every family treats an invalid/non-advancing cursor or reaching the cap before an empty page as a failed snapshot; the outer retry then preserves the existing store if all attempts fail.
6. **TP/SL as combined orders**: When placing a TP or SL on a position with no amount (full position), the code checks for existing opposite algo orders, cancels them, and combines both into a single TP/SL order.
7. **Position conflict detection**: `mapPositions` detects when isolated and cross positions exist simultaneously for the same symbol and warns the user.
8. **Throttled order sync**: Private WebSocket order events trigger a debounced (2-second) full order re-fetch to ensure state synchronization.
9. **Mid-market candle updates**: The `books5` channel is subscribed alongside candle channels to provide faster mid-market price updates for the chart. Uses median of top bid, last candle close, and top ask.
10. **HTTP timeout**: REST timeout is set to 20 seconds (higher than typical) to accommodate batch leverage requests.
11. **Cache-busting on markets**: `fetchMarkets` appends `?${nanoid()}` to the URL to prevent caching.
12. **Leverage in isolated+hedged mode**: Must set leverage separately for long and short sides. The two POSTs are fired in **parallel** via `Promise.all` to fit inside the worker-call timeout. After a confirmed success, the store is patched optimistically (positions + `fetchedLeverage`) and the WS `positions` topic delivers the authoritative value shortly after — there is no follow-up `fetchPositions` call. See "Leverage" subsection under "Position & Margin Configuration" for the full design.
13. **Position side fallback**: `mapPositions` defaults to `PositionSide.Long` if position side is unrecognized, with a comment "FALLBACK TO LONG POSITION IF SENT FROM OKX".
14. **Dispose cleanup**: Private WS `dispose()` clears the throttle timeout before calling parent dispose (see `blofin.ws-private.ts dispose`).
15. **afterMethodCalledInWorker**: Updates local `_accountPositionMode` and `_accountMarginType` cache when `changePositionMode` or `changeMarginType` is called from the worker thread.
16. **Normal-order TP/SL fallback modeling**: BloFin may echo TP/SL only on `orders` channel records (without `tpslId`). `addInlineTpSlFallbackOrders` (run inside `fetchOrdersWithRetry`) synthesizes virtual protective legs from normal-order trigger fields unless a linked algo leg already exists. `mapOrders` itself does NOT synthesize protective legs from normal records — see the transient WS-only window note under "Attached TP/SL Representation".
17. **Batch placement for attached TP/SL**: Inline TP/SL on normal orders stays on `/trade/batch-orders` for speed; only reduce-only compatibility mode can force `/trade/order`.
18. **Bounded incremental fills**: Private order events pass cumulative
    `filledSize` and `filledSize × averagePrice` through `utils/incrementalFills`.
    Each event reports only that execution's base amount and price, without
    floating-point subtraction noise. The shared tracker retains up to 500
    orders, including terminal rows, so replayed filled frames emit nothing.
19. **Opening trigger writes are single-row**: BloFin publishes no batch
    placement/cancellation contract for `/order-algo` and `/cancel-algo`.
    `placeOrders` retains ordinary/TP-SL batching but serializes Conditional
    writes. Trigger cancels are paced at 350 ms to remain below the documented
    30 trade requests per 10 seconds per-user limit.

## Error Handling

### API Response Format

BloFin API success payloads are not perfectly uniform, but generally follow:

```json
{
  "code": "0",
  "msg": "success|",
  "data": { ... }
}
```

- `code === "0"` indicates success
- Error responses have non-zero `code` and descriptive `msg`
- Per-order placement rows may use `msg: "success"` or `msg: "Order placed"`; both are treated as successful placements in adapter parsing.

### Known Error Codes

| Code     | Description                                            |
| -------- | ------------------------------------------------------ |
| `152406` | Your IP is not included in your API key's IP whitelist |
| `152401` | Access key does not exist                              |

Both of those are **configuration faults that retrying cannot fix**, so the
response interceptor in `blofin.api.ts` feeds them to
`BaseExchange.recordAuthFailure()` and the tick loop backs off exponentially
(capped at 60s). Three details are load-bearing:

- **The tick loop must schedule off `getTickInterval()`, not a hardcoded
  `10_000`.** It used to hardcode the interval, so the backoff had no effect and
  a dead key was polled at full rate forever — the e2e bad-key bound caught it
  as 132 HTTP requests against a limit of 100. The metadata `tickInterval` is
  also `10_000`, so reading it changes nothing in the healthy case.
- **Only a _signed_ success clears the streak.** Public endpoints return
  `code: "0"` with no credentials involved, so counting them would immediately
  cancel the backoff for a rejected key. The interceptor gates on the
  `ACCESS-SIGN` header being present.
- **Startup must not immediately enter another full tick after auth rejection.**
  The initial private snapshot already proves the key is bad; schedule the next
  attempt through the backed-off tick interval instead of doubling the startup
  request burst.

Unrelated non-zero codes (e.g. `400` from `set-position-mode`) must NOT record
an auth failure — they are real rejections but not credential problems.

### Retry Strategies

- **Network errors**: Axios retry interceptor retries 3 times on network errors (see `blofin.api.ts`)
- **`fetchOrders`**: Custom retry with 3 attempts and exponential backoff (`sleep(1000 * attempt)`). On total failure, returns existing store orders (see `fetchOrdersWithRetry`)
- **`fetchPositionsHelper`**: On error, returns existing store positions for the symbol (or all positions) to prevent data loss
- **`fetchMarkets`**: On error, returns existing store markets
- **`fetchTickersRaw`**: On error, returns empty array
- **`fetchBalanceAndPositions`**: On error, returns existing store balance and positions

### Error Propagation in Orders

- `placeOrderBatch`: Checks top-level `code` and per-order `msg` fields. Treats `success`, `Order placed`, and empty/undefined row messages as successful; throws on other row messages.
- `cancelNormalOrders` / `cancelAlgoOrders` / `cancelTriggerOrders`: Throw on HTTP errors and on genuine API error codes — **except** rows the exchange reports as already gone (per-row `code:"1000"` / message "Cancel failed as the order has been filled, triggered, canceled or does not exist"). Those are reconciled out of the store via `removeOrders` and treated as canceled, not thrown (see `isAlreadyGoneCancelRow`). Throwing on already-gone rows made flatten/cancel-all loops spin forever — the stale store order never left, so each retry re-issued a cancel that always "failed" (e2e cleanup timeout on blofin). The cancel only tolerates an already-gone result when its exact `orderId`, `tpslId`, or `algoId` correlates to the requested/store identity. Every other partial, missing, or mismatched row throws.

## Testing / Validation

- `validateAccount`: Tests API access by calling the `BALANCE` endpoint with `accountType=futures`. Returns empty string on success, error message on failure.

## Broker ID

- **Value**: `b3cdedc0a20880ba`
- Applied to the payload `brokerId` field and as the unconditional prefix for
  generated `clientOrderId` values (truncated to 32 chars)
- Only the payload `brokerId` field is gated to production or
  `tealstreetProxy`; the client-order-ID prefix is always applied
- The Rust bridge passes this environment/proxy decision as `brokerAttribution`;
  native callers set `Options.broker_attribution` for the same policy. It applies
  to normal, trigger and protective placements, including replacements.
- Format: `clientOrderId = "${BROKER_ID}${uuid}".slice(0, 32)`

## Tealstreet Proxy

When `tealstreetProxy` is enabled:

- **Private WS** resolves its tunnelled or direct host through
  `src/transport/tealstreet/routing/resolveExchangeRouting.ts`.
- **REST** resolves the configured CORS proxy through the routing table and
  `src/transport/tealstreet/transportResolution.ts` inside `safeCexAxiosCreate`.

Blofin does **not** use the HTTP-over-WebSocket adapter for REST requests. In proxy/tunnel option mapping, Blofin is handled as proxy-first routing (`supportsTunnel: false` at metadata level).

## Position & Margin Configuration

### Position Mode

- **One-Way (net_mode)**: Single position per symbol. API value: `net_mode` / positionSide: `net`
- **Hedged (long_short_mode)**: Separate long/short positions. API value: `long_short_mode` / positionSide: `long` or `short`
- Position mode is **global** (applies to all symbols)
- Changed via `POST /api/v1/account/set-position-mode`
- Treat a successful `set-position-mode` response (`code: "0"`) as the commit
  point. Do not await follow-up position/mode refetches inside
  `changePositionMode`; the browser worker call has a 5s timeout, and BloFin can
  confirm the mode change while the extra refetches still make the UI toast
  "failed" and leave optimistic local mode state behind.

### Margin Type

- **Cross**: Shared margin across positions. API value: `cross`
- **Isolated**: Per-position margin. API value: `isolated`
- Margin type is **global** (applies to all symbols)
- Changed via `POST /api/v1/account/set-margin-mode`
- Warning: Changing margin type with open positions may cause unexpected behavior
- A non-zero response code is checked immediately after the POST. Margin-mode
  and symbol-position refetches run only after BloFin confirms success, so a
  rejected change cannot spend extra reads or mutate the store.

### Leverage

#### Endpoints used

- **Set**: `POST /api/v1/account/set-leverage` — payload `{ instId, leverage, marginMode, [positionSide] }`. `positionSide` is required for isolated + hedged mode; omitted otherwise.
- **Fetch (single or batch)**: `GET /api/v1/account/batch-leverage-info` — accepts a single `instId` or comma-joined list (max 20 per request). Used by both `fetchLeverageForSymbol` (single instId) and `fetchPositionMetadata` (batched).
- **Market max**: `GET /api/v1/market/instruments` — `maxLeverage` field is parsed during `fetchMarkets` into `market.limits.leverage.max` (slider upper bound). Not the user's currently-set leverage.

#### `fetchLeverageForSymbol` (`blofin.exchange.ts`)

Implements the base-class hook used by `UiStateHandler.handleActiveMarginSymbolChange`. Because Blofin sets `fetchLeverageOnMarginSymbolChange: true`, switching the active margin symbol triggers a single-symbol fetch that populates `store.fetchedLeverage[symbol]`.

Response shape handling:

- **Net mode**: response contains a single row with `positionSide: 'net'`. Both `buyLeverage` and `sellLeverage` are derived from that row.
- **Hedged isolated**: response contains separate rows with `positionSide: 'long'` and `positionSide: 'short'`. Each side maps to the corresponding `buyLeverage` / `sellLeverage`.
- Falls back to `net` row (or first row) if a side row is missing, to avoid leaving either side at `0`.
- The returned `FetchedLeverage` also carries `marginType` and `positionMode`.
  The margin panel uses that as the no-open-position fallback; without it,
  flat BloFin symbols render a blank position-mode selector even though the
  account-global mode was fetched during startup.
- Returns `null` and surfaces the error message via `emitAndLogError` when `data.code !== '0'`.

This is the React layer's no-position-on-symbol fallback — without it, `getCurrentPositionSizeAndLeverage` would land on `1x` and the place-order % slider would jitter when switching to a symbol the user doesn't currently hold.

#### `setLeverage` (lean refactor)

`setLeverage` (`blofin.exchange.ts`) is intentionally streamlined to fit comfortably inside the 5s `callMethodInWorker` budget defined in `base.ts`. CCXT's reference implementation (`ts/src/blofin.ts → setLeverage`) is a single POST with no prerequisite reads; we follow the same shape.

Order of operations:

1. Validate the market exists; clamp `leverage` to `market.limits.leverage` bounds.
2. **No-op skip**: return early if either (a) all live position records for the symbol already report the requested leverage, or (b) there are no position records and `store.fetchedLeverage[symbol]` already matches.
3. Read `marginType` and `positionMode` from the cached account-global values (`getOrFetchMarginType()` / `getOrFetchPositionMode()` with no symbol arg). Both are populated during `start()` so this is in-memory.
4. POST to `SET_LEVERAGE`. In hedged isolated mode, fire long and short POSTs in **parallel** via `Promise.all` (not serial).
5. **Check `data.code !== '0'` BEFORE any store mutation**, so an exchange-level error doesn't leave the store in an inconsistent state.
6. Optimistically patch `store.positions` for the symbol (clone existing records with new `leverage`) via `addOrUpdatePositions`, and update `store.fetchedLeverage[symbol]` so the no-position fallback reflects the change immediately. The WS `positions` topic delivers the authoritative value shortly after.

What `setLeverage` deliberately does NOT do:

- Does not require a `Position` to exist beforehand (Blofin's API doesn't need one). The previous "Position not found on Blofin" throw was overly strict and added 1–4 REST calls of latency before the actual operation.
- Does not call `fetchPositions([symbol])` after the POST. The previous follow-up fetch could blow the worker timeout (each call did a fresh margin/positionMode/leverage prefetch). The optimistic patch + WS reconciliation replaces it.
- Does not re-await `getOrFetchMarginType(symbol)` inside each POST body — uses the single resolved `marginType` in scope.

#### Leverage preservation in `mapPositions`

Blofin occasionally pushes partial position events without a usable `leverage` field. To avoid zeroing a known-good leverage value (which would cause the React fallback in `getCurrentPositionSizeAndLeverage` to snap to `1x` and jitter the place-order % slider), `mapPositions` resolves leverage with this priority:

1. `parseFloat(p.leverage)` if finite and `> 0`.
2. Otherwise, look up the existing position by `id` (built via `createPositionId(symbol, side, positionMode)`) and reuse its leverage if `> 0`.
3. Otherwise, fall back to `store.fetchedLeverage[symbol]` (per-side in hedged mode) when present and not `isFetching`.
4. Otherwise, leave at `0` (preserves the original "no leverage available" semantics).

`mapPositions` applies the same preserve-on-blank rule to
`liquidationPrice`: numeric values (including explicit `0`) are accepted,
but blank/missing fields reuse the existing position's liquidation price.

#### `tick` uses `addOrUpdatePositionsFull` (preserves leverage on closed symbols)

The 10-second `tick` previously did a wholesale `store.update({ positions })` with the result of `fetchPositions()`. Because that REST call only returns OPEN positions, every tick wiped the empty position records the WS layer (`handlePositionTopic`) intentionally keeps around — destroying their preserved leverage and again causing the % slider to fall back to `1x` between tick and the next WS update.

`tick` now calls `this.store.addOrUpdatePositionsFull(positions)` (matching the WS handler's semantics). Closed-symbol records are zeroed in contracts/notional/upnl/liq via `createEmptyPositionFrom`, but `leverage`, `marginType`, `side`, `positionMode`, `id`, and `symbol` survive.

The initial bootstrap inside `start()` is intentionally left as a wholesale `store.update({ positions })` because there is nothing to preserve at that point.

#### No separate WS leverage stream

Confirmed against CCXT pro (`ts/src/pro/blofin.ts → handlePositions` / `parsePosition`): there is no dedicated leverage WS topic on Blofin. Both we and CCXT extract leverage from the `leverage` string field of the `positions` channel payload. The private WS subscriber list in `blofin.ws-private.ts` is correct as-is.

## Timeframe Intervals

Defined in `INTERVAL` mapping (BloFin uses uppercase for hours/days/weeks):

| Internal | BloFin API |
| -------- | ---------- |
| `1m`     | `1m`       |
| `3m`     | `3m`       |
| `5m`     | `5m`       |
| `15m`    | `15m`      |
| `30m`    | `30m`      |
| `1h`     | `1H`       |
| `2h`     | `2H`       |
| `4h`     | `4H`       |
| `6h`     | `6H`       |
| `12h`    | `12H`      |
| `1d`     | `1D`       |
| `1w`     | `1W`       |

## OHLCV Candle Fetching

- Max limit: 1440 candles per request
- Candle array format: `[timestamp, open, high, low, close, ?, ?, volume]` (index 7 is volume)
- Timestamps from API are in milliseconds, divided by 1000 for internal use
- Candles are sorted by timestamp ascending after fetch

## Grafana Monitoring

Endpoints are categorized for monitoring:

- **Every request** (logged individually): All trading, leverage, and margin mode endpoints
- **Periodic** (aggregated): Balance, positions, unfilled orders, unfilled algo orders

## Polling / Tick Loop

The `tick()` method runs every 10 seconds, fetching:

1. Balance and positions (via `fetchBalanceAndPositions`)
2. Orders (via `fetchOrders` with 3 retries and exponential backoff)

Both fetches are independent -- one failing does not block the other. The tick loop schedules itself via `loop(() => this.tick(count + 1), 10_000)`.

Positions are committed to the store via `addOrUpdatePositionsFull` (matching the WS handler) rather than a wholesale `store.update({ positions })`. This preserves closed-symbol position records and their leverage between ticks, which the place-order % slider's no-position fallback depends on. See "Leverage" under "Position & Margin Configuration".

## CCXT Reference Implementation

For implementation inspiration and reference, consult the CCXT library's BloFin integration:

- **REST API Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/blofin.ts
- **WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/blofin.ts

Notable cross-references for leverage handling:

- `blofin.ts → setLeverage`: single POST with no position prerequisite — our lean `setLeverage` mirrors this shape.
- `blofin.ts → fetchLeverage` / `fetchLeverages`: REST-only reference. BloFin's
  official docs now mark single-instrument `account/leverage-info` deprecated;
  this integration uses only `account/batch-leverage-info`, including for a
  single symbol.
- `pro/blofin.ts → handlePositions` / `parsePosition`: confirms there is no separate WS leverage stream; leverage is read from the `leverage` field of the `positions` channel payload, identical to our private WS handler.

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `clientOrderId`, rule `^[A-Za-z0-9]{1,32}$`, on `placeOrder` and on each regular order of `placeOrders`. Replaces the generated `BROKER_ID…` ID; no prefix is composed.

## Rust engine account safety

The Rust venue lives at `sdk-rs/venues/blofin`. It retains each large REST
response's status envelope while streaming its data rows, so a non-zero API
code or a failed page cannot commit as an empty authoritative snapshot.
Snapshot revisions prevent older REST data from overwriting newer socket events.

Two defensive corrections to the TS behavior are recorded in the replay gate:
malformed successful mode reads do not silently initialize cross/one-way mode,
and an empty position push clears used margin and unrealized PnL. Per-market
settings retain leverage and margin when positions close. Cumulative fills keep
quantity and traded-value watermarks for 500 orders, including terminal rows.
Startup execution reporting waits for the first complete pending-order census;
up to 500 early cumulative socket observations wait for that execution baseline.
A complete REST prefix can prove a buffered suffix even when newer socket state
prevents that REST snapshot from replacing the order store. Each proven suffix
uses the difference in cumulative traded value, preserving its execution price.
A native `createTime` at or after private startup proves that a newly filled
order has no pre-session prefix and may report immediately. For older or unknown
orders, an unprovable startup prefix is a quiet baseline; missing historical
quote value also waits for a known value before reporting further suffixes.
Reporting starts after this baseline, so later REST polls cannot consume live
socket executions. An existing order absent from the census has no proven
prefix even if its first socket update arrives after readiness: it seeds a
baseline instead of replaying historical executions. An unprovable prefix
updated after reporting readiness, or any suffix with missing cumulative quote
value, emits an order-specific error so an unresolved live execution gap is
visible. These boundaries deliberately avoid inventing quantities or prices from
cumulative rows; they do not promise recovery of executions whose prefix is
unknown. Once a new cumulative quote baseline is known, later proven suffixes
continue normally without replaying the unresolved gap. Invalid live cumulative
quotes retain the last proven prefix, including in startup buffering and bounded
queue eviction, so a later valid quote can recover the complete proven interval.
An invalid pre-session quote instead advances a quiet historical quantity floor
with unknown quote basis, preventing later rows from replaying that history.

Rust private startup is gated on the margin-mode and position-mode reads. Until
each loads, any failure (429, 5xx, transport, non-JSON, malformed row, unknown
code, a read raced by a settings write) retries that read through
`startup::Backoff` (1 s doubling to 30 s, forever); a 418/429 waits at least its
`Retry-After`, else 60 s (BloFin's one-minute limit window), and holds the other
waiting settings read too. The 10 s tick no longer re-reads them. Only documented key refusals end startup with one
`StartupFailed`: 152401/152408 (`AuthInvalid`), 152402 (`AuthExpired`), 152406
(`AuthPermission`), and 152409 after one re-signed resend; 152405/152407/152410
(timestamp/nonce) and 152404 stay transient. Before the account loads, a socket
login refused with one of those codes fails startup too; BloFin's generic
`60009 Login failed` closes and reconnects with backoff, now immediately rather
than at the auth deadline. A rejection clears the settings retries and the tick
and retires the private socket; a refused trading key also emits `Deactivate`.
A missing passphrase or invalid read-only credentials fail startup with
`InvalidInput`.

Rust commands preflight an entire batch before I/O. Cancel responses must cover
exactly the requested native identities even when the top-level code is zero;
per-row failures are never swallowed. Cancel-replace keeps attached trigger
references and execution prices and gives each replacement a new client ID.
Positive partial protection sizes that round to zero are rejected, and only an
explicit zero selects full-position semantics. Unsupported time-in-force shapes
are rejected. Split sizes conserve native lot units. These defensive corrections
to legacy TS behavior are listed in the Rust gate and commit messages.

Rust fills queries walk cursors past newer records when necessary: BloFin can
ignore the requested time window on fills history. They return actual executions
only; the TS fallback that treats order-total history as executions is not used.
Income walks fail rather than silently returning a partial ledger after a failed
page or an exhausted pagination bound. Query JSON shapes match the engine's
Binance history surface, including seconds for execution timestamps and
milliseconds for order/income/public-trade timestamps.

Rust closed-position history retains its rows at the 100-page cap and emits
`history.incomplete` with the actual request command before completion, allowing
the host to mark the owning result incomplete and notify only that caller.

Terminal order rows remain until reconciliation, and cancel-all selects only open
orders in both TS and Rust. Rust additionally keeps terminal socket states
monotonic and rejects amending or merging terminal protection, preventing late
frames or retained canceled legs from creating new orders.

Rust startup can publish a partial valid catalogue while public metadata retries.
A REST account scope containing an unmappable nonzero SWAP position or working
SWAP order is rejected visibly and retains the previous scope; it must not
publish a successful empty account. Zero unknown positions and explicitly
non-SWAP products remain outside the owned scope. Full per-row retention is
planned separately from this temporary whole-scope safety guard.

Closed Rust symbols restore their margin setting to the confirmed account default
on both REST and socket full snapshots (including explicit zero rows). Live
positions keep their own margin mode; closing one must not make the next opening
order inherit that old mode. Leverage remains cached across the close.

The Rust login timestamp is a JSON integer, and its exact decimal text is used
in the WS signature. BloFin's documented TPSL/algo examples contain null unused
protection fields: Rust treats these as absent, likewise nullable client IDs,
attachments and position leverage/liquidation metadata. Required quantities and
identities are not defaulted from null. A malformed socket row is logged while
valid siblings continue; malformed positions retain that symbol's known legs
and account totals. A row without a usable position identity preserves the whole
snapshot. Required-field corruption in REST private snapshots remains atomic.
Initial ticker rows are independent: malformed rows are reported and skipped so
other tickers and private startup can progress. The captured public tape currently
has no null values, so null-quantity tests describe defensive behavior rather than
claiming a captured exchange payload.
