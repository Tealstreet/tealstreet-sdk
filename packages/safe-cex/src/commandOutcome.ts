import type { BuilderAuthorizationDetail, OrderIdentityReceipt } from '@tealstreet/safe-cex-wasm/host';
import type { ErrorCategory } from './health/errorPatterns';

import { builderAuthorizationOf } from '@tealstreet/safe-cex-wasm/host';

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

export type CommandOutcome = 'notSent' | 'rejected' | 'unknown';
export type ExchangeErrorKind =
  | 'rejected'
  | 'invalidInput'
  | 'insufficientBalance'
  | 'authInvalid'
  | 'authExpired'
  | 'authPermission'
  | 'rateLimited'
  | 'network'
  | 'timeout'
  | 'venueUnavailable'
  | 'unsupported'
  | 'internal';
const OUTCOMES: readonly string[] = ['notSent', 'rejected', 'unknown'];
const KINDS: readonly string[] = [
  'rejected',
  'invalidInput',
  'insufficientBalance',
  'authInvalid',
  'authExpired',
  'authPermission',
  'rateLimited',
  'network',
  'timeout',
  'venueUnavailable',
  'unsupported',
  'internal',
];

/** Cloneable command failure details shared by SDK workers and CLI consumers. */
export type SerializedCommandError = {
  message: string;
  name?: string;
  stack?: string;
  code?: string | number;
  /** Definitive builder approval details; present only with the validated stable code. */
  reason?: BuilderAuthorizationDetail['reason'];
  requiredRaw?: number;
  authorizedRaw?: number;
  command?: string;
  timeoutMs?: number;
  /** `unknown`: the write may have executed; `notSent`/`rejected`: it did not take effect. */
  outcome?: CommandOutcome;
  /** Rust venue classification (`ExchangeError`); absent where the venue did not classify. */
  kind?: ExchangeErrorKind;
  venueCode?: string;
  venueMessage?: string;
  retryAfterMs?: number;
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
  for (const key of ['name', 'stack', 'command', 'clientOrderId', 'receiptKey', 'venueCode', 'venueMessage'] as const) {
    const field = source[key];
    if (typeof field === 'string') result[key] = field;
  }
  if (
    (typeof source.code === 'string' && source.code !== 'GRVT_BUILDER_UNAUTHORIZED') ||
    (typeof source.code === 'number' && Number.isFinite(source.code))
  ) {
    result.code = source.code;
  }
  const builder = builderAuthorizationOf(source);
  if (builder) Object.assign(result, builder);
  if (
    typeof source.timeoutMs === 'number' &&
    Number.isSafeInteger(source.timeoutMs) &&
    source.timeoutMs > 0 &&
    source.timeoutMs <= 2_147_483_647
  )
    result.timeoutMs = source.timeoutMs;
  if (typeof source.outcome === 'string' && OUTCOMES.includes(source.outcome)) {
    result.outcome = source.outcome as CommandOutcome;
  }
  if (typeof source.kind === 'string' && KINDS.includes(source.kind)) result.kind = source.kind as ExchangeErrorKind;
  if (typeof source.retryAfterMs === 'number' && Number.isFinite(source.retryAfterMs) && source.retryAfterMs >= 0) {
    result.retryAfterMs = source.retryAfterMs;
  }
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

const CATEGORIES: Record<ExchangeErrorKind, ErrorCategory> = {
  rejected: 'order_rejected',
  invalidInput: 'order_rejected',
  insufficientBalance: 'insufficient_balance',
  authInvalid: 'auth_invalid',
  authExpired: 'auth_expired',
  authPermission: 'auth_permissions',
  rateLimited: 'rate_limit',
  network: 'network',
  timeout: 'network',
  venueUnavailable: 'exchange_down',
  unsupported: 'unsupported_backend',
  internal: 'unknown',
};

/** The health category of a classified failure; undefined leaves text classification to decide. */
export const exchangeErrorCategory = (value: unknown): ErrorCategory | undefined => {
  if (value === null || typeof value !== 'object') return undefined;
  const { kind } = value as { kind?: unknown };
  return typeof kind === 'string' && KINDS.includes(kind) ? CATEGORIES[kind as ExchangeErrorKind] : undefined;
};

/** The classified fields of an engine error, for the `'error'` event payload. */
export type ClassifiedError = { kind?: ExchangeErrorKind; outcome?: CommandOutcome; venueCode?: string };

/** Only the closed-set classification of `value`; undefined when it carries none. */
export const classifiedError = (value: unknown): ClassifiedError | undefined => {
  if (value === null || typeof value !== 'object') return undefined;
  const { kind, outcome, venueCode } = value as { kind?: unknown; outcome?: unknown; venueCode?: unknown };
  const result: ClassifiedError = {};
  if (typeof kind === 'string' && KINDS.includes(kind)) result.kind = kind as ExchangeErrorKind;
  if (typeof outcome === 'string' && OUTCOMES.includes(outcome)) result.outcome = outcome as CommandOutcome;
  if (typeof venueCode === 'string' && venueCode) result.venueCode = venueCode;
  return Object.keys(result).length ? result : undefined;
};

const AUTH_KINDS: readonly string[] = ['authInvalid', 'authExpired', 'authPermission'];

/**
 * The classification an engine diagnostic carries in `fields` (engine
 * `notice_error`). An auth kind counts only on `auth.rejected`, the
 * account-level evidence; a read or order refusal never asserts one
 * (ExchangeHealthMonitor.processEvent). No kind is invented: an IP mismatch is
 * not a bad key.
 */
export const engineEventClassified = (event: {
  code: string;
  fields?: Record<string, unknown>;
}): ClassifiedError | undefined => {
  const classified = classifiedError(event.fields) ?? {};
  const accountAuth = event.code === 'auth.rejected';
  if (!accountAuth && classified.kind && AUTH_KINDS.includes(classified.kind)) delete classified.kind;
  return Object.keys(classified).length ? classified : undefined;
};

/**
 * Closed-set Sentry tags for a classified failure: kind, its health category,
 * outcome and the venue's code. Never free text, so tags stay low-cardinality.
 */
export const exchangeErrorTags = (value: unknown): Record<string, string> => {
  if (value === null || typeof value !== 'object') return {};
  const { kind, outcome, venueCode } = value as { kind?: unknown; outcome?: unknown; venueCode?: unknown };
  const tags: Record<string, string> = {};
  if (typeof kind === 'string' && KINDS.includes(kind)) {
    tags.error_kind = kind;
    tags.error_category = CATEGORIES[kind as ExchangeErrorKind];
  }
  if (typeof outcome === 'string' && OUTCOMES.includes(outcome)) tags.error_outcome = outcome;
  if (typeof venueCode === 'string' && /^[\w.-]{1,32}$/.test(venueCode)) tags.venue_code = venueCode;
  return tags;
};
