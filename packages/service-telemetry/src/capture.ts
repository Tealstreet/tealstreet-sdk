import * as Sentry from '@sentry/bun';

import { isServiceSentryEnabled } from './init';
import { scrub, scrubString } from './scrub';

/**
 * The Sentry half of an `alert()` call.
 *
 * Sentry sees conditions Slack does not — the per-session reasons
 * exchange-server drops, and everything the 15-minute Slack throttle swallows —
 * so "is this getting worse" becomes answerable without anything being posted
 * to a channel.
 */

/**
 * Sentry bills EVENTS, not issues, so grouping does not bound cost.
 *
 * The conditions being recorded here are exactly the ones that flooded Slack —
 * a signing timeout can recur every few seconds per session — so an unthrottled
 * capture just moves the flood and pays for it. One event per condition per
 * window, carrying how many were suppressed since the last one, keeps the trend
 * readable at a bounded rate.
 */
const DEFAULT_WINDOW_MS = 5 * 60 * 1000;

const parsedWindow = Number(process.env.SERVICE_SENTRY_WINDOW_MS);
const WINDOW_MS = Number.isFinite(parsedWindow) && parsedWindow > 0 ? parsedWindow : DEFAULT_WINDOW_MS;

/**
 * A long-lived process must not accumulate a map entry per distinct error
 * string forever. Above this, entries older than one window are dropped; a
 * bucket that has not fired in a window has nothing left to say anyway.
 */
const MAX_TRACKED_KEYS = 500;

type Bucket = { lastSentAt: number; suppressed: number };
const buckets = new Map<string, Bucket>();

function prune(now: number): void {
  if (buckets.size <= MAX_TRACKED_KEYS) return;

  // Expired first: a bucket that has not fired in a window has nothing left to
  // report anyway.
  for (const [key, bucket] of buckets) {
    if (now - bucket.lastSentAt > WINDOW_MS) buckets.delete(key);
  }

  // Still over means every key is live, so expiry alone is not a bound. Drop
  // the oldest until it is — losing a suppressed count is strictly better than
  // growing without limit in a process that runs for weeks.
  if (buckets.size <= MAX_TRACKED_KEYS) return;
  const oldestFirst = [...buckets.entries()].sort((a, b) => a[1].lastSentAt - b[1].lastSentAt);
  for (const [key] of oldestFirst.slice(0, buckets.size - MAX_TRACKED_KEYS)) {
    buckets.delete(key);
  }
}

function claim(key: string): { send: boolean; suppressed: number } {
  const now = Date.now();
  const bucket = buckets.get(key);

  if (bucket && now - bucket.lastSentAt < WINDOW_MS) {
    bucket.suppressed += 1;
    return { send: false, suppressed: 0 };
  }

  const suppressed = bucket?.suppressed ?? 0;
  buckets.set(key, { lastSentAt: now, suppressed: 0 });
  prune(now);
  return { send: true, suppressed };
}

export type CaptureAlertInput = {
  /** The alert's throttle key. Also the Sentry fingerprint. */
  key: string;
  message: string;
  detail?: Record<string, unknown>;
  level: 'warn' | 'error';
  /** Whether this one also paged. Lets Sentry filter to what woke somebody. */
  severity: 'low' | 'critical';
  /** True when the alert path deliberately kept this out of Slack. */
  slackSuppressed?: boolean;
  /**
   * The Error itself, where the caller has one. Without it the event carries a
   * message and nothing else, which is how 37 `unhandled rejection` events
   * arrived over two days naming a signing timeout but never the code that
   * failed to catch it. Grouping does NOT change — `fingerprint` is still the
   * alert key — so existing issues keep receiving these.
   */
  error?: unknown;
};

/**
 * A copy carrying a scrubbed message and stack.
 *
 * Sentry sends `error.message` and every frame verbatim, and these services'
 * errors quote venue responses — the same reason `scrubString` exists for the
 * message. Copied rather than mutated: the original is still on its way to
 * whatever else handles it.
 */
function sanitizedError(err: Error): Error {
  const copy = new Error(scrubString(err.message));
  copy.name = err.name;
  copy.stack = err.stack ? scrubString(err.stack) : undefined;
  return copy;
}

/**
 * The fingerprint is scrubbed and length-capped like everything else.
 *
 * exchange-server's `alertKey` embeds up to 160 characters of raw venue error
 * text, so the key is as untrusted as the message — and an unbounded one would
 * also mint an unbounded number of Sentry issues.
 */
function fingerprintFor(key: string): string {
  return scrubString(key).slice(0, 120);
}

export function captureAlert({
  key,
  message,
  detail = {},
  level,
  severity,
  slackSuppressed = false,
  error,
}: CaptureAlertInput): void {
  if (!isServiceSentryEnabled()) return;

  try {
    const fingerprint = fingerprintFor(key);
    const { send, suppressed } = claim(fingerprint);
    if (!send) return;

    const sentryLevel = level === 'error' ? 'error' : 'warning';
    const tags = { severity, slack_suppressed: String(slackSuppressed) };
    const extra = {
      ...(scrub(detail) as Record<string, unknown>),
      alert_key: fingerprint,
      ...(suppressed > 0 ? { suppressed_since_last_event: suppressed } : {}),
      ...(error instanceof Error ? { alert_message: scrubString(message) } : {}),
    };

    // An Error goes as an EXCEPTION so the stack survives; the message keeps
    // its place in `extra`. Anything else has no stack to carry, so the message
    // is all there is.
    if (error instanceof Error) {
      Sentry.captureException(sanitizedError(error), { level: sentryLevel, fingerprint: [fingerprint], tags, extra });
      return;
    }

    Sentry.captureEvent({
      message: scrubString(message),
      level: sentryLevel,
      fingerprint: [fingerprint],
      tags,
      extra,
    });
  } catch {
    // Telemetry must never be able to fail an alert, or the service.
  }
}

/**
 * A periodic measurement, not an alert: info level, never throttled (the caller
 * owns the cadence) and never routed to Slack. Counts go in `extra` so they can
 * be summed across containers and compared release to release.
 */
export function captureServiceInfo({
  key,
  message,
  detail = {},
}: {
  key: string;
  message: string;
  detail?: Record<string, unknown>;
}): void {
  if (!isServiceSentryEnabled()) return;
  try {
    const fingerprint = fingerprintFor(key);
    Sentry.captureEvent({
      message: scrubString(message),
      level: 'info',
      fingerprint: [fingerprint],
      extra: { ...(scrub(detail) as Record<string, unknown>), alert_key: fingerprint },
    });
  } catch {
    // Telemetry must never be able to fail the service.
  }
}

/**
 * Window totals as Sentry metrics, which — unlike event `extra` — Sentry can sum
 * and split by `release` and attributes. Counts are window totals; gauges are
 * per-window summaries (percentiles), never per-sample, to bound volume.
 */
export function recordServiceMetrics({
  prefix,
  counts,
  gauges,
  attributes,
}: {
  prefix: string;
  counts: Record<string, number>;
  gauges: Record<string, number>;
  attributes: Record<string, string>;
}): void {
  if (!isServiceSentryEnabled()) return;
  try {
    for (const [name, value] of Object.entries(counts)) {
      if (value > 0) Sentry.metrics.count(`${prefix}.${name}`, value, { attributes });
    }
    for (const [name, value] of Object.entries(gauges)) Sentry.metrics.gauge(`${prefix}.${name}`, value, { attributes });
  } catch {
    // Telemetry must never be able to fail the service.
  }
}

/** Test seam — the throttle map is module-level. */
export function resetCaptureAlertForTests(): void {
  buckets.clear();
}
