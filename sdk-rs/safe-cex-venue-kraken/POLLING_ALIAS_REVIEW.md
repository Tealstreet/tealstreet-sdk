# Kraken polling continuation aliases

Four reviewed profiles fix `ohlcv_polling::{PollFuture, PollCallback,
CandleCallback, Cleanup}` at the actual `binding::rt::Rt` Scope host.
`PollFuture` is `Pin<Box<dyn Future<Output = Result<(), Thrown>>>>`;
`PollCallback` is `Rc<dyn Fn() -> PollFuture<Rt>>`; `CandleCallback` takes the
original `Rc<Js>` candle and returns `Result<(), Thrown>`; `Cleanup` takes no
arguments and returns that unit result. They are owning continuation/delivery/
cancellation capabilities, not wire decoders. Expected Rust types are authored
independently of the compiler AST; no other host instantiation is classified.

The live reference is `kraken.exchange.ts::listenOHLCV`: immediately start a
poll, read the last two candles up to Date.now, emit the latest only when its
timestamp/close/volume differs, cache that original cell after the callback,
and schedule another poll in 5000ms if still active. Cleanup disables the key,
clears its timeout and deletes the candle cache. The Rust production
`ohlcv_polling::listen_ohlcv` and concrete `PollHost` implement those operations.

`tests/support/accepted_polling_aliases.rs` invokes this production polling
function with the actual Scope of a constructed KrakenExchange. Only terminal
HTTP/runtime are fixtures; the chart client, options conversion, callback
consumer and timer/map host remain production implementations. Four named
witnesses observe held HTTP completion and native candle fields, the exact
five-second timer plus duplicate suppression, changed candle delivery and
callback/cache identity with UTF16 consumer mutation, and cleanup removing
cache/timer state with no subsequent REST request. This does not claim cleanup
cancels an already-started HTTP request, nor cover the separate public typed
listener or Js dispatcher wrapper.

`scripts/polling-alias-contracts.mjs` uses maintained `acceptedAliasContext`
to obtain fresh compiler artifacts, authenticated external roots, macro origins
and declaration claims. It executes the stock finite native identity/Serde/
conversion probes. Its negative mode requires all four actual missing-contract
receipts. Classification is counted separately from production decoder results.

`scripts/polling-alias-controls.py` serially corrupts four live consumers:
initial PollFuture dispatch, timer delay, delivered candle cell, and cleanup
cache removal. Each must compile and fail its exact named runtime witness.
The runner restores exact source bytes in `finally`, then requires a passing
named test. Compiler errors are rejected as negative evidence. No production
body, shared discovery guard or exemption changes persist in this cohort.

Commands from `sdk-rs`, one Cargo build at a time:

```sh
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/polling-alias-contracts.mjs --expect-unclassified
cargo test --locked -p safe-cex-venue-kraken --jobs 1 --lib accepted_polling_aliases
python3 safe-cex-venue-kraken/scripts/polling-alias-controls.py
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/polling-alias-contracts.mjs
RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR="$PWD/target/accepted-inventory" node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-kraken --inventory --jobs=1
```

Discovery remains in the separate accepted-inventory target. Readiness additionally
requires the full parity index, extractor check, all-features cancellation and
override gates, plus fmt --check and all-target Clippy for Kraken and parity.
Following aliases remain separate obligations; this cohort is not whole-Kraken
accepted-inventory completion.

The selected canonical replay reports four explicit classifications, zero failed
owners and zero decoder runtime results. All four constructed-Scope witnesses
and all four named compiled semantic controls pass after exact restoration,
including a repeat after inheriting the replay-clock master merge. The full
Kraken inventory clears these profiles and advances to `classes::BodyFuture`,
which needs its own finite profile. Thirteen of the original 24 aliases remain;
this result does not classify them or assert full Kraken inventory success.
