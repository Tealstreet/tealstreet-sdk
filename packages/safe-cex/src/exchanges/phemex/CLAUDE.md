# Phemex Exchange Integration

This directory contains the Phemex perpetual futures exchange integration for Tealstreet.

## Exchange Information

- **Type**: Centralized Exchange (CEX)
- **Products supported**: USDT-margined and USDC-margined perpetual futures (`PerpetualV2`)
- **Position modes**: Hedged (default) and One-Way
- **Margin types**: Cross and Isolated (indicated by leverage sign convention)

## API Documentation

- **Official API Documentation**: https://phemex-docs.github.io/

## CCXT Reference Implementation

- **REST API Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/phemex.ts
- **WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/phemex.ts

## File Structure

- `phemex.api.ts` - REST API client with HMAC-SHA256 authentication and request signing
- `phemex.exchange.ts` - Main exchange class (`PhemexExchange`) extending `BaseExchange`
- `phemex.types.ts` - TypeScript type definitions, endpoint constants, base URLs, and field mappings
- `phemex.ws-private.ts` - Private WebSocket handlers for orders, positions, and account balance
- `phemex.ws-public.ts` - Public WebSocket handlers for tickers, klines, and orderbook

## Authentication

### REST API (`phemex.api.ts`)

Signing uses HMAC-SHA256 with the following process:

1. Compute an expiry timestamp: `Math.round((currentTime + 60000) / 1000)`
2. Concatenate: `url + queryParams + expiryString` (+ JSON body if present)
3. Sign with `createHmac('sha256', secret).update(concatenated).digest('hex')`

**Request headers**:

- `X-PHEMEX-ACCESS-TOKEN` - API key
- `X-PHEMEX-REQUEST-EXPIRY` - Expiry timestamp (seconds)
- `X-PHEMEX-REQUEST-SIGNATURE` - HMAC-SHA256 hex signature
- `Content-Type: application/json`

**Time synchronization**: Uses `virtualClock.getCurrentTime()` for timestamps, not `Date.now()`. This is important for keeping request signatures valid when the local clock drifts.

**Account identity**: `getAccount()` reads `data.account.userID` from the
USDT `/g-accounts/accountPositions` response. A malformed success payload with
no user ID fails with an explicit error instead of returning a fabricated ID or
throwing from nested destructuring.

**Signer-failure branch does NOT say "auth failed".** The `remoteSigner.sign` error branch in `phemex.ws-private.ts auth` emits `Remote WS signing unavailable: …`, not `Remote WS auth failed: …`. The latter wording matches `health/errorPatterns.ts`'s `auth_invalid` (case-insensitive substring), which the web controller treats as a terminal API-key rejection and suppresses automatic account reloads for five minutes — the very reload a signer-less tab needs to re-pair. See `../CLAUDE.md` → "Remote-signer WS auth" and the bybit template in `bybit/bybit.ws-private.ts`. The separate wrong-key emit (`'Phemex websocket error - wrong api key or secret'`, on `id === 999 && error`) is the real auth-failure path and is left as is.

### WebSocket Authentication (`phemex.ws-private.ts`)

1. Compute expiry: same as REST (`(currentTime + 60000) / 1000`)
2. Sign: `createHmac('sha256', secret).update(apiKey + expiryString).digest('hex')`
3. Send auth message: `{ method: 'user.auth', params: ['API', apiKey, signature, expiry], id: 999 }`
4. On successful auth (response `id: 999` with no error), subscribe to `aop_p.subscribe` for account/order/position snapshots and updates
5. On auth failure, emits error: `'Phemex websocket error - wrong api key or secret'`

## Startup Sequence

Periodic account reads and terminal-order reconciliation share a checked
authority revision. Private account/position pushes, socket lifecycle changes,
and accepted REST commits advance it before updating the store. A pending read
may commit only while its captured revision still matches, including after any
symbol-scoped recovery await; stale reads cannot clear newer positions/balances.

The `start()` method (`phemex.exchange.ts:108`) orchestrates initialization:

1. **Parallel fetch** (`Promise.all`):
   - `fetchMarkets()` -> on completion, stores markets, sets `loaded.markets = true`, then immediately calls `publicWebsocket.connectAndSubscribe()` (public WS starts as soon as markets are loaded)
   - `fetchTickersRaw()` -> raw ticker data fetched in parallel with markets
2. **Parse tickers**: After both resolve, `parseTickers()` converts raw data, stores tickers, sets `loaded.tickers = true`
3. **Branch by mode**:
   - **Public data only** (`_startPublicDataOnly`): Sets empty orders/balance/positions, marks all as loaded. Logs "Ready to watch public data on Binance" (legacy log message).
   - **Private mode** (`_startPrivate`):
     1. Connects private WebSocket (`privateWebsocket.connectAndSubscribe()`)
     2. Sets `isHedged = true` (hardcoded default)
     3. Calls `tick()` which fetches positions and balance via REST
     4. Logs "Ready to trade on Phemex"
     5. Initial order loading is NOT done via REST -- relies on the WebSocket `aop_p` subscription snapshot

**Note on order loading**: REST `fetchOrders()` is commented out. Initial orders come from the private WebSocket snapshot (`type: 'snapshot'` in `aop_p` subscription), and `fetchOrders()` simply returns `this.store.orders`. Successful `placeOrder()` REST create responses are also normalized and upserted immediately when they describe an open order, so newly created stop orders do not depend on the private WebSocket echo arriving before the UI assertion window.

### Tick Loop

- REST and `accounts_p` pushes use the same balance convention: each currency's
  `total = accountBalanceRv - position UPnL`, `free = accountBalanceRv - totalUsedBalanceRv`,
  and `used = totalUsedBalanceRv`. Aggregate `total`, `free`, and `used` sum those
  independent currency fields; free must not be derived from normalized total.
  This keeps a fresh REST poll from changing total solely because it followed a
  WebSocket update, including when UPnL is negative.
- `tick()` runs on a configurable interval via `options.extra.tickInterval`
- Each tick calls `fetchPositionsAndBalance()`, which fetches both `currency=USDT` and `currency=USDC`, merges their positions and balances, then returns one complete snapshot for the full store write. If one settlement-account request fails or returns an invalid payload, a successful sibling snapshot still refreshes and the failed currency's last-known balance/positions are retained until its next successful poll. With no prior slice, the unavailable currency is represented as zero so per-market margin lookup fails closed rather than falling back to the sibling aggregate.
- Rust: an unmappable position row (REST or stream) is skipped and logged; the store keeps that symbol's positions and the currency balance still commits. A REST row without a readable symbol keeps every stored position of that currency. Order rows with unsupported status/type skip the same way; malformed numbers on known order rows still reject the frame.
- Position row mapping intentionally does not require a populated `marketsBySymbol` index. Full account snapshots filter to known markets before committing the store, while symbol-scoped recovery may need to preserve a valid nonzero REST row during hosted startup or stale-index windows.
- Order formatting reads zero-size symbol-scoped raw position rows when mapped positions are empty, because Phemex reports one-way mode as `posMode: "OneWay", posSide: "Merged"` even when there is no open position. Dropping those zero rows from the store is correct, but opening orders must still send `posSide: "Merged"` for one-way accounts.
- On error, logs the error and continues the loop
- Uses `loop()` utility for scheduling

## History axes: NONE implemented, but the venue publishes up to three

phemex's history method was an unconditional `return []` — an **unbranded stub**, so
a brand check could not see it and it read as a real implementation.
`exchange.capabilities` now treats such a stub as absent. More importantly, do not
read that stub as a statement about phemex:

| Venue route                               | Axis                          | Notes                                                                    |
| ----------------------------------------- | ----------------------------- | ------------------------------------------------------------------------ |
| `/g-positions/positions-history`          | **Reported closed positions** | The strongest axis kind — held today only by okx, bybit, blofin, bitunix |
| `/api-data/futures/v2/tradeAccountDetail` | Ledger                        | `amountRv`, `typeDesc`, **running `balanceRv`**, `createTime`            |
| `/g-orders/orderTrade`                    | Fills                         | `execQty`, `execPriceEp`, `execFeeEv`, **`fundingFeeEv`**, `tradeType`   |
| `/g-orders/list`                          | Order history                 | `offset`/`limit` ≤ 200, `start`/`end` in ms                              |

The running `balanceRv` is what makes this venue cheap to grade: consecutive-delta arithmetic
against a venue-supplied balance column is how bitget's gross-vs-net convention was pinned
without guessing. Do that before assuming a profile.

**Probing hazard, separate from all of the above and still live:** an earlier probe of the
executions route wedged unrelated REST on this venue, including public `/products`. Phemex
is symbol-scoped for fills and has no enumerable income ledger, so ingestion reports the
fills axis as `unsupported` and does not invoke `fetchHistoricalFills`. Do not add a new
metadata bypass for this; fix either the symbol source or the tunnel/proxy behavior first.
Probe narrowly, one route at a time, and expect to need a fresh connection after.

## REST API Endpoints

All endpoints are defined in `ENDPOINTS` constant in `phemex.types.ts`:

| Constant                | Path                                | Method  | Description                                                              |
| ----------------------- | ----------------------------------- | ------- | ------------------------------------------------------------------------ |
| `MARKETS`               | `/public/products`                  | GET     | Fetch available perpetual markets (`perpProductsV2`)                     |
| `TICKERS`               | `/md/v3/ticker/24hr/all`            | GET     | Fetch 24hr ticker data for all symbols                                   |
| `ACCOUNT`               | `/g-accounts/accountPositions`      | GET     | Fetch account balance and positions (params: `currency=USDT` or `USDC`)  |
| `POSITIONS`             | `/g-accounts/accountPositions`      | GET     | Same endpoint as ACCOUNT, also accepts `symbol` param                    |
| `ORDER`                 | `/g-orders/create`                  | **PUT** | Place a new order                                                        |
| `CANCEL_ORDER`          | `/g-orders/cancel`                  | DELETE  | Cancel a single order (params: `symbol`, `posSide`, `orderID`)           |
| `BATCH_ORDERS`          | `/g-orders`                         | DELETE  | Cancel multiple orders (up to 10 per request; `orderID` comma-separated) |
| `SET_LEVERAGE`          | `/g-positions/leverage`             | **PUT** | Set leverage for a symbol                                                |
| `SET_LEVERAGE_CONTRACT` | `/positions/leverage`               | **PUT** | Set leverage (contract endpoint, unused)                                 |
| `SET_POSITION_MODE`     | `/g-positions/switch-pos-mode-sync` | **PUT** | Switch between Hedged/OneWay position mode                               |
| `USER_TRADES`           | `/exchange/order/v2/tradingList`    | GET     | User execution history (primary for `fetchHistoricalFills`)              |
| `USER_TRADES_LEGACY`    | `/exchange/order/trade`             | GET     | Legacy fallback execution history endpoint                               |
| `KLINE`                 | `/exchange/public/md/v2/kline/list` | GET     | Fetch OHLCV candlestick data                                             |
| `ORDERBOOK`             | `/md/v2/fullbook`                   | GET     | Fetch full orderbook snapshot (params: `symbol`, `limit`)                |

**Base URLs** (`BASE_URL`):

- Livenet: `https://api.phemex.com`
- Testnet: `https://testnet-api.phemex.com`

**Important**: Order placement and leverage changes use HTTP PUT method, not POST.

Execution-history behavior (`fetchHistoricalFills`):

- Calls `USER_TRADES` first, then `USER_TRADES_LEGACY` on fallback path
- Filters non-trade execution rows (`execType !== trade`) before mapping
- Normalizes symbol/side/timestamp and returns Unix-second execution timestamps

**Currently not collected by ingestion.**

The implementation exists, but invoking it was observed to wedge the shared
Singapore HOW WebSocket tunnel for users in restricted jurisdictions or with
incomplete proxy whitelisting. Symptoms: unrelated public REST endpoints
(`/public/products`) time out at 20s, markets fail to load, chart subscriptions
get dropped after 30s. The current protection is that Phemex is symbol-scoped
for fills and cannot enumerate traded symbols from an income ledger, so
`isAxisSupported` reports fills as `unsupported` and never calls the route. The
exact upstream tunnel/proxy interaction has not yet been isolated; do not make
Phemex collectible until a production canary confirms no regression.

## WebSocket Implementation

### WebSocket URLs (`BASE_WS_URL`)

- Public livenet: `wss://ws.phemex.com`
- Public testnet: `wss://testnet-api.phemex.com/ws`
- Private livenet: `wss://ws.phemex.com`
- Private testnet: `wss://testnet-api.phemex.com/ws`

### Public WebSocket (`phemex.ws-public.ts`)

Class name is `BinancePublicWebsocket` (legacy naming from fork).

| Method            | Channel                           | Description                                                     |
| ----------------- | --------------------------------- | --------------------------------------------------------------- |
| On connect        | `perp_market24h_pack_p.subscribe` | Subscribes to all perpetual 24hr ticker updates                 |
| `listenOHLCV`     | `kline_p.subscribe`               | Subscribes to kline data; params: `[symbol, resolution]`        |
| `listenOrderBook` | `orderbook_p.subscribe`           | Subscribes to orderbook incremental updates; params: `[symbol]` |

**Ping/pong**: Sends `{"method":"server.ping","params":[]}` with incrementing request IDs. Pong is detected by `json?.method === 'server.ping'` or `json?.result === 'pong'`. The `handlePongEvent()` from `BaseWebSocket` schedules the next ping after 5 seconds, and sets a 7.5-second timeout for pong miss that triggers reconnection.

**Reconnection**: Inherits `BaseWebSocket.onClose` which handles cleanup and reconnection. On reconnect, `onOpen` re-subscribes to `perp_market24h_pack_p`, resets orderbook state, and re-subscribes to all active kline/orderbook topics.

**Ticker data format**: Array-based packed format where each row contains:
`[symbol, openRp, _, _, lastRp, volumeRq, _, openInterestRv, indexRp, markRp, fundingRateRr, _, bid, ask]`

Bid and ask are optional fields -- only included if present (`phemex.ws-public.ts:153-154`).

**Orderbook management**: Snapshot-first approach:

1. Subscribe to full-depth WS incremental updates (`orderbook_p.subscribe`, `[symbol, false, 0]`). The omitted depth defaults to 30 and leaves deeper REST levels stale. Full-depth periodic snapshots replace the book.
2. On first WS message, trigger REST snapshot fetch via `/md/v2/fullbook` (limit: 1000)
3. Buffer all WS updates while snapshot loads
4. After snapshot arrives, filter buffered updates by `sequence` number (discard updates older than snapshot)
5. Apply buffered updates, then process subsequent updates incrementally. Dispatch preserves the outer frame sequence on the book payload; dropping it discards every buffered delta during a slow REST snapshot.
6. `processOrderBookUpdate` applies each delta through shared `updateOrderBookByPrice`: bids stay descending, asks stay ascending, and zero-amount updates for absent levels are ignored.
7. If the REST snapshot request fails or times out, the failure is caught locally, logged with a cooldown, and the buffered deltas are cleared so the next WS delta can retry snapshot loading without surfacing an unhandled promise rejection. Late snapshots after unsubscribe/disposal are ignored.

**OHLCV streaming**: Kline topic format is `kline_p.subscribe|||{symbol}|||{resolution}`. Message handler key uses interval and symbol: `kline_p.{interval}.{symbol}`. Each kline message contains up to 2 rows (current and previous candle). The handler processes both and filters by matching interval. Kline data format: `[timestamp, interval, lastClose, open, high, low, close, volume, turnover]`.

**Inbound dispatch**: `onMessage` does not scan the handler map or search the
raw frame body. It routes the packed ticker by exact `method`, orderbook frames
by `orderbook_p.{symbol}`, and kline frames by translating row field 1 (wire
resolution) back to the exact `kline_p.{interval}.{symbol}` handler key. One
kline/orderbook frame records one stats hit for its matched topic; the fixed
packed-ticker topic preserves its historical per-callback stats accounting.

**Topic management**: Uses a `topics` dictionary with reference counting. Topics are unsubscribed only when all listeners are removed (count reaches 0).

The always-on ticker stream plus dynamic topics are counted against Phemex's 20-subscription connection cap. A warning is logged at 18/20 active subscriptions, and zero-reference topics are deleted so reconnect only restores live listeners.

### Private WebSocket (`phemex.ws-private.ts`)

Class name is `BinancePrivateWebsocket` (legacy naming from fork).

After authentication, subscribes to `aop_p.subscribe` (Account/Order/Position).

**Message types** (all in same subscription):

- `accounts_p` - Account balance updates
- `orders_p` - Order updates (snapshot on connect, then incremental)
- `positions_p` - Position updates

**Ping/pong**: Same `server.ping` method as public WS, with `sendWithLog` for grafana tracking.

**Snapshot handling**: On initial `type: 'snapshot'`, replaces all orders in store (only keeps `OrderStatus.Open` orders). Subsequent updates use `addOrUpdateOrders`.

**Order mapping**: Private WebSocket `orders_p` rows and successful REST create response rows both go through `PhemexExchange.mapOrderUpdate()`. The mapper accepts both WS field names (`ordType`, `orderQty`) and REST-create field names (`orderType`, `orderQtyRq`), because `/g-orders/create` can return an already usable open stop row before the WebSocket echo arrives.

**Fill emission**: Fills are emitted only for non-snapshot messages. Fills are deduplicated using `uniqBy` on order ID. Fill price uses `execPriceRp` from raw order info when available, falling back to the formatted order price.

**Duplicate order status handling** (`phemex.ws-private.ts:358-367`): When a single message batch contains the same `orderID` with both open and closed statuses, the code detects the closed status update and overrides the open one. This prevents transient open states from appearing in the UI.

**Debounced REST reconciliation on terminal order events**: For non-snapshot `orders_p` messages containing terminal statuses (`filled`, `triggered`, `canceled`, `rejected`), the private WS schedules a 1-second debounced `fetchPositionsAndBalance()` call. This fast-follow sync reduces position-open/close lag when WS position payloads are delayed or partial. A scoped recovery read is skipped only when all affected native `posSide` legs have finite nonzero rows in the full snapshot (`Merged` requires a one-way row). Missing/flat legs or unknown native sides still recover. Nonzero full-snapshot IDs remain authoritative even when another leg requires recovery; a lagging scoped row cannot overwrite them.

## Order Types and Mappings

Defined in `ORDER_TYPE` mapping:

| Internal Type      | Phemex `ordType`                                                     |
| ------------------ | -------------------------------------------------------------------- |
| `Market`           | `Market`                                                             |
| `Limit`            | `Limit`                                                              |
| `StopLoss`         | `Stop`                                                               |
| `TakeProfit`       | `MarketIfTouched`                                                    |
| `TrailingStopLoss` | `MarketIfTouched` (with `pegPriceType: TrailingStopPeg`)             |
| `Conditional`      | Disabled; fails closed until `stopPxRp` + `triggerType` are verified |

**Order status mapping** (`ORDER_STATUS`):

- `new`, `untriggered`, `partially_filled`, `partiallyfilled` -> `Open`
- `triggered`, `filled` -> `Closed`
- `rejected`, `canceled` -> `Canceled`

**Time in force options** (`TIME_IN_FORCE`):

- `GoodTillCancel`, `ImmediateOrCancel`, `FillOrKill`, `PostOnly`

**Stop/TP order type detection**: Uses `stopDirection` and `side` fields:

- `Rising` + `Sell` = TakeProfit; `Rising` + `Buy` = StopLoss
- `Falling` + `Sell` = StopLoss; `Falling` + `Buy` = TakeProfit
- Handles `Stop`, `StopLimit`, `MarketIfTouched`, and `LimitIfTouched` ordTypes
- Unrecognized types default to `OrderType.Market`

**Trigger types**: `ByMarkPrice` or `ByLastPrice` (sent as `triggerType`, `slTrigger`, `tpTrigger` fields)

**Broker ID**: All orders include `clOrdID` prefixed with `Tealstreet_` followed by a UUID (`phemex.api.ts:15`).

**Reduce-only detection** (WS): Checks both `order.reduceOnly` boolean and `execInst` field for `'ReduceOnly'` or `'CloseOnTrigger'` strings (`phemex.ws-private.ts:409-411`).

## Order Lifecycle

### Order Placement (`placeOrder`, `phemex.exchange.ts:752`)

1. Calls `formatCreateOrder()` which delegates to `formatCreateTrailingStopLossOrder()` for trailing stops
2. For standard orders:
   - Resolves position side via `getOrderPositionSide()`
   - Splits into lots if amount exceeds `maxOrderQtyRq`
   - Each lot gets a unique `clOrdID` (broker prefix + UUID)
   - Stop/TP orders set `closeOnTrigger: 'true'`
   - Attached TP/SL use `stopLossRp` and `takeProfitRp` fields on the same order payload
3. Each payload sent via PUT to `/g-orders/create`
4. Checks response: `code !== 0` is an error; `ordStatus: 'Suspended'` with `execStatus: 'PendingAllocate'` indicates insufficient margin
5. Successful response rows are passed through `mapOrderUpdate()` and open orders are immediately upserted into the store before returning order IDs. The WebSocket remains authoritative later, but this covers the normal CI/UI window where the REST create succeeds and the private WS echo is delayed.

### Order Update (`updateOrder`, `phemex.exchange.ts:708`)

Uses **cancel-and-replace** pattern (no native amend):

1. Extracts the real order ID by splitting on `__` (strips `__stop_loss` / `__take_profit` suffixes)
2. Finds all related orders (parent + child TP/SL orders sharing the same base ID)
3. If multiple related orders exist (parent with TP/SL children):
   - Reconstructs the full order payload from the parent and child orders
   - Updates the specific price being changed (parent price, stopLoss price, or takeProfit price)
4. Cancels ALL related orders via `cancelOrders()`
5. Re-places the reconstructed order via `placeOrder()`

### Child Order (TP/SL) Handling

When a parent order has attached TP/SL, the WebSocket reports them as:

- Parent: `{orderID}` with `stopLossRp` and `takeProfitRp` fields
- The WS handler creates virtual child orders: `{orderID}__stop_loss` and `{orderID}__take_profit`
- Child orders have `amount: 0` (they inherit from parent context)
- Child orders have inverted side relative to the parent
- Trigger types are read from `slTrigger` and `tpTrigger` fields respectively

### Cancel Batching (`cancelOrders`, `phemex.exchange.ts:635`)

1. Deduplicates orders by stripping `__stop_loss`/`__take_profit` suffixes and using `uniqBy` on ID
2. Resolves position side for each order via `getOrderPositionSide()`
3. Groups orders by `{posSide}:{symbol}`
4. For single-order groups: uses `/g-orders/cancel` with `symbol`, `posSide`, `orderID`
5. For multi-order groups: chunks IDs into batches of 10, uses `/g-orders` with comma-separated `orderID`
6. Validates all responses have `code === 0`
7. Updates store to mark orders as canceled

### Trailing Stop Loss (`formatCreateTrailingStopLossOrder`, `phemex.exchange.ts:897`)

- Metadata keeps `supportsTrailingStopLoss: false`; this dormant path is not exposed in the UI and must be live-validated before enablement.
- Resolves position for the symbol and opposite side
- Calculates `priceDistance` as absolute difference between `ticker.last` and `opts.price`
- Sets `pegPriceType: 'TrailingStopPeg'` and `pegOffsetValueRp` (negative for sell, positive for buy)
- Always uses the full position size (`position.contracts`)
- Uses the documented `orderQtyRq` quantity field and does not send the non-contract `type` field
- Sets `ordType: 'Stop'` (not `MarketIfTouched`)

### Opening Trigger Orders

Metadata keeps `supportsOpeningTriggerOrders: false`. `formatCreateOrder` explicitly rejects `OrderType.Conditional` so non-UI callers cannot send the old malformed `priceRp` payload. Before enabling, implement and verify `stopPxRp` plus `triggerType` according to `CONDITIONAL_ORDERS.md`.

### Position Side Resolution (`getOrderPositionSide`, `phemex.exchange.ts:937`)

- Returns `'Merged'` for one-way mode
- For hedged mode: Buy -> `'Long'`, Sell -> `'Short'`
- Inverts the side for reduce-only, StopLoss, TakeProfit, and TrailingStopLoss orders
- Uses `opts.info?.posSide` if already available (for order updates)

## Symbol Format

- **Internal format**: Uppercase string, e.g., `BTCUSDT`, `ETHUSDC`
- **`formatSymbolIn`**: Converts exchange symbol to uppercase (simple `.toUpperCase()`)
- **`formatSymbolOut`**: Looks up the original symbol from `market.info.symbol` to get the exchange-native format
- **Market ID format**: `{base}/{quote}:{margin}` (e.g., `BTC/USDT:USDT`)
- **Supported margin currencies**: USDT and USDC (filtered in `fetchMarkets`)
- **Market filter**: Only `PerpetualV2` type products are included

## Data Precision & Conversion

Phemex V2 API uses **real-value (Rp/Rv/Rr/Rq) suffixed fields** rather than the legacy scaled integer format (Ep/Ev). All price and quantity fields are string representations of decimals, not scaled integers.

### Field Naming Convention

| Suffix | Meaning         | Example field       |
| ------ | --------------- | ------------------- |
| `Rp`   | Real Price      | `priceRp`, `markRp` |
| `Rv`   | Real Value      | `accountBalanceRv`  |
| `Rr`   | Real Rate/Ratio | `leverageRr`        |
| `Rq`   | Real Quantity   | `orderQtyRq`        |

All values are parsed with `parseFloat()`. There are no integer scaling factors in the V2 (g-prefix) API endpoints used by this integration.

### Price & Amount Precision

- `market.precision.price` from `tickSize` field (e.g., `0.01`)
- `market.precision.amount` from `qtyStepSize` field (e.g., `0.001`)
- Prices and amounts are adjusted using `adjust()` utility before order placement
- Large orders are automatically split based on `maxOrderQtyRq` from market info

### Balance Calculation (`phemex.exchange.ts:407-418`)

- `total = accountBalanceRv` (bonus balance is excluded -- the `bonusBalanceRv` addition is commented out)
- `used = totalUsedBalanceRv`
- `free = total - used`
- `upnl` is summed from parsed positions

### uPNL Calculation (`phemex.exchange.ts:328`)

- Uses `ticker.last` (not mark price): `upnl = contracts * (last - entryPrice)` for longs, inverted for shorts
- Entry price sourced from `avgEntryPrice` or `avgEntryPriceRp` fields

## Rate Limiting

- **Rate-limited client** (`xhr`): `maxRPS: 3` (3 requests per second)
- **Unlimited client** (`unlimitedXHR`): No local client rate limit; `placeOrder` and `cancelOrders` use it so chaser cancel-and-replace and split orders do not queue behind account polling. Phemex's current REST contract-group quota is 500 requests/minute, so upstream 429 handling still applies.
- HTTP-over-WebSocket: Uses `axiosHttpOverWebsocketAdapter` when `tealstreetTunnel` is enabled
- Request timeout: 20,000ms (hardcoded in `phemex.api.ts:25`, overrides `recvWindow`)
- Axios retry: 3 retries on network errors (`isNetworkError` condition)

## State Reconciliation

### Orders

- **Initial state**: Comes from WebSocket `aop_p` subscription snapshot (`type: 'snapshot'`)
- **Created-order upsert**: Successful `/g-orders/create` responses are normalized through the same mapper as WebSocket order updates and open rows are merged into the store immediately.
- **Snapshot handling**: On snapshot, replaces entire order store with only `OrderStatus.Open` orders; non-open orders in the snapshot are filtered out (`phemex.ws-private.ts:354`)
- **Incremental updates**: Subsequent messages use `store.addOrUpdateOrders()` which merges by order ID
- **Batch deduplication**: Within a single WebSocket message, if the same `orderID` appears with both open and closed statuses, the closed status takes precedence (`phemex.ws-private.ts:358-367`)
- **REST fallback**: `fetchOrders()` simply returns `this.store.orders` -- there is no REST reconciliation. The REST-create upsert is only for orders this client just placed; it is not a general open-orders fetch. A `TODO: fetchAndSetPrivateSnapshot` comment exists in `phemex.exchange.ts`.

### Positions

- **REST polling**: Positions are fetched via REST in the `tick()` loop using `fetchPositionsAndBalance()` across both USDT and USDC settlement accounts before the full replacement
- **WebSocket updates**: `positions_p` messages map and upsert positions via `store.addOrUpdatePositions()` (not update-only)
- **Zero-contract guard**: Brand-new zero-contract WS positions are ignored unless they already exist in the store; full cleanup is handled by periodic/full REST sync (`addOrUpdatePositionsFull`)
- **Filtering**: Only positions for markets present in the store are processed (both REST and WS)
- **Full replacement on tick**: `tick()` uses `store.addOrUpdatePositionsFull(positions)` which replaces all positions
- **Terminal-order fast follow**: Filled/canceled/rejected order WS events enqueue their symbols and affected native position sides for the debounced sync, retaining every affected leg across the debounce window. After the full positions+balance fetch, `fetchPositions([symbol])` runs if any queued leg is missing/flat or its native side is unknown. Long/Short require that exact side; Merged requires a one-way row. Scoped rows may add missing IDs or replace flat IDs, but cannot overwrite finite nonzero full-snapshot IDs. This retains hedge-leg and hosted startup/stale-index recovery without regressing a fresh full-snapshot row. Failed currency slices retain their prior positions through the full-fetch helper and await later polling.

### Balance

- **REST polling**: Fetched alongside positions in `tick()` from both settlement accounts; `balance.currencies` is authoritative for per-market margin. Aggregate totals are the nominal USD-stable sum of USDT and USDC, not a conversion-rate feed.
- **WebSocket updates**: `accounts_p` messages merge every received account by its `currency` into `balance.currencies`, preserving the sibling settlement slice until its own update arrives.
- **Fast-follow sync**: Terminal order events trigger the same debounced positions+balance reconciliation described above.
- **WS balance total calculation**: each updated currency uses `total = accountBalanceRv - currency upnl` (different from REST which uses just `accountBalanceRv`); aggregate fields sum the currency slices.

## Exchange Metadata

### Grafana Monitoring

**Periodic endpoints** (tracked for latency monitoring): `POSITIONS`, `ACCOUNT`
**Every-call endpoints**: `BATCH_ORDERS`, `CANCEL_ORDER`, `ORDER`, `SET_LEVERAGE`, `SET_LEVERAGE_CONTRACT`, `SET_POSITION_MODE`

### Implementation Capabilities

#### Leverage (`setLeverage`)

- Per-symbol, per-side leverage setting via `setLeverageHelper`
- **Hedged mode**: Sets `longLeverageRr` and `shortLeverageRr` independently (or both if no side specified)
- **One-way mode**: Sets single `leverageRr`
- **Cross margin convention**: Negative leverage value indicates cross margin (e.g., `-10` = 10x cross)
- **Isolated margin**: Positive leverage value (e.g., `10` = 10x isolated)
- Uses PUT method on `/g-positions/leverage`
- `prepLeverage()` helper clamps to market min/max and preserves margin type sign unless `allowChangingMarginType` is true

#### Pre-flight throws must emit — this venue hid a failure behind it

`changeMarginType` and `setLeverageHelper` need the symbol's current leverage
and margin mode, because Phemex encodes margin type as the _sign_ of leverage.
Their `!position` / `!market` / `!ticker` guards used to be raw
`throw new Error(...)` with no `emitAndLogError` first — the exact pattern
`../CLAUDE.md` prohibits.

That is not cosmetic. A raw throw bypasses `BaseExchange.emitter`, so it reaches
**no** diagnostic surface: not the toast pipeline, not
`exchangeAccountController.on('error')`, and not the Cypress requests-log
artifact (which records only _emitted_ errors). The e2e margin-type step failed
with `expected 'Isolated' to equal 'Cross'` and **1551 logged requests contained
no negative `leverageRr` and no error at all** — the switch simply never
happened, invisibly.

Adding the emits is what found the actual bug on the next run: the log then
carried `14x Position ADAUSDT not found — open a position before changing
margin type`. Keep the emits; they are what makes this venue debuggable.

#### Never read leverage state through `getOrFetchPositionsForSymbol`

Use **`resolveLeverageContext(symbol)`**. Both callers previously read the
current leverage/margin mode off `getOrFetchPositionsForSymbol`, which is wrong
twice over:

- it early-returns `[]` whenever `store.loaded.positions` is false — the
  permanent state of the never-`start()`ed main-thread mirror that executes
  hosted-mode order writes (the same trap documented as bitget quirks 22/23);
- **`mapPosition` drops size-0 rows on purpose**, so a _flat_ symbol has no
  store position even on a fully loaded worker — and changing margin type
  while flat is the normal case.

Both guards then threw "Position not found" before issuing any request. Two
distinct user-visible failures came out of that: the margin switch silently
never happened, and `setLeverage` fell through to a **positive** `leverageRr`,
which flips a cross account to isolated as a side effect.

`resolveLeverageContext` reads the raw `accountPositions` row first, because it
is the current exchange state and is present at size 0 with `leverageRr` and
`crossMargin` populated. Only fall back to the store row if the raw read fails
or omits the symbol, and only when the store row carries both `marginType` and
`positionMode` — guessing a default is what flips the mode. This order matters:
symbol-scoped empty fetches cannot clear stale open rows via
`addOrUpdatePositions([])`, so a stale store row can otherwise override a
fresh flat-row margin switch. Read the raw row **raw**, not via `mapPosition`,
which would discard it.

#### A leverage change on a flat symbol must record `fetchedLeverage`

`mapPosition` drops size-0 rows, so after `setLeverage` on a symbol with no open
position there is nothing in `store.positions` for the place-order % slider to
read, and it falls back to **1x** — order sizes then come out `leverage` times
too small. The e2e margin-slider step catches this as `expected 5.45 to be close
to 55.5675` (22.23 available x 10x x 0.25).

`setLeverageHelper` therefore writes `store.fetchedLeverage[symbol]` after a
confirmed success, including the confirmed `marginType` and `positionMode`,
which is the app's documented no-position fallback (blofin's
`fetchLeverageForSymbol` is the reference implementation). The response code is
checked **before** any store mutation, so a rejected change cannot record
metadata the exchange never applied.

`changePositionMode` has the same flat-symbol requirement: Phemex can switch a
zero-size symbol from OneWay to Hedged and then still have no mapped
`store.positions` row because `mapPosition` intentionally drops size-0 rows.
After a successful switch, refresh `accountPositions` and persist the confirmed
`positionMode` / `marginType` in `fetchedLeverage[symbol]` so the UI selector
does not fall back to stale mode metadata.

#### Margin Type (`changeMarginType`)

- Implemented by toggling leverage sign via `setLeverageHelper` with `allowChangingMarginType = true`
- Cross margin: negative leverage; Isolated margin: positive leverage
- Current leverage comes from `resolveLeverageContext`, which works on a flat
  symbol — it does **not** require an open position

#### Position Mode (`changePositionMode`)

- Switches between `Hedged` and `OneWay` per symbol
- Uses PUT on `/g-positions/switch-pos-mode-sync`
- Params: `symbol`, `targetPosMode` (`Hedged` or `OneWay`)
- Requires all positions for that symbol to be closed first (error code `39201`)
- After switching, refetches positions for the symbol and merges with existing positions (filtering out old ones for that symbol)

#### Positions

- **Default mode**: `isHedged: true` (set on start, hardcoded)
- Position side determined by `posSide` field (`Long`/`Short`) or `Side` field (`Buy`/`Sell`)
- UPnL calculated against `ticker.last` (not mark price)
- Leverage sign indicates margin type: negative = cross, positive = isolated
- `crossMargin` boolean field also checked for margin type
- Position mode read from `posMode` field (`'Hedged'` or `'OneWay'`)

#### Not Implemented

- `fetchOrders()`: Returns store orders only (no REST call)
- `changeMarginType()`: Not exposed as independent REST call; uses leverage sign manipulation

## Testnet / Demo

- **REST base URL**: `https://testnet-api.phemex.com`
- **WebSocket URL**: `wss://testnet-api.phemex.com/ws` (same for public and private)
- URLs are selected via `options.testnet` boolean
- Metadata sets `supportsTestnet: false`; the dormant URLs are not a supported environment and the testnet WS host remains unverified
- Same broker ID (`Tealstreet_`) used for both environments

## Known Quirks / Implementation Notes

1. **Legacy class naming**: WebSocket classes are named `BinancePublicWebsocket` and `BinancePrivateWebsocket` (carried over from original fork). The exchange class itself is correctly named `PhemexExchange`.
2. **Log messages**: Some log messages reference "Binance" instead of "Phemex" (e.g., `_startPublicDataOnly` logs "Ready to watch public data on Binance" at `phemex.exchange.ts:173`).
3. **fetchOrders not implemented**: REST order fetching is commented out; initial order state relies on WebSocket `aop_p` subscription snapshot. `placeOrder()` still upserts successful REST-created open orders so the local store reflects orders this client just created while waiting for the WebSocket echo. There is a TODO comment in `phemex.exchange.ts` about implementing `fetchAndSetPrivateSnapshot`.
4. **API response format**: Success responses have `code: 0`. Non-zero codes indicate errors with message in `msg` field.
5. **Suspended orders**: Orders with `ordStatus: 'Suspended'` and `execStatus: 'PendingAllocate'` indicate insufficient margin (`phemex.exchange.ts:763`).
6. **Position side in orders**: Uses `posSide` field - `Merged` for one-way mode, `Long`/`Short` for hedged mode. For reduce-only/TP/SL orders in hedged mode, the position side is inverted.
7. **Kline daily+ alignment**: For resolutions >= 86400s (1 day), fetching adjusts the end time to UTC start of day minus one day to align with exchange candle boundaries (`phemex.exchange.ts:463-466`).
8. **Commented-out code**: Extensive commented-out code blocks in `formatCreateOrder()` and the WS private handler show alternative approaches for TP/SL as separate orders and COIN-M contract handling that were explored but not used.
9. **Order ID comment references Binance**: Comment at `phemex.exchange.ts:889` says "otherwise Binance will duplicate the IDs" -- leftover from the fork origin.
10. **Bonus balance excluded**: `bonusBalanceRv` is intentionally excluded from total balance calculation (commented out at `phemex.exchange.ts:407`).
11. **Position contracts**: The code has a comment about COIN-M contract calculation (`phemex.exchange.ts:324`) but uses raw `size` for USD-M since it is not contract-based.

## Error Handling

### REST API Errors

- **Response code check**: All API responses are checked for `code !== 0`. Non-zero codes indicate errors with the message in the `msg` field.
- **Suspended order detection**: `ordStatus: 'Suspended'` + `execStatus: 'PendingAllocate'` is treated as an error with message "Order is suspended. Likely insufficient margin."
- **Network retries**: Axios retry configured for 3 retries on network errors (`isNetworkError`).
- **Error propagation pattern**: Most methods catch errors, log via `emitAndLogError()`, and either return fallback data (e.g., existing store state) or re-throw.

### Specific Error Codes

- `39201` - "the original positions are not closed when switch pos mode" (position mode change, `phemex.exchange.ts:529-534`)

### WebSocket Errors

- **Auth failure**: Detected by `json?.id === 999 && json?.error`, emits "Phemex websocket error - wrong api key or secret"
- **Pong miss**: If no pong received within 7.5 seconds of ping, triggers reconnection (inherited from `BaseWebSocket.handlePongEvent`)

### Error-as-Success Patterns

- `cancelOrders()` checks each response in the batch for `code !== 0` and throws an aggregate error if any fail
- `fetchPositionsAndBalance()` returns existing store state on error rather than throwing
- `fetchMarkets()` returns existing `store.markets` on error

## OHLCV / Kline

**Supported timeframes** (mapped to seconds):
`1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `2h`, `3h`, `4h`, `6h`, `12h`, `1d`, `1w`, `1M`, `3M`, `1Y`

**REST fetch format**: Array rows `[time, _, _, open, high, low, close, volume]` (indices 1 and 2 are unused)

**WS streaming format**: `[timestamp, interval, lastClose, open, high, low, close, volume, turnover]`

**Fetch parameters**: Uses `from`/`to` (unix seconds), `symbol`, `resolution` (seconds string). Limit is capped at 1000.

## Testing / Validation

Exchange-specific unit coverage exists for private WS reconciliation:

- `src/__tests__/phemex.ws-private.test.ts` - Covers WS position upsert behavior, zero-contract guard behavior, and debounced REST reconciliation after terminal `orders_p` events.
- `validateAccount()` - Attempts GET on `/g-accounts/accountPositions` with `currency: USDT`. Returns empty string on success, error message on failure.
- `getAccount()` - Reads `data.account.userID` from the USDT account-position response for user identification.

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `clOrdID`, rule 1-40 characters. Replaces the generated `BROKER_ID_…` ID; no prefix is composed.
- The Rust wrapper uses the same rule and `Tealstreet_…` mint format. Both paths
  share `BROKER_ID` from `phemex.types.ts`; `phemex.api.ts` re-exports it for
  existing callers.


## Rust income and client-ID reads

The Rust engine adds reported account income through the documented
`/api-data/futures/v2/tradeAccountDetail` route. It preserves `amountRv` signs
and `typeDesc` verbatim; it does not derive gross PnL, funding, fees or closed
positions from the running balance. Both USDT and USDC are walked with the
shared span pager. Unknown types remain `other`. A failed sibling currency,
malformed row, oversized page or incomplete coverage is an error, never an
empty/partial success. These documented schema fixtures are not live account
validation and do not remove the ingestion/tunnel canary restriction above.

Ledger rows have no documented symbol or transaction ID. Symbol filtering
therefore fails explicitly. Returned `tranId` is a stable composite of currency,
creation time, type, amount, running balance and duplicate occurrence. The
default range is the last 30 days; explicit inclusive bounds use milliseconds.

Rust definitive lookup uses
`/api-data/g-futures/orders/by-order-id?symbol=...&clOrdID=...`, which includes
closed orders. Only a successful exact-ID empty result is definitive; failures
propagate and mismatched/malformed rows fail. Unknown markets return a
non-definitive miss without I/O. The primary key and primary egress always
perform this lookup even when ordinary GETs use secondary read credentials.
`Triggered` still maps to Closed without becoming a terminal store tombstone.

TS still has inert income/order-history fallbacks and a non-definitive lookup
fallback. Its implemented fill history is a symbol-scoped latest page capped
at 200, so this sweep does not claim unbounded Phemex TS history.

## Rust socket supervision

The Rust public socket uses `engine::socket::Supervisor`: a 10-second connection deadline, 20-second idle watchdog, native `server.ping` every five seconds, and capped exponential reconnect delay with jitter. Short open/close flaps retain the failure count; sustained traffic resets it. Public reconnect still invalidates book generations and restores active subscriptions.

Rust `setLeverage(symbol, leverage, side)` preserves the opposite hedge leg's
cached leverage in the required two-field request. Both values retain their
margin sign: negative in cross, positive in isolated. Only the selected leg's
settings and positions change after success. Omitting side changes both legs;
one-way mode uses `leverageRr` and ignores side. An unknown opposite hedge
leverage fails before a write rather than inventing a replacement value.

Rust `changePositionMode` also requires a symbol and sends one PUT for that
symbol. A successful mode-only write for a market absent from account rows
records its confirmed mode separately, without inventing margin or leverage.
Orders and leverage changes still require the full reconciled context. Fresh
authoritative account rows replace the mode-only record; reads started before
the write cannot overwrite it. Conflicting delayed stream rows also wait for
that authoritative read before replacing the confirmed mode. Missing, unknown
or rejected symbols never
trigger writes for other markets. The account hedge indicator reflects whether
any known symbol remains hedged.
