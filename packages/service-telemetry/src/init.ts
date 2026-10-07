import * as Sentry from '@sentry/bun';

import { RESTRICTIVE_SENTRY_DATA_COLLECTION } from './dataCollection';
import { SERVICE_DROPPED_INTEGRATIONS } from './integrations';
import { scrub, scrubString } from './scrub';

/**
 * Sentry for the Bun edge services (`exchange-server`, `candle-server`).
 *
 * These run inside Cloudflare Containers, whose stdout is live-tail only and
 * retained nowhere, and whose Workers have observability off. Before this,
 * Slack was the ONLY durable record either service had — which is why filtering
 * the Slack flood would otherwise have deleted the evidence rather than filed
 * it. Sentry is the record; Slack stays the notification.
 *
 * No DSN is a supported state: every function here no-ops, and the service
 * behaves exactly as it did before.
 */

/**
 * Names verified against the installed `@sentry/bun` tree, not guessed — a
 * filter that matches the wrong string still passes a unit test that fabricates
 * the names, and leaks in production.
 */
export const DROPPED_INTEGRATIONS = SERVICE_DROPPED_INTEGRATIONS;

export type ServiceSentryOptions = {
  /** Absent (unset secret, local dev) disables everything. */
  dsn: string | undefined;
  /** Tag on every event — the two services share an org, not a stream. */
  service: string;
  release?: string;
  environment?: string;
};

let initialised = false;

export function isServiceSentryEnabled(): boolean {
  return initialised;
}

export function initServiceSentry({ dsn, service, release, environment }: ServiceSentryOptions): boolean {
  if (!dsn || initialised) return initialised;

  Sentry.init({
    dsn,
    release,
    environment,
    initialScope: { tags: { service } },

    // These processes hold users' exchange credentials and act on their
    // accounts. Keep the restrictive v10 baseline explicit: Sentry v11
    // removed `sendDefaultPii` and collects richer request context by default.
    dataCollection: RESTRICTIVE_SENTRY_DATA_COLLECTION,
    tracesSampleRate: 0,

    /**
     * The default integration set is the leak, not the events.
     *
     * `Console` turns every `console.error` into a breadcrumb — and the alert
     * path logs its own unscrubbed detail line before deciding anything.
     * `Http` / `NodeFetch` / `Fetch` breadcrumb outgoing request URLs, and a signed venue
     * REST call carries the API key and signature in its QUERY STRING;
     * `RequestData` is what lifts that query string onto an event.
     * `tracesSampleRate: 0` does not turn breadcrumbs off, so dropping these is
     * what actually prevents it.
     *
     * `OnUncaughtException` / `OnUnhandledRejection` go for a different reason:
     * VOLUME. Both services register their own handlers and route them through
     * `alert()` -> `captureAlert`, which is where the throttle lives. Leaving
     * Sentry's defaults on would send a second, UNTHROTTLED event for every
     * rejection — and the highest-volume condition here (`Signing request <id>
     * timed out`, every few seconds per session) reaches Sentry exactly that
     * way. That is the flood this package exists to bound, billed per event.
     */
    integrations: (defaults) =>
      defaults.filter((integration) => !DROPPED_INTEGRATIONS.has(integration.name)),

    beforeSend(event) {
      for (const exception of event.exception?.values ?? []) {
        if (exception.value) exception.value = scrubString(exception.value);
      }
      if (event.message) event.message = scrubString(event.message);
      if (event.extra) event.extra = scrub(event.extra) as Record<string, unknown>;
      if (event.contexts) event.contexts = scrub(event.contexts) as typeof event.contexts;
      // Container hostnames are shard identifiers, not useful, and they change
      // every deploy.
      delete event.server_name;
      return event;
    },

    // Belt and braces: any breadcrumb an integration we did not drop produces.
    beforeBreadcrumb(breadcrumb) {
      if (breadcrumb.message) breadcrumb.message = scrubString(breadcrumb.message);
      if (breadcrumb.data) breadcrumb.data = scrub(breadcrumb.data) as Record<string, unknown>;
      return breadcrumb;
    },
  });

  initialised = true;
  return true;
}

/**
 * A shutdown handler calls `process.exit` immediately, which drops anything in
 * flight. Resolves instantly when Sentry is off, and never throws — telemetry
 * must not be able to hold up or fail a shutdown.
 */
export async function flushServiceSentry(timeoutMs = 2_000): Promise<void> {
  if (!initialised) return;
  try {
    await Sentry.flush(timeoutMs);
  } catch {
    // Never block exit on telemetry.
  }
}

/** Test seam — the module-level `initialised` latch is otherwise permanent. */
export function resetServiceSentryForTests(): void {
  initialised = false;
}
