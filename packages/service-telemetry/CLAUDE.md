# CLAUDE.md — @tealstreet/service-telemetry

Observability plumbing shared by the Bun edge services, `services/exchange-server`
and `mobile-services/candle-server`. Source-only, no build.

| File                  | Purpose                                                                       |
| --------------------- | ----------------------------------------------------------------------------- |
| `scrub.ts`            | The one redaction pass. `apps/cli/src/sentry.ts` imports it too.              |
| `dataCollection.ts`   | Shared restrictive Sentry v11 data collection baseline.                       |
| `integrations.ts`     | Shared Sentry integration names to drop for credential and volume safety.     |
| `init.ts`             | `initServiceSentry` / `flushServiceSentry`. No DSN is a supported state.      |
| `capture.ts`          | `captureAlert` — the Sentry half of an `alert()` call, with its own throttle. |
| `alertKey.ts`         | Normalizes volatile tokens out of a throttle key.                             |

## Why this package exists

Both services ran inside Cloudflare Containers with **no retained logs at all** —
container stdout is live-tail only, and both Workers had observability off. Slack
was the only durable record either one had, which is why filtering
exchange-server's Slack flood (2026-09-08) would otherwise have deleted the
evidence rather than filed it. **Sentry is the record; Slack is the
notification.**

Projects: `exchange-server` (`4512049750933504`) and `candle-server`
(`4512049750999040`), org `tealstreet`. DSNs are Worker secrets
`EXCHANGE_SERVER_SENTRY_DSN` (on `tealstreet-connect`) and
`CANDLE_SERVER_SENTRY_DSN` (on `tealstreet-data`), passed into the container by
the `envVars` block — the container sees only what is listed there.

## The default integration set is the leak, not the events

`@sentry/bun` default-enables `Console`, `Http`, an outgoing fetch wrapper
(`NodeFetch` in v10, `Fetch` in v11), and `RequestData`. In these services that
is a credential exfiltration path:

- **`Console`** breadcrumbs every `console.error` — and `alert()` logs its own
  UNSCRUBBED detail line before deciding anything.
- **`Http` / `NodeFetch` / `Fetch`** breadcrumb outgoing request URLs, and a
  signed venue REST call carries the API key and signature **in the query
  string**.

`tracesSampleRate: 0` does **not** turn breadcrumbs off. `BunServer` goes too,
as does v11's `BunHttpServer`: they capture the incoming request, and
exchange-server's WS upgrade URL carries the user's `proxyToken` in its query
string.

Sentry v11 removed `sendDefaultPii` and made rich data collection the default.
Keep `initServiceSentry` on the explicit restrictive `dataCollection` baseline:
no inferred user info, no cookies, no request/response bodies, no GenAI content,
no database query data, no GraphQL documents/variables, no queue arguments, no
stack-frame local variables, and denylist forwarded / IP / user-identifying
header or query values.

**`OnUncaughtException` and `OnUnhandledRejection` are dropped for a different
reason — VOLUME.** Both services register their own handlers and route them
through `alert()` → `captureAlert`, which is where the throttle lives. Sentry's
defaults would send a second, unthrottled event for every rejection, and the
highest-volume condition in exchange-server (`Signing request <id> timed out`,
every few seconds per session) arrives exactly that way. Leaving them on
re-creates the flood inside Sentry and bills for it.

`beforeBreadcrumb` scrubs whatever an integration we did not drop produces.

**The filter matches by NAME, so unit tests that fabricate names prove
nothing.** `integration-names.test.ts` deliberately does not mock the SDK: it
reads `getDefaultIntegrations({})` and fails if a dropped name is no longer a
real default — which is what catches a typo or an SDK rename. Do not "restore
default integrations" for richer context.

## `captureAlert` has its own throttle, and it is not optional

It is called **before** the Slack filters, deliberately: the conditions worth
counting are exactly the ones Slack drops. But **Sentry bills events, not
issues**, so grouping does not bound cost — an unthrottled capture just moves
the flood and pays for it. One event per fingerprint per `SERVICE_SENTRY_WINDOW_MS`
(default 5 min), carrying `suppressed_since_last_event`.

The fingerprint is the alert key, scrubbed and capped at 120 chars. Both matter:
exchange-server's `alertKey` embeds up to 160 characters of raw venue error text,
so the key is as untrusted as the message, and an uncapped one mints unbounded
Sentry issues. `MAX_TRACKED_KEYS` bounds the throttle map in a process that runs
for weeks. Exchange-server may still suppress especially noisy per-session
process-handler failures before they reach `captureAlert`; keep aggregate
diagnostics such as signer `signing-degraded` minutes when dropping individual
user/session failures.

## `alertKey` — narrow in both directions

Over-collapsing is the worse failure: a bucket shared with an unrelated
condition silences a real alert for a whole window. So bare numbers are NOT
collapsed (`status code 401` and `503` are different conditions), and the id
rule needs 16+ chars with a letter AND a digit or it eats `ECONNREFUSED`,
`disconnected` and `shared-public-restart`. `describe()` never lets
`JSON.stringify` throw — a throw inside an `unhandledRejection` handler
re-enters as an uncaught exception, which is the key that pages.

## Adding a service

Declare the dep, add the DSN to the Worker's `Env` type **and** its `envVars`
block, `initServiceSentry` at every ENTRY POINT (exchange-server has two:
`index.ts` per worker and `gateway-entry.ts` for the parent), and
`flushServiceSentry` before any `process.exit`. Add the package to
`.github/workflows/deploy-connect-data-edge.yml` `paths:` or edits here will not
deploy.
