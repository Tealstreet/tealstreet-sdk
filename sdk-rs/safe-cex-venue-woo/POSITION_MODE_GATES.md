# WOO position-mode evidence and cancel declaration gates

**10 rows moved: 0 pending → ported, 10 → divergent**, measured with tomllib against actual fresh integration parent `dad3db626db479de0168dac660eabc9419bbabaa`. Source gate SHA `90a501622aed281d66825b89d6367372df896b7e`, branch feat/woo-v3-mode-evidence. No unrelated unreviewed feature branch ancestor. Later evidence-only commits do not relabel measured exits.

Fixed expectation: 22 WOO/journal manifests,534rows,ten independently enumerated symbols,10distinct CHOSEN notes,15real primary/supporting Rust targets. Before {'divergent': 290, 'ported': 170, 'pending': 74}; after {'divergent': 300, 'ported': 170, 'pending': 64}. All three assigned v3 files now have zero pending rows: algo-writes, account-writes, cancel-all-after. This is a census, not full WOO parity or live lifecycle readiness.

## Gates at the source SHA

Scoped all-target cargo check0; scoped WOO tests0 (position_mode, account_leverage, cancel_all_after): **3 reported targets/11passed/0failed/0ignored**. Extractor0, output inspected zero updates/additions/orphans. Source oracle0; leaf formatting0; independent tomllib census0. Cargo ran serially with jobs2/offline/locked; oracle env SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Manifest writer ran before source commit and reported Updated2manifests/0added/0orphaned; the committed extractor check verifies its result.

Optional audit with existing prebuilt parity-check binary exited1: **360 inherited diagnostics**, zero in either touched source module. Diagnostic files: legacy_data_reads.rs, legacy_market_reads.rs, legacy_metadata.rs, legacy_order_control.rs, legacy_socket_control.rs, runtime_selector.rs. Example: legacy_metadata.rs::fetch_account_info is absent from declared rust_files and its method rename note omits the exact qualified target; inherited Host helpers/route helpers also lack mappings/exemptions. All implicated legacy sources and relevant manifests are byte-identical to the actual base (inherited diff0). These belong to other lanes; no off-partition fix or audit weakening was made. The binary was not rebuilt; no full-workspace parity success is claimed. Full output is retained in JSON.

## New-family discrimination

Two simultaneous substitutions shared one compiling overlay. Separate --exact test commands each failed with101 at its own named native assertion; exact source bytes restored once in finally, then both exact tests passed0 (1passed/4filtered each). Driver0; scoped working-tree restoration diff0. Stopped at iteration2, zero survivors. These are combined-overlay measurements, not isolated-overlay pairs.

| Family / exact test | Named assertion | Broken/restored exits |
| --- | --- | --- |
| position-mode-order / native_position_mode_builders | MODE native first blocker | 101 / 0 |
| cancel-declarations / native_cancel_declarations | MODE native cancel timestamp | 101 / 0 |

The mode mutation physically moves already-active before margin-account checking; a case with SPOT, an already-active target and exposure/pending orders asserts WHICH blocker came FIRST. The cancel mutation swaps timestamp/expectedTriggerTime Serde labels symmetrically, exposing distinct decoded -0/infinity bits before any serialization. The controls touch disjoint modules/test paths. JSON retains both full patches, call sites, exact commands and per-test failure/restored logs.

## Floors and adaptation boundaries

Five native tests stop at these fixed independent fixture floors: zero27, exposure21, snapshots36, builders40, responses16 (**140 runtime cases**); mode declarations3/refusals11; cancel declarations5/refusals8. Native wrapper fields/source/complete/reconciliation IDs, vector identity/order, existing row identity and all nested numeric fields are inspected directly. Existing inherited string-field mappings are reused rather than claimed as newly exhaustive. Nested numeric fixtures include signed zero and non-finite values, including an algo child; declaration numbers remain unrestricted before runtime safe-integer checks. No decode/re-encode comparison or wire equality appears in the new tests.

Source AST pins full readonly nested records, literals/types/requiredness, optional current mode, both ready/blocker branches and all six blocker members. Source alias/freeze witnesses are separate from native value assertions. Runtime snapshots retain untyped captured rows: typed declaration row decoding cannot preempt the source evidence and blocker sequence. Completeness flags are source attestations, not independently fetched-page proof. Exact-zero uses ASCII string grammar, rejecting exponents/numeric coercion/line terminators. The builder validates target first, catches account/snapshot refusals, reconciles all three IDs, then applies margin/already-active/exposure/regular/algo ordering. Response request guards precede envelope validation.

Each divergent row names its own exact Rust target and reason: captured transport/UTF-8 ownership replaces arbitrary JS host objects/getters and alias identity; public source/complete/accepted/method/path construction is wider than literals enforced at declaration ingress; Ready/Blocked is operation-only with wider public Strings; the ready decoder facade is explicit request/target arguments like leverage. Existing Value-returning cancel operations remain their existing API; declaration codecs coexist separately. No shared type/trait, live route or other-lane source was authored.

Read-only sub-agent review found no concrete blocker; no gates/edits were performed by review. Earlier uncommitted oracle exit1 came from AST textual leading-pipe formatting, fixed with semantic union-member comparison before source commit; it is not a runtime failure attributed to the clean base or committed source.

A read-only compiler sample exited0 during the test build and showed proc-macro dylib loading waiting in dyld/mapSegments/CodeSignatureInFile/fcntl. This is a host-wait observation, not a correctness gate; no host security/build setting was changed.

The broader WOO all-target test build was intentionally stopped after the observed macOS proc-macro loader wait in existing v3_contracts/ws_commands targets. Cargo exit-15 (wrapper241) is retained; no test assertion ran in that attempt and no full-suite success is claimed. The new target and affected existing targets above were measured separately.

Not run: completed full WOO all-target test suite, workspace build/tests/parity audit, monorepo suites/typecheck, HOW-specific tests, host clippy, wasm clippy/build, doctests, CI reruns, live/credentialed lifecycle. No exits claimed for them.

## Merge-down gates

Completed merge `cfc7b266aadb8afb4f2437d3f5a2c843d1aa3138` has actual integration parent `661cd4f26ca6aa8ae09684e52f76897afe1cb456`. The only conflict was the lib.rs export append; all three exports (own v3_position_mode and incoming legacy_fill_recovery/public_parity) were retained. Initial merge1 was resolved with a merge commit0. Source/tests/oracle/control scripts and both owned manifests survived byte-for-byte from source gate SHA90a501622a. Relevant local dependency sources and Cargo manifests were unchanged. Independent lock traversal stopped at481 WOO-rooted package records/edges, all unchanged; unrelated Polymarket record changed.

At this completed merge: scoped all-target check0; fresh position_mode test target0 (**5passed/0failed/0ignored**); extractor0 with output Would update0/0added/0orphaned; independent tomllib census0. Actual integration-parent census:534rows, pending57→47, **10rows moved (0ported/10divergent)**,10distinct notes/15targets, all three assigned files0pending. Earlier source-SHA11passed and two101→0 native controls remain attributed to their original SHA, not relabeled as merged-HEAD runs. No new mutation matrix or full-suite rerun. Incoming legacy/public test targets were not rerun here. Other not-run and inherited audit findings above remain applicable.

## Final fetch and merge before push

Merge `133d3809f8a023033c1f592161e949f29716c5ba`, actual integration parent `08ae16ec90c07b515a248b1009901cae079ab775`, merge0. Fresh extractor0 (Would update0/0added/0orphaned) and tomllib census0 at this SHA: **10moved,0ported/10divergent**, pending57→47 of534rows across22manifests;10distinct notes/15targets; owned three files0pending. Independently traversed native local dependency manifests and compared tracked WOO source/tests/scripts, all 7 local crates' inputs, actual TS sources, owned manifests, workspace Cargo.toml and Cargo.lock: no changes from the measured cfc7 merge. No Cargo repeat; cfc7 check0 and five-test0 remain attached there. Final evidence-only commit does not relabel these gates. Fresh branch originally based on dad3db626d; no unrelated unreviewed feature-branch ancestor. No full-suite/workspace/clippy/wasm claims; earlier stopped build and inherited optional-audit1 remain disclosed above.
