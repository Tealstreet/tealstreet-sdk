PR 7476 conflict resolution and changed-input gates.

Measured merge SHA: `2fcd332be03deec4f9604e9d224dbeeb3131bab6`. Integration parent: `3e4e053b47fbfa44475712b68fdf53c23f5071cd`; previous branch parent: `3b28ea0bc212222cfd25d5a979b40203a74e7d3d`.

Only conflict: equivalent private callback aliases LegResolver/ResolveLeg in safe-cex-exchange/src/position_intervals.rs. Kept the exact previously gated file; no incoming behavior was removed. No CLAUDE.md conflicts occurred; incoming paragraphs were retained automatically.

Owned implementation source is byte-unchanged from gate SHA `d6bcceefccf09b7b5a9e5ed1fdd28ebf89ad0531`: scoped `git diff --exit-code` exits 0 on the explicit 14 paths in source-identity.json, stopping after path14. The full four-crate source diff exits 1 because integration adds Store balance retention/balance_handle and has_websocket; numeric and crypto dependency source also changed. This is not a claim that entire crates were byte-unchanged.

All changed-input local gates were rerun serially at the merge SHA: one cargo check scopes all four affected crates with all-targets/jobs2 (exit0), strict host clippy same scope (exit0), five named test targets/eight tests (exit0 each), extractor (exit0; output read:0 updates/added/orphaned), metadata freshness and three live TS source oracles (exit0), assigned nine-file census (exit0). Exact commands/exits are in gates.json and outputs in named logs. This is scoped checking, not workspace compilation.

Eight production mutation controls failed their named runtime assertions (exit101 each), were restored, then passed (exit0 each). Enumeration stops after control8; no survivors within this floor. Source restoration is independently compared against git show at the merge SHA. Earlier controls remain separately bound to their original SHAs.

The existing Rust-parity binary exits1 at the inherited TWAP thread_local module macro before enumeration. The previous24 parent diagnostics were not re-enumerated; no global Rust-parity pass is claimed. No parent parity errors were repaired. Supplemental sub-agent review at this exact merge SHA found no issues; reviewer ran no gates.

Not run: full workspace/monorepo/crate suites, wasm clippy/runtime, CI reruns, live venue networking, binding freshness or exhaustive mutations. Browser-token/reentrancy/equal-valued-schema-swap limits remain. The evidence-only delivery commit adds this directory; measured exits bind the merge SHA, not that documentation commit.
