# Bitrue audit remediation gates

Current integration validation is recorded in [WORKSPACE_GATES.md](WORKSPACE_GATES.md) at7b4b2980f9. It supersedes the historical NOT RUN list for the requested workspace/wasm commands without attributing them to9f. Incoming integration annotations changed7 already-ported rows outside the frozen divergent cohort; historical manifest comparisons remain bound to694→9f, and current comparisons use49a→7b.

All clean gates below were executed at **`9f1be06793a067d64a079a9dbfebae84774cd01a`**. Later delivery changes add evidence/reporting only; this sheet does not silently attach earlier executions to a newer SHA. Initial parent was `694e07f7e049a5a7047c41e994a0fc854dc78a59`; merge-down `e98c0fd218` incorporated integration `8fcf323edfc6747fcdf4427ce19267a7147d018b`. Source/tests identity against lint-fixed `db68c19363` exited0. No unmerged Crypto.com ancestor remains after that merge-down.

| Gate at9f1be06793 | Exit | Measured scope |
| --- | ---: | --- |
| cargo fmt |0|Bitrue package only, --check|
| cargo check |0|Bitrue --all-targets --locked --jobs 2|
| cargo test |0|Bitrue --all-targets --locked --jobs 1 -- --test-threads=1;60passed/0failed/0ignored|
| host cargo clippy |0|Bitrue --all-targets --locked --jobs 2|
| wasm cargo clippy |0|Bitrue --all-targets --locked --target wasm32-unknown-unknown --jobs 2|
| parity:extract:check |0|actual Yarn command; output: Would update0 manifests,0added/0orphaned|

Cargo used RUSTFLAGS=-Dwarnings. Actual-source tests used SAFE_CEX_TYPESCRIPT_PATH supplied externally. For Cargo records, full argv, environment description, durations, logs and hashes are in [gates.json](evidence/audit-remediation/9f1be06793/gates.json). The annotation writer ran before commit9f, with the reconciled manifest blobs subsequently committed there; it exited0 and printed Updated0/added0/orphaned0. This is distinct from the clean9f extractor check. [Operation states](evidence/audit-remediation/9f1be06793/annotation-operations.json) preserve that distinction.

## Runtime controls

Each mutated run is **base9f plus its retained patch**, not a clean9f execution. Each compiled, entered one named native test, failed the indicated assertion with exit101, restored the exact original source bytes, and then passed with exit0. No compiler-blocked control or survivor was observed in this nine-control floor, stopping index8 native-key. See [controls](evidence/audit-remediation/9f1be06793/controls.json), [patch hashes](evidence/audit-remediation/9f1be06793/mutation-provenance.json), [portable runner](evidence/audit-remediation/control-runner.py) and [receipt verifier](evidence/audit-remediation/verify-receipts.py).

| Control | Named runtime assertion | Mutated | Restored |
| --- | --- | ---: | ---: |
|delay lower leverage GET|ENTRY-sync|101|0|
|delay fetchSymbolMaxLeverage subcall|ENTRY-leverage|101|0|
|delay updateSymbolMetadata subcall|ENTRY-update|101|0|
|delay raw account GET|ENTRY-raw|101|0|
|delay fetchBalance subcall|ENTRY-balance|101|0|
|delay cold metadata fetch|ENTRY-position|101|0|
|omit eager cache update|ENTRY-CACHE-WRITE|101|0|
|suppress immediate raw error emission|ENTRY-SYNC-ERROR-EMIT|101|0|
|force wrong ordinary native contract key|PRICE-NATIVE-SCALAR-KEY|101|0|

Cache omission is an assertion-discrimination control, not a claim that it reproduces the exact old deferred implementation. The last control checks a decoded ordinary native scalar result before invoking source; replacement strings are explicitly different inputs illustrating representation loss. See [behaviour scope](AUDIT_REMEDIATION.md#behaviour-confidence) for the17 executable rows and8 erased declarations separately.

## Canonical diagnostics

Direct `parity-check`, `parity-report` and `parity-progress` each exited **1**, with **5897 diagnostics**; all three multisets match. The same measured9f code with the exact integration8fc manifest snapshot also produced5897. Revised Bitrue annotations added0 and removed0 diagnostics. The indexer completed. The historical5895 baseline has two additional inherited shared-core inventions: `safe-cex-exchange/src/exchange.rs::Exchange::recover_after_offline` and `safe-cex-exchange/src/websocket.rs::WsProtocol::recovers_after_offline`. They are not suppressed or fixed in this venue slice. Bitrue contributes107 diagnostics in each measured multiset; this is a census, not evidence of107 equivalent or defective behaviours.

The binaries were built at `d4e20845a85cc4e2711c99648ef7547f8aa806df`; hashes match the retained Crypto receipt. Git source/dependency identity for all three entry binaries, library modules, parity/workspace Cargo manifests and Cargo.lock against9f exited0. This is direct-binary execution with verified provenance, not a fresh Cargo build. [Before](evidence/audit-remediation/9f1be06793/canonical-before.json), [after](evidence/audit-remediation/9f1be06793/canonical-after.json) and [baseline delta](evidence/audit-remediation/9f1be06793/canonical-baseline-delta.json) include exact commands, hashes, classifications and final diagnostics. The exact common [diagnostic body](evidence/audit-remediation/9f1be06793/canonical-diagnostics.log.gz) is retained compressed; adding each receipt's optional wrapper reconstructs all six raw hashes, as checked in [reconstruction](evidence/audit-remediation/9f1be06793/canonical-reconstruction.json). The reference-manifest fingerprint floor contains513 archived files from8fc; no production manifest was regenerated.

## Earlier attempts and limits

At `bac1f58b63cb34388fe79598ff863abe9412698c`, fmt exited0 and scoped check101: the new boundary fixture called Store's inherent update instead of its trait method (E0061). At `e8c375228bcc78328a63d26cf58e37762061b44a`, fmt/check/tests exited0 (60tests), then host clippy101 for Option early-return style. That queue stopped: wasm clippy and controls did **not** run there. The one-line correction is `db68c19363`. Both partial receipts remain alongside9f. Earlier dirty-tree entry probes had E0560 boxed-error literal failures and later native/source preflight output; their exact source snapshots were not retained. Those logs are explicitly uncommitted preflights, not SHA-bound runtime mutation gates.

A rejected overlapping Git add/commit was retried after the active commit finished; no lock was removed and no history rewritten. **Development Cargo-overlap deviation:** the older Crypto.com development sequence overlapped a Cargo-spawning CI-wiring import and a Cargo test. This inherited deviation is disclosed on PR7973 itself, as on the merged Crypto PRs. The present queue runs one Cargo/Cargo-spawning operation at a time. Checker executions and TypeScript extraction do not spawn Cargo.

**Not run at9f:** workspace fmt, workspace check/build/clippy, wasm build, wasm execution, full monorepo suites, include-ignored tests, remote CI or CI reruns. Scoped wasm clippy is not wasm execution. Cancellation, autonomous Promise progress, full exchange-engine scheduling, every immediate-start error combination and exhaustive input equivalence remain unproved. Read-only sub-agent reviews have their own bounded scope and are not independent Cargo executions. No human review gate or self-merge occurred.
