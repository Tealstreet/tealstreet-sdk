# CLAUDE.md - WEEX Exchange Integration

WEEX uses Contract V3 for this adapter. The Contract V3 changelog states
that Futures Contract V3 launched on 2026-03-09 and Contract WebSocket V3
launched on 2026-03-18; V2 is no longer maintained.

## Exchange Information

- Exchange name: `weex` (Contract V3 perpetual futures).
- Availability: development-only (`enabled: false`, `enabledInDev: true`,
  `enableInPreview: false`). Preview and production are both production lanes;
  do not enable this adapter there without a fresh release review.
- Testnet/demo: the adapter does not expose testnet support
  (`supportsTestnet: false`). WEEX's 2026-10-15 docs say Futures simulation now
  lives at `https://api-sim.weex.com` and keeps the same paths/signing rules.
  Do not set `options.testnet` or promote demo/UAT flows until `BASE_URL.testnet`
  is changed to the simulation host and covered by routing/signing tests.

## Local Smoke Test Keys

Use `.apikey.json` in this directory for local manual smoke tests. It is
gitignored and must not be committed.

Expected shape:

```json
{
  "key": "ACCESS-KEY",
  "secret": "secret",
  "passphrase": "passphrase"
}
```

## Domains

- REST: `https://api-contract.weex.com`
- Public WS: `wss://ws-contract.weex.com/v3/ws/public`
- Private WS: `wss://ws-contract.weex.com/v3/ws/private`

## Auth

REST private endpoints use headers:

- `ACCESS-KEY`
- `ACCESS-PASSPHRASE`
- `ACCESS-TIMESTAMP`
- `ACCESS-SIGN`
- `Content-Type: application/json`

The REST signature is:

```text
base64(hmac_sha256(secret, timestamp + METHOD + path + ("?" + queryString if present) + bodyString))
```

Use `virtualClock.getCurrentTimeMs()` for timestamps. Sign `DELETE` requests
as uppercase `DELETE`.

Private WebSocket auth is header-based, not a login message. The private WS
signature signs:

```text
timestamp + "/v3/ws/private"
```

Node/Bun and CLI paths use the header-capable reconnecting `ws` transport. It
evaluates the required private-auth header provider for every connection
attempt, so the timestamp and signature are regenerated before each reconnect.
A renderer that exposes a Node marker but cannot load `ws` is also unsupported:
`BaseWebSocket` fails closed for those required auth headers instead of silently
dropping them. Browser WebSocket APIs cannot set upgrade headers, and WEEX has
no private login-message alternative; the private socket is deliberately
disabled (no credentials are put in the URL) and the exchange continues its
2-second REST safety loop instead. Public `User-Agent` and Tealstreet routing
headers are static best-effort headers, not fail-closed authentication.

Private server keepalive frames are `{ "type": "ping", ... }`; reply
immediately with `{ "method": "PONG", "id": n }`. Public pings instead use
`event: "ping"`. WEEX closes a connection after more than 10 unanswered pings.

## API Documentation

- [Contract V3 WebSocket overview](https://www.weex.com/api-doc/contract/Websocket/websocket-intro)
- [Contract V3 introduction](https://www.weex.com/api-doc/contract/intro)
- [Contract V3 changelog](https://www.weex.com/api-doc/contract/changelog)
- [Contract V3 access restrictions](https://www.weex.com/api-doc/contract/QuickStart/AccessRestrictions)
- [Contract V3 signature](https://www.weex.com/api-doc/contract/QuickStart/Signature)
- [Contract V3 API domain](https://www.weex.com/api-doc/products/futures/QuickStart/APIDomain)
- [Get API trading symbols](https://www.weex.com/api-doc/products/futures/Market_API/GetApiTradingSymbols)
- [Place orders in batch](https://www.weex.com/api-doc/contract/Transaction_API/PlaceOrdersBatch)
- [Modify TP/SL conditional order](https://www.weex.com/api-doc/contract/Transaction_API/ModifyTpSlOrder)
- [CCXT WEEX reference](https://github.com/ccxt/ccxt/wiki/weex)

## File Structure

Follows the standard layout in `../CLAUDE.md`; `weex.utils.ts` contains the
Contract V3 signing helpers shared by the REST and private WebSocket paths.

## Implementation Notes

- WEEX launched `POST /capi/v3/batchOrders` on 2026-05-28 with at most five
  orders per request. `placeOrders` uses it only for contiguous ordinary
  `LIMIT`/`MARKET` rows, correlates the raw response array by the required
  echoed `clientOrderId`, and returns IDs in input order. Count, identity,
  success-flag, and successful-order-ID ambiguity fail closed; successful
  PostOnly rows retain their store provenance even when a sibling row is
  rejected. Conditional/algo, direct or attached TP/SL, close, reduce-only,
  and otherwise exceptional shapes retain their dedicated single-order paths.
- Normal order amend is not documented in Contract V3. Use cancel-and-replace
  for `updateOrder`, preserving TP/SL fields when present.
- Direct TP/SL trigger-price edits use `POST /capi/v3/modifyTpSlOrder` with
  the actual protective conditional-order ID. Synthetic `__tp`/`__sl` rows
  retain only the opening parent ID, which the endpoint does not document as
  an amend selector, so their edits fail closed without sending or canceling.
  The V3 modify endpoint has no quantity field, so TP/SL size edits also fail
  closed.
- Opening `OrderType.Conditional` orders use `POST /capi/v3/algoOrder` and
  require `triggerPrice`; an execution `price` selects STOP (limit), while its
  absence selects STOP_MARKET. They are read back as opening conditionals and
  cancel through `/algoOrder`, distinct from position-protective TP/SL plans.
- WEEX added `reduceOnly` to normal order, batch order, conditional order, and
  TP/SL placement docs on 2026-09-01. The current adapter sends it only on the
  regular single-order path; regular batches intentionally skip reduce-only,
  opening conditionals force opening semantics, and standalone TP/SL does not
  include `reduceOnly`. Do not broaden reduce-only support without request-shape
  regressions and a credentialed smoke test.
- Public symbols are already Tealstreet/Binance-style (`BTCUSDT`).
- WEEX sizes in V3 REST public examples are base-asset quantities; still keep
  `contractVal` from `exchangeInfo` in `market.info` for smoke-test auditing.
- WEEX 24h REST ticker `priceChangePercent` is ratio-shaped in live public data
  (`0.001476` means about `0.1476%`), even though the generated docs examples
  can look percent-shaped. Keep the multiply-by-100 normalizer unless a fresh
  live capture proves the wire changed.
- Order preflight failures, response-level rejections, and transport failures
  emit through `emitAndLogError` at their point of origin before rejecting the
  caller. Keep the POST catch narrow so response parsing is not double-emitted.
- Contract V3 returns HTTP 429 when an IP-weight or account order-count limit is
  exceeded and documents a 10-second ban for continued abuse. Automatic retry
  is therefore read-only: GET/HEAD plus the documented read-only
  `POST /capi/v3/account/income` query. A 429 gets at most one retry, preferring
  `Retry-After`, then exhausted `X-REMAINING-WEIGHT-*` /
  `X-ORDER-REMAINING-*` windows, then the published `X-USED-WEIGHT-*` /
  `X-ORDER-COUNT-*` interval, with a 10-second fallback and a 60-second cap.
  Read network failures retain up to three retries. Order placement,
  cancellation, TP/SL modification, leverage/margin changes, closing, and all
  other mutations are never retried automatically, including after ambiguous
  network failures, because their execution status may be unknown.

## Primary Endpoints

Rust WEEX reports `leverageQuery: onReadAndMetadata`. The shared SDK selected-
symbol metadata hook issues the existing signed `symbolConfig` read even when
a flat symbol already has cached rails. Explicit leverage reads stay fresh,
public-only accounts issue no private metadata request, and malformed responses
reject without replacing the last good rails. Native settings-revision fences
continue to protect newer writes; no new polling loop is added.

Rust income queries emit `history.incomplete` with the actual native command
ID before partial completion or failure. This includes range chunk failures,
opaque cursors and chunk/lifetime budgets. The host routes coverage to the
owning caller and marks successful partial arrays incomplete; non-range read
failures still reject, while range failures retain the existing continuation.

Rust fill queries also mark the owning result incomplete when an in-scope row
has an empty `id`/`tradeId`/`fillId`. The unidentified row stays skipped and valid
peers remain available. One coded Warn `row.skipped` names `id` per query, across
all windows; the owned `history.incomplete` remains visible at Error-only logging.
No execution identity is fabricated. Absent identity fields and malformed
financial values retain the existing strict failure behavior.

- Markets: `GET /capi/v3/market/exchangeInfo`
- Tickers: `GET /capi/v3/market/ticker/24hr`,
  `GET /capi/v3/market/ticker/bookTicker`
- Depth: `GET /capi/v3/market/depth`
- Klines: `GET /capi/v3/market/klines`,
  `GET /capi/v3/market/historyKlines`
- Public trades: `GET /capi/v3/market/trades`
- API-tradable symbols: `GET /capi/v3/market/apiTradingSymbols`
- Open interest: `GET /capi/v3/market/openInterest` (documented, not wired)
- Risk limits: `GET /capi/v3/market/riskLimits` (documented, not wired)
- Balance: `GET /capi/v3/account/balance`
- Account config: `GET /capi/v3/account/accountConfig`
- Symbol config: `GET /capi/v3/account/symbolConfig`
- Commission rate: `GET /capi/v3/account/commissionRate` (documented, not wired)
- Positions: `GET /capi/v3/account/position/allPosition`
- Margin type: `POST /capi/v3/account/marginType`
- Leverage: `POST /capi/v3/account/leverage`
- Open orders: `GET /capi/v3/openOrders`
- Open algo orders: `GET /capi/v3/openAlgoOrders`
- Place order: `POST /capi/v3/order`
- Place regular batch (maximum 5): `POST /capi/v3/batchOrders`
- Place conditional: `POST /capi/v3/algoOrder`
- Place TP/SL: `POST /capi/v3/placeTpSlOrder`
- Cancel orders: `DELETE /capi/v3/batchOrders`
- Cancel all normal: `DELETE /capi/v3/allOpenOrders`
- Cancel algo: `DELETE /capi/v3/algoOrder`
- Cancel all algo: `DELETE /capi/v3/algoOpenOrders`
- User trades: `GET /capi/v3/userTrades`
- Income: `POST /capi/v3/account/income`

## 2026-10 Audit Findings

WEEX is implemented, not a stub: REST startup, public WS, private WS, market
data, balances, positions, orders, order writes, batch writes, conditional
orders, TP/SL, fills, and income all have adapter code plus targeted regression
coverage. It remains development-only and must not be enabled for preview or
production without a fresh release review.

Current official docs and live public reads agree with most local Contract V3
endpoint shapes. CCXT is a useful reference for parsing/order fields, but it is
not the source of truth for WEEX batch placement or simulation: the current
official changelog says batch placement launched on 2026-05-28, while CCXT still
contains stale comments that `POST /capi/v3/batchOrders` was unsupported.

Release blockers for a collaboration or production enablement:

- **Non-crypto listings:** WEEX now lists tradfi-style contracts beside crypto
  perps. Live `apiTradingSymbols` returned symbols such as `AAPLUSDT`,
  `QQQUSDT`, `XAUUSDT`, and `NATGASUSDT`, and `exchangeInfo` documents
  `contractType: TRADIFI_PERPETUAL`. The current `fetchMarkets` filter only
  checks `quoteAsset === 'USDT'` and `marginAsset === 'USDT'`; it does not filter
  tradfi contracts or implement a WEEX `instrument-class` classifier. Before a
  launch, choose one policy explicitly: crypto-only filtering, or a classifier
  and user-facing tradfi preflight.
- **Cancel response handling:** normal batch cancel and cancel-all endpoints
  return per-row `{ success, errorCode, errorMessage }`, but the adapter marks
  local orders canceled without validating those rows. Production code should
  fail or partially update based on venue-confirmed results.
- **Order reconciliation safety:** `fetchOrders` currently uses
  `Promise.allSettled` and can commit a partial snapshot when either normal or
  algo order fetching fails. A failed leg is not an authoritative empty order
  list; preserve last-known-good state or throw.
- **Private WS runtime:** WEEX private sockets require upgrade headers. Browser
  workers cannot provide them, so web accounts rely on the 2-second REST safety
  loop for private state unless a header-capable route is added and tested.
- **Private WS position aliases:** the docs use `marginMode` on WS position
  payloads, while the normalizer currently reads REST's `marginType`. REST
  refresh should correct this, but a Node/Bun private WS path can briefly mark
  isolated positions as cross.
- **Paging/history gaps:** open orders and open algo orders read only the first
  page. `userTrades` is limited by the venue to 7-day windows and is not paged.
  Income history has the shared time-window pager, but it does not use WEEX's
  `nextKeyId` / `nextKeyTime` cursor.
- **Credentialed UAT gap:** repository notes still say WEEX has no credential in
  the local key sheet. Deterministic tests cover shapes, but account validation,
  private snapshots, writes, cancels, TP/SL, reconnect behavior, and REST routing
  need a live credentialed smoke pass before release.

## Open Features / Upgrade Candidates

- Wire Futures simulation via `https://api-sim.weex.com` and add a regression
  that `options.testnet` cannot hit production.
- Decide how to handle non-crypto contracts: filter them out, or implement a
  WEEX classifier from `contractType`, `underlyingType`, and
  `underlyingSubType` with the shared tradfi preflight copy.
- Support documented trailing conditional orders (`TRAILING_MARKET`) only after
  mapping activation/callback fields and lifecycle/cancel behavior.
- Wire open interest and risk-limit/notional-bracket endpoints if product needs
  them; metadata currently advertises no open-interest support.
- Add account-specific fee-rate support from
  `GET /capi/v3/account/commissionRate` if `supportsFeeRates` is meant to mean
  user-specific futures fees rather than static market fees.
- Replace first-page-only order reads and 7-day fills reads with paged range
  walkers before relying on WEEX for history/reconciliation modules.
- Inspect cancel/close response rows before mutating local store state.

## WebSocket Channels

Subscribe with:

```json
{ "method": "SUBSCRIBE", "params": ["BTCUSDT@ticker"], "id": 1 }
```

Public channels:

- `<symbol>@ticker`
- `<symbol>@depth15` or `<symbol>@depth200`
- `<symbol>@trade`
- `<symbol>@kline_<interval>_LAST_PRICE`

REST klines accept `1m,5m,15m,30m,1h,4h,12h,1d,1w`; the current V3
WebSocket candlestick contract additionally accepts `2h` and `6h`. Public
subscriptions reject other safe-cex intervals before sending and log negative
subscription acknowledgements from WEEX.

Ticker subscriptions follow `activeTickerSymbols:changed` and send only the
incremental subscribe/unsubscribe diff. The socket reserves ten of WEEX's 100
channel slots for chart/orderbook consumers and enforces the documented 240
subscription operations per rolling hour on each connection.

Private channels:

- `account`
- `positions`
- `orders`
- `fill`

## Client order ID

Contract: `packages/safe-cex/CLAUDE.md` → "Client order ID contract" (honor verbatim on one native order, or reject before any write).

- Sends `newClientOrderId`, rule `^[.A-Z:/a-z0-9_-]{1,36}$`, on the single and regular batch paths.

## Rust engine

`engine: 'rust'` selects `sdk-rs/venues/weex` for Contract V3 futures. Native
and Workers transports carry signed private upgrade headers; browser transports
continue the two-second REST loop. Income ranges use the engine's shared span
pager. Reads stage complete snapshots and preserve newer stream events.
The account starts after the ticker seed commits, or on its first failure, so a
funded account never waits on retrying public reads and replays stay ordered.

The Rust gate records intentional TS corrections: opening trigger orders retain
both trigger and execution prices, failed order halves preserve the last complete
snapshot, stale frames cannot reopen terminal state, duplicate execution IDs emit
once, malformed placement acknowledgements fail without invented IDs, partial
cancellation failures retain rejected orders, and margin changes preserve each
symbol's position mode. See `sdk-rs/harness/gate.mjs` and the venue tests.

Rust `setLeverage(symbol, leverage, side)` preserves the opposite isolated
leverage in the required pair of request fields, including on flat symbols.
Only the selected leg's setting and positions change after success. Omitting
side changes both isolated legs. Cross margin uses the coupled `crossLeverage`
field and ignores side. An unknown opposite isolated leverage fails before a
write rather than guessing its current setting.

Rust `changePositionMode(hedged, symbol)` sends only the selected symbol's margin
configuration, retaining its existing margin type. Omitting symbol retains the
legacy positions-first, otherwise all-markets scope. Confirmed per-symbol modes
remain in their own configuration caches for subsequent order and margin calls.

`FetchLeverage` is a fresh symbol-scoped `GET /capi/v3/account/symbolConfig`
(as the TS adapter's `fetchLeverageForSymbol`), so WEEX reports
`leverageQuery: onRead`. It answers the margin-type rails and commits the
configuration unless a settings write or newer read landed meanwhile.


## Native REST order-book query

Rust fetchOrderBook is an unsigned command-owned GET of the existing Contract V3
market/depth endpoint, raw encoded symbol with no catalog lookup, limit200 by
default and explicit caller limit forwarded unchanged. Null-coalesced bids/b and
asks/a aliases preserve side order, duplicates and zero quantities, with cumulative
base amount totals; contractVal never rescales the query. lastUpdateId retains
exact string/integer bytes without floating-point conversion, missing/null becomes
an empty string, and level IDs are absent.

The query deliberately corrects TS's malformed-price/quantity fallback and prefix
parsing: invalid/nonfinite finances, malformed non-null structures and cumulative
overflow fail atomically. It never mutates subscription generations, buffered depth,
account settings or metadata. Public native/HTTP/transport failures complete only
their owner with typed NotSent/non-auth evidence; native0/200/absent code succeed.
Existing get() shared-pause admission and rate::delay HTTP429 header/window policy
remain in force (bounded60s), including deferred query ownership. There is no new
failed-request retry, stream recovery or auth invalidation. Standalone decoder and
native capability are explicit. OnReadAndMetadata settings fences and owned
empty-fill history markers remain independent.
