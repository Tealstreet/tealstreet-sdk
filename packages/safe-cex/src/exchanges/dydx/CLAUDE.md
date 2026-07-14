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

### Credential Detection (`dydx.sdk.ts:294-326`)

The `initializeDydxSDK()` function auto-detects the credential format:

- If `secret` contains 12+ whitespace-separated words, treated as a **mnemonic** and uses `LocalWallet.fromMnemonic()`
- Otherwise, treated as a **private key** (hex, with or without `0x` prefix) and uses `LocalWallet.fromPrivateKey()`

### Wallet Address Validation (`dydx.sdk.ts:60-64`)

After wallet creation, the derived address is compared against the configured address (`options.key`). A mismatch throws an error immediately.

### Configuration Fields

- **`key`** (API Key field): dYdX wallet address in bech32 format (e.g., `dydx1...`)
- **`secret`** (Secret field): Either a 12-word mnemonic phrase or a hex private key
- **`dydxAddress`** (optional): Separate trading address. If set, used for balance/position queries instead of `key` (`dydx.exchange.ts:88-90`)
- **`dydxSubaccountNumber`** (optional): Subaccount number, defaults to `0`

### Subaccounts

Each wallet can have subaccounts 0-127. The subaccount number is passed via `options.dydxSubaccountNumber` and defaults to `0`. Isolated margin is achieved by using different subaccounts.

### SDK Components (`dydx.sdk.ts:29-81`)

- **`CompositeClient`**: High-level client combining Node and Indexer clients. Primary interface for placing and cancelling orders.
- **`IndexerClient`**: Queries blockchain data (markets, orders, positions, balances) via REST
- **`ValidatorClient`**: Submits transactions (obtained from CompositeClient). Used for block height queries.
- **`LocalWallet`**: Wallet management from mnemonic or private key

## Startup Sequence

Defined in `start()` at `dydx.exchange.ts:248-301`:

```
1. validateAccount() [if SDK not initialized]
   a. initializeDydxSDK() - creates wallet, CompositeClient, IndexerClient, ValidatorClient
   b. getSubaccount() - verifies subaccount exists (404 = empty account, treated as success)
2. fetchMarkets() - GET /v4/perpetualMarkets (sequential)
   -> store.update({ markets, loaded.markets: true })
3. fetchTickers() - GET /v4/perpetualMarkets again (sequential, same endpoint)
   -> store.update({ tickers, loaded.tickers: true })
4. publicWebsocket.connectAndSubscribe() - connects WS, subscribes to markets + block_height
5. privateWebsocket.connectAndSubscribe() [if !publicDataOnly && key exists]
   a. setCredentials(address, subaccount)
   b. Subscribes to v4_subaccounts channel
6. tick() - initial private data fetch (sequential)
   a. fetchPositions() -> store.update({ positions })
   b. fetchBalance() -> store.update({ balance })
   c. fetchOpenOrders() [on first tick, count=0, since count%4===0]
   d. Schedules next tick in 10 seconds
```

Each step checks `this.isDisposed` before proceeding.

## REST API Endpoints

All endpoints are defined in `dydx.types.ts:24-46`.

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

### Grafana Monitoring (`dydx.types.ts:49-55`)

- **Periodic endpoints**: `PERPETUAL_MARKETS`, `PERPETUAL_POSITIONS`, `ASSET_POSITIONS`, `ORDERS`
- **Every-tick endpoints**: None defined

## WebSocket Implementation

### Public WebSocket (`dydx.ws-public.ts`)

**Connection**: Uses `BaseWebSocket.createWebsocket()` to connect to `wss://indexer.dydx.trade/v4/ws` (or testnet equivalent).

**On Open** (`dydx.ws-public.ts:64-70`):

1. Tracks last received timestamp
2. Clears subscription set (in case of reconnect)
3. Calls `subscribe()` which subscribes to `v4_markets` and `v4_block_height`
4. Emits WS reconnected event

**Subscription Tracking**: Uses a `Set<string>` to track active subscriptions by key (e.g., `v4_orderbook:BTC-USD`). Prevents duplicate subscriptions.

**Message Types**:

- `subscribed` / `unsubscribed`: Confirmation messages, logged
- `error`: Logged as error
- `channel_data`: Individual updates (used by block_height and custom handlers)
- `channel_batch_data`: Batched updates (markets, trades, orderbook, candles)

**Handler Registration**: The `messageHandlers` map uses topic keys like `v4_orderbook.BTC-USD` or `v4_candles.BTC-USD.1MIN`. Multiple handlers can register for the same topic.

**Orderbook**: Subscribes with `batched: true`. Updates are delivered as `channel_batch_data` and dispatched to registered handlers via `handleOrderbookUpdate()`. The `listenOrderBook()` method creates per-listener state tracking via `orderBookInnerState[innerId]` with snapshot loading.

**Candles**: Subscribe format uses `{market}/{resolution}` as ID (e.g., `BTC-USD/1MIN`). The `listenOHLCV()` method polls until connected, then subscribes and registers a handler.

**Ping**: No-op (`dydx.ws-public.ts:608-610`). dYdX maintains connections automatically.

**Reconnection**: Handled by `BaseWebSocket`. On close, `onClose()` triggers reconnection logic. On reconnect, `onOpen()` re-subscribes to `v4_markets` and `v4_block_height`, but per-market subscriptions (orderbook, candles) are re-established by the `waitForConnectedAndSubscribe` polling in `listenOHLCV` and `listenOrderBook`.

### Private WebSocket (`dydx.ws-private.ts`)

**Connection**: Same WSS URL as public. No separate authentication required -- dYdX subaccount channel is public (anyone can subscribe to any address's subaccount data).

**Credentials**: Set via `setCredentials(address, subaccountNumber)` before connecting (`dydx.ws-private.ts:29-32`).

**On Open** (`dydx.ws-private.ts:47-59`):

1. Tracks last received
2. Clears subscription set
3. Subscribes to `v4_subaccounts` with ID `{address}/{subaccountNumber}`
4. Calls `this.parent.fetchAndSetPrivateSnapshot(10)` to fetch fresh REST data on reconnect

**Message Format**: Subaccount uses `channel_data` (NOT `channel_batch_data`).

**Subaccount Updates** (`dydx.ws-private.ts:164-193`):

- Emits raw `subaccount-update` event
- Iterates `data.orders` -> emits `order-update` for each
- Iterates `data.fills` -> emits `fill-update` for each
- Iterates `data.positions` -> emits `position-update` for each
- If `data.subaccount` present -> emits `balance-update`

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

### safe-cex to dYdX Mapping (`dydx.exchange.ts:126-136`)

| safe-cex `OrderType` | dYdX `DydxOrderType` |
| -------------------- | -------------------- |
| `Market`             | `MARKET`             |
| `Limit`              | `LIMIT`              |
| `StopLoss`           | `STOP_MARKET`        |
| `TakeProfit`         | `TAKE_PROFIT_MARKET` |
| `TrailingStopLoss`   | `STOP_MARKET`        |

### dYdX to safe-cex Mapping (`dydx.exchange.ts:141-152`)

| dYdX `DydxOrderType` | safe-cex `OrderType` |
| -------------------- | -------------------- |
| `MARKET`             | `Market`             |
| `LIMIT`              | `Limit`              |
| `STOP_MARKET`        | `StopLoss`           |
| `STOP_LIMIT`         | `StopLoss`           |
| `TAKE_PROFIT_MARKET` | `TakeProfit`         |
| `TAKE_PROFIT_LIMIT`  | `TakeProfit`         |

### Time In Force Mapping (`dydx.exchange.ts:157-168`)

| safe-cex            | dYdX                                         |
| ------------------- | -------------------------------------------- |
| `GoodTillCancel`    | `GTT`                                        |
| `ImmediateOrCancel` | `IOC`                                        |
| `FillOrKill`        | `FOK`                                        |
| `PostOnly`          | `GTT` (POST_ONLY handled via execution flag) |

### Order Flags (`dydx.types.ts:103-107`)

| Flag          | Value | Meaning                                                   |
| ------------- | ----- | --------------------------------------------------------- |
| `SHORT_TERM`  | 0     | In-memory orders, expire in up to 20 blocks (~20 seconds) |
| `LONG_TERM`   | 64    | Stateful orders stored on blockchain                      |
| `CONDITIONAL` | 32    | Conditional/trigger orders                                |

### Order Status Mapping (`dydx.exchange.ts:173-184`)

| dYdX Status            | safe-cex Status |
| ---------------------- | --------------- |
| `OPEN`                 | `Open`          |
| `PARTIALLY_FILLED`     | `Open`          |
| `FILLED`               | `Closed`        |
| `CANCELED`             | `Canceled`      |
| `BEST_EFFORT_CANCELED` | `Canceled`      |
| `UNTRIGGERED`          | `Open`          |

## Order Lifecycle

### Order Placement (`dydx.exchange.ts:746-815`)

1. Validate SDK is initialized and market exists
2. Generate unique `clientId` via `generateClientId()` -- timestamp-based with counter (`dydx.exchange.ts:77-82`)
3. Convert order parameters (side, type, timeInForce)
4. For LIMIT/MARKET orders: fetch current block height, set `goodTilBlock = currentHeight + 20`
5. Branch on order type:
   - **Short-term LIMIT** with `goodTilBlock`: calls `sdkClient.placeShortTermOrder()`
   - **All other types**: calls `sdkClient.placeOrder()` with full parameters
6. Returns `[clientId.toString()]` as the order ID

### Order Cancellation (`dydx.exchange.ts:858-892`)

1. Extract `clientId` from `order.info.clientId` or `order.id`
2. Look up `clobPairId` from `marketsCache`
3. Fetch current block height
4. Call `sdkClient.cancelOrder()` with `goodTilBlock: currentHeight + 10`
5. **Note**: Always uses `DydxOrderFlags.SHORT_TERM` (value 0) for cancellation

### Order Update (`dydx.exchange.ts:817-856`)

dYdX does **not** support native order modification. Implemented as cancel-and-replace:

1. Build new order from existing order with updates applied
2. Cancel old order via `cancelOrder()`
3. Place new order via `placeOrder()`

### Batch Operations

- `cancelOrders()`: Uses `forEachSeries` (sequential) from `p-iteration`. Each failure is logged but does not stop the batch.
- `cancelAllOrders()`: Fetches all open orders via REST, then cancels sequentially.
- `cancelSymbolOrders()`: Fetches open orders for symbol, then cancels sequentially.
- `placeOrders()`: Uses `forEachSeries` (sequential). Each failure is logged but does not stop the batch.

### Client ID Generation (`dydx.exchange.ts:77-82`)

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

### Market Precision (`dydx.exchange.ts:366-372`)

Precision is derived from `tickSize` and `stepSize` fields in the market data:

- `precision.price` = `parseFloat(sourceMarket.tickSize)` -- the minimum price increment
- `precision.amount` = `parseFloat(sourceMarket.stepSize)` -- the minimum size increment

### Quantums and Subticks

dYdX uses **quantums** (smallest unit) and **subticks** for on-chain order representation:

- **Quantums**: Size in smallest units (`atomicResolution`)
- **Subticks**: Price in smallest price increments (`quantumConversionExponent`)
- The SDK's `CompositeClient` handles conversion to/from human-readable values automatically
- Market metadata fields: `atomicResolution`, `stepBaseQuantums`, `quantumConversionExponent`, `subticksPerTick`

### Balance Calculation (`dydx.exchange.ts:448-528`)

- USDC balance found by matching `assetId === ASSET_TYPES.USDC` (IBC token address)
- Used margin estimated as `positionValue / 10` (assumes ~10x leverage average)
- Free balance = total - used, clamped to `Math.max(0)`
- Unrealized PnL calculated from positions and added in `tick()` (`dydx.exchange.ts:317-318`)

### Leverage

Default max leverage is hardcoded to 20 (`dydx.exchange.ts:389`). `setLeverage()` is a no-op with a warning -- dYdX manages leverage through position sizing, not per-market API settings.

## Rate Limiting

### Axios Rate Limit (`dydx.exchange.ts:68`)

```typescript
this.xhr = rateLimit(createAPI(opts, this), { maxRPS: 10 });
```

### REST API Retry (`dydx.api.ts:35`)

```typescript
retry(xhr, { retries: 3, retryCondition: isNetworkError });
```

Retries only on network errors (connection failures, timeouts), not HTTP errors.

### On-Chain Rate Limits

- **Short-term orders**: 200 order attempts per block (~200/second)
- **Long-term orders**: 2 orders per block (~2/second) or 20 per 100 blocks
- **Conditional orders**: Same as long-term orders

### Tick Interval

`tick()` runs every 10 seconds (`dydx.exchange.ts:336`). Orders are only refreshed every 4th tick (~40 seconds, `count % 4 === 0`).

## State Reconciliation

### REST Polling (`dydx.exchange.ts:303-338`)

The `tick()` loop runs every 10 seconds:

1. **Every tick**: Fetches positions and balance from REST, updates store
2. **Every 4th tick**: Fetches open orders from REST, updates store
3. Unrealized PnL is calculated from positions via `sumBy(positions, 'unrealizedPnl')` and injected into balance

### WebSocket Updates

Private WS emits events (`order-update`, `fill-update`, `position-update`, `balance-update`) which are consumed by the exchange class. However, the current implementation does not have explicit listeners on these emitter events in the exchange class -- reconciliation relies on the `tick()` polling loop.

### Private Snapshot on Reconnect

When the private WebSocket reconnects (`dydx.ws-private.ts:58`), it calls `this.parent.fetchAndSetPrivateSnapshot(10)` to immediately refresh private data from REST, preventing stale state after a disconnection.

### Empty Account Handling

All fetch methods (balance, positions, orders) handle HTTP 404 as "empty account" rather than error. This allows new/unfunded accounts to connect without errors (`dydx.exchange.ts:223-230`, `481-488`, `544-549`, `623-628`).

## Exchange Metadata

- **Exchange Name**: Used as provided by `ExchangeName` parameter
- **Position Mode**: One-way only (`getPositionMode()` returns `PositionMode.OneWay`)
- **Margin Type**: Cross-margin by default; isolated via subaccounts
- **Contract Size**: Always 1 (`dydx.exchange.ts:391`)
- **Fees**: Hardcoded taker=0.05%, maker=0.002% (`dydx.exchange.ts:392-393`)

## Testnet / Demo

### URL Differences (`dydx.types.ts:3-21`)

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

2. **Block height fallback is completely broken** (`dydx.exchange.ts:202`): If `sdkClient.getCurrentBlockHeight()` throws, the catch block returns `Math.floor(Date.now() / 1000)` as a "safe default". This is NOT a rough approximation — it's an entirely wrong value. `Date.now() / 1000` is seconds since 1970 (~1.75 billion in 2026), while dYdX block heights are in the ~60 million range. The fallback returns a number ~30× larger than any real block height. Every order placed while the fallback is active will compute `goodTilBlock = currentHeight + 20 ≈ 1.75e9`, which dYdX's validator rejects as out-of-window for short-term orders. Treat this fallback as "orders fail" rather than "orders might misbehave" — the only reason it hasn't burned users is that `getCurrentBlockHeight()` rarely throws in practice.

3. **Cancel always uses SHORT_TERM flag**: `cancelOrder()` hardcodes `DydxOrderFlags.SHORT_TERM` regardless of the original order's flags (`dydx.exchange.ts:866`). This means cancelling long-term or conditional orders may not work correctly.

4. **Order symbol uses clobPairId**: In `fetchOpenOrders()`, the order's `symbol` is set to `dydxOrder.clobPairId` rather than the market ticker (`dydx.exchange.ts:657`). This may cause symbol mismatch issues.

5. **Leverage is not configurable**: `setLeverage()` is a no-op that logs a warning. Leverage is managed through position sizing.

6. **Used margin is estimated**: Balance calculation estimates used margin as `positionValue / 10` (assuming ~10x average leverage), not from actual margin data (`dydx.exchange.ts:502`).

7. **Markets cache**: Raw market data is cached in `marketsCache` Map for `clobPairId` lookup during order cancellation (`dydx.exchange.ts:62`, `365`, `873`).

8. **HTTP-over-WebSocket adapter commented out**: The tunnel adapter in `dydx.api.ts:37-39` is commented out.

9. **Public vs private WS use same URL**: Both WebSocket classes connect to the same Indexer WebSocket URL. dYdX does not require authentication for private subaccount data -- anyone can subscribe if they know the address.

10. **POST_ONLY handling**: When `PostOnly` time-in-force is requested, it maps to `GTT` for the TimeInForce enum, and the actual POST_ONLY behavior is set via the `execution` flag in the SDK (`dydx.sdk.ts:116`).

## Error Handling

### REST API Errors (`dydx.api.ts:43-67`)

**Response interceptor handles two error formats:**

1. **Body errors**: `response.data.errors` array -- messages are joined and thrown
2. **HTTP errors**: Status code errors -- extracts message from `data.errors[0].msg`, `data.message`, or `statusText`

### SDK Errors

Most SDK operations throw on failure. The exchange class wraps all SDK calls in try/catch and logs with `LogSeverity.Error`.

### 404 as Success

All private data fetches (balance, positions, orders) treat HTTP 404 as "empty account" -- not an error. This pattern appears in:

- `validateAccount()` (`dydx.exchange.ts:223-231`)
- `fetchBalance()` (`dydx.exchange.ts:481-488`, `505-510`)
- `fetchPositions()` (`dydx.exchange.ts:544-549`)
- `fetchOpenOrders()` (`dydx.exchange.ts:623-628`)

### Tick Error Resilience

The `tick()` loop catches all errors silently and always schedules the next tick (`dydx.exchange.ts:332-336`). Individual failures do not break the polling cycle.

### Retry Strategy

Network errors on REST calls are retried 3 times via `axios-retry` (`dydx.api.ts:35`).

## Testing / Validation

Fresh deterministic regressions belong in the package-level
`src/__tests__/audit-regressions/` directory. Do not commit credentialed or
network-dependent test scripts beside exchange runtime code.

### Validation Flow

`validateAccount()` (`dydx.exchange.ts:203-246`):

1. Initializes SDK client
2. Attempts to fetch subaccount data
3. 404 = valid but empty account (logged as info)
4. Other errors = validation failure

## SDK Package

**Package**: `@dydxprotocol/v4-client-js` (v3.4.0)

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

### Completed Features

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
- [ ] Fix order symbol to use market ticker instead of clobPairId
- [ ] Support LONG_TERM and CONDITIONAL order flags in cancellation
