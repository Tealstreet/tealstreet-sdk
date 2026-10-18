# Native Store facade reuse

## Ownership and behavior

The opaque native owner now contains an `Rc<Graph>` and `OnceCell<Js>`. Both
native facade entrypoints use that capability. The cached object, its native
resource, and its methods retain only `Graph`; the graph has no capability/cache
back-reference. The existing graph remains the sole material Store authority.
The balance update helper downcasts the same capability and mutates its graph.

Repeated facade access retains object/method identity and custom descriptors.
Reset still replaces the source dictionaries and rows at the existing mutation
sites. Facade construction invokes no user code; a construction error leaves the
cell empty and propagates without a fallback value.

The actual-owner regression fails on the original implementation at repeated
facade identity. Fixed controls exercise hidden, immutable UTF-16 properties,
reset generation changes, retained facade use after Store drop, and final graph
release after the last retained method drops. A weak clock-capture marker tracks
the graph lifetime without exposing its private type; a separate weak opaque
capability verifies the facade does not retain its cache owner. Independent
read-only review verified the absence of reverse ownership edges.

## Matched release builds

The control is `480f4a9063984baab10805506441e9d9e2840400`, including the
checked Binance eager snapshot repair. The cache is
`e5da2bf716eab49d0b9d7ddacf68df25b549a35c`. Their Rust production diff is
only `js_runtime/native_store/mod.rs`; dependencies and profile settings match.
Both use the pinned Rust 1.99.0 toolchain and this command from `sdk-rs`:

```sh
CARGO_INCREMENTAL=0 cargo build --release --locked -p safe-cex-ffi --features node --jobs 1
```

Cargo reported `release` / `optimized` for both. No release overrides or Rust
flags were set. The addon build script currently uses the development profile;
this PR changes no build configuration. Artifact hashes and raw timing samples
are in [the benchmark receipt](NATIVE_STORE_FACADE_BENCHMARK.json).

## Controlled replay method

The unchanged 570-frame Binance bundle has SHA-256
`df9a504272da029f0c871cb9c44bb2777ff092c34ea966a68d3c126269218d5f`.
Each report starts two maintained TS accounts and two native accounts using the
same captured REST bodies and explicit clocks. The runner checks the immutable
bundle hash before each report and the per-dial physical/application receipts.

`replayWallMs` spans first controlled frame feed through the final output/state
drain RPC. Per-worker `process.cpuUsage` deltas span the same phase and include
native addon threads; startup and shutdown are excluded. CPU time is primary
because other lanes share this 10-core machine. Wall time is reported alongside
it. No local Cargo build ran during measurement.

Control/cache reports interleaved four times. Before any timing values were
observed, pair 1 was designated warm-up because the offered peer compiler pause
was confirmed after its start. Primary statistics use pairs 2, 3 and 4, giving
six samples per implementation. All eight reports and warm-up samples are
retained in the receipt. Other lanes continued work, so these observations do
not represent an isolated machine benchmark.

The checker compares complete retained native outputs and final state, including
serialized V8 bytes, property order and reference structure. It applies no clock
removal, sorting, tolerance, JSON conversion or channel normalization.

## Release results

Values are seconds, shown as median (minimum–maximum), with six primary samples.
The TS reference is the six TS workers in the cache reports; the receipt also
retains the separate TS control-report samples.

| Implementation | Process CPU | Replay wall time |
| --- | --- | --- |
| Native control | 17.02 (15.84–17.61) | 47.25 (43.65–52.77) |
| Native cache | 9.63 (8.99–9.71) | 22.18 (19.55–26.53) |
| Maintained TS reference | 2.44 (2.34–2.59) | 6.99 (5.50–8.79) |

The cache uses 43.4% less CPU (1.77× speedup) and the observed wall-time median
improves 2.13×. **The performance target remains open:** native CPU is 3.95× TS
and native wall time is 3.17× TS, above the requested 2× limit. Further optimized
profiling is the next step toward that target.

All 16 native runs verified all 570 inputs with zero drops. Every complete native
output/state profile has the same V8 SHA-256:
`dede8d5c2e6843fce4599710c0378841b31740f43780a7b46b6ebbc85b44bfa7`.
Both native and TS emit exactly matching 238 ticker Store deliveries, 237 books
and 149 candles. The frozen base still differs from TS in request statistics,
message-handler statistics and socket health; those rows remain visible. This
is a complete native before/after equivalence proof, not a full TS parity claim.
Later master repairs for those fields are separate from this matched pair.

To check primary reports after replaying them with `streams.mjs`:

```sh
node sdk-rs/scripts/live-diff/facade-throughput.mjs \
  control-2.json cache-2.json control-3.json cache-3.json control-4.json cache-4.json
```

## Development-profile qualification

Earlier unoptimized development runs measured about 182 seconds before caching
and 90 seconds after caching. Those values are diagnostic only. They are not
combined with the release statistics or used for the release target.
