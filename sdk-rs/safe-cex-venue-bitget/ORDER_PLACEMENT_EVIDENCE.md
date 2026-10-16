# Bitget placement bodies

Fresh branch `feat/rust-bitget-placement-bodies` has direct integration parent
**9f3703306ed458ab72645d55f892d45a0b14e78a**. Code and review SHA:
**05c0524ef40ece3f77abcec3eeb652a5a6732718**. No separate unreviewed lane
branch was an ancestor. Incoming integration changes are outside this review.

Python 3.11 tomllib census at that SHA: **8 pending -> ported, 0 -> divergent**;
112 ported / 80 divergent / 35 pending becomes **120 / 80 / 27**. Eleven
manifests, 227 unchanged `(file, kind, ts)` identities. Enumeration stopped at
sorted file eleven, `bitgetusdc.exchange.ts.toml`. Census is distinct from proof.

| TypeScript symbol | Executable Rust body in src/order_placement.rs |
| --- | --- |
| BitgetExchange.placeOrder | place_order |
| BitgetExchange.placeOrders | place_orders |
| BitgetExchange.placeOrderBatch | place_order_batch |
| BitgetExchange#placeUtaOrders | place_uta_orders |
| BitgetExchange#placeUtaStrategyOrders | place_uta_strategy_orders |
| BitgetExchange.placeAlgoOrders | place_algo_orders |
| BitgetExchange#cancelUtaStrategyOrders | cancel_uta_strategy_orders |
| BitgetInverseExchange.placeOrderBatch | inverse_place_order_batch |

Bodies preserve source capture/read order, original payload/error references,
caller-ID resolution, ensure/format/placement ordering, strategy versus normal
routing, positive-size filtering, array-literal spread, margin/group/chunk order,
optional response methods and holes, truthy ID fallbacks, catch boundaries and
schedule placement. Classic batch uses groupBy/chunk/omit; inverse retains the
ordinary object reducer, rest destructuring, repeated length reads and manual
50-item slices. Strategy cancellation retains undefined-only default options,
null failure, uniqBy IDs, lazy ID/client fallbacks, strict versus tolerated missing
orders, source logging and scheduling even when every request reports absent.

Ten exemptions were generated from the actual OrderPlacementHost trait, Iterator
associated type and eight methods. Supplied contracts provide receiver/captured
callee binding, native operators/intrinsics/imported operations, map/reduce/
forEach, native iteration/destructuring and spread, and runtime-owned async work.
Concrete Exchange/class/HTTP/socket assembly remains pending. No shared trait,
shared type, dependency or lockfile was changed by this slice.

## Native and source evidence floor

Six tests assert decoded native variants, independent fields/counts, f64 bits,
UTF-16 strings, payload/option/formatter/error aliases, endpoint choices, argument
counts and effects before source/wire comparisons. Counts come from literal
fixture recipes. Captured array methods assert native method selectors and owner
identity. Native HTTP owner is a supplied contract, not a separate decoded owner
assertion. There is no decode/re-encode gate.

The source oracle executes eight actual TS initializers, actual endpoint/enums,
API success/message/not-found helpers and omitUndefined with actual Lodash
partition/groupBy/chunk/omit/uniqBy/omitBy. Formatter, receiver, margin, HTTP,
classification and scheduling collaborators are fixtures. This is a bounded
native/source floor, not proof for arbitrary getters/proxies/prototypes/coercions,
overridden array methods, concurrent promise scheduling or ownership timing.
Promises in native fixtures settle in one poll.

| Group | Independent case floor | Last iteration |
| --- | --- | --- |
| Single placement | 8 | 7, formatter rejection |
| Bulk placement | 8 | 7, assertion failure |
| Classic/inverse batch | 16 | 15, inverse sparse response holes |
| UTA normal/strategy | 14 | 13, strategy empty |
| Algo placement | 5 | 4, empty |
| Strategy cancellation | 8 | 7, null options |

Total **59 cases**, not an exhaustive enumeration. Batch fixtures independently
provide 52 orders (51 BTC, one ETH), requiring 50/1/1 request lots. Sparse success
responses preserve undefined IDs. Other cases cover empty/no-ID responses,
formatter/request/margin failures, noniterable first placer return, UTA trailing
rejection, client/payload fallbacks, missing optional methods, strict/default
not-found handling and error identity.

Two precommit test attempts exited 101 because fixtures contradicted the source:
UTA no-ID still supplied a truthy clientOid, then strategy error supplied a message
where the fixture expected fallback. Corrected fixtures passed. These are fixture
corrections, not production defects or mutation evidence.

## Sub-agent review citation

`/root/account_read_review` reviewed exact committed
**05c0524ef40ece3f77abcec3eeb652a5a6732718**, verifying the direct parent above.
No blocking body/mapping findings across the eight rows within the supplied-native
scope. Review checked capture/read/catch ordering, routing, batching, ID fallback,
iteration, cancellation absence/default behavior, scheduling and original aliases.
Reviewer git reads exited **0**; tomllib eight transitions plus ten unique real
helper exemptions **0**; independent committed-source cancellation floor of eight
cases node/Python **0**, with seven TS dependencies checked byte-identical to the
review SHA. Cancellation enumeration stopped at case 7, null options. Loader root
was rebased for node -e. No edits. Reviewer did not run Cargo/native tests,
extractor, mutations, host/wasm clippy, workspace builds or CI. Broader prototype,
getter, coercion and temporal contracts remain the limits stated above.

## Measured gates

All Cargo runs use `/opt/homebrew/opt/rustup/bin` on PATH, `--jobs 2`, one Cargo
at a time. Native/source tests use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
At exact code SHA **05c0524ef40ece3f77abcec3eeb652a5a6732718**:

| Gate | Measured exit |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2 | 0, Finished dev |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract | 0; output read: zero updated/added/orphaned |
| same node/yarn parity:extract:check | 0; output read: zero would-update/added/orphaned |
| Python 3.11 tomllib census | 0, eight transitions above |

Mutation/restoration measurements and integration preservation are appended below
only after process completion.

Not run for this slice: full Bitget crate suite, workspace/monorepo builds,
host clippy, wasm clippy, CI or live-venue tests. Earlier full-suite measurements
in REST_ACTIONS_EVIDENCE.md belong to the previous slice and are not claimed as
new placement gates. Concrete assembly remains pending.

## Discriminating controls and restoration

At exact code SHA **05c0524ef40ece3f77abcec3eeb652a5a6732718**, eight one-at-a-time
mutations compiled successfully and each exited **101** at its named native
assertion. Each control was restored before the next; no survivor occurred.

| Control | Exact selected test | Named native assertion |
| --- | --- | --- |
| single-formatter | `native_single_then_source` | assertion `left == right` failed: PLACE native independent event count single:uta:call:formatUtaCreateOrder |
| bulk-filter | `native_bulk_then_source` | assertion `left == right` failed: PLACE native independent array count returned value |
| classic-margin-coin | `native_batches_then_source` | assertion `left == right` failed: PLACE native UTF16 50-row lot fields and boundaries.marginCoin |
| inverse-chunk | `native_batches_then_source` | assertion `left == right` failed: PLACE native independent array count 50-row lot fields and boundaries.orderList |
| uta-endpoint | `native_uta_then_source` | assertion `left == right` failed: PLACE native UTF16 UTA endpoint selection |
| strategy-endpoint | `native_uta_then_source` | assertion `left == right` failed: PLACE native UTF16 UTA endpoint selection |
| algo-plan | `native_algo_then_source` | assertion `left == right` failed: PLACE native UTF16 algo plan routing endpoint |
| cancel-default | `native_cancel_then_source` | assertion `left == right` failed: PLACE native independent event count cancel:absent:call:unlimitedXHR.post |

Stopped at control index 7, cancellation default; this is a bounded control
floor, not exhaustive mutation coverage. After restoring exact committed bytes,
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget
--test order_placement --jobs 2` exited **0**, six passed, 59-case floor.
Selected controls used the same command plus the named test and `-- --exact`.
Source/test/script byte preservation, edition-2024 rustfmt check on the two new
Rust files, patch whitespace and oracle node syntax each independently exited
**0** after restoration. Tomllib census rerun at the exact code SHA exited **0**.

## Integration merge and preservation

Merged integration **775a019fe06014b8b858b3f8f6cbdabac552abaf** down; exact merge SHA
**e9894bc1bf28258074a0823f4b5d10a2185fd104**. Sole conflict: parity/exempt.toml. Resolution
used the entire stage-3 integration file plus ten generated actual helper
records. Tomllib asserted all 1,153 integration records retained in order, all
ten own records retained and unique Rust symbols; exit **0**. No CLAUDE conflict.
No separate unreviewed lane ancestor; incoming integration is outside own review.

At this exact merge SHA, own src/tests/scripts scoped byte diff versus gate SHA
**05c0524ef40ece3f77abcec3eeb652a5a6732718** exited **0**. Recursive local Rust
crate/build/manifest and TS/package inputs excluding Cargo.lock diff exited **0**.
The wider diff including Cargo.lock exited **1**, disclosed: only the unrelated
safe-cex-venue-woo 0.1.0 record differs. Independent tomllib lock comparison
proved identical record keys/version and every one of **481** Bitget-reachable
records and dependency lists unchanged, Woo unreachable; exit **0**. BFS stopped
at index **480**, ucd-trie 0.1.7 (crates.io). This is dependency-record preservation,
not whole-workspace compilation or host/wasm feature coverage.

Tomllib census against merged integration exited **0**, again exactly eight
pending -> ported, zero divergent; eleven manifests / 227 unchanged identities,
112/80/35 -> 120/80/27, stopped at file eleven named above. Cargo checks/tests
and controls were not rerun after this merge: their actual exits remain measured
at code SHA 05c0524ef4, whose tested inputs are preserved as stated.

Fresh metadata gates at exact merge SHA
**e9894bc1bf28258074a0823f4b5d10a2185fd104** completed: manifest writer **0**,
output read zero updated/added/orphaned; extractor checker **0**, output read
zero would-update/added/orphaned. The following commit changes only this evidence
file; it is not a new Cargo measurement. Final scoped source/test/script diff
and whitespace checks are measured again before push. Integration merged above
was the latest observed fetched head for this delivery.
