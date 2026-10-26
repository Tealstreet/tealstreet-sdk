# Bitunix Exchange Integration

## Exchange Information

- **Type**: Centralized Exchange (CEX), Futures only
- **Quote currency**: USDT only (markets must be `OPEN`, quote USDT, and advertise `isApiSupported !== false`)
- **Position modes**: Hedged (default) and One-Way
- **Margin types**: Cross and Isolated (`CROSS` / `ISOLATION`)

## API Documentation

- **Official API Documentation**: <https://www.bitunix.com/api-docs/futures/>

## File Structure

| File                    | Description                                                             |
| ----------------------- | ----------------------------------------------------------------------- |
| `bitunix.api.ts`        | REST API client with double-SHA256 signature scheme, axios interceptors |
| `bitunix.exchange.ts`   | Main exchange class extending `BaseExchange`                            |
| `bitunix.types.ts`      | Type definitions, endpoint constants, type mapping objects              |
| `bitunix.utils.ts`      | SHA256 hex hashing utility (uses `@noble/hashes`)                       |
| `bitunix.ws-futures.ts` | Futures-specific WebSocket for klines and orderbook (gzip-compressed)   |
| `bitunix.ws-private.ts` | Private WebSocket for orders, positions, TP/SL, balance (login auth)    |
| `bitunix.ws-public.ts`  | Public WebSocket for tickers, mark prices, depth_book1                  |

## Authentication

### REST API Signature (Double-SHA256)

Bitunix uses a **double-SHA256** signature scheme (NOT HMAC), implemented by `createAPI()`.

1. **Headers sent on every authenticated request**:
   - `api-key`: API key
   - `sign`: Computed signature
   - `nonce`: UUID (generated per request via `uuid()`)
   - `timestamp`: Unix timestamp in milliseconds (from `virtualClock`)

2. **Signature computation**:

   ```text
   paramsStripped = URLparams with '=' and '&' characters removed
   toSign = nonce + timestamp + apiKey + paramsStripped [+ JSON body if POST]
   digest = SHA256(toSign)
   signature = SHA256(digest + secretKey)
   ```

3. **Public endpoint bypass**: URLs containing the string `'public'` skip authentication entirely.

4. **Testnet mode**: Same base URL but adds `x-simulated-trading: 1` header.

5. **Response interceptor** (`bitunix.api.ts`): Logs API errors when response has `msg` but no `data`, unless request-scoped suppression flags are set:
   - `__suppressExchangeError`
   - `__suppressExchangeRateLimitError` (used for `"request too frequently"` polling noise)
     Bitunix often returns HTTP 200 even for failures, so `code`/payload validation is still required.

### Private WebSocket Login

Implemented by `BitunixPrivateWebsocket.sendLogin()`:

```json
{
  "op": "login",
  "args": [
    {
      "apiKey": "<key>",
      "timestamp": "<unix_milliseconds>",
      "nonce": "<uuid>",
      "sign": "SHA256(SHA256(nonce + timestamp + apiKey) + secret)"
    }
  ]
}
```

- Before sending login, calls `virtualClock.syncBybitClientServerTime()` and waits 25ms.
- Login automatically subscribes to `balance`, `order`, `position`, and `tpsl` channels (no explicit subscribe needed)
- On login failure, re-syncs clock, waits 2s, and retries.

### Time Synchronization

- Uses `virtualClock` for all timestamps (REST and WebSocket)
- Before WS login, syncs via `virtualClock.syncBybitClientServerTime()` (note: reuses Bybit's sync mechanism)

## Startup Sequence

Defined by `BitunixExchange.start()`:

1. **`fetchMarkets()`** -- REST call to load all trading pairs. Store updated with markets. Returns early if disposed.
2. **`futuresWebsocket.connectAndSubscribe()`** -- Futures WS started early (needed for orderbook loading).
3. **`fetchTickers()`** -- REST call to load initial ticker data.
4. **Store update** -- Markets and tickers marked as loaded.
5. **`configurePublicWebsocketShards()`** -- Splits supported markets into groups of at most 300 and creates one `ticker` plus one `price` connection per group.
6. **Public WebSocket connect** -- Each shard connects and replays its assigned topics at a paced four subscription frames/second.
7. **`privateWebsocket.connectAndSubscribe()`** -- Private WS with login auth.
8. **`fetchPositionMode()`** -- REST call to determine Hedged vs One-Way mode, updates store setting.
9. **`tick()`** -- First tick: fetches balance, positions. Continues polling.
10. **`fetchOrders()`** -- REST call to load open orders.
11. **`fetchPositions()`** -- Final positions fetch (fire-and-forget, no await).

### Tick Loop

Defined by `BitunixExchange.tick()`:

- Runs on configurable interval (`options.extra?.tickInterval`)
- **Every tick**: Fetches balance (REST), updates positions from balance response
- **Every 32nd tick** (or when position mode changes): Full positions and orders refresh via REST
- On error, logs and continues

## REST API Endpoints

| Constant               | Path                                               | Description                                                                                                                                                                                                                                    |
| ---------------------- | -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MARKETS`              | `/api/v1/futures/market/trading_pairs`             | All trading pairs (public)                                                                                                                                                                                                                     |
| `ACCOUNT`              | `/api/v1/futures/account`                          | Account info, balance, position mode. Current docs show `data` as an array of account rows for the requested `marginCoin`; older/live variants may return a single object.                                                                     |
| `TICKERS`              | `/api/v1/futures/market/tickers`                   | All tickers (public)                                                                                                                                                                                                                           |
| `LEVERAGE_MARGIN_MODE` | `/api/v1/futures/account/get_leverage_margin_mode` | Per-symbol leverage and margin mode                                                                                                                                                                                                            |
| `HEDGE_MODE`           | `/api/v1/futures/account/change_position_mode`     | Change hedge/one-way mode                                                                                                                                                                                                                      |
| `SET_LEVERAGE`         | `/api/v1/futures/account/change_leverage`          | Set leverage for a symbol                                                                                                                                                                                                                      |
| `SET_MARGIN_TYPE`      | `/api/v1/futures/account/change_margin_mode`       | Change cross/isolated margin                                                                                                                                                                                                                   |
| `OPEN_ORDERS`          | `/api/v1/futures/trade/get_pending_orders`         | Fetch open regular orders                                                                                                                                                                                                                      |
| `OPEN_TPSL_ORDERS`     | `/api/v1/futures/tpsl/get_pending_orders`          | Fetch open TP/SL orders                                                                                                                                                                                                                        |
| `ORDER`                | `/api/v1/futures/trade/place_order`                | Place single order                                                                                                                                                                                                                             |
| `MODIFY_ORDER`         | `/api/v1/futures/trade/modify_order`               | Amend an exactly reconciled pending limit order by stable `orderId`; private-WS state remains authoritative                                                                                                                                    |
| `BATCH_ORDERS`         | `/api/v1/futures/trade/batch_order`                | Place batch orders (**hard cap 5 per request** — rejects whole batch with `"batch order max size:5"` if exceeded). Response: `{code, data: {successList:[{orderId, clientId}], failureList:[{errorMsg, errorCode}]}, msg}` — NOT a flat array. |
| `CANCEL_ORDER`         | `/api/v1/futures/trade/cancel_orders`              | Cancel regular orders                                                                                                                                                                                                                          |
| `CANCEL_ORDER_TP_SL`   | `/api/v1/futures/tpsl/cancel_order`                | Cancel TP/SL orders                                                                                                                                                                                                                            |
| `TP_SL_ORDER`          | `/api/v1/futures/tpsl/place_order`                 | Place partial TP/SL order                                                                                                                                                                                                                      |
| `POSITION_TP_SL_ORDER` | `/api/v1/futures/tpsl/position/place_order`        | Place full-position TP/SL                                                                                                                                                                                                                      |
| `HISTORY_TRADES`       | `/api/v1/futures/trade/get_history_trades`         | User fills history (primary for `fetchHistoricalFills`)                                                                                                                                                                                        |
| `KLINE`                | `/api/v1/futures/market/kline`                     | Kline/candlestick data (public)                                                                                                                                                                                                                |
| `POSITIONS`            | `/api/v1/futures/position/get_pending_positions`   | Fetch open positions                                                                                                                                                                                                                           |

**Public endpoints** (no auth): `MARKETS`, `TICKERS`, `KLINE`

Additional TP/SL modify endpoints:

- `MODIFY_TP_SL_ORDER`: `/api/v1/futures/tpsl/modify_order` for standalone
  partial TP/SL rows by raw TP/SL order id.
- `MODIFY_POSITION_TP_SL_ORDER`:
  `/api/v1/futures/tpsl/position/modify_order` for full-position TP/SL rows by
  symbol and `positionId`.

Execution-history behavior (`fetchHistoricalFills`):

- Uses the documented `HISTORY_TRADES` endpoint with symbol/time-window params.
- Defensively parses mixed payload shapes (`data.data`, `tradeList`, `list`, `rows`).
- Normalizes timestamps to Unix seconds and dedupes by order/time/price/qty/side key

## WebSocket Implementation

### Architecture: Sharded Public Feeds Plus Private/Futures Sockets

The exchange always has private and futures sockets. Public feeds are sharded based on the supported-market count:

1. **`publicWebsocket`** (primary) -- Subscribes to `ticker` for the first group of at most 300 markets.
2. **`publicWebsocketSecondary`** -- Subscribes to `price` for the same first group.
3. **`publicWebsocketShards`** -- Adds a ticker/price pair for every additional group of at most 300 markets.
4. **`privateWebsocket`** -- Authenticated; receives `order`, `position`, `tpsl`, `balance` events.
5. **`futuresWebsocket`** -- Uses a different URL base (`api.bitunix.com` vs `fapi.bitunix.com`); handles klines and orderbook with gzip-compressed messages.

### URLs

| WebSocket | Livenet                                                      | Testnet |
| --------- | ------------------------------------------------------------ | ------- |
| Public    | `wss://fapi.bitunix.com/public/`                             | Same    |
| Private   | `wss://fapi.bitunix.com/private/`                            | Same    |
| Futures   | `wss://api.bitunix.com/ws-futures/?from=contract&compress=2` | Same    |

### Fallback URL Logic

All three WS classes share the same fallback policy:

- If `tealstreetWsRoute` option is set, starts with fallback URL
- Otherwise, after **2 failed connection attempts** (MAX_ATTEMPTS) where **0 messages** were received on both the current and previous connection, switches to Tealstreet proxy URLs
- Fallback URLs route through `bitunix-fapi-proxy-ja.tealstreet.io` or `bitunix-api-proxy-ja.tealstreet.io`
- Emits `grafana-info` event when switching to fallback

### Reconnection

All WS classes use `BaseWebSocket.onClose()` which handles reconnection. On close/error:

- Clears ping loop timeout
- Calls `this.onClose()` (from BaseWebSocket) which triggers reconnect

### Ping/Pong

| WebSocket | Ping Format                  | Interval | Pong Behavior                                                                                                                       |
| --------- | ---------------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Public    | `{ "ping": <timestamp_ms> }` | 15s      | Normal pong expected                                                                                                                |
| Private   | `{ "op": "ping" }`           | 15s      | Current docs describe a pong; the client still handles the ping optimistically for compatibility with historically silent sessions. |
| Futures   | `{ "ping": <timestamp_ms> }` | 15s      | Normal pong expected                                                                                                                |

### Public WebSocket Channels

- **`ticker`** -- Last price, open, high, low, base volume, quote volume, percentage. Updates `last`, `percentage`, `volume`, `quoteVolume` on ticker.
- **`price`** -- Mark price, index price, funding rate, funding time. Updates `mark`, `index`, `fundingRate`, `fundingTime`.
- **`depth_book1`** -- Best bid/ask only (top of book). Updates `bid`, `ask` on ticker.

**Subscription limits**: Bitunix documents at most 300 channel subscriptions per connection and five client messages per second. Public topics are sharded at 300, batched in groups of 20, and emitted every 250ms (four subscription frames/second, leaving room for ping traffic). `onOpen()` tracks the market topics once and the shared queue sends them once; do not reintroduce the former send-then-replay double subscription.

### Private WebSocket Channels

Auto-subscribed on successful login:

- **`balance`** -- Balance update. Does NOT contain uPNL, so triggers a throttled REST fetch.
- **`order`** -- Events: `CREATE`, `UPDATE`, `CLOSE`. Processes order data into store and triggers throttled REST orders fetch.
- **`position`** -- Events: `OPEN`, `UPDATE`, `CLOSE`. Updates `positionIdCache` from WS events. On `CLOSE`, deletes from cache. Triggers throttled REST positions fetch.
- **`tpsl`** -- Events: `CREATE`, `UPDATE`, `CLOSE`. Processes TP/SL orders with compound IDs. Always triggers throttled REST orders fetch.

### Futures WebSocket

- Messages arrive as **gzip-compressed Blobs**, decompressed with `pako.inflate()`.
- Subscribes to system channels on open: `system_notice`, `reconnected`, `onconnected`
- **Kline channel**: `market_<symbol_lower>_klineop_<interval>` (e.g., `market_btcusdt_klineop_60min`)
- **Orderbook channel**: `market_<symbol_lower>_depthop_<precision>` (e.g., `market_btcusdt_depthop_0.1`)
- Kline data updates ticker `last` price as a side effect.
- The legacy `depthop` feed sends complete, already sorted books. A read-only production frame captured on 2026-07-15 had bids descending and asks ascending; the parser preserves that order and only computes cumulative totals.

### Kline Interval Mapping

| Tealstreet | Bitunix |
| ---------- | ------- |
| `1m`       | `1min`  |
| `3m`       | `3min`  |
| `5m`       | `5min`  |
| `15m`      | `15min` |
| `30m`      | `30min` |
| `1h`       | `60min` |
| `2h`       | `2h`    |
| `4h`       | `4h`    |
| `6h`       | `6h`    |
| `12h`      | `12h`   |
| `1d`       | `1day`  |
| `1w`       | `1week` |

### Throttled REST Fetches from WS Events

The private WebSocket uses 1-second debounced REST fetches:

- `throttledFetchBalance()` -- On `balance` WS event
- `throttledFetchOrders()` -- On any `order` or `tpsl` WS event
- `throttledFetchPositions()` -- On any `position` WS event

Each uses `clearTimeout` + `setTimeout(fn, 1000)` pattern to debounce rapid successive events.

## Order Types and Mappings

### Order Type

| Internal Type                | Bitunix API Value      | Notes                             |
| ---------------------------- | ---------------------- | --------------------------------- |
| `OrderType.Limit`            | `LIMIT`                |                                   |
| `OrderType.Market`           | `MARKET`               |                                   |
| `OrderType.StopLoss`         | `STOP_MARKET`          | Placed via TP/SL endpoints        |
| `OrderType.TakeProfit`       | `TAKE_PROFIT_MARKET`   | Placed via TP/SL endpoints        |
| `OrderType.TrailingStopLoss` | `TRAILING_STOP_MARKET` | **Not supported** -- throws error |

### Time In Force

| Internal TIF        | Bitunix Value |
| ------------------- | ------------- |
| `GoodTillCancel`    | `GTC`         |
| `ImmediateOrCancel` | `IOC`         |
| `FillOrKill`        | `FOK`         |
| `PostOnly`          | `POST_ONLY`   |

### Trigger Type

| Bitunix Value | Internal Type                    | Context           |
| ------------- | -------------------------------- | ----------------- |
| `MARK`        | `OrderStopTriggerType.MarkPrice` | REST docs example |
| `MARK_PRICE`  | `OrderStopTriggerType.MarkPrice` | REST API          |
| `LAST`        | `OrderStopTriggerType.LastPrice` | REST alias        |
| `LAST_PRICE`  | `OrderStopTriggerType.LastPrice` | REST API          |
| `Mark`        | `OrderStopTriggerType.MarkPrice` | WebSocket         |
| `Last`        | `OrderStopTriggerType.LastPrice` | WebSocket         |

**Default**: When trigger type is omitted (by Bitunix), code defaults to `OrderStopTriggerType.MarkPrice`.

### Order Side

| Bitunix | Internal         |
| ------- | ---------------- |
| `BUY`   | `OrderSide.Buy`  |
| `SELL`  | `OrderSide.Sell` |

Position sides from API: `LONG`/`SHORT` or `BUY`/`SELL` (endpoint-dependent, handled in `fetchPositions`).

## Order Lifecycle

### Order Placement (`placeOrder` / `formatCreateOrder`)

1. **TrailingStopLoss**: Throws immediately -- not supported.
2. **TP/SL orders** (`StopLoss`/`TakeProfit`): Routed to `formatTPSLOrder()`.
3. **Regular orders**: Market orders omit `price`. Limit orders include `price`.
4. **Client ID**: Generated as `ts_<uuid>` prefix.
5. **`tradeSide`**: `OPEN` for new positions, `CLOSE` for reduce-only.
6. **`positionId`**: Required for reduce-only orders. Sourced from `position.info.positionId` or `positionIdCache`.
7. **Large orders**: Split into lots if `amount > maxSize`. Each lot is `lotSize`, remainder added as final payload.
8. **Inline TP/SL**: Limit orders can carry `tpPrice`/`slPrice` parameters for bracket orders.
9. **TIF caching**: Stores `clientId -> timeInForce` in `orderTifCache` for REST API workaround.

### TP/SL Order Placement (`formatTPSLOrder`)

Two distinct paths based on order amount:

- **Full-position TP/SL** (`amount === 0` or unspecified): Uses `POSITION_TP_SL_ORDER` endpoint. Preserves existing counterpart (e.g., when placing TP, keeps existing SL).
- **Partial TP/SL** (specific amount): Uses `TP_SL_ORDER` endpoint with `tpQty`/`slQty`.

Both require `positionId`. Prefers WS-cached positionId over REST-polled `position.info`.
Before formatting a TP/SL or reduce-only payload, `placeOrder`/`placeOrders`
silently refresh positions once when a live position exists but has no
`positionId`; only the post-refresh preflight miss should emit/log. This keeps
the expected market-fill-before-positionId hydration race out of Sentry while
still surfacing persistent identity failures.
For full-position TP/SL (`amount: 0`), placement also refreshes open orders
before formatting so adding one side includes any live sibling side in the
position TP/SL payload. Omitting the sibling can clear it on Bitunix.

### Batch Order Processing (`placeOrderBatch`)

Orders are separated into three categories:

1. **Position TP/SL orders** (`isPositionOrder` flag) -- Sent individually to `POSITION_TP_SL_ORDER`
2. **Regular TP/SL orders** (have `tpPrice`/`slPrice` but not `LIMIT` type) -- Sent individually to `TP_SL_ORDER`
3. **Regular orders** -- Grouped by symbol, then chunked into batches of **5** (Bitunix's hard cap; >5 → `"batch order max size:5"`):
   - Single order: `POST` to `ORDER` endpoint
   - Multiple orders: `POST` to `BATCH_ORDERS` endpoint. Response shape is `data.data = {successList: [{orderId, clientId}], failureList: [{errorMsg, errorCode}]}` — NOT a flat array. The parser reads from `successList` for IDs and surfaces `failureList[].errorMsg` per rejected leg. A historical flat-array fallback is kept for unit-test mocks. **Top-level `data.code !== 0` is also validated before parsing** — Bitunix returns HTTP 200 for whole-batch rejections (rate limit, server reject) with `{code, msg}` and no successList/failureList; without this guard the parser falls through with empty IDs and the caller sees the misleading "no order IDs returned" symptom.
   - **1 s `sleep` between chunks.** `unlimitedXHR` has no client-side rate limit (kept fast for the single-order path), so back-to-back batch POSTs (e.g. 19 legs → 4 batches) hit Bitunix's server-side limiter with `"request too frequently"`. Both 150 ms and 500 ms still hit the limiter empirically; 1 s keeps the effective rate at ~5 orders/sec and clears it. **Do not auto-retry on the rate-limit response** — a `"request too frequently"` reply can arrive after Bitunix has already accepted the underlying order, so a retry risks double-placement. Tune the inter-chunk sleep instead. Only sleep between chunks, never before the first.

All TP/SL orders use `unlimitedXHR` (no rate limit). Checks `data.code !== 0` for error detection since Bitunix returns HTTP 200 for errors.

### Order Update (`updateOrder`)

`updateOrder` uses the native
[`/api/v1/futures/trade/modify_order`](https://www.bitunix.com/api-docs/futures/trade/modify_order.html)
endpoint only when identity and immutable attributes are exactly reconciled:

- The order must be a pending `LIMIT` with `info.orderId === order.id`, an
  upstream `LIMIT` type, and a raw boolean `reduceOnly` value matching the
  normalized order. The endpoint does not accept side, trade side, TIF, or
  reduce-only fields, so those attributes remain attached to the same order.
- In the web app's main thread, worker store snapshots deliberately strip raw
  `order.info` before `updateOrderById` reads the row. When raw `info` is absent
  altogether, Bitunix native amend falls back to the normalized `order.id`,
  `order.type`, and `order.reduceOnly` that came from the worker snapshot. If raw
  `info` is present but contradicts those fields, the stricter reconciliation
  guard still rejects before any exchange write.
- Bitunix's private order channel omits `reduceOnly` and can omit other
  immutable/protection fields on `UPDATE`. Tealstreet caches submitted
  reduce-only state by `clientId`, and WS updates merge onto the existing raw
  snapshot instead of replacing it, so repeated chart drags retain
  reconciliation provenance.
- Quantity is the requested total base-asset quantity, including after partial
  fills, rounded to market precision. It must remain above the already-filled
  quantity.
- Attached virtual `__tp` / `__sl` price edits amend the stable parent
  `orderId`. The request carries both reconciled protection legs, trigger
  types, execution types, and any required limit execution prices so editing
  one leg cannot drop its sibling. A virtual-leg quantity edit is rejected.
- The request includes `symbol` as shown in Bitunix's official example, in
  addition to the documented identity, price, and quantity fields. A
  successful HTTP response must have `code: 0`; when it returns a different
  `orderId`, that replacement parent ID is propagated rather than treated as a
  post-mutation error. Attached virtual edits rebuild their `__tp` / `__sl`
  display ID on that acknowledged parent so optimistic UI state cannot overlay
  a protection row onto the regular order. Bitunix explicitly says HTTP
  acknowledgement is not authoritative, so the existing private order-channel
  refresh still owns final state reconciliation.

There is no broad cancel/recreate fallback. Market orders, missing or
inconsistent raw order metadata, and malformed prices or quantities reject
before any exchange write. The one exception is an attached virtual TP/SL chart
drag where `/trade/modify_order` returns Bitunix's venue-side "The order does
not support this operation": the parent limit order is canceled and recreated
with the requested attached TP/SL trigger plus any market-execution sibling.
Do not use that fallback for ordinary parent price/quantity edits, and do not
silently recreate attached LIMIT-execution protection because `PlaceOrderOpts`
cannot preserve the execution limit price. Standalone partial TP/SL rows route
through `MODIFY_TP_SL_ORDER` with the raw Bitunix TP/SL id retained in
`order.info`; full-position TP/SL rows (`info.type === "POSITION_TPSL"` or
`amount === 0`) route through `MODIFY_POSITION_TP_SL_ORDER` and preserve the
sibling leg when it is present in the store.

### Virtual Order ID Generation

For orders that contain attached TP/SL:

- Parent order: Uses exchange-assigned `orderId`
- Virtual TP: `{orderId}__tp`
- Virtual SL: `{orderId}__sl`

For standalone TP/SL rows from REST or the `tpsl` WS channel with both TP and
SL on one raw Bitunix TP/SL order id:

- TP order: `{orderId}__{positionId}__tp`
- SL order: `{orderId}__{positionId}__sl`
- The raw Bitunix TP/SL id is retained in `order.info.id` and
  `order.info.orderId`. Chart display IDs must stay distinct so the TP and SL
  lines do not collapse into one `OrderLineManager` entry, but exchange writes
  that target TP/SL endpoints must send the raw id.

### Cancel Orders (`cancelOrders`)

Complex logic handling four order categories:

1. **Virtual TP/SL orders**: Identifies parent order, determines which TP/SL to keep, cancels parent, recreates if needed.
2. **Standalone TP/SL orders**: Canceled via `CANCEL_ORDER_TP_SL` endpoint (one at a time).
3. **Regular limit/market orders**: Grouped by symbol, canceled via `CANCEL_ORDER` endpoint with `orderList` array.
4. **Parent recreation**: If only one side (TP or SL) of a virtual order is being canceled, the parent is recreated with the remaining side preserved.

## Symbol Format

- API format: `BTCUSDT` (base + quote concatenated)
- Internal format: Same -- no transformation needed
- Market ID format: `BTC/USDT` (with slash)
- Markets are filtered to `quote === 'USDT'`, `symbolStatus === 'OPEN'`, and API-trading support. The official schema defines `isApiSupported: false` as API trading disabled, so these symbols fail closed and are not surfaced as tradable.
- Parse `isApiSupported` defensively, not as strict boolean-only data. Bitunix has drifted response shapes elsewhere, so string/number false values (`"false"`, `"0"`, `0`, etc.) must fail closed too. Missing values stay allowed for older payloads.

## Data Precision & Conversion

### Price and Amount Precision

- **Price precision**: From `quotePrecision` via `parsePrecision()`.
- **Amount precision**: From `basePrecision` via `parsePrecision()`.
- **Rounding**: Uses `adjust()` from `safe-math` for both price and amount

### Order Size Limits

- `maxLimitOrderVolume` from market data used as `limits.amount.max`
- Max leverage hardcoded to 100 in market limits (regardless of API `maxLeverage` field)

### Balance Calculation

`fetchBalance()` selects the `USDT` account row from either an object response or the documented account-array response:

```text
free = available
upnl = crossUnrealizedPNL + isolationUnrealizedPNL
used = margin
total = free + used + frozen
```

Malformed or missing numeric fields are treated as `0` so a payload drift cannot poison the balance store with `NaN`.

### Ticker Data

- `bid` and `ask` set to `last` from REST (updated to real values from `depth_book1` WS)
- `percentage` is derived as `((last - open) / open) * 100`; the REST response has no `priceChangePercent` field.
- `index` set to `markPrice` from REST (updated separately from `price` WS channel)
- `fundingRate` and `fundingTime` are 0 from REST (updated from `price` WS channel)
- `openInterest` always 0

### OHLCV

- Kline timestamps are in milliseconds from API, divided by 1000 for internal use.
- Volume uses `quoteVol` field
- Max limit: 200 candles per request
- Timeout: 5 minutes for kline requests

## Rate Limiting

- **Primary `xhr`**: 5 requests/second (`maxRPS: 5` via `axios-rate-limit`)
- **`unlimitedXHR`**: No rate limit (used for order placement to avoid delays)
- **Default timeout**: 20,000ms (comment notes this is needed for leverage batch operations)
- **Retry**: 3 retries on network errors via `axios-retry` — reads only; a signature rejection (`10007`) is re-signed and resent once, writes included (`utils/signatureRejection.ts`)
- **HTTP adapter**: Uses `axiosHttpOverWebsocketAdapter` for proxy support
- **Subscription batching**: Public subscriptions are grouped by 20, paced every 250ms, and sharded at 300 topics per connection.

## State Reconciliation

### Position ID Cache

`positionIdCache: Map<string, string>` (key format: `"SYMBOL:side"`)

- **Populated from**: WS `position` events on `OPEN`/`UPDATE`.
- **Cleared on**: WS `position` `CLOSE` event (deletes entry)
- **Purpose**: Survives store serialization where `position.info` may be lost
- **Lookup priority** (`getPositionId`): `position.info.positionId` first, then cache fallback.
- **TP/SL priority** (`formatTPSLOrder`): Cache first (more recent from WS), then `position.info` fallback.
- **REST behavior**: `fetchPositions()` keeps REST `positionId` in `position.info` but intentionally does not populate `positionIdCache`; REST can lag behind WS after close/reopen.

### Order TIF Cache

`orderTifCache: Map<string, OrderTimeInForce>` (keyed by `clientId`)

- **Populated from**: Order placement and WS `order` `CREATE` events.
- **Purpose**: Workaround for REST API bug where `get_pending_orders` always returns `effect:"GTC"` even for `POST_ONLY` orders
- **Lookup**: `fetchRegularOrders()` checks cache first, then falls back to the REST value.

### Position Reconciliation

- REST positions are merged with "fake" zero-contract positions for all markets.
- `uniqBy` on `id` ensures real positions override fake ones
- WS `position` events update `position.info.positionId` directly on existing store positions.
- WS events trigger throttled REST refresh (1s debounce)

### Order Reconciliation

- `fetchOrders()` combines results from both `fetchRegularOrders()` and `fetchTpslOrders()`.
- WS `order` events directly add/update orders in store via `addOrUpdateOrders()`
- WS events also trigger throttled REST refresh for consistency
- Virtual TP/SL orders generated from both REST data and WS data

Rust opts into the engine's uncertain-placement fence. An Unknown placement
refuses later placements and amends on its symbol (NotSent) until a clean
regular and TP/SL census, dispatched after the seal, commits; cancels stay
admitted. The uncertain census supersedes an ordinary census in flight, joins
an owed one, and stays owed until a census numbered at or after the request
commits.

### Positions Response Format

The API may return positions in several formats:

- Direct array: `data: [...]`
- Wrapped: `data: { positionList: [...] }`
- Extra nested wrappers: `data: { data: { positionList: [...] } }`

`fetchPositions()` also accepts common alias fields observed across endpoint revisions (`instId`/`contract` for symbol, `size`/`positionAmt` for quantity, `avgPrice`/`entryPrice` for entry, `unRealizedProfit`/`upl` for uPNL, and `marginType` for margin mode). It still preserves the raw row in `position.info`.

Similarly, TP/SL orders may be direct arrays or wrapped in `orderList`.

## Exchange Metadata

- `exchangeName`: `'bitunix'`
- `enableFakeFullTpSl`: `false`
- `accountPositionMode`: Defaults to `PositionMode.Hedged`, updated from the selected `fetchBalance()` account row (`positionMode`)
- `symbolSettings`: `Map<string, { marginType, leverage }>` -- cached per-symbol from `updateSymbolMetadata()`
- `grafanaEndpointsEvery`: `ORDER`, `BATCH_ORDERS`, `SET_LEVERAGE`, `HEDGE_MODE`
- `grafanaEndpointsPeriodic`: `ACCOUNT`, `OPEN_ORDERS`, `POSITIONS`

## Testnet/Demo

- **REST base URL**: `https://testnet.bitunix.com` is defined, but `createAPI()` currently uses `BASE_URL.livenet` and differentiates simulated trading with a header.
- **Testnet differentiation**: Header `x-simulated-trading: 1` added to all requests when `options.testnet` is true
- **WebSocket URLs**: Same as livenet (no separate testnet WS URLs in configuration)
- **Fallback WS URLs**: Same for testnet and livenet

## Order history (`/api/v1/futures/trade/get_history_orders`)

Rust history queries reject failed transport, HTTP/native, malformed-row or
malformed-envelope pages, including after earlier valid pages. They no longer
return clean empty/partial history on failure. Page caps and full duplicate or
nonadvancing walks retain validated rows with an actual-command-owned
`history.incomplete` Warn before completion, including a full order page stuck
at the inclusive start time. Empty end pages and rows crossing the requested
start remain complete. The host flags and warns only the owning caller; trading
writes, live execution baselines and background coverage policies are unchanged.

`fetchHistoricalOrders` reads this endpoint. Mapper: `bitunix.order-history.ts`, pure and
exported. Rows nest under **`data.orderList`** — the row key is per-endpoint here
(`orderList`, `tradeList`, `positionList`).

**Three traps, all verified live on sheet row 56, all of which return less data rather
than an error:**

- **`skip` does NOT work on this endpoint, unlike `get_history_trades` where it does.**
  `skip=100` returns a byte-identical first page. Routing this axis through the shared
  `fetchBitunixHistoryPages` therefore caps it at 100 orders and reads as a small account
  — it fetched 200 raw rows for 100 distinct ids before the fix. `fetchHistoricalOrders` walks
  the `startTime`/`endTime` window backwards instead, and recovered 164 of the venue's own
  reported `total: "164"` for a 30-day window.
- **`status` is ignored outright.** `status=CANCELED` returned 100/100 rows reading
  `FILLED`. So the filter cannot be used, and more importantly it cannot be used to _ask_
  whether cancels are returned at all.
- **A wrong row key looks exactly like an empty account.** Bitunix answers a path that
  does not exist with `code: 404`, but answers a real path read under the wrong key with
  `code: 0` and nothing. `code: 0` plus zero rows is therefore not evidence of an empty
  account; check the key first.

**The 0% never-filled rate here is UNVERIFIED, not a clean result.** Over 30 days this
account returned 164 orders, 25 of them `LIMIT`, with zero cancellations — where okx's
comparable account shows 58 cancels among 114 limits. Because the `status` filter is
ignored, whether the endpoint returns unfilled cancels at all cannot be confirmed from
read-only probing. Treat bitunix order history as fills-shaped until a deliberate
place-and-cancel proves otherwise.

`price` can be the **literal string `"MARKET"`** on a market order — not `""`, not `"0"`,
the word — so `parseFloat` yields `NaN` and an unguarded mapper publishes NaN into the
axis. Filled quantity is `tradeQty`, not `qty`.

## Known Quirks / Implementation Notes

1. **Separate TP/SL API**: TP/SL orders use entirely separate endpoints (`/tpsl/place_order`, `/tpsl/cancel_order`, `/tpsl/get_pending_orders`) from regular orders.

2. **Position TP/SL vs Partial TP/SL**: Two types: `POSITION_TPSL` (full position, amount=0) uses `/tpsl/position/place_order`, and `TPSL` (partial, specific amount) uses `/tpsl/place_order`.

3. **REST API TIF bug**: `get_pending_orders` always returns `effect:"GTC"` even for `POST_ONLY` orders. Mitigated by caching TIF from order placement and WS events in `orderTifCache`.

4. **Default trigger type omission**: Bitunix omits `tpStopType`/`slStopType` when it's MarkPrice (the default). Code defaults to `OrderStopTriggerType.MarkPrice` when missing.

5. **Balance WS lacks uPNL**: Balance WS message doesn't include unrealized PnL, so a throttled REST fetch is triggered instead of using WS data directly.

6. **Private pong compatibility**: Current docs describe a server pong. Historical sessions were silent, so the private socket still calls `handlePongEvent()` and updates `lastReceived` optimistically after ping. Do not remove that compatibility path without an authenticated capture.

7. **HTTP 200 for errors**: Bitunix returns HTTP 200 even for API errors. `validateAccount` must reject every `data.code !== 0` envelope before fetching the balance, using `data.msg` or the stable truthy fallback `Bitunix account validation failed`; the response interceptor only logs these errors and does not throw.

8. **Position side inconsistency**: API returns `side` as `BUY`/`SELL` or `LONG`/`SHORT` depending on endpoint version. Code handles both.

9. **`fetchPositionMode` reuses `ACCOUNT`, NOT `HEDGE_MODE`**: Bitunix's `GET /account/change_position_mode` (the `HEDGE_MODE` constant) now returns `{code:1, msg:"Network Error"}` and would surface as a startup toast. `fetchPositionMode` instead calls `GET /account?marginCoin=USDT` and classifies any non-`ONE_WAY` value as Hedged (`positionMode !== 'ONE_WAY'`), matching `fetchBalance`'s default so the two helpers can't disagree when the field is missing or returns unexpected casing. `HEDGE_MODE` is still used by `changePositionMode` (POST). Don't switch the fetch back without verifying GET is accepted again.

10. **Position mode detection from balance**: `accountPositionMode` is updated from `fetchBalance()` response (`data.positionMode`), not from `fetchPositionMode()` directly during the tick loop.

11. **Futures WS uses different domain**: Public/private use `fapi.bitunix.com`, but futures uses `api.bitunix.com`.

12. **Missing `listenOHLCV` on public WS**: The public WebSocket's `listenOHLCV` throws "Not implemented". Klines are handled by the futures WebSocket instead.

13. **Order clientId prefix**: All orders get client IDs prefixed with `ts_`.

14. **Margin mode values**: `ISOLATION` (not `ISOLATED`) for isolated margin, `CROSS` for cross margin.

15. **Multi-assets / joint-margin visibility**: Bitunix help docs describe multi-assets/joint-margin as eligible collateral assets backing USDT-M margin through discount-rate conversion, not as a separate positions endpoint. Keep using `/api/v1/futures/account?marginCoin=USDT` and `/api/v1/futures/position/get_pending_positions`, but parse account `data` as object-or-array and positions through the flexible wrapper/alias extractor. Do not expand this exchange to USDC-M/Coin-M support without separately auditing market discovery, margin coin selection, order sizing, and settlement semantics.

## Error Handling

### Retry Strategy for Position Errors

- **"Missing positionId"**: Position exists in store but ID is unavailable. Triggers `fetchPositions()` refresh, then retries order placement.
- **"Position does not exist"** (case-insensitive): API rejected a stale positionId. Same refresh-and-retry flow.
- **"nothing to reduce"**: No position exists at all. **Not retried** -- thrown immediately.

Both pre-flight throws inside `formatCreateOrder` (`nothing to reduce`, `Missing positionId`) call `this.emitAndLogError(msg)` before `throw new Error(msg)` — required so the toast pipeline fires. Past regression: for ~6 months these were raw throws and produced no toast, only an unhandled-promise rejection in DevTools. See `packages/safe-cex/src/exchanges/CLAUDE.md` → "Pre-flight validation throws must emit first" for the convention.

Every pre-flight rejection in `formatTPSLOrder` follows the same emit-before-throw rule, including missing position, missing `positionId`, missing market, and missing trigger price.

**The two messages route through `errorPatterns.ts` to different toast actions:**

- `'nothing to reduce'` → `order_warning` → yellow `warningToast`. Real user-action mistake (Reduce-Only left on after switching tabs / from the limit form). Worth nudging the user about.
- `'missing positionid'` → `suppressed` → **no toast at all**. Internal recovery: `placeOrder` catches the throw, refreshes positions, and retries. Showing a toast on the happy retry-then-succeed path would be noise. Still emitted so console / Sentry / `requestsLogAtom` capture it; if it ever persists after refresh you'll see it in DevTools and the dev Requests Log drawer.

**If you reword either emitted message in `formatCreateOrder`, update the matching pattern in `errorPatterns.ts`** or the classifier will fall through to `unknown` → generic red error toast (defeats both the user-friendly warning and the suppression).

Bitunix may reject an otherwise `OPEN`, API-supported crypto market with
`This trading pair does not currently support trading via OpenAPI.` or
`Stock Perpetual trading permissions not activated. Please update App or use Web to sign.`
Live `ZECUSDT` metadata was verified as `isApiSupported: true`, so do not treat
the "Stock Perpetual" wording as proof that Tealstreet sent a stock payload.
Classify these as per-symbol/product-agreement order warnings and keep them in
the web Sentry blacklist; they are actionable venue/account state, not app
defects or terminal API-key auth failures.

### REST Error Handling

- All fetch methods (`fetchMarkets`, `fetchTickers`, `fetchBalance`, `fetchPositions`) catch errors and return existing store data as fallback
- `fetchPositions` logs `[Bitunix] fetchPositions failed:` to console
- Error messages extracted from `err?.response?.data?.msg || err?.message`

### WebSocket Error Handling

- On WS error: closes socket if not already closed, then triggers close handler
- Close handler clears ping timeout and calls `onClose()` for reconnection
- Private WS login failure: emits error, syncs clock, waits 2s, retries login

### Batch Order Error Handling

- Single orders: throws on error (propagates to caller)
- Batch orders: individual errors within a batch are logged via `emitAndLogError` but don't stop other orders in the batch
- TP/SL orders: throws on error

## Testing / Validation

Fresh deterministic regressions belong in the package-level
`src/__tests__/audit-regressions/` directory, derived from current runtime
contracts and verified upstream behavior.

## Private WS auth must go through the remote signer

`sendLogin` signs `sha256(sha256(nonce + timestamp + apiKey) + secret)` — note
this is **plain concatenation, not an HMAC**, which is why it needs its own
`bitunix-double-sha256` output format rather than reusing `hex`.

It used to compute that inline from `options.secret`. **Hosted
exchange-server runs with `secret: ''` by design** (the secret never leaves the
client), so the signature was taken over an empty secret and bitunix rejected
the handshake with _"Bitunix private ws login failed, double check your key and
secret"_ — which reads like bad credentials and is not. It reproduces **only**
in hosted mode: local dev and `yarn keys audit` both hold the real secret and
authenticate fine, so the key audits as valid while e2e reports it dead.

The signer branch lives in `BaseExchange.signWsAuth` and is locked by a golden
vector in `__tests__/base.ws-auth-signing.test.ts` generated with Node's
built-in crypto. See `../CLAUDE.md` → "Remote-signer WS auth" for the general
rule; bitunix is the fourth venue to hit it after kraken, blofin and apexomni.

**Signer-failure branch does NOT say "auth failed".** The `remoteSigner.sign` error branch in `bitunix.ws-private.ts sendLogin` emits `Remote WS signing unavailable: …`, not `Bitunix remote WS auth failed: …`. The latter wording matches `health/errorPatterns.ts`'s `auth_invalid` (case-insensitive substring), which the web controller treats as a terminal API-key rejection and suppresses automatic account reloads for five minutes — the very reload a signer-less tab needs to re-pair. See `../CLAUDE.md` → "Remote-signer WS auth" and the bybit template in `bybit/bybit.ws-private.ts`.

**Verified in e2e run 31351139247**: 18 successful `[bitunix login] success` frames
against 11 failures, where every failure — and every REST `10003 Token invalid` —
falls inside the 24-second window of the _Bad API Key_ test, which registers a
deliberately bad key. Reading a raw count of "login failed" lines without
bucketing them by time makes a working handshake look broken; that test
guarantees a nonzero failure count on every run. Check for the success frames.

**A remote-signer WS auth change is a two-target deploy.** The request half runs
on the exchange-server, but `signWsAuth` runs in the **browser**, so the fix is
not live until the web bundle ships too. Dispatching e2e after only the server
redeploy yields `Unsupported WS auth outputFormat: bitunix-double-sha256` from
the still-old browser — a real symptom of a stale deploy, not of a broken fix.

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `clientId`. Bitunix publishes no length limit; the rule refuses only empty or whitespace-bearing IDs. The TIF / reduce-only caches key on whichever `clientId` is sent.

## Rust futures engine

The sans-I/O implementation is `sdk-rs/venues/bitunix/`; the wasm factory name
is `bitunix`. It preserves the USDT futures scope and legacy full-snapshot
`depthop` books. Binary futures frames are decoded by the hosts (gzip or zlib).

`realizedPNL` already includes fees and funding. Carry it verbatim for positions
and realized income; the commission ledger still exposes `-fee`, including
positive rebates. Do not subtract fee/funding again or synthesize funding rows.

Intentional Rust corrections are listed in the replay gate: failed reads preserve
state, cumulative order executions emit once, historical fills dedupe by trade ID,
split generated IDs are unique, and a
cancel/recreate amendment requires acknowledged cancellation and final remaining
quantity. Public `ft`/`nft` may be ISO strings: the next funding time uses `nft`
(or `ft` if absent), converted to epoch milliseconds rather than the ISO year.

Orders expose the native `clientId` as top-level `clientOrderId` (REST and WS);
virtual TP/SL legs carry none, since a caller ID names exactly one order. Live
fills are emitted from cumulative `dealAmount`/`averagePrice` on UPDATE/CLOSE.

Rust cancellation rejects absent-store IDs before any batch write because native
symbol and protection identity require reconciliation. Empty lists remain no-ops.

Rust command failures carry `VenueError` classification and preserve native
messages/codes. Preflight and signing failures are `NotSent`; lost write responses,
HTTP 408/5xx, malformed acknowledgements and undocumented native errors remain
`Unknown`. HTTP status takes precedence over body classification. Native `10001`
is a network error, while `10005`/`10006` are rate limits, per the
[Bitunix error table](https://www.bitunix.com/api-docs/futures/ErrorCode/error_code.html).

Batch failures finish once with accepted IDs as partial detail. Independent later
batch legs still run; any uncertain leg keeps the overall outcome `Unknown`.
After a confirmed write, a later rejection cannot claim the whole command was
rejected or unsent. A failed read while reconciling an acknowledged cancellation
does not imply an uncertain new write. Recovery writes require definitive native
rejection; malformed or server-error answers never trigger cancel/recreate.

Rust public discovery skips malformed typed catalogue/ticker rows with a coded
warning while committing valid peers only after the complete envelope succeeds.
Empty, filtered-only and wholly malformed seeds retry without claiming readiness;
intentional eligibility filters do not count as malformed rows. Private startup
begins from the usable catalogue and can finish before tickers; ticker recovery
starts only public ticker sockets and never resets account settings or polling.
Discovery uses shared capped exponential backoff with jitter and visible
degradation/recovery. HTTP 418/429 honor `Retry-After` (60-second default); native
`10005`/`10006` wait 60 seconds even with missing/null data, while `10001` remains a network
failure. Malformed public previews are not copied into diagnostics. Private,
financial history and trading policies remain separate.

Rust socket supervision retains one retry state per private/futures/public-shard
feed. Refusals use shared capped exponential jitter; open alone never resets the
failure streak. Connection attempts expire after 10 seconds; opened public feeds
expire after 45 seconds without traffic. Private feeds retain legacy silent-session
compatibility until their first real pong, then use the same 45-second traffic
deadline. Pings run every 15 seconds without synthetic traffic. Retired socket
epochs and queue/login timers cannot mutate a replacement. Futures retirement
immediately invalidates depth and publishes consumer clearing, retaining interests
for one replay of the current subscriptions at the replacement open.

Rust construction retains validated secondary read credentials. Signed GETs
(account axes, symbol settings and history) use that identity and the SDK's api-key
GET discriminator sends them through secondary egress. Unsigned public GETs also
use the configured secondary GET transport without adding authentication. POSTs,
other mutations and private WS login retain primary identity/egress. Partial/empty
secondary credentials fail before startup I/O, including public-only engines;
primary-only configurations keep existing signatures. Account/settings request
tags name the selected credential slot; history is never account-auth evidence.
This signing/routing fix does not classify native account-auth outcomes.

Rust ordinary position reconciliation treats private position-ID notifications
as routing hints rather than newer financial snapshots. Reads retain latest-serial
and accepted settings/mode write-revision fences, while command-owned preflight
keeps full generation checks. Position notifications coalesce without postponing
a read; an in-flight ordinary or owned serial is not superseded by background
refresh before a 10-second lost-read bound. Only its matching completion/failure
releases the token, and activity during a read schedules one follow-up. Existing
REST/WS position-ID routing precedence and all financial mappers are unchanged.

Rust position snapshots isolate typed malformed rows by the native symbol (exact
existing aliases/case), retaining both cached hedge legs and REST routing metadata
for a rejected symbol. Unknown row identities hold absence, while valid rows may
replace their own IDs, including explicit zero closes. Known bad/valid duplicates
quarantine the symbol in either order; bounded coverage overflow holds everything
conservatively. A partial snapshot cannot mix held exposure with contradictory
position modes, establish first readiness or qualify as a trading prerequisite.
Only admitted rows update settings; WS routing hints and existing financial/side
fallbacks stay unchanged. Order/TP-SL rows use a separate policy.

Rust regular/TP-SL REST snapshots retain malformed native raw-ID families together
with cached parent/virtual siblings and routing metadata; valid peers and safely
absent raw families advance only after complete envelopes. Unknown identity holds
absence within that endpoint scope, while an admitted raw parent supersedes its
entire old family, including omitted brackets or changed standalone leg shapes.
Known bad/valid duplicates quarantine the raw ID, and bounded coverage overflow
holds conservatively. Only admitted groups advance execution baselines; pending
execution and optimistic/terminal guards remain. Partial initial halves cannot
establish readiness, both staged serials/generations must be current, and partial
command-owned refreshes cannot qualify as trading prerequisites. WS generation
fences and financial/side defaults remain separate unchanged policies.

Rust Bitunix declares synchronized host-clock input. The SDK adopts that capability
before the first engine start and supplies its virtual clock for authenticated
REST and private-login timestamps, including later socket epochs. Signing keeps
its existing millisecond flooring and byte format. This does not add automatic
signature-rejection retries or resend writes.

Rust private-login controls are inspected even without `data`. An explicit false
`data.result` keeps the existing rejection diagnostic; absent/null/malformed
results warn as malformed stream data without claiming invalid credentials or
copying the response. Failed controls share one two-second retry per socket
without postponing its deadline. Success and socket retirement cancel that retry;
a late timer cannot sign on a replacement. The existing first-pong compatibility,
login timestamp units and write retry policy remain unchanged.

Rust engine transport honors explicit `tealstreetWsRoute: true` on the first
public-shard, private and futures/business socket attempt, selecting that channel's
declared proxy. False/unset keeps two silent direct attempts before fallback;
feed identities remain independent. The engine's native routing plan uses the
same selection. The shared resolver and TS socket classes retain their existing
fallback contract, and a direct transport still reports unsupported proxy routing.

Rust regular and TP/SL reads retain independent in-flight serials and ten-second
lost-read deadlines. Ordinary refresh coalesces against either a background or
command-owned read until its deadline; matching completion/failure alone releases
it. Notifications keep the first one-second repair deadline, and late callbacks
cannot retire newer reads. This scheduling does not relax generation checks,
financial admission, initial two-half readiness or command prerequisites.

Rust ordinary order reconciliation admits healthy REST peers across WS-only
notifications using endpoint serials and a separate command-mutation revision.
Newer validated WS raw families retain their current visible rows, native metadata,
fill watermarks and terminal absence over stale REST data. Malformed WS identity
coverage holds affected families or endpoint absence without blocking healthy
peers. Sparse unknown execution updates remain eligible for REST metadata repair.
Both initial halves retain overlap coverage until a complete current pair can
publish; malformed or overflowed coverage cannot establish readiness. Write
planning/dispatch, responses (including partial cancel ACKs) and cleanup invalidate
older ordinary reads. Command-owned prerequisites retain global generation fences;
no write is retried by reconciliation.

Rust ordinary balance reads also retain a current in-flight serial until completion
or the existing ten-second lost-read bound. Private notifications, refresh and
reconnect coalesce against that read; notifications retain the first repair
deadline. WS-only notification overlap cannot discard a valid financial response,
while accepted position-mode writes fence older reads by a separate revision.
Late callbacks cannot release replacements, and local signing failure releases
before ordinary repair. Financial mapping and the500ms polling policy are unchanged.

Rust per-symbol settings reads retain their attempt serial and accepted settings-
write revision across position notifications, including activity on other symbols.
Publication, scoped auth and retry/reset share this fence. Accepted leverage,
margin and position-mode writes supersede older metadata; refused or unproved
writes do not. This preserves canonical-market admission, grouped refusals,
bounded private-read recovery and existing margin-mode fallback.
