# P1 type ownership plan

Base: mirror origin/master d08a71e1d176f53d5fd57338872ae7a570c72e36, fetched 2026-11-08. User update: FULL GO; Position normal PR, NEEDS-NX first, NX lands first, CLEAR hostile review required. No merge or deployment by this worker.

## Source audit

The required SAFE_CEX_RUST_PORT_v1.md is absent from current origin/master (both root and docs/plans). Section 6 was recovered from mirror commit c77ebb53571, docs/plans/SAFE_CEX_RUST_PORT_v1.md. It requires Rust-owned models and real runtime TS enums. Current origin/master TYPE_PARITY.md and P1A_FIELDS.md were read. Their historical parity claims do not describe today's tree completely: see live AST comparisons below.

There are TWO Rust Position definitions: safe-cex-types/src/lib.rs and engine/src/model.rs. Engine does not depend on safe-cex-types; the latter has a deliberately isolated serde feature set. Do not introduce whole-workspace feature unification to connect them. The native engine output projection and generated contract must be covered together, with separate Cargo invocations. The packed frame adapter currently fabricates ID fallback, display-number defaults and quantity extras; a type re-export alone cannot remove these semantics.

The script CORE_TYPES_DTS is ALREADY generated: script-authoring/scripts/render-script-types-bundle.ts calls script-api-core/scripts/generate-monaco-types.ts and script-api-web/scripts/generate-monaco-types.ts. No hand-kept Position entry remains in that pipeline. Regenerate its bundle with corepack yarn generate:script-types; never edit the literal bundle by hand.

## Evidence and measurement

Tracked TS/TSX/MTS files in packages and apps/cli; generated script bundle excluded. Exact word grep counts plus TypeScript checker identifiers resolving (including aliases) to each declaration in safe-cex/src/types.ts. Includes tests. Compiler counts exclude property names and count import/export/type identifiers.

Counts below are unique files / resolved identifier occurrences. These are static type consumers, not estimated runtime call counts. Lexical grep counts are kept independently; strings/comments and similarly named concepts explain extra matches. Program created with safe-cex tsconfig, across tracked SDK packages and apps/cli. Unresolved imports are not counted as resolved consumers; the full package compiler is a separate required gate. Baseline full typecheck was attempted while install was unfinished and failed because turbo was not yet linked; repeat after install. The existing broad parity checker fails before model comparison at missing oracle LocalSignerMetadata; do not label that gate green or silently remove fields to satisfy it.

## Shared file ownership and parallel execution

One integrator (Position worker initially) owns all shared-file wiring: packages/safe-cex/src/types.ts; sdk-rs/safe-cex-types/src/lib.rs; examples/generate.rs; generate.cjs; tests/check-types.cjs; packages/safe-cex/src/store/frameApplier.ts; store.base.ts; store.interface.ts; sdk-rs/engine/src/model.rs; store.rs; ingest.rs; wire.rs; sdk-rs/hosts/wasm/js/wire.mjs and wire.d.mts; package.json; script-authoring/scripts/render-script-types-bundle.ts; script-api-core/scripts/generate-monaco-types.ts; script-api-web/scripts/generate-monaco-types.ts; script-authoring/src/script-types-bundle.ts. These paths conflict across every relevant slice and must NOT be edited independently by parallel workers.

Split before parallel integration: one generated module per public type under packages/safe-cex/src/generated/, named re-exports in types.ts, and one Rust source module per slice. Move each frame section into a type-specific module; keep frameApplier.ts as sequencing/error/readiness orchestration. Native output shaping belongs in per-type Rust output modules, not new TS coercion helpers. Each worker owns its module and tests; the integrator makes only imports/calls/re-exports in shared files. This is source organization, not a dual runtime path: when a type moves, delete its old declaration and old execution path in the same integration commit. No fallback/feature flags/migration-state registry.

Generate only changed type outputs for review, but run complete generator freshness checks at integration. Shared enums: Position owns PositionSide/PositionMode/MarginType; Order owns OrderSide/OrderStatus/OrderType/OrderTimeInForce/OrderStopTriggerType/OrderRejectReason. OptionGreeks is shared by Position/Market: Position establishes one generated module; Market must reuse it. Other workers cannot overwrite these modules. Bundle regeneration is serialized by the integrator AFTER source patches are combined. Never merge separately regenerated giant string literals.

Consumer file conflicts: any file listed under multiple slices in the appendix is integrator-owned for edits. Workers propose narrow patches in their reports; one integrator applies the combined patch. Tests unique to a slice can run concurrently. The Rust engine store/wire changes are serialized; do not change packed protocol independently.

Slice order is dependency/integration order: 1 Position, 2 Order, then 3 Balance, 4 Market, 5 Ticker, 6 Fill/execution, 7 Books/candles, 8 History, 9 Accounts/options, 10 Store/remainder. After the shared-file split, 3–9 can be implemented in isolated modules concurrently with integration in that order. Slice 6 requires Order enums; Market requires Position's option dependency; Store is last. No later-slice worker may commit a modification to another worker's owned files.

## Slice contracts, ownership, tests and NX impact

### 1 Position

Types: `Position`, `DisplayedPosition`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/position.rs (new canonical contract module)`
- `packages/safe-cex/src/generated/Position.ts (new)`
- `packages/safe-cex/src/generated/PositionSide.ts (new)`
- `packages/safe-cex/src/generated/PositionMode.ts (new)`
- `packages/safe-cex/src/generated/MarginType.ts (new)`
- `packages/safe-cex/src/store/positionFrames.ts (new, binary layout only)`
- `packages/safe-cex/src/store/positionFrames.test.ts (new)`
- `sdk-rs/engine/src/position_output.rs (new native output semantics)`
- `sdk-rs/safe-cex-types/tests/position-contract.cjs (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| Position | 150 | 118 | 485 |
| DisplayedPosition | 5 | 5 | 20 |

Current generated vs hand-written differences:

- Position: liquidationPrice: TS number | null; generated number.
- DisplayedPosition: No existing generated declaration.

Public type changes / NX impact: Preserve current number | null liquidationPrice, readonly and optional fields, runtime enums, option.greeks and native quantity units. No renamed or removed Position fields planned. Unknown remains null. Preserve DefaultStore full-refresh merge/retain behavior, including closed-symbol settings; native frame snapshots keep their existing replacement path.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

Position red-first cases: unknown liquidation (zero/negative/NaN/infinity -> null), valid positive unchanged, no size/unit repair in JS, true missing side refused, optional realizedPnl/marginSize/lastUpdate omission, inverse/contract quantities from native data, empty full refresh zeroes retained exposure while preserving settings, serialize and worker/main frame paths agree, script validator accepts null but rejects unchecked arithmetic, CLI $liq refuses unknown, OEMS keeps null. Existing anchors: store.base.test.ts; __tests__/audit-regressions/rust.frame-finite.test.ts; __tests__/audit-regressions/rust-frame.inverse-units.test.ts; sdk-rs/harness/frame-wire.integration.mjs; cli-core/src/preprocess/substitute.test.ts; cli-core/src/utils/PromptFormatter.test.ts; oems-core/src/boundContext.test.ts; script-authoring/src/validate/validate.test.ts; script-authoring/src/script-types-bundle.test.ts.

### 2 Order

Types: `Order`, `PlaceOrderOpts`, `UpdateOrderOpts`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/order.rs (new)`
- `packages/safe-cex/src/generated/Order.ts (new and dependency enum modules)`
- `packages/safe-cex/src/store/orderFrames.ts (new)`
- `packages/safe-cex/src/store/orderFrames.test.ts (new)`
- `sdk-rs/engine/src/order_output.rs (new)`
- `sdk-rs/safe-cex-types/tests/order-contract.cjs (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| Order | 228 | 149 | 785 |
| PlaceOrderOpts | 87 | 81 | 420 |
| UpdateOrderOpts | 48 | 47 | 105 |

Current generated vs hand-written differences:

- Order: No field-shape difference in committed final bindings.
- PlaceOrderOpts: No field-shape difference in committed final bindings.
- UpdateOrderOpts: No field-shape difference in committed final bindings.

Public type changes / NX impact: Keep exact runtime enums, readonly, optional triggers/extras, native placement size and base fills. NX must preserve absent vs zero triggers, native remaining/filled quantities, and terminal order identity. No static shape change currently needed; verify scope again at slice start.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

### 3 Balance

Types: `Balance`, `CoinBalance`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/balance.rs (new)`
- `packages/safe-cex/src/generated/Balance.ts (new)`
- `packages/safe-cex/src/generated/CoinBalance.ts (new)`
- `packages/safe-cex/src/store/balanceFrames.ts (new)`
- `packages/safe-cex/src/store/balanceFrames.test.ts (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| Balance | 91 | 64 | 177 |
| CoinBalance | 8 | 8 | 20 |

Current generated vs hand-written differences:

- Balance: No field-shape difference in committed final bindings.
- CoinBalance: No field-shape difference in committed final bindings.

Public type changes / NX impact: No planned field-shape change. Preserve mutable CoinBalance and dictionary keys/order. NX must not fabricate free/used/total/upnl before native account readiness; remove TS balance defaults only with a native empty-balance producer.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

### 4 Market

Types: `Market`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/market.rs (new)`
- `packages/safe-cex/src/generated/Market.ts (new and OptionGreeks-related shared dependency as applicable)`
- `packages/safe-cex/src/store/marketFrames.ts (new)`
- `packages/safe-cex/src/store/marketFrames.test.ts (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| Market | 397 | 232 | 827 |

Current generated vs hand-written differences:

- Market: No field-shape difference in committed final bindings.

Public type changes / NX impact: No planned shape change. Preserve optional option/future metadata, precision and min/max semantics, native instrument identity. NX must not infer missing expiry/strike or equate zero with absent limits.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

### 5 Ticker

Types: `Ticker`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/ticker.rs (new)`
- `packages/safe-cex/src/generated/Ticker.ts (new)`
- `packages/safe-cex/src/store/tickerFrames.ts (new)`
- `packages/safe-cex/src/store/tickerFrames.test.ts (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| Ticker | 125 | 111 | 380 |

Current generated vs hand-written differences:

- Ticker: Missing generated observedAt.

Public type changes / NX impact: Add missing observedAt to Rust generation, preserving the current optional TS tuple `[number | null, number | null, number | null]` (last/mark/index). Keep midPrice optional; explicit null clears transport midpoint but is not a stored numeric quote. NX must preserve independent receipt timestamps and not advance them on snapshots.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

### 6 Fill and execution

Types: `OrderFillEvent`, `Execution`, `PublicTrade`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/execution.rs (new, including missing OrderFillEvent)`
- `packages/safe-cex/src/generated/OrderFillEvent.ts (new)`
- `packages/safe-cex/src/generated/Execution.ts (new)`
- `packages/safe-cex/src/generated/PublicTrade.ts (new)`
- `packages/safe-cex/src/store/fillFrames.ts (new)`
- `packages/safe-cex/src/store/fillFrames.test.ts (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| OrderFillEvent | 17 | 16 | 63 |
| Execution | 52 | 37 | 146 |
| PublicTrade | 10 | 10 | 25 |

Current generated vs hand-written differences:

- OrderFillEvent: No existing generated declaration.
- Execution: No field-shape difference in committed final bindings.
- PublicTrade: No field-shape difference in committed final bindings.

Public type changes / NX impact: OrderFillEvent is a separate event contract from Execution; do not substitute one for the other. Add the missing Rust event contract before cutover. NX must retain id vs orderId, fees, venue-native execution identity, optional position side. Current names stay.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

### 7 Books and candles

Types: `Candle`, `OrderBook`, `OrderBookOrders`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/market_data.rs (new)`
- `packages/safe-cex/src/generated/Candle.ts (new)`
- `packages/safe-cex/src/generated/OrderBook.ts (new)`
- `packages/safe-cex/src/generated/OrderBookOrders.ts (new)`
- `packages/safe-cex/src/store/bookFrames.ts (new)`
- `packages/safe-cex/src/store/candleFrames.ts (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| Candle | 114 | 109 | 346 |
| OrderBook | 121 | 118 | 370 |
| OrderBookOrders | 7 | 6 | 15 |

Current generated vs hand-written differences:

- Candle: No field-shape difference in committed final bindings.
- OrderBook: No field-shape difference in committed final bindings.
- OrderBookOrders: No field-shape difference in committed final bindings.

Public type changes / NX impact: Keep mutable book levels, finite zero prices, zero-size deletes and timestamp units. NX must preserve snapshot/delta baselines and not share transferred ArrayBuffers with retained books. No planned nullable/rename/removal changes.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

### 8 History and income

Types: `HistoricalOrder`, `HistoricalTrade`, `ClosedPnlEntry`, `IncomeRecord`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/history.rs (new)`
- `packages/safe-cex/src/generated/HistoricalOrder.ts (new)`
- `packages/safe-cex/src/generated/HistoricalTrade.ts (new)`
- `packages/safe-cex/src/generated/ClosedPnlEntry.ts (new)`
- `packages/safe-cex/src/generated/IncomeRecord.ts (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| HistoricalOrder | 23 | 23 | 59 |
| HistoricalTrade | 13 | 7 | 20 |
| ClosedPnlEntry | 18 | 18 | 50 |
| IncomeRecord | 31 | 29 | 110 |

Current generated vs hand-written differences:

- HistoricalOrder: No field-shape difference in committed final bindings.
- HistoricalTrade: No field-shape difference in committed final bindings.
- ClosedPnlEntry: No field-shape difference in committed final bindings.
- IncomeRecord: No field-shape difference in committed final bindings.

Public type changes / NX impact: No planned shape changes; keep fee/funding/gross/net distinction, optional identities and units. NX journal consumers must not infer execution identity from order identity or merge unrelated records.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

### 9 Accounts and options

Types: `ExchangeOptions`, `ExchangeAccount`, `ExchangeAccountConfig`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/options.rs (new)`
- `packages/safe-cex/src/generated/ExchangeOptions.ts (new)`
- `packages/safe-cex/src/generated/ExchangeAccount.ts (new)`
- `packages/safe-cex/src/generated/ExchangeAccountConfig.ts (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| ExchangeOptions | 267 | 264 | 842 |
| ExchangeAccount | 10 | 10 | 23 |
| ExchangeAccountConfig | 26 | 25 | 147 |

Current generated vs hand-written differences:

- ExchangeOptions: Missing generated localSigner; Missing generated polymarket; Missing generated hyperliquidFastBook; Missing generated callerReportsPlacementFailures; Missing generated workerMirror; Missing generated engine; Missing generated publicFeed; extra: TS {
    /** `Infinity` turns periodic REST polling off, for callers that fetch on demand. */
    tickInterval?: number;
    /** Rust command waiting deadline in ms (default 120000); expiry never retries or cancels a write. */
    rustCommandTimeoutMs?: number;
    recvWindow?: number;
    // Optional AsterDEX REST API version override.
    asterdexApiVersion?: 'v1' | 'v3';
    // Optional Aster Code builder attribution overrides.
    // Aster accepts one feeRate per order; maker/taker fields are Tealstreet placement policy.
    asterBuilderAddress?: string;
    asterBuilderFeeRateMaker?: string;
    asterBuilderFeeRateTaker?: string;
    // Lighter builder/integrator fee mode.
    lighterBuilderFeeMode?: 'dynamic' | 'static';
    // Optional override for Lighter integrator account index.
    lighterIntegratorAccountIndex?: string;
    // Lighter TP/SL allowed execution slippage, as a percentage from trigger price.
    lighterStopSlippagePct?: number;
    // HyroTrader uses the same connector for the shared simulator demo and real user-pasted challenge keys.
    hyrotraderMode?: 'demo' | 'production';
    /**
     * KuCoin broker attribution config.
     *
     * Browser builds may supply this from NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_*
     * or NEXT_PUBLIC_KUCOIN_SPOT_BROKER_* env vars when product policy permits
     * sending signed broker attribution from the client runtime.
     */
    kucoinBroker?: {
      readonly partner: string;
      readonly brokerKey: string;
      readonly brokerName: string;
      readonly verify?: boolean;
    };
  } (optional); generated {
        /**
         * @ts-mutable
         */
        tickInterval?: number;
        /**
         * @ts-mutable
         */
        recvWindow?: number;
        /**
         * @ts-mutable
         */
        asterdexApiVersion?: "v1" | "v3";
        /**
         * @ts-mutable
         */
        asterBuilderAddress?: string;
        /**
         * @ts-mutable
         */
        asterBuilderFeeRateMaker?: string;
        /**
         * @ts-mutable
         */
        asterBuilderFeeRateTaker?: string;
        /**
         * @ts-mutable
         */
        lighterBuilderFeeMode?: "dynamic" | "static";
        /**
         * @ts-mutable
         */
        lighterIntegratorAccountIndex?: string;
        /**
         * @ts-mutable
         */
        lighterStopSlippagePct?: number;
        /**
         * @ts-mutable
         */
        hyrotraderMode?: "demo" | "production";
        /**
         * @ts-mutable
         */
        kucoinBroker?: {
            readonly partner: string;
            readonly brokerKey: string;
            readonly brokerName: string;
            readonly verify?: boolean;
        };
    } (optional).
- ExchangeAccount: No field-shape difference in committed final bindings.
- ExchangeAccountConfig: No field-shape difference in committed final bindings.

Public type changes / NX impact: Generation currently omits localSigner, polymarket, hyperliquidFastBook, callerReportsPlacementFailures, workerMirror, engine, publicFeed, extra.rustCommandTimeoutMs. Add them with current optionality; do not drop public fields. Runtime capabilities (callbacks/ports/signers) retain explicit TS-owned imports, not JSON records. NX must regenerate against the final boundary and preserve private signer setup.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

### 10 Store and remaining public surface

Types: `StoreData`, `StoreOptions`, `StoreDataLoaded`, `StoreUpdateIds`, `FetchedLeverage`, `DisplayedMargin`.

Owned source/output/test paths (existing unless marked new):

- `sdk-rs/safe-cex-types/src/store_model.rs`
- `packages/safe-cex/src/generated/StoreData.ts (new)`
- `packages/safe-cex/src/generated/StoreOptions.ts (new)`
- `packages/safe-cex/src/generated/StoreDataLoaded.ts (new)`
- `packages/safe-cex/src/generated/StoreUpdateIds.ts (new)`
- `packages/safe-cex/src/generated/FetchedLeverage.ts (new)`
- `packages/safe-cex/src/generated/DisplayedMargin.ts (new)`

All listed shared wiring is integrator-owned. Consumer paths below are owned by this slice only when no other slice lists them; otherwise integrator-owned.

| Type | Grep files | Compiler files | Compiler references |
|---|---:|---:|---:|
| StoreData | 38 | 38 | 165 |
| StoreOptions | 1 | 1 | 3 |
| StoreDataLoaded | 3 | 3 | 7 |
| StoreUpdateIds | 1 | 1 | 2 |
| FetchedLeverage | 12 | 12 | 52 |
| DisplayedMargin | 2 | 1 | 2 |

Current generated vs hand-written differences:

- StoreData: No existing generated declaration.
- StoreOptions: No field-shape difference in committed final bindings.
- StoreDataLoaded: No field-shape difference in committed final bindings.
- StoreUpdateIds: No field-shape difference in committed final bindings.
- FetchedLeverage: No field-shape difference in committed final bindings.
- DisplayedMargin: No field-shape difference in committed final bindings.

Public type changes / NX impact: StoreData has Rust representation but is not currently exported in committed bindings. Generate it after its object dependencies; retain runtime capability boundaries. NX must consume complete native initial state and authoritative update IDs instead of defaults. Perform an exhaustive export census before declaring all of types.ts generated.

Proof required: a scoped compile-time bidirectional contract test (readonly, optionality, enum member/runtime identity, and bare-string rejection); native output fixture -> real packed-frame decode -> DefaultStore/serialization test; slice consumer tests using real generated declarations and null/absent/zero edge cases. Run corepack yarn typecheck, corepack yarn lint, corepack yarn test, and generation/bundle freshness checks. Cargo commands use -j2 and --locked, isolated by crate to respect feature policy. Tests for changed behavior must fail on the base first.

## Per-slice, per-area consumer census

Areas safe-cex/frame/store are disjoint in this table; other SDK packages are retained, including script-api-core which exports the public authoring surface. Each item is an exact compiler-resolved path with its identifier count. The generated bundle is intentionally separate and not counted as a source consumer.

### 1 Position

#### Position

**packages/cli-core: 23 files / 103 references.**

- `packages/cli-core/src/commands/nativeAccountState.test.ts` (2)
- `packages/cli-core/src/commands/trading/BreakevenCommand.test.ts` (5)
- `packages/cli-core/src/commands/trading/ChartCommand.test.ts` (2)
- `packages/cli-core/src/commands/trading/ChartCommand.ts` (3)
- `packages/cli-core/src/commands/trading/MonitorCommand.format.test.ts` (4)
- `packages/cli-core/src/commands/trading/MonitorCommand.ts` (2)
- `packages/cli-core/src/commands/trading/PositionCommand.test.ts` (2)
- `packages/cli-core/src/commands/trading/StopCommand.test.ts` (5)
- `packages/cli-core/src/commands/trading/SwarmCommand.test.ts` (6)
- `packages/cli-core/src/commands/trading/TrailCommand.test.ts` (5)
- `packages/cli-core/src/commands/trading/sizeSyntax.test.ts` (5)
- `packages/cli-core/src/commands/trading/smallCommands.test.ts` (5)
- `packages/cli-core/src/commands/utility/WaitCommand.test.ts` (12)
- `packages/cli-core/src/preprocess/ifElse.test.ts` (5)
- `packages/cli-core/src/preprocess/loop.test.ts` (5)
- `packages/cli-core/src/preprocess/math.test.ts` (5)
- `packages/cli-core/src/preprocess/preprocess.test.ts` (5)
- `packages/cli-core/src/preprocess/preprocessIntegration.test.ts` (5)
- `packages/cli-core/src/preprocess/reactiveVars.test.ts` (5)
- `packages/cli-core/src/preprocess/substitute.test.ts` (6)
- `packages/cli-core/src/preprocess/trackedVars.test.ts` (5)
- `packages/cli-core/src/types.ts` (2)
- `packages/cli-core/src/utils/PromptFormatter.ts` (2)

**packages/safe-cex: 78 files / 322 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/binance.live-smoke-leverage.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.attached-tpsl-payload.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.trading-safety-defaults.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.parity.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.remaining-audit.test.ts` (7)
- `packages/safe-cex/src/__tests__/audit-regressions/bybit.initial-position-snapshot.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bybitdemo.order-risk.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.private-recovery.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.account-equity.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/orderly.isolated-margin.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.account-state.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.leverage-side.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/consumerFrame.ts` (3)
- `packages/safe-cex/src/__tests__/cutover/metadata.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.ws-private.ts` (5)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (14)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.ws-private.ts` (6)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetinverse.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (9)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.ws-private.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bybit/syncUnifiedBalanceUpnlFromPositions.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.ws-private.ts` (6)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.ws-private.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (9)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.ws-private.ts` (5)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.utils.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.ws-private.ts` (4)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts` (4)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (5)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (5)
- `packages/safe-cex/src/exchanges/base.ts` (5)
- `packages/safe-cex/src/exchanges/bybit/bybit.inverse.ts` (2)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (5)
- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (4)
- `packages/safe-cex/src/utils/risk.test.ts` (8)
- `packages/safe-cex/src/utils/risk.ts` (10)
- `packages/safe-cex/src/utils/roeRoi/calculateRelativeReturns.test.ts` (3)
- `packages/safe-cex/src/utils/roeRoi/calculateRelativeReturns.ts` (2)
- `packages/safe-cex/src/utils/roeRoi/calculateReturns.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.test.ts` (4)
- `packages/safe-cex/src/writer/writeContext.ts` (3)
- `packages/safe-cex/src/writer/writer.test.ts` (3)

**frame applier: 1 files / 5 references.**

- `packages/safe-cex/src/store/frameApplier.ts` (5)

**store: 4 files / 23 references.**

- `packages/safe-cex/src/store/store.base.test.ts` (4)
- `packages/safe-cex/src/store/store.base.ts` (9)
- `packages/safe-cex/src/store/store.interface.ts` (8)
- `packages/safe-cex/src/store/store.stub.ts` (2)

**packages/script-api-core: 12 files / 32 references.**

- `packages/script-api-core/src/__tests__/fixtures.ts` (3)
- `packages/script-api-core/src/__tests__/mockAdapter.ts` (2)
- `packages/script-api-core/src/adapter/AdapterContext.ts` (2)
- `packages/script-api-core/src/adapter/createCoreAdapter.test.ts` (4)
- `packages/script-api-core/src/adapter/createCoreAdapter.ts` (5)
- `packages/script-api-core/src/api/CoreApi.types.ts` (4)
- `packages/script-api-core/src/index.ts` (1)
- `packages/script-api-core/src/order/downSizeReduceOrder.test.ts` (3)
- `packages/script-api-core/src/order/downSizeReduceOrder.ts` (2)
- `packages/script-api-core/src/order/prepareOrder.reduceLock.test.ts` (2)
- `packages/script-api-core/src/order/prepareOrder.ts` (3)
- `packages/script-api-core/src/script.ts` (1)

#### DisplayedPosition

**store: 1 files / 2 references.**

- `packages/safe-cex/src/store/store.stub.ts` (2)

**packages/safe-cex: 4 files / 18 references.**

- `packages/safe-cex/src/types.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedMarginPostprocessor.test.ts` (6)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.ts` (6)

### 2 Order

#### Order

**packages/cli-core: 16 files / 79 references.**

- `packages/cli-core/src/commands/nativeAccountState.test.ts` (2)
- `packages/cli-core/src/commands/trading/BreakevenCommand.test.ts` (6)
- `packages/cli-core/src/commands/trading/BreakevenCommand.ts` (2)
- `packages/cli-core/src/commands/trading/BumpCommand.test.ts` (5)
- `packages/cli-core/src/commands/trading/CancelCommand.test.ts` (32)
- `packages/cli-core/src/commands/trading/CancelCommand.ts` (3)
- `packages/cli-core/src/commands/trading/ChartCommand.test.ts` (3)
- `packages/cli-core/src/commands/trading/ChartCommand.ts` (2)
- `packages/cli-core/src/commands/trading/ChaseCommand.format.test.ts` (2)
- `packages/cli-core/src/commands/trading/MonitorCommand.format.test.ts` (3)
- `packages/cli-core/src/commands/trading/MonitorCommand.ts` (2)
- `packages/cli-core/src/commands/trading/smallCommands.test.ts` (6)
- `packages/cli-core/src/types.ts` (3)
- `packages/cli-core/src/utils/PromptFormatter.ts` (2)
- `packages/cli-core/src/utils/formatBaseSize.test.ts` (4)
- `packages/cli-core/src/utils/formatBaseSize.ts` (2)

**packages/exchange-mcp: 1 files / 2 references.**

- `packages/exchange-mcp/src/tools.ts` (2)

**packages/safe-cex: 114 files / 626 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/binance.cancel-filled-race.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bingx.audit-regressions.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bingx.native-amend.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.trading-safety-defaults.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.native-modify.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.parity.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/bybit.recently-placed-order-preservation.test.ts` (21)
- `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.lookup.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.native-edit.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.place-order-response.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/cryptocom.order-writes.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.contract.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.private-recovery.test.ts` (7)
- `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.client-order-id.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.native-amend.audit-regression.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/kucoin.cancel-replace-contract-size.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/kucoinspot.rust-boundary.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/mexc.audit-regressions.test.ts` (23)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.account-equity.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/orderly.native-amend.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.batch-cancel.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.native-amend.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/reconciliation-fetch-results.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.account-state.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.asterdex-wrapper.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.bingx-hosted-provider.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.cancel-all-placement-races.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.engine-capabilities.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.fill-financials.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hyperliquid-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.parity-sweep.test.ts` (3)
- `packages/safe-cex/src/__tests__/cutover/consumerFrame.ts` (3)
- `packages/safe-cex/src/__tests__/cutover/metadata.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/faultBench/badLink.scenarios.test.ts` (4)
- `packages/safe-cex/src/__tests__/lighter.utils.test.ts` (6)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/binance/binance.utils.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (22)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetinverse.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (15)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.ws-private.ts` (6)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (13)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (18)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (15)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (15)
- `packages/safe-cex/src/__tests__/reference/bybit/hyrotrader.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.orders.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.starbase-private.ts` (3)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts` (5)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (12)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.ws-private.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.ws-private.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (15)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (31)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.utils.ts` (7)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.ws-private.ts` (5)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.ws-private.ts` (3)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts` (5)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.trading.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (7)
- `packages/safe-cex/src/__tests__/reference/weex/weex.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (10)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.private.ts` (4)
- `packages/safe-cex/src/exchanges/base.ts` (12)
- `packages/safe-cex/src/exchanges/clientOrderId.ts` (3)
- `packages/safe-cex/src/exchanges/lighter/lighter.utils.ts` (2)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (2)
- `packages/safe-cex/src/orderAmendment.test.ts` (3)
- `packages/safe-cex/src/orderAmendment.ts` (2)
- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (5)
- `packages/safe-cex/src/utils/risk.test.ts` (24)
- `packages/safe-cex/src/utils/risk.ts` (10)
- `packages/safe-cex/src/webworker/ExchangeWorker.test.ts` (3)
- `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance.ts` (3)
- `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.test.ts` (3)
- `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.ts` (10)
- `packages/safe-cex/src/webworker/postProcessors/ChaserOrderEnrichmentPostprocessor.ts` (6)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/TwapOrderEnrichmentPostprocessor.test.ts` (3)
- `packages/safe-cex/src/writer/writeContext.ts` (3)
- `packages/safe-cex/src/writer/writer.test.ts` (3)

**frame applier: 1 files / 10 references.**

- `packages/safe-cex/src/store/frameApplier.ts` (10)

**store: 4 files / 35 references.**

- `packages/safe-cex/src/store/store.base.test.ts` (3)
- `packages/safe-cex/src/store/store.base.ts` (17)
- `packages/safe-cex/src/store/store.interface.ts` (13)
- `packages/safe-cex/src/store/store.stub.ts` (2)

**packages/script-api-core: 13 files / 33 references.**

- `packages/script-api-core/src/__tests__/fixtures.ts` (2)
- `packages/script-api-core/src/__tests__/mockAdapter.ts` (2)
- `packages/script-api-core/src/adapter/AdapterContext.ts` (2)
- `packages/script-api-core/src/adapter/HostBindings.ts` (3)
- `packages/script-api-core/src/adapter/createCoreAdapter.test.ts` (4)
- `packages/script-api-core/src/adapter/createCoreAdapter.ts` (4)
- `packages/script-api-core/src/api/CoreApi.types.ts` (4)
- `packages/script-api-core/src/index.ts` (1)
- `packages/script-api-core/src/order/downSizeReduceOrder.test.ts` (2)
- `packages/script-api-core/src/order/downSizeReduceOrder.ts` (4)
- `packages/script-api-core/src/order/prepareOrder.reduceLock.test.ts` (2)
- `packages/script-api-core/src/order/prepareOrder.ts` (2)
- `packages/script-api-core/src/script.ts` (1)

#### PlaceOrderOpts

**apps/cli: 1 files / 3 references.**

- `apps/cli/src/cross-trade-daemon.ts` (3)

**packages/cli-core: 6 files / 25 references.**

- `packages/cli-core/src/commands/trading/BaseBuySellCommand.ts` (8)
- `packages/cli-core/src/commands/trading/CloseCommand.ts` (3)
- `packages/cli-core/src/commands/trading/ScaleCommand.test.ts` (3)
- `packages/cli-core/src/commands/trading/SwarmCommand.ts` (2)
- `packages/cli-core/src/order/prepareCliOrder.ts` (5)
- `packages/cli-core/src/types.ts` (4)

**packages/exchange-mcp: 1 files / 3 references.**

- `packages/exchange-mcp/src/tools.ts` (3)

**packages/safe-cex: 68 files / 370 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/backpack.native-batch.regression.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.fail-closed.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.forwarding.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/lighter.client-order-index.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/mexc.trading-reference.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.batch-orders.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.opening-trigger.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.single-order-client-id.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.account-state.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.apexomni-wrapper.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.cancel-all-placement-races.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.client-order-id-rules.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hyperliquid-wrapper.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.kucoinspot-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.lighter-wrapper.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.paradex-wrapper.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.product-options.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/weex.regular-batch-orders.test.ts` (3)
- `packages/safe-cex/src/__tests__/faultBench/badLink.scenarios.test.ts` (3)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (9)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (18)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (10)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (18)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (17)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/bybit/hyrotrader.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (14)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (12)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (15)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (11)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.trading.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (9)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (8)
- `packages/safe-cex/src/crossTrade/runtime/crossTradePlaceOrder.ts` (3)
- `packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts` (3)
- `packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts` (2)
- `packages/safe-cex/src/exchanges/base.ts` (13)
- `packages/safe-cex/src/exchanges/bybit/bybit.order-params.ts` (8)
- `packages/safe-cex/src/exchanges/clientOrderId.ts` (5)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (3)
- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (1)
- `packages/safe-cex/src/webworker/twap/TwapInstance.ts` (3)

**packages/script-api-core: 5 files / 19 references.**

- `packages/script-api-core/src/adapter/HostBindings.ts` (3)
- `packages/script-api-core/src/adapter/createCoreAdapter.ts` (5)
- `packages/script-api-core/src/order/downSizeReduceOrder.test.ts` (3)
- `packages/script-api-core/src/order/downSizeReduceOrder.ts` (3)
- `packages/script-api-core/src/order/prepareOrder.ts` (5)

#### UpdateOrderOpts

**packages/cli-core: 1 files / 3 references.**

- `packages/cli-core/src/types.ts` (3)

**packages/safe-cex: 41 files / 94 references.**

- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.trading.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/rust.exchange.wasm.test.ts` (2)
- `packages/safe-cex/src/exchanges/base.ts` (3)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (2)
- `packages/safe-cex/src/types.ts` (1)

**packages/script-api-core: 5 files / 8 references.**

- `packages/script-api-core/src/adapter/HostBindings.ts` (3)
- `packages/script-api-core/src/adapter/createCoreAdapter.ts` (2)
- `packages/script-api-core/src/api/CoreApi.types.ts` (1)
- `packages/script-api-core/src/index.ts` (1)
- `packages/script-api-core/src/script.ts` (1)

### 3 Balance

#### Balance

**packages/cli-core: 4 files / 17 references.**

- `packages/cli-core/src/commands/trading/SwarmCommand.test.ts` (5)
- `packages/cli-core/src/commands/trading/sizeSyntax.test.ts` (5)
- `packages/cli-core/src/commands/trading/smallCommands.test.ts` (5)
- `packages/cli-core/src/types.ts` (2)

**packages/safe-cex: 51 files / 143 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/backpack.state-reconciliation.regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/reconciliation-fetch-results.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/metadata.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.exchange.startup.test.ts` (4)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.ws-private.ts` (4)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/syncUnifiedBalanceUpnlFromPositions.ts` (3)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.utils.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.ws-private.ts` (4)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.private.ts` (2)
- `packages/safe-cex/src/crossTrade/getCopierWeightRatio.test.ts` (4)
- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (6)
- `packages/safe-cex/src/utils/getBalanceSlice.test.ts` (3)
- `packages/safe-cex/src/utils/getBalanceSlice.ts` (3)
- `packages/safe-cex/src/utils/getTotalBalanceFromBalance.test.ts` (3)
- `packages/safe-cex/src/utils/getTotalBalanceFromBalance.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.test.ts` (2)

**store: 1 files / 2 references.**

- `packages/safe-cex/src/store/store.stub.ts` (2)

**packages/script-api-core: 8 files / 15 references.**

- `packages/script-api-core/src/__tests__/fixtures.ts` (2)
- `packages/script-api-core/src/__tests__/mockAdapter.ts` (2)
- `packages/script-api-core/src/adapter/AdapterContext.ts` (2)
- `packages/script-api-core/src/adapter/HostBindings.ts` (2)
- `packages/script-api-core/src/adapter/createCoreAdapter.ts` (2)
- `packages/script-api-core/src/api/CoreApi.types.ts` (3)
- `packages/script-api-core/src/index.ts` (1)
- `packages/script-api-core/src/script.ts` (1)

#### CoinBalance

**packages/safe-cex: 8 files / 20 references.**

- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (2)
- `packages/safe-cex/src/types.ts` (2)

### 4 Market

#### Market

**apps/cli: 3 files / 6 references.**

- `apps/cli/src/cross-trade-daemon.ts` (2)
- `apps/cli/src/repl.ts` (2)
- `apps/cli/src/script-adapter.ts` (2)

**packages/cli-core: 11 files / 31 references.**

- `packages/cli-core/src/commands/trading/ChartCommand.test.ts` (3)
- `packages/cli-core/src/commands/trading/ChartCommand.ts` (4)
- `packages/cli-core/src/commands/trading/ChaseCommand.format.test.ts` (2)
- `packages/cli-core/src/commands/trading/MonitorCommand.format.test.ts` (2)
- `packages/cli-core/src/commands/trading/PositionCommand.test.ts` (2)
- `packages/cli-core/src/commands/utility/tasksAndKill.test.ts` (6)
- `packages/cli-core/src/types.ts` (2)
- `packages/cli-core/src/utils/PromptFormatter.ts` (2)
- `packages/cli-core/src/utils/formatBaseSize.test.ts` (2)
- `packages/cli-core/src/utils/formatBaseSize.ts` (3)
- `packages/cli-core/src/utils/resolveMarketSymbol.ts` (3)

**packages/safe-cex-server: 11 files / 42 references.**

- `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.test.ts` (6)
- `packages/safe-cex-server/src/publicExchange/PublicExchangeFactory.ts` (8)
- `packages/safe-cex-server/src/publicExchange/hydrateStore.test.ts` (4)
- `packages/safe-cex-server/src/publicExchange/hydrateStore.ts` (2)
- `packages/safe-cex-server/src/publicExchange/marketCache.test.ts` (2)
- `packages/safe-cex-server/src/publicExchange/marketCache.ts` (2)
- `packages/safe-cex-server/src/publicExchange/serialization.test.ts` (4)
- `packages/safe-cex-server/src/publicExchange/serialization.ts` (4)
- `packages/safe-cex-server/src/publicdata/publicdata.test.ts` (3)
- `packages/safe-cex-server/src/publicdata/tickerState.ts` (3)
- `packages/safe-cex-server/src/publicdata/types.ts` (4)

**packages/safe-cex: 181 files / 651 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/binance.live-smoke-leverage.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bingx.native-amend.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.attached-tpsl-payload.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.history-identity.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.leverage-read-probe.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.private-executions.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.rust-max-leverage.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.trading-safety-defaults.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.websocket-transport.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.native-modify.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.parity.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.remaining-audit.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/blofin.remaining-audit.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bybitdemo.order-risk.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.fail-closed.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/client-order-id.forwarding.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.cfm-private-ws-contract-size.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.native-edit.test.ts` (6)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.open-orders-polling.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.place-order-response.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/cryptocom.order-writes.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/exchange.listener-isolation.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/fill-event.incremental.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/grvt.exact-symbol-cancel.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.contract.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/hyrotrader.order-recovery.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.client-order-id.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.native-amend.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/kucoin.algo-update-rejection.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/kucoin.cancel-replace-contract-size.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/lighter.public-data.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx-rust-oracle.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.account-equity.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.closed-pnl.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.monthly-candles.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.orderbook-unsubscribe.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.remaining-audit.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.sequenced-orderbook-adoption-gate.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.split-remainder.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.websocket-routing.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okxeu.xperp-markets.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/order-history.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/order-payload-normalization.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/orderly.expected-order-rejections.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/orderly.isolated-margin.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/orderly.native-amend.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.native-amend.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.single-order-client-id.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/polymarket.public-book-state.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.account-state.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.apexomni-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.cancel-all-placement-races.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.client-order-id-rules.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.closed-pnl.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.fill-financials.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hyperliquid-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.in-process-public-feed.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.journal-paths.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.kucoinspot-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.leverage-side.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.market-refresh.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.paradex-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.polling-policy.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.startup-demand.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.ticker-observations.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.underlying-token.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/shared-public-account-overlay.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/spot-products.public-parity.test.ts` (7)
- `packages/safe-cex/src/__tests__/audit-regressions/txflow.account.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/txflow.history.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/woo.v3-public-data.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/woo.v3-public-parity.test.ts` (5)
- `packages/safe-cex/src/__tests__/blofin.contract-scaling-regression.test.ts` (21)
- `packages/safe-cex/src/__tests__/cutover/engineFailure.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/metadata.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/cutover/sharedPublicFeed.wasm.test.ts` (9)
- `packages/safe-cex/src/__tests__/faultBench/badLink.scenarios.test.ts` (2)
- `packages/safe-cex/src/__tests__/faultBench/binanceLookup.scenarios.test.ts` (2)
- `packages/safe-cex/src/__tests__/faultBench/bybitClockSync.scenarios.test.ts` (2)
- `packages/safe-cex/src/__tests__/faultBench/bybitLostCreate.scenarios.test.ts` (2)
- `packages/safe-cex/src/__tests__/in-process-public-data.test.ts` (6)
- `packages/safe-cex/src/__tests__/kucoinspot.exchange.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.exchange.startup.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.utils.test.ts` (10)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (4)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.fee-overlay.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.options.ts` (6)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.orders.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (20)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.ws-private.ts` (11)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.utils.ts` (16)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (13)
- `packages/safe-cex/src/__tests__/reference/okx/okx.order-history.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okxeu.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (9)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.order-state.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.public-parity.ts` (9)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.ticker.ts` (2)
- `packages/safe-cex/src/__tests__/rust.candle-routing.wasm.test.ts` (2)
- `packages/safe-cex/src/crossTrade/getCopierWeightRatio.test.ts` (7)
- `packages/safe-cex/src/crossTrade/getCopierWeightRatio.ts` (3)
- `packages/safe-cex/src/crossTrade/runtime/CrossTradeManager.test.ts` (3)
- `packages/safe-cex/src/crossTrade/runtime/CrossTradeManager.ts` (4)
- `packages/safe-cex/src/crossTrade/runtime/crossTradePlaceOrder.test.ts` (3)
- `packages/safe-cex/src/exchanges/base.ts` (11)
- `packages/safe-cex/src/exchanges/bybit/bybit.order-params.ts` (1)
- `packages/safe-cex/src/exchanges/external-public-data.ts` (4)
- `packages/safe-cex/src/exchanges/lighter/lighter.utils.ts` (5)
- `packages/safe-cex/src/exchanges/okx/okx.closed-pnl.ts` (3)
- `packages/safe-cex/src/exchanges/rust/publicFeed.ts` (6)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (8)
- `packages/safe-cex/src/instrument-class/bybit.instrumentClass.ts` (2)
- `packages/safe-cex/src/instrument-class/instrumentClass.test.ts` (2)
- `packages/safe-cex/src/instrument-class/resolve.ts` (3)
- `packages/safe-cex/src/instrument-class/types.ts` (2)
- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (6)
- `packages/safe-cex/src/utils/baseToNativeContracts.ts` (2)
- `packages/safe-cex/src/utils/getBalanceSlice.test.ts` (6)
- `packages/safe-cex/src/utils/getBalanceSlice.ts` (2)
- `packages/safe-cex/src/utils/getTickerPredictedBestAsk.test.ts` (4)
- `packages/safe-cex/src/utils/getTickerPredictedBestAsk.ts` (2)
- `packages/safe-cex/src/utils/getTickerPredictedBestBid.test.ts` (4)
- `packages/safe-cex/src/utils/getTickerPredictedBestBid.ts` (2)
- `packages/safe-cex/src/utils/getTotalBalanceFromBalance.test.ts` (5)
- `packages/safe-cex/src/utils/getTotalBalanceFromBalance.ts` (2)
- `packages/safe-cex/src/utils/risk.ts` (6)
- `packages/safe-cex/src/utils/sizing.test.ts` (7)
- `packages/safe-cex/src/utils/sizing.ts` (13)
- `packages/safe-cex/src/webworker/ExchangeWorker.forward-frames.test.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.rust-recovery.test.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.symbol-metadata.test.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.test.ts` (4)
- `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.test.ts` (3)
- `packages/safe-cex/src/webworker/handlers/BybitTickerSubscriptions.test.ts` (20)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.ts` (5)
- `packages/safe-cex/src/webworker/postProcessors/OrdersRiskProfilePostprocessor.ts` (2)
- `packages/safe-cex/src/webworker/twap/TwapInstance.test.ts` (3)
- `packages/safe-cex/src/webworker/twap/TwapInstance.timeout.test.ts` (3)
- `packages/safe-cex/src/webworker/twap/TwapManager.test.ts` (2)
- `packages/safe-cex/src/webworker/twap/TwapOptions.ts` (2)
- `packages/safe-cex/src/webworker/twap/TwapState.ts` (2)
- `packages/safe-cex/src/webworker/twap/TwapTypes.test.ts` (2)

**frame applier: 1 files / 2 references.**

- `packages/safe-cex/src/store/frameApplier.ts` (2)

**store: 4 files / 17 references.**

- `packages/safe-cex/src/store/store.base.test.ts` (4)
- `packages/safe-cex/src/store/store.base.ts` (6)
- `packages/safe-cex/src/store/store.interface.ts` (5)
- `packages/safe-cex/src/store/store.stub.ts` (2)

**packages/script-api-core: 21 files / 78 references.**

- `packages/script-api-core/src/__tests__/fixtures.ts` (3)
- `packages/script-api-core/src/__tests__/mockAdapter.ts` (2)
- `packages/script-api-core/src/adapter/AdapterContext.ts` (3)
- `packages/script-api-core/src/adapter/HostBindings.ts` (3)
- `packages/script-api-core/src/adapter/createCoreAdapter.test.ts` (4)
- `packages/script-api-core/src/adapter/createCoreAdapter.ts` (10)
- `packages/script-api-core/src/api/CoreApi.types.ts` (5)
- `packages/script-api-core/src/index.ts` (1)
- `packages/script-api-core/src/order/downSizeReduceOrder.test.ts` (5)
- `packages/script-api-core/src/order/downSizeReduceOrder.ts` (3)
- `packages/script-api-core/src/order/notionalSizing.test.ts` (4)
- `packages/script-api-core/src/order/notionalSizing.ts` (2)
- `packages/script-api-core/src/order/orderPricing.test.ts` (9)
- `packages/script-api-core/src/order/orderPricing.ts` (8)
- `packages/script-api-core/src/order/prepareOrder.options.test.ts` (3)
- `packages/script-api-core/src/order/prepareOrder.reduceLock.test.ts` (2)
- `packages/script-api-core/src/order/prepareOrder.ts` (2)
- `packages/script-api-core/src/order/shouldRoundSizeDown.ts` (2)
- `packages/script-api-core/src/script.ts` (1)
- `packages/script-api-core/src/utils/bbo.ts` (4)
- `packages/script-api-core/src/utils/getMarginCurrency.ts` (2)

### 5 Ticker

#### Ticker

**apps/cli: 1 files / 2 references.**

- `apps/cli/src/script-adapter.ts` (2)

**packages/cli-core: 5 files / 14 references.**

- `packages/cli-core/src/commands/trading/ChartCommand.test.ts` (2)
- `packages/cli-core/src/commands/trading/PriceCommand.ts` (5)
- `packages/cli-core/src/commands/trading/inverseOrderDisplay.test.ts` (2)
- `packages/cli-core/src/preprocess/conditions.ts` (3)
- `packages/cli-core/src/types.ts` (2)

**packages/safe-cex-server: 4 files / 9 references.**

- `packages/safe-cex-server/src/publicExchange/tickerCache.test.ts` (2)
- `packages/safe-cex-server/src/publicExchange/tickerCache.ts` (3)
- `packages/safe-cex-server/src/publicdata/tickerState.ts` (2)
- `packages/safe-cex-server/src/publicdata/types.ts` (2)

**packages/safe-cex: 86 files / 287 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/bybitdemo.order-risk.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/lighter.public-data.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okxeu.xperp-markets.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/polymarket.public-book-state.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust-frame.inverse-units.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.in-process-public-feed.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.polling-policy.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.ticker-observations.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/spot-products.public-parity.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/woo.funding-refresh.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/woo.v3-public-parity.test.ts` (3)
- `packages/safe-cex/src/__tests__/cutover/metadata.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/in-process-public-data.test.ts` (6)
- `packages/safe-cex/src/__tests__/kucoinspot.exchange.test.ts` (2)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.ws-secondary-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (12)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.utils.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts` (6)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.public-parity.ts` (17)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.ticker.ts` (2)
- `packages/safe-cex/src/crossTrade/getCopierWeightRatio.test.ts` (5)
- `packages/safe-cex/src/exchanges/base.ts` (3)
- `packages/safe-cex/src/exchanges/external-public-data.ts` (7)
- `packages/safe-cex/src/exchanges/rust/publicFeed.ts` (4)
- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (4)
- `packages/safe-cex/src/utils/getTickerPredictedBestAsk.test.ts` (4)
- `packages/safe-cex/src/utils/getTickerPredictedBestAsk.ts` (2)
- `packages/safe-cex/src/utils/getTickerPredictedBestBid.test.ts` (4)
- `packages/safe-cex/src/utils/getTickerPredictedBestBid.ts` (2)
- `packages/safe-cex/src/utils/risk.ts` (6)
- `packages/safe-cex/src/webworker/ExchangeWorker.test.ts` (4)
- `packages/safe-cex/src/webworker/ExchangeWorker.ts` (5)
- `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.test.ts` (3)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/OrdersRiskProfilePostprocessor.ts` (2)

**frame applier: 1 files / 3 references.**

- `packages/safe-cex/src/store/frameApplier.ts` (3)

**store: 4 files / 33 references.**

- `packages/safe-cex/src/store/store.base.test.ts` (5)
- `packages/safe-cex/src/store/store.base.ts` (20)
- `packages/safe-cex/src/store/store.interface.ts` (6)
- `packages/safe-cex/src/store/store.stub.ts` (2)

**packages/script-api-core: 10 files / 32 references.**

- `packages/script-api-core/src/__tests__/fixtures.ts` (3)
- `packages/script-api-core/src/__tests__/mockAdapter.ts` (2)
- `packages/script-api-core/src/adapter/AdapterContext.ts` (3)
- `packages/script-api-core/src/adapter/HostBindings.ts` (3)
- `packages/script-api-core/src/adapter/createCoreAdapter.ts` (4)
- `packages/script-api-core/src/api/CoreApi.types.ts` (9)
- `packages/script-api-core/src/index.ts` (1)
- `packages/script-api-core/src/order/prepareOrder.ts` (2)
- `packages/script-api-core/src/script.ts` (1)
- `packages/script-api-core/src/utils/bbo.ts` (4)

### 6 Fill and execution

#### OrderFillEvent

**apps/cli: 2 files / 5 references.**

- `apps/cli/src/exchange-listeners.test.ts` (2)
- `apps/cli/src/exchange-listeners.ts` (3)

**packages/safe-cex: 13 files / 54 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/bitrue.rust-boundary.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/exchange.listener-isolation.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/fill-event.incremental.test.ts` (11)
- `packages/safe-cex/src/__tests__/audit-regressions/kucoinspot.rust-boundary.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/lighter.rust-boundary.wasm.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/polymarket.private-executions.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.frame-recovery.test.ts` (11)
- `packages/safe-cex/src/__tests__/audit-regressions/spot-ack-fill.rust-boundary.wasm.test.ts` (5)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (2)
- `packages/safe-cex/src/types.ts` (1)
- `packages/safe-cex/src/webworker/ExchangeWorker.ts` (2)
- `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.ts` (3)

**frame applier: 1 files / 4 references.**

- `packages/safe-cex/src/store/frameApplier.ts` (4)

#### Execution

**packages/safe-cex: 37 files / 146 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/position-intervals.test.ts` (3)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (16)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.fills.ts` (9)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (8)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.utils.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.private.ts` (2)
- `packages/safe-cex/src/exchanges/base.ts` (3)
- `packages/safe-cex/src/exchanges/kraken/kraken.fills.ts` (4)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (3)
- `packages/safe-cex/src/journal/buildRecords.test.ts` (2)
- `packages/safe-cex/src/journal/buildRecords.ts` (3)
- `packages/safe-cex/src/journal/position-intervals.ts` (6)
- `packages/safe-cex/src/journal/woo.position-adapter.ts` (4)
- `packages/safe-cex/src/types.ts` (1)

#### PublicTrade

**packages/safe-cex: 10 files / 25 references.**

- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (2)
- `packages/safe-cex/src/exchanges/base.ts` (3)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (2)
- `packages/safe-cex/src/types.ts` (1)

### 7 Books and candles

#### Candle

**apps/cli: 1 files / 3 references.**

- `apps/cli/src/repl.ts` (3)

**packages/cli-core: 2 files / 5 references.**

- `packages/cli-core/src/commands/trading/ChartCommand.ts` (2)
- `packages/cli-core/src/types.ts` (3)

**packages/safe-cex-web: 1 files / 4 references.**

- `packages/safe-cex-web/src/candleCache/IndexedDBCandleCacheBackend.ts` (4)

**packages/safe-cex: 96 files / 316 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/bybit.monthly-candles.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.candle-fences.test.ts` (6)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.in-process-public-feed.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/woo.v3-public-parity.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/sharedPublicFeed.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/startupDemand.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.exchange.ohlcv-fallback.test.ts` (2)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binance.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.ws-futures.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.ws-public.ts` (6)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.starbase-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (6)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.utils.ts` (6)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.ws-business.ts` (2)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.public-parity.ts` (8)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.ws-public.ts` (3)
- `packages/safe-cex/src/candleCache/CandleCache.test.ts` (3)
- `packages/safe-cex/src/candleCache/CandleCache.ts` (4)
- `packages/safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend.test.ts` (2)
- `packages/safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend.ts` (3)
- `packages/safe-cex/src/candleCache/types.ts` (6)
- `packages/safe-cex/src/exchanges/base.ts` (9)
- `packages/safe-cex/src/exchanges/external-public-data.ts` (6)
- `packages/safe-cex/src/exchanges/in-process-public-data.ts` (2)
- `packages/safe-cex/src/exchanges/rust/publicFeed.ts` (2)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (7)
- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (2)
- `packages/safe-cex/src/webworker/handlers/OHLCVSubscriptionsHandler.ts` (2)

**frame applier: 1 files / 2 references.**

- `packages/safe-cex/src/store/frameApplier.ts` (2)

**packages/script-api-core: 8 files / 16 references.**

- `packages/script-api-core/src/__tests__/fixtures.ts` (2)
- `packages/script-api-core/src/__tests__/mockAdapter.ts` (2)
- `packages/script-api-core/src/adapter/AdapterContext.ts` (2)
- `packages/script-api-core/src/adapter/HostBindings.ts` (3)
- `packages/script-api-core/src/adapter/createCoreAdapter.ts` (2)
- `packages/script-api-core/src/api/CoreApi.types.ts` (3)
- `packages/script-api-core/src/index.ts` (1)
- `packages/script-api-core/src/script.ts` (1)

#### OrderBook

**apps/cli: 4 files / 18 references.**

- `apps/cli/src/record/recorders/orderbook.ts` (5)
- `apps/cli/src/script-adapter.test.ts` (4)
- `apps/cli/src/script-adapter.ts` (2)
- `apps/cli/src/script-orderbook-cache.ts` (7)

**packages/cli-core: 1 files / 3 references.**

- `packages/cli-core/src/types.ts` (3)

**packages/safe-cex: 105 files / 332 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/apexomni.rust-public.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/asterdex.orderbook-snapshot.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/lighter.public-data.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/mexc.depth-sequence.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/okx-full-depth.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.sequenced-orderbook-adoption-gate.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/paradex.full-depth.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/phemex.orderbook.audit-regression.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/polymarket.public-book-state.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/polymarket.rust-boundary.wasm.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.in-process-public-feed.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/sequenced-delta-orderbook.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/woo.orderbook-update.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/woo.v3-public-parity.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/sharedPublicFeed.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/startupDemand.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binance.orderbook.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binance.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.ws-futures.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.starbase-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.ws-public.ts` (8)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.ws-public.ts` (5)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.ws-public.ts` (10)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.utils.ts` (4)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okx.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.orderbook-recovery.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.public-parity.ts` (6)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.ws-public.ts` (4)
- `packages/safe-cex/src/__tests__/rust.exchange.wasm.test.ts` (4)
- `packages/safe-cex/src/exchanges/base.ts` (5)
- `packages/safe-cex/src/exchanges/external-public-data.ts` (6)
- `packages/safe-cex/src/exchanges/in-process-public-data.ts` (2)
- `packages/safe-cex/src/exchanges/rust/publicFeed.ts` (2)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (9)
- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (2)
- `packages/safe-cex/src/utils/orderbook.test.ts` (12)
- `packages/safe-cex/src/utils/orderbook.ts` (3)
- `packages/safe-cex/src/utils/sequencedDeltaOrderBook.ts` (4)
- `packages/safe-cex/src/utils/updateOrderBook.test.ts` (3)
- `packages/safe-cex/src/utils/updateOrderBook.ts` (5)
- `packages/safe-cex/src/webworker/handlers/OrderBookSubscriptionsHandler.ts` (2)

**frame applier: 1 files / 4 references.**

- `packages/safe-cex/src/store/frameApplier.ts` (4)

**packages/script-api-core: 7 files / 13 references.**

- `packages/script-api-core/src/__tests__/fixtures.ts` (2)
- `packages/script-api-core/src/__tests__/mockAdapter.ts` (2)
- `packages/script-api-core/src/adapter/AdapterContext.ts` (2)
- `packages/script-api-core/src/adapter/HostBindings.ts` (2)
- `packages/script-api-core/src/api/CoreApi.types.ts` (3)
- `packages/script-api-core/src/index.ts` (1)
- `packages/script-api-core/src/script.ts` (1)

#### OrderBookOrders

**packages/safe-cex: 6 files / 15 references.**

- `packages/safe-cex/src/__tests__/reference/blofin/blofin.ws-public.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.ws-public.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.public-parity.ts` (3)
- `packages/safe-cex/src/types.ts` (3)
- `packages/safe-cex/src/utils/orderbook.test.ts` (2)
- `packages/safe-cex/src/utils/updateOrderBook.test.ts` (2)

### 8 History and income

#### HistoricalOrder

**packages/safe-cex: 23 files / 59 references.**

- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binance.order-history.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.mappers.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.order-history.ts` (2)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.order-history.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.order-history.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.order-history.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.order-history.ts` (2)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.order-history.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts` (4)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.order-history.ts` (2)
- `packages/safe-cex/src/exchanges/base.ts` (4)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (3)
- `packages/safe-cex/src/journal/woo.position-adapter.ts` (3)
- `packages/safe-cex/src/types.ts` (1)

#### HistoricalTrade

**packages/safe-cex: 7 files / 20 references.**

- `packages/safe-cex/src/__tests__/reference/binance/binance.utils.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.starbase-private.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.v3.private.ts` (3)
- `packages/safe-cex/src/types.ts` (1)

#### ClosedPnlEntry

**packages/safe-cex: 18 files / 50 references.**

- `packages/safe-cex/src/__tests__/reference/binance/binance.closed-pnl.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.income.ts` (3)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (3)
- `packages/safe-cex/src/exchanges/base.ts` (4)
- `packages/safe-cex/src/exchanges/bitunix/bitunix.income.ts` (2)
- `packages/safe-cex/src/exchanges/blofin/blofin.income.ts` (2)
- `packages/safe-cex/src/exchanges/bybit/bybit.closed-pnl.ts` (2)
- `packages/safe-cex/src/exchanges/okx/okx.closed-pnl.ts` (2)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (4)
- `packages/safe-cex/src/types.ts` (1)
- `packages/safe-cex/src/utils/resolveClosedPositions.test.ts` (4)
- `packages/safe-cex/src/utils/resolveClosedPositions.ts` (4)

#### IncomeRecord

**packages/safe-cex: 29 files / 110 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/income-paging.test.ts` (5)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.income.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.income.ts` (7)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/bybit/hyrotrader.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.income.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (5)
- `packages/safe-cex/src/__tests__/reference/okx/okx.income.ts` (3)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (4)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.income.ts` (4)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.history.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/weex/weex.utils.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/woo/woo.income.ts` (3)
- `packages/safe-cex/src/exchanges/base.ts` (7)
- `packages/safe-cex/src/exchanges/bitunix/bitunix.income.ts` (3)
- `packages/safe-cex/src/exchanges/blofin/blofin.income.ts` (3)
- `packages/safe-cex/src/exchanges/income-paging.ts` (7)
- `packages/safe-cex/src/exchanges/kraken/kraken.income.ts` (4)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (3)
- `packages/safe-cex/src/types.ts` (1)

### 9 Accounts and options

#### ExchangeOptions

**packages/exchange-mcp: 1 files / 2 references.**

- `packages/exchange-mcp/src/config.ts` (2)

**packages/safe-cex: 263 files / 840 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/backpack.execution-history.regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/backpack.native-batch.regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/backpack.order-mapping.regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/backpack.state-reconciliation.regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/backpack.trigger-reference.regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/base-exchange.watchdog.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/binance-variants.rust-boundary.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.native-modify.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.parity.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.remaining-audit.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bitunix.validate-account.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/bybit.recently-placed-order-preservation.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/capability-branding.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.cfm-private-ws-contract-size.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.native-edit.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.open-orders-polling.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.private-ws-auth.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/coinbase.secondary-read-key.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/exchange-metadata-capabilities.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.fee-subject.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.schedule-cancel.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/hyperliquid.spot-markets-merge.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/kraken.signed-query-order.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.client-order-id.audit-regression.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/krakenspot.native-amend.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/kucoin.normal-order-rejection.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/lighter.public-data.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/lighter.rust-boundary.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/lighter.signing-modes.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.monthly-candles.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.orderbook-unsubscribe.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.remaining-audit.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.rest-signing.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.sequenced-orderbook-adoption-gate.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.split-remainder.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okx.websocket-routing.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okxdemo.transport.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/okxeu.xperp-markets.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/order-payload-normalization.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/orderly.native-amend.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/orderly.rust-transport.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/polymarket.rust-boundary.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.account-identity.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.apexomni-wrapper.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.asterdex-signer-setup.test.ts` (12)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.asterdex-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.backpack-hosted-provider.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.backpack-hosted-signing.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.bingx-hosted-provider.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.bingx-hosted-signing.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.bitget-hosted-provider.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.bitget-hosted-signing.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.bitunix-hosted-signing.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.blofin-hosted-provider.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.blofin-hosted-signing.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.coinbase-read-options.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.command-errors.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.cosigned-page-engine.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.cryptocom-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.delegated-provider.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.delegated-signer-setup.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.grvt-hosted-provider.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hosted-presentation-provider.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hosted-provider.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hosted-signing-facade.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hosted-signing-venues.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hyperliquid-remote-signing.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hyperliquid-signer-setup.test.ts` (5)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hyperliquid-wrapper.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hyro-construction.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.hyrotrader-unconfigured.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.keyless-remote-signing.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.kucoin-hosted-provider.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.kucoin-hosted-signing.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.kucoinspot-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.lighter-remote-signing.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.lighter-signer-setup.test.ts` (6)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.lighter-wrapper.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.local-signer-setup.test.ts` (19)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.mexc-signer-setup.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.mirror-hosted-init-fields.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.okx-hosted-provider.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.okx-hosted-signing.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.page-engine-signing.wasm.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.paradex-remote-signing.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.paradex-signer-setup.test.ts` (8)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.paradex-signing-transport.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.paradex-wrapper.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.parity-sweep.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.payload-signer-refusal.test.ts` (11)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.phemex-hosted-signing.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.polling-native-auth.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.polling-policy.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.product-options.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.read-key-policy.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.remote-signer-runtime.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.remote-signing-policy.test.ts` (8)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.secondary-read-keys-bypass.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.selected-signer-mode.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.startup-demand.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.startup-readiness.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.woo-hosted-provider.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.woo-hosted-signing.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.worker-command-contract.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/signing.request-shapes.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/transport-remote-signing.audit-regression.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/txflow.rust-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/woo.legacy-v1-signing.test.ts` (2)
- `packages/safe-cex/src/__tests__/base.remote-signer.test.ts` (2)
- `packages/safe-cex/src/__tests__/base.ws-auth-signing.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/realVenue.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/sharedPublicFeed.wasm.test.ts` (4)
- `packages/safe-cex/src/__tests__/hyperliquid.exchange.startup.test.ts` (2)
- `packages/safe-cex/src/__tests__/hyperliquidspot.balance.test.ts` (2)
- `packages/safe-cex/src/__tests__/kucoin.exchange-docs.test.ts` (8)
- `packages/safe-cex/src/__tests__/kucoinspot.exchange.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.builder-fee.test.ts` (4)
- `packages/safe-cex/src/__tests__/lighter.exchange.account-lookup.test.ts` (3)
- `packages/safe-cex/src/__tests__/lighter.exchange.builder-fee.test.ts` (3)
- `packages/safe-cex/src/__tests__/lighter.exchange.ohlcv-fallback.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.exchange.place-orders.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.exchange.signer-gate.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.exchange.startup.test.ts` (2)
- `packages/safe-cex/src/__tests__/lighter.signer.test.ts` (6)
- `packages/safe-cex/src/__tests__/okxeu.validate-account.test.ts` (3)
- `packages/safe-cex/src/__tests__/paradex.payload-signer.test.ts` (4)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/apexomni/apexomni.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binance.api.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.api.ts` (3)
- `packages/safe-cex/src/__tests__/reference/binance/binanceinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binanceoptions.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.api.ts` (5)
- `packages/safe-cex/src/__tests__/reference/binance/binancepm.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/binance/binancespot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitget.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitget/bitgetusdc.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybit.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitinverse.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/bybit/bybitspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.api.ts` (4)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.api.ts` (4)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/kraken.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kraken/krakenspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.broker.ts` (4)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/lighter/lighter.signer.ts` (4)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okx.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okxdemo.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/okx/okxeudemo.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.api.ts` (3)
- `packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/paradex/paradex.sdk.ts` (9)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/phemex/phemex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.api.ts` (3)
- `packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.api.ts` (3)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.api.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/woo/woo.api.ts` (4)
- `packages/safe-cex/src/__tests__/reference/woo/woo.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/rust.capability-manifest.wasm.test.ts` (4)
- `packages/safe-cex/src/__tests__/rust.construction-rules.wasm.test.ts` (6)
- `packages/safe-cex/src/__tests__/rust.exchange.wasm.test.ts` (3)
- `packages/safe-cex/src/__tests__/rust.public-mode-rules.wasm.test.ts` (6)
- `packages/safe-cex/src/__tests__/transport-adapter-selection.characterization.test.ts` (3)
- `packages/safe-cex/src/createApiWithSecondaryKey.test.ts` (12)
- `packages/safe-cex/src/createApiWithSecondaryKey.ts` (6)
- `packages/safe-cex/src/createExchangeFromAccount.test.ts` (4)
- `packages/safe-cex/src/createExchangeFromAccount.ts` (6)
- `packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts` (3)
- `packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts` (3)
- `packages/safe-cex/src/exchanges/asterdex/asterdex.signing.ts` (2)
- `packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts` (4)
- `packages/safe-cex/src/exchanges/base.ts` (6)
- `packages/safe-cex/src/exchanges/binance/binance.signing.ts` (3)
- `packages/safe-cex/src/exchanges/bybit/bybit.signing.ts` (2)
- `packages/safe-cex/src/exchanges/bybit/bybit.site.ts` (3)
- `packages/safe-cex/src/exchanges/hostedSigningVenues.ts` (2)
- `packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.constants.ts` (2)
- `packages/safe-cex/src/exchanges/lighter/lighter.builder-fee.ts` (3)
- `packages/safe-cex/src/exchanges/rust/asterdexSignerSetup.ts` (4)
- `packages/safe-cex/src/exchanges/rust/betaVenues.ts` (2)
- `packages/safe-cex/src/exchanges/rust/construction.ts` (3)
- `packages/safe-cex/src/exchanges/rust/delegatedSignerSetup.ts` (8)
- `packages/safe-cex/src/exchanges/rust/hyperliquidSignerSetup.ts` (4)
- `packages/safe-cex/src/exchanges/rust/legacySigning.ts` (3)
- `packages/safe-cex/src/exchanges/rust/localSignerSetup.ts` (7)
- `packages/safe-cex/src/exchanges/rust/mexcSignerSetup.ts` (3)
- `packages/safe-cex/src/exchanges/rust/paradex.signing.ts` (2)
- `packages/safe-cex/src/exchanges/rust/paradexSignerSetup.ts` (4)
- `packages/safe-cex/src/exchanges/rust/publicFeed.ts` (2)
- `packages/safe-cex/src/exchanges/rust/remoteSignerProvider.ts` (3)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (6)
- `packages/safe-cex/src/index.ts` (5)
- `packages/safe-cex/src/safeCexAxiosCreate.test.ts` (26)
- `packages/safe-cex/src/safeCexAxiosCreate.ts` (5)
- `packages/safe-cex/src/transport-resolution.characterization.test.ts` (3)
- `packages/safe-cex/src/transport/engine.bybit-public-reads.test.ts` (4)
- `packages/safe-cex/src/transport/engine.lighter-rest-proxy.test.ts` (4)
- `packages/safe-cex/src/transport/engine.native-matrix.test.ts` (3)
- `packages/safe-cex/src/transport/engine.native-routing-port.test.ts` (5)
- `packages/safe-cex/src/transport/engine.proxy-token-context.test.ts` (2)
- `packages/safe-cex/src/transport/engine.public-data-routing.test.ts` (4)
- `packages/safe-cex/src/transport/engine.public-proxy-guard.test.ts` (3)
- `packages/safe-cex/src/transport/engine.publicDataOnlyRoute.test.ts` (2)
- `packages/safe-cex/src/transport/engine.secondary-read-keys-bypass.test.ts` (5)
- `packages/safe-cex/src/transport/engine.test.ts` (6)
- `packages/safe-cex/src/transport/engine.ts` (19)
- `packages/safe-cex/src/transport/registry.ts` (4)
- `packages/safe-cex/src/transport/tealstreet/tealstreetTransport.ts` (2)
- `packages/safe-cex/src/transport/types.ts` (6)
- `packages/safe-cex/src/types.ts` (2)
- `packages/safe-cex/src/utils/payloadSigner.ts` (3)
- `packages/safe-cex/src/webworker/ExchangeWorker.rust-recovery.test.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.test.ts` (3)
- `packages/safe-cex/src/webworker/ExchangeWorker.ts` (3)
- `packages/safe-cex/src/webworker/postProcessors/BasePostprocessor.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/BasePostprocessor.ts` (3)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedMarginPostprocessor.test.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.test.ts` (8)
- `packages/safe-cex/src/webworker/postProcessors/ExchangeDependentPostprocessor.ts` (3)
- `packages/safe-cex/src/webworker/postProcessors/MessageHandlerStatsPostprocessor.test.ts` (16)
- `packages/safe-cex/src/webworker/postProcessors/TwapOrderEnrichmentPostprocessor.test.ts` (2)

#### ExchangeAccount

**packages/cli-core: 2 files / 2 references.**

- `packages/cli-core/src/index.ts` (1)
- `packages/cli-core/src/types.ts` (1)

**packages/safe-cex: 8 files / 21 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/rust.account-identity.test.ts` (5)
- `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/kucoin/kucoinspot.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (2)
- `packages/safe-cex/src/exchanges/base.ts` (3)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (4)
- `packages/safe-cex/src/types.ts` (1)

#### ExchangeAccountConfig

**apps/cli: 8 files / 54 references.**

- `apps/cli/src/account-log.ts` (2)
- `apps/cli/src/config.test.ts` (15)
- `apps/cli/src/config.ts` (16)
- `apps/cli/src/exchange-manager.ts` (4)
- `apps/cli/src/listener/handlers/account.ts` (6)
- `apps/cli/src/platform-adapter.ts` (7)
- `apps/cli/src/removed-venue.test.ts` (3)
- `apps/cli/src/repl.ts` (1)

**packages/cli-core: 4 files / 21 references.**

- `packages/cli-core/src/index.ts` (1)
- `packages/cli-core/src/types.ts` (10)
- `packages/cli-core/src/worker-host/WorkerPlatformAdapter.test.ts` (3)
- `packages/cli-core/src/worker-host/WorkerPlatformAdapter.ts` (7)

**packages/safe-cex: 13 files / 72 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/removed-venue-refusal.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust-only-beta-registry.test.ts` (3)
- `packages/safe-cex/src/__tests__/rust.bybit-direct-private-start.wasm.test.ts` (2)
- `packages/safe-cex/src/accounts/buildExchangeAccountConfig.ts` (5)
- `packages/safe-cex/src/accounts/normalizeExchangeAccountConfig.ts` (3)
- `packages/safe-cex/src/accounts/parseImportAccountData.ts` (6)
- `packages/safe-cex/src/createExchangeFromAccount.test.ts` (36)
- `packages/safe-cex/src/createExchangeFromAccount.ts` (5)
- `packages/safe-cex/src/index.ts` (1)
- `packages/safe-cex/src/transport-resolution.characterization.test.ts` (2)
- `packages/safe-cex/src/transport/engine.native-routing-port.test.ts` (3)
- `packages/safe-cex/src/transport/tealstreet/transportResolution.ts` (2)
- `packages/safe-cex/src/types.ts` (1)

### 10 Store and remaining public surface

#### StoreData

**packages/safe-cex: 35 files / 124 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/rust.account-state.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.apexomni-wrapper.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.engine-capabilities.test.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.in-process-public-feed.test.ts` (3)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.kucoinspot-wrapper.test.ts` (2)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.native-startup.fixtures.ts` (4)
- `packages/safe-cex/src/__tests__/audit-regressions/rust.startup-demand.test.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/consumerFrame.ts` (2)
- `packages/safe-cex/src/__tests__/cutover/consumers.wasm.test.ts` (4)
- `packages/safe-cex/src/__tests__/cutover/sharedPublicFeed.wasm.test.ts` (2)
- `packages/safe-cex/src/__tests__/in-process-public-data.health.test.ts` (16)
- `packages/safe-cex/src/__tests__/in-process-public-data.test.ts` (7)
- `packages/safe-cex/src/exchanges/base.ts` (2)
- `packages/safe-cex/src/exchanges/in-process-public-data.ts` (2)
- `packages/safe-cex/src/types.ts` (5)
- `packages/safe-cex/src/webworker/ExchangeWorker.forward-frames.test.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.rust-recovery.test.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.symbol-metadata.test.ts` (2)
- `packages/safe-cex/src/webworker/ExchangeWorker.ts` (11)
- `packages/safe-cex/src/webworker/StoreWorkerPayload.ts` (2)
- `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.test.ts` (2)
- `packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.ts` (3)
- `packages/safe-cex/src/webworker/postProcessors/BasePostprocessor.test.ts` (6)
- `packages/safe-cex/src/webworker/postProcessors/BasePostprocessor.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/ChaserOrderEnrichmentPostprocessor.ts` (3)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedMarginPostprocessor.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedMarginPostprocessor.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.ts` (3)
- `packages/safe-cex/src/webworker/postProcessors/ExchangeDependentPostprocessor.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/MessageHandlerStatsPostprocessor.test.ts` (4)
- `packages/safe-cex/src/webworker/postProcessors/MessageHandlerStatsPostprocessor.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/OrdersRiskProfilePostprocessor.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/TwapOrderEnrichmentPostprocessor.test.ts` (2)
- `packages/safe-cex/src/webworker/postProcessors/TwapOrderEnrichmentPostprocessor.ts` (2)

**store: 3 files / 41 references.**

- `packages/safe-cex/src/store/store.base.ts` (11)
- `packages/safe-cex/src/store/store.interface.ts` (28)
- `packages/safe-cex/src/store/store.stub.ts` (2)

#### StoreOptions

**packages/safe-cex: 1 files / 3 references.**

- `packages/safe-cex/src/types.ts` (3)

#### StoreDataLoaded

**packages/cli-core: 1 files / 2 references.**

- `packages/cli-core/src/types.ts` (2)

**packages/safe-cex: 2 files / 5 references.**

- `packages/safe-cex/src/types.test.ts` (2)
- `packages/safe-cex/src/types.ts` (3)

#### StoreUpdateIds

**packages/safe-cex: 1 files / 2 references.**

- `packages/safe-cex/src/types.ts` (2)

#### FetchedLeverage

**packages/safe-cex: 11 files / 47 references.**

- `packages/safe-cex/src/__tests__/audit-regressions/uistate.null-leverage-race.test.ts` (5)
- `packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/blofin/blofin.exchange.ts` (3)
- `packages/safe-cex/src/__tests__/reference/grvt/grvt.exchange.ts` (7)
- `packages/safe-cex/src/__tests__/reference/mexc/mexc.exchange.ts` (2)
- `packages/safe-cex/src/__tests__/reference/txflow/txflow.account.ts` (6)
- `packages/safe-cex/src/__tests__/reference/weex/weex.exchange.ts` (3)
- `packages/safe-cex/src/exchanges/base.ts` (5)
- `packages/safe-cex/src/exchanges/rust/rust.exchange.ts` (7)
- `packages/safe-cex/src/types.ts` (3)
- `packages/safe-cex/src/webworker/handlers/UiStateHandler.ts` (3)

**frame applier: 1 files / 5 references.**

- `packages/safe-cex/src/store/frameApplier.ts` (5)

#### DisplayedMargin

**packages/safe-cex: 1 files / 2 references.**

- `packages/safe-cex/src/types.ts` (2)

## Integration and completion gates

1. Plan is published before code edits; copy this exact plan into docs/plans/TYPE_OWNERSHIP_P1.md for a durable PR link.
2. Implement only Position in this worktree, red tests first. Directly replace its declaration and remove its TS output shaping/defaults in one slice. Do not use as Position to conceal producer drift. Do not drop native quantity extras or null handling just to delete lines.
3. Record baseline failures separately. Run required package and root gates after dependencies finish installing; native -j2 tests must cover actual producer semantics, not just a copied TS fixture.
4. PR title: feat(types): generate Position from the Rust model (P1 slice 1). Normal PR per latest instruction. First lines: Public type changes / NX impact, NEEDS-NX. List shape changes separately from Store semantic changes and packed-protocol compatibility. NX must land first; consumer code outside this mirror is not claimed verified.
5. CLEAR hostile review remains required. Do not mark merged/deployed or self-merge. Record PR URL, HEAD SHA, checks and any unresolved needs in /Users/sam/agent-reports/type-ownership-p1.txt.

## Position integration contract (implementation audit update)

The Position cutover uses POSITIONS revision 2, eleven f64 columns, and required
native ID/symbol references. Enum-word bits 24/25 mark presence of native
contracts/native PnL; a present non-finite native size rejects the section rather
than looking absent and selecting a base-unit fallback. The private engine
Position remains its internal arithmetic representation; Rust output helpers
own its public JSON/frame normalization, tested against the generated contract.
Older Position sections are refused. NX must rebuild native/WASM producers and
adopt this decoder in the same coordinated cutover, before the SDK PR merges.

PLAN-READY
