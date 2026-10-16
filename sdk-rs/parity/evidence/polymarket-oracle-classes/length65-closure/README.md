# Length-65 control closure

The accepted pricing experiment's length-65 production fault survived because
its valid-array profile used `n % 33` with n=0..63: no valid input had more than
32 elements. The native length comparator already rejected missing pairs;
the conditional fault was never reached. No production parsing behavior is
changed by this repair.

The revised 512-case token profile keeps every positive length 0..32 and adds
31 larger boundary lengths, up through 1025. It includes 63/64/65/66 and
127/128/129, 255/256/257, 511/512/513, 1023/1024/1025. Rust independently
constructs the input recipe and requires exactly one length-65 witness at
iteration296. Its native output length is asserted before inspecting source
output or output encoding. The fixed aggregate expectation is **9037 native
pairs**: sum(0..32), the 31 declared larger lengths, and 64 copies of the
mixed/Unicode/duplicate profiles' 1+2+2 accepted pairs. The case floor remains
512 calls, stopping at iteration511; this is finite coverage, not all lengths.

All twelve selected controls now require a compiled named runtime failure;
there is no survivor-accepting branch. The list is eleven Rust faults and one
source Map-erasure meta-control, stopping at control index11. Every restoration
must pass its exact native test. An unexpected survival is retained as a finding
and makes the campaign fail after recording every outcome. `verify.py` checks
input/source hashes, exact commands/selectors, the actual mutation bytes,
log hashes, named runtime assertions and independent twelve-control expectations.

Receipts for this campaign are isolated here. The original accepted pricing
receipts and length-65 survivor remain historical evidence; they are not erased
or relabelled. The old costs at511b do not price this larger revised corpus,
and this repair does not provide a universal per-row rate or complete parity.
Canonical declarations remain at [14](../../../MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../../../MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../../../MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../../../MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../../../MANIFEST-SPEC.md#forced-surrogate-conflict).

The starting merge is `1e613af14dd5fb8232db7b857761dfa9bb8dc6c6`, containing
fetched origin `1810a76a9764020538782a82b29780b98c9778d1` plus the accepted
pricing work and a local commit preserving its final post-commit receipts.
Frozen fetched base `1810a76a97` does **not** contain accepted merge
`53ee502232` (fixed-SHA ancestor check exit1). The original accepted pricing
source commit `902efafbb8` is an ancestor (exit0). An earlier check used the
shared, moving origin ref and incorrectly attributed its later ancestry to the
frozen base; this wording is corrected rather than treating that check as proof.
The now-visible fixed head `53ee502232` is merged before final handoff; source
equivalence against7a20 was rejected because the merge brought shared-core changes.
All nine baselines and twelve controls were therefore rerun at9ce4, as recorded
below; final evidence-only descendants receive their own three-gate receipts.
No other lane's unreviewed source was added.
No manifest row status is changed.

Run serially from repository root (one Cargo at a time; no other source readers
during mutations):

```sh
python3.14 sdk-rs/safe-cex-venue-polymarket/tests/class_cost.py baseline --evidence-dir sdk-rs/parity/evidence/polymarket-oracle-classes/length65-closure
python3.14 sdk-rs/safe-cex-venue-polymarket/tests/class_cost.py controls --evidence-dir sdk-rs/parity/evidence/polymarket-oracle-classes/length65-closure
python3.14 sdk-rs/parity/evidence/polymarket-oracle-classes/length65-closure/verify.py --sha <measured-commit>
python3.14 sdk-rs/parity/evidence/polymarket-oracle-classes/gates.py --evidence-dir sdk-rs/parity/evidence/polymarket-oracle-classes/length65-closure
```

Only the three current local gates are required: workspace formatting, locked
workspace build and host workspace Clippy with all targets. No wasm run and no
push. Full workspace/package tests, standalone wasm build/runtime, other venue
oracles, extractor/static checks and central wiring/layers/routing are not run
for this unit unless explicitly recorded later. Results below must name their
measured SHA rather than borrow previous campaign exits.

## Initial campaign at7a20 (historical)

At **`7a20fea5f09918ff0b7d3c1d3bcdb1c0b3bbd237`**, all nine fresh baseline
rows passed (exit0). All **11/11 selected Rust faults** and **1/1 source-encoder
control** compiled and failed their named native assertions (exit101); all
**12 restorations passed (exit0)**. **Zero survivors among these twelve selected
controls**, stopping at index11. The independent receipt verifier exited0.

The former survivor now fails at `generated_tokens_native_length:296`, native
length **0** versus independently expected **65**. The length17 control still
fails at iteration136. The restored token test asserts all 9037 native pairs and
its length65 witness; native lengths are checked before source output is
unwrapped. No output roundtrip is used.

The first baselines at811, before the review's assertion-order fix, are archived
under `attempt-history`; they are not counted as current-source evidence.
`verification.json`, `control-summary.json`, every per-command receipt and its
sanitized log retain their measured SHA. The next evidence-only commit receives
its own three-gate recheck; those exits must not be borrowed from an ancestor.

This closes the specific input-coverage gap exposed by the selected length65
fault. Larger lengths and other unselected faults can still escape a finite
corpus. No complete-function proof, population confidence percentage, updated
pricing rate or manifest-status upgrade follows from the twelve detections.

## Post-merge revalidation

Fixed integration head `53ee502232b005173d8a6f3f0d7cf94bad7f072c` was merged
at **`9ce4b36501015db12c8215d3c40fd4d8f8090555`**. It brought shared-core and
other venue changes, so the attempted source-equivalence check against7a20
failed. The prior campaign was not carried forward as current-head assurance.
All nine baselines and all twelve controls were executed again at9ce4.

Fresh results: **9 baseline exits0; 11/11 Rust faults plus1/1 encoder control
caught at named compiled native assertions (exit101); 12 restorations exit0;
zero selected survivors**. Length65 fails at `generated_tokens_native_length:296`
with native0 versus65. Independent `verify.py --sha 9ce4b36501` exited0. Current flat
receipts bind9ce4; earlier7a20 command receipts are preserved in attempt-history
and Git. No extra rates or complete-function claims are inferred.

Formatting, locked workspace build and host all-targets Clippy also exited0
at9ce4. The following evidence-only commit receives fresh three-gate receipts
at its own SHA. Its final local receipt records source equivalence to9ce4 and
fixed53 ancestry. No later moving tracking-ref value is used as a built parent.
