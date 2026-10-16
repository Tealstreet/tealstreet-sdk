# MEXC Store notification callsite inventory

The MEXC Store writer migration is against the landed fallible contract. The
production crate has ten direct notifying Store mutation call sites; each
propagates its result with `?`, except the market-cap write whose source catches
and logs the failure before falling back. Exchange methods use the shared
`From<StoreMutationError> for ExchangeError` conversion. No inferred `map_err`,
stringification, discarded result, or widened signature remains at these
writers.

The three compiler-collected sites are explicit. Both `fetchAndSetOrders` and
`fetchAndSetRegularOrders` propagate a failed `BaseExchange::log` from their
`Err` match arms with `?`, so neither arm erases the returned error. The
The `mexc_reconciliation_live_store_transitions` subscriber callback increments
its counter and explicitly returns `Ok(())`, matching the fallible subscriber
contract. Other `BaseExchange::log` calls in the venue also propagate their
`Result` with `?`.

The shared `ExchangeErrorMetadata.store_subscriber_ids` field now retains
notification identities through both conversion paths. IDs remain Store-local;
the venue does not compare equal IDs from different Stores. The ticker and
position throw witness asserts the first listener's ID on each independent
Store.

| TypeScript callsite | Store operation | Rust propagation point |
| --- | --- | --- |
| `MexcExchange.fetchLeverageForSymbol` | `updateMarket`, then fallback/success `update` | `src/exchange.rs::fetch_leverage_for_symbol`; `update_market` failures are caught and logged before fallback, while `update_fetched_leverage` errors propagate. |
| `MexcExchange.updateSymbolMetadata` | Delegates to `fetchLeverageForSymbol` | `src/exchange.rs::update_symbol_metadata` propagates the delegated error. |
| `MexcExchange.fetchAndSetOrders` | Per-symbol `setOrdersForSymbol` or bulk `update` | `src/exchange.rs::fetch_and_set_orders` propagates the mutation result. |
| `MexcExchange.fetchAndSetRegularOrders` | Per-symbol `setOrdersForSymbol` or bulk `update` | `src/exchange.rs::fetch_and_set_orders` shared implementation propagates the mutation result. |
| `MexcPublicWebsocket#handleTicker` | `updateTicker` | `src/ws_public.rs::handle_ticker_frame`, returned through `on_message`. |
| `MexcPrivateWebsocket#handleOrder` | `addOrUpdateOrders`; terminal rows then `updateOrderStatus` | `src/ws_private.rs::MexcPrivateHandlers::handle_order`; an earlier write failure stops later status work. |
| `MexcPrivateWebsocket#handlePosition` | `addOrUpdatePositions` | `src/ws_private.rs::MexcPrivateHandlers::handle_position`. |
| `MexcPrivateWebsocket#applyStopLikeFrame` | `addOrUpdateOrders` | `src/ws_private.rs::MexcPrivateHandlers::apply_stop_like_frame`, shared by plan and stop handlers. |
| `MexcExchange.noteRejectedRegularOrder` | `updateOrderStatus` before rejection-error emission | `src/exchange.rs::note_rejected_regular_order`; a notification error propagates after commit and skips the later emission. |
| `MexcPrivateWebsocket#throttledFetchBalance` | `update({ balance })` after awaited `fetchBalance` | Not implemented in the Rust venue slice; remains pending. |

MEXC has no TypeScript `Store.set_setting` callsite. Existing throw witnesses
cover the committed ticker and position updates, leverage's caught market
failure/fallback, and both order snapshot methods. Order-frame and plan/stop
handler throw ordering still lack Rust witnesses. Subscriber identity is now
represented by the shared `ExchangeErrorMetadata.store_subscriber_ids` field
and asserted for ticker, position, and rejected-order failures. The rejected
order witness also pins the committed cancellation, shield, and skipped error
emission. No venue-local box or sidecar was introduced. Other notification
findings remain where multi-write or route-level ordering still lacks a
venue-specific witness.
