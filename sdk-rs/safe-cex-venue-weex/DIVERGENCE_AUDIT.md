# WEEX divergent-row audit

Frozen audit input: 7fd8028ee5ab29add1380bf55c3a373642c2430f, the clean merge of
origin aab488a8e2ac3a06ad1f53777831d21659aac289 into accepted
85f18f93bd452b2909b7a4c54f2bf22668f2e520. The accepted findings branch remains
an ancestor; this deliverable includes it. No unrelated unreviewed feature
ancestor was added. Delivery branch: fix/weex-divergent-closure. No push.

The independent TOML population is 121 ported / 35 divergent / 0 pending,
156 rows in six manifests. The audit enumerates sorted manifest filenames and
then source row order, stopping at divergent iteration 35, public handleDepth.
All 35 notes and their maintained TypeScript/native bodies were inspected.
Completion is 35 audited / 0 unaudited in this frozen block, not exhaustive
conformance for all inputs, host implementations or scheduler interleavings.
The 121 ported rows are outside this audit's population.

Canonical declarations remain at MANIFEST-SPEC.md anchors
[14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and
[19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).
WEEX policies additionally cite #weex-fill-projection-choice,
#weex-record-shape-choice, #weex-class-assembly-choice and
#weex-async-observation-choice there. Their declarations are not copied here.

## Disjoint results

| Primary audit result | Rows |
| --- | ---: |
| Supported existing rationale | 11 |
| Explanation resolved | 20 |
| Documentation resolved | 1 |
| Scheduling repaired | 3 |
| Behaviour repaired | 0 |
| Total | 35 |

These primary outcomes are disjoint. The three scheduling rows share one native
mechanism; they are not three independent implementations. The accepted prior
normalizeMarket coercion repair is supported ancestry here, not another new
repair. Twenty-four notes change; statuses, mappings, layers and file declarations
remain unchanged. No source defect was relabelled to obtain completion.

| Residual manifest bucket | Rows |
| --- | ---: |
| FORCED | 1 |
| CHOSEN | 22 |
| MIXED | 12 |
| CORRECTED | 0 |
| FINDING | 0 |
| Total | 35 |

## Every frozen row

| Iteration | Source symbol | Primary result | Residual | Resolution / witness |
| --- | --- | --- | --- | --- |
| 1 | WeexExchange | explanation | CHOSEN | Explicit assembly choice; classes target checks 45 own arrows and receiver binding, without concrete body-linking proof. |
| 2 | WeexExchange.fetchOrders | explanation | CHOSEN | Account-read rejection/dedupe witness; distinguish surviving peer jobs from owned aggregation continuations and required settled semantics. |
| 3 | WeexExchange.fetchIncomeHistoryRange | explanation | MIXED | Actual wrapper returns D800 tranId; record_boundaries distinguishes native IDs and whole-page admission from whole-range loss. |
| 4 | WeexKlineRow | explanation | FORCED | Actual checker accepts frozen tuple, jsonParse returns D800 at index 0; native tuple fields precede source observation. No copied object-extra-field or unproved depth cause remains. |
| 5 | WeexExchangeInfoResponse | supported | MIXED | Existing assets/symbols names, returned-unit and depth witnesses; whole response refused. |
| 6 | WeexOrderResult | supported | MIXED | Existing actual parseOrderResult returned ID, native order/client distinction and whole-record refusal. |
| 7 | toNumber | explanation | CHOSEN | input-prices: unit input returns default 0, depth-150 .25 returns .25; native argument admission supplies neither result. |
| 8 | toIntegerString | supported | MIXED | Existing returned D800 update-ID and separate input/capture costs; native positive strings precede source profile. |
| 9 | normalizeTimestampToMs | explanation | CHOSEN | input-prices: unit returns 0, depth-150 .25 returns 250; native admission refuses; host timezone remains explicit. |
| 10 | normalizeTimestampToSeconds | explanation | CHOSEN | input-prices: both selected inputs return 0; native refusal loses that normal result. |
| 11 | normalizeMarket | supported | MIXED | Prior accepted scalar-before-string repair, info-unit and both snapshot directions retained. |
| 12 | normalizePercentage | explanation | CHOSEN | input-prices: unit returns 20 for 12/10, depth-150 .25 returns 25; native admission refuses. |
| 13 | normalizeTicker | supported | MIXED | Existing raw/book snapshots, returned info unit and unread full-Market admission prices retained. |
| 14 | normalizePositionMode | explanation | CHOSEN | input-prices returns OneWay for both selected refused inputs; native variant read directly. |
| 15 | normalizeMarginType | explanation | CHOSEN | input-prices returns Cross; retained executable-object/depth scope cites actual policies. |
| 16 | normalizePositionSide | explanation | CHOSEN | input-prices returns Long; no returned-unit claim for an enum default. |
| 17 | normalizeOrderSide | explanation | CHOSEN | input-prices returns undefined; native Option variant asserted before source observation. |
| 18 | normalizeOrderStatus | explanation | CHOSEN | input-prices returns Open; argument refusal loses the normal default. |
| 19 | normalizeOrderType | explanation | CHOSEN | input-prices returns Limit; argument refusal loses the normal default. |
| 20 | normalizeTriggerType | explanation | CHOSEN | input-prices returns LastPrice; argument refusal loses the normal default. |
| 21 | normalizeBalance | supported | MIXED | Existing retained array info, returned unit and both native snapshot directions retained. |
| 22 | normalizeCandle | explanation | CHOSEN | Unread unit/deep property leaves complete source Candle; native admission loses it. Numeric fields checked directly. |
| 23 | normalizeExecution | supported | MIXED | Accepted fills_error_order covers closed fields, cold ignored input and returned units; no new fill repair claimed. |
| 24 | normalizeHistoricalTrade | supported | MIXED | Accepted second timestamp/order guard and model prices retained; no status promotion. |
| 25 | normalizeIncome | supported | MIXED | Existing actual transaction-ID unit and native fields; this utility is not the range pager. |
| 26 | normalizePublicTrade | supported | MIXED | Existing returned ID unit and native strings/enums/bits; complete input refusal priced. |
| 27 | normalizeOrderBookSide | explanation | CHOSEN | Ignored third-element unit/depth leaves one source level; native admission loses the vector. |
| 28 | normalizeOrderBook | explanation | CHOSEN | Ignored unit/depth leaves both source sides with cumulative amounts 2/4; native admission loses the book. |
| 29 | fakePositionsForMarket | supported | MIXED | Existing two-position result, symbol unit and unread typed-Market input costs; fresh info is not a copied Market alias. |
| 30 | WeexPrivateWebsocket | explanation | CHOSEN | Explicit assembly choice; six data fields and 14 arrows tested, without production transport/Store adapter wiring claim. |
| 31 | WeexPrivateWebsocket#throttledFetchOrders | scheduling | CHOSEN | Ready result suspends before disposal/update/report; four native cases plus actual callback source profile. |
| 32 | WeexPrivateWebsocket#throttledFetchPositions | scheduling | CHOSEN | Same repaired reaction boundary, independent positions result/update assertions and intervening disposal. |
| 33 | WeexPrivateWebsocket#throttledFetchBalance | scheduling | CHOSEN | Same repaired reaction boundary, independent balance result/update assertions and ready-rejection report. |
| 34 | WeexPublicWebsocket | explanation | CHOSEN | Explicit assembly choice; 20 own arrows and prototype dispose checked; no fixed production adapter table claimed. |
| 35 | WeexPublicWebsocket#handleDepth | documentation | CHOSEN | Existing snapshot rejection/route-throw reporting policy retained; remove historical class-scope resolution wording and distinguish required observer lifetime from fixture evidence. |

## Scheduling repair and discriminating evidence

At df77e64a99270163f5245d7a539b5c66d412790b, the three ready-refresh tests compiled
and each failed 101 at its named `WEEX ready refresh reaction waits` assertion:
balance:success, orders:success and positions:success. Actual-source refresh-turn
already passed 0, failed 1 with the named await-removal control, and restored 0.
At 28f7057e42976a8c89da849faa74970804ca82b2 the three native tests passed 0.
The fix defers both a fetched success and rejection before update/report, retaining
eager fetch invocation and allowing the intervening disposal check to suppress
updates. It does not create a JavaScript scheduler or retain dropped observers.
The new source/native inventory is twelve cases, ending balance:apply-error.

The first kline supplement compile at 28f7057e42 failed because the witness
mistakenly treated slot 7 as numeric. The fixture/assertion were corrected to the
maintained string slot in 7d12ea9bb3. That compiler error is not mutation proof.
Early source-profile drafts also rejected incorrect witness expectations (a
fallback argument, an empty equal-endpoint range and notional rather than amount
totals); maintained source and old feeders were unchanged. The fixed range is
1..2 with one record followed by two empty boundary pages, not a zero-width walk.
At b2a6d939eb the income supplement also failed compilation because IncomeRecord
does not implement JsNumberOutput. de97bcb49b uses its existing Serde model
boundary; it introduces no numeric-capture registration. The refused typed page
is observed through that single Serde route, before invoking the range wrapper.

`divergence-observations.cjs` executes actual selected declarations and shared
imports. Its four profiles stop at 42 cases: twelve refresh cases, one frozen
kline observation, 28 refused-input prices and one actual range result. Source
counterfactuals change only the selected witness/control: remove await, replace
observed units, or remove a producer case. Maintained reference files remain
unchanged; these are in-memory counterfactuals. Native strings/variants/number bits and admission outcomes are checked
before source comparisons. The input-price profile demonstrates normal source
results lost before native invocation, not mapper-induced numeric corruption.
Existing response, utility, fill and runtime corpora remain unchanged.

## Follow-through limits

Pure utility edges terminate in captured Value access, shared coercion/number/date
primitives, fixed enum tables and fresh typed models. The two book helpers call
the shared pure total calculator; that dependency does not notify Store. The
class walk terminates at required imported superclass/member/receiver capabilities.
`fetchOrders` follows local normal/algo read bodies to request/normalizer and
settlement capabilities, then spread/uniq; arbitrary supplied callbacks are not
proved by a recording host. The range wrapper follows the existing shared pager,
its typed ledger, page callback and 100ms sleep, then the warning/date capabilities;
it adds no pager. Refresh follows fetch to disposal and the appropriate Store
update capability, including thrown update reporting. Public depth follows live
book updates/totals and the routed callback; the gap edge is retained by the
required observer operation. Incoming callers do not expand this population.

The sub-agent reviews inspect all 35 notes, the 26 type/utility bodies and nine
runtime rows. Runtime review identified the ready-refresh defect and separated
observer ownership from that repair. SHA-bound delivery gate exits, restored
mutation controls and review receipts belong under ignored
target/weex-divergence-audit/<sha>/. This document does not claim pending runs
passed; earlier measurements above are not transferred to another SHA. Required delivery gates are fmt,
workspace build and host workspace Clippy. Wasm and central wiring/layers/routing
are not run in this lane. Full suites and parity/extractor/static checks are not
part of the current three-gate bar.
