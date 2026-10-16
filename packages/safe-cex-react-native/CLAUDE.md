# CLAUDE.md — @tealstreet/safe-cex-react-native

React Native candle-cache backend for `@tealstreet/safe-cex`. Persists chart data into AsyncStorage.

- **Single export**: `ReactNativeAsyncStorageCandleCacheBackend`
- **Key file**: `src/candleCache/ReactNativeAsyncStorageCandleCacheBackend.ts`
- **Dependency**: `@tealstreet/safe-cex`
- **Constraint**: must stay React-Native-safe — no browser globals, no Node-only APIs
- **Design**: takes an `AsyncStorageLike` interface so we don't pin a specific AsyncStorage implementation
- **Validation**: if the cache contract changes here, run the candle-cache tests in `packages/safe-cex` too
