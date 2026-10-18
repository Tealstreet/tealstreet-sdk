# Kraken construction callback aliases

`construction::Factory<'static, binding::rt::Rt>` is the owning native callable
`Rc<dyn Fn(Rc<Js>, Rc<Js>, BaseUrl) -> Result<Rc<Js>, Thrown> + 'static>`.
`RateLimit<'static, Rt>` takes one native client and returns the same client or
its original native failure. Both are constructor dependency capabilities, not
wire decoders. These two finite profiles fix only the actual Scope host and
lifetime; they do not classify other instantiations. Expected Rust types are
independently authored, not generated from the compiler target AST.

The source constructors in `kraken.exchange.ts` and `krakenspot.exchange.ts`
pass `createAPI` into `createXhrWithSecondaryKey` and apply
`rateLimit(xhr, {maxRPS: 5})`. Both create a separate unlimited client. Futures
also constructs a history client with the distinct history base. The production
Rust `construct_with` passes its Factory/RateLimit callbacks to the concrete
Scope `secondary_client`, then assigns unlimited/history clients independently.
No production body changes are needed for this cohort.

`scripts/construction-alias-contracts.mjs` uses maintained
`acceptedAliasContext` to authenticate fresh rustdoc/check artifacts, macro
origins, external roots and public declaration claims, then executes stock
native alias invariant identity, Serde and conversion detectors.
`--expect-unclassified` requires both actual missing-contract failures. Specific
non-decoder classification is separate from executing the constructor/client.

`tests/support/accepted_construction_aliases.rs` uses the host retained by a real
constructed Futures or Spot exchange. The native witnesses inspect both API
base selections and real signing headers, synchronous getter-thrown object
identity with lone UTF16/-0 cells, rate-limited client identity, five requests
before a 1000ms window and its held sixth request, and the separate unlimited
client. Both independently typed aliases and the actual production constructor
callbacks are exercised. The terminal HTTP/runtime are fixtures; client,
constructor, signing and limiter bodies remain production implementations.

`scripts/construction-alias-controls.py` changes the production Factory's base
selection, then separately the constructor's five-request limit to four. Each
must compile and fail its own named native assertion. Exact bytes are restored
in `finally`, followed by a passing named test. Compiler failures do not count
as controls. No broad non-decoder rule or exemption is added.

Commands run serially from `sdk-rs`:

```sh
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/construction-alias-contracts.mjs --expect-unclassified
cargo test --locked -p safe-cex-venue-kraken --jobs 1 --lib accepted_construction_aliases
python3 safe-cex-venue-kraken/scripts/construction-alias-controls.py
RUSTC_BOOTSTRAP=1 node safe-cex-venue-kraken/scripts/construction-alias-contracts.mjs
RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR="$PWD/target/accepted-inventory" node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-kraken --inventory --jobs=1
```

Discovery stays in the separate accepted-inventory target. Full parity index,
strict scoped all-target Clippy and all-features cancellation inventory are
separate readiness gates. Remaining Kraken aliases are not discharged by these
finite construction profiles.
