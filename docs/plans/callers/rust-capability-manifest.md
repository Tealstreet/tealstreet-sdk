# Rust capability manifest

`packages/safe-cex/src/exchanges/rust/capabilityManifest.json` records, for
every `BETA_VENUES` entry, the `ExchangeCapabilities` that
`RustEngineExchange.loadCapabilities` returns on the wasm engine. The SDK
generates it (`corepack yarn workspace @tealstreet/safe-cex capability-manifest`)
and `test-wasm` fails when it differs from the runtime.

Callers:

- **NX** (`packages/venue-data`, after this lands and the mirror imports it):
  `journal/axis-grading.registry.test.ts` reads the manifest for
  `fillHistory` instead of constructing venues through the legacy reference
  registry, which cannot reach the wasm engine in NX CI.

Deletion prerequisites: none added or cleared.
