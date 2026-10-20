# Orderly Network Exchange Integration

## Exchange Information

Orderly Network is an omnichain orderbook DEX infrastructure that supports white-label deployments. Multiple front-ends (KodiakFi, SalsaDEX, Mode, WooFi, What, Tealstreet DEX) share the same backend. All variants extend the base `OrderlyExchange` class, differing only in `orderlyExchangeName` and the `order_tag` used for broker attribution.

## API Documentation

- **Official API Documentation**: https://orderly.network/docs/build-on-omnichain/introduction
- **Isolated Margin Flow**: https://orderly.network/docs/build-on-omnichain/user-flows/isolated-margin
- **Edit Order Contract**: https://orderly.network/docs/build-on-omnichain/restful-api/private/edit-order
- **Mainnet host note**: Current docs advertise `https://api.orderly.org`, while this integration intentionally remains on the still-live `https://api-evm.orderly.org` EVM host. Treat a host migration as a separately verified routing change, not a documentation-only cleanup.

## File Structure

### Core

| File                    | Description                                                                                   |
| ----------------------- | --------------------------------------------------------------------------------------------- |
| `orderly.exchange.ts`   | Main exchange class extending `BaseExchange`                                                  |
| `orderly.api.ts`        | REST API client with ed25519 request signing, rate limiting, retry, broker ID injection       |
| `orderly.types.ts`      | TypeScript type definitions, endpoint constants, order type/side maps, Grafana endpoint lists |
| `orderly.utils.ts`      | Symbol normalization utilities (`PERP_BTC_USDC` <-> `BTCUSDC`), memoized with lodash          |
| `orderly.encryption.ts` | ed25519 signing using `@noble/ed25519` + `bs58` + base64url encoding                          |
| `orderly.ws-private.ts` | Private WebSocket: ed25519 auth, order/algo execution reports, position pushes                |
| `orderly.ws-public.ts`  | Public WebSocket: tickers, BBOs, mark prices, funding rates, klines, orderbook                |

### White-label Exchange Variants (`exchanges/`)

Each variant extends `OrderlyExchange` with a different `orderlyExchangeName`:

| Variant        | File                                                  | `orderlyExchangeName` | `order_tag`                            |
| -------------- | ----------------------------------------------------- | --------------------- | -------------------------------------- |
| KodiakFi       | `exchanges/kodiakfi/kodiakfi.exchange.ts`             | `KODIAKFI`            | `TEALKODIAK`                           |
| Mode           | `exchanges/mode/mode.exchange.ts`                     | `MODE`                | `TEALMODE`                             |
| SalsaDEX       | `exchanges/salsadex/salsadex.exchange.ts`             | `SALSADEX`            | `TEALSALSA`                            |
| Tealstreet DEX | `exchanges/tealstreet-dex/tealstreet-dex.exchange.ts` | `Tealstreet DEX`      | `TEALSTREET` (default)                 |
| What           | `exchanges/what/what.exchange.ts`                     | `WHAT`                | `TEALWHAT`                             |
| WooFi          | `exchanges/woofi/woofi.exchange.ts`                   | `WOOFI`               | `TEALSTREET` (default, no special tag) |

The `order_tag` mapping is in `OrderlyExchange.placeOrder()` and branches on `this.orderlyExchangeName`.

## Authentication

Orderly uses **ed25519** signature-based authentication (NOT HMAC):

### Key Format

Keys must have an `ed25519:` prefix. The `OrderlyExchange` constructor prepends it when missing.

### REST API Headers

- `orderly-key`: API key (with `ed25519:` prefix)
- `orderly-timestamp`: Millisecond timestamp from `virtualClock`
- `orderly-signature`: ed25519 signature (base64url-encoded)
- `orderly-textsign`: The raw text that was signed
- `orderly-account-id`: Account ID (from `options.applicationId`), set as default header
- `Content-Type`: `application/x-www-form-urlencoded` (GET/DELETE) or `application/json` (POST/PUT)

### Signature Generation (`orderly.encryption.ts`)

1. Concatenate: `timestamp + METHOD + url + body`
   - `signRequest()` JSON-stringifies a present, non-empty body.
   - `signRequest()` serializes request params into the URL query string before signing.
2. Sign using ed25519: `sign(encoder.encode(textSign), bs58.decode(secret.split(':')[1]))`
   - Only the portion after the `ed25519:` prefix of the secret is used for signing
3. Encode result: `base64url(Buffer.from(signature).toString('base64'))`
   - base64url replaces `+` with `-` and `/` with `_`

### Public Endpoints

The `createAPI()` request interceptor skips authentication for URLs containing `'/public/'`.

### Broker ID Injection

The request interceptor injects broker IDs on POST requests:

- `PLACE_ORDER`: `broker_id` field (snake_case)
- `ALGO_ORDER`: `brokerId` field (camelCase)
- Livenet broker ID: `ab82cb09-cfec-4473-80a3-b740779d0644`
- Testnet broker ID: `e708a644-9ce5-46c6-b50c-74b15f62d8ca`

### Private WebSocket Authentication

On connection open, sends an `auth` event:

```json
{
  "event": "auth",
  "params": {
    "orderly_key": "<api_key>",
    "sign": "ed25519_sign(<timestamp>, secret)",
    "timestamp": "<ms_timestamp>"
  }
}
```

`OrderlyPrivateWebSocket.onMessage()` subscribes to channels only after receiving `{ "event": "auth", "success": true }`.

## Startup Sequence

`OrderlyExchange.start()` executes the following sequence:

1. **Validate account** - `start()` calls `GET /v1/client/info` inline. On a **definitive auth rejection** (4xx — expired/revoked/wrong key) it emits `'deactivate'` and returns. On a **transient failure** (no HTTP response, 408, 429, or 5xx) it **throws instead** — see "Transient validation must not deactivate" under Known Quirks.
2. **Fetch markets** - `fetchMarkets()` calls `GET /v1/public/info`, filters to `PERP_` symbols, stores in store.
3. **Dispose check** - Returns if `isDisposed` (checked after each async call).
4. **Fetch tickers** - `fetchTickers()` calls `GET /v1/public/futures`, maps to internal ticker format.
5. **Fetch default margin modes** - `fetchAndUpdateMarginTypes()` calls `GET /v1/client/margin_modes` and caches each symbol's `CROSS`/`ISOLATED` default. Failure is emitted but does not disable the account.
6. **Fetch account leverage** (fire-and-forget) - `fetchAndUpdateAccountLeverage()` runs in parallel with subsequent steps, fetches `GET /v1/client/info`, and stores `max_leverage`. Its rejection is observed and emitted so a transient failure cannot become an unhandled promise rejection.
7. **Store markets and tickers** - Updates store with loaded flags.
8. **Connect WebSockets** - `publicWebsocket.connectAndSubscribe()` and `privateWebsocket.connectAndSubscribe()` start connections in parallel (neither awaited).
9. **First tick** - `await this.tick()` fetches positions (which also updates balance) and orders.
10. **Fetch orders** - Final `fetchOrders()` gets both limit and algo orders, stores them.

## REST API Endpoints

| Constant                                        | Endpoint                    | Method          | Description                                               |
| ----------------------------------------------- | --------------------------- | --------------- | --------------------------------------------------------- |
| `ACCOUNT`                                       | `/v1/client/info`           | GET             | Account info (leverage, fees)                             |
| `BALANCE`                                       | `/v1/client/holding`        | GET             | Account balance/holdings (unused, balance from positions) |
| `MARKETS`                                       | `/v1/public/info`           | GET             | Markets/symbol info                                       |
| `TICKERS`                                       | `/v1/public/futures`        | GET             | Futures tickers                                           |
| `POSITIONS`                                     | `/v1/positions`             | GET             | All positions + collateral values                         |
| `POSITION`                                      | `/v1/position/{symbol}`     | GET             | Single position                                           |
| `ORDERS`                                        | `/v1/orders`                | GET/DELETE      | Limit orders / cancel all                                 |
| `PLACE_ORDER` / `CANCEL_ORDER` / `UPDATE_ORDER` | `/v1/order`                 | POST/DELETE/PUT | Place / cancel / update single order                      |
| `ALGO_ORDERS` / `CANCEL_ALGO_ORDERS`            | `/v1/algo/orders`           | GET/DELETE      | Algo orders / cancel all                                  |
| `ALGO_ORDER`                                    | `/v1/algo/order`            | POST/PUT        | Place / update algo order                                 |
| `KLINE`                                         | `/v1/kline`                 | GET             | Kline/candlestick data                                    |
| `ORDERBOOK`                                     | `/v1/orderbook`             | GET             | Order book snapshot                                       |
| `LEVERAGE`                                      | `/v1/client/leverages`      | POST            | Set batch or symbol/margin-mode leverage                  |
| `MARGIN_MODE` / `MARGIN_MODES`                  | `/v1/client/margin_mode(s)` | POST/GET        | Set/get per-symbol default margin mode                    |

### Grafana Endpoint Tracking

- **Every call**: `ALGO_ORDER`, `POSITION`, `UPDATE_ORDER`, `CANCEL_ORDERS`, `CANCEL_ALGO_ORDERS`, `CANCEL_SYMBOL_ORDERS`, `CANCEL_ORDER`, `PLACE_ORDER`, `LEVERAGE`
- **Periodic**: `ACCOUNT`, `BALANCE`, `POSITIONS`, `ALGO_ORDERS`, `ORDERS`, `MARGIN_MODES`

## WebSocket Implementation

### Network Configuration

| Environment | Public WS                                                | Private WS                                                                  |
| ----------- | -------------------------------------------------------- | --------------------------------------------------------------------------- |
| Livenet     | `wss://ws-evm.orderly.org/ws/stream/<accountId>`         | `wss://ws-private-evm.orderly.org/v2/ws/private/stream/<accountId>`         |
| Testnet     | `wss://testnet-ws-evm.orderly.org/ws/stream/<accountId>` | `wss://testnet-ws-private-evm.orderly.org/v2/ws/private/stream/<accountId>` |

Both URLs are appended with `options.applicationId` (the account ID).

### Public WebSocket (`orderly.ws-public.ts`)

**On connect** (`onOpen`):

1. Sends initial ping
2. Subscribes to four global topics: `tickers`, `bbos`, `markprices`, `estfundingrate`
3. Resubscribes to any previously-registered on-demand topics (`subscribe()`)
4. Emits `wsReconnected` if reconnecting WebSocket is enabled

**Message routing** (`onMessage`): Uses string `.includes()` checks against `event` or `topic` fields to dispatch to handlers. Breaks after first match.

**On-demand subscriptions**:

- `<PERP_SYMBOL>@kline_<interval>` - Kline updates
- `<PERP_SYMBOL>@orderbookupdate` - Orderbook incremental updates

**Ping/Pong**:

- Active ping: `{ "event": "ping" }` sent by client
- Responds to server ping with `{ "event": "pong" }`

**Reconnection**: Uses `BaseWebSocket.createWebsocket()` which creates either a `ReconnectingWebSocket` or native `WebSocket` depending on `ENABLE_RECONNECTING_WEBSOCKET` config. On reconnect, `onOpen` re-subscribes to all stored topics.

**Orderbook pattern** (`OrderlyPublicWebsocket.listenOrderBook`):

1. Subscribe to `@orderbookupdate` topic
2. On first message, fetch REST snapshot via `GET /v1/orderbook/<symbol>`
3. Buffer all WS updates received during the snapshot fetch
4. After snapshot loads, filter buffered updates to only those with `ts > data.timestamp`
5. Apply buffered updates to snapshot
6. Subsequent updates applied incrementally via `updateOrderBook()` utility
7. If the REST snapshot fails, clear the buffered updates and emit the error; the next WS update then starts a fresh snapshot attempt instead of buffering forever.

### Private WebSocket (`orderly.ws-private.ts`)

**On connect** (`onOpen`):

1. Signs timestamp with ed25519 and sends `auth` event
2. If `ENABLE_RECONNECTING_WEBSOCKET`, calls `fetchAndSetPrivateSnapshot()` to re-fetch orders

**After auth success** (`OrderlyPrivateWebSocket.subscribe`):

1. Subscribes to `executionreport`
2. Subscribes to `algoexecutionreportv2`
3. Subscribes to `position`
4. Starts ping timer

**Ping/Pong**: Same as public WS (`{ "event": "ping" }`/`{ "event": "pong" }`)

**Execution Report handling** (`OrderlyPrivateWebSocket.handleExecutionReport`):

- `NEW`, `REPLACED`, `PARTIAL_FILLED`, `PARTIALLY_FILLED`: Adds or updates order in store
- `CANCELLED`, `FILLED`: Updates order status
- `FILLED` or partial fills: Emits `fill` event with `executedPrice` and `executedQuantity`
- **Cumulative fills**: Private `executionreport` events carry `totalExecutedQuantity`; `mapLimitOrder` uses it before the REST-only `executed` fallback so partial orders retain their full amount, cumulative filled quantity, remaining amount, and Open status between polls.

**Algo Execution Report handling** (`OrderlyPrivateWebSocket.handleAlgoExecutionReport`):

- Data may be a single object or array; normalized to array
- `NEW`, `REPLACED`, partial fills: Maps child orders via `mapAlgoOrder()` and adds/updates in store
- `CANCELLED`, `FILLED`: Updates status for both the parent and all child orders
- `FILLED`: Emits `fill` event with `averageExecutedPrice` and `totalExecutedQuantity`

**Position handling** (`OrderlyPrivateWebSocket.handlePositionReport`):

- Maps documented camel-case position pushes through the same mapper as REST positions.
- Preserves `marginMode`, venue leverage, allocated isolated `margin`, liquidation price, and position size.
- Cross positions retain the legacy symbol ID; isolated positions add an `:isolated` suffix so both modes for one symbol can coexist in the store.

## Order Types and Mappings

| Internal Type                | Orderly API Value | API Endpoint      | Notes                                                                                                    |
| ---------------------------- | ----------------- | ----------------- | -------------------------------------------------------------------------------------------------------- |
| `OrderType.Limit`            | `LIMIT`           | `/v1/order`       | Requires `order_price`                                                                                   |
| `OrderType.Market`           | `MARKET`          | `/v1/order`       | No price needed                                                                                          |
| `OrderType.TakeProfit`       | `TAKE_PROFIT`     | `/v1/algo/order`  | Via positional algo order                                                                                |
| `OrderType.StopLoss`         | `STOP_LOSS`       | `/v1/algo/order`  | Via positional algo order                                                                                |
| `OrderType.TrailingStopLoss` | `TRAILING_STOP`   | read-only mapping | Placement fails closed because callback-rate/value inputs are not represented by the current integration |

### Order Side Mapping

- `BUY` <-> `OrderSide.Buy`
- `SELL` <-> `OrderSide.Sell`

## Order Lifecycle

### Limit/Market Orders

1. **Place**: `POST /v1/order` with `order_type`, `order_quantity`, `order_price` (limit only), `side`, `reduce_only`, explicit `margin_mode`, `order_tag`, and `broker_id`. Reduce-only writes infer the matching open position's mode; if Cross and Isolated rows make the target ambiguous, the write fails before REST.
2. **Large order splitting** (`OrderlyExchange.placeOrder`): If `amount > market.limits.amount.max`, the order is split into equal lots plus a remainder lot. Uses `mapSeries` for sequential placement.
3. **Update** (`OrderlyExchange.updateOrder`): Uses the documented native `PUT /v1/order` edit route for exactly reconciled, REST-proven, pending zero-fill ordinary GTC LIMIT rows. The request carries required `order_id`/`symbol`/`order_type`/`side` identity plus only the changed `order_price` or `order_quantity`, requires `success: true` with `data.status: 'EDIT_SENT'`, and returns the unchanged ID. Explicit REST `CROSS` or `ISOLATED` provenance is required; camel-case WS-only provenance, unknown modes, and other missing/ambiguous identity, fill, precision, visibility, reduce-only, time-in-force, or trigger state fail before a write. There is no cancel-and-replace fallback.
4. **Cancel**: `DELETE /v1/order?order_id=<id>&symbol=<symbol>` - parameters sent as query string

### Algo Orders (TP/SL)

1. **Place**: `OrderlyExchange.placePositionalAlgoOrder` posts `algo_type: 'POSITIONAL_TP_SL'` and explicit `margin_mode` with `STOP_LOSS` and/or `TAKE_PROFIT` children whose type is `CLOSE_POSITION`.
2. **Sibling management**: When placing a new TP or SL, checks for existing sibling orders (same symbol, different type). If found, cancels parent algo order and re-places both together. Bulk cancellation deliberately bypasses this helper so a panic/nuke cannot recreate a sibling while cancelling.
3. **Update**: `OrderlyExchange.updateOrder` uses `PUT /v1/algo/order` with the parent `order_id` and a child carrying its `order_id` and new `trigger_price`.
4. **Cancel** (`OrderlyExchange.cancelAlgoOrder`): Cancels via `DELETE /v1/algo/order?order_id=<parentId>&symbol=<symbol>`. If a sibling order exists, it is automatically re-created to preserve the other side of the TP/SL pair.

### Bracket Orders

When `OrderlyExchange.placeOrder` receives `stopLoss` and/or `takeProfit` alongside a limit/market order:

1. The main order is placed first
2. A positional algo order is created with the opposite side containing the bracket orders

### Order ID Tracking

- Limit order IDs: `order_id` from the REST response (numeric, stringified)
- Algo order IDs: `algo_order_id` from child orders, with `parentId` set to `root_algo_order_id`

## Symbol Format

- **Orderly format**: `PERP_BTC_USDC` (underscore-separated with `PERP_` prefix)
- **Internal format**: `BTCUSDC` (concatenated base + quote)
- **Normalization** (`normalizeSymbol`): `PERP_BTC_USDC` -> split on `_` -> take [1] and [2] -> `BTCUSDC`
- **Reverse** (`reverseSymbol`): `BTCUSDC` -> strip `USDC` -> `PERP_<base>_USDC`
- Both functions are memoized with `lodash/memoize`
- Market ID format: `<base>/<quote>:USDC` (e.g., `BTC/USDC:USDC`)

## Data Precision & Conversion

- **Price precision**: `quote_tick` from `/v1/public/info` (used as the step size, e.g., `0.01`)
- **Amount precision**: `base_tick` from `/v1/public/info`
- **Amount limits**: `base_min` and `base_max` from market info
- **Leverage limits**: Minimum leverage falls back to 1. Per-symbol maximum leverage is derived as `Math.round(1 / base_imr)`, with the legacy `max_leverage` field and then 100 as fallbacks for older or incomplete rows.
- **Price rounding**: Uses `adjust()` utility from `safe-math` module with precision values
- **uPNL calculation** (`OrderlyExchange.fetchPositions`):
  - `priceDiff = max(entry, mark) - min(entry, mark)`
  - Direction determined by comparing mark vs entry with position side
  - Rounded to 2 decimal places: `Math.round(value * 100) / 100`
- **Balance derivation**: From positions endpoint fields `total_collateral_value` and `free_collateral`. `OrderlyExchange.tick()` subtracts computed uPNL from balance `total`.

## Rate Limiting

- **Primary API**: The `OrderlyExchange` constructor wraps its API client at 10 requests/second.
- **Timeout**: 20,000ms (`RECV_WINDOW`, configurable via `options.extra.recvWindow`)
- **Retry**: `createAPI()` configures 3 retries on network errors.
- **HTTP over WebSocket**: `createAPI()` supports the `tealstreetTunnel` option.

## State Reconciliation

### Order State

- **REST polling**: Limit orders fetched with `status: 'INCOMPLETE'`, algo orders also filtered by `INCOMPLETE`
- **WebSocket updates**: Execution reports add/update individual orders in real-time
- **Deduplication**: `mapLimitOrder` and `mapAlgoOrder` use `addOrUpdateOrder` which matches by order ID
- **Stale order handling**: On WebSocket reconnect, `fetchAndSetPrivateSnapshot()` is called to re-fetch all orders

### Position State

- **REST polling**: Fetched every tick via `GET /v1/positions`
- **Private WS**: The `position` topic applies real-time camel-case rows through `mapPosition()`.
- **Fake positions**: `OrderlyExchange.fetchPositions` creates zero-contract rows in the symbol's cached default margin mode; real positions override matching fakes via `uniqBy` on `id`.
- **Margin mode and identity**: REST `margin_mode` and WS `marginMode` map to `MarginType`. Isolated IDs use `<legacy-id>:isolated`, avoiding collisions with simultaneous Cross positions.
- **Leverage and margin**: The documented position `leverage` is preferred, then `1 / imr`, then `accountLeverage`. `marginSize` uses the allocated margin when positive and otherwise derives `notional / leverage`.

### Balance State

- Updated from the positions endpoint response fields `free_collateral` and `total_collateral_value`.
- uPNL is computed client-side from positions and reconciled into the balance in `OrderlyExchange.tick()`.

## Exchange Metadata

- **Position mode**: OneWay only (`PositionMode.OneWay`) - hardcoded, no hedge mode support
- **Margin type**: Cross and Isolated are exposed for every Orderly-family integration; the selector is per symbol (`marginTypeIsGlobal: false`).
- **Leverage**: `setLeverage(symbol, leverage)` targets that symbol and its selected/default margin mode via `POST /v1/client/leverages`. `setAllLeverage` remains the Cross batch path.
- **Leverage constraints**: Per-symbol writes clamp to the reconciled market limit. Cross batch writes round to an integer and clamp to the venue-supported 1-100 range.

## Testnet/Demo

| Component  | Livenet                                                 | Testnet                                                         |
| ---------- | ------------------------------------------------------- | --------------------------------------------------------------- |
| REST API   | `https://api-evm.orderly.org`                           | `https://testnet-api-evm.orderly.org`                           |
| Public WS  | `wss://ws-evm.orderly.org/ws/stream`                    | `wss://testnet-ws-evm.orderly.org/ws/stream`                    |
| Private WS | `wss://ws-private-evm.orderly.org/v2/ws/private/stream` | `wss://testnet-ws-private-evm.orderly.org/v2/ws/private/stream` |
| Broker ID  | `ab82cb09-cfec-4473-80a3-b740779d0644`                  | `e708a644-9ce5-46c6-b50c-74b15f62d8ca`                          |

Testnet selection is via `options.testnet` boolean.

## Known Quirks / Implementation Notes

1. **ed25519 key prefix**: Keys must start with `ed25519:`. The `OrderlyExchange` constructor prepends this prefix if missing.
2. **Leverage and margin defaults are per symbol/mode**: `setLeverage` sends `symbol`, `leverage`, and `margin_mode` to `/v1/client/leverages`. `changeMarginType` updates the symbol's `default_margin_mode`, caches it only after `success: true`, and order placement still sends `margin_mode` explicitly. `setAllLeverage` intentionally remains a Cross batch update.
3. **Positional TP/SL**: TP/SL are placed as `POSITIONAL_TP_SL` algo orders with `child_orders` array containing `STOP_LOSS` and/or `TAKE_PROFIT` children with `type: 'CLOSE_POSITION'`.
4. **Sibling order management**: When placing a new TP or SL, the code checks for existing sibling orders. If found, it cancels the parent algo order and re-places both together. This prevents orphaned TP/SL orders.
5. **Cancel algo order re-creates sibling**: `OrderlyExchange.cancelAlgoOrder` automatically re-creates the sibling when cancelling one side of a TP/SL pair.
6. **Bulk cancellation is direct and complete**: `cancelAllOrders` and `cancelSymbolOrders` sequentially await both normal (`DELETE /v1/orders`) and algo (`DELETE /v1/algo/orders`) paths. They must not call `cancelAlgoOrder`, because that helper re-creates a TP/SL sibling; each path is attempted even if the other fails.
7. **Regular updates are strict stable-ID native edits**: `OrderlyExchange.updateOrder` sends `PUT /v1/order` only for an exactly reconciled REST `NEW` zero-fill ordinary GTC LIMIT row with safe canonical ID, raw symbol/type/side parity, explicit boolean `reduce_only`, full visibility, aligned price/quantity precision, and explicit `CROSS` or `ISOLATED` `margin_mode`. It sends required identity plus only the changed price or quantity and accepts only `success: true` + `EDIT_SENT`. Unsupported, partial, no-op, malformed, camel-case WS-only, unknown-mode, or ambiguous regular rows fail before any write and never cancel/recreate. The established algo child update remains separate.
8. **White-label `order_tag`**: Each white-label variant uses a different `order_tag` determined by `this.orderlyExchangeName` in `placeOrder()`.
9. **Large order splitting**: Orders exceeding `market.limits.amount.max` are automatically split into `Math.ceil(amount / maxSize)` lots plus a remainder.
10. **Algo order already cancelled**: If algo cancel fails with an "already canceled" message, `cancelAlgoOrder` updates the local status to Canceled without throwing.
11. **Balance from positions endpoint**: Balance is not fetched from `/v1/client/holding`. Instead it comes from the `/v1/positions` response fields (`total_collateral_value`, `free_collateral`). Commented-out balance polling code exists in `tick()`.
12. **Cancel order uses query params**: Despite being a DELETE, `cancelOrder` sends `order_id` and `symbol` as URL query parameters, not as a request body.
13. **Params serialization**: Axios's alphabetical `paramsSerializer` applies only to unsigned public requests. `signRequest()` serializes signed params itself in insertion order and signs exactly that query string; do not assume the two serializers are interchangeable.
14. **cancelSymbolOrders**: Symbol-scoped cancellation converts the internal symbol with `reverseSymbol()` before issuing both normal and algo DELETEs.
15. **Kline timestamp**: `fetchOHLCV` converts millisecond API timestamps to internal seconds and reverses results into ascending order.
16. **Transient validation must not deactivate** (`start()` + `isTransientOrderlyValidationError`): `start()` runs on **every (re)connect**, and Orderly is the **only** exchange that emits `'deactivate'`. The web app turns `'deactivate'` into `account.active = false` (`exchangeAccountController` `deactivate-status` handler), and `CleanupComponentWorker` then **force-switches the user off their selected account**. So the account-validation `GET /v1/client/info` at the top of `start()` must only `emit('deactivate')` on a **definitive 4xx auth rejection**; a transient failure (no HTTP response — network/CORS/proxy/tunnel drop — or 408 / 429 / 5xx) must **`throw`** instead, so the worker treats it like any other exchange's transient `start()` failure (logs it, leaves the account ACTIVE, recovers on the next reconnect). Do **not** route this back through `validateAccount()` (which collapses the error to a string and loses the HTTP status needed to classify it). Past bug: a reconnect-time blip on this one request silently disabled a valid account and yanked the user's active-account selection.
17. **Trailing-stop placement is gated**: Orderly's upstream API supports `TRAILING_STOP`, but this integration does not expose the required callback-rate/value inputs and every family member advertises `supportsTrailingStopLoss: false`. `placeAlgoOrder` therefore emits an actionable error and rejects direct trailing-stop calls instead of posting an empty `POSITIONAL_TP_SL` payload. The mapper remains read-capable for trailing stops created elsewhere.
18. **Simultaneous margin modes require explicit write targeting**: The store can represent Cross and Isolated positions for one symbol. Common `PlaceOrderOpts` has no margin-mode field, so writes use the symbol selector/default cache; reduce-only writes use the single matching open position when unambiguous and otherwise fail before REST. Call `changeMarginType` first when both modes have a matching position.

## Error Handling

- **Validation failure on start**: Auth rejections (4xx) emit the error and `'deactivate'`, then return without starting WebSockets or the tick loop. Transient failures (no response / 408 / 429 / 5xx) throw instead — see Known Quirks → "Transient validation must not deactivate".
- **Tick errors**: `OrderlyExchange.tick` catches and logs errors with `emitAndLogError`, then continues the loop.
- **Network retries**: `createAPI()` configures 3 retries on network errors via `axios-retry`.
- **Algo cancel "already canceled"**: `cancelAlgoOrder` marks the order Canceled locally instead of throwing.
- **Fetch fallbacks**: Most fetch methods (tickers, positions, limit orders, algo orders) catch errors and return the existing store state or empty array rather than throwing.
- **Startup leverage fetch**: The intentionally fire-and-forget account-leverage request has an explicit rejection handler that emits the failure while startup continues.
- **PlaceOrder errors**: `OrderlyExchange.placeOrder` logs errors via `emitAndLogError` and rethrows them to the caller.
- **Native regular amend errors**: Every regular-order preflight failure emits and throws before a write. Signing/network errors and non-`EDIT_SENT` responses are emitted and rethrown; they never fall back to cancel/recreate.

## Tick Loop

- Runs on configurable interval via `options.extra.tickInterval` using `loop()` utility
- **Every tick**: Positions fetched (also updates balance from position response), uPNL recalculated
- **Every 4th tick** (`count % 4 === 0`): Orders fetched (both limit and algo)
- Count increments each iteration, passed recursively

## Testing / Validation

Fresh deterministic Orderly regressions live under `src/__tests__/audit-regressions/`. They cover bulk algo cancellation, private-WS cumulative fills, REST/WS Cross and Isolated position mapping, collision-safe position identity, private position subscription, default-mode loading, margin-mode and leverage writes, explicit-mode order placement, ambiguous reduce-only rejection, per-symbol leverage caps, the 100x Cross batch clamp and metadata, orderbook snapshot retry, startup error observation, fail-closed trailing-stop placement, and the stable-ID regular LIMIT amend boundary for both proven margin modes. They do not make network calls or place live orders.

## CCXT Reference

Orderly Network is not available in CCXT. Use https://github.com/ccxt/ccxt for general exchange pattern reference.

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `client_order_id`, rule up to 36 characters not starting with `-`.
