65 pending rows moved: **13 ported, 52 divergent**. Nine-file triple (ported/divergent/pending): **24/6/65 → 37/58/0**, 95 symbols. Counts use `tomllib` and the independently supplied 65-row assignment; see `movement.json`. The census stops after file 9 and is separate from proof of behavior.

Measured code and static review SHA: `d6bcceefccf09b7b5a9e5ed1fdd28ebf89ad0531`.
Initial branch parent: `d3f2bb3940811e315aa9e4ab9d970f6b3c08783a`.
Integration merged down from `55d8addaf74e5ff2c2da80ab19a03109b6fb9476` in `3b08f3c8a6b546c37c362744cefef78a5431e61e`.
Incoming ancestry is from integration; no extra side-branch work was introduced. This branch's changes received the sub-agent review in `review.md`; no human review claim.

The implementations cover WS redaction, bounded REST diagnostics, the complete exchange metadata service, contextual/module token management, a callable loader cache, public-option construction and account-option conversion. The factory takes real host-installed loaders/constructors; it does not automatically construct the descriptor-only venue registry. Divergence rows name the typed host, immutable snapshot, UTF-8 diagnostic and scheduling choices. Two inherited build/lint defects were fixed with a missing fixture clock method and a private callback alias; inherited Rust-parity noise is untouched.

Measured gates at the code SHA (exact commands in `gates.json`, outputs in matching logs):

| Gate | Exit |
| --- | --- |
| check-safe-cex-types | 0 |
| check-safe-cex-store | 0 |
| check-safe-cex-exchange | 0 |
| check-safe-cex | 0 |
| host-clippy | 0 |
| test-diagnostics | 0 |
| test-exchange_metadata | 0 |
| test-access_token | 0 |
| test-root_services | 0 |
| test-production_store_wiring | 0 |
| extractor | 0 |
| metadata-freshness | 0 |
| diagnostics-source | 0 |
| token-source | 0 |
| factory-account-source | 0 |
| census | 0 |
| rust-parity-existing-binary | 1 |

The four checks use `--all-targets --jobs 2`; strict host clippy scopes the same four crates. These are scoped claims, not workspace compilation. The five test commands execute eight tests total: diagnostics 3, metadata 1, token 1, factory/account 2, production store wiring 1. Native field/identity/bit assertions precede any Rust wire assertions. Metadata expectations use original live source keys for 47 records, 86 metadata fields plus five feature fields, and the unified Bybit variant; this enumeration is a floor, not arbitrary-input or equal-valued-swap completeness.

Extractor output was read: 7341 symbols, 512 files, **would update 0 / added 0 / orphaned 0**. The existing Rust-parity binary (not rebuilt this turn) exits 1 before status/invention enumeration at inherited `safe-cex-webworker/src/twap_manager.rs`'s module-level `thread_local!`. It is also present at merge-down parent 55d8addaf7. This run does not enumerate or certify the previously reported 24 parent diagnostics, or prove a global Rust parity pass.

All **eight** deliberately broken production controls exit 101 at their named runtime assertions and restore to exit 0; see `controls.json` and paired logs. The symmetric requiresCors/forbidCors serializer/decoder swap fails the independent native GRVT assertion. `restoration.json` verifies restored source bytes against `git show` at the measured SHA. Enumeration stops at control 8; no survivors among these eight executed controls. The initial signing control failed to compile because it made sign_ms unused; that failed harness is retained under `attempt-controls-271bb352` and is not counted as a caught runtime mutation. The corrected control uses sign_ms + 1.0.

Earlier attempts have their own SHA-bound subdirectories and are not final gate claims. The final follow-up static review found no unresolved significant findings after correcting direct metadata field proof, mobile snapshot classification, and signed-zero extrema.

Not run: workspace/repo-wide builds, full monorepo or full crate suites, wasm clippy/runtime, CI reruns, live venue networking, binding freshness, browser token runtime tests or a complete mutation census. Callback reentrancy and equal-valued schema swaps remain coverage limits. No PR was opened and no merge into integration was performed.

The delivery commit adds only this evidence directory to the measured code SHA; gate numbers above belong to the measured SHA, not to the documentation commit.
