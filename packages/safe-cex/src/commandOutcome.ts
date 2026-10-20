import type { OrderIdentityReceipt } from '@tealstreet/safe-cex-wasm/host';

// Frozen errors must retain their identity and command outcome. Weak membership
// carries only reporting state and cannot keep completed operations alive.
const reportedErrors = new WeakSet<object>();

export const markErrorReported = (value: unknown): void => {
  if (value === null || typeof value !== 'object') return;
  reportedErrors.add(value);
  try {
    Object.defineProperty(value, 'errorReported', {
      value: true,
      configurable: true,
      writable: true,
      enumerable: true,
    });
  } catch {
    /* Frozen/native errors retain reporting state through the WeakSet. */
  }
};

export const isErrorReported = (value: unknown): boolean => {
  if (value === null || typeof value !== 'object') return false;
  if (reportedErrors.has(value)) return true;
  try {
    return (value as { errorReported?: unknown }).errorReported === true;
  } catch {
    return false;
  }
};

/** Cloneable command failure details shared by SDK workers and CLI consumers. */
export type SerializedCommandError = {
  message: string;
  name?: string;
  stack?: string;
  code?: string | number;
  command?: string;
  timeoutMs?: number;
  outcome?: 'unknown';
  clientOrderId?: string;
  clientOrderIds?: string[];
  /** Correlates this immutable snapshot with later native receipt events. */
  receiptKey?: string;
  commandId?: number;
  orderIdentities?: OrderIdentityReceipt[];
  /** Actual native client IDs; caller requests and other namespaces stay separate. */
  actualClientOrderIds?: string[];
  partialOrderIds?: string[];
  partialOrderCount?: number;
  totalOrderCount?: number;
  failedOrderIndex?: number;
  errorReported?: boolean;
};

/** Preserve known command attribution without copying arbitrary error properties. */
export const serializeCommandError = (value: unknown): SerializedCommandError => {
  const source = (value !== null && typeof value === 'object' ? value : {}) as Record<string, unknown>;
  const result: SerializedCommandError = {
    message: typeof source.message === 'string' ? source.message : String(value),
  };
  const errorReported = isErrorReported(value) ? true : source.errorReported;
  if (typeof errorReported === 'boolean') result.errorReported = errorReported;
  for (const key of ['name', 'stack', 'command', 'clientOrderId', 'receiptKey'] as const) {
    const field = source[key];
    if (typeof field === 'string') result[key] = field;
  }
  if (typeof source.code === 'string' || (typeof source.code === 'number' && Number.isFinite(source.code))) {
    result.code = source.code;
  }
  if (
    typeof source.timeoutMs === 'number' &&
    Number.isSafeInteger(source.timeoutMs) &&
    source.timeoutMs > 0 &&
    source.timeoutMs <= 2_147_483_647
  )
    result.timeoutMs = source.timeoutMs;
  if (source.outcome === 'unknown') result.outcome = 'unknown';
  for (const key of ['clientOrderIds', 'actualClientOrderIds', 'partialOrderIds'] as const) {
    const ids = source[key];
    if (Array.isArray(ids) && ids.every((id): id is string => typeof id === 'string')) result[key] = [...ids];
  }
  if (typeof source.commandId === 'number' && Number.isSafeInteger(source.commandId) && source.commandId > 0) {
    result.commandId = source.commandId;
  }
  if (Array.isArray(source.orderIdentities) && source.orderIdentities.every(isOrderIdentityReceipt)) {
    result.orderIdentities = source.orderIdentities.map(({ symbol, kind, identity }) => ({ symbol, kind, identity }));
  }
  for (const key of ['partialOrderCount', 'totalOrderCount', 'failedOrderIndex'] as const) {
    const count = source[key];
    if (typeof count === 'number' && Number.isSafeInteger(count) && count >= 0) result[key] = count;
  }
  return result;
};

/** Restore the same allowlisted fields after a worker's structured-clone boundary. */
export const deserializeCommandError = (value: unknown): Error & SerializedCommandError => {
  const details = serializeCommandError(value);
  return Object.assign(new Error(details.message), details);
};

const isOrderIdentityReceipt = (value: unknown): value is OrderIdentityReceipt => {
  if (value === null || typeof value !== 'object') return false;
  const row = value as Record<string, unknown>;
  return (
    typeof row.symbol === 'string' &&
    typeof row.identity === 'string' &&
    ['clientOrderId', 'clientOrderIndex', 'orderCode'].includes(row.kind as string)
  );
};
