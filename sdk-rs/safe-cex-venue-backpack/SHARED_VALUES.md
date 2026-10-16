# Shared runtime value cutover

The concrete Backpack frames now retain `safe_cex_exchange::js_runtime::value::Js`
directly. `native_value::Value` re-exports that type; there is no venue value enum
and no conversion at socket-handler or throttle boundaries. Arrays keep holes,
strings retain UTF-16 units, objects retain references, and numbers retain f64 bits.
The `NativeValue` trait contains the adapter's existing prepared-data operations
and checked UTF-8 projection. Prepared inputs are still owned snapshots, while
spreading a live runtime object preserves its nested references. This cutover
does not broaden the prepared API into executable getters, arbitrary coercion
hooks, promises or a general JavaScript host surface.

The full suite found an earlier source mismatch: the local REST `Market` record
contained an unused boolean `option: false`, while the current TypeScript
`fetchMarkets` result omits that property. The local field is removed and the
source oracle pins absence. The shared model bridge already omits `OptionTerms`
and is unchanged in that respect.

Measured against the cutover before its master refresh:

- `SAFE_CEX_TYPESCRIPT_PATH=$PWD/node_modules/typescript cargo test --locked
  --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --
  --include-ignored`: 107 tests passed across 26 completed targets, zero failed.
- The new constructed-venue test
  `shared_runtime_graph_reaches_backpack_throttled_ticker_without_projection`
  passes canonical runtime values into the real ticker throttle. It checks a
  deferred read after caller mutation, native infinity/negative-zero behavior,
  NaN omission, nested spread identity, holes and a lone UTF-16 unit's explicit
  UTF-8 projection refusal. Source parseFloat semantics require a string `"-0"`
  to retain negative zero; a numeric negative zero stringifies to positive zero.
- Three separate compiling production controls copy the deferred object, replace
  prepared numbers with zero, and replace malformed UTF-16 with U+FFFD. Each exits
  101 at the corresponding assertion; restored production passes the same test.
- Host all-target/all-feature and wasm library/all-feature strict clippy pass.
  Global parity passes (7459 rows, 17098 public Rust items). No source status is
  promoted by replacing the storage type.

The initial full run exposed the local `option` mismatch above; the final full
run passes after its repair. Extractor checking at the initial master base reports
only the incoming Polymarket exchange/registry formatting updates owned by #190.
Final integration evidence belongs in the PR; no extractor writer is used.
