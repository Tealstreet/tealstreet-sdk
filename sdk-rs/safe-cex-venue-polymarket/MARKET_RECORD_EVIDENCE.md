# POLYMARKET market and trading record declarations

Nine rows **pending → ported**, zero to divergent: Token, GammaMarket, Position, SignedOrder, CreateOrderRequest, CancelRequest, ExchangeOptions, GammaMarketInfo and DataPosition. Independent tomllib census **74 / 18 / 49 → 83 / 18 / 40**, 141 rows in seven manifests. [Census](evidence/market-records/census.json) stops at seventh file polymarket.ws-public.ts.toml. Current manifests had49 pending at the supplied parent; the user estimate41 was not used as a measured count. Census, target resolution and native projection proof remain separate claims.

Branch **feat/polymarket-market-records-0e014c**. Actual original parent **0e014c0aa040874ed83ad9f13d9ae617c57a6435**, directly integration; no unreviewed lane branch ancestor. Implementation **b88d90e0862dd6e5837808317556bc18e3de8af4**. First integration merge/gate SHA **d2d9254e0d661b77976e7d8c4d2aa4443a29c248**, including **f6c865ddd8331c32516f063d90d045761bd2c73b**. Final merge includes integration **5c3c55b7b5631e9d04b3d31bc98b213b5888f18d** at **dad345cf9b7ffcd03c3a4d415fa9f7b511a49fc0**. That final merge SHA is the fresh measured gate SHA below.

| Gate at final measured SHA | Measured exit/result |
| --- | --- |
| Scoped POLYMARKET cargo check, all targets, locked/jobs2 | 0 |
| Scoped POLYMARKET cargo test, locked/jobs2 | 0; 91 passed, zero failed; zero doc tests |
| Scoped POLYMARKET fmt check | 0 |
| Standalone actual-declaration source oracle | 0; 54 successful fixture roots |
| Official POLYMARKET-only invention/status checker | 0; 141 rows, 249 indexed public Rust items, zero scoped exemptions |
| Actual Yarn parity:extract:check | 0; output inspected: would update0 manifests, zero added/orphaned symbols |
| Mutation driver | 0; 15 compiled controls, baseline0 / named runtime101 / exact restored0 each |

[Commands/exits](evidence/market-records/gates.json), [controls](evidence/market-records/controls/summary.json), raw outputs and source corpus are tracked alongside. Cargo ran sequentially. Oracle calls used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Twelve new indexed declarations are nine root records plus Outcome, Nullable and NumberOrText. Shared ws_records Decode/Text/Optional/Side/OrderType remain their existing actual targets. Every off-pending venue row resolves; no target or exemption was fabricated.

The source checker inventories actual TypeScript properties, nested records/array elements, nullable unions and imported SignatureType. Independently fixed root field counts: **3,17,9,13,3,3,5,22,14**. It refuses unresolved heritage, unexpected call/construct/index signatures, readonly changes, required/element Undefined and singleton BooleanLiteral narrowing. Current full false|true boolean domains remain bool. Source declarations are erased; native refusal tests and fixture validators prove declared shapes, not TypeScript runtime validation.

SignatureType members are resolved from the actual constants file, and the actual checker independently asserts Number-type assignability to that enum. Numeric signatureType/signerType fields therefore retain f64, including negative zero, NaN, Infinity and arbitrary Number values, rather than narrowing to the four closed Rust constant variants. This is declaration/representation proof, not signing or API acceptance proof.

Native fields are directly asserted against independent field-role expectations: every text unit, numeric bit/NaN, literal, array count/slot, property presence and nullable arm. Optional::Absent, Optional::Undefined, Optional::Value(Nullable::Null), false and zero remain distinct. Nullable boolean Value profiles1/4/5 are independently **negRisk[F,T,F], enableOrderBook[T,F,T], acceptingOrders[T,F,F]**; every role sees both values and distinct vectors. Numeric fixture0 uses distinct field-role values. [Independent expectations](evidence/market-records/independent-expectations.json). Text includes astral pairs, lone high/low surrogates, empty strings, NUL and whitespace. No Rust wire serializer or decoded-value re-encoding comparison exists. The tagged corpus bridge consumes native values from corpus JSON, not venue JSON text.

Scope is dense own-record/native-tree projection. General JS objects/accessors/proxies, object identity, venue JSON text parsing, handlers, signing, normalization and full exchange/class composition are separate. Existing UTF-8 records and divergent rows are unchanged. Other40 pending rows remain pending; no MEXC work was added.

## Controls and proof floors

| Isolated fault | Named assertion | Baseline / mutation / restored |
| --- | --- | --- |
| PolymarketToken | `native literal PolymarketToken.outcome/0` | 0 / 101 / 0 |
| PolymarketGammaMarket | `native array count PolymarketGammaMarket.tokens/0` | 0 / 101 / 0 |
| PolymarketPosition | `native text PolymarketPosition.token_id/0` | 0 / 101 / 0 |
| PolymarketSignedOrder | `native text PolymarketSignedOrder.makerAmount/0` | 0 / 101 / 0 |
| PolymarketCreateOrderRequest | `native text PolymarketCreateOrderRequest.order.salt/0` | 0 / 101 / 0 |
| PolymarketCancelRequest | `native optional presence PolymarketCancelRequest.orderIds/1` | 0 / 101 / 0 |
| PolymarketExchangeOptions | `native text PolymarketExchangeOptions.apiKey/1` | 0 / 101 / 0 |
| GammaMarketInfo | `native text GammaMarketInfo.id/0` | 0 / 101 / 0 |
| PolymarketDataPosition | `native Number bits PolymarketDataPosition.size` | 0 / 101 / 0 |
| boundary.NumberOrText | `native number/string arm GammaMarketInfo.orderPriceMinTickSize/0` | 0 / 101 / 0 |
| boundary.Nullable | `native nullable arm GammaMarketInfo.orderMinSize/3` | 0 / 101 / 0 |
| boundary.NumericEnum | `native Number bits PolymarketSignedOrder.signatureType` | 0 / 101 / 0 |
| boundary.NullableFlags | `native bool GammaMarketInfo.enableOrderBook/5` | 0 / 101 / 0 |
| source.NumericEnumMembers | `actual nine market/order/options declarations` | 0 / 101 / 0 |
| source.BooleanSingleton | `singleton boolean literal requires distinct native type` | 0 / 101 / 0 |

Thirteen production projection/value faults and two actual-source declaration faults compiled and failed their named runtime assertions. Fifteen-control floor ends after source.BooleanSingleton. No survivor or wrong failure in either the initial or fresh full control pass; both sets of31 baseline/broken/restored logs are retained. All source readers stopped before mutations. Driver restores exact Rust, type-source and constant-source bytes in finally; final restoration diff0 is recorded.

Source floor: **9 × 6 = 54 successful root fixtures**, stops at profile5 per type and whole source fixture53, DataPosition/5. Native floor: same54 successful projections plus6 malformed-root refusal attempts = **60 root decode attempts**. Refusal sequence stops at missing curPrice in DataPosition. Nested nonempty arrays independently expect two elements through index1, empty arrays zero at profile4. Numeric profiles0..5 cover distinct finite values, negative zero, NaN, Infinity,2^53 and a negative fraction, stopping at5. Non-NaN bits and native NaN predicates are asserted; no NaN-payload exhaustiveness claim. Outcome/Side/OrderType literal coverage is bounded by these six profiles; imported numeric enum membership inventory stops at POLY_1271, fourth member. Concurrent Rust test scheduling is not a global iteration-order claim.

## Review and development

[Static sub-agent review](evidence/market-records/review.md) found nullable-boolean Value coverage gaps and singleton-boolean erasure. Both fixed before implementation commit, with dedicated controls; final static pass found no actionable issue. Reviewer executed no gates or index counts, then stopped all readers.

[Development receipt](evidence/market-records/development.json): initial standalone source oracle0/54. Initial native attempt101 was not a compiled mutation: generator assertion stopped before creating the test target, and cargo reported no market_records target. Corrected draft2 passed11 tests; corrected draft3 after both review fixes passed11. These uncommitted diagnostics at the original parent are not gates covering the final SHA. Actual manifest writer0, zero added/orphaned symbols, after parseManifest/serializeManifest; no hand-spelled TOML arrays. Raw outputs retained.

## Merge and refreshed gates

[Initial SHA-bound gates and controls](evidence/market-records/initial-gates/gates.json) all passed at d2d9254e0d661b77976e7d8c4d2aa4443a29c248. Final merge preservation showed own src/tests diff0, but broader31-path diff1: Cargo.lock, MANIFEST-SPEC and shared safe-cex-crypto source changed, including new Poseidon loader/example modules. That raw failed preservation attempt is retained in [receipt](evidence/market-records/initial-gates/merge-preservation.json) and raw diff. No claim of whole-input byte preservation was made. All six scoped gates and all15 controls were freshly rerun at dad345cf9b7ffcd03c3a4d415fa9f7b511a49fc0; the final table and raw logs cover that SHA and the new dependency inputs. The evidence-only commit changes no gate inputs.

Not run: workspace compile/tests, full monorepo suites, host clippy, wasm clippy, CI reruns, live REST/websocket, general JSON/runtime/identity, signing or full production class composition gates. The scoped run built crypto as a dependency; its standalone crypto tests were not run.
