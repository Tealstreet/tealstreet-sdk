# Phase 9: TypeScript caller inventory and deletion dependencies

Audited 2026-10-20 on the phase-9 readiness branch, against the repository's
`packages/` and `apps/` source. This is an inventory and an ordered future
migration plan. This change deletes no TypeScript venue. The authoritative
phase-9 decision and gate results live in [RUST_REWRITE.md](RUST_REWRITE.md).

## Log shim removal (PR6)

Engine diagnostics now use `Event` exclusively. WASM actions and Durable Object
messages use `{ op: 'event', level, code, message, fields }`; native clients get
`{ type: 'event', level, code, message, fields }`. Levels are debug/info/warn/error.
Host transport/command callbacks still use `onLog`; those are not venue shim
notices. The DO wraps them as `host.diagnostic` events, and its refused requests
as `host.rejected`; native oversized notices use `host.notice_dropped`.

New engine codes: `command.failed` for replyless diagnostics, `command.duplicate`
for an in-flight ID collision, `startup.incomplete` for the startup watchdog,
and `public_feed.rejected` for invalid trusted-provider updates. Startup retries
use `request.failed`; malformed discovery rows count `row.skipped`, and rejected
empty catalogues count `snapshot.discarded`, including when their warning is
filtered. Existing severity and retry cadence are preserved.

**Deploy safe-cex-server before Info+ venue events matter to native clients.**
The old NativeFrameSession rejects `event`; deployment is Sam's call and this
PR performs none. DO clients must accept the coded `event` envelope before
adopting this host version. tealstreet-next remains read-only in this lane.


## What is ready, and what is still a deletion blocker

The SDK exposes **49 registered exchange names**. `RUST_VENUES` includes **48**.
The two registries currently default **9 names** to `RustEngineExchange`:
`coinbase`, `coinbasespot`, and Orderly's `tealstreet-dex`, `kodiakfi`, `mode`,
`salsadex`, `what`, `woofi`, plus testnet-only `txflow`. They default the other **40 names** to TypeScript,
including **39 names with an available Rust adapter**. Registration in
`RUST_VENUES` proves the adapter exists; it does not prove every optional product,
credential mode, consumer, or live deployment is ready to delete.

Before this readiness change, only `ExchangeWorker` selected Rust from explicit
`engine: 'rust'`; `createExchange` ignored that option. This PR adds explicit
Rust selection to the SDK factory as well, preserving the default registries.
Consequently CLI, MCP and public factory consumers can opt in through the same
factory, but their current default creation paths still use the registry. An
explicit conformance suite is evidence for the tested frame/store/consumer
contract, not evidence of live private connectivity or an automatic migration
of saved accounts.

| Family / exact registered names | Rust adapter | Default registry / deletion condition |
| --- | --- | --- |
| Coinbase: `coinbase`, `coinbasespot` | Both | Rust default; active CFM/spot and saved spot-only. Legacy INTX fixtures remain compatibility coverage for retired Advanced Trade routes. Frozen TS reference remains a harness dependency. Inert Starbase code is not an enabled product. |
| Orderly: `tealstreet-dex`, `kodiakfi`, `mode`, `salsadex`, `what`, `woofi` | All six | Rust default; all are broker options of one Orderly crate. `woo` is WOO X, a separate family. Frozen TS reference remains a harness dependency. |
| `txflow` | Yes | Rust default, testnet USDC perpetuals only; mainnet rejects before I/O and production/preview availability stays disabled. Retain `__tests__/reference/txflow` for independent public/private replay and audits, plus the production `exchanges/txflow/txflow.client-id.ts` helper used by the Rust wrapper. The former Phase-0 scaffold has been replaced; preserve [exact signing, account, order/query scope and measurement gates](TXFLOW_RUST_PORT.md). |
| Binance: `binance`, `binancepm` | Both | TS default; preserve USD-M and PM protocol/account/query scope and regression gates while switching both registries after consumer/routing/key-mode evidence. All four Binance Rust profiles now exist; shared TS implementation/reference files still have callers until each profile defaults/migrates. |
| Binance Spot: `binancespot` | Yes | TS default; signed WebSocket API user-data subscription with ACK/subscription-ID/liveness checks, native per-asset currency balances and USD totals; no derivative positions/settings. Native client-order-ID identity and supported regular/protective/order-list paths retain whole-batch preflight and final-fill-aware cancel/replace; unsupported derivative flags/STP/trailing or conditional parameters reject. Private history/fills/income retain the existing TS empty-result contract; public-trades/client-lookup queries unsupported. Preserve exact scope, live public versus documented private evidence and USD-M/PM controls in [BINANCE_VARIANTS_PORT.md](BINANCE_VARIANTS_PORT.md). |
| Binance COIN-M: `binanceinverse` | Yes | TS default; native contract order/placement amounts, USD-per-contract multiplier, base position contracts, coin-denominated PnL/margin and USD-valued aggregate balances with unpriced coin detail retained. Worker/main bridges preserve native count versus quote notional. Regular/stops/trailing endpoints and cancel/replace retain identity/terminal/final-fill proof; caller client IDs reject before batch writes. Public trades implemented; private history/fills/income retain TS empty results and client lookup unsupported. Preserve one-way/hedged/account sizing/routing evidence, and shared USD-M/PM regressions; [exact profile scope](BINANCE_VARIANTS_PORT.md). |
| Bybit: `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader` | All five | TS default; typed classification now reaches the real CLI TradFi preflight. The `bybit` linear account supports all four `enableInverseMarkets` / `enableOptionMarkets` combinations, category-specific feeds/reconciliation and native sizing, typed option terms/Greeks and explicit option APIs. Standalone inverse/spot/demo/Hyro retain their own profile boundaries; registration does not enable combined options on them. Actual TS/native and documented private fixtures do not establish live combined/private connectivity; Hyro connectivity remains unverified. Preserve `bybit.site`, public order parameters, shared-feed/default migration and [exact evidence](RUST_PRODUCT_SEMANTICS.md). |
| Bitget: `bitget`, `bitgetusdc`, `bitgetinverse` | All three | TS default; switch together after inverse/native-size and UTA ledger/trailing/optional-product checks. Preserve transport constants shared outside the family. |
| OKX: `okx`, `okxeu`, `okxdemo`, `okxeudemo` | All four | TS default; global linear/inverse swaps and EU `FUTURES` rows with `ruleType=xperp` now share the existing crate. Account-scoped EU discovery, private product subscriptions/reconciliation, scoped/joined histories and worker/main native chaser units are covered by typed fixtures. Preserve native placement counts, reference-price inverse base quantities, USD position PnL/margin and native ledger currency. Live public captures do not prove live private sizing/connectivity; host/demo, external-public account overlays, defaults and broker extraction still gate deletion. [Exact scope/evidence](RUST_PRODUCT_SEMANTICS.md). |
| `apexomni` | Yes | TS default; native zkLink/API auth plus wrapper/consumer gates; no inference to other ApeX products. |
| `asterdex` | Yes | TS default; V1/V3 auth, builder attribution and exported wallet approval utilities remain independent deletion dependencies. |
| `backpack` | Yes | TS default; perpetuals only, with native uint32 decimal caller IDs; do not imply Backpack spot. |
| `bingx` | Yes | TS default; explicit Rust selection is available; migrate default creation after gates. |
| `blofin` | Yes | TS default; preserve broker constants and native-unit consumer semantics. |
| `bitrue` | Yes | TS default; futures adapter; no spot coverage inferred. |
| `cryptocom` | Yes | TS default; preserve optional account/product gates before switching creation defaults. |
| `kucoin` | Yes | TS default; futures adapter, native contract-unit and broker configuration gates. |
| `kucoinspot` | Yes | TS default; opt-in native HF Spot in the KuCoin crate. Preserve native Spot IDs/base units, trade-account currency balances, broker/passphrase and secondary-read routing, caller-context amendments/cancels and fixed Spot settings. Futures coverage is a separate profile; see [scope and evidence](venue-ports/KUCOIN_SPOT.md). |
| `hyperliquid` | Yes | TS default; perps/HIP-3 adapter. Public builder/agent utilities and `hyperliquid.cloid` still have callers. |
| `hyperliquidspot` | Yes | TS default; distinct spot product in the existing Hyperliquid crate. Native spot identities, balances, signing, live public/documented private gates and native worker/late-consumer checks pass ([RUST_SPOT_PRODUCTS.md](RUST_SPOT_PRODUCTS.md)); no combined perps/spot account claim. Preserve wallet/builder/agent and cloid helper callers before deletion. |
| `kraken` | Yes | TS default; futures adapter. |
| `krakenspot` | Yes | TS default; distinct spot state machine in the existing Kraken crate, including Spot form signing, WS v2 token authentication and atomic txid amend. Live public/documented private gates and native worker/late-consumer checks pass; exact scope and independent review evidence are in [RUST_SPOT_PRODUCTS.md](RUST_SPOT_PRODUCTS.md); preserve futures regression and consumer/default migration prerequisites. |
| `lighter` | Yes | TS default; local signing on main/test chains 304/300. Remote-only account snapshot polling, payload-only public startup and mixed local/callback behavior match the current TS startup contract; no supported remote transaction signer in TS. Keep wallet approval/account-tier helpers, the wrapper's API-key-index parser and TS oracle; [variant evidence](LIGHTER_VARIANTS_PORT.md). |
| `lighter-robinhood` | Yes | TS default; explicit Rust alias with Robinhood main/test URLs, USDG, signing chains 466324/300 and no default zkLighter integrator attribution. Same credential modes and retained helper/oracle requirements as `lighter`; [variant evidence](LIGHTER_VARIANTS_PORT.md). |
| `mexc` | Yes | TS default; conditional/trailing and native-unit proofs still required when removing reference coverage. |
| `paradex` | Yes | TS default; retain binary/text feed and Starknet account mode checks. |
| `phemex` | Yes | TS default; retain native account identity and shared broker constants. |
| `weex` | Yes | TS default; retain protocol/routing gates. |
| `woo` | Yes | TS default; WOO X, not Orderly `woofi`; keep WOO position-history adapter used by Rust. |
| `polymarket` | Yes | TS default, development-only (`enabled: false`, `enabledInDev: true`), `supportsChaser: false`, native 1x leverage; generic algorithm fixtures do not enable venue features. Prediction CLOB V2 outcome shares with typed prediction/holding metadata; native local-wallet signature types 0/1/2/3 and supplied/derived L2 credentials. Limit/GTC only; no client IDs, market/conditional/bracket/reduce-only orders, or generic fills/order-history/income/public-trades query claims. Preserve authoritative final-fill-safe cancel-replace and bounded sampled price history. Read [venue-ports/POLYMARKET.md](venue-ports/POLYMARKET.md) for exact auth/product/query scope; private tests use documented fixtures, not live-key proof. |
| `grvt` | Yes | TS default; perpetual-only Rust catalog, with local EIP-712 signing (chains 325/326), API-key/session login, routed/direct private auth, subaccount discovery and mandatory builder authorization. Canonical caller IDs are decimal 1..2^63-1; limit/market/protective/bracket commands and final-fill-safe cancel-replace, one-way positions, real leverage reads/updates; margin-mode mutation unsupported. Bounded fills/history/funding/public-trade queries and definitive client-ID lookup, including the chaser fill-verification hook. Preserve public browser builder-authorization exports and signing/session routing. [Explicit retirement decision](GRVT_PRODUCTS_DECISION.md) resolves unexposed mapper-only Future/Call/Put paths; no future/options claim or TS deletion. Exact scope/evidence: [GRVT_RUST_PORT.md](GRVT_RUST_PORT.md). |
| `breakoutprop` | Yes | TS default, legacy DXtrade only and disabled in production/preview (`enabledInDev: true`); authenticated REST polling, no venue WebSocket or anonymous public feed. Username/password and exact FULL_TRADING account selection are required even for public-data-only reads; `supportsChaser: false` remains enforced by metadata. LIMIT/MARKET/last-price STOP, GTC, reduce-only as CLOSE, confirmed zero-fill cancel-replace; caller IDs, attached brackets/close-all and leverage/margin/mode changes unsupported. One-way/cross account model, native lot IDs and explicitly estimated margin/PnL; quote-derived books/live candles are synthetic. Generic fills/order/income history remain branded inert fallbacks, public trades unsupported; internal cancel history proof is not a history-query implementation. Public fixture is documented/scripted under the recorded waiver, with live credential capture/connectivity still unverified; read [RUST_BREAKOUTPROP.md](RUST_BREAKOUTPROP.md). |
| `bitunix` | Yes | TS default; opt-in USDT futures. Closed-PnL rows stay TS only (no `closedPositions` capability). Scope and gate evidence: [RUST_BITUNIX.md](RUST_BITUNIX.md). |

Every `ExchangeName` is in `RUST_VENUES`.
Do not silently route a venue through another family's adapter.

Native consumer proof includes both WebSocket venues and HTTP-polling venues.
BreakoutProp starts the actual Rust engine with documented DXtrade session/
account/quote responses; it emits no venue socket frames. Its fixture proves
polling dispatch, frame/store publication and consumer compatibility under that
input. It is not a live credential capture or evidence that new proprietary
Breakout Terminal accounts work with this legacy DXtrade adapter.

## tealstreet-next callers at cutover

Read-only inventory of every tealstreet-next use of the SDK that the landing
branch changes. The landing branch reaches master and tealstreet-next in one
cutover, so each item below must be handled in the same change set (or before).

**Inputs.** Original tealstreet-next audit: `origin/master` `41e538d78d` (2026-10-20), read with
`git grep`/`git show` only; Tealstreet/tealstreet-mobile `master` through the
GitHub API. SDK `origin/feat/rust-rewrite` `1fa06c93b5` (2026-10-21), which has
the merged contract PRs #1375, #1377, #1383, #1386, #1394, #1400, #1411, #1424,
#1401, #1439, #1441, #1443, #1445, #1447, #1451, #1453, #1462, #1467 and #1469 (venue construction and the Polymarket, Orderly, BreakoutProp and AsterDEX construction rules moved into the shared `safe-cex-venues` crate; messages and the `start()` rejection point unchanged) and the per-venue classified-error PRs (#1403, #1404,
#1417, #1425, #1429, #1432, #1434, #1436, #1437, #1438, #1442, #1444, #1446,
#1452, #1455, #1457, #1458, #1460, #1463, #1468, #1470, #1471, #1473, #1478,
#1479, #1475, and the B9 SDK PRs #1482, #1483, #1484), plus #1388 (engine log
events, `getHealth`), #1450 (B5), #1510 and #1531 (payload-signer routing, narrowed to Bybit/Binance).
The [2026-10-21 refresh](#contract-refresh-2026-10-21) checks SDK landing
`3d73437e589`, tealstreet-next local `origin/master` `fdd2389a825`
(`git grep`/`git show` only), and mobile `master` `8377a4f3f1e` through the
GitHub API. Existing line references retain the original NX audit revision;
new history-forwarding references below use `fdd2389a825`. Open PR proposals
are not merged contracts.

Construction follow-ups: checked SDK landing `570d15f3001` for #1563/#1567/#1575, retaining
those downstream inventory revisions. The existing NX script-editor options
already declare `extra.asterdexApiVersion?: 'v1' | 'v3'` and
`extra.hyrotraderMode?: 'demo' | 'production'`; no new caller edit.
The native account/fill follow-up checks #1430 at `3002a226469` against the same
NX revision; mobile `master` remains `8377a4f3f1e` (GitHub API).
The leverage follow-up checks #1522 at `faad41cfba5` with those inventory revisions.
The coded-health follow-up checks #1583 at `916585a30f2`, with NX `fdd2389a825`
and mobile `8377a4f3f1e` unchanged; its caller inventory remains cutover work.
The OKX port follow-up checks #1588 at `447134dbf24`; mirror `origin/master`
`1f91dfe4dcb` still has the old 8443 URLs. Live-master/hot-patch status below is
the overseer's 2026-10-21 report, not evidence of a deployed change.
The coded-only host follow-up checks #1586 at `ef68c4ba563`; its landed PR6
protocol/deployment section above is retained as cutover work.
The fill-identity follow-up checks #1592 at `389f42aa2e2`; focused live-fill
callers were rechecked read-only at NX `65e086a3c90`. Earlier inventory pins
still apply to the other caller rows.
The private-snapshot follow-up checks #1524 at `b69285f9909`; caller shapes
and diagnostic delivery remain the already documented SDK/host contracts.
The construction build follow-up checks #1599 at `e7f0b3d3628`; the shared
JSON remains a source-build input, now consumed by the Cargo build script.

**Impact key.** *CT*: breaks at compile time. *RT*: compiles, behaves
differently at runtime. *Blocker*: no new equivalent exists yet; cutover
cannot proceed for the affected venues/hosts until one does.

**Re-run** when a contract PR merges: repeat the three scans in
[Audit method](#audit-method), then update the Status and Resolving PRs columns
and the rows that cite that PR. Open contract work is tracked separately from
the merged behavior below. #1430 introduced financial tails under revision 1;
#1592 now writes FILLS revision 2 with execution identity, as detailed below.

### Contract refresh, 2026-10-21

Initial batch checked landing `3d73437e589`, including the merges after the last refresh
(#1549, `00efd136bb6`) and the earlier candidates named in the handoff.
The construction follow-ups check through #1575 at `570d15f3001`, including
#1563, the #1550 logging merge and #1518's internal failure-path refactor.
The subsequent #1558 logging-level merge is checked at `e757d8d7990`.
Native account reads, identities, fill financials and nuke check #1430 at `3002a226469`.
Fresh leverage reads and combined capability records check #1522 at `faad41cfba5`.
Coded health, SDK event metadata and worker forwarding check #1583 at `916585a30f2`.
The following rows record the caller effect, including changes requiring no
downstream edit. PR numbers refer to Tealstreet/tealstreet-sdk.

| Merged PR / landing commit | Contract and caller effect |
| --- | --- |
| #1476 / `0741419bf2` | Local signing-credential validation moves from TS into `safe-cex-venues`, shared by WASM/native construction. The existing error text and `start()` rejection remain; engine loading now precedes this validation. B2's hosted TS hedge and #1531's payload-signer routing remain in force. |
| #1519 / `9363852438` | One options blob reaches every venue; each Rust arm consumes its fields. No caller edit. A non-AsterDEX secondary read key without a secret now gets `Secondary read credentials require a nonempty key and secret`, replacing a decode error; AsterDEX keeps its separate read-agent rules. |
| #1520 / `418e4f6ebc` | Engine capability `hostClock` (CAPABILITIES bit 14) replaces the bridge's virtual-clock name list for Backpack, Coinbase/spot and Orderly. Same corrected signing time; no caller edit. Ship matching engine and reader. |
| #1392 / `48aeec3783` (earlier merge, stale caller row corrected here) | Resolved Base polling policy now scales every Rust venue's periodic reconciliation, including server multipliers and auth backoff; `Infinity` pauses and a finite update resumes. Socket supervision, startup and explicit reads are unaffected. Existing caller configuration works unchanged. |
| #1529 / `ea39da2a1b` | BloFin historical-order `amount`/`filled` use base units, as fills/open orders already do; unknown-market rows are omitted. The owner-run `historical_orders` backfill remains required before Rust ingestion. |
| #1531 / `a4d17f8a88` | Only private Bybit/Binance-family `payloadSigner` accounts fall back to TS. Other non-Paradex private Rust accounts refuse the option as `unsupported`/`notSent`; the construction row below already records the MCP effect. |
| #1542 / `7ee1411110` and #1554 / `f52f08ea65` | Kraken futures returns the available newest ledger page when its 500-row page cannot be traversed, with `history.incomplete`; Binance exhausted income walks also warn. #1554 adds command-correlated, non-enumerable `incomplete: true` to those array results in the JS WASM host. Forwarding and ingestion must preserve/read it (B11). Kraken HTTP failures also retain the venue error code in their text. |
| #1551 / `c2a36b7ca1` | Binance native stop-limit orders refuse `updateOrder` as `unsupported`/`notSent`, before cancellation, instead of becoming stop-market orders with the wrong trigger. Regular stop-market amend remains supported. Surface the refusal; no new call shape. |
| #1513 / `084a326abf` | Cancel/amend of a just-acknowledged placement may wait for its order-store row on opted-in venues, up to 10 s before dispatch. Chaser STOP already waits for placement IDs; no caller edit. A timeout still requires reconciliation before retry. |
| #1527 / `8c1e779021` | A book-cadence frame carrying fills also carries pending store sections; worker/main consumers apply orders before emitting those fills. No reader or FILLS revision change. Venue-delivered execution-before-order ordering remains possible. |
| #1538 / `c18c30e45e` | First venue batch uses coded log events; duplicate auth notices in one dispatch count once. Native sessions use `event` notices; DO logs carry `code`/`fields` (#1388). No new `exchange.on` code payload: B9's classified fields remain the host integration. |
| #1547 / `e8ad6facc1` | Hyperliquid zero TP/SL prices omit that bracket leg; unified/portfolio spot holdings populate `balance.coins` for the margin panel. `currencies` remains unset to preserve sizing. No caller edit. |
| #1555 / `92a1608191` | Bybit OHLCV with `limit` ignores `from` and returns the newest bars ending at `to`, as TS. Chart/history callers keep the same arguments. |
| #1546 / `2bd1ef7ae8` and #1556 / `5b0c16a952` | Mobile options/costing and existing production issues are documented in [RUST_MOBILE_B3.md](RUST_MOBILE_B3.md). B3 and the app fixes remain owner items. |
| #1501 / `550e534e08` | Spot-enabled `hyperliquid` accounts now run on Rust; the perp engine merges spot pairs, holdings and orders. B8's product fallback is superseded. The keyless hosted/payload-signer TS routing remains in place. |
| #1506 / `c0d37f95d9` | Optional `UNDERLYING_TOKENS` frame section 20 carries merged Hyperliquid spot token names; both consumers reconstruct `Market.info.hlTokenName`/`hlTokenFullName`. The Margin panel's underlying-token risk disclosure works without an app edit. Ship matching engine/reader/applier. |
| #1560 / `d68a47b7c7` | Orderly bounded OHLCV reads now use native history and preserve caller bounds; trailing-stop display uses the native extreme/callback with BUY adding and SELL subtracting. Candle/order shapes and write contracts are unchanged. The landed display/read note is preserved below. |
| #1550 / `012e073e4f` | The remaining fifteen venue families use coded log events, completing #1538's migration. Message text and levels remain unchanged; B9's classified-error integration and the native/DO notice contract remain the caller requirements. |
| #1563 / `60a3dc1e79` | Shared Rust AsterDEX API-version resolution now also supplies optional WASM construction metadata. The wrapper forwards only a valid explicit `extra.asterdexApiVersion` override and adopts the native version for signing eligibility/STP checks after loading. Synchronous pre-host and older-host inference remains parity-tested; account/options/trading call shapes are unchanged. |
| #1567 / `d06eabae16` | One shared JSON rules table derives signing/public-mode eligibility synchronously in TS and before native validation/startup. Optional raw `publicMode` carries caller intent; `construction()` adds optional `canSign`/`publicDataOnly`, and the WASM module exports no-I/O `resolveConstruction`. Existing application options and legacy native/harness input remain compatible; the explicit-private Paradex refusal moves into the registry. See [Construction metadata and public mode](#construction-metadata-and-public-mode-1563-1567). |
| #1518 / `8b115cefd7` | Internal `Cx::done` is success-only and `Cx::fail` owns command failures. Hosts still emit the existing `done` error shape with classified detail; message text is unchanged. No caller edit or new wire revision. |
| #1575 / `570d15f300` | Rust selects HyroTrader endpoints from optional raw mode/network maps, retaining legacy explicit endpoints when that config is absent. Private non-Paradex `payloadSigner` refusal moves into the registry; WASM registry failures carry JSON `Error.venueError`, which the SDK converts to `ExchangeError`, preserving `unsupported`/`notSent` for that refusal. Factory routing and application options stay unchanged; see [Host construction rules](#host-construction-rules-1575). |
| #1558 / `e757d8d799` | At migrated retried-read sites, the first failure is Error; repeats with the same digit-normalised message at that site are Warn until 90 s after its last Error, or until the message changes. Text stays stable, reducing repeat toast/Sentry reports while retaining periodic Errors for auth detection. Backpack optional child-leg failures now emit Error instead of Warn. No caller shape or classified-error field changes. |
| #1430 / `3002a22646` | Account reads synchronize authoritative native state without a network refresh or new streaming freshness observation; worker mirrors forward the reads. Native identity lookup expands by capability. Live fills add optional `realizedPnl`/`fee` tails under the existing revision-1 header, and native nuke batches closes without automatic retry. See [Native account reads, identities and fills](#native-account-reads-identities-and-fills-1430) for caller and host requirements. |
| #1522 / `faad41cfba` | BingX/WEEX `fetchLeverageForSymbol` now makes fresh native symbol reads (`leverageQuery: 'onRead'`), retaining `account: 'query'` in the combined capability records. The metadata-refreshed bridge issues a native command for uncached symbols instead of returning a cache miss immediately; valid published rails remain cached. Existing UI calls and `FetchedLeverage \| null` shape stay unchanged. |
| #1583 / `916585a30f` | SDK `engine-event` and the same worker op expose received coded diagnostics. Legacy Error and Info/Warn delivery retains event metadata for caller-controlled duplicate suppression; Error telemetry adds scalar `event_code`. `ExchangeHealthMonitor.processEvent` classifies codes/optional typed detail, and `processHealth` copies authoritative engine counters. Downstream handlers still need migration; preserve the [coded diagnostic caller inventory](#coded-diagnostic-caller-cutover-pr5). #1586 removes the engine log shim; host protocol and deployment requirements are recorded above. |
| #1588 / `447134dbf2` | Rust/TS OKX public/private/business WebSocket URLs use default TLS 443 across global/EEA live/demo, preserving host/path; relay upstream metadata changes the global live private/business targets. SDK arguments stay unchanged. Proxy upstreams and live master need a separate owner-run port migration before the public-internet 8443 deadline; see [OKX operations prerequisite](#okx-websocket-port-operations-prerequisite-1588). |
| #1586 / `ef68c4ba56` | Removes `Cx::log`, `LogLevel`, `Notice::Log`, WASM `log` actions and native `log` notices. Diagnostics use coded `event` envelopes; DO changes from `op: 'log'` to `op: 'event'` with `warn`, preserving its host `onLog` callbacks as coded host events. Deploy compatible safe-cex-server before native clients receive Info+ venue events; DO clients must accept the new envelope before host adoption. Sam owns that deployment. See [Log shim removal](#log-shim-removal-pr6). |
| #1592 / `389f42aa2e` | Rust always writes FILLS revision 2 with optional execution-ID refs and aligned financial columns. Updated readers retain revisions 0/1; old readers reject revision 2, requiring coordinated producer/consumer builds. Hyperliquid perp/merged spot/spot-only `tid` becomes SDK fill `id`, with `orderId` separately preserved; other venues retain order-ID fallback. See [Fill identity and wire compatibility](#fill-identity-and-wire-compatibility-1592). |
| #1524 / `b69285f990` | Private snapshots (batch-3 venues) isolate rejected rows while valid peers commit; stored rows protected by rejected identities remain visible, including Bitrue protective children and Kraken open orders. Unidentified rows prevent absence-based removal within their snapshot scope. Coded `row.skipped` / `snapshot.discarded` diagnostics use the existing event/health paths. No new caller shape; see [Private snapshot row isolation](#private-snapshot-row-isolation-1524). |
| #1599 / `e7f0b3d36` | Rust construction rules are compiled from the shared JSON into a static table by Cargo `build.rs`, replacing runtime JSON decoding. B1 source builds still require the JSON input; compiled engines contain the table. TS rules, parity and caller options/eligibility are unchanged. |

#1543 (retry jitter), #1552 (bounded Binance Retry-After), #1545 (WASM
headroom), and the #1541/#1544/#1548/#1557 replay-clock fixes do not change a
downstream call shape. They add no cutover edit here. #1592 separately lands
execution-ID attribution and FILLS revision 2; #1430 did not introduce either.

### Summary: cutover blockers (no new equivalent)

| # | Status | Resolving PRs | Blocker and what is left | tealstreet-next callers | Affects |
| --- | --- | --- | --- | --- | --- |
| B1 | **Open: owner decision 1** | none | **Engine delivery.** Copybara does not mirror `sdk-rs/**`; tealstreet-next already owns an unrelated `sdk-rs/`; `@tealstreet/safe-cex-wasm` `pkg/` is a gitignored build output. No tealstreet-next build runs a Rust/wasm step. See [Build and delivery steps](#build-and-delivery-steps). | every host below; `apps/web/next.config.mjs`, `services/exchange-server` build, `apps/cli/scripts/build.sh`, `packages/exchange-mcp/tsup.config.ts` | all 9 default-Rust names immediately; every opt-in later |
| B2 | **Decided short term: keep remote signing** (Sam, 2026-10-21). Hosted accounts keep working at cutover; the engine port is costed, undecided | #1498 (hedge), #1505 (hosted Coinbase/Orderly); costing in [RUST_REMOTE_SIGNING.md](RUST_REMOTE_SIGNING.md) (#1500) | **Keyless hosted signing.** exchange-server constructs private accounts with `secret: ''` and a live `RemoteSigner` (`services/exchange-server/src/server.ts:1015-1034`); the browser signs via `exchange.signHttpRequest`/`signWsAuth` (`apps/web/src/controllers/exchangeController/exchangeAccountController.ts:3029`, `apps/web/src/utils/shim/worker/signingHost.ts:191`). No engine path signs through a remote signer. Hosted accounts must keep working at and after cutover; server-held session keys are the likely long-term direction but are not committed. **Hedge:** `runsOnRustEngine` keeps every keyless remote-signed account (`isKeylessRemoteSigned`: a `remoteSigner`, no `secret` and no AsterDEX V3 wallet key) on the TS venue, so `createExchange` and `ExchangeWorker` never start it on an engine that cannot sign; a directly built `RustEngineExchange` refuses it at startup with `keyless remote-signed accounts run on the TS venue` instead of starting and rejecting every order (Hyperliquid did). Lighter (read-only without a secret, as TS), Paradex and Polymarket (signing key in `key`) stay on the engine. **Default-Rust names (#1505):** master's TS Coinbase and Orderly venues are restored as hosted-signing-only venues (`exchanges/hostedSigningVenues.ts`). A keyless hosted account on coinbase, coinbasespot or an Orderly broker runs them, and a browser `RustEngineExchange` signs `signHttpRequest`/`signWsAuth` through them, as master did; a main-thread mirror whose server worker is TS keeps its forwarders' capability defaults. TxFlow has no kept venue (no hosted users; its master build was a stub). Orderly hosted private WS signs with the local secret, so it stays unavailable, as on master. **Left:** the engine port itself (Sam's decision, costed in RUST_REMOTE_SIGNING.md); these TS venues are deleted when it lands or session keys replace remote signing. | `services/exchange-server/src/server.ts:972-1106`, `remote-signer.ts:513-693`; web `signingHost.ts:74-191`, `exchangeAccountController.ts:2482,3029` | hosted mode for every Rust venue |
| B3 | **Open: owner decision 3** | options/costing #1546; live-issue inventory #1556 | **React Native / Hermes.** Sync reaches the wasm imports through `registry.native.ts`, breaking Metro bundling; Hermes cannot execute wasm. The 9 default-Rust names are filtered from the mobile UI, so bundling is the immediate cutover problem. [RUST_MOBILE_B3.md](RUST_MOBILE_B3.md) recommends keeping device TS for cutover and routing through exchange-server with session keys as a direction; neither is decided. Existing transport/signing/storage app fixes remain separate owner items. See [Mobile](#mobile). | tealstreet-mobile | the synced mobile bundle; any future on-device Rust execution |
| B4 | **Resolved in the SDK; one tealstreet-next edit** | #1443 (merged `6d6e3d666f`); #1462 (merged `341ea148bb`) | **Main-thread trading and reads.** An instance with a `callMethod` listener (the web controller's passthrough in `store`-payload and frame modes, and the hosted `ExchangeServerWorker` shim) forwards `placeOrder(s)`, `placeOrderWithClientOrderId`, `updateOrder`, `cancelOrders`, `cancelSymbolOrders`, `cancelAllOrders`, `findOrderByClientOrderId`, `nuke*`, `fetchPublicTrades` (`components/chart/datafeed/DefaultDatafeed.ts:836`), `fetchOHLCV` (`components/dom/Dom.tsx:237`, `lightweightCharts/SafeCexKLinesChart.tsx:29`) and the journal axes `fetchHistoricalFills`/`fetchHistoricalOrders`/`fetchIncomeHistory(Range)`/`fetchHistoricalPositions` (`utils/historical/collectAccountAxes.ts:229-241`) whole to the worker's engine. It adopts the worker's capability record without building an engine, so Lighter `changeAccountTier`/`fetchAccountTier` (`components/settings/profile/PerExchangeSettingsBody.tsx:74-130`) appear once markets load and forward. No edit for these. Optional edits: journal ingestion (`utils/historical/runHistoricalWindow.ts:45`) should `await exchange.loadCapabilities()` before `isAxisSupported`, or for one round trip after markets load it asks unsupported axes and records empty rather than unsupported; `PerExchangeSettingsBody` does not re-render when the tier methods appear, so a panel opened before adoption shows the tier controls disabled until its next render. **Left:** warm start (`utils/exchange/eagerExchangeWarmStart.ts:124` `ensureMarkets` → `fetchMarkets` at `:58`, from `hydrateExchangeFromCache.ts:31`) starts a second private engine on the main thread; skip it for Rust accounts (edit below). Deploy exchange-server before web: an older server answers forwarded methods with an empty failure. | `eagerExchangeWarmStart.ts:116-130`; optional `runHistoricalWindow.ts:45`, `PerExchangeSettingsBody.tsx:74-130` | web, all Rust venues |
| B5 | **Resolved; no tealstreet-next edit** | #1388 (merged `f9e21a8f37`), #1450 (merged `5ff7ebde6c`) | **Socket health and recovery.** Before #1450, `store.socketHealth` was never written by frames, `reconnect`/`recoverAfterOffline` were no-ops/`true`, `getAllSockets()` returned `[]` and `authFailureStreak` stayed 0. #1450 writes `socketHealth` through the TS summarizer on every socket change and every 5 s, makes `reconnect`/`recoverAfterOffline` close open engine sockets (code 1006) so the venue reopens them (`false` only after dispose or engine death), returns per-channel handles from `getAllSockets` (`isReadonlyFallbackActive()` is always `false`), raises `authFailureStreak` on credential rejections, and teaches the CLI's `findKeyRejection` the engine's wording. Every caller listed then works unchanged; a private stream counts as healthy for 60 s of silence (TS: 20 s), and a socket already in venue backoff (up to 30 s) is not hurried. | web `exchangeAccountController.ts:3391-3415`, `utils/exchange/getWebSocketStatus.ts:27-55`, `getExchangeAppearsConnected.tsx:34`, `components/connectionStatus/{ConnectionStatusIndicator.tsx:39,97,ReconnectingBanner.tsx:157-261}`, `nudges/hostedServerForInstability.ts:66`, `ConnectionStateWorker.tsx:364`, `utils/desktop/desktopRecovery.ts:82`, `order/createLimitOrder.ts:221-222`; CLI `packages/cli-core/src/commands/BaseCommand.ts:173`, `ConnectionCommand.ts:182-196`, `ConnectionMonitor.ts:128-132`, `legacyCommands.ts:28`, `apps/cli/src/repl.ts:2831,2903`; exchange-server `public-exchange-manager.ts:95,459` | all Rust venues |
| B6 | **Resolved; no tealstreet-next edit** | #1441 (merged `ae3a1fc122`) | **In-process shared public data.** Rust accepts exchange-server's `externalPublicData: { type: 'in-process', source }` (`server.ts:1015-1034`) unchanged. `serveInProcessPublicData` (`exchanges/rust/publicFeed.ts`) speaks the shared-feed protocol over an in-memory port pair, so the account ingests natively and validates the catalog as with `RustPublicFeedProvider`. Like the TS adapter it is lenient: private startup continues after 30 s without a loaded source, and account markets the source has not listed yet are tolerated (their book/candle requests wait). It follows the server's re-attaching source across public-exchange restarts and re-announces a changed catalog. Real field mismatches still fail, so the source must run on the same engine (at cutover the public exchange is Rust too). One hub per source broadcasts changed tickers every 250 ms. Websocket gateways still reject. Optional exchange-server edit: `createSharedPublicDataSource` keys by venue only, so testnet accounts receive the mainnet source; Rust ignores it for testnet and runs its own feed, while TS prices testnet off mainnet. | `services/exchange-server/src/server.ts:1015-1034`, `shared-public-data-source.ts:50-131`, `public-exchange-manager.ts:73-293` | hosted accounts on Rust venues with `supportsPublicOnly` |
| B7 | **Resolved; no tealstreet-next edit** | #1439 (merged `0ced7ccf1e`) | **Per-account proxy token across async engine work.** Each engine HTTP transport captures the contextual token provider of the context that created the account (`captureProxyTokenProvider`) and reinstates it synchronously around proxy authorization (`withProxyTokenProvider`), so requests started from timers, sockets, shared public feeds or another account's context use the account's own live provider (`transport/engine.proxy-token-context.test.ts`). The HOW adapter and relay sockets read the token after awaits or on open, so they bind the lookup at request/socket start (`HOWConfig.bindProxyToken`), and the engine creates relays inside the account scope. Shared HOW connections are still authenticated once per connection by whichever request opens them, as with TS. | `services/exchange-server/src/server.ts:2,59,77,796-819`, `virtual-worker-context.ts:681` | hosted mode, proxied venues |
| B8 | **Resolved; no tealstreet-next edit** | #1445 (merged `fc771ab13d`), #1447 (merged `8f29096de9`), #1453 (merged `fe43821c3e`), #1451 (merged `a0f4e7c53c`, superseded), #1501 (Hyperliquid spot markets) | **Missing per-venue methods.** `fetchClosedPnl` serves engine closed-position rows (`fetchClosedPositions` venue action) through the TS mappers for bybit, bybitdemo, hyrotrader, bybitinverse, the four okx names and blofin (#1445); `fetchHistoricalPositions` follows each TS declaration for bybit/bybitdemo/hyrotrader, okx (linear), blofin, bitget*, hyperliquid and paradex (#1447); Bybit's five names sync the raw REST wallet account into `balance.info` every 2 s (`fetchBalanceInfo`, #1453). All use the existing call shapes. Hyperliquid with `enableSpotMarkets` runs on Rust: the perp engine merges spot pairs (TS display alias), spot holdings (`balance.coins`) and spot orders, as the TS venue does. | web `components/openPositions/ClosedPositions.tsx:93-121`, `utils/historical/collectAccountAxes.ts:229`, `ChartContainer.tsx:161`, `useChartSession.ts:303`, `Margin.tsx:85-110,413,566`, `isBybitInverseUnifiedMargin.ts:25`, `placeOrderStoreData.ts:107`, `bybit-skin/.../AccountInfo.tsx:32-45,97`, `MarginHeader.tsx:56`; MCP `packages/exchange-mcp/src/tools.ts:182` | none |
| B9 | **Resolved in the SDK; caller migration remains** | #1377, #1383, #1401, #1424, #1482, #1483, #1484, #1583 (merged); venue PRs merged for all 26 venue families | **Typed errors.** `ExchangeError` (exported from `@tealstreet/safe-cex`, `kind`/`outcome`/`venueCode`/`venueMessage`/`retryAfterMs`/partial-batch fields) now rejects **command promises** on the main thread. Through the worker it arrives as a plain `Error` with the same fields (`serializeCommandError`/`deserializeCommandError`), so callers must read fields, not `instanceof`. Classified venues: apexomni, asterdex, backpack, binance, bingx, bitget, bitrue, blofin, breakoutprop, bybit (+variants, #1479, which also fixes the invalid-options hang), coinbase (+spot, #1478), cryptocom, grvt, hyperliquid, kraken (+spot), kucoin (+spot), lighter (#1475), mexc, okx, orderly (#1471), paradex, phemex, polymarket, txflow (#1473), weex, woo. Since #1482 `start()` rejects with an `ExchangeError` carrying the startup `kind` (a credential deactivation is `authInvalid`), and `validateAccount` returns `{ error, kind }` with unchanged `error` text. #1483 tags worker Sentry reports with closed-set `error_kind`/`error_category`/`error_outcome`/`venue_code` (`exchangeErrorTags`). #1484 adds `classified: { kind?, outcome?, venueCode? }` to the `'error'` event options on the main thread and through the worker, and `ExchangeHealthMonitor.processMessage(message, isError, classified?)` prefers a specific classified kind over text patterns. **Left (tealstreet-next):** pass `opts.classified` for ordinary command/legacy errors; for coded diagnostics, adopt #1583's `engine-event`/metadata, `processEvent` and `processHealth`. Suppress a duplicate legacy diagnostic only when its matching event metadata is present. Preserve auth/reload behavior and the caller inventory below. See [Error strings](#error-strings-that-must-move-to-codes). | see [Error strings](#error-strings-that-must-move-to-codes) | all |
| B10 | **Resolved, given B1** | #1375 (merged `b6c4f006a6`) | **candle-server public exchanges never started.** `fetchMarkets` (`exchange-pool.ts:265`) now starts the engine and waits ≤30 s for native data; it still needs the engine loadable in that image (B1). See [Mobile](#mobile). | `mobile-services/candle-server/src/exchange-pool.ts:169-312` | candles for the 9 default names |
| B11 | **Open: history completeness forwarding and ingest** | #1542, #1554 (host flag merged; propagation remains) | **Partial history must not become complete coverage.** Kraken futures' saturated income page and Binance's exhausted income chunks resolve with available rows and a non-enumerable `incomplete: true` in the JS WASM host. Worker/JSON hops lose that property; collection and derived history can also discard it. Carry completeness explicitly through each hop and consume it before recording coverage. See [Incomplete history](#incomplete-history-1542-1554). | SDK `CallMethodHandler`/Rust mirror; NX `ExchangeServerWorker`, `collectAccountAxes`, `runHistoricalWindow` | affected Rust income reads and any fill/position axis derived from them |

### What remains before cutover (beyond owner decisions B1–B3)

SDK work: preserve #1554's incomplete-history metadata across forwarding and
any derived-history reads (B11); the host flag alone does not close this.
B2's engine port is costed in
[RUST_REMOTE_SIGNING.md](RUST_REMOTE_SIGNING.md) and undecided; hosted accounts
keep working without it.

tealstreet-next edits, shipped with the cutover (listed in [tealstreet-next caller changes shipped at cutover](#tealstreet-next-caller-changes-shipped-at-cutover)):

1. Capability record: the 10 compile-time call sites and two test fixtures (#1400).
2. Warm-start skip for Rust accounts (B4).
3. `callMethodInWorkerFireAndForget.ts:3-7` also accepts `code === 'COMMAND_TIMEOUT'`.
4. Root `CLAUDE.md:662` names `isBaseCapabilityFallback` as the brand check; point it at `exchange.capabilities`.
5. Pass typed detail for ordinary errors (#1484); migrate coded diagnostics to `processEvent`/`processHealth` (#1583), forwarding `engine-event` and suppressing duplicate legacy delivery only by event metadata. See [Coded diagnostic callers](#coded-diagnostic-caller-cutover-pr5).
6. Preserve/read history completeness through the worker/hosted response and
   journal collection; incomplete windows never call `completeWindow` (B11).

Cutover item, `collectAccountAxes`: #1554 supplies an incomplete-history flag,
but its forwarding/coverage work is still open (B11). The exact transport and
ingest requirements are in [Incomplete history](#incomplete-history-1542-1554).

Owner item, one-off at cutover: `historical_orders` (created by `supabase/historical_data.sql`) already holds BloFin order rows that TS wrote in contract units. Rust writes base units, so the table would mix both. The fix is to **multiply** `amount`/`filled` by the instrument's `contractValue` (base = contracts × contractValue, e.g. 1000 × 0.001 = 1 BTC). Do not select rows by `ingested_at`: an upsert keeps the original timestamp, so a TS-era row that Rust later rewrites in base units would be converted twice. Run the backfill with BloFin order ingestion paused, before Rust writes any BloFin order row, or make it idempotent: convert only rows whose `filled` far exceeds the summed base `qty` of the matching `historical_fills` (TS fills were already base units). The owner runs it; there is no SQL in this repo.

Deployment order: exchange-server before web (B4 forwarding, `fetchEngineCapabilities`, then B11 completeness metadata). #1538 already emits venue events: deploy safe-cex-server with #1388's notice validator before connecting such engines to native sessions (an older validator closes the session on `type: "event"`).

Not blockers but required edits are listed per area below.

### Venue names on Rust

| Set | Names | Selection |
| --- | --- | --- |
| Rust by default (TS classes deleted) | `coinbase`, `coinbasespot`, `tealstreet-dex`, `kodiakfi`, `mode`, `salsadex`, `what`, `woofi`, `txflow` (testnet only) | `registry.ts`, `registry.native.ts`; no opt-in, every factory/worker/server/CLI/MCP path |
| Rust adapter, TS default | `apexomni`, `asterdex`, `backpack`, `binance`, `binancepm`, `binancespot`, `binanceinverse`, `bingx`, `bitget`, `bitgetusdc`, `bitgetinverse`, `bitrue`, `blofin`, `breakoutprop`, `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader`, `cryptocom`, `grvt`, `hyperliquid`, `hyperliquidspot`, `kraken`, `krakenspot`, `kucoin`, `kucoinspot`, `lighter`, `lighter-robinhood`, `mexc`, `okx`, `okxeu`, `okxdemo`, `okxeudemo`, `paradex`, `phemex`, `polymarket`, `weex`, `woo`, `bitunix` | `ExchangeOptions.engine: 'rust'` in `createExchange`, `createExchangeFromAccount` overrides, `ExchangeWorker`; MCP `EXCHANGE_ENGINE=rust`. No tealstreet-next caller sets it today. No `'typescript'` value (Owner decision 4) |
| TS only | `binanceoptions` (master-only, survives the merge) | |

`ExchangeName` keeps every name, so the exhaustive maps in
`apps/web/src/components/chart/web/hostedMarketProduct.ts:23` and
`components/newApiFlow/getNewApiFlowTutorialUrl.ts:56` still compile. The
webpack chunk names `exchange-coinbase`, `exchange-coinbasespot`,
`exchange-<orderly broker>`, `exchange-txflow` disappear (no tealstreet-next
reference). Venue lists outside the SDK that must be reconciled by hand:
`apps/admin/server/exchangeAssets.ts:57-130` (missing `binanceinverse`,
`binanceoptions`, `hyrotrader`, `mexc`, `txflow`), `packages/exchange-pretty-error-messages/src/prettifyErrorMessage.tsx:16-22`
(`bybit*` prefix excludes `hyrotrader`), `apps/extension/src/exchanges/registry.ts` (bybit only),
`packages/exchange-signer/src/index.ts:108-128` (no consumers).

### Contract changes by caller

#### Construction, registry and engine selection

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| 9 names default to `RustEngineExchange` | web `utils/exchange/createExchangeFromAccount.ts:6,57`, `preloadAccountExchanges.ts:3`, `signingHost.ts:23,88`; exchange-server `server.ts:1059` (`storeWebworkerStart`), `public-exchange-manager.ts:80`; CLI `apps/cli/src/platform-adapter.ts:62`, `record/recorders/orderbook.ts:43`; MCP `packages/exchange-mcp/src/server.ts:23`; `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.ts:142`; `mobile-services/candle-server/src/exchange-pool.ts:169` | TS venue class | Rust class; needs the engine loadable in that realm (B1) and, on main threads, a worker engine (B4) | RT |
| `ExchangeOptions.engine?: 'rust'` | none set; `createExchangeFromAccount` callers above | n/a | opt-in per account; `accountToExchangeOptions` does not map an engine field, so callers pass it as an override | add |
| `start()` single-use; startup failures reject (`StartupFailed`, #1383) | MCP `server.ts:13` (start-once, retried on failure on the same instance); CLI `platform-adapter.ts:247-261` (30 s race) | TS `start()` re-callable | a failed Rust start needs a new instance; MCP retry must recreate the exchange | RT |
| `validateAccount()` starts the engine; error text from engine startup | web `components/newApiFlow/useAccountForm.ts:210-275` (matches `'passphrase'`, `'api key'`, `'secret'`, `'whitelist'`, `'permission'`, `'secondary'`) | TS venue messages | engine messages (e.g. `<name> credentials were rejected`, `Orderly requires base58-encoded 32-byte key and secret`) | RT |
| `externalPublicData` websocket gateway rejected; in-process source accepted (B6); `privateOnly`+`publicFeed: direct` rejected | exchange-server `server.ts:1015-1034` | none for in-process | `publicFeed: { mode: 'shared', port }` + `RustPublicFeedProvider` for gateways | RT, B6 |
| Keyless `remoteSigner` account routes to TS | exchange-server `server.ts:1015`; web hosted signing | browser-signed | #1498/#1505 keep hosted TS and browser signing working (B2); a direct private engine construction is refused, with the documented exceptions | none through factory/worker; direct construction RT |
| `payloadSigner` used only by Paradex | MCP `packages/exchange-mcp/src/config.ts:15-22,59-72` (hmac proxy signer) | Bybit/Binance REST | #1510/#1531: `runsOnRustEngine` keeps Bybit/Binance-family `payloadSigner` accounts (not `publicDataOnly`) on TS, whose signers honour it; every other private non-Paradex Rust account with one, including default-Rust Coinbase/Orderly (their kept TS classes ignore it), is refused at startup with `ExchangeError` `kind: 'unsupported'`, `outcome: 'notSent'`. #1575 enforces this in the registry after WASM loading, before venue I/O or signer invocation | RT (MCP `EXCHANGE_ENGINE=rust` signer mode: Bybit/Binance run TS; other venues fail loudly) |
| Resolved polling policy (#1392) | MCP `config.ts:59-72` (`extra.tickInterval: Infinity` disables polling); web `exchangeAccountController` worker-config multipliers | TS `getTickInterval()` policy | all Rust periodic reconciliation now scales from the resolved interval, including overrides, server multipliers and auth backoff. `Infinity` pauses, finite updates resume; native rate floors/cooldowns remain. Startup, socket supervision and explicit reads continue | none; existing controls now apply to Rust |
| `instanceof BlofinExchange` | web `components/reactiveStateHelper/exchangeReactiveStateHelper/BlofinExchangeReactiveStateHelper.tsx:6,20` | true for blofin | false when blofin runs Rust | RT (on blofin switch) |
| `RUST_VENUES` becomes `ReadonlySet` (#1400) | none | | | none |
| `Exchange` gains `fetchEngineSnapshot`; #1400 adds `capabilities`/`loadCapabilities` | no `implements Exchange` downstream; only `as Exchange` casts (`apps/cli/src/repl.ts:713,1517-1925`, `cross-trade-daemon.ts:222`, web tests) | | casts still compile | none |
| #1476/#1519/#1520 construction and clock policy | factory/worker creation and MCP config callers above | TS local-credential check, per-venue options/clock name lists | shared Rust validation, uniform options and engine `hostClock`; no caller edit. Missing secondary-secret errors are now specific; #1531 still narrows payload-signer fallback | RT validation text; same signing clock |
| Construction metadata/public mode (#1563/#1567) | saved-account/factory/worker callers above; web `components/customModules/monacoTypesString.ts` already declares the Aster override and `publicDataOnly` | synchronous venue branches and derived public-mode hint | shared rules preserve synchronous getters; after WASM loading they adopt native `canSign`/`publicDataOnly` when present, while Aster STP reads native `asterdexApiVersion`. `WasmEngineLike.construction()` remains optional; missing fields use TS fallback. See the input/compatibility boundaries below | none for application callers; additive host API |

#### Construction metadata and public mode (#1563, #1567)

`packages/safe-cex/src/exchanges/rust/construction-rules.json` is now the single
rules table for signing and private-data eligibility. TS `derivePublicMode`
evaluates it synchronously before WASM loads. Since #1599, the Rust registry's
`build.rs` reads and validates the same JSON during Cargo builds and generates
a static table in `OUT_DIR`; the engine evaluates that table before credential
validation/startup, without runtime JSON decoding. Source delivery/builds (B1)
must retain the JSON at `packages/safe-cex/src/exchanges/rust/construction-rules.json`
relative to the repository root; compiled engines contain the generated table
and do not need the JSON at runtime.
The compiled parity matrix covers every registered Rust name across 69 input
combinations, including signer kinds, public intent and venue-specific rules.
This preserves existing options and eligibility; it does not add remote signing
to an engine or establish that credentials are valid.

| Host boundary | Landed contract and compatibility |
| --- | --- |
| SDK bridge → native `EngineOptions` | Optional `publicMode: { requested: boolean \| null, remoteSigner: boolean, payloadSigner: boolean, apiKeyIndexPresent: boolean }` carries raw intent/presence beside the existing `publicDataOnly` hint. The SDK sends it; application callers keep using `ExchangeOptions.publicDataOnly`, credentials and signer options. With `publicMode`, Rust derives public mode and sets `accountStartup` from it rather than trusting a conflicting derived hint. |
| `WasmEngineLike.construction?(): string` | JSON `EngineConstruction` now has optional `canSign`, `publicDataOnly` and `asterdexApiVersion` fields. The wrapper adopts available native fields after loading and uses shared synchronous TS rules for missing fields/older hosts. This metadata is separate from frame/engine capabilities and contains no credentials. Without `publicMode`, native/harness construction keeps the legacy `publicDataOnly`/`accountStartup` interpretation and omits the two new metadata fields; non-Aster metadata can still be `{}`. |
| WASM module `resolveConstruction(venue, optionsJson): string` | New named export returns the same metadata JSON without constructing an engine or performing I/O. `WasmEngineModule` declares it optional, so existing custom loaders need no new method. Resolving metadata is not credential validation or successful startup; the production wrapper still has synchronous TS getters before async loading. |
| Explicit-private Paradex startup | `publicDataOnly: false` without key, Paradex address or subkey address still rejects `start()` with `Rust Paradex private accounts require a key, Paradex address and subkey address`. The registry now enforces it after WASM loading and before venue I/O, instead of the bridge refusing before loading. Public/automatic mode and the existing private-field requirements stay as before. |

#### Host construction rules (#1575)

The construction scope in [RUST_VENUE_CONSTRUCTION.md](RUST_VENUE_CONSTRUCTION.md)
is now landed through #1563/#1567/#1575. B2's factory routing for keyless remote
signing and supported payload signers still applies; the engine signing port
remains an owner item. Paradex signing transport and polling are unchanged.

| Host boundary | Landed contract and compatibility |
| --- | --- |
| SDK bridge → HyroTrader registry | Optional `EngineOptions.hyrotrader` carries raw `mode` plus `production`/`simulator` maps, each with `rest`, `publicWs`, `privateWs` and their `livenet`/`testnet` URLs. Rust selects simulator for `mode: 'demo'` or an empty production REST livenet URL (even for testnet accounts), then selects the requested network. All three selected URLs must be configured or construction rejects with `HyroTrader REST/public/private URLs are not configured`. Raw config takes precedence over explicit `endpoints`; omitting it preserves legacy native/harness explicit endpoints. Application callers keep `extra.hyrotraderMode` and `testnet`; this is no new live-connectivity evidence. |
| Native private `payloadSigner` refusal | `payloadSigner` is parsed across features. After public-mode derivation, private non-Paradex construction refuses it as `unsupported`/`notSent`, including with a local secret, before HTTP/WS I/O or a signing callback. Public accounts bypass the refusal, including the existing Aster/Lighter payload-only public fallback. Paradex delegated signing and the Bybit/Binance TS factory exceptions remain. |
| Raw WASM constructor → SDK startup | Registry construction failures throw a JS `Error` with JSON-encoded `venueError` detail. The SDK wraps valid detail in `ExchangeError`, preserving message/classification; `validateAccount()` still reports `kind: 'unsupported'` for the payload-signer refusal. Errors without usable detail retain their original form for older/custom loaders; invalid options JSON remains a plain constructor error. No constructor arguments or application call shapes change. |

#### Capability detection

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| #1400 (merged) un-exports `isBaseCapabilityFallback`, `isInertCapability`, `isEmptyStubCapability` | web `components/openPositions/ClosedPositions.tsx:11,114`, `utils/historical/collectAccountAxes.ts:5,162,164`; `packages/venue-data/src/journal/symbolScopedFills.ts:1,74-76` (+ fixtures `symbolScopedFills.test.ts:94-99`, `symbolScopedFills.bybit.test.ts:35,47`); `packages/venue-data/.../axis-grading.registry.test.ts:9,89`; `scripts/history-audit/lib/axis-inventory.ts:3,72,74` (call `:128`), `closedPnl.ts:32,234`, `venue-survey.ts:4,86,107,165` | `isBaseCapabilityFallback(exchange.fetchX, 'fetchX')` | `exchange.capabilities.{fillHistory,orderHistory,incomeHistory,incomeRange,closedPnl,…}`; on never-started instances `await exchange.loadCapabilities()` first. Rust mirrors adopt the worker's record from the snapshot frame's `CAPABILITIES` section (#1411) | CT |
| MCP capability gating | `packages/exchange-mcp/src/tools.ts:2,15,363` | `!isInertCapability(fn)` | already rewritten in the mirrored package by #1400; arrives through Copybara, not a hand edit | none |
| Coinbase/Orderly fills and order history now served by the engine | web `collectAccountAxes.ts:162-241`, MCP `tools.ts:168,175` | branded empty (TS) | real data; Orderly/Coinbase/TxFlow history rejects `orderId`/`status` filters | RT (additive) |

#### Methods and properties

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| `fetchClosedPnl` | web `ClosedPositions.tsx:93-121`; MCP `tools.ts:182` | TS (blofin, bybit, bybitinverse, okx, bitunix) | engines with the `closedPositions` capability (bybit, bybitdemo, hyrotrader, bybitinverse, okx names, blofin) serve `fetchClosedPositions` rows, mapped by the TS mappers; others report `capabilities.closedPnl` false, as TS (Rust bitunix reports it false; its closed PnL stays on the TS venue) | none (handled) |
| `fetchHistoricalPositions` | web `collectAccountAxes.ts:229` (`!` asserted) | TS bitget/blofin/bybit/hyperliquid/okx/paradex | as TS: bybit/bybitdemo/hyrotrader fold reported rows with the order-axis leg join, okx (linear) and blofin map reported rows, bitget* fold account-wide fills, hyperliquid/paradex fold per-symbol fills; venues TS leaves undeclared (e.g. bybitinverse) keep the base refusal | none (handled) |
| Incomplete income results (#1542/#1554) | web `collectAccountAxes.ts` income symbol enumeration, `runHistoricalWindow.ts` coverage | successful arrays treated as complete | inspect/preserve `incomplete: true` before enumeration, folding or spreading, and leave affected coverage uncompleted (B11) | RT, B11 |
| Binance native stop-limit `updateOrder` (#1551) | web order amendment; SDK chaser amend path | re-place could change the order into stop-market | reject as `unsupported`/`notSent` before any write; do not hide the refusal or retry with a different order type | RT |
| `updateSymbolMetadata` no-op on weex | web via `callMethodInWorker` `hooks/margin/usePositionMode.ts:62`, `useMarginType.ts:62`, `useLeverageSlider.ts:235`, `hooks/market/useSymbolMetadataUpdate.ts:14` | TS weex | silent no-op | RT |
| `recoverAfterOffline` closes open engine sockets for the venue to reconnect; `false` only after disposal or engine death | web `ConnectionStateWorker.tsx:364`, `utils/desktop/desktopRecovery.ts:82` (reload on `false`) | per-socket recovery | per-channel recovery; dead engines reload | none (B5 fixed) |
| `getAllSockets`/`publicWebsocket`/`privateWebsocket`/`businessWebsocket` are per-channel engine socket handles; `authFailureStreak` counts `Deactivate` and `auth.rejected` | CLI `ConnectionCommand.ts:182-196`, `ConnectionMonitor.ts:128-132`, `legacyCommands.ts:28`, `repl.ts:2831,2903`; web `order/createLimitOrder.ts:221-222` (`privateWebsocket.isReadonlyFallbackActive()` is `false`) | TS sockets | handles expose `lastReceived`, health, `reconnect`, `recoverAfterOffline`; no `ws`, no per-topic stats | none (B5 fixed) |
| `fetchMarketsAndTickers` (not on base), `fetchTickers`, `fetchPositions` via `as any` | web `exchangeAccountController.ts:668-669,687,750,863`, `components/customModules/frame/feedSources.ts:82-83`, `eagerExchangeWarmStart.ts:52,58` | REST reads | `fetchMarketsAndTickers` undefined on Rust; `fetchMarkets`/`fetchTickers` start the engine and wait ≤30 s for native data, else throw `Timed out waiting for native markets|tickers` (#1375). #1430 makes `fetchPositions` wait for its native sections and synchronize engine state; see below | RT |
| Native account reads (#1430): `fetchOrders`, `fetchPositions`, `fetchBalance`, `fetchPositionMode` | MCP `tools.ts:62,82-83,144,152,160,322`; web warm-start `eagerExchangeWarmStart.ts:75,86` | cached JS store or venue REST | await native section readiness and synchronize current engine state; main-thread mirrors forward to their worker. No forced network refresh. Missing public-only private sections reject visibly | RT; same arguments/results |
| Native identity `getAccount` (#1430) | no application exchange-identity calls found in web/CLI/MCP/server at `fdd2389a825` (CLI adapter `getAccount` is a different API) | limited native lookup support | query/after-load behavior follows `capabilities.account`; unsupported venues retain an explicit refusal. See identity boundary below | additive; no app edit found |
| `fetchLeverageForSymbol` (#1522) | mirrored worker `UiStateHandler.ts:27-64`; web `hooks/margin/useMarginType.ts:27`, `usePositionMode.ts:27` consume `store.fetchedLeverage` | BingX/WEEX settings-only reads and uncached metadata rails could return `null`; WEEX native command had no handler | BingX/WEEX read fresh symbol rails; MEXC/WOO/Kraken `onMetadata` reads query only when no usable cached rails exist. UI still fetches only for a missing active-symbol entry. Failure keeps the SDK warning/`null` result contract | additive data; no app edit |
| Native `nuke`/`nukePositions` (#1430) | web `hooks/trading/useNuke.ts:7`; mirrored CLI `commands/trading/NukeCommand.ts:32,42,51,60` | inherited store-based closes/retries | one native-state batch of reduce-only market closes; no automatic replay even if callers pass `3`. Observe/catch the promise and reconcile partial/unknown outcomes; CLI's max-retries text must not imply a guarantee | RT; caller error/retry handling |
| `changeAccountTier`, `fetchAccountTier` (#1386/#1394) | web `components/settings/profile/PerExchangeSettingsBody.tsx:46-62,74-130` | TS LighterExchange | Rust provides both; `balance.info.lighterAccountLimits = { user_tier_name }` synced; the main-thread mirror forwards them (#1462) | none (B4) |
| `fetchPublicTrades` | web `components/chart/datafeed/DefaultDatafeed.ts:831-836` (binance only) | TS | engine command; main-thread mirror forwards it (#1462) | RT (B4) |
| Live fill identity/financials (#1430/#1592) | web `exchangeAccountController.ts:2315-2355`, `fillOrderToast.tsx`, `FillsModule.tsx:141`, `HistoricalIngestionWorker.tsx:297`, `ChartTradingController.ts:239-245`; CLI `repl.ts:941-949`; script-editor `monacoTypesString.ts:614-618` | Rust IDs identified the order; #1430 added optional financials | `{ id, orderId, symbol, side?, price, amount, realizedPnl?, fee? }`; only reported finite values appear, including zero. Since #1592, `id` is the reported execution identity or falls back to `orderId`; `orderId` remains the order identity. Existing script-editor types already declare the optional fields | RT for order-ID assumptions; matching engine/reader/applier required |
| `options.enableSpotMarkets` on hyperliquid | web `ChartContainer.tsx:161`, `useChartSession.ts:303` | spot markets | the Rust perp engine merges spot markets, holdings and orders into the account, as the TS venue; best-effort spot discovery retries on the discovery backoff without holding back perps | none (handled) |
| `store.balance.info` raw venue payload | web `Margin.tsx:85-110,136,358,413,566`, `isBybitInverseUnifiedMargin.ts:25`, `placeOrderStoreData.ts:107`, `bybit-skin/.../AccountInfo.tsx:32-45,97`, `MarginHeader.tsx:56` | Bybit `accountType`, multi-collateral array, Lighter limits | Lighter tier bridged; Bybit (all five names) syncs the engine's last REST wallet account verbatim (`balanceInfo` capability, `fetchBalanceInfo` every 2 s), replacing `info` as TS does; frame-forwarding workers send `info` changes to main | none (handled) |
| `Market.info` raw payload | cli-core `order/prepareCliOrder.ts:44`, `commands/session/CheckCommand.ts:14` (`classifyMarket`); MCP strips `info` (`tools.ts:113`) | raw venue JSON | typed exceptions only (Bybit class/region/underlying, Polymarket, OKX) | RT |
| `Market.info.hlTokenName`/`hlTokenFullName` (Hyperliquid spot) | web `components/margin/Margin.tsx:783-789` (Underlying Token risk disclosure) | TS merged spot markets | typed `UNDERLYING_TOKENS` frame section (token name, full name) applied as the same `info` fields, so the disclosure renders unchanged | none (handled) |
| `store` writes from the app | web `eagerExchangeWarmStart.ts:20-107`, `exchangeAccountController.ts:691-884,2573`, `feedSources.ts:98`; `packages/safe-cex-server/src/publicExchange/hydrateStore.ts:6-13`; candle-server `exchange-pool.ts` | TS store | frames overwrite markets/tickers/orders/positions/balance on the next engine frame; app writes are transient | RT |

#### Native account reads, identities and fills (#1430)

| Boundary | Landed behavior and caller requirement |
| --- | --- |
| Current account state | `fetchOrders`/`fetchPositions`/`fetchBalance`/`fetchPositionMode` start or reuse the engine, wait up to 30 s for markets plus the relevant private section, then call `syncAccountState()`. Missing private sections on public-only accounts reject with `Native <label> is unavailable for a public-data-only account`; timeout/startup/disposal failures also reject. Reads use current native state, without forcing HTTP reconciliation. Mirrors forward these async reads to the worker; their synchronous `syncAccountState()` is a no-op. B4's warm-start edit remains necessary because `fetchMarkets` still starts a local engine. |
| Decisions and freshness | `syncAccountState(): void` is additive on `Exchange` (Base is a no-op); in-process Rust requires a started engine and ready native market catalog. CLI conditions/substitution/loops and chaser creation synchronize before store-based decisions. Generic cancel-all selects native open-order symbols; set-all leverage uses native positions and hedge-leg sides. The zero-length `ACCOUNT_READ` section 21 marks private snapshots so both consumers update rows without advancing orders/positions freshness IDs. Reads preserve pending streaming changes, fills and public baselines; they do not establish a fresh venue observation. Ship matching engine, reader and applier. |
| Account identity | `getAccount()` uses native `fetchAccount` for `capabilities.account: 'query'`; `'afterLoad'` still waits for startup, and `'unsupported'` refuses. #1430 expands query support across supported aliases of Backpack, Binance USD-M/PM, BingX, Bitget, Bybit (excluding HyroTrader), Crypto.com, GRVT, KuCoin, Lighter/Robinhood, MEXC, Paradex, Phemex, WEEX and WOO. Lighter lookup can be unsigned; MEXC/WEEX validate selected read credentials and namespace identity from the primary key. These identities do not refresh financial state or establish new signing support. |
| Bulk close and cancel | `nukePositions` reads native positions, sizes closes in venue order units (inverse notional where required), preserves hedge legs and submits valid reduce-only market closes in one `placeOrders` batch. Unusable rows are skipped; if the valid-close batch resolves, skipped rows cause rejection, while a batch failure propagates its own error. `nuke` also attempts cancel-all after close failure, then rethrows the original close failure. Concurrent nukes reject; legacy `tries` arguments do not trigger retries. Web `useNuke` must handle the returned rejection; CLI prints progress and catches warnings asynchronously, so its immediate success is not completion evidence. Partial/unknown outcomes require reconciliation before any retry. |

#1522's leverage reads use native signed symbol queries: BingX maps long/short
rails to `buyLeverage`/`sellLeverage`; WEEX returns rails plus `marginType` from
the symbol configuration. BingX fences stale store publication against
configuration/settings changes and newer same-symbol queries. Superseded reads
and reads issued during a pending settings write still return current cached
buy/sell rails, falling back independently to validated read values for a
missing rail; overlap alone does not fail the read or cause a `null` result.
WEEX fences store publication against settings writes. Native read failures
still leave settings intact. `onMetadata` serves finite positive cached buy/sell rails, otherwise
issues native `fetchLeverage` (Kraken spot still returns `null`); `settings` stays cache-only. Ship matching engine and
capability reader so BingX/WEEX retain both account lookup and on-read leverage.

#### Fill identity and wire compatibility (#1592)

#1430 introduced optional financial tails under revision 1. #1592 now always
writes **FILLS revision 2**: the existing order/market/side/price/amount prefix
and raw-symbol refs are followed by execution-ID refs (`NONE` when absent),
then aligned f64 `realizedPnl[n]` and `fee[n]` columns (`NaN` when unreported).
The updated reader accepts revisions 0/1, including revision-1 frames without
financials and with #1430's financial tails; revision 2 is accepted only for
FILLS. Missing/nonfinite financial values become absent SDK fields; reported
zeroes survive.

`FillRows.fillId?(i)` exposes the optional native execution identity. The SDK
emits `id = fillId ?? orderId`, retaining `orderId` separately. Hyperliquid perp,
merged spot and spot-only fills populate `id` with the decimal `tid` string;
other native venues leave execution identity absent and retain order-ID fallback.
Use `orderId` for order attribution; do not assume every venue's `id` identifies
a unique execution. Live fills remain events rather than persisted store rows.

**Build compatibility prerequisite:** old readers reject FILLS revision 2.
Ship matching Rust/WASM producers, JS readers and SDK appliers, including both
worker and main-thread consumers when forwarding frames. A new reader can read
legacy producers; updating only the producer breaks older consumers. This
requirement is separate from B1's choice of engine delivery mechanism.

Focused NX checks at `65e086a3c90`: web optimistic-order retirement and CLI
`recordFill` already prefer `orderId ?? id`; fill toast coalescing also prefers
`orderId`. FillsModule uses live fills only as a symbol-based database refresh
hint, and HistoricalIngestionWorker discards the payload and marks the account
dirty. No edits to those handlers are required for the new identity. Script
consumers assuming `id === orderId` must instead use the separate `orderId`.

Replay now captures native `executionId` separately from order ID and captures
finite fee. #1592's green Hyperliquid/HyperliquidSpot CI replay gates prove the
four removed identity-only exception groups (37 paths); unrelated declarations
remain. This does not claim identity parity for every venue. See the
[fill execution identity plan](RUST_FILL_EXECUTION_ID.md).

Live financial conventions remain venue-specific: Hyperliquid close fills use
net PnL (`closedPnl - fee`, requiring both values) and the reported fee; opening
fills omit both. MEXC/WEEX preserve reported realized PnL and absolute fees.
Synthetic/cumulative-order fills invent no financial values. Historical
execution PnL retains its separate gross convention; this does not resolve
B11 completeness forwarding/coverage or owner-run backfills.
Order mapping also retains generic `Order.triggerType` alongside the existing
TP/SL-specific trigger fields; no caller signature changes.

#### Private snapshot row isolation (#1524)

The third snapshot batch covers Backpack, Bitrue, BloFin, Crypto.com, GRVT,
Kraken futures/spot, KuCoin futures/spot, MEXC, OKX and WOO. Migrated private
list reads accept valid peers when another row cannot be mapped. A rejection
with a recoverable identity retains that identity's stored copy; a rejection
without an identity prevents absence-based removal of stored rows in that
snapshot's scope. Accepted rows can still replace their matching stored rows.
A fully valid empty snapshot still clears its scope. Structural failures,
catalogue safety holds and existing revision/terminal fences remain in effect;
this is not permission to commit every malformed response. KuCoin futures
still discards the whole order snapshot for a missing-ID order row; its
missing-ID position rows instead protect stored positions.

Callers can observe a mixture of newly mapped and retained older rows. A store
update or resolved account read is not proof that every row was freshly decoded.
Do not infer a close/cancel or zero balance merely from a skipped row. Balance
aggregates keep venue-specific safety rules: KuCoin spot, for example, retains
its previous USD wallet total when rejected currency rows would mix old and new
values. No new per-row freshness flag, method argument or frame revision is
introduced by #1524.

Bitrue malformed order trees protect named children as well as the parent;
when a child lacks an ID, cached children of that parent are protected too.
Kraken futures/spot isolate malformed open-order rows instead of rejecting valid
peers. Kraken spot also skips a malformed execution update alone, without
emitting a fill for it or changing its order.

`ROW_SKIPPED` / `SNAPSHOT_DISCARDED` are the existing wire codes `row.skipped` /
`snapshot.discarded`, delivered through the coded diagnostic/health paths above.
A row skip does not mean the whole snapshot was discarded, and a discard is
not evidence that the account is empty. Repeated skip diagnostics may be
suppressed until recovery; emitted levels vary by venue/path (several use Warn
when retaining a stored copy and Error when none can be kept). Use code and
level, not message matching or an assumed one-event-per-row count. The B9
caller migration and snapshot collection requirements remain open.

#### Events and worker protocol

| Change | tealstreet-next file:line | Old usage | New usage | Impact |
| --- | --- | --- | --- | --- |
| Engine error logs → `'error'` payload `Exchange Error: <message>`; exactly one `'error'` per failed command (#1375, landed) | web `exchangeAccountController.ts:1986-2055` (`(err: string, opts)`); CLI `repl.ts:958-1025` (`args[0] as string`); candle-server `exchange-pool.ts:224-230` | venue `emitAndLogError` strings | prefixed strings; one toast per rejection after #1375 | RT |
| New events `command-progress`, `engine-dead`, worker op `command-progress` (#1375) | web dispatcher `exchangeAccountController.ts:2425-2800` (unknown ops ignored) | n/a | additive; surface `engine-dead` to the user | add |
| Worker frame mode (`__workerConfig { acceptsFrames: true }` → `op: 'frame'`, store payload slimmed, no `fill` op) | web `utils/shim/worker/workerFactories.ts:386` (does not set it); exchange-server VWC `virtual-worker-context.ts:247-320` | full `store` payloads | unchanged unless opted in; opting in requires `exchange.applyWorkerFrame(buffer)` on the main thread | none until opted in |
| `callMethodInWorker` timeout (#1375, landed): Rust = `rustCommandTimeoutMs + 1 s`, error `Rust worker command timed out; reconcile interrupted writes before retrying` with `code: 'COMMAND_TIMEOUT'`, `outcome: 'unknown'` | web `utils/exchange/callMethodInWorkerFireAndForget.ts:3-7` (`error.message === 'Timeout callMethodInWorker'`) | 5 s, exact message | match `code === 'COMMAND_TIMEOUT'` as well | RT |
| Partial-batch fields across the worker | web `order/createScaledOrder.ts:64-71` (`partialOrderCount`, `partialOrderIds`) | TS venue errors | preserved across the worker by `serializeCommandError` (#1375, landed) | none |
| Packed ticker delta adds `midPrice` | exchange-server `public-virtual-worker-context.ts:4,200-235` (imports `PACKED_TICKER_FIELDS`) | | follows the imported constant | none |
| `exec-result`/`updatePositions:done` gain `errorDetails` (#1375) | web CLI `components/cli/CLI.tsx:306`, `cliChainRunner.ts:454-479`, `runCliLine.ts:77-78` | `{reqId, success, error}` | additive | add |
| Native/DO diagnostic protocol (#1388/#1586) | no current NX `NativeFrameSession` consumer found; native/DO host adopters | native `event` plus legacy `log`; DO `op: 'log'` with optional code/fields | #1586 diagnostics use native `type: 'event'` / DO `op: 'event'`, levels debug/info/warn/error. Deploy compatible safe-cex-server before native Info+ events; upgrade DO clients before host adoption. #1554's command-correlated `history.incomplete` still needs ID translation when a server remaps commands | host compatibility/deployment prerequisite; Sam's call |
| #1527 fill/store frame ordering | web fill listeners, provisional positions and SDK chasers | fills could flush before already-pending orders by up to 250 ms | a fill-bearing flush carries pending store sections; existing appliers publish them before fill events. Venue execution-before-order delivery can still occur | none; no new FILLS revision |
| Coded diagnostics (#1583) | web worker dispatcher `exchangeAccountController.ts:2715-2727`; web/CLI health/error listeners in the [caller inventory](#coded-diagnostic-caller-cutover-pr5) | only legacy `error`/`log` paths handled | add worker `op: 'engine-event', data: [event]` dispatch and code-aware listeners; legacy Error `opts.engineEvent` / Info-Warn third `log` argument identifies duplicates. Debug has only structured delivery when received | additive SDK/worker API; caller migration required |

#### Exchange-server hosting

The server runs `store.webworker` in-process on a fake Worker
(`services/exchange-server/src/server.ts:1059`, `public-exchange-manager.ts:80`);
the op protocol is its ABI. Beyond B2, B6, B7:

| Item | tealstreet-next file:line | Requirement |
| --- | --- | --- |
| Signer error text parsed by `unknownSendOutcome.ts` | `remote-signer.ts:580,592,602,659,664,714,761` | keep byte-identical, or move `NOT_SENT_MESSAGE` and `alerts.ts` to codes together |
| Unhandled engine promise rejections | `alerts.ts:62,81-94` (`uncaught-exception`/`unhandled-rejection` routing) | engine promises must not reject unhandled, or alert routing changes |
| `callMethod` passthrough of any method name | `server.ts:1273-1344` | every hosted method name the web sends must exist on the Rust class |
| `exchangeWorkers[key].exchange` live access | `shared-public-data-source.ts:70-131` | `.store.subscribe`, `.store.{tickers,markets,loaded}`, `.listenOrderBook`, `.listenOHLCV` on the Rust object |
| Redis market/ticker seeds into `init` | `public-exchange-manager.ts:240`; `packages/safe-cex-server/src/publicExchange/{serialization.ts:95-130,tickerCache.ts:53-93}` | `Market[]`/`Ticker[]` JSON shape unchanged; Rust frames replace seeded store data after start |
| 512 MB worker memory guard | `Dockerfile.connect-exchange-cloudflare` `TEALSTREET_WORKER_MEMORY_MB=512` | wasm linear memory counts against it; one engine per account plus one per shared venue |
| Wasm stack frames | `packages/service-telemetry/src/capture.ts:97-120`, `alertKey.ts:22-26` | only JS sourcemaps ship; stable error `name`/`message` needed for alert keys |

#### CLI and MCP

| Item | tealstreet-next file:line | Requirement |
| --- | --- | --- |
| Console output is part of the CLI result | `apps/cli/src/oneShotResult.ts:41-76`, `repl.ts:2864-2919` (`setConsoleTap`; `KEY_REJECTION`; any exchange `error` line ⇒ `UNCONFIRMED`) | Rust errors must still log to `console.error`/`warn` with matching text |
| Log classification | `apps/cli/src/log-classify.ts:30-57` | engine log lines keep the critical keywords |
| MCP stdout is the protocol stream | `packages/exchange-mcp/src/stdio.ts` | wasm init/engine must not write stdout |
| Writable generic emitter | cli-core `worker-host/WorkerPlatformAdapter.ts:76`, `router.ts:38-107` (emit `exec-log`/`exec-result`) | `ExchangeEmitter` accepts arbitrary events (it does) |
| Global teardown | `repl.ts:2940-2948` (`virtualClock.stop()`, `disposeHOWWebSockets()`), MCP `index.ts:15` | `dispose()` must release engine timers/sockets so Bun/Node exit |
| Chaser/TWAP/cross-trade managers driven against the exchange | cli-core `ChaseCommand.ts:340-350`, `TwapCommand.ts`, `apps/cli/src/cross-trade-daemon.ts:221-224` | `on('update'|'fill')`, `mintClientOrderId`, `findOrderByClientOrderId`, `store.*BySymbol` on Rust (present) |

### OKX WebSocket port operations prerequisite (#1588)

**Open ops work, before 2026-10-31.** [OKX's port-retirement notice](https://www.okx.com/docs-v5/log_en/)
states public-internet WebSocket connections to 8443 stop on that date; 443
already works, with the same hosts and paths. #1588 removes `:8443` from SDK
global/EEA live/demo URLs; relay metadata covers only global live private/business
upstreams. This merge does not migrate deployed proxy upstreams or allowlists.
The overseer identified these deployment prerequisites on 2026-10-21; repo
[OKX routing notes](../../packages/safe-cex/src/exchanges/okx/CLAUDE.md) describe
the global/EEA tunnels and Caddy passthrough, while the
[routing table](../../packages/safe-cex/src/transport/tealstreet/routing/routingTable.ts)
pins the global relay targets and host+path allowlist requirement.

| Ops dependency | Required action / owner boundary |
| --- | --- |
| Python `ws-proxy` global/EEA termination | Move global `okx/private` / `okx/business` upstreams on `ws.okx.com:8443` and EEA `okxeu/private` / `okxeu/business` upstreams on `wseea.okx.com:8443` to default 443, preserving each entity host and channel path. Changing only client URLs does not update these server-side destinations. |
| `okx-proxy-ja.tealstreet.io` Caddy passthrough | Move its deployed upstream destination from 8443 to 443 before the deadline. Its client-facing 443 listener is not proof that its OKX upstream already uses 443. |
| `tradetunnel` relay allowlist | Ensure deployed `RELAY_ALLOWED_UPSTREAMS` host+path-prefix rules accept the port-less global-live `wss://ws.okx.com/ws/v5/private` and `/ws/v5/business` targets from relay metadata, retaining the intended host/path match. |
| Live master / hot patch | The overseer reports live master still uses 8443; checked mirror `origin/master` `1f91dfe4dcb` also retains those URLs. Sam decides the production hot patch and rollout timing; the Rust landing merge is not a deployed fix. |

Keep this prerequisite separate from B1's engine delivery and the Rust cutover:
the port deadline also affects the live TS path. No proxy edit, hot patch or
deployment is performed by this documentation refresh.

### Error strings that must move to codes

All of these match message **text**; Rust venue messages differ from the TS
venue messages they replace. Codes available on landing: `ExchangeError`
(#1424) on rejected **command promises**, with `kind` (`rejected`,
`invalidInput`, `authInvalid`, `authExpired`, `authPermission`, `rateLimited`,
`network`, `timeout`, `venueUnavailable`, `unsupported`, `internal`), `outcome`
(`notSent`, `rejected`, `unknown`) and `venueCode`, for all 26 classified venue families
in B9; host `error.code` (`COMMAND_TIMEOUT`, `COMMAND_DEADLINE_FAILED`,
`ENGINE_PROTOCOL_ERROR`, `ENGINE_DISPOSED`, `ENGINE_DEAD`, #1375). Through the
web worker the error is a plain `Error` carrying those fields, so match
`(err as { kind?: string }).kind`, never `instanceof ExchangeError`. The
`'error'` event carries `opts.classified` (#1484), worker Sentry reports carry
`error_kind`-family tags (#1483), `start()` rejects with an `ExchangeError` and `validateAccount`
returns `kind` since #1482. #1388's event codes (`ws.reconnect`, `auth.rejected`, …) are
counted in `getHealth()`. Since #1583 they also reach `exchange.on('engine-event', …)`
and the worker's `engine-event` op; legacy Error uses `opts.engineEvent`, and
Info/Warn supplies the event as the third `log` argument. These are diagnostic
metadata, separate from classified command failures. Error telemetry carries
scalar `event_code` and the event object in extra data; explicit tags still win.

| Site | Matched text | Status | Target |
| --- | --- | --- | --- |
| SDK `health/errorPatterns.ts` via web `exchangeAccountController.ts:1773,1831,356-394` and CLI `apps/cli/src/exchange-manager.ts:203`, `repl.ts:958-1025`, `error-classifier.ts:18-76` | ~48 regexes (auth/rate/network/time-sync/order) incl. `Bybit API key environment mismatch` | **Movable** (#1484): pass `opts.classified` from the `'error'` handler (`:1986-2019`, `repl.ts:959`) | `processEvent(event, classified?)` for coded diagnostics and `processHealth(exchange.getHealth())` for lifetime counters (#1583); `processMessage(message, isError, classified)` for ordinary command/legacy TS errors. Do not classify a diagnostic twice via its structured and metadata-bearing legacy paths. The exhaustive switch `:356-394` is CT if categories change |
| web `exchangeAccountController.ts:245-246,338-340` | `/error sign/i` (bybit*); `builder fee has not been approved` (hyperliquid*) | **Movable**: `opts.classified` on the `'error'` event (#1484); bybit (#1479) and hyperliquid (#1470) are classified | `opts.classified.kind === 'authInvalid'` / `venueCode`, keeping the text for TS venues |
| `packages/exchange-pretty-error-messages/src/bybit/bybitHandler.ts:12,21-33` | `/ab not enough for new leverage/i`, `EC_*` reject reasons, `/error sign/i` | **Movable** for command rejections: bybit trading is classified (#1479) | map from `venueCode`; the `'error'`-event toasts read `opts.classified.venueCode` (#1484) |
| web `utils/exchange/callMethodInWorkerFireAndForget.ts:3-7` | `'Timeout callMethodInWorker'` | **Movable now** | `error.message === CALL_METHOD_IN_WORKER_TIMEOUT \|\| (error as { code?: string }).code === 'COMMAND_TIMEOUT'` (the Rust timeout message differs, `rust.exchange.ts:730-737`; TS venues have no code) |
| web `order/createLimitOrder.ts:361`, `createMarketOrder.ts:357` via SDK `utils/unknownSendOutcome.ts` | `UNKNOWN_OUTCOME_MESSAGE`/`NOT_SENT_MESSAGE` regexes | **Done in the SDK** (#1424): `isUnknownOutcome` checks `outcome` first (`unknownSendOutcome.ts:19-21`) and `outcome` survives the worker | no web edit; arrives with the safe-cex mirror. #1401 (merged) marks the Lighter, Coinbase, TxFlow, Orderly and Bybit unknown-outcome write sites |
| web `order/createLimitOrder.ts:367-372` | Lighter `below exchange minimums`, `min_base_amount`, `min_quote_amount` | **Movable** for command rejections: Lighter is classified (#1475) | `kind: 'invalidInput'`, keeping the text for TS venues |
| web `order/createScaledOrder.ts:642-651` | `reduce only`, `reduce-only`, `nothing to reduce`, `position does not exist`, `no position`, `position`+`close` | Partial: every Rust venue family is classified; TS-only venues still need the text | `err.kind === 'rejected' && err.venueCode === <venue code>`, keeping the text fallback for unclassified venues |
| web `components/newApiFlow/useAccountForm.ts:246-275` | `passphrase`, `api key`, `secret`, `whitelist`, `permission`, `secondary` | **Movable** (#1482): `validateAccount` returns `{ error, kind }` | branch on `result.kind` (`authInvalid`/`authPermission`), keeping the text for the field hint and TS venues |
| web `components/toasts/errorToast.tsx:30-32` | `Server was unavailable for a request`, `Cannot read properties of undefined` | Unchanged (app/TS strings) | n/a |
| web `utils/sentry/sentryErrorBlacklist.ts` | ~148 verbatim venue error regexes | **Movable** (#1483): worker reports carry `error_kind`/`error_category`/`error_outcome`/`venue_code` tags | filter Rust venues by tag; keep the regexes for TS venues |
| CLI `apps/cli/src/oneShotResult.ts:41-56` (`KEY_REJECTION`), `connect-error.ts:13-17`, `log-classify.ts:30-57` | key-rejection, `authentication`/`token`/`401`/`timed out`, critical log keywords | Partial: `code === 'COMMAND_TIMEOUT'` works for command timeouts; #1450 teaches `findKeyRejection` the engine wording | auth: startup `kind` (#1482); a runtime rejection raises `authFailureStreak` (#1450) and counts `auth.rejected` in `getHealth()` (#1388) |
| exchange-server `alerts.ts:82-89`, SDK `unknownSendOutcome.ts:10-13` | `Invalid API-key`, `Way too many requests`, `banned until`, signer messages | Blocked: process-handler reasons never see the error object | `kind: 'authInvalid'\|'rateLimited'` where the object is reachable; signer messages stay JS |

### Build and delivery steps

| Host | Today | Needed for the engine |
| --- | --- | --- |
| Web (Next.js, `apps/web`) | `next.config.mjs:330-336` `asyncWebAssembly`; safe-cex via tsconfig paths; only wasm is hand-copied `public/wasm/poseidon_crypto.wasm` | resolve `@tealstreet/safe-cex-wasm` (browser condition → per-family `pkg/web/<family>`); run `yarn rust:build-wasm --families` before `next build` (Vercel needs a Rust toolchain or prebuilt artifacts); tealstreet-next `sdk-rs/` path conflict (B1); worker realms (`StoreWorkerEntry.ts`, `ExchangeServerWorker.ts`, WorkerShim) load the engine |
| exchange-server (Bun) | `bun build --target bun` single file; `Dockerfile.connect-exchange-cloudflare` copies only `dist/*.js`; `scripts/cloudflare/deploy-edge.sh:45-47` has no Rust step | node/bun build of `@tealstreet/safe-cex-wasm/node`; inline or COPY the wasm; add the build to `deploy-edge.sh` and `.github/workflows/deploy-connect-data-edge.yml` (path filter lacks `sdk-rs`); budget memory |
| CLI (`apps/cli`) | `scripts/build.sh:60-98` `bun build --compile` ×5 targets, no asset embedding; `cli-release.yml` `bun-version: latest` | embed the wasm in the binary (Bun file import → `/$bunfs/`) and set the loader; pin Bun |
| MCP (`packages/exchange-mcp`) | tsup ESM bundle; `files` = `index.js`, `wasm/poseidon_crypto.wasm`, licences; release pack check `exchange-mcp-release.yml:134-149` | SDK side already adds `files: dist/wasm/rust` and `registerBundledRustEngine()`; update the release workflow's exact file list, licences script and smoke probe |
| candle-server (`mobile-services/candle-server`) | `createPublicExchange` per venue | engine for the 9 default names; image already has a Rust stage (`Dockerfile.data-candles-cloudflare`) |
| Mobile | see below | Hermes bridge or TS fallback (B3) |

### Mobile

tealstreet-mobile does not depend on safe-cex as a package: `yarn sync`
(`tools/sync`) copies tealstreet-next `packages/*` into the gitignored
`src/packages/` and rewrites `@tealstreet/X` to `@packages/X`. It never copies
`sdk-rs/`, so `@tealstreet/safe-cex-wasm` cannot reach the app.

| Realm | How exchanges are built | Cutover effect |
| --- | --- | --- |
| App thread (Hermes, Expo 54, RN 0.81, New Architecture) | synced web code (`createExchangeFromAccount` → `registry.native.ts`) constructs exchanges that are never started | Metro bundle fails once synced: `rust.exchange.ts` imports `@tealstreet/safe-cex-wasm/host`, `/wire` and dynamically `/engine`, absent from `src/packages`; Metro ignores package `exports` (`unstable_enablePackageExports=false`) and has no `.wasm` handling. `metro.config.js` already stubs paradex-sdk and poseidon-crypto for Hermes; a similar stub (or a sync change) unblocks the bundle. Hermes execution needs the native host (B3). |
| Hidden WebView | loads the web app's `/exchange-host` (`apps/web/src/app/exchange-host/page.tsx:53`), a web bundle on `registry.ts` with WebAssembly; its Worker owns the hosted socket and signs with `createExchange(...).signHttpRequest` (`signingHost.ts:88-191`) | #1498/#1505 preserve hosted TS signing, including browser delegates for default-Rust Coinbase/Orderly (B2). The WebView supports wasm, but this is not an on-device execution migration or a chosen B3 solution. |

No Rust venue runs on mobile today: all 9 default names have
`supportedOnMobile: false` and `isExchangeAvailableInCurrentEnv` filters them in
React Native. JS-only or WebView changes ship OTA (`expo-updates`, runtime
version per build, `check-ota-native-deps` guard); a Hermes native bridge needs
a native EAS build and a device acceptance gate.

[RUST_MOBILE_B3.md](RUST_MOBILE_B3.md) (#1546/#1556) inventories device-direct
order writes, validation and warm-start reads, and costs the B3 options. Its
recommendation to keep device TS through cutover is an owner decision still
to make. It separately records existing app issues: missing Hermes transport
registration for proxied accounts (Bitget/BloFin/Binance inverse construction),
Lighter's throwing poseidon stub on order writes, and plaintext AsyncStorage
venue secrets/sync keys. Those app fixes are outside this SDK docs lane;
neither the costed recommendation nor this inventory resolves them.

**candle-server** (`mobile-services/candle-server`, serves mobile charts):
builds `createPublicExchange(name, { forceAllProxiesOff: true })`
(`src/exchange-pool.ts:169`) without `start()`, then `fetchMarkets()` (`:265`,
falls back to duck-typed `fetchMarketsAndTickers()` on `'Not implemented'`,
`:267-283`), `store.update({ markets, loaded })` (`:309-312`) and `fetchOHLCV`.
Before #1375 a Rust `fetchMarkets` returned the empty store (`<name> returned an
empty market list`) and `fetchOHLCV` rejected `<name> engine is not started`, so
candle requests for the 9 default names failed (502). #1375 makes
`fetchMarkets`/`fetchTickers` start the engine and wait for native data, which
clears this on landing (#1375 merged); still needs the engine in the image. The smoke `KNOWN_FAILURES` lists 8
of the 9 names (not `txflow`). Its Dockerfiles ship only `dist` and nothing
calls `setWasmEngineLoader`; the default node loader needs the gitignored
`sdk-rs/hosts/wasm/pkg/node`.

### Audit method

1. SDK delta: three-dot `git diff origin/master...origin/feat/rust-rewrite --
   packages/safe-cex sdk-rs/hosts` plus `gh pr diff` for each open contract PR.
2. tealstreet-next callers: `git -C <tealstreet-next> fetch origin master`, then
   `git grep -n "safe-cex" origin/master -- ':!packages/safe-cex' ':!sdk-rs'`
   grouped by host (apps/web; apps/cli, cli-core, exchange-mcp, venue-data,
   script-api-core; services/exchange-server, mobile-services, safe-cex-server,
   other packages), then each changed SDK symbol/string by name.
3. Mobile: GitHub tree/code search of Tealstreet/tealstreet-mobile.

## Consumer gaps repaired by this readiness change

The following repairs preserve the existing consumer seam without changing
registry defaults or claiming broader product support:

- `packages/safe-cex/src/index.ts` now honors explicit `engine: 'rust'` for
  registered Rust aliases. Factory callers previously stayed on the TS class
  even when they explicitly selected Rust. Unknown/unported aliases retain
  the existing registry fallback, as `ExchangeWorker` does.
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` and
  `packages/safe-cex/src/exchanges/base.ts` now enable the native placement-unit
  bridge for all four OKX aliases, in the worker and main store respectively.
  OKX base fills divided by `contractSize` must remain exact native contract
  quantities for chaser/copier arithmetic. The product-semantics follow-up also
  carries inverse native counts/quote notional and preserves USD-valued versus
  native PnL units; [scope and proofs](RUST_PRODUCT_SEMANTICS.md).
- `packages/safe-cex/src/exchanges/base.ts` reapplies worker-derived chaser
  tags when native order frames and derived `activeChasers` fields arrive in
  either order. The reusable `chaserOrderUpdates` function in
  `packages/safe-cex/src/webworker/postProcessors/ChaserOrderEnrichmentPostprocessor.ts`
  supplies the same enrichment to the worker postprocessor and main frame
  consumer. Main-store changes use the existing order mutation path and its
  update-ID/notification contract. This helper is now an additional dependency
  of the base shell that must survive its eventual extraction.

## Consumer-blockers follow-up

Native feed ownership and public-factory lifecycle are implemented on
`feat/rust-consumer-blockers`; no TS venue/default is deleted or changed.

| Caller boundary | Current proof / required integration |
| --- | --- |
| `ExchangeWorker` private-only / shared public data | Native Public-role suppression including retries and public-topic guards on multiplexed account sockets; one running public Rust provider fans typed public data into two private `ExchangeWorker` instances over transferred ports. Catalog/contract and typed semantic identity, native snapshots, private-state isolation, subscriptions and disposal are tested, including unchanged-seed readiness and frame-forwarded public sources. In-process `externalPublicData` sources are served over the same protocol (B6); legacy websocket gateway config is rejected on Rust, never silently ignored. |
| `safe-cex-server` `PublicExchangeFactory` | Cold native metadata startup, lazy Redis cache hits, forced native refetch/replacement and failed-refresh preservation are explicit; handle identity stays stable but callers re-read `handle.exchange` after refresh. A bounded startup deadline aborts stuck metadata transports; cache fallback revives a fresh lazy engine. Native product/environment profiles isolate current/latest cache and lock keys, including registry-default Rust venues. Native and unit tests cover concurrent refresh, delayed cache reads, hour rollover and disposal. |
| Saved-account CLI | Actual saved CFM config/factory/start and CLI account formatter run on native state; compiled Bun executable embeds real WASM and starts outside the repository. Offline routing fixture is not a proxy/live-auth claim. |
| MCP | Actual protocol reads native catalog, balance, positions and orders; native public feeds keep store-only ticker reads current (proved through Binance protocol calls); native REST polling remains active (proved through BreakoutProp protocol calls); explicit `EXCHANGE_ENGINE=rust`; packaged isolated Node artifact owns its CJS/WASM bytes and Rust dependency notices. No fallback on an explicit unsupported engine selection. |
| Native host | Trusted host-only typed public ingress; loopback frame-server proof and command-client rejection prevent private sections entering the public seam. No deployment change. |
| Mobile | Awaited `setReactNativeRustEngineLoader` caches host initialization; real browser-module bytes drive two saved account stores under Node. A Bun subprocess loads the actual native registry and Rust class without global WebAssembly/TextEncoder/TextDecoder during import, then initializes real bytes and reads the native catalog. Frame/host I/O still requires codec globals. Hermes/JSI bridge implementation, mobile binary packaging and device verification remain open. |

The shared policy retains each native venue's own metadata/account overlays and
private reconciliation. It does not generalize product coverage, signing modes,
arbitrary `market.info`, or hosted deployments. Provider/catalog failure stops
receiving accounts; it never silently opens account public sockets. See
[RUST_CONSUMER_BLOCKERS.md](RUST_CONSUMER_BLOCKERS.md).

Additional [rewrite audit findings](RUST_AUDITS.md) remain cutover gates.
The consumer repairs and offline proofs do not settle downstream engine delivery,
hosted keyless signing, a Hermes/native bridge, full live private parity or the
production shadow/canary evidence. Keep these separate from the repaired startup,
shared-feed and package-loading boundaries documented here.

## Source-proven deletion blockers and repaired boundaries

The Polymarket landing adds typed prediction sections to
`sdk-rs/engine/src/model.rs`, `sdk-rs/engine/src/wire.rs` and the WASM reader.
`packages/safe-cex/src/store/frameApplier.ts` reconstructs prediction-specific
`Market.info` (condition/outcome/question/description/end date/slug/tick and
neg-risk) and holding metadata through both frame consumers. This is an explicit
typed metadata exception, not an arbitrary native JSON `info` bridge. The
[product-semantics follow-up](RUST_PRODUCT_SEMANTICS.md) adds typed Bybit
classification and retained Bybit/OKX product coverage, with actual consumer and
native fixture proofs. It retains TS venues and registry defaults.

The table records current boundaries after both follow-ups. Repaired engine
seams and product fixtures do not migrate old application configuration or
certify live private connectivity. Passing baseline conformance does not make
those TS paths deletable.

| Existing path / exact source | Current status and user-visible effect | Required replacement and proof before deletion |
| --- | --- | --- |
| `packages/safe-cex/src/webworker/ExchangeWorker.ts`; `exchanges/base.ts`; `exchanges/rust/rust.exchange.ts` | **Repaired:** private-only and the legacy pre-start hook select native disabled policy. Public-role sockets/retries are suppressed; private/business roles and metadata REST stay local. | Preserve typed native policy and role/reconnect tests. Shared data must enter native state; `publicDataOnly` remains distinct from `privateOnly`. |
| `packages/safe-cex/src/exchanges/base.ts` (`initExternalPublicData`); TS provider modules; `exchanges/rust/publicFeed.ts` | **Explicit replacement implemented:** worker-safe shared-public-feed ports ingest typed tickers/books/candles natively and retain account metadata/overlays. In-process `externalPublicData` is served through `serveInProcessPublicData`; websocket gateway config rejects with migration guidance on Rust. | Application/gateway callers must adopt `RustPublicFeedProvider` and transferred `publicFeed` ports before their default cutover. Provider identity/catalog/readiness/disposal and private fan-out are tested; this does not claim an existing deployed gateway was migrated. |
| `sdk-rs/venues/bybit/src/lib.rs`; `sdk-rs/engine/src/model.rs`, `wire.rs`; `packages/safe-cex/src/store/frameApplier.ts`; `packages/cli-core/src/order/prepareCliOrder.ts` | **Metadata gap closed in documented native fixtures:** optional typed classifications preserve native `symbolType`, `marketRegion` and `underlyingTicker` across actual Rust discovery, worker application, transferred buffers and main application. | The real `prepareCliOrder` refuses stock/ETF/commodity/FX; explicitly empty and `innovation` crypto pass, absent and unknown new labels remain unknown. Preserve this regression and minimal typed metadata when migrating defaults/readers; this is not an arbitrary native JSON bridge or live TradFi/private connectivity proof. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.ts`; Rust adapter `fetchMarkets` | **Repaired:** cache miss explicitly starts Rust; Redis hits remain lazy. Forced refresh starts a replacement native engine and publishes it after successful cache persistence. | Preserve cache/lock/fallback semantics and native cold/refetch/error/disposal/hour-rollover regressions. Re-read `handle.exchange` after refresh; cached metadata does not prove a live engine/feed has started. |
| `sdk-rs/venues/okx/src/{lib,account,trading,history}.rs`; `packages/safe-cex/src/store/frameApplier.ts`; OKX TS reference family | **Product filter gap closed:** global linear/inverse swaps and EU X-Perp `FUTURES` retain native instrument/type/underlying metadata. EU uses account-scoped discovery for authenticated accounts and joins its product feeds, reconciliation and supported histories. | Keep actual inverse quantity/position/PnL/margin/ledger fixtures, native placement/chaser bridge, all four host/demo profiles and USD-M/PM control gates. Private documented fixtures and source/unit proofs do not certify live private units or connectivity. External-public overlays and default/deletion migration remain separate blockers. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts`; `exchanges/rust/rust.exchange.ts` option forwarding; `sdk-rs/venues/bybit/` category states | **Combined-product gap closed for the `bybit` linear profile:** saved flags are forwarded and all four inverse/options combinations retain independent public categories and shared private-account reconciliation. Typed option terms, quote/position/account Greeks, chain/expiries and native option order sizing/caller IDs are explicit. | Actual TS/native WASM fixtures compare discovery, private read categories, option APIs and price/limit-IV order payloads; Rust option seeds and depth-25 books have native protocol/frame proofs because TS generic tickers/sockets omit or misroute that product. Unsupported option controls/settings/queries fail explicitly; standalone demo/inverse/spot/Hyro keep their narrower profiles. Preserve external-public policy and live combined/private validation boundaries; do not infer default/deletion readiness. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `sdk-rs/venues/grvt/src/lib.rs` (`on_item` catalog filter); `packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts` (`fetchMarkets` catalog mapping), `packages/safe-cex/src/exchanges/grvt/CLAUDE.md` | Product gap resolved by [explicit retirement decision](GRVT_PRODUCTS_DECISION.md): TS posts `{}` and the official API defaults to `PERPETUAL`. Its generic mapper accepts injected Future/Call/Put rows but drops semantic metadata and cannot correctly cancel their symbols; explicit public non-perpetual catalogs were empty on production/testnet. | Retire only the unexposed mapper-only paths from cutover parity. Preserve TS, public exports and all other consumer/dependency gates. Any future enablement needs real listings, typed kind/expiry/strike/settlement metadata through frames/consumers and complete product lifecycle evidence; generic dispatch and perpetual replay do not prove it. |

## WASM packaging for web consumers

Since #1393 the browser/Workers default loader imports only the account venue's
family engine (`@tealstreet/safe-cex-wasm/engine` → `pkg/web/<family>`), with
one literal import per family. **tealstreet-next web builds must run
`yarn rust:build-wasm --families` before bundling and ship every family engine
the bundler emits**; a bundle that includes safe-cex fails to resolve without
all 27 family packages. Node, the CLI, MCP and React Native still load the
combined build from `yarn rust:build-wasm`, so builds serving both need both.
`loadWebEngine(venue)` and `setWasmEngineLoader((venue) => …)` now receive the
Rust venue. Measurements and decision: [RUST_WASM_FAMILIES.md](RUST_WASM_FAMILIES.md).

## Runtime callers and their replacements

These are the complete direct callers/importers outside the venue runtime
subtree. A type import of `Exchange`/`BaseExchange` is an API dependency, not a
construction of the old implementation. Rust currently extends `BaseExchange`,
so deleting that base first would also delete its working adapter.

| Exact path | Role / current dependency | Replacement before deletion |
| --- | --- | --- |
| `packages/safe-cex/src/index.ts` | Async class registry/cache; constructs every direct SDK/private/public exchange | Preserve `createExchange`/`createPublicExchange` API and explicit engine choice; move default aliases in both registries after evidence. Later return a neutral exchange interface, not `typeof BaseExchange`. |
| `packages/safe-cex/src/webworker/ExchangeWorker.ts` | Registry fallback, explicit Rust constructor, frame forwarding, store cadence, consumers/postprocessors | Keep Rust frame path and compatibility store mirror; replace fallback only per ready alias. Keep `instanceof RustEngineExchange` behavior or replace it with an explicit frame-source capability. |
| `packages/safe-cex/src/candleCache/types.ts` | `Exchange` interface for candle fetcher | Neutral candle-read interface; preserve history/query semantics. |
| `packages/safe-cex/src/crossTrade/getCopierWeightRatio.ts` | Exchange/store weighting contract | Neutral exchange/store interface; same balances/positions. |
| `packages/safe-cex/src/crossTrade/runtime/CrossTradeManager.ts` | Exchange lifecycle, fills, store and order placement | Neutral interface backed by Rust; preserve fills, native position units, reconciliation. |
| `packages/safe-cex/src/crossTrade/runtime/crossTradePlaceOrder.ts` | Copier order entry | Neutral order interface with native placement-unit extras and caller-ID contract. |
| `packages/safe-cex/src/webworker/commandExecutor.ts` | Exchange supplied to CLI worker commands | Keep adapter; replace imported interface path only after extraction. |
| `packages/safe-cex/src/webworker/TelgramWorker.ts` | Exchange/store observer | Same neutral read/lifecycle interface; keep notifications. |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.ts` | Order update/ticker notification and task lifecycle | Keep on frame-backed store; preserve immediate order updateId reactions and throttled ticker reactions. |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance.ts` | Live orders/positions/book, amend/cancel/place | Same interface backed by Rust; preserve caller ID replacement/native units and terminal orders. |
| `packages/safe-cex/src/webworker/twap/TwapInstance.ts` | Child chasers, book/position/store reads | Keep shell and frame-backed store; preserve scheduling, timeout and fill semantics. |
| `packages/safe-cex/src/webworker/postProcessors/ExchangeDependentPostprocessor.ts` | Base type for chaser/TWAP/message stats pipeline | Neutral exchange interface; retain derived state and pipeline order. |
| `packages/safe-cex/src/webworker/handlers/BaseHandler.ts` | Worker handler exchange type | Neutral exchange interface and existing command contract. |
| `packages/safe-cex/src/webworker/handlers/CallMethodHandler.ts` | Runtime `CallableMethodsInWorker` allowlist | Extract allowlist independently of implementation; preserve strict method dispatch. |
| `packages/safe-cex/src/safeCexAxiosCreate.ts` | TS venue Axios signing/retry/stats helpers accept `BaseExchange` | Remove only after all TS API factories and reference harness use cease. Rust requests go through engine transport; preserve shared Axios use outside this module if any. |
| `packages/safe-cex/src/createApiWithSecondaryKey.ts` | TS primary/secondary API construction | Retire after every TS venue/reference dependency is gone; prove Rust secondary-key routing first. |
| `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.ts` | Creates public exchange and reads/refreshes/hydrates markets; return type `BaseExchange` | Keep tested native startup/replacement and cache/lock API; re-read `handle.exchange` after refresh. Extract the neutral interface before deleting BaseExchange. |
| `packages/cli-core/src/commands/trading/ChaseCommand.ts` | Exchange type for shared chaser and book subscription | Neutral interface; preserve store-native units and cancellation. |
| `packages/cli-core/src/worker-host/router.ts` | Routes CLI commands to a live exchange | Keep worker adapter and structural `ExchangeLike` API. |
| `packages/exchange-mcp/src/server.ts` | `createExchange` then start/lifecycle; Exchange type | Same factory API, Rust loader/transport, neutral interface. |
| `packages/exchange-mcp/src/tools.ts` | Account/store reads, order/history calls, runtime `isInertCapability` | Keep RPC schemas; extract capability branding/probe helpers from base and preserve unavailable-query errors. |
| `packages/exchange-mcp/src/config.ts` | Registry-derived exchange-name validation | Stable supported-name registry independent of implementation; do not advertise an absent Rust alias. |
| `apps/cli/src/exchange-factory.ts` | Saved-account factory/start/dispose typed as `BaseExchange` | Preserve account normalization and lifecycle through neutral SDK API. |
| `apps/cli/src/repl.ts` | Exchange type, live account/store reads, refresh/action dispatch | Preserve `ExchangeLike`/worker adapter and frame-backed store; no venue constructor required. |
| `apps/cli/src/cross-trade-daemon.ts` | Casts platform-created exchange to full `Exchange` | Replace cast with explicit copier interface and Rust-backed implementation. |
| `apps/cli/src/record/recorders/orderbook.ts` | Direct keyless `createExchange` plus lifecycle/book callbacks | Same public Rust factory/subscription contract; prove book depth/cadence and disposal. |

Indirect construction paths are also deletion dependencies:

- `packages/safe-cex/src/createExchangeFromAccount.ts` normalizes credentials,
  options, product flags, broker settings, secondary keys and routing, then
  delegates to `createExchange`; preserve every field.
- `apps/cli/src/platform-adapter.ts` dynamically imports that saved-account
  factory; `apps/cli/src/exchange-manager.ts`,
  `packages/cli-core/src/commands/account/AccountCommand.ts`, and
  `packages/cli-core/src/legacyCommands.ts` create through the platform adapter.
  Replacing a registry constructor covers them only after current account
  options and the CLI build's WASM loading are verified.
- `packages/cli-core/src/worker-host/WorkerPlatformAdapter.ts` wraps the existing
  worker exchange; `packages/cli-core/src/types.ts` intentionally uses structural
  `ExchangeLike` instead of importing the concrete TS base.
- `packages/script-api-core/src/adapter/HostBindings.ts` deliberately defines
  a neutral structural host interface; `adapter/createCoreAdapter.ts` consumes
  store reads through it. Retain this boundary instead of introducing concrete
  Rust/TS class dependencies.
- `packages/safe-cex/src/webworker/store.webworker.ts` constructs
  `ExchangeWorker`; its worker/main protocol, frame transfer, restart, error and
  hosted/object compatibility paths must keep working.

`apps/web/` has no direct BaseExchange or concrete venue import in this mirror's
source audit. The downstream tealstreet-next consumers are inventoried in
[tealstreet-next callers at cutover](#tealstreet-next-callers-at-cutover).
Existing in-repo consumers still need their factory and store contract.

## tealstreet-next caller changes shipped at cutover

These land in tealstreet-next together with the cutover, never before it. Each
row is the exact edit the SDK contract change requires.

### Capability record (RUST_QUALITY item 8; #1400, #1411)

`isBaseCapabilityFallback`, `isInertCapability` and `isEmptyStubCapability` are
no longer exported from `@tealstreet/safe-cex/src/exchanges/base` (they were
never on the package index). The only reader is `exchange.capabilities`
(`ExchangeCapabilities`, keys in `CAPABILITY_METHODS`); it also counts an
unconditional `return []` as absent. A Rust exchange reports its record once the
engine exists: after `start()`, or after `await exchange.loadCapabilities()`,
which creates the engine without starting it. Every row is a compile-time break.

| File | Old call | New call |
| --- | --- | --- |
| `apps/web/src/components/openPositions/ClosedPositions.tsx:114` | `isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')` | `!exchange.capabilities.closedPnl` (drop the line-11 import) |
| `apps/web/src/utils/historical/collectAccountAxes.ts:162` | `!isBaseCapabilityFallback(exchange.fetchHistoricalOrders, 'fetchHistoricalOrders')` | `exchange.capabilities.orderHistory` |
| `apps/web/src/utils/historical/collectAccountAxes.ts:164` | `isBaseCapabilityFallback(exchange.fetchHistoricalFills, 'fetchHistoricalFills')` | `!exchange.capabilities.fillHistory` (drop the line-5 import; reword the line-143 comment; `HistoryCapableExchange` already extends `Exchange`) |
| `packages/venue-data/src/journal/symbolScopedFills.ts:74-76` | `typeof exchange.fetchIncomeHistoryRange === 'function' && !isBaseCapabilityFallback(…)` | `exchange.capabilities.incomeRange` (parameter type `{ capabilities: ExchangeCapabilities }`; drop the line-1 import) |
| `packages/venue-data/src/journal/symbolScopedFills.test.ts:94-99`, `symbolScopedFills.bybit.test.ts:35,47` | `{ fetchIncomeHistoryRange: … }` fixtures | `{ capabilities: { incomeRange: … } }` |
| `packages/venue-data/src/journal/axis-grading.registry.test.ts:89` | `!isBaseCapabilityFallback(exchange.fetchHistoricalFills, 'fetchHistoricalFills')` | `(await exchange.loadCapabilities()).fillHistory` (the instance is never started) |
| `scripts/history-audit/lib/axis-inventory.ts:72,74` (called at `:128`) | `isBaseCapabilityFallback(fn, method)` → `'base-fallback'`; `isEmptyStubCapability(fn)` → `'empty-stub'` | `await instance.loadCapabilities()`, then map `fetchClosedPnl`→`closedPnl`, `fetchIncomeHistory`→`incomeHistory`, `fetchIncomeHistoryRange`→`incomeRange`, `fetchHistoricalFills`→`fillHistory`: `'implemented'`/`'unavailable'` (the two absent states merge) |
| `scripts/history-audit/lib/closedPnl.ts:234` | `isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')` | `!exchange.capabilities.closedPnl` |
| `scripts/history-audit/lib/venue-survey.ts:86` | `!isBaseCapabilityFallback(exchange.fetchIncomeHistoryRange, 'fetchIncomeHistoryRange')` | `exchange.capabilities.incomeRange` |
| `scripts/history-audit/lib/venue-survey.ts:107` | `!isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')` | `exchange.capabilities.closedPnl` |
| `scripts/history-audit/lib/venue-survey.ts:165` | `isBaseCapabilityFallback(exchange.fetchHistoricalFills, 'fetchHistoricalFills')` | `!exchange.capabilities.fillHistory` |
| `CLAUDE.md:662` (root) | names `isBaseCapabilityFallback` as the brand check | name `exchange.capabilities` |

`packages/exchange-mcp/src/tools.ts` is a mirrored package: #1400 already
rewrote it, and Copybara carries that change.

Main-thread mirrors (`createExchangeFromAccount` objects that apply worker
frames and never create an engine) adopt the record from the worker's snapshot
frame (`CAPABILITIES` section, #1411) or, since #1462, through the
`fetchEngineCapabilities` worker method once markets load. Before adoption, a
Rust mirror's history methods are unbranded forwarders, so fills, orders and
income report **present** and `closedPnl` reports absent until the record arrives.
`apps/web/src/order/lostSendResolver.ts:82` needs no change: without an engine,
`findOrderByClientOrderId` answers from open orders, never definitively, as
before.

`RUST_VENUES` is now a `ReadonlySet<ExchangeName>` (use `isRustVenue`), and the
engine host no longer has `accountId()` (`getAccount()` answers for every venue
that reports an account lookup). No tealstreet-next file outside the mirrored
packages uses either. #1411's market quantity units travel as frame flag bits
and need no tealstreet-next edit.

### Warm start skips Rust accounts (B4)

`apps/web/src/utils/exchange/eagerExchangeWarmStart.ts:123-124`: keep
`applyCachedSnapshot`, then return `{ hydratedFromCache }` before
`ensureMarkets` (`:58` calls `fetchMarkets`) and `startEagerUserDataFetches`
(`:127`) when `exchange instanceof RustEngineExchange` (import from
`@tealstreet/safe-cex/src/exchanges/rust/rust.exchange`). Otherwise warm start
starts a second private engine on the main thread before the worker exists. The
SDK cannot make `fetchMarkets` inert, because candle-server and the public
factory rely on it starting the engine (B10).

### Error classification reaches the health monitor (B9; #1484)

`ExchangeHealthMonitor` uses a classified kind only when the host passes it.
The SDK puts it on the `'error'` event options (`opts.classified`), the same
object that already carries `sendToSentry`/`enhancedContext`. The calls below
remain required for ordinary command/host errors and legacy TS diagnostics.

For received native diagnostics, #1583 adds `processEvent(event, classified?)`:
classification follows stable codes and available typed detail without matching
message text. Recognized codes may classify even Info/Debug events; unrecognized
Info/Debug returns `null`; unrecognized Warn/Error uses typed classification
or `unknown`.
A generic `request.failed` message alone does not establish auth failure.
`processHealth(exchange.getHealth())` copies lifetime counters into
`HealthState.engineHealth`; it does not increment observed `errorCounts` or
reconstruct events hidden by log-level filtering. Polling/calling it remains
host work, and an absent snapshot leaves the last one unchanged. Collect snapshots
in the engine-owning realm: an unstarted/disposed Rust exchange or a main-thread
mirror without a host returns `undefined`. Hosts must deliver snapshots to the
monitor owner; forwarding `engine-event` alone does not forward counter snapshots.

Use either `engine-event` or the event attached to a legacy notification for
that diagnostic. Metadata-bearing legacy Error/Info/Warn is compatibility
delivery, not evidence of another failure. Keep ordinary command/host errors
without metadata. See the [caller inventory](#coded-diagnostic-caller-cutover-pr5)
for web/CLI listeners, worker dispatch and telemetry migration. #1583 added no
native notice format; #1586 now removes legacy engine `log` actions/notices and
changes the DO diagnostic envelope to coded `op: 'event'`. Upgrade matching
WASM/JS hosts and DO clients together, and deploy compatible safe-cex-server
before native Info+ events. The [PR6 section](#log-shim-removal-pr6) records the
protocol; the deployment remains Sam's decision.

| File | Old call | New call |
| --- | --- | --- |
| `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:1986-2019` (`exchange.on('error', (err, opts) => …)`) and the warning handler `:2041-2051` | builds `{ message, sentryData, enhancedContext, forceToast, suppressConsoleLog }` or passes `err` | add `classified: opts?.classified` to each object (and to the `err` type at `:1779-1788`) |
| `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:1831` | `healthMonitor.processMessage(errorMessage, msgType === 'error')` | `healthMonitor.processMessage(errorMessage, msgType === 'error', classified)` |
| `apps/cli/src/repl.ts:959-962` (`'error'`) and `:1015` (`'warning'`) | `monitor.processMessage(error, true)` | `monitor.processMessage(error, true, (args[1] as { classified?: ClassifiedError } \| undefined)?.classified)` (`ClassifiedError` from `@tealstreet/safe-cex/src/commandOutcome`; it is not on the package index) |

### Command timeout code (B9)

`apps/web/src/utils/exchange/callMethodInWorkerFireAndForget.ts:3-7`:
`isCallMethodInWorkerTimeout` also accepts
`(error as { code?: string }).code === 'COMMAND_TIMEOUT'`. A Rust worker
command times out with `Rust worker command timed out; reconcile interrupted
writes before retrying` instead of `Timeout callMethodInWorker`.

### Incomplete history (#1542, #1554)

Native sessions and Workers now translate event command IDs through the pending
owner and send those notices only to that client. Retired, disconnected, malformed
and unknown owners cannot leak notices to another client. `history.incomplete`
remains Warn but survives an Error minimum level as result metadata. The
structured marker arrives before its command result; JSON clients must still
track it to carry completeness onward.

**Open cutover work (B11), not a completed caller migration.** In the JS WASM
host, a `history.incomplete` event with a pending `fields.command` marks that
command's array result with non-enumerable `incomplete: true`. Currently Kraken
futures emits it for a saturated 500-row income page that its since-only cursor
cannot traverse; Binance emits it when an income chunk walk exhausts its bound.
Available rows remain useful, but totals are lower bounds. An unflagged result
is not proof that every venue's every history endpoint is exhaustive: other
`history.incomplete` notices without a command ID do not acquire this flag.

The following boundaries still need implementation before affected Rust
history can claim journal coverage. NX references in this subsection were
checked at `fdd2389a825`:

| Boundary | Required edit / proof |
| --- | --- |
| SDK `webworker/handlers/CallMethodHandler.ts` → `RustEngineExchange.forwardToWorker` (`WORKER_FORWARDED_METHODS`); NX `services/exchange-server` VWC → `apps/web/src/utils/shim/worker/ExchangeServerWorker.ts` | Carry completeness as an explicit serializable field beside the rows, and restore it at the receiving API boundary. The current `callMethod:done` sends the array directly: structured clone drops the non-enumerable property, as does the hosted JSON hop. Prove both worker and hosted round trips, including an unflagged concurrent request. Deploy the sender and receiver together. |
| NX `apps/web/src/utils/historical/collectAccountAxes.ts` (`runAxis`, `enumerateTradedSymbols`, symbol-scoped fill aggregation) and derived-history adapters | Inspect the flag before `map`/`filter`/spread/folding/enumeration can drop it. A truncated income symbol list also makes the derived fill/position axis incomplete, even if each subsequent fill read succeeds. Preserve that state in `CollectedAxis`/`AxisOutcome`; do not return `status: 'ok'` for it. Any SDK fold that consumes a flagged result must likewise propagate incompleteness. |
| NX `apps/web/src/utils/historical/runHistoricalWindow.ts` (`collectAccountAxis` → `postHistoricalRecords` → `completeWindow`) | Leave `historical_coverage` uncompleted for incomplete results. Currently only `status: 'ok'` reaches `completeWindow`, so the collection outcome must distinguish partial data from a successful empty/full window. Keep the available rows distinct from a claim of complete coverage; persisting partial rows must not complete that claim. Add a truncated-window regression. |
| Native sessions / Durable Object clients, if adopted (`sdk-rs/hosts/native/src/server.rs`, `sdk-rs/hosts/wasm/js/workers.mjs`) | The array flag is attached by `EngineHost`, not encoded as a new frame or `done` field. A direct session client must correlate `history.incomplete` itself and carry completeness onward. Both hosts now remap the event's `fields.command` through the same pending owner as `done` and target the owning client; remote consumers must correlate the translated marker before consuming rows. No current NX `NativeFrameSession` caller was found. |

The BloFin `historical_orders` base-unit backfill (#1529) is a separate
owner-run data migration, described above. Neither this metadata propagation
nor the backfill is implemented by this documentation refresh.

## Shared helpers that a venue-directory deletion would otherwise remove

The class import list alone is insufficient. These surviving runtime imports
must be extracted to stable shared API/policy modules (and imports/re-exports
updated) or retained deliberately when deleting the venue implementation:

| Caller | Venue modules / replacement |
| --- | --- |
| `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` | `asterdex/asterdex.types` auth/STP validation/types; `blofin/blofin.types`, `okx/okx.types`, `phemex/phemex.types` broker IDs; `bybit/bybit.types` Hyro endpoints; `hyperliquid/hyperliquid.cloid` caller-ID rules. Extract these without carrying TS network/venue classes into the shared module. Keep `clientOrderId` and base capability semantics. |
| `packages/safe-cex/src/index.ts` | Public exports from Bybit `bybit.order-params`; Hyperliquid `hyperliquid.builderFee`, `hyperliquid.types`, `hyperliquid.agent`; Aster `asterdex.builder-approval`, `asterdex.builder-fee`; GRVT `grvt.builder`; Lighter `lighter.builder-approval`, `lighter.account-tier`. These wallet/approval/public types remain an API even when a venue runs Rust. Move them preserving export names and independent browser wallet functionality. |
| `packages/safe-cex/src/createExchangeFromAccount.ts`, `accounts/buildExchangeAccountConfig.ts`, `transport/engine.ts` | `bybit/bybit.site` normalization/site/routing policy. Extract shared policy before deleting Bybit directory. |
| `packages/safe-cex/src/transport/tealstreet/routing/routingTable.ts` | `bitget/bitget.types` WebSocket constant. Keep routing policy/hostname fidelity in shared transport config. |
| `packages/safe-cex/src/types.ts`, `webworker/postProcessors/MessageHandlerStatsPostprocessor.ts` | `exchanges/types` shared telemetry/API types. Extract independently of venue removal. |
| `packages/safe-cex/src/exchanges/base.ts` | `packages/safe-cex/src/webworker/postProcessors/ChaserOrderEnrichmentPostprocessor.ts` exports shared `chaserOrderUpdates`. Preserve the function when extracting the base shell/main-frame consumer; it is shared with the worker postprocessor and keeps derived tags independent of message arrival order. |
| `packages/safe-cex/src/exchanges/base.ts`, `base.ws.ts`, `external-public-data.ts`, `in-process-public-data.ts` | Shared lifecycle, emitter, health, capability, public feed/store and TS socket infrastructure. Base remains a runtime dependency of RustEngineExchange. Extract shell contracts and public-feed policy before removing the base. Remove TS socket machinery only after all old venue/reference uses cease. |

The worker's postprocessor registry also retains `DisplayedPositionsPostprocessor`,
`DisplayedMarginPostprocessor`, `ChaserOrderEnrichmentPostprocessor`,
`OrdersRiskProfilePostprocessor`, `ChaserOrderPostprocessor`, `TwapPostprocessor`
and `MessageHandlerStatsPostprocessor`. None is replaced by deleting a venue:
keep derived display/store fields and sequencing while inputs become Rust frames.

## Ordered deletion plan

1. **Prove the seam first (this PR).** Run the `*.wasm.test` conformance matrix
   over every `RUST_VENUES` name through worker/frame/store and consumer paths;
   fix adapter/FrameApplier failures. Preserve TS runtime and the frozen reference
   corpus. Keep absence of optional data distinct from an authoritative empty
   snapshot, updateId bumps, native units, terminal fences and serialize/stub
   parity. Run integration/build/replay controls as applicable.
2. **Close unsupported aliases/products.** Port the 4 missing names or obtain an
   explicit supported-product retirement decision in a separate change. A
   perps port never retires a saved spot/inverse account implicitly. Audit
   all credential/signing modes, proxy/tunnel/region, secondary keys, product
   toggles, hosted public data and query capabilities before changing defaults.
   Preserve the closed Bybit classification/combined-product and OKX inverse/EU
   proofs in [RUST_PRODUCT_SEMANTICS.md](RUST_PRODUCT_SEMANTICS.md). Remaining
   blockers include private-only/external-public feed modes, public-factory
   cold/forced discovery and live account/consumer migration. Registration alone
   does not remove these blockers. GRVT's unexposed non-perpetual mappings have
   a separate [retirement decision](GRVT_PRODUCTS_DECISION.md); its other gates
   remain, and no TS venue is deleted here.
3. **Make default creation converge.** Switch ready aliases in both
   `registry.ts` and `registry.native.ts`; keep all current factory entry points
   and account normalization. Prove actual CLI, public factory and MCP creation,
   module loading/bundling, forced refresh, and worker legacy/object consumers.
   The explicit `engine: 'rust'` selector added here is not this default switch.
4. **Extract shared contracts and wallet utilities.** Move the modules above,
   public exchange interface, method allowlist, capability branding and shell
   lifecycle out of `BaseExchange`/venue directories. Update public re-exports
   without changing consumer APIs. Keep `DefaultStore`/FrameApplier as the
   compatibility seam until their consumers have replacements.
5. **Delete implementations in dependency order, with fresh evidence per PR.**
   Coinbase/Orderly/TxFlow production implementation is already replaced and their
   frozen references remain. Next choose independent ready families (Backpack,
   BingX, Crypto.com, MEXC, BloFin, Bitrue, Phemex, WEEX, WOO, ApeX, Paradex,
   Aster, Polymarket, GRVT, Breakout legacy) after defaults and helper extraction. Delete
   Bybit/Hyro and OKX groups together (subclasses depend on parents), then
   Bitget's three profiles together.
   Remove KuCoin futures, Kraken futures, Hyperliquid perps and Lighter only
   while retaining shared code required by still-unported spot/broker siblings.
   Remove the four Binance TS profiles only after each default path and native
   unit/account/query contract is proven and their shared oracle is retained
   independently; the new Spot/COIN-M ports do not retire USD-M/PM evidence. Retire the remaining 6 implementations only after step 2.
   This ordering is a dependency order, not an assertion that all are already
   deployment-approved.
6. **Retire each old oracle deliberately.** Rewrite useful deterministic venue
   regressions to drive Rust using independent documented fixtures. The replay
   harness dynamically loads TS classes from production paths; freeze/move the
   intended oracle before removing those paths, following Coinbase/Orderly/TxFlow's
   test-only reference pattern. Keep a fixed baseline/performance oracle or
   explicitly replace that gate with a reviewed absolute budget. Never turn a
   TS-vs-Rust gate into Rust-vs-itself.
7. **Remove shared TS machinery last.** Only after no runtime/test/reference/
   harness import remains, remove TS API factories, Axios/signing-only helpers,
   `BaseWebSocket` and `BaseExchange`. The TS store and object payload bridge
   need a separate consumer migration: worker chasers/TWAP/CLI/postprocessors,
   hosted consumers, serialization/warm boot and downstream frame adoption must
   all have explicit replacements. Their deletion is not authorized or proven
   by this phase-9 readiness work. Keep the master merge/deployment decision
   separate, per the rewrite plan.

## Audit method and scope

The audit used repository-wide `rg` for `BaseExchange`, concrete `*.exchange`
imports/constructors/inheritance, registry calls, factory calls, dynamic import/
require and `instanceof`, then resolved relative and `@tealstreet/safe-cex` module
paths in tracked source against both production `src/exchanges/` and frozen
`src/__tests__/reference/` modules. The original resolver omitted imports into
the latter root; PR review caught that omission and the frozen-root pass below
closes it explicitly. The appendices group every direct static/dynamic
class/base/registry import plus mock/source-reference literals by exact caller
file. Paths are relative to repository root; targets below are relative to
`packages/safe-cex/src/exchanges/`, except explicitly repository-rooted
targets for the frozen `src/__tests__/reference/` corpus. Both roots are
resolved: test-only classes remain real inheritance/import dependencies even
though their production registry has switched to Rust. A `.test` file is not runtime evidence;
`__tests__/reference` is deliberately executable only in tests/harness.
Generated bundles, dependency directories and non-source documentation are not
production callers. Comments mentioning BaseExchange without an import are
context, not another call site. Arbitrary consumer-supplied dynamic paths or
external/downstream repositories are outside this audit's proof.

The harness is outside the requested `packages/`/`apps/` inventory but is an
explicit deletion blocker: `sdk-rs/harness/venues.mjs` and `kucoin.mjs` specify
TS module paths dynamically; `replay.mjs` loads them and
`known-ts-fixes-*.mjs` patches the independent TS oracle. Update that mapping
and fixture provenance when a production class moves to test-only references.

## Appendix A: venue runtime import graph

These callers remain until their family is migrated. `base` consumers switch to the extracted shell/interface; websocket/API implementations are removed with their venue; subclass groups migrate together. Registry entries are replaced by Rust constructors per readiness above. The `rust/` adapter and loaders stay.

| Exact caller path | Imported base/class/registry targets |
| --- | --- |
| `packages/safe-cex/src/exchanges/apexomni/apexomni.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/apexomni/apexomni.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/apexomni/apexomni.ws-private.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/exchanges/apexomni/apexomni.ws-public.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/exchanges/backpack/backpack.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/backpack/backpack.ws-private.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/exchanges/backpack/backpack.ws-public.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/exchanges/base.ws.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binance.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binance.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binance.ws-private.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/exchanges/binance/binance.ws-public.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/exchanges/binance/binanceinverse.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binanceinverse.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binanceinverse.ws-private.ts` | `binance/binanceinverse.exchange` |
| `packages/safe-cex/src/exchanges/binance/binanceinverse.ws-public.ts` | `binance/binanceinverse.exchange` |
| `packages/safe-cex/src/exchanges/binance/binancepm.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binancepm.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binancepm.ws-private.ts` | `binance/binancepm.exchange` |
| `packages/safe-cex/src/exchanges/binance/binancepm.ws-public.ts` | `binance/binancepm.exchange` |
| `packages/safe-cex/src/exchanges/binance/binancespot.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binancespot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/binance/binancespot.ws-private.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/exchanges/binance/binancespot.ws-public.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/exchanges/bingx/bingx.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bingx/bingx.ws-private.ts` | `bingx/bingx.exchange` |
| `packages/safe-cex/src/exchanges/bingx/bingx.ws-public.ts` | `bingx/bingx.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitget.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitget/bitget.ws-private.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitget.ws-public.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitget.ws-secondary-public.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitgetinverse.exchange.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitget/bitgetusdc.exchange.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/exchanges/bitrue/bitrue.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitrue/bitrue.ws-private.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/exchanges/bitrue/bitrue.ws-public.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.ws-futures.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.ws-private.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/exchanges/bitunix/bitunix.ws-public.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/exchanges/blofin/blofin.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/blofin/blofin.ws-private.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/exchanges/blofin/blofin.ws-public.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/exchanges/breakoutprop/breakoutprop.api.ts` | `breakoutprop/breakoutprop.exchange` |
| `packages/safe-cex/src/exchanges/breakoutprop/breakoutprop.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/breakoutprop/breakoutprop.ws-public.ts` | `breakoutprop/breakoutprop.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybit.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybit.fee-overlay.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybit.ws-private.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybit.ws-public.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitdemo.exchange.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitinverse.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybitinverse.ws-private.ts` | `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitinverse.ws-public.ts` | `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitspot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/bybit/bybitspot.ws-private.ts` | `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/exchanges/bybit/bybitspot.ws-public.ts` | `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/exchanges/bybit/hyrotrader.exchange.ts` | `base`, `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/hyrotrader.order-recovery.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/exchanges/bybit/hyrotrader.ws-private.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/exchanges/cryptocom/cryptocom.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/cryptocom/cryptocom.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/cryptocom/cryptocom.ws-private.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/exchanges/cryptocom/cryptocom.ws-public.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/exchanges/external-public-data.ts` | `base` |
| `packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/grvt/grvt.ws-private.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/exchanges/grvt/grvt.ws-public.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-private.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.ws-public.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.ws-private.ts` | `hyperliquid/hyperliquidspot.exchange` |
| `packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.ws-public.ts` | `hyperliquid/hyperliquidspot.exchange` |
| `packages/safe-cex/src/exchanges/in-process-public-data.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/kraken.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/kraken.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/kraken.ws-private.ts` | `kraken/kraken.exchange` |
| `packages/safe-cex/src/exchanges/kraken/kraken.ws-public.ts` | `kraken/kraken.exchange` |
| `packages/safe-cex/src/exchanges/kraken/krakenspot.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/krakenspot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/kraken/krakenspot.ws-private.ts` | `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/exchanges/kraken/krakenspot.ws-public.ts` | `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/exchanges/kucoin/kucoin.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/kucoin/kucoin.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/kucoin/kucoin.ws-private.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/exchanges/kucoin/kucoin.ws-public.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/exchanges/kucoin/kucoinspot.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/kucoin/kucoinspot.ws-private.ts` | `kucoin/kucoinspot.exchange` |
| `packages/safe-cex/src/exchanges/kucoin/kucoinspot.ws-public.ts` | `kucoin/kucoinspot.exchange` |
| `packages/safe-cex/src/exchanges/lighter/lighter.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/lighter/lighter.ws-private.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/exchanges/lighter/lighter.ws-public.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/exchanges/mexc/mexc.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/exchanges/okx/okx.api.ts` | `base`, `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okx.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/okx/okx.ws-private.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okx.ws-public.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okxdemo.exchange.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okxeu.exchange.ts` | `base`, `okx/okx.exchange` |
| `packages/safe-cex/src/exchanges/okx/okxeudemo.exchange.ts` | `okx/okxeu.exchange` |
| `packages/safe-cex/src/exchanges/paradex/paradex.api.ts` | `base`, `paradex/paradex.exchange` |
| `packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/paradex/paradex.ws-private.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/exchanges/paradex/paradex.ws-public.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/exchanges/phemex/phemex.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/phemex/phemex.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/phemex/phemex.ws-private.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/exchanges/phemex/phemex.ws-public.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/exchanges/polymarket/polymarket.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/polymarket/polymarket.ws-private.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/exchanges/polymarket/polymarket.ws-public.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/exchanges/registry.native.ts` | `apexomni/apexomni.exchange`, `asterdex/asterdex.exchange`, `backpack/backpack.exchange`, `base`, `binance/binance.exchange`, `binance/binanceinverse.exchange`, `binance/binancepm.exchange`, `binance/binancespot.exchange`, `bingx/bingx.exchange`, `bitget/bitget.exchange`, `bitget/bitgetinverse.exchange`, `bitget/bitgetusdc.exchange`, `bitrue/bitrue.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `breakoutprop/breakoutprop.exchange`, `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange`, `bybit/hyrotrader.exchange`, `cryptocom/cryptocom.exchange`, `grvt/grvt.exchange`, `hyperliquid/hyperliquid.exchange`, `hyperliquid/hyperliquidspot.exchange`, `kraken/kraken.exchange`, `kraken/krakenspot.exchange`, `kucoin/kucoin.exchange`, `kucoin/kucoinspot.exchange`, `lighter/lighter.exchange`, `mexc/mexc.exchange`, `okx/okx.exchange`, `okx/okxdemo.exchange`, `okx/okxeu.exchange`, `okx/okxeudemo.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange`, `polymarket/polymarket.exchange`, `rust/rust.exchange`, `weex/weex.exchange`, `woo/woo.exchange` |
| `packages/safe-cex/src/exchanges/registry.ts` | `apexomni/apexomni.exchange`, `asterdex/asterdex.exchange`, `backpack/backpack.exchange`, `base`, `binance/binance.exchange`, `binance/binanceinverse.exchange`, `binance/binancepm.exchange`, `binance/binancespot.exchange`, `bingx/bingx.exchange`, `bitget/bitget.exchange`, `bitget/bitgetinverse.exchange`, `bitget/bitgetusdc.exchange`, `bitrue/bitrue.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `breakoutprop/breakoutprop.exchange`, `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange`, `bybit/hyrotrader.exchange`, `cryptocom/cryptocom.exchange`, `grvt/grvt.exchange`, `hyperliquid/hyperliquid.exchange`, `hyperliquid/hyperliquidspot.exchange`, `kraken/kraken.exchange`, `kraken/krakenspot.exchange`, `kucoin/kucoin.exchange`, `kucoin/kucoinspot.exchange`, `lighter/lighter.exchange`, `mexc/mexc.exchange`, `okx/okx.exchange`, `okx/okxdemo.exchange`, `okx/okxeu.exchange`, `okx/okxeudemo.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange`, `polymarket/polymarket.exchange`, `rust/rust.exchange`, `weex/weex.exchange`, `woo/woo.exchange` |
| `packages/safe-cex/src/exchanges/rust/loadNodeEngine.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/exchanges/rust/loadWebEngine.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/weex/weex.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/weex/weex.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/weex/weex.ws-private.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/exchanges/weex/weex.ws-public.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/exchanges/woo/woo.api.ts` | `base` |
| `packages/safe-cex/src/exchanges/woo/woo.exchange.ts` | `base` |
| `packages/safe-cex/src/exchanges/woo/woo.ws-private.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/exchanges/woo/woo.ws-public.ts` | `woo/woo.exchange` |

## Appendix B: tests and frozen references

These are all direct base/concrete-class/registry callers in test source at the audit checkpoint. Type-only mocks retain the neutral interface; implementation tests migrate to Rust-backed tests or reviewed test-only frozen references before the target is removed. Assertions are not silently dropped. Entries importing only `rust/rust.exchange` already exercise the new adapter and stay. The new `cutover/consumers.wasm.test.ts` drives the shared consumer/store seam; `cutover/realVenue.wasm.test.ts` supplies real native venue startup/frame coverage. Their support modules `packages/safe-cex/src/__tests__/cutover/consumerFrame.ts` and `packages/safe-cex/src/__tests__/cutover/realVenue.ts` construct inputs; the latter deliberately depends on Bybit endpoint constants and the replay harness/typed tape builders, so those test-only dependencies must move with any helper/oracle extraction.

| Exact caller path | Imported base/class/registry targets |
| --- | --- |
| `packages/safe-cex/src/__tests__/audit-regressions/binance-variants.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.breakoutprop-wrapper.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-private-rust-port.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-public-delta-rust-port.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-signing-integer-precision.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-trading-rust-port.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.grvt-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.price-history.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.private-executions.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.public-book-state.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.polymarket-frame.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.account-identity.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.candle-handler-guard.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.cfm-private-ws-contract-size.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.native-edit.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.ohlcv-stats-cleanup.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.open-orders-polling.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.place-order-response.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.private-ws-auth.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbasespot.spot-only.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbasespot.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.cancel-algo-orders.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.expected-order-rejections.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.isolated-margin.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.leverage-clamp.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.market-leverage-limits.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.native-amend.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.orderbook-snapshot-retry.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.private-ws-fill-mapping.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.startup-leverage-error.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.trailing-stop-rejection.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbasespot.exchange.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/kodiakfi/kodiakfi.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/mode/mode.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/salsadex/salsadex.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/what/what.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/woofi/woofi.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-private.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/cutover/consumers.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/cutover/metadata.wasm.test.ts` | `base`, `rust/rust.exchange`; retain the optional-metadata/multi-market frame contract, replacing the TS base with a neutral consumer bridge before base deletion |
| `packages/safe-cex/src/__tests__/cutover/realVenue.wasm.test.ts` | `rust/rust.exchange` |
| `packages/cli-core/src/commands/utility/tasksAndKill.test.ts` | `base` |
| `packages/cli-core/src/worker-host/router.test.ts` | `base` |
| `packages/exchange-mcp/src/tools.test.ts` | `base`, `registry` |
| `packages/safe-cex/src/__tests__/asterdex.leverage-brackets.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/apexomni.partial-amend.test.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/apexomni.remaining-audit.test.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/apexomni.rust-parity.test.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/apexomni.split-tpsl.test.ts` | `apexomni/apexomni.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.builder-attribution.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.orderbook-snapshot.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.private-balance.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.testnet-clock.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.trailing-stop-capability.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.v3-auth-context.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/asterdex.v3-stp.test.ts` | `asterdex/asterdex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.execution-history.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.native-batch.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.order-mapping.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.orderbook-depth.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.state-reconciliation.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/backpack.trigger-reference.regression.test.ts` | `backpack/backpack.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-exchange.dispose.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-exchange.http-failure-context.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-exchange.watchdog.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-websocket.contract.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/base-websocket.reconnect-policy.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.cancel-symbol-orders.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.income-pacing.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.order-snapshot-failure.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.position-mode-regression.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.private-stream-recovery.test.ts` | `binance/binance.exchange`, `binance/binancepm.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.response-shape.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.shared-public-leverage.test.ts` | `binance/binance.exchange`, `binance/binancepm.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binance.v3-account-contract.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binanceinverse.exchange.test.ts` | `base`, `binance/binanceinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancepm.position-sign.test.ts` | `binance/binancepm.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancepm.response-shape.test.ts` | `binance/binancepm.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancespot.private-wire-shape.test.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancespot.rest-contract.test.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/binancespot.user-data-stream.test.ts` | `binance/binancespot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bingx.audit-regressions.test.ts` | `bingx/bingx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bingx.native-amend.test.ts` | `bingx/bingx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.account-info.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.public-ws-product-type.test.ts` | `bitget/bitget.exchange`, `bitget/bitgetusdc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.rest-freshness-and-market-shape.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.sized-stop-plan-type.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.trailing-stop.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.uta.test.ts` | `bitget/bitget.exchange`, `bitget/bitgetinverse.exchange`, `bitget/bitgetusdc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.validate-account.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitget.ws-fill.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.attached-tpsl-payload.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.history-identity.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.leverage-read-probe.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.order-snapshots.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.private-executions.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.remaining-audit.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.trading-safety-defaults.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitrue.websocket-transport.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitunix.native-modify.test.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitunix.remaining-audit.test.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bitunix.validate-account.test.ts` | `bitunix/bitunix.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/blofin.auth-failure-backoff.test.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/blofin.remaining-audit.test.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/blofin.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/breakoutprop.order-contract.test.ts` | `breakoutprop/breakoutprop.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit-inverse.conditional-orders.test.ts` | `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.advanced-order-params.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.auth-failure-backoff.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.demo-url-resolution.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.initial-position-snapshot.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.margin-mode.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.market-slippage.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.monthly-candles.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.positions-pagination.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.recently-placed-order-preservation.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.reinit-on-signing-failure.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybit.uta-default.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/bybitdemo.order-risk.test.ts` | `bybit/bybitdemo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/capability-branding.test.ts` | `base`, `registry` |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.fail-closed.test.ts` | `apexomni/apexomni.exchange`, `asterdex/asterdex.exchange`, `backpack/backpack.exchange`, `base`, `binance/binance.exchange`, `binance/binancepm.exchange`, `binance/binancespot.exchange`, `bingx/bingx.exchange`, `bitget/bitget.exchange`, `bitrue/bitrue.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `breakoutprop/breakoutprop.exchange`, `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange`, `bybit/hyrotrader.exchange`, `cryptocom/cryptocom.exchange`, `grvt/grvt.exchange`, `hyperliquid/hyperliquid.exchange`, `hyperliquid/hyperliquidspot.exchange`, `kraken/kraken.exchange`, `kraken/krakenspot.exchange`, `kucoin/kucoin.exchange`, `kucoin/kucoinspot.exchange`, `lighter/lighter.exchange`, `mexc/mexc.exchange`, `okx/okx.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange`, `polymarket/polymarket.exchange`, `rust/rust.exchange`, `weex/weex.exchange`, `woo/woo.exchange`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.forwarding.test.ts` | `backpack/backpack.exchange`, `binance/binance.exchange`, `binance/binancepm.exchange`, `binance/binancespot.exchange`, `bingx/bingx.exchange`, `bitget/bitget.exchange`, `bitrue/bitrue.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange`, `cryptocom/cryptocom.exchange`, `grvt/grvt.exchange`, `kucoin/kucoin.exchange`, `kucoin/kucoinspot.exchange`, `okx/okx.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange`, `rust/rust.exchange`, `weex/weex.exchange`, `woo/woo.exchange`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/cryptocom.order-writes.test.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/cryptocom.valuation-streams.test.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/cryptocom.websocket-protocol.test.ts` | `cryptocom/cryptocom.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/exchange-metadata-capabilities.test.ts` | `base`, `registry` |
| `packages/safe-cex/src/__tests__/audit-regressions/external-public-data.liveness.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/fill-event.incremental.test.ts` | `asterdex/asterdex.exchange`, `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-fill-p0-regressions.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt-update-order-p0-regressions.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/grvt.instrument-metadata-cache.audit-regression.test.ts` | `grvt/grvt.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.cloid.test.ts` | `hyperliquid/hyperliquid.exchange`, `hyperliquid/hyperliquidspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.hip3-startup-budget.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.market-data-normalization.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.mismatched-api-wallet.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.schedule-cancel.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.spot-markets-merge.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.terminal-cancel.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.tif-and-reduce-downsize.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.configuration.test.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.contract.test.ts` | `base`, `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.order-recovery.test.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.private-recovery.test.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.public-snapshot.test.ts` | `bybit/bybit.exchange`, `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.signing.test.ts` | `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kraken.remaining-audit.test.ts` | `kraken/kraken.exchange`, `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.client-order-id.audit-regression.test.ts` | `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.native-amend.audit-regression.test.ts` | `kraken/krakenspot.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.algo-update-rejection.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.book-buffer-gap.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.cancel-replace-contract-size.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.cancel-symbol-v3.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.normal-order-rejection.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/kucoin.remaining-audit.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/lighter.public-data.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/lighter.remaining-audit.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/lighter.rust-boundary.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/mexc.audit-regressions.test.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/mexc.depth-sequence.test.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/mexc.order-reconciliation.test.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/mexc.trading-reference.test.ts` | `mexc/mexc.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx-full-depth.audit-regression.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx-rust-oracle.audit-regression.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.conditional-orders.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.monthly-candles.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.orderbook-unsubscribe.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.remaining-audit.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.sequenced-orderbook-adoption-gate.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.split-remainder.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okx.websocket-routing.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/okxdemo.transport.test.ts` | `okx/okxdemo.exchange`, `okx/okxeudemo.exchange`, `registry` |
| `packages/safe-cex/src/__tests__/audit-regressions/okxeu.xperp-markets.test.ts` | `okx/okxeu.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/order-payload-normalization.test.ts` | `binance/binance.exchange`, `bitget/bitget.exchange`, `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.rust-transport.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.batch-cancel.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.batch-orders.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.candle-fences.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.empty-history-cursor.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.execution-stream.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.followups.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.full-depth.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.heartbeat.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.history-auth-readiness.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.native-amend.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.opening-trigger.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.sbe.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.single-order-client-id.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/paradex.stop-loss-order-types.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.get-account.audit-regression.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.margin-type-flat-position.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.order-write-routing.audit-regression.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.positions-balance.audit-regression.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.remaining-audit.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/phemex.rust-public-parity.test.ts` | `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.market-positions.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.safety-state-placement.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/polymarket.websocket-contract.test.ts` | `polymarket/polymarket.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/private-websocket.auth-resubscribe.test.ts` | `bitget/bitget.exchange`, `bybit/bybit.exchange`, `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/reconciliation-fetch-results.test.ts` | `apexomni/apexomni.exchange`, `hyperliquid/hyperliquid.exchange`, `lighter/lighter.exchange`, `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust-frame.inverse-units.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.apexomni-wrapper.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.asterdex-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.client-order-id-rules.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.default-wasm-loader.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.hyperliquid-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.kraken-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.lighter-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.paradex-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/rust.woo-wrapper.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/shared-public-account-overlay.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `okx/okxeu.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/socket-diagnostics-summary.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/transport-remote-signing.audit-regression.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.conditional-orders.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.dynamic-tickers.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.order-errors.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.private-ws.audit-regression.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.public-kline.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.regular-batch-orders.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/weex.tpsl-amend.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.account-info-readback.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.funding-refresh.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.legacy-list-building.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.orderbook-update.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.remaining-audit.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.rust-parity.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/audit-regressions/woo.websocket-routing.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/__tests__/base.remote-signer.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/base.ws-auth-signing.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/binance-market-websocket.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/binance.tick.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/bitget.liquidation-price.test.ts` | `bitget/bitget.exchange` |
| `packages/safe-cex/src/__tests__/bitrue.execution-history.test.ts` | `bitrue/bitrue.exchange` |
| `packages/safe-cex/src/__tests__/blofin.contract-scaling-regression.test.ts` | `blofin/blofin.exchange` |
| `packages/safe-cex/src/__tests__/bybit.brazil-inverse-amend.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.conditional-order.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.execution-history.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/bybit.income-history.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/bybit.inverse-exchange.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange`, `bybit/hyrotrader.exchange` |
| `packages/safe-cex/src/__tests__/bybit.inverse-spot.startup-rejection.test.ts` | `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/__tests__/bybit.market-startup.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.markets-failure.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.options-exchange.test.ts` | `bybit/bybit.exchange`, `bybit/bybitdemo.exchange` |
| `packages/safe-cex/src/__tests__/bybit.private-socket-category-gate.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybit.startup-auth-log.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/bybitinverse.order-units.test.ts` | `bybit/bybitinverse.exchange` |
| `packages/safe-cex/src/__tests__/coinbase.rust.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/faultBench/badLink.scenarios.test.ts` | `okx/okx.exchange` |
| `packages/safe-cex/src/__tests__/faultBench/binanceLookup.scenarios.test.ts` | `binance/binance.exchange` |
| `packages/safe-cex/src/__tests__/faultBench/bybitClockSync.scenarios.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/faultBench/bybitLostCreate.scenarios.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/__tests__/hyperliquid.exchange.startup.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/hyperliquid.hip3-market-loading.test.ts` | `hyperliquid/hyperliquid.exchange` |
| `packages/safe-cex/src/__tests__/hyperliquidspot.balance.test.ts` | `hyperliquid/hyperliquidspot.exchange` |
| `packages/safe-cex/src/__tests__/in-process-public-data.health.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/in-process-public-data.test.ts` | `base` |
| `packages/safe-cex/src/__tests__/kucoin.exchange-docs.test.ts` | `kucoin/kucoin.exchange` |
| `packages/safe-cex/src/__tests__/kucoinspot.exchange.test.ts` | `kucoin/kucoinspot.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.account-lookup.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.builder-fee.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.ohlcv-fallback.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.place-orders.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.signer-gate.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/lighter.exchange.startup.test.ts` | `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/okxeu.validate-account.test.ts` | `okx/okxeu.exchange`, `okx/okxeudemo.exchange` |
| `packages/safe-cex/src/__tests__/orderly.rust.wasm.test.ts` | `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/paradex.error-details.test.ts` | `paradex/paradex.exchange` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts` | `base` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` | `base` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts` | `base` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` | `base` |
| `packages/safe-cex/src/__tests__/rust.exchange.wasm.test.ts` | `base`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/targeted-fill-arrows.execution-history.test.ts` | `apexomni/apexomni.exchange`, `bitunix/bitunix.exchange`, `blofin/blofin.exchange`, `lighter/lighter.exchange`, `okx/okx.exchange`, `paradex/paradex.exchange`, `phemex/phemex.exchange` |
| `packages/safe-cex/src/__tests__/transport-adapter-selection.characterization.test.ts` | `base`, `registry`, `rust/rust.exchange` |
| `packages/safe-cex/src/__tests__/transport-direct-ws.test.ts` | `bitget/bitget.exchange`, `grvt/grvt.exchange`, `lighter/lighter.exchange` |
| `packages/safe-cex/src/__tests__/weex.utils.test.ts` | `weex/weex.exchange` |
| `packages/safe-cex/src/__tests__/woo.execution-history.test.ts` | `woo/woo.exchange` |
| `packages/safe-cex/src/createApiWithSecondaryKey.test.ts` | `base` |
| `packages/safe-cex/src/crossTrade/getCopierWeightRatio.test.ts` | `base` |
| `packages/safe-cex/src/crossTrade/runtime/crossTradePlaceOrder.test.ts` | `base` |
| `packages/safe-cex/src/index.test.ts` | `registry` |
| `packages/safe-cex/src/safeCexAxiosCreate.test.ts` | `base` |
| `packages/safe-cex/src/telemetry.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/transport/direct.test.ts` | `bybit/bybit.exchange` |
| `packages/safe-cex/src/transport/engine.test.ts` | `base` |
| `packages/safe-cex/src/webworker/ExchangeWorker.startup-phase.test.ts` | `registry` |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance.terminate.test.ts` | `base` |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance.test.ts` | `base` |
| `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/BaseHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/BybitTickerSubscriptions.test.ts` | `bybit/bybit.exchange`, `bybit/bybitinverse.exchange`, `bybit/bybitspot.exchange` |
| `packages/safe-cex/src/webworker/handlers/CallMethodHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/ChaserOrderHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/CommandHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/DevHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/OHLCVSubscriptionsHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/OrderBookSubscriptionsHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/TwapHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/UiStateHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/handlers/UpdatePositionsHandler.test.ts` | `base` |
| `packages/safe-cex/src/webworker/postProcessors/ChaserOrderPostprocessor.test.ts` | `base` |
| `packages/safe-cex/src/webworker/postProcessors/MessageHandlerStatsPostprocessor.test.ts` | `base` |
| `packages/safe-cex/src/webworker/postProcessors/TwapPostprocessor.test.ts` | `base` |
| `packages/safe-cex/src/webworker/twap/TwapInstance.test.ts` | `base` |
| `packages/safe-cex/src/webworker/twap/TwapInstance.timeout.test.ts` | `base` |


## Appendix C: frozen oracle dependencies and their replacement policy

At the Coinbase/Orderly audit checkpoint, resolving the frozen root found **32 direct class-importing caller files**:
11 Coinbase audit files (including the two client-ID suites already listed in
Appendix B), 10 Orderly audit files, three Coinbase reference subclasses/socket
types and eight Orderly reference broker subclasses/socket types. Appendix B
now names all 32 exact class targets, adding the 30 previously absent callers
and extending the existing two client-ID entries. These counts are test/reference
source counts for that checkpoint; the current runtime totals are 49 names / 48 Rust aliases.

That checkpoint's Coinbase/Orderly frozen-module graph below additionally includes signing/API/type/
utility imports: **79 static imports in 40 caller files** at this review
checkpoint. TxFlow's later frozen-reference additions are recorded separately below. Full target paths avoid confusing frozen classes with removed
production implementations. Each row remains test-only. Keep these fixed
oracles while the replay and audit tests need them; their replacement is a
reviewed Rust-backed test using independent documented inputs, or deliberate
retention in `__tests__/reference`. Do not reintroduce them into production,
delete their assertions implicitly, or replace TS-vs-Rust evidence with
Rust-vs-itself. Broker subclasses inherit the fixed Orderly reference rather
than selecting a production registry adapter. Coinbase spot inherits its fixed
Coinbase reference, and socket type imports depend on that same class.

| Exact caller path | Frozen reference targets (repository-relative) |
| --- | --- |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.fail-closed.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.forwarding.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.account-identity.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.candle-handler-guard.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.cfm-private-ws-contract-size.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.native-edit.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.ohlcv-stats-cleanup.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.open-orders-polling.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.place-order-response.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.private-ws-auth.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbase.rest-signing.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/coinbasespot.spot-only.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbasespot.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/no-retried-writes.test.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.cancel-algo-orders.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.expected-order-rejections.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.isolated-margin.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.leverage-clamp.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.market-leverage-limits.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.native-amend.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.orderbook-snapshot-retry.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.private-ws-fill-mapping.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.startup-leverage-error.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/orderly.trailing-stop-rejection.test.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.orders.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.orders.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/coinbase/coinbasespot.exchange.ts` | `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts`, `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.types.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/kodiakfi/kodiakfi.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/mode/mode.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/salsadex/salsadex.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/tealstreet-dex/tealstreet-dex.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/what/what.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/exchanges/woofi/woofi.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.encryption.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.utils.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-private.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-public.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-private.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.encryption.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.utils.ts` |
| `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.types.ts`, `packages/safe-cex/src/__tests__/reference/orderly/orderly.utils.ts` |

Source-inspection strings in `coinbase.starbase-private.test.ts`,
`coinbase.starbase-ws-session.test.ts`, `coinbase.starbase-public-frames.test.ts`,
`coinbase.starbase-cutover.test.ts`, `coinbase.starbase-session.test.ts` and
`coinbase.starbase-orders.test.ts` (all under
`packages/safe-cex/src/__tests__/audit-regressions/`) read the frozen Coinbase
source with file reads; they are not static class imports. Their exact
paths and source assertions must be updated deliberately if the frozen reference
moves. `packages/script-api-core/src/adapter/orderEntryPoints.test.ts` also
names the frozen Coinbase class path to verify that test/reference code is
classified as test source. These source-policy assertions survive independently
of runtime venue deletion.


## Appendix D: TxFlow frozen reference and retained helper

TxFlow now defaults to Rust in both production registries. The removed
`src/exchanges/txflow/txflow.api.ts` and `txflow.exchange.ts` paths are not
production callers. The implemented test oracle lives in
`src/__tests__/reference/txflow`; `sdk-rs/harness/venues.mjs` dynamically loads
its `txflow.exchange.ts` for independent replay. Keep that reference until its
oracle and audits have an explicitly reviewed replacement, without changing
TS-vs-Rust comparisons into Rust-vs-itself.

The current TxFlow-only static frozen-module graph has **27 distinct module
edges in 13 caller files** (type imports included):

| Exact caller path | Frozen reference targets (repository-relative) |
| --- | --- |
| `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.fail-closed.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.account.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.history.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.public.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.rust-wrapper.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.signing.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.signing.ts` |
| `packages/safe-cex/src/__tests__/audit-regressions/txflow.socket-pacing.test.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.ws-public.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.api.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.api.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.trading.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.ws-public.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.trading.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.signing.ts` |
| `packages/safe-cex/src/__tests__/reference/txflow/txflow.ws-public.ts` | `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts`, `packages/safe-cex/src/__tests__/reference/txflow/txflow.types.ts` |

`src/exchanges/rust/rust.exchange.ts` imports the production
`src/exchanges/txflow/txflow.client-id.ts` rule; the frozen
`txflow.trading.ts` also imports/re-exports that helper. It is an active
shared dependency, not a deleted venue implementation. The frozen
`txflow.signing.ts` imports Hyperliquid's TS action-preimage helper, so preserve
or deliberately extract that helper before deleting the Hyperliquid TS tree.
The reference also retains its normal BaseExchange/BaseWebSocket, API-factory
and transport dependencies. Generic consumer fixtures do not authorize deleting
those oracle dependencies or enabling measurement-gated order controls.

### Orderly port-review display/read corrections

Orderly `fetchOHLCV` callers supplying `from`/`to` now receive native history
within those bounds rather than a locally filtered latest-only window. Consumers
keep the same candle shape. Order lists/chart overlays see a real trailing
trigger derived from native extreme/callback value or rate: BUY adds, SELL
subtracts. This corrects TS's BUY/rate display, with no write-contract change.

### BingX port-review metadata and stream recovery

Rust BingX `updateSymbolMetadata(symbol)` now reads native leverage and margin
mode for flat symbols and publishes them together when the read remains current.
Revision/query-generation races and reads issued during a pending settings write
prevent stale store publication; they still return current cached buy/sell rails,
with an independent validated-read fallback for each missing rail. Overlap alone
never fails the read or produces `null`; transport/native-response failures
remain failures. A superseded read finishes without another margin-mode request;
a read issued during a pending write stays ineligible to publish even after that
write settles. Current reads retain atomic leverage/margin publication.
`fetchLeverageForSymbol` and selected metadata refreshes keep their existing APIs.
No extra polling or write retry is added. Malformed position-stream rows retain prior exposure
while valid peers update, followed by authoritative REST repair. Hedge leverage
and bracket trigger-source contracts remain as documented.

### Lighter port-review confirmation and failures

A Rust Lighter reduce-only limit order without matching known exposure now waits
for account-read completion, up to10 seconds, before any nonce/signing request.
Unconfirmed, failed or timed-out reads refuse the unsent leg; a prior accepted
leg remains reflected in the aggregate outcome. Late reads cannot release later
queued orders. Both Lighter deployments keep their existing placement units and
result shapes. HTTP405/429 include neutral guidance with native diagnostics;
explicit invalid-margin21613 rejection refreshes state without changing cached
mode by guess or automatically replaying a placement.


### Phemex explicit index triggers and daily cutoff

Explicit SL/TP index-price sources now produce the documented `ByIndexPrice`
wire value instead of native refusal or TS's mark/last fallback. Default sources
are unchanged. OHLCV consumers retain the caller's daily cutoff; Rust deliberately
keeps the latest valid daily bars that TS's prior-day shift omitted. Shapes and
caller signatures are unchanged.

### Coinbase port-review retained read semantics

Coinbase Rust spot currency balances continue using native total/free and their
held difference; chart reads continue honoring an explicit from bound. These
intentional corrections to the TS reference are not parity defects. CFM summary
unrealized-PnL/equity aggregation remains an explicit accounting risk pending
portfolio and downstream consumer evidence. This revalidation changes no API,
selector, deployment status or runtime behavior.

## Coded diagnostic caller cutover (PR5)

The SDK exposes `engine-event` (`EngineEvent`), also forwarded by ExchangeWorker
as `{ op: 'engine-event', data: [event] }`. Until callers migrate, Error still
uses `error` with `opts.engineEvent`; Info/Warn uses `log` with the event as its
third argument. Choose one path per report; skip a legacy notification only
when its event metadata is present. Keep ordinary command/host error listeners. Use
`ExchangeHealthMonitor.processEvent(event, classified?)` for native diagnostics
and `processHealth(exchange.getHealth())` for authoritative lifetime counters;
observed event counts cannot substitute for filtered/deduplicated engine counts.
Keep `processMessage` only for legacy TS/unstructured diagnostics.

Read-only tealstreet-next inventory; application changes remain cutover work:

| Caller | Required change and preserved behavior |
| --- | --- |
| `apps/web/src/controllers/exchangeController/exchangeAccountController.ts:180-383` and its worker listener | Accept/forward `engine-event`; classify code and typed errors before toast, auth badge and terminal reload latch. Preserve 120 s auth badge/reload behavior and startup-phase tags. Avoid classifying the same diagnostic on both legacy and structured paths. |
| `apps/cli/src/exchange-manager.ts` (health-monitor owner) and its error/log listeners | Feed events to `processEvent` and refresh engine counters via `getHealth()`; keep legacy fallback for TS venues. |
| `apps/cli/src/error-classifier.ts` | Use typed venue codes/fields for Bybit environment mismatch rather than the message prefix; preserve critical dedup by account/category. |
| `apps/web/src/utils/telemetry/safeCexTelemetrySink.ts` | Preserve `event_code` tags and use stable codes for grouping; builder-fee reporting stays independent. |
| `packages/exchange-pretty-error-messages/src/bybit/bybitHandler.ts` and `bybitRejectReasonToLocale.ts` | Select translations from typed venue code/fields and event code, including signing failures and order rejection; message text is display data. |
| `packages/exchange-pretty-error-messages/src/bingx/bingxHandler.ts` | Keep display passthrough; use the monitor's code-based severity/category rather than adding text classification. |

`auth.rejected` identifies a credential rejection; typed `authExpired` and
`authPermission` distinguish expiry and permission. A generic `request.failed`
must not be upgraded to auth solely by message wording (Paradex signing/network
failures still share that code). Preserve the current legacy Error notifications
until callers and any remaining venue classifications are migrated together.

Deployment ordering: **safe-cex-server must be deployed before Info+ venue events
matter to native clients**. Old NativeFrameSession rejects the native `event`
notice. Deployment is Sam's call; neither logging PR deploys anything.
