# Phase 9: TypeScript caller inventory and deletion dependencies

Audited 2026-10-20 on the phase-9 readiness branch, against the repository's
`packages/` and `apps/` source. This is an inventory and an ordered future
migration plan. This change deletes no TypeScript venue. The authoritative
phase-9 decision and gate results live in [RUST_REWRITE.md](RUST_REWRITE.md).

## What is ready, and what is still a deletion blocker

The SDK exposes **49 registered exchange names**. `RUST_VENUES` includes **47**.
The two registries currently default **9 names** to `RustEngineExchange`:
`coinbase`, `coinbasespot`, and Orderly's `tealstreet-dex`, `kodiakfi`, `mode`,
`salsadex`, `what`, `woofi`, plus testnet-only `txflow`. They default the other **40 names** to TypeScript,
including **38 names with an available Rust adapter**. Registration in
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
| `kucoinspot` | No | TS required; futures coverage cannot substitute for saved spot accounts. |
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
| `bitunix` | None | TS required. Roster work-in-progress is not a registered Rust adapter; an explicit port or supported-product retirement decision remains necessary. |

The 2 names absent from `RUST_VENUES` are exactly `bitunix`, `kucoinspot`.
Do not silently route them through another family's adapter.

Native consumer proof includes both WebSocket venues and HTTP-polling venues.
BreakoutProp starts the actual Rust engine with documented DXtrade session/
account/quote responses; it emits no venue socket frames. Its fixture proves
polling dispatch, frame/store publication and consumer compatibility under that
input. It is not a live credential capture or evidence that new proprietary
Breakout Terminal accounts work with this legacy DXtrade adapter.

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
| `ExchangeWorker` private-only / shared public data | Native Public-role suppression including retries and public-topic guards on multiplexed account sockets; one running public Rust provider fans typed public data into two private `ExchangeWorker` instances over transferred ports. Catalog/contract and typed semantic identity, native snapshots, private-state isolation, subscriptions and disposal are tested, including unchanged-seed readiness and frame-forwarded public sources. Existing TS `externalPublicData` callers must adopt `publicFeed`/`RustPublicFeedProvider`; legacy gateway config is rejected on Rust, never silently ignored. |
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
| `packages/safe-cex/src/exchanges/base.ts` (`initExternalPublicData`); TS provider modules; `exchanges/rust/publicFeed.ts` | **Explicit replacement implemented:** worker-safe shared-public-feed ports ingest typed tickers/books/candles natively and retain account metadata/overlays. Legacy TS `externalPublicData` rejects with migration guidance on Rust. | Application/gateway callers must adopt `RustPublicFeedProvider` and transferred `publicFeed` ports before their default cutover. Provider identity/catalog/readiness/disposal and private fan-out are tested; this does not claim an existing deployed gateway was migrated. |
| `sdk-rs/venues/bybit/src/lib.rs`; `sdk-rs/engine/src/model.rs`, `wire.rs`; `packages/safe-cex/src/store/frameApplier.ts`; `packages/cli-core/src/order/prepareCliOrder.ts` | **Metadata gap closed in documented native fixtures:** optional typed classifications preserve native `symbolType`, `marketRegion` and `underlyingTicker` across actual Rust discovery, worker application, transferred buffers and main application. | The real `prepareCliOrder` refuses stock/ETF/commodity/FX; explicitly empty and `innovation` crypto pass, absent and unknown new labels remain unknown. Preserve this regression and minimal typed metadata when migrating defaults/readers; this is not an arbitrary native JSON bridge or live TradFi/private connectivity proof. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.ts`; Rust adapter `fetchMarkets` | **Repaired:** cache miss explicitly starts Rust; Redis hits remain lazy. Forced refresh starts a replacement native engine and publishes it after successful cache persistence. | Preserve cache/lock/fallback semantics and native cold/refetch/error/disposal/hour-rollover regressions. Re-read `handle.exchange` after refresh; cached metadata does not prove a live engine/feed has started. |
| `sdk-rs/venues/okx/src/{lib,account,trading,history}.rs`; `packages/safe-cex/src/store/frameApplier.ts`; OKX TS reference family | **Product filter gap closed:** global linear/inverse swaps and EU X-Perp `FUTURES` retain native instrument/type/underlying metadata. EU uses account-scoped discovery for authenticated accounts and joins its product feeds, reconciliation and supported histories. | Keep actual inverse quantity/position/PnL/margin/ledger fixtures, native placement/chaser bridge, all four host/demo profiles and USD-M/PM control gates. Private documented fixtures and source/unit proofs do not certify live private units or connectivity. External-public overlays and default/deletion migration remain separate blockers. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts`; `exchanges/rust/rust.exchange.ts` option forwarding; `sdk-rs/venues/bybit/` category states | **Combined-product gap closed for the `bybit` linear profile:** saved flags are forwarded and all four inverse/options combinations retain independent public categories and shared private-account reconciliation. Typed option terms, quote/position/account Greeks, chain/expiries and native option order sizing/caller IDs are explicit. | Actual TS/native WASM fixtures compare discovery, private read categories, option APIs and price/limit-IV order payloads; Rust option seeds and depth-25 books have native protocol/frame proofs because TS generic tickers/sockets omit or misroute that product. Unsupported option controls/settings/queries fail explicitly; standalone demo/inverse/spot/Hyro keep their narrower profiles. Preserve external-public policy and live combined/private validation boundaries; do not infer default/deletion readiness. [Evidence](RUST_PRODUCT_SEMANTICS.md). |
| `sdk-rs/venues/grvt/src/lib.rs` (`on_item` catalog filter); `packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts` (`fetchMarkets` catalog mapping), `packages/safe-cex/src/exchanges/grvt/CLAUDE.md` | Product gap resolved by [explicit retirement decision](GRVT_PRODUCTS_DECISION.md): TS posts `{}` and the official API defaults to `PERPETUAL`. Its generic mapper accepts injected Future/Call/Put rows but drops semantic metadata and cannot correctly cancel their symbols; explicit public non-perpetual catalogs were empty on production/testnet. | Retire only the unexposed mapper-only paths from cutover parity. Preserve TS, public exports and all other consumer/dependency gates. Any future enablement needs real listings, typed kind/expiry/strike/settlement metadata through frames/consumers and complete product lifecycle evidence; generic dispatch and perpetual replay do not prove it. |

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
source audit. The application-level adoption described in phase 6 concerns
tealstreet-next, outside this mirror; this audit cannot certify that downstream
consumer. Existing in-repo consumers still need their factory and store contract.

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
source counts for that checkpoint; the current runtime totals are 49 names / 47 Rust aliases.

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
