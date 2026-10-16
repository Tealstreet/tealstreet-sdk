# Full OKX, Kraken and WEEX divergent explanation audit

All92 divergent rows are frozen at `d74a1c01f77439f9df61fa330063be194aab7576`. The original36-row sample remains intact; this adds the other56, including three notes without CHOSEN. No row substitution or status promotion occurs. [Frozen full rows](full-selection.json), [machine-readable verdicts](full-classification.json), and the [original sample](README.md) retain every historical explanation.

The original branch started directly at d74. After the user merged PRs7914 and7932, this extension merged integration `333fd419fdbf70f081039610f22da9ee653bc96f` down at `5dddff63fb177febc0199ea08bf70642d9231b46`. All changes unique to this extension are audit evidence and37 finding-note prefixes. Four findings in the frozen audit were already known (three OKX prototype rows and the routed shared-stats cause); PR7914 separately repaired/ruled those causes. They are not37new repairs or four newly open obligations. No unreviewed unrelated feature ancestor is claimed.

| Venue | Supported | Finding | Unresolved | Last frozen iteration |
| --- | ---: | ---: | ---: | --- |
| OKX26 | 13 | 13 | 0 | 26, onMessage |
| Kraken31 | 15 | 16 | 0 | 31, Spot fetchOHLCV |
| WEEX35 | 19 | 12 | 4 | 35, normalizeHistoricalTrade |
| Total92 | 47 (51.1%) | 41 (44.6%) | 4 (4.3%) | All frozen divergent rows |

Finding primary classes are disjoint per row: **28 explanation gaps, six scheduling gaps, two represented-input behavior differences, one stale-scope-only finding, four already-known findings**. Some rows have secondary causes; they are not counted twice. Thirty-seven new finding causes are annotated while preserving prior note text. **Zero statuses or Rust targets move.** Supported explanations are bounded to the named cost; they are not full function equivalence.

Canonical declarations remain at [14](../../MANIFEST-SPEC.md#shared-utf8-string-boundary), [15](../../MANIFEST-SPEC.md#closed-typed-output-boundary), [16](../../MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [17](../../MANIFEST-SPEC.md#numeric-ingress-defect), [18](../../MANIFEST-SPEC.md#divergence-taxonomy) and [19](../../MANIFEST-SPEC.md#forced-surrogate-conflict). This audit records observations rather than duplicating those declarations.

## Additional frozen rows

The original36 identities/verdicts remain in README.md and classification.json. Full-classification.json maps them into this full SHA256 order. Source paths below use the venue source/crate directories; new probes execute maintained declarations, and native bodies are inspected only.

### okx

| Full iteration | Symbol | Classification / primary class | Observation and proof limit |
| ---: | --- | --- | --- |
| 7 | okxBillCategory | finding / existing | Existing inherited category finding at income.rs18; actual source plain-object lookup. Separately repaired in PR7914, not newly discovered. |
| 14 | mapOkxBillToIncomeRecords | finding / existing | Existing inherited bill-category propagation finding at income.rs44/79. Separately repaired in PR7914. |
| 15 | OKXDemoExchange | supported / representation | Demo base carrier/flag versus JS prototype is explicitly priced; family_lifecycle.rs22/106 and okxdemo.exchange.ts6. |
| 16 | OKXBusinessWebsocket | supported / representation | Private base plus fresh topics, externally supplied class capabilities; websocket_account_events.rs20/104 versus okx.ws-business.ts30. Prototype/replaceable methods explicitly priced. |
| 17 | findTickerMarkForCurrency | finding / explanation | Actual exported findTickerMarkForCurrency uses D800 currency/instrument comparison and returns17. exchange.ts96/100; exchange.rs95 requires str/owned optional Strings. CHOSEN-only string clause omits the program comparison and whole-input loss; depth cost remains separate. |
| 18 | OKXPublicWebsocket.handleOpenInterestEvents | finding / explanation | Actual handleOpenInterestEvents forwards D800 Store selector and openInterest19. okx.ws-public.ts258; public_socket.rs195 requires Value/String. Whole-frame loss not named by array-shape choice. |
| 19 | OKXEUExchange.isSupportedMarketInstrument | supported / input | Own-record input versus inherited FUTURES/xperp/linear fields is explicitly priced and has existing source/JSON-view witnesses; market_policy.rs5 and okxeu.exchange.ts32. |
| 20 | OKXPublicWebsocket#dispatchMessageHandler | finding / existing | Already routed global inherited stats mutation finding; public_socket.rs109/125 calls core stats. This is not a newly found callback error. Core cause separately ruled in PR7914. |
| 21 | OKXExchange#placeOrderBatch | supported / closed output | Truthful non-string-ID/diagnostic refusals and deferred later-row/request effects are explicitly priced; rest_operations.rs77/151 versus okx.exchange.ts2189. mapSeries initial payload await prevents an immediate eager-POST claim. |
| 22 | OKXPrivateWebsocket | supported / representation | Private socket field carriers and missing JS callable prototype/replaceable arrows explicitly priced; websocket_account_events.rs8/64 and okx.ws-private.ts19. |
| 23 | OKXPublicWebsocket | supported / representation | Public fresh state plus host-resolved retained callbacks explicitly priced; websocket_account_events.rs13/74 and okx.ws-public.ts25. No fixed SDK host assembly claimed. |
| 24 | OKXEUDemoExchange | supported / representation | EU base/flag composition and separately owned constructor policy explicitly priced; family_lifecycle.rs26/132 and okxeudemo.exchange.ts6. |
| 25 | OKXPublicWebsocket.subscribe | finding / explanation | Actual subscribe sends topic instId containing materialized D800; pre-stringify units asserted before escaped wire. public_socket.rs299 takes Map<String,Value>. Existing live-alias price remains supported but UTF-8 clause omits the program-transmitted unit and whole-operation loss. |
| 26 | OKXPublicWebsocket.onMessage | finding / explanation | Actual onMessage parses escaped D800 channel, invokes actual dispatch, records D800 key and calls own callback. Native public_socket.rs146/168 rejects materialized units through Value parser/str registry. Whole-frame/callback loss is not priced. No full transport/core assembly proof. |

### kraken

| Full iteration | Symbol | Classification / primary class | Observation and proof limit |
| ---: | --- | --- | --- |
| 2 | KrakenExchange.validateAccount | finding / scheduling | FORCED returned-error cost is supported, but source validateAccount GET1598 runs before Promise return; validation.rs142/152 defers GET until poll. Supplemental source prefix witnesses exact invocation. |
| 14 | mapKrakenFills | finding / explanation | Actual mapKrakenFills batch returns ordinary ID then D800 ID in fixed two-output order. kraken.fills.ts73 versus fills.rs72. Whole-batch refusal must be separate from chosen object/depth restrictions. |
| 15 | KrakenExchange | supported / assembly | Futures class fixed source bodies versus externally supplied member bindings explicitly disclosed; kraken.exchange.ts89, classes.rs49/109. |
| 16 | isPositionSizeLeg | finding / explanation | Actual isPositionSizeLeg distinguishes D800/D800 true from D800/DC00 false; ledger classifier consumes both materialized string values (kraken.income.ts24; income.rs9 Value). Required native strings cannot retain them; input-only CHOSEN explanation omits this program comparison. |
| 17 | num | supported / input | Income num presence/Number projection is within explicitly declared wire/executable-input boundary; kraken.income.ts31 and income.rs14. No retained-unit output inferred. |
| 18 | mapKrakenAccountLogRow | finding / explanation | Actual account-log ROW producer returns one tranId D800:pnl. kraken.income.ts79 and income.rs28 require whole-row String decoding; price larger refusal under19. |
| 19 | num | supported / input | Fills num Number conversion and finite/zero result within declared wire domain; kraken.fills.ts31 and fills.rs20. Executable coercion excluded explicitly. |
| 20 | toMs | supported / date host | Explicit Date.parse host/timezone adapter and input restriction; kraken.income.ts42 and income.rs21. No retained string result. |
| 21 | KrakenPublicWebsocket | supported / assembly | Fixed source arrows/prototype mapTimeframe versus external callable member table is disclosed; kraken.ws-public.ts13/352 and classes.rs430/515. |
| 22 | toSeconds | supported / date host | Date.parse to seconds and explicit host/input restriction; kraken.fills.ts26 and fills.rs9. No forced output claim from a numeric result. |
| 23 | KrakenExchange.fetchTickers | finding / scheduling | Source fetchTickers GET270 before return versus lazy tickers.rs42/56. Supplemental source prefix executed; proposed additional returned-info unit inquiry not executed and not counted. |
| 24 | sort | supported / locale/input | Source sort localeCompare kraken.api.ts20 and shared pinned collator helpers.rs24. Locale/input choice declared; wider private-caller surrogate comparison inquiry withdrawn for lack of consumer proof. |
| 25 | KrakenSpotExchange.fetchBalance | finding / scheduling | Source Spot balance POST541 before return versus spot_balance.rs12/26 lazy body. Separate source probe returns D800 at currencies.USDT.info.balance; omitted whole-response/fallback cost is a secondary justification cause. |
| 26 | KrakenSpotExchange.fetchWsToken | finding / explanation | Actual Spot fetchWsToken returns D800. reads.rs103 returns UTF-8 Value; exact token/response loss and stale constructor-pending wording omitted. Native POST is eager at114: no prefix finding. |
| 27 | KrakenExchange.fetchOrders | finding / scheduling | Source fetchOrders GET347 before return versus lazy orders.rs40/53. Supplemental prefix executed; additional order-id unit inquiry not executed and not counted. |
| 28 | createKrakenSpotClientOrderId | supported / generator ABI | Source module-owned fixed32 generator versus reusable native supplied-entropy factory; krakenspot.exchange.ts43 and helpers.rs39. Optional manual-size omission explicitly priced; no host entropy integration claim. |
| 29 | KrakenPrivateWebsocket | supported / assembly | Source fixed private arrows versus required external member bindings explicitly disclosed; kraken.ws-private.ts32 and classes.rs103/634. |
| 30 | KrakenExchange.fetchHistoricalPositions | supported / delegation | Historical positions typed delegation and no pre-poll effects explicitly priced; kraken.exchange.ts89, reads.rs264, MANIFEST-SPEC journal declaration. Unproved account-D800 caller inquiry withdrawn; missing concrete runtime wiring is not refuted by landed constructor body alone. |
| 31 | KrakenSpotExchange.fetchOHLCV | finding / scheduling | Source Spot OHLCV Store/options/GET490-504 before return versus lazy spot_reads.rs35/48/70. Supplemental source prefix executed; extra transmitted-pair unit inquiry not executed or counted. |

### weex

| Full iteration | Symbol | Classification / primary class | Observation and proof limit |
| ---: | --- | --- | --- |
| 13 | normalizeTimestampToSeconds | supported / date host/input | Numeric timestamp-to-seconds and explicit timezone/input boundary; weex.utils.ts66 and utils.rs200. No retained-unit inference. |
| 14 | WeexPrivateWebsocket#throttledFetchOrders | unresolved / lifecycle | Orders refresh reporting choice is visible/priced, but ready-child timing/drop ownership unproved; weex.ws-private.ts204 and private_feed.rs380/408. Same gap as original positions timer. |
| 15 | toNumber | supported / input | Numeric/fallback toNumber with explicit callable/depth exclusions; weex.utils.ts39 and utils.rs129. Surrogate does not survive numeric output. |
| 16 | WeexExchange.fetchOrders | supported / host/error reporting | Declared additional rejected-branch reporting in fetchOrders; weex.exchange.ts392 and account_reads.rs490. Arbitrary host aggregate ownership/timing not inferred. |
| 17 | normalizeBalance | finding / explanation | normalizeBalance returns original rows as info (weex.utils.ts345), while utils.rs462 clones. New source probe observes info.tag D800 and both input/output mutation directions. Whole-result and alias costs omitted. |
| 18 | WeexKlineRow | supported / input | Declared JSON/UTF-8 tuple input cost, bounded by actual outgoing numeric-candle consumer weex.exchange.ts428-443 and response_types.rs36. FORCED tuple-output inquiry withdrawn; no raw unit escapes that inspected path. |
| 19 | normalizeCandle | supported / date host/input | Numeric-only normalizeCandle projection; weex.utils.ts349 and utils.rs352. Coercion/depth/timezone restrictions explicitly priced; no retained-unit output. |
| 20 | normalizePositionSide | supported / input | Closed position-side enum/fallback and dynamic/depth exclusions; weex.utils.ts154 and utils.rs74. |
| 21 | normalizePublicTrade | finding / explanation | Actual normalizePublicTrade returns id D800, directly asserted; weex.utils.ts451 and public_trade.rs28/48 require String/whole-record decoding. Plain CHOSEN omits exact result cost. |
| 22 | normalizeTimestampToMs | supported / date host/input | Timestamp-to-ms numeric output with explicit timezone/DST and bounded/executable input costs; weex.utils.ts49 and utils.rs171. |
| 23 | normalizeOrderBookSide | supported / input | Numeric levels and explicit iterable/coercion restrictions; weex.utils.ts462 and utils.rs239. Source surrogate price filters; no retained output-unit inference. |
| 24 | WeexPublicWebsocket | supported / assembly | Public constructor external21 adapters/eight fields/twenty arrows/prototype dispose honestly disclosed; weex.ws-public.ts19 and classes.rs143/public_feed.rs184. No fixed host body wiring claim. |
| 25 | normalizePositionMode | supported / input | Closed position-mode enum/fallback and declared dynamic/depth input cost; weex.utils.ts148 and utils.rs52. |
| 26 | WeexExchange | supported / assembly | Exchange constructor requires complete external callable table before super; weex.exchange.ts118 and classes.rs120. Target labels explicitly not callback semantics. |
| 27 | WeexExchange.fetchIncomeHistoryRange | unresolved / producer boundary | Shared pager composition and inherited typed/alias/drop choices named; income_reads.rs126 and weex.exchange.ts525. Exact WEEX producer/caller retained-unit or alias effect unproved; no foreign mapper witness transferred. |
| 28 | normalizeTriggerType | supported / input | Closed trigger enum/default and explicit dynamic/depth restriction; weex.utils.ts174 and utils.rs118. |
| 29 | normalizeExecution | finding / behavior | normalizeExecution returns D800 symbol. Additionally actual JSON maker:null plus commission:{toString:null} throws later conversion TypeError, while fills.rs78 maker refusal pre-empts79-83. Native path inspected, not newly executed. weex.utils.ts400-401 carries maker then evaluates commission. |
| 30 | normalizeMarginType | supported / input | Closed margin enum/default with explicit executable/depth input choice; weex.utils.ts151 and utils.rs63. |
| 31 | WeexOrderResult | finding / explanation | Actual parseOrderResult consumer returns D800 orderId; weex.exchange.ts905-912 and response_types.rs120. Whole-record refusal omitted; no diagnostic-only errorMessage inference. |
| 32 | normalizeIncome | finding / explanation | Actual normalizeIncome returns tranId D800; weex.utils.ts432 and income.rs28 require whole-record String decoding. REST fetch/range-pending wording is stale; those rows are ported/divergent. |
| 33 | normalizeOrderType | supported / input | Closed order-type enum/fallback and dynamic/depth choices; weex.utils.ts169 and utils.rs107. |
| 34 | WeexPrivateWebsocket#throttledFetchBalance | unresolved / lifecycle | Balance refresh reporting cost priced, ready-resolution/drop timing unproved; weex.ws-private.ts228 and private_feed.rs392/408. No new behavioral defect asserted. |
| 35 | normalizeHistoricalTrade | finding / behavior | Actual normalizeHistoricalTrade returns D800 orderId. It inherits early maker refusal, and fills.rs132 validates buyer before later timestamp133 versus source428 later timestamp after carrying buyer. Exact second-Date.parse runtime count not executed; native ordering inspected only. |

## Evidence and withdrawals

Original source profiles and twenty controls measure `3f2d53594f`. The [supplement](full-source-observations.mjs) and [runner](run-full-controls.py) measure `bc1af045fbc773062dea0458a4e7c6f805ecb7a2`: all-profile command0, all21 baseline0/mutant1/restored0 at the initially named assertions with tracked inputs unchanged. The maker control initially changed TypeError to Error and failed the class assertion; that attempt is retained but rejected as same-class precedence proof. A later targeted control uses TypeError and must fail the exact later-commission message assertion. [Receipts/logs](full-measured-bc1af045fb/controls.json) retain that input SHA. There are41 selected source-profile controls across the original/supplement batches, with the maker profile requiring the separately recorded proof repair, with overlapping producers; not41 independently mutated native bodies. No Rust/native mutation or new native test was run.

The WEEX maker counterexample is ordinary represented JSON: maker:null and commission:{toString:null}. Actual source throws the later conversion TypeError; inspected native maker projection returns earlier. HistoricalTrade inherits it and separately validates buyer before its second timestamp. The latter second-Date.parse count has not been newly executed; no paired native runtime result is claimed.

The new Kraken leg-classifier observation is a program comparison: identical D800 units match while D800/DC00 do not. This is distinct from enum/numeric helpers that erase the unit. The native Value/string boundary and the actual ledger-classifier call are inspected. No private sort/signing caller inference is transferred from this exported/ledger comparison.

Four unresolved rows remain: WEEX positions/orders/balance refresh ready-future/drop timing, and the exact WEEX range producer/alias boundary. Supported reporting/pager costs do not close those questions. Withdrawn inquiries include forced output claims from numeric/enum/kline consumers, standalone private sort/account-delegation units without the real caller proof, source-immediate OKX POST despite mapSeries payload await, and additional Kraken ticker/order/Spot-OHLCV unit claims not executed. No complete SDK/host/browser assembly or exhaustive runtime behavior claim is made.

TS venue source trees remain byte-identical to d74. Integration changed OKX income/rest-operations in PR7914; those known historical findings are explicitly separated above. Other audited native venue production trees are unchanged by integration. All production changes unique to this extension are zero. Prior measurements retain their actual SHAs; merge-down/documentation/annotation descendants do not inherit runtime measurements as though rerun.

The extractor writer/check and full audit metadata gates are recorded separately after annotation. No Cargo check/test, native production control, workspace build/clippy, wasm lane, global parity reader, live exchange call or CI rerun was executed for this audit.

The same-class proof repair measured `2a71dbd69e`: all-profile baseline0; targeted
maker baseline0/mutant1/restored0, now failing the exact later-commission
message assertion with TypeError on both source paths. [Proof-repair receipt](same-class-control.json)
links the complete SHA/logs. The earlier class-changing attempt remains visible
and is not accepted as same-class precedence proof. Static017 receipts precede
this stronger proof predicate; final integrity receipts are recorded separately.

Final full metadata/static gates measured `aebc8ffd331162480d54cfc8d02b4efea0b47840`:
extractor writer0 (Updated0), full integrity0, deliberate in-memory status
corruption1 at FULL-AUDIT-ZERO-STATUS, restored integrity0, source syntax0,
extractor-check0 (Would update0; zero added/orphaned). [Final gate sheet](full-static-aebc8ffd33/gates.json)
retains exact commands and published-log hashes. The final read-only sub-agent
delta review returned MERGE at aebc8ffd33, resolving the earlier same-class
control/citation block; no gates were rerun by the reviewer.
