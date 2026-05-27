# BTCC Exchange Integration

## Exchange Information

- **Type**: Centralized Exchange (CEX), Futures/CFD contracts
- **Architecture**: REST-only (no WebSocket implementation)
- **Position mode**: One-Way only (`PositionMode.OneWay`), not changeable via API
- **Margin type**: Cross only (`MarginType.Cross`), not changeable via API
- **Leverage**: Determined by contract/symbol type, not adjustable via API
- **Authentication**: Session-based with token login + MD5 request signing

## API Documentation

- **Base URL**: `https://api1.btloginc.com:9081`
- No publicly available API documentation link found. Implementation based on proprietary API.

## File Structure

| File               | Description                                                                       |
| ------------------ | --------------------------------------------------------------------------------- |
| `btcc.api.ts`      | Minimal REST API client (no auth interceptor -- signing is in the exchange class) |
| `btcc.exchange.ts` | Main exchange class extending `BaseExchange` (~934 lines)                         |
| `btcc.types.ts`    | Type definitions, endpoint constants, enums, and response types                   |

**Note**: BTCC is a **REST-only** implementation. There are no WebSocket files. All data is fetched via polling.

## Authentication

### Four Required Credentials

BTCC requires four credentials, mapped to Tealstreet fields:

| Tealstreet Field | BTCC Meaning | Used As                      | Validated In          |
| ---------------- | ------------ | ---------------------------- | --------------------- |
| API Key          | `api_key`    | API key identifier           | `ensureCredentials()` |
| API Secret       | `secret_key` | Signing secret (used in MD5) | `ensureCredentials()` |
| Application ID   | `user_name`  | Account username             | `ensureCredentials()` |
| Passphrase       | `password`   | Account password             | `ensureCredentials()` |

Validation in `ensureCredentials()` (`btcc.exchange.ts:105-118`) throws descriptive errors for each missing field.

### Login Flow

`ensureLoggedIn()` (`btcc.exchange.ts:198-220`):

1. If `force` is false and `this.token` exists, returns immediately (cached session).
2. Calls `ensureCredentials()` to validate all four fields are present.
3. **POST** to `/v1/user/login` with params: `{ user_name, password, company_id: 1, api_key }`.
4. Params are signed via `buildSignedParams()` (see below), but with `auth: false` (no token injected since we don't have one yet).
5. Response: `{ token, account: { id, account_no, balance, equity, ... } }`.
6. Stores `token`, `account`, and `accountNumber`.
7. Starts heartbeat timer.

### Request Signing (MD5)

`buildSignedParams()` (`btcc.exchange.ts:140-167`):

1. Collect all non-empty parameters as strings:
   - Booleans become `"1"` or `"0"`
   - Numbers converted to string via `.toString()`
   - Other values cast to `String()`
2. Append `secret_key=<apiSecret>` to the parameters.
3. Sort all keys alphabetically using `localeCompare`.
4. Build query string: `key1=encodeURIComponent(value1)&key2=encodeURIComponent(value2)&...`
5. Compute `sign = MD5(queryString)` (using Node.js `crypto.createHash('md5')`).
6. Return original params with `sign` appended.

**Important**: The secret key is included directly in the sorted parameter string before hashing (not as a separate HMAC key). This MD5 approach is unusual for crypto exchanges.

### Authenticated Request Flow

`apiRequest()` (`btcc.exchange.ts:169-196`):

1. If `auth !== false`:
   - Calls `ensureLoggedIn()` to guarantee valid session
   - Injects `token` parameter
   - Injects `accountid` parameter (from `this.account.id`) if not already present
2. Signs all parameters via `buildSignedParams()`
3. **All requests send parameters via URL query string** -- even POST requests use `params`, not request body
4. Checks response `data.code !== 0` for errors

### Session Keepalive

`startHeartbeat()` (`btcc.exchange.ts:222-236`):

- `setInterval` runs every **20 seconds**
- Calls `GET /v1/user/keepalive`
- On failure: clears `this.token` (forcing re-login on next request) and emits error
- Timer is cleared on `dispose()`

### Account Number vs Account ID

Two different identifiers used in different contexts:

- `account.id` (numeric): Used as `accountid` parameter in most API requests
- `account.account_no` (string): Stored as `accountNumber`, used for report queries as `accountNo`

`ensureAccountNumber()` (`btcc.exchange.ts:120-129`) resolves from `this.accountNumber` or `this.account?.account_no`.

## Startup Sequence

Defined in `btcc.exchange.ts:378-398` (`start()`):

1. **`ensureLoggedIn()`** -- Authenticates with BTCC, gets token and account data. Starts heartbeat.
2. **`fetchMarkets()`** -- REST call to `/v1/config/symbollist`. Builds symbol mapping tables. Store updated with markets.
3. **`fetchTickers()`** -- Returns placeholder tickers (all zeros) derived from market list. Store updated with tickers.
4. **`tick()`** -- First tick: fetches positions, balance, orders. Continues polling.

There is no WebSocket initialization step.

### Tick Loop

`tick()` (`btcc.exchange.ts:400-428`):

- **Every tick**: Fetch positions via `POSITION_LIST`, fetch orders via `ORDER_LIST`
- **Every other tick** (`count % 2 === 0`): Fetch balance via `ACCOUNT_INFO`
- Wait for `tickInterval` (configurable via `options.extra.tickInterval`), then repeat
- On error, logs and continues (does not crash the loop)

## REST API Endpoints

| Constant          | Path                         | Method | Description                                   |
| ----------------- | ---------------------------- | ------ | --------------------------------------------- |
| `LOGIN`           | `/v1/user/login`             | POST   | Authenticate and get token + account          |
| `KEEPALIVE`       | `/v1/user/keepalive`         | GET    | Keep session alive (called every 20s)         |
| `SYMBOL_LIST`     | `/v1/config/symbollist`      | GET    | Fetch all trading symbols                     |
| `ACCOUNT_GROUP`   | `/v1/config/accountgroup`    | GET    | Fetch account groups                          |
| `ACCOUNT_INFO`    | `/v1/account/account`        | GET    | Fetch account balance/equity/margin           |
| `POSITION_LIST`   | `/v1/account/positionlist`   | GET    | Fetch open positions (paginated)              |
| `OPEN_POSITION`   | `/v1/account/openposition`   | POST   | Open a new market position                    |
| `CLOSE_POSITION`  | `/v1/account/closeposition`  | POST   | Close an existing market position             |
| `UPDATE_POSITION` | `/v1/account/updateposition` | POST   | Update an existing position                   |
| `OPEN_PENDING`    | `/v1/account/openpending`    | POST   | Place a pending (limit/stop) order            |
| `UPDATE_PENDING`  | `/v1/account/updatepending`  | POST   | Modify a pending order                        |
| `CANCEL_PENDING`  | `/v1/account/cancelpending`  | POST   | Cancel a pending order                        |
| `ORDER_LIST`      | `/v1/account/orderList`      | GET    | Fetch all pending orders                      |
| `TRADE_REPORT`    | `/v1/user/getTradeReport`    | GET    | Fetch execution/trade history (paginated)     |
| `PROFIT_REPORT`   | `/v1/user/getProfitReport`   | GET    | Fetch closed PnL / profit history (paginated) |
| `REPORT_DEAL`     | `/v1/user/getReportDeal`     | GET    | Fetch deal reports                            |

## WebSocket Implementation

**None.** BTCC is entirely REST-based. All `listen*` methods are no-ops:

- `listenOrderBook()` -- Returns empty cleanup function (`btcc.exchange.ts:917-919`)
- `listenOHLCV()` -- Returns empty cleanup function (`btcc.exchange.ts:921-923`)
- `listenOHLCVs()` -- Empty function (`btcc.exchange.ts:925`)
- `listenOHLCVWebworker()` -- Returns empty cleanup function (`btcc.exchange.ts:927-929`)
- `listenOrderBookWebworker()` -- Returns empty cleanup function (`btcc.exchange.ts:931-933`)

## Order Types and Mappings

### BTCC Order Types (`BTCCOrderType` enum)

| Enum Value         | Numeric | Description                               | reduceOnly? |
| ------------------ | ------- | ----------------------------------------- | ----------- |
| `LimitOpen`        | 1       | Limit order to open a new position        | No          |
| `StopOpen`         | 2       | Stop order to open a new position         | No          |
| `LimitClose`       | 3       | Limit order to close an existing position | Yes         |
| `MarketStopLoss`   | 4       | Market stop-loss to close a position      | Yes         |
| `MarketTakeProfit` | 5       | Market take-profit to close a position    | Yes         |

### Internal Type to BTCC Type Mapping

Determined by `derivePendingType()` (`btcc.exchange.ts:596-607`):

| `PlaceOrderOpts.type`  | `reduceOnly`/`close`? | BTCC Order Type                                     |
| ---------------------- | --------------------- | --------------------------------------------------- |
| `OrderType.Limit`      | No                    | `LimitOpen` (1)                                     |
| `OrderType.Limit`      | Yes                   | `LimitClose` (3)                                    |
| `OrderType.StopLoss`   | No                    | `StopOpen` (2)                                      |
| `OrderType.StopLoss`   | Yes                   | `MarketStopLoss` (4)                                |
| `OrderType.TakeProfit` | No                    | `StopOpen` (2)                                      |
| `OrderType.TakeProfit` | Yes                   | `MarketTakeProfit` (5)                              |
| `OrderType.Market`     | No                    | Uses `OPEN_POSITION` endpoint (not `OPEN_PENDING`)  |
| `OrderType.Market`     | Yes                   | Uses `CLOSE_POSITION` endpoint (not `OPEN_PENDING`) |

### BTCC Order Status (`BTCCOrderStatus` enum)

| Enum Value        | Numeric | Internal Status        |
| ----------------- | ------- | ---------------------- |
| `Initial`         | 0       | `OrderStatus.Open`     |
| `Pending`         | 1       | `OrderStatus.Open`     |
| `Filled`          | 2       | `OrderStatus.Closed`   |
| `PartiallyFilled` | 4       | `OrderStatus.Open`     |
| `Canceled`        | 5       | `OrderStatus.Canceled` |
| `Rejected`        | 6       | `OrderStatus.Canceled` |

`fetchOrders()` only returns orders with status `Initial`, `Pending`, or `PartiallyFilled`.

### BTCC Position Status (`BTCCPositionStatus` enum)

| Enum Value | Numeric | Description     |
| ---------- | ------- | --------------- |
| `Initial`  | 0       | Not yet active  |
| `Open`     | 1       | Active position |
| `Closed`   | 2       | Closed position |

### Direction Mapping

| Numeric Value | Position Side        | Order Side       |
| ------------- | -------------------- | ---------------- |
| 1             | `PositionSide.Long`  | `OrderSide.Buy`  |
| 2             | `PositionSide.Short` | `OrderSide.Sell` |

## Order Lifecycle

### Market Order Placement

`placeOrder()` (`btcc.exchange.ts:609-676`):

Market orders bypass the pending order system entirely:

- **Open (non-reduceOnly)**: `POST` to `OPEN_POSITION` with `{ symbol, direction, request_volume, request_price, multiple }`. Returns position ID.
- **Close (reduceOnly/close)**: `POST` to `CLOSE_POSITION` with `{ symbol, direction, positionid, request_volume, request_price, multiple }`. Requires finding the matching position via `findPositionForReduceOnly()`.

### Pending Order Placement

For Limit/StopLoss/TakeProfit orders:

1. Derives BTCC order type via `derivePendingType()`.
2. Builds payload: `{ symbol, direction, type, request_volume, request_price, stop_loss, take_profit, multiple }`.
3. For reduce-only or close-type orders, looks up position ID and attaches `positionid`.
4. `POST` to `OPEN_PENDING`. Returns order ID.

### Order Update (Native Amend)

`updateOrder()` (`btcc.exchange.ts:687-710`):

BTCC supports native order amendment via `UPDATE_PENDING`:

- Sends `{ id, direction, type, symbol, request_volume, request_price, stop_loss, take_profit }`.
- The `type` field is mapped: `1` for stop orders (`StopOpen`, `MarketStopLoss`, `MarketTakeProfit`), `0` for limit orders.
- Preserves existing `stop_loss` and `take_profit` from `order.info`.

### Order Cancellation

`cancelOrders()` (`btcc.exchange.ts:712-716`):

- Cancels orders one at a time via `POST` to `CANCEL_PENDING` with `{ id: <orderId> }`.
- No batch cancel support.

`cancelAllOrders()` (`btcc.exchange.ts:718-722`):

- Fetches all current orders, then cancels each individually.

### Batch Order Placement

`placeOrders()` (`btcc.exchange.ts:678-685`):

- Sequential execution (no parallelism). Each order is placed via `placeOrder()` one at a time.

### Position Lookup for Reduce-Only

`findPositionForReduceOnly()` (`btcc.exchange.ts:586-594`):

- Finds position on the opposite side (Buy order -> Short position, Sell order -> Long position).
- Falls back to any position for the symbol if no opposite-side match found.

## Symbol Format

### Raw vs Display Symbols

BTCC uses internal symbol names that differ from display format:

| Raw (API)            | Display (Internal) |
| -------------------- | ------------------ |
| `crypto/BTCUSDT100x` | `BTCUSDT`          |

### Symbol Mapping Infrastructure

Three bidirectional maps (`btcc.exchange.ts:85-87`):

- **`rawSymbolToDisplay`**: `Map<string, string>` -- API name to clean display name
- **`displaySymbolToRaw`**: `Map<string, string>` -- Reverse mapping for API calls
- **`symbolMetaByDisplay`**: `Map<string, BTCCSymbolRaw>` -- Full metadata cached by display symbol

### `buildDisplaySymbol()` Logic

`btcc.exchange.ts:280-292`:

1. If `base_currency` and `profit_currency` exist on the symbol metadata, concatenate them (e.g., `BTC` + `USDT` = `BTCUSDT`).
2. Otherwise, take the last segment of the name after `/`, and try regex match `^([A-Z]+)([A-Z]+)(\d+)?x?$` to extract base and quote.
3. Final fallback: `clampSymbol()` which strips spaces/colons and uppercases.

### `clampSymbol()` Helper

`btcc.exchange.ts:75`: Removes colons and whitespace, uppercases result.

### Leverage Extraction from Symbol Name

`getDefaultLeverage()` (`btcc.exchange.ts:294-308`):

1. Extract numeric suffix from raw name (e.g., `100x` in `crypto/BTCUSDT100x` -> `100`).
2. Fall back to `leverage_max` from symbol metadata.
3. Final fallback: `20`.

## Data Precision & Conversion

### Price Precision

- `pricePrecision` comes from `BTCCSymbolRaw.digits` (defaults to `2` if not set).

### Amount Precision

`getPrecisionFromStep()` (`btcc.exchange.ts:60-73`):

- Derived from `volumes_step` (e.g., `0.01` -> precision `2`).
- Handles scientific notation (e.g., `1e-8` -> precision `8`).
- Handles integer step (e.g., `1` -> precision `0`).

### Balance Calculation

`fetchBalance()` (`btcc.exchange.ts:430-456`):

```
total = equity ?? balance
free = free_margin
used = margin ?? (total - free)
upnl = total - balance
```

### Position Mapping

`mapPosition()` (`btcc.exchange.ts:458-482`):

- `contracts` = `volume` or `exec_volume`
- `entryPrice` = `open_price`
- `leverage` = `multiple` or default leverage from symbol
- `unrealizedPnl` = `profit` or `exec_profit`
- **`liquidationPrice` = `close_price`** (notably, maps BTCC's `close_price` to liquidation price)
- `notional` = `entryPrice * contracts`

### `toNumber()` Helper

`btcc.exchange.ts:132-138`: Safe number conversion with fallback. Returns `fallback` for null, undefined, empty string, NaN, Infinity.

## Rate Limiting

- **Primary `xhr`**: 5 requests/second (`maxRPS: 5` via `axios-rate-limit`)
- **`unlimitedXHR`**: Separate instance with no rate limit (exists but appears unused in current code)
- **No explicit exchange-documented rate limits** -- the 5 RPS is a conservative client-side guard
- No retry logic configured on the axios instance

## State Reconciliation

### Polling-Based Only

Since there are no WebSockets, all state is reconciled through REST polling:

- **Positions**: Fetched every tick via `POSITION_LIST`. Filtered to `status === BTCCPositionStatus.Open` and `volume > 0`. Uses `store.addOrUpdatePositionsFull()` for full replacement.
- **Orders**: Fetched every tick via `ORDER_LIST`. Filtered to open statuses (Initial, Pending, PartiallyFilled). Uses `store.addOrUpdateOrdersFull()` for full replacement.
- **Balance**: Fetched every other tick via `ACCOUNT_INFO`.

### No Deduplication Needed

Because the store is fully replaced each tick (`addOrUpdatePositionsFull`, `addOrUpdateOrdersFull`), there is no risk of stale data accumulation. The trade-off is higher latency for state updates compared to WebSocket-based exchanges.

## Exchange Metadata

- `exchangeName`: `'btcc'`
- `token`: Session token from login (nullable)
- `account`: Full account object from login response
- `accountNumber`: String version of `account.account_no`
- `heartbeatTimer`: `setInterval` reference for keepalive (cleared on dispose)

### Unsupported Features

These methods throw errors immediately:

- `setLeverage()`: "BTCC API does not expose leverage adjustment endpoint"
- `changePositionMode()`: "BTCC API does not support position mode changes via API"
- `changeMarginType()`: "BTCC API does not expose margin type changes"

### Stub Features

These return empty/placeholder data:

- `fetchTickers()`: Returns tickers with all-zero values
- `fetchOHLCV()`: Returns empty array
- `fetchPublicTrades()`: Returns empty array
- `fetchSymbolMaxLeverage()`: Returns `getDefaultLeverage(symbol)` (calculated, not fetched)

## Testnet/Demo

No testnet or demo mode is implemented. The base URL is hardcoded to production (`https://api1.btloginc.com:9081`).

## Known Quirks / Implementation Notes

1. **No WebSocket implementation**: All data is polled via REST. Real-time data latency depends entirely on tick interval.

2. **No ticker endpoint**: `fetchTickers()` returns placeholder tickers with all-zero values. Real price data only comes through position/order data.

3. **No OHLCV or public trades**: `fetchOHLCV()` and `fetchPublicTrades()` return empty arrays. Charting will not work for BTCC.

4. **Market orders bypass pending system**: Market orders go to `OPEN_POSITION`/`CLOSE_POSITION` (immediate execution) instead of `OPEN_PENDING`.

5. **MD5 signing**: Unusual for crypto exchanges -- uses MD5 hash instead of HMAC-SHA256. The secret key is included directly in the sorted parameter string before hashing.

6. **All params via query string**: Even POST requests send parameters as URL query params (via axios `params`), not in the request body. The `Content-Type: application/json` header is set but the body is empty.

7. **Leverage is fixed per symbol**: Cannot be changed via API. Determined by contract type (extracted from symbol name suffix like `100x`).

8. **No position mode or margin type changes**: Both `changePositionMode()` and `changeMarginType()` throw errors. Hardcoded to `OneWay` + `Cross`.

9. **Liquidation price from `close_price`**: The raw position's `close_price` field is mapped to `liquidationPrice` (`btcc.exchange.ts:478`).

10. **Balance polling throttled**: Balance is only fetched every other tick (`count % 2 === 0`) to reduce API calls.

11. **Order update type field quirk**: When updating a pending order via `UPDATE_PENDING`, the `type` parameter is `1` for stop orders (`StopOpen`, `MarketStopLoss`, `MarketTakeProfit`) and `0` for limit orders (`btcc.exchange.ts:691-695`). This differs from the `BTCCOrderType` enum values.

12. **Account number vs account ID**: `account.id` (numeric) is used for API requests as `accountid`; `account.account_no` (string) is stored separately for report queries as `accountNo`.

13. **Symbol name parsing**: Complex regex-based extraction from raw names like `crypto/BTCUSDT100x`. Multiple fallback strategies in `buildDisplaySymbol()`.

14. **No CCXT reference implementation**: BTCC is not available in CCXT.

15. **Login re-authentication**: If keepalive fails, `this.token` is set to `null`. The next `apiRequest()` call triggers `ensureLoggedIn()` which re-authenticates transparently.

16. **Paginated reports**: Trade reports (`getTradeReport`) and profit reports (`getProfitReport`) support pagination with `pageNo`, `pageSize` (max 200), and time range filters (`beginTime`/`endTime`).

17. **Contract size from metadata**: `BTCCSymbolRaw.contract_size` is included in market data as `contractSize` (defaults to `1`).

18. **Sequential order cancellation**: `cancelOrders()` cancels orders one at a time in a loop. No batch cancel API available.

## Error Handling

### API Error Detection

`apiRequest()` (`btcc.exchange.ts:191-193`):

- All responses checked for `data.code !== 0`
- Error message extracted from `data.msg` or defaults to `'BTCC API error'`
- Throws `Error` with the message

### Login Errors

- `ensureCredentials()` throws descriptive errors per missing field
- Login failure throws with `data.msg` or `'BTCC login failed'`
- `validateAccount()` catches login errors and returns error message string (does not throw)

### Keepalive Errors

`startHeartbeat()` (`btcc.exchange.ts:230-234`):

- On keepalive failure: emits error via `emitAndLogError`, clears token
- Does not throw -- silently degrades to re-login on next request

### Tick Loop Errors

`tick()` (`btcc.exchange.ts:422-425`):

- Catches all errors within the tick
- Emits via `emitAndLogError` with message `'BTCC polling tick failed'`
- Continues to next tick (loop does not break)

### Position Not Found

`placeOrder()` for reduce-only market orders (`btcc.exchange.ts:619-621`):

- If no matching position found, throws `'No BTCC position available to close'`

## Testing / Validation

No test file exists for the BTCC exchange. The implementation should be validated manually through:

1. Login and session management
2. Symbol mapping correctness
3. Order placement (market and pending)
4. Position fetching and mapping
5. Report/execution history retrieval
