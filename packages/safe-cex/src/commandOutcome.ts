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
  partialOrderIds?: string[];
  partialOrderCount?: number;
  totalOrderCount?: number;
  failedOrderIndex?: number;
};

/** Preserve known command attribution without copying arbitrary error properties. */
export const serializeCommandError = (value: unknown): SerializedCommandError => {
  const source = (value !== null && typeof value === 'object' ? value : {}) as Record<string, unknown>;
  const result: SerializedCommandError = {
    message: typeof source.message === 'string' ? source.message : String(value),
  };
  for (const key of ['name', 'stack', 'command', 'clientOrderId'] as const) {
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
  for (const key of ['clientOrderIds', 'partialOrderIds'] as const) {
    const ids = source[key];
    if (Array.isArray(ids) && ids.every((id): id is string => typeof id === 'string')) result[key] = [...ids];
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
