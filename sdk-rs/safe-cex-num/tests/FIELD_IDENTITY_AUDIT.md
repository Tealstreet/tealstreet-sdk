# Field identity and NaN capture audit

Baseline: e03504cacb. This audit adds evidence; it does not retroactively make
an earlier corpus field-independent. The inventory contained 97 num test files:
62 top-level, 22 dependency-audit artifacts, nine compiler fixtures/snapshots,
three corpus files and one replacement fixture. Dependency metadata and compiler
diagnostics are not domain-record fixtures. Six owned delegation files in
exchange/shared_truthiness, transport/shared_truthiness and exchange/shared_stringify
were also inspected: scalar truthiness, independently varied network-error fields,
and JSON wire behavior respectively; no DTO roundtrip identity claim there.

## Shared feeders and preservation

`oracle.cjs`, `measure.cjs`, `build-corpus.cjs`, `corpus/pins.json` and
`corpus/engines-disagree.json` feed the arithmetic measurement, pin and Rust gates.
Their original bytes remain unchanged. Operands vary independently; subtract and
divide distinguish their identities. NaN already uses the symbolic `NaN` encoding.
No swap-blind repeated-field record was found in this scalar arithmetic feeder.

`numeric_ingress.cjs` has one direct consumer, numeric_ingress.rs; the CI wiring
checker also reads that Rust test's compiler-control registry. The original oracle
bytes remain unchanged; the Rust book assertions were subsequently strengthened
by the cardinality audit. Its 2,019 JSON-number spellings are scalar inputs, not a claim
of independence between the record fields built from them. It cannot generate
NaN: valid JSON numbers parse to a finite value or signed infinity.

`p4_quotes_oracle.cjs` has one direct in-crate reader. All 1,152 quote rows had
bid equal to ask. Preserve that original file too; the separately named
`p4_quotes_field_identity.cjs` adds 162 actual-source cases with independent
bid/ask/last values. It is new evidence, not an independent historical capture.
Static reference searches covered workflows, scripts, packages and SDK source;
unknown external consumers cannot be ruled out.

## Repeated-value repairs

Nine num fixture families had an identity gap: placement's six numbers; book's
sides/rows/three numbers; nested tuples; ordered map values; typed versus opaque
numbers; per-element visitation counters; configuration credentials; the five
info-model fixtures' ordinary populated fields; and bid/ask quotes.
`numeric_field_identity.rs` supplements the first eight, without rewriting the
edge corpus. It asserts native fields, distinct key-associated values and distinct
per-element counters through tuple arities 1..16. Quotes use the supplement above.
The existing 60,570 info comparisons establish info preservation, not identity of
all ordinary model fields. The supplement covers the populated ordinary fields;
unpopulated optional model/configuration fields are not claimed covered.

## Gate assertion shapes

| Gate | Identity assertion |
| --- | --- |
| differential, selection, p4_utils, p4_coercion, p4_number_conversion, p4_number_strings | Scalar native numbers/strings or tagged errors; independently varied arguments |
| js_primitives, js_number_hooks, js_number_string, js_trim | Native primitive results, hook order/counts or exact output strings |
| js_math_round, js_log10, js_pow, js_math_extrema, js_date, lodash_random | Native Number bits, with NaN-only equivalence; random also counts source calls |
| js_bigint | Native integer/error result; decimal formatting is the observed result |
| js_collation, lodash_case, js_replace | Native ordering/string/code-unit results; unequal operands and reverse cases |
| js_property, js_table_lookup, get_key | Native tagged lookup result, own value, presence/truthiness/nullishness and prototype identity |
| js_own_keys, js_json_stringify | Enumeration and exact wire serialization are the operations under test, not DTO roundtrips |
| numeric_ingress | Native numeric leaves, retained opaque values, explicit refusal controls; eight repeated-value families supplemented |
| numeric_field_identity | Direct native model/tuple/map fields and individual visit counters |
| p4_quotes | Native scalar output; independent bid/ask supplement added |
| p4_sizing | Scalar native outputs; returned Balance now also checks native used/free/total/upnl and nested currencies before its wire comparison |

One num gate's assertion shape changed: p4_sizing's Balance branch. The numeric
suite already read native leaves but needed distinct values. No frozen artifact
was regenerated. GRVT's two converted gates and native field census are documented
in its own FIELD_IDENTITY_AUDIT.md.

NaN capture provenance and controls are in NUMBER_CAPTURE_AUDIT.md.

Compiling consumer controls replaced the ask return with bid, swapped stopLoss
and takeProfit in the production input buffer, and swapped bid/ask there. They
failed QUOTE FIELD IDENTITY, FIELD IDENTITY placement, and FIELD IDENTITY ticker,
respectively. Restored production passed both supplements. Scoped changed-target
tests passed on Node 24 and 26; host and wasm strict clippy passed separately.
Sub-agent review returned MERGE with the optional-field exclusions above.
