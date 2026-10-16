# Bitrue audit remediation

Current integration validation is recorded in [WORKSPACE_GATES.md](WORKSPACE_GATES.md) at7b4b2980f9. It supersedes the historical NOT RUN list for the requested workspace/wasm commands without attributing them to9f. Incoming integration annotations changed7 already-ported rows outside the frozen divergent cohort; historical manifest comparisons remain bound to694→9f, and current comparisons use49a→7b.

Integration parent: `694e07f7e049a5a7047c41e994a0fc854dc78a59`. Merge-down `e98c0fd218a0d07afd4582b0a931acecb085a1ac` incorporates integration `8fcf323edfc6747fcdf4427ce19267a7147d018b`; Bitrue source/tests remain byte-identical to `db68c19363`. This branch starts directly there and has no unmerged Crypto.com ancestor. Crypto.com checkpoint16e62ae350 merged as PR7959: the5 pending and2 original socket behaviour findings were already closed by merged PR7872. The original Crypto20 therefore left18, audited without status movement.

Bitrue's frozen cohort is25 divergent rows: **3 previously supported scopes and22 findings** from [the five-venue audit](../parity/DIVERGENCE_BLOCK_AUDIT.md#bitrue-observations). The disposition is **16 rationale-to-write /6 code-to-fix**. The16 are explanation findings; the6 are scheduling findings. No additional behaviour defect was established in the read-only review of the16. All25 original statuses, targets, layers and identities remain unchanged: **82 ported /25 divergent /0 pending** over107 rows. The deterministic cohort reaches original iteration25, `BitrueExchange.fetchBalance`.

The canonical declarations remain in MANIFEST-SPEC: [§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), [§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict). The notes record local observations and costs, not alternative declarations.

## Scheduling fixes

All six entries now create their source request/subcall during invocation and return a future for post-await work. They do not eagerly poll a response. Their native callback type is `Result<Fut, ExchangeError>`, distinguishing synchronous initiation failure from a future rejection. Existing test hosts explicitly wrap successful initiation, without changing their actual-source corpora.

| Original iteration | Row | Repaired invocation effect |
| ---: | --- | --- |
|4|fetchSymbolMaxLeverage|call lower sync helper before returning; fallback Store read stays after await|
|7|updateSymbolMetadata|call lower sync helper before returning; ignore maximum after await|
|8|fetchBalanceAndPositionsRaw|start GET now; immediate initiation errors emit now; emitter failure remains returned future error|
|15|fetchPositionMetadata|decide missing keys/start cold fetch now, or finish supplied/cache branch now|
|16|fetchAndSyncSymbolMaxLeverage|guards and GET initiation now; keep parse/update/catch after await|
|25|fetchBalance|call account fetch now; balance_handle and parse stay after await|

The metadata no-await branch builds and atomically replaces before disposal, including `Some([])` clearing the cache; selected handles remain retained after cache replacement/clear. Cold processing happens after await against then-current state. No Store/cache guard spans a transport callback. Leverage keeps its named source catch: a notification failure can follow a committed Store write; the upper fallback subsequently sees that committed state. The earlier retained Arc balance repair remains intact.

`entry_timing.rs` asserts six native starts before any poll, zero response polls at entry, native outcomes, eager supplied metadata mutation/selection, empty clearing, rebuild-before-disposal and both immediate account error/emitter outcomes before invoking the source probe. `entry_timing_oracle.cjs` executes unchanged actual field initializers at explicit transport/Store hosts. The floor is6 cold rows, followed by metadata supplied/empty/disposed observations and2 account initiation-error cases, stopping emitter-failure=true. Older metadata corpus logging inside an async callback is continuation coverage, not additional entry proof. Unsupplied cache-hit timing, every other start-error combination, autonomous Promise completion, cancellation/observer-drop and exhaustive microtask behaviour are not certified by this fixture.

## Rationale scope

The16 notes replace generic UTF-8 claims with the actual materialized field/key, program observation and whole-input cost. They retain independently declared residual costs separately. This repairs the named explanation gaps; it is not a new25/25 whole-row supported-explanation score, certification of every historical CHOSEN clause, or behaviour-parity promotion.

| Iteration | Row | Exact observed loss |
| ---: | --- | --- |
|1|FETCH_ORDERS_RESPONSE|code D800; whole orders envelope refused|
|3|TICKER_WS_UPDATE|amount D800; whole ticker refused|
|5|pickTradeOrderId|returned bidId unit used as requested/returned order identifier; supplied record refused|
|9|getPositionAccounts|contractName unit retained in eligible account identity; whole supplied account envelope refused|
|10|FETCH_LEVERAGE_BRACKET_RESPONSE|inspectable msg unit; whole leverage envelope refused; no update failure inferred from ignored msg|
|11|resolveTradeQty|D800 market key selects multiplier5/quantity15; distinct FFFD key yields3; native symbol/Market string admission excludes original key|
|13|FETCH_ACCOUNT_RESPONSE|inspectable code unit; whole account response refused|
|14|setMarketMaxLeverage|D800 selected market updated to7, distinct FFFD market stays3; native original key/market unavailable|
|17|FETCH_CONTRACTS_RESPONSE|item0.symbol unit; entire array and other good entries refused|
|18|WS_ORDER_TRADE_UPDATE|unrestricted e unit; entire event refused; not proof an unknown event reaches a handler|
|19|normalizeTradeSymbol|returned D800USDT compared/returned by historical-fill path; supplied Value refused|
|20|FETCH_MY_TRADES_RESPONSE|inspectable msg unit; whole response refused; no effect attributed to a caller that discards msg|
|21|getAccountPositionSnapshots|snapshot symbol retained and used for metadata keys; whole supplied envelope/all snapshots refused|
|22|parseBalance|returned info.extra unit with usable balance31; whole input refused before any balance is built|
|23|reverseWsSymbol|D800_USDT becomes E-<D800>-USDT; underscore is required by the inspected private caller's reversal branch|
|24|FETCH_KLINE_RESPONSE|item0.high unit; entire array refused|

Local choices remain explicit in the row notes: shared JSON/undefined profiles instead of helper-only non-wire carriers for the three trade helpers; typed Market limits and subscriber-error projection for the setter; bounded recursion plus non-executable account profiles for the two account helpers; numeric Balance.upnl, owned info and bounded input for parseBalance. The existing depth160 source-accepted/native-refused account supplement and nonnumeric surviving-UPNL refusals price different costs from the new string probes. The two opaque wire records retain their own future_model/data element depth/NaN restrictions. These are separate input/output-policy explanations, not costs proved by a lone-surrogate fixture.

`boundary_prices.rs` directly asserts native whole-input refusals and representable helper/store/balance values before source execution. Seven independent JSON fixtures containing lone-surrogate units cover the8 helper findings (one envelope covers both account helpers), with an additional bracket exact-key fixture. Source observes string type/length/units, eligible identity, selected Store key and numeric consequences before JSON capture. The setter probe uses an explicit Store-update host; the older2250-case witness separately executes the actual source Store update/notification bodies. Historical-fill and private-order downstream chains are inspected, not newly claimed as complete live handler executions.

The8 erased wire declarations are covered by the maintained wire witness:444 probes across10 declarations,436 representable probes **including rejected shapes**, and8 lone-surrogate probes. Representable inputs exercise6 public/shared raw/owned/borrowed paths; surrogate refusals use2 raw paths. Decoded native fields, enum variants, cardinalities and number bits are asserted independently of serialized field names. This target has no subsequent ordinary record-encoder assertion; decoder coverage is not encoder coverage. These are schema/JSON.parse/decoder observations, not executable TypeScript response-validator parity.

## Retained supported scopes

The previous3 supported explanations retain their bounded subjects. `isSuccessCode` has21 actual-source comparisons; strict comparisons return bool and its surrogate supplement returns false. `parseMaxLeverageFromErrorMessage` has46 comparisons; ASCII-digit extraction and its suffix-unit supplement return5 without retained units. Neither generic string representation alone proves a local observable cost on those numeric/bool outputs; unsupported blanket wording is withdrawn.

The owner explicitly chooses to keep the two private helpers aligned with shared response/string admission, avoiding helper-specific acceptance rules and extra value carriers the inspected maintained calls do not otherwise use. A broader adapter is possible; this is a policy tradeoff, not a claim that classification needs a JavaScript engine. The caller AST floor is eight `data?.code` calls and one `errorMessage` call in the exchange file, ending at line1334. The error initializer reads `any`; these calls do not prove source values always fit the native profile. Source non-JSON values and lone-surrogate strings remain excluded. This supports only the declared input-profile explanation.

`parseMaxLeverageFromBracketResponse` retains80 ordinary comparisons and the independently measured200000-candidate source argument-stack RangeError/native iterative maximum3 choice. Numeric output alone does not make symbol units irrelevant: the new exact-key supplement measures D800 selecting second entry7 over first FFFD entry3; native refuses the original whole response and explicit replacement selects3. A native ordinary scalar B key selecting7 proves that key-selection assertion can distinguish a wrong first-entry lookup. This is an additional scoped representation price, not an unproved claim that all numeric outputs ignore UTF-16 keys.

## Behaviour confidence

The independently enumerated executable cohort has **17 rows**: the3 previously supported helpers,8 rationale helper functions and6 scheduling entries. Passing actual-source/native corpora, new phase probes and explicit mismatch probes are reported at their measured SHA in the gate sheet. The8 other cohort rows are erased wire declarations and reported separately. Seventeen is not derived from test-function counts or the total25 cohort length.

The executable row identities are `isSuccessCode`, `parseMaxLeverageFromErrorMessage`, `parseMaxLeverageFromBracketResponse`, `setMarketMaxLeverage`, `normalizeTradeSymbol`, `pickTradeOrderId`, `resolveTradeQty`, `getPositionAccounts`, `getAccountPositionSnapshots`, `parseBalance`, `reverseWsSymbol`, `fetchAndSyncSymbolMaxLeverage`, `fetchSymbolMaxLeverage`, `updateSymbolMetadata`, `fetchBalanceAndPositionsRaw`, `fetchBalance`, `fetchPositionMetadata`. This is the frozen25-row audit cohort only, stopping its final iteration25; it is not an enumeration of every Bitrue function.

An explanation correction alone establishes no behaviour parity. These are finite source-paired observations under the admitted host/data profiles, not17 wholly equivalent implementations or a venue-wide coverage census. No status promotion or exhaustive equivalence claim is made. Source output units that native admission refuses remain intentional, explicitly priced divergence. The source/corpus/view/continuation limits above stay visible even when the gates pass.

## Evidence

See AUDIT_REMEDIATION_GATES.md and the frozen per-row disposition/annotation hashes. Gates and controls retain their actual SHAs. The first boundary fixture had a Store inherent/trait update compilation error; the first entry probe had boxed-error struct-literal errors. These are retained compiler preflights, never runtime discrimination proof. A Git index-lock rejection was retried after the earlier commit finished; no lock removal or history rewrite occurred. The old Crypto development Cargo-overlap deviation remains disclosed on both PR bodies; this queue runs one Cargo/Cargo-spawning operation at a time. No human review gate or self-merge is involved.
