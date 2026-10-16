# Store notification audit

Audited Coinbase at merge `228ca8aec8`; recorded on `7ca0e78c04`. This follows
Bybit's `STORE_NOTIFICATION_AUDIT.md` method. Its check script is not in this
integration checkout, so the Coinbase audit records and runs its own AST census
and reachability check.

## Ported-row reachability

At the recorded audit commit, the Coinbase manifest contained 426 rows: 254
ported, 163 pending, and 9 divergent. At the current follow-on head it contains
270 ported, 143 pending, and 13 divergent. The maintained checker now resolves
154 current ported callable rows to TypeScript declarations, then follows
local call expressions to direct Store mutation methods. It finds zero
reachable Store notification calls. It also asserts that every direct Store
writer remains pending or divergent with a `FINDING` note, so a future writer
cannot be claimed ported while the subscriber-error contract is not representable.

The ported `CoinbaseStarbaseSequencedOrderBook` consumes the shared sequenced
book operation with a persistent `OrderBook` handle. Its state updates are
book-local; the Rust path does not call `Store` or `Store::update`, and the
subscription helper methods do not publish through Store. No exchange or
WebSocket lifecycle row is ported.

Negative controls: injecting `this.store.update({})` into the ported
`CoinbaseStarbaseReadOnlyState.applyPositions` makes the checker exit 1 because
that row would remain marked ported. Separately, routing that method through
`this.start()` makes the transitive reachability assertion fail. Both source
mutations were restored; the latest checker run reports 142/142 declarations,
zero reachable notification paths, 13 direct sites, and 10 direct writer rows
(eight pending and two divergent). The auth-failed handler is an additional
indirect writer through `markPrivateUserStreamDegraded` and is also divergent.
Adversarial review found computed Store access and local mutator aliases were
not recognized. The checker now covers `this.store["update"](...)`, a bound
`const notify = this.store.update; notify(...)`, and `const store = this["store"];
store.update(...)` as executable self-controls; all three are detected as direct
writers.

## Direct Store writers

The source census found 13 direct notification call sites in 10 writer rows:

| TypeScript row | Store operation | Calls |
| --- | --- | ---: |
| `CoinbaseExchange.start` | `update` | 1 |
| `CoinbaseExchange._startPublicDataOnly` | `update` | 1 |
| `CoinbaseExchange.tick` | `update` | 4 |
| `CoinbaseExchange.markPrivateUserStreamMessageReceived` | `updateSocketHealth` | 1 (divergent) |
| `CoinbaseExchange.markPrivateUserStreamDegraded` | `updateSocketHealth` | 1 (divergent) |
| `CoinbaseExchange#reconcileOpenOrdersFromRest` | `update` | 1 |
| `CoinbaseExchange.cancelOrders` | `updateOrdersStatus` | 1 |
| `CoinbaseExchange.placeOrder` | `addOrUpdateOrder` | 1 |
| `CoinbasePrivateWebsocket.handleUserEventOrders` | `addOrUpdateOrders` | 1 |
| `CoinbasePrivateWebsocket#applyOrderSnapshotBatch` | `update` | 1 |

No source `setSetting` call is active; the only Coinbase occurrence is
commented out. When these handlers are ported, record the concrete notification
call site and classify each affected row `divergent` with a `FINDING` citing the
shared notification contract until the fallible ABI represents the subscriber
failure. The Store owner is cutting over the full mutation API; do not add a
venue-local wrapper or panic in place of the notification error.

The checker intentionally follows local source call names conservatively and
has an explicit Store mutator inventory. Imported/core operations are separate
boundaries and need contract inspection; the sequenced-book operation above was
checked directly. Extend the mutator inventory when the shared notification
API changes.

The three implemented health handler rows had live-source tests in
`src/private_health_tests.rs`; their pre-cutover `FINDING` notes identified the
missing fallible notification behavior. The auth-failed row reaches the direct
Store writer through its delegation to `markPrivateUserStreamDegraded`.

## Rust Store result conversion sweep before the cutover

Re-audited by mutation callee, not by conversion spelling, after the worker's
inferred `if let Err` finding and before integration `9419b340d8a6`. Enumerated the public Store mutation callees in
the shared contract (`update`, `update_*`, `add_or_update_*`, and `set_setting`)
and searched every Coinbase production Rust module for direct, qualified,
trait-method, and locally aliased calls. `private_health.rs` has two direct Store
mutation call sites: `Store::update` and `Store::update_socket_health`. At that
time, both returned unit, so there were zero Store mutation Results whose
error type is inferred, stringified, converted with `?`/`Into`, or discarded
with `.ok()`/fallback handling. This is a measured zero by callee reachability
within the implemented production Rust surface, not a `map_err`-only search.
At that pre-cutover checkpoint, the 13 TypeScript mutation sites above were
tracked across eight pending and two direct divergent writer rows; the
auth-failed delegating row was a third notification-crossing divergence.

## Fallible notification cutover

After integration `9419b340d8a6`, all three private-health handlers return
`Result<(), StoreMutationError>`. `mark_private_user_stream_message_received`
returns `Store::update` directly, and `mark_private_user_stream_degraded`
propagates `Store::update_socket_health` before logging or warning; the
auth-failed helper returns the delegated result unchanged. These three rows
are now `ported`. The error remains `StoreMutationError::Notification`, which
retains the Store-local subscriber ID and original `StoreSubscriberError`; it
is not converted into `ExchangeError`, whose subscriber-ID field has not
landed in integration. Subscriber IDs from different Stores are never compared
as global identities.

The live-source test still covers successful transition order. The new
subscriber-failure control asserts the state commits, the first failure and
Store-local ID reach the handler caller, later subscribers are skipped, and
degraded logging/warning effects do not run after the failed update. The
message-received path separately verifies that its local timestamp/reset and
health clearing occur before the returned notification error. The checker
allows these three named result-returning handlers and continues to require all
other direct Store writers to remain pending or divergent with a `FINDING`.

## Post-`a09ea6489b` re-enumeration

After merging the corrected integration head, the Coinbase call-reachability
checker resolved 154/154 currently ported callable rows, found zero reachable
notification chains, and inventoried 13 direct calls across 10 pending writer
rows. The scoped `cargo check -p safe-cex-venue-coinbase --all-targets
--locked --jobs 1` then completed with no Coinbase diagnostics. This is the
first post-merge iteration; it produced no additional Coinbase Store sites.
The separate workspace-level KuCoin unused-`Result` diagnostics reported by
the integration owner are outside this crate and are not included in this
count.
