# Bybit non-UTF-8 FIX-audit

Authority: the 37 exact rows in `assignments/codex-0w5ohi.json`, linked from
`non-utf8-assignment-index.json`, and the async ruling in
`divergence-cluster-rulings.md`. Thirteen rows are reconciled below; the other
24 remain pending individual production-binding audits.

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
`scripts/create-api-retry-config-boundaries.cjs` (`safetyCases`). Row 37 is an accepted safety residual. The seven earlier verdicts
and their proof are in PR #1182.


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
