# AsterDEX Store notification audit

Audited the currently ported AsterDEX manifest rows by source reachability,
including calls made by each ported method, after reading the Bybit
`STORE_NOTIFICATION_AUDIT.md` method. This is not a Rust call-site grep.

One row reaches a notifying Store operation: `AsterDEXExchange.afterMethodCalledInWorker`
calls `Store.setSetting` when `method === 'changePositionMode'`. The production
`DefaultStore.setSetting` mutates `options.isHedged` before notifying. A throwing
subscriber therefore leaves the new value committed and throws out of the hook.
The Rust helper now invokes the same mutation through a fallible callback, and
its test proves the committed value remains changed when notification fails.
The shared Store API and `ExchangeError` conversion preserve the Store-local
subscriber id; the worker-hook row is now ported.

The extracted `AsterDEXPrivateWebsocket#handleExecutionReport` and
`#mirrorCumulativeOrderFill` also cross this seam at their `Store.updateOrder`
calls in `asterdex.ws-private.ts`. Their Rust helpers preserve the update
decisions but do not apply them; both manifest rows remain divergent until a
runtime applies the write and propagates its error. Any Rust caller must
propagate the typed error rather than stringify or discard it.

`AsterDEXPublicWebsocket#handleTradeUpdate` crosses the same boundary through
`Store.updateTicker`; the Rust helper returns the open-string trade event and
ticker update plan but does not apply the notification-producing write. The
new Store API and conversion can preserve the mutation error and subscriber id
when a runtime applies the plan.

`AsterDEXPublicWebsocket#handleMarkPriceUpdate` and `#handleTickerUpdate` also
call `Store.updateTicker` before emitting; their helpers return the patch and
event inputs but do not apply the notification-producing write. The new Store
API and conversion can preserve the mutation error and subscriber id when a
runtime applies those plans.

The other ported rows are definitions, pure helpers, read-only mappings, or
response/request decisions with no reachable Store notification. The currently
ported public websocket rows are `connectionURL`, `ping`, `subscribe`, and
`unsubscribe`.
`ping` sends only when the socket is connected. `subscribe` and `unsubscribe`
check the venue socket's `readyState === WebSocket.OPEN` before synchronously
calling `BaseWebSocket.send`; there is no await or callback between either
guard and send. These rows therefore cannot reach the no-socket reconnect path
that could continue through `processClose` and health publication. No AsterDEX
websocket lifecycle method is ported yet. The URL helper only reads testnet
options and formats the URL.

The pending `AsterDEXPrivateWebsocket#handleOrderTradeUpdate` also reaches
`Store.updateOrder`; it is listed for the next implementation slice.

Result: the worker hook's notification finding is closed and its row is ported.
**5 mapped handlers** cross the seam and remain divergent because their helpers
return plans without applying the notifying write. The pending
`handleOrderTradeUpdate` row is included in the inventory for its future slice;
other divergent handlers remain separately classified.
