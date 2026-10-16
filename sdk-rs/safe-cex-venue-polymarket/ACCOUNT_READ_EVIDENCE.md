# POLYMARKET account REST bodies: eight rows

Fresh branch `feat/polymarket-account-reads-d4b45c` started directly from integration **d4b45c92c11acf2005262f792d26ac2a56a7cf05**. No unreviewed branch was used as its starting parent. Implementation: `f26f6c54b4`. First integration merge: **c33a28fc07c4d412c2bde807737ca1be1f4b2d24**, including integration `0643f8d26dbecd42875c88561dddf96d045874f1`. Manifest fix: **f66c36b0f3b686ef7ddb8edad188426945dc892d**. Mutation driver fix: **5c17992cffbdf1d8b3e25fcca3bb9dfec0ec28ff**. Final integration merge: **7872efa420df08db4226c4458fe75989de0ece75**, including **9fad5d8d1bbb15b743af7af172cbcc5c69238a5a**. Evidence files subsequently added without changing implementation inputs.

## Rows and scope

Eight pending -> ported, zero -> divergent: API `postOrder`, `postOrders`, `getOpenOrders`, `getTrades`, `getGammaMarkets`, `getPositions`; exchange `getAccount`, `validateAccount`. Independently parsed seven venue manifests: **83/18/40 -> 91/18/32** (ported/divergent/pending), 141 rows. This is a venue-only census, stopping at `PolymarketPublicWebSocket#handleOrderBookUpdate`, the last symbol of `polymarket.ws-public.ts.toml`; it is not a repository census or completeness proof. See `evidence/account-reads/metadata-gates/census.json` for every changed row and actual targets.

`account_reads.rs` contains the actual callable method bodies over current receiver/child methods and runtime intrinsics. Credentials precede submission; the clob-client serializer receives the original single order; batch mapping preserves captured length, live slots, holes, fresh rows and nested order references. Array-map allocation/species and CreateDataProperty are bound separately from ordinary parameter Set. Cursor arrays return directly, including bypass of the trade limit and replacement of earlier accumulated pages; envelopes append original elements and truncate after append/cursor access. Gamma defaults are nullish, flags remain copied, and offsets accumulate. Holdings use fixed500 pages with sizeThreshold0. Account validation re-reads credentials after derivation and captures the logger before error-message coercion.

Whole async tasks start eagerly and survive observer abandonment. Await-ready child operations yield; returning ordinary data introduces no extra await. Return resolution delegates thenable assimilation. Factory/class composition, Axios/interceptors/retry/transport, wallet/derivation and `orderToJsonV2` implementations remain dependencies or separately pending rows. No signing, serialization or network implementation is claimed.

## Measured gates, with their actual SHAs

| SHA | Gate | Exit / observation |
|---|---|---|
| c33a28fc07 | Actual eight-method source oracle | 0;96 fixtures plus four named scheduling/lifetime witnesses |
| c33a28fc07 | Scoped cargo check, all-targets, locked, jobs2 | 0 |
| c33a28fc07 | Full POLYMARKET cargo tests, locked, jobs2 | 0;103 passed,0 failed,0 doc tests |
| c33a28fc07 | Scoped cargo fmt check | 0 |
| c33a28fc07 | Scoped official target checker, empty exemptions | **1**; missing31 public trait members and exact auxiliary rename note |
| f66c36b0f3 | Corrected official target checker | 0;141 rows,289 public Rust items |
| f66c36b0f3 | Actual parity:extract Writer | 0;Updated0/add0/orphan0 output confirmed |
| f66c36b0f3 | Actual parity:extract:check | 0;would-update0/add0/orphan0 output confirmed |
| 7872efa420 | Fresh scoped check, all-targets, locked, jobs2 | 0 |
| 7872efa420 | Fresh official target checker | 0;141 rows,289 public Rust items,empty exemptions |
| 7872efa420 | Fresh scoped fmt check | 0 |
| 7872efa420 | Fresh actual parity:extract:check | 0;would-update0/add0/orphan0 output confirmed |

Exact commands, exits, stdout/stderr and code identifiers are tracked under `evidence/account-reads/`. The source corpus is losslessly compressed as `initial-gates/source-oracle.json.gz`, with its uncompressed hash/size beside it. All Node oracle invocations used `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`. Yarn output text was checked in addition to its measured exit.

The initial target failure was corrected by mapping the real AccountHost trait and all31 members (two associated types and29 methods) to bodies that use those bindings, with exact auxiliary targets in each rename note. No Rust item, exemption or source class was fabricated. Writer/extractor were not reached in the stopped initial gate runner; their later exits belong to f66c36b0f3, as shown above.

## Discrimination and native assertions

All **18 compiled controls** produced their **named native assertion**, mutation exit101, and exact restored pass0. Baselines passed12 tests at each run's actual SHA. Controls1..8 were measured at f66c36b0f3: owner fields for each submission, cursor, trade truncation, Gamma offset, holdings threshold, affiliate null and validation fallback. The first driver stopped after validateAccount (exit1) because its next ownership anchor matched two delimiters; mutation9 was never applied. The runner was corrected, not the test or production behavior.

Controls9..18 were measured at5c17992cff: whole-method ownership, ready-await yield, immediate-return scheduling, return resolution, direct-array alias, inherited setters, map length, logger capture, copied Gamma flags and nested-order original pointer. The resumed driver exited0 and stopped after **boundary.NestedOrderAlias**. No behavioral survivor or wrong-runtime-failure result occurred. Both stopped and resumed summaries/raw logs are retained; do not describe the first driver as exit0.

Native assertions inspect decoded object fields/own keys, UTF-16 units, Number bits/NaN, original Rc references, error identity, independent page/result/request counts and effects before source projections. The floor is96 family calls (eight bodies x12, each ending index11), plus four independent native witnesses,100 roots. Actual-source enumeration ends fixture95 validateAccount/11, followed by four independent witnesses. The Rust harness may execute tests concurrently; no global witness execution ordering is claimed. Pagination fixtures reach three pages; fixed-size holdings fixtures return500+500+1. Invalid Gamma limits deliberately reach an injected second-request rejection to end the oracle; the production body retains the source loop without a new guard. Array-map fixtures capture length3, skip index1 when deleted and ignore appended index3.

This is a finite method-body floor. It does not prove arbitrary proxies/getters/coercions, every custom iterator/species/thenable, complete alias-graph reconstruction, dependency implementations or assembled SDK classes. Native pointer assertions relate each returned/mapped node to its actual original native input/response node. Static native sub-agent review caught scheduling, Set, copied-field/key, nested-pointer and logger/severity issues, all corrected before final controls; it executed no gates. The gate runner's clean-input audit is external, with its measured receipt tracked, rather than enforced by that runner.

## Preservation and limits

The manifest-only fix preserves31 code/input paths from c33a28fc07 (excluding the changed manifest directory), exit0. The later driver-only fix preserves production Rust, native tests, actual-source oracle, manifests and lock; its changed Python anchor/resume behavior is explicitly recorded. `code-blobs.json` proves identical implementation/native-test/oracle blobs at all four proof points.

Final merge audit: strict32-input diff exit**1**, not0. **28 paths are byte-identical**, exit0. The four remaining files were checked precisely: only a foreign Lighter SDK workspace member and lock package were added (every existing package/dependency/checksum entry remains equal); only two tests were appended to a foreign KuCoin script; only a WEEX factory section and Orderly API/feed appendix were added to the Spec. An initial whole-Spec-prefix assertion failed because the WEEX section was inserted; that failure and corrected exact foreign-section comparison are retained. Global exemptions contain no POLYMARKET entry. Fresh scoped check/target/format/extractor gates ran at7872efa420; prior full-suite/control exits remain at their measured SHAs, with these preservation proofs. Full103 tests and18 controls were **not rerun at7872efa420**.

Not run: workspace/monorepo builds or suites, host or wasm clippy, CI, standalone dependency suites, live exchange/wallet/signing/transport/serializer tests. No claim that the whole venue or remaining32 pending rows is complete.
