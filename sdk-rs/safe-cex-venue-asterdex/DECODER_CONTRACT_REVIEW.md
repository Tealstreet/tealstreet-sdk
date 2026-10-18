# Remaining decoder record contracts

This batch reviews eleven original Aster native/default record owners. The
eight wire owners retain their existing decoding behavior, including opaque
extension maps and the twelve-element kline protocol tuple. The three workflow
inputs have the concrete repairs below. This is a selected-owner review, not a
whole-package accepted-set gate.

| Owner | Native fields | Independent raw-text observations |
| --- | ---: | ---: |
| AsterDEXMarketResponseFiltersItem | 13 | 41 |
| AsterDEXMarketResponse | 23 | 102 |
| AsterDEXPositionResponse | 17 | 63 |
| AsterDEXOrderResponse | 25 | 92 |
| AsterDEXTickerResponse | 17 | 79 |
| AsterDEXKlineResponse | 12 | 45 |
| AsterDEXWebSocketMessage | 4 | 24 |
| AsterDEXPlaceOrderOpts | 2 | 118 |
| ExecutionOrder | 2 | 120 |
| CheckParams | 9 | 30 |
| ApproveParams | 6 | 24 |

All 130 direct fields are listed above. Projections also expand nested records
and all seventeen core order fields.
Each of the 738 independently authored observations executes actual production
Deserialize through owned Value, qualified borrowed Value, direct JSON text and
qualified text Deserializer plus end-of-input verification. Native projections
inspect actual fields, enum variants, Option presence, binary64 bits, vectors
and recursive opaque Value kinds/order/bits. Actual Clone must reproduce every
observation. The order extension also executes qualified From and Borrow,
compares every core field and requires both absent STP and the original borrowed
field address. Expected values do not come from serializing decoded owners.

The source witness checks actual erased TS declarations and Node JSON.parse
numeric bits. Its three tooling DTO schemas use individually reviewed null
normalization only after the separate actual-runtime witness described below.
This does not claim that TS has a runtime schema validator or that normalized
tooling inputs equal the raw TS static input shapes.

## Measured decoder repairs

On master `c2bfbd5560`, all three tests in
`tests/accepted_decoder_records.rs` failed after actual compilation:

- CheckParams accepted the complete valid nine-field array in Serde order.
- ApproveParams accepted the complete valid six-field array in Serde order.
- ExecutionOrder rejected valid direct decimal and negative-zero order JSON with
  `invalid type: map, expected f64`, under flatten and arbitrary_precision.

The first two now use the maintained concrete object guard and private
serde(remote) companion, preserving field order, renames and Option rules. The
third captures the complete input through the existing shared numeric ingress
before the flattened remote decoder. Its private Serialize-only projection
checks numeric output without adding a public serialization trait or creating
another Deserialize owner. All three ordinary regression tests pass after the
repair. See the canonical [object-only recipe](../safe-cex-parity/accepted-set/MAP-ONLY-INGRESS.md)
and [numeric ingress requirement](../parity/MANIFEST-SPEC.md#numeric-ingress-defect).

The flattened wire records already reject positional arrays without an outer
object guard. Their negative-array examples document the actual boundary but
are not distinguishing guard-removal controls. Kline explicitly accepts its
complete valid tuple and rejects short/long arrays, objects and scalars.

## Per-field workflow null review

Explicit null preserves the existing native Option-to-None behavior only for
the following ten fields. Every TS use was reviewed for validators, undefined
tests, property-presence checks, spread/default replacement and serialization.
None distinguishes null from omission in its eventual runtime effects.

| Owner / field | Actual maintained TS use |
| --- | --- |
| CheckParams.apiKey | builder-approval.ts:274, `apiKey || signerAddress` |
| CheckParams.baseUrl | builder-approval.ts:267,270 forwards to falsy guards at 51-75 |
| CheckParams.builderConfig | builder-approval.ts:236,245 optional chaining; 311 truthy fee fallback |
| CheckParams.builderAddress | builder-approval.ts:236,245 nullish fallback |
| CheckParams.requiredFeeRate | builder-approval.ts:311 nullish fallback |
| CheckParams.testnet | builder-approval.ts:261,266-267 forwards to truthy chain/endpoint selection; api.ts:80-83 |
| ApproveParams.baseUrl | builder-approval.ts:414-415 forwards to falsy guards at 51-75 |
| ApproveParams.builderName | builder-approval.ts:362 forwards to typed-data nullish default at 201; body nullish default at 406 |
| ApproveParams.testnet | builder-approval.ts:365 forwards to truthy typed-data chain at 202, body chain at 407 and endpoint at 414; api.ts:80-83 |
| ExecutionOrder.postOnly | exchange.ts:886 forwards to builder-fee.ts:127 strict `=== true`, via fee selection at 134 |

`scripts/accepted-tooling-null-source.cjs` executes these actual unmodified TS
consumers with a fresh source nonce Map and identical clock for each comparison.
Twenty-four CheckParams scenarios cover approval, insufficient cap, invalid
address and config-only address selection. Six ApproveParams scenarios exercise
successful signing/submission and invalid inputs. Comparisons include result,
actual signature/query bytes, wallet typed-data payload, URLSearchParams body,
proxy calls, headers and fetch effects; assertions force the intended paths.
Another 120 comparisons cover every actual source order/time-in-force enum value
and four API types through the actual maker/fee bodies. All 150 null-versus-
omission comparisons pass. Native cases separately observe every nullable field
through all four Deserialize routes. Ordinary wire Option fields continue to
reject explicit null; there is no blanket optional-field allowance.

## Compiled controls and gates

`scripts/accepted-decoder-contracts.mjs` refreshes native/default rustdoc,
inventories exactly the eleven owners and compares every compiler field in
declaration order with the observation field list. It uses maintained contract
and policy shard readers, one Cargo job, and requires eleven runtime receipts,
zero classifications and zero failures. The source gate first executes the
150 actual-runtime null comparisons, then the 738 independent shape/bit checks.

`scripts/accepted-decoder-controls.py` executed eleven isolated compiled faults:
remove only each repaired DTO object guard; bypass ExecutionOrder whole-input
capture; lose a filter, market, position, order, ticker, kline or WS numeric sign;
and discard STP presence in order options. The filter fault also reaches the
nested market, and the STP fault also reaches ExecutionOrder. Every control
requires exactly the specified owners to fail native value/admission assertions,
then byte-exact restoration and a passing eleven-owner run. A compilation error
or missing receipt cannot count as proof.

At the original review checkpoint, the native baseline passed all eleven runtime
owners, zero explicit classifications and zero failures. Its ordinary inventory
discovered 184 owners and eight aliases, with no new decoder owners from the three
remote companions or the private output projection. The distinct corpus
contains 738 inputs; its source gate and the 150 runtime null comparisons pass.
All eleven faults compiled and failed the exact named native observations. The
filter and STP faults each failed two related owners; the other nine each failed
one. The baseline and all eleven byte-exact restorations passed all eleven
runtime owners with zero classifications and failures (23 receipt sets total).

Other original Aster owners, lexical Wire decoders, private companions' separate
capabilities, aliases, features and WASM remain outside this selected execution.
The canonical [string boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary)
and [typed output boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary)
still apply.

## Master integration

The normal master merges retain all 159 existing contract records, including
the separately reviewed Wire owners from #868, and adds only these eleven keys.
The selected runner does not assert the historical package-wide owner total.

Both ExecutionOrder helper sets have production callers. The binding's
placeOrder/placeOrders decode ExecutionOrder; its private Wire/output projection
preserves whole-input numeric capture. OrderExecution's placement and amendment
workflows consume PreparedExecutionOrder. The latter carries already prepared
options; neither helper set replaces or falls back to the other.

The refreshed three ordinary workflow regressions, 738 actual-source observations,
150 runtime null comparisons and local parity pass after this merge. Core optional
model fields in these records are observed through their JSON presence/value;
retained native undefined admission is outside this JSON decoder corpus.

The refreshed selected runner passes eleven production-native owners with zero
classifications or failed owners. Replacing both workflow source files with
master7d76523624f4 compiles and fails all three ordinary regressions: the two DTOs
accept their complete arrays, and ExecutionOrder rejects decimals with
`invalid type: map, expected f64`. Exact restoration passes all three. The required
all-features cancellation inventory also passes.
After the final master union, source comparisons, parity7464/16636, scoped strict
Clippy, formatting and the required all-features cancellation target pass.
