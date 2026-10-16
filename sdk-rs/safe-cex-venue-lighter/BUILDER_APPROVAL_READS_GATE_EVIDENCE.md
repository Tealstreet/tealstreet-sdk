# Lighter builder approval reads

Parent actually built on: `039d308cd99cfd09e22205fb1083c3df5a306548`, the requested integration head. The branch starts there; no unreviewed feature ancestor was added. Implementation commit: `6e017efb517ca932bb5713991a38d75dc0337661`. Mutation-driver correction: `89a274bef0bfd083f105f589ab9a83a9b84d9cdd`.

## Rows and boundary

Independent Python `tomllib` comparison against the parent moved exactly **9 pending rows: 8 ported, 1 divergent**. The five owned source-file manifests contain 155 rows, with `(ported, divergent, pending)` changing from `(84, 5, 66)` to `(92, 6, 57)`. Builder approval alone changes from `(13, 1, 14)` to `(21, 2, 5)`, total 28. No new rows or removals. Enumeration stops after these five owned manifests and nine changed rows.

Ported: `resolveApprovalFeeCaps`, `createApprovalClient`, `ensureWasmReady`, `createApprovalTransactionClient`, `fetchAccountLimitsForApproval`, `resolveAccountIndex`, `fetchAccountSnapshot`, `getNextNonce`. Divergent: `assertPartnerAttributionTierEligibleForApproval`.

Every changed row points to its actual public function in `src/builder_approval_reads.rs` and the public `LighterApprovalReadRuntime` capability record. All nine function declarations and the runtime declaration were independently checked. The unchanged canonical checker was also invoked; its separate failure is recorded below, not replaced by this declaration census.

The divergent row's own note says **CHOSEN**: unexpected best-effort tier lookup errors invoke the required visible-warning capability with the original error. TypeScript silently discards those errors. The undefined fallback and exact-message Error rethrow remain. This follows the active visible-failure requirement, rather than claiming an unavoidable Rust limitation.

This branch authors only `lighter.builder-approval.ts.toml`, its Rust module/export, test/oracle, proof drivers and this sheet. It does not author `lighter.exchange.ts`, its manifest, SDK source or SDK manifests. Existing SDK promise/error records and helpers are imported dependencies. The canonical scope includes SDK manifests read-only because this venue crate already contains SDK modules.

## Measured gates

All cargo invocations were sequential and used at most two jobs. `CARGO` denotes `/opt/homebrew/opt/rustup/bin/cargo`; `YARN` denotes `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`. Native test invocations set `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Measured SHA | Command | Observed result |
| --- | --- | --- |
| `6e017efb517ca932bb5713991a38d75dc0337661` | `CARGO check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2` | exit 0; Finished dev profile |
| `6e017efb517ca932bb5713991a38d75dc0337661` | `YARN parity:extract:check` | exit 0; 7341 symbols, 512 files; would update 0 manifests, 0 added, 0 orphaned |
| `89a274bef0bfd083f105f589ab9a83a9b84d9cdd` | `CARGO test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_builder_approval_reads` | exit 0; 5 passed, 0 failed, 0 ignored |
| `89a274bef0bfd083f105f589ab9a83a9b84d9cdd` | `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-lighter/scripts/builder-approval-reads-parity.py` | exit 1; canonical checker stopped at existing `transaction_builders.rs` imported-inherent-owner diagnostic |

The check/extractor started on the implementation SHA; the subsequent commit changes only the mutation driver. `git diff --exit-code` between those two SHAs over venue `src/`, `tests/`, all parity manifests and SDK Cargo files exited 0. Their measured SHA remains the implementation SHA.

The canonical finding is at the existing `impl TransactionBuildError` in `transaction_builders.rs`: its two `use super::...::*` statements trigger the checker's imported-owner rejection despite the same file declaring `pub struct TransactionBuildError`. `imports_name(UseTree::Glob)` returns true without prioritizing that declaration. `git diff --exit-code` from the integration parent to the measured commit for both this source and the checker exited 0; read-only review independently confirmed the scope links and byte identity. No SDK source, exemptions or checker rules were altered. Zero Lighter exemption blocks were selected. This invocation did not finish a successful canonical census.

## Native assertions and proof limits

The oracle executes the unmodified complete TypeScript builder-approval module and actual imported source domain/tier/fee/utility helpers. SDK constructors, loader and crypto API are opaque controlled dependencies. Native tests decode source expectations into native values, then assert actual outcomes, initialization receipts, native result/error, constructor arguments, request arguments, warning receipts and original-handle identity before trace assertions. No decode/re-encode comparison is used.

The independently fixed source-case floors are 10 fee-cap, 4 HTTP-client, 3 WASM readiness, 4 transaction-client, 9 limits, 10 tier, 11 index resolution, 10 snapshot and 4 nonce cases: **65**, stopping at this corpus. Native groups assert floors 10, 11, 19 and 25. The fifth test separately asserts the first-await construction boundary and completion after dropping the NativePromise observer. The host task engine retains the owned frame through settlement; no borrow guard is held across polling.

Witnesses include six changing accepted fee getters, explicit undefined/extra fee keys, negative zero and nonfinite numbers, huge BigInt IDs, zero/negative lookup IDs, first-row precedence and sparse holes, distinct four fee values, post-await index rereads, initialization receipts, original client/crypto/error identities, and synchronous call throws separately from rejected imported promises. The error/result outcome assertion runs before unwrap, so an accidental error remains a named native failure.

General JavaScript microtask scheduling, exotic getters/iterators, array species and host object implementations are not exhaustively proved. Those are explicit source-contract primitives, as are imported SDK construction, loader and crypto functions. No live HTTP, wasm loader, signing, wallet or production SDK integration gate was run; these nine helper bodies perform no replacement crypto implementation. The higher-level approval preparation/signing/submission rows remain pending.

Read-only sub-agent review found the initial eager-poll extra resumption and warning mutation's denied-reference-drop issue; both were corrected before frozen controls. Review rechecked the native initialization/outcome assertions, original warning error, ownership boundary and nine real targets. The reviewer ran no builds, tests or mutations. Precommit fixture failures (String representation and a mock TransactionClient own-property mismatch) were corrected before the measured passing SHA; they are not presented as frozen mutation proofs.

Full workspace compilation, full venue suites, host clippy, wasm clippy and CI were not run.

## Compiled mutation controls

`/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-lighter/scripts/builder-approval-reads-mutations.py` completed with exit **0**, measured on `89a274bef0bfd083f105f589ab9a83a9b84d9cdd`. Each control changed the real Rust body, compiled it, reached the named native assertion with exit 101, restored the original source bytes, then ran its exact test with exit 0 and 1 passed. A compile error or failed trace alone cannot satisfy the driver. Enumeration stopped after these five controls, with no survivors in that floor.

| Family / regression | Exact scoped test | Named failing assertion | Broken / restored exits |
| --- | --- | --- | --- |
| Fee caps: spread `feeValue` instead of `feeCaps` | `approval_reads_caps` | `approval_reads::native result "caps-missing"` | 101 / 0 |
| Clients/loader: reverse readiness guard | `approval_reads_clients` | `approval_reads::native init receipts "wasm-false"` | 101 / 0 |
| Limits/tier: add 61 seconds instead of 60 | `approval_reads_limits_tier` | `approval_reads::native requests "limits-ready"` | 101 / 0 |
| CHOSEN tier warning: drop error without notifying sink | `approval_reads_limits_tier` | `approval_reads::native tier fallback warning "tier-best-effort"` | 101 / 0 |
| Account reads: maker fee reads taker field | `approval_reads_accounts` | `approval_reads::native result "snapshot-full"` | 101 / 0 |

The driver uses `CARGO test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_builder_approval_reads -- <test> --exact`. Raw logs and receipts live under ignored `sdk-rs/target/lighter-approval-mutations/`. Final source-byte restoration and `git diff --exit-code HEAD -- sdk-rs/safe-cex-venue-lighter/src/builder_approval_reads.rs` both passed. This sheet is authored evidence; generated receipts are not committed.

## Integration merge and preservation

Merged fetched integration `4afb5dbf48e5f071608ee23c74e367a107695439` down in merge commit `74ba3d2e0e8d02d7818bcbea160e25f71e295105`. The sole conflict was the end of Lighter `src/lib.rs`; resolution keeps both `builder_approval_reads` and incoming `exchange_account_access` exports. No CLAUDE conflict occurred. No merge to integration or PR creation was performed.

On merge SHA `74ba3d2e0e8d02d7818bcbea160e25f71e295105`, the same scoped cargo check exited **0**, the same scoped native test exited **0** (5 passed, 0 failed, 0 ignored), and extractor check exited **0** with output `Would update 0 manifests; 0 added, 0 orphaned symbols`. These are new measurements after incoming workspace/Cargo and shared exchange/transport changes; earlier gates are not claimed to compile that graph.

`git diff --exit-code 89a274bef0bfd083f105f589ab9a83a9b84d9cdd 74ba3d2e0e8d02d7818bcbea160e25f71e295105 -- <named inputs>` exited **0**. The named input floor comprises this approval-read module, native test, oracle and mutation driver, its owned builder-approval manifest; existing native builder approval, utils, deployment, account tier, types, transaction promise/signing/builders modules; all tracked `safe-cex-num` inputs; and TypeScript domain types plus builder approval, types, utils, builder fee and account tier source files. Thus the five frozen mutation controls retain their measured source/test/dependency inputs. Enumeration stops at these named inputs; this is not a claim that the whole crate or workspace stayed unchanged. Incoming Lighter files are the account-access module, test and oracle plus the preserved root export.

Commit `2a663e46c9e23e63eb8aafed9b9cc42c2df7aafd` updates only the optional canonical scope driver to discover members from real read-only mappings, accommodating the incoming standalone `safe-cex-lighter-sdk` crate. Its Python syntax check exited **0**. Scoped native testing started on the merge SHA before this script-only commit; `git diff --exit-code` between these SHAs over venue source/tests, all manifests and SDK Cargo files exited **0**. The updated canonical driver was not rerun; the last measured canonical invocation remains the exit-1 finding at `89a274bef0bfd083f105f589ab9a83a9b84d9cdd`.

After merge, an independent `tomllib` comparison against integration `4afb5dbf48e5f071608ee23c74e367a107695439` again found exactly nine pending moves (eight ported, one divergent), unchanged 155 owned rows, and every changed Rust function/runtime declaration present: exit **0**. Owned `(ported, divergent, pending)` is **(92, 6, 57)**. The entire Lighter venue manifest directory is **(212, 19, 105)**, total 336; other-lane progress is included in that full-venue census but not counted as this slice's gain. `git diff --name-only` against that integration head lists only this owned manifest and seven venue implementation/test/script/evidence paths.

No post-merge mutation repetitions, full venue suite, workspace build, clippy, wasm gate, CI rerun or live signing/network check were performed. The five restored mutation tests passed on their explicitly named earlier SHA, and the new scoped test passed on the merge SHA.
