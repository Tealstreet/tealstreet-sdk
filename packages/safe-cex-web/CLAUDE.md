# CLAUDE.md — @tealstreet/safe-cex-web

Browser candle-cache backend for `@tealstreet/safe-cex`. Persists chart data to IndexedDB.

- **Single export**: `IndexedDBCandleCacheBackend`
- **Key file**: `src/candleCache/IndexedDBCandleCacheBackend.ts`
- **Dependency**: `@tealstreet/safe-cex`
- **Constraint**: browser-only — no Node or Redis deps; no SSR usage
- **Design**: matches the cache contract used by the React Native and Redis backends so the rest of safe-cex stays storage-agnostic
- **Used by**: `apps/web` for persistent chart history across sessions
