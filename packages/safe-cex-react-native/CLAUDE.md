# CLAUDE.md — @tealstreet/safe-cex-react-native

React Native host adapters for `@tealstreet/safe-cex`. Persists chart data into AsyncStorage and registers the host's Rust engine loader.

- **Exports**: `ReactNativeAsyncStorageCandleCacheBackend`, `setReactNativeRustEngineLoader`
- **Key file**: `src/candleCache/ReactNativeAsyncStorageCandleCacheBackend.ts`
- **Dependency**: `@tealstreet/safe-cex`
- **Constraint**: must stay React-Native-safe — no browser globals, no Node-only APIs
- **Design**: takes an `AsyncStorageLike` interface so we don't pin a specific AsyncStorage implementation
- **Validation**: if the cache contract changes here, run the candle-cache tests in `packages/safe-cex` too

Await `setReactNativeRustEngineLoader` before constructing Rust accounts.
The host supplies an already initialized module or a native bridge implementing
`WasmEngineModule`; a failed initialization can retry on a new account, while
concurrent accounts share successful module initialization. The barrel and
adapter class import without WebAssembly/TextEncoder/TextDecoder globals; the
native-registry subprocess proves the Binance futures/spot aliases select the real Rust
class before restoring those globals and loading an actual catalog. Actual
frame I/O requires the host's TextEncoder/TextDecoder implementations. Do not reinitialize
wasm-bindgen memory while accounts are alive. The real-module WASM test proves
the saved-account store seam using bundled browser bytes in Node; it does not
prove a Hermes bridge or physical-device compatibility. Hermes deployments
still need that native bridge and a device acceptance gate before TS deletion.
