# Public async SDK wrapper census

Frozen baseline: `cd5ca4bdf633d377da51da417678023f6c8686b5`. The companion [JSON snapshot](public-async-wrapper-census.json) records all 32 production `impl Exchange` implementations in 29 venue source files, with exact declaration lines and whole-file SHA-256 values. It excludes the two Lighter websocket `tests.rs` fixtures. Configuration aliases share their concrete implementation; this is not a registry-name count.

This is a source census for routing real-binding investigations, not an executed verdict for every method. A deferred Rust future is an observable divergence only when a native probe establishes a synchronous prefix, detached continuation, or disposal race that differs from the maintained TypeScript class. None of the unprobed candidates below is marked FIX-complete or ACCEPT.

## Two existing execution families

For typed native async bodies, use the shared `safe_cex_exchange::eager::start_eager` introduced in [#1050](https://github.com/Tealstreet/tealstreet-sdk/pull/1050). Clone the existing owner into its static body, pass that owner’s existing `Rc<dyn ExchangeRuntime>`, and retain `T` directly. The boundary delegates once to the canonical Promise driver. Dropping the returned waiter must not cancel the running operation; do not introduce per-venue spawn/poll helpers.

For a binding whose member already returns a source Promise, invoke the actual member before constructing the Rust waiter, capture its `Result<Js, Thrown>`, then await that same Promise with the owning runtime. Do not spawn another Promise around it. WEEX’s common `invoke` and `unit` helpers use this form in the accompanying fix.

The shared boundary eagerly polls the typed body to its first real suspension. It cannot infer source `await` fences inside arbitrary immediately-ready native leaf futures. Each owner must check actual ready and held HTTP, continuation ownership, Store effects and disposal where relevant. Preserve existing source Promise/Realm/task drivers rather than replacing them indiscriminately.

## Measured family regressions

- Phemex: common `invoke` / `invoke_unit` now call the existing JS member before boxing its waiter. `binding/divergence_regressions.rs` proves original client/initializer calls before SDK waiter polling and error emission after dropping a pending ticker waiter; real HTTP retains its interceptor microtasks. The installed private reconciliation Promise already retained its eager prefix, silent catch and post-await disposal guard (ACCEPT, independently mutation controlled). See `safe-cex-venue-phemex/DIVERGENCE_FIX_AUDIT.md`.

- BloFin: `tests/divergence_timing_binding.rs` calls the actual public SDK, Store and signed HTTP path. Compiled master observes zero logs before polling; maintained TypeScript and #1050 observe three. Four production-native scheduler tests separately cover drop continuation, native non-Clone/non-serializable values and number bits, error metadata/identity, and one ready-result await boundary. All 26 boxed SDK async returns are migrated in #1050; this does not close every method-specific FIX-audit row.
- WEEX: `tests/eager_sdk_binding.rs` calls the actual constructed public SDK. Compiled baseline leaves `Store.options.isHedged` false where the source changes it synchronously, and dropping an unpolled orders waiter sends zero requests where the maintained class sends both signed normal/algo requests. The source oracle executes the whole maintained class with only terminal Axios replaced. These witnesses cover both common helpers; they do not claim all deeper helper contracts are unrestricted JavaScript parity.

- AsterDEX: `tests/eager_sdk_binding.rs` executes NativeRuntime, the constructed venue, production HTTP host, signing and reqwest against a local HTTP server. The whole maintained TS class proves validation logs before caller polling and discarded getAccount/changePositionMode promises complete their HTTP and Store work. Both native assertions fail on the frozen deferred wrapper. The repaired common invoke/unit helpers call the existing JS member immediately and await that same Promise; no extra spawn is introduced. This measures the SDK boundary, not every deeper socket/error claim.

## All production implementations at the frozen baseline

Counts are implementation counts: seven typed-async implementations, sixteen deferred-JS implementations, two deferred-Realm implementations, three eager-JS implementations, three distinct mixed implementations, and one synchronous refusal. BloFin and WEEX are included in their original baseline categories, with fixes described above. “Eager JS invocation” describes only the common call boundary and is not a blanket ACCEPT for every method.

| Implementation | Exact source declaration | Baseline wrapper family | Entry to inspect |
| --- | --- | --- | --- |
| `ApexOmniExchange` | [safe-cex-venue-apexomni/src/binding/exchange.rs:97](../safe-cex-venue-apexomni/src/binding/exchange.rs) | typed async | inner native async methods; start / boxed typed methods |
| `AsterDEXExchange` | [safe-cex-venue-asterdex/src/native_exchange.rs:126](../safe-cex-venue-asterdex/src/native_exchange.rs) | deferred JS invocation | invoke / unit |
| `BackpackExchange` | [safe-cex-venue-backpack/src/adapter_exchange.rs:42](../safe-cex-venue-backpack/src/adapter_exchange.rs) | mixed native task | adapter.rs -> rest_reads.rs; get/post creators may precede boxed Task continuations |
| `BinanceExchange` | [safe-cex-venue-binance/src/exchange_trait.rs:108](../safe-cex-venue-binance/src/exchange_trait.rs) | typed async | async call / unit / income_call invoke ExchangeOwner after polling |
| `BinancePMExchange` | [safe-cex-venue-binance/src/exchange_trait.rs:352](../safe-cex-venue-binance/src/exchange_trait.rs) | typed async | async call / unit / income_call invoke ExchangeOwner after polling |
| `BinanceSpotExchange` | [safe-cex-venue-binance/src/exchange_trait.rs:492](../safe-cex-venue-binance/src/exchange_trait.rs) | typed async | async call / unit / income_call invoke ExchangeOwner after polling |
| `BinanceInverseExchange` | [safe-cex-venue-binance/src/inverse/exchange.rs:39](../safe-cex-venue-binance/src/inverse/exchange.rs) | eager JS invocation | invoke / invoke_unit call member before boxing |
| `BingxExchange` | [safe-cex-venue-bingx/src/binding/exchange.rs:126](../safe-cex-venue-bingx/src/binding/exchange.rs) | typed async | inner reads / history native async methods |
| `BitgetExchange` | [safe-cex-venue-bitget/src/binding/exchange.rs:219](../safe-cex-venue-bitget/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit |
| `BitrueExchange` | [safe-cex-venue-bitrue/src/binding/exchange.rs:197](../safe-cex-venue-bitrue/src/binding/exchange.rs) | deferred JS invocation | invoke / unit |
| `BitunixVenue` | [safe-cex-venue-bitunix/src/adapter_exchange.rs:36](../safe-cex-venue-bitunix/src/adapter_exchange.rs) | typed async | adapter_rest.rs boxed native methods |
| `BlofinNativeExchange` | [safe-cex-venue-blofin/src/native_exchange.rs:92](../safe-cex-venue-blofin/src/native_exchange.rs) | typed async | inner native methods; all 26 boxed async returns migrated in #1050 |
| `BreakoutpropExchange` | [safe-cex-venue-breakoutprop/src/binding.rs:316](../safe-cex-venue-breakoutprop/src/binding.rs) | eager JS invocation | invoke / invoke_unit call member before boxing |
| `BybitExchange` | [safe-cex-venue-bybit/src/binding/exchange.rs:656](../safe-cex-venue-bybit/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit; configuration variants share implementation |
| `CoinbaseExchange` | [safe-cex-venue-coinbase/src/binding/exchange.rs:164](../safe-cex-venue-coinbase/src/binding/exchange.rs) | mixed custom Promise | result observes already-created CoinbaseStarbasePromise; inspect ticker_values / public_market_reads and per-method continuations |
| `CryptocomExchange` | [safe-cex-venue-cryptocom/src/binding/exchange.rs:219](../safe-cex-venue-cryptocom/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit |
| `GrvtExchange` | [safe-cex-venue-grvt/src/binding/exchange.rs:163](../safe-cex-venue-grvt/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit |
| `HyperliquidExchange` | [safe-cex-venue-hyperliquid/src/binding.rs:297](../safe-cex-venue-hyperliquid/src/binding.rs) | deferred Realm invocation | async Binding::invoke / typed / unit |
| `HyperliquidSpotExchange` | [safe-cex-venue-hyperliquid/src/spot_binding.rs:217](../safe-cex-venue-hyperliquid/src/spot_binding.rs) | deferred Realm invocation | shared Binding plus spot boxed typed methods / unit |
| `KrakenExchange` | [safe-cex-venue-kraken/src/binding/exchange.rs:277](../safe-cex-venue-kraken/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit |
| `KucoinExchange` | [safe-cex-venue-kucoin/src/kucoin_exchange.rs:347](../safe-cex-venue-kucoin/src/kucoin_exchange.rs) | deferred JS invocation | invoke / invoke_unit; Futures and Spot implementations |
| `KucoinSpotExchange` | [safe-cex-venue-kucoin/src/kucoin_exchange.rs:464](../safe-cex-venue-kucoin/src/kucoin_exchange.rs) | deferred JS invocation | invoke / invoke_unit; Futures and Spot implementations |
| `LighterNativeExchange` | [safe-cex-venue-lighter/src/binding/exchange.rs:304](../safe-cex-venue-lighter/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit |
| `MexcExchange` | [safe-cex-venue-mexc/src/exchange.rs:182](../safe-cex-venue-mexc/src/exchange.rs) | mixed native assembly | native typed methods; exchange_binding.rs::typed / unit invoke body inside boxed async |
| `OkxExchange` | [safe-cex-venue-okx/src/binding/exchange.rs:216](../safe-cex-venue-okx/src/binding/exchange.rs) | eager JS invocation | call / invoke / invoke_unit call member before boxing |
| `OrderlyExchange` | [safe-cex-venue-orderly/src/native_exchange.rs:128](../safe-cex-venue-orderly/src/native_exchange.rs) | deferred JS invocation | invoke / unit |
| `ParadexExchange` | [safe-cex-venue-paradex/src/native_exchange.rs:143](../safe-cex-venue-paradex/src/native_exchange.rs) | deferred JS invocation | invoke / unit |
| `PhemexExchange` | [safe-cex-venue-phemex/src/binding/exchange.rs:223](../safe-cex-venue-phemex/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit |
| `PolymarketExchange` | [safe-cex-venue-polymarket/src/binding/exchange.rs:181](../safe-cex-venue-polymarket/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit |
| `TxFlowExchange` | [safe-cex-venue-txflow/src/exchange.rs:58](../safe-cex-venue-txflow/src/exchange.rs) | synchronous refusal | phase-zero reject_preflight called before returned rejected future |
| `WeexExchange` | [safe-cex-venue-weex/src/binding/exchange.rs:167](../safe-cex-venue-weex/src/binding/exchange.rs) | deferred JS invocation | invoke / unit; corrected in the accompanying real-binding regression |
| `WooExchange` | [safe-cex-venue-woo/src/binding/exchange.rs:201](../safe-cex-venue-woo/src/binding/exchange.rs) | deferred JS invocation | invoke / invoke_unit |

Backpack creates some HTTP operations before returning its task; Coinbase has its own already-created Promise family; MEXC mixes direct typed readers with `exchange_binding::{typed,unit}`. Their exact deeper files are recorded in the table. Hyperliquid’s two implementations share the async Realm invoke boundary and must preserve its driver/identity semantics. The three Binance linear-family implementations share `exchange_trait::{call,unit,income_call}`; Binance inverse uses a separate in-family binding that already calls the member synchronously. TxFlow’s phase-zero preflight refusal is synchronous and does not establish coverage for unimplemented future REST methods.

Refresh this snapshot only after reviewing changed wrapper bodies. Its hashes identify the inspected baseline; they are not an allowlist or a policy exemption. Source drift alone must not silently turn a candidate into an executed verdict.

Shared socket entrypoints also require an eager prefix. BaseWebSocket now starts
preparation/dial through the existing OwnedTasks registry and canonical
`eager::start_eager`; Bitget secondary and Backpack public prove the real ready
host prefix. This is a core dial boundary, not a new venue SDK wrapper family.
The shared native plain-socket boundary additionally exposes one CONNECTING
handle while its owned eager handshake is pending. Actual native Bitget
onMarketsChanged observes that handle, preserves the source topic and follows
the existing send/reconnect Error/Close path before upgrade completion. This
closes the separately compiled held-handshake finding; it adds no per-venue
scheduler. See `safe-cex-venue-bitget/EAGER_DIAL_AUDIT.md` and exchange-core's
WS_CONTRACT.md for source/native witnesses and independent controls.

## Native final-handle destruction

The coordinator lifetime ruling requires deterministic full teardown when the
final native Exchange handle is dropped. Venue `Drop` implementations must use the single
`safe_cex_exchange::native_final_drop(base, venue_cleanup)` entrypoint. It marks
the shared disposed flag, runs venue state/task cleanup, then completes native
base cleanup even if venue cleanup or Store observers return errors. This is a
native-only lifetime policy, classified once at the canonical core entrypoint in
the override gate; it makes no TypeScript dispose/super claim. Existing no-super
and chosen-base rejection controls remain mandatory. Backpack and BingX use this
entrypoint with real constructed-owner teardown probes. Stored native timers use
weak owners so they cannot prevent final-handle destruction; they must still run
while the caller holds the original Exchange.

Backpack's measured mixed-native Task repair covers all 23 owned Task entrypoints
using shared start_eager. Its real held HTTP probes preserve request-start timing
and verify post-response catch/log and reconciliation after waiter drop. The
original source timer callback body is shared with the weak native timer binding;
no duplicate subscription or reconciliation algorithm is introduced.
