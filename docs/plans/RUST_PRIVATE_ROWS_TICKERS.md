# Private row isolation and ticker selection audit

Status: **future-phase plan; implementation and regression gates pending**.
Read-only source audit of every Rust family and spot submodule. No runtime
changes are included in this document. The app payload companion is
`RUST_APP_STOP_PAYLOADS.md`. Public-startup work is checkpointed separately on
`feat/rust-public-startup-resilience`; broker/cancel PR #1347 is merged.

Prioritize money/state/caller impact. An aggregate all-market feed need not be
rewritten to imitate per-symbol TS lifecycle. Conversely a protocol cap that
leaves the selected market stale is a real missing-data defect. Existing native
sequence, timestamp, revision, generation, closure and fill fences remain in force.

## Concrete findings and public/private boundary

- **BreakoutProp authenticated discovery:** migrate Instruments/Details/Quotes
  to shared public-read backoff, isolate malformed catalogue peers, and test
  authenticated/public-only startup before declaring every family complete.
  This is explicitly deferred from the public PR to this phase because these
  reads are coupled to native account identity and portfolio processing.
- **Paradex coordination update (2026-10-20):** `feat/rust-paradex-emitter`
  owns Paradex trading/errors/SBE and TinyEmitter isolation. This lane owns
  already-implemented public blank ticker/startup. Narrow REST position blank
  leverage/liquidation tolerance is being folded into that public-startup PR to
  unblock valid startup; generic private-row isolation stays in the follow-up.
  Only those optional fields may use TS-compatible zero defaults; keep private
  sizing/risk and command numerics strict. Integrate landing first.
- **Polymarket partial catalogue:** an unmappable previously held token must
  retain its existing holding instead of disappearing on snapshot replacement.
  Distinguish intentional marketSlugs scope from rejected selected metadata;
  never guess prediction/token identities for an unknown asset.

- **MEXC coordination update (2026-10-20):** `feat/rust-venue-highs-a` owns
  public ticker interest, row-local order snapshots, stop snapping and single
  pacing. This cross-venue lane owns markets/tickers startup plus private
  position-mode and plan-list bootstrap failure/recovery/readiness in
  `mexc/src/account.rs`. Integrate landing first; verify the other lane's ticker
  and order-row work rather than duplicate it. Native authentication and
  envelope/paging completeness remain strict. Transient private bootstrap
  failures must not be exposed as invalid credentials that dispose the SDK.
- **MEXC active ticker loss:** `mexc/src/lib.rs::on_open` subscribes only the
  first100 catalogue entries and `on_command` ignores SetTickerSymbols. The TS
  `mexc.ws-public.ts::reconcileTickerSubscriptions` first filters active symbols,
  then takes100, and reconciles changes. Selecting a later market in Rust leaves
  it without live ticker updates; this is a concrete production fix.
- **WOO early-private omission:** `woo/src/account.rs::account_item` Positions
  returns for each row lacking `store.ticker(symbol)`, and later replaces the
  position list. Public-startup moves account start ahead of ticker readiness,
  so a pending/failed ticker seed can make actual exposure disappear. A zero
  placeholder does not solve valuation: the mapper derives notional and PnL
  from mark, so mark0 fabricates losses. Its ambiguous-trigger order mapper also
  requires a ticker and can omit known protection. Resolve this boundary before
  landing public-startup; retain/retry identified unmappable rows or use a proven
  native risk/reference source, without inventing a price.
- **Snapshot omission despite a syntactically valid row** occurs in several
  families when market lookup or order-kind mapping returns None. Simply making
  every typed parse failure `continue` would extend that deletion defect to all
  malformed rows. Capture identity before full row parsing and preserve it.
- Many current `invalid`/`failed` flags reject the whole list, safely retaining
  the previous snapshot. Those are freshness/isolation work, not evidence of an
  existing silent deletion. Authentication/status/envelope/page-continuation
  failures must remain authoritative failures even after row isolation.

## Ticker command matrix

Sources below are relative to `sdk-rs/venues/`. All names in a cell share the
same handler. Base TS fallback (`exchanges/base.ts::getTickerSymbolsForSubscription`)
uses all available symbols when selection is empty or has no known match;
venue-specific TS WEEX/MEXC instead use zero topics for nonempty unknown-only
selection. Do not impose one fallback blindly on both contracts.

| Registered names | SetTickerSymbols and actual feeds | Production assessment / regression target |
| --- | --- | --- |
| `binance`, `binancepm`, `binanceinverse`, `binancespot` | Explicit no-op; aggregate ticker/mark feeds in `binance/src/lib.rs`. | All-market feed is equivalent for selected freshness; no lifecycle churn needed absent measured limit. Keep selection from disabling account-required valuation. |
| `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader` | Falls through handler; public open queues tickers.SYMBOL for every owned catalogue market, paced ten topics/frame. | TS supports selection for ordinary/inverse/spot. Current Rust still receives selected markets when all subscriptions fit. Add selection if required by limits or explicit caller contract; preserve category sockets, pacing and option/inverse eligibility. |
| `bitget`, `bitgetusdc`, `bitgetinverse` | No selector handler; all ticker topics spread across3 public sockets (`lib.rs::on_open`). | Selected data covered; resource-only difference unless a concrete venue limit is exceeded. Do not disturb book/candle roles. |
| `blofin` | Explicit no-op; all-market tickers/funding topics (`lib.rs::subscribe_all`). | Selected data covered; no measured missing-market defect. |
| `okx`, `okxeu`, `okxdemo`, `okxeudemo` | Explicit no-op; all market ticker/mark/index/OI/funding subscriptions (`lib.rs::subscribe`). | Selected data covered. Shared underlying index subscriptions must be refcounted if narrowing later. |
| `backpack` | No selector branch; every market gets ticker/trade/bookTicker subscriptions on public open. | All selected prices covered; narrowing is not a correctness repair by itself. |
| `bingx` | No selector branch; market.contracts aggregate plus requested books/candles. | Aggregate feed covers selection. |
| `bitrue` | Explicit no-op; ticker channels for all markets (`lib.rs::on_open`). | Selected data covered subject to protocol limits; no limit failure established here. |
| `cryptocom` | Explicit no-op; market_channels prioritizes all tickers then mark/index/funding under350 total slots. | TS has the same350 cap and priority, with skip diagnostics. No active-selector TS contract found. Record limitation; changing cap or promising all valuations requires separate evidence. |
| `kucoin` | Explicit no-op; snapshot topics for every market, sharded and chunked100; detail feeds for books/candles. | Selected data covered by snapshots. Retain public/private bullet roles and sharding. |
| `kucoinspot` | Explicit no-op; /market/ticker:all aggregate. | Equivalent selected freshness. |
| `phemex` | Explicit no-op; perp_market24h_pack_p aggregate. | Equivalent selected freshness. |
| `mexc` | Selector ignored; only first100 catalogue markets get sub.ticker. | **Fix:** filter active selection before100 cap, reconcile additions/removals, retain selection across prestart and reconnect. Unknown-only selection follows documented MEXC TS empty result. |
| `weex` | Implemented: active_symbols updated, reconcile_topics sends ticker deltas;90 ticker /100 total channel budget after book/candle reservations. | Pin selected market beyond default90, switching, unsubscribe, prestart/reconnect and channel-budget interactions. Unknown-only produces no tickers, matching TS. |
| `kraken` | Implemented: unsubscribe old selected set, replace symbols, subscribe new; empty/unknown-only falls back all. | Existing semantic parity; add lifecycle tests only if gaps found. |
| `krakenspot` | Implemented: unsubscribe/subscribe, chunks50; empty means all, nonempty unknown-only means none. | TS Spot does not expose the same selector path; assess whether shared Base fallback is intended before changing. Do not call this a proven native bug merely because futures differs. |
| `woo` | Explicit no-op; tickers/bbos/markprices aggregate. | Equivalent selected freshness once public socket is live; early-private ticker dependency is separate. |
| `tealstreet-dex`, `woofi`, `kodiakfi`, `mode`, `salsadex`, `what` | Explicit no-op; Orderly tickers/bbos/markprices aggregate. | Equivalent selected freshness. Coordinate Orderly race lane before touching private state. |
| `hyperliquid`, `hyperliquidspot` | No selector branch; allMids core+DEX feeds, requested books/candles/contexts. | Aggregate covers all midpoint prices; retain required per-market enrichment separately. |
| `grvt` | Explicit no-op; all-market v1.ticker.d subscriptions paced batches20. | Selected prices covered; preserve sequence/first-ACK state if narrowing. |
| `lighter`, `lighter-robinhood` | Explicit no-op; market_stats/all plus ticker/ID for every market. | Aggregate/per-market combination covers selected prices; do not drop account valuation feeds through a cosmetic filter. |
| `apexomni` | Explicit no-op; instrumentInfo.all plus book/candle-derived detail subscriptions. | Aggregate covers selection. |
| `asterdex` | Explicit no-op; !ticker@arr + !markPrice@arr@1s aggregate. | Equivalent selected freshness. |
| `paradex` | Explicit no-op; public summary/read pipeline covers all available products. | Selection is not currently used by TS either; preserve complete summary. |
| `coinbase`, `coinbasespot` | Explicit no-op; product ticker fields refreshed by REST public polling; requested book/candle/trade feeds update subscribed products. | All-catalogue polling is current source. Decide streaming requirements from freshness measurements, not selector implementation style. |
| `polymarket` | Explicit no-op; midpoint REST seed, market WS asset IDs follow book interests. | Matches existing TS subscribed-assets model. Watchlist-only live quotes without a book are a capability question; adding selector-driven assets needs separate interests/refcounts so closing a book cannot unsubscribe a watched token. |
| `breakoutprop` | Explicit no-op; quote polling and portfolio path. | No equivalent dynamic selector TS contract found. Preserve account quote valuation rather than filtering solely to UI watchlist. |
| `txflow` | Falls through; subscribes ticker for all known assets on API socket. | Selected prices covered; preserve metadata asset IDs and other socket subscriptions. |

`bitunix` is TS-only and outside the Rust registry.

## Private snapshot row matrix

Paths are under `sdk-rs/venues/<venue>/src/`; named functions are stable review
anchors, and listed endpoints are concrete examples rather than an exhaustive
routing specification. Existing code is assessed before future row isolation.

| Registered names | Concrete account path | Current isolation / identity to preserve |
| --- | --- | --- |
| Binance family | `binance/account.rs::on_account_item(PositionRisk)`; `on_account_response(OpenOrders/OpenAlgoOrders)`; openOrders/openAlgoOrders + positionRisk. | Typed parse failure marks joined snapshot failed. Unknown native order type returns from callback and can disappear from successful replacement. Preserve native orderId or PM client/strategy ID, family, symbol/positionSide and inverse quantity metadata. Coordinate Binance audit lane. |
| Bybit family / HyroTrader | `bybit/account.rs::on_account_item`, /v5/order/realtime and /v5/position/list. | Bad parse/semantics flags whole list invalid. Out-of-product rows intentionally ignored. Distinguish this eligibility filter from rejected owned rows; preserve orderId, attached children and symbol/positionIdx identity. |
| Backpack | `backpack/account.rs::account_item`, /api/v1/orders and /api/v1/position. | Parse failure aborts snapshot; successfully parsed unknown-symbol rows are omitted then list replaced. Preserve order ID and symbol position key, trigger-repair provenance. |
| Bitget family | `bitget/account.rs::account_item` Positions and Orders(snapshot). | Parser/map errors fail whole snapshot. Row-level recovery must preserve normal/plan/UTA source identity, trailing/strategy metadata and child IDs. Distinguish explicit zero positions from failed mapping. |
| BloFin | `blofin/account.rs::account_item` + `commit_positions`/final set_orders; /api/v1/account/positions and normal/TPSL/trigger pending lists. | Typed parse failure fails list; map_position None and map_orders empty silently omit rows. Preserve orderId/tpslId/algoId, canonical child IDs, sources map and position identity. Pagination cursor extraction must remain valid even if economics rejected. |
| OKX variants | `okx/account.rs::account_item` Positions/Orders + final set_orders/commit_positions; /api/v5/account/positions and normal/algo pending lists. | Known-market invalid row fails list; unknown-market row can be omitted. Preserve ordId/algoId, derived leg IDs, product scope, margin/leverage metadata and position side. Never mark lost mapped row terminal by absence. |
| BingX | `bingx/account.rs::on_item` Positions/Orders; /openApi/swap/v2/user/positions and /trade/openOrders. | Parse errors fail list; unknown-symbol positions return before staging. Preserve native order identity, hedge side, reduce/close and execution revision state. |
| Bitrue | `bitrue/account.rs::snapshot_item` nested positions and `account_item(Orders)`; final Snapshot/Orders commits. | Nested coin/group invalidity and order mapping errors reject list; unknown contract groups are skipped. Preserve nested contract+side identity, order tree, child records and fill quote metadata. Account/coin mismatch remains fatal. |
| Crypto.com | `cryptocom/account.rs::account_item`; private/get-positions, private/get-open-orders, private/advanced/get-open-orders. | Parse errors invalidate snapshot; unknown instrument rows are skipped and replacement can remove prior IDs. Preserve order_id, instrument position key and advanced-family/terminal fencing. |
| KuCoin futures | `kucoin/account.rs::account_item` and response; /api/v1/positions and regular/stop order reads. | Any parse/order-map failure retries whole snapshot, then retains old state. Preserve native order IDs, parent/leg IDs and native lots/settings. Duplicate page/identity failures remain envelope-level completeness failures. |
| KuCoin Spot | `kucoin/spot/mod.rs::on_response(Pending::Orders)` HF page loop. | Malformed/invalid row aborts read; old orders survive. Preserve order ID + symbol/native size on isolated rejection. Account positions intentionally empty. |
| Kraken futures | `kraken/account.rs::account_item`; /derivatives/api/v3/openorders and /openpositions. | Bad known row fails list; unknown market rows silently skipped. Preserve order_id, symbol/side, trigger/provenance and current position settings. |
| Kraken Spot | `kraken/spot/account.rs` REST open-orders object iterator and execution-frame snapshot. | Typed map iteration or semantic invalidity rejects full list/frame. Object-map key gives identity even if row body fails: retain that exact key/provenance. No synthetic positions. |
| WOO | `woo/account.rs::account_item`, /v3/positions, /v1/orders, /v3/algo/orders. | Bad typed rows fail snapshot; missing ticker and some unmappable orders return without staging. **Critical early-private issue above.** Preserve native normal/algo root and descendants; position symbol+side and classification metadata. |
| Orderly aliases | `orderly/account.rs::account_item`, position and normal/algo paging. | Map errors flag whole list; position helper returns None for unavailable markets. Preserve broker-suffixed native symbol, mode/side, algo lineage/proofs/stamps. Coordinate race owner; never weaken duplicate/cursor/root completeness checks. |
| Hyperliquid / spot | `hyperliquid/account.rs::account_item(Orders)`, map_order, clearinghouse-state asset-position and spot balance paths. | Typed order error invalidates list; unknown coin -> map_order None. Position iteration can skip unregistered coins after replacing scoped positions. Preserve oid and native coin/DEX scope; spot holdings are balances, not invented futures positions. |
| GRVT | `grvt/account.rs::account_item(Positions/Orders)`; /lite/v1/positions, /lite/v1/open_orders. | Decode failure retains whole prior snapshot. Recover canonical client-order identity from metadata plus native-ID map; native order ID alone may not equal store ID. Preserve native/client bindings, timestamp fences and builder state. |
| Lighter variants | `lighter/account.rs::snapshot_row` nested positions and account_map_order; account/order REST and websocket snapshots. | Nested decode errors reject snapshot, but position_row None and unknown market order Ok(None) can omit prior records. Preserve numeric order index, market ID, signed-account identity, position key and settings; avoid short-string number coercion into identity. |
| ApeX Omni | `apexomni/account.rs::account_item`; positions and /v3/open-orders. | Mapping error fails list; position Ok(None) needs zero/out-of-scope/unmapped distinction. Preserve id/client binding, canonical symbol+side, leverage proof and settings. |
| AsterDEX | `asterdex/account.rs::on_account_item`, regular account/position and order lists. | Invalid economics/order map fails list. Retain orderId and symbol/positionSide keys, settings. Signature/version/account errors stay fatal; never retry writes through row helper. |
| Paradex | `paradex/account.rs::on_item`, registered_market, /positions and /orders commits. | registered_market Ok(false) skips unmappable rows before decoder; invalid known rows retain whole list. Preserve ID/symbol plus position/order version maps; absence currently records closures, so rejected IDs must be excluded from that step. |
| Coinbase aliases | `coinbase/account.rs::private_on_item` Positions/Orders and finish_orders. | Mapping/duplicate/size failures mark whole snapshot invalid and retain old state. Preserve native order_id, exact placement-unit proof, fill value and CFM/INTX scope/position IDs; do not silently downgrade wrong portfolio. |
| Polymarket | `polymarket/account.rs::account_item`; /data/orders and Data API positions. | Invalid order/holding fails snapshot; unknown holding asset is silently skipped. Preserve native order ID/token asset identity, prediction-holding side table, trade coverage. Final absence marks orders terminal: rejected IDs must never enter that path. |
| BreakoutProp | `breakoutprop/account.rs::parse_portfolio` + Snapshot::commit. | Typed portfolio/semantic invalidity rejects whole transaction (existing tests pin this). Row isolation must preserve positionCode/orderCode and linked settings, but account mismatch and balance completeness remain fatal. |
| TxFlow | `txflow/account.rs::account_item` State/Orders + commit_state. | Malformed mapping invalidates read; valid zero position returns None. Preserve asset/coin+mode and native order IDs; keep account/nonce/authorization structures strict. |
| Phemex | `phemex/account.rs::account_item` and on-orders response; /g-accounts/accountPositions plus currency-scoped orders. | Parse or row.map failure invalidates currency slice; unknown-market position skipped. Preserve orderID, symbol/posSide/currency slice, native context, terminal/provenance map. Valid zero position is an explicit removal, not a malformed row. |
| WEEX | `weex/account.rs::account_item`, /capi/v3/openOrders, /openAlgoOrders, /account/position/allPosition. | Empty mapped order or failed position staging fails cycle. Preserve normal/algo parent+leg IDs and symbol/side; configuration/balance rows remain strict. |

## Shared helper design to audit before implementation

Do not implement row isolation by deleting `failed=true` assignments globally.
Use one engine utility for snapshot coverage while each venue owns field names,
account/product eligibility, native identity conversion and revision rules.

Suggested outcomes for a structurally valid JSON item:

- **Accepted(keys, rows)**: complete native economics/provenance map succeeded.
- **Explicitly closed(keys)**: fully valid native zero/terminal state; retains
  the venue's existing final-event proof and stale-snapshot fences.
- **Out of requested scope**: positively identified other account/product only
  where the endpoint contract permits it. Account mismatch in an account-bound
  envelope is a failure, never a benign filter.
- **Rejected(Some(keys))**: recognizable native identity but bad/unmappable row;
  warn generically and preserve current matching rows plus necessary descendants
  and metadata. Do not replace them with zero/default economics.
- **Rejected(None)**: identity cannot be recovered safely. Valid peers may be
  applied as incremental updates, but this response cannot authorize removal of
  any absent row in its scope. Retain the old set and arrange reconciliation.

A bounded `SnapshotCoverage<K>` can collect protected keys plus an
`unidentified` completeness flag. Before replacement, the venue merges protected
**current** records into accepted rows; the helper must not resurrect an earlier
clone over a newer websocket terminal/fill/update. Existing generation/revision
checks run first. Native-to-canonical aliases (GRVT client IDs, PM strategy IDs,
compound child IDs), family and account/product scope are mandatory inputs, not
engine guesses. Preserve the same keys in side maps for cancel/amend proofs,
leverage/units, prediction holdings, cumulative fill values and provenance.

Full successful empty snapshots remain authoritative and remove ghosts. A
missing/malformed list field, wrong envelope/account, HTTP/auth failure,
truncated stream, duplicate conflicting identity, page-cycle/limit or missing
continuation token is **not** an empty snapshot or an isolated bad row. Keep the
last complete state, expose failure and retry safe reads. Balance/equity totals
must not be recomputed from a partial subset of assets/positions; keep native
aggregate validation strict until an explicit partial-balance contract exists.

The identity parser should inspect `RawValue` before typed economics, accept only
exact native string/integer representations, and reject float-rounded IDs.
Never fabricate an identity from a missing symbol/side. Warnings include stage
and row category only, never raw private rows/keys/auth/signatures. A rejected
row is not an execution and must not emit a fill.

## Regression phases (pending)

1. Shared coverage tests: known bad ID retained; unknown bad ID cannot authorize
   deletions; valid peers update; a valid empty list still removes ghosts;
   family/scope/child identity and side maps remain coherent; no new fill.
2. Venue fake transports: seed two live rows, return one changed valid row and
   one known-ID malformed/unmappable row; assert only valid row updates. Add
   unidentified row, missing envelope list, malformed body, valid zero/terminal,
   failed later page and successful empty variants separately.
3. Delay snapshots across WS fill/cancel/reopen or account mode/generation
   change; retained rows must be the current state, never a stale clone. Keep
   existing terminal/closure fences and no duplicate execution notifications.
4. MEXC ticker tests: >100 markets, select an initially excluded market, switch
   selection live, prestart selection, reconnect, duplicates/unknown symbols,
   empty reset and active order/book/candle interactions. Assert actual protocol
   messages and live store updates, not a stored selection field alone.
5. Record aggregate-feed no-op equivalence per family. Only implement additional
   selectors where freshness, caps or explicit caller semantics require them.
6. Add actual Wasm private-row/app-payload boundary tests, per-phase correctness
   review, all gates, PR review comments and exact green CI before landing.
