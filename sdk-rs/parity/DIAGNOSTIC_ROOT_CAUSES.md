# Systemic causes of canonical diagnostics

Cross-lane glob follow-up: the specific same-file nominal/imported-owner false positive contributes **0 of the 5895 diagnostics**. It is a real indexing abort, already repaired by the checker source in queued PR 7854 before this corpus was produced. The controlled measurement and its limits follow below; this does not establish the absence of other checker false positives.

The frozen corpus contains **5895 diagnostics**. Three repeated annotation patterns account for **3782 (64.2%)**, and a fourth legacy-format cluster accounts for another **449**, making **4231 (71.8%)**. The remaining **1664** are not yet assigned a systemic cause. The measurements do not support extrapolating five heterogeneous repairs into 2450 independent behavioral defects. They also do not establish a single writer fix that would lawfully clear thousands of diagnostics.

Measured source/data revision: `ba57ffe69d813b4fb1de6ab67b9784618c162efa`. This analysis branches from reviewed PR 7854 head `fc4c3a501ae53384bdc3fa0e847527a87a763c16`, which was in the merge queue when the analysis began; its integration parent is `333fd419fdbf70f081039610f22da9ee653bc96f`. That reviewed feature dependency is explicit, not an unreviewed ancestor. No manifest row, status, exemption or checker rule changes in this analysis. TypeScript remains the maintained reference.

## Largest root-cause families

Counts are disjoint diagnostic occurrences, not row counts or measured repair yields. A structural family establishes the repeated omission that triggers validation; it does not certify every existing mapping's semantic correctness.

| Repeated omission or adaptation | Diagnostics | Share | Measured grouping | What a lawful batch must establish |
| --- | ---: | ---: | --- | --- |
| Public trait/API surfaces omitted when business-function mappings are authored | 2563 | 43.5% | 216 unclaimed trait declarations plus 2347 associated members, across 250 trait families in 161 Rust files | Associate each trait/member with actual TypeScript behavior and explain adapted splits/shared roles; mapping a trait owner alone does not claim its members |
| Rust owner/namespace relocation recorded in rs but absent from rename declarations | 670 | 11.4% | 45 TypeScript source files / 106 Rust files; exact callable/type leaf preserved; unresolved targets excluded | Review the existing receiver/namespace adaptation, then record exact rename targets consistently across a source family |
| Mappings added without maintaining the per-source Rust file declaration | 549 | 9.3% | 130 missing source/file pairs in 46 TypeScript manifests, pointing into 71 real Rust files | Confirm ownership once per source/file pair, then reconcile that header; do not add an unrestricted file superset |
| Top three | 3782 | 64.2% | Grouping units differ between rows | No measured automatic repair or permission to suppress a class |

The first family is principally an authoring-model mismatch: one source operation becomes a Rust function plus an explicit host/dependency interface, but the annotation names only the business function. It is not 2563 newly discovered missing TypeScript functions. The exact inventory includes trait methods, associated types and constants; calling all 2347 members methods would be inaccurate. There are 250 affected trait families: 216 trait owners are themselves unclaimed, while 33 already have a mapping and one has an exemption, each leaving associated members unclaimed. The complete maintained public inventory contains 444 traits and 5109 members; only the stated subset is diagnostic. These totals are not a proof that every trait is disposable host glue.

Bounded source reviews demonstrate the pattern in CrossTradeRuntime, GRVT PrivateFeedHost, WOO legacy socket Host, Backpack REST Host and the Poseidon loader interfaces. GRVT PrivateFeedHost alone produces 63 diagnostics; BuilderHost produces 52; WOO's legacy fill-recovery and socket-control Host families each produce 51. CrossTradeManagerDeps already maps its runtime interface, but clock/log/scheduler methods derive from globals surrounding the source methods rather than four new TypeScript declarations. The source and Rust call sites therefore matter when deciding where split ownership belongs. Blanket Host exemptions or treating an owner mapping as implicit coverage would change the gate, not repair these omissions. The governing contracts remain [invention](MANIFEST-SPEC.md#5-the-gates) and [split mapping](MANIFEST-SPEC.md#10-holes-8-and-9--layer-was-tied-to-a-status-and-one-ts-symbol-could-not-map-to-two-rust-items).

The second family is narrower than all 1247 rename diagnostics. It requires an existing resolved target and an exactly unchanged leaf while qualifiers differ. It includes free-function extraction, a differently named receiver struct, and four unchanged-leaf namespace relocations. The other rename diagnostics are not silently put in the same bucket. Examples are AsterDEX exchange methods moved onto AccountFetch, and equal-leaf exchange methods exposed as free operations. Bitget contributes 53 such diagnostics in its exchange source, Bybit 51, Hyperliquid 49 and WOO 49. Those are source-family review opportunities, not demonstrated four-edit fixes. The applicable rule is [qualified correspondence](MANIFEST-SPEC.md#11-hole-10--name-correspondence-and-what-it-does-not-cover).

The third family excludes every unresolved target. Of its 549 occurrences, 317 have no declared Rust files and 232 have an incomplete declaration. One concrete high-fan-out pair is hyperliquidspot.types.ts to spot_models.rs: 50 already-resolving mappings lack the same declaration. The same source also has 16 mappings into spot_types.rs. Existing SpotToken/SpotMarket/SpotMeta source declarations and real Rust models, and Backpack's exchange-to-adapter/startup mappings, were reviewed as bounded examples. The earlier one-diagnostic Backpack factory sample was not representative of header fan-out. Adding a legitimately owned file once can address many rows, but the 50-for-one figure here is an exposure count, not a performed repair. See [per-source provenance](MANIFEST-SPEC.md#12-provenance--a-rust-file-must-be-declared-for-the-typescript-file-that-maps-into-it).

## Additional cluster and unresolved remainder

**Legacy crate-qualified mappings account for 449 occurrences:** 113 unresolved old-format targets produce 113 resolution, 113 rename and 113 provenance diagnostics; ten such rows also lack layer, and 100 additional unclaimed public declarations have a compatible canonical file/item spelling. This connects multiple diagnostic classes to one repeated authoring format. The string conversion finds 104 unclaimed counterpart candidates, of which four are already counted under the higher-priority trait family. Candidate spelling does not establish a semantic replacement, and no such mapping is rewritten. Examples include safe_cex_crypto loader paths, worker class paths and Breakoutprop module paths. The 113 old targets are not 449 independent repairs.

The **1664 residual diagnostics** comprise 1091 unclaimed non-trait items, 464 other rename failures, 58 other missing layers, 36 unused source declarations, 13 unresolved canonical-format mappings and two stale exemptions. This is a bounded remainder, not 1664 independent behavioral defects, and not six repair units merely because six class names remain. The missing-layer hypothesis alone cannot explain hundreds or thousands: there are only 68 layer failures in fifteen manifests, ten already assigned to the legacy cluster.

## Is there one broken generator?

No shared generator regression was demonstrated. The maintained extractor's reconcile function preserves the whole matched row and owning file metadata. New rows start pending with empty annotations; Rust mappings, layers, rename notes and rust_files are deliberately agent-owned. It therefore does not infer the missing trait ownership, rename intent or source-file associations. Changing it to manufacture those declarations would assert facts not established by extraction.

History distinguishes forgotten migration from later repeated authoring omissions. Public trait/member invention checks originate in `13409676a6` (2026-10-11). Qualified correspondence and explicit rename-token separation are in `ef86ce8d40` and `e08207e284` that evening; per-file provenance replaces the central registry in `9160ace42c`, following `ca5c000c66`. Later WOO and GRVT ports still exhibit the omissions. These examples rule out explaining the entire corpus solely as pre-contract manifests awaiting one schema migration. No claim is made that every historical ad-hoc lane writer has been audited.

The evidence favors **systemic annotation debt with concentrated review families**, rather than thousands of independent algorithm defects. A shared annotation workflow could process the 250 trait families and 130 source/file pairs in batches while preserving actual ownership and reviewed adaptations. The 670 relocation cases can likewise be reviewed by their 45 source families. These are inventories of review scope, not a sum of independent repair units or a measured completion estimate: the groups overlap source ownership, and association work remains. No one-line fix clearing a thousand is proved. Conversely, the 2.4 ratio from five repairs is not a defensible estimate of remaining work. Changing check into a report-only command is a separate decision; this analysis changes neither behavior nor validation severity.

## Measurement and controls

`parity-trait-inventory` parses real source with syn and intersects with the maintained public index. Explicit test-only and visibility checks prevent a test/private trait sharing an ID with a production struct from being mistaken for a public trait. `scripts/diagnostic-causes.py` joins the inventory, the complete direct failing checker output and decoded TOML. Unknown or empty diagnostics fail; assignment priority makes the reported groups disjoint. Generated logs and JSON remain ignored under target. The finite enumeration ends after every line of the 5895-diagnostic capture and all 512 manifests; semantic source reviews stop at the examples named above.

At tool revision `4f3fded23584b2a10c05d11b397883e8660fa2d2`, disabling trait-family assignment made the independently literal nine-diagnostic fixture fail at `CAUSE independently expected disjoint root groups` (exit 1). Exact restoration passed (0). Changing the Rust observer's trait-owner ID made its independently literal six-item fixture fail at `TRAITS exact native declaration identities` after compilation (101); exact restoration passed (0). Neither expectation is obtained from the production artifact being measured.

Review found a real observer defect: test-only traits could collide with production nominal IDs. At regression revision `3abe65fce5`, adding test-only Receiver and ignored-module traits reproduced the named native-identity failure (101). The corrected observer at ba57 passes (0), including a restricted-trait case. Its complete production TSV is byte-identical before and after correction, so the defect did not change this corpus's counts. This is a fixed future-input defect, not a hidden survivor or a checker false positive.

At ba57, scoped parity all-target cargo check, strict scoped host Clippy, the one-test inventory target and the inventory execution each return 0. The complete canonical check still returns 1 with exactly the same diagnostic multiset. The three Python fixture tests, read-only cause analysis and root extractor check each return 0 at the same revision; extractor text reports zero changes/additions/orphans. No production manifest, checker-library or exemption input is edited; original policy declarations remain linked through [the taxonomy](MANIFEST-SPEC.md#divergence-taxonomy), without being copied into this report.

Not rerun for this observational slice: workspace build, workspace host Clippy, wasm build/Clippy/lane, complete Rust/monorepo suites, report/progress wrappers, live accounts or browser execution. The prior PR's exits retain their own SHAs. No class was disabled, no status moved, and no repairs were performed to manufacture these measurements.


| Gate at ba57 | Exit | Scope |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --all-targets --jobs 2` | 0 | Scoped parity compile floor |
| `cargo clippy --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --all-targets --jobs 2 -- -D warnings` | 0 | Scoped host lint, not workspace or wasm |
| `cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --bin parity-trait-inventory --jobs 2` | 0 | One independent native-identity test |
| `cargo run --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --bin parity-trait-inventory --jobs 2 -- sdk-rs` | 0 | Complete public trait inventory; corrected TSV unchanged |
| `sdk-rs/target/debug/parity-check --workspace sdk-rs` | 1 | Same full 5895 diagnostic multiset |
| `python3.12 sdk-rs/parity/scripts/diagnostic-causes.test.py` | 0 | Three independent fixture tests |
| `python3.12 sdk-rs/parity/scripts/diagnostic-causes.py --diagnostics <direct-log> --traits <inventory-tsv>` | 0 | Complete disjoint count; corrected output unchanged |
| `yarn parity:extract:check` | 0 | Zero changes/additions/orphans; real Yarn Node CLI and output text inspected |

Cargo operations, including both mutation/restoration pairs, were sequential with CARGO_INCREMENTAL=0. Read-only Git comparison at report revision `a6b530e426bc0f25d195d5c1325cc5d03d0602ca` confirms every manifest, exemption, checker-library/reexport and TypeScript input is identical to the queued PR 7854 head. Compiler/cwd inspection at a6b finds no active worktree compiler; the empty incremental directory is removed and absence verified (0 KiB removed). No additional cache-space recovery is claimed.


Read-only sub-agent review at `a6b530e426bc0f25d195d5c1325cc5d03d0602ca` returns MERGE for the observational scope, with no concrete blocker. The reviewer independently recomputes counts/shares and grouping scopes, inspects complete diagnostic/TSV/JSON equality and named mutation/restoration logs, verifies completed scoped gates and preserved extractor metadata, and checks contract history against later source-port commits. It runs no Cargo, tests, mutations or edits; its commands use the shared primary worktree, with no checkout-isolation claim. The later gate-table and review-record commits are documentation only and do not relabel source measurements.


## Same-file nominal beside a glob: measured impact

The count is **0 / 5895** for the specific conservative imported-owner rule. At measured follow-up revision `eaf4514fa92f20c14929d355eaa856eb99ff6d07`, the unchanged maintained checker emits exactly the frozen 5895-line multiset, with zero imported-owner errors. The CrossTradeLoop declarations and methods are indexed. Its is_disposed member and the distinct CrossTradeLoopFn alias still appear as unclaimed public items; neither is an imported-owner rejection. No claim about all other glob/reexport rules is made.

The archived integration source at `333fd419fdbf70f081039610f22da9ee653bc96f` has the eager guard in RustIndex::items: a matching import, including any glob, returns an indexing error immediately. The producer of this corpus includes `c54d4a5287efe6f1f4cab77823e8bcd4945cdf04` from PR 7854: it defers owner resolution and proves an unconditional same-scope nominal declaration despite a glob, while retaining explicit-import uncertainty checks. The production cross_trade/mod.rs Git blob is identical at integration 333 and the measured branch (`933d8edd16407768aea7fbb9d5180cac45e17971`). This is checker-version/phase separation, not a claim that the archived false positive is a manifest defect. The executable used by the other lane was not independently identified here.

run builds rust_index with error propagation before invoking check. Consequently the eager imported-owner failure cannot be one of the later status/invention diagnostics. It prevents their enumeration. There is no measured thousands-to-zero repair yield from a command that aborts before producing its list.

The temporary control reinstates the old eager refusal only when the owner has a same-file struct/enum/union declaration. This isolates the local-nominal/glob case while preserving direct imported-owner support. The existing bookkeeping call is retained so the strict dead-code lint remains meaningful. No production checker change is committed. All exact source bytes and the compiled checker are restored after the control.

The full-workspace old-local-rule run compiles (0), exits 1 with exactly one indexing error, and emits zero status/invention lines. Its first blocker is TealstreetTransport beside crate provider/transport-resolution globs. That is an observed first blocker, not an enumeration of all potentially rejected owners. The independent CrossTrade test materializes all five real sibling source files, retaining pub use types::*. It asserts exactly three native indexed identities (CrossTradeLoop, is_disposed and run) before verifying that all three remain invention-gated against an unrelated baseline manifest. The temporary old-local-rule control compiles and runs one test, then fails at `GLOB production CrossTradeLoop must index` (101). After exact restoration, the same one-test target passes (0). This distinguishes the reported CrossTrade false positive from whichever module is encountered first in the complete workspace.

| Follow-up command (eaf4514fa9 unless noted) | Exit | Measured result |
| --- | ---: | --- |
| Direct maintained `parity-check --workspace sdk-rs`, baseline at `3d024b5b7239ef49c1a4589048cf0659ac5e4782` | 1 | Exact original 5895 diagnostic multiset; zero imported-owner errors |
| Selected CrossTrade production-source test, baseline | 0 | One test passed before mutation |
| Scoped `cargo build -p safe-cex-parity --bin parity-check --jobs 2`, temporary local rule | 0 | Negative-control executable compiled |
| Direct maintained checker, temporary local rule | 1 | One TealstreetTransport indexing abort; zero manifest diagnostics |
| `cargo test -p safe-cex-parity --test gates production_cross_trade_nominal_beside_glob_remains_indexed --jobs 2 -- --exact`, temporary local rule | 101 | One test executed; named CrossTradeLoop indexing assertion fails |
| Same scoped checker build, restored | 0 | Restored executable compiled |
| Direct maintained checker, restored | 1 | Exact original 5895 diagnostic multiset; multiset additions/removals both zero |
| Same selected CrossTrade test, restored | 0 | One test passed; native identities and invention assertions retained |

All Cargo commands use the SDK manifest and --locked, sequentially with CARGO_INCREMENTAL=0. Generated logs remain ignored under target. The witness is source-index behavior, not execution of the native trading loop.

Rejected preliminary probes are retained as limitations: replacing the bookkeeping call with the old error caused strict dead-code compilation failure (101), so that attempt proves no runtime behavior. Reinstating the full old imported-owner guard compiles (0) but aborts first at ExchangeWorker, which is not the targeted local-glob witness. An early local-only probe then aborts at TealstreetTransport before reaching CrossTrade; assuming CrossTrade must be first was an invalid ordering expectation. The completed control instead checks the full workspace's one indexing abort and independently requires the named CrossTrade fixture failure. Those earlier probes are not counted as successful named controls. Source bytes and the compiled checker are restored in the completed run.

The conclusion is limited to this specific rule and frozen corpus: a real pre-validation checker defect exists in the archived guard, but **it manufactures none of the 5895 diagnostics already captured after its repair**. The count does not certify every remaining manifest claim, and does not adjudicate broader public-alias/glob conservatism. No row, mapping, status, exemption or canonical validation severity changes in this follow-up. Policy definitions remain canonical at [the taxonomy](MANIFEST-SPEC.md#divergence-taxonomy) and its linked declarations.


Post-control scoped gates at `5a80a396a195758d5efa125e5a0794355ecfe9d6` both complete with exit 0: `cargo check --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --all-targets --jobs 2` and the same scoped `cargo clippy` command with `-- -D warnings`. Their Finished dev text is inspected after completion; they are not workspace or wasm checks. No active worktree compiler is observed afterward. The empty incremental directory is removed and absence verified (0 KiB removed). Git comparison confirms the production checker library/reexports, all manifests and exemptions are unchanged after exact restoration. No extractor, workspace build, workspace Clippy, wasm lane, full suites, or new report/progress run is claimed for this focused follow-up.

Read-only sub-agent review at `5a80a396a195758d5efa125e5a0794355ecfe9d6` returns MERGE for the scoped regression/explanation. It independently compares all three 5895-line multisets, the exact archival CrossTrade source blob, the one-line mutant abort and named compiled CrossTrade failure/restoration logs. It runs no Cargo, tests, mutations or edits and does not independently remeasure exits. The baseline canonical row above retains its actual 3d revision; the selected test and mutation/restoration results belong to eaf, while these later scoped compile/lint exits belong to 5a. This review record changes no measured source input.

## Measured authoring follow-up

The ten largest trait/API families were annotated at their existing consuming
operations. Diagnostics fell 5895 -> 5321 with zero additions: 495 API ownership,
61 rename and 18 provenance diagnostics cleared. This required 495 target
annotations across 69 existing rows, with no status movement. The measured yield
is 49.5 API diagnostics per family, or 57.4 including collateral; a family is not
one row edit. Per-family revisions, denominators, controls, gates and limits are
recorded in [trait declaration evidence](TRAIT_DECLARATIONS_EVIDENCE.md).

The next measured batch examines ranks 11–20 and clears another 363 diagnostics
(287 API, 46 rename, 30 provenance), leaving 4958. Its yield is 6.259 diagnostics
per family-row edit event, versus 8.319 in the first batch; cumulative yield is
7.378 across 127 events on 122 distinct rows. Two source capabilities remain
unassigned for lack of established production algorithm callers, keeping their
invention diagnostics visible. The evidence sheet records the decay denominators
and why the approximately-709-edit original scenario is not a calendar forecast.

Ranks 21–30 remove another 285 diagnostics (214 API, 45 rename, 26 provenance),
leaving 4673 with no additions or status movement. Family-row yields now run
8.319 -> 6.259 -> 6.196, cumulatively 1222/173 = 7.064. Including the two later
role-wording corrections gives 1222/175 = 6.983. Three capabilities remain
unassigned; thirty families are examined and twenty-seven fully assigned.
The same evidence sheet records an inclusive 47m26s elapsed lane window for the
preceding 937 removals: 1185.2/hour. Its equal-rate 4.18-hour remaining-work
scenario is conditional on similar work; it does not establish the unmeasured
tail or other causes' effort, nor a calendar completion date.
