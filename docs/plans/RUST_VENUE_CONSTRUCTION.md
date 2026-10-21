# Venue construction follow-ups

Landing branch: `feat/rust-rewrite`. Preserve venue behavior; one PR per phase.

1. Expose AsterDEX's existing native API-version resolver for registry validation
   and construction metadata. Send only the caller's explicit version from TS.
   Read native metadata in `canSign` and STP preflight once WASM exists. Preserve
   synchronous pre-host inference, since exchange construction precedes async
   WASM loading. Cover explicit overrides, trimmed auth precedence, flat-field
   fallback, invalid credentials, and no-key public accounts.
2. Derive public mode and signing eligibility in the registry, carrying raw
   caller public-mode intent separately from the legacy derived value. Move
   the explicit-private Paradex refusal with the same production behavior.
3. Move HyroTrader endpoints, Hyperliquid spot routing validation, and private
   non-Paradex payload-signer refusal to Rust. Keep factory routing exceptions
   for keyless remote signing and payload signers. Retain harness polling options.

Verify each phase with focused registry, venue, wasm-host and TS bridge checks;
CI owns compiled WASM, replay and timing gates. Audit each diff, publish review
findings, merge only green CI, and apply the lane's landing/revert guard.
