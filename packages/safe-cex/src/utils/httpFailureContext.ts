import type { HttpFailure } from './httpFailure';

import { getHttpFailure } from './httpFailure';

/**
 * The most recent non-2xx an exchange's REST clients resolved WITHOUT rejecting.
 *
 * `axiosErrorEnhancer` already tags `statusCode` on the rejected path, so this
 * exists purely for the path it cannot see: the HOW adapter never calls axios'
 * `settle()`, so a tunnelled non-2xx resolves, no axios error is ever
 * constructed, and the status is lost before any error is emitted.
 *
 * Six venues have no response interceptor at all, so there is no point where
 * the failing response and the eventual error message are both in scope. For
 * those, correlation by recency is the only link available — which is why what
 * it produces is labelled as a correlation and never as the error's own status.
 */
export type HttpFailureRecord = HttpFailure & {
  url?: string;
  at: number;
};

/**
 * Tags describing a nearby swallowed HTTP failure.
 *
 * Deliberately NOT named `statusCode`/`statusText`: those are set by
 * `formatErrorForSentry` from the error's own response and are causal. These
 * are circumstantial — the age is included so whoever reads the event in Sentry
 * can judge the correlation instead of trusting it.
 */
export type HttpFailureContext = {
  recentHttpStatus: number;
  recentHttpStatusText?: string;
  recentHttpUrl?: string;
  recentHttpAgeMs: number;
};

/**
 * How long a swallowed failure stays relevant to a later error.
 *
 * Short on purpose. Thirty-two WebSocket modules also call `emitAndLogError`,
 * and a WS error has no HTTP request behind it at all — a wide window would
 * decorate those with a status from an unrelated REST call. A URL match would
 * be the exact fix, but the emitter is handed a bare string and so has no URL
 * to match against; the record's own URL is emitted as a tag instead, so
 * whoever reads the event can see what it is being correlated with.
 *
 * The record is also consumed ONCE by design (see `emitAndLogError`), so one
 * 404 cannot decorate a burst of unrelated errors behind it.
 */
export const HTTP_FAILURE_CORRELATION_WINDOW_MS = 1_000;

/**
 * Record a response if it failed. A successful response CLEARS the record
 * rather than leaving it: the client is evidently working again, and a stale
 * failure is worse than none because it reads as current.
 */
export const recordHttpFailure = (response: unknown, now: number, url?: string): HttpFailureRecord | undefined => {
  const failure = getHttpFailure(response);
  if (!failure) return undefined;
  return { ...failure, url, at: now };
};

/**
 * Reduce a request URL to something safe to INDEX.
 *
 * Sentry tags are indexed, so two rules apply. Kraken rewrites `config.url` to
 * `path?postData` and Coinbase prefixes the private-proxy host, so a raw URL
 * would put per-request query strings — nonces, order params — into a permanent
 * index. And a tag whose value is unique per request is worthless as a tag.
 * Keep the path, drop the query, cap the length.
 */
const tagSafeUrl = (url: string | undefined): string | undefined => {
  if (!url) return undefined;
  const path = url.split('?')[0];
  return path ? path.slice(0, 80) : undefined;
};

export const readHttpFailureContext = (
  record: HttpFailureRecord | undefined,
  now: number,
): HttpFailureContext | undefined => {
  if (!record) return undefined;
  const age = now - record.at;
  if (age < 0 || age > HTTP_FAILURE_CORRELATION_WINDOW_MS) return undefined;
  return {
    recentHttpStatus: record.status,
    ...(record.statusText ? { recentHttpStatusText: record.statusText } : {}),
    ...(tagSafeUrl(record.url) ? { recentHttpUrl: tagSafeUrl(record.url) } : {}),
    // Bucketed: a per-millisecond tag value is unique per event, which is the
    // definition of a useless index entry.
    recentHttpAgeMs: Math.round(age / 100) * 100,
  };
};
