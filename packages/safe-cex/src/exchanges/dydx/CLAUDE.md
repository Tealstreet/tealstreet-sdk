# dYdX v4 Exchange Integration

This directory contains the dYdX v4 exchange integration for Tealstreet.

## Exchange Information

dYdX v4 is a decentralized perpetual futures exchange built on the dYdX Chain (Cosmos-based), offering:

- Fully decentralized order book and matching engine
- Non-custodial trading with on-chain settlement
- High-performance trading with low latency
- Multiple order types (market, limit, stop, take profit)
- Cross-margin perpetual futures
- Subaccount-based isolated margin (subaccounts 0-127)

## API Documentation

**Official Documentation**: https://docs.dydx.xyz/

### Key Documentation Links

1. **Quick Start (TypeScript)**: https://docs.dydx.xyz/interaction/client/quick-start-ts
2. **REST API (Indexer)**: https://docs.dydx.xyz/indexer-client/http
3. **WebSocket API**: https://docs.dydx.xyz/indexer-client/websockets
4. **GitHub Repository**: https://github.com/dydxprotocol/v4-clients
5. **NPM Package**: https://www.npmjs.com/package/@dydxprotocol/v4-client-js

## File Structure

| File                 | Purpose                                                                                                                 |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `dydx.exchange.ts`   | Main exchange class extending `BaseExchange`. Order placement, cancellation, market/ticker/position fetching, tick loop |
| `dydx.api.ts`        | Axios instance for REST API calls to the Indexer. Network retry, error interceptor                                      |
| `dydx.sdk.ts`        | Wrapper around `@dydxprotocol/v4-client-js`. CompositeClient, wallet management, order/cancel operations                |
| `dydx.types.ts`      | All TypeScript types, enums, endpoint constants, WebSocket message interfaces                                           |
| `dydx.ws-public.ts`  | Public WebSocket: markets, orderbook, trades, candles, block height                                                     |
| `dydx.ws-private.ts` | Private WebSocket: subaccount updates (orders, positions, fills, balance)                                               |

## Authentication

dYdX v4 uses **mnemonic-based wallet authentication** via the Cosmos SDK:

### Credential Detection (`initializeDydxSDK`)

The `initializeDydxSDK()` function auto-detects the credential format:

- If `secret` contains 12+ whitespace-separated words, treated as a **mnemonic** and uses `LocalWallet.fromMnemonic()`
- Otherwise, treated as a **private key** (hex, with or without `0x` prefix) and uses `LocalWallet.fromPrivateKey()`

### Wallet Address Validation (`DydxSDKClient.initialize`)

After wallet creation, the derived address is compared against the configured address (`options.key`). A mismatch throws an error immediately.

### Configuration Fields

- **`key`** (API Key field): dYdX wallet address in bech32 format (e.g., `dydx1...`)
- **`secret`** (Secret field): Either a 12-word mnemonic phrase or a hex private key
- **`dydxAddress`** (optional): Separate trading address. If set, used for balance/position queries instead of `key`
- **`dydxSubaccountNumber`** (optional): Subaccount number, defaults to `0`

### Subaccounts

Each wallet can have subaccounts 0-127. The subaccount number is passed via `options.dydxSubaccountNumber` and defaults to `0`. Isolated margin is achieved by using different subaccounts.

### SDK Components

- **`CompositeClient`**: High-level client combining Node and Indexer clients. Primary interface for placing and cancelling orders.
- **`IndexerClient`**: Queries blockchain data (markets, orders, positions, balances) via REST
- **`ValidatorClient`**: Submits transactions (obtained from CompositeClient). Used for block height queries.
- **`LocalWallet`**: Wallet management from mnemonic or private key

## Startup Sequence

Defined by `DydxExchange.start()`:

```
1. fetchMarkets() - GET /v4/perpetualMarkets (sequential)
   -> store.update({ markets, loaded.markets: true })
2. fetchTickers() - GET /v4/perpetualMarkets again (sequential, same endpoint)
   -> store.update({ tickers, loaded.tickers: true })
3. publicWebsocket.connectAndSubscribe()
4. public-data-only: set empty orders/positions/balance loaded and return (no SDK, private WS, or tick loop)
5. private mode: validateAccount() if SDK is not initialized
   a. initializeDydxSDK() - creates wallet, CompositeClient, IndexerClient, ValidatorClient
   b. getSubaccount() - verifies subaccount exists (404 = empty account, treated as success)
   c. getUserFeeTier() - replaces display-only market fee defaults when available
6. privateWebsocket.connectAndSubscribe()
   a. setCredentials(address, subaccount)
   b. Subscribes to v4_subaccounts channel
7. tick() - initial private data fetch (sequential)
   a. fetchPositions() -> store.update({ positions })
   b. fetchBalance() -> store.update({ balance })
   c. fetchOpenOrders() [on first tick, count=0, since count%4===0]
   d. Schedules next tick in 10 seconds
```

Each step checks `this.isDisposed` before proceeding.

## REST API Endpoints

All endpoints are defined in `dydx.types.ts`.

### Markets

- `GET /v4/perpetualMarkets` - List all perpetual markets (used for both markets and tickers)
- `GET /v4/orderbooks/perpetualMarket/{ticker}` - Get orderbook
- `GET /v4/trades/perpetualMarket/{ticker}` - Get recent trades
- `GET /v4/candles/perpetualMarkets/{ticker}` - Get OHLCV data
- `GET /v4/historicalFunding/{ticker}` - Get funding rate history

### Account (Subaccount-specific, via SDK IndexerClient)

- `GET /v4/addresses/{address}` - Get all subaccounts for address
- `GET /v4/addresses/{address}/subaccountNumber/{subaccountNumber}` - Get specific subaccount
- `GET /v4/perpetualPositions` - Get perpetual positions
- `GET /v4/assetPositions` - Get asset holdings (USDC balance)
- `GET /v4/orders` - List orders with filtering
- `GET /v4/fills` - Get trade execution history
- `GET /v4/transfers` - Get transfer history
- `GET /v4/historicalPnl` - Get historical PnL
- `GET /v4/historical-funding` - Get funding payments

### Utility

- `GET /v4/time` - Get server timestamp
- `GET /v4/height` - Get current block height
- `GET /v4/screen` - Compliance screening

### Grafana Monitoring

- **Periodic endpoints**: `PERPETUAL_MARKETS`, `PERPETUAL_POSITIONS`, `ASSET_POSITIONS`, `ORDERS`
- **Every-tick endpoints**: None defined

## WebSocket Implementation

### Public WebSocket (`dydx.ws-public.ts`)

**Connection**: Uses `BaseWebSocket.createWebsocket()` to connect to `wss://indexer.dydx.trade/v4/ws` (or testnet equivalent).

**On Open** (`DydxPublicWebSocket.onOpen`):

1. Tracks last received timestamp
2. Clears the connection-local subscription set
3. Subscribes to `v4_markets`, `v4_block_height`, and per-market trades
4. Replays active orderbook/candle listener topics from `topics`
5. Emits WS reconnected event

**Subscription Tracking**: Uses a `Set<string>` to track active subscriptions by key (e.g., `v4_orderbook:BTC-USD`). Prevents duplicate subscriptions.

**Message Types**:

- `subscribed` / `unsubscribed`: Confirmation messages, logged
- `error`: Logged as error
- `channel_data`: Individual updates (used by block_height and custom handlers)
- `channel_batch_data`: Batched updates (markets, trades, orderbook, candles)

**Handler Registration**: The `messageHandlers` map uses topic keys like `v4_orderbook.BTC-USD` or `v4_candles.BTC-USD.1MIN`. Multiple handlers can register for the same topic.

**Orderbook**: Subscribes with `batched: true`. The `subscribed` frame carries
the initial `{price,size}` object snapshot; subsequent `channel_batch_data`
frames carry `[price,size]` tuple deltas. Each listener holds an isolated local
book and applies tuple deltas through `updateOrderBookByPrice`; size `"0"`
removes a level.

**Candles**: Subscribe format uses `{market}/{resolution}` as ID (e.g., `BTC-USD/1MIN`). The `listenOHLCV()` method polls until connected, then subscribes and registers a handler.

**Ping**: No-op. dYdX maintains connections automatically.

**Reconnection**: Handled by `BaseWebSocket`. On reconnect, `onOpen()` restores
the base channels, all per-market trade channels, and every active orderbook or
candle topic recorded in `topics`. The initial listener polling only establishes
the first subscription; reconnect replay is owned by `onOpen()`.

**Trades / last price**: trade frames are keyed by the WebSocket message `id`
(the market ticker), not the trade UUID. The handler emits the public fill and
updates `ticker.last` from each finite trade price.

### Private WebSocket (`dydx.ws-private.ts`)

**Connection**: Same WSS URL as public. No separate authentication required -- dYdX subaccount channel is public (anyone can subscribe to any address's subaccount data).

**Credentials**: Set via `setCredentials(address, subaccountNumber)` before connecting.

**On Open** (`DydxPrivateWebSocket.onOpen`):

1. Tracks last received
2. Clears subscription set
3. Subscribes to `v4_subaccounts` with ID `{address}/{subaccountNumber}`
4. Calls `this.parent.fetchAndSetPrivateSnapshot(10)` to fetch fresh REST data on reconnect

**Message Format**: Subaccount uses `channel_data` (NOT `channel_batch_data`).

**Subaccount Updates** (`DydxPrivateWebSocket.handleSubaccountUpdate`):

- Maps orders and positions into the store immediately
- Maps `subaccount.equity/freeCollateral` into exact balance state
- Emits normalized public `fill` events for fills
- Retains the raw compatibility events (`subaccount-update`, `order-update`,
  `fill-update`, `position-update`, `balance-update`)

**Ping**: No-op, same as public.

### WebSocket Channels

| Channel            | Constant                | ID Format                      | Message Type         | Batched |
| ------------------ | ----------------------- | ------------------------------ | -------------------- | ------- |
| Markets            | `v4_markets`            | None                           | `channel_batch_data` | Yes     |
| Trades             | `v4_trades`             | Ticker (e.g., `BTC-USD`)       | `channel_batch_data` | Yes     |
| Orderbook          | `v4_orderbook`          | Ticker                         | `channel_batch_data` | Yes     |
| Candles            | `v4_candles`            | `{ticker}/{resolution}`        | `channel_batch_data` | Yes     |
| Subaccount         | `v4_subaccounts`        | `{address}/{subaccountNumber}` | `channel_data`       | No      |
| Block Height       | `v4_block_height`       | None                           | `channel_data`       | No      |
| Parent Subaccounts | `v4_parent_subaccounts` | `{address}`                    | --                   | --      |

## Order Types and Mappings

### safe-cex to dYdX Mapping

| safe-cex `OrderType` | dYdX `DydxOrderType` |
| -------------------- | -------------------- |
| `Market`             | `MARKET`             |
| `Limit`              | `LIMIT`              |
| `Conditional`        | `STOP_MARKET`        |
| `StopLoss`           | `STOP_MARKET`        |
| `TakeProfit`         | `TAKE_PROFIT_MARKET` |
| `TrailingStopLoss`   | `STOP_MARKET`        |

### dYdX to safe-cex Mapping

| dYdX `DydxOrderType` | safe-cex `OrderType` |
| -------------------- | -------------------- |
| `MARKET`             | `Market`             |
| `LIMIT`              | `Limit`              |
| `STOP_MARKET`        | `StopLoss`           |
| `STOP_LIMIT`         | `StopLoss`           |
| `TAKE_PROFIT_MARKET` | `TakeProfit`         |
| `TAKE_PROFIT_LIMIT`  | `TakeProfit`         |

### Time In Force Mapping

| safe-cex            | dYdX                                         |
| ------------------- | -------------------------------------------- |
| `GoodTillCancel`    | `GTT`                                        |
| `ImmediateOrCancel` | `IOC`                                        |
| `FillOrKill`        | `FOK`                                        |
| `PostOnly`          | `GTT` (POST_ONLY handled via execution flag) |

### Order Flags

| Flag          | Value | Meaning                                                   |
| ------------- | ----- | --------------------------------------------------------- |
| `SHORT_TERM`  | 0     | In-memory orders, expire in up to 20 blocks (~20 seconds) |
| `LONG_TERM`   | 64    | Stateful orders stored on blockchain                      |
| `CONDITIONAL` | 32    | Conditional/trigger orders                                |

### Order Status Mapping

| dYdX Status            | safe-cex Status |
| ---------------------- | --------------- |
| `OPEN`                 | `Open`          |
| `PARTIALLY_FILLED`     | `Open`          |
| `FILLED`               | `Closed`        |
| `CANCELED`             | `Canceled`      |
| `BEST_EFFORT_CANCELED` | `Canceled`      |
| `UNTRIGGERED`          | `Open`          |

## Order Lifecycle

### Order Placement (`DydxExchange.placeOrder`)

1. Validate SDK is initialized and market exists
2. Generate unique `clientId` via `generateClientId()` -- timestamp-based with counter
3. Convert order parameters (side, type, timeInForce)
4. Short-term MARKET and IOC/FOK LIMIT orders fetch current block height and set
   `goodTilBlock = currentHeight + 20`
5. Resting GTT LIMIT and CONDITIONAL orders use the stateful SDK path with a
   90-day `goodTilTimeInSeconds`; IOC/FOK LIMIT orders use the short-term wrapper
   and all other orders use the general wrapper. Conditional market orders use
   the SDK's required IOC/FOK execution enum rather than unsupported `DEFAULT`.
6. Returns `[clientId.toString()]` as the order ID

### Order Cancellation (`DydxExchange.cancelOrder`)

1. Extract `clientId` from `order.info.clientId` or `order.id`
2. Use the market ticker restored from the Indexer's `clobPairId` when the order
   was mapped
3. Read the original `orderFlags` preserved in `order.info`
4. Stateful cancels use a zero `goodTilBlock` plus the 90-day duration; short-term
   cancels use the current block height plus 10 blocks
5. Pass the ticker as the SDK `marketId`; the SDK resolves it to `clobPairId`

### Order Update (`DydxExchange.updateOrder`)

dYdX does **not** support native order modification. Implemented as cancel-and-replace:

1. Build new order from existing order with updates applied
2. Cancel old order via `cancelOrder()`
3. Place new order via `placeOrder()`

### Batch Operations

- `cancelOrders()`: Uses `forEachSeries` (sequential) from `p-iteration`. Each failure is logged but does not stop the batch.
- `cancelAllOrders()`: Fetches all open orders via REST, then cancels sequentially.
- `cancelSymbolOrders()`: Fetches open orders for symbol, then cancels sequentially.
- `placeOrders()`: Uses `forEachSeries` (sequential). Each failure is logged but does not stop the batch.

### Client ID Generation (`DydxExchange.generateClientId`)

```typescript
private generateClientId(): number {
  const now = Date.now();
  this.lastClientId = Math.max(now % 0xFFFFFFFF, this.lastClientId + 1);
  return this.lastClientId;
}
```

Uses modulo `0xFFFFFFFF` (uint32 max) of current timestamp, with monotonic increment guarantee via `Math.max`.

## Symbol Format

dYdX uses `BASE-QUOTE` format: `BTC-USD`, `ETH-USD`, `SOL-USD`.

- All markets are USDC-settled perpetual futures
- Symbol is used directly as both `market.id` and `market.symbol`

## Data Precision & Conversion

### Market Precision (`DydxExchange.fetchMarkets`)

Precision is derived from `tickSize` and `stepSize` fields in the market data:

- `precision.price` = `parseFloat(sourceMarket.tickSize)` -- the minimum price increment
- `precision.amount` = `parseFloat(sourceMarket.stepSize)` -- the minimum size increment

### Quantums and Subticks

dYdX uses **quantums** (smallest unit) and **subticks** for on-chain order representation:

- **Quantums**: Size in smallest units (`atomicResolution`)
- **Subticks**: Price in smallest price increments (`quantumConversionExponent`)
- The SDK's `CompositeClient` handles conversion to/from human-readable values automatically
- Market metadata fields: `atomicResolution`, `stepBaseQuantums`, `quantumConversionExponent`, `subticksPerTick`

### Balance Calculation (`DydxExchange.fetchBalance`)

- `total` comes from the Indexer subaccount's exact `equity`
- `free` comes from exact `freeCollateral` and is not clamped
- `used` is the non-negative difference between equity and free collateral
- Unrealized PnL is reconciled from mapped positions

### Leverage

Default max leverage is hardcoded to 20. `setLeverage()` is a no-op with a warning -- dYdX manages leverage through position sizing, not per-market API settings.

## Rate Limiting

### Axios Rate Limit

```typescript
this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 10 });
```

### REST API Retry

```typescript
retry(xhr, { retries: 3, retryCondition: isNetworkError });
```

Retries only on network errors (connection failures, timeouts), not HTTP errors.

### On-Chain Rate Limits

- **Short-term orders**: 200 order attempts per block (~200/second)
- **Long-term orders**: 2 orders per block (~2/second) or 20 per 100 blocks
- **Conditional orders**: Same as long-term orders

### Tick Interval

`tick()` runs every 10 seconds. Orders are refreshed every 4th tick (~40 seconds, `count % 4 === 0`).

## State Reconciliation

### REST Polling (`DydxExchange.tick`)

The `tick()` loop runs every 10 seconds:

1. **Every tick**: Fetches positions and balance from REST, updates store
2. **Every 4th tick**: Fetches open orders from REST, updates store
3. Unrealized PnL is calculated from positions via `sumBy(positions, 'unrealizedPnl')` and injected into balance

### WebSocket Updates

The initial `subscribed` snapshot and subsequent `channel_data` subaccount
updates are mapped immediately into `addOrUpdateOrders`,
`addOrUpdatePositions`, and exact balance state. Fill updates emit normalized
`fill` events. The REST tick remains the reconciliation safety net.

### Private Snapshot on Reconnect

When the private WebSocket reconnects, it calls
`this.parent.fetchAndSetPrivateSnapshot(10)` to immediately refresh private
data from REST, preventing stale state after a disconnection.

### Empty Account Handling

All private fetch methods handle HTTP 404 as "empty account" rather than an
error. This allows new or unfunded accounts to connect without errors.

## Exchange Metadata

- **Exchange Name**: Used as provided by `ExchangeName` parameter
- **Position Mode**: One-way only (`getPositionMode()` returns `PositionMode.OneWay`)
- **Margin Type**: Cross-margin by default; isolated via subaccounts
- **Contract Size**: Always 1
- **Trade History**: `supportsTradesHistory: false`; the adapter has no
  `fetchExecutions` or `fetchHistoricalTrades` implementation. Private fill
  events do not make a historical REST method available.
- **Fees**: Public-only markets expose clearly documented display defaults.
  Private startup queries `ValidatorClient.get.getUserFeeTier()` and replaces
  every market with the wallet's current maker/taker PPM values.

## Testnet / Demo

### URL Differences

| Component      | Production                               | Testnet                                       |
| -------------- | ---------------------------------------- | --------------------------------------------- |
| REST (Indexer) | `https://indexer.dydx.trade`             | `https://indexer.v4testnet.dydx.exchange`     |
| WebSocket      | `wss://indexer.dydx.trade/v4/ws`         | `wss://indexer.v4testnet.dydx.exchange/v4/ws` |
| Validator RPC  | `https://dydx-ops-rpc.kingnodes.com:443` | `https://test-dydx-rpc.kingnodes.com`         |
| Chain ID       | `dydx-mainnet-1`                         | `dydx-testnet-4`                              |
| Native Token   | `adydx` (18 decimals)                    | `adv4tnt` (18 decimals)                       |

Environment is selected via `options.testnet` boolean. The SDK's `Network.testnet()` / `Network.mainnet()` handles the full configuration.

### Known Testnet Differences

- Testnet uses different chain ID (`dydx-testnet-4` vs `dydx-mainnet-1`)
- Native token denomination differs (`adv4tnt` vs `adydx`)
- USDC IBC denomination is the same on both networks

### Test Credentials

Stored in `.apikey.json`:

```json
{
  "apiKey": "dydx1...",
  "secret": "word1 word2 ... word12",
  "testnet": true
}
```

## Known Quirks / Implementation Notes

1. **Tickers and Markets share an endpoint**: Both `fetchMarkets()` and `fetchTickers()` call the same `GET /v4/perpetualMarkets` endpoint. Tickers have many fields set to 0 (bid, ask, last, mark, index) because the markets endpoint does not provide live prices -- these are populated by WebSocket updates.

2. **Block height errors fail closed**: if the Validator client cannot return the
   current height, short-term placement/cancellation throws a clear error and does
   not manufacture a timestamp-shaped height.

3. **Cancellation is flag-aware**: Indexer order flags are preserved in
   `order.info`, so short-term, long-term, and conditional cancellations use their
   required expiry representation.

4. **Order symbols are restored from CLOB pair IDs**: the markets response seeds a
   reverse map from `clobPairId` to ticker; unknown pair IDs are skipped instead of
   creating uncancellable orders.

5. **Leverage is not configurable**: `setLeverage()` is a no-op that logs a warning. Leverage is managed through position sizing.

6. **Balance uses Indexer collateral fields**: `equity` and `freeCollateral`
   are the source of truth; the old position-value/10 estimate is retired.

7. **CLOB-pair reverse map**: the markets response builds the ticker lookup
   needed to map Indexer orders before cancellation.

8. **HTTP-over-WebSocket adapter commented out**: The tunnel adapter in
   `dydx.api.ts` is commented out.

9. **Public vs private WS use same URL**: Both WebSocket classes connect to the same Indexer WebSocket URL. dYdX does not require authentication for private subaccount data -- anyone can subscribe if they know the address.

10. **POST_ONLY handling**: `PostOnly` maps to `GTT`; the SDK `execution` flag
    carries the actual post-only instruction.

## Error Handling

### REST API Errors

**Response interceptor handles two error formats:**

1. **Body errors**: `response.data.errors` array -- messages are joined and thrown
2. **HTTP errors**: Status code errors -- extracts message from `data.errors[0].msg`, `data.message`, or `statusText`

### SDK Errors

Most SDK operations throw on failure. The exchange class wraps all SDK calls in try/catch and logs with `LogSeverity.Error`.

### 404 as Success

All private data fetches (balance, positions, orders) treat HTTP 404 as "empty account" -- not an error. This pattern appears in:

- `validateAccount()`
- `fetchBalance()`
- `fetchPositions()`
- `fetchOpenOrders()`

### Tick Error Resilience

The `tick()` loop logs errors and always schedules the next tick. Individual
failures do not break the polling cycle.

### Retry Strategy

Network errors on REST calls are retried 3 times via `axios-retry`.

## Testing / Validation

Fresh deterministic regressions belong in the package-level
`src/__tests__/audit-regressions/` directory. Do not commit credentialed or
network-dependent test scripts beside exchange runtime code.

### Validation Flow

`validateAccount()`:

1. Initializes SDK client
2. Attempts to fetch subaccount data
3. 404 = valid but empty account (logged as info)
4. Other errors = validation failure

## SDK Package

**Package**: `@dydxprotocol/v4-client-js` (v3.6.0)

The SDK is wrapped in `dydx.sdk.ts` which provides:

- `DydxSDKClient` class: Manages CompositeClient, IndexerClient, ValidatorClient, and wallet
- `initializeDydxSDK()`: Factory function that creates and initializes the client from `ExchangeOptions`
- `createWalletFromPrivateKey()`: Helper for hex key to LocalWallet conversion

## External References

- **CCXT REST Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/dydx.ts
- **CCXT WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/dydx.ts
- **dYdX Status**: https://status.dydx.exchange/
- **dYdX Discord**: https://discord.gg/dydx
- **dYdX Documentation**: https://docs.dydx.xyz/

## Implementation Status

The adapter remains disabled in exchange metadata. The July 2026 audit repairs
are covered by fresh deterministic regressions, but enabling dYdX is a separate
product decision and requires credentialed testnet/live acceptance before any
user-facing rollout.

### Regression-covered surfaces

- [x] REST API client with wallet-based authentication
- [x] Market data fetching (markets, tickers, orderbook, trades, candles)
- [x] Account management (balance, positions, account info)
- [x] Order management (place, cancel, update via cancel-and-replace)
- [x] Public WebSocket (orderbook, trades, tickers, candles)
- [x] Private WebSocket (orders, positions, balance updates)
- [x] Position management (fetch positions, real-time updates)
- [x] Proper error handling with LogSeverity levels

### Future Enhancements

- [ ] Transfer operations (between subaccounts, withdraw, deposit)
- [ ] Funding payments history
- [ ] Historical PnL tracking
- [ ] Vault operations (MegaVault deposits/withdrawals)
- [ ] Multi-subaccount support for isolated margin
