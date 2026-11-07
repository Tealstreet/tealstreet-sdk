# Bybit terminal order history

Source: **Bybit Tech Bot API announcement, effective 2026-11-12**, supplied
verbatim by Tealstreet ops on 2026-11-07. Its affected endpoints are
`GET /v5/order/history` and `GET /v5/spread/order/history`:

> When the `orderStatus` parameter is not passed:
>
> - Before: The endpoints return `Cancelled`, `Rejected`, and `Deactivated` orders from the most recent 24 hours.
> - After: `Cancelled`, `Rejected`, and `Deactivated` orders will no longer be returned.

> When `orderStatus` is specified, the endpoints will continue to return matching orders from the most recent 24 hours for `Cancelled`, `Rejected`, and `Deactivated` orders.

The [v5 changelog](https://bybit-exchange.github.io/docs/changelog/v5) checked
during investigation did not contain this notice. The [standard history
page](https://bybit-exchange.github.io/docs/v5/order/order-list) documents a
single string `orderStatus`, so query each status separately. The [spread
history page](https://bybit-exchange.github.io/docs/v5/spread/trade/order-history)
does not document `orderStatus` as a request parameter. There are no spread
history callers in either stack; no request or assumed filter support is added
for it. There was no live or authenticated probing.

## Caller census and reconciliation

Paths below are relative to `packages/safe-cex/src/__tests__/reference/bybit/`
for TS and `sdk-rs/venues/bybit/src/` for Rust. TS venue implementations are
retained only as test/replay references; SDK factories use Rust.

| Caller                                                                    | Scope                                                                                                     | Action                                                                                                                                 |
| ------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| TS `bybit.exchange.ts` / `fetchHistoricalOrders`                          | Linear and opted-in inverse; inherited by demo and Hyro                                                   | Default plus three status walks; merge/dedupe by order ID before global newest-first limit                                             |
| TS `bybit.exchange.ts` / `fetchExecutionsFallbackFromOrderHistory`        | Linear/demo/Hyro and inverse symbols                                                                      | Unchanged: maps only rows whose status contains `filled`; never depends on the three disappearing statuses                             |
| TS `bybitinverse.exchange.ts` / `fetchExecutionsFallbackFromOrderHistory` | Dedicated inverse                                                                                         | Same filled-only predicate; unchanged                                                                                                  |
| TS `hyrotrader.order-recovery.ts` / `findOrder`                           | Hyro lost-create reconciliation                                                                           | Realtime, default history, then separate terminal-status walks, each with its own cursor guard                                         |
| Rust `history.rs` / `start_query` → `next_window` / `request_page`        | `bybit`, `bybitinverse`, `bybitspot`, `bybitdemo`, `hyrotrader`; enabled linear/inverse/option categories | Already fixed in SDK #3072; retain default plus three status axes, requested windows and independent cursors, global dedupe/sort/limit |
| Rust `hyro.rs` / `Lookup`, driven by `trading.rs`                         | Hyro lost-create reconciliation                                                                           | Already fixed in SDK #3070; retain realtime, default and three filtered lookup stages                                                  |

The shared TS API factory signs/transmits caller parameters without adding
history reads. `bybitspot.types.ts` declares `USER_ORDERS`, but its standalone
TS spot class has no caller. Standalone TS inverse/spot do not override
`fetchHistoricalOrders`; this change does not introduce a new history API for
those references. Rust already implements their order history. Ordinary
reconciliation/cancel confirmation uses realtime or private-stream evidence,
not history; no history query needs adding there.

No `sdk-rs/parity/` directory or Bybit method mapping exists in this mirror.
This census reconciles the reference methods with the current native walkers;
no generated parity data or result mappings are regenerated. Existing status
mappings remain unchanged, including closed versus canceled distinctions.

## Regression evidence

- `bybit.order-history-status.test.ts`: seven regressions proved failing before
  the reference fix. Covers filtered-only rows, independent status/category/
  window cursors, order-ID dedupe, global limiting, one-sided bounds, and Hyro
  terminal identity recovery without resubmission.
- `private-tape-bybit.mjs --history-only`: a separate strict CI gate executes
  `fetchOrderHistory` with explicit expected results. Unfiltered history has
  only a filled row; all three other statuses exist only in filtered pages.
  Both replay CI jobs require this case. The existing lifecycle tape and its
  known-failure declarations remain byte-for-byte unchanged.
  Each filtered walk repeats its row on a second page using the same cursor
  token across statuses. TS/Rust must return each order exactly once.
- `private-tape-bybit.test.mjs`: guards command/expected-result coverage and
  status/cursor identity in the tape matcher.
- Rust's existing `account.rs` terminal-history regressions cover independent
  cursors, dedupe, requested windows, global limits and filtered-page failures;
  `trading_variants.rs` covers the Hyro lookup stages.

Scope: deterministic tests and local replay only; no trading, keys, deployment
or authenticated exchange checks.
