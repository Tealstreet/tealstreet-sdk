# Coinbase Exchange Integration

This directory contains the Coinbase Advanced Trade exchange integration for Tealstreet, supporting both futures (INTX perpetuals + CFM dated futures) and spot trading through a portfolio-based architecture.

## Exchange Information

- **Exchange**: Coinbase Advanced Trade (INTX perpetuals, CFM dated futures, spot)
- **Position Mode**: One-way only
- **Margin Type**: Cross only
- **Leverage**: Hardcoded to 10x (slider disabled in UI)
- **Default Quote**: `USD`
- **Default Symbol**: `BTC-USD`
- **Tick Interval**: 1000ms (REST polling frequency)

## API Documentation

- **Advanced Trade API**: https://docs.cdp.coinbase.com/advanced-trade/docs/welcome
- **INTX API (International)**: https://docs.cdp.coinbase.com/intx/docs/welcome
- **CCXT REST reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/coinbase.ts
- **CCXT WebSocket reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/coinbase.ts

## File Structure

| File                       | Description                                                                                                                                                             |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `coinbase.exchange.ts`     | Main exchange class (`CoinbaseExchange`) extending `BaseExchange`. Handles portfolio detection, balance/ticker/position fetching, order placement, and OHLCV retrieval. |
| `coinbasespot.exchange.ts` | Spot variant (`CoinbaseSpotExchange`). Trivial subclass that just sets `supportSpot = true` (same as parent default). Currently disabled in metadata.                   |
| `coinbase.api.ts`          | Axios-based REST client factory. Configures JWT auth interceptor, retry logic (3 retries on network errors), proxy/tunnel support, and public endpoint bypass.          |
| `coinbase.types.ts`        | All TypeScript types: REST response shapes, WebSocket event types, endpoint constants, order configuration types, and lookup maps for order side/type/TIF.              |
| `coinbase.utils.ts`        | JWT signing via Web Crypto API (ES256/ECDSA P-256), PEM-to-PKCS8 conversion, order state/type formatting helpers.                                                       |
| `coinbase.ws-public.ts`    | Public WebSocket: handles `candles`, `market_trades`, `l2_data`, and `heartbeats` channels. Manages order book snapshots/updates and OHLCV streaming.                   |
| `coinbase.ws-private.ts`   | Private WebSocket: handles `user` channel for real-time order updates (snapshot + incremental) and `heartbeats`. Emits fill events on order completion.                 |
| `coinbase.products.json`   | Sample product data for reference/testing.                                                                                                                              |

## Authentication

Coinbase uses **JWT-based authentication** with ES256 (ECDSA P-256) signatures:

- **API Key** (`options.key`): Used as the JWT `kid` (Key ID) and `sub` (Subject) claims
- **API Secret** (`options.secret`): An EC private key in PEM format (SEC1 or PKCS8)
- The `sign()` function in `coinbase.utils.ts`:
  1. Constructs a JWT header with `alg: "ES256"`, `typ: "JWT"`, `kid`, and a random `nonce` (UUID)
  2. Constructs a payload with `iss: "coinbase-cloud"`, `nbf` (current time), `exp` (120 seconds from now), `sub`, and optionally `uri` (for REST requests)
  3. Signs with ECDSA P-256 + SHA-256 via `crypto.subtle`
  4. The `CryptoKey` is cached after first import to avoid repeated PEM parsing (`cachedCryptoKey`/`cachedKeySecret` module-level variables in `coinbase.utils.ts:7-8`)
- **PEM conversion** (`coinbase.utils.ts:47-75`): Coinbase provides SEC1 format keys (`-----BEGIN EC PRIVATE KEY-----`) but Web Crypto API requires PKCS8. The `pemToPkcs8Der()` function detects the format and wraps SEC1 keys in a PKCS8 `PrivateKeyInfo` ASN.1 structure with P-256 OIDs. PKCS8 keys (`-----BEGIN PRIVATE KEY-----`) are passed through as-is.
- **Escaped newlines**: The secret has `\\n` replaced with actual `\n` before parsing (`coinbase.utils.ts:92`)
- **Time source**: Uses `virtualClock.getCurrentTimeMs()` for JWT timestamps, not `Date.now()`
- **REST requests**: The `uri` parameter is `"METHOD api.coinbase.com/path"` (e.g., `"GET api.coinbase.com/api/v3/brokerage/products"`)
- **WebSocket auth**: JWT is generated without a `uri` parameter and sent in the subscribe message payload
- **Header format**: `Authorization: Bearer <jwt>` (when using tealstreetTunnel) or `Coinbase-Authorization: Bearer <jwt>` (direct connection) (`coinbase.api.ts:67-69`)

## Startup Sequence

### `start()` (`coinbase.exchange.ts:106-127`)

1. **Portfolio detection** (sequential, blocking): `findSelectedPortfolio()` is called first. This determines the account type (INTX/CFM/spot) which governs all subsequent data fetching.
2. **Market loading** (sequential, blocking): `fetchMarkets()` loads product data. Early-returns if disposed after this call.
3. **Store update**: Markets are stored and `loaded.markets` is set to `true`.
4. **Public WebSocket**: `publicWebsocket.connectAndSubscribe()` is called (non-blocking, fires and forgets).
5. **Branching**:
   - **Private mode** (`!publicDataOnly`): calls `_startPrivate()`
   - **Public-only mode**: calls `_startPublicDataOnly()`

### `_startPrivate()` (`coinbase.exchange.ts:129-157`)

1. **Private WebSocket**: `privateWebsocket.connectAndSubscribe()` (non-blocking).
2. **Re-detect portfolio**: `findSelectedPortfolio()` is called again (refreshes portfolio data after WebSocket connects).
3. **First tick**: `tick()` is called, which sequentially fetches tickers, balance, and positions.
4. **Note**: Order fetching via REST is intentionally **commented out** (lines 148-156). Orders are loaded exclusively from the WebSocket `user` channel snapshot.

### `_startPublicDataOnly()` (`coinbase.exchange.ts:159-176`)

Sets empty orders, zero balance, empty positions, and marks all as loaded.

### `tick()` loop (`coinbase.exchange.ts:178-221`)

Runs on a 1-second interval via `loop()`. Each tick:

1. **Fetches tickers** (REST poll) -- dispatches `poll_book.{symbol}` and `poll_kline.{symbol}` callbacks for each ticker to update OB/kline subscribers.
2. **Fetches balance** (REST poll).
3. **Fetches positions** (REST poll).
4. Schedules the next `tick()` via `loop(fn, tickInterval)`.

Each fetch is sequential within a tick. Errors are caught and logged but do not stop the loop.

### `findSelectedPortfolio()` (`coinbase.exchange.ts:742-777`)

1. Fetches all portfolios via `LIST_PORTFOLIOS`.
2. Attempts `PORTFOLIO_BREAKDOWN` for each portfolio in parallel (`Promise.all`). The first portfolio that succeeds (no auth error) is selected.
3. If no portfolio has API permissions, throws `'No portfolio with permissions found'`.
4. **CFM detection hack** (line 766-775): If the selected portfolio is not INTX, tries to call `BALANCE_CFM`. If that succeeds (returns `balance_summary`), the portfolio type is overridden to `'CFM'`.

## REST API Endpoints

All endpoints are defined in `ENDPOINTS` constant in `coinbase.types.ts`:

| Endpoint              | Path                                                         | Description                                                                                                 |
| --------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `MARKETS`             | `/api/v3/brokerage/products`                                 | List products (spot + futures). Supports `product_type=FUTURE` and `contract_expiry_type=PERPETUAL` params. |
| `LIST_PORTFOLIOS`     | `/api/v3/brokerage/portfolios`                               | List all portfolios for the account                                                                         |
| `PORTFOLIO_BREAKDOWN` | `/api/v3/brokerage/portfolios/{portfolio_uuid}`              | Portfolio breakdown with spot positions, perp positions, futures positions                                  |
| `BALANCE`             | `/api/v3/brokerage/intx/portfolio/{portfolio_uuid}`          | INTX portfolio balance (total_balance, buying_power)                                                        |
| `BALANCE_CFM`         | `/api/v3/brokerage/cfm/balance_summary`                      | CFM futures balance summary                                                                                 |
| `LIST_POSITIONS_INTX` | `/api/v3/brokerage/intx/positions/{portfolio_uuid}`          | INTX perpetual positions                                                                                    |
| `LIST_POSITIONS_CFM`  | `/api/v3/brokerage/cfm/positions`                            | CFM futures positions                                                                                       |
| `KLINE`               | `/api/v3/brokerage/products/{product_id}/candles`            | Historical candlestick data                                                                                 |
| `ORDER`               | `/api/v3/brokerage/orders`                                   | Place orders (POST)                                                                                         |
| `CANCEL_ORDER_BATCH`  | `/api/v3/brokerage/orders/batch_cancel`                      | Batch cancel orders (POST, max 100 per batch)                                                               |
| `INTX_INSTRUMENTS`    | `https://api.international.coinbase.com/api/v1/instruments/` | INTX instruments with live quotes (external URL, treated as public endpoint)                                |
| `LIST_ACCOUNTS`       | `/api/v3/brokerage/accounts`                                 | List accounts (type defined but only used for `getAccount`)                                                 |
| `ORDER_BOOK`          | `/api/v3/brokerage/product_book`                             | REST order book (type defined but order book is fetched via WebSocket)                                      |

### Public vs Private Endpoints

Only `INTX_INSTRUMENTS` is treated as a public endpoint (no JWT auth). All other endpoints require JWT authentication. The `PUBLIC_ENDPOINTS` array in `coinbase.types.ts:64-72` contains commented-out entries for MARKETS, tickers, klines, and orderbook -- these were once considered public but now require auth.

### Base URLs

- **REST**: `https://api.coinbase.com` (both livenet and testnet -- no testnet support)
- **INTX instruments**: `https://api.international.coinbase.com` (direct external URL)

### Proxy Handling for External URLs (`coinbase.api.ts:43-46`)

When a request URL starts with `http` (i.e., the INTX instruments endpoint which is a full URL rather than a relative path), the private proxy URL is prepended if enabled: `${privateProxyUrl}/${config.url}`.

## WebSocket Implementation

### Public WebSocket (`coinbase.ws-public.ts`)

**URL**: `wss://advanced-trade-ws.coinbase.com` (same for livenet/testnet)

| Channel         | Handler                         | Description                                                                                                                                     |
| --------------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `heartbeats`    | `handlePongEvent()`             | Connection keepalive. No manual ping needed.                                                                                                    |
| `candles`       | `handleOhlcvStreamEvents()`     | Real-time candlestick updates per product. Subscribed on-demand when `listenOHLCV()` is called.                                                 |
| `market_trades` | `handleTradeStreamEvents()`     | Real-time trade events. Used as secondary data source to patch candles, especially for intervals < 5 minutes where candle updates are too slow. |
| `l2_data`       | `handleOrderBookStreamEvents()` | Level 2 order book with snapshot + incremental updates. Subscribed on-demand when `listenOrderBook()` is called.                                |

**Default subscriptions on connect**: Only `heartbeats` is subscribed automatically. `candles`, `market_trades`, and `l2_data` are subscribed on-demand.

**Subscription format**:

```json
{
  "type": "subscribe",
  "product_ids": ["BTC-USD"],
  "channel": "candles"
}
```

**Unsubscription**: Same format with `"type": "unsubscribe"`.

**No authentication on public WebSocket**: Unlike the private WebSocket, public subscribe messages do not include a `jwt` field.

**Reconnection logic**: Inherited from `BaseWebSocket`. On reconnection:

- `resetOrderbook()` is called in `onOpen()` to clear stale order book state (`coinbase.ws-public.ts:46`)
- All topics in `this.topics` are re-subscribed via `subscribe()` (`coinbase.ws-public.ts:47`)
- If `ENABLE_RECONNECTING_WEBSOCKET` is true, emits `wsReconnected` event (`coinbase.ws-public.ts:48-50`)

**Ping/pong**: The `ping()` method is a **no-op** (`coinbase.ws-public.ts:460`). Coinbase WebSockets rely on server-sent heartbeats. The `BaseWebSocket.trackLastReceived()` is called on every message. If no message is received for 20 seconds (`WS_RECONNECT_THRESHOLD_MS`), the base class triggers reconnection.

**Error handling**: If a WebSocket message has `type: "error"`, it is emitted via `this.parent.emitter.emit('error', json.message)` and processing stops (`coinbase.ws-public.ts:73-76`).

**On-demand subscription pattern** (`listenOHLCV`, `listenOrderBook`):

1. Uses `waitForConnectedAndSubscribe()` which polls `this.isConnected` every 100ms until ready
2. Only sends a subscribe message if no existing handler exists for that topic (avoids duplicate subscriptions)
3. Returns an unsubscribe function that removes handlers, decrements topic count, and sends unsubscribe if no handlers remain

### Order Book Specifics

- Snapshot (`type: 'snapshot'`): Clears bids/asks and rebuilds from scratch (`coinbase.ws-public.ts:383-397`)
- Updates (`type: 'update'`): Only applied after snapshot is loaded (tracked via `orderBookInnerState[innerId].isSnapshotLoaded`)
- Side mapping: `'offer'` maps to `asks`, `'bid'` maps to `bids`
- After each update: sorts order book and recalculates totals via `sortOrderBook()` / `calcOrderBookTotal()`
- Each `listenOrderBook` call gets a unique `innerId` via `nanoid()` for independent snapshot tracking

### OHLCV Streaming Details

For each `listenOHLCV` call, **two channels** are subscribed:

1. **Primary**: `candles.{symbol}` -- receives candle updates from Coinbase
2. **Secondary**: `market_trades.{symbol}` -- receives individual trades

**Interval-dependent behavior** (`coinbase.ws-public.ts:201-208`):

- For intervals >= 5 minutes: Uses candle channel data, ignores trade channel for candle construction
- For intervals < 5 minutes (`relyOnTrades = true`): Ignores candle channel data, builds candles from individual trades instead (because Coinbase candle updates are too infrequent at sub-5min intervals)

**Trade-based candle construction** (`coinbase.ws-public.ts:241-283`):

- Constructs OHLCV from individual trades using `timestamp - (timestamp % intervalInSeconds)` for candle alignment
- Detects new candle periods and resets OHLC to the trade price
- Volume is calculated as `size / price` (converted from quote to base volume)

**Daily/weekly/monthly candles** (`coinbase.ws-public.ts:216-237`):

- TradingView only interpolates sub-daily candles, so the code manually accumulates daily+ data
- If `dailyCandles[symbol]` is not populated, triggers an async fetch of the last 2 daily candles
- Accumulates high/low/close/volume from incoming candle data into the stored daily candle

### Private WebSocket (`coinbase.ws-private.ts`)

**URL**:

- **Direct**: `wss://advanced-trade-ws.coinbase.com`
- **Via proxy**: `wss://fsjnpy143pbz.asia.tealstreet.io/coinbase/private` (when `tealstreetProxy` is enabled)

| Channel      | Handler             | Description                                                                                     |
| ------------ | ------------------- | ----------------------------------------------------------------------------------------------- |
| `user`       | `handleUserEvent()` | Order snapshots and updates. Provides real-time order fills, cancellations, and status changes. |
| `heartbeats` | `handlePongEvent()` | Connection keepalive                                                                            |

**Private subscription includes JWT in the message** (`coinbase.ws-private.ts:115-126`):

```json
{
  "type": "subscribe",
  "channel": "user",
  "jwt": "<signed-jwt-without-uri>"
}
```

Both `user` and `heartbeats` channels are subscribed sequentially in a loop, each with a fresh JWT.

**Reconnection behavior** (`coinbase.ws-private.ts:26-31`):

- On open, calls `subscribe()` to re-authenticate
- If `ENABLE_RECONNECTING_WEBSOCKET` is true, calls `this.parent.fetchAndSetPrivateSnapshot()` (which is a no-op for Coinbase)

**Grafana logging**: If `ENABLE_PWS_LOG` is true (default), every private WebSocket message is emitted as `'grafana-pws'` event (`coinbase.ws-private.ts:37-39`)

**Order snapshot authority**: An upstream `orders: []` snapshot canonically clears the store and marks orders loaded. A non-empty snapshot whose every order is quarantined for missing or invalid market/CFM metadata is not authoritative: it preserves last-known-good orders and their loaded state. Mixed snapshots replace the store with valid siblings and emit an error for each skipped record.

## Order Types and Mappings

### Supported Order Configurations

| Order Type  | Configuration Key           | Details                                                                                              |
| ----------- | --------------------------- | ---------------------------------------------------------------------------------------------------- |
| Market      | `market_market_ioc`         | IOC market order with `base_size`                                                                    |
| Limit (GTC) | `limit_limit_gtc`           | Limit order with `base_size`, `limit_price`, optional `post_only`                                    |
| Stop Loss   | `stop_limit_stop_limit_gtc` | Stop-limit GTC with `base_size`, `limit_price` (1% offset from stop), `stop_price`, `stop_direction` |

### Order Type Mapping (from WebSocket events)

| Coinbase Type  | Internal Type        |
| -------------- | -------------------- |
| `"Market"`     | `OrderType.Market`   |
| `"Limit"`      | `OrderType.Limit`    |
| `"Stop Limit"` | `OrderType.StopLoss` |

Unknown order types default to `OrderType.Limit` with a `console.error` (`coinbase.utils.ts:153-156`).

### Order Side Mapping

| Coinbase Side | Internal Side    |
| ------------- | ---------------- |
| `"BUY"`       | `OrderSide.Buy`  |
| `"SELL"`      | `OrderSide.Sell` |

### Order Status Mapping

| Coinbase Status | Internal Status        |
| --------------- | ---------------------- |
| `PENDING`       | `OrderStatus.Open`     |
| `OPEN`          | `OrderStatus.Open`     |
| `FILLED`        | `OrderStatus.Closed`   |
| `CANCELLED`     | `OrderStatus.Canceled` |
| `EXPIRED`       | `OrderStatus.Canceled` |
| `FAILED`        | `OrderStatus.Canceled` |

Unknown statuses default to `OrderStatus.Open` with a `console.error` (`coinbase.utils.ts:139-142`).

## Order Lifecycle

### Order Placement (`coinbase.exchange.ts:1037-1054`)

1. `placeOrder()` calls `formatCreateOrder()` to build the payload.
2. Sends POST to `ENDPOINTS.ORDER`.
3. Checks `data.success`:
   - If `false` and `preview_failure_reason` exists, throws that reason.
   - If `false` without preview reason, throws `failure_reason` or `'Unknown error'`.
4. Returns `[data.order_id]` on success.

### Order Format (`coinbase.exchange.ts:1056-1243`)

- **Client order ID**: `"tealstreet-" + uuid()` (`coinbase.exchange.ts:1177`)
- **Leverage**: Always `"10"` (hardcoded, line 1181)
- **Market orders**: Uses `market_market_ioc` with `base_size` only
- **Limit orders**: Uses `limit_limit_gtc` with `base_size`, `limit_price`, and `post_only` (when `timeInForce === OrderTimeInForce.PostOnly`)
- **Stop loss orders**: Uses `stop_limit_stop_limit_gtc`:
  - `stop_direction`: `STOP_DIRECTION_STOP_UP` for buy-side stops, `STOP_DIRECTION_STOP_DOWN` otherwise
  - `limit_price` is calculated as 1% offset from stop price: `stopPrice * 1.01` (buy stops) or `stopPrice * 0.99` (sell stops)
  - Both `limit_price` and `stop_price` are rounded to the market's price precision

### Order Update (Cancel + Replace) (`coinbase.exchange.ts:1020-1035`)

There is no native amend/edit order API used. Updates are performed as:

1. Cancel the existing order via `cancelOrders([order])`
2. Place a new order with updated price/amount via `placeOrder()`

### Batch Cancel (`coinbase.exchange.ts:999-1018`)

- Order IDs are chunked into batches of 100 via `lodash/chunk`
- Batches are processed **sequentially** via `forEachSeries`
- After each batch cancel, `store.updateOrdersStatus()` is called to mark those orders as canceled locally

### Order State from WebSocket (`coinbase.ws-private.ts:64-111`)

**Snapshot** (`type === 'snapshot'`): Completely replaces the store's orders and marks `loaded.orders = true`.

**Update** (`type === 'update'`): Uses `store.addOrUpdateOrders()` to merge. For filled orders, emits a `'fill'` event with:

- `price`: from `avg_price` or `'market'` if absent
- `amount`: from `order.filled` (cumulative quantity)

**Order price resolution** (`coinbase.ws-private.ts:79`): Uses `stop_price` if present, otherwise `limit_price`, otherwise `'0'`. This means stop-limit orders show their stop price, not their limit price.

**Contract size handling** (`normalizeCfmContractSize`): Expiring CFM products require a positive finite contract-size multiplier; invalid market metadata is quarantined during market loading. CFM orders and positions resolve by product ID and use that same validator before mapping `amount`, `filled`, `remaining`, or contracts to base units. An unresolved market or invalid CFM multiplier is skipped with a non-secret exchange error rather than guessed as `1`; valid `1` remains a valid multiplier. INTX and spot order quantities stay in their API-provided base units.

### Not Yet Implemented

- TakeProfit orders (commented out in `formatCreateOrder`, line 1057-1059, 1152-1153)
- TrailingStopLoss orders (commented out)
- `trigger_bracket_gtc` / `trigger_bracket_gtd` (types defined but not used)
- `limit_limit_gtd` (types defined but not used)
- `sor_limit_ioc` (types defined but not used)
- `fetchHistoricalTrades` (returns empty array, line 930-932)
- `fetchAndSetPrivateSnapshot` (empty method, line 1245)

## Symbol Format

- **Spot**: `BTC-USDC`, `ETH-USD` (base-quote with hyphen)
- **CFM Futures**: Product IDs as returned by the API (e.g., `BIT-28AUG24-CDE` for dated futures)
- **INTX Perpetuals**: `BTC-PERP-INTX` format (the exchange appends `-INTX` to the INTX instrument symbol in `fetchTickersIntx`, line 675)
- **USDC/USD quirk**: The WebSocket public channels substitute `USDC` with `USD` in symbol lookups (`symbol.replace('USDC', 'USD')`) because "coinbase USDC book is fake and is really USD". This applies in `listenOHLCV` (line 186), candle handler (line 145), and `listenOrderBook` (line 367).
- **Default quote**: `USD`
- **Symbol conversion**: When converting from other exchanges, a hyphen is inserted before `USD` if not present (e.g., `BTCUSD` becomes `BTC-USD`)

## Data Precision & Conversion

### Price and Amount Rounding

- **Price precision**: Derived from `price_increment` in the product data (e.g., `"0.01"`). Stored as `market.precision.price` as a float.
- **Amount precision**: Derived from `base_increment` (e.g., `"0.001"`). Stored as `market.precision.amount` as a float.
- **Rounding function**: Uses `adjust(value, precision)` from `../../utils/safe-math` which rounds to the nearest step of the given precision (not decimal places).
- Both price and amount are adjusted before order placement (`coinbase.exchange.ts:1072-1076`).
- **Stop loss limit price**: `adjust(stopPrice * (1.01 or 0.99), pPrice)` -- applies price precision after the 1% offset calculation.

### String-to-Number Conversions

All Coinbase API responses return numeric values as strings. The code uses `parseFloat()` throughout for conversion. Notable patterns:

- **Empty string handling**: For tickers, empty strings for price/volume/percentage default to `0` (`coinbase.exchange.ts:614-621`)
- **Funding interval**: Parsed from microseconds to seconds: `parseFloat(marketIntx.funding_interval || '0') / 1000 / 1000` (`coinbase.exchange.ts:677`)

### Position Size Calculations

- **CFM**: `contracts = multiply(rawSize, contractSize)` -- size is in number of contracts, multiplied by a validated positive finite contract size for base amount. Invalid or unresolved products are skipped rather than emitted in a guessed unit. Notional = `size * markPrice * contractSize`.
- **INTX**: `contracts = abs(net_size)` -- already in base units. Notional = `size * markPrice`. UPNL is manually calculated: `(side === Long ? 1 : -1) * size * (markPrice - entryPrice)` rather than using the API's `unrealized_pnl`.

### Candle Timestamps

- REST API returns `start` as a Unix timestamp string (seconds). Stored as-is (integer seconds).
- WebSocket candle events also use `start` as seconds, parsed with `parseInt()`.
- Trade timestamps are ISO 8601 strings parsed via `dayjs(rawTime).unix()`.

## Rate Limiting

- **maxRPS**: 3 requests/second (applied via `axios-rate-limit` on the primary `xhr` instance, `coinbase.exchange.ts:75`)
- **unlimitedXHR**: A separate Axios instance without rate limiting (`coinbase.exchange.ts:76`), available for batch operations (currently not used in active code paths)
- **Request timeout**: 20 seconds, set in the request interceptor for both public and private endpoints (`coinbase.api.ts:57,73`), overriding the default `RECV_WINDOW` of 20 seconds
- **Network retry**: 3 retries on network errors via `axios-retry` with `isNetworkError` condition (`coinbase.api.ts:32`)
- **Tick loop pacing**: The `tick()` loop sleeps for `tickInterval` (1000ms) between iterations via `loop()`, which calls `sleep(minInterval)` before scheduling the next call

### Effective Request Pattern per Tick

Each tick makes (at minimum):

1. `MARKETS` or `INTX_INSTRUMENTS` (for tickers, 1-2 calls depending on supported types)
2. `BALANCE` or `BALANCE_CFM` (1 call)
3. `LIST_POSITIONS_INTX` or `LIST_POSITIONS_CFM` (1 call for futures types)

With 3 RPS limit, a single tick cycle takes approximately 1-2 seconds of API time.

## State Reconciliation

### Orders

- **Initial load**: Orders come exclusively from the WebSocket `user` channel snapshot (REST `fetchOrders` is commented out).
- **Snapshot handling** (`coinbase.ws-private.ts:91-95`): On a `snapshot` event, the entire order store is **replaced** (not merged). This means any locally-optimistic order state is overwritten.
- **Update handling** (`coinbase.ws-private.ts:96-109`): On an `update` event, `store.addOrUpdateOrders(orders)` is called, which merges by order ID.
- **Local cancel optimism** (`coinbase.exchange.ts:1010-1013`): After sending a batch cancel REST request, `store.updateOrdersStatus()` immediately marks orders as canceled locally, before the WebSocket confirms it.
- **No deduplication**: The WebSocket handler does not deduplicate orders. If the same order appears in both snapshot and update, `addOrUpdateOrders` handles the merge by ID.

### Positions

- **Fetched via REST only**: Positions are loaded every tick via `fetchPositions()`. There is no WebSocket position channel (the `handlePositionTopic` method in the private WS is an empty stub, `coinbase.ws-private.ts:113`).
- **Fake empty positions**: For every market without an open position, a zero-size `Position` object is created with default values (leverage=10, side=Long, entryPrice=0). This is required by the `hasEmptyPositions` metadata flag.

### Tickers

- **REST polling only**: Tickers are refreshed every tick. There is no active WebSocket ticker subscription (the `ticker_batch` handler is commented out in `coinbase.ws-public.ts:25`).
- **Ticker-driven callbacks**: After tickers are fetched, the code iterates over them and triggers `poll_book.{symbol}` and `poll_kline.{symbol}` callbacks for order book and candle subscribers (`coinbase.exchange.ts:192-199`). This provides a ticker-as-proxy update path for subscribers.

### Balance

- **INTX UPNL from positions**: For INTX portfolios, `upnl` is calculated as the sum of all position-level `unrealizedPnl` from the store, **not** from the balance API response (`coinbase.exchange.ts:500-502`). The comment explains: "need to get upnl from positions because the balance version is rebalanced every few seconds and wrong."
- **CFM UPNL**: Hardcoded to `0` (`coinbase.exchange.ts:476`). The CFM balance response has `unrealized_pnl` available but it is not used.

## Exchange Metadata Configuration

Key metadata settings (from `exchangeMetadata.ts`):

| Setting                       | Value                                                                            |
| ----------------------------- | -------------------------------------------------------------------------------- |
| `enabled`                     | `true` (coinbase), `false` (coinbasespot)                                        |
| `supportsTestnet`             | `false`                                                                          |
| `requiresCors`                | `true`                                                                           |
| `requiresPassphrase`          | `false`                                                                          |
| `defaultQuote`                | `"USD"`                                                                          |
| `defaultSymbol`               | `"BTC-USD"`                                                                      |
| `tickInterval`                | `1000` ms                                                                        |
| `orderbookTickDelay`          | `60`                                                                             |
| `orderPendingUpdateDelay`     | `1000` ms                                                                        |
| `stopOrderPendingUpdateDelay` | `1000` ms                                                                        |
| `positionModes`               | `[PositionMode.OneWay]`                                                          |
| `marginTypes`                 | `[MarginType.Cross]`                                                             |
| `leverageSliderDisabled`      | `true`                                                                           |
| `hasEmptyPositions`           | `true` (fake empty positions are created for all markets without open positions) |
| `supportsChaser`              | `false`                                                                          |
| `supportsPartialStops`        | `true`                                                                           |
| `supportsInfinityStops`       | `false`                                                                          |
| `supportsTradesHistory`       | `false`                                                                          |
| `tvIntraDayMultipliers`       | `['1', '5', '15', '30', '60', '120', '360']`                                     |
| `tvMaxGetBarFetches`          | `20`                                                                             |
| `latencyDomain`               | `api.international.coinbase.com`                                                 |

## Testnet / Demo

Coinbase has **no testnet support** (`supportsTestnet: false`). Both livenet and testnet URLs in `BASE_URL` and `BASE_WS_URL` point to the same production endpoints (`coinbase.types.ts:4-18`):

- REST: `https://api.coinbase.com`
- WebSocket: `wss://advanced-trade-ws.coinbase.com`

The testnet option is accepted but has no effect.

## Known Quirks / Implementation Notes

1. **INTX API errors with valid data** (`coinbase.exchange.ts:652-662`): The INTX instruments endpoint sometimes returns HTTP errors but still includes valid data in the response body. The code explicitly catches this: if `err.response?.data?.length` is truthy, it uses the error response data instead of propagating the error.

2. **Fake empty positions** (`coinbase.exchange.ts:781-797`): For all markets that don't have an open position, the exchange creates fake zero-size Position objects with `leverage: 10`, `side: PositionSide.Long`, `entryPrice: 0`. This is tracked by the `hasEmptyPositions` metadata flag.

3. **UPNL from positions not balance** (`coinbase.exchange.ts:500-502`): For INTX, unrealized PnL is calculated from the sum of position-level unrealized PnL rather than from the balance API, because "the balance version is rebalanced every few seconds and wrong."

4. **No testnet**: Both livenet and testnet URLs point to the same production endpoints. `supportsTestnet` is `false`.

5. **Leverage hardcoded** (`coinbase.exchange.ts:1181`): All orders are sent with `leverage: "10"` regardless of user settings. The leverage slider is disabled in the UI. There is a TODO comment: `// TODO. leverage setting?`

6. **USDC/USD substitution**: WebSocket handlers replace `USDC` with `USD` in symbol lookups for order book and candle subscriptions, as Coinbase treats the USDC book as equivalent to USD.

7. **Portfolio detection hack** (`coinbase.exchange.ts:766-775`): When a non-INTX portfolio is found, the code attempts to call the CFM balance endpoint to detect if the account supports CFM futures, and overrides the portfolio type to `'CFM'` if successful.

8. **No manual ping**: Unlike most exchanges, Coinbase WebSockets use server-sent heartbeats only. Both `CoinbasePublicWebsocket.ping()` and `CoinbasePrivateWebsocket.ping()` are intentionally empty no-ops.

9. **Tickers via REST polling**: Tickers are fetched via REST polling in the `tick()` loop (every `tickInterval` = 1 second) rather than through WebSocket ticker streams. The `ticker_batch` handler exists in commented-out code but is not active.

10. **Orders via WebSocket only**: Orders are loaded exclusively through the private WebSocket `user` channel snapshot, not via a REST endpoint. The `fetchOrders()` call in `_startPrivate` is commented out.

11. **X-MBX-APIKEY header** (`coinbase.api.ts:23`): The API client sets an `X-MBX-APIKEY` header (a Binance convention) which is a leftover from when the implementation was based on Binance. It has no effect on Coinbase.

12. **Daily candle formatting** (`coinbase.ws-public.ts:216-237`): For daily/weekly/monthly intervals, the code manually accumulates candle data because TradingView only interpolates sub-daily candles. The implementation fetches the last 2 daily candles on first request to initialize the accumulator. There is a TODO: `// TODO handle new daily candles during session`

13. **Commented-out Binance endpoints** (`coinbase.types.ts:36-48`): The ENDPOINTS object contains commented-out Binance endpoint paths (LEVERAGE_BRACKET, TICKERS_24H, etc.) with a TODO to remove them.

14. **Trade tape gaps** (`coinbase.ws-public.ts:167-169`): Only the first trade in each `market_trades` event is used for candle patching. There is a TODO: `// TODO: if you want actual trade tape without holes fix this...`

15. **Duplicate portfolio detection call** (`coinbase.exchange.ts:108,137`): `findSelectedPortfolio()` is called twice during startup -- once at the beginning of `start()` and once at the beginning of `_startPrivate()`. This results in redundant API calls.

16. **`validateAccount` returns undefined on success** (`coinbase.exchange.ts:96-103`): The success path has `return '';` commented out, so the method implicitly returns `undefined` on success and an error string on failure.

17. **market_trades secondary channel only when not publicDataOnly** (`coinbase.ws-public.ts:300`): The trade stream for candle patching is only subscribed when `!this.parent.options.publicDataOnly`, meaning public-only mode has degraded sub-5min candle quality.

## Error Handling

### REST Error Pattern

All REST methods follow a consistent try/catch pattern:

```typescript
try {
  // API call
} catch (err: any) {
  this.emitAndLogError(err?.response?.data?.message || err?.message);
  return this.store.<previousState>; // fallback to existing store state
}
```

This means REST failures are **non-fatal**: the tick loop continues, and the store retains its previous state.

### Place Order Error Handling (`coinbase.exchange.ts:1040-1053`)

Order placement has a two-level error check:

1. **API success check**: If `data.success === false`, checks `error_response.preview_failure_reason` first, then falls back to `failure_reason` or `'Unknown error'`.
2. **Network/HTTP error**: Caught in the outer try/catch, re-throws after logging.

Both paths throw errors (unlike other REST methods that silently return fallback state), ensuring order failures are propagated to the caller.

### INTX Instruments Error-as-Success (`coinbase.exchange.ts:653-661`)

The INTX instruments endpoint uniquely handles errors differently:

```typescript
try {
  const { data: _data } = await this.xhr.get(...);
  data = _data;
} catch (err: any) {
  if (err.response?.data?.length) {
    data = err.response.data;  // use error response body as valid data
  } else {
    throw err;
  }
}
```

### WebSocket Error Handling

- **Public WS** (`coinbase.ws-public.ts:73-76`): Messages with `type: "error"` emit an error event and skip further processing.
- **Unknown channels** (`coinbase.ws-public.ts:78-82`): Messages for unregistered channels log a `console.error` but do not crash.
- **Private WS**: No explicit error channel handling. Unknown channels are silently ignored (only `user` and `heartbeats` are processed).

### Reconnection (inherited from `BaseWebSocket`)

- **Health check**: `trackLastReceived()` is called on every WebSocket message. If no message arrives for `WS_RECONNECT_THRESHOLD_MS` (20 seconds), triggers reconnection.
- **Reconnect delay**: `WS_MIN_RECONNECTION_DELAY` = 1000 + random(0-2000) ms (jittered)
- **Max reconnect attempts**: After 100 `onClose` or `reconnect` calls, logs an error
- **On reconnect**: Public WS resets orderbook and re-subscribes. Private WS re-authenticates with fresh JWTs.

## Testing / Validation

There are no test files specific to the Coinbase integration in this directory. Testing is done through the shared test infrastructure (`yarn test` in the safe-cex package).

Validation endpoints are tracked via Grafana:

- **Every call**: `ORDER`, `CANCEL_ORDER_BATCH`
- **Periodic**: `LIST_PORTFOLIOS`, `LIST_POSITIONS_INTX`, `LIST_POSITIONS_CFM`, `BALANCE`, `BALANCE_CFM`, `PORTFOLIO_BREAKDOWN`

## Portfolio System

Coinbase has a portfolio-based architecture. The exchange detects the portfolio type at startup via `findSelectedPortfolio()`:

1. Fetches all portfolios via `LIST_PORTFOLIOS`
2. Attempts to get breakdown for each portfolio to find one with API permissions
3. If the portfolio is not INTX type, attempts to detect CFM support by calling `BALANCE_CFM`
4. Sets `selectedPortfolio.type` to one of: `'INTX'`, `'CFM'`, or default (spot-capable)

**Portfolio type determines**:

- **INTX**: Perpetual futures trading via International Exchange. Has its own balance, position, and ticker endpoints.
- **CFM**: Coinbase Financial Markets (dated futures). Uses separate CFM balance and position endpoints.
- **Default (non-INTX)**: Spot trading supported. Balance from portfolio breakdown, positions from spot_positions.

The `getSupportedTypes()` method returns `('spot' | 'cfm' | 'intx')[]`:

- INTX and CFM are mutually exclusive (checked at `fetchBalance`, line 251-254)
- Spot is available alongside CFM but not alongside INTX
- If no supported types are found, emits an error

## Exchange Variants

### `coinbase` (Futures)

The primary variant. Registered as `CoinbaseExchange`. Supports:

- INTX perpetual futures
- CFM dated futures
- Spot trading alongside CFM (via `supportSpot = true`)

### `coinbasespot` (Spot Only)

Registered as `CoinbaseSpotExchange`. Inherits everything from `CoinbaseExchange` with `supportSpot = true`. Currently **disabled** in metadata (`enabled: false`, `enabledInDev: false`). The spot variant is effectively redundant since the main `coinbase` variant already has `supportSpot = true`.

Both variants are loaded from the same directory (no separate `coinbasespot/` directory).

## OHLCV / Candlestick Details

### Supported Intervals

| Interval | Coinbase Granularity |
| -------- | -------------------- |
| `1m`     | `ONE_MINUTE`         |
| `5m`     | `FIVE_MINUTE`        |
| `15m`    | `FIFTEEN_MINUTE`     |
| `30m`    | `THIRTY_MINUTE`      |
| `1h`     | `ONE_HOUR`           |
| `2h`     | `TWO_HOUR`           |
| `6h`     | `SIX_HOUR`           |
| `1d`     | `ONE_DAY`            |

- Maximum 300 candles per request (`coinbase.exchange.ts:953`)
- Candles are returned in reverse chronological order from the API and reversed in code (`coinbase.exchange.ts:986`)
- Daily candles are cached in `this.dailyCandles` for use in real-time daily/weekly/monthly candle formatting
- For intervals < 5 minutes, the WebSocket relies on `market_trades` channel instead of `candles` channel because candle updates are too infrequent
- Start/end timestamps are sent as Unix seconds (divided by 1000 from millisecond dayjs values)

## CCXT Reference Implementation

For implementation inspiration and reference, consult the CCXT library's Coinbase integration:

- **REST API Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/coinbase.ts
  - Note: `coinbaseadvanced.ts` is a thin alias that extends `coinbase.ts`
- **WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/coinbase.ts
