# Registry parity-step recovery

Code revision: `e38190956cb4d1a780e1b5f5143ad8698bd1b9fa`.
Pre-repair head: `d9dc86000cecaa9ea8c710c1d5e3805044a07dd2`.
Actual integration merge base: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.

[Run 38676079756, job 116229868819](https://github.com/Tealstreet/tealstreet-next/actions/runs/38676079756/job/116229868819)
passed Coinbase's notification audit but failed at
`Check live registry TypeScript equivalence`. `registry-data` panicked with
`Serde derive must supply the enum wire inventory`; the Node checker observed
Cargo exit 101. This is a failure of the full parity job. The earlier Coinbase
venue result did not establish that the job was green.

`GatewayChannel` gained a strict string decoder in shared-types commit
`3bbc52657f`. It passes a string to its private derived wire enum, rather than
calling the example's custom `deserialize_enum` collector. The example was
unchanged. I own this registry-oracle repair on the Coinbase recovery PR.

The repair captures Serde's static variant metadata through its typed
`unknown_variant` error, supports direct enum derives too, and retains the
exhaustive Rust match that forces review of native-only/skipped variants.
Missing, empty, or successful-without-metadata probes fail closed. No manual
production variant list or error-message parsing replaces the inventory.

| Measurement | Revision | Exit | Observed result |
| --- | --- | --- | --- |
| `yarn registry:check` | `d9dc86000c` | 1 | Same inventory panic as CI; Cargo child 101. |
| `yarn registry:check`, after actually checking out the merge base without stash | `ac0f5fccf2` | 1 | Same inventory panic; Cargo child 101. |
| `cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --example registry-data --jobs 2` | Code bytes later committed as `e38190956c` | 0 | Three example tests passed before commit; restored run below remeasures them at the committed revision. |
| `yarn registry:check` | `e38190956c` | 0 | Output confirms 47 ordered names and live-source thresholds/subscription keys. The Node oracle negative-control test passed, zero skipped. |
| Same-length seven-member metadata replacement at `EnumInventoryError::unknown_variant` | `e38190956c` plus controlled working-tree mutation | 101 | `tests::gateway_strict_decoder_supplies_all_seven_native_wire_members` fails native slice equality: wrong final name despite seven members. |
| Restore original source; run all example tests | `e38190956c` | 0 | Three passed, zero failed. Original and restored SHA-256 match. |

[The control record](evidence/gate-recovery/control.json) retains the code SHA,
source hashes and measured exits. A final source diff against HEAD is clean.
The original registry example and gateway decoder are byte-identical between
merge base and pre-repair head. Local compiler: Rust 1.99.0; Cargo build jobs
limited to two. TypeScript and its node_modules were available to the oracle.

## CI-wiring comparison

`node --test scripts/rust-ci-wiring.test.mjs` was actually run at both
`ac0f5fccf2` and `d9dc86000c`. Both exit **1**, with **177 passed / 86 failed**.
The first layer assertion in `sdk-rs/scripts/check-layers.test.mjs` fails with
`unclassified crate: workspace -> safe-cex-lighter-sdk`. The Apexomni script
equality assertion in `scripts/rust-ci-wiring.test.mjs::audit` expects four test
targets while `package.json` now declares eight. These unchanged upstream inputs
are verified by a scoped base/head diff exiting 0. Their cascading failures are
not Coinbase regressions. I also own their repair in this recovery work; this
diagnosis does not call them passed.

Workspace build/check/fmt, host clippy and wasm gates remain separate required
measurements. They are not implied by the registry step or its example tests.
The full CI parity job must complete successfully before it is called green.

## CI-wiring repair and discriminating controls

At code revision `43cabb625fc4c838c6ba39b2900d463cee8f5e45`, the same
`node --test scripts/rust-ci-wiring.test.mjs` exits **0** with **286 passed,
zero failed and zero skipped**. The repair classifies the Lighter protocol SDK
in `exchange-core`, adjacent to crypto below it and venue adapters above it,
with one explicit existing downward skip to numeric primitives. The six venue
layer grants and the validator are unchanged. The independent script literals
now retain Apexomni's eight and KuCoin's forty actual test targets. KuCoin's
stale command assertion also failed in both original base/head logs; it remained
visible after the first two faults were repaired.

The nineteen new command-omission tests cover the four added Apexomni targets
and fifteen added KuCoin targets. This is the stopping floor for those controls,
not a claim that every target was individually mutated. The Lighter direction
control first requires the live graph to pass, then injects an upward/cycle edge
and requires its named refusal. Both live and populated venue fixtures run.

Two additional mutations changed the actual files in this worktree, individually:

- Remove the SDK classification: `every actual Cargo workspace crate obeys its
  declared layer` fails; Node exits **1**.
- Remove Apexomni's `exchange` test target from `package.json`: `Rust workflow
  enforces parity and live routing gates` fails; Node exits **1**.

Both files were restored in `finally` blocks. Original and restored SHA-256
values match, and the restored full suite exits **0**, 286/286 passing. The
[control record](evidence/gate-recovery/ci-wiring-control.json) records the code
revision, assertions, hashes and exits. Sub-agent review checked the graph
classification, real target files, command literals and control discrimination;
it caught the initial erroneous forty-one-target label, corrected in this
revision. The reviewer did not run Cargo, metadata or tests.

Workspace fmt check at `e38190956c` exits **0**. An initial workspace check
started at `00658c87de` was manually interrupted with exit **130** after free disk
space fell to about 575 MiB; no compiler error was observed before interruption.
Its Cargo inputs remained byte-identical through `43cabb625f` (scoped diff exits
0). This is not a pass or proof of the cause of the stall. Only this worktree's
generated incremental cache was removed. The restarted workspace gates disable
incremental caching, retain all selected targets/lints, and limit Cargo jobs to
two. The restarted `cargo check --manifest-path sdk-rs/Cargo.toml --workspace
--all-targets --locked --jobs 2` exits **0** at code revision
`43cabb625fc4c838c6ba39b2900d463cee8f5e45`, after 2351.54 seconds. A scoped diff
excluding documentation and evidence between that revision and `107d2cd988`
exits **0**. Host `cargo clippy --workspace --all-targets --locked --jobs 2`
also exits **0**, after 5044.73 seconds, against the same Rust revision. The
scoped Rust/Cargo/wasm-runner/workflow diff through `a6fe6224e5` exits **0**;
later JavaScript dependency repair commits did not change those inputs.
Workspace `cargo build --workspace --locked --jobs 2` exits **0** after
361.61 seconds. The wasm policy/runner suite exits **0**, 14 passed, zero failed
or skipped; the policy classifies 38 browser crates and one native-only crate.
The full policy-selected wasm build exits **0**, after 1524.56 seconds. The full policy-selected wasm clippy run also exits **0**, after 5529.63 seconds. The scoped Rust/Cargo/wasm-runner/workflow diff through `881dcb68c0` exits **0**. Fresh workspace fmt exits **0**, after 10.63 seconds. The full CI-wiring suite, including the new dependency tests, exits **0**, 290 passed, zero failed or skipped, after 13.38 seconds. Both were measured at `881dcb68c0dd0e25cfb5d801b4dc431f166b111e`. The eight-target Apex native run also exits **0** at that revision, after 530.20 seconds: exchange 59, protocol 7, pure 6, records 28, signer_utils 6, signing 7, transaction_records 21, and utils 10 tests passed, zero failed, ignored or filtered out. This enumeration stops at those eight command-listed targets, ending with utils; it does not imply later integration parity steps passed. The [workspace gate record](evidence/gate-recovery/workspace-gates.json)
records only completed exits and names the outstanding measurements. These
measurements do not cover newer integration Rust inputs. Integration parity
runs at `f2229103d0`, `0445a75375`, and `d19d2874e0` were cancelled;
successor run 38692425805 at `f853d56fd2` completed with failure. Coinbase and
registry passed; `Check apexomni live TypeScript conformance` failed with
53 tests passed and six failed before native assertions: three source loaders
reported `ERR_PACKAGE_PATH_NOT_EXPORTED` for `@noble/hashes/hmac.js`, and three
reported `MODULE_NOT_FOUND` for `bignumber.js`. This matches the isolated
dependency repair in PR #7902; its local gate measurements are recorded separately above.
[The integration job](https://github.com/Tealstreet/tealstreet-next/actions/runs/38692425805/job/116281188601)
is red; no integration parity pass is inferred.
