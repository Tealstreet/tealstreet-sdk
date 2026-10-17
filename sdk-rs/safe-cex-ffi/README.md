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

With the `node` feature, `nativeAccountOpen`, `nativeAccountCallMethod`,
`nativeAccountSignerAvailable` and `nativeAccountClose` expose the registry's
JSON account host. Events and private Store slices are forwarded to JS; signing
and proxy tokens remain host callbacks. `nativeAccountClose` stops forwarding
immediately, then resolves after disposal completes, or rejects with its error.
Calls and repeated closes against a removed account reject.
`smoke/bun/native-exchange.mjs` exercises this surface against a local HTTP server.

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
