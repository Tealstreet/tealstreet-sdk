# Accepted-input contract review

The 25 retained native/default census keys are reviewed individually in
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

Six constructors receive typed values or host capabilities. Existing native
SDK tests inspect their fields, optional lists, nested shared identity, errors,
UTF-16 units and numeric values. Production client-host tests in
`safe-cex-venue-lighter::binding::sdk_client::tests` execute the real SDK client
against its native host. The 13 retained free helpers have individual non-decoder
reasons for text/field output, host property access, protocol/error routing and
interceptor dispatch.

The unused standalone signer and Poseidon adapter contracts and their exclusive
native observations were removed after the shipped-caller census. All 25 retained
contract entries are unchanged. See `SIGNER_RETIREMENT_REVIEW.md` for the deletion
boundary; earlier 33-key receipts included the retired adapter and do not describe
the current package.

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
