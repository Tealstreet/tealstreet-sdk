# Accepted-input contract review

The 33 selected native/default census keys are reviewed individually in
[the SDK contract file](../safe-cex-parity/accepted-set/contracts/safe-cex-lighter-sdk.json). Exact compiler APIs and the
per-owner reasons are part of each contract. The review follows the shared
[string](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary) and
[surrogate](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict) boundaries.

`api_reads::Fields<bool>` is a real decoder for sequences of `(Vec<u16>, bool)`
tuples. Its runtime contract tests empty and populated sequences, tuple arity,
boolean values and the integer `u16` domain. Native observations inspect decoded
lengths, keys and values directly, including astral pairs and lone surrogate
units. `tests/accepted_aliases.rs` exercises the same production alias; no
arbitrary generic-value decoder claim is made.

The other five fixed public aliases are future or callback capabilities.
Their independently registered compiler instantiations use `bool` and `String`;
`NativeMethod` uses `dyn ReadHost<Value = bool, Error = String>`. The ordinary gate
checks actual native conversion/decoder traits and compiler target identity.
The alias integration test also executes receiver refusal, original callback
errors, immediate results and polled future results.

Nine constructors receive typed values or host capabilities. Existing native
SDK tests inspect their fields, optional lists, nested shared identity, errors,
UTF-16 units and numeric values. Twelve production client-host tests in
`safe-cex-venue-lighter::binding::sdk_client::tests` execute the real SDK client
against its native host. The 17 selected free helpers have individual
non-decoder reasons: text/field output construction, host property access,
protocol/error routing, interceptor dispatch or byte-to-hex encoding.

`PoseidonKeyHost` retains its exact ten selected trait entrypoints. The raw
property selector is executed directly against native key variants; expected
private/public key units and unknown-property `Undefined` are independent of
serialization. The probe's unused index and scheduler panic if invoked, so the
accessor cannot gain an unobserved loader or scheduling effect. Other selected
methods retain their individually reviewed typed APIs and the existing
production bridge test.

Run the ordinary scoped native gate from the repository root:

```sh
RUSTUP_TOOLCHAIN=1.99.0 CARGO_INCREMENTAL=0 node \
  sdk-rs/safe-cex-parity/scripts/accepted-set.mjs \
  --package=safe-cex-lighter-sdk
```

Run SDK native evidence with the local TypeScript compiler configured:

```sh
SAFE_CEX_TYPESCRIPT_PATH="$PWD/node_modules/typescript" CARGO_INCREMENTAL=0 \
  cargo +1.99.0 test --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-lighter-sdk --tests
```

The helper's census report covers selected module-scope native/default keys.
It is not a runtime receipt or feature/wasm matrix result. Ordinary source
inventory additionally sees local scopes; the client callback macro expansion
is a separate production source-discovery repair.

The compiling negative control replaces the production `privateKey` accessor's
text result with `Undefined`. The ordinary native gate then fails the
`PoseidonKeyHost` witness: expected native units `[55296]`, observed
`Undefined`. Restoring the source byte for byte passes the same gate with two
production runtime results, 31 explicit classifications and zero failed owners.
The native SDK evidence suite passes all 28 tests, including both fixed-alias
tests. This control tests the native value before any serialization can hide it.

The ordinary wasm gate, using the same command with
`--target=wasm32-unknown-unknown`, independently discovers the same 33 keys and
returns two production runtime results, 31 explicit classifications and zero
failed owners. The native and wasm receipts together pass the scoped ratchet;
the shared defect ledger is unchanged. This package has no additional feature
combinations.

## Referral constructor ownership audit

`ReferralApi.constructor` is ACCEPT under decision 2 (owned native values).
The maintained complete TS class stores the exact client in an enumerable,
writable, configurable own property. Replacing it routes the next query to the
replacement; deleting it makes the next query reject with TypeError. These
property descriptors and deletion remain explicit differences from the typed
Rust facade, which supports replacement with another typed client.

`tests/api_reads.rs::referral_constructor_retains_routes_replaces_and_releases_exact_client`
executes the mapped production constructor and query with terminal ReadHost
primitives. Construction retains exactly one client reference and starts no
request or task. Both clients receive their own query, replacement releases the
first client, settled work releases its temporary references, and dropping the
facade releases the second client. Weak references prove neither client leaks.
The companion source oracle executes the maintained complete ReferralApi class
for descriptor, replacement, deletion and request-routing observations.

The ownership control inserts an extra retained reference into the production
constructor. It compiles and fails `REFERRAL constructor one retained client`;
restoring the exact constructor passes. No production change is needed.
