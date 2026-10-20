# Fifteen-family comparison protocol

Declared before collecting this cohort. Retain the initial eleven-family cohort
in `RUST_PERF_INITIAL11.json`, all 54 method profiles, the sampled CPU diagnosis,
and the aborted QoS pilot; this cohort does not erase or reclassify them.

Freeze candidate production Rust at merge 0f771c14b2. Control Rust is exactly
f66bdc26f9a7f419452b73cd9f484a4da63b5c4f, with its original default aggregate
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

First run one independent official three-run candidate gate per family, in the
fixed roster below. Each must pass. Then collect ten valid adjacent A/B pairs
per family, using one official single-run gate per side. Round-major schedule:
rotate the roster left by the round index; side order for a family's valid pair
index p and fixed roster index v is AB if (p+v) is even, BA otherwise. This gives
five AB and five BA pairs per family. Keep all samples and gate exits.

Roster: binance, bybit, okx, cryptocom, weex, bingx, mexc, blofin, backpack,
bitget, hyperliquid, bitrue, woo, lighter, orderly. Books use CI's exact family
symbols, including BTCUSDC and ETHUSDC for Orderly.

Persist every attempt's commands, source/artifact/input hashes, elapsed time,
exit status, raw outputs and logs before classification. CPU, steady CPU and
memory threshold failures are valid data, never a reason to replace a pair.
Output/signature/subscription/request/write differences, invalid arithmetic,
missing/nonfinite metrics, process timeouts and other execution failures abort
with evidence retained. Only the existing settled-memory timeout or a negative
aggregate retained-memory baseline may exclude an entire pair. Preserve both
sides of any excluded pair. Retry that same family/order in a fixed immediate
retry slot, up to fifteen total attempts per family, independent of performance
direction. Stop at ten valid pairs regardless of result; exhausting fifteen is
inconclusive. Refuse to overwrite prior report or log destinations.

The primary table uses pooled raw TS and Rust upper-middle medians, matching the
official gate formula, rather than medians of per-pair ratios. Report main CPU,
steady CPU, retained memory, raw CPU ranges, AB/BA strata and individual paired
margin changes. Every candidate family must satisfy the official aggregate
thresholds and have strictly wider main-CPU and retained-memory ratios than its
control. A narrowing keeps the performance objective open and calls for an
evidence-backed source phase, not extra favorable runs. Label tiny gains amid
wide variation as fragile; no p-value-based success rule.

Independently reviewed by the native-host reviewer before implementation. The
comparison is estimated at 45–80 minutes for 15 families from previous recorded
process envelopes; shared-host contention may extend it. Private correctness,
per-family sizes, Rust/TS checks and final CI remain independent required gates.

The fourteen-family protocol was never sampled; retain it as an unmeasured
checkpoint. This is a fresh cohort with new report/log destinations. Before
starting, finish all 32 private variant gates (including all seven Orderly
names on both networks), Bitget classic/inverse/UTA parity checks and full
repository checks. Optional diagnostic profiles use exactly three replays and
three probe rounds for each of OKX/Bybit single/batch; finish them before this
uninstrumented cohort. Paired margin changes include main CPU, steady CPU and
retained memory. Do not overwrite earlier checkpoints or diagnostic attempts.

HOLD: before any sample, origin922b5fdc89 landed Bybit variants/shared engine
changes. This source freeze is superseded; do not run this protocol until its
source pins and variant correctness scope are refreshed and reviewed. No
fifteen-family public or optional diagnostic samples were collected.
