# Polymarket integration

This directory implements the Polymarket CLOB V2 adapter. It remains development-only (`enabled: false`, `enabledInDev: true`) until a credentialed, small-order production smoke test covers create, observe, cancel, and fill. Preview and production are both production lanes; there is no staging environment and this adapter must not be deployed merely for testing.

## Sources of truth

Use only current official Polymarket documentation and official client source when changing wire contracts:

- [CLOB V2 migration](https://docs.polymarket.com/v2-migration)
- [CLOB authentication](https://docs.polymarket.com/developers/CLOB/authentication)
- [CLOB endpoints](https://docs.polymarket.com/developers/CLOB/endpoints)
- [Gamma markets](https://docs.polymarket.com/developers/gamma-markets-api/get-markets)
- [Data API positions](https://docs.polymarket.com/developers/data-api/get-current-positions-for-a-user)
- [WebSocket overview](https://docs.polymarket.com/developers/CLOB/websocket/wss-overview)
- [`@polymarket/clob-client-v2`](https://github.com/Polymarket/clob-client-v2)

The adapter pins `@polymarket/clob-client-v2` to an exact version. Its `OrderBuilder` is the source of truth for V2 typed-data construction, amount rounding, contract selection, and POLY_1271 wrapping. Do not replace it with remembered V1 fields.

## API surfaces

The integration uses three APIs:

| API | Base URL | Purpose |
| --- | --- | --- |
| CLOB | `https://clob.polymarket.com` | Orders, books, prices, auth, balance |
| Gamma | `https://gamma-api.polymarket.com` | Market metadata and discovery |
| Data | `https://data-api.polymarket.com` | Current user positions |

Gamma market discovery uses `/markets/keyset`, capped at100 rows, with the
opaque response `next_cursor` sent as `after_cursor`. Deep offset pagination
is rejected by the current API; never stop after the silently capped first
legacy page. Missing, repeated, or malformed page data fails closed. `clobTokenIds` and `outcomes` are JSON-encoded strings and must be parsed and zipped one-to-one. Malformed or unequal arrays are skipped. Each token becomes one safe-cex market. `orderPriceMinTickSize` controls price precision, `orderMinSize` controls minimum amount, and `negRisk` is retained for signing.

Collateral and the exchange metadata quote are `pUSD`, not legacy USDC.e. Balance requests use `/balance-allowance` with `asset_type=COLLATERAL` and the account signature type.

Current holdings come from the Data API `/positions` endpoint, paginated by `limit` and `offset`. Do not reconstruct holdings by replaying CLOB trades.

## Authentication

### L1 credential derivation

Credential derivation first reads the CLOB `/time` endpoint, then signs the documented EIP-712 `ClobAuth` payload:

- domain: `ClobAuthDomain`, version `1`, chain `137`
- message fields: wallet `address`, server `timestamp`, `nonce`
- fixed attestation message from the official client

The derivation request sends `POLY_ADDRESS`, `POLY_SIGNATURE`, `POLY_TIMESTAMP`, and `POLY_NONCE`.

### L2 requests

Authenticated CLOB requests sign the exact transmitted body with HMAC-SHA256. The secret and digest use URL-safe base64. The five required headers are:

- `POLY_ADDRESS`: signer wallet address
- `POLY_SIGNATURE`: URL-safe HMAC digest
- `POLY_TIMESTAMP`: seconds
- `POLY_API_KEY`: derived/provided API key
- `POLY_PASSPHRASE`: API passphrase

Keep JSON body normalization in `polymarket.api.ts` and HMAC construction in `polymarket.signing.ts` aligned. Signing one representation and transmitting another produces authentication failures.

Official private paths include `GET /data/orders`, `GET /data/trades`, `GET /data/order/{id}`, `DELETE /order`, and `DELETE /orders`. Cursor pagination ends at `LTE=`; it is independent from Gamma/Data offset pagination.

Although the upstream CLOB exposes
`GET /data/trades`, this adapter does not implement safe-cex's
`fetchHistoricalFills` contract yet; documenting an
endpoint constant is not an implementation.

## CLOB V2 orders

V2 uses domain version `2` and selects the verifying contract per market:

| Market | Contract |
| --- | --- |
| Standard | `0xE111180000d2663C0091e4f400237545B87B996B` |
| Neg-risk | `0xe2222d279d744050d28e00520010520000310F59` |

The V2 order struct has `salt`, `maker`, `signer`, `tokenId`, `makerAmount`, `takerAmount`, `side`, `signatureType`, `timestamp`, `metadata`, and `builder`. Legacy `taker`, `expiration`, `nonce`, and `feeRateBps` are not signed fields. The REST envelope still sends `expiration: "0"` as wire metadata.

Supported signature types are EOA `0`, POLY_PROXY `1`, POLY_GNOSIS_SAFE `2`, and POLY_1271 `3`. The account `funder` is the deposit wallet/maker. For POLY_1271, the official builder produces the required ERC-7739 wrapper; do not treat it as a plain EOA signature.

## WebSockets and lifecycle

The public and private channels are distinct:

- market: `wss://ws-subscriptions-clob.polymarket.com/ws/market`
- user: `wss://ws-subscriptions-clob.polymarket.com/ws/user`

Subscription `type` values are lowercase `market` and `user`. Heartbeats are raw `PING` frames every 10 seconds; raw `PONG` marks the connection healthy. Initial market frames may be arrays of events. `book` requires both full
sides, sorted best-first; `price_change` applies every absolute-size level
change (zero deletes) without throttling away intermediate depth changes.
Reconnect discards prior books until a new snapshot. Quote, trade and tick
events have independent monotonic timestamps; tick changes update market
precision/signing metadata. Market `price_change` events contain a `price_changes` array, while user `order` and `trade` events arrive as direct event objects.

`start()` initializes external public data first, loads Gamma markets and initial tickers, then starts private polling and the user WebSocket only when credentials exist. Public-data-only startup returns before private work. The market WebSocket connects lazily when an order-book listener subscribes. Both sockets are disposed with the exchange.

Private positions, orders, and balance reads fail closed: a rejected REST/Data API request is logged and rethrown so the periodic tick preserves the last known store snapshot instead of replacing it with empty positions/orders or a zero balance. User-channel authentication and trade events refresh positions through `fetchAndSetPositions()`, which commits only a successfully fetched authoritative snapshot; never fetch position data from a WebSocket handler without persisting its result.

Order placement deliberately supports only plain GTC limit orders. Market orders require the official market-order builder because BUY amounts are collateral while SELL amounts are shares and the price is a worst-price bound; the current safe-cex shape does not encode that distinction safely. Conditional, stop-loss, take-profit, trailing-stop, trigger/bracket, close-only, reduce-only, and unsupported time-in-force shapes must reject before signing or POSTing. A limit order requires a finite price within the market tick bounds and a positive amount at or above the Gamma minimum.

## Known product limits

- This is a spot/prediction-market model: there is no leverage or liquidation concept.
- Only plain GTC limit orders are supported; market, stop, bracket, close-only, and alternate time-in-force orders fail closed before any write.
- The adapter is not production-enabled until the credentialed lifecycle gate at the top of this document is completed.
- Avoid live order tests in automated CI. Signing/auth, REST bodies, market parsing, and WebSocket frames have deterministic audit-regression coverage.

## Validation

New integration regressions belong in `src/__tests__/audit-regressions/`, not beside runtime exchange code. Run:

```bash
cd packages/safe-cex
yarn lint
yarn typecheck
yarn test-ci
```

For fast iteration, run the `polymarket.*.test.ts` files in `src/__tests__/audit-regressions/` directly with Vitest, then complete the full package checks before review.

## Client order ID

This venue does not honor `PlaceOrderOpts.clientOrderId`: `placeOrder` / `placeOrders` reject any caller ID before a write (`packages/safe-cex/CLAUDE.md` → "Client order ID contract"). Keep that guard as the first statement of any new placement override.

## Rust engine port

The optional `engine: 'rust'` uses `safe-cex-venue-polymarket` with the same
wallet key and nested `polymarket` options. Supplied L2 credentials must be a
complete triple; otherwise L1 derivation reads fresh server time. Non-EOA
accounts require a funder. Prediction metadata and holding settlement flags
are typed frame sections, hydrated in both worker and main-thread stores.

Only unique owned trade executions emit fills; cumulative order notifications
and repeated settlement statuses never emit again. Maker executions retain
their own order/token/side/price/amount. Failed and stale snapshots preserve
known state. Zero outcome price is valid. Balance PnL aggregates holdings.

A multi-order failure reports every earlier accepted native ID. A matched
placement acknowledgement alone does not prove a GTC order fully filled.
Cancel-and-replace requires a fresh terminal order read covering all known
execution evidence; replacement shares floor to market precision so final
fills plus replacement never exceed the requested total. Ambiguous writes
are never retried. Caller IDs and unsupported shapes reject the whole batch.

Price history uses millisecond caller bounds, converts raw request bounds to
seconds and `fidelity` to interval minutes, and returns sampled points with
OHLC equal and zero volume. Unsupported historical fills/orders/income/public
trades return explicit errors in the Rust engine. Derivatives setters perform
no writes; leverage is always1 and position mode one-way. Development-only
enablement remains pending the credentialed production lifecycle validation
described above; automated fixtures never place live orders.

For large prediction universes, optional `polymarket.marketSlugs` explicitly
scopes discovery to1–100 distinct nonempty Gamma slugs. Queries use repeated
`slug` parameters (bracketed arrays are ignored by Gamma). Omit the option for
the complete keyset walk. A filter never changes token identity or truncates
an outcome pair. Public performance fixtures disclose their selected slugs.


Final amendment reads require explicit `size_matched`. When executions have
been observed, `associate_trades` must cover their native trade IDs; absent
coverage cannot authorize replacement. Each cumulative amount retains its own
association set, including earlier observations that delayed executions may
strengthen. These are overlapping observations, never quantities to add twice.
Successful authoritative removal of a known order fences subsequent old events.

Price history without bounds uses the API-defined `interval=max` horizon; the
server's available horizon depends on fidelity and is not all-history coverage.
An upper-only bound requires `from`, and `from` must be at least 1000ms because
the endpoint treats `startTs=0` as absent. These unsupported requests reject
before transport rather than silently omitting older requested data.


## Rust public discovery resilience

Rust catalogue/ticker reads retry through the shared engine capped exponential
backoff. A malformed typed public row is skipped with a warning while valid
neighbours remain usable. Private account initialization starts once the usable
catalogue is available, without waiting for REST ticker seeds. Account snapshots,
mutation acknowledgements and signing validation retain their stricter rules.
See `docs/plans/RUST_STARTUP_AUDIT.md` and the native startup resilience regressions.

### Rust partial-catalogue account safety

An owned Data API holding whose market the catalogue walk skipped as malformed
(or an unidentifiable skipped row) fails visibly and preserves the prior
snapshot. A holding on a market discovery does not list at all (e.g. a resolved,
`closed` market) is skipped with a one-time log, as TS does, so redeemable
positions cannot block loading. A documented `slug` outside an explicit
`marketSlugs` selection remains intentionally out of scope. Missing `curPrice`
requires an observed positive last price or an actual last-trade observation;
a BBO-only row's default zero is not a price. Explicit native `curPrice: 0`
remains valid. Full rejected-row isolation is tracked separately.

## Rust socket supervision

Both Rust market and user sockets use `engine::socket::Supervisor` for a 10-second connection deadline and capped exponential reconnect delay with jitter. Native raw `PING` remains every10 seconds, and only exact raw `PONG` satisfies its deadline; ordinary market/user data cannot keep an unanswered ping alive. Short open/close flaps retain the failure count. Public subscriptions remain lazy and reconnect requires a fresh book snapshot.
