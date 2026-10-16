# Concrete Exchange binding

`native_exchange::AsterDEXExchange::new` assembles the existing constructor,
interceptors, REST workflows, clock and streams on `safe_cex_exchange::js_runtime`.
HTTP selection receives the exact primary/secondary options. The registry's
production host routes the final signed request through the common HTTP stack.

`tests/exchange_trait.rs` constructs the concrete exchange against a scripted
transport. Its all-sixteen witness calls start, get_account, validate_account,
fetch_positions, fetch_orders, fetch_markets, fetch_tickers,
fetch_symbol_max_leverage, set_leverage, change_position_mode,
change_margin_type, place_order, update_order, cancel_orders, schedule_cancel
and fetch_public_trades. It observes REST effects, decoded values and precision,
loaded Store flags and native socket startup. Separate witnesses cover secondary
read credentials/transport options, V1 HMAC, V3 ABI/code signing, auth-before-dial,
caller-ID preflight and one signature-rejection resend for both HTTP 200 and 400.

On 2026-10-16 each of the sixteen production trait overrides was independently
mutated to return a synthetic error before dispatch; schedule_cancel instead
returned success. For every mutation the all-sixteen witness exited 101 with
`test result: FAILED`, and no compilation failure. All sixteen were caught and
the exact production bytes restored before the passing gate. The controls do
not replace the source-oracle mutation witnesses already in the crate.

The complete scoped source-oracle run passed 186 tests in 23 targets, including
the source mutation witnesses. Native registry HTTP and constructor tests passed
13/13. The shared PlaceOrderOpts witness now observes impliedVolatility too
(325 measured source cases, 52 accepted, 18 fields).

Scoped commands (from repository root):

```sh
cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-asterdex --test exchange_trait --jobs 1
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-asterdex --all-targets --jobs 1 -- --include-ignored
cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --test native_host_asterdex --test rust_venues --jobs 1
```

The source inherits `BaseExchange.scheduleCancel`'s exact `Not implemented`
rejection (base.ts:989). Public trades preserve the existing declared refusal
of the source's unchecked uppercase BUY/SELL cast into the closed lower-case
OrderSide model. Representation costs remain in the owning manifest rows and
[canonical boundaries](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary).
