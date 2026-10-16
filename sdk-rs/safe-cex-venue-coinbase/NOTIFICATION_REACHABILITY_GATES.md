# Coinbase notification audit gates

Code revision: `ab96095922d025b98e0b5e51e355a4ca9b31a668`.
Integration parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
No unreviewed lane is an ancestor of this repair.

## Measured at the code revision

| Command / measurement | Exit | Observed result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2` | 0 | Finished dev profile in 4m 44s; zero compiler errors. This is a scoped crate check, not workspace compilation. |
| `node --test sdk-rs/safe-cex-venue-coinbase/scripts/store-notification-declarations.test.mjs` | 0 | 13 passed, 0 failed, 0 skipped. Isolated TypeScript 6.0.3 supplied through `SAFE_CEX_TYPESCRIPT_PATH`. |
| `node sdk-rs/safe-cex-venue-coinbase/scripts/check-store-notification.mjs` | 0 | 262 audited roots, 262 matching source declarations, 4 injected function properties, 248 currently ported callables; 14 finding roots, 29 finding paths; 13 direct Store calls in 10 writer rows; 0 unhandled ported paths. |
| `yarn parity:extract:check` through the installed Node/Yarn entrypoint | 0 | Output: `Would update 0 manifests; 0 added, 0 orphaned symbols.` |
| `node sdk-rs/safe-cex-venue-coinbase/scripts/scoped-parity.cjs` | 0 | `parity: invention and status gates passed (426 rows, 948 public Rust items)`. |
| `git diff --check` | 0 | No whitespace errors. |
| Python 3.12 `tomllib` census of all 16 Coinbase manifests, compared with the integration parent | 0 | Parent: 405 ported / 21 divergent / 0 pending. Current: 391 ported / 35 divergent / 0 pending. Exactly 14 ported to divergent; zero pending moves. All nonpending target files exist; symbol resolution is the separate scoped gate above. |

## Full venue invocation and revision boundaries

`node sdk-rs/safe-cex-venue-coinbase/scripts/check.mjs` exited **0** with
`CARGO_BUILD_JOBS=2`, isolated TypeScript and its node_modules available. Cargo
completed 49 native test executables: 248 passed, 0 failed, 0 ignored, 0
filtered out. The subsequent Node suite passed 13/13 and the final production
notification walk exited 0 with the counts above. The existing mandatory
runner retains its Cargo phase and now runs three phases: Cargo, executable
declaration/control tests, and the standalone audit.

This invocation began at `f5d3edf6eb114e3c1c440d61a9968f9d9ad13448`.
The review correction `ab96095922d025b98e0b5e51e355a4ca9b31a668` was
committed while Cargo was building. It changed one manifest note, the review
document and the Node control suite; it changed no Rust source/test, shared
dependency, Cargo configuration, TypeScript source or runner. The Node phases
read the corrected suite after Cargo finished. A separate fresh 13-test run
and standalone walk at `ab96095922` also exited 0. This is not a claim that the
entire invocation started at `ab96095922`.

`git diff --exit-code f5d3edf6eb ab96095922 --` for crate `src`, `tests`,
`scripts/check.mjs`, workspace/crate Cargo files and Coinbase TypeScript
sources exited 0. The complete commit name diff contains only the three
review-correction files above; native tests do not read the changed manifest
note. The full invocation's revision boundary is recorded rather than silently
reattributing its starting commit. A merge-down before delivery reported
`Already up to date` against integration `ac0f5fccf2`.

## Discrimination controls

The executable Node suite creates disposable script/helper/review copies beside
the real script, and removes them in `finally`. It does not leave a mutated
production source behind. Its assertions require these exact child outcomes:

| Mutation | Exit | Required failing assertion |
| --- | --- | --- |
| Drop typed function properties | 1 | `every reviewed callable resolves to a source declaration` |
| Drop callable type aliases | 1 | `every reviewed callable resolves to a source declaration` |
| Restore the stale 159-root expectation | 1 | `reviewed callable corpus total` |
| Substitute a root while retaining length 262 | 1 | `every ported callable has notification review coverage` |
| Inject an actual Store write into `createAPI` through the source reader | 1 | `Store writer cannot claim notification parity: createAPI` |
| Change a reviewed call fingerprint | 1 | `every direct Store call matches its reviewed source site` |
| Inject an actual extra Store write into already-FINDING `tick` | 1 | `reviewed direct Store call floor` |
| Promote unsupported `tick` to ported | 1 | `reviewed callable status: CoinbaseExchange.tick` |

A no-op child exits 0, but the independent harness assertion
`independent reviewed root floor` throws as required. A missing compiler exits
1 with `MODULE_NOT_FOUND`; it is not counted as a skipped oracle. Restored
production walks and the mirror without ancestor node_modules exit 0. These
are Node audit controls, not Rust mutation exits or native ABI witnesses.

## Additional check and limits

`node --test scripts/rust-ci-wiring.test.mjs` exited 1: 177 passed / 86 failed.
Failures include the existing unclassified `safe-cex-lighter-sdk` workspace
crate and an Apexomni command expectation missing newer test targets. The
workflow, this test, and its layer-check inputs are unchanged by this repair;
this result is not a green CI claim. Coinbase removal/masking/conditioning and
its root runner wiring assertions passed within that run.

The [reachability review](NOTIFICATION_REACHABILITY_REVIEW.md) records the
native findings, all direct writer families and static-analysis limits. The
audit stops at the 262 reviewed roots and 13 recognized direct calls. External
callback bodies, destructuring aliases, `.call`/`.apply`, and arbitrary dynamic
dispatch are outside this lexical floor. No workspace build, host/wasm clippy,
full monorepo suite, CI rerun, or native notification ABI closure is claimed.

Sub-agent review at the code revision found no remaining material blocker in
this notification repair. Its earlier incorrect-await and missing-new-call
control findings were corrected before this revision. Review itself ran no
gates. Manifest classification uses [the canonical taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).
