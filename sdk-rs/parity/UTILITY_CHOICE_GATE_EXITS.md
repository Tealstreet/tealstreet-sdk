# Utility choice gate exits

Original integration parent:49a942e76143d6d1d44567edff59c12669d92627.
Rationale/native gate commit:d537489ceecc805874bf56eb0bb9daecac80c8c7.
Oracle repair/control commit:43434194953eebbc0041448422428d55e5f44e4d.
Merge-down integration:60c59f017b79d1dabbc41fafa4f254f86ede5c0a.
Measured delivery merge:0b7b25f9f9b0b846e4afc315a7944fb791321696.
No unmerged feature branch is an ancestor. Registry PR7978 is included through
its integration merge. Its policy and this unit's two policies were retained
once each when resolving their append conflict. This is authored evidence;
temporary captures/logs remain outside the repository.

## Measured gates

All Cargo runs use CARGO_INCREMENTAL=0 and two jobs, with one Cargo at a time.
The native commands use --manifest-path sdk-rs/Cargo.toml --locked --jobs2.

| Measured commit | Command/scope | Actual exit | Completed observation |
| --- | --- | ---: | --- |
| d537489cee | cargo test -p safe-cex-store -p safe-cex-crypto --test remaining_utils |0 |Two crypto and four store tests; native values/identity, no Rust serialization |
| d537489cee | cargo test -p safe-cex-store --lib virtual_clock::tests::periodic_timer_preserves_the_ts_sleep_detection_bug |0 |One selected clock test,32 filtered out |
| d537489cee | standalone utils_final_oracle.cjs |1 |ENOENT: assumed worktree-local nanoid file; a fixture portability failure |
| d537489cee | clock_oracle.mjs --check |0 |19 source cases, ending at worker-timer; existing corpus unchanged |
| 4343419495 | node sdk-rs/parity/scripts/utility-source-gate.cjs |0 |Resolved nanoid6.0.1; literal source payload and clock-marker guards |
| 0b7b25f9f9 | Same guarded source command |0 |Both source producers completed; no empty-success acceptance |
| 0b7b25f9f9 | cargo build --workspace --locked, from sdk-rs |0 |Finished0.93s; CARGO_BUILD_JOBS=2; zero warnings/errors |
| 0b7b25f9f9 | cargo clippy --workspace --all-targets --locked, from sdk-rs |0 |Host workspace lint, Finished53.19s; zero warnings/errors |
| 0b7b25f9f9 | node ../.github/scripts/rust-wasm.mjs --clippy, from sdk-rs |0 |Helper0/asserted Cargo child0;38 selected packages/all targets, FFI wasm feature, Finished56.17s; zero warnings/errors |
| 0b7b25f9f9 | parity:extract, explicit YarnJS entry |0 |Output Updated0 manifests,0 added,0 orphaned |
| 0b7b25f9f9 | parity:extract:check, same entry |0 |Output Would update0 manifests,0 added,0 orphaned |
| 0b7b25f9f9 | Independent tomllib/Git census against integration60c59f017b |0 |512 manifests; exactly11 notes plus one declared mirror-to-adapted layer correction; zero status/target/file/row changes |
| 0b7b25f9f9 | Git diff against b4abce7569: Rust/Cargo/toolchain/.github |0 |All these executable/build inputs are unchanged, not a transferred numeric gate claim |
| 0b7b25f9f9 | git diff --check |0 |No whitespace errors |

Writer/check text was inspected. Source/native gates are distinct commands;
seven native tests finished before the first source invocation. No new Rust
runtime or test body is added here, and native gates were not rerun after the
integration merge. Delivery Cargo gates are measured separately on that merge.
The wasm helper uses the actual workflow arguments, not an extra -D warnings
flag. It validates all38 selected packages and excludes only native-only parity;
it does not run the separate wasm build/runtime/all-feature job steps.

## Deliberate controls

All four controls ran at43434194953eebbc0041448422428d55e5f44e4d. HEAD did not
change; each file was restored bytewise before rerunning the same gate. These
are source-producer and annotation controls, not new native-runtime mutations.

| Iteration | Deliberate break | Named failing assertion | Mutated/restored |
| ---: | --- | --- | --- |
|1 |Empty utility source producer |UTILITY SOURCE producer emits JSON |1/0 |
|2 |Empty clock source producer |UTILITY CLOCK completed nineteen-case marker |1/0 |
|3 |Actual source predicate uses a fixed copy instead of its mutable exclusion array |source mutable exclusion list |1/0 |
|4 |Change selected manifest uuid status to ported |UTILITY NOTE-ONLY STRUCTURE |1/0 |

Enumeration stops at control4. There were no survivors in these four controls.
The earlier ENOENT failure is retained as a fixture discovery, not mislabeled as
an assertion mutation. The actual TypeScript, clock corpus and manifest status
were restored; no source behavior was repaired or fixture expected value changed.

## Floors and limits

The independent named cohort contains11 identities across eight utility files.
Their tomllib census is20 ported /11 divergent /0 pending; statuses do not move.
Only scheduleNextSync changes its layer to adapted, reflecting its actual host
future/handle/generation adaptation under the row schema. This is11 new owner
rationales, one layer correction, one oracle dependency-path repair and producer
guards, with zero SDK runtime repairs or canonical-checker changes.

The source guard independently expects the exact15 exclusion names,16 outcome
booleans, one deterministic nanoid sample, two wrapped byte projections and the
19-case clock completion marker. Those are literal expectations, not counts
copied from an output artifact. The older utility oracle also executes ancillary
wallet/merge/resolver/loop cases; those executions do not resolve extra rows.
No arbitrary input, real CSPRNG distribution, complete event-loop behavior or
production embedding conformance is claimed. Concrete startup capture,
byte-source construction and clock proxy/timeout/timer/unref assembly remain
unproved. Node24.20.0 arm64 and the existing compiler were used; no installation
or isolated compiler/dependency validation is claimed for this unit.

Read-only sub-agent review passed all eleven source/native mappings and prices
after correcting the clock layer and import-time wording. A separate review
passed the dependency resolver, independent source guard, four control scopes
and preservation of both policy sections. Neither reviewer ran Cargo or tests.

Not run: full crate/monorepo suites, all-feature compilation, separate wasm build
and runtime/FFI/browser job steps, real venue/network/CSPRNG-host checks, Node26,
new oracle isolation, canonical parity-check binary or CI reruns. No complete
function-conformance or canonical diagnostic reduction is claimed.
