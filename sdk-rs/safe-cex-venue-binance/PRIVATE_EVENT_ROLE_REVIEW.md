# Private-event value roles

Six owners lacked accepted-set records. None deserializes a wire record: the
actual shared `Js` decoder and existing external receiver classifications keep
those independent obligations. Each free function retains its exact compiler API
and a specific non-decoder reason; `PrivateEnum::wire` has a compiler-confirmed
self-only typed entrypoint. The existing `EventArray` alias is unchanged.

| Owner | Individual role |
| --- | --- |
| `PrivateEnum::wire` | Render a closed native categorical enum to its outgoing wire value. |
| `record` | Build an outgoing native object from static names and existing value handles. |
| `string` | Wrap literal Rust text as a UTF16 native string without parsing it. |
| `stock` | Resolve an existing current-realm property referent/scalar by name. |
| `table` | Select a typed table member and render its categorical value, or retain the existing inherited realm member. |
| `array_for_each` | Traverse the initial array length and invoke the supplied callback on present native elements; callback decoding remains separately owned. |

These are role classifications, not executed decoder receipts or an arbitrary-JS
admission claim. The maintained source bodies use these helpers for private
order/account events; separate actual native bindings establish bounded effects.
`production_native_private_orders_frames_update_the_real_store` constructs the
real Futures/PM/Spot owners over `native_dependencies`, dispatches full messages
through the actual private state and observes native Store enum/numeric fields
and fill effects. The corresponding position test dispatches account messages
through real Futures/PM state and reads contracts, entry, PnL, side and notional.
Only primitive host ports are scripted; no event mapper or Store is replaced.

The `accepted_receivers` witness keeps the three primitive receivers' stock
constructor/toString/prototype and missing-key checks. Arrays have a distinct
canonical Array.prototype and its own toString, matching JavaScript. Their
separate thirteen-field native/Node vm profile checks same-realm sharing,
cross-realm separation, retained identity after rebinding, later prototype and
function property writes, native signed-zero element/property bits and the
rendered value.
Native references and number bits are observed before projection. The mandatory
Binance oracle includes the ignored Node comparison. These finite profiles do
not claim arbitrary getters/callables or Array constructor coverage.

The whole-source `text_in` and `attach` pins and all external-Js receiver item
pins remain exact. Do not refresh them for temporary mutation controls. Native
controls bypass no scanner guard; they compile the real library in normal tests,
then restore the original source before canonical inventory is rerun.

From `sdk-rs`, run serially:

```sh
export CARGO_INCREMENTAL=0
node safe-cex-venue-binance/scripts/private-event-role-contracts.mjs
python3 safe-cex-venue-binance/scripts/private-event-role-controls.py
cargo test --locked --jobs 2 -p safe-cex-venue-binance --test accepted_receivers -- --include-ignored
cargo clippy --locked --jobs 2 -p safe-cex-venue-binance --features native --lib --test private_classes --test accepted_receivers -- -D warnings
cargo test --locked --jobs 1 -p safe-cex-exchange --all-features --test cancellable_future_inventory
```

The pre-contract canonical RED requires all six compiled missing-contract
failures by exact name and a passing EventArray alias. GREEN checks exactly seven
individual classifications while retaining full compiler/dependency discovery.
The independent controls fault wire rendering, iteration, object construction,
stock lookup, literal construction and table mapping separately. Every fault
must compile and fail its named actual production binding; exact original source
restoration must pass after each. Compiler failures and empty selections fail the
runner. No production behavior, alias/receiver/local-function guard or
cancellation permission changes.
