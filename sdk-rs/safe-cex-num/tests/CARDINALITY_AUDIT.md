# Collection cardinality audit

The audit distinguishes shortening captured expectations from changing a
production collection result. A green corpus count does not establish how many
native comparisons ran. Original source/corpus artifacts remain unchanged.
This document records measured slices; the remaining-scope list is not a pass.

## Reproduced false greens

`js_number_string` originally zipped 10,034 native inputs with unchecked Node
results. Capturing zero results or one result compiled and passed the original
target. This is expectation truncation, not a scalar production function returning
a collection. The repair independently pins source and native-input totals and
counts executed comparisons. Permanent empty/subset child controls require exit
101 at `NUMBER-SPELLING source total`; truncating after that check independently
requires `NUMBER-SPELLING compared/native count`. Restored target: two passed on
Node 24 and Node 26. Original input values and inline Node source are unchanged.

`numeric_ingress::Array::next_element_seed` was temporarily changed to consume
all input elements before returning None. The original shared placement/book
numeric target passed. The shipped decoder does not discard these elements;
this injected production mutation exposed a shipped assertion gap. A preliminary
non-consuming mutation failed on trailing elements and was not counted as proof.

The repair independently requires one native bid and one native ask on each of
three decoder routes for all 2,019 spellings, then six executed numeric checks
per book. The consuming-empty mutant compiled and failed
`INGRESS-CARDINALITY native bids`. Keeping that mutant while removing BOTH length
assertions compiled and failed `INGRESS-CARDINALITY compared book numbers`.
Production and assertions were restored; the scoped target passed. Separately,
a partial iterator-discard mutant already failed the field-identity supplement's
`FIELD IDENTITY book sides and rows count`; that is coverage now, after the
field-identity sweep, not a never-blind historical claim.

## Static pairwise inventory

Five literal Rust zip sites exist across num and GRVT tests: Number spelling,
numeric-field-identity's numeric slice helper, sizing's recursive JSON comparer,
GRVT's native Vec identity helper, and GRVT's four builder constants. Apart from
the repaired spelling gate, each checks paired lengths before walking. The
recursive sizing comparer also requires identical object key sets. Static checks
alone do not substitute for production controls.

## Existing collection comparisons

Eight independent production mutations compiled and exited 101: own-key
enumeration retaining zero or one key; whitespace splitting retaining zero or
one segment; and each UTF-16 first/all replacement API clearing or removing one
unit from ASCII-only results. The ASCII restriction keeps the subset mutation
from substituting an unrelated invalid-surrogate refusal for a comparison.

Key enumeration failed its complete expected-vector equality in
`own_key_order_matches_live_node`; splitting failed complete vector equality in
`whitespace_split_preserves_js_empty_segments_and_coalesces_runs`; all four
replacement controls failed `JavaScript replacement mismatch`. The replacement
gate separately pins 33,780 source rows and compares full code-unit vectors.
All production changes were restored and all three scoped targets passed.
No fixture values or oracle bytes changed. This measures present coverage, not
the historical pre-sweep state of every target.

The object-normalization wrapper lacked a direct witness: the recursive wrapper
calls types directly rather than calling num's object wrapper. The gate now
invokes both public wrappers against the same 300 original source rows. Clearing
or removing one member after each delegation compiled and failed, independently,
`OWN-KEYS normalized object` and `OWN-KEYS normalized JSON`. All four controls
were restored and the scoped target passed. These are new direct-wrapper checks;
the absence of an earlier direct check is not claimed as a measured false green.

## Retained input and opaque passthrough supplements

New two-element arrays and two-key maps exercise `from_retained`, opaque
`JsNumberInput::serialize`, and `js_deserialize_passthrough` separately. Typed
Vec/map outputs use direct native counts and values. Opaque serialization compares
the complete JSON graph; passthrough checks two members and two elements each
across six direct/tagged raw/owned/borrowed routes, then 24 executed number checks.
These are new structural supplements, not regenerated source-corpus evidence.

Twelve independent production empty/subset controls compiled and exited 101 at
`FIELD IDENTITY captured array cardinality count`, `CAPTURE-CARDINALITY native
map`, `CAPTURE-CARDINALITY opaque serialization`, or `PASSTHROUGH-CARDINALITY
native members` / `native array nested`, according to the operation mutated.
All mutations were restored; both scoped supplements passed. An initial test
mistake compared integer-token JSON expectations with float-token serialization;
the corrected baseline passed before all twelve controls were rerun. Results
from that failing initial baseline are not used as mutation evidence.

The typed `Object::next_key_seed` iterator was independently mutated to consume
all remaining entries or discard the last entry before yielding keys. Both
compiled and failed `CAPTURE-CARDINALITY native map` on a direct map output,
without relying on missing required struct fields. Production was restored and
the scoped retained-input target passed.

## Scope boundaries

GRVT opaque payload and protocol-table controls are recorded in the venue audit.
The threshold/missing-source-count inventory is addressed by the supplement below.
Already-exact primary gates are not all retrofitted with executed-row counters.
The types owner published finite direct array/object into_value proof at
4635e74e65; that is not nested shared-model info or all-numeric-graph coverage,
and publication alone does not establish integration landing.

## Source-count supplement

The original oracle files remain byte-identical. This supplement replaces the
threshold/missing-count checks in thirteen files and adds independently counted
supplementary captures. It counts completed assertion callbacks, not fetched
rows; early-return branches in numeric utilities retain their assertions before
returning to the counter. It does not retrofit every already-exact corpus gate.

| Capture | Exact rows |
| --- | ---: |
| truthiness and Number(any), separately | 1,680 each |
| log10 | 333,938 |
| Date.parse / TimeClip, each timezone | 30,161 / 10,013 |
| root collation | 165,422 |
| JSON.stringify | 1,914 |
| canonical property entries | 5,135 |
| table hits / surrogate witnesses / numeric witnesses | 40,850 / 2 / 5 |
| get-key / unknown-RHS refusals | 836 / 2 |
| BigInt | 10,156 |
| pow | 228,973 |
| numeric utilities / selection | 10,565 / 20,022 |
| coercion number / float / int / string / measured | 10,018 / 10,018 / 30,264 / 18 / 25 |
| streamed lodash case | 1,148,148 |
| trim sets / members per set | 4 / 25 |

`source_cardinality_controls.py` builds only its named targets, then executes
one Node process per corpus invocation (Date has one per timezone). Empty and
one-row-omission capture controls fail `SOURCE-CARDINALITY <name> source total`.
The independent after-count control clears rows only after that check and fails
`SOURCE-CARDINALITY <name> compared total`. Streamed lodash drains its child and
counts capture and comparisons separately; it does not buffer a million rows.
Date baseline assertions cover both timezones; negative controls fail first in
UTC and do not claim independent second-timezone failure evidence.

These are test-input/control-path mutations, not claims that production scalar
operations return collections or that shipped decoders discard input. Exact
counts do not establish content identity: the existing value/bit assertions
remain required. The independent all-scalar whitespace walk executes 4,448,256
native comparisons in addition to the source-set/member checks.

Node 26 ran 25 baseline/restored targets and 96 named controls over 32
collections (69 num, 12 GRVT, 15 trim controls); Node 24 ran the same 25
baselines. The singleton GRVT trigger-boundary omission is necessarily empty,
not a nonempty proper-subset witness. Driver target/name totals are pinned
independently of its registry, so deleting a registered control does not lower
the expected count silently.
