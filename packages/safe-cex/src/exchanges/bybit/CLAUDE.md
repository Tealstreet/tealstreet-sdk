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

| File                    | Description                                                                                                                                                                                                                                                                                                                                                                                |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `bybit.exchange.ts`     | Main exchange class extending `BaseExchange`. Handles linear perpetual futures (USDT and USDC settled). Contains order placement, position management, balance calculation, and all trading operations.                                                                                                                                                                                    |
| `bybit.api.ts`          | REST API client factory. Creates Axios instance with HMAC-SHA256 signing, retry logic, recv_window handling, and virtual clock time synchronization.                                                                                                                                                                                                                                       |
| `bybit.order-params.ts` | Validates and serializes shared `maxSlippagePct` plus adapter-only typed BBO and TickSize execution controls.                                                                                                                                                                                                                                                                              |
| `bybit.income.ts`       | Transaction-log route AND the row mapper, shared by linear and inverse. `fetchBybitIncomeHistory` / `fetchBybitIncomeHistoryRange` take a `BybitIncomeContext` of `{ xhr, log, category, accountType }`; `mapBybitTransactionToIncomeRecords` decomposes one row into several `IncomeRecord`s.                                                                                             |
| `bybit.types.ts`        | Type definitions, endpoint constants, type mapping objects (`ORDER_TYPE`, `ORDER_SIDE`, `ORDER_STATUS`, `ORDER_TIME_IN_FORCE`, `ORDER_TRIGGER_TYPE`, `POSITION_SIDE`), WebSocket URL constants, interval mappings, `BybitApiKeyInfo` interface, transaction log types, and `ORDER_HISTORY_MAX_WINDOW_MS` (moved here so `bybit.income.ts` can use it without importing the exchange back). |
| `bybit.rateLimits.ts`   | VIP-level rate limit table. Defines `BybitVipLevel` (No VIP through PRO-6), `BybitProductType` (Futures/Option/Spot), and `BybitAccountType` (Classic+UTA / UTA Pro). Used to dynamically adjust batch order limits.                                                                                                                                                                       |
| `bybit.ws-public.ts`    | Public WebSocket (`BybitPublicWebsocket`). Handles ticker updates, kline/OHLCV streaming, orderbook snapshot+delta, and public trade data for linear markets.                                                                                                                                                                                                                              |
| `bybit.ws-private.ts`   | Private WebSocket (`BybitPrivateWebsocket`). Handles order updates, position updates, authentication, and time sync. Subscribes to `order` and `position` topics.                                                                                                                                                                                                                          |

### Demo Trading - `BybitDemoExchange`

| File                    | Description                                                                                                                                                                                                                                        |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bybitdemo.exchange.ts` | Minimal subclass of `BybitExchange` that sets `isDemo = true` and restricts to `['linear']` category. Uses demo-specific REST and private WebSocket URLs (`api-demo.bybit.com`, `stream-demo.bybit.com`); public data stays on the mainnet stream. |

### Inverse Perpetual - `BybitInverseExchange`

| File                         | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `bybitinverse.exchange.ts`   | Standalone exchange class (extends `BaseExchange` directly, NOT `BybitExchange`). Handles inverse perpetual contracts (USD-quoted, coin-settled like BTCUSD). Contains specialized amount handling (integer USD contracts) and position mapping (USD-to-BTC conversion). Carries its own income route delegating to `bybit.income.ts` with `category: 'inverse'` — see the UNIFIED-only note below, and do NOT wire it to this class's `accountType` getter. |
| `bybitinverse.ws-public.ts`  | Public WebSocket for inverse markets (`BybitInversePublicWebsocket`). Connects to `v5/public/inverse` endpoint. Supports dynamic ticker topic subscription via `activeTickerSymbols:changed` event.                                                                                                                                                                                                                                                          |
| `bybitinverse.ws-private.ts` | Private WebSocket for inverse markets (`BybitInversePrivateWebsocket`). Identical auth flow to linear private WS. Subscribes to `order` and `position` topics.                                                                                                                                                                                                                                                                                               |

### Spot Trading - `BybitSpotExchange`

| File                      | Description                                                                                                                                                                                                                         |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bybitspot.exchange.ts`   | Standalone exchange class for spot trading (extends `BaseExchange` directly). Sets `isSpot = true`. No position tracking (`fetchPositions` returns `[]`). All API calls use `category: 'spot'`. Leverage fixed at min:1, max:1.     |
| `bybitspot.types.ts`      | Spot-specific type definitions. Contains separate `BASE_WS_URL` with `v5/public/spot` WebSocket endpoint. Shares the same `ENDPOINTS` and type mappings as linear, but lacks `CREATE_BATCH_ORDER`.                                  |
| `bybitspot.ws-public.ts`  | Public WebSocket for spot markets (`BybitPublicWebsocket` - note: same class name as linear). Connects to `v5/public/spot`. Orderbook depth is 200 levels (vs 1000 for linear/inverse). Supports dynamic ticker topic subscription. |
| `bybitspot.ws-private.ts` | Private WebSocket for spot markets. Same structure as linear, except it subscribes to `order` ALONE — spot has no positions.                                                                                                        |

## Per-account fees on exchange-server

`takerFee`/`makerFee` come from the signed `FEE_RATE` endpoint, which the keyless
shared exchange skips. `bybit` and `bybitinverse` load them in `start()` via
`loadAccountMarketOverlay(bybitFeeOverlay(...))` — see safe-cex `CLAUDE.md` →
"Account data on shared public markets". `bybitspot` never fetched fees.

## Authentication

### Rust signer custody

Rust Bybit variants use opaque primary/secondary identities and the engine's
send-time Signer for REST, opposite-environment diagnostics and static private
socket authentication. Trusted setup consumes raw credentials before venue
construction; secondary GET routing and primary writes/socket login remain.
Protocol clock calibration affects canonical stamping; raw timers stay unchanged.
This does not add a transient session-token service or a remote provider bootstrap.

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
   - `Content-Type`: `application/json`
   - `x-site-id`: `BRA_BTL` only when `options.bybitSiteId === 'BRA_BTL'`

Each request gets a unique `requestId` param via `nanoid()`.

The KLINE endpoint is **unsigned** (auth headers stripped for public market data) but must keep site headers such as `x-site-id: BRA_BTL`.

### Bybit Brazil Account Mode

Bybit Brazil accounts use the normal global Bybit REST/WS hosts, but Bybit requires the official `x-site-id: BRA_BTL` header. The user opts into this with `ExchangeAccountConfig.bybitSiteId`, which is forwarded to `ExchangeOptions.bybitSiteId` only for `bybit`, `bybitinverse`, and `bybitspot`.

Current status for beta:

- Functionally implemented in account shape, account creation/editing, sync/runtime fingerprints, REST headers, and Bybit WS connection options.
- Brazil-marked WS connections now resolve through the CTO-provided header-injecting route at `wss://fsjnpy143pbz-tyo.tealstreet.io/bybitbra/...` (a container on the Vultr tunnel hosts, AWS_EXIT step 5b; the hosted exchange server dials the same host):
  - private: `/bybitbra/v5/private`
  - public linear: `/bybitbra/v5/public/linear`
  - public inverse: `/bybitbra/v5/public/inverse`
  - public spot: `/bybitbra/v5/public/spot`
- Covered by focused safe-cex/web tests, package-local typechecks, and root lint during implementation.
- Not venue-validated yet because we do not have a real Bybit Brazil account/API key. Treat the first Brazil-account rollout as beta validation rather than fully proven production behavior.

Rules:

- Keep this as the specific `bybitSiteId` field. Do not replace it with a user-controlled generic header bag.
- Explicitly drop the field for `bybitdemo`; demo uses `api-demo.bybit.com` credentials and has no Brazil KYC/site concept.
- REST requests include the header on signed and unsigned routes. Public KLINE strips auth but preserves `x-site-id`.
- Brazil inverse writes its own order rows from the REST response (`recordRecentlyPlacedOrderFromOpts`, `applyLocalOrderUpdateAfterRestWrite`). Every inverse row goes through `remainingInStoreUnits`: `remaining` in BTC, USD contracts in `extra.remainingNative`. A row holding contracts in `remaining` is drawn on the chart as contracts x price (`2,600,192 USD` for a $1,000 ETHUSD leg).
- WS classes resolve the routing table before falling back to direct Bybit URLs. For `bybitSiteId: 'BRA_BTL'`, the routing table must send browser WS through the dedicated Bybit Brazil proxy because browser-native direct WebSockets cannot set custom upgrade headers.
- Keep the local REST order-lifecycle fallback for `bybitSiteId: 'BRA_BTL'` until Brazil-account WS behavior is venue-validated. REST create/amend/cancel successes locally show, update, and retire chart rows while snapshots/WS catch up. Keep this scoped to Brazil/demo; normal live Bybit still lets private WS own order rows.

Beta validation checklist:

- Add or edit a non-demo Bybit account, enable "Bybit Brazil account", and confirm validation succeeds.
- Confirm REST activity works for the account mode the user actually trades: account info, balance, orders, positions, order history, executions, order placement/amend/cancel, and KLINE.
- Confirm public and private WS behavior separately, including that Brazil accounts connect through the `/bybitbra` proxy paths and receive order lifecycle updates without waiting for manual refresh.
- Do not assume Brazil accounts are spot-only. Product availability is a venue/account-permission question; let Bybit errors prove unsupported derivatives before hiding perps/inverse in the UI.
- Regression-check a normal non-Brazil Bybit account to ensure the unset `bybitSiteId` remains a no-op and does not cause reload/sync churn.

**Retry logic** (`shouldRetryBybitRequest`): Retries up to 3 times on network errors **on reads only** (a lost response may hide an accepted order, so writes are never resent; this replaced a HyroTrader-only `CREATE_ORDER` carve-out) or, ONCE, a rejected signature (`signatureRejectedResponse(isBybitSignRejection)`; a rejection, so safe to resend, writes included), except `MARKETS` (`/v5/market/instruments-info`) network errors, which are not retried by the shared axios retry interceptor. Linear startup owns a single delayed retry for transient Trading-market failures (`timeout`, `network`, `408`, and `5xx`) and deliberately does not retry rate-limit or blocked responses. `isBybitSignRejection` reads the body through `getBybitResponseMeta`, so it matches `retCode`/`ret_code` 10004 and the message in any of the envelope shapes Bybit uses.

**Signature rejections are re-signed once, on whichever path they arrive.** A rejected 4xx goes through axios-retry (`signatureRejectedResponse`, above); the usual case goes through the response interceptor, because Bybit sends `10004` ("error sign!" or, now, "Error sign, please check your signature generation algorithm") enveloped on HTTP 200, which axios-retry never sees. `isBybitSignRejection` matches it and the shared `resendOnSignatureRejection` (`utils/signatureRejection.ts`) resends under `SIGNATURE_RETRY_CONFIG_KEY`, so the request interceptor (or, hosted, the remote signer) signs afresh. Only the resend's failure counts toward the auth streak. In hosted mode the resend reaches the device for a new signature because `remoteSigningAdapter` hands back the config with itself still as its adapter — see `packages/safe-cex/src/exchanges/CLAUDE.md` → "A resend must be re-signed".

**Trading market startup telemetry** (`bybit.exchange.ts`): `BybitTradingMarketsRequestError` must keep the underlying cause classified with a fixed vocabulary (`rate-limited`, `blocked`, `timeout`, `network`, `HTTP <status>`, `unknown`) so Sentry gets real signal without raw Bybit `retMsg` values, proxy IPs, or ban timestamps. The error carries a worker Sentry cooldown key; repeated same-reason startup failures are suppressed for five minutes and the next reported event includes a suppressed-count summary. Keep this scoped to the required Trading markets request, not optional PreLaunch markets.

**Time sync** (`bybit.api.ts` response interceptor): On `recv_window` errors (response `retMsg` containing `'recv'`), resyncs from the `server_timestamp[…]` Bybit names in the rejection (`parseBybitServerTimestamp` → `virtualClock.syncFromServerTime`; falls back to `syncBybitClientServerTime`) and retries the signed request once — rejected, so the retry cannot double an order. A successful retry surfaces nothing. **`syncBybitClientServerTime` used to test `typeof window`, which is undefined in a web worker too**, so in the browser the resync zeroed the offset and the retry failed again (the 2026-10-09 toast); it now skips only on a real Node/Bun server. The periodic `syncClientServerTime` likewise no longer zeroes a worker's offset every 5 minutes (it keeps what the main thread pushed). On a server the parsed correction is NOT applied: there `virtualClock` is one process-wide offset shared by every user's exchange. If the retry also fails, the error is logged and returned to the caller. Also logs server unavailability when the raw response includes "too much traffic or a configuration error".

**HTTP-over-WebSocket tunnel** (`bybit.api.ts:87-90`): When `options.tealstreetTunnel` is set, the Axios adapter is replaced with `createAxiosHttpOverWebsocketAdapter` using the configured `proxyRegion`.

### WebSocket Authentication (`bybit.ws-private.ts`)

1. Sync time with `virtualClock.syncBybitClientServerTime()`
2. Wait 25ms (`bybit.ws-private.ts:39`)
3. Compute `expires = currentTimeMs + 10_000` (10-second expiry window)
4. Sign: `HMAC-SHA256(secret, "GET/realtime" + expires)`
5. Send auth: `{ op: 'auth', args: [apiKey, expires, signature] }`
6. On auth failure (`json.success === false && json.op === 'auth'`), retry up to 3 times with time re-sync (`authRetries` counter at `bybit.ws-private.ts:82-86`)

**`sendReconnectSequence` must `await this.auth()` before `this.subscribe()`, in
all three variants.** Bybit refuses a private `subscribe` on a socket that has
not authenticated. Locally the un-awaited form happens to work — `auth` reaches
its `sendWithLog` with no intervening `await` — but under a remote signer the
signature is a round trip to the paired browser, so an un-awaited call lets
`subscribe` go out first and the `order`/`position` topics are never
established. The orders table then stops updating entirely: a placed order
vanishes when its optimistic row expires, a moved order vanishes the same way,
and a cancelled one stays on screen because no removal event ever arrives. REST
polling keeps tickers, positions and balance looking healthy throughout, which
is what makes it read as "the app is fine, orders are broken".

Measured on bybit's own wire, 2026-10-01: a `subscribe` sent before any `auth`
is answered `{"success":false,"ret_msg":"Request not authorized","op":"subscribe"}`,
and the same subscribe after a successful auth is answered `success:true`. That
refusal IS logged — `onMessage`'s `json.success === false` branch emits it — but
nothing retries, because the auth-retry arm is gated on `json.op === 'auth'`. So
grep for `[WS]subscribe:Request not authorized`.

`b7b6822f95` ("remote signing ... WS auth for all exchanges") added the `await`
to the LINEAR socket only, so hosted bybit USDT worked and hosted BTCUSD did
not — reported as "impossible to use Tealstreet on Bybit's inverse market"
(2026-10-01) and fixed for inverse and spot the same day.

**Fast Execution timestamp precision change** (Bybit announcement: 2026-11-05
02:30 UTC): Bybit's `execution.fast` private WS topic changes `creationTime`
and item `execTime` from millisecond to microsecond precision without changing
field names or types. The retained TS Bybit sockets do NOT subscribe to
`execution.fast` today: linear/demo and inverse subscribe to `order` +
`position`, and spot subscribes to `order` only. So the announcement does not
change the current private WS fill path. Still, any execution-row mapper or
future `execution.fast` consumer must normalize timestamps through
`normalizeBybitExecutionTimestampMs` / `normalizeBybitExecutionTimestampSeconds`
so 10-digit seconds, 13-digit milliseconds and 16-digit microseconds all land in
the same internal seconds/ms units. Demo inherits the linear mapper; inverse has
its own mapper; spot currently has no historical-fill mapper in the retained TS
adapter.

**On a failed auth the socket sends NOTHING and reconnects** — it does not
subscribe. Subscribing anyway was tried and is worse: bybit answers a ping on an
unauthenticated `/v5/private` socket (measured 2026-10-01, 7 pings 7 pongs), so
any frame sets `isInitialized` and keeps `lastReceived` fresh, the 20s staleness
threshold never trips, and a socket with zero topics reports healthy forever.
See `../CLAUDE.md` → "Remote-signer WS auth" for the full contract, including why
the failure message must never contain the words "auth failed".

Gated by `../../__tests__/audit-regressions/bybit.private-ws-auth-ordering.test.ts`
(ordering, the signer REQUEST shape, exact subscribe args per variant, the
local-HMAC path, both signer failure modes, `onOpen` wiring, and that a disposed
exchange never asks the signer). It runs on every PR via root `test:ci`.

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
   If the private WS publishes positions before this REST snapshot finishes,
   merge the REST result with the recent WS-updated store using
   `mergeUpdatesUsingLastUpdate()` before `addOrUpdatePositionsFull()`. The
   follow-up snapshot path already does this; startup must match it or a stale
   empty REST result can briefly clear fresh WS positions on first load.
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

### Direct-path fallback through athena (`WsDirectFallback`)

All six non-demo socket classes (`bybit`/`bybitinverse`/`bybitspot` × public/private)
hold a `WsDirectFallback` (`utils/wsDirectFallback.ts`). A connection that produced
zero messages is a silent failure; after two consecutive silent connects the class
dials `route.fallback` — the athena Caddy passthrough `bybit-proxy-ja.tealstreet.io`
declared in `transport/tealstreet/routing/routingTable.ts` — for the rest of the socket's lifetime and
emits `grafana-info` `Using tealstreet bybit … proxy`. Any message at all (a pong,
an auth error) resets the streak, so a bad key never trips it. The resolver attaches
`fallback` only to a DIRECT resolution: a Brazil (`BRA_BTL`) account is tunnelled
through the header-injecting `bybitbra` path and gets no fallback, because the
passthrough would drop `x-site-id`. Demo and hyrotrader terminate elsewhere and
declare none. It must stay a fallback, never the default: everyone on it shares
one egress IP, and Bybit limits connections per IP.

The three PRIVATE classes walk a chain instead of a single fallback: direct →
`route.relay` (the tradetunnel's `rpc.relay`, dialled via `RelayWebSocket` through
the private proxy fleet so an IP-restricted key still authenticates) → `route.fallback`.
The relay is only offered while `transport/tealstreet/wsRelay` is on (`WS_RELAY_PRIVATE` flag via
`__workerConfig`), and Bybit's cosmetic `?requestId=` is NOT appended to a relay url
because that url already carries the `ts-relay` query.

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
- Secondary parser: Uses `publicTrade` topic for sub-second candle construction. Calculates candle start time via `createLiveCandleStart(opts.interval)` (`utils/calendarCandles.ts`), built once per subscription: the epoch floor for intraday intervals, Monday weeks and calendar months for `1w`/`1M`. It used to epoch-floor `1w` too, which on Thursday-Sunday opened a bogus Thursday bar mid-week and then dropped every later `kline.W` frame as "bleeding backwards" (and on Monday-Wednesday warned on every trade). Supports second-interval (`opts.interval.endsWith('s')`) candle creation from individual trades
- Linear single-symbol OHLCV listeners arm a per-`kline.{interval}.{symbol}` no-data timer. If neither the kline topic nor its paired `publicTrade.{symbol}` handler sees data for 25s while the socket is otherwise alive, re-subscribe that kline/trade pair rather than waiting for whole-socket staleness. Keep `publicTrade` reconnect state ref-counted by active handlers so one interval teardown does not strand another interval after reconnect.

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
| `1M`     | `M`   |

`1M` is not a `Timeframe` (it is an `ExtendedTimeframe`; the map is typed
`Record<Timeframe | '1M', string>`), but the chart asks for it and Bybit serves it
on every category: `M` is the UTC calendar month stamped the 1st 00:00 UTC, `W`
the Monday 00:00 UTC week (REST `interval=M`, WS `kline.M.{SYMBOL}`, checked live
2026-10-10). `bybitspot.types.ts` carries its own `INTERVAL` that nothing imports —
every Bybit variant reads `bybit.types.ts`.

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

6. **Sequential submission**: Lots are submitted sequentially via `mapSeries`. Each failure is logged but does not abort subsequent lots — **except a single-lot order carrying the CALLER's client id whose response was lost** (`utils/unknownSendOutcome.isUnknownOutcome`), which is rethrown in all three variants; an order with a generated id (stops, conditionals, scaled legs) has nothing to look up and still returns `[]`. Returning `[]` there read as "not placed", so a chase placed again on top of a live order; thrown, the caller looks it up by its `orderLinkId`. Gated by `__tests__/faultBench/bybitLostCreate.scenarios.test.ts`.

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

For position-bound TP/SL rows, `mapOrder` must also treat Bybit V5
`createType` as a fallback classifier when `stopOrderType` is temporarily
`UNKNOWN` during reconciliation. `CreateByTakeProfit` /
`CreateByPartialTakeProfit` stay `OrderType.TakeProfit`, and
`CreateByStopLoss` / `CreateByPartialStopLoss` stay `OrderType.StopLoss`.
Without that fallback, the interim row can be mapped as a plain `Limit`, get
snapped to near-touch price by chart/open-order formatting, and appear as a
phantom order until the next snapshot corrects it.

Bybit Demo can also surface position TP/SL rows with a blank/missing `side`,
especially when private order WS is degraded (`1006`) and the UI is relying on
REST snapshots. Keep this as a local mapping recovery, not a reason to proxy demo
private WS: infer the protective closing side from `positionIdx`
(`1 -> Sell`, `2 -> Buy`) or from the single open one-way position. Chart order
PnL labels depend on that side matching the open position. Regression:
`bybitdemo.order-risk.test.ts`.

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

### Cancel Order Logic

- **UNIFIED accounts** (`bybit.exchange.ts:1668-1681`): Use batch cancel (`CANCEL_ORDERS`) with up to 10 orders per batch. Uses `unlimitedXHR` (no rate limiting). Checks `retExtInfo.list` for individual order errors.
- **CONTRACT accounts** (`bybit.exchange.ts:1682-1698`): Fall back to individual `CANCEL_ORDER` calls via `forEachSeries`. Errors are collected and thrown as a joined string.
- **Cancel all for symbol** (`cancelSymbolOrders` at `bybit.exchange.ts:1702-1726`): When `cancelTpSl=false`, preserves TP/SL orders by saving them, canceling all, then re-placing the TP/SL orders with flipped sides.
- Successful generic cancels do not force an order-inclusive REST snapshot.
  Chaser depends on private WS order events; injecting REST snapshots after every
  cancel/create/amend was observed to wipe just-placed Bybit Demo chaser orders
  from the local store and make the chaser stop and cancel itself.
- `cancelOrders` calls `markOrderSeen` for every id up front, so the
  recently-placed grace window can never resurrect an order we deliberately
  cancelled. On success it then calls `retireCancelledOrders`, which removes the
  rows from the store **only for Demo or Brazil account mode** — see "Demo has
  to retire its own rows".
- `"order not exists or too late to cancel"` is treated as success, both as a
  top-level `retMsg` and per-row in `retExtInfo.list`. The chaser cancels at the
  touch, so an order can fill inside the cancel round trip; throwing there made
  the chaser stop and abandon the chase. Genuine cancel errors still throw.
- **Inter-batch delay**: 1000ms between cancel batches (`bybit.exchange.ts:1666`)

## Symbol Format

- **Linear**: Raw symbol from Bybit (e.g., `BTCUSDT`, `ETHUSDC`, `BTC-27JUN25`)
- **Inverse**: Raw symbol (e.g., `BTCUSD`, `ETHUSD`)
- **Spot**: Raw symbol (e.g., `BTCUSDT`, `ETHUSDT`)
- Market ID uses `market.symbol` directly (not the `base/quote:settle` format)
- PreLaunch markets are fetched separately with `status: 'PreLaunch'` and included when they resolve within the startup grace window

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

Bybit Demo can return empty strings for the UTA aggregate margin fields
(`totalAvailableBalance`, `totalMarginBalance`, `totalInitialMargin`,
`totalMaintenanceMargin`) while still returning valid `totalEquity`,
`totalWalletBalance`, and coin rows. Treat those blanks as missing values, not
zero and not `NaN`. In that shape, derive available margin from coin-level
wallet value minus `totalPositionIM`, `totalOrderIM`, `locked`, and `bonus`,
then fall back to wallet/equity totals. The regression is
`bybit.uta-default.test.ts`'s blank aggregate balance case.

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
- Income history: 50 per page (the venue's own cap, clamped), max 10 pages safety limit, 1000-record cap (`fetchBybitIncomeHistory` in `bybit.income.ts`)
- OHLCV: max 1000 candles per request (`bybit.exchange.ts:773`)

## State Reconciliation

### Position Merge Strategy (`bybit.exchange.ts:294-334`)

The `fetchAndSetPrivateSnapshotHelper` method handles merging REST-fetched positions with WebSocket-pushed positions:

1. **Time-based guard**: A 1-second delay (`INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1000`) controls the merge behavior
2. **Recent WS update**: If a WebSocket position update arrived within the last 1 second, `mergeUpdatesUsingLastUpdate(fetchedPositions, storePositions)` is used. This preserves WebSocket data over REST data for recently-updated positions.
3. **No recent WS update**: REST-fetched positions are used directly via `addOrUpdatePositionsFull()`
4. The `lastWebsocketPositionsUpdate` timestamp is set by the private WS `handlePositionTopic` handler (`bybit.ws-private.ts:164`)

### The inverse position topic is UPDATE-ONLY, and newly reachable

`bybitinverse.ws-private.ts:handlePositionTopic` calls
`updatePositions(positions.map((p) => [p, p]))`, which rewrites only entries the
store already holds and returns early when the store is empty
(`store/store.base.ts`). Linear calls `addOrUpdatePositions` instead. So an
inverse position opened while the tab is up does not appear from the WS at all —
it appears on the next REST poll, one tick later.

**This became reachable only when the auth ordering above was fixed**: before
that the inverse private socket never subscribed in hosted mode, so the handler
never ran. A tick is ~500ms–1s, so the gap is latency rather than wrongness, and
it is deliberately left as it is. Three things have to be settled before
switching it to `addOrUpdatePositions`, and all three were got wrong on a first
attempt that was reverted:

- **Scope it on the frame's own `category`, not on `store.marketsBySymbol`.**
  Bybit's v5 private stream is ONE socket per account (all three variants share
  `/v5/private`) while `fetchPositions` here asks for `category: 'inverse'`, so
  an unscoped add can put a USDT linear position in the inverse store — where
  `mapPosition`'s `market?.isInverse` branch is skipped, because the linear symbol
  is absent from the inverse store's `marketsBySymbol`, so its size is read as
  base units. A market-list filter looks
  equivalent and is not: `waitForExternalPublicDataLoaded` RESOLVES on a 30s
  timeout with markets empty, and `store.update({ markets })` clears and
  repopulates `marketsBySymbolMap`, so the filter can silently drop every real
  position with no log and no counter. Whether the payload carries `category`,
  and whether one socket really does mix categories, are both UNMEASURED — one
  frame dumped from a UTA account holding BTCUSD plus a USDT perp settles both.
- **A flat one-way frame has no usable side.** `mapPosition` resolves
  `positionIdx === 0` through `POSITION_SIDE[p.side] || PositionSide.Long`, and
  `POSITION_SIDE` maps only `Buy`/`Sell`, so a flat frame becomes a phantom
  `BTCUSD:long:oneway` row while the real short keeps stale contracts. Only
  `addOrUpdatePositionsFull` zeroes unannounced rows, and nothing dedupes the
  phantom away — its id differs from the real row's, so `auditPositions()` would
  not remove it even with `removeDupes` on. Linear has
  `dedupePositionsById` for its documented `seq: -1` placeholder rows; inverse
  has no equivalent.
- **Do NOT also sync the balance here.** `syncBalanceUpnlFromPositions` is safe
  on the linear class because linear `unrealizedPnl` is quote-denominated.
  Inverse leaves it BASE-denominated (`packages/safe-cex/CLAUDE.md` → the inverse
  rule), and `formatBalance` compensates by deriving `total = totalEquity - upnl`
  so the pair is consistent at fetch time. Writing `upnl` per frame without
  recomputing `total` breaks that identity: a 10,000-contract BTCUSD position
  moving 1% at $83,000 is a true equity change of about $100 and would render as
  about $0.0012, and equity would then sit frozen between balance polls instead
  of snapping to an authoritative `totalEquity`. It is also a no-op on a CONTRACT
  (classic) account, which is where inverse users are most likely to be.

**`bybitspot` has no position handling at all, and that is correct.**
`bybitspot.exchange.ts`'s `fetchPositions` is `async () => []` unconditionally and
both `addOrUpdatePositionsFull` sites are fed by it, so `store.positions` is
permanently empty. The handler that read it, its `onMessage` dispatch, the
`position` subscribe arg, `mapPosition`, `lastWebsocketPositionsUpdate` and its
merge branch, the per-ticker uPnL loop in `bybitspot.ws-public.ts` and
`formatBalance`'s `upnl` reduce were all therefore dead and were deleted; spot's
subscribe is now `['order']` alone and `Balance.upnl` is the literal `0`. Do not
"restore" the position topic: the socket is the shared `/v5/private`, so it would
deliver derivatives positions into a spot store.

### Order Updates from WebSocket (`bybit.ws-private.ts:98-158`)

The private WS `handleOrderTopic` processes order lifecycle events:

1. **PartiallyFilled guard** (`bybit.ws-private.ts:105-109`): Filters out false-positive partial fill events where both price and amount are <= 0 (occurs during order replacement)
2. **Fill events**: Emitted on `Filled` or `PartiallyFilled` via `emitter.emit('fill', ...)`.
   **The payload comes from `resolveBybitExecution` (`bybit.fill-event.ts`), NOT from
   the order's `price`/`qty`** — those are the order's LIMIT and SIZE. On a market
   order bybit fills `price` with a slippage-protected bound, so fills were reported
   ~10% adverse in both directions (a buy at $0.057 against a $0.0519 market, sells at
   $0.047), and `qty` announced a partial fill as the whole order. Read `avgPrice` and
   `cumExecQty`, exactly as `mapOrder`, the fills mapper and `bybit.order-history.ts`
   already do — this event was the one place that disagreed. `avgPrice` is `""` until
   something fills, so parse fallibly: `parseFloat('')` is `NaN` and renders as a fill
   at no price at all. The PartiallyFilled guard above deliberately still reads the raw
   fields, so its behaviour is unchanged.
3. **Terminal states** (`Cancelled`, `Filled`, `Deactivated`, `Rejected`): Orders (including virtual SL/TP) are updated with `Closed` or `Canceled` status
4. **Reject reason filtering**: `EC_PerCancelRequest` (user cancel) and `EC_NoError` are excluded from error logging
5. **Active states** (`New`, `Untriggered`, `PartiallyFilled`): Orders are added or updated in the store via `addOrUpdateOrders()`

### Reinit Flag (`needsReinit`)

Set to `true` when:

- `fetchOrdersHelper` fails — a venue error OR a request that THROWS
- `fetchPositionsHelper` fails without symbols, or its request throws

The throw case is the hosted remote signer being unavailable or timing out
(`reinitOnThrow`); before it was covered, a failed startup order fetch stayed
empty until a private order frame happened to arrive. When set, the next tick
performs a full `fetchOrdersPositionsAndBalance()` instead of the normal
positions+balance-only snapshot. **The tick clears the flag BEFORE that refetch**,
because the refetch swallows its own errors: clearing it after meant a failed
reinit was retried exactly once. **It backs off from 15s to 5min
(`REINIT_RETRY_MS` doubling, reset by a reinit that succeeds)**: a persistent
failure such as a key missing the order permission otherwise refetched — and
toasted — on every 500ms tick. Inverse and spot carry the same helpers and the
same tick. Gated by `__tests__/audit-regressions/bybit.reinit-on-signing-failure.test.ts`.

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

The `fetchMarketsHelper` method starts **two** market requests:

1. Normal markets: `{ category, limit: 1000 }`
2. PreLaunch markets: `{ category, limit: 1000, status: 'PreLaunch' }`

Normal Trading markets are required. PreLaunch markets are optional during startup: they are merged when they resolve within the short startup grace window, otherwise startup continues with Trading markets only. Active markets have `status === 'Trading' || status === 'PreLaunch'`.

**Note**: Inverse variant fetches markets sequentially, not in parallel (`bybitinverse.exchange.ts:754-759`).

### `symbolType` is the asset-class field, and `innovation` is CRYPTO

`/v5/market/instruments-info` publishes `symbolType`, and `fetchMarketsHelper`
preserves the whole instrument as `Market.info`, so classifying an instrument
costs no request. `instrument-class/bybit.instrumentClass.ts` maps it; measured
live 2026-09-29 across 886 linear instruments:

| `symbolType` | count | asset class | examples                           |
| ------------ | ----- | ----------- | ---------------------------------- |
| `''`         | 502   | crypto      | BTCUSDT, SOLUSDT                   |
| `stock`      | 196   | equity      | AAPLUSDT, SHOPUSDT                 |
| `innovation` | 127   | **crypto**  | AEONUSDT, 1000TAGUSDT              |
| `ETF`        | 54    | etf         | ARKKUSDT, EWJUSDT                  |
| `commodity`  | 4     | commodity   | XAUUSDT, XAGUSDT, CLUSDT, BZUSDT   |
| `forex`      | 3     | fx          | EURUSDUSDT, GBPUSDUSDT, USDJPYUSDT |

All 26 inverse instruments are `''`.

**Two rules that look right and are wrong.** "A non-empty `symbolType` means
TradFi" mislabels the 127 `innovation` markets — Bybit's innovation zone is new
or higher-risk TOKENS — so anything warning off that cries wolf on one linear
market in seven. And `marketRegion` cannot stand in: it is populated on exactly
the stock and ETF rows (219 US + 21 HK + 8 KR + 2 CN = 250 = 196 + 54) and is
empty on commodity and forex, so keying off it misses gold and FX entirely.

An unrecognised value is reported verbatim as `unknown`, never assumed crypto —
a new TradFi type guessed as a perp is the failure the module exists to prevent.

### Open feature: contract tradeability probes

**Probe 1 below is now implemented** as `instrument-class/` — what an instrument
IS. Probe 2, whether the ACCOUNT may trade it, is still open and is the harder
half; nothing in the classifier may be read as answering it.

There is no CLI command today that answers "is this contract tradeable?" beyond
`markets`, which only prints the loaded symbols. The adapter already has enough
public metadata for a first pass, because `fetchMarketsHelper` preserves the
raw `/v5/market/instruments-info` object as `Market.info`.

Think of future support as two separate probes:

1. **Public instrument status** from cached market metadata. No new REST request
   is required after startup. A CLI command such as `market XAUUSDT` or
   `markets --info XAUUSDT` could find the symbol in `ctx.exchange.store.markets`
   and print:
   - `info.status` (`Trading`, `PreLaunch`, etc.) and our `market.active`
   - `info.symbolType`, `info.displayName`, `info.fullName`
   - TradFi fields: `info.marketRegion`, `info.underlyingTicker`
   - pre-listing fields: `info.isPreListing`,
     `info.preListingInfo?.curAuctionPhase`
   - limits: `lotSizeFilter.minOrderQty`, `maxOrderQty`,
     `maxMktOrderQty`, `minNotionalValue`, plus `priceFilter.tickSize`
   - risk metadata: `riskParameters`, `unifiedMarginTrade`

   If the CLI needs a live refresh rather than the startup cache, add an
   optional `fetchMarkets?` method to `cli-core`'s `ExchangeLike` or bridge a
   worker command. Do not assume `marketsBySymbol` exists in the CLI type;
   today it exposes only `store.markets`.

2. **Account/order eligibility** through Bybit's signed pre-check endpoint.
   Bybit documents `POST /v5/order/pre-check`, with request parameters matching
   create-order. This is not wired in `ENDPOINTS` today. A future
   `preCheckOrder` / `checkTradeability` method should reuse the exact order
   serialization path used by `placeOrder` / `formatOrderForBatch`: category,
   precision adjustment, `positionIdx`, trigger/TP/SL restrictions, BBO/slippage
   validation, and HyroTrader's required `orderLinkId`. It must not call
   `recordRecentlyPlacedOrder`.

   Minimal linear probe shape:

   ```json
   {
     "category": "linear",
     "symbol": "XAUUSDT",
     "side": "Buy",
     "orderType": "Limit",
     "qty": "0.01",
     "price": "2500",
     "timeInForce": "GTC",
     "positionIdx": 0,
     "reduceOnly": false,
     "orderLinkId": "<generated only where required>"
   }
   ```

   Response interpretation:
   - `retCode: 0` means the hypothetical order passed the check.
   - `retCode: 110007` is still a structured margin result; Bybit says
     `retExtInfo` may include post-check IM/MM fields even when balance is
     insufficient.
   - Other non-zero `retCode` values should be surfaced through the same Bybit
     venue-error normalization as real order placement. This should catch
     agreement rejections (`110123` / `110125`) without placing an order.

   Limits and caveats from Bybit docs:
   - Signed/private endpoint; do not run it from public-only sessions.
   - Supports `linear`, `inverse`, and `option`; conditional orders are not
     supported.
   - Only Cross Margin and Portfolio Margin modes are documented; isolated
     margin is not supported. `inverse` is not supported in Cross Margin mode.
   - Demo and HyroTrader support is unproven. HyroTrader may not proxy this
     endpoint even if order placement works.
   - The Web UI warnings for TradFi products may combine public metadata,
     trading-session calendars, and account/order eligibility. Do not claim the
     public instrument status alone proves an account can open exposure.

`/v5/user/agreement` is not a status endpoint. It signs legal terms, requires a
master account with transfer/withdraw-class permissions, and should remain
outside any passive CLI "can I trade this?" query.

### Fee Rates

Fetched per category via `fetchFees()` before market loading (`bybit.exchange.ts:681-698`). Fee rates are matched by symbol and attached to the market object as `takerFee` and `makerFee`. Silently returns empty array on failure (non-blocking).

**Spot variant**: Does not fetch fee rates.

### Income History / Transaction Log

- Uses `/v5/account/transaction-log` endpoint
- Only supports `accountType: 'UNIFIED'`
- Bybit limits queries to **7-day windows**. `fetchBybitIncomeHistoryRange` (`bybit.income.ts`)
  chunks on its own `SEVEN_DAYS_MS`, a FULL seven days, with a 100ms delay between chunks, and
  advances `currentStart = currentEnd + 1` because the venue treats both bounds as inclusive — a
  shared endpoint returns the boundary row twice and it cannot be deduped afterwards, since bybit
  tranIds are not unique. `ORDER_HISTORY_MAX_WINDOW_MS` (7 days minus a millisecond, in
  `bybit.types.ts`) is a DIFFERENT guard: it splits the inner per-call window for direct callers.
  So a full-7d chunk re-splits into two requests, the second covering a single millisecond.
- Paginated with cursor, max 10 pages safety limit
- **`category: 'inverse'` is VALIDATED against a live UNIFIED account** (2026-09-25, sheet row 6,
  `yarn history-audit --ledger-probe --targets "bybitinverse:6:30" --proxy`). A real 5-contract
  BTCUSD round trip produced two `TRADE` rows carrying `symbol: 'BTCUSD'`, which
  `fetchIncomeHistoryRange` mapped to two `COMMISSION` records, which enumerated to `['BTCUSD']`,
  which `fetchHistoricalFills` then answered with both fills. That whole chain is the fill-arrow
  path, so it is proven rather than inferred. An account that has never traded inverse answers
  `retCode 0` with an empty list — accepted and idle, NOT refused; the probe now prints which of
  the two it saw, because they are the same value to the mapper.

**`accountType` is hardcoded UNIFIED on BOTH variants, and that is not an oversight.** The
transaction log serves UNIFIED accounts only. `BybitInverseExchange` has an `accountType` getter
returning `CONTRACT` when `unifiedMarginStatus === 1`, and wiring it into the income context looks
correct right up until the venue answers a non-zero retCode — which this path swallows into an
empty list, indistinguishable from "this account traded nothing". **A classic (CONTRACT) account
therefore collects no inverse fills, silently.** Linear swallows identically, so this predates the
inverse route; the honest fix is to throw, which `enumerateTradedSymbols` already documents it
expects ("a failed income read THROWS rather than returning an empty set"), or to route classic
accounts at `/v5/account/contract-transaction-log`. Not done. Note the context type still ADMITS
`'CONTRACT'`, so the type will not stop you.

**One transaction row becomes several `IncomeRecord`s.** Bybit reports `cashFlow`,
`funding` and `fee` as three independent terms on the same row, so mapping a row to a
single record merges terms of the reconciliation identity. The mapper is the pure,
exported `mapBybitTransactionToIncomeRecords` in `bybit.income.ts` — kept out of the
exchange class because an instance-field parser cannot be unit tested. That file now owns the
HTTP route as well: `fetchBybitIncomeHistory` / `fetchBybitIncomeHistoryRange` take a
`BybitIncomeContext` of `{ xhr, log, category, accountType }`, so linear and inverse share one
implementation. Inverse cannot inherit it — `BybitInverseExchange` extends `BaseExchange`, not
`BybitExchange` — and a second copy would drift.

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

### Demo Trading (`bybitdemo.exchange.ts`, `hyrotrader.exchange.ts`)

`BybitDemoExchange` is a minimal subclass of `BybitExchange` with only two changes:

- `isDemo = true` -- triggers demo URL selection for REST and the private WebSocket; public data intentionally uses mainnet
- `get supportedCategories()` returns `['linear']`

`HyrotraderExchange` is a first-party Bybit-compatible simulator/prod variant. It
keeps the Bybit parsers/order lifecycle, but uses explicit URL override hooks
instead of pretending its exchange name is `bybitdemo`:

- Production/user-pasted challenge accounts use
  `NEXT_PUBLIC_HYROTRADER_PROD_REST_URL`,
  `NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL`, and
  `NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL`. The WebSocket URLs are optional
  and derive from the REST origin as `/v5/public/linear` and `/v5/private`.
  While Hyro's production HTTPS/WSS origin is not live, these accounts fall back
  to the configured `NEXT_PUBLIC_HYROTRADER_SIMULATOR_*` origins but still stay
  `hyrotraderMode: 'production'` / `isDemo = false` for grading, journaling and
  hosted-server nudges. Do not use `bybitdemo` for Hyro simulator keys; it talks
  to Bybit's demo host, not Hyro.
- Metadata keeps HyroTrader **dev-only** (`enabled: false`, `enabledInDev: true`,
  `enableInPreview: false`) until private REST/WS can run from preview/prod via
  Hyro HTTPS/WSS origins or a Tealstreet relay. Public market-data HTTPS/WSS is
  not enough to validate/link a private account.
- The shared `/hyrotrader-demo` account is explicitly marked with
  `ExchangeAccountConfig.hyrotraderDemo: true`, which maps to
  `options.extra.hyrotraderMode: 'demo'`, sets `isDemo = true`, and uses the
  simulator-only `NEXT_PUBLIC_HYROTRADER_SIMULATOR_*` env family, including the
  bundled shared API key/secret. Production accounts default to
  `hyrotraderMode: 'production'` and may borrow simulator ORIGINS during v1, but
  must never read simulator API keys/secrets as a credential fallback.
- Hyro's simulator origin may be plain HTTP on port `8012`; WS uses the same
  host/port with `ws://.../v5/public/linear` and `ws://.../v5/private`. Do not
  force HTTPS/WSS in probes or config normalization, and keep exact simulator
  IPs out of committed docs/config. Plain `http://` + `ws://` is unreachable
  from any HTTPS origin (mixed-content block is non-overridable in production
  browsers), so direct-browser against the simulator only works from local
  HTTP dev. Any production path from a Tealstreet HTTPS origin needs either
  TLS fronting by Hyro or a Tealstreet-origin proxy. Hyro confirmed on
  2026-10-15 that HTTPS/WSS should already be available on the main deployment;
  the separate test server was not configured yet and was awaiting their DevOps
  team, so do not add a relay scaffold unless the TLS route remains unavailable.
- Public market data should use Hyro's own public REST/WS endpoints directly as
  the baseline. Do not proxy public market-data traffic unnecessarily, and do not
  assume direct Bybit L2 is identical to the simulator's served book.
- **A HyroTrader `retCode: 10003 "API key is invalid"` is not a reliable
  signal that credentials are wrong.** Hyro deactivates the account on
  their end when the user's challenge expires or is disabled (observed
  2026-10-15: Hatef reactivated the test challenge and the same
  key/secret pair started returning `retCode: 0` again, no credential
  change on our side). The generic auth-invalid pattern in
  `health/errorPatterns.ts` catches it as a critical credential error
  and raises the auth badge, which is user-safe but misleading: it
  reads as "rotate your key" when the user cannot fix it from their
  side at all. The Bybit live/demo mismatch probe in `bybit.api.ts` is
  correctly gated on `exchange.name === 'bybit' || 'bybitdemo'` and
  does not run for `hyrotrader`, so there is no automatic
  disambiguation here either.
  Per Hatef (2026-10-15), the current and near-term production flow is
  user-paste-key: a trader copies their challenge's API key and secret
  from HyroTrader and enters them into Tealstreet like any other
  exchange. OAuth is a future convenience that automates the paste
  step; it does not change the underlying signing scheme or create
  partner service keys. So the UX branches for `retCode 10003` are:
  - **Paste-key accounts (today's prod, CLI, and any user who manually
    added hyrotrader)**: route to contact-Hyro + check-challenge-status
    guidance. Do not auto-trigger a re-paste flow for a key that has
    not actually changed. The user cannot fix it from their side.
  - **Shared `/hyrotrader-demo` bundled-key account**: surface as
    "demo temporarily unavailable" and raise an internal alert — no
    user action possible because rotating the bundled key does nothing
    when Hyro has deactivated it.
  - **OAuth-linked accounts (v2, not yet shipped)**: route to a
    Reconnect Hyro flow, driven by Hyro's authenticated status
    endpoint which they'll poll every 30 s and surface revocation
    within 30 s. Preferable to inferring state from `retCode 10003`
    once that lifecycle signal exists.
  Deferred OAuth/server-held credential design lives in
  `docs/plans/HYROTRADER_OAUTH_V2.md`; do not implement a browser-held
  OAuth credential path. Sam's current direction is that OAuth-linked HyroTrader
  private trading should run through `services/exchange-server` because every
  private request may need Tealstreet-held signing material. OAuth has no known
  end-user launch timeline yet.
- reconciliation is USDT-only (`linear`, `settleCoin=USDT`) because the simulator
  contract does not support Bybit's USDC linear branch
- order placement sends a generated `orderLinkId` because the simulator requires
  idempotent client order IDs; regular Bybit remains unchanged
- Hyro validates through the UNIFIED wallet, keeps one-way/cross settings, and
  skips API-key/fee metadata. Multiple placements and cancellations use individual
  endpoints; batch endpoints are outside the simulator contract.
- Hyro income-history methods carry the base unsupported-capability brand because
  transaction-log is unavailable. Explicit Bybit BBO/TickSize controls are rejected.
- Hyro create recovery disables transport retries, queries realtime/history after
  uncertain outcomes, and reuses the original ID only after both axes show absence.
  Failed/incomplete queries keep the outcome unknown and block replacement submission.
- Pending split/list submissions retain accepted siblings and unresolved IDs for the
  adapter lifetime. Concurrent identical submissions share recovery; after completion
  an identical new submission is independent. Definitive rejection permits a fresh ID.
- Duplicate client-ID responses preserve the original identity and require reconciliation.
  Readiness is checked immediately before every create dispatch, including split siblings
  and retries; a recovery pause preserves accepted siblings and pending IDs.
- Recovered open/partial orders publish the authoritative row, including filled/remaining
  quantity and amended price. Private order sightings during an active submission prevent
  late acknowledgements or stale lookups from overwriting newer state, even if a terminal
  row has since left the store. Matching client-ID observations persist with unresolved
  groups between caller retries and are cleared on resolution/rejection/disposal.
- An acknowledgement crossing a private session or authoritative orders-snapshot boundary
  is reconciled before publication. An already acknowledged identity is never re-created;
  complete lookup absence returns that ID without an optimistic open row.
- Recovery lookups and group publication check that same boundary. A change surfaces an
  unknown outcome and retains any discovered identity for caller reconciliation; stale
  responses cannot overwrite a newer snapshot or trigger another create for a known ID.
- A recovered terminal order returns its ID without becoming an optimistic open row.
  Unresolved IDs are included in errors; reconnect/restart requires authoritative REST
  state, and callers must not blindly replace an unresolved order after recreating the adapter.

### HyroTrader simulator recovery and acceptance

The supported subset follows the partner's October 6 simulator contract. Wallet
validation replaces `/v5/account/info`; API-key metadata, fee-rate metadata,
transaction-log, batch operations, mode switching and margin adjustment are unavailable.
Only USDT perpetuals, one-way positions (`positionIdx=0`) and cross margin are supported.
Slippage protection uses the simulator-configured gate. Explicit `maxSlippagePct`
and Bybit BBO/TickSize options are rejected because the contract does not establish
client-defined protection. Plain market orders remain supported.
Optional simulator preview/evidence endpoints are not used by this connector.
Hyro conditional creates follow Bybit semantics but the simulator JSON contract is
stricter about scalar types: send `triggerDirection` as numeric `1`/`2`, not a
string.

`hyrotrader.ws-private.ts` waits for correlated auth and subscribe acknowledgements,
then reconciles all pages of current orders/positions and the UNIFIED wallet before
opening the event gate. The snapshot publishes atomically; failed, malformed, stale
or incomplete responses preserve prior state and leave recovery unhealthy. Position
rows require valid leverage for open size, and mapped numeric fields must be finite
before publication. Zero-size clear rows may omit leverage; map them with the
existing symbol leverage (or `1`) so close events clear state without publishing
`NaN`. Open position rows may use either `avgPrice` or `entryPrice` for the entry
price alias. Frames arriving during recovery invalidate the in-flight snapshot and
trigger a bounded re-read. A failed handshake or unsettled snapshot reconnects with
visible errors. Creates are refused while an established private session is recovering.

Public streams carry periodic snapshots. Hyro candles use authoritative kline volume
and never accumulate publicTrade snapshots; seconds candles are unavailable. Bybit's
trade-derived candle path is unchanged. Orderbook snapshot/delta and ticker parsing
remain shared, but Hyro public endpoints are the venue source of truth for this
adapter; do not swap in direct Bybit L2 as an assumed-equivalent fallback. Hyro's
public WS rejects oversized subscribe batches with `at most 32 topics per
connection`. The shared ticker/reconnect subscribe path already chunks at 10
and is safe for both venues; the bulk kline subscribe (`listenOHLCVs`) stays at
50 for ordinary Bybit livenet and is overridden to 10 on `HyrotraderExchange`
via `publicKlineSubscribeChunkSize`. Do not blanket-shrink the kline chunk on
the shared Bybit path.

Configure the web build before enabling the connector:

| Setting                                                                                    | Purpose                                                                                               |
| ------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| `NEXT_PUBLIC_HYROTRADER_PROD_REST_URL`                                                     | Required production/challenge REST origin for ordinary `hyrotrader` accounts; no fallback to Bybit    |
| `NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL`                                                | Optional production public socket URL; defaults to REST-derived `/v5/public/linear`                   |
| `NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL`                                               | Optional production private socket URL; defaults to REST-derived `/v5/private`                        |
| `NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL`                                                | Shared demo/probe simulator REST origin, including plain `http://<host>:8012`; no fallback to Bybit   |
| `NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL`                                           | Optional simulator public socket URL; defaults to REST-derived `/v5/public/linear`                    |
| `NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL`                                          | Optional simulator private socket URL; defaults to REST-derived `/v5/private`                         |
| `NEXT_PUBLIC_HYROTRADER_SIMULATOR_API_KEY` / `NEXT_PUBLIC_HYROTRADER_SIMULATOR_API_SECRET` | Existing branded-demo defaults, bundled publicly; only a disposable shared demo account belongs here  |
| `HYROTRADER_ENABLED`                                                                       | App feature flag controlling connector/demo access; runtime overrides may differ from source defaults |

Until Hyro's production origin is live, ordinary `hyrotrader` accounts use the
simulator origin when `NEXT_PUBLIC_HYROTRADER_PROD_REST_URL` is unset. This is
still not a Bybit fallback, and only the `/hyrotrader-demo` account reads the
simulator API key/secret envs.

Normal connected accounts supply their own credentials through exchange options,
not environment variables. During v1 they may use the simulator origin with
user-pasted challenge keys, but only the shared `/hyrotrader-demo` account should
read the simulator API key/secret envs.
All `NEXT_PUBLIC_*` settings are fixed at build time. Changing deployment settings
requires a new web build. Keep account credentials out of committed files and logs.

The live acceptance sequence requires the configured simulator origin and a
simulator account permitted to trade:

1. Fetch server time, instruments, tickers, kline and book; confirm USDT-only markets
   and periodic snapshot semantics without duplicate candle volume.
2. Validate the UNIFIED wallet, orders and positions; prove REST and private WS
   signatures against the actual host, including rejected auth and subscribe failures.
3. Place/amend/cancel a small resting limit order, exercise market execution,
   leverage and position TP/SL, then close and reconcile fills, PnL and wallet.
4. Interrupt a create response and query realtime plus history before retrying its
   original `orderLinkId`; prove the accepted order is neither lost nor duplicated.
5. Force a private disconnect; require fresh auth/sub ACKs and a complete REST
   snapshot before accepting private events or placing another order.
6. Verify the effective feature flag, signed-out branded-demo startup and
   `?code=hyrotrader` referral attribution through the existing RefWorker/set-ref flow.

Local regressions cover steps 1–5 with contract fixtures, exact transmitted HMAC
signatures, and a server-initiated native WebSocket close. They do not establish live
host compatibility or referral conversion reporting. The partner referral code's
existence alone does not prove end-to-end signup attribution.

Scoped gates from the repository root:

```sh
yarn workspace @tealstreet/safe-cex vitest run src/__tests__/audit-regressions/hyrotrader.contract.test.ts src/__tests__/audit-regressions/hyrotrader.order-recovery.test.ts src/__tests__/audit-regressions/hyrotrader.private-recovery.test.ts src/__tests__/audit-regressions/hyrotrader.public-snapshot.test.ts src/__tests__/audit-regressions/hyrotrader.signing.test.ts src/__tests__/audit-regressions/hyrotrader.configuration.test.ts
yarn workspace @tealstreet/safe-cex typecheck
yarn workspace @tealstreet/safe-cex lint
```

### URL Differences

| Component  | Livenet            | Testnet                    | Bybit Demo              | HyroTrader       |
| ---------- | ------------------ | -------------------------- | ----------------------- | ---------------- |
| REST       | `api.bybit.com`    | `api-testnet.bybit.com`    | `api-demo.bybit.com`    | env-configured   |
| Public WS  | `stream.bybit.com` | `stream-testnet.bybit.com` | `stream.bybit.com`      | env/REST-derived |
| Private WS | `stream.bybit.com` | `stream-testnet.bybit.com` | `stream-demo.bybit.com` | env/REST-derived |

URL selection logic:

- Bybit-compatible subclasses can override `getBybitRestBaseUrl`,
  `getBybitPublicWsUrl`, and `getBybitPrivateWsUrl`
- Demo fallback checks stable exchange identity plus `exchange.isDemo`
- Testnet: checks `options.testnet`
- Livenet: default

Constructor-order guardrail: `BybitExchange` creates REST clients inside its own
constructor, before subclass field initializers such as
`BybitDemoExchange.isDemo = true` have run. Built-in demo URL selection must
therefore key off the stable exchange identity (`name === 'bybitdemo'`) as well
as `isDemo`; otherwise demo keys are signed against `api.bybit.com` and Bybit
returns `10003 API key is invalid`. Bybit-compatible subclasses that need a
custom origin should override the URL methods, because prototype methods are
available during parent construction.

### Feature Limitations

- Demo testnet fallback: `DEMO_URL.testnet` falls through to `api-testnet.bybit.com` (same as regular testnet)
- Demo only supports `linear` category (no inverse or spot)
- HyroTrader only supports `linear` USDT perpetuals and a documented subset of
  Bybit V5 endpoints. Do not probe unsupported inherited endpoints, even best-effort.
- Demo's private WS drops `order` frames. `recordRecentlyPlacedOrder`
  therefore upserts just-placed **resting** rows into the store on `isDemo`
  only (`this.store.addOrUpdateOrders([order])`). Without it a limit/chaser row
  may never reach the store at all: the UI shows nothing and the Chaser
  supervisor idles beside an unmanaged resting order, so a second click places a
  DUPLICATE. Plain `OrderType.Market` is excluded: Bybit returns an `orderId`
  for market orders, but they execute as IOC and must not become local open rows
  that demo has no private frame to retire. Live Bybit keeps its WS-driven flow
  untouched.
- Chaser gates on demo private-WS health at **startup only**, and only when the
  socket is genuinely dead (reconnect-paused, or never initialized after a short
  grace window). See `webworker/chaserOrder/CLAUDE.md`; do not restore the
  per-store-update runtime check, which killed live chases on routine `1006`
  blips, and do not treat `privateHealthy === false` alone as fatal.
- Do not reintroduce a generic create/amend/cancel REST snapshot backstop to make
  Bybit Demo Chaser work during private-WS degradation. That path caused
  snapshot/order-event races. The correct protection is the recently-placed-order
  shield below, not more snapshots.

### Recently-placed order shielding

`/v5/order/realtime` lags order creation, worst on `api-demo`. Several paths
wholesale-replace `store.orders` from it — private-WS reconnect housekeeping
(`BaseWebSocket.trackConnected` re-open resync), a `needsReinit` startup refetch, and
the `SET_TRADING_STOP` backstop. A snapshot fetched before the venue indexed a
just-placed order used to erase it from the store; the Chaser supervisor saw its
live order "disappear", stopped itself, and cancelled that order on the exchange.

`recentlyPlacedOrders` holds each just-placed id, and `fetchOrders` runs its
result through `preserveRecentlyPlacedOrders`, so every wholesale replacement
honors the shield regardless of what triggered the fetch. Rules that make this
safe rather than dangerous:

- An id leaves the map on its **first sighting anywhere** — REST snapshot,
  private-WS frame (`markOrderSeen` in `bybit.ws-private.ts`), or our own cancel.
- Protection also expires after `RECENTLY_PLACED_ORDER_GRACE_MS` (8s).
- **Never widen this to already-sighted orders.** Once an order has been seen, a
  later absence means filled or cancelled and MUST be honored, or the chaser will
  re-place on top of a fill and overfill the user's target.
- `preserveRecentlyPlacedOrders` prefers the live store row over the constructed
  placement row, so observed fills are not rolled back.

Guarded by `__tests__/audit-regressions/bybit.recently-placed-order-preservation.test.ts`.
That file and the code it covers were both silently deleted once by an SDK
mirror-sync commit (`3fd8301610`), which is how this regressed for two weeks —
keep them together.

## Demo has to retire its own rows

`recordRecentlyPlacedOrder` injecting a resting row into the store on demo is
only half a lifecycle. The other half was missing and produced phantom orders.

Plain market orders are not part of this lifecycle. Bybit's create-order
acknowledgement can include an `orderId` for a market order, but the matching
engine treats it as IOC; adding that id to `store.orders` produced phantom
open-order/depth markers after reduce-only market closes when demo never sent a
terminal `Filled` frame.

`handleOrderTopic` is the **only** path that removes an order row, and it needs a
private `order` frame. Demo drops those, and the periodic `tick` calls
`fetchAndSetPrivateSnapshotHelper(false)` — positions and balance, deliberately
not orders. So nothing retired a cancelled row: it was injected at placement and
then outlived the real order indefinitely. Symptoms were a stuck resting order
the venue no longer had, and a cancel button that looked dead, because the UI
removed the row optimistically and the next store broadcast restored it. No error
toast appeared, because Bybit's `"order not exists or too late to cancel"` reply
is treated as success. Every chaser reprice cancels and replaces, so the debris
accumulated one row per attempt.

`retireCancelledOrders` closes it: `markOrderSeen` for every id (so the shield
cannot resurrect them from any cancel path), plus `store.removeOrders` when
`isDemo`. It runs from `cancelOrdersHelper` after a successful lot **and** on the
already-gone branch, and from `cancelSymbolOrders`.

- **Keep local retirement scoped to Demo or `bybitSiteId: 'BRA_BTL'`.** Normal
  live Bybit/Inverse/Spot WS-driven removal owns the row; removing locally there
  would race it.
- **Do not "fix" this with a periodic order refetch instead.** Bybit ticks at
  500ms and `fetchOrders` is cursor-paginated per settle coin.
- `cancelSymbolOrders` snapshots the symbol's ids _before_ the cancel, because it
  re-places TP/SL afterwards and those new rows must survive.
- The sibling `bybitinverse` and `bybitspot` adapters do not subclass
  `BybitExchange`; keep their recently placed preservation, WS `markOrderSeen`,
  and Brazil local REST lifecycle fallback in sync with regular Bybit.
- A genuine cancel failure must leave the row in place — never hide real state.

Still open by design: a manual (non-chaser) demo limit order that _fills_ leaves
its row behind, since nothing attempts a cancel on it. Chaser orders are covered
because `cleanupInstance` and the reprice path both cancel by id.

## `fetchFillsForOrderIds` exists for degraded-WS chasers

Returns executed quantity per order id from `USER_TRADES`, keyed by id. The
chaser's cancel-replace reconciliation and the Bybit Demo REST fallback both
depend on it: when the private WebSocket never delivers order frames, this is
the only way to tell a filled order from a vanished one.

The chaser replaces orders on the strength of this result, so **every failure
mode has to be loud**. An under-report is indistinguishable from "no fills" and
makes the chaser replace on top of a fill, overfilling the user's target.

- **A symbol it cannot resolve throws, it is never skipped.** The symbol comes
  from `store.orders`, then `retiredOrderSymbols`, then `recentlyPlacedOrders`.
  The middle one exists because the chaser cancels, reconciles, then replaces —
  and on demo the cancel retires the row, so a store-only lookup found nothing
  and silently skipped every id. That returned an empty map and overfilled.
- **Executions are paged to exhaustion**, capped by
  `FILL_RECONCILIATION_MAX_PAGES`; blowing the cap throws. A single-page read
  under-reports an order filled in more than 50 tranches.
- A non-`OK` `retMsg` throws rather than reporting zero fills.
- Rows are filtered by exact `orderId` match; `USER_TRADES` can return
  neighbouring executions for the symbol.
- Cost is one paged `USER_TRADES` walk per requested id, sequential via
  `mapSeries`. Normally one id per reprice.

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
- The web `createStopLossOrder` wrapper treats this empty result as accepted for
  Bybit-family stop-like orders only. Keep that carveout paired with this
  adapter behavior; reverting it makes successful `trading-stop` calls look like
  failed placements while the real stop appears moments later in open orders.
- Repeating the same position TP/SL can return `"not modified"` from
  `SET_TRADING_STOP`. Treat that as a successful no-op rather than an exchange
  error and keep the linear/demo snapshot refresh; the BE button can
  legitimately submit the same entry-price stop twice.

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
- **Success is `retCode === 0` with empty `reasons`. Never compare `retMsg`.**
  Live wire 2026-10-09: on an account already in `REGULAR_MARGIN`, setting
  `REGULAR_MARGIN` answered
  `{"retCode":0,"retMsg":"Request accepted","result":{"reasons":[]}}`, so a
  no-op is a success, not a "not modified" error (unlike leverage's 110043). A
  third-party SDK's success fixture (WSS-Group/BybitApi, Feb 2025) records the
  same body. A real cross/isolated switch has not been captured yet.
  `88692e3afb` (2026-07-15) moved off the deprecated per-symbol
  `/v5/position/switch-isolated` but kept its `retMsg === 'OK'` check, so every
  call threw before setting `marginType` and the UI kept the old mode until a
  reload.

### WebSocket Position Update Invalidation

A 1-second delay (`INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY`) is applied when merging REST-fetched positions with WebSocket-pushed positions. If a WebSocket position update arrived within 1 second, `mergeUpdatesUsingLastUpdate` preserves the WS data over REST data.

### Time Synchronization

- REST: `virtualClock.getCurrentTimeMs()` for timestamps
- If recv_window errors occur, auto-syncs via `virtualClock.syncBybitClientServerTime()` and retries once
- WebSocket: Syncs time before auth on every connection/reconnection

### Spot-Specific: there is NO position PnL pass

`bybitspot.ws-public.ts` used to recompute unrealized PnL for matching positions on
every ticker frame. It was deleted: `fetchPositions` is `async () => []` and both
`addOrUpdatePositionsFull` sites are fed by it, so `store.positions` is permanently
empty and the loop could never match anything. `Balance.upnl` is the literal `0`
for the same reason — treat it as "spot has no unrealized PnL", not as a measured
zero.

One dead reader survives on the ORDER path and was deliberately left for its own
change: `bybitspot.exchange.ts`'s `_hasFullSizedPositionOppositeSide` reads
`store.positions` and is therefore always `false`, so `placeOrder` can never route
a spot SL/TP to `placeStopLossOrTakeProfit`. Removing it is a change to order
placement rather than to this doc's subject, and it is the last user of that
file's `PositionSide` import.

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

| Error Message / Code                          | Handling                                                                                | Location                           |
| --------------------------------------------- | --------------------------------------------------------------------------------------- | ---------------------------------- |
| `10004` / `/error sign/i`                     | Re-signed and resent once (response interceptor); a rejected 4xx also via `axios-retry` | `isBybitSignRejection`             |
| `"timestamp and recv_window param"`           | Auto time sync via `virtualClock.syncBybitClientServerTime()`, then one retry           | `bybit.api.ts:92-96`               |
| `"accountType only support UNIFIED."`         | Set `forceUnifiedBalance=true`, `unifiedMarginStatus=6`, retry                          | `bybit.exchange.ts:380-383`        |
| Account margin-mode failure                   | Surface `result.reasons[].reasonMsg`, then throw                                        | `changeMarginType`                 |
| `"order not modified"`                        | Treated as success (no-op), no error thrown                                             | `bybit.exchange.ts:1550,1593,1633` |
| `110043` / `"leverage not modified"`          | Treated as success (desired leverage is already active); refresh positions afterward    | `setLeverage`                      |
| `110123` / `110125` Trading Terms agreement   | Reword as actionable order warning; never auto-sign from order placement                | order write paths                  |
| `"Partial symbols switched successfully..."`  | Logged as error but does not throw (partial success on position mode switch)            | `bybit.exchange.ts:1787-1789`      |
| `"All symbols switched successfully."`        | Silent success                                                                          | `bybit.exchange.ts:1786`           |
| Network errors                                | Auto-retry up to 3 times on reads only (via `axios-retry`)                              | `shouldRetryBybitRequest`          |
| `"too much traffic or a configuration error"` | Logged as "Server was unavailable", no automatic retry beyond axios-retry               | `bybit.api.ts:98-100`              |
| Batch order partial failures                  | Individual errors reported via `retExtInfo.list[].code`, successful IDs still returned  | `bybit.exchange.ts:1376-1401`      |
| Batch placement failure                       | Falls back to individual `placeOrder()` per order in the failed batch                   | `bybit.exchange.ts:1336-1346`      |

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

**That spelling is for 401/403 ONLY, and every other status must stay a bare
`HTTP <n>`.** The `else` arm reads `response.statusText`, which is `undefined`
on every tunnelled response because the frame spells it `status_text` — so a
500 renders as `HTTP 500`. That looks like a bug and is not one. Routing it
through the shared `getHttpFailure` would produce `HTTP 500 Internal Server
Error`, and `errorPatterns.ts` matches `'server error'` → `exchange_down`,
which demotes the red toast to a deduped log; `'timeout'` does the same to
408/504. This is the general rule in `packages/safe-cex/CLAUDE.md`: the number
goes on, the canonical reason never does. 401/403 are the deliberate exception
because there the auth wording IS the intended classification. Pinned by
`bybit.auth-failure-backoff.test.ts`.

The 401/403 strings are load-bearing beyond this file:
`apps/web/src/utils/sentry/sentryFingerprint.ts` matches
`/HTTP 403 Forbidden[\s\S]*API key rejected/i`. Changing the wording silently
breaks that grouping.

**Bybit live/demo environment mismatch detection is an auth-failure add-on, not
a separate startup path.** Bybit documents `10003` as "API key is invalid" and
explicitly says to check whether the key and domain match across `mainnet`,
`testnet`, `mainnet-demo`, and `testnet-demo`. For livenet `bybit` /
`bybitdemo` only, the response interceptor probes the opposite REST host once
with signed `GET /v5/user/query-api`: `bybit` probes `api-demo.bybit.com`, and
`bybitdemo` probes `api.bybit.com`. A successful opposite-host probe prepends
`Bybit API key environment mismatch: ...` to the original `retMsg`; it never
replaces the original text, because `health/errorPatterns.ts` and the Sentry
fingerprint above rely on `unauthorized` / `forbidden` / `API key rejected` or
`API key is invalid` still being present. **That reliance was only made true on
2026-09-25** — the bybit entry's literal was `Api key invalid`, which the live
`API key is invalid.` does not contain, so the commonest rejection of all
classified as `unknown` and raised no auth badge. The entry now carries a regex
for the live wording AND the bare ret codes, which cannot drift with bybit's
copy. Probe requests are tagged out of
`recordAuthFailure()` / `recordAuthSuccess()` and interceptor error logging, so
a positive opposite-env probe cannot reset the backoff for the actually rejected
primary environment. The probe is scoped out for `options.testnet`,
`bybitSiteId: 'BRA_BTL'`, custom Bybit-compatible origins such as HyroTrader,
`bybitinverse`, `bybitspot`, and remote-signer/no-secret paths.

The Rust engine applies this diagnostic to its terminal startup fee-read 10003.
One opposite-host GET uses the exact rejected GET credential, including a
secondary read key, and has both a two-second native deadline and request timeout.
Only HTTP success plus integer `retCode: 0` adds the hint. Fractional, floating
zero, quoted and other malformed codes retain the original error. Concurrent
category failures share one probe
and late replies cannot emit another terminal failure. The original native
message stays in `venueMessage`; primary/read-key deactivation policy is retained.
Native eligibility receives the SDK's site marker and excludes any nonempty
site, custom endpoint, testnet, Hyro, inverse, spot or remote-signing account.

**`start()` does not claim the account is ready when the key was rejected.** All
three bybit variants end startup with a readiness line, and they used to log
`Ready to trade on Bybit` unconditionally — beside the auth error, for an account
that could not trade. `fetchMarginAccountInfos` now THROWS the venue's `retMsg`
instead of returning after `emitAndLogError`, and `start()` catches it into
`privateProbeFailure`, which the readiness line reads. The message quotes that
reason verbatim rather than classifying it, so it cannot blame the key for a
transport failure.

**Attach the probe's handler where it is created, in all three variants.** A rejected
key answers the probe before the markets load, so a `.catch` added after those awaits
leaves the rejection unhandled and Node exits — exchange-mcp listed its tools and died
200ms later. Gated by `bybit.startup-auth-log.test.ts` and
`bybit.inverse-spot.startup-rejection.test.ts`.

**Do NOT gate that line on `authFailureStreak`.** It reads correctly and is a
no-op in production: `bybit.api.ts` signs markets and tickers too, so their
successful 200s call `recordAuthSuccess()` and zero the streak — and in the
`!useExternalPublicData` branch they settle AFTER the probe has already failed.
The probe's own result is the only signal a later request cannot reset. The
`externalPublicData=false` case in `__tests__/bybit.startup-auth-log.test.ts`
exists to catch exactly this: a streak-based gate passes the other case and
fails that one.

**Startup still SUCCEEDS on a rejected key, deliberately.** Markets, tickers and
the public socket are all up before that point, so throwing out of `start()`
would dispose the exchange and strand public market data that works. The
readiness line degrades to a warning naming what is unavailable; the red badge,
the toast and the tick backoff are separate mechanisms that already fire.

**Known limitation, deliberately not "fixed":** a bodyless 403 from the egress
proxy is indistinguishable here from a venue credential rejection, so it is
blamed on the user's key and counts toward the auth backoff. Bybit's real
credential failures arrive ENVELOPED (`10003`/`10005`/`10010`/`33004`) on HTTP
200 and are counted separately, which argues the bodyless case is transport —
but a bad key was also observed producing bodyless 4xx in the e2e run above, so
the two cannot be told apart from the status alone. Removing the attribution
would leave a genuinely dead key polling at 500ms forever, which is the worse
failure, so the current behaviour stands until something can distinguish them.

Nothing backed off either — the tick loop re-armed unconditionally at the 500ms
metadata interval, so the error count was purely a function of how long you
watched (120 errors vs a bound of 100). `BaseExchange.recordAuthFailure()` /
`recordAuthSuccess()` drive an exponential backoff inside `getTickInterval()`.
Two rules for calling them:

- **Record failure from the transport layer**, which is the only place that sees
  the HTTP status. Also record on the `BYBIT_AUTH_RET_CODES` set, because bybit
  returns credential failures inside a normal envelope too — sometimes on HTTP
  200 — so status alone under-counts. `10004` ("error sign") counts — it is
  what a wrong secret returns — but only once the re-signed resend fails too.
- **Only a signed request may record success.** `KLINE` is fetched unsigned and
  shares the same client, so a chart fetch would otherwise reset the streak
  while the credentials are still rejected.

### Error-as-Success Patterns

- **"order not modified"**: Returned when `REPLACE_ORDER` or `SET_TRADING_STOP`
  is called but nothing actually changed. Treat as success for virtual SL/TP
  update, limit order update, position-bound TP/SL update, and repeated
  position TP/SL placement.
- **"leverage not modified" / `110043`**: Returned when `SET_LEVERAGE` is called
  with the already-active leverage. Treat as a successful no-op and refresh the
  symbol's positions instead of surfacing an error toast. This applies to demo
  accounts too because `BybitDemoExchange` inherits `BybitExchange.setLeverage`.
- **Trading Terms agreement (`110123` / `110125`)**: Returned when the Bybit
  master account has not accepted the required contract agreement. Surface this
  as a user-action order warning, not an auth failure. For HyroTrader, the user
  normally cannot sign because HyroTrader controls the Bybit account; tell them
  to wait briefly/retry or contact HyroTrader. Do not auto-call
  `/v5/user/agreement` from an order failure: it signs legal terms, only works
  from a master account, and requires transfer/withdraw-class permissions.
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
| Income history        | Yes (UNIFIED only)             | Yes (UNIFIED only)                  | No                                  | Yes (inherited)              |
| Execution queries     | Yes                            | Yes                                 | No                                  | Yes (inherited)              |
| Market fetch parallel | Yes (PreLaunch in parallel)    | No (sequential)                     | No (single request, no PreLaunch)   | Yes (inherited)              |

## Inverse and options on the `bybit` connection (opt-in, off for Tealstreet)

`BybitExchange` can also trade inverse contracts (`enableInverseMarkets`) and options
(`enableOptionMarkets`) on the same unified account. **Both are off by default and no
Tealstreet app sets them** — only exchange-mcp's `EXCHANGE_INVERSE` / `EXCHANGE_OPTIONS`
do. Tealstreet users trade inverse on the separate `bybitinverse` adapter. Do not wire
either toggle into the web or mobile account form without a product decision.

- **The private socket drops rows for categories the connection does not trade**
  (`tradesCategoryOf`, `rowsThisConnectionTrades` in `bybit.ws-private.ts`). Bybit's
  `/v5/private` carries every category on the account, and the mappers decide inverse
  and option handling by SYMBOL, so without this gate a linear-only user holding BTCUSD
  had it stored in coin units and its USD value moved their balance upnl. Gated by
  `bybit.private-socket-category-gate.test.ts`. Do not remove it to "show everything".
- Categories route by symbol shape: `isBybitInverseSymbol` (`bybit.inverse.ts`, checked
  live against all 26 inverse and 906 linear symbols) and `isBybitOptionSymbol`. Inverse
  takes a second public socket on `/v5/public/inverse`, created only when the toggle is on.
- A failed inverse or option read keeps that category's stored rows
  (`fetchCategoryRowsOrKeep`), so one bad poll neither wipes them nor jumps the balance.
- **Known gaps, all opt-in only:** UTA 1.0 accounts (status 3/4) are not excluded, though
  their inverse sits in the CONTRACT wallet, so `total` comes out low by the inverse PnL;
  account-wide order history lists linear then inverse rather than newest first.
- **Brazil local order rows convert inverse amends** through `remainingInStoreUnits`, as
  `bybitinverse` does: `remaining` in the coin at the (new) price, USD contracts in
  `extra.remainingNative`. Gated by `bybit.brazil-inverse-amend.test.ts`.

## Testing / Validation

Run from the `packages/safe-cex` directory:

```bash
yarn lint
yarn typecheck
yarn test-ci
```

## Common Issues and Solutions

1. **"error sign" on requests**: Signature mismatch. Check time sync, recv_window. The API re-signs and resends once automatically.
2. **"timestamp and recv_window param"**: Local clock is out of sync. Auto-resolved by `virtualClock.syncBybitClientServerTime()` plus one signed-request retry.
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

Linear and inverse are separate `BaseExchange` subclasses, so keep
`fetchHistoricalFills` parser aliases and fallback behavior mirrored. The chart
fill-arrow path reads stored `Execution` rows; if inverse discards a valid primary
execution shape or throws before order-history fallback, the inverse chart gets an
empty fill list while linear Bybit still draws arrows.

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

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Linear, inverse and spot send `orderLinkId`, rule `^[A-Za-z0-9_-]{1,36}$`, on the single-order path and on the batch path (`formatOrderForBatch`).
- **HyroTrader rejects caller IDs** (`clientOrderIdRule = undefined`): its order recovery re-mints `orderLinkId` after a reject, which would rewrite a caller's ID. Its own `placeOrders` override must keep calling `assertCallerClientOrderIds` first.
- Regular-order `updateOrder` uses `REPLACE_ORDER` (amend), which keeps `orderLinkId`.

## Rust engine variants

With `engine: 'rust'`, `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo` and
`hyrotrader` use one `sdk-rs/venues/bybit` state machine with a variant/paths
option. Demo/testnet routing and the Hyro configured-origin selection remain
as described above. The registry resolves raw `hyrotrader` mode and URL maps,
using the simulator for demo mode or an empty production REST livenet URL,
then selecting testnet/livenet. All three selected URLs must be configured.
Native callers may instead supply explicit `endpoints` when `hyrotrader` is
absent; missing endpoints fail before I/O.

Inverse keeps native USD order amounts and coin position PnL. The frame bridge
preserves native order remainder and reconstructs native position size for
copiers. Its wallet PnL is converted to USD, fixing the TS aggregate-unit bug.
Spot reads current instrument precision/quantity fields, sends base-coin market
buy units, and uses spot conditional/cancellation families. It refuses trailing
position stops. Hyro keeps its order/position-only private subscription and
emits incremental fills from cumulative order executions; ordinary Bybit variants
use individual execution events.

Rust execution topics deduplicate the latest 4,096 `(symbol, execId)` pairs
per product, including repeated rows within one frame and across reconnects.
Invalid quantity/price packets do not consume identity. Older evicted IDs and
legacy packets without `execId` have no exactly-once guarantee. Demo/Hyro
accepted cancels and terminal stream rows advance reconciliation revisions,
so an in-flight REST snapshot cannot restore a retired order even at the same
engine timestamp. Those variants retain the latest 4,096 terminal native order
IDs across reconnects to filter replica-stale open rows in later reads too.
Only final wire statuses establish tombstones; `Triggered`/`PendingCancel` do not.
Older evicted order IDs have no persistent terminal-fence guarantee. The inverse variant waits for authoritative order updates
after amend; coin-unit `remaining` and native `extra.remainingNative` stay
separate throughout that path.

Linear Rust accounts now honor `enableInverseMarkets` and
`enableOptionMarkets` together. Other variants keep their TS product eligibility
and do not enable these combined modes. Independent category startup, public
feeds and snapshot generations join one catalog/account; a shared private
subscription acknowledgment heals every enabled category, and category position
updates preserve sibling leverage/margin settings. Failed or malformed snapshots
retain prior state. Account-wide history joins categories before newest-first
global limiting.

Typed option terms survive frames as `Market.option`, and position Greeks keep
their documented native units keyed by position ID. Option-chain/expiry/account
Greek queries use the native endpoints. Plain limit/market option orders retain
base quantity and required client identity; IV-priced limit orders send
`orderIv` without a competing price. Unsupported derivative controls and IV on
non-options reject the whole batch before writes. Options do not support the
derivative candle/leverage/position-mode paths. Typed instrument classification
preserves absent/empty/unknown `symbolType`, region and underlying through both
frame consumers, restoring the actual CLI TradFi preflight.

The TS implementations remain the replay oracle and support existing TS-engine
callers. No proven-unused helper or alternate Rust venue crate is retained. Verification,
known TS defects and live-connectivity limits are recorded in
`docs/plans/BYBIT_VARIANTS_PORT.md` and
`docs/plans/RUST_PRODUCT_SEMANTICS.md`.

Rust Partial position TP/SL edits preserve the owner-selected TS cancel/recreate
semantics: the raw requested price against mark selects SL or TP while keeping
Buy/Sell. A replacement uses trading-stop directly, native total quantity and
the new leg's default trigger when flipping type. The cancellation must prove
the captured order was accepted (matching per-row status or Hyro order ID),
and replacement errors remain visible. Missing-mark and standalone inverse
amount-only fallbacks retain their distinct TS fields. Shared-family success
requests authoritative order reconciliation even with a healthy private socket;
it never restores the old canceled row to mimic a local TS lifecycle state.

### Native explicit catalogue refresh

Rust `refreshMarkets()` issues a bounded public catalogue walk across all enabled
categories, commits atomically, then flushes the host before returning one
added/removed delta. Account/order/settings/book/candle/ticker state and known
symbol fees survive. It never starts signed reconciliation or restarts sockets.
`fetchMarkets()` and `fetchTickers()` remain cached. New symbol fees need startup
fee discovery after restart; degraded optional products also require restart.
Shared consumers must refresh their source then restart provider/accounts through
the existing identity-change contract. Hyro retains its USDT perpetual filter
and normal 10 s ping deadline; 5 s watch wakes never imply an early ping.


## Rust private row admission

Ordinary REST order/position snapshots and WS order/position arrays admit healthy
peers when another row is malformed. A rejected native order retains its cached
parent, attached TP/SL family and position-bound metadata; a valid admitted parent
owns its complete family, including explicit removal of old brackets. Rejected
positions retain the affected symbol's cached legs and Greeks without republishing
cached leverage/mode as newly confirmed settings. Unknown identity holds absence
removal for the category, while valid peers can progress; ambiguous same-symbol
mode transitions remain coherent. Coverage storage is bounded and exhaustion
holds the scope conservatively.

Rejected WS evidence fences older REST joins and coalesces a newer read-only
repair. Orders use the existing bounded retry cadence; positions use account
polling. Strict Hyro handshake/account evidence and financial execution/history
coverage remain separate. Structural/envelope or paging failures never establish
authoritative empty exposure.


Rust acknowledged leverage writes (including native leverage-not-modified replies)
advance the existing positions revision before publishing the selected leverage.
An older in-flight positions join cannot restore stale leverage/settings; it
coalesces one fresh read after settling. A refused write leaves the authoritative
read eligible. The fence adds no write retries and does not alter position-mode
switch semantics.

Rust closed-PnL and unbounded order-history page-cap warnings carry their actual
native query command in `history.incomplete` before partial completion/window
continuation. Existing rows, caps and error policies are preserved; the host can
flag the owning partial array and route its warning without crossing callers.

Rust release code generation for the Bybit crate uses `opt-level = "z"` for
family WASM size. CI still requires the public CPU/memory gates and complete raw replay
outputs, including caller identities; the source parsers, handlers and financial
calculations retain their existing behavior.
