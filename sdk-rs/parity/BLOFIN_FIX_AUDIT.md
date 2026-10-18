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
| `BlofinExchange.fetchTickers` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.fetchTickersRaw` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.getOrFetchPositionMode` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.getOrFetchMarginType` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.fetchLeverageForSymbol` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.fetchOrders` | FIXED #1068: compiled real-binding sibling-pagination RED → GREEN; both the public entry and its retry join execute. Other helper rows remain pending individual timing probes. |
| `BlofinExchange#fetchOrdersWithRetry` | FIXED #1068: compiled real-binding sibling-pagination RED → GREEN; both the public entry and its retry join execute. Other helper rows remain pending individual timing probes. |
| `BlofinExchange#fetchPaginatedOrderRows` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.fetchNormalOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.fetchAlgoOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.fetchTriggerOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.fetchHistoricalOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.cancelOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.setLeverage` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.placeOrder` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.placeOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.changePositionMode` | FIXED #1050: actual SDK three call-prefix logs and confirmed Store mode match whole-class source. Source mapping from #1064 is preserved. |
| `BlofinExchange.changeMarginType` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#formatTriggerOrder` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#placeTriggerOrder` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#formatAlgoOrder` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#formatNormalOrder` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#placeOrderBatch` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#placeAlgoOrderBatch` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#cancelNormalOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#cancelAlgoOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#cancelTriggerOrders` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange#getPositionSide` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinPositionAdapterDeps` | PENDING real-binding probe; original static finding is not a verdict. |
| `BlofinExchange.afterMethodCalledInWorker` | PENDING real-binding probe; original static finding is not a verdict. |

Remaining WEEX and candle-cache assignment rows are not classified by this ledger.
