# Binance Exchange Integration

This directory contains four Binance exchange variants for Tealstreet: **USD-M Futures**, **COIN-M Inverse Futures**, **Spot**, and **Portfolio Margin**. All variants share the same directory rather than having separate subdirectories.

## Exchange Information

| Property           | Futures (`binance`) | Inverse (`binanceinverse`)          | Portfolio Margin (`binancepm`) | Spot (`binancespot`)      |
| ------------------ | ------------------- | ----------------------------------- | ------------------------------ | ------------------------- |
| Exchange class     | `BinanceExchange`   | `BinanceInverseExchange`            | `BinancePMExchange`            | `BinanceSpotExchange`     |
| Pretty name        | Binance             | Binance Inverse                     | Binance Portfolio Margin       | Binance Spot              |
| Production enabled | Yes                 | No (`enabledInDev: true`, preview off) | Yes                            | No (`enabledInDev: true`) |
| Testnet support    | Yes                 | Yes                                 | Yes                            | Yes                       |

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

### COIN-M Inverse Futures (`exchangeName: 'binanceinverse'`)

| File                           | Description                                                                                 |
| ------------------------------ | ------------------------------------------------------------------------------------------- |
| `binanceinverse.api.ts`        | COIN-M REST API client using `dapi.binance.com` / `demo-dapi.binance.com`                  |
| `binanceinverse.exchange.ts`   | COIN-M exchange class (`BinanceInverseExchange`) with inverse market, balance, and order mapping |
| `binanceinverse.types.ts`      | COIN-M endpoint constants, WebSocket hosts, and shared mappings                             |
| `binanceinverse.ws-private.ts` | COIN-M private WebSocket: listen-key stream, account/order updates                          |
| `binanceinverse.ws-public.ts`  | COIN-M public WebSocket: dstream tickers, book tickers, mark price, kline, orderbook depth  |

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
   **A failed listenKey request retries** (5s doubling to 60s) instead of
   rejecting un-awaited and leaving the private stream unbuilt; there is ONE
   keepalive chain per socket (`scheduleListenKeyRefresh` — every reconnect used
   to start another) and a failed keepalive reschedules in 60s rather than ending
   it. A failed startup `fetchOrders` sets `ordersSnapshotStale`: orders are NOT
   marked loaded (an empty list would read as "no open orders") and the tick
   refetches until one succeeds. Binance PM carries the same code. Gated by
   `__tests__/audit-regressions/binance.private-stream-recovery.test.ts`.
   **The livenet listenKey and every public request are tagged
   `markSkipRemoteSigning`** (both `createAPI` and `createPublicAPI`, plus PM and
   spot), so a hosted server sends them without a browser round trip — they were
   already unsigned. Never tag on a credential condition; see
   `__tests__/binance.public-endpoint-tagging.test.ts`.
   **Every open after the first resyncs orders** (`throttledFetchOrders`): the
   base house-keep snapshot attaches to `this.ws` while the async connect is still
   awaiting the listenKey, so it never fired, and events in the gap were lost.
2. **`fetchPositionMode()`** -- `GET /fapi/v1/positionSide/dual`. Sets `isHedged` store setting based on `dualSidePosition`.
3. **`tick()`** -- The first tick immediately fetches both `/positionRisk` and `/account`. A 3s health loop then reconciles `/account` every 25s while the private WS is connected, or every 3s while it is disconnected. `/positionRisk` is refreshed every 75s in addition to event-triggered repairs.
4. **`fetchOrders()`** -- Two parallel calls: `GET /fapi/v1/openOrders` + `GET /fapi/v1/openAlgoOrders?algoType=CONDITIONAL`. Merges regular + algo orders. A failed pair returns the current order-store snapshot, rather than an authoritative empty list, so startup/reconnect refreshes preserve live order state until Binance answers successfully.
5. **`fetchPositions()`** -- Reconciles detailed positions from
   `GET /fapi/v3/positionRisk` joined with `GET /fapi/v1/symbolConfig` for
   leverage and margin type. V3 only returns symbols with a position or open
   order, so zero-position rows are synthesized for the rest of the market set.

`changePositionMode()` runs in the worker for the web app. Keep the main-thread
copy in sync through `afterMethodCalledInWorker`; update both `isHedged` and the
symbol's `fetchedLeverage.positionMode`. Treat Binance code `-4059` (`No need to
change position side`) as idempotent success for the requested mode, and keep it
in the REST client's silent error-code list so duplicate clicks don't emit a UI error.

Futures REST snapshots that expect arrays must validate the response shape before
calling `.find()` / `.map()` / `.reduce()`. Binance and proxy/tunnel paths can
surface `{ code, msg }` error envelopes where an array is expected; those should
emit a clear `Binance <endpoint> response malformed` exchange error and preserve
the previous store snapshot, not leak minified `e.find is not a function` startup
errors to users.

### Portfolio Margin Startup Differences

- Uses `papiXhr` instead of `xhr` for authenticated endpoints.
- `fetchBalance()` is separate from position fetching (tick calls `fetchBalance()` instead of `fetchPositionsAndBalance()`).
- `fetchPositions()` creates fake zero-size positions for all markets to ensure UI shows all symbols (`binancepm.exchange.ts:343-365`).
- PM REST snapshots must validate array response shapes before `.map()` / `.find()`. Binance and the HoW/proxy path can surface non-array error envelopes; those should become clear `Binance PM <endpoint> response malformed` exchange errors and preserve the previous store snapshot, not raw `e.map is not a function` / `t.find is not a function` crashes.

### Spot Startup Differences

- No `fetchPositionMode()` call (no hedge mode support).
- `tick()` has no counter-based position risk refresh; always calls `fetchPositionsAndBalance()`.
- Uses `options.extra?.tickInterval` for tick interval instead of hardcoded 5000ms.

## REST API Endpoints

### USD-M Futures (`fapi.binance.com`)

| Endpoint                     | Method      | Description                                                                          |
| ---------------------------- | ----------- | ------------------------------------------------------------------------------------ |
| `/fapi/v1/exchangeInfo`      | GET         | Fetch markets (symbols, filters, contract types)                                     |
| `/fapi/v3/balance`           | GET         | Fetch account balances per asset                                                     |
| `/fapi/v3/account`           | GET         | Fetch compact account info and balances                                              |
| `/fapi/v1/symbolConfig`      | GET         | Fetch current leverage and margin type per symbol                                    |
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
| `/fapi/v1/algoOpenOrders`    | DELETE      | Cancel all open algo orders for a symbol (SL/TP/trailing)                            |
| `/fapi/v1/positionSide/dual` | GET/POST    | Get or set hedge mode (dual position side)                                           |
| `/fapi/v1/leverage`          | POST        | Set leverage for a symbol                                                            |
| `/fapi/v1/marginType`        | POST        | Set margin type (ISOLATED or CROSSED)                                                |
| `/fapi/v3/positionRisk`      | GET         | Fetch detailed active position risk info                                             |
| `/fapi/v1/klines`            | GET         | OHLCV candle data                                                                    |
| `/fapi/v1/depth`             | GET         | Orderbook snapshot (limit up to 1000)                                                |
| `/fapi/v1/trades`            | GET         | Recent public trades (max 1000)                                                      |
| `/fapi/v1/userTrades`        | GET         | User trade history (requires symbol)                                                 |
| `/fapi/v1/income`            | GET         | Income history (realized PnL, funding fees, commissions)                             |
| `/fapi/v1/listenKey`         | POST/PUT    | Create or keepalive a user data stream listen key                                    |

### COIN-M Inverse Futures (`dapi.binance.com`)

`binanceinverse` uses Binance's COIN-M `/dapi` family, not the USD-M `/fapi`
family. Keep it as a separate variant because symbols, collateral, native order
quantity, and position sizing are inverse-contract specific.

| Endpoint                     | Method      | Description                                                        |
| ---------------------------- | ----------- | ------------------------------------------------------------------ |
| `/dapi/v1/exchangeInfo`      | GET         | Fetch COIN-M markets and contract metadata, including contract size |
| `/dapi/v1/account`           | GET         | Fetch COIN-M account assets and balances                           |
| `/dapi/v2/leverageBracket`   | GET         | Fetch leverage brackets per symbol                                 |
| `/dapi/v1/ticker/24hr`       | GET         | 24h ticker statistics                                              |
| `/dapi/v1/ticker/bookTicker` | GET         | Best bid/ask prices                                                |
| `/dapi/v1/premiumIndex`      | GET         | Mark price, index price, funding rate                              |
| `/dapi/v1/openOrders`        | GET         | Fetch all open orders                                              |
| `/dapi/v1/order`             | POST/DELETE | Place or cancel a single order, including stop/TP market orders    |
| `/dapi/v1/batchOrders`       | POST/DELETE | Place or cancel batch orders                                       |
| `/dapi/v1/allOpenOrders`     | DELETE      | Cancel all open orders for a symbol                                |
| `/dapi/v1/positionSide/dual` | GET/POST    | Get or set hedge mode                                              |
| `/dapi/v1/leverage`          | POST        | Set leverage for a symbol                                          |
| `/dapi/v1/marginType`        | POST        | Set margin type                                                    |
| `/dapi/v1/positionRisk`      | GET         | Fetch detailed position risk info                                  |
| `/dapi/v1/klines`            | GET         | OHLCV candle data                                                  |
| `/dapi/v1/depth`             | GET         | Orderbook snapshot                                                 |
| `/dapi/v1/trades`            | GET         | Recent public trades                                               |
| `/dapi/v1/userTrades`        | GET         | User trade history                                                 |
| `/dapi/v1/income`            | GET         | Income history                                                     |
| `/dapi/v1/listenKey`         | POST/PUT    | Create or keepalive a user data stream listen key                  |

### Spot (`api.binance.com`)

| Endpoint               | Method      | Description                              |
| ---------------------- | ----------- | ---------------------------------------- |
| `/api/v3/exchangeInfo` | GET         | Fetch spot markets                       |
| `/api/v3/account`      | GET         | Fetch spot balances                      |
| `/api/v3/ticker/24hr`  | GET         | 24h ticker statistics (includes bid/ask) |
| `/api/v3/order`        | POST/DELETE | Place or cancel a single order           |
| `/api/v3/openOrders`   | DELETE      | Cancel all open orders for a symbol      |
| `/api/v3/openOrders`   | GET         | Fetch all currently open orders          |
| `/api/v3/klines`       | GET         | OHLCV candle data                        |
| `/api/v3/depth`        | GET         | Orderbook snapshot                       |

Spot private events do not have a REST listen-key endpoint. Binance removed
`POST`/`PUT`/`DELETE /api/v3/userDataStream` on 2026-02-20; private subscription
is performed on the WebSocket API as described below.

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

As of **2026-04-23**, Binance retired the legacy `wss://fstream.binance.com/ws` endpoint for USDⓈ-M futures and split it into three category-specific base URLs (`/public/ws`, `/market/ws`, `/private/ws`). PM private remains on `/pm/ws/`. Spot public streams remain on `stream.binance.com`, while Spot private events use the separately authenticated WebSocket API after Binance removed listen keys on 2026-02-20. See the [USD-M WebSocket change notice](https://developers.binance.com/docs/derivatives/usds-margined-futures/websocket-market-streams/Important-WebSocket-Change-Notice) and [Spot WebSocket API](https://developers.binance.com/docs/binance-spot-api-docs/websocket-api/general-api-information).

| Variant             | Public (high-frequency)                    | Market (regular market data)               | Private (user data)                                     |
| ------------------- | ------------------------------------------ | ------------------------------------------ | ------------------------------------------------------- |
| **Futures** livenet | `wss://fstream.binance.com/public/ws`      | `wss://fstream.binance.com/market/ws`      | `wss://fstream.binance.com/private/ws?listenKey=…`      |
| **Futures** testnet | `wss://demo-fstream.binance.com/public/ws` | `wss://demo-fstream.binance.com/market/ws` | `wss://demo-fstream.binance.com/private/ws?listenKey=…` |
| **Inverse** livenet | `wss://dstream.binance.com/ws`             | same socket via live subscriptions         | `wss://dstream.binance.com/ws/{listenKey}`              |
| **Inverse** testnet | `wss://dstream.binancefuture.com/ws`       | same socket via live subscriptions         | `wss://dstream.binancefuture.com/ws/{listenKey}`        |
| **PM** livenet      | `wss://fstream.binance.com/public/ws`      | `wss://fstream.binance.com/market/ws`      | `wss://fstream.binance.com/pm/ws/{listenKey}`           |
| **PM** testnet      | `wss://demo-fstream.binance.com/public/ws` | `wss://demo-fstream.binance.com/market/ws` | `wss://demo-fstream.binance.com/pm/ws/{listenKey}`      |
| **Spot** livenet    | `wss://stream.binance.com:9443/ws`         | (same as public — unaffected by split)     | `wss://ws-api.binance.com:443/ws-api/v3`                |
| **Spot** testnet    | `wss://stream.testnet.binance.vision/ws`   | (same as public — unaffected by split)     | `wss://ws-api.testnet.binance.vision/ws-api/v3`         |

**Stream → endpoint routing (futures + PM):**

- `/public/ws`: `!bookTicker`, `{symbol}@bookTicker`, `{symbol}@depth`, `{symbol}@rpiDepth`
- `/market/ws`: `!ticker@arr`, `!markPrice@arr@1s`, `{symbol}@kline_{interval}`, `@aggTrade`, `@trade`, `@forceOrder`, `@miniTicker`, etc.
- `/private/ws`: user data via `?listenKey=<key>` (and optional `&events=...` filter — omitted in our impl to receive all events, matching legacy behavior)

### Public WebSocket Channels

For futures and PM, channels are split between two underlying connections owned by `BinancePublicWebsocket` / `BinancePMPublicWebsocket`:

- The class itself connects to `/public/ws` and handles `!bookTicker` + `{symbol}@depth`.
- A sibling `BinanceMarketWebsocket` / `BinancePMMarketWebsocket` connects to `/market/ws` and handles `!ticker@arr`, `!markPrice@arr@1s`, and `{symbol}@kline_{interval}`.
- Callers continue to use the single `publicWebsocket` reference; `listenOHLCV` is forwarded to the sibling market socket while `listenOrderBook` stays on the public socket.
- On-demand depth topics are ref-counted. The final unsubscribe deletes the
  topic key, unsubscribe-before-connect is a no-op for the refcount, and
  reconnect replay skips non-positive legacy entries so phantom depth streams
  cannot accumulate.

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

Futures and PM use listenKey-based authentication. Spot uses a signed WebSocket
API subscription and never calls the retired Spot listen-key REST endpoints.

#### Futures (`BinancePrivateWebsocket`)

| Event Type           | Description                                                                              |
| -------------------- | ---------------------------------------------------------------------------------------- |
| `ACCOUNT_UPDATE`     | Balance and position changes; triggers throttled position re-fetch                       |
| `ORDER_TRADE_UPDATE` | Regular order lifecycle (NEW, PARTIALLY_FILLED, FILLED, CANCELED, EXPIRED)               |
| `ALGO_UPDATE`        | Algo order lifecycle (NEW, TRIGGERED, TRIGGERING, CANCELED, EXPIRED, REJECTED, FINISHED) |

`ACCOUNT_UPDATE` applies position quantity/PnL immediately, but the payload omits
leverage, margin type, and liquidation price. The follow-up `/positionRisk`
repair is therefore capped at one request per 10 seconds; repeated position
events coalesce into the pending repair instead of consuming the shared proxy
IP's request-weight budget at the old 500 ms cadence. Periodic reconciliation
still provides a full safety snapshot.

USD-M background `/account` and `/positionRisk` reconciliation also observes
Binance's `X-MBX-USED-WEIGHT-1M` response header. At 80% of the 2,400 IP-weight
limit it pauses background reads for 60 seconds; HTTP 429/418 uses
`Retry-After` when present. This guard never blocks explicit trading requests,
and WebSocket account/order updates continue while reconciliation is paused.

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
  liquidation price between the 75s full-risk ticks.

#### Spot (`BinancePrivateWebsocket`)

| Event Type                | Description               |
| ------------------------- | ------------------------- |
| `outboundAccountPosition` | Balance changes per asset |
| `executionReport`         | Order lifecycle events    |

**Spot-specific**: Uses `data.c` (clientOrderId) as order ID (not `data.i` like futures).

On every socket open (including reconnect), Spot sends
`userDataStream.subscribe.signature`. Its params are `apiKey`, a 5-second
`recvWindow`, `timestamp`, and the lowercase hex HMAC-SHA256 of those first
three params sorted alphabetically. HMAC keys and hosted `remoteSigner` accounts
use the same canonical payload. A non-200/malformed subscription response fails
closed. There is no listen-key fallback because Binance removed those endpoints
on 2026-02-20. If an asynchronous hosted signature resolves after a newer socket
open, its attempt token is stale and the frame is dropped so the new connection
cannot receive duplicate subscriptions.

**Spot order-type mapping.** Spot `executionReport` reports the order type in the `o` field (values `LIMIT`/`MARKET`/`STOP_LOSS`/`STOP_LOSS_LIMIT`/`TAKE_PROFIT`/`TAKE_PROFIT_LIMIT`/`LIMIT_MAKER`) — **not** the futures `ot` field or futures values. `handleOrderEvents` maps via `ORDER_TYPE_SPOT` (in `binancespot.types.ts`); `LIMIT_MAKER` → `OrderType.Limit`, both `STOP_LOSS*` → `StopLoss`, both `TAKE_PROFIT*` → `TakeProfit`. Spot has no `reduceOnly` (kept `false`).

Spot order requests never send futures-only `positionSide`, `closePosition`, or
`reduceOnly`. Regular post-only limits use `LIMIT_MAKER`; stop-loss and
take-profit use the spot market-trigger types. The shared trailing-stop option
is rejected because it supplies a futures callback-rate shape, while Binance
Spot requires `trailingDelta` on a stop order. Spot also requires an explicit
positive `quantity` for the supported order shapes here, so `binancespot`
advertises `supportsInfinityStops: false` and `formatCreateOrder` rejects
`amount <= 0` before `/api/v3/order`.

WebSocket API events arrive as `{ subscriptionId, event }`; the inner Spot event
has top-level `executionReport` fields, and `outboundAccountPosition.B` carries
changed balances. Do not wrap fixtures in the futures-only nested `o`/`a`
envelopes. Balance events merge the changed assets into the existing currency
snapshot and recompute USD totals.

**Spot execution-type vs order-status.** In `executionReport`, `x` is the **execution type** (`NEW`/`TRADE`/`CANCELED`/…) and `X` is the **order status** (`NEW`/`PARTIALLY_FILLED`/`FILLED`/`CANCELED`/…). Fills arrive with `x === 'TRADE'` (the handler previously checked `X === 'TRADE'`, which never matched). On a `TRADE` the handler emits `fill` **and** mirrors the cumulative fill onto the store row (`filled = z`, `remaining = q - z`), transitioning to `Closed` when `X === 'FILLED'` / `z === q`. Without the store update a filled spot order stayed `Open` until the next REST `fetchOrders` — same class of bug the futures `PARTIALLY_FILLED` note describes.

#### Portfolio Margin (`BinancePMPrivateWebsocket`)

| Event Type                       | Description                                                                                                              |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `ACCOUNT_UPDATE`                 | Balance and position changes (same as futures)                                                                           |
| `ORDER_TRADE_UPDATE`             | Regular order events (uses `clientOrderId` via `data.c`, not `orderId`)                                                  |
| `CONDITIONAL_ORDER_TRADE_UPDATE` | Conditional order events (nested in `so` object, uses `data.c` as ID, `data.st` for strategy type, `data.os` for status) |

**PM-specific**: Position side in `ACCOUNT_UPDATE` is determined by the signed
`p.pa` value, while stored contracts and notional are always absolute and
non-negative, matching the futures REST/WS contract. One-way mode is indicated
by `p.ps === 'BOTH'`. PM does NOT have the `throttledFetchOrders` debounce that
futures has.

### ListenKey and Spot Subscription Management

Futures and PM retain listen keys:

1. **Create**: `POST /listenKey` returns `{ listenKey: "..." }`
2. **Keepalive**: `PUT /listenKey` every 30 minutes via `setTimeout`
3. ListenKey expires after 60 minutes if not renewed
4. Keepalive is self-rescheduling: each successful `PUT` schedules the next keepalive

Spot has no listen-key lifecycle. The WebSocket API subscription is connection
scoped and is re-signed on each open. A `session.status` heartbeat must report
`userDataStream: true`; if it does not, the socket reconnects and subscribes
again.

### Ping/Pong

WebSocket health is monitored by two independent mechanisms:

1. **Application heartbeat**: Futures and PM use `LIST_SUBSCRIPTIONS`. Spot uses WebSocket API `session.status` and accepts it as a pong only when the response is successful and reports `userDataStream: true`. On pong, `handlePongEvent()` schedules the next ping in 5 seconds and sets a pong miss timeout at `1.5 * 5000 = 7,500ms`. If pong is not received within 7.5 seconds, the WebSocket triggers a reconnection.
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

**Futures** (`BinanceExchange.cancelOrders`):

1. Split orders into regular and algo based on `isAlgoOrderType()`.
2. **Regular orders**: Group by symbol. Single cancel via `DELETE /fapi/v1/order` with `orderId`. Batch cancel via `DELETE /fapi/v1/batchOrders` with `orderIdList` (max 10 per batch, processed via `forEachSeries`).
3. **Algo orders**: Cancel one at a time via `DELETE /fapi/v1/algoOrder` with `algoId` parameter.
4. Error code `-2011` ("Unknown order sent") is treated as success (order already cancelled).
5. Batch cancel constructs `orderIdList` as raw string `[123,456]` (not parsed with `parseInt`) to avoid precision loss on large IDs.

Symbol-wide cancellation calls both Binance cancel-all families: regular
`DELETE /fapi/v1/allOpenOrders` and Algo Service
`DELETE /fapi/v1/algoOpenOrders`, each with `symbol`. Binance documents the
second endpoint specifically for conditional TP/SL and trailing orders; the
regular endpoint does not claim that coverage. Both requests are attempted even
when one fails. Local regular and algo rows are marked canceled independently
only after their corresponding request succeeds, and any partial failure is
re-thrown so the caller cannot mistake a partial cancel for full success.

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
- **Inverse**: `BTCUSD_PERP`, `ETHUSD_PERP` (COIN-M native symbol, quote is USD, margin is base coin)
- **Market ID format (inverse)**: `{base}/USD:{base}` (e.g., `BTC/USD:BTC`)
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
- **Inverse**: Amount precision is native contract count from `LOT_SIZE.stepSize`
  (typically integer contracts), and `market.contractSize` is the fixed USD value
  per contract. Orders send native contracts; positions expose base-size
  `contracts` by converting native contracts through mark/last price and keep the
  native count in `position.info.contractsNative`.
- **Spot**: Uses `LOT_SIZE.stepSize` and `PRICE_FILTER.tickSize` from
  exchangeInfo. If a filter is absent, the compatibility fallback is
  `10 ** -baseAssetPrecision` / `10 ** -quoteAssetPrecision`. Amount limits
  likewise use `LOT_SIZE.minQty` and `maxQty` when present.

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
- **Inverse**: `abs(nativeContracts * market.contractSize)` for USD notional; base-size exposure is `nativeContracts * contractSize / mark`.
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

### User Trades (`/fapi/v1/userTrades`)

`fetchHistoricalFills` builds its query through `buildUserTradesParams` and maps rows through
`mapBinanceUserTrade` (both in `binance.utils.ts`) rather than inline, because the exchange method is
an instance field and cannot be reached from a test. Two fixed defects worth not reintroducing:

- **The request used to be hardcoded to `{ symbol, limit: 1000 }`**, so `limit`, `from`/`to`,
  `orderId`, and `fromId` were all silently discarded — a caller asking for 5 rows, one order, or a
  one-hour window received the account's most recent 1000 fills instead. Verified live: `limit: 5`
  now returns 5, an `orderId` returns just that order's fills, and a time window returned 66 of 131.
- **`quoteQty` was the one numeric passed through uncoerced.** Binance returns every number as a
  decimal string, so `quoteQty` violated `HistoricalTrade.quoteQty: number` on 100% of rows while its
  siblings were `+`-coerced. Absent/null now maps to `undefined` rather than `NaN`.

Binance rejects `fromId` combined with `startTime`/`endTime`, so a time window wins and `fromId` is
only sent when neither bound is present. `limit` is clamped to the endpoint's 1000 maximum. Deeper
history than the most recent 1000 fills per symbol still requires explicit `fromId` paging by the
caller — `fetchHistoricalFills` issues a single request.

### Income History Rate Limit

The `/fapi/v1/income` endpoint currently carries IP request weight 30, returns
at most 1000 records, and accepts at most a 7-day time window per request.
`fetchIncomeHistoryRange` handles longer ranges and pagination within a window;
separate seven-day chunks are paced two seconds apart so long journal backfills
do not burst this high-weight endpoint.

**The 2s chunk pacing is a WEIGHT budget and `maxRPS: 3` does not substitute for
it.** The axios limiter counts requests, not weight: at weight 30, 3 RPS is 5400
weight/min against a 2400 cap. Removing the sleep because "the client already
rate-limits" is wrong, and `binance.income-pacing.test.ts` exists to catch it.

**Paging is span-based — see `../income-paging.ts` (shared with weex).** The
previous implementation gated _both_ ends of its loop on a full-page count
(`length === 1000` to enter the inner walk, `< 1000` to leave it). Both are
guesses about the venue, and either one stopping early truncates the ledger
silently — which matters more here than anywhere else, because binance currently
PASSES the canonical gate, so a silent truncation corrupts a _green_ verdict
rather than producing an obviously absent one. It had never fired only because
the test account is quiet enough that no 7-day window reaches 1000 records.

Two defects the live probe (`yarn history-audit --income-paging`) found that
fixtures structurally could not:

- **`fetchIncomeHistory` swallows errors into `[]`, and a pager must not be
  handed that contract.** A rate-limited page and an empty window become the same
  value, so a 429 mid-walk reads as "nothing more here". Measured: a 90-day walk
  returned 492 of 512 records while reporting success. `_fetchIncomeHistoryPage`
  is the RETHROWING variant used by the range walker; the public method keeps
  returning `[]` because the worker `CallMethodHandler` and the module-frame
  account hooks call it directly and expect an array.
- **A full page whose boundary millisecond is shared cuts rows.** Stepping the
  cursor to `newest + 1` skipped a `COMMISSION` that shared its timestamp with a
  `REALIZED_PNL` — 279 of 280 records. Branch boundaries are inclusive, with an
  exclusive continuation only when an inclusive re-ask yields nothing new.

Verified live on sheet row 2: 280/280 records over 30 days and 512/512 over 90
days, against a forced small page size so the walk really pages.

## State Reconciliation

### Position Reconciliation

**WebSocket partial updates**: `ACCOUNT_UPDATE` events provide real-time position changes (entry price, contracts, unrealized PnL). They are upserted immediately via `store.addOrUpdatePosition()`, including brand-new positions; one-way `ps=BOTH` events derive long/short from the signed amount. Unknown zero-size rows are ignored, while a zero update for an existing row closes it immediately. Every event containing position rows schedules a full `/positionRisk` repair because the WS payload omits leverage, margin type, and liquidation price.

**REST full snapshots**: Two REST endpoints provide position data at different detail levels:

1. **`/fapi/v3/account`** (every 25s while the private WS is connected; every 3s while disconnected): refreshes balances. Its compact position rows deliberately do not replace the rich position snapshot.
2. **`/fapi/v3/positionRisk` + `/fapi/v1/symbolConfig`** (every 75s and after WS account-update throttled refreshes): joins price/risk fields with current leverage and margin type. Because V3 only returns symbols with positions or open orders, the mapper synthesizes zero rows for omitted markets.

**Merge strategy**: WebSocket events apply fast quantity/PnL changes. The V3
position-risk reconciliation is authoritative for position detail; account V3
is authoritative only for balances and preserves the current position array.

**Stale snapshot guard**: Every WS position event advances a shared position-state version. Both event-triggered `/positionRisk` repairs and periodic `/account`/`positionRisk` snapshots capture that version before requesting; if a newer WS event arrives before the response, its position rows are discarded. `/account` balance data and loaded flags still commit because they are independent of the stale position rows.

### Order Reconciliation

**WebSocket real-time updates**: `ORDER_TRADE_UPDATE` and `ALGO_UPDATE` events apply immediate order status changes (NEW, FILLED, CANCELED, etc.) to the store.

**Throttled REST refresh** (`binance.ws-private.ts:267-290`): After any order/algo WebSocket event, a 10-second debounced `fetchOrders()` re-fetches all open orders from REST and replaces the store's order list. This ensures any missed events or race conditions are corrected. A transient REST failure returns the current store snapshot, so this full-store write cannot turn a failed request into an empty-order assertion; a successful empty response still clears the list.

**PM variant**: Does NOT have the throttled order refresh mechanism. Relies solely on WebSocket events for order state.

**PARTIALLY_FILLED must mirror cumulative fill onto the store row.** Binance's `ORDER_TRADE_UPDATE` carries the cumulative filled qty in `z` (and last fill qty in `l`). On `PARTIALLY_FILLED` / `FILLED`, `handleOrderEvents` (futures and PM regular) emits a `fill` event AND calls `store.updateOrder({ id }, { filled: z, remaining: q - z, lastUpdate })`. Without that store update, the only fill signal is the emitted event — which the chaser supervisor in `webworker/chaserOrder/` only subscribes to for `hyperliquid`/`grvt`. For every other exchange (including Binance) the chaser reads `order.filled` from the store. If the store row still shows `filled: 0` from the NEW event, the chaser will cancel and re-place a moved-away order at the **full original size** instead of the remaining size. The 10s `throttledFetchOrders` debounce is far too coarse to plug this — the race fires inside a single price-chase tick. Don't strip the per-event `updateOrder` thinking it's redundant with the throttled REST refresh.

### Balance Reconciliation

**Futures**: Aggregates per-asset balances from `/fapi/v3/account`:

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
```

### Binance Inverse (`binanceinverse`)

```
enabled: false
enabledInDev: true
enableInPreview: false
supportsTestnet: true
supportsPublicOnly: true
supportsTrailingStopLoss: true
supportsChaser: true
supportsTPSLCloseAttribute: true
supportsTunnel: true
supportedOnMobile: false
defaultQuote: USD
supportedTPSLTriggerTypes: [LastPrice, MarkPrice]
defaultTPTriggerType: LastPrice
defaultSLTriggerType: MarkPrice
doesNotSupportOpenInterest: true
alwaysCalculateUnrealizedPnlFromLastPrice: true
tickInterval: 500
latencyDomain: dapi.binance.com
orderbookTickDelay: 60
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
```

### Binance Spot (`binancespot`)

```
enabled: false (enabledInDev: true)
supportsTestnet: true
supportsChaser: true
doesNotSupportOpenInterest: true
latencyDomain: api.binance.com
orderbookTickDelay: 60
```

## Testnet / Demo

### URL Differences

| Variant   | Livenet REST               | Testnet REST                     |
| --------- | -------------------------- | -------------------------------- |
| Futures   | `https://fapi.binance.com` | `https://demo-fapi.binance.com`  |
| PM (papi) | `https://papi.binance.com` | `https://demo-fapi.binance.com`  |
| PM (fapi) | `https://fapi.binance.com` | `https://demo-fapi.binance.com`  |
| Spot      | `https://api.binance.com`  | `https://testnet.binance.vision` |

| Variant      | Livenet WS (public + market)                                                  | Testnet WS (public + market)                                                            |
| ------------ | ----------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Futures      | `wss://fstream.binance.com/public/ws` + `wss://fstream.binance.com/market/ws` | `wss://demo-fstream.binance.com/public/ws` + `wss://demo-fstream.binance.com/market/ws` |
| Spot public  | `wss://stream.binance.com:9443/ws`                                            | `wss://stream.testnet.binance.vision/ws`                                                |
| Spot private | `wss://ws-api.binance.com:443/ws-api/v3`                                      | `wss://ws-api.testnet.binance.vision/ws-api/v3`                                         |

### Behavioral Differences

- **Spot private auth**: Both environments use the same signed WebSocket API
  request contract; only the socket host differs. There is no Spot listen-key
  signing exception.
- **Symbol availability**: Testnet typically has a much smaller set of symbols.
- **PM testnet**: Both papi and fapi use Binance's documented USD-M demo host.
  Binance does not document a separate Portfolio Margin sandbox, so PM-specific
  papi endpoint availability still needs to be treated as best effort.

## Known Quirks / Implementation Notes

- **Max leverage is per-account, from the SIGNED `leverageBracket` endpoint.** A
  keyless instance (`isPublicDataOnly`) builds every market with
  `leverage.max = 1`. On exchange-server, private accounts read markets from the
  shared `__pub_binance` / `__pub_binancepm` instance, so `start()` must call
  `loadAccountMaxLeverage` and `adaptExternalMarkets` must overlay it (new
  objects, never mutate — the shared markets are referenced by every account).
  Without it the slider pins to 1x and `setLeverage` clamps every change to 1x
  (reported 2026-10-07; latent since the shared-public-data change of 2026-08-15).

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

The separation also applies to symbol-wide cancellation. Always keep
`/fapi/v1/allOpenOrders` and `/fapi/v1/algoOpenOrders` paired in
`cancelSymbolOrders`; do not optimistically mark both order families canceled
after only one endpoint succeeds.

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
- Uses `/api/v3/account`; spot balances map into `balance.currencies`/`coins`
  and spot returns no derivative positions.
- Account event handler (`outboundAccountPosition`) treats all positions as Long side (`binancespot.ws-private.ts:104`)
- No full-position/infinity stops; spot stop-loss and take-profit orders must
  carry a positive explicit quantity.
- No batch order support -- places orders one at a time in a loop. Spot may
  split orders above `LOT_SIZE.maxQty`, but only when `maxQty > 0`; a malformed
  zero max must fall back to one order, not an infinite/empty split.
- No `bigIntAsStringJsonParse` in API layer

### Spot Precision Calculation

Spot reads tradable amount and price increments from `LOT_SIZE.stepSize` and
`PRICE_FILTER.tickSize`. Only malformed/absent filters fall back to
`10 ** -baseAssetPrecision` and `10 ** -quoteAssetPrecision`.

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

Futures runs a 3-second private-socket health loop. While connected, the large `/account` snapshot is fetched every 25 seconds; while disconnected, it falls back to every 3 seconds until WS service returns. Full position risk data is re-fetched every 75 seconds and after private WS account updates. PM retains its 5-second `/balance` tick and 16-tick (~80-second) position-risk cadence. Spot uses `options.extra?.tickInterval` for its interval.

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
4. For data-fetching operations (tickers, positions, balance, and full order snapshots): return current store state as fallback. A successful empty response remains authoritative; only an error response is preserved as the existing snapshot.

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
- [x] Symbol-wide regular + Algo Service cancellation with partial-failure-safe local state
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
| Private WS path         | `/private/ws?listenKey=…`                             | `/pm/ws/{listenKey}`                                                     | `/ws-api/v3` + signed subscription           |
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

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- **USD-M** sends `newClientOrderId`, rule `^[.A-Z:/a-z0-9_-]{1,36}$`; **PM** the same charset capped at 32. **Spot's documented legal range is narrower**, `^[A-Za-z0-9_-]{1,36}$` — `.`, `:` and `/` that USD-M accepts are rejected by Spot, so it has its own rule.
- A caller ID replaces the generated `x-s3vFFRQp…` ID outright; no broker prefix is composed. PM and Spot return `newClientOrderId` as the order ID, so the placement result is the caller's ID there.
- `updateOrder` is cancel+replace on all three; the replacement gets a generated ID (Spot refuses a reused ID until the old order fills).
