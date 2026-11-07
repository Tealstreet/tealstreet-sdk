export const EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE = 'Hyperliquid API returned an empty order response body';

type UnknownRecord = Record<string, unknown>;

const HYPERLIQUID_DEFERRED_ORDER_STATUSES = new Set(['waitingForFill', 'waitingForTrigger']);
const HYPERLIQUID_TERMINAL_CANCEL_ERROR_PATTERN =
  /Order was never placed, already canceled, or filled\.?(?:\s*asset=\d+)?/gi;

export type ParsedHyperliquidOrderResponse = {
  orderIds: string[];
  waitingForTrigger: boolean;
};

/**
 * Thrown when an `/exchange` response cannot be read as any documented order
 * result — an empty body, a missing `statuses` vector, or a status entry we do
 * not recognise. Distinct from a venue rejection: the order may still have
 * landed, so callers should reconcile from REST rather than assume it did not.
 */
export class HyperliquidUnreadableOrderResponseError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'HyperliquidUnreadableOrderResponseError';
  }
}

const isRecord = (value: unknown): value is UnknownRecord =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const getRecord = (value: unknown): UnknownRecord | undefined => (isRecord(value) ? value : undefined);

const getNonEmptyString = (value: unknown): string | undefined => {
  if (value === undefined || value === null) return undefined;
  const stringValue = typeof value === 'string' ? value : JSON.stringify(value);
  return stringValue ? stringValue : undefined;
};

const describeResponse = (value: unknown): string => {
  if (value === undefined) return 'undefined';
  if (value === null) return 'null';
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
};

export const isOnlyHyperliquidTerminalCancelError = (message: unknown): boolean => {
  if (typeof message !== 'string') return false;

  const terminalCancelErrors = message.match(HYPERLIQUID_TERMINAL_CANCEL_ERROR_PATTERN);
  if (!terminalCancelErrors?.length) return false;

  const withoutTerminalCancelErrors = message
    .replace(HYPERLIQUID_TERMINAL_CANCEL_ERROR_PATTERN, '')
    .replace(/[,\s.]+/g, '');

  return withoutTerminalCancelErrors.length === 0;
};

const getResponseData = (data: unknown): UnknownRecord | undefined => {
  const root = getRecord(data);
  const response = getRecord(root?.response);
  return getRecord(response?.data);
};

const getStatuses = (data: unknown): unknown[] | undefined => {
  const responseData = getResponseData(data);
  if (Array.isArray(responseData?.statuses)) return responseData.statuses;

  const root = getRecord(data);
  return Array.isArray(root?.statuses) ? root.statuses : undefined;
};

const getOid = (value: unknown): string | undefined => {
  const record = getRecord(value);
  const oid = record?.oid;
  return oid === undefined || oid === null ? undefined : String(oid);
};

const isHyperliquidDeferredOrderStatus = (value: unknown): value is 'waitingForFill' | 'waitingForTrigger' =>
  typeof value === 'string' && HYPERLIQUID_DEFERRED_ORDER_STATUSES.has(value);

export const extractHyperliquidResponseError = (data: unknown): string | undefined => {
  const root = getRecord(data);
  if (!root) return undefined;

  const topLevelError = getNonEmptyString(root.error);
  if (topLevelError) return topLevelError;

  if (root.status === 'err') {
    return getNonEmptyString(root.response) ?? 'Hyperliquid API rejected the request';
  }

  const responseData = getResponseData(data);
  if (responseData?.status === 'err') {
    return getNonEmptyString(responseData.response) ?? 'Hyperliquid API rejected the request';
  }

  const statusObject = getRecord(responseData?.status);
  const statusError = getNonEmptyString(statusObject?.error);
  if (statusError) return statusError;

  const statuses = getStatuses(data);
  if (statuses) {
    const errors: string[] = [];
    for (const status of statuses) {
      const error = getNonEmptyString(getRecord(status)?.error);
      if (error && !errors.includes(error)) errors.push(error);
    }
    if (errors.length > 0) return errors.join(', ');
  }

  return undefined;
};

export const parseHyperliquidOrderResponse = (data: unknown): ParsedHyperliquidOrderResponse => {
  if (data === undefined || data === null) {
    throw new HyperliquidUnreadableOrderResponseError(EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE);
  }

  const error = extractHyperliquidResponseError(data);
  if (error) {
    throw new Error(error);
  }

  const statuses = getStatuses(data);
  if (!statuses || statuses.length === 0) {
    throw new HyperliquidUnreadableOrderResponseError(
      `Hyperliquid order response missing statuses: ${describeResponse(data)}`,
    );
  }

  const orderIds: string[] = [];
  let waitingForTrigger = false;

  for (const status of statuses) {
    if (isHyperliquidDeferredOrderStatus(status)) {
      if (status === 'waitingForTrigger') {
        waitingForTrigger = true;
      }
      continue;
    }

    const statusRecord = getRecord(status);
    if (!statusRecord) {
      throw new HyperliquidUnreadableOrderResponseError(
        `Hyperliquid order response contained unsupported status: ${describeResponse(status)}`,
      );
    }

    const oid = getOid(statusRecord.resting) ?? getOid(statusRecord.filled);
    if (!oid) {
      throw new HyperliquidUnreadableOrderResponseError(
        `Hyperliquid order response contained unrecognized status: ${describeResponse(status)}`,
      );
    }

    orderIds.push(oid);
  }

  return { orderIds, waitingForTrigger };
};

export const hyperliquidOrderResponseHasWaitingForTrigger = (data: unknown): boolean =>
  getStatuses(data)?.includes('waitingForTrigger') ?? false;

export const hyperliquidOrderResponseHasDeferredOrderStatus = (data: unknown): boolean =>
  getStatuses(data)?.some(isHyperliquidDeferredOrderStatus) ?? false;
