# WOO structural alias review

Four aliases retain their production or declaration consumers. The inactive
V3 book-level array alias is retired with its isolated websocket caller closure;
its exact registration is removed without adding identity-fallback permission.

| Alias | Native role |
| --- | --- |
| `legacy_metadata::JsString` | Actual `Vec<u16>` decoder; every unit and length are observed, including isolated surrogates. |
| `legacy_metadata::Rows` | Typed `Rc<RefCell<Vec<Rc<Metadata>>>>` cache handle. `Metadata` has no decoder. Native traits classify it separately from executing the real cache path. |
| `legacy_metadata::Fields` | Typed ordered outbound static-name/Text-or-Number request fields; `Field` has no decoder. |
| `v3_contracts::WooV3Decimal` | Actual unrestricted UTF8 `String` decoder and its five real String conversion routes. It does not validate decimal syntax. |

Decoder observations use owned, borrowed, qualified and raw-text entrypoints.
All observations inspect native members before encoding the observation record.
Raw lone-surrogate String refusal is explicit; the UTF16 unit-vector decoder
retains those units. The remaining V3 decimal alias enables no migration route.

The former book-array identity is now refused by the ordinary alias guard.
The remaining four records retain their original native observations.

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

These four receipts do not classify other aliases, function owners, feature
configurations or Wasm execution. Preserve all existing declaration/source cases.
