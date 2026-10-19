# Comparator review

The REST `fetchOrderBook` binding parses string prices with `parseFloat`, so NaN
reaches both subtraction comparators in `exchange_helpers::transform_order_book`.
Rust slice sorting can panic on these inputs. Both sides now delegate to shared
`safe_cex_num::sorting::stable_sort`, preserving TS subtraction and NaN-as-equality.
Cumulative quote totals still run only after each side is completely sorted.

The two `native_exchange::fix_audit_tests::rest_book_*` regressions construct the
production venue with NativeRuntime. Only terminal HTTP responses are scripted;
the actual member, API factory, response transform and depth mapper execute.
`tests/book-sort-source.cjs` extracts maintained `fetchOrderBook` and its endpoint
helper, using CI Node 24.20.0 / V8 13.6. Ten profiles have 22, 32, 63, 64, 65, 95,
127, 128, 129 and 257 levels per side. Unique amounts pin row identity, while
native number bits pin prices, quantities and all cumulative quote totals.

With the original Rust helper, both named tests compile and reach the production
"comparison function does not correctly implement a total order" panic. Their
bounded await subsequently fails instead of hanging after the runtime task dies.
The unchanged fixtures pass with canonical sorting.

The other four crate sites were reviewed together:

| Site | Reason the existing comparator is total |
| --- | --- |
| `helpers::normalize_aster_dex_realized_pnl_deltas` bucket | Nonfinite time/id keys become zero; original index breaks ties. |
| `ws_helpers::public_depth_snapshot` bids | Admission requires finite, positive prices before sorting. |
| `ws_helpers::public_depth_snapshot` asks | Same finite-price admission. |
| `api_helpers` signing keys | Lexicographic UTF-16 string comparison. |

The three unchanged nested exchange helpers retain their existing classifications.
Their whole-file pins are refreshed after reviewing the sole production change.
The `levels` decoder helper keeps its real runtime-owner obligation and now runs
the canonical sorter rather than Rust slice sorting. No scanner or family rule
changes are needed.
