# Coinbase create/cancel and order fallback bodies

Moved **8 pending -> ported, 0 -> divergent**. Python 3.11 tomllib enumerated through 16 Coinbase manifests and 426 rows at authoring parent `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a`: **329/17/80 -> 337/17/72** (ported/divergent/pending). Exact moved rows and real native targets are in `ORDER_CREATE_CENSUS.json`; scoped native mapping independently validated every target. Census is separate from behavioral proof. The fresh branch used reviewed integration directly, with no foreign unreviewed ancestor. The authoring parent did not yet contain PR 7537; its four rows are not counted as this slice's gain.

Mapped source field bodies: formatCreateOrder, buildOptimisticOrderFromCreate, placeOrder, cancelOrders, shouldPollOrders, mergeRestOrdersWithFreshWsState, emitOrdersFetchError and resolveMarketByCoinbaseProductId. The amount formatter adjusts once with amount precision, price/stop operations use price precision, and all source configuration branches/undefined-price text/caller-ID precedence remain. Optimistic orders preserve response-config override and numeric guards. Hot completion uses the existing independent local Promise driver, including unawaited/dropped results. Cancellation uses the actual installed p-iteration source await boundaries, including rejected callback/series joins. REST merging retains order handles and original array identity when no additions are needed.

Boundary: declared native PlaceOrderOpts/request/Market/Order fields, own genuine Market records, declared string response IDs, and representable plain JSON response/error values. Inherited non-Market prototype outputs, opaque host objects, callable getters and symbol properties are excluded. The existing resolver utility's divergent row is unchanged. The native inline optimistic Order carrier retains shared response-root identity, separate shallow info root, object/array field aliases and primitive copies. Root property replacement inserts a new field handle; object/array mutation uses the retained handle. Transport, timers, Store and caller-ID policies are explicit called operations. Full exchange assembly remains pending; separately mapped callees retain their own statuses; this slice does not claim their implementation or live venue execution.

## Measured gates

All measurements here cover **`27f13bed99d56af5baa4ff4e4bbd843419d99790`**. Commands, exits and log hashes are committed in `ORDER_CREATE_GATE_RESULTS.json`.

| Gate | Exit | Measured output |
| --- | ---: | --- |
| Scoped Coinbase Cargo check, all targets, jobs 2 | 0 | Finished dev profile; zero errors |
| Affected library/order_create/decoder tests, jobs 2 | 0 | 21 passed, 0 failed/ignored; three completed test binaries |
| Explicit Node/Yarn extractor check | 0 | 7341 symbols / 512 files; zero updates/additions/orphans; text inspected |
| Scoped native mapping | 0 | 426 rows / 555 public Rust items; real targets and status gates passed |
| Sequential mutation driver | 0 | Ten compiling assertion failures; ten restored passes |

Oracle environment: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Cargo operations ran sequentially with jobs 2. Extractor writer ran after manifest edits; native file inventory was serialized from actual target paths. Exploratory drafts had compiler/interface/lint errors and oracle setup gaps, corrected before the authoring SHA. Those dirty runs are not substituted for the passing SHA-bound measurements. Scoped check spent six minutes in macOS dynamic-library loading; it completed normally, without restart or source changes.

## Native proof floors

Native request branch presence, amount/price strings, side/stop enums, caller IDs, leverage and optional fields are asserted directly. Native optimistic enum fields, symbol, numeric bits, remaining amount, info values/aliases and lastUpdate are asserted before any wire assertion. Native returned IDs, arrays/market/order handles, polling state, errors and ordered host operations are asserted directly. Native outputs are never re-encoded or roundtripped. The live source error projection preserves cross-realm TypeError metadata.

The independent fixture plan stops at **42 formatter, 31 optimistic, 18 placement, 8 cancellation, 8 polling, 7 REST merge, 8 fetch-error and 7 market-resolution cases = 129**. The formatter floor consists of 24 type/side/TIF combinations, 8 price boundaries, 6 amount boundaries, 2 errors and 2 caller-ID cases. Fixture SHA-256 is `9be7b6c0ed22d306f089ca644551fabd526716d0ade50893a70821589018358e`; the native test pins content, not only length. Additional source probes stop at **2 dropped-completion cases**, **2 cancellation timing cases with one queued marker each**, and **1 shallow-alias/property-replacement/primitive-copy probe**. These are explicit floors, not exhaustive input or scheduler enumeration.

## Discriminating controls

| Compiling mutation | Named native assertion | Mutated / restored exit |
| --- | --- | --- |
| Amount uses price precision | native amount precision | 101 / 0 |
| Optimistic zero price accepted | decoded native optimistic presence | 101 / 0 |
| Wrong placement reconciliation reason | native reconciliation reason | 101 / 0 |
| Cancellation batch 100 -> 101 | native side effect count | 101 / 0 |
| Poll deadline 10000 -> 20000 | decoded native number bits | 101 / 0 |
| Freshness strict boundary becomes inclusive | decoded native merged order count | 101 / 0 |
| Rate-limit deadline 60000 -> 6000 | decoded native number bits | 101 / 0 |
| Remove USD -> USDC alias preference | decoded native alias market | 101 / 0 |
| Deep-copy shallow info object/array fields | native optimistic info object alias | 101 / 0 |
| Abandon producer after its first suspension | native unawaited producer continues through write check | 101 / 0 |

Enumeration stopped after these ten controls; no observed survivor. Each mutated run reached its named test assertion and exited 101, without compilation errors. Each restored run exited 0. The driver restores bytes in finally; independent source/test/script `git diff --exit-code` against the gate SHA exited 0.

Read-only subagent review found no remaining blocker within the explicit boundary, confirmed all eight functions, and checked hot completion, real helper timing, coercion metadata, response-root/shallow info identity and borrow-release order. The reviewer ran no Cargo, native tests or controls.

Not run: full Coinbase all-target test suite, workspace builds/tests, monorepo suites, host clippy, wasm clippy/builds, CI or live Coinbase network operations.

## Integration merge-down

Merged integration `65e086a3c9039a79e4c7e9f354aba31d8f83009e` into code SHA **`287bc8f9820a3b359a84e8269e94fdf65b9d047a`**. Kept both CLAUDE paragraphs and all exemption groups; TOML parsing/unique targets passed. The whole Coinbase source/test comparison to authoring SHA exited **1** because PR 7537 added the frame module/tests, scanner decoder visibility and decoder inventory. Shared transport implementation/configuration inputs also changed.

Reran affected gates at the merge SHA: scoped check **0**; library, order, decoder, frame/scanner/public validation tests **0** (**29 passed**, 0 failed/ignored, eight completed binaries); extractor **0**, zero-update text inspected; scoped native mapping **0** (426 rows / 569 public native items). Exact commands/exits/hashes and preservation comparisons are in `ORDER_CREATE_MERGE_GATE_RESULTS.json`. This is an affected test selection, not a full Coinbase all-target suite.

The order module, native order tests, fixture, live source oracle, mutation driver and local Promise test support remained byte-unchanged (`git diff --exit-code`, **0**). The ten authoring controls carry; controls were not rerun after the merge. Final tomllib enumeration stopped at 16 manifests and 426 rows against merged integration: **333/17/76 -> 341/17/68**, true gain **8 pending -> ported, 0 -> divergent**. PR 7537's four incoming rows are excluded from this gain.

Final delivery merge: integration `82274b95ecbae81c5c8b20c4d3f65d60cf64a360` is an ancestor of code SHA `91a6db64a5a43d4c4291281c10c49b7434b61fc4`. Whole Coinbase source/tests remain byte-unchanged from post-merge gate SHA `287bc8f9820a3b359a84e8269e94fdf65b9d047a` (diff exit **0**); scoped scripts, configuration, dependency code, source oracle and Coinbase manifests also compare unchanged (diff exit **0**). Only the lockfile's unrelated ApexOmni package adds the existing candle-cache dependency; Coinbase dependency resolution is unchanged. The scoped check, 29 affected tests and native mapping carry; no extra Cargo/controls rerun. Final extractor independently exited **0**, zero-update text inspected. Exact measurements are in `ORDER_CREATE_FINAL_GATE_RESULTS.json`. Final tomllib comparison against this actual merged integration SHA remains **341/17/68**, with **8 pending -> ported, 0 -> divergent**.
