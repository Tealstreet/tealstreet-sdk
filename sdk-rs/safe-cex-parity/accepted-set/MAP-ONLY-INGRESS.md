# Object-only record ingress

Use the existing `safe_cex_types::strict_decode::object` deserializer guard for
TypeScript object records. Apply it at the concrete record's `Deserialize`
entrypoint, using a private Serde **remote** companion to retain the field schema.
The helper already exists; this recipe needs no new macro, attribute crate or
scanner permission. Tuple-shaped protocols keep their array representation.
First confirm the actual decoder accepts an otherwise valid positional array.
If it already rejects that array, record the native evidence and review the
existing boundary before changing its implementation.

```rust
// Preserve the original type's traits; do not add Serialize just for this repair.
#[derive(Debug)]
pub struct PriceRecord {
    pub id: String,
    pub amount: f64,
}

#[derive(serde::Deserialize)]
#[serde(remote = "PriceRecord")]
struct PriceRecordWire {
    id: String,
    amount: f64,
}

impl<'de> serde::Deserialize<'de> for PriceRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        PriceRecordWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}
```

Keep all existing rename, default, custom field decoding, flattening and presence
rules on the remote schema. An optional field's absent/null distinction needs its
existing field decoder. Preserve the source's unknown-field policy. Constraining
the outer shape does not authorize changing those behaviors.

A remote derive emits a decoder returning `PriceRecord`; it does not implement
`Deserialize` for `PriceRecordWire`. Adding an ordinary derived `Deserialize` to
a second private `Wire` creates another independently inventoried decoder. A
parent's object guard does not discharge that decoder's own contract. Keep the
remote companion private and run fresh ordinary compiler inventory after repair.
Any additional discovered owner/API still needs individual review and evidence.

## Preserve numeric ingress

The shape guard belongs inside the existing numeric ingress pipeline. For a
venue record already using `safe_cex_num::numeric_ingress::JsNumberInput`, the
same remote schema is called as follows:

```rust,ignore
let input = <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize<'de>>::deserialize(d)?;
input
    .decode_with(|decoder| {
        PriceRecordWire::deserialize(safe_cex_types::strict_decode::object(decoder))
    })
    .map_err(serde::de::Error::custom)
```

Retain the existing numeric front door and its output checks, including binary64
rounding, signed zero and nonfinite handling. Follow the crate's dependency
layer: lower-layer types use their established numeric backend. Do not add a
reverse dependency to adopt this illustrative venue spelling. The governing
contract is [numeric ingress](../../parity/MANIFEST-SPEC.md#numeric-ingress-defect).

## Required proof for each repaired owner

1. Author a valid object and expected native fields from the TS/input contract.
   Exercise both `serde_json::from_str::<Owner>` and the actual qualified/native
   decoder paths. Compare native fields, optional-field presence and numeric
   bits as appropriate; a serialized round trip or `is_ok()` alone is insufficient.
2. Reject an independently authored **complete positional array in Serde field
   order**, using values that would satisfy every field decoder. Also cover
   scalar/null shapes, an empty array and relevant nested/optional cases. A
   malformed or wrongly ordered array can fail without the shape guard and
   provides no proof of this repair. Generic sample-map ordering is insufficient.
3. Remove only the `object(...)` guard in an isolated control, require the full
   valid positional array to become accepted and the named shape assertion to
   fail, then restore and pass. Compilation errors do not count as that failure.
4. Classify the actual owner with `shape: "map"`, an independently authored
   positive `sample`, exact compiler entrypoints, and direct native value
   witnesses. Include the complete ordered array in explicit observation or
   native cases; preserve every alternate decoder/conversion path. Run fresh
   scoped production native probes. If this removes measured standing debt,
   remove the exact repaired ledger entry in that same change; never add an
   exemption or broaden the contract to permit the array.

Follow the selected crate's Cargo concurrency guidance. Both the review helper
and `accepted-set.mjs` accept `--jobs=1`; direct `runProbes` callers can set
`configuration.jobs = 1`. Run accepted probes serially because they share their
scratch workspace.

## Maintained production references

- [CrossTradeStore](../../safe-cex-exchange/src/cross_trade/mod.rs) uses the
  shape-only remote pattern. Its
  [native regression](../../safe-cex-exchange/tests/cross_trade_store_shape.rs)
  observes actual snapshot fields and rejects the full seven-field positional
  array through both JSON-value and JSON-text inputs.
- [TickerMarkInput](../../safe-cex-venue-okx/src/exchange.rs) applies the same
  guard inside existing `JsNumberInput` decoding, retaining optional-field
  presence and the existing numeric output check.
- [The shared guard](../../safe-cex-types/src/strict_decode.rs) and
  [types guidance](../../safe-cex-types/CLAUDE.md) explain the object boundary and
  private companion policy.

This recipe covers object records. String-literal enums need their own string
admission guard and tagged-object rejection proof; that is a separate input
shape. Proposal reports, private visibility and a passing neighboring owner do
not replace the repaired owner's native evidence.
