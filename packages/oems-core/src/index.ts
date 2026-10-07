// Public surface for @tealstreet/oems-core.
// Package entry point — the one place a barrel is allowed in this repo.

export { isOemsReadMethod, isOemsWriteMethod, OEMS_READ_METHODS, OEMS_WRITE_METHODS } from './surface';
export type { OemsReadMethod, OemsWriteMethod } from './surface';

export type {
  OemsAccountRef,
  OemsBalanceRef,
  OemsContext,
  OemsMarketRef,
  OemsOrderAddress,
  OemsOrderRef,
  OemsOrderType,
  OemsPositionAddress,
  OemsPositionMode,
  OemsPositionRef,
  OemsPositionSide,
  OemsSide,
} from './context.types';

export type {
  OemsAmendOrder,
  OemsCancelOrders,
  OemsClosePositions,
  OemsIntent,
  OemsIntentKind,
  OemsOrderScope,
  OemsPlaceOrder,
  OemsPlaceStopLoss,
  OemsPlan,
  OemsPositionScope,
  OemsReducePositions,
  OemsSetLeverage,
} from './intent.types';

export { OEMS_PLAN_SCHEMA, OEMS_PLAN_SCHEMA_NAME, OEMS_PLAN_SCHEMA_VERSION } from './planSchema';

export { OemsPlanError, parseOemsPlan } from './parsePlan';

export { resolveOemsPlan } from './resolvePlan';
export type { OemsCommand, OemsOverride, OemsRejection, OemsResolution, ResolveOptions } from './resolvePlan';

// The rounding arithmetic is NOT re-exported. It lives in
// `@tealstreet/script-api-core/src/order/rounding` and callers import it from
// there. Re-exporting it here would make `roundMarketOrderSize` name two
// different functions with different signatures — the step-based one and the
// `Market`-shaped one in `orderPricing.ts` — reachable by two routes.

export {
  boundOemsContext,
  boundOemsTranscript,
  boundReadResult,
  boundReadResults,
  OEMS_CONTEXT_LIMITS,
  OEMS_READ_RESULT_LIMITS,
} from './boundContext';

export {
  AGENT_READ_TOOL_NAMES,
  AGENT_READ_TOOLS,
  buildReadToolsBlock,
  MAX_READ_CALLS_PER_TURN,
  parseReadCalls,
  READ_CALL_SCHEMA,
} from './readTools';
export type { ReadCallRejection, ReadCallResolution, ReadCallSchema, ResolvedReadCall } from './readTools';

export { buildOemsPrompt, OEMS_MAX_HOPS, OEMS_PROMPT_VERSION } from './prompt';
export type { BuiltOemsPrompt, OemsPromptInput, OemsReadResult, OemsTranscriptTurn } from './prompt';
