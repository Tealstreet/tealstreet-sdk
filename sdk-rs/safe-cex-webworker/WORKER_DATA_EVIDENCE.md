# Native callMethod and candle DATA posts

`CallMethodHandler` previously serialized results into `serde_json::Value`, and
`OHLCVSubscriptionsHandler` used `json!(candle)`. Three constructed-handler
regressions failed on those bodies: native candle NaN/infinities became null,
and own undefined order properties disappeared. The committed regression
checkpoint is `64403282c1`; its tests use actual handlers and an injected Exchange,
runtime and post sink, rather than replacement handler implementations.

The replacement requires `WorkerDataPost`. `WorkerDataPayload` carries the
existing runtime `Js` and deliberately has no Serialize implementation. Typed
results use the existing `from_typed` encoder, backed by canonical retained
numeric capture. Candle subscription metadata is still incoming JSON; the Candle
is captured separately before the post. Assigned void results retain own
undefined; unknown methods omit result. The native Error and order-book post
capabilities and shared Candle model are unchanged.

`tests/data_posts.rs` inspects native values and own properties before any JSON
trace conversion. It requires both candle rows before walking twelve numeric
fields, distinguishes absent/undefined order properties, and checks signed zero
and nonfinite order numbers. Five native tests additionally compare seven cases
from actual TypeScript BaseHandler, CallMethodHandler, ThrottleManager and
OHLCVSubscriptionsHandler classes. The source post uses actual structuredClone
before observing numbers and own undefined. The independent source census is
seven cases; each native comparison requires exactly one matching source case.
Both successful and unknown-method post rejection enter the source error catch,
preserving the original error name, message, stack and code.

Run the native proof and its five compiled controls from `sdk-rs`, with explicit
`SAFE_CEX_TYPESCRIPT_PATH` and `SAFE_CEX_WORKER_LODASH_PATH` dependency paths:

```sh
cargo test --locked -p safe-cex-webworker --test data_posts --jobs 1
python3 safe-cex-webworker/scripts/data-post-controls.py
```

The independent faults restore result JSON projection, truncate candles to one
row, replace assigned void with null, ignore post rejection, and restore candle
JSON projection. Every fault compiles and fails its named runtime assertion;
every exact byte restoration passes all five tests. Run controls exclusively:
they temporarily edit production files and invoke Cargo sequentially.

The concrete browser adapter materializes direct JS values with js-sys APIs,
then invokes the required host post callback. `examples/worker_data_host.rs` and
`tests/worker-data-host.cjs` execute the actual WASM adapter and actual receiving
structuredClone. They inspect NaN, both infinities, signed zero, own undefined,
BigInt, booleans/null, lone UTF-16 key/value units, own __proto__ data, shared
references, cycles, Map/Set identity and callback failure. The receiver detaches
from the sender while preserving graph relationships and values.

```sh
python3 safe-cex-webworker/scripts/data-host-controls.py
```

This command builds the actual WASM example, uses the installed matching
wasm-bindgen CLI and runs Node. Three independent browser faults replace
undefined with null, nonfinite numbers with null, and negative zero with positive
zero. All three compile, fail named Node assertions and pass after exact source
restoration. No JSON-serialized stand-in is used for the browser witness.

The browser adapter covers ordinary data graphs, not arbitrary proxy/getter
execution timing or opaque runtime capabilities. Functions, promises, symbols
and opaque builtin handles refuse at this host boundary. Typed Exchange models
still impose their existing owned-object, enum and string boundaries. This work
does not widen shared Candle, incoming commands, Store models or other ordinary
JSON worker channels. Legacy finite trace tests explicitly opt into the separate
test-only `support/data_trace.rs`; native DATA tests bypass that lossy adapter.

This is the outgoing worker sender boundary. The main-thread Exchange RPC
`call_method_in_worker` result and request-argument APIs retain their current
Value ABI and require a separate consumer cutover. The local emitter/code
carrier work is owned separately; this change does not claim that ABI is widened.
