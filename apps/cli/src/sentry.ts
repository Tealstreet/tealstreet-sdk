import * as Sentry from '@sentry/bun';

// One scrubber for every Tealstreet service that reports to Sentry. It used to
// live here; two Bun services then needed the same one, and a second copy of a
// redaction rule is worse than none because it still looks like protection.
import { RESTRICTIVE_SENTRY_DATA_COLLECTION } from '@tealstreet/service-telemetry/src/dataCollection';
import { OUTGOING_REQUEST_BREADCRUMB_INTEGRATIONS } from '@tealstreet/service-telemetry/src/integrations';
import { scrub, scrubString } from '@tealstreet/service-telemetry/src/scrub';
import { BRAND, brandEnv } from './brand';

const DSN = 'https://dc80fdcd175badb5f3da861273c46f93@o815347.ingest.us.sentry.io/4511874153709568';

const THROTTLE_WINDOW_MS = 5 * 60 * 1000;
const MAX_EVENTS_PER_KEY_PER_WINDOW = 2;

const seen = new Map<string, { count: number; firstSeen: number }>();

function isTelemetryEnabled(): boolean {
  const optOut = brandEnv('TELEMETRY');
  if (optOut === '0' || optOut === 'false' || optOut === 'off') return false;
  // Only released binaries report. `bun run src/index.ts` leaves this undefined.
  return Boolean(process.env.TEALSTREET_CLI_VERSION) || process.env.TEALSTREET_FORCE_SENTRY === '1';
}

function shouldSend(key: string): boolean {
  const now = Date.now();
  const entry = seen.get(key);

  if (!entry || now - entry.firstSeen > THROTTLE_WINDOW_MS) {
    seen.set(key, { count: 1, firstSeen: now });
    return true;
  }

  if (entry.count >= MAX_EVENTS_PER_KEY_PER_WINDOW) return false;

  entry.count++;
  return true;
}

function eventKey(event: Sentry.ErrorEvent): string {
  const exception = event.exception?.values?.[0];
  return [exception?.type, exception?.value, event.message].filter(Boolean).join('|') || 'unknown';
}

export function initCliSentry(): void {
  if (!isTelemetryEnabled()) return;

  Sentry.init({
    dsn: DSN,
    debug: false,
    release: `${BRAND.binName}-cli@${process.env.TEALSTREET_CLI_VERSION ?? 'dev'}`,
    initialScope: { tags: { brand: BRAND.id } },

    // This runs on a user's own machine against their own trading accounts.
    // Keep the restrictive v10 baseline explicit: Sentry v11 removed
    // `sendDefaultPii` and collects richer request context by default.
    dataCollection: RESTRICTIVE_SENTRY_DATA_COLLECTION,
    serverName: undefined,
    tracesSampleRate: 0,
    integrations: (defaults) =>
      defaults.filter((integration) => !OUTGOING_REQUEST_BREADCRUMB_INTEGRATIONS.has(integration.name)),

    beforeSend(event) {
      if (!shouldSend(eventKey(event))) return null;

      for (const exception of event.exception?.values ?? []) {
        if (exception.value) exception.value = scrubString(exception.value);
      }
      if (event.message) event.message = scrubString(event.message);
      if (event.extra) event.extra = scrub(event.extra) as Record<string, unknown>;
      if (event.contexts) event.contexts = scrub(event.contexts) as typeof event.contexts;

      // The CLI's own log file paths carry the OS username.
      delete event.server_name;

      return event;
    },

    beforeBreadcrumb(breadcrumb) {
      if (breadcrumb.message) breadcrumb.message = scrubString(breadcrumb.message);
      if (breadcrumb.data) breadcrumb.data = scrub(breadcrumb.data) as Record<string, unknown>;
      return breadcrumb;
    },
  });
}

/**
 * Crash handlers call `process.exit` immediately, which would drop the event in
 * flight. Resolves instantly when telemetry is off.
 */
export async function flushCliSentry(timeoutMs = 2000): Promise<void> {
  if (!isTelemetryEnabled()) return;
  try {
    await Sentry.flush(timeoutMs);
  } catch {
    // Never block exit on telemetry.
  }
}

/** No-ops when telemetry is off, so callers don't need their own guard. */
export function captureCliError(error: unknown, context?: Record<string, unknown>): void {
  if (!isTelemetryEnabled()) return;
  try {
    Sentry.captureException(error, context ? { extra: scrub(context) as Record<string, unknown> } : undefined);
  } catch {
    // Telemetry must never break the CLI.
  }
}
