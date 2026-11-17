# CLAUDE.md — @tealstreet/safe-cex-react-native

Mobile is DELEGATED-only. This package exports the AsyncStorage candle-cache
backend and the pure-JS `FrameStore`; it has no WebAssembly engine or loader.
The host receives SCXF frames from the delegated session and calls
`store.ingest(frame, { snapshot, seq })`. Generated `StoreData` is the only
account-state type. Hermes requires the host TextEncoder/TextDecoder codecs
used by the wire reader, without a wasm engine or local signing.

The candle backend accepts an `AsyncStorageLike` implementation, avoiding a
specific storage dependency. Keep it free of browser globals and Node APIs.
When changing the cache contract, run the scoped safe-cex candle-cache tests.
