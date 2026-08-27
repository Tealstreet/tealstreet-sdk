/**
 * Shared reader for the HTTP status of a response that axios never rejected.
 *
 * The HTTP-over-WebSocket adapter resolves every tunnel frame without calling
 * axios' `settle()`, so a non-2xx arrives at a response interceptor's SUCCESS
 * handler with its status intact but never inspected. Every venue then builds
 * its error message out of the BODY, and a body that isn't the venue's usual
 * JSON envelope — an edge error page, plain text, an empty string — produces a
 * message that names nothing you can act on. Bybit's was `Unknown Exchange
 * Error`; Hyperliquid's was `Invalid response from Hyperliquid API: null`.
 *
 * These helpers only READ. Nothing here throws or rejects: converting a
 * swallowed non-2xx into a rejection changes control flow at 24 call sites at
 * once, and doing it on one venue alone already cost four regressions
 * (toasts on routine probe 4xx, a chart datafeed rejection, and a paged
 * collector discarding windows it had already gathered).
 */

type HttpFailureShape = {
  status?: unknown;
  statusText?: unknown;
  /** The tunnel frame is snake_case — the Rust `ResponseData` has no `rename_all`. */
  status_text?: unknown;
};

export type HttpFailure = {
  status: number;
  /**
   * Empty when the venue sent a non-standard code: the tunnel fills this from
   * `status.canonical_reason().unwrap_or("")`, so the key is always present and
   * an empty string means "no reason", not "not a tunnel response".
   */
  statusText: string;
};

const readStatusText = (response: HttpFailureShape): string => {
  const candidates = [response.statusText, response.status_text];
  for (const candidate of candidates) {
    if (typeof candidate === 'string' && candidate) return candidate;
  }
  return '';
};

/**
 * The status of a response that failed, or `undefined` when it did not.
 *
 * `undefined` for anything under 400 and for a response carrying no numeric
 * status at all, so a caller can treat a truthy result as "this really failed".
 */
export const getHttpFailure = (response: unknown): HttpFailure | undefined => {
  if (typeof response !== 'object' || response === null || Array.isArray(response)) return undefined;
  const shape = response as HttpFailureShape;
  if (typeof shape.status !== 'number' || shape.status < 400) return undefined;
  return { status: shape.status, statusText: readStatusText(shape) };
};

/**
 * Append ` (HTTP 429 Too Many Requests)` to a message the caller was already
 * going to emit.
 *
 * Returns the message untouched when the response did not fail, so it is safe
 * to wrap an existing `emitAndLogError` argument unconditionally. Idempotent:
 * a message that already names this status is returned as-is, so a venue that
 * formats its own status does not get it twice.
 */
export const appendHttpFailure = (message: string, response: unknown): string => {
  const failure = getHttpFailure(response);
  if (!failure) return message;
  if (message.includes(`HTTP ${failure.status}`)) return message;
  const reason = failure.statusText ? ` ${failure.statusText}` : '';
  return `${message} (HTTP ${failure.status}${reason})`;
};

const describeBody = (value: unknown): string => {
  if (value === undefined) return 'undefined';
  if (value === null) return 'null';
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
};

/**
 * A standalone message for a failure with no venue error text to attach to.
 *
 * `label` names the venue so the message survives being read out of context in
 * Sentry. The body is included when there is one worth reading — the common
 * case is an edge error page or plain text that the venue's own parser could
 * make nothing of.
 */
export const describeHttpFailure = (label: string, response: unknown): string => {
  const failure = getHttpFailure(response);
  const status = failure ? String(failure.status) : 'unknown';
  const reason = failure?.statusText ? ` ${failure.statusText}` : '';
  const body = describeBody((response as { data?: unknown } | null)?.data);
  const detail = body && body !== 'null' && body !== 'undefined' ? `: ${body.slice(0, 300)}` : '';
  return `${label} API error (HTTP ${status}${reason})${detail}`;
};
