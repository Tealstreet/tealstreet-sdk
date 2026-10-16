# Coinbase delta parser native assertions

Code revision: `20896d5232c276bc8b2ced6776d6f30067bb307a`.
Integration parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
Fresh branch: `test/coinbase-delta-native-before-wire`; no other lane or
unreviewed feature branch is an ancestor.

The earlier-row audit found `parseDeltaLevels` already ported, with no
production mismatch found within the existing input boundary. Its existing
`public_delta_level_actions_and_error_precedence` test captured native results
into serialized values before checking them. The test now retains all 4,170
results and checks native result kind, level count, action enum, price and
amount bits, and error name/message for every case before the first schema or
serialization assertion. Expected values come from the live TypeScript
result. Raw JSON negative-zero cases remain in the corpus, and their source
tags become explicit negative-zero bit expectations. The existing schema and
wire comparisons run afterward.

Shared declarations remain at their canonical [string-boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[output-boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary) and
[numeric-ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect) anchors.
This change introduces no new decoder or representation policy.

## Measured exits at the code revision

| Command / measurement | Exit | Observed result |
| --- | --- | --- |
| `cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --test public public_delta_level_actions_and_error_precedence --jobs 2 -- --exact --nocapture` | 0 | One focused test passed, 20 filtered out. Its native pass stops after source case 4,170, then checks schema and wire values. |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2` | 0 | Finished dev profile in 35.51s, zero compiler errors. Scoped crate check. |
| `yarn parity:extract:check` through the installed Node/Yarn entrypoint | 0 | Output: `Would update 0 manifests; 0 added, 0 orphaned symbols.` |
| Python 3.12 `tomllib` census against the integration parent | 0 | 426 rows: 405 ported / 21 divergent / 0 pending at both revisions; zero row moves. No manifest or production file changed. |
| `git diff --check` | 0 | No whitespace errors. |
| Merge-down of fetched `origin/feat/safe-cex-rust` | 0 | Already up to date at `ac0f5fccf2`. |

The focused oracle used isolated TypeScript 6.0.3 through
`SAFE_CEX_TYPESCRIPT_PATH`, with its node_modules available. Cargo commands
ran sequentially with two build jobs.

## Independent mutation controls

Both controls swap only the price and amount input indices inside the helper,
leaving contexts, serializers, arrays and corpus length unchanged. The Rust
change swaps `level.get(1)` / `level.get(2)` in `parse_delta_levels`; the
TypeScript change independently swaps `level[1]` / `level[2]` in
`parseDeltaLevels`. Each changes two digits and preserves source-file byte
length. They are controlled working-tree mutations against the code revision,
not committed production changes.

| Run | Exit | Named observation |
| --- | --- | --- |
| Rust swap | 101 | `delta native price bits`, input `[["change",2,1]]`, frame `change`, context `field`. |
| Restore Rust | 0 | Focused test passed all 4,170 cases. Source bytes match the original SHA-256. |
| TypeScript swap | 101 | `delta native price bits` at the same input; source expectation changes independently of the native implementation. |
| Restore TypeScript | 0 | Focused test passed all 4,170 cases. Source bytes match the original SHA-256. |

[The measured control record](evidence/delta-native/controls.json) contains
the exact command, failing assertion excerpts, source identities, byte lengths
and original/mutant/restored SHA-256 values. The control driver exited 0 only
after requiring both 101 failures, both named native assertions and both
restored passes. A final `git diff --exit-code HEAD --` over both mutated
production files exited 0. No mutation survived and no production mutation is
left in the worktree.

## Review and scope

Local adversarial sub-agent review of the committed test found no material
issue. It confirmed independent source expectations, all native cases before
wire assertions, retained schema/wire checks and negative-zero bit handling.
The reviewer ran no gates; the runtime exits above are the authoring lane's
measurements.

This is an earlier-row test repair with zero pending moves. The separately
reviewed Coinbase notification repair is [PR 7863](https://github.com/Tealstreet/tealstreet-next/pull/7863);
its 14 finding annotations are not ancestors of this fresh branch. This branch
does not claim that its inherited notification census has been repaired.
The complete venue runner, the other 20 public tests, workspace compilation,
host clippy and wasm clippy were not rerun on this branch. No full JavaScript
input enumeration or notification ABI closure is claimed.
