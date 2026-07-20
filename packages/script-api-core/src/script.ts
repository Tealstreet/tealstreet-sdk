/**
 * User-facing surface for `@tealstreet/script-api-core`. Phase 6 v1.1.
 *
 * This file is the ENTRY POINT the Phase 6 types generator
 * (`scripts/generate-monaco-types.ts`) walks. It re-exports only the
 * symbols a user script in `~/.tealstreet/scripts/` can legitimately
 * reach — `CoreApi`, `ScriptArgs`, `ScriptMeta`, the domain types and
 * opts, the runtime enums, `CORE_CONSTANTS` / `CORE_UTILS`, and the
 * pure utility functions.
 *
 * **Why a second entry exists.** The package's main entry (`index.ts`)
 * re-exports both the user surface AND host-side internals
 * (`InProcessProxy`, `runScript`, `buildApi`, `validateTargets`,
 * capability validators, transpile helpers, etc.). Hosts that wire
 * the runtime — CLI's `script-runner.ts`, web's worker — need that
 * full surface. The generator can't target `index.ts` because
 * `dts-bundle-generator` would inline the host symbols into the
 * seeded `~/.tealstreet/scripts/types.d.ts`, and a novice
 * autocompleting `import { InProcessProxy } …` (as a value, not type)
 * would hit a cryptic `Cannot find module` at script-load (the seeded
 * dir has no installed npm package backing the ambient types).
 *
 * **What this file is NOT.** It is NOT consumed by host code — every
 * existing host consumer keeps importing from
 * `@tealstreet/script-api-core` (resolves to `package.json`'s
 * `main: "src/index.ts"`). Only the Phase 6 generator points here.
 *
 * **Drift safety.** The negative assertions in
 * `apps/cli/src/script-types-bundle.test.ts` fail if a host-only
 * symbol leaks into the bundle (e.g. someone accidentally adds
 * `InProcessProxy` to the re-export list below).
 */

export { CORE_API_VERSION } from './version';

// Type contract — every interface/opts a user script needs.
export type {
  AccountInfo,
  AccountOpts,
  Balance,
  Candle,
  CandleOpts,
  CancelFilter,
  ClosePositionOpts,
  CombinedBalance,
  CombinedBalanceOpts,
  CoreApi,
  CoreConstants,
  CoreExchange,
  CoreIo,
  CoreMeta,
  CoreStorage,
  CoreUtils,
  ExchangeName,
  ExchangeNameMap,
  FormatPnlOpts,
  HistoricalPosition,
  HistoricalPositionQuery,
  InvokedFrom,
  Market,
  Order,
  OrderBook,
  OrderQueryOpts,
  OrderbookOpts,
  PlaceOrderOptions,
  Position,
  PositionOpts,
  PrintKind,
  ReturnsResult,
  RiskResult,
  RunCommandResult,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from './api/CoreApi.types';

// Runtime enums — user scripts use these for string-side discriminants.
export {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from './api/CoreApi.types';

// Capability declaration types — go into `export const meta`.
export type {
  Capabilities,
  ReadMethodName,
  RunCommandCapability,
  ScriptMeta,
  StorageCapability,
  TargetName,
  WriteMethodName,
} from './api/capabilities.types';

// Args shape — the first parameter on the user's default-exported function.
export type { ScriptArgs } from './api/args.types';

// Frozen runtime bundles — `CORE_CONSTANTS` (exchange names, side enums
// pinned to runtime literals) and `CORE_UTILS` (the same functions
// re-exported individually below, packaged as one object).
export { CORE_CONSTANTS } from './constants';
export { CORE_UTILS } from './utils';

// Individual util exports — user scripts can either call these directly
// or reach them via `api.utils.*`. Kept as named exports so editor
// "go to definition" lands on the implementation file.
// `calculateOrdersRisk` is omitted — see `src/utils/index.ts`.
export {
  adjust,
  calculateReturns,
  convertSymbol,
  formatPnl,
  getBBOMid,
  getCandleEndTime,
  getCandleStartTime,
  getFarTouchPrice,
  getMarginCurrency,
  getNearTouchPrice,
  getNumCandlesInRange,
  getTickerPredictedBestAsk,
  getTickerPredictedBestBid,
  normalizeEpochToSeconds,
  sleep,
  uuid,
} from './utils';
