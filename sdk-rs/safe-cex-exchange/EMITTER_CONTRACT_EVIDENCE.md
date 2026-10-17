# Native emitter and error-code boundary

The maintained `BaseExchange.emitter` uses TinyEmitter without a payload
projection. `tests/core-emitter-source.cjs` executes that initializer and the
maintained `formatErrorForSentry` function. It checks original references,
argument counts, own undefined fields, UTF-16 units, once/reentry/throw order,
and eight native error-code profiles.

`tests/support/emitter_retained.rs` reaches the production facade and typed base.
Its six witnesses inspect native number bits, property presence and reference
identity before any finite JSON observations. The original emitter/code bridge
compiled and failed three of these regressions before the repair. Formatter
outputs retain own undefined keys and source property order; throwing code
getters report their cause without recursive error projection.

The constructed Aster private-frame witness is
`safe-cex-venue-asterdex/src/native_emitter_tests.rs`:
`native_private_fill_keeps_infinite_price_through_event_and_worker_post`.
It feeds actual New/Filled frames, observes Infinity in the fill event, and
checks that the required worker DATA capability receives the same object.

`safe-cex-webworker/tests/worker-data-host.cjs` executes the compiled Wasm
browser materializer. The native Error identity assertion fails with the prior
plain-object materializer and passes after the repair. The actual structured
clone retains Error identity and stack while applying the engine's ordinary
Error-name and custom-property behavior.

Typed producers call the existing native codec before any JSON serialization.
Previously JSON-shaped fixture observations remain explicit finite projections;
they do not establish an arbitrary-value ingress or host capability.
Runtime exception-report callbacks and main-thread worker RPC arguments/success
results retain their separately scoped JSON ABIs. Raw error-input widening is a
separate follow-up, and Candle widening remains held.
