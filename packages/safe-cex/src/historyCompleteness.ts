import type { HistoryFailureDetail } from '@tealstreet/safe-cex-wasm/host';

import { historyFailureOf } from '@tealstreet/safe-cex-wasm/host';

/**
 * A positive incompleteness assertion. Absence never promises exhaustive history.
 * `reason` is the engine's closed code (`pageFull`, `cursorStalled`, ...), never venue text.
 */
export type HistoryCompleteness = {
  readonly incomplete: true;
  readonly reason?: string;
  readonly failure?: HistoryFailureDetail;
};
export type HistoricalRows<T> = T[] & {
  readonly incomplete?: true;
  readonly incompleteReason?: string;
  readonly historyFailure?: HistoryFailureDetail;
};

/** Only a short identifier passes: anything else is not one of the engine's codes. */
const reasonOf = (value: unknown): string | undefined =>
  typeof value === 'string' && /^[A-Za-z]{1,40}$/.test(value) ? value : undefined;

const isIncomplete = (value: unknown): value is HistoryCompleteness =>
  typeof value === 'object' &&
  value !== null &&
  !Array.isArray(value) &&
  Object.prototype.hasOwnProperty.call(value, 'incomplete') &&
  (value as { incomplete?: unknown }).incomplete === true;

const failureOf = (value: object, field: 'failure' | 'historyFailure', reason: string | undefined) => {
  try {
    return reason === 'laterPageFailed' ? historyFailureOf(Reflect.get(value, field)) : undefined;
  } catch {
    return undefined;
  }
};

/** Read the host's explicit array marker before mapping or serializing its rows. */
export const historyCompletenessOf = (rows: unknown): HistoryCompleteness | undefined => {
  if (
    !Array.isArray(rows) ||
    !Object.prototype.hasOwnProperty.call(rows, 'incomplete') ||
    (rows as { incomplete?: unknown }).incomplete !== true
  ) {
    return undefined;
  }
  const reason = reasonOf((rows as { incompleteReason?: unknown }).incompleteReason);
  const failure = failureOf(rows, 'historyFailure', reason);
  return reason ? { incomplete: true, reason, ...(failure && { failure }) } : { incomplete: true };
};

/** Restore only a valid assertion on its owning result, never on an unrelated reply. */
export const withHistoryCompleteness = <T>(rows: T, completeness: unknown): T => {
  if (!Array.isArray(rows) || !isIncomplete(completeness)) return rows;
  // Cloned transport arrays and mapped rows are mutable; tolerate frozen direct callers too.
  const target =
    !Object.isExtensible(rows) ||
    Object.getOwnPropertyDescriptor(rows, 'incomplete')?.configurable === false ||
    Object.getOwnPropertyDescriptor(rows, 'historyFailure')?.configurable === false
      ? rows.slice()
      : rows;
  Object.defineProperty(target, 'incomplete', {
    value: true,
    writable: true,
    configurable: true,
    enumerable: false,
  });
  const reason = reasonOf(completeness.reason);
  if (reason) {
    Object.defineProperty(target, 'incompleteReason', {
      value: reason,
      writable: true,
      configurable: true,
      enumerable: false,
    });
  }
  const failure = failureOf(completeness, 'failure', reason);
  if (failure) {
    Object.defineProperty(target, 'historyFailure', {
      value: failure,
      writable: true,
      configurable: true,
      enumerable: false,
    });
  } else {
    // A replacement assertion cannot borrow another input's failure classification.
    Reflect.deleteProperty(target, 'historyFailure');
  }
  return target as T;
};

/** Preserve a source query's assertion through SDK-owned row transformations. */
export const copyHistoryCompleteness = <T>(source: unknown, rows: T): T =>
  withHistoryCompleteness(rows, historyCompletenessOf(source));
