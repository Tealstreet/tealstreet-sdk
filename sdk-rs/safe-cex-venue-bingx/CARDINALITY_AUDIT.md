# BingX comparison cardinality audit

Baseline: dc98b530da458c72f28b7fb957b9a0481473cdba. All five integration-test
files were inspected. No expected/native zip exists. Native nested values use
whole-value equality, including Vec observations, so unequal array lengths do
not silently truncate. Scalars in ticker, sort and errors have fixed source
case-count assertions and direct native comparisons.

A distinct admission FALSE GREEN was measured in the types target: replacing
the oracle case list with [] passed (0 comparisons); removing every
FETCH_BALANCE_RESPONSE case also passed (535 typed plus 195 UTF-8 cases).
These were source-corpus mutations, not production-output mutations. Both were
restored. The original target had declaration-name checks but no case census.

Production baseline controls independently emptied and truncated each of
FetchBalanceResponse.data and FetchPositionsResponse.data. All four compiled
and failed the original native_fields assertion, then restored/passed. Thus
these production outputs were already covered; no production empty-output
false green was found for either field. Representative constant ORDER_SIDE
empty/subset controls also compile and fail ordered constant ORDER_SIDE.
Other constant arrays use the same whole-value equality; they were inspected,
not individually mutated. Scalar targets cannot return empty collections.

Repair retains original oracle bytes and values. Types now assert 883 total
cases, per-type case/boundary counts, 220 source output rows, 2 expected rows per
valid response-array case, 5 executed routes per non-boundary case, and 3659
executed routes total (631*5 + 252*2). Separately, recursive native-array length
equality runs before full native-value equality. It compares the complete
native Vec observation to expected length; no zip or common-length slicing.
Constants now assert 15 inherited-property probes for each of six tables.

Controls cover both native collections (empty/subset), source empty/subset case
lists, removing a type while faking the original total, empty source outputs,
and moving one output row between cases while preserving the original aggregate.
The first layer checks source totals and per-case counts. Native collection
mutations leave those counts valid but fail BINGX-CARDINALITY native array length,
independently demonstrating the second layer. Constant probe empty/subset
controls fail BINGX-CARDINALITY inherited probes. Each control requires exit101,
a named assertion and test-failure output, then restoration/pass. A first constant
mutation builder accidentally spanned adjacent declarations and failed compilation;
it was rejected as evidence and repaired before the four successful controls.

Original actual-source capture: 883 cases, 2,056,625 bytes, SHA256
22e841718fa6e4fbbe2a21781c14d4c92e0cb543b0d8df6d2ad629e4c20c7d3b.
It is preserved losslessly in tests/fixtures/types-cardinality-original.json.gz
as historical evidence, not consumed as expected values by the live target.
Original and current types_oracle.cjs SHA256:
4fc5a54caa06a14aaf80260c09fae3ee5a415a61656b3fa1ad7022e301193f68.
No fixture-value repairs; no production or manifest changes. Double-parse remains
UNMEASURED. One Node invocation per corpus; scoped Cargo targets run serially.


## Six-shape extension

Integration ba6468e0076f360c9331607dd433896f442e5e6b is merged at
53b9b20f93713253d6107e708fb1b3e8bb1248b6. Its BingX formatting repair is
included. Any remaining fleet extractor failure is a HARD required-CI blocker,
not a cosmetic caveat; this scoped audit does not assert fleet extractor green.

Census: five test files / six test functions, two native variable-length response
fields, fixed constant tables, and scalar results from sort, two error helpers,
and ticker bid/ask. No truncating zip exists. Whole-value assertions inspect all
keys/elements; source-driven loop bounds still require independently pinned counts.
The previous admission false green is not reclassified as a native-output defect.

The new six_shape_controls.py ran 21 controls, each compiled, exited101 at its
named assertion, and restored/passed:

- Six production response mutations: balance.data and positions.data each EMPTY,
  SUBSET and EXTRA, with source total, per-type, aggregate-row and per-case-row
  guards simultaneously bypassed. All fail BINGX-CARDINALITY native array length.
- Empty entire type corpus with those source guards bypassed fails
  BINGX-CARDINALITY executed route comparisons.
- Replacing every admitted BINGX_BALANCE expected object with {} fails
  native_fields BINGX_BALANCE. Whole-object equality prevents a rich expected
  object elsewhere from hiding an empty one here.
- Three representative ORDER_SIDE native mutations (0, 1, 3 rows) with export
  census bypassed fail ordered constant ORDER_SIDE. Other fixed tables remain
  inspected, not individually mutated by this extension.
- Two inherited-probe mutations (empty and subset), with source probe count
  bypassed, fail BINGX-CARDINALITY inherited comparisons.
- Eight scalar-subcorpus mutations: empty and subset for sort, ticker, error
  builder and error normalizer, each with its source count bypassed. They fail
  their respective independent executed-comparison assertions.

Independent executed counts now include sort784, bid/ask336, error builder70,
error normalizer65, and inherited probes15 per table, in addition to types3659.
Production EMPTY/SUBSET/EXTRA collection mutations apply to collection outputs;
scalar return types cannot produce a variable number of results. No order handler
is implemented, so no order-handler coverage is claimed. No new fixture values or
oracle code changed. Source and native cardinality expectations remain separate.
Original type-corpus capture/hash above remains byte-identical. Manifest triple
remains14/10/75. Double-parse remains UNMEASURED.

## Consuming versus non-consuming collection controls

Census at f37fc19cc09c1cf16d92d4d53ca11d8bcb36add2: two serde-decoded
collection fields, FetchBalanceResponse.data and FetchPositionsResponse.data.
Each occurs once in a public record and once in its private serde companion;
those four declarations are two decode sites, not four distinct consumers.

The permanent controls in cardinality_controls.py and six_shape_controls.py
insert a Result::map AFTER successful decode_with. They deserialize and consume
all input elements first, then clear/truncate the resulting vector and return
success. These are consuming venue-deserializer mutants. They do not return
None early from SeqAccess and cannot rely on serde's trailing-element rejection.
The shared Array::next_element_seed implementation itself was not mutated by
this venue audit; the proof is scoped to these two venue output boundaries.

Measured proof already run on f37fc19cc0: for EACH field, consuming EMPTY and
SUBSET plus EXTRA succeeded in compilation and failed exit101 at
BINGX-CARDINALITY native array length with all source count guards bypassed.
All six restored/passed. This is a named native-length failure, not a serde
parse/trailing-element error. No additional Cargo run is claimed for this
wording-only clarification. Future book/fill/order/candle/ticker-batch decoders
remain outside this census until they are implemented and separately mutated.


## Extractor repair and guard-removal controls

Fetched integration a09ea6489b2012e43dfca621621ae8e916d4d03c and merged at
8179705c1b150ce210fc998f3cff4837e604b613 on the follow-on branch. The prior
inherited extractor caveat is RETIRED. The full actual extractor check measured
7341 symbols across512 files; Would update0 manifests,0 added,0 orphaned, exit0.
This run uses locally provisioned smol-toml1.4.2; it is not a clean-install proof.
The separately tracked dependency-install issue is not a symbol-census failure.

The consuming-empty control was additionally run for BOTH collection fields with
all source-count guards AND the independent native-array-length guard bypassed,
while keeping the injected production mutant active. Both compiled and failed
exit101 at native_fields FETCH_BALANCE_RESPONSE / native_fields
FETCH_POSITIONS_RESPONSE, then restored/passed. The full native-value equality
is a third independent defense. Removing lengths therefore does not make these
BingX targets green; this result does not claim the length guard is the sole
load-bearing assertion. No source or fixture changes were needed. The permanent
runner now contains23 controls; the final two run via --group guard-removal.
These are injected mutation controls, not evidence of a live deserializer bug.
