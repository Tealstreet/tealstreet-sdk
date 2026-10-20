# Bitrue compiler trial after the 25-family merge

The fixed 21-family sequence stopped at Bitrue with identical output, memory
1.53x, main CPU 0.99x and steady CPU 0.93x. Eleven other public gates passed;
zero comparison pairs ran. Preserve `RUST_PERF_FIRST25.json.gz` and all older
negative results. The diagnostic attributes 30.542 ms to inclusive websocket
engine calls and 17.879 ms to host work outside those calls. Neither figure
isolates parsing or packing.

Merge landing `13e391377fdc48925541ada2e1766f918f82a889` (including
`74b0a7dd14`) and validate the 25-family integration first. Retain
the new prediction/currency wire contracts, account identity, transport-derived
options and polling controls alongside ordered batching. Keep the existing
singleton string path unchanged during this compiler trial.

After a source checkpoint, archive the 189 existing 21-family artifacts before
overwriting any package. Build only Bitrue with its existing venue-level `z`
profile; verify matching web/Node bytes and retain exact source/artifact hashes.
With all other owned jobs quiet, run exactly one official three-run public gate
using `E-BTC-USDT,E-ETH-USDT` and an exclusive output destination.

Change only `[profile.release.package.safe-cex-venue-bitrue]` to `opt-level = 3`.
Build Bitrue once, retaining both compiled versions and their size deltas. Run
exactly one official three-run public gate with the same fixture and helpers,
then one private correctness gate. Do not change singleton encoding, clocks,
comparison rules or thresholds between the two compiler configurations. Keep
every output and failure; no direction-based retries or extra samples.

Retain `3` only if its whole public gate passes (including memory), the private
gate passes, both unrounded Rust CPU medians improve over `z`, and the size
policy is satisfied. Otherwise retain `z` and report
the compiler experiment as inconclusive or unfavorable. A fixed-order local
trial is not the final matched before/after comparison. If a CPU gate remains
open, declare a separate bounded singleton-encoding experiment before running
it; the current diagnostics establish only a hypothesis for that path.

A numerical CPU, steady CPU or memory threshold failure is valid trial data;
retain it and continue to the other declared configuration. Correctness,
operational or invalid-memory failures abort the trial without replacement
runs. If selecting `z`, restore its saved nine bindings and Cargo profile field
byte-for-byte and verify their hashes; do not add a third rebuild.

Keep the existing 805,888-byte cap if the selected build fits. If it breaches,
record the actual size and apply the existing declared rule: actual size plus
3%, rounded up to a whole KiB. The full 25-family release and isolated feature,
size, real host/SDK, private and public gates remain required before a new
complete comparison. Earlier 21/24-family controls must retain their original
source labels; a final comparison needs a matching original landing control.

Review each phase with a sub-agent, commit stable results, and keep all source
and method changes explicit. Do not push or open the final PR until every
required gate passes; target `feat/rust-rewrite`, run adversarial reviews, wait
for green CI, merge and cut a fresh branch.

Integration checkpoint: all merge conflicts resolved preserving the nested
workspace and ordered delivery. Independent phase reviews passed for engine/wire,
host/native, WASM/loaders and replay harness. Verification passed: 2,308 Rust
tests in 183 groups; native all-target and WASM Clippy, formatting and bans;
487 harness tests; 44 repository typecheck tasks, 28 lint tasks and 27 test tasks.
The two Workers fixtures now register their connected client; assertions and
timeouts are unchanged. Retain the initial failing fixture/typecheck attempts
in `RUST_PERF_MERGE25.json.gz`. No new release/performance runs preceded this
checkpoint. Existing E2 binaries still need archiving before the compiler trial.
