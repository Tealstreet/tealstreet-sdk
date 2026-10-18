# BloFin FIX-audit progress

Exact assignment: `codex-4ri6pb.json` from the non-UTF-8 assignment index;
30 BloFin rows. This ledger distinguishes measured fixes from pending probes.
An internal async function being lazy is not alone a production scheduling
verdict: the public SDK invokes it inside a runtime-owned eager operation.

The whole maintained-class oracle and actual constructed SDK regression are
`safe-cex-venue-blofin/tests/divergence_retry_source.cjs` and
`divergence_timing_binding.rs::rejected_order_family_does_not_cancel_a_sibling_pagination`.
On baseline master `0bc1800e5f`, failed normal orders cancel TPSL pagination after
`page-a`; source requests `page-b` too. The compiled assertion fails with two
requests versus three. With each child retained by the shared Promise driver,
the same assertion passes without advancing time and after the public waiter is
dropped. This measures sibling survival, not arbitrary microtask-count parity.

| Assigned row | Ruling / evidence |
| --- | --- |
| `BlofinExchange.fetchTickers` | ACCEPT assigned claim: `tickers`: actual SDK executes fetchTickersRaw/HTTP without polling the returned waiter; native last-price bits and one signed GET match the maintained class. #1050 owns the public eager prefix. |
| `BlofinExchange.fetchTickersRaw` | ACCEPT assigned claim: `tickers`: the actual parent reaches this raw reader and GET; discarded waiter still completes the same request. It is awaited immediately by the already-eager SDK parent, not returned as an independently lazy public operation. |
| `BlofinExchange.getOrFetchPositionMode` | FIX: cached-reader reaction cutover (this PR). Compiled original-body CACHED-AWAIT RED → GREEN; normal/trigger/batch read the new hedged mode after a legitimate worker acknowledgement; the old inline cache await sends net instead of long. |
| `BlofinExchange.getOrFetchMarginType` | FIX: cached-reader reaction cutover (this PR). Compiled original-body CACHED-AWAIT RED → GREEN; TPSL and batch read the new isolated margin after a legitimate worker acknowledgement; the old inline cache await sends cross. |
| `BlofinExchange.fetchLeverageForSymbol` | ACCEPT assigned claim: `leverage`: discarded SDK waiter still sends the source instId/marginMode and commits distinct buy/sell native leverage fields. Captured cached margin is cross on both sides. |
| `BlofinExchange.fetchOrders` | FIXED #1068: compiled real-binding sibling-pagination RED → GREEN; both the public entry and its retry join execute. Other helper rows remain pending individual timing probes. |
| `BlofinExchange#fetchOrdersWithRetry` | FIXED #1068: compiled real-binding sibling-pagination RED → GREEN; both the public entry and its retry join execute. Other helper rows remain pending individual timing probes. |
| `BlofinExchange#fetchPaginatedOrderRows` | ACCEPT assigned claim: `active-orders`: each of normal/TPSL/trigger sends its initial GET and exact cursor follow-up, then stops on the empty page after the public waiter is dropped. #1068 separately proves sibling survival after rejection. |
| `BlofinExchange.fetchNormalOrders` | ACCEPT assigned claim: `active-orders`: normal first page and npage cursor GET occur with awaited and dropped waiters; mapped output remains empty for the deliberately unknown instrument. |
| `BlofinExchange.fetchAlgoOrders` | ACCEPT assigned claim: `active-orders`: TPSL first page and apage cursor GET occur with awaited and dropped waiters; #1068 additionally retains its pagination after a different family rejects. |
| `BlofinExchange.fetchTriggerOrders` | ACCEPT assigned claim: `active-orders`: trigger first page and tpage cursor GET retain orderType=trigger with awaited and dropped waiters. |
| `BlofinExchange.fetchHistoricalOrders` | ACCEPT assigned claim: `history`: real SDK captures instId, begin/end and limit, performs the same signed GET and returns the same empty typed result; dropping the waiter still starts the request. |
| `BlofinExchange.cancelOrders` | ACCEPT assigned claim: three `cancel-*` profiles: real SDK groups and runs the respective actual private helper; already-gone Store reconciliation and continuation survive waiter drop. This parent deliberately awaits family groups in source order. |
| `BlofinExchange.setLeverage` | ACCEPT assigned claim: `set-leverage`: cached hedged/isolated reads lead to two source-ordered long/short POSTs, clamp 300 to 100, and commit the same native cache even after waiter drop. |
| `BlofinExchange.placeOrder` | FIX: cached-reader reaction cutover (this PR). Compiled original-body CACHED-AWAIT RED → GREEN; normal, TPSL and trigger public Base capability routes send the source mode fields and IDs, including after the waiter is dropped. |
| `BlofinExchange.placeOrders` | FIX: cached-reader reaction cutover (this PR). Compiled original-body CACHED-AWAIT RED → GREEN; mixed normal/TPSL/trigger Base capability route preserves source request order, modes and IDs after the pair of cached reads. |
| `BlofinExchange.changePositionMode` | FIXED #1050: actual SDK three call-prefix logs and confirmed Store mode match whole-class source. Source mapping from #1064 is preserved. |
| `BlofinExchange.changeMarginType` | ACCEPT assigned claim: `margin`: actual SDK performs the POST then forced margin GET, with the same source confirmation and dropped-waiter continuation; the real TS producer returns void. |
| `BlofinExchange#formatTriggerOrder` | FIX: cached-reader reaction cutover (this PR). Compiled original-body CACHED-AWAIT RED → GREEN; the trigger placement profile witnesses margin-before-position-mode suspension in the actual signed payload. |
| `BlofinExchange#placeTriggerOrder` | ACCEPT assigned claim: trigger and mixed-batch cache-race profiles reach the real placement POST and t1 ID after the parent formatter resumes; dropping the public waiter does not prevent that POST. This helper is immediately awaited by the owned parent. |
| `BlofinExchange#formatAlgoOrder` | FIX: cached-reader reaction cutover (this PR). Compiled original-body CACHED-AWAIT RED → GREEN; the TPSL placement profile witnesses position-mode-before-margin suspension in the actual signed payload. |
| `BlofinExchange#formatNormalOrder` | FIX: cached-reader reaction cutover (this PR). Compiled original-body CACHED-AWAIT RED → GREEN; the normal placement profile witnesses margin-before-position-mode suspension in the actual signed payload. |
| `BlofinExchange#placeOrderBatch` | ACCEPT assigned claim: normal and mixed-batch cache-race profiles reach the real batch POST and n1 ID; dropping the public waiter retains the placement. Internal borrowed future is immediately awaited by the owned SDK parent. |
| `BlofinExchange#placeAlgoOrderBatch` | ACCEPT assigned claim: TPSL and mixed-batch cache-race profiles reach the real TPSL POST and a1 ID; dropping the public waiter retains placement. Its source payload loop is sequential and immediately awaited by the owned parent. |
| `BlofinExchange#cancelNormalOrders` | ACCEPT assigned claim: `cancel-normal`: exact signed request and already-gone n removal match source before completion; same reconciliation survives dropped waiter. |
| `BlofinExchange#cancelAlgoOrders` | ACCEPT assigned claim: `cancel-algo`: exact signed TPSL-root request removes only a__stop_loss and survives dropped waiter; source real 1000 ms delay is retained. |
| `BlofinExchange#cancelTriggerOrders` | ACCEPT assigned claim: `cancel-trigger`: authoritative instId/algoId provenance is checked, actual signed POST removes t, and dropped waiter retains reconciliation plus the actual 350 ms timer. |
| `BlofinExchange#getPositionSide` | FIX: cached-reader reaction cutover (this PR). Compiled original-body CACHED-AWAIT RED → GREEN; actual normal/trigger payload changes from net to long and protective batch payload uses short after the cached mode Promise resumes. |
| `BlofinPositionAdapterDeps` | ACCEPT production reachability: `journal` executes the real SDK historical-position reader and its actual typed account/symbol/settle-currency closures. The source account/key/settleCurrency and base-unit closedSize=2 match despite market contractSize=1000. The producer supplies no contractSize callback; arbitrary throwing or malformed replacement callbacks are not installed by this binding. Existing generic-domain refusal remains documented, rather than widening the shared carrier. |
| `BlofinExchange.afterMethodCalledInWorker` | ACCEPT assigned claim: all four cache-race profiles execute the actual native/source hooks for supported boolean arguments and null/void results. Whole-class changePositionMode and changeMarginType producers are asserted to return undefined; arbitrary truthy result.positionMode records are not produced by these worker calls, so their closed-enum refusal remains an unreachable typed boundary. |

Remaining WEEX and candle-cache assignment rows are not classified by this ledger.

## Remaining-27 measurement scope

Eight assigned rows share the cached-await bug: both mode readers, both public
placement entries, the three formatters and getPositionSide. Four independent
public placement profiles fail with the pre-fix reader bodies and pass with the
canonical Promise reactions. The two reader bodies are the only scheduling
change; helper DTO/error values stay native.

The other 19 rulings concern the specific assigned async or typed-input claim,
not removal of the row's independent snapshot/alias/UTF-16 profiles. The ten
complete-class profiles in `remaining_binding_source.cjs` compare actual SDK
HTTP, results and Store for both awaited and discarded native waiters. The four
placement profiles compare final HTTP fields, request order and IDs, and also
discard the public waiter after the real Base capability prefix. Private
helpers are reached through these actual public parents, not invoked with fake
formatters. Existing #1068 rejection/held-response proof remains in the same
target. No real time advances while draining Promise work.
