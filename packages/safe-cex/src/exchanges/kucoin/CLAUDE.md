# KuCoin Exchange Integrations

This directory contains KuCoin integrations for Tealstreet.

- `kucoin` targets KuCoin Futures linear perpetuals on `api-futures.kucoin.com`.
- `kucoinspot` targets KuCoin Spot trading on `api.kucoin.com`.

Keep the two variants separate. Spot uses dash-delimited symbols such as
`BTC-USDT`, one-way fake zero positions, no leverage, and the modern Spot HF
order endpoints (`/api/v1/hf/orders`). Futures uses contract symbols such as
`XBTUSDTM` and futures-only position/leverage semantics.

## KuCoin Spot Notes

Official docs used for `kucoinspot`:

- Announcements: https://www.kucoin.com/docs-new/rest/spot-trading/market-data/get-announcements
- Spot symbols: https://www.kucoin.com/docs-new/rest/spot-trading/market-data/get-all-symbols
- Spot tickers: https://www.kucoin.com/docs-new/rest/spot-trading/market-data/get-all-tickers
- Spot klines: https://www.kucoin.com/docs-new/rest/spot-trading/market-data/get-klines
- Spot HF orders: https://www.kucoin.com/docs-new/rest/spot-trading/orders/add-order
- Spot HF open orders by page: https://www.kucoin.com/docs-new/rest/spot-trading/orders/get-open-orders-by-page

Spot REST auth is the same API-key-v2 HMAC scheme as futures:

1. `KC-API-PASSPHRASE = base64(HMAC-SHA256(passphrase, secret))`.
2. `KC-API-SIGN = base64(HMAC-SHA256(timestamp + METHOD + endpointWithQuery + body, secret))`.
3. GET/DELETE query strings must be included in the signed endpoint; POST body
   must be JSON without extra spaces.

Spot WebSocket tokens are fetched from classic Spot/Margin endpoints:

- Public token: `POST /api/v1/bullet-public`
- Private token: `POST /api/v1/bullet-private`
- Connection URL: `<instanceServers[0].endpoint>?token=<token>&connectId=<timestamp>`
- Ping payload: `{ "id": "<timestamp>", "type": "ping" }`

Spot public topics:

- Ticker: `/market/ticker:all`
- Order book: `/spotMarket/level2Depth50:{symbol}`
- Klines: `/market/candles:{symbol}_{interval}`

Spot private topics:

- Orders: `/spotMarket/tradeOrdersV2`
- Balance: `/account/balance`

Unlike the futures socket, KuCoin Spot private WS does not send a follow-up
`login` payload after `bullet-private`; the private token authenticates private
topic subscriptions. Order and balance messages should be treated as triggers
for throttled REST refreshes because websocket payloads can be partial.

`kucoinspot` is intentionally dev-only for the web app via exchange metadata:
`enabled: false`, `enabledInDev: true`, and `enableInPreview: false`.
`apps/web` consumes these flags in the new API flow and account availability
helpers.

## KuCoin Futures Exchange Integration

The existing `kucoin.*` files target KuCoin Futures (linear perpetual) trading API.

## Exchange Information

| Property                                         | Value                    |
| ------------------------------------------------ | ------------------------ |
| `exchangeName`                                   | `kucoin`                 |
| `prettyName`                                     | `Kucoin`                 |
| `requiresPassphrase`                             | `true`                   |
| `requiresApplicationId`                          | `false`                  |
| `enabled`                                        | `false` (dev only)       |
| `enabledInDev`                                   | `true`                   |
| `supportsTestnet`                                | `false`                  |
| `supportsTradesHistory`                          | `false`                  |
| `orderbookTickDelay`                             | `60`                     |
| `supportsTrailingStopLoss`                       | `false`                  |
| `supportsChaser`                                 | `true`                   |
| `supportsReadOnlyKey`                            | `false`                  |
| `tickInterval`                                   | `500`                    |
| `tvMaxGetBarFetches`                             | `10`                     |
| `alwaysCalculateUnrealizedPnlFromLastPrice`      | `true`                   |
| `forceAccountPositionModeChangeOnExchangeItself` | `true`                   |
| `hideLightweightChartsPopovers`                  | `true`                   |
| `enforceWsTunnel`                                | `true`                   |
| `latencyDomain`                                  | `api-futures.kucoin.com` |

## API Documentation

- **Official API Documentation**: https://docs.kucoin.com/
- **Futures API Documentation**: https://docs.kucoin.com/futures/

## File Structure

- `kucoin.api.ts` - REST API client with HMAC-SHA256 signature, encrypted passphrase, HTTP-over-WebSocket adapter
- `kucoin.exchange.ts` - Main exchange class (`KucoinExchange`) extending `BaseExchange`
- `kucoin.types.ts` - TypeScript type definitions, endpoint constants, order/status/TIF mapping tables
- `kucoin.ws-private.ts` - Private WebSocket (token-based auth, private topic subscriptions, order/algo order events)
- `kucoin.ws-public.ts` - Public WebSocket (token-based connection, tickers, klines, orderbook)

## Authentication

### REST API Signature (API Key Version 2)

KuCoin uses HMAC-SHA256 with an encrypted passphrase (`kucoin.api.ts:12-73`):

1. **Encrypted Passphrase** (computed once at API creation time, not per-request):

   ```
   encryptedPassphrase = base64(HMAC-SHA256(passphrase, secret))
   ```

2. **Per-request Signature** (authenticated/private REST only):
   - Concatenate: `timestamp + METHOD + url + stringifiedBody`
   - If GET/DELETE has query params, `url` includes the query string
   - KuCoin requires the signature prehash URL to use the unencoded query
     content, even though the request URL itself is URL-encoded
   - `stringifiedBody` is empty string for GET requests
   - Sign: `base64(HMAC-SHA256(concatenated_string, secret))`

3. **Headers** (set on authenticated/private REST requests; public-data-only
   public REST requests omit `KC-API-*` credentials and signatures):

   | Header               | Value                                      |
   | -------------------- | ------------------------------------------ |
   | `KC-API-KEY`         | API key (set as default header)            |
   | `KC-API-SIGN`        | Base64 HMAC-SHA256 signature (per-request) |
   | `KC-API-TIMESTAMP`   | Millisecond timestamp from virtualClock    |
   | `KC-API-PASSPHRASE`  | Encrypted passphrase (set as default)      |
   | `KC-API-KEY-VERSION` | `'2'`                                      |
   | `X-CHANNEL-API-CODE` | Broker ID (`'3zexp'`)                      |
   | `Content-Type`       | `'application/json'`                       |

4. **HTTP-over-WebSocket**: Authenticated/private REST uses `axiosHttpOverWebsocketAdapter` for REST API tunneling through WebSocket connections (`kucoin.api.ts:43`). Public-data-only REST does not install the tunnel adapter.

### Private WebSocket Authentication

Private WebSocket auth is token-based. `POST /api/v1/bullet-private` returns a
token and dynamic endpoint; the private token authenticates subsequent private
topic subscriptions. Do not send a follow-up `login` payload after `welcome`.

## Startup Sequence

The `start()` method (`kucoin.exchange.ts:128-174`) follows this initialization order:

1. **Fetch markets**: Sequential call to `fetchMarkets()`, store with `loaded.markets = true`
2. **Dispose check**: Return early if disposed
3. **Fetch tickers**: Sequential call to `fetchTickers()`, store with `loaded.tickers = true`
4. **Dispose check**: Return early if disposed
5. **Connect WebSockets**: Both public and private WebSockets connect simultaneously (not awaited)
   - Public WS: Fetches token via POST, then connects
   - Private WS: Fetches token via POST, then connects
6. **Initial tick**: Calls `this.tick()` to fetch positions and balance (awaited)
7. **Dispose check**: Return early if disposed
8. **Fetch orders**: Sequential call to `fetchOrders()`, store with `loaded.orders = true`
9. **Logging**: Logs ready state

Key difference from other exchanges: Markets and tickers are fetched **sequentially**, not in parallel. WebSocket connections start after tickers but before private data. The tick loop begins during startup (not after all data loads).

## REST API Endpoints

Defined in `ENDPOINTS` object in `kucoin.types.ts`:

| Constant                     | Path                                  | Method | Description                 |
| ---------------------------- | ------------------------------------- | ------ | --------------------------- |
| `ACCOUNT` / `BALANCE`        | `/api/v1/account-overview`            | GET    | Account info and balance    |
| `MARKETS`                    | `/api/v1/contracts/active`            | GET    | Active markets/contracts    |
| `TICKERS`                    | `/api/v1/allTickers`                  | GET    | All ticker data             |
| `POSITIONS` / `POSITIONS_V2` | `/api/v1/positions`                   | GET    | Open positions              |
| `FETCH_SINGLE_ACCOUNT`       | `/api/v2/position`                    | GET    | Single position info        |
| `SYMBOL_LEVERAGE`            | `/api/v1/position/risk/limit`         | GET    | Leverage risk limits        |
| `ORDERS`                     | `/api/v1/orders`                      | GET    | Open orders (status=active) |
| `PLACE_ORDER`                | `/api/v1/orders`                      | POST   | Place order                 |
| `CANCEL_ORDERS`              | `/api/v1/orders`                      | DELETE | Cancel single order (/{id}) |
| `CANCEL_ALL_ORDERS`          | `/api/v3/orders`                      | DELETE | Cancel all orders           |
| `BATCH_ORDERS`               | `/api/v1/orders/multi`                | POST   | Batch order placement       |
| `ALGO_ORDERS`                | `/api/v1/stopOrders`                  | GET    | Fetch algo/stop orders      |
| `CANCEL_ALGO_ORDER`          | `/api/v1/stopOrders/{orderId}`        | DELETE | Cancel algo order           |
| `PLACE_ALGO_ORDER`           | `/api/v1/st-orders`                   | POST   | Place algo order            |
| `PLACE_TRAILING_STOP_ORDER`  | `/api/v1/stop-order`                  | POST   | Place trailing stop order   |
| `KLINE`                      | `/api/v1/kline/query`                 | GET    | Kline/candlestick data      |
| `SET_LEVERAGE`               | `/api/v2/changeCrossUserLeverage`     | POST   | Set cross leverage          |
| `SET_POSITION_MODE`          | `/api/v2/position/switchPositionMode` | POST   | Change position mode        |
| `SET_MARGIN_MODE`            | `/api/v2/position/changeMarginMode`   | POST   | Change margin mode (v2)     |
| `GET_PUBLIC_WS_TOKEN`        | `/api/v1/bullet-public`               | POST   | Get public WS token         |
| `GET_PRIVATE_WS_TOKEN`       | `/api/v1/bullet-private`              | POST   | Get private WS token        |

## WebSocket Implementation

### Dynamic Connection URLs

KuCoin WebSocket endpoints are **not hardcoded**. They are obtained dynamically via REST API token requests:

- **Public**: `POST /api/v1/bullet-public` returns `instanceServers[0].endpoint` and `token`
- **Private**: `POST /api/v1/bullet-private` returns `instanceServers[0].endpoint` and `token`
- **Connection URL format**: `<server.endpoint>?token=<token>&connectId=<timestamp>`

The first server from the `instanceServers` array is always used (`kucoin.ws-public.ts:55-56`, `kucoin.ws-private.ts:35-36`).

### Keepalive / Ping-Pong

KuCoin requires JSON-formatted ping messages (not raw text):

```json
{ "id": "<timestamp>", "type": "ping" }
```

Expected response: `{"type": "pong"}` (`kucoin.ws-public.ts:79-84`, `kucoin.ws-private.ts:62-66`).

After a pong, the next ping is scheduled in 5 seconds (via `BaseWebSocket.handlePongEvent()`). If pong is missed for 7.5 seconds, reconnection is triggered.

### Reconnection Logic

Reconnection is handled by `BaseWebSocket`:

- Uses `ReconnectingWebSocket` when `ENABLE_RECONNECTING_WEBSOCKET` is enabled
- On reconnect, public WS calls `emitWsReconnected()`
- On reconnect, private WS calls `parent.fetchAndSetPrivateSnapshot()` to resync state (`kucoin.ws-private.ts:57-58`)

### Public Channels

**Subscription format**:

```json
{
  "id": "<timestamp>",
  "type": "subscribe",
  "topic": "/contractMarket/ticker:<symbol1>,<symbol2>,...",
  "privateChannel": false,
  "response": true
}
```

| Topic Pattern                            | Description                               | Subscription   |
| ---------------------------------------- | ----------------------------------------- | -------------- |
| `/contractMarket/ticker:<symbols>`       | Ticker data (price, bid/ask, funding)     | All markets    |
| `/contractMarket/candle<interval>:<sym>` | Kline updates (on demand)                 | Per instrument |
| `/contractMarket/level2:<symbol>`        | Order book depth (snapshot + incremental) | Per instrument |

Tickers are subscribed on connect as a comma-separated list of all market symbols. Candle and orderbook channels are subscribed on demand.

**Message routing** (`kucoin.ws-public.ts:109-157`): All messages are JSON-parsed. Routing is based on the `topic` field prefix (`startsWith` checks), with `type` field used for pong detection.

### Private Channels

Token obtained via `POST /api/v1/bullet-private`. Subscribe flow (`kucoin.ws-private.ts:52-60`, `kucoin.ws-private.ts:179-204`):

1. `onOpen()` subscribes to the private topics and starts pinging
2. A `welcome` message can call `subscribe()` again; the implementation guards duplicate subscriptions

**Subscribed channels**:

| Topic                            | Description             |
| -------------------------------- | ----------------------- |
| `/contractMarket/tradeOrders`    | Order events            |
| `/contractMarket/advancedOrders` | Algo order events       |
| `/contract/positionAll`          | Position updates        |
| `/contractAccount/wallet`        | Account/balance updates |
| `/contract/marginMode`           | Margin mode updates     |
| `/contract/crossLeverage`        | Cross leverage updates  |

Each channel is subscribed individually with `privateChannel: true`.

## Order Types and Mappings

### Normal Orders

| Internal Type      | KuCoin API Value | Notes                              |
| ------------------ | ---------------- | ---------------------------------- |
| `OrderType.Limit`  | `limit`          | Requires `price` and `timeInForce` |
| `OrderType.Market` | `market`         |                                    |

### Algo Orders (TP/SL)

| Internal Type                | KuCoin Plan Type | Description                    |
| ---------------------------- | ---------------- | ------------------------------ |
| `OrderType.TakeProfit`       | `pos_profit`     | Full position TP (no amount)   |
| `OrderType.TakeProfit`       | `profit_plan`    | Partial TP (specific amount)   |
| `OrderType.StopLoss`         | `pos_loss`       | Full position SL (no amount)   |
| `OrderType.StopLoss`         | `loss_plan`      | Partial SL (specific amount)   |
| `OrderType.TrailingStopLoss` | `track_plan`     | Trailing stop with `rangeRate` |

Additional API-to-internal mappings: `tp` -> TakeProfit, `sl` -> StopLoss, `psl` -> StopLoss, `ptp` -> TakeProfit, `track` -> TrailingStopLoss.

### Time In Force

| Internal TIF        | KuCoin Value |
| ------------------- | ------------ |
| `GoodTillCancel`    | `GTC`        |
| `ImmediateOrCancel` | `IOC`        |
| `FillOrKill`        | `FOK`        |
| `PostOnly`          | `POST_ONLY`  |

Also maps from KuCoin response: `normal` -> GTC, `post_only` -> PostOnly

### Order Side Mapping

KuCoin uses descriptive side names that encode position direction:

| KuCoin Value           | Internal Side    | Description             |
| ---------------------- | ---------------- | ----------------------- |
| `open_long`            | `OrderSide.Buy`  | Open long position      |
| `open_short`           | `OrderSide.Sell` | Open short position     |
| `close_long`           | `OrderSide.Sell` | Close long position     |
| `close_short`          | `OrderSide.Buy`  | Close short position    |
| `reduce_close_long`    | `OrderSide.Sell` | Reduce close long       |
| `reduce_close_short`   | `OrderSide.Buy`  | Reduce close short      |
| `offset_close_long`    | `OrderSide.Sell` | Offset close long       |
| `offset_close_short`   | `OrderSide.Buy`  | Offset close short      |
| `burst_close_long`     | `OrderSide.Sell` | Liquidation close long  |
| `burst_close_short`    | `OrderSide.Buy`  | Liquidation close short |
| `delivery_close_long`  | `OrderSide.Sell` | Delivery close long     |
| `delivery_close_short` | `OrderSide.Buy`  | Delivery close short    |
| `buy_single`           | `OrderSide.Buy`  | One-way mode buy        |
| `sell_single`          | `OrderSide.Sell` | One-way mode sell       |
| `buy`                  | `OrderSide.Buy`  | Simple buy              |
| `sell`                 | `OrderSide.Sell` | Simple sell             |

### Order Status Mapping

| KuCoin Value                                                           | Internal Status        |
| ---------------------------------------------------------------------- | ---------------------- |
| `init` / `new` / `open` / `match` / `not_trigger` / `partially_filled` | `OrderStatus.Open`     |
| `filled` / `done`                                                      | `OrderStatus.Closed`   |
| `canceled`                                                             | `OrderStatus.Canceled` |

## Order Lifecycle

### Order Placement

`placeOrder` (`kucoin.exchange.ts:823-831`): Routes to either `formatAlgoOrder` -> `placeAlgoOrders` or `formatCreateOrder` -> `placeOrderBatch` based on order type.

**Normal orders** are placed individually despite the batch endpoint existing (`kucoin.exchange.ts:949-972`). Uses `unlimitedXHR` (no rate limit) for order placement.

**Algo orders** select endpoint based on presence of `rangeRate` field:

- With `rangeRate`: `PLACE_TRAILING_STOP_ORDER` (`/api/v1/stop-order`)
- Without: `PLACE_ALGO_ORDER` (`/api/v1/st-orders`)

### Update Order (Cancel-Replace)

KuCoin has **no PUT endpoint for orders**. `updateOrder` (`kucoin.exchange.ts:1055-1091`) implements cancel-and-replace:

1. Find related orders (same parent ID)
2. If compound order (has virtual TP/SL children): delegates to `updateVirtualTpSlOrder`
3. If algo order: delegates to `updateAlgoOrder`
4. Otherwise: cancel the old order, then place a new one with updated fields

`updateVirtualTpSlOrder` (`kucoin.exchange.ts:997-1053`):

1. Find parent, SL, and TP orders
2. Cancel the parent order
3. Reconstruct with updated price/amount/TP/SL
4. Place as new order

### Virtual TP/SL Order IDs

Limit orders with attached `stopLoss`/`takeProfit` create compound order IDs in `mapOrder` (`kucoin.exchange.ts:1253-1312`):

- `<orderId>__stop_loss` for the stop loss leg
- `<orderId>__take_profit` for the take profit leg

The virtual orders have the **inverse side** of the parent order (if parent is Buy, TP/SL are Sell).

### Stop Orders via mapOrder

`mapOrder` (`kucoin.exchange.ts:1253-1312`) handles stop order detection:

- If `o.stop === true` and `o.stopType === 'down'`: maps to `OrderType.StopLoss`
- If `o.stop === true` and `o.stopType === 'up'`: maps to `OrderType.TakeProfit`
- If `o.stop === true` and `o.stopPriceType === 'TP'`: maps to `OrderType.TrailingStopLoss`
- Price is taken from `stopPrice` for stop orders

### Plan Type Selection

`getOrderPlanTypeAndAmount` (`kucoin.exchange.ts:1372-1398`) determines whether TP/SL targets the full position or a partial amount:

- If `amount === 0`: always uses position-level plan (`pos_loss`/`pos_profit`)
- If `amount > 0` but smaller than the opposite position's contracts: uses partial plan (`loss_plan`/`profit_plan`)
- Otherwise: uses position-level plan (infinite stop)

### Cancellation

Both normal and algo orders are cancelled **individually** (no batch cancellation):

- Normal orders: `DELETE /api/v1/orders/{orderId}` (`kucoin.exchange.ts:737-762`)
- Algo orders: `DELETE /api/v1/stopOrders/{orderId}` (`kucoin.exchange.ts:764-780`)
- Virtual TP/SL orders are resolved to their parent ID before cancellation
- `cancelSymbolOrders`: Uses `DELETE /api/v1/orders` with symbol param (`kucoin.exchange.ts:782-797`)
- All cancellations use `unlimitedXHR` (no rate limit)

### Algo Order Side Logic

`getAlgoOrderSide` (`kucoin.exchange.ts:1343-1360`):

- TP/SL orders: Side is inverted (Buy->short, Sell->long, representing the hold side)
- Trailing stops: Uses `close_short`/`close_long` format

### Trailing Stop Distance Calculation

`formatAlgoOrder` for trailing stops (`kucoin.exchange.ts:918-936`):

1. Calculates absolute price distance between current ticker `last` and trigger price
2. Converts to percentage: `Math.round(((priceDistance * 100) / ticker.last) * 10) / 10`
3. Sets as `rangeRate` on the payload

## Symbol Format

- **KuCoin Format**: `BTCUSDTM` or `BTCUSDT_UMCBL` (suffixed)
- **Internal Format**: `BTCUSDT` (concatenated, no suffix)
- **Conversion**: `symbol.split('_')[0]` strips the `_UMCBL` suffix
- Market `id` field stores the KuCoin format, `symbol` stores normalized
- **Testnet**: Uses `SUSDT` margin coin and `sumcbl` product type (via `apiMarginCoin` and `apiProductType` getters)
- **Market filtering**: Only markets where `quote === apiMarginCoin` are included (`kucoin.exchange.ts:446`)

## Data Precision & Conversion

### Precision Calculation

`getPrecisionFromTickSize` (`kucoin.exchange.ts:450-464`): Converts tick size to decimal places count:

- For decimal numbers (e.g., `0.001`): counts significant decimal places after trimming trailing zeros
- For whole numbers (e.g., `5`, `10`): returns `0`

Both price and amount precision use this method:

```
pricePrecision = getPrecisionFromTickSize(parseFloat(m.tickSize))
sizePrecision = getPrecisionFromTickSize(parseFloat(m.lotSize))
```

### Price and Amount Rounding

All prices and amounts are rounded using `adjust()` from `safe-math.ts`:

- `price = adjust(opts.price, pPrice)` where `pPrice` is `market.precision.price`
- `amount = adjust(opts.amount, pAmount)` where `pAmount` is `market.precision.amount`

### Contract-size units: store outputs are BASE, order input is NATIVE

**KuCoin Futures sizes are integer CONTRACT counts** (`currentQty` /
`size` / `filledSize` / `matchSize`), where `1 contract = market.contractSize`
base units (e.g. XBTUSDTM `multiplier = 0.001` → 1 contract = 0.001 BTC). The
rest of the app (web `getPositionNativeSize` = `contracts / contractSize`,
direct-USD `notional`, base-size columns) expects **position/order sizes in
BASE units and `notional` in quote (USD)** — the OKX/BitMEX/BloFin convention.

So everything that **reads** a size from KuCoin and writes it into the store
must convert native→base via `contractSizeFor(internalSymbol)`:

- `mapPosition`: `contracts = |currentQty| * contractSize` (base),
  `notional = |currentQty| * contractSize * markPrice` (≈ exchange `markValue`),
  `extra.contractsNative = |currentQty|`.
- `mapOrder`: `amount/filled/remaining = nativeSize * contractSize`,
  `extra.remainingNative = nativeRemaining`.
- private-WS `fill`: `amount = matchSize * contractSize`.

Order **placement is the opposite direction**: `opts.amount` is NATIVE
contracts (the web size input is contract-primary when `contractSize !== 1`),
so `formatCreateOrder` sends `size = adjust(opts.amount, lotSize)` **unchanged**
— do NOT multiply order placement by `contractSize`. Any comparison that mixes
`opts.amount` (native) with `position.contracts` (base) must convert one side
(see `getOrderPlanTypeAndAmount` → `multiply(opts.amount, contractSizeFor(...))`).

**Regression guard:** omitting `contractSize` here is a silent 1000× bug on
XBTUSDTM — it inflates every USD/leverage/drawdown readout and makes the web
close path (`contracts / contractSize`) over-close by 1/contractSize. Only
masked in prod because KuCoin is dev-only (`enabled: false`). All `kucoin`
markets are linear (metadata filters to `quote === apiMarginCoin` USDT), so the
conversion is the simple linear `native * contractSize`; there is no inverse
path here.

## Rate Limiting

- **Primary API** (`xhr`): Rate-limited to 3 requests/second via `axios-rate-limit` (`kucoin.exchange.ts:68`)
- **Unlimited API** (`unlimitedXHR`): No rate limit, used for order placement/cancellation (`kucoin.exchange.ts:69`)
- **Timeout**: 20,000ms (configurable via `options.extra.recvWindow`, default `RECV_WINDOW = 20_000`)
- **Retry**: 3 retries on network errors via `axios-retry` (`kucoin.api.ts:41`)
- **Tick interval**: 500ms (from exchange metadata), configurable via `options.extra.tickInterval`

### Dual Axios Instances

The exchange maintains two Axios instances:

1. `xhr`: Rate-limited (3 RPS) for general queries (markets, tickers, positions, balance)
2. `unlimitedXHR`: Unrestricted for time-sensitive operations (order placement, cancellation)

## State Reconciliation

### Tick Loop

The `tick()` method (`kucoin.exchange.ts:176-223`) runs on configurable interval:

1. **Every tick**: Fetch positions, update via `addOrUpdatePositionsFull`
2. **Every 4th tick** (`count % 4 === 0`): Fetch balance
3. **Every tick**: Fetch orders (normal + algo + trailing stop)
4. Store updates are batched into a single `store.update()` call

The tick interval is configurable via `options.extra.tickInterval` (default from metadata: 500ms).

### WebSocket vs REST

- **Order events** (private WS): Processed immediately via `mapOrder`, then state is updated:
  - `filled`: Emits `fill` event, updates order status to Closed
  - `cancelled`: Updates order status to Canceled
  - `open`/`partially_filled`: Adds or updates order
- **Algo order events**: `cancel` -> Canceled, `triggered`/`active` -> add/update
- **No debounced re-fetch**: Unlike BloFin, KuCoin does NOT trigger a throttled full order re-fetch after WS events. State relies on WS events + periodic tick fetches.
- **Reconnection**: On private WS reconnect, `fetchAndSetPrivateSnapshot()` is called

### Fake Positions

`fetchPositions` with specific symbols (`kucoin.exchange.ts:330-351`):

- For symbols not in open positions, `fetchPositionMetadata` is called per-symbol
- Creates fake positions with `contracts=0` containing leverage, margin type, and position mode
- No deduplication (unlike BloFin)

### Balance Calculation

`fetchBalance` (`kucoin.exchange.ts:225-246`): Maps from KuCoin fields:

- `free` = `availableBalance`
- `used` = `orderMargin`
- `total` = `accountEquity`
- `upnl` = `unrealisedPNL`

## Testnet/Demo

Testnet is **disabled** in exchange metadata (`supportsTestnet: false`).

When testnet would be enabled:

- `apiMarginCoin` returns `'SUSDT'` instead of `'USDT'` (`kucoin.exchange.ts:80`)
- `apiProductType` returns `'sumcbl'` instead of `'umcbl'` (`kucoin.exchange.ts:76`)
- Market filtering uses `SUSDT` quote currency
- Private WS uses the same token-authenticated account topics as livenet

## Known Quirks / Implementation Notes

1. **Dynamic WebSocket URLs**: WS endpoints are not hardcoded. Obtained via POST to `bullet-public` / `bullet-private` which returns `instanceServers` with dynamic endpoints and tokens.
2. **No batch order support used**: Despite `BATCH_ORDERS` endpoint existing (`/api/v1/orders/multi`), orders are placed individually in a loop (`kucoin.exchange.ts:949-972`).
3. **Update order not supported**: KuCoin has no PUT endpoint for orders. `updateOrder` cancels the old order and places a new one (cancel-and-replace pattern).
4. **Algo order plan types**: TP/SL orders use different `planType` values depending on whether they target the full position (`pos_profit`/`pos_loss`) or partial amount (`profit_plan`/`loss_plan`).
5. **Virtual TP/SL orders**: Limit orders with attached `stopLoss`/`takeProfit` create compound order IDs: `<orderId>__stop_loss` and `<orderId>__take_profit`.
6. **Success code**: API success is indicated by `data.code === '200000'` (string, not number).
7. **Encrypted passphrase for REST only**: Passphrase must be HMAC-SHA256 encrypted for REST API headers. Private WS uses the `bullet-private` token and does not send the passphrase.
8. **Kline timestamps**: Futures REST kline `from`/`to` params and returned candle timestamps are in **milliseconds**. WS candle timestamps are seconds.
9. **Leverage per-side in isolated**: In isolated margin mode, long and short sides can have different leverage values. Code warns if they differ in one-way mode (`kucoin.exchange.ts:286-289`).
10. **Orders cancelled individually**: Both normal and algo orders are cancelled one by one (no batch cancellation).
11. **HTTP-over-WebSocket adapter**: Authenticated/private REST API uses `axiosHttpOverWebsocketAdapter` as the default adapter, tunneling HTTP requests through WebSocket (`kucoin.api.ts:43`). Public-data-only REST is not tunneled.
12. **Trailing stop disabled in metadata**: `supportsTrailingStopLoss: false` in exchange metadata, but the code has implementation for it.
13. **Welcome triggers guarded subscribe**: On private WS, receiving a `welcome` message calls `subscribe()` again, but duplicate subscriptions are guarded.
14. **Position mode change blocked with open positions**: `changePositionMode` explicitly checks for open positions and throws if any exist (`kucoin.exchange.ts:1195-1223`).
15. **Margin mode requires symbol**: `changeMarginType` throws if no symbol is provided, unlike exchanges with global margin mode (`kucoin.exchange.ts:1165-1193`).
16. **Fetch missing-position metadata**: Position metadata for zero-size/fake positions is fetched from `/api/v2/position/getMarginMode`, `/api/v2/position/getPositionMode`, and cross leverage from `/api/v2/getCrossUserLeverage`.
17. **Ticker percentage from WS**: WS ticker `priceChangePercent` is multiplied by 100 to convert from decimal to percentage (`kucoin.ws-public.ts:174`).
18. **Commented-out leverage fetch**: The `fetchCurrentLeverage` and `fetchLeverageBrackets` methods are fully commented out (`kucoin.exchange.ts:466-540`), suggesting a previous implementation that was replaced.
19. **Duplicate trailing stop fetch**: `fetchTrailingStopOrders` fetches from the same `ALGO_ORDERS` endpoint as `fetchAlgoOrders`, filtering by `stopType === 'trailing' || orderType === 'trailing'` -- may result in duplicate orders if not filtered out (`kucoin.exchange.ts:643-673`).
20. **Orderbook gap recovery**: Futures `/contractMarket/level2` deltas must line up with the previous sequence. If a higher non-contiguous sequence arrives, refetch the REST `/api/v1/level2/snapshot` snapshot instead of applying the delta to a stale local book.
21. **Broker ID**: `X-CHANNEL-API-CODE: '3zexp'` sent as a default REST header.

## Error Handling

### API Response Format

All KuCoin API responses follow this structure:

```json
{
  "code": "200000",
  "msg": "success",
  "data": { ... }
}
```

- `code === '200000'` indicates success (string comparison)
- Error responses have a different code and descriptive `msg`

### Error Recovery Patterns

- **`fetchBalance`**: Returns existing store balance on error (`kucoin.exchange.ts:230-233`)
- **`fetchOpenPositions`**: Returns empty array on error (`kucoin.exchange.ts:357-358`)
- **`fetchNormalOrders` / `fetchAlgoOrders`**: Returns empty array on error (`kucoin.exchange.ts:612-616`, `kucoin.exchange.ts:636-640`)
- **`fetchMarkets`**: Returns empty array on error (`kucoin.exchange.ts:411-413`)
- **`fetchTickers`**: Returns empty array on error (`kucoin.exchange.ts:545-548`)
- **`start()`**: Wrapped in try/catch, logs error but does not throw (`kucoin.exchange.ts:129-173`)
- **`tick()`**: Wrapped in try/catch, logs error and continues to next tick (`kucoin.exchange.ts:217-219`)

### Order Error Propagation

- `placeOrderBatch`: Emits error event and throws on failure (`kucoin.exchange.ts:963-968`)
- `cancelNormalOrders` / `cancelAlgoOrders`: Emit error and re-throw (`kucoin.exchange.ts:755-760`, `kucoin.exchange.ts:775-779`)
- `updateOrder` / `updateAlgoOrder`: Emit error and re-throw (`kucoin.exchange.ts:1087-1090`, `kucoin.exchange.ts:1114-1116`)

## Kline Interval Mapping

| Internal | KuCoin   |
| -------- | -------- |
| `1m`     | `1min`   |
| `3m`     | `3min`   |
| `5m`     | `5min`   |
| `15m`    | `15min`  |
| `30m`    | `30min`  |
| `1h`     | `1hour`  |
| `2h`     | `2hour`  |
| `4h`     | `4hour`  |
| `6h`     | `6hour`  |
| `12h`    | `12hour` |
| `1d`     | `1day`   |
| `1w`     | `1week`  |

### OHLCV Fetching

- Max limit: 1000 candles per request (default 500)
- REST kline params: `symbol`, `granularity`, `from` (milliseconds), `to` (milliseconds)
- Candle array format: `[timestamp, open, high, low, close, volume]` (index 5 is volume)
- REST timestamps are in milliseconds (divided by 1000 when mapped internally)
- WS candle timestamps are seconds

## Grafana Monitoring

- **Every request** (logged individually): All trading, leverage, position mode, margin mode, and algo order endpoints
- **Periodic** (aggregated): Account, balance, positions, orders, fetch single account

## Position Handling

- Supports both One-Way (`NORMAL_MODE`) and Hedged (`HEDGE_MODE`) position modes
- In one-way mode: `buy_single` / `sell_single` for order sides
- In hedge mode: `open_long` / `close_short` / `open_short` / `close_long`
- Position side determined from `currentQty` sign (positive = Long, negative = Short)
- `fetchPositionMetadata` uses the v2 margin mode, position mode, and cross leverage endpoints
- Margin type and position mode are derived from positions, not global settings

## CCXT Reference Implementation

- **REST API**: https://github.com/ccxt/ccxt/blob/master/ts/src/kucoin.ts
- **WebSocket**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/kucoin.ts
- **Futures REST API**: https://github.com/ccxt/ccxt/blob/master/ts/src/kucoinfutures.ts
- **Futures WebSocket**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/kucoinfutures.ts
