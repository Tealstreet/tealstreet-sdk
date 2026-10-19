# Ticker ingress construction order

This audit follows ticker objects into the canonical Store and the full native
Store payload. The typed SDK return and native-account REST-command projections
are separate surfaces. A matching numeric snapshot does not establish matching
own-key order, and an ordered Js carrier alone does not prove its producer.

The source census at `90c47a577e` finds 44 constructor/event object candidates in
39 TypeScript files across 26 venue families. Four WOO V3 objects are outside the
constructed production exchange: the two public-parity objects, the V3 public
event, and the retired V3 ticker composer. The live census therefore contains
40 objects in 35 files. WOO's existing `TICKER_RETIREMENT_REVIEW.md` and
`WS_STATE_GATES.md` record that caller boundary.

Construction orders differ between venues and between methods. WOO places index
before mark; Apex and Lighter initialize volume before mark; Binance Spot places
fundingTime before fundingRate. Preserve each actual mapper's native object at
ingress. Reordering the shared Ticker serializer or attaching an order table
would lose these distinctions.

## Constructed binding evidence

| Producer | Result | Evidence scope |
| --- | --- | --- |
| HL core market helper | Master RED, shared-Host GREEN | Actual startup and maintained fetchMarketsHelper; Store root order also matches after #1248. Test commit `08d9fe2ec1` is included in #1264. |
| Phemex REST parser | RED, then GREEN | Actual public-only startup, terminal HTTP, canonical Store, full payload preparation and owned native capture. The only baseline difference is key order. |
| Phemex stream insertion | Positive | Actual connected socket frame inserts a previously absent ticker; compared with the constructed maintained socket handler. |
| WOO legacy startup | Positive | Actual public-only startup, market/ticker HTTP, canonical Store and full prepared native payload. Index precedes mark and openInterest is last. |
| Apex startup | Prior fix in #1248 | Its actual mapper constructs the original Js object; this batch does not repeat that earlier compiled proof. |

`parity/scripts/ticker-ingress-source.cjs` executes the maintained classes and
DefaultStore. `parity/test-support/ticker_ingress.rs` reads native own keys,
String variants and numeric bits before JSON. It checks the same row again after
StorePayloadPreparer and OwnedJsValue capture, without sorting or masking.

Phemex originally grouped numeric reads before appending percentage/fundingTime;
that grouping changed both insertion order and property-read order from the
source literal. The live parser now constructs the literal in source order.
Its stream insertion already matched. WOO's frozen `ee045514f4` trace predates
#1248's removal of output DTO re-serialization; the current constructed legacy
ingress/payload proof is positive. This is not a replay of that complete tape or
a claim about typed REST returns or inactive V3 routes.

## Reached typed handoffs requiring binding probes

These seven crates contain eleven producer profiles. They remain candidates
until a constructed binding proves a mismatch or a positive result; a struct
occurrence or matching field values alone does not close them.

| Crate | Maintained producers | Native handoff |
| --- | --- | --- |
| AsterDEX | transformTicker | account_fetch::fetch_tickers into binding/members::fetchTickers via from_typed |
| Backpack | fetchTickers | adapter_runtime::Data::Tickers into StoreUpdate |
| Binance | Futures, PM and Spot fetchTickers | public_rest::ticker into exchange_classes::fetch_tickers via decoded |
| BingX | formatTickers/fetchTickers | data_reads::fetch_tickers into the startup StoreUpdate |
| Bitunix | fetchTickers | adapter_rest::Data::Tickers into StoreUpdate |
| BloFin | formatTickers | rest_lifecycle::start into StoreUpdate |
| Coinbase | Spot CFM, INTX and V3 tickers | market_position_reads::ReadTicker into binding/lifecycle::ticker_values |

The other source families are Bitget, Bitrue, BreakoutProp, Bybit, Crypto.com,
GRVT, Kraken, KuCoin, Lighter, MEXC, OKX, Orderly, Paradex, Polymarket and WEEX.
Their source constructors remain in the census; no blanket ACCEPT or whole-venue
parity conclusion follows from their shared runtime carriers.
