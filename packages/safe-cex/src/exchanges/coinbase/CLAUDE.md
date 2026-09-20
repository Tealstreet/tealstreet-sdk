# Coinbase Exchange Integration

This directory contains the Coinbase Advanced Trade exchange integration for Tealstreet, supporting both futures (INTX perpetuals + CFM dated futures) and spot trading through a portfolio-based architecture.

## Exchange Information

- **Exchange**: Coinbase Advanced Trade (INTX perpetuals, CFM dated futures, spot)
- **Position Mode**: One-way only
- **Margin Type**: Cross only
- **Reduce-only sizing**: Client-side auto-downsize is enabled because the
  create-order API has no server-side reduce-only cap
- **Leverage**: Hardcoded to 10x (slider disabled in UI)
- **Default Quote**: `USD`
- **Default Symbol**: `BTC-USD`
- **Tick Interval**: 1000ms (REST polling frequency)

## API Documentation

- **Advanced Trade REST API**: https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/rest-api
- **Advanced Trade WebSocket API**: https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/websocket/websocket-overview
- **International Derivatives Overview**: https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/guides/derivatives/overview
- **Starbase Technical Migration Guide**: https://docs.cdp.coinbase.com/coinbase-app/advanced-trade-apis/guides/derivatives/technical
- **INTX changelog**: https://docs.cdp.coinbase.com/international-exchange/changes/changelog
- **CCXT REST reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/coinbase.ts
- **CCXT WebSocket reference**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/coinbase.ts

## INTX gateway migration status

Coinbase documents a hard September 9, 2026 INTX cutover (schedule subject to
change) with no parallel-running window. The published Starbase contract uses
JSON-RPC 2.0 at `https://drb.coinbase.com/api/v2` and
`wss://drb.coinbase.com/ws/api/v2`; Coinbase Advanced clients keep their CDP
keys and exchange a fresh signed JWT through `public/auth` with
`grant_type: coinbase_cdp`. Direct institutional Deribit accounts use a
different newly provisioned Deribit-key flow and must not be conflated with
this adapter. The implementation plan and cutover gates are tracked in
`docs/safe_cex_audit_july/exchanges/coinbase-starbase-migration.md`.

The first follow-on foundation is implemented in `coinbase.starbase.ts`. It
contains the published HTTP/WS endpoint constants, typed JSON-RPC request and
response handling, monotonic request IDs, an injected HTTP POST transport that
validates exact public/private method grammar before appending `/{method}` to
the HTTP base URL, CDP token exchange/refresh policy,
and unified WebSocket heartbeat primitives. Sanitized live-public and
official-schema fixtures and tests are under
`src/__tests__/audit-regressions/coinbase.starbase-*`.

The second follow-on is implemented in `coinbase.starbase-public.ts`. It is an
inert markets/public-data layer bounded by Coinbase's live public gateway and
published Starbase schemas: exact BTC/ETH linear-crypto-USDC-perpetual instrument
normalization with inverse/fiat/option rejection,
legacy `{BASE}-PERP-INTX` saved-symbol compatibility, ticker/book/candle
mappers, exact public request/channel builders, desired-subscription replay,
and order-book gap/reconnect recovery. Committed sanitized 2026-07-15 public
instrument/book/ticker/trade/WS evidence has exact byte hashes locked in tests.
Public trades corroborate `amount = contracts * contract_size` for BTC and ETH, book
and ticker sizes share that base-amount boundary, and the schema defines open
interest for the captured linear futures in underlying base coin. Mappers
therefore preserve these values directly as `bidBaseAmount`, `askBaseAmount`,
and `openInterestBase` while retaining the upstream schema contradiction in the
migration doc.
Raw-book `new`, `change`, and `delete` actions mean add, update, and remove a
price level. Contradictions are validated before a frame is applied; they clear
the book and require a new snapshot instead of silently becoming upserts.

The third follow-on is implemented in `coinbase.starbase-private.ts`. It is an
inert, read-only private-state layer with exact USDC/future request builders for
account summary, positions, open orders, ID-addressed order state,
currency/instrument order history, and user trades. Order-history requests
deliberately omit `with_continuation` and `continuation`: the current official
response schema publishes only an `order[]` result even though the optional
request parameters describe a different continuation object. Strict gateway
DTOs preserve legacy INTX symbol identity plus raw order/trade IDs, state,
amount, filled amount, and contract-unit fields. Account balance, equity,
available funds, withdrawal funds, initial margin, and maintenance margin are
explicitly named as USDC because the requests select `currency: USDC`. Position
`size_currency` is explicitly base coin and `size` remains explicitly the
gateway's published USD/quote size. Gateway margin and PnL fields stay
gateway-labelled instead of being silently asserted as safe-cex quote values.

Coinbase's migration guide says Starbase `amount` is base coin, but inherited
OpenAPI descriptions still say perpetual `amount` and `filled_amount` can be
USD. The private DTO therefore remains decodable for partial orders and trades,
while the safe-cex order mapper only emits an unfilled order and returns a
typed parity blocker for non-zero fills; historical-trade normalization stays
blocked entirely. The OpenAPI also describes `matching_id` as always null while
typing it as a string, so the decoder follows the published response example
and fails closed on non-null values. These upstream contradictions must be
resolved by a Coinbase schema correction or approved account capture before
write-path or reconciliation cutover.

The fourth follow-on is implemented in `coinbase.starbase-orders.ts`. It is an
inert, transport-free order-write contract layer for the published
`private/buy`, `private/sell`, `private/edit`, `private/cancel`, and
`private/close_position` methods. Exact builders use lower-case gateway order
and time-in-force values and preserve `{BASE}-PERP-INTX` as the saved symbol
while sending `{BASE}_USDC-PERPETUAL`. The technical migration guide takes
precedence for new request construction and explicitly defines `amount` as
base coin; every place-order builder therefore requires a typed total-base
amount whose source is that guide. The inherited OpenAPI parameter prose still
calls perpetual `amount` USD, so contract-unit conversion and all size edits
remain typed blockers instead of inferring total-versus-remaining behavior for
partially filled orders.

The guide's OTOCO example also publishes a `take_limit` child with `price` and
`trigger` but no `trigger_price`; that more specific migration example takes
precedence over the inherited generic trigger prose for this exact child shape.
Stop children and `take_market` still require the OpenAPI `trigger_price`.
Top-level `take_limit` follows the generic OpenAPI request contract and still
requires `trigger_price`; the omission is scoped only to that specific OTOCO
child example.

The request enum admits `market_limit`, but the published response `order_type`
enum does not. The method parameter `valid_until` is an integer timestamp, but
Coinbase publishes neither its unit nor a range. Both remain typed write gates;
the layer does not claim a decodable market-limit lifecycle or emit a guessed
milliseconds/seconds deadline.

Starbase `order_id` is the only canonical write identity. Labels are decoded as
non-canonical display/correlation metadata and the contract layer intentionally
does not expose `edit_by_label` or `cancel_by_label`. A triggered stop-limit's
published `trigger_order_id` is modeled as its pre-trigger predecessor;
`oto_order_ids` are the primary OTOCO order's child IDs; and
`primary_order_id` links a child back to its primary. No missing lifecycle IDs
are manufactured. Filled write responses and returned trades remain parity
blocked until Coinbase resolves the response amount-unit conflict. This module
is not imported by the active exchange, WebSockets, API factory, routing, or
metadata.

The fifth follow-on is implemented in `coinbase.starbase-session.ts`. It is an
inert, transport-free Cancel on Disconnect contract layer for
`private/enable_cancel_on_disconnect`,
`private/disable_cancel_on_disconnect`, and
`private/get_cancel_on_disconnect`. The only published optional parameter is
`scope: connection | account`; omission preserves the gateway's default
connection scope. Enable and disable are explicitly recorded as WebSocket-only
and accept only the exact `"ok"` result. The getter strictly accepts the exact
`{ enabled: boolean, scope }` result object. No active runtime imports this
module, and it does not enable Cancel on Disconnect by itself.

The sixth follow-on is implemented in `coinbase.starbase-ws-session.ts`. It is
an inert lifecycle controller for a future unified WebSocket. A connection is
not ready for subscription replay until it has used a fresh connection-local
`public/auth`, enabled the published heartbeat, and—when an explicit Cancel on
Disconnect policy is supplied—mutated and read back that exact policy. It also
drives the existing 49-minute WebSocket re-auth policy and answers only an
already-classified heartbeat signal. A classified rate-limit disconnect enters
an explicit backoff gate; a future credit-aware transport must acknowledge its
own backoff decision before activation can run again, because Coinbase does not
publish a close code or universal retry delay that this layer could safely
guess.

This controller owns no socket, timer, reconnect loop, subscription method, or
raw-frame parser. Subscription replay is an injected callback so it does not
choose between the conflicting private-subscription examples/contracts in the
current schema. No active runtime imports the controller.

The seventh follow-on is implemented in `coinbase.starbase-cutover.ts`. It is
an inert, data-only cutover assessment. Its enforcement-facing selector accepts
no arguments: callers cannot inject evidence or claim that an account is INTX.
It consumes only repository-owned evidence and remains explicitly product-unbound
until a future integration derives INTX identity from a trusted runtime
portfolio. Therefore spot, CFM, and unclassified traffic all remain on the
existing Coinbase paths; public reads, private reads, and writes stay legacy.

The only synthetic evaluator is named `ForTestsOnly`. It can describe an atomic
HTTP-only comparison plan for a test-classified INTX portfolio after the public
and private HTTP response contracts, HTTP authentication contract, and
credentialed HTTP authentication evidence are ready. That plan consumes only
JSON-RPC method results and always disables writes. It contains no WebSocket
session, subscriptions, or private notifications.

WebSocket session activation and the unpublished private notification envelope
are represented by separate, closed gates. Deterministic repository facts use
`implemented | not-implemented`; credentialed/live facts use
`verified | unverified`, so implemented code cannot be mistaken for live proof.
The HTTP-shadow, WebSocket, and authoritative runtime activation flags are all
literal `false`. Unit parity, credentialed HTTP read parity, credentialed
WebSocket lifecycle, the notification envelope, write canary, and cutover
runbook remain unresolved. Enabling anything requires trusted portfolio binding
and a reviewed code change, not caller input or configuration. The assessment
has no endpoint, transport, store, registry, metadata, or routing import.

The eighth follow-on is implemented in
`coinbase.starbase-public-frames.ts`. It is an inert public-only classifier for
independently captured live envelopes from 2026-07-15. A subscription frame is
accepted only as exact JSON-RPC `method: subscription`, without `id`/`result`,
when `params.channel` exactly matches a desired, acknowledged public channel;
the captured `params` boundary is passed to the existing `{ data }` decoder.
Heartbeat frames accept only `params.type: heartbeat | test_request`, without
an ID. A `public/test` response requires a known positive pending request ID
and exactly one valid `result` or `error`. Its only optional top-level keys are
the proven gateway metadata `usIn`, `usOut`, `usDiff`, and `testnet`; timestamps
must be positive safe integers, the difference non-negative, and jointly
present timing values must agree. Every other own key is rejected.
Canonical error codes must be safe integers; integer-looking values outside
JavaScript's exact range fail closed.

Before `JSON.parse`, a bounded JSON grammar scan rejects duplicate object keys
at every depth after decoding escapes, including spellings such as `id` and
`\u0069d`. This prevents last-key-wins parsing from bypassing exact envelope or
result/error checks while preserving arrays and escaped string contents.

The classifier does not infer private channels from public evidence. Every
non-acknowledged subscription, including private-looking names, remains
explicitly unsupported and unclassified. The published AsyncAPI still omits
the outer envelope, so the source capture hashes are pinned in code and the
migration doc. The classifier owns no socket, dispatch, subscription state,
store, routing, or active import, and its activation constant is literal
`false`.

`private/get_user_trades_by_order` is also deferred: its published response
example contains a trade array, but `PrivateGetUserTradesByOrderResponse`
defines no `result` property. The inert reader uses the coherent
`private/get_user_trades_by_currency*` response family until Coinbase publishes
an exact by-order result schema.

All eight modules are intentionally inert: no current exchange, WebSocket, API
factory, metadata, or shared-routing file imports them. Do not wire them into
the active adapter until read-only private state passes source-of-truth parity
and all normalization blockers are resolved. The public mapper intentionally
supports only BTC/ETH linear USDC perpetuals; discovery requests USDC futures
and filters those exact names before strict decoding because the live result
includes additional perpetuals and dated futures. Expanded product support,
an exact amount increment, and maximum base amount remain gated; live
`contract_size` and `min_trade_amount` are positive decimals but neither is
treated as an unpublished amount step. The inert market-parity assessment
returns typed blockers for both missing market fields instead of manufacturing
a safe-cex `Market`. Coinbase has not published
the inbound notification envelope in its Starbase OpenAPI or AsyncAPI schema.
The inert public classifier is therefore bounded to the pinned 2026-07-15 live
public captures; the controller still accepts an already-classified signal
because the two modules are not wired together. Public stream mappers start at
the AsyncAPI channel payload's published `{ data }` shape, reached through the
captured notification `params` boundary.

The private read layer currently consumes only JSON-RPC method results.
Although AsyncAPI lists private portfolio/order/position channels, Coinbase has
not published the unified socket's outer inbound notification envelope.
Authenticated notification classification and live snapshot/delta
reconciliation therefore remain deferred; do not infer Deribit envelopes from
channel names. The pure read-state holder validates a complete result before
replacement, so malformed or duplicate REST snapshots preserve the
last-known-good account, positions, open orders, ID-addressed order state,
order history, or trade page. Order-state results must echo the requested
canonical `order_id`, and instrument-history rows must all match the requested
Starbase instrument. Account decoding also preserves Coinbase's optional
`total_*_usd` cross-collateral fields under explicit USD names; that does not
resolve the unlabelled PnL-unit blocker.

## File Structure

| File                       | Description                                                                                                                                                             |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `coinbase.exchange.ts`     | Main exchange class (`CoinbaseExchange`) extending `BaseExchange`. Handles portfolio detection, balance/ticker/position fetching, order placement, and OHLCV retrieval. |
| `coinbasespot.exchange.ts` | Spot-only saved-account compatibility variant (`CoinbaseSpotExchange`). Currently disabled for new accounts in metadata.                                                |
| `coinbase.api.ts`          | Axios-based REST client factory. Configures JWT auth interceptor, retry logic (3 retries on network errors), proxy/tunnel support, and public endpoint bypass.          |
| `coinbase.types.ts`        | All TypeScript types: REST response shapes, WebSocket event types, endpoint constants, order configuration types, and lookup maps for order side/type/TIF.              |
| `coinbase.utils.ts`        | JWT signing via Web Crypto API (ES256/ECDSA P-256), PEM-to-PKCS8 conversion, order state/type formatting helpers.                                                       |
| `coinbase.ws-public.ts`    | Public WebSocket: handles `candles`, `market_trades`, `l2_data`, and `heartbeats` channels. Manages order book snapshots/updates and OHLCV streaming.                   |
| `coinbase.ws-private.ts`   | Private WebSocket: handles `user` channel for real-time order updates (snapshot + incremental) and `heartbeats`. Emits fill events on order completion.                 |
| `coinbase.products.json`   | Sample product data for reference/testing.                                                                                                                              |
| `coinbase.starbase.ts`     | Inert Starbase JSON-RPC, auth-refresh, endpoint, HTTP transport, and heartbeat primitives. Not wired to active Coinbase traffic.                                        |
| `coinbase.starbase-public.ts` | Inert Starbase instrument/public-data normalization, public request/subscription contracts, saved-symbol compatibility, and sequenced-book recovery. Not routed.        |
| `coinbase.starbase-private.ts` | Inert Starbase read-only account/position/order-state/order-history/trade request contracts, strict gateway DTOs, last-known-good snapshot holder, and typed safe-cex parity gates. Not routed. |
| `coinbase.starbase-orders.ts` | Inert Starbase order-write builders, strict result decoders, canonical-ID lifecycle links, and typed amount-unit parity gates. No transport and not routed. |
| `coinbase.starbase-session.ts` | Inert Starbase Cancel on Disconnect request/response contracts, including explicit WebSocket-only mutation metadata. No transport and not routed. |
| `coinbase.starbase-ws-session.ts` | Inert unified-WebSocket activation, re-auth, heartbeat, Cancel on Disconnect proof, subscription-replay ordering, and rate-limit backoff gate. No socket, classifier, routing, or active import. |
| `coinbase.starbase-cutover.ts` | Inert product-unbound cutover assessment: repository-owned evidence, test-only HTTP JSON-RPC shadow evaluation, separately closed WebSocket gates, legacy authority, and compile-time-disabled activation. No active import. |
| `coinbase.starbase-public-frames.ts` | Inert public-only raw text-frame classifier bounded to pinned live notification/heartbeat envelopes and known `public/test` IDs. Private subscriptions remain unsupported/unclassified; activation is false. |

## Authentication

Coinbase uses **JWT-based authentication**. Existing CDP keys may be ES256
(ECDSA P-256 PEM), while current CDP-created keys may be Ed25519 base64
secrets:

- **API Key** (`options.key`): Used as the JWT `kid` (Key ID) and `sub` (Subject) claims
- **API Secret** (`options.secret`): Either an EC private key in PEM format
  (SEC1 or PKCS8) or an Ed25519 base64 key secret
- Advanced Trade downloaded JSON credentials are accepted directly: `name` is
  used as the API key (`organizations/{org_id}/apiKeys/{key_id}`), while
  `privateKey` / `private_key` / `keySecret` / `key_secret` / `secret` is used
  as the API secret. A bare key ID or unsupported secret shape fails before
  request setup with an actionable validation message.
- The `sign()` function in `coinbase.utils.ts`:
  1. Constructs a JWT header with `alg: "ES256"` or `alg: "EdDSA"`, `typ: "JWT"`, `kid`, and a random `nonce` (UUID)
  2. Constructs a payload with `nbf` (current time), `exp` (120 seconds from now), `sub`, and optionally `uri` (for REST requests). Ed25519 keys use Coinbase's current `iss: "cdp"` and `aud: ["cdp_service"]` claims; ES256 keeps the existing `iss: "coinbase-cloud"` payload for compatibility.
  3. Signs ES256 with ECDSA P-256 + SHA-256 via `crypto.subtle`, or Ed25519 with `@noble/ed25519`
  4. The ES256 `CryptoKey` is cached after first import to avoid repeated PEM parsing (`cachedCryptoKey`/`cachedKeySecret` module-level variables in `coinbase.utils.ts`)
- **PEM conversion** (`coinbase.utils.ts`): Coinbase provides SEC1 format keys (`-----BEGIN EC PRIVATE KEY-----`) but Web Crypto API requires PKCS8. The `pemToPkcs8Der()` function detects the format and wraps SEC1 keys in a PKCS8 `PrivateKeyInfo` ASN.1 structure with P-256 OIDs. PKCS8 keys (`-----BEGIN PRIVATE KEY-----`) are passed through as-is.
- **Escaped newlines**: The secret has `\\n` replaced with actual `\n` before parsing (`coinbase.utils.ts`)
- **Time source**: Uses `virtualClock.getCurrentTimeMs()` for JWT timestamps, not `Date.now()`
- **REST requests**: The `uri` parameter is `"METHOD api.coinbase.com/path"` (e.g., `"GET api.coinbase.com/api/v3/brokerage/products"`)
- **Hosted REST signing**: When `options.remoteSigner` is present, the server-side interceptor must not call the local ES256 signer because hosted mode has `secret: ''`. It returns the unsigned private request for `createRemoteSigningAdapter`, which delegates signing to the browser/device copy of this same interceptor.
- **WebSocket auth**: JWT is generated without a `uri` parameter and sent in the subscribe message payload. Hosted private WebSocket subscriptions use the `coinbase-jwt` `ws-auth` output format and carry the resulting JWT in `wsAuthSignature`.
- **Header format**: `Authorization: Bearer <jwt>` (when using tealstreetTunnel) or `Coinbase-Authorization: Bearer <jwt>` (direct connection) (`coinbase.api.ts`)

## Startup Sequence

### `start()`

1. **External public data**: `initExternalPublicData()` runs first. When a
   shared provider is available, local market loading and the public WebSocket
   are skipped.
2. **Portfolio detection**: Private mode selects a permitted portfolio before
   loading markets and reuses that selection during private startup. Public-only
   mode intentionally skips this authenticated call and supports INTX public
   data only.
3. **Market loading**: Without an external provider, `fetchMarkets()` loads
   markets, updates `loaded.markets`, and starts the public WebSocket. In
   public-only mode the market source is `fetchMarketsFromIntxInstruments()`.
4. **Branching**:
   - **Private mode** (`!publicDataOnly`): calls `_startPrivate()`
   - **Public-only mode**: calls `_startPublicDataOnly()`

### `_startPrivate()`

1. **Private WebSocket**: `privateWebsocket.connectAndSubscribe()` (non-blocking).
2. **Portfolio reuse**: the portfolio selected by `start()` is reused. `_startPrivate()` only calls `findSelectedPortfolio()` if startup did not select one yet; this avoids duplicate `/api/v3/brokerage/portfolios` calls during account load.
3. **First tick**: `tick()` is called, which sequentially fetches tickers, balance, positions, and, only while orders are still unloaded, a throttled REST open-order bootstrap snapshot.
4. **Order authority**: orders are loaded and refreshed from the WebSocket `user` channel snapshot/updates. REST open-order fetching is a bootstrap/manual fallback only; failed REST order reconciliation rethrows and does not mark orders loaded, so last-known-good order state remains in the store.
5. **Snapshot batching**: Coinbase sends open-order `user` snapshots in
   batches of 50. `coinbase.ws-private.ts` accumulates snapshot frames until
   the first batch with fewer than 50 raw orders before replacing the order
   store; otherwise accounts with more than 50 open orders would show only the
   last snapshot batch.
6. **REST active-status filter**: `/api/v3/brokerage/orders/historical/batch`
   rejects multiple active `order_status` values in one request. The bootstrap
   fallback queries `PENDING`, `OPEN`, `QUEUED`, `CANCEL_QUEUED`, and
   `EDIT_QUEUED` one status at a time and deduplicates by order ID.

### `_startPublicDataOnly()`

Fetches public INTX tickers, sets empty orders and positions plus a zero
balance, and marks those private-data stores loaded.

### `tick()` loop

Runs on a 1-second interval via `loop()`. Each tick:

1. **Fetches tickers** (REST poll) -- dispatches `poll_book.{symbol}` and `poll_kline.{symbol}` callbacks for each ticker to update OB/kline subscribers.
2. **Fetches balance** (REST poll).
3. **Fetches positions** (REST poll).
4. Schedules the next `tick()` via `loop(fn, tickInterval)`.

Each fetch is sequential within a tick. Errors are caught and logged but do not stop the loop.

### `findSelectedPortfolio()`

1. Fetches all portfolios via `LIST_PORTFOLIOS`.
2. Attempts `PORTFOLIO_BREAKDOWN` for each portfolio in parallel (`Promise.all`). The first portfolio that succeeds (no auth error) is selected.
3. If no portfolio has API permissions, throws `'No portfolio with permissions found'`.
4. **CFM detection hack**: If the selected portfolio is not INTX, tries to call `BALANCE_CFM`. If that succeeds (returns `balance_summary`), the portfolio type is overridden to `'CFM'`.

## REST API Endpoints

All endpoints are defined in `ENDPOINTS` constant in `coinbase.types.ts`:

| Endpoint              | Path                                                         | Description                                                                                                 |
| --------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `MARKETS`             | `/api/v3/brokerage/products`                                 | List products (spot + futures). Supports `product_type=FUTURE` and `contract_expiry_type=PERPETUAL` params. |
| `LIST_PORTFOLIOS`     | `/api/v3/brokerage/portfolios`                               | List all portfolios for the account                                                                         |
| `PORTFOLIO_BREAKDOWN` | `/api/v3/brokerage/portfolios/{portfolio_uuid}`              | Portfolio breakdown with spot positions, perp positions, futures positions                                  |
| `BALANCE`             | `/api/v3/brokerage/intx/portfolio/{portfolio_uuid}`          | INTX portfolio balance (total_balance, buying_power)                                                        |
| `BALANCE_CFM`         | `/api/v3/brokerage/cfm/balance_summary`                      | CFM futures balance summary                                                                                 |
| `LIST_POSITIONS_INTX` | `/api/v3/brokerage/intx/positions/{portfolio_uuid}`          | INTX perpetual positions                                                                                    |
| `LIST_POSITIONS_CFM`  | `/api/v3/brokerage/cfm/positions`                            | CFM futures positions                                                                                       |
| `KLINE`               | `/api/v3/brokerage/products/{product_id}/candles`            | Historical candlestick data                                                                                 |
| `ORDER`               | `/api/v3/brokerage/orders`                                   | Place orders (POST)                                                                                         |
| `EDIT_ORDER`          | `/api/v3/brokerage/orders/edit`                              | Native stable-ID edit for a strictly reconciled spot LIMIT subset                                          |
| `CANCEL_ORDER_BATCH`  | `/api/v3/brokerage/orders/batch_cancel`                      | Batch cancel orders (POST, max 100 per batch)                                                               |
| `INTX_INSTRUMENTS`    | `https://api.international.coinbase.com/api/v1/instruments/` | INTX instruments with live quotes (external URL, treated as public endpoint)                                |
| `LIST_ACCOUNTS`       | `/api/v3/brokerage/accounts`                                 | List accounts (type defined but currently unused)                                                           |
| `ORDER_BOOK`          | `/api/v3/brokerage/product_book`                             | REST order book (type defined but order book is fetched via WebSocket)                                      |

### Public vs Private Endpoints

Only `INTX_INSTRUMENTS` is treated as public by the active adapter (no JWT auth and remote-signing skip tag). Coinbase also publishes unauthenticated Advanced Trade market-data routes under `/api/v3/brokerage/market/...`, but this adapter still uses the authenticated private equivalents (`/products`, `/product_book`, `/products/{id}/candles`) for account-backed sessions. Do not mark all public-data mode traffic as remote-signing-skippable; tag only URL-proven public endpoints.

### Base URLs

- **REST**: `https://api.coinbase.com` (both livenet and testnet -- no testnet support)
- **INTX instruments**: `https://api.international.coinbase.com` (direct external URL)

### Proxy Handling for External URLs (`coinbase.api.ts`)

When a request URL starts with `http` (i.e., the INTX instruments endpoint which is a full URL rather than a relative path), the private proxy URL is prepended if enabled: `${privateProxyUrl}/${config.url}`.

## WebSocket Implementation

### Public WebSocket (`coinbase.ws-public.ts`)

**URL**: `wss://advanced-trade-ws.coinbase.com` (same for livenet/testnet)

| Channel         | Handler                         | Description                                                                                                                                     |
| --------------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `heartbeats`    | `handlePongEvent()`             | Connection keepalive. No manual ping needed.                                                                                                    |
| `candles`       | `handleOhlcvStreamEvents()`     | Real-time candlestick updates per product. Subscribed on-demand when `listenOHLCV()` is called.                                                 |
| `market_trades` | `handleTradeStreamEvents()`     | Real-time trade events. Used as secondary data source to patch candles, especially for intervals < 5 minutes where candle updates are too slow. |
| `l2_data`       | `handleOrderBookStreamEvents()` | Level 2 order book with snapshot + incremental updates. Subscribed on-demand when `listenOrderBook()` is called.                                |

**Default subscriptions on connect**: Only `heartbeats` is subscribed automatically. `candles`, `market_trades`, and `l2_data` are subscribed on-demand.

**Subscription format**:

```json
{
  "type": "subscribe",
  "product_ids": ["BTC-USD"],
  "channel": "candles"
}
```

**Unsubscription**: Same format with `"type": "unsubscribe"`.

**No authentication on public WebSocket**: Unlike the private WebSocket, public subscribe messages do not include a `jwt` field.

**Reconnection logic**: Inherited from `BaseWebSocket`. On reconnection:

- `resetOrderbook()` is called in `onOpen()` to clear stale order book state (`coinbase.ws-public.ts`)
- All topics in `this.topics` are re-subscribed via `subscribe()` (`coinbase.ws-public.ts`)
- If `ENABLE_RECONNECTING_WEBSOCKET` is true, emits `wsReconnected` event (`coinbase.ws-public.ts`)

**Ping/pong**: The `ping()` method is a **no-op** (`coinbase.ws-public.ts`). Coinbase WebSockets rely on server-sent heartbeats. The `BaseWebSocket.trackLastReceived()` is called on every message. If no message is received for 20 seconds (`WS_RECONNECT_THRESHOLD_MS`), the base class triggers reconnection.

**Error handling**: If a WebSocket message has `type: "error"`, it is emitted via `this.parent.emitter.emit('error', json.message)` and processing stops (`coinbase.ws-public.ts`).

Public candle frames may contain products that have no active local listener.
`handleOhlcvStreamEvents()` skips those products before dispatch and stats
recording, so one unsolicited product cannot abort the remaining products in
the same frame.

**On-demand subscription pattern** (`listenOHLCV`, `listenOrderBook`):

1. Uses `waitForConnectedAndSubscribe()` which polls `this.isConnected` every 100ms until ready
2. Only sends a subscribe message if no existing handler exists for that topic (avoids duplicate subscriptions)
3. Returns an unsubscribe function that removes handlers, decrements topic count, and sends unsubscribe if no handlers remain

### Order Book Specifics

- Snapshot (`type: 'snapshot'`): Clears bids/asks and rebuilds from scratch (`coinbase.ws-public.ts`)
- Updates (`type: 'update'`): Only applied after snapshot is loaded (tracked via `orderBookInnerState[innerId].isSnapshotLoaded`)
- Side mapping: `'offer'` maps to `asks`, `'bid'` maps to `bids`
- After each update: sorts order book and recalculates totals via `sortOrderBook()` / `calcOrderBookTotal()`
- Each `listenOrderBook` call gets a unique `innerId` via `nanoid()` for independent snapshot tracking

### OHLCV Streaming Details

For each `listenOHLCV` call, **two channels** are subscribed:

1. **Primary**: `candles.{symbol}` -- receives candle updates from Coinbase
2. **Secondary**: `market_trades.{symbol}` -- receives individual trades

**Interval-dependent behavior** (`coinbase.ws-public.ts`):

- For intervals >= 5 minutes: Uses candle channel data, ignores trade channel for candle construction
- For intervals < 5 minutes (`relyOnTrades = true`): Ignores candle channel data, builds candles from individual trades instead (because Coinbase candle updates are too infrequent at sub-5min intervals)

**Trade-based candle construction** (`coinbase.ws-public.ts`):

- Constructs OHLCV from individual trades using `timestamp - (timestamp % intervalInSeconds)` for candle alignment
- Detects new candle periods and resets OHLC to the trade price
- Volume is calculated as `size / price` (converted from quote to base volume)

**Daily/weekly/monthly candles** (`coinbase.ws-public.ts`):

- TradingView only interpolates sub-daily candles, so the code manually accumulates daily+ data
- If `dailyCandles[symbol]` is not populated, triggers an async fetch of the last 2 daily candles
- Accumulates high/low/close/volume from incoming candle data into the stored daily candle

### Private WebSocket (`coinbase.ws-private.ts`)

**URL**:

- **Direct**: `wss://advanced-trade-ws-user.coinbase.com`
- **Via proxy/tunnel**: the `wsPrivate` route returned by
  `resolveExchangeRouting()`; the shared routing table is authoritative and no
  hostname is hardcoded here

| Channel      | Handler             | Description                                                                                     |
| ------------ | ------------------- | ----------------------------------------------------------------------------------------------- |
| `user`       | `handleUserEvent()` | Order snapshots and updates. Provides real-time order fills, cancellations, and status changes. |
| `heartbeats` | `handlePongEvent()` | Connection keepalive                                                                            |

**Private subscription includes JWT in the message** (`coinbase.ws-private.ts`):

```json
{
  "type": "subscribe",
  "channel": "user",
  "jwt": "<signed-jwt-without-uri>"
}
```

Both `user` and `heartbeats` channels are subscribed sequentially in a loop, each with a fresh JWT. In hosted mode, each JWT is requested from `options.remoteSigner`; do not gate this path on `options.secret`, because the hosted server never holds the secret.

**Reconnection behavior** (`coinbase.ws-private.ts`):

- On open, calls `subscribe()` to re-authenticate
- If `ENABLE_RECONNECTING_WEBSOCKET` is true, calls `this.parent.fetchAndSetPrivateSnapshot()` (which is a no-op for Coinbase)
- If per-channel JWT signing throws (remote-signer offline, brief network blip), the handler closes the socket via `this.ws?.close()` — never `forceClose()`. `forceClose()` sets `forceClosed=true` and the base reconnect path in `base.ws.ts` refuses to reconnect until an explicit `forceOpen()`; a plain `close()` lets the exponential-backoff/failure-streak circuit breaker retry, which is the right posture for a transient hosted-signer glitch.
- A monotonic `subscriptionAttempt` counter guards `subscribe()` and `onOpen`: a stale in-flight JWT from a previous socket generation is dropped instead of being sent on a fresh socket.

**Grafana logging**: If `ENABLE_PWS_LOG` is true (default), every private WebSocket message is emitted as `'grafana-pws'` event (`coinbase.ws-private.ts`)

**Order snapshot authority**: Coinbase sends snapshot order rows in batches of 50; accumulate them until the first batch with fewer than 50 raw orders before replacing the store. An upstream empty final snapshot canonically clears the store and marks orders loaded. A non-empty final snapshot whose every order is quarantined for missing or invalid market/CFM metadata is not authoritative: it preserves last-known-good orders and their loaded state. Mixed snapshots replace the store with valid siblings and emit an error for each skipped record. Treat `CANCEL_QUEUED` as an open queued-cancel state per Coinbase's official user-channel schema unless Coinbase documents a behavior change.

## Order Types and Mappings

### Supported Order Configurations

| Order Type  | Configuration Key           | Details                                                                                              |
| ----------- | --------------------------- | ---------------------------------------------------------------------------------------------------- |
| Market      | `market_market_ioc`         | IOC market order with `base_size`                                                                    |
| Limit (GTC) | `limit_limit_gtc`           | Limit order with `base_size`, `limit_price`, optional `post_only`                                    |
| Stop Loss   | `stop_limit_stop_limit_gtc` | Stop-limit GTC with `base_size`, `limit_price` (1% offset from stop), `stop_price`, `stop_direction` |

### Order Type Mapping (from WebSocket events)

| Coinbase Type  | Internal Type        |
| -------------- | -------------------- |
| `"Market"`     | `OrderType.Market`   |
| `"Limit"`      | `OrderType.Limit`    |
| `"Stop Limit"` | `OrderType.StopLoss` |

Unknown order types default to `OrderType.Limit` with a `console.error` (`coinbase.utils.ts`).

### Order Side Mapping

| Coinbase Side | Internal Side    |
| ------------- | ---------------- |
| `"BUY"`       | `OrderSide.Buy`  |
| `"SELL"`      | `OrderSide.Sell` |

### Order Status Mapping

| Coinbase Status | Internal Status        |
| --------------- | ---------------------- |
| `PENDING`       | `OrderStatus.Open`     |
| `OPEN`          | `OrderStatus.Open`     |
| `FILLED`        | `OrderStatus.Closed`   |
| `CANCELLED`     | `OrderStatus.Canceled` |
| `EXPIRED`       | `OrderStatus.Canceled` |
| `FAILED`        | `OrderStatus.Canceled` |

Unknown statuses default to `OrderStatus.Open` with a `console.error` (`coinbase.utils.ts`).

## Order Lifecycle

### Account Identity

`getAccount()` returns the UUID of the selected permitted portfolio as the
stable `userId`. When startup has not selected a portfolio yet, it uses the
same `findSelectedPortfolio()` permission probe as the private startup path;
it never calls a parameterized INTX endpoint without a portfolio UUID.

### Order Placement (`coinbase.exchange.ts`)

1. `placeOrder()` calls `formatCreateOrder()` to build the payload.
2. Sends POST to `ENDPOINTS.ORDER`.
3. Checks `data.success`:
   - If `false`, prefers the documented `new_order_failure_reason`, then the
     preview and legacy failure fields.
4. Returns the documented `success_response.order_id`, with the legacy
   top-level `order_id` accepted as a compatibility fallback. A successful
   response without either ID is rejected rather than returning `undefined`.

### Order Format (`coinbase.exchange.ts`)

- **Client order ID**: `"tealstreet-" + uuid()` (`coinbase.exchange.ts`)
- **Leverage**: Always `"10"` (hardcoded)
- **Market orders**: Uses `market_market_ioc` with `base_size` only
- **Limit orders**: Uses `limit_limit_gtc` with `base_size`, `limit_price`, and `post_only` (when `timeInForce === OrderTimeInForce.PostOnly`)
- **Stop loss orders**: Uses `stop_limit_stop_limit_gtc`:
  - `stop_direction`: `STOP_DIRECTION_STOP_UP` for buy-side stops, `STOP_DIRECTION_STOP_DOWN` otherwise
  - `limit_price` is calculated as 1% offset from stop price: `stopPrice * 1.01` (buy stops) or `stopPrice * 0.99` (sell stops)
  - Both `limit_price` and `stop_price` are rounded to the market's price precision

### Order Update (`coinbase.exchange.ts`)

Non-INTX updates are native-edit-or-fail-closed. `updateOrder()` calls
`EDIT_ORDER` only for a canonical private-WebSocket row whose market metadata
proves spot/base units and whose raw order proves the same stable ID, product,
side, `OPEN` + ordinary `Limit` shape, no trigger/protection, zero fill, and
the exact full current price and size. Raw numeric provenance uses strict
unsigned-decimal strings, lexical zero checks, and exact decimal equality; it
does not coerce exchange strings through JavaScript `Number` for safety gates.

Coinbase requires `order_id`, `price`, and `size` together, so both full values
are sent even for a one-field update. Success requires `success: true` and an
empty documented `errors` array, and the canonical order ID is returned
unchanged. Response errors are surfaced; there is no cancel/recreate fallback.
Partial fills, triggers, non-canonical rows, CFM contract sizes, and unknown
unit provenance all fail before an exchange write.

INTX alone retains the existing cancel-then-place lifecycle until its Starbase
migration. This exception is selected by the authenticated INTX portfolio and
must not be widened to other non-ready shapes.

### Batch Cancel (`coinbase.exchange.ts`)

- Order IDs are chunked into batches of 100 via `lodash/chunk`
- Batches are processed **sequentially** via `forEachSeries`
- After each batch cancel, `store.updateOrdersStatus()` is called to mark those orders as canceled locally

### Open Feature: Scaled Limit / Ladder Orders

Verified against the official Advanced Trade Create Order docs on 2026-09-17:
Coinbase publishes `scaled_limit_gtc` as an `order_configuration` shape on the
existing `POST /api/v3/brokerage/orders` endpoint. The docs describe it as a
scaled order that divides a large order into multiple smaller limit orders
placed incrementally across a user-defined price range. This is not a separate
batch-create endpoint; the documented batch endpoint remains
`/api/v3/brokerage/orders/batch_cancel` for canceling orders.

The published `ScaledLimitGtc` schema includes explicit child `orders`
(`LimitLimitGtc[]`) plus declarative ladder fields: `quote_size`, `base_size`,
`num_orders`, `min_price`, `max_price`, `price_distribution`
(`FLAT`, `LINEAR_INCREASING`, `LINEAR_DECREASING`,
`CUSTOM_PRICE_DISTRIBUTION`), `size_distribution` (`INCREASING`, `DECREASING`,
`EVENLY_SPLIT`, `CUSTOM_SIZE_DISTRIBUTION`), `size_diff`, and `size_ratio`.

The active adapter does not currently support this: `formatCreateOrder()` emits
only `market_market_ioc`, `limit_limit_gtc`, and `stop_limit_stop_limit_gtc`.
Before adding Tealstreet ladder-order support, verify live response identity
semantics (parent order ID vs child IDs), private WebSocket child-order events,
product availability across spot/CFM/INTX, and how local cancel/update
reconciliation should treat the generated child orders.

### Order State from WebSocket (`coinbase.ws-private.ts`)

**Snapshot** (`type === 'snapshot'`): A genuinely empty snapshot clears the
store. A snapshot with valid mapped orders replaces the store and marks orders
loaded. A non-empty snapshot whose records are all quarantined preserves the
last-known-good orders and loaded state.

**Update** (`type === 'update'`): Uses `store.addOrUpdateOrders()` to merge. For filled orders, emits a `'fill'` event with:

- `price`: from `avg_price` or `'market'` if absent
- `amount`: from `order.filled` (cumulative quantity)

**Order price resolution** (`coinbase.ws-private.ts`): Uses `stop_price` if present, otherwise `limit_price`, otherwise `'0'`. This means stop-limit orders show their stop price, not their limit price.

**Contract size handling** (`normalizeCfmContractSize`): Expiring CFM products require a positive finite contract-size multiplier; invalid market metadata is quarantined during market loading. CFM orders and positions resolve by product ID and use that same validator before mapping `amount`, `filled`, `remaining`, or contracts to base units. An unresolved market or invalid CFM multiplier is skipped with a non-secret exchange error rather than guessed as `1`; valid `1` remains a valid multiplier. INTX and spot order quantities stay in their API-provided base units.

### Not Yet Implemented

- TakeProfit orders (commented out in `formatCreateOrder`)
- TrailingStopLoss orders (commented out)
- Scaled limit / ladder orders via Coinbase `scaled_limit_gtc`
- `trigger_bracket_gtc` / `trigger_bracket_gtd` (types defined but not used)
- `limit_limit_gtd` (types defined but not used)
- `sor_limit_ioc` (types defined but not used)
- `fetchAndSetPrivateSnapshot` (empty method)

## Symbol Format

- **Spot**: `BTC-USDC`, `ETH-USD` (base-quote with hyphen)
- **CFM Futures**: Product IDs as returned by the API (e.g., `BIT-28AUG24-CDE` for dated futures)
- **INTX Perpetuals**: `BTC-PERP-INTX` format (the exchange appends `-INTX` to the INTX instrument symbol in `fetchTickersIntx`)
- **USDC/USD quirk**: The WebSocket public channels substitute `USDC` with
  `USD` in symbol lookups because Coinbase treats ordinary `*-USDC` books as
  equivalent to `*-USD`. REST candle requests, public WebSocket
  subscribe/unsubscribe payloads, OHLCV handler keys, reconnect stats, and
  unsubscribe cleanup all use the normalized symbol so `BTC-USDC` charts do not
  fail with a Coinbase symbol error or leak stats. `USDT-USDC` and `EURC-USDC`
  remain exact exceptions.
- **Default quote**: `USD`
- **Symbol conversion**: When converting from other exchanges, a hyphen is inserted before `USD` if not present (e.g., `BTCUSD` becomes `BTC-USD`)

## Data Precision & Conversion

### Price and Amount Rounding

- **Price precision**: Derived from `price_increment` in the product data (e.g., `"0.01"`). Stored as `market.precision.price` as a float.
- **Amount precision**: Derived from `base_increment` (e.g., `"0.001"`). Stored as `market.precision.amount` as a float.
- **Rounding function**: Uses `adjust(value, precision)` from `../../utils/safe-math` which rounds to the nearest step of the given precision (not decimal places).
- Both price and amount are adjusted before order placement (`coinbase.exchange.ts`).
- **Stop loss limit price**: `adjust(stopPrice * (1.01 or 0.99), pPrice)` -- applies price precision after the 1% offset calculation.

### String-to-Number Conversions

All Coinbase API responses return numeric values as strings. The code uses `parseFloat()` throughout for conversion. Notable patterns:

- **Empty string handling**: For tickers, empty strings for price/volume/percentage default to `0` (`coinbase.exchange.ts`)
- **Funding interval**: Parsed from microseconds to seconds: `parseFloat(marketIntx.funding_interval || '0') / 1000 / 1000` (`coinbase.exchange.ts`)

### Position Size Calculations

- **CFM**: `contracts = multiply(rawSize, contractSize)` -- size is in number of contracts, multiplied by a validated positive finite contract size for base amount. Invalid or unresolved products are skipped rather than emitted in a guessed unit. Notional = `size * markPrice * contractSize`.
- **INTX**: `contracts = abs(net_size)` -- already in base units. Notional = `size * markPrice`. UPNL is manually calculated: `(side === Long ? 1 : -1) * size * (markPrice - entryPrice)` rather than using the API's `unrealized_pnl`.

### Candle Timestamps

- REST API returns `start` as a Unix timestamp string (seconds). Stored as-is (integer seconds).
- WebSocket candle events also use `start` as seconds, parsed with `parseInt()`.
- Trade timestamps are ISO 8601 strings parsed via `dayjs(rawTime).unix()`.

## Rate Limiting

- **maxRPS**: 3 requests/second (applied via `axios-rate-limit` on the primary `xhr` instance, `coinbase.exchange.ts`)
- **unlimitedXHR**: A separate Axios instance without rate limiting (`coinbase.exchange.ts`), available for batch operations (currently not used in active code paths)
- **Request timeout**: 20 seconds, set in the request interceptor for both public and private endpoints (`coinbase.api.ts`), overriding the default `RECV_WINDOW` of 20 seconds
- **Network retry**: 3 retries on network errors via `axios-retry` with `isNetworkError` condition (`coinbase.api.ts`)
- **Tick loop pacing**: The `tick()` loop sleeps for `tickInterval` (1000ms) between iterations via `loop()`, which calls `sleep(minInterval)` before scheduling the next call

### Effective Request Pattern per Tick

Each tick makes (at minimum):

1. `MARKETS` or `INTX_INSTRUMENTS` (for tickers, 1-2 calls depending on supported types)
2. `BALANCE` or `BALANCE_CFM` (1 call)
3. `LIST_POSITIONS_INTX` or `LIST_POSITIONS_CFM` (1 call for futures types)
4. `LIST_ORDERS` (only while orders are not loaded yet, and at most every 10 seconds if the first attempt fails)

With 3 RPS limit, a single tick cycle takes approximately 1-2 seconds of API time; order polling is intentionally throttled separately to avoid turning the 1-second ticker/balance loop into an order-history loop.

## State Reconciliation

### Orders

- **Initial load**: Orders are hydrated from the WebSocket `user` channel snapshot and from REST `GET /api/v3/brokerage/orders/historical/batch` when the initial snapshot is missing.
- **WebSocket refresh** (`coinbase.ws-private.ts`): the `user` channel is the automatic order source. Private WS opens/reconnects do not kick off a REST order snapshot; the channel's own snapshot/update events refresh the store without spending REST budget.
- **REST fallback** (`coinbase.exchange.ts`): `fetchOrders()` requests only active statuses (`PENDING`, `OPEN`, `QUEUED`, `CANCEL_QUEUED`, `EDIT_QUEUED`) and paginates with `cursor`. The tick loop uses this only while orders are still unloaded, throttled to at most every 10 seconds. Manual worker calls can still invoke it. Rate limits and other transport failures are non-authoritative: they preserve last-known-good store state by throwing before any caller can mark the failed snapshot loaded.
- **Snapshot handling** (`coinbase.ws-private.ts`): Empty or valid-bearing
  snapshots are authoritative and replace the store. Invalid-only non-empty
  snapshots preserve last-known-good state; mixed snapshots retain their valid
  siblings.
- **Update handling** (`coinbase.ws-private.ts`): On an `update` event, `store.addOrUpdateOrders(orders)` is called, which merges by order ID.
- **Local cancel optimism** (`coinbase.exchange.ts`): After sending a batch cancel REST request, `store.updateOrdersStatus()` immediately marks orders as canceled locally, before the WebSocket confirms it.
- **No deduplication**: The WebSocket handler does not deduplicate orders. If the same order appears in both snapshot and update, `addOrUpdateOrders` handles the merge by ID.

### Positions

- **Fetched via REST only**: Positions are loaded every tick via `fetchPositions()`. There is no WebSocket position channel (the `handlePositionTopic` method in the private WS is an empty stub, `coinbase.ws-private.ts`).
- **Fake empty positions**: For every market without an open position, a zero-size `Position` object is created with default values (leverage=10, side=Long, entryPrice=0). This is required by the `hasEmptyPositions` metadata flag.

### Tickers

- **REST polling only**: Tickers are refreshed every tick. There is no active WebSocket ticker subscription (the `ticker_batch` handler is commented out in `coinbase.ws-public.ts`).
- **Ticker-driven callbacks**: After tickers are fetched, the code iterates over them and triggers `poll_book.{symbol}` and `poll_kline.{symbol}` callbacks for order book and candle subscribers (`coinbase.exchange.ts`). This provides a ticker-as-proxy update path for subscribers.

### Balance

- **INTX UPNL from positions**: For INTX portfolios, `upnl` is calculated as the sum of all position-level `unrealizedPnl` from the store, **not** from the balance API response (`coinbase.exchange.ts`). The comment explains: "need to get upnl from positions because the balance version is rebalanced every few seconds and wrong."
- **CFM UPNL**: Hardcoded to `0` (`coinbase.exchange.ts`). The CFM balance response has `unrealized_pnl` available but it is not used.

## Exchange Metadata Configuration

Key metadata settings (from `exchangeMetadata.ts`):

| Setting                       | Value                                                                            |
| ----------------------------- | -------------------------------------------------------------------------------- |
| `enabled`                     | `true` (coinbase), `false` (coinbasespot)                                        |
| `supportsTestnet`             | `false`                                                                          |
| `requiresCors`                | `true`                                                                           |
| `requiresPassphrase`          | `false`                                                                          |
| `defaultQuote`                | `"USD"`                                                                          |
| `defaultSymbol`               | `"BTC-USD"`                                                                      |
| `tickInterval`                | `1000` ms                                                                        |
| `orderbookTickDelay`          | `60`                                                                             |
| `orderPendingUpdateDelay`     | `1000` ms                                                                        |
| `stopOrderPendingUpdateDelay` | `1000` ms                                                                        |
| `positionModes`               | `[PositionMode.OneWay]`                                                          |
| `marginTypes`                 | `[MarginType.Cross]`                                                             |
| `leverageSliderDisabled`      | `true`                                                                           |
| `hasEmptyPositions`           | `true` (fake empty positions are created for all markets without open positions) |
| `supportsChaser`              | `false`                                                                          |
| `supportsPartialStops`        | `true`                                                                           |
| `supportsInfinityStops`       | `false`                                                                          |
| `autoDownSizeReduceOrders`    | `true` (no server-side reduce-only cap)                                          |
| `tvIntraDayMultipliers`       | `['1', '5', '15', '30', '60', '120', '360']`                                     |
| `tvMaxGetBarFetches`          | `20`                                                                             |
| `latencyDomain`               | `api.international.coinbase.com`                                                 |

## Testnet / Demo

Coinbase has **no testnet support** (`supportsTestnet: false`). Both livenet and testnet URLs in `BASE_URL` and `BASE_WS_URL` point to the same production endpoints (`coinbase.types.ts`):

- REST: `https://api.coinbase.com`
- Public WebSocket: `wss://advanced-trade-ws.coinbase.com`
- Private WebSocket: `wss://advanced-trade-ws-user.coinbase.com`

The testnet option is accepted but has no effect.

## Known Quirks / Implementation Notes

1. **INTX API errors with valid data** (`coinbase.exchange.ts`): The INTX instruments endpoint sometimes returns HTTP errors but still includes valid data in the response body. The code explicitly catches this: if `err.response?.data?.length` is truthy, it uses the error response data instead of propagating the error.

2. **Fake empty positions** (`coinbase.exchange.ts`): For all markets that don't have an open position, the exchange creates fake zero-size Position objects with `leverage: 10`, `side: PositionSide.Long`, `entryPrice: 0`. This is tracked by the `hasEmptyPositions` metadata flag.

3. **UPNL from positions not balance** (`coinbase.exchange.ts`): For INTX, unrealized PnL is calculated from the sum of position-level unrealized PnL rather than from the balance API, because "the balance version is rebalanced every few seconds and wrong."

4. **No testnet**: Both livenet and testnet URLs point to the same production endpoints. `supportsTestnet` is `false`.

5. **Leverage hardcoded** (`coinbase.exchange.ts`): All orders are sent with `leverage: "10"` regardless of user settings. The leverage slider is disabled in the UI. There is a TODO comment: `// TODO. leverage setting?`

6. **USDC/USD substitution**: WebSocket handlers replace `USDC` with `USD` in symbol lookups for order book and candle subscriptions, as Coinbase treats the USDC book as equivalent to USD.

7. **Portfolio detection hack** (`coinbase.exchange.ts`): When a non-INTX portfolio is found, the code attempts to call the CFM balance endpoint to detect if the account supports CFM futures, and overrides the portfolio type to `'CFM'` if successful.

8. **No manual ping**: Unlike most exchanges, Coinbase WebSockets use server-sent heartbeats only. Both `CoinbasePublicWebsocket.ping()` and `CoinbasePrivateWebsocket.ping()` are intentionally empty no-ops.

9. **Tickers via REST polling**: Tickers are fetched via REST polling in the `tick()` loop (every `tickInterval` = 1 second) rather than through WebSocket ticker streams. The `ticker_batch` handler exists in commented-out code but is not active.

10. **Orders via WebSocket only**: Orders are loaded exclusively through the private WebSocket `user` channel snapshot, not via a REST endpoint. The `fetchOrders()` call in `_startPrivate` is commented out.

11. **X-MBX-APIKEY header** (`coinbase.api.ts`): The API client sets an `X-MBX-APIKEY` header (a Binance convention) which is a leftover from when the implementation was based on Binance. It has no effect on Coinbase.

12. **Daily candle formatting** (`coinbase.ws-public.ts`): For daily/weekly/monthly intervals, the code manually accumulates candle data because TradingView only interpolates sub-daily candles. The implementation fetches the last 2 daily candles on first request to initialize the accumulator. There is a TODO: `// TODO handle new daily candles during session`

13. **Commented-out Binance endpoints** (`coinbase.types.ts`): The ENDPOINTS object contains commented-out Binance endpoint paths (LEVERAGE_BRACKET, TICKERS_24H, etc.) with a TODO to remove them.

14. **Trade tape gaps** (`coinbase.ws-public.ts`): Only the first trade in each `market_trades` event is used for candle patching. There is a TODO: `// TODO: if you want actual trade tape without holes fix this...`

15. **Portfolio detection caching** (`coinbase.exchange.ts`): `findSelectedPortfolio()` is called by `start()` before market loading. `_startPrivate()` reuses that value and only probes if no portfolio was selected yet, so saved spot-only accounts do not spend an extra rate-limited portfolio-list request during load.

16. **Order USD/USDC aliases** (`coinbase.ws-private.ts`, `coinbase.exchange.ts`, `coinbase.orders.ts`): private `user` channel and REST open-order rows may deliver ordinary spot orders on either side of Coinbase's `*-USD` / `*-USDC` equivalence while the loaded/displayed market is the other side. Resolve aliases before quarantining an order as a missing market. When both equivalent markets are loaded, prefer the `*-USDC` display symbol so active-market open-order tabs, orderbook overlays, and chart lines match the symbol users commonly select; keep `USDT-USDC` and `EURC-USDC` exact because Coinbase documents them as exceptions to the USD-equivalence rule.

17. **Account validation shape**: `validateAccount()` returns
    `{ error: '', balance }` on success and `{ error }` on failure. Balance is
    best-effort after the authenticated markets probe.

18. **market_trades secondary channel only when not publicDataOnly** (`coinbase.ws-public.ts`): The trade stream for candle patching is only subscribed when `!this.parent.options.publicDataOnly`, meaning public-only mode has degraded sub-5min candle quality.

## Error Handling

### REST Error Pattern

Most private-state REST readers follow a best-effort try/catch pattern:

```typescript
try {
  // API call
} catch (err: any) {
  this.emitAndLogError(err?.response?.data?.message || err?.message);
  return this.store.<previousState>; // fallback to existing store state
}
```

This means most REST failures are **non-fatal**: the tick loop continues, and
the store retains its previous state. Open-order reconciliation is stricter:
`fetchOrders()` rethrows transport/rate-limit failures so generic snapshot
callers cannot commit stale orders as a successfully loaded snapshot.

### Place Order Error Handling (`coinbase.exchange.ts`)

Order placement has a two-level error check:

1. **API success check**: If `data.success === false`, prefers
   `error_response.new_order_failure_reason`, then the preview and legacy
   failure fields.
2. **Order ID check**: Successful responses use
   `success_response.order_id`, with the legacy top-level `order_id` as a
   compatibility fallback. A missing ID is rejected.
3. **Network/HTTP error**: Caught in the outer try/catch, re-thrown after logging.

Both paths throw errors (unlike other REST methods that silently return fallback state), ensuring order failures are propagated to the caller.

### INTX Instruments Error-as-Success (`coinbase.exchange.ts`)

The INTX instruments endpoint uniquely handles errors differently:

```typescript
try {
  const { data: _data } = await this.xhr.get(...);
  data = _data;
} catch (err: any) {
  if (err.response?.data?.length) {
    data = err.response.data;  // use error response body as valid data
  } else {
    throw err;
  }
}
```

### WebSocket Error Handling

- **Public WS** (`coinbase.ws-public.ts`): Messages with `type: "error"` emit an error event and skip further processing.
- **Unknown channels** (`coinbase.ws-public.ts`): Messages for unregistered channels log a `console.error` but do not crash.
- **Private WS**: No explicit error channel handling. Unknown channels are silently ignored (only `user` and `heartbeats` are processed).

### Reconnection (inherited from `BaseWebSocket`)

- **Health check**: `trackLastReceived()` is called on every WebSocket message. If no message arrives for `WS_RECONNECT_THRESHOLD_MS` (20 seconds), triggers reconnection.
- **Reconnect delay**: `WS_MIN_RECONNECTION_DELAY` = 1000 + random(0-2000) ms (jittered)
- **Max reconnect attempts**: After 100 `onClose` or `reconnect` calls, logs an error
- **On reconnect**: Public WS resets orderbook and re-subscribes. Private WS re-authenticates with fresh JWTs.

## Testing / Validation

The runtime directory remains test-free. Deterministic Coinbase regressions
live under `src/__tests__/audit-regressions/` and run through the shared
safe-cex Vitest configuration.

Validation endpoints are tracked via Grafana:

- **Every call**: `ORDER`, `EDIT_ORDER`, `CANCEL_ORDER_BATCH`
- **Periodic**: `LIST_PORTFOLIOS`, `LIST_POSITIONS_INTX`, `LIST_POSITIONS_CFM`, `BALANCE`, `BALANCE_CFM`, `PORTFOLIO_BREAKDOWN`

## Portfolio System

Coinbase has a portfolio-based architecture. The exchange detects the portfolio type at startup via `findSelectedPortfolio()`:

1. Fetches all portfolios via `LIST_PORTFOLIOS`
2. Attempts to get breakdown for each portfolio to find one with API permissions
3. If the portfolio is not INTX type, attempts to detect CFM support by calling `BALANCE_CFM`
4. Sets `selectedPortfolio.type` to one of: `'INTX'`, `'CFM'`, or default (spot-capable)

**Portfolio type determines**:

- **INTX**: Perpetual futures trading via International Exchange. Has its own balance, position, and ticker endpoints.
- **CFM**: Coinbase Financial Markets (dated futures). Uses separate CFM balance and position endpoints.
- **Default (non-INTX)**: Spot trading supported. Balance from portfolio breakdown, positions from spot_positions.

The `getSupportedTypes()` method returns `('spot' | 'cfm' | 'intx')[]`:

- INTX and CFM are mutually exclusive (checked in `fetchBalance`)
- Spot is available alongside CFM but not alongside INTX
- If no supported types are found, emits an error

## Exchange Variants

### `coinbase` (Futures)

The primary variant. Registered as `CoinbaseExchange`. Supports:

- INTX perpetual futures
- CFM dated futures
- Spot trading alongside CFM (via `supportSpot = true`)

### `coinbasespot` (Spot Only)

Registered as `CoinbaseSpotExchange`. Currently **disabled** in metadata (`enabled: false`, `enabledInDev: false`) but kept runnable for previously saved accounts. Unlike the main `coinbase` variant, it is intentionally spot-only:

- `findSelectedPortfolio()` skips `INTX` portfolios and picks the first non-INTX portfolio whose portfolio breakdown is readable.
- `getSupportedTypes()` always returns `['spot']` after portfolio selection.
- It must not call `BALANCE_CFM` to detect futures support; doing so can rate-limit saved spot-only accounts and make market loading fail before ticker search has any symbols.
- Portfolio-list/probe rate limits, 5xx, and network failures must be rethrown as the original transport error. Only expected per-portfolio permission misses should be skipped; otherwise a real 429 gets mislabeled as "No Coinbase spot portfolio with permissions found" and reconnect handling sees the wrong error family.

Both variants are loaded from the same directory (no separate `coinbasespot/` directory).

## OHLCV / Candlestick Details

### Supported Intervals

| Interval | Coinbase Granularity |
| -------- | -------------------- |
| `1m`     | `ONE_MINUTE`         |
| `5m`     | `FIVE_MINUTE`        |
| `15m`    | `FIFTEEN_MINUTE`     |
| `30m`    | `THIRTY_MINUTE`      |
| `1h`     | `ONE_HOUR`           |
| `2h`     | `TWO_HOUR`           |
| `6h`     | `SIX_HOUR`           |
| `1d`     | `ONE_DAY`            |

- Maximum 300 candles per request (`coinbase.exchange.ts`)
- Candles are returned in reverse chronological order from the API and reversed in code (`coinbase.exchange.ts`)
- Daily candles are cached in `this.dailyCandles` for use in real-time daily/weekly/monthly candle formatting
- For intervals < 5 minutes, the WebSocket relies on `market_trades` channel instead of `candles` channel because candle updates are too infrequent
- Start/end timestamps are sent as Unix seconds (divided by 1000 from millisecond dayjs values)

## CCXT Reference Implementation

For implementation inspiration and reference, consult the CCXT library's Coinbase integration:

- **REST API Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/coinbase.ts
  - Note: `coinbaseadvanced.ts` is a thin alias that extends `coinbase.ts`
- **WebSocket Implementation**: https://github.com/ccxt/ccxt/blob/master/ts/src/pro/coinbase.ts
