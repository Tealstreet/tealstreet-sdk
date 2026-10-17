# Constructed Paradex binding

`native_exchange::ParadexExchange::new` supplies the runtime, per-options HTTP
host and external public-data capability to the existing source constructor and
body ports. Production operations use the shared `js_runtime`; signing, ABI,
number, Store, HTTP and websocket primitives stay in their existing owners.
There is no venue-local radix conversion or throttle implementation.

The constructed witness is `tests/exchange_trait.rs`. Its all-sixteen test calls
start, getAccount, validateAccount, fetchPositions, fetchOrders,
fetchSymbolMaxLeverage, setLeverage, changePositionMode, changeMarginType,
placeOrder, placeOrders, updateOrder, cancelOrders, scheduleCancel,
fetchPublicTrades and fetchOHLCV. The five inherited unsupported methods preserve
the source's exact `Not implemented` rejection. fetchPositions retains its source
balance write. The single-placement override checks the caller-ID contract first;
batch placement enters through the shared validated-batch capability.

One Node capture executes the actual AST-selected SDK classes and installed
Starknet implementation. It compares native Order, ModifyOrder and JWT wire
signatures and four JWT headers, independently pins three completed comparisons,
and records Node version/major, architecture and V8. It also requires the actual
SDK to reject the invalid key used by the native authentication-failure test.
The source bodies and actual preimage values are not rewritten.

Native amendments independently observe amount and price steps 0.003 / 0.25.
The original all-sixteen case remains unchanged; a separate limit/stop supplement
checks nonzero amount 0.759, execution price 102.25, trigger price 103.25,
unchanged stop execution price and omitted limit trigger. It observes payload
fields before any signing-oracle projection. This covers amendment axes, not
all placement/bracket/IOC input combinations.

Public socket proof checks three subscriptions and actual BBO delivery through
shared leading/trailing throttle. Private proof waits for the matching auth reply
before all three private channels, then observes typed order, position and balance
fields. Position notifications are captured independently of the source's REST
refresh, which can subsequently replace the position snapshot.

The only accepted WS behavior choice is private onOpen recovery, citing
`safe-cex-exchange/WS_CONTRACT.md` and the base.ws connectAndSubscribe manifest row.
Direct authenticate retains source catch-and-log. On signing failure there is no
token HTTP submission, auth frame or private subscription; the original log and
shared recovery log occur, and reconnect closes the failed socket. Source ping
still precedes authentication. Exchange disposal writes the shared flag, then
public/private socket disposal, preserving exchange emitter listeners. Both
socket source bodies call the existing JS BaseWebSocket prototype dispose after
local timer cleanup; exchange disposal adds no BaseExchange cleanup.

`tests/native_host_paradex.rs` in the registry constructs the Rust route using
production native inputs, with the existing required host limiter argument, and
checks real loopback HTTP, venue identity, market count, independent native
precision fields and fetchMarkets Store purity. Native hosted remoteSigner is
explicitly refused pending its SDK capability binding; no local-secret fallback
is inferred. The default registry path remains module-host.

The constructed witness has eight tests, including a separate two-case Ethereum
SDK derivation/JWT comparison on distinct keys, chain IDs and account hashes.
This executes the checked-in actual SDK account derivation and unchanged source
initializer/JWT declarations; native JWT headers are observed before comparison.
The earlier full venue checkpoint passed 38 tests across 30 target summaries;
this additional Ethereum witness also passes. Strict host all-target lint and
WASM library lint pass for the venue and registry. The sequential runner
`scripts/binding-controls.py` checks each of the sixteen native entrypoint faults,
three independent amendment-axis substitutions, extra base disposal, an own
shadow disposal flag, and removal of onOpen recovery observation. Each fault
must compile and fail a named behavioral assertion, then exact restoration must
pass. It never changes reference source or expected fixture values.

```sh
SAFE_CEX_TYPESCRIPT_PATH=/path/to/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-paradex --test exchange_trait --locked --jobs 1
SAFE_CEX_TYPESCRIPT_PATH=/path/to/typescript python3 sdk-rs/safe-cex-venue-paradex/scripts/binding-controls.py
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --test native_host_paradex --test rust_venues --locked --jobs 1
```

The concrete binding admits the shared runtime value and closed native model
profiles; it does not broaden the SDK encoder or declared DTO representations.
Standalone source-oracle targets retain their independent capability-level scope.


Measured production controls: all 22 compiled faults exited 101 at their named
behavioral assertions; all exact restorations exited 0. This covers sixteen
entrypoints, three amendment axes, two disposal faults and authentication
recovery. No source reference or expected fixture was modified. Layer checks
pass 88/88. Final registry, inventory and parity results are recorded below.


After merging master da917a5808, registry construction and membership targets
pass 16/16 (one Paradex loopback witness and fifteen registry tests). Removing
only the Paradex table entry compiles and fails the named construction assertion
with exit101; exact restoration passes. The all-features override contract,
including its ignored live-source check, passes 6/6. All 18 new inventory entries
were reviewed: source member dispatches call their existing body ports, socket
disposal selects its source body, and concrete start/drop dispatch stays on the
venue. The HTTP factory reads the supplied exchange object's name as in TS,
without a direct base getter bypass or a new audit exception. Parity reports zero
findings over 7,459 rows and 17,187 public items; layers pass 88/88.


Final post-merge checks also pass eight constructed tests, strict venue host
all-target and WASM library lint, and venue/registry formatting. Global selected
trait declarations now pass 31/31 after merging master bc9f062226, which fixes
the formerly stale GRVT floor. The refreshed Paradex registry witness and
all-feature/ignored override contract pass 1/1 and 6/6 respectively. Extractor check remains zero
changed/added/orphaned manifests after the merge.

## Venue-wide cancellation entrypoints

The public Exchange trait now delegates cancelAllOrders and cancelSymbolOrders
to their existing source body ports (`paradex.exchange.ts:2225–2244`). Each
constructed empty-Store witness compiles and fails on the old trait default
(zero DELETE requests instead of one), then passes with its venue override.
The symbol-scoped wire URL is /orders?market=BTC-USD-PERP; the unscoped URL is
/orders. Both error paths are exercised with a real shared HTTP 400 response,
which must propagate through the existing source catch/log/rethrow behavior.
All 12 constructed tests and both actual-source command tests pass. This adds
cancellation entrypoint evidence; private frame/fill coverage is separate.


## Native public streams and historical reads

The constructed Exchange trait now reaches the existing venue bodies for public
book and candle listeners, fills, income, income ranges and historical orders.
The canonical shared journal reader receives the venue's branded fills axis and
symbol-scoped option; its wrapped income failure retains the original cause.
Book registration and cleanup share the same helpers as the standalone source
port, with owned callbacks for the native binding. Cleanup removes the actual
socket listener or polling timer.

Seven new `real_trait_` tests use the production constructor, scripted signed HTTP
and actual public socket frames. All seven fail against the original trait
fallbacks and pass with the hooks. The full constructed target passes 20 tests;
the standalone actual-source book oracle also passes. Wire query assertions
inspect the shared adapter's serialized URLs.

`scripts/trait-binding-controls.py` compiles independent faults in the trait
hooks, history member dispatch, fills brand, journal scope, book callback field
and candle cleanup. Two further faults empty or truncate actual returned fills.
Every control requires named behavioral failures after compilation and exact
source restoration, with all seven regressions passing afterward. It does not
change the TypeScript reference or expected fixture values.

Measured controls: all eight compiled faults fail their named regressions; every
exact restoration passes all seven tests. The cleanup witness counts serialized
query URLs and verifies that a changed post-cleanup candle is not delivered.

Strict host all-target and WASM library clippy pass. Venue-local parity passes
153 manifest rows and 508 public Rust items, retaining complete workspace target
resolution. The override inventory adds fifteen reviewed Paradex hook edges:
existing body ports, capability closures, and dynamic reads in the canonical
journal adapter; no base-hook bypass or new exemption is introduced.
