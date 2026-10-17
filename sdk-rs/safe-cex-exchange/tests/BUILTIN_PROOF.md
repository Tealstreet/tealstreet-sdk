# Shared builtin production witnesses

The Node oracle in `js_builtins_oracle.cjs` checks array holes/mutation,
420 installed-Dayjs chains, own-property lists and JSON property selection.
`js_builtins_native` additionally uses the production `NativeRuntime`, native
HTTP host, Store and installed global tables without a replacement runtime.

The following scoped tests compiled and failed with the four shared runtime
builtin files taken unchanged from master `895a092364`, then passed after exact
restoration of the fixed files:

| Crate / test target | Named witness | Master failure |
| --- | --- | --- |
| `safe-cex-exchange / js_builtins_native` | `native_host_executes_source_array_dayjs_property_and_console_calls` | `object.pop is not a function` |
| `safe-cex-venue-lighter / exchange_binding` | `place_order_merges_a_sub_minimum_trailing_lot_into_the_previous_lot` | placement rejects before expected signed lots |
| `safe-cex-venue-orderly / exchange_trait` | `fetch_ohlcv_returns_source_rows_reversed_oldest_first` | missing reverse rejects candle history |
| `safe-cex-venue-bitrue / exchange_binding` | `listen_ohlcv_maps_singapore_kline_times_through_the_real_dayjs_global` | no candle callbacks instead of two Singapore-adjusted rows |
| `safe-cex-venue-phemex / exchange_binding` | `bound_daily_chart_history_ends_at_previous_utc_midnight` | missing Dayjs daily-boundary operation yields no candle rows |

Each venue witness constructs its production binding and invokes the production
body, with deterministic transport inputs; no helper-name stub supplies these
builtins. Run an individual witness with `cargo test --manifest-path
sdk-rs/Cargo.toml -p <crate> --test <target> <name>`. The native core witness
requires `--all-features`. No workspace test run is needed.

Two further Dayjs regressions use the same production NativeRuntime and host:
32 offset-string/coercion cases and 576 start/end boundary cases. Both compiled
and failed against the earlier #453 implementation (numeric-only refusal and
ignored false flag), then passed after the repair. Opaque offset inputs keep
their original reference; invalid offset strings return the original instance.
The current-master library control also fails both tests before exact restoration.
