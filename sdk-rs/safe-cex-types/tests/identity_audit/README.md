# Field identity supplement and inventory

This supplement was added after equal-field and symmetric-roundtrip blindness
was discovered. It does not claim historical independence for previous green
runs. The seven original frozen Store JSON artifacts and thirteen live provider
sources remain byte-identical; no shared feeder was regenerated in place.

Run from repository root with SAFE_CEX_TYPESCRIPT_PATH naming an installed
compiler: `python3 tests/identity_audit/inventory.py` (full path under this crate).
The script recursively parses JSON objects and raw PositionRecord numeric JSON,
not grep counts. On the 2026-10-13 merged a3d514bec8 tree it examined 121,714
JSON-visible records, 10,615 equal-value groups, seven frozen artifacts and 13
live providers. Enum candidates are provisionally strings. These are candidates,
not proved holes: distinct per-field cases elsewhere may already pin identity.
The expanded native/gate dispositions are in DISPOSITIONS.md; these historical
counts are retained as the scope of the original inventory.

The 71 dynamic macros are classified into five buckets in DISPOSITIONS.md.
Run `python3 sdk-rs/safe-cex-types/tests/identity_audit/controls.py` from the repo
root to replay the compiling mutations serially; set CARGO to the intended binary.
Each mutation must fail its named assertion and restore to passing. The script
temporarily edits source files: run it only with exclusive ownership of this checkout.

`field_identity_supplement.rs` adds four native model families: Balance's four
numeric fields; Ticker's eleven numeric and two string fields; Order's five
numeric and two string fields; and five one-hot StoreDataLoaded records. These
assert native getters after decode, not re-serialized bytes. A symmetric rename
mutation swaps Balance used/free in BOTH public Serialize and private Deserialize
metadata. It compiles and fails `FIELD-IDENTITY Balance.used`; restored tests pass.
No original feeder bytes changed. This is a supplement, not an in-place repair.

NaN capture scope: the same providers contain 170 stable NaN tags and 111 raw
64-bit captures, zero of which are NaN payloads. PositionRecord numeric JSON
spellings cannot produce NaN; validation numeric messages originate valid JSON
number spellings. Derived/native-patch/orderbook/risk/dictionary NaN results are
stable tags or isNaN booleans, not platform payload comparisons. Signed zero
remains bit-exact. The broader native bit-corpus disposition is recorded in DISPOSITIONS.md.

Later NaN-capture supplement: capture_bits canonicalizes only NaN to
7ff8000000000000, accepting both CPU sign/payload examples; all finite values,
both infinities and signed zero retain bits. Compiling zero-normalization and
finite-normalization controls fail respectively CAPTURE-BITS signed zero remains
observable and CAPTURE-BITS finite payload remains exact; restoring both passes.
This is not a rewrite of old feeder captures. The above Node capture inventory
was collected on arm64 macOS with Node 24.20.0; original frozen artifacts lack
architecture metadata and are not retrospectively attributed to this machine.
