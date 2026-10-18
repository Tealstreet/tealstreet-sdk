# Orderly concrete Exchange binding

`native_exchange::OrderlyExchange::new` constructs the six source subclasses
(KodiakFI, MODE, SalsaDEX, TealstreetDEX, What and Woofi) on the shared
`js_runtime` Scope/Js/HTTP/socket primitives. Its private binding hosts dispatch
to the already ported source bodies. No TypeScript evaluator is used.

The constructed-exchange witness in `tests/exchange_trait.rs` calls all sixteen
Exchange methods through scripted transports. It checks actual request routes,
loaded Store slices, the source positions-read balance write, distinct fractional
price/amount steps, margin state, broker injection, caller IDs, stable-ID PUT
amendment and DELETE cancellation. The four inherited unsupported operations
retain `Not implemented` (base.ts:799, 832, 989, 1138). Separate witnesses cover
all six source order tags, exact Ed25519 read preimages, UTF-16 client-ID
boundaries before any HTTP, definitive/transient status handling before dialing,
and actual private auth/subscription frames. The signing-exception unit witness
dials the real shared socket facade with a malformed key and observes no frames
plus shared recovery/reconnect diagnostics.

Measured local results before PR creation:

- Full scoped suite with live actual-TypeScript oracles and ignored tests:
  82 tests passed across 18 targets. `SAFE_CEX_TYPESCRIPT_PATH` pointed at the
  installed repository compiler; no oracle or fixture values were changed.
- All-sixteen production controls compiled independently. Each injected a
  method-specific early failure into the concrete trait implementation; the
  single all-sixteen E2E assertion failed with exit 101 for every mutation,
  including exact-error assertions for inherited unsupported methods. All
  original source bytes were restored and the seven E2E tests passed again.
- Native registry: all six names construct through production native HTTP and
  strict server-side Ed25519 verification. Aster's native HMAC regression and
  twelve registry construction/membership tests pass: fourteen tests total.
- Parity: zero findings, 7459 rows / 17081 public Rust items after merging master.
- Registry live TS equivalence: 48 ordered names in both source registries.
- Layer policy: 88 tests passed with the Orderly constructor dependency listed.
- Strict host all-target and wasm library clippy passed for Orderly and registry.
  Scoped rustfmt completed successfully.

The shared native registry adapter now takes a venue name; Aster's existing
routing is retained. Native hosted HTTP now uses the shared signer bridge; the source local-secret
requirements for private HTTP and WS remain. See HOSTED_SIGNER_EVIDENCE.md. The per-options HTTP factory owns transport
selection, while original Orderly API bodies own serialization, signing, retries
and rate limiting.

The private onOpen recovery intentionally follows the shared WS contract:
`safe-cex-exchange/WS_CONTRACT.md` and the `BaseWebSocket.connectAndSubscribe`
row in `parity/manifest/exchanges/base.ws.ts.toml`, whose split target includes
`BaseWebSocket::authenticate_and_subscribe`. Its unchanged source signing body
runs inside that shared recovery operation. Signing exceptions send no frames,
disregard silence and attempt reconnect; actual subscriptions still wait for
the server's successful auth reply. The onOpen row is divergent and the overseer
accepted this choice on 2026-10-16. Source disposal sets the shared writable flag
and disposes its public and private sockets in source order; it does not invoke
BaseExchange disposal. No extra read or disposal guards were added.

The constructed disposal regression observes the shared flag, closed sockets and
retained emitter listeners. Independent compiled controls restoring either the
extra base cleanup or the own flag shadow fail their intended assertion (101);
exact restoration passes.

Commands (run sequentially to keep Cargo processes scoped):

```sh
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-orderly --all-targets --jobs 1 -- --include-ignored
cargo test --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --test native_host_orderly --test native_host_asterdex --test rust_venues --jobs 1
cargo run --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-parity --bin parity-check --
cargo clippy --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-orderly -p safe-cex-registry --all-targets --jobs 1 -- -D warnings
cargo clippy --locked --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-orderly -p safe-cex-registry --target wasm32-unknown-unknown --lib --jobs 1 -- -D warnings
node sdk-rs/safe-cex-registry/scripts/check.mjs
node --test sdk-rs/scripts/check-layers.test.mjs
```


## Strict ticker frame rows

The three `*_rows_refuse_only_null_and_string_values` production socket tests
compile against master 1334785318 and fail at the later valid ADA update. With
the admission fix, each runs all six account subclasses and preserves complete
BTC/LTC rows refused for null/string fields, updates both valid ETH/ADA siblings,
and reports both symbols with the original field/value. Only HTTP, sockets and
the deterministic scheduler are terminal scripted inputs; the binding, source
stream body, typed Store and class lifecycle are real.

Two additional actual-socket controls cover later source-property errors taking
precedence over deferred typed reports, and inherited lookup names remaining
no-ops while good siblings update. Each covers all three topics and six classes.
The four existing private-topic regressions pass across those same subclasses.
All eight maintained-source stream comparisons pass with their raw-value Store;
that helper oracle is separate from the native typed-boundary proof. Normal Node
module resolution replaces its broken worktree-root-only lodash path. Scoped
strict Clippy, cancellation inventory (6), override inventory, parity (7459 rows)
and TS extractor checks pass. No shared model or transport API is changed.
