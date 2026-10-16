Moved **9 pending rows: 4 ported, 5 divergent**, measured independently with
Python 3.11 tomllib across 8 ASTERDEX manifests (231 rows). Starting parent:
`9c663a908f40c2955e27b1ce58f97fb5c08fb331`. Before 126/87/18, after 130/92/9
(ported/divergent/pending). normalizeAddress stays divergent but now names the
complete native wrapper. The five new divergent rows have distinct canonical
notes and distinct real Rust targets.

Implementation/review/gate input SHA:
`f820e03b2ca4c673aa90e1df3a60436a8670ad3a`.
Full argv, exits and outputs: PUBLIC_ASSEMBLY_EVIDENCE.json.

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p
  safe-cex-venue-asterdex --all-targets --jobs 2`: **0**.
- Same crate, `cargo test --all-targets --jobs 2 -- --include-ignored
  --test-threads=2`, with SAFE_CEX_TYPESCRIPT_PATH supplied: **0**, 172 passed,
  0 failed, 0 ignored across 21 harnesses.
- Extractor: **0**; output says “Would update 0 manifests; 0 added, 0 orphaned
  symbols.” Official serializer/writer ran after manifest edits.
- Independent nine-row census: **0**, iteration 1 stopped at 8 files/231 rows.
- Six compiled controls: each named assertion **101**, exact restore, named
  test **0**. Control runner **0**, iteration 1 stopped after family 6; zero
  survivors. Production SHA256 and source/test/script/manifest diff after
  restoration match the input commit exactly (diff **0**).

| Family | Named assertion rejected by compiled mutation |
| --- | --- |
| Assembly | construct-open-close native endpoint |
| Dispatch | message-numeric-pong native numeric pong guard |
| Delegates | delegate-cleanup native topics |
| Chain switch | switch-ok native wallet request fields |
| Builder check | check-approved native builder check result |
| Wallet approval | approve-success native wallet approval result |

Controls with full failed/restored output: PUBLIC_ASSEMBLY_CONTROLS.json.
Review and development findings: PUBLIC_ASSEMBLY_REVIEW.md. Source/native case
iteration 1 stops at 16 public cases (53 operations, 69 snapshots) and 36 builder
cases. All native model/result/request assertions precede deferred raw wire
comparisons across each family. These are behavioral floors, not an exhaustive
proof over all inputs. No Value/QueryValue conversion shim was introduced;
builder signing uses four generated strings directly.

Not run: workspace compilation, safe-cex-exchange check (shared code was not
edited), host clippy, wasm check/clippy, full monorepo suites, CI reruns, or real
network/browser-wallet operations. The scoped check is not a workspace claim.

Merge-down/push preservation will be recorded separately against the actual
merge SHA. This sheet's numbers remain bound to the implementation SHA above.

Merged integration down at `c3d5b6c346609d61108520c47b778c4a2a3f462f` (merge exit **0**),
actual integration parent `2a0e1813663e4b5bb627bfde9db765cd958c5d84`;
150 integration commits after the starting parent. The first fetch raced a
shared ref (exit **1**), retry succeeded (**0**); no history was rewritten.
Source/tests, extended inputs and reachable lock graph each compare unchanged
(**0**) to the implementation SHA. Cargo/control gates were not rerun after
this unchanged-input merge. Extractor (**0**, green text) and census (**0**)
were remeasured at the merge SHA. See PUBLIC_ASSEMBLY_MERGE_EVIDENCE.json.
