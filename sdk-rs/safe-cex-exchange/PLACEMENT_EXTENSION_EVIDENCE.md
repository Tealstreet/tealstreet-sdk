# Venue placement extension repair

The associated `Exchange::PlaceOpts` preserves full venue payloads through raw
erasure, shared caller-ID validation, typed capability preflight and private
`ValidatedOrder<V>` / `ValidatedBatch<V>` construction. Ordinary venues explicitly
choose `PlaceOrderOpts`; extended venues supply Borrow, From and DeserializeOwned.
The JSON path decodes the selected venue type directly. Venue deserializers remain
responsible for rejecting malformed, null and conflicting extension fields.

Validation for implementation a794509d38:

- All-feature, all-target `cargo test --locked -p safe-cex-exchange --all-features
  --all-targets -- --include-ignored`: 93 passed, zero failed or ignored. Includes
  13 contract tests, live TS oracles and all three trybuild diagnostic snapshots.
- Plain all-feature/all-target Clippy, package formatting, two doctests and both
  default/all-feature wasm checks passed.
- Parity invention/status passed: 6,901 rows and 945 public items. The extractor
  required its canonical final newline in the edited base manifest; regenerated.
- Independent detached review: MERGE. Reviewer reran 13 contract tests and the
  three trybuild cases; strongest attack was an invalid second order producing
  zero writes through both native and sequential batch dispatch.

Each mutation below was run independently and restored. Behavioral controls
compiled and failed the named assertion, not a compiler error:

| Mutation | Rejecting test |
| --- | --- |
| Project full typed opts through Borrow then From | `full_venue_options_survive_typed_and_json_dyn_placement` |
| Decode JSON as shared opts then From | `malformed_or_conflicting_json_extensions_cannot_fall_back_to_shared_options` |
| Remove the batch capability-validation pass | `every_batch_extension_is_validated_before_sequential_or_native_write` |
| Refuse shared inputs instead of converting through From | `generic_nuke_converts_shared_order_into_plain_venue_options` |

The actual nuke path proves shared-generated orders reach an extended venue as
its Plain default. The family-variant test proves capability validation belongs
to each concrete venue type, including two variants with the same options type.

Compile-time controls independently changed A to B in each cross-venue fixture,
and removed the private-field write in the privacy fixture. Each then compiled;
trybuild rejected that unexpected success. Restored snapshots pass and require
E0308 for each transfer and E0616 for the private write; doctest annotations are
not used as diagnostic proof.
