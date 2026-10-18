# Host boundary probe

`yarn ffi:smoke:node` builds the addon, runs the Node account regressions, and
calls `parity_probe` and `version` from Bun.
`yarn ffi:smoke:web` builds wasm and calls the same exports in a real browser
module Worker, driven by the headless Puppeteer approach used by
[the escape suite](../../apps/module-frame/escape/run.mjs).

Features default to empty. `node` enables napi-rs and its native build setup;
`wasm` enables wasm-bindgen. They are mutually exclusive: build each separately,
never with `--all-features`. The SDK dependency remains present in both builds
so future wasm-hostile dependencies fail this gate early.

Wasm has no threads here: use a single-threaded executor. This matches safe-cex's
existing one-WebWorker-per-account model in [plan §7](../../docs/plans/SAFE_CEX_RUST_PORT_v1.md).
Browsers cannot set custom `Origin`, `User-Agent`, or `sec-fetch-*` headers or
open raw sockets. These are existing TypeScript runtime constraints enforced
by `isNodeOrBunRuntime()`, rather than new Rust restrictions; see
[the shared safe-cex guidance](../../packages/safe-cex/CLAUDE.md).

Prerequisites: the workspace Rust toolchain, Bun, Node/Yarn dependencies and
Puppeteer's Chrome (`yarn workspace tealstreet-module-frame exec puppeteer browsers install chrome`).
Run `yarn ffi:setup` to install the wasm-bindgen CLI version from `sdk-rs/Cargo.lock`
into `$CARGO_HOME/bin` (default `~/.cargo/bin`). Local builds and CI use that
same location explicitly; setup is a no-op when the matching tool is present.
Bun must already be on PATH; the native smoke names it and its installation
instructions if missing. Setup does not install Bun.
`CARGO` can specify the Cargo executable. Build output is ignored under `dist/`;
both smokes rebuild before executing and fail on errors/timeouts.

`tests/native-account-placement.test.mjs` exercises client-ID minting/placement,
option capability errors and nuke dispatch through the real Node addon, using a
loopback public-data gateway and HTTP server. Run it after
`node sdk-rs/safe-cex-ffi/scripts/build.mjs node` with
`node --test sdk-rs/safe-cex-ffi/tests/native-account-placement.test.mjs`.
The matching scoped Rust suite is
`cargo test -p safe-cex-registry --test native_account_placement` from `sdk-rs`.

With the `node` feature, `nativeAccountOpen`, `nativeAccountCallMethod`,
`nativeAccountSignerAvailable` and `nativeAccountClose` expose the registry's
owned-value account host. Events and worker Store payloads are forwarded to JS; signing
and proxy tokens remain host callbacks. `nativeAccountSignerAvailable(id, available)`
returns a Promise that resolves after the host applies the availability change.
Await it before the next operation; released handles reject with `AccountClosedError`
and never-issued handles with the existing missing-account error. `nativeAccountClose` stops forwarding
immediately, then resolves after disposal completes, or rejects with its error.
Repeating close on an issued handle resolves as a no-op without retaining the
account. Calls on released handles reject with `name: "AccountClosedError"`,
`code: "ACCOUNT_CLOSED"` and a message identifying the closed handle. Never-issued
handles retain the missing-account error. Calling the `dispose` method keeps
the handle available, matching the exchange API's disposal semantics.
`tests/native-account-lifecycle.test.mjs` exercises these contracts through the
compiled addon.
Rejections are JavaScript Error objects with the source `name`, plain `message`
and optional `code`. An absent source code stays absent; callers do not parse
error messages as JSON. `tests/native-account-errors.test.mjs` checks this
through the actual registry and native transport.

The native API takes `nativeAccountOpen({exchange, options}, onEvent, onStore, sign?, proxyToken?)`
and `nativeAccountCallMethod(id, method, argsArray)`. Options, method results,
events, Store payloads and signer requests/responses are JavaScript values.
The boundary retains `NaN`, infinities, negative zero, own `undefined` properties and
UTF-16 strings. No JSON text API is accepted. Proxy-token callbacks resolve strings.
The owned tree supports structured data, arrays and BigInts; cycles and functions
reject visibly. Native builds require Node-API 6 or later.
Emitter payloads and error codes retain the shared core’s JS values through owned
capture. Hosted signer throws and rejected promises preserve name, message and
code, including lone UTF-16 surrogates in name/message and own undefined,
nonfinite or structured code values.
`tests/native-account-values.test.mjs` and the Rust `native_account_values`
target exercise actual signed Binance reads containing nonfinite prices.
The signer-rejection Node and Rust targets drive failed signing through actual
Binance accounts and verify that no terminal HTTP request is sent.

Store delivery uses the shared worker payload preparer and leading/trailing
throttle (250ms times `tuning.dataUpdateThrottleMultiplier`). Startup posts a
forced full snapshot; live posts omit stable markets, orders and positions.
Browser-mode tickers use packed deltas, while `hostedMode` sends full tickers.
Payloads include public markets, socket health and message statistics, and omit
truthy raw `info` on orders, positions and displayed positions. Closing or
deactivating the account cancels pending delivery. The Rust `native_account_store`
and Node `tests/native-account-store.test.mjs` targets drive actual Binance
gateway frames and signed HTTP reads through the account boundary.

Venue `deactivate` requests clear host delivery, dispose the exchange, and post
`{event: "deactivate-status", data: true}`, matching the TS worker. The Store
reset performed by disposal is suppressed. The Rust `native_account_deactivation`
suite and Node `tests/native-account-deactivation.test.mjs` drive an actual
Orderly-family account through a loopback expired-key response. The Rust suite
also verifies that signer waits resolve as disposed.

`smoke/bun/native-exchange.mjs` exercises this surface against a local HTTP server.

Streaming uses the same command channel: call `listenOHLCV([options])`,
`listenOHLCVs([options])` or `listenOrderBook([symbol])` to obtain a numeric
registration handle. `onEvent` receives the method name with
`data: {subscriptionId, data: callbackValue}`. Call `unlisten([subscriptionId])`
to run the venue cleanup; repeated cleanup is silent. Account disposal closes
all registrations and suppresses subsequent callbacks. Bulk candle APIs that
return no cleanup in TS retain that venue lifetime while allowing host delivery
to be stopped through the handle.

`tests/native-account-streams.test.mjs` and the scoped Rust
`native_account_streams` suite drive real Binance gateway candle/book frames,
observe remote unsubscribe, and check disposal through the native account API.

The account method table exposes `findOrderByClientOrderId` through the venue's
lookup implementation so hosts can reconcile a lost placement response. It also
accepts the worker's `removeOrders([{id}])` and `fetchLastLogsFromWebsockets`, plus
`setActiveTickerSymbols` / `getActiveTickerSymbols` from the exchange API.

Run the production-path regressions without Bun:

```sh
cd sdk-rs
cargo test -p safe-cex-registry --test native_account_methods
node safe-cex-ffi/scripts/build.mjs node
node --test safe-cex-ffi/tests/native-account-methods.test.mjs
```

Both suites construct real registry accounts and read orders over loopback HTTP.
The Node suite loads the compiled napi addon and observes its Store callbacks.

`tests/native-account-factory-errors.test.mjs` exercises shared-factory proxy
auth and server errors through a real Bitrue account. The corresponding Rust
`native_account_factory_errors` target also checks request outcome counters on
the native registry path. Both use loopback HTTP.

`tests/native-account-callback-lifetime.test.mjs` checks account close in a child
Node process. Open accounts own JS signer and proxy-token callbacks; venue runtime
closures hold weak references so disposal releases callbacks even when a venue
runtime remains retained. Close rejects pending callback requests with a visible closed-account error. The child must exit naturally after close.

Native `start` follows the worker startup result: `true` after the forced full
Store post and `false` after cancellation or failure. Disposal, close and
deactivation cancel pending startup. Errors and the 30-second deadline use the
existing worker logger with `worker_error_phase: "startup"`; disposed failures
follow the source console warning path. A lost startup race leaves the actual
venue operation running, while its invalidated delivery token suppresses a late
full snapshot. `tests/native-account-startup.test.mjs` exercises real gateway
startup, cancellation, signer rejection and the actual deadline.

`updateSymbolMetadata` reuses the worker's keyed 500ms leading/trailing throttle.
It acknowledges immediately with null while the venue request runs separately;
late failures follow the existing runtime exception reporting. Close and
venue deactivation clear pending trailing metadata calls. The Rust
`native_account_metadata` and Node `native-account-metadata.test.mjs` targets
observe real Bitrue leverage-bracket HTTP requests and disposal.

The parity manifest records native Store delivery as worker split members:
construction reuses `_throttledStoreUpdate`, subscribed updates follow
`_setupExchangeEventListeners`, snapshots follow `sendSnapshot`, and teardown
follows `_clearRuntimePipeline`. The native host shares the worker throttle and
payload preparer; its weak owners and owned Store frames retain the documented
native ownership boundary. `NativeAccountFuture` remains part of the native
registry host mapping after its rename from `JsonFuture`.

The individually listed native-boundary exemptions cover `OwnedJsValue` and its
registry alias, the `JsData` napi codec, host callback handles/settlement glue,
and the owned callback/std-error origin carriers. They name ABI representations
with no separate TypeScript SDK declarations. They preserve the explicit tree
cycle/depth and callback-inspection limits; they do not exempt Store delivery,
exchange policy or worker algorithms from their source mappings and binding tests.

Log, error, warning and Grafana event callbacks carry the worker’s rest-argument
array, including an empty array for zero arguments. A single array argument stays
one array element; fill payloads retain their scalar shape. Private websocket
frame/subscription diagnostics (`grafana-pws`, `grafana-pws-sub`) use this same
bridge. `tests/native-account-events.test.mjs` drives a real Binance preflight
error and checks its metadata argument; registry event tests cover emitter arity
and both diagnostic channels through a real native account.
