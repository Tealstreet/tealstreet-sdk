# Bitget Exchange Integration

This directory contains the Bitget exchange integration for Tealstreet, supporting three contract variants: USDT-margined linear, USDC-margined linear, and coin-margined inverse perpetual futures.

## Exchange Information

- **Pretty Name**: Bitget
- **Exchange Names**: `bitget` (USDT-M), `bitgetusdc` (USDC-M), `bitgetinverse` (Inverse)
- **Status**: `bitget` and `bitgetusdc` enabled; `bitgetinverse` disabled (`enabled: false`, `enableInPreview: false`)
- **Broker ID**: `Tealstreet` (sent as `X-CHANNEL-API-CODE` header)

## API Documentation

- **Official API Documentation**: https://www.bitget.com/api-doc/
- **Contract/Futures API**: https://www.bitget.com/api-doc/contract/intro
- **CCXT REST Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/bitget.ts
- **CCXT WebSocket Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/bitget.ts

## Rust engine parity

The engine (`sdk-rs/venues/bitget`) supports `bitget`, `bitgetusdc` and
`bitgetinverse`. Inverse order amount/filled and placement sizes preserve the TS
native units; only remaining and position display sizes use the TS price-based
8-decimal conversion. Collateral uses each market's base coin and preserves the
per-coin balance map through worker frames.

UTA bills use v3 `financial-records`, retaining raw signed amount and fee metadata
without assuming trade rows are gross PnL. Classic trailing modifies through
`modify-plan-order`; UTA trailing uses `type=trailing_stop` and native distance
parameters. Size-only edits preserve original callback parameters and activation
basis. Unsupported external opening, ambiguous exposure, limit-execution or
index-triggered trailing configurations remain visible and cancelable; edits
fail before canceling them.

Documented native regressions and private tapes cover these engine paths; the TS
implementation below remains the replay reference for its supported overlap.
See `docs/plans/RUST_BITGET_PARITY.md` for verification and explicit TS fixes.

## UTA Support Status

Bitget Unified Trading Account (UTA) keys cannot call Classic v2 account/trade
endpoints. The adapter now routes UTA for **livenet `bitget` USDT-futures** and
**livenet `bitgetusdc` USDC-futures**:

- Validation, balance, open positions, and open normal orders route through
  `/api/v3/*` after the account-type probe marks `isUta = true`.
- Normal market/limit place and cancel use v3 payloads (`category`, `qty`,
  `posSide`, `clientOid`) and do not send v2 `productType`, `marginCoin`, or
  `size` fields. Attached TP/SL on normal UTA orders rides the same
  `/api/v3/trade/place-order` request via `takeProfit` / `stopLoss`,
  `tpTriggerBy` / `slTriggerBy`, and market strategy order types.
- UTA position mode comes from account-level `holdMode` on
  `/api/v3/account/settings`; per-symbol `marginMode` comes from
  `symbolConfigList` when present. Fresh/flat symbols can be absent from that
  list, and Bitget documents UTA futures order `marginMode` as defaulting to
  `crossed`, so metadata fallback must still return `holdMode` plus cross
  margin instead of blocking order placement.
- Standalone TP/SL placement, open-order fetch, and cancel use v3 strategy
  endpoints (`type: "tpsl"`; `tpslMode: "full"` without `qty`, or
  `tpslMode: "partial"` with base-coin `qty`). Bitget's docs describe both
  shapes, but only `partial + qty` is exercised by CCXT and other Bitget
  adapters in the wild. `formatUtaStrategyOrder` therefore resolves the live
  position size when the UI sends `amount: 0` (infinity-stops marker) so the
  request goes out as `partial + qty` whenever a position exists. Only the
  pre-position case (user sets TP/SL before opening) falls back to the
  documented `tpslMode: "full"` shape.
- Private WS uses `wss://ws.bitget.com/v3/ws/private` and subscribes with
  `{ instType: "UTA", topic: "account" | "position" | "order" }`.
  Its proxied route is the v3 equivalent of Classic's: the relay upstream
  `ws.bitget.com/v3/ws/private` (routing `privateUta`, resolved as
  `wsPrivateUta`; the exchange server's pin keeps ws-proxy `/bitget/v3/private`
  first). See "Private WS egress".

Account-type detection is eager during validation/startup and lazy on private
methods that choose between Classic v2 and UTA v3. The lazy path shares one
in-flight `ACCOUNT_SETTINGS_V3` probe and then memoizes the result on the
exchange instance, so unstarted main-thread instances can still route UTA
orders/reads correctly without racing multiple probes.

Still out of scope for this slice: `bitgetinverse`, testnet UTA, bills,
trailing stops, and in-place modify of UTA strategy orders. UTA TP/SL updates
use cancel-and-replace through the v3 strategy cancel/place endpoints instead.
The other paths must keep using Classic v2 behavior until their own slices land.

UTA historical fills route through `GET /api/v3/trade/fills` after account-type
detection marks the instance as UTA. Bitget documents this route as 90-day
lookback only, with each request capped to a 30-day start/end window and cursor
pagination. The adapter splits requested windows into sub-30-day spans, follows
the response `cursor`, maps `execId`, `execPrice`, `execQty`, `execPnl`,
`feeDetail[]`, `tradeScope`, `side`, and `tradeSide`, and filters requested
symbols client-side because the official request parameters do not include a
symbol filter. Do not fall through from UTA to Classic `/api/v2/mix/order/fills`.

CCXT's current Bitget UTA support uses the same credential model (`key`,
`secret`, `passphrase`) and does not suggest an extra account-linking key. The
remaining useful comparison items are additional UTA slices: map
`GET /api/v3/trade/fills` against live accounts for any undocumented fields
such as explicit `posSide`; add UTA history-orders; add UTA strategy-order
history and modify endpoints; add UTA trailing-stop
strategy placement; add UTA batch place/modify/cancel; and revisit v3 public
market/WS streams only if we decide to move public data off Classic v2. Keep
coin-margined and testnet UTA disabled until each has its own confirmed Bitget
behavior and regression coverage.

When a supported linear variant unexpectedly reaches a v2 private endpoint and
gets `40085` / "Unified Account mode ... Classic Account API is not supported",
treat that as a late UTA-routing signal: promote the instance to UTA and retry
the v3 path. Only variants that still have `supportsUtaRouting = false` should
surface `BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE`; do not show the old broad
"UTA is not supported, use Classic" wall-off on `bitget` / `bitgetusdc`.

## File Structure

| File                            | Description                                                                                                         |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `bitget.api.ts`                 | REST API client: Axios instance creation, HMAC-SHA256 request signing, response interceptors                        |
| `bitget.exchange.ts`            | Main `BitgetExchange` class (USDT-M linear) extending `BaseExchange`. Contains all trading logic                    |
| `bitget.types.ts`               | Constants (`BASE_URL`, `ENDPOINTS`, `BROKER_ID`, `RECV_WINDOW`), interval/status/side/type mappings, response types |
| `bitget.ws-private.ts`          | Private WebSocket: authentication, order/position/account streaming with throttled REST reconciliation              |
| `bitget.ws-public.ts`           | Primary public WebSocket: ticker snapshots (1/3 of markets), OHLCV candles, order book (books channel)              |
| `bitget.ws-secondary-public.ts` | Secondary public WebSocket: handles another 1/3 of market tickers. Two instances run (index 1 and 2)                |
| `bitgetinverse.exchange.ts`     | `BitgetInverseExchange` class: coin-margined inverse futures, overrides balance/position/order methods              |
| `bitgetusdc.exchange.ts`        | `BitgetUSDCExchange` class: minimal override, changes `productType` to `usdc-futures` and `marginCoin` to `USDC`    |

## Authentication

Authentication uses **HMAC-SHA256** with Base64 encoding. Three credentials are required: API key, secret, and **passphrase** (`requiresPassphrase: true` in metadata).

### REST API Signing (`bitget.api.ts`)

Headers set on every request:

- `ACCESS-KEY`: API key
- `ACCESS-PASSPHRASE`: Passphrase (set once in Axios defaults)
- `ACCESS-SIGN`: HMAC-SHA256 signature (per-request via interceptor)
- `ACCESS-TIMESTAMP`: Current virtual clock timestamp in milliseconds
- `X-CHANNEL-API-CODE`: Broker ID (`"Tealstreet"`)
- `Content-Type`: `application/json`

Signature construction (`bitget.api.ts:38-68`):

```
toSign = timestamp + METHOD + url(?queryString) + jsonBody
signature = base64(HMAC-SHA256(secret, toSign))
```

The `toJsonString()` helper (`bitget.api.ts:92-105`) strips leading underscores from JSON keys (keys starting with `_` have the underscore removed in the serialized output). The `$` character is also stripped from symbol parameters before signing and sending (`bitget.api.ts:47,56`).

**Signer-failure branch does NOT say "auth failed".** The `remoteSigner.sign` error branch in `auth` emits `Remote WS signing unavailable: …`, not `Remote WS auth failed: …`. The latter wording matches `health/errorPatterns.ts`'s `auth_invalid` (case-insensitive substring), which the web controller treats as a terminal API-key rejection and suppresses automatic account reloads for five minutes — the very reload a signer-less tab needs to re-pair. See `../CLAUDE.md` → "Remote-signer WS auth" and the bybit template in `bybit/bybit.ws-private.ts`.

### WebSocket Authentication (`bitget.ws-private.ts:536-590`)

```
timestamp = unix epoch seconds
sign = base64(HMAC-SHA256(secret, timestamp + "GET/user/verify"))
```

Sent as `{ op: "login", args: [{ apiKey, passphrase, timestamp, sign }] }`. On successful login, private channels are subscribed by parsing the ack structurally (`event: "login"`, `code: 0`/`"0"`). Do not match the login payload as an exact string; Bitget can include extra fields such as `connId`.

### Time Synchronization

Both REST and WS use `virtualClock` for timestamps. REST uses `virtualClock.getCurrentTimeMs()` (milliseconds), WS auth uses `virtualClock.getCurrentTime().unix()` (seconds).

## Startup Sequence

The `start()` method (`bitget.exchange.ts:144-191`) initializes in this **strict sequential** order:

1. **`fetchMarkets()`** -- REST call to get available contracts; updates store with `markets`, sets `loaded.markets = true`
2. **`publicWebsocket.connectAndSubscribe()`** -- starts primary public WS (ticker for 1/3 of markets) immediately after markets load
3. **`fetchTickers()`** -- REST call to get initial ticker data (bid/ask/last/index/funding); updates store with `tickers`, sets `loaded.tickers = true`
4. **Two secondary public WS connections started in parallel** (non-awaited):
   - `secondsTickersWebsocket.connectAndSubscribe()` -- secondary public WS for 2nd third of tickers
   - `thirdTickersWebsocket.connectAndSubscribe()` -- secondary public WS for 3rd third of tickers
5. **`_detectAccountType()`** -- probes `ACCOUNT_SETTINGS_V3`; UTA routing is
   enabled only for livenet `bitget` USDT-futures when this succeeds
6. **`privateWebsocket.connectAndSubscribe()`** -- private WS chooses Classic v2
   channels or UTA v3 topics from the completed account-type probe
7. **`tick()`** -- first polling cycle: fetches positions (every tick), balance (every 2nd tick)
8. **`fetchOrders()`** -- Classic fetches all three order types sequentially;
   UTA fetches v3 unfilled normal orders only:
   - `fetchNormalOrders()` (limit/market orders, paginated separately for
     `status=live` and `status=partially_filled`)
   - `fetchAlgoOrders()` (TP/SL, paginated with `planType: 'profit_loss'`)
   - `fetchTrailingStopOrders()` (trailing stops, paginated with `planType: 'track_plan'`)
9. Store updated with `orders`, sets `loaded.orders = true`

Each step checks `this.isDisposed` to allow early exit if the exchange is disposed during initialization.

**Note**: There is no `publicDataOnly` mode -- Bitget always requires authentication.

### Private WS Degraded State

After Bitget order writes (place/cancel/update), the exchange starts a short
private-stream grace window. If no private order/algo/position/account frame
arrives within that window, it sets `socketHealth.privateDegradedReason` so the
web topbar can show a degraded private stream even though REST writes still
succeed and the socket may appear connected. Any authenticated private payload
clears the degraded reason. This mirrors Coinbase's post-write user-stream
watchdog and is intentionally lighter than Lighter's full route-recovery system.
The watchdog only reconnects the current route; route changes follow evidence
about the route (see "Private WS egress"). Literal `pong` heartbeat frames must
not count as private-stream payloads for this decision: a browser can keep the
direct socket alive with pongs while receiving zero authenticated order,
position, or account data. While degraded, Bitget runs its own periodic REST
private snapshot (`fetchPositions`, full `fetchOrders`, fast `fetchBalance`)
until a real private frame arrives. Do not rely on private WS reconnect-open
handlers for this: `ENABLE_RECONNECTING_WEBSOCKET` is false in this repo, and
the ordinary tick loop does not poll standalone Classic algo TP/SL orders.

**Only the instance that started its private stream arms this watchdog**
(`privateStreamStarted`, set in `start()`). The web's main-thread mirror places
every UI order but is never started (see `apps/web/src/controllers/CLAUDE.md` →
dual-instance model), so it never sees a private frame; before the guard, every
UI order fired the toast 8s later, started this REST loop writing the UI's
store, and `recoverMissingPrivateStream` opened a second private socket from
the mirror.

**Bitget answers a failed login with `event: "error"`, not `event: "login"`**
(30005 login failed, 30011 key, 30012 passphrase, 30013 timestamp, 30015 sign;
IP and region refusals arrive the same way). `onMessage` reports it once per
failure streak and reconnects when it arrives before a successful login; after
login it is a refused subscription and is reported without reconnecting. Left
unhandled, a rejected socket kept answering pings with zero subscriptions and
passed every liveness check while delivering nothing.

## REST API Endpoints

Private trading/account endpoints are Classic v2 unless explicitly marked as v3
below. UTA routing is gated by `usesUtaRouting` (`isUta && supportsUtaRouting &&
!testnet`) and is currently true for livenet `bitget` and `bitgetusdc`.

### Market Data

| Constant          | Path                                 | Usage                              |
| ----------------- | ------------------------------------ | ---------------------------------- |
| `MARKETS`         | `/api/v2/mix/market/contracts`       | Fetch available contracts          |
| `TICKERS`         | `/api/v2/mix/market/tickers`         | Fetch all ticker prices            |
| `KLINE`           | `/api/v2/mix/market/candles`         | Fetch OHLCV candlestick data       |
| `SYMBOL_LEVERAGE` | `/api/v2/mix/market/symbol-leverage` | Fetch leverage brackets per symbol |

### Account & Positions

| Constant                     | Path                                   | Usage                                                |
| ---------------------------- | -------------------------------------- | ---------------------------------------------------- |
| `ACCOUNT_SPOT`               | `/api/v2/spot/account/info`            | Fetch spot account info (for userId)                 |
| `ACCOUNT`                    | `/api/v2/mix/account/account`          | Fetch single account details (leverage, margin mode) |
| `BALANCE`                    | `/api/v2/mix/account/accounts`         | Fetch all account balances                           |
| `POSITIONS` / `POSITIONS_V2` | `/api/v2/mix/position/all-position`    | Fetch all open positions                             |
| `POSITION_LEVERAGE`          | `/api/v2/mix/position/single-position` | Fetch single position details                        |
| `FETCH_SINGLE_ACCOUNT`       | `/api/v2/mix/account/account`          | Fetch per-symbol account metadata                    |
| `UTA_ACCOUNT_ASSETS`         | `/api/v3/account/assets`               | UTA balance/account assets                           |
| `UTA_POSITIONS`              | `/api/v3/position/current-position`    | UTA current positions                                |

### Order Management

| Constant                    | Path                                    | Usage                                              |
| --------------------------- | --------------------------------------- | -------------------------------------------------- |
| `PLACE_ORDER`               | `/api/v2/mix/order/place-order`         | Place single order (not used; batch is preferred)  |
| `BATCH_ORDERS`              | `/api/v2/mix/order/batch-place-order`   | Place batch orders (up to 50 per symbol)           |
| `UPDATE_ORDER`              | `/api/v2/mix/order/modify-order`        | Modify a normal order                              |
| `ORDERS`                    | `/api/v2/mix/order/orders-pending`      | Fetch pending normal orders (paginated, limit 100) |
| `ORDER_HISTORY`             | `/api/v2/mix/order/orders-history`      | Fetch historical filled orders                     |
| `CANCEL_ORDERS`             | `/api/v2/mix/order/batch-cancel-orders` | Batch cancel normal orders                         |
| `CANCEL_ALL_ORDERS`         | `/api/v2/mix/order/cancel-all-orders`   | Cancel all orders                                  |
| `UTA_PLACE_ORDER`           | `/api/v3/trade/place-order`             | UTA normal market/limit order placement            |
| `UTA_PLACE_STRATEGY_ORDER`  | `/api/v3/trade/place-strategy-order`    | UTA standalone TP/SL strategy placement            |
| `UTA_CANCEL_ORDER`          | `/api/v3/trade/cancel-order`            | UTA normal order cancellation                      |
| `UTA_CANCEL_STRATEGY_ORDER` | `/api/v3/trade/cancel-strategy-order`   | UTA standalone strategy order cancellation         |
| `UTA_CANCEL_BATCH`          | `/api/v3/trade/cancel-batch`            | UTA batch cancellation constant, not preferred yet |
| `UTA_MODIFY_ORDER`          | `/api/v3/trade/modify-order`            | UTA modify-order constant, not preferred yet       |
| `UTA_ORDER_FILLS`           | `/api/v3/trade/fills`                   | UTA historical per-fill executions                 |

### Algo/Stop Orders

| Constant                    | Path                                     | Usage                      |
| --------------------------- | ---------------------------------------- | -------------------------- |
| `PLACE_ALGO_ORDER`          | `/api/v2/mix/order/place-tpsl-order`     | Place TP/SL algo order     |
| `UPDATE_ALGO_ORDER`         | `/api/v2/mix/order/modify-tpsl-order`    | Modify TP/SL algo order    |
| `ALGO_ORDERS`               | `/api/v2/mix/order/orders-plan-pending`  | Fetch pending algo orders  |
| `UTA_STRATEGY_ORDERS`       | `/api/v3/trade/unfilled-strategy-orders` | UTA standalone TP/SL fetch |
| `CANCEL_ALGO_ORDER`         | `/api/v2/mix/order/cancel-plan-order`    | Cancel an algo order       |
| `PLACE_TRAILING_STOP_ORDER` | `/api/v2/mix/order/place-plan-order`     | Place trailing stop order  |

### Account Settings

| Constant              | Path                                    | Usage                                              |
| --------------------- | --------------------------------------- | -------------------------------------------------- |
| `SET_LEVERAGE`        | `/api/v2/mix/account/set-leverage`      | Set leverage for a symbol (Classic + UTA fallback) |
| `SET_LEVERAGE_V3`     | `/api/v3/account/set-leverage`          | UTA per-side ISO leverage (`posSide` honored)      |
| `ACCOUNT_SETTINGS_V3` | `/api/v3/account/settings`              | UTA probe + hold/margin mode metadata              |
| `UTA_SET_HOLD_MODE`   | `/api/v3/account/set-hold-mode`         | UTA one-way / hedge mode changes                   |
| `SET_POSITION_MODE`   | `/api/v2/mix/account/set-position-mode` | Set hedge or one-way mode                          |
| `SET_MARGIN_MODE`     | `/api/v2/mix/account/set-margin-mode`   | Set cross or isolated margin                       |

## WebSocket Implementation

The implementation uses **four WebSocket connections** simultaneously:

### 1. Public WebSocket (`bitget.ws-public.ts`)

- **URL**: `wss://ws.bitget.com/v2/ws/public` (or fallback: `wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public`)
- **Channels subscribed**:
  - `ticker` for 1/3 of all markets (index `i % 3 === 0`)
  - `candle{interval}` for active chart symbol (subscribed on-demand via `listenOHLCV`)
  - `books` for active order book symbol (subscribed on-demand via `listenOrderBook`)
- **instType**: Derived from `this.parent.apiProductType.toUpperCase()` for every subscribe and unsubscribe frame, so each variant uses its own product type.
- **On-demand subscriptions**: Candle and order-book listeners are reference-counted. Cleanup is idempotent, cancels a pending pre-connect subscription, and deletes the final topic key before sending its unsubscribe so reconnects cannot revive a stopped stream.
- Handles snapshot/update actions for order books with incremental updates

### 2. Secondary Public WebSocket #1 (`bitget.ws-secondary-public.ts`, index=1)

- **URL**: Same as public WebSocket
- **Channels**: `ticker` for 1/3 of all markets (index `i % 3 === 1`)
- Ticker-only stream to distribute load across connections

### 3. Secondary Public WebSocket #2 (`bitget.ws-secondary-public.ts`, index=2)

- **URL**: Same as public WebSocket
- **Channels**: `ticker` for 1/3 of all markets (index `i % 3 === 2`)
- Ticker-only stream for the remaining third of markets

**Note**: Both public WS classes derive `instType` from `this.parent.apiProductType.toUpperCase()`, matching the private WS. This keeps ticker, candle, and orderbook subscriptions on the selected variant's product type.

### 4. Private WebSocket (`bitget.ws-private.ts`)

- **Classic URL**: `wss://ws.bitget.com/v2/ws/private` direct; a proxied account dials the relay `wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?ts-relay=wss://ws.bitget.com/v2/ws/private` (the exchange server: ws-proxy `wss://fsjnpy143pbz.asia.tealstreet.io/bitget/v2/private`, then the relay)
- **UTA URL**: `wss://ws.bitget.com/v3/ws/private` direct; proxied: the relay to `ws.bitget.com/v3/ws/private` (the exchange server: ws-proxy `.../bitget/v3/private`, then the relay)

#### Private WS egress (Bitget checks the key's bound IP)

Since 2026-10, Bitget rejects a private login whose source IP the API key is not
bound to: `{event: "error", code: 40018, msg: "Current IP does not match the bound
IP"}`. REST was unaffected (it egresses through our proxies), so users saw a dead
private stream while orders still placed — the browser's and the hosted
container's IPs are never on a key's whitelist.

Routing (`routingTable.ts`) is OKX's pattern: `private`/`privateUta` are
`proxyConditional` to ws-proxy (`python/ws-proxy`, egress `54.255.57.166`, which
legacy's add-account form asks Bitget users to bind via `exchangesRequireWsIp`;
master's form no longer does, AWS_EXIT.md 3e) with a
`relayUpstream` and `relayWhenTunnelled`, so the tradetunnel relay — egressing
from the private proxy fleet, i.e. the account's REST IPs — is offered even to a
proxied account. Bitget does NOT gate the relay on `WS_RELAY_PRIVATE`.

**Since AWS_EXIT step 3e the browser build has no ws-proxy**: its `host` and
`fallback` ARE the relay url (`BROWSER_WS_HOSTS`), so a proxied socket offers the
relay twice and a direct one walks direct → relay. The ws-proxy-first chain above
survives only in the hosted exchange server's pinned hosts
(`services/exchange-server/src/egress-hosts.ts`).

`bitget.ws-private.ts` walks the chain with `WsDirectFallback`:

- **40018 (or 40024, restricted location) before login** → `rejectCurrent()` and switch to the next route at once;
  reported (once) only when every route is refused, and then it stops
  switching routes (the socket may still reconnect to the last one).
- **A tunnel socket** (ReconnectingWebSocket — it reconnects internally and never
  re-runs the route choice) advances after 2 opens that closed before login, or
  6 connects that failed before opening (an outage does that too, hence the
  higher bar). Both counters reset on every switch; a login clears the
  closes-before-login count, an open clears the failed-connects count.
- **A native direct socket** (non-proxied account: direct → relay → ws-proxy)
  advances through its own close path (`processClose` → `connectAndSubscribe`
  → `resolveUrl`), and after 2 silent connects as before.

Infra this depends on: ws-proxy's `bitget/v2/private` + `bitget/v3/private`
routes; tradetunnel `RELAY_ALLOWED_UPSTREAMS` including `ws.bitget.com/v2/ws/private`
and `/v3/ws/private` (live `kubectl` env + `terraform/{tokyo,singapore}`); the
private fleet's `stream_bitget_backend` (already present). The browser build's
relay dials `fsjnpy143pbz-tyo`; the exchange server's dials `fsjnpy143pbz.jp`
(Bitget's HOW egress is Tokyo).

- **Authentication**: Required before subscribing (login op)
- **Channels subscribed**:
  - `account` with `coin: 'default'` - Balance/account updates
  - `positions` - Position changes (snapshot + incremental updates)
  - `orders` - Normal order updates (new, partial-fill, filled, cancel)
  - `orders-algo` - Algo/stop order updates (live, triggered, cancel)
    Inbound dispatch accepts both `orders-algo` and `ordersAlgo`; Bitget/CCXT
    examples have used both spellings for Classic trigger-order frames.
- **UTA topics subscribed**:
  - `{ instType: "UTA", topic: "account" }`
  - `{ instType: "UTA", topic: "position" }`
  - `{ instType: "UTA", topic: "order" }`

Normal-order fill handling recognizes the documented `partially_filled` status. Fill events use `baseVolume` (latest fill quantity), falling back to `accBaseVolume` when the latest-fill field is absent.

### Reconnection & Fallback URL Logic

The two PUBLIC WebSocket classes implement this **fallback URL mechanism** (`bitget.ws-public.ts`, `bitget.ws-secondary-public.ts`); the private socket uses `WsDirectFallback` instead (see "Private WS egress"):

- Each connection tracks `messageCount` and `lastMessageCount`
- On each `connectAndSubscribe()` call, if both `lastMessageCount` and `messageCount` are 0 (no messages received since last connection), `connectionAttempts` increments
- After `MAX_ATTEMPTS` (2) consecutive connections with zero messages, `useFallbackUrl` is set to `true`
- On switch to fallback, a `grafana-info` event is emitted for monitoring
- `messageCount` is reset to 0 on every new connection

### Ping/Pong Heartbeat

- All WS connections send literal string `"ping"` as heartbeat
- Server responds with literal string `"pong"`
- Handled in `onMessage` before JSON parsing (`data === 'pong'`)
- `pingAt` tracks the time for latency measurement via `handlePongEvent()`

### Private WS Reconnection

When `ENABLE_RECONNECTING_WEBSOCKET` is enabled, the private WS `onOpen` handler (`bitget.ws-private.ts:63-71`) triggers `executeOrdersFetch()` to reconcile order state after reconnection.

### Order Book Updates

The public WS `listenOrderBook` (`bitget.ws-public.ts:269-372`) handles incremental order book updates:

- `action: "snapshot"` -- clears and replaces the entire book
- `action: "update"` -- applies incremental changes via `updateOrderBook()` utility
- After each update, book is sorted and totals recalculated

## Order Types and Mappings

### Normal Orders

- **Limit** (`limit`): Standard limit orders with optional `presetStopSurplusPrice` and `presetStopLossPrice` (virtual TP/SL)
- **Market** (`market`): Market orders

### Algo/Stop Orders

- **Position-level TP** (`pos_profit`): Take profit on entire position (amount = 0)
- **Position-level SL** (`pos_loss`): Stop loss on entire position (amount = 0)
- **Sized TP** (`profit_plan`): Take profit with specific amount
- **Sized SL** (`loss_plan`): Stop loss with specific amount
- **Trailing Stop** (`track_plan`): Trailing stop loss with `callbackRatio` (percentage distance)

### Virtual TP/SL on Limit Orders

When a limit order has `presetStopLossPrice` or `presetStopSurplusPrice`, `mapOrder()` (`bitget.exchange.ts:1487-1506`) creates additional virtual orders with suffixed IDs:

- `{orderId}__stop_loss` for the SL component
- `{orderId}__take_profit` for the TP component

These virtual orders require special handling in cancel/update operations (the parent limit order is canceled and re-placed with updated preset prices via `updateVirtualTpSlOrder`, `bitget.exchange.ts:1211-1256`).

### Plan Type Mapping (WS short codes to REST values)

```
psl -> pos_loss       |  ptp -> pos_profit
sl  -> loss_plan      |  tp  -> profit_plan
track -> track_plan
```

The `getCancelPlanType()` method (`bitget.exchange.ts:1639-1658`) maps both WS short codes and REST values to the correct `planType` for cancellation, with a fallback to `getOrderPlanTypeAndAmount()` computed from position state.

### Trigger Types

- `mark_price` (default): Triggers on mark price
- `fill_price`: Triggers on last/fill price (mapped to `OrderStopTriggerType.LastPrice`)

### Order Status Mapping (`bitget.types.ts:89-106`)

```
init, new, not_trigger, partial_fill, partially_fill, partially_filled, live -> Open
triggered, full_fill, filled, executed -> Closed
fail_trigger, cancel, cancelled, canceled, fail_execute -> Canceled
```

### Order Side Mapping (`bitget.types.ts:123-140`)

Supports many variations: `open_long`, `open_short`, `close_long`, `close_short`, `reduce_close_*`, `offset_close_*`, `burst_close_*`, `delivery_close_*`, `buy_single`, `sell_single`, `buy`, `sell`.

Outbound V2 order payloads use the documented lowercase `buy`/`sell` and `open`/`close` values. Keep inbound aliases separate from this wire contract.

### Time In Force Mapping

```
normal / gtc -> GoodTillCancel
post_only -> PostOnly
fok -> FillOrKill
ioc -> ImmediateOrCancel
```

## Order Lifecycle

### Order Placement

1. **Normal orders** are formatted via `formatCreateOrder()` (`bitget.exchange.ts:1027-1083`):
   - Position mode is fetched to determine `side`/`tradeSide` fields
   - Orders exceeding `maxSize` are split into multiple lots
   - Each payload gets a unique `clientOid` via `uuid()`
   - Placed via `placeOrderBatch()` using `BATCH_ORDERS` endpoint (up to 50 per symbol per batch)
2. **Algo orders** are formatted via `formatAlgoOrder()` (`bitget.exchange.ts:1085-1148`):
   - Placed one at a time via `placeAlgoOrders()`
   - Trailing stops route to `PLACE_TRAILING_STOP_ORDER`, others to `PLACE_ALGO_ORDER`
   - Trailing stops compute `callbackRatio` as percentage distance from current price and use the documented `place-plan-order` contract

### Order Update (Amend vs Cancel-Replace)

- **Normal orders** (`updateOrder`, `bitget.exchange.ts:1258-1291`): Always **cancel + re-place** (no server-side amend). The existing order is canceled, then a new order is placed with updated parameters.
- **Virtual TP/SL orders** (`updateVirtualTpSlOrder`, `bitget.exchange.ts:1211-1256`): Cancels the parent limit order and re-places it with updated preset prices. All related orders (parent + `__stop_loss` + `__take_profit`) are found by splitting on `__`.
- **Standalone Classic algo orders** (`updateAlgoOrder`, `bitget.exchange.ts:1293-1334`): Uses the `UPDATE_ALGO_ORDER` endpoint for in-place modification. When `size` is empty string, it indicates a position-level stop (no specific quantity).
  - If updating amount to 0, preserves the order's current size to avoid converting sized stops to position-level
  - Position-level orders (amount=0) require `payload.size = ''`
- **Standalone UTA TP/SL orders**: Do not use in-place modify. `updateUtaStrategyOrder` formats the replacement from the existing order fields plus the requested price or amount change, then cancels via `UTA_CANCEL_STRATEGY_ORDER` and re-places through `UTA_PLACE_STRATEGY_ORDER`.
  - The original order is snapshotted as a placeable payload **before** the cancel fires so a rollback can restore it if the replacement place call fails. Building the snapshot after the cancel would race the position state that `formatUtaStrategyOrder` reads (position size, hedge mode) and could produce an unplaceable rollback payload.
  - On place-failure the handler attempts one best-effort rollback place; whether the rollback succeeds or not it re-throws the original place error so the caller sees the update as failed. On rollback failure the emitted error explicitly tells the user the stop is not on the book and needs to be re-added.

### Order Cancellation

- **Normal orders** (`cancelNormalOrders`, `bitget.exchange.ts:922-951`): Virtual TPSL suffixes are stripped to get the actual `orderId`; orders are deduped by ID, grouped by symbol, chunked into lots of 50.
- **Algo orders** (`cancelAlgoOrders`, `bitget.exchange.ts:953-969`): Canceled one at a time; requires correct `planType` from `getCancelPlanType()`. If the raw venue `planType` is missing, infer from immutable order fields (`amount > 0` => `loss_plan` / `profit_plan`, `amount === 0` => `pos_loss` / `pos_profit`). Never recompute cancel plan type from current positions: a TP can close the position while the sibling stop is still stale in the local order store, and flat positions would incorrectly downgrade a sized stop to a full-position cancel.
- **UTA strategy orders** (`cancelUtaStrategyOrders`): Bitget can return `"Failed to stop the strategy because the corresponding strategy ID could not be found"` for a stale local TP/SL line whose strategy order is already absent. Treat that exact cancel-strategy response as idempotent success on user cancel/cancel-all, including HOW-resolved HTTP 400 envelopes; do not emit a user-facing exchange error for it. Keep the cancel leg of `updateUtaStrategyOrder` strict, because placing a replacement after the old strategy ID is already gone can recreate stale protection.
- **Cancel all** (`cancelAllOrders`, `bitget.exchange.ts:902-911`): Single POST to `CANCEL_ALL_ORDERS`.

### Plan Type Determination

`selectPlanTypeAndAmount(opts, positions)` is the pure decision; `getOrderPlanTypeAndAmount(opts)`
is the async wrapper that resolves which positions to hand it.

- If `amount === 0`, the order is position-level (`pos_loss`/`pos_profit`)
- If `amount !== 0` and **any** open position exists on the protected side, the order is a sized stop (`loss_plan`/`profit_plan`) with the size **clamped** to `min(requested, floorToLot(contracts))`
- If `amount !== 0` and the protected side is flat (or the position is smaller than one lot), the order falls back to position-level (`pos_loss`/`pos_profit`)
- Trailing stops always use `track_plan`

**Clamp, don't downgrade.** The match used to require `contracts >= amount`, so a
sized stop that overshot the position by even one lot silently became a
whole-position stop. That is exactly what happens in practice: price drift
between opening a position and sizing its stop asked for **31 against a
30-contract long** (e2e run 31353474704), and the user's deliberately-sized stop
came back covering everything. Bitget is on the `autoDownSizeReduceOrders: true`
list — it does **not** auto-cap — so passing the oversized value through would
just be rejected. Clamping is the only option that preserves intent.

Two details are load-bearing:

- **The clamp floors to the lot.** `adjust` rounds to **nearest**, and
  `position.contracts` comes straight off the venue with no guarantee of lot
  alignment, so `adjust(min(31, 30.6), 1)` would give **31** — back above the
  position it was meant to bound. `floorToLot` exists for this.
- **A sub-lot position still downgrades.** Clamping dust (0.4 contracts, lot 1)
  would POST `size: "0"`, which is not a sized stop at all.

**This is not the forbidden hand-rolled reduce-only clamp.** `../CLAUDE.md` →
"Reduce-only order downsizing" bans re-adding a clamp in
`formatCreateOrder`/`placeOrder` for **limit and market** reduce-only orders,
because the `autoDownSizeReduceOrders` flag path adds hedge-side matching and
orders-ahead accounting that a bare `Math.min` misses. Neither applies here:
side matching is already exact (see below), orders-ahead is meaningless for
trigger orders, and the alternative is not "no clamp" — it's `pos_loss` with the
size field omitted, a worse silent behavior change. Don't revert this to the
`>= amount` form.

Side matching is on `PositionSide` derived from `opts.side` (Buy→Short,
Sell→Long), never on `holdSide`. `getAlgoOrderSide` inverts only the payload
field, and agrees in both modes, so a hedge account holding both legs cannot
have the wrong leg clamped against.

**`formatAlgoOrder` must POST the returned amount, not `opts.amount`.** It
destructured `amount` from `getOrderPlanTypeAndAmount` and then used it only as a
truthiness gate (`if (amount) payload.size = adjust(opts.amount, …)`). Any
sizing decision made in `selectPlanTypeAndAmount` was therefore discarded on the
way to the wire — which is why the first fix for this would have been a no-op.
The same two lines exist in `bitgetinverse.exchange.ts`'s override; change both.

**Deploying a plan-type change means a PREVIEW release, not an exchange-server
redeploy.** `formatAlgoOrder` is on the order-write path, and order writes run on
the browser's main-thread mirror (quirk 22), so this code ships in the web
bundle. Redeploying only the exchange-server leaves the run on the old browser
code and the fix looks dead. The tell is cheap: reword the downgrade warning as
part of the change, and if the artifact log comes back with the OLD wording, you
deployed the wrong target.

**`amount === 0` is a product sentinel, not "unknown size."** It means "close the
whole position" and the web app renders it as **"Full Position"**
(`apps/web/src/utils/validation/isFullStop.ts`). An attempt to populate it from
the protected position shipped and had to be reverted — it turned every
position-level TP/SL into a fixed-size one on drag. Never size a zero-amount stop.

**Position lookup must not read `store.positions` directly — this is another
instance of quirk 22.** Hosted-mode order writes run on the never-`start()`ed
main-thread mirror, so a sized stop found no position, fell through to the
position-level branch, and was POSTed as `planType: pos_loss` **with the `size`
field omitted entirely** — the requested size silently vanished and the UI showed
"Full Position". `resolvePositionsForPlanType(symbol)` fixes this with a
short-lived REST snapshot. Three things about it are load-bearing:

- The store short-circuit asks the **exact question the caller needs**: is there
  a position on this symbol AND this side with `contracts >= amount`? Two weaker
  forms were both shipped and both wrong. `positions.length > 0` looks satisfied
  whenever the account holds _any other symbol_. A symbol-only check looks
  satisfied whenever the store holds _any row for that symbol_ — e.g. a stale or
  zeroed long while the live position is short. The second one was caught in CI:
  4 buy-side stops went out as `pos_loss` while sell-side stops on the same
  symbol were correctly sized. The rule: if the store cannot already answer the
  question, fetch.
- **Compare against the size that will actually be POSTed**, i.e.
  `adjust(amount, precision.amount)`, not the raw request. A "close the whole
  position" stop derived from a percentage lands at e.g. 30.34 on a 30-contract
  position with lot size 1; the sent size is 30 and fits perfectly, but the raw
  30.34 fails `contracts >= amount` and silently downgrades to `pos_loss`.
  Observed live as 4 such stops in a single run.
- **Only non-empty snapshots are cached.** `placeOrders` formats algo payloads
  before the entry order is placed, so a bracket always takes one snapshot while
  flat; caching that would downgrade every sized stop for the rest of the TTL.
- A failed fetch **degrades to a position-level stop and emits**. It cannot throw:
  `formatAlgoOrder` runs inside `placeOrders`' `Promise.all`, so a throw would also
  block the unrelated entry order in the same bracket.

`getCancelPlanType` deliberately stays **synchronous** and reads only
`store.positions`. By cancel time the position is usually already closed, so a
refetch would report flat and compute `pos_loss` for an order that was placed as
`loss_plan` — and the cancel would fail. It only computes at all when
`order.info.planType` is missing.

`bitgetinverse` used to carry a byte-identical copy of this logic
(`getInverseOrderPlanTypeAndAmount`) and stayed broken when the base class was
fixed. It now delegates to the inherited methods — don't re-fork it.

## Symbol Format

- **USDT-M (default)**: Symbols like `BTCUSDT`, `ETHUSDT`. Product type: `usdt-futures` (testnet: `susdt-futures`)
- **USDC-M**: Same symbol format but with USDC quote. Product type: `usdc-futures` (testnet: `susdc-futures`)
- **Inverse**: Symbols like `BTCUSD`, `ETHUSD` (quote is `USD`). Product type: `coin-futures` (testnet: `scoin-futures`)
- **Internal suffix stripping**: API responses include suffixed symbols like `BTCUSDT_USDT-FUTURES`; these are stripped via `.replace('_${productType.toUpperCase()}', '')`
- **`$` stripping**: The API interceptor strips `$` from symbol params/body before signing (used internally for chart kline exclusions). The kline endpoint is excluded from this stripping (`bitget.api.ts:46`).
- **Market resolution** (`_resolveMarket`, `bitget.exchange.ts:452-475`): Supports multiple input formats (`BTCUSDT`, `BTC-PERP`, `BTC-USDT`, `BTC`) by normalizing separators and trying suffix variations.

## Data Precision & Conversion

### Price Precision (`bitget.exchange.ts:563-564`)

Calculated from API fields `pricePlace` and `priceEndStep`:

```typescript
priceDecimals = 10 / 10 ** (parseFloat(m.pricePlace) + 1);
pricePrecision = priceDecimals * parseFloat(m.priceEndStep);
```

For example, `pricePlace=2, priceEndStep=5` yields `0.1 * 5 = 0.5` (price moves in 0.5 increments).

### Amount Precision

Uses `sizeMultiplier` directly from the API as `precision.amount`.

### Rounding

All prices and amounts are rounded via the `adjust()` utility from `safe-math` before being sent to the API.

### Inverse Contract Conversion (`bitgetinverse.exchange.ts:227-249`)

For inverse contracts, USD contract sizes are converted to crypto amounts for display:

```
cryptoAmount = contractsNative / referencePrice
```

The `referencePrice` is taken from ticker mark price, ticker last price, or entry price (in that priority). Results are rounded to 8 decimal places. The native USD value is stored in `extra.contractsNative`.

### Balance Calculation (`bitget.exchange.ts:279-285`)

```
used = unionTotalMargin - unionAvailable
free = unionAvailable
total = unionTotalMargin - unrealizedPL
upnl = unrealizedPL
```

### Realized PnL Calculation (`bitget.exchange.ts:432-435`)

```
realizedPnl = achievedProfits + totalFee - deductedFee
```

## Rate Limiting

- **Rate-limited XHR**: `maxRPS: 3` (3 requests per second) for general API calls
- **Unlimited XHR**: A separate Axios instance (`unlimitedXHR`) without rate limiting, used for:
  - Batch order placement (`BATCH_ORDERS`)
  - Order cancellation (`CANCEL_ORDERS`, `CANCEL_ALL_ORDERS`, `CANCEL_ALGO_ORDER`)
  - Algo order placement and updates
- **Batch size**: Orders are chunked into groups of 50 per symbol for batch operations
- **Recv window**: 20,000ms default timeout (`RECV_WINDOW`), configurable via `options.extra.recvWindow`
- **Network retry**: 3 retries on network errors via `axios-retry` — reads only; a signature rejection (`40009`) is re-signed and resent once, writes included (`utils/signatureRejection.ts`)
- **Tunnel support**: When `options.tealstreetTunnel` is set, the `axiosHttpOverWebsocketAdapter` is used (`bitget.api.ts:85-87`)

## State Reconciliation

### Order Reconciliation (`bitget.ws-private.ts:323-385`)

The private WS triggers throttled REST fetches (500ms debounce) after receiving order events. The `executeOrdersFetch()` method implements sophisticated reconciliation:

1. **Sequence tracking**: A `fetchSeq` counter (`ordersFetchRequestSeq`) ensures stale responses from superseded fetches are discarded (`bitget.ws-private.ts:325,329`)
2. **Tombstone cache** (`terminalOrderTombstones`, 10s TTL): When an order transitions to a terminal state (canceled/filled) via WS, its ID is tombstoned. REST fetches that return this order as "open" within the TTL window are filtered out (`bitget.ws-private.ts:342-345`)
3. **WS-open order grace** (`recentWsOpenOrders`, 5s TTL): Orders recently opened via WS are preserved even if REST doesn't yet return them, preventing premature removal (`bitget.ws-private.ts:374-378`)
4. **Algo order preservation**: WS-delivered algo orders (TP/SL/trailing stops) that REST doesn't return are preserved since Bitget's `orders-plan-pending` doesn't always return TPSL orders placed via `place-tpsl-order` (`bitget.ws-private.ts:366-379`)
5. **Terminal state protection**: If the local store has an order in terminal state (Canceled/Closed/Canceling) but REST returns it as Open, the REST version is filtered out (`bitget.ws-private.ts:353-359`)
6. **Cache pruning**: Both caches are pruned when size exceeds 2000 entries or on each reconciliation (`bitget.ws-private.ts:387-398`)

### Position Reconciliation (`bitget.ws-private.ts:220-295`)

- WS `positions` channel sends both `snapshot` and incremental updates
- `snapshot` action triggers `addOrUpdatePositionsFull()` which replaces the full position set (clearing stale positions that are omitted) (`bitget.ws-private.ts:239`)
- Incremental updates call `addOrUpdatePosition()` for each position individually (`bitget.ws-private.ts:242-244`)
- Empty snapshots (no positions) are applied immediately to clear UI (`bitget.ws-private.ts:228-229`)
- After WS events, `throttledFetchPositions()` schedules a REST refetch (500ms debounce) for authoritative data
- `liquidationPrice` is parsed from both REST and WS payloads. If Bitget
  sends the field blank/missing for an existing open position, preserve the
  previous nonzero value until an explicit numeric value arrives; an explicit
  `"0"` is still treated as authoritative.

### Tick Polling Reconciliation (`bitget.exchange.ts:193-237`)

- **Every tick** (2s): `fetchPositions()` via REST -> `addOrUpdatePositionsFull()`
- **Every 2nd tick**: `fetchBalance()` via REST
- **Every 4th tick**: `fetchNormalOrders()` via REST, but only virtual TP/SL orders (the `__stop_loss` / `__take_profit` orders) are taken from the REST response; existing non-virtual orders in the store are preserved because WS updates are more current
- **Tick interval**: 2,000ms (from metadata `tickInterval`)

## Exchange Metadata

```typescript
{
  requiresPassphrase: true,
  requiresCors: true,
  supportsTestnet: false,           // Note: testnet product types exist but feature is disabled
  supportsTrailingStopLoss: false,  // Implementation exists but not exposed
  orderbookTickDelay: 60,
  tickInterval: 2_000,
  stopOrderPendingUpdateDelay: 4_000,
  positionModeIsGlobal: true,
  marginTypeIsGlobal: false,
  hasIndependentHedgedLeverageInIsolatedMargin: true,
  autoDownSizeReduceOrders: true,
  supportsChaser: true,
  supportsTunnel: true,
  // No supportsWsRoute: bitget never reads `tealstreetWsRoute` — public is
  // `fallbackOnly`, private follows `tealstreetProxy`. Advertising it rendered
  // a checkbox nothing read.
  tvMaxGetBarFetches: 5,
  alwaysCalculateUnrealizedPnlFromLastPrice: true,
  supportedTPSLTriggerTypes: [MarkPrice],      // Only mark price trigger type
  latencyDomain: 'api.bitget.com',
}
```

### Variant Metadata Differences

- **BitgetUSDC**: Same as Bitget, `defaultQuote: 'USDC'`
- **BitgetInverse**: `enabled: false`, `enableInPreview: false`, `requiresTickersForMarketsLoaded: true`

## Testnet/Demo

Testnet support exists in code but is **disabled** in metadata (`supportsTestnet: false`).

### URL Differences

- **REST**: Same base URL (`https://api.bitget.com`) for both livenet and testnet
- **Product types** change:
  - USDT-M: `susdt-futures` (testnet) vs `usdt-futures` (livenet)
  - USDC-M: `susdc-futures` (testnet) vs `usdc-futures` (livenet)
  - Inverse: `scoin-futures` (testnet) vs `coin-futures` (livenet)
- **Margin coins** change: `SUSDT`/`SUSDC` (testnet) vs `USDT`/`USDC` (livenet)

### Environment Mismatch Detection

`validateAccount()` and `fetchOpenPositions()` detect the `"environment is incorrect"` API error and emit a helpful message: `"API key environment mismatch: Your API key appears to be for [mode], but Tealstreet is configured for [mode]. Please check your Testnet setting."` The UTA account-type probe must propagate this as a terminal validation failure instead of falling through to Classic v2 balance/position endpoints.

## Known Quirks / Implementation Notes

1. **Three public WebSocket connections for tickers**: Markets are split into thirds (`i % 3`) across three separate WebSocket connections to stay within Bitget's per-connection subscription limits. Both public WS classes derive `instType` from the parent exchange, so USDT-M, USDC-M, and future enabled variants subscribe to their matching product type.

2. **Virtual TP/SL orders on limits**: Bitget allows `presetStopSurplusPrice`/`presetStopLossPrice` on limit orders, but these do not stream via the private WebSocket. They are polled every 4th tick and mapped to virtual orders with `__stop_loss`/`__take_profit` ID suffixes.

3. **Order update = cancel + re-place**: Normal order updates are implemented by canceling the existing order and placing a new one (not a true server-side amend). Similarly, virtual TP/SL updates cancel the parent order and re-place it with updated preset prices.

4. **Algo order update endpoint**: The `UPDATE_ALGO_ORDER` endpoint (`modify-tpsl-order`) is used for in-place modification of standalone TP/SL orders. When `size` is empty string, it indicates a position-level stop.

5. **Hedge mode side inversion**: In hedge mode, Bitget inverts the `side` field for reduce-only orders. The WS handler explicitly re-inverts this for both `orders` and `orders-algo` channels. In `orders-algo` (`bitget.ws-private.ts:180`), the inversion is applied unconditionally for hedge mode; in `orders` (`bitget.ws-private.ts:130-134`), only for `reduceOnly === 'yes'`.

6. **REST lag vs WS**: Bitget's REST API can lag behind WebSocket updates by several seconds. The implementation maintains tombstone caches (10s TTL for terminal orders, 5s grace for new WS orders) to prevent stale REST data from overwriting newer WS state.

7. **Bitget REST caches responses**: The REST API caches responses for a few seconds server-side. The tick cycle avoids overwriting limit orders from REST since WS updates are more current.

8. **`toJsonString` underscore stripping**: The `toJsonString()` helper in `bitget.api.ts` strips leading underscores from JSON keys (e.g., `_key` becomes `key` in the serialized output). This is used as a workaround for certain API fields.

9. **Position mode is "global"**: `positionModeIsGlobal: true` in metadata means changing position mode (hedge/one-way) affects all contracts of that product type, which triggers a full position refetch.

10. **Margin type is per-symbol**: `marginTypeIsGlobal: false` means cross/isolated margin can be set independently per symbol.

11. **Trailing stop not exposed**: `supportsTrailingStopLoss: false` remains the product gate. The dormant placement path follows the current `place-plan-order` contract (`planType: track_plan`, `callbackRatio`, market execution) so enabling it does not revive the removed legacy endpoint.

12. **`alwaysCalculateUnrealizedPnlFromLastPrice: true`**: The frontend calculates unrealized PnL from last price rather than trusting the exchange-provided value.

13. **Pagination and empty pages**: Classic normal orders, algo orders, and trailing stop orders all use cursor-based pagination with `idLessThan` and `endId`, limited to 100 per page with a safety cap of 20 iterations. For Classic normal orders, fetch both `status=live` and `status=partially_filled`; Bitget's default regular pending-order query does not prove partially-filled orders absent. Treat both `entrustedList: null` and `entrustedList: []` as empty pages for these successful pending-order endpoints.

14. **Commented-out leverage fetching**: `fetchCurrentLeverage()` and `fetchLeverageBrackets()` are fully implemented but commented out (`bitget.exchange.ts:593-667`). Leverage is instead obtained from position data.

15. **OHLCV timestamp division**: Bitget returns candle timestamps in milliseconds; code divides by 1000 (`bitget.exchange.ts:882`) for internal format (seconds).

16. **Ticker `change24h` is a ratio**: The `change24h` field from the API is a decimal ratio (e.g., 0.05 for 5%), so it is multiplied by 100 for percentage display (`bitget.ws-public.ts:184`).

17. **`mapPosition` defensive guards**: The `mapPosition` method (`bitget.exchange.ts:1510-1570`) has defensive null/undefined checks and logs warnings when required fields (`total`, `openPriceAvg`) are missing, returning a zeroed-out position for invalid payloads.

18. **One-way mode isolated leverage mismatch warning**: `fetchPositionMetadata()` (`bitget.exchange.ts:326-332`) warns if isolated long and short leverages differ in one-way mode.

19. **Response interceptor error logging**: The response interceptor in `bitget.api.ts` fires `emitAndLogError` (user-facing toast) when `code !== '00000'` and data has no `data` field; it does not throw, so the caller still receives the response. Codes in the file-scope `SILENT_ERROR_CODES` set are skipped — currently `'40084'` ("Classic Account mode"), because `_detectAccountType` deliberately probes a v3 endpoint expecting that exact response on Classic accounts, and `'40085'` ("Unified Account mode ... Classic Account API is not supported"), because supported linear callers can promote to UTA and retry while unsupported variants surface scoped variant guidance. The interceptor also suppresses raw `"environment is incorrect"` messages because validation/position callers replace them with a Testnet-vs-Live recovery message. During `validateAccount()`, Bitget sets a validation-scope flag so expected bad-key, bad-secret, passphrase, IP whitelist, UTA-mode, and signature failures (a `40009` only after its one re-signed resend fails too) are returned to the add-account UI without Sentry/toast emission; runtime polling/trading requests outside that scope still emit. **Add new codes or message guards when you have a probe / fallback path that legitimately expects an error or a caller-owned translation**; do not relax the interceptor itself.

20. **Trailing stop orders response format**: `fetchTrailingStopOrders()` handles both array and `{ entrustedList }` response formats (`bitget.exchange.ts:824-828`).

21. **UTA vs Classic routing is explicit and linear-only for now**: UTA accounts are detected by `_detectAccountType()`, a shared probe of `ACCOUNT_SETTINGS_V3` during validation/startup and lazily before private v2/v3 branch points on unstarted instances — 200 → `isUta=true`, 40084 (`Classic Account mode`) → `isUta=false`. Other Bitget business-error envelopes from the probe, including environment mismatch and auth failures, are terminal validation errors and must not fall through to Classic v2 endpoints; only transport/transient failures keep the safe `isUta=false` fallback. The private startup path awaits this probe before `privateWebsocket.connectAndSubscribe()` and before `tick()`, so v3-routed accounts do not race into v2 private endpoints. Every v3 path must read `usesUtaRouting`, not bare `isUta`; `supportsUtaRouting` is inherited by livenet `bitget` and `bitgetusdc`, but stays false on `bitgetinverse` until its own UTA slice exists. The `40084` reply is a **feature signal**, not an error — it lives in `SILENT_ERROR_CODES` in `bitget.api.ts` so the response interceptor does not surface it as a toast on every Classic-account login. A `40085` reply from v2 private endpoints is the opposite signal: the key is UTA and the requested Classic API is unsupported. Supported linear variants must promote to UTA routing and retry; variants still on Classic routing should surface `BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE`.

22. **Order placement resolves position-mode + margin-type via the account endpoint, NOT via an open position.** `formatCreateOrder` / `placeOrderBatch` need `posMode` + `marginMode` to build the `side`/`tradeSide`/`marginMode` fields, and bitget is the **only** adapter that gates _every_ order on resolving these (woo/bitmex derive side without it). `getOrFetchPositionMode` / `getOrFetchMarginType` therefore go through `resolveSymbolPositioning(symbol)`, which prefers an existing position but **falls back to `fetchPositionMetadata([symbol])`** (a GET to `FETCH_SINGLE_ACCOUNT` carrying `posMode`/`marginMode` with no open position required). **This fallback is load-bearing in hosted-exchange-server mode:** user order writes run on the never-`start()`ed main-thread mirror (see `apps/web/src/controllers/CLAUDE.md` → dual-instance model), whose `store.loaded.positions` is `false`, so `getOrFetchPositionsForSymbol` returns `[]` for a flat symbol. The previous code **raw-threw** there ("No positions found …") — and because the throw bypassed `emitAndLogError`, orders silently never POSTed (no toast, no log, position never opened → e2e "expected 0 to equal 1"). Do **not** revert these methods to throwing on an empty position list.

23. **`formatCreateOrder` lot-split must guard `maxSize > 0`.** The large-order split is `lots = (maxSize > 0 && amount > maxSize) ? ceil(amount / maxSize) : 1`. The `maxSize > 0` guard is load-bearing: on the hosted-server **mirror**, the streamed market can carry `limits.amount.max === 0`, and `ceil(amount / 0) = Infinity` — lodash **`times(Infinity)` returns `[]`**, so `formatCreateOrder` produced **zero payloads** and the order silently never POSTed (no throw, no error; e2e "expected 0 to equal 1"). The same zeroed `maxSize` would surface as `rest = amount % 0 = NaN`, so `rest` is guarded identically. Real users in worker mode have a real `maxSize` and never hit this. **Root cause not yet fixed:** why the mirror's bitget market gets `limits.amount.max === 0` (server→mirror market serialization) is a separate follow-up — the guard makes order placement correct regardless, but if you touch market hydration, check whether the mirror's `limits` survive the stream.

24. **`placeOrders` must `await Promise.all` the algo payloads.** It read
    `algoOrders.map(this.formatAlgoOrder)` — an array of **unresolved Promises** —
    and passed it straight to `placeAlgoOrders`, which reads `payload.planType`
    (always `undefined` on a Promise, so every algo order took the non-trailing
    endpoint and trailing stops silently went to the wrong one) and POSTed the
    Promise itself as the request body. Typecheck does not catch this:
    `Promise<Record<string, any>>` is structurally assignable to
    `Record<string, any>`. The single-order `placeOrder` path awaited correctly,
    which is why only batch placement was affected. The `formatCreateOrder`
    line directly above it in `placeOrders` already had the correct
    `await Promise.all(...)` shape — match it.

25. **UTA attached TP/SL and standalone TP/SL are different endpoints.** Attached TP/SL on a normal market/limit order rides `POST /api/v3/trade/place-order` as `takeProfit` / `stopLoss`, trigger fields, and market strategy order types; do not split those into separate strategy orders. Standalone `OrderType.StopLoss` and `OrderType.TakeProfit` route to `POST /api/v3/trade/place-strategy-order` with `type: "tpsl"`. Full-position standalone stops send `tpslMode: "full"` and no `qty`; sized stops send `tpslMode: "partial"` plus base-coin `qty`. Fetch active standalone TP/SL through `GET /api/v3/trade/unfilled-strategy-orders?type=tpsl`, and cancel them through `POST /api/v3/trade/cancel-strategy-order` without a `category`. Do not route UTA standalone stops to Classic `PLACE_ALGO_ORDER` / `CANCEL_ALGO_ORDER`; those return the UTA-vs-Classic `40085` failure. Trailing stops and in-place strategy modify remain separate, unimplemented slices.

## Error Handling

### REST API Error Codes

- **Error response format**: `{ code: string, msg: string, data: ... }`. Code `"00000"` or `0` indicates success.
- **`"environment is incorrect"`**: API key/testnet mismatch -- detected in `validateAccount()` and `fetchOpenPositions()` with a user-friendly message.
- **Batch order failures**: `data.failureList` items are logged individually via `emitAndLogError()` and throw (`bitget.exchange.ts:1172-1175`).

### Network Retry

- `axios-retry` configured with 3 retries on network errors; `40009` is re-signed and resent once

### WebSocket Error Recovery

- Private WS events that fail to parse are silently skipped (JSON parse returns null)
- Position/order fetch failures in throttled reconciliation are caught silently (`bitget.ws-private.ts:292-294, 383-384`)
- `executeOrdersFetch` checks `fetchSeq` to discard stale responses from superseded fetches

### Error Emission Pattern

Most methods follow the pattern:

```typescript
catch (err: any) {
  this.emitAndLogError(err?.response?.data?.msg || err?.message);
  throw err;  // Re-throw for callers
}
```

Some methods (like `tick()`) catch errors without re-throwing to prevent tick loop interruption.

## Exchange Variants

### `BitgetExchange` (USDT-M Linear) - `bitget`

- **Product type**: `usdt-futures` / `susdt-futures` (testnet)
- **Margin coin**: `USDT` / `SUSDT` (testnet)
- **Status**: Enabled, production-ready
- The base implementation; all other variants extend this class

### `BitgetUSDCExchange` (USDC-M Linear) - `bitgetusdc`

- **Product type**: `usdc-futures` / `susdc-futures` (testnet)
- **Margin coin**: `USDC` / `SUSDC` (testnet)
- **Status**: Enabled
- Minimal override (`bitgetusdc.exchange.ts`): changes `apiProductType` and `apiMarginCoin` getters, inheriting livenet UTA routing as `USDC-FUTURES`
- Uses type assertion (`as any`) since the getter return types don't match the base class
- Inherits all trading logic, WebSocket handlers, and order management from base class

### `BitgetInverseExchange` (Coin-Margined) - `bitgetinverse`

- **Product type**: `coin-futures` / `scoin-futures` (testnet)
- **Margin coin**: Per-symbol base asset (e.g., `BTC` for `BTCUSD`, `ETH` for `ETHUSD`)
- **Status**: Disabled (`enabled: false`), not yet in preview
- Extensive overrides needed because inverse contracts use the base asset as margin:
  - `fetchMarkets()`: Filters by `quoteCoin === 'USD'` instead of `'USDT'`; sets `isInverse: true`
  - `fetchBalance()`: Aggregates multi-currency balances (BTC, ETH, etc.) with USD conversion via tickers; populates `coins` field
  - `fetchOpenPositions()`: Fetches per-marginCoin, converts USD contract sizes to crypto amounts (`contractsNative / referencePrice`)
  - `mapOrder()`: Converts USD remaining to crypto for display; stores `extra.remainingNative`
  - `cancelAllOrders()`: Iterates over all margin coins
  - `cancelNormalOrders()` / `cancelAlgoOrders()`: Uses per-symbol `getMarginCoinForSymbol()`
  - `placeOrderBatch()`: Uses per-symbol margin coin
  - `setLeverage()` / `changeMarginType()`: Uses per-symbol margin coin
  - Has its own `generateUuid()` helper instead of using the shared `uuid()` utility

## Timeframe / Interval Mapping

```
1m -> "1m"    3m -> "3m"     5m -> "5m"     15m -> "15m"    30m -> "30m"
1h -> "1H"    2h -> "2H"     4h -> "4H"     6h -> "6Hutc"   12h -> "12Hutc"
1d -> "1Dutc" 1w -> "1Wutc"
```

Note: 6h and above use UTC-anchored intervals (`Hutc`, `Dutc`, `Wutc`).

## Order Side Logic

### Normal Orders (V2 API) (`bitget.exchange.ts:1572-1584`)

- **One-way mode**: `side` = `Buy`/`Sell`, no `tradeSide`
- **Hedged mode (open)**: `tradeSide` = `Open`, `side` = direction
- **Hedged mode (close/reduce)**: `tradeSide` = `Close`, side is **inverted** (Buy becomes Sell, Sell becomes Buy)

### Algo/Stop Orders (`bitget.exchange.ts:1586-1623`)

- **Hedged mode**: `holdSide` = `long`/`short` (position being protected)
- **One-way mode**: `holdSide` = `buy`/`sell`
- **Trailing stop**: Uses `side` plus `tradeSide: close` in hedge mode, with `buy`/`sell` order directions.

## Testing / Validation

Run targeted type checking:

```bash
cd packages/safe-cex && yarn typecheck
```

Key validation points:

- `validateAccount()` tests API key validity by fetching balance
- `getAccount()` uses the V2 spot account-info endpoint and maps its camelCase user, parent, and inviter IDs
- Environment mismatch detection in both `validateAccount()` and `fetchOpenPositions()`

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Classic and UTA both send `clientOid`, rule `^[.A-Z:/a-z0-9_-]{1,32}$`. Inherited by inverse and USDC.
- **The native modify-order sends `newClientOid: uuid()`**, so an amend rewrites the client ID even though it keeps the order. Not changed: whether Bitget accepts the old ID as `newClientOid` is unverified, and a rejection falls back to cancel+replace silently.

## Rust account and trading audit fixes

Isolated hedge leverage preserves the requested Long/Short through the shared
command: Classic uses `holdSide`, UTA uses `posSide`; omission defaults long.
Successful writes and snapshots retain independent buy/sell settings, including
flat legs and stale-read fences. Cross and one-way leverage stay coupled.
Commands wait for the existing account probe. Anonymous orders below one lot
are skipped within a batch; explicit caller IDs still fail preflight if no
order can honor them. Sized full-position stops emit a downgrade warning.

Classic history recognizes forced-close long/short suffixes and absent profit
as zero. An unmappable or malformed REST position/order row (including one on
a market missing from a partial catalogue) is skipped with a redacted warning
and keeps only its own stored copy: orders by native id (preset TP/SL children
follow their parent id), positions by symbol and side (no side keeps both legs;
no symbol keeps every stored position). Valid peers still update; a skipped row
with no stored copy waits for the next snapshot. Actual empty lists clear state.
Rust inverse positions size coin quantity by the row's own mark price, then
ticker mark/last, then entry (TS skips the row mark), so a snapshot read before
tickers load is not sized at entry.
Malformed envelopes and failed reads never clear good state. Busy
Classic stream refreshes reschedule, zero leverage limits default to 1/20, and
invalid bounds fail visibly before writes. UTA one-way fills retain their
execution-derived position leg. The Confirm audit item remains outside this
change.

Rust private startup is gated on the `/api/v3/account/settings` probe, at
startup and after a late `40085` promotion. 40084 detects Classic; a 200 with
valid settings detects UTA. A first-probe transport or HTML 5xx failure keeps
the TS fallback to Classic (a later 40085 promotes). Every other failure (429/418,
5xx or system-code envelopes, non-JSON, malformed settings, unknown codes, and
transport failures once UTA is confirmed) retries through `startup::Backoff`
(1 s doubling to 30 s, forever), so polling never stalls undetected. Before the
account loads, only documented key refusals end startup with one
`StartupFailed`: 40001/40006/40011/40012/40037 or an "environment is incorrect"
message (`AuthInvalid`), 40014/40018 (`AuthPermission`), and 40009 after one
re-signed resend; timestamp codes stay transient. A UTA account on inverse or
testnet fails startup as `Unsupported`, a missing passphrase or invalid
read-only credentials as `InvalidInput`. Before the account loads, a socket login
refused with 30011/30012 (`event: "error"` or `login`) fails startup; other login
errors (30005 etc.) reconnect with backoff. A rejection stops the probe and
account timers and retires the private socket; a refused trading key also emits
`Deactivate`. A 418/429 sets `rest_paused_until` from `Retry-After`, else 30 s,
and the probe retry waits out that window; transport failures keep plain backoff.

## Rust private WS row recovery

Ordinary order/position arrays decode rows independently. Valid Classic/UTA peers
advance while rejected cached identities retain exposure, protection and native
routing metadata. Unknown identity holds absence. Rejection observers protect
older pending REST reads without replacing or renewing actual WS financial
freshness; fresh admitted reads can heal. Partial snapshots preserve an earlier
clean snapshot's older-read fence without claiming new global financial authority.
All-rejected initial snapshots never establish loaded-empty readiness; valid empty
snapshots still load and clear positions.

Position mode is account/product-wide: partial or contradictory rows, including
incremental deltas, cannot flip global routing while retaining exposure in another
mode. A complete consistent snapshot can change mode. Retained cached positions
do not republish stale settings. Native plans and trailing metadata use the same
revision/terminal/opened fence as retained order rows, preserving cancel routes.
Read-only recovery uses the existing coalesced timers and busy/pause handling;
malformed-only or foreign-only traffic does not claim useful financial liveness.
The financial fill channel, auth, signing and write-retry policy are unchanged.

## Rust dispatch signer checkpoint

The draft native migration stages private HTTP and Classic/UTA login with shared
engine SignStep work. Venue state carries opaque identities; trusted setup owns
all material. Private primary passphrase is mandatory; an absent secondary
passphrase inherits its primary provider ref, while explicit empty rejects.
Ordinary GET uses secondary, degraded balance recovery remains primary, and POST
and WS stay primary. Public broker headers remain unchanged.

Owned identity/history/trade read provider refusal bypasses network read retries.
Background account failures retain existing recovery/backoff, and provider refusal
cannot promote an unconfirmed Classic account. Earlier accepted writes, native40009 resend evidence
and original rollback diagnostics retain existing outcome semantics. Hosted
admission is described below. No secret restoration or venue-local signing
fallback is permitted.

## Rust hosted and remote signing

Hosted `signHttpRequest`/`signWsAuth` and keyless `remoteSigner` are admitted
for all three aliases through the shared owned provider; no TS venue is built.
`bitget.signing.ts` is the standalone HTTP codec shared with the `bitget.api.ts`
interceptor: same `$` symbol strip (except candles), `encode: false` query,
underscore-stripped signed body and ms timestamp. Hosted capture uses primary
custody only and sets `ACCESS-KEY`/`ACCESS-SIGN`/`ACCESS-TIMESTAMP`/
`ACCESS-PASSPHRASE`; the passphrase comes from the provider's Present result.

Remote WS login keeps the legacy wire (`exchangeName: 'bitget'`, Base64,
`extraData.timestamp` in seconds over `timestamp + "GET" + "/user/verify"`).
The decode is declared in `construction-rules.json` as `secondsTimestamp` with
`secondsSuffix`, shared with OKX; native login work is three typed parts.
Private `PayloadSigner` is typed unsupported (the TS interceptor never calls
it), RemoteSigner wins without fallback, and primary-only remote wire refuses
secondary-identity work before any callback. Actual-WASM proofs are CI-run.
