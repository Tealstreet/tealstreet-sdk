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

Gamma market discovery is paginated with `limit` and `offset`. `clobTokenIds` and `outcomes` are JSON-encoded strings and must be parsed and zipped one-to-one. Malformed or unequal arrays are skipped. Each token becomes one safe-cex market. `orderPriceMinTickSize` controls price precision, `orderMinSize` controls minimum amount, and `negRisk` is retained for signing.

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

Subscription `type` values are lowercase `market` and `user`. Heartbeats are raw `PING` frames every 10 seconds; raw `PONG` marks the connection healthy. Market `price_change` events contain a `price_changes` array, while user `order` and `trade` events arrive as direct event objects.

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
