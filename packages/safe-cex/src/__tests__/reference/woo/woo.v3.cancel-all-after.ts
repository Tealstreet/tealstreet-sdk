import { WOO_V3_ENDPOINTS } from './woo.v3.contracts';

/**
 * Inert, transport-free WOO X V3 dead-man-switch contracts.
 * The live Woo exchange intentionally does not import or route this file.
 */

type UnknownRecord = Record<string, unknown>;

export type WooV3CancelAllAfterRequest = Readonly<{
  method: 'POST';
  path: typeof WOO_V3_ENDPOINTS.TRADE_CANCEL_ALL_AFTER;
  body: Readonly<{ triggerAfter: number }>;
}>;

export type WooV3CancelAllAfterResponse = Readonly<{
  accepted: true;
  timestamp: number;
  expectedTriggerTime: number;
}>;

export const WOO_V3_CANCEL_ALL_AFTER_LIMITS = Object.freeze({
  minimumTriggerAfterMs: 0,
  maximumTriggerAfterMs: 900_000,
  requestsPerSecond: 10,
} as const);

export const WOO_V3_CANCEL_ALL_AFTER_PARITY = Object.freeze({
  vipOnly: true,
  cancelsOrdinaryOrders: true,
  cancelsAlgoOrders: true,
  zeroCancelsTimer: true,
  repeatedCallReplacesTimer: true,
  requestAndResponseFixtures: true,
  runtimeImported: false,
  transportOwner: false,
  credentialedDemoLifecycle: false,
} as const);

const requireRecord = (value: unknown, path: string): UnknownRecord => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${path} must be an object`);
  }
  return value as UnknownRecord;
};

const requireExactKeys = (input: UnknownRecord, expected: readonly string[], path: string): void => {
  const actual = Object.keys(input).sort();
  const sortedExpected = [...expected].sort();
  if (actual.length !== sortedExpected.length || actual.some((key, index) => key !== sortedExpected[index])) {
    throw new Error(`${path} must contain exactly ${sortedExpected.join(', ')}`);
  }
};

const requireNonnegativeSafeInteger = (value: unknown, path: string): number => {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new Error(`${path} must be a nonnegative safe integer`);
  }
  return value as number;
};

export const buildWooV3CancelAllAfterRequest = (triggerAfter: number): WooV3CancelAllAfterRequest => {
  if (
    !Number.isSafeInteger(triggerAfter) ||
    triggerAfter < WOO_V3_CANCEL_ALL_AFTER_LIMITS.minimumTriggerAfterMs ||
    triggerAfter > WOO_V3_CANCEL_ALL_AFTER_LIMITS.maximumTriggerAfterMs
  ) {
    throw new Error('WOO V3 triggerAfter must be a safe integer from 0 through 900000 milliseconds');
  }

  return Object.freeze({
    method: 'POST',
    path: WOO_V3_ENDPOINTS.TRADE_CANCEL_ALL_AFTER,
    body: Object.freeze({ triggerAfter }),
  });
};

export const decodeWooV3CancelAllAfterResponse = (input: unknown): WooV3CancelAllAfterResponse => {
  const response = requireRecord(input, 'WOO V3 cancel-all-after response');
  requireExactKeys(response, ['success', 'timestamp', 'data'], 'WOO V3 cancel-all-after response');
  if (response.success !== true) {
    throw new Error('WOO V3 cancel-all-after response.success must be true');
  }

  const data = requireRecord(response.data, 'WOO V3 cancel-all-after response.data');
  requireExactKeys(data, ['expectedTriggerTime'], 'WOO V3 cancel-all-after response.data');

  return Object.freeze({
    accepted: true,
    timestamp: requireNonnegativeSafeInteger(response.timestamp, 'WOO V3 cancel-all-after response.timestamp'),
    expectedTriggerTime: requireNonnegativeSafeInteger(
      data.expectedTriggerTime,
      'WOO V3 cancel-all-after response.data.expectedTriggerTime',
    ),
  });
};
