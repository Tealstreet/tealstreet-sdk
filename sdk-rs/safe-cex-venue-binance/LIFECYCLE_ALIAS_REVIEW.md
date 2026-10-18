# History, API and lifecycle capabilities

Exactly twenty public aliases in `rest_history`, `api_factory` and `lifecycle`
are covered here. `acceptedAliasContext` authenticates their actual compiler
declarations, expanded targets, external artifacts, macro origins and manifest
claims. The finite runner supplies no replacement compiler identities or flags.
Each alias has an individual source-role record. Actual native Deserialize and
conversion probes decide whether its concrete type admits raw data.

These aliases carry executable callbacks, pending work or cleanup. Their bodies
consume separately decoded values. Classifying a capability does not exempt its
input or result values from decoder coverage.

The baseline command `scripts/lifecycle-alias-contracts.mjs --expect-unclassified`
requires twenty missing-contract refusals by name. The ordinary command requires
twenty native results without failures. Compiler snapshots and receipts remain
under `target/`.

The native witnesses in `tests/support/lifecycle_aliases.rs` construct actual
`native_dependencies` and the production owner. Only terminal HTTP, socket I/O,
the remote signer and runtime clock are controlled. Venue methods, pagination,
Store, signing, interceptors and callback bridges retain their implementations.

| Aliases | Native observation |
| --- | --- |
| `HistoryFuture`, `HistoryGet` | Held HTTP remains pending; release preserves execution identity, price and signed-zero quantity. |
| `PageCall`, `PageResolver` | The actual paginator resolves the current owner method, forwards each range/type query and returns transaction identity and income bits. |
| `HistoryLog`, `HistoryEmit` | Malformed execution/order rows preserve distinct error-log and error-event effects, severity and event arity. |
| `CandleCallback`, `CandleListen` | The same consumer receives the seed and a subsequent actual companion-socket kline. |
| `CandleFetch`, `SeedFuture` | The real OHLCV seed uses `limit=1`, waits for held HTTP and reports mapping failures through the debug logger. |
| `CandleCleanup` | Cleanup detaches live delivery and suppresses the result of a seed already running. |
| `ApiFuture`, `RequestInterceptor` | Real signed income HTTP preserves the symbol and has an independently checked HMAC. |
| `ResponseInterceptor` | Success and negative-code responses traverse the actual pipeline; rejection retains response data and emits once. |
| `RemoteSignCall` | The native host forwards account/auth payload, waits for its held host and returns the released signature. |
| `LifecycleCall`, `LifecycleAsyncCall`, `LifecycleFuture` | Actual tick settlement updates native Store balance bits, loaded flags and reconciliation time. |
| `TickResolver`, `TickTimer` | The actual timer resolves another tick at fire time; account and risk refresh intervals stay independent. |

The income-range witness additionally exposed JSON loss in two concrete native
consumer seams. `income_records` stringified numeric income before typed
admission; `native_host::paginate` projected the result through serde JSON.
Direct native typed conversion now preserves signed zero, NaN and infinities.
The existing numeric transaction/trade ID to String policy is preserved on
copied rows. The actual maintained `_fetchIncomeHistoryPage`,
`fetchIncomeHistoryRange` and shared `paginateIncomeRange` form the source oracle;
all four numeric profiles retain their original requests and binary64 values.
NaN is observed by numeric kind; every other numeric value is compared by bits.

`scripts/lifecycle-alias-controls.py` disconnects all twenty production dataflows
separately. Each must compile and fail its named native assertion, then pass after
exact byte restoration. Compiler errors and unrelated failures do not count.
The income input/output JSON controls independently restore each old boundary.

Run from `sdk-rs`, serially:

```sh
node safe-cex-venue-binance/scripts/lifecycle-alias-contracts.mjs
cargo test --locked -p safe-cex-venue-binance --features native --jobs 1 --test private_classes lifecycle_aliases::
python3 safe-cex-venue-binance/scripts/lifecycle-alias-controls.py
```

This finite native/default measurement does not establish arbitrary callback
implementations, executable getters, other feature combinations, or all paths
through these modules. Other alias cohorts retain their own records/witnesses.
