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
