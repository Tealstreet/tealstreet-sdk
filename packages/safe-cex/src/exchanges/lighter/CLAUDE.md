# Lighter Exchange Integration

This directory contains the Lighter exchange integration for Tealstreet. Lighter is a decentralized exchange (DEX) running as a ZK rollup on Arbitrum, using Poseidon-hash-based cryptographic signing for transaction authorization.

## Exchange Information

- **Type**: Decentralized exchange (ZK rollup on Arbitrum)
- **Signing**: Poseidon hash-based cryptography (WASM)
- **Margin**: Cross and isolated (switchable via signed leverage update)
- **Position Mode**: One-way / net only. Hedge mode is not supported as of 2026-05-23; live testing showed an opposite-side non-reduce fill reduced the existing position instead of opening a separate hedge.
- **Quote Currency**: USDC for zkLighter perp margin/quote handling; USDG for `lighter-robinhood`. Lighter's `/api/v1/orderBooks` also returns spot books; this perps exchange must filter spot rows out via `market_type`.
- **Status**: Enabled in production and dev (`enabled: true`, `enabledInDev: true`, preview-enabled)

## API Documentation

Official API documentation:

- Get started: <https://apidocs.lighter.xyz/docs/get-started-for-programmers>
- Rate limits: <https://apidocs.lighter.xyz/docs/rate-limits>
- Account tiers: <https://apidocs.lighter.xyz/docs/account-types>
- Premium volume quota: <https://apidocs.lighter.xyz/docs/volume-quota-program>

CCXT has a Lighter adapter, but Tealstreet uses the internal `@tealstreet/lighter-sdk` signer/API path here. Use CCXT only as a loose reference, not as the source of truth for Robinhood deployment config.

## File Structure

| File                           | Description                                                                                                |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| `lighter.api.ts`               | SDK-based API client factory; creates `LighterApiClient` with request/response/error interceptors          |
| `lighter.exchange.ts`          | Main exchange class (`LighterExchange`) extending `BaseExchange`; implements all trading operations        |
| `lighter.types.ts`             | Constants and type mappings: deployment profiles, URLs, endpoints, interval/order/status maps              |
| `lighter.utils.ts`             | Utility functions: client order ID generation, precision extraction, hex prefix helpers, BigInt conversion |
| `lighter.ws-public.ts`         | Public WebSocket (`LighterPublicWebsocket`); market data: tickers, orderbook, candles, trades              |
| `lighter.ws-public-ohlcv.ts`   | Public candle/trade channel builders and OHLCV recovery guards                                             |
| `lighter.ws-public-parsers.ts` | Public-frame dispatch and numeric parsing helpers                                                          |
| `lighter.ws-policy.ts`         | Public/private close classification and reconnect delay policy                                             |
| `lighter.signer.ts`            | Lighter transaction signer adapter                                                                         |
| `lighter.builder-approval.ts`  | Builder/integrator approval status and submission helpers                                                  |
| `lighter.builder-fee.ts`       | Integrator fee mode and transaction attribution helpers                                                    |
| `lighter.ws-private.ts`        | Private WebSocket (`LighterPrivateWebsocket`); account events: orders, positions, fills, balance           |

`fetchOrders()` coalesces and returns the REST order snapshot without mutating
the store. The periodic private snapshot and throttled private-WebSocket heal
commit that result with `addOrUpdateOrdersFull()` after disposal checks.

## Authentication

Lighter uses a two-layer authentication model:

### REST API Authentication

Lighter REST authentication is carried by signed transaction payloads or explicit auth fields on the request:

- Trading writes use signed `sendTx` / `sendTxBatch` payloads.
- Account/order read endpoints that need private access pass the signer auth token through documented `auth` query params or `authorization` headers.

`lighter.api.ts` strips stale `X-Timestamp` / `X-Account-Index` headers if they are inherited from older callers. Do not reintroduce those global headers: they are not part of the current upstream REST contract and they trigger browser CORS preflight failures through the Tealstreet REST proxy.

Important identifier distinction:

- `accountIndex` (numeric account identifier, potentially large) is the Lighter account ID.
- `apiKeyIndex` (0-254) selects which API key signs the transaction.
- `apiKeyIndex` is not the same value as `accountIndex`.

### Transaction Signing (ZK Rollup)

For write operations (placing/cancelling orders), a `TransactionClient` from `@tealstreet/lighter-sdk` is initialized with:

- **WASM cryptography**: Uses `@tealstreet/poseidon-crypto` (Poseidon hash), initialized via `initWasm()` / `getCryptoApi()` at startup
- **Private key**: From `options.secret`
- **Account index**: Fetched from the `/api/v1/account` endpoint and stored as `BigInt`
- **API key index**: Parsed from `options.apiKeyIndex` (converted to integer via `parseInt`)
- **Signer domain chain ID**: resolved via `resolveLighterDeploymentProfile()` — `304` (livenet), `300` (testnet), or `466324` (Robinhood Chain Lighter)

The signer adapter wraps `TransactionClient` operations for:

- `createOrder()` — placing regular, market-style, stop-loss, and take-profit orders
- `cancelOrder()` — cancelling orders
- `updateLeverage()` — leverage and cross/isolated mode changes
- `approveIntegrator()` — builder/integrator approval transactions

Signed transactions are submitted via `lighterApiClient.sendRawTx()`.

### Private WebSocket Authentication

On connection open:

- Always subscribes to `account_all/<accountIndex>` (no auth token required).
- Then attempts signer-token auth and subscribes to `account_all_orders/<accountIndex>` with `auth: <token>`.
- If token is unavailable (or that channel is unsupported), it continues with `account_all` and REST order reconciliation.

### Required Credentials

From `exchangeMetadata`:

- `requiresApiKeyIndex: true` — API key index is required
- `requiresPassphrase: false`
- `requiresApplicationId: true` — use account index in Application ID for the most deterministic setup
- `requiresNoSecret: false` — Private key (secret) is needed for trading

The exchange enters `publicDataOnly` mode when it cannot sign privately (missing secret/remote signer) or when `apiKeyIndex` is missing.

Account index resolution currently supports:

- Explicit account index in `applicationId`
- L1 wallet address lookup (via `key` or `applicationId`) to resolve account index
- Legacy fallback where older setups stored account index in `apiKeyIndex`

## Startup Sequence

The current `start()` flow:

1. Initialize external public data if configured.
2. Determine `publicDataOnly` from explicit option, signer availability, and `apiKeyIndex`.
3. Warm up signer asynchronously when private trading is possible, without blocking public streams.
4. Fetch markets and seed empty tickers, then connect public WebSocket unless external public data is active.
5. In public-data-only mode, mark private datasets loaded and return.
6. Await signer warmup; connect private WebSocket only if signer initialization succeeds.
7. Fetch initial private snapshot (`fetchPositions()` + `fetchRawBalance()`), update store, then start the tick loop.

Signer startup resolves account index, initializes Poseidon WASM, validates API key index, and creates the `LighterSignerAdapter`.

## REST API Endpoints

### Account

| Endpoint                    | Method | Description                                             |
| --------------------------- | ------ | ------------------------------------------------------- |
| `/api/v1/account`           | GET    | Get account info (`accountIndex`, `referrer`)           |
| `/api/v1/accountLimits`     | GET    | Get account tier/limits (`user_tier`, `user_tier_name`) |
| `/api/v1/changeAccountTier` | POST   | Change account tier (`standard`, `premium`, `plus`)     |

Additional account-related endpoints used in discovery/integration:

- `GET /api/v1/accountsByL1Address` — resolve account indexes for an L1 wallet.
- `GET /api/v1/accountLimits` — requires signer auth; used to show Standard/Premium/Plus tier in the Margin module.
- `POST /api/v1/changeAccountTier` — requires signer auth and form-encoded `account_index` + `new_tier`.
- `GET /api/v1/nextNonce` — fetch next nonce for (`account_index`, `api_key_index`).
- `GET /api/v1/systemConfig` — global integrator fee caps and protocol settings.

### Market Data

| Endpoint                   | Method | Description                                                                          |
| -------------------------- | ------ | ------------------------------------------------------------------------------------ |
| `/api/v1/orderBookDetails` | GET    | Preferred market metadata source: margin mode, margin fractions, precision, limits   |
| `/api/v1/orderBooks`       | GET    | Fallback active-market list (perps + spot rows), missing some risk metadata          |
| `/api/v1/exchangeStats`    | GET    | Public ticker/statistics fallback (`order_book_stats`)                               |
| `/api/v1/candles`          | GET    | Candlestick data for supported raw resolutions                                       |
| `/api/v1/orderBookOrders`  | GET    | Current orderbook levels (`market_id`, `limit`), via `OrderApi.getOrderBookOrders()` |
| `/api/v1/trades`           | GET    | Get recent trades                                                                    |

### Orders

| Endpoint                        | Method | Description                                                         |
| ------------------------------- | ------ | ------------------------------------------------------------------- |
| `/api/v1/accountActiveOrders`   | GET    | Open-order reconciliation                                           |
| `/api/v1/accountInactiveOrders` | GET    | Account inactive order history (primary for `fetchHistoricalFills`) |

### Transactions

| Endpoint              | Method | Description                                 |
| --------------------- | ------ | ------------------------------------------- |
| `/api/v1/sendTx`      | POST   | Submit one signed transaction               |
| `/api/v1/sendTxBatch` | POST   | Submit a batch of signed order transactions |

**Note**:

- Open-order reconciliation currently uses direct REST calls to `ENDPOINTS.OPEN_ORDERS` with `account_index`, `market_id`, and `auth` token.
- The implementation first probes `market_id=255` (all-markets sentinel), then falls back to per-market fetches when needed.
- OHLCV backfill uses REST candles (`/api/v1/candles`) and is layered under WS streaming in `listenOHLCV()`.
- Execution-history fetches for chart fills use `/api/v1/accountInactiveOrders` first, then `/api/v1/trades` as a fallback source.
- `/api/v1/trades` requests must include `sort_by=timestamp` and `limit` within `1..100`; `sort_dir=desc` is used.
- Fill side mapping in `fetchHistoricalFills` supports both direct side flags (`side`, `isAsk`, `is_ask`) and account-scoped inference from `ask_account_id` / `bid_account_id`.

## WebSocket Implementation

### Public WebSocket (`lighter.ws-public.ts`)

**Connection**: URL appends a unique `?requestId=<nanoid>` query parameter on each connection (`connectAndSubscribe`).

**Subscription format**: `{ type: "subscribe", channel: "<channel>" }`

Topics are tracked per channel and re-subscribed on reconnect.

| Channel Pattern                  | Description                                                        |
| -------------------------------- | ------------------------------------------------------------------ |
| `market_stats/all`               | Market stats snapshot/update stream                                |
| `ticker/<marketId>`              | Per-market BBO/ticker updates                                      |
| `order_book/<marketId>`          | Order book snapshots and incremental updates                       |
| `candle/<marketId>/<resolution>` | Candlestick updates for a specific market and supported resolution |
| `trade/<marketId>`               | Trade stream (secondary source for real-time candle updates)       |

**Ping/pong**: Sends ping on open, responds to server ping with pong, and tracks latency via base WS helpers.

**Error frames**: Channel-less `{ error: { code, message } }` frames are recorded as public-handler telemetry and logged with both fields at warning severity, so rejected subscriptions cannot disappear as an apparently quiet stream.

**Reconnect and route promotion**:

- Classifies close events and applies reconnect backoff (`calculateLighterPublicReconnectDelay`).
- Promotes direct route to readonly direct mode after repeated direct-route failures.
- Promotes direct route to proxy mode after consecutive direct-route failures when proxy routing is enabled.

**Ticker fallback behavior**:

- Immediately fetches ticker snapshot from `/api/v1/exchangeStats` on open.
- Starts REST ticker polling when no WS messages are seen shortly after connect (watchdog).
- Keeps REST ticker polling active while stream is quiet/disconnected, or while WS traffic exists but no positive ticker price has been hydrated yet. Generic orderbook/candle/pong traffic is socket liveness, not ticker recovery; stop ticker REST fallback only after `mark`, `last`, `index`, `bid`, or `ask` is positive.

**Orderbook fallback behavior**:

- On subscribe, primes orderbook immediately through `OrderApi.getOrderBookOrders()` (before waiting for first WS snapshot). The SDK maps the current official `/api/v1/orderBookOrders` route and normalizes its `{ bids, asks, remaining_base_amount }` response.
- On public WS reconnects, active orderbook topics are primed through the same REST route, and public close events keep active books on REST fallback until live WS book frames resume. Do not make reconnect recovery wait only on the long stale-orderbook timer; DOM/depth/orderbook panels need a near-term book after account reloads or route changes.
- That endpoint returns raw orders, so the public socket coalesces equal-price orders per side before sorting and calculating cumulative depth totals.
- If initial WS snapshot does not arrive quickly, enables periodic REST orderbook fallback.
- If WS orderbook goes stale, attempts recovery with re-subscribe first, then reconnect on repeated failure.

**OHLCV behavior**:

- Primary source is WS candle channel, with trades channel as secondary interpolation source.
- Per-topic no-data timeout is intentionally quiet-tolerant: ~120s without OHLCV data before guarded WS recovery (`attemptOhlcvRecovery`).
- The timeout is shared by all listeners on that candle topic: it stays armed until the final listener unsubscribes, a listener that unsubscribes before connect cancels its pending subscribe retry, and reconnect re-arms one guard for each still-active candle topic.
- Public socket health also uses the ~120s quiet-stream threshold so lightly traded symbols do not falsely mark the account degraded.
- `lighter.exchange.ts` additionally polls REST latest candle every 5s and only emits when changed if WS candle flow is stale for ~120s.

### Private WebSocket (`lighter.ws-private.ts`)

**Connection**:

- Direct route: `parent.lighterDeploymentProfile.wsUrl` (`wss://mainnet.zklighter.elliot.ai/stream`, testnet equivalent, or `wss://api.rh.lighter.xyz/stream`)
- Proxy route (when `tealstreetWsRoute` is enabled): `resolveExchangeRouting(name, options).wsPrivate.host` (routing table)
- Readonly fallback appends `?readonly=true` on direct route only

**Authentication/subscription flow**: Subscribes to `account_all/<accountIndex>` first, then attempts token-authenticated subscribe for `account_all_orders/<accountIndex>`.

**Subscribed channels**:

- `account_all/<accountIndex>` (always)
- `account_all_orders/<accountIndex>` (when token is available and channel is supported)

Private data can arrive as:

- `account_all` payloads with by-market orders/positions/trades and nested arrays
- `account_all_orders` payloads for order updates/acks (when available)

**Ping/pong and idle guards**:

- Standard mode: ping every 10s, idle ping threshold 20s, idle reconnect threshold 180s.
- Readonly-fallback mode: ping every 20s, idle ping threshold 45s, idle reconnect threshold 180s.
- During server WS rate-limit cooldown, ping cadence is throttled and subscribe retries are delayed.

**Diagnostics and recovery windows**:

- Warns if connected but no private account stream payloads after ~90s.
- Warns when account stream becomes stale after prior activity, with a ~90s threshold and 60s warning cooldown.
- Recovery attempts are intentionally delayed for low-traffic conditions:
  - no-stream recovery after 180s from connect
  - stale-stream recovery after 120s since last stream message
  - recovery attempt cooldown 180s
- Recovery strategy is re-subscribe first, then guarded reconnect fallback.

**Readonly fallback behavior**:

- Readonly fallback is promoted after repeated readonly-signal handshake failures on direct route.
- Readonly fallback warning toast/log is cooldown-gated per account/environment.

**Reconnection behavior**:

- Private WS owns its close/reconnect flow (not delegated to BaseWebSocket `onClose`).
- Reconnect delay is computed via `calculateLighterPrivateReconnectDelay` with handshake failure and rate-limit awareness.
- On reconnect (with reconnecting-websocket mode), snapshot reconciliation is triggered.

**Fill notification timing**: Only emits `fill` events if:

- Order/update event timestamp is within the last 5 seconds
- At least 5 seconds have passed since WebSocket connection
- This prevents notification spam from historical fills on reconnect

**Order throttle fetch**: Uses debounced `throttleFetchOrders()` (default 500ms) to avoid excessive API calls. Only triggers when reconciliation needs REST backfill; cancellation flows are handled specially to avoid resurrecting recently canceled orders.

**WebSocket diagnostics**: Private WebSocket messages go through `BaseWebSocket`'s runtime-gated diagnostics helpers. They populate the web Requests drawer only when WS diagnostics are enabled through the Requests Log toggle; Grafana upload is a separate opt-in.

## Order Types and Mappings

### Supported Order Types

| API Type             | Internal Type          |
| -------------------- | ---------------------- |
| `LIMIT`              | `OrderType.Limit`      |
| `MARKET`             | `OrderType.Market`     |
| `STOP_LOSS`          | `OrderType.StopLoss`   |
| `STOP_MARKET`        | `OrderType.StopLoss`   |
| `STOP_LIMIT`         | `OrderType.StopLoss`   |
| `TAKE_PROFIT`        | `OrderType.TakeProfit` |
| `TAKE_PROFIT_MARKET` | `OrderType.TakeProfit` |
| `TAKE_PROFIT_LIMIT`  | `OrderType.TakeProfit` |

### Time in Force

| API Value   | Internal Value      |
| ----------- | ------------------- |
| `GTC`       | `GoodTillCancel`    |
| `IOC`       | `ImmediateOrCancel` |
| `FOK`       | `FillOrKill`        |
| `POST_ONLY` | `PostOnly`          |

### Order Status

| API Status               | Internal Status        |
| ------------------------ | ---------------------- |
| `PENDING`                | `OrderStatus.Open`     |
| `OPEN`                   | `OrderStatus.Open`     |
| `PARTIALLY_FILLED`       | `OrderStatus.Open`     |
| `FILLED`                 | `OrderStatus.Closed`   |
| `CANCELLED` / `CANCELED` | `OrderStatus.Canceled` |
| `EXPIRED`                | `OrderStatus.Closed`   |
| `REJECTED`               | `OrderStatus.Canceled` |

### Trigger Types

| API Type | Internal Type                     |
| -------- | --------------------------------- |
| `INDEX`  | `OrderStopTriggerType.IndexPrice` |
| `MARK`   | `OrderStopTriggerType.MarkPrice`  |
| `LAST`   | `OrderStopTriggerType.LastPrice`  |

## Order Lifecycle

### Order ID Generation

Client-side IDs use one instance allocator: the timestamp/random candidate seeds a positive 48-bit index, and each reservation is greater than the previous reservation even when time moves backward. Every actual lot or sibling reserves independently before its send; interleaved orders can occupy intervening indices. Failed sends do not release indices, and exhaustion rejects instead of wrapping.

### Order Placement

1. Validate signer adapter is available
2. Resolve market and its precision/limits
3. Adjust amount and price to market precision via `adjust()`
4. For market-style orders: choose a positive live reference price before applying 2% slippage. Prefer side BBO (`ask` for buys, `bid` for sells), then `mark`, `last`, `index`, midpoint, and opposite-side BBO. Never sign a market-style order from a zero/missing mark price. For stop-loss/take-profit orders, Lighter's docs define `price` as the worst acceptable execution price after `triggerPrice`; apply the configured stop slippage percent from trigger (`sell = trigger * (1 - pct)`, `buy = trigger * (1 + pct)`) rather than sending `price === triggerPrice`.
5. Normalize conditional stop-loss/take-profit market variants to IOC
6. Enforce Lighter minimums for non-market orders (`min_base_amount`, `min_quote_amount`)
7. Build `orderParams` with orderId, marketId, isBuy, scaled price/amount, order type, TIF, trigger price, postOnly, reduceOnly, and optional builder fee attrs
8. Convert price/amount with `toLighterScaledInt(value, decimals)` using market-provided decimals (`supported_size_decimals`, `supported_price_decimals`) rather than assuming fixed 18-decimal scaling
9. If amount exceeds the market max amount, split into lots:
   - Calculate lot count: `Math.ceil(amount / maxSize)`
   - Calculate remainder: `amount % maxSize`
   - Each lot reserves its own orderId from the shared instance allocator
   - 100ms delay between lot submissions
   - Remainder submitted as final lot if non-zero
10. Each lot: `signerAdapter.createOrder(...)`
11. Return generated numeric client order indexes from the exchange method; signer/API tx hashes are internal and must not be exposed as `Order.id`
12. Trigger throttled order fetch via `privateWebsocket.throttleFetchOrders()`

### Batched Scaled Orders

`placeOrders()` has a Lighter-specific override for independent scaled limit ladders:

- Compatible ladders are same-market, non-reduce-only limit orders with no inline TP/SL siblings and no oversized lot splitting.
- Compatible ladders are signed as multiple `L2CreateOrder` transactions and submitted once via `/api/v1/sendTxBatch`.
- Builder/integrator attribution is preserved on every transaction in the batch via each order's `L2TxAttributes`.
- Order hints are recorded per generated client order id before submission so REST/WS reconciliation preserves side, TIF, and conditional intent.
- `placeOrders()` returns the generated numeric client order indexes, not the `sendTxBatch` transaction hashes. Returning tx hashes causes fast drag/cancel flows to pass hex ids into cancellation, which cannot be converted to Lighter order indexes.
- One private-order reconciliation fetch is triggered after batch success or an uncertain batch error.
- Unsupported inputs fall back to sequential `placeOrder()` behavior: mixed markets, market/trailing orders, conditional TP/SL batches, reduce-only ladders, oversized lot splits, or preflight incompatibilities.
- If batch submission errors after signing/submission is attempted, do not blindly resubmit sequentially. Surface an actionable error and ask the user to check open orders before retrying because the sequencer may still accept individual transactions from the batch.

`L2CreateGroupedOrders` (`tx_type = 28`) is also wired in the internal SDK/signer, but it is not a generic scaled-ladder primitive. Upstream Lighter validation constrains it to semantic OTO/OCO/OTOCO groups:

- max 3 orders;
- same perps market id;
- grouping type `1` = OTO, `2` = OCO, `3` = OTOCO;
- OCO requires exactly one stop-loss child and one take-profit child, same side/amount/expiry, both reduce-only;
- OTO/OTOCO require child orders opposite the parent and child amount `0`.

Use `sendTxBatch` for independent scaled ladder entries. Use `L2CreateGroupedOrders` only when those protocol-level trigger/cancel relationships are intended and validated.

### Order Update

Cancel-and-replace pattern (Lighter does not support native order modification):

1. Build new order from existing order properties
2. Apply price/amount updates
3. Preserve `tpTriggerType` and `slTriggerType` if present
4. Cancel old order via `cancelOrders([order])`
5. Place new order via `placeOrder(newOrder)`

### Order Cancellation

1. Collect order IDs to cancel
2. Add all IDs to `recentlyCancelledOrders` map with current timestamp before API calls
3. Immediately remove orders from store
4. Chunk into groups of 10
5. Process chunks sequentially with 1-second delay between chunks
6. Within each chunk, cancel orders sequentially via `signerAdapter.cancelOrder(...)`
7. Each cancellation requires both `orderId` (as BigInt) and `marketId` (as int)
8. Reject non-numeric order ids before mutating the store. Hex transaction hashes are not cancellable order indexes and should surface an actionable "wait for open-order sync" error rather than a raw `BigInt` `SyntaxError` or a duplicate cancel-and-replace.

### Cancel Race Condition Handling

The `recentlyCancelledOrders` map (with 30-second TTL) prevents race conditions where:

- API still returns a cancelled order before processing completes
- WebSocket delivers order updates for recently cancelled orders
- Cleanup happens during `fetchOrders()`: entries older than 30 seconds are deleted.
- WebSocket handler also skips recently cancelled orders.

## Symbol Format

Perp markets use the symbol directly as returned from the API (for current Lighter perps this is usually the bare base, e.g. `BTC`, `ETH`, `LIT`). The market ID is a numeric string (e.g., `"0"`, `"1"`).

`/api/v1/orderBooks` also includes spot markets such as `ETH/USDC` with `market_type: "spot"` and high market ids. Do not expose those through `LighterExchange`; the exchange class is perps-only. Bare perp symbols should be modeled with the deployment's quote/margin currency internally (`USDC` for zkLighter, `USDG` for Robinhood), while explicit suffix symbols (`BTCUSDC`, `ETH/USDT`, `BTCUSDG`) should keep the parsed quote if Lighter returns them.

## Data Precision & Conversion

### BigInt / Decimal Scaling

Transaction payload values are scaled to integers before signing/submission:

- Price: `toLighterScaledInt(price, getLighterPriceDecimals(market))`
- Amount: `toLighterScaledInt(amount, getLighterAmountDecimals(market))`
- Decimals come from market metadata when available (`supported_price_decimals`, `supported_size_decimals`) and fall back to precision-derived decimals.

### Market Precision

Extracted from market data during `fetchMarketsHelper()`:

- `precision.amount` — Derived from `supported_size_decimals`
- `precision.price` — Derived from `supported_price_decimals`
- `limits.amount.min` — From `min_base_amount`
- `limits.amount.max` — From `order_quote_limit`
- `limits.leverage.max` — Derived from market metadata `min_initial_margin_fraction` when available; do not tighten this from account position `initial_margin_fraction`, which represents current leverage/margin state and can be much lower than the market cap
- `market.info.market_config.market_margin_mode` — Used as the flat-position display/default risk mode when no live position row has confirmed account state

`/api/v1/orderBookDetails` is the preferred source and `/api/v1/orderBooks` is only a fallback. Some active markets expose `market_config.market_margin_mode: 1` and reject cross-mode risk transactions. Keep `market_config.market_margin_mode` in `market.info`, but remember it is market configuration, not proof that the user's account has already applied that mode. If order placement later returns `invalid margin mode`, refresh position/account state and make the UI require an explicit re-apply of the intended mode instead of silently auto-switching or retrying.

### Utility Functions (`lighter.utils.ts`)

- `generateLighterClientId(accountIndex)` — Generates `lighter-{accountIndex}-{timestamp}-{random6digits}`
- `getPrecision(num)` — Counts decimal places in a number
- `dropPrefix(hash)` / `addPrefix(hash)` — Remove/add `0x` prefix
- `stringToBigInt(value)` / `bigIntToString(value)` — Safe BigInt conversion

### Balance Calculation (`lighter.exchange.ts`)

- `totalEquity` and `usedMargin` parsed from API response
- Unrealized PnL computed locally from positions: `(lastPrice - entryPrice) * contracts` for longs, inverse for shorts
- `total = subtract(totalEquity, upnl)` — Wallet balance without unrealized PnL
- `free` from API's `availableBalance`
- `fetchRawBalance()` also fetches `/api/v1/accountLimits` with a signer auth token and stores it at `balance.info.lighterAccountLimits` when available.
- Private account stream updates invalidate the short `/api/v1/account` snapshot cache and trigger a debounced balance refresh. Keep this path intact so Margin free/used/total updates do not wait for the next periodic tick or reuse pre-trade snapshots.
- The web Margin module reads `lighterAccountLimits.user_tier_name` / `user_tier` to display the Lighter account tier. Tiers are Standard, Premium, and Plus; they affect fees, order latency, and rate limits.
- `changeAccountTier(newTier)` posts to `/api/v1/changeAccountTier`, then invalidates/refetches account limits and balance.
- The Exchange settings UI exposes a Lighter-only tier selector. Lighter enforces no open orders/positions and a 24-hour cooldown between tier changes; the change is not documented as one-way.

## Rate Limiting

- Lighter has both IP and L1/account-scoped limits. Current public docs do not expose a stable error discriminator that tells us which bucket was exceeded.
- Do not assume rate-limit errors are IP-only. User-facing warnings should stay neutral unless we have stronger local context, for example cached account tier or degraded direct-connectivity state.
- When direct private WS falls into readonly/degraded mode or Lighter returns explicit policy/geo restriction signals, prefer georestriction/degraded-connectivity wording. Do not suggest self-heal or account-tier switching for that path; use VPN / Tealstreet WS proxy guidance instead.
- Standard accounts remain much tighter; Plus, Premium, and Builder tiers have higher weighted limits. Builder is documented at 240,000 weighted requests/minute. Premium `sendTx`/`sendTxBatch` capacity scales with staked LIT (currently 4,000-40,000/minute), and Premium also has volume quota for create/cancel-all/modify/grouped order transaction types.
- Rate-limit toasts are tier-aware via cached `/api/v1/accountLimits` only. Standard accounts can mention small limits and Plus/Premium guidance; Premium/Plus accounts should not be told to upgrade; unknown tier should keep neutral account/session wording. Do not fetch account limits from the toast path because the account may already be rate-limited.
- Private WS has explicit rate-limit handling:
  - server rate-limit detection (`too many websocket messages`, `rate limit`)
  - 60s cooldown window
  - delayed re-subscribe scheduling with buffer
  - rate-limited ping throttling
- Private WS rate-limit toasts are intentionally escalation-based. The first `too many websocket messages` / `rate limit` stream error is treated as self-healing: log it, enter cooldown, delay pings/subscriptions, and retry without `forceToast`. Only force-toast the neutral account/session guidance if rate-limit errors repeat after the cooldown window. Reset that escalation state once real private account stream activity resumes.
- Do not route transient startup rate-limit cooldowns through `forceToast`; first-session Lighter init can recover on its own and a forced toast creates noise without user action. Keep immediate `forceToast` behavior for degraded/geo/policy connectivity messages, auth/action-required failures, and private stream errors that are not the known self-healing rate-limit path.
- Public WS reconnect policy includes explicit rate-limit reconnect delay handling.
- Order-placement errors with HTTP `429`/`405`, rate-limit text, or volume-quota text normalize through the same tier-aware Lighter rate-limit guidance.
- Future improvement: preserve `sendRawTx` / `sendRawTxBatch` quota telemetry such as `volume_quota_remaining` from successful transaction responses. That could support a separate "trading quota low/exhausted" warning, but it should not be used to claim IP-vs-account REST/WS bucket unless Lighter exposes an explicit discriminator.
- `recvWindow`: 20,000ms (`RECV_WINDOW`)
- `tickInterval`: 2,000ms (polling interval between tick cycles)
- Lot order submissions: 100ms delay between orders
- Cancel batches: 1-second delay between groups of 10
- Order/position reconciliation fetches are throttled/debounced in private WS handlers.

## State Reconciliation

### Position Merging

The tick loop uses a time-based invalidation strategy:

1. `INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1_000ms`
2. If a WebSocket position update occurred within the last 1 second:
   - Uses `mergeUpdatesUsingLastUpdate()` to merge REST-fetched positions with store positions
   - Preserves fresh WebSocket data from being overwritten by stale REST data
3. Otherwise: REST-fetched positions replace store positions entirely

### Position Tracking

- Uses `PositionMode.OneWay`; Lighter positions are net per symbol.
- Placeholder (zero-size) one-way positions are created for all active markets.
- Placeholder margin type uses confirmed cached/position state first, then `market_config.market_margin_mode`, then cross as the last fallback. Do not treat market config as confirmed account mode.
- Isolated liquidation prices are sanity-checked. If Lighter reports a liquidation price on the profit side of entry for an isolated position, derive a safer display value from entry, size, allocated/fallback margin, and maintenance margin fraction.
- `uniqBy([...positions, ...fakePositions], 'id')` ensures real positions override placeholders
- Position IDs generated via `createPositionId(symbol, side, PositionMode.OneWay)`

As of 2026-05-23, Lighter should not be modeled as hedged in Tealstreet. Signed create-order payloads expose `isBuy`/`isAsk` and `reduceOnly`, but no `positionSide` or position-mode field. A live test confirmed that a non-reduce opposite-side sell smaller than the current long reduced the long rather than opening a separate short.

### Order Reconciliation

- `fetchOrders()` returns the coalesced REST snapshot; the private snapshot and
  throttled WebSocket heal call `addOrUpdateOrdersFull()` after disposal guards.
- `recentlyCancelledOrders` map filters out orders pending cancellation
- Submission-time `placedOrderHints` preserve conditional intent (`StopLoss`/`TakeProfit`), side, reduceOnly, and TIF during WS/REST reconciliation by order ID.
- Conditional price mapping prioritizes positive `triggerPrice` / `trigger_price` over `limitPrice` / `price`; Lighter active-order snapshots use documented hyphenated types like `take-profit`, so normalize those before deciding which price to render.
- WebSocket order handler:
  - FILLED orders: Update status to `Closed`, emit `fill` event if fresh
  - CANCELLED/CANCELED: Remove from store via `removeOrders()`
  - OPEN/PENDING/PARTIALLY_FILLED: Add or update in store
  - Skips false-positive fills where price and amount are both 0 (replacement artifacts)

## Exchange Metadata

- `enabled`: `true`
- `enabledInDev`: `true`
- `enableInPreview`: `true`
- `defaultQuote`: `USDC`
- `marginTypes`: `[Cross, Isolated]`
- `positionModes`: `[OneWay]`
- `hidePositionModeSelector`: `true`
- `forceAccountPositionModeChangeOnExchangeItself`: `false`
- `leverageIsGlobal`: `true`
- `marginTypeIsGlobal`: `true`
- `positionModeIsGlobal`: `true`
- `forbidCors`: `true`
- `requiresCors`: `false`
- `tickInterval`: `2000` ms
- `orderbookTickDelay`: `100` ms
- `orderPendingUpdateDelay`: `1000` ms
- `leverageMagnets`: `[1, 2, 3, 5, 10, 15, 20, 25, 30, 50]`
- `supportsChaser`: `true`
- `supportsPartialStops`: `true`
- `supportsInfinityStops`: `false`
- `supportsFeeRates`: `true`
- `tvMaxGetBarFetches`: `10`
- `candleApiCountLimit`: `500`
- `disconnectWarningThreshold`: `60000` ms
- `recvWindow`: `20000` ms
- `TP/SL trigger types`: `MarkPrice`
- `supportsTrailingStopLoss`: `false`

## Testnet / Demo

### Network Configuration

**REST API**:

- **Livenet**: proxied through `https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy/https://mainnet.zklighter.elliot.ai`
- **Testnet**: proxied through `https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy/https://testnet-api.lighter.xyz`
- **Robinhood**: proxied through `https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy/https://api.rh.lighter.xyz`

**WebSocket** (direct + optional proxy route):

- **Direct Public (Livenet)**: `wss://mainnet.zklighter.elliot.ai/stream`
- **Direct Public (Testnet)**: `wss://testnet.zklighter.elliot.ai/stream`
- **Direct Public (Robinhood)**: `wss://api.rh.lighter.xyz/stream`
- **Direct Private (Livenet)**: `wss://mainnet.zklighter.elliot.ai/stream`
- **Direct Private (Testnet)**: `wss://testnet.zklighter.elliot.ai/stream`
- **Direct Private (Robinhood)**: `wss://api.rh.lighter.xyz/stream`
- **Proxy Public**: `wss://fsjnpy143pbz-tyo.tealstreet.io/lighter/stream?readonly=true`
- **Proxy Private**: `wss://fsjnpy143pbz-tyo.tealstreet.io/lighter/stream`
- **Proxy Public/Private (Robinhood)**: `wss://fsjnpy143pbz-tyo.tealstreet.io/lighter-rh/stream`
- These are containers on the Vultr tunnel hosts (AWS_EXIT step 5b); the hosted exchange server dials the same paths.

When `tealstreetWsRoute` is enabled, both public and private Lighter WebSockets start on the Tealstreet proxy route. Public ticker/orderbook priming must not wait for direct-route failure promotion, because the order form and ticks chart depend on those public streams even when private connectivity is proxied. Keep rare fallbacks available for proxy-infra development: public ticker REST fallback should run while proxy WS is slow/down, and direct public WS should only be probed after repeated proxy-route failures before returning to proxy if that probe also fails.

### Chain IDs

- **Mainnet**: `42161`
- **Testnet (Sepolia)**: `421614`
- **Robinhood L1 EVM chain**: `4663`
- Note: these are network IDs; transaction signer domain IDs used by `TransactionClient` are `304` (livenet), `300` (testnet), and `466324` (Robinhood Chain Lighter).

### Known Differences

- Testnet is **disabled** in metadata (`supportsTestnet: false`), though URLs are defined
- Signer domain ID selection is based on `resolveLighterDeploymentProfile({ exchangeName, testnet })`, not only `options.testnet`.
- `lighter-robinhood` is a separate deployment with its own REST/WS hosts, USDG collateral, proxied REST by default, and optional Tealstreet WS proxy routing through `/lighter-rh/stream`.

## Known Quirks / Implementation Notes

1. **WASM dependency**: The Poseidon crypto module requires WASM initialization before any signing can occur. The exchange validates/warmups WASM during account validation and signer initialization.

2. **Order update = cancel + replace**: `updateOrder()` cancels the existing order and places a new one since Lighter does not support native order modification.

3. **Cancel race condition handling**: The `recentlyCancelledOrders` map prevents cancelled orders from being reintroduced while exchange snapshots lag cancellation processing. Entries are retained for 30 seconds.

4. **WebSocket position invalidation**: After a WebSocket position update, REST-fetched positions are merged with existing store positions for 1 second to avoid stale data overwriting fresh WebSocket updates.

5. **Lazy-loaded exchange**: Registered in `exchanges/registry.ts` with dynamic import and Webpack chunk name `exchange-lighter` due to heavy WASM crypto dependencies.

6. **Both CANCELLED and CANCELED spellings**: The code handles both `CANCELLED` and `CANCELED` status strings and both `USER_CANCELLED` and `USER_CANCELED` cancel reasons.

7. **Market and stop-order slippage**: Market orders are executed as slippage-bounded orders. Use a positive live reference price before applying 2% slippage; `mark` may lag initial ticker/orderbook data after startup, so fall back to side BBO, last, index, midpoint, or opposite-side BBO rather than signing price `0`. Lighter stop-loss/take-profit orders trigger against mark price; the signed create-order payload has no trigger-source field. Lighter stop-loss/take-profit orders also require a `price` guard: official docs state that this price is the allowed slippage/worst acceptable execution price after the trigger. The web Margin module exposes `lighterStopSlippagePct` (default 1%, max 50%) and `lighter.exchange.ts` converts it into the conditional order `price`.

8. **CORS forbidden**: The exchange metadata sets `forbidCors: true` and all REST traffic is routed through the Tealstreet exchange proxy.

9. **Leverage/margin-mode updates share signer path**: `setLeverage()` and `changeMarginType()` both use signed `tx_type = 20` (`update_leverage`) with per-market `marginMode` (`0` cross, `1` isolated). Margin-mode switches can be rejected with `21132` if the market has open positions or open orders. Some markets are configured for isolated risk (`market_config.market_margin_mode = 1`); if the user's account has stale cross state, create-order can reject with `21613 invalid margin mode`. Refresh state and surface an explicit re-apply instruction; do not auto-switch margin mode before order placement.

10. **OHLCV timestamp normalization + fallback**: Candle timestamps are normalized to internal seconds, and `listenOHLCV()` uses REST fallback polling + de-dup emission whenever WS candles go stale.

11. **SDK client order IDs**: `generateLighterClientId()` in `lighter.utils.ts` is not used in the main order flow. One synchronous instance allocator reserves positive 48-bit numeric indices for every single, split, bracket and batched leg. Frozen/backward time and overlapping sends cannot reuse an index within that instance; failed or unknown sends keep their reservations. Exhaustion rejects before the next send instead of wrapping. This is not a cross-device, cross-instance or restart uniqueness guarantee, and generated IDs are not retry authorization. Caller-supplied IDs remain unsupported.

12. **Ticker mark/index fallback**: If `markPrice` is not provided but `indexPrice` is, mark is set to index price. `market_stats` may identify markets by `market_id` without `symbol`; keep id-based ticker matching and do not overwrite existing prices with `0` when optional fields are omitted.

13. **Ticker REST fallback shutdown**: Do not stop public REST ticker fallback just because any WS frame arrived. Lighter can stream orderbook/candle/trade frames while ticker fields remain zero, especially through proxy/degraded routes; keep fallback polling until at least one ticker has a positive `mark`, `last`, `index`, `bid`, or `ask`.

14. **Ticker `daily_price_change` units**: Lighter `market_stats.daily_price_change` is already expressed in percentage points (for example `40.52` means `+40.52%`), not a decimal ratio. Do not multiply it by `100` when mapping to internal ticker `percentage`.

15. **Ticker funding units**: Lighter `market_stats.current_funding_rate` / `funding_rate` are percentage points per 1h funding period (for example `0.0012` means `0.0012%` hourly), while Tealstreet stores ticker `fundingRate` as an 8h decimal. Convert with `(rate * 8) / 100`; Lighter's own UI treats the displayed `0.0012%` hourly floor as `0.0100%` 8h.

16. **Banner/UX caveat (outside this directory)**: UI reconnect/account status logic is driven by socket-health last-message timestamps. Keep thresholds aligned with Lighter's quiet-stream windows (public ~120s, private reconnect ~180s, metadata warning ~60s) to avoid false-positive degraded status on low-traffic symbols. Open Positions should display explicit `ISO` / `CROSS` badges because Lighter risk mode can vary by market and stale account mode can block order placement.

## Error Handling

### API Response Errors

Two error interceptors handle API responses:

1. **Response interceptor**: Checks for `response.data.error` or `response.data.success === false`. Formats error as `[code] message` and throws with `emitAndLogError()`.
2. **Error interceptor**: Catches request errors and emits to exchange via `emitAndLogError()`.

### Specific Error Patterns

- **Account index not available**: private order sync returns existing store orders and logs a cooldown-gated warning until account index resolves
- **Signer unavailable**: order placement/cancellation/leverage updates throw actionable signer initialization/auth errors
- **`invalid auth: couldnt find account`**: authenticated order reconciliation returns existing store orders and logs one warning; do not let this fail the whole private snapshot because positions/balance can still update while the user fixes the account index/API key index/private key tuple
- **Market not found**: warning logged during cancellation target resolution; error thrown during placement
- **WASM not ready**: signer initialization retries once after explicit runtime warmup, then cooldown-gates further retries when needed
- **Stale browser WASM artifact**: If runtime throws `globalThis.poseidon2HashFp5 is not a function`, `/wasm/poseidon_crypto.wasm` is outdated versus SDK code. Ensure the updated binary exists at `apps/web/public/wasm/poseidon_crypto.wasm`.
- **Rate-limit ambiguity**: 429/405, WS rate-limit errors, and volume-quota messages are not enough to prove IP vs L1/account bucket. Keep user-facing copy neutral.
- **Transient REST transport misses**: public candle retries and periodic private snapshot `NO_RESPONSE` / timeout / network failures are expected during mobile sleep-wake and degraded Robinhood/direct routes. Suppress exchange-level Sentry emission for those paths and keep them as cooldown-gated warnings; payload-shape and venue validation errors should still emit normally.
- **Invalid margin mode on create-order**: Treat as stale or mismatched account risk state. Refresh positions/account snapshot, move flat placeholder display away from the stale market-config value when safe, and tell the user to explicitly re-apply the intended mode. Do not auto-switch or auto-retry the order.
- **Fill false-positive detection**: Skips FILLED events where both price and amount are <= 0 (replacement artifacts)

### Cancel Reason Handling

When an order is cancelled with a reason other than `USER_CANCELLED` or `USER_CANCELED`, the cancel reason is emitted as an error notification (only for fresh orders within 5 seconds of update, and 5+ seconds after connection).

## SDK Dependencies

### `@tealstreet/lighter-sdk`

- `LighterApiClient` — HTTP client with interceptor support and `sendRawTx()` method
- `OrderApi` — Order management API (`getAccountActiveOrders()`)
- `CandlestickApi` — Candlestick/OHLCV data API (`getCandlesticks()`)
- `TransactionClient` — Transaction builder and signer for ZK rollup operations

### `@tealstreet/poseidon-crypto`

- `initWasm()` — Initialize the WASM-based Poseidon hash module
- `isWasmReady()` — Check if WASM is initialized
- `getCryptoApi()` — Get the crypto API instance passed to `TransactionClient`

WASM initialization is performed during `validateAccount()` and again during `start()` if needed.

## Supported Timeframes

Raw Lighter requests support only `1m`, `5m`, `15m`, `30m`, `1h`, `4h`, `12h`, and `1d`. `fetchOHLCV()` and `listenOHLCV()` reject `3m`, `2h`, `6h`, and `1w` with an explicit error before making a Lighter request. TradingView continues to aggregate those display intervals client-side from supported base resolutions.

## Grafana Monitoring

**Every-request tracking**: `SEND_TX`, `SEND_TX_BATCH`
**Periodic tracking**: `ACCOUNT`, `ACCOUNT_LIMITS`, `OPEN_ORDERS`

## 2026-03 Integration Update

This section supersedes older notes that reference direct `transactionClient` usage in `lighter.exchange.ts`.

### Current signer path

- `lighter.exchange.ts` now uses `LighterSignerAdapter` from `lighter.signer.ts`.
- Provider: `tealstreet-sdk` only (internal `@tealstreet/lighter-sdk` + `@tealstreet/poseidon-crypto`).
- Exchange-side leverage and cross/isolated mode updates now use internal transaction signing (`tx_type = 20`) via `TransactionClient.getUpdateLeverageTransaction(...)`.
- Position mode is fixed to one-way in Tealstreet metadata (`positionModes: [OneWay]`) and the selector is hidden. No public Lighter API primitive is currently wired for one-way/hedged switching, and live testing on 2026-05-23 showed Lighter behaves as net-position only.

### Private WS authentication

- Private WS always subscribes to `account_all/<accountIndex>`.
- When available, `token` from `signerAdapter.getAuthToken(...)` is attached to `account_all_orders/<accountIndex>` subscribe payload.
- If token/channel is unavailable, orders continue reconciling via REST fallback.

### Integrator fee wiring

- Lighter order placement supports partner/integrator attribution on create-order transactions.
- Transport payload for integrator-attributed create orders uses official lighter-go shape:
  - `L2TxAttributes: { "1": IntegratorAccountIndex, "2": IntegratorTakerFee, "3": IntegratorMakerFee }`
  - Do not send duplicate top-level `Integrator*` fields together with `L2TxAttributes`.
- Signing semantics for create-order with integrator attrs use fp5 aggregation:
  - `baseFp5 = poseidon2HashFp5(baseCreateOrderPreimage)`
  - `attrFp5 = poseidon2HashFp5(l2AttributesPreimage)`
  - `aggregatedPreimage = baseFp5 + attrFp5`
  - `sig = schnorrSign(aggregatedPreimage, privateKey)`
- Current default runtime attribution is maker-free and taker-priced:
  - `IntegratorMakerFee = 0` (`0 bps`)
  - `IntegratorTakerFee = 250` (`2.5 bps`)
- Starting with Lighter's September 2026 partner-attribution rule, non-zero
  integrator fees require the client's account tier to be Plus or Premium. Order
  placement checks `/api/v1/accountLimits` when attribution is configured and
  blocks known Standard accounts before signing; sendTx rejection code `21520`
  is normalized to the same user-facing upgrade guidance for stale/unknown
  tier caches.
- `options.extra.lighterBuilderFeeMode` is retained as a legacy compatibility knob, but both current modes resolve to the same maker/taker fee split.
- Integrator index can be provided either:
  - globally via constants in `lighter.builder-fee.ts`, or
  - per-account via `options.extra.lighterIntegratorAccountIndex`.
- Integrator account index must be a numeric `accountIndex` value (not API key or API key index).
- `lighter-robinhood` uses the Robinhood-specific Tealstreet integrator account index `47296`. Do not reuse the ordinary zkLighter mainnet index (`158385`) for Robinhood.
- `ApproveIntegrator` flow now has local SDK signing + transport support (`tx_type = 45`).
- Existing/legacy saved Lighter accounts may predate Tealstreet's builder-fee approval flow. Do not assume an account is approved just because it can trade or has an API key.
- Lighter rejects attributed create-order transactions with `21149 integrator is not approved` when `L2TxAttributes` are present but the account has no valid approval for the configured integrator/fee caps.
- Lighter rejects Standard accounts for integrator approval with the same Plus/Premium account-tier policy. Approval helpers use authenticated `/api/v1/accountLimits` when the API private key is available, skip the check when it is not, and normalize `21520` responses from submission.
- Approval checks must validate expiry and each fee cap against the runtime maker/taker fee requirements, not merely that any non-zero approval exists. Existing approvals capped at `300` still cover the current `0` maker / `250` taker split.

### Account-form approval wiring

- New account-form approval helper component:
  - `apps/web/src/components/settings/accountsSettings/LighterBuilderFeeApproval.tsx`
- Flow uses shared helpers from:
  - `packages/safe-cex/src/exchanges/lighter/lighter.builder-approval.ts`
- Browser flow:
  - one-click wallet-sign + submit from account form (`approveLighterBuilderFeeWithBrowserWallet`).
- Desktop wrapper flow:
  - opens `/trade/lighter-l1sig` in system browser for L1 signature capture, then user pastes `L1Sig` back into account form and submits.
- Approval helper REST calls must not attach legacy `X-Timestamp` / `X-Account-Index` headers. `account` and `nextNonce` use documented query params, and the extra headers can trigger browser/proxy preflight failures that surface as "No response received from server" before the signature helper opens.
- `AccountForm` submit button is blocked while required Lighter approval is missing (same UX pattern as Hyperliquid/GRVT).
- Standard-tier accounts are the exception: account linking remains allowed, the approval helper shows a non-blocking Plus/Premium warning, and attributed orders are blocked later by `LighterExchange` until the user changes tier.
- After a Standard user upgrades to Plus/Premium, they still need to complete `ApproveIntegrator`. `LighterExchange.ensurePartnerAttributionApproved` catches this at trade time and throws `LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE`, which names Account Settings → Edit → Approve Builder Fee as the fix. `sendTx` also normalizes venue code `21149 integrator is not approved` to the same branded message as a defense in depth.
- After approval, refresh the account-form status and verify it shows approved before expecting attributed orders to succeed.

### ApproveIntegrator discovery status (current)

- The discovery runbook is `APPROVE_INTEGRATOR_DISCOVERY.md`.
- `tx_type = 45` and PascalCase `tx_info` contract are validated against upstream references.
- `systemConfig` endpoint confirms current fee caps:
  - perps maker/taker max: `1000`
  - spot maker/taker max: `10000`
- `nextNonce` rejects mismatched account/API-key pairs with `20001 invalid param`.
- `sendTx` currently rejects probe payloads with placeholder signatures as expected:
  - `21120 invalid signature`
- Signature generation/wiring is now implemented in SDK + account-form helper flow.
- **UNVERIFIED** (2026-04, pre-fp5): earlier testing observed `21120 invalid signature` when order integrator fee (`200`) was below the approved max (`300`). This was recorded before `poseidon2HashFp5` aggregation signing was added and may no longer be accurate. Needs re-testing with current fp5 signing path before treating as a constraint.

### Partner integration nuances (docs-aligned)

- Partner attribution is permissionless by account index.
- `IntegratorAccountIndex` is where partner fees accrue.
- `ApproveIntegrator` is submitted by the client account.
- L1 private key is only required for cross-L1 approval contexts; same-L1 flows can be L2-only.

### Dependency and CI safety

- CI smoke workflow is defined in `.github/workflows/lighter-signer-smoke.yml`.
- `test:lighter-smoke` includes metadata, signer, utilities, and private-WS close/backoff coverage.
- Workspace install remains lockfile-enforced in CI (`yarn --frozen-lockfile`).

### Internal signer upgrade procedure

1. Update `@tealstreet/lighter-sdk` internal signer code as needed.
2. Run:
   - `yarn workspace @tealstreet/safe-cex test:lighter-smoke`
   - `yarn workspace @tealstreet/safe-cex typecheck`
   - `yarn workspace @tealstreet/safe-cex lint`
   - focused Lighter batch/grouped-order tests when touching `placeOrders()` or `@tealstreet/lighter-sdk` transaction serialization.
3. Run manual dev checklist below with a small-size account.
4. Merge only after smoke + manual checks pass.

### Rollback procedure

1. Revert internal signer changes in `@tealstreet/lighter-sdk` / `lighter.signer.ts`.
2. Re-run smoke + typecheck + lint.
3. Re-run manual dev checklist to confirm stability.

### Manual dev validation checklist

Use a small funded account and minimum-safe size.

1. Start exchange in a small funded Lighter account.
2. For accounts that predate builder-fee approval, confirm the account-form approval status first. If it shows `Approve Builder Fee`, complete `ApproveIntegrator` and refresh until approved.
3. Place tiny limit order and confirm the successful `tx_type=14` payload includes `L2TxAttributes` for the configured integrator.
4. Cancel the same order immediately.
5. Place tiny market order.
6. Close with reduce-only market order.
7. Force/restart private websocket connection and verify:
   - orders reconcile correctly (no canceled-order resurrection),
   - conditional order type/price remain stable through WS + REST reconciliation (no limit downgrade for SL/TP),
   - account-all-orders duplicate subscribe errors (`30003`) are treated as benign,
   - positions reconcile correctly,
   - balance updates continue.
8. Confirm no nonce/auth errors are emitted in logs for place/cancel/auth flows.

## Client order ID

This venue does not honor `PlaceOrderOpts.clientOrderId`: `placeOrder` / `placeOrders` reject any caller ID before a write (`packages/safe-cex/CLAUDE.md` → "Client order ID contract"). Keep that guard as the first statement of any new placement override.

## Rust engine

`engine: 'rust'` selects `sdk-rs/venues/lighter` for both `lighter` and
`lighter-robinhood` perpetuals, with deployment-local hosts, quote currency and
signing chain. The Rust host forwards Application ID, API key index, integrator
index (as exact decimal text), `remoteSigner`, `extra.lighterSignerProvider`,
and `extra.lighterStopSlippagePct`. Missing API key index, or neither a local
secret nor `remoteSigner`, starts public-only data. A remote signer without a
local secret enables unsigned account snapshots and polling; it does not
provide transaction signing or a private socket. Remote-only native writes fail
when the signer is unavailable. Caller client order IDs are refused on every
shape and across the entire batch before any engine command/write.

Both sockets use engine timers for handshake, idle and reconnect watchdogs.
Private fills come exclusively from execution records, deduplicated by native
trade ID; a filled-order cumulative amount is never another execution. Live
executions during the first five seconds after connecting are retained, while
subscription history is suppressed. Settings use per-market confirmation clocks
and uncertain writes block dependent settings updates until fresh confirmation.

Rust history reads actual executions from `trades`, inactive orders from
`accountInactiveOrders`, and funding from `positionFunding`. It preserves exact
IDs, signed rebates and actual account-side realized PnL, converts native fee
rate ticks using `FeeTick = 1_000_000`, and fails incomplete cursor walks rather
than returning a successful empty/partial history. Execution results retain SDK
seconds; engine range inputs use integer milliseconds.

The signer supports current market IDs `0..32767` except reserved `255`; API key
indices remain `0..254`. Independent TS/Go-WASM vectors cover serialization,
preimages, builder fp5 aggregation, auth and the new high market IDs. Unused
first-port grouped-order/approval signer surfaces and their unused 32-byte hash
helpers are removed; the exchange places bracket siblings sequentially as TS
currently does. Deliberate TS fixes are listed in the port plan and replay gate.

## Rust position leverage and partial amendments

Position `initial_margin_fraction` is a percent string (`"20.00"` = 20% = 5x;
`"1.00"` = 100x, clamped to the market max); strings above 100 and integers are
1/10,000 ticks, floats are percents.
After a private reconnect, fills made during the outage are backfilled once from
`/api/v1/trades`; fills stamped ahead of the host clock are kept.
Public market risk fractions keep their separate tick-based interpretation.
Margin-mode writes preserve the parsed position leverage. Invalid market
bounds fail without panicking or replacing a known account snapshot.
Price-only cancel/recreate amendments use the observed remaining quantity;
explicit replacement amounts keep the caller's requested quantity. This fixes
the TS default that re-places the original amount after a partial execution.

## Rust shared public feeds

Shared public-feed snapshots revalue loaded native positions with the current
positive normalized mark, then last price, using the direct ticker handler's
PnL formula. The ticker mapper copies index into mark when mark is absent;
an explicit zero mark falls through to last. These are cumulative ticker
snapshots, so the fallback uses current native fields rather than which
optional fields appeared in a venue delta.
Public quotes do not mark private datasets loaded and do not change balance;
the next authenticated account snapshot calculates balance from native reported
PnL (or retained prior same-side PnL when omitted), preserving native equity/free.
Shared prices can still revalue displayed positions after that snapshot. Later
native REST ticker seeds cannot replace provider quotes and reapply position
valuations after restoration.

A partial Rust catalogue must not turn unknown owned exposure into an empty
successful account. A REST snapshot with an unmappable nonzero position, or
an open-order scope with an unmappable working order, warns and retains its
previous scope. Zero unknown positions and terminal unknown orders remain
ignorable. This bounded whole-scope guard precedes separate row-isolation work.

Rust private account valuation uses the native `positions[].unrealized_pnl` and
`position_value` when supplied (official model:
https://github.com/elliottech/lighter-python/blob/main/docs/AccountPosition.md).
Missing native PnL retains the prior finite valuation for the same market and
position side; a new unvalued exposure remains visible with zero unavailable PnL
and a warning. Realized PnL is never substituted. Aggregate balance sums the
preserved position PnL instead of calculating a fictitious entry loss from a
zero/missing ticker. Native account equity/free remain authoritative; a genuine
later public quote can update position PnL, which a subsequent private snapshot
without PnL retains. Explicit native PnL overrides conflicting public quotes.

## Rust reduce-only confirmation and rate errors

When a reduce-only limit lacks matching exposure, Rust waits for the account
read to complete with target size/absence evidence, with a10s deadline. The wait
belongs to one command and read serial. Settings-only retained positions cannot
confirm it, and a timed-out read cannot release the next queued order. Failed
confirmation sends no nonce/signature; prior accepted legs keep aggregate outcome
safety. HTTP405/429 retain native diagnostics and add neutral guidance without
replaying placement. An explicit21613 placement refusal refreshes account/orders
without guessing the opposite margin mode.
