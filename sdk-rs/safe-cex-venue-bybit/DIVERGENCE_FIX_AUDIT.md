# Bybit non-UTF-8 FIX-audit

Authority: the 37 exact rows in `assignments/codex-0w5ohi.json`, linked from
`non-utf8-assignment-index.json`, and the async ruling in
`divergence-cluster-rulings.md`. All 37 rows are reconciled below, including history rows 6/16 from
PR #1198, socket rows 25/35, public scheduling rows 1/2/12/13 and the final
private reconnect / position-control rows 3/7/8/23.

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
row 37 ACCEPT-safety in PR #1188, the five Hyro rows below and delegated
history rows 6/16 in PR #1198, plus the seven retained rows in PR #1212,
37 of 37 are reconciled after socket rows25/35, the public scheduling audit and
the final private reconnect / position-control audit below.

The final four delegated rows 3/7/8/23 are reconciled in the final audit below.
The seven rows retained by codex-0w5ohi, 18/24/26/27/32/33/36, are reconciled
in the retained batch below. Existing WS carrier owners retain their separate
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

## Delegated history: rows 6 and 16

Both primary and inverse history mapper rows are FIX in PR #1198. The live
historical-fills reader visibly refuses malformed strict output fields after
source paging and recovery, preserving valid neighbors and source error priority.
The synchronous raw mapper and its original identity/alias guards are unchanged.
See `DELEGATED_HISTORY_FIX_AUDIT.md` for the real-binding RED/GREEN, six compiled
production controls and its original ten-row delegated remainder.

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

This batch reconciles all seven rows retained by codex-0w5ohi. Delegated socket
rows25/35 are reconciled below; history rows6/16 retain their separate #1198
milestone. The other four delegated rows remain open.


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
history milestone; the other delegated rows retain the separate verdicts below.


## Delegated inverse auth: row 25 ACCEPT

The numeric-expiry difference requires a nonfinite clock result. The production
native registry supplies `VIRTUAL_CLOCK`, whose default clock reads SystemTime;
private reconnect clears its prior offset before the auth sample. Auth adds a
fixed10000ms, with no account/configuration expiry input. An injected broken clock
can still exercise the previously recorded nonfinite JSON boundary; it does not
describe this constructed native production path. The coordinator authorized this
reachability classification on thread-dm50ve5c1yer,2026-10-19.

`inverse_auth_production_clock_keeps_finite_expiry_at_real_signer` enters the real
inverse private onOpen, waits for its source25ms boundary and captures the original
hosted signer request before its pending result settles. It verifies the actual
clock interval, cleared stale offset, finite native expiry, exact signing payload,
request kind and account identity. The maintained inverse auth and clock members
execute three native-Node profiles with real Date.now and the original offset
reset. A compiled null-expiry production fault must fail this witness before an
exact restored GREEN. No auth/signing carrier or production body changes here.

This socket batch reconciles rows25 and35. History rows6/16 are merged from #1198;
the four other delegated rows remain open.


The shared public-topic timing control is strengthened alongside this socket
batch; the public timing reconciliation below closes its assignment rows. The older witness advanced
virtual time in50ms increments, so a first market-chunk wait changed from5ms to0ms
could survive. `market_additions_wait_five_ms_between_real_socket_chunks` now
runs actual Spot and dedicated Inverse sockets at unchanged time,4ms and5ms for
each remaining chunk. It retains the eager first send, exact topic count/order,
and existing pending-tail disposal/weak-owner guards. Both maintained TS methods
independently execute with held sleeps, checking sends at0/5/10ms and the final
wait through15ms. `public-topic-controls.py` requires each production fault to
compile RED, restores every source byte and requires the full target GREEN.


## Dedicated public scheduling: rows 1/2/12/13 ACCEPT

| Rows | Live source member | Reason |
| --- | --- | --- |
| 1, 12 | Spot/Inverse onMarketsChanged | The installed markets listener commits topic state, sends the first chunk and creates its 5ms sleep synchronously. It then transfers the tail to the actual socket's owned operation, so emitter return does not discard it. |
| 2, 13 | Spot/Inverse subscribe | Actual onOpen sends the first ten-topic chunk before ping. The owned continuation sends later chunks at 10ms boundaries without a caller waiter being polled. |

The original first-poll/cancel-on-waiter-drop notes described a helper future in
isolation. Neither production path exposes that waiter: PublicTopics immediately
calls BaseWebSocket::start_message_operation. Explicit socket/owner disposal
cancels the whole native operation under the accepted deterministic-teardown
ruling and exchange-runtime-callback-choice decision. This audit does not widen
typed topic/Market ingress or claim arbitrary executable input graphs.

The new dedicated_subscribe_starts_before_ping_and_retains_ten_ms_continuation
witness constructs real Spot and dedicated Inverse sockets with 25 active markets.
Maintained source and native wire chunks agree at 0/9/10/19/20/29/30ms. Source
settlement still awaits the final 10ms sleep and empty topics create no waits.
The existing market_additions_wait_five_ms_between_real_socket_chunks retains
its two dedicated protocols, exact 0/4/5/9/10ms sends, ordered topics and duplicate
append behavior. The original 24 public-source compositions, old admission/alias
assertions and socket-disposal/weak-owner checks remain intact.

public-scheduling-controls.py compiles four independent production faults:
deferred subscription prefix, zero first subscription wait, discarded subscription
tail and discarded market tail. Every fault must fail its named real-socket test;
exact source restoration must pass before the next fault. No production body or
identity/alias guard changes in that batch. Its four remaining delegated rows
3/7/8/23 are reconciled by the final audit below.

## Final delegated reconnect and position controls

| Assignment rows | Source owner | Verdict | Production observation |
| --- | --- | --- | --- |
| 3, 23 | Inverse / Spot `sendReconnectSequence` | ACCEPT | The constructed protocol calls the actual owned reconnect operation: clock sync starts before onOpen returns, the held clock gates the 25ms sleep and real authentication, then the variant subscribes to order+position or order only and pings. Local/signed/missing/error and initially disposed profiles match the maintained variant methods. Native disposal cancels pending tails under the existing PRIVATE_OPEN_REVIEW.md ruling; the source detached Promise survives disposal. This remains an explicit bounded divergence. |
| 7 | Primary `setLeverage` | ACCEPT | The public SDK invokes the installed member before waiter polling. Cached positions start POST immediately; empty positions start fetch immediately. Dropping the SDK waiter preserves initial upsert, a live market-limit read after the held fetch, POST, and the original refreshed allocation's publication. Primary/demo receivers match both source profiles. The supplementary public JSON helper's first-poll contract is not the production receiver. |
| 8 | Primary `changePositionModeHelper` | ACCEPT | The installed helper starts POST before returning its canonical Promise. Abandoning the Rust awaiter preserves all-position refresh, optional symbol refresh, publication and the final success/partial/refusal outcome, in that order. Six source profiles match each primary/demo receiver. Native fetched allocations, signed zero and lone-surrogate units survive the callbacks unchanged. Optional request omission and the documented typed helper input boundary remain classified separately. |

`private-open-fixtures.mjs` extracts each maintained primary/inverse/Spot owner's
onOpen, reconnect, auth, subscribe and ping methods. The existing eight primary
profiles remain; the additional inverse and Spot profiles run through the real
`PrivateProtocol`/`BaseWebSocket` composition with both queued and inline runtimes
(32 additional comparisons). The separate disposal regression keeps all three
unpolled/pending/inline-pending cancellation checks. No source callback recovery
or diagnostic ruling changes.

`delegated-controls-fixtures.cjs` extracts the maintained leverage and mode-helper
methods and holds each terminal request/refresh. The native probe constructs
primary/demo exchanges, executes their original control members and compares
five checkpoints for each of eight source profiles (16 native compositions).
Its Store observer checks original fetched allocation identity before conversion;
terminal HTTP/fetch dependencies are controlled, not the control algorithm.
Successful, partial and rejected mode responses retain exact diagnostics. The
existing 32 retained-position and full position-control corpora remain unchanged.

`delegated-audit-controls.py` independently compiles missing clock-prefix,
forced auth-refusal, deferred SDK-unit call and lost mode-refresh publication
faults. Each must fail its named real-binding assertion, then pass after exact
source restoration. These are probe sensitivity controls, not new production FIX
claims. No production algorithm, decoder, identity/alias guard, or shared Promise
scheduler changes in this batch.
