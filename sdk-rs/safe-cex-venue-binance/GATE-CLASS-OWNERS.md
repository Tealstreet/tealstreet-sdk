# Binance final owner slice

Integration parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
The inherited unfinished assemblies were checkpointed, then cherry-picked onto
that integration parent. No unrelated feature branch is an ancestor of this
slice. PR: https://github.com/Tealstreet/tealstreet-next/pull/7851.

## Census and scope

Independent Python `tomllib` enumeration compared keyed symbols in twenty
Binance manifests against the integration parent. It stopped at
`binancespot.ws-public.ts.toml`, covering 286 rows:

| Revision | Ported | Divergent | Pending |
| --- | ---: | ---: | ---: |
| Integration parent | 252 | 22 | 12 |
| Implementation `fc427e57abdb790830d80d1b074c69003e3fcb98` | 252 | 34 | 0 |

**Rows moved: 12 pending to divergent; 0 pending to ported.** Concrete Rust
dispatch, public/private socket and exchange owner items now exist. The nine
dispatch/socket rows retain the observed source/native string finding, citing
[the string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary) and
[surrogate observability](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
The three exchange rows inventory the omissions reviewed in
[CLASS_OWNER_REVIEW.md](CLASS_OWNER_REVIEW.md), with links to
[alias ownership](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) and
[the taxonomy](../parity/MANIFEST-SPEC.md#divergence-taxonomy).
These results do not establish whole-venue parity or repair those omissions.
Earlier body-row notes retain their extraction boundaries and unmeasured
composition qualifications. This slice does not retroactively broaden their
proof; zero pending describes row statuses, not the absence of open findings.

## Measured gates

Machine-readable results live in [gate-evidence/class-owners](gate-evidence/class-owners).
Every record retains the SHA where its command actually ran.

| Gate | Measured revision | Result |
| --- | --- | --- |
| Crate formatting check | `f14b814e119c032a3da44582062850e7e9a40573` | exit 0 |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-binance --all-targets --jobs 2` | `f14b814e119c032a3da44582062850e7e9a40573` | exit 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-binance --all-targets --jobs 2 -- --include-ignored` | `fc427e57abdb790830d80d1b074c69003e3fcb98` | exit 0; 274 passed, 0 failed, 0 ignored |
| Extractor writer | `fc427e57abdb790830d80d1b074c69003e3fcb98` | exit 0; Updated 0 manifests; 0 added, 0 orphaned |
| Extractor check | `fc427e57abdb790830d80d1b074c69003e3fcb98` | exit 0; Would update 0 manifests; 0 added, 0 orphaned |
| Four new source oracle scripts in a tracked-source mirror without ancestor dependencies | `fc427e57abdb790830d80d1b074c69003e3fcb98` | all four exit 0 |

The full test run launched all seventy independently enumerated top-level
integration test targets plus the library binary. Enumeration stopped at
`tests/types.rs`; no doctest or workspace test coverage is claimed. See
[full-tests.json](gate-evidence/class-owners/full-tests.json).

The formatter/check results retain their measured revision. Actual
`git diff --exit-code f14b814e11 fc427e57ab` over crate Rust source, tests,
Cargo manifest and workspace lockfile exited 0. The intervening committed delta
contains only three oracle dependency-loader scripts. See
[compile-preservation.json](gate-evidence/class-owners/compile-preservation.json).
The integration workflow is unchanged: 92 named steps and one Binance oracle
step on both sides. No gate was removed or renamed.

Fetched integration was merged down before delivery and was already contained.
At evidence commit `803f0314053457467217e464c4a6b97f47a6a245`, the actual
whole-repository diff against the full-test SHA, excluding only this slice's
evidence paths, exited 0. The later proof commit adds evidence only; results
remain attached to their original measured SHAs. See
[evidence-tip-preservation.json](gate-evidence/class-owners/evidence-tip-preservation.json).

The mirror used the canonical `safe-cex-types/ci-deps.cjs` selectors, copied
lockfile, and frozen dependency installation with scripts disabled. Install
output completed successfully. Missing-hmac and missing-lodash controls each
failed with Node exit 1 and `MODULE_NOT_FOUND`; restoration passed with exit 0.
See [isolated-oracles.json](gate-evidence/class-owners/isolated-oracles.json).

The earlier full-test attempt at `f14b814e11` was cancelled with exit -2 when
the mirror exposed ambient lodash resolution. It did not pass. The loader was
fixed before the final full-test run. A still earlier check was cancelled with
exit -15 following review; an exploratory check during edits is not used as
final evidence.

## Discriminating controls

Five Rust mutations and four actual-TypeScript-body corruptions ran serially at
`9e520a107728af40dbe8c8d4f80157a36967ef53`. Each failed with Cargo exit **101**
at the named assertion; each restored run exited **0**. Rust inputs were restored
byte for byte. The source corruptions affected isolated VM memory, not tracked
TypeScript. Enumeration stopped at control nine, `source-exchange-client`.

| Control | Named assertion |
| --- | --- |
| Rust dispatch payload | `native JSON overflow is Infinity` |
| Rust public ticker alias | `native same-generation alias receives second price 12.3 rather than 4.2` |
| Rust synchronous private open | `native synchronous open callback must propagate failure` |
| Rust exchange client route | `native exchange constructor client route binancepm.fapiXhr` |
| Rust retained async rejection | `native retained unobserved rejection reports before release` |
| Source dispatch | `actual source escaped surrogate invokes dispatch callback once` |
| Source ticker alias | `actual source same-generation alias receives second price 12.3 rather than 4.2` |
| Source private open | `actual synchronous onOpen preserves original thrown failure` |
| Source client route | `actual PM FAPI client remains primary-only FAPI factory` |

The committed `actual_source_controls_fail_at_the_named_assertion` test also
requires the child exit and its specific `AssertionError`; a dependency failure
does not satisfy it. Replacing the oracle with a successful no-op failed Cargo
101 at `actual source control must assert-fail: dispatch`, then restoration
passed 0, measured at `4d2600b115000ca7e78ef1956163919e60e22658`.
See [source-control-noop.json](gate-evidence/class-owners/source-control-noop.json).

A stronger native numeric conversion control ran at `fc427e57ab`: replacing
the lexical f64 parse with `as_f64().unwrap_or(0.0)` changed decoded Infinity to
0. `dispatch_native_overflow_signed_zero_and_deep_frame` failed with exit 101
at `native JSON overflow is Infinity`, then the restored test passed with exit
0. The original input hash and final byte restoration are recorded in
[rust-native-overflow-conversion.json](gate-evidence/class-owners/rust-native-overflow-conversion.json).
Final source/test `git diff --exit-code HEAD` exited 0. No control in this
enumerated set survived; this makes no claim about unrun mutations.

Source case floors: dispatch 64 cases through case 63; public classes five
through case 4; private classes three through case 2; exchange constructors
three through case 2. Alias collision uses two independent source rows. Source
surrogate observations cover the helper and three private owners; native public
observations cover five owners. Native assertions run before source/wire
comparisons. These are measured floors, not exhaustive input domains.

## Review and limits

Local read-only sub-agent rounds found and repaired the issues recorded in
[CLASS_OWNER_REVIEW.md](CLASS_OWNER_REVIEW.md). Public review approved `9e520a1077`,
exchange review approved `b80fa59337` for the explicitly divergent slice, and
private review approved both `9e520a1077` and the final loader delta at
`fc427e57ab`. Reviewers did not run Cargo or mutation gates.
The final read-only evidence addendum audit checked the raw full-test and
numeric-control logs, independent target enumeration, restored input hash and
empty source/test diff; it found no material findings.

Not run: workspace build, full monorepo suites, host clippy, wasm clippy or wasm
build, doctests, CI reruns, live exchange requests, exhaustive enumeration or a
whole-venue parity audit. Accepted source/native findings remain inventoried.
