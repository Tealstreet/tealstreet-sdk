# PR 7851 sprint merge evidence

Integration parent: `539ac8165b8e36ef009dc467123e69fe14e002e2`.
PR head: `40e3b3aadae05e95035dea0e1d90084bd659cdaa`.
Initial merge: `9edcdc5f9f5d22a9d2c7711ef00c1a39d4b117d2`.
Batch head: `dcb88b8e64efc8da8ed831388c9adaec2609b77f`.
Resolved source commit: `bbea559c1ce7eee3ac44ae5cd30320e9ff8d1c45`.

The batch was merged into `merge/binance-owners`. Three conflicted files required reconciliation: the owned-task tests, exchange runtime error sources, and MEXC public websocket handlers. Both parent test families remain: 19 owned-task tests, the exact union of the parents' 16 and 15 declarations. Enumeration stopped at `priming_poll_cancellation_does_not_reinsert_task`; this is the conflict scope, not a repository census. Parent-only test bodies were retained. Opaque Store subscriber causes remain directly observable while typed REST and Store origins remain retained. MEXC retains replacement-array behavior and fallible statistics cleanup. Adjacent assertions now include the retained original Store cause and independently check its identity.

Two independent sub-agent reviews found no actionable blocker within the merge resolutions. Reviewers inspected the parent union, original error identity, typed provenance, complete metadata assertions, MEXC replacement arrays and fallible call sites. They did not run Cargo or audit the entire batch.

Manifests were merged without regeneration. Historical `GATE_EVIDENCE.md` content was retained. Existing venue findings remain recorded; whole-venue parity is unclaimed.

## Measured gates

Workspace build **0** at the resolved source commit (444.7 seconds). Host workspace clippy was interrupted after the user confirmed the merge landed and redirected this lane: **no Cargo clippy exit measured**, orchestrator exit **130**. Scoped tests and the planned error-source mutation were **NOT RUN**. Machine-readable records are under `gate-evidence/sprint-merge-7851/`. Each record names its measured commit and whether tracked inputs differed from it. Rust 1.99.0, jobs 2, incremental compilation disabled. Build and clippy ran serially.

The initial pre-batch build passed at `9edcdc5f9f5d22a9d2c7711ef00c1a39d4b117d2`. Its clippy was interrupted for the requested batch merge: no Cargo clippy exit was measured (orchestrator exit 130). Those historical results do not cover the resolved batch.

## Scope

No pushes. This deliverable merges the requested heads and preserves their work; it does not close additional Binance findings. No wasm clippy/build, doctests, formatter, extractor, whole-workflow parity job or full workspace test suite was run for this merge. The requested handoff gates are workspace build and host workspace clippy; scoped native assertions and the error-source control were planned but not executed before the priority change. The interrupted clippy run is not a passing gate.
