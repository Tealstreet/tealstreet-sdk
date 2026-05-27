# CLAUDE.md — @tealstreet/logging

Structured logging system with Grafana integration, Sentry error reporting, and debug level control.

- **Key files**: `debugLogger.ts` (factory with throttling), `log.ts` (main function), `logAtom.ts` (Jotai atom), `useLog.ts` (React hook) — 10 source files
- **Build**: `yarn build-force` (tsup)
- **Features**: Message throttling, Sentry integration, Grafana backend logging, severity levels (Error, Warning, Info, Local)
- **Security**: Redacts sensitive fields (keys, tokens, passwords, secrets)
- **No barrel file** — import directly from individual files
- **Dependencies**: `jotai` (state management — `log.ts` uses `getDefaultStore` + `logAtom`), `lodash` (utilities), `@tealstreet/utils` (stableHash); LogSeverity enum is defined locally in `LogSeverity.ts`
