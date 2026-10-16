# Polymarket order and book adapters

**8 pending → ported; 0 pending → divergent.** Triple **66 / 18 / 57**, total141. [Census](evidence/order-adapters/census.json) checks an independently named eight-symbol set across seven venue manifests against actual requested parent **`e20df553203cb460fb7b4f46cf8dd65d9c589c3a`**. Branch `feat/polymarket-order-adapters-e20df5` starts directly at that reviewed integration parent, with no unreviewed private lane ancestor. Census is separate from runtime proof.

## Callable bodies and limits

Exchange: #mapOrder, fetchOrders, cancelOrders, cancelAllOrders, fetchOrderBook, listenOrderBook. Private websocket: #mapOrder, #handleOrderUpdate. Exact function renames and all actual receiver/runtime/task/anonymous-record targets are individually mapped. Full classes, constructors and concrete host composition remain pending. The official scoped checker validates all off-pending targets with an independently empty exemption file.

Mapping captures the token-map receiver/get callable before evaluating asset_id. An absent required map raises the original access error before reading asset_id; an absent optional private map skips that first read. Falsy symbol lookup rereads asset_id, unknown status rereads status, and original_size/size_matched are each read again for remaining. Enum-object property lookup retains inherited constructor entries. Native Order contains all12 source fields, original info identity, f64 values/Number bits and current virtual clock.

Order reads use current mapOrder over live ordinary Array.map slots and preserve holes. Cancel checks length before mapping, rereads length for map and again after await for its message. The cancellation ID array is newly mapped; it retains original native values and holes while later input mutation changes only the post-await count. Book conversion reads bids before asks, maps live slots and preserves holes with independent running totals. Listener passes the original callback, invokes current connect after subscribing and returns exact child cleanup. Connect failure keeps earlier effects; cleanup drop has no automatic unsubscribe behavior.

Private order updates store the exact shared mapped object, then inspect the later input status. Fill fields are read after store mutation and before debug logging; filled supplies amount. Native id/status log reads occur after preceding effects/coercions. Try/catch logs failures and lets catch-log failures propagate. fetchOrders logs and rejects original failures; synchronous API exceptions log before returning the observer.

Entire asynchronous method tasks, including post-await mapping/coercion/logging, are started and retained by the host independently of their observer. Dropping the observer does not cancel the API operation or suppress method effects. Await observation first yields/schedules before polling even an already-ready child, matching source microtask order. Synchronous input/call exceptions do not invent an await delay. Hosts must execute/retain scheduled tasks and queue wakeups as continuations.

Runtime bindings supply ECMAScript property/ToPropertyKey, abstract ToString, lowercase, enum lookup, captured Map.get, ordinary Array.map, current child calls, original failures and virtual clock. Native Order/Fill/Book project ordinary source records/arrays. General JavaScript parser/encoder/proxy/accessor/species implementations, full client pipeline, classes/construction, autonomous scheduling and network are outside this slice. UTF-16 identifiers, symbols, request/error/log values retain lone surrogates. Replacement happens only inside parseFloat, where lone-surrogate/U+FFFD terminates a numeric prefix. No native wire comparison or re-encoding is used. NaN is checked by native is_nan/source NaN tag; no NaN payload-bit parity is claimed.

## Gate exits attached to SHA

Final scoped exits and controls were measured at **`1f0778de1716c7973efd812dab544e595fa787da`**, after merging integration **`ef054ba0b8e9038241576f44c09f652348f2f4bb`** down. [gates.json](evidence/order-adapters/gates.json) contains exact commands and TypeScript oracle environment. Raw logs are tracked; Cargo runs were sequential.

| Gate | Result |
| --- | --- |
| Scoped Cargo check, Polymarket, all targets, locked, jobs2 | 0 |
| Scoped Polymarket tests, locked, jobs2 | 0; 70 passed, zero failed; doc tests zero cases |
| Scoped Cargo fmt check | 0 |
| Official venue-only invention/status checker | 0; 141 rows, 211 public Rust items; zero exemptions |
| Actual Yarn parity:extract:check | 0; output reports would update0 manifests, zero added/orphaned symbols |
| Ten compiled controls, eight row faults plus two async boundaries | 0; shared10-test baseline0, each mutation101 and exact restored-test0 |

The extractor parse/serialize API edited TOML arrays, followed by the writer: output updated0 manifests, zero added/orphaned symbols. An initial uncommitted mapping script reported ENOENT for a missing .toml path suffix before writing any manifest; its numeric exit was not retained. This was corrected before the writer above. Development test drafts exited101 on a trace-builder borrow/lifetime error, then unused fixture storage, then four synchronous-exception prefix captures missing in the oracle. Each was corrected before committed gates. Corrected focused ten-test draft passed0. [Development logs](evidence/order-adapters/development/) retain those measured failures; they cover dirty drafts, not this gate SHA. Two scoped dirty production compile checks measured0. No committed failed-control attempt is silently dropped.

## Native proof and controls

The Node oracle executes original AST-selected bodies, original enum maps/types and logging severities in ordinary receiver scopes. It captures native plain record keys, object identities and values/Number bits before JSON output. Native assertions inspect each of12 mapped Order fields, five Fill fields and every book price/amount/total/slot directly, plus original info/error/callback/cleanup identities. Independent numeric expectations also assert amount, remaining, filled and per-side totals. Expected prefixes/traces/counts come from fixed scenario rules, not artifact lengths. Synchronous failures, sparse slots, post-call input mutations, live field rereads, store mutation before fill, generated/original errors, ignored observers and already-ready completion timing have separate witnesses.

| Isolated fault | Named native assertion | Baseline / mutation / restored |
| --- | --- | --- |
| Exchange map: change required map to optional | native token map mode | 0 / 101 / 0 |
| Private map: change optional map to required | native token map mode | 0 / 101 / 0 |
| fetchOrders: discard mapped slots | native mapped slot:fetchOrders | 0 / 101 / 0 |
| cancelOrders: read asset_id instead of id | native cancellation id | 0 / 101 / 0 |
| cancelAllOrders: change info severity to debug | native log severity | 0 / 101 / 0 |
| fetchOrderBook: exchange bid/ask arrays | independent native book bid count | 0 / 101 / 0 |
| listenOrderBook: omit connect | native synchronous prefix:exchange.listenOrderBook | 0 / 101 / 0 |
| handleOrderUpdate: use amount instead of filled | native fill amount:native Number bits | 0 / 101 / 0 |
| Task boundary: drop whole body instead of owning it | native abandoned method effects:fetchOrders | 0 / 101 / 0 |
| Await boundary: bypass first yield | native ready-child prefix:fetchOrders | 0 / 101 / 0 |

[Control summary](evidence/order-adapters/controls/summary.json) and21 raw baseline/broken/restored logs retain actual reached iteration and measured SHA. Every final fault compiled and reached its named runtime assertion; restored source bytes match exactly. No survivor was observed within this ten-control floor.

Coverage floor: **8 bodies ×12 scenarios =96** source/native root calls, plus **4** discarded-observer calls and **4** already-ready-child calls, **104** calls per full passing ten-test corpus. Source capture occurs once per process. Passing exact row restores run12 native roots; boundary restores run4 native roots; source capture still runs104. Base enumeration stops at fixture95, private #handleOrderUpdate/scenario11; ignored and ready enumerations each stop at their fourth method, fetchOrderBook/scenario0. Cleanup replays stop at2; enum inherited-key floor is constructor only; mapped Order fields stop at12/lastUpdate, Fill fields at5/amount, standard bid slots at3 and ask slots at2. Controls stop at10/awaitYield; census stops at manifest7/polymarket.ws-public.ts.toml. These are finite floors, not complete venue parity.

Static sub-agent review found that retaining only the child API operation did not preserve post-await body effects after observer drop, and synchronous fetch catch was deferred incorrectly. Entire-body task ownership and eager synchronous catch fixed those findings. Review then requested a ready-child microtask witness and an explicit required-map error contract; first-poll yield, four ready-child witnesses and the missing-required-map case address them. Final review found no remaining actionable issue within stated boundaries and verified mappings/control anchors; readers stopped before mutations. Reviewer ran no Cargo/oracle/control gates.

No workspace compilation/tests, host clippy, wasm clippy, CI, MEXC gates, real network, general runtime implementations, autonomous executor/timer or full-client pipeline/wire gates were run.

## Final merge preservation

The final merge includes integration `0f7fd2ce92e79d0c38e87ea5dd15b21927ea8c1a` at comparison head `b4b7e0d0a97686d6305c68da091a38c22fc0dd83`. [Preservation receipt](evidence/order-adapters/merge-preservation.json) records required venue src/tests diff0 and extended30-input diff0 against gate SHA `1f0778de1716c7973efd812dab544e595fa787da`: venue source/tests/scripts/manifests, TS venue/enum/logging/original-parser inputs, seven dependency sources/manifests, Cargo.lock/workspace settings, checker/extractor and package/lock inputs are unchanged. Venue exemptions remain independently expected0; scoped checker supplies an empty exemption file. Post-merge census remains66/18/57; source/test/script/manifest whitespace check0. Prior scoped/control exits remain attached to their measured SHA; no fresh scoped run at this comparison head is claimed. Fresh global extractor there exited0, with output reporting zero manifest changes. This following evidence-only commit changes no gate inputs.
