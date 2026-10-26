# ApeX Omni Exchange Integration

This directory contains the ApeX Omni (ApeX Pro) exchange integration for Tealstreet, a DEX perpetual futures platform built on zkLink Layer 2.

## Exchange Information

ApeX Omni is a decentralized perpetual futures exchange using zkLink's Layer 2 infrastructure. It supports USDT and USDC settlement currencies. Every order must be cryptographically signed with a zkLink L2 key (`omniKeySeed`) in addition to standard REST API authentication. The exchange supports hedged position mode (separate long/short positions per symbol) with cross margin only.

## API Documentation

- **Official API Documentation**: https://api-docs.pro.apex.exchange/
- **CCXT REST Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/apex.ts
- **CCXT WebSocket Reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/apex.ts

## File Structure

### Core Files

| File                     | Description                                                                          |
| ------------------------ | ------------------------------------------------------------------------------------ |
| `apexomni.exchange.ts`   | Main exchange class (`ApexOmniExchange`) extending `BaseExchange`. ~1420 lines.      |
| `apexomni.api.ts`        | REST API client factory with HMAC-SHA256 signing and response error interceptor.     |
| `apexomni.types.ts`      | Constants: base URLs, endpoints, order/status/TIF mappings, cancel reason enum.      |
| `apexomni.utils.ts`      | Utility functions: `getPrecision()`, `generateApeXRandomClientId()`, `dropPrefix()`. |
| `apexomni.ws-private.ts` | Private WebSocket: account data (orders, positions, fills, wallets).                 |
| `apexomni.ws-public.ts`  | Public WebSocket: market data (tickers, orderbook, candles, recent trades).          |

`fetchOrders()` is a pure REST mapper. Startup/tick reconciliation and the
private WebSocket's throttled order heal commit its returned snapshot with
`addOrUpdateOrdersFull()` only after checking disposal, so a late response cannot
mutate a torn-down exchange.

### Interface Directory (`interface/`)

| File                            | Description                                                                                                                                                                               |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `apexomni.index.interface.ts`   | `ApexMarket` enum, `KeyPair`, `ApiKeyCredentials`, `OrderWithClientId` types. Barrel exports.                                                                                             |
| `apexomni.private.interface.ts` | `UserObject`, `AccountObject`, `AccountBalanceObject`, `OrderObject`, `OpenPositionObject`, `CreateOrderOptionsObject`, `FundingRateObject`, `HistoricalPNLObject`, `OrderResponseObject` |
| `apexomni.public.interface.ts`  | `TickerObject`, `DepthObject`, `PerpetualContractObject`, `SymbolObject`, `GlobalObject`, `KlineObject`, `TradeObject`, `HistoryFundingObject`                                            |

### zkLink Signer (`zklink/signer/`)

| File                           | Description                                                               |
| ------------------------------ | ------------------------------------------------------------------------- |
| `types.ts`                     | `TxSignature` and `TxEthSignature` interfaces                             |
| `utils.ts`                     | `initSdk()` (one-time WASM init), assertion helpers for validating params |
| `transactions/contract.ts`     | `signContract()` -- signs perpetual contract orders (**primary method**)  |
| `transactions/order.ts`        | `signOrder()` -- signs spot orders (not used by exchange class)           |
| `transactions/transfer.ts`     | `signTransfer()` -- signs L2 transfer transactions                        |
| `transactions/withdraw.ts`     | `signWithdraw()` -- signs withdrawal transactions                         |
| `transactions/changePubKey.ts` | `signChangePubKey()` -- signs public key change transactions              |

## Authentication

ApeX Omni uses a **triple authentication** system:

### 1. REST API Authentication (HMAC-SHA256)

Every REST request is signed via an axios request interceptor (`apexomni.api.ts:46-101`):

- **Headers**: `APEX-SIGNATURE`, `APEX-API-KEY`, `APEX-PASSPHRASE`, `APEX-TIMESTAMP`
- **Primary signature construction**: Concatenates `timestamp + HTTP_METHOD + /api<endpoint>[?<sorted query>] + <sorted body for writes>`. The primary serializer preserves meaningful falsy values such as `0` and `false`.
- **Legacy compatibility retry**: A response code `20016`/`401`, or a response message mentioning `sign`/`signature`, is retried once with the older serializer/signature shape. That legacy serializer drops falsy values; it is never forced for a particular endpoint.
- **Key handling**: `Buffer.from(options.secret).toString('base64')` is the HMAC key
- **Hash**: `@noble/hashes` HMAC-SHA256, output as base64 string (`hmacSha256Base64` in `apexomni.utils.ts`)
- **Parameter sorting**: Both GET params and POST data are serialized with `qs.stringify()` and alphabetical sorting; only `undefined`/`null` are removed on the primary path.
- **Credentials required**: `key` (API key), `secret` (API secret), `passphrase`
- **Exception**: Kline endpoint (`/v3/klines`) is sent with empty headers (no auth) (`apexomni.api.ts:54-56`)

### 2. WebSocket Authentication (HMAC-SHA256)

Private WebSocket authentication (`apexomni.ws-private.ts`):

- **Message to sign**: `[timestamp, 'GET', '/ws/accounts']` joined as string
- **Key handling**: `Buffer.from(secret).toString('base64')` signed with `create-hmac` SHA256, output as base64
- **Payload sent**:
  ```json
  { "op": "login", "args": [JSON.stringify({ type: "login", httpMethod: "GET", requestPath: "/ws/accounts", apiKey, passphrase, timestamp, signature })] }
  ```

### 3. Order Signing (zkLink L2)

Orders require cryptographic signing via the zkLink Layer 2 SDK (`apexomni.exchange.ts:714-716`):

- **SDK**: `zklink-web-sdk-alpha` (WASM-based)
- **Initialization**: `initSdk()` calls `init()` once (lazy singleton, `zklink/signer/utils.ts:8-13`)
- **Signer creation**: `sdk.newRpcSignerWithProvider({})` then `signer.initZklinkSigner(omniKeySeed)`
- **Credential**: `omniKeySeed` from `ExchangeOptions` (validated in `validateAccount()` at `apexomni.exchange.ts:183-188`)
- **Contract signing**: `signContract()` (`zklink/signer/transactions/contract.ts`) is used for all perpetual orders
  - Uses `sdk.ContractBuilder` to build, `sdk.newContract()` to create, `signer.createSignedContract()` to sign
  - Returns `{ tx: { ...signedData, signature: { pubKey, signature } } }`
  - The exchange extracts only `signature.tx.signature.signature` (`apexomni.exchange.ts:987-989`)

### SignContractPayload Fields

| Field          | Derivation                                                                    |
| -------------- | ----------------------------------------------------------------------------- |
| `accountId`    | `BigNumber(zkLinkConfig.id).mod(UINT32_MAX).toNumber()`                       |
| `subAccountId` | Always `0`                                                                    |
| `slotId`       | `BigNumber(sha256(clientOrderId)).mod(UINT64_MAX).div(UINT32_MAX).toNumber()` |
| `nonce`        | `BigNumber(sha256(clientOrderId)).mod(UINT32_MAX).toNumber()`                 |
| `pairId`       | `l2PairId` from contract config (looked up by market symbol)                  |
| `size`         | `BigNumber(size).multipliedBy(1e18).toFixed(0, ROUND_DOWN)` (fixed-point)     |
| `price`        | `BigNumber(price).multipliedBy(1e18).toFixed(0, ROUND_DOWN)` (fixed-point)    |
| `direction`    | `true` for buy/long, `false` for sell/short                                   |
| `feeRates`     | `[ceil(makerFeeRate * 1e4), ceil(takerFeeRate * 1e4)]` (integers)             |
| `hasSubsidy`   | Always `false`                                                                |

## Startup Sequence

The `start()` method (`apexomni.exchange.ts:193-265`) proceeds as follows:

1. **Check public-data-only mode**: If no key/secret or `publicDataOnly` is set, skip private data.
2. **Sequential**: `fetchMarkets()` -- fetches symbols/contract config. Creates empty ticker stubs for each market. Updates store with `{ markets, tickers, loaded: { markets: true, tickers: true } }`.
3. **Check disposal**: Return early if disposed.
4. **Sequential**: `publicWebsocket.connectAndSubscribe()` -- connects public WS and subscribes to `instrumentInfo.all`.
5. **If public-data-only**: Mark balance/positions/orders as loaded, return.
6. **Sequential**: `privateWebsocket.connectAndSubscribe()` -- connects private WS and authenticates.
7. **Parallel** (`Promise.all`):
   - `fetchPositions()` -- fetches account data, maps positions, creates fake positions for all markets, updates store with positions. Also stores `zkLinkConfig` (l2Key, ethereumAddress, spotAccount, etc.).
   - `fetchRawBalance()` -- fetches account balance.
8. **Sequential**: Update store with balance.
9. **Sequential**: `tick()` -- starts the polling loop.

## REST API Endpoints

All endpoints are prefixed with the base URL (`https://omni.apex.exchange/api`):

| Constant         | Path                          | Method | Description                                                                        |
| ---------------- | ----------------------------- | ------ | ---------------------------------------------------------------------------------- |
| `USER`           | `/v3/user`                    | GET    | Validate API credentials, fetch user info                                          |
| `SYMBOLS`        | `/v3/symbols`                 | GET    | Fetch all trading pairs and contract configs (includes `contractConfig`)           |
| `KLINE`          | `/v3/klines`                  | GET    | Fetch OHLCV data (**no auth headers sent**)                                        |
| `ACCOUNT`        | `/v3/account`                 | GET    | Fetch account data: positions, l2key, ethereumAddress, wallets                     |
| `BALANCE`        | `/v3/account-balance`         | GET    | Fetch `totalEquityValue`, `availableBalance`, `initialMargin`, `maintenanceMargin` |
| `OPEN_ORDERS`    | `/v3/open-orders`             | GET    | Fetch all open/untriggered orders                                                  |
| `FILLS`          | `/v3/fills`                   | GET    | User fills history (primary for `fetchHistoricalFills`)                            |
| `HISTORY_ORDERS` | `/v3/history-orders`          | GET    | Filled-history fallback (`fetchHistoricalFills`)                                   |
| `CREATE_ORDER`   | `/v3/order`                   | POST   | Place a new order (form-urlencoded body with zkLink signature)                     |
| `CANCEL_ORDER`   | `/v3/delete-order`            | POST   | Cancel a single order by ID (form-urlencoded)                                      |
| `CANCEL_ORDERS`  | `/v3/delete-open-orders`      | POST   | Cancel all open orders for a symbol (form-urlencoded)                              |
| `SET_LEVERAGE`   | `/v3/set-initial-margin-rate` | POST   | Set leverage via initial margin rate (`multipart/form-data`)                       |

The current official Python connector also contains a `create_batch_orders_v3()` method that posts
an `orders` array to `/v3/batch-orders`. The current official API reference and connector README do
not document that endpoint's response schema, maximum order count, processing order, partial-failure
behavior, rollback behavior, or atomicity. It is therefore intentionally absent from `ENDPOINTS` and
is not used as a split-order safety mechanism.

### Request Content Types

- **Order creation** (`CREATE_ORDER`): `application/x-www-form-urlencoded; charset=UTF-8` (params stringified via `qs`)
- **Order cancellation** (`CANCEL_ORDER`, `CANCEL_ORDERS`): `application/x-www-form-urlencoded; charset=UTF-8`
- **Set leverage** (`SET_LEVERAGE`): `multipart/form-data` (also set via interceptor at `apexomni.api.ts:96-98`)
- **Default**: `application/json, charset=utf-8`

### Response Error Interceptor

The API client has a response interceptor (`apexomni.api.ts:107-115`) that checks for non-zero `code` field in response data and throws with format `[code] msg`.

Execution-history behavior (`fetchHistoricalFills`):

- Calls `/v3/fills` first
- Uses the rate-limited API client for execution-history polling (to reduce HOW/tunnel timeout bursts from concurrent unlimited requests)
- Tries symbol variants (`market.id` and `market.symbol`) to handle response/query format drift
- Uses v3-first params (`symbol`, `page`, `limit`, optional time window) and omits `token` by default
- Falls back to `/v3/history-orders` rows where `cumSuccessFillSize > 0`
- Uses the normal primary signing path; any signature rejection follows the API client's one-shot legacy retry shared by all endpoints
- Accepts both nested payload roots (`data`, `result`) and proxy-wrapped `body` roots when extracting rows
- Normalizes to `Execution[]` with Unix-second timestamps, symbol guards, side/price/qty field fallbacks, and fee/orderId mapping when present
- Supports both verbose row fields (for example `symbol/side/price/size/createdAt`) and compact fill rows (`s/S/p/v/T/i`)
- Keeps a bounded private-WS fills cache and returns cached executions when REST execution-history requests timeout
- Execution range filtering and sorting retain raw millisecond timestamps (including fractional ISO timestamps); only returned timestamps use whole Unix seconds. Both range bounds are inclusive, so the venue's `endTimeExclusive` is sent as one millisecond after the requested upper bound.
- Execution IDs are internal deduplication keys; equal fills in the same second remain distinct when their native IDs differ. The returned `Execution` shape is unchanged.
- REST execution identity prefers `tradeId`, `tradeID`, `fillId`, `execId`, `id`, then compact `i`; history-order fallback rows use the same native-row identity. Private socket identity prefers documented fill `id`, then `matchFillId`. Synthetic order cache entries never reserve genuine execution replay identities.

## WebSocket Implementation

### Public WebSocket (`apexomni.ws-public.ts`)

**URL**: `wss://quote.omni.apex.exchange/realtime_public?v=2&timestamp=...&requestId=<nanoid>`

**Connection**: `connectAndSubscribe()` creates a WebSocket, attaches open/message/close listeners (`apexomni.ws-public.ts:40-49`).

**Default subscriptions** (always active):

- `instrumentInfo.all` -- all ticker updates

**On-demand subscriptions** (registered via `listenOHLCV` / `listenOrderBook`):

| Channel Pattern              | Description                                                      |
| ---------------------------- | ---------------------------------------------------------------- |
| `candle.<interval>.<symbol>` | OHLCV candle updates for a specific symbol and interval          |
| `recentlyTrade.H.<symbol>`   | Recent trade data, secondary source for real-time candle updates |
| `orderBook200.H.<symbol>`    | Order book with 200 depth levels, snapshot + delta updates       |

**Message routing** (`apexomni.ws-public.ts:77-96`): Messages are matched by checking `data.includes('topic":"<topic>')`. For each matching topic, handlers are called with parsed JSON.

**Ping/Pong**:

- Client sends: `{ op: "ping" }`
- Server sends: `{ op: "ping" }` -- client responds with `{ op: "pong", args: [timestamp] }`
- Pong received: `handlePongEvent()` tracks latency

**Reconnection**: When `ENABLE_RECONNECTING_WEBSOCKET` is true, `emitWsReconnected()` is called on open (`apexomni.ws-public.ts:57`).

**Subscribe batching**: Topics are chunked into batches of 50 and subscribed separately (`apexomni.ws-public.ts:70-75`).

**OHLCV Candle Logic** (`apexomni.ws-public.ts:173-293`):

- Primary source: `candle.<interval>.<symbol>` provides candle snapshots.
- Secondary source: `recentlyTrade.H.<symbol>` updates the current candle with live trades (updates high/low/close/volume). Only subscribed if `!publicDataOnly`.
- Backward-bleeding protection: Candles with timestamps older than the last known candle are ignored.
- New candle detection: If trade timestamp is in a newer candle period, a new candle is created from the previous close.
- **waitForConnectedAndSubscribe**: Polls every 100ms until WebSocket is connected before subscribing.

**OrderBook Logic** (`apexomni.ws-public.ts:295-380`):

- `snapshot` messages: Completely replace the order book from `data.a` (asks) and `data.b` (bids).
- `delta` messages: Update individual price levels using `updateOrderBook()`.
- Validate every level before mutation; snapshot insertion sorts levels and drops zero sizes.
- Snapshots reset the `data.u` cursor. Ignore pre-snapshot and duplicate/older deltas; require consecutive updates. A sequence gap clears the visible book and unsubscribes/subscribes while awaiting a fresh snapshot.
- Reconnect clears each listener's sequence and book before resubscribing.

**Ticker/InstrumentInfo Handling** (`apexomni.ws-public.ts:98-171`):

- Updates ticker fields: `last` (p), `mark` (mp), `index` (xp), `percentage` (pr \* 100), `openInterest` (o), `fundingRate` (fr), `volume` (v), `quoteVolume` (to).
- Fallback: If mark price is absent but index is present, mark = index.
- **Real-time PnL**: Calculates unrealized PnL from mark/last price for matching positions (long/short direction aware). Unknown/inverse markets skip only that ticker's PnL work and never abort the rest of the batch.

### Private WebSocket (`apexomni.ws-private.ts`)

**URL**:

- Direct: `wss://quote.omni.apex.exchange/realtime_private?v=2&timestamp=<current>&requestId=<nanoid>` (or testnet equivalent)
- Proxy (when `tealstreetProxy` is enabled): `wss://fsjnpy143pbz.asia.tealstreet.io/apex/private`

**Connection flow**:

1. Create WebSocket using direct Apex endpoint by default; use Tealstreet proxy only when `tealstreetProxy` is enabled. Note: `requiresCors: true` + `enforceWsTunnel: true` in `exchangeMetadata` forces `tealstreetProxy: true` for every Apex account via `createExchangeFromAccount.ts`, so production always hits the proxy URL.
2. On open: `await auth()` -> record `lastConnectTime` -> `subscribe()` -> `ping()`.
3. If `ENABLE_RECONNECTING_WEBSOCKET`, call `fetchAndSetPrivateSnapshot()` on reconnect.

**Subscribed channels**: `['ws_zk_accounts_v3', 'ws_user_v1', 'ws_notify_v1']`

> **Caveat**: `ws_user_v1` and `ws_notify_v1` have no message handlers in our code, but they are kept in the subscribe args because dropping them empirically correlated with the upstream proxy closing the socket with code `1005` ("no status code received") shortly after each connect. Subscribing to channels we don't process is cheap and matches the production-tested legacy behavior. The auth payload's `topics` field was likewise removed for the same reason — our PR added it as a hint but the proxy did not tolerate it.

> **`onOpen` is `async` and AWAITS `auth()` — this is not the ack-gating that
> `d1184f3d8e` reverted, and it must not be "restored" to the un-awaited form.**
> That revert undid three bundled things: the `topics` field, subscribe gated on
> the server's login ACK behind a 5s watchdog, and the trimmed channel list.
> Awaiting waits on OUR signer, never on Apex. The local HMAC branch of `auth`
> reaches `sendWithLog` with no `await` in front of it, so on the direct path the
> login still precedes the subscribe in the same macrotask, one microtask later —
> whereas ack-gating waited on the wire for up to 5s. What it fixes is HOSTED
> mode (exchange-server), where the signature is a round trip to the paired
> browser: un-awaited, `subscribe()` reached the wire before the login and Apex
> never established the private channels, so orders and fills stopped updating
> while REST polling kept the UI looking healthy. Same defect class as bybit
> inverse/spot — see `../CLAUDE.md` → "Remote-signer WS auth".
>
> **On a failed login the socket sends NOTHING and reconnects.** `remoteSigner.sign`
> throws rather than resolving `{ error }`, and subscribing anyway would be worse
> than silence: `base.ws.ts trackLastReceived` sets `isInitialized` on the first
> inbound frame, so any frame makes a socket holding zero channels report healthy
> indefinitely. The failure message must also never contain the words "auth
> failed" — `health/errorPatterns.ts` matches that literal as `auth_invalid` and
> suppresses automatic account reloads for five minutes.
>
> Pinned by `../../__tests__/audit-regressions/apexomni.private-ws-auth-ordering.test.ts`,
> which also fails if the three-channel list is trimmed or the `topics` field
> comes back — so the 1005 fix is now gated rather than only documented.
> **Unverified against a live Apex account** (no funded key on the sheet); a
> hosted session capturing the login/subscribe exchange is what would confirm it.

**Primary channel: `ws_zk_accounts_v3`** (`type: 'delta' | 'snapshot'`):

| Sub-field                               | Description                                     |
| --------------------------------------- | ----------------------------------------------- |
| `contents.orders[]`                     | Order updates: status, fill info, TP/SL params  |
| `contents.positions[]`                  | Position updates: size, entry price, side       |
| `contents.fills[]`                      | Trade fill details: price, size, fee, direction |
| `contents.wallets[]`                    | Wallet balance per token (USDC/USDT)            |
| `contents.positionClosedTransactions[]` | Position close details                          |
| `contents.deleverages[]`                | Deleverage events                               |
| `contents.transfers[]`                  | Transfer events                                 |

**Ping/Pong** (`apexomni.ws-private.ts`):

- Client sends heartbeat: `{ op: "ping" }` every 5s. Keep this cadence: 15s correlated with code-1005 closes through the roughly 30s tunnel idle window.
- Server ping compatibility: `{ op: "ping" }` or `{ ping: <ts> }` -- client responds with `{ op: "pong", args: [...] }`
- Pong compatibility: handles `{ op: "pong" }`, `{ ret_msg: "pong" }`, or `{ pong: <ts> }` and updates latency via `handlePongEvent(5_000)`

**Throttled Refetch Mechanism**:

- `throttleFetchOrders()` (`apexomni.ws-private.ts:133-147`): Debounced 1-second delay before refetching orders via REST. Cleared and reset on each call.
- `throttleFetchPositions()` (`apexomni.ws-private.ts:149-164`): Same pattern for positions.

**Order Topic Handling** (`apexomni.ws-private.ts:240-318`):

- Only triggers `throttleFetchOrders()` if no orders in the batch are `CANCELED` (cancellations are handled by blacklist).
- Skips recently cancelled orders via `recentlyCancelledOrders` map.
- Skips FILLED orders with `price <= 0 && amount <= 0` (false positives from order replacements).
- Only genuine `contents.fills[]` executions emit `'fill'`, once per native execution ID and with the full execution quantity. Snapshot fills seed the cache and replay guard without notifications. Order cumulative totals can repair the history cache, never emit execution notifications; later genuine executions replace the corresponding synthetic quantity.
- `CANCELED` orders: Removes from store including associated `__stop_loss` and `__take_profit` virtual orders. Emits error for non-trivial cancel reasons.
- Final filled/canceled/expired rows mark parent and virtual children terminal so delayed REST snapshots cannot reopen them. Terminal execution deduplication survives order removal; all private tracking collections are bounded.
- `PENDING`/`OPEN`/`UNTRIGGERED` orders: Added or updated in the store.

**Position Topic Handling** (`apexomni.ws-private.ts:321-328`):

- Maps positions via `mapPosition()`, updates store, records `lastWebsocketPositionsUpdate` timestamp, then calls `throttleFetchPositions()` for reconciliation.
- Realized PnL reads `curRealisedPnl` first, including explicit zero, then the documented WS `realizedPnl` field. Missing size, unknown side and non-finite financial values reject reconciliation; malformed balance values preserve the last known balance.

## Order Types and Mappings

### Supported Order Types

| Internal Type          | API Type(s)                                | Description                                         |
| ---------------------- | ------------------------------------------ | --------------------------------------------------- |
| `OrderType.Limit`      | `LIMIT`                                    | Standard limit order                                |
| `OrderType.Market`     | `MARKET`                                   | Market order (price set to mark +/-2% for slippage) |
| `OrderType.StopLoss`   | `STOP_MARKET` / `STOP_LIMIT`               | Stop loss trigger order                             |
| `OrderType.TakeProfit` | `TAKE_PROFIT_MARKET` / `TAKE_PROFIT_LIMIT` | Take profit trigger order                           |

### Order Status Mapping

| API Status    | Internal Status        |
| ------------- | ---------------------- |
| `PENDING`     | `OrderStatus.Open`     |
| `OPEN`        | `OrderStatus.Open`     |
| `FILLED`      | `OrderStatus.Closed`   |
| `CANCELED`    | `OrderStatus.Canceled` |
| `EXPIRED`     | `OrderStatus.Closed`   |
| `UNTRIGGERED` | `OrderStatus.Open`     |

### Side Mapping

| API    | Internal         |
| ------ | ---------------- |
| `BUY`  | `OrderSide.Buy`  |
| `SELL` | `OrderSide.Sell` |

### Time-in-Force Mapping

| Internal            | API                   |
| ------------------- | --------------------- |
| `GoodTillCancel`    | `GOOD_TIL_CANCEL`     |
| `ImmediateOrCancel` | `IMMEDIATE_OR_CANCEL` |
| `FillOrKill`        | `FILL_OR_KILL`        |
| `PostOnly`          | `POST_ONLY`           |

### Trigger Price Types

| API      | Internal                          |
| -------- | --------------------------------- |
| `INDEX`  | `OrderStopTriggerType.IndexPrice` |
| `MARKET` | `OrderStopTriggerType.LastPrice`  |

### Cancel Reason Enum

Defined in `apexomni.types.ts:97-105`:

- `UNKNOWN_ORDER_CANCEL_REASON` -- Unknown
- `EXPIRED` -- Order has expired
- `USER_CANCELED` -- User manually canceled
- `COULD_NOT_FILL` -- Unable to fill FOK/IOC/Post-only
- `REDUCE_ONLY_CANCELED` -- Unable to fulfil reduce-only
- `LIQUIDATE_CANCELED` -- Liquidation triggered cancellation
- `INTERNAL_FAILED` -- Internal processing / L2 validation failure

## Order Lifecycle

### Client Order ID Generation

Two formats exist:

1. **Exchange class** (`apexomni.exchange.ts:99-101`): `apexomni-{accountId}-{timestamp}-{random6digits}` where random digits come from `d8(6)` (numeric string).
2. **Utils** (`apexomni.utils.ts:11-13`): `apexomni-tealstreet-{random}` (not used by the exchange class).

### Nonce/Slot Derivation

From the client order ID (`apexomni.exchange.ts:829-830`):

- `nonce = BigNumber(toHex(sha256(clientOrderId))).mod(UINT32_MAX).toNumber()`
- `slotId = BigNumber(toHex(sha256(clientOrderId))).mod(UINT64_MAX).div(UINT32_MAX).toNumber()`

Where `UINT32_MAX = '4294967295'` and `UINT64_MAX = '18446744073709551615'`.

### Limit Fee Calculation

`limitFee = ceil(price * takerFeeRate * size)` rounded to 6 decimal places using `feeHelper()` (`apexomni.exchange.ts:820-826`).

### Order Placement Flow (`apexomni.exchange.ts:707-984`)

1. Look up market from store.
2. Initialize zkLink SDK and signer with `omniKeySeed`.
3. Adjust amount/price/stopLoss/takeProfit to market precision.
4. Calculate execution price:
   - Limit: use the provided price.
   - Market: `mark * 1.02` (buy) or `mark * 0.98` (sell).
   - StopLoss/TakeProfit: `calculateTpSlPrice()` with 0.5% slippage from trigger.
5. Build the request with the canonical `BROKER_ID` (`'6587'`).
6. **Order splitting**: If amount exceeds `maxOrderSize`, allocate whole amount ticks across bounded lots so their sum is exactly the adjusted request. Split brackets reject before SDK initialization or writes.
7. For each lot (sequential via `mapSeries`):
   - Generate client order ID.
   - Compute expiration: 30 days from now + 1-week buffer for on-chain validity.
   - Look up `l2PairId` from contract config.
   - Calculate `limitFee`.
   - Sign the order via `signContract()`.
   - If TP specified: generate separate client ID, nonce, signing payload, and signature for the TP sub-order (side is inverted, price uses `calculateTpSlPrice()`).
   - If SL specified: same as TP but for stop loss.
   - Serialize params with `qs.stringify()` (sorted, filtered).
   - POST to `CREATE_ORDER` via `unlimitedXHR` with `application/x-www-form-urlencoded` content type.
8. Trigger `throttleFetchOrders()` to sync state.
9. Return array of order IDs.

### TP/SL on Orders (Bracket Orders)

TP/SL are sent alongside the parent order using `isOpenTpslOrder`, `isSetOpenTp`, `isSetOpenSl` flags. Each TP/SL requires:

- Its own `clientOrderId`, `expiration`, `limitFee`, `signature` (all independently signed).
- The side is inverted from the parent order.
- Price is calculated via `calculateTpSlPrice()` (0.5% slippage from trigger).
- Trigger price type: TP defaults to `MARKET` unless `IndexPrice` specified; SL defaults to `INDEX` unless `LastPrice` specified.

### Virtual Order IDs

TP/SL sub-orders are represented internally with composite IDs:

- `{orderId}__take_profit`
- `{orderId}__stop_loss`

These are parsed by `mapOrder()` from the `openTpParams` / `openSlParams` (or `openTpParam` / `openSlParam`) fields on the parent order.

### Order Update (Cancel-and-Replace)

`updateOrder()` (`apexomni.exchange.ts:991-1042`):

- For TP/SL virtual orders: Finds the base order, cancels it, then re-places with updated TP/SL price.
- For regular orders: Cancels and re-places with updated price/amount.
- No native amend endpoint exists.

### Order Cancellation

`cancelOrdersHelper()` (`apexomni.exchange.ts:1051-1111`):

1. For composite IDs (`__stop_loss`, `__take_profit`): Extract base order ID. Cancelling the base order cancels all associated TP/SL.
2. **Blacklist**: Add order IDs to `recentlyCancelledOrders` map **before** API calls to prevent race conditions.
3. **Immediate store removal**: Remove all affected orders from store immediately.
4. Chunk into batches of 10 with 1-second sleep between batches.
5. Cancel sequentially within each batch via `forEachSeries`.
6. Uses `unlimitedXHR` for cancellation requests.

### Cancel Symbol Orders

`cancelSymbolOrders()` (`apexomni.exchange.ts:1113-1142`):

- Uses `CANCEL_ORDERS` endpoint to cancel all orders for a symbol.
- If `cancelTpSl` is false, saves TP/SL orders and re-creates them after the bulk cancel.

### Recently Cancelled Orders Blacklist

`recentlyCancelledOrders` (`Map<string, number>`, `apexomni.exchange.ts:138`):

- Tracks order IDs with cancellation timestamps.
- Entries expire after 10 seconds (cleanup in `fetchOrders()` at `apexomni.exchange.ts:427-430`).
- Prevents: (1) WebSocket re-adding cancelled orders before API processes them, (2) REST `fetchOrders` returning stale data.

## Symbol Format

- **API symbol (market ID)**: Hyphenated with settlement currency, e.g., `BTC-USDT`, `ETH-USDC`.
- **Internal symbol (crossSymbolName)**: Concatenated, e.g., `BTCUSDT`, `ETHUSDC`.
- **Market ID** is used in API requests; **symbol** is used internally for store lookups.
- **Conversion logic**: `symbol.includes('-')` checks determine which format to use (`apexomni.exchange.ts:763`, `apexomni.exchange.ts:1123`).

## Data Precision & Conversion

### Price and Amount Precision

From market config:

- `precision.amount` = `parseFloat(market.stepSize)` (e.g., `0.1`)
- `precision.price` = `parseFloat(market.tickSize)` (e.g., `0.001`)

### Adjustment

`adjust(value, precision)` from `safe-math` is used for all order values (`apexomni.exchange.ts:723-729`).

### zkLink Fixed-Point

For L2 signing, prices and sizes are scaled to 18 decimals: `BigNumber(value).multipliedBy(1e18).toFixed(0, ROUND_DOWN)`.

### Fee Rate Scaling

Fee rates for signing: `Math.ceil(feeRate * 1e4)` -- converting to basis points as integers (`apexomni.exchange.ts:834-835`).

### TP/SL Execution Price Calculation

`calculateTpSlPrice()` (`apexomni.exchange.ts:1402-1420`):

- BUY orders: trigger price \* 1.005 (0.5% slippage above)
- SELL orders: trigger price \* 0.995 (0.5% slippage below)
- Result is rounded to price step precision via `adjust()`.

### Balance Calculation

The Rust venue's `Venue::on_public_feed` hook revalues loaded open positions
from authoritative native ticker mark/last prices, using the same directional
PnL formula as its direct instrument handler. The direct mapper folds index
into mark only when the venue omits mark; an explicit zero mark falls back to
last even when index is present. Shared snapshots
retain the current mark across last-only patches. The hook does not establish
private readiness or alter balance fields; balance remains owned by the
existing authenticated account-balance reconciliation. Native engine tests
compare direct/shared hedged valuation and provider price restoration after a
delayed local quote seed.

Unrealized PnL is calculated locally (`apexomni.exchange.ts:330-340`) from positions and ticker prices rather than using the API value, due to incompatibility with the USDC PnL approach:

- `upnl = sum(direction * (ticker.last - entryPrice) * contracts)` for each position.
- `total = totalEquityValue - upnl` (wallet balance without PnL).
- `free = totalEquityValue - initialMargin`.

### Leverage Derivation

Position leverage (`apexomni.exchange.ts:1218-1225`):

1. If `customImr` is set: `round(1 / customImr)`
2. Else if `customInitialMarginRate` is set: `round(1 / customInitialMarginRate)`
3. Else if existing position in store has leverage: use that
4. Else: `1 / market.info.defaultInitialMarginRate`

## Rate Limiting

- **Rate-limited client** (`this.xhr`): 5 RPS via `axios-rate-limit` -- used for polling (balance, positions, account, symbols).
- **Unlimited client** (`this.unlimitedXHR`): No rate limit -- used for time-critical operations (order placement, cancellation, leverage changes).
- **Retry logic**: 3 retries on network errors (reads only). Signature failures (`20016`, `401`, or a message mentioning sign/signature) are handled by the one-shot legacy-signing resend in the response interceptor, not axios-retry.
- **Recv window**: 20,000ms default timeout (configurable via `options.extra.recvWindow`).
- **Cancel batching**: Orders cancelled in chunks of 10, 1-second sleep between batches (`apexomni.exchange.ts:1087-1090`).
- **Subscribe batching**: WebSocket topics subscribed in chunks of 50 (`apexomni.ws-public.ts:71`).

## State Reconciliation

### Tick Loop

`tick()` (`apexomni.exchange.ts:306-316`) runs `fetchAndSetPrivateSnapshotHelper()` in a loop with configurable `tickInterval`.

### Private Snapshot Helper (`apexomni.exchange.ts:271-304`)

1. **Parallel**: Fetch positions and orders via REST.
2. **Position merging**: If WebSocket positions were updated within `INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY` (1 second), merge REST data with store using `mergeUpdatesUsingLastUpdate()`. Otherwise, use REST data directly.
3. Update positions and orders in store.
4. Fetch and update balance.

### WebSocket -> REST Reconciliation

- **Orders**: `throttleFetchOrders()` debounces REST refetch by 1 second after WebSocket order events. Skipped for cancellation-only batches.
- **Positions**: `throttleFetchPositions()` debounces REST refetch by 1 second after every WebSocket position event. This ensures WS updates are quickly followed by REST verification.
- **On reconnect**: If `ENABLE_RECONNECTING_WEBSOCKET`, full private snapshot is fetched on WebSocket reconnection.

### Order Store Strategy

- `fetchOrders()` only returns the mapped REST snapshot. The private snapshot
  helper and throttled WebSocket heal commit successful results with
  `store.addOrUpdateOrdersFull()` (full replacement), which removes orders no
  longer in the API response.
- Missing arrays and malformed financial rows reject the read; they never become an authoritative empty snapshot. Missing or blank symbols/order IDs fail before unknown-market rows are skipped.
- WebSocket `handleOrderTopic()` uses `store.addOrUpdateOrders()` (partial update) and `store.removeOrders()` for cancellations.

### Fake Positions

`fetchPositions()` creates placeholder long and short positions for every market to ensure the UI always shows position slots. Real and placeholder IDs both use the canonical market symbol, so a native symbol such as `BTC-USDT` cannot create a second slot beside `BTCUSDT:long`.

## Exchange Metadata

### Candle Intervals

| Timeframe | API interval |
| --------- | ------------ |
| `1m`      | `1`          |
| `3m`      | `3`          |
| `5m`      | `5`          |
| `15m`     | `15`         |
| `30m`     | `30`         |
| `1h`      | `60`         |
| `2h`      | `120`        |
| `4h`      | `240`        |
| `6h`      | `360`        |
| `12h`     | `720`        |
| `1d`      | `D`          |
| `1w`      | `W`          |

### Grafana Monitoring

- **Every-request tracking**: `CREATE_ORDER`, `SET_LEVERAGE`, `CANCEL_ORDER`, `CANCEL_ORDERS`, `USER`
- **Periodic tracking**: `ACCOUNT`, `BALANCE`

### Broker ID

All orders use the single `BROKER_ID = '6587'` constant for Tealstreet
attribution. The trusted order-placement regression pins the exact serialized
transport payload so the numeric broker ID cannot drift back to a display name.

## Testnet/Demo

| Environment | REST Base URL                            | Public WS                                                   | Private WS                                                                           |
| ----------- | ---------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| Livenet     | `https://omni.apex.exchange/api`         | `wss://quote.omni.apex.exchange/realtime_public?v=2&...`    | `wss://quote.omni.apex.exchange/realtime_private?v=2&...` (or proxy when enabled)    |
| Testnet     | `https://testnet.omni.apex.exchange/api` | `wss://qa-quote.omni.apex.exchange/realtime_public?v=2&...` | `wss://qa-quote.omni.apex.exchange/realtime_private?v=2&...` (or proxy when enabled) |

Note: Private WS now follows `BASE_WS_URL.private` for livenet/testnet and appends a live timestamp + requestId. Proxy routing is opt-in via `tealstreetProxy`.

## Known Quirks / Implementation Notes

### Dual XHR Clients

The exchange maintains two axios clients (`apexomni.exchange.ts:119-123`): a rate-limited one (5 RPS) for background polling and an unlimited one for order operations. This prevents background balance/position polling from blocking urgent order placement.

### Private WebSocket Endpoint Selection

Private WebSocket uses direct ApeX endpoints by default and switches to Tealstreet proxy only when `tealstreetProxy` is enabled.

### Order Splitting

Large orders exceeding `maxOrderSize` are automatically split into multiple lots only when they do
not carry attached TP/SL. A split order with `stopLoss` or `takeProfit` fails before SDK setup or any
REST write. The official `/v3/order` contract covers one parent plus its attached TP/SL legs; its REST
success only means the request was received, with acceptance confirmed later through WebSocket order
state. Although the official Python connector contains an undocumented `/v3/batch-orders` call, ApeX
does not publish an all-or-none commit/rollback guarantee for its rows. Sequential calls and that
undocumented batch can therefore both leave only some lots live or protected. Single-lot orders keep
the documented attached TP/SL path, with `tpSize` and `slSize` matching the exact signed parent size.

The upstream blocker is specifically a missing atomic contract, not missing request serialization:
safe split brackets require ApeX to document that every parent and attached leg in a multi-order
request is accepted or rejected as one transaction, including the behavior when matching/L2
validation rejects one row after REST receipt. Until then, `placeOrder()` emits and throws
`ApeX Omni cannot atomically attach TP/SL to split orders` before signing or sending anything.

### Virtual bracket updates

Updating a virtual `__take_profit` or `__stop_loss` child cancels and replaces the parent order. The
replacement copies both stored virtual siblings before applying the edited price, so changing one leg
does not silently remove the other. Any cancellation or replacement failure rejects the update; once
the old parent has been cancelled, callers must never interpret an empty result as success.
Amending the regular parent also preserves both protective siblings. Every replacement gets a fresh generated client ID; the original caller ID stays attached to the canceled order.

### OHLCV Kline Auth Exception

Kline requests are sent without authentication headers -- the axios interceptor strips them for the `/v3/klines` endpoint (`apexomni.api.ts:54-56`).

### Expiration Buffer

Order signatures include a 1-week buffer (168 hours) added to the expiration time via `addOrderExpirationBufferHours()` to ensure signatures remain valid when processed on-chain (`apexomni.exchange.ts:83-86`). Base expiration is 30 days from current time.

### Position Mode and Margin

- **Position mode**: Always `PositionMode.Hedged` (separate long/short per symbol).
- Authenticated startup publishes `store.options.isHedged = true` for consumers of account mode.
- **Margin type**: Always `MarginType.Cross` (not configurable).
- **Leverage**: Sent as `initialMarginRate = 1/leverage` to the `SET_LEVERAGE` endpoint, clamped to market min/max bounds. If the symbol is absent from the store, `setLeverage()` refreshes positions, re-filters for the symbol, and fails clearly if state is still unavailable.
- **Account identity**: `getAccount()` reads `data.data.id` from `/v3/account`. The documented v3 response has no inviter field, so `affiliateId` is null.

### Ticker Abbreviations

Ticker data fields from WebSocket use single-letter abbreviations: `s` (symbol), `pr` (price ratio), `p` (last price), `h` (high), `l` (low), `mp` (mark price), `xp` (index price), `o` (open interest), `v` (volume), `fr` (funding rate), `to` (turnover/quote volume), `tc` (trade count).

### Market Filtering

Only markets with `enableTrade: true` are included (`apexomni.exchange.ts:551`).

### mapPosition Null Safety

`mapPosition()` returns `null` if the market is not found for a position's symbol, with a warning log (`apexomni.exchange.ts:1204-1207`). Callers filter out nulls.

### mapOrder Empty Array on Missing Market

`mapOrder()` returns an empty array `[]` if the market is not found (`apexomni.exchange.ts:1329-1332`), with a console warning.
Private WebSocket order processing treats that as an ignorable per-order result and continues the rest of the batch before any `orders[0]` access.

### TP/SL Params API Inconsistency

The API returns both singular (`openSlParam`, `openTpParam`) and plural (`openSlParams`, `openTpParams`) versions of TP/SL data. The code checks both (`apexomni.exchange.ts:1362-1363`).

### Content-Type for SET_LEVERAGE

The `SET_LEVERAGE` endpoint requires `multipart/form-data`. This is set both in the `setLeverage()` call (`apexomni.exchange.ts:1166`) and via the interceptor (`apexomni.api.ts:96-98`).

## Error Handling

### Response Interceptor

The axios response interceptor (`apexomni.api.ts:107-115`) checks for non-zero `code` field and throws with `[code] msg` format. This means callers receive a thrown error rather than needing to check response codes.

### Retry Strategy

The API client retries up to 3 times on:

- Network errors on reads (detected by `axios-retry`'s `isNetworkError`)

Signature errors are not an axios-retry case: Apex reports them in a 200 envelope (`code`/`msg`), and the response interceptor resends once with legacy signing. The removed `isErrorSignature` read Bybit's `ret_msg`/`retMsg`, which Apex never sends.

### Cancel Reason Error Emission

Cancel reasons from WebSocket are emitted as errors unless they are `UNKNOWN_ORDER_CANCEL_REASON`, `INTERNAL_FAILED`, or `USER_CANCELED` (`apexomni.ws-private.ts:288-294`).

### False Positive FILLED Detection

FILLED order events with `price <= 0 && amount <= 0` are silently ignored as they are false positives from order replacements (`apexomni.ws-private.ts:267-269`).

### WebSocket Error Logging

Private WebSocket errors during throttled refetch are caught and logged to console with `[APEX OMNI]` prefix (`apexomni.ws-private.ts:142-143`, `apexomni.ws-private.ts:160-161`).

## Testing / Validation

Fresh deterministic regressions belong in the package-level
`src/__tests__/audit-regressions/` directory, not beside exchange runtime
code. Run the trusted suite from the `packages/safe-cex` directory:

```bash
cd packages/safe-cex && yarn test-ci
```

### Account Validation

`validateAccount()` (`apexomni.exchange.ts:173-191`):

1. Validates REST API credentials by calling `GET /v3/user`.
2. Validates zkLink signing key by initializing SDK and attempting `signer.initZklinkSigner(omniKeySeed)`.
3. Returns empty string on success, or error message on failure (`'Invalid OmniKey seed'` for bad seed).

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends the caller's ID as both `clientId` and `clientOrderId`; it is hashed into the signed nonce, so it is resolved before SDK init and signing. Rule: non-empty, no whitespace.
- Mapped orders expose `clientOrderId` from the venue's `clientOrderId` or `clientId` field.


## Rust public discovery resilience

Rust catalogue/ticker reads retry through the shared engine capped exponential
backoff. A malformed typed public row is skipped with a warning while valid
neighbours remain usable. Private account initialization starts once the usable
catalogue is available, without waiting for REST ticker seeds. Account snapshots,
mutation acknowledgements and signing validation retain their stricter rules.
See `docs/plans/RUST_STARTUP_AUDIT.md` and the native startup resilience regressions.

## Rust startup valuation

The Rust balance aggregate values each position from the live `ticker.last` like
TS whenever that price is finite and positive (native PnL lags). Only with a
placeholder, zero or non-finite quote does it fall back to reported private
`unrealisedPnl`, so a placeholder cannot manufacture a funded-account loss. When a private
position omits PnL, Rust retains that position's prior finite valuation and warns;
new exposure remains visible with unavailable PnL represented as zero. Realized
PnL stays separate. Wallet total remains `totalEquityValue - upnl`, with native
margin-derived free/used values preserved. The dedicated native
`tests/startup_valuation.rs` covers startup, native updates and omission recovery.

## Rust port-review read corrections

Mapped order remaining and late-fill default amendment sizes preserve operand
decimal precision without rounding to market lot steps. REST candle history skips
typed invalid price/volume rows with coded warnings and returns sorted valid peers.
Malformed fields/timestamps/envelopes and nonempty all-invalid results fail; a
valid empty symbol array remains empty success. Live candle rules are unchanged.
Client IDs on protective/multi-leg placement remain rejected before writes, and
explicit leverage commands continue sending a verified write despite cached equality.
