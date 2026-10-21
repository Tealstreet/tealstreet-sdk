# Unknown order cancellation audit

Scope: `CancelOrders` on all 27 Rust venue crates and their registered exchange
aliases. A nonempty ID absent from the store must produce a native cancellation
request or an explicit command error. Store absence is not proof that an order is
absent at the exchange. Empty requested lists remain valid no-ops.

The eight changed families preflight the whole requested list and report the
missing identity before any cancellation write. This preserves the known order
in a mixed known/unknown batch. The caller can reconcile and retry explicitly;
there is no inferred symbol or guessed ordinary/algo family. Symbol-wide cancel
race behavior belongs to the separate order-races lane and is unchanged here.

| Rust family and registered aliases | Absent-store ID behavior after this change | Evidence |
| --- | --- | --- |
| Binance: `binance`, `binancepm`, `binanceinverse`, `binancespot` | **Fixed:** explicit error; native family and symbol require stored order | `binance/src/trading.rs` `CancelOrders`; new `binance/tests/account.rs` regression |
| Bybit: `bybit`, `bybitdemo`, `bybitinverse`, `bybitspot`, `hyrotrader` | **Fixed:** explicit error before batch planning | `bybit/src/trading.rs` `CancelOrders`; new `bybit/tests/account.rs` regression |
| OKX: `okx`, `okxeu`, `okxdemo`, `okxeudemo` | **Fixed:** explicit error; instrument and algo family require stored order | `okx/src/trading.rs` `plan_cancel`; new `okx/tests/trading.rs` regression |
| BloFin: `blofin` | **Fixed:** explicit error; symbol and trigger provenance are required | `blofin/src/trading.rs` `cancel_targets`; new `blofin/tests/trading.rs` regression |
| Bitget: `bitget`, `bitgetusdc`, `bitgetinverse` | **Fixed:** explicit error before expanding shared native identities | `bitget/src/trading.rs` `CancelOrders`; new `bitget/tests/trading.rs` regression |
| Bitunix: `bitunix` | Explicit missing-order error before batch planning; native symbol/protection identity requires reconciliation | `bitunix/src/trading.rs` `plan_cancel`; `bitunix/tests/trading.rs` unknown/mixed and empty-list controls |
| Bitrue: `bitrue` | **Fixed:** known IDs are cancelled, then the command reports `Order … not found` for the unknown ones | `bitrue/src/trading.rs` `CancelOrders`; `bitrue/tests/trading.rs::cancellation_missing_from_store_reports_error_and_attempts_known_mixed_targets` |
| Crypto.com: `cryptocom` | **Fixed:** explicit error; ordinary and advanced requests require different paths | `cryptocom/src/trading.rs` `CancelOrders`; new `cryptocom/tests/trading.rs` regression |
| Hyperliquid: `hyperliquid`, `hyperliquidspot` | **Fixed:** explicit error; native action requires asset identity | `hyperliquid/src/trading.rs` `CancelOrders`; new `hyperliquid/tests/trading.rs` regression |
| ApeX Omni: `apexomni` | Sends `/v3/delete-order` using the supplied validated ID | `apexomni/src/trading.rs` `CancelOrders` |
| AsterDEX: `asterdex` | Explicit missing-symbol error unless a placement acknowledgement retains the symbol; symbol-aware cancellation sends directly | `asterdex/src/trading.rs` `CancelOrders`, `plan_cancel_refs`; `asterdex/tests/trading.rs` external-order cancellation cases |
| Backpack: `backpack` | Explicit `Order not found` error | `backpack/src/trading.rs` `CancelOrders` |
| BingX: `bingx` | Explicit `Order … not found` error | `bingx/src/trading.rs` `cancel` |
| BreakoutProp: `breakoutprop` | Sends supplied native ID/code, or previously acknowledged code | `breakoutprop/src/trading.rs` `cancel_steps` |
| Coinbase: `coinbase`, `coinbasespot` | Sends supplied validated IDs in native cancel batches | `coinbase/src/trading.rs` `cancel_steps` |
| GRVT: `grvt` | Explicit `Unknown GRVT order to cancel` error | `grvt/src/trading.rs` `CancelOrders` |
| Kraken: `kraken` | Sends supplied validated IDs to native cancellation | `kraken/src/trading.rs` `prepare_cancel` |
| Kraken spot: `krakenspot` | Sends supplied nonempty ID to native `CancelOrder` | `kraken/src/spot/trading.rs` `CancelOrders` |
| KuCoin futures: `kucoin` | **Fixed:** sends supplied native ID directly when uncached; later IDs continue after a rejection and final command reports the failure | `kucoin/src/trading.rs` `CancelOrders`; `kucoin/tests/trading.rs` uncached/mixed regression |
| KuCoin spot: `kucoinspot` | Explicit missing-symbol error; symbol-aware variant sends native request | `kucoin/src/spot/trading.rs` `CancelOrders` |
| Lighter: `lighter`, `lighter-robinhood` | Explicit invalid-index or missing-order error | `lighter/src/trading.rs` `CancelOrders` |
| MEXC: `mexc` | Explicit `MEXC order … not found` error | `mexc/src/trading.rs` `cancel_steps` |
| Orderly: `tealstreet-dex`, `kodiakfi`, `mode`, `salsadex`, `what`, `woofi` | Explicit reconciled-order error unless a stored child proves its native parent | `orderly/src/trading.rs` `plan_cancel` |
| Paradex: `paradex` | Sends supplied nonempty unique IDs to single/batch DELETE | `paradex/src/trading.rs` `CancelOrders` |
| Phemex: `phemex` | Explicit cached-order error; native ID may resolve from a stored virtual leg | `phemex/src/trading.rs` `cancel_plan` |
| Polymarket: `polymarket` | Sends supplied nonempty deduplicated IDs to `DELETE /orders` | `polymarket/src/trading.rs` `CancelOrders` |
| TxFlow: `txflow` | Explicit missing-symbol error; symbol-aware variant sends native request | `txflow/src/trading.rs` `CancelOrders` |
| WEEX: `weex` | Sends supplied validated native ID; virtual suffix selects protective family | `weex/src/trading.rs` `plan_cancel` |
| WOO: `woo` | Explicit `WOO order … not found` error | `woo/src/trading.rs` `plan_cancel` |

The table has separate rows for Kraken/KuCoin spot implementations inside their
parent crates; all paths refer to `sdk-rs/venues/`. Unsupported shared commands
(such as `CancelOrdersWithSymbols` on venues that cannot honor it) continue to
return explicit unsupported-command errors. Bitunix has its own registered
futures crate and is audited separately from WEEX.

## Regression evidence

Each of the eight preflight-error families has
`cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch`.
It tests both an unknown ID by itself and a valid known order followed by an
unknown ID. Both must finish with an error naming the absent ID, issue zero HTTP
writes, and preserve the known open order. The cases use authenticated scripted
startup fixtures rather than an unauthenticated early rejection.

Outside the KuCoin addendum below, no endpoint, cancel acknowledgement semantics,
terminal-order rule, cancel-all behavior, retry policy, or automatic order
replacement changes.

KuCoin addendum: symbol cancellation preserves the separate stop book; only the
empty-symbol account-wide command deletes it. The Rust SDK routes true
`cancelAllOrders()` to that command even with no stored orders, while
`cancelSymbolOrders(symbol)` retains symbol scope. Regressions cover both scopes,
stop-book failure reporting, uncached-ID direct dispatch followed by a known ID
on both successful and rejected responses, and rejection of empty identities.

## Acknowledged placements the store has not seen

A placement resolves with venue ids before the private stream or the next
reconciliation stores those orders. On venues whose cancellation needs a stored
order, an immediate cancel or amend of a returned id used to fail with the
explicit missing-order error above, and a chaser stopped mid-placement left its
order live. Such venues declare `Venue::cancel_needs_stored_order`; the engine
(`engine/src/placement_acks.rs`) then holds a `CancelOrders` or `AmendOrder`
naming an acknowledged, still-unstored id until the store has it, or for at most
10 seconds on an engine-owned timer, and dispatches it unchanged. An id that
never appears still fails explicitly at the venue; unrelated unknown ids are
never held. A stored virtual leg (`{id}_sl`), a group child whose `parent_id` is
the id ends the wait; a fill of the id shortens it to 1.5 seconds, since a
venue may drop a filled order while a partial fill's row may still follow. A
later write to a held order waits behind it, so writes to one order keep their
arrival order. Opted in: Binance,
Bybit, Crypto.com, Hyperliquid, Backpack, BingX, GRVT, Lighter, MEXC, Orderly,
TxFlow, WOO. BloFin, Bitget, Bitrue, OKX and Phemex route acknowledged ids from
their own acknowledgement caches; AsterDEX and KuCoin spot cancel with symbols;
the remaining venues send the supplied id directly.
