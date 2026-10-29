# GRVT (Gravity) Exchange Integration

This directory contains the GRVT exchange integration for Tealstreet.

## Exchange Information

GRVT (Gravity) is a hybrid cryptocurrency derivatives exchange built on ZK Stack (zkSync Hyperchain). This adapter exposes perpetuals:

- Perpetual futures trading
- On-chain settlement with off-chain orderbook
- Session-based authentication with API keys
- EIP-712 signed orders for trustless verification
- The upstream protocol enumerates Perpetual, Future, Call and Put; current SDK discovery requests perpetuals only
- Builder fee program (Tealstreet integration)
- Full and Lite API formats (compact field names for performance)

### Supported product boundary

`fetchMarkets()` posts `{}` to `all_instruments`. The official API defaults
omitted/empty `k` to `PERPETUAL`; it does not mean all kinds. The generic mapper
will accept injected Future/Call/Put rows, and native-ID ticker/book/order
plumbing has no additional kind guard. These latent paths are not supported
non-perpetual product lifecycles: signing metadata drops kind/expiry/strike.
Exact-symbol cancellation's snapshot safeguards do not enable new products.
Read-only production and
testnet probes on 2026-10-20 returned only perpetuals for `{}` and no rows for
explicit `FUTURE/CALL/PUT` filters.

The Rust cutover decision retires this unexposed mapper-only surface, retaining
perpetuals and all TS sources. It does not declare GRVT incapable of offering
those products. See [the product decision and evidence](../../../../../docs/plans/GRVT_PRODUCTS_DECISION.md).
Future enablement needs real listings, typed expiry/strike/kind/settlement
metadata through consumers and product-specific lifecycle verification.

## API Documentation

**Official Documentation**: https://api-docs.grvt.io/

**Local API Documentation** (cached reference; check the official docs above
for current protocol behavior before making an integration change):

- **Market Data API**: [./docs/market_data_api.md](./docs/market_data_api.md)
- **Market Data Streams**: [./docs/market_data_streams.md](./docs/market_data_streams.md)
- **Trading API**: [./docs/trading_api.md](./docs/trading_api.md)
- **Trading Streams**: [./docs/trading_streams.md](./docs/trading_streams.md)
- **Source**: https://github.com/gravity-technologies/api-spec/tree/main/artifacts/apidocs

### Key Documentation Links

1. **API Setup Guide**: https://api-docs.grvt.io/api_setup/
2. **Trading API**: https://api-docs.grvt.io/trading_api/
3. **Market Data API**: https://api-docs.grvt.io/market_data_api/
4. **Trading WebSocket Streams**: https://api-docs.grvt.io/trading_streams/
5. **GitHub Repository (JS Client)**: https://github.com/gravity-technologies/grvt-js-client
6. **GitHub Repository (Python SDK)**: https://github.com/gravity-technologies/grvt-pysdk
7. **NPM Package**: `@grvt/client` (forked as `@tealstreet/grvt-client`)

## File Structure

### Core

| File                 | Purpose                                                                                                                                                                                                                                                                                           |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `grvt.exchange.ts`   | Main exchange class (1852 lines). Authentication, order placement with EIP-712 signing, balance/position/order fetching, leverage management, tick loop                                                                                                                                           |
| `grvt.api.ts`        | Three Axios instances: MDG (public market data), Edge (authentication), TDG (private trading). Session cookie management, auto-reauthentication on error 1000                                                                                                                                     |
| `grvt.types.ts`      | TypeScript types, enums, endpoint constants, WebSocket message interfaces. Defines both full and lite API format interfaces                                                                                                                                                                       |
| `grvt.signing.ts`    | **Legacy / mostly dead.** Only `validatePrivateKey` is imported (by `grvt.exchange.ts`). The `signOrder` / `generateNonce` / `generateExpiration` exports are unused — real order signing goes through `GrvtSigner.sign()` / `.decode()` from `@tealstreet/grvt-ts-sdk` (see `grvt.exchange.ts`). |
| `grvt.builder.ts`    | Builder fee authorization. Browser wallet signing for builder authorization, proxy-based API calls, check/authorize builder status                                                                                                                                                                |
| `grvt.ws-public.ts`  | Public WebSocket: ticker (delta), orderbook (snapshot), candles. JSON-RPC 2.0 protocol                                                                                                                                                                                                            |
| `grvt.ws-private.ts` | Private WebSocket: orders, positions, fills. Connects through Tealstreet proxy with session cookie authentication                                                                                                                                                                                 |

### Other

- `docs/` - Local API documentation reference (cached from GitHub)
- `SETUP.md` - Additional setup instructions

`src/exchanges/**` is production runtime code only. Keep credential utilities,
manual probes, and debugging scripts out of this directory. Derive and verify a
signer address with trusted wallet tooling outside the repository; never add a
package-local script that reads `.apikey.json`.

## Authentication

GRVT uses a multi-layer authentication system:

### Session-Based Authentication (`grvt.exchange.ts:235-270`)

1. **API Key Login**: `POST /auth/api_key/login` to Edge Gateway
   - Sends `{ api_key: <key> }` with `X-Grvt-Remember-Me: true` header
   - Edge gateway returns session cookie (`gravity=...`) and `X-Grvt-Account-Id` header
2. **Session Extraction** (`grvt.api.ts:134-179`):
   - Browser: Cookie extracted from proxy header `X-Ts-Grvt-Session` (browser cannot read `Set-Cookie` due to CORS)
   - Node.js: Cookie extracted from `Set-Cookie` header directly
   - Account ID extracted from `X-Grvt-Account-Id` response header
3. **Subsequent Requests**: Session cookie and account ID attached to all TDG requests via request interceptor (`grvt.api.ts:247-276`)

### Auto-Reauthentication (`grvt.api.ts:330-384`)

When a TDG request fails with error code `1000` (session expired):

1. Clears existing session cookie and account ID
2. Clears SDK cached cookie
3. Calls `exchange.authenticate()` to get new session
4. Retries the failed request with new credentials
5. Uses `_retry` flag on config to prevent infinite loops
6. Replaces both cookie and proxy-session headers and reconnects an authenticated private socket with the new session

### EIP-712 Order Signing (`grvt.exchange.ts:1050-1126`)

Every order must be cryptographically signed:

1. **Wallet Initialization** (`grvt.exchange.ts:92-124`): `ethers.Wallet` created from `opts.secret` (private key). Address validated against `opts.grvtSignerAddress` if provided.
2. **Domain Configuration**: `{ name: "GRVT Exchange", version: "0", chainId }` where chainId is 326 (testnet) or 325 (production)
3. **EIP-712 Message Structure** (`grvt.exchange.ts:1083-1106`):
   - `subAccountID`: Integer trading account ID
   - `isMarket`: Boolean
   - `timeInForce`: Numeric enum (1=GTT, 2=AON, 3=IOC, 4=FOK)
   - `legs[].assetID`: Instrument hash as hex string (e.g., `"0x030501"`)
   - `legs[].contractSize`: `size * 10^baseDecimals` (integer)
   - `legs[].limitPrice`: `price * 1e9` (PRICE_MULTIPLIER constant)
   - `legs[].isBuyingContract`: Boolean
   - `nonce`, `expiration`: Random nonce and 1-hour expiration in nanoseconds
4. **Builder Fee**: When authorized, uses `OrderWithBuilderFee` type template instead of `Order`, adding `builder` and `builderFee` fields
5. **Signing**: Uses `GrvtSigner.sign()` from `@tealstreet/grvt-ts-sdk`, then `GrvtSigner.decode()` to extract r, s, v

### Configuration Fields

- **`key`**: API key from GRVT UI
- **`secret`**: Ethereum private key (hex, with or without `0x` prefix)
- **`testnet`**: Boolean for environment selection
- **`subaccount`**: Sub-account ID (passed as `grvtTradingAccountId`)
- **`grvtSignerAddress`**: Optional validation address
- **`grvtTradingAccountId`**: Trading account ID for sub-account queries

### Three Axios Instances (`grvt.exchange.ts:130-132`)

| Instance           | Base URL                    | Auth           | Purpose            |
| ------------------ | --------------------------- | -------------- | ------------------ |
| `xhr` (MDG)        | `market-data.{env}.grvt.io` | None           | Public market data |
| `edgeXhr` (Edge)   | `edge.{env}.grvt.io`        | API key login  | Authentication     |
| `tradingXhr` (TDG) | `trades.{env}.grvt.io`      | Session cookie | Private trading    |

All three are rate-limited to 10 RPS and have 5 network error retries.

## Startup Sequence

Defined in `start()` at `grvt.exchange.ts:369-451`:

```
1. authenticate() [if session cookie not present]
   -> POST /auth/api_key/login to Edge Gateway
   -> Extracts session cookie and account ID from response headers
2. fetchMarkets() -> POST /lite/v1/all_instruments (sequential)
3. store.update({ markets, loaded.markets: true })
4. publicWebsocket.connectAndSubscribe() (non-blocking)
   -> Connects to WSS, subscribes to all ticker delta streams in batches
6. fetchTickers() -> creates placeholder tickers from markets (sequential)
   -> store.update({ tickers, loaded.tickers: true })
6. fetchBalance() -> POST /lite/v1/aggregated_account_summary (sequential)
   -> Auto-detects grvtTradingAccountId from sub_account_summaries if not set
   -> store.update({ balance, loaded.balance: true })
7. updateMarketLeverageLimits(markets) [requires auth, sequential]
   -> only after the trading account is configured or auto-detected
   -> getAllInitialLeverage() via SDK TDG client and updates each max
   -> a failure preserves the existing placeholder limits and startup continues
8. fetchBuilderAuthorization() [background, non-blocking]
   -> checkGrvtBuilderAuthorized() via proxy API
9. privateWebsocket.connectAndSubscribe() [after fetchBalance]
   -> Connects through Tealstreet proxy
   -> Sends auth message, then subscribes to v1.order, v1.position, v1.fill
10. fetchPositions() -> SDK TDG positions (sequential)
    -> Merges real positions with fake positions for empty markets
    -> store.update({ positions, loaded.positions: true })
11. fetchOpenOrders() -> SDK TDG openOrders (sequential)
    -> store.update({ orders, loaded.orders: true })
12. tick() -> starts 60-second balance polling loop
```

Each fetch step has try/catch that sets loaded flag even on failure.

## REST API Endpoints

All endpoints are defined in `grvt.types.ts:23-51`.

### Market Data Gateway (MDG) - Public

| Endpoint                   | Method | Purpose               |
| -------------------------- | ------ | --------------------- |
| `/lite/v1/all_instruments` | POST   | Get all instruments   |
| `/lite/v1/instrument`      | POST   | Get single instrument |
| `/lite/v1/ticker`          | POST   | Get full ticker       |
| `/lite/v1/mini`            | POST   | Get mini ticker       |
| `/lite/v1/book`            | POST   | Get orderbook         |
| `/lite/v1/trade_history`   | POST   | Get trade history     |
| `/lite/v1/kline`           | POST   | Get candlestick data  |
| `/lite/v1/funding`         | POST   | Get funding rate data |

### Edge Gateway - Authentication

| Endpoint              | Method | Purpose                         |
| --------------------- | ------ | ------------------------------- |
| `/auth/api_key/login` | POST   | Authenticate and obtain session |

### Trading Data Gateway (TDG) - Private

| Endpoint                              | Method | Purpose                        |
| ------------------------------------- | ------ | ------------------------------ |
| `/lite/v1/account_summary`            | POST   | Get sub-account summary        |
| `/lite/v1/aggregated_account_summary` | POST   | Get aggregated account summary |
| `/lite/v1/positions`                  | POST   | Get current positions          |
| `/lite/v1/open_orders`                | POST   | Get open orders                |
| `/lite/v1/create_order`               | POST   | Create a single order          |
| `/lite/v1/cancel_order`               | POST   | Cancel specific order          |
| `/lite/v1/cancel_all_orders`          | POST   | Cancel all orders              |
| `/lite/v1/order_history`              | POST   | Get order history              |
| `/lite/v1/fill_history`               | POST   | Get fill/trade history         |
| `/lite/v1/get_all_initial_leverage`   | POST   | Get leverage settings          |
| `/lite/v1/set_initial_leverage`       | POST   | Set leverage for instrument    |
| `/full/v1/authorize_builder`          | POST   | Authorize builder              |
| `/full/v1/get_authorized_builders`    | POST   | Check authorized builders      |

### Grafana Monitoring (`grvt.types.ts:53-55`)

- **Every-tick endpoints**: `CREATE_ORDER`, `CANCEL_ORDER`, `CANCEL_ALL_ORDERS`
- **Periodic endpoints**: `ACCOUNT_SUMMARY`, `POSITIONS`, `OPEN_ORDERS`

### API Format: Full vs Lite

GRVT offers two response formats:

- **Full** (`/full/v1/`): Long-form camelCase field names
- **Lite** (`/lite/v1/`): Shortened single/two-character field names (used by this implementation)

The code handles both formats throughout with fallback patterns like `feed.i || feed.instrument`.

## WebSocket Implementation

### Public WebSocket (`grvt.ws-public.ts`)

**Protocol**: JSON-RPC 2.0

**Connection**: `wss://market-data.{env}.grvt.io/ws/full`

**On Open** (`grvt.ws-public.ts:41-48`):

1. Tracks last received
2. Re-subscribes all existing subscriptions (reconnect support)
3. Creates ticker subscriptions only for markets without an existing logical
   topic; queued batches repeat the same check, so reconnect cannot duplicate
   ticker topics. Batch timers are retained and invalidated on reconnect,
   ticker unsubscribe, and `dispose()`, so stale work cannot recreate state or
   send after shutdown. `dispose()` removes the market listener and delegates
   socket shutdown to `BaseWebSocket.dispose()`.

**Ticker Subscriptions** (`grvt.ws-public.ts:61-88`):

- Subscribes to `v1.ticker.d` (delta stream) for every market
- Batched in groups of 20 with 100ms stagger between batches
- Refresh rate: 500ms per instrument

**Subscription Format**:

```json
{
  "jsonrpc": "2.0",
  "id": <requestId>,
  "method": "subscribe",
  "params": {
    "stream": "v1.book.s",
    "selectors": ["BTC_USDT_Perp@500-50"]
  }
}
```

**Selector Format**:

- Orderbook: `{instrument}@{refreshMs}-{depth}` (e.g., `BTC_USDT_Perp@500-50`)
- Candles: `{instrument}@{interval}-{type}` (e.g., `BTC_USDT_Perp@CI_1_M-TRADE`)
- Ticker: `{instrument}@{refreshMs}` (e.g., `BTC_USDT_Perp@500`)

**Streams Handled**:

- `v1.book.s`: Orderbook snapshots -> `handleOrderbookUpdate()`
- `v1.candle`: Candlestick data -> `handleKlineUpdate()`
- `v1.ticker.s` / `v1.ticker.d`: Full ticker snapshot / delta -> `handleTickerUpdate()`

**Ticker Update Logic** (`grvt.ws-public.ts:325-434`):

- Delta stream (`v1.ticker.d`) only sends changed fields
- Fields parsed: mark price, index price, last price, best bid/ask, funding rate, open interest, volume, 24h change
- Uses O(1) `store.marketsById` hashmap lookup
- Fallback: if no `last` price but `mark` is present, uses mark as last for unpopulated tickers

**Orderbook**: Subscribes to `v1.book.s` (snapshot mode, not delta). Each update is a full orderbook state.

**Candle Close -> Ticker Update**: When a candle close price is received, the ticker's `last` price is also updated (`grvt.ws-public.ts:479`).

**Error Code 1100**: "Stream not supported on this gateway" errors are silently ignored (`grvt.ws-public.ts:171`).

**Reconnection**: `resubscribe()` re-sends all stored subscriptions from the `subscriptions` Map.

### Private WebSocket (`grvt.ws-private.ts`)

**Connection**: Through Tealstreet proxy at `wss://fsjnpy143pbz.jp.tealstreet.io/grvt` (hardcoded, `grvt.ws-private.ts:33`).

**Authentication Flow** (`grvt.ws-private.ts:69-92`):

1. On WebSocket open, sends auth message to proxy:
   ```json
   {
     "type": "auth",
     "session_cookie": "rm=true; gravity=<sessionCookie>",
     "account_id": "<accountId>",
     "testnet": true/false
   }
   ```
2. Proxy responds with `{ type: "auth_success" }`
3. Then subscribes to private streams

If the proxy rejects authentication or GRVT returns official error `1000`
(authentication required), the socket refreshes the Edge session once and
reconnects because proxy auth must be the first frame. `auth_success` is only
transport readiness: it does **not** reset that budget. A private-feed payload
that remains healthy for five minutes is the sole reset point. A failed refresh
or another auth error before that bounded healthy session force-closes the
private socket, clears its timers/socket reference, and emits a redacted,
actionable error; it must never continue with the stale cookie.

**Subscriptions** (`grvt.ws-private.ts:94-117`):

- Uses legacy format (not JSON-RPC): `{ request_id, method: "subscribe", stream, feed, is_full: false }`
- Subscribes to `v1.order`, `v1.position`, `v1.fill` with feed `[subAccountId]`
- `is_full: false` requests lite format for efficiency

**Ping/Pong** (`grvt.ws-private.ts:48-53`): Sends string `"ping"`, expects string `"pong"` response.
Both TS and Rust schedule a replaceable one-shot ping five seconds after a
pong, independently of intervening account data. Rust fences that timer by
socket and authenticated-session generation and clears it on retirement.
Heartbeat timing is not subscription identity; replay protocol audits still
require authentication before private sends and exact responses to server pings.

**Stream Handling** (`grvt.ws-private.ts:199-217`):

| Stream        | Handler                  | Action                                                    |
| ------------- | ------------------------ | --------------------------------------------------------- |
| `v1.order`    | `handleOrderUpdate()`    | Updates store orders, emits fill events for filled orders |
| `v1.state`    | `handleOrderUpdate()`    | Same handler as order                                     |
| `v1.cancel`   | `handleOrderUpdate()`    | Same handler as order                                     |
| `v1.position` | `handlePositionUpdate()` | Updates store positions                                   |
| `v1.fill`     | `handleFillUpdate()`     | Emits fill events                                         |

**Order Update Logic** (`grvt.ws-private.ts:224-284`):

- **FILLED**: Updates status to Closed, emits fill event with amount, removes from store, triggers throttled order fetch
- **CANCELLED/REJECTED**: Updates status to Canceled, removes from store, triggers throttled order fetch
- **OPEN/PENDING**: Adds or updates order in store, triggers throttled order fetch
- Safeguard: throttled fetch won't wipe orders if API returns empty but store has orders (`grvt.ws-private.ts:574-579`)

**Throttling**:

- Orders: 1000ms debounce (`grvt.ws-private.ts:554-558`)
- Positions: 500ms throttle with pending event tracking (`grvt.ws-private.ts:600-631`)

**Error Codes Handled** (`grvt.ws-private.ts:183-197`):

- `1003`: Malformed syntax -- silently ignored
- `1008`: IP not whitelisted -- shows a user-facing error pointing at the Tealstreet IPs in the API key form
- Other codes: Logged as errors

## Order Types and Mappings

### Order Type Determination (`grvt.exchange.ts:910-924`)

Order type is determined by checking trigger metadata first:

- If `trigger.trigger_type === 'STOP_LOSS'` -> `OrderType.StopLoss`
- If `trigger.trigger_type === 'TAKE_PROFIT'` -> `OrderType.TakeProfit`
- If `is_market` is true -> `OrderType.Market`
- Otherwise -> `OrderType.Limit`

### Time In Force Mapping (`grvt.exchange.ts:220-229`)

| safe-cex            | GRVT                       | Signing Enum Value |
| ------------------- | -------------------------- | ------------------ |
| `GoodTillCancel`    | `GOOD_TILL_TIME`           | 1                  |
| `ImmediateOrCancel` | `IMMEDIATE_OR_CANCEL`      | 3                  |
| `FillOrKill`        | `FILL_OR_KILL`             | 4                  |
| `PostOnly`          | (set via `post_only` flag) | --                 |

### Order Status Mapping (`grvt.exchange.ts:959-975`)

| GRVT Status | safe-cex Status                  |
| ----------- | -------------------------------- |
| `PENDING`   | `Open` (sent to matching engine) |
| `OPEN`      | `Open`                           |
| `FILLED`    | `Closed`                         |
| `CANCELLED` | `Canceled`                       |
| `REJECTED`  | `Canceled`                       |

## Order Lifecycle

### Order Placement (`grvt.exchange.ts:985-1334`)

1. Validate wallet exists and market is found
2. Read `instrument_hash`, `base_decimals`, and `quote_decimals` from the current market snapshot's `info`; fail closed if the cached signing metadata is missing or invalid
3. Round price and size to market precision via `adjust()`
4. Generate nonce (random up to 1e9) and expiration (1 hour in nanoseconds)
5. Build EIP-712 message with contract-level values:
   - `contractSize = size * 10^baseDecimals`
   - `limitPrice = price * 1e9` (PRICE_MULTIPLIER)
   - `assetID = instrument_hash` (hex string)
6. Check builder authorization and include builder fields if authorized
7. Sign using `GrvtSigner.sign()` + `GrvtSigner.decode()` for r, s, v
8. Build full-format API order payload (human-readable values, not multiplied)
9. Submit via `grvtClient.tdgClient.createOrder()`
10. Extract `order_id` from response

### TP/SL Support (`grvt.exchange.ts:1146-1198`)

Two modes:

1. **Standalone SL/TP**: When `opts.type === OrderType.StopLoss` or `OrderType.TakeProfit`
   - Uses `opts.triggerPrice || opts.price` as trigger price
   - Supports trigger types: `LAST`, `MARK`, `INDEX`
2. **Attached TP/SL**: When `opts.takeProfit` or `opts.stopLoss` is set on a regular order
   - Attached as metadata trigger on the order

Trigger metadata format:

```json
{
  "trigger_type": "STOP_LOSS" | "TAKE_PROFIT",
  "tpsl": {
    "trigger_by": "LAST" | "MARK" | "INDEX",
    "trigger_price": "50000",
    "close_position": false
  }
}
```

### Order Cancellation

- **Single**: `cancelOrder()` via `grvtClient.tdgClient.cancelOrder()` with `sub_account_id` and `order_id`
- **All**: `cancelAllOrders()` via `grvtClient.tdgClient.cancelAllOrders()` with `sub_account_id`
- **By Symbol**: `cancelSymbolOrders()` reads the complete authenticated subaccount open-orders snapshot, matches exact native leg instruments, and cancels the matching native IDs individually. It never uses broad kind/base/quote cancellation filters.
- **Batch**: `cancelOrders()` loops sequentially, each failure logged but does not stop the batch

### Order Update (`grvt.exchange.ts:1459-1496`)

Regular entry TP/SL options create an ordinary entry followed by distinct
opposite-side reduce-only trigger orders. Both protective legs are retained;
a failure reports the IDs already created. A standalone SL/TP uses market
execution and explicit non-split trigger metadata. Symbol cancel-all uses the
complete native open-orders snapshot (`POST /lite/v1/open_orders`, `{sa}`),
then exact native order IDs (or validated uint64 client IDs when the native ID
is unassigned). The endpoint has no instrument filter, page cursor, limit or
documented response cap; its documented result is a complete snapshot, excluding
new orders still in flight. The raw read preserves error/truncation signals
that SDK schema mapping would obscure. Malformed/ambiguous envelopes, partial
responses, invalid or duplicate identities, account mismatches, and unsupported
multi-leg orders fail before the first write.
Client-ID fallback also fails if local order/lifecycle evidence changes during
the snapshot or write-authentication awaits, rather than retargeting a reused
ID. Disposing during reads/authentication stops writes; cancellation failures stop the sequence and
are never retried on ambiguous network failure. Individual writes preserve
raw responses too: they require an explicit accepted ACK or a returned order
matching the requested native/client ID, account and instrument. Missing,
false, malformed or mismatched acknowledgements stop all remaining writes.
Explicit error/code aliases (`e`, `error`, `c`, `code`) invalidate both snapshot
rows and returned Order proofs even alongside success fields; mixed Ack/Order
objects are rejected regardless of the acknowledgement value;
no local terminal state is inferred from an ACK. Client-only cancellations
refuse the trading client's internal authentication resend; a fresh complete
snapshot is required after session rejection. An exact native-ID cancellation
may retain one definitive-auth retry only while the exchange is live and the
same authenticated main account/subaccount is preserved. Global `cancelAllOrders()`
retains the account-wide native request. See the current
[official open-orders contract](https://api-docs.grvt.io/trading_api/#open-orders).

GRVT does not support native order modification. `updateOrder()` builds and
validates the full replacement *before* cancelling the live order, then
cancels and places the desired total minus already-filled quantity. REST `mapOrder()` and private-WS `parseOrder()` must
both preserve GRVT's `time_in_force`/`ti`, `post_only`/`po`, `reduce_only`/`ro`,
and TP/SL metadata. `post_only: true` is represented as safe-cex
`OrderTimeInForce.PostOnly`; GTT, IOC, and FOK map to their respective
safe-cex TIF values.

For standalone TP/SL, the store price is `trigger_price`, and replacement
passes it explicitly as `triggerPrice`, preserves the native market/limit execution choice and separate limit price, together with `trigger_by` and
`close_position`. Do not default a missing/unsupported TIF, trigger price,
trigger type, or close flag: fail before cancellation so an existing order is
never silently replaced with weaker semantics.

Validate the upstream TIF before interpreting `post_only`: only
`GOOD_TILL_TIME` plus `post_only: true` maps to `PostOnly`. `ALL_OR_NONE`,
`RETAIL_PRICE_IMPROVEMENT`, unknown TIFs, and maker IOC/FOK combinations do
not have a safe-cex representation and must leave `timeInForce` undefined so
`updateOrder()` rejects them before cancellation.

GRVT also requires `is_split_position` on TP/SL metadata. `PlaceOrderOpts`
cannot represent it, so `updateOrder()` must reject a split TP/SL before
cancellation rather than silently recreate it as a non-split trigger.

### Chaser REST Fill Verification

`placeOrder()` returns the decimal nonce/client-order ID as the canonical
safe-cex order ID; GRVT assigns the hex `order_id` asynchronously. The chaser
therefore passes canonical nonces to `fetchFillsForOrderIds()` when it checks
for a fill that private WS has not delivered yet. Match a fill's `co` /
`client_order_id` first, and only use `_nonceToOrderId` to translate a fill
that omits a client ID. If client-ID fields disagree, do not use the real-ID
mapping as a fallback: skipping an ambiguous fill is safer than crediting a
different chaser order and changing its remaining size. GRVT fill history is
reverse chronological and cursor-paginated: retain each canonical nonce's
creation time (placement time or `metadata.create_time` / lite `ct`) in the
same bounded lifecycle as its real-ID mapping, then query from the oldest
requested creation time less a small clock-skew margin. If any requested nonce
has an unknown timestamp, query from the safe broad start instead. Use the
official 1000-record maximum and stop at the explicit bounded page cap. Every
page must own a string `n` or `next` cursor; only `''` is terminal, while a
missing, malformed, or repeated cursor and page-cap exhaustion are errors,
never partial fill results that could permit a replacement order. Credit only
strict full-string finite, positive decimal fill amounts on both REST history
and private-WS events; malformed, zero, negative, non-finite, and
trailing-junk values must not reach (or change) the chaser's remaining size.

### Builder Fee Integration (`grvt.exchange.ts:304-351`, `grvt.builder.ts`)

If the user has authorized the Tealstreet builder:

1. `getBuilderInfo()` checks `_builderAuthorized` cache (fetched during startup)
2. If authorized, includes `builder` (address) and `builderFee` (rate) in both:
   - EIP-712 signed message (uses `OrderWithBuilderFee` type template)
   - API payload (full and lite formats)
3. Builder fee: default `0.03` (3 bps) -> `builderFeeInt = 300` for EIP-712 as uint32 (via `grvtBuilderFeeRateToRaw`, which is `percent * 10000`)

Builder authorization is main-account scoped, not sub-account scoped. The check
endpoint expects an empty request body and uses the authenticated session's main
account. The browser-wallet authorization body must use `X-Grvt-Account-Id`
(when it is address-shaped) or the auth response's `funding_account_address` as
`main_account_id` and the EIP-712 `mainAccountID`; if auth only returns a
non-address account id, fetch `aggregated_account_summary` and use its
`main_account_id` / `ma`. Do not require the connected wallet address to equal
that account id: API-key setups can sign with a wallet associated with the main
account. The request body was the source of GRVT's `invalid main account ID`
rejection, not signer-address inequality.

## Symbol Format

GRVT uses underscore-separated format:

- **Perpetual**: `{BASE}_{QUOTE}_Perp` (e.g., `BTC_USDT_Perp`)
- **Future**: `{BASE}_{QUOTE}_{EXPIRY}` (e.g., `BTC_USDT_20250328`)
- **Options**: `{BASE}_{QUOTE}_{EXPIRY}_{STRIKE}_{TYPE}` (e.g., `BTC_USDT_20250328_50000_C`)

### Internal Symbol Handling (`grvt.exchange.ts:211-213`)

- `market.id`: Original case from API (e.g., `BTC_USDT_Perp`) -- used for API calls
- `market.symbol`: Uppercased (e.g., `BTC_USDT_PERP`) -- used for internal store lookups
- `symbolToInstrumentId()`: Converts back from uppercase to API format (replaces `_PERP` with `_Perp`)

## Data Precision & Conversion

### Market Precision (`grvt.exchange.ts:467-470`)

From lite format instrument data:

- `precision.amount` = `parseFloat(inst.ms)` -- min_size used as step size
- `precision.price` = `parseFloat(inst.ts)` -- tick_size

### EIP-712 Signing Precision (`grvt.exchange.ts:1078-1095`)

For EIP-712 signature, values are converted to contract-level integers:

- **Contract Size**: exact decimal string scaled by instrument `bd`, using checked BigInt arithmetic; missing metadata fails preflight
- **Limit Price**: exact decimal string scaled by 1e9 using checked BigInt arithmetic
- **Asset ID**: `instrument_hash` passed as hex string (e.g., `"0x030501"`)

### API Payload vs EIP-712

The API payload uses **human-readable decimal strings** (e.g., `size: "0.1"`, `limit_price: "50000"`), while the EIP-712 signed message uses **contract-level integers**. These are NOT interchangeable.

### Timestamp Handling

GRVT uses nanosecond timestamps throughout:

- `expiration`: `(BigInt(Date.now()) + 3600000n) * 1000000n` as a decimal string
- `create_time`: `BigInt(Date.now()) * 1000000n` as a decimal string
- Kline timestamps: Divided by `1_000_000_000` to convert to seconds
- Position update timestamps: Divided by `1_000_000` to convert to milliseconds

### Balance Calculation (`grvt.exchange.ts:602-673`)

Uses `/lite/v1/aggregated_account_summary`:

- `total` = `parseFloat(aggData.te)` (total_equity)
- `free` = `parseFloat(aggData.ts2)` (total_sub_account_available_balance), falls back to total_equity only when absent; an explicit zero stays zero
- `used` = total - free
- Spot balances parsed from `aggData.sb` array

### Auto-Detection of Trading Account ID (`grvt.exchange.ts:643-654`)

If `grvtTradingAccountId` is not configured, it's auto-detected from the first sub-account in `aggregated_account_summary.sub_account_summaries`. This is crucial because many API calls require a valid `sub_account_id`.

## Rate Limiting

### Axios Rate Limits (`grvt.exchange.ts:130-132`)

All three API instances are rate-limited to 10 RPS:

```typescript
this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 10 });
this.edgeXhr = rateLimit(createEdgeAPI(opts, this), { maxRPS: 10 });
this.tradingXhr = rateLimit(createTradingAPI(opts, this), { maxRPS: 10 });
```

### Network Retry

All three instances retry 5 times on network errors, **reads only**. GRVT POSTs its reads, so `grvtRetry.ts` names them (matched on the last path segment, since the TDG client posts under both `/lite/v1/` and `/full/v1/`); any other path, including every order write, is never resent:

```typescript
retry(instance, { retries: 5, retryCondition: networkErrorOnRead(isGrvtRead) });
```

### Tick Interval

Balance polling runs every 60 seconds via `loop()` utility (`grvt.exchange.ts:196`). Comment indicates this is ~0.17 req/10s, well below GRVT's tier-based limit of 25-225 req/10s.

### WebSocket Ticker Batching

Ticker subscriptions are batched in groups of 20 with 100ms stagger (`grvt.ws-public.ts:73-87`).

## State Reconciliation

### REST Polling

- **Balance**: Every 60 seconds via `tick()` (`grvt.exchange.ts:177-198`)
- **Positions**: Not periodically polled. Updated via WebSocket + throttled REST fetch on position events
- **Orders**: Not periodically polled. Updated via WebSocket + throttled REST fetch on order events

### WebSocket -> REST Reconciliation

Both order and position WebSocket events trigger a throttled REST fetch to ensure consistency:

- **Orders**: 1000ms debounce. Safeguard prevents wiping orders if API returns empty but store has orders (`grvt.ws-private.ts:574-579`)
- **Positions**: 500ms throttle with pending event tracking

### Fake Positions (`grvt.exchange.ts:675-698`)

GRVT creates "fake" zero-size positions for every market (one-way mode). Real positions are merged with fake positions -- fake positions are filtered out for symbols that have real positions (`grvt.exchange.ts:797-800`). This ensures the UI always shows a position entry for every market.

### Ticker deltas

The rolling buy/sell volumes and open price are retained independently across
partial updates. Quote volume and percentage use the current last price even
when a frame omits the corresponding input. GRVT empty strings clear numeric
fields to zero; next funding time is nanoseconds in either JSON numeric form.

### Ticker Population

Initial tickers are placeholders with all zeros (`fetchTickers()` just creates stubs from markets). Real values are populated via WebSocket `v1.ticker.d` delta stream.

## Exchange Metadata

- **Position Mode**: One-way (`getPositionMode()` returns `PositionMode.OneWay`)
- **Margin Type**: native `mt` / `margin_type` (`CROSS` or `ISOLATED`); legacy missing fields retain isolated fallback.
- **Default Leverage**: 10x (`grvt.exchange.ts:73`)
- **SDK Package**: `@tealstreet/grvt-ts-sdk` (forked) for signing, `@tealstreet/grvt-client` (forked from `@grvt/client`) for API

## Testnet / Demo

### URL Differences (`grvt.types.ts:1-21`)

| Component         | Production                          | Testnet                                     |
| ----------------- | ----------------------------------- | ------------------------------------------- |
| Market Data (MDG) | `https://market-data.grvt.io`       | `https://market-data.testnet.grvt.io`       |
| Trading (TDG)     | `https://trades.grvt.io`            | `https://trades.testnet.grvt.io`            |
| Edge (Auth)       | `https://edge.grvt.io`              | `https://edge.testnet.grvt.io`              |
| WebSocket         | `wss://market-data.grvt.io/ws/full` | `wss://market-data.testnet.grvt.io/ws/full` |

Additional environments:

- **Staging**: `*.staging.gravitymarkets.io` (REST) / `*.stg.gravitymarkets.io` (WS) -- note subdomain mismatch
- **Dev**: `*.dev.gravitymarkets.io`

### Chain ID Differences (`grvt.exchange.ts:1062`)

- Production: chainId `325`
- Testnet: chainId `326`

Note: `grvt.signing.ts` has different values (325 testnet, 324 mainnet) but `grvt.exchange.ts` uses its own logic (326 testnet, 325 production). The exchange.ts values are the ones actually used for order signing.

### API Key Management

- Testnet: https://testnet.grvt.io/exchange/account/api-keys
- Mainnet: https://grvt.io/exchange/account/api-keys

### Test Credentials

Stored in `.apikey.json`:

```json
{
  "apiKey": "<api_key>",
  "secret": "<private_key_0x_prefixed>",
  "testnet": true,
  "accountId": "<account_id>",
  "subAccountId": "<sub_account_id>"
}
```

## Known Quirks / Implementation Notes

1. **Three separate Axios instances**: MDG (public), Edge (auth), TDG (private) each have separate base URLs and interceptors. This is required because GRVT uses different hostnames for each gateway.

2. **Lite vs Full format everywhere**: Nearly every data parser has dual-format handling (e.g., `feed.i || feed.instrument`). The implementation primarily uses lite format for efficiency.

3. **Instrument signing metadata cache**: `fetchMarkets()` retains only `ih`, `bd`, and `qd` in each `Market.info`. `placeOrder()` reads and validates that snapshot instead of re-fetching all instruments. GRVT's market refresh replaces same-ID markets too, so metadata-only upstream changes do not leave signing stale.

4. **Private WS through proxy**: Private WebSocket gets its Tealstreet proxy host from `resolveExchangeRouting(...).wsPrivate`, rather than connecting directly to GRVT, because browser security prevents sending cookies directly.

5. **Symbol case sensitivity**: GRVT uses mixed case (`BTC_USDT_Perp`) while the store uses uppercase (`BTC_USDT_PERP`). The `symbolToInstrumentId()` method handles conversion, and market lookups try multiple case formats.

6. **Trading account ID auto-detection**: If not provided, auto-detected from the first sub-account in aggregated_account_summary (`grvt.exchange.ts:643-654`). Many operations silently skip if no trading account ID is available.

7. **Diagnostics**: GRVT runtime and builder paths have no console diagnostics. Use only safe `BaseExchange.log()` messages with explicit severity where an exchange instance exists; never log API keys, proxy tokens, session cookies, auth headers, request/response bodies, or signatures. The trusted GRVT audit regression statically rejects console calls in runtime sources.

8. **Chainid discrepancy**: `grvt.signing.ts` defines testnet chainId as 325, but `grvt.exchange.ts` uses 326. The exchange.ts values are authoritative for order signing.

9. **HTTP-over-WebSocket adapter commented out**: All three API instances have the HOW adapter commented out (`grvt.api.ts`).

10. **Builder authorization cached as tristate**: `_builderAuthorized` is `true`, `false`, or `null` (unknown). If null, `getBuilderInfo()` triggers a fetch before returning.

11. **No batch order placement**: `placeOrders()` loops sequentially. Current official GRVT trading docs do not document a bulk-create endpoint.

12. **GRVT SDK client uses `any` casts**: Many SDK client calls use `(this.grvtClient as any).tdgClient.*` because the TypeScript types are incomplete.

13. **Leverage per-symbol, not global**: Leverage is set per-instrument via `set_initial_leverage`. The `fetchLeverageForSymbol()` method fetches all leverage settings and filters for the requested symbol.

14. **GRVT error responses can be HTTP 200**: The API can return HTTP 200 with error data in the body (checked via `data?.c && data?.m` pattern in all interceptors).

15. **Private WS subscription uses legacy format**: Not JSON-RPC 2.0 like public WS, but `{ request_id, method, stream, feed, is_full }` format.

## Error Handling

### REST API Errors (`grvt.api.ts`)

All three API instances handle errors in interceptors:

**Error Formats** (checked in order):

1. Lite format in body: `{ c: <code>, m: <message>, s: <status> }`
2. Full format: `{ error: { code, message } }`
3. Plain message: `{ message: "..." }`
4. String body
5. HTTP status code errors

**HTTP 200 with Error Body**: GRVT can return HTTP 200 with error data. The response interceptor checks `data?.c && data?.m` and rejects the promise if present.

### Authentication Errors

| Code | Meaning                            | Handling                                          |
| ---- | ---------------------------------- | ------------------------------------------------- |
| 1000 | Session expired                    | Auto-reauthenticate and retry (`grvt.api.ts:331`) |
| 1001 | Not authorized (signer not tagged) | Logged, user-facing error                         |
| 1400 | No trade permission                | Logged                                            |
| 2000 | Unauthorized signer                | Logged                                            |
| 2002 | Signature mismatch                 | Logged                                            |

### WebSocket Errors

| Code | Meaning              | Handling                        |
| ---- | -------------------- | ------------------------------- |
| 1003 | Malformed syntax     | Silently ignored                |
| 1008 | IP not whitelisted   | User-facing error with proxy IP |
| 1100 | Stream not supported | Silently ignored (public WS)    |

### Order Placement Errors (`grvt.exchange.ts:1289-1333`)

Multiple error response formats are checked:

1. Top-level lite: `response.m`, `response.c`
2. Full format: `response.error?.message`
3. Nested lite: `response.data?.m`
4. HTTP status: `response.status >= 400`
5. Axios response data: `err.response?.data`

### Resilience Patterns

- `tick()` catches errors silently, continues polling (`grvt.exchange.ts:189-191`)
- `start()` wraps each fetch in try/catch, sets loaded flags even on failure
- `cancelOrders()` catches per-order failures and continues
- `placeOrders()` catches per-order failures and continues

## Testing / Validation

### Test Credentials

File: `.apikey.json` in the exchange directory.

### Validation Flow (`grvt.exchange.ts:272-298`)

1. Authenticate via `authenticate()`
2. Verify SDK client is initialized
3. Call `getFundingAccountSummary()` to verify session
4. Return empty string on success, error message on failure

Signer-address derivation and verification are not package-local test helpers.
Use trusted wallet tooling outside the repository and compare only the public
address with the address tagged in the GRVT UI. Never expose the private key or
add a credential-reading script under `src/exchanges/**`.

## External References

- **Official Website**: https://grvt.io/
- **API Documentation**: https://api-docs.grvt.io/
- **Status Page**: https://status.grvt.io/
- **Discord**: https://discord.gg/grvt
- **Python SDK**: https://github.com/gravity-technologies/grvt-pysdk
- **JS Client**: https://github.com/gravity-technologies/grvt-js-client
- **EIP-712 Spec**: https://eips.ethereum.org/EIPS/eip-712
- **Builder Codes**: https://api-docs.grvt.io/builder_codes/

## Implementation Status

### Completed Features

- [x] REST API client with session-based authentication (three gateways)
- [x] Market data fetching (instruments, tickers via WS, orderbook, candles)
- [x] Account management (balance, positions, account summary)
- [x] Order placement with EIP-712 signing
- [x] Order cancellation (single, all, by symbol)
- [x] TP/SL order support (standalone and attached)
- [x] Public WebSocket (ticker delta, orderbook snapshot, candles)
- [x] Private WebSocket (orders, positions, fills) via proxy
- [x] Leverage management (get/set per instrument)
- [x] Builder fee integration (check/authorize/include in orders)
- [x] Auto-reauthentication on session expiry
- [x] Sub-account ID auto-detection

### Future Enhancements

- [ ] Transfer operations (internal transfers, deposits, withdrawals)
- [ ] Vault operations (invest, redeem, query)
- [ ] Cross-exchange vault support
- [ ] Cache instrument data to avoid re-fetching on every order
- [ ] Resolve chainId discrepancy between signing.ts and exchange.ts

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `metadata.client_order_id`. GRVT types it uint64 with the top half reserved for its own UI, so the rule accepts canonical decimal strings `1..2^63-1`. The caller's ID then becomes the canonical safe-cex order ID: it replaces the nonce everywhere the nonce was used (`recordNonce`, builder-fee events, the return value).

## Rust socket supervision

All Rust public, per-timeframe candle and private sockets use `engine::socket::Supervisor` for connection deadlines and capped exponential reconnect delay with jitter. Public/candle idle deadlines remain 20 seconds; the private idle deadline is 90 seconds and proxy authentication has a separate 15-second deadline. Native lowercase ping/pong remains venue-owned, including the replaceable, socket/generation-fenced private ping five seconds after a pong. Terminal authentication rejection still suppresses retries; dropping the last candle interest cancels its supervision and pending reconnect.

## Rust shared public feed

Shared provider quotes enter the native ticker store and run GRVT's existing
position and balance PnL calculation using the current last price. This starts
only after native private balance and position snapshots are loaded; a public
quote cannot mark private datasets ready. Native account-summary and catalog
refreshes retain provider quotes and the resulting private PnL. Summary equity,
available funds and margin usage continue to come from the authenticated venue.

## Rust public discovery resilience

Rust catalogue/ticker reads retry through the shared engine capped exponential
backoff. A malformed typed public row is skipped with a warning while valid
neighbours remain usable. Private account initialization starts once the usable
catalogue is available, without waiting for REST ticker seeds. Account snapshots,
mutation acknowledgements and signing validation retain their stricter rules.
See `docs/plans/RUST_STARTUP_AUDIT.md` and the native startup resilience regressions.

## Rust builder error contract

Native definitive missing/insufficient approval preserves
`GRVT_BUILDER_UNAUTHORIZED`, `reason: unauthorized | cap-too-low`,
`requiredRaw: 300` and the parsed `authorizedRaw` for a low cap. The shared host
and SDK command allowlist carry this through local, worker and hosted rejection.
Use SDK-root `builderAuthorizationOf(error)` for structural recognition; the old
TS class prototype is not reconstructed. NX's existing class-only modal handler
must migrate deliberately. Pending/unknown checks and exhausted network or
malformed-list failures remain ordinary errors. Existing validation order,
missing/unreadable-cap policy, retry budget, FIFO and write outcomes stay intact.


### Native REST order-book query

Rust exposes public SDK fetchOrderBook through an unsigned MDG POST on live and
testnet, using the stored native instrument ID. The [official lite book schema](https://api-docs.grvt.io/market_data_api/#orderbook-levels)
requires {i:nativeId,d:50}; this deliberately corrects TS's legacy instrument_id
body and long-only response mapping. Caller limit stays ignored. Lite r/b/a/p/s
and full result/bids/asks/price/size aliases map directly to numeric price and base
amount, cumulative amount totals per side, native order and zero quantity rows;
no price IDs, sequence metadata or decimal/contract scaling is introduced.

Unknown markets, missing/null result and missing/null sides retain TS's empty
book behavior. Present malformed structures, invalid/nonfinite financial rows
and cumulative overflow fail the complete query. Named record boundaries require
objects. This command never publishes subscription books or account/session
headers, including on public code1000 or responses with cookies/account IDs.
HTTP/native failures do not retry or refresh auth; transport uses the existing
five bounded read reissues, then a typed owned Network/NotSent failure. Public
rate1006/HTTP429 retains bounded usable Retry-After metadata without account-auth
classification. The standalone family explicitly enables its JSON book decoder.
