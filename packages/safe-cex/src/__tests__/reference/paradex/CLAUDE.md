# Paradex Exchange Integration

This directory contains the Paradex exchange integration for Tealstreet. Paradex is a decentralized perpetual futures exchange built on Starknet, offering self-custody cross-margin trading.

## Exchange Information

- **Type**: Decentralized exchange (Starknet L2)
- **Signing**: StarkNet typed data signatures (EIP-712 style)
- **Margin**: Cross-margin only in this integration (the venue's isolated-account routing is not represented safely yet)
- **Position Mode**: One-way (no hedge mode)
- **Quote Currency**: USD
- **Status**: Enabled (`enabled: true`, `supportsTestnet: true`)

## API Documentation

- **Official Documentation**: https://docs.paradex.trade/
- **Getting Started**: https://docs.paradex.trade/api/general-information/api-quick-start
- **Authentication**: https://docs.paradex.trade/api/general-information/authentication
- **REST API Reference**: https://docs.paradex.trade/api/prod/
- **WebSocket API**: https://docs.paradex.trade/ws/general-information
- **Rate Limits**: https://docs.paradex.trade/api/general-information/rate-limits
- **Create Order**: https://docs.paradex.trade/api/prod/orders/new
- **Batch Orders**: https://docs.paradex.trade/api/prod/orders/batch
- **Batch Cancellation**: https://docs.paradex.trade/api/prod/orders/cancel-batch
- **Order Instructions**: https://docs.paradex.trade/trading/orders/order-instructions
- **WebSocket Error Handling**: https://docs.paradex.trade/ws/general-information/error-handling
- **CCXT REST**: https://github.com/ccxt/ccxt/blob/master/ts/src/paradex.ts
- **CCXT WebSocket**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/paradex.ts

### SDK References

- **JavaScript SDK** (patched): `/thirdparty/paradex.js` — Package `@tealstreet/paradex-sdk` (PR #40 for Starknet RPC v0_8)
- **Python SDK** (upstream reference, not in repo): https://github.com/tradeparadex/paradex-py
- **Python SDK Docs**: https://tradeparadex.github.io/paradex-py/

**IMPORTANT**: When implementing or debugging Paradex features, consult the upstream Python SDK (https://github.com/tradeparadex/paradex-py) for WebSocket formats, REST structures, and authentication flows. Note: There is no local copy of the Python SDK in this repository (`/python/` only contains `ws-proxy`).

## File Structure

| File                    | Description                                                                              |
| ----------------------- | ---------------------------------------------------------------------------------------- |
| `paradex.api.ts`        | REST API client with JWT authentication, axios interceptors, retry logic                 |
| `paradex.exchange.ts`   | Main exchange class (`ParadexExchange`) extending `BaseExchange`; all trading operations |
| `paradex.types.ts`      | Type definitions: URLs, endpoints, order/side/status maps, response types                |
| `paradex.sdk.ts`        | SDK integration: account init, `ParadexRequestSigner`, `ParadexJWTManager`               |
| `paradex.ws-public.ts`  | Public WebSocket: BBO tickers, full-depth books, trades, OHLCV polling                |
| `paradex.ws-private.ts` | Private WebSocket: orders, positions, balance updates with JWT auth                      |

Private position events are coalesced into a throttled REST reconciliation. The
returned full snapshot is committed with `addOrUpdatePositionsFull()` after a
disposal guard; a thrown fetch leaves the last-known-good store untouched.

## Authentication

Paradex uses a complex multi-layer authentication model combining StarkNet signatures, JWT tokens, and subkey delegation.

### Credential Fields

- **`key`** (stored in `options.key`): Subkey private key (StarkNet private key, 66 chars with `0x` prefix) or Ethereum private key
- **`paradexAddress`** (stored in `options.paradexAddress`): Main Paradex account address on StarkNet
- **`subkeyAddress`** (stored in `options.subkeyAddress`): Subkey public address for delegated signing

Runtime mode guardrails:

- Private trading mode requires `key + paradexAddress + subkeyAddress`
- Paradex does **not** require `secret`; do not rely on the BaseExchange default `secret`-based private/public mode check

From `exchangeMetadata`:

- `requiresParadexAddress: true`
- `requiresNoSecret: true` — Secret field is not used
- `requiresPassphrase: false`
- `requiresApplicationId: false`

### SDK Account Initialization (`paradex.sdk.ts:41-110`)

Two authentication modes based on key length:

**Subkey Authentication** (key length === 66):

1. Create StarkNet `Account` directly with `paradexAddress` as address and `key` as private key
2. Uses `starknet` library's `Account` class with Paradex RPC node URL from config

**Ethereum Authentication** (key length !== 66):

1. Create `Wallet` from Ethereum private key via `ethers`
2. Wrap wallet with `SDK.Signer.ethersSignerAdapter()`
3. Derive account via `SDK.Account.fromEthSigner()`

The SDK is lazy-loaded (`paradex.sdk.ts:16-21`) to avoid loading if not needed.

### Request Signing (`ParadexRequestSigner`, `paradex.sdk.ts:152-341`)

**Note**: The `signRequest` method is **dead code** -- it is never called in production. All REST API authentication uses JWT Bearer tokens exclusively (via `authenticateRequest()` in `paradex.api.ts`). The `ParadexJWTManager.generateNewToken()` method performs its own signing through `signParadexTypedData()` with a different TypedData structure (5-field `Request` type with method/path/body/timestamp/expiration); it does NOT call `signRequest`. The `signRequest` method remains in the codebase but is unused.

The `signRequest` method's approach (for reference only -- not used at runtime):

1. Build message: `method + timestamp + path + body`
2. If message > 31 characters, hash via `starknetKeccak()` (`paradex.sdk.ts:186`)
3. Sign using StarkNet typed data with domain `{name: 'Paradex', version: '1', chainId: config.paradexChainId}`
4. Headers: `PARADEX-STARKNET-ACCOUNT`, `PARADEX-STARKNET-SIGNATURE`, `PARADEX-TIMESTAMP` (seconds)

### `payloadSigner`

With `options.payloadSigner` set, no Starknet private key enters the process. `key` carries the subkey PUBLIC key, and `initializeParadexSDK` returns an account stub holding only `paradexAddress`, before the 66-character private-key check. `signParadexTypedData()` passes the typed-data message hash (`starknet` `typedData.getMessageHash`, the same hash `Account.signMessage` signs) to the signer with `scheme: 'stark'`, and places its `signature` verbatim where the serialised `["r","s"]` goes. Any headers it returns go on the same request: the JWT auth fetch, the order POST, TP/SL POSTs and the amendment PUT. A batch of more than one order is refused, because a delegated signer produces one signature per request. `signRequest` signs only with a local key. Tests: `src/__tests__/paradex.payload-signer.test.ts`.

The Rust engine supports the same payload signer contract. It emits a signing
action containing the message hash and public key, then resumes the held HTTP
request only after a signature event. Native hosts use
`NetworkTransport::with_payload_signer`; JS hosts receive the account's
`payloadSigner` callback. Signing and HTTP share one deadline; failures and late
answers cannot send unsigned orders. JWT generation follows TS's direct raw
fetch path. `remoteSigner` remains the TS API-client adapter contract: by itself
it cannot replace JWT's StarkNet signing; use a local key or `payloadSigner` for
that step. Signed protocol-relative URLs retain TS adapter semantics: browser
HTTP inherits the page scheme, Node HTTP rejects them, and HOW concatenates the
API base URL. The routing host exposes its cached `httpMode`; do not resolve
routing a second time or treat all three adapters alike.

The JS host validates signer header names and rejects CR/LF before encoding
`name:value` lines for WASM. Values keep their exact bytes, including colons and
surrounding whitespace; empty values and no headers are valid. The core also
validates delegated headers for native callers before issuing HTTP.

**Order Signing** (`paradex.sdk.ts:242-340`):

1. Generate `signature_timestamp` as `Date.now()` (milliseconds, used as nonce)
2. Convert side to chain format: `"1"` for BUY, `"2"` for SELL
3. Scale size and price by `1e8` (quantum values) following Python SDK's `chain_size()`/`chain_price()`
4. Convert `paradexChainId` string to hex via byte encoding (matches Python's `int_from_bytes`)
5. Sign typed data with `Order` primary type containing: timestamp, market, side, orderType, size, price
6. Return `{signature: JSON.stringify([r, s]), signature_timestamp, headers?}`

### JWT Token Management (`ParadexJWTManager`, `paradex.sdk.ts:349-547`)

JWT tokens are used for both REST API Bearer auth and WebSocket authentication.

- **Token validity**: 5 minutes (`TOKEN_VALIDITY_MS`)
- **Refresh buffer**: 2 minutes before expiry (`REFRESH_BUFFER_MS`)
- **Refresh deduplication**: `refreshPromise` prevents concurrent token generation (`paradex.sdk.ts:392-393`)
- **Generation endpoint**: `POST /auth/{subkeyAddress}` with signature headers
- **Signature for auth**: TypedData with `Request` type containing method, path, body, timestamp (seconds), expiration (24 hours)
- **Required headers**: `PARADEX-STARKNET-ACCOUNT` (main account), `PARADEX-STARKNET-SIGNATURE`, `PARADEX-TIMESTAMP`, `PARADEX-SIGNATURE-EXPIRATION`

**Key timing detail**: JWT auth timestamps use Unix **seconds** (`Math.floor(Date.now() / 1000)`), while order signing timestamps use **milliseconds** (`Date.now()`).

### REST API Authentication (`paradex.api.ts:82-126`)

Axios request interceptor flow:

1. **Public endpoints** (`/trades`, `/funding`, `/summary`, `/markets` excluding `/account`): Skip auth
2. **Signature-header requests** (e.g., onboarding): Skip JWT to avoid deadlock during SDK init
3. **Order placement**: Adds `PARADEX-PARTNER: TEALSTREET` header (`paradex.api.ts:105-108`)
4. **All other private requests**: `authenticateRequest()` obtains JWT via manager and sets `Authorization: Bearer <token>`

### Private WebSocket Authentication (`paradex.ws-private.ts`)

1. Wait for SDK initialization
2. Get JWT token from manager
3. Send JSON-RPC 2.0 auth message: `{jsonrpc: "2.0", method: "auth", params: {bearer: token}, id: <trackedRequestId>}`
4. Auth response: Result `{node_id: "…"}` (or legacy `{}`) on success; response id is matched against the tracked auth request id
5. On successful auth, subscribe to private channels

## Startup Sequence

The `start()` method (`paradex.exchange.ts:189-282`) proceeds as follows:

1. **Wait for SDK init**: If `sdkInitPromise` is in progress, await it
2. **Fetch markets** (sequential): `GET /markets` → filter active perpetual futures only (`asset_kind: PERP` / `*-PERP`; exclude options and spot) → store
3. **Fetch tickers** (sequential): `GET /markets/summary?market=ALL` → filter active perpetual futures only (`*-PERP`) → store
4. **Connect public WebSocket**: Subscribe to `bbo.{symbol}` + `trades.{symbol}` for filtered perps and `markets_summary`; orderbook stays on-demand
5. **Connect private WebSocket** (if `jwtManager` available): Auth + subscribe to orders/positions/balance
6. **Fetch leverage settings**: `GET /account/margin` → populate `leverageSettings` map
7. **Initial tick**: Fetch positions → fetch balance → calculate uPnL from positions → store
8. **Fetch orders**: `GET /orders` → filter open statuses → store
9. **Start tick loop**: `tick()` → positions + balance + orders + leverage refresh → `loop(tick, 10_000)`

**Notes**:

- Markets and tickers are fetched sequentially (not parallel).
- The tick loop interval is 10 seconds.
- Private account initialization should continue even when ticker cache is temporarily empty (do not early-return from startup before private tick begins).

## REST API Endpoints

### Authentication

| Endpoint         | Method | Description                    |
| ---------------- | ------ | ------------------------------ |
| `/auth`          | POST   | Get JWT token (base auth)      |
| `/auth/{pubkey}` | POST   | Get JWT token with subkey auth |

### Account

| Endpoint                        | Method | Description                                                            |
| ------------------------------- | ------ | ---------------------------------------------------------------------- |
| `/account`                      | GET    | Account info (account_number, total_collateral, free_collateral, etc.) |
| `/onboarding`                   | POST   | Account onboarding                                                     |
| `/account/margin`               | GET    | Get all margin/leverage configs                                        |
| `/account/margin/{market}`      | POST   | Set leverage for a market                                              |
| `/account/profile/market_max_slippage/{market}` | POST   | Set market-scoped max slippage tolerance                               |

### Markets

| Endpoint               | Method | Description                               |
| ---------------------- | ------ | ----------------------------------------- |
| `/markets`             | GET    | List all markets                          |
| `/markets/summary`     | GET    | Tickers (use `?market=ALL`)               |
| `/orderbook/{market}`  | GET    | Orderbook (with `?depth=N`)               |
| `/markets/klines`      | GET    | OHLCV klines (public, limited intervals)  |
| `/tradingview/history` | GET    | TradingView OHLCV (supports 1D, 4H, etc.) |

### Orders

| Endpoint                           | Method | Description                                                         |
| ---------------------------------- | ------ | ------------------------------------------------------------------- |
| `/orders`                          | GET    | List open orders                                                    |
| `/orders-history`                  | GET    | List historical orders (execution fallback)                         |
| `/orders`                          | POST   | Place order (with signature)                                        |
| `/orders/batch`                    | POST   | Place 1-10 independently signed LIMIT orders                        |
| `/orders/batch`                    | DELETE | Cancel an exact locally bounded pair by ID                          |
| `/orders`                          | DELETE | Cancel all orders (optional `?market=` filter)                      |
| `/orders/{id}`                     | DELETE | Cancel specific order                                               |
| `/orders/by_client_id/{client_id}` | DELETE | Cancel by client ID                                                 |
| `/fills`                           | GET    | Private fills history (`fetchHistoricalFills` primary; cursor-paginated) |

### Positions

| Endpoint              | Method | Description                      |
| --------------------- | ------ | -------------------------------- |
| `/positions`          | GET    | Get all positions                |
| `/positions/{market}` | GET    | Get position for specific market |

Execution-history behavior (`fetchHistoricalFills`):

- Uses authenticated `/fills` with cursor pagination for primary execution history
- Falls back to `/orders-history` rows where filled size is positive (`size - remaining_size > 0`)
- Normalizes symbol/side/timestamp and returns Unix-second execution timestamps
- Private history auth is retryable: `fetchHistoricalOrders` and `fetchHistoricalFills`
  retry SDK/JWT readiness briefly, then throw `ParadexPrivateHistoryAuthNotReadyError`.
  Do not convert auth-not-ready into `[]`, because ingestion would complete the
  coverage window as genuinely empty and never revisit it.

## WebSocket Implementation

### Public WebSocket (`paradex.ws-public.ts`)

**Protocol**: JSON-RPC 2.0 control messages and binary SBE market/account data.
Both public and private URLs explicitly negotiate `sbeSchemaId=1&sbeSchemaVersion=2`.
The live venue rejects a JSON-only handshake. `paradex.sbe.ts` decodes the published
schema 1:2 with checked lengths, mandatory fields and enum validation; malformed
frames leave state untouched. The engine's host must preserve raw binary frames
and deliver them through the binary ingress; text control messages remain text.
Routing uses the account's existing Tealstreet resolver. Routed Paradex requests
carry proxy credentials in `Tealstreet-Authorization`, preserving the venue's JWT
`Authorization: Bearer …` header.

Paradex-only WASM builds retain `wsBinary` for SBE and the stable `wsMessage`
entry for JSON controls. The optional text byte/empty-action entries are enabled
by the other venues through the `text-ingress` feature; mixed/default builds
keep them. The host selects its existing text fallback when those entries are
absent, keeping the binary feed and account routing unchanged.

**Connection URL**: `wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2`

**Keepalive**: The venue uses RFC6455 control pings; socket libraries answer with
control pongs automatically ([official keepalive contract](https://github.com/tradeparadex/paradex-docs/blob/main/fern/pages/websockets/introduction.mdx)).
Application JSON pings are rejected (including
JSON-RPC `ping`, which is not an API method). SBE heartbeat template 40 refreshes
liveness during idle periods. The stale-message watchdog reconnects after 90 seconds.

**Subscription format**:

```json
{
  "id": <messageId>,
  "jsonrpc": "2.0",
  "method": "subscribe",
  "params": { "channel": "<channel_name>" }
}
```

**Channel routing** (`paradex.ws-public.ts:72-113`):

- Subscription updates arrive as `{method: "subscription", params: {channel, data}}`, where `params.data` may be a single object or an array (handler normalizes both)
- Channel name is split on `.` to extract type and symbol (e.g., `bbo.BTC-USD-PERP` -> type=`bbo`, symbol=`BTC-USD-PERP`)
- Handlers registered in `messageHandlers` map by channel type

**Subscribed channels on connect** (`paradex.ws-public.ts`):

- `bbo.{symbol}` — broadcast for every market; near-real-time best bid/ask/index updates
- `trades.{symbol}` — broadcast for every market; live last-price updates
- `markets_summary` — single aggregate stream for slow-changing fields (volume_24h, open_interest, funding_rate)
- Orderbook channels are **not** subscribed on connect; only when `listenOrderBook()` is called

| Channel                                               | Format    | Description                                                                                    |
| ----------------------------------------------------- | --------- | ---------------------------------------------------------------------------------------------- |
| `bbo.{market}`                                        | Automatic | Best bid/ask/index per symbol (every state update is applied)                                     |
| `trades.{market}`                                     | Automatic | Live last-price + drives in-memory OHLCV update from trade ticks                               |
| `markets_summary`                                     | Automatic | Aggregate stream for volume_24h / open_interest / funding_rate (slow cadence)                  |
| `order_book.{market}.deltas` | On-demand | Full-depth initial snapshot, then incremental inserts/updates/deletes |

**BBO handling** (`paradex.ws-public.ts`):

- Applies every message synchronously, including consecutive different markets
- Updates bid/ask/index in ticker store from per-symbol `bbo.{market}` events
- Last price is **not** updated from bbo — that comes from the trades feed

**Markets summary handling** (`paradex.ws-public.ts`):

- Applies every message synchronously, including consecutive different markets
- Used as a **secondary** source for volume / open-interest / funding rate updates only.
- Bid/ask/last/mark/index updates come from `bbo` and `trades`, not from `markets_summary`,
  because `markets_summary` does not push at tick-rate cadence and would visibly lag the
  watchlist and positions PnL.
- Handles either single-object or array payload variants in `params.data`

**Orderbook handling** (`paradex.ws-public.ts:190-247`):

- Applies every delta synchronously; callbacks are throttled separately per symbol to 100ms
- Full-depth snapshots and deltas use `inserts`, `updates`, `deletes` arrays
- Each level: `{side: "BUY"|"SELL", price: string, size: string}`
- Validates the complete update before mutation; a gap resubscribes and awaits a fresh snapshot
- Retains the sequence floor during recovery so stale snapshots cannot rewind the book
- Emits the complete sorted book to registered callbacks
- On-demand subscription: `subscribeToOrderBook()` / `unsubscribeFromOrderBook()`
- Resubscribes active orderbooks on reconnect (`paradex.ws-public.ts:396-403`)

**Trades handling** (`paradex.ws-public.ts:249-297`):

- Updates `last` and `mark` price from trade price (with channel-symbol fallback when trade payload omits `market`)
- Calls `updateOHLCVFromTrade()` for all active OHLCV subscriptions for the symbol

**OHLCV implementation** (`paradex.ws-public.ts:494-604`):

- **Hybrid approach**: REST polling + trade-based real-time updates
- Uses documented `/markets/klines` for 1m through 1h; wider intervals temporarily retain the public `/tradingview/history` compatibility route
- Polls every 3 seconds (`paradex.ws-public.ts:578`)
- Between polls, `updateOHLCVFromTrade()` constructs/updates candles from trade feed
- Poll replies cannot replace a newer candle bucket or decrease its known volume.
  A revision fence also retains trades received while a poll was in flight.
  The REST candle has no observation timestamp, so equal-bucket, equal-volume
  replies cannot prove whether their close is newer than a cached trade.
- Only emits callback when candle data actually changes
- `pollKey` format: `"{symbol}:{interval}"` used to track active subscriptions
- Cleanup function stops polling, clears callbacks, clears cache

**Max slippage auto-set**: When `listenOrderBook()` or `listenOHLCV()` is first called, automatically sets that market's max slippage to 20% via `/account/profile/market_max_slippage/{market}`.

### Private WebSocket (`paradex.ws-private.ts`)

**Protocol**: JSON-RPC 2.0 control messages and binary SBE market/account data.
Both public and private URLs explicitly negotiate `sbeSchemaId=1&sbeSchemaVersion=2`.
The live venue rejects a JSON-only handshake. `paradex.sbe.ts` decodes the published
schema 1:2 with checked lengths, mandatory fields and enum validation; malformed
frames leave state untouched. The engine's host must preserve raw binary frames
and deliver them through the binary ingress; text control messages remain text.
Routing uses the account's existing Tealstreet resolver. Routed Paradex requests
carry proxy credentials in `Tealstreet-Authorization`, preserving the venue's JWT
`Authorization: Bearer …` header.

**Connection URL**: Same as public (`wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2`)

**Authentication**: JWT bearer token sent via `{method: "auth", params: {bearer: token}}` immediately on open.

**Ping/pong**: `{ type: "ping" }` every 30 seconds (`paradex.ws-private.ts:405-415`).

**Subscribed channels** (after auth success, `paradex.ws-private.ts:358-390`):

- `orders.ALL` — All order updates across all markets
- `positions` — Position updates
- `balance_events` — Balance/equity updates

**Order handler** (`paradex.ws-private.ts:128-182`):

- Maps order fields to internal format
- For stop orders, uses `trigger_price` as display price
- Emits cancel reason as error notification when present (with user-friendly formatting via `formatCancelReason`)
- Always calls `addOrUpdateOrder()` to update store
- Triggers throttled `fetchOrders()` (1-second debounce)
- Known cancel reasons mapped: `ORDER_SIZE_BELOW_MIN`, `INSUFFICIENT_MARGIN`, `PRICE_OUT_OF_RANGE`, `POST_ONLY_WOULD_CROSS`, etc.

**Position handler** (`paradex.ws-private.ts:204-260`):

- Skips closed positions or zero-size positions
- Creates position with `PositionMode.OneWay` and `MarginType.Cross`
- Triggers throttled `fetchPositions()` (500ms throttle, `paradex.ws-private.ts:287-318`)
- Throttle prevents excessive fetches: tracks `lastPositionFetchTime` and `hasPendingPositionEvents`

**Balance handler** (`paradex.ws-private.ts:262-285`):

- Handles payload shape variants (`data`, `data.data`, array wrappers) and maps balance fields with fallback precedence:
  - `total_collateral` → `account_value` → `equity` → `margin_balance` → `wallet_balance`
  - `free_collateral` → `free` → `available_margin` → `available_collateral`
  - `initial_margin_requirement` → `position_margin` → `used_margin` → `max(total - free, 0)`
- uPnL set to 0 (calculated from positions elsewhere)

## Order Types and Mappings

### Internal -> API Type Mapping

| Internal Type                | API Type             |
| ---------------------------- | -------------------- |
| `OrderType.Market`           | `MARKET`             |
| `OrderType.Limit`            | `LIMIT`              |
| `OrderType.Conditional`      | `STOP_LIMIT`         |
| `OrderType.StopLoss`         | `STOP_MARKET`        |
| `OrderType.TakeProfit`       | `TAKE_PROFIT_MARKET` |
| `OrderType.TrailingStopLoss` | Rejected             |

### API -> Internal Type Mapping

| API Type             | Internal Type          |
| -------------------- | ---------------------- |
| `MARKET`             | `OrderType.Market`     |
| `LIMIT`              | `OrderType.Limit`      |
| `STOP_MARKET`        | `OrderType.StopLoss`   |
| `STOP_LIMIT`         | `OrderType.StopLoss`   |
| `STOP_LOSS_MARKET`   | `OrderType.StopLoss`   |
| `STOP_LOSS_LIMIT`    | `OrderType.StopLoss`   |
| `TAKE_PROFIT_MARKET` | `OrderType.TakeProfit` |
| `TAKE_PROFIT_LIMIT`  | `OrderType.TakeProfit` |

The REST and private-WebSocket mappers share `ORDER_TYPE` and
`isParadexTriggerOrderType` from `paradex.types.ts`. Generic `STOP_LIMIT` and
`STOP_MARKET` rows without `REDUCE_ONLY` map to `OrderType.Conditional`; the
same types with `REDUCE_ONLY` remain protective stop-loss orders. Keep all
documented trigger variants in those definitions so readback displays
`trigger_price` and native amendment retains the correct trigger family.

### Order Status Mapping

| API Status    | Internal Status        |
| ------------- | ---------------------- |
| `NEW`         | `OrderStatus.Open`     |
| `OPEN`        | `OrderStatus.Open`     |
| `PENDING`     | `OrderStatus.Open`     |
| `UNTRIGGERED` | `OrderStatus.Open`     |
| `FILLED`      | `OrderStatus.Closed`   |
| `CANCELED`    | `OrderStatus.Canceled` |
| `REJECTED`    | `OrderStatus.Canceled` |

### Time in Force Mapping

| Input                       | Paradex Value         |
| --------------------------- | --------------------- |
| `GoodTillCancel` / `GTC`    | `GTC`                 |
| `ImmediateOrCancel` / `IOC` | `IOC`                 |
| `PostOnly` / `POST_ONLY`    | `POST_ONLY`           |
| `GTT`                       | `GTC` (fallback)      |
| `FOK`                       | `IOC` (closest match) |

## Order Lifecycle

### Order Placement (`paradex.exchange.ts:845-972`)

1. **Generate identity**: Create one validated `ts_${uuid()}` `client_id` (non-empty suffix, at most 64 characters) before signing
2. **Sign order**: `requestSigner.signOrder()` generates typed data signature with timestamp nonce
3. **Map order type**: Convert internal `OrderType` to Paradex API type; opening conditionals use `STOP_LIMIT`
4. **Determine prices**: Opening conditionals require separate finite positive `price` (execution) and `triggerPrice`; protective stops keep `opts.price` as the trigger price
5. **Map time in force**: Convert to Paradex instruction (`GTC`, `IOC`, `POST_ONLY`)
6. **Set safety attributes**: TP/SL types get `REDUCE_ONLY`; opening conditionals do not. Main and native-batch orders explicitly use Paradex's documented default `stp: EXPIRE_TAKER`
7. **Build payload**: `{market, side, type, size, price?, trigger_price?, instruction, flags?, stp, signature, signature_timestamp, client_id}`
8. **POST /orders**: Submit order and accept only a non-empty canonical Paradex `id` whose echoed `client_id` exactly matches the request
9. **TP/SL bracket orders**: If `takeProfit` or `stopLoss` specified, place as separate orders (`placeTakeProfitStopLoss`)

The strict identity contract applies to the main single order. Existing
attached TP/SL child placement remains sequential after the confirmed main
order and retains its established partial-success behavior.

### TP/SL Orders (`paradex.exchange.ts:974-1076`)

- Uses `TAKE_PROFIT_MARKET` and `STOP_LOSS_MARKET` order types
- Opposite side from main order (to close position)
- Always has `REDUCE_ONLY` flag
- Each TP/SL order is independently signed
- TP/SL prices are validated: TP above entry for longs / below for shorts, SL inverse
- Requires explicit order size (no "infinite" TP/SL that closes entire position)
- If TP/SL placement fails, main order is NOT cancelled (error swallowed with warning log)

### Order Update (`ParadexExchange.updateOrder`)

The integration uses native `PUT /orders/{order_id}` modification and never
cancels or places a replacement from `updateOrder()`:

- Exact reconciled open `LIMIT` and `UNTRIGGERED` TPSL shapes are supported.
- The raw upstream side, type, market, stable ID, execution price, total base
  size, filled/remaining size, reduce-only state, and instruction/post-only
  state must reconcile before signing.
- TPSL price edits change `trigger_price` while preserving the exact upstream
  `STOP_*` / `TAKE_PROFIT_*` type and any separate limit execution price.
- TIF/post-only, reduce-only, and STP are not fields in Paradex's modify
  request; keeping the same ID preserves them server-side. They must never be
  synthesized into the modify payload.
- Triggered TPSL, market, isolated-margin, stale/mismatched, and otherwise
  ambiguous shapes reject before signing or writing. There is no
  cancel-and-replace fallback.

### Order Cancellation

- **Single and one-order tail**: `DELETE /orders/{id}`
- **Multiple**: `DELETE /orders/batch` with Axios request data
  `{order_ids: [firstId, secondId]}` for exact two-ID chunks only
- **All**: `DELETE /orders`
- **By symbol**: `DELETE /orders?market={symbol}`

The official batch-cancel schema publishes no maximum. Its concrete request
example demonstrates two order IDs, so the integration deliberately adopts
only exact pairs and never invents a larger venue limit. An odd final order
stays on the established single-cancel endpoint.

Each batch response must be an object containing a `results` array that
accounts for every requested ID exactly once. Results may be reordered and are
correlated only by `id`; missing, duplicate, and unknown identities fail
closed. `QUEUED_FOR_CANCELLATION` and `ALREADY_CLOSED` are the only successful
statuses. A correlated `NOT_FOUND` is an explicit rejection. Because the
official schema says `id` is present only when an order is found, an
identity-less `NOT_FOUND` remains uncorrelated and is reported as malformed
with the exact missing requested-ID set—never paired by response position.

`cancelOrders()` attempts every pair and final tail sequentially even when an
earlier attempt fails. After all requested orders have been attempted, it
rethrows a sole failure unchanged or throws one aggregate error containing
every failed chunk's exact IDs and cause. It does not silently swallow a
partial cancellation failure.

### Batch Order Placement (`ParadexExchange.placeOrders`)

Contiguous ordinary `LIMIT` orders use the official `POST /orders/batch`
boundary in chunks of 1-10. The whole local chunk is preflighted, assigned a
unique `client_id`, and independently signed before the first network write.
Successful rows are correlated exclusively by their echoed `client_id`, so a
reordered response still returns Paradex order IDs in input order.

Paradex publishes batch errors only as `{error, message}` without a client ID,
request index, or response-order guarantee. If any risk-checked row fails, the
adapter therefore throws one aggregate error containing the exact missing
client-ID set and all decoded upstream errors; it never guesses a one-to-one
pairing or returns partial IDs. Missing, duplicate, unknown, or count-mismatched
response identities fail closed.

Only documented LIMIT instruction combinations are batched: GTC, POST_ONLY,
and IOC. MARKET remains on the existing single-order path because Paradex's
instruction matrix says GTC cannot accompany MARKET and IOC is LIMIT-only,
while the legacy single mapper defaults MARKET to GTC. Trigger/conditional,
attached TP/SL, trailing, close, unsupported-TIF, and otherwise composite
orders also retain their dedicated sequential paths so no child or semantic
attribute is dropped.

### Roadmap feature boundaries

- Opening trigger orders are implemented through the existing safe-cex
  `price + triggerPrice` shape as non-reduce-only `STOP_LIMIT` requests. Missing
  prices, reduce-only routing, attached TP/SL children, and trailing orders
  reject before signing or writing.
- Custom STP selection is not exposed by `PlaceOrderOpts`; placements therefore
  pin the venue's documented `EXPIRE_TAKER` default. Do not infer another STP
  mode from TIF or order side.
- RPI, interactive/VWAP flags, and the newer named TPSL limit families do not
  have lossless generic safe-cex write shapes. Readback remains supported, but
  writes stay schema-gated rather than overloading unrelated fields.
- Paradex isolated orders require `on_behalf_of_account` and can coexist with a
  main-account position for the same market. The current order input and
  symbol-keyed position/margin selection do not carry that routing identity,
  so metadata remains cross-only and isolated writes/amends fail closed.
- The official WebSocket trading schema currently exposes only LIMIT, MARKET,
  STOP_LIMIT, and STOP_MARKET while REST exposes eight order families. Trading
  remains on REST until the adapter can choose the transport without dropping
  a type/flag and the authenticated request lifecycle can be validated.

## Symbol Format

Paradex uses hyphenated `BASE-QUOTE` format:

- Examples: `BTC-USD-PERP`, `ETH-USD-PERP`, `SOL-USD-PERP`
- Market ID equals the symbol string (no separate numeric ID)
- Perpetual options are filtered out (symbols ending in `-C` or `-P`)

## Data Precision & Conversion

### Market Precision

From market API response (`paradex.exchange.ts:420-474`):

- `precision.amount`: From `step_size` or `order_size_increment`
- `precision.price`: From `tick_size` or `price_tick_size`
- `limits.amount.min/max`: From `min_order_size` / `max_order_size`
- `limits.leverage.max`: Calculated as `Math.floor(1 / IMF)` where IMF comes from `delta1_cross_margin_params.imf_base` or `initial_margin_fraction`

### Order Signing Quantum Values (`paradex.sdk.ts`)

For order signatures, size and price are scaled by `1e8`:

- Use deterministic decimal math (BigNumber), not `parseFloat`/`Math.floor`, to avoid IEEE754 drift
- `chainSize = decimal(size).multipliedBy(1e8).integerValue(ROUND_DOWN)`
- `chainPrice = decimal(price).multipliedBy(1e8).integerValue(ROUND_DOWN)` (or `"0"` for market orders)
- Native amendments use the distinct official `ModifyOrder` typed-data shape,
  which adds the stable order `id`; never reuse the placement `Order`
  signature for `PUT /orders/{order_id}`.

### Balance Calculation (`paradex.exchange.ts`)

- REST `/account` parsing is defensive across payload variants (`data` wrapper, account/equity style fields, collateral style fields)
- `total`: prefers `total_collateral`, falls back to `account_value`/`equity`
- `free`: prefers `free_collateral`, falls back to `free`/`available_margin`
- `used`: prefers `initial_margin_requirement`, falls back to `position_margin`, then `max(total - free, 0)`
- `upnl`: Calculated from positions via `sumBy(positions, 'unrealizedPnl')` in tick loop

### OHLCV Timestamps

- `/markets/klines` uses Unix **milliseconds** for `start_at` / `end_at` parameters and returns millisecond candle timestamps.
- `/tradingview/history` uses Unix **seconds** for `from` / `to` parameters and returns second candle timestamps, which are converted to milliseconds internally.

## Rate Limiting

### REST API

- **Global rate limit**: `maxRPS: 10` via `axios-rate-limit` (`paradex.exchange.ts:71`)
- **Network retry**: 3 retries on network errors with linear backoff (1s, 2s, 3s) via `axios-retry` (`paradex.api.ts:73-77`)
- Current published REST limits vary by endpoint; keep the client-side 10 RPS ceiling unless the official rate-limit reference is re-verified during a deliberate tuning pass.

### WebSocket

- **BBO/markets summary handlers**: Apply every message synchronously; a global throttle would discard different markets
- **Orderbook handler**: Every delta is applied; callbacks are throttled per symbol to 100ms (leading+trailing). Invalid updates or sequence gaps resubscribe and wait for a fresh snapshot; duplicate/older sequence numbers are ignored.
- **Order fetch throttle**: 1-second debounce (`paradex.ws-private.ts:430-435`)
- **Position fetch throttle**: 500ms minimum between fetches (`paradex.ws-private.ts:287-318`)

### Tick Interval

- **Tick loop**: 10 seconds (`paradex.exchange.ts:323`)
- **OHLCV polling**: 3 seconds (`paradex.ws-public.ts:578`)

## State Reconciliation

### Position Management

- **One-way mode**: Single position per symbol (Long or Short)
- **Fake positions**: Created for all active markets with zero contracts, using stored leverage or default 10x (`paradex.exchange.ts:731-746`)
- **Merge strategy**: Actual positions overwrite fake positions in a `Map<symbol, Position>` (`paradex.exchange.ts:755-766`)
- **WebSocket updates**: Position handler creates/updates individual positions (`addOrUpdatePosition`)
- **REST refresh**: Tick loop fetches all positions and replaces store

### Leverage Tracking

- `leverageSettings` map (`Map<string, number>`) caches per-market leverage (`paradex.exchange.ts:66`)
- Populated from `GET /account/margin` on startup and every tick
- Used for fake positions when API leverage is 0 or unavailable
- On `setLeverage()`: Optimistically updates local positions immediately, creates fake position if none exists

### Order Reconciliation

- REST `fetchOrders()` filters by open statuses: `NEW`, `OPEN`, `PENDING`, `UNTRIGGERED` (`paradex.exchange.ts:642`)
- WebSocket handler calls `addOrUpdateOrder()` for all status changes
- Throttled REST fetch (1s) triggered after each WebSocket order event
- Cancel reason emitted as user notification via `formatCancelReason()`

### Balance Reconciliation

- REST balance from `/account` is normalized defensively across collateral-style and equity-style payloads
- WebSocket `balance_events` rows are normalized with the same field-fallback precedence as REST
- uPnL calculated from positions in tick loop, not from balance API

## Exchange Metadata

| Setting                                     | Value                                         |
| ------------------------------------------- | --------------------------------------------- |
| `enabled`                                   | `true`                                        |
| `supportsTestnet`                           | `true`                                        |
| `defaultQuote`                              | `USD`                                         |
| `marginTypes`                               | `[Cross]`                                     |
| `positionModes`                             | `[OneWay]`                                    |
| `leverageIsGlobal`                          | `false`                                       |
| `marginTypeIsGlobal`                        | `true`                                        |
| `positionModeIsGlobal`                      | `true`                                        |
| `requiresCors`                              | `false`                                       |
| `forbidCors`                                | `false`                                       |
| `enforceWsTunnel`                           | `true`                                        |
| `tickInterval`                              | `500` ms (metadata) / `10_000` ms (tick loop) |
| `orderbookTickDelay`                        | `100` ms                                      |
| `orderPendingUpdateDelay`                   | `1000` ms                                     |
| `stopOrderPendingUpdateDelay`               | `1000` ms                                     |
| `leverageMagnets`                           | `[1, 5, 10, 20, 50, 100]`                     |
| `supportsChaser`                            | `true`                                        |
| `supportsPartialStops`                      | `true`                                        |
| `supportsInfinityStops`                     | `false`                                       |
| `supportsFeeRates`                          | `false`                                       |
| `supportsTrailingStopLoss`                  | `false`                                       |
| `supportsOpeningTriggerOrders`              | `true`                                        |
| `supportsReadOnlyKey`                       | `false`                                       |
| `supportsClosedPnLEndpoint`                 | `false`                                       |
| `tvMaxGetBarFetches`                        | `10`                                          |
| `candleApiCountLimit`                       | `2000`                                        |
| `disconnectWarningThreshold`                | `5000` ms                                     |
| `tvIntraDayMultipliers`                     | `['1', '5', '15', '30', '60', '120', '240']`  |
| `tvHasDaily`                                | `true`                                        |
| `tvHasWeeklyAndMonthly`                     | `false`                                       |
| `alwaysCalculateUnrealizedPnlFromLastPrice` | `true`                                        |
| `latencyDomain`                             | `api.prod.paradex.trade`                      |
| `defaultTradingViewSymbol`                  | `BTC-USD-PERP`                                |
| `TP/SL trigger types`                       | `MarkPrice`, `LastPrice`                      |

## Testnet / Demo

### URLs

- **REST Livenet**: `https://api.prod.paradex.trade/v1`
- **REST Testnet**: `https://api.testnet.paradex.trade/v1`
- **WebSocket Livenet**: `wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2`
- **WebSocket Testnet**: `wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2`

### Differences

- Chain ID differs between environments (fetched from SDK config)
- SDK config endpoint differs: `SDK.Config.fetchConfig('testnet')` vs `SDK.Config.fetchConfig('prod')`
- Testnet credentials at: https://testnet.paradex.trade/
- `src/exchanges/paradex/` is runtime-only. Fresh deterministic regressions belong in `src/__tests__/audit-regressions/`; do not restore credentialed/manual exchange-tree tests.

## Order history (`GET /orders-history`)

`fetchHistoricalOrders` reads this endpoint with cursor pagination. Mapper:
`paradex.order-history.ts`, pure and exported. Measured live over 7 days: 301 orders,
**184 of which never filled (61.1%)** — the highest never-filled share of any venue in
the epic, and entirely invisible to `/fills`.

Two shape facts, both verified live and both silent failures if assumed away:

- **`status` carries no information here — every row reads `CLOSED`.** 100/100 rows on a
  live account came back `CLOSED` whether they filled or were rejected outright, because
  `/orders-history` _is_ the terminal set (open orders live on `/orders`). The fill/cancel
  distinction comes from **`cancel_reason`**; a mapper that trusts `status` marks every
  cancellation as a completed order.
- **There is no `filled` field.** It is `size - remaining_size`, the same derivation the
  live order path uses. An unfilled row carries `remaining_size === size` and
  `avg_fill_price: ""`.

`reduce_only` is not a top-level boolean either — it appears as `"REDUCE_ONLY"` inside the
`flags` array.

`start_at` / `end_at` **are** honoured (a window the account has no orders in returns
`results: []`). Worth stating only because four other venues in this epic answered a
windowed request with their full history and no error.

History and income continue through empty pages when `next` names an unvisited
cursor; only cursor exhaustion or the existing page cap completes traversal.

## Known Quirks / Implementation Notes

1. **Native amend is intentionally fail-closed**: `updateOrder()` supports only
   exact reconciled open LIMIT and untriggered TPSL orders. Triggered TPSL,
   isolated-margin, and ambiguous/stale shapes reject without a write; do not
   restore cancel-and-replace as a fallback.

2. **No infinite TP/SL**: Paradex requires explicit order size for TP/SL orders. Zero-sized "close entire position" TP/SL is not supported (`paradex.exchange.ts:978-985`).

3. **TP/SL always REDUCE_ONLY**: All stop order types (`STOP_LIMIT`, `TAKE_PROFIT_MARKET`, `STOP_LOSS_MARKET`, etc.) automatically get the `REDUCE_ONLY` flag; Paradex rejects them without it (`paradex.exchange.ts:927`).

4. **FOK mapped to IOC**: Paradex does not support Fill-or-Kill; it is mapped to IOC as the closest match (`paradex.exchange.ts:1407`).

5. **Max slippage auto-set**: Orderbook/OHLCV listeners set per-market max slippage to 20% before market-order emulation.

6. **OHLCV not on WebSocket**: Paradex does not support OHLCV/kline streaming via WebSocket. Implementation uses REST polling every 3 seconds combined with trade-based candle construction (`paradex.ws-public.ts:494`).

7. **Proxy 200-with-error pattern**: Paradex responses may return HTTP 200 with error in body when proxied. Response interceptor checks `response.data.error` and converts to rejection (`paradex.api.ts:131-147`).

8. **PARADEX-PARTNER header**: Added to all order placement POST requests for tracking (`paradex.api.ts:105-108`).

9. **Perp-only market filtering**: Paradex returns perps, spot, and hundreds of option markets. Keep only active perpetual futures (`asset_kind: PERP` / symbols ending `-PERP`) and filter out option suffixes (`-C`/`-P`) so public WebSocket startup stays below Paradex's per-connection subscription cap.

10. **Leverage on zero positions**: Paradex does not return leverage for zero-size positions. The `leverageSettings` map stores leverage separately and is used as fallback (`paradex.exchange.ts:65-66`).

11. **Cancelled order UI lag**: Known issue (see `TODO.md`) where cancelled orders from WebSocket do not immediately update the UI; they disappear only after the next `fetchOrders()` poll.

12. **SDK init deadlock prevention**: Request interceptor checks for existing signature headers before waiting for SDK init, avoiding deadlock when the interceptor runs during SDK initialization itself (`paradex.api.ts:112-117`).

13. **Lazy-loaded exchange**: Registered in `exchanges/registry.ts` with dynamic import and Webpack chunk name `exchange-paradex`.

14. **Mark/last/bid/ask sources**: bid/ask come from `bbo.{symbol}` (broadcast at startup). `last` (and a synthetic `mark`) come from `trades.{symbol}` (also broadcast). `markets_summary` is reserved for slow-changing fields (volume_24h / open_interest / funding_rate). Do **not** route bid/ask/last back through `markets_summary` — its cadence is too slow for live UX, and an earlier attempt to consolidate everything into it visibly broke watchlist/positions PnL refresh.

15. **Orderbook on-demand subscription**: Orderbook channels are only subscribed when `listenOrderBook()` is called, preventing subscription to 100+ unused orderbooks. `order_book.{market}.deltas` preserves full depth; the former `snapshot@15@100ms` feed truncated books to fifteen levels. A new socket or recovery requires a snapshot before any delta applies. The sequence check conservatively resubscribes on a nonconsecutive sequence rather than risk publishing an incomplete book. Official schema: `tradeparadex/paradex-docs`, `fern/apis/prod_ws/asyncapi-2.6.0.yaml`, `ResponsesPriceBookUpdate`.

16. **Public/private mode detection**: Paradex must not be treated as public-only just because `secret` is empty. Runtime private-mode checks must use Paradex credentials (`key`, `paradexAddress`, `subkeyAddress`).

17. **Batch errors have no row identity**: `POST /orders/batch` success rows
    echo `client_id`, but published error rows contain only `error` and
    `message`. Correlate successes by generated client identity and report
    failures as an unpaired aggregate missing-ID set; never infer error order.

18. **Single main-order responses are identity checked**: Every
    `placeMainOrder()` generates the same bounded `ts_${uuid()}` identity used
    by native placement batches before signing. `POST /orders` must return a
    non-empty canonical `id` and echo that exact `client_id`; missing or
    mismatched identity fails closed before attached child placement begins.

19. **Batch cancellation uses a conservative local bound**: Paradex documents
    `DELETE /orders/batch` but publishes no maximum request size. Use exact
    two-ID chunks because that is the demonstrated request shape, keep a
    one-ID tail on the single endpoint, and strictly correlate every response
    row by order `id`. All chunks are attempted before any collected failure is
    propagated.

20. **Opening conditionals are exact STOP_LIMIT orders**: Require distinct
    positive execution and trigger prices, omit `REDUCE_ONLY`, and classify
    readback by its reduce-only state. Never degrade a trailing order to a
    static stop or attach TP/SL children before the opening trigger fills.

## Error Handling

### API Error Extraction (`paradex.api.ts:129-169`)

Two-layer error handling:

1. **Success response with error body**: `response.data.error` detected, formatted as `"Paradex Error: {error}: {message}"`, converted to rejection
2. **HTTP error response**: `error.response.data.error` extracted, enhanced error message created with `Object.defineProperty` to override read-only `message`

### Specific Error Patterns

- **JWT not initialized**: `"JWT manager not initialized"` thrown if authenticated request attempted before SDK init (`paradex.api.ts:27`)
- **Empty JWT token**: `"JWT token is empty"` thrown if token generation returns empty string (`paradex.api.ts:36`)
- **Missing credentials**: Specific error messages for missing `paradexAddress`, `subkeyAddress`, or `key` (`paradex.exchange.ts:169-178`)
- **SDK init failure**: Re-thrown to stop exchange initialization (`paradex.exchange.ts:124`)

### Order Error Handling (`paradex.exchange.ts:949-971`)

- API error response extracted: checks `error.response.data.error` and `error.response.data.message`
- Error emitted via `emitAndLogError()` for user notification
- For TP/SL failures: error is caught and logged as warning, main order is preserved (`paradex.exchange.ts:1056-1073`)

### Cancel Reason Formatting (`paradex.ws-private.ts:187-202`)

User-friendly messages for known cancel reasons:

- `ORDER_SIZE_BELOW_MIN` -> "Order size below minimum required"
- `INSUFFICIENT_MARGIN` -> "Insufficient margin/balance"
- `POST_ONLY_WOULD_CROSS` -> "Post-only order would execute immediately"
- `SELF_TRADE_PREVENTION` -> "Self-trade prevention triggered"
- Unknown reasons: underscores replaced with spaces, lowercased

## Testing / Validation

Fresh deterministic regressions belong in the package-level
`src/__tests__/audit-regressions/` directory. Do not commit credentialed or
network-dependent test scripts beside exchange runtime code.

`paradex.native-amend.test.ts` locks the official REST body, distinct
`ModifyOrder` signature, stable response identity, partial-fill total-base
sizing, exact trigger-type preservation, flags-based reduce-only readback, and
all no-cancel/no-place failure boundaries.

`paradex.batch-orders.test.ts` locks 10-order chunking, exact LIMIT-only
eligibility, independent signing, client-ID correlation under reordered
successes, preflight/signing atomicity, mixed-family routing, aggregate
unidentified error handling, and malformed-response rejection.

`paradex.opening-trigger.test.ts` locks the STOP_LIMIT signing/request shape,
separate execution and trigger prices, non-reduce semantics, explicit default
STP, readback classification, capability metadata, and every no-write
preflight boundary.

### Known Test Gaps (from TODO.md)

- Short positions: Not thoroughly tested
- TP/SL orders: Trigger price handling, WebSocket updates for triggered orders
- Cancelled order UI responsiveness

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `client_id` (single and native batch), rule 1-64 characters, in place of the generated `ts_…` ID. Close orders (`close: true`) are not an honorable shape.


## Rust public discovery resilience

Rust catalogue/ticker reads retry through the shared engine capped exponential
backoff. A malformed typed public row is skipped with a warning while valid
neighbours remain usable. Private account initialization starts once the usable
catalogue is available, without waiting for REST ticker seeds. Account snapshots,
mutation acknowledgements and signing validation retain their stricter rules.
See `docs/plans/RUST_STARTUP_AUDIT.md` and the native startup resilience regressions.

Summary ticker numbers accept null as zero as well as empty strings, matching
TS's optional-field defaults. Mixed nullable bid/ask/funding fields retain valid
last/mark prices on REST and JSON summaries. Instrument, book and signed/private
required numbers do not inherit that null coercion.

Rust JSON position snapshots and updates accept an empty string or null for optional
`leverage` and `liquidation_price`, matching TS's `value || '0'` default. Zero
leverage uses the existing cached setting or 10x fallback; liquidation defaults
to zero. Nonempty malformed values and booleans remain invalid; position size/PnL
still reject null and empty strings. Account margin and depth
numbers retain strict validation. Binary position decoding is unchanged.

## Rust socket supervision

Both Rust sockets use `engine::socket::Supervisor`: a 10-second connection deadline and capped exponential reconnect delay with jitter that survives short open/close flaps. Validated idle limits remain 60 seconds public and 90 seconds private; private authentication and subscription replies have 15-second deadlines. Native SBE heartbeat template40 and host RFC6455 control frames prove liveness. Rust sends no application JSON ping. Public per-book resynchronization and subscription retry timers remain independent.

Rust bounded history reads that exhaust their page budget emit
`history.incomplete` with the actual query command before partial completion.
This lets the host flag and notify only the owning caller. Unbounded cap and
other cursor-stall paths retain their existing failure behavior.


## Native REST order-book query

Rust supports the public SDK fetchOrderBook with an independent unsigned
GET /orderbook/{encoded native symbol}?depth=N on production/testnet, default20
and explicit caller depth forwarded without clamping. No market seed, private
credentials or subscription is required. As TS does, native side order,
duplicate prices and zero quantities survive, the first two string tuple fields
are price/base size and extra metadata is ignored; each total is price*amount.
There are no level IDs, cumulative totals, decimal scaling or update IDs.

Both side arrays are required; present empty/one-sided books succeed. Malformed
objects/tuples/finances and quote-total overflow fail the complete query. REST
metadata never changes live SBE/JSON books, sequence/readiness or account/JWT
state. HTTP/native/transport failures stay command-owned typed NotSent and
explicitly non-auth, even for public credential-looking refusals. Only structured
venue reasons are retained, with bounded usable HTTP429 Retry-After hints and no
automatic retry or discovery/account startup. The standalone family enables its
precise JSON book-command decoder. Existing market-order quote admission and
private write/JWT/signing policies remain separate.
