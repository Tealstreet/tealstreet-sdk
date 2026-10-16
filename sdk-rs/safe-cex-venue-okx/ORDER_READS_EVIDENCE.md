# OKX order and position reads evidence

Eight pending rows become ported, zero become divergent. Actual fresh parent is 5911167261853a8cbcc99d79cf45b785bb016648; it contains the reviewed account-validation merge, with no unreviewed feature branch as an ancestor. Native source commit is 0f26ddbed5f5affc38affafeaae4710715eb0148; reviewed test corrections are at b8821a80e92b42a1187ab7697001edd3de0b4edc. KuCoin is untouched in this slice.

All targets below are in safe-cex-venue-okx/src/order_reads.rs. Normal/algo nested recursive callbacks map to two real native targets each; their ordered TOML arrays and provenance were produced with the manifest serializer. OrderReadOps and AwaitValue are supporting C3-exempt declarations, not additional moved rows. The parity writer ran after manifest edits and reported Updated 0 / 0 added / 0 orphaned; that writer invocation preceded the source commit and is not attributed to a committed SHA.

| Maintained TS symbol | Native target(s) |
| --- | --- |
| OKXExchange.fetchOrders | fetch_orders |
| OKXExchange.fetchNormalOrders | fetch_normal_orders; fetch_normal_instrument |
| OKXExchange.fetchAlgoOrders | fetch_algo_orders; fetch_algo_instrument |
| OKXExchange.fetchHistoricalOrders | fetch_historical_orders |
| OKXExchange.fetchClosedPositionRows | fetch_closed_position_rows |
| OKXExchange.fetchHistoricalPositions | fetch_historical_positions |
| OKXExchange.fetchClosedPnl | fetch_closed_pnl |
| OKXExchange.fetchIncomeHistoryRange | fetch_income_history_range |

## Measured gates

Cargo executable: /opt/homebrew/opt/rustup/bin/cargo. Every invocation was sequential, jobs 2. Source oracle environment: SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Actual measured SHA | Command | Exit |
| --- | --- | --- |
| 0f26ddbed5f5affc38affafeaae4710715eb0148 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| 0f26ddbed5f5affc38affafeaae4710715eb0148 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test order_reads | 0 |
| 0f26ddbed5f5affc38affafeaae4710715eb0148 | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |
| b489f74ffde8c774af3d6565003e4d5ecee7d049 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| b489f74ffde8c774af3d6565003e4d5ecee7d049 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test order_reads | 0 |
| b8821a80e92b42a1187ab7697001edd3de0b4edc | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| b8821a80e92b42a1187ab7697001edd3de0b4edc | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test order_reads | 0 |

The focused test is order_reads_native_before_source: one test containing 53 literal expected fixtures. Extractor output was inspected: 7341 symbols / 512 source files; Would update 0 manifests / 0 added / 0 orphaned. git diff --exit-code 0f26ddbed5 HEAD over OKX src, OKX manifest directory, and parity/exempt.toml at b489f74ffde8c774af3d6565003e4d5ecee7d049 exited 0. These older exits remain attached to their measured SHAs. Fresh merge-head metadata measurements are recorded below when completed.

## Native proof and finite floors

Native values, ordered request keys, endpoints, explicit initial and continuation cursors, original row/mapper return/market/error identities, account value, aggregation order, distinct sorting fields, independent result/request/warning/cleanup counts, and full original rejection name/message/code/ids [8,3] are asserted before packing or child source comparison. Normal and algo IDs differ: continuation ordId 9901 versus algoId a9901. Initial per-instrument cursors are native undefined; historical-order initial cursor is omitted and undefined; raw/closed initial timestamp cursor is native string "20000". Literal capture prefix flatten, p-iteration.map, getInstrumentTypes is asserted before awaits. Wire events include invokes, map dispatch, position mapping, and iterator cleanup; capture/get/operator traces are retained for native assertions, not claimed as directly observable JS wire events.

The independent floor is 53 cases: aggregation 2; normal/algo 4 each; historical orders 12; raw positions 10; closed PnL 12; reported historical positions 4; income range 5. Enumeration stops at aggregate/reject, normal/reject, algo/reject, history/close-error, raw/cap, closed/close-error, positions/close-error, and range/empty. It is a finite witnessed floor, not exhaustive provider/runtime coverage.

Dense continuation fixtures use 100 plus 1 rows per instrument/page (normal/algo 202 rows across two instruments; historical/raw/closed 101 rows across two pages). Sparse cap fixtures use one physical row and logical array length 100, including a source custom iterator; they reach 50 historical-order or 100 raw/closed pages. Historical/closed cap warning count is one; raw is zero. Last cap-page sleeps are included. This proves those sparse pagination boundaries, not a dense 5000/10000-row corpus or production load. Historical orders deduplicate repeated IDs; raw and closed PnL retain repeated position IDs. Different createdTime, closedTime, and timestamp values discriminate sort-field selection. Range uses two literal windows, last-write Map deduplication, and exclusive next start 604800001; output and original last record identity are asserted natively.

The maintained-source oracle extracts the actual eight TypeScript AST bodies and actual endpoint table. Recorded fixture bindings provide imported flatten/map/omitUndefined/oldestCursorId/sleep and mapper results. Actual JS Promise/iteration/array length/Set/Map/sort/operators run in those source bodies. The selected normal/algo/raw source methods are not overwritten by their delegation bindings. Reported-position context keys are checked, but context callbacks are not invoked: their implementation is separate provider proof.

Read-only sub-agent review caught a real initial IteratorClose omission. All four for-of loops now use native row-body callbacks inside an explicit whole host acquisition/step/close capability, including original body-error precedence over a throwing Iterator.return. Paired iterator-error and close-error source witnesses exercise actual Iterator.return. Review then requested native capture-order and first-cursor assertions, added before packing. The correction commit bb95c4ea0e3668a1de3b4353d9ec4bec2d1bb716 had a scoped check compiler exit 101 for a missing field-key dereference; its focused test was not run. Commit b489f74ffd corrects it; both refreshed gates above passed. Review also found that range Map insertion already matched descending timestamps, making sorting unproved. Commit b8821a80e92b42a1187ab7697001edd3de0b4edc changes the native and source fixtures so timestamps [2,3] require reordering into [other,same], and refreshes both gates. Reviewer measured source/manifest/exemption preservation b489 to b882 at exit 0. That was a static blindness finding, not a measured surviving mutation. Precommit draft compilation, oracle-binding overwrite, numeric serialization, and cap-expectation failures were development failures, not mutation evidence.

## Independent status census

Python 3.11 tomllib census at 0f26ddbed5f5affc38affafeaae4710715eb0148 versus actual parent 5911167261853a8cbcc99d79cf45b785bb016648 exited 0: OKX 88 ported / 19 divergent / 56 pending becomes 96 / 19 / 48, exactly eight pending-to-ported rows. Twelve OKX manifests were enumerated; stop file okxeudemo.exchange.ts.toml. This is a status census, not complete venue proof. Fresh merge-head comparison follows below.

## Unmeasured scope

No workspace compilation, full OKX/KuCoin venue suite or script, KuCoin cargo check/test, host clippy, wasm clippy, full Rust-index audit, CI rerun, or live network test was run. Concrete exchange/XHR/module assembly, imported mapper bodies, position context callbacks, language intrinsic/provider implementations, external p-iteration concurrency, custom promise scheduling/cancellation, arbitrary getters/coercions/iterators, and already-ported rows outside this slice were not proved. The mapped claim is native function-body parity under explicit expression contracts, with recursive paging translated to a native loop; provider obligations remain explicit in each row.

## Compiling family controls

Seven production mutations cover six new families, with an additional range-sort proof for the review finding. First five were measured at b489f74ffde8c774af3d6565003e4d5ecee7d049; the two range controls were measured at b8821a80e92b42a1187ab7697001edd3de0b4edc. Every mutation compiled, failed a named native assertion at exit 101, restored the source bytes, and then passed the same order_reads target at exit 0. Control enumeration stopped at income-range-sort. Zero measured survivors; zero mutation compiler failures. This is not eight independent symbol controls.

| Actual SHA | Family / native call site | Mutation | Assertion that actually failed | Broken / restored |
| --- | --- | --- | --- | --- |
| b489f74ffde8c774af3d6565003e4d5ecee7d049 | aggregation / fetch_orders | swap trailing and conditional spread groups | ORDER native aggregation order | 101 / 0 |
| b489f74ffde8c774af3d6565003e4d5ecee7d049 | instrument paging / fetch_algo_instrument | last algoId becomes ordId | ORDER native last accumulated id cursor | 101 / 0 |
| b489f74ffde8c774af3d6565003e4d5ecee7d049 | historical paging / fetch_historical_orders | oldestCursorId selector ordId becomes algoId | ORDER native ordId cursor selector | 101 / 0 |
| b489f74ffde8c774af3d6565003e4d5ecee7d049 | position paging / fetch_closed_position_rows | before parameter becomes begin | ORDER native params key order | 101 / 0 |
| b489f74ffde8c774af3d6565003e4d5ecee7d049 | reported positions / fetch_historical_positions | invert inverse-market exclusion | POSITION native captured row-market identity | 101 / 0 |
| b8821a80e92b42a1187ab7697001edd3de0b4edc | income range / fetch_income_history_range | next start loses plus one | RANGE native exclusive next start | 101 / 0 |
| b8821a80e92b42a1187ab7697001edd3de0b4edc | income range / fetch_income_history_range | omit sort-timestamp-desc, return Map insertion order | RANGE native timestamp ordering | 101 / 0 |

The first control runner stopped with its own exit 1 after control five because it expected ORDER native independent result count, while the earlier POSITION native captured row-market identity assertion failed. Its captured Cargo output shows successful compilation, that named native failure at exit 101, and restored target exit 0. This is a caught mutation, not a survivor; the table reports the actual assertion. The remaining two controls ran separately at the corrected fixture SHA. Source was restored byte-for-byte throughout; the two test-only fixture changes between those SHAs are documented above.

## Merge preservation and fresh metadata gate

Fetched and merged integration c7c2023a03f820869af3137bd352d2c78702a5d5. The only merge conflict was parity/exempt.toml append additions. A local /usr/bin/python3 lacks tomllib; the initial resolver failed, and a shell without failure stopping committed unresolved markers in merge commit a435d2c0a4279b27973e6863e388c0f23f3d8aa7. No gates were run at that commit. Corrective commit e317f8c10c8c4766dd8c236f145bb305b25ee758 uses /opt/homebrew/bin/python3.11 to preserve every incoming exemption record plus exactly the two original local C3 records. History was not rewritten.

Measured at e317f8c10c8c4766dd8c236f145bb305b25ee758:

- git diff --exit-code b8821a80e92b42a1187ab7697001edd3de0b4edc HEAD over OKX src, tests, scripts, Cargo.toml, and the whole OKX manifest directory: exit 0.
- Separate scoped diff over sdk-rs/Cargo.toml and Cargo.lock; safe-cex-types, num, store, exchange, transport, crypto, candle-cache; TypeScript OKX sources and types.ts; logging LogSeverity.ts; package.json and yarn.lock: exit 0. The Cargo/source-oracle inputs are byte-unchanged, so their measured b882 exits stand; Cargo was not rerun at e317 and those exits are not relabeled.
- Independent parsed exemption union equality: exit 0. Every incoming record plus exactly two unchanged local C3 records; no fabricated symbol/status.
- Fresh node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check: exit 0. Output inspected: 7341 symbols / 512 files, Would update 0 / 0 added / 0 orphaned.
- Fresh Python 3.11 tomllib census versus merged integration c7c2023a03f820869af3137bd352d2c78702a5d5: exit 0, exactly eight pending-to-ported rows, zero divergent transitions. Integration OKX 88 / 19 / 56 becomes branch 96 / 19 / 48 (ported / divergent / pending). Floor twelve manifests, stopped at okxeudemo.exchange.ts.toml. KuCoin rows are unchanged.
- git merge-base --all origin/feat/safe-cex-rust HEAD returned exactly c7c2023a03f820869af3137bd352d2c78702a5d5. Branch diff against that fetched integration contains seven slice files; no old integration merge-down is carried as unreviewed work.

This receipt update changes documentation only. Source, tests, manifests, exemptions, and measured build/oracle inputs are retained from the explicit SHAs above.

Final read-only sub-agent review at e317f8c10c8c4766dd8c236f145bb305b25ee758 found no material delivery blocker. It independently measured own/shared input preservation, working diff excluding this receipt, and parsed exact exemption union at exit 0; checked the seven actual control outputs, fresh metadata results, and seven-file integration diff. It ran no Cargo, tests, or Rust-index gate.
