# WOO structural alias review

The five assigned aliases all have production users or public wire-codec uses.
None is removed, relabeled as a callback, or admitted by an identity fallback.

| Alias | Native role |
| --- | --- |
| `legacy_metadata::JsString` | Actual `Vec<u16>` decoder; every unit and length are observed, including isolated surrogates. |
| `legacy_metadata::Rows` | Typed `Rc<RefCell<Vec<Rc<Metadata>>>>` cache handle. `Metadata` has no decoder. Native traits classify it separately from executing the real cache path. |
| `legacy_metadata::Fields` | Typed ordered outbound static-name/Text-or-Number request fields; `Field` has no decoder. |
| `v3_contracts::WooV3Decimal` | Actual unrestricted UTF8 `String` decoder and its five real String conversion routes. It does not validate decimal syntax. |
| `v3_ws_public::WooV3WsBookLevel` | Actual two-slot String array decoder; order, cardinality and both members are observed. |

Decoder observations use owned, borrowed, qualified and raw-text entrypoints.
All observations inspect native members before encoding the observation record.
Raw lone-surrogate String refusal is explicit; the UTF16 unit-vector decoder
retains those units. Neither V3 alias enables an inert migration route.

The array registration is exact to the compiler owner, source, nongeneric AST,
length, element identity and authenticated sysroot String artifact. Its expected
native type belongs to the tooling module. Caller redirects and unknown arrays
remain refused; there is no tuple/array family or identity fallback permission.
The policy test independently removes expanded-type validation, requires its
named refusal assertion to fail and restores the original policy in a new process.

`structural_alias_tests::metadata_aliases_keep_native_units_cache_identity_and_signed_fields`
constructs the actual WooExchange. It calls installed metadata members, inspects
native units/numeric bits and parent/child cache identity, then drives the actual
configured GET and leverage POST through the production signer. Runtime and HTTP
are primitive host ports. The separate control script corrupts native symbol
retention, cache reuse and request fields independently; each must compile and
fail that real class assertion, then pass after exact production restoration.

Run serially from `sdk-rs`:

```sh
node safe-cex-venue-woo/scripts/structural-alias-contracts.mjs
node --test safe-cex-parity/scripts/accepted-woo-alias.test.mjs
cargo test --locked --jobs 1 -p safe-cex-venue-woo --lib structural_alias_tests
python3 safe-cex-venue-woo/scripts/structural-alias-controls.py
```

These five receipts do not classify other aliases, function owners, feature
configurations or Wasm execution. Preserve all existing declaration/source cases.
