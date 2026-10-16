# Canonical diagnostic characterization

Five independently incorrect metadata units now repaired remove twelve diagnostics: 5907 becomes 5895. The latest four repairs remove four diagnostics; statuses do not move. The largest three classes contain demonstrable annotation defects in the reviewed samples; no checker relaxation is justified by those samples. Check/report/progress still return 1, so no globally validated census or canonical report is claimed.

Baseline data revision: `0828120734aa537a0e70c5e609a9221f392de579`, with Rust checker inputs `3a72e9229041df133f970c8f5b76da2540ac9b1d` and merged integration parent `0445a753756f197e4363222eab9e787868e4f700`. The maintained check/report/progress binaries each returned 1 after indexing completed. Their complete outputs contain the same 5907 diagnostics. Source and gate history remain in ORPHAN_FINDINGS_EVIDENCE.md.

Repaired data revision: `a281be2cedb24ff6ece67eb05596e68467984f23`, after merging integration `d74a1c01f77439f9df61fa330063be194aab7576`. Its merged baseline is isolated below.

The classifier is `scripts/diagnostic-census.mjs`. It counts occurrences, including repeats, and refuses unknown, ambiguous, empty or wrapper output. Its input is direct maintained checker output, not a generated report or a replacement gate. Generated logs remain ignored under target; this document is manually authored.

| Diagnostic class | Original baseline | Poseidon repair | Three-family repairs |
| --- | ---: | ---: | ---: |
| Public Rust item without a mapping or exemption | 3758 | 3754 | 3754 |
| Rename note missing the exact target token | 1251 | 1250 | 1247 |
| Rust source file absent from the owning declaration | 664 | 663 | 662 |
| Target does not resolve | 127 | 126 | 126 |
| Nonempty mapping without a layer | 68 | 68 | 68 |
| Declared Rust source file unused by mapped rows | 37 | 36 | 36 |
| Exemption no longer names a public item | 2 | 2 | 2 |
| Total | 5907 | 5899 | 5895 |

Original baseline families are 2147 status and 3760 invention diagnostics. The largest three classes total 5673, or 96.0%. This is a complete enumeration of this captured output, ending at safe-cex-webworker/src/worker_session.rs::WorkerTable. It is not 5907 distinct TypeScript rows or behavioral defects: the status diagnostics cover 1414 distinct row identities across 122 source files, plus 37 file-level diagnostics. A single malformed mapping can produce several diagnostics and leave its real Rust declaration unclaimed. Simple status tallies do not validate that mapping.

## Rulings on the three largest classes

**Unmapped public items:** genuine missing annotations are demonstrated; no checker false positive was reproduced in the sampled cases. In the Poseidon example, the existing main row points to legacy safe_cex_crypto::poseidon_example::main, while the real file declares main, ConsoleValue, ExampleConsole and ExampleConsole::log. The TypeScript main runs five examples and calls console.log/console.error; the Rust main runs those sections through the explicit console trait and typed arguments. Those four declarations implement or support that existing source row. The checker does not infer a split from the presence of its enclosing module or main mapping. [The invention contract](MANIFEST-SPEC.md#5-the-gates) and [split mapping contract](MANIFEST-SPEC.md#10-holes-8-and-9--layer-was-tied-to-a-status-and-one-ts-symbol-could-not-map-to-two-rust-items) account for this case without a helper exemption. The source main is behavior, not an exemptable binary entrypoint merely because it is named main.

Backpack's api_factory.rs is a second bounded example: it declares a public Host and associated members used by create_api; mapping create_api alone does not annotate those members. Public declarations inside private modules are also deliberately indexed. The existing syn_discovers_enums_methods_and_inline_namespaces fixture independently distinguishes unrestricted pub from pub(crate), private methods, fields, variants and test-only items. This is intentional conservative scope, not evidence that every public helper has an independent TypeScript function. No behavioral ruling is made for the other unmapped items without reviewing their call sites and source counterparts.

**Rename notes:** the sampled failure is a real annotation defect for a deliberate adaptation. CrossTradeManager#_failureKey returns account + ':' + symbol in TypeScript; CrossTradeManager::failure_key returns the same format in Rust. At baseline its note explains the behavior but omits its exact target token. The emitted expected path retains the leading underscore. [The correspondence contract](MANIFEST-SPEC.md#11-hole-10--name-correspondence-and-what-it-does-not-cover) accepts a documented rename, but general prose is not that declaration. This sample does not demonstrate an algorithm defect or justify disabling correspondence for the class. Across the full log, 1179 distinct rename targets are named; 116 also appear among unresolved targets, so rename errors cannot all be treated as cosmetic.

**Undeclared source files:** the sampled failure is a real provenance defect. Backpack's createAPI row already points to api_factory.rs::create_api. The TypeScript factory sets up the HTTP client and interceptors, which are implemented in that Rust file, but its manifest declares only api.rs. [The provenance contract](MANIFEST-SPEC.md#12-provenance--a-rust-file-must-be-declared-for-the-typescript-file-that-maps-into-it) reports that missing declaration independently of the rename note. Of 664 distinct targets in this class, 114 also appear as unresolved; blanket declaration widening would preserve those broken mappings. The Poseidon example exhibits the same class through a legacy target despite already declaring its actual source file.

These are finite source reviews, stopping at Poseidon main and its three console declarations, CrossTradeManager#_failureKey, and Backpack createAPI plus its Host declaration. They establish real metadata defects in the largest classes and consistency with the written checker contract; they do not establish semantic validity or invalidity of every member. No checker rule, public visibility, exemption or live-owned FINDING is changed to suppress a class.

## Representative repair

The smallest identified repair is the existing single Poseidon example row. Its target must name the actual main and its console ABI split. Its status, layer, identity, existing behavioral caveats and exact source-file declaration are preserved. Boundary rulings remain canonical at [the taxonomy](MANIFEST-SPEC.md#divergence-taxonomy), [UTF-8 boundary](MANIFEST-SPEC.md#shared-utf8-string-boundary), [typed output boundary](MANIFEST-SPEC.md#closed-typed-output-boundary) and [snapshot boundary](MANIFEST-SPEC.md#owned-snapshot-vs-live-alias); this characterization does not redeclare them.

The repaired row uses the maintained manifest serializer's four-target array, with an explanation for main, its console argument enum, host trait and log member. No completion of the other classes is implied. Its production-source regression independently names all four native targets before checking validation with no exemptions.

## Baseline measurements

At tool revision `87498bc05aa05efe566c0c18c26917ece87d9599`, the direct maintained parity-check binary returned 1 and reproduced the full 5907-class distribution above. The classifier returned 0 after accounting for every nonempty line. The scoped two-test Node target returned 0.

A deliberate classifier mutation stopped counting unmapped-public diagnostics. The independently literal expectation failed with process exit 1 at `DIAGNOSTIC independently expected class counts`. Exact tracked bytes were restored, and the same two-test target returned 0. The control runner returned 0. This is a Node assertion proof, not a compiled Rust control. Unknown diagnostic, wrapper output and empty-input rejection are also tested. Gate results for the representative manifest repair follow at its own committed revision.

## Representative measurements

The required merge-down incorporated integration `d74a1c01f77439f9df61fa330063be194aab7576` via `3e56ea5aefeea59e36e011d4fd8818edab74981e`, without conflicts. Incoming changes include Bybit inverse reads and boundary audits; separate-checkout review found no overlapping changed path or new unsupported source-indexing form. The checker library and exemption file are unchanged. Earlier broad host/wasm results retain their historical Rust input revision in ORPHAN_FINDINGS_EVIDENCE.md and are not transferred to this merge.

The direct maintained checker ran against a git-archive manifest snapshot of merge revision 3e56, using the current identical production Rust index inputs. It returned 1 with 5907 diagnostics. An independent full multiset comparison against the pre-merge 87498 output found zero added and zero removed diagnostics, not merely equal totals. The production index inputs are unchanged between 3e56 and repair `a281be2cedb24ff6ece67eb05596e68467984f23`; that repair changes only this sheet, the single manifest and a test file.

At a281, the direct checker returned 1 with 5899 diagnostics, distributed as shown in the repaired column: 2143 status and 3756 invention. Full multiset subtraction found exactly eight removals, all the Poseidon example: four unclaimed public declarations, one rename declaration, one undeclared-file mapping, one unresolved legacy target and one unused-file declaration. There were zero additions. Canonical validation is still red; this is an eight-diagnostic repair, not global gate success.

The scoped all-target parity cargo check returned 0 at a281. All 72 tests in the gate file returned 0, including the existing public/private discovery witness and the new maintained Poseidon-source/manifest regression. Its independent four-target expectation is checked before the validator result; no wire roundtrip substitutes for native mapping assertions.

At the same SHA, a control used the maintained manifest serializer to remove only ConsoleValue from the array. The exact focused test compiled and ran one test, failing at `POSEIDON exact native targets` with exit 101. Exact original bytes were restored; the same one-test target returned 0. The runner returned 0 and required compilation completion, one executed test, the named failure and a successful restoration. No compiling survivor was observed.

An independent recursive tomllib census at a281 returned 0: 512 manifests, 4796 ported / 2229 divergent / 309 pending / 7 not-applicable, total 7341. A git-archive/tomllib census at integration parent d74 returned 4794 / 2231 / 309 / 7 over the same 512 files. The difference remains the original orphan phase's two status changes. This representative repair changes one row's mapping/note and moves zero statuses or pending rows; incoming integration accounts for the earlier pending-count change.

## Committed repair gates

All following root executions measured source/data revision `a281be2cedb24ff6ece67eb05596e68467984f23`. The root Yarn entry points ran through the real Yarn Node CLI, with output text inspected. Their canonical diagnostic multisets equal the direct checker's complete 5899-line output. Later evidence-only commits are not new measurements.

| Command | Exit | Scope |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-parity --all-targets --jobs 2` | 0 | Scoped parity crate, all targets |
| `cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --test gates --jobs 2` | 0 | All 72 tests in the changed file |
| `cargo clippy --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --all-targets --jobs 2 -- -D warnings` | 0 | Scoped parity host lint |
| `yarn parity:extract` | 0 | Updated zero manifests; zero added/orphaned symbols |
| `yarn parity:extract:check` | 0 | Would update zero manifests; zero added/orphaned symbols |
| `yarn parity:check` | 1 | 2143 status / 3756 invention diagnostics |
| `yarn parity:report` | 1 | Same 5899 diagnostics; report refused |
| `yarn parity:progress` | 1 | Same 5899 diagnostics; progress report refused |
| `rustfmt --check --edition 2024 sdk-rs/safe-cex-parity/tests/gates.rs` | 0 | Changed Rust test file |
| `node --test sdk-rs/parity/scripts/diagnostic-census.test.mjs` | 0 | Both focused classifier tests |

Cargo commands were sequential, including the mutation's two compiled test executions. Generated logs are untracked. After restoration/reconciliation, exact Git diff showed only this manual sheet dirty; no source or manifest input was transferred to a different SHA.

Not rerun at this merge/repair: workspace build, workspace-wide host Clippy, or the wasm lane. Their prior requested executions passed at the historical 3a72 Rust inputs, as recorded separately. Also not run: full Rust/monorepo suites, the separate full FFI smoke job, browser/live-account checks or clean-install/CI-home isolation. The historical auxiliary worker recovery fixture failure is not a new recovery pass. These limitations do not prevent the measured metadata characterization or establish global behavioral parity.

## Adversarial review

Separate-checkout sub-agent review at `26716e0f3ddecf8a468d90e9de3fd9dbf0be4e39` returned MERGE for the scoped characterization and representative repair, with no concrete blocker. The reviewer independently compared the complete diagnostic multisets, all seven repaired class counts and current census, and inspected both mutation/restoration pairs plus scoped gate completion text. It confirmed zero integration diagnostic additions/removals, exactly eight repair removals and zero additions, preserved source inputs and truthful non-run limits. Its diff check exited 0. The reviewer ran no Cargo, source edits or mutations and did not independently remeasure process exits or their SHA attribution. This subsequent review record changes no checker, test or manifest input.

## Cache hygiene and read-only refresh

At revision `912ac7a020b54a6a9f3880e66347920491bac46c`, process inspection found no Cargo, rustc or Clippy compiler active in this worktree. The local sdk-rs/target/debug/incremental directory occupied 12827044 KiB (13134893056 allocated bytes) before deletion. Its deletion returned 0 and absence was verified afterward. This is the measured size of the removed cache, not a claim about concurrent fleet-wide free-space changes.

The existing maintained binaries were then rerun without invoking Cargo, preserving the cache's absence:

| Command | Exit | Result |
| --- | ---: | --- |
| `sdk-rs/target/debug/parity-check --workspace sdk-rs` | 1 | 5899 diagnostics: 2143 status and 3756 invention |
| `sdk-rs/target/debug/parity-report --workspace sdk-rs` | 1 | Same diagnostic multiset; report refused |
| `sdk-rs/target/debug/parity-progress --workspace sdk-rs` | 1 | Same diagnostic multiset; progress report refused |
| `node sdk-rs/parity/scripts/diagnostic-census.mjs <direct-checker-log>` | 0 | Same seven repaired class counts shown above; all diagnostic lines accounted for |

Independent multiset comparisons returned 0 against the a281 repaired output for all three binaries. The only additional report/progress text was the expected validation-refused message. No new mapping, status, exemption or checker rule changed in this refresh. The largest-class rulings and bounded source samples above remain the delivered characterization of the original 5907 diagnostics. Root Yarn wrappers, compilation, Clippy, wasm and test targets were not rerun at 912; their recorded earlier exits retain their own revisions. The incremental directory remained absent after the read-only refresh.

## Three-family repair follow-up

The integration parent actually incorporated is `fada0ab29c9534bf4b51ec6e9c9385cf490b5b4c`, via merge `a0e2104cbe534556e23e16b4f3adfa650a30930b`. Its incoming changes from the prior parent are Bybit test/evidence only. Direct maintained checking at the merged baseline returns 1 with 5899 diagnostics. Full multiset comparison against the preceding read-only refresh finds zero additions or removals. This isolates the repair from integration movement; earlier broad Rust measurements retain their historical SHA above.

Metadata revision `e185f189b9c5ead442081f6f1f7e7d87961485d8` changes three CrossTrade notes and Backpack's file declaration. The existing Poseidon repair remains present. The CrossTrade methods are `_failureKey`, `_recordSuccess` and `_meetsMinAmount`; each now names its existing Rust target as a standalone rename token. Their source bodies respectively construct the account/symbol key, remove that key after success, and apply the minimum-amount guard. This is correspondence annotation for the existing adaptation, under [the correspondence contract](MANIFEST-SPEC.md#11-hole-10--name-correspondence-and-what-it-does-not-cover), not a new behavioral port. Backpack declares the exact api_factory.rs file already used by its existing createAPI mapping, under [the provenance contract](MANIFEST-SPEC.md#12-provenance--a-rust-file-must-be-declared-for-the-typescript-file-that-maps-into-it). No unrelated file allowance is added. Existing boundary rulings stay at [the taxonomy](MANIFEST-SPEC.md#divergence-taxonomy) and its linked canonical declarations; no declaration is copied here.

At e185 the direct checker returns 1 with 5895 diagnostics: 2139 status and 3756 invention. Complete multiset subtraction from the merged baseline proves exactly three rename diagnostics and one undeclared-file diagnostic removed, with zero additions. Remaining per-class counts are the last table column above. The unmapped-public class stays 3754: declaring Backpack's factory file does not annotate its other public members. The finite source review now stops after the three named CrossTrade methods, Backpack createAPI/factory provenance, and the earlier Poseidon main/console split. These samples justify these repairs, not a blanket ruling on every remaining item.

| Repair unit | Independently incorrect metadata units repaired | Diagnostics removed | Diagnostics per unit |
| --- | ---: | ---: | ---: |
| Poseidon existing row mapping/split | 1 | 8 | 8.0 |
| CrossTrade existing method rename notes | 3 | 3 | 1.0 |
| Backpack existing source-file declaration | 1 | 1 | 1.0 |
| This follow-up, excluding prior Poseidon repair | 4 | 4 | 1.0 |
| Cumulative three families | 5 | 12 | 2.4 |

Grouping instead by the three source-file families gives 12 / 3 = 4.0; that is a different denominator, not five repairs becoming three. Five heterogeneous repair units do not establish whether the remaining 5895 diagnostics represent 70 or 700 defects. No checker rule, exemption, public visibility or diagnostic class is changed or suppressed.

## Follow-up controls and attribution finding

At `927bc1d7ca4e00a28560543614a33c99e7fde05f`, all four `manifest-repair-controls.mjs` invocations return 0: `cross-failure-key`, `cross-record-success`, `cross-meets-minimum`, `backpack-provenance`. Each uses the maintained manifest parser/serializer to remove exactly its new declaration. Each mutated focused Node target returns 1 at its independently expected native assertion, then at `REPAIR selected source declarations validate` from the real maintained checker. Exact original bytes are restored and all three focused tests return 0. Native assertion names are `CROSSTRADE explicit rename token: CrossTradeManager#_failureKey`, the corresponding `_recordSuccess` and `_meetsMinAmount` names, and `BACKPACK native source declarations`. These are Node 1-to-0 controls, not Rust compilation controls; the earlier Poseidon 101-to-0 compiled proof remains separately attributed to a281.

Sub-agent review found an attribution defect in that runner: `git diff --quiet -- <manifest>` checked against the index, allowing staged edits to be reported under HEAD. A deliberately staged formatting newline survived the old guard at 927: runner exit 0 while reporting 927, despite non-HEAD input. This is an observed survivor at that guard call site, not a passing attribution proof. Its manifest bytes and index were restored exactly. The four earlier clean-state declaration controls remain valid independently of this survivor.

Correction `6cf213770a053964e0c165008af87d8280961140` compares against HEAD before reading or mutating the input. It also tightens the native rename assertion from substring matching to whitespace-delimited target matching. The same staged newline is rejected with exit 1 at `Control requires manifest identical to HEAD`, leaving the staged fixture untouched. After exact restoration and index cleanup, the clean `cross-failure-key` control returns 0, with its inner native/checker mutation returning 1 and restored three-test target returning 0. No survivor remains for this measured staged-input case; the runner does not claim general repository/concurrent-edit isolation.

## Follow-up measured gates

Unless separately attributed above, the following measured source/data revision is `6cf213770a053964e0c165008af87d8280961140`. Root scripts used the real Yarn Node CLI; text and child completion were inspected. Cargo invocations were sequential, with CARGO_BUILD_JOBS=2 and CARGO_INCREMENTAL=0. Each canonical entry point completed its scoped parity binary compilation before returning the validation failure. These are not workspace-build, all-target or lint measurements.

| Command or independent check | Exit | Measured result |
| --- | ---: | --- |
| `yarn parity:extract` | 0 | Updated zero manifests; zero added/orphaned symbols |
| `yarn parity:extract:check` | 0 | Would update zero manifests; zero added/orphaned symbols |
| `yarn parity:check` | 1 | 5895 diagnostics, 2139 status / 3756 invention |
| `yarn parity:report` | 1 | Same full diagnostic multiset; report refused |
| `yarn parity:progress` | 1 | Same full diagnostic multiset; progress refused |
| `node --import tsx --test --test-reporter=tap sdk-rs/parity/scripts/manifest-repairs.test.mjs` | 0 | Three tests passed; zero failed |
| `node --test sdk-rs/parity/scripts/diagnostic-census.test.mjs` | 0 | Two tests passed; zero failed |
| `node sdk-rs/parity/scripts/diagnostic-census.mjs <direct-checker-log>` | 0 | All 5895 diagnostic occurrences classified |
| Independent complete diagnostic multiset comparisons | 0 | Exactly four removals / zero additions versus merge baseline; canonical three equal direct output |
| Python 3.12 git-archive/tomllib comparison and literal count/field assertions | 0 | All 512 manifests / 7341 row identities enumerated; 4796 ported / 2229 divergent / 309 pending / 7 not-applicable |
| `git diff --check` | 0 | No whitespace errors in follow-up source/evidence changes |

The exhaustive manifest comparison stops at all 512 files in the merged baseline archive, with no row identities added or removed. It finds exactly three note additions and one file.rust_files addition, with zero status, rs or layer changes. The extractor writer confirms no further reconciliation is needed. This raw independent census remains unvalidated globally while the canonical commands return 1. Preliminary audit invocations returned 1 for an interpreter without tomllib, an empty-symbol-table assumption, and a cache-absence assertion while Cargo was running; no partial result from those attempts is treated as a passing audit.

After the final canonical command completed, process/cwd inspection found no Cargo, rustc or Clippy compiler active in this worktree. The recreated incremental directory occupied zero KiB; deletion and verified absence returned 0. The previous 13.13 GB deletion remains the separate historical measurement above, not additional space reclaimed here.

Not rerun for this metadata/Node follow-up: cargo check --all-targets, Rust gate tests, workspace build, scoped or workspace host Clippy, wasm build/Clippy/lane, FFI smoke, full Rust/monorepo suites, browser or live-account checks. Prior broad build/host/wasm exits retain their old Rust input revision; they do not cover this newer integration parent. No CI rerun or forced CI push was requested. All four new declaration controls and the corrected staged-input attribution control are the measured Node proofs, with their revisions distinguished above.

Separate-checkout sub-agent review inspected the metadata repairs, independent native expectations and real-checker assertion. It found the staged-input guard defect at 927; the survivor and correction are recorded above. Source rereview at 6cf found no further concrete blocker and diff checking returned 0. The reviewer ran no Cargo, tests, mutations or edits; source review is not an independent process-exit remeasurement. Final evidence review is recorded in the subsequent review record when completed.

## Integration conflict reconciliation

PR 7854 became conflicting while the preceding measurements were being recorded. Merge `892b2f9928795c0a617ab75fae9be0b19872a797` incorporates integration `333fd419fdbf70f081039610f22da9ee653bc96f`. The new parent includes the independently merged PR 7914 orphan sweep, which overlaps the original phase of this branch. All twenty-two conflict paths were resolved to the incoming integration files in full; store/tests/root_utils.rs also takes integration's matching instrument ABI expectation. Independent sub-agent comparison confirms all twenty-three files are byte-identical to that parent. No CLAUDE conflict occurred.

This preserves integration's stricter proxy object decoding, larger cache/proxy witnesses, OKX row spread/data indexing and continuation behavior. The instrument classifier keeps integration's ProtoMember-tag ABI and existing CHOSEN note. That differs from this branch's historical whole-result refusal; the old AssetClass promotion is superseded, not carried into the new census. No executable-referent parity is claimed, nor does this merge alter [the canonical closed-output policy](MANIFEST-SPEC.md#closed-typed-output-boundary). The metadata task does not re-adjudicate that already-merged choice. Nine obsolete orphan control cases and their unused decoder helper were retired in favor of integration's maintained control inventory; nine indexing/singleton/risk cases remain. The unreferenced instrument oracle was deleted. The independent sixty-outcome income test remains compatible by source review and receives its own fresh execution below.

At merge 892, the exhaustive Python 3.12 tomllib audit returns 0: 512 manifests / 7341 row identities, **4795 ported / 2230 divergent / 309 pending / 7 not-applicable**. Against integration 333, status and layer changes are zero. The metadata delta is only the three CrossTrade notes, Backpack source declaration, Poseidon mapping/note, and previously assigned chaser/TWAP indexing annotations. This is the current raw census, not the superseded 4796/2229 count and not a validated report. There is no unreviewed feature branch merged as an ancestor; source review covers the branch-owned changes.

All three root canonical commands at 892 return 1 with the same full 5895-diagnostic multiset as before the merge: 2139 status and 3756 invention, with the same seven per-class counts. To isolate repair yield on the new inputs, a separate ignored manifest snapshot keeps all merged manifests except the Poseidon, CrossTrade and Backpack source manifests, which take integration 333's exact versions. The maintained checker against that snapshot and the same merged Rust index returns 1 with 5907 diagnostics. Full subtraction removes exactly twelve diagnostics and adds zero. This is a controlled metadata comparator, not an untouched-parent source-index execution; integration's module-level worker macro still requires this branch's assigned indexing repair. The five-unit cumulative ratio remains 12 / 5 = 2.4, with the latest four units yielding 4 / 4 = 1.0.

At 892, root extractor writer and extractor-check each return 0, with zero updates/additions/orphans. The three native-manifest/real-checker Node tests return 0. The pruned Python control runner's syntax compilation and help invocation return 0; that is not a rerun of its compiled mutation controls. Diff checking against integration returns 0. A merge-stage diff check against the old branch returned 2 for trailing blank lines in incoming already-tracked generated evidence logs; those incoming receipts were preserved rather than rewriting their bytes. No new generated output is added by this branch.


Post-merge compiled checks at 892 completed sequentially:

| Command | Exit | Scope |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity -p safe-cex-store -p safe-cex-exchange -p safe-cex-candle-cache -p safe-cex-venue-okx --all-targets --jobs 2` | 0 | Five selected crates and dependencies; compilation completed |
| `cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-okx --test income_inherited --jobs 2` | 0 | One retained test passed; twelve identities and sixty bill outcomes, native assertions before wire |

The actual maintained-source oracle receives its compiler location through SAFE_CEX_TYPESCRIPT_PATH, without a host path in this record. The snapshot comparator, all three canonical output comparisons, classifier, fresh three-test Node target, Python syntax/help checks and independent manifest audit also return their recorded expected results at 892. After these commands, compiler/cwd inspection finds no active worktree compiler. The empty recreated incremental directory occupies zero KiB; removal and absence verification return 0.

Not rerun at 892: workspace build, scoped or workspace host Clippy, wasm lane/build/Clippy, full Rust/monorepo suites, full FFI smoke, browser/live accounts, clean-install isolation, historical compiled mutation controls, or the four Node mutation controls. The latter remain measured at 927/6cf, with the three family manifests and test/control inputs preserved through this merge. This merge's native-manifest tests are a fresh execution, not a relabeling of those controls. No compiler gate running is counted as passed.

The frozen 599 evidence received sub-agent MERGE after inspection of all four mutation/restoration logs, guard survivor/correction, counts and ratios. Separate merge source review at 892 finds no concrete blocker, confirms all twenty-three resolved/ABI files equal integration, and verifies the retained nine control anchors by inspection. It runs no Cargo/tests/mutations. The source verdict was conditional on completion of the running gates; the measured scoped check and retained income test above subsequently completed with exit 0.


Final separate-checkout evidence review at `8c8070b3614a7972052f4c7cc826946edad9406a` returns MERGE with no remaining concrete blocker. The reviewer independently verifies the complete canonical diagnostic multisets, the exact three-file comparator snapshot and its twelve-removal/zero-addition subtraction, and a git-archive/tomllib census of all 512 manifests / 7341 rows with zero status/layer movement. It inspects completed scoped-check, one-test income and three-test Node logs plus their 892 exit receipts; it does not rerun Cargo, tests or mutations, or independently remeasure process exits. Cache absence is observed at review time. This subsequent review record changes no measured source or metadata input.
