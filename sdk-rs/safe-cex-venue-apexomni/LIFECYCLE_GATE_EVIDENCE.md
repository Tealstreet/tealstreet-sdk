# ApeX Omni lifecycle and signature bodies

This slice moves **10 pending rows to divergent**, with no rows moved to ported.
The independent parent is `1181108d2dfeee7dd6cb1d21a3a39922e1e13b9e`:
27 ported / 102 divergent / 39 pending. The slice leaves
27 ported / 112 divergent / 29 pending, of 168 rows. Python 3.11 `tomllib`
compares manifest symbols by `(ts, kind)`; the count excludes unchanged rows.
The branch began directly at that merged integration head, without another
unreviewed lane as an ancestor.

Implementation checkpoints are `8a685d42e2` and
`00540b4db3f54f5a6513d2ac5e6458d5461f6134`. The latter adds the class signature
wrapper and separates the imported dayjs clock from the virtual exchange clock.
The manifest writer ran after each mapping/provenance edit. The mapped target
file is `safe-cex-venue-apexomni/src/lifecycle.rs`; all ten methods have actual
Rust bodies, composing shared exchange, Store, number and signer implementations.
`ApexServices` is an explicit required imported-capability interface, not a
substitute implementation for any pending venue method.

## Bucket justification for the ten moved rows

All ten are **CHOSEN**. These are direct boundary decisions for this slice,
independent of classifications in earlier ApeX slices. The shared string
boundary is defined at
`sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary`.

| Source method | Reason for CHOSEN |
| --- | --- |
| `validateAccount` | Typed validation strings and shared errors replace arbitrary thrown JS values and non-string response messages. Unsupported truthy message types are refused visibly. Shared module SDK initialization and signer construction stay with the wallet host, avoiding a separate JS error heap and wallet lifecycle. |
| `start` | Shared Store/Market/Ticker records and polled concurrent futures replace mutable record aliases and eager Promise continuations. Public-data initialization and socket factories stay with their host. The first tick is owned background work; startup does not await it. A separate JS scheduler would detach this venue from the SDK runtime. |
| `fetchAndSetPrivateSnapshot` | The real wrapper polls its helper future rather than creating an eager JS Promise. It preserves delegation and shared errors while retaining one runtime executor. |
| `fetchAndSetPrivateSnapshotHelper` | Shared Position/Order/Balance records and polled concurrent reads replace mutable JS aliases and Promise scheduling. Native optional None collapses absent and explicit undefined, so the oracle projects optional model fields explicitly. The source WebSocket cutoff and merge priority remain in the body; a second record heap would bypass shared Store ownership. |
| `tick` | A host loop capability owns the next future and interval instead of ambient JS timers/Promise scheduling. The source guard and snapshot/error/interval sequence remain in the body; another scheduler would split runtime ownership. |
| `fetchOHLCV` | Shared options, Timeframe and Candle records replace arbitrary JS option objects and mutable candle aliases. Imported dayjs clock, Date TimeClip and calendar subtraction stay with the host; this preserves the distinction from the virtual exchange clock without introducing another calendar environment. |
| `listenOHLCV` | Native callbacks/cleanup return Result and owned Candle values instead of JS void callbacks, arbitrary thrown values and shared mutable references. Options, callback invocation and cleanup are delegated directly to the public socket capability; a separate callback heap would split socket ownership. |
| `listenOrderBook` | Native callbacks/cleanup return Result and owned OrderBook values instead of JS void callbacks, arbitrary thrown values and mutable aliases. Symbol, callback invocation and cleanup remain direct delegation through the public socket capability. |
| `signOrder` | The source payload:any and SDK signer become the existing typed SignContractPayload and opaque wallet capabilities. The unused receiver is elided into an associated function, prefix stays ignored, and the body calls the real native sign_contract before strict signature property access. A second JS payload/wallet heap would duplicate SDK ownership. |
| `setLeverage` | Shared typed Market/Position values and polled requests replace unrestricted JS records, aliases and eager Promise scheduling. Missing state is an explicit error; shared Math extrema preserve NaN/signed zero and shared toFixed formats the rate. Keeping one Store/transport model avoids parallel ownership. |

## Native comparisons

The lifecycle corpus runs the unchanged actual TypeScript class declarations,
actual TypeScript DefaultStore and actual merge helper, with controlled imported
runtime capabilities. It has **28 independently expected source cases**:
three per method for nine lifecycle methods, with a fourth fetchOHLCV case.
That fourth case places Date TimeClip before sub-millisecond subtraction;
another case covers the dayjs clock fallback. Native comparisons assert
result fields, enum tags, Store state, exact notification counts, request and
callback/cleanup operations, and loop counts before any wire encoding.

Optional Source model properties use the actual omitUndefined helper for
comparison with native Option fields. This projection does not claim JS
property presence, object identity or insertion order. Request values retain
their undefined tags. The native HTTP fixture yields at await boundaries;
the source corpus drains background Promise work before capturing final state.
These comparisons cover that paired host profile, not every JS microtask/native
Future interleaving or every concurrent rejection/disposal race.

The signature corpus retains its 30 transaction cases and adds **seven independently
expected class-wrapper cases**. The actual TypeScript field initializer calls
the actual source signContract function. Native comparisons cover result kind
and value, error name/message and ordered SDK argument values, including missing
signature metadata. They do not independently prove imported SDK/WASM cryptography.

The initial Store mock replaced positions and was an incorrect oracle instrument:
the real Store retains missing positions as closed records. Replacing it with
the actual TypeScript Store fixed the comparison. Production balance/loaded
writes also use one atomic Store update, matching the source notification count.
No failed comparison was silenced or converted into a ported claim.

## Gate scope

Workspace compilation, full suites, host clippy, wasm clippy and live HTTP/WS
execution were not run or claimed. The global mapping checker was attempted:
it exited 1 on the unmodified `safe-cex-webworker/src/twap_manager.rs` item macro
`thread_local`, which it cannot index without expansion. It stops there before
enumerating this slice's mappings; a successful global mapping audit is not claimed.

Measured at `00540b4db3f54f5a6513d2ac5e6458d5461f6134`:

| Gate | Measured result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-apexomni --all-targets --jobs 2` | exit 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-apexomni --test exchange --jobs 2 -- --test-threads=1` | exit 0; 27 passed, including all nine lifecycle tests |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-apexomni --test signing --jobs 2 -- --test-threads=1` | exit 0; 7 passed, including the class wrapper |
| Seven lifecycle leaf body controls | compiled; exit 101; each named native assertion failed |
| Two lifecycle caller body controls, separate from their helper controls | compiled; exit 101; each named native assertion failed |
| Class-wrapper result-shape control | compiled; exit 101; failed `exchange signOrder native result` |
| Restore all controls, then both scoped targets above | exit 0; 27 exchange + 7 signing passed |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | exit 0; would update 0 manifests, 0 added, 0 orphaned |
| Independent TOML census and baseline-delta assertions | exit 0; 27 / 112 / 29 and exactly 10 moved to divergent |

Oracle invocations set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
The maintained `scripts/mutate_lifecycle.py` checks the compiler-success/failure
output and named assertions, restores production text in finally, then requires
both restored scoped targets to pass. No control survived or was dropped.
Ignored logs are under `sdk-rs/target/apexomni-controls/`; this committed sheet
is the reviewable evidence, including the failed global-tool claim above.
