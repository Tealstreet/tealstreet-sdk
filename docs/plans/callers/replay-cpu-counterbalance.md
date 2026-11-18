# Fixed replay CPU sampling

SDK CI and local public replay callers use sixteen fixed interleaved pairs,
eight in each physical order. Explicit local budgets may be larger but must
be even; canonical collection and certification require exactly sixteen.
Callers supplying `--runs 9` must move to `--runs 16`. Private correctness
counts, formulas, tolerances and the strict 1.05 eligibility bound do not change.

All first and eligible second measurements retain every original result.
The raw inventory is 34 slots, with each implementation/mode/index checked;
offline and Bitget paired consumers require the matching fixed inventory.
Older nine-pair evidence cannot be substituted into a new certificate.
Historical sealed runtimes remain evidence for their original protocol.

Reports retain CPU model, guest topology, frequency source/range and physical
sample order. These fields diagnose the runner; they never select a baseline
or grant acceptance. Optional A/A and GC/tiering dispatch artifacts are
explicitly non-certifying and are not run on every PR. No application, NX,
server, release, WASM cap or runtime venue obligation changes.

Evidence and the remaining EPYC 7763 TxFlow slowdown are documented in
`docs/plans/RUST_GATE_TIMING.md`. Fixing that slowdown is separate work; the
unchanged strict objective continues to fail it.
