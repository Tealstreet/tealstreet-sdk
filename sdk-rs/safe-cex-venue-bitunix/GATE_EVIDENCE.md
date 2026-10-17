# Bitunix gate evidence

## REST reads

Measured code SHA: `e5be8a653988d3df165be65db5a0304a99728d06`.
Integration parent: `f7db35f4c42fb35bde2038d30274ab57fa86de43`.
This code commit starts directly on integration, without inherited unmerged lane commits.
The subsequent evidence commit changes only this sheet; the recorded exits belong to the code SHA above.

Python `tomllib` over all nine Bitunix manifests, comparing the parent to the code SHA:
**0 pending → ported; 4 pending → divergent**. Before: **13/15/94**;
after: **13 ported / 19 divergent / 90 pending**, with 122 total rows.

| Source method | Rust target in `src/rest_reads.rs` | Bucket |
| --- | --- | --- |
| `fetchMarkets` | `fetch_markets` | MIXED: FORCED shared numeric amount limit; CHOSEN JSON/model and host callback boundaries |
| `fetchTickers` | `fetch_tickers` | CHOSEN: shared JSON/models and caller-owned I/O/store callbacks |
| `fetchBalance` | `fetch_balance` | CHOSEN: shared JSON/models and caller-owned I/O/store/report callbacks |
| `fetchPositionMode` | `fetch_position_mode` | CHOSEN: shared JSON and caller-owned I/O |

The GET future supplies `xhr.response.data`. Request endpoints/configuration,
source aliases and numeric coercions, filtering/duplicate order, cached fallbacks,
position-slice identity, and balance's mode update before a later parsing failure
are covered by the oracle. Position-mode reads propagate the generic transport
error. Fallback callbacks run after the await rather than freezing the store at
method entry. Hooks are infallible Rust callbacks; omitted public GET config is
represented as an empty object. Cached market/ticker arrays and balance objects
are cloned. Shared string/model inputs and stock JSON exclude executable host
objects and inherited market-index values. Malformed market/ticker arrays use
explicit Rust diagnostic messages instead of engine-generated JavaScript text.

### Measured gates

All commands ran from the worktree root. Native-oracle commands used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit | Measured result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | Scoped crate and all its targets compile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test rest_reads -- --nocapture` | 0 | `actual_typescript_rest_reads`: 1 passed |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | Invention/status passed: 122 rows, 36 public Rust items |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols |

The actual extractor writer also ran before the code commit: exit 0,
updated 0 manifests, with no added or orphaned symbols.

### Native witness and discrimination

`tests/rest-reads-oracle.cjs` extracts the actual exchange arrow bodies and runs
the actual get-key module, parsePrecision function, shared enums and endpoint
table. The native Rust test stops after case index 32: **33 fixed cases**,
split **7 markets / 6 tickers / 10 balances / 10 position modes**. Independent
expectations assert **8 market outputs, 6 ticker outputs and 33 GET calls**.
Market/ticker fields and balance numbers are compared on decoded native values;
numeric assertions compare f64 bits with only NaN canonicalized, retaining signed
zero. Native assertions run before request-frame assertions. Four source outputs
explicitly retain a string amount limit while the shared Rust model holds its
numeric coercion; the source string and the numeric expectation are both observed.

One production mutation changed ticker volume's `baseVol` lookup to `quoteVol`.
Against the source bytes committed at the measured code SHA, the scoped test
compiled and exited **101** at **`REST native volume`**: native 19 versus source
17. The source was restored, the same scoped test exited **0**, and it passed
again at the recorded code SHA. This is one discriminating control, not a claim
that every newly moved method received an independent mutation. No other controls
were attempted; the single attempted control had no survivor.

Not run: workspace-wide compilation, host/wasm Clippy, full suites, live exchange
requests, sub-agent review, REST writes, remaining REST read methods, or websocket
execution. The corpus is a measured floor, not exhaustive input coverage.

## Utility slice already merged

Measured SHA: `147758f7211fd7054c7a7c512c8a583a108c6b1a`.
Integration parent: `2bc83d78d4f821c4b116d082d02c7b5a2595ffc5`.
The merged slice moved **0 pending → ported; 5 pending → divergent (CHOSEN)**,
changing Bitunix from **13/10/99** to **13/15/94**.

At that SHA, the same scoped cargo check exited 0; scoped
`--test response_helpers --test row_extraction` exited 0 with both tests passing;
the venue parity script exited 0 (122 rows, 32 public Rust items); and
`parity:extract:check` exited 0 with no manifest changes, added or orphaned symbols.
Swapping production `data`/`result` traversal order compiled and exited 101 at
`RESPONSE native candidate 14:1` (native 2 versus source 1); restoration passed.
Native floors were 63 response cases/207 candidate values, 88 first-value cases,
64 account cases and 64 position cases/52 position rows. No full workspace,
Clippy, live API or websocket checks were run for that slice.

This tracked sheet preserves the prior measurements previously recorded only
under ignored `target/`; build logs remain local and are not committed artifacts.

## Orphan helper recovery

Measured merge/code SHA: `eb1ebdf6663b4529bed663577d416d80214bbc44`.
Merge parents are killed-lane head `e1a2f388c8efcbe89d368f53b44920a25e8ad117`
and the requested integration head `355683b84a`. The subsequent sheet-only
commit changes no code or manifests; these exits belong to the merge SHA.

Compared with integration `355683b84a`, Python `tomllib` measured **0 pending →
ported; 7 pending → divergent (CHOSEN)**, changing **13/19/90** to **13/26/83**.
The original orphan head had eight off-pending rows: five response/row-selection
rows were already integrated, and three TP/SL rows were still pending. Contrary
to its branch name, that pushed SHA contained no order-link implementation.
Four order-link rows and their implementation/oracle/tests were restored from
this worktree's preserved killed-lane uncommitted snapshot. Nothing is counted
as newly moved merely because the five existing rows were revalidated.

Conflicts in the manifest, module exports and directory guidance were resolved
by preserving integration's utility/read work and adding TP/SL/order-link
exports and mappings. No other integration changes were selectively discarded.
There is deliberately an unmerged killed-lane ancestor in this recovery branch;
the new measurements establish its current scoped gates rather than trusting
its old narrative or unexecuted control matrices.

All commands below ran at the merge SHA with
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`
for actual-source tests.

| Gate | Exit | Result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | All Bitunix targets compile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 -- --nocapture` | 0 | All 11 Bitunix integration tests passed; zero library/doc tests |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | Real invention/status gate: 122 rows, 44 public Rust items |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols |

The actual extractor writer ran before the merge commit, exit 0, with no added,
orphaned or rewritten manifests. `tomllib` counted **39 off-pending rows and 44
nonempty named Rust references**; the actual status/invention gate resolved
those references to real Rust items. The scoped check confirms they compile.
No nonexistent target is being credited as implemented.

One production control per recovered family was compiled, failed its named
native assertion with exit **101**, restored, and passed the same test with
exit **0**. The restored source bytes are those committed at the merge SHA.

| Family/test target | Production mutation | Named assertion |
| --- | --- | --- |
| `response_helpers` | Swap `data` and `result` traversal edges | `RESPONSE native candidate 14:1` |
| `row_extraction` | Skip the first boolean false value | `FIRST native selected value` |
| `tpsl_helpers` | Drop one ID separator underscore | `TPSL native leg ID` |
| `order_links` | Classify zero quantity using `filled` instead of `amount` | `ORDER LINK native position` |

The controls stop after these four attempts: four failures caught, four restored
passes, no survivors. Older checked-in control scripts contain additional
controls; those larger matrices were not run or credited for this recovery.

Native corpus floors: response selection **63 cases / 207 values**; first-value
selection **88 cases**; account selection **64 cases**; position selection
**64 cases / 52 rows**; TP/SL **1156 ID, 69 trigger, 578 reduce-only comparisons**;
order links **247 comparisons per helper / 275 parent rows / 17 native signed-zero
amount assertions**. Source-observed UTF-8 refusals remain declared in the tests
and manifest. These are finite measured corpora, not exhaustive input coverage.

Not run: workspace-wide compilation, host/wasm Clippy, monorepo suites, live
exchange requests, writes/websocket execution or sub-agent review. This recovery
adds the killed lane's helper implementation; the separately preserved REST-write
slice is not included in this branch or in its measured row counts.

## REST writes

Measured code SHA: `17cb5b26ce2eec8d17013c481d18a2bd37c0e4fa`.
Integration parent: `5b5d9004c528addfc33e1ff47ad6961ac5dc94d8`.
The branch was created at requested head `8fd9e9443b`, then fast-forwarded to
integration after PR 7422 merged. No unmerged recovery ancestor is being carried
in this write slice. The following sheet-only commit does not change tested code.

Python `tomllib` over all nine Bitunix manifests, comparing that integration
parent to the code SHA, measured **4 pending → ported; 1 pending → divergent**.
Before: **13/26/83**. After: **17 ported / 27 divergent / 78 pending**.

| Source method | Rust target in `src/rest_writes.rs` | Status |
| --- | --- | --- |
| `changePositionMode` | `change_position_mode` | ported |
| `changeMarginType` | `change_margin_type` | ported |
| `cancelSymbolOrders` | `cancel_symbol_orders` | ported |
| `placeOrders` | `place_orders` | ported |
| `setLeverage` | `set_leverage` | divergent, CHOSEN |

The four ported operations own the source guards, iteration/flattening,
sequencing, catch boundaries and forwarding. Their caller-owned transport,
property/callee projections and store/error operations are fallible. Mode changes
consume only the contracts-number projections actually read by the source,
retaining all f64 Number values and examining every position before the guard.
Margin symbols use UTF-16 code units, preserving the complete source string domain
rather than rejecting a lone surrogate. The POST binding receives the unchanged
symbol units plus the exact marginMode/marginCoin values. Cancellation captures
filter length, skips missing slots and retains selected opaque order values.
Batch hydration uses live length; formatting captures its length once, skips
holes and flattens in order. Generic error/results/payloads are forwarded without
narrowing them to a JSON/model representation. Pending child cancellation,
hydration, formatting and batch implementations are not claimed by these wrappers.

Leverage's CHOSEN boundary is real: it consumes shared typed Market/Position and
UTF-8 symbol identities, excluding inherited non-Market index entries and
lone-surrogate symbols. Shared Math.min/Math.max preserve NaN and signed zero;
JsJsonValue retains native nonfinite request numbers. Initial lookup order,
missing-value errors, unchanged-leverage skip, both long/short updates, and the
POST/update/report catch behavior match the source within that declared domain.

### Measured write gates

Native-oracle commands used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit | Result at the measured code SHA |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | All Bitunix targets compile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test rest_writes -- --nocapture` | 0 | `actual_typescript_rest_writes`: 1 passed |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | Invention/status passed: 122 rows, 49 public Rust items |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols |

The extractor writer also ran before the code commit, exit 0, updating 0
manifests with no added/orphaned symbols. No full suite was needed for the write
slice after its focused oracle passed; the recovery's 11-test suite is recorded
separately above and is not presented as a write-code suite run.

The witness extracts the five actual TypeScript method bodies, actual shared
enums, endpoint table and createPositionId helper, with controlled host I/O/store
and child-method bindings. It stops at case index **43**, a floor of **44 cases**:
**8 mode / 9 margin / 11 leverage / 7 cancellation / 9 batching**. Independent
per-case native/source effect counts are pinned from those workflows. Native
assertions inspect f64 bits (only NaN canonicalized), full UTF-16 symbol units,
request values, selected order tokens, flattened payload ordering, error identity
markers/rejections and acknowledgement reference identity. No wire serialization
is used for these comparisons. Cases include a retained lone-surrogate margin
symbol, NaN/signed-zero leverage, logging failures, sparse/deleted slots, hydration
appends and formatting appends excluded by the captured length.

### One discriminating control per newly moved row

Each source mutation compiled, exited **101** at the named native assertion,
was restored, and the same focused test exited **0**. The restored source bytes
are those committed at the measured code SHA; its oracle passed there again.

| Row | Production mutation | Named assertion family |
| --- | --- | --- |
| `changePositionMode` | Emit ONE_WAY instead of HEDGE | `WRITE native effect "changePositionMode"` |
| `changeMarginType` | Emit CROSS instead of ISOLATION | `WRITE native effect "changeMarginType"` |
| `setLeverage` | Increase the maximum clamp bound by one | `WRITE native effect "setLeverage"` |
| `cancelSymbolOrders` | Invert symbol matching | `WRITE native effect "cancelSymbolOrders"` |
| `placeOrders` | Reverse the flattened request list | `WRITE native effect "placeOrders"` |

The enumeration stops after five controls: five caught failures and five
restored passes, no survivors. No larger control matrix was attempted.

Not run for this write slice: full Bitunix suite, workspace compilation,
host/wasm Clippy, monorepo suites, live POSTs, websocket execution or sub-agent
review. These are source/native orchestration witnesses, not a live-trading or
complete-venue claim. Remaining write/callee rows remain pending in the manifest.

## Websocket lifecycle slice — 2026-10-13

**Moved 14 rows pending → ported; zero new divergent rows.**
Code head: `b905bea8a0a7b55244a4145ac8da286b9c8d3191`.
Implementation commit: `00691c26cc48a8a3055ff757b71c3d048b172924`.
Merge-down: `cb12d89fcc51db853bb25e56d8dd9a516feb4530`, incorporating integration
`61c78429c39fc37b14d3fb5d479572ec74ab9c29` before push. Initial base was
`54851c81b7c780ebfea3682883bd3a49e12b9d8f`.

Python `tomllib` compared the exchange manifests against that integration parent:
**17 ported / 27 divergent / 78 pending → 31 ported / 27 divergent / 64 pending**.
The journal adapter added by integration is included in the target census;
it contributes no rows to this slice's exchange-only triple.

| Source row family | Rows | Real Rust targets |
| --- | ---: | --- |
| Public declarations | 2 | `websocket_public.rs::{BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION, INTERVAL}` |
| Public lifecycle | 4 | `websocket_public.rs::{ping, clear_subscription_queue, drain_subscription_queue, dispose}` |
| Private heartbeat | 1 | `websocket_private.rs::ping` |
| Private debounce | 3 | `websocket_private.rs::{throttled_fetch_balance, throttled_fetch_orders, throttled_fetch_positions}` |
| Private reconciliation | 3 | `websocket_private.rs::{fetch_throttled_balance, fetch_throttled_orders, fetch_throttled_positions}` |
| Futures heartbeat | 1 | `websocket_futures.rs::ping` |

All targets above are in `safe-cex-venue-bitunix/src/`. Native generic timer,
handler, queue, payload and loaded-state carriers preserve host identity.
Fallible host projections own property access, scheduling, I/O and store spread;
Rust owns source guards, ordering, frame fields, delays and loaded flags.
Constants are declaration-only ports. No socket/class/stream-consumer completion
is inferred from these fourteen rows.

### Measured gates

| Command | Exit | Evidence |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | Scoped compilation at merge-down SHA |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test websocket_lifecycle --test rest_reads -- --nocapture` | 0 | Both focused oracle tests passed at merge-down SHA |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | At code head: invention/status census passed, 125 rows, 66 public Rust items |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | At merge-down SHA: 7,341 symbols / 512 source files; zero updates, additions or orphans |
| Focused `websocket_lifecycle` test after post-merge mutation restoration | 0 | At code head: actual-source oracle passed again |

The final code-head commit changes only `tests/parity-check.py`: the original
exchange-only gate reported three unmapped journal items after merge-down.
The corrected gate links the existing exchange and Bitunix journal manifests
into its temporary scope, without exemptions or invented mappings. Production,
source TypeScript, websocket oracle and native witness files are byte-identical
between the implementation commit and code head, verified with `git diff`.

The source oracle executes twelve actual TypeScript members in **82 lifecycle
cases**, plus both public declaration initializers. Decoded-native assertions
run before JSON wire encoding. Independent method/effect totals supplement the
source comparisons. Cases cover disposed guards, negative zero/NaN heartbeat
values, setter and host failures, absent/zero/active timer handles, second getter
reads, fresh queue replacement with a retained old alias, final-batch cooldown,
empty subscription batches, and loaded-state changes/disposal during an await.
Native reconciliation assertions also retain fetched payload pointer identity.
Oracle runtime: Node `v26.7.0`, V8 `14.6.202.34-node.28`, arm64 macOS.

### Discriminating production controls

The checked-in reproducer is `tests/websocket-controls.py`. Each of the fourteen
websocket mutations compiled, failed its named decoded-native assertion with
exit **101**, restored original source bytes and passed with exit **0**. These
runs preceded the implementation commit; the unchanged production/witness/source
bytes bind them to the code head. No mutation survivors occurred.

| Target | Production mutation | Native assertion |
| --- | --- | --- |
| Subscription limit | 300 → 301 | `WS native subscription limit` |
| Interval declaration | 1min → 3min | `WS native interval` |
| Public ping | frame key ping → pong | `WS native effect "public.ping"` |
| Futures ping | frame key ping → pong | `WS native effect "futures.ping"` |
| Private ping | operation ping → pong | `WS native effect "private.ping"` |
| Queue clear | unset timer before clearing it | `WS native effect "public.clearSubscriptionQueue"` |
| Queue drain | 250ms → 251ms | `WS native effect "public.drainSubscriptionQueue"` |
| Public dispose | wrong markets event | `WS native effect "public.dispose"` |
| Balance debounce | wrong timer field | `WS native effect "private.throttledFetchBalance"` |
| Orders debounce | wrong timer field | `WS native effect "private.throttledFetchOrders"` |
| Positions debounce | wrong timer field | `WS native effect "private.throttledFetchPositions"` |
| Balance reconciliation | loaded.balance true → false | `WS native effect "private.fetchThrottledBalance"` |
| Orders reconciliation | loaded.orders true → false | `WS native effect "private.fetchThrottledOrders"` |
| Positions reconciliation | loaded.positions true → false | `WS native effect "private.fetchThrottledPositions"` |

After merge-down, at code head, public ping was independently mutated again:
its compiled frame-key mutation failed `WS native effect "public.ping"` with
exit **101**, and restoration passed with exit **0**. This confirms discrimination
in the merged dependency context.

The separate compatibility commit `32e1744d10` sets new shared `Ticker.timestamp`
to `None` for REST ticker construction, matching the source's absent field.
Its additional production control (`None` → `Some(0.0)`) failed
`REST native absent ticker timestamp` with exit **101** and restored to **0**.
The focused REST-read oracle also passed after integration merge-down.

Local gate logs remain ignored under `sdk-rs/target/bitunix-websocket-evidence/`;
this tracked sheet is the reviewer-facing record. Live sockets/timers, connection
assembly, login signing, dispatch, stream parsers and subscription listeners are
outside this slice; their pending rows remain explicit in the manifests.


## Remaining-row checkpoint — 2026-10-14

**Moved 16 rows off pending: 8 pending → ported; 8 pending → divergent (CHOSEN).**
Code/manifest head: `f817e5188fe5d03dadd4d9e3f4b58db5084eff55`. Initial base:
`17b6bcd0db760cfb175d1f5e21e581b5d29bd2a5`. Integration merge-down:
`08e57280d78b2d72950e770c01d170be6ebed9d9`, incorporating
`89ad07ac80f6efc169430a2f94bd59898660e9f9`.
Python `tomllib` compared both the initial base and integration parent with this
head: **31/27/64 → 39 ported / 35 divergent / 48 pending**. The three journal
adapter rows are included in the target census, outside this exchange triple.

| Moved family | Rows | Status | Native witness |
| --- | ---: | --- | --- |
| Trade income, closed position, historical order mappers | 3 | CHOSEN divergent | `history_mappers`: 528 actual-source cases; explicit native numeric union variants, numeric bits and enum values |
| Public listener rejection behavior | 2 | ported | `public_listener_errors`: 6 actual-source rejections, no argument reads or callback calls |
| Symbol Set, batching, subscribe, market/channel tracking, book subscription and market additions | 6 | ported | `public_subscriptions`: 49 cases plus 2 full UTF16 Set cases, decoded-native effects before encoding, ordered Set dedupe and topic state |
| Position reads and concurrent order aggregation | 2 | CHOSEN divergent | `position_reads`: 32 mapping cases plus 5 all-settled cases, native fields/effects and fetched order pointer identity |
| Position id, protection snapshot and hydration | 3 | CHOSEN divergent | `order_state`: 42 snapshot, 12 borrowed-id and 13 hydration cases; numeric bits, strict object-id identity, returned identifier/payload identity and error sequencing |

Typed shared UTF8/retained-JSON boundaries are explicit for the mapper, position
and protection rows. Position infos are owned snapshots; arbitrary JS host
objects, getters and prototype graphs are outside that model. Aggregation also
keeps both rejection values alongside the successful source concatenation
payload rather than silently dropping them; this deliberate error-outcome API
accounts for its CHOSEN status. No generic subscription carrier is narrowed to
UTF8: the ordered Set witness includes lone high/low surrogates and astral units.

### Gates and controls

| Gate | Exit | Measured result |
| --- | ---: | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | Scoped compilation at merge-down SHA |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test history_mappers --test public_subscriptions --test public_listener_errors --test position_reads --test order_state -- --nocapture` | 0 | All five touched test targets passed at merge-down SHA |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | At code/manifest head: 125 rows, 83 real public Rust items; no inventions or status violations |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | At merge-down: 7,341 symbols / 512 source files, zero updates/additions/orphans |
| Actual extractor writer after exact-target rename notes were corrected | 0 | Zero manifest updates, additions or orphans |

The final metadata commit only names the five workflows' exact extracted targets
in their rename notes. Production and native witnesses did not change after the
scoped compilation/tests. `git diff` confirmed no Bitunix source, witness or
manifest changes were introduced by the integration merge itself.

`tests/pending-controls.py` is the checked-in reproducer; optional positional
family names run only selected controls. Every control below compiled, failed
its named decoded-native assertion with exit **101**, restored source bytes,
and passed with exit **0**. The first seven ran before the subsequent position/
protection commits; their source and witnesses remained unchanged. The last five
ran at `60850c5eccdf3adeab01837f3e93f1060e33b8ce`. Merge-down left all these
production/witness bytes unchanged. No survivors occurred.

| Family | Production mutation | Named native assertion |
| --- | --- | --- |
| Income sign | -fee → fee | `HISTORY decoded native "trade"` |
| Closed notional | multiply → add | `HISTORY decoded native "closed"` |
| Historical type | MARKET → LIMIT | `HISTORY decoded native "order"` |
| Listener rejection | wrong error text | `PUBLIC listener native rejection` |
| Ordered Set | inverted dedupe guard | `SUB native full UTF16 set` |
| Batching | default 20 → 19 | `SUB native effect "enqueueSubscriptions"` |
| Market cap | 300 → 299 retained markets | `SUB native event count` |
| Position quantity | remove absolute value | `POSITION decoded native "positions"` |
| Order aggregation | drop successful TP/SL rows | `POSITION decoded native "orders"` |
| Snapshot trigger | accept zero price | `ORDER STATE decoded native "snapshot"` |
| Position id | inverted info precedence | `ORDER STATE native position id identity` |
| Hydration | skip zero-amount full-position refresh | `ORDER STATE native hydration effect count` |

Local logs remain ignored under `sdk-rs/target/bitunix-pending-evidence/`.
This is a measured checkpoint, with **48 pending rows still explicit**; it is
not a completed-venue or live-transport claim. Remaining factory, orchestration,
order routes and websocket connection/message rows continue separately.

## API and exchange lifecycle

**5 rows moved: 4 pending → ported; 1 pending → divergent (CHOSEN).**
Python `tomllib` over the nine Bitunix exchange manifests measures
**39 ported / 35 divergent / 48 pending → 43 ported / 36 divergent / 43 pending**,
with 122 rows before and after. This is a slice; 43 Bitunix rows remain pending.

Branch: `feat/rust-bitunix-api-lifecycle`.
Fresh base: `8cbc4aed134a44085fb6c20e261d2bf504d7adc6`.
Implementation SHA: `76854d797233dd3d19e4236704e6a70b607d5a72`.
Measured post-merge code SHA: `5b12589a581dc1b46d42f265d6b31e6b54153214`.
Integration merge parent: `9f59912c6f39a0df543e6228c38ad5a57258d2a2`.
The merge changed no Bitunix implementation, witness, source or manifest bytes.
The next commit adds only this hand-authored sheet; build output stays in ignored
`sdk-rs/target/bitunix-api-lifecycle-evidence/`.

| Source row | Real Rust target | Status |
| --- | --- | --- |
| `createAPI` | `api.rs::{create_api, sign_request, receive_response}` | divergent, CHOSEN |
| `BitunixExchange.dispose` | `exchange_lifecycle.rs::dispose` | ported |
| `BitunixExchange#configurePublicWebsocketShards` | `exchange_lifecycle.rs::configure_public_websocket_shards` | ported |
| `BitunixExchange.listenOrderBook` | `exchange_lifecycle.rs::listen_order_book` | ported |
| `BitunixExchange.fetchIncomeHistoryRange` | `exchange_lifecycle.rs::{fetch_income_history_range, IncomeHistoryRange}` | ported |

Rust owns disposal order, captured shard length/live index visits, symbol
projection before chunking, the source 300-symbol cap, shared mutable symbol
arrays, constructor-before-push ordering and returned socket identity. Opaque
listener and income delegates retain input/result/error identities. The range
wrapper owns all three option fields, preserves absent versus null income type,
and awaits a deferred result. Fallible host projections own property/member
access and constructors; symbols can use full UTF-16 carriers without a venue
string conversion.

The API owns client configuration, retry/transport/interceptor registration,
query stripping and double SHA-256 signing, strict suppression flags, header
short-circuit order, and message/rate classification. Actual TypeScript factory,
SHA, get-key and HTTP-failure modules execute in the witness, alongside the real
`qs` and lodash dependencies. Native raw header entries are observed before the
host AxiosHeaders constructor or any request encoding. The API's CHOSEN boundary
is retained JSON/shared UTF-8 plus typed flags and String errors; executable
objects, lone-surrogate strings and JavaScript Error instances are outside that
carrier. Host projections own query encoding/decoding, clock/nonce, AxiosHeaders
construction/config shallow copy and HTTP-failure reporting. Non-object request
headers produce an explicit typed transport error. Suppressed and ordinary
responses remain their original borrowed values.

### Scoped gates at the measured code SHA

Native commands used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit | Measured result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | Bitunix and all its targets compile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test api --test exchange_lifecycle -- --nocapture` | 0 | 2 tests; 42 API cases and 30 lifecycle cases |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | Actual invention/status indexer: 125 rows, 91 public Rust items, including the three journal rows |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added, 0 orphaned symbols |

The actual extractor writer ran before committing: exit 0, updated 0 manifests,
0 added and 0 orphaned symbols. All off-pending rows have indexed Rust targets;
no exemption was added. `tomllib` compares each source row with the fresh base,
counts exactly five pending-to-off-pending changes, and asserts the measured
43/36/43 triple.

The native witness compares property order, f64 bits with only NaN canonicalized,
UTF-16 units, effects and opaque identities before transport serialization.
Independent floors are **8 factory / 17 request / 17 response** and
**5 disposal / 15 shard / 4 listener / 6 income-range** cases. Lifecycle cases
include sparse arrays, deletion/appending during disposal, symbol-array mutation
between primary/secondary calls, absent/null income types, signed zero, infinity,
NaN, opaque thrown objects and a genuinely pending first Rust future poll.

### Discriminating production controls

`/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/api-lifecycle-controls.py`
exited **0**. The finalized implementation/witness bytes used by these controls
are identical at the implementation and measured post-merge code SHAs.

| Family | Production mutation | Native failure | Restored |
| --- | --- | --- | --- |
| API factory | timeout 20,000 → 19,000 | compiled exit 101, `API native effect create` | exit 0 |
| API signing | digest/secret concatenation reversed | compiled exit 101, `API native effect headers` | exit 0 |
| API response | suppression guard inverted | compiled exit 101, `API native event count` | exit 0 |
| Disposal | public disposal before base | compiled exit 101, `LIFECYCLE native effect "dispose"` | exit 0 |
| Shards | cap 300 → 299 | compiled exit 101, `LIFECYCLE native effect "configurePublicWebsocketShards"` | exit 0 |
| Listener | delegate invoked twice | compiled exit 101, `LIFECYCLE native effect "listenOrderBook"` | exit 0 |
| Income range | start/end arguments swapped | compiled exit 101, `LIFECYCLE native effect "fetchIncomeHistoryRange"` | exit 0 |

The listener mutant adds Clone/FnMut bounds solely to make the erroneous second
invocation compile; restored production retains its original FnOnce interface.
An earlier range mutant was rejected for an unused argument at compilation and
is not counted as native discrimination. Its replacement above uses both
arguments, compiles, fails the native assertion and restores to pass.

The corpus is a measured floor. Workspace/full-crate suites, wasm execution,
live exchange networking and the remaining pending methods were not gated by
this slice.

## Websocket events and login signing

**8 rows moved: 7 pending → ported; 1 pending → divergent (CHOSEN).**
Python `tomllib` over the nine Bitunix exchange manifests measures
**43 ported / 36 divergent / 43 pending → 50 ported / 37 divergent / 35 pending**,
with 122 rows before and after. This slice leaves 35 rows pending.

Branch: `feat/rust-bitunix-websocket-events`.
Fresh base: `3ec6f40c8b6159f425a3294c4ba8506fd274d726`.
Implementation SHA: `f100c38099142c49af7cd9a6b9381b8f61630e35`.
Measured post-merge code SHA: `3eb4072630272ede7752914d0cedb94314a7cb74`.
Integration merge parent: `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444`.
The merge changed no Bitunix source, implementation, witness or manifest bytes,
nor the shared numeric/types crates or TS get-key/jsonParse dependencies.
The subsequent evidence commit changes only this hand-authored sheet. Generated
control logs stay in ignored `sdk-rs/target/bitunix-websocket-event-evidence/`.

| Source member | Real Rust target | Status |
| --- | --- | --- |
| `BitunixPublicWebsocket.onOpen` | `websocket_public.rs::{on_open, ping_loop}` | ported |
| `BitunixPublicWebsocket.onMessage` | `websocket_public.rs::on_message` | ported |
| `BitunixPublicWebsocket.handleTickerStreamEvent` | `websocket_public.rs::handle_ticker_stream_event` | ported |
| `BitunixPublicWebsocket.handlePriceStreamEvent` | `websocket_public.rs::handle_price_stream_event` | ported |
| `BitunixPublicWebsocket.handleDepthBookStreamEvent` | `websocket_public.rs::handle_depth_book_stream_event` | ported |
| `BitunixFuturesWebsocket.onOpen` | `websocket_futures.rs::{on_open, ping_loop}` | ported |
| `BitunixFuturesWebsocket.onMessage` | `websocket_futures.rs::on_message` | ported |
| `BitunixPrivateWebsocket.sendLogin` | `websocket_private.rs::send_login` | divergent, CHOSEN |

Rust owns open ordering, independent secondary checks, subscription frames and
positive-count topic sends, timer reads/clear/assignment, the recursive ping
continuation and its 15,000ms interval. Public dispatch owns count increments,
channel selection, stats order and its source try/catch boundary. It iterates
live kline indices and supplies a fresh one-element array per callback retaining
the same parsed object. Futures dispatch owns blob await/error logging, the
post-blob disposed guard, original-blob fallback and captured forEach length with
live hole skipping. Generic fallible projections supply host members, scheduler,
JSON parsing, blob inflation and opaque values/errors; no venue interpreter or
concrete UTF-8 channel carrier is imposed on those event methods.

The three stream handlers own shared `parse_float`, binary64 patch values,
percentage intermediates and depth `|| 0`. Field access/ToString projections
supply full UTF-16 units. Surrogate replacement inside numeric parsing cannot
extend an ASCII decimal token or JavaScript whitespace; the original host field
is retained. Actual source assertions cover leading/trailing lone surrogates,
truncated exponents, BOM/line-separator whitespace, an astral character, signed
zero, infinity and NaN. Rust receives coercion strings, not oracle numeric outputs.

Login owns timestamp/nonce/key capture before synchronization, trimming,
ordered sync and 25ms sleep awaits, post-sync signer lookup, local double SHA-256,
remote request construction, diagnostics and the login frame. Actual source SHA
runs in the witness. The CHOSEN boundary is shared UTF-8 strings, retained JSON
signer replies and String errors instead of executable host objects,
lone-surrogate keys or JavaScript Error instances. Malformed null replies produce
an explicit error. Remote signer identity is retained; remote failures and
missing signatures report before returning without a send.

### Scoped gates at the measured code SHA

Native commands used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit | Measured result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | Bitunix and all its targets compile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test websocket_events --test websocket_login -- --nocapture` | 0 | 2 tests; 124 event cases and 25 login cases |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | Actual invention/status indexer: 125 rows and 101 public Rust items, including three journal rows |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Would update 0 manifests; 0 added and 0 orphaned symbols |

The actual extractor writer ran before committing: exit 0, updated 0 manifests,
0 added and 0 orphaned symbols. No exemption was added. The `tomllib` comparison
with the fresh base asserts exactly eight pending-to-off-pending changes and the
50/37/35 triple. All off-pending rows have indexed real Rust targets, including
both inline ping-loop extractions.

The event oracle executes seven actual TS arrows, actual get-key/jsonParse
modules and real pako inflation. Independent case floors are
**13 public opens / 13 futures opens / 23 public messages / 27 futures messages /
16 ticker / 16 price / 16 depth**. Login executes its actual TS arrow and SHA
helper with **25 cases**, including pre-sync capture, signer lookup after sync,
remote errors, missing/non-string signatures and null replies. Native assertions
compare binary64 bits with only NaN canonicalized, full UTF-16 units, field/frame
entry order, state and opaque reference/array identity before transport encoding.
The blob witness genuinely yields Pending on the first Rust poll.

### Discriminating production controls

`/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/websocket-event-controls.py`
exited **0** against the finalized implementation and witness bytes. Every listed
mutation compiled, failed a native assertion with exit **101**, then passed after
restoration with exit **0**. There are eight controls for eight moved source rows.

| Family | Production mutation | Native assertion |
| --- | --- | --- |
| Public open | ticker subscription → price | `EVENT native effect public.onOpen:markets` |
| Futures open | ping interval 15,000 → 14,000 | `EVENT native effect futures.onOpen:schedule` |
| Public message | message increment +1 → +2 | `EVENT native effect public.onMessage:setCount` |
| Futures message | omit last captured handler index | `EVENT native effect count futures.onMessage` |
| Ticker | percentage scale 100 → 10 | `EVENT native effect public.handleTickerStreamEvent:update` |
| Price | funding-rate field fr → ft | `EVENT native effect public.handlePriceStreamEvent:field` |
| Depth | preserve bid negative zero | `EVENT native effect public.handleDepthBookStreamEvent:update` |
| Login | sleep 25 → 24ms | `LOGIN native effect sleep` |

The corpus is a measured floor, using controlled host member/coercion/parse/
inflate/scheduler projections. Class/runtime composition and other pending
members remain outside this slice. Workspace/full-crate suites, wasm execution
and live exchange networking were not run.

## History pagination and remaining REST readers

Measured implementation: `b9f6a7fc653c1e12fd0604092ae1f8acfdf7a47e`.
Fresh base: `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a`.
Post-merge code: `9357defd04fd1f954e902e82bce218fb0f4e7a9d`, including
integration through `5eabdd1396505c2cb959c7870441d20492738663` and the separately
pushed compile fix `1f097f6dba8bd004b738bda79d645f952ebe646a`.

`tomllib` measures **50 ported / 37 divergent / 35 pending** at the fresh base
and **56 ported / 40 divergent / 26 pending** after merging. Exactly nine rows
move off pending: **six ported and three CHOSEN divergent**. The venue still has
26 pending exchange rows. No exemption was added.

| Source member | Real Rust target | Status |
| --- | --- | --- |
| `fetchBitunixHistoryPages` | `history_reads.rs::fetch_bitunix_history_pages` | ported |
| `fetchClosedPositionRows` | `history_reads.rs::fetch_closed_position_rows` | ported |
| `fetchIncomeHistory` | `history_reads.rs::fetch_income_history` | ported |
| `fetchClosedPnl` | `history_reads.rs::fetch_closed_pnl` | ported |
| `fetchHistoricalPositions` | `history_reads.rs::fetch_historical_positions` | ported |
| `fetchHistoricalOrders` | `history_reads.rs::fetch_historical_orders` | ported |
| `fetchOHLCV` | `history_rest.rs::fetch_ohlcv` | divergent, CHOSEN |
| `updateSymbolMetadata` | `history_rest.rs::update_symbol_metadata` | divergent, CHOSEN |
| `validateAccount` | `history_rest.rs::validate_account` | divergent, CHOSEN |

The six history methods own pagination, request options, row selection,
time-window filtering, full UTF-16 ID deduplication, caps, warnings, source
callback selection, stable sorting and limit slicing. Generic host projections
supply opaque responses/rows/errors, account members and imported mappers.
The shared pager returns collected rows after reporting a failure; historical
orders discard them. Orders advance the inclusive oldest cursor before mapping,
including raw IDs whose mapper returns null, and retain page overlap. Position
mapping resolves settlement currency for every row and retains the source's
omitted `realizedIsNetOfFunding` option. Canonical mapper timestamp fields are
plain Numbers excluding NaN. Inline selector callbacks are real indexed Rust
targets, including `HistoryRowSelectors`, `history_row_time` and `history_row_id`.

The three CHOSEN readers retain shared UTF-8 strings, JSON payloads, canonical
models and host projections instead of executable object graphs and arbitrary
JavaScript errors. OHLCV owns interval splitting, the source limit/default/cap,
date-subtraction arguments, timeout and numeric candle fields; null rows have an
explicit native diagnostic. Metadata owns strict `ISOLATION` selection and
settings-before-position-update ordering, preserving unchanged cached position
identity. Validation owns strict numeric code zero and account/error/balance
ordering. It additionally retains a rejected balance fetch in `balance_error`
instead of silently discarding that error as the source does; the source-visible
error/balance fields are separately compared. Each manifest note names its
boundary and actual-source witness.

### Scoped gates and source witnesses

Native gates use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Cargo is `/opt/homebrew/opt/rustup/bin/cargo`.

| Command | Exit | Measured result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | Passed before committing and after the compile-fix merge |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test history_reads --test history_rest -- --nocapture` | 0 | Seven tests, 118 actual-source cases |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | Actual indexer: 125 rows, 116 public Rust items; every off-pending target resolves |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` | 0 | Actual writer: updated 0 manifests, added 0 symbols, orphaned 0 symbols |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Freshness: would update 0 manifests, added 0 symbols, orphaned 0 symbols |

The history oracle executes six actual source arrows and real income, journal
and order mappers. Case floors are **22 pager / 11 closed rows / 11 income /
11 closed PnL / 11 historical positions / 17 historical orders**. The REST oracle
executes three actual source arrows and real dayjs with a controlled clock:
**15 OHLCV / 10 metadata / 10 validation** cases. Native assertions compare
numeric bits with only NaN canonicalized, complete UTF-16 units, host access and
effect order, selector callbacks, record order and opaque reference identity
before transport encoding. Fixtures cover lone surrogates, nullish values,
duplicate pages, caps, mapping omissions, failures and signed zero.

### Discriminating production controls

`/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/history-read-controls.py`
exited **0** on the finalized source and witness bytes. All nine controls compiled,
failed the named decoded-native assertion with exit **101**, and passed after
restoration with exit **0**. Logs and generated JSON remain ignored in `target/`;
this manually maintained SHA-bound sheet is tracked.

| Family | Production mutation | Native assertion |
| --- | --- | --- |
| Pager | skip multiplier 100 → 99 | `HREAD native effect get` |
| Closed rows | reverse mtime/ctime precedence | `HREAD native effect selectorTime` |
| Income | trades endpoint → positions | `HREAD native effect delegate` |
| Closed PnL | drop mapped records | `HREAD native result` |
| Historical positions | settlement fallback USDT → USD | `HREAD native effect resolved` |
| Historical orders | oldest cursor → oldest minus one | `HREAD native effect get` |
| OHLCV | cap 200 → 199 | `RREST native effect subtract` |
| Metadata | ISOLATION → lowercase isolation | `RREST native effect set` |
| Validation | invert numeric code-zero comparison | `RREST native result` |

`git diff --exit-code b9f6a7fc653c1e12fd0604092ae1f8acfdf7a47e
9357defd04fd1f954e902e82bce218fb0f4e7a9d --` over Bitunix source/tests/manifest,
its source TypeScript and journal adapter, shared numeric and shared type crates
exited **0**. The native and mutation gate exits therefore stand after merging;
the scoped check, indexer, extractor freshness and count were rerun.
The last integration merge adds only Binance files, preserving these gates.

The separately authorized urgent workspace check ran on the compile-fix branch,
including integration `200ef7e5dd`: **exit 0, zero errors, zero warnings**. This
Bitunix slice did not run a workspace/full-crate test suite, wasm execution,
live exchange networking, or tests for other pending methods.

## Exchange and websocket session control

Measured implementation: `b60c123b05c6cb1ad9d197539755c91410e2efb0`.
Fresh integration base: `e413ab5016a4451c935648a910a4a005b0414134`.
Post-merge code: `a27b68c11ef6b6b2dd3f80ab4b6e3a16fab04b3e`;
actual integration parent: `e20df553203cb460fb7b4f46cf8dd65d9c589c3a`.
All ancestors come from the supplied base or merged integration.

`tomllib` measures **56 ported / 40 divergent / 26 pending** at the base and
**63 ported / 41 divergent / 18 pending** after merging. Exactly **eight** rows
move off pending: **seven ported, one CHOSEN divergent**. No exemption was added.
The class/constructor composition and 18 remaining venue rows stay pending.

| Source member | Real Rust control target | Status |
| --- | --- | --- |
| `BitunixExchange.start` | `session_control.rs::start` | ported |
| `BitunixExchange.tick` | `session_control.rs::tick` | ported |
| `BitunixExchange.listenOHLCV` | `session_control.rs::listen_ohlcv` | ported |
| `BitunixPublicWebsocket.connectAndSubscribe` | `session_control.rs::connect_public` | ported |
| `BitunixFuturesWebsocket.connectAndSubscribe` | `session_control.rs::connect_futures` | ported |
| `BitunixPrivateWebsocket.connectAndSubscribe` | `session_control.rs::connect_private` | ported |
| `BitunixPrivateWebsocket.onOpen` | `session_control.rs::{private_on_open, private_after_login}` | ported |
| `BitunixFuturesWebsocket.listenOHLCV` | `ohlcv_subscription.rs::{listen_ohlcv, OhlcvSubscription}` | divergent, CHOSEN |

Rust owns startup sequencing, post-await disposal checks, loaded patches, original
market-array retention, connection/shard ordering, position-mode refresh, tick
catch/report and count/interval scheduling. Opaque host values and imported
fetch/store operations are explicit projections. Actual witness updates assert
original record and loaded-object identity before observing their fields.
The exchange OHLCV wrapper retains live symbol reads, synchronous metadata-error
suppression, unawaited launch and opaque callback/unlistener identity.

The three connections own attempts, fallback selection and state resets, route
keys, repeated live fallback reads, diagnostic suppression, socket assignment and
listener order. Public connections additionally clear the subscription queue.
Real extracted `connection_close` and `connection_error` continuations retain
current socket/timer access at invocation, closed-state checks and source close
exception suppression. The private method retains its async rejection shape.
Private open launches the original fallible login Future/result detached; the host
scheduler receives a real Rust continuation. Login rejection is observed as a
host task rejection, including a synchronous login failure inside the source IIFE.
The continuation owns its disposal guard, ping, 15,000ms schedule and assignment.
The native login Future genuinely returns Pending before explicit resume.

The CHOSEN listener retains shared UTF-8 strings, JSON payloads, typed numeric
counts, String errors and canonical Candle. It owns the existing public INTERVAL
lookup, channel/frame, parser identity, append/filter, count updates, retry,
cancellation, cleanup, ticker update and parse_float fields. Captured channels
retain their initial symbol; parsing reads the live options. The source's pending
retry can resubscribe after cancellation, and Rust preserves that behavior.
Raw source string timestamps are explicitly converted through shared js_number
for canonical Candle.timestamp. The oracle observes both source string timestamps
and their numeric/NaN projections; native timestamp assertions are separate from
other native candle fields. Three malformed input cases have explicit Rust
messages while the source throws property-access errors. Arbitrary accessors,
prototype-polluted interval/count tables and lone-surrogate symbols are outside
this typed boundary. Connection URLs remain opaque full UTF-16 values.

### Scoped gates at the measured code SHA

Cargo is `/opt/homebrew/opt/rustup/bin/cargo`. Native commands use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit | Measured result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | Passed before committing and after merging |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test session_control --test ohlcv_subscription -- --nocapture` | 0 | Eight tests, 143 actual-source cases |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | Actual invention/status indexer: 125 rows, 210 public Rust items; every target resolves |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` | 0 | Actual writer: updated 0 manifests, added 0, orphaned 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Freshness: would update 0 manifests, added 0, orphaned 0 |

The lifecycle oracle executes seven actual TypeScript arrows, with independently
fixed case floors: **12 start / 16 tick / 6 delegation / 20 public connections /
20 futures connections / 20 private connections / 12 private opens**. Connection
cases include changed fallback getters between URL selection and diagnostics,
full UTF-16 lone-surrogate URLs, missing/closed sockets and throwing callbacks.
The candle oracle executes the actual futures listenOHLCV arrow and extracts the
actual public INTERVAL initializer: **37 cases**, including all 12 intervals,
multiple listener identities, Unicode symbols, retry/cancellation, callback errors,
NaN/infinity/signed zero and observed timestamp/malformed-input differences.

Effect, field, numeric-bit, opaque identity, callback and state assertions consume
decoded native values. Native candle fields and timestamp are asserted before callback observations;
native channel/frame objects are checked before frame-string observations. Only NaN is canonicalized. The enumeration
stops at these stated floors; it does not claim class or runtime completeness.

### Discriminating production controls

`/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/session-control-controls.py`
exited **0**. Every listed production mutation compiled, failed a named native
assertion with exit **101**, then passed after restoration with exit **0**.
Generated logs/JSON stay ignored in `target/`; this manual SHA-bound sheet is tracked.

| Family | Production mutation | Native assertion |
| --- | --- | --- |
| Startup | futures socket → private socket | `SESSION native effect` (connect argument) |
| Tick | refresh period 32 → 31 | `SESSION native effect` (mode/fetch ordering) |
| Delegation | invert configured-symbol guard | `SESSION native effect` (symbol/metadata ordering) |
| Public connection | public kind → futures | `SESSION native effect` (queue ordering) |
| Futures connection | futures kind → public | `SESSION native effect` (queue ordering) |
| Private connection | private kind → futures | `SESSION native effect` (routing selection) |
| Private open | ping delay 15,000 → 14,999ms | `SESSION native effect` (schedule argument) |
| Candle listener | parse volume from close | `CANDLE native fields` |

Post-merge `git diff --exit-code b60c123b05c6cb1ad9d197539755c91410e2efb0
 a27b68c11ef6b6b2dd3f80ab4b6e3a16fab04b3e --` over Bitunix `src`, `tests`,
manifest, source TypeScript, shared numeric and shared type crates exited **0**.
Native and mutation exits therefore stand after merging. The lockfile changes only
Bingx's dependency on safe-cex-store; Bitunix dependency versions remain unchanged.
The scoped check, indexer, freshness and tomllib count were rerun after merging.

Not run: full-crate/workspace test suites, workspace compilation, host/wasm clippy,
wasm execution, live exchange networking, or other pending method tests. No shared
type or trait was changed; all added traits are local to the Bitunix venue crate.

## Live order reads, placement and amendment

Measured implementation: `7c34ae7c314ef99ddd4b0ff7a98ba843a909867a`.
Fresh base: `b588da1cd0ab56a565af6cc3c55e7afeb1dbc43b`.
Post-merge code: `139eacec700ae67d58aac8663a755e1d2fe999d1`;
actual integration parent: `671cdadf56e6ec1c1955795f0976a8cfd0d28d09`.
Ancestors come from the supplied base or merged integration.

`tomllib` measures **63 ported / 41 divergent / 18 pending** at the base and
**65 ported / 47 divergent / 10 pending** after merging. Exactly **eight** rows
move off pending: **two ported, six CHOSEN divergent**. No exemption was added.
Cancellation, historical fills, remaining websocket/class composition and ten
remaining venue rows stay pending.

| Source member | Real Rust control target | Status |
| --- | --- | --- |
| `fetchRegularOrders` | `order_models.rs::fetch_regular_orders` | divergent, CHOSEN |
| `fetchTpslOrders` | `order_models.rs::fetch_tpsl_orders` | divergent, CHOSEN |
| `formatCreateOrder` | `order_models.rs::format_create_order` | divergent, CHOSEN |
| `formatTPSLOrder` | `order_models.rs::format_tpsl_order` | divergent, CHOSEN |
| `updateStandaloneTpslOrder` | `order_models.rs::update_standalone_tpsl_order` | divergent, CHOSEN |
| `updateOrder` | `order_models.rs::update_order` | divergent, CHOSEN |
| `placeOrder` | `order_placement.rs::place_order` | ported |
| `placeOrderBatch` | `order_placement.rs::place_order_batch` | ported |

Placement retains opaque options, payloads, positions, results, message objects
and errors. Rust owns caller resolution outside try, hydration/format/batch,
short-circuit retry selection, UTF-16 warning/key construction, long then short
cache deletion, positions refresh/update, exactly one retry and original-error
rethrow. Native assertions verify input/result/error identities and lone-surrogate
symbols before transport observations.

Batch control retains opaque values and errors, live input/property/iterator
projections, source object identity and full UTF-16 keys/IDs. Rust owns position,
TPSL and regular classification, omit fields, grouped integer-key enumeration,
five-leg chunks, per-symbol 1000ms inter-chunk sleeps, strict versus truthy code
guards, raw id/client/UUID priority, modern/legacy response lists, partial IDs,
rejection emits and source branch-specific rethrow/continue behavior. Its sleep
Future genuinely yields Pending twice in the eleven-leg case.

The six model methods use retained JSON and shared UTF-8 strings rather than
executable/prototype graphs, own undefined properties, lone-surrogate wire strings
and mutable aliased records. Each row names its boundary and reason. Read outputs
use JsJsonValue so native undefined/nonfinite numbers and raw id/lastUpdate types
remain observable. Info dictionaries are owned copies; the source witness also
observes the parent/virtual-child shared-info identity. TPSL read matching retains
primitive position-id keys and last-position-wins ordering. Formatter priority
remains info-before-cache for reduce-only creation and cache-before-info for
standalone TPSL. Full-position preservation, lot/remainder splitting, caller ID
assertion and cache effects remain in source order. Amendments own exact identity,
type/reduce-only reconciliation, trigger/qty/LIMIT execution preflight, native
modify routing, Number(code), emits and replacement-parent/virtual-child IDs.
Owned target snapshots exclude source mutations during transport awaits.

Numeric coercion, parse_float, precision/adjustment and protocol enum ownership
remain in shared/previously mapped modules. Remaining subtraction composes shared
Number, after_decimal, extrema/power and math_round, retaining string precision.
Malformed model rows have explicit errors; coercion errors propagate through the
host error type. Opaque error factories and error-reason projections retain source
report/rethrow ordering. No shared type or trait was changed.

### Scoped gates at the measured code SHA

Cargo is `/opt/homebrew/opt/rustup/bin/cargo`. Native commands use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Exit | Measured result |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --all-targets --jobs 2` | 0 | Passed before committing and after merging |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitunix --jobs 2 --test order_models --test order_placement -- --nocapture` | 0 | Eight tests, 162 source cases: 156 native comparisons and six observed UTF-16 input refusals |
| `/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/parity-check.py` | 0 | Actual invention/status indexer: 125 rows, 298 public Rust items; every target resolves |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` | 0 | Actual writer: updated 0 manifests, added 0, orphaned 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | Freshness: would update 0 manifests, added 0, orphaned 0 |

The oracle executes eight actual TS arrows and the actual utility arrows they
call, real safe-math, get-key, inverseObj, omitUndefined, enums and lodash. Clock,
UUID, transports, caller validation and store/cache effects are controlled host
projections. Independent floors are **22 create / 19 TPSL format / 17 regular
reads / 15 TPSL reads / 17 standalone amend / 26 regular amend / 16 placement /
30 batch**. Each model family includes one source-successful lone-surrogate input
that serde JSON refuses before the native method; these six cases are separately
counted, never claimed as native comparisons. API read fixtures use JSON ingress
before executing the source, retaining the source carrier's absence semantics.

Assertions consume decoded native fields/effects/results and numeric bits with
only NaN canonicalized; no decode/re-encode comparison supplies the native gate.
Enumeration stops at the stated floors. Class/runtime composition and the other
pending methods remain outside this slice.

### Discriminating production controls

`/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-bitunix/tests/order-operation-controls.py`
exited **0** on finalized source/witness bytes. Every mutation compiled, failed the
named native assertion with exit **101**, and passed after restoration with exit
**0**. The retry mutation fails the native result assertion; the script was
corrected to name that measured assertion. No mutation survived. Generated logs
and JSON stay ignored in `target/`; this manually maintained SHA-bound sheet is tracked.

| Family | Production mutation | Native assertion |
| --- | --- | --- |
| Regular reads | reverse remaining operands | `MODEL native result` |
| TPSL reads | lastUpdate clock +1 | `MODEL native result` |
| Create | lot count ceil → floor | `MODEL native effect` (assertSingle count) |
| TPSL format | full-position flag true → false | `MODEL native result` |
| Standalone amend | modify TPSL endpoint → place TPSL | `MODEL native effect` (post endpoint) |
| Regular amend | modify order endpoint → place order | `MODEL native effect` (post endpoint) |
| Placement | disable Missing positionId retry | `PLACE native result` |
| Batch | chunk size five → four | `BATCH native effect` (post payload) |

Post-merge `git diff --exit-code 7c34ae7c314ef99ddd4b0ff7a98ba843a909867a
139eacec700ae67d58aac8663a755e1d2fe999d1 --` over Bitunix `src`, `tests`, manifest,
source TypeScript, shared numeric and shared type crates exited **0**. Native and
mutation exits stand after merging. The lockfile adds only OKX dependencies;
Bitunix dependency versions remain unchanged. The scoped check, indexer, extractor
freshness and tomllib count were rerun after merging.

Not run: workspace compilation, full-crate/workspace test suites, host/wasm clippy,
wasm execution, live exchange networking or other pending method tests.

## Venue state, private messages, fills, cancellation and futures books

Implementation SHA: `d879bbb561ae4a29b05b07a32b856ddf663596a8`.
Base: `2a8f733506ff1bb167076694e18a3032e96088a1`.
Merge-down SHA: `d6d3e11422ad7899bd6ac093160562db27721c45`.
Integration parent: `ad731bb9ef4c5c6c8255645a3b412e933f8acda2`.
The following evidence commit changes only this tracked sheet.

Tomllib measures **10 rows moved: 3 pending→ported, 7 pending→divergent**.
Before: **65 ported / 47 divergent / 10 pending**; after:
**68 ported / 54 divergent / 0 pending**, 122 exchange rows.
The constructors and cancellation control are ported. Four class state rows,
private messages, historical fills and futures books have individual CHOSEN
notes naming their host/state/JSON boundaries. Class state extraction uses
explicit host/module operations instead of JavaScript inheritance and bound
arrows; it does not claim an assembled network runtime. All source-declared
venue fields are represented, including the client ID rule.

`tests/event-composition-oracle.cjs` extracts the actual four method arrows,
class field initializers and constructors. It executes the actual enum tables,
get-key, decimal subtraction, omitUndefined, groupBy, JSON parser, book totals,
client-rule factory and exchange helper arrows. Only platform/base/factory,
store, clock and transport effects are controlled. The native test compares
83 source cases and records three separate source-accepted lone-surrogate
JSON refusals. Object fields, presence, UTF-16 units and binary64 values are
compared before wire encoding; object enumeration order is not asserted.
Native assertions also cover opaque cancellation error identity, mutable parent
and input identities across an actual Pending poll, stable book identity and
cleanup, and a genuinely suspended detached login retry. Constructor tests
cover UTF-16 symbols, caches/timers/counters/defaults, factory ordering and
original parent/options/store identity, plus nine client-rule inputs.

| Gate | Result |
| --- | --- |
| Scoped Bitunix all-targets check, jobs=2 | 0, before and after merge |
| `cargo test ... --test event_composition -- --nocapture --test-threads=1` | 0; 5 native family tests, 86 cases including 3 refusals |
| Actual extractor writer (`parity:extract`) | 0; 7341 symbols, 512 files, 0 manifests changed |
| Post-merge extractor freshness (`parity:extract:check`) | 0 |
| `tests/parity-check.py`, actual indexer without exemptions | 0; 125 rows, 408 public Rust items |
| Tomllib transition/target-file declarations | 0; all 10 targets populated; 0 venue pending |
| `git diff --check` | 0 |
| Source/test/dependency byte preservation against implementation SHA | 0 |

`tests/event-composition-controls.py` establishes **13 compiled controls**,
all named native assertion failures with exit **101**, followed by restored
passes with exit **0**:

| Control | Named native assertion |
| --- | --- |
| Public max attempts 2→3 | COMPOSITION native state |
| Ignore supplied secondary input | COMPOSITION native state |
| Private handled pong false→true | COMPOSITION native state |
| Futures max attempts 2→3 | COMPOSITION native state |
| Exchange fake-full default false→true | COMPOSITION native state |
| Constructor rate limit 5→6 | EVENT native effect rateLimit |
| Keep first duplicate fill instead of replacement | HISTORY native result |
| Pong boundary >5000→>=5000 | EVENT native effect pong |
| Login retry delay 2000→2001 | EVENT native effect sleep |
| Standalone cancellation uses regular endpoint | EVENT native effect post |
| Suppress parent recreation | EVENT native complete effects |
| Book cumulative total +1 | EVENT native effect book |
| Omit wait-timer clear | EVENT native complete effects |

Draft controls rejected by the compiler were corrected to compile; those
compiler failures are not counted as discrimination. The timer control's
initial assertion selector was corrected to the observed named complete-effect
assertion, then its compiled failure and restored pass were rerun. No surviving
control was omitted. Generated logs/JSON stay under ignored `sdk-rs/target/`;
this narrative and reproducible source/tests are tracked.

Merge-down left Bitunix `src`, `tests`, crate dependencies, venue manifests,
TypeScript sources and shared numeric/type sources byte-identical to the gated
implementation SHA. Native/control exits therefore stand. No shared types or
traits changed. Not run: full crate/workspace suites, workspace-wide check,
live exchange/network credentials, browser automation, or PR creation/merge.

A final pre-push merge advanced to `a451b3a6c2d108704998d9b95f39ef61af616390`,
with integration parent `1674d852f7687b8d427cd4b77c3aee067688979f`.
The shared remote ref advanced during fetch; a standalone fetch retry exited 0
and the next merge reported already up to date. The same standalone scoped
byte-preservation command against `d879bbb561ae4a29b05b07a32b856ddf663596a8`
again exited 0 after this merge. Native and mutation gates remain applicable.

## Visible strict placement amount refusal (2026-10-18)

Overseer shared decision3 keeps nonfinite order amounts outside the admitted
placement-input domain. The maintained formatCreateOrder still produces quantity
"NaN", an empty positive-infinity payload list, or quantity"-Infinity"; those
actual-source observations remain in the existing oracle's optional
nativePlacementNumbers mode, without changing original corpus inputs or floors.

Previously the constructed public typed entry projected the amount to JSON null
and returned a generic decode Error with no field report. The native body
BitunixVenue::validate_placement_input, invoked by the existing typed capability
hook after caller-ID preflight, now emits the amount/original value and returns
TypeError before hydration or HTTP. Six single/batch cases cover all3 nonfinite
values without a caller-ID confound. Existing caller-ID rejection priority stays
intact. No carrier widening or zero substitution occurs.

The final regression compiles/fails against the old behavior; a control admitting
NaN through the final guard also compiles/fails at the field-error assertion.
Exact production source bytes are restored. Final constructed identity/dispatch7
tests pass, alongside the earlier9 unchanged source/orchestration tests. Required
all-feature cancellation7/override8/binding25, strict native/Wasm all-target
Clippy, formatting/diff, parity7459/16613 and extraction0 pass. Logs
/tmp/bitunix-visible-{amount-control-red,amount-fixed,final-native,
final-inventories,final-native-clippy,final-wasm-clippy,final-parity,
final-extract}.log. This accepted input residual is recorded in known-residuals.md.

## Money-path D: attached amendment cancel/recreate

Maintained source `updateOrder` catches an unsupported-operation refusal on an
attached virtual TP/SL leg and calls `recreateAttachedTpslParentOrder`. The native
binding previously emitted/threw without recovery. The new public
`native_identity::attached_amendment_recreates_only_after_matching_refusal_and_successful_cancel`
regression constructs the actual venue, signed clients and DefaultStore, replacing
only terminal REST responses. Its optional source-oracle mode executes actual
amendment, fallback, hydration, formatter and placement bodies with the actual
TypeScript DefaultStore; the original model corpus stays unchanged.

Eight source/native cases retain TP/SL trigger types and protection legs, reject
nonmatching errors and ordinary-parent unsupported errors without cancel, refuse
both LIMIT leg execution semantics before cancel, preserve Store on failed
cancel, and retain canceled Store status on failed placement. Exact payloads and
request order, returned chart aliases, visible errors and exact parent/child
versus neighboring-ID status changes are compared. Fixture error envelopes keep
`data: {}` so method emissions are observed independently of the API's separate
msg-without-data diagnostic.

The regression compiled and failed on master (`f203fd13c2`, expected NEW__tp,
actual unsupported-operation error). With the final assertions, restoring the
original `update_order` body compiled and failed the same recovery assertion;
exact source restoration passed. Logs: `/tmp/bitunix-attached-red.log`,
`/tmp/bitunix-attached-control-red.log`, and
`/tmp/bitunix-attached-control-restored.log`.

Scoped native identity/model/placement/trait tests, strict native and WASM
all-target Clippy, all-feature cancellation/override/binding inventories and full
parity passed. Inventory outcomes are cancellation6pass/1explicit export-ignore,
override7pass/1live-TS-ignore and binding25pass. Changed production bodies have no
cancellation/override body pins; the required target passed without a pin edit.
Manifest rows now map the fallback, string classifier and internal trigger helper
using the existing retained snapshot/UTF-8 boundaries. No carrier migration.

Final normal-master refresh `c5b55adb18` preserves both distinct append-only test
and doc additions. All18 scoped native tests (including the inherited80-case
ticker witness), native/WASM strict all-target lint, inventories6+7+25 with the two
explicit ignores, parity7459/16587 and extractor0updates pass. The independent
money-path audit WEEX receipt is57 concrete binding tests +1 original trading
corpus, all passing. All checks ran serially with Cargo jobs1.
