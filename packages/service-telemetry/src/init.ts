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
  /** Extra tags on every event, e.g. the container's region, so a rate can be split by it. */
  tags?: Record<string, string>;
};

let initialised = false;

export function isServiceSentryEnabled(): boolean {
  return initialised;
}

export function initServiceSentry({ dsn, service, release, environment, tags = {} }: ServiceSentryOptions): boolean {
  if (!dsn || initialised) return initialised;

  const exchange = service === 'exchange-server' || service === 'exchange-gateway';
  const label = (value: string | undefined): string | undefined => {
    const trimmed = value?.trim();
    return trimmed && !['undefined', 'null', 'unknown'].includes(trimmed.toLowerCase()) ? trimmed : undefined;
  };
  const configuredBox = exchange ? label(process.env.TEALSTREET_BOX) : undefined;
  const box = configuredBox && /^[a-zA-Z0-9][a-zA-Z0-9._-]{0,127}$/.test(configuredBox) ? configuredBox : undefined;
  const imageRelease = exchange ? (label(release) ?? label(process.env.TEALSTREET_RELEASE)) : release;
  const scopeTags: Record<string, string> = { ...tags, service };
  if (exchange) {
    delete scopeTags.region;
    delete scopeTags.box;
    if (box) scopeTags.box = box;
  }

  Sentry.init({
    dsn,
    // An explicit empty release stops the SDK's SENTRY_RELEASE/build inference.
    release: exchange ? (imageRelease ?? '') : imageRelease,
    serverName: exchange ? (box ?? '') : undefined,
    environment,
    initialScope: { tags: scopeTags },

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
    integrations: (defaults) => {
      const retained = defaults.filter((integration) => !DROPPED_INTEGRATIONS.has(integration.name));
      if (!exchange) return retained;
      const identity: (typeof defaults)[number] = {
        name: 'ServiceBoxIdentity',
        beforeSetup(client) {
          // Bun replaces blank serverName; clear it before any integration starts a session.
          Object.assign(client.getOptions(), { serverName: box ?? '', release: imageRelease });
          client.on('beforeSendSession', (session) => {
            if ('aggregates' in session) {
              session.attrs = { ...session.attrs, release: imageRelease };
            } else session.release = imageRelease;
          });
          client.on('beforeEnvelope', (envelope) => {
            for (let i = envelope[1].length - 1; i >= 0; i--) {
              const [header, payload] = envelope[1][i];
              // Sessions require a release; never send a caller's inferred fallback instead.
              if (!imageRelease && ['session', 'sessions'].includes(header.type)) {
                envelope[1].splice(i, 1);
                continue;
              }
              if (header.type !== 'trace_metric' && header.type !== 'log') continue;
              // Scope attributes are merged after beforeSendMetric/beforeSendLog.
              const container = payload as { items: { attributes: Record<string, unknown> }[] };
              for (const { attributes } of container.items) {
                delete attributes['server.address'];
                delete attributes['sentry.release'];
                delete attributes.region;
                delete attributes.box;
                if (box) {
                  attributes['server.address'] = { type: 'string', value: box };
                  attributes.box = { type: 'string', value: box };
                }
                if (imageRelease) attributes['sentry.release'] = { type: 'string', value: imageRelease };
              }
            }
          });
        },
      };
      return [identity, ...retained];
    },

    beforeSend(event) {
      for (const exception of event.exception?.values ?? []) {
        if (exception.value) exception.value = scrubString(exception.value);
      }
      if (event.message) event.message = scrubString(event.message);
      if (event.extra) event.extra = scrub(event.extra) as Record<string, unknown>;
      if (event.contexts) event.contexts = scrub(event.contexts) as typeof event.contexts;
      // Keep only the explicit box identity, never an SDK-inferred container hostname.
      delete event.server_name;
      if (exchange) {
        if (box) event.server_name = box;
        if (imageRelease) event.release = imageRelease;
        else delete event.release;
        event.tags = { ...event.tags };
        // CONNECT_REGION can describe legacy routing, so it is not a box-location fact.
        delete event.tags.region;
        delete event.tags.box;
        if (box) event.tags.box = box;
      }
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
