# Retired delegation extractions

The standalone `delegated_methods` module contained six forwarding APIs and
`CandleOptionsHandle`. After the dead message-entry and public-candle-delegate
tail was removed, a tracked Rust search across the SDK found only the definitions,
the module declaration and `tests/delegated_methods.rs`. No composed native,
registry, FFI/NAPI or wasm export called those generic APIs. Their standalone
oracle replaced the venue operations with supplied callbacks. Remove that module,
its test/oracle/control and its three historical receipts; preserve the actual
class routes and all six TypeScript symbols.

The live paths are `ExchangeState::listen_order_book` (Futures, PM and Spot),
`ExchangeState::listen_ohlcv` (PM and Spot delegation), and the
`Method::HistoricalPositions` arm of `ExchangeState::invoke`. The latter calls
`NativeImports::fold_closed_positions` with `symbol_scoped = true`, which uses
the shared folded-position implementation and real income/fills requests.
The source counterparts are the five listener fields in the three maintained
exchange classes and `BinanceExchange.fetchHistoricalPositions`.

The native `private_classes` target keeps these executed witnesses:

- `production_native_candle_delegation_uses_market_socket_and_cleanup` constructs
  all three actual venues, delivers candle frames and checks returned cleanup.
- `public_listener_aliases::production_native_public_listener_aliases_wait_buffer_reuse_and_unsubscribe`
  constructs all three venues and checks snapshot waiting, native depth updates,
  reference reuse and unsubscribe through the real public sockets.
- `delegated_native_routes::production_native_historical_positions_keeps_symbol_scoped_fold`
  constructs Futures with `native_dependencies`, supplies a nonempty income row
  at the terminal HTTP adapter, and checks signed income and symbol-scoped fills
  requests through the actual method. Independent virtual-clock advancement
  drives the real request limiter; no fold callback is replaced.

`scripts/delegated-native-control.py` changes only the live historical dispatch's
`symbol_scoped` argument to false. The native test must compile and fail its named
missing-fills assertion, then pass after exact source restoration. The unchanged
candle and listener controls belong to their existing route cohorts.

The package command, `rust-ci-wiring.test.mjs` and `rust.yml` have no explicit
`delegated_methods` target selector. Historical broad-run inventories remain
bound to their original gate SHAs, rather than being rewritten as current runs.

Before deletion, the canonical accepted-set module measurement must expose the
three missing order-book function contracts and the missing options alias.
After deletion there are no owners in that module: the authentic full compiler
inventory establishes their absence. An absent `--module` selector correctly
fails to match; it is not an executed zero-test success or a decoder waiver.
Run the retained native witnesses separately. No classification is added for
a dead helper and no TypeScript manifest status is promoted by this cleanup.

## Checked retirement

The canonical module measurement before deletion (source `bab9149238`) executes
exactly four named missing-contract failures: the three book wrappers and the
options alias. The full compiler inventory after deletion scans all 197 decoder
owners and contains no retired-module reference. At `c36a7d0a48`, every other
owner name is unchanged; the refreshed `02106979e4` inventory independently
confirms absence against the newer shared runtime. No decoder classification is
invented for removed code. The final master/#962 merges change only contract
metadata, review-note ordering and docs from that tested production tree.

The refreshed native candle, depth and history witnesses pass. The historical
flag fault produces the named compiled wire failure and passes after exact
restoration; strict native Clippy, fmt, extractor, full supplied-manifest and
all-features cancellation checks also pass. This is scoped native evidence,
not a claim for every venue operation or alternate host.
