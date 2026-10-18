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

## Controlled replay method

The input is the unchanged 570-frame Binance bundle
`df9a504272da029f0c871cb9c44bb2777ff092c34ea966a68d3c126269218d5f.bundle.v8`.
Each run starts two maintained TS accounts and two native accounts, with captured
REST bodies, explicit source clocks, and per-dial physical/application drain.
`replayWallMs` starts before the first controlled frame and ends when the final
output/state drain RPC returns. Startup, shutdown and report comparison are
outside this interval. Final command-barrier duration alone is not throughput.

The final artifacts use identical explicit release build flags, the pinned
toolchain, `CARGO_INCREMENTAL=0`, and one build job. Control and candidate replay
interleave A/B three times, with two TS and two native workers per report and no
local Cargo build active. Per-worker `process.cpuUsage` deltas span the same
feed-through-drain phase, including all native addon threads, and report user,
system and total milliseconds. CPU time is primary under shared machine load;
wall time is reported alongside it. Both report medians, minimum/maximum spread
and all raw samples. Startup and shutdown CPU are excluded. These are observations
under shared load, not isolated benchmark distributions.
Complete retained native outputs and final states are compared without sorting,
clock removal, tolerance, JSON conversion, or channel normalization.

## Results

Final paired measurements and full gate receipts will be recorded before READY.
The initial merged-#1111 runtime baseline (`bb07e4424f`, addon `6c522f1b...`)
measured 259.10/262.25 seconds versus TS 7.54/9.82 seconds. Both native runs
consumed and processed all frames and emitted identical 237 books, 149 candles
and 238 ticker Store deliveries. That preliminary run overlapped local build
work and is not the final paired timing control.

## Release measurement qualification

The earlier development-profile timing is not a production throughput claim.
The addon build script currently invokes Cargo without a profile flag, while the
workspace customizes only dev debuginfo. The release target is being measured
with explicit `cargo build --release --locked -p safe-cex-ffi --features node
--jobs 1`, `CARGO_INCREMENTAL=0`, and Cargo's unchanged release defaults on both
sides. No build/profile configuration is changed by this PR.

The no-local-build development pair measured 183.83/180.02 seconds before the
cache and 89.51/91.33 seconds after it. Both candidates byte-match one baseline
variant. The other baseline repeats a two-entry combined Store/book ordering
swap. The independent #1127 repair traced that to a raw Binance task before
enqueue, not the FFI callback queues; release controls inherit its checked
`start_eager` production change unchanged. Release timing and an unqualified
full-output comparison remain pending.
