# Public discovery and private startup audit

Scope: retry catalogue/ticker reads with the shared engine backoff, skip and
warn on a malformed public row, preserve TS blank-number semantics, and prevent
an unavailable ticker feed from indefinitely hiding an otherwise usable account.
Private snapshots, candles, signing validation and writes stay strict.

The common policy lives in `sdk-rs/engine/src/startup.rs`. A venue rebuilds each
request when its retry timer fires: signatures refresh, streamed row buffers
reset, and partial pages cannot be appended twice. Independent catalogue and
ticker sources keep independent retry state. Warnings never include raw rows.

Initial inventory (paths are under `sdk-rs/venues/<venue>/src/`; line numbers
refer to the pre-change landing revision):

| Venue | Catalogue/ticker retry before change | Account dependency / malformed public rows |
| --- | --- | --- |
| OKX | None | Ticker read blocks private startup; one malformed row fails the list (`lib.rs:738,841,995`). |
| BloFin | None | Ticker read blocks private startup; one malformed row fails the list (`lib.rs:505,617`). |
| Binance family | Catalogue none; ticker errors finish with an empty list | Account waits for all ticker reads to settle (`lib.rs:709`); malformed rows are silently skipped (`lib.rs:1290`). |
| Bybit family / HyroTrader | Exponential retry, venue-local state | Account requires every ticker category (`lib.rs:939`, `account.rs:1089`); malformed public row fails its source. |
| Backpack | Independent exponential retry | Account waits for daily and mark reads (`lib.rs:211`); one bad row retries whole source. |
| BingX | Independent exponential retry | Account waits for daily then premium (`lib.rs:639`); one bad row fails source. |
| Bitget family | None | Ticker success starts private (`lib.rs:677`); one malformed row fails list. |
| Crypto.com | None | Ticker success starts private (`lib.rs:534`); one malformed row fails list. |
| KuCoin futures / spot | Futures startup reads none | Futures ticker success launches (`lib.rs:980`); one malformed row fails list; spot is a feature-gated public helper. |
| Kraken | Fixed poll | Ticker success starts account (`lib.rs:696`); malformed row fails snapshot. |
| Lighter | First catalogue failure falls back, second stops | Account starts after markets (`lib.rs:1032`); malformed row fails catalogue; tickers retry by poll. |
| MEXC | Three linear startup retries | Ticker success starts account (`lib.rs:587`); malformed row fails whole list. |
| Orderly | Exponential retry, shared single timer | Ticker success starts account (`lib.rs:817`); malformed row fails list. |
| Paradex | Constant 1s | Account requires tickers (`lib.rs:127`); malformed row fails list. |
| Phemex | Constant 2s | Ticker success starts account (`lib.rs:574`); malformed row fails list. |
| Polymarket | Three linear attempts | All midpoint batches precede private (`lib.rs:245,745`); malformed Gamma row fails page. |
| TxFlow | Constant 5s | Every queued ticker precedes account (`lib.rs:193,230`); malformed metadata fails list. |
| WEEX | Constant 2s | Both ticker sources precede account (`lib.rs:727`); rows already skip but lack warning. |
| WOO | Three linear attempts | Ticker success starts account (`lib.rs:901`); malformed row fails source. |
| ApeX Omni | Constant 1s | Account starts after catalogue (`public.rs:239`); malformed product fails catalogue. |
| AsterDEX | Public request retry layer | Daily ticker success starts account (`lib.rs:270`); malformed row fails source. |
| Bitrue | Immediate retries then constant 2s | Account already starts at catalogue (`lib.rs:822`); malformed contract fails source. |
| BreakoutProp | Three delayed attempts | Authenticated public discovery uses account identity; malformed streamed row fails its operation. |
| Coinbase INTX | Constant 1s public retry | Private discovery starts first for credentialed accounts; malformed public row fails list. |
| GRVT | Five immediate retries, then catalogue retry 5s | Catalogue starts account; malformed product fails catalogue (`lib.rs:721,825`). |
| Hyperliquid | Two metadata retries then stop | Combined catalogue/context reads precede account (`lib.rs:201`); preserving paired row indexes requires care when skipping. |

Numeric audit: `engine::num::parse` already maps blank to zero. KuCoin's
`Number` explicitly accepts blank/null as zero, and Bitget's optional numeric
wrapper defaults blanks through `get()`. Bybit intentionally has separate
`float` (TS `parseFloat`, including NaN) and `float_zero` paths; do not turn all
blank fields into zero globally. Kraken and Paradex ticker mappers explicitly
use TS zero fallbacks and need their public parser to accept blanks. Other
strict numeric wrappers require field-level TS verification before broadening.

Regression coverage must prove an initial read failure schedules bounded
backoff and eventually recovers, a malformed public row preserves valid peers
with a warning, private startup proceeds when the catalogue is usable but
tickers fail or remain pending, and private/order data remains strict.

Implementation progress for Binance, Bybit, Backpack, BingX, Bitget,
Crypto.com, KuCoin futures/spot, MEXC, Orderly, Phemex, WEEX and WOO:

- Catalogue/ticker retry timers now use the shared capped exponential backoff;
  requests are rebuilt with fresh signatures and empty streamed buffers.
- Public malformed rows are warned and isolated. An entirely unusable catalogue
  retries rather than committing an empty authoritative catalogue. Orderly
  duplicate identities are isolated before insertion.
- Private discovery/snapshots start after catalogue readiness. Bybit retains
  the authenticated account-info prerequisite but no longer requires tickers.
  KuCoin futures REST snapshots start before ticker replies, token acquisition
  or private WebSocket welcome; welcome does not duplicate in-flight snapshots.
- MEXC and WEEX public numeric parsing follows their TS `toNumber` blank/null
  defaults, including last-price fallback. WEEX checks both book/daily arrival
  orders and tracks independently available touch prices. The strict private
  financial decoders are unchanged.
- Public socket scheduling is otherwise retained; moving independent private
  startup does not require changing healthy public subscription timing.
- Each family has scripted `tests/startup_resilience.rs` regressions; KuCoin
  Spot adds `tests/startup_spot.rs` under the `spot` feature. The fixture drivers
  retain the early private requests rather than discarding them while awaiting
  public ticker responses.

Public recovery account boundary (owned subset): starting private reads from a
usable partial catalogue must not turn an unmappable nonzero native holding
into an authoritative empty snapshot. WOO, Orderly, BingX, Backpack, MEXC,
Phemex and Crypto.com now reject that snapshot and retain prior positions;
KuCoin also rejects unknown owned lots before using contract multipliers.
Binance COIN-M rejects missing owned contract metadata rather than assuming a
one-dollar contract. Each boundary has a scripted regression. KuCoin and
Binance inverse live positions have the same sizing guard; Phemex rejects an
unknown owned live row before granting snapshot authority or consuming its
sequence. This is a safety boundary, not the future tolerant private peer merge.

Bybit maps native positions independently of catalogue membership (including
native inverse dollar quantities and private mark fallback); Binance linear
preserves native quantities/notional without requiring contract metadata.
Bitget's position mapper already rejects missing markets; WEEX already marks
its snapshot invalid when `StagedPosition::new` cannot map a position. Their
snapshot error paths retain previous positions, so no additional skip guard is
needed. Incremental private frames that ignore unknown symbols without granting
snapshot authority do not clear the previous snapshot.

Early public sockets now retain only fields actually supplied by valid live
frames across slower REST discovery/retries. Shared `startup::LiveTickers`
overlays those fields and appends known live peers omitted by REST, preserving
REST values for fields the stream did not supply. Backpack reuses its one early
public socket; BingX separates temporary REST row indexes from live store
indexes. Regression fixtures cover intervening BBO/trade updates, reordered
seeds, repeated retries, and no duplicate Backpack connection.

The Rust SDK waits for catalogue plus the private balance/orders/positions for
credentialed accounts, while public-only clients still require real tickers.
Shared public-feed compatibility and seed readiness remain prerequisites.
Fake-Wasm tests use real host frame application, worker frame forwarding and a
shared-feed handshake; public discovery warnings do not invalidate funded
account validation, while private authentication errors still do.

The final owned-working-order boundary audit covers the same 12 families.
WOO regular/protective children, Orderly lineages, Backpack, BingX and
Crypto.com reject a snapshot that would silently discard a working supported
product whose market is absent. Terminal rows and explicitly unsupported spot
products retain their previous filtering behavior. Phemex validates unknown
working linear orders before granting live snapshot authority or consuming its
sequence. KuCoin rejects unknown working futures sizing in REST and live input
rather than treating a lot as one unit. All retain prior whole order snapshots
and emit a visible warning; tolerant valid-peer merging remains the next phase.
MEXC and Bitget already reject failed order mapping, while Binance, Bybit and
WEEX preserve native unknown order rows without catalogue-dependent quantity
conversion. No additional skip guard is needed for those paths.

| Family | Working-order mapping when public market metadata is missing |
| --- | --- |
| Binance | Native placement quantities retained; inverse order quantities are native lots, unlike base-unit positions. |
| Bybit | Native order rows retained; invalid semantic mapping rejects its snapshot. |
| Bitget | Existing mapper returns an error and rejects the snapshot. |
| BingX | New working-row guard rejects its snapshot; terminal rows remain filtered. |
| Backpack | New perpetual working-row guard rejects its snapshot; terminal/spot filtering stays unchanged. |
| Crypto.com | New perpetual working-row guard rejects its snapshot; terminal/spot filtering stays unchanged. |
| KuCoin | New working-row guard rejects guessed lot sizing; both initial normal/stop books must complete before `loaded.orders`. Unknown-symbol live executions warn and preserve state before sizing/fills. |
| MEXC | Existing failed mapper marks its snapshot invalid. |
| Orderly | New working-lineage guard rejects its snapshot; explicit final lineage status and spot exclusions stay unchanged. |
| Phemex | New working linear-row guard rejects live snapshot authority before its sequence is consumed. |
| WEEX | Native order converters preserve rows without catalogue-based sizing. |
| WOO | New regular/protective leaf guard rejects its snapshot; a FILLED parent does not prove a NEW protection child is terminal. |
