# Bitunix Exchange Integration

## Exchange Information

- **Type**: Centralized Exchange (CEX), Futures only
- **Quote currency**: USDT only (markets must be `OPEN`, quote USDT, and advertise `isApiSupported !== false`)
- **Position modes**: Hedged (default) and One-Way
- **Margin types**: Cross and Isolated (`CROSS` / `ISOLATION`)

## API Documentation

- **Official API Documentation**: <https://www.bitunix.com/api-docs/futures/>

## File Structure

| File                       | Description                                                             |
| -------------------------- | ----------------------------------------------------------------------- |
| `bitunix.api.ts`           | REST API client with double-SHA256 signature scheme, axios interceptors |
| `bitunix.exchange.ts`      | Main exchange class extending `BaseExchange`                            |
| `bitunix.types.ts`         | Type definitions, endpoint constants, type mapping objects              |
| `bitunix.utils.ts`         | SHA256 hex hashing utility (uses `@noble/hashes`)                       |
| `bitunix.ws-futures.ts`    | Futures-specific WebSocket for klines and orderbook (gzip-compressed)   |
| `bitunix.ws-private.ts`    | Private WebSocket for orders, positions, TP/SL, balance (login auth)    |
| `bitunix.ws-public.ts`     | Public WebSocket for tickers, mark prices, depth_book1                  |

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
| `HISTORY_TRADES`       | `/api/v1/futures/trade/get_history_trades`         | User fills history (primary for `fetchExecutions`)                                                                                                                                                                                             |
| `KLINE`                | `/api/v1/futures/market/kline`                     | Kline/candlestick data (public)                                                                                                                                                                                                                |
| `POSITIONS`            | `/api/v1/futures/position/get_pending_positions`   | Fetch open positions                                                                                                                                                                                                                           |

**Public endpoints** (no auth): `MARKETS`, `TICKERS`, `KLINE`

Additional TP/SL modify endpoints:

- `MODIFY_TP_SL_ORDER`: `/api/v1/futures/tpsl/modify_order` for standalone
  partial TP/SL rows by raw TP/SL order id.
- `MODIFY_POSITION_TP_SL_ORDER`:
  `/api/v1/futures/tpsl/position/modify_order` for full-position TP/SL rows by
  symbol and `positionId`.

Execution-history behavior (`fetchExecutions`):

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

| WebSocket | Ping Format                  | Interval | Pong Behavior                                                                                                                                   |
| --------- | ---------------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Public    | `{ "ping": <timestamp_ms> }` | 15s      | Normal pong expected                                                                                                                            |
| Private   | `{ "op": "ping" }`           | 15s      | Current docs describe a pong; the client still handles the ping optimistically for compatibility with historically silent sessions. |
| Futures   | `{ "ping": <timestamp_ms> }` | 15s      | Normal pong expected                                                                                                                            |

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

| Bitunix Value | Internal Type                    | Context   |
| ------------- | -------------------------------- | --------- |
| `MARK_PRICE`  | `OrderStopTriggerType.MarkPrice` | REST API  |
| `LAST_PRICE`  | `OrderStopTriggerType.LastPrice` | REST API  |
| `Mark`        | `OrderStopTriggerType.MarkPrice` | WebSocket |
| `Last`        | `OrderStopTriggerType.LastPrice` | WebSocket |

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

There is no cancel/recreate fallback. Market orders, missing or inconsistent
raw order metadata, and malformed prices or quantities reject before any
exchange write. Standalone partial TP/SL rows route through
`MODIFY_TP_SL_ORDER` with the raw Bitunix TP/SL id retained in `order.info`;
full-position TP/SL rows (`info.type === "POSITION_TPSL"` or `amount === 0`)
route through `MODIFY_POSITION_TP_SL_ORDER` and preserve the sibling leg when it
is present in the store.

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
- Markets are filtered to `quote === 'USDT'`, `symbolStatus === 'OPEN'`, and `isApiSupported !== false`. The official schema defines `false` as API trading disabled, so these symbols fail closed and are not surfaced as tradable.

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
- **Retry**: 3 retries on network errors via `axios-retry`
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

## Known Quirks / Implementation Notes

1. **Separate TP/SL API**: TP/SL orders use entirely separate endpoints (`/tpsl/place_order`, `/tpsl/cancel_order`, `/tpsl/get_pending_orders`) from regular orders.

2. **Position TP/SL vs Partial TP/SL**: Two types: `POSITION_TPSL` (full position, amount=0) uses `/tpsl/position/place_order`, and `TPSL` (partial, specific amount) uses `/tpsl/place_order`.

3. **REST API TIF bug**: `get_pending_orders` always returns `effect:"GTC"` even for `POST_ONLY` orders. Mitigated by caching TIF from order placement and WS events in `orderTifCache`.

4. **Default trigger type omission**: Bitunix omits `tpStopType`/`slStopType` when it's MarkPrice (the default). Code defaults to `OrderStopTriggerType.MarkPrice` when missing.

5. **Balance WS lacks uPNL**: Balance WS message doesn't include unrealized PnL, so a throttled REST fetch is triggered instead of using WS data directly.

6. **Private pong compatibility**: Current docs describe a server pong. Historical sessions were silent, so the private socket still calls `handlePongEvent()` and updates `lastReceived` optimistically after ping. Do not remove that compatibility path without an authenticated capture.

8. **HTTP 200 for errors**: Bitunix returns HTTP 200 even for API errors. `validateAccount` must reject every `data.code !== 0` envelope before fetching the balance, using `data.msg` or the stable truthy fallback `Bitunix account validation failed`; the response interceptor only logs these errors and does not throw.

9. **Position side inconsistency**: API returns `side` as `BUY`/`SELL` or `LONG`/`SHORT` depending on endpoint version. Code handles both.

10. **`fetchPositionMode` reuses `ACCOUNT`, NOT `HEDGE_MODE`**: Bitunix's `GET /account/change_position_mode` (the `HEDGE_MODE` constant) now returns `{code:1, msg:"Network Error"}` and would surface as a startup toast. `fetchPositionMode` instead calls `GET /account?marginCoin=USDT` and classifies any non-`ONE_WAY` value as Hedged (`positionMode !== 'ONE_WAY'`), matching `fetchBalance`'s default so the two helpers can't disagree when the field is missing or returns unexpected casing. `HEDGE_MODE` is still used by `changePositionMode` (POST). Don't switch the fetch back without verifying GET is accepted again.

11. **Position mode detection from balance**: `accountPositionMode` is updated from `fetchBalance()` response (`data.positionMode`), not from `fetchPositionMode()` directly during the tick loop.

12. **Futures WS uses different domain**: Public/private use `fapi.bitunix.com`, but futures uses `api.bitunix.com`.

13. **Missing `listenOHLCV` on public WS**: The public WebSocket's `listenOHLCV` throws "Not implemented". Klines are handled by the futures WebSocket instead.

14. **Order clientId prefix**: All orders get client IDs prefixed with `ts_`.

15. **Margin mode values**: `ISOLATION` (not `ISOLATED`) for isolated margin, `CROSS` for cross margin.

16. **Multi-assets / joint-margin visibility**: Bitunix help docs describe multi-assets/joint-margin as eligible collateral assets backing USDT-M margin through discount-rate conversion, not as a separate positions endpoint. Keep using `/api/v1/futures/account?marginCoin=USDT` and `/api/v1/futures/position/get_pending_positions`, but parse account `data` as object-or-array and positions through the flexible wrapper/alias extractor. Do not expand this exchange to USDC-M/Coin-M support without separately auditing market discovery, margin coin selection, order sizing, and settlement semantics.

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
