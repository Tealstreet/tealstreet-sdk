# Shared-core behavior gate exits

This is authored evidence. Results below attach to the named measured commits,
not automatically to later documentation or integration merges. Temporary controls
are separately described against their committed base and were restored bytewise.
No generated logs, source captures or host paths are committed.

Parent: `d4e20845a85cc4e2711c99648ef7547f8aa806df` (integration).
No unmerged feature ancestor. CrossTrade/Store PR7954 remains separate.

All Cargo invocations use `--manifest-path sdk-rs/Cargo.toml --locked --jobs 2`.
Oracle tests use `SAFE_CEX_TYPESCRIPT_PATH`; Cargo uses `CARGO_INCREMENTAL=0`.
Host capture here is Node24.20.0, arm64. No Node26 execution is claimed for this slice.

## Failing before repair

These are executed assertion failures, not inferred differences or compile failures.

| Measured commit | Scoped cargo test selection | Exit and named assertion |
| --- | --- | --- |
| `0eca4f22f74700631fc0585f3a511ee7f191a290` | candle-cache `--test monthly_behavior` |101; all four original monthly tests fail their native start/next/close/count assertions |
| `98c136bf99393fcfff125c26ad4d595df0f04551` | registry `--test public_data_providers retains_literal_info_object` |101; declaration and provider literal-object-kind assertions both fail |
| `5a2a96a2c735c8fb1b9451edb245df683046c3bd` | store `--test redaction_behavior` |101; REDACTOR native own-key count/0 |
| `3d6b17c44a59f21db210c5ffbcaadfad2bdd0c1f` | exchange+transport `--lib behavior_native --no-fail-fast` |101; SOCKET REDACTOR native own-key omission and HOW FORM native decoded data |
| `18ca9e1893880cc3868e12625fe70ab221412709` | transport `--test tunnel disposed_how_message` |101; HOW DISPOSE native postdispose callback count |
| `32d8d8ee906d63c2761c9cac5ddb98f0d72c7190` | venue-okx `--test journal_numbers` |101; OKX JOURNAL native numeric kind retained |
| `fbf65b07984a7f9bb33bf2a8cfe4e095f3ec7584` | webworker `--test handlers offline_recovery_` |101; RECOVERY native inner and outer command admitted |
| `0e5629d31a729b291305026813e551d6962be9cd` | candle-cache `--test monthly_behavior monthly_negative_year` |101; MONTHLY native negative-year helper rollover |

The disposed-message gate launched at the listed commit; a journal-test-only
commit landed while its already-built binary waited for process startup. The
worker baseline gate launched at the listed commit; only an OKX test expectation
was corrected during that run. Neither later edit changes the compiled target.

## Passing restoration and supplements

| Measured commit | Selection | Measured exit |
| --- | --- | --- |
| `2dd041136b264b7eede40c68759e5e28d0a04341` | candle-cache monthly_behavior and divergence_audit, `--include-ignored` |0; four new monthly tests plus the updated convergence fixture |
| `fcc2e2439bd69094e2ccf39e4d9d32d3f4ef6aed` | registry public_data_providers retains_literal_info_object |0; two tests |
| `a32f3998b1e348dbe7e0218d6962dde0ae1edc81` | webworker handlers offline_recovery_ |0; two admission tests |
| `ac0b430e0ed57b0c56bd7a94f000dac366856242` | webworker handlers |0; eleven tests including native false-policy and hook-error cases |
| `ac0b430e0ed57b0c56bd7a94f000dac366856242` | candle-cache monthly_behavior, divergence_audit, p4_calendar `--include-ignored` |0; five monthly tests, convergence fixture and maintained calendar oracle |
| `ac0b430e0ed57b0c56bd7a94f000dac366856242` | store redaction_behavior |0; per-value prototype/depth native assertions precede each public-text check; all-native-before-all-wire ordering is measured separately below |
| `ac0b430e0ed57b0c56bd7a94f000dac366856242` | exchange+transport `--lib behavior_native --no-fail-fast` |0; native socket diagnostic and form-fallback tests |
| `ac0b430e0ed57b0c56bd7a94f000dac366856242` | transport tunnel disposed_how_message |0; one test |
| `ac0b430e0ed57b0c56bd7a94f000dac366856242` | venue-okx journal_numbers, position_adapter, field_identity `--include-ignored` |0; five tests |
| `55f022ef494cd6f530c25b80cee4d75070ab9dea` | node call_method_oracle.mjs |0; eighteen-command inventory, six recovery handler outcomes and actual BaseExchange all-socket order |

The five later multi-target gates launched serially at `ac0b430e0ed57b0c56bd7a94f000dac366856242`; subsequent
commits during those runs changed worker tests/manifests/docs only. They are not
reported as full-tree gates at those later commits. Journal's prior projection
corpus is wire evidence; native Infinity/NaN/negative-zero assertions are supplied
separately and run before that test's source witness.

Calendar stops at38native/37source observations (different helper-route floors),
including the negative-year supplement. Redactor stops at twelve utility cases
and four public projections, plus one enabled socket send/receive sequence.
HOW form stops at eight literal cases and disposal at one active/one disposed
entry. Worker source supplement stops at six inner/outer outcomes and one base
false/true socket sequence. These are floors, not completeness claims.

## Deliberate controls and delivery gates

At this earlier scoped stage, no workspace build was claimed. The later dedicated
workspace build is recorded below. No workspace host clippy, full wasm matrix, all-feature
compilation, full monorepo suite, CI rerun or real-venue networking is claimed
by this slice. Scoped host and wasm claims remain separate.

Fourteen deliberate controls completed: each compiled, failed its named native
assertion with exit101, then passed the same target after bytewise restoration
with exit0. No survivor was dropped. Commands use the same common Cargo flags
as above. The source/test base is7e974396a9bfd1dce6b62050de9741383628e8b0.
Controls1–8 launched at that SHA. Controls9–14 launched at
ecb27b28b58413e94588dabd33795a14afd59e65. The core-redactor restoration
launched at7e974396a9 and finished after that later commit; its compiled target
closure is unchanged. The later commit changes manifests and reorders only the
Store integration test after its control had already finished. No claim that a
mutated run validates an unmodified whole commit is made.

| Iteration | Mutation | Package / target selection | Named failing assertion | Mutated / restored exits |
| --- | --- | --- | --- | --- |
| 1 | monthly start returns0 | `safe-cex-candle-cache --test monthly_behavior monthly_start` | MONTHLY native chartCandleStartTime/0 |101 /0 |
| 2 | monthly next returns0 | `safe-cex-candle-cache --test monthly_behavior monthly_next` | MONTHLY native nextChartCandleStartTime/0 |101 /0 |
| 3 | monthly close returns0 | `safe-cex-candle-cache --test monthly_behavior monthly_close` | MONTHLY native chartCandleCloseTime/0 |101 /0 |
| 4 | monthly count returns0 | `safe-cex-candle-cache --test monthly_behavior monthly_count` | MONTHLY native count/0 |101 /0 |
| 5 | declaration uses ordinary Value capture | `safe-cex-registry --test public_data_providers gateway_declaration_retains_literal` | GATEWAY declaration literal object kind |101 /0 |
| 6 | provider uses ordinary Value capture | `safe-cex-registry --test public_data_providers gateway_provider_retains_literal` | GATEWAY provider literal object kind |101 /0 |
| 7 | utility keeps __proto__ own key | `safe-cex-store --test redaction_behavior` | REDACTOR native own-key count/0 |101 /0 |
| 8 | private socket keeps __proto__ own key | `safe-cex-exchange --lib behavior_native_socket_redactor` | SOCKET REDACTOR native own-key omission |101 /0 |
| 9 | form fallback keeps __proto__ own key | `safe-cex-transport --lib behavior_native_form_fallback` | HOW FORM native decoded data |101 /0 |
| 10 | disposed message guard disabled | `safe-cex-transport --test tunnel disposed_how_message` | HOW DISPOSE native postdispose callback count |101 /0 |
| 11 | computed nonfinite number replaced with Null | `safe-cex-venue-okx --test journal_numbers` | OKX JOURNAL native numeric kind retained |101 /0 |
| 12 | recovery command removed from admission list | `safe-cex-webworker --test handlers worker_route_is_admitted` | RECOVERY native inner command admitted; RECOVERY native outer command admitted |101 /0 |
| 13 | socket protocol flag ignored | `safe-cex-webworker --test handlers offline_recovery_visits_every_socket` | RECOVERY native false policy result |101 /0 |
| 14 | year remapping moved after rollover | `safe-cex-candle-cache --test monthly_behavior monthly_negative_year` | MONTHLY native negative-year helper rollover |101 /0 |

Enumeration stops at control14. The twelve historical rows are covered by
controls1–7 and9–12 (control12 independently fails both worker entry tests);
controls8,13,14 protect coupled/newly exposed paths.


## Delivery measurements at merge commit56615144aabbfb180bb383680697fad3c01d458c

This commit merges integration694e07f7e049a5a7047c41e994a0fc854dc78a59 into
this branch. It retains the original d4e20845a85cc4e2711c99648ef7547f8aa806df
base. No unmerged feature branch is an ancestor. No TypeScript implementation
file changes. The later evidence commit changes documentation only; it does not
extend these gate claims to unmeasured code.

For the following check and lint commands, SELECTED means exactly:
`-p safe-cex-types -p safe-cex-candle-cache -p safe-cex-store -p safe-cex-exchange -p safe-cex-registry -p safe-cex-transport -p safe-cex-webworker -p safe-cex-venue-okx`.
All use `--manifest-path sdk-rs/Cargo.toml --locked --jobs 2`.
Wasm selection follows the eight entries' empty feature lists in
`.github/rust-wasm.json`; it is not the full workspace matrix.

| Gate | Actual measured exit |
| --- | --- |
| cargo check SELECTED --all-targets |0 |
| cargo clippy SELECTED --all-targets -- -D warnings (host) |0 |
| cargo clippy SELECTED --all-targets --target wasm32-unknown-unknown -- -D warnings |0 |
| candle-cache tests monthly_behavior, divergence_audit, p4_calendar --include-ignored |0; seven tests |
| registry test public_data_providers |0; seven tests |
| exchange+transport --lib behavior_native --no-fail-fast |0; two tests |
| transport test tunnel disposed_how_message |0; one test |
| venue-okx tests journal_numbers, position_adapter, field_identity --include-ignored |0; five tests |
| webworker tests handlers, store_notification |0; thirteen tests |
| store test redaction_behavior after all-native-before-all-wire reordering |0; one test |
| repeated utility-redactor own-key control against the reordered test |101 named REDACTOR native own-key count/0, then0 after restoration |
| parity:extract:check via the explicit YarnJS entry |0; output: Would update0 manifests,0 added,0 orphaned |
| rustfmt --edition2024 --check, only changed Rust files |0; initial loop attempt127 because zsh path variable replaced PATH, corrected to rust_source_file |
| git diff --check against merged integration |0 |
| new added-line host-path scan |0; no host paths introduced |

Extractor writer ran after the last manifest edit and reported Updated0
manifests,0 added,0 orphaned. Both extractor outcomes are based on output text,
not a shim status alone. Writer reconciliation preserved every existing row.

Direct tomllib iteration over512 manifests measures7341 rows:
4802ported /2228divergent /304pending /7not-applicable. Comparing indexed
row identities/statuses with integration694e07f7e0 measures zero moved rows.
The census counts both distinct Market declarations in the ApexOmni interface;
it does not collapse same-name rows into a dictionary key.

Source isolation copies tracked implementation/test/helper files outside the
repository with no node_modules in its ancestors. Eleven actual oracle
invocations pass with an existing isolated TypeScript6.0.3 compiler:
five calendar operations, utility redactor, gateway provider, HOW form,
HOW disposal, journal and worker. Initial archive omitted the maintained logging
package and failed loudly with ENOENT; adding that tracked dependency gives11/11
exit0. A nonexistent compiler path gives exit1 with MODULE_NOT_FOUND, not a skip.
No dependency installation was performed or claimed here.

Independent sub-agent reviews inspected calendar/redactor, gateway/journal and
worker/HOW scopes. They found and prompted the negative-year helper correction,
source false/error recovery witnesses, stale carrier/notification notes, bucket
corrections and historical native/wire ordering clarification. Production scopes
passed after correction. Reviewers did not claim rerunning root's Cargo gates.

Not run: workspace host clippy, full wasm matrix, all-feature
host/wasm compilation, full crate/monorepo test suites, Node26, real venue/API
networking, browser execution, CI reruns. No gate was suppressed. Prior
CrossTrade/Store PR7954 is separate, remains mergeable when inspected, and is not
claimed as rebuilt by these measurements.


## Requested workspace build

Measured commit: `d674c0e70d4244a890a3db73d9e3d23e968b51d8`.

From the tracked sdk-rs directory, ran exactly:

```sh
cargo build --workspace --locked
```

Environment: `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`.
Actual process exit: **0**. Cargo reported `Finished dev profile` in1m17s;
output contains zero warning lines. No code, dependency or manifest edits were
needed. This supersedes the initial delivery's workspace-build NOT RUN entry.
The evidence update changes this document only; all workspace build inputs remain
byte-identical to the measured commit.

This build covers PR7970's workspace tree. Separate PR7954 is not an ancestor;
no combined-merge build is claimed. The earlier host/wasm lint scopes and their
measured commits remain as recorded above.
