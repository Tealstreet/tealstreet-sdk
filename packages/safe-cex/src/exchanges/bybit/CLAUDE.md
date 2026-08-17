# Bybit Exchange Integration

This directory contains the Bybit exchange integration for Tealstreet, covering linear perpetuals, inverse perpetuals, spot trading, and demo trading. All variants use the Bybit V5 Unified API.

## Exchange Information

- **Exchange**: Bybit (CEX)
- **API Version**: V5 Unified API
- **Variants**: Linear (`BybitExchange`), Inverse (`BybitInverseExchange`), Spot (`BybitSpotExchange`), Demo (`BybitDemoExchange`)
- **Broker ID**: `Tealstreet` (sent via `X-Referer` header)
- **Settlement**: USDT and USDC (linear), Coin-settled (inverse), N/A (spot)

## API Documentation

When working with Bybit exchange integration, consult the official API documentation:

- **V5 API Introduction**: https://bybit-exchange.github.io/docs/v5/intro
- **REST API Reference**: https://bybit-exchange.github.io/docs/v5/order/create-order
- **WebSocket Reference**: https://bybit-exchange.github.io/docs/v5/ws/connect
- **Rate Limits**: https://bybit-exchange.github.io/docs/v5/rate-limit
- **Transaction Log**: https://bybit-exchange.github.io/docs/v5/account/transaction-log
- **CCXT REST Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/bybit.ts
- **CCXT WebSocket Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/bybit.ts

## File Structure

### Core (Linear USDT/USDC Perpetual) - `BybitExchange`

| File                    | Description                                                                                                                                                                                                                                                                |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bybit.exchange.ts`     | Main exchange class extending `BaseExchange`. Handles linear perpetual futures (USDT and USDC settled). Contains order placement, position management, balance calculation, and all trading operations.                                                                    |
| `bybit.api.ts`          | REST API client factory. Creates Axios instance with HMAC-SHA256 signing, retry logic, recv_window handling, and virtual clock time synchronization.                                                                                                                       |
| `bybit.order-params.ts` | Validates and serializes shared `maxSlippagePct` plus adapter-only typed BBO and TickSize execution controls.                                                                                                                                                              |
| `bybit.types.ts`        | Type definitions, endpoint constants, type mapping objects (`ORDER_TYPE`, `ORDER_SIDE`, `ORDER_STATUS`, `ORDER_TIME_IN_FORCE`, `ORDER_TRIGGER_TYPE`, `POSITION_SIDE`), WebSocket URL constants, interval mappings, `BybitApiKeyInfo` interface, and transaction log types. |
| `bybit.rateLimits.ts`   | VIP-level rate limit table. Defines `BybitVipLevel` (No VIP through PRO-6), `BybitProductType` (Futures/Option/Spot), and `BybitAccountType` (Classic+UTA / UTA Pro). Used to dynamically adjust batch order limits.                                                       |
| `bybit.ws-public.ts`    | Public WebSocket (`BybitPublicWebsocket`). Handles ticker updates, kline/OHLCV streaming, orderbook snapshot+delta, and public trade data for linear markets.                                                                                                              |
| `bybit.ws-private.ts`   | Private WebSocket (`BybitPrivateWebsocket`). Handles order updates, position updates, authentication, and time sync. Subscribes to `order` and `position` topics.                                                                                                          |

### Demo Trading - `BybitDemoExchange`

| File                    | Description                                                                                                                                                                                                                                        |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bybitdemo.exchange.ts` | Minimal subclass of `BybitExchange` that sets `isDemo = true` and restricts to `['linear']` category. Uses demo-specific REST and private WebSocket URLs (`api-demo.bybit.com`, `stream-demo.bybit.com`); public data stays on the mainnet stream. |

### Inverse Perpetual - `BybitInverseExchange`

| File                         | Description                                                                                                                                                                                                                                                              |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `bybitinverse.exchange.ts`   | Standalone exchange class (extends `BaseExchange` directly, NOT `BybitExchange`). Handles inverse perpetual contracts (USD-quoted, coin-settled like BTCUSD). Contains specialized amount handling (integer USD contracts) and position mapping (USD-to-BTC conversion). |
| `bybitinverse.ws-public.ts`  | Public WebSocket for inverse markets (`BybitInversePublicWebsocket`). Connects to `v5/public/inverse` endpoint. Supports dynamic ticker topic subscription via `activeTickerSymbols:changed` event.                                                                      |
| `bybitinverse.ws-private.ts` | Private WebSocket for inverse markets (`BybitInversePrivateWebsocket`). Identical auth flow to linear private WS. Subscribes to `order` and `position` topics.                                                                                                           |

### Spot Trading - `BybitSpotExchange`

| File                      | Description                                                                                                                                                                                                                                                                                         |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bybitspot.exchange.ts`   | Standalone exchange class for spot trading (extends `BaseExchange` directly). Sets `isSpot = true`. No position tracking (`fetchPositions` returns `[]`). All API calls use `category: 'spot'`. Leverage fixed at min:1, max:1.                                                                     |
| `bybitspot.types.ts`      | Spot-specific type definitions. Contains separate `BASE_WS_URL` with `v5/public/spot` WebSocket endpoint. Shares the same `ENDPOINTS` and type mappings as linear, but lacks `CREATE_BATCH_ORDER`.                                                                                                  |
| `bybitspot.ws-public.ts`  | Public WebSocket for spot markets (`BybitPublicWebsocket` - note: same class name as linear). Connects to `v5/public/spot`. Orderbook depth is 200 levels (vs 1000 for linear/inverse). Supports dynamic ticker topic subscription. Calculates unrealized PnL for spot positions on ticker updates. |
| `bybitspot.ws-private.ts` | Private WebSocket for spot markets. Same structure as linear private WS.                                                                                                                                                                                                                            |

## Authentication

### REST API Signing (`bybit.api.ts`)

Authentication uses HMAC-SHA256 with the following process:

1. Generate a timestamp from `virtualClock.getCurrentTimeMs()`
2. For GET requests: serialize params with `qs.stringify(params)`
3. For POST requests: `JSON.stringify(data)`
4. Compute signature: `HMAC-SHA256(secret, timestamp + apiKey + recvWindow + data)`
5. Set headers:
   - `X-BAPI-SIGN`: the hex-encoded signature
   - `X-BAPI-API-KEY`: the API key
   - `X-BAPI-TIMESTAMP`: the timestamp
   - `X-BAPI-RECV-WINDOW`: configurable, defaults to `20000` ms (via `options.extra.recvWindow` or `RECV_WINDOW` constant)
   - `X-Referer`: `Tealstreet` (broker ID)
   - `Content-Type`: `application/json, charset=utf-8`

Each request gets a unique `requestId` param via `nanoid()`.

The KLINE endpoint is **unsigned** (headers stripped for public market data) -- `bybit.api.ts:63`.

**Retry logic** (`bybit.api.ts:49-52`): Retries up to 3 times on network errors or `"error sign!"` responses. The `isErrorSignature()` function checks both `ret_msg` and `retMsg` response fields.

**Time sync** (`bybit.api.ts:92-105`): On `recv_window` errors (response `retMsg` containing `'recv'`), automatically calls `virtualClock.syncBybitClientServerTime()`. Also logs server unavailability when the raw response includes "too much traffic or a configuration error".

**HTTP-over-WebSocket tunnel** (`bybit.api.ts:87-90`): When `options.tealstreetTunnel` is set, the Axios adapter is replaced with `createAxiosHttpOverWebsocketAdapter` using the configured `proxyRegion`.

### WebSocket Authentication (`bybit.ws-private.ts`)

1. Sync time with `virtualClock.syncBybitClientServerTime()`
2. Wait 25ms (`bybit.ws-private.ts:39`)
3. Compute `expires = currentTimeMs + 10_000` (10-second expiry window)
4. Sign: `HMAC-SHA256(secret, "GET/realtime" + expires)`
5. Send auth: `{ op: 'auth', args: [apiKey, expires, signature] }`
6. On auth failure (`json.success === false && json.op === 'auth'`), retry up to 3 times with time re-sync (`authRetries` counter at `bybit.ws-private.ts:82-86`)

## Startup Sequence

### Linear (`BybitExchange.start()` -- `bybit.exchange.ts:190-259`)

```
1. virtualClock.start()                          -- Initialize time synchronization
2. Reset batchLimit to 10
3. [NON-BLOCKING] fetchAndCacheApiKeyInfo()       -- Fire-and-forget: fetches VIP level, updates batchLimit
4. [PARALLEL] Promise.all([
     fetchMarkets()                               -- Loads markets, then IMMEDIATELY starts public WS
       .then(connectAndSubscribe())               -- Public WS connects as soon as markets are stored
     fetchTickersWithoutParse()                    -- Raw tickers for later parsing
     fetchMarginAccountInfos()                    -- Gets unifiedMarginStatus and marginMode (private only)
   ])
5. parseTickers(rawTickers)                       -- Parse and store tickers
6. [PUBLIC-ONLY EXIT] If publicDataOnly, mark balance/positions/orders as loaded, return
7. privateWebsocket.connectAndSubscribe()         -- Start private WS (auth + subscribe order/position)
8. [PARALLEL] fetchOrdersPositionsAndBalance()
     Promise.all([
       fetchOrders()                              -- Paginated cursor-based fetch per settleCoin
       fetchPositions()                           -- Per settleCoin (USDT, optionally USDC for UNIFIED)
       fetchRawBalance()                          -- Account type-aware balance fetch
     ])
9. tick()                                         -- Start periodic refresh loop
```

### Tick Loop (`bybit.exchange.ts:348-363`)

Each tick:

1. If `needsReinit` is true, full re-fetch of orders + positions + balance (then reset flag)
2. Otherwise, call `fetchAndSetPrivateSnapshotHelper(false)` -- positions + balance only (skips orders)
3. Schedule next tick via `loop()` using `options.extra.tickInterval`

### Inverse and Spot Variants

Follow the same pattern with minor differences:

- **Inverse** (`bybitinverse.exchange.ts:203-265`): Uses `publicInverseWebsocket` instead of `publicWebsocket`. Markets fetched sequentially (not in parallel for PreLaunch). No additional safety check for credentials before private WS.
- **Spot** (`bybitspot.exchange.ts:144-192`): No API key info fetch. `fetchPositions()` returns empty array. No batch order support.

## REST API Endpoints

All endpoints use the V5 API prefix. Defined in `bybit.types.ts`:

| Constant               | Path                          | Method | Description                            |
| ---------------------- | ----------------------------- | ------ | -------------------------------------- |
| `ACCOUNT_MARGIN`       | `/v5/account/info`            | GET    | Account margin mode and unified status |
| `ACCOUNT`              | `/v5/user/query-api`          | GET    | API key info, VIP level, user ID       |
| `BALANCE`              | `/v5/account/wallet-balance`  | GET    | Wallet balance (params: `accountType`) |
| `FEE_RATE`             | `/v5/account/fee-rate`        | GET    | Maker/taker fee rates per symbol       |
| `UNFILLED_ORDERS`      | `/v5/order/realtime`          | GET    | Open orders (paginated with cursor)    |
| `TICKERS`              | `/v5/market/tickers`          | GET    | Market tickers                         |
| `MARKETS`              | `/v5/market/instruments-info` | GET    | Instrument info (markets)              |
| `CANCEL_ORDER`         | `/v5/order/cancel`            | POST   | Cancel single order                    |
| `CANCEL_ORDERS`        | `/v5/order/cancel-batch`      | POST   | Batch cancel (UNIFIED accounts only)   |
| `CANCEL_SYMBOL_ORDERS` | `/v5/order/cancel-all`        | POST   | Cancel all orders for a symbol         |
| `POSITIONS`            | `/v5/position/list`           | GET    | Position list                          |
| `KLINE`                | `/v5/market/kline`            | GET    | OHLCV candle data                      |
| `SET_LEVERAGE`         | `/v5/position/set-leverage`   | POST   | Set buy/sell leverage                  |
| `SET_TRADING_STOP`     | `/v5/position/trading-stop`   | POST   | Set TP/SL/trailing stop on position    |
| `CREATE_ORDER`         | `/v5/order/create`            | POST   | Place single order                     |
| `CREATE_BATCH_ORDER`   | `/v5/order/create-batch`      | POST   | Place batch orders (up to 20)          |
| `REPLACE_ORDER`        | `/v5/order/amend`             | POST   | Amend existing order                   |
| `SET_POSITION_MODE`    | `/v5/position/switch-mode`    | POST   | Switch one-way/hedge mode              |
| `SET_MARGIN_MODE`      | `/v5/account/set-margin-mode` | POST   | Switch account-wide UTA margin mode    |
| `USER_ORDERS`          | `/v5/order/history`           | GET    | Historical orders                      |
| `USER_TRADES`          | `/v5/execution/list`          | GET    | Execution/fill history                 |
| `USER_CLOSED_PNL`      | `/v5/position/closed-pnl`     | GET    | Closed PnL records                     |
| `TRANSACTION_LOG`      | `/v5/account/transaction-log` | GET    | Transaction log (funding, PnL, etc.)   |

### Opening conditional trigger source

Bybit linear and inverse opening conditionals (`OrderType.Conditional`) map
`PlaceOrderOpts.triggerType` directly to V5 `triggerBy`: `LastPrice`,
`MarkPrice`, or `IndexPrice`. The same source price must be used when deriving
`triggerDirection`; otherwise Bybit can accept a conditional order whose trigger
source differs from the price used to decide rise-vs-fall direction.

Do not reuse this mapping for TP/SL without auditing those legacy paths first:
they still use their separate `tpTriggerType` / `slTriggerType` behavior.

### Base URLs

| Environment | REST                            | Public WS (Linear)                                | Public WS (Inverse)                                | Private WS                                  |
| ----------- | ------------------------------- | ------------------------------------------------- | -------------------------------------------------- | ------------------------------------------- |
| **Livenet** | `https://api.bybit.com`         | `wss://stream.bybit.com/v5/public/linear`         | `wss://stream.bybit.com/v5/public/inverse`         | `wss://stream.bybit.com/v5/private`         |
| **Testnet** | `https://api-testnet.bybit.com` | `wss://stream-testnet.bybit.com/v5/public/linear` | `wss://stream-testnet.bybit.com/v5/public/inverse` | `wss://stream-testnet.bybit.com/v5/private` |
| **Demo**    | `https://api-demo.bybit.com`    | `wss://stream.bybit.com/v5/public/linear`         | `wss://stream.bybit.com/v5/public/inverse`         | `wss://stream-demo.bybit.com/v5/private`    |
| **Spot**    | `https://api.bybit.com`         | `wss://stream.bybit.com/v5/public/spot`           | N/A                                                | `wss://stream.bybit.com/v5/private`         |

## WebSocket Implementation

### Public WebSocket Topics

| Topic Pattern               | Description                                                                   | Used By         |
| --------------------------- | ----------------------------------------------------------------------------- | --------------- |
| `tickers.{SYMBOL}`          | Real-time ticker updates (bid, ask, last, mark, index, funding, OI, volume)   | All variants    |
| `kline.{INTERVAL}.{SYMBOL}` | OHLCV candlestick data                                                        | All variants    |
| `orderbook.1000.{SYMBOL}`   | Order book snapshot + delta (1000 levels for linear/inverse)                  | Linear, Inverse |
| `orderbook.200.{SYMBOL}`    | Order book snapshot + delta (200 levels for spot)                             | Spot            |
| `publicTrade.{SYMBOL}`      | Public trade data (used as secondary candle source for sub-minute timeframes) | All variants    |

### Private WebSocket Topics

| Topic      | Description                                                                             |
| ---------- | --------------------------------------------------------------------------------------- |
| `order`    | Order lifecycle events (New, PartiallyFilled, Filled, Cancelled, Deactivated, Rejected) |
| `position` | Position updates (size, entry price, unrealized PnL, leverage)                          |

### Connection Details

- **URL format**: Base WS URL + `?requestId=` + `nanoid()` (unique per connection)
- **Event listeners**: `open`, `message`, `close` registered on each `createWebsocket()` call

### Subscription Rate Limiting

Topics are chunked into groups of **10** with **10ms** delay between chunks to prevent connection closure (`bybit.ws-public.ts:90-94`, `bybitinverse.ws-public.ts:117-121`). OHLCV multi-symbol subscriptions use chunks of **50** (`bybit.ws-public.ts:195-198`).

### Ping/Pong

All WebSockets (public and private) send `{ op: 'ping' }` for keepalive. The `pingAt` timestamp is recorded via `performance.now()` for latency tracking. Pong responses are handled by `handlePongEvent()` from `BaseWebSocket`.

### Reconnection

- **BaseWebSocket** provides the underlying reconnection mechanism via `ENABLE_RECONNECTING_WEBSOCKET` config flag
- On reconnect, `emitWsReconnected()` is called for public WS
- Private WS reconnect triggers `sendReconnectSequence()` which: syncs time, re-authenticates, re-subscribes, and calls `fetchAndSetPrivateSnapshot()` to refresh orders/positions
- All subscribed topics are stored in `this.topics` and re-subscribed on reconnect via `this.subscribe()`

### Snapshot vs Delta Updates

**Orderbook** (`bybit.ws-public.ts:394-431`):

- `type === 'snapshot'`: Clears entire orderbook, rebuilds from data. Fields: `data.a` (asks) and `data.b` (bids) as `[price, amount]` tuples
- `type === 'delta'`: Applies incremental updates via `updateOrderBook()`. Entries with `amount === 0` are removed. After each update, `sortOrderBook()` and `calcOrderBookTotal()` are called

**Kline** (`bybit.ws-public.ts:208-293`):

- Primary parser: Uses kline topic data. Prevents "bleeding candles backwards" by checking timestamps
- Secondary parser: Uses `publicTrade` topic for sub-second candle construction. Calculates candle start time via `getCandleStartTime()`. Supports second-interval (`opts.interval.endsWith('s')`) candle creation from individual trades

### Dynamic Ticker Subscriptions

- **Linear**: Subscribes to ALL market tickers on connect (no dynamic add/remove)
- **Inverse** (`bybitinverse.ws-public.ts:34-75`): Listens for `activeTickerSymbols:changed` events. Computes diff between current and desired topics, subscribes only new ones. Disposes listener in `dispose()` override.
- **Spot** (`bybitspot.ws-public.ts:35-70`): Same dynamic pattern as inverse. Uses `getTickerSymbolsForPublicStream()` to determine desired symbols.

### Message Routing

Messages are routed by string matching on the raw data (`data.includes('topic":"${topic}')`) before parsing JSON. This avoids parsing every message and only deserializes via `jsonParse()` when a handler match is found (`bybit.ws-public.ts:97-112`).

## Order Types and Mappings

### Order Type Mapping (`ORDER_TYPE`)

| Bybit Value         | Internal Type                |
| ------------------- | ---------------------------- |
| `Limit`             | `OrderType.Limit`            |
| `Market`            | `OrderType.Market`           |
| `Stop`              | `OrderType.StopLoss`         |
| `StopLoss`          | `OrderType.StopLoss`         |
| `TakeProfit`        | `OrderType.TakeProfit`       |
| `PartialTakeProfit` | `OrderType.TakeProfit`       |
| `PartialStopLoss`   | `OrderType.StopLoss`         |
| `TrailingStop`      | `OrderType.TrailingStopLoss` |

### Time-in-Force Mapping (`ORDER_TIME_IN_FORCE`)

| Bybit Value | Internal Type       |
| ----------- | ------------------- |
| `GTC`       | `GoodTillCancel`    |
| `IOC`       | `ImmediateOrCancel` |
| `FOK`       | `FillOrKill`        |
| `PostOnly`  | `PostOnly`          |
| `RPI`       | `PostOnly`          |

Bybit RPI orders are maker-only and can be placed by eligible market makers.
Tealstreet does not place RPI orders, but orders created elsewhere map to the
internal `PostOnly` value so their time-in-force is never undefined. Outbound
serialization uses the separate `ORDER_TIME_IN_FORCE_OUTBOUND` map so native
Tealstreet PostOnly orders remain `PostOnly` and are never promoted to RPI.

### Order Status Mapping (`ORDER_STATUS`)

| Bybit Value                                                  | Internal Status |
| ------------------------------------------------------------ | --------------- |
| `Created`, `New`, `Active`, `Untriggered`, `PartiallyFilled` | `Open`          |
| `Rejected`, `Filled`, `Deactivated`, `Triggered`             | `Closed`        |
| `PendingCancel`, `Cancelled`                                 | `Canceled`      |

### Trigger Type Mapping (`ORDER_TRIGGER_TYPE`)

| Bybit Value  | Internal Type |
| ------------ | ------------- |
| `IndexPrice` | `IndexPrice`  |
| `MarkPrice`  | `MarkPrice`   |
| `LastPrice`  | `LastPrice`   |

### OHLCV Interval Mapping (`INTERVAL`)

| Internal | Bybit |
| -------- | ----- |
| `1m`     | `1`   |
| `3m`     | `3`   |
| `5m`     | `5`   |
| `15m`    | `15`  |
| `30m`    | `30`  |
| `1h`     | `60`  |
| `2h`     | `120` |
| `4h`     | `240` |
| `6h`     | `360` |
| `12h`    | `720` |
| `1d`     | `D`   |
| `1w`     | `W`   |

## Order Lifecycle

### Order Placement Flow

1. **Check for position-bound stops** (`bybit.exchange.ts:1029-1049`): If a full-sized position exists on the opposite side and the order is SL/TP, route to `placeStopLossOrTakeProfit()` (which uses `SET_TRADING_STOP`). If trailing stop, route to `placeTrailingStopLoss()`.

2. **Position index resolution** (`getOrderPositionIdx` at `bybit.exchange.ts:1993-2001`): Determines `positionIdx` based on position mode:
   - One-Way: `0`
   - Hedged Buy/Long: `1`, Sell/Short: `2`
   - If `reduceOnly`, the index is flipped (1 <-> 2)

3. **Trigger direction** (`bybit.exchange.ts:1071-1084`): For conditional orders, computes `triggerDirection` (1 = price above current, 2 = price below). Compares trigger price against mark or last depending on the trigger type setting. Opening conditionals use `PlaceOrderOpts.triggerType`, defaulting to Last Price when omitted; TP/SL still use `tpTriggerType` / `slTriggerType`.

4. **Type conversion for conditional stops and opening triggers**: When a SL/TP has a `triggerPrice`, the `orderType` is sent as `Market` (not StopLoss/TakeProfit), because Bybit's conditional market orders use `triggerPrice` + `orderType: Market`. Linear and inverse also map `OrderType.Conditional` opening triggers to Bybit `Market` or `Limit` requests based on whether an execution `price` is present, force `reduceOnly: false`, and keep `closeOnTrigger: false`.

5. **Order size splitting** (`bybit.exchange.ts:1128-1141`): Orders exceeding `market.limits.amount.max` are split into multiple lots. Only the first lot carries TP/SL; subsequent lots have TP/SL stripped to avoid duplicates.

6. **Sequential submission**: Lots are submitted sequentially via `mapSeries`. Each failure is logged but does not abort subsequent lots.

7. **Demo reconciliation**: After initial orders are loaded, successful create /
   batch / amend mutations on `BybitDemoExchange` force a debounced
   order-inclusive REST snapshot. Demo private WS can remain generically healthy
   while missing order topic events, so do not gate this backstop solely on
   `privateWebsocket.isHealthy`.

### Native Market Slippage Protection

All Bybit variants map `PlaceOrderOpts.maxSlippagePct` to V5
`slippageToleranceType: 'Percent'` and `slippageTolerance`. The value must be
within `0.01`–`10` and have no more than two decimal places. It is accepted
only for plain `OrderType.Market` orders; Bybit does not allow these native
fields on conditional orders or market orders carrying attached TP/SL.

Programmatic callers can additionally import `BybitPlaceOrderOpts` (or the
narrower `BybitTickSizeMarketOrderOpts`) and set
`bybitSlippageToleranceTicks`. The tick count must be an integer from 1 through
10000 and is serialized as `slippageToleranceType: 'TickSize'`. It is accepted
only on plain market orders and is mutually exclusive with `maxSlippagePct`.
No default is supplied: omitting both fields sends neither native slippage
parameter. Spot retains its existing Bybit market-quantity unit behavior; this
option changes only execution-price tolerance.

Protected orders always use the single-order endpoint. Bybit's batch-place
documentation does not expose the slippage fields, so `placeOrders` preflights
them and routes each through `placeOrder`.

Do not silently translate `maxSlippagePct` to Bybit's `TickSize` mode: the
shared field is explicitly a percentage and cannot represent a tick count.

### Programmatic BBO Orders

`BybitBboOrderOpts` exposes BBO placement without adding order-ticket UI. Set
`bybitBbo: { sideType: 'Queue' | 'Counterparty', level: 1 | 2 | 3 | 4 | 5 }`.
BBO is available only on linear/inverse adapters (including the linear demo
adapter), omits absolute `price`, and always uses `/v5/order/create` because the
batch schema does not list BBO fields.

Bybit's BBO-specific documentation does not define interactions with order
type, TIF, conditionals, attached TP/SL, or slippage fields. The adapter
therefore deliberately restricts the first typed surface to a plain Limit
order, absent/default `GoodTillCancel`, no absolute price, no trigger/TP/SL,
and no slippage control. Those are adapter safety restrictions, not claims
about undocumented venue behavior. `Queue`/`Counterparty` and level are never
inferred from a regular limit order, and omitting `bybitBbo` sends no BBO
fields.

### Batch Order Placement (`placeOrders` -- `bybit.exchange.ts:1251-1293`)

1. **Preflight**: Explicit Percent/TickSize/BBO controls for the whole list are validated before any I/O, so invalid intent cannot be swallowed by the special-order continue-on-error loop
2. **Separation**: Orders are split into "special" (position-bound SL/TP, trailing stops, native slippage-protected market orders, and BBO orders) and "regular" orders
3. **Special orders**: Placed individually via `placeOrder()`, transport/venue errors are logged and non-fatal
4. **Regular orders**: Grouped by category, chunked by `batchLimit` (10 or 20 based on VIP level)
5. **Batch fallback**: If `CREATE_BATCH_ORDER` fails for a batch, falls back to individual `CREATE_ORDER` per order in that batch (`bybit.exchange.ts:1336-1346`)
6. **Inter-batch delay**: 1200ms sleep between batches (`bybit.exchange.ts:1328-1329`)
7. **Error handling**: Individual order errors within a batch are reported via `retExtInfo.list[].code` and logged, but partial successes are preserved

### Virtual TP/SL Orders (`mapOrder` -- `bybit.exchange.ts:1895-1970`)

When a limit order has TP/SL attached, the `mapOrder` function generates virtual orders with synthetic IDs:

- `{orderId}__stop_loss` -- virtual stop loss
- `{orderId}__take_profit` -- virtual take profit

These virtual orders:

- Are derived from the parent order's `stopLoss` and `takeProfit` fields
- Have the **inverse side** of the parent order (Buy parent -> Sell SL/TP)
- Have `filled: 0` and `remaining` equal to the parent order's amount
- Carry trigger type information from `slTriggerBy` / `tpTriggerBy`

**Cancel path implication**: `cancelOrders` filters out virtual TP/SL IDs at
its entry point in all three variants (`bybit`, `bybitspot`, `bybitinverse`).
Synthetic IDs are not known to Bybit's cancel endpoint, and on UNIFIED
accounts they make the whole batch throw (`CANCEL_ORDERS` returns a list-level
non-zero `retExtInfo.list[].code`, which our helper rethrows). The brackets
hang off the parent limit order anyway — canceling the parent drops them.
To clear only a bracket while keeping the parent, use `updateOrder` with an
empty `stopLoss`/`takeProfit`, not `cancelOrders` on the virtual row.

### Update Order Logic (`updateOrder` -- `bybit.exchange.ts:1487-1641`)

The update path depends on the order type:

1. **Trailing stop loss**: Cancel + recreate via `placeTrailingStopLoss()` (cannot be amended in-place)

2. **Position-bound TP/SL** (non-virtual, has `tpslMode` in `order.info`):
   - Computes `projectedType` based on new price vs mark price (ensures correct TP/SL classification after price change)
   - Cancel + recreate via `placeStopLossOrTakeProfit()` using `SET_TRADING_STOP`
   - Includes `tpslMode: 'Full'` or `'Partial'` and size fields (`slSize`/`tpSize`)

3. **Virtual TP/SL** (`__stop_loss`/`__take_profit` suffix):
   - Finds the parent order by stripping the suffix from the ID
   - Updates via `REPLACE_ORDER` on the parent, setting `stopLoss` or `takeProfit` field

4. **Free-floating conditional stops** (non-virtual SL/TP without `tpslMode`, detected by absence of `tpslMode` in `order.info`):
   - Updated via `REPLACE_ORDER` directly, setting `triggerPrice` for the conditional

5. **Limit orders**: Updated via `REPLACE_ORDER` with `price` and/or `qty` fields

6. **Fallback path** (`bybit.exchange.ts:1601-1638`): For position-bound TP/SL when cancel+replace could not run (e.g., mark price unavailable), falls through to `SET_TRADING_STOP` with `tpslMode` and size parameters

7. **Demo reconciliation**: Plain limit / conditional amend success uses the
   same demo-only debounced order snapshot backstop as order creation. This is
   intentionally narrower than live Bybit, where private WS order events should
   remain the primary reconciliation path.

### Cancel Order Logic

- **UNIFIED accounts** (`bybit.exchange.ts:1668-1681`): Use batch cancel (`CANCEL_ORDERS`) with up to 10 orders per batch. Uses `unlimitedXHR` (no rate limiting). Checks `retExtInfo.list` for individual order errors.
- **CONTRACT accounts** (`bybit.exchange.ts:1682-1698`): Fall back to individual `CANCEL_ORDER` calls via `forEachSeries`. Errors are collected and thrown as a joined string.
- **Cancel all for symbol** (`cancelSymbolOrders` at `bybit.exchange.ts:1702-1726`): When `cancelTpSl=false`, preserves TP/SL orders by saving them, canceling all, then re-placing the TP/SL orders with flipped sides.
- Successful cancels force an order-inclusive REST snapshot so stale TP/SL rows
  do not linger when Bybit demo/private WS is degraded.
- **Inter-batch delay**: 1000ms between cancel batches (`bybit.exchange.ts:1666`)

## Symbol Format

- **Linear**: Raw symbol from Bybit (e.g., `BTCUSDT`, `ETHUSDC`, `BTC-27JUN25`)
- **Inverse**: Raw symbol (e.g., `BTCUSD`, `ETHUSD`)
- **Spot**: Raw symbol (e.g., `BTCUSDT`, `ETHUSDT`)
- Market ID uses `market.symbol` directly (not the `base/quote:settle` format)
- PreLaunch markets are fetched separately with `status: 'PreLaunch'` and included

## Data Precision & Conversion

### Price and Amount Adjustment

All prices and amounts use the `adjust()` utility function from `../../utils/safe-math`, which rounds values to the precision specified by the market's `tickSize` (price) and `qtyStep` (amount).

```
price = adjust(opts.price, market.precision.price)    // e.g., adjust(100.123, 0.01) -> 100.12
amount = adjust(opts.amount, market.precision.amount)  // e.g., adjust(1.5678, 0.001) -> 1.567
```

### Inverse Contract Conversions

For inverse perpetuals (`BybitInverseExchange`):

**Order amounts** (`bybitinverse.exchange.ts:991-1008`):

- API expects qty in **integer USD contract values** (each contract = $1)
- Amounts are rounded to integers with `Math.round()`, floored with `Math.floor()` for the API string
- Clamped to `[minOrderQty, maxOrderQty]` range
- Lot splitting for inverse uses `Math.floor()` for integer division

**Position contracts** (`bybitinverse.exchange.ts:1844-1867`):

- API returns `size` in USD contracts
- `contractsNative` stores the raw USD contract count
- `contracts` field is converted from USD to BTC: `contractsNative / referencePrice`
- BTC precision: `Math.round(btcAmount * 1e8) / 1e8` (8 decimal places)
- Reference price priority: `ticker.mark || ticker.last || entryPrice`
- `notional` for inverse is set to `contractsNative` (USD value)

**Order remaining** (`bybitinverse.exchange.ts:1912-1916`):

- `remainingNative` stores raw USD remaining
- `remaining` converted to BTC: `Math.round((remaining / price) * 1e8) / 1e8`

### USDC Position PnL

For non-USDT, non-inverse linear positions (USDC-settled), unrealized PnL is manually calculated (`bybit.exchange.ts:1858-1863`):

```
unrealizedPnl = direction * (ticker.mark * contracts - entryPrice * contracts)
```

### Balance Calculations

All Bybit variants use the UTA account-wide fields:

- `total = totalEquity - sumOfPositionUPnL` (position uPnL is summed from store, not the API field)
- `upnl = sum of all position unrealizedPnl`
- `used = totalMaintenanceMargin + totalInitialMargin`
- `free = totalAvailableBalance` (falls back to `totalMarginBalance`)
- Per-coin balances reported with `equity` and `usdValue`

The retired Classic-account calculation was removed. In particular, do not
restore a balance path based on per-coin `availableToWithdraw`; Bybit deprecated
that field for UNIFIED accounts on 2025-01-09.

## Rate Limiting

### Global Rate Limiting

- Global Axios rate limit: **5 requests per second** via `axios-rate-limit` (`bybit.exchange.ts:141`)
- `unlimitedXHR`: A second Axios instance without rate limiting, used for urgent operations (cancel orders) (`bybit.exchange.ts:142`)

### VIP-Level Rate Limits (`bybit.rateLimits.ts`)

The rate limit system is VIP-level aware and adjusts batch order limits dynamically.

| VIP Level            | Classic + UTA | UTA Pro |
| -------------------- | ------------- | ------- |
| No VIP               | 10            | 10      |
| VIP-1                | 20            | 20      |
| VIP-2                | 40            | 40      |
| VIP-3 to VIP-Supreme | 60            | 60      |
| PRO-1                | 100           | 150     |
| PRO-2                | 150           | 200     |
| PRO-3                | 200           | 250     |
| PRO-4 to PRO-6       | 200           | 300     |

### Batch Limit Logic (`bybit.exchange.ts:482-511`)

- Default batch limit: **10 orders** per batch
- If VIP level allows >= 20 req/s for futures: batch limit increases to **20 orders**
- The batch limit is determined at startup by fetching API key info via `/v5/user/query-api`
- Between batches, a **1200ms delay** is enforced
- Between cancel batches, a **1000ms delay** is enforced

### Pagination Limits

- Orders: 50 per page, recursive cursor-based fetch (`bybit.exchange.ts:526-527`)
- Positions: 200 per page, recursively follows `nextPageCursor` for both full-account and symbol queries
- Position pagination can return duplicate flat rows for the same symbol and
  `positionIdx`. In particular, a real updated flat row may be followed by an
  empty placeholder row (`seq: -1`, blank `updatedTime` / `positionStatus`) with
  default leverage. Deduplicate mapped positions by internal position id and
  prefer rows with real update metadata before writing to the store.
- Executions: 100 per page, max 5 recursion depth (`bybit.exchange.ts:826-829`)
- Income history: 50 per page default, max 10 pages safety limit (`bybit.exchange.ts:923`)
- OHLCV: max 1000 candles per request (`bybit.exchange.ts:773`)

## State Reconciliation

### Position Merge Strategy (`bybit.exchange.ts:294-334`)

The `fetchAndSetPrivateSnapshotHelper` method handles merging REST-fetched positions with WebSocket-pushed positions:

1. **Time-based guard**: A 1-second delay (`INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1000`) controls the merge behavior
2. **Recent WS update**: If a WebSocket position update arrived within the last 1 second, `mergeUpdatesUsingLastUpdate(fetchedPositions, storePositions)` is used. This preserves WebSocket data over REST data for recently-updated positions.
3. **No recent WS update**: REST-fetched positions are used directly via `addOrUpdatePositionsFull()`
4. The `lastWebsocketPositionsUpdate` timestamp is set by the private WS `handlePositionTopic` handler (`bybit.ws-private.ts:164`)

### Order Updates from WebSocket (`bybit.ws-private.ts:98-158`)

The private WS `handleOrderTopic` processes order lifecycle events:

1. **PartiallyFilled guard** (`bybit.ws-private.ts:105-109`): Filters out false-positive partial fill events where both price and amount are <= 0 (occurs during order replacement)
2. **Fill events**: Emitted on `Filled` or `PartiallyFilled` via `emitter.emit('fill', ...)`
3. **Terminal states** (`Cancelled`, `Filled`, `Deactivated`, `Rejected`): Orders (including virtual SL/TP) are updated with `Closed` or `Canceled` status
4. **Reject reason filtering**: `EC_PerCancelRequest` (user cancel) and `EC_NoError` are excluded from error logging
5. **Active states** (`New`, `Untriggered`, `PartiallyFilled`): Orders are added or updated in the store via `addOrUpdateOrders()`

### Reinit Flag (`needsReinit`)

Set to `true` when:

- `fetchOrdersHelper` fails (`bybit.exchange.ts:537-538`)
- `fetchPositionsHelper` fails without symbols (`bybit.exchange.ts:601-602`)

When set, the next tick cycle performs a full `fetchOrdersPositionsAndBalance()` instead of the normal positions+balance-only snapshot.

## Exchange Metadata

### Account Types

#### Unified Margin Status

Bybit documents Classic accounts as unavailable. Each variant therefore starts
with `unifiedMarginStatus = 6` (UTA 2.0 Pro), and its first wallet request uses
`accountType: 'UNIFIED'`. The status returned by `/v5/account/info` is still
retained for account diagnostics and the defensive retired-account fallback.

#### UTA 2.0 Handling (`bybit.exchange.ts:374-389`)

If the balance endpoint returns `"accountType only support UNIFIED."`, the code automatically:

1. Sets `forceUnifiedBalance = true`
2. Sets `unifiedMarginStatus = 6`
3. Retries with `accountType: 'UNIFIED'`

### Balance Calculation

**UNIFIED account**: Uses `totalEquity` minus summed position uPnL for total. Available balance from `totalAvailableBalance`. Reports per-coin balances including USD values.

The exact retired-account error match remains only as a defensive retry guard;
normal startup no longer depends on it.

### Markets Include PreLaunch

The `fetchMarketsHelper` method makes **two** parallel requests (`bybit.exchange.ts:703-709`):

1. Normal markets: `{ category, limit: 1000 }`
2. PreLaunch markets: `{ category, limit: 1000, status: 'PreLaunch' }`

Both are merged into the final market list. Active markets have `status === 'Trading' || status === 'PreLaunch'`.

**Note**: Inverse variant fetches markets sequentially, not in parallel (`bybitinverse.exchange.ts:754-759`).

### Fee Rates

Fetched per category via `fetchFees()` before market loading (`bybit.exchange.ts:681-698`). Fee rates are matched by symbol and attached to the market object as `takerFee` and `makerFee`. Silently returns empty array on failure (non-blocking).

**Spot variant**: Does not fetch fee rates.

### Income History / Transaction Log

- Uses `/v5/account/transaction-log` endpoint
- Only supports `accountType: 'UNIFIED'`
- Bybit limits queries to **7-day windows**; `fetchIncomeHistoryRange` chunks ranges accordingly with 100ms delay between chunks (`bybit.exchange.ts:983-1012`)
- Paginated with cursor, max 10 pages safety limit

**One transaction row becomes several `IncomeRecord`s.** Bybit reports `cashFlow`,
`funding` and `fee` as three independent terms on the same row, so mapping a row to a
single record merges terms of the reconciliation identity. The mapper is the pure,
exported `mapBybitTransactionToIncomeRecords` in `bybit.income.ts` — kept out of the
exchange class because an instance-field parser cannot be unit tested.

Two things that must not be re-simplified:

- **`record.fee` is load-bearing.** The previous inline mapper read only `cashFlow`, so
  every trading fee silently vanished from the ledger — an absent term, not a wrong one,
  which nothing downstream can detect. `fee` is positive when charged, so the emitted
  income is `-fee`; a negative `fee` is a rebate.
- **`tranId` is suffixed** (`:flow` / `:funding` / `:fee`). Splitting a row without
  distinct ids lets downstream dedupe collapse the terms back into one.

`change` is bybit's own net for the row. It is deliberately NOT used to derive the fee —
its exact composition is undocumented here — so the emitted incomes sum to
`cashFlow + funding - fee` rather than to `change`. Category comes from the row `type`
(`TRADE`/`DELIVERY` → realized PnL, `SETTLEMENT` → funding, `TRANSFER_*` → transfer,
`LIQUIDATION` → liquidation, `FEE_REFUND` → rebate); `incomeType` keeps the binance-style
uppercase spellings the journal aggregator recognises.

### Closed PnL — `USER_CLOSED_PNL`, and the shape trap in mapping it

`bybit` and `bybitinverse` implement `fetchClosedPnl` against the real
`/v5/position/closed-pnl` (7-day query windows via `getExecutionQueryWindows`,
`nextPageCursor` paging, `limit` 100, `category` `linear`/`inverse`). This is the
exchange-reported closed-position anchor — as opposed to the browser-inferred rows
described in `apps/web/src/components/componentWorkers/CLAUDE.md`.

**Until 2026-08 all three variants GETed Tealstreet's own
`/api/trades-history/bybit/closed-pnl`**, backed by `users_app_history_bybit_*` — tables
with 0 rows and no producer — so the method looked implemented and always returned `[]`.
`bybitspot` was still that same trap against `/api/trades-history/bybit` until the
history unification deleted the method 2026-08-16.

The mapper is the pure, exported `mapBybitClosedPnl` in `bybit.closed-pnl.ts`. Its
consumer casts the result `as unknown[] as ClosedPosition[]`, **so TypeScript catches no
shape mistake at all** and the runtime contract has to be held deliberately:

- **`side` is the CLOSING order's side and inverts** — a `Sell` close means the position
  was Long. Backwards flips every row and still compiles.
- **Coerce every numeric.** Bybit returns decimal strings; the grid sorts with
  `a.notional - b.notional` and divides by `entryPrice`, so a passed-through string is NaN.
- **`fees` is `openFee + closeFee`.** The dead route hardcoded `fees: 0`.
- **`account` must be stamped** from `opts.accountName` — that option's only remaining
  purpose now the call goes to the venue.

**`closedPnl` is NET of open and close fees AND of FUNDING** —
`closedPnl = realized − fees + funding` — so it is not directly comparable to the ledger's
`realized_pnl` category, which comes from `cashFlow` with fees and funding as separate
records. Reconciling means adding the fee term back **and** removing funding.

The funding half was measured 2026-08-13 and corrects a `funding: false` that had been
assumed: over 30 days of BTCUSDT both `--canonical` and `--closed-pnl` put the residual at
**−0.05958766** against a window funding of **−0.05958766**, equal to every digit. It hid
because the account had never been reconciled over a window containing funding and every
unit fixture used `fundingFee: '0'`, so the term was unexercised on both sides at once.

**It is not fixable on this axis alone.** The row carries no funding field, and attributing
funding to a round trip needs the interval the position was open — which the axis that
reports `opened == closed` cannot give. The interval now comes from the FILLS axis via
`reconstructPositionIntervals`, and the journal adapter `aggregateBybitClosedPnl` takes it
through `resolveContext`, subtracting funding to reach canonical gross: `--canonical` PASSes
with residual −1.96e-16 across 131 records.

`mapBybitClosedPnl` (the `ClosedPnlEntry` path) still reports the venue's net figure — it
has no fills input and is not the journal path. Without context the adapter degrades to the
same net figure rather than inventing one, which the gate then FAILs on. Contrast okx, which
reports `fundingFee` per position and needs no reconstruction.

**Spot is deliberately excluded.** `bybitspot` has no `fetchClosedPnl` and
`BybitSpotMetadata` sets `supportsClosedPnLEndpoint: false` — it inherits `true` through
the `...BybitMetadata` spread, and that flag is a **source switch** for the Closed
Positions tab, not a capability advertisement. Leaving it `true` with no implementation
renders the tab permanently empty.

## Testnet/Demo

### Demo Trading (`bybitdemo.exchange.ts`)

A minimal subclass of `BybitExchange` with only two changes:

- `isDemo = true` -- triggers demo URL selection for REST and the private WebSocket; public data intentionally uses mainnet
- `get supportedCategories()` returns `['linear']`

All logic (order placement, position management, etc.) is inherited from `BybitExchange`.

### URL Differences

| Component  | Livenet            | Testnet                    | Demo                    |
| ---------- | ------------------ | -------------------------- | ----------------------- |
| REST       | `api.bybit.com`    | `api-testnet.bybit.com`    | `api-demo.bybit.com`    |
| Public WS  | `stream.bybit.com` | `stream-testnet.bybit.com` | `stream.bybit.com`      |
| Private WS | `stream.bybit.com` | `stream-testnet.bybit.com` | `stream-demo.bybit.com` |

URL selection logic (`bybit.api.ts:27-29`):

- Demo: checks `exchange.name.includes('demo')`
- Testnet: checks `options.testnet`
- Livenet: default

### Feature Limitations

- Demo testnet fallback: `DEMO_URL.testnet` falls through to `api-testnet.bybit.com` (same as regular testnet)
- Demo only supports `linear` category (no inverse or spot)

## Known Quirks / Implementation Notes

### Virtual TP/SL Orders

When a limit order has TP/SL attached, they are represented as virtual orders with IDs:

- `{orderId}__stop_loss`
- `{orderId}__take_profit`

These are bracket orders on the parent order, not independent position stops. They are updated via `REPLACE_ORDER` on the parent order, not via `SET_TRADING_STOP`.

### Position-Bound TP/SL vs Free-Floating Conditional Stops

The `updateOrder` method distinguishes between:

1. **Virtual TP/SL** (`__stop_loss`/`__take_profit` suffix): Updated via `REPLACE_ORDER` on parent
2. **Position-bound TP/SL** (has `tpslMode` in `order.info`): Cancel + recreate via `SET_TRADING_STOP`
3. **Free-floating conditional stops** (no `tpslMode`, has `triggerPrice` in info for inverse/spot, or lacks `tpslMode` for linear): Updated via `REPLACE_ORDER` directly

**Note**: The detection of free-floating conditionals differs between supported
derivatives and spot:

- Linear and inverse: check for the absence of position TP/SL mode (`Full` /
  `Partial`), so websocket sentinel values such as `UNKNOWN` do not turn
  opening triggers into protective stops.
- Spot: checks `Boolean(order.info?.triggerPrice)`.

### TP/SL as Position Stops

When a full-sized position exists on the opposite side, `StopLoss` and `TakeProfit` orders are placed via `SET_TRADING_STOP` endpoint instead of `CREATE_ORDER`. This endpoint:

- Does NOT return an `orderId` (returns empty array)
- Supports `tpslMode: 'Full'` (entire position) or `'Partial'` (specific size)
- Uses `slSize`/`tpSize` for partial stops
- Forces an order-inclusive REST snapshot after success so `/v5/order/realtime`
  can surface the real TP/SL row IDs even when private WS is degraded or
  geo-flaky. Do not gate this refresh solely on WebSocket health.

### Trailing Stop Loss

Trailing stops are set via `SET_TRADING_STOP` with `trailingStop` parameter (distance from current price, not absolute price). The distance is computed as `|ticker.last - opts.price|` adjusted to price precision (`bybit.exchange.ts:1227-1230`). Updating a trailing stop requires cancel + recreate.

### Inverse Contract Amount Handling

For inverse perpetuals (`BybitInverseExchange`):

- API expects qty in **integer USD contract** values (each contract = $1)
- Amounts are rounded to integers with `Math.round()` and floored with `Math.floor()` for API strings
- Position `contracts` field is converted from USD to BTC: `contractsNative / referencePrice` (8 decimal precision)
- `extra.contractsNative` stores the raw USD contract count
- `extra.unrealizedPnlNative` stores PnL in native currency (BTC for inverse)

### Batch Order Cancellation

- **UNIFIED accounts**: Use batch cancel endpoint (`CANCEL_ORDERS`) with up to 10 orders per batch
- **CONTRACT accounts**: Fall back to individual `CANCEL_ORDER` calls sequentially
- Cancel operations use `unlimitedXHR` (no rate limiting) for urgency

### Cancel All Preserves TP/SL

`cancelSymbolOrders` with `cancelTpSl=false` (`bybit.exchange.ts:1702-1726`):

1. Saves existing TP/SL orders
2. Cancels ALL orders for the symbol
3. Re-places saved TP/SL orders with **flipped sides** (Buy -> Sell, Sell -> Buy)

Comment in code: "we need to re-create TP/SL after cancel all / before bybit was not canceling them"

### Position Mode

- `positionIdx: 0` = One-Way mode
- `positionIdx: 1` = Hedge mode (Buy side / Long)
- `positionIdx: 2` = Hedge mode (Sell side / Short)
- Mode `3` = Hedged when setting via `SET_POSITION_MODE`, mode `0` = One-Way
- Position mode is per-category for linear (settles with `coin: 'USDT'`)
- For inverse, position mode can be set per-symbol

**Stop order position index** (`getStopOrderPositionIdx` at `bybit.exchange.ts:2003-2006`): Inverts the position index (`{ 0: 0, 1: 2, 2: 1 }`) because stop orders close the opposite side.

**Linear changePositionMode quirk** (`bybit.exchange.ts:1759-1769`): The `symbol` parameter is unconditionally set to `undefined` on line 1760 (`symbol = undefined`), forcing bulk mode change for all USDT linear symbols.

### Margin Type

- Margin mode is global for a UTA account (`marginTypeIsGlobal: true`), not per symbol.
- `changeMarginType(true)` sends `setMarginMode: 'ISOLATED_MARGIN'`.
- `changeMarginType(false)` sends `setMarginMode: 'REGULAR_MARGIN'` (cross margin).
- After a successful switch, every position is refetched because one request changes the whole account.
- Bybit returns detailed failures in `result.reasons`; surface those reason messages before the generic `retMsg`.

### WebSocket Position Update Invalidation

A 1-second delay (`INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY`) is applied when merging REST-fetched positions with WebSocket-pushed positions. If a WebSocket position update arrived within 1 second, `mergeUpdatesUsingLastUpdate` preserves the WS data over REST data.

### Time Synchronization

- REST: `virtualClock.getCurrentTimeMs()` for timestamps
- If recv_window errors occur, auto-syncs via `virtualClock.syncBybitClientServerTime()`
- WebSocket: Syncs time before auth on every connection/reconnection

### Spot-Specific: PnL Calculation on Ticker Update

The spot public WebSocket calculates unrealized PnL for positions on each ticker update (`bybitspot.ws-public.ts:182-195`):

```
unrealizedPnl = directionMult * (currentValue - entryValue)
```

where `directionMult` is +1 for Long, -1 for Short.

### Spot-Specific: No Batch Orders

The spot types file (`bybitspot.types.ts`) does not define `CREATE_BATCH_ORDER`. Spot uses individual order placement only.

### Spot-Specific: Position Index Always 0

Spot `getOrderPositionIdx` always returns `0` regardless of options (`bybitspot.exchange.ts:1195-1197`).

### Position Fetch Limitation

Linear and inverse position snapshots follow cursors. Do not collapse them back
to a single 200-row request. Linear snapshots also deduplicate duplicate rows by
internal position id; keep this guard in place so Bybit placeholder rows from
later pages cannot overwrite fresher leverage or position metadata.

### Public WS hot-path conventions

The public WS files (`bybit.ws-public.ts`, `bybitinverse.ws-public.ts`,
`bybitspot.ws-public.ts`) follow the shared hot-path patterns documented
in `../CLAUDE.md` → "Public WebSocket Hot-Path Patterns". Notable
applications in this directory:

- **`onMessage` dispatch is O(1) via `"topic":"<full>"` extract + Map
  lookup**, with a first-segment prefix fallback for handlers keyed by
  short prefix (`tickers`, `kline`). Don't reintroduce the
  `Object.entries + data.includes(\`topic":"${topic}\`)` walk — it was
removed because every WS frame paid N substring scans (one per
registered handler) over multi-KB payloads. **`recordMessageHandlerStats`
must be called with the handler key that matched, not the raw wire
topic** — on a prefix-only match (`tickers`, `kline`) record the prefix
key. Recording `fullTopic`there once ballooned`messageHandlersStats`
  to one entry per subscribed symbol (~600 on linear) and put every
  ticker under reconnect-banner performance tracking.
- **Orderbook deltas don't trail a `sortOrderBook`.** Bybit V5 sends
  snapshots pre-sorted (bids DESC, asks ASC) and `updateOrderBook` does
  binary-search sorted insertion. The trailing sort that used to live
  here was re-sorting an already-sorted 1000-level book on every delta.
- **Snapshot/delta parser uses direct `data.a` / `data.b` indexed
  for-loops**, not `Object.entries(data.data).forEach`.
- **Orderbook deltas use `updateOrderBookByPrice`, not `updateOrderBook`.**
  ~95% of incoming delta levels on a 1000-level book are existing-level
  mutations or removals; the prior object-arg form allocated a wrapper
  per delta level only to read `.price` for binary search and `.amount`
  for dispatch. The primitive-arg sibling allocates inside the function
  only on the insertion path. See `../../utils/CLAUDE.md` →
  "Orderbook utilities — two API shapes".
- **Ticker handler passes the ticker reference directly to
  `updateTicker`.** `tickersBySymbol[d.symbol]` already returns the
  object the store needs — passing `{ id: d.symbol }` allocated a fresh
  wrapper per ticker frame (~600/sec on linear) just to hand the same
  string back. Linear/inverse/spot all match this shape.
- **`secondaryParser` (publicTrade → tick-by-tick candle) avoids
  `..._rest` destructuring**, uses plain `+` for volume accumulation
  (not `safe-math.add`), and uses ternaries over `Math.max/min`.
  Bybit V5 publicTrade carries `s/L/i/BT/RPI` we don't read; the prior
  rest-spread allocated a wrapper per trade. Same shape applies to the
  primary kline `parser` for the existing-candle update branch.
- **`listenOHLCVs` (multi-symbol kline) caches `inverseObj(INTERVAL)`
  to module scope** (`INTERVAL_INVERSE`) and replaces `topic.split('.')`
  with two `indexOf` slices. The previous shape rebuilt the inverse
  map per kline frame via `Object.entries + spread-reduce` (quadratic
  in INTERVAL key count) and allocated a 3-element array via `split`.
- **Spot `handleTickersEvent` uses an indexed for-loop with `continue`
  for the position-PnL pass**, not `positions.filter(...)`. Hoist
  `marketsBySymbol[d.symbol]` outside the loop — all matching
  positions are for the same symbol so the market lookup is invariant.

## Error Handling

### REST API Error Codes and Patterns

| Error Message / Code                          | Handling                                                                               | Location                           |
| --------------------------------------------- | -------------------------------------------------------------------------------------- | ---------------------------------- |
| `"error sign!"`                               | Auto-retry up to 3 times (via `axios-retry`)                                           | `bybit.api.ts:17-22`               |
| `"timestamp and recv_window param"`           | Auto time sync via `virtualClock.syncBybitClientServerTime()`                          | `bybit.api.ts:92-96`               |
| `"accountType only support UNIFIED."`         | Set `forceUnifiedBalance=true`, `unifiedMarginStatus=6`, retry                         | `bybit.exchange.ts:380-383`        |
| Account margin-mode failure                   | Surface `result.reasons[].reasonMsg`, then throw                                       | `changeMarginType`                 |
| `"order not modified"`                        | Treated as success (no-op), no error thrown                                            | `bybit.exchange.ts:1550,1593,1633` |
| `110043` / `"leverage not modified"`          | Treated as success (desired leverage is already active); refresh positions afterward   | `setLeverage`                      |
| `"Partial symbols switched successfully..."`  | Logged as error but does not throw (partial success on position mode switch)           | `bybit.exchange.ts:1787-1789`      |
| `"All symbols switched successfully."`        | Silent success                                                                         | `bybit.exchange.ts:1786`           |
| Network errors                                | Auto-retry up to 3 times (via `axios-retry`)                                           | `bybit.api.ts:49-52`               |
| `"too much traffic or a configuration error"` | Logged as "Server was unavailable", no automatic retry beyond axios-retry              | `bybit.api.ts:98-100`              |
| Batch order partial failures                  | Individual errors reported via `retExtInfo.list[].code`, successful IDs still returned | `bybit.exchange.ts:1376-1401`      |
| Batch placement failure                       | Falls back to individual `placeOrder()` per order in the failed batch                  | `bybit.exchange.ts:1336-1346`      |

### WebSocket Error Handling

| Error                               | Handling                                                      | Location                      |
| ----------------------------------- | ------------------------------------------------------------- | ----------------------------- |
| Auth failure (`success === false`)  | Log error, retry up to 3 times with `sendReconnectSequence()` | `bybit.ws-private.ts:72-87`   |
| PartiallyFilled with price/amount 0 | Ignored as false positive (occurs during order replacement)   | `bybit.ws-private.ts:105-109` |
| Reject reasons                      | Logged unless `EC_PerCancelRequest` or `EC_NoError`           | `bybit.ws-private.ts:129-136` |

### Auth failures resolve as successes, and used to poll forever

**The HOW adapter resolves non-2xx without axios' `settle()`**, so a 401 reaches
the response interceptor as a _success_ carrying `{"raw_response":""}` — no
`retCode`, no `retMsg`. `getBybitResponseMeta` reports `hasExplicitStatus:
false`, the existing branch skips it, and every
`emitAndLogError(v(data,'retMsg'))` caller then emits `undefined`, which
`base.ts` renders as the contentless **`Unknown Exchange Error`**. A bad key
produced 89 of those in a single 30-second e2e window.

The interceptor now synthesizes `retCode`/`retMsg` from the HTTP status. Spell
the reason out (`HTTP 401 Unauthorized — API key rejected`) rather than emitting
a bare `HTTP 401`: **the tunnel frame carries no `statusText`**, and both
`health/errorPatterns.ts`'s `auth_invalid` set and the e2e toast assertion match
on words like "unauthorized" / "api key", never on the numeric status.

Nothing backed off either — the tick loop re-armed unconditionally at the 500ms
metadata interval, so the error count was purely a function of how long you
watched (120 errors vs a bound of 100). `BaseExchange.recordAuthFailure()` /
`recordAuthSuccess()` drive an exponential backoff inside `getTickInterval()`.
Two rules for calling them:

- **Record failure from the transport layer**, which is the only place that sees
  the HTTP status. Also record on the `BYBIT_AUTH_RET_CODES` set, because bybit
  returns credential failures inside a normal envelope too — sometimes on HTTP
  200 — so status alone under-counts. `10004` ("error sign") is deliberately
  excluded: it is usually clock drift and is already retried.
- **Only a signed request may record success.** `KLINE` is fetched unsigned and
  shares the same client, so a chart fetch would otherwise reset the streak
  while the credentials are still rejected.

### Error-as-Success Patterns

- **"order not modified"**: Returned when `REPLACE_ORDER` is called but nothing actually changed. Treated as success in all three contexts (virtual SL/TP update, limit order update, position-bound TP/SL update).
- **"leverage not modified" / `110043`**: Returned when `SET_LEVERAGE` is called
  with the already-active leverage. Treat as a successful no-op and refresh the
  symbol's positions instead of surfacing an error toast. This applies to demo
  accounts too because `BybitDemoExchange` inherits `BybitExchange.setLeverage`.
- **Empty cancel results**: Individual cancel errors within a batch cancel are collected but the overall operation may still partially succeed.

## Exchange Variant Differences

| Feature               | `BybitExchange` (Linear)       | `BybitInverseExchange`              | `BybitSpotExchange`                 | `BybitDemoExchange`          |
| --------------------- | ------------------------------ | ----------------------------------- | ----------------------------------- | ---------------------------- |
| Base class            | `BaseExchange`                 | `BaseExchange`                      | `BaseExchange`                      | `BybitExchange`              |
| Category              | `linear`                       | `inverse`                           | `spot`                              | `linear`                     |
| Settle coins          | USDT (+ USDC for UNIFIED)      | `[undefined]` (coin-settled)        | N/A                                 | USDT (+ USDC for UNIFIED)    |
| Position tracking     | Yes                            | Yes (USD->BTC conversion)           | No (`fetchPositions` returns `[]`)  | Yes                          |
| Batch orders          | Yes (`CREATE_BATCH_ORDER`)     | Yes (`CREATE_BATCH_ORDER`)          | No                                  | Yes                          |
| isSpot                | false                          | false                               | true                                | false                        |
| isDemo                | false                          | false                               | false                               | true                         |
| REST URL              | `api.bybit.com`                | `api.bybit.com`                     | `api.bybit.com`                     | `api-demo.bybit.com`         |
| Public WS             | `v5/public/linear`             | `v5/public/inverse`                 | `v5/public/spot`                    | `v5/public/linear` (mainnet) |
| Orderbook depth       | 1000 levels                    | 1000 levels                         | 200 levels                          | 1000 levels                  |
| Leverage range        | From market info               | From market info                    | Fixed 1x                            | From market info             |
| Amount handling       | Precision-adjusted float       | Integer USD contracts               | Precision-adjusted float            | Precision-adjusted float     |
| Fee rates fetched     | Yes                            | Yes                                 | No                                  | Yes                          |
| Dynamic ticker topics | No (subscribes all on connect) | Yes (`activeTickerSymbols:changed`) | Yes (`activeTickerSymbols:changed`) | No                           |
| Income history        | Yes (UNIFIED only)             | No                                  | No                                  | Yes (inherited)              |
| Execution queries     | Yes                            | Yes                                 | No                                  | Yes (inherited)              |
| Market fetch parallel | Yes (PreLaunch in parallel)    | No (sequential)                     | No (single request, no PreLaunch)   | Yes (inherited)              |

## Testing / Validation

Run from the `packages/safe-cex` directory:

```bash
yarn lint
yarn typecheck
yarn test-ci
```

## Common Issues and Solutions

1. **"error sign!" on requests**: Signature mismatch. Check time sync, recv_window. The API retries 3x automatically.
2. **"timestamp and recv_window param"**: Local clock is out of sync. Auto-resolved by `virtualClock.syncBybitClientServerTime()`.
3. **"accountType only support UNIFIED."**: Defensive fallback for an unexpected stale account status; normal requests are UNIFIED-first.
4. **Margin-mode switch rejected**: Surface Bybit's returned reason; the integration uses the UTA account-level margin-mode endpoint.
5. **Batch order failures**: Falls back to individual order placement per order in the failed batch.
6. **WebSocket auth failure**: Retries up to 3 times with time re-sync between attempts.
7. **TP/SL not updating**: Check if the order is virtual (bracket on parent), position-bound, or free-floating conditional -- each uses a different update path.
8. **Order "not modified" response**: Treated as success (no-op), not an error.
9. **Partial position mode switch**: Returns "Partial symbols switched successfully, excluding symbols with open orders or positions." -- logged as error but does not throw.
10. **False PartiallyFilled on replace**: When replacing an order, Bybit emits a PartiallyFilled event with 0 price and 0 amount. This is filtered out in `handleOrderTopic`.
11. **Position pagination**: Position fetches follow `nextPageCursor`; do not collapse them back to a single 200-row request.

## `Execution` position fields — only `maker` exists

`/v5/execution/list` carries `isMaker`, and that is the ONLY one of
`positionSide` / `realizedPnl` / `maker` it reports. A live key dump (2026-08,
`yarn history-audit --fills-probe --targets bybit:6:DOGEUSDT`) returned no `execPnl`,
no `closedPnl` and no position side. `closedSize` is the closest-named field and is a
size, not a PnL — do not map it to `realizedPnl`.

Realized PnL comes from `/v5/position/closed-pnl` instead, and **that figure is NET of
fees**, whereas `Execution.realizedPnl` is documented gross. Converting is required
before the two can be summed; skipping it produces a residual exactly the size of the
fee term, which reads as a mapper bug rather than a convention mismatch.

## Closed-PnL rows carry no real open time

Measured live 2026-08-12 on row 6. Every `/v5/position/closed-pnl` row returns
`opened == closed` — the same timestamp on both ends, even for a position held for
seconds across several fills:

```
closed=15:12:39 opened=15:12:39 long qty=141 entry=0.07078 exit=0.07077 pnl=-0.0123872
```

OKX's equivalent axis returns the true open time, so **position duration is derivable
there and not here**. Anything showing hold time, or bucketing trades by session, must
either source `openedAt` from fills on bybit or mark it unavailable — computing it from
these two fields yields zero for every trade.

`realizedPnl` here is NET of fees, matching OKX's closed-position axis and opposite to the
gross convention on both venues' fills: `-0.0123872 + 0.0109772 = -0.00141` gross.

One more shape note: a partial residual closes as its own row with its own size
(`qty=1` rows appeared from a 1-DOGE remainder), so row count is not trade count.

## Hedge mode: the leg is NOT on fills — join orders on `orderId`

Measured live 2026-08-12 (row 6) against a hedge-mode e2e run:

- `/v5/execution/list` returns **no position side at all**, in hedge mode as in one-way.
  Its full key set contains no `positionIdx` and no `posSide`.
- `/v5/order/history` carries **`positionIdx` on every row** — census `0:34 1:9 2:7` over
  the run, where `0` is one-way, `1` the long leg and `2` the short leg.
- `/v5/position/closed-pnl` carries a side, but no usable open time (see above).

So a hedged bybit account **cannot be folded from fills alone** — the fill does not know
which leg it belongs to, and the order side cannot be used to infer it (a `sell` opens the
short leg or closes the long one, and both occur seconds apart). The leg has to come from
the order axis, joined on `orderId`, which makes hedge-mode reconstruction a two-axis
operation on this venue.

OKX is the contrast: its fills carry `posSide` directly, so the same reconstruction is
single-axis there. Do not write one fold and assume it ports.
