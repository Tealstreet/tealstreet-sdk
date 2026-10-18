# Recorded public market responses

Captured 2026-10-18, without credentials. These bytes are replayed to the maintained
TypeScript exchange and the compiled native addon by `native-public-markets.test.mjs`.
The original live-diff reports retained response hashes but no bodies; these fresh
recordings reproduce their missing-market-field findings.

| Fixture | Public GET | SHA-256 |
| --- | --- | --- |
| `backpack-markets.json` | https://api.backpack.exchange/api/v1/markets | `80def488230e5cea02e0a712c8b20adb900984f307d5a5f27ffdd05962667251` |
| `cryptocom-markets.json` | https://api.crypto.com/exchange/v1/public/get-instruments | `93e8e3e27d1d895938f2a3a1ece58b5081abf8929e4605169e3c205d154846f0` |

WOO's `woo-futures.json` is the unchanged 2026-10-18 HTTP 200 body for
`GET https://api.woox.io/v1/public/futures` (SHA-256
`83afa9db3fabc2b381e750d390b8eae2bfd438a314a3edc1364bded57d688107`).
It contains 222 rows; exactly one, `PERP_SOLV_USDT`, has `index_price:null`.
`woo-markets.json` is the companion `/v1/public/info` body (SHA-256
`127af0d83577e85fc022eab1dc62d0840a72899761e084f36725b6e576fc211a`).
The production-addon regression admits only that market from the companion body,
then serves all 222 futures rows byte-exact. This bounds the null-decoder proof
independently of the separately tracked whole-Store processing cost; it does not
claim full-venue timing or 222-ticker parity. Both maintained TS and native run
real refreshMarkets/fetchTickers, and the sole returned ticker is deep-equal.

`paradex-markets.json` is the exact public GET
https://api.prod.paradex.trade/v1/markets HTTP 200 body from the live-diff
`paradex-post-1044.json` report, captured 2026-10-18. SHA-256:
`a53b4fb1391e5e028c2801dcf3b5faebaf124b22bdf47f572325e7a48505a33a`.
The test compares all 66 returned markets, including their own `fees` objects,
using the real TS constructor and production native account binding. The mapper
already emits fees; the canonical shared Market runtime model preserves them.
The live body defaults both fee values to zero; distinct negative maker and
positive taker values have separate native shared-model and numeric-ingress
coverage. No live credentials or upstream request is needed for this regression.

`binance-delivery-prefix.json.gz` contains the exact first 12 WebSocket frames
and all six original HTTP responses from the 2026-10-18 Binance public capture.
Its `sourceBundleSha256` pins the complete 570-frame capture:
`df9a504272da029f0c871cb9c44bb2777ff092c34ea966a68d3c126269218d5f`.
The JSON envelope stores HTTP response bytes in base64; frame payloads and
timestamps are unchanged. `native-account-delivery-order.test.mjs` reconstructs
the replay bundle and drives the real TS worker and production addon four times,
with two fresh accounts per side each time. It requires parsed/transport frame
receipts, zero canonical Promise jobs, exact ticker/book/candle values, and
identical combined native output across all eight accounts, including every
Store field, with the combined channel order matching the maintained TS worker. Binance's snapshot listener must start through `start_eager` so its
HTTP continuation belongs to that job system before the next clock advance.
`binance-snapshot-ordering-evidence.json` records the compiled RED/GREEN and
source enqueue/clock observations. General classification of raw runtime tasks
remains follow-up work; these receipts do not certify all raw tasks. Existing
cross-language metadata differences remain separate. The test claims neither
full Store parity nor a new 60-second capture. Run with
`LIVE_DIFF_ADDON=/absolute/addon.node` to check an immutable compiled artifact.
