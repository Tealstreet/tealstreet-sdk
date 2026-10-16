# Bybit static table lookup audit

Measured at source revision `7607f3ef86d9eaa3875c169b9873a7b7e922a9b9`.
Run `node sdk-rs/safe-cex-venue-bybit/scripts/table-lookup-audit.mjs`.
The AST walk inspected 27 TypeScript files (the Bybit directory excluding
`.test.ts` files, plus its journal position adapter), 389 element-access
expressions. **101 static protocol/configuration table accesses: 100 venue + 1 journal.**
Nested brackets are separate operations, including fixed-key reads.
Closed source is distinct from immunity: numeric TypeScript annotations alone
do not prove numeric coercion, and typed unions alone do not validate raw input.
No uppercase, numeric-conversion, null-prototype or namespace guard was added
to obtain immunity. URL rows state the ordinary network source; their category
lookup is separately listed, not assumed protected by the network ternary. This is not
101 wire-controlled keys: URL/network and rate-table keys often have constrained
callers. Array indexes, raw-record reads, mutable dictionaries, Store indexes and
handler registries are excluded from this static-table count, and emitted separately
by the script for inspection. The selection is explicit, not a general alias-analysis
proof; it includes lowercase typeMap, rateLimitEntry, inverseObj products and inline
position-index maps, not only uppercase names.

**13 results feed local truthiness/`||`; 3 further results feed `??`.**
Classification follows local variables inside the owning method; it does not claim
whole-program downstream control-flow analysis. Conditions selecting whether a lookup
runs are not tests of its result. Key-side `||` is distinguished explicitly.

`ts-bug:` inherited Object.prototype members are truthy functions (and __proto__
is an object), not missing values. Do not turn these into ordinary unknown-key
fallbacks. No venue-local prototype map or shared enum widening is authorized.
Full order/income/history/journal integration stays pending. The inherited-side
function-valued portion now cites the CHOSEN closed-output declaration below;
the __proto__ object-valued portion remains FINDING. No uppercase-key immunity is used at its side lookup. The existing inverse
position claim is already divergent, with source witnesses for 12 inherited keys
and numeric positionIdx bypasses: it refuses the function-valued side instead of
preserving it. The shared TableHit/ProtoMember representation is now consumed by inverse_position,
replacing its local twelve-name list. The typed Position.side boundary still cannot
carry an inherited function/object; that refusal is not a ported claim.

The 478-case inverse-position oracle passes with explicit `typeof side` and
truthiness assertions, in addition to the serialized result and Rust refusal.
Replacing the live source lookup with an own-key-guarded lookup makes the scoped
Cargo test fail at exit 101: `not an undefined table miss` (string versus function).
Restoring the source passes. This is a source-correction detector, not evidence
that Rust currently reproduces function-valued fields.

## Every static-table access

| Site | Owner | Expression | Result truthiness/fallback | Consequence / boundary |
| --- | --- | --- | --- | --- |
| exchanges/bybit/bybit.api.ts:85 | createAPI | `DEMO_URL[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.api.ts:85 | createAPI | `BASE_URL[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.api.ts:90 | createAPI | `DEMO_URL[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.api.ts:90 | createAPI | `BASE_URL[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.api.ts:123 | getOppositeBybitEnvironmentProbe | `BASE_URL[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.api.ts:131 | getOppositeBybitEnvironmentProbe | `DEMO_URL[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.exchange.ts:504 | getBybitRestBaseUrl | `DEMO_URL[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.exchange.ts:504 | getBybitRestBaseUrl | `BASE_URL[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.exchange.ts:508 | getBybitPublicWsUrl | `BASE_WS_URL.public[category][network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.exchange.ts:508 | getBybitPublicWsUrl | `BASE_WS_URL.public[category]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.exchange.ts:512 | getBybitPrivateWsUrl | `(this.usesBybitDemoEnvironment() ? DEMO_WS_URL : BASE_WS_URL).private[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.exchange.ts:1259 | updateBatchLimitBasedOnVipLevel | `rateLimitEntry[BybitProductType.Futures][accountType]` | No | Closed source, not immunity: product/account keys selected from fixed enum branches; outer value feeds >= 20, not truthiness. |
| exchanges/bybit/bybit.exchange.ts:1259 | updateBatchLimitBasedOnVipLevel | `rateLimitEntry[BybitProductType.Futures]` | No | Closed source, not immunity: product/account keys selected from fixed enum branches; outer value feeds >= 20, not truthiness. |
| exchanges/bybit/bybit.exchange.ts:1792 | fetchOHLCV | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:2434 | placeOrder | `ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce &#124;&#124; OrderTimeInForce.GoodTillCancel]` | No | The || is inside the KEY expression, not on the lookup result. |
| exchanges/bybit/bybit.exchange.ts:2462 | placeOrder | `inverseObj(ORDER_SIDE)[opts.side]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:2463 | placeOrder | `inverseObj(ORDER_TYPE)[type]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:2835 | formatOrderForBatch | `ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce &#124;&#124; OrderTimeInForce.GoodTillCancel]` | No | The || is inside the KEY expression, not on the lookup result. |
| exchanges/bybit/bybit.exchange.ts:2862 | formatOrderForBatch | `inverseObj(ORDER_SIDE)[opts.side]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:2863 | formatOrderForBatch | `inverseObj(ORDER_TYPE)[type]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:3257 | mapPosition | `POSITION_SIDE[p.side]` | Yes: &#124;&#124; | Inherited function suppresses Long fallback; positionIdx 1/2 bypass lookup. |
| exchanges/bybit/bybit.exchange.ts:3332 | mapOrder | `POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE[v(o, 'createType')]` | Yes: Boolean and &#124;&#124; via createTypeOrderType | Changes isStop and selects oType before the next ORDER_TYPE lookup. |
| exchanges/bybit/bybit.exchange.ts:3345 | mapOrder | `ORDER_TYPE[oType]` | No | Result feeds enum equality checks (TP/SL/Conditional), not || or a truthiness test. |
| exchanges/bybit/bybit.exchange.ts:3354 | mapOrder | `ORDER_SIDE[o.side]` | No truthiness; ?? | Inherited function suppresses inferProtectiveOrderSide. |
| exchanges/bybit/bybit.exchange.ts:3355 | mapOrder | `ORDER_TIME_IN_FORCE[o.timeInForce]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:3368 | mapOrder | `ORDER_STATUS[v(o, 'orderStatus')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:3384 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'triggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:3385 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'triggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:3386 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'triggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:3405 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'slTriggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:3419 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'tpTriggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.exchange.ts:3474 | getStopOrderPositionIdx | `{ 0: 0, 1: 2, 2: 1 }[positionIdx]` | No | Closed source, not numeric-coercion immunity: getOrderPositionIdx returns only 0/1/2 (spot always 0). |
| exchanges/bybit/bybit.income.ts:53 | bybitTransactionCategory | `TYPE_CATEGORY[String(type ?? '')]` | Yes: &#124;&#124; | Inherited function suppresses other/original-income-type fallback. |
| exchanges/bybit/bybit.income.ts:86 | push | `CATEGORY_INCOME_TYPE[recordCategory]` | No | Derived category key; function from preceding TYPE_CATEGORY can flow here. |
| exchanges/bybit/bybit.income.ts:146 | fetchBybitIncomeHistory | `typeMap[opts.incomeType]` | Yes: &#124;&#124; | Inherited function suppresses other/original-income-type fallback. |
| exchanges/bybit/bybit.order-history.ts:55 | mapBybitHistoricalOrder | `ORDER_SIDE[String(row.side ?? '')]` | Yes: !side | Inherited function bypasses null-return guard. |
| exchanges/bybit/bybit.order-history.ts:61 | mapBybitHistoricalOrder | `ORDER_TYPE[stopType]` | Yes: &#124;&#124; | Inherited function suppresses later order-type/default Limit fallback. |
| exchanges/bybit/bybit.order-history.ts:61 | mapBybitHistoricalOrder | `ORDER_TYPE[String(row.orderType ?? '')]` | Yes: &#124;&#124; | Inherited function suppresses later order-type/default Limit fallback. |
| exchanges/bybit/bybit.order-history.ts:70 | mapBybitHistoricalOrder | `ORDER_STATUS[String(row.orderStatus ?? '')]` | No truthiness; ?? | Inherited function suppresses Closed fallback. |
| exchanges/bybit/bybit.order-history.ts:76 | mapBybitHistoricalOrder | `POSITION_SIDE_BY_IDX[String(row.positionIdx ?? '')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.ws-private.ts:36 | connectAndSubscribe | `(this.parent.isDemo ? DEMO_WS_URL : BASE_WS_URL).private[network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.ws-public.ts:73 | connectAndSubscribe | `BASE_WS_URL.public[this.category][network]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.ws-public.ts:73 | connectAndSubscribe | `BASE_WS_URL.public[this.category]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybit.ws-public.ts:152 | maybeRecoverOhlcvStream | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.ws-public.ts:358 | listenOHLCVs | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.ws-public.ts:378 | listenOHLCVs | `INTERVAL_INVERSE[interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.ws-public.ts:401 | listenOHLCV | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybit.ws-public.ts:569 | listenOHLCVOld | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:721 | updateBatchLimitBasedOnVipLevel | `rateLimitEntry[BybitInverseProductType.Futures][accountType]` | No | Closed source, not immunity: product/account keys selected from fixed enum branches; outer value feeds >= 20, not truthiness. |
| exchanges/bybit/bybitinverse.exchange.ts:721 | updateBatchLimitBasedOnVipLevel | `rateLimitEntry[BybitInverseProductType.Futures]` | No | Closed source, not immunity: product/account keys selected from fixed enum branches; outer value feeds >= 20, not truthiness. |
| exchanges/bybit/bybitinverse.exchange.ts:1073 | fetchOHLCV | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:1543 | placeOrder | `ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce &#124;&#124; OrderTimeInForce.GoodTillCancel]` | No | The || is inside the KEY expression, not on the lookup result. |
| exchanges/bybit/bybitinverse.exchange.ts:1573 | placeOrder | `inverseObj(ORDER_SIDE)[opts.side]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:1574 | placeOrder | `inverseObj(ORDER_TYPE)[type]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:1964 | formatOrderForBatch | `ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce &#124;&#124; OrderTimeInForce.GoodTillCancel]` | No | The || is inside the KEY expression, not on the lookup result. |
| exchanges/bybit/bybitinverse.exchange.ts:1991 | formatOrderForBatch | `inverseObj(ORDER_SIDE)[opts.side]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:1992 | formatOrderForBatch | `inverseObj(ORDER_TYPE)[type]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2387 | mapPosition | `POSITION_SIDE[p.side]` | Yes: &#124;&#124; | Inherited function suppresses Long fallback; positionIdx 1/2 bypass lookup. |
| exchanges/bybit/bybitinverse.exchange.ts:2459 | mapOrder | `POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE[v(o, 'createType')]` | Yes: Boolean and &#124;&#124; via createTypeOrderType | Changes isStop and selects oType before the next ORDER_TYPE lookup. |
| exchanges/bybit/bybitinverse.exchange.ts:2472 | mapOrder | `ORDER_TYPE[oType]` | No | Result feeds enum equality checks (TP/SL/Conditional), not || or a truthiness test. |
| exchanges/bybit/bybitinverse.exchange.ts:2473 | mapOrder | `ORDER_TIME_IN_FORCE[o.timeInForce]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2481 | mapOrder | `ORDER_STATUS[v(o, 'orderStatus')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2484 | mapOrder | `ORDER_SIDE[o.side]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2494 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'triggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2495 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'triggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2496 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'triggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2515 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'slTriggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2529 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'tpTriggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.exchange.ts:2569 | getStopOrderPositionIdx | `{ 0: 0, 1: 2, 2: 1 }[positionIdx]` | No | Closed source, not numeric-coercion immunity: getOrderPositionIdx returns only 0/1/2 (spot always 0). |
| exchanges/bybit/bybitinverse.ws-private.ts:34 | connectAndSubscribe | `(this.parent.isDemo ? DEMO_WS_URL : BASE_WS_URL).private[ this.parent.options.testnet ? 'testnet' : 'livenet' ]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybitinverse.ws-public.ts:95 | connectAndSubscribe | `BASE_WS_URL.public['inverse'][this.parent.options.testnet ? 'testnet' : 'livenet']` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybitinverse.ws-public.ts:95 | connectAndSubscribe | `BASE_WS_URL.public['inverse']` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybitinverse.ws-public.ts:276 | listenOHLCVs | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.ws-public.ts:296 | listenOHLCVs | `INTERVAL_INVERSE[interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.ws-public.ts:319 | listenOHLCV | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitinverse.ws-public.ts:461 | listenOHLCVOld | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:670 | fetchOHLCV | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:766 | placeOrder | `ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce &#124;&#124; OrderTimeInForce.GoodTillCancel]` | No | The || is inside the KEY expression, not on the lookup result. |
| exchanges/bybit/bybitspot.exchange.ts:787 | placeOrder | `inverseObj(ORDER_SIDE)[opts.side]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:788 | placeOrder | `inverseObj(ORDER_TYPE)[type]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:1208 | mapOrder | `POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE[v(o, 'createType')]` | Yes: Boolean and &#124;&#124; via createTypeOrderType | Changes isStop and selects oType before the next ORDER_TYPE lookup. |
| exchanges/bybit/bybitspot.exchange.ts:1216 | mapOrder | `ORDER_TYPE[oType]` | No | Result feeds enum equality checks (TP/SL/Conditional), not || or a truthiness test. |
| exchanges/bybit/bybitspot.exchange.ts:1217 | mapOrder | `ORDER_TIME_IN_FORCE[o.timeInForce]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:1230 | mapOrder | `ORDER_STATUS[v(o, 'orderStatus')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:1233 | mapOrder | `ORDER_SIDE[o.side]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:1245 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'triggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:1246 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'triggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:1265 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'slTriggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:1279 | mapOrder | `ORDER_TRIGGER_TYPE[v(o, 'tpTriggerBy')]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.exchange.ts:1292 | getStopOrderPositionIdx | `{ 0: 0, 1: 2, 2: 1 }[positionIdx]` | No | Closed source, not numeric-coercion immunity: getOrderPositionIdx returns only 0/1/2 (spot always 0). |
| exchanges/bybit/bybitspot.ws-private.ts:34 | connectAndSubscribe | `(this.parent.isDemo ? DEMO_WS_URL : BASE_WS_URL).private[ this.parent.options.testnet ? 'testnet' : 'livenet' ]` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybitspot.ws-public.ts:95 | connectAndSubscribe | `BASE_WS_URL.public['spot'][this.parent.options.testnet ? 'testnet' : 'livenet']` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybitspot.ws-public.ts:95 | connectAndSubscribe | `BASE_WS_URL.public['spot']` | No | Configuration lookup; ordinary network chosen as livenet/testnet. Nested category lookup is counted separately. |
| exchanges/bybit/bybitspot.ws-public.ts:275 | listenOHLCVs | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.ws-public.ts:295 | listenOHLCVs | `INTERVAL_INVERSE[interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.ws-public.ts:318 | listenOHLCV | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/bybitspot.ws-public.ts:448 | listenOHLCVOld | `INTERVAL[opts.interval]` | No | Value only at lookup; downstream consumers still need domain checks. |
| exchanges/bybit/hyrotrader.exchange.ts:425 | getBybitRestBaseUrl | `HYROTRADER_SIMULATOR_URL[network]` | Yes: !url | Inherited function bypasses missing-URL error if out-of-domain network reaches method; normal caller supplies fixed network. |
| exchanges/bybit/hyrotrader.exchange.ts:436 | getBybitPublicWsUrl | `HYROTRADER_SIMULATOR_WS_URL.public.linear[network]` | Yes: !url | Inherited function bypasses missing-URL error if out-of-domain network reaches method; normal caller supplies fixed network. |
| exchanges/bybit/hyrotrader.exchange.ts:444 | getBybitPrivateWsUrl | `HYROTRADER_SIMULATOR_WS_URL.private[network]` | Yes: !url | Inherited function bypasses missing-URL error if out-of-domain network reaches method; normal caller supplies fixed network. |
| journal/bybit.position-adapter.ts:227 | foldRun | `CLOSE_REASON[String(first.execType ?? '')]` | No truthiness; ?? | Inherited function suppresses user fallback in journal record. |

## Shared lookup integration

After merging integration 6eb262afc5, inverse_position consumes js_table_lookup
against the existing POSITION_SIDE entries; the venue-local inherited-name list
is removed. All 478 source cases pass. Replacing Inherited with the Missing/Long
fallback compiles and fails the inherited-side Rust refusal assertion at exit
101; restoring it passes. This is distinct from the source-own-key mutation above.
Lookup support does not make the returned typed record faithful on inherited
hits; the later output ruling and its qualifications are recorded below.

## Output declaration qualification

Section 15 at integration 54e2d7e820 authorizes the function-valued closed-field
refusal as CHOSEN. The inverse-position note cites that declaration while keeping
the actual result explicit: TS returns the remainder of the Position record when
JSON omits its function-valued side; Rust returns Err for the entire mapping.
Those serialized outcomes do not converge. The existing __proto__ witness is
also distinct: typeof is object and serialized side is {}, not an omitted field;
its refusal remains a separate FINDING outside the function-omission rationale.

## Mutable handler-map arity follow-up

This is outside the static-table census above. The live public onMessage oracle
now covers all twelve inherited names as both exact and prefix topics: 24 paths.
Before serialization it records member kind/arity, receive/fallback/diagnostic
calls, stats, and the exception. Eight positive-arity functions record stats then
throw TypeError at indexed invocation; three zero-arity functions and the prototype
object return at the length guard. This is not WOO's iterable-handler error.
Current Rust own-map dispatch skips all twelve with no handler stats; the eight
positive-arity differences are FINDING, not covered by closed-output refusal.
The scoped dispatch oracle passes; changing the source length guard from >0 to
>=0 fails the named prototype-stats toString assertion, and restoration passes.
