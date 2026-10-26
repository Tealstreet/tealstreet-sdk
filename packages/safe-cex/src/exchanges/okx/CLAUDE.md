# OKX Exchange Integration

This directory contains the OKX exchange integration for Tealstreet. Regular OKX supports **linear and inverse perpetual swaps**. OKX EU additionally supports **X-Perps**, which OKX exposes as `FUTURES` instruments with `ruleType === 'xperp'`. Spot instruments are not requested.

## Exchange Information

- **Exchange**: OKX (formerly OKEx)
- **Type**: Centralized Exchange (CEX)
- **Supported products**: Linear and inverse perpetual swaps (`ctType === 'linear' || ctType === 'inverse'`); OKX EU also includes X-Perp `FUTURES` rows where `ruleType === 'xperp'`
- **API version**: v5
- **Base URL in code**: `https://www.okx.com` (`okxeu` uses `https://eea.okx.com`; `okxdemo` uses the official Demo REST host `https://openapi.okx.com`; `okxeudemo` uses `https://eea.okx.com`; demo variants add `x-simulated-trading: 1` and route REST through Tealstreet's OKX REST proxy path). OKX Global now documents `https://openapi.okx.com` as its primary REST domain; `www.okx.com` remains supported for live global REST.
- **Latency domain**: `www.okx.com` (`okxdemo`: `openapi.okx.com`; `okxeu` / `okxeudemo`: `eea.okx.com`)
- **Browser transport**: `okxeu` requires the Tealstreet REST/WS proxy route. Direct Node requests to `eea.okx.com` can authenticate, but browser private REST fails CORS preflight (`OPTIONS`) before OKX can process the signed request. `okxdemo` and `okxeudemo` require the Tealstreet REST proxy route, while demo WebSockets stay direct to `wspap.okx.com` / `wseeapap.okx.com`. Keep `okxeu.requiresCors = true` and demo metadata `requiresCors = true`; regular `okx` remains direct-capable by metadata.
- **okxeu WS tunnel paths**: private/business sockets tunnel via dedicated `okxeu/private` / `okxeu/business` paths (see `transport/tealstreet/routing/routingTable.ts`), whose legacy Python ws-proxy configuration targets `wss://wseea.okx.com:8443` (the upstream must move to 443; this SDK change does not deploy it). Do NOT reuse the global `okx/*` paths — they terminate on `ws.okx.com`, where EEA credentials fail auth. `eea.okx.com` + `wseea.okx.com` are allowlisted in tradetunnel `ALLOWED_URLS` and in every live HAProxy proxy config (July 2026 rollout).

## API Documentation

When working with OKX exchange integration, consult the official API documentation:

- **Official API Documentation (v5)**: https://www.okx.com/docs-v5/
- **REST API**: https://www.okx.com/docs-v5/en/#rest-api
- **WebSocket API**: https://www.okx.com/docs-v5/en/#websocket-api

### CCXT Reference Implementation

- **REST API Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/okx.ts
- **WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/okx.ts

## File Structure

| File                    | Class/Export           | Description                                                                                                  |
| ----------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------ |
| `okx.types.ts`          | Constants, mappings    | Endpoint paths, broker ID, order status/type/side/interval mappings, WebSocket URLs                          |
| `okx.api.ts`            | `createAPI()`          | REST API client factory with HMAC-SHA256 signing via axios interceptors                                      |
| `okx.exchange.ts`       | `OKXExchange`          | Main exchange class extending `BaseExchange`; all trading logic                                              |
| `okxdemo.exchange.ts`   | `OKXDemoExchange`      | Minimal subclass that forces `testnet: true` / `isDemo = true` while reusing global OKX trading logic        |
| `okxeudemo.exchange.ts` | `OKXEUDemoExchange`    | Minimal subclass that forces `testnet: true` / `isDemo = true` while reusing OKX EU trading logic            |
| `okx.ws-public.ts`      | `OKXPublicWebsocket`   | Public WebSocket for market data (tickers, mark price, index, OI, funding, order book)                       |
| `okx.ws-private.ts`     | `OKXPrivateWebsocket`  | Private WebSocket for account, positions, and regular order updates                                          |
| `okx.ws-business.ts`    | `OKXBusinessWebsocket` | Business WebSocket for candles (OHLCV), algo orders, and algo-advance updates; extends `OKXPrivateWebsocket` |

## OKX EU market list on exchange-server

`okxeu` lists markets from the signed `ACCOUNT_INSTRUMENTS` (what this EEA
account may trade). The keyless shared exchange can only use the public list, so
`OKXEUExchange.fetchAccountMarketOverlay` narrows the shared markets to the
account's instruments and takes their `limits`. See safe-cex `CLAUDE.md` →
"Account data on shared public markets".

## Authentication

> **A 401 on a proxied OKX request is usually NOT OKX.** `EXCHANGE-SERVER-27`
> looked like OKX rejecting an API key for six days and was our own proxy
> credential arriving malformed — an empty token becomes `Bearer `, axios trims
> it to `Bearer`, and the edge reports that as a MISSING header. A bad token
> would be a 403, not a 401. Check `restProxyHeaderSent` on the failure sample
> before touching anything here; see `services/exchange-server/CLAUDE.md` →
> "A venue 401 through the proxy is often OUR credential, not the venue's".

OKX requires three credentials: **API key**, **API secret**, and **passphrase**. The passphrase is mandatory and validated at API client creation time; `createAPI` throws `'OKX requires a passphrase'` if missing.

### REST API Signing

Signing is performed in an axios request interceptor:

1. **Public endpoint bypass**: Requests to URLs containing `'public'` skip signing entirely
2. **Timestamp**: ISO 8601 format from `virtualClock.getCurrentTime().toISOString()` (e.g., `2024-01-15T12:30:00.000Z`)
3. **String to sign**: Concatenation of `timestamp + METHOD + url + body`
   - `url` includes query parameters (decoded via `qs.stringify`)
   - `body` is JSON-stringified data or empty string
   - The signing interceptor writes that exact JSON string back to `config.data` and preserves already-serialized bodies. Do not sign `JSON.stringify(config.data)` without also pinning the sent body; proxy/remote-signing/retry paths can otherwise sign quoted JSON while transmitting unquoted JSON, which OKX rejects as `50113 Invalid Sign`.
4. **Signature**: HMAC-SHA256 of the string-to-sign using the API secret, output as **base64**
5. **Headers**:
   - `OK-ACCESS-KEY`: API key
   - `OK-ACCESS-SIGN`: Base64 HMAC-SHA256 signature
   - `OK-ACCESS-TIMESTAMP`: ISO 8601 timestamp
   - `OK-ACCESS-PASSPHRASE`: Passphrase
6. **Testnet header**: When `options.testnet` is true, adds `x-simulated-trading: 1`

**Signer-failure branch does NOT say "auth failed", on either private or business.** Both `okx.ws-private.ts` and `okx.ws-business.ts` `auth` emit `Remote WS signing unavailable: …` on the `remoteSigner.sign` error branch, not `Remote WS auth failed: …`. The latter wording matches `health/errorPatterns.ts`'s `auth_invalid` (case-insensitive substring), which the web controller treats as a terminal API-key rejection and suppresses automatic account reloads for five minutes — the very reload a signer-less tab needs to re-pair. See `../CLAUDE.md` → "Remote-signer WS auth" and the bybit template in `bybit/bybit.ws-private.ts`.

### WebSocket Authentication (Private + Business)

Both private and business WebSockets authenticate after connecting via a `login` operation:

1. **Timestamp**: Unix epoch seconds from `virtualClock.getCurrentTime().unix()`
2. **String to sign**: `timestamp + 'GET' + '/users/self/verify'`
3. **Signature**: HMAC-SHA256 with API secret, output as **base64**
4. **Login payload**:
   ```json
   {
     "op": "login",
     "args": [{ "apiKey": "...", "passphrase": "...", "timestamp": 1234567890, "sign": "base64sig" }]
   }
   ```
5. On successful login (response `code === 0`), the `subscribe()` method is called

### Time Sync

The `virtualClock` is used for both REST and WebSocket authentication timestamps. This ensures clock synchronization even if the local system clock drifts.

### Account Validation

Regular OKX validates credentials by fetching `ENDPOINTS.BALANCE` (`/api/v5/account/account-position-risk`). OKX EU overrides `validateAccount()` to fetch `ENDPOINTS.ACCOUNT_CONFIG` (`/api/v5/account/config`) instead, because the OKX EU host can return HTTP 503 on `account-position-risk` during the new-account validation gate even when the key, secret, and passphrase are otherwise valid. Startup still fetches balance and positions through the normal SWAP-scoped balance path after account config has loaded.

Both variants return the current validation contract `{ error, balance? }`; they do not return a bare error string.

## Startup Sequence

Defined in `OKXExchange.start()`:

### Phase 1: Market Data (parallel)

```
Promise.all([
  fetchMarkets()   --> store.update({ markets, loaded.markets: true })
  fetchTickersRaw() --> returns raw ticker data
])
```

- **Markets** (`/api/v5/public/instruments?instType=SWAP`; OKX EU also `instType=FUTURES`): Fetches supported instruments, keeps linear/inverse contracts, and stores them. Signed OKX EU / OKX EU Demo sessions use the account-scoped `/api/v5/account/instruments` endpoint instead, because EEA demo/live accounts can have generic `trade` permission while only being entitled to a subset of public markets. For OKX EU, regular dated futures are ignored unless `ruleType === 'xperp'`.
- **Tickers** (`/api/v5/market/tickers?instType=SWAP`; OKX EU also `instType=FUTURES`): Fetched in parallel with markets

### Phase 2: Parse & Connect Public WS (sequential)

1. Parse raw tickers via `parseTickers()` (requires markets to be loaded first for `marketsById` lookup)
2. Store tickers and set `loaded.tickers: true`
3. Connect public WebSocket (`publicWebsocket.connectAndSubscribe()`)
4. Connect business WebSocket (`businessWebsocket.connectAndSubscribe()`)

### Phase 3: Early exit for public-only mode

If `publicDataOnly` or missing key/secret, log and return. No private connections are made.

### Phase 4: Account Config (sequential, blocks on result)

1. `getOrFetchPositionMode()` -- fetches `/api/v5/account/config` to get `posMode` and `acctLv`
   - Caches results in `_accountPositionMode` and `_accountLevel`
   - **Must complete before positions can be mapped** (position ID generation depends on position mode)

### Phase 5: Private WS + Data (parallel after config)

1. Connect private WebSocket (`privateWebsocket.connectAndSubscribe()`)
2. In parallel:
   - `fetchBalanceAndPositions()` -- fetches balance + positions, stores with `loaded.balance: true, loaded.positions: true`
   - `fetchOrders()` -- fetches all order types (normal + 4 algo types), stores with `loaded.orders: true`

### Phase 6: Start Tick Loop

- Calls `tick()` which recalculates balance on each cycle
- The tick loop runs every `tickInterval` ms (2000ms by default) via the `loop()` utility
- `loop()` uses `requestAnimationFrame` when the tab is visible, falls back to `setTimeout` when hidden

### Disposal Check

Between Phase 1 and Phase 2, `if (this.isDisposed) return;` prevents continuing if the exchange was disposed during async fetches.

## REST API Endpoints

All endpoints are defined in `ENDPOINTS` in `okx.types.ts`:

| Constant               | Path                                    | Method | Description                                                                    |
| ---------------------- | --------------------------------------- | ------ | ------------------------------------------------------------------------------ |
| `ACCOUNT`              | `/api/v5/account/config`                | GET    | Get account configuration (position mode, account level)                       |
| `ACCOUNT_CONFIG`       | `/api/v5/account/config`                | GET    | Same path as ACCOUNT (used for position mode/account level fetch)              |
| `ACCOUNT_INSTRUMENTS`  | `/api/v5/account/instruments`           | GET    | Account-available instruments for signed OKX EU / OKX EU Demo market discovery |
| `PARTNER`              | `/api/v5/users/partner/if-rebate`       | GET    | Check partner rebate status                                                    |
| `MARKETS`              | `/api/v5/public/instruments`            | GET    | Fetch available instruments (`instType=SWAP`)                                  |
| `TICKERS`              | `/api/v5/market/tickers`                | GET    | Fetch ticker data (`instType=SWAP`)                                            |
| `BALANCE`              | `/api/v5/account/account-position-risk` | GET    | Account balance and position risk (`instType=SWAP`)                            |
| `POSITIONS`            | `/api/v5/account/positions`             | GET    | Fetch open positions (`instType=SWAP`)                                         |
| `KLINE`                | `/api/v5/market/candles`                | GET    | Fetch OHLCV candlestick data                                                   |
| `KLINE_HISTORY`        | `/api/v5/market/history-candles`        | GET    | Fetch older OHLCV data when recent candles cannot cover the request            |
| `UNFILLED_ORDERS`      | `/api/v5/trade/orders-pending`          | GET    | Fetch unfilled regular orders (paginated, 100 per page)                        |
| `UNFILLED_ALGO_ORDERS` | `/api/v5/trade/orders-algo-pending`     | GET    | Fetch unfilled algo orders (paginated, 100 per page)                           |
| `FILLS`                | `/api/v5/trade/fills`                   | GET    | Recent fills (primary for `fetchHistoricalFills`)                              |
| `FILLS_HISTORY`        | `/api/v5/trade/fills-history`           | GET    | Older fills fallback for `fetchHistoricalFills`                                |
| `PLACE_ORDERS`         | `/api/v5/trade/batch-orders`            | POST   | Place batch orders (up to 20 per batch)                                        |
| `PLACE_ALGO_ORDER`     | `/api/v5/trade/order-algo`              | POST   | Place single algo order                                                        |
| `UPDATE_ORDER`         | `/api/v5/trade/amend-order`             | POST   | Amend an existing order                                                        |
| `CANCEL_ORDERS`        | `/api/v5/trade/cancel-batch-orders`     | POST   | Cancel batch orders (up to 20 per batch)                                       |
| `CANCEL_ALGO_ORDERS`   | `/api/v5/trade/cancel-algos`            | POST   | Cancel algo orders (up to 10 per batch)                                        |
| `SET_LEVERAGE`         | `/api/v5/account/set-leverage`          | POST   | Set leverage for an instrument                                                 |
| `LEVERAGE`             | `/api/v5/account/leverage-info`         | GET    | Get leverage info (supports comma-separated instIds, batched in groups of 20)  |
| `SET_POSITION_MODE`    | `/api/v5/account/set-position-mode`     | POST   | Switch position mode (`long_short_mode` or `net_mode`)                         |

Execution-history behavior (`fetchHistoricalFills`):

- Fetches `/trade/fills` first, then `/trade/fills-history` if needed
- Converts contract-size amounts to base qty using `contractsToBase(...)`
- Normalizes fill timestamps to Unix seconds and resolves symbol from `instId`
- Ingestion gates this through the real `fetchHistoricalFills` override and the
  journal axis tables, not through exchange metadata.

## WebSocket Implementation

OKX uses **three separate WebSocket connections**, all on the default TLS port 443.

The SDK direct URLs and global private/business relay targets omit the port,
so secure WebSockets use 443. The Python ws-proxy upstreams for both
`ws.okx.com` and `wseea.okx.com`, the `okx-proxy-ja` Caddy passthrough upstream,
and tradetunnel's `RELAY_ALLOWED_UPSTREAMS` host/path allowlist must also be
checked and moved to 443 before OKX retires 8443 on 2026-10-31. Changing these
SDK constants does not deploy those infrastructure updates.
See the [OKX changelog](https://www.okx.com/docs-v5/log_en/).

### URLs

| Type     | Production                             | OKX Demo                                  | OKX EU Demo                                  |
| -------- | -------------------------------------- | ----------------------------------------- | -------------------------------------------- |
| Public   | `wss://ws.okx.com/ws/v5/public`   | `wss://wspap.okx.com/ws/v5/public`   | `wss://wseeapap.okx.com/ws/v5/public`   |
| Private  | `wss://ws.okx.com/ws/v5/private`  | `wss://wspap.okx.com/ws/v5/private`  | `wss://wseeapap.okx.com/ws/v5/private`  |
| Business | `wss://ws.okx.com/ws/v5/business` | `wss://wspap.okx.com/ws/v5/business` | `wss://wseeapap.okx.com/ws/v5/business` |

`okxdemo` is a separate exchange variant because OKX Demo Trading uses separate
demo API credentials. It forces `options.testnet = true`, which selects the
right-hand `wspap.okx.com` sockets and adds `x-simulated-trading: 1` on every
REST request. Metadata keeps `supportsTestnet: false` because the generic OKX
testnet checkbox is still not the product surface; users choose the separate
OKX Demo environment instead. Demo REST is intentionally proxied through the
existing OKX/Tokyo Tealstreet REST proxy pool so third-party-linked keys validate
from Tealstreet routing rather than the user's browser IP.

`okxeudemo` follows the same separate-credential pattern for EEA demo keys.
Those keys validate on `https://eea.okx.com` with `x-simulated-trading: 1`;
global `openapi.okx.com` returns `50119` for EEA-scoped demo keys.

### Demo Validation Status (2026-08-21)

- `okxdemo` is functionally ready in code, but still needs confirmation
  validation with a known-good Global OKX Demo key. In particular, verify that
  Global demo credentials authenticate and trade against the documented
  `https://openapi.okx.com` REST host with `x-simulated-trading: 1`. If
  `openapi.okx.com` fails for a valid Global demo key, re-test `https://www.okx.com`
  before changing product assumptions.
- `okxeudemo` is functionally working with an EEA-scoped demo key: account
  linking, charts, balance loading, and market order placement succeeded on the
  account-available X-Perp instrument
  (`BTC-USD_UM_XPERP-310328` / compact `BTCUSD_UM_XPERP310328`).
- Do not treat the observed OKX EU product scope as globally proven yet. The
  tested EEA demo account reported generic `read_only,trade` API permissions,
  but `/api/v5/account/instruments?instType=SWAP` returned no BTC SWAP markets
  and `/api/v5/account/instruments?instType=FUTURES` returned BTC USD_UM dated
  futures plus one X-Perp. More OKX EU live/demo accounts are needed to decide
  whether this is universal OKX EU behavior, jurisdiction/account-specific
  entitlement, or demo-only scope. Use `/api/v5/account/instruments` as the
  account-scoped source of truth; public `/api/v5/public/instruments` only proves
  that a product exists on the venue, not that a given account may trade it.

Do NOT route `okxdemo` or `okxeudemo` private/business sockets through the
Tealstreet WS proxy. Unlike Bybit Demo, OKX Demo authenticates with a normal
post-connect `login` frame, so browser-native WebSocket connections to
`wspap.okx.com` / `wseeapap.okx.com` are the intended path. Demo metadata sets
`requiresCors: true` for REST, but not `enforceWsTunnel: true`, and
`routingTable.ts` intentionally leaves demo variants out of the OKX
private/business WS override groups.

Private and business WebSockets support Tealstreet proxy routing. All three
`connectAndSubscribe` methods resolve their host from the shared routing
resolver instead of hardcoding a URL:

```ts
const route = resolveExchangeRouting(this.parent.name, this.parent.options).wsPrivate; // or .wsBusiness / .wsPublic
const baseUrl = this.directFallback.resolveUrl(route.tunnelled ? route.host! : this.endpoint, route.fallback);
this.ws = this.createWebsocket(baseUrl);
```

`directFallback` is a `WsDirectFallback` (`utils/wsDirectFallback.ts`; `protected`
on the private class because the business class extends it). After two consecutive
connects that produced zero messages it dials `route.fallback` — the athena Caddy
passthrough `okx-proxy-ja.tealstreet.io` (legacy upstream 8443, pending an infrastructure change to 443) —
for the rest of the socket's lifetime and emits `grafana-info`. The resolver attaches
`fallback` only to a DIRECT resolution, so with `tealstreetProxy` on the private and
business sockets keep the chosen EKS tunnel and never degrade to the passthrough.
Only global `okx` declares it: `okxeu` credentials do not authenticate on
`ws.okx.com`, and the demo variants terminate on `wspap`/`wseeapap`.

Private and business also carry `route.relay` — the tradetunnel's `rpc.relay`
through the private proxy fleet (`RelayWebSocket`) — walked BEFORE the athena
passthrough and only while `transport/tealstreet/wsRelay` is on (`WS_RELAY_PRIVATE` flag). The
fleet's `ws.okx.com` route exists since 2026-09-25; before that a relay attempt
failed silently and the chain moved on to athena, which is also what happens
on a tunnel without `RELAY_ENABLED`.

The actual tunnelled host (the `wss://fsjnpy143pbz.*` family) and the
`tealstreetProxy` gating live in `transport/tealstreet/routing/routingTable.ts` + the
`resolveExchangeRouting` resolver — not in the OKX ws classes. To change
where OKX private/business traffic egresses, edit the table.

### Ping/Pong

All three WebSocket types send `'ping'` as a text frame. The server responds with `'pong'` (text frame, not a WebSocket pong frame).

- **Ping interval**: 5 seconds after receiving pong
- **Pong miss timeout**: 7.5 seconds (1.5x ping interval) -- triggers reconnection
- **Stale message detection**: `trackLastReceived()` runs a 1-second check loop:
  - If no message for >6 seconds (`WS_PING_THRESHOLD_MS`): sends a ping
  - If no message for >20 seconds (`WS_RECONNECT_THRESHOLD_MS`): triggers reconnection

### Reconnection Logic

Reconnection is handled by `BaseWebSocket` (`base.ws.ts`):

- **ReconnectingWebSocket mode** (`ENABLE_RECONNECTING_WEBSOCKET = false` by default): Currently disabled; uses native WebSocket with manual reconnection
- **Native WebSocket reconnection**: On close, waits `WS_MIN_RECONNECTION_DELAY` (1-3 seconds, randomized), then calls `connectAndSubscribe()`
- **Max reconnect count**: 100 attempts before giving up
- **Private WS reconnect**: On reconnection, calls `fetchAndSetPrivateSnapshot()` which re-fetches all orders from REST to reconcile state
- **Public WS reconnect**: Emits `wsReconnected` event to notify the application

### Public WebSocket Channels (`okx.ws-public.ts`)

Subscribed per-instrument for all loaded markets on `onOpen`:

| Channel         | Handler                    | Description                                                                            |
| --------------- | -------------------------- | -------------------------------------------------------------------------------------- |
| `tickers`       | `handleTickerEvents`       | Bid, ask, last price, 24h percentage, volume                                           |
| `mark-price`    | `handleMarkPriceEvents`    | Mark price updates                                                                     |
| `index-tickers` | `handleIndexTickerEvents`  | Index price (subscribed with `-SWAP` stripped from instId)                             |
| `open-interest` | `handleOpenInterestEvents` | Open interest in contract currency (`oiCcy`)                                           |
| `funding-rate`  | `handleFundingRateEvents`  | Funding rate and next funding time                                                     |
| `books`         | Order book listener        | Snapshot + incremental updates (subscribed on-demand per symbol via `listenOrderBook`) |

**Message routing**: Public and business frames are parsed once and routed only
from the exact `arg.channel`, `arg.instId`, and (where present) `arg.instType`
tuple. Public order books use `books.{instId}`; business candle wire channels
(`candle1m`, `candle1H`, etc.) reverse-map through `INTERVAL` to the internal
`candle.{timeframe}.{instId}` handler key. Algo-order channels are matched by
their exact `arg.channel`. Handler stats are updated only when that exact key
has callbacks; payload fields outside `arg` never influence dispatch.

**Order channels record ONE collapsed stats topic, `orders`.** `orders`,
`orders-algo` and `algo-advance` all record under that single key. OKX pushes an
order channel only for a category the account currently holds orders in, so
tracking them separately produced up to three entries that went quiet
independently as categories emptied — `algo-advance` was measured at 1199s while
every other feed on the same socket stayed 1s fresh, which marked the account
delayed in the reconnect banner and, past the error threshold, fed
`requestReload` in `exchangeAccountController`.

The other half of the fix is in the client: `getGroupedMessageTopicStats` skips
staleness for order topics when the account holds no open orders, since
flattening is what stops the traffic. Do NOT instead stop recording these
channels — that is a subscription-completeness check and silencing it would hide
a failed order subscribe permanently. See `../CLAUDE.md` → "`messageHandlerStats`
checks SUBSCRIPTIONS, not the connection".

**The two halves must be deployed together, and only one of them ships with the
client.** The client half is a substring test — `isOrderEventTopic` matches
`'order'` and excludes `'book'` — run against a topic key it did not create.
`messageHandlerStats` is part of the serialized store (`store/store.base.ts`), so
for any venue the client cannot reach directly the keys are produced by whichever
build runs the socket. OKX is `requiresCors: true`, so that is the
exchange-server.

`algo-advance` does not contain the substring `order`. So a client carrying the
gate, talking to an exchange-server that predates the collapse, reads a key the
gate cannot match and warns exactly as before — with the fix visibly present in
source, present in the bundle, and doing nothing. There is no error and no log
line; the only symptom is the banner you were trying to remove. Observed
2026-08-08: the fix landed at 08:20, a mobile bundle rebuilt from it at 13:40
still showed `algo-advance` as the warned topic at 13:47, and current safe-cex
has no code path that can produce that key. The exchange-server had not been
redeployed. Redeploying it is the expected remedy; that had not yet been
confirmed when this was written.

If this ever recurs, check the deployed exchange-server build before re-reading
the client code. Do not "fix" it by broadening the substring match to catch
`algo-*` — that hides the version skew instead of surfacing it, and the skew
affects every other collapsed-topic gate the same way.

### Private WebSocket Channels (`okx.ws-private.ts`)

Subscribed after successful login:

| Channel                      | Handler               | Description                                                             |
| ---------------------------- | --------------------- | ----------------------------------------------------------------------- |
| `account`                    | `handleAccountTopic`  | Total equity updates for balance calculation                            |
| `positions` (instType: SWAP) | `handlePositionTopic` | Position updates including entry price, leverage, margin, PnL           |
| `orders` (instType: SWAP)    | `handleOrderTopic`    | Regular order status changes (live, partially_filled, filled, canceled) |

**Error event handling**: Messages containing `"event":"error"` are parsed and the error message is emitted via `emitAndLogError`.

### Business WebSocket Channels (`okx.ws-business.ts`)

Extends `OKXPrivateWebsocket` class. Subscribed after successful login:

| Channel                         | Handler                        | Description                                                          |
| ------------------------------- | ------------------------------ | -------------------------------------------------------------------- |
| `orders-algo` (instType: SWAP)  | `handleOrderTopic` (inherited) | Algo order updates (TP/SL/conditional/trailing)                      |
| `algo-advance` (instType: SWAP) | `handleOrderTopic` (inherited) | Advanced algo order updates                                          |
| `candle{interval}`              | OHLCV listener                 | Candlestick data (subscribed on-demand per symbol via `listenOHLCV`) |

### Order Book Subscription Pattern

Order book is subscribed on-demand (not at startup):

1. `listenOrderBook(symbol, callback)` waits for markets to be loaded (polls every 100ms)
2. Waits for WebSocket to be connected (polls every 100ms)
3. Subscribes to `books` channel for the specific `instId`
4. **Snapshot** (`action === 'snapshot'`): Replaces entire order book, filtering zero-amount entries
5. **Delta updates** (`action === 'update'`): Applies incremental updates via `updateOrderBook()`
6. Every snapshot records `seqId`; every delta must have `prevSeqId` equal to
   the last accepted sequence. A discontinuity is ignored and triggers one
   unsubscribe/subscribe cycle; deltas remain suppressed until a fresh snapshot.
   This predecessor check intentionally remains bespoke instead of using
   `SequencedDeltaOrderBook`: OKX documents valid maintenance-reset deltas where
   `prevSeqId` matches the last message but `seqId` becomes smaller. The shared
   helper currently treats every lower sequence as stale and would discard that
   valid depth change. Its OKX adoption gate is a reset-aware policy plus
   deterministic maintenance-reset coverage; do not migrate before both exist.
   An empty keepalive update with `prevSeqId === seqId` is also valid. Replayed or
   otherwise discontinuous frames fail closed through the existing resubscribe.
   The deprecated `checksum` remains present as `0` and is not consulted.
7. **Post-processing**: Filters out invalid prices (bids > last price, asks < last price), sorts, and calculates totals
8. Stores topic for re-subscription on reconnect; reconnect clears the old sequence state
9. The cleanup function filters out its parser; when the last parser is
   removed, it deletes the reconnect topic and sends the channel unsubscribe

### OHLCV Subscription Pattern

Similar on-demand pattern:

1. Waits for markets loaded and WebSocket connected
2. Subscribes to `candle{OKX_INTERVAL}` channel for the specific `instId`
3. Candle data format: array where index 0=timestamp(ms), 1=open, 2=high, 3=low, 4=close, 7=volume
4. Returns cleanup function that unsubscribes

## Order Types and Mappings

### Regular Orders (via batch-orders endpoint)

| Internal Type                    | OKX `ordType` | Notes                        |
| -------------------------------- | ------------- | ---------------------------- |
| `OrderType.Market`               | `market`      | Market order                 |
| `OrderType.Limit`                | `limit`       | Limit order (GTC by default) |
| `OrderType.Limit` + PostOnly TIF | `post_only`   | Post-only limit order        |

### Algo Orders (via order-algo endpoint)

| Internal Type                | OKX `ordType`     | Notes                                               |
| ---------------------------- | ----------------- | --------------------------------------------------- |
| `OrderType.Conditional`      | `trigger`         | Opening trigger order; uses `triggerPx` + `orderPx` |
| `OrderType.StopLoss`         | `conditional`     | Uses `slTriggerPx`, `slOrdPx=-1` (market execution) |
| `OrderType.TakeProfit`       | `conditional`     | Uses `tpTriggerPx`, `tpOrdPx=-1` (market execution) |
| `OrderType.TrailingStopLoss` | `move_order_stop` | Uses `callbackSpread`, `closeFraction=1`            |

### Fetched Algo Order Types

When fetching orders, four algo types are queried separately:

- `oco` -- One-cancels-other orders
- `conditional` -- Stop loss / take profit orders
- `trigger` -- Opening conditional trigger orders
- `move_order_stop` -- Trailing stop loss orders

### Time in Force

| OKX `ordType` | Mapped TIF                        |
| ------------- | --------------------------------- |
| `post_only`   | `OrderTimeInForce.PostOnly`       |
| All others    | `OrderTimeInForce.GoodTillCancel` |

### Stop Trigger Types

Supported trigger types for TP/SL (defined in metadata `supportedTPSLTriggerTypes`):

- `OrderStopTriggerType.LastPrice` (default for TP)
- `OrderStopTriggerType.MarkPrice` (default for SL)

OKX API trigger type values: `'last'`, `'mark'`, `'index'`.

Opening conditional orders (`OrderType.Conditional`) are deliberately separate
from TP/SL. They use OKX `ordType: "trigger"` with `triggerPx`,
`orderPx` (`-1` for market execution), and `triggerPxType`. Do not route these
through OKX `ordType: "conditional"`; that OKX value is reserved here for
protective TP/SL-style algos using `tpTriggerPx` / `slTriggerPx`.

Reduce-only `ordType: "trigger"` rows are classified as TP/SL only when the
selected trigger source can be trusted. Startup tickers seed OKX `mark` and
`index` from `last` until the public websocket sends real `mark-price` /
`index-tickers` updates, so mark/index reduce-only triggers stay mapped as
neutral conditional orders until that source has arrived. Authenticated OKX
instances that use in-process public data consult the OKX source exchange's
loaded-source flags; websocket gateway snapshots are trusted only when their
mark/index value is finite and differs from the seeded `last` placeholder.

**Trigger type mapping**:

- SL defaults to `'mark'` when no trigger type specified
- TP defaults to `'last'` when no trigger type specified

### Order Status Mapping (`ORDER_STATUS`)

| OKX State          | Internal Status    |
| ------------------ | ------------------ |
| `live`             | `OrderStatus.Open` |
| `partially_filled` | `OrderStatus.Open` |

`filled` and `canceled` states are handled via WebSocket events to update/remove orders.

### Order Type Mapping (`ORDER_TYPE`)

| OKX `ordType`       | Internal Type      |
| ------------------- | ------------------ |
| `market`            | `OrderType.Market` |
| `limit`             | `OrderType.Limit`  |
| `post_only`         | `OrderType.Limit`  |
| `fok`               | `OrderType.Limit`  |
| `ioc`               | `OrderType.Limit`  |
| `optimal_limit_ioc` | `OrderType.Market` |

### Reverse Order Type (`REVERSE_ORDER_TYPE`)

| Internal Type                | OKX `ordType`     |
| ---------------------------- | ----------------- |
| `OrderType.Market`           | `market`          |
| `OrderType.Limit`            | `limit`           |
| `OrderType.Conditional`      | `trigger`         |
| `OrderType.StopLoss`         | `conditional`     |
| `OrderType.TakeProfit`       | `conditional`     |
| `OrderType.TrailingStopLoss` | `move_order_stop` |

### Order Side (`ORDER_SIDE`)

| OKX Side | Internal Side    |
| -------- | ---------------- |
| `buy`    | `OrderSide.Buy`  |
| `sell`   | `OrderSide.Sell` |

### Position Side (`POSITION_SIDE`)

| OKX Side | Internal Side        |
| -------- | -------------------- |
| `long`   | `PositionSide.Long`  |
| `short`  | `PositionSide.Short` |

### Candle Intervals (`INTERVAL`)

| Internal | OKX     |
| -------- | ------- |
| `1m`     | `1m`    |
| `3m`     | `3m`    |
| `5m`     | `5m`    |
| `15m`    | `15m`   |
| `30m`    | `30m`   |
| `1h`     | `1H`    |
| `2h`     | `2H`    |
| `4h`     | `4H`    |
| `6h`     | `6H`    |
| `12h`    | `12H`   |
| `1d`     | `1D`    |
| `1w`     | `1W`    |
| `1M`     | `1Mutc` |

Note: Minutes use lowercase (e.g., `1m`), hours and days use uppercase (e.g., `1H`, `1D`).

**Monthly is `1Mutc`, never `1M`.** OKX's plain `1M`, and the `6H`, `12H`, `1D`
and `1W` above, are **Hong Kong time**: they open at 16:00 UTC (measured live
2026-10-10: `1M` opened 2026-09-30T16:00Z, `1W` Sunday 2026-10-04T16:00Z, `1D`
2026-10-09T16:00Z, `12H` 04:00/16:00Z, `6H` 04:00/10:00Z — `4H` and finer
happen to sit on the UTC grid), while `1Mutc` opened 2026-10-01T00:00Z — the chart's month.
A HK month matches no chart candle, so `1M` maps to the `*utc` bar (REST
`bar=1Mutc`, WS channel `candle1Mutc`, which reverse-maps to the
`candle.1M.{instId}` handler). `1D`/`1W`/`6H`/`12H` are deliberately left on Hong
Kong time: switching them to `1Dutc`/`1Wutc`/`6Hutc`/`12Hutc` moves every bar
existing OKX charts already show. The candle cache judges "closed" by each
candle's own close time, so those off-grid bars are not stored while forming.

## Order Lifecycle

### Order Placement

1. `placeOrders()` ensures position mode is loaded via `getOrFetchPositionMode()`
2. Orders are partitioned into algo and normal orders using `isAlgoOrder()`
3. Normal orders are formatted via `formatCreateOrder()` which may split into multiple lots
4. The `deriveAlgoOrdersFromNormalOrdersOpts()` base method extracts attached TP/SL from normal order opts and creates separate algo orders
5. All algo orders (explicit + derived) are formatted via `formatAlgoOrder()`
6. Normal orders are placed first via `placeOrderBatch()`, then algo orders via `placeAlgoOrderBatch()`

### Order ID Generation

- **Regular orders**: `clOrdId` = `{BROKER_ID}{uuid}` truncated to 32 chars, plus `tag: BROKER_ID`
- **Algo orders**: `algoClOrdId` = `{BROKER_ID}{uuid}` truncated to 32 chars, plus `tag: BROKER_ID`
- **Broker ID**: `e847386590ce4dBC` in `okx.types.ts`

### Algo Order ID Suffixes

Algo orders use composite IDs with suffixes:

- Stop loss: `{algoId}_sl`
- Take profit: `{algoId}_tp`
- Trailing stop loss: `{algoId}_tsl`

When cancelling algo orders, the suffix is stripped to get the base `algoId`: `o.id.replace(/_[a-zA-Z]+$/, '')`

### OCO/Conditional Order Mapping

A single OKX `oco` or `conditional` order can produce **two** internal orders:

- If `slTriggerPx` is set: creates a StopLoss order with `_sl` suffix
- If `tpTriggerPx` is set: creates a TakeProfit order with `_tp` suffix

An OKX `trigger` algo order maps to one internal `OrderType.Conditional` when
it is not reduce-only. Its internal `price` is the trigger price, and its
execution limit price is preserved as `order.extra.conditionalOrderPrice`
when OKX returns a positive `ordPx` / `orderPx`. Reduce-only `trigger` rows
created outside Tealstreet are treated as protective TP/SL based on side and
reference price rather than as opening conditionals.

### Order Update (Amend)

- **Limit orders**: Amended in-place via `ENDPOINTS.UPDATE_ORDER` with `newPx` and/or `newSz`
- **Opening trigger algos** (`OrderType.Conditional` / OKX `ordType: "trigger"`): Amended in-place via `ENDPOINTS.UPDATE_ALGO_ORDER` (`/api/v5/trade/amend-algos`) with `newTriggerPx`, `newOrdPx`, and `newTriggerPxType` on every amend; amount edits add `newSz`
- **Protective/trailing algo orders**: Still cancel-replaced by `updateAlgoOrder()` because they can map to multiple internal orders or unsupported amend fields:
  1. Finds all related orders sharing the same base algo ID
  2. Cancels them all
  3. Re-places with updated parameters
- **Amount validation**: If adjusted amount is 0, emits error `'Order amount is too small'` and returns empty

### Order Size Splitting (`okx.exchange.ts` around lines 1230–1250)

If an adjusted order amount exceeds `market.limits.amount.max`, it is
automatically split by `formatCreateOrder`:

- `maxSize` and amount precision must both be finite positive numbers; the
  adjusted amount must also be finite and positive before the fast path or
  splitting can create an outbound `sz`
- `maxSize` is the minimum of `maxIcebergSz`, `maxLmtSz`, `maxMktSz`, `maxStopSz`, `maxTriggerSz`, `maxTwapSz`
- The maximum lot is rounded down to the nearest amount-precision step, then
  emitted repeatedly with a final precision-aligned remainder
- Every lot uses OKX's native `sz` field, is at most `maxSize`, and all lots
  sum exactly to the already-adjusted requested amount
- Every payload receives its own broker-prefixed `clOrdId`; IDs are never
  reused within a split batch

### Batch Processing

- **Regular orders**: Placed in batches of up to 20 via `PLACE_ORDERS`, processed sequentially between batches (`mapSeries`)
- **Algo orders**: Placed sequentially one at a time via `PLACE_ALGO_ORDER`
- **Cancel normal**: Batched in groups of 20, processed sequentially
- **Cancel algo**: Batched in groups of 10, processed sequentially; suffix stripped from ID

### Algo Order Special Handling

- **SL/TP with zero amount**: Uses `closeFraction: '1'` instead of `sz` (close entire position)
- **SL/TP execution**: Always uses market price (`slOrdPx: '-1'` or `tpOrdPx: '-1'`)
- **SL/TP are always `reduceOnly: true`**
- **Trailing stop loss**: Uses `callbackSpread` for distance, `closeFraction: '1'`, empty `activePx` and `callbackRatio`

### Position Side Logic (`getPositionSide`)

- In **one-way mode** (`PositionMode.OneWay`): Returns `undefined` (omitted from payload)
- In **hedged mode**: Buy = `'long'`, Sell = `'short'`
- **Inverted for close orders**: SL, TP, trailing, and reduce-only orders flip the side (Buy becomes `'short'`, Sell becomes `'long'`)

## Symbol Format

- **OKX instrument ID (`instId`)**: `BTC-USDT-SWAP`, `ETH-USDT-SWAP`; OKX EU X-Perps use IDs like `ETH-USD_UM_XPERP-310404`
- **Internal symbol**: `BTCUSDT`, `ETHUSDT` (derived by stripping `-SWAP` and removing all `-` from `instId`)
- **X-Perp internal symbol**: Keeps the venue's `_UM_XPERP` identity and expiry while removing dashes (for example `ETHUSD_UM_XPERP310404`) so it does not collide with `ETHUSD` SWAP markets.
- **Market filter**: Linear and inverse SWAP contracts are included; OKX EU also includes `FUTURES` X-Perps. Spot is not requested.
- **Index ticker subscription**: Uses the market `uly` when available (e.g. `BTC-USDT`, `ETH-USD`), falling back to `instId` without `-SWAP`
- **Index ticker response mapping**: Updates every stored market whose index source matches the received `instId`; this allows `ETH-USD` to feed both `ETH-USD-SWAP` and `ETH-USD_UM_XPERP-...`

## Data Precision & Conversion

### Price and Amount Precision

- **Price precision** (`precision.price`): `tickSz` from instrument data (e.g., `0.01`)
- **Amount precision** (`precision.amount`): `lotSz` from instrument data (e.g., `0.001`)
- **Adjustment**: The `adjust()` utility (from `safe-math`) is used to round values to the correct precision step size

### Contract Size Handling

- **Contract size** (`contractSize`): `ctVal` from instrument data (e.g., `0.01` BTC per contract)
- **Order amounts**: Specified in **contracts** (native units), not base currency
- **Display conversion**: `contracts * contractSize` converts to base currency amount
- **Order fills**: Public `Order.filled` is base-sized for display/position math, while `Order.extra.filledInPlacementUnits` keeps cumulative `accFillSz` in native contracts for chaser replacement arithmetic.
- **Minimum amount**: `BigNumber(minSz).multipliedBy(ctVal)`
- **Position notional**: Linear contracts use `contractsNative * markPrice * contractSize`; inverse contracts use their quote-denominated contract value

### BigNumber Usage

- `BigNumber` (from `bignumber.js`) is used for minimum amount calculation to avoid floating point errors
- The `multiply`, `subtract`, `add`, `adjust` functions from `safe-math` are used throughout for safe arithmetic

### Balance Amounts

All balance/position monetary values use `roundUSD()` for display rounding.

### Candle Timestamps

OKX candle timestamps are in **milliseconds**. They are divided by 1000 for internal storage in seconds.

### OHLCV Data

- REST candle data: Array format `[timestamp, open, high, low, close, ?, ?, volume]` -- index 7 is volume
- WebSocket candle data: Same array format, index 7 is volume
- REST results are sorted ascending by timestamp after fetching
- `fetchOHLCV` requests `/market/candles` first. When the requested `to` lies
  beyond the recent 1,440-bar window, or the recent endpoint returns no rows,
  it retries through `/market/history-candles`. The history request respects
  OKX's 100-row per-request cap.

## Rate Limiting

### REST API

- **Retry policy**: 3 retries on network errors via `axios-retry` — reads only; a signature rejection (`50113`) is re-signed and resent once, writes included (`utils/signatureRejection.ts`)
- **Receive window (timeout)**: `RECV_WINDOW = 20_000` ms (20 seconds), overridable via `options.extra.recvWindow`

### Leverage Fetch Batching

- Leverage info supports comma-separated `instId` values, batched in groups of 20
- **250ms sleep** between batches to avoid rate limiting

### Order Batch Limits

- Regular orders: Up to 20 per batch
- Algo orders: One at a time, processed sequentially
- Cancel normal orders: Up to 20 per batch
- Cancel algo orders: Up to 10 per batch

## State Reconciliation

### WebSocket Order Updates

Orders received via WebSocket are processed individually:

1. Mapped through `mapOrders()` to convert to internal format
2. **`filled` or `canceled`**: Updates order status via `store.updateOrdersStatus()` (removes from active orders)
3. **`live` or `partially_filled`**: Adds or updates via `store.addOrUpdateOrders()`
4. **Fill events**: `filled` and `partially_filled` emit a `'fill'` event with
   fill price and base-asset amount converted by `contractsToBase`; `lotSz`
   (`market.precision.amount`) is not a contract-value conversion.

### WebSocket Position Updates

1. Positions are mapped through `mapPositions()`
2. Updated in store via `store.addOrUpdatePositions()`
3. Balance `used` (from `mmr` sum) and `upnl` are recalculated from the position data

### WebSocket Account Updates

1. `totalEq` from account data is summed for total collateral
2. Balance `free` and `total` are recalculated using current `used` and `upnl` from store

### REST Snapshot on Reconnection

When a private WebSocket reconnects:

1. `fetchAndSetPrivateSnapshot()` is called (from base class)
2. This re-fetches all orders from REST API to reconcile any missed WebSocket events
3. Called with up to 10 retry attempts

### Tick Loop Reconciliation

- Runs every 2 seconds (`tickInterval: 2_000`)
- Recalculates balance using stored balance info, current positions, and current orders
- Does NOT re-fetch from REST (unlike some exchanges); relies on WebSocket for data freshness

### Recursive Pagination for Order Fetching

Both normal and algo order fetches use recursive pagination:

- Page size: 100 results
- If exactly 100 results returned, fetches next page using `after` cursor (last order's `ordId` or `algoId`)
- Accumulates results across pages

## Exchange Metadata

Key metadata values from `exchangeMetadata.ts`:

| Property                          | Value                                                                 |
| --------------------------------- | --------------------------------------------------------------------- |
| `exchangeName`                    | `okx`                                                                 |
| `prettyName`                      | `OKX`                                                                 |
| `requiresPassphrase`              | `true`                                                                |
| `marginTypes`                     | `[MarginType.Cross]` (cross-only)                                     |
| `positionModes`                   | `[PositionMode.OneWay, PositionMode.Hedged]` (inherited from default) |
| `positionModeIsGlobal`            | `true`                                                                |
| `unifiedMargin`                   | `false`                                                               |
| `supportsTestnet`                 | `false`                                                               |
| `supportsChaser`                  | `true`                                                                |
| `supportsTrailingStopLoss`        | `false` (disabled at metadata level despite code support)             |
| `hasEmptyPositions`               | `false`                                                               |
| `orderbookTickDelay`              | `250` ms                                                              |
| `tickInterval`                    | `2_000` ms                                                            |
| `klineCallbackResetThreshold`     | `60`                                                                  |
| `tvMaxGetBarFetches`              | `5`                                                                   |
| `requiresTickersForMarketsLoaded` | `true`                                                                |
| `supportedTPSLTriggerTypes`       | `[LastPrice, MarkPrice]`                                              |
| `defaultTPTriggerType`            | `LastPrice`                                                           |
| `defaultSLTriggerType`            | `MarkPrice`                                                           |
| `privateProxyIps`                 | `privateProxyIpsTokyo`                                                |
| `requiresCors`                    | okx=false; okxeu=true                                                 |

`okxeu.requiresCors` is intentionally true because browser private REST to
`eea.okx.com` fails CORS preflight before OKX can process signed requests.

### Account Levels (`_accountLevel`)

Fetched from `/api/v5/account/config` (`acctLv` field):

| Level | Mode                  | Description                               |
| ----- | --------------------- | ----------------------------------------- |
| 1     | Spot mode             | Spot trading only                         |
| 2     | Futures mode          | Non-unified margin, per-currency balances |
| 3     | Multi-currency margin | Unified margin across currencies          |
| 4     | Portfolio margin      | Full portfolio margining                  |

The account level affects balance calculation. When `accLevel === 2` (Futures mode), per-currency balance breakdowns are provided in `balance.currencies`. Otherwise, currencies is an empty object.

### Position Modes (`_accountPositionMode`)

Fetched from `/api/v5/account/config` (`posMode` field):

| OKX Value         | Internal Value        | Description                       |
| ----------------- | --------------------- | --------------------------------- |
| `long_short_mode` | `PositionMode.Hedged` | Separate long and short positions |
| `net_mode`        | `PositionMode.OneWay` | Net position (single direction)   |

**Position mode is global** (`positionModeIsGlobal: true`). Switching modes requires **all positions to be closed first**.

### Margin Type

OKX in this implementation **only supports cross margin** (`marginTypes: [MarginType.Cross]`). Calling `changeMarginType()` emits the error: `"Only cross mode is supported"`. All order and leverage payloads hardcode `mgnMode: 'cross'` or `tdMode: 'cross'`.

### Leverage

- **Setting leverage**: POST to `/api/v5/account/set-leverage` with `instId`, `lever` (string), `mgnMode: 'cross'`
- **Fetching leverage**: GET to `/api/v5/account/leverage-info` with comma-separated `instId` values (batched in groups of 20 with 250ms sleep between batches)
- Leverage is clamped to `[market.limits.leverage.min, market.limits.leverage.max]`
- After setting leverage, positions are re-fetched from the API
- On error, positions are still re-fetched and error is re-thrown

### Balance Calculation (`formatBalanceUsedAvail`)

1. **Total collateral**: Sum of `eq * markPrice` across all balance currencies (mark price from tickers for non-stablecoin assets; stablecoins use multiplier of 1)
2. **Used (positions)**: Sum of `imr` (initial margin requirement) across positions
3. **Used (orders)**: Sum of `(remaining * price / leverage) * 1.1` for open limit, non-reduce-only orders (the 1.1 is a hand-wavy multiplier)
4. **Unrealized PnL**: Sum of `upl` across positions
5. **Free**: `totalCollateral - usedPositions - usedOrders`
6. **Total**: `totalCollateral - upnl`
7. **Per-currency breakdown** (Futures mode only, `accLevel === 2`): For each currency, tracks used margin and upnl separately

### Position Mapping (`mapPositions`)

- **Contracts**: `pos` field, absolute value
- **Side**: Determined from `posSide` (`long`/`short`), or from sign of `pos` in net mode (negative = short)
- **Entry price**: `avgPx` (zero when no contracts)
- **Notional**: `contractsNative * markPrice * contractSize`
- **Leverage**: From position `lever` field, falls back to existing stored position's leverage
- **Liquidation price**: `liqPx` field (defaults to 0 if missing)
- **Margin type**: From `mgnMode` field (`cross` or `isolated`)
- **Position mode**: Derived from `posSide` (`net` = OneWay, otherwise Hedged)
- **Position ID**: Created via `createPositionId(symbol, side, positionMode)`

## Testnet / Demo

- **`supportsTestnet`**: `false` in `okx` metadata. The generic OKX testnet
  checkbox remains hidden because demo credentials are separate from live
  credentials.
- **`okxdemo`**: separate first-class variant. It subclasses `OKXExchange`, sets
  `isDemo = true`, and forces `options.testnet = true` in the constructor.
- **`okxeudemo`**: separate first-class variant. It subclasses
  `OKXEUExchange`, sets `isDemo = true`, and forces `options.testnet = true` in
  the constructor.
- **REST behavior**: global demo REST uses `https://openapi.okx.com` plus
  `x-simulated-trading: 1`; EEA demo REST uses `https://eea.okx.com` plus the
  same header. Both route through the existing OKX/Tokyo Tealstreet REST proxy
  pool. This is REST proxying only, not HOW/WS tunnelling.
- **Proxy dependency**: global demo's `openapi.okx.com` host must be present in
  the private-proxy fleet before browser-proxied global demo requests can reach
  OKX. EEA demo uses the existing `eea.okx.com` proxy route.
- **WebSocket behavior**: global demo sockets connect directly to
  `wss://wspap.okx.com/ws/v5/{public|private|business}`; EEA demo sockets
  connect directly to
  `wss://wseeapap.okx.com/ws/v5/{public|private|business}`. The routing
  table intentionally does not give demo variants OKX private/business proxy
  overrides.
- **Order-state safeguards**: do not copy Bybit Demo's recently-placed-order
  shield into OKX Demo speculatively. As of implementation, no in-repo probe
  evidence shows OKX Demo dropping private `orders` / algo-order frames. If a
  live probe proves that it does, port the narrow Bybit pattern only:
  recently-placed unsighted order protection, demo-only local retirement after
  confirmed cancels, and exact-order fill reconciliation. Do not add generic
  post-mutation REST snapshots; those can race just-placed chaser orders.

## Known Quirks / Implementation Notes

- **Trailing stop loss**: Code exists but is disabled at the metadata level (`supportsTrailingStopLoss: false`). The `formatAlgoOrder` and `mapOrders` methods fully implement it.
- **Margin type**: Hardcoded to cross only; attempting to change emits an error.
- **Position mode change**: Requires all positions to be closed first; checked client-side before API call.
- **Account config caching**: Position mode and account level are cached in `_accountPositionMode` and `_accountLevel`; use `force = true` parameter to re-fetch. Both are fetched from a single endpoint call (`getOrFetchPositionModeAndAccLevel`).
- **Algo order updates**: Opening trigger algos amend in-place; protective/trailing algos are cancelled and re-created.
- **Order book filtering**: Bids above last price and asks below last price are filtered out after each update.
- **Recursive order fetching**: Both regular and algo order fetches use recursive pagination (when exactly 100 results are returned, fetch next page using `after` cursor).
- **Business WebSocket inheritance**: `OKXBusinessWebsocket` extends `OKXPrivateWebsocket` to reuse `auth()` and `handleOrderTopic()` methods. However, it overrides `connectAndSubscribe`, `onOpen`, `onMessage`, `ping`, `auth`, and `subscribe` -- meaning most of the parent's methods are replaced.
- **Contract sizes**: Order amounts are in contracts (native size), which are multiplied by `contractSize` (`ctVal`) for display. The `precision.amount` is `lotSz` and `precision.price` is `tickSz`.
- **Position mode in one-way mode**: `fetchPositionMetadata` creates two virtual positions (Long + Short) for each symbol in one-way mode so that both sides are represented in the store.
- **Balance info passthrough**: `formatBalanceUsedAvail` stores the raw `bal` array in `balance.info` so it can be reused in the tick loop without re-fetching.
- **Partner check**: The `PARTNER` endpoint (`/api/v5/users/partner/if-rebate`) is defined but not called in any code path within this directory.
- **Stablecoin mark price fallback**: When calculating balance equity notional, if no ticker is found for a currency (e.g., USDT), the mark price defaults to 1.
- **Trailing stop price calculation**: For trailing stop loss orders, the displayed price is calculated relative to the position entry price and last price, with special handling when a position exists.
- **`afterMethodCalledInWorker`**: When `changePositionMode` is called in a WebWorker, the local `_accountPositionMode` cache is updated to match.
- **WebSocket diagnostics**: Private and business WebSocket diagnostics go through `BaseWebSocket`'s runtime-gated `emitWsDiagnostic*` helpers. They populate the web Requests drawer only when WS diagnostics are enabled through the Requests Log toggle; Grafana upload is a separate opt-in.
- **Potential follow-up (not confirmed bug)**: Inverse close sizing in the web layer currently prefers `Math.max(extra.contractsNative, notional/contractSize)` in [`getPositionNativeSize.ts`](mdc:apps/web/src/utils/position/getPositionNativeSize.ts) to avoid partial closes. This can theoretically oversize if notional is briefly inflated or stale. Re-evaluate if oversize-close reports appear; consider freshness/tolerance-based bounds instead of raw max/min switching.

## Income History / Account Bills

`fetchIncomeHistory` / `fetchIncomeHistoryRange` read `/api/v5/account/bills` (7-day
retention) and `/api/v5/account/bills-archive` (3 months). The endpoint is chosen from
the requested window, not discovered by failure: a window older than an endpoint's
retention answers `code=0` with an **empty list**, never an error.

**One OKX bill carries BOTH `fee` and `pnl`.** This is the structural difference from
binance, which emits a separate row per category. Measured live over 100 `type=2` rows:
`sum balChg = -0.48706`, `sum fee = -0.42126`, `sum pnl = -0.06580`, residual
`0.0000000000`. Mapping a bill 1:1 to an `IncomeRecord` therefore merges realized PnL and
commission — two distinct terms of the reconciliation identity — into a single number.

The mapper is the pure, exported `mapOkxBillToIncomeRecords` in `okx.income.ts` (kept out
of the exchange class so it is unit-testable). One bill becomes up to three records —
`:pnl`, `:fee`, and a `:adj` residual — with the invariant that **the emitted incomes sum
to that bill's `balChg`**. The residual term is what keeps that true for bill types whose
balance change is not explained by fee + pnl (transfers, interest, conversions); without
it those rows would silently drop out of the ledger.

Three things not to "simplify":

- **Bill `type` codes come from the ccxt listing** vendored at `v2/ccxt/python/ccxt/okx.py`
  — `5`/`9` are liquidation and ADL, while `13`/`14` are DDH and block trade and must NOT
  be categorised as liquidation.
- **An unresolvable `instId` falls back to the raw id rather than dropping the row.**
  Dropping it would remove value from the ledger and break the sum-to-`balChg` invariant.
- **`incomeType` is the binance-style uppercase spelling, not OKX's raw type.** The raw
  type is a bare number (`"2"`) that no consumer can interpret, and the journal aggregator
  keys on the binance spellings.

Scoped to `instType: 'SWAP'` — the journal is position-based and perp-only. The
consequence, worth stating explicitly: the returned ledger reconciles against swap-account
flows, **not** against total account equity.

This scoping is deliberate and is **not** widened by `getInstrumentTypes()`. The journal,
ledger, order-history and closed-position axes stay `SWAP`-only even on OKX EU, so X-Perp
activity does not appear in them. Live trading paths (markets, tickers, positions, open
orders, per-market fills) do follow `getInstrumentTypes()`.

## Closed positions (`/api/v5/account/positions-history`)

`fetchClosedPnl` reads this endpoint. It is a **different source from `/account/bills`**,
which is the whole reason it can cross-check the ledger rather than restate it — see
`scripts/history-audit/README.md` → "Cross-axis oracle".

Three things verified live, each of which silently produces wrong data if assumed:

- **`begin`/`end` are IGNORED here.** Passing them returns the same unbounded 100 rows
  spanning nine months, so it looks like a working time filter until you check the oldest
  row. The time bounds are **`before`** (newer than) and **`after`** (older than), as
  **timestamps** — unlike `/bills`, where `after` is a billId. Page backwards by setting
  `after` to the oldest `uTime` seen, minus 1ms.
- **Do NOT dedupe on `posId`.** OKX reuses a posId across separate open→close cycles in
  net mode. A live 30-day DOGE window returned **87 rows across only 3 posIds**, and
  deduping on it destroyed 84 real closes and 96% of the realized PnL. Every row is a
  distinct close event; the row IS the event.
- **`realizedPnl` is net of BOTH fee and funding** — measured as
  `pnl(-0.0014) + fee(-0.0100359) + fundingFee(0) = realizedPnl(-0.0114359)`. Bybit's
  `closedPnl` excludes funding, so anything comparing the two conventions must know which
  a venue uses or it reports a residual exactly the size of the funding term and blames
  the mapper.

`fee` is negative when charged here (the opposite of bybit), and sizes are in CONTRACTS
so `closeTotalPos` needs `contractsToBase`. Mapper: `okx.closed-pnl.ts`, pure and exported.

Cross-checked 2026-08-12 against the ledger over 30 days on DOGEUSDT: 87 closes,
`closedPnl = -1.18589240` vs `ledger = -1.19091000`, residual `5.018e-3` — reported
BOUNDARY, i.e. within a few window-edge fees rather than a mapping defect.

## Order history (`/api/v5/trade/orders-history`)

`fetchHistoricalOrders` reads the **7-day** route. Mapper: `okx.order-history.ts`, pure and
exported. Measured live over 7 days: 255 orders, **58 of which never filled (22.7%)** —
invisible to fills, executions and closed positions alike.

**Do not "improve" this by paging into `/api/v5/trade/orders-history-archive`.** The
3-month archive returns **filled orders only**, so reaching further back silently drops
every cancellation — the one population this axis holds that the fills axis does not.
Verified live on row 13: `state=canceled` returned **0 rows** from the archive and **58**
from the 7-day route, and a matched 2-day window returned 100/0 filled/canceled from the
archive against 80/20 from this route. Depth on this axis costs the axis.

Two more details that produce silently wrong data if assumed:

- **`sz` and `accFillSz` are CONTRACTS.** A `DOGE-USDT-SWAP` row reading `sz: "0.14"` at
  `avgPx: 0.07168` carries `fee: -0.005` — 0.05% of $10, not of $0.01 — because one
  contract is 1000 DOGE. The mapper therefore _requires_ a `toBase` converter and returns
  `null` when the market is unknown, rather than emitting a raw contract count that would
  be off by the contract size with nothing downstream able to notice.
- **`ORDER_STATUS` in `okx.types.ts` is NOT reusable here.** It maps only `live` and
  `partially_filled`, because the live order feed never sees a terminal state. Running
  history rows through it sends `filled`, `canceled` and `mmp_canceled` to the caller's
  default and reports every cancellation as a completed order. `okx.order-history.ts`
  keeps its own table on purpose.

`after` on this endpoint is an **ordId cursor**, unlike `/positions-history` where the
same-named param is a timestamp. Mixing the two returns the newest page forever, which
reads as a short history rather than as a broken loop.

**Known gap:** conditional/TP-SL orders live in a separate `/orders-algo-history` archive
and are NOT included. A live probe found 100 canceled `conditional` rows there, so this is
real missing volume — but those rows carry `sz: ""` with `closeFraction: "1"` (position-
scoped, no size), which has no honest mapping onto `HistoricalOrder.amount`. Wiring it
needs a decision about representing position-scoped protective orders, not just a mapper.

## Error Handling

### General Pattern

Most methods follow the same error handling pattern:

1. Catch errors from axios responses
2. Extract message via `err?.response?.data?.msg || err?.message`
3. Call `this.emitAndLogError(message)` to emit error toast + log
4. Return the current store state as fallback (for fetches) or throw (for mutations)

### Specific Error Handling

| Context                             | Error Pattern                                       | Behavior                                                       |
| ----------------------------------- | --------------------------------------------------- | -------------------------------------------------------------- |
| `validateAccount`                   | Any error from `GET /account/account-position-risk` | Returns `{ error }`; success returns `{ error: '', balance? }` |
| `fetchMarkets`                      | Any fetch error                                     | Emits error, returns current `store.markets`                   |
| `fetchTickersRaw`                   | Any fetch error                                     | Emits error, returns empty array                               |
| `parseTickers`                      | Any parse error                                     | Emits error, returns current `store.tickers`                   |
| `fetchBalanceAndOpenPositions`      | Any fetch error                                     | Emits error, returns current store balance + positions         |
| `formatBalanceUsedAvail`            | Any calculation error                               | Emits error, returns current `store.balance`                   |
| `placeOrderBatch`                   | Batch response with `sMsg`                          | Throws error with the message                                  |
| `placeAlgoOrderBatch`               | Response without `algoId`                           | Throws error with `sMsg`                                       |
| `updateTriggerAlgoOrder`            | Amend row with non-zero `sCode` or `sMsg`           | Emits error and throws so the edit is not accepted             |
| `cancelNormalOrders`                | Response with `sMsg` per item                       | Emits error per failed cancellation (does not throw)           |
| `cancelAlgoOrders`                  | Response with `sMsg` per item                       | Emits error per failed cancellation (does not throw)           |
| `setLeverage`                       | Any error                                           | Emits error, re-fetches positions, re-throws                   |
| `changePositionMode`                | Any error                                           | Emits error, re-fetches positions, re-throws                   |
| `getOrFetchPositionModeAndAccLevel` | Any fetch error                                     | Emits error, re-throws                                         |
| WebSocket error event               | `"event":"error"` message                           | Emits error from `json.msg`                                    |

The REST response interceptor checks the first failed row under `data[]` before
falling back to the top-level `msg` or generic `code`; OKX batch endpoints often
return top-level `code: "1"` while the actionable message is in row `sMsg`.

### Retry Strategy

- **REST**: 3 automatic retries on network errors via `axios-retry`; `50113` is re-signed and resent once
- **WebSocket reconnection**: Up to 100 reconnect attempts with randomized delay (1-3 seconds)
- **`fetchAndSetPrivateSnapshot`**: Up to 10 retry attempts on WS reconnect

### No "Order Not Found" Success Pattern

Unlike some exchanges, OKX does **not** treat "order not found" on cancel as success. Cancel errors are emitted per-item via `sMsg` checks.

## HTTP-over-WebSocket

When `options.tealstreetTunnel` is enabled, the axios adapter is replaced with `bigIntAxiosHttpOverWebsocketAdapter` which routes HTTP requests over a WebSocket tunnel (supports BigInt serialization).

## Grafana Monitoring

Endpoints are split into two monitoring groups:

- **Every-request logging** (`GRAFANA_ENDPOINTS_EVERY`): Order placement, cancellation, amendment, leverage operations, position mode changes
- **Periodic logging** (`GRAFANA_ENDPOINTS_PERIODIC`): Balance, positions, unfilled orders, unfilled algo orders

## Testing / Validation

- `validateAccount()`: Tests API connectivity by fetching the balance endpoint
  and returns `{ error, balance? }`.
- `src/exchanges/okx/` is production runtime only. Do not add tests or fixtures here.
- Fresh deterministic regressions belong in `src/__tests__/audit-regressions/`.
- Use `yarn test-ci` from the `safe-cex` package root for the full deterministic suite.

## The SAME trade carries gross PnL on fills and net PnL on closed positions

Measured live 2026-08-12 on row 13, against an e2e run whose actions were known
independently (a deliberate position flip):

| Axis                         | Field         | Value                  |
| ---------------------------- | ------------- | ---------------------- |
| `/trade/fills`               | `fillPnl`     | `-0.0014` — **gross**  |
| `/account/positions-history` | `realizedPnl` | `-0.0112889` — **net** |
| both                         | fee           | `0.0098889`            |

`-0.0014 - 0.0098889 = -0.0112889` exactly. Two axes on ONE venue, both naming the
quantity "realized PnL", differing by the fee. Anything that picks an axis per venue, or
sums across axes, silently changes the number by the commission — and every reconciliation
total still balances, because the fee moves with it.

This is why `Execution.realizedPnl` is documented gross and why a canonical position model
has to normalize convention at the adapter rather than trusting the field name.

## A flip through zero IS split by positions-history

The same run sold 280 against a 140 long in one fill. `/positions-history` returned:

```
closed=15:04:24 opened=15:04:12 long  qty=140 entry=0.07064 exit=0.07063
closed=15:04:32 opened=15:04:24 short qty=140 entry=0.07063 exit=0.07064
```

The long closes at the flip instant and the short opens at that same instant, each with
the CLOSED size (140), not the fill size (280). So on OKX the split does not have to be
inferred — the venue reports it, and `opened` is a real open time. Prefer this axis over
folding fills whenever it is available; the fold is only needed where it is not.

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `clOrdId`, rule `^[A-Za-z0-9]{1,32}$` (OKX: case-sensitive alphanumeric). Replaces the generated `BROKER_ID…` ID; no prefix is composed. Inherited by OKX EU and both demo variants.
- The native amend keeps the order and its `clOrdId`; no new client ID is sent.
- With secondary GET credentials, cached client-ID hits cannot prove primary
  identity and are bypassed in both TS and Rust. The primary response determines
  success, absence or failure without changing the cached secondary snapshot.
- Definitive client-ID lookup always uses the primary REST client that placed
  the order. A secondary read key can belong to another account; its not-found
  response cannot establish absence from the primary account. Rust also tags
  this request with the explicit primary routing hint.

## Rust replay depth correction

Price updates cache native market IDs to current ticker rows. Rebuild this
derived map after either authoritative market or ticker table replacement;
preserve Store's last duplicate-symbol row and known markets without tickers.
Mark/index readiness still records those known markets for private trigger
fallbacks. Repeated updates reuse the readiness ID instead of allocating it.

Books retain every venue level. Never filter bids/asks by the last trade price:
trades can lag book updates, so that filter permanently deletes valid liquidity.
The Rust replay oracle includes this correction explicitly in `KNOWN_TS_FIXES`.

## Rust engine scope and verified corrections

`engine: 'rust'` supports linear and inverse SWAPs under all four OKX names;
EEA profiles additionally retain `FUTURES` with `ruleType: xperp`, matching TS.
Signed EEA discovery uses account-scoped instruments for both product types.
Catalogs, tickers, positions, and regular/algo orders stage complete product
unions before committing. X-Perp dated IDs stay distinct from matching SWAPs.
Sizes sent to OKX remain contracts; fills and remaining quantities use ctVal,
while the existing book contract uses lotSz. Inverse base quantities divide
quote contract value by the native reference price; position notional remains
quote value. Position PnL and margin are USD-normalized, while raw coin collateral
and ledger units remain distinct. Exact native filled/leaves and position counts
survive both frame consumers. EEA income walks both product types before global
limiting; stalled/capped composite sources fail explicitly. Generic account-wide
fills retain the inherited empty-result contract.

The replay oracle also implements public trades (previously inherited an
unimplemented method), and OHLCV request bounds use UTC calendar arithmetic
so a local timezone cannot move the monthly boundary. Rust additionally
isolates bracket entries with their protective legs, amends OCO legs natively,
refuses negative protective sizes and unsafe market/terminal amends, and
continues standalone cancel batches after item failures. These corrections are
listed in the harness KNOWN_TS_FIXES; they are independently tested instead
of normalizing away output differences. Newer account equity survives REST
snapshots and subsequent local balance ticks in both Rust and TS. Account WS
`totalEq` is a separate aggregate anchor, never a synthetic USD currency row:
USD is real settlement collateral for X-Perps. Details are currency deltas;
validate the whole push before merging, retain currencies absent from it, and
include live-order initial margin in free balance. Revision fences stop an
older REST join, startup callback, or asynchronous local tick from restoring
stale equity. A newer successful REST read can replace the WS anchor.

Public-only business sockets subscribe candles directly without sending an
empty-credential login or private order topics. Private accounts authenticate
before subscribing account-order channels. This also corrects the TS replay
oracle's previous unconditional keyless login.

Rust oracle audit: historical fill windows compare native millisecond timestamps
before truncating public execution timestamps to seconds. Positive protective
dust must not become close-all; ambiguous standalone protective limit amends
fail before cancellation in Rust, preserving the existing execution price.

EEA account reconciliation reads explicit SWAP and FUTURES scopes and commits
positions/orders only after both products finish successfully. Failed, malformed
or stale later-product reads retain the proven whole-account snapshot. Public
index prices use a catalog-owned fanout table for shared inverse/X-Perp indexes.
Untraded X-Perps can appear live with empty REST `last` and `open24h` fields;
unavailable REST seeds start at finite zero. An explicitly empty live last/open
quote retains the last proven trade/change. Blank bid/ask/volume/last seed
values are zero; malformed nonempty ones also start at zero with one counted
warning, so the market keeps a ticker row that live updates can patch (they
never create one). A ticker row that does not parse at all, or names an unknown
instrument, is skipped with "Skipped malformed public discovery row".

Rust startup is fail-open per row and retried per read through the shared
engine `startup::Backoff` (1, 2, 4 … 30 s, separately for the catalogue and the
ticker seed). A malformed instrument row is skipped with a Debug notice and its
instId remembered, so a position or working order on it logs an error and keeps
the previous account snapshot instead of disappearing. A failed or malformed
read restarts its whole product chain without publishing a partial union; when
a product scope yields no usable rows, valid rows from the other product merge
into the existing catalogue and that read is retried. Private startup begins
once the catalogue loads, independent of the ticker seed. `fetchLeverage` reads
cross `leverage-info` for one symbol (flat symbols have no position row to
carry it). Account config is guarded by an in-flight flag. Until the first
config read succeeds, any failure (429, 5xx, transport, non-JSON, malformed or
unknown-code envelope) retries through `startup::Backoff` (1 s doubling to 30 s,
forever); a 418/429 waits at least its `Retry-After`, else 10 s (the discovery
default), before the retry. Only documented key refusals end startup with one `StartupFailed`:
REST 50101/50103/50104/50105/50111/50114/50119 (`AuthInvalid`), 50100/50110/50120
(`AuthPermission`), and 50113 after one re-signed resend; timestamp codes
50102/50112 stay transient. Before the account is ready, a private/business
socket login refused with 60001/60003/60005/60024/60032 fails startup too; other
login errors (60009 etc.) reconnect with backoff. A rejection stops config
retries and closes and retires the private and business sockets; a refused
trading key also emits `Deactivate` (a read-only key does not). Invalid read-only
credentials fail startup with `InvalidInput` before any request. Once configured
config is re-read every 30 s (a failed poll waits for the next interval), and a posMode
or acctLv changed in the OKX app updates the hedged flag and reconciles
positions instead of rejecting orders.

Rust account startup preserves native currency balances when collateral FX is
unavailable; it retains the previous USD aggregate or waits to publish the first
one. Stable currencies use one; other currencies need a finite positive public
quote or the native position mark. Entry prices and discounted `disEq`/`adjEq`
are not raw equity conversion rates. Native `totalEq` remains authoritative.
The first real public price supersedes a startup native mark and converts inverse
exposure, native IMR and native PnL consistently. Missing-market currencies remain
visible and require native aggregate equity rather than a one-dollar fallback.

Rust closed-position caps and ordinary history caps/cursor stalls emit
`history.incomplete` with their actual query command before partial completion.
The host flags only the owning result and routes its warning to that caller.
EEA composite-source cap/cursor failures still reject; strict financial rows,
caps, cursors and retained-row policies are unchanged.


## Rust signer migration checkpoint

The draft Rust adoption stages native private HTTP and private/business login
through the engine SignStep/provider. Venue options contain opaque identity refs;
trusted setup owns key, secret and passphrase material, including absent-only
secondary passphrase fallback. Native GET reads prefer secondary; client-ID lookup,
ordered writes and both WS login channels stay primary. Public requests remain
unsigned. Setup validation may report sanitized `Invalid local signer setup`
before startup rather than the TS client error text.

The draft integrates the landed shared encoding/passphrase presentation core.
Hosted HTTP reuses `okx.signing.ts`, a standalone request serializer also used by
the TS API interceptor; no TS venue is constructed. Four-alias browser and
keyless RemoteSigner captures use owned primary refs and Base64 output.
Native private/business WS work has three typed byte parts (seconds, GET,
/users/self/verify), whose concatenation and MAC are unchanged. The bridge
decodes them only against the declared `secondsTimestamp`/`secondsSuffix` wire
codec, which Bitget shares with its own `/user/verify` suffix. That shape lets
the shared bridge preserve numeric legacy `extraData.timestamp` from the exact
prepared receipt without confusing an HTTP preimage with WS work. Native login
JSON keeps its existing string timestamp. Private PayloadSigner remains typed
unsupported/notSent; RemoteSigner wins and cannot fall back. Hosted admission
still requires positive actual-WASM tests and final CI/replay proof before merge.
No caller may restore raw secrets to sanitized options or use a local fallback.
