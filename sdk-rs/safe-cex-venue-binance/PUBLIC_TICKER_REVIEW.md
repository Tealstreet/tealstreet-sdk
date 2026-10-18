# Native public ticker contracts

The current compiler census has seven unclassified functions in
`public_ticker_events`: the Futures and PM ticker/mark/book handlers and
`parse_field`. The two live aliases already have their own reviewed contracts.
The old JSON adapter functions disappeared with #936; no replacement decoder
or exemption is introduced here.

All seven functions consume raw event data, so every row is an executed decoder
contract. The six generic bodies run with concrete native array, capture,
lookup and update capabilities; the witness only records their callback effects.
It does not reproduce parsing, iteration or patch construction. The observations
retain ordered patch keys and binary64 bits before JSON transport, distinguish
callback error identity, and check capture-before-iteration, skipped unknown
symbols, method capture before coercion, live append/truncation and abrupt exits.
`parse_field` calls the real keyed reader and parseFloat conversion, including
missing/null values, snake/camel aliases, malformed coercion and inherited keys.
Four explicit input profiles inject actual native NaN, positive/negative infinity and
negative zero on both sides before the production call. NaNs are described by one
canonical NaN token; finite, infinite and signed-zero
values are compared by exact bits.

`public-ticker-source.cjs` executes the maintained TypeScript classes' actual
methods on constructed exchange/websocket instances. Only the terminal Store
capture/update capability is instrumented. It produces 24 cases per handler and
20 field-reader cases; the maintained runner compares them against each committed
contract before invoking the canonical compiler-authenticated module measurement.
The source oracle reads the actual `v` utility for the numeric field reader.

Separate constructed Rust `native_dependencies`/ExchangeOwner tests feed the
real public and market sockets, publish through DefaultStore, and compare every
native ticker field with the live TS source (80 Futures, 80 PM, 48 Spot cases).
`public-ticker-controls.py` independently substitutes the wrong source field in
each real handler and replaces the numeric reader's requested key. Each fault
must compile and fail its named constructor/source comparison, then pass after
exact source restoration. No fixture assertion, source floor, algorithm body,
public type or production callback is replaced by this PR.

Run from `sdk-rs`, with one Cargo build at a time:

```sh
node safe-cex-venue-binance/scripts/public-ticker-contracts.mjs
python3 safe-cex-venue-binance/scripts/public-ticker-controls.py
cargo test --locked --jobs 1 -p safe-cex-venue-binance --features native --test private_classes production_ticker_coercion_
cargo test --locked --jobs 1 -p safe-cex-exchange --all-features --test cancellable_future_inventory
```

The baseline is exact master `63a742990e`: all seven selected functions fail
missing classification by name. The same canonical module selection must pass
all nine final receipts; a measurement process exiting zero alone is insufficient.
