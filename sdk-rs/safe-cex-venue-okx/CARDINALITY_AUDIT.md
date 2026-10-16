# Comparison cardinality audit

Scope: 16 tracked OKX root Rust targets plus support/ws.rs and five P3b targets, inspected by reading test bodies and native/expected comparison paths. Zero truncating pairwise comparisons were found in this scope. This does not exclude oracle-driven nested loops: the ticker gate had a measured variant-4 defect. Inspection is not mutation-proven coverage.

| Targets | Comparison boundary |
| --- | --- |
| api_policy, exchange_policy, numeric_helper_identity, numeric_ingress, pretty_symbol | Source case totals or fixed profile inputs; direct scalar/optional native assertions |
| field_identity, mappers | Direct native fields and complete income Vec equality; no native-driven pairing |
| market_policy | 401 source cases; complete matching-market Vec equality plus borrowed identity checks |
| ping, public_socket, subscription, ticker_events | Complete effects/frames/stats collections; source totals and expected-driven native ticker lookup |
| position_adapter | 310 source cases; entire record and complete callback Vec equality |
| response_phase | Fixed source indexes and type assertions; no native collection output |
| rest_operations | 118 source cases; entire ID Vec and independent HTTP bytes/order/count assertions |
| tables | Complete table equality and expected-driven literal inventory |
| support/ws | Recording/scheduling only; no comparison loop |
| P3b backend_api, runtime_backends | Entire refusal/read vectors and chunk maps; explicit clear emptiness |
| P3b field_identity, numeric_ingress | Explicit chunk/candle indexes and scalar field identity |
| P3b owner_lifecycle | Exactly 20 replies plus complete expected ID-set equality |

Three collection-return consumers receive independent compiling empty-output and subset-drop controls: matching markets, income records and placement IDs. Their existing comparisons assert full collection equality rather than a zip. The subset mutation removes one output only when there are multiple outputs; it preserves a nonempty result. These controls establish current discrimination, not historical immunity before earlier identity supplements. No corpus, feeder or golden bytes are rewritten.

The separate P4-utils calendar test contains a zip with a preceding expected/native length equality, but only prints the total source rows. An empty source corpus could therefore satisfy both counts at zero. This was routed to the coordinator; P4-utils owns the row and was authorized to repair that test file. It is excluded from the owned five-target P3b verdict.

Browser rollback and LoadedChunks execution remain UNEXECUTED. Native inspection or wasm lint does not establish execution coverage.

## Measured controls and repair

Six independent production changes compiled: empty and nonempty subset outputs for get_markets_for_index_ticker_inst_id, map_okx_bill_to_income_records and placement IDs. All targets failed at exit 101. Market controls failed `case 398`; income controls failed `mapper case 29`; placement empty failed `placement 33` and subset failed `placement 34`. Restored targets passed 4/0/0 across three binaries. This proves those current full-collection assertions discriminate; it does not prove every collection site or any pre-sweep baseline was immune.

A temporary source capture mutation replacing every expected ticker patch with {} passed 4/0/0 including 209 live cases, while comparing zero numeric fields. Outer case, write and trace counts remained correct. The original source artifact was restored byte-identically: SHA256 0195f72902be89659c2ca85e267323ddac600d0f54cd442204554cda9a3f7012. No past green is relabeled.

The ticker test now requires 209 cases, 216 source/native writes and 792 source/native numeric comparisons. Each handler has a fixed field inventory (ticker six, funding two, other handlers one), each successful case has one write except index ticker with two, and native loops use these independent bounds rather than oracle lengths or keys. Source per-write fields are checked before comparison. Three permanent compiled test-consumer controls drop every expected field, drop one field per write, or bypass both field-count guards after dropping all fields. The first two fail at exit101 on TICKER-CARDINALITY source field total; bypass fails on TICKER-CARDINALITY expected field identity. Restored ticker target passes 5/0/0, including the control children and live corpus.

The subsequent six-shape broadcast expands the audit to extra native outputs and subcorpus removal. That expanded proof is not claimed by the six empty/subset production controls or the ticker repair above.

## Expanded source inventories and native extras

The original source captures remain unchanged. Twelve changed OKX targets passed 24/0/0 after pinning operation/profile subcorpora in separate Rust assertions, including method distributions, disposal/failure combinations, pretty hot/cold/error paths and journal known/success/error combinations. The inventory counts were measured once per full source corpus on darwin arm64 Node24.20.0, then retained as independent literal expectations. These assertions are supplements, not claims that older corpora had those protections.

Three additional compiling production mutations duplicate a matching market, an income record or a placement ID. All fail101 at complete collection equality. The ticker Store fixture now requires exactly its two seeded records; P3b typed-record tests require one candle, and the KV identity witness requires one chunk, sparse width100 and exactly one occupied candle.

A consuming production decoder mutation parses the entire input record before clearing its candle vector; both typed-record targets fail101 at CACHE-CARDINALITY decoded candles and CACHE-CARDINALITY numeric ingress candles. Removing these two length assertions while keeping the consuming-empty mutation still fails101 at native indexing, not serde trailing-input rejection. Thus this site's preexisting index access already rejects emptiness; the new explicit length assertions supply named diagnostics and independently detect extra outputs. They are not claimed to be the sole empty-output protection.

The extra-candle decoder mutation passed the original two targets3/0/0, then failed both new length assertions. The permanent cache script retains consuming-empty, extra, extra-without-guards and consuming-empty-without-guards controls with byte restoration in finally. Its extra-without-guards mode demonstrates the length assertions are load-bearing for excess output. This is a test-gap reproduction using injected production code, not a shipped decoder defect.

Expanded audit remains scoped: complete mutation proof is recorded per operation. It does not establish every array site in shared num/types/transport or every unimplemented OKX runtime row.

## Additional executed controls

For matching markets, income records and placement IDs, source total assertions and operation-group assertions were temporarily bypassed while production returned empty collections. All three compiled and failed101 at complete native/expected collection equality. These are independent of the six earlier empty/subset controls and three surplus-output controls; twelve production variants are recorded for these three consumers.

An injected open-interest writer appended an unrelated third ticker without adding a notification. It compiled and failed101 at TICKER-CARDINALITY native stored records. Three injected KV load-result mutations independently add a chunk, extend sparse width, or add an occupied candle; one compiler artifact ran the three fixed controls, failing101 at CACHE-CARDINALITY native chunk count, sparse width and occupied candles respectively. The script restores all source and tests byte-for-byte.

The consuming-empty/extra cache script does not discard unread serde input: ChunkRecordWire finishes parsing before the output vector is cleared or extended. Extra-without-guards passes3/0/0; consuming-empty-without-guards fails by native indexing. Those opposite results distinguish surplus-output blindness from preexisting empty-output protection rather than attributing either to serde trailing-element validation.

## String-selected assertions and filtered witnesses

At integration dbdf3d15dd, a read-only whole-suite inspection found five conditional assertions without execution counts and two filtered every assertions without nonempty floors. An independent sub-agent confirmed the same seven sites: Rust pong delay; source ticker mark, index-success and index-error; source REST inherited status; and source position constructor/prototype filters. Existing operation-group guards and the source public dispatch 8/3/1 arity floors were counted protections, not these gaps. AST-selection and CSV filters are separate from oracle-case assertions.

The repair pins actual assertion executions: pong 1; ticker 19/18/1; REST inherited status 5; each position filter 18. The branch-controls script disables each conditional or empties each filter independently and requires its named failure, restoring source bytes in finally. Original output records are retained; these execution guards add no historical independence claim and move no manifest row.
