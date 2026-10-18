# V3 candle mapper retirement

Assigned row 12 is ACCEPT: the constructed WOO exchange never reaches
`mapWooV3Candles`. Its TypeScript definition is in `woo.v3.public.ts:347`;
the only calls are the audit test and the comparison facade in
`woo.v3.public-parity.ts`. That facade is imported only by audit tests.
The package index exports neither module, and the maintained class dependency
probe constructs two exchanges without loading any V3 module.

The Rust `map_woo_v3_candles` had only two consumers: `tests/v3_public.rs`
and `tests/v3_mapper_field_identity.rs`. No registry, FFI, venue binding or
other production Rust caller uses it. Removing the function therefore removes
its nested merge sort too. The sort's exact local-function policy entry and
three mutation-runner tails are removed in the same change.

The existing source capture remains unchanged: 42 archived mapper cases are
explicitly excluded from the retired native facade. All other 2,142 public
decoder/mapper cases remain asserted. Native field identity still checks all
28 market, 56 futures-stat and 13 orderbook cases. The independent
`WooV3KlineRow` record, its strict decoder, all fields and numeric observations
remain. The source-only NaN-sort capture does not justify keeping an unused
Rust implementation.

`fix_audit_tests::` passes before and after removal: the actual native receiver
and globals omit all 47 V3 names, while legacy methods and socket capabilities
remain installed. The maintained source probe independently checks two
constructed receivers, their dependency graph and all eight false gates.
`binding_streams::fetch_ohlcv_routes_recent_reads_to_kline_and_history_to_the_proxy_client`
also passes before and after, using the actual configured exchange and signed
client to inspect legacy recent/proxy routes, parameters and candle outputs.
These are dead-code proofs, not a claimed behavioral regression failure.

Before READY, run the full parity index, cancellation inventory, repository
format check and WOO all-target strict Clippy. No generated fixture is changed,
no production TypeScript is edited and no V3 route is enabled.
