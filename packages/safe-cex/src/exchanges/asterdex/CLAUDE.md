# AsterDEX Exchange Integration

## Exchange Information

AsterDEX is a decentralized perpetual futures exchange with Binance-style market semantics.  
This integration now supports a **dual API model**:

- **V1**: API key + HMAC-SHA256 signing (legacy/default path).
- **V3**: API wallet/agent signing (ECDSA over keccak payload with microsecond nonce).

The uplift is intentionally backward-compatible: existing V1 accounts continue to work unchanged while V3 is available for migration.

## API Documentation

- **V1 Futures API docs (legacy)**: <https://github.com/asterdex/api-docs/blob/master/V1%28Legacy%29/EN/aster-finance-futures-api.md>
- **V3 Futures API docs (recommended)**: <https://github.com/asterdex/api-docs/blob/master/V3%28Recommended%29/EN/aster-finance-futures-api-v3.md>
- **Official changelog**: <https://github.com/asterdex/api-docs/blob/master/CHANGELOG.md>
- **V1 vs V3 overview**: <https://github.com/asterdex/api-docs/blob/master/Aster%20API%20Overview.md>
- **CCXT REST reference**: <https://github.com/ccxt/ccxt/blob/master/ts/src/aster.ts>
- **CCXT WS reference**: <https://github.com/ccxt/ccxt/blob/master/ts/src/pro/aster.ts>
- **Main website**: <https://asterdex.com>

## File Structure

- `asterdex.exchange.ts`: main exchange class; resolves API version and routes fapi calls via `getFapiPath()`
- `asterdex.api.ts`: REST client with V1+V3 signing logic, timestamp retry, and multiple rate-limited clients
- `asterdex.builder-fee.ts`: Aster Code builder-address/rate resolver and maker/taker placement policy
- `asterdex.builder-approval.ts`: Aster Code builder approval check and main-wallet approval helpers
- `asterdex.types.ts`: exchange types plus version/auth helpers (`resolveAsterDEXApiVersion`, `hasAsterDEXV3AuthContext`, `getAsterDEXFapiPath`)
- `asterdex.virtualClock.ts`: Aster-specific time sync with V1/V3 time endpoint fallback and proxy fallback
- `asterdex.ws-public.ts`: public WebSocket (tickers, depth, trades, mark prices, klines)
- `asterdex.ws-private.ts`: private WebSocket (listen key lifecycle, order/account events)

## API Version Selection

Version is resolved in `resolveAsterDEXApiVersion()` using this order:

1. **Explicit override**: `options.extra.asterdexApiVersion` (`'v1' | 'v3'`)
2. **Auto-V3**: enabled only when all V3 auth fields are present and valid:
   - `options.auth.userAddress`
   - `options.auth.signerAddress`
   - valid 64-byte hex private key (`options.auth.privateKey` or `options.secret`)
3. **Fallback**: `v1`

`AsterDEXExchange` stores this as `apiVersion` and all REST calls route through `getFapiPath(endpoint, version?)`.

## Authentication

### V1 (Legacy / Default)

- Header: `X-MBX-APIKEY`
- Params include `recvWindow` + `timestamp`
- Signature: HMAC-SHA256 over query string

### V3 (Wallet / Agent Model)

- Header: `X-MBX-APIKEY`
- Standard signed params include `recvWindow`, `timestamp`, plus:
  - `user`
  - `signer`
  - `nonce` (microseconds, monotonic)
- Saved account mapping: `applicationId` is the `user` / login-main wallet,
  `apiKey` is the API Wallet / agent / vault address (`signer`), and
  `apiSecret` is the private key for that API wallet. Users must not replace
  the main wallet with the API/vault wallet; both identities are required for
  V3 trading/query endpoints.
- Signature flow:
  1. Convert business params to string values and sort keys
  2. ABI encode `[string payloadJson, address user, address signer, uint256 nonce]`
  3. keccak hash encoded payload
  4. apply Ethereum signed-message hash
  5. sign via secp256k1 ECDSA
- Aster Code query/trading endpoints (`POST /fapi/v3/order`, `GET /fapi/v3/builder`,
  `GET /fapi/v3/agent`) use the newer EIP-712 `Message.msg` signer instead:
  1. Build the exact final query string in insertion order, without URL encoding
  2. Append `asterChain`, `user`, `signer`, and monotonic `nonce`
  3. Sign `types: { Message: [{ name: 'msg', type: 'string' }] }` under domain
     `AsterSignTransaction` v1, zero verifying contract, chainId `1666` mainnet or `714` testnet
  4. Send that exact query string plus `signature`

The signed `msg` must equal the sent query string byte-for-byte excluding the trailing
`signature`. Do not reorder params, pass decimals as JS numbers, or rely on Axios
default serialization for Aster Code requests. V1 HMAC signing is unchanged.

### Common Request Behavior

- All private request parameters are sent as query params (including POST/PUT/DELETE).
- Request body is deleted for signed requests.
- Timestamp recovery (`-1021`) resyncs clock and retries once with fresh signature.

### Secondary read-only key (read/write signing split)

Unlike the single-keypair wallet DEXs (hyperliquid, paradex, lighter, …),
AsterDEX **does** support a secondary read-only key — so it is NOT in
`exchangesWithoutSecondaryKey` (`utils/exchangeSupportsSecondaryKey.ts`). It works
via the V3 **agent** model: a read-only agent wallet shares the account's main
wallet (`applicationId` → `userAddress`) and differs only in the agent's signer
address (`secondary_apiKey` → `signerAddress`) and its private key
(`secondary_apiSecret` → `signerPrivateKey`).

Wiring: `createAPIInstances`'s **`primary`** client is built through
`createApiWithSecondaryKey`, so when a secondary key is set, GET (read) requests —
account, positions, open orders, balance, user trades, leverage bracket — sign with
the read-only agent, while writes (order place/cancel via `secondaryAPI`; leverage/
margin changes) stay on the primary agent. Default-off: with no secondary key all
clients are identical (byte-for-byte unchanged behavior). `validateAccount` is a GET,
so with a secondary key present it validates the **read** key (the write key is
exercised on first order) — same caveat as the other non-binance HMAC exchanges.

**Egress.** `createAPI` takes its transport from a per-call `transportOptions`
argument (default = the primary account). The secondary read client passes its own
`ExchangeOptions`, so its GETs pick up `overrideProxyUrl` = the public CORS
proxy (`PUBLIC_CORS_PROXY`) — reads egress there while writes stay on the
primary transport. This
is the same handling as the tunnelled CORS HMAC exchanges (bitget et al.); Aster's
`createAPI` previously read transport from `exchange.options` and ignored the override,
which is why the split had to thread `transportOptions` through. Default-off: with no
secondary key `transportOptions` defaults to the primary and every client is
byte-identical to before.

### Public Endpoints (No Auth)

Both V1 and V3 public prefixes are treated as public (examples):

- `/fapi/v1/*` public market endpoints
- `/fapi/v3/*` public market endpoints

### Time Synchronization

`AsterDEXVirtualClock`:

- syncs on startup and every 5 minutes
- attempts preferred version first, then fallback:
  - preferred `v1`: `/fapi/v1/time` then `/fapi/v3/time`
  - preferred `v3`: `/fapi/v3/time` then `/fapi/v1/time`
- also attempts proxy fallback via the transport's public CORS proxy (`withPublicCorsProxy`)
- falls back to local UTC time if all attempts fail

## Startup Sequence

`start()` flow:

1. resolve API mode (`v1`/`v3`) and initialize APIs
2. sync exchange clock with preferred API version
3. validate account credentials (private mode only)
4. fetch markets + tickers (unless external public data is configured)
5. fetch positions + open orders (private mode only; failures are non-fatal)
6. fetch current position mode via `GET /fapi/{version}/positionSide/dual` and set `store.options.isHedged`
7. connect public WS (unless external public data is configured)
8. connect private WS (private mode only)
9. start the private health loop (private mode only). The loop wakes every
   3 seconds but only performs REST work when the balance or safety
   reconciliation interval is due.

Private-mode checks are based on `!isPublicDataOnly` (which now respects V3 auth readiness via overridden `canSign`).

## REST API Endpoints (Version-Aware)

Most endpoints use `/fapi/{apiVersion}/...` where `apiVersion` is `v1` or `v3`.

| Endpoint (logical) | V1 Path                      | V3 Path                      | Notes                        |
| ------------------ | ---------------------------- | ---------------------------- | ---------------------------- |
| account            | `/fapi/v4/account`           | `/fapi/v3/account`           | V1 keeps v4 account endpoint |
| position risk      | `/fapi/v1/positionRisk`      | `/fapi/v3/positionRisk`      | signed                       |
| open orders        | `/fapi/v1/openOrders`        | `/fapi/v3/openOrders`        | signed                       |
| order place/cancel | `/fapi/v1/order`             | `/fapi/v3/order`             | signed                       |
| exchange info      | `/fapi/v1/exchangeInfo`      | `/fapi/v3/exchangeInfo`      | public                       |
| leverage bracket   | `/fapi/v1/leverageBracket`   | `/fapi/v3/leverageBracket`   | signed                       |
| ticker 24h         | `/fapi/v1/ticker/24hr`       | `/fapi/v3/ticker/24hr`       | public                       |
| klines             | `/fapi/v1/klines`            | `/fapi/v3/klines`            | public                       |
| depth              | `/fapi/v1/depth`             | `/fapi/v3/depth`             | public                       |
| trades             | `/fapi/v1/trades`            | `/fapi/v3/trades`            | public                       |
| user trades        | `/fapi/v1/userTrades`        | `/fapi/v3/userTrades`        | signed                       |
| leverage           | `/fapi/v1/leverage`          | `/fapi/v3/leverage`          | signed                       |
| position mode      | `/fapi/v1/positionSide/dual` | `/fapi/v3/positionSide/dual` | signed                       |
| margin type        | `/fapi/v1/marginType`        | `/fapi/v3/marginType`        | signed                       |
| STP mode           | not available                | `/fapi/v3/stpMode`           | signed, account-global       |
| listen key         | `/fapi/v1/listenKey`         | `/fapi/v3/listenKey`         | private WS lifecycle         |
| time               | `/fapi/v1/time`              | `/fapi/v3/time`              | public                       |

## WebSocket Implementation

### Network Configuration

| Component  | Production                                             | Testnet                                                         |
| ---------- | ------------------------------------------------------ | --------------------------------------------------------------- |
| Public WS  | `wss://fstream.asterdex.com/stream?requestId=<nanoid>` | `wss://fstream5.asterdex-testnet.com/stream?requestId=<nanoid>` |
| Private WS | `wss://fstream.asterdex.com/ws/<listenKey>`            | `wss://fstream5.asterdex-testnet.com/ws/<listenKey>`            |

Each environment uses one host for public and private streams, with `/stream`
for public multiplexed traffic and `/ws/<listenKey>` for private user data.

### Public WebSocket (`asterdex.ws-public.ts`)

**On connect** (`AsterDEXPublicWebsocket.onOpen`):

1. Sends initial ping
2. Subscribes to aggregate streams only (to avoid disconnections from too many individual subscriptions):
   - `!ticker@arr` - All tickers in batch
   - `!markPrice@arr@1s` - All mark prices at 1-second intervals
3. Resubscribes to any previously-registered on-demand topics (depth, kline streams) from the `topics` map
4. Emits `wsReconnected` if reconnecting WebSocket is enabled
5. Sets `wsReady = true` after subscriptions complete

**Subscription format**:

```json
{ "method": "SUBSCRIBE", "params": [...streams], "id": <incrementing_id> }
```

**On-demand subscriptions**:

- `<symbol>@depth20@100ms` - documented 20-level partial-book snapshots
- `<symbol>@kline_<interval>` - Kline/candlestick updates
- `<symbol>@aggTrade` - Aggregate trade updates

**Ping mechanism** (`AsterDEXPublicWebsocket.ping`):

- Sends `{ "id": -1, "method": "LIST_SUBSCRIPTIONS" }` as application-level ping
- `-1` is reserved; normal subscription request IDs are positive and cannot be
  consumed as pongs.

**Message routing** (`AsterDEXPublicWebsocket.onMessage`):

1. Check for pong response (id === -1)
2. Check for subscription confirmation (result === null)
3. Check for error response
4. Handle stream data with `stream` field:
   - `!markPrice@arr@1s` / `!markPrice@arr`: Iterate array, call `handleMarkPriceUpdate` per item
   - `!ticker@arr`: Iterate array, call `handleTickerUpdate` per item
   - Other streams: Route via `handleStreamData` which maps stream type to event handler
5. Handle batch updates (array format, legacy)
6. Handle single events (object with `.e` field)

**Orderbook subscription** (`listenOrderBook`):

- Uses the documented `depth20` partial-book stream, never bare `@depth` (which is an incremental diff stream).
- Publishes only snapshots with positive levels on both sides and a non-crossed best bid/ask. Invalid frames are dropped; the last good book remains visible.
- Multiplexes listeners by canonical stream: one upstream subscription fans out isolated snapshot copies, per-callback unsubscribe keeps the stream alive, and only the final listener removes the topic and sends `UNSUBSCRIBE`.
- Tracks active streams in `topics` for reconnection; disposal clears listener registries and pending retries.
- Uses retry loop: up to 50 attempts with 100ms intervals to wait for WS connection
- Returns unsubscribe function that cleans up all callback references and topics

**Kline subscription** (`AsterDEXPublicWebsocket.subscribeToKline`):

- Wait-for-connection with exponential backoff: `100ms * 1.5^(floor(retryCount/10))`, max 2000ms
- If connected but not initialized, queues subscription in `pendingSubscriptions`

**Reconnection behavior** (`onClose`, `asterdex.ws-public.ts`):

- Resets `wsReady = false` and clears `pendingSubscriptions`
- Does NOT clear `topics` (needed for reconnection resubscription)
- Calls the captured base `onClose` instance handler. `BaseWebSocket.onClose` is an arrow field, not a prototype method, so do not use `getPrototypeOf` for this.

### Private WebSocket (`asterdex.ws-private.ts`)

**Connection** (`AsterDEXPrivateWebsocket.connectAndSubscribe`):

1. Fetches listen key via `POST /fapi/{version}/listenKey` (resolved by `getFapiPath`)
2. Constructs the environment-specific URL: production
   `wss://fstream.asterdex.com/ws/<listenKey>` or testnet
   `wss://fstream5.asterdex-testnet.com/ws/<listenKey>`
3. No explicit subscription needed - events auto-sent once connected

**Listen key management** (`fetchListenKey` / `updateListenKey`):

- Created via `POST /fapi/{version}/listenKey`
- Refreshed every 30 minutes via `PUT /fapi/{version}/listenKey` (key valid for 60 minutes)
- Refresh scheduled via `setTimeout`, re-scheduled after each refresh

**On connect** (`onOpen`):

1. Sends initial ping
2. If `ENABLE_RECONNECTING_WEBSOCKET`, calls `fetchAndSetPrivateSnapshot(10)` to re-fetch orders (10 retry attempts)

**Ping**: Same `{ "id": 42, "method": "LIST_SUBSCRIPTIONS" }` pattern as public WS

**Event types handled**:

| Event                     | Handler                       | Description                                 |
| ------------------------- | ----------------------------- | ------------------------------------------- |
| `ORDER_TRADE_UPDATE`      | `handleOrderTradeUpdate`      | Futures order creates, fills, cancellations |
| `ACCOUNT_UPDATE`          | `handleAccountPositionUpdate` | Position and balance changes                |
| `executionReport`         | `handleExecutionReport`       | Legacy/spot order execution reports         |
| `outboundAccountPosition` | `handleAccountUpdate`         | Legacy account balance updates              |

**ORDER_TRADE_UPDATE handling** (`asterdex.ws-private.ts`):

- `NEW`: Adds order to store via `addOrUpdateOrder`
- `PARTIALLY_FILLED` / `FILLED`: Mirrors cumulative `filled` / `remaining`
  onto the existing store order immediately, then emits the `fill` event
  with `L` (last execution price, `ap` fallback) and `l` (last filled qty). Do not remove the
  immediate store update — chasers and order rows should not wait for the
  slower REST order refresh to see partial fill size.
- `FILLED`: Updates status to `Closed`, remaining to 0
- `CANCELED` / `REJECTED` / `EXPIRED`: Updates status to `Canceled`
- Every order lifecycle event schedules a separate debounced REST order
  reconciliation. Order repair does not run from account position events.

**ACCOUNT_UPDATE handling** (`asterdex.ws-private.ts`):

- Processes position updates from `data.a.P` array
- In one-way mode (`ps === 'BOTH'`), determines side from position amount sign
- For non-zero positions: Updates existing position or logs that `fetchPositions()` will create it
- For zero positions: Removes all positions for that symbol (position closed)
- Handles position flip: If new side doesn't match existing, removes opposite side position
- **Position repair only**: Increments the exchange's
  `positionStateVersion`, writes the WS deltas to the store, then schedules a
  delayed `fetchPositions()` repair. The repair commits only if
  `positionStateVersion` did not advance while the REST request was in
  flight; otherwise the stale response is discarded and a queued follow-up
  repair runs. Do not schedule order refreshes from `ACCOUNT_UPDATE`.
- Processes balance updates from `data.a.B` array (wallet balance, cross wallet balance)

## Order Types and Mappings

- `OrderType.Limit` -> `LIMIT`: Requires `price` and `timeInForce`
- `OrderType.Market` -> `MARKET`: No `price` or `timeInForce`
- `OrderType.StopLoss` -> `STOP_MARKET`: Requires `stopPrice` and `workingType`
- `OrderType.TakeProfit` -> `TAKE_PROFIT_MARKET`: Requires `stopPrice` and `workingType`
- `OrderType.TrailingStopLoss` -> `TRAILING_STOP_MARKET`: Disabled; Aster requires `callbackRate`, which `PlaceOrderOpts` cannot represent unambiguously.

Additional reverse mappings exist for `STOP` -> `StopLoss`, `TAKE_PROFIT` -> `TakeProfit`, `TRAILING_STOP` -> `TrailingStopLoss` (`AsterDEXExchange.reverseMapOrderType`).

### Time In Force

| Internal TIF        | AsterDEX Value | Description         |
| ------------------- | -------------- | ------------------- |
| `GoodTillCancel`    | `GTC`          | Good Till Cancel    |
| `ImmediateOrCancel` | `IOC`          | Immediate or Cancel |
| `FillOrKill`        | `FOK`          | Fill or Kill        |
| `PostOnly`          | `GTX`          | Good Till Crossing  |

### Order Status Mapping

| API Status         | Internal Status        |
| ------------------ | ---------------------- |
| `NEW`              | `OrderStatus.Open`     |
| `PARTIALLY_FILLED` | `OrderStatus.Open`     |
| `FILLED`           | `OrderStatus.Closed`   |
| `CANCELED`         | `OrderStatus.Canceled` |
| `REJECTED`         | `OrderStatus.Canceled` |
| `EXPIRED`          | `OrderStatus.Canceled` |

## Order Lifecycle

### Regular Orders (Limit/Market)

1. **Place**: `POST /fapi/{version}/order` via `secondaryAPI` (5 RPS rate limit) with params: `symbol`, `side`, `type`, `quantity`, `price` (limit only), `timeInForce` (non-market), `reduceOnly`
2. **Close position orders**: When `amount === 0` for TP/SL, `placeOrder` sets `closePosition='true'` instead of `quantity`, and omits `reduceOnly`.
3. **Update** (`AsterDEXExchange.updateOrder`): Uses **cancel-and-replace** pattern (no PUT endpoint). For TP/SL orders, maps `Order.price` to `triggerPrice` since the price field holds the trigger price.
4. **Cancel**: `DELETE /fapi/{version}/order` with `symbol` and `orderId` params via `secondaryAPI`

### Bracket Orders (TP/SL with entry)

When `takeProfit` or `stopLoss` is provided with a limit/market order (`AsterDEXExchange.placeOrder`):

1. Splits into array of orders: main entry + SL + TP
2. TP/SL orders have opposite side, `reduceOnly: true`
3. All placed sequentially via `placeOrders()`

### Batch Operations

`placeOrders` and `cancelOrders` iterate individually (no batch API). They continue on individual failures and throw only if all operations fail.

### Stop Price Trigger Types (`AsterDEXExchange.placeOrder`)

- **TakeProfit**: Defaults to `CONTRACT_PRICE` (LastPrice), uses `MARK_PRICE` only if explicitly set to `MarkPrice`
- **StopLoss**: Defaults to `MARK_PRICE` (MarkPrice), uses `CONTRACT_PRICE` only if explicitly set to `LastPrice`

### Self-Trade Prevention (V3 only)

- `fetchStpMode()` reads the account-global mode and accepts only
  `EXPIRE_TAKER`, `EXPIRE_MAKER`, or `EXPIRE_BOTH`.
- `changeStpMode(mode)` changes the account-global mode.
- Aster-specific callers may pass `stpMode` through
  `AsterDEXPlaceOrderOpts` to override the account default for one order.
- All three paths reject before request I/O on V1, and malformed runtime
  values or unknown read responses fail closed.

### Builder Attribution (Aster Code)

- Aster futures builder attribution is per-exchange only. It does not stack with
  Hyperliquid or any other venue's builder fee.
- Order placement uses one decimal-string `feeRate` per order plus `builder`; Aster
  does not expose separate venue-side maker/taker rates. Tealstreet resolves a client-side
  maker/taker policy at placement time:
  - post-only `LIMIT` (`GTX`) uses `feeRateMaker`
  - every other order path uses `feeRateTaker`
- Built-in maker/taker policy rates default to `0.0001` (1 bps). A real
  builder wallet address is still required; placeholder builder constants keep
  attribution disabled.
- Futures fee cap is `0.001` (10 bps). Reject invalid, negative, scientific-notation,
  NaN, or over-cap rates. Do not clamp them.
- Resolver skips attribution entirely when unconfigured, when both configured rates are
  `0`, or when the selected maker/taker rate for this order is `0`. Never send
  `builder` without `feeRate`, never send `feeRate` without `builder`, and never send
  `feeRate='0'`.
- Split oversized children, bracket TP/SL siblings, `placeOrders()` loops, and
  `closePosition='true'` TP/SL close orders all carry attribution when configured.
- ADL and liquidation orders are excluded from builder fees by Aster itself; do not
  attempt to work around that.
- Approval requires the user's main wallet EIP-712 signature (`ApproveBuilder`). An
  agent-only imported account cannot approve in-app; surface actionable guidance to
  re-add with the main wallet or approve out-of-band on Aster.
- `ApproveBuilder` is a main-wallet action and must follow Aster's demo shape
  exactly:
  - switch/request the browser wallet on BNB Smart Chain (`chainId` `0x38`)
    before `eth_signTypedData_v4`
  - EIP-712 domain `chainId` is `56` for EVM wallets, and submitted
    `signatureChainId` is also `56`; `asterChain` remains `Mainnet`/`Testnet`
  - include the explicit `EIP712Domain` type in the typed data plus
    `ApproveBuilder` fields in this order: `Builder`, `MaxFeeRate`,
    `BuilderName`, `AsterChain`, `User`, `Nonce`
  - submit `POST /fapi/v3/approveBuilder` as
    `application/x-www-form-urlencoded` body fields in demo order:
    `builder`, `maxFeeRate`, `builderName`, `asterChain`, `user`, `nonce`,
    `signature`, `signatureChainId`
  - do not reuse the Aster Code query/order signer chain ids (`1666`/`714`)
    for approval signatures; using them opens or signs in some wallets but
    Aster rejects the submit with signature verification failure
- Browser account-form approval checks/submits use the Tealstreet JP proxy when
  a proxy token is available. A direct local browser request can fail with
  `Ip not in whitelist` even when the saved account would work through the
  normal exchange tunnel/proxy path.
- Read-side approval checks use signed `GET /fapi/v3/builder`. Treat parse/network
  failures as `unverified`, not as a denial, so already-approved users are not locked
  out by a transient read failure.

## Symbol Format

- **Format**: Concatenated base + quote: `BTCUSDT`, `ETHUSDT`
- **No transformation needed** - symbols pass through directly (unlike Orderly which uses `PERP_` prefix)
- Market ID is the symbol itself (e.g., `BTCUSDT`)

## Data Precision & Conversion

### Market Precision (`AsterDEXExchange.fetchMarkets`)

- `pricePrecision` and `quantityPrecision` from `exchangeInfo` are integer digits (e.g., `2` means 2 decimal places)
- Converted to decimal step size via `precisionToString()` utility (e.g., `2` -> `0.01`)
- **Tick size**: Extracted from `PRICE_FILTER` filter's `tickSize` field. Falls back to `market.precision.price`.
- **Lot size**: `minQty` and `maxQty` from `LOT_SIZE` filter
- Price rounding uses `adjust()` utility with the tick size, not the decimal precision directly

### Balance Calculation (`AsterDEXExchange.fetchBalance`)

- `total`: `totalWalletBalance` from account endpoint (`/fapi/v4/account` on V1, `/fapi/v3/account` on V3)
- `free`: `availableBalance`
- `used`: `totalInitialMargin + totalOpenOrderInitialMargin`
- `upnl`: `totalUnrealizedProfit`

### Leverage Brackets (`AsterDEXExchange.fetchLeverageBrackets`)

- Fetched via signed `GET /fapi/{version}/leverageBracket` (`primaryAPI`) during market loading
- Max leverage is from the first bracket (highest leverage tier) per symbol
- Default leverage: 10 if brackets unavailable

## Rate Limiting

Four API instances with different rate limits (`createAPIInstances`):

- `primaryAPI` (20 req/s): Account info, positions, orders, balance, user trades, leverage brackets
- `secondaryAPI` (5 req/s): Order placement, order cancellation
- `unlimitedAPI` (20 req/s): Time-sensitive operations (same config as primary)
- `publicAPI` (10 req/s): Market data, tickers, klines, depth, public trades

### Retry Logic (`createAPI`)

- 3 retries on network errors, 503, and 504 responses
- Delay: `retryCount * 1000` ms (1s, 2s, 3s)
- Uses `axios-retry` library

## State Reconciliation

### Order State

- **REST polling**: Fetched as a safety reconciliation roughly every 75
  seconds, not on every private health-loop tick.
- **WebSocket updates**: `ORDER_TRADE_UPDATE` events update individual orders in real-time
  - `NEW` orders are added via `addOrUpdateOrder`
  - Status changes update existing orders via `updateOrder`
  - cumulative partial/full fill quantities are mirrored onto the store row
    immediately
- **Debounced order repair**: `ORDER_TRADE_UPDATE` and `executionReport`
  schedule a 10-second debounced `fetchOrders()` refresh. This is separate
  from position repair and coalesces bursty order lifecycle events.
- **Reconnection**: `fetchAndSetPrivateSnapshot(10)` called on private WS reconnect

### Position State

- **REST polling**: Fetched as a safety reconciliation roughly every 25
  seconds, with a stale-response guard based on `positionStateVersion`
- **WebSocket updates**: `ACCOUNT_UPDATE` events provide immediate position changes
  - Updates existing positions if found in store
  - Handles position flips (removes opposite side in one-way mode)
  - Handles position close (removes all positions for symbol)
- **Coalesced position repair**: After a WS position update, schedules a
  1-second delayed `fetchPositions()` repair. The scheduler allows one
  pending timer and one in-flight repair; additional `ACCOUNT_UPDATE` frames
  coalesce, and stale REST responses are discarded when a newer WS position
  version arrived during the request.
- **Fake positions**: Created for all active non-spot markets with both Long and Short sides, 0 contracts, leverage from positionRisk or 0

### Balance State

- Fetched by the private health loop roughly every 25 seconds from the account
  endpoint (`/fapi/v4/account` on V1, `/fapi/v3/account` on V3)
- `ACCOUNT_UPDATE` is accepted only with a strictly numeric, increasing `E`,
  because Aster explicitly requires user-data consumers to order frames by
  event time. Missing, malformed, duplicate, and stale frames leave canonical
  state untouched.
- A standalone USDT update maps `wb`/`cw` to canonical `total/free/used` and
  applies position `up` deltas to `upnl`. Any non-USDT or known multi-asset
  state is retained only under `balance.currencies`; root totals remain
  last-known-good until one bounded account-endpoint reconciliation succeeds.
  That reconciliation is version-guarded so an older REST response cannot
  replace a newer account frame, and it never sums raw asset balances without
  Aster's conversion rates.

## Exchange Metadata

- **Position mode**: Global account setting fetched from `GET /fapi/{version}/positionSide/dual`; supports One-Way and Hedge.
- **Default margin type**: Cross (`MarginType.Cross`)
- **Position side in one-way mode**: `"BOTH"` - side determined from amount sign
- **Position side in hedge mode**: `LONG`/`SHORT` must be sent with new orders, and `reduceOnly` must not be sent.
- **Leverage**: Per-symbol (unlike Orderly which is account-level)
- **Supports changing position mode**: `changePositionMode(hedged)` via `POST /fapi/{version}/positionSide/dual`
- **Supports changing margin type**: `changeMarginType(isolated, symbol)` via `POST /fapi/{version}/marginType`
- **Timeframe mapping** (`AsterDEXExchange.fetchOHLCV`): Supports `1m, 3m, 5m, 15m, 30m, 1h, 2h, 4h, 6h, 12h, 1d, 1w`

## Testnet/Demo

| Component  | Production                          | Testnet                                      |
| ---------- | ----------------------------------- | -------------------------------------------- |
| REST API   | `https://fapi.asterdex.com`         | `https://fapi.asterdex-testnet.com`          |
| Public WS  | `wss://fstream.asterdex.com/stream` | `wss://fstream5.asterdex-testnet.com/stream` |
| Private WS | `wss://fstream.asterdex.com/ws/...` | `wss://fstream5.asterdex-testnet.com/ws/...` |

`ASTERDEX_ENDPOINTS` is the single source for REST and WS environment hosts;
the dedicated Aster clock uses the same REST constant.

## Known Quirks / Implementation Notes

- **BaseWebSocket lifecycle handlers are instance arrow fields — `getPrototypeOf`-based super calls silently no-op.** `BaseWebSocket.onClose` is a class-field arrow function (an instance property), not a prototype method, so `Object.getPrototypeOf(Object.getPrototypeOf(this)).onClose` is `undefined` and a guarded call skips the base reconnect logic entirely. This bug shipped: both AsterDEX WS classes used that trick in their `onClose` overrides, so sockets were permanently dead after any disconnect. The fix (and the required pattern, matching `bitmex.ws-private.ts`) is to retain the base instance handler in the constructor — `const baseOnClose = this.onClose; this.onClose = (event) => { ...asterdex cleanup...; baseOnClose(event); }`. `dispose()` IS a regular prototype method, so `super.dispose()` works from the arrow-field overrides.
- **Update order not supported**: AsterDEX has no PUT endpoint for orders. `updateOrder` cancels the old order and places a new one.
- **Bracket orders**: TP/SL attached to entry orders are placed sequentially as separate orders (non-atomic).
- **Close-position TP/SL**: `amount=0` uses `closePosition='true'` instead of `quantity`, and omits `reduceOnly`.
- **Trailing stops are intentionally disabled in metadata**: Aster requires a `callbackRate` (and supports an optional activation price), but the shared `PlaceOrderOpts` has only a generic price field. Do not re-enable the UI capability until the order input can encode the rate without guessing.
- **Native chase remains deliberately unwired**: the V3 chase placement
  response exposes a strategy ID but no underlying order ID. The shared
  chaser immediately tracks, reconciles, and cancels concrete order IDs, and
  Aster does not document a deterministic response mapping from the returned
  strategy/client-strategy ID to that order ID. Keep the existing client-side
  chaser until the full placement/cancel/fill lifecycle can be represented.
- **OTO/OCO/OTOCO strategy brackets remain deliberately unwired**: the V3
  schema lists `firstDriven*` and `secondDriven*` fields but does not define a
  normative TP/SL trigger graph or provide an OTOCO bracket example. Guessing
  those edges could leave an entry unprotected or both exits live, so attached
  TP/SL continues through the documented sequential path.
- **Agent registration remains an onboarding boundary**:
  `registerAndApproveAgent` must be signed by the user's owner wallet and
  requires explicit permission, expiry, chain, and optional withdrawal/IP
  choices. The exchange runtime holds the already-approved agent signer, not
  an owner-wallet consent surface, so it must not synthesize this request.
- **Guarded cancellation requires unavailable placement provenance**: Aster's
  guarded cancel contract requires the exact nonce originally used to place
  the order or batch. The request signer currently creates that nonce inside
  the Axios interceptor, the order response/store model does not retain it,
  and normal signing generates a fresh nonce for cancellation. Never route an
  order through `guardedCancelOrder`/`guardedBatchOrders` without durable,
  per-order placement-nonce provenance.
- **Unknown-order cancel is idempotent**: Aster returns `-2011 Unknown order sent`
  when canceling an order that has already disappeared. Treat this as a
  completed cancel and suppress the user-facing API error, especially for the
  open-position close path where successful market close is followed by stale
  TP/SL cleanup.
- **Builder reads are not account execution history**:
  `builder/userTrades` returns every approved user's activity under the
  authenticated builder identity, while `builder/approvedUserList` is builder
  administration data. Neither maps to `fetchHistoricalFills()` for the current
  account, and safe-cex has no builder-admin result surface, so these endpoints
  remain outside the exchange runtime rather than contaminating user history.
- **STP is available only through the typed Aster runtime surface**: the
  integration supports V3 account-global read/write and per-order overrides;
  there is no generic cross-exchange UI setting yet.
- **Orderbook snapshots fail closed**: `handleDepthSnapshot` consumes only documented partial-book streams. Every level must be an exact two-string pair; it drops boolean, numeric, array, object, malformed, zero-quantity, empty-side, or crossed frames rather than treating diffs as snapshots, trimming levels, or synthesizing a spread.
- **Orderbook registrations are counted, not deduplicated**: subscribing the same callback twice creates two independent registrations. Each returned unsubscribe removes only its own registration; the upstream stream closes only after the final registration is removed.
- **Historical trades**: served by `fetchHistoricalFills`; the separate `fetchHistoricalTrades` method was deleted 2026-08-16 after its normalized realized-PnL, hedge-leg and maker fields were ported across.
- **Virtual clock**: uses dedicated Aster clock with proxy fallback and V1/V3
  time endpoint fallback in browser windows, WebWorkers, and other non-window
  runtimes. A failed retry keeps the last good offset; lifecycle generations
  prevent both late offset writes and a late initial sync from installing an
  interval after `stop()` or disposal.
- **Orders placed individually**: no batch endpoint usage in `placeOrders`/`cancelOrders`; failures are tolerated per-item.
- **Trigger defaults**: TP defaults to `CONTRACT_PRICE` and SL defaults to `MARK_PRICE` (Binance-like behavior).
- **All signed params in query**: POST/PUT/DELETE signed requests use query parameters and clear request body.
- **Dual signing**: V1 routes use HMAC-SHA256; standard V3 routes use wallet signature with `user`, `signer`, and microsecond `nonce`; Aster Code routes use EIP-712 `Message.msg` query-string signing with `asterChain`.
- **Binance-style 200-with-error handling**: interceptor treats negative `code` values as API errors.
- **Ticker bid/ask source**: bid/ask are derived only from validated partial orderbook snapshots, not 24h ticker REST payload or diff frames.
- **Private WS repair is split by purpose**: `ACCOUNT_UPDATE` schedules
  delayed position-only repair with `positionStateVersion` stale-response
  protection. Non-USDT balance frames additionally schedule one debounced,
  `balanceStateVersion`-guarded account refresh. Order lifecycle events
  (`ORDER_TRADE_UPDATE` / `executionReport`) schedule a separate 10-second
  debounced order refresh. This prevents account-update bursts from stacking
  order, position, and balance REST calls on the periodic safety loop.
- **WS error forwarding**: public WS error events route through close/reconnect flow.
- **Position flip cleanup**: one-way mode updates remove stale opposite-side positions.
- **O(1) ticker lookup**: uses `tickersBySymbol` map instead of linear scans.
- **Fake position baseline leverage**: synthetic empty positions use positionRisk leverage or `0`; Rust stores these settings without empty position rows.

## Error Handling

### Timestamp Error Recovery (`createAPI` response interceptor)

- Error code `-1021` with "Timestamp" in message triggers automatic resync
- Resyncs time via `asterDEXVirtualClock.syncClientServerTime()`
- Retries the original request once (uses `__isRetryRequest` flag to prevent infinite loop)
- Strips stale signature params before retry (`timestamp`, `signature`, `recvWindow`, plus `user`/`signer`/`nonce` when present); pre-built Aster Code URLs are rebuilt from the stored unsigned params.

### HTTP Status Code Handling (`createAPI` response interceptor)

- `401`: Logs "Authentication failed. Check API credentials."
- `429`: Logs "Rate limit exceeded. Reducing request frequency."
- `503/504`: Logs "Service temporarily unavailable. Will retry." (also covered by axios-retry)

### Binance-style 200-with-error (`createAPI` response interceptor)

- Checks `response.data.code < 0` to detect errors returned with successful HTTP status
- Throws error with message: `AsterDEX API Error: <msg> (code: <code>)`

### Operation-level Error Handling

- Batch order placement: Continues placing remaining orders even if one fails; only throws if all fail (`AsterDEXExchange.placeOrders`)
- Batch cancel: Same pattern - continues on individual failure (`AsterDEXExchange.cancelOrders`)
- Startup: Positions and orders fetch failures are caught individually; startup continues (`AsterDEXExchange.start`)
- Tick errors: Caught at tick level, logged as warning, and the tick loop continues (`AsterDEXExchange.tick`)

## Tick Loop

- Runs as a 3-second private-state health loop, but most ticks do no REST
  work. It is a scheduler, not a full refresh loop.
- **Balance reconciliation**: roughly every 25 seconds via the account
  endpoint.
- **Position safety reconciliation**: roughly every 25 seconds via
  `positionRisk`, guarded by `positionStateVersion` so stale REST cannot
  overwrite newer WS position deltas.
- **Order safety reconciliation**: roughly every 75 seconds via
  `openOrders`. Faster order repair comes from the private WS debounce.
- Only runs if `!isPublicDataOnly` (includes V1 key/secret or valid V3 wallet auth).
- Open-interest polling remains separate from this private-state loop.
- Uses `loop()` utility for scheduling, which handles `isDisposed` check.

## Testing / Validation

`src/__tests__/audit-regressions/asterdex.orderbook-snapshot.test.ts` is the deterministic P0 regression for the partial-stream subscription, authoritative snapshot delivery, exact string-pair validation, independent duplicate registrations, and fail-closed invalid/diff handling.

`src/__tests__/audit-regressions/asterdex.trailing-stop-capability.test.ts` keeps the UI-facing trailing-stop capability disabled until a callback-rate input is added.

`src/__tests__/audit-regressions/asterdex.testnet-clock.test.ts` locks the
documented testnet REST/public/private WS hosts, worker-safe clock sync, and
the deferred initial-sync disposal race.

`src/__tests__/audit-regressions/asterdex.private-balance.test.ts` locks
canonical USDT ACCOUNT_UPDATE mapping, strict `E` ordering, malformed-frame
rejection, and non-USDT last-known-good root preservation plus stale-guarded
authoritative reconciliation.

`src/__tests__/audit-regressions/asterdex.v3-stp.test.ts` locks V3-only STP
account reads/writes, per-order override serialization, V1 rejection before
I/O, and malformed mode rejection.

`src/__tests__/audit-regressions/asterdex.builder-attribution.test.ts` locks the
Aster builder-fee resolver, V1 no-op behavior, V3 maker/taker attribution,
split/bracket/close-position propagation, Aster Code signed-query byte parity,
nonce monotonicity, and approval-status parsing.

## CCXT Reference

Aster is available in CCXT (`ts/src/aster.ts`). This implementation follows Binance Futures-style request/response patterns while preserving Aster-specific behaviors.

## Client order ID

This venue does not honor `PlaceOrderOpts.clientOrderId`: `placeOrder` / `placeOrders` reject any caller ID before a write (`packages/safe-cex/CLAUDE.md` → "Client order ID contract"). Keep that guard as the first statement of any new placement override.

## Rust engine port

`sdk-rs/venues/asterdex` implements the sans-I/O runtime for `engine: 'rust'`.
It shares identical Binance-family wire records, not Binance socket/depth/auth
or order-lifecycle policy. Public books retain the documented depth20 snapshot
contract. Native URLs carry socket roles; host transports apply account routing.

- Rust signs only through the injected provider at dispatch. Trusted setup
  (`exchanges/rust/asterdexSignerSetup.ts`, native
  `sdk-rs/registry/src/asterdex_setup.rs`) resolves the API version from the
  material's shape, sends it with the public signer addresses, and hands key
  material only to the provider. V3 wallet keys are trimmed hex; V1 secrets
  stay byte-exact. See `docs/plans/RUST_ASTER_SIGNER_ADMISSION.md`.
- Both V1 and V3 are supported. The exact standard-V3 ABI/EIP-191 versus Code
  EIP-712 split above is preserved, with independent ethers signature vectors.
- Browser-hosted signing (`signHttpRequest`) supports V1 only, through
  `asterdex.signing.ts`'s standalone V1 request codec, which the TS venue also
  uses. The host admits an account only when setup resolved an explicit V1, so
  V3 wallet keys never become HMAC keys. Hosted WS auth is refused; Aster uses a
  listen key. Hosted stamps come from the host's shared clock, not
  `asterDEXVirtualClock`.
- The wrapper forwards builder policy and public auth addresses. The read
  agent is a provider slot; with an `auth.privateKey` wallet it signs with the
  primary wallet's ref. Reads use the read agent and its secondary route;
  writes and listen keys keep the primary agent.
- Private reconciliation commits each domain atomically and rejects stale
  snapshots, including mode reads invalidated by later mode-write acknowledgements.
  Unique native executions emit even when their order-state payload is older.
  Order fills retain native execution identities/prices; terminal
  cancellation cannot overwrite a filled order. Typed currency balances cross
  the binary frame alongside aggregate balances.
- Conditional, split and bracket placements remain sequential. Amendments
  preflight before cancel and stop if the original fills during cancellation or
  cancellation is unconfirmed. Unknown-order is idempotent only for ordinary
  cancellation. Caller symbols and bounded ACK provenance keep newly placed
  orders cancelable before reconciliation; no placeholder Store rows are made.
  Caller client IDs and trailing placement remain explicitly unsupported.
- V3 STP uses typed account read/write commands and per-order overrides.
  Browser owner-wallet builder approval and agent onboarding stay outside the
  exchange engine; ordinary placement never synthesizes owner consent.
- Rust fixes TS's hedge-zero event behavior by closing only the reported leg,
  and returns historical candle timestamps in the shared seconds contract.
  Configured leverage reads use actual account settings instead of the inherited
  market-maximum fallback. These differences and conservative amend cancellation
  are recorded in the harness known-fix notes.

Port plan, phase reviews and gate evidence: `docs/plans/RUST_ASTERDEX.md`.

Rust accepts absent ACCOUNT_UPDATE entry price only on an explicitly zero
position amount; malformed present entry values and unproven exposure remain
atomic rejections. Bounded fill history pages native seven-day windows and
normalizes cumulative PnL once across deduplicated symbol/execution IDs before
filtering order IDs or invalid sizes. V3's orderId overrides time search, so
bounded order-specific queries filter locally after normalization. Saturated
single-millisecond reads, incomplete walks and later-page failures return errors,
never plausible partial histories. Unbounded query behavior is unchanged.
