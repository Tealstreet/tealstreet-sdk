# Bybit non-UTF-8 FIX-audit

Authority: the 37 exact rows in `assignments/codex-0w5ohi.json`, linked from
`non-utf8-assignment-index.json`, and the async ruling in
`divergence-cluster-rulings.md`. This first batch covers seven rows; the other
30 remain pending individual production-binding audits.

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

This batch reconciles five further rows. With the seven rows in PR #1182 and
row 37 ACCEPT-safety in separate PR #1188, 13 of 37 are reconciled and 24 remain.

The remaining 24 rows are split by module: codex-0efu4x owns rows
1/2/3/6/7/8/12/13/16/23/25/35 (public/private socket lifecycle, history mapping
and position controls); codex-0w5ohi retains rows
18/22/24/26/27/28/29/30/31/32/33/36 (Spot/Hyro, placement options, account,
market and startup/facade work). Existing WS carrier owners retain their separate
body changes; the socket audit owner coordinates those boundaries first.
