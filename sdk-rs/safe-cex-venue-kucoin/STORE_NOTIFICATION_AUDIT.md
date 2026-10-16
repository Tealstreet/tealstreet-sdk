# Store notification reachability audit

At merge `469cb47e90`, all 191 KuCoin rows were measured: 62 ported, 29 divergent,
100 pending. The preceding audit commit `2ddf2941ab` moved Spot `setLeverage`
from ported to FINDING; it is the one direct implemented Store writer. There are
no implemented `set_setting` callers.

The follow-up treats notification as reachability, not a direct-call grep.
A TypeScript AST walk resolved all 30 remaining ported callable/accessor rows:
25 field functions, four constant functions and one accessor. The other 32
ported rows are data/types. A reviewer independently resolved the same census.

The transitive venue calls are `toInternalSymbol -> toKucoinSymbol/toBinance`,
`toApiSymbol -> toKucoinSymbol`, sizing/remainder helpers -> `contractSizeFor`,
position-side helpers -> `isAlgoOrder`, broker config -> `trimString`, and
`snapshotShardSymbols -> parent.toApiSymbol`. These end in pure conversion,
number arithmetic and collection reads. Their Store operations are getters for
markets, symbol indexes, orders, positions and tickers. Account-mode decisions,
protocol tables and record decoding do not enter lifecycle or notification.
Spot `changeMarginType` and `changePositionMode` are source no-ops. Neither
ported socket helper (`snapshotShardSymbols`, `getCandleHandlerTopic`) calls
send, connect, reconnect or health publication. Thus no additional ported row
needs reclassification: one affected formerly ported row remains the result.

The already-divergent `sendChannelOp` does have an independent fixed path:

`sendChannelOp -> BaseWebSocket.send -> reconnect -> onClose -> processClose
-> BaseExchange.syncSocketHealthToStore -> syncSocketHealthToStoreInternal
-> DefaultStore.updateSocketHealth -> notify`.

For a non-heartbeat subscription frame with no socket, source `send` starts
reconnect without awaiting it. Store health commits, the first listener throws,
and later listeners are skipped. The reconnect promise rejects with the same
error; the synchronous venue helper still returns normally. This differs from
Spot `setLeverage`, whose own promise rejects. Its manifest note retains both
the prior CHOSEN UTF-8 boundary and this separate shared notification FINDING.

`socket-store-notifications.cjs` runs the actual source venue/core method chain,
actual heartbeat predicate, and production Store constructor/update/notify.
Socket inventory, diagnostics and watchdog startup are explicit fixture inputs;
this is not a complete socket-constructor/lifecycle test. It observes the
otherwise-unawaited promise, without changing `send`'s return. Subscribe and
unsubscribe both exhibit the failure; ping takes the actual early return.
A catch-and-continue mutation in actual Store notify compiles and fails the
named skipped-subscriber assertion through the Rust test child (exit 101).

The original audit used the infallible shared notification signature. The
local cutover below replaces it without a venue error side channel or panic.
Arbitrary callbacks supplied by consumers are outside this fixed-call audit.
Pending runtime rows are not being claimed implemented by this census.

Spot cancel_all_orders: zero Store writes and zero fixed callees publishing
health. Calls the supplied unlimited RestAdapter and BaseExchange error emitter;
error emission does not notify Store. No new notification cutover call site.

## Notification ABI cutover inventory

The local merge of `af44700b073debf65911b9985f925ea8b20d482c` collects six
production sites. This is an implementation inventory; no row is promoted by
merely compiling against the new contract.

| Site | Propagated contract |
| --- | --- |
| `spot_helpers::set_leverage` Store update | `Result<(), StoreMutationError>` unchanged, including notification id and cause |
| `accounts::emit` | `Result<(), ExchangeError>` through account and validation callers |
| `order_reads::emit` | `Result<(), ExchangeError>` through both categories and the combined read |
| `spot_reads::fetch_markets` reporting | `Result<(), ExchangeError>` before fallback |
| `spot_reads::fetch_tickers` reporting | `Result<(), ExchangeError>` before fallback |
| `spot_writes::cancel_all_orders` reporting | original listener `ExchangeError` boxed into the existing standard Error contract |

The sixth site, Spot cancellation, was absent from core's five-site enumeration.
Bad-code read emissions inside the source try can fail into its catch, which
emits again. Catch emissions escape on failure and cannot return the fallback.
Cancellation begins reporting in the catch, so a listener failure replaces the
original request failure; successful reporting preserves that request failure.
Raw non-string error input remains a separate pending core contract. The
indirect socket-send finding above is not closed by these six call-site edits.

At integration `9af82bfeae`, ExchangeErrorMetadata contains the shared
store_subscriber_ids list. Reporting tests retain distinct inner-to-outer lists
on the first and second listener errors. Equal ids are never compared across
Stores. The leverage method returns StoreMutationError directly. Cancellation's
existing Box<dyn Error> remains necessary for its RestFailure/ExchangeError
return alternatives; it was not introduced as a subscriber-id workaround.

The symbol-cancel entry uses the same fallible cancel_request reporting calls as
cancel-all; it introduces no new Store write or independent emission conversion.
The sixty-case reporting oracle includes symbol cancellation with successful and
throwing listeners for both venue rejection and transport failure.

Sequential Spot cancel_orders reuses cancel_request, adding no Store write or
emission conversion. The reporting corpus now has 66 cases: six new cases enter
the actual cancelOrders method with two orders and assert failure stops at one
request. Both listener-error precedence and intact transport failure are checked.
