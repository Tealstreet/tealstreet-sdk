# Binance formatter CI repair gates

Implementation measured: `183d894f2712e718d6d5d3d31de76ed90a1a3119`. Fresh integration parent:
`d4e20845a85cc4e2711c99648ef7547f8aa806df`.
PR #7947 contains no unmerged owner/prototype work from draft PR #7851.

## Repair and discriminating control

[Run 38705397284](https://github.com/Tealstreet/tealstreet-next/actions/runs/38705397284)
stopped at Check venue oracle binance with step exit 1 before Cargo tests:
the root-invoked generator selected an isolated default stable toolchain without
rustfmt. The overall run was cancelled; that is distinct from the observed step
failure. Git diff of the generator from that run's commit to the fresh parent
exited 0, recorded in ci-baseline-formatter-input.json.

Default formatting now runs from sdk-rs, selecting its toolchain pin and declared
components. Explicit RUSTFMT retains caller-directory semantics, including a
relative executable path. Generated values and freshness assertions are unchanged.
The two executable regression tests are imported by the mandatory CI wiring audit.

Removing cwd deliberately failed Node exit 1 at
`Binance formatter must select the pinned workspace before formatting`;
restoring the source bytes passed both cases with Node exit 0. The wrapper models the
missing root component without changing installed toolchains and delegates
successful formatting to the real pinned formatter. No freshly downloaded Linux
toolchain reproduction is claimed. Root freshness with RUSTUP_TOOLCHAIN unset
also passed with exit 0. These records belong to the measured implementation above.

Read-only adversarial sub-agent review approved the three-file change, including
both regression cases and preserved override semantics, without findings.
Review did not run Cargo and is distinct from the measured commands below.

## Measured local commands

| Named record | Exit | Seconds |
| --- | ---: | ---: |
| hotfix-workspace-fmt | 0 | 11.87 |
| hotfix-host-clippy | 101 | 1601.71 |
| hotfix-host-clippy-retry | 0 | 1647.72 |
| hotfix-workspace-build | 0 | 317.54 |
| hotfix-wasm-policy | 0 | 1.3 |
| hotfix-wasm-build | 0 | 531.74 |
| hotfix-wasm-clippy | 0 | 3381.4 |
| hotfix-binance-oracle | 0 | 1691.68 |
| hotfix-workspace-doctests | 1 | 0.1 |
| hotfix-workspace-doctests-retry | 0 | 449.29 |
| hotfix-wiring | 0 | 43.78 |
| hotfix-scoped-check | 0 | 324.69 |
| hotfix-extractor-check | 0 | 57.12 |
| hotfix-parity-status | 1 | 36.12 |

Exact commands, implementation SHAs and tracked-diff hashes are in matching JSON
files under gate-evidence/ci-toolchain. One Cargo process ran at a time, jobs 2,
toolchain 1.99.0. Host measurements are on macOS, not a Linux CI rerun. Wasm build
and all-target clippy are separate invocations of the official policy helper.

The first host clippy exit 101 was an incremental cache I/O failure in safe-cex-crypto's
lib test (missing dependency graph/query cache paths), not a successful lint
verdict. Its failure record is retained. Subsequent invocations explicitly set
CARGO_INCREMENTAL=0; the full host workspace all-target retry is recorded separately.
The initial workspace build completed with exit 0 before that environment change.

The initial doctest command exited 1 before compiling because cargo test rejects
--keep-going. Its argument-rejection record is retained; the corrected workspace
doctest retry uses --no-fail-fast and has a separate result above.

The full Binance command completed with 242 passed, 0 failed and 0 ignored across 66 binaries.
Independent expectation: 65 integration files through tests/types.rs,234 literal
attributes minus 1 macro-template attribute plus 9 source_test invocations through
pm_candle_delegate = 242; the library has zero tests. Expected and observed counts
are separate JSON records. Generator freshness and Yarn terminal output were
read; wrapper status alone was not treated as a gate.

## Scope

Rows moved: 0. Tomllib census: 252 ported / 22 divergent / 12 pending; 286 rows in 20 files
through binancespot.ws-public.ts.toml; unchanged relative to the fresh parent.
No TypeScript reference, generated Rust, production Rust, or parity classification
was changed. Canonical classification guidance remains at
[the existing taxonomy anchor](../parity/MANIFEST-SPEC.md#divergence-taxonomy).
The owner/prototype findings remain visible in separate draft PR #7851.
Whole-venue parity and whole-workflow green status are not inferred from this slice.

Not run: CI reruns, live venue requests or the full TypeScript monorepo suite.
Only the commands and feature combinations recorded here are claimed.

The extra parity-status command remains failed with exit 1 with status/invention diagnostics.
Inspection floor: first global status (AsterDEX reconcileBalanceAfterPrivateUpdate
rename), first invention (WsRuntime::monotonic_ms exemption), and first two Binance
statuses (createPublicAPI missing layer and api_factory.rs declaration).
Full diagnostic audit is not claimed. Exact samples are in parity-status-observed.json.
Rust sources, TOML files and Cargo.lock compare with exit 0 against the fresh parent, recorded
in parity-inputs-parent-diff.json; the parent executable was not separately rerun.
Extractor freshness does not establish Rust mapping validation or whole-workflow success.

## Evidence commit preservation

Evidence tip `74fbc828464d57eb80b07bc1cdb33999d1e87952` compared with measured source `183d894f2712e718d6d5d3d31de76ed90a1a3119`
with `git diff --exit-code`, excluding only this sheet and its normalized evidence
directory: exit 0. The exact command and changed paths are in
`gate-evidence/ci-toolchain/source-preservation.json`. Gate results remain attached
to the measured source; this audit is not a gate rerun.
