# Polymarket Exchange Integration

This directory contains the Polymarket prediction market integration for Tealstreet.

## Exchange Information

Polymarket is a decentralized prediction market platform built on Polygon:

- Binary outcome markets (YES/NO tokens)
- Prices represent probabilities (0.00 - 1.00)
- No leverage (positions are 1:1 token holdings)
- Settlement based on real-world event outcomes
- USDC as the settlement currency
- ERC1155 token standard for outcome tokens

## API Documentation

**Official Documentation**: https://docs.polymarket.com/

### Key Endpoints

- **CLOB REST API**: `https://clob.polymarket.com`
- **Gamma API (metadata)**: `https://gamma-api.polymarket.com`
- **WebSocket (CLOB)**: `wss://ws-subscriptions-clob.polymarket.com`
- **WebSocket (Live data)**: `wss://ws-live-data.polymarket.com`
- **Polygon Chain ID**: 137

### Important API Documentation Sections

1. **Quick Start**: https://docs.polymarket.com/developers/CLOB/quickstart
2. **Orders**: https://docs.polymarket.com/developers/CLOB/orders/orders
3. **WebSocket**: https://docs.polymarket.com/developers/CLOB/websocket/wss-overview
4. **Authentication**: https://docs.polymarket.com/developers/CLOB/clients/methods-l2

### Key Differences from Perpetual Exchanges

| Aspect      | Perpetual Exchanges         | Polymarket                   |
| ----------- | --------------------------- | ---------------------------- |
| Market Type | Futures contracts           | Binary prediction markets    |
| Price Range | Variable                    | 0.00 - 1.00 (probability)    |
| Leverage    | 1x - 125x                   | None (1x only)               |
| Positions   | Long/Short with entry price | Token holdings (ERC1155)     |
| Settlement  | Continuous PnL              | Binary outcome on resolution |
| Signing     | API key/secret HMAC         | EIP-712 Ethereum signatures  |

## File Structure

```
polymarket/
├── polymarket.exchange.ts     # Main exchange class (PolymarketExchange)
├── polymarket.api.ts          # REST API client (PolymarketAPI) — dual CLOB + Gamma clients
├── polymarket.signing.ts      # EIP-712 order signing + HMAC L2 auth + wallet utilities
├── polymarket.ws-public.ts    # Public WebSocket handler (PolymarketPublicWebSocket)
├── polymarket.ws-private.ts   # Private WebSocket handler (PolymarketPrivateWebSocket)
├── polymarket.types.ts        # TypeScript type definitions (API responses, WS events, etc.)
├── polymarket.constants.ts    # API endpoints, type mappings, contract addresses, EIP-712 domain
└── CLAUDE.md                  # This documentation
```

## Authentication

Polymarket uses a two-tier authentication system:

### L1 (Public) — No Auth Required

- Market data endpoints (markets, prices, orderbook, price history, spreads, midpoints)
- Public trade history
- Determined by matching against `publicEndpoints` array in `polymarket.api.ts:84-96`

### L2 (Private) — API Credentials Required

- Order management (place, cancel, update)
- User positions and trades
- Balance queries

### Credential Flow

1. User provides Ethereum private key via `opts.key`
2. Wallet created via `ethers.Wallet` in constructor (`polymarket.exchange.ts:89`)
3. If no pre-configured credentials, key derives API credentials via `deriveApiCredentials()`:
   - Signs a message: `"I am signing this message for Polymarket API key derivation. Nonce: {nonce}. Timestamp: {timestamp}"` (`polymarket.signing.ts:257`)
   - Calls `GET /auth/derive-api-key` with `POLY_ADDRESS`, `POLY_SIGNATURE`, `POLY_TIMESTAMP`, `POLY_NONCE` headers
   - Returns `{ apiKey, secret, passphrase }` stored as `this.credentials`
4. Alternatively, credentials can be provided directly via `opts.polymarket.apiKey/apiSecret/apiPassphrase` (`polymarket.exchange.ts:98-113`)

### HMAC Signature Generation (L2 Headers)

Generated in `buildL2Headers()` / `generateHmacSignature()` (`polymarket.signing.ts:208-241`):

1. Build the message string: `{timestamp}{METHOD}{path}{body}`
2. Decode the secret from base64 to get the HMAC key
3. Compute HMAC-SHA256 using `ethers.computeHmac('sha256', key, message)`
4. Base64-encode the resulting signature
5. Timestamp is seconds since epoch, nonce is milliseconds since epoch

Headers sent:

- `POLY_ADDRESS`: API key (see note below)
- `POLY_SIGNATURE`: HMAC signature
- `POLY_TIMESTAMP`: Request timestamp (seconds)
- `POLY_NONCE`: Request nonce (milliseconds)
- `POLY_PASSPHRASE`: API passphrase

**Note on `POLY_ADDRESS` dual usage**: In `buildL2Headers()` (`polymarket.signing.ts:235`), `POLY_ADDRESS` is set to `credentials.apiKey` for authenticated HMAC requests. However, in the `deriveApiKey()` call (`polymarket.api.ts:468`), `POLY_ADDRESS` is set to the wallet address (not the API key), because credentials have not been derived yet at that point.

### EIP-712 Order Signing

Orders are signed using EIP-712 typed data (`polymarket.signing.ts:140-167`):

- Domain: `{ name: 'Polymarket CTF Exchange', version: '1', chainId: 137, verifyingContract: '0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E' }` (`polymarket.constants.ts:149-154`)
- Order type fields: `salt`, `maker`, `signer`, `taker`, `tokenId`, `makerAmount`, `takerAmount`, `expiration`, `nonce`, `feeRateBps`, `side`, `signatureType` (`polymarket.constants.ts:160-175`)
- Signed via `wallet.signTypedData(EIP712_DOMAIN, EIP712_ORDER_TYPES, message)`

### Signature Types

| Type               | Value | Description                       |
| ------------------ | ----- | --------------------------------- |
| `EOA`              | `0`   | Standard Ethereum wallet          |
| `POLY_PROXY`       | `1`   | Magic.link or email-based wallets |
| `POLY_GNOSIS_SAFE` | `2`   | Gnosis Safe multisig              |

Default is `EOA`. Configurable via `opts.polymarket.signerType`.

## Startup Sequence

The `start()` method (`polymarket.exchange.ts:193-230`) executes the following in strict sequential order:

1. **Fetch markets** via `fetchMarkets()` — calls Gamma API `GET /markets?active=true`
   - Populates `tokenToSymbol`, `symbolToToken`, and `marketMetadata` Maps
   - Creates two Market objects per prediction market (one for YES, one for NO token)
   - Checks `this.isDisposed` after each async step
   - Updates store: `{ markets, loaded.markets: true }`

2. **Fetch tickers** via `fetchTickers()` — calls CLOB API `GET /midpoints` in batches of 100 token IDs
   - Uses `tokenToSymbol` keys from step 1
   - All ticker fields (bid, ask, last, mark, index) set to the same midpoint price
   - Updates store: `{ tickers, loaded.tickers: true }`

3. **Start periodic tick** via `tick()` — first execution is awaited
   - Fetches positions, balance, and orders (see Tick section below)
   - Schedules subsequent ticks via `loop(() => this.tick(), 10_000)` (10-second interval)

4. **Fetch initial orders** (only if `this.credentials` is set)
   - Calls `GET /orders` via `fetchOrders()`
   - Updates store: `{ orders, loaded.orders: true }`

**Note**: WebSocket connections are NOT started in `start()`. The `dispose()` method has a TODO comment noting "WebSockets will be disposed here when implemented." The public and private WebSocket classes exist but are not wired into the startup flow.

### Tick Cycle (`polymarket.exchange.ts:235-276`)

Runs every 10 seconds via `loop()`. Only executes if `this.credentials` is set:

1. Fetch positions (trade-history-based calculation)
2. Fetch balance (USDC balance from contract)
3. Calculate `upnl` from positions via `sumBy(positions, 'unrealizedPnl')`
4. Fetch orders
5. On error, logs and continues (does not throw)

## REST API Endpoints

### Dual API Architecture

Two separate axios clients (`polymarket.api.ts`):

1. **CLOB API** (`this.xhr`): `https://clob.polymarket.com` — trading, orders, market data
2. **Gamma API** (`this.gammaXhr`): `https://gamma-api.polymarket.com` — market metadata, search, questions

### L1 Public Endpoints (CLOB)

| Endpoint            | Method | Path                  | Description                 |
| ------------------- | ------ | --------------------- | --------------------------- |
| Markets             | GET    | `/markets`            | All CLOB markets            |
| Simplified Markets  | GET    | `/simplified-markets` | Simplified market data      |
| Order Book          | GET    | `/book`               | Single token order book     |
| Order Books (batch) | POST   | `/book`               | Multiple token order books  |
| Midpoint            | GET    | `/midpoint`           | Single token midpoint price |
| Midpoints           | GET    | `/midpoints`          | Multiple token midpoints    |
| Price               | GET    | `/price`              | Token price for a side      |
| Prices              | GET    | `/prices`             | Multiple token prices       |
| Spread              | GET    | `/spread`             | Bid/ask spread              |
| Spreads (batch)     | GET    | `/spreads`            | Multiple token spreads      |
| Last Trade Price    | GET    | `/last-trade-price`   | Last trade price            |
| Price History       | GET    | `/prices-history`     | Historical prices           |

### L2 Private Endpoints (CLOB)

| Endpoint          | Method | Path                    | Description         |
| ----------------- | ------ | ----------------------- | ------------------- |
| Place Order       | POST   | `/order`                | Single order        |
| Place Orders      | POST   | `/orders`               | Batch orders        |
| Get Open Orders   | GET    | `/orders`               | List open orders    |
| Get Order         | GET    | `/order/{id}`           | Single order by ID  |
| Cancel Order      | DELETE | `/cancel`               | Cancel by order IDs |
| Cancel Orders     | DELETE | `/cancel-orders`        | Cancel multiple     |
| Cancel Market     | DELETE | `/cancel-market-orders` | Cancel by market    |
| Cancel All        | DELETE | `/cancel-all`           | Cancel all orders   |
| User Trades       | GET    | `/trades`               | User trade history  |
| Balance/Allowance | GET    | `/balance-allowance`    | USDC balance        |
| Derive API Key    | GET    | `/auth/derive-api-key`  | Derive credentials  |
| Delete API Key    | DELETE | `/auth/api-key`         | Revoke credentials  |

### Gamma API Endpoints

| Endpoint     | Method | Path                     | Description               |
| ------------ | ------ | ------------------------ | ------------------------- |
| List Markets | GET    | `/markets`               | All markets with metadata |
| Get Market   | GET    | `/markets/{conditionId}` | Single market             |
| By Slug      | GET    | `/markets/slug/{slug}`   | Market by URL slug        |
| Search       | GET    | `/markets?search=query`  | Search markets            |

## WebSocket Implementation

### Connection Architecture

Both public and private WebSockets connect to `wss://ws-subscriptions-clob.polymarket.com` (defined in `POLYMARKET_CONFIG.WS_CLOB`).

### Public WebSocket (`polymarket.ws-public.ts`)

**Channels handled:**

| Event Type     | Handler                 | Description                           |
| -------------- | ----------------------- | ------------------------------------- |
| `price_change` | `handlePriceChange`     | Token price updates (throttled 500ms) |
| `book`         | `handleOrderBookUpdate` | Order book snapshot/delta             |
| `pong`         | `handlePongEvent`       | Keepalive response                    |

**Subscription model:**

- Initial subscription: `{ type: 'MARKET', assets_ids: [...], markets: [...] }` (`polymarket.ws-public.ts:92-98`)
- Dynamic subscribe: `{ assets_ids: [tokenId], operation: 'subscribe' }` (`polymarket.ws-public.ts:176-183`)
- Dynamic unsubscribe: `{ assets_ids: [tokenId], operation: 'unsubscribe' }` (`polymarket.ws-public.ts:193-200`)

**Order book callbacks:** Managed via `orderbookCallbacks` Map (`Map<string, Set<(ob: OrderBook) => void>>`). Reference-counted: unsubscribes from asset only when last callback is removed (`polymarket.ws-public.ts:206-227`).

**Price change throttling:** Uses lodash `throttle` at 500ms with `{ leading: true, trailing: true }`. Updates store via `this.store.updateTicker()` with all price fields (bid, ask, last, mark, index) set to the same value.

### Private WebSocket (`polymarket.ws-private.ts`)

**Channels handled:**

| Event Type      | Handler             | Description                  |
| --------------- | ------------------- | ---------------------------- |
| `order`         | `handleOrderUpdate` | Order status changes         |
| `trade`         | `handleTradeUpdate` | Trade executions             |
| `authenticated` | _(inline)_          | Auth confirmation + snapshot |
| `error`         | _(inline)_          | Error messages               |
| `pong`          | `handlePongEvent`   | Keepalive response           |

**Authentication:** Sends `{ type: 'USER', auth: { apiKey, secret, passphrase } }` on connect (`polymarket.ws-private.ts:101-108`). On successful auth (`authenticated` event), calls `this.parent.fetchAndSetPrivateSnapshot()` to load initial state.

**Order update handling (`polymarket.ws-private.ts:193-216`):**

- Maps API order to internal Order via `mapOrder()`
- Calls `this.store.addOrUpdateOrders([order])`
- If order status is `'matched'`, emits a `'fill'` event

**Trade update handling (`polymarket.ws-private.ts:221-247`):**

- Resolves symbol from `tokenToSymbol` map (accessed via `(this.parent as any).tokenToSymbol`)
- Emits `'fill'` event with trade details
- Triggers `this.parent.fetchPositions()` to recalculate positions

### Reconnection Logic

Both WebSocket classes implement their own `onClose` handler:

- Clears ping interval
- If not disposed, schedules reconnection via `setTimeout(() => this.connectAndSubscribe(), WS_CONFIG.RECONNECT_DELAY)` (1 second delay)
- This is simpler than the `BaseWebSocket.onClose` pattern — direct reconnect without exponential backoff

### Ping/Pong

- Sends `{ type: 'ping' }` every `WS_CONFIG.PING_INTERVAL` (30 seconds)
- Uses recursive `setTimeout` pattern (not `setInterval`)
- Pong timeout configured at `WS_CONFIG.PONG_TIMEOUT` (10 seconds) but not actively enforced in code
- Max reconnect attempts: `WS_CONFIG.MAX_RECONNECT_ATTEMPTS` (10) — defined but not enforced with a counter

## Order Types and Mappings

All type mappings are defined in `polymarket.constants.ts`.

### OrderType Mapping

**Polymarket to Internal** (`ORDER_TYPE_MAP`):

| Polymarket API | Internal (`OrderType`) |
| -------------- | ---------------------- |
| `'GTC'`        | `OrderType.Limit`      |
| `'GTD'`        | `OrderType.Limit`      |
| `'FOK'`        | `OrderType.Market`     |
| `'FAK'`        | `OrderType.Market`     |

**Internal to Polymarket** (`ORDER_TYPE_REVERSE`):

| Internal (`OrderType`)       | Polymarket API |
| ---------------------------- | -------------- |
| `OrderType.Limit`            | `'GTC'`        |
| `OrderType.Market`           | `'FOK'`        |
| `OrderType.StopLoss`         | `'GTC'`        |
| `OrderType.TakeProfit`       | `'GTC'`        |
| `OrderType.TrailingStopLoss` | `'GTC'`        |

**Note**: StopLoss, TakeProfit, and TrailingStopLoss are not natively supported by Polymarket. They fall back to `'GTC'` (limit order behavior).

### OrderSide Mapping

| Polymarket API | Internal (`OrderSide`) |
| -------------- | ---------------------- |
| `'BUY'`        | `OrderSide.Buy`        |
| `'SELL'`       | `OrderSide.Sell`       |

**For EIP-712 signing** (`ORDER_SIDE_NUMERIC` in `polymarket.signing.ts`):

| Side     | Numeric Value |
| -------- | ------------- |
| `'BUY'`  | `0`           |
| `'SELL'` | `1`           |

### OrderStatus Mapping

| Polymarket API | Internal (string) | Maps to `OrderStatus`  |
| -------------- | ----------------- | ---------------------- |
| `'live'`       | `'open'`          | `OrderStatus.Open`     |
| `'matched'`    | `'closed'`        | `OrderStatus.Closed`   |
| `'cancelled'`  | `'canceled'`      | `OrderStatus.Canceled` |
| `'delayed'`    | `'open'`          | `OrderStatus.Open`     |

### Order Types on Polymarket

- **GTC** (Good-Till-Cancelled): Standard limit order
- **FOK** (Fill-Or-Kill): Must fill entirely or cancel
- **FAK** (Fill-And-Kill): Partial fills allowed, unfilled portion cancelled
- **GTD** (Good-Till-Date): Limit order with expiration

## Order Lifecycle

### Placing an Order (`polymarket.exchange.ts:536-576`)

1. Validate wallet and credentials exist
2. Resolve `tokenId` from `symbolToToken` map
3. Adjust price/amount using market precision (`pPrice`, `pAmount`)
4. Default price to `0.5` if not provided
5. Create signed order via `createSignedOrder()`:
   - Builds unsigned order data (`buildOrderData` in `polymarket.signing.ts:103-135`)
   - For EOA wallets, `maker` and `signer` are the same address
   - Signs with EIP-712 typed data
6. Post to `POST /order` with `{ order, owner: credentials.apiKey, orderType }`
7. Check `response.success` — throw if false
8. Return `[orderID]`

### Maker/Taker Amount Calculation (`polymarket.signing.ts:68-98`)

Both USDC and shares use 6-decimal precision internally:

**BUY order** (spending USDC to receive shares):

- `makerAmount = floor(price * size * 10^6)` (USDC in base units)
- `takerAmount = floor(size * 10^6)` (shares in base units)

**SELL order** (spending shares to receive USDC):

- `makerAmount = floor(size * 10^6)` (shares in base units)
- `takerAmount = floor(price * size * 10^6)` (USDC in base units)

### Order Defaults

- **Salt**: Random 256-bit value via `ethers.randomBytes(32)` (`polymarket.signing.ts:33-36`)
- **Taker**: `0x0000000000000000000000000000000000000000` (exchange operator fills)
- **Expiration**: 7 days from now (`calculateExpiration()` at `polymarket.signing.ts:49-53`)
- **Nonce**: `'0'` (simplified; see `generateNonce()` at `polymarket.signing.ts:41-43`)
- **Fee Rate**: `'0'` basis points (`POLYMARKET_CONFIG.FEES.DEFAULT_FEE_BPS`)

### Updating an Order (`polymarket.exchange.ts:581-599`)

Cancel-and-replace pattern:

1. Cancel existing order via `this.api.cancelOrder(order.id)`
2. Merge update fields with original order
3. Place new order via `this.placeOrder()`

### Canceling Orders

- **Single**: `DELETE /cancel` with `{ orderIds: [orderId] }` (`polymarket.api.ts:400-404`)
- **Multiple**: `DELETE /cancel-orders` with `{ orderIds }` (`polymarket.api.ts:409-413`)
- **By market**: `DELETE /cancel-market-orders` with `{ market }` (`polymarket.api.ts:418-422`)
- **All**: `DELETE /cancel-all` (`polymarket.api.ts:427-429`)

### No Bracket/TP/SL Support

Polymarket does not natively support stop loss, take profit, or trailing stop orders. The `ORDER_TYPE_REVERSE` mapping sends them as `'GTC'` limit orders, which means they will sit on the book but will not trigger based on price movement.

## Symbol Format

### Convention

`{market-slug}-{outcome}`

Examples:

- `will-bitcoin-hit-100k-Yes`
- `trump-wins-2024-No`

### Market Structure

- Each prediction market (from Gamma API) has a **condition ID** and a **slug**
- Two tokens per market: `Yes` and `No` (capitalized in `token.outcome`)
- Each token has a unique **ERC1155 token ID** (a long numeric string)

### Internal Mapping Caches (`polymarket.exchange.ts:75-79`)

1. **`tokenToSymbol`**: `Map<string, string>` — token ID to symbol
2. **`symbolToToken`**: `Map<string, string>` — symbol to token ID (reverse)
3. **`marketMetadata`**: `Map<string, GammaMarketInfo>` — condition ID to full metadata

### Market Data Fields

When creating `Market` objects in `fetchMarkets()` (`polymarket.exchange.ts:306-365`):

- `id` = token ID (ERC1155 token ID string)
- `symbol` = `{slug}-{outcome}`
- `base` = market slug
- `quote` = `'USDC'`
- `active` = `gm.active && !gm.closed`
- `contractSize` = 1
- `precision.amount` = 2 (shares), `precision.price` = 4 (0.0001 tick)
- `limits.amount.min` = first token's price or 0.01
- `limits.leverage` = `{ min: 1, max: 1 }`
- `info` = `{ conditionId, tokenId, outcome, question, description, endDate, slug }`

## Data Precision & Conversion

### Price Precision

- **Tick size**: 0.0001 (4 decimal places)
- **Range**: 0.00 to 1.00 (probability)
- Adjusted via `adjust(price, pPrice)` where `pPrice = market.precision.price || 4`

### Amount Precision

- **Step size**: 0.01 (2 decimal places for share quantities)
- Adjusted via `adjust(amount, pAmount)` where `pAmount = market.precision.amount || 2`

### USDC Decimals

- USDC on Polygon has **6 decimals**
- Balance: raw value divided by `1e6` (`polymarket.exchange.ts:285`)
- Order amounts: multiplied by `10^6` for EIP-712 signing (`polymarket.signing.ts:74-79`)
- Shares also use `10^6` precision internally for signing

### Price History

- API returns `{ t: number, p: string }` arrays
- Timestamp `t` is in seconds, converted to milliseconds: `h.t * 1000`
- Price `p` is a string, parsed with `parseFloat`
- All OHLCV fields (open, high, low, close) are set to the same price value
- Volume is always `0`

## Rate Limiting

Defined in `RATE_LIMITS` (`polymarket.constants.ts:217-226`):

| Endpoint Type | Rate Limit |
| ------------- | ---------- |
| Public        | 10 RPS     |
| Private       | 5 RPS      |
| Orders        | 5/second   |

### Implementation

Rate limiting is implemented manually in the request interceptor (`polymarket.api.ts:80-133`), not via `axios-rate-limit`:

1. Determine if request is public or private based on URL matching
2. Calculate minimum interval: `1000 / rateLimit` ms
3. If time since last request < min interval, sleep for the difference
4. Update `rateLimiter.lastRequestTime`

### Retry Strategy

Both CLOB and Gamma clients use `axios-retry` (`polymarket.api.ts:67-71, 202-206`):

- Retries: 3
- Condition: Network errors only (`isNetworkError`)
- Delay: Linear backoff (`retryCount * 1000` ms)

### Ticker Batching

`fetchTickers()` fetches midpoints in batches of 100 token IDs (`polymarket.exchange.ts:379-383`), avoiding hitting rate limits with large market lists.

## State Reconciliation

### Position Tracking from Trade History

Unlike perpetual exchanges that provide position endpoints, Polymarket positions are **calculated from trade history** (`polymarket.exchange.ts:422-495`):

1. Fetch all user trades via `GET /trades`
2. Iterate through trades, accumulating holdings per token ID in `holdingsMap`
3. For buys: add `size * price` to cost, add `size` to holdings
4. For sells: reduce cost proportionally using average price, reduce holdings
5. Only tokens with positive remaining size become positions
6. All positions are `PositionSide.Long` (holding tokens; shorting = selling tokens you own)
7. Position mode is always `PositionMode.OneWay`

**Cost basis tracking**: Maintained in `this.costBasis` Map (`Map<string, number>`), updated during `fetchPositions()`.

**Unrealized PnL**: `(currentPrice - avgPrice) * holding.size` where `currentPrice` comes from store tickers.

### Order State

Orders come from REST polling every 10 seconds in `tick()`, plus real-time WebSocket updates:

- WebSocket `order` events call `this.store.addOrUpdateOrders([order])` for immediate updates
- WebSocket `trade` events trigger `this.parent.fetchPositions()` to recalculate positions
- No deduplication logic — the store's `addOrUpdateOrders` handles merging

### Balance

- Fetched via `GET /balance-allowance`, returns `{ balance, allowance }` in raw USDC units
- All balance reported as `free` (available), `used` always `0` initially
- `upnl` calculated from positions via `sumBy(positions, 'unrealizedPnl')` in `tick()`

## Exchange Metadata

### No-Op Methods

The following methods are no-ops because prediction markets don't support these features:

| Method                     | Behavior                             | Reference                        |
| -------------------------- | ------------------------------------ | -------------------------------- |
| `setLeverage()`            | Logs warning, returns void           | `polymarket.exchange.ts:760-762` |
| `changePositionMode()`     | Logs warning, returns void           | `polymarket.exchange.ts:767-769` |
| `changeMarginType()`       | Logs warning, returns void           | `polymarket.exchange.ts:774-776` |
| `fetchSymbolMaxLeverage()` | Always returns `1`                   | `polymarket.exchange.ts:781-783` |
| `getPositionMode()`        | Always returns `PositionMode.OneWay` | `polymarket.exchange.ts:131-133` |

### Polygon Smart Contract Addresses

| Contract                     | Address                                      |
| ---------------------------- | -------------------------------------------- |
| CTF Exchange                 | `0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E` |
| USDC (Polygon)               | `0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174` |
| Conditional Tokens Framework | `0x4D97DCd97eC945f40cF65F87097ACe5EA0476045` |

### Fee Configuration

All fees are currently `0` (`polymarket.constants.ts:36-43`):

- `DEFAULT_FEE_BPS`: `'0'`
- `TAKER_FEE_BPS`: `0`
- `MAKER_FEE_BPS`: `0`

## Testnet/Demo

There is no separate testnet for Polymarket. Both `REST_BASE` and `WS_CLOB` point to production:

- REST: `https://clob.polymarket.com`
- WS: `wss://ws-subscriptions-clob.polymarket.com`
- Gamma: `https://gamma-api.polymarket.com`

All testing must be done on mainnet Polygon (Chain ID 137) with real USDC.

## Known Quirks / Implementation Notes

### Order Update is Cancel-and-Replace

`updateOrder()` cancels the existing order and places a new one (`polymarket.exchange.ts:581-599`). There is no native order amendment on Polymarket's CLOB.

### OHLCV Data is Point-in-Time

Polymarket's price history API returns point-in-time prices, not true OHLCV data. In `fetchOHLCV()` (`polymarket.exchange.ts:687-714`):

- Same price used for open, high, low, and close
- Volume is always `0`
- Fidelity parameter hardcoded to `60` (1 minute)

### Order Book Polling Fallback

`listenOrderBook()` (`polymarket.exchange.ts:658-682`) uses **REST polling every 2 seconds** as a fallback. The WebSocket `subscribeToOrderBook()` also provides real-time updates, but the exchange class method does not use it.

### WebSocket Not Wired to Startup

The `PolymarketPublicWebSocket` and `PolymarketPrivateWebSocket` classes exist and are functional, but `start()` does not call `connectAndSubscribe()` on either. The `dispose()` method has a TODO: "WebSockets will be disposed here when implemented."

### Private WebSocket Accesses Parent Internals

Both `PolymarketPrivateWebSocket.handleTradeUpdate()` and `PolymarketPrivateWebSocket.mapOrder()` access the parent's `tokenToSymbol` map via `(this.parent as any).tokenToSymbol`, breaking encapsulation (`polymarket.ws-private.ts:226, 253`). The same pattern appears in the public WebSocket (`polymarket.ws-public.ts:242`).

### Ticker Prices are Uniform

When fetching tickers, all price fields (bid, ask, last, mark, index) are set to the same midpoint value (`polymarket.exchange.ts:392-399`). There is no separate bid/ask spread in ticker data. Similarly, `percentage`, `fundingRate`, `volume`, `quoteVolume`, and `openInterest` are all `0`.

### Position `notional` Calculation

Position notional is `holding.size * currentPrice`. If no ticker exists for the symbol, `currentPrice` falls back to `avgPrice` (entry price).

### OHLCV Listener Polling

`listenOHLCV()` (`polymarket.exchange.ts:719-753`) uses REST polling every 5 seconds, fetching the last 60 seconds of data. Only new candles (by timestamp) are forwarded to the callback.

### Market `limits.amount.min`

Set to the first token's price or 0.01 fallback (`polymarket.exchange.ts:338`). This is somewhat arbitrary — it uses the price of the first token, not the actual minimum order size from the market data.

## Error Handling

### Response Interceptor (`polymarket.api.ts:136-167`)

**200-with-error pattern**: Checks `response.data?.error` — if present, creates an error with message `"Polymarket Error: {errorMessage}"` and rejects the promise.

**HTTP error responses**: Extracts `message` or `error` from `error.response.data`, creates enhanced error object with `Object.create(error)` and overrides the `message` property. Calls `exchange.log()` with the error.

### Order Placement Errors

`postOrder()` returns `{ success: boolean, errorMsg?: string }`. The caller in `placeOrder()` checks `response.success` and throws if false: `throw new Error(response.errorMsg || 'Order placement failed')` (`polymarket.exchange.ts:569-571`).

### Graceful Degradation in Tick

The `tick()` method wraps everything in try/catch (`polymarket.exchange.ts:270-272`). On error, it logs and continues to the next tick. This prevents a single API failure from stopping the polling cycle.

### Market/Ticker Fetch Failures

Both `fetchMarkets()` and `fetchTickers()` return empty arrays on error rather than throwing (`polymarket.exchange.ts:361-364, 411-413`). This allows the exchange to start even if initial data fetch partially fails.

### Credential Validation

`validateAccount()` (`polymarket.exchange.ts:149-168`) attempts to derive credentials if needed, then tests by fetching balance. Returns error message string on failure (not throwing), or empty string on success.

## Testing / Validation

No dedicated test files exist in this directory. Testing should cover:

1. **EIP-712 signing**: Use `verifyOrderSignature()` (`polymarket.signing.ts:271-302`) to validate signed orders
2. **HMAC generation**: Verify L2 headers match expected format
3. **Market mapping**: Ensure `tokenToSymbol`/`symbolToToken` maps are correctly populated
4. **Position calculation**: Verify cost basis math from trade history
5. **Order lifecycle**: Place, verify, cancel flow

## External References

- **Official Docs**: https://docs.polymarket.com/
- **Python Client**: https://github.com/Polymarket/py-clob-client
- **TypeScript Client**: https://github.com/Polymarket/clob-client
- **Real-time Client**: https://github.com/Polymarket/real-time-data-client
- **Gamma API**: https://gamma-api.polymarket.com
