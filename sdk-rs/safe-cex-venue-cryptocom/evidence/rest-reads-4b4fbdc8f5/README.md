# Crypto.com REST read bodies

Code/gate SHA `4b4fbdc8f5a17ae2fb75a97900beb8c7070a3144`. Actual fresh integration parent `a1338bc368a19c179baf10f08fab602064136fc9`. Parent is the requested integrated head; no unreviewed lane commits are ancestors. Evidence-only commits do not imply gates were executed on a later SHA.

**8 pending→ported/adapted, 0 divergent**, from tomllib comparison over the venue: 84/18/35 →92/18/27 (ported/divergent/pending). Census floor137rows across6manifests stops `['sdk-rs/parity/manifest/exchanges/cryptocom/cryptocom.ws-public.ts.toml', 'CryptocomPublicWebSocket.dispose']`; this is a venue census, not exhaustive proof.

Rows: fetchMarkets, fetchTickers, fetchOHLCV, fetchOrderBook, fetchOrders, fetchPositions, fetchBalance and private #mapOrder. Entire source arrows are ported at explicit native dependency interfaces in `src/rest_reads.rs`. No shared crate/type/trait edited. Exchange class construction/wiring, start/tick/order-write bodies and remaining WebSocket bodies are still pending.

Full read bodies preserve request paths/params, eager post invocation before await, repeated getter reads, original native field/error values, numeric mappings/fallbacks, optional/nullish branches and logging. Order reads resolve Promise.all before regular/advanced posts; result destructuring interleaves iterator entries with .data reads and closing. Position reads await position mode, perform all three map/filter stages and strict Number-zero predicate, resolve Set before the symbol map, iterate markets incrementally, prepare native fake-array push before constructing both long/short arguments, spread both arrays and return original uniqBy dependency result. Ticker template coercion precedes clock; market power callee lookup precedes decimal getter. Balance assignment reads key then computes Math.max, THEN coerces the computed property key, matching actual source runtime evidence. Source current+legacy order fields and repeated cumulative getter are retained.

Native host bindings for property/getter/proxy methods, callbacks, array/iterator/Set/Promise/transport, numeric/coercion operators, standard parseFloat/Math Number results and imported interval/position-ID/uniqBy/mapper/clock functions are explicit dependencies; their implementations, arbitrary replaced builtins, host ABI and runtime/class scheduling are not claimed ported. Generic native Value/Error handles preserve original referents; stock fixture bindings are bounded adapters, not every runtime object class. f64 checks prove finite exact bits (including -0 and infinities) and NaN classification, not exact NaN payload bits. Strings use native Value carriers capable of UTF16; this fixture floor does not claim every Unicode/coercion input.

The oracle extracts and executes the unchanged full actual eight arrows and original switch helpers. Direct decoded native fields, numeric/string values, errors by original pointer, request/log values, nonboolean filter return values and original tradable/orderID/symbol pointers are asserted BEFORE event/inventory comparisons. No Rust encoder, reencoder, wire or roundtrip equality. Currency replacement retains the decoded old object, which can still mutate without a borrow guard across the write.

Independent fixture floor **68**, stops index67 `balance-replace-retain`; fixture expectedPosts are independently specified (including0 for Promise.all getter failure), not derived from captured artefact lengths. Cases include actual/empty data, malformed/special numbers, current/legacy order fields, repeated bound/filled getters, first/second request errors, original logging errors, nonboolean filter returns, ticker/key coercion throws, swapped response iterator vs indexed values, close failure, and body getter failure surviving distinct close failure. Counts are bounded floors; no exhaustive input/schedule/host/return-alias proof.

Read-only sub-agent review at the code SHA found no material blocker at these declared boundaries; eight moved rows own66actualqualifiedtargets, missing/nonexistent0, stopped #mapOrder. See review.md. Reviewer ran no Cargo/controls/CI. Earlier development failures were corrected before this SHA: local host orphan rules and unused error tag compilation (101), string severity binding, independent pre-post count, and balance ordering. The review recommendation to move key coercion ahead of max was disproved by actual source native trace; max→key order restored. Development failures are not mutation proof; retained root logs were not discarded.

## Measured sequential gates

Exact command/env/SHA/process exit and raw logs in baseline.json, controls.json and gates.json. TypeScript path set; jobs2; one Cargo at a time. Yarn output checked, not exit alone. Writer/extractor must state0 manifests updated,0added,0orphaned.

| Gate at code SHA | Exit | Raw log |
|---|---:|---|
| New68native baseline | 0 | `baseline.log` |
| venue-tests | 0 | `venue-tests.log` |
| check | 0 | `check.log` |
| host-clippy | 101 | `host-clippy.log` |
| wasm-clippy | 101 | `wasm-clippy.log` |
| fmt | 0 | `fmt.log` |
| writer | 0 | `writer.log` |
| extractor | 0 | `extractor.log` |

host-clippy is NOT a lint pass. Observed diagnostics: error: usage of an `Arc` that is not `Send` and `Sync` /    --> safe-cex-transport/src/tealstreet.rs:157:29 / error: usage of an `Arc` that is not `Send` and `Sync` /    --> safe-cex-transport/src/tealstreet.rs:177:14 / error: could not compile `safe-cex-transport` (lib) due to 2 previous errors

wasm-clippy is NOT a lint pass. Observed diagnostics: error: usage of an `Arc` that is not `Send` and `Sync` /    --> safe-cex-transport/src/tealstreet.rs:157:29 / error: usage of an `Arc` that is not `Send` and `Sync` /    --> safe-cex-transport/src/tealstreet.rs:177:14 / error: could not compile `safe-cex-transport` (lib) due to 2 previous errors

Venue executable floor 18 stops `tests/ws_policy.rs`; no workspace scope.

## Discriminating controls

Each mutation is code SHA plus retained patch. Every selected mutant compiles and reaches its named native assertion at runtime (101), then exact restored bytes pass the68-case floor (0). Floor8 stops index7 map-order. No survivor in this selected floor; no exhaustive mutation claim. The map-order mutation is caught at the orders-two callsite invoking the original private mapOrder body, before its standalone fixture; this is the reported intended assertion, not quietly dropped preemption.

| Row family | Named assertion | Runtime exit | Restoration exit |
|---|---|---:|---:|
| markets | `CRYPTOCOM-READ-NATIVE-NUMBER markets-filter-fallback[0].contractSize` | 101 | 0 |
| tickers | `CRYPTOCOM-READ-NATIVE-NUMBER tickers-full[0].bid` | 101 | 0 |
| ohlcv | `CRYPTOCOM-READ-NATIVE-NUMBER ohlcv-full[0].open` | 101 | 0 |
| book | `CRYPTOCOM-READ-NATIVE-NUMBER book-full.request.depth` | 101 | 0 |
| orders | `CRYPTOCOM-READ-NATIVE-TEXT orders-two[0].id` | 101 | 0 |
| positions | `CRYPTOCOM-READ-NATIVE-TEXT positions-full[0].id` | 101 | 0 |
| balance | `CRYPTOCOM-READ-NATIVE-NUMBER balance-full.total` | 101 | 0 |
| map-order | `CRYPTOCOM-READ-NATIVE-TEXT orders-two[0].id` | 101 | 0 |

controls-survival.json records source/tests diff exit0 after exact restoration.

## Explicit non-runs

No workspace build/test/clippy, full monorepo suite, wasm runtime/dedicated wasm check, live exchange/API/WebSocket calls, CI reruns, canonical-index/exhaustive source proof, exhaustive host/scheduling/coercion/NaN-payload/return-alias proof, human review, PR creation or integration merge. Host and wasm clippy are separate attempts; a failure does not certify any unmeasured lint target.

## Integration merge and delivery measurements

Integration `f364cee096ff92584c157cad8b96f12b92cf3a09` merged down at `6a13eaed7bdd47fb942df9d1f71cc7195d698f16`. Own source/tests diff against the original gate SHA4b4fbdc8f5 exited0. Recursive inputs changed in shared safe-cex-exchange source, transport relay, package/exemption metadata and unrelated Lighter SDK lock dependencies. Those exact changed paths are in merge-survival.json. To remeasure this slice proportionately, scoped all-targets check, the new68-case REST target, host/wasm clippy, scoped fmt, writer and extractor ran again at6a13eaed7b:

| Post-merge gate at6a13eaed7b | Exit | Raw log |
|---|---:|---|
| check | 0 | `merge-check.log` |
| rest-tests | 0 | `merge-rest-tests.log` |
| host-clippy | 101 | `merge-host-clippy.log` |
| wasm-clippy | 101 | `merge-wasm-clippy.log` |
| fmt | 0 | `merge-fmt.log` |
| writer | 0 | `merge-writer.log` |
| extractor | 0 | `merge-extractor.log` |

The full venue suite remains measured at4b4fbdc8f5; it was NOT rerun after shared dependency changes. New REST-native floor68 and all-targets check passed after those changes. Host and wasm clippy each failed at unchanged transport tealstreet.rs:157,177 arc_with_non_send_sync, before a venue lint verdict. The eight runtime mutation controls were not rerun on the merge; they remain historical4b4fbdc8f5 measurements over byte-identical controlled source/tests. No inherited failing lint was silently treated as a pass.

Immediately before delivery, integration `8a98237aa1ea496db3b0e16bafedcce0bfd06fc4` merged down at `312426ec45afc58ad927942c13f2f6be89e42bdd`. All Cargo/Rust/TypeScript gate inputs remain byte-unchanged from6a13eaed7b; only parity/exempt.toml changed. Source/tests diff exited0; push-survival.json lists the complete input comparison. Writer/extractor were rerun for this metadata change at312426ec45:

| Push metadata gate at312426ec45 | Exit | Raw log |
|---|---:|---|
| writer | 0 | `push-writer.log` |
| extractor | 0 | `push-extractor.log` |

Both writer/extractor output0 updated manifests,0added,0orphaned. delivery-census.json compares the delivery merge against final integration: **8 pending→ported,0divergent;92ported/18divergent/27pending**, floor137rows/6manifests stopping publicWebSocket.dispose. Both merge-downs moved0Crypto.com rows; the eight moved rows are the port. Remaining Cargo measurements retain their actual6a13 or4b SHA; no new executions are claimed at312426. Delivery evidence-only commit adds no source/test/manifest change. No Bitrue files were touched by this slice.
