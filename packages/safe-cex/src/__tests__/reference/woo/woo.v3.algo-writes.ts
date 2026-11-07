import type { WooV3Decimal } from './woo.v3.contracts';
import type { WooV3AlgoOrder } from './woo.v3.private';
import type { WooV3RequestInput } from './woo.v3.request';

import { WOO_V3_ENDPOINTS } from './woo.v3.contracts';

/**
 * Inert, transport-free contracts for the namespaced V3 algo-order writes.
 * The live Woo exchange intentionally does not import this file.
 */

type UnknownRecord = Record<string, unknown>;

export type WooV3AlgoWriteTarget = Readonly<{ algoOrderId: number } | { clientAlgoOrderId: number }>;
export type WooV3AlgoTriggerPriceType = 'MARKET_PRICE' | 'MARK_PRICE' | 'INDEX_PRICE' | 'LAST_PRICE';

type WooV3AlgoCommonInput = Readonly<{
  symbol: string;
  side: 'BUY' | 'SELL';
  positionMode: 'ONE_WAY' | 'HEDGE_MODE';
  clientAlgoOrderId?: number;
  algoOrderTag?: string;
}>;

export type WooV3StopMarketPlaceInput = WooV3AlgoCommonInput &
  Readonly<{
    family: 'STOP_MARKET';
    quantity: WooV3Decimal;
    triggerPrice: WooV3Decimal;
    triggerPriceType: WooV3AlgoTriggerPriceType;
  }>;

export type WooV3TrailingPlaceInput = WooV3AlgoCommonInput &
  Readonly<{
    family: 'TRAILING_STOP';
    quantity: WooV3Decimal;
    /** Explicit vendor-native input; never construct this from a safe-cex absolute trigger. */
    callback: Readonly<{
      source: 'WOO_NATIVE_CALLBACK_VALUE';
      value: WooV3Decimal;
    }>;
    activatedPrice?: WooV3Decimal;
  }>;

export type WooV3PositionalTriggerInput = Readonly<{
  triggerPrice: WooV3Decimal;
  triggerPriceType: WooV3AlgoTriggerPriceType;
}>;

export type WooV3PositionalPlaceInput = WooV3AlgoCommonInput &
  Readonly<{
    family: 'POSITIONAL_TP_SL';
    stopLoss?: WooV3PositionalTriggerInput;
    takeProfit?: WooV3PositionalTriggerInput;
  }>;

export type WooV3AlgoPlaceInput =
  | WooV3StopMarketPlaceInput
  | WooV3TrailingPlaceInput
  | WooV3PositionalPlaceInput;

export type WooV3AlgoPlaceBuildResult =
  | Readonly<{
      ready: true;
      request: WooV3RequestInput;
      family: WooV3AlgoPlaceInput['family'];
      triggerSemantics:
        | 'official-last-price-trigger'
        | 'vendor-native-callback-value-no-safe-cex-conversion'
        | 'official-last-price-positional-children';
    }>
  | Readonly<{
      ready: false;
      blocker: 'only-market-price-trigger-is-documented';
      family: WooV3AlgoPlaceInput['family'];
    }>;

export type WooV3AlgoPlaceResponseRow = Readonly<{
  algoOrderId: number;
  clientAlgoOrderId: number;
  algoType: 'STOP' | 'TRAILING_STOP' | 'POSITIONAL_TP_SL' | 'TAKE_PROFIT' | 'STOP_LOSS';
  quantity: WooV3Decimal;
}>;

export type WooV3AlgoPlaceDecodeResult =
  | Readonly<{
      ready: true;
      row: WooV3AlgoPlaceResponseRow;
      timestamp: number;
      identityEvidence: 'response-echoed-algo-order-id';
    }>
  | Readonly<{
      ready: false;
      blocker: 'positional-response-parent-child-identity-requires-demo-evidence';
      rows: readonly WooV3AlgoPlaceResponseRow[];
      timestamp: number;
    }>;

export type WooV3AlgoAmendInput = Readonly<{
  currentOrder: WooV3AlgoOrder;
  target: WooV3AlgoWriteTarget;
  update: Readonly<{
    triggerPrice?: WooV3Decimal;
    triggerPriceType?: WooV3AlgoTriggerPriceType;
    quantity?: WooV3Decimal;
    callbackValue?: WooV3Decimal;
  }>;
}>;

export type WooV3AlgoAmendBuildResult =
  | Readonly<{
      ready: true;
      request: WooV3RequestInput;
      target: WooV3AlgoWriteTarget;
      updateSemantics: 'untriggered-zero-fill-stop-trigger-only';
    }>
  | Readonly<{
      ready: false;
      blocker:
        | 'target-id-mismatch'
        | 'terminal-or-triggered-algo-cannot-be-amended'
        | 'partial-fill-algo-amend-semantics-require-demo-evidence'
        | 'quantity-amend-semantics-require-demo-evidence'
        | 'positional-child-update-not-documented'
        | 'trailing-callback-update-not-documented'
        | 'only-market-price-trigger-is-documented'
        | 'stop-trigger-provenance-incomplete'
        | 'algo-family-update-not-supported';
      target: WooV3AlgoWriteTarget;
    }>;

export type WooV3AlgoMutationResult = Readonly<{
  accepted: true;
  target: WooV3AlgoWriteTarget;
  timestamp: number;
  identityEvidence: 'request-target-only-response-does-not-echo-id-or-status';
  gatewayData: Readonly<UnknownRecord>;
}>;

export type WooV3AlgoCancelPendingBuildResult =
  | Readonly<{
      ready: true;
      request: WooV3RequestInput;
      scope: 'all-algo-orders-all-symbols';
    }>
  | Readonly<{
      ready: false;
      blocker: 'symbol-scoped-pending-cancel-query-schema-conflicts-with-description';
      scope: Readonly<{ symbol: string }>;
    }>;

export type WooV3AlgoCancelPendingResult = Readonly<{
  accepted: true;
  scope: 'all-algo-orders-all-symbols';
  timestamp: number;
  gatewayData: Readonly<UnknownRecord>;
}>;

export class WooV3AlgoWriteResponseError extends Error {
  readonly code?: number;
  readonly gatewayResponse: Readonly<UnknownRecord>;
  readonly contractStatus = 'vendor-error-envelope-undocumented' as const;

  constructor(message: string, gatewayResponse: UnknownRecord, code?: number) {
    super(message);
    this.name = 'WooV3AlgoWriteResponseError';
    this.gatewayResponse = Object.freeze({ ...gatewayResponse });
    this.code = code;
  }
}

export const WOO_V3_ALGO_WRITE_PARITY = Object.freeze({
  requestAndResponseFixtures: true,
  runtimeImported: false,
  credentialedDemoLifecycle: false,
  triggerPriceType: 'MARKET_PRICE-only',
  trailingInput: 'vendor-native-callbackValue-no-safe-cex-conversion',
  positionalResponseIdentity: 'blocked-pending-demo-evidence',
  positionalUpdate: 'blocked-official-edit-schema-has-no-childOrders',
  partialFillAndQuantityAmend: 'blocked-pending-demo-evidence',
  symbolPendingCancel: 'blocked-published-query-schema-conflicts-with-prose',
  cancelResponseIdentity: 'request-target-only',
} as const);

const DECIMAL_PATTERN = /^(?:\d+(?:\.\d*)?|\.\d+)$/;

const requireRecord = (value: unknown, path: string): UnknownRecord => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${path} must be an object`);
  return value as UnknownRecord;
};

const requireArray = (value: unknown, path: string): readonly unknown[] => {
  if (!Array.isArray(value)) throw new Error(`${path} must be an array`);
  return value;
};

const requireBoolean = (value: unknown, path: string): boolean => {
  if (typeof value !== 'boolean') throw new Error(`${path} must be a boolean`);
  return value;
};

const requireInteger = (value: unknown, path: string, minimum = 0): number => {
  if (!Number.isSafeInteger(value) || (value as number) < minimum) {
    throw new Error(`${path} must be a safe integer of at least ${minimum}`);
  }
  return value as number;
};

const requireString = (value: unknown, path: string): string => {
  if (typeof value !== 'string' || value.length === 0) throw new Error(`${path} must be a non-empty string`);
  return value;
};

const requireEnum = <T extends string>(value: unknown, values: readonly T[], path: string): T => {
  if (typeof value !== 'string' || !values.includes(value as T)) {
    throw new Error(`${path} must be one of ${values.join(', ')}`);
  }
  return value as T;
};

const requireDecimal = (value: unknown, path: string, allowZero = false): WooV3Decimal => {
  if (typeof value !== 'string' || !DECIMAL_PATTERN.test(value)) {
    throw new Error(`${path} must be a finite non-negative decimal string`);
  }
  if (!allowZero && !/[1-9]/.test(value)) throw new Error(`${path} must be greater than zero`);
  return value;
};

const decimalParts = (value: WooV3Decimal): readonly [string, string] => {
  const [rawWhole = '0', rawFraction = ''] = value.split('.');
  return [rawWhole.replace(/^0+(?=\d)/, '') || '0', rawFraction.replace(/0+$/, '')];
};

const compareDecimals = (left: WooV3Decimal, right: WooV3Decimal): number => {
  const [leftWhole, leftFraction] = decimalParts(left);
  const [rightWhole, rightFraction] = decimalParts(right);
  if (leftWhole.length !== rightWhole.length) return leftWhole.length > rightWhole.length ? 1 : -1;
  if (leftWhole !== rightWhole) return leftWhole > rightWhole ? 1 : -1;
  const width = Math.max(leftFraction.length, rightFraction.length);
  const paddedLeft = leftFraction.padEnd(width, '0');
  const paddedRight = rightFraction.padEnd(width, '0');
  return paddedLeft === paddedRight ? 0 : paddedLeft > paddedRight ? 1 : -1;
};

const requirePerpetualSymbol = (symbol: string): string => {
  if (!/^PERP_[A-Z0-9]+_[A-Z0-9]+$/.test(symbol)) throw new Error('WOO V3 symbol must use PERP_BASE_QUOTE form');
  return symbol;
};

const validateTarget = (target: WooV3AlgoWriteTarget): WooV3AlgoWriteTarget => {
  const record = target as UnknownRecord;
  const hasAlgoOrderId = record.algoOrderId !== undefined;
  const hasClientAlgoOrderId = record.clientAlgoOrderId !== undefined;
  if (hasAlgoOrderId === hasClientAlgoOrderId) throw new Error('WOO V3 algo target requires exactly one ID');
  if (hasAlgoOrderId) return Object.freeze({ algoOrderId: requireInteger(record.algoOrderId, 'WOO V3 algoOrderId', 1) });
  return Object.freeze({
    clientAlgoOrderId: requireInteger(record.clientAlgoOrderId, 'WOO V3 clientAlgoOrderId', 1),
  });
};

const validateCommon = (input: WooV3AlgoCommonInput) => {
  const symbol = requirePerpetualSymbol(input.symbol);
  const side = requireEnum(input.side, ['BUY', 'SELL'] as const, 'WOO V3 algo side');
  const positionMode = requireEnum(
    input.positionMode,
    ['ONE_WAY', 'HEDGE_MODE'] as const,
    'WOO V3 algo positionMode',
  );
  if (input.clientAlgoOrderId !== undefined) {
    requireInteger(input.clientAlgoOrderId, 'WOO V3 clientAlgoOrderId', 1);
  }
  if (input.algoOrderTag !== undefined && (input.algoOrderTag.length === 0 || input.algoOrderTag.length > 64)) {
    throw new Error('WOO V3 algoOrderTag must contain 1 to 64 characters');
  }
  const positionSide =
    positionMode === 'ONE_WAY' ? 'BOTH' : side === 'BUY' ? ('SHORT' as const) : ('LONG' as const);
  return Object.freeze({
    symbol,
    side,
    positionSide,
    ...(input.clientAlgoOrderId === undefined ? {} : { clientAlgoOrderId: input.clientAlgoOrderId }),
    ...(input.algoOrderTag === undefined ? {} : { algoOrderTag: input.algoOrderTag }),
  });
};

const hasUnsupportedTrigger = (triggerPriceType: WooV3AlgoTriggerPriceType): boolean =>
  triggerPriceType !== 'MARKET_PRICE';

export const buildWooV3AlgoPlaceRequest = (input: WooV3AlgoPlaceInput): WooV3AlgoPlaceBuildResult => {
  const common = validateCommon(input);
  if (input.family === 'STOP_MARKET') {
    if (hasUnsupportedTrigger(input.triggerPriceType)) {
      return Object.freeze({ ready: false, blocker: 'only-market-price-trigger-is-documented', family: input.family });
    }
    const body = Object.freeze({
      ...common,
      algoType: 'STOP',
      quantity: requireDecimal(input.quantity, 'WOO V3 stop quantity'),
      marginMode: 'CROSS',
      triggerPrice: requireDecimal(input.triggerPrice, 'WOO V3 stop triggerPrice'),
      triggerPriceType: 'MARKET_PRICE',
      type: 'MARKET',
      reduceOnly: true,
    });
    return Object.freeze({
      ready: true,
      request: Object.freeze({ method: 'POST', path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER, body }),
      family: input.family,
      triggerSemantics: 'official-last-price-trigger',
    });
  }

  if (input.family === 'TRAILING_STOP') {
    const callback = requireRecord(input.callback, 'WOO V3 trailing callback');
    requireEnum(
      callback.source,
      ['WOO_NATIVE_CALLBACK_VALUE'] as const,
      'WOO V3 trailing callback source',
    );
    const body = Object.freeze({
      ...common,
      algoType: 'TRAILING_STOP',
      quantity: requireDecimal(input.quantity, 'WOO V3 trailing quantity'),
      marginMode: 'CROSS',
      type: 'MARKET',
      callbackValue: requireDecimal(callback.value, 'WOO V3 trailing callbackValue'),
      ...(input.activatedPrice === undefined
        ? {}
        : { activatedPrice: requireDecimal(input.activatedPrice, 'WOO V3 trailing activatedPrice') }),
      reduceOnly: true,
    });
    return Object.freeze({
      ready: true,
      request: Object.freeze({ method: 'POST', path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER, body }),
      family: input.family,
      triggerSemantics: 'vendor-native-callback-value-no-safe-cex-conversion',
    });
  }

  const triggers = [
    input.takeProfit && { algoType: 'TAKE_PROFIT' as const, trigger: input.takeProfit },
    input.stopLoss && { algoType: 'STOP_LOSS' as const, trigger: input.stopLoss },
  ].filter((value): value is NonNullable<typeof value> => value !== undefined);
  if (triggers.length === 0) throw new Error('WOO V3 positional TP/SL requires at least one child');
  if (triggers.some(({ trigger }) => hasUnsupportedTrigger(trigger.triggerPriceType))) {
    return Object.freeze({ ready: false, blocker: 'only-market-price-trigger-is-documented', family: input.family });
  }
  const childOrders = Object.freeze(
    triggers.map(({ algoType, trigger }) =>
      Object.freeze({
        algoType,
        side: common.side,
        triggerPrice: requireDecimal(trigger.triggerPrice, `WOO V3 positional ${algoType} triggerPrice`),
        type: 'CLOSE_POSITION',
        triggerPriceType: 'MARKET_PRICE',
      }),
    ),
  );
  const body = Object.freeze({
    symbol: common.symbol,
    algoType: 'POSITIONAL_TP_SL',
    childOrders,
    positionSide: common.positionSide,
    marginMode: 'CROSS',
    ...(input.clientAlgoOrderId === undefined ? {} : { clientAlgoOrderId: input.clientAlgoOrderId }),
    ...(input.algoOrderTag === undefined ? {} : { algoOrderTag: input.algoOrderTag }),
  });
  return Object.freeze({
    ready: true,
    request: Object.freeze({ method: 'POST', path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER, body }),
    family: input.family,
    triggerSemantics: 'official-last-price-positional-children',
  });
};

const decodeSuccessData = (input: unknown, name: string): Readonly<{ data: UnknownRecord; timestamp: number }> => {
  const response = requireRecord(input, `WOO V3 ${name} response`);
  const success = requireBoolean(response.success, `WOO V3 ${name}.success`);
  if (!success) {
    const code = response.code === undefined ? undefined : requireInteger(response.code, `WOO V3 ${name}.code`);
    const message =
      response.message === undefined
        ? `WOO V3 ${name} failed with an undocumented error envelope`
        : requireString(response.message, `WOO V3 ${name}.message`);
    throw new WooV3AlgoWriteResponseError(message, response, code);
  }
  return Object.freeze({
    data: requireRecord(response.data, `WOO V3 ${name}.data`),
    timestamp: requireInteger(response.timestamp, `WOO V3 ${name}.timestamp`),
  });
};

const decodePlaceRow = (input: unknown, path: string): WooV3AlgoPlaceResponseRow => {
  const row = requireRecord(input, path);
  return Object.freeze({
    algoOrderId: requireInteger(row.algoOrderId, `${path}.algoOrderId`, 1),
    clientAlgoOrderId: requireInteger(row.clientAlgoOrderId, `${path}.clientAlgoOrderId`),
    algoType: requireEnum(
      row.algoType,
      ['STOP', 'TRAILING_STOP', 'POSITIONAL_TP_SL', 'TAKE_PROFIT', 'STOP_LOSS'] as const,
      `${path}.algoType`,
    ),
    quantity: requireDecimal(row.quantity, `${path}.quantity`, true),
  });
};

export const decodeWooV3AlgoPlaceResponse = (
  input: unknown,
  build: Extract<WooV3AlgoPlaceBuildResult, { ready: true }>,
  knownAlgoOrderIds: ReadonlySet<number> = new Set(),
): WooV3AlgoPlaceDecodeResult => {
  if (
    build.request.method !== 'POST' ||
    build.request.path !== WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER ||
    !build.request.body
  ) {
    throw new Error('WOO V3 algo place response requires its matching place request');
  }
  const { data, timestamp } = decodeSuccessData(input, 'place algo order');
  const rows = Object.freeze(
    requireArray(data.rows, 'WOO V3 place algo order.data.rows').map((row, index) =>
      decodePlaceRow(row, `WOO V3 place algo order.data.rows[${index}]`),
    ),
  );
  if (rows.length === 0) throw new Error('WOO V3 place algo order.data.rows must not be empty');
  const ids = new Set<number>();
  for (const row of rows) {
    if (knownAlgoOrderIds.has(row.algoOrderId)) throw new Error('WOO V3 algo place response reused a known algoOrderId');
    if (ids.has(row.algoOrderId)) throw new Error('WOO V3 algo place response repeated an algoOrderId');
    ids.add(row.algoOrderId);
  }

  if (build.family === 'POSITIONAL_TP_SL') {
    return Object.freeze({
      ready: false,
      blocker: 'positional-response-parent-child-identity-requires-demo-evidence',
      rows,
      timestamp,
    });
  }

  if (rows.length !== 1) throw new Error(`WOO V3 ${build.family} place response must contain exactly one row`);
  const row = rows[0]!;
  const expectedType = build.family === 'STOP_MARKET' ? 'STOP' : 'TRAILING_STOP';
  if (row.algoType !== expectedType) throw new Error('WOO V3 algo place response algoType does not match request');
  const expectedQuantity = build.request.body.quantity;
  if (typeof expectedQuantity !== 'string' || compareDecimals(row.quantity, expectedQuantity) !== 0) {
    throw new Error('WOO V3 algo place response quantity does not match request');
  }
  const expectedClientId = build.request.body.clientAlgoOrderId;
  if (expectedClientId !== undefined && row.clientAlgoOrderId !== expectedClientId) {
    throw new Error('WOO V3 algo place response clientAlgoOrderId does not match request');
  }
  return Object.freeze({
    ready: true,
    row,
    timestamp,
    identityEvidence: 'response-echoed-algo-order-id',
  });
};

const targetMatchesOrder = (target: WooV3AlgoWriteTarget, order: WooV3AlgoOrder): boolean =>
  ('algoOrderId' in target && target.algoOrderId === order.algoOrderId) ||
  ('clientAlgoOrderId' in target && target.clientAlgoOrderId === order.clientAlgoOrderId);

export const buildWooV3AlgoAmendRequest = (input: WooV3AlgoAmendInput): WooV3AlgoAmendBuildResult => {
  const target = validateTarget(input.target);
  if (!targetMatchesOrder(target, input.currentOrder)) {
    return Object.freeze({ ready: false, blocker: 'target-id-mismatch', target });
  }
  if (compareDecimals(input.currentOrder.totalExecutedQuantity, '0') !== 0) {
    return Object.freeze({
      ready: false,
      blocker: 'partial-fill-algo-amend-semantics-require-demo-evidence',
      target,
    });
  }
  if (
    input.currentOrder.algoStatus !== 'NEW' ||
    input.currentOrder.rootAlgoStatus !== 'NEW' ||
    input.currentOrder.isTriggered ||
    input.currentOrder.triggerStatus !== 'USELESS'
  ) {
    return Object.freeze({ ready: false, blocker: 'terminal-or-triggered-algo-cannot-be-amended', target });
  }
  if (input.update.quantity !== undefined) {
    requireDecimal(input.update.quantity, 'WOO V3 algo amend quantity');
    return Object.freeze({ ready: false, blocker: 'quantity-amend-semantics-require-demo-evidence', target });
  }
  if (
    input.currentOrder.algoType === 'POSITIONAL_TP_SL' ||
    input.currentOrder.algoType === 'TP_SL' ||
    input.currentOrder.algoType === 'TAKE_PROFIT' ||
    input.currentOrder.algoType === 'STOP_LOSS'
  ) {
    return Object.freeze({ ready: false, blocker: 'positional-child-update-not-documented', target });
  }
  if (input.currentOrder.algoType === 'TRAILING_STOP' || input.update.callbackValue !== undefined) {
    if (input.update.callbackValue !== undefined) {
      requireDecimal(input.update.callbackValue, 'WOO V3 algo amend callbackValue');
    }
    return Object.freeze({ ready: false, blocker: 'trailing-callback-update-not-documented', target });
  }
  if (input.currentOrder.algoType !== 'STOP') {
    return Object.freeze({ ready: false, blocker: 'algo-family-update-not-supported', target });
  }
  if (input.currentOrder.triggerPriceType !== 'MARKET_PRICE') {
    return Object.freeze({ ready: false, blocker: 'only-market-price-trigger-is-documented', target });
  }
  if (input.currentOrder.triggerPrice === undefined) {
    return Object.freeze({ ready: false, blocker: 'stop-trigger-provenance-incomplete', target });
  }
  if (input.update.triggerPriceType !== undefined && hasUnsupportedTrigger(input.update.triggerPriceType)) {
    return Object.freeze({ ready: false, blocker: 'only-market-price-trigger-is-documented', target });
  }
  if (input.update.triggerPrice === undefined) throw new Error('WOO V3 STOP amend requires triggerPrice');
  const triggerPrice = requireDecimal(input.update.triggerPrice, 'WOO V3 algo amend triggerPrice');
  if (compareDecimals(triggerPrice, input.currentOrder.triggerPrice) === 0) {
    throw new Error('WOO V3 algo amend must change triggerPrice');
  }
  const body = Object.freeze({
    ...target,
    triggerPrice,
    ...(input.update.triggerPriceType === undefined ? {} : { triggerPriceType: 'MARKET_PRICE' }),
  });
  return Object.freeze({
    ready: true,
    request: Object.freeze({ method: 'PUT', path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER, body }),
    target,
    updateSemantics: 'untriggered-zero-fill-stop-trigger-only',
  });
};

export const buildWooV3AlgoCancelRequest = (targetInput: WooV3AlgoWriteTarget): WooV3RequestInput =>
  Object.freeze({
    method: 'DELETE',
    path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDER,
    query: Object.freeze(validateTarget(targetInput)),
  });

export const buildWooV3AlgoCancelPendingRequest = (
  scope: 'all' | Readonly<{ symbol: string }> = 'all',
): WooV3AlgoCancelPendingBuildResult => {
  if (scope !== 'all') {
    return Object.freeze({
      ready: false,
      blocker: 'symbol-scoped-pending-cancel-query-schema-conflicts-with-description',
      scope: Object.freeze({ symbol: requirePerpetualSymbol(scope.symbol) }),
    });
  }
  return Object.freeze({
    ready: true,
    request: Object.freeze({ method: 'DELETE', path: WOO_V3_ENDPOINTS.TRADE_ALGO_ORDERS }),
    scope: 'all-algo-orders-all-symbols',
  });
};

const decodeMutation = (
  input: unknown,
  name: string,
  target: WooV3AlgoWriteTarget,
): WooV3AlgoMutationResult => {
  const { data, timestamp } = decodeSuccessData(input, name);
  return Object.freeze({
    accepted: true,
    target: validateTarget(target),
    timestamp,
    identityEvidence: 'request-target-only-response-does-not-echo-id-or-status',
    gatewayData: Object.freeze({ ...data }),
  });
};

export const decodeWooV3AlgoAmendResponse = (
  input: unknown,
  target: WooV3AlgoWriteTarget,
): WooV3AlgoMutationResult => decodeMutation(input, 'amend algo order', target);

export const decodeWooV3AlgoCancelResponse = (
  input: unknown,
  target: WooV3AlgoWriteTarget,
): WooV3AlgoMutationResult => decodeMutation(input, 'cancel algo order', target);

export const decodeWooV3AlgoCancelPendingResponse = (input: unknown): WooV3AlgoCancelPendingResult => {
  const { data, timestamp } = decodeSuccessData(input, 'cancel pending algo orders');
  return Object.freeze({
    accepted: true,
    scope: 'all-algo-orders-all-symbols',
    timestamp,
    gatewayData: Object.freeze({ ...data }),
  });
};
