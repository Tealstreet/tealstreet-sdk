# POLYMARKET native websocket record declarations

Eight rows **pending → ported**, zero to divergent: APIOrder, Trade, WSSubscription, WSOperation, WSPriceChange, WSOrderBookUpdate, WSTrade and WSOrderUpdate. Independent tomllib census **66 / 18 / 57 → 74 / 18 / 49**, 141 rows in seven manifests. [Census](evidence/ws-records/census.json) stops at seventh file `polymarket.ws-public.ts.toml`; census, target resolution and native projection proof are separate claims.

Branch **feat/polymarket-ws-records-97d0e0**. Actual original parent **97d0e03c6f938c7e53d7ecb97e7518084267e871**, directly integration; no unreviewed lane branch ancestor. Implementation **daff02c2224704935350855590edae50f8d321b0**. Integration merged down before gates at **f60b85e1fef58dff9e3531714f4a93b1a14fe47a**, including **7495cc9eec52fd1c6444775455859a45e20ad247**. That merge SHA is the measured gate SHA. The separately finished in-hand MEXC slice `4cf1c70604ebd73e171026a8572b2d64c3c57651` is not this branch's starting ancestor; future work here is POLYMARKET only.

| Gate at measured SHA | Measured exit/result |
| --- | --- |
| Scoped POLYMARKET cargo check, all targets, locked/jobs2 | 0 |
| Scoped POLYMARKET cargo test, locked/jobs2 | 0; 80 passed, zero failed; zero doc tests |
| Scoped POLYMARKET fmt check | 0 |
| Standalone actual-declaration source oracle | 0; 32 successful fixture roots |
| Official POLYMARKET-only invention/status checker | 0; 141 rows, 237 indexed public Rust items, zero exemptions |
| Actual Yarn parity:extract:check | 0; output inspected: would update0 manifests, zero added/orphaned symbols |
| Mutation driver | 0; 15 compiled controls, baseline0 / named runtime101 / exact restored0 each |

[Commands/exits](evidence/ws-records/gates.json), [control summary](evidence/ws-records/controls/summary.json), raw outputs and source corpus are tracked alongside. Cargo invocations were sequential. Oracle calls used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Twenty-six new indexed declarations comprise eight root records, three nested records, nine literal-union types, five native boundary types/traits and Decode::decode. The official checker resolves every off-pending row; no target was invented or exemption introduced.

The source TypeScript compiler inventories actual declarations, array element types, nested properties and both intersection aliases against a fixed independent shape expectation. Root field counts are independently fixed at **13,14,5,4,3,7,15,14**. TypeScript declarations are erased: fixture structural checks and native refusal tests prove declared shapes, not existing TypeScript runtime validation. The scope is densely populated own-record/native-value-tree projection, not general JS objects/accessors/proxies, object identity, JSON text parsing, venue wire behavior, handlers or full class composition. WSTrade/WSOrderUpdate decode the same flat input into a native base record plus typed discriminator. Existing UTF-8 record implementations and their divergent rows remain unchanged.

Direct native assertions inspect every projected field, literal member, array count/slot, UTF-16 code unit, Number bits/NaN and Optional::Absent/Undefined/Value. Root and nested expectations use fixed roles, not the artifact's shape or length. Strings retain astral pairs and lone high/low surrogates. Numeric fixture0 values are independently distinct by full field role, including created_at/expiration; cases1–3 preserve negative zero, NaN and2^53. [Independent timestamp expectations](evidence/ws-records/timestamp-expectations.json). There is no Rust wire serializer or native re-encoding comparison; every assertion concerns the decoded native value. The tagged fixture bridge reads native values from corpus JSON, not venue JSON text.

## Controls and bounded floors

| Isolated fault | Named assertion | Baseline / mutation / restored |
| --- | --- | --- |
| PolymarketAPIOrder | `native text PolymarketAPIOrder.id/0` | 0 / 101 / 0 |
| PolymarketTrade | `native text PolymarketTrade.taker_order_id/0` | 0 / 101 / 0 |
| PolymarketWSSubscription | `native text PolymarketWSSubscription.markets[0]/1` | 0 / 101 / 0 |
| PolymarketWSOperation | `native text PolymarketWSOperation.assets_ids[0]/1` | 0 / 101 / 0 |
| PolymarketWSPriceChange | `native array count PolymarketWSPriceChange.price_changes/0` | 0 / 101 / 0 |
| PolymarketWSOrderBookUpdate | `native text PolymarketWSOrderBookUpdate.bids[0].price/1` | 0 / 101 / 0 |
| PolymarketWSTrade | `native text PolymarketWSTrade.taker_order_id/0` | 0 / 101 / 0 |
| PolymarketWSOrderUpdate | `native text PolymarketWSOrderUpdate.id/0` | 0 / 101 / 0 |
| boundary.UTF16 | `native text PolymarketTrade.id/1` | 0 / 101 / 0 |
| boundary.NumberSign | `native Number bits PolymarketTrade.match_time` | 0 / 101 / 0 |
| boundary.OptionalPresence | `native optional presence PolymarketWSOperation.assets_ids/3` | 0 / 101 / 0 |
| source.Intersection | `actual eight source declarations including intersections/nested fields` | 0 / 101 / 0 |
| boundary.TimestampIdentity | `native Number bits PolymarketAPIOrder.created_at` | 0 / 101 / 0 |
| source.RequiredUndefined | `required undefined union refused` | 0 / 101 / 0 |
| source.ArrayUndefined | `required undefined union refused` | 0 / 101 / 0 |

Each mutation compiled and failed its named runtime assertion. Twelve production projection/value faults and three actual-source declaration faults are retained in31 raw baseline/broken/restored logs. No survivor or wrong failure in this15-control floor; enumeration stops after source.ArrayUndefined. The driver restores original Rust and TypeScript bytes in finally and verifies final equality. All reviewers stopped reading before mutations.

Source floor: **8 × 4 = 32 successful roots**, stopped at fixture3 per type and whole source fixture31, WSOrderUpdate/3. Native floor: same32 valid root projections plus6 malformed-root refusal attempts = **38 root decode attempts**. Optional-array refusals run two record families with two inputs each, stop at WSOperation/undefined element1; literal and required-omission refusals stop at missing maker_order_id. Nested nonempty arrays independently expect two elements through index1, empty arrays zero. Numeric cases stop at index3; non-NaN bits and native NaN predicate are asserted. Nine current literal-union domains are represented across four profiles, including all four status/order-type members; this is bounded coverage, not an exhaustive future-schema census or all possible inputs. Concurrent Rust test scheduling is not a global execution-order claim.

## Development and review

Uncommitted diagnostics at the original parent are not gates covering the measured SHA. First native draft exited101 during compilation because apiKey was not snake_case; renamed Rust api_key while retaining source lookup apiKey. Corrected draft2 exited0/10 tests. A local patch generator exited1 on quoting syntax before making changes; the following draft3 shell test repeated old inputs and exited0/10. Corrected draft4, with both review fixes applied, exited0/10. [Development receipt](evidence/ws-records/development.json) and raw logs are retained. The actual manifest writer exited0 with zero added/orphaned symbols after parseManifest/serializeManifest; arrays were not hand-spelled in TOML.

[Static sub-agent review](evidence/ws-records/review.md) initially found indistinguishable created_at/expiration expectations and descriptor filtering of required/array-element Undefined arms. Both fixed before implementation commit, with added discriminating controls. Optional PROPERTY undefined remains supported; required/element undefined drift is now refused. Reviewer checked all15 anchors and declared26 targets, ran no gates, then stopped readers. Final static pass had no remaining actionable finding.

## Final merge preservation

Pre-push merge includes integration **7e9e5c660472d985e231394357b25ed171b929f2** at **8db37f98ee66df4b4cf836d38f58c3d67a203873**. [Receipt](evidence/ws-records/merge-preservation.json): own src/tests diff0; initial31-path broader diff1 because Cargo.lock and MANIFEST-SPEC changed. Remaining29 paths are byte-identical, including scripts, manifests, TS trees, seven dependency source/config pairs, checker/extractor code, Cargo configuration and yarn.lock. Package.json diff0. The lock change solely adds the existing safe-cex-crypto dependency to foreign safe-cex-venue-bitget; strict whole-lock parsed equality after removing exactly that edge passes0, with no version or other dependency changes. The entire old spec remains an unchanged prefix; its sole append documents Orderly REST bodies. Independently expected zero POLYMARKET exemptions remain zero. Earlier semantic-receipt attempt incorrectly assumed the lock package was Orderly and exited1 before proof; the reviewed package is Bitget. The failed raw diff and receipt attempts are retained, not reported as byte equality.

Relevant POLYMARKET gate inputs survive unchanged. No fresh scoped Cargo/control exit is claimed at the merge head. A fresh [global extractor](evidence/ws-records/merge-extractor.json) there exited0 with would update0 manifests, zero added/orphaned symbols. The evidence-only commit changes no gate inputs.

Not run: workspace compile/tests, full monorepo suites, host clippy, wasm clippy, CI rerun, live REST/websocket, handler behavior, venue JSON parser, general runtime/object identity or full production class composition. Other49 pending rows remain pending. No new MEXC work in this branch.
