# Breakout Prop Exchange Integration

This directory contains the Breakout Prop trading platform integration for Tealstreet.

## Exchange Information

Breakout Prop is a proprietary trading (prop trading) platform that uses a DevExpress (DXtrade) backend, unlike typical crypto exchanges. It provides simulated trading environments for prop trading evaluations. There are no trading fees, no funding rates, and no liquidation prices exposed through the API. Position mode is always one-way with cross margin.

## API Documentation

- **DXtrade REST API docs**: `https://demo.dx.trade/developers/#/DXtrade-REST-API`
- **Base URL**: `https://breakout.prosp.devexperts.com/dxsca-web`
- **Broker ID**: `Tealstreet` (defined but not currently sent as a header -- see TODO at `breakoutprop.api.ts:29`)
- **Receive Window**: 20,000ms (default, configurable via `options.extra.recvWindow`)

## File Structure

| File                         | Description                                                                 |
| ---------------------------- | --------------------------------------------------------------------------- |
| `breakoutprop.api.ts`        | REST API client factory with session-based auth interceptor and retry logic |
| `breakoutprop.exchange.ts`   | Main exchange class (`BreakoutpropExchange`) extending `BaseExchange`       |
| `breakoutprop.types.ts`      | Constants, endpoint paths, response types, order/position mapping tables    |
| `breakoutprop.ws-private.ts` | Private WebSocket class (instantiated but **not connected** in `start()`)   |
| `breakoutprop.ws-public.ts`  | Public WebSocket class (polling-based -- `connectAndSubscribe` is a no-op)  |

## Authentication

### Session Token Flow

Breakoutprop uses a **session-based** authentication model (no HMAC signing for REST):

1. **Login** (`breakoutprop.exchange.ts:99-114`): POST to `/login` with `{ domain: "default", username: <apiKey>, password: <apiSecret> }`.
2. **Response**: `{ sessionToken: string, timeout: "00:30:00" }`.
3. **Caching**: Token is cached locally and reused until `sessionTokenExpiresAt` (set to `Date.now() + 30 * 60 * 1000`). The `getSessionToken()` method checks expiry before every API call and re-authenticates if needed.
4. **Header injection** (`breakoutprop.api.ts:49-67`): An axios request interceptor attaches the token:
   - Via Tealstreet tunnel: `Authorization: DXAPI <sessionToken>`
   - Direct connection: `Coinbase-Authorization: DXAPI <sessionToken>`
5. **Guard**: Requests to endpoints other than `/login` throw `new Error('No session token')` if `sessionToken` is null (`breakoutprop.api.ts:50-52`).

### Account Discovery

After login, the exchange calls `getAccountCode()` (`breakoutprop.exchange.ts:116-151`) which:

1. GETs `/users/{username}` to retrieve user details.
2. Filters accounts to those with `accountStatus: "FULL_TRADING"`.
3. If multiple valid accounts exist and no `subaccount` option is set, throws an error.
4. If `subaccount` is specified, finds the matching account by checking if `account` string includes the subaccount number.
5. Account format is `default:{subaccount}`.

### WebSocket Authentication (Private, unused)

The private WebSocket class (`breakoutprop.ws-private.ts:41-51`) has an `auth()` method that signs `GET/realtime{expires}` with HMAC-SHA256 using `create-hmac`, and sends `{ op: "auth", args: [key, expires, signature] }`. However, the private WebSocket is **never connected** in the current implementation.

## Startup Sequence

The `start()` method (`breakoutprop.exchange.ts:653-719`) proceeds as follows:

1. **Sequential**: `getAccountCode()` -- authenticates and resolves the account code.
2. **Sequential**: `fetchMarkets()` -- fetches combined instruments and instrument details. Updates store with markets.
3. **Parallel** (via `Promise.all`):
   - `fetchTickers()` -- fetches Quote market data for all market symbols. Updates store with tickers.
   - `fetchPortfolioRaw()` -- fetches portfolio. Formats and updates store with balance, positions, and orders.
4. **Sequential**: `tick()` -- starts the polling loop.

The commented-out section (`breakoutprop.exchange.ts:683-718`) shows an alternative path that would connect WebSockets and fetch balance/orders separately, but this is currently disabled.

## REST API Endpoints

All endpoints use URL parameter replacement via `replaceUrlParams()` for `{accountCode}`, `{username}`, and `{orderCode}` placeholders.

| Constant             | Path                                        | Method | Description                                          |
| -------------------- | ------------------------------------------- | ------ | ---------------------------------------------------- |
| `LOGIN`              | `/login`                                    | POST   | Authenticate and obtain session token                |
| `USER`               | `/users/{username}`                         | GET    | Fetch user details and account list                  |
| `INSTRUMENTS`        | `instruments/query`                         | GET    | Fetch available trading instruments (base info)      |
| `INSTRUMENT_DETAILS` | `accounts/{accountCode}/instruments/query`  | GET    | Fetch per-account instrument details (margin, sizes) |
| `MARKET_DATA`        | `marketdata`                                | POST   | Fetch quotes (tickers) and candle data               |
| `PORTFOLIO`          | `accounts/{accountCode}/portfolio`          | GET    | Fetch balance, positions, and orders (combined)      |
| `POSITIONS`          | `accounts/{accountCode}/positions`          | GET    | Fetch positions (defined but not used directly)      |
| `ORDERS`             | `accounts/{accountCode}/orders`             | GET    | Fetch orders (defined but partially implemented)     |
| `PLACE_ORDER`        | `accounts/{accountCode}/orders`             | POST   | Place a new order                                    |
| `CANCEL_ORDER`       | `accounts/{accountCode}/orders/{orderCode}` | DELETE | Cancel an order by code                              |

### Market Data Endpoint

The `/marketdata` endpoint is used for both tickers and candles via POST with different `eventTypes`:

- **Quotes**: `{ eventTypes: [{ type: "Quote" }], symbols: [...], account: accountCode }`
- **Candles**: `{ eventTypes: [{ type: "Candle", candleType: "...", fromTime: "...", toTime: "..." }], symbols: [...], account: accountCode }`

## WebSocket Implementation

### Public WebSocket (`breakoutprop.ws-public.ts`)

The public WebSocket `connectAndSubscribe` is an **empty function** (`breakoutprop.ws-public.ts:92`). No real WebSocket connection is established. Instead, data flows through a polling pattern:

- **OHLCV listener** (`listenOHLCV`): Registers handlers on `poll_kline.{symbol}` topic. Creates synthetic candles from ticker `last` price where open=high=low=close=last, volume=0.
- **OrderBook listener** (`listenOrderBook`): Registers handlers on `poll_book.{symbol}` topic. Creates a synthetic 2-level order book from bid/ask with hardcoded amount of 1.
- **Polling delivery**: The `tick()` method in the exchange class fetches tickers via REST and dispatches them to registered `poll_book.*` and `poll_kline.*` message handlers.

The subscribe method (would be used if WebSocket were connected) uses format: `{ event: "sub", params: { channel: topic, cb_id: nanoid() } }`.

Ping format: `{ op: "ping" }`.

### Private WebSocket (`breakoutprop.ws-private.ts`)

The private WebSocket class (`BitruePrivateWebsocket` -- note: class name is a misnomer, likely copied from Bitrue) is instantiated but **never connected** in `start()`. It:

- Connects to `ws://localhost:5050/breakoutws?requestId=<nanoid>` (local proxy).
- Authenticates via HMAC-SHA256 on open.
- Subscribes to `["order", "position"]` channels.
- Parses received messages via `jsonParse` but does nothing with the parsed data (`breakoutprop.ws-private.ts:30-31`).
- Sends `{ op: "ping" }` for keepalive.

## Order Types and Mappings

Only **Limit** orders are currently mapped:

| API Value | Internal Type     |
| --------- | ----------------- |
| `LIMIT`   | `OrderType.Limit` |

### Side Mapping

| API Value | Internal Value   |
| --------- | ---------------- |
| `BUY`     | `OrderSide.Buy`  |
| `SELL`    | `OrderSide.Sell` |

### Status Mapping

| API Value | Internal Value     |
| --------- | ------------------ |
| `WORKING` | `OrderStatus.Open` |

### Position Side Mapping

| API Value | Internal Value       |
| --------- | -------------------- |
| `BUY`     | `PositionSide.Long`  |
| `SELL`    | `PositionSide.Short` |

## Order Lifecycle

### Order ID Generation

Order codes are generated as `tealstreet-{uuid}` using the `uuid()` utility (`breakoutprop.exchange.ts:623`).

### Order Placement (`breakoutprop.exchange.ts:578-651`)

1. Look up the market from the store.
2. Ensure session token is valid.
3. Get account code.
4. Adjust amount and price to market precision using `adjust()`.
5. Build order request with `omitUndefined()`:
   - `orderCode`, `type`, `instrument`, `quantity`, `side`, `limitPrice`, `positionEffect: "OPEN"`, `tif: "GTC"`.
6. POST to `PLACE_ORDER` endpoint.
7. Return the `orderId` from the response.

### Order Update (`breakoutprop.exchange.ts:560-576`)

Implemented as **cancel-and-replace**: cancels the existing order, then places a new one with updated price/amount.

### Order Cancellation

- **Single order** (`breakoutprop.exchange.ts:520-535`): DELETE to `accounts/{accountCode}/orders/{orderCode}`.
- **Bulk cancel** (`breakoutprop.exchange.ts:537-558`): Chunks orders into batches of 10, processes each batch in parallel with `Promise.all`. Note: there is a bug -- the `cancelOrder` call inside the map is missing `await` (`breakoutprop.exchange.ts:545`).

### Order Fetching

Two paths exist:

1. **`fetchOrders()`** (`breakoutprop.exchange.ts:424-470`): GETs the ORDERS endpoint, but currently just `console.log`s the data and returns `[]`. The working implementation is commented out.
2. **`formatOrders(portfolioRaw)`** (`breakoutprop.exchange.ts:472-518`): Extracts orders from the portfolio response. This is the active path, used in `tick()` and `start()`.

Orders are extracted from the first leg: side, price, quantity, filled/remaining all come from `order.legs[0]`.

## Symbol Format

- **Format**: Symbol with `USD` suffix, e.g., `BTCUSD`, `ETHUSD`.
- **Base derivation**: Base currency is derived by stripping `USD` from the symbol (`breakoutprop.exchange.ts:234`).
- **Quote currency**: Hardcoded as `"USD"`.

## Data Precision & Conversion

### Price Precision

Derived from the instrument's `priceIncrement` field: `precision.price = -Math.log10(priceIncrement)` (number of decimal places). For example, `priceIncrement: 0.01` yields `precision.price = 2`. (`breakoutprop.exchange.ts:240`)

### Amount Precision

Set directly from `instrumentDetails.minOrderSize` (`breakoutprop.exchange.ts:239`). Note: this stores the minimum order size as the precision, not the number of decimal places.

### Amount/Price Adjustment

Orders use `adjust(value, precision)` from `safe-math` to round values before sending to the API (`breakoutprop.exchange.ts:606-607`).

### Leverage Derivation

Leverage is always calculated as `1 / marginRate` from position data (`breakoutprop.exchange.ts:367`). Max leverage per market is `1 / marginRate.tiers[0].value` (`breakoutprop.exchange.ts:250`).

## Rate Limiting

- **Rate-limited client** (`this.xhr`): 5 RPS via `axios-rate-limit` (`breakoutprop.exchange.ts:76`).
- **Unlimited client** (`this.unlimitedXHR`): Created but not currently used (`breakoutprop.exchange.ts:77`).
- **Retry logic**: 3 retries on network errors or `'error sign!'` responses via `axios-retry` (`breakoutprop.api.ts:40-43`).
- **Bulk cancel batching**: Orders are cancelled in batches of 10 (`breakoutprop.exchange.ts:538`).

## State Reconciliation

### Polling Architecture

Since WebSockets are not active, the exchange operates on a **polling loop** via `tick()` (`breakoutprop.exchange.ts:786-823`):

1. Fetch portfolio (balance, positions, orders) via the single `PORTFOLIO` endpoint.
2. Fetch tickers for all market symbols.
3. Dispatch ticker data to registered `poll_book.*` and `poll_kline.*` message handlers.
4. Wait for `tickInterval` (configurable via `options.extra.tickInterval`), then repeat via `loop()`.

### Portfolio as Primary Data Source

Balance, positions, and orders all come from the single `PORTFOLIO` endpoint (`breakoutprop.exchange.ts:318-332`). There are dedicated `POSITIONS` and `ORDERS` endpoints defined in the types but the portfolio endpoint is used for combined fetching.

### Fake Positions

For symbols without an active position, `formatPositions()` creates empty placeholder positions (`breakoutprop.exchange.ts:390-404`) with `contracts: 0`, `entryPrice: 0`, `leverage: 0`, and `side: PositionSide.Long`.

## Exchange Metadata

### Candle Intervals

| Timeframe | API candleType |
| --------- | -------------- |
| `1m`      | `m`            |
| `5m`      | `5m`           |
| `15m`     | `15m`          |
| `30m`     | `30m`          |
| `1h`      | `h`            |
| `2h`      | `2h`           |
| `4h`      | `4h`           |
| `1d`      | `d`            |
| `1w`      | `w`            |

### Grafana Monitoring

- **Every-request tracking**: `PLACE_ORDER`, `CANCEL_ORDER`
- **Periodic tracking**: `PORTFOLIO`, `POSITIONS`, `ORDERS`

## Testnet/Demo

There is no testnet/demo URL differentiation in the current implementation. The `BASE_URL` is hardcoded to `https://breakout.prosp.devexperts.com/dxsca-web`. Commented-out alternatives include:

- `http://localhost:5050/breakoutprop` (local proxy)
- `https://app.breakoutprop.com/dxsca-web` (app URL)

The WebSocket URLs point to a local proxy at `ws://localhost:5050/breakoutws` for both public and private.

## Known Quirks / Implementation Notes

- **XBTUSD filter**: The symbol `XBTUSD` is explicitly filtered out from both instruments and instrument details because it causes HTTP 500 errors in ticker queries (`breakoutprop.exchange.ts:164`, `breakoutprop.exchange.ts:183`).
- **No real order book**: The order book is a synthetic 2-level book constructed from bid/ask of the ticker, with hardcoded amount of 1 (`breakoutprop.ws-public.ts:53-78`).
- **No real candles from WebSocket**: OHLCV data via `listenOHLCV` produces synthetic candles where open/high/low/close are all `ticker.last` with volume 0 (`breakoutprop.ws-public.ts:29-36`).
- **No funding rate/open interest**: Ticker fields `fundingRate`, `fundingTime`, `openInterest`, and `percentage` are always 0 (`breakoutprop.exchange.ts:299-306`).
- **Ticker last/mark/index all use bid**: The `last`, `mark`, and `index` fields on tickers are all set to `rawTicker.bid` (`breakoutprop.exchange.ts:299-301`).
- **fetchOrders is a stub**: The `fetchOrders()` method exists but just logs and returns `[]`; working order fetching goes through `formatOrders(portfolioRaw)` instead (`breakoutprop.exchange.ts:424-430`).
- **Position mode**: Always `OneWay` with `Cross` margin type (hardcoded).
- **Maker/taker fees**: Always 0 (prop trading platform, `breakoutprop.exchange.ts:259-260`).
- **No leverage/margin/position mode changes**: These are not implemented (prop trading platform has fixed leverage via margin rates).
- **Class name mismatch**: The private WebSocket class is named `BitruePrivateWebsocket`, likely copied from the Bitrue exchange (`breakoutprop.ws-private.ts:8`).
- **lastUpdate on orders uses price**: In `formatOrders()`, the `lastUpdate` field is set to `order.legs[0].price` instead of a timestamp (`breakoutprop.exchange.ts:511`). This appears to be a bug.
- **Missing await in cancelOrders**: In `cancelOrders()`, the `this.cancelOrder(order.id)` call inside the map is missing `await`, so errors may not propagate correctly (`breakoutprop.exchange.ts:545`).
- **Tealstreet tunnel support**: When `options.tealstreetTunnel` is enabled, the axios adapter is swapped to `axiosHttpOverWebsocketAdapter` for HTTP-over-WebSocket transport (`breakoutprop.api.ts:45-47`).
- **No CCXT reference**: This is a prop trading platform using DXtrade backend, not a standard crypto exchange.

## Error Handling

### Retry Strategy

The API client retries up to 3 times on:

- Network errors (detected by `axios-retry`'s `isNetworkError`)
- Signature errors (response body contains `'error sign!'` in `ret_msg` or `retMsg`)

See `breakoutprop.api.ts:11-16` and `breakoutprop.api.ts:40-43`.

### Session Token Guard

An axios request interceptor throws `new Error('No session token')` for any request (except `/login`) if `sessionToken` is null (`breakoutprop.api.ts:50-52`).

### Error Propagation Pattern

Most fetch methods follow a try/catch pattern:

1. Try the API call.
2. On error, call `this.emitAndLogError(err?.response?.data?.msg || err?.message)`.
3. Return the current store data as fallback (e.g., `this.store.markets`, `this.store.balance`).

Methods that mutate state (placeOrder, cancelOrder) re-throw after logging.

## Testing / Validation

No test files exist for this exchange. Validation is performed at runtime via `validateAccount()` which attempts `getSessionToken()` and returns an error string on failure.
