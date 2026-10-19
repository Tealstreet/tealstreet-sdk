# Bybit non-UTF-8 FIX-audit

Authority: the 37 exact rows in `assignments/codex-0w5ohi.json`, linked from
`non-utf8-assignment-index.json`, and the async ruling in
`divergence-cluster-rulings.md`. Twenty-five rows are reconciled below; the other
12 remain with the delegated production-binding audit owner.

| Assignment rows | Source owner | Verdict | Production observation |
| --- | --- | --- | --- |
| 4, 14, 17 | Primary / inverse / spot `reinitOnThrow` | ACCEPT | The installed receiver already starts the canonical Promise and retains its rejection continuation after the caller drops its waiter. Both initial flag states, success and rejection are tested on constructed native owners. The generic Rust helper's cancellation does not describe this production receiver. |
| 5, 15, 19 | Primary / inverse / spot `fetchBalance` | FIX | The installed member starts its raw request eagerly and retains native raw/formatted identity and the post-await disposal guard. The common typed SDK invocation helper delayed entry until waiter polling; it now calls the existing member first and awaits that same Promise. |
| 34 | Primary `changePositionMode` | FIX | The public SDK unit wrapper now reaches the original linear/USDT helper before its waiter is polled. Held helper settlement and waiter abandonment retain the installed Promise continuation. Option/inverse symbol refusal is unchanged. |

The wrapper census identifies a deferred **JS invocation** family. The typed
`start_eager` boundary is for native async bodies; this family already owns a
canonical source Promise. Both common helpers and the specialized `get_account`
wrapper capture the actual member result before boxing the Rust waiter. They
await that same result through `await_js`, with no additional task or Promise.

`binding/receiver_tests.rs` constructs NativeRuntime and the primary/demo/inverse/spot production
variants. The value-helper probe invokes the real installed `fetchBalance`
member with held terminal raw/formatter dependencies; it does not claim a
separate public SDK fetch-balance method. The public `get_account` and
`change_position_mode` probes exercise their actual SDK methods. Native raw
identity, UTF-16 and signed zero are observed before typed conversion. The
account probe retains the established numeric ID-to-text conversion.

`tests/exchange_binding.rs` discards an unpolled public orders waiter, drives the
real signed client and holds its first HTTP response. Releasing it must send the
second settle-coin read without recreating or polling the SDK waiter. Both
requests retain their production signature headers.

`scripts/eager-sdk-fixtures.cjs` extracts the maintained primary/inverse/spot
balance methods and primary position-mode method. It asserts eager invocation,
held-response order and the discarded Promise disposal branches in six balance
profiles plus one unit and one account profile. The existing account-reader
oracle executes these checks before its unchanged 104 profiles. Existing account-reader, execution-guard and
binding source corpora remain in place. Stream-handler carrier work is separate.

The inherited factory probe now gives its two callers one shared observer waker
and gives runtime/transport work a separate waker. Exact live observer counts
remain two, or one after dropping the first waiter; owner cancellation notifies
both and releases transport. Releasing a held response must also wake the runtime.
The old combined count included an extra eager scheduler wake (4 versus 3),
which has no TS Promise counterpart. The maintained Axios source independently
asserts that both response observers settle after probe release. Original
singleflight, cached-response, HTTP, clock, auth and disposal assertions remain.

## Position-mode and position-index receiver batch

| Assignment rows | Source owner | Verdict | Production observation |
| --- | --- | --- | --- |
| 9 | Primary `fetchPositionMode` | FIX | The real primary/demo receiver now binds the original mapper once and supplies the original array and index through canonical native `Array.map`. The prior receiver passed only the row. Mapped row identity and the first mode's UTF-16 units survive publication. |
| 10 | Primary `getOrderPositionIdx` | ACCEPT | The installed member already starts eagerly, retains the original symbol graph, reads mutable side/reduceOnly after await and forwards native rejection identity. A compiled early-copy fault fails the live-graph check. Placement and amendment now call that same member. |
| 11 | Primary `getStopOrderPositionIdx` | FIX | The member was absent on the primary receiver and placement bypassed it. It is now installed once for all variants and used by placement. Its omit facade retains inherited enumerable fields and shallow aliases. |
| 20 | Spot `getOrderPositionIdx` | ACCEPT | The actual Spot member returns a Promise of zero without reading options. A throwing symbol getter is never evaluated; a compiled nonzero-return fault fails. |
| 21 | Spot `getStopOrderPositionIdx` | FIX | The old own-only copy lost inherited fields and threw getter errors synchronously. The shared member now keeps inherited aliases and rejects its source Promise with the original thrown value. |

The maintained-source execution-guard oracle adds 28 receiver profiles while
preserving all existing 20 window, 8 reinit, 87 index, 24 stop and 39 execution
cases. Real constructed primary/demo/inverse/spot receivers observe source
method dispatch, native symbol/array/row identity, UTF-16, signed zero, mutable
options and original rejection values. Numeric/text/prototype index lookup and
getter rejection are checked before any typed conversion. The existing JSON
helpers remain supplemental public adapter coverage, not production bindings.

The three production defects compiled RED on master `0f5dbb345e`: missing member
`stop_index_live_member`, own-only copy `stop_index_inherited_alias`, and mapper
arity `position_mode_mapper_arguments`. `position-index-controls.py` also
compiles early-copy and nonzero-Spot-return faults, requires named runtime
failures, restores exact bytes and requires GREEN. No WebSocket bodies change.

The position batch reconciles five rows. With the seven rows in PR #1182,
row 37 ACCEPT-safety in PR #1188 and the five Hyro rows below, 18 of 37
are reconciled and 19 remain.

The former 19-row remainder was split by module: codex-0efu4x owns rows
1/2/3/6/7/8/12/13/16/23/25/35 (public/private socket lifecycle, history mapping
and position controls); codex-0w5ohi retained rows
18/24/26/27/32/33/36 (reconciled in the retained batch below) (Spot, placement options, account,
market and startup/facade work). Existing WS carrier owners retain their separate
body changes; the socket audit owner coordinates those boundaries first.

## Original-method retry guard: row 37 ACCEPT-safety

The coordinator ruled the original-method guard an accepted safety residual on
2026-10-19 (thread-dm3hk29u8vyn). It remains in the production factory unchanged.
An original order-mutating POST may have executed before a network failure;
replaying its body can duplicate or contradict that operation.

The maintained `createAPI` / `shouldRetryBybitRequest`, actual Axios and
axios-retry replay `/v5/order/create`, `/v5/order/cancel` and `/v5/order/amend`
when a terminal adapter reports `error.config.method = 'get'`. Each source case
sends POST then signed GET to the same endpoint, retains the serialized POST
body, and generates a fresh request ID and signature. An unchanged POST failure
makes only one attempt. These are scripted transport witnesses; they do not
assert that the live venue accepts a GET mutation or that a duplicate occurred.

`tests/api_factory.rs::native_order_mutation_network_failures_never_replay_post_bodies`
uses the actual native factory and signed terminal requests for all three routes
with unchanged, lowercase GET and uppercase GET failure configs. All nine must
make one attempt, preserve POST bytes/HMAC and retain the network error config.
The source test executes the six maintained-source rewrite/control profiles.
`factory-retry-condition-control.py` bypasses only the original-method conjunct,
requires the named native replay-count failure, then restores exact production
bytes and requires GREEN. Its existing instruments-info exclusion control stays.

Evidence: `/tmp/bybit-retry-mutating-evidence.json` records the initial three
source replays; the reproducible maintained oracle is
`scripts/create-api-retry-config-boundaries.cjs` (`safetyCases`). Row 37 is an
accepted safety residual. The seven earlier verdicts and their proof are in
PR #1182.


## Hyro account and configuration: rows 22, 28, 29, 30, 31

| Assignment rows | Source owner | Verdict | Production observation |
| --- | --- | --- | --- |
| 22 | `fetchMarginAccountInfos` | ACCEPT | The real receiver starts the balance dependency before polling, retains its original native value and publishes `{balance}` after settlement only while active. |
| 28 | `validateAccount` | FIX | Error classification now uses the shared `Js::ordinary_has_instance` prototype operation. An ordinary object inheriting Error.prototype must return and emit its original message, without an invented `Error: ` prefix. |
| 29 | `fetchIncomeHistoryRange` | ACCEPT | The installed method already ignores range arguments and returns fresh Promises and empty arrays without HTTP; the source fallback brand remains. |
| 30, 31 | `optionsEnabled`, `inverseEnabled` | ACCEPT | Both modes remain disabled for production and demo Hyro even when account options explicitly opt in. |

`receiver_tests::hyro_divergence_account_configuration_receivers` constructs the
actual native venue with configured test origins and no wire transport. It
observes held balance settlement, disposal, native identity, UTF16 and negative
zero. Validation covers synchronous and awaited failures, detached Error
prototypes, primitive throws, raw message values and original logger rejection.
The maintained five TS members execute 21 independent profiles in
`scripts/hyro-account-fixtures.cjs`; the existing validation corpus stays intact.

The shared operation follows the bound target even for call-only functions,
checks callability before the left operand, and reads the constructor prototype
only for object operands. Its Node unit compares thirteen cases, including null
prototypes and bound constructors; native checks retain throwing-getter identity
and prove that shadowed `__proto__` accessors are not read.
`scripts/hyro-account-controls.py` mutates each accepted guard/result and the
Error classification separately, requires compiled named native failures, then
restores exact source bytes and replays the receiver.

## Retained startup, markets, tickers and placement: rows 18/24/26/27/32/33/36

| Row | Verdict | Production binding evidence |
| --- | --- | --- |
| 18 | FIX | Spot startup kept its eager Promise and sequential markets/tickers flow, but narrowed the private-probe warning to UTF-8. The original warning code units now reach the logger. |
| 24 | ACCEPT | The stronger enum is strictly placement input. Six null/malformed/conflicting extension profiles refuse through the actual placement capability before signing or HTTP; existing positive BBO/tick-size/ordinary bindings retain allowed fields. |
| 26 | ACCEPT | Decision 3 applies per market row. Good/bad/good HTTP pages retain both valid neighbors through fetch and refresh, while malformed asset/symbol values are visibly reported. Source property/coercion errors remain distinct. |
| 27 | FIX | Spot's production helper projected the complete response through JSON before mapping. It now captures retained metadata directly, including own undefined, lone-surrogate text and infinity; row-only typed refusal remains. |
| 32 | ACCEPT | The actual primary receiver already starts its canonical Promise eagerly, fetches markets/tickers concurrently and keeps continuations after waiter abandonment. Detached cache failures visibly report the original value under the shared no-silent-failure rule, instead of the source empty catch. |
| 33 | FIX | Primary/Spot production parsing now uses the original indexed list and live market dictionary. Falsy input returns the original cached array; unread metadata is untouched and parseFloat retains UTF-16 prefixes. |
| 36 | ACCEPT | The shared Rust carrier installs the separate Spot members. Existing actual facade/socket/trading tests confirm Spot class identity, category, fixed leverage, empty positions and inherited refusals. |

`receiver_tests::retained_audit_*` constructs the native production owner and
observes raw identity, UTF-16 units, IEEE bits, held clock/request settlement,
loaded flags, detached reporting and private connection/snapshot/tick ordering.
The three master defects compile and fail by assertion before their fixes.
`scripts/retained-audit-fixtures.cjs` executes the maintained TS members through
the existing API-validation source gate. `retained-audit-controls.py` compiles
cache-copy, numeric-prefix, raw-info projection and warning-text faults, then
restores exact production bytes and requires the named native witnesses GREEN. The market-row HTTP, Spot facade and
primary placement targets retain their original cases and effects. The exported
JSON mappers remain supplemental tests, with no production fallback through them.

This batch reconciles all seven rows retained by codex-0w5ohi. The twelve rows
delegated to codex-0efu4x retain that owner's separate milestones and verdicts.


## Delegated private trade filter: row 35 FIX

The production private-frame handler took a dense snapshot before invoking the
live category selector. A selector replacing a later row therefore delivered the
old row, unlike the maintained `rowsThisConnectionTrades` member. The native
binding now calls the original array filter member. The shared array algorithm
captures the initial length, skips absent indices and reads later rows after
each preceding callback. Its completed result feeds the existing event consumers.

`binding::delegated_socket_tests::private_filter_reads_live_indices_through_real_frame_binding`
constructs primary and demo exchanges and enters through their real private
socket. Fourteen profiles check callback order, selected count and original row
identity under replacement, deletion, truncation, appends, sparse input and false
selectors. The seven corresponding maintained-member profiles run directly from
`scripts/private-filter-fixtures.cjs`. Existing account-category, dedicated
Spot/Inverse and typed event/output guards stay in place. Independent compiled
snapshot and ignored-selector faults must fail the named real-frame witness;
`private-filter-controls.py` restores exact source bytes and requires GREEN.

This milestone reconciles delegated row35. Rows6/16 retain their separate #1198
history milestone; the other nine delegated rows remain open.
