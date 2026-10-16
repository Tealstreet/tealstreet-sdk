# Kucoin validation finding repair

Base integration: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
Branch: `feat/kucoin-finding-closures`. The base contains merged integration work.

`accounts::validate_spot_account` now uses the existing shared
`AccountValidationError::from_value` for an ordinary bad-code response. Truthy
numeric, boolean, array and object `msg` values retain their decoded native shape;
falsy values use the original fallback. This return path does not emit and does
not fetch balance. No shared type, trait or transport ABI changed.

Native assertions check number type and value 77, independent request count one,
no emission, absent balance and original JSON shapes before maintained source
comparisons. Supplemental enumeration floor is four rejection shapes, stopping
on the numeric case. Existing failure/cancellation/balance witnesses remain.

This repairs one cause, not the entire divergent row. Catch emission still meets
the string-only ExchangeErrorInput, raw response transformation remains pending,
and the previously declared balance/result shape choices remain. Canonical
policies are sdk-rs/parity/MANIFEST-SPEC.md#shared-utf8-string-boundary,
sdk-rs/parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias and
sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy.

## Inventory

Tomllib at the base: Kucoin 146 ported / 45 divergent / 0 pending. Ten rows have
leading FINDING declarations; 22 divergent rows contain FINDING text, including
mixed causes. This implementation moves zero pending rows and does not claim
that ten findings are closed. Six class member factories still lack concrete
body bindings. Store.market_by_symbol remains a typed own-entry map; shared
prototype replacement is explicitly unsupported. Closed order enums and ticker
string fields do not gain dynamic publication capabilities in this patch.

## Measurements

Pushed gate SHA: `70a388f7f0b00f9437295f33b07fa246f4536b61`.

| Gate | Measured exit/result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --all-targets --jobs 2 | 0 |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-kucoin --test spot_validation --jobs 2 -- --test-threads=1 | 0; nine passed |
| node "$YARN_JS" parity:extract:check | 0; output Would update 0 manifests; 0 added, 0 orphaned symbols |

The compiler environment was supplied by SAFE_CEX_TYPESCRIPT_PATH. Cargo calls
were sequential. Extractor writer exited 0 with no orphaned/added symbols.
The earlier pushed SHA `3a6e9cfef2bee6c7df1b8457797bce71aca384b6` had check
exit 101 from the test-only nonexistent Balance::default call; it was corrected
with the explicit existing fixture. Test/extractor gates did not run after that
compile failure, and that failure is not a mutation proof.

At the gate SHA, substituting
`AccountValidationError::from_value(Value::String(message.to_string()))` for the
native value retention compiled and exited **101** in the focused
`validation_native_rejection_values_are_retained_without_emission` test.
`KUCOIN-VALIDATION-NATIVE original rejection value` observed Bool(true) versus
String("true") before any source comparison. Exact original bytes were restored;
the same single test exited **0**, one passed/eight filtered out. This restored
filtered run is not another nine-test run. No control survivor.

Sub-agent review of the actual diff found the truthy/no-emit behavior aligned.
The reviewer ran no gates. Concrete remaining class blockers are typed
BaseWebSocket versus opaque V, private lifecycle Cell state, typed
BaseExchange/Store/JSON arguments and closed outputs. Concrete bindings need
shared state or widened body entrypoints; root-string host dispatch would leave
the same omission. Shared prototype replacement remains explicitly unsupported;
no venue-specific poison-key workaround was added.

After gates, the full crate src/tests preservation diff against the gate SHA
measured exit **0**. This sheet-only receipt carries those measured input results;
it does not claim freshly rerun Cargo at the receipt SHA. Tomllib remains
146 ported / 45 divergent / 0 pending: zero status transitions, one cause repaired.

Not run: workspace compilation, full venue/workspace suites, host/wasm clippy,
CI reruns, live exchange calls or complete owning-class runtime assembly.
