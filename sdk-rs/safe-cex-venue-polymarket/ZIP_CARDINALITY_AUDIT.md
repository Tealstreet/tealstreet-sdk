# Cardinality audit

Baseline: `7206f1785429c3802f726daaa35f7bd983289539` (local response slice).
The baseline evidence lives under
`target/polymarket-cardinality/7206f1785429c3802f726daaa35f7bd983289539`.

The two native pairwise walks are recursive constants arrays and book levels.
Both already assert native length independently before zip. Constants maps
compare complete own-key lists; response hashes compare complete Option<Vec>
values. Ten compiling production mutations (empty/subset bids, asks, hashes,
EIP712_ORDER_TYPES and ENDPOINTS) failed named runtime assertions at 101;
all ten restorations passed. No production cardinality-blind survivor was found
at those sites. This is a field/site census, not a claim about pending formatters.

Four source-corpus omissions did survive the original targets at exit 0:
empty/subset auth numeric cases and subset source bids/asks. These are source
coverage failures, distinct from production zip truncation. Each restored at 0.
The repair pins source counts and independently counts comparisons actually
performed: 9 auth numeric cases / 72 native field comparisons; book source sides
2 levels each (numeric cases 1 each, absent case 0) / 244 native level comparisons;
response source hashes 2 per populated case / 86 native hash comparisons;
13 constant exports / 118 native leaf comparisons. Each constant has a fixed
source leaf count; constant numeric observations have a fixed source total of 15.
Accepted string-case counts are pinned separately from total case counts.
Existing native length/identity checks remain independent of source totals.

`tests/cardinality_controls.py` repeats all 14 controls and adds three deliberate
source-guard bypasses: empty native bids still fail native length; empty numeric
source still fails the performed-comparison count; shortened source bids still
fail the performed-level count. Every control requires 101 plus its named
assertion and then a restored 0. Cargo runs serially with --jobs 1; each auth
control invokes Node once for the entire corpus, never once per case.

The repair changes no oracle or fixture values. SHA-256 at baseline and repair:

- auth_types_oracle.cjs: `240cd2dda05028e7e3bc264181d7330d21a0f04d14fa68934cfeecd7fd7d8a97`
- constants_oracle.cjs: `7ae417c9a2200abbbca41c8e9d7a8c22c37816e451d0446659ba960b0a45ef98`

Final runs must be associated with their actual committed SHA in the evidence
folder; the baseline survivor logs do not describe the repaired head.

## Consuming decoder controls

The implementation census is three decoded collection fields: book bids, book
asks and response transactionsHashes. No fill/order/candle/ticker batch decoder
is implemented. The earlier production controls truncate fully decoded vectors;
they consume input but do not themselves prove the shared SeqAccess seam.
`tests/consuming_controls.py` temporarily injects a fault into shared
`Array::next_element_seed`: drain the iterator before returning None, or retain
one element and drain the rest before yielding it. Side-selective predicates use
the existing actual-source sample's distinct prices, allowing independent asks
coverage. No shared implementation change is retained. These are injected
mutations, not claims of a live shared decoder defect.

A second consuming-empty bids control removes both source-side length guards
and the native/source length equality while retaining the consuming mutant.
An independently accumulated per-case comparison count must still reject it at
`native_book.level_comparison_count`. Response hashes also have an explicit
native length check before whole-vector identity comparison. The count guard
expects source cardinality, never the decoded native artifact's own length.
The same oracle hashes above remain unchanged.
