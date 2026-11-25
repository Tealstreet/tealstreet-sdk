# CLAUDE.md — @tealstreet/logging

Structured logging system with Grafana integration, Sentry error reporting, and debug level control.

- **Key files**: `debugLogger.ts` (factory with throttling), `log.ts` (main function), `logAtom.ts` (Jotai atom), `useLog.ts` (React hook) — 10 source files
- **Build**: `yarn build-force` (tsup)
- **Features**: Message throttling, Sentry integration, Grafana backend logging, severity levels (Error, Warning, Info, Local)
- **Security**: Redacts sensitive fields (keys, tokens, passwords, secrets)
- **No barrel file** — import directly from individual files
- **Dependencies**: `jotai` (state management — `log.ts` uses `getDefaultStore` + `logAtom`), `lodash` (utilities), `@tealstreet/utils` (stableHash); LogSeverity enum is defined locally in `LogSeverity.ts`

`startLoggingToGrafana(userId, username?)` accepts only the already-selected public
chat handle, never an Auth0 name/email or an account name. Each queued log/notice
captures it as JSON-escaped `username=` text beside the unchanged user ID; it is
not a Loki label, Sentry tag or issue title. The owning cleanup clears identity
context on logout, while stale cleanup cannot clear a newer user's context.

Ingest tokens, pending token requests, queued entries and callbacks are bound to
the full tenant-qualified user ID and lifecycle generation. Logout or an owner
change discards queued work and invalidates deferred uploads, even with a cached
token; there is no final cleanup upload. Restarting the same active owner retains
its generation and queue, while logout followed by login creates a new generation.

Retry exhaustion increments `droppedBatches` and adds a bounded warning to the
existing local `logAtom`. It includes only entry count, retry limit and HTTP status,
never batch contents or raw errors, and bypasses upload logging to avoid recursion.
For a 502, the optional ingest header adds only a validated 400–599 upstream status;
malformed header text is ignored.
**A refusal is dropped without retrying** (AGT-67): a 4xx from the ingest Worker
other than 401/408/429, or a 502 whose upstream status is a 4xx other than 429.
Resending those five times only multiplied the load on a sink that refused them.
**A 429 (the Worker's per-subject limit, or Loki's) pauses that owner's uploads**
for `RATE_LIMIT_COOLDOWN_MS` and keeps the batch pending (the pending cap bounds
memory); a self-rescheduling timeout resumes the drain, and an owner change clears
the pause. Dropping and moving on would send more into the window, not less. A 401
clears the owner's cached ingest token and retries with a fresh one; 408, 5xx and
transport failures keep the bounded backoff.
The same owner-generation checks fence late failures after logout or replacement.
