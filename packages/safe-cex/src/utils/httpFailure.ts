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
 * Append ` (HTTP 429)` to a message the caller was already going to emit.
 *
 * Returns the message untouched when the response did not fail, so it is safe
 * to wrap an existing `emitAndLogError` argument unconditionally. Idempotent:
 * a message that already names this status is returned as-is, so a venue that
 * formats its own status does not get it twice.
 *
 * **The number goes on, the canonical reason never does.**
 * `ExchangeHealthMonitor.processMessage` classifies by matching patterns over
 * the WHOLE message, so an appended reason phrase changes how the error is
 * treated — and enumerating which phrases are safe does not work. Three review
 * rounds over one small change turned up three separate collisions:
 * `'unauthorized'`/`'forbidden'`/`'not allowed'` (401/403/405) push a working
 * key into the terminal auth family; `'server error'` (500) and `'timeout'`
 * (408/504) demote a red toast to a deduped log; and `/market.*not found/i`
 * (404) is FIRST in the list under `suppressed`, so it would drop the error
 * entirely — a regex bridges the original message and the suffix, which the
 * substring reasoning misses. The list also grows whenever someone adds a
 * pattern, with nothing to tell them this append exists.
 *
 * The bare number costs nothing: `'429'`, `'502'` and `'503'` are already
 * patterns in that file, and their categories (`rate_limit`, `exchange_down`)
 * are the right answers for a transport failure — so the number classifies
 * exactly the cases worth classifying and is inert everywhere else. The reason
 * text still reaches Sentry through `httpFailureContext`'s tags.
 */
export const appendHttpFailure = (message: unknown, response: unknown): string => {
  // Coerced: bitget and bitunix pass `v(response.data, 'msg')`, which is `any`
  // and can be a number, and a telemetry helper must not throw into a request.
  const text = typeof message === 'string' ? message : String(message ?? '');
  const failure = getHttpFailure(response);
  if (!failure) return text;
  if (text.includes(`HTTP ${failure.status}`)) return text;
  // The NUMBER only, never the canonical reason. See the note above.
  return `${text} (HTTP ${failure.status})`;
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
 * make nothing of. Like `appendHttpFailure`, it emits the status NUMBER only.
 */
export const describeHttpFailure = (label: string, response: unknown): string => {
  const failure = getHttpFailure(response);
  const status = failure ? String(failure.status) : 'unknown';
  const body = describeBody((response as { data?: unknown } | null)?.data);
  const detail = body && body !== 'null' && body !== 'undefined' ? `: ${body.slice(0, 300)}` : '';
  // No canonical reason here either — this string goes to `emitAndLogError` on
  // the same path, so it faces exactly the classification collisions described
  // on `appendHttpFailure`. The body is included instead, which is the part
  // worth reading anyway.
  return `${label} API error (HTTP ${status})${detail}`;
};
