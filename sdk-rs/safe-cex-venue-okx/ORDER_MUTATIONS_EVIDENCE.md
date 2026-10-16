# OKX order mutations evidence

Nine pending rows become ported; zero become divergent. Fresh branch feat/okx-order-mutations is based on actual integration parent 157f83240a85280c25df395ba6a51225813fe9bf, with no unreviewed feature branch as an ancestor. Native source/test/manifest commit and initial gate SHA is be1b0b7b8a797fcc71cbb811ad8ec417fc07741a. Kucoin is unchanged. Existing MarketAccountOps, shared types and shared traits are unchanged; no workspace sweep is claimed or needed for a shared field/trait change in this slice.

All 25 real functions in safe-cex-venue-okx/src/order_mutations.rs have manifest targets, including transitive native helpers and actual callback bodies. parseManifest/serializeManifest generated the ordered arrays and every full target's provenance. Only OrderMutationOps is a new C3 supporting declaration, grouped beside existing OKX exemptions. Its callback/value/receiver/future capabilities are representation, not an extra moved row or algorithm exemption. Existing venue-oracle:okx already uses all-targets and discovers the new test; that full script was not run. The writer ran after manifest edits before the source commit: exit 0, Updated 0 manifests / 0 added / 0 orphaned. This precommit exit is not relabeled as a committed-SHA measurement.

| Maintained TypeScript function | Native root / owned callback helpers |
| --- | --- |
| OKXExchange.updateOrder | update_order |
| OKXExchange.updateAlgoOrder | update_algo_order |
| OKXExchange#updateTriggerAlgoOrder | update_trigger_algo_order |
| OKXExchange.cancelOrders | cancel_orders |
| OKXExchange.cancelSymbolOrders | cancel_symbol_orders |
| OKXExchange#cancelNormalOrders | cancel_normal_orders / cancel_kind / cancel_batch |
| OKXExchange#cancelAlgoOrders | cancel_algo_orders / cancel_kind / cancel_batch |
| OKXExchange.setLeverage | set_leverage / recover_positions |
| OKXExchange.changePositionMode | change_position_mode / recover_positions |

## Measured gates

Cargo executable /opt/homebrew/opt/rustup/bin/cargo; every invocation sequential, jobs 2. Focused source-oracle environment SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

| Actual measured SHA | Command | Exit |
| --- | --- | --- |
| be1b0b7b8a797fcc71cbb811ad8ec417fc07741a | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| be1b0b7b8a797fcc71cbb811ad8ec417fc07741a | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test order_mutations | 0 |
| be1b0b7b8a797fcc71cbb811ad8ec417fc07741a | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |

Focused test order_mutations_native_before_source passed one test containing the independent 50-fixture floor. Extractor output was inspected: 7341 symbols / 512 source files, Would update 0 manifests / 0 added / 0 orphaned. Draft compilation failed for an unused test import; draft source runs exposed harness lexical-this binding, unexported helper, missing ENDPOINTS context and mode-setter trace issues. They were corrected before this SHA; those development failures are not mutation controls. Native assertions passed before the source-loader/trace corrections, and the complete native/source draft passed before the committed gates.

## Native assertions and finite floors

Every fixture checks native results, ordered payload keys, string-valued price/size/IDs, numeric replacement fields, original order/update/nested-extra/group/refreshed-position references, separate normal/algo routes, request/batch/refresh/store/diagnostic/constructor counts, enum/mode assignment, fresh error name/message/absent code and ids, and original transport error identity/name/message/code WRITE/ids [8,6] as applicable. Diagnostic counts and literal Native::S messages are asserted for every profile, including zero messages. All native fixtures finish before child source comparison; output packing and wire assertions cannot substitute for these values.

Independent floor 50: limit amend 7; algo recreation 5; trigger amend 9; cancellation dispatch 4; symbol cancellation 3; normal cancellation 4; algo cancellation 4; leverage 7; mode 7. Enumeration stops at limit/delegate, algo/place-reject, trigger/empty-body, dispatch/algo-only, symbol/reject, normal/reject, algo-cancel/reject, leverage/recovery-reject and mode/guard-error. Global stop is mode/guard-error. This is a finite floor, not exhaustive venue/runtime coverage.

Limit payload uses instId INST, full ordId ROOT_tp, unadjusted newPx "17.3" and adjusted newSz "2.25". Missing price and original amount string "0" discriminate omitted payload keys; tiny amount emits and returns an empty result. Missing market throws outside the network catch. Algo recreation keeps the nested extra reference and arbitrary tag 29, uses raw replacement amount 2.2 and price 17.3, retains broad prefix siblings ROOT_sl and ROOTX_tp, cancels before placing, and preserves original rejection identity when either helper rejects.

Trigger payload strips the suffix to algoId ROOT, adjusts trigger to "17.5", distinguishes conditional limit "12.5" from market sentinel "-1", retains index trigger type and "2.25" size. Invalid trigger and tiny amount emit then construct fresh errors. Top-level and row response failures emit CODE/ROW and throw fresh errors; absent response body succeeds. Custom response.data.find observes the native callback's exact string ROW-RAW for sCode "0" and sMsg ROW-RAW; the source custom method asserts the same string. This distinguishes raw predicate results from boolean coercion, even though ordinary Array.find would hide the difference.

Cancellation dispatch preserves original group order and awaits normal before algo; normal rejection prevents algo dispatch. Empty symbol matches still delegate an empty group. Private cancellation filters the wrong kind, skips unknown markets, preserves full normal IDs and strips algo suffixes, and reads accumulator spread before market.id before order.id. Literal normal 41-row and algo 21-row fixtures create independent widths 20/20/1 and 10/10/1. A reject stops after the first request. Per-row WARN messages are natively asserted once per successful fixture batch.

Leverage clamps input 50 to upper bound 9 and skips unchanged leverage. Position find still occurs with a missing market. Callee-reentry fixture changes Math.min during Math.max and changes the upper limit to 7; the captured original min and fresh upper-bound read produce "7". Successful refresh uses store.update with original refreshed positions; recovery uses addOrUpdatePositions and fresh error construction. Late-error fixture emits DOWN, then refreshes/merges and rereads the original error as AFTER. Recovery rejection escapes with the original transport error and no fresh construction. Mode guard emits before any request when contracts > 0 and is outside the try; a guard getter rejection preserves the original error. Long-short/net request codes are distinct. Mode assignment to hedged survives a later refresh rejection and recovery.

The oracle extracts the actual nine authored arrow initializers, actual ENDPOINTS/enums/adjust and private getOKXTriggerPxType declarations, with the actual safe-math initialization/dependencies. Factory.call(receiver) binds lexical this without rewriting the maintained initializer. Only non-selected delegation methods receive fixtures. Real installed lodash chunk/partition and p-iteration forEachSeries run in the source binding. Native hosts remain finite ready-value and expression/receiver bindings; imported provider implementations are not thereby proved.

Read-only sub-agent production comparison found no concrete mismatch under these expression contracts. It identified two static proof gaps: optional find coerced every observed callback value to truth, and diagnostics were only asserted for a subset of profiles. The custom raw predicate witness and per-profile native diagnostic/constructor assertions close those gaps before the measured SHA. Neither gap was a measured surviving mutation. Static metadata review exited 0: exact nine pending transitions, all 25 real native functions in targets/notes/provenance, and one new support exemption; existing MarketAccountOps source preservation versus the parent exited 0. The reviewer ran no Cargo, test, oracle or index gate.

## Local operation contracts

OrderMutationOps wraps the unchanged value/receiver carrier and retains raw callback values for filter, find, optional_find and partition. Those are complete authored method/import operations, including lookup, call receiver, iteration, holes and callback return semantics. series invokes the captured imported forEachSeries with a real native batch future and preserves sequential adoption/rejection. adopt_return models implicit async return adoption separately from explicit await_value; neither is proved for arbitrary scheduling by the ready fixture.

Receiver captures bind current callees before authored argument expressions; xhr.post:ENDPOINTS.* includes endpoint argument evaluation. has models the JS in operator, including inherited properties/proxies. Error capture precedes the message expression and new_error implements actual construction/coercion. Template, suffix regex, arithmetic, computed indexing, object spread and destructuring handles represent whole source language operations. define-literal-field uses literal DefineDataProperty semantics rather than ordinary assignment; array-spread-prefix completes accumulator iteration before object getters and array-prefix-append completes the literal without another spread. Standard payload assignments and mode assignment retain normal property setter semantics. These are explicit production host requirements; the fixture implementations prove only the finite supplied profiles.

## Independent status census

Python 3.11 tomllib comparison at be1b0b7b8a797fcc71cbb811ad8ec417fc07741a against actual parent 157f83240a85280c25df395ba6a51225813fe9bf exited 0: OKX 104 ported / 19 divergent / 40 pending becomes 113 / 19 / 31, exactly nine pending-to-ported changes and zero divergent transitions. Twelve OKX manifests were enumerated; stop file okxeudemo.exchange.ts.toml. This is a census, not full venue proof. Fresh merge comparison is recorded separately below.

## Unmeasured scope

No workspace compilation, full OKX/Kucoin venue suite or script, Kucoin cargo check/test, host clippy, wasm clippy, full Rust-index audit, CI rerun or live network gate was run. Concrete class/XHR/module/store/provider assembly, imported helper implementation equivalence, arbitrary getters/prototypes/coercions/forEach holes/iterators/thenables, cancellation/dropped-future behavior, arbitrary implicit-return/await adoption and p-iteration scheduling are not proved. The mapped claim is native function-body parity under explicit complete operation contracts; class/constructor and unrelated pending rows remain pending. No shared core query ABI was adopted.

## Compiling family controls

All seven controls were measured with HEAD be1b0b7b8a797fcc71cbb811ad8ec417fc07741a plus the specified temporary production patch. Every mutation compiled and failed a named native assertion at exit 101, restored the original source bytes, and passed the same order_mutations target at exit 0. The tests and oracle were not weakened or mutated. After the last restoration, source/test byte comparison against that SHA exited 0. Seven families cover nine new rows; this is not nine independent symbol mutations. Enumeration stops at mode-guard-message: seven compiling controls, zero measured survivors, zero mutation compiler failures. No exhaustive mutation claim is made.

| Native call site | Production mutation | Actual failing native assertion | Broken / restored |
| --- | --- | --- | --- |
| update_order | newPx payload key becomes newPxWrong | LIMIT native payload keys | 101 / 0 |
| update_algo_order | cancellation delegates to cancelNormalOrders instead of cancelAlgoOrders | ALGO native cancel before recreation | 101 / 0 |
| update_trigger_algo_order | raw optional-find callback result becomes a boolean | TRIGGER native raw find predicate string | 101 / 0 |
| cancel_orders | algo dispatch runs before normal dispatch | CANCEL native sequential group order | 101 / 0 |
| cancel_kind | normal/algo widths 20/10 become 10/20 | BATCH native independent request count | 101 / 0 |
| recover_positions | freeze first message and reuse after refresh instead of rereading | ERROR native message | 101 / 0 |
| change_position_mode | open-position guard diagnostic becomes Switch position mode | DIAGNOSTIC native message value | 101 / 0 |

The recovery control decoded native message DOWN against independent AFTER. The raw-predicate control catches the reviewed booleanization counterexample; the diagnostic control catches a changed warning even though mode/open returns undefined. These closed static gaps were not previously measured survivors.

## Integration merge and refreshed inputs

Final fetch merged actual integration 836e2a9493ae2d7a0f32cb86d502109d73fa8e7d at merge SHA 2f182a26a966fe1c476ce13c8c2393db50d56eb6. The merge completed cleanly, including the grouped local exemption. Original parent remains 157f83240a85280c25df395ba6a51225813fe9bf; integration is the only added merge ancestry. git merge-base --all returned exactly the fetched integration SHA. The integration diff contains seven slice files, stopping at tests/order_mutations.rs; no old unreviewed feature merge-down is carried.

Measured at 2f182a26a966fe1c476ce13c8c2393db50d56eb6:

- git diff --exit-code be1b0b7b8a797fcc71cbb811ad8ec417fc07741a HEAD over the whole OKX src/tests/scripts/Cargo.toml and the whole OKX manifest directory: exit 0.
- Separate byte comparison over sdk-rs/Cargo.toml/Cargo.lock; safe-cex-types, num, store, exchange, transport and candle-cache; TS OKX sources, types.ts, safe-math.ts; package.json and yarn.lock: exit 0.
- safe-cex-crypto/src comparison: exit 1, incoming Poseidon loader changes. All-build-input preservation is not claimed. The scoped OKX Cargo check and focused test are therefore refreshed at this merge SHA, alongside fresh extractor metadata. The seven control exits remain attributed to their original be1b0b7b8a797fcc71cbb811ad8ec417fc07741a-plus-patch measurements, not relabeled as new dependency measurements.
- Independent parsed exemption equality: exit 0, all 1090 incoming records in original relative order plus exactly one unchanged reviewed OrderMutationOps record, 1091 total. No incoming record was removed or fabricated.
- Fresh Python 3.11 tomllib comparison against merged integration: exit 0, exactly nine pending-to-ported changes, zero divergent transitions. Integration OKX 104/19/40 becomes branch 113/19/31 (ported/divergent/pending). Floor twelve manifests; stop okxeudemo.exchange.ts.toml. These are fresh values, not reused exits from the parent census.

| Actual measured SHA | Refreshed command | Exit |
| --- | --- | --- |
| 2f182a26a966fe1c476ce13c8c2393db50d56eb6 | cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2 | 0 |
| 2f182a26a966fe1c476ce13c8c2393db50d56eb6 | cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test order_mutations | 0 |
| 2f182a26a966fe1c476ce13c8c2393db50d56eb6 | node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 |

Fresh focused target passed order_mutations_native_before_source with its unchanged 50-fixture floor. Extractor output inspected: 7341 symbols / 512 source files, Would update 0 / 0 added / 0 orphaned. Both scoped Cargo commands rebuilt against the incoming crypto source; these are actual fresh merge-SHA exits. The eventual receipt-only commit does not relabel them as measurements at a later SHA.

Final read-only sub-agent delivery review measured owned/shared-except-crypto preservation at exit 0, independently verified the exact exemption union and fresh nine-row census, and inspected all seven raw control failure/restoration pairs. It confirmed each mutation compiled and reached the named native assertion, each restoration passed, and all fresh merge-head gate records agree with the actual output at 2f182a26a966fe1c476ce13c8c2393db50d56eb6. No delivery blocker remained. The reviewer ran only read/git/parser verification, no Cargo/test/index gate. This receipt update changes documentation only; the measured source, tests, manifests, exemptions and build/oracle inputs remain retained from the explicit SHAs above.
