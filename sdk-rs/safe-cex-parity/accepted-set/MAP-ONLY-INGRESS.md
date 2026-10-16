# Object-only records and string-only enums

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

## String-only literal enums

A Serde-derived unit-variant enum also accepts an externally tagged object such
as `{"long": null}`. A TypeScript string-literal union does not admit that object.
For an enum representing such a union, use the existing
`String::deserialize(d)?` admission guard, then pass the admitted string to a
private Serde remote enum through `serde::de::value::StringDeserializer`.
This is the maintained pattern in
[shared OrderSide and the other string enums](../../safe-cex-types/src/lib.rs).
It needs no new public helper, derive macro or gate permission.

```rust
#[derive(Debug, PartialEq)]
pub enum Side {
    Long,
    Short,
}

#[derive(serde::Deserialize)]
#[serde(remote = "Side")]
enum SideWire {
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
}

impl<'de> serde::Deserialize<'de> for Side {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize<'de>>::deserialize(d)?;
        SideWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
```

Preserve all existing literal spellings, rename/alias rules and serialization
behavior. Do not trim, change case or stringify a non-string input. Retain an
existing serialization derive or delegation separately; the example adds only
decoding. The remote enum carries the reviewed decoding spellings without
implementing an independent `Deserialize` for `SideWire`. If a real private enum
retains its own `Deserialize`, it remains an independently gated owner and needs
its own string boundary and native evidence. An outer string guard cannot cover
that helper. Do not apply this repair to a protocol that actually declares
tagged objects or payload-carrying variants; review that schema separately.

Required proof for each repaired enum:

1. Derive the exact complete literal set from the maintained source. Decode
   every admitted spelling and compare the actual native variant. Preserve
   native-to-wire serialization where it already exists; a round trip alone
   cannot prove variant identity. Existing Serde metadata oracles must still
   inspect the complete remote variant schema, including any reviewed aliases.
2. Reject a valid tagged map for **every** unit variant, including
   `{"long": null}` for the example, through ordinary owned and borrowed JSON,
   the qualified decoder and JSON text. Include arrays, null, numbers, booleans,
   unknown strings and non-admitted case/whitespace/BOM spellings. A misspelled
   tag or invalid payload can fail without the string guard and proves nothing
   about this boundary.
3. In an isolated compiling control, remove only the string-admission front
   door: replace the two statements in `deserialize` with
   `SideWire::deserialize(d)`. Require a previously refused valid tagged map to
   become accepted and the named shape assertion to fail. Restore exact bytes
   and require the native tests to pass. Compiler failures do not count.
4. Classify the actual owner with `shape: "string"`, the independently reviewed
   `literals`, exact compiler entrypoints and direct native variant observations.
   Preserve every alternate decoder path and run fresh ordinary inventory and
   scoped production probes. Remove any exact repaired standing-debt entry in
   the same change; do not widen the classification to admit tagged maps or add
   a private-helper exemption.

Object records and string enums have different admission guards. Proposal
reports, private visibility and a passing neighboring owner never replace the
repaired owner's native evidence. Follow the same serial Cargo/probe rules above.
