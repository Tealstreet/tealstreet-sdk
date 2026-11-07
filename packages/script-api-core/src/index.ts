// Public surface for the @tealstreet/script-api-core package.
// See docs/custom-scripts/SPEC.md.

export { CORE_API_VERSION } from './version';

// API contract — types only, no runtime values.
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
  UpdateOrderPatch,
} from './api/CoreApi.types';

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

export type {
  Capabilities,
  ReadMethodName,
  RunCommandCapability,
  ScriptMeta,
  StorageCapability,
  TargetName,
  WriteMethodName,
} from './api/capabilities.types';

export type { ScriptArgs } from './api/args.types';

// Frozen constants + utils bundles that hosts plug into the script's `api`.
export { CORE_CONSTANTS } from './constants';
export { CORE_UTILS } from './utils';

// ─────────────────────────────────────────────────────────────────────
// HOST-SIDE EXPORTS BELOW — used by CLI/web hosts that embed the
// runtime. NOT in the user-facing types bundle.
//
// Phase 6 v1.1 shipped the entry-point split: `src/script.ts` is the
// dedicated user-facing surface, and the Phase 6 types generator
// points there instead of here. The seeded `~/.tealstreet/scripts/
// types.d.ts` no longer contains the host symbols below, so
// completions for `InProcessProxy` / `runScript` / `buildApi` / etc.
// no longer surface in user editors.
//
// This file (`index.ts`) is what `package.json:main` resolves, so all
// host consumers (`apps/cli/src/script-runner.ts`, the web worker
// shim, etc.) keep importing from `@tealstreet/script-api-core` and
// reach the full surface — both the user-facing re-exports above and
// the host-side internals below.
//
// The `@internal` JSDoc tags below remain as repo-internal markers
// for contributors reading the source. They were also valuable when
// the user bundle still leaked these symbols; today's bundle drift
// test (`packages/script-authoring/src/script-types-bundle.test.ts`) enforces that
// host symbols never re-appear in the seeded `types.d.ts`.
// ─────────────────────────────────────────────────────────────────────

/** @internal Host-side — adapter contract. User scripts should never reach this. */
export type { CoreAdapter, MaybePromise } from './adapter/AdapterContext';
/** @internal Host-side — proxy seam. */
export type { ApiProxy } from './proxy/ApiProxy';
/** @internal Host-side — concrete proxy. Importing as a VALUE in a user script fails at load. */
export { InProcessProxy } from './proxy/InProcessProxy';

/** @internal Host-side — target validation. */
export { validateTargets, type ScriptDescriptor, type RuntimeDescriptor } from './runtime/targetCheck';
/** @internal Host-side — target API surface descriptors. */
export {
  CLI_API_VERSION,
  AGENT_READ_API_PATHS,
  CORE_READ_API_PATHS,
  CORE_SHARED_API_PATHS,
  CORE_WRITE_API_PATHS,
  CLI_READ_API_PATHS,
  MOBILE_READ_API_PATHS,
  OEMS_API_PATHS,
  OEMS_READ_API_PATHS,
  OEMS_WRITE_API_PATHS,
  PORTABLE_READ_ONLY_API_PATHS,
  RUNNABLE_TARGETS,
  RUNTIME_SURFACES,
  getAllowedApiPathsForTargets,
  getApiPathAvailability,
  isKnownApiPath,
  narrowToRunnableTargets,
  type ApiPath,
  type ApiPathAvailability,
  type RunnableTargetName,
  type RuntimeSurfaceDescriptor,
} from './runtime/runtimeSurface';
/** The portable `CoreAdapter` body and the host seam it is written against.
 *  A runtime supplies `HostBindings`; it does not implement `CoreAdapter`. */
export { createCoreAdapter } from './adapter/createCoreAdapter';
export { ScriptBatchOrderError } from './adapter/ScriptBatchOrderError';
export { bySymbol } from './adapter/HostBindings';
export type { HostAccount, HostBindings, HostExchange, HostWriteEvent } from './adapter/HostBindings';
export { ScriptValidationError } from './order/ScriptValidationError';
export {
  isStopLikeOrderType,
  roundMarketCloseSize,
  roundMarketOrderSize,
  roundOrderPrice,
  roundOrderSize,
  roundOrderSizeToNearest,
  venueAutoDownsizesReduceOrders,
  venueSupportsInfinityStops,
} from './order/orderPricing';
export { notionalToNativeSize } from './order/notionalSizing';
export { clampReduceOnlyOrder, prepareOrder } from './order/prepareOrder';
export type { OrderPolicyContext, ReduceOnlyClampContext } from './order/prepareOrder';

/** @internal Host-side — account resolution (SPEC §2.6). */
export { resolveAccount } from './runtime/accountResolver';

/** @internal Host-side — args tokenizer (SPEC §7.1). */
export { parseScriptArgs } from './runtime/argParser';

/**
 * @internal Host-side — capability validators (SPEC §4). Fire BEFORE the
 * adapter is called. Not meant for user scripts.
 */
export {
  MAX_RUN_COMMAND_DEPTH,
  assertIdempotentForRetry,
  assertWithinStorageQuota,
  validateExchangeCapability,
  validateReadCapability,
  validateRunCommandCapability,
  validateWriteCapability,
} from './runtime/capabilityCheck';

/** @internal Host-side — transpile contract (SPEC §13). */
export { stripScriptTypeImports, wrapScriptBody, type TranspileFn } from './runtime/transpile';

/** @internal Host-side — canonical script entry (SPEC §6). User scripts are CALLED by `runScript`, they don't call it. */
export {
  runScript,
  type RunContext,
  type RunScriptResult,
  type ScriptFn,
  type ScriptOutputLine,
  type ScriptRecord,
} from './runtime/runScript';
/** @internal Host-side — `api` factory. */
export { buildApi, createScriptIoSink, type BuildApiInputs, type ScriptIoSink } from './runtime/buildApi';

/** @internal Host-side — capture-syntax writeback (SPEC §7.4). */
export { normalizeReturnForCapture, type CaptureWriteback } from './runtime/captureWriteback';

// ─────────────────────────────────────────────────────────────────────
// USER-FACING UTILS — safe to import from user scripts.
// ─────────────────────────────────────────────────────────────────────

// Individual util exports — user scripts CAN call these directly, but
// they're also available via `api.utils.*`. `calculateOrdersRisk` is
// omitted — see `src/utils/index.ts`.
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
