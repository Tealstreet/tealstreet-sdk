# Orphan findings and canonical source indexing

Historical measurements below retain their recorded revisions. Merge `892b2f9928795c0a617ab75fae9be0b19872a797` incorporates integration `333fd419fdbf70f081039610f22da9ee653bc96f`, including the independently merged PR 7914 orphan repairs. Conflicting runtime families, manifests and fixtures now use integration exactly. Its instrument ProtoMember-tag ABI and divergent AssetClass supersede this sheet's earlier whole-result refusal and collateral AssetClass promotion; those earlier measurements are not current claims. Superseded cache/proxy/instrument/income/lookup/stats controls were removed from scripts/orphan-finding-controls.py; integration's orphan_findings_controls.py owns their replacement inventory. The dead instrument oracle was removed. Current diagnostic metadata repairs and post-merge gates are recorded in CANONICAL_DIAGNOSTICS.md.

Parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae` on `feat/safe-cex-rust`.
Branch: `feat/rust-orphan-finding-sweep`; PR: https://github.com/Tealstreet/tealstreet-next/pull/7854.
No unreviewed feature branch is an ancestor. After the gates, integration advanced to `1a43f3a8f25b7e5fe1c238794e79bad11e4c4788` through merged PR 7853. Merge-down commit `be66c959f25f4910520785d9232faba4ddfa7f58` incorporates its CI/mirror changes. An exact Git diff from measured code b3 to that merge found no change under sdk-rs or packages/safe-cex except this evidence sheet; the parity package commands are unchanged. These are b3 gate measurements, not claims of new runs at the merge or documentation commit. The post-merge tomllib census and ten-row scope audit exited zero with unchanged counts/status/rs pointers.

## Scope and rulings

Exactly ten assigned rows in six groups were reviewed. TypeScript production files are unchanged. Nine retain `divergent` status with a CHOSEN ruling; calculateOrdersRisk returns to ported because its blockers were already repaired. Zero pending rows move. Removing the illegal AssetClass marker also moves that sibling type row from divergent to ported. The tomllib census is 4802 ported / 2215 divergent / 317 pending / 7 not-applicable, total 7341; this is a census, not execution proof.

| Rows | Repair or ruling | Canonical reference |
| --- | --- | --- |
| OKXExchange.findOrderByClientOrderId | Shared TableHit reaches the explicit mapper state argument; mapper error/empty outcomes precede output refusal. Nonterminal states retain mapper status. Store-hit copy remains explicit. | [closed output](MANIFEST-SPEC.md#closed-typed-output-boundary), [ownership](MANIFEST-SPEC.md#owned-snapshot-vs-live-alias) |
| okxBillCategory; mapOkxBillToIncomeRecords | Remove Other substitution for inherited hits. Refuse category-bearing output while retaining empty/fee-only success and earlier source effects/errors. | [closed output](MANIFEST-SPEC.md#closed-typed-output-boundary) |
| recordMessageHandlerStats; reconcileMessageHandlerStats; cleanupMessageHandlerStats | Existing inherited lookup behavior remains. Process-wide inherited-referent writes and cross-socket alias effects remain deliberately omitted to isolate socket hosts. New cleanup witnesses distinguish native success from source prototype errors. | [taxonomy](MANIFEST-SPEC.md#divergence-taxonomy) |
| CandleCacheChunkRecord | Whole-entry capture repairs negative zero/overflow loss. Bounded capture of ignored extras remains an explicit choice. | [numeric ingress](MANIFEST-SPEC.md#numeric-ingress-defect) |
| bybitInstrumentClass | Remove PrototypeProperty widening; retain the inherited branch, read source metadata, then refuse the closed classified output. Classifier/registry/resolve propagate Result. | [closed output](MANIFEST-SPEC.md#closed-typed-output-boundary) |
| initBroadcastChannel | Whole-entry tagged ingress repairs number loss/refusal. Retains bounded capture and existing runtime-scoped ownership. | [numeric ingress](MANIFEST-SPEC.md#numeric-ingress-defect), [taxonomy](MANIFEST-SPEC.md#divergence-taxonomy) |
| calculateOrdersRisk | Recovery and profile-ordering blockers were stale. Current shared serializer already matches all three source field-order vectors. Pure utility values/order and catchable prototype failures are measured; the separate worker fixture still expects the old defect and fails before its recovery checks. | [taxonomy](MANIFEST-SPEC.md#divergence-taxonomy) |

During the original orphan sweep, no live-owned finding row was edited. The later explicitly assigned canonical-indexing follow-up changes worker storage/import layout and expands existing Lighter trait impls without changing their FINDING rows. Registry function signature propagation does not claim worker registration or a live risk pipeline.

## Witness floors

Each enumeration stops at the stated stock-prototype/corpus floor; custom or mutated prototypes are not an exhaustive claim.

- Cache: seven fields × seven numeric spellings × three ingress routes = 147 native bit comparisons; final case is volume / 5e-324 / borrowed. Seven rejected shapes and one 140-level ignored-extra source/native depth distinction supplement them.
- Broadcast: six spellings × three routes = 18 native bit comparisons, plus native Value negative-zero control; final route is 5e-324 / borrowed. Tagged-shape and 140-level ignored-extra checks supplement them.
- Instrument: twelve source inherited identities, eleven callables plus the prototype object, with twelve native refusals before wire observations; the final member is toLocaleString. Six mapped labels × four venues = 24 native class comparisons. Existing 114-case description fixture is retained: 102 ordinary comparisons and twelve explicitly separate refusals.
- Income: twelve inherited identities × five bill modes = sixty native outcomes; final member/mode is toLocaleString / error. Empty, fee, pnl, adjustment and coercion precedence are separate branches.
- Lookup: the 158-case REST-orchestration corpus also includes placement controls; final case 157 is lookup state {toString:null}. It includes 38 inherited mapper inputs and fourteen inherited status refusals. Native member expectations are independent literal variants, and statuses use explicit native enum constants. Callback errors, empty results, array property keys and throwing coercion are included. Source store alias and native owned-copy mutations are separately witnessed.
- Socket stats: 78 inherited/ordinary and own-shadow profiles, with 78 native cleanup comparisons. Source peer alias identity and inherited writes surviving deletion precede projection. The twelve inherited names and ordinary control are the enumeration floor; profile 77 is ordinary / two own handlers / own stats.
- Risk: twelve members × two access points = 24 native error identities; the final member/access point is toLocaleString / id. Pure utility corpus reaches risk fixture 103 of 104 and field-order vector 2 of three, plus the distinct cumulative-field identity supplement. The actual-processor source corpus has 96 failure/reporting profiles through index 95, 25 reachability controls through index 24 and three ordering cases through index 2; its native fixture is blocked by its obsolete assertion before recovery checks.

## Execution

Pre-control measured code SHA: `9e21470def0933ef1a290b4e44d5fc3293cfcd62`. Control SHA: `434283714ff8da97574c6ccf21695234d78e12ae`. Compilation and final target gate SHA: `b3fe22d053ebfce9ed885ce812ff082b6a1f4fb3`, which adds only the root registry fixture Result unwrap; production source is byte-identical to the control SHA.

| Gate | Exit | Scope |
| --- | --- | --- |
| `cargo test -p safe-cex-venue-okx --test income_inherited --jobs 1` | 0 | One test, sixty outcomes; restored after numeric wire spelling correction |
| `cargo test -p safe-cex-exchange --lib websocket::tests::live_ws_typescript_oracle --jobs 1 -- --include-ignored` | 0 | One test, including 78 stateful cleanup profiles |
| `cargo test -p safe-cex-store --lib risk::tests::prototype_bucket_errors_are_catchable --jobs 1` | 0 | One test, 24 named access-point checks |
| `cargo test -p safe-cex-store --test derived --test risk_identity_supplement --jobs 1 -- --include-ignored` | 0 | Three tests; 104 risk cases, three key-order vectors, direct cumulative-field identity |
| `cargo test -p safe-cex-webworker --test risk_postprocessor actual_processor_reports_and_explicitly_clears_prior_risk --jobs 1` | 101 | Auxiliary live-owned fixture expects obsolete positionSize-before-text ordering. It stops on the first seed before report/clear/notification checks; no native recovery execution claim |

Final native gates at `b3fe22d053ebfce9ed885ce812ff082b6a1f4fb3`:

| Gate | Exit | Scope |
| --- | --- | --- |
| `cargo check --all-targets --jobs 1` | 0 | Only safe-cex-candle-cache, safe-cex-exchange, safe-cex-store and safe-cex-venue-okx, with their dependencies |
| `cargo clippy --all-targets --jobs 1` | 0 | Same four crates, unchanged strict workspace lint table |
| `cargo test -p safe-cex-store --test root_utils root_instrument_registry_keeps_four_native_entries_and_classifiers --jobs 1` | 0 | One selected test; four independent native registry classifications |
| Selected native integration targets, `--jobs 1 --no-fail-fast -- --include-ignored` | 0 | 22 tests across numeric_ingress, field_identity, numeric_ingress_findings, instrument_class, mappers, rest_operations and income_inherited; only the same four crates |

All eleven compiling controls below ran at `434283714ff8da97574c6ccf21695234d78e12ae`; each exited 101 at the named assertion and then 0 after exact source restoration. The runner additionally required a finished test build, one selected test and FAILED. No surviving mutation was observed. Each negative run stops at its first named assertion, not at an implied complete mutation enumeration.

| Control | First stopped case / assertion | Mutated / restored |
| --- | --- | --- |
| Cache Value rebuffer | lastAccessed / -0 / owned: `cache::native source bits` | 101 / 0 |
| Broadcast tagged decoder bypass | -0 / raw: `proxy::native source bits` | 101 / 0 |
| Inherited asset fallback | constructor: `instrument::inherited classified output must refuse` | 101 / 0 |
| Income category Other substitution | constructor: `income::helper inherited output must refuse` | 101 / 0 |
| Eager bill refusal | constructor / empty: `income::empty inherited bill must succeed` | 101 / 0 |
| Lookup fabricated live input | constructor: `lookup::native mapper override` | 101 / 0 |
| Lookup constructor/toString identity swap | constructor: `lookup::native mapper override` | 101 / 0 |
| Lookup forced Open status | case 149, live with Closed mapper result: `lookup::native mapped status` | 101 / 0 |
| Cleanup keeps original own entry | constructor own-shadow profile: `stats::native cleanup deletes own entry` | 101 / 0 |
| Risk symbol guard omission | constructor / symbol: `risk::prototype symbol must return TypeError` | 101 / 0 |
| Risk loss-side field-order flag omission | next / profile 0: `RISK-PROFILE source branch field order` | 101 / 0 |

Command: `python3.11 sdk-rs/parity/scripts/orphan-finding-controls.py <control> --cargo cargo`, with SAFE_CEX_TYPESCRIPT_PATH supplied. The eleven explicit control names are cache, broadcast, instrument, income-category, income-bill, lookup-input, lookup-member, lookup-status, stats-cleanup, risk and risk-order.

Remaining measured gates at `b3fe22d053ebfce9ed885ce812ff082b6a1f4fb3`:

| Gate | Exit | Scope / result |
| --- | --- | --- |
| `cargo build -p safe-cex-venue-okx --target wasm32-unknown-unknown --jobs 1` | 0 | OKX library and dependencies, including all four changed crates; no wasm execution or wasm Clippy claim |
| `yarn parity:extract:check` | 0 | 7341 symbols / 512 files; would update zero manifests, zero added/orphaned symbols; output text inspected |
| `yarn parity:check` | 1 | Indexer rejects item macro thread_local in safe-cex-webworker/src/chaser_manager.rs before global target validation |
| `yarn parity:report` | 1 | Same indexer failure; no canonical report produced |
| `yarn parity:progress` | 1 | Same indexer failure; no canonical progress report produced |

The final extractor writer after the risk manifest edit also reported zero updates, additions or orphans. Array targets, row identities, non-note/status fields and file metadata were preserved, independently checked by the reviewer. The initial thirteen touched Rust files passed `rustfmt --check --edition 2024` at the control SHA; the added root registry fixture passed the same command at b3. Root diff checks exited zero.

The reviewer reproduced the canonical parity-check failure at exact integration parent `ac0f5fccf24207b26c279e67da2a88f2c72a7aae` using the existing real checker and its supported --workspace option against a materialized separate checkout: actual process exit 1 with the identical diagnostic. The worker source has the same Git blob at parent and b3, `95fc38286934e5a249f2da9d791ef35d954647f4`. At the original b3 measurement no worker or indexer source was changed and no gate was weakened; the subsequently assigned indexer repair is recorded below. This establishes an inherited indexer blocker, not a passing canonical gate. The reviewer separately inspected definitions and containing impl scopes for all sixteen distinct rs paths in the ten assigned rows; each exists with a substantive body/type. That finite manual audit does not establish global canonical resolution or ownership validation.

Original orphan-sweep commands use `--manifest-path sdk-rs/Cargo.toml`; source oracles receive the explicit TypeScript compiler location through SAFE_CEX_TYPESCRIPT_PATH without a machine-specific path in this record. Those original Cargo operations were sequential with one job. This sheet is a manually authored record; generated logs remain untracked under target.

At the original orphan-sweep measurement, not run: workspace compilation or tests, complete venue/crate suites, wasm Clippy or wasm execution, browser/live-account integration, and a new clean-install dependency-isolation proof. The separately requested follow-up measurements appear below. Existing shared oracle loaders and coercion implementations remain dependencies.

## Audit history

Final evidence-only commit `dedcdb98d70f79fd50359a9dd98fcefa40d7aecb` received separate-checkout sub-agent MERGE with no concrete blocker. That review retained both the inherited canonical failure and auxiliary native recovery gap explicitly. The later merge-down adds no SDK/source changes as checked above. A post-merge audit query initially exited 1 by indexing an empty manifest without a symbol array; using the schema-appropriate empty default produced the successful census and scope audit, without changing manifests.

The first four-crate all-target check at `434283714ff8da97574c6ccf21695234d78e12ae` exited 101 at the root registry fixture still expecting the old classifier return type. The follow-up `b3fe22d053ebfce9ed885ce812ff082b6a1f4fb3` updates that ordinary native expectation to unwrap the catchable Result. An initial direct rustfmt invocation also exited 1 because it incorrectly requested edition 2021; the workspace-edition 2024 check exited 0 without edits.

The first cache numeric attempt exited 101 because Node interpreted a negative numeric argument as an option; the corrected separator and restored numeric/field-identity target passed. An instrument test compile attempt exited 101 on a temporary borrowed expected JSON value; the owned expectation fixed the harness. Neither is a discriminating mutation proof. The first combined native batch later exited 101 only on income wire integer/float spelling after its native assertions passed; numeric-only wire canonicalization and direct assertions for every native fee field restored the sixty-outcome target to exit 0.

The source __proto__ witness initially confused Object.prototype itself with reading its accessor on Object.prototype, which returns null. The witness now checks the actual inherited referent. A later audit found the lookup identity expectation reused production's helper; independent twelve-member expectations replaced it before mutation controls. No compiling survivor was observed in those audits.

Adversarial sub-agent review used a separate checkout. Reviews at 7c539c6f93, 3574ad2603 and 712a62af33 found no remaining concrete blocker after the recorded fixes; MERGE was conditional on root execution. Reviewer metadata preservation, Python syntax/mutation-anchor and diff checks exited 0. The reviewer did not compile or execute Rust/source/mutation gates. Review at `434283714ff8da97574c6ccf21695234d78e12ae` independently rechecked the 4802/2215/317/7 census, metadata preservation and eleven available failure/restoration log pairs; MERGE remained conditional on root compilation and final parity gates. It did not independently remeasure process exits or their SHA attribution. Shared coercion remains a delegated dependency, not a complete new proof of ECMAScript coercion.

The first review also trusted the worker fixture's stale ordering premise. Root execution exposed that assertion; current serializer construction and the independent pure-utility oracle disproved the premise. The utility ruling was corrected to ported. The original risk ruling edited no worker or shared-profile type file; the follow-up worker changes below concern only singleton storage and startup imports.


## Canonical indexing follow-up

The overseer subsequently assigned this lane the inherited indexer failure and explicitly requested the locked workspace build, host Clippy and configured wasm lane. Ownership was posted on PR 7854 before changes. The original measurements above remain historical measurements, including their failed gates.

The new production-source regression at `2560dd5d82` failed with exit 101 at `CHASER production source index` against the old worker layout. Commit `84a666b071` moved the private chaser thread-local key into its non-generic sole accessor; its restored source regression and the 65-test gate file exited zero. The TWAP key subsequently received the same scope change. Repeated native calls retain pointer-identical singleton handles and fresh managers remain independent; their actual execution is recorded below when complete.

Once the first macro was removed from module scope, canonical indexing exposed imported inherent owners in worker startup. The indexer now proves only unconditional direct same-crate struct/enum/union origins, retaining impl-site public methods and their invention diagnostics. File/module conditional ancestry, duplicate imports/declarations, indirect aliases and cross-root origins remain guarded; startup's glob became explicit imports. Self-review caught the need to carry inline-module namespaces into deferred owner proofs, and added the matching positive and negative witnesses before delivery. A 69-test development gate at `c54d4a5287` exited zero.

The next module macro was Lighter's seven transaction trait impls. Those exact impl bodies were written explicitly and the unused macro definition/calls removed. Separate-checkout review independently checked all seven original type/kind bindings and surrounding-source preservation. That implementation step changed no manifest or exemption; a later owned-note correction is recorded below. The maintained-source test independently names eight public transaction types, stopping at AnyTxInfo, and checks their invention diagnostics.

At `fd3d826a2f`, canonical source indexing completed and validation returned exit 1 with 5911 diagnostics: 2152 status and 3759 invention diagnostics, ending at safe-cex-webworker/src/worker_session.rs::WorkerTable. No canonical report was produced and these live-owner manifest claims were neither rewritten nor bypassed.

The initial owner-conditional mutation at `632782fd63` exited 101 before compilation finished because it removed the only field read, producing denied dead_code; the runner rejected it (compiled=false, caught=false), restored source and obtained exit zero. It is not a discriminating control. The corrected mutation consumes the field while forcing acceptance. The first Lighter control at ff467a8e54 caught the macro regression but its restored fixture failed because the source's super import was placed at crate root. That rejected run is not a passing control. Commit 336595c268 puts the maintained source in its actual child-module context and the corrected control passes.

All seven follow-up controls below compiled and reached their named assertions with exit 101, then restored exact tracked bytes and passed with exit 0. Each runner exited 0. The runner refuses to count a compilation failure or an unsuccessful restoration.

| Control | Measured source SHA | Named assertion / stop | Mutated / restored |
| --- | --- | --- | --- |
| Restore module-level chaser macro | `632782fd6316abbce05fc944cbc007f2d5c1813c` | CHASER production source index | 101 / 0 |
| Break expected accessor target | `632782fd6316abbce05fc944cbc007f2d5c1813c` | CHASER public accessor indexed | 101 / 0 |
| Restore module-level TWAP macro | `632782fd6316abbce05fc944cbc007f2d5c1813c` | TWAP production source index | 101 / 0 |
| Accept conditional owner import | `ff467a8e54fa1f138d0d9836105f20dc6b5692e9` | OWNER uncertain origin must refuse; cfg(unix) import | 101 / 0 |
| Drop inline owner namespaces | `ff467a8e54fa1f138d0d9836105f20dc6b5692e9` | OWNER uncertain origin must refuse; root nominal vs nested external owner | 101 / 0 |
| Restore Lighter declaration macro | `336595c2688ef161e9ff322125aca8b3c9d7f83a` | LIGHTER transaction source index | 101 / 0 |
| Return a fresh chaser manager | `336595c2688ef161e9ff322125aca8b3c9d7f83a` | CHASER native singleton identity | 101 / 0 |

Command: `python3.11 sdk-rs/parity/scripts/orphan-finding-controls.py <control>`, using the explicit names indexer-scope, indexer-assertion, twap-scope, owner-conditional, owner-namespace, lighter-declarations and singleton. The negative owner corpus has twelve literal source cases plus a conditional-file case; it stops on that file case in the unmutated test. No compiling mutation survivor was observed.

At source SHA `336595c2688ef161e9ff322125aca8b3c9d7f83a`, the extractor check exited 0 and its output reported zero manifests to update and zero added/orphaned symbols. A recursive tomllib census exited 0: 512 manifests, 4802 ported / 2215 divergent / 317 pending / 7 not-applicable, total 7341. Initial census queries used a nonrecursive glob and then the wrong plural table key; their empty counters were rejected, not reported as census results. This indexing follow-up moves zero rows.

Follow-up delivery measurements at `336595c2688ef161e9ff322125aca8b3c9d7f83a`, using Rust 1.99.0 and Node 24.20.0:

| Gate | Exit | Measured scope |
| --- | --- | --- |
| `cargo test --locked -p safe-cex-parity --test gates --jobs 1` | 0 | All 71 tests in the touched gate file |
| `cargo test --locked -p safe-cex-webworker --test chaser_manager_singleton --jobs 1` | 0 | Both native singleton tests |
| `CARGO_BUILD_JOBS=1 cargo build --workspace --locked` | 0 | Whole workspace, default host build; completed in 46m 21s |
| `rustfmt --check --edition 2024` on the eight changed Rust files | 0 | Source and test formatting, including recursive module checking |

The Cargo commands above ran from sdk-rs. The later evidence-only commit d26bc1bc09 changed no build inputs; these measurements remain attributed to 336595c268. The delivery pipeline is sequential. During the earlier c54 development iteration, a canonical command was started before the preceding 69-test process had been confirmed complete; both subsequently completed, and that overlap was acknowledged. It is not represented as a sequential run here.

Host Clippy and the configured wasm lane have not completed at this evidence step; no exit is claimed for them yet. Canonical indexing also exposed a missing exact-target declaration in the owned OKX lookup note; that annotation correction and final canonical measurements follow separately.

## Integration refresh

Integration advanced during the broad run. The unfinished host Clippy attempt was interrupted after verifying that its subprocesses were actively checking integration-test targets; no exit or pass is claimed for that attempt. Its queued wasm stages had not started. Completed measurements above retain their original source SHAs.

Merge commit `a8a0a5b9eb4ca74d51b847f489243937ab9222f1` incorporates integration `0445a753756f197e4363222eab9e787868e4f700`, including crypto, signing and dependency changes. The merge had no conflicts. Separate-checkout sub-agent review found no overlapping changed path or new production item macro/imported-owner hazard; the reviewer ran no Cargo or checker. This does not transfer the earlier broad result to the merged revision.

The owned OKX lookup note now names its existing exact free-function target and explains the explicit mapper context. Its rs, status, row identity and canonical boundary anchors are unchanged. No invention exemption is added. A post-merge recursive tomllib census exited 0 with 512 manifests, 4788 ported / 2229 divergent / 317 pending / 7 not-applicable, total 7341. The integration refresh changed that census; this indexing repair moves zero pending rows.

## Merged source execution

Measured source SHA: `3a72e9229041df133f970c8f5b76da2540ac9b1d`, with integration parent `0445a753756f197e4363222eab9e787868e4f700` merged before the source push. The independent git-archive/tomllib census of that parent is 4786 ported / 2231 divergent / 317 pending / 7 not-applicable. The branch's original orphan repair remains +2 ported / -2 divergent; the indexing follow-up moves no rows.

These merged-source gates run sequentially, with one Cargo command at a time. Direct Cargo commands use one job. The unchanged wasm helper explicitly supplies `--jobs 2` for its full-policy build and Clippy, overriding CARGO_BUILD_JOBS=1; those two real gates use the configured two jobs.

| Gate | Exit | Measured scope / result |
| --- | --- | --- |
| `yarn parity:extract:check` | 0 | Output inspected: zero updates, additions and orphans across 512 manifests / 7341 symbols |
| `yarn parity:check` | 1 | Indexing completes; 2151 status and 3761 invention diagnostics |
| `yarn parity:report` | 1 | Same 5912 validation diagnostics; no canonical report produced |
| `yarn parity:progress` | 1 | Same 5912 validation diagnostics; no canonical progress report produced |
| `CARGO_BUILD_JOBS=1 cargo build --workspace --locked` | 0 | Merged-source default host workspace build, completed in 4m 13s |
| `CARGO_BUILD_JOBS=1 cargo clippy --workspace --locked --all-targets` | 0 | Whole host workspace, all targets; completed in 39m 39s |
| `node --test ../.github/scripts/rust-wasm.test.mjs` | 0 | Fourteen policy/workflow/helper tests; real wasm build and Clippy are separate gates |
| `CARGO_BUILD_JOBS=1 node ../.github/scripts/rust-wasm.mjs --build` | 0 | Actual wasm build of all 38 classified crates, including worker and FFI; completed in 5m 20s |
| `CARGO_BUILD_JOBS=1 node ../.github/scripts/rust-wasm.mjs --clippy` | 0 | Actual all-target wasm Clippy for the 38 classified crates and configured FFI feature; completed in 53m 52s |
| `CARGO_BUILD_JOBS=1 cargo build --locked -p safe-cex-exchange --target wasm32-unknown-unknown` | 0 | Exchange default-feature wasm build |
| `CARGO_BUILD_JOBS=1 cargo build --locked -p safe-cex-exchange --all-features --target wasm32-unknown-unknown` | 0 | Exchange all-feature wasm build |
| `CARGO_BUILD_JOBS=1 node safe-cex-ffi/scripts/setup.mjs` | 0 | Maintained setup verified the installed locked wasm-bindgen tool |
| `CARGO_BUILD_JOBS=1 node safe-cex-transport/scripts/warning-wasm.mjs` | 0 | Actual compiled wasm executed under Node: warning output, deduplication, JavaScript reentry, relay overlay, tunnel endpoints and direct-route assertions passed |
| `CARGO_BUILD_JOBS=1 cargo build --locked --target wasm32-unknown-unknown -p safe-cex-types -p safe-cex-num -p safe-cex-store -p safe-cex-crypto` | 0 | Retained four-crate core wasm build |
| `cargo test --locked -p safe-cex-parity --test gates --jobs 1` | 0 | All 71 tests in the touched gate file, rerun after integration refresh |
| `cargo test --locked -p safe-cex-webworker --test chaser_manager_singleton --jobs 1` | 0 | Both native accessor identity tests, rerun after integration refresh |

The owned OKX lookup status diagnostic is absent after its exact-target note correction. Compared with the historical fd3 validation, the refreshed integration adds invention diagnostics for safe-cex-crypto/src/starknet.rs::hash_typed_data and safe-cex-crypto/src/starknet.rs::sign. The observed diagnostic enumeration ends at safe-cex-webworker/src/worker_session.rs::WorkerTable; the first status diagnostic is CrossTradeManager#_failureKey. These are manifest validation diagnostics, not a census of proven behavioral defects. No live-owner mapping is rewritten to bypass them.

The merged-source note and metadata received separate-checkout sub-agent review with no concrete blocker. The reviewer independently checked all eleven changed row identities using (ts, kind), preserved targets/file metadata/other fields, and confirmed that only AssetClass and calculateOrdersRisk change status. An initial ts-only identity query folded a kind-distinct row and was discarded before those conclusions. The reviewer ran no Cargo, mutation or checker.

The completed validator also exposed annotations for the two accessors repaired in this assignment. The existing CHOSEN chaser singleton row used a legacy crate-style rs instead of its declared file path; the existing CHOSEN TWAP singleton note lacked its exact target token. Chaser now points to safe-cex-webworker/src/chaser_manager.rs::chaser_order_manager_instance. Both notes name their actual targets, retain the lazy per-thread versus eager TypeScript export rationale, and cite the taxonomy anchor. Neither row was a live-owned FINDING, and their statuses are unchanged. Separate-checkout review verified the maintained TypeScript exports and real Rust bodies, exactly chaser rs/note plus TWAP note changed, and all other fields/file declarations were preserved. Reconciliation after those edits reported zero updates, additions or orphans.

Those annotation edits occurred after the 3a72 canonical measurements above. They change no Rust, Cargo or wasm-policy build input; the running broad gates remain attributed to that Rust input revision. Updated canonical/extractor measurements belong to the subsequent committed annotation revision and are recorded separately, rather than transferring the 5912 count to it.

The annotation revision is `52d4d64d64804814ffc7132f9801c7162d9bf21c`. Exact Git diff against 3a72 lists only this manual evidence sheet and the two singleton manifests. The host Clippy run completed with those Rust inputs unchanged; no later commit is presented as a new broad-gate execution.

## Annotation validation

Measured annotation SHA: `0828120734aa537a0e70c5e609a9221f392de579`. Its Rust/Cargo/wasm-policy inputs are unchanged from 3a72. The following canonical runs use the maintained binaries built by the recorded workspace build, directly with their supported workspace option; they are not new Yarn wrapper measurements.

| Gate | Exit | Measured result |
| --- | --- | --- |
| `yarn parity:extract:check` | 0 | Output inspected: zero updates, additions and orphans; completed in 19.31s |
| `sdk-rs/target/debug/parity-check --workspace sdk-rs` | 1 | Indexing completes; 2147 status and 3760 invention diagnostics |
| `sdk-rs/target/debug/parity-report --workspace sdk-rs` | 1 | Same 5907 validation diagnostics; no canonical report produced |
| `sdk-rs/target/debug/parity-progress --workspace sdk-rs` | 1 | Same 5907 validation diagnostics; no canonical progress report produced |

The two singleton annotations remove five diagnostics: the chaser legacy target's file-declaration, rename and unresolved-target diagnostics, the chaser public accessor invention diagnostic, and the TWAP exact-target rename diagnostic. Both rows retain their existing divergent statuses. This is zero status or pending-row movement. The remaining enumeration starts at CrossTradeManager#_failureKey and ends at safe-cex-webworker/src/worker_session.rs::WorkerTable. No remaining singleton diagnostic was observed in these three outputs; the global validation gates remain failed.

At evidence revision `96b467d9069e11d8c97da8d1aa07fa27599d1427`, a fresh recursive tomllib census exited 0 with 512 manifests and 4788 ported / 2229 divergent / 317 pending / 7 not-applicable, total 7341. Separate-checkout sub-agent review found no concrete blocker: the reviewer read the raw logs, independently counted the five removed diagnostics and confirmed unchanged Rust/Cargo/wasm-policy inputs. The reviewer did not rerun processes or independently measure their exits or SHA attribution. The later wasm Clippy completion is a root measurement; its completed-log review is recorded below.

## Delivery scope

The sequential merged-source pipeline has returned an actual exit for every queued command; none remains running. The locked SDK workspace build, host all-target Clippy and every command in the configured wasm job passed. The wasm helper's real all-target Clippy and its fourteen-test policy audit are distinct measurements. The two focused Rust test targets passed with 71 and 2 tests respectively. Canonical check/report/progress remain exit 1; their source indexing completes, but the recorded manifest diagnostics prevent reports. No global canonical success is claimed.

These are local executions of the maintained job commands. Not run: full Rust or monorepo test suites, the separate full FFI smoke job, browser or live-account integration, or clean-install/CI-home dependency isolation. The earlier auxiliary worker recovery fixture failed at its recorded source SHA before its recovery assertions; this PR claims no new native recovery pass. Mutation/control measurements retain their individual source SHAs above, rather than being attributed to the later integration merge or evidence commits.

Final separate-checkout sub-agent review at `e604fe46e030db794cf817c8ad0a775db8f0aa56` returned MERGE with no concrete blocker. The reviewer inspected the completed logs, independently counted sixteen table exits (thirteen zero and three canonical validation failures), checked source preservation and individual measurement attribution, and obtained diff-check exit 0. Its only earlier wording finding was the helper's configured two jobs; that correction precedes this review. The reviewer ran no Cargo or mutations and did not independently remeasure process exits. This subsequent evidence-only record changes no build or validation input.
