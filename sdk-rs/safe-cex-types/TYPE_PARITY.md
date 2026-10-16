# P0d type ownership parity

## Decision

**The three object shapes and all nine dependent enum declarations now match the TS contract; whole-consumer cutover is not yet proven.**
The Rust models generate the same 72 field declarations (including nested fields),
requiredness, enum members and readonly modifiers with the generation passes in
`examples/generate.rs`. The scoped checker proves bidirectional model and enum
assignability, all runtime members, and rejection of every bare enum literal.
Plain ts-rs 12.0.1 alone does not reproduce today's contract:
it drops readonly and emits type-only string unions for runtime TypeScript enums.
The raw enum difference was a real cutover blocker, not cosmetic; the final
output now renders real string-valued enums to resolve it. This phase does not
replace `packages/safe-cex/src/types.ts` or change its consumers.

Baseline: `packages/safe-cex/src/types.ts` at commit `d30921c422cd5f7612f75fedd2a394ead8b6da89`
(blob `d4b1082b8302aefe7de285d4881a8bc449cc2162`). The scoped checker reads the current file,
not a copied snapshot. Architecture: [port plan section 6](../../docs/plans/SAFE_CEX_RUST_PORT_v1.md#6-type-ownership-invert-it-in-p1).
Behavioral context stays in [safe-cex guidance](../../packages/safe-cex/CLAUDE.md)
and [exchange guidance](../../packages/safe-cex/src/exchanges/CLAUDE.md).

## Generation and classification

Run `yarn types:generate` from the repository root. The host-only Cargo example
exports exactly three roots and their dependencies into a freshly recreated bindings
directory. `bindings/raw/` commits the actual ts-rs output and
`bindings/enum-members.json` commits Rust-owned metadata for exported TS enums.
`generate.cjs` parses every raw file with the repo's TypeScript compiler, renders
string unions as real enums, and recursively adds readonly to property signature
nodes. The printer retains doc comments and works with multiline/nested declarations;
no line-shaped rewriting remains. Rust nested structs use `#[ts(inline)]` to retain
the anonymous TypeScript object shapes.

The enum metadata pass parses Rust `src/lib.rs` with the already-selected `syn`
dev dependency. It follows inline and file-backed nested modules (`name.rs` and
`name/mod.rs`), including declarations re-exported with `pub use`. It collects
only enums deriving `TS` and validates members only when raw bindings export that
type, so ordinary unexported Rust error enums are allowed. Rust variant identifiers
supply member names; explicit `serde(rename)` strings supply values. Duplicate TS
names, duplicate values, unsupported exported variants and unsupported module source
layouts (including explicit `#[path]`) fail instead of silently skipping validation.
Source scanning does not evaluate cfg; unsupported inactive modules also fail closed.

The AST pass requires metadata for every raw string-union enum and compares its
literal values before rendering; new unexplained enum exports fail. The checker
independently derives the oracle's dependency closure from the three roots and
compares that closure to every raw export, including enum names and metadata coverage.
Neither coverage pass has a hardcoded nine-enum allowlist. A tenth dependency is
checked automatically; an unexplained tenth export fails.

The readonly policy remains scoped to these all-readonly P0d models; the checker
now compares readonly flags on **both** oracle and generated nodes. A future mutable
oracle field fails parity until generation policy is changed deliberately. The
current generator does not read the oracle to produce output. All existing raw and
final outputs are removed before export, so stale bindings cannot be promoted.

Every table shows the **current baseline**, **raw ts-rs output**, and **final output**.
Commas instead of semicolons, double quotes instead of single quotes, compact
layout, generated header, JSON numeric spelling (`0.0` vs `0`) and type-only imports
for object declarations are **(a) cosmetic**; field order
is retained. Lost comments are not a type mismatch, but unit/semantic documentation
must remain available before cutover. No generated declaration substitutes for
that behavioral guidance.

Codes used in every field row:

- **B1 — (b), fixed in generator:** raw ts-rs omits readonly. This changes consumers'
  ability to mutate fields; it is not cosmetic. The final generator restores it,
  including multiline, documented nested fields. Rust ownership/mutability is not TS readonly.
- **B2 — (b), fixed in Rust metadata:** `Option<T>` would normally emit a required
  `T | null`. `#[ts(optional_fields)]` emits `?: T`; every optional field also has
  serde omission/default/present-value deserialization. No null union is added.
- **B3 — (b), fixed in Rust metadata:** `#[ts(type = "any")]` alone bypasses inferred
  optionality. All three `info` fields explicitly add `#[ts(optional)]`.
- **B4 — (b), fixed in generator:** raw ts-rs emits a union instead of an enum.
  The final pass renders `export enum` with exact Rust member names and serde
  literal values. Runtime member access and enum assignability now match the
  baseline, and bare strings remain rejected. See the enum table.
- **C1 — (c), intentional numeric representation:** `f64` preserves TypeScript's
  double precision `number`, including fractional quantities/timestamps and zero.
  No integer/count/newtype validation or unit conversion is introduced. JSON cannot
  carry NaN/infinities; serde_json does not preserve those TS values (serialization
  produces null for nonfinite f64). This spike proves finite JSON data only.
- **C2 — (c), intentional JSON boundary with an unresolved broader domain:**
  `info?: any` is reproduced in TS, but Rust stores `Option<serde_json::Value>`.
  It preserves absent vs present JSON null, primitives, arrays and objects.
  Functions, undefined-valued object members, cyclic objects, class identity and
  other non-JSON JS data cannot be represented faithfully by this storage type.
  Do not infer full JS `any` parity from the TS annotation. The worker strips truthy Order/Position
  info in `ExchangeWorker._sanitizeForPostMessage` (not Market info); broader FFI policy
  still needs a decision before cutover.
- **C3 — (c), intentional nested representation:** named Rust helpers inline into
  the original anonymous TS objects or unions, preserving their mutability. They do not add exported TS names.

Present non-info nulls are rejected by `deserialize_present`, matching strict TS
optionals instead of silently turning invalid null into absence. For info, the
same helper deserializes `Value::Null` as `Some(Value::Null)`; missing fields
use `None`. Rust does not retain an own-property `undefined` separately from
absence, since both are omitted at the JSON boundary. TS optional property
semantics depend on consumers' `exactOptionalPropertyTypes` configuration;
this spike neither changes that configuration nor claims JSON can encode undefined.
The `no-serde-warnings` ts-rs feature suppresses its unsupported `deserialize_with`
attribute warning; the generated AST and wire gates explicitly validate its effect.

## Field-by-field triage

Each snippet is the property's declaration at the path in the first column.
`{ … }` abbreviates an object whose children are individually listed below.
No fields from Order, Position or Market are omitted.

### Order (30 rows)

| Field | Current types.ts | Raw ts-rs | Final generated | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `Order.id` | `readonly id: string` | `id: string` | `readonly id: string` | `String` | B1 |
| `Order.parentId` | `readonly parentId?: string` | `parentId?: string` | `readonly parentId?: string` | `Option<String>` | B1, B2 |
| `Order.status` | `readonly status: OrderStatus` | `status: OrderStatus` | `readonly status: OrderStatus` | `OrderStatus` | B1, B4 |
| `Order.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `Order.type` | `readonly type: OrderType` | `type: OrderType` | `readonly type: OrderType` | `OrderType` | B1, B4 |
| `Order.side` | `readonly side: OrderSide` | `side: OrderSide` | `readonly side: OrderSide` | `OrderSide` | B1, B4 |
| `Order.price` | `readonly price: number` | `price: number` | `readonly price: number` | `f64` | B1, C1 |
| `Order.amount` | `readonly amount: number` | `amount: number` | `readonly amount: number` | `f64` | B1, C1 |
| `Order.filled` | `readonly filled: number` | `filled: number` | `readonly filled: number` | `f64` | B1, C1 |
| `Order.remaining` | `readonly remaining: number` | `remaining: number` | `readonly remaining: number` | `f64` | B1, C1 |
| `Order.reduceOnly` | `readonly reduceOnly: boolean` | `reduceOnly: boolean` | `readonly reduceOnly: boolean` | `bool` | B1 |
| `Order.close` | `readonly close?: boolean` | `close?: boolean` | `readonly close?: boolean` | `Option<bool>` | B1, B2 |
| `Order.info` | `readonly info?: any` | `info?: any` | `readonly info?: any` | `Option<serde_json::Value>` | B1, B3, C2 |
| `Order.timeInForce` | `readonly timeInForce?: OrderTimeInForce` | `timeInForce?: OrderTimeInForce` | `readonly timeInForce?: OrderTimeInForce` | `Option<OrderTimeInForce>` | B1, B2, B4 |
| `Order.lastUpdate` | `readonly lastUpdate: number` | `lastUpdate: number` | `readonly lastUpdate: number` | `f64` | B1, C1 |
| `Order.triggerType` | `readonly triggerType?: OrderStopTriggerType` | `triggerType?: OrderStopTriggerType` | `readonly triggerType?: OrderStopTriggerType` | `Option<OrderStopTriggerType>` | B1, B2, B4 |
| `Order.tpTriggerType` | `readonly tpTriggerType?: OrderStopTriggerType` | `tpTriggerType?: OrderStopTriggerType` | `readonly tpTriggerType?: OrderStopTriggerType` | `Option<OrderStopTriggerType>` | B1, B2, B4 |
| `Order.slTriggerType` | `readonly slTriggerType?: OrderStopTriggerType` | `slTriggerType?: OrderStopTriggerType` | `readonly slTriggerType?: OrderStopTriggerType` | `Option<OrderStopTriggerType>` | B1, B2, B4 |
| `Order.takeProfit` | `readonly takeProfit?: number` | `takeProfit?: number` | `readonly takeProfit?: number` | `Option<f64>` | B1, B2, C1 |
| `Order.stopLoss` | `readonly stopLoss?: number` | `stopLoss?: number` | `readonly stopLoss?: number` | `Option<f64>` | B1, B2, C1 |
| `Order.extra` | `readonly extra?: { … }` | `extra?: { … }` | `readonly extra?: { … }` | `Option<OrderExtra>` | B1, B2, C3 |
| `Order.extra.remainingNative` | `readonly remainingNative?: number` | `remainingNative?: number` | `readonly remainingNative?: number` | `Option<f64>` | B1, B2, C1 |
| `Order.extra.filledInPlacementUnits` | `readonly filledInPlacementUnits?: number` | `filledInPlacementUnits?: number` | `readonly filledInPlacementUnits?: number` | `Option<f64>` | B1, B2, C1 |
| `Order.extra.remainingNotional` | `readonly remainingNotional?: number` | `remainingNotional?: number` | `readonly remainingNotional?: number` | `Option<f64>` | B1, B2, C1 |
| `Order.extra.conditionalOrderPrice` | `readonly conditionalOrderPrice?: number` | `conditionalOrderPrice?: number` | `readonly conditionalOrderPrice?: number` | `Option<f64>` | B1, B2, C1 |
| `Order.extra.rejectReason` | `readonly rejectReason?: OrderRejectReason` | `rejectReason?: OrderRejectReason` | `readonly rejectReason?: OrderRejectReason` | `Option<OrderRejectReason>` | B1, B2, B4 |
| `Order.extra.isFromChaser` | `readonly isFromChaser?: boolean` | `isFromChaser?: boolean` | `readonly isFromChaser?: boolean` | `Option<bool>` | B1, B2 |
| `Order.extra.chaserId` | `readonly chaserId?: string` | `chaserId?: string` | `readonly chaserId?: string` | `Option<String>` | B1, B2 |
| `Order.extra.chaserAttempts` | `readonly chaserAttempts?: number` | `chaserAttempts?: number` | `readonly chaserAttempts?: number` | `Option<f64>` | B1, B2, C1 |
| `Order.extra.chaserMaxAttempts` | `readonly chaserMaxAttempts?: number` | `chaserMaxAttempts?: number` | `readonly chaserMaxAttempts?: number` | `Option<f64>` | B1, B2, C1 |

### Position (18 rows)

| Field | Current types.ts | Raw ts-rs | Final generated | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `Position.id` | `readonly id: string` | `id: string` | `readonly id: string` | `String` | B1 |
| `Position.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `Position.side` | `readonly side: PositionSide` | `side: PositionSide` | `readonly side: PositionSide` | `PositionSide` | B1, B4 |
| `Position.entryPrice` | `readonly entryPrice: number` | `entryPrice: number` | `readonly entryPrice: number` | `f64` | B1, C1 |
| `Position.notional` | `readonly notional: number` | `notional: number` | `readonly notional: number` | `f64` | B1, C1 |
| `Position.leverage` | `readonly leverage: number` | `leverage: number` | `readonly leverage: number` | `f64` | B1, C1 |
| `Position.marginType` | `readonly marginType?: MarginType` | `marginType?: MarginType` | `readonly marginType?: MarginType` | `Option<MarginType>` | B1, B2, B4 |
| `Position.positionMode` | `readonly positionMode?: PositionMode` | `positionMode?: PositionMode` | `readonly positionMode?: PositionMode` | `Option<PositionMode>` | B1, B2, B4 |
| `Position.unrealizedPnl` | `readonly unrealizedPnl: number` | `unrealizedPnl: number` | `readonly unrealizedPnl: number` | `f64` | B1, C1 |
| `Position.realizedPnl` | `readonly realizedPnl?: number` | `realizedPnl?: number` | `readonly realizedPnl?: number` | `Option<f64>` | B1, B2, C1 |
| `Position.contracts` | `readonly contracts: number` | `contracts: number` | `readonly contracts: number` | `f64` | B1, C1 |
| `Position.liquidationPrice` | `readonly liquidationPrice: number` | `liquidationPrice: number` | `readonly liquidationPrice: number` | `f64` | B1, C1 |
| `Position.marginSize` | `readonly marginSize?: number` | `marginSize?: number` | `readonly marginSize?: number` | `Option<f64>` | B1, B2, C1 |
| `Position.info` | `readonly info?: any` | `info?: any` | `readonly info?: any` | `Option<serde_json::Value>` | B1, B3, C2 |
| `Position.extra` | `readonly extra?: { … }` | `extra?: { … }` | `readonly extra?: { … }` | `Option<PositionExtra>` | B1, B2, C3 |
| `Position.extra.contractsNative` | `readonly contractsNative?: number` | `contractsNative?: number` | `readonly contractsNative?: number` | `Option<f64>` | B1, B2, C1 |
| `Position.extra.unrealizedPnlNative` | `readonly unrealizedPnlNative?: number` | `unrealizedPnlNative?: number` | `readonly unrealizedPnlNative?: number` | `Option<f64>` | B1, B2, C1 |
| `Position.lastUpdate` | `readonly lastUpdate?: number` | `lastUpdate?: number` | `readonly lastUpdate?: number` | `Option<f64>` | B1, B2, C1 |

### Market (24 rows)

| Field | Current types.ts | Raw ts-rs | Final generated | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `Market.id` | `readonly id: string` | `id: string` | `readonly id: string` | `String` | B1 |
| `Market.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `Market.prettySymbol` | `readonly prettySymbol?: string` | `prettySymbol?: string` | `readonly prettySymbol?: string` | `Option<String>` | B1, B2 |
| `Market.base` | `readonly base: string` | `base: string` | `readonly base: string` | `String` | B1 |
| `Market.quote` | `readonly quote: string` | `quote: string` | `readonly quote: string` | `String` | B1 |
| `Market.active` | `readonly active: boolean` | `active: boolean` | `readonly active: boolean` | `bool` | B1 |
| `Market.precision` | `readonly precision: { … }` | `precision: { … }` | `readonly precision: { … }` | `MarketPrecision` | B1, C3 |
| `Market.precision.amount` | `readonly amount: number` | `amount: number` | `readonly amount: number` | `f64` | B1, C1 |
| `Market.precision.price` | `readonly price: number` | `price: number` | `readonly price: number` | `f64` | B1, C1 |
| `Market.limits` | `readonly limits: { … }` | `limits: { … }` | `readonly limits: { … }` | `MarketLimits` | B1, C3 |
| `Market.limits.amount` | `readonly amount: { … }` | `amount: { … }` | `readonly amount: { … }` | `MarketLimitsAmount` | B1, C3 |
| `Market.limits.amount.min` | `readonly min: number` | `min: number` | `readonly min: number` | `f64` | B1, C1 |
| `Market.limits.amount.max` | `readonly max: number` | `max: number` | `readonly max: number` | `f64` | B1, C1 |
| `Market.limits.leverage` | `readonly leverage: { … }` | `leverage: { … }` | `readonly leverage: { … }` | `MarketLimitsLeverage` | B1, C3 |
| `Market.limits.leverage.min` | `readonly min: number` | `min: number` | `readonly min: number` | `f64` | B1, C1 |
| `Market.limits.leverage.max` | `readonly max: number` | `max: number` | `readonly max: number` | `f64` | B1, C1 |
| `Market.contractSize` | `readonly contractSize?: number` | `contractSize?: number` | `readonly contractSize?: number` | `Option<f64>` | B1, B2, C1 |
| `Market.isExpiring` | `readonly isExpiring?: boolean` | `isExpiring?: boolean` | `readonly isExpiring?: boolean` | `Option<bool>` | B1, B2 |
| `Market.isInverse` | `readonly isInverse?: boolean` | `isInverse?: boolean` | `readonly isInverse?: boolean` | `Option<bool>` | B1, B2 |
| `Market.isSpot` | `readonly isSpot?: boolean` | `isSpot?: boolean` | `readonly isSpot?: boolean` | `Option<bool>` | B1, B2 |
| `Market.isReadOnly` | `readonly isReadOnly?: boolean` | `isReadOnly?: boolean` | `readonly isReadOnly?: boolean` | `Option<bool>` | B1, B2 |
| `Market.info` | `readonly info?: any` | `info?: any` | `readonly info?: any` | `Option<serde_json::Value>` | B1, B3, C2 |
| `Market.takerFee` | `readonly takerFee?: number` | `takerFee?: number` | `readonly takerFee?: number` | `Option<f64>` | B1, B2, C1 |
| `Market.makerFee` | `readonly makerFee?: number` | `makerFee?: number` | `readonly makerFee?: number` | `Option<f64>` | B1, B2, C1 |

`Order.remaining` and `Position.contracts` remain base-unit numbers; the nested
native fields remain separately optional. `Order.extra.filledInPlacementUnits`
remains its distinct placement-unit quantity. None is calculated or converted by
this model layer. `Order.price`, `Order.lastUpdate` and all required Position
numbers remain required even when zero; `Position.lastUpdate` remains optional.
`Market.precision` and both `limits` objects remain required, and min/max zero
values are retained. Requiredness comes from the actual declarations; no derivative
field is made optional based on an unavailable-data interpretation. This spike
makes no new per-venue claim about the meaning of any zero.

## Dependent enums — nine, all ported

These are declarations in their own right, listed here for the extractor's refreshed
baseline. No parity manifest rows were added or edited by this branch. Every row
is **B4 / (b), fixed in final output**. Raw literal unions remain committed unchanged,
so both the initial gap and the transformation can be reviewed directly.

| Enum | Current types.ts member → literal | Raw ts-rs | Final generated TypeScript | Decision |
| --- | --- | --- | --- | --- |
| `OrderStatus` | `Open → "open", Closed → "closed", Canceled → "canceled", Creating → "creating", Updating → "updating", Canceling → "canceling"` | `export type OrderStatus = "open" \| "closed" \| "canceled" \| "creating" \| "updating" \| "canceling"` | `export enum OrderStatus { Open = "open", Closed = "closed", Canceled = "canceled", Creating = "creating", Updating = "updating", Canceling = "canceling" }` | B4 fixed: names, values, runtime and enum assignability match. |
| `OrderType` | `Market → "market", Limit → "limit", Conditional → "conditional", StopLoss → "stop_market", TakeProfit → "take_profit_market", TrailingStopLoss → "trailing_stop_market"` | `export type OrderType = "market" \| "limit" \| "conditional" \| "stop_market" \| "take_profit_market" \| "trailing_stop_market"` | `export enum OrderType { Market = "market", Limit = "limit", Conditional = "conditional", StopLoss = "stop_market", TakeProfit = "take_profit_market", TrailingStopLoss = "trailing_stop_market" }` | B4 fixed: names, values, runtime and enum assignability match. |
| `OrderSide` | `Buy → "buy", Sell → "sell"` | `export type OrderSide = "buy" \| "sell"` | `export enum OrderSide { Buy = "buy", Sell = "sell" }` | B4 fixed: names, values, runtime and enum assignability match. |
| `OrderTimeInForce` | `GoodTillCancel → "GoodTillCancel", ImmediateOrCancel → "ImmediateOrCancel", FillOrKill → "FillOrKill", PostOnly → "PostOnly"` | `export type OrderTimeInForce = "GoodTillCancel" \| "ImmediateOrCancel" \| "FillOrKill" \| "PostOnly"` | `export enum OrderTimeInForce { GoodTillCancel = "GoodTillCancel", ImmediateOrCancel = "ImmediateOrCancel", FillOrKill = "FillOrKill", PostOnly = "PostOnly" }` | B4 fixed: names, values, runtime and enum assignability match. |
| `OrderStopTriggerType` | `MarkPrice → "mark_price", LastPrice → "last_price", IndexPrice → "index_price"` | `export type OrderStopTriggerType = "mark_price" \| "last_price" \| "index_price"` | `export enum OrderStopTriggerType { MarkPrice = "mark_price", LastPrice = "last_price", IndexPrice = "index_price" }` | B4 fixed: names, values, runtime and enum assignability match. |
| `OrderRejectReason` | `PostOnly → "PostOnly", InsufficientBalance → "InsufficientBalance", ReduceOnly → "ReduceOnly"` | `export type OrderRejectReason = "PostOnly" \| "InsufficientBalance" \| "ReduceOnly"` | `export enum OrderRejectReason { PostOnly = "PostOnly", InsufficientBalance = "InsufficientBalance", ReduceOnly = "ReduceOnly" }` | B4 fixed: names, values, runtime and enum assignability match. |
| `PositionSide` | `Long → "long", Short → "short"` | `export type PositionSide = "long" \| "short"` | `export enum PositionSide { Long = "long", Short = "short" }` | B4 fixed: names, values, runtime and enum assignability match. |
| `PositionMode` | `Hedged → "hedged", OneWay → "oneway"` | `export type PositionMode = "hedged" \| "oneway"` | `export enum PositionMode { Hedged = "hedged", OneWay = "oneway" }` | B4 fixed: names, values, runtime and enum assignability match. |
| `MarginType` | `Isolated → "isolated", Cross → "cross"` | `export type MarginType = "isolated" \| "cross"` | `export enum MarginType { Isolated = "isolated", Cross = "cross" }` | B4 fixed: names, values, runtime and enum assignability match. |

The initial spike demonstrated two separate failures in raw unions: today's
`OrderSide.Buy` is a runtime member missing from raw output, while a plain `"buy"`
string is rejected by today's enum but accepted by the union. The
[round 1 overseer review](https://github.com/Tealstreet/tealstreet-next/pull/7194#issuecomment-6107720529)
measured **9,977 runtime member-access sites across 674 files** (OrderSide 3,117,
OrderType 2,264, PositionSide 1,540). This is the overseer's measurement, not a
whole-consumer compilation performed by this spike. Its ruling requires real enums
and exact member names; a union plus a const object would fix member access but
still silently widen assignability.

The final generator now meets that ruling. The checker asserts an exported enum
AST with every member name and value, executes each compiled enum and compares its
runtime object, and compiles access to every member. It rejects all 30 bare member
literals in both the generated and baseline types. A union-plus-const negative
control makes the same bare-literal guard fail with TS2578 (unused expected error),
even though runtime member access compiles. Legacy and generated Order,
Position and Market now assign in **both directions**, as do all nine enum types
and individual members. There is no consumer edit and no enum runtime shim.
These checks establish the scoped declaration contract, not universal compatibility
with every unported model, build configuration or consumer runtime.

## Parity manifest integration

The twelve named exports (`Order`, `Position`, `Market` and the nine enums listed
above) now map one-to-one to `safe-cex-types/src/lib.rs::<Name>` in
`../parity/manifest/types.ts.toml`, with `status = "ported"` and `layer = "mirror"`.
The six C3 helpers have exact exemptions in `../parity/exempt.toml`: `MarketLimits`,
`MarketLimitsAmount`, `MarketLimitsLeverage`, `MarketPrecision`, `OrderExtra` and
`PositionExtra`. Each reason names its anonymous TS parent object and explains
that the inline representation adds no exported TS name or behavior.
These declaration mappings retain the C1/C2 limits above.

Round 3 verification reproduced eighteen invention failures before annotation.
After annotation, coverage reports 6,232 symbols across 456 files, zero added or
orphaned rows and zero manifests needing updates; invention and status pass with
6,232 rows and 20 public Rust items. `yarn parity:report` reports:

| Scope | Ported / total | Distinct Rust items claimed |
| --- | --- | --- |
| `types.ts` | 12 / 67 | 12 |
| `safe-cex-types` | 12 / 12 | 12 |
| Total | 12 / 6232 | 12 |

No Rust item is claimed by multiple rows. The declaration checker still passes
72 fields and nine real enums, and regenerated bindings are unchanged.

## CI enforcement

`.github/workflows/rust.yml` runs a dedicated TypeScript parity job on pull requests
and integration/master pushes. Both path filters include this crate, the oracle
`packages/safe-cex/src/types.ts`, the root dependency manifest/lockfile and the workflow.
An oracle-only change therefore fires the same gate as a Rust model change.

The job installs only TypeScript in a runner-isolated directory, using the root
selector/package-manager pin and frozen root Yarn lockfile. It selects that compiler
explicitly with `SAFE_CEX_TYPESCRIPT_PATH`, so a persistent checkout's node_modules
cannot shadow it. Rust retains the verified isolated toolchain/cache configuration.
CI runs the locked Cargo export, structural AST pass, a tracked diff plus untracked
binding check, the Node generator/coverage regressions, and the oracle parity checker.
No whole-monorepo installation or test suite is needed for this declaration gate.

## Evidence and limits

- `node sdk-rs/safe-cex-types/tests/check-types.cjs` compares every property and
  every enum declaration/member pair to the current source using the TypeScript AST.
  It reports 72 fields and 9 real enums, checks raw literal sets and runtime objects,
  tests bidirectional model/enum assignment and uses
  expected compilation failures for readonly mutation, a required numeric
  field set to undefined, nullable optional numbers, and every bare enum literal.
  Runtime access and bidirectional enum assignability are positive checks.
  The fixture compiles only these extracted declarations and generated bindings;
  it is not a whole-package or 55k-LOC consumer typecheck.
- `cargo test -p safe-cex-types --all-targets` checks every enum member's exact
  serialization/deserialization, unknown enum rejection, required numeric fields,
  absent optionals, nested extras, zero values and present/absent info with null.
  Rust generator tests cover nested inline/file module discovery, non-TS enum exclusion, duplicate
  literal rejection, and stale-binding cleanup. AST transformation regressions run
  with `node --test sdk-rs/safe-cex-types/tests/*.test.cjs`.
- `cargo fmt --check`, `cargo clippy -p safe-cex-types --all-targets -- -D warnings`,
  `cargo test -p safe-cex-types`, and the wasm32 build are required local gates.
  Their actual results are recorded below; none implies FFI or exchange
  runtime parity, which is outside this phase.
- After committing generated output, `yarn types:generate && git diff --exit-code`
  verifies the tracked tree does not change. Both raw and final bindings are
  reviewable, and generation uses `Config::new()` with a crate-relative absolute
  output path, independent of TS_RS environment configuration or caller cwd.

The result supports Rust ownership of these finite JSON object shapes with explicit
optional and readonly policy. It does not establish complete type ownership for
unported types, arbitrary JS info values, nonfinite numbers,
FFI transport or runtime behavior. No baseline cutover or consumer rewrite is part
of P0d.

### Actual local gate results

The initial spike gates passed after sub-agent implementation and document audits.
The results below are refreshed after the round 2 generator review:

| Gate | Actual outcome |
| --- | --- |
| `cargo fmt --check` (sdk-rs) | Pass; no formatting changes. |
| `cargo clippy -p safe-cex-types --all-targets -- -D warnings` | Pass; no warnings. |
| `cargo test -p safe-cex-types` | Pass; 2 wire tests, 0 failures. |
| `cargo test -p safe-cex-types --all-targets` | Pass; 2 wire tests and 3 generator tests, 0 failures. |
| `node --test sdk-rs/safe-cex-types/tests/*.test.cjs` | Pass; 5 AST/coverage regressions, 0 failures. |
| `cargo build -p safe-cex-types --target wasm32-unknown-unknown` | Pass. |
| `node sdk-rs/safe-cex-types/tests/check-types.cjs` | Pass; 72 fields, 9 real enums, runtime members, bidirectional assignments and 30 bare-literal rejections per side. |
| Original negative checker probe | Making generated `Order.parentId` required failed with `Order.parentId: optionality`; restored output passed. |
| Round 2 end-to-end probes | Rust field doc comment preserved 24/24 Market readonly properties; MarginType in inline and file modules remained a real enum; ordinary InternalError was allowed; raw/final Ghost bindings were removed. Restored source regenerated successfully. |
| CI dependency/gate rehearsal | Isolated frozen Yarn install selected TypeScript 6.0.3; locked generation, 5 Node regressions, oracle checker and binding determinism passed. |
| Hosted parity CI | [Actions run 38131269744](https://github.com/Tealstreet/tealstreet-next/actions/runs/38131269744), commit `081eac07a7`: Rust / TypeScript parity completed successfully, including isolated compiler install, locked generation, tracked/untracked determinism, regressions and oracle comparison. |
| CI untracked-output negative control | An injected Unexpected.ts was detected by the untracked binding guard; removed after the probe. Both event path filters were checked for the oracle and dependency inputs. |
| Enum declaration negative probe | Replacing final OrderSide with its raw union failed with `OrderSide: enum declaration required`; restored output passed. |
| Member-name negative probe | Changing `Buy` to `BUY` failed with `OrderSide: member names/values`; restored output passed. |
| Union-plus-const compiler control | Runtime access compiled, but the bare-literal expected-error guard yielded TS2578; permissiveness detected. |

The first wire test run failed because serde_json distinguishes integer and float
Number representations (`0` vs `0.0`). The comparison now checks numeric values
as doubles while retaining exact keys, optional absence and nested structure.
The corrected gates above passed. Whole-package TS lint/typecheck/tests and runtime
conformance were not run: the TypeScript package is unchanged, and this spike's
checker isolates the declarations instead of running unrelated consumers.

## P1a core extension

The core extension adds Ticker, Balance/CoinBalance, Execution, Candle,
OrderBook/OrderBookOrders, PublicTrade, historical fills/orders, placement and
history options, and small store metadata shapes. The oracle calls the order-book
row `OrderBookOrders` (plural). The checker now covers 227 properties and still
independently derives nine runtime enums from the oracle dependency graph.

B1 now preserves mixed mutability: Rust field documentation `@ts-mutable` tells
the structural AST pass which properties remain writable. CoinBalance and order
books are mutable, as in the oracle. Other properties retain readonly; arrays
remain mutable arrays. Oracle readonly flags remain the independent assertion.
Literal aliases use Rust `@ts-union` intent with separately committed metadata;
Timeframe remains a string union rather than gaining enum nominality.
ExecutionPositionSide inlines PositionSide plus the bare literal `both` (C3).
Balances preserve recursive currency maps, mutable coin balances, and info null
(C2). Ticker derivatives remain required f64 fields with zero sentinels (C1).
Execution timestamp stays in seconds; historical trade time stays in milliseconds.

The field checker normalizes Array<T>/T[] and quote spelling, preserves union
identity, and checks index key domains while ignoring parameter names. An audit
proved that string-to-number index-key drift passed bidirectional assignment;
the added negative regression now rejects it structurally. Generator regressions
also cover nested mutable properties and explicit literal-alias intent.

Phase gates: fmt, warning-denying crate clippy, all-targets tests (4 wire and
3 generator tests), wasm build, 8 Node regressions and the declaration checker
passed. This proves finite JSON declarations/storage, with the existing C1/C2
limits; no callback invocation, exchange implementation or consumer cutover is
claimed.

## P1a options, account configuration and metadata

The second extension covers ExchangeName, ExchangeOptions, ExchangeAccountConfig,
PublicExchangeOptions, ValidateAccountResult, ExchangeAccount, UpdateOrderOpts,
timeframe aliases, income/provenance records and request/store metadata. The
oracle has exactly nine local runtime enum declarations; all nine were already
ported in P0. New literal aliases stay aliases, with their raw value sets and
Rust declaration intent committed in `string-unions.json`. `@ts-shape` marks
inline object/scalar unions, which must not become TypeScript runtime enums.

The checker compares interfaces as object shapes and resolves the
PublicExchangeOptions Omit/intersection for structural field comparison. Its
separate compiler fixture still compiles the original intersection. Public data
options require literal true, preserve absent credentials, and reject false;
no general boolean validation is introduced. ExchangeAccount preserves absent
versus present-null subId/affiliateId. UpdateOrderOpts retains both amount and
price when both are present: the TS union permits that object, so selecting an
untagged Rust variant must not discard the other declared key.

**C4 — (c), deliberate in-process capability boundary:** ExchangeOptions and
PublicExchangeOptions reproduce all TS field declarations, but Rust stores only
the finite JSON domain. RemoteSigner, PayloadSigner, TransportProvider and
PublicDataSource remain TypeScript-owned runtime contracts. Generated bindings
import their exact original types through `runtime-boundaries.ts`; none becomes
any, JSON, a renamed callback interface or a Rust invocation implementation.
Rust uses four distinct uninhabited capability markers. Absence is representable;
a present marker has no constructible value and every JSON representation,
including null, fails deserialization. The websocket external-public-data variant
round-trips; the in-process variant cannot cross this boundary. These symbols and
related signing request/result shapes remain pending for the IO/FFI phase.
Both options manifest rows explicitly qualify the finite-data/declaration claim.

The isolated assignment fixture substitutes distinct unique-symbol opaque types
for these four boundaries only. Structural comparison first verifies the actual
oracle reference identifiers. `check-runtime-imports.cjs` separately compiles all
real generated bindings and checks that every boundary export resolves to the
original TS declaration. It checks diagnostics on generated files and import
identity, not the whole package's dependency diagnostics or capability behavior.
This is deliberately narrower than whole-consumer cutover proof.

The current manifest reports 52/67 declarations in types.ts, 52 distinct Rust
items, and 52/6232 overall, up from 12/6232. The remaining rows cover derived/store
aggregates, runtime callbacks/signing messages and Writable utilities;
those are not newly marked ported. TS types.ts and consumers are unchanged.
Phase 2 clippy, all-targets tests (6 wire plus 3 generator), oracle declaration
comparison (416 fields, 9 enums), real-import checks and invention/status gates
passed. P1a final CI and determinism evidence follows separately.

## P1a final enforcement and evidence

[Field and alias triage](P1A_FIELDS.md) records all 358 new property/index
rows beside the oracle, raw output, final output and Rust storage. Combined with
P0's 72 rows, the final checker reports 430 fields. Nested object-union branches
are compared structurally and counted; their mutability, optionality and index
key domains have negative regressions. Mutable oracle assignments and bare
literal-alias assignments compile, while readonly mutations, unavailable required
Ticker numbers and bare PositionSide strings are rejected.

The workflow retains both types.ts oracle path filters and the existing locked
regeneration/determinism/AST gates. It adds the transport/types.ts oracle input,
the real boundary-import gate, and the exact P1a feature push trigger so this
branch can receive a hosted run without opening a PR. The isolated frozen-lockfile
TypeScript install passed. Nine Node regressions, the 430-field/nine-enum gate,
and the import identity check passed with the isolated compiler. A source-only
copy with no monorepo node_modules also passed the import check; this confirms
its deliberate generated-file/import scope, not imported dependency correctness.

Actual negative probes used copied oracles; production types.ts was never edited:

- Changing generated Ticker.mark to optional failed with `Ticker.mark: optionality`.
- Removing readonly from the copied oracle alone failed with `Ticker.mark: readonly`.
- Adding a temporary real Rust Tenth enum and optional Ticker dependency, generating
  bindings, and checking a copied oracle discovered ten real enums automatically.
  Replacing its final declaration with the raw union failed with
  `Tenth: enum declaration required`.
- The untracked-binding guard detected an injected Unexpected.ts.

Temporary Rust/source/binding probes were restored and regeneration returned to
the original nine-enum declaration gate. The coverage gate reports 6,232 symbols
across 456 files, zero added/orphaned rows and zero manifests to update. The
invention/status gates pass with 6,232 rows and 92 public Rust items. The report
shows 52/67 types.ts rows and 52/6232 total, with 52 distinct items claimed.


Final local gates passed: cargo fmt --check; warning-denying safe-cex-types
all-targets clippy; all-targets tests (6 wire and 3 generator, zero failures);
wasm32 build; 9 Node regressions; 430-field/nine-enum declaration parity;
real runtime imports; coverage, invention/status and parity report.
Hosted Actions evidence and post-commit determinism are recorded in the task
completion report so this document does not claim a run before it completes.

## Literal-union Rust identifiers

TypeScript string unions supply literals, not member identifiers. The Rust
binding generator derives UpperCamelCase names for every `@ts-union` enum:
`rateLimit` becomes `RateLimit`, `realized_pnl` becomes `RealizedPnl`, and
`BRA_BTL` becomes `BraBtl`. Hyphens, underscores and camel/acronym boundaries split words;
letters within each word are lowercased after its initial capital.

Before general identifier conversion, one or more ASCII digits followed by one
case-sensitive interval unit derive a unit-first identifier. The explicit map
is `s → S`, `m → M`, `h → H`, `d → D`, `w → W`, `M → MN`: `1m → M1` and
`1M → MN1` distinguish minute from month. Digit spelling is preserved.

`ValueN` (zero-based literal position) is a per-variant fallback only. Outside
that interval rule, ASCII letters and digits plus hyphen/underscore separators
must derive a valid Rust identifier. Other punctuation, non-ASCII literals,
empty names and bare numbers fall back. Keyword comparison uses the exact
**derived identifier**, preserving case: `static → Static`, `type → Type`, and
`crate → Crate` are legal. Raw identifiers such as `r#fn` are an escape hatch
where Rust permits them; `Self` remains reserved and cannot become `r#Self`.
Collisions between normalized literals and the reserved `Value` plus digits
fallback namespace also require fallback. Every colliding variant falls back;
other variants keep semantic names. Ordinary comments name each fallback
literal and reason, so they do not alter ts-rs documentation or TypeScript output.

`cargo run -p safe-cex-types --example generate -- --union-names` emits the
mechanical name plan, including fallback explanations, for source authoring.
Normal generation validates the convention before replacing any bindings.
Serde renames retain the exact wire values. RequestOutcome exposes `Ok`,
`RateLimit`, `Auth`, `Network`, and `Other` for P1d requestStats callers.

Round 1 verification compared the complete generated TypeScript file set and
SHA-256 hashes before and after: all 105 raw/final `.ts` files were byte-identical.
Only Rust-name metadata changed. Seven wire tests include the live P1d outcome
API and exact serialization; five generator tests include stale-name rejection,
normalization collisions, reserved words and numeric/hyphenated fallbacks.
The nine Node regressions, 430-field/nine-enum oracle check, real boundary imports,
wasm build, warning-denying clippy, fmt, manifest extraction/check/report all
passed; manifest coverage remains 52/6232.

Round 2 inspection confirms AnyTimeframe does **not** flatten the literal unions:
Rust retains an untagged wrapper around SecondsTimeframe, Timeframe, and
ExtendedTimeframe; generated TS retains their union. Minute and month therefore
inhabit different inner enums. The consumer regression nonetheless constructs
`AnyTimeframe::Variant1(Timeframe::M1)` and
`AnyTimeframe::Variant2(ExtendedTimeframe::MN1)`, deserializes each exact branch,
and checks unchanged wire strings. A synthetic combined enum tests distinct
`M1`/`MN1`; a `1m`/`m1` collision proves interval names still use collision checks.
The rule replaces 27 timeframe fallbacks; four remain for hyphenated literals
and the reserved word `static`, reducing 31 fallbacks to four.

Round 2 passed eight wire tests, six generator tests, the same nine Node tests,
all local parity/build/lint gates, and the full 105-file TypeScript byte check.

Round 3 derives `LighterRobinhood`, `TealstreetDex`, `InProcess`, and `Static`,
leaving **zero** positional fallback variants in the current production model.
Negative controls still cover genuine reserved `Self`, separator/camel collisions,
unsupported punctuation and invalid identifier results. The case-sensitive
interval map and both minute/month tests remain. AnyTimeframe retains separate
inner enums by construction, so no shared Rust variant namespace exists today;
the synthetic combined-enum test protects the map if flattening is added later.

Round 3 passed nine wire tests, seven generator tests, nine Node regressions,
430-field/nine-enum oracle parity, real boundary imports, native lint/fmt,
wasm build, manifest extraction/check/report, and the unchanged 105-file TS
path/hash inventory. The production oracle remains untouched.


## Risk result dependency (P1d)

OrderRiskProfile is the mutable types.ts result row required by the risk engine,
owned here under D1. All eight fields match the oracle; optional full-stop/text/fee
fields preserve omission (present null is rejected). TS mutability markers keep
the generator from adding readonly. The export root and bidirectional field
checker include it; safe-cex-store imports this canonical definition.

## Shared gateway wire vocabulary (P4a registry review)

`gateway` owns external-public-data's wire types below all venue crates. It
adds no generated bindings to the types.ts model. Four envelope constructors
keep literal tags valid for producers. `GatewayChannel` deliberately includes
the runtime `auth` cast omitted by the TS union; `GatewayMessage` deliberately
rejects typed optional null fields. Both differences are recorded in the
external-public-data manifest. Registry descriptors remain in safe-cex-registry.

## Strict input shapes

String enums require a JSON string before a private serde-derived companion
performs canonical decoding. Named records require a map before the companion
runs. This closes serde's externally tagged enum objects and positional record
arrays without changing valid field decoding, flattening, or untagged unions.
Companion metadata is checked against the original model declarations; accepted
spelling metadata is checked independently of output round trips, with a live
TypeScript AST comparison for the named domain unions. Private-companion access
has independent compile-fail controls, without relying on inert error codes.

The repair inventory is **104 definitions: 36 inhabited string enums, 64 named
records (including two store-contract records), and four uninhabited capability
markers**. The four markers cannot be exercised and retain their existing
implementation; they are neither observed widenings nor verified strict codecs.
The output-only account error/result records and the explicitly array-shaped
PackedTickerDelta are outside the record decode inventory.

### Balance dictionary enumeration repair

Balance.currencies and coins are JsObjectMap rather than lexical BTreeMap. The
map's private backing IndexMap cannot be mutated through DerefMut: insert and
collect preserve canonical integer-index ordering, while non-index names retain
insertion order. This applies recursively through Balance values and to Serialize.
Map-only Deserialize rejects positional arrays. Own data properties are modelled;
prototype replacement and inherited referent mutation remain separate findings.
The backend lives in types::js_coercion, with num delegating, so account and model
key enumeration share the same canonical array-index rule without a layer cycle.

Balance repair verification: 196 live Node key-order cases run through real
DefaultStore/Store facade wiring, including nested currencies, coins and actual
serialized bytes. Typed writes cover replacement and delete/reinsert; a suffix
selector asserts insertion-first margin-asset selection. A lexical-sort mutation
compiled and failed BOTH named outer-order and wrong-asset assertions; restoring
returned both tests green. Locked scoped compile, separate host and wasm strict
Clippy, parity invention/status and 37-crate layer audit pass. Independent
read-only review returned MERGE. No prototype mutation or numeric ingress claim
is included in this ordering repair.
