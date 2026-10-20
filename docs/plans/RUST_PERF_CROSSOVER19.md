# Nineteen-venue / fifteen-family comparison protocol

Declared before collecting this cohort. Retain the initial eleven-family cohort
in `RUST_PERF_INITIAL11.json`, all 54 method profiles, the sampled CPU diagnosis,
and the aborted QoS pilot; this cohort does not erase or reclassify them.

Freeze candidate production Rust at merge ecca589fb3334332fb0563b116f96f0086cb7de9. Control Rust is exactly
922b5fdc894e0641a05abe9f63d732dc9dddd104, with its original default aggregate
build profiles, bindings, immediate-message host and aggregate loader. Use the
same final shared replay/helper import closure, wire decoder, fixture bytes,
TS sources, dependency lockfile and Node executable on both sides. Hash these
inputs and both runtimes/bindings before and after every comparison pair.
Changes needed for correctness require a documented new source checkpoint and
refreshed provenance before measurements; no CPU-directed run extensions.

Finish all owned builds/tests before measuring. No inspector, method observer,
QoS clamp, concurrent replay or other owned workload during this cohort. The
shared machine may still have external load; report observed variability instead
of excluding slow or unfavorable samples. Optional decoder prototype remains
deferred: sampled WASM cost does not establish duplicate UTF8 validation cost.

First run one independent official three-run candidate gate per venue fixture, in the
fixed roster below. Each must pass. Then collect ten valid adjacent A/B pairs
per venue fixture, using one official single-run gate per side. Round-major schedule:
rotate the roster left by the round index; side order for a venue fixture's valid pair
index p and fixed roster index v is AB if (p+v) is even, BA otherwise. This gives
five AB and five BA pairs per venue fixture. Keep all samples and gate exits.

Roster: binance, bybit, okx, cryptocom, weex, bingx, mexc, blofin, backpack,
bitget, hyperliquid, bitrue, woo, lighter, orderly, bybitinverse, bybitspot, bybitdemo, hyrotrader. Books use CI's exact family
symbols, including BTCUSDC and ETHUSDC for Orderly, BTCUSD and ETHUSD for Bybit
inverse, and BTCUSDT and ETHUSDT for the other Bybit profiles.

Persist every attempt's commands, source/artifact/input hashes, elapsed time,
exit status, raw outputs and logs before classification. CPU, steady CPU and
memory threshold failures are valid data, never a reason to replace a pair.
Output/signature/subscription/request/write differences, invalid arithmetic,
missing/nonfinite metrics, process timeouts and other execution failures abort
with evidence retained. Only the existing settled-memory timeout or a negative
aggregate retained-memory baseline may exclude an entire pair. Preserve both
sides of any excluded pair. Retry that same venue/order in a fixed immediate
retry slot, up to fifteen total attempts per venue fixture, independent of performance
direction. Stop at ten valid pairs regardless of result; exhausting fifteen is
inconclusive. Refuse to overwrite prior report or log destinations.

The primary table uses pooled raw TS and Rust upper-middle medians, matching the
official gate formula, rather than medians of per-pair ratios. Report main CPU,
steady CPU, retained memory, raw CPU ranges, AB/BA strata and individual paired
margin changes. Every candidate venue fixture must satisfy the official aggregate
thresholds and have strictly wider main-CPU and retained-memory ratios than its
control. A narrowing keeps the performance objective open and calls for an
evidence-backed source phase, not extra favorable runs. Label tiny gains amid
wide variation as fragile; no p-value-based success rule.

Independently reviewed by the native-host reviewer before implementation. The
comparison is estimated at 60–100 minutes for 19 venue fixtures from previous recorded
process envelopes; shared-host contention may extend it. Private correctness,
per-family sizes, Rust/TS checks and final CI remain independent required gates.

The fourteen- and fifteen-family protocols were never sampled; retain it as an unmeasured
checkpoint. This is a fresh cohort with new report/log destinations. Before
starting, finish all 36 private variant gates (including all seven Orderly
names on both networks), Bitget classic/inverse/UTA parity checks and full
repository checks. Optional diagnostic profiles use exactly three replays and
three probe rounds for each of OKX/Bybit single/batch; finish them before this
uninstrumented cohort. Paired margin changes include main CPU, steady CPU and
retained memory. Do not overwrite earlier checkpoints or diagnostic attempts.

This refreshed checkpoint includes the Bybit variant merge before its first
sample. ESBUILD_WORKER_THREADS=0 applies to both official replay subprocesses
and the replay CLI so compiler workers do not retain shared buffers inside the
engine measurement. Preserve loaded-code/constructed-engine baselines, signed
component releases, exact arithmetic, nonnegative aggregate public retention,
post-cutoff GC and lazy post-GC WASM byte accounting on both sides. The newly
landed book-identity and ticker-storage optimizations belong to the original
control too. Settled-memory diagnostics now include sample/transition/count
metadata; only that exact collector error shape is a quality exclusion.

Before any public/profile sample, private Kodiak validation rejected a
consistent TS net release (-1984568 bytes). Preserve the failed private report
and input manifest19 as unmeasured checkpoints. Private runs now allow a
negative net total while retaining finite absolute counters and exact signed
arithmetic; public default validation and thresholds are unchanged. Refresh
both roots to this helper checkpoint in a new manifest19b, then rerun all36
private variants before sampling. Original production binaries stay pinned.
