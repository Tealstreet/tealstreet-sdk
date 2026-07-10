# Binance Exchange Integration

This directory contains three Binance exchange variants for Tealstreet: **USD-M Futures**, **Spot**, and **Portfolio Margin**. All variants share the same directory rather than having separate subdirectories.

## Exchange Information

| Property           | Futures (`binance`) | Portfolio Margin (`binancepm`) | Spot (`binancespot`)      |
| ------------------ | ------------------- | ------------------------------ | ------------------------- |
| Exchange class     | `BinanceExchange`   | `BinancePMExchange`            | `BinanceSpotExchange`     |
| Pretty name        | Binance             | Binance Portfolio Margin       | Binance Spot              |
| Production enabled | Yes                 | Yes                            | No (`enabledInDev: true`) |
| Testnet support    | Yes                 | Yes                            | Yes                       |

## API Documentation

- **USD-M Futures**: https://developers.binance.com/docs/derivatives/usds-margined-futures/general-info
- **COIN-M Futures**: https://developers.binance.com/docs/derivatives/coin-margined-futures/general-info
- **Portfolio Margin**: https://developers.binance.com/docs/derivatives/portfolio-margin/general-info
- **Spot API**: https://developers.binance.com/docs/binance-spot-api-docs/rest-api
- **CCXT REST**: https://github.com/ccxt/ccxt/blob/master/ts/src/binance.ts
- **CCXT WebSocket**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/binance.ts

## File Structure

### USD-M Futures (core variant, `exchangeName: 'binance'`)

| File                    | Description                                                                              |
| ----------------------- | ---------------------------------------------------------------------------------------- |
| `binance.api.ts`        | REST API client with HMAC-SHA256 signing, public and authenticated axios instances       |
| `binance.exchange.ts`   | Main exchange class (`BinanceExchange`) extending `BaseExchange`                         |
| `binance.types.ts`      | Type definitions, endpoint constants, type mappings, income history types                |
| `binance.ws-private.ts` | Private WebSocket: account updates, order fills, algo order events, listenKey management |
| `binance.ws-public.ts`  | Public WebSocket: tickers, book tickers, mark price, kline, orderbook depth              |

### Spot (`exchangeName: 'binancespot'`)

| File                        | Description                                                                    |
| --------------------------- | ------------------------------------------------------------------------------ |
| `binancespot.api.ts`        | Spot REST API client (same HMAC-SHA256 signing, different base URL)            |
| `binancespot.exchange.ts`   | Spot exchange class (`BinanceSpotExchange`) extending `BaseExchange`           |
| `binancespot.types.ts`      | Spot-specific endpoints and base URLs                                          |
| `binancespot.ws-private.ts` | Spot private WebSocket: `executionReport` and `outboundAccountPosition` events |
| `binancespot.ws-public.ts`  | Spot public WebSocket: tickers, book tickers, kline, orderbook (no mark price) |

### Portfolio Margin (`exchangeName: 'binancepm'`)

| File                      | Description                                                                                |
| ------------------------- | ------------------------------------------------------------------------------------------ |
| `binancepm.api.ts`        | PM REST API client with dual base URL support (`papi` and `fapi`)                          |
| `binancepm.exchange.ts`   | PM exchange class (`BinancePMExchange`) extending `BaseExchange`                           |
| `binancepm.types.ts`      | PM-specific endpoints (papi/v1 paths) and base URLs                                        |
| `binancepm.ws-private.ts` | PM private WebSocket: `CONDITIONAL_ORDER_TRADE_UPDATE` events, throttled position fetching |
| `binancepm.ws-public.ts`  | PM public WebSocket: same as futures (uses fstream for market data)                        |

### Other Files

| File           | Description                        |
| -------------- | ---------------------------------- |
| `.apikey.json` | Local API key storage (gitignored) |
| `.gitignore`   | Ignores `.apikey.json`             |

## Authentication

All three variants use the same authentication mechanism:

- **Method**: HMAC-SHA256 signature
- **Header**: `X-MBX-APIKEY: <api_key>` sent on all authenticated requests
- **Signature generation** (`binance.api.ts:99-112`):
  1. Collect all request parameters (query params or body data)
  2. Add `timestamp` (from `virtualClock.getCurrentTimeMs()`) and `recvWindow` (default `20_000` ms)
  3. Serialize to query string via `qs.stringify` with `arrayFormat: 'repeat'`
  4. Compute `HMAC-SHA256(secret, queryString)` and append as `signature` parameter
- **All data sent as query params**: Even POST/PUT/DELETE requests have body data moved to query params after signing (Binance API requirement, `binance.api.ts:112`)
- **Listen key requests**: On livenet, listen key POST/PUT requests are NOT signed (they only need the API key header) (`binance.api.ts:83`)
- **Public endpoints**: Requests to public endpoints strip the API key header and use a longer timeout (20s) (`binance.api.ts:87-97`)
- **Big integer handling**: Uses `bigIntAsStringJsonParse` for response transformation to avoid JS precision loss on large order IDs (`binance.api.ts:30`)
- **Time sync**: Uses `virtualClock` for timestamps, which accounts for clock drift between client and server

### Client Order ID Prefix

All orders use the broker prefix `x-s3vFFRQp` followed by a UUID:

- Regular orders: `newClientOrderId: 'x-s3vFFRQp' + uuid()`
- Algo orders: `clientAlgoId: 'x-s3vFFRQp' + uuid()`

### Secondary Key Support

- **Futures**: Supports a secondary API key via `createApiWithSecondaryKey` (`binance.exchange.ts:92`). The `xhr` property automatically rotates between primary and secondary keys. Both keys are validated on startup. The secondary key rate limit is commented out.
- **Portfolio Margin**: `validateAccount()` validates both keys explicitly via `_primaryXhr` and `_secondaryXhr` (wired through `createApiWithSecondaryKey`, kept on the PM instance directly since PM has no single `this.xhr`) when `secondary_key` is set (`binancepm.exchange.ts` → `validateAccount`).
- **Spot**: No secondary key support.

## Startup Sequence

All three variants follow the same high-level pattern, with variant-specific differences noted below.

### Futures (`BinanceExchange.start()`, `binance.exchange.ts:147-178`)

1. **`fetchMarkets()`** -- Sequential REST call to `GET /fapi/v1/exchangeInfo` (public) + `GET /fapi/v1/leverageBracket` (authenticated). Filters for `PERPETUAL` contracts with `USDT` or `USDC` margin. Extracts `PRICE_FILTER`, `LOT_SIZE`, and `MARKET_LOT_SIZE` from symbol filters. Updates store with `markets` and sets `loaded.markets = true`.
2. **Disposal check** -- If disposed after any async step, returns immediately.
3. **`fetchTickers()`** -- Three sequential REST calls: `GET /fapi/v1/ticker/24hr`, `GET /fapi/v1/ticker/bookTicker`, `GET /fapi/v1/premiumIndex` (each awaited individually, not using `Promise.all`). Merges into `Ticker[]` using `marketsBySymbol` O(1) lookup. Updates store with `tickers` and `loaded.tickers = true`.
4. **`publicWebsocket.connectAndSubscribe()`** -- Initiates the public WebSocket connection (non-blocking).
5. **Branch**: If `publicDataOnly`, calls `_startPublicDataOnly()` which sets empty orders/balance/positions and marks all as loaded. Otherwise, calls `_startPrivate()`.

### Private Startup (`_startPrivate()`, `binance.exchange.ts:180-205`)

1. **`privateWebsocket.connectAndSubscribe()`** -- Fetches listen key via `POST /fapi/v1/listenKey`, opens WebSocket to `wss://fstream.binance.com/private/ws?listenKey={listenKey}` (post-2026-04-23 format; the old `/ws/{listenKey}` path was decommissioned). Non-blocking.
2. **`fetchPositionMode()`** -- `GET /fapi/v1/positionSide/dual`. Sets `isHedged` store setting based on `dualSidePosition`.
3. **`tick()`** -- First tick executes immediately (count=0, so `count % 16 === 0` triggers `fetchPositions()`), then calls `fetchPositionsAndBalance()`. Starts the 5s tick loop.
4. **`fetchOrders()`** -- Two parallel calls: `GET /fapi/v1/openOrders` + `GET /fapi/v1/openAlgoOrders?algoType=CONDITIONAL`. Merges regular + algo orders. Updates store.
5. **`fetchPositions()`** -- Background call (no await), kicks off a detailed `GET /fapi/v2/positionRisk` fetch.

### Portfolio Margin Startup Differences

- Uses `papiXhr` instead of `xhr` for authenticated endpoints.
- `fetchBalance()` is separate from position fetching (tick calls `fetchBalance()` instead of `fetchPositionsAndBalance()`).
- `fetchPositions()` creates fake zero-size positions for all markets to ensure UI shows all symbols (`binancepm.exchange.ts:343-365`).

### Spot Startup Differences

- No `fetchPositionMode()` call (no hedge mode support).
- `tick()` has no counter-based position risk refresh; always calls `fetchPositionsAndBalance()`.
- Uses `options.extra?.tickInterval` for tick interval instead of hardcoded 5000ms.

## REST API Endpoints

### USD-M Futures (`fapi.binance.com`)

| Endpoint                     | Method      | Description                                                                          |
| ---------------------------- | ----------- | ------------------------------------------------------------------------------------ |
| `/fapi/v1/exchangeInfo`      | GET         | Fetch markets (symbols, filters, contract types)                                     |
| `/fapi/v2/balance`           | GET         | Fetch account balances per asset                                                     |
| `/fapi/v2/account`           | GET         | Fetch account info with positions and balances                                       |
| `/fapi/v1/leverageBracket`   | GET         | Fetch leverage brackets per symbol                                                   |
| `/fapi/v1/ticker/24hr`       | GET         | 24h ticker statistics                                                                |
| `/fapi/v1/ticker/bookTicker` | GET         | Best bid/ask prices                                                                  |
| `/fapi/v1/premiumIndex`      | GET         | Mark price, index price, funding rate                                                |
| `/fapi/v1/openOrders`        | GET         | Fetch all open regular orders                                                        |
| `/fapi/v1/openAlgoOrders`    | GET         | Fetch all open algo orders (params: `algoType=CONDITIONAL`)                          |
| `/fapi/v1/order`             | POST/DELETE | Place or cancel a single regular order                                               |
| `/fapi/v1/batchOrders`       | POST/DELETE | Place or cancel batch regular orders (max 5 per batch for placing, 10 for canceling) |
| `/fapi/v1/algoOrder`         | POST/DELETE | Place or cancel a single algo order (SL/TP/trailing)                                 |
| `/fapi/v1/allOpenOrders`     | DELETE      | Cancel all open orders for a symbol                                                  |
| `/fapi/v1/positionSide/dual` | GET/POST    | Get or set hedge mode (dual position side)                                           |
| `/fapi/v1/leverage`          | POST        | Set leverage for a symbol                                                            |
| `/fapi/v1/marginType`        | POST        | Set margin type (ISOLATED or CROSSED)                                                |
| `/fapi/v2/positionRisk`      | GET         | Fetch detailed position risk info                                                    |
| `/fapi/v1/klines`            | GET         | OHLCV candle data                                                                    |
| `/fapi/v1/depth`             | GET         | Orderbook snapshot (limit up to 1000)                                                |
| `/fapi/v1/trades`            | GET         | Recent public trades (max 1000)                                                      |
| `/fapi/v1/userTrades`        | GET         | User trade history (requires symbol)                                                 |
| `/fapi/v1/income`            | GET         | Income history (realized PnL, funding fees, commissions)                             |
| `/fapi/v1/listenKey`         | POST/PUT    | Create or keepalive a user data stream listen key                                    |

### Spot (`api.binance.com`)

| Endpoint                 | Method      | Description                              |
| ------------------------ | ----------- | ---------------------------------------- |
| `/api/v3/exchangeInfo`   | GET         | Fetch spot markets                       |
| `/api/v3/ticker/24hr`    | GET         | 24h ticker statistics (includes bid/ask) |
| `/api/v3/order`          | POST/DELETE | Place or cancel a single order           |
| `/api/v3/openOrders`     | DELETE      | Cancel all open orders for a symbol      |
| `/api/v3/allOrders`      | GET         | Fetch open orders                        |
| `/api/v3/klines`         | GET         | OHLCV candle data                        |
| `/api/v3/depth`          | GET         | Orderbook snapshot                       |
| `/api/v3/userDataStream` | POST/PUT    | Create or keepalive a listen key         |

### Portfolio Margin (`papi.binance.com` + `fapi.binance.com` for public data)

| Endpoint                             | Method      | Description                              |
| ------------------------------------ | ----------- | ---------------------------------------- |
| `/papi/v1/balance`                   | GET         | PM account balance                       |
| `/papi/v1/account`                   | GET         | PM account info                          |
| `/papi/v1/um/leverageBracket`        | GET         | UM leverage brackets                     |
| `/papi/v1/um/openOrders`             | GET         | Open regular orders                      |
| `/papi/v1/um/conditional/openOrders` | GET         | Open conditional (SL/TP) orders          |
| `/papi/v1/um/order`                  | POST/DELETE | Place or cancel a regular order          |
| `/papi/v1/um/conditional/order`      | POST/DELETE | Place or cancel a conditional order      |
| `/papi/v1/um/allOpenOrders`          | DELETE      | Cancel all open orders for a symbol      |
| `/papi/v1/um/positionSide/dual`      | GET/POST    | Hedge mode                               |
| `/papi/v1/um/leverage`               | POST        | Set leverage                             |
| `/papi/v1/um/positionRisk`           | GET         | Position risk                            |
| `/papi/v1/um/userTrades`             | GET         | User trades                              |
| `/papi/v1/listenKey`                 | POST/PUT    | Listen key                               |
| `/fapi/v1/*`                         | GET         | Public market data (shared with futures) |

## WebSocket Implementation

### Base URLs

As of **2026-04-23**, Binance retired the legacy `wss://fstream.binance.com/ws` endpoint for USDⓈ-M futures and split it into three category-specific base URLs (`/public/ws`, `/market/ws`, `/private/ws`). Spot and PM private (`/pm/ws/`) endpoints are unchanged. See: [Important WebSocket Change Notice](https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams/Important-WebSocket-Change-Notice).

| Variant             | Public (high-frequency)                    | Market (regular market data)               | Private (user data)                                     |
| ------------------- | ------------------------------------------ | ------------------------------------------ | ------------------------------------------------------- |
| **Futures** livenet | `wss://fstream.binance.com/public/ws`      | `wss://fstream.binance.com/market/ws`      | `wss://fstream.binance.com/private/ws?listenKey=…`      |
| **Futures** testnet | `wss://stream.binancefuture.com/public/ws` | `wss://stream.binancefuture.com/market/ws` | `wss://stream.binancefuture.com/private/ws?listenKey=…` |
| **PM** livenet      | `wss://fstream.binance.com/public/ws`      | `wss://fstream.binance.com/market/ws`      | `wss://fstream.binance.com/pm/ws/{listenKey}`           |
| **PM** testnet      | `wss://stream.binancefuture.com/public/ws` | `wss://stream.binancefuture.com/market/ws` | `wss://stream.binancefuture.com/pm/ws/{listenKey}`      |
| **Spot** livenet    | `wss://stream.binance.com:9443/ws`         | (same as public — unaffected by split)     | `wss://stream.binance.com:9443/ws/{listenKey}`          |
| **Spot** testnet    | `wss://testnet.binance.vision/ws`          | (same as public — unaffected by split)     | `wss://testnet.binance.vision/ws/{listenKey}`           |

**Stream → endpoint routing (futures + PM):**

- `/public/ws`: `!bookTicker`, `{symbol}@bookTicker`, `{symbol}@depth`, `{symbol}@rpiDepth`
- `/market/ws`: `!ticker@arr`, `!markPrice@arr@1s`, `{symbol}@kline_{interval}`, `@aggTrade`, `@trade`, `@forceOrder`, `@miniTicker`, etc.
- `/private/ws`: user data via `?listenKey=<key>` (and optional `&events=...` filter — omitted in our impl to receive all events, matching legacy behavior)

### Public WebSocket Channels

For futures and PM, channels are split between two underlying connections owned by `BinancePublicWebsocket` / `BinancePMPublicWebsocket`:

- The class itself connects to `/public/ws` and handles `!bookTicker` + `{symbol}@depth`.
- A sibling `BinanceMarketWebsocket` / `BinancePMMarketWebsocket` connects to `/market/ws` and handles `!ticker@arr`, `!markPrice@arr@1s`, and `{symbol}@kline_{interval}`.
- Callers continue to use the single `publicWebsocket` reference; `listenOHLCV` is forwarded to the sibling market socket while `listenOrderBook` stays on the public socket.

Subscribed on connect via `SUBSCRIBE` method:

| Channel                     | Endpoint (futures/PM) | Description                                                        | Variants               |
| --------------------------- | --------------------- | ------------------------------------------------------------------ | ---------------------- |
| `!ticker@arr`               | `/market/ws`          | 24h rolling ticker updates (last price, volume, percentage change) | All                    |
| `!bookTicker`               | `/public/ws`          | Best bid/ask price updates (skipped in publicDataOnly mode)        | All                    |
| `!markPrice@arr@1s`         | `/market/ws`          | Mark price, index price, funding rate at 1s intervals              | Futures, PM (not Spot) |
| `{symbol}@kline_{interval}` | `/market/ws`          | OHLCV candle updates (on-demand subscription)                      | All                    |
| `{symbol}@depth`            | `/public/ws`          | Orderbook depth updates (on-demand subscription)                   | All                    |

**Orderbook management**: Uses snapshot + incremental update pattern (`binance.ws-public.ts`):

1. Subscribe to `{symbol}@depth` stream
2. On first message, fetch REST snapshot via `/depth?limit=1000`
3. Buffer incoming updates during snapshot fetch in `orderBookInnerState[innerId].updates`
4. When snapshot arrives, filter buffered updates where `u > lastUpdateId` from snapshot
5. Apply filtered buffered updates to the snapshot
6. Mark snapshot as loaded; apply subsequent incremental updates directly
7. On each update, `calcOrderBookTotal()` before calling back. **No `sortOrderBook`** — `updateOrderBook` does sorted insertion via binary search and Binance REST `/depth` returns pre-sorted (bids DESC, asks ASC). See `../CLAUDE.md` → "Public WebSocket Hot-Path Patterns".

**On-demand subscriptions** (kline, depth):

- Use a `waitForConnectedAndSubscribe()` polling loop (100ms intervals) to wait until WebSocket is connected before subscribing.
- Track reference counts in `topics` map; only unsubscribe when count reaches zero.
- Kline subscriptions must cancel any pending wait-for-connect timer on unsubscribe. Rapid timeframe switching can otherwise leave delayed subscriptions behind after the caller already unsubscribed.
- Remove zero-ref kline topics from `topics` instead of leaving `0` counts, because reconnect replay subscribes every key in `topics`.
- Return an unsubscribe function for cleanup.

**Message routing**: The public WebSocket extracts the `e` (event type) and `s` (symbol) fields once via targeted `indexOf` against the raw frame and dispatches in three tiers: (1) broadcast handler keyed by event type alone (`24hrTicker`, `markPriceUpdate`, `bookTicker`); (2) per-symbol handler keyed by `${event}.${symbol}` (`depthUpdate.BTCUSDT`); (3) prefix iteration only when (1) and (2) miss, used for multi-segment keys like `kline.{interval}.{symbol}` whose interval lives in `k.i`. The shared `dispatchBinanceMessage` helper at the top of `binance.ws-public.ts` is reused by both `BinanceMarketWebsocket` and `BinancePublicWebsocket`.

**Hot-path lookups**: All three ticker handlers (`!ticker@arr`, `!markPrice@arr@1s`, `!bookTicker`) read tickers via `parent.store.tickersBySymbol[event.s]`. The previous `parent.store.tickers.find(...)` pattern was N×M = ~360K linear scans per `!ticker@arr` frame (~600 events × ~600 tickers). See `../CLAUDE.md` → "Public WebSocket Hot-Path Patterns".

### Private WebSocket Channels

Connection uses listenKey-based authentication (no WebSocket-level auth required).

#### Futures (`BinancePrivateWebsocket`)

| Event Type           | Description                                                                              |
| -------------------- | ---------------------------------------------------------------------------------------- |
| `ACCOUNT_UPDATE`     | Balance and position changes; triggers throttled position re-fetch                       |
| `ORDER_TRADE_UPDATE` | Regular order lifecycle (NEW, PARTIALLY_FILLED, FILLED, CANCELED, EXPIRED)               |
| `ALGO_UPDATE`        | Algo order lifecycle (NEW, TRIGGERED, TRIGGERING, CANCELED, EXPIRED, REJECTED, FINISHED) |

**ORDER_TRADE_UPDATE fields** (nested in `o` object):

- `i` = orderId, `s` = symbol, `S` = side (BUY/SELL), `ot` = order type
- `X` = status, `p` = price, `sp` = stop price, `ap` = average fill price
- `q` = original quantity, `z` = filled quantity, `l` = last fill quantity
- `R` = reduceOnly, `wt` = working type (MARK_PRICE/CONTRACT_PRICE)

**ALGO_UPDATE fields** (nested in `o` object):

- `aid` = algoId, `caid` = clientAlgoId, `s` = symbol, `S` = side
- `o` = order type, `X` = status, `tp` = trigger price, `q` = quantity
- `R` = reduceOnly, `cp` = closePosition, `wt` = working type

**Throttled order refresh** (`binance.ws-private.ts:267-290`): After any order or algo event, `throttledFetchOrders()` is called. This uses a 10-second debounce (`clearTimeout`/`setTimeout`) to batch multiple rapid events into one REST fetch. The REST fetch replaces the entire order store to ensure consistency.

**Throttled position fetch** (`binance.ws-private.ts:211-252`): Account update events trigger `throttledFetchPositions()` with 500ms throttle. Uses a scheduling pattern:

- If enough time has passed since last fetch (>500ms), execute immediately.
- Otherwise, schedule a delayed fetch for the remaining time.
- Uses `hasPendingPositionEvents` flag to ensure events during throttle delay are not lost.
- `positionFetchScheduled` prevents duplicate scheduled fetches.
- Only one position repair may be in flight. Each WS position event increments
  a version; a REST response started before a newer event is discarded and a
  fresh repair is scheduled, so stale REST cannot overwrite the immediate WS
  state.
- The fetch result from `/positionRisk` must be committed with
  `store.addOrUpdatePositionsFull()`. Calling `fetchPositions()` without
  storing its return value drops the only Binance field that carries
  liquidation price between the ~80s full-risk ticks.

#### Spot (`BinancePrivateWebsocket`)

| Event Type                | Description               |
| ------------------------- | ------------------------- |
| `outboundAccountPosition` | Balance changes per asset |
| `executionReport`         | Order lifecycle events    |

**Spot-specific**: Uses `data.c` (clientOrderId) as order ID (not `data.i` like futures). Order events use the same nested `o` object pattern as futures (`events.forEach(({ o: data })`).

**Spot order-type mapping.** Spot `executionReport` reports the order type in the `o` field (values `LIMIT`/`MARKET`/`STOP_LOSS`/`STOP_LOSS_LIMIT`/`TAKE_PROFIT`/`TAKE_PROFIT_LIMIT`/`LIMIT_MAKER`) — **not** the futures `ot` field or futures values. `handleOrderEvents` maps via `ORDER_TYPE_SPOT` (in `binancespot.types.ts`); `LIMIT_MAKER` → `OrderType.Limit`, both `STOP_LOSS*` → `StopLoss`, both `TAKE_PROFIT*` → `TakeProfit`. Spot has no `reduceOnly` (kept `false`).

**Spot execution-type vs order-status.** In `executionReport`, `x` is the **execution type** (`NEW`/`TRADE`/`CANCELED`/…) and `X` is the **order status** (`NEW`/`PARTIALLY_FILLED`/`FILLED`/`CANCELED`/…). Fills arrive with `x === 'TRADE'` (the handler previously checked `X === 'TRADE'`, which never matched). On a `TRADE` the handler emits `fill` **and** mirrors the cumulative fill onto the store row (`filled = z`, `remaining = q - z`), transitioning to `Closed` when `X === 'FILLED'` / `z === q`. Without the store update a filled spot order stayed `Open` until the next REST `fetchOrders` — same class of bug the futures `PARTIALLY_FILLED` note describes.

#### Portfolio Margin (`BinancePMPrivateWebsocket`)

| Event Type                       | Description                                                                                                              |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `ACCOUNT_UPDATE`                 | Balance and position changes (same as futures)                                                                           |
| `ORDER_TRADE_UPDATE`             | Regular order events (uses `clientOrderId` via `data.c`, not `orderId`)                                                  |
| `CONDITIONAL_ORDER_TRADE_UPDATE` | Conditional order events (nested in `so` object, uses `data.c` as ID, `data.st` for strategy type, `data.os` for status) |

**PM-specific**: Position side in `ACCOUNT_UPDATE` is determined by `p.pa > 0` (long) vs `p.pa <= 0` (short), with a `p.ps === 'BOTH'` check for one-way mode (`binancepm.ws-private.ts:362-364`). PM does NOT have the `throttledFetchOrders` debounce that futures has.

### ListenKey Management

All variants use the same pattern:

1. **Create**: `POST /listenKey` returns `{ listenKey: "..." }`
2. **Keepalive**: `PUT /listenKey` every 30 minutes via `setTimeout`
3. ListenKey expires after 60 minutes if not renewed
4. Keepalive is self-rescheduling: each successful `PUT` schedules the next keepalive

### Ping/Pong

WebSocket health is monitored by two independent mechanisms:

1. **Ping/Pong via `LIST_SUBSCRIPTIONS`**: A `LIST_SUBSCRIPTIONS` command (`{ id: 42, method: 'LIST_SUBSCRIPTIONS' }`) is sent as a ping. Response with `id: 42` is treated as pong. On pong, `handlePongEvent()` schedules the next ping in 5 seconds and sets a pong miss timeout at `1.5 * 5000 = 7,500ms`. If pong is not received within 7.5 seconds, the WebSocket triggers a reconnection.
2. **Last received check via `checkLastReceived()`**: `trackLastReceived()` is called on every incoming message. A polling loop (every 1 second) calls `checkLastReceived()`, which triggers reconnection if no message of any kind has been received within `WS_RECONNECT_THRESHOLD_MS` (20 seconds).

### Reconnection Logic

Handled by `BaseWebSocket` (parent class in `base.ws.ts`):

- **Pong miss timeout**: If pong not received within 7.5 seconds of the last pong, calls `reconnect()`.
- **Last received timeout**: If no message of any kind received within `WS_RECONNECT_THRESHOLD_MS` (20s), `checkLastReceived()` calls `reconnect()`.
- **Close handler** (`onClose`): Cleans up event listeners, waits a minimum reconnection delay, then calls `connectAndSubscribe()` to re-establish the connection. Uses `reconnecting` flag to prevent concurrent reconnection attempts. Has a hard limit of 100 `onClose` calls before it logs and stops reconnecting (returns early without re-establishing the connection).
- **Send failure**: If `ws.send()` fails because WebSocket is not in OPEN state, triggers reconnection.
- **ReconnectingWebSocket**: If the underlying WebSocket is a `ReconnectingWebSocket` instance, uses its built-in `reconnect()` method; otherwise, closes the native WebSocket and lets `onClose` handle reconnection.

## Order Types and Mappings

### Type Mappings

| Binance API            | Internal Type                | Order API                   |
| ---------------------- | ---------------------------- | --------------------------- |
| `LIMIT`                | `OrderType.Limit`            | Regular (`/fapi/v1/order`)  |
| `MARKET`               | `OrderType.Market`           | Regular (`/fapi/v1/order`)  |
| `STOP_MARKET`          | `OrderType.StopLoss`         | Algo (`/fapi/v1/algoOrder`) |
| `TAKE_PROFIT_MARKET`   | `OrderType.TakeProfit`       | Algo (`/fapi/v1/algoOrder`) |
| `TRAILING_STOP_MARKET` | `OrderType.TrailingStopLoss` | Algo (`/fapi/v1/algoOrder`) |

### Time In Force

| Binance | Internal            |
| ------- | ------------------- |
| `GTC`   | `GoodTillCancel`    |
| `IOC`   | `ImmediateOrCancel` |
| `FOK`   | `FillOrKill`        |
| `GTX`   | `PostOnly`          |

### Stop Trigger Types

| Binance          | Internal                         |
| ---------------- | -------------------------------- |
| `MARK_PRICE`     | `OrderStopTriggerType.MarkPrice` |
| `CONTRACT_PRICE` | `OrderStopTriggerType.LastPrice` |

## Order Lifecycle

### Order Placement Architecture (Futures)

Orders are split into **regular** (limit/market) and **algo** (SL/TP/trailing) categories via `isAlgoOrderType()` (`binance.exchange.ts:1263-1265`):

**`formatCreateOrder()` flow** (`binance.exchange.ts:1267-1442`):

1. Look up market by symbol (O(1) hashmap).
2. If trailing stop, delegate to `formatCreateTrailingStopLossOrder()`.
3. Determine `positionSide` via `getOrderPositionSide()` -- BOTH for one-way, LONG/SHORT for hedged with side flipping for reduce-only/SL/TP orders.
4. Apply precision: `adjust(amount, pAmount)` and `adjust(price, pPrice)`.
5. For SL/TP: build algo order payload with `algoType: 'CONDITIONAL'`, `triggerPrice`, `closePosition` or `quantity + reduceOnly`.
6. For limit/market: build regular order payload. If amount exceeds `market.limits.amount.max`, split into lots.
7. If `opts.stopLoss` or `opts.takeProfit` attached (bracket order), add additional algo payloads with flipped side and `reduceOnly`.

**Regular orders** (`placeRegularOrderBatch()`, `binance.exchange.ts:1505-1543`):

- Placed via `/fapi/v1/order` (single) or `/fapi/v1/batchOrders` (up to 5 per batch)
- Uses `unlimitedXHR` (bypasses rate limiting)
- Batch orders are serialized via `JSON.stringify(lot)` in `batchOrders` param
- Batch response items with `code` field are logged as errors, successful ones yield `orderId`
- Returns `orderId` from exchange response (not `clientOrderId`) to match WebSocket events

**Algo orders** (`placeAlgoOrders()`, `binance.exchange.ts:1546-1561`):

- Placed via `/fapi/v1/algoOrder` one at a time (no batch support)
- Require `algoType: 'CONDITIONAL'`
- Use `triggerPrice` instead of `price`/`stopPrice`
- Support `closePosition` (close entire position) or `quantity` + `reduceOnly`
- Returns `algoId` from exchange response
- `clientAlgoId` from website-placed orders is unreliable for cancellation; always use `algoId`

**closePosition orders** (`cancelExistingClosePositionOrders()`, `binance.exchange.ts:1194-1233`):

- Binance allows only ONE closePosition order per direction per symbol
- Before placing a new closePosition order, **fresh orders are fetched from REST** (not from local store, which may be stale due to throttled WebSocket updates)
- Filters for existing closePosition orders with matching symbol, type, side, and `close === true && !amount`
- Cancels any conflicting orders before placing the new one
- If `closePosition` requested but no position exists and no amount specified, throws an error

**Bracket orders** (entry + SL + TP):

- Entry goes as regular order
- SL and TP are placed as algo orders with `quantity` + `reduceOnly` (not `closePosition`, since position doesn't exist yet)
- In hedge mode, `reduceOnly` is not sent (hedge mode uses `positionSide` instead)

**Trailing stop loss** (`formatCreateTrailingStopLossOrder()`, `binance.exchange.ts:1444-1481`):

- Requires an existing position (throws if not found)
- Calculates `callbackRate` as percentage distance from current last price to trigger price
- Rounds to one decimal place: `Math.round(((priceDistance * 100) / ticker.last) * 10) / 10`
- Sets `priceProtect: 'TRUE'`
- Uses full position size as quantity

### Order Placement Architecture (Portfolio Margin)

PM splits orders differently (`binancepm.exchange.ts:878-935`):

- **Regular orders** (LIMIT, MARKET): Placed via `/papi/v1/um/order`
- **Conditional orders** (STOP_MARKET, TAKE_PROFIT_MARKET, TRAILING_STOP_MARKET): Placed via `/papi/v1/um/conditional/order` with `strategyType` instead of `type`
- Conditional orders that have `closePosition` but no quantity attempt to look up the position size from the store (`binancepm.exchange.ts:894-904`)
- Uses `newClientOrderId` (not exchange-assigned orderId) as the order ID
- No batch order support -- all orders placed one at a time

### Order Cancellation

**Futures** (`binance.exchange.ts:1042-1130`):

1. Split orders into regular and algo based on `isAlgoOrderType()`.
2. **Regular orders**: Group by symbol. Single cancel via `DELETE /fapi/v1/order` with `orderId`. Batch cancel via `DELETE /fapi/v1/batchOrders` with `orderIdList` (max 10 per batch, processed via `forEachSeries`).
3. **Algo orders**: Cancel one at a time via `DELETE /fapi/v1/algoOrder` with `algoId` parameter.
4. Error code `-2011` ("Unknown order sent") is treated as success (order already cancelled).
5. Batch cancel constructs `orderIdList` as raw string `[123,456]` (not parsed with `parseInt`) to avoid precision loss on large IDs (`binance.exchange.ts:1074`).

**Portfolio Margin** (`binancepm.exchange.ts:626-673`):

- Groups by symbol, cancels one at a time via `forEachSeries`.
- Conditional orders (SL/TP/trailing): `DELETE /papi/v1/um/conditional/order` with `newClientStrategyId`.
- Regular orders: `DELETE /papi/v1/um/order` with `origClientOrderId`.
- No `-2011` silent error handling.

**Spot** (`binancespot.exchange.ts:380-406`):

- Individual cancel via `DELETE /api/v3/order` with `origClientOrderId`.
- No batch cancellation support.

### Order Updates

All variants use cancel-and-recreate pattern (no native edit endpoint used). The `updateOrder()` method:

1. Builds a new order payload from the existing order with updated price/amount.
2. Preserves `tpTriggerType` and `slTriggerType` if present (futures and PM).
3. Calls `cancelOrders([order])`.
4. Calls `placeOrder(newOrder)`.

## Symbol Format

- **Futures/PM**: `BTCUSDT`, `ETHUSDC` (base + quote concatenated)
- **Market ID format**: `{base}/{quote}:{margin}` (e.g., `BTC/USDT:USDT`)
- **Spot**: `BTCUSDT` (base + quote concatenated)
- **Market ID format (spot)**: `{base}/{quote}` (e.g., `BTC/USDT`)
- For futures/PM, `fetchMarkets` accepts `contractType` of either `PERPETUAL` (crypto perps) or `TRADIFI_PERPETUAL` (TradFi perps — gold, silver, single-name equities, ETFs), with USDT or USDC margin. **The wire value is literally `TRADIFI_PERPETUAL` with the typo'd extra "I" — Binance ships it that way; do not "correct" it to `TRADFI_PERPETUAL`.** TradFi symbols (`XAUUSDT`, `TSLAUSDT`, `SPYUSDT`, …) flow through the same `/fapi/v1/*` endpoints, the same WS streams, and the same algo/closePosition/leverageBracket code paths as crypto perps. The user must have signed the one-time TradFi-Perps agreement on Binance (`POST /fapi/v1/stock/contract`) — we don't call this endpoint, the user has to opt in via Binance's own UI before our orders will work.
- Spot loads all symbols from `exchangeInfo` without filtering

## Data Precision & Conversion

### Price and Amount Precision

- **Futures/PM**: Precision values come from symbol filters:
  - `PRICE_FILTER.tickSize` -> `market.precision.price` (e.g., `0.10` for BTCUSDT)
  - `LOT_SIZE.stepSize` -> `market.precision.amount` (e.g., `0.001`)
  - Minimum amount is `max(LOT_SIZE.minQty, MARKET_LOT_SIZE.minQty)`
  - Maximum amount is `min(LOT_SIZE.maxQty, MARKET_LOT_SIZE.maxQty)`
- **Spot**: Uses `baseAssetPrecision` and `quoteAssetPrecision` from exchangeInfo, converted to step size via `1 / precision` (e.g., precision 8 -> step `0.00000001`). Limits are hardcoded to min=0, max=999_999_999.

### Rounding

The `adjust()` utility from `../../utils/safe-math` is used for rounding prices and amounts to exchange precision before sending orders. It aligns values to the step size (e.g., `adjust(1.23456, 0.01)` -> `1.23`).

### Big Integer Handling

- **Futures**: Uses `bigIntAsStringJsonParse` for all REST responses (`binance.api.ts:30`). This parser detects integers that exceed `Number.MAX_SAFE_INTEGER` and converts them to strings. Critical for `orderId` fields.
- **PM**: Does NOT use `bigIntAsStringJsonParse` -- uses standard `jsonParse` in WebSocket handlers (`binancepm.ws-private.ts:119`).
- **Spot**: Does NOT use `bigIntAsStringJsonParse` in API layer (`binancespot.api.ts`).

### Candle Timestamps

All OHLCV data from Binance arrives in milliseconds. Converted to seconds via `time / 1000` before returning (`binance.exchange.ts:930`).

`listenOHLCV` for futures seeds the realtime callback with the latest REST candle (`fetchOHLCV({ limit: 1 })`) before waiting for the first WebSocket kline frame. Keep this seed lightweight and disposable-safe: ignore the result after unsubscribe/dispose, and route through `externalPublicDataProvider.subscribeCandle()` when external public data is configured.

### Notional Calculation

- **Futures**: `Math.abs(contracts * entryPrice + upnl)` -- includes unrealized PnL in notional to correct for shorts (`binance.exchange.ts:596`).
- **PM**: `Math.abs(parseFloat(p.notional))` -- uses the exchange-provided notional directly (`binancepm.exchange.ts:331`).
- **Spot**: `Math.abs(contracts) * entryPrice + upnl` (`binancespot.exchange.ts:297`).

## Rate Limiting

| Variant     | Rate Limit             | Details                                                                            |
| ----------- | ---------------------- | ---------------------------------------------------------------------------------- |
| **Futures** | 3 RPS                  | Applied to primary API client via `axios-rate-limit` (`binance.exchange.ts:93-96`) |
| **Spot**    | 3 RPS                  | Applied via `axios-rate-limit` (`binancespot.exchange.ts:64`)                      |
| **PM**      | No explicit rate limit | Both `papiXhr` and `fapiXhr` are unthrottled (`binancepm.exchange.ts:72-74`)       |

All variants use an additional `unlimitedXHR` for order placement (bypasses rate limiting).

Network errors are retried up to 3 times via `axios-retry` (condition: `isNetworkError` only, not HTTP errors).

The `recvWindow` parameter defaults to `20_000` ms (20 seconds) for all variants, overridable via `options.extra.recvWindow`.

### Income History Rate Limit

The `/fapi/v1/income` endpoint is rate-limited to 30 requests per minute with max 1000 records per request and 7-day time window per request. The `fetchIncomeHistoryRange` method (`binance.exchange.ts:845-900`) handles chunking across longer ranges and pagination within 7-day windows. A 100ms delay between chunks helps respect rate limits.

## State Reconciliation

### Position Reconciliation

**WebSocket partial updates**: `ACCOUNT_UPDATE` events provide real-time position changes (entry price, contracts, unrealized PnL). They are upserted immediately via `store.addOrUpdatePosition()`, including brand-new positions; one-way `ps=BOTH` events derive long/short from the signed amount. Unknown zero-size rows are ignored, while a zero update for an existing row closes it immediately. Every event containing position rows schedules a full `/positionRisk` repair because the WS payload omits leverage, margin type, and liquidation price.

**REST full snapshots**: Two REST endpoints provide position data at different detail levels:

1. **`/fapi/v2/account`** (every 5s via `tick()`): Returns positions with entry price, contracts, unrealized PnL, leverage, isolated flag. Does NOT include liquidation price or adlQuantile.
2. **`/fapi/v2/positionRisk`** (every ~80s, at `count % 16 === 0`, and after WS account-update throttled refreshes): Returns full position risk data including liquidation price, margin type, max notional, and adlQuantile.

**Merge strategy** (`binance.exchange.ts:574-607`): When processing `/fapi/v2/account`, the code finds the existing position in the store by ID and spreads it (`...foundPosition`) before overwriting with fresh data. This preserves fields from `/positionRisk` (like liquidation price) that are not present in `/account` responses.

**Stale snapshot guard**: Every WS position event advances a shared position-state version. Both event-triggered `/positionRisk` repairs and periodic `/account`/`positionRisk` snapshots capture that version before requesting; if a newer WS event arrives before the response, its position rows are discarded. `/account` balance data and loaded flags still commit because they are independent of the stale position rows.

### Order Reconciliation

**WebSocket real-time updates**: `ORDER_TRADE_UPDATE` and `ALGO_UPDATE` events apply immediate order status changes (NEW, FILLED, CANCELED, etc.) to the store.

**Throttled REST refresh** (`binance.ws-private.ts:267-290`): After any order/algo WebSocket event, a 10-second debounced `fetchOrders()` re-fetches all open orders from REST and replaces the store's order list. This ensures any missed events or race conditions are corrected.

**PM variant**: Does NOT have the throttled order refresh mechanism. Relies solely on WebSocket events for order state.

**PARTIALLY_FILLED must mirror cumulative fill onto the store row.** Binance's `ORDER_TRADE_UPDATE` carries the cumulative filled qty in `z` (and last fill qty in `l`). On `PARTIALLY_FILLED` / `FILLED`, `handleOrderEvents` (futures and PM regular) emits a `fill` event AND calls `store.updateOrder({ id }, { filled: z, remaining: q - z, lastUpdate })`. Without that store update, the only fill signal is the emitted event — which the chaser supervisor in `webworker/chaserOrder/` only subscribes to for `hyperliquid`/`grvt`. For every other exchange (including Binance) the chaser reads `order.filled` from the store. If the store row still shows `filled: 0` from the NEW event, the chaser will cancel and re-place a moved-away order at the **full original size** instead of the remaining size. The 10s `throttledFetchOrders` debounce is far too coarse to plug this — the race fires inside a single price-chase tick. Don't strip the per-event `updateOrder` thinking it's redundant with the throttled REST refresh.

### Balance Reconciliation

**Futures** (`binance.exchange.ts:412-563`): Aggregates per-asset balances from `/fapi/v2/account`:

- **Stablecoins** (USDT, USDC, BUSD, FDUSD, BFUSD, LDUSDT, BNFCR): 1:1 USD value.
- **Non-stablecoins**: Converted to USD using market tickers (direct pair lookup, or BTC routing).
- Logs balance aggregation details once on first fetch (`hasLoggedBalanceAggregation` flag).
- Falls back to top-level `totalWalletBalance`/`availableBalance` if aggregation throws.

**PM**: Fetches from `/papi/v1/balance`, aggregates only USDT, USDC, and FDUSD assets. Uses `totalWalletBalance`, `crossMarginFree`, and `umUnrealizedPNL`.

## Exchange Metadata

### Binance Futures (`binance`)

```
enabled: true
supportsTestnet: true
supportsTrailingStopLoss: true
supportsChaser: true
supportsTPSLCloseAttribute: true
supportsTunnel: true
positionModes: [OneWay, Hedged]  (default)
marginTypes: [Cross, Isolated]  (default)
supportedTPSLTriggerTypes: [LastPrice, MarkPrice]
defaultTPTriggerType: LastPrice
defaultSLTriggerType: MarkPrice
doesNotSupportOpenInterest: true
alwaysCalculateUnrealizedPnlFromLastPrice: true
tickInterval: 500
latencyDomain: fapi.binance.com
orderbookTickDelay: 60
supportsTradesHistory: true
```

### Binance Portfolio Margin (`binancepm`)

```
enabled: true
supportsTestnet: true
supportsTrailingStopLoss: true
supportsChaser: true
supportsTunnel: true
hideMarginTypeSelector: true
marginTypes: [Cross]
supportedTPSLTriggerTypes: [LastPrice, MarkPrice]
defaultTPTriggerType: LastPrice
defaultSLTriggerType: MarkPrice
doesNotSupportOpenInterest: true
alwaysCalculateUnrealizedPnlFromLastPrice: true
tickInterval: 500
latencyDomain: papi.binance.com
orderbookTickDelay: 60
supportsTradesHistory: true
```

### Binance Spot (`binancespot`)

```
enabled: false (enabledInDev: true)
supportsTestnet: true
supportsChaser: true
doesNotSupportOpenInterest: true
latencyDomain: api.binance.com
orderbookTickDelay: 60
supportsTradesHistory: true
```

## Testnet / Demo

### URL Differences

| Variant   | Livenet REST               | Testnet REST                        |
| --------- | -------------------------- | ----------------------------------- |
| Futures   | `https://fapi.binance.com` | `https://testnet.binancefuture.com` |
| PM (papi) | `https://papi.binance.com` | `https://testnet.binancefuture.com` |
| PM (fapi) | `https://fapi.binance.com` | `https://testnet.binancefuture.com` |
| Spot      | `https://api.binance.com`  | `https://testnet.binance.vision`    |

| Variant | Livenet WS (public + market)                                                  | Testnet WS (public + market)                                                            |
| ------- | ----------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Futures | `wss://fstream.binance.com/public/ws` + `wss://fstream.binance.com/market/ws` | `wss://stream.binancefuture.com/public/ws` + `wss://stream.binancefuture.com/market/ws` |
| Spot    | `wss://stream.binance.com:9443/ws`                                            | `wss://testnet.binance.vision/ws`                                                       |

### Behavioral Differences

- **Listen key signing**: On livenet, listen key POST/PUT requests are NOT signed. On testnet, they ARE signed (the `!options.testnet` check in the interceptor skips the bypass; `binance.api.ts:83`).
- **Symbol availability**: Testnet typically has a much smaller set of symbols.
- **PM testnet**: Both papi and fapi use the same testnet base URL (`testnet.binancefuture.com`), so the PM-specific papi endpoints may not be fully supported on testnet.

## Known Quirks / Implementation Notes

### Big Integer Order IDs

Binance order IDs can exceed JavaScript's `Number.MAX_SAFE_INTEGER`. All responses use `bigIntAsStringJsonParse` to convert big integers to strings. When constructing `orderIdList` for batch cancellation, IDs are joined as raw strings (not parsed with `parseInt`) to avoid precision loss.

### Error Handling in 200 Responses

Binance can return errors with HTTP 200 status codes (negative `code` field). The API interceptor (`binance.api.ts:126-152`) checks for `response.data.code < 0` and converts these to rejected promises. Specific handling:

- Error code `-2011` ("Unknown order sent") is in the `SILENT_ERROR_CODES` list and does not emit UI errors.
- All other negative codes are emitted via `exchange.emitAndLogError()` AND rejected.
- Non-200 responses: The error interceptor checks `error.response.data.msg` and emits if not in silent list.

### closePosition Constraint

Binance allows only ONE `closePosition` order per direction per symbol.
Before placing a new closePosition SL/TP, `cancelExistingClosePositionOrders`
filters **the local store** for conflicting "infinite" (size-less)
closePosition rows and cancels them.

**Do NOT reintroduce a `fetchOrders()` here.** An earlier version of this
helper did a REST `fetchOrders` per placement (commit `d7274bff31`) because
stale store IDs were causing cancel attempts to fail with `-2011 Unknown
order sent`. 21 minutes later (`e343d5b717`) `cancelOrders` was taught to
treat `-2011` as a successful cancel, so the REST roundtrip became
redundant — and at the scale of many accounts on one IP it was a major
contributor to Binance's 2400 RPM IP rate limit (each call = 2 parallel
REST requests, amplified per closePosition placement). The local store is
kept current by the WS `ORDER_TRADE_UPDATE` / `ALGO_UPDATE` NEW/CANCELED
events; in the rare case the store is missing or stale, `cancelOrders`'s
silent `-2011` handling absorbs the discrepancy.

### Algo vs Regular Order Separation (Futures)

As of 2025-12-09, stop/TP/trailing orders must use the Algo Order API. The `isAlgoOrderType()` helper determines routing. This affects both placement and cancellation endpoints. The comment in `binance.types.ts:42-43` documents this migration date.

### Standalone Binance WebSocket Consumers Outside `safe-cex`

A few UI components open their own Binance USDⓈ-M futures WebSockets directly (bypassing the exchange layer). These must use the post-2026-04-23 category-prefixed URLs the same way the exchange layer does:

- `apps/web/src/components/lightweightCharts/TicksChart.tsx` — subscribes to `{symbol}@aggTrade` and must use `wss://fstream.binance.com/market/ws/...` (aggTrade is a market-data stream).
- `v2/frontend/src/modules/ticksChart/TicksChartBinance.tsx` — same component in the legacy V2 frontend; same `/market/ws/` rule applies.

If you add a new direct subscription, route it by category: `bookTicker`/`@depth` → `/public/ws`, everything else (kline, ticker, markPrice, aggTrade, trade, forceOrder…) → `/market/ws`.

### Portfolio Margin Fake Positions

PM only returns positions with non-zero size from the API. The implementation creates fake zero-size positions for all loaded markets (both long and short) and deduplicates with `uniqBy(..., 'id')` to ensure the UI shows all available markets (`binancepm.exchange.ts:343-365`).

### Portfolio Margin Conditional Orders

PM uses a different naming convention than futures:

- `strategyType` instead of `type` for conditional orders
- `strategyId` / `newClientStrategyId` instead of `algoId` / `clientAlgoId`
- WebSocket event is `CONDITIONAL_ORDER_TRADE_UPDATE` (data in `so` object) vs `ALGO_UPDATE` (data in `o` object)
- Status field is `data.os` instead of `data.X`
- Order type field is `data.st` instead of `data.o`

### Portfolio Margin Margin Type

Calling `changeMarginType()` on PM always throws an error with "Portfolio margin is not supported" (`binancepm.exchange.ts:595-598`). PM accounts only support cross margin. The metadata has `hideMarginTypeSelector: true` to hide the UI control.

### Spot Limitations

- Order type in the WS handler is now mapped from the spot `o` field via `ORDER_TYPE_SPOT` (see "Spot order-type mapping" below) — previously hardcoded to `OrderType.Limit`
- No mark price or funding rate data (uses last price for mark and index, `binancespot.exchange.ts:239-240`)
- No hedge mode or margin type management
- No position risk or leverage endpoints
- Production disabled (`enabled: false`), only available in dev
- Uses `fapi/v2/account` endpoint for balance/positions (may not be correct for spot; `binancespot.types.ts:21`)
- Account event handler (`outboundAccountPosition`) treats all positions as Long side (`binancespot.ws-private.ts:104`)
- No batch order support -- places orders one at a time in a loop (`binancespot.exchange.ts:604-613`)
- No `bigIntAsStringJsonParse` in API layer

### Spot Precision Calculation

Spot computes precision differently from futures: `1 / parseFloat(v(m, 'baseAssetPrecision'))` where `baseAssetPrecision` is an integer like 8, resulting in a step size of `0.00000001` (`binancespot.exchange.ts:202-203`). This is inverted from futures which reads `stepSize` directly from filters.

### Tealstreet Tunnel Support

Both futures and PM support the Tealstreet tunnel (`supportsTunnel: true`):

- **Futures**: Uses `bigIntAxiosHttpOverWebsocketAdapter` (preserves big int handling) (`binance.api.ts:50-52`, `115-117`).
- **PM**: Uses `axiosHttpOverWebsocketAdapter` (standard adapter) (`binancepm.api.ts:71-73`).
- **Spot**: No tunnel support.

### TODO Notes Found in Code

- `binance.exchange.ts:67`: `// TODO: translate our err messages`
- `binancepm.exchange.ts:57`: `// TODO: translate our err messages`
- `binancespot.exchange.ts:51`: `// TODO: translate our err messages`
- (resolved) `binancespot.ws-private.ts` order-type hardcode — now mapped from the `o` field
- `binance.ws-public.ts:43`: `// TODO: maybe something better here...` -- regarding publicDataOnly bookTicker skip
- `binance.types.ts:39`: `// migrating to v3 requires fake positions as in portfolio margin implementation` -- regarding POSITION_RISK endpoint

### Open Interest

All three variants set `doesNotSupportOpenInterest: true` in metadata, and `openInterest: 0` is hardcoded in ticker construction. The code comments note "Binance doesn't provides all tickers data" for this field.

### Tick Polling

Futures and PM tick every 5 seconds. Full position risk data (via `/positionRisk`) is re-fetched every 16th tick (~80 seconds) to get detailed info like liquidation price, and is also committed after private WS account-update throttled position refreshes. The intervening ticks use `/account` (futures) or `/balance` (PM) for lighter updates. Spot uses `options.extra?.tickInterval` for its interval.

## Error Handling

### Error Codes Handled

| Code                | Message              | Behavior                                                                                            |
| ------------------- | -------------------- | --------------------------------------------------------------------------------------------------- |
| `-2011`             | "Unknown order sent" | Silent success in cancel flows -- order already gone (`binance.exchange.ts:1089-1096`, `1116-1119`) |
| Any negative `code` | Various              | Converted from 200 response to rejected promise (`binance.api.ts:130-141`)                          |

### Retry Strategy

- **Network errors**: Retried up to 3 times via `axios-retry` with `isNetworkError` condition (all variants).
- **API errors**: NOT retried. Errors from the exchange (non-2xx or 200-with-negative-code) are propagated immediately.
- **WebSocket position fetch failure**: Silent catch, will retry on next event (`binance.ws-private.ts:270-280`).
- **WebSocket order fetch failure**: Silent catch, will retry on next order event (`binance.ws-private.ts:285-288`).

### Error Propagation Pattern

Most methods follow this pattern:

1. Try the operation.
2. On catch, call `this.emitAndLogError(err?.response?.data?.msg || err?.message)` to display in UI.
3. For critical operations (order placement, cancellation): re-throw the error.
4. For data-fetching operations (tickers, positions, balance): return current store state as fallback.

### Batch Order Error Handling

For batch order placement via `/fapi/v1/batchOrders`, each item in the response array is checked individually. Items with a `code` field are logged as errors; items without are treated as successful (`binance.exchange.ts:1527-1534`).

## Testing / Validation

- [ ] Account validation with API keys (primary and secondary for futures)
- [ ] Balance display with multi-asset aggregation (stablecoins + non-stablecoins)
- [ ] Market loading (PERPETUAL + USDT/USDC only)
- [ ] Ticker updates via WebSocket (24h, bookTicker, markPrice)
- [ ] Order placement: limit, market, stop loss, take profit, trailing stop
- [ ] Bracket orders (entry + SL + TP)
- [ ] closePosition orders (only one per direction per symbol)
- [ ] Algo order cancellation (individual via algoId)
- [ ] Batch regular order cancellation (up to 10 per batch)
- [ ] Order updates via cancel-and-recreate
- [ ] Position mode switching (One-Way / Hedged)
- [ ] Margin type switching (Cross / Isolated)
- [ ] Leverage adjustment
- [ ] Orderbook depth with snapshot + incremental updates
- [ ] OHLCV candle data via REST and WebSocket
- [ ] Public trades fetch
- [ ] Income history fetch with pagination
- [ ] ListenKey keepalive (30-minute interval)
- [ ] WebSocket reconnection handling
- [ ] Testnet/livenet switching
- [ ] Big integer order ID handling
- [ ] Portfolio Margin fake positions
- [ ] Portfolio Margin conditional order placement and cancellation

## Variant Differences Summary

| Feature                 | Futures                                               | Portfolio Margin                                                         | Spot                                         |
| ----------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------ | -------------------------------------------- |
| Base URL                | `fapi.binance.com`                                    | `papi.binance.com` + `fapi.binance.com`                                  | `api.binance.com`                            |
| Exchange class          | `BinanceExchange`                                     | `BinancePMExchange`                                                      | `BinanceSpotExchange`                        |
| Order API               | Regular + Algo                                        | Regular + Conditional (strategy)                                         | Regular only                                 |
| Order ID type           | `orderId` / `algoId`                                  | `clientOrderId` / `newClientStrategyId`                                  | `clientOrderId`                              |
| Stop orders             | Algo API (`/fapi/v1/algoOrder`)                       | Conditional API (`/papi/v1/um/conditional/order`)                        | `stopPrice` field on regular order           |
| Margin types            | Cross + Isolated                                      | Cross only                                                               | N/A                                          |
| Fake positions          | No                                                    | Yes (for all markets)                                                    | No                                           |
| Balance fetch           | Aggregated per-asset                                  | Per-asset (USDT/USDC/FDUSD)                                              | Top-level fields                             |
| Private WS events       | `ACCOUNT_UPDATE`, `ORDER_TRADE_UPDATE`, `ALGO_UPDATE` | `ACCOUNT_UPDATE`, `ORDER_TRADE_UPDATE`, `CONDITIONAL_ORDER_TRADE_UPDATE` | `outboundAccountPosition`, `executionReport` |
| Private WS path         | `/ws/{listenKey}`                                     | `/pm/ws/{listenKey}`                                                     | `/ws/{listenKey}`                            |
| Mark price stream       | Yes                                                   | Yes                                                                      | No (uses last price)                         |
| Secondary key           | Supported                                             | Supported (validates both)                                               | Not supported                                |
| Income history          | Supported                                             | Not implemented                                                          | Not implemented                              |
| Public trades           | Supported                                             | Not implemented                                                          | Not implemented                              |
| Executions              | Supported                                             | Not implemented                                                          | Not implemented                              |
| Tunnel adapter          | `bigIntAxiosHttpOverWebsocketAdapter`                 | `axiosHttpOverWebsocketAdapter`                                          | Not supported                                |
| BigInt JSON parse       | Yes (API + WS)                                        | No                                                                       | No                                           |
| Throttled order refresh | 10s debounce                                          | No                                                                       | No                                           |
| Batch order placement   | Yes (5 per batch)                                     | No (one at a time)                                                       | No (one at a time)                           |
| Batch order cancel      | Yes (10 per batch)                                    | No (one at a time)                                                       | No (one at a time)                           |
