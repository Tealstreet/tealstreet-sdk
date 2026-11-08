# BingX Spot (`bingxspot`)

This is a separate native venue in `src/spot`, compiled in the existing `bingx`
registry/WASM family. It never uses swap market, order, balance or socket routes.
The TypeScript fixture oracle lives under
`packages/safe-cex/src/__tests__/reference/bingxspot`; production factories do not
load it.

## Admission and remaining product work

**Native and fixture-only until live-key smoke passes.** `bingxspot` is absent
from both SDK `BETA_VENUES` and the WASM beta table. `assertBetaVenue` refuses it
in every environment before I/O. The native/hosted registry also refuses it
through the shared unsupported-venue path. Production, dev and preview metadata
flags are all disabled. Native fixture tests construct the module directly.
Admission is a separate follow-up after smoke, not part of this change.

- **NX:** #9111 has merged the account-picker integration for the new public
  `ExchangeName` member `bingxspot`. `apps/web` is outside this repo.
- **NEEDS-SMOKE:** the lead must verify the AWS-era transport dependency with
  live keys, exercise both compressed Spot streams and REST trading, and then
  explicitly admit the venue. This implementation does not claim that the AWS
  migration or live trading has been validated.
- WASM size caps remain owned by the lead and are not edited here.

## Official protocol

The current [BingX Spot v3 docs](https://bingx-api.github.io/docs-v3/#/en/Spot/Introduction)
are authoritative for the routes and payloads. The official
[Spot market reference](https://github.com/BingX-API/api-ai-skills/blob/main/skills/spot-market/api-reference.md)
warns that its legacy market sections need a mapping review: current v3 quote
routes below supersede the old ticker/kline examples.

| Purpose | Method and path |
| --- | --- |
| Symbols and precision | `GET /openApi/spot/v1/common/symbols` |
| Balances | `GET /openApi/spot/v1/account/balance` |
| Open orders | `GET /openApi/spot/v1/trade/openOrders` |
| Ordinary order | `POST /openApi/spot/v1/trade/order` |
| Single cancel | `POST /openApi/spot/v1/trade/cancel` |
| Cancel all on one symbol | `POST /openApi/spot/v1/trade/cancelOpenOrders` |
| Client-ID lookup | `GET /openApi/spot/v1/trade/query` |
| Historical executions | `GET /openApi/spot/v1/trade/myTrades` |
| OHLCV | `GET /openApi/spot/v2/quote/klines` |
| Tickers | `GET /openApi/spot/v2/quote/ticker` |
| REST depth | `GET /openApi/spot/v1/market/depth` |
| Listen key create/renew | `POST` / `PUT /openApi/user/auth/userDataStream` |

REST uses `https://open-api.bingx.com`. Both Spot sockets use
`wss://open-api-ws.bingx.com/market`; the private URL adds `?listenKey=...`.
[Spot public streams](https://github.com/BingX-API/api-ai-skills/blob/main/skills/spot-ws-market/api-reference.md)
use `SYMBOL@ticker`, full `SYMBOL@depth50` snapshots and
`SYMBOL@kline_1min` (REST uses `1m`).
[Private streams](https://github.com/BingX-API/api-ai-skills/blob/main/skills/spot-ws-account/api-reference.md)
require explicit `spot.executionReport` and `ACCOUNT_UPDATE` subscriptions.
The shared host inflates GZIP/deflate frames before the native decoder. Replies
to native `Ping` are `Pong`. Listen keys renew every 30 minutes; engine socket
supervision owns reconnect backoff and epoch retirement. Old session responses
cannot replace a newer key.

## Permissions and transport

Use a BingX API key with **Read** permission for balances, orders and fills, and
**Spot Trading** permission for placement/cancellation. A swap-only trading grant
is insufficient. Enable no withdrawal permission. Hosted signing uses the
existing BingX HMAC-SHA256 provider flow: `remoteSigning.enabled=true`,
`payloadSupported=false`, with `X-BX-APIKEY` protected. Optional read credentials
serve reads; writes, listen keys and post-unknown order reads use the primary.
See the official [trade](https://github.com/BingX-API/api-ai-skills/blob/main/skills/spot-trade/api-reference.md),
[account](https://github.com/BingX-API/api-ai-skills/blob/main/skills/spot-account/api-reference.md)
and [authentication](https://github.com/BingX-API/api-ai-skills/blob/main/skills/references/authentication.md)
references.

Browser REST requires an allowed CORS route or a server transport. Private
routing prepares the same explicit tunnel policy as BingX; headless callers
with direct egress can opt out with `tealstreetTunnel:false`. API IP allowlists
must match the actual selected primary/read egress. Before admission, verify
AWS egress to `open-api.bingx.com:443` and `open-api-ws.bingx.com:443`, TLS,
listen-key renewal, compressed message delivery and API IP permissions. No
infrastructure, production host, database or release change is included.

## Mapping and bounded behavior

Market IDs use `BASE-QUOTE`; SDK symbols concatenate base and quote. Tick/step
sizes are native increments, leverage is one, and `isSpot=true`. Positions are
an authoritative empty set. Free/locked currency balances are preserved; USD
aggregation requires valid USD/USDT/USDC quotes. Unpriced nonzero holdings retain
the prior aggregate rather than becoming zero. New balance reads and live
mutations fence older balance responses; a late ticker seed preserves live data.

Only ordinary LIMIT/MARKET placement is supported. Amounts are in base units.
The official v3 order endpoint requires `quoteOrderQty` for market BUY, so a
market buy requires a caller price for quote conversion (`amount * price`). The
price is a conversion reference, not an execution-price guarantee. Market SELL
uses `quantity`; LIMIT uses quantity, price and GTC/IOC/FOK/PostOnly. Unsupported
brackets, triggers, reduce-only, close and venue-specific controls fail before
any write. Caller IDs are preserved verbatim and must match `[A-Za-z0-9_]{1,40}`;
placement uses `newClientOrderId`, lookup uses `clientOrderID`. Native order and
trade IDs never pass through floating point.

Batches are completely preflighted before serial dispatch. Released HTTP
408/429/5xx, transport failures, malformed or contradictory acknowledgements and
unlisted errors remain **Unknown**. There is no write resend or fallback-domain
retry. Fresh primary-key open-order reads reconcile uncertain outcomes through
engine read covers. Symbol cancellation uses the native endpoint, including
orders not yet present in the store; the symbol is always explicit. Unsupported amendments and derivative settings fail locally.

The venue documents seven-day trade-history retention, an ascending-time result
and conflicting maximum page sizes (500/1000). This adapter requests at most 500
rows. Results retain caller time bounds even if the venue defaults an invalid
range; such a response is marked incomplete. A capped window carries an explicit incomplete marker; an old requested
range carries a retention marker. It never claims a full capped page proves the
entire window. The 2,000-row open-orders ceiling similarly cannot prove account
coverage and leaves cached orders intact. Client-ID absence is nondefinitive
because the venue documents a two-hour lookup range. Account identity, income
history and historical orders are not advertised.

Fixtures and source provenance are recorded in `tests/fixtures/spot/README.md`.
