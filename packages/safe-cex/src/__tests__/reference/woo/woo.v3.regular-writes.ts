import type { WooV3Decimal } from './woo.v3.contracts';
import type { WooV3RegularOrder } from './woo.v3.private';
import type { WooV3RequestInput } from './woo.v3.request';

import { WOO_V3_ENDPOINTS } from './woo.v3.contracts';

/**
 * Inert, transport-free contracts for the namespaced V3 regular-order writes.
 * The live Woo exchange intentionally does not import this file.
 */

type UnknownRecord = Record<string, unknown>;

export type WooV3RegularTimeInForce = 'GTC' | 'IOC' | 'FOK' | 'POST_ONLY';
export type WooV3RegularWriteTarget = Readonly<{ orderId: number } | { clientOrderId: number }>;

export type WooV3RegularPlaceInput = Readonly<{
  symbol: string;
  side: 'BUY' | 'SELL';
  positionMode: 'ONE_WAY' | 'HEDGE_MODE';
  type: 'LIMIT' | 'MARKET';
  timeInForce?: WooV3RegularTimeInForce;
  quantity: WooV3Decimal;
  price?: WooV3Decimal;
  reduceOnly: boolean;
  clientOrderId?: number;
  orderTag?: string;
  visibleQuantity?: WooV3Decimal;
  postOnlyAdjusted?: boolean;
}>;

export type WooV3RegularPlaceResult = Readonly<{
  orderId: number;
  type: 'LIMIT' | 'MARKET' | 'IOC' | 'FOK' | 'POST_ONLY';
  price: WooV3Decimal;
  quantity: WooV3Decimal;
  amount: WooV3Decimal;
  clientOrderId: number;
  bidAskLevel: number;
  timestamp: number;
}>;

export type WooV3RegularAmendInput = Readonly<{
  currentOrder: WooV3RegularOrder;
  target: WooV3RegularWriteTarget;
  update: Readonly<{ price?: WooV3Decimal; totalQuantity?: WooV3Decimal }>;
}>;

export type WooV3RegularAmendBuildResult =
  | Readonly<{
      ready: true;
      request: WooV3RequestInput;
      target: WooV3RegularWriteTarget;
      quantitySemantics: 'total-equals-remaining-because-zero-filled-provenance';
    }>
  | Readonly<{
      ready: false;
      blocker:
        | 'partial-fill-amend-semantics-require-demo-evidence'
        | 'terminal-order-cannot-be-amended'
        | 'target-id-mismatch';
      target: WooV3RegularWriteTarget;
    }>;

export type WooV3RegularMutationResult = Readonly<{
  status: 'NEW' | 'CANCELLED' | 'PARTIAL_FILLED' | 'FILLED' | 'REJECTED' | 'INCOMPLETE' | 'COMPLETED';
  target: WooV3RegularWriteTarget;
  timestamp: number;
  identityEvidence: 'request-target-only-response-does-not-echo-id';
}>;

export type WooV3RegularCancelPendingResult = Readonly<{
  status: 'CANCELLED' | 'REJECTED';
  scope: Readonly<{ symbol?: string }>;
  timestamp: number;
}>;

export class WooV3WriteResponseError extends Error {
  readonly code?: number;
  readonly gatewayResponse: Readonly<UnknownRecord>;
  readonly contractStatus = 'vendor-error-envelope-undocumented' as const;

  constructor(message: string, gatewayResponse: UnknownRecord, code?: number) {
    super(message);
    this.name = 'WooV3WriteResponseError';
    this.gatewayResponse = Object.freeze({ ...gatewayResponse });
    this.code = code;
  }
}

export const WOO_V3_REGULAR_WRITE_PARITY = Object.freeze({
  requestAndResponseFixtures: true,
  runtimeImported: false,
  credentialedDemoLifecycle: false,
  partialFillAmendQuantitySemantics: 'blocked-pending-demo-evidence',
  amendResponseIdentity: 'request-target-only',
  clientOrderIdRange: 'javascript-safe-integers-only',
  crossFamilyAllOrders: 'deferred-because-it-also-cancels-algo-orders',
} as const);

const DECIMAL_PATTERN = /^(?:\d+(?:\.\d*)?|\.\d+)$/;

const requireRecord = (value: unknown, path: string): UnknownRecord => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${path} must be an object`);
  return value as UnknownRecord;
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

const validateTarget = (target: WooV3RegularWriteTarget): WooV3RegularWriteTarget => {
  const record = target as UnknownRecord;
  const hasOrderId = record.orderId !== undefined;
  const hasClientOrderId = record.clientOrderId !== undefined;
  if (hasOrderId === hasClientOrderId) throw new Error('WOO V3 write target requires exactly one ID');
  if (hasOrderId) return Object.freeze({ orderId: requireInteger(record.orderId, 'WOO V3 orderId', 1) });
  return Object.freeze({ clientOrderId: requireInteger(record.clientOrderId, 'WOO V3 clientOrderId', 1) });
};

const mapPlaceType = (input: WooV3RegularPlaceInput): WooV3RegularPlaceResult['type'] => {
  requireEnum(input.type, ['LIMIT', 'MARKET'] as const, 'WOO V3 place type');
  if (input.type === 'MARKET') {
    if (input.timeInForce !== undefined) throw new Error('WOO V3 market orders do not accept a time-in-force');
    return 'MARKET';
  }
  const timeInForce = requireEnum(
    input.timeInForce ?? 'GTC',
    ['GTC', 'IOC', 'FOK', 'POST_ONLY'] as const,
    'WOO V3 place timeInForce',
  );
  switch (timeInForce) {
    case 'GTC':
      return 'LIMIT';
    case 'IOC':
      return 'IOC';
    case 'FOK':
      return 'FOK';
    case 'POST_ONLY':
      return 'POST_ONLY';
  }
};

export const buildWooV3RegularPlaceRequest = (input: WooV3RegularPlaceInput): WooV3RequestInput => {
  const symbol = requirePerpetualSymbol(input.symbol);
  const side = requireEnum(input.side, ['BUY', 'SELL'] as const, 'WOO V3 place side');
  const quantity = requireDecimal(input.quantity, 'WOO V3 place quantity');
  const type = mapPlaceType(input);
  const price = input.price === undefined ? undefined : requireDecimal(input.price, 'WOO V3 place price');
  if (type === 'MARKET' && price !== undefined) throw new Error('WOO V3 market orders must not send price');
  if (type !== 'MARKET' && price === undefined) throw new Error(`WOO V3 ${type} orders require price`);
  if (input.positionMode !== 'ONE_WAY' && input.positionMode !== 'HEDGE_MODE') {
    throw new Error('WOO V3 positionMode must be ONE_WAY or HEDGE_MODE');
  }
  if (input.orderTag !== undefined && (input.orderTag.length === 0 || input.orderTag.length > 64)) {
    throw new Error('WOO V3 orderTag must contain 1 to 64 characters');
  }
  if (input.clientOrderId !== undefined) requireInteger(input.clientOrderId, 'WOO V3 clientOrderId');
  const visibleQuantity =
    input.visibleQuantity === undefined
      ? undefined
      : requireDecimal(input.visibleQuantity, 'WOO V3 visibleQuantity', true);
  if (visibleQuantity !== undefined && compareDecimals(visibleQuantity, quantity) > 0) {
    throw new Error('WOO V3 visibleQuantity must not exceed quantity');
  }
  if (visibleQuantity !== undefined && type === 'MARKET') {
    throw new Error('WOO V3 market orders must not send visibleQuantity');
  }
  if (input.postOnlyAdjusted !== undefined && type !== 'POST_ONLY') {
    throw new Error('WOO V3 postOnlyAdjusted is only valid for POST_ONLY');
  }

  const positionSide =
    input.positionMode === 'ONE_WAY'
      ? undefined
      : input.reduceOnly
        ? side === 'BUY'
          ? 'SHORT'
          : 'LONG'
        : side === 'BUY'
          ? 'LONG'
          : 'SHORT';
  const body = Object.freeze({
    symbol,
    side,
    ...(positionSide === undefined ? {} : { positionSide }),
    type,
    ...(input.clientOrderId === undefined ? {} : { clientOrderId: input.clientOrderId }),
    ...(input.orderTag === undefined ? {} : { orderTag: input.orderTag }),
    ...(price === undefined ? {} : { price }),
    quantity,
    reduceOnly: input.reduceOnly,
    ...(visibleQuantity === undefined ? {} : { visibleQuantity }),
    marginMode: 'CROSS',
    ...(input.postOnlyAdjusted === undefined ? {} : { postOnlyAdjusted: input.postOnlyAdjusted }),
  });
  return Object.freeze({ method: 'POST', path: WOO_V3_ENDPOINTS.TRADE_ORDER, body });
};

export const buildWooV3RegularAmendRequest = (
  input: WooV3RegularAmendInput,
): WooV3RegularAmendBuildResult => {
  const target = validateTarget(input.target);
  const order = input.currentOrder;
  if (
    ('orderId' in target && target.orderId !== order.orderId) ||
    ('clientOrderId' in target && target.clientOrderId !== order.clientOrderId)
  ) {
    return Object.freeze({ ready: false, blocker: 'target-id-mismatch', target });
  }
  if (order.status !== 'NEW' && order.status !== 'PARTIAL_FILLED') {
    return Object.freeze({ ready: false, blocker: 'terminal-order-cannot-be-amended', target });
  }
  const executed = requireDecimal(order.executed, 'WOO V3 current executed quantity', true);
  if (order.status !== 'NEW' || compareDecimals(executed, '0') !== 0) {
    return Object.freeze({
      ready: false,
      blocker: 'partial-fill-amend-semantics-require-demo-evidence',
      target,
    });
  }
  const price =
    input.update.price === undefined ? undefined : requireDecimal(input.update.price, 'WOO V3 amend price');
  const totalQuantity =
    input.update.totalQuantity === undefined
      ? undefined
      : requireDecimal(input.update.totalQuantity, 'WOO V3 amend totalQuantity');
  if (price === undefined && totalQuantity === undefined) throw new Error('WOO V3 amend requires price or totalQuantity');
  if (price !== undefined && !['LIMIT', 'IOC', 'FOK', 'POST_ONLY'].includes(order.type)) {
    throw new Error(`WOO V3 ${order.type} orders cannot amend price`);
  }
  if (
    (price === undefined || compareDecimals(price, order.price) === 0) &&
    (totalQuantity === undefined || compareDecimals(totalQuantity, order.quantity) === 0)
  ) {
    throw new Error('WOO V3 amend must change at least one field');
  }
  const body = Object.freeze({
    ...target,
    ...(price === undefined ? {} : { price }),
    ...(totalQuantity === undefined ? {} : { quantity: totalQuantity }),
  });
  return Object.freeze({
    ready: true,
    request: Object.freeze({ method: 'PUT', path: WOO_V3_ENDPOINTS.TRADE_ORDER, body }),
    target,
    quantitySemantics: 'total-equals-remaining-because-zero-filled-provenance',
  });
};

export const buildWooV3RegularCancelRequest = (
  symbolInput: string,
  targetInput: WooV3RegularWriteTarget,
): WooV3RequestInput => {
  const target = validateTarget(targetInput);
  return Object.freeze({
    method: 'DELETE',
    path: WOO_V3_ENDPOINTS.TRADE_ORDER,
    query: Object.freeze({ symbol: requirePerpetualSymbol(symbolInput), ...target }),
  });
};

export const buildWooV3RegularCancelPendingRequest = (symbolInput?: string): WooV3RequestInput =>
  Object.freeze({
    method: 'DELETE',
    path: WOO_V3_ENDPOINTS.TRADE_ORDERS,
    ...(symbolInput === undefined ? {} : { query: Object.freeze({ symbol: requirePerpetualSymbol(symbolInput) }) }),
  });

const decodeSuccessData = (input: unknown, name: string): Readonly<{ data: UnknownRecord; timestamp: number }> => {
  const response = requireRecord(input, `WOO V3 ${name} response`);
  const success = requireBoolean(response.success, `WOO V3 ${name}.success`);
  if (!success) {
    const code = response.code === undefined ? undefined : requireInteger(response.code, `WOO V3 ${name}.code`);
    const message =
      response.message === undefined
        ? `WOO V3 ${name} failed with an undocumented error envelope`
        : requireString(response.message, `WOO V3 ${name}.message`);
    throw new WooV3WriteResponseError(message, response, code);
  }
  return Object.freeze({
    data: requireRecord(response.data, `WOO V3 ${name}.data`),
    timestamp: requireInteger(response.timestamp, `WOO V3 ${name}.timestamp`),
  });
};

export const decodeWooV3RegularPlaceResponse = (
  input: unknown,
  request: WooV3RequestInput,
  knownOrderIds: ReadonlySet<number> = new Set(),
): WooV3RegularPlaceResult => {
  if (request.method !== 'POST' || request.path !== WOO_V3_ENDPOINTS.TRADE_ORDER || !request.body) {
    throw new Error('WOO V3 place response requires its matching place request');
  }
  const expected = request.body;
  const { data, timestamp } = decodeSuccessData(input, 'place order');
  const orderId = requireInteger(data.orderId, 'WOO V3 place order.data.orderId', 1);
  if (knownOrderIds.has(orderId)) throw new Error('WOO V3 place response reused a known orderId');
  const type = requireEnum(
    data.type,
    ['LIMIT', 'MARKET', 'IOC', 'FOK', 'POST_ONLY'] as const,
    'WOO V3 place order.data.type',
  );
  if (type !== expected.type) throw new Error('WOO V3 place response type does not match request');
  const quantity = requireDecimal(data.quantity, 'WOO V3 place order.data.quantity');
  if (compareDecimals(quantity, expected.quantity as string) !== 0) {
    throw new Error('WOO V3 place response quantity does not match request');
  }
  const clientOrderId = requireInteger(data.clientOrderId, 'WOO V3 place order.data.clientOrderId');
  if (expected.clientOrderId !== undefined && clientOrderId !== expected.clientOrderId) {
    throw new Error('WOO V3 place response clientOrderId does not match request');
  }
  const price = requireDecimal(data.price, 'WOO V3 place order.data.price', true);
  if (expected.price !== undefined && compareDecimals(price, expected.price as string) !== 0) {
    throw new Error('WOO V3 place response price does not match request');
  }
  return Object.freeze({
    orderId,
    type,
    price,
    quantity,
    amount: requireDecimal(data.amount, 'WOO V3 place order.data.amount', true),
    clientOrderId,
    bidAskLevel: requireInteger(data.bidAskLevel, 'WOO V3 place order.data.bidAskLevel'),
    timestamp,
  });
};

const decodeMutationResponse = (
  input: unknown,
  name: string,
  target: WooV3RegularWriteTarget,
  statuses: readonly WooV3RegularMutationResult['status'][],
): WooV3RegularMutationResult => {
  const { data, timestamp } = decodeSuccessData(input, name);
  return Object.freeze({
    status: requireEnum(data.status, statuses, `WOO V3 ${name}.data.status`),
    target: validateTarget(target),
    timestamp,
    identityEvidence: 'request-target-only-response-does-not-echo-id',
  });
};

export const decodeWooV3RegularAmendResponse = (
  input: unknown,
  target: WooV3RegularWriteTarget,
): WooV3RegularMutationResult =>
  decodeMutationResponse(
    input,
    'amend order',
    target,
    ['NEW', 'CANCELLED', 'PARTIAL_FILLED', 'FILLED', 'REJECTED', 'INCOMPLETE', 'COMPLETED'],
  );

export const decodeWooV3RegularCancelResponse = (
  input: unknown,
  target: WooV3RegularWriteTarget,
): WooV3RegularMutationResult => decodeMutationResponse(input, 'cancel order', target, ['CANCELLED', 'REJECTED']);

export const decodeWooV3RegularCancelPendingResponse = (
  input: unknown,
  symbolInput?: string,
): WooV3RegularCancelPendingResult => {
  const { data, timestamp } = decodeSuccessData(input, 'cancel pending orders');
  return Object.freeze({
    status: requireEnum(data.status, ['CANCELLED', 'REJECTED'] as const, 'WOO V3 cancel pending orders.data.status'),
    scope: Object.freeze(symbolInput === undefined ? {} : { symbol: requirePerpetualSymbol(symbolInput) }),
    timestamp,
  });
};
